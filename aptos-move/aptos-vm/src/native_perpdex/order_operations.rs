// Copyright (c) Aptos Foundation
// Translated from: aptos_market::order_operations

use crate::native_perpdex::market_clearinghouse_order_info::new_clearinghouse_order_info;
use crate::native_perpdex::market_types::{
    self, Market, MarketClearinghouseCallbacks, OrderCancellationReason,
};
use crate::native_perpdex::order_book::{self};
use crate::native_perpdex::order_book_types::{single_order_type, OrderId};
use crate::native_perpdex::order_placement::cleanup_order_internal;
use crate::native_perpdex::single_order_types::{
    destroy_single_order, destroy_single_order_request, destroy_order_from_state,
    SingleOrder,
};

// ===================== cancel_order_with_client_id =====================

pub fn cancel_order_with_client_id<M: Clone + Copy, R: Clone>(
    market: &mut Market<M>,
    user: [u8; 32],
    client_order_id: Vec<u8>,
    cancellation_reason: OrderCancellationReason,
    _cancel_reason: Vec<u8>,
    callbacks: &dyn MarketClearinghouseCallbacks<M, R>,
) {
    let order = order_book::try_cancel_single_order_with_client_order_id(
        market_types::get_order_book_mut(market),
        user,
        client_order_id.clone(),
    );
    if let Some(ord) = order {
        cancel_single_order_helper(market, ord, true, cancellation_reason, _cancel_reason, callbacks);
        return;
    }
    // pre_cancel_order would be called here but requires secondary resources
    // In native context, this is handled by the caller
}

// ===================== cancel_order =====================

pub fn cancel_order<M: Clone + Copy, R: Clone>(
    market: &mut Market<M>,
    account: [u8; 32],
    order_id: OrderId,
    emit_event: bool,
    cancellation_reason: OrderCancellationReason,
    cancel_reason: Vec<u8>,
    callbacks: &dyn MarketClearinghouseCallbacks<M, R>,
) -> SingleOrder<M> {
    let order = order_book::cancel_single_order(
        market_types::get_order_book_mut(market),
        account,
        order_id,
    );
    cancel_single_order_helper(
        market,
        order.clone(),
        emit_event,
        cancellation_reason,
        cancel_reason,
        callbacks,
    );
    order
}

// ===================== try_cancel_order =====================

pub fn try_cancel_order<M: Clone + Copy, R: Clone>(
    market: &mut Market<M>,
    account: [u8; 32],
    order_id: OrderId,
    emit_event: bool,
    cancellation_reason: OrderCancellationReason,
    cancel_reason: Vec<u8>,
    callbacks: &dyn MarketClearinghouseCallbacks<M, R>,
) -> Option<SingleOrder<M>> {
    let maybe_order = order_book::try_cancel_single_order(
        market_types::get_order_book_mut(market),
        account,
        order_id,
    );
    match maybe_order {
        Some(order) => {
            cancel_single_order_helper(
                market,
                order.clone(),
                emit_event,
                cancellation_reason,
                cancel_reason,
                callbacks,
            );
            Some(order)
        },
        None => None,
    }
}

// ===================== decrease_order_size =====================

pub fn decrease_order_size<M: Clone + Copy, R: Clone>(
    market: &mut Market<M>,
    account: [u8; 32],
    order_id: OrderId,
    size_delta: u64,
    callbacks: &dyn MarketClearinghouseCallbacks<M, R>,
) {
    let ob = market_types::get_order_book_mut(market);
    order_book::decrease_single_order_size(ob, account, order_id, size_delta);
    let order_with_state = order_book::get_single_order(ob, order_id).unwrap();
    let (order, _) = destroy_order_from_state(order_with_state);
    let (order_request, _unique_priority_idx) = destroy_single_order(order);
    let (
        user,
        oid,
        client_order_id,
        price,
        _orig_size,
        remaining_size,
        is_bid,
        trigger_condition,
        time_in_force,
        _creation_time_micros,
        metadata,
    ) = destroy_single_order_request(order_request);
    callbacks.decrease_order_size(
        new_clearinghouse_order_info(
            user,
            oid,
            client_order_id,
            is_bid,
            price,
            time_in_force,
            single_order_type(),
            trigger_condition,
            metadata,
        ),
        remaining_size,
    );
    // Event emission omitted - caller handles events
}

// ===================== cancel_single_order_helper =====================

pub fn cancel_single_order_helper<M: Clone + Copy, R: Clone>(
    _market: &mut Market<M>,
    order: SingleOrder<M>,
    _emit_event: bool,
    _cancellation_reason: OrderCancellationReason,
    _cancel_reason: Vec<u8>,
    callbacks: &dyn MarketClearinghouseCallbacks<M, R>,
) {
    let (order_request, _unique_priority_idx) = destroy_single_order(order);
    let (
        account,
        order_id,
        client_order_id,
        price,
        _orig_size,
        remaining_size,
        is_bid,
        trigger_condition,
        time_in_force,
        _creation_time_micros,
        metadata,
    ) = destroy_single_order_request(order_request);

    cleanup_order_internal(
        account,
        order_id,
        client_order_id,
        single_order_type(),
        is_bid,
        time_in_force,
        remaining_size,
        price,
        trigger_condition,
        metadata,
        callbacks,
        false,
    );
    // Event emission omitted - caller handles events
}
