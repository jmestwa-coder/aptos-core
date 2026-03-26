// Copyright (c) Aptos Foundation
// Translated from: aptos_market::bulk_order_utils

#[allow(dead_code)]
use crate::native_perpdex::bulk_order_types::{
    get_all_prices, get_all_prices_mut, get_all_sizes_mut,
    get_order_request_mut, get_prices_and_sizes_mut, new_bulk_order, new_bulk_order_request,
    BulkOrder, BulkOrderRequest,
};
use crate::native_perpdex::order_book_types::{IncreasingIdx, OrderId};
use crate::native_perpdex::order_match_types::{
    get_price_from_match_details, get_remaining_size_from_match_details,
    is_bid_from_match_details, OrderMatchDetails,
};

// ===================== Constants =====================

const EPRICE_CROSSING: u64 = 1;
const E_BID_LENGTH_MISMATCH: u64 = 2;
const E_ASK_LENGTH_MISMATCH: u64 = 3;
const E_EMPTY_ORDER: u64 = 4;
const E_BID_SIZE_ZERO: u64 = 5;
const E_ASK_SIZE_ZERO: u64 = 6;
const E_BID_ORDER_INVALID: u64 = 7;
const E_ASK_ORDER_INVALID: u64 = 8;
const E_BULK_ORDER_DEPTH_EXCEEDED: u64 = 9;
const E_INVALID_SEQUENCE_NUMBER: u64 = 10;
const EUNEXPECTED_MATCH_SIZE: u64 = 11;

const MAX_BULK_ORDER_DEPTH_PER_SIDE: u64 = 40;

// ===================== Functions =====================

pub fn new_bulk_order_request_with_sanitization<M: Clone>(
    account: [u8; 32],
    sequence_number: u64,
    bid_prices: Vec<u64>,
    bid_sizes: Vec<u64>,
    ask_prices: Vec<u64>,
    ask_sizes: Vec<u64>,
    metadata: M,
) -> Result<BulkOrderRequest<M>, u64> {
    if sequence_number == 0 {
        return Err(E_INVALID_SEQUENCE_NUMBER);
    }
    let num_bids = bid_prices.len() as u64;
    let num_asks = ask_prices.len() as u64;

    if num_bids != bid_sizes.len() as u64 {
        return Err(E_BID_LENGTH_MISMATCH);
    }
    if num_asks != ask_sizes.len() as u64 {
        return Err(E_ASK_LENGTH_MISMATCH);
    }
    if num_bids == 0 && num_asks == 0 {
        return Err(E_EMPTY_ORDER);
    }
    if num_bids > MAX_BULK_ORDER_DEPTH_PER_SIDE {
        return Err(E_BULK_ORDER_DEPTH_EXCEEDED);
    }
    if num_asks > MAX_BULK_ORDER_DEPTH_PER_SIDE {
        return Err(E_BULK_ORDER_DEPTH_EXCEEDED);
    }
    if !validate_not_zero_sizes(&bid_sizes) {
        return Err(E_BID_SIZE_ZERO);
    }
    if !validate_not_zero_sizes(&ask_sizes) {
        return Err(E_ASK_SIZE_ZERO);
    }
    if !validate_price_ordering(&bid_prices, true) {
        return Err(E_BID_ORDER_INVALID);
    }
    if !validate_price_ordering(&ask_prices, false) {
        return Err(E_ASK_ORDER_INVALID);
    }
    if num_bids > 0 && num_asks > 0 {
        if bid_prices[0] >= ask_prices[0] {
            return Err(EPRICE_CROSSING);
        }
    }
    Ok(new_bulk_order_request(
        account,
        sequence_number,
        bid_prices,
        bid_sizes,
        ask_prices,
        ask_sizes,
        metadata,
    ))
}

