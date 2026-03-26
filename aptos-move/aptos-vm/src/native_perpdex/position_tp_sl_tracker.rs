// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::position_tp_sl_tracker

use crate::native_perpdex::builder_code_registry::BuilderCode;
use crate::native_perpdex::order_book_types::OrderId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===================== Constants =====================

const E_INVALID_TP_SL_SIZE_INCREASE: u64 = 1;

// ===================== Types =====================

/// Represents an Object<PerpMarket> address
pub type PerpMarketRef = [u8; 32];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PriceIndexKey {
    pub trigger_price: u64,
    pub account: [u8; 32],
    pub limit_price: Option<u64>,
    pub is_full_size: bool,
    pub builder_code: Option<BuilderCode>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PendingRequest {
    V1 {
        order_id: OrderId,
        account: [u8; 32],
        limit_price: Option<u64>,
        size: Option<u64>,
        builder_code: Option<BuilderCode>,
    },
}

/// In Move this is stored as a resource on the market object.
/// In Rust we pass it as a mutable parameter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingOrderTracker {
    pub price_move_up_index: BTreeMap<PriceIndexKey, PendingRequest>,
    pub price_move_down_index: BTreeMap<PriceIndexKey, PendingRequest>,
}

// ===================== Functions =====================

pub fn new_price_index_key(
    trigger_price: u64,
    account: [u8; 32],
    limit_price: Option<u64>,
    is_full_size: bool,
    builder_code: Option<BuilderCode>,
) -> PriceIndexKey {
    PriceIndexKey {
        trigger_price,
        account,
        limit_price,
        is_full_size,
        builder_code,
    }
}

pub fn get_trigger_price(key: &PriceIndexKey) -> u64 {
    key.trigger_price
}

impl PendingOrderTracker {
    pub fn new() -> Self {
        PendingOrderTracker {
            price_move_up_index: BTreeMap::new(),
            price_move_down_index: BTreeMap::new(),
        }
    }
}

impl PendingRequest {
    pub fn get_account(&self) -> [u8; 32] {
        match self {
            PendingRequest::V1 { account, .. } => *account,
        }
    }

    pub fn get_order_id(&self) -> OrderId {
        match self {
            PendingRequest::V1 { order_id, .. } => *order_id,
        }
    }

    pub fn get_size(&self) -> Option<u64> {
        match self {
            PendingRequest::V1 { size, .. } => *size,
        }
    }

    pub fn destroy(self) -> ([u8; 32], OrderId, Option<u64>, Option<u64>, Option<BuilderCode>) {
        match self {
            PendingRequest::V1 {
                account,
                order_id,
                limit_price,
                size,
                builder_code,
            } => (account, order_id, limit_price, size, builder_code),
        }
    }
}

/// Get ready price-move-up orders (mark_price >= trigger_price)
pub fn get_ready_price_move_up_orders(
    tracker: &PendingOrderTracker,
    mark_price: u64,
    limit: u64,
) -> Vec<PendingRequest> {
    let mut ready_orders = Vec::new();
    for (key, pending_request) in &tracker.price_move_up_index {
        if ready_orders.len() as u64 >= limit {
            break;
        }
        if mark_price >= key.trigger_price {
            let PendingRequest::V1 {
                account,
                order_id,
                limit_price,
                size,
                builder_code: _,
            } = pending_request;
            ready_orders.push(PendingRequest::V1 {
                account: *account,
                order_id: *order_id,
                limit_price: *limit_price,
                size: *size,
                builder_code: key.builder_code,
            });
        } else {
            break;
        }
    }
    ready_orders
}

/// Get ready price-move-down orders (mark_price <= trigger_price)
pub fn get_ready_price_move_down_orders(
    tracker: &PendingOrderTracker,
    mark_price: u64,
    limit: u64,
) -> Vec<PendingRequest> {
    let mut ready_orders = Vec::new();
    for (key, pending_request) in tracker.price_move_down_index.iter().rev() {
        if ready_orders.len() as u64 >= limit {
            break;
        }
        if mark_price <= key.trigger_price {
            let PendingRequest::V1 {
                account,
                order_id,
                limit_price,
                size,
                builder_code: _,
            } = pending_request;
            ready_orders.push(PendingRequest::V1 {
                account: *account,
                order_id: *order_id,
                limit_price: *limit_price,
                size: *size,
                builder_code: key.builder_code,
            });
        } else {
            break;
        }
    }
    ready_orders
}

