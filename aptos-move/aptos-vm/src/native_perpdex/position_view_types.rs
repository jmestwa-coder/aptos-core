// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::position_view_types

use crate::native_perpdex::price_management::AccumulativeIndex;
use serde::{Deserialize, Serialize};

// ===================== Types =====================

/// Represents a PerpMarket object reference - in native Rust we use an address placeholder
pub type PerpMarketRef = [u8; 32];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PositionViewInfo {
    V1 {
        market: PerpMarketRef,
        size: u64,
        entry_px_times_size_sum: u128,
        avg_acquire_entry_px: u64,
        user_leverage: u8,
        is_long: bool,
        is_isolated: bool,
        funding_index_at_last_update: AccumulativeIndex,
        unrealized_funding_amount_before_last_update: i64,
    },
}

// ===================== Functions =====================

pub fn new_position_view_info(
    market: PerpMarketRef,
    size: u64,
    entry_px_times_size_sum: u128,
    avg_acquire_entry_px: u64,
    user_leverage: u8,
    is_long: bool,
    is_isolated: bool,
    funding_index_at_last_update: AccumulativeIndex,
    unrealized_funding_amount_before_last_update: i64,
) -> PositionViewInfo {
    PositionViewInfo::V1 {
        market,
        size,
        entry_px_times_size_sum,
        avg_acquire_entry_px,
        user_leverage,
        is_long,
        is_isolated,
        funding_index_at_last_update,
        unrealized_funding_amount_before_last_update,
    }
}

impl PositionViewInfo {
    pub fn get_position_info_market(&self) -> PerpMarketRef {
        match self {
            PositionViewInfo::V1 { market, .. } => *market,
        }
    }

    pub fn get_position_info_size(&self) -> u64 {
        match self {
            PositionViewInfo::V1 { size, .. } => *size,
        }
    }

    pub fn get_position_info_is_long(&self) -> bool {
        match self {
            PositionViewInfo::V1 { is_long, .. } => *is_long,
        }
    }

    pub fn get_position_info_user_leverage(&self) -> u8 {
        match self {
            PositionViewInfo::V1 { user_leverage, .. } => *user_leverage,
        }
    }

    pub fn get_position_info_is_isolated(&self) -> bool {
        match self {
            PositionViewInfo::V1 { is_isolated, .. } => *is_isolated,
        }
    }

    pub fn get_position_info_entry_px_times_size_sum(&self) -> u128 {
        match self {
            PositionViewInfo::V1 {
                entry_px_times_size_sum,
                ..
            } => *entry_px_times_size_sum,
        }
    }

    pub fn get_position_info_avg_acquire_entry_px(&self) -> u64 {
        match self {
            PositionViewInfo::V1 {
                avg_acquire_entry_px,
                ..
            } => *avg_acquire_entry_px,
        }
    }

    pub fn get_position_info_funding_index_at_last_update(&self) -> AccumulativeIndex {
        match self {
            PositionViewInfo::V1 {
                funding_index_at_last_update,
                ..
            } => funding_index_at_last_update.clone(),
        }
    }

    pub fn get_position_info_unrealized_funding_amount_before_last_update(&self) -> i64 {
        match self {
            PositionViewInfo::V1 {
                unrealized_funding_amount_before_last_update,
                ..
            } => *unrealized_funding_amount_before_last_update,
        }
    }
}
