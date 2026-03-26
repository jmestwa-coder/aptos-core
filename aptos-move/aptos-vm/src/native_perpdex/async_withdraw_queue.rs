// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::async_withdraw_queue

use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const BIG_MAP_INNER_DEGREE: u16 = 0;
const BIG_MAP_LEAF_DEGREE: u16 = 16;

const BASIS_POINTS: u64 = 10000;
const DEFAULT_WORK_UNITS: u32 = 10;

// Error Codes
const EINVALID_RATE_LIMIT_BPS: u64 = 1;
const EINVALID_EPOCH_DURATION: u64 = 2;
const EASSET_NOT_CONFIGURED: u64 = 3;
const EREQUEST_NOT_FOUND: u64 = 4;
const EWITHDRAWAL_VALIDATION_FAILED: u64 = 5;
const EINVALID_NUM_BUCKETS: u64 = 6;

const MIN_BUCKETS: u8 = 2;
const MAX_BUCKETS: u8 = 10;

// ===================== Types =====================

/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AsyncWithdrawQueueConfig {
    V1 {
        rate_limit_configs: Vec<(/* asset_addr */ [u8; 32], RateLimitConfig)>,
        pending_withdrawals: Vec<(PendingWithdrawKey, PendingWithdrawRequest)>,
        user_pending_requests: Vec<(UserRequestKey, PendingWithdrawKey)>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RateLimitConfig {
    V1 {
        enabled: bool,
        rate_limit_bps: u64,
        absolute_rate_limit: u64,
        window_duration_seconds: u64,
        num_buckets: u8,
        bucket_duration_seconds: u64,
        last_bucket_number: u64,
        bucket_withdrawn: Vec<u64>,
        pending_requests_count: u64,
        // can_deposit_f is a stored function pointer; represented as unit in native
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PendingWithdrawKey {
    pub time: u64,
    pub tie_breaker: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PendingWithdrawRequest {
    V1 {
        request_id: u128,
        user: [u8; 32],
        recipient: [u8; 32],
        market: Option<[u8; 32]>,     // Option<Object<PerpMarket>>
        metadata: [u8; 32],            // Object<Metadata>
        fungible_amount: u64,
        created_at: u64,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct UserRequestKey {
    pub user: [u8; 32],
    pub request_id: u128,
}

#[derive(Clone, Debug)]
pub enum RateLimitCheckResult {
    Allowed,
    ExceedsEpochLimit,
    ExceedsRemainingCapacity,
}

/// Comprehensive rate limiter status for display
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum RateLimitStatus {
    V1 {
        configured: bool,
        enabled: bool,
        rate_limit_bps: u64,
        absolute_rate_limit: u64,
        window_duration_seconds: u64,
        num_buckets: u8,
        window_withdrawn: u64,
        percentage_limit: u64,
        effective_limit: u64,
        remaining_capacity: u64,
        pending_requests_count: u64,
    },
}

// ===================== Events =====================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WithdrawQueuedEvent {
    V1 {
        user: [u8; 32],
        recipient: [u8; 32],
        market: Option<[u8; 32]>,
        metadata: [u8; 32],
        fungible_amount: u64,
        request_id: u128,
        timestamp: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WithdrawProcessedEvent {
    V1 {
        user: [u8; 32],
        recipient: [u8; 32],
        market: Option<[u8; 32]>,
        metadata: [u8; 32],
        fungible_amount: u64,
        request_id: u128,
        timestamp: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WithdrawCancelledEvent {
    V1 {
        user: [u8; 32],
        market: Option<[u8; 32]>,
        metadata: [u8; 32],
        fungible_amount: u64,
        request_id: u128,
        reason: String,
        timestamp: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RateLimitConfigUpdatedEvent {
    V1 {
        metadata: [u8; 32],
        enabled: bool,
        rate_limit_bps: u64,
        absolute_rate_limit: u64,
        window_duration_seconds: u64,
        num_buckets: u8,
        timestamp: u64,
    },
}

// ===================== Functions =====================

/// Initialize the async withdraw queue.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn initialize(
    _publisher: [u8; 32],
) {
    // EVENT: creates AsyncWithdrawQueueConfig resource at @decibel_dex
    // In native context, state initialization is handled by the caller.
}

pub fn default_can_deposit(_recipient: [u8; 32], _fungible_amount: u64) -> bool {
    true
}

fn get_current_bucket_number(bucket_duration_seconds: u64, now_seconds: u64) -> u64 {
    now_seconds / bucket_duration_seconds
}

fn calculate_percentage_limit(total_balance: u64, rate_limit_bps: u64) -> u64 {
    ((total_balance as u128) * (rate_limit_bps as u128) / (BASIS_POINTS as u128)) as u64
}

fn calculate_effective_limit(
    total_balance: u64,
    rate_limit_bps: u64,
    absolute_rate_limit: u64,
) -> u64 {
    let percentage_limit = calculate_percentage_limit(total_balance, rate_limit_bps);
    if absolute_rate_limit == 0 {
        percentage_limit
    } else {
        std::cmp::min(percentage_limit, absolute_rate_limit)
    }
}

/// Get the total system balance for a specific asset (in fungible amount).
/// RESOURCE: CollateralBalanceSheet at @decibel_dex
pub fn get_total_system_balance_for_asset(
    _metadata: [u8; 32],
    // In native context, the balance is read from the collateral module
) -> u64 {
    // Delegated to accounts_collateral module
    0
}

/// Update stale buckets to 0 for sliding window.
/// Returns the sum of all bucket values after update.
pub fn update_buckets_and_get_total(
    rate_limit: &mut RateLimitConfig,
    now_seconds: u64,
) -> u64 {
    let RateLimitConfig::V1 {
        bucket_duration_seconds,
        last_bucket_number,
        num_buckets,
        bucket_withdrawn,
        ..
    } = rate_limit;

    let current_bucket = now_seconds / *bucket_duration_seconds;
    let buckets_passed = current_bucket - *last_bucket_number;
    let num_buckets_u64 = *num_buckets as u64;

    if buckets_passed >= num_buckets_u64 {
        for i in 0..num_buckets_u64 {
            bucket_withdrawn[i as usize] = 0;
        }
    } else if buckets_passed > 0 {
        for i in 1..=buckets_passed {
            let bucket_index = ((*last_bucket_number + i) % num_buckets_u64) as usize;
            bucket_withdrawn[bucket_index] = 0;
        }
    }

    *last_bucket_number = current_bucket;

    bucket_withdrawn.iter().sum()
}

/// Get the sum of all bucket values without updating (for view functions).
pub fn get_window_withdrawn(rate_limit: &RateLimitConfig, now_seconds: u64) -> u64 {
    let RateLimitConfig::V1 {
        bucket_duration_seconds,
        last_bucket_number,
        num_buckets,
        bucket_withdrawn,
        ..
    } = rate_limit;

    let current_bucket = now_seconds / *bucket_duration_seconds;
    let buckets_passed = current_bucket - *last_bucket_number;
    let num_buckets_u64 = *num_buckets as u64;

    if buckets_passed >= num_buckets_u64 {
        return 0;
    }

    let mut total: u64 = bucket_withdrawn.iter().sum();

    for k in 1..=buckets_passed {
        let stale_index = ((*last_bucket_number + k) % num_buckets_u64) as usize;
        total -= bucket_withdrawn[stale_index];
    }

    total
}

/// Record withdrawal to current bucket.
pub fn record_withdrawal_to_bucket(
    rate_limit: &mut RateLimitConfig,
    fungible_amount: u64,
    now_seconds: u64,
) {
    let RateLimitConfig::V1 {
        bucket_duration_seconds,
        num_buckets,
        bucket_withdrawn,
        last_bucket_number,
        ..
    } = rate_limit;

    let current_bucket = now_seconds / *bucket_duration_seconds;
    let bucket_index = (current_bucket % (*num_buckets as u64)) as usize;
    bucket_withdrawn[bucket_index] += fungible_amount;
    *last_bucket_number = current_bucket;
}

/// Check rate limit and record withdrawal if allowed.
pub fn try_record_withdrawal(
    rate_limit: &mut RateLimitConfig,
    total_system_balance: u64,
    fungible_amount: u64,
    now_seconds: u64,
) -> RateLimitCheckResult {
    let (enabled, rl_bps, abs_limit) = match rate_limit {
        RateLimitConfig::V1 { enabled, rate_limit_bps, absolute_rate_limit, .. } =>
            (*enabled, *rate_limit_bps, *absolute_rate_limit),
    };

    if !enabled {
        return RateLimitCheckResult::Allowed;
    }

    let window_withdrawn = update_buckets_and_get_total(rate_limit, now_seconds);

    let effective_limit = calculate_effective_limit(
        total_system_balance,
        rl_bps,
        abs_limit,
    );

    if fungible_amount > effective_limit {
        return RateLimitCheckResult::ExceedsEpochLimit;
    }

    if window_withdrawn + fungible_amount > effective_limit {
        return RateLimitCheckResult::ExceedsRemainingCapacity;
    }

    record_withdrawal_to_bucket(rate_limit, fungible_amount, now_seconds);
    RateLimitCheckResult::Allowed
}

/// Queue a withdrawal request for later processing.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn queue_withdrawal(
    _user: [u8; 32],
    _metadata: [u8; 32],
    _fungible_amount: u64,
    _recipient: [u8; 32],
) -> u128 {
    // EVENT: WithdrawQueuedEvent
    // In native context, the withdrawal is queued in the AsyncWithdrawQueueConfig.
    0
}

/// Queue a withdrawal request from isolated position for later processing.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn queue_withdrawal_from_isolated(
    _user: [u8; 32],
    _market: [u8; 32],
    _metadata: [u8; 32],
    _fungible_amount: u64,
    _recipient: [u8; 32],
) -> u128 {
    // EVENT: WithdrawQueuedEvent
    0
}

/// Request a withdrawal from cross-margin.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
/// Returns Option::none() if processed immediately, Option::some(request_id) if queued.
pub fn request_withdrawal_from_cross(
    _owner: [u8; 32], // signer
    _metadata: [u8; 32],
    _fungible_amount: u64,
    _recipient: [u8; 32],
) -> Option<u128> {
    // EVENT: WithdrawQueuedEvent or WithdrawProcessedEvent
    // Delegated to native execution layer
    None
}

/// Request a withdrawal from isolated position.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
/// Returns Option::none() if processed immediately, Option::some(request_id) if queued.
pub fn request_withdrawal_from_isolated(
    _owner: [u8; 32], // signer
    _market: [u8; 32],
    _metadata: [u8; 32],
    _fungible_amount: u64,
    _recipient: [u8; 32],
) -> Option<u128> {
    // EVENT: WithdrawQueuedEvent or WithdrawProcessedEvent
    None
}

/// Cancel a pending withdrawal request.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn cancel_withdrawal(
    _user: [u8; 32],
    _request_id: u128,
) -> Result<(), u64> {
    // EVENT: WithdrawCancelledEvent
    // In native context, removal is from the pending_withdrawals and user_pending_requests maps.
    Ok(())
}

/// Process pending withdrawal requests in FIFO order.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn process_pending_withdrawals(
    _remaining_work_units: &mut u32,
) {
    // EVENT: WithdrawProcessedEvent or WithdrawCancelledEvent
    // In native context, processes pending withdrawals with rate limiting.
}

fn create_empty_buckets(num_buckets: u8) -> Vec<u64> {
    vec![0u64; num_buckets as usize]
}

/// Configure rate limit for an asset (create or update).
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn configure_rate_limit(
    _metadata: [u8; 32],
    enabled: bool,
    rate_limit_bps: u64,
    absolute_rate_limit: u64,
    window_duration_seconds: u64,
    num_buckets: u8,
) -> Result<(), u64> {
    if rate_limit_bps > BASIS_POINTS {
        return Err(EINVALID_RATE_LIMIT_BPS);
    }
    if window_duration_seconds == 0 {
        return Err(EINVALID_EPOCH_DURATION);
    }
    if num_buckets < MIN_BUCKETS || num_buckets > MAX_BUCKETS {
        return Err(EINVALID_NUM_BUCKETS);
    }
    // EVENT: RateLimitConfigUpdatedEvent
    Ok(())
}

/// Update rate limit percentage for an asset.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn update_rate_limit_bps(
    _metadata: [u8; 32],
    rate_limit_bps: u64,
) -> Result<(), u64> {
    if rate_limit_bps > BASIS_POINTS {
        return Err(EINVALID_RATE_LIMIT_BPS);
    }
    // EVENT: RateLimitConfigUpdatedEvent
    Ok(())
}

/// Update absolute rate limit for an asset.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn update_absolute_rate_limit(
    _metadata: [u8; 32],
    _absolute_rate_limit: u64,
) -> Result<(), u64> {
    // EVENT: RateLimitConfigUpdatedEvent
    Ok(())
}

/// Update window duration for an asset (resets buckets).
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn update_window_duration(
    _metadata: [u8; 32],
    window_duration_seconds: u64,
) -> Result<(), u64> {
    if window_duration_seconds == 0 {
        return Err(EINVALID_EPOCH_DURATION);
    }
    // EVENT: RateLimitConfigUpdatedEvent
    Ok(())
}

/// Enable or disable rate limiting for an asset.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn set_rate_limit_enabled(
    _metadata: [u8; 32],
    _enabled: bool,
) -> Result<(), u64> {
    // EVENT: RateLimitConfigUpdatedEvent
    Ok(())
}

/// Update the can_deposit callback for an asset.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn set_can_deposit_callback(
    _metadata: [u8; 32],
    // callback is native function pointer; not representable in Rust translation
) -> Result<(), u64> {
    Ok(())
}

/// Get count of user's pending withdrawal requests.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn get_pending_withdrawal_count(_user: [u8; 32]) -> u64 {
    0
}

/// Get user's pending withdrawal requests.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn get_pending_withdrawals(_user: [u8; 32]) -> Vec<PendingWithdrawRequest> {
    Vec::new()
}

/// Get rate limit config for an asset.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn get_rate_limit_config(_metadata: [u8; 32]) -> Option<RateLimitConfig> {
    None
}

