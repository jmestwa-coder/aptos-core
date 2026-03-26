// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::dead_mans_switch_operations
//
// This is a thin wrapper module that delegates to dead_mans_switch_tracker
// for the perp market context.

use crate::native_perpdex::dead_mans_switch_tracker::{self, DeadMansSwitchTracker};

// ===================== Functions =====================

/// Update keep-alive state for a user on a specific market.
pub fn keep_alive(
    tracker: &mut DeadMansSwitchTracker,
    parent: [u8; 32],
    market: [u8; 32],
    account: [u8; 32],
    timeout_seconds: u64,
    current_time_secs: u64,
) -> Result<(), u64> {
    dead_mans_switch_tracker::keep_alive(
        tracker, parent, market, account, timeout_seconds, current_time_secs,
    )
}

/// Check if an order is still valid based on dead man's switch state.
pub fn is_order_valid(
    tracker: &DeadMansSwitchTracker,
    account: [u8; 32],
    order_creation_time_secs: Option<u64>,
    current_time_secs: u64,
) -> bool {
    dead_mans_switch_tracker::is_order_valid(
        tracker, account, order_creation_time_secs, current_time_secs,
    )
}

/// Set minimum keep-alive time for the tracker.
pub fn set_min_keep_alive_time_secs(
    tracker: &mut DeadMansSwitchTracker,
    parent: [u8; 32],
    market: [u8; 32],
    min_keep_alive_time_secs: u64,
) {
    dead_mans_switch_tracker::set_min_keep_alive_time_secs(
        tracker, parent, market, min_keep_alive_time_secs,
    );
}
