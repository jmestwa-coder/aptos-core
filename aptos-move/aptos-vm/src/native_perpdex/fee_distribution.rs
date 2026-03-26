// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::fee_distribution

use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const EINVALID_AMOUNT: u64 = 1;
const EINVALID_ADDRESS: u64 = 2;
const EINVALID_FEE_DISTRIBUTION: u64 = 3;
const ESYSTEM_FEE_DRAIN: u64 = 4;

// ===================== Types =====================

/// CollateralBalanceType is defined externally; we use a lightweight stand-in.
/// In the real integration, this type comes from collateral_balance_sheet.
pub type CollateralBalanceType = [u8; 32]; // Placeholder for the actual enum

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FeeWithDestination {
    V1 {
        address: [u8; 32],
        fees: u64,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FeeDistribution {
    RegularTrade_V1 {
        balance_type: CollateralBalanceType,
        position_fee_delta: i64,
        treasury_fee_delta: i64,
        builder_or_referrer_fees: Option<FeeWithDestination>,
    },
    MarginCall_V1 {
        balance_type: CollateralBalanceType,
        position_fee_delta: i64,
    },
}

// ===================== Functions =====================

pub fn new_fee_distribution(
    balance_type: CollateralBalanceType,
    position_fee_delta: i64,
    builder_or_referrer_fees: Option<FeeWithDestination>,
) -> FeeDistribution {
    let misc_fees = match &builder_or_referrer_fees {
        Some(FeeWithDestination::V1 { fees, .. }) => *fees,
        None => 0,
    };
    let treasury_fee_delta = (misc_fees as i64) - position_fee_delta;
    FeeDistribution::RegularTrade_V1 {
        balance_type,
        position_fee_delta,
        treasury_fee_delta,
        builder_or_referrer_fees,
    }
}

pub fn new_margin_call_fee_distribution(
    balance_type: CollateralBalanceType,
    position_fee_delta: u64,
) -> FeeDistribution {
    FeeDistribution::MarginCall_V1 {
        balance_type,
        position_fee_delta: position_fee_delta as i64,
    }
}

pub fn new_fee_with_destination(address: [u8; 32], fees: u64) -> FeeWithDestination {
    FeeWithDestination::V1 { address, fees }
}

pub fn zero_fees(balance_type: CollateralBalanceType) -> FeeDistribution {
    FeeDistribution::RegularTrade_V1 {
        balance_type,
        position_fee_delta: 0,
        treasury_fee_delta: 0,
        builder_or_referrer_fees: None,
    }
}

pub fn is_margin_call_fee_distribution(fee_dist: &FeeDistribution) -> bool {
    matches!(fee_dist, FeeDistribution::MarginCall_V1 { .. })
}

pub fn add(
    a: &FeeDistribution,
    b: &FeeDistribution,
) -> Result<FeeDistribution, u64> {
    match (a, b) {
        (
            FeeDistribution::RegularTrade_V1 {
                balance_type,
                position_fee_delta,
                treasury_fee_delta,
                builder_or_referrer_fees,
            },
            FeeDistribution::RegularTrade_V1 {
                balance_type: other_bt,
                position_fee_delta: other_pfd,
                treasury_fee_delta: other_tfd,
                builder_or_referrer_fees: other_brf,
            },
        ) => {
            if balance_type != other_bt {
                return Err(EINVALID_FEE_DISTRIBUTION);
            }
            if builder_or_referrer_fees.is_none() != other_brf.is_none() {
                return Err(EINVALID_FEE_DISTRIBUTION);
            }
            if let (Some(FeeWithDestination::V1 { address: a_addr, .. }), Some(FeeWithDestination::V1 { address: b_addr, .. })) = (builder_or_referrer_fees, other_brf) {
                if a_addr != b_addr {
                    return Err(EINVALID_ADDRESS);
                }
            }
            let new_brf = match (builder_or_referrer_fees, other_brf) {
                (Some(FeeWithDestination::V1 { address, fees }), Some(FeeWithDestination::V1 { fees: other_fees, .. })) => {
                    Some(FeeWithDestination::V1 { address: *address, fees: fees + other_fees })
                }
                _ => None,
            };
            Ok(FeeDistribution::RegularTrade_V1 {
                balance_type: *balance_type,
                position_fee_delta: position_fee_delta + other_pfd,
                treasury_fee_delta: treasury_fee_delta + other_tfd,
                builder_or_referrer_fees: new_brf,
            })
        }
        _ => Err(EINVALID_FEE_DISTRIBUTION),
    }
}

pub fn get_position_fee_delta(fee_dist: &FeeDistribution) -> i64 {
    match fee_dist {
        FeeDistribution::RegularTrade_V1 { position_fee_delta, .. } => *position_fee_delta,
        FeeDistribution::MarginCall_V1 { position_fee_delta, .. } => *position_fee_delta,
    }
}

pub fn get_system_fee_delta(fee_dist: &FeeDistribution) -> i64 {
    match fee_dist {
        FeeDistribution::RegularTrade_V1 { treasury_fee_delta, .. } => *treasury_fee_delta,
        FeeDistribution::MarginCall_V1 { .. } => 0, // MarginCall has no treasury_fee_delta
    }
}

pub fn get_builder_or_referrer_fees(fee_dist: &FeeDistribution) -> Option<FeeWithDestination> {
    match fee_dist {
        FeeDistribution::RegularTrade_V1 { builder_or_referrer_fees, .. } => *builder_or_referrer_fees,
        FeeDistribution::MarginCall_V1 { .. } => None,
    }
}

pub fn get_balance_type(fee_dist: &FeeDistribution) -> CollateralBalanceType {
    match fee_dist {
        FeeDistribution::RegularTrade_V1 { balance_type, .. } => *balance_type,
        FeeDistribution::MarginCall_V1 { balance_type, .. } => *balance_type,
    }
}

/// Distribute fees for a single position.
/// In native context, actual collateral mutations are handled externally.
/// Returns the distribution instructions rather than mutating state directly.
/// // RESOURCE: CollateralBalanceSheet mutated
pub fn distribute_fees_for_position(
    fee_dist: &FeeDistribution,
) -> Result<(), u64> {
    match fee_dist {
        FeeDistribution::RegularTrade_V1 {
            position_fee_delta,
            treasury_fee_delta,
            ..
        } => {
            if *position_fee_delta == 0 {
                return Ok(());
            }
            if *position_fee_delta > 0 {
                // User pays fees - withdraw from user, distribute misc, rest to treasury
                Ok(())
            } else {
                // Rebate - treasury_fee_delta must be >= 0
                if *treasury_fee_delta < 0 {
                    return Err(EINVALID_AMOUNT);
                }
                Ok(())
            }
        }
        _ => Err(EINVALID_FEE_DISTRIBUTION),
    }
}

/// Distribute fees - main entry point.
/// Routes to margin call or standard distribution.
/// In native context, the actual collateral operations are performed by the caller
/// using the fee distribution data. This function validates the distributions.
pub fn distribute_fees(
    taker_fee: &FeeDistribution,
    maker_fee: &FeeDistribution,
    _backstop_vault_addr: [u8; 32],
    standard_vault_pct: u64,
    _market: [u8; 32], // Object<PerpMarket> address
) -> Result<DistributionResult, u64> {
    if is_margin_call_fee_distribution(taker_fee) || is_margin_call_fee_distribution(maker_fee) {
        distribute_fees_for_margin_call(taker_fee, maker_fee, standard_vault_pct)
    } else {
        distribute_fees_for_standard(taker_fee, maker_fee, standard_vault_pct)
    }
}

/// Result of fee distribution computation
#[derive(Clone, Debug)]
pub struct DistributionResult {
    pub backstop_final: u64,
}

fn distribute_fees_for_standard(
    taker_fee: &FeeDistribution,
    maker_fee: &FeeDistribution,
    standard_vault_pct: u64,
) -> Result<DistributionResult, u64> {
    let taker_tfd = get_system_fee_delta(taker_fee);
    let maker_tfd = get_system_fee_delta(maker_fee);
    // Verify no system drain
    if maker_tfd + taker_tfd > 0 {
        return Err(ESYSTEM_FEE_DRAIN);
    }
    // treasury_fee_delta is negative when treasury gains
    let treasury_gain = -(maker_tfd + taker_tfd);
    let vault_fee = treasury_gain * (standard_vault_pct as i64) / 100;
    let backstop_final = if vault_fee > 0 { vault_fee as u64 } else { 0 };
    Ok(DistributionResult { backstop_final })
}

fn distribute_fees_for_margin_call(
    taker_fee: &FeeDistribution,
    maker_fee: &FeeDistribution,
    standard_vault_pct: u64,
) -> Result<DistributionResult, u64> {
    // In the margin call path, we compute backstop shares per side
    let (taker_gain, taker_bs) = distribute_side_to_treasury(taker_fee, 0, standard_vault_pct)?;
    let (maker_gain, maker_bs) = distribute_side_to_treasury(maker_fee, 0, standard_vault_pct)?;

    let total_backstop = taker_bs + maker_bs;
    let treasury_gain = taker_gain + maker_gain;

    let backstop_final = if treasury_gain <= 0 {
        0
    } else if (treasury_gain as u64) < total_backstop {
        treasury_gain as u64
    } else {
        total_backstop
    };

    Ok(DistributionResult { backstop_final })
}

fn distribute_side_to_treasury(
    fee: &FeeDistribution,
    margin_call_vault_pct: u64,
    standard_vault_pct: u64,
) -> Result<(i64, u64), u64> {
    if is_margin_call_fee_distribution(fee) {
        let pfd = get_position_fee_delta(fee);
        if pfd < 0 {
            return Err(EINVALID_AMOUNT);
        }
        let fees = pfd as u64;
        Ok((fees as i64, fees * margin_call_vault_pct / 100))
    } else {
        let gain = -get_system_fee_delta(fee);
        let backstop = if gain > 0 {
            (gain as u64) * standard_vault_pct / 100
        } else {
            0
        };
        Ok((gain, backstop))
    }
}
