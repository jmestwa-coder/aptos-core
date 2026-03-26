// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::internal_oracle_state

use serde::{Deserialize, Serialize};

// ===================== Types =====================

/// On-chain resource: InternalSourceState
/// In the native Rust context, this is passed as a parameter instead of being
/// read from global storage.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum InternalSourceState {
    V1 {
        /// Spot price
        spot_price: u64,
        /// Timestamp of the last update (seconds)
        update_time: u64,
        // source_ref: ExtendRef is not needed in native Rust context
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum InternalSourceIdentifier {
    V1 {
        object_address: [u8; 32],
    },
}

// ===================== Functions =====================

/// Updates the internal source price and timestamp.
/// RESOURCE_WRITE: InternalSourceState at self.object_address
pub fn update_internal_source_price(
    state: &mut InternalSourceState,
    new_value: u64,
    current_time_secs: u64,
) {
    let InternalSourceState::V1 {
        spot_price,
        update_time,
    } = state;
    *spot_price = new_value;
    *update_time = current_time_secs;
}

/// Returns (spot_price, update_time) from the internal source state.
/// RESOURCE_READ: InternalSourceState at self.object_address
pub fn get_internal_source_data(state: &InternalSourceState) -> (u64, u64) {
    let InternalSourceState::V1 {
        spot_price,
        update_time,
    } = state;
    (*spot_price, *update_time)
}
