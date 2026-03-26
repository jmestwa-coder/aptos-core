// Copyright (c) Aptos Foundation
// Translated from: aptos_market::bulk_order_book

#[allow(dead_code)]
use crate::native_perpdex::bulk_order_types::{
    self, destroy_bulk_order, destroy_bulk_order_request, get_active_price, get_active_size,
    get_order_id, get_order_request, get_sequence_number, get_unique_priority_idx,
    new_bulk_order, new_bulk_order_match, new_bulk_order_place_response_rejection,
    new_bulk_order_place_response_success, new_bulk_order_request, get_account_from_request,
    BulkOrder, BulkOrderPlaceResponse, BulkOrderRequest,
};
use crate::native_perpdex::bulk_order_utils::{
    cancel_at_price_level, match_order_and_get_next_from_bulk_order,
    new_bulk_order_with_sanitization, reinsert_order_into_bulk_order,
};
use crate::native_perpdex::order_book_types::{bulk_order_type, IncreasingIdx, OrderId};
use crate::native_perpdex::order_book_utils::BigOrderedMap;
use crate::native_perpdex::order_match_types::{
    destroy_active_matched_order, get_account_from_match_details, get_order_id_from_match_details,
    validate_bulk_order_reinsertion_request, ActiveMatchedOrder, OrderMatch, OrderMatchDetails,
};
use crate::native_perpdex::price_time_index::{self, PriceTimeIndex};

// ===================== Constants =====================

const EORDER_ALREADY_EXISTS: u64 = 1;
const EORDER_NOT_FOUND: u64 = 2;
const E_REINSERT_ORDER_MISMATCH: u64 = 3;
const ENOT_BULK_ORDER: u64 = 4;

// ===================== BulkOrderBook =====================

#[derive(Clone, Debug)]
pub enum BulkOrderBook<M: Clone> {
    V1 {
        orders: BigOrderedMap<[u8; 32], BulkOrder<M>>,
        order_id_to_address: BigOrderedMap<OrderId, [u8; 32]>,
    },
}

pub fn new_bulk_order_book<M: Clone>() -> BulkOrderBook<M> {
    BulkOrderBook::V1 {
        orders: BigOrderedMap::new(),
        order_id_to_address: BigOrderedMap::new(),
    }
}

// ===================== Match =====================

pub fn get_single_match_for_taker<M: Clone + Copy>(
    book: &mut BulkOrderBook<M>,
    price_time_idx: &mut PriceTimeIndex,
    active_matched_order: ActiveMatchedOrder,
    is_bid_side: bool,
) -> OrderMatch<M> {
    let (order_id, matched_size, remaining_size, order_book_type) =
        destroy_active_matched_order(active_matched_order);
    assert!(order_book_type == bulk_order_type(), "ENOT_BULK_ORDER");

    let BulkOrderBook::V1 {
        orders,
        order_id_to_address,
    } = book;

    let order_address = order_id_to_address.get(&order_id).unwrap();
    let mut order = orders.remove(&order_address);
    let order_match = new_bulk_order_match(&order, !is_bid_side, matched_size);
    let (next_price, next_size) =
        match_order_and_get_next_from_bulk_order(&mut order, !is_bid_side, matched_size);

    if remaining_size == 0 && next_price.is_some() {
        let price = next_price.unwrap();
        let size = next_size.unwrap();
        price_time_index::place_maker_order(
            price_time_idx,
            order_id,
            bulk_order_type(),
            price,
            get_unique_priority_idx(&order),
            size,
            !is_bid_side,
        );
    }
    orders.add(order_address, order);
    order_match
}

// ===================== Cancel helpers =====================

fn cancel_active_order_for_side<M: Clone>(
    price_time_idx: &mut PriceTimeIndex,
    order: &BulkOrder<M>,
    is_bid_side: bool,
) {
    let active_price = get_active_price(get_order_request(order), is_bid_side);
    if let Some(price) = active_price {
        price_time_index::cancel_active_order(
            price_time_idx,
            price,
            get_unique_priority_idx(order),
            is_bid_side,
        );
    }
}

fn cancel_active_orders<M: Clone>(
    price_time_idx: &mut PriceTimeIndex,
    order: &BulkOrder<M>,
) {
    cancel_active_order_for_side(price_time_idx, order, true);
    cancel_active_order_for_side(price_time_idx, order, false);
}

fn activate_first_price_level_for_side<M: Clone>(
    price_time_idx: &mut PriceTimeIndex,
    order: &BulkOrder<M>,
    order_id: OrderId,
    is_bid_side: bool,
) {
    let req = get_order_request(order);
    let active_price = get_active_price(req, is_bid_side);
    let active_size = get_active_size(req, is_bid_side);
    if let Some(price) = active_price {
        price_time_index::place_maker_order(
            price_time_idx,
            order_id,
            bulk_order_type(),
            price,
            get_unique_priority_idx(order),
            active_size.unwrap(),
            is_bid_side,
        );
    }
}

fn activate_first_price_levels<M: Clone>(
    price_time_idx: &mut PriceTimeIndex,
    order: &BulkOrder<M>,
    order_id: OrderId,
) {
    activate_first_price_level_for_side(price_time_idx, order, order_id, true);
    activate_first_price_level_for_side(price_time_idx, order, order_id, false);
}

// ===================== Reinsert =====================

