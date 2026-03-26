// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::liquidation

use crate::native_perpdex::global_liquidation_state::BackstopLiquidationContinuation;
use crate::native_perpdex::i64_math;
use crate::native_perpdex::work_unit_utils::WorkUnit;
use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const EBACKSTOP_LIQUIDATOR_NOT_INITIALIZED: u64 = 1;
const ECANNOT_SETTLE_BACKSTOP_LIQUIDATION: u64 = 2;
const EACCOUNT_EQUITY_NOT_POSITIVE: u64 = 4;
const EBACKSTOP_LIQUIDATOR_COVERED_LOSS_NOT_ZERO: u64 = 5;

// ===================== Types =====================

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarginCallResult {
    Solvent,
    RequiresBackstopLiquidation,
    Reprocess,
    Continuation,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarginCallResultDetail {
    None,
    PositionNotFound,
    FeeExceedsHealthRatio,
    LiquidationPriceZero,
    EffectiveSlippageExceedsMMRRatio,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarginCallContinuation {
    V1 {
        slippage_to_test: u64,
    },
}

// ===================== Functions =====================

pub fn is_requires_backstop_liquidation(result: &MarginCallResult) -> bool {
    matches!(result, MarginCallResult::RequiresBackstopLiquidation)
}

pub fn is_continuation(result: &MarginCallResult) -> bool {
    matches!(result, MarginCallResult::Continuation)
}

pub fn is_reprocess(result: &MarginCallResult) -> bool {
    matches!(result, MarginCallResult::Reprocess)
}

pub fn is_solvent(result: &MarginCallResult) -> bool {
    matches!(result, MarginCallResult::Solvent)
}

pub fn get_slippage_to_test(cont: &MarginCallContinuation) -> u64 {
    let MarginCallContinuation::V1 { slippage_to_test } = cont;
    *slippage_to_test
}

pub fn default_margin_call_continuation(starting_slippage_pct: u64) -> MarginCallContinuation {
    MarginCallContinuation::V1 {
        slippage_to_test: starting_slippage_pct,
    }
}

/// Compute the minimum number of units to liquidate to bring account equity
/// back to maintenance margin requirement.
///
/// See Move source for derivation and examples.
pub fn min_liquidation_units(
    position_size: u64,
    account_equity: i64,
    maintenance_margin_required: u64,
    inv_leverage_w_mmr_buffer: u64,
    effective_slippage: u64,
    mark_price: u64,
    size_mult: u64,
    scale: u64,
    min_size: u64,
    lot_size: u64,
) -> u64 {
    let deficit: u64 = maintenance_margin_required - (account_equity as u64) + 1;
    let numerator: u128 = (deficit as u128) * (size_mult as u128) * (scale as u128);
    let denominator: u128 =
        (mark_price as u128) * ((inv_leverage_w_mmr_buffer as u128) - (effective_slippage as u128));

    if denominator == 0 {
        return position_size;
    }

    // ceil division
    let mut u_star = (numerator + denominator - 1) / denominator;

    if u_star <= (min_size as u128) {
        u_star = min_size as u128;
    }
    if u_star % (lot_size as u128) != 0 {
        u_star = u_star + (lot_size as u128) - (u_star % (lot_size as u128));
    }
    if u_star > (position_size as u128) {
        position_size
    } else {
        u_star as u64
    }
}

/// Sort positions by unrealized PnL in descending order (most profitable first).
/// Uses heapsort for guaranteed O(n log n) with no recursion.
pub fn heapsort_descending_by_upnl(upnls: &mut Vec<(usize, i64)>) {
    let n = upnls.len();
    if n <= 1 {
        return;
    }
    // Build min-heap
    let mut start = n / 2;
    while start > 0 {
        start -= 1;
        sift_down_min(upnls, start, n - 1);
    }
    // Extract min to end
    let mut end = n - 1;
    while end > 0 {
        upnls.swap(0, end);
        end -= 1;
        sift_down_min(upnls, 0, end);
    }
}

fn sift_down_min(arr: &mut Vec<(usize, i64)>, start: usize, end: usize) {
    let mut root = start;
    loop {
        let mut child = 2 * root + 1;
        if child > end {
            break;
        }
        if child + 1 <= end && arr[child + 1].1 < arr[child].1 {
            child += 1;
        }
        if arr[root].1 <= arr[child].1 {
            break;
        }
        arr.swap(root, child);
        root = child;
    }
}

/// Check if ADL should be triggered based on threshold.
/// Returns Some(adl_price) if ADL should be triggered, None otherwise.
/// Delegates to backstop_liquidator_profit_tracker.
pub fn should_trigger_adl_price(
    mark_price: u64,
    threshold: u64,
    realized_pnl: i64,
    realized_pnl_watermark: i64,
    entry_px_times_size_sum: u128,
    liquidation_size: u64,
    is_long: bool,
    size_multiplier: u64,
) -> Option<u64> {
    if threshold == 0 || liquidation_size == 0 {
        return None;
    }

    let exit_px_times_size = (mark_price as u128) * (liquidation_size as u128);
    let pnl_magnitude = (exit_px_times_size as i128) - (entry_px_times_size_sum as i128);
    let pnl_amount = (pnl_magnitude / (size_multiplier as i128)) as i64;
    let unrealized_pnl = if is_long { pnl_amount } else { -pnl_amount };

    let realized_pnl_delta = realized_pnl - realized_pnl_watermark;
    let total_pnl_from_watermark = realized_pnl_delta + unrealized_pnl;

    if total_pnl_from_watermark > 0 || (-total_pnl_from_watermark as u64) < threshold {
        return None;
    }

    let is_short: i128 = if is_long { -1 } else { 1 };
    let bankruptcy_price_times_sz =
        ((realized_pnl_delta + (threshold as i64)) as i128) * is_short
            * (size_multiplier as i128)
            + (entry_px_times_size_sum as i128);
    let bankruptcy_price =
        (bankruptcy_price_times_sz / (liquidation_size as i128)) as i64;
    let capped_bankruptcy_price = std::cmp::max(bankruptcy_price, 1) as u64;

    let raw_adl_price = if is_long {
        std::cmp::max(mark_price, capped_bankruptcy_price)
    } else {
        std::cmp::min(mark_price, capped_bankruptcy_price)
    };

    let adl_price = if raw_adl_price == 0 { 1 } else { raw_adl_price };
    Some(adl_price)
}
