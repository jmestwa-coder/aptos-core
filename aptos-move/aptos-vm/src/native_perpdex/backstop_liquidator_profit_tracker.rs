// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::backstop_liquidator_profit_tracker

use crate::native_perpdex::i64_math;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===================== Constants =====================

const EINVALID_ADDRESS: u64 = 1;
const ETRACKER_NOT_INITIALIZED: u64 = 2;
const EINVALID_PERCENTAGE: u64 = 3;
const E_INVALID_NEW_WATERMARK: u64 = 4;

const DEFAULT_MARGIN_AS_PROFIT_PERCENTAGE: u64 = 5000; // 50%
const DEFAULT_MARGIN_AS_PROFIT_PERCENTAGE_DIVISOR: u64 = 10000;

// ===================== Types =====================

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarketTrackingData {
    V1 {
        realized_pnl: i64,
        realized_pnl_watermark: i64,
        entry_px_times_size_sum: u128,
        liquidation_size: u64,
        is_long: bool,
    },
}

pub fn unpack(data: MarketTrackingData) -> (i64, i64, u128, u64, bool) {
    let MarketTrackingData::V1 {
        realized_pnl,
        realized_pnl_watermark,
        entry_px_times_size_sum,
        liquidation_size,
        is_long,
    } = data;
    (realized_pnl, realized_pnl_watermark, entry_px_times_size_sum, liquidation_size, is_long)
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackstopProfitChangeReason {
    LiquidationProfit,
    LiquidationLoss,
    PositionNetting,
}

/// RESOURCE: BackstopLiquidatorProfitTracker at @decibel_dex
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BackstopLiquidatorProfitTracker {
    V1 {
        market_data: BTreeMap<[u8; 32], MarketTrackingData>, // Object<PerpMarket> address -> data
        blp_margin_as_profit_percentage: u64,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AdlTrackerStatus {
    Status {
        adl_threshold: u64,
        mark_price: u64,
        realized_pnl: i64,
        realized_pnl_watermark: i64,
        unrealized_pnl: i64,
        total_pnl: i64,
        pnl_from_watermark: i64,
        inventory_size: u64,
        inventory_is_long: bool,
        entry_px_times_size_sum: u128,
        would_trigger_adl: bool,
        adl_price: u64,
    },
}

// ===================== Functions =====================

pub fn initialize(
    admin_addr: [u8; 32],
    decibel_dex_addr: [u8; 32],
) -> Result<BackstopLiquidatorProfitTracker, u64> {
    if admin_addr != decibel_dex_addr {
        return Err(EINVALID_ADDRESS);
    }
    Ok(BackstopLiquidatorProfitTracker::V1 {
        market_data: BTreeMap::new(),
        blp_margin_as_profit_percentage: DEFAULT_MARGIN_AS_PROFIT_PERCENTAGE,
    })
}

pub fn initialize_market(tracker: &mut BackstopLiquidatorProfitTracker, market: [u8; 32]) {
    let BackstopLiquidatorProfitTracker::V1 { market_data, .. } = tracker;
    market_data.insert(market, MarketTrackingData::V1 {
        realized_pnl: 0,
        realized_pnl_watermark: 0,
        entry_px_times_size_sum: 0,
        liquidation_size: 0,
        is_long: false,
    });
}

fn handle_position_netting(
    market: [u8; 32],
    market_data: &mut MarketTrackingData,
    exit_price: u64,
    netted_size: u64,
    size_multiplier: u64,
) {
    let MarketTrackingData::V1 {
        realized_pnl,
        entry_px_times_size_sum,
        liquidation_size,
        is_long,
        ..
    } = market_data;

    let entry_px_times_size_netted =
        *entry_px_times_size_sum * (netted_size as u128) / (*liquidation_size as u128);
    let pnl = calculate_pnl(
        entry_px_times_size_netted,
        exit_price,
        netted_size,
        *is_long,
        size_multiplier,
    );
    *realized_pnl += pnl;
    // EVENT: BackstopLiquidatorProfitEvent - PositionNetting
}

pub fn handle_regular_trade(
    tracker: &mut BackstopLiquidatorProfitTracker,
    market: [u8; 32],
    exit_price: u64,
    size: u64,
    is_long: bool,
    size_multiplier: u64,
) {
    let BackstopLiquidatorProfitTracker::V1 { market_data, .. } = tracker;
    let md = match market_data.get_mut(&market) {
        Some(md) => md,
        None => return,
    };
    let MarketTrackingData::V1 {
        is_long: md_is_long,
        liquidation_size: md_liq_size,
        entry_px_times_size_sum: md_eps,
        ..
    } = md;

    if *md_is_long == is_long || *md_liq_size == 0 {
        return;
    }

    let existing_size = *md_liq_size;
    if size >= existing_size {
        handle_position_netting(market, md, exit_price, existing_size, size_multiplier);
        let MarketTrackingData::V1 { entry_px_times_size_sum, liquidation_size, .. } = md;
        *entry_px_times_size_sum = 0;
        *liquidation_size = 0;
    } else {
        let remaining_size = *md_liq_size - size;
        handle_position_netting(market, md, exit_price, size, size_multiplier);
        let MarketTrackingData::V1 { entry_px_times_size_sum, liquidation_size, .. } = md;
        *entry_px_times_size_sum =
            *entry_px_times_size_sum * (remaining_size as u128) / (*liquidation_size as u128);
        *liquidation_size = remaining_size;
    }
}

pub fn handle_liquidation_acquisition(
    tracker: &mut BackstopLiquidatorProfitTracker,
    market: [u8; 32],
    entry_price: u64,
    size: u64,
    is_long: bool,
    blp_position_size: u64,
    blp_position_is_long: bool,
    size_multiplier: u64,
) {
    let BackstopLiquidatorProfitTracker::V1 { market_data, .. } = tracker;
    let md = market_data.get_mut(&market).expect("market not initialized");
    let entry_px_times_size = (entry_price as u128) * (size as u128);

    let MarketTrackingData::V1 {
        is_long: md_is_long,
        liquidation_size: md_liq_size,
        entry_px_times_size_sum: md_eps,
        ..
    } = md;

    if *md_is_long == is_long || *md_liq_size == 0 {
        *md_eps += entry_px_times_size;
        *md_liq_size += size;
        *md_is_long = is_long;
    } else if size > *md_liq_size {
        let existing_size = *md_liq_size;
        handle_position_netting(market, md, entry_price, existing_size, size_multiplier);
        let MarketTrackingData::V1 { entry_px_times_size_sum, liquidation_size, is_long: il, .. } = md;
        let remaining_size = size - existing_size;
        *entry_px_times_size_sum = (entry_price as u128) * (remaining_size as u128);
        *liquidation_size = remaining_size;
        *il = is_long;
    } else {
        let remaining_size = *md_liq_size - size;
        handle_position_netting(market, md, entry_price, size, size_multiplier);
        let MarketTrackingData::V1 { entry_px_times_size_sum, liquidation_size, .. } = md;
        *entry_px_times_size_sum =
            *entry_px_times_size_sum * (remaining_size as u128) / (*liquidation_size as u128);
        *liquidation_size = remaining_size;
    }

    // Clamp to BLP position
    let MarketTrackingData::V1 {
        entry_px_times_size_sum,
        liquidation_size,
        is_long: md_is_long,
        ..
    } = md;
    if *liquidation_size > 0 {
        if blp_position_size == 0 || blp_position_is_long != *md_is_long {
            *entry_px_times_size_sum = 0;
            *liquidation_size = 0;
        } else if blp_position_size < *liquidation_size {
            *entry_px_times_size_sum =
                *entry_px_times_size_sum * (blp_position_size as u128) / (*liquidation_size as u128);
            *liquidation_size = blp_position_size;
        }
    }
}

pub fn track_profit(
    tracker: &mut BackstopLiquidatorProfitTracker,
    market: [u8; 32],
    profit: i64,
) {
    let BackstopLiquidatorProfitTracker::V1 { market_data, blp_margin_as_profit_percentage } = tracker;
    let md = market_data.get_mut(&market).expect("market not initialized");
    let MarketTrackingData::V1 { realized_pnl, .. } = md;

    let pnl_delta = if profit > 0 {
        i64_math::mul_div(profit, *blp_margin_as_profit_percentage as u64, DEFAULT_MARGIN_AS_PROFIT_PERCENTAGE_DIVISOR).unwrap_or(profit)
    } else {
        profit
    };
    *realized_pnl += pnl_delta;
    // EVENT: BackstopLiquidatorProfitEvent
}

pub fn set_realized_pnl_watermark(
    tracker: &mut BackstopLiquidatorProfitTracker,
    _caller: [u8; 32],
    market: [u8; 32],
    new_watermark: i64,
) -> Result<(), u64> {
    let BackstopLiquidatorProfitTracker::V1 { market_data, .. } = tracker;
    let md = market_data.get_mut(&market).expect("market not initialized");
    let MarketTrackingData::V1 { realized_pnl, realized_pnl_watermark, .. } = md;
    if new_watermark > *realized_pnl {
        return Err(E_INVALID_NEW_WATERMARK);
    }
    if *realized_pnl_watermark != new_watermark {
        *realized_pnl_watermark = new_watermark;
        // EVENT: RealizedPnlWatermarkChanged
    }
    Ok(())
}

fn calculate_pnl(
    entry_px_times_size: u128,
    exit_price: u64,
    size: u64,
    is_long: bool,
    size_multiplier: u64,
) -> i64 {
    let exit_px_times_size = (exit_price as u128) * (size as u128);
    let pnl_magnitude = (exit_px_times_size as i128) - (entry_px_times_size as i128);
    let pnl_amount = (pnl_magnitude / (size_multiplier as i128)) as i64;
    if is_long { pnl_amount } else { -pnl_amount }
}

pub fn should_trigger_adl(
    tracker: &BackstopLiquidatorProfitTracker,
    market: [u8; 32],
    mark_price: u64,
    threshold: u64,
    size_multiplier: u64,
) -> Option<u64> {
    if threshold == 0 {
        return None;
    }
    let BackstopLiquidatorProfitTracker::V1 { market_data, .. } = tracker;
    let md = match market_data.get(&market) {
        Some(md) => md,
        None => return None,
    };
    let MarketTrackingData::V1 {
        realized_pnl,
        realized_pnl_watermark,
        entry_px_times_size_sum,
        liquidation_size,
        is_long,
    } = md;

    if *liquidation_size == 0 {
        return None;
    }

    let unrealized_pnl = calculate_pnl(
        *entry_px_times_size_sum,
        mark_price,
        *liquidation_size,
        *is_long,
        size_multiplier,
    );

    let realized_pnl_delta = realized_pnl - realized_pnl_watermark;
    let total_pnl_from_watermark = realized_pnl_delta + unrealized_pnl;

    if total_pnl_from_watermark > 0 || (-total_pnl_from_watermark as u64) < threshold {
        return None;
    }

    // Calculate ADL settle price
    let is_short: i128 = if *is_long { -1 } else { 1 };

    let bankruptcy_price_times_sz =
        ((realized_pnl_delta + (threshold as i64)) as i128) * is_short
            * (size_multiplier as i128)
            + (*entry_px_times_size_sum as i128);

    let bankruptcy_price =
        (bankruptcy_price_times_sz / (*liquidation_size as i128)) as i64;

    let capped_bankruptcy_price = std::cmp::max(bankruptcy_price, 1) as u64;

    let raw_adl_price = if *is_long {
        std::cmp::max(mark_price, capped_bankruptcy_price)
    } else {
        std::cmp::min(mark_price, capped_bankruptcy_price)
    };

    // Note: In the real implementation, round_price_to_ticker would be called here.
    // For native, the caller handles ticker rounding.
    let adl_price = if raw_adl_price == 0 { 1 } else { raw_adl_price };

    Some(adl_price)
}

pub fn get_realized_pnl(tracker: &BackstopLiquidatorProfitTracker, market: [u8; 32]) -> i64 {
    let BackstopLiquidatorProfitTracker::V1 { market_data, .. } = tracker;
    match market_data.get(&market) {
        None => 0,
        Some(MarketTrackingData::V1 { realized_pnl, .. }) => *realized_pnl,
    }
}

pub fn set_blp_margin_as_profit_percentage(
    tracker: &mut BackstopLiquidatorProfitTracker,
    _caller: [u8; 32],
    percentage: u64,
) -> Result<(), u64> {
    if percentage == 0 {
        return Err(EINVALID_PERCENTAGE);
    }
    if percentage >= DEFAULT_MARGIN_AS_PROFIT_PERCENTAGE_DIVISOR {
        return Err(EINVALID_PERCENTAGE);
    }
    let BackstopLiquidatorProfitTracker::V1 { blp_margin_as_profit_percentage, .. } = tracker;
    if *blp_margin_as_profit_percentage != percentage {
        *blp_margin_as_profit_percentage = percentage;
        // EVENT: BlpMarginAsProfitPercentageChanged
    }
    Ok(())
}

pub fn get_unrealized_pnl(
    tracker: &BackstopLiquidatorProfitTracker,
    market: [u8; 32],
    mark_price: u64,
    size_multiplier: u64,
) -> i64 {
    let BackstopLiquidatorProfitTracker::V1 { market_data, .. } = tracker;
    match market_data.get(&market) {
        None => 0,
        Some(MarketTrackingData::V1 { entry_px_times_size_sum, liquidation_size, is_long, .. }) => {
            if *liquidation_size == 0 {
                return 0;
            }
            calculate_pnl(*entry_px_times_size_sum, mark_price, *liquidation_size, *is_long, size_multiplier)
        }
    }
}

pub fn get_total_pnl(
    tracker: &BackstopLiquidatorProfitTracker,
    market: [u8; 32],
    mark_price: u64,
    size_multiplier: u64,
) -> i64 {
    let realized = get_realized_pnl(tracker, market);
    let unrealized = get_unrealized_pnl(tracker, market, mark_price, size_multiplier);
    realized + unrealized
}

pub fn view_market_tracking_data(
    tracker: &BackstopLiquidatorProfitTracker,
    market: [u8; 32],
) -> Option<MarketTrackingData> {
    let BackstopLiquidatorProfitTracker::V1 { market_data, .. } = tracker;
    market_data.get(&market).copied()
}

pub fn view_blp_margin_as_profit_pct(tracker: &BackstopLiquidatorProfitTracker) -> u64 {
    let BackstopLiquidatorProfitTracker::V1 { blp_margin_as_profit_percentage, .. } = tracker;
    *blp_margin_as_profit_percentage
}

// ===================== Dispatch stubs (by-addr) for perp_engine delegation =====================

pub fn get_total_pnl_by_addr(_market: [u8; 32], _mark_price: u64) -> i64 {
    // Dispatch layer resolves BackstopLiquidatorProfitTracker + PriceManagement
    0
}

pub fn get_adl_tracker_status_by_addr(
    _market: [u8; 32], _adl_threshold: u64,
) -> AdlTrackerStatus {
    // Dispatch layer resolves BackstopLiquidatorProfitTracker
    AdlTrackerStatus::Status {
        adl_threshold: 0,
        mark_price: 0,
        realized_pnl: 0,
        realized_pnl_watermark: 0,
        unrealized_pnl: 0,
        total_pnl: 0,
        pnl_from_watermark: 0,
        inventory_size: 0,
        inventory_is_long: false,
        entry_px_times_size_sum: 0,
        would_trigger_adl: false,
        adl_price: 0,
    }
}

pub fn set_realized_pnl_watermark_by_addr(
    _caller: [u8; 32], _market: [u8; 32], _new_watermark: i64,
) {
    // Dispatch layer resolves BackstopLiquidatorProfitTracker
}

pub fn initialize_market_by_addr(_market: [u8; 32]) {
    // Dispatch layer resolves BackstopLiquidatorProfitTracker
}