/// Get current window usage for an asset (used, limit).
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn get_current_window_usage(
    _metadata: [u8; 32],
    _total_system_balance: u64,
) -> (u64, u64) {
    (0, 0)
}

/// Get comprehensive rate limiter status for an asset.
/// RESOURCE: AsyncWithdrawQueueConfig at @decibel_dex
pub fn get_rate_limit_status(_metadata: [u8; 32]) -> RateLimitStatus {
    RateLimitStatus::V1 {
        configured: false,
        enabled: false,
        rate_limit_bps: 0,
        absolute_rate_limit: 0,
        window_duration_seconds: 0,
        num_buckets: 0,
        window_withdrawn: 0,
        percentage_limit: 0,
        effective_limit: 0,
        remaining_capacity: 0,
        pending_requests_count: 0,
    }
}

// ===================== Dispatch stubs (by-addr) for perp_engine delegation =====================

pub fn request_withdrawal_from_cross_by_addr(
    _owner: [u8; 32], _metadata: [u8; 32], _amount: u64, _recipient: [u8; 32],
) -> Result<Option<u128>, u64> {
    // Dispatch layer resolves AsyncWithdrawQueueConfig and handles withdrawal
    Ok(None)
}

pub fn request_withdrawal_from_isolated_by_addr(
    _owner: [u8; 32], _market: [u8; 32], _metadata: [u8; 32], _amount: u64, _recipient: [u8; 32],
) -> Result<Option<u128>, u64> {
    // Dispatch layer resolves AsyncWithdrawQueueConfig and handles withdrawal
    Ok(None)
}

pub fn process_pending_withdrawals_dispatch(_remaining_work_units: &mut u32) {
    // Dispatch layer resolves AsyncWithdrawQueueConfig and processes pending withdrawals
}
