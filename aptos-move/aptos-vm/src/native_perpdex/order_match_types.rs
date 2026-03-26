// Copyright (c) Aptos Foundation
// Translated from: aptos_trading::order_match_types

use crate::native_perpdex::order_book_types::{
    bulk_order_type, good_till_cancelled, is_single_order_type, single_order_type, IncreasingIdx,
    OrderId, OrderType, TimeInForce,
};

// ===================== Constants =====================

const E_REINSERT_ORDER_MISMATCH: u64 = 8;

// ===================== Types =====================

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrderMatchDetails<M: Clone> {
    SingleOrder {
        order_id: OrderId,
        account: [u8; 32],
        client_order_id: Option<Vec<u8>>,
        unique_priority_idx: IncreasingIdx,
        price: u64,
        orig_size: u64,
        remaining_size: u64,
        is_bid: bool,
        time_in_force: TimeInForce,
        creation_time_micros: u64,
        metadata: M,
    },
    BulkOrder {
        order_id: OrderId,
        account: [u8; 32],
        unique_priority_idx: IncreasingIdx,
        price: u64,
        remaining_size: u64,
        is_bid: bool,
        sequence_number: u64,
        creation_time_micros: u64,
        metadata: M,
    },
}

// NOTE: Clone for Option<Vec<u8>> requires special handling since Copy isn't available.
// We implement it manually to support the Copy-like semantics needed.
// Actually, Vec<u8> doesn't implement Copy so OrderMatchDetails can't derive Copy when it contains Option<Vec<u8>>.
// We'll remove Copy and use Clone instead.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrderMatch<M: Clone> {
    V1 {
        order: OrderMatchDetails<M>,
        matched_size: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum ActiveMatchedOrder {
    V1 {
        order_id: OrderId,
        matched_size: u64,
        remaining_size: u64,
        order_book_type: OrderType,
    },
}

// ===================== Constructors =====================

pub fn new_single_order_match_details<M: Clone + Copy>(
    order_id: OrderId,
    account: [u8; 32],
    client_order_id: Option<Vec<u8>>,
    unique_priority_idx: IncreasingIdx,
    price: u64,
    orig_size: u64,
    remaining_size: u64,
    is_bid: bool,
    time_in_force: TimeInForce,
    creation_time_micros: u64,
    metadata: M,
) -> OrderMatchDetails<M> {
    OrderMatchDetails::SingleOrder {
        order_id,
        account,
        client_order_id,
        unique_priority_idx,
        price,
        orig_size,
        remaining_size,
        is_bid,
        time_in_force,
        creation_time_micros,
        metadata,
    }
}

pub fn new_bulk_order_match_details<M: Clone + Copy>(
    order_id: OrderId,
    account: [u8; 32],
    unique_priority_idx: IncreasingIdx,
    price: u64,
    remaining_size: u64,
    is_bid: bool,
    sequence_number: u64,
    creation_time_micros: u64,
    metadata: M,
) -> OrderMatchDetails<M> {
    OrderMatchDetails::BulkOrder {
        order_id,
        account,
        unique_priority_idx,
        price,
        remaining_size,
        is_bid,
        sequence_number,
        creation_time_micros,
        metadata,
    }
}

pub fn new_order_match<M: Clone + Copy>(
    order: OrderMatchDetails<M>,
    matched_size: u64,
) -> OrderMatch<M> {
    OrderMatch::V1 {
        order,
        matched_size,
    }
}

pub fn new_order_match_details_with_modified_size<M: Clone + Copy>(
    original: &OrderMatchDetails<M>,
    new_remaining_size: u64,
) -> OrderMatchDetails<M> {
    let mut res = original.clone();
    match res {
        OrderMatchDetails::SingleOrder {
            ref mut remaining_size,
            ..
        } => *remaining_size = new_remaining_size,
        OrderMatchDetails::BulkOrder {
            ref mut remaining_size,
            ..
        } => *remaining_size = new_remaining_size,
    }
    res
}

pub fn new_active_matched_order(
    order_id: OrderId,
    matched_size: u64,
    remaining_size: u64,
    order_book_type: OrderType,
) -> ActiveMatchedOrder {
    ActiveMatchedOrder::V1 {
        order_id,
        matched_size,
        remaining_size,
        order_book_type,
    }
}

// ===================== Getters =====================

pub fn get_matched_size<M: Clone>(order_match: &OrderMatch<M>) -> u64 {
    let OrderMatch::V1 { matched_size, .. } = order_match;
    *matched_size
}

pub fn get_account_from_match_details<M: Clone + Copy>(
    details: &OrderMatchDetails<M>,
) -> [u8; 32] {
    match details {
        OrderMatchDetails::SingleOrder { account, .. } => *account,
        OrderMatchDetails::BulkOrder { account, .. } => *account,
    }
}

pub fn get_order_id_from_match_details<M: Clone + Copy>(
    details: &OrderMatchDetails<M>,
) -> OrderId {
    match details {
        OrderMatchDetails::SingleOrder { order_id, .. } => *order_id,
        OrderMatchDetails::BulkOrder { order_id, .. } => *order_id,
    }
}

pub fn get_unique_priority_idx_from_match_details<M: Clone + Copy>(
    details: &OrderMatchDetails<M>,
) -> IncreasingIdx {
    match details {
        OrderMatchDetails::SingleOrder {
            unique_priority_idx,
            ..
        } => *unique_priority_idx,
        OrderMatchDetails::BulkOrder {
            unique_priority_idx,
            ..
        } => *unique_priority_idx,
    }
}

pub fn get_price_from_match_details<M: Clone + Copy>(details: &OrderMatchDetails<M>) -> u64 {
    match details {
        OrderMatchDetails::SingleOrder { price, .. } => *price,
        OrderMatchDetails::BulkOrder { price, .. } => *price,
    }
}

pub fn get_orig_size_from_match_details<M: Clone + Copy>(details: &OrderMatchDetails<M>) -> u64 {
    match details {
        OrderMatchDetails::SingleOrder { orig_size, .. } => *orig_size,
        OrderMatchDetails::BulkOrder { .. } => {
            panic!("get_orig_size_from_match_details called on BulkOrder")
        },
    }
}

pub fn get_remaining_size_from_match_details<M: Clone + Copy>(
    details: &OrderMatchDetails<M>,
) -> u64 {
    match details {
        OrderMatchDetails::SingleOrder {
            remaining_size, ..
        } => *remaining_size,
        OrderMatchDetails::BulkOrder {
            remaining_size, ..
        } => *remaining_size,
    }
}

pub fn get_time_in_force_from_match_details<M: Clone + Copy>(
    details: &OrderMatchDetails<M>,
) -> TimeInForce {
    match details {
        OrderMatchDetails::SingleOrder { time_in_force, .. } => *time_in_force,
        OrderMatchDetails::BulkOrder { .. } => good_till_cancelled(),
    }
}

pub fn get_metadata_from_match_details<M: Clone + Copy>(details: &OrderMatchDetails<M>) -> M {
    match details {
        OrderMatchDetails::SingleOrder { metadata, .. } => *metadata,
        OrderMatchDetails::BulkOrder { metadata, .. } => *metadata,
    }
}

pub fn get_client_order_id_from_match_details<M: Clone + Copy>(
    details: &OrderMatchDetails<M>,
) -> Option<Vec<u8>> {
    match details {
        OrderMatchDetails::SingleOrder {
            client_order_id, ..
        } => client_order_id.clone(),
        OrderMatchDetails::BulkOrder { .. } => None,
    }
}

pub fn is_bid_from_match_details<M: Clone + Copy>(details: &OrderMatchDetails<M>) -> bool {
    match details {
        OrderMatchDetails::SingleOrder { is_bid, .. } => *is_bid,
        OrderMatchDetails::BulkOrder { is_bid, .. } => *is_bid,
    }
}

pub fn get_book_type_from_match_details<M: Clone + Copy>(
    details: &OrderMatchDetails<M>,
) -> OrderType {
    match details {
        OrderMatchDetails::SingleOrder { .. } => single_order_type(),
        OrderMatchDetails::BulkOrder { .. } => bulk_order_type(),
    }
}

pub fn is_bulk_order_from_match_details<M: Clone + Copy>(
    details: &OrderMatchDetails<M>,
) -> bool {
    matches!(details, OrderMatchDetails::BulkOrder { .. })
}

pub fn is_single_order_from_match_details<M: Clone + Copy>(
    details: &OrderMatchDetails<M>,
) -> bool {
    matches!(details, OrderMatchDetails::SingleOrder { .. })
}

pub fn get_sequence_number_from_match_details<M: Clone + Copy>(
    details: &OrderMatchDetails<M>,
) -> u64 {
    match details {
        OrderMatchDetails::BulkOrder {
            sequence_number, ..
        } => *sequence_number,
        _ => panic!("get_sequence_number_from_match_details called on non-BulkOrder"),
    }
}

pub fn get_creation_time_micros_from_match_details<M: Clone + Copy>(
    details: &OrderMatchDetails<M>,
) -> u64 {
    match details {
        OrderMatchDetails::SingleOrder {
            creation_time_micros,
            ..
        } => *creation_time_micros,
        OrderMatchDetails::BulkOrder {
            creation_time_micros,
            ..
        } => *creation_time_micros,
    }
}

// ===================== Destructure =====================

pub fn destroy_order_match<M: Clone + Copy>(
    order_match: OrderMatch<M>,
) -> (OrderMatchDetails<M>, u64) {
    let OrderMatch::V1 {
        order,
        matched_size,
    } = order_match;
    (order, matched_size)
}

pub fn destroy_single_order_match_details<M: Clone + Copy>(
    details: OrderMatchDetails<M>,
) -> (
    OrderId,
    [u8; 32],
    Option<Vec<u8>>,
    IncreasingIdx,
    u64,
    u64,
    u64,
    bool,
    TimeInForce,
    u64,
    M,
) {
    match details {
        OrderMatchDetails::SingleOrder {
            order_id,
            account,
            client_order_id,
            unique_priority_idx,
            price,
            orig_size,
            remaining_size,
            is_bid,
            time_in_force,
            creation_time_micros,
            metadata,
        } => (
            order_id,
            account,
            client_order_id,
            unique_priority_idx,
            price,
            orig_size,
            remaining_size,
            is_bid,
            time_in_force,
            creation_time_micros,
            metadata,
        ),
        _ => panic!("destroy_single_order_match_details called on non-SingleOrder"),
    }
}

pub fn destroy_active_matched_order(
    order: ActiveMatchedOrder,
) -> (OrderId, u64, u64, OrderType) {
    let ActiveMatchedOrder::V1 {
        order_id,
        matched_size,
        remaining_size,
        order_book_type,
    } = order;
    (order_id, matched_size, remaining_size, order_book_type)
}

pub fn get_active_matched_size(order: &ActiveMatchedOrder) -> u64 {
    let ActiveMatchedOrder::V1 { matched_size, .. } = order;
    *matched_size
}

pub fn is_active_matched_book_type_single_order(order: &ActiveMatchedOrder) -> bool {
    let ActiveMatchedOrder::V1 {
        order_book_type, ..
    } = order;
    is_single_order_type(order_book_type)
}

// ===================== Validation =====================

pub fn validate_single_order_reinsertion_request<M: Clone + Copy>(
    self_details: &OrderMatchDetails<M>,
    other: &OrderMatchDetails<M>,
) -> bool {
    match (self_details, other) {
        (
            OrderMatchDetails::SingleOrder {
                order_id: s_oid,
                account: s_acc,
                unique_priority_idx: s_upi,
                price: s_price,
                orig_size: s_orig,
                is_bid: s_bid,
                ..
            },
            OrderMatchDetails::SingleOrder {
                order_id: o_oid,
                account: o_acc,
                unique_priority_idx: o_upi,
                price: o_price,
                orig_size: o_orig,
                is_bid: o_bid,
                ..
            },
        ) => {
            s_oid == o_oid
                && s_acc == o_acc
                && s_upi == o_upi
                && s_price == o_price
                && s_orig == o_orig
                && s_bid == o_bid
        },
        _ => false,
    }
}

pub fn validate_bulk_order_reinsertion_request<M: Clone + Copy>(
    self_details: &OrderMatchDetails<M>,
    other: &OrderMatchDetails<M>,
) -> bool {
    match (self_details, other) {
        (
            OrderMatchDetails::BulkOrder {
                order_id: s_oid,
                account: s_acc,
                unique_priority_idx: s_upi,
                price: s_price,
                is_bid: s_bid,
                sequence_number: s_seq,
                ..
            },
            OrderMatchDetails::BulkOrder {
                order_id: o_oid,
                account: o_acc,
                unique_priority_idx: o_upi,
                price: o_price,
                is_bid: o_bid,
                sequence_number: o_seq,
                ..
            },
        ) => {
            s_oid == o_oid
                && s_acc == o_acc
                && s_upi == o_upi
                && s_price == o_price
                && s_bid == o_bid
                && s_seq == o_seq
        },
        _ => false,
    }
}
