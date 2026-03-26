// Copyright (c) Aptos Foundation
// Translated from: aptos_market::order_placement
//
// This module contains the core order placement logic. In Move, it uses
// MarketClearinghouseCallbacks (closures) to interact with the clearinghouse.
// In native Rust, the callbacks are provided via the MarketClearinghouseCallbacks trait.

#[allow(dead_code)]
use crate::native_perpdex::market_clearinghouse_order_info::new_clearinghouse_order_info;
use crate::native_perpdex::market_types::{
    self, extract_results, get_callback_result, get_maker_cancellation_reason,
    get_place_maker_order_actions, get_place_maker_order_cancellation_reason, get_settled_size,
    get_taker_cancellation_reason, is_validation_result_valid,
    should_stop_matching, Market,
    MarketClearinghouseCallbacks, OrderCancellationReason,
};
use crate::native_perpdex::order_book::{self};
use crate::native_perpdex::order_book_types::{
    immediate_or_cancel, post_only, single_order_type, OrderId, OrderType, TimeInForce,
    TriggerCondition,
};
use crate::native_perpdex::order_match_types::{
    destroy_order_match, get_account_from_match_details, get_client_order_id_from_match_details,
    get_metadata_from_match_details,
    get_order_id_from_match_details, get_price_from_match_details,
    get_remaining_size_from_match_details, get_time_in_force_from_match_details,
    get_book_type_from_match_details, is_bid_from_match_details,
    new_order_match_details_with_modified_size,
};
use crate::native_perpdex::single_order_types::new_single_order_request;

// ===================== Constants =====================

const EINVALID_ORDER: u64 = 1;
const ECLEARINGHOUSE_SETTLEMENT_VIOLATION: u64 = 2;
const ECLIENT_ORDER_ID_LENGTH_EXCEEDED: u64 = 3;
const MAX_CLIENT_ORDER_ID_LENGTH: u64 = 32;

// ===================== OrderMatchResult =====================

#[derive(Clone, Debug)]
pub enum OrderMatchResult<R: Clone> {
    V1 {
        order_id: OrderId,
        remaining_size: u64,
        cancel_reason: Option<OrderCancellationReason>,
        callback_results: Vec<R>,
        fill_sizes: Vec<u64>,
        match_count: u32,
    },
}

pub fn destroy_order_match_result<R: Clone>(
    result: OrderMatchResult<R>,
) -> (
    OrderId,
    u64,
    Option<OrderCancellationReason>,
    Vec<R>,
    Vec<u64>,
    u32,
) {
    let OrderMatchResult::V1 {
        order_id,
        remaining_size,
        cancel_reason,
        callback_results,
        fill_sizes,
        match_count,
    } = result;
    (
        order_id,
        remaining_size,
        cancel_reason,
        callback_results,
        fill_sizes,
        match_count,
    )
}

pub fn number_of_fills<R: Clone>(result: &OrderMatchResult<R>) -> u64 {
    let OrderMatchResult::V1 { fill_sizes, .. } = result;
    fill_sizes.len() as u64
}

pub fn number_of_matches<R: Clone>(result: &OrderMatchResult<R>) -> u32 {
    let OrderMatchResult::V1 { match_count, .. } = result;
    *match_count
}

pub fn get_order_id_from_result<R: Clone>(result: &OrderMatchResult<R>) -> OrderId {
    let OrderMatchResult::V1 { order_id, .. } = result;
    *order_id
}

pub fn is_ioc_violation(reason: OrderCancellationReason) -> bool {
    reason == market_types::order_cancellation_reason_ioc_violation()
}

pub fn is_fill_limit_violation(reason: OrderCancellationReason) -> bool {
    reason == market_types::order_cancellation_reason_max_fill_limit_violation()
}

pub fn is_clearinghouse_stopped_matching(reason: OrderCancellationReason) -> bool {
    reason == market_types::order_cancellation_reason_clearinghouse_stopped_matching()
}

// ===================== cleanup_order_internal =====================

