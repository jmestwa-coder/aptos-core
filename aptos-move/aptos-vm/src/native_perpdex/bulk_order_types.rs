// Copyright (c) Aptos Foundation
// Translated from: aptos_trading::bulk_order_types

use crate::native_perpdex::order_book_types::{IncreasingIdx, OrderId};
use crate::native_perpdex::order_match_types::{
    new_bulk_order_match_details, new_order_match, OrderMatch,
};

// ===================== Types =====================

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BulkOrderRequest<M: Clone> {
    V1 {
        account: [u8; 32],
        order_sequence_number: u64,
        bid_prices: Vec<u64>,
        bid_sizes: Vec<u64>,
        ask_prices: Vec<u64>,
        ask_sizes: Vec<u64>,
        metadata: M,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BulkOrder<M: Clone> {
    V1 {
        order_request: BulkOrderRequest<M>,
        order_id: OrderId,
        unique_priority_idx: IncreasingIdx,
        creation_time_micros: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BulkOrderPlaceResponse<M: Clone> {
    SuccessV1 {
        order: BulkOrder<M>,
        cancelled_bid_prices: Vec<u64>,
        cancelled_bid_sizes: Vec<u64>,
        cancelled_ask_prices: Vec<u64>,
        cancelled_ask_sizes: Vec<u64>,
        previous_seq_num: Option<u64>,
    },
    RejectionV1 {
        account: [u8; 32],
        sequence_number: u64,
        existing_sequence_number: u64,
    },
}

// ===================== Constructors =====================

pub fn new_bulk_order<M: Clone>(
    order_request: BulkOrderRequest<M>,
    order_id: OrderId,
    unique_priority_idx: IncreasingIdx,
    creation_time_micros: u64,
) -> BulkOrder<M> {
    BulkOrder::V1 {
        order_request,
        order_id,
        unique_priority_idx,
        creation_time_micros,
    }
}

pub fn new_bulk_order_request<M: Clone>(
    account: [u8; 32],
    sequence_number: u64,
    bid_prices: Vec<u64>,
    bid_sizes: Vec<u64>,
    ask_prices: Vec<u64>,
    ask_sizes: Vec<u64>,
    metadata: M,
) -> BulkOrderRequest<M> {
    BulkOrderRequest::V1 {
        account,
        order_sequence_number: sequence_number,
        bid_prices,
        bid_sizes,
        ask_prices,
        ask_sizes,
        metadata,
    }
}

pub fn new_bulk_order_place_response_success<M: Clone>(
    order: BulkOrder<M>,
    cancelled_bid_prices: Vec<u64>,
    cancelled_bid_sizes: Vec<u64>,
    cancelled_ask_prices: Vec<u64>,
    cancelled_ask_sizes: Vec<u64>,
    previous_seq_num: Option<u64>,
) -> BulkOrderPlaceResponse<M> {
    BulkOrderPlaceResponse::SuccessV1 {
        order,
        cancelled_bid_prices,
        cancelled_bid_sizes,
        cancelled_ask_prices,
        cancelled_ask_sizes,
        previous_seq_num,
    }
}

pub fn new_bulk_order_place_response_rejection<M: Clone>(
    account: [u8; 32],
    sequence_number: u64,
    existing_sequence_number: u64,
) -> BulkOrderPlaceResponse<M> {
    BulkOrderPlaceResponse::RejectionV1 {
        account,
        sequence_number,
        existing_sequence_number,
    }
}

// ===================== Getters for BulkOrder =====================

pub fn get_unique_priority_idx<M: Clone>(order: &BulkOrder<M>) -> IncreasingIdx {
    let BulkOrder::V1 {
        unique_priority_idx,
        ..
    } = order;
    *unique_priority_idx
}

pub fn get_order_id<M: Clone>(order: &BulkOrder<M>) -> OrderId {
    let BulkOrder::V1 { order_id, .. } = order;
    *order_id
}

pub fn get_creation_time_micros<M: Clone>(order: &BulkOrder<M>) -> u64 {
    let BulkOrder::V1 {
        creation_time_micros,
        ..
    } = order;
    *creation_time_micros
}

pub fn get_order_request<M: Clone>(order: &BulkOrder<M>) -> &BulkOrderRequest<M> {
    let BulkOrder::V1 { order_request, .. } = order;
    order_request
}

pub fn get_order_request_mut<M: Clone>(order: &mut BulkOrder<M>) -> &mut BulkOrderRequest<M> {
    let BulkOrder::V1 { order_request, .. } = order;
    order_request
}

// ===================== Getters for BulkOrderRequest =====================

pub fn get_account_from_request<M: Clone>(req: &BulkOrderRequest<M>) -> [u8; 32] {
    let BulkOrderRequest::V1 { account, .. } = req;
    *account
}

pub fn get_sequence_number<M: Clone>(req: &BulkOrderRequest<M>) -> u64 {
    let BulkOrderRequest::V1 {
        order_sequence_number,
        ..
    } = req;
    *order_sequence_number
}

pub fn get_total_remaining_size<M: Clone>(req: &BulkOrderRequest<M>, is_bid_side: bool) -> u64 {
    let BulkOrderRequest::V1 {
        bid_sizes,
        ask_sizes,
        ..
    } = req;
    if is_bid_side {
        bid_sizes.iter().sum()
    } else {
        ask_sizes.iter().sum()
    }
}

pub fn get_active_price<M: Clone>(req: &BulkOrderRequest<M>, is_bid_side: bool) -> Option<u64> {
    let BulkOrderRequest::V1 {
        bid_prices,
        ask_prices,
        ..
    } = req;
    let prices = if is_bid_side { bid_prices } else { ask_prices };
    if prices.is_empty() {
        None
    } else {
        Some(prices[0])
    }
}

pub fn get_all_prices<M: Clone>(req: &BulkOrderRequest<M>, is_bid_side: bool) -> Vec<u64> {
    let BulkOrderRequest::V1 {
        bid_prices,
        ask_prices,
        ..
    } = req;
    if is_bid_side {
        bid_prices.clone()
    } else {
        ask_prices.clone()
    }
}

pub fn get_all_prices_mut<M: Clone>(
    req: &mut BulkOrderRequest<M>,
    is_bid_side: bool,
) -> &mut Vec<u64> {
    match req {
        BulkOrderRequest::V1 {
            bid_prices,
            ask_prices,
            ..
        } => {
            if is_bid_side {
                bid_prices
            } else {
                ask_prices
            }
        },
    }
}

pub fn get_all_sizes<M: Clone>(req: &BulkOrderRequest<M>, is_bid_side: bool) -> Vec<u64> {
    let BulkOrderRequest::V1 {
        bid_sizes,
        ask_sizes,
        ..
    } = req;
    if is_bid_side {
        bid_sizes.clone()
    } else {
        ask_sizes.clone()
    }
}

pub fn get_all_sizes_mut<M: Clone>(
    req: &mut BulkOrderRequest<M>,
    is_bid_side: bool,
) -> &mut Vec<u64> {
    match req {
        BulkOrderRequest::V1 {
            bid_sizes,
            ask_sizes,
            ..
        } => {
            if is_bid_side {
                bid_sizes
            } else {
                ask_sizes
            }
        },
    }
}

pub fn get_active_size<M: Clone>(req: &BulkOrderRequest<M>, is_bid_side: bool) -> Option<u64> {
    let BulkOrderRequest::V1 {
        bid_sizes,
        ask_sizes,
        ..
    } = req;
    let sizes = if is_bid_side { bid_sizes } else { ask_sizes };
    if sizes.is_empty() {
        None
    } else {
        Some(sizes[0])
    }
}

pub fn get_prices_and_sizes_mut<M: Clone>(
    req: &mut BulkOrderRequest<M>,
    is_bid_side: bool,
) -> (&mut Vec<u64>, &mut Vec<u64>) {
    match req {
        BulkOrderRequest::V1 {
            bid_prices,
            bid_sizes,
            ask_prices,
            ask_sizes,
            ..
        } => {
            if is_bid_side {
                (bid_prices, bid_sizes)
            } else {
                (ask_prices, ask_sizes)
            }
        },
    }
}

// ===================== BulkOrderPlaceResponse =====================

pub fn is_success_response<M: Clone>(resp: &BulkOrderPlaceResponse<M>) -> bool {
    matches!(resp, BulkOrderPlaceResponse::SuccessV1 { .. })
}

pub fn is_rejection_response<M: Clone>(resp: &BulkOrderPlaceResponse<M>) -> bool {
    matches!(resp, BulkOrderPlaceResponse::RejectionV1 { .. })
}

pub fn destroy_bulk_order_place_response_success<M: Clone>(
    resp: BulkOrderPlaceResponse<M>,
) -> (BulkOrder<M>, Vec<u64>, Vec<u64>, Vec<u64>, Vec<u64>, Option<u64>) {
    match resp {
        BulkOrderPlaceResponse::SuccessV1 {
            order,
            cancelled_bid_prices,
            cancelled_bid_sizes,
            cancelled_ask_prices,
            cancelled_ask_sizes,
            previous_seq_num,
        } => (
            order,
            cancelled_bid_prices,
            cancelled_bid_sizes,
            cancelled_ask_prices,
            cancelled_ask_sizes,
            previous_seq_num,
        ),
        _ => panic!("Expected SuccessV1"),
    }
}

pub fn destroy_bulk_order_place_response_rejection<M: Clone>(
    resp: BulkOrderPlaceResponse<M>,
) -> ([u8; 32], u64, u64) {
    match resp {
        BulkOrderPlaceResponse::RejectionV1 {
            account,
            sequence_number,
            existing_sequence_number,
        } => (account, sequence_number, existing_sequence_number),
        _ => panic!("Expected RejectionV1"),
    }
}

// ===================== Bulk Order Match =====================

pub fn new_bulk_order_match<M: Clone + Copy>(
    order: &BulkOrder<M>,
    is_bid_side: bool,
    matched_size: u64,
) -> OrderMatch<M> {
    let BulkOrder::V1 {
        order_request,
        order_id,
        unique_priority_idx,
        creation_time_micros,
    } = order;
    let BulkOrderRequest::V1 {
        account,
        order_sequence_number,
        bid_prices,
        bid_sizes,
        ask_prices,
        ask_sizes,
        metadata,
    } = order_request;

    let (price, remaining_size) = if is_bid_side {
        (bid_prices[0], bid_sizes[0] - matched_size)
    } else {
        (ask_prices[0], ask_sizes[0] - matched_size)
    };

    new_order_match(
        new_bulk_order_match_details(
            *order_id,
            *account,
            *unique_priority_idx,
            price,
            remaining_size,
            is_bid_side,
            *order_sequence_number,
            *creation_time_micros,
            *metadata,
        ),
        matched_size,
    )
}

// ===================== Set Empty =====================

pub fn set_empty<M: Clone>(order: &mut BulkOrder<M>) {
    match order {
        BulkOrder::V1 { order_request, .. } => match order_request {
            BulkOrderRequest::V1 {
                bid_prices,
                bid_sizes,
                ask_prices,
                ask_sizes,
                ..
            } => {
                bid_prices.clear();
                bid_sizes.clear();
                ask_prices.clear();
                ask_sizes.clear();
            },
        },
    }
}

// ===================== Destructure =====================

pub fn destroy_bulk_order<M: Clone>(
    order: BulkOrder<M>,
) -> (BulkOrderRequest<M>, OrderId, IncreasingIdx, u64) {
    let BulkOrder::V1 {
        order_request,
        order_id,
        unique_priority_idx,
        creation_time_micros,
    } = order;
    (
        order_request,
        order_id,
        unique_priority_idx,
        creation_time_micros,
    )
}

pub fn destroy_bulk_order_request<M: Clone>(
    req: BulkOrderRequest<M>,
) -> ([u8; 32], u64, Vec<u64>, Vec<u64>, Vec<u64>, Vec<u64>, M) {
    let BulkOrderRequest::V1 {
        account,
        order_sequence_number,
        bid_prices,
        bid_sizes,
        ask_prices,
        ask_sizes,
        metadata,
    } = req;
    (
        account,
        order_sequence_number,
        bid_prices,
        bid_sizes,
        ask_prices,
        ask_sizes,
        metadata,
    )
}
