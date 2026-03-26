// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::volume_tracker

use crate::native_perpdex::decibel_time;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===================== Constants =====================

const NUM_HISTORICAL_DAYS_TO_TRACK: u64 = 30;
const SECONDS_IN_DAY: u64 = 86400;
const EINTERNAL_INVARIANT: u64 = 3;

// ===================== Types =====================

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum VolumeType {
    Global,
    Maker([u8; 32]),
    Taker([u8; 32]),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum DayVolume {
    V1 {
        day_since_epoch: u64,
        volume: u128,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VolumeHistory {
    V1 {
        latest_day_since_epoch: u64,
        latest_day_volume: u128, // In native: plain u128 instead of Aggregator<u128>
        history: Vec<DayVolume>,
        total_volume_in_window: u128,
        total_volume_all_time: u128, // In native: plain u128 instead of Aggregator<u128>
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VolumeStats {
    V1 {
        global_history: VolumeHistory,
        user_taker_volume_history: BTreeMap<[u8; 32], VolumeHistory>,
        user_maker_volume_history: BTreeMap<[u8; 32], VolumeHistory>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum VolumeHistoryView {
    V1 {
        latest_day_since_epoch: u64,
        latest_day_volume: u128,
        history: Vec<DayVolume>,
        total_volume_in_window: u128,
        total_volume_all_time: u128,
    },
}

// ===================== Functions =====================

pub fn initialize(now_seconds: u64) -> VolumeStats {
    VolumeStats::V1 {
        global_history: VolumeHistory::V1 {
            latest_day_since_epoch: now_seconds / SECONDS_IN_DAY,
            latest_day_volume: 0,
            history: Vec::new(),
            total_volume_in_window: 0,
            total_volume_all_time: 0,
        },
        user_taker_volume_history: BTreeMap::new(),
        user_maker_volume_history: BTreeMap::new(),
    }
}

pub fn get_global_volume_in_window(
    volume_stats: &mut VolumeStats,
    now_seconds: u64,
) -> u128 {
    let VolumeStats::V1 { global_history, .. } = volume_stats;
    update_volume_history(global_history, 0, now_seconds);
    let VolumeHistory::V1 { total_volume_in_window, .. } = global_history;
    *total_volume_in_window
}

pub fn get_maker_volume_in_window(
    volume_stats: &mut VolumeStats,
    user_addr: [u8; 32],
    now_seconds: u64,
) -> u128 {
    let VolumeStats::V1 { user_maker_volume_history, .. } = volume_stats;
    match user_maker_volume_history.get_mut(&user_addr) {
        None => 0,
        Some(volume_history) => {
            update_volume_history(volume_history, 0, now_seconds);
            let VolumeHistory::V1 { total_volume_in_window, .. } = volume_history;
            *total_volume_in_window
        }
    }
}

pub fn get_taker_volume_in_window(
    volume_stats: &mut VolumeStats,
    user_addr: [u8; 32],
    now_seconds: u64,
) -> u128 {
    let VolumeStats::V1 { user_taker_volume_history, .. } = volume_stats;
    match user_taker_volume_history.get_mut(&user_addr) {
        None => 0,
        Some(volume_history) => {
            update_volume_history(volume_history, 0, now_seconds);
            let VolumeHistory::V1 { total_volume_in_window, .. } = volume_history;
            *total_volume_in_window
        }
    }
}

pub fn get_total_volume_in_window(
    volume_stats: &mut VolumeStats,
    user_addr: [u8; 32],
    now_seconds: u64,
) -> u128 {
    let mut total_volume = 0u128;
    let VolumeStats::V1 {
        user_maker_volume_history,
        user_taker_volume_history,
        ..
    } = volume_stats;

    if let Some(maker_history) = user_maker_volume_history.get_mut(&user_addr) {
        update_volume_history(maker_history, 0, now_seconds);
        let VolumeHistory::V1 { total_volume_in_window, .. } = maker_history;
        total_volume += *total_volume_in_window;
    }
    if let Some(taker_history) = user_taker_volume_history.get_mut(&user_addr) {
        update_volume_history(taker_history, 0, now_seconds);
        let VolumeHistory::V1 { total_volume_in_window, .. } = taker_history;
        total_volume += *total_volume_in_window;
    }
    total_volume
}

pub fn get_maker_volume_all_time(
    volume_stats: &mut VolumeStats,
    user_addr: [u8; 32],
) -> u128 {
    let VolumeStats::V1 { user_maker_volume_history, .. } = volume_stats;
    match user_maker_volume_history.get(&user_addr) {
        None => 0,
        Some(VolumeHistory::V1 { total_volume_all_time, .. }) => *total_volume_all_time,
    }
}

pub fn get_taker_volume_all_time(
    volume_stats: &mut VolumeStats,
    user_addr: [u8; 32],
) -> u128 {
    let VolumeStats::V1 { user_taker_volume_history, .. } = volume_stats;
    match user_taker_volume_history.get(&user_addr) {
        None => 0,
        Some(VolumeHistory::V1 { total_volume_all_time, .. }) => *total_volume_all_time,
    }
}

fn get_or_init_user_volume(
    user_history: &mut BTreeMap<[u8; 32], VolumeHistory>,
    user_addr: [u8; 32],
    now_seconds: u64,
) -> &mut VolumeHistory {
    user_history.entry(user_addr).or_insert_with(|| {
        VolumeHistory::V1 {
            latest_day_since_epoch: now_seconds / SECONDS_IN_DAY,
            latest_day_volume: 0,
            history: Vec::new(),
            total_volume_in_window: 0,
            total_volume_all_time: 0,
        }
    })
}

pub fn track_maker_and_global_volume(
    stats: &mut VolumeStats,
    maker_addr: [u8; 32],
    volume: u128,
    now_seconds: u64,
) {
    let VolumeStats::V1 {
        global_history,
        user_maker_volume_history,
        ..
    } = stats;
    update_volume_history(global_history, volume, now_seconds);
    let maker_history = get_or_init_user_volume(user_maker_volume_history, maker_addr, now_seconds);
    update_volume_history(maker_history, volume, now_seconds);
}

pub fn track_taker_volume(
    stats: &mut VolumeStats,
    taker_addr: [u8; 32],
    volume: u128,
    now_seconds: u64,
) {
    let VolumeStats::V1 { user_taker_volume_history, .. } = stats;
    let taker_history = get_or_init_user_volume(user_taker_volume_history, taker_addr, now_seconds);
    update_volume_history(taker_history, volume, now_seconds);
}

pub fn track_volume(
    stats: &mut VolumeStats,
    maker_addr: [u8; 32],
    taker_addr: [u8; 32],
    volume: u128,
    now_seconds: u64,
) {
    let VolumeStats::V1 {
        global_history,
        user_taker_volume_history,
        user_maker_volume_history,
    } = stats;
    update_volume_history(global_history, volume, now_seconds);

    let taker_history = get_or_init_user_volume(user_taker_volume_history, taker_addr, now_seconds);
    update_volume_history(taker_history, volume, now_seconds);

    let maker_history = get_or_init_user_volume(user_maker_volume_history, maker_addr, now_seconds);
    update_volume_history(maker_history, volume, now_seconds);
}

fn update_volume_history(
    history: &mut VolumeHistory,
    volume: u128,
    now_seconds: u64,
) {
    let VolumeHistory::V1 {
        latest_day_since_epoch,
        latest_day_volume,
        total_volume_all_time,
        ..
    } = history;

    let current_day = now_seconds / SECONDS_IN_DAY;
    if current_day != *latest_day_since_epoch {
        rollover_volume_history(history, now_seconds);
        // Reset latest day volume
        let VolumeHistory::V1 { latest_day_volume, latest_day_since_epoch, .. } = history;
        *latest_day_volume = 0;
        *latest_day_since_epoch = current_day;
    }
    if volume > 0 {
        let VolumeHistory::V1 { latest_day_volume, total_volume_all_time, .. } = history;
        *latest_day_volume += volume;
        *total_volume_all_time += volume;
    }
    // EVENT: VolumeHistoryUpdateEvent (on rollover)
}

fn rollover_volume_history(history: &mut VolumeHistory, now_seconds: u64) {
    let current_day = now_seconds / SECONDS_IN_DAY;
    let VolumeHistory::V1 {
        latest_day_since_epoch,
        latest_day_volume,
        history: history_vec,
        total_volume_in_window,
        ..
    } = history;

    history_vec.push(DayVolume::V1 {
        day_since_epoch: *latest_day_since_epoch,
        volume: *latest_day_volume,
    });

    *total_volume_in_window += *latest_day_volume;

    // Remove oldest entries outside the tracking window
    let mut i = 0;
    while i < history_vec.len() {
        let DayVolume::V1 { day_since_epoch, volume } = history_vec[i];
        if day_since_epoch < current_day.saturating_sub(NUM_HISTORICAL_DAYS_TO_TRACK) {
            *total_volume_in_window -= volume;
            history_vec.swap_remove(i);
        } else {
            i += 1;
        }
    }
}

// ============ View APIs ============

pub fn volume_history_to_view(history: &VolumeHistory) -> VolumeHistoryView {
    let VolumeHistory::V1 {
        latest_day_since_epoch,
        latest_day_volume,
        history,
        total_volume_in_window,
        total_volume_all_time,
    } = history;
    VolumeHistoryView::V1 {
        latest_day_since_epoch: *latest_day_since_epoch,
        latest_day_volume: *latest_day_volume,
        history: history.clone(),
        total_volume_in_window: *total_volume_in_window,
        total_volume_all_time: *total_volume_all_time,
    }
}

pub fn get_global_volume_history_view(volume_stats: &VolumeStats) -> VolumeHistoryView {
    let VolumeStats::V1 { global_history, .. } = volume_stats;
    volume_history_to_view(global_history)
}

pub fn get_maker_volume_history_view(
    volume_stats: &VolumeStats,
    user_addr: [u8; 32],
) -> Option<VolumeHistoryView> {
    let VolumeStats::V1 { user_maker_volume_history, .. } = volume_stats;
    user_maker_volume_history
        .get(&user_addr)
        .map(volume_history_to_view)
}

pub fn get_taker_volume_history_view(
    volume_stats: &VolumeStats,
    user_addr: [u8; 32],
) -> Option<VolumeHistoryView> {
    let VolumeStats::V1 { user_taker_volume_history, .. } = volume_stats;
    user_taker_volume_history
        .get(&user_addr)
        .map(volume_history_to_view)
}