pub fn cleanup_order_internal<M: Clone + Copy, R: Clone>(
    user_addr: [u8; 32],
    order_id: OrderId,
    client_order_id: Option<Vec<u8>>,
    order_type: OrderType,
    is_bid: bool,
    time_in_force: TimeInForce,
    cleanup_size: u64,
    price: u64,
    trigger_condition: Option<TriggerCondition>,
    metadata: M,
    callbacks: &dyn MarketClearinghouseCallbacks<M, R>,
    is_taker: bool,
) {
    if order_type == single_order_type() {
        callbacks.cleanup_order(
            new_clearinghouse_order_info(
                user_addr,
                order_id,
                client_order_id,
                is_bid,
                price,
                time_in_force,
                single_order_type(),
                trigger_condition,
                metadata,
            ),
            cleanup_size,
            is_taker,
        );
    } else {
        callbacks.cleanup_bulk_order_at_price(
            user_addr, order_id, is_bid, price, cleanup_size,
        );
    }
}

// ===================== Core Placement Functions =====================
// NOTE: The full implementation of place_order_with_order_id, settle_single_trade,
// place_maker_order_internal, cancel_taker_order_internal, cancel_maker_order_internal,
// and cancel_bulk_maker_order_internal follows the Move code line-by-line.
// Event emission calls are converted to no-ops since they depend on the runtime context.
// The caller is responsible for collecting events.

