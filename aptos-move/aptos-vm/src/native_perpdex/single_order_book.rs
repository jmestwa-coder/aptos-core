// Copyright (c) Aptos Foundation
// Translated from: aptos_market::single_order_book

#[allow(dead_code)]
use crate::native_perpdex::order_book_types::{
    new_account_client_order_id, single_order_type, AccountClientOrderId, IncreasingIdx, OrderId,
};
use crate::native_perpdex::order_book_utils::BigOrderedMap;
use crate::native_perpdex::order_match_types::{
    new_order_match,
    new_single_order_match_details, ActiveMatchedOrder, OrderMatch, OrderMatchDetails,
    validate_single_order_reinsertion_request,
    get_order_id_from_match_details, get_unique_priority_idx_from_match_details,
    get_remaining_size_from_match_details, get_price_from_match_details,
    is_bid_from_match_details,
};
use crate::native_perpdex::pending_order_book_index::{
    cancel_pending_order, new_pending_order_book_index, place_pending_order,
    take_ready_price_based_orders as pending_take_ready_price_based_orders,
    take_ready_time_based_orders as pending_take_ready_time_based_orders,
    PendingOrderBookIndex,
};
use crate::native_perpdex::price_time_index::{self, PriceTimeIndex};
use crate::native_perpdex::single_order_types::{
    self, destroy_order_from_state, destroy_single_order, destroy_single_order_request,
    get_account, get_client_order_id, get_order_from_state, get_order_request, get_price,
    get_remaining_size as get_req_remaining_size, get_remaining_size_from_state,
    get_trigger_condition, get_unique_priority_idx, get_unique_priority_idx_from_state,
    increase_remaining_size_from_state, is_active_order, is_bid, new_order_with_state,
    new_single_order, set_remaining_size_from_state, decrease_remaining_size_from_state,
    new_order_request_from_match_details, get_metadata_from_state, set_metadata_in_state,
    OrderWithState, SingleOrder, SingleOrderRequest,
};

// ===================== Constants =====================

const EORDER_ALREADY_EXISTS: u64 = 1;
const EORDER_NOT_FOUND: u64 = 2;
const EINVALID_INACTIVE_ORDER_STATE: u64 = 3;
const E_REINSERT_ORDER_MISMATCH: u64 = 4;
const EORDER_CREATOR_MISMATCH: u64 = 5;
const ENOT_SINGLE_ORDER_BOOK: u64 = 6;

// ===================== SingleOrderBook =====================

#[derive(Clone, Debug)]
pub enum SingleOrderBook<M: Clone> {
    V1 {
        orders: BigOrderedMap<OrderId, OrderWithState<M>>,
        client_order_ids: BigOrderedMap<AccountClientOrderId, OrderId>,
        pending_orders: PendingOrderBookIndex,
    },
}

pub fn new_single_order_book<M: Clone>() -> SingleOrderBook<M> {
    SingleOrderBook::V1 {
        orders: BigOrderedMap::new(),
        client_order_ids: BigOrderedMap::new(),
        pending_orders: new_pending_order_book_index(),
    }
}

// ===================== Cancel =====================

pub fn cancel_order<M: Clone + Copy>(
    book: &mut SingleOrderBook<M>,
    price_time_idx: &mut PriceTimeIndex,
    order_creator: [u8; 32],
    order_id: OrderId,
) -> SingleOrder<M> {
    let SingleOrderBook::V1 {
        orders,
        client_order_ids,
        pending_orders,
    } = book;
    let order_with_state_option = orders.remove_or_none(&order_id);
    assert!(order_with_state_option.is_some(), "EORDER_NOT_FOUND");
    let order_with_state = order_with_state_option.unwrap();
    let (order, is_active) = destroy_order_from_state(order_with_state);
    let order_request = get_order_request(&order);
    assert!(order_creator == get_account(order_request), "EORDER_CREATOR_MISMATCH");

    if is_active {
        price_time_index::cancel_active_order(
            price_time_idx,
            get_price(order_request),
            get_unique_priority_idx(&order),
            is_bid(order_request),
        );
        if let Some(client_id) = get_client_order_id(order_request) {
            client_order_ids.remove(&new_account_client_order_id(
                get_account(order_request),
                client_id,
            ));
        }
    } else {
        let trigger = get_trigger_condition(order_request);
        cancel_pending_order(
            pending_orders,
            trigger.unwrap(),
            get_unique_priority_idx(&order),
        );
        if let Some(client_id) = get_client_order_id(order_request) {
            client_order_ids.remove(&new_account_client_order_id(
                get_account(order_request),
                client_id,
            ));
        }
    }
    order
}

