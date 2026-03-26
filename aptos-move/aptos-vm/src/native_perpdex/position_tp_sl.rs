// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::position_tp_sl
//
// Take-profit/stop-loss logic. This module wraps position_tp_sl_tracker
// and pending_order_tracker with position lookups.

use crate::native_perpdex::order_book_types::OrderId;
use crate::native_perpdex::pending_order_tracker::{GlobalSummary, PendingTpSlInfo};
use crate::native_perpdex::perp_positions::{PerpPosition, UserPositions};
use crate::native_perpdex::builder_code_registry::BuilderCode;
use crate::native_perpdex::position_tp_sl_tracker::{
    self, PendingOrderTracker, PendingRequest,
};

// ===================== Constants =====================

const EINVALID_TP_SL_ORDER_ID: u64 = 16;

// ===================== Types =====================

pub type PerpMarketRef = [u8; 32];

// ===================== Functions =====================

/// Dequeue all pending TP/SL orders that are ready to be executed
pub fn take_ready_tp_sl_orders(
    tp_sl_tracker: &mut PendingOrderTracker,
    global_summary: &mut GlobalSummary,
    user_positions: &UserPositions,
    market: PerpMarketRef,
    mark_price: u64,
    price_move_up: bool,
    limit: u32,
) -> Vec<PendingRequest> {
    let pending_orders = if price_move_up {
        position_tp_sl_tracker::take_ready_price_move_up_orders(
            tp_sl_tracker,
            mark_price,
            limit,
        )
    } else {
        position_tp_sl_tracker::take_ready_price_move_down_orders(
            tp_sl_tracker,
            mark_price,
            limit,
        )
    };

    for pending_order in &pending_orders {
        let account = pending_order.get_account();
        if let Some(position) = user_positions.positions().get(&market) {
            let is_position_long = position.is_long();
            let is_tp = (is_position_long && price_move_up)
                || (!is_position_long && !price_move_up);
            let is_full_sized = pending_order.get_size().is_none();
            let order_id = pending_order.get_order_id();

            // Remove from pending_order_tracker
            // In the full implementation this calls remove_full_sized_tp_sl_for_order
            // or remove_fixed_sized_tp_sl_for_order on global_summary
        }
    }

    pending_orders
}

/// Get full-sized TP order for an account and market
pub fn get_tp_order(
    global_summary: &GlobalSummary,
    tp_sl_tracker: &PendingOrderTracker,
    user_positions: &UserPositions,
    account: [u8; 32],
    market: PerpMarketRef,
) -> Option<PendingTpSlInfo> {
    get_full_sized_tp_sl_order(
        global_summary,
        tp_sl_tracker,
        user_positions,
        account,
        market,
        true,
    )
}

/// Get full-sized SL order for an account and market
pub fn get_sl_order(
    global_summary: &GlobalSummary,
    tp_sl_tracker: &PendingOrderTracker,
    user_positions: &UserPositions,
    account: [u8; 32],
    market: PerpMarketRef,
) -> Option<PendingTpSlInfo> {
    get_full_sized_tp_sl_order(
        global_summary,
        tp_sl_tracker,
        user_positions,
        account,
        market,
        false,
    )
}

fn get_full_sized_tp_sl_order(
    global_summary: &GlobalSummary,
    tp_sl_tracker: &PendingOrderTracker,
    user_positions: &UserPositions,
    account: [u8; 32],
    market: PerpMarketRef,
    is_tp: bool,
) -> Option<PendingTpSlInfo> {
    let position = user_positions.positions().get(&market)?;
    let position_is_long = position.is_long();
    // Delegate to global_summary's get_full_sized_tp_sl_order
    // This is a simplified version - full impl reads from global_summary
    None // Placeholder: full implementation reads from pending_order_tracker state
}

/// Validate TP/SL trigger price against current mark price
pub fn validate_tp_sl(
    position: &PerpPosition,
    mark_price: u64,
    trigger_price: u64,
    is_tp: bool,
) -> bool {
    let is_position_long = position.is_long();
    if (is_position_long && is_tp) || (!is_position_long && !is_tp) {
        trigger_price > mark_price
    } else {
        trigger_price < mark_price
    }
}

/// Add TP/SL order
pub fn add_tp_sl(
    tp_sl_tracker: &mut PendingOrderTracker,
    global_summary: &mut GlobalSummary,
    position: &PerpPosition,
    account: [u8; 32],
    market: PerpMarketRef,
    order_id: OrderId,
    trigger_price: u64,
    limit_price: Option<u64>,
    size: Option<u64>,
    is_tp: bool,
    builder_code: Option<BuilderCode>,
    allow_abort: bool,
) {
    let position_size = position.get_size();
    let position_is_long = position.is_long();

    // Delegate to pending_order_tracker::add_tp_sl and position_tp_sl_tracker::add_new_tp_sl
    let price_index = position_tp_sl_tracker::new_price_index_key(
        trigger_price,
        account,
        limit_price,
        size.is_none(),
        builder_code,
    );

    position_tp_sl_tracker::add_new_tp_sl(
        tp_sl_tracker,
        account,
        order_id,
        price_index,
        limit_price,
        size,
        is_tp,
        position_is_long,
    );
    // EVENT: PositionUpdateEvent
}

/// Cancel TP/SL order
pub fn cancel_tp_sl(
    tp_sl_tracker: &mut PendingOrderTracker,
    global_summary: &mut GlobalSummary,
    user_positions: &UserPositions,
    account: [u8; 32],
    market: PerpMarketRef,
    order_id: OrderId,
) -> Result<(), u64> {
    let position = user_positions
        .positions()
        .get(&market)
        .ok_or(EINVALID_TP_SL_ORDER_ID)?;
    let _position_is_long = position.is_long();

    // Delegate to pending_order_tracker::cancel_tp_sl
    // Simplified version - full impl searches through all TP/SL types
    Err(EINVALID_TP_SL_ORDER_ID)
}

/// Increase TP/SL size
pub fn increase_tp_sl_size(
    tp_sl_tracker: &mut PendingOrderTracker,
    global_summary: &mut GlobalSummary,
    position: &PerpPosition,
    account: [u8; 32],
    market: PerpMarketRef,
    trigger_price: u64,
    limit_price: Option<u64>,
    builder_code: Option<BuilderCode>,
    size_delta: u64,
    is_tp: bool,
    mark_price: u64,
) {
    let position_is_long = position.is_long();
    let position_size = position.get_size();

    // Validate trigger price
    let valid = if (position_is_long && is_tp) || (!position_is_long && !is_tp) {
        trigger_price > mark_price
    } else {
        trigger_price < mark_price
    };
    assert!(valid, "Invalid trigger price: {}", 5);

    let price_index = position_tp_sl_tracker::new_price_index_key(
        trigger_price,
        account,
        limit_price,
        false,
        builder_code,
    );

    position_tp_sl_tracker::increase_fixed_sized_pending_tp_sl_size(
        tp_sl_tracker,
        &price_index,
        size_delta,
        is_tp,
        position_is_long,
        position_size,
    );
    // EVENT: PositionUpdateEvent
}


// ===================== Stub functions and types for perp_engine delegation =====================

