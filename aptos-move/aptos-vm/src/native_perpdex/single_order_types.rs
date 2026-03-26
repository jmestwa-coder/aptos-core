// Copyright (c) Aptos Foundation
// Translated from: aptos_trading::single_order_types

use crate::native_perpdex::order_book_types::{IncreasingIdx, OrderId, TimeInForce, TriggerCondition};
use crate::native_perpdex::order_match_types::{
    destroy_single_order_match_details, OrderMatchDetails,
};

// ===================== Constants =====================

const EINVALID_ORDER_SIZE_DECREASE: u64 = 1;

// ===================== Types =====================

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SingleOrderRequest<M: Clone> {
    V1 {
        account: [u8; 32],
        order_id: OrderId,
        client_order_id: Option<Vec<u8>>,
        price: u64,
        orig_size: u64,
        remaining_size: u64,
        is_bid: bool,
        trigger_condition: Option<TriggerCondition>,
        time_in_force: TimeInForce,
        creation_time_micros: u64,
        metadata: M,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SingleOrder<M: Clone> {
    V1 {
        order_request: SingleOrderRequest<M>,
        unique_priority_idx: IncreasingIdx,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrderWithState<M: Clone> {
    V1 {
        order: SingleOrder<M>,
        is_active: bool,
    },
}

// ===================== Constructors =====================

pub fn new_order_request_from_match_details<M: Clone + Copy>(
    order_match_details: OrderMatchDetails<M>,
) -> SingleOrderRequest<M> {
    let (
        order_id,
        account,
        client_order_id,
        _unique_priority_idx,
        price,
        orig_size,
        remaining_size,
        is_bid,
        time_in_force,
        creation_time_micros,
        metadata,
    ) = destroy_single_order_match_details(order_match_details);
    SingleOrderRequest::V1 {
        account,
        order_id,
        client_order_id,
        price,
        orig_size,
        remaining_size,
        is_bid,
        trigger_condition: None,
        time_in_force,
        creation_time_micros,
        metadata,
    }
}

pub fn new_single_order<M: Clone>(
    order_request: SingleOrderRequest<M>,
    unique_priority_idx: IncreasingIdx,
) -> SingleOrder<M> {
    SingleOrder::V1 {
        order_request,
        unique_priority_idx,
    }
}

pub fn new_order_with_state<M: Clone>(
    order: SingleOrder<M>,
    is_active: bool,
) -> OrderWithState<M> {
    OrderWithState::V1 { order, is_active }
}

pub fn new_single_order_request<M: Clone>(
    account: [u8; 32],
    order_id: OrderId,
    client_order_id: Option<Vec<u8>>,
    price: u64,
    orig_size: u64,
    remaining_size: u64,
    is_bid: bool,
    trigger_condition: Option<TriggerCondition>,
    time_in_force: TimeInForce,
    creation_time_micros: u64,
    metadata: M,
) -> SingleOrderRequest<M> {
    SingleOrderRequest::V1 {
        account,
        order_id,
        client_order_id,
        price,
        orig_size,
        remaining_size,
        is_bid,
        trigger_condition,
        time_in_force,
        creation_time_micros,
        metadata,
    }
}

// ===================== Getters for SingleOrderRequest =====================

pub fn get_order_id<M: Clone>(req: &SingleOrderRequest<M>) -> OrderId {
    let SingleOrderRequest::V1 { order_id, .. } = req;
    *order_id
}

pub fn get_account<M: Clone>(req: &SingleOrderRequest<M>) -> [u8; 32] {
    let SingleOrderRequest::V1 { account, .. } = req;
    *account
}

pub fn get_trigger_condition<M: Clone>(req: &SingleOrderRequest<M>) -> Option<TriggerCondition> {
    let SingleOrderRequest::V1 {
        trigger_condition, ..
    } = req;
    *trigger_condition
}

pub fn get_remaining_size<M: Clone>(req: &SingleOrderRequest<M>) -> u64 {
    let SingleOrderRequest::V1 {
        remaining_size, ..
    } = req;
    *remaining_size
}

pub fn set_remaining_size<M: Clone>(req: &mut SingleOrderRequest<M>, new_size: u64) {
    match req {
        SingleOrderRequest::V1 {
            remaining_size, ..
        } => *remaining_size = new_size,
    }
}

pub fn get_client_order_id<M: Clone>(req: &SingleOrderRequest<M>) -> Option<Vec<u8>> {
    let SingleOrderRequest::V1 {
        client_order_id, ..
    } = req;
    client_order_id.clone()
}

pub fn get_price<M: Clone>(req: &SingleOrderRequest<M>) -> u64 {
    let SingleOrderRequest::V1 { price, .. } = req;
    *price
}

pub fn is_bid<M: Clone>(req: &SingleOrderRequest<M>) -> bool {
    let SingleOrderRequest::V1 { is_bid, .. } = req;
    *is_bid
}

pub fn get_creation_time_micros<M: Clone>(req: &SingleOrderRequest<M>) -> u64 {
    let SingleOrderRequest::V1 {
        creation_time_micros,
        ..
    } = req;
    *creation_time_micros
}

// ===================== Getters for SingleOrder =====================

pub fn get_unique_priority_idx<M: Clone>(order: &SingleOrder<M>) -> IncreasingIdx {
    let SingleOrder::V1 {
        unique_priority_idx,
        ..
    } = order;
    *unique_priority_idx
}

pub fn get_order_request<M: Clone>(order: &SingleOrder<M>) -> &SingleOrderRequest<M> {
    let SingleOrder::V1 { order_request, .. } = order;
    order_request
}

// ===================== Getters for OrderWithState =====================

pub fn get_order_from_state<M: Clone>(state: &OrderWithState<M>) -> &SingleOrder<M> {
    let OrderWithState::V1 { order, .. } = state;
    order
}

pub fn get_metadata_from_state<M: Clone + Copy>(state: &OrderWithState<M>) -> M {
    let OrderWithState::V1 { order, .. } = state;
    let SingleOrder::V1 { order_request, .. } = order;
    let SingleOrderRequest::V1 { metadata, .. } = order_request;
    *metadata
}

pub fn set_metadata_in_state<M: Clone>(state: &mut OrderWithState<M>, new_metadata: M) {
    match state {
        OrderWithState::V1 { order, .. } => match order {
            SingleOrder::V1 { order_request, .. } => match order_request {
                SingleOrderRequest::V1 { metadata, .. } => *metadata = new_metadata,
            },
        },
    }
}

pub fn increase_remaining_size_from_state<M: Clone>(state: &mut OrderWithState<M>, size: u64) {
    match state {
        OrderWithState::V1 { order, .. } => match order {
            SingleOrder::V1 { order_request, .. } => match order_request {
                SingleOrderRequest::V1 {
                    remaining_size, ..
                } => *remaining_size += size,
            },
        },
    }
}

pub fn decrease_remaining_size_from_state<M: Clone>(
    state: &mut OrderWithState<M>,
    size: u64,
) -> Result<(), u64> {
    match state {
        OrderWithState::V1 { order, .. } => match order {
            SingleOrder::V1 { order_request, .. } => match order_request {
                SingleOrderRequest::V1 {
                    remaining_size, ..
                } => {
                    if *remaining_size <= size {
                        return Err(EINVALID_ORDER_SIZE_DECREASE);
                    }
                    *remaining_size -= size;
                    Ok(())
                },
            },
        },
    }
}

pub fn set_remaining_size_from_state<M: Clone>(
    state: &mut OrderWithState<M>,
    new_remaining_size: u64,
) {
    match state {
        OrderWithState::V1 { order, .. } => match order {
            SingleOrder::V1 { order_request, .. } => match order_request {
                SingleOrderRequest::V1 {
                    remaining_size, ..
                } => *remaining_size = new_remaining_size,
            },
        },
    }
}

pub fn get_remaining_size_from_state<M: Clone>(state: &OrderWithState<M>) -> u64 {
    let OrderWithState::V1 { order, .. } = state;
    let SingleOrder::V1 { order_request, .. } = order;
    let SingleOrderRequest::V1 {
        remaining_size, ..
    } = order_request;
    *remaining_size
}

pub fn get_unique_priority_idx_from_state<M: Clone>(state: &OrderWithState<M>) -> IncreasingIdx {
    let OrderWithState::V1 { order, .. } = state;
    let SingleOrder::V1 {
        unique_priority_idx,
        ..
    } = order;
    *unique_priority_idx
}

pub fn is_active_order<M: Clone>(state: &OrderWithState<M>) -> bool {
    let OrderWithState::V1 { is_active, .. } = state;
    *is_active
}

// ===================== Destructure =====================

pub fn destroy_order_from_state<M: Clone>(
    state: OrderWithState<M>,
) -> (SingleOrder<M>, bool) {
    let OrderWithState::V1 { order, is_active } = state;
    (order, is_active)
}

pub fn destroy_single_order<M: Clone>(
    order: SingleOrder<M>,
) -> (SingleOrderRequest<M>, IncreasingIdx) {
    let SingleOrder::V1 {
        order_request,
        unique_priority_idx,
    } = order;
    (order_request, unique_priority_idx)
}

pub fn destroy_single_order_request<M: Clone>(
    req: SingleOrderRequest<M>,
) -> (
    [u8; 32],
    OrderId,
    Option<Vec<u8>>,
    u64,
    u64,
    u64,
    bool,
    Option<TriggerCondition>,
    TimeInForce,
    u64,
    M,
) {
    let SingleOrderRequest::V1 {
        account,
        order_id,
        client_order_id,
        price,
        orig_size,
        remaining_size,
        is_bid,
        trigger_condition,
        time_in_force,
        creation_time_micros,
        metadata,
    } = req;
    (
        account,
        order_id,
        client_order_id,
        price,
        orig_size,
        remaining_size,
        is_bid,
        trigger_condition,
        time_in_force,
        creation_time_micros,
        metadata,
    )
}
