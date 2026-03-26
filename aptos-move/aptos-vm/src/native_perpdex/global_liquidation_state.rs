// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::global_liquidation_state

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===================== Constants =====================

const EINVALID_AUTHORIZED_ACCOUNT: u64 = 1;
const EGLOBAL_LIQUIDATION_STATE_ALREADY_INITIALIZED: u64 = 2;
const ENOT_INITIALIZED: u64 = 3;

// ===================== Types =====================

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackstopLiquidationContinuation {
    V1 {
        continuation_in_progress: bool,
        accumulated_negative_attribution: i64,
    },
}

/// RESOURCE: GlobalLiquidationState at @decibel_dex
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GlobalLiquidationState {
    V1 {
        pending_continuations: BTreeMap<[u8; 32], BackstopLiquidationContinuation>,
    },
}

// ===================== Functions =====================

pub fn default_backstop_liquidation_continuation() -> BackstopLiquidationContinuation {
    BackstopLiquidationContinuation::V1 {
        continuation_in_progress: false,
        accumulated_negative_attribution: 0,
    }
}

pub fn is_continuation_in_progress(cont: &BackstopLiquidationContinuation) -> bool {
    let BackstopLiquidationContinuation::V1 { continuation_in_progress, .. } = cont;
    *continuation_in_progress
}

pub fn get_accumulated_negative_attribution(cont: &BackstopLiquidationContinuation) -> i64 {
    let BackstopLiquidationContinuation::V1 { accumulated_negative_attribution, .. } = cont;
    *accumulated_negative_attribution
}

pub fn update_continuation(
    cont: &mut BackstopLiquidationContinuation,
    new_continuation_in_progress: bool,
    new_accumulated_negative_attribution: i64,
) {
    let BackstopLiquidationContinuation::V1 {
        continuation_in_progress,
        accumulated_negative_attribution,
    } = cont;
    *continuation_in_progress = new_continuation_in_progress;
    *accumulated_negative_attribution = new_accumulated_negative_attribution;
}

pub fn initialize(
    admin_addr: [u8; 32],
    decibel_dex_addr: [u8; 32],
) -> Result<GlobalLiquidationState, u64> {
    if admin_addr != decibel_dex_addr {
        return Err(EINVALID_AUTHORIZED_ACCOUNT);
    }
    Ok(GlobalLiquidationState::V1 {
        pending_continuations: BTreeMap::new(),
    })
}

pub fn has_pending_continuation(state: &GlobalLiquidationState, account: [u8; 32]) -> bool {
    let GlobalLiquidationState::V1 { pending_continuations } = state;
    pending_continuations.contains_key(&account)
}

pub fn set_continuation(
    state: &mut GlobalLiquidationState,
    account: [u8; 32],
    continuation: BackstopLiquidationContinuation,
) {
    let GlobalLiquidationState::V1 { pending_continuations } = state;
    pending_continuations.insert(account, continuation);
}

pub fn remove_continuation(
    state: &mut GlobalLiquidationState,
    account: [u8; 32],
) -> Option<BackstopLiquidationContinuation> {
    let GlobalLiquidationState::V1 { pending_continuations } = state;
    pending_continuations.remove(&account)
}
