// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::tp_sl_utils

use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const EINVALID_TP_SL_PARAMETERS: u64 = 1;
const E_TP_TRIGGER_PRICE_INVALID: u64 = 2;
const E_SL_TRIGGER_PRICE_INVALID: u64 = 3;

// ===================== Types =====================

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TpSlStatus {
    INACTIVE,
    ACTIVE,
}

/// Represents a child TP/SL order attached to a parent.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChildTpSlOrder {
    V1 {
        trigger_price: u64,
        parent_order_id: u128, // OrderId - field order must match Move
        limit_price: Option<u64>,
    },
}

// ===================== Functions =====================

pub fn get_inactive_tp_sl_status() -> TpSlStatus {
    TpSlStatus::INACTIVE
}

pub fn get_active_tp_sl_status() -> TpSlStatus {
    TpSlStatus::ACTIVE
}

/// Validate and construct child TP/SL orders from raw parameters.
/// Returns (tp_order, sl_order) where each is None if not specified.
pub fn validate_and_get_child_tp_sl_orders(
    parent_order_id: u128, // OrderId
    is_buy: bool,
    parent_limit_price: u64,
    tp_trigger_price: Option<u64>,
    tp_limit_price: Option<u64>,
    sl_trigger_price: Option<u64>,
    sl_limit_price: Option<u64>,
    validate_price_fn: &dyn Fn(u64) -> Result<(), u64>,
) -> Result<(Option<ChildTpSlOrder>, Option<ChildTpSlOrder>), u64> {
    // Validate TP prices
    if let Some(tp_trigger) = tp_trigger_price {
        validate_price_fn(tp_trigger)?;
        if is_buy {
            if tp_trigger < parent_limit_price {
                return Err(E_TP_TRIGGER_PRICE_INVALID);
            }
        } else {
            if tp_trigger > parent_limit_price {
                return Err(E_TP_TRIGGER_PRICE_INVALID);
            }
        }
    }
    if tp_limit_price.is_some() {
        let tp_lp = tp_limit_price.unwrap();
        validate_price_fn(tp_lp)?;
        if tp_trigger_price.is_none() {
            return Err(EINVALID_TP_SL_PARAMETERS);
        }
    }

    // Validate SL prices
    if let Some(sl_trigger) = sl_trigger_price {
        validate_price_fn(sl_trigger)?;
        if is_buy {
            if sl_trigger > parent_limit_price {
                return Err(E_SL_TRIGGER_PRICE_INVALID);
            }
        } else {
            if sl_trigger < parent_limit_price {
                return Err(E_SL_TRIGGER_PRICE_INVALID);
            }
        }
    }
    if sl_limit_price.is_some() {
        let sl_lp = sl_limit_price.unwrap();
        validate_price_fn(sl_lp)?;
        if sl_trigger_price.is_none() {
            return Err(EINVALID_TP_SL_PARAMETERS);
        }
    }

    let tp = tp_trigger_price.map(|trigger_price| ChildTpSlOrder::V1 {
        trigger_price,
        parent_order_id,
        limit_price: tp_limit_price,
    });

    let sl = sl_trigger_price.map(|trigger_price| ChildTpSlOrder::V1 {
        trigger_price,
        parent_order_id,
        limit_price: sl_limit_price,
    });

    Ok((tp, sl))
}


// ===================== Stub functions for perp_engine delegation =====================

pub fn process_tp_sl_order(
    _market: [u8; 32],
    _account: [u8; 32],
    _trigger_price: Option<u64>,
    _limit_price: Option<u64>,
    _size: Option<u64>,
    _is_tp: bool,
    _builder_code: Option<crate::native_perpdex::builder_code_registry::BuilderCode>,
) -> Result<Option<crate::native_perpdex::order_book_types::OrderId>, u64> {
    if _trigger_price.is_some() {
        Ok(Some(crate::native_perpdex::order_book_types::OrderId { order_id: 0 }))
    } else {
        if _limit_price.is_some() || _size.is_some() {
            return Err(5); // EINVALID_TP_SL_PARAMETERS
        }
        Ok(None)
    }
}