/// Take (remove) ready price-move-up orders
pub fn take_ready_price_move_up_orders(
    tracker: &mut PendingOrderTracker,
    mark_price: u64,
    limit: u32,
) -> Vec<PendingRequest> {
    let mut ready_orders = Vec::new();
    while !tracker.price_move_up_index.is_empty() && (ready_orders.len() as u32) < limit {
        let first_key = tracker.price_move_up_index.keys().next().cloned();
        if let Some(key) = first_key {
            if mark_price >= key.trigger_price {
                let pending_request = tracker.price_move_up_index.remove(&key).unwrap();
                let PendingRequest::V1 {
                    account,
                    order_id,
                    limit_price,
                    size,
                    builder_code: _,
                } = pending_request;
                ready_orders.push(PendingRequest::V1 {
                    account,
                    order_id,
                    limit_price,
                    size,
                    builder_code: key.builder_code,
                });
            } else {
                break;
            }
        } else {
            break;
        }
    }
    ready_orders
}

/// Take (remove) ready price-move-down orders
pub fn take_ready_price_move_down_orders(
    tracker: &mut PendingOrderTracker,
    mark_price: u64,
    limit: u32,
) -> Vec<PendingRequest> {
    let mut ready_orders = Vec::new();
    while !tracker.price_move_down_index.is_empty() && (ready_orders.len() as u32) < limit {
        let last_key = tracker.price_move_down_index.keys().next_back().cloned();
        if let Some(key) = last_key {
            if mark_price <= key.trigger_price {
                let mut pending_request = tracker.price_move_down_index.remove(&key).unwrap();
                match &mut pending_request {
                    PendingRequest::V1 { builder_code, .. } => {
                        *builder_code = key.builder_code;
                    },
                }
                ready_orders.push(pending_request);
            } else {
                break;
            }
        } else {
            break;
        }
    }
    ready_orders
}

pub fn cancel_pending_tp_sl(
    tracker: &mut PendingOrderTracker,
    price_key: &PriceIndexKey,
    is_tp: bool,
    is_position_long: bool,
) {
    let index = if is_tp == is_position_long {
        &mut tracker.price_move_up_index
    } else {
        &mut tracker.price_move_down_index
    };
    index.remove(price_key);
}

pub fn add_new_tp_sl(
    tracker: &mut PendingOrderTracker,
    account: [u8; 32],
    order_id: OrderId,
    key: PriceIndexKey,
    limit_price: Option<u64>,
    size: Option<u64>,
    is_tp: bool,
    is_position_long: bool,
) {
    let request = PendingRequest::V1 {
        account,
        order_id,
        limit_price,
        size,
        builder_code: key.builder_code,
    };
    let index = if is_tp == is_position_long {
        &mut tracker.price_move_up_index
    } else {
        &mut tracker.price_move_down_index
    };
    index.insert(key, request);
}

pub fn increase_fixed_sized_pending_tp_sl_size(
    tracker: &mut PendingOrderTracker,
    key: &PriceIndexKey,
    size_delta: u64,
    is_tp: bool,
    is_position_long: bool,
    position_size: u64,
) {
    let index = if is_tp == is_position_long {
        &mut tracker.price_move_up_index
    } else {
        &mut tracker.price_move_down_index
    };
    let mut pending_request = index.remove(key).expect("key not found");
    match &mut pending_request {
        PendingRequest::V1 { size, .. } => {
            assert!(size.is_some(), "Invalid TP/SL size increase: {}", E_INVALID_TP_SL_SIZE_INCREASE);
            let current_size = size.unwrap();
            *size = Some(std::cmp::min(current_size + size_delta, position_size));
        },
    }
    index.insert(key.clone(), pending_request);
}

pub fn get_pending_order_id(
    tracker: &PendingOrderTracker,
    key: &PriceIndexKey,
    is_tp: bool,
    is_position_long: bool,
) -> Option<OrderId> {
    let index = if is_tp == is_position_long {
        &tracker.price_move_up_index
    } else {
        &tracker.price_move_down_index
    };
    index.get(key).map(|req| req.get_order_id())
}

pub fn get_pending_tp_sl(
    tracker: &PendingOrderTracker,
    key: &PriceIndexKey,
    is_tp: bool,
    is_position_long: bool,
) -> ([u8; 32], OrderId, Option<u64>, Option<u64>, Option<BuilderCode>) {
    let index = if is_tp == is_position_long {
        &tracker.price_move_up_index
    } else {
        &tracker.price_move_down_index
    };
    let req = index.get(key).expect("key not found").clone();
    req.destroy()
}


// ===================== Stub functions for perp_engine delegation =====================

pub fn destroy_pending_request(
    _request: PendingRequest,
) -> (
    [u8; 32],                                                     // account
    crate::native_perpdex::order_book_types::OrderId,            // order_id
    Option<u64>,                                                  // limit_price
    Option<u64>,                                                  // size
    Option<crate::native_perpdex::builder_code_registry::BuilderCode>, // builder_code
) {
    ([0u8; 32], crate::native_perpdex::order_book_types::OrderId { order_id: 0 }, None, None, None)
}
