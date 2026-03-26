// Copyright (c) Aptos Foundation
// Translated from: aptos_market::market_clearinghouse_order_info

use crate::native_perpdex::order_book_types::{OrderId, OrderType, TimeInForce, TriggerCondition};

// ===================== Types =====================

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarketClearinghouseOrderInfo<M: Clone + Copy> {
    V1 {
        account: [u8; 32],
        order_id: OrderId,
        client_order_id: Option<Vec<u8>>,
        is_bid: bool,
        limit_price: u64,
        time_in_force: TimeInForce,
        order_type: OrderType,
        trigger_condition: Option<TriggerCondition>,
        metadata: M,
    },
}

// ===================== Constructor =====================

pub fn new_clearinghouse_order_info<M: Clone + Copy>(
    account: [u8; 32],
    order_id: OrderId,
    client_order_id: Option<Vec<u8>>,
    is_bid: bool,
    limit_price: u64,
    time_in_force: TimeInForce,
    order_type: OrderType,
    trigger_condition: Option<TriggerCondition>,
    metadata: M,
) -> MarketClearinghouseOrderInfo<M> {
    MarketClearinghouseOrderInfo::V1 {
        account,
        order_id,
        client_order_id,
        is_bid,
        limit_price,
        time_in_force,
        order_type,
        trigger_condition,
        metadata,
    }
}

// ===================== Getters =====================

pub fn get_account<M: Clone + Copy>(info: &MarketClearinghouseOrderInfo<M>) -> [u8; 32] {
    let MarketClearinghouseOrderInfo::V1 { account, .. } = info;
    *account
}

pub fn get_order_id<M: Clone + Copy>(info: &MarketClearinghouseOrderInfo<M>) -> OrderId {
    let MarketClearinghouseOrderInfo::V1 { order_id, .. } = info;
    *order_id
}

pub fn is_bid<M: Clone + Copy>(info: &MarketClearinghouseOrderInfo<M>) -> bool {
    let MarketClearinghouseOrderInfo::V1 { is_bid, .. } = info;
    *is_bid
}

pub fn get_client_order_id<M: Clone + Copy>(
    info: &MarketClearinghouseOrderInfo<M>,
) -> Option<Vec<u8>> {
    let MarketClearinghouseOrderInfo::V1 {
        client_order_id, ..
    } = info;
    client_order_id.clone()
}

pub fn get_metadata<M: Clone + Copy>(info: &MarketClearinghouseOrderInfo<M>) -> &M {
    let MarketClearinghouseOrderInfo::V1 { metadata, .. } = info;
    metadata
}

pub fn into_inner<M: Clone + Copy>(
    info: MarketClearinghouseOrderInfo<M>,
) -> (
    [u8; 32],
    OrderId,
    Option<Vec<u8>>,
    bool,
    u64,
    TimeInForce,
    OrderType,
    Option<TriggerCondition>,
    M,
) {
    let MarketClearinghouseOrderInfo::V1 {
        account,
        order_id,
        client_order_id,
        is_bid,
        limit_price,
        time_in_force,
        order_type,
        trigger_condition,
        metadata,
    } = info;
    (
        account,
        order_id,
        client_order_id,
        is_bid,
        limit_price,
        time_in_force,
        order_type,
        trigger_condition,
        metadata,
    )
}
