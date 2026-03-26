// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::order_margin
//
// This module bridges between order placement validation and position/collateral state.
// Functions take references to positions and collateral and delegate to pending_order_tracker.

use crate::native_perpdex::pending_order_tracker::{self, GlobalSummary};

// ===================== Types =====================

pub type PerpMarketRef = [u8; 32];

// ===================== Functions =====================

/// Calculate available order margin for an account.
/// available = free_collateral_for_crossed - pending_order_margin
pub fn available_order_margin(
    global_summary: &GlobalSummary,
    account: [u8; 32],
    free_collateral_for_crossed: u64,
) -> u64 {
    let order_margin = global_summary.get_pending_order_margin(account);
    if order_margin >= free_collateral_for_crossed {
        0
    } else {
        free_collateral_for_crossed - order_margin
    }
}

/// Validate reduce-only order placement.
/// Returns None if valid, Some(reason) if invalid.
pub fn validate_reduce_only_order(
    global_summary: &GlobalSummary,
    account: [u8; 32],
    market: PerpMarketRef,
    is_long: bool,
    position_size: u64,
    position_is_long: bool,
) -> Option<String> {
    if position_size == 0 {
        return Some("Cannot place reduce only order with no position".to_string());
    }
    if is_long == position_is_long {
        return Some(
            "Reduce only order direction must be opposite to position direction".to_string(),
        );
    }
    if let Some(account_summary) = global_summary.summary().get(&account) {
        if let Some(market_state) = account_summary.markets().get(&market) {
            let pending_order_tracker::ReduceOnlyOrders::V1 { orders, .. } =
                market_state.reduce_only_orders();
            if orders.len() >= (pending_order_tracker::MAX_REDUCE_ONLY_ORDERS_PER_MARKET as usize)
            {
                return Some(
                    "Maximum allowed number of reduce only orders exceeded for market".to_string(),
                );
            }
        }
    }
    None
}

/// Validate non-reduce-only order placement.
/// Returns true if order can be placed (enough margin).
pub fn validate_non_reduce_only_order_placement(
    global_summary: &GlobalSummary,
    account: [u8; 32],
    market: PerpMarketRef,
    order_size: u64,
    limit_price: u64,
    is_long: bool,
    position_size: u64,
    position_is_long: bool,
    user_leverage: u8,
    available_margin: u64,
    size_multiplier: u64,
) -> bool {
    // Get existing market state
    let (mut eff_longs, mut eff_shorts, bulk_longs, bulk_shorts) =
        if let Some(account_summary) = global_summary.summary().get(&account) {
            if let Some(market_state) = account_summary.markets().get(&market) {
                (
                    *market_state.pending_single_longs(),
                    *market_state.pending_single_shorts(),
                    *market_state.pending_bulk_longs(),
                    *market_state.pending_bulk_shorts(),
                )
            } else {
                let empty = pending_order_tracker::PendingOrderSummary::V1 {
                    price_size_sum: 0,
                    size_sum: 0,
                };
                (empty, empty, empty, empty)
            }
        } else {
            let empty = pending_order_tracker::PendingOrderSummary::V1 {
                price_size_sum: 0,
                size_sum: 0,
            };
            (empty, empty, empty, empty)
        };

    // Add new order to effective pending
    if is_long {
        match &mut eff_longs {
            pending_order_tracker::PendingOrderSummary::V1 {
                price_size_sum,
                size_sum,
            } => {
                *price_size_sum += (order_size as u128) * (limit_price as u128);
                *size_sum += order_size;
            },
        }
    } else {
        match &mut eff_shorts {
            pending_order_tracker::PendingOrderSummary::V1 {
                price_size_sum,
                size_sum,
            } => {
                *price_size_sum += (order_size as u128) * (limit_price as u128);
                *size_sum += order_size;
            },
        }
    }

    // Calculate required margin for this market with new order
    let effecting_pending_price_size = pending_price_size_for_market(
        position_size,
        position_is_long,
        &eff_longs,
        &eff_shorts,
        &bulk_longs,
        &bulk_shorts,
    );

    let divisor = (size_multiplier as u128) * (user_leverage as u128);
    let required_margin_for_market = if divisor == 0 {
        0u64
    } else {
        ((effecting_pending_price_size + divisor - 1) / divisor) as u64
    };

    // Calculate total required margin across all markets
    let mut total_required_margin = 0u64;
    if let Some(account_summary) = global_summary.summary().get(&account) {
        for (acc_market, state) in account_summary.markets() {
            if *acc_market != market {
                total_required_margin += state.pending_margin();
            }
        }
    }
    total_required_margin += required_margin_for_market;
    total_required_margin <= available_margin
}

// Re-use the same helper from pending_order_tracker
fn pending_price_size_for_market(
    position_size: u64,
    position_is_long: bool,
    pending_single_longs: &pending_order_tracker::PendingOrderSummary,
    pending_single_shorts: &pending_order_tracker::PendingOrderSummary,
    pending_bulk_longs: &pending_order_tracker::PendingOrderSummary,
    pending_bulk_shorts: &pending_order_tracker::PendingOrderSummary,
) -> u128 {
    let pending_order_tracker::PendingOrderSummary::V1 {
        price_size_sum: sl_pss,
        size_sum: sl_ss,
    } = pending_single_longs;
    let pending_order_tracker::PendingOrderSummary::V1 {
        price_size_sum: ss_pss,
        size_sum: ss_ss,
    } = pending_single_shorts;
    let pending_order_tracker::PendingOrderSummary::V1 {
        price_size_sum: bl_pss,
        size_sum: bl_ss,
    } = pending_bulk_longs;
    let pending_order_tracker::PendingOrderSummary::V1 {
        price_size_sum: bs_pss,
        size_sum: bs_ss,
    } = pending_bulk_shorts;

    let total_longs_size = sl_ss + bl_ss;
    let total_longs_price_size = sl_pss + bl_pss;
    let total_shorts_size = ss_ss + bs_ss;
    let total_shorts_price_size = ss_pss + bs_pss;

    if position_is_long {
        let effective_pending_short_size = if total_shorts_size > 2 * position_size {
            total_shorts_size - 2 * position_size
        } else {
            0
        };
        let short_notional = if total_shorts_size == 0 {
            0
        } else {
            (effective_pending_short_size as u128) * total_shorts_price_size
                / (total_shorts_size as u128)
        };
        std::cmp::max(total_longs_price_size, short_notional)
    } else {
        let effective_pending_long_size = if total_longs_size > 2 * position_size {
            total_longs_size - 2 * position_size
        } else {
            0
        };
        let long_notional = if total_longs_size == 0 {
            0
        } else {
            (effective_pending_long_size as u128) * total_longs_price_size
                / (total_longs_size as u128)
        };
        std::cmp::max(total_shorts_price_size, long_notional)
    }
}
