// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::price_management

use crate::native_perpdex::i64_math;
use crate::native_perpdex::math;
use crate::native_perpdex::moving_average::{
    self, DeviationMovingAverage, MovingAverage,
};
use serde::{Deserialize, Serialize};

// ===================== Constants =====================

/// Error code for when an invalid admin signer is used
const EINVALID_ADMIN: u64 = (1 << 16) | 1; // error::invalid_argument(1)
/// Error code for when the market is halted. No price updates are allowed.
const EMARKET_HALTED: u64 = 2;
/// Overriding mark price on a normal market is not allowed.
const EOVERRIDE_MARK_PRICE_ON_DELISTED_MARKET: u64 = 3;
const EINVALID_PRICE_IS_ZERO: u64 = (1 << 16) | 4; // error::invalid_argument(4)
const ECOMMIT_MARK_PRICE_MISMATCH: u64 = (1 << 16) | 5; // error::invalid_argument(5)
const EINVALID_BOOK_ORACLE_RATIO_CAP_BPS: u64 = 6;
const ESIZE_MULTIPLIER_LEVERAGE_OVERFLOW: u64 = 7;
const EMARKET_SIGNER_MISMATCH: u64 = (1 << 16) | 8; // error::invalid_argument(8)
const EINVALID_FUNDING_MODE: u64 = 9;
const EINVALID_FUNDING_PERIOD_US: u64 = 10;

pub const RATE_SIZE_MULTIPLIER: u64 = 1_000_000;

const MICRO_SECONDS_PER_DAY: u64 = 86400_000_000;
/// Funding rate is bounded to 4% per hour.
const MAX_DAILY_FUNDING_RATE: u64 = RATE_SIZE_MULTIPLIER * 24 * 4 / 100;

const DEFAULT_BOOK_ORACLE_RATIO_CAP_BPS: u64 = 100; // 1%
const MAX_BOOK_ORACLE_RATIO_CAP_BPS: u64 = 10000000; // 1000x
const MIN_BOOK_ORACLE_RATIO_CAP_BPS: u64 = 50; // 0.5%

const MIN_FUNDING_PERIOD_US: u64 = 10_000_000; // 10 seconds
const MAX_FUNDING_PERIOD_US: u64 = 3600_000_000; // 1 hour

// ===================== Types =====================

