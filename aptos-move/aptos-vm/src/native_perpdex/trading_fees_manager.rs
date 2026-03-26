// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::trading_fees_manager

use crate::native_perpdex::builder_code_registry::{self, BuilderCode, Registry};
use crate::native_perpdex::fee_distribution::{
    self, FeeDistribution, FeeWithDestination,
};
use crate::native_perpdex::referral_registry::{self, Referrals, ReferralCode, ReferrerState};
use crate::native_perpdex::volume_tracker::{self, VolumeStats, VolumeHistoryView};
use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const EINVALID_ADDRESS: u64 = 1;
const EEXCEEDED_VOLUME_THRESHOLD: u64 = 2;
const EMARKET_MAKER_REBATE_EXCEEDS_TAKER_FEE: u64 = 3;
const EINVALID_FEE_TIER_ARRAY_LENGTHS: u64 = 4;
const EINVALID_BACKSTOP_VAULT_FEE_PCT: u64 = 5;

const FEE_PRECISION: u64 = 10000; // 10000 => 1%
const MARKET_MAKER_TIER_PCT_PRECISION: u64 = 100; // 100 => 1%
const USER_VOLUME_THRESHOLD_FOR_REFERRER: u64 = 10000; // 10k USDC

// ===================== Types =====================

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReferralFeeConfig {
    V1 {
        referral_fee_enabled: bool,
        referral_fee_pct: u64,
        referred_fee_discount_pct: u64,
        discount_eligibility_volume_threshold: u128,
        referrer_eligibility_volume_threshold: u128,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TradingFeeConfiguration {
    V1 {
        tier_thresholds: Vec<u128>,
        tier_maker_fees: Vec<u64>,
        tier_taker_fees: Vec<u64>,
        market_maker_absolute_threshold: u128,
        market_maker_tier_pct_thresholds: Vec<u64>,
        market_maker_tier_fee_rebates: Vec<u64>,
        builder_max_fee: u64,
        backstop_vault_fee_pct: u64,
        referral_fee_config: ReferralFeeConfig,
    },
}

/// RESOURCE: GlobalState at @decibel_dex
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GlobalState {
    V1 {
        volume_stats: VolumeStats,
        fee_config: TradingFeeConfiguration,
        referrals: Referrals,
        backstop_liquidator_address: [u8; 32],
    },
}

// ===================== Functions =====================

fn create_default_referral_fee_config(volume_precision_multiplier: u128) -> ReferralFeeConfig {
    ReferralFeeConfig::V1 {
        referral_fee_enabled: false,
        referral_fee_pct: 0,
        referred_fee_discount_pct: 0,
        discount_eligibility_volume_threshold: 100_000_000 * volume_precision_multiplier,
        referrer_eligibility_volume_threshold: (USER_VOLUME_THRESHOLD_FOR_REFERRER as u128) * volume_precision_multiplier,
    }
}

fn create_default_config(volume_precision_multiplier: u128) -> TradingFeeConfiguration {
    TradingFeeConfiguration::V1 {
        tier_thresholds: vec![
            10_000_000 * volume_precision_multiplier,
            50_000_000 * volume_precision_multiplier,
            200_000_000 * volume_precision_multiplier,
            1_000_000_000 * volume_precision_multiplier,
            4_000_000_000 * volume_precision_multiplier,
            15_000_000_000 * volume_precision_multiplier,
        ],
        tier_maker_fees: vec![110, 90, 60, 30, 0, 0, 0],
        tier_taker_fees: vec![340, 300, 250, 220, 210, 190, 180],
        market_maker_absolute_threshold: 0,
        market_maker_tier_pct_thresholds: vec![],
        market_maker_tier_fee_rebates: vec![],
        builder_max_fee: 1000,
        backstop_vault_fee_pct: 0,
        referral_fee_config: create_default_referral_fee_config(volume_precision_multiplier),
    }
}

pub fn initialize(
    admin_addr: [u8; 32],
    decibel_dex_addr: [u8; 32],
    volume_precision_multiplier: u64,
    backstop_liquidator_address: [u8; 32],
    now_seconds: u64,
) -> Result<(GlobalState, Registry), u64> {
    if admin_addr != decibel_dex_addr {
        return Err(EINVALID_ADDRESS);
    }
    let volume_stats = volume_tracker::initialize(now_seconds);
    let referrals = referral_registry::initialize();
    let fee_config = create_default_config(volume_precision_multiplier as u128);
    let registry = builder_code_registry::initialize(
        admin_addr,
        decibel_dex_addr,
        fee_config.builder_max_fee(),
    )?;

    let global_state = GlobalState::V1 {
        volume_stats,
        fee_config,
        referrals,
        backstop_liquidator_address,
    };
    // EVENT: TradingFeeTierUpdatedEvent
    Ok((global_state, registry))
}

// Convenience accessors for TradingFeeConfiguration
impl TradingFeeConfiguration {
    fn builder_max_fee(&self) -> u64 {
        let TradingFeeConfiguration::V1 { builder_max_fee, .. } = self;
        *builder_max_fee
    }
}

fn validate_fee_tier_array_lengths(
    tier_thresholds: &[u128],
    tier_maker_fees: &[u64],
    tier_taker_fees: &[u64],
    market_maker_tier_pct_thresholds: &[u64],
    market_maker_tier_fee_rebates: &[u64],
) -> Result<(), u64> {
    if tier_maker_fees.len() != tier_thresholds.len() + 1 {
        return Err(EINVALID_FEE_TIER_ARRAY_LENGTHS);
    }
    if tier_taker_fees.len() != tier_thresholds.len() + 1 {
        return Err(EINVALID_FEE_TIER_ARRAY_LENGTHS);
    }
    if !market_maker_tier_pct_thresholds.is_empty()
        && market_maker_tier_fee_rebates.len() != market_maker_tier_pct_thresholds.len() + 1
    {
        return Err(EINVALID_FEE_TIER_ARRAY_LENGTHS);
    }
    Ok(())
}

fn find_max_value(values: &[u64]) -> u64 {
    values.iter().copied().max().unwrap_or(0)
}

fn find_min_value(values: &[u64]) -> u64 {
    values.iter().copied().min().unwrap_or(0)
}

fn calculate_min_net_taker_fee(
    min_base_taker_fee: u64,
    referral_fee_enabled: bool,
    referral_fee_pct: u64,
    referred_fee_discount_pct: u64,
) -> u64 {
    if referral_fee_enabled {
        let after_discount =
            (min_base_taker_fee as u128) * (100 - referred_fee_discount_pct as u128) / 100;
        let after_referral_split = after_discount * (100 - referral_fee_pct as u128) / 100;
        after_referral_split as u64
    } else {
        min_base_taker_fee
    }
}

fn validate_rebate_vs_taker_fee(
    market_maker_tier_fee_rebates: &[u64],
    tier_taker_fees: &[u64],
    referral_fee_enabled: bool,
    referral_fee_pct: u64,
    referred_fee_discount_pct: u64,
) -> Result<(), u64> {
    if market_maker_tier_fee_rebates.is_empty() || tier_taker_fees.is_empty() {
        return Ok(());
    }
    let max_rebate = find_max_value(market_maker_tier_fee_rebates);
    let min_base_taker_fee = find_min_value(tier_taker_fees);
    let min_net_taker_fee = calculate_min_net_taker_fee(
        min_base_taker_fee,
        referral_fee_enabled,
        referral_fee_pct,
        referred_fee_discount_pct,
    );
    if max_rebate > min_net_taker_fee {
        return Err(EMARKET_MAKER_REBATE_EXCEEDS_TAKER_FEE);
    }
    Ok(())
}

pub fn update_fee_config(
    global_state: &mut GlobalState,
    builder_registry: &mut Registry,
    tier_thresholds: Vec<u128>,
    tier_maker_fees: Vec<u64>,
    tier_taker_fees: Vec<u64>,
    market_maker_absolute_threshold: u128,
    market_maker_tier_pct_thresholds: Vec<u64>,
    market_maker_tier_fee_rebates: Vec<u64>,
    builder_max_fee: u64,
    backstop_vault_fee_pct: u64,
    referral_fee_enabled: bool,
    referral_fee_pct: u64,
    referred_fee_discount_pct: u64,
    discount_eligibility_volume_threshold: u128,
    referrer_eligibility_volume_threshold: u128,
) -> Result<(), u64> {
    if backstop_vault_fee_pct > 100 {
        return Err(EINVALID_BACKSTOP_VAULT_FEE_PCT);
    }
    validate_fee_tier_array_lengths(
        &tier_thresholds,
        &tier_maker_fees,
        &tier_taker_fees,
        &market_maker_tier_pct_thresholds,
        &market_maker_tier_fee_rebates,
    )?;
    validate_rebate_vs_taker_fee(
        &market_maker_tier_fee_rebates,
        &tier_taker_fees,
        referral_fee_enabled,
        referral_fee_pct,
        referred_fee_discount_pct,
    )?;

    let fee_config = TradingFeeConfiguration::V1 {
        tier_thresholds,
        tier_maker_fees,
        tier_taker_fees,
        market_maker_absolute_threshold,
        market_maker_tier_pct_thresholds,
        market_maker_tier_fee_rebates,
        builder_max_fee,
        backstop_vault_fee_pct,
        referral_fee_config: ReferralFeeConfig::V1 {
            referral_fee_enabled,
            referral_fee_pct,
            referred_fee_discount_pct,
            discount_eligibility_volume_threshold,
            referrer_eligibility_volume_threshold,
        },
    };
    builder_code_registry::set_global_max_fee(builder_registry, builder_max_fee);
    let GlobalState::V1 { fee_config: fc, .. } = global_state;
    *fc = fee_config;
    // EVENT: TradingFeeTierUpdatedEvent
    Ok(())
}

pub fn set_builder_max_fee(
    global_state: &mut GlobalState,
    builder_registry: &mut Registry,
    builder_max_fee: u64,
) {
    let GlobalState::V1 { fee_config, .. } = global_state;
    let TradingFeeConfiguration::V1 { builder_max_fee: bmf, .. } = fee_config;
    *bmf = builder_max_fee;
    builder_code_registry::set_global_max_fee(builder_registry, builder_max_fee);
    // EVENT: TradingFeeTierUpdatedEvent
}

fn compute_fee_from_basis_points(notional_value: u128, fee_basis_points: u64) -> u64 {
    ((notional_value * (fee_basis_points as u128)) / ((FEE_PRECISION * 100) as u128)) as u64
}

fn is_backstop_liquidator(global_state: &GlobalState, account_addr: [u8; 32]) -> bool {
    let GlobalState::V1 { backstop_liquidator_address, .. } = global_state;
    account_addr == *backstop_liquidator_address
}

pub fn get_maker_fee_for_notional(
    global_state: &mut GlobalState,
    builder_registry: &Registry,
    account_addr: [u8; 32],
    fee_tracking_addr: [u8; 32],
    balance_type: fee_distribution::CollateralBalanceType,
    notional_value: u128,
    builder_code: Option<BuilderCode>,
    now_seconds: u64,
) -> FeeDistribution {
    if is_backstop_liquidator(global_state, account_addr) {
        return fee_distribution::new_fee_distribution(balance_type, 0, None);
    }

    let (rebate, maker_fees, _, referral_fee_config) =
        get_maker_fees_and_config(global_state, fee_tracking_addr, now_seconds);

    if rebate != 0 {
        let rebate_amount = compute_fee_from_basis_points(notional_value, rebate);
        let builder_fee = get_builder_fee(builder_registry, account_addr, builder_code, notional_value);
        let brf = if builder_fee > 0 {
            let builder_addr = builder_code_registry::get_builder_from_builder_code(builder_code.as_ref().unwrap());
            Some(fee_distribution::new_fee_with_destination(builder_addr, builder_fee))
        } else {
            None
        };
        fee_distribution::new_fee_distribution(
            balance_type,
            (builder_fee as i64) - (rebate_amount as i64),
            brf,
        )
    } else {
        let base_fee = compute_fee_from_basis_points(notional_value, maker_fees);
        let builder_fee = get_builder_fee(builder_registry, account_addr, builder_code, notional_value);
        let (discounted_base_fee, brf) = if builder_fee > 0 {
            let builder_addr = builder_code_registry::get_builder_from_builder_code(builder_code.as_ref().unwrap());
            (base_fee, Some(fee_distribution::new_fee_with_destination(builder_addr, builder_fee)))
        } else {
            get_referral_fee(global_state, base_fee, fee_tracking_addr, &referral_fee_config, now_seconds)
        };
        fee_distribution::new_fee_distribution(
            balance_type,
            (discounted_base_fee + builder_fee) as i64,
            brf,
        )
    }
}

fn get_builder_fee(
    registry: &Registry,
    user_addr: [u8; 32],
    builder_code: Option<BuilderCode>,
    notional_value: u128,
) -> u64 {
    match builder_code {
        Some(code) => builder_code_registry::get_builder_fee_for_notional(registry, user_addr, code, notional_value),
        None => 0,
    }
}

fn get_referral_fee(
    global_state: &mut GlobalState,
    base_fee: u64,
    user_addr: [u8; 32],
    referral_fee_config: &ReferralFeeConfig,
    now_seconds: u64,
) -> (u64, Option<FeeWithDestination>) {
    let ReferralFeeConfig::V1 {
        referral_fee_enabled,
        referral_fee_pct,
        referred_fee_discount_pct,
        discount_eligibility_volume_threshold,
        ..
    } = referral_fee_config;

    if !referral_fee_enabled {
        return (base_fee, None);
    }

    let referrer_addr_opt = get_referrer_addr(global_state, user_addr);
    if referrer_addr_opt.is_none() {
        return (base_fee, None);
    }

    let user_volume = get_user_volume_all_time(global_state, user_addr, now_seconds);
    if user_volume >= *discount_eligibility_volume_threshold {
        return (base_fee, None);
    }

    let referrer_addr = referrer_addr_opt.unwrap();
    let discounted_base_fee = (base_fee * (100 - referred_fee_discount_pct)) / 100;
    let referral_fee = (discounted_base_fee * referral_fee_pct) / 100;
    (
        discounted_base_fee,
        Some(fee_distribution::new_fee_with_destination(referrer_addr, referral_fee)),
    )
}

pub fn get_taker_fee_for_notional(
    global_state: &mut GlobalState,
    builder_registry: &Registry,
    account_addr: [u8; 32],
    fee_tracking_addr: [u8; 32],
    balance_type: fee_distribution::CollateralBalanceType,
    notional_value: u128,
    builder_code: Option<BuilderCode>,
    now_seconds: u64,
) -> FeeDistribution {
    if is_backstop_liquidator(global_state, account_addr) {
        return fee_distribution::new_fee_distribution(balance_type, 0, None);
    }

    let (fee_pct, _, referral_fee_config) =
        get_taker_fees_and_config(global_state, fee_tracking_addr, now_seconds);
    let base_fee = compute_fee_from_basis_points(notional_value, fee_pct);
    let builder_fee = get_builder_fee(builder_registry, account_addr, builder_code, notional_value);
    let (discounted_base_fee, brf) = if builder_fee > 0 {
        let builder_addr = builder_code_registry::get_builder_from_builder_code(builder_code.as_ref().unwrap());
        (base_fee, Some(fee_distribution::new_fee_with_destination(builder_addr, builder_fee)))
    } else {
        get_referral_fee(global_state, base_fee, fee_tracking_addr, &referral_fee_config, now_seconds)
    };
    fee_distribution::new_fee_distribution(
        balance_type,
        (discounted_base_fee + builder_fee) as i64,
        brf,
    )
}

pub fn get_fees_for_margin_call(
    balance_type: fee_distribution::CollateralBalanceType,
    notional_value: u128,
    margin_call_fee_pct: u64,
    slippage_and_margin_call_fee_scale: u64,
) -> FeeDistribution {
    let scaled_fee_pct = ((margin_call_fee_pct as u128) * ((FEE_PRECISION * 100) as u128)
        / (slippage_and_margin_call_fee_scale as u128)) as u64;
    fee_distribution::new_margin_call_fee_distribution(
        balance_type,
        compute_fee_from_basis_points(notional_value, scaled_fee_pct),
    )
}

pub fn get_maker_fees_and_config(
    global_state: &mut GlobalState,
    user_addr: [u8; 32],
    now_seconds: u64,
) -> (u64, u64, u64, ReferralFeeConfig) {
    let GlobalState::V1 { volume_stats, fee_config, .. } = global_state;

    let total_volume = volume_tracker::get_total_volume_in_window(volume_stats, user_addr, now_seconds);

    // Clone config to avoid borrow conflicts
    let config_clone = fee_config.clone();
    let TradingFeeConfiguration::V1 {
        market_maker_absolute_threshold,
        backstop_vault_fee_pct,
        referral_fee_config,
        ..
    } = &config_clone;

    let market_maker_fee_rebate = if *market_maker_absolute_threshold != 0 {
        let maker_volume = volume_tracker::get_maker_volume_in_window(volume_stats, user_addr, now_seconds);
        let global_volume = volume_tracker::get_global_volume_in_window(volume_stats, now_seconds);
        get_market_maker_fee_rebate(&config_clone, maker_volume, global_volume)
    } else {
        0
    };

    if market_maker_fee_rebate != 0 {
        (market_maker_fee_rebate, 0, *backstop_vault_fee_pct, referral_fee_config.clone())
    } else {
        let maker_fee = get_maker_fees_for_volume(&config_clone, total_volume);
        (0, maker_fee, *backstop_vault_fee_pct, referral_fee_config.clone())
    }
}

pub fn get_taker_fees_and_config(
    global_state: &mut GlobalState,
    user_addr: [u8; 32],
    now_seconds: u64,
) -> (u64, u64, ReferralFeeConfig) {
    let GlobalState::V1 { volume_stats, fee_config, .. } = global_state;
    let taker_volume = volume_tracker::get_total_volume_in_window(volume_stats, user_addr, now_seconds);
    let config_clone = fee_config.clone();
    let TradingFeeConfiguration::V1 { backstop_vault_fee_pct, referral_fee_config, .. } = &config_clone;
    (
        get_taker_fees_for_volume(&config_clone, taker_volume),
        *backstop_vault_fee_pct,
        referral_fee_config.clone(),
    )
}

fn get_market_maker_fee_rebate(config: &TradingFeeConfiguration, maker_volume: u128, global_volume: u128) -> u64 {
    let TradingFeeConfiguration::V1 {
        market_maker_tier_fee_rebates,
        market_maker_absolute_threshold,
        market_maker_tier_pct_thresholds,
        ..
    } = config;

    if market_maker_tier_fee_rebates.is_empty() {
        return 0;
    }
    if maker_volume < *market_maker_absolute_threshold {
        return 0;
    }
    if global_volume == 0 {
        return 0;
    }

    let volume_pct = (maker_volume
        .checked_mul((MARKET_MAKER_TIER_PCT_PRECISION * 100) as u128)
        .unwrap_or(0)
        / global_volume) as u64;

    let mut tier = 0usize;
    while tier < market_maker_tier_pct_thresholds.len()
        && volume_pct >= market_maker_tier_pct_thresholds[tier]
    {
        tier += 1;
    }
    market_maker_tier_fee_rebates[tier]
}

fn get_maker_fees_for_volume(config: &TradingFeeConfiguration, volume: u128) -> u64 {
    let TradingFeeConfiguration::V1 { tier_thresholds, tier_maker_fees, .. } = config;
    let mut tier = 0usize;
    while tier < tier_thresholds.len() && volume >= tier_thresholds[tier] {
        tier += 1;
    }
    tier_maker_fees[tier]
}

fn get_taker_fees_for_volume(config: &TradingFeeConfiguration, volume: u128) -> u64 {
    let TradingFeeConfiguration::V1 { tier_thresholds, tier_taker_fees, .. } = config;
    let mut tier = 0usize;
    while tier < tier_thresholds.len() && volume >= tier_thresholds[tier] {
        tier += 1;
    }
    tier_taker_fees[tier]
}

pub fn track_volume(
    global_state: &mut GlobalState,
    maker_addr: [u8; 32],
    taker_addr: [u8; 32],
    volume: u128,
    now_seconds: u64,
) {
    let GlobalState::V1 { volume_stats, .. } = global_state;
    volume_tracker::track_volume(volume_stats, maker_addr, taker_addr, volume, now_seconds);
}

pub fn track_global_and_maker_volume(
    global_state: &mut GlobalState,
    maker_addr: [u8; 32],
    volume: u128,
    now_seconds: u64,
) {
    let GlobalState::V1 { volume_stats, .. } = global_state;
    volume_tracker::track_volume(volume_stats, maker_addr, [0u8; 32], volume, now_seconds);
}

pub fn track_taker_volume(
    global_state: &mut GlobalState,
    taker_addr: [u8; 32],
    volume: u128,
    now_seconds: u64,
) {
    let GlobalState::V1 { volume_stats, .. } = global_state;
    volume_tracker::track_taker_volume(volume_stats, taker_addr, volume, now_seconds);
}

pub fn register_referral_code(
    global_state: &mut GlobalState,
    user_addr: [u8; 32],
    referral_code: String,
) -> Result<(), u64> {
    let GlobalState::V1 { referrals, .. } = global_state;
    referral_registry::register_referral_code(referrals, user_addr, referral_code, false)
}

pub fn register_referrer(
    global_state: &mut GlobalState,
    user_addr: [u8; 32],
    referrer_code: String,
    now_seconds: u64,
) -> Result<(), u64> {
    let user_volume = get_user_volume_all_time(global_state, user_addr, now_seconds);
    let GlobalState::V1 { fee_config, referrals, .. } = global_state;
    let TradingFeeConfiguration::V1 { referral_fee_config, .. } = fee_config;
    let ReferralFeeConfig::V1 { referrer_eligibility_volume_threshold, .. } = referral_fee_config;
    if user_volume >= *referrer_eligibility_volume_threshold {
        return Err(EEXCEEDED_VOLUME_THRESHOLD);
    }
    referral_registry::register_referral(referrals, user_addr, referrer_code, false)
}

pub fn get_referral_codes(global_state: &GlobalState, user_addr: [u8; 32]) -> Vec<String> {
    let GlobalState::V1 { referrals, .. } = global_state;
    referral_registry::get_referral_codes(referrals, user_addr)
}

pub fn set_max_referral_codes_for_address(
    global_state: &mut GlobalState,
    user_addr: [u8; 32],
    max_usage: u64,
) {
    let GlobalState::V1 { referrals, .. } = global_state;
    referral_registry::set_max_referral_codes_for_address(referrals, user_addr, max_usage);
}

pub fn set_max_usage_per_referral_code_for_address(
    global_state: &mut GlobalState,
    user_addr: [u8; 32],
    max: u64,
) {
    let GlobalState::V1 { referrals, .. } = global_state;
    referral_registry::set_max_usage_per_referral_code_for_address(referrals, user_addr, max);
}

pub fn register_affiliate(global_state: &mut GlobalState, user_addr: [u8; 32]) {
    let GlobalState::V1 { referrals, .. } = global_state;
    referral_registry::register_affiliate(referrals, user_addr);
}

pub fn admin_register_referral_code(
    global_state: &mut GlobalState,
    user_addr: [u8; 32],
    referral_code: String,
) -> Result<(), u64> {
    let GlobalState::V1 { referrals, .. } = global_state;
    referral_registry::register_referral_code(referrals, user_addr, referral_code, true)
}

pub fn admin_register_referrer(
    global_state: &mut GlobalState,
    user_addr: [u8; 32],
    referrer_code: String,
) -> Result<(), u64> {
    let GlobalState::V1 { referrals, .. } = global_state;
    referral_registry::register_referral(referrals, user_addr, referrer_code, true)
}

pub fn get_referrer_addr(global_state: &GlobalState, user_addr: [u8; 32]) -> Option<[u8; 32]> {
    let GlobalState::V1 { referrals, .. } = global_state;
    referral_registry::get_referrer_addr(referrals, user_addr)
}

pub fn get_fee_tier(
    global_state: &mut GlobalState,
    user_addr: [u8; 32],
    now_seconds: u64,
) -> u64 {
    let GlobalState::V1 { volume_stats, fee_config, .. } = global_state;
    let maker_volume = volume_tracker::get_total_volume_in_window(volume_stats, user_addr, now_seconds);
    let TradingFeeConfiguration::V1 { tier_thresholds, .. } = fee_config;
    let mut tier = 0usize;
    while tier < tier_thresholds.len() && maker_volume >= tier_thresholds[tier] {
        tier += 1;
    }
    tier as u64
}

fn get_user_volume_all_time(
    global_state: &mut GlobalState,
    user_addr: [u8; 32],
    _now_seconds: u64,
) -> u128 {
    let GlobalState::V1 { volume_stats, .. } = global_state;
    volume_tracker::get_maker_volume_all_time(volume_stats, user_addr)
        + volume_tracker::get_taker_volume_all_time(volume_stats, user_addr)
}

// ============ View APIs for Volume History ============

pub fn view_global_volume_history(global_state: &GlobalState) -> VolumeHistoryView {
    let GlobalState::V1 { volume_stats, .. } = global_state;
    volume_tracker::get_global_volume_history_view(volume_stats)
}

pub fn view_maker_volume_history(
    global_state: &GlobalState,
    user_addr: [u8; 32],
) -> Option<VolumeHistoryView> {
    let GlobalState::V1 { volume_stats, .. } = global_state;
    volume_tracker::get_maker_volume_history_view(volume_stats, user_addr)
}

pub fn view_taker_volume_history(
    global_state: &GlobalState,
    user_addr: [u8; 32],
) -> Option<VolumeHistoryView> {
    let GlobalState::V1 { volume_stats, .. } = global_state;
    volume_tracker::get_taker_volume_history_view(volume_stats, user_addr)
}

// ============ View APIs for Referral Registry ============

pub fn view_referrer_state(
    global_state: &GlobalState,
    user_addr: [u8; 32],
) -> Option<ReferrerState> {
    let GlobalState::V1 { referrals, .. } = global_state;
    referral_registry::get_referrer_state(referrals, user_addr)
}

pub fn view_referral_code_state(
    global_state: &GlobalState,
    referral_code: &str,
) -> Option<ReferralCode> {
    let GlobalState::V1 { referrals, .. } = global_state;
    referral_registry::get_referral_code_state(referrals, referral_code)
}

pub fn view_referrer_for_code(
    global_state: &GlobalState,
    referral_code: &str,
) -> Option<[u8; 32]> {
    let GlobalState::V1 { referrals, .. } = global_state;
    referral_registry::get_referrer_for_code(referrals, referral_code)
}

pub fn view_trading_fee_config(global_state: &GlobalState) -> TradingFeeConfiguration {
    let GlobalState::V1 { fee_config, .. } = global_state;
    fee_config.clone()
}
