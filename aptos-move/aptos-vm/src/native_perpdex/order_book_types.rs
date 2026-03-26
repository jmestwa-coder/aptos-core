// Copyright (c) Aptos Foundation
// Translated from: aptos_trading::order_book_types
// Shared type definitions used by perp_order and order_id_generation.

use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const EINVALID_TIME_IN_FORCE: u64 = 5;

// ===================== Structs =====================

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy)]
pub struct OrderId {
    pub order_id: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct AccountClientOrderId {
    pub account: [u8; 32], // address
    pub client_order_id: Vec<u8>, // Move String = UTF-8 bytes in BCS
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy)]
pub struct IncreasingIdx {
    pub idx: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy)]
pub struct DecreasingIdx {
    pub idx: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub struct OrderType {
    pub r#type: u16,
}

// ===================== TimeInForce enum =====================
// Move enum with 3 unit variants. BCS serialization: variant index (ULEB128) then fields.

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
#[allow(non_camel_case_types)]
pub enum TimeInForce {
    /// Good till cancelled (index 0)
    GTC,
    /// Post Only (index 1)
    POST_ONLY,
    /// Immediate or Cancel (index 2)
    IOC,
}

// ===================== TriggerCondition enum =====================

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum TriggerCondition {
    PriceMoveAbove(u64),
    PriceMoveBelow(u64),
    TimeBased(u64),
}

// ===================== Constants for OrderType =====================

const SINGLE_ORDER_TYPE: u16 = 0;
const BULK_ORDER_TYPE: u16 = 1;

// ===================== Functions =====================

pub fn single_order_type() -> OrderType {
    OrderType {
        r#type: SINGLE_ORDER_TYPE,
    }
}

pub fn bulk_order_type() -> OrderType {
    OrderType {
        r#type: BULK_ORDER_TYPE,
    }
}

pub fn is_bulk_order_type(order_type: &OrderType) -> bool {
    order_type.r#type == BULK_ORDER_TYPE
}

pub fn is_single_order_type(order_type: &OrderType) -> bool {
    order_type.r#type == SINGLE_ORDER_TYPE
}

pub fn new_order_id_type(order_id: u128) -> OrderId {
    OrderId { order_id }
}

pub fn new_account_client_order_id(
    account: [u8; 32],
    client_order_id: Vec<u8>,
) -> AccountClientOrderId {
    AccountClientOrderId {
        account,
        client_order_id,
    }
}

pub fn into_decreasing_idx_type(increasing: &IncreasingIdx) -> DecreasingIdx {
    DecreasingIdx {
        idx: u128::MAX - increasing.idx,
    }
}

pub fn get_order_id_value(order_id: &OrderId) -> u128 {
    order_id.order_id
}

pub fn time_in_force_from_index(index: u8) -> Result<TimeInForce, u64> {
    if index == 0 {
        Ok(TimeInForce::GTC)
    } else if index == 1 {
        Ok(TimeInForce::POST_ONLY)
    } else if index == 2 {
        Ok(TimeInForce::IOC)
    } else {
        Err(EINVALID_TIME_IN_FORCE)
    }
}

pub fn good_till_cancelled() -> TimeInForce {
    TimeInForce::GTC
}

pub fn post_only() -> TimeInForce {
    TimeInForce::POST_ONLY
}

pub fn immediate_or_cancel() -> TimeInForce {
    TimeInForce::IOC
}

pub fn new_time_based_trigger_condition(time_secs: u64) -> TriggerCondition {
    TriggerCondition::TimeBased(time_secs)
}

pub fn price_move_up_condition(price: u64) -> TriggerCondition {
    TriggerCondition::PriceMoveAbove(price)
}

pub fn price_move_down_condition(price: u64) -> TriggerCondition {
    TriggerCondition::PriceMoveBelow(price)
}

pub fn get_trigger_condition_indices(
    trigger: &TriggerCondition,
) -> (Option<u64>, Option<u64>, Option<u64>) {
    match trigger {
        TriggerCondition::PriceMoveAbove(price) => (None, Some(*price), None),
        TriggerCondition::PriceMoveBelow(price) => (Some(*price), None, None),
        TriggerCondition::TimeBased(time) => (None, None, Some(*time)),
    }
}
