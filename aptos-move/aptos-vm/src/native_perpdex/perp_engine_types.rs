// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::perp_engine_types

use crate::native_perpdex::builder_code_registry::BuilderCode;
use crate::native_perpdex::order_book_types::OrderId;
use crate::native_perpdex::tp_sl_utils::ChildTpSlOrder;
use serde::{Deserialize, Serialize};

// ===================== Types =====================

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderMetadata {
    V1_RETAIL {
        is_reduce_only: bool,
        use_backstop_liquidation_margin: bool,
        is_margin_call: bool,
        twap: Option<TwapMetadata>,
        tp_sl: TpSlMetadata,
        builder_code: Option<BuilderCode>,
    },
    V1_BULK {
        builder_code: Option<BuilderCode>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TpSlMetadata {
    V1 {
        tp: Option<ChildTpSlOrder>,
        sl: Option<ChildTpSlOrder>,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TwapMetadata {
    V1 {
        start_time_seconds: u64,
        frequency_seconds: u64,
        end_time_seconds: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderActions {
    V1 {
        actions: Vec<SingleOrderAction>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SingleOrderAction {
    CancelOrder {
        account: [u8; 32],
        order_id: OrderId,
    },
    ReduceOrderSize {
        account: [u8; 32],
        order_id: OrderId,
        size_delta: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderMatchingActions {
    SettleTradeMatchingActions(OrderActions),
    PlaceMakerOrderActions(OrderActions),
}

// ===================== Functions =====================

pub fn new_default_order_metadata() -> OrderMetadata {
    OrderMetadata::V1_RETAIL {
        is_reduce_only: false,
        use_backstop_liquidation_margin: false,
        is_margin_call: false,
        twap: None,
        tp_sl: TpSlMetadata::V1 {
            tp: None,
            sl: None,
        },
        builder_code: None,
    }
}

pub fn new_liquidation_metadata() -> OrderMetadata {
    OrderMetadata::V1_RETAIL {
        is_reduce_only: false,
        use_backstop_liquidation_margin: true,
        is_margin_call: true,
        twap: None,
        tp_sl: TpSlMetadata::V1 {
            tp: None,
            sl: None,
        },
        builder_code: None,
    }
}

pub fn is_reduce_only(order_metadata: &OrderMetadata) -> bool {
    match order_metadata {
        OrderMetadata::V1_RETAIL { is_reduce_only, .. } => *is_reduce_only,
        OrderMetadata::V1_BULK { .. } => false,
    }
}

pub fn use_backstop_liquidation_margin(order_metadata: &OrderMetadata) -> bool {
    match order_metadata {
        OrderMetadata::V1_RETAIL { use_backstop_liquidation_margin, .. } => *use_backstop_liquidation_margin,
        OrderMetadata::V1_BULK { .. } => false,
    }
}

pub fn is_margin_call(order_metadata: &OrderMetadata) -> bool {
    match order_metadata {
        OrderMetadata::V1_RETAIL { is_margin_call, .. } => *is_margin_call,
        OrderMetadata::V1_BULK { .. } => false,
    }
}

pub fn get_builder_code_from_metadata(order_metadata: &OrderMetadata) -> Option<BuilderCode> {
    match order_metadata {
        OrderMetadata::V1_RETAIL { builder_code, .. } => *builder_code,
        OrderMetadata::V1_BULK { builder_code } => *builder_code,
    }
}

pub fn new_order_metadata(
    is_reduce_only: bool,
    twap: Option<TwapMetadata>,
    tp: Option<ChildTpSlOrder>,
    sl: Option<ChildTpSlOrder>,
    builder_code: Option<BuilderCode>,
) -> OrderMetadata {
    OrderMetadata::V1_RETAIL {
        is_reduce_only,
        use_backstop_liquidation_margin: false,
        is_margin_call: false,
        twap,
        tp_sl: TpSlMetadata::V1 { tp, sl },
        builder_code,
    }
}

pub fn new_twap_metadata(
    start_time_seconds: u64,
    frequency_seconds: u64,
    end_time_seconds: u64,
) -> TwapMetadata {
    TwapMetadata::V1 {
        start_time_seconds,
        frequency_seconds,
        end_time_seconds,
    }
}

pub fn get_twap_from_metadata(
    order_metadata: &OrderMetadata,
) -> (u64, u64, u64) {
    match order_metadata {
        OrderMetadata::V1_RETAIL { twap, .. } => {
            match twap {
                Some(TwapMetadata::V1 {
                    start_time_seconds,
                    frequency_seconds,
                    end_time_seconds,
                }) => (*start_time_seconds, *frequency_seconds, *end_time_seconds),
                None => (0, 0, 0),
            }
        }
        OrderMetadata::V1_BULK { .. } => (0, 0, 0),
    }
}