pub fn try_cancel_order_with_client_order_id<M: Clone + Copy>(
    book: &mut SingleOrderBook<M>,
    price_time_idx: &mut PriceTimeIndex,
    order_creator: [u8; 32],
    client_order_id: Vec<u8>,
) -> Option<SingleOrder<M>> {
    let SingleOrderBook::V1 {
        client_order_ids, ..
    } = book;
    let account_client_order_id =
        new_account_client_order_id(order_creator, client_order_id);
    let order_id = client_order_ids.get(&account_client_order_id);
    match order_id {
        None => None,
        Some(oid) => Some(cancel_order(book, price_time_idx, order_creator, oid)),
    }
}

pub fn try_cancel_order<M: Clone + Copy>(
    book: &mut SingleOrderBook<M>,
    price_time_idx: &mut PriceTimeIndex,
    order_creator: [u8; 32],
    order_id: OrderId,
) -> Option<SingleOrder<M>> {
    let SingleOrderBook::V1 { orders, .. } = book;
    let is_creator = orders.get_and_map(&order_id, |ows| {
        get_account(get_order_request(get_order_from_state(ows))) == order_creator
    });
    match is_creator {
        None => None,
        Some(false) => None,
        Some(true) => Some(cancel_order(book, price_time_idx, order_creator, order_id)),
    }
}

pub fn client_order_id_exists<M: Clone>(
    book: &SingleOrderBook<M>,
    order_creator: [u8; 32],
    client_order_id: Vec<u8>,
) -> bool {
    let SingleOrderBook::V1 {
        client_order_ids, ..
    } = book;
    let account_client_order_id =
        new_account_client_order_id(order_creator, client_order_id);
    client_order_ids.contains(&account_client_order_id)
}

// ===================== Place =====================

pub fn place_maker_or_pending_order<M: Clone + Copy>(
    book: &mut SingleOrderBook<M>,
    price_time_idx: &mut PriceTimeIndex,
    order_req: SingleOrderRequest<M>,
    ascending_idx: IncreasingIdx,
) {
    if get_trigger_condition(&order_req).is_some() {
        place_pending_order_internal(book, order_req, ascending_idx);
        return;
    }
    place_ready_maker_order_with_unique_idx(book, price_time_idx, order_req, ascending_idx);
}

fn place_ready_maker_order_with_unique_idx<M: Clone + Copy>(
    book: &mut SingleOrderBook<M>,
    price_time_idx: &mut PriceTimeIndex,
    order_req: SingleOrderRequest<M>,
    ascending_idx: IncreasingIdx,
) {
    let SingleOrderBook::V1 {
        orders,
        client_order_ids,
        ..
    } = book;
    let order = new_single_order(order_req.clone(), ascending_idx);
    let order_id = single_order_types::get_order_id(&order_req);
    let prev = orders.upsert(order_id, new_order_with_state(order.clone(), true));
    assert!(prev.is_none(), "EORDER_ALREADY_EXISTS");

    if let Some(client_id) = get_client_order_id(&order_req) {
        client_order_ids.add(
            new_account_client_order_id(get_account(&order_req), client_id),
            order_id,
        );
    }
    price_time_index::place_maker_order(
        price_time_idx,
        order_id,
        single_order_type(),
        get_price(&order_req),
        ascending_idx,
        get_req_remaining_size(&order_req),
        is_bid(&order_req),
    );
}

fn place_pending_order_internal<M: Clone + Copy>(
    book: &mut SingleOrderBook<M>,
    order_req: SingleOrderRequest<M>,
    ascending_idx: IncreasingIdx,
) {
    let SingleOrderBook::V1 {
        orders,
        client_order_ids,
        pending_orders,
    } = book;
    let order_id = single_order_types::get_order_id(&order_req);
    let order = new_single_order(order_req.clone(), ascending_idx);
    orders.add(order_id, new_order_with_state(order, false));

    if let Some(client_id) = get_client_order_id(&order_req) {
        client_order_ids.add(
            new_account_client_order_id(get_account(&order_req), client_id),
            order_id,
        );
    }

    place_pending_order(
        pending_orders,
        order_id,
        get_trigger_condition(&order_req).unwrap(),
        ascending_idx,
    );
}

