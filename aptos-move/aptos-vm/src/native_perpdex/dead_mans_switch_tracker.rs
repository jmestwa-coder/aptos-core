// Copyright (c) Aptos Foundation
// Translated from: aptos_market::dead_mans_switch_tracker

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===================== Constants =====================

const E_KEEP_ALIVE_TIMEOUT_TOO_SHORT: u64 = 0;

// ===================== Types =====================

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeepAliveState {
    pub session_start_time_secs: u64,
    pub expiration_time_secs: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeadMansSwitchTracker {
    pub min_keep_alive_time_secs: u64,
    pub state: BTreeMap<[u8; 32], KeepAliveState>,
}

// ===================== Functions =====================

pub fn new_dead_mans_switch_tracker(
    min_keep_alive_time_secs: u64,
) -> DeadMansSwitchTracker {
    DeadMansSwitchTracker {
        min_keep_alive_time_secs,
        state: BTreeMap::new(),
    }
}

pub fn set_min_keep_alive_time_secs(
    tracker: &mut DeadMansSwitchTracker,
    _parent: [u8; 32],
    _market: [u8; 32],
    min_keep_alive_time_secs: u64,
) {
    // EVENT: MinKeepAliveTimeUpdatedEvent
    tracker.min_keep_alive_time_secs = min_keep_alive_time_secs;
}

pub fn is_order_valid(
    tracker: &DeadMansSwitchTracker,
    account: [u8; 32],
    order_creation_time_secs: Option<u64>,
    current_time_secs: u64,
) -> bool {
    match tracker.state.get(&account) {
        None => true, // No keep-alive set, all orders valid
        Some(state) => {
            let order_time = order_creation_time_secs.unwrap_or(current_time_secs);
            if state.session_start_time_secs > order_time {
                return false; // Order from before session start
            }
            state.expiration_time_secs >= current_time_secs
        }
    }
}

fn disable_keep_alive(
    tracker: &mut DeadMansSwitchTracker,
    _parent: [u8; 32],
    _market: [u8; 32],
    account: [u8; 32],
) {
    let _removed = tracker.state.remove(&account);
    // EVENT: KeepAliveDisabledEvent { was_registered: removed.is_some() }
}

pub fn keep_alive(
    tracker: &mut DeadMansSwitchTracker,
    parent: [u8; 32],
    market: [u8; 32],
    account: [u8; 32],
    timeout_seconds: u64,
    current_time_secs: u64,
) -> Result<(), u64> {
    if timeout_seconds == 0 {
        disable_keep_alive(tracker, parent, market, account);
        return Ok(());
    }
    if timeout_seconds < tracker.min_keep_alive_time_secs {
        return Err(E_KEEP_ALIVE_TIMEOUT_TOO_SHORT);
    }

    let expiration_time = current_time_secs + timeout_seconds;

    match tracker.state.get_mut(&account) {
        Some(state) => {
            if current_time_secs > state.expiration_time_secs {
                // Start new session - invalidates old orders
                state.session_start_time_secs = current_time_secs;
            }
            state.expiration_time_secs = expiration_time;
            // EVENT: KeepAliveUpdateEvent
        }
        None => {
            tracker.state.insert(account, KeepAliveState {
                session_start_time_secs: 0, // All existing orders valid
                expiration_time_secs: expiration_time,
            });
            // EVENT: KeepAliveUpdateEvent
        }
    }
    Ok(())
}