pub fn new_bulk_order_with_sanitization<M: Clone>(
    order_id: OrderId,
    unique_priority_idx: IncreasingIdx,
    mut order_req: BulkOrderRequest<M>,
    best_bid_price: Option<u64>,
    best_ask_price: Option<u64>,
    creation_time_micros: u64,
) -> (BulkOrder<M>, Vec<u64>, Vec<u64>, Vec<u64>, Vec<u64>) {
    let bid_price_crossing_idx = discard_price_crossing_levels(
        &get_all_prices(&order_req, true),
        best_ask_price,
        true,
    );
    let ask_price_crossing_idx = discard_price_crossing_levels(
        &get_all_prices(&order_req, false),
        best_bid_price,
        false,
    );

    let (cancelled_bid_prices, cancelled_bid_sizes) = if bid_price_crossing_idx > 0 {
        let cancelled_bid_prices =
            trim_start(get_all_prices_mut(&mut order_req, true), bid_price_crossing_idx);
        let cancelled_bid_sizes =
            trim_start(get_all_sizes_mut(&mut order_req, true), bid_price_crossing_idx);
        (cancelled_bid_prices, cancelled_bid_sizes)
    } else {
        (Vec::new(), Vec::new())
    };

    let (cancelled_ask_prices, cancelled_ask_sizes) = if ask_price_crossing_idx > 0 {
        let cancelled_ask_prices =
            trim_start(get_all_prices_mut(&mut order_req, false), ask_price_crossing_idx);
        let cancelled_ask_sizes =
            trim_start(get_all_sizes_mut(&mut order_req, false), ask_price_crossing_idx);
        (cancelled_ask_prices, cancelled_ask_sizes)
    } else {
        (Vec::new(), Vec::new())
    };

    let bulk_order = new_bulk_order(
        order_req,
        order_id,
        unique_priority_idx,
        creation_time_micros,
    );
    (
        bulk_order,
        cancelled_bid_prices,
        cancelled_bid_sizes,
        cancelled_ask_prices,
        cancelled_ask_sizes,
    )
}

fn validate_not_zero_sizes(sizes: &[u64]) -> bool {
    for &s in sizes {
        if s == 0 {
            return false;
        }
    }
    true
}

fn validate_price_ordering(prices: &[u64], is_descending: bool) -> bool {
    if prices.is_empty() {
        return true;
    }
    for i in 0..prices.len() - 1 {
        if is_descending {
            if prices[i] <= prices[i + 1] {
                return false;
            }
        } else {
            if prices[i] >= prices[i + 1] {
                return false;
            }
        }
    }
    true
}

fn trim_start(v: &mut Vec<u64>, new_start: usize) -> Vec<u64> {
    let trimmed: Vec<u64> = v.drain(..new_start).collect();
    trimmed
}

fn discard_price_crossing_levels(
    prices: &[u64],
    best_price: Option<u64>,
    is_bid_side: bool,
) -> usize {
    let mut i = 0;
    if let Some(best_price) = best_price {
        while i < prices.len() {
            if is_bid_side && prices[i] < best_price {
                break;
            } else if !is_bid_side && prices[i] > best_price {
                break;
            }
            i += 1;
        }
    }
    i
}

pub fn reinsert_order_into_bulk_order<M: Clone>(
    order: &mut BulkOrder<M>,
    other: &OrderMatchDetails<M>,
) where
    M: Copy,
{
    let is_bid_side = is_bid_from_match_details(other);
    let (prices, sizes) = get_prices_and_sizes_mut(get_order_request_mut(order), is_bid_side);
    let other_price = get_price_from_match_details(other);
    if !prices.is_empty() && prices[0] == other_price {
        sizes[0] += get_remaining_size_from_match_details(other);
    } else {
        prices.insert(0, other_price);
        sizes.insert(0, get_remaining_size_from_match_details(other));
    }
}

pub fn match_order_and_get_next_from_bulk_order<M: Clone>(
    order: &mut BulkOrder<M>,
    is_bid_side: bool,
    matched_size: u64,
) -> (Option<u64>, Option<u64>) {
    let (prices, sizes) = get_prices_and_sizes_mut(get_order_request_mut(order), is_bid_side);
    assert!(matched_size <= sizes[0], "EUNEXPECTED_MATCH_SIZE");
    sizes[0] -= matched_size;
    if sizes[0] == 0 {
        prices.remove(0);
        sizes.remove(0);
    }
    if sizes.is_empty() {
        (None, None)
    } else {
        (Some(prices[0]), Some(sizes[0]))
    }
}

pub fn cancel_at_price_level<M: Clone>(
    order: &mut BulkOrder<M>,
    price: u64,
    is_bid_side: bool,
) -> u64 {
    let (prices, sizes) = get_prices_and_sizes_mut(get_order_request_mut(order), is_bid_side);
    for i in 0..prices.len() {
        if prices[i] == price {
            let cancelled_size = sizes[i];
            prices.remove(i);
            sizes.remove(i);
            return cancelled_size;
        }
    }
    0
}