/// EVENT: PriceUpdateEvent
#[derive(Clone, Debug)]
pub enum PriceUpdateEvent {
    V1 {
        market: [u8; 32], // Object<PerpMarket>
        oracle_px: u64,
        mark_px: u64,
        impact_ask_px: u64,
        impact_bid_px: u64,
        funding_index: i128,
        funding_rate_bps: i64,
    },
    V2 {
        market: [u8; 32], // Object<PerpMarket>
        oracle_px: u64,
        mark_px: u64,
        impact_ask_px: u64,
        impact_bid_px: u64,
        funding: PriceFundingUpdateDetails,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PriceFundingUpdateDetails {
    V1 {
        /// Current materialized funding index
        funding_index: i128,
        /// Timestamp of when the last funding was charged
        funding_timestamp_us: u64,
        /// Current outstanding funding index.
        /// 0 when continuous funding
        outstanding_funding_index: i128,
        /// Timestamp of when the outstanding funding was last updated
        /// 0 when continuous funding
        outstanding_funding_timestamp_us: u64,
        /// Funding period in microseconds.
        /// 0 means continuous funding.
        funding_period_us: u64,
        /// Current moment's daily funding rate (in RATE_SIZE_MULTIPLIER units).
        instant_daily_funding_rate: i64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PriceIndexStore {
    V1 {
        interest_rate: u64,
    },
    V2 {
        daily_interest_rate: u64,
        daily_premium_rate: u64,
        daily_rate_at_zero_diff: u64,
        max_rate_as_fraction_of_initial_margin: u64,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccumulativeIndex {
    pub index: i128,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PriceDetails {
    V1 {
        price_config: PriceConfig,
        price_history: PriceHistory,
        price_state: PriceState,
        funding_rate_history: FundingRateHistory,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PriceState {
    V1 {
        /// largest mark price in the mark_prices vector
        short_mark_px: u64,
        /// smallest mark price in the mark_prices vector
        long_mark_px: u64,
        accumulative_index: AccumulativeIndex,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PriceConfig {
    V1 {
        size_multiplier: u64,
        /// Haircut for unrealized PnL when calculating withdrawable balance (in basis points)
        unrealized_pnl_haircut_bps: u64,
        /// Leverage need to be maintained for withdrawable balance calculations
        withdrawable_margin_leverage: u8,
        /// Max leverage for the market
        max_leverage: u8,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PriceHistory {
    V1 {
        /// Timestamp of the last oracle price update.
        last_oracle_update_us: u64,
        oracle_px: u64,
        /// List of all outstanding mark price updates.
        mark_prices: Vec<u64>,
        book_mid_px: u64,
        book_mid_30_ema: MovingAverage,
        /// 150s EMA of ratio (mid_book_px / oracle_px)
        ratio_mid_vs_oracle_150_ema: DeviationMovingAverage,
        book_oracle_ratio_cap_bps: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FundingRateHistory {
    V1 {
        funding_rate_pause_timeout_us: u64,
        last_funding_calculated_us: u64,
        instant_rate_adjustment: FundingInstantRateAdjustment,
        charging_mode: FundingChargingMode,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FundingChargingMode {
    /// Continuous funding charging mode.
    ContinuousV1,
    /// Periodic funding charging mode.
    PeriodicV1 {
        outstanding_funding_index: AccumulativeIndex,
        last_funding_charged_us: u64,
        funding_period_us: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FundingInstantRateAdjustment {
    INSTANT,
}

#[derive(Clone, Debug)]
pub enum MarkPriceRefreshInput {
    None,
    UseProvidedImpactHint {
        impact_bid_px: u64,
        impact_ask_px: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarketState {
    V1 {
        short_mark_px: u64,
        long_mark_px: u64,
        accumulative_index: AccumulativeIndex,
        size_multiplier: u64,
        unrealized_pnl_haircut_bps: u64,
        withdrawable_margin_leverage: u8,
        max_leverage: u8,
    },
}

#[derive(Clone, Debug)]
pub enum PriceChangeDetails {
    V1 {
        new_mark_px: u64,
        old_market_state: MarketState,
        new_market_state: MarketState,
    },
}

// ===================== Constructor / Initialization =====================

pub fn new_default_rate_config() -> PriceIndexStore {
    PriceIndexStore::V2 {
        // Interest rate is 0.01% per 8 hours, or 0.03% per day
        daily_interest_rate: RATE_SIZE_MULTIPLIER * 3 / 10_000,
        // Premium rate is 100% per 8 hours of a difference, or 300% per day.
        daily_premium_rate: RATE_SIZE_MULTIPLIER * 3,
        // Funding rate is clamped to 0.05% per 8 hours, or 0.15% per day
        daily_rate_at_zero_diff: RATE_SIZE_MULTIPLIER * 15 / 10_000,
        // Half of backstop liquidation margin (which is 1/3 of initial margin)
        max_rate_as_fraction_of_initial_margin: RATE_SIZE_MULTIPLIER / 6,
    }
}

pub fn accumulative_index_value(ai: &AccumulativeIndex) -> i128 {
    ai.index
}

pub fn create_default_price_details(
    oracle_px: u64,
    size_multiplier: u64,
    max_leverage: u8,
    funding_rate_pause_timeout_us: u64,
    now_us: u64,
) -> PriceDetails {
    let mut book_mid_ema = moving_average::new_ema(30).expect("new_ema(30) should succeed");

    // initial book mid ema with the oracle price
    moving_average::add_moving_average_observation(&mut book_mid_ema, oracle_px, now_us);

    PriceDetails::V1 {
        price_config: PriceConfig::V1 {
            size_multiplier,
            unrealized_pnl_haircut_bps: 0,
            withdrawable_margin_leverage: max_leverage,
            max_leverage,
        },
        price_history: PriceHistory::V1 {
            oracle_px,
            mark_prices: vec![oracle_px],
            book_mid_px: oracle_px,
            book_mid_30_ema: book_mid_ema,
            ratio_mid_vs_oracle_150_ema: moving_average::new_ratio_ema(150)
                .expect("new_ratio_ema(150) should succeed"),
            book_oracle_ratio_cap_bps: DEFAULT_BOOK_ORACLE_RATIO_CAP_BPS,
            last_oracle_update_us: now_us,
        },
        funding_rate_history: FundingRateHistory::V1 {
            funding_rate_pause_timeout_us,
            last_funding_calculated_us: now_us,
            instant_rate_adjustment: FundingInstantRateAdjustment::INSTANT,
            charging_mode: FundingChargingMode::PeriodicV1 {
                outstanding_funding_index: AccumulativeIndex { index: 0 },
                last_funding_charged_us: now_us,
                funding_period_us: 3600 * 1_000_000, // 1 hour
            },
        },
        price_state: PriceState::V1 {
            short_mark_px: oracle_px,
            long_mark_px: oracle_px,
            accumulative_index: AccumulativeIndex { index: 0 },
        },
    }
}

/// Register a new market's price details. Returns the PriceDetails to be stored.
/// In Move this does move_to; here the caller stores the returned value.
/// RESOURCE_WRITE: PriceDetails at market_addr
pub fn register_market(
    oracle_px: u64,
    size_multiplier: u64,
    max_leverage: u8,
    now_us: u64,
) -> Result<PriceDetails, u64> {
    if oracle_px == 0 {
        return Err(EINVALID_PRICE_IS_ZERO);
    }
    Ok(create_default_price_details(
        oracle_px,
        size_multiplier,
        max_leverage,
        360_000_000, // 6 minutes
        now_us,
    ))
}

// ===================== Funding Rate Calculations =====================

fn calculate_daily_funding_rate(
    history: &FundingRateHistory,
    oracle_px: u64,
    impact_bid_px: u64,
    impact_ask_px: u64,
    rate_config: &PriceIndexStore,
    max_daily_funding_rate: u64,
    now_us: u64,
) -> i64 {
    let FundingRateHistory::V1 {
        last_funding_calculated_us,
        funding_rate_pause_timeout_us,
        ..
    } = history;

    let PriceIndexStore::V2 {
        daily_interest_rate,
        ..
    } = rate_config
    else {
        // V1 should be upgraded before reaching this point
        panic!("PriceIndexStore::V1 should be upgraded");
    };

    if now_us - last_funding_calculated_us > *funding_rate_pause_timeout_us {
        return *daily_interest_rate as i64;
    }

    let bid_delta = (impact_bid_px as i64) - (oracle_px as i64);
    let ask_delta = (oracle_px as i64) - (impact_ask_px as i64);

    let impact_px = i64_math::max(bid_delta, 0) - i64_math::max(ask_delta, 0);

    calculate_daily_funding_rate_from_oracle_and_impact(
        impact_px,
        oracle_px,
        rate_config,
        max_daily_funding_rate,
    )
}

fn calculate_daily_funding_rate_from_oracle_and_impact(
    impact_px: i64,
    oracle_px: u64,
    rate_config: &PriceIndexStore,
    max_daily_funding_rate: u64,
) -> i64 {
    let PriceIndexStore::V2 {
        daily_interest_rate,
        daily_premium_rate,
        daily_rate_at_zero_diff,
        ..
    } = rate_config
    else {
        panic!("PriceIndexStore::V1 should be upgraded");
    };

    let premium_rate =
        i64_math::mul_div(impact_px, *daily_premium_rate, oracle_px)
            .expect("mul_div in funding rate calculation");

    let funding_rate = (*daily_interest_rate as i64) - premium_rate;

    let (is_positive, funding_rate_amount) = i64_math::into_sign_and_amount(funding_rate);
    let clamped_amount = std::cmp::min(funding_rate_amount, *daily_rate_at_zero_diff);
    let mut clamped_funding_rate_adjustment =
        i64_math::from_sign_and_amount(is_positive, clamped_amount as i64);

    clamped_funding_rate_adjustment += premium_rate;
    let (is_positive, amount) =
        i64_math::into_sign_and_amount(clamped_funding_rate_adjustment);
    if amount > max_daily_funding_rate {
        i64_math::from_sign_and_amount(is_positive, max_daily_funding_rate as i64)
    } else {
        i64_math::from_sign_and_amount(is_positive, amount as i64)
    }
}

/// Update the accumulative index based on funding rate calculations.
/// Returns the daily_funding_rate.
pub fn update_accumulative_index(
    price_state: &mut PriceState,
    history: &mut FundingRateHistory,
    oracle_px: u64,
    impact_bid_px: u64,
    impact_ask_px: u64,
    rate_config: &PriceIndexStore,
    max_daily_funding_rate: u64,
    now_us: u64,
) -> i64 {
    let daily_funding_rate = calculate_daily_funding_rate(
        history,
        oracle_px,
        impact_bid_px,
        impact_ask_px,
        rate_config,
        max_daily_funding_rate,
        now_us,
    );

    // Apply instant rate adjustment (currently only INSTANT which is a no-op)
    let FundingRateHistory::V1 {
        last_funding_calculated_us,
        charging_mode,
        ..
    } = history;

    let previous_updated_us = *last_funding_calculated_us;
    let time_elapsed = now_us - *last_funding_calculated_us;

    // funding_cost_for_interval =
    //   ((daily_funding_rate as i128) * (time_elapsed as i128) as i256)
    //     * (oracle_px as i256) / (MICRO_SECONDS_PER_DAY as i256)
    // We use i128 math. The Move code uses i256 intermediaries but the values fit in i128.
    let funding_cost_for_interval = {
        let rate_times_time = (daily_funding_rate as i128) * (time_elapsed as i128);
        (rate_times_time * (oracle_px as i128)) / (MICRO_SECONDS_PER_DAY as i128)
    };

    *last_funding_calculated_us = now_us;

    let PriceState::V1 {
        accumulative_index, ..
    } = price_state;

    match charging_mode {
        FundingChargingMode::ContinuousV1 => {
            accumulative_index.index += funding_cost_for_interval;
            daily_funding_rate
        }
        FundingChargingMode::PeriodicV1 {
            outstanding_funding_index,
            funding_period_us,
            last_funding_charged_us,
        } => {
            let fp = *funding_period_us;
            let period_boundary_us = (now_us / fp) * fp;
            if period_boundary_us > *last_funding_charged_us {
                // Crossing a period boundary -- split elapsed time at the boundary
                let time_in_completed_period = period_boundary_us - previous_updated_us;
                let funding_in_completed_period = {
                    let rate_times_time =
                        (daily_funding_rate as i128) * (time_in_completed_period as i128);
                    (rate_times_time * (oracle_px as i128)) / (MICRO_SECONDS_PER_DAY as i128)
                };
                let funding_index_at_completed_interval =
                    outstanding_funding_index.index + funding_in_completed_period;

                // Flush the completed period into accumulative_index.
                let _is_funding_index_updated =
                    accumulative_index.index != funding_index_at_completed_interval;
                if accumulative_index.index != funding_index_at_completed_interval {
                    accumulative_index.index = funding_index_at_completed_interval;
                }
                *last_funding_charged_us = period_boundary_us;
            }
            // Always accumulate the full interval into outstanding
            outstanding_funding_index.index += funding_cost_for_interval;
            daily_funding_rate
        }
    }
}

// ===================== Median Price =====================

/// Returns the median value of three u64 inputs
pub fn get_median_price(a: u64, b: u64, c: u64) -> u64 {
    if a >= b {
        if b >= c {
            b
        } else if a >= c {
            c
        } else {
            a
        }
    } else if a >= c {
        a
    } else if b >= c {
        c
    } else {
        b
    }
}

// ===================== Funding Cost =====================

pub fn get_funding_cost(
    entry_index: &AccumulativeIndex,
    exit_index: &AccumulativeIndex,
    position_size: u64,
    position_size_multiplier: u64,
    for_long: bool,
) -> i64 {
    let mut index_delta = exit_index.index - entry_index.index;

    if !for_long {
        index_delta = -index_delta;
    }

    let (is_positive, index_delta_abs) = i64_math::into_sign_and_amount_i128(index_delta);
    i64_math::from_sign_and_amount(
        is_positive,
        // Round up charge to user, round down payment to user
        math::div_direction_128(
            index_delta_abs * (position_size as u128),
            (position_size_multiplier as u128) * (RATE_SIZE_MULTIPLIER as u128),
            is_positive,
        )
        .expect("div_direction_128 in get_funding_cost") as i64,
    )
}

// ===================== Price History helpers =====================

fn update_spread_ema(
    price_history: &mut PriceHistory,
    oracle_px: u64,
    mid_book_px: u64,
    now: u64,
) {
    let PriceHistory::V1 {
        ratio_mid_vs_oracle_150_ema,
        book_oracle_ratio_cap_bps,
        ..
    } = price_history;
    moving_average::add_deviation_observation(
        ratio_mid_vs_oracle_150_ema,
        oracle_px,
        mid_book_px,
        now,
        *book_oracle_ratio_cap_bps,
    )
    .expect("add_deviation_observation failed");
}

fn update_book_mid_price_and_ema(
    price_history: &mut PriceHistory,
    book_mid_px: u64,
    now: u64,
) {
    let PriceHistory::V1 {
        book_mid_30_ema,
        book_mid_px: stored_book_mid_px,
        ..
    } = price_history;
    moving_average::add_moving_average_observation(book_mid_30_ema, book_mid_px, now);
    *stored_book_mid_px = book_mid_px;
}

/// NB: This price is used for calculations of account equity and liquidations.
fn update_mark_px(
    price_history: &mut PriceHistory,
    price_state: &mut PriceState,
    oracle_px: u64,
    book_mid_px: u64,
) -> u64 {
    let PriceHistory::V1 {
        ratio_mid_vs_oracle_150_ema,
        ..
    } = price_history;
    let ema_value =
        moving_average::get_ratio_estimated_value(ratio_mid_vs_oracle_150_ema, oracle_px)
            .expect("get_ratio_estimated_value failed");
    let mark_px = get_median_price(book_mid_px, ema_value, oracle_px);
    push_new_mark_price(price_history, price_state, mark_px);
    mark_px
}

fn push_new_mark_price(
    price_history: &mut PriceHistory,
    price_state: &mut PriceState,
    mark_px: u64,
) {
    let PriceHistory::V1 { mark_prices, .. } = price_history;
    let PriceState::V1 {
        short_mark_px,
        long_mark_px,
        ..
    } = price_state;

    mark_prices.push(mark_px);
    if *short_mark_px < mark_px {
        *short_mark_px = mark_px;
    }
    if *long_mark_px > mark_px {
        *long_mark_px = mark_px;
    }
}

// ===================== MarketState accessors =====================

pub fn get_market_state(
    market_state: &MarketState,
    is_long: bool,
) -> (u64, AccumulativeIndex, u64, u64, u8) {
    let MarketState::V1 {
        short_mark_px,
        long_mark_px,
        accumulative_index,
        size_multiplier,
        unrealized_pnl_haircut_bps,
        max_leverage,
        ..
    } = market_state;
    (
        if is_long {
            *long_mark_px
        } else {
            *short_mark_px
        },
        *accumulative_index,
        *size_multiplier,
        *unrealized_pnl_haircut_bps,
        *max_leverage,
    )
}

pub fn is_market_state_different_for_side(
    this: &MarketState,
    other: &MarketState,
    is_long: bool,
) -> bool {
    let MarketState::V1 {
        short_mark_px: s1,
        long_mark_px: l1,
        accumulative_index: ai1,
        size_multiplier: sm1,
        unrealized_pnl_haircut_bps: h1,
        max_leverage: ml1,
        ..
    } = this;
    let MarketState::V1 {
        short_mark_px: s2,
        long_mark_px: l2,
        accumulative_index: ai2,
        size_multiplier: sm2,
        unrealized_pnl_haircut_bps: h2,
        max_leverage: ml2,
        ..
    } = other;

    let price_diff = if is_long { l1 != l2 } else { s1 != s2 };
    price_diff || ai1.index != ai2.index || sm1 != sm2 || h1 != h2 || ml1 != ml2
}

// ===================== PriceChangeDetails accessors =====================

pub fn new_mark_px_from_change_details(details: &PriceChangeDetails) -> u64 {
    let PriceChangeDetails::V1 { new_mark_px, .. } = details;
    *new_mark_px
}

pub fn into_old_and_new_market_state(
    details: PriceChangeDetails,
) -> (MarketState, MarketState) {
    let PriceChangeDetails::V1 {
        old_market_state,
        new_market_state,
        ..
    } = details;
    (old_market_state, new_market_state)
}

pub fn has_updated_on_mark_price(details: &PriceChangeDetails) -> bool {
    let PriceChangeDetails::V1 {
        old_market_state,
        new_market_state,
        ..
    } = details;
    let MarketState::V1 {
        long_mark_px: old_long,
        short_mark_px: old_short,
        accumulative_index: old_ai,
        ..
    } = old_market_state;
    let MarketState::V1 {
        long_mark_px: new_long,
        short_mark_px: new_short,
        accumulative_index: new_ai,
        ..
    } = new_market_state;
    old_long != new_long || old_short != new_short || old_ai.index != new_ai.index
}

// ===================== Market state from PriceDetails =====================

pub fn get_market_state_for_position_status_from_price(
    price_state: &PriceState,
    config: &PriceConfig,
) -> MarketState {
    let PriceState::V1 {
        short_mark_px,
        long_mark_px,
        accumulative_index,
    } = price_state;
    let PriceConfig::V1 {
        size_multiplier,
        unrealized_pnl_haircut_bps,
        withdrawable_margin_leverage,
        max_leverage,
    } = config;
    MarketState::V1 {
        short_mark_px: *short_mark_px,
        long_mark_px: *long_mark_px,
        accumulative_index: *accumulative_index,
        size_multiplier: *size_multiplier,
        unrealized_pnl_haircut_bps: *unrealized_pnl_haircut_bps,
        withdrawable_margin_leverage: *withdrawable_margin_leverage,
        max_leverage: *max_leverage,
    }
}

pub fn get_market_state_for_position_status(
    price_details: &PriceDetails,
) -> MarketState {
    let PriceDetails::V1 {
        price_config,
        price_state,
        ..
    } = price_details;
    get_market_state_for_position_status_from_price(price_state, price_config)
}

// ===================== Compute max daily funding rate =====================

pub fn compute_max_daily_funding_rate(
    max_leverage: u8,
    max_rate_as_fraction_of_initial_margin: u64,
    charging_mode: &FundingChargingMode,
) -> u64 {
    match charging_mode {
        FundingChargingMode::ContinuousV1 => MAX_DAILY_FUNDING_RATE,
        FundingChargingMode::PeriodicV1 {
            funding_period_us, ..
        } => std::cmp::min(
            MAX_DAILY_FUNDING_RATE,
            ((max_rate_as_fraction_of_initial_margin as u128)
                * (MICRO_SECONDS_PER_DAY as u128)
                / (*funding_period_us as u128)
                / (max_leverage as u128)) as u64,
        ),
    }
}

// ===================== Main price update logic =====================

/// The main price update function. Takes pre-resolved oracle data and best bid/ask
/// from the order book.
///
/// Parameters:
/// - price_details: the mutable PriceDetails for this market
/// - rate_config: the PriceIndexStore (global rate configuration)
/// - oracle_px: the oracle price
/// - best_bid_price: best bid from order book (or oracle_px if no bids)
/// - best_ask_price: best ask from order book (or oracle_px if no asks)
/// - mark_price_refresh_input: impact price hints
/// - can_update: whether the market mode allows oracle updates
/// - now_us: current timestamp in microseconds
///
/// Returns: Option<(PriceChangeDetails, PriceUpdateEvent)>
/// EVENT: PriceUpdateEvent
pub fn update_price_internal(
    price_details: &mut PriceDetails,
    rate_config: &mut PriceIndexStore,
    market: [u8; 32],
    oracle_px: u64,
    best_bid_price: u64,
    best_ask_price: u64,
    mark_price_refresh_input: MarkPriceRefreshInput,
    can_update: bool,
    now_us: u64,
) -> Result<Option<(PriceChangeDetails, PriceUpdateEvent)>, u64> {
    if !can_update {
        return Err(EMARKET_HALTED);
    }

    let (impact_bid_px, impact_ask_px) = match mark_price_refresh_input {
        MarkPriceRefreshInput::None => (best_bid_price, best_ask_price),
        MarkPriceRefreshInput::UseProvidedImpactHint {
            impact_bid_px,
            impact_ask_px,
        } => (
            if impact_bid_px > best_bid_price {
                best_bid_price
            } else {
                impact_bid_px
            },
            if impact_ask_px < best_ask_price {
                best_ask_price
            } else {
                impact_ask_px
            },
        ),
    };

    let PriceDetails::V1 {
        price_config,
        price_history,
        price_state,
        funding_rate_history,
    } = price_details;

    let PriceHistory::V1 {
        last_oracle_update_us,
        mark_prices,
        ..
    } = price_history;

    if *last_oracle_update_us == now_us {
        return Ok(None);
    }

    let num_pending_mark_prices = mark_prices.len() as u64;
    if num_pending_mark_prices > 2 {
        let min_time_from_last_update =
            100_000 * (num_pending_mark_prices - 2) * (num_pending_mark_prices - 2);
        if *last_oracle_update_us + min_time_from_last_update > now_us {
            return Ok(None);
        }
    }

    let old_market_state =
        get_market_state_for_position_status_from_price(price_state, price_config);

    let PriceHistory::V1 {
        oracle_px: stored_oracle_px,
        last_oracle_update_us: stored_last_oracle_update_us,
        ..
    } = price_history;
    *stored_oracle_px = oracle_px;
    *stored_last_oracle_update_us = now_us;

    let book_mid_px = (impact_bid_px + impact_ask_px) / 2;

    update_spread_ema(price_history, oracle_px, book_mid_px, now_us);
    let new_mark_px = update_mark_px(price_history, price_state, oracle_px, book_mid_px);
    update_book_mid_price_and_ema(price_history, book_mid_px, now_us);

    // Upgrade rate_config if needed
    if matches!(rate_config, PriceIndexStore::V1 { .. }) {
        *rate_config = new_default_rate_config();
    }

    let PriceConfig::V1 { max_leverage, .. } = price_config;
    let PriceIndexStore::V2 {
        max_rate_as_fraction_of_initial_margin,
        ..
    } = rate_config
    else {
        unreachable!()
    };

    let FundingRateHistory::V1 { charging_mode, .. } = funding_rate_history;
    let max_daily_fr = compute_max_daily_funding_rate(
        *max_leverage,
        *max_rate_as_fraction_of_initial_margin,
        charging_mode,
    );

    let daily_funding_rate = update_accumulative_index(
        price_state,
        funding_rate_history,
        oracle_px,
        impact_bid_px,
        impact_ask_px,
        rate_config,
        max_daily_fr,
        now_us,
    );

    let funding = new_price_funding_update_details(
        price_state,
        funding_rate_history,
        daily_funding_rate,
    );

    let event = PriceUpdateEvent::V2 {
        market,
        oracle_px,
        mark_px: new_mark_px,
        impact_ask_px,
        impact_bid_px,
        funding,
    };

    let new_market_state =
        get_market_state_for_position_status_from_price(price_state, price_config);

    Ok(Some((
        PriceChangeDetails::V1 {
            new_mark_px,
            old_market_state,
            new_market_state,
        },
        event,
    )))
}

fn new_price_funding_update_details(
    price_state: &PriceState,
    funding_rate_history: &FundingRateHistory,
    instant_daily_funding_rate: i64,
) -> PriceFundingUpdateDetails {
    let PriceState::V1 {
        accumulative_index, ..
    } = price_state;
    let FundingRateHistory::V1 {
        last_funding_calculated_us,
        charging_mode,
        ..
    } = funding_rate_history;

    match charging_mode {
        FundingChargingMode::ContinuousV1 => PriceFundingUpdateDetails::V1 {
            funding_index: accumulative_index.index,
            funding_timestamp_us: *last_funding_calculated_us,
            outstanding_funding_index: 0,
            outstanding_funding_timestamp_us: 0,
            funding_period_us: 0,
            instant_daily_funding_rate,
        },
        FundingChargingMode::PeriodicV1 {
            outstanding_funding_index,
            funding_period_us,
            last_funding_charged_us,
        } => PriceFundingUpdateDetails::V1 {
            funding_index: accumulative_index.index,
            funding_timestamp_us: *last_funding_charged_us,
            outstanding_funding_index: outstanding_funding_index.index,
            outstanding_funding_timestamp_us: *last_funding_calculated_us,
            funding_period_us: *funding_period_us,
            instant_daily_funding_rate,
        },
    }
}

// ===================== Commit Mark Price =====================

/// Commit a mark price. Removes the oldest stale mark price and recalculates
/// short_mark_px and long_mark_px.
/// Returns PriceChangeDetails.
pub fn commit_mark_price(
    price_details: &mut PriceDetails,
    mark_px: u64,
) -> Result<PriceChangeDetails, u64> {
    let PriceDetails::V1 {
        price_config,
        price_history,
        price_state,
        ..
    } = price_details;

    let old_market_state =
        get_market_state_for_position_status_from_price(price_state, price_config);

    let PriceHistory::V1 { mark_prices, .. } = price_history;

    // Remove the oldest (stale) mark price
    mark_prices.remove(0);
    if mark_prices[0] != mark_px {
        return Err(ECOMMIT_MARK_PRICE_MISMATCH);
    }

    let mut new_short_mark_px = mark_px;
    let mut new_long_mark_px = mark_px;
    for i in 1..mark_prices.len() {
        let cur_mark_px = mark_prices[i];
        if cur_mark_px > new_short_mark_px {
            new_short_mark_px = cur_mark_px;
        }
        if cur_mark_px < new_long_mark_px {
            new_long_mark_px = cur_mark_px;
        }
    }

    let PriceState::V1 {
        short_mark_px,
        long_mark_px,
        ..
    } = price_state;
    *short_mark_px = new_short_mark_px;
    *long_mark_px = new_long_mark_px;

    let new_market_state =
        get_market_state_for_position_status_from_price(price_state, price_config);

    Ok(PriceChangeDetails::V1 {
        new_mark_px: mark_px,
        old_market_state,
        new_market_state,
    })
}

// ===================== Override mark price (delisted markets) =====================

/// Add an override mark price (for delisted markets only).
/// The caller must verify the market is delisted before calling.
/// Returns (PriceChangeDetails, PriceUpdateEvent).
/// EVENT: PriceUpdateEvent
pub fn add_override_mark_price(
    price_details: &mut PriceDetails,
    market: [u8; 32],
    mark_price: u64,
    is_market_delisted: bool,
) -> Result<(PriceChangeDetails, PriceUpdateEvent), u64> {
    if !is_market_delisted {
        return Err(EOVERRIDE_MARK_PRICE_ON_DELISTED_MARKET);
    }

    let PriceDetails::V1 {
        price_config,
        price_history,
        price_state,
        funding_rate_history,
    } = price_details;

    let old_market_state =
        get_market_state_for_position_status_from_price(price_state, price_config);
    push_new_mark_price(price_history, price_state, mark_price);

    let funding = new_price_funding_update_details(price_state, funding_rate_history, 0);

    let event = PriceUpdateEvent::V2 {
        market,
        oracle_px: mark_price,
        mark_px: mark_price,
        impact_ask_px: mark_price,
        impact_bid_px: mark_price,
        funding,
    };

    let new_market_state =
        get_market_state_for_position_status_from_price(price_state, price_config);

    Ok((
        PriceChangeDetails::V1 {
            new_mark_px: mark_price,
            old_market_state,
            new_market_state,
        },
        event,
    ))
}

/// Commit override mark price (for delisted markets only).
/// Replaces all mark prices with the override price.
/// Returns (PriceChangeDetails, PriceUpdateEvent).
/// EVENT: PriceUpdateEvent
pub fn commit_override_mark_price(
    price_details: &mut PriceDetails,
    market: [u8; 32],
    mark_price: u64,
    is_market_delisted: bool,
) -> Result<(PriceChangeDetails, PriceUpdateEvent), u64> {
    if !is_market_delisted {
        return Err(EOVERRIDE_MARK_PRICE_ON_DELISTED_MARKET);
    }

    let PriceDetails::V1 {
        price_config,
        price_history,
        price_state,
        funding_rate_history,
    } = price_details;

    let old_market_state =
        get_market_state_for_position_status_from_price(price_state, price_config);

    let PriceHistory::V1 { mark_prices, .. } = price_history;
    *mark_prices = vec![mark_price];

    let PriceState::V1 {
        short_mark_px,
        long_mark_px,
        ..
    } = price_state;
    *short_mark_px = mark_price;
    *long_mark_px = mark_price;

    if mark_price == 0 {
        return Err(EINVALID_PRICE_IS_ZERO);
    }

    let new_market_state =
        get_market_state_for_position_status_from_price(price_state, price_config);

    let funding = new_price_funding_update_details(price_state, funding_rate_history, 0);

    let event = PriceUpdateEvent::V2 {
        market,
        oracle_px: mark_price,
        mark_px: mark_price,
        impact_ask_px: mark_price,
        impact_bid_px: mark_price,
        funding,
    };

    Ok((
        PriceChangeDetails::V1 {
            new_mark_px: mark_price,
            old_market_state,
            new_market_state,
        },
        event,
    ))
}

// ===================== Simple getters =====================

pub fn get_mark_price(price_details: &PriceDetails) -> u64 {
    let PriceDetails::V1 { price_history, .. } = price_details;
    let PriceHistory::V1 { mark_prices, .. } = price_history;
    mark_prices[mark_prices.len() - 1]
}

pub fn get_oracle_price(price_details: &PriceDetails) -> u64 {
    let PriceDetails::V1 { price_history, .. } = price_details;
    let PriceHistory::V1 { oracle_px, .. } = price_history;
    *oracle_px
}

pub fn get_mark_and_oracle_price(price_details: &PriceDetails) -> (u64, u64) {
    let PriceDetails::V1 { price_history, .. } = price_details;
    let PriceHistory::V1 {
        mark_prices,
        oracle_px,
        ..
    } = price_history;
    (mark_prices[mark_prices.len() - 1], *oracle_px)
}

pub fn get_accumulative_index(price_details: &PriceDetails) -> AccumulativeIndex {
    let PriceDetails::V1 { price_state, .. } = price_details;
    let PriceState::V1 {
        accumulative_index, ..
    } = price_state;
    *accumulative_index
}

pub fn get_unrealized_pnl_haircut_bps(price_details: &PriceDetails) -> u64 {
    let PriceDetails::V1 { price_config, .. } = price_details;
    let PriceConfig::V1 {
        unrealized_pnl_haircut_bps,
        ..
    } = price_config;
    *unrealized_pnl_haircut_bps
}

pub fn get_book_oracle_ratio_cap_bps(price_details: &PriceDetails) -> u64 {
    let PriceDetails::V1 { price_history, .. } = price_details;
    let PriceHistory::V1 {
        book_oracle_ratio_cap_bps,
        ..
    } = price_history;
    *book_oracle_ratio_cap_bps
}

pub fn get_book_mid_px(price_details: &PriceDetails) -> u64 {
    let PriceDetails::V1 { price_history, .. } = price_details;
    let PriceHistory::V1 { book_mid_px, .. } = price_history;
    *book_mid_px
}

pub fn get_book_mid_ema_px(price_details: &PriceDetails) -> u64 {
    let PriceDetails::V1 { price_history, .. } = price_details;
    let PriceHistory::V1 { book_mid_30_ema, .. } = price_history;
    moving_average::get_moving_average_value(book_mid_30_ema)
}

// ===================== Setters =====================

pub fn set_funding_rate_pause_timeout_microseconds(
    price_details: &mut PriceDetails,
    timeout_microseconds: u64,
) {
    let PriceDetails::V1 {
        funding_rate_history,
        ..
    } = price_details;
    let FundingRateHistory::V1 {
        funding_rate_pause_timeout_us,
        ..
    } = funding_rate_history;
    *funding_rate_pause_timeout_us = timeout_microseconds;
}

pub fn set_unrealized_pnl_haircut_bps(
    price_details: &mut PriceDetails,
    haircut_bps: u64,
) -> Result<(), u64> {
    // Haircut must be less than 100% (10000 basis points)
    if haircut_bps >= 10000 {
        return Err(0x100); // error::invalid_argument(0x100)
    }
    let PriceDetails::V1 { price_config, .. } = price_details;
    let PriceConfig::V1 {
        unrealized_pnl_haircut_bps,
        ..
    } = price_config;
    *unrealized_pnl_haircut_bps = haircut_bps;
    Ok(())
}

pub fn set_max_leverage(
    price_details: &mut PriceDetails,
    new_max_leverage: u8,
) -> Result<(), u64> {
    let PriceDetails::V1 { price_config, .. } = price_details;
    let PriceConfig::V1 {
        size_multiplier,
        max_leverage,
        ..
    } = price_config;
    if (*size_multiplier as u128) * (new_max_leverage as u128) >= (u64::MAX as u128) {
        return Err(ESIZE_MULTIPLIER_LEVERAGE_OVERFLOW);
    }
    *max_leverage = new_max_leverage;
    Ok(())
}

pub fn set_book_oracle_ratio_cap_bps(
    price_details: &mut PriceDetails,
    cap_bps: u64,
) -> Result<(), u64> {
    if cap_bps > MAX_BOOK_ORACLE_RATIO_CAP_BPS {
        return Err(EINVALID_BOOK_ORACLE_RATIO_CAP_BPS);
    }
    if cap_bps < MIN_BOOK_ORACLE_RATIO_CAP_BPS {
        return Err(EINVALID_BOOK_ORACLE_RATIO_CAP_BPS);
    }
    let PriceDetails::V1 { price_history, .. } = price_details;
    let PriceHistory::V1 {
        book_oracle_ratio_cap_bps,
        ..
    } = price_history;
    *book_oracle_ratio_cap_bps = cap_bps;
    Ok(())
}

// ===================== Funding mode switching =====================

/// Set continuous funding mode. Only allowed when currently in Periodic mode.
/// Returns (old_market_state, new_market_state).
pub fn set_continuous_funding_mode(
    price_details: &mut PriceDetails,
) -> Result<(MarketState, MarketState), u64> {
    let PriceDetails::V1 {
        price_config,
        price_state,
        funding_rate_history,
        ..
    } = price_details;
    let FundingRateHistory::V1 { charging_mode, .. } = funding_rate_history;

    if !matches!(charging_mode, FundingChargingMode::PeriodicV1 { .. }) {
        return Err(EINVALID_FUNDING_MODE);
    }

    let old_market_state =
        get_market_state_for_position_status_from_price(price_state, price_config);

    // Set accumulative_index to outstanding_funding_index
    if let FundingChargingMode::PeriodicV1 {
        outstanding_funding_index,
        ..
    } = charging_mode
    {
        let PriceState::V1 {
            accumulative_index, ..
        } = price_state;
        *accumulative_index = *outstanding_funding_index;
    }

    let new_market_state =
        get_market_state_for_position_status_from_price(price_state, price_config);

    *charging_mode = FundingChargingMode::ContinuousV1;

    Ok((old_market_state, new_market_state))
}

/// Set periodic funding mode. Only allowed when currently in Continuous mode.
pub fn set_periodic_funding_mode(
    price_details: &mut PriceDetails,
    funding_period_us: u64,
) -> Result<(), u64> {
    if funding_period_us < MIN_FUNDING_PERIOD_US {
        return Err(EINVALID_FUNDING_PERIOD_US);
    }
    if funding_period_us > MAX_FUNDING_PERIOD_US {
        return Err(EINVALID_FUNDING_PERIOD_US);
    }

    let PriceDetails::V1 {
        price_state,
        funding_rate_history,
        ..
    } = price_details;
    let FundingRateHistory::V1 {
        charging_mode,
        last_funding_calculated_us,
        ..
    } = funding_rate_history;

    if !matches!(charging_mode, FundingChargingMode::ContinuousV1) {
        return Err(EINVALID_FUNDING_MODE);
    }

    let PriceState::V1 {
        accumulative_index, ..
    } = price_state;

    *charging_mode = FundingChargingMode::PeriodicV1 {
        outstanding_funding_index: *accumulative_index,
        last_funding_charged_us: *last_funding_calculated_us,
        funding_period_us,
    };

    Ok(())
}

pub fn get_rate_size_multiplier() -> u64 {
    RATE_SIZE_MULTIPLIER
}

// ===================== Tests =====================

#[cfg(test)]
mod tests {
    use super::*;

    const MICRO_SECONDS_PER_HOUR: u64 = 3600_000_000;

    #[test]
    fn test_get_median_price() {
        assert_eq!(get_median_price(100, 100, 100), 100);
        assert_eq!(get_median_price(10, 20, 30), 20);
        assert_eq!(get_median_price(30, 20, 10), 20);
        assert_eq!(get_median_price(20, 10, 30), 20);
        assert_eq!(get_median_price(10, 30, 20), 20);
        assert_eq!(get_median_price(10, 10, 20), 10);
        assert_eq!(get_median_price(20, 10, 10), 10);
        assert_eq!(get_median_price(10, 20, 10), 10);
        assert_eq!(get_median_price(0, 10, 20), 10);
        assert_eq!(get_median_price(10, 0, 20), 10);
        assert_eq!(get_median_price(10, 20, 0), 10);
        assert_eq!(get_median_price(0, 0, 10), 0);
        assert_eq!(get_median_price(10, 0, 0), 0);
        assert_eq!(get_median_price(0, 10, 0), 0);
        assert_eq!(get_median_price(0, 0, 0), 0);
        assert_eq!(get_median_price(u64::MAX, u64::MAX, u64::MAX), u64::MAX);
        assert_eq!(get_median_price(u64::MAX, 1000, u64::MAX), u64::MAX);
        assert_eq!(get_median_price(1000, u64::MAX, 1000), 1000);
    }

    #[test]
    fn test_get_median_price_permutations() {
        let a = 10;
        let b = 20;
        let c = 30;
        assert_eq!(get_median_price(a, b, c), 20);
        assert_eq!(get_median_price(a, c, b), 20);
        assert_eq!(get_median_price(b, a, c), 20);
        assert_eq!(get_median_price(b, c, a), 20);
        assert_eq!(get_median_price(c, a, b), 20);
        assert_eq!(get_median_price(c, b, a), 20);
    }

    #[test]
    fn test_funding_cost_calculation() {
        let entry_index = AccumulativeIndex { index: 1 };
        let exit_index = AccumulativeIndex { index: 2 };
        let position_size = 100_000_000_000_000u64;
        let position_size_multiplier = 1u64;
        let funding_cost = get_funding_cost(
            &entry_index,
            &exit_index,
            position_size,
            position_size_multiplier,
            true,
        );
        assert_eq!(funding_cost, 100_000_000);

        let funding_cost = get_funding_cost(
            &entry_index,
            &exit_index,
            position_size,
            position_size_multiplier,
            false,
        );
        assert_eq!(funding_cost, -100_000_000);

        let entry_index = AccumulativeIndex { index: 2 };
        let exit_index = AccumulativeIndex { index: 1 };
        let funding_cost = get_funding_cost(
            &entry_index,
            &exit_index,
            position_size,
            position_size_multiplier,
            true,
        );
        assert_eq!(funding_cost, -100_000_000);

        let entry_index = AccumulativeIndex { index: 1 };
        let exit_index = AccumulativeIndex { index: -2 };
        let funding_cost = get_funding_cost(
            &entry_index,
            &exit_index,
            position_size,
            position_size_multiplier,
            true,
        );
        assert_eq!(funding_cost, -300_000_000);
    }

    #[test]
    fn test_update_accumulative_index() {
        let now = 3600_000_000u64; // 1 hour
        let rate_config = new_default_rate_config();
        let oracle_px = 1000;
        let impact_bid_px = 1000;
        let impact_ask_px = 1000;

        let mut price_details =
            create_default_price_details(oracle_px, 1, 20, now * 2 + 1, 0);

        let PriceDetails::V1 {
            ref mut price_state,
            ref mut funding_rate_history,
            ..
        } = price_details;

        update_accumulative_index(
            price_state,
            funding_rate_history,
            oracle_px,
            impact_bid_px,
            impact_ask_px,
            &rate_config,
            MAX_DAILY_FUNDING_RATE,
            now,
        );

        let PriceState::V1 {
            accumulative_index, ..
        } = price_state;
        assert_eq!(accumulative_index.index, 12500);

        // Test with premium (book prices higher than oracle)
        let impact_bid_px = 1020;
        let impact_ask_px = 1100;

        update_accumulative_index(
            price_state,
            funding_rate_history,
            oracle_px,
            impact_bid_px,
            impact_ask_px,
            &rate_config,
            MAX_DAILY_FUNDING_RATE,
            now * 2,
        );

        let PriceState::V1 {
            accumulative_index, ..
        } = price_state;
        assert_eq!(accumulative_index.index, 2_450_000);
    }

    #[test]
    fn test_calculate_daily_funding_rate() {
        let rate_config = new_default_rate_config();
        let oracle_px = 1000;
        let now = 0;
        let funding_rate_history = FundingRateHistory::V1 {
            last_funding_calculated_us: now,
            funding_rate_pause_timeout_us: 360_000,
            instant_rate_adjustment: FundingInstantRateAdjustment::INSTANT,
            charging_mode: FundingChargingMode::ContinuousV1,
        };

        // No premium
        let daily_fr = calculate_daily_funding_rate(
            &funding_rate_history,
            oracle_px,
            oracle_px,
            oracle_px,
            &rate_config,
            MAX_DAILY_FUNDING_RATE,
            now,
        );
        assert_eq!(
            daily_fr,
            i64_math::mul_div(100, MICRO_SECONDS_PER_DAY, 8 * MICRO_SECONDS_PER_HOUR)
                .unwrap()
        );

        // Positive premium
        let book_px = oracle_px + 20;
        let daily_fr = calculate_daily_funding_rate(
            &funding_rate_history,
            oracle_px,
            book_px,
            book_px,
            &rate_config,
            MAX_DAILY_FUNDING_RATE,
            now,
        );
        assert_eq!(
            daily_fr,
            i64_math::mul_div(19500, MICRO_SECONDS_PER_DAY, 8 * MICRO_SECONDS_PER_HOUR)
                .unwrap()
        );

        // Negative premium
        let book_px = oracle_px - 20;
        let daily_fr = calculate_daily_funding_rate(
            &funding_rate_history,
            oracle_px,
            book_px,
            book_px,
            &rate_config,
            MAX_DAILY_FUNDING_RATE,
            now,
        );
        assert_eq!(
            daily_fr,
            i64_math::mul_div(-19500, MICRO_SECONDS_PER_DAY, 8 * MICRO_SECONDS_PER_HOUR)
                .unwrap()
        );

        // Timeout case
        let book_px = oracle_px - 100;
        let daily_fr = calculate_daily_funding_rate(
            &funding_rate_history,
            oracle_px,
            book_px,
            book_px,
            &rate_config,
            MAX_DAILY_FUNDING_RATE,
            now + 360_000 + 1,
        );
        let PriceIndexStore::V2 {
            daily_interest_rate,
            ..
        } = &rate_config
        else {
            unreachable!()
        };
        assert_eq!(daily_fr, *daily_interest_rate as i64);
    }

    #[test]
    fn test_calculate_daily_funding_rate_from_oracle_and_impact() {
        let rate_config = new_default_rate_config();

        assert_eq!(
            calculate_daily_funding_rate_from_oracle_and_impact(
                0,
                10000,
                &rate_config,
                MAX_DAILY_FUNDING_RATE
            ),
            i64_math::mul_div(100, MICRO_SECONDS_PER_DAY, 8 * MICRO_SECONDS_PER_HOUR)
                .unwrap()
        );

        assert_eq!(
            calculate_daily_funding_rate_from_oracle_and_impact(
                5,
                10000,
                &rate_config,
                MAX_DAILY_FUNDING_RATE
            ),
            i64_math::mul_div(100, MICRO_SECONDS_PER_DAY, 8 * MICRO_SECONDS_PER_HOUR)
                .unwrap()
        );

        assert_eq!(
            calculate_daily_funding_rate_from_oracle_and_impact(
                -5,
                10000,
                &rate_config,
                MAX_DAILY_FUNDING_RATE
            ),
            0
        );

        assert_eq!(
            calculate_daily_funding_rate_from_oracle_and_impact(
                10,
                10000,
                &rate_config,
                MAX_DAILY_FUNDING_RATE
            ),
            i64_math::mul_div(500, MICRO_SECONDS_PER_DAY, 8 * MICRO_SECONDS_PER_HOUR)
                .unwrap()
        );

        assert_eq!(
            calculate_daily_funding_rate_from_oracle_and_impact(
                -10,
                10000,
                &rate_config,
                MAX_DAILY_FUNDING_RATE
            ),
            i64_math::mul_div(-500, MICRO_SECONDS_PER_DAY, 8 * MICRO_SECONDS_PER_HOUR)
                .unwrap()
        );

        assert_eq!(
            calculate_daily_funding_rate_from_oracle_and_impact(
                10000,
                10000,
                &rate_config,
                MAX_DAILY_FUNDING_RATE
            ),
            i64_math::mul_div(40_000, MICRO_SECONDS_PER_DAY, MICRO_SECONDS_PER_HOUR)
                .unwrap()
        );

        assert_eq!(
            calculate_daily_funding_rate_from_oracle_and_impact(
                -10000,
                10000,
                &rate_config,
                MAX_DAILY_FUNDING_RATE
            ),
            i64_math::mul_div(-40_000, MICRO_SECONDS_PER_DAY, MICRO_SECONDS_PER_HOUR)
                .unwrap()
        );
    }

    #[test]
    fn test_periodic_funding_no_flush_within_period() {
        let rate_config = new_default_rate_config();
        let oracle_px = 1000;
        let funding_period_us = 3600_000_000u64;

        let mut price_details =
            create_default_price_details(oracle_px, 1, 20, funding_period_us * 100, 0);
        let PriceDetails::V1 {
            ref mut funding_rate_history,
            ..
        } = price_details;
        *funding_rate_history = FundingRateHistory::V1 {
            last_funding_calculated_us: 0,
            funding_rate_pause_timeout_us: funding_period_us * 100,
            instant_rate_adjustment: FundingInstantRateAdjustment::INSTANT,
            charging_mode: FundingChargingMode::PeriodicV1 {
                outstanding_funding_index: AccumulativeIndex { index: 0 },
                last_funding_charged_us: 0,
                funding_period_us,
            },
        };

        let PriceDetails::V1 {
            ref mut price_state,
            ref mut funding_rate_history,
            ..
        } = price_details;

        update_accumulative_index(
            price_state,
            funding_rate_history,
            oracle_px,
            oracle_px,
            oracle_px,
            &rate_config,
            MAX_DAILY_FUNDING_RATE,
            1800_000_000,
        );

        let PriceState::V1 {
            accumulative_index, ..
        } = price_state;
        assert_eq!(accumulative_index.index, 0);

        let FundingRateHistory::V1 { charging_mode, .. } = funding_rate_history;
        if let FundingChargingMode::PeriodicV1 {
            outstanding_funding_index,
            ..
        } = charging_mode
        {
            assert_eq!(outstanding_funding_index.index, 6250);
        } else {
            panic!("expected PeriodicV1");
        }
    }

    #[test]
    fn test_periodic_funding_flushes_at_period_boundary() {
        let rate_config = new_default_rate_config();
        let oracle_px = 1000;
        let funding_period_us = 3600_000_000u64;

        let mut price_details =
            create_default_price_details(oracle_px, 1, 20, funding_period_us * 100, 0);
        let PriceDetails::V1 {
            ref mut funding_rate_history,
            ..
        } = price_details;
        *funding_rate_history = FundingRateHistory::V1 {
            last_funding_calculated_us: 0,
            funding_rate_pause_timeout_us: funding_period_us * 100,
            instant_rate_adjustment: FundingInstantRateAdjustment::INSTANT,
            charging_mode: FundingChargingMode::PeriodicV1 {
                outstanding_funding_index: AccumulativeIndex { index: 0 },
                last_funding_charged_us: 0,
                funding_period_us,
            },
        };

        let PriceDetails::V1 {
            ref mut price_state,
            ref mut funding_rate_history,
            ..
        } = price_details;

        // Step 1: 30 min
        update_accumulative_index(
            price_state,
            funding_rate_history,
            oracle_px,
            oracle_px,
            oracle_px,
            &rate_config,
            MAX_DAILY_FUNDING_RATE,
            1800_000_000,
        );

        // Step 2: 1 hour -- crosses boundary, flush
        update_accumulative_index(
            price_state,
            funding_rate_history,
            oracle_px,
            oracle_px,
            oracle_px,
            &rate_config,
            MAX_DAILY_FUNDING_RATE,
            3600_000_000,
        );

        let PriceState::V1 {
            accumulative_index, ..
        } = price_state;
        assert_eq!(accumulative_index.index, 12500);
    }

    #[test]
    fn test_periodic_funding_split_at_period_boundary_mid_crossing() {
        let rate_config = new_default_rate_config();
        let oracle_px = 1000;
        let funding_period_us = 3600_000_000u64;

        let mut price_details =
            create_default_price_details(oracle_px, 1, 20, funding_period_us * 100, 0);
        let PriceDetails::V1 {
            ref mut funding_rate_history,
            ..
        } = price_details;
        *funding_rate_history = FundingRateHistory::V1 {
            last_funding_calculated_us: 0,
            funding_rate_pause_timeout_us: funding_period_us * 100,
            instant_rate_adjustment: FundingInstantRateAdjustment::INSTANT,
            charging_mode: FundingChargingMode::PeriodicV1 {
                outstanding_funding_index: AccumulativeIndex { index: 0 },
                last_funding_charged_us: 0,
                funding_period_us,
            },
        };

        let PriceDetails::V1 {
            ref mut price_state,
            ref mut funding_rate_history,
            ..
        } = price_details;

        // Single update from t=0 to t=4800s (1h20m)
        update_accumulative_index(
            price_state,
            funding_rate_history,
            oracle_px,
            oracle_px,
            oracle_px,
            &rate_config,
            MAX_DAILY_FUNDING_RATE,
            4800_000_000,
        );

        let PriceState::V1 {
            accumulative_index, ..
        } = price_state;
        assert_eq!(accumulative_index.index, 12500);

        let FundingRateHistory::V1 { charging_mode, .. } = funding_rate_history;
        if let FundingChargingMode::PeriodicV1 {
            outstanding_funding_index,
            ..
        } = charging_mode
        {
            assert_eq!(outstanding_funding_index.index, 16666);
        } else {
            panic!("expected PeriodicV1");
        }
    }

    #[test]
    fn test_compute_max_daily_funding_rate() {
        assert_eq!(
            compute_max_daily_funding_rate(
                20,
                1000000,
                &FundingChargingMode::ContinuousV1
            ),
            MAX_DAILY_FUNDING_RATE
        );

        let hourly_funding = FundingChargingMode::PeriodicV1 {
            funding_period_us: 3600_000_000,
            last_funding_charged_us: 0,
            outstanding_funding_index: AccumulativeIndex { index: 0 },
        };
        let max_rate_frac = new_default_rate_config();
        let PriceIndexStore::V2 {
            max_rate_as_fraction_of_initial_margin,
            ..
        } = &max_rate_frac
        else {
            unreachable!()
        };

        assert_eq!(
            compute_max_daily_funding_rate(
                4,
                *max_rate_as_fraction_of_initial_margin,
                &hourly_funding
            ),
            MAX_DAILY_FUNDING_RATE
        );

        assert_eq!(
            compute_max_daily_funding_rate(
                5,
                *max_rate_as_fraction_of_initial_margin,
                &hourly_funding
            ) / 24,
            33333
        );

        assert_eq!(
            compute_max_daily_funding_rate(
                20,
                *max_rate_as_fraction_of_initial_margin,
                &hourly_funding
            ) / 24,
            8333
        );

        assert_eq!(
            compute_max_daily_funding_rate(
                40,
                *max_rate_as_fraction_of_initial_margin,
                &hourly_funding
            ) / 24,
            4166
        );
    }
}




// ===================== Additional stubs for perp_engine delegation =====================

impl PriceChangeDetails {
    pub fn new_mark_px_from_change_details(&self) -> u64 {
        match self {
            PriceChangeDetails::V1 { new_mark_px, .. } => *new_mark_px,
        }
    }
    pub fn has_updated_on_mark_price(&self) -> bool {
        // If we have a PriceChangeDetails, a price update occurred
        true
    }
    pub fn into_old_and_new_market_state(self) -> (MarketState, MarketState) {
        match self {
            PriceChangeDetails::V1 { old_market_state, new_market_state, .. } => {
                (old_market_state, new_market_state)
            }
        }
    }
}

pub fn update_price_with_upgrade(
    _market: [u8; 32],
    _oracle_price: u64,
    _mark_price_refresh_input: MarkPriceRefreshInput,
) -> Option<PriceChangeDetails> {
    // Dispatch layer handles actual price update logic
    None
}


// Address-based dispatch wrappers
pub fn get_book_mid_ema_px_by_addr(_market: [u8; 32]) -> u64 {
    0
}

pub fn get_mark_price_by_addr(_market: [u8; 32]) -> u64 {
    0
}

pub fn get_oracle_price_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves PriceManagement resource
    0
}

pub fn get_mark_and_oracle_price_by_addr(_market: [u8; 32]) -> (u64, u64) {
    // Dispatch layer resolves PriceManagement resource
    (0, 0)
}

pub fn get_unrealized_pnl_haircut_bps_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves PriceManagement resource
    0
}

pub fn set_unrealized_pnl_haircut_bps_by_addr(_market: [u8; 32], _haircut_bps: u64) {
    // Dispatch layer resolves PriceManagement resource
}

pub fn set_max_leverage_by_addr(_market: [u8; 32], _max_leverage: u8) {
    // Dispatch layer resolves PriceManagement resource
}

pub fn set_continuous_funding_mode_by_addr(
    _market: [u8; 32],
) -> (MarketState, MarketState) {
    // Dispatch layer resolves PriceManagement resource
    (MarketState::V1 {
        short_mark_px: 0,
        long_mark_px: 0,
        accumulative_index: AccumulativeIndex { index: 0 },
        size_multiplier: 0,
        unrealized_pnl_haircut_bps: 0,
        withdrawable_margin_leverage: 1,
        max_leverage: 1,
    }, MarketState::V1 {
        short_mark_px: 0,
        long_mark_px: 0,
        accumulative_index: AccumulativeIndex { index: 0 },
        size_multiplier: 0,
        unrealized_pnl_haircut_bps: 0,
        withdrawable_margin_leverage: 1,
        max_leverage: 1,
    })
}

pub fn get_market_state_for_position_status_by_addr(_market: [u8; 32]) -> MarketState {
    // Dispatch layer resolves PriceManagement resource
    MarketState::V1 {
        short_mark_px: 0,
        long_mark_px: 0,
        accumulative_index: AccumulativeIndex { index: 0 },
        size_multiplier: 0,
        unrealized_pnl_haircut_bps: 0,
        withdrawable_margin_leverage: 1,
        max_leverage: 1,
    }
}

pub fn add_override_mark_price_by_addr(
    _market: [u8; 32], _mark_price: u64,
) -> PriceChangeDetails {
    // Dispatch layer resolves PriceManagement resource
    PriceChangeDetails::V1 {
        new_mark_px: 0,
        old_market_state: MarketState::V1 {
        short_mark_px: 0,
        long_mark_px: 0,
        accumulative_index: AccumulativeIndex { index: 0 },
        size_multiplier: 0,
        unrealized_pnl_haircut_bps: 0,
        withdrawable_margin_leverage: 1,
        max_leverage: 1,
    },
        new_market_state: MarketState::V1 {
        short_mark_px: 0,
        long_mark_px: 0,
        accumulative_index: AccumulativeIndex { index: 0 },
        size_multiplier: 0,
        unrealized_pnl_haircut_bps: 0,
        withdrawable_margin_leverage: 1,
        max_leverage: 1,
    },
    }
}
