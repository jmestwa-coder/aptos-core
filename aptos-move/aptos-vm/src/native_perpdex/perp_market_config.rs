// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::perp_market_config

use crate::native_perpdex::math::{self, Precision};
use crate::native_perpdex::oracle::OracleSource;
use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const MAX_ALLOWLIST_SIZE: u64 = 100;

const EINVALID_MIN_SIZE: u64 = 1;
const EINVALID_LOT_SIZE: u64 = 2;
const EINVALID_TICKER_SIZE: u64 = 3;
const ESIZE_NOT_RESPECTING_MIN_SIZE: u64 = 4;
const ESIZE_NOT_RESPECTING_LOT_SIZE: u64 = 5;
const EPRICE_NOT_RESPECTING_TICKER_SIZE: u64 = 6;
const EINVALID_ALLOWLIST_SIZE: u64 = 7;
const EINVALID_PRICE: u64 = 8;
const EINVALID_SIZE: u64 = 9;
const EORDER_SIZE_TOO_LARGE: u64 = 10;
const EPRICE_SIZES_LENGTH_MISMATCH: u64 = 11;
const ECANNOT_CHANGE_MODE_WHEN_DELISTING: u64 = 12;
const EINVALID_PCT: u64 = 13;
const EINVALID_COOLDOWN: u64 = 14;
const EMARGIN_CALL_FEE_EXCEEDS_LEVERAGE_CAP: u64 = 15;
const ESIZE_MULTIPLIER_LEVERAGE_OVERFLOW: u64 = 16;
const EMARKET_IS_ISOLATED_ONLY: u64 = 17;
const ECANNOT_TIGHTEN_ISOLATED_ONLY_RESTRICTION: u64 = 18;

const SLIPPAGE_AND_MARGIN_CALL_FEE_PCT_SCALE: u64 = 1000000;

const MAX_I64: u64 = i64::MAX as u64;
const MAX_U64: u128 = u64::MAX as u128;

