// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::adl_tracker

use crate::native_perpdex::i64_math;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===================== Types =====================

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ADLKey {
    pub entry_px: u64,
    pub timestamp: i64,
    pub account: [u8; 32],
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ADLValue {
    pub leverage: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LeverageBuckets {
    V1 {
        buckets: Vec<BTreeMap<ADLKey, ADLValue>>,
        cutoffs: Vec<u8>,
    },
}

/// RESOURCE: ADLTracker at market object address
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ADLTracker {
    V1 {
        long_positions: LeverageBuckets,
        short_positions: LeverageBuckets,
    },
}

// ===================== Functions =====================

fn new_leverage_buckets_with_cutoffs(cutoffs: Vec<u8>) -> LeverageBuckets {
    let n = cutoffs.len();
    let mut buckets = Vec::with_capacity(n + 1);
    for _ in 0..=(n) {
        buckets.push(BTreeMap::new());
    }
    LeverageBuckets::V1 { buckets, cutoffs }
}

fn get_bucket_index(lb: &LeverageBuckets, leverage: u8) -> usize {
    let LeverageBuckets::V1 { cutoffs, .. } = lb;
    for (i, &cutoff) in cutoffs.iter().enumerate() {
        if leverage <= cutoff {
            return i;
        }
    }
    cutoffs.len()
}

pub fn initialize_tracker() -> ADLTracker {
    let cutoffs = vec![1u8, 2, 4, 8, 16, 32, 64];
    ADLTracker::V1 {
        long_positions: new_leverage_buckets_with_cutoffs(cutoffs.clone()),
        short_positions: new_leverage_buckets_with_cutoffs(cutoffs),
    }
}

pub fn remove_position(
    tracker: &mut ADLTracker,
    account: [u8; 32],
    is_long: bool,
    entry_px: u64,
    timestamp: i64,
    leverage: u8,
) {
    let ADLTracker::V1 { long_positions, short_positions } = tracker;
    let key = ADLKey { entry_px, timestamp, account };
    let buckets = if is_long { long_positions } else { short_positions };
    let bucket_index = get_bucket_index(buckets, leverage);
    let LeverageBuckets::V1 { buckets: b, .. } = buckets;
    b[bucket_index].remove(&key);
}

pub fn add_position(
    tracker: &mut ADLTracker,
    account: [u8; 32],
    is_long: bool,
    entry_px: u64,
    timestamp: i64,
    leverage: u8,
) {
    let ADLTracker::V1 { long_positions, short_positions } = tracker;
    let key = ADLKey { entry_px, timestamp, account };
    let value = ADLValue { leverage };
    let buckets = if is_long { long_positions } else { short_positions };
    let bucket_index = get_bucket_index(buckets, leverage);
    let LeverageBuckets::V1 { buckets: b, .. } = buckets;
    b[bucket_index].insert(key, value);
}

pub fn get_next_adl_address(
    tracker: &ADLTracker,
    is_long: bool,
    mark_price: u64,
) -> [u8; 32] {
    let ADLTracker::V1 { long_positions, short_positions } = tracker;
    let buckets = if is_long { long_positions } else { short_positions };
    let LeverageBuckets::V1 { buckets: b, .. } = buckets;

    let mut best_score: i64 = i64::MIN;
    let mut best_account: [u8; 32] = [0u8; 32];
    let mut found_any = false;

    let scale_factor: u64 = 1_000_000;

    for bucket in b.iter() {
        if bucket.is_empty() {
            continue;
        }

        let (key, value) = if is_long {
            bucket.iter().next().unwrap()
        } else {
            bucket.iter().next_back().unwrap()
        };

        let unit_profit = if is_long {
            (mark_price as i64) - (key.entry_px as i64)
        } else {
            (key.entry_px as i64) - (mark_price as i64)
        };

        let score = i64_math::mul_div(
            unit_profit,
            (value.leverage as u64) * scale_factor,
            key.entry_px,
        ).unwrap_or(0
        );

        if !found_any || score >= best_score {
            best_score = score;
            best_account = key.account;
            found_any = true;
        }
    }

    best_account
}

pub fn view_adl_cutoffs(tracker: &ADLTracker) -> Vec<u8> {
    let ADLTracker::V1 { long_positions, .. } = tracker;
    let LeverageBuckets::V1 { cutoffs, .. } = long_positions;
    cutoffs.clone()
}

pub fn view_adl_bucket_sizes(tracker: &ADLTracker) -> (Vec<u64>, Vec<u64>) {
    let ADLTracker::V1 { long_positions, short_positions } = tracker;
    let LeverageBuckets::V1 { buckets: long_b, .. } = long_positions;
    let LeverageBuckets::V1 { buckets: short_b, .. } = short_positions;

    let long_sizes: Vec<u64> = long_b.iter().map(|b| b.len() as u64).collect();
    let short_sizes: Vec<u64> = short_b.iter().map(|b| b.len() as u64).collect();
    (long_sizes, short_sizes)
}
