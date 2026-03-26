// Copyright (c) Aptos Foundation
// Translated from: aptos_market::order_book

use crate::native_perpdex::bulk_order_book::{self, BulkOrderBook};
use crate::native_perpdex::bulk_order_types::{BulkOrder, BulkOrderPlaceResponse, BulkOrderRequest};
use crate::native_perpdex::order_book_types::{IncreasingIdx, OrderId, TriggerCondition};
use crate::native_perpdex::order_match_types::{
    is_active_matched_book_type_single_order, OrderMatch, OrderMatchDetails,
    is_single_order_from_match_details,
};
use crate::native_perpdex::price_time_index::{self, PriceTimeIndex};
use crate::native_perpdex::single_order_book::{self, SingleOrderBook};
use crate::native_perpdex::single_order_types::{OrderWithState, SingleOrder, SingleOrderRequest};

// ===================== OrderBook =====================

#[derive(Clone, Debug)]
pub enum OrderBook<M: Clone> {
    UnifiedV1 {
        single_order_book: SingleOrderBook<M>,
        bulk_order_book: BulkOrderBook<M>,
        price_time_idx: PriceTimeIndex,
    },
}

pub fn new_order_book<M: Clone>() -> OrderBook<M> {
    OrderBook::UnifiedV1 {
        single_order_book: single_order_book::new_single_order_book(),
        bulk_order_book: bulk_order_book::new_bulk_order_book(),
        price_time_idx: price_time_index::new_price_time_idx(),
    }
}

// ===================== Single order APIs =====================

pub fn client_order_id_exists<M: Clone>(
    book: &OrderBook<M>,
    order_creator: [u8; 32],
    client_order_id: Vec<u8>,
) -> bool {
    let OrderBook::UnifiedV1 {
        single_order_book, ..
    } = book;
    single_order_book::client_order_id_exists(single_order_book, order_creator, client_order_id)
}

pub fn get_single_order_metadata<M: Clone + Copy>(
    book: &OrderBook<M>,
    order_id: OrderId,
) -> Option<M> {
    let OrderBook::UnifiedV1 {
        single_order_book, ..
    } = book;
    single_order_book::get_order_metadata(single_order_book, order_id)
}

pub fn get_order_id_by_client_id<M: Clone>(
    book: &OrderBook<M>,
    order_creator: [u8; 32],
    client_order_id: Vec<u8>,
) -> Option<OrderId> {
    let OrderBook::UnifiedV1 {
        single_order_book, ..
    } = book;
    single_order_book::get_order_id_by_client_id(single_order_book, order_creator, client_order_id)
}

pub fn get_single_order<M: Clone>(
    book: &OrderBook<M>,
    order_id: OrderId,
) -> Option<OrderWithState<M>> {
    let OrderBook::UnifiedV1 {
        single_order_book, ..
    } = book;
    single_order_book::get_order(single_order_book, order_id)
}

pub fn get_single_remaining_size<M: Clone>(book: &OrderBook<M>, order_id: OrderId) -> u64 {
    let OrderBook::UnifiedV1 {
        single_order_book, ..
    } = book;
    single_order_book::get_remaining_size(single_order_book, order_id)
}

pub fn cancel_single_order<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    order_creator: [u8; 32],
    order_id: OrderId,
) -> SingleOrder<M> {
    let OrderBook::UnifiedV1 {
        single_order_book,
        price_time_idx,
        ..
    } = book;
    single_order_book::cancel_order(single_order_book, price_time_idx, order_creator, order_id)
}

pub fn try_cancel_single_order<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    order_creator: [u8; 32],
    order_id: OrderId,
) -> Option<SingleOrder<M>> {
    let OrderBook::UnifiedV1 {
        single_order_book,
        price_time_idx,
        ..
    } = book;
    single_order_book::try_cancel_order(
        single_order_book,
        price_time_idx,
        order_creator,
        order_id,
    )
}

pub fn try_cancel_single_order_with_client_order_id<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    order_creator: [u8; 32],
    client_order_id: Vec<u8>,
) -> Option<SingleOrder<M>> {
    let OrderBook::UnifiedV1 {
        single_order_book,
        price_time_idx,
        ..
    } = book;
    single_order_book::try_cancel_order_with_client_order_id(
        single_order_book,
        price_time_idx,
        order_creator,
        client_order_id,
    )
}

pub fn place_maker_order<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    order_req: SingleOrderRequest<M>,
    ascending_idx: IncreasingIdx,
) {
    let OrderBook::UnifiedV1 {
        single_order_book,
        price_time_idx,
        ..
    } = book;
    single_order_book::place_maker_or_pending_order(
        single_order_book,
        price_time_idx,
        order_req,
        ascending_idx,
    );
}

pub fn decrease_single_order_size<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    order_creator: [u8; 32],
    order_id: OrderId,
    size_delta: u64,
) {
    let OrderBook::UnifiedV1 {
        single_order_book,
        price_time_idx,
        ..
    } = book;
    single_order_book::decrease_order_size(
        single_order_book,
        price_time_idx,
        order_creator,
        order_id,
        size_delta,
    );
}

pub fn set_single_order_metadata<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    order_id: OrderId,
    metadata: M,
) {
    let OrderBook::UnifiedV1 {
        single_order_book, ..
    } = book;
    single_order_book::set_order_metadata(single_order_book, order_id, metadata);
}