// ===================== Types =====================

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PerpMarketConfiguration {
    V1 {
        info: PerpMarketInfoConfig,
        precision: PerpMarketPrecisionConfig,
        risk: PerpMarketRiskConfig,
        state: PerpMarketStateConfig,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PerpMarketInfoConfig {
    V1 { name: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum PerpMarketPrecisionConfig {
    V1 {
        sz_precision: Precision,
        min_size: u64,
        lot_size: u64,
        ticker_size: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PerpMarketRiskConfig {
    V1 {
        max_leverage: u8,
        liquidation_details: MarketLiquidationConfig,
        is_isolated_only: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PerpMarketStateConfig {
    V1 {
        mode: MarketMode,
        previous_market_mode: Option<MarketMode>,
        adl_trigger_threshold: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PerpMarketOracleSource {
    V1 { oracle_source: OracleSource },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum MarketLiquidationConfig {
    V1 {
        margin_call_fee_pct: u64,
        margin_call_backstop_pct: u64,
        starting_slippage_pct: u64,
        slippage_increment_pct: u64,
        cooldown_period_micros: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarketMode {
    Open,
    ReduceOnly {
        reason: ReduceOnlyReason,
        allowlist: Vec<[u8; 32]>,
    },
    AllowlistOnly {
        allowlist: Vec<[u8; 32]>,
    },
    Halt,
    Delisting,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReduceOnlyReason {
    OracleStale,
    AdminOperation,
}

/// Event data returned when market status changes.
/// EVENT: MarketStatusChangeEvent
#[derive(Clone, Debug)]
pub struct MarketStatusChangeEvent {
    pub mode: MarketMode,
    pub reason: Option<String>,
}

// ===================== Functions =====================

pub fn get_slippage_and_margin_call_fee_scale() -> u64 {
    SLIPPAGE_AND_MARGIN_CALL_FEE_PCT_SCALE
}

/// Returns the maximum allowed margin_call_fee_pct for a given max_leverage.
/// Formula: floor((SCALE / max_leverage) * bmr_ratio)
pub fn get_margin_call_fee_cap(max_leverage: u8) -> u64 {
    SLIPPAGE_AND_MARGIN_CALL_FEE_PCT_SCALE / (max_leverage as u64) / 3
}

/// Register a new market configuration. Returns the configuration to be stored.
/// In Move this does move_to; in Rust the caller stores the returned value.
pub fn register_market(
    name: String,
    sz_decimals: u8,
    min_size: u64,
    lot_size: u64,
    ticker_size: u64,
    max_leverage: u8,
    margin_call_fee_pct: u64,
    is_isolated_only: bool,
) -> Result<PerpMarketConfiguration, u64> {
    if lot_size == 0 {
        return Err(EINVALID_LOT_SIZE);
    }
    if min_size == 0 {
        return Err(EINVALID_MIN_SIZE);
    }
    if min_size % lot_size != 0 {
        return Err(EINVALID_MIN_SIZE);
    }
    if ticker_size == 0 {
        return Err(EINVALID_TICKER_SIZE);
    }
    let fee_cap = get_margin_call_fee_cap(max_leverage);
    if margin_call_fee_pct > fee_cap {
        return Err(EMARGIN_CALL_FEE_EXCEEDS_LEVERAGE_CAP);
    }
    let precision = math::new_precision(sz_decimals)?;
    let size_multiplier = math::get_decimals_multiplier(&precision);
    if (size_multiplier as u128) * (max_leverage as u128) >= MAX_U64 {
        return Err(ESIZE_MULTIPLIER_LEVERAGE_OVERFLOW);
    }

    Ok(PerpMarketConfiguration::V1 {
        info: PerpMarketInfoConfig::V1 { name },
        precision: PerpMarketPrecisionConfig::V1 {
            sz_precision: precision,
            min_size,
            lot_size,
            ticker_size,
        },
        risk: PerpMarketRiskConfig::V1 {
            max_leverage,
            liquidation_details: MarketLiquidationConfig::V1 {
                margin_call_fee_pct,
                margin_call_backstop_pct: 100,     // 100%
                starting_slippage_pct: 5000,       // .5%
                slippage_increment_pct: 5000,      // .5%
                cooldown_period_micros: 1000000,   // 1 second
            },
            is_isolated_only,
        },
        state: PerpMarketStateConfig::V1 {
            mode: MarketMode::Open,
            previous_market_mode: None,
            adl_trigger_threshold: 0,
        },
    })
}

// ===================== Getters =====================

pub fn get_sz_decimals(config: &PerpMarketConfiguration) -> u8 {
    let PerpMarketConfiguration::V1 { precision, .. } = config;
    let PerpMarketPrecisionConfig::V1 { sz_precision, .. } = precision;
    math::get_decimals(sz_precision)
}

pub fn get_sz_precision(config: &PerpMarketConfiguration) -> Precision {
    let PerpMarketConfiguration::V1 { precision, .. } = config;
    let PerpMarketPrecisionConfig::V1 { sz_precision, .. } = precision;
    *sz_precision
}

pub fn get_size_multiplier(config: &PerpMarketConfiguration) -> u64 {
    let p = get_sz_precision(config);
    math::get_decimals_multiplier(&p)
}

pub fn get_min_size(config: &PerpMarketConfiguration) -> u64 {
    let PerpMarketConfiguration::V1 { precision, .. } = config;
    let PerpMarketPrecisionConfig::V1 { min_size, .. } = precision;
    *min_size
}

pub fn get_lot_size(config: &PerpMarketConfiguration) -> u64 {
    let PerpMarketConfiguration::V1 { precision, .. } = config;
    let PerpMarketPrecisionConfig::V1 { lot_size, .. } = precision;
    *lot_size
}

pub fn get_ticker_size(config: &PerpMarketConfiguration) -> u64 {
    let PerpMarketConfiguration::V1 { precision, .. } = config;
    let PerpMarketPrecisionConfig::V1 { ticker_size, .. } = precision;
    *ticker_size
}

pub fn get_max_leverage(config: &PerpMarketConfiguration) -> u8 {
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 { max_leverage, .. } = risk;
    *max_leverage
}

pub fn get_name(config: &PerpMarketConfiguration) -> &str {
    let PerpMarketConfiguration::V1 { info, .. } = config;
    let PerpMarketInfoConfig::V1 { name } = info;
    name.as_str()
}

pub fn get_margin_call_fee_pct(config: &PerpMarketConfiguration) -> u64 {
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 {
        liquidation_details,
        ..
    } = risk;
    let MarketLiquidationConfig::V1 {
        margin_call_fee_pct,
        ..
    } = liquidation_details;
    *margin_call_fee_pct
}

pub fn get_margin_call_backstop_pct(config: &PerpMarketConfiguration) -> u64 {
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 {
        liquidation_details,
        ..
    } = risk;
    let MarketLiquidationConfig::V1 {
        margin_call_backstop_pct,
        ..
    } = liquidation_details;
    *margin_call_backstop_pct
}

pub fn get_starting_slippage_pct(config: &PerpMarketConfiguration) -> u64 {
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 {
        liquidation_details,
        ..
    } = risk;
    let MarketLiquidationConfig::V1 {
        starting_slippage_pct,
        ..
    } = liquidation_details;
    *starting_slippage_pct
}

pub fn get_slippage_increment_pct(config: &PerpMarketConfiguration) -> u64 {
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 {
        liquidation_details,
        ..
    } = risk;
    let MarketLiquidationConfig::V1 {
        slippage_increment_pct,
        ..
    } = liquidation_details;
    *slippage_increment_pct
}

pub fn get_cooldown_period_micros(config: &PerpMarketConfiguration) -> u64 {
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 {
        liquidation_details,
        ..
    } = risk;
    let MarketLiquidationConfig::V1 {
        cooldown_period_micros,
        ..
    } = liquidation_details;
    *cooldown_period_micros
}

pub fn get_adl_trigger_threshold(config: &PerpMarketConfiguration) -> u64 {
    let PerpMarketConfiguration::V1 { state, .. } = config;
    let PerpMarketStateConfig::V1 {
        adl_trigger_threshold,
        ..
    } = state;
    *adl_trigger_threshold
}

pub fn get_is_isolated_only(config: &PerpMarketConfiguration) -> bool {
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 {
        is_isolated_only, ..
    } = risk;
    *is_isolated_only
}

pub fn get_market_mode(config: &PerpMarketConfiguration) -> &MarketMode {
    let PerpMarketConfiguration::V1 { state, .. } = config;
    let PerpMarketStateConfig::V1 { mode, .. } = state;
    mode
}

// ===================== Validation =====================

fn validate_size_internal(
    lot_size: u64,
    min_size: u64,
    size: u64,
    allow_below_min_size: bool,
) -> Result<(), u64> {
    if size == 0 {
        return Err(EINVALID_SIZE);
    }
    if size > MAX_I64 {
        return Err(EORDER_SIZE_TOO_LARGE);
    }
    if size % lot_size != 0 {
        return Err(ESIZE_NOT_RESPECTING_LOT_SIZE);
    }
    if !allow_below_min_size && size < min_size {
        return Err(ESIZE_NOT_RESPECTING_MIN_SIZE);
    }
    Ok(())
}

fn validate_price_internal(ticker_size: u64, price: u64) -> Result<(), u64> {
    if price == 0 {
        return Err(EINVALID_PRICE);
    }
    if price % ticker_size != 0 {
        return Err(EPRICE_NOT_RESPECTING_TICKER_SIZE);
    }
    Ok(())
}

fn validate_price_and_size_internal(
    ticker_size: u64,
    lot_size: u64,
    min_size: u64,
    precision_multiplier: u64,
    price: u64,
    size: u64,
    allow_below_min_size: bool,
) -> Result<(), u64> {
    validate_price_internal(ticker_size, price)?;
    validate_size_internal(lot_size, min_size, size, allow_below_min_size)?;
    if (price as u128) * (size as u128) > (MAX_I64 as u128) * (precision_multiplier as u128) {
        return Err(EORDER_SIZE_TOO_LARGE);
    }
    Ok(())
}

pub fn validate_size(
    config: &PerpMarketConfiguration,
    size: u64,
    allow_below_min_size: bool,
) -> Result<(), u64> {
    let lot_size = get_lot_size(config);
    let min_size = get_min_size(config);
    validate_size_internal(lot_size, min_size, size, allow_below_min_size)
}

pub fn validate_price(config: &PerpMarketConfiguration, price: u64) -> Result<(), u64> {
    let ticker_size = get_ticker_size(config);
    validate_price_internal(ticker_size, price)
}

pub fn validate_price_and_size(
    config: &PerpMarketConfiguration,
    price: u64,
    size: u64,
    allow_below_min_size: bool,
) -> Result<(), u64> {
    let ticker_size = get_ticker_size(config);
    let lot_size = get_lot_size(config);
    let min_size = get_min_size(config);
    let precision_multiplier = get_size_multiplier(config);
    validate_price_and_size_internal(
        ticker_size,
        lot_size,
        min_size,
        precision_multiplier,
        price,
        size,
        allow_below_min_size,
    )
}

pub fn validate_array_of_price_and_size(
    config: &PerpMarketConfiguration,
    prices: &[u64],
    sizes: &[u64],
) -> Result<(), u64> {
    if prices.len() != sizes.len() {
        return Err(EPRICE_SIZES_LENGTH_MISMATCH);
    }

    let ticker_size = get_ticker_size(config);
    let lot_size = get_lot_size(config);
    let min_size = get_min_size(config);
    let precision_multiplier = get_size_multiplier(config) as u128;

    for i in 0..prices.len() {
        let price = prices[i];
        let size = sizes[i];
        validate_price_internal(ticker_size, price)?;
        validate_size_internal(lot_size, min_size, size, false)?;
        if (price as u128) * (size as u128) > (MAX_I64 as u128) * precision_multiplier {
            return Err(EORDER_SIZE_TOO_LARGE);
        }
    }
    Ok(())
}

pub fn validate_price_and_size_allow_below_min_size(
    config: &PerpMarketConfiguration,
    price: u64,
    size: u64,
) -> Result<(), u64> {
    validate_price_and_size(config, price, size, true)
}

// ===================== Rounding =====================

fn safe_round_to_granularity(value: u64, granularity: u64, ceil: bool) -> Result<u64, u64> {
    let divided = math::div_direction_64(value, granularity, ceil)?;
    let result = (divided as u128) * (granularity as u128);
    if result > u64::MAX as u128 {
        // This should never happen, but if it does, return smaller value
        Ok((result - (granularity as u128)) as u64)
    } else {
        Ok(result as u64)
    }
}

pub fn round_price_to_ticker(
    config: &PerpMarketConfiguration,
    price: u64,
    ceil: bool,
) -> Result<u64, u64> {
    let ticker_size = get_ticker_size(config);
    safe_round_to_granularity(price, ticker_size, ceil)
}

pub fn round_size_to_lot(
    config: &PerpMarketConfiguration,
    size: u64,
    ceil: bool,
) -> Result<u64, u64> {
    let min_size = get_min_size(config);
    let lot_size = get_lot_size(config);
    let rounded_size = safe_round_to_granularity(size, lot_size, ceil)?;
    if rounded_size < min_size {
        if ceil {
            Ok(min_size)
        } else {
            Ok(0)
        }
    } else {
        Ok(rounded_size)
    }
}

// ===================== Market mode checks =====================

pub fn is_open(config: &PerpMarketConfiguration) -> bool {
    matches!(get_market_mode(config), MarketMode::Open)
}

pub fn is_reduce_only(
    config: &PerpMarketConfiguration,
    order_address: &[u8; 32],
) -> bool {
    match get_market_mode(config) {
        MarketMode::Open => false,
        MarketMode::ReduceOnly { allowlist, .. } => !allowlist.contains(order_address),
        MarketMode::AllowlistOnly { .. } => false,
        MarketMode::Halt => false,
        MarketMode::Delisting => false,
    }
}

pub fn can_place_order(
    config: &PerpMarketConfiguration,
    order_address: &[u8; 32],
) -> bool {
    match get_market_mode(config) {
        MarketMode::Open => true,
        MarketMode::ReduceOnly { .. } => true,
        MarketMode::AllowlistOnly { allowlist } => allowlist.contains(order_address),
        MarketMode::Halt => false,
        MarketMode::Delisting => false,
    }
}

pub fn can_settle_order(
    config: &PerpMarketConfiguration,
    maker_address: &[u8; 32],
    taker_address: &[u8; 32],
) -> bool {
    match get_market_mode(config) {
        MarketMode::Open => true,
        MarketMode::ReduceOnly { .. } => true,
        MarketMode::AllowlistOnly { allowlist } => {
            allowlist.contains(maker_address) || allowlist.contains(taker_address)
        }
        MarketMode::Halt => false,
        MarketMode::Delisting => false,
    }
}

pub fn can_update_oracle(config: &PerpMarketConfiguration) -> bool {
    match get_market_mode(config) {
        MarketMode::Open => true,
        MarketMode::ReduceOnly { .. } => true,
        MarketMode::AllowlistOnly { .. } => true,
        MarketMode::Halt => false,
        MarketMode::Delisting => false,
    }
}

pub fn is_market_delisted(config: &PerpMarketConfiguration) -> bool {
    matches!(get_market_mode(config), MarketMode::Delisting)
}

// ===================== Setters =====================

/// Set market mode. Returns event data if the mode changed.
/// Once a market is in Delisting mode, it cannot be changed.
/// EVENT: MarketStatusChangeEvent
pub fn set_market_mode(
    config: &mut PerpMarketConfiguration,
    new_mode: MarketMode,
    reason: Option<String>,
) -> Result<Option<MarketStatusChangeEvent>, u64> {
    let PerpMarketConfiguration::V1 { state, .. } = config;
    let PerpMarketStateConfig::V1 { mode, .. } = state;

    if matches!(mode, MarketMode::Delisting) {
        return Err(ECANNOT_CHANGE_MODE_WHEN_DELISTING);
    }

    if mode == &new_mode {
        return Ok(None);
    }

    *mode = new_mode.clone();

    Ok(Some(MarketStatusChangeEvent {
        mode: new_mode,
        reason,
    }))
}

pub fn set_reduce_only(
    config: &mut PerpMarketConfiguration,
    allowlist: Vec<[u8; 32]>,
    reason: Option<String>,
) -> Result<Option<MarketStatusChangeEvent>, u64> {
    if allowlist.len() > MAX_ALLOWLIST_SIZE as usize {
        return Err(EINVALID_ALLOWLIST_SIZE);
    }
    set_market_mode(
        config,
        MarketMode::ReduceOnly {
            allowlist,
            reason: ReduceOnlyReason::AdminOperation,
        },
        reason,
    )
}

pub fn set_reduce_only_on_oracle_stale(
    config: &mut PerpMarketConfiguration,
    allowlist: Vec<[u8; 32]>,
) -> Result<Option<MarketStatusChangeEvent>, u64> {
    if allowlist.len() > MAX_ALLOWLIST_SIZE as usize {
        return Err(EINVALID_ALLOWLIST_SIZE);
    }

    let PerpMarketConfiguration::V1 { state, .. } = config;
    let PerpMarketStateConfig::V1 {
        mode,
        previous_market_mode,
        ..
    } = state;

    // Only transition from Open
    if !matches!(mode, MarketMode::Open) {
        return Ok(None);
    }

    let current_mode = mode.clone();
    let new_mode = MarketMode::ReduceOnly {
        allowlist,
        reason: ReduceOnlyReason::OracleStale,
    };

    if &current_mode == &new_mode {
        return Ok(None);
    }

    *previous_market_mode = Some(current_mode);
    *mode = new_mode.clone();

    Ok(Some(MarketStatusChangeEvent {
        mode: new_mode,
        reason: Some("Oracle stale".to_string()),
    }))
}

pub fn set_open(
    config: &mut PerpMarketConfiguration,
    reason: Option<String>,
) -> Result<Option<MarketStatusChangeEvent>, u64> {
    set_market_mode(config, MarketMode::Open, reason)
}

pub fn halt_market(
    config: &mut PerpMarketConfiguration,
    reason: Option<String>,
) -> Result<Option<MarketStatusChangeEvent>, u64> {
    set_market_mode(config, MarketMode::Halt, reason)
}

/// Resume market to previous mode from reduce-only (oracle stale recovery).
/// EVENT: MarketStatusChangeEvent
pub fn resume_market_to_previous_mode_from_reduce_only(
    config: &mut PerpMarketConfiguration,
) -> Option<MarketStatusChangeEvent> {
    let PerpMarketConfiguration::V1 { state, .. } = config;
    let PerpMarketStateConfig::V1 {
        mode,
        previous_market_mode,
        ..
    } = state;

    match mode {
        MarketMode::ReduceOnly {
            reason: ReduceOnlyReason::OracleStale,
            ..
        } => {
            let prev_mode = previous_market_mode
                .take()
                .expect("previous_market_mode must be set");
            *mode = prev_mode.clone();
            Some(MarketStatusChangeEvent {
                mode: prev_mode,
                reason: Some("Oracle recovered from staled state".to_string()),
            })
        }
        _ => None,
    }
}

/// Delist market.
/// EVENT: MarketStatusChangeEvent
pub fn delist_market(
    config: &mut PerpMarketConfiguration,
    reason: Option<String>,
) -> MarketStatusChangeEvent {
    let PerpMarketConfiguration::V1 { state, .. } = config;
    let PerpMarketStateConfig::V1 { mode, .. } = state;
    *mode = MarketMode::Delisting;
    MarketStatusChangeEvent {
        mode: MarketMode::Delisting,
        reason,
    }
}

pub fn allowlist_only(
    config: &mut PerpMarketConfiguration,
    allowlist: Vec<[u8; 32]>,
    reason: Option<String>,
) -> Result<Option<MarketStatusChangeEvent>, u64> {
    if allowlist.len() > MAX_ALLOWLIST_SIZE as usize {
        return Err(EINVALID_ALLOWLIST_SIZE);
    }
    set_market_mode(config, MarketMode::AllowlistOnly { allowlist }, reason)
}

pub fn set_max_leverage(
    config: &mut PerpMarketConfiguration,
    max_leverage: u8,
) -> Result<(), u64> {
    let PerpMarketConfiguration::V1 {
        precision, risk, ..
    } = config;
    let PerpMarketRiskConfig::V1 {
        liquidation_details,
        max_leverage: current_leverage,
        ..
    } = risk;
    let fee_cap = get_margin_call_fee_cap(max_leverage);
    let MarketLiquidationConfig::V1 {
        margin_call_fee_pct,
        ..
    } = liquidation_details;
    if *margin_call_fee_pct > fee_cap {
        return Err(EMARGIN_CALL_FEE_EXCEEDS_LEVERAGE_CAP);
    }
    let PerpMarketPrecisionConfig::V1 { sz_precision, .. } = precision;
    let size_multiplier = math::get_decimals_multiplier(sz_precision);
    if (size_multiplier as u128) * (max_leverage as u128) >= MAX_U64 {
        return Err(ESIZE_MULTIPLIER_LEVERAGE_OVERFLOW);
    }
    *current_leverage = max_leverage;
    Ok(())
}

pub fn set_margin_call_fee_pct(
    config: &mut PerpMarketConfiguration,
    margin_call_fee_pct: u64,
) -> Result<(), u64> {
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 {
        max_leverage,
        liquidation_details,
        ..
    } = risk;
    let fee_cap = get_margin_call_fee_cap(*max_leverage);
    if margin_call_fee_pct > fee_cap {
        return Err(EMARGIN_CALL_FEE_EXCEEDS_LEVERAGE_CAP);
    }
    set_liquidation_field_margin_call_fee_pct(liquidation_details, margin_call_fee_pct);
    Ok(())
}

pub fn set_margin_call_backstop_pct(
    config: &mut PerpMarketConfiguration,
    margin_call_backstop_pct: u64,
) -> Result<(), u64> {
    if margin_call_backstop_pct > 100 {
        return Err(EINVALID_PCT);
    }
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 {
        liquidation_details,
        ..
    } = risk;
    set_liquidation_field_margin_call_backstop_pct(liquidation_details, margin_call_backstop_pct);
    Ok(())
}

pub fn set_starting_slippage_pct(
    config: &mut PerpMarketConfiguration,
    starting_slippage_pct: u64,
) -> Result<(), u64> {
    if starting_slippage_pct > SLIPPAGE_AND_MARGIN_CALL_FEE_PCT_SCALE {
        return Err(EINVALID_PCT);
    }
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 {
        liquidation_details,
        ..
    } = risk;
    set_liquidation_field_starting_slippage_pct(liquidation_details, starting_slippage_pct);
    Ok(())
}

pub fn set_slippage_increment_pct(
    config: &mut PerpMarketConfiguration,
    slippage_increment_pct: u64,
) -> Result<(), u64> {
    if slippage_increment_pct > SLIPPAGE_AND_MARGIN_CALL_FEE_PCT_SCALE {
        return Err(EINVALID_PCT);
    }
    if slippage_increment_pct == 0 {
        return Err(EINVALID_PCT);
    }
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 {
        liquidation_details,
        ..
    } = risk;
    set_liquidation_field_slippage_increment_pct(liquidation_details, slippage_increment_pct);
    Ok(())
}

pub fn set_cooldown_period_micros(
    config: &mut PerpMarketConfiguration,
    cooldown_period_micros: u64,
) -> Result<(), u64> {
    if cooldown_period_micros == 0 {
        return Err(EINVALID_COOLDOWN);
    }
    if cooldown_period_micros > 30000000 {
        return Err(EINVALID_COOLDOWN);
    }
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 {
        liquidation_details,
        ..
    } = risk;
    set_liquidation_field_cooldown_period_micros(liquidation_details, cooldown_period_micros);
    Ok(())
}

/// Allow cross margin mode on a market.
/// One-way transition: can only change from isolated-only to cross-allowed.
pub fn allow_cross_margin_mode(config: &mut PerpMarketConfiguration) {
    let PerpMarketConfiguration::V1 { risk, .. } = config;
    let PerpMarketRiskConfig::V1 {
        is_isolated_only, ..
    } = risk;
    if *is_isolated_only {
        *is_isolated_only = false;
    }
}

pub fn set_adl_trigger_threshold(
    config: &mut PerpMarketConfiguration,
    threshold: u64,
) {
    let PerpMarketConfiguration::V1 { state, .. } = config;
    let PerpMarketStateConfig::V1 {
        adl_trigger_threshold,
        ..
    } = state;
    *adl_trigger_threshold = threshold;
}

pub fn set_min_size(
    config: &mut PerpMarketConfiguration,
    min_size: u64,
) -> Result<(), u64> {
    if min_size == 0 {
        return Err(EINVALID_MIN_SIZE);
    }
    let lot_size = get_lot_size(config);
    if min_size % lot_size != 0 {
        return Err(EINVALID_MIN_SIZE);
    }
    let PerpMarketConfiguration::V1 { precision, .. } = config;
    set_precision_field_min_size(precision, min_size);
    Ok(())
}

pub fn decrease_lot_size(
    config: &mut PerpMarketConfiguration,
    new_lot_size: u64,
) -> Result<(), u64> {
    if new_lot_size == 0 {
        return Err(EINVALID_LOT_SIZE);
    }
    let current_lot_size = get_lot_size(config);
    if current_lot_size % new_lot_size != 0 {
        return Err(EINVALID_LOT_SIZE);
    }
    let PerpMarketConfiguration::V1 { precision, .. } = config;
    let PerpMarketPrecisionConfig::V1 { lot_size, .. } = precision;
    *lot_size = new_lot_size;
    Ok(())
}

pub fn increase_lot_size(
    config: &mut PerpMarketConfiguration,
    new_lot_size: u64,
) -> Result<(), u64> {
    if new_lot_size == 0 {
        return Err(EINVALID_LOT_SIZE);
    }
    let current_lot_size = get_lot_size(config);
    let min_size = get_min_size(config);
    if new_lot_size % current_lot_size != 0 {
        return Err(EINVALID_LOT_SIZE);
    }
    if min_size % new_lot_size != 0 {
        return Err(EINVALID_MIN_SIZE);
    }
    let PerpMarketConfiguration::V1 { precision, .. } = config;
    let PerpMarketPrecisionConfig::V1 { lot_size, .. } = precision;
    *lot_size = new_lot_size;
    Ok(())
}


// ===================== Internal field-mutation helpers =====================
// These exist to avoid `ref mut` inside implicitly-borrowing patterns.

fn set_liquidation_field_margin_call_fee_pct(ld: &mut MarketLiquidationConfig, v: u64) {
    let MarketLiquidationConfig::V1 { margin_call_fee_pct, .. } = ld;
    *margin_call_fee_pct = v;
}

fn set_liquidation_field_margin_call_backstop_pct(ld: &mut MarketLiquidationConfig, v: u64) {
    let MarketLiquidationConfig::V1 { margin_call_backstop_pct, .. } = ld;
    *margin_call_backstop_pct = v;
}

fn set_liquidation_field_starting_slippage_pct(ld: &mut MarketLiquidationConfig, v: u64) {
    let MarketLiquidationConfig::V1 { starting_slippage_pct, .. } = ld;
    *starting_slippage_pct = v;
}

fn set_liquidation_field_slippage_increment_pct(ld: &mut MarketLiquidationConfig, v: u64) {
    let MarketLiquidationConfig::V1 { slippage_increment_pct, .. } = ld;
    *slippage_increment_pct = v;
}

fn set_liquidation_field_cooldown_period_micros(ld: &mut MarketLiquidationConfig, v: u64) {
    let MarketLiquidationConfig::V1 { cooldown_period_micros, .. } = ld;
    *cooldown_period_micros = v;
}

fn set_precision_field_min_size(p: &mut PerpMarketPrecisionConfig, v: u64) {
    let PerpMarketPrecisionConfig::V1 { min_size, .. } = p;
    *min_size = v;
}

// ===================== Tests =====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_round_to_granularity() {
        assert_eq!(safe_round_to_granularity(10, 3, false).unwrap(), 9);
        assert_eq!(safe_round_to_granularity(10, 3, true).unwrap(), 12);
        assert_eq!(safe_round_to_granularity(10, 1, false).unwrap(), 10);
        assert_eq!(safe_round_to_granularity(10, 1, true).unwrap(), 10);
        assert_eq!(safe_round_to_granularity(10, 10, false).unwrap(), 10);
        assert_eq!(safe_round_to_granularity(10, 10, true).unwrap(), 10);
        assert_eq!(safe_round_to_granularity(10, 100, false).unwrap(), 0);
        assert_eq!(safe_round_to_granularity(10, 100, true).unwrap(), 100);

        assert_eq!(
            safe_round_to_granularity(u64::MAX, 1, false).unwrap(),
            u64::MAX
        );
        assert_eq!(
            safe_round_to_granularity(u64::MAX, 1, true).unwrap(),
            u64::MAX
        );
        assert_eq!(
            safe_round_to_granularity(u64::MAX, 2, false).unwrap(),
            u64::MAX - 1
        );
        assert_eq!(
            safe_round_to_granularity(u64::MAX, 2, true).unwrap(),
            u64::MAX - 1
        );
    }

    #[test]
    fn test_register_market() {
        let config = register_market(
            "BTC-PERP".to_string(),
            6,
            1,
            1,
            1,
            20,
            5000,
            false,
        )
        .unwrap();
        assert_eq!(get_max_leverage(&config), 20);
        assert!(is_open(&config));
    }

    #[test]
    fn test_validate_size() {
        let config = register_market(
            "TEST".to_string(),
            6,
            100,
            10,
            5,
            20,
            5000,
            false,
        )
        .unwrap();

        // Valid size
        assert!(validate_size(&config, 100, false).is_ok());
        assert!(validate_size(&config, 110, false).is_ok());

        // Size not multiple of lot
        assert!(validate_size(&config, 15, false).is_err());

        // Size below min
        assert!(validate_size(&config, 10, false).is_err());

        // Allow below min
        assert!(validate_size(&config, 10, true).is_ok());
    }
}


// ===================== Stub functions for perp_engine delegation =====================
// These accept market addresses ([u8; 32]) and are called from perp_engine.
// The dispatch layer resolves addresses to actual PerpMarketConfiguration resources.

pub fn update_internal_oracle_price(_market: [u8; 32], _price: u64) {
    // Dispatch layer updates the oracle price in the configuration resource
}

pub fn update_oracle_status(_market: [u8; 32]) {
    // Dispatch layer updates oracle health status
}

pub fn is_market_delisted_by_addr(_market: [u8; 32]) -> bool {
    false
}

pub fn get_margin_call_fee_pct_by_addr(_market: [u8; 32]) -> u64 {
    0
}

pub fn get_margin_call_fee_cap_by_addr(_max_leverage: u8) -> u64 {
    SLIPPAGE_AND_MARGIN_CALL_FEE_PCT_SCALE / (_max_leverage as u64) / 3
}

pub fn get_ticker_size_by_addr(_market: [u8; 32]) -> u64 {
    1
}

pub fn validate_price_and_size_by_addr(
    _market: [u8; 32], _price: u64, _size: u64, _allow_below_min_size: bool,
) -> Result<(), u64> {
    Ok(())
}

pub fn validate_size_by_addr(_market: [u8; 32], _size: u64, _allow_below_min: bool) -> Result<(), u64> {
    Ok(())
}

pub fn round_price_to_ticker_by_addr(_market: [u8; 32], price: u64, _ceil: bool) -> u64 {
    price
}

pub fn validate_price_and_size_allow_below_min_size_by_addr(
    _market: [u8; 32], _price: u64, _size: u64,
) -> Result<(), u64> {
    Ok(())
}

pub struct OracleData {
    pub price: u64,
    pub status: OracleStatus,
}

pub enum OracleStatus {
    Valid,
    Invalid,
    Down,
}

impl OracleData {
    pub fn is_status_invalid(&self) -> bool {
        matches!(self.status, OracleStatus::Invalid)
    }
    pub fn is_status_down(&self) -> bool {
        matches!(self.status, OracleStatus::Down)
    }
    pub fn get_price(&self) -> u64 {
        self.price
    }
}

pub fn get_oracle_data(
    _market: [u8; 32], _target_precision: crate::native_perpdex::math::Precision,
) -> OracleData {
    OracleData { price: 0, status: OracleStatus::Valid }
}

// ===================== Open interest tracker dispatch stubs =====================
// In Move these are in open_interest_tracker module, which is not a separate Rust module.

pub fn increase_max_open_interest_by_addr(_market: [u8; 32], _new_oi: u64) {
    // Dispatch layer resolves OpenInterestTracker
}

pub fn decrease_max_open_interest_by_addr(_market: [u8; 32], _new_oi: u64) {
    // Dispatch layer resolves OpenInterestTracker
}

pub fn increase_max_notional_open_interest_by_addr(_market: [u8; 32], _new_noi: u64) {
    // Dispatch layer resolves OpenInterestTracker
}

pub fn decrease_max_notional_open_interest_by_addr(_market: [u8; 32], _new_noi: u64) {
    // Dispatch layer resolves OpenInterestTracker
}

pub fn get_max_open_interest_delta_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves OpenInterestTracker
    0
}

pub fn get_current_open_interest_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves OpenInterestTracker
    0
}

pub fn get_max_notional_open_interest_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves OpenInterestTracker
    0
}

pub fn get_adl_trigger_threshold_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves PerpMarketConfiguration
    0
}

pub fn get_name_by_addr(_market: [u8; 32]) -> String {
    // Dispatch layer resolves PerpMarketConfiguration
    String::new()
}

pub fn get_max_leverage_by_addr(_market: [u8; 32]) -> u8 {
    // Dispatch layer resolves PerpMarketConfiguration
    1
}

pub fn get_sz_decimals_by_addr(_market: [u8; 32]) -> u8 {
    // Dispatch layer resolves PerpMarketConfiguration
    0
}

pub fn get_min_size_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves PerpMarketConfiguration
    0
}

pub fn get_lot_size_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves PerpMarketConfiguration
    0
}

pub fn get_margin_call_backstop_pct_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves PerpMarketConfiguration
    0
}

pub fn get_starting_slippage_pct_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves PerpMarketConfiguration
    0
}

pub fn get_slippage_increment_pct_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves PerpMarketConfiguration
    0
}

pub fn get_cooldown_period_micros_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves PerpMarketConfiguration
    0
}

pub fn is_open_by_addr(_market: [u8; 32]) -> bool {
    // Dispatch layer resolves PerpMarketConfiguration
    true
}

pub fn get_market_mode_by_addr(_market: [u8; 32]) -> MarketMode {
    // Dispatch layer resolves PerpMarketConfiguration
    MarketMode::Open
}

pub fn set_reduce_only_by_addr(
    _market: [u8; 32], _allowlist: Vec<[u8; 32]>, _reason: Option<String>,
) {
    // Dispatch layer resolves PerpMarketConfiguration
}

pub fn set_open_by_addr(_market: [u8; 32], _reason: Option<String>) {
    // Dispatch layer resolves PerpMarketConfiguration
}

pub fn halt_market_by_addr(_market: [u8; 32], _reason: Option<String>) {
    // Dispatch layer resolves PerpMarketConfiguration
}

pub fn allowlist_only_by_addr(
    _market: [u8; 32], _allowlist: Vec<[u8; 32]>, _reason: Option<String>,
) {
    // Dispatch layer resolves PerpMarketConfiguration
}

pub fn set_max_leverage_by_addr(_market: [u8; 32], _max_leverage: u8) {
    // Dispatch layer resolves PerpMarketConfiguration
}

pub fn set_margin_call_fee_pct_by_addr(_market: [u8; 32], _pct: u64) {
    // Dispatch layer resolves PerpMarketConfiguration
}

pub fn set_margin_call_backstop_pct_by_addr(_market: [u8; 32], _pct: u64) {
    // Dispatch layer resolves PerpMarketConfiguration
}

pub fn set_starting_slippage_pct_by_addr(_market: [u8; 32], _pct: u64) {
    // Dispatch layer resolves PerpMarketConfiguration
}

pub fn set_slippage_increment_pct_by_addr(_market: [u8; 32], _pct: u64) {
    // Dispatch layer resolves PerpMarketConfiguration
}

pub fn set_cooldown_period_micros_by_addr(_market: [u8; 32], _micros: u64) {
    // Dispatch layer resolves PerpMarketConfiguration
}

pub fn set_min_size_by_addr(_market: [u8; 32], _min_size: u64) {
    // Dispatch layer resolves PerpMarketConfiguration
}

pub fn decrease_lot_size_by_addr(_market: [u8; 32], _new_lot_size: u64) {
    // Dispatch layer resolves PerpMarketConfiguration + OpenInterestTracker
}

pub fn increase_lot_size_by_addr(_market: [u8; 32], _new_lot_size: u64) {
    // Dispatch layer resolves PerpMarketConfiguration + OpenInterestTracker
}

pub fn require_configuration_migration_by_addr(_market: [u8; 32]) -> bool {
    // Dispatch layer resolves PerpMarketConfiguration
    false
}

pub fn migrate_to_configuration_by_addr(_market: [u8; 32]) {
    // Dispatch layer resolves PerpMarketConfiguration + Global
}

pub fn delist_market_by_addr(_market: [u8; 32], _reason: Option<String>) {
    // Dispatch layer resolves PerpMarketConfiguration
}

pub fn get_oracle_source_by_addr(_market: [u8; 32]) -> Vec<u8> {
    // Dispatch layer resolves PerpMarketConfiguration, returns serialized OracleSource
    Vec::new()
}
