// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::decibel_time
//
// In the native Rust context, we do not have access to the Move global storage
// (TimeOverride resource) or the framework timestamp module. This module provides
// the same interface but requires callers to supply the current timestamp.

// ===================== Constants =====================

const EINVALID_AUTHORIZED_ACCOUNT: u64 = 0;

/// Conversion factor between seconds and microseconds
const MICRO_CONVERSION_FACTOR: u64 = 1_000_000;

// ===================== TimeOverride =====================
// In Move this is an on-chain resource. In Rust we model it as a struct.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TimeOverride {
    Offset {
        time_offset_microseconds: u64,
    },
}

/// State holder for decibel_time, containing the override and the current chain timestamp.
#[derive(Clone, Debug)]
pub struct DecibelTimeState {
    pub time_override: Option<TimeOverride>,
    /// The current chain timestamp in microseconds (from aptos_framework::timestamp)
    pub chain_now_microseconds: u64,
}

impl DecibelTimeState {
    pub fn new(chain_now_microseconds: u64, time_override: Option<TimeOverride>) -> Self {
        Self {
            time_override,
            chain_now_microseconds,
        }
    }
}

// ===================== Functions =====================

pub fn increment_time(
    state: &mut DecibelTimeState,
    account_address: &[u8; 32],
    deployer_address: &[u8; 32],
    increment_microseconds: u64,
) -> Result<(), u64> {
    if account_address != deployer_address {
        return Err(EINVALID_AUTHORIZED_ACCOUNT);
    }
    match &mut state.time_override {
        Some(TimeOverride::Offset {
            time_offset_microseconds,
        }) => {
            *time_offset_microseconds += increment_microseconds;
            Ok(())
        },
        None => {
            // No TimeOverride exists -- in Move this would abort because the resource doesn't exist.
            // We create one for flexibility.
            state.time_override = Some(TimeOverride::Offset {
                time_offset_microseconds: increment_microseconds,
            });
            Ok(())
        },
    }
}

pub fn now_microseconds(state: &DecibelTimeState) -> u64 {
    match &state.time_override {
        Some(TimeOverride::Offset {
            time_offset_microseconds,
        }) => state.chain_now_microseconds + time_offset_microseconds,
        None => state.chain_now_microseconds,
    }
}

pub fn now_seconds(state: &DecibelTimeState) -> u64 {
    now_microseconds(state) / MICRO_CONVERSION_FACTOR
}

pub fn init_module(
    deployer_address: &[u8; 32],
    expected_address: &[u8; 32],
) -> Result<TimeOverride, u64> {
    if deployer_address != expected_address {
        return Err(EINVALID_AUTHORIZED_ACCOUNT);
    }
    Ok(TimeOverride::Offset {
        time_offset_microseconds: 0,
    })
}