// ===================== Match =====================

pub fn get_single_match_for_taker<M: Clone + Copy>(
    book: &mut SingleOrderBook<M>,
    active_matched_order: ActiveMatchedOrder,
) -> OrderMatch<M> {
    let (order_id, matched_size, remaining_size, order_book_type) =
        crate::native_perpdex::order_match_types::destroy_active_matched_order(active_matched_order);
    assert!(order_book_type == single_order_type(), "ENOT_SINGLE_ORDER_BOOK");

    let SingleOrderBook::V1 {
        orders,
        client_order_ids,
        ..
    } = book;

    let order_with_state = if remaining_size == 0 {
        let mut ows = orders.remove(&order_id);
        set_remaining_size_from_state(&mut ows, 0);
        ows
    } else {
        orders.modify_and_return(&order_id, |ows| {
            set_remaining_size_from_state(ows, remaining_size);
            ows.clone()
        })
    };

    let (order, is_active_flag) = destroy_order_from_state(order_with_state);
    assert!(is_active_flag, "EINVALID_INACTIVE_ORDER_STATE");

    let (order_request, unique_priority_idx) = destroy_single_order(order);
    let (
        account,
        oid,
        client_order_id,
        price,
        orig_size,
        size,
        is_bid_val,
        _trigger_condition,
        time_in_force,
        creation_time_micros,
        metadata,
    ) = destroy_single_order_request(order_request);

    if remaining_size == 0 {
        if let Some(ref cid) = client_order_id {
            client_order_ids.remove(&new_account_client_order_id(account, cid.clone()));
        }
    }

    new_order_match(
        new_single_order_match_details(
            oid,
            account,
            client_order_id,
            unique_priority_idx,
            price,
            orig_size,
            size,
            is_bid_val,
            time_in_force,
            creation_time_micros,
            metadata,
        ),
        matched_size,
    )
}

// ===================== Decrease Size =====================

pub fn decrease_order_size<M: Clone + Copy>(
    book: &mut SingleOrderBook<M>,
    price_time_idx: &mut PriceTimeIndex,
    order_creator: [u8; 32],
    order_id: OrderId,
    size_delta: u64,
) {
    let SingleOrderBook::V1 { orders, .. } = book;

    let order_opt = orders.modify_if_present_and_return(&order_id, |ows| {
        assert!(
            get_account(get_order_request(get_order_from_state(ows))) == order_creator,
            "EORDER_CREATOR_MISMATCH"
        );
        decrease_remaining_size_from_state(ows, size_delta).expect("Invalid order size decrease");
        ows.clone()
    });

    assert!(order_opt.is_some(), "EORDER_NOT_FOUND");
    let order_with_state = order_opt.unwrap();

    if is_active_order(&order_with_state) {
        let order = get_order_from_state(&order_with_state);
        let order_request = get_order_request(order);
        price_time_index::decrease_order_size(
            price_time_idx,
            get_price(order_request),
            get_unique_priority_idx_from_state(&order_with_state),
            size_delta,
            is_bid(order_request),
        );
    }
}

// ===================== Reinsert =====================

pub fn reinsert_order<M: Clone + Copy>(
    book: &mut SingleOrderBook<M>,
    price_time_idx: &mut PriceTimeIndex,
    reinsert_order_details: OrderMatchDetails<M>,
    original_order: &OrderMatchDetails<M>,
) {
    assert!(
        validate_single_order_reinsertion_request(&reinsert_order_details, original_order),
        "E_REINSERT_ORDER_MISMATCH"
    );
    let order_id = get_order_id_from_match_details(&reinsert_order_details);
    let unique_idx = get_unique_priority_idx_from_match_details(&reinsert_order_details);
    let reinsert_remaining_size = get_remaining_size_from_match_details(&reinsert_order_details);

    let SingleOrderBook::V1 { orders, .. } = book;
    let present = orders.modify_if_present(&order_id, |ows| {
        increase_remaining_size_from_state(ows, reinsert_remaining_size);
    });

    if !present {
        let ascending_idx = unique_idx;
        let order_req = new_order_request_from_match_details(reinsert_order_details);
        place_ready_maker_order_with_unique_idx(book, price_time_idx, order_req, ascending_idx);
        return;
    }

    price_time_index::increase_order_size(
        price_time_idx,
        get_price_from_match_details(&reinsert_order_details),
        unique_idx,
        reinsert_remaining_size,
        is_bid_from_match_details(&reinsert_order_details),
    );
}

