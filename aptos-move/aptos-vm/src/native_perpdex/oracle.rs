// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::oracle

use crate::native_perpdex::internal_oracle_state::InternalSourceIdentifier;
use crate::native_perpdex::math::{self, Precision};
use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const ENOT_INTERNAL_ORACLE: u64 = 1;
const EINVALID_ORACLE_TYPE: u64 = 2;

pub const ORACLE_TYPE_SINGLE_INTERNAL: u8 = 0;
pub const ORACLE_TYPE_SINGLE_PYTH: u8 = 1;
pub const ORACLE_TYPE_SINGLE_CHAINLINK: u8 = 2;
pub const ORACLE_TYPE_COMPOSITE_PYTH_INTERNAL: u8 = 3;
pub const ORACLE_TYPE_COMPOSITE_CHAINLINK_INTERNAL: u8 = 4;

// ===================== Types =====================

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PythSource {
    V1 {
        /// Pyth price identifier (32 bytes)
        price_identifier: [u8; 32],
        /// Max age in seconds before considered stale
        max_staleness_secs: u64,
        /// Max confidence interval threshold
        confidence_interval_threshold: u64,
        /// Rescale decimals
        rescale_decimals: i8,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChainlinkSource {
    V1 {
        /// Chainlink feed ID
        feed_id: Vec<u8>,
        /// Max age in seconds before considered stale
        max_staleness_secs: u64,
        /// Rescale decimals
        rescale_decimals: i8,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum InternalSource {
    V1 {
        source_id: InternalSourceIdentifier,
        /// Max age in seconds before considered stale
        max_staleness_secs: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SingleOracleSource {
    Internal(InternalSource),
    Pyth(PythSource),
    Chainlink(ChainlinkSource),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum OracleSource {
    Single {
        primary: SingleOracleSource,
    },
    Composite {
        primary: SingleOracleSource,
        secondary: SingleOracleSource,
        /// Max deviation in basis points between oracles before reduce-only mode
        oracles_deviation_bps: u64,
        /// Count of consecutive deviations before considering the oracle invalid
        consecutive_deviation_count: u8,
        /// Last primary price used to calculate deviation
        last_primary_price: u64,
        /// Current consecutive deviation counter
        current_deviation_count: u8,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum OracleStatus {
    Ok,
    Invalid, // There are discrepancies but we can operate.
    Down,    // Can't operate.
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum OracleData {
    V1 { price: u64, status: OracleStatus },
}

/// Input struct for providing oracle prices from the dispatch layer.
/// Since native Rust cannot call Pyth/Chainlink APIs directly, the caller
/// provides the resolved prices and health status.
pub struct OraclePriceInput {
    /// The resolved price for the primary oracle source
    pub primary_price: u64,
    /// Whether the primary oracle is healthy (not stale, confidence ok)
    pub primary_healthy: bool,
    /// The resolved price for the secondary oracle source (if composite)
    pub secondary_price: u64,
    /// Whether the secondary oracle is healthy
    pub secondary_healthy: bool,
}

// ===================== Functions =====================

pub fn new_internal_source(
    source_id: InternalSourceIdentifier,
    max_staleness_secs: u64,
) -> SingleOracleSource {
    SingleOracleSource::Internal(InternalSource::V1 {
        source_id,
        max_staleness_secs,
    })
}

pub fn new_pyth_source(
    identifier_bytes: [u8; 32],
    max_staleness_secs: u64,
    confidence_interval_threshold: u64,
    rescale_decimals: i8,
) -> SingleOracleSource {
    SingleOracleSource::Pyth(PythSource::V1 {
        price_identifier: identifier_bytes,
        max_staleness_secs,
        confidence_interval_threshold,
        rescale_decimals,
    })
}

pub fn new_chainlink_source(
    feed_id: Vec<u8>,
    max_staleness_secs: u64,
    rescale_decimals: i8,
) -> SingleOracleSource {
    SingleOracleSource::Chainlink(ChainlinkSource::V1 {
        feed_id,
        max_staleness_secs,
        rescale_decimals,
    })
}

pub fn new_single_oracle(oracle_source: SingleOracleSource) -> OracleSource {
    OracleSource::Single {
        primary: oracle_source,
    }
}

pub fn new_composite_oracle(
    primary: SingleOracleSource,
    secondary: SingleOracleSource,
    oracles_deviation_bps: u64,
    consecutive_deviation_count: u8,
) -> Result<OracleSource, u64> {
    let oracle_source = OracleSource::Composite {
        primary,
        secondary,
        oracles_deviation_bps,
        consecutive_deviation_count,
        last_primary_price: 0,
        current_deviation_count: 0,
    };

    let oracle_type = get_oracle_type(&oracle_source)?;

    if oracle_type != ORACLE_TYPE_COMPOSITE_PYTH_INTERNAL
        && oracle_type != ORACLE_TYPE_COMPOSITE_CHAINLINK_INTERNAL
    {
        return Err(EINVALID_ORACLE_TYPE);
    }

    Ok(oracle_source)
}

// ===================== OracleData accessors =====================

pub fn get_price(data: &OracleData) -> u64 {
    let OracleData::V1 { price, .. } = data;
    *price
}

pub fn is_status_ok(data: &OracleData) -> bool {
    let OracleData::V1 { status, .. } = data;
    matches!(status, OracleStatus::Ok)
}

pub fn is_status_invalid(data: &OracleData) -> bool {
    let OracleData::V1 { status, .. } = data;
    matches!(status, OracleStatus::Invalid)
}

pub fn is_status_down(data: &OracleData) -> bool {
    let OracleData::V1 { status, .. } = data;
    matches!(status, OracleStatus::Down)
}

// ===================== Oracle data retrieval =====================

/// Get oracle data using pre-resolved prices and health status from the dispatch layer.
///
/// In Move, this function calls into Pyth/Chainlink APIs. In native Rust,
/// the caller resolves prices externally and passes them via OraclePriceInput.
pub fn get_oracle_data(
    oracle_source: &OracleSource,
    input: &OraclePriceInput,
) -> OracleData {
    match oracle_source {
        OracleSource::Single { .. } => {
            if input.primary_healthy {
                OracleData::V1 {
                    price: input.primary_price,
                    status: OracleStatus::Ok,
                }
            } else {
                OracleData::V1 {
                    price: input.primary_price,
                    status: OracleStatus::Down,
                }
            }
        }
        OracleSource::Composite {
            consecutive_deviation_count,
            last_primary_price,
            current_deviation_count,
            ..
        } => {
            let primary_healthy = input.primary_healthy;
            let secondary_healthy = input.secondary_healthy;

            if primary_healthy && secondary_healthy {
                // Both oracles healthy - check for cross-oracle deviation
                if *current_deviation_count >= *consecutive_deviation_count {
                    return OracleData::V1 {
                        price: input.primary_price,
                        status: OracleStatus::Invalid,
                    };
                }
                OracleData::V1 {
                    price: input.primary_price,
                    status: OracleStatus::Ok,
                }
            } else if !primary_healthy && secondary_healthy {
                OracleData::V1 {
                    price: input.secondary_price,
                    status: OracleStatus::Ok,
                }
            } else if primary_healthy && !secondary_healthy {
                OracleData::V1 {
                    price: input.primary_price,
                    status: OracleStatus::Ok,
                }
            } else {
                // Both oracles unhealthy
                OracleData::V1 {
                    price: *last_primary_price,
                    status: OracleStatus::Down,
                }
            }
        }
    }
}

/// Update oracle status (deviation tracking for composite oracles).
///
/// In Move, this reads prices from Pyth/Chainlink. In native Rust, the caller
/// provides pre-resolved prices.
pub fn update_oracle_status(
    oracle_source: &mut OracleSource,
    input: &OraclePriceInput,
) {
    match oracle_source {
        OracleSource::Single { .. } => {
            // No-op for single oracle
        }
        OracleSource::Composite {
            oracles_deviation_bps,
            consecutive_deviation_count,
            last_primary_price,
            current_deviation_count,
            ..
        } => {
            let primary_healthy = input.primary_healthy;
            let secondary_healthy = input.secondary_healthy;

            if primary_healthy && secondary_healthy {
                *last_primary_price = input.primary_price;
                check_and_handle_deviation(
                    input.primary_price,
                    input.secondary_price,
                    *oracles_deviation_bps,
                    current_deviation_count,
                    *consecutive_deviation_count,
                );
            } else if !primary_healthy && secondary_healthy {
                *current_deviation_count = 0;
            } else if primary_healthy && !secondary_healthy {
                *current_deviation_count = 0;
                *last_primary_price = input.primary_price;
            } else {
                // Both unhealthy
                *current_deviation_count = 0;
            }
        }
    }
}

/// Update internal oracle price within an OracleSource.
pub fn update_internal_oracle_price(
    oracle_source: &OracleSource,
) -> Result<InternalSourceIdentifier, u64> {
    match oracle_source {
        OracleSource::Single {
            primary: SingleOracleSource::Internal(internal),
        } => {
            let InternalSource::V1 { source_id, .. } = internal;
            Ok(*source_id)
        }
        OracleSource::Composite {
            secondary: SingleOracleSource::Internal(internal),
            ..
        } => {
            let InternalSource::V1 { source_id, .. } = internal;
            Ok(*source_id)
        }
        _ => Err(ENOT_INTERNAL_ORACLE),
    }
}

// ===================== Pyth price conversion =====================

/// Convert a Pyth price to u64 with the target precision.
///
/// In Move this calls pyth::get_price_unsafe. In native Rust, the caller
/// provides the raw pyth_price and pyth_expo.
///
/// pyth_expo_magnitude: the absolute value of the exponent
/// pyth_expo_negative: whether the exponent is negative
pub fn convert_pyth_price_to_u64(
    mut pyth_price: u64,
    pyth_expo_magnitude: u64,
    pyth_expo_negative: bool,
    rescale_decimals: i8,
    target_precision: &Precision,
) -> Result<u64, u64> {
    let target_decimals = math::get_decimals(target_precision) as i16;
    let expo_signed: i16 = if pyth_expo_negative {
        -(pyth_expo_magnitude as i16)
    } else {
        pyth_expo_magnitude as i16
    };
    let aggregate_decimals = target_decimals + (rescale_decimals as i16) + expo_signed;

    if aggregate_decimals < 0 {
        let p = math::new_precision((-aggregate_decimals) as u8)?;
        pyth_price /= math::get_decimals_multiplier(&p);
    } else if aggregate_decimals > 0 {
        let p = math::new_precision(aggregate_decimals as u8)?;
        pyth_price *= math::get_decimals_multiplier(&p);
    }

    Ok(pyth_price)
}

// ===================== Oracle type identification =====================

pub fn get_oracle_type(oracle_source: &OracleSource) -> Result<u8, u64> {
    match oracle_source {
        OracleSource::Single {
            primary: SingleOracleSource::Internal(_),
        } => Ok(ORACLE_TYPE_SINGLE_INTERNAL),
        OracleSource::Single {
            primary: SingleOracleSource::Pyth(_),
        } => Ok(ORACLE_TYPE_SINGLE_PYTH),
        OracleSource::Single {
            primary: SingleOracleSource::Chainlink(_),
        } => Ok(ORACLE_TYPE_SINGLE_CHAINLINK),
        OracleSource::Composite {
            primary: SingleOracleSource::Pyth(_),
            secondary: SingleOracleSource::Internal(_),
            ..
        } => Ok(ORACLE_TYPE_COMPOSITE_PYTH_INTERNAL),
        OracleSource::Composite {
            primary: SingleOracleSource::Chainlink(_),
            secondary: SingleOracleSource::Internal(_),
            ..
        } => Ok(ORACLE_TYPE_COMPOSITE_CHAINLINK_INTERNAL),
        _ => Err(EINVALID_ORACLE_TYPE),
    }
}

/// Get primary oracle price (unsafe - no health check).
/// The caller provides the pre-resolved primary price.
pub fn get_primary_oracle_price_unsafe(primary_price: u64) -> u64 {
    primary_price
}

/// Get secondary oracle price (unsafe - no health check).
/// Only valid for composite oracles.
pub fn get_secondary_oracle_price_unsafe(
    oracle_source: &OracleSource,
    secondary_price: u64,
) -> Result<u64, u64> {
    match oracle_source {
        OracleSource::Composite { .. } => Ok(secondary_price),
        _ => Err(EINVALID_ORACLE_TYPE),
    }
}

pub fn is_composite(oracle_source: &OracleSource) -> bool {
    matches!(oracle_source, OracleSource::Composite { .. })
}

#[cfg(test)]
pub fn get_current_deviation_count(oracle_source: &OracleSource) -> u8 {
    match oracle_source {
        OracleSource::Composite {
            current_deviation_count,
            ..
        } => *current_deviation_count,
        _ => 0,
    }
}

// ===================== Internal helpers =====================

fn abs_diff(x: u64, y: u64) -> u64 {
    if x > y {
        x - y
    } else {
        y - x
    }
}

fn calculate_deviation_bps(price1: u64, price2: u64) -> u64 {
    // If either price is 0, return max deviation to force oracle invalidation.
    if price1 == 0 || price2 == 0 {
        return u64::MAX;
    }
    let diff = abs_diff(price1, price2);
    // Calculate deviation in basis points: (abs_diff / price1) * 10000
    // Use mul_div to prevent overflow
    // aptos_std::math64::mul_div(abs_diff, 10000, price1)
    ((diff as u128) * 10000 / (price1 as u128)) as u64
}

fn check_and_handle_deviation(
    current_price: u64,
    reference_price: u64,
    deviation_threshold_bps: u64,
    current_deviation_count: &mut u8,
    consecutive_deviation_count: u8,
) {
    let deviation_bps = calculate_deviation_bps(current_price, reference_price);

    if deviation_bps > deviation_threshold_bps {
        if *current_deviation_count >= consecutive_deviation_count {
            return;
        }
        *current_deviation_count += 1;
    } else {
        *current_deviation_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_deviation_bps_with_zero_price() {
        assert_eq!(calculate_deviation_bps(0, 50000), u64::MAX);
        assert_eq!(calculate_deviation_bps(50000, 0), u64::MAX);
        assert_eq!(calculate_deviation_bps(0, 0), u64::MAX);
        // 5% deviation: (52500 - 50000) / 50000 * 10000 = 500 bps
        assert_eq!(calculate_deviation_bps(50000, 52500), 500);
        assert_eq!(calculate_deviation_bps(50000, 50000), 0);
    }

    #[test]
    fn test_convert_pyth_price_to_u64_basic() {
        let precision = math::new_precision(8).unwrap();
        // pyth_price=6041000000000, expo=-18, rescale=0, target=8 decimals
        // aggregate_decimals = 8 + 0 + (-18) = -10
        // 6041000000000 / 10^10 = 604
        let result =
            convert_pyth_price_to_u64(6041000000000, 18, true, 0, &precision).unwrap();
        assert_eq!(result, 604);
    }

    #[test]
    fn test_convert_pyth_price_to_u64_with_rescale() {
        let precision = math::new_precision(8).unwrap();
        // pyth_price=6041000000000, expo=-18, rescale=3, target=8 decimals
        // aggregate_decimals = 8 + 3 + (-18) = -7
        // 6041000000000 / 10^7 = 604100
        let result =
            convert_pyth_price_to_u64(6041000000000, 18, true, 3, &precision).unwrap();
        assert_eq!(result, 604100);
    }
}