pub fn take_ready_price_based_orders<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    oracle_price: u64,
    order_limit: u64,
) -> Vec<SingleOrder<M>> {
    let OrderBook::UnifiedV1 {
        single_order_book, ..
    } = book;
    single_order_book::take_ready_price_based_orders(single_order_book, oracle_price, order_limit)
}

pub fn take_ready_time_based_orders<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    order_limit: u64,
    current_time_secs: u64,
) -> Vec<SingleOrder<M>> {
    let OrderBook::UnifiedV1 {
        single_order_book, ..
    } = book;
    single_order_book::take_ready_time_based_orders(
        single_order_book,
        order_limit,
        current_time_secs,
    )
}

// ===================== APIs for both single and bulk =====================

pub fn best_bid_price<M: Clone>(book: &OrderBook<M>) -> Option<u64> {
    let OrderBook::UnifiedV1 { price_time_idx, .. } = book;
    price_time_index::best_bid_price(price_time_idx)
}

pub fn best_ask_price<M: Clone>(book: &OrderBook<M>) -> Option<u64> {
    let OrderBook::UnifiedV1 { price_time_idx, .. } = book;
    price_time_index::best_ask_price(price_time_idx)
}

pub fn get_slippage_price<M: Clone>(
    book: &OrderBook<M>,
    is_bid_side: bool,
    slippage_bps: u64,
) -> Option<u64> {
    let OrderBook::UnifiedV1 { price_time_idx, .. } = book;
    price_time_index::get_slippage_price(price_time_idx, is_bid_side, slippage_bps)
}

pub fn is_taker_order<M: Clone>(
    book: &OrderBook<M>,
    price: u64,
    is_bid_side: bool,
    trigger_condition: Option<TriggerCondition>,
) -> bool {
    if trigger_condition.is_some() {
        return false;
    }
    let OrderBook::UnifiedV1 { price_time_idx, .. } = book;
    price_time_index::is_taker_order(price_time_idx, price, is_bid_side)
}

pub fn get_single_match_for_taker<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    price: u64,
    size: u64,
    is_bid_side: bool,
) -> OrderMatch<M> {
    let OrderBook::UnifiedV1 {
        single_order_book,
        bulk_order_book,
        price_time_idx,
    } = book;
    let result = price_time_index::get_single_match_result(price_time_idx, price, size, is_bid_side);
    if is_active_matched_book_type_single_order(&result) {
        single_order_book::get_single_match_for_taker(single_order_book, result)
    } else {
        bulk_order_book::get_single_match_for_taker(
            bulk_order_book,
            price_time_idx,
            result,
            is_bid_side,
        )
    }
}

pub fn reinsert_order<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    reinsert_order_details: OrderMatchDetails<M>,
    original_order: &OrderMatchDetails<M>,
) {
    let OrderBook::UnifiedV1 {
        single_order_book,
        bulk_order_book,
        price_time_idx,
    } = book;
    if is_single_order_from_match_details(&reinsert_order_details) {
        single_order_book::reinsert_order(
            single_order_book,
            price_time_idx,
            reinsert_order_details,
            original_order,
        );
    } else {
        bulk_order_book::reinsert_order(
            bulk_order_book,
            price_time_idx,
            reinsert_order_details,
            original_order,
        );
    }
}

// ===================== Bulk order APIs =====================

pub fn get_bulk_order_remaining_size<M: Clone>(
    book: &OrderBook<M>,
    order_creator: [u8; 32],
    is_bid_side: bool,
) -> u64 {
    let OrderBook::UnifiedV1 {
        bulk_order_book, ..
    } = book;
    bulk_order_book::get_remaining_size(bulk_order_book, order_creator, is_bid_side)
}

pub fn place_bulk_order<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    order_req: BulkOrderRequest<M>,
    next_order_id_fn: impl FnOnce() -> OrderId,
    next_increasing_idx: IncreasingIdx,
    creation_time_micros: u64,
) -> BulkOrderPlaceResponse<M> {
    let OrderBook::UnifiedV1 {
        bulk_order_book,
        price_time_idx,
        ..
    } = book;
    bulk_order_book::place_bulk_order(
        bulk_order_book,
        price_time_idx,
        order_req,
        next_order_id_fn,
        next_increasing_idx,
        creation_time_micros,
    )
}

pub fn get_bulk_order<M: Clone>(book: &OrderBook<M>, order_creator: [u8; 32]) -> BulkOrder<M> {
    let OrderBook::UnifiedV1 {
        bulk_order_book, ..
    } = book;
    bulk_order_book::get_bulk_order(bulk_order_book, order_creator)
}

pub fn cancel_bulk_order<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    order_creator: [u8; 32],
) -> BulkOrder<M> {
    let OrderBook::UnifiedV1 {
        bulk_order_book,
        price_time_idx,
        ..
    } = book;
    bulk_order_book::cancel_bulk_order(bulk_order_book, price_time_idx, order_creator)
}

pub fn cancel_bulk_order_at_price<M: Clone + Copy>(
    book: &mut OrderBook<M>,
    order_creator: [u8; 32],
    price: u64,
    is_bid_side: bool,
) -> (u64, BulkOrder<M>) {
    let OrderBook::UnifiedV1 {
        bulk_order_book,
        price_time_idx,
        ..
    } = book;
    bulk_order_book::cancel_bulk_order_at_price(
        bulk_order_book,
        price_time_idx,
        order_creator,
        price,
        is_bid_side,
    )
}