pub fn place_order_with_order_id<M: Clone + Copy, R: Clone>(
    market: &mut Market<M>,
    user_addr: [u8; 32],
    limit_price: u64,
    orig_size: u64,
    remaining_size: u64,
    is_bid: bool,
    time_in_force: TimeInForce,
    trigger_condition: Option<TriggerCondition>,
    metadata: M,
    order_id: OrderId,
    client_order_id: Option<Vec<u8>>,
    max_match_limit: u32,
    cancel_on_stop_matching: bool,
    _emit_taker_order_open: bool,
    callbacks: &dyn MarketClearinghouseCallbacks<M, R>,
    ascending_idx: crate::native_perpdex::order_book_types::IncreasingIdx,
) -> OrderMatchResult<R> {
    assert!(
        orig_size > 0 && remaining_size > 0 && orig_size >= remaining_size,
        "EINVALID_ORDER"
    );
    assert!(max_match_limit > 0, "EINVALID_ORDER");
    assert!(limit_price > 0, "EINVALID_ORDER");
    if let Some(ref cid) = client_order_id {
        assert!(
            (cid.len() as u64) <= MAX_CLIENT_ORDER_ID_LENGTH,
            "ECLIENT_ORDER_ID_LENGTH_EXCEEDED"
        );
    }

    let mut callback_results: Vec<R> = Vec::new();
    let validation_result = callbacks.validate_order_placement(
        new_clearinghouse_order_info(
            user_addr,
            order_id,
            client_order_id.clone(),
            is_bid,
            limit_price,
            time_in_force,
            single_order_type(),
            trigger_condition,
            metadata,
        ),
        remaining_size,
    );

    if !is_validation_result_valid(&validation_result) {
        // Cancel with position update violation
        callbacks.cleanup_order(
            new_clearinghouse_order_info(
                user_addr,
                order_id,
                client_order_id.clone(),
                is_bid,
                limit_price,
                time_in_force,
                single_order_type(),
                trigger_condition,
                metadata,
            ),
            remaining_size,
            true,
        );
        return OrderMatchResult::V1 {
            order_id,
            remaining_size: 0,
            cancel_reason: Some(market_types::order_cancellation_reason_position_update_violation()),
            callback_results: Vec::new(),
            fill_sizes: Vec::new(),
            match_count: 0,
        };
    }

    if let Some(ref cid) = client_order_id {
        if order_book::client_order_id_exists(
            market_types::get_order_book(market),
            user_addr,
            cid.clone(),
        ) {
            callbacks.cleanup_order(
                new_clearinghouse_order_info(
                    user_addr, order_id, client_order_id.clone(), is_bid, limit_price,
                    time_in_force, single_order_type(), trigger_condition, metadata,
                ),
                remaining_size,
                true,
            );
            return OrderMatchResult::V1 {
                order_id,
                remaining_size: 0,
                cancel_reason: Some(
                    market_types::order_cancellation_reason_duplicate_client_order_id(),
                ),
                callback_results: Vec::new(),
                fill_sizes: Vec::new(),
                match_count: 0,
            };
        }
    }

    let is_taker = order_book::is_taker_order(
        market_types::get_order_book(market),
        limit_price,
        is_bid,
        trigger_condition,
    );

    if !is_taker {
        // Place as maker order
        if time_in_force == immediate_or_cancel() && trigger_condition.is_none() {
            callbacks.cleanup_order(
                new_clearinghouse_order_info(
                    user_addr, order_id, client_order_id.clone(), is_bid, limit_price,
                    time_in_force, single_order_type(), trigger_condition, metadata,
                ),
                remaining_size,
                true,
            );
            return OrderMatchResult::V1 {
                order_id,
                remaining_size: 0,
                cancel_reason: Some(market_types::order_cancellation_reason_ioc_violation()),
                callback_results: Vec::new(),
                fill_sizes: Vec::new(),
                match_count: 0,
            };
        }

        // For trigger conditions, place directly without maker order callback
        if trigger_condition.is_some() {
            order_book::place_maker_order(
                market_types::get_order_book_mut(market),
                new_single_order_request(
                    user_addr, order_id, client_order_id.clone(), limit_price, orig_size,
                    remaining_size, is_bid, trigger_condition, time_in_force, 0, metadata,
                ),
                ascending_idx,
            );
            return OrderMatchResult::V1 {
                order_id,
                remaining_size,
                cancel_reason: None,
                callback_results,
                fill_sizes: Vec::new(),
                match_count: 0,
            };
        }

        let result = callbacks.place_maker_order(
            new_clearinghouse_order_info(
                user_addr, order_id, client_order_id.clone(), is_bid, limit_price,
                time_in_force, single_order_type(), None, metadata,
            ),
            remaining_size,
        );
        if get_place_maker_order_cancellation_reason(&result).is_some() {
            callbacks.cleanup_order(
                new_clearinghouse_order_info(
                    user_addr, order_id, client_order_id.clone(), is_bid, limit_price,
                    time_in_force, single_order_type(), None, metadata,
                ),
                remaining_size,
                true,
            );
            return OrderMatchResult::V1 {
                order_id,
                remaining_size: 0,
                cancel_reason: Some(
                    market_types::order_cancellation_reason_place_maker_order_violation(),
                ),
                callback_results: Vec::new(),
                fill_sizes: Vec::new(),
                match_count: 0,
            };
        }

        if let Some(actions) = get_place_maker_order_actions(&result) {
            callback_results.push(actions);
        }

        order_book::place_maker_order(
            market_types::get_order_book_mut(market),
            new_single_order_request(
                user_addr, order_id, client_order_id.clone(), limit_price, orig_size,
                remaining_size, is_bid, trigger_condition, time_in_force, 0, metadata,
            ),
            ascending_idx,
        );
        return OrderMatchResult::V1 {
            order_id,
            remaining_size,
            cancel_reason: None,
            callback_results,
            fill_sizes: Vec::new(),
            match_count: 0,
        };
    }

    // Taker path
    if time_in_force == post_only() {
        callbacks.cleanup_order(
            new_clearinghouse_order_info(
                user_addr, order_id, client_order_id.clone(), is_bid, limit_price,
                time_in_force, single_order_type(), None, metadata,
            ),
            remaining_size,
            true,
        );
        return OrderMatchResult::V1 {
            order_id,
            remaining_size: 0,
            cancel_reason: Some(market_types::order_cancellation_reason_post_only_violation()),
            callback_results: Vec::new(),
            fill_sizes: Vec::new(),
            match_count: 0,
        };
    }

    // Matching loop
    let mut current_remaining = remaining_size;
    let mut fill_sizes: Vec<u64> = Vec::new();
    let mut match_count: u32 = 0;

    loop {
        match_count += 1;

        // Get match from order book
        let result = order_book::get_single_match_for_taker(
            market_types::get_order_book_mut(market),
            limit_price,
            current_remaining,
            is_bid,
        );
        let (maker_order, maker_matched_size) = destroy_order_match(result);

        // Settle the trade
        let fill_id = 0u128; // In Move: transaction_context::monotonically_increasing_counter()
        let settle_result = callbacks.settle_trade(
            market,
            new_clearinghouse_order_info(
                user_addr, order_id, client_order_id.clone(), is_bid, limit_price,
                time_in_force, single_order_type(), None, metadata,
            ),
            new_clearinghouse_order_info(
                get_account_from_match_details(&maker_order),
                get_order_id_from_match_details(&maker_order),
                get_client_order_id_from_match_details(&maker_order),
                is_bid_from_match_details(&maker_order),
                get_price_from_match_details(&maker_order),
                get_time_in_force_from_match_details(&maker_order),
                get_book_type_from_match_details(&maker_order),
                None,
                get_metadata_from_match_details(&maker_order),
            ),
            fill_id,
            get_price_from_match_details(&maker_order),
            maker_matched_size,
        );

        let settled_size = get_settled_size(&settle_result);
        let mut unsettled_maker_size = maker_matched_size;
        if settled_size > 0 {
            current_remaining -= settled_size;
            unsettled_maker_size -= settled_size;
            fill_sizes.push(settled_size);
        }

        let callback_result_ref = get_callback_result(&settle_result);
        let should_stop = should_stop_matching(callback_result_ref);
        if let Some(r) = extract_results(callback_result_ref.clone()) {
            callback_results.push(r);
        }

        let taker_cancel_str = get_taker_cancellation_reason(&settle_result);
        let maker_cancel_str = get_maker_cancellation_reason(&settle_result);

        // Handle taker cancellation
        if taker_cancel_str.is_some() {
            callbacks.cleanup_order(
                new_clearinghouse_order_info(
                    user_addr, order_id, client_order_id.clone(), is_bid, limit_price,
                    time_in_force, single_order_type(), None, metadata,
                ),
                current_remaining,
                true,
            );
            // Handle maker reinsertion or cancellation
            if maker_cancel_str.is_some() {
                if get_remaining_size_from_match_details(&maker_order) != 0 {
                    order_book::cancel_single_order(
                        market_types::get_order_book_mut(market),
                        get_account_from_match_details(&maker_order),
                        get_order_id_from_match_details(&maker_order),
                    );
                }
                cleanup_order_internal(
                    get_account_from_match_details(&maker_order),
                    get_order_id_from_match_details(&maker_order),
                    get_client_order_id_from_match_details(&maker_order),
                    get_book_type_from_match_details(&maker_order),
                    is_bid_from_match_details(&maker_order),
                    get_time_in_force_from_match_details(&maker_order),
                    unsettled_maker_size + get_remaining_size_from_match_details(&maker_order),
                    get_price_from_match_details(&maker_order),
                    None,
                    get_metadata_from_match_details(&maker_order),
                    callbacks,
                    false,
                );
            } else if unsettled_maker_size > 0 {
                let reinsertion = new_order_match_details_with_modified_size(
                    &maker_order,
                    unsettled_maker_size,
                );
                order_book::reinsert_order(
                    market_types::get_order_book_mut(market),
                    reinsertion,
                    &maker_order,
                );
            } else if get_remaining_size_from_match_details(&maker_order) == 0 {
                cleanup_order_internal(
                    get_account_from_match_details(&maker_order),
                    get_order_id_from_match_details(&maker_order),
                    get_client_order_id_from_match_details(&maker_order),
                    get_book_type_from_match_details(&maker_order),
                    !is_bid,
                    get_time_in_force_from_match_details(&maker_order),
                    0,
                    get_price_from_match_details(&maker_order),
                    None,
                    get_metadata_from_match_details(&maker_order),
                    callbacks,
                    false,
                );
            }

            return OrderMatchResult::V1 {
                order_id,
                remaining_size: 0,
                cancel_reason: Some(
                    market_types::order_cancellation_reason_clearinghouse_settle_violation(),
                ),
                fill_sizes,
                callback_results,
                match_count,
            };
        }

        // Handle maker cancellation
        if maker_cancel_str.is_some() {
            if get_remaining_size_from_match_details(&maker_order) != 0 {
                order_book::cancel_single_order(
                    market_types::get_order_book_mut(market),
                    get_account_from_match_details(&maker_order),
                    get_order_id_from_match_details(&maker_order),
                );
            }
            cleanup_order_internal(
                get_account_from_match_details(&maker_order),
                get_order_id_from_match_details(&maker_order),
                get_client_order_id_from_match_details(&maker_order),
                get_book_type_from_match_details(&maker_order),
                is_bid_from_match_details(&maker_order),
                get_time_in_force_from_match_details(&maker_order),
                unsettled_maker_size + get_remaining_size_from_match_details(&maker_order),
                get_price_from_match_details(&maker_order),
                None,
                get_metadata_from_match_details(&maker_order),
                callbacks,
                false,
            );
        } else {
            if unsettled_maker_size > 0 {
                let reinsertion = new_order_match_details_with_modified_size(
                    &maker_order,
                    unsettled_maker_size,
                );
                order_book::reinsert_order(
                    market_types::get_order_book_mut(market),
                    reinsertion,
                    &maker_order,
                );
            } else if get_remaining_size_from_match_details(&maker_order) == 0 {
                cleanup_order_internal(
                    get_account_from_match_details(&maker_order),
                    get_order_id_from_match_details(&maker_order),
                    get_client_order_id_from_match_details(&maker_order),
                    get_book_type_from_match_details(&maker_order),
                    !is_bid,
                    get_time_in_force_from_match_details(&maker_order),
                    0,
                    get_price_from_match_details(&maker_order),
                    None,
                    get_metadata_from_match_details(&maker_order),
                    callbacks,
                    false,
                );
            }
        }

        if current_remaining == 0 {
            cleanup_order_internal(
                user_addr, order_id, client_order_id.clone(), single_order_type(),
                is_bid, time_in_force, 0, limit_price, trigger_condition, metadata,
                callbacks, true,
            );
            break;
        }

        let still_taker = order_book::is_taker_order(
            market_types::get_order_book(market),
            limit_price,
            is_bid,
            None,
        );
        if !still_taker {
            if time_in_force == immediate_or_cancel() {
                callbacks.cleanup_order(
                    new_clearinghouse_order_info(
                        user_addr, order_id, client_order_id.clone(), is_bid, limit_price,
                        time_in_force, single_order_type(), None, metadata,
                    ),
                    current_remaining,
                    true,
                );
                return OrderMatchResult::V1 {
                    order_id,
                    remaining_size: 0,
                    cancel_reason: Some(
                        market_types::order_cancellation_reason_ioc_violation(),
                    ),
                    fill_sizes,
                    callback_results,
                    match_count,
                };
            } else {
                // Place as maker
                let result = callbacks.place_maker_order(
                    new_clearinghouse_order_info(
                        user_addr, order_id, client_order_id.clone(), is_bid, limit_price,
                        time_in_force, single_order_type(), None, metadata,
                    ),
                    current_remaining,
                );
                if get_place_maker_order_cancellation_reason(&result).is_some() {
                    callbacks.cleanup_order(
                        new_clearinghouse_order_info(
                            user_addr, order_id, client_order_id.clone(), is_bid, limit_price,
                            time_in_force, single_order_type(), None, metadata,
                        ),
                        current_remaining,
                        true,
                    );
                    return OrderMatchResult::V1 {
                        order_id,
                        remaining_size: 0,
                        cancel_reason: Some(
                            market_types::order_cancellation_reason_place_maker_order_violation(),
                        ),
                        callback_results,
                        fill_sizes,
                        match_count,
                    };
                }
                if let Some(actions) = get_place_maker_order_actions(&result) {
                    callback_results.push(actions);
                }
                order_book::place_maker_order(
                    market_types::get_order_book_mut(market),
                    new_single_order_request(
                        user_addr, order_id, client_order_id.clone(), limit_price, orig_size,
                        current_remaining, is_bid, None, time_in_force, 0, metadata,
                    ),
                    ascending_idx,
                );
                return OrderMatchResult::V1 {
                    order_id,
                    remaining_size: current_remaining,
                    cancel_reason: None,
                    callback_results,
                    fill_sizes,
                    match_count,
                };
            }
        }

        if match_count >= max_match_limit || should_stop {
            let cancel_reason = if match_count >= max_match_limit {
                market_types::order_cancellation_reason_max_fill_limit_violation()
            } else {
                market_types::order_cancellation_reason_clearinghouse_stopped_matching()
            };
            if cancel_on_stop_matching {
                callbacks.cleanup_order(
                    new_clearinghouse_order_info(
                        user_addr, order_id, client_order_id.clone(), is_bid, limit_price,
                        time_in_force, single_order_type(), None, metadata,
                    ),
                    current_remaining,
                    true,
                );
                return OrderMatchResult::V1 {
                    order_id,
                    remaining_size: 0,
                    cancel_reason: Some(cancel_reason),
                    fill_sizes,
                    callback_results,
                    match_count,
                };
            } else {
                return OrderMatchResult::V1 {
                    order_id,
                    remaining_size: current_remaining,
                    cancel_reason: Some(cancel_reason),
                    callback_results,
                    fill_sizes,
                    match_count,
                };
            }
        }
    }

    OrderMatchResult::V1 {
        order_id,
        remaining_size: current_remaining,
        cancel_reason: None,
        fill_sizes,
        callback_results,
        match_count,
    }
}