pub fn reinsert_order<M: Clone + Copy>(
    book: &mut BulkOrderBook<M>,
    price_time_idx: &mut PriceTimeIndex,
    reinsert_order_details: OrderMatchDetails<M>,
    original_order: &OrderMatchDetails<M>,
) {
    assert!(
        validate_bulk_order_reinsertion_request(&reinsert_order_details, original_order),
        "E_REINSERT_ORDER_MISMATCH"
    );
    let account = get_account_from_match_details(&reinsert_order_details);

    let BulkOrderBook::V1 { orders, .. } = book;
    let order_option = orders.remove_or_none(&account);
    assert!(order_option.is_some(), "EORDER_NOT_FOUND");
    let mut order = order_option.unwrap();
    cancel_active_orders(price_time_idx, &order);
    reinsert_order_into_bulk_order(&mut order, &reinsert_order_details);
    activate_first_price_levels(
        price_time_idx,
        &order,
        get_order_id_from_match_details(&reinsert_order_details),
    );
    orders.add(account, order);
}

// ===================== Cancel =====================

pub fn cancel_bulk_order<M: Clone + Copy>(
    book: &mut BulkOrderBook<M>,
    price_time_idx: &mut PriceTimeIndex,
    account: [u8; 32],
) -> BulkOrder<M> {
    let BulkOrderBook::V1 { orders, .. } = book;
    let order_opt = orders.remove_or_none(&account);
    assert!(order_opt.is_some(), "EORDER_NOT_FOUND");
    let order = order_opt.unwrap();
    let order_copy = order.clone();
    cancel_active_orders(price_time_idx, &order);

    let (order_request, order_id, unique_priority_idx, creation_time_micros) =
        destroy_bulk_order(order);
    let (account_addr, _old_seq_num, _bid_prices, _bid_sizes, _ask_prices, _ask_sizes, metadata) =
        destroy_bulk_order_request(order_request);

    let new_order_request = new_bulk_order_request(
        account_addr,
        0,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        metadata,
    );
    let new_order = new_bulk_order(
        new_order_request,
        order_id,
        unique_priority_idx,
        creation_time_micros,
    );

    orders.add(account, new_order);
    order_copy
}

pub fn cancel_bulk_order_at_price<M: Clone + Copy>(
    book: &mut BulkOrderBook<M>,
    price_time_idx: &mut PriceTimeIndex,
    account: [u8; 32],
    price: u64,
    is_bid_side: bool,
) -> (u64, BulkOrder<M>) {
    let BulkOrderBook::V1 { orders, .. } = book;
    let order_opt = orders.remove_or_none(&account);
    assert!(order_opt.is_some(), "EORDER_NOT_FOUND");
    let mut order = order_opt.unwrap();

    let active_price = get_active_price(get_order_request(&order), is_bid_side);
    let was_active = active_price.is_some() && active_price.unwrap() == price;

    if was_active {
        cancel_active_order_for_side(price_time_idx, &order, is_bid_side);
    }

    let cancelled_size = cancel_at_price_level(&mut order, price, is_bid_side);

    if was_active {
        let oid = get_order_id(&order);
        activate_first_price_level_for_side(price_time_idx, &order, oid, is_bid_side);
    }

    let order_copy = order.clone();
    orders.add(account, order);
    (cancelled_size, order_copy)
}

// ===================== Getters =====================

pub fn get_bulk_order<M: Clone>(
    book: &BulkOrderBook<M>,
    account: [u8; 32],
) -> BulkOrder<M> {
    let BulkOrderBook::V1 { orders, .. } = book;
    let result = orders.get(&account);
    assert!(result.is_some(), "EORDER_NOT_FOUND");
    result.unwrap()
}

pub fn get_remaining_size<M: Clone>(
    book: &BulkOrderBook<M>,
    account: [u8; 32],
    is_bid_side: bool,
) -> u64 {
    let BulkOrderBook::V1 { orders, .. } = book;
    let result_option = orders.get_and_map(&account, |order| {
        bulk_order_types::get_total_remaining_size(get_order_request(order), is_bid_side)
    });
    assert!(result_option.is_some(), "EORDER_NOT_FOUND");
    result_option.unwrap()
}

// ===================== Place =====================

pub fn place_bulk_order<M: Clone + Copy>(
    book: &mut BulkOrderBook<M>,
    price_time_idx: &mut PriceTimeIndex,
    order_req: BulkOrderRequest<M>,
    next_order_id_fn: impl FnOnce() -> OrderId,
    next_increasing_idx: IncreasingIdx,
    creation_time_micros: u64,
) -> BulkOrderPlaceResponse<M> {
    let account = get_account_from_request(&order_req);
    let new_sequence_number = get_sequence_number(&order_req);

    let BulkOrderBook::V1 {
        orders,
        order_id_to_address,
    } = book;

    let order_option = orders.remove_or_none(&account);
    let (order_id, previous_seq_num) = if let Some(old_order) = order_option {
        let existing_sequence_number = get_sequence_number(get_order_request(&old_order));
        if new_sequence_number <= existing_sequence_number {
            orders.add(account, old_order);
            return new_bulk_order_place_response_rejection(
                account,
                new_sequence_number,
                existing_sequence_number,
            );
        }
        cancel_active_orders(price_time_idx, &old_order);
        (get_order_id(&old_order), Some(existing_sequence_number))
    } else {
        let oid = next_order_id_fn();
        order_id_to_address.add(oid, account);
        (oid, None)
    };

    let (bulk_order, cancelled_bid_prices, cancelled_bid_sizes, cancelled_ask_prices, cancelled_ask_sizes) =
        new_bulk_order_with_sanitization(
            order_id,
            next_increasing_idx,
            order_req,
            price_time_index::best_bid_price(price_time_idx),
            price_time_index::best_ask_price(price_time_idx),
            creation_time_micros,
        );
    orders.add(account, bulk_order.clone());
    activate_first_price_levels(price_time_idx, &bulk_order, order_id);
    new_bulk_order_place_response_success(
        bulk_order,
        cancelled_bid_prices,
        cancelled_bid_sizes,
        cancelled_ask_prices,
        cancelled_ask_sizes,
        previous_seq_num,
    )
}
