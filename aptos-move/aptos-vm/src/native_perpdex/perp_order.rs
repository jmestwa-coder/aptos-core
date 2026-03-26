// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::perp_order

use crate::native_perpdex::order_book_types::{OrderId, TimeInForce, TriggerCondition};
use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const EINVALID_CLIENT_ORDER_ID_LENGTH: u64 = 1;
const MAX_CLIENT_ORDER_ID_LENGTH: u64 = 32;

// ===================== Types =====================
// Move enums with V1 variant. BCS serialization: variant index (ULEB128) + fields.

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PerpOrderRequestCommonArgs {
    V1 {
        price: u64,
        orig_size: u64,
        is_buy: bool,
        time_in_force: TimeInForce,
        /// Move string is Vec<u8> in BCS
        client_order_id: Option<Vec<u8>>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PerpOrderRequestExtendedArgs {
    V1 {
        user: [u8; 32], // address
        common_args: PerpOrderRequestCommonArgs,
        order_id: OrderId,
        trigger_condition: Option<TriggerCondition>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum PerpOrderRequestTpSlArgs {
    V1 {
        tp_trigger_price: Option<u64>,
        tp_limit_price: Option<u64>,
        sl_trigger_price: Option<u64>,
        sl_limit_price: Option<u64>,
    },
}

// ===================== Constructor functions =====================

pub fn new_order_common_args(
    price: u64,
    orig_size: u64,
    is_buy: bool,
    time_in_force: TimeInForce,
    client_order_id: Option<Vec<u8>>,
) -> Result<PerpOrderRequestCommonArgs, u64> {
    // Validate client_order_id length if provided
    if let Some(ref id) = client_order_id {
        if id.len() as u64 > MAX_CLIENT_ORDER_ID_LENGTH {
            return Err(EINVALID_CLIENT_ORDER_ID_LENGTH);
        }
    }
    Ok(PerpOrderRequestCommonArgs::V1 {
        price,
        orig_size,
        is_buy,
        time_in_force,
        client_order_id,
    })
}

pub fn new_order_extended_args(
    user: [u8; 32],
    common_args: PerpOrderRequestCommonArgs,
    order_id: OrderId,
    trigger_condition: Option<TriggerCondition>,
) -> PerpOrderRequestExtendedArgs {
    PerpOrderRequestExtendedArgs::V1 {
        user,
        common_args,
        order_id,
        trigger_condition,
    }
}

pub fn new_order_tp_sl_args(
    tp_trigger_price: Option<u64>,
    tp_limit_price: Option<u64>,
    sl_trigger_price: Option<u64>,
    sl_limit_price: Option<u64>,
) -> PerpOrderRequestTpSlArgs {
    PerpOrderRequestTpSlArgs::V1 {
        tp_trigger_price,
        tp_limit_price,
        sl_trigger_price,
        sl_limit_price,
    }
}

pub fn new_empty_order_tp_sl_args() -> PerpOrderRequestTpSlArgs {
    PerpOrderRequestTpSlArgs::V1 {
        tp_trigger_price: None,
        tp_limit_price: None,
        sl_trigger_price: None,
        sl_limit_price: None,
    }
}

// ===================== Getter functions =====================

pub fn get_price(args: &PerpOrderRequestCommonArgs) -> u64 {
    let PerpOrderRequestCommonArgs::V1 { price, .. } = args;
    *price
}

pub fn get_orig_size(args: &PerpOrderRequestCommonArgs) -> u64 {
    let PerpOrderRequestCommonArgs::V1 { orig_size, .. } = args;
    *orig_size
}

pub fn get_is_buy(args: &PerpOrderRequestCommonArgs) -> bool {
    let PerpOrderRequestCommonArgs::V1 { is_buy, .. } = args;
    *is_buy
}

pub fn get_time_in_force(args: &PerpOrderRequestCommonArgs) -> TimeInForce {
    let PerpOrderRequestCommonArgs::V1 { time_in_force, .. } = args;
    *time_in_force
}

pub fn get_user(args: &PerpOrderRequestExtendedArgs) -> [u8; 32] {
    let PerpOrderRequestExtendedArgs::V1 { user, .. } = args;
    *user
}

pub fn get_common_args(
    args: &PerpOrderRequestExtendedArgs,
) -> &PerpOrderRequestCommonArgs {
    let PerpOrderRequestExtendedArgs::V1 { common_args, .. } = args;
    common_args
}

pub fn get_order_id(args: &PerpOrderRequestExtendedArgs) -> OrderId {
    let PerpOrderRequestExtendedArgs::V1 { order_id, .. } = args;
    *order_id
}

pub fn get_client_order_id(args: &PerpOrderRequestCommonArgs) -> Option<Vec<u8>> {
    let PerpOrderRequestCommonArgs::V1 {
        client_order_id, ..
    } = args;
    client_order_id.clone()
}

pub fn get_trigger_condition(
    args: &PerpOrderRequestExtendedArgs,
) -> Option<TriggerCondition> {
    let PerpOrderRequestExtendedArgs::V1 {
        trigger_condition, ..
    } = args;
    *trigger_condition
}

// ===================== Destructure functions =====================

pub fn extended_as_inner(
    args: &PerpOrderRequestExtendedArgs,
) -> (
    [u8; 32],
    &PerpOrderRequestCommonArgs,
    OrderId,
    Option<TriggerCondition>,
) {
    let PerpOrderRequestExtendedArgs::V1 {
        user,
        common_args,
        order_id,
        trigger_condition,
    } = args;
    (*user, common_args, *order_id, *trigger_condition)
}

pub fn extended_into_inner(
    args: PerpOrderRequestExtendedArgs,
) -> (
    [u8; 32],
    PerpOrderRequestCommonArgs,
    OrderId,
    Option<TriggerCondition>,
) {
    let PerpOrderRequestExtendedArgs::V1 {
        user,
        common_args,
        order_id,
        trigger_condition,
    } = args;
    (user, common_args, order_id, trigger_condition)
}

pub fn common_as_inner(
    args: &PerpOrderRequestCommonArgs,
) -> (u64, u64, bool, TimeInForce, Option<Vec<u8>>) {
    let PerpOrderRequestCommonArgs::V1 {
        price,
        orig_size,
        is_buy,
        time_in_force,
        client_order_id,
    } = args;
    (
        *price,
        *orig_size,
        *is_buy,
        *time_in_force,
        client_order_id.clone(),
    )
}

pub fn common_into_inner(
    args: PerpOrderRequestCommonArgs,
) -> (u64, u64, bool, TimeInForce, Option<Vec<u8>>) {
    let PerpOrderRequestCommonArgs::V1 {
        price,
        orig_size,
        is_buy,
        time_in_force,
        client_order_id,
    } = args;
    (price, orig_size, is_buy, time_in_force, client_order_id)
}

pub fn tpsl_into_inner(
    args: PerpOrderRequestTpSlArgs,
) -> (Option<u64>, Option<u64>, Option<u64>, Option<u64>) {
    let PerpOrderRequestTpSlArgs::V1 {
        tp_trigger_price,
        tp_limit_price,
        sl_trigger_price,
        sl_limit_price,
    } = args;
    (
        tp_trigger_price,
        tp_limit_price,
        sl_trigger_price,
        sl_limit_price,
    )
}