// ===================== Getters =====================

pub fn get_order_id_by_client_id<M: Clone>(
    book: &SingleOrderBook<M>,
    order_creator: [u8; 32],
    client_order_id: Vec<u8>,
) -> Option<OrderId> {
    let SingleOrderBook::V1 {
        client_order_ids, ..
    } = book;
    let key = new_account_client_order_id(order_creator, client_order_id);
    client_order_ids.get(&key)
}

pub fn get_order_metadata<M: Clone + Copy>(
    book: &SingleOrderBook<M>,
    order_id: OrderId,
) -> Option<M> {
    let SingleOrderBook::V1 { orders, .. } = book;
    orders.get_and_map(&order_id, |ows| get_metadata_from_state(ows))
}

pub fn set_order_metadata<M: Clone + Copy>(
    book: &mut SingleOrderBook<M>,
    order_id: OrderId,
    metadata: M,
) {
    let SingleOrderBook::V1 { orders, .. } = book;
    let present = orders.modify_if_present(&order_id, |ows| {
        set_metadata_in_state(ows, metadata);
    });
    assert!(present, "EORDER_NOT_FOUND");
}

pub fn is_active_order_in_book<M: Clone>(
    book: &SingleOrderBook<M>,
    order_id: OrderId,
) -> bool {
    let SingleOrderBook::V1 { orders, .. } = book;
    orders
        .get_and_map(&order_id, |ows| is_active_order(ows))
        .unwrap_or(false)
}

pub fn get_order<M: Clone>(
    book: &SingleOrderBook<M>,
    order_id: OrderId,
) -> Option<OrderWithState<M>> {
    let SingleOrderBook::V1 { orders, .. } = book;
    orders.get(&order_id)
}

pub fn get_remaining_size<M: Clone>(
    book: &SingleOrderBook<M>,
    order_id: OrderId,
) -> u64 {
    let SingleOrderBook::V1 { orders, .. } = book;
    orders
        .get_and_map(&order_id, |ows| get_remaining_size_from_state(ows))
        .unwrap_or(0)
}

// ===================== Take Ready Orders =====================

pub fn take_ready_price_based_orders<M: Clone + Copy>(
    book: &mut SingleOrderBook<M>,
    current_price: u64,
    order_limit: u64,
) -> Vec<SingleOrder<M>> {
    let SingleOrderBook::V1 {
        orders,
        client_order_ids,
        pending_orders,
    } = book;

    let order_ids = pending_take_ready_price_based_orders(pending_orders, current_price, order_limit);
    let mut result = Vec::new();
    for oid in order_ids {
        let ows = orders.remove(&oid);
        let (order, _) = destroy_order_from_state(ows);
        let order_request = get_order_request(&order);
        if let Some(cid) = get_client_order_id(order_request) {
            client_order_ids.remove(&new_account_client_order_id(get_account(order_request), cid));
        }
        result.push(order);
    }
    result
}

pub fn take_ready_time_based_orders<M: Clone + Copy>(
    book: &mut SingleOrderBook<M>,
    order_limit: u64,
    current_time_secs: u64,
) -> Vec<SingleOrder<M>> {
    let SingleOrderBook::V1 {
        orders,
        client_order_ids,
        pending_orders,
    } = book;

    let order_ids = pending_take_ready_time_based_orders(pending_orders, order_limit, current_time_secs);
    let mut result = Vec::new();
    for oid in order_ids {
        let ows = orders.remove(&oid);
        let (order, _) = destroy_order_from_state(ows);
        let order_request = get_order_request(&order);
        if let Some(cid) = get_client_order_id(order_request) {
            client_order_ids.remove(&new_account_client_order_id(get_account(order_request), cid));
        }
        result.push(order);
    }
    result
}
