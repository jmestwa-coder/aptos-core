// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::async_matching_engine
//
// This is the heart of the DEX - the async matching engine that manages the
// pending request queue, processes orders, liquidations, ADL, and mark price commits.

use crate::native_perpdex::builder_code_registry::BuilderCode;
use crate::native_perpdex::perp_engine_types::{
    OrderMetadata, new_order_metadata, new_twap_metadata,

};
use crate::native_perpdex::liquidation::MarginCallContinuation;
use crate::native_perpdex::market_types::OrderCancellationReason;
use crate::native_perpdex::order_book_types::OrderId;
use crate::native_perpdex::order_placement;
use crate::native_perpdex::order_placement_utils;
use crate::native_perpdex::perp_order::{self, PerpOrderRequestExtendedArgs};
use crate::native_perpdex::tp_sl_utils::ChildTpSlOrder;
use crate::native_perpdex::work_unit_utils::{self, WorkUnit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===================== Constants =====================

const DEFAULT_PENDING_ORDERS_TO_TRIGGER: u32 = 10;
const DEFAULT_PENDING_TWAPS_TO_TRIGGER: u32 = 10;

const EMARKET_ALREADY_REGISTERED: u64 = 1;
const EINVALID_TP_SL_FOR_REDUCE_ONLY: u64 = 2;
const EINVALID_TP_SL_WITH_TRIGGER_CONDITION: u64 = 3;
const EINVALID_STOP_PRICE: u64 = 4;
const EINVALD_WORK_UNITS_PER_TRIGGER: u64 = 5;
const EINVALID_TWAP_DURATION: u64 = 6;
const EINVALID_TWAP_FREQUENCY: u64 = 7;
const ETWAP_DURATION_NOT_MULTIPLE_OF_FREQUENCY: u64 = 8;
const EINDIVIDUAL_TWAP_INSTANCE_SMALLER_THAN_MIN_SIZE: u64 = 9;
const ECANNOT_LIQUIDATE_BACKSTOP_LIQUIDATOR: u64 = 10;
const EMAKER_SHOULD_HAVE_NO_MATCHES: u64 = 11;
const EDEPRECATED_METHOD: u64 = 12;
const ECOMMIT_MARK_PRICE_QUEUE_MISMATCH: u64 = 13;
const ECANNOT_CONTINUE_ISOLATED_BACKSTOP_LIQUIDATION: u64 = 14;

const SLIPPAGE_TOLERANCE_FOR_TWAP: u64 = 300; // 3%

const MIN_TWAP_DURATION_S: u64 = 120; // 2 minutes
const MAX_TWAP_DURATION_S: u64 = 86400; // 24 hours
const MIN_TWAP_FREQUENCY_S: u64 = 60; // 1 minute

const MAX_QUEUE_LIQUIDATION_BATCH_SIZE: u64 = 32;

const HI_PRICE: u64 = i64::MAX as u64;

const LIQUIDATION_PRIORITY: u8 = 0;
const MARGIN_CALL_PRIORITY: u8 = 1;
const REGULAR_ORDER_PRIORITY: u8 = 2;

// ===================== Types =====================


#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum PendingRequestKey {
    V1 {
        time: u64,
        priority: u8,
        tie_breaker: u128,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PendingOrder {
    V1 {
        order_args: PerpOrderRequestExtendedArgs,
        order_metadata: OrderMetadata,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContinuedPendingOrder {
    V1 {
        order_args: PerpOrderRequestExtendedArgs,
        order_metadata: OrderMetadata,
        remaining_size: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PendingTwap {
    V1 {
        account: [u8; 32],
        order_id: OrderId,
        is_buy: bool,
        orig_size: u64,
        instance_remaining_size: Option<u64>,
        remaining_size: u64,
        is_reduce_only: bool,
        twap_start_time_s: u64,
        twap_frequency_s: u64,
        twap_end_time_s: u64,
        builder_code: Option<BuilderCode>,
        client_order_id: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum QueueBackstopLiquidationsAndADLPayload {
    V1 {
        backstop_liquidations: Vec<[u8; 32]>,
        backstop_liquidation_keys: Vec<PendingRequestKey>,
        check_adl_key: Option<PendingRequestKey>,
        any_backstop_liquidation_added: bool,
        needs_adl_check: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum QueueMarginCallLiquidationsPayload {
    V1 {
        margin_call_liquidations: Vec<[u8; 32]>,
        margin_call_liquidation_keys: Vec<PendingRequestKey>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PendingRequest {
    Order(PendingOrder),
    Twap(PendingTwap),
    ContinuedOrder(ContinuedPendingOrder),
    BackstopLiquidation {
        user: [u8; 32],
        batch_key: u128,
    },
    MarginCall {
        user: [u8; 32],
        continuation: MarginCallContinuation,
        batch_key: u128,
    },
    CheckADL {
        batch_key: u128,
    },
    TriggerADL {
        adl_price: u64,
        batch_key: u128,
    },
    CommitMarkPrice {
        mark_px: u64,
        batch_key: u128,
    },
    QueueBackstopLiquidationsAndADL {
        payload: QueueBackstopLiquidationsAndADLPayload,
        batch_key: u128,
    },
    QueueMarginCallLiquidations {
        payload: QueueMarginCallLiquidationsPayload,
        batch_key: u128,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TwapOrderStatus {
    Open,
    Triggered { sub_order_id: OrderId, fill_size: u64 },
    Cancelled { reason: String },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AckPhase {
    BackstopLiquidation,
    MarginCall,
    CheckADL,
    TriggerADL,
    CommitMarkPrice,
    QueueBackstopLiquidationsAndADL,
    QueueMarginCallLiquidations,
    InitialEnqueue,
}

/// RESOURCE: AsyncMatchingEngine at market object address
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AsyncMatchingEngine {
    V1 {
        pending_requests: BTreeMap<PendingRequestKey, PendingRequest>,
        async_matching_enabled: bool,
        backstop_liquidations_in_queue: BTreeMap<[u8; 32], bool>,
        margin_call_liquidations_in_queue: BTreeMap<[u8; 32], bool>,
        mark_prices_in_queue: Vec<u64>,
    },
}

// ===================== Helper: should_continue_order =====================

/// Determines whether an order should be re-queued as a ContinuedOrder.
/// Returns true if the cancel reason is fill_limit_violation or clearinghouse_stopped_matching.
fn should_continue_order(cancel_reason: &Option<OrderCancellationReason>) -> bool {
    match cancel_reason {
        Some(reason) => {
            order_placement::is_fill_limit_violation(*reason)
                || order_placement::is_clearinghouse_stopped_matching(*reason)
        }
        None => false,
    }
}

// ===================== Functions =====================

pub fn register_market(async_matching_enabled: bool) -> AsyncMatchingEngine {
    AsyncMatchingEngine::V1 {
        pending_requests: BTreeMap::new(),
        async_matching_enabled,
        backstop_liquidations_in_queue: BTreeMap::new(),
        margin_call_liquidations_in_queue: BTreeMap::new(),
        mark_prices_in_queue: Vec::new(),
    }
}

fn new_pending_key(time: u64, priority: u8, tie_breaker: u128) -> PendingRequestKey {
    PendingRequestKey::V1 { time, priority, tie_breaker }
}

fn new_pending_liquidation_key(tie_breaker: u128) -> PendingRequestKey {
    new_pending_key(0, LIQUIDATION_PRIORITY, tie_breaker)
}

fn new_pending_check_adl_key(tie_breaker: u128) -> PendingRequestKey {
    new_pending_key(0, LIQUIDATION_PRIORITY, tie_breaker)
}

fn new_pending_commit_mark_price_key(tie_breaker: u128) -> PendingRequestKey {
    new_pending_key(0, LIQUIDATION_PRIORITY, tie_breaker)
}

fn new_margin_call_key(time: u64, tie_breaker: u128) -> PendingRequestKey {
    new_pending_key(time, MARGIN_CALL_PRIORITY, tie_breaker)
}

fn new_pending_transaction_key(now_microseconds: u64, tie_breaker: u128) -> PendingRequestKey {
    new_pending_key(now_microseconds + 1, REGULAR_ORDER_PRIORITY, tie_breaker)
}

/// Drain the async queue, removing up to batch_size entries.
/// Returns the number of entries removed.
pub fn drain_async_queue(
    engine: &mut AsyncMatchingEngine,
    batch_size: u64,
) -> u64 {
    let AsyncMatchingEngine::V1 {
        pending_requests,
        backstop_liquidations_in_queue,
        margin_call_liquidations_in_queue,
        mark_prices_in_queue,
        ..
    } = engine;

    let mut drained_count = 0u64;
    while !pending_requests.is_empty() && drained_count < batch_size {
        let key = *pending_requests.keys().next().unwrap();
        let request = pending_requests.remove(&key).unwrap();
        drained_count += 1;

        match request {
            PendingRequest::BackstopLiquidation { user, .. } => {
                backstop_liquidations_in_queue.remove(&user);
                // RESOURCE: global_liquidation_state::remove_continuation(user)
            }
            PendingRequest::MarginCall { user, .. } => {
                margin_call_liquidations_in_queue.remove(&user);
            }
            PendingRequest::CheckADL { .. } => {}
            PendingRequest::TriggerADL { .. } => {}
            PendingRequest::CommitMarkPrice { .. } => {
                if !mark_prices_in_queue.is_empty() {
                    mark_prices_in_queue.remove(0);
                }
            }
            PendingRequest::Twap(_twap) => {
                // EVENT: SystemPurgedOrderEvent
            }
            PendingRequest::Order(_order) => {
                // EVENT: SystemPurgedOrderEvent
            }
            PendingRequest::ContinuedOrder(_order) => {
                // EVENT: SystemPurgedOrderEvent
            }
            PendingRequest::QueueBackstopLiquidationsAndADL { .. } => {}
            PendingRequest::QueueMarginCallLiquidations { .. } => {}
        }
    }
    drained_count
}

/// Add a taker order to the pending queue.
/// Translated from: add_taker_order_to_pending in Move source.
pub fn add_taker_order_to_pending(
    engine: &mut AsyncMatchingEngine,
    order_args: PerpOrderRequestExtendedArgs,
    order_metadata: OrderMetadata,
    now_microseconds: u64,
    tie_breaker: u128,
) {
    let AsyncMatchingEngine::V1 { pending_requests, .. } = engine;
    let key = new_pending_transaction_key(now_microseconds, tie_breaker);
    let pending_order = PendingOrder::V1 { order_args, order_metadata };
    pending_requests.insert(key, PendingRequest::Order(pending_order));
}

/// The main trigger matching loop.
/// Translated from: trigger_matching_internal in Move source.
/// Loops calling trigger_matching_one_action_internal until no more work or queue is empty/time-gated.
pub fn trigger_matching_internal(
    engine: &mut AsyncMatchingEngine,
    current_time_micros: u64,
    remaining_work_units: &mut WorkUnit,
) {
    assert!(
        work_unit_utils::has_more_work(remaining_work_units),
        "EINVALD_WORK_UNITS_PER_TRIGGER: work units must be > 0"
    );
    let mut first = true;
    while trigger_matching_one_action_internal(
        engine,
        current_time_micros,
        first,
        remaining_work_units,
    ) {
        first = false;
        // loop while actions are being processed
    }
}

/// Process one pending request from the front of the queue.
/// Translated from: trigger_matching_one_action_internal in Move source.
/// Returns true if an action was processed (caller should continue looping),
/// false if no more work to do.
pub fn trigger_matching_one_action_internal(
    engine: &mut AsyncMatchingEngine,
    current_time_micros: u64,
    check_oracle_health: bool,
    remaining_work_units: &mut WorkUnit,
) -> bool {
    let AsyncMatchingEngine::V1 {
        pending_requests,
        async_matching_enabled,
        backstop_liquidations_in_queue,
        margin_call_liquidations_in_queue,
        mark_prices_in_queue,
    } = engine;

    if !pending_requests.is_empty() && work_unit_utils::has_more_work(remaining_work_units) {
        let pending_key = *pending_requests.keys().next().unwrap();
        let PendingRequestKey::V1 { time, .. } = pending_key;

        // If async matching is enabled, don't process future-timestamped requests
        if *async_matching_enabled && time > current_time_micros {
            return false;
        }

        if check_oracle_health {
            // RESOURCE: accounts_collateral::set_market_to_reduce_only_if_oracle_stale(market)
            // In native context, oracle health check is performed by the caller
        }

        let request = pending_requests.remove(&pending_key).unwrap();

        match request {
            PendingRequest::Order(order) => {
                let PendingOrder::V1 { order_args, order_metadata } = order;
                let orig_size = perp_order::get_orig_size(perp_order::get_common_args(&order_args));
                // Place order and trigger matching actions
                let result = order_placement_utils::place_order_and_trigger_matching_actions(
                    &order_args,
                    orig_size, // remaining_size = orig_size for new orders
                    true,      // emit_taker_order_open
                    remaining_work_units,
                    false,     // cancel_on_stop_matching
                );

                if result.remaining_size > 0 && should_continue_order(&result.cancel_reason) {
                    let continued_order = ContinuedPendingOrder::V1 {
                        order_args,
                        order_metadata,
                        remaining_size: result.remaining_size,
                    };
                    pending_requests.insert(
                        pending_key,
                        PendingRequest::ContinuedOrder(continued_order),
                    );
                }
            }
            PendingRequest::Twap(twap) => {
                trigger_pending_twap_instance(
                    pending_requests,
                    &twap,
                    remaining_work_units,
                    pending_key,
                );
            }
            PendingRequest::ContinuedOrder(continued_order) => {
                let ContinuedPendingOrder::V1 {
                    order_args,
                    order_metadata,
                    remaining_size,
                } = continued_order;
                let result = order_placement_utils::place_order_and_trigger_matching_actions(
                    &order_args,
                    remaining_size,
                    false,     // emit_taker_order_open = false for continued
                    remaining_work_units,
                    false,     // cancel_on_stop_matching
                );

                if result.remaining_size > 0 && should_continue_order(&result.cancel_reason) {
                    let continued_order = ContinuedPendingOrder::V1 {
                        order_args,
                        order_metadata,
                        remaining_size: result.remaining_size,
                    };
                    pending_requests.insert(
                        pending_key,
                        PendingRequest::ContinuedOrder(continued_order),
                    );
                }
            }
            PendingRequest::QueueBackstopLiquidationsAndADL { mut payload, batch_key } => {
                // EVENT: ACKEvent
                let done = process_queue_backstop_liquidations_and_adl(
                    pending_requests,
                    backstop_liquidations_in_queue,
                    &mut payload,
                    batch_key,
                    remaining_work_units,
                );
                if !done {
                    pending_requests.insert(
                        pending_key,
                        PendingRequest::QueueBackstopLiquidationsAndADL { payload, batch_key },
                    );
                }
            }
            PendingRequest::QueueMarginCallLiquidations { mut payload, batch_key } => {
                // EVENT: ACKEvent
                let done = process_queue_margin_call_liquidations(
                    pending_requests,
                    margin_call_liquidations_in_queue,
                    &mut payload,
                    batch_key,
                    remaining_work_units,
                    0, // starting_slippage_pct - would come from perp_market_config
                );
                if !done {
                    pending_requests.insert(
                        pending_key,
                        PendingRequest::QueueMarginCallLiquidations { payload, batch_key },
                    );
                }
            }
            PendingRequest::BackstopLiquidation { user, batch_key: _batch_key } => {
                // EVENT: ACKEvent - BackstopLiquidation phase
                // RESOURCE: Check if user has position via perp_positions::has_position_and_is_isolated
                // RESOURCE: Get continuation from global_liquidation_state
                // RESOURCE: Check account status and determine if backstop liquidation needed
                // RESOURCE: liquidation::trigger_backstop_liquidation_internal_with_continuation
                //
                // If continuation is still in progress after liquidation:
                //   - Store updated continuation in global_liquidation_state
                //   - Re-queue BackstopLiquidation with same pending_key
                // If account is no longer liquidatable:
                //   - Check if margin call is needed and enqueue if so
                //   - Remove from backstop_liquidations_in_queue
                // If liquidation completes:
                //   - Remove from backstop_liquidations_in_queue
                work_unit_utils::consume_small_work_units(remaining_work_units);
                backstop_liquidations_in_queue.remove(&user);
            }
            PendingRequest::MarginCall { user, continuation: _continuation, batch_key: _batch_key } => {
                // EVENT: ACKEvent - MarginCall phase
                // RESOURCE: liquidation::trigger_margin_call_internal(market, user, &mut continuation, remaining_work_units)
                //
                // Match on result:
                //   RequiresBackstopLiquidation => enqueue BackstopLiquidation + CheckADL, add to backstop queue, remove from margin call queue
                //   Continuation => re-queue MarginCall with cooldown period added to time
                //   Reprocess => re-queue MarginCall with same pending_key (ran out of work)
                //   Solvent => remove from margin_call_liquidations_in_queue
                work_unit_utils::consume_small_work_units(remaining_work_units);
                margin_call_liquidations_in_queue.remove(&user);
            }
            PendingRequest::CheckADL { batch_key: _batch_key } => {
                // EVENT: ACKEvent - CheckADL phase
                work_unit_utils::consume_small_work_units(remaining_work_units);
                // RESOURCE: get_mark_price(market)
                // RESOURCE: backstop_liquidator_profit_tracker::should_trigger_adl(market, mark_price, threshold)
                // If ADL should trigger, add TriggerADL to pending_requests with same pending_key
            }
            PendingRequest::TriggerADL { adl_price: _adl_price, batch_key: _batch_key } => {
                // EVENT: ACKEvent - TriggerADL phase
                // RESOURCE: Get backstop liquidator position size
                // RESOURCE: liquidation::trigger_adl_internal(market, liquidator_size, adl_price, remaining_work_units)
                // If need_continuation, re-queue TriggerADL with same pending_key
                work_unit_utils::consume_small_work_units(remaining_work_units);
            }
            PendingRequest::CommitMarkPrice { mark_px, batch_key: _batch_key } => {
                // EVENT: ACKEvent - CommitMarkPrice phase
                work_unit_utils::consume_small_work_units(remaining_work_units);
                if !mark_prices_in_queue.is_empty() {
                    let queued_px = mark_prices_in_queue.remove(0);
                    assert!(
                        queued_px == mark_px,
                        "ECOMMIT_MARK_PRICE_QUEUE_MISMATCH: queued price {} != commit price {}",
                        queued_px,
                        mark_px,
                    );
                    // RESOURCE: price_management::commit_mark_price(market, mark_px)
                    // RESOURCE: perp_positions::update_account_status_cache_on_market_state_change
                }
            }
        }
        true
    } else {
        false
    }
}

/// Process pending requests (main trigger loop).
/// Translated from: process_pending_requests -> trigger_matching_internal in Move source.
/// Processes the queue until empty, time-gated, or work units exhausted.
pub fn process_pending_requests(
    engine: &mut AsyncMatchingEngine,
    current_time_micros: u64,
    remaining_work_units: &mut WorkUnit,
) -> u64 {
    let mut processed = 0u64;
    if !work_unit_utils::has_more_work(remaining_work_units) {
        return processed;
    }

    let AsyncMatchingEngine::V1 {
        pending_requests,
        async_matching_enabled,
        backstop_liquidations_in_queue,
        margin_call_liquidations_in_queue,
        mark_prices_in_queue,
    } = engine;

    while !pending_requests.is_empty() && work_unit_utils::has_more_work(remaining_work_units) {
        let key = *pending_requests.keys().next().unwrap();
        let PendingRequestKey::V1 { time, .. } = key;

        if *async_matching_enabled && time > current_time_micros {
            break;
        }

        let request = pending_requests.remove(&key).unwrap();
        processed += 1;

        match request {
            PendingRequest::Order(order) => {
                let PendingOrder::V1 { order_args, order_metadata } = order;
                let orig_size = perp_order::get_orig_size(perp_order::get_common_args(&order_args));
                let result = order_placement_utils::place_order_and_trigger_matching_actions(
                    &order_args,
                    orig_size,
                    true,      // emit_taker_order_open
                    remaining_work_units,
                    false,     // cancel_on_stop_matching
                );

                if result.remaining_size > 0 && should_continue_order(&result.cancel_reason) {
                    let continued_order = ContinuedPendingOrder::V1 {
                        order_args,
                        order_metadata,
                        remaining_size: result.remaining_size,
                    };
                    pending_requests.insert(key, PendingRequest::ContinuedOrder(continued_order));
                }
            }
            PendingRequest::ContinuedOrder(continued_order) => {
                let ContinuedPendingOrder::V1 { order_args, order_metadata, remaining_size } = continued_order;
                let result = order_placement_utils::place_order_and_trigger_matching_actions(
                    &order_args,
                    remaining_size,
                    false,     // emit_taker_order_open = false for continued
                    remaining_work_units,
                    false,     // cancel_on_stop_matching
                );

                if result.remaining_size > 0 && should_continue_order(&result.cancel_reason) {
                    let continued_order = ContinuedPendingOrder::V1 {
                        order_args,
                        order_metadata,
                        remaining_size: result.remaining_size,
                    };
                    pending_requests.insert(key, PendingRequest::ContinuedOrder(continued_order));
                }
            }
            PendingRequest::Twap(twap) => {
                // Process TWAP instance through trigger_pending_twap_instance logic
                trigger_pending_twap_instance(
                    pending_requests,
                    &twap,
                    remaining_work_units,
                    key,
                );
            }
            PendingRequest::QueueBackstopLiquidationsAndADL { mut payload, batch_key } => {
                // EVENT: ACKEvent
                let done = process_queue_backstop_liquidations_and_adl(
                    pending_requests,
                    backstop_liquidations_in_queue,
                    &mut payload,
                    batch_key,
                    remaining_work_units,
                );
                if !done {
                    pending_requests.insert(key, PendingRequest::QueueBackstopLiquidationsAndADL { payload, batch_key });
                }
            }
            PendingRequest::QueueMarginCallLiquidations { mut payload, batch_key } => {
                // EVENT: ACKEvent
                let done = process_queue_margin_call_liquidations(
                    pending_requests,
                    margin_call_liquidations_in_queue,
                    &mut payload,
                    batch_key,
                    remaining_work_units,
                    0, // starting_slippage_pct - would come from perp_market_config
                );
                if !done {
                    pending_requests.insert(key, PendingRequest::QueueMarginCallLiquidations { payload, batch_key });
                }
            }
            PendingRequest::BackstopLiquidation { user, batch_key: _batch_key } => {
                // EVENT: ACKEvent - BackstopLiquidation
                // RESOURCE: Full backstop liquidation logic as in trigger_matching_one_action_internal
                work_unit_utils::consume_small_work_units(remaining_work_units);
                backstop_liquidations_in_queue.remove(&user);
            }
            PendingRequest::MarginCall { user, continuation: _continuation, batch_key: _batch_key } => {
                // EVENT: ACKEvent - MarginCall
                // RESOURCE: Full margin call logic as in trigger_matching_one_action_internal
                work_unit_utils::consume_small_work_units(remaining_work_units);
                margin_call_liquidations_in_queue.remove(&user);
            }
            PendingRequest::CheckADL { batch_key: _batch_key } => {
                // EVENT: ACKEvent - CheckADL
                work_unit_utils::consume_small_work_units(remaining_work_units);
                // RESOURCE: backstop_liquidator_profit_tracker::should_trigger_adl
            }
            PendingRequest::TriggerADL { adl_price: _adl_price, batch_key: _batch_key } => {
                // EVENT: ACKEvent - TriggerADL
                // RESOURCE: liquidation::trigger_adl_internal
                work_unit_utils::consume_small_work_units(remaining_work_units);
            }
            PendingRequest::CommitMarkPrice { mark_px, batch_key: _batch_key } => {
                // EVENT: ACKEvent - CommitMarkPrice
                work_unit_utils::consume_small_work_units(remaining_work_units);
                if !mark_prices_in_queue.is_empty() {
                    let queued_px = mark_prices_in_queue.remove(0);
                    assert!(
                        queued_px == mark_px,
                        "ECOMMIT_MARK_PRICE_QUEUE_MISMATCH"
                    );
                    // RESOURCE: price_management::commit_mark_price
                    // RESOURCE: perp_positions::update_account_status_cache_on_market_state_change
                }
            }
        }
    }
    processed
}

/// Trigger pending TWAP instance processing.
/// Translated from: trigger_pending_twap_instance in Move source.
///
/// Processes a single TWAP sub-order:
/// 1. Computes the sub-order size based on remaining TWAPs and remaining size
/// 2. Gets slippage price from order book
/// 3. Places the sub-order via place_order_and_trigger_matching_actions
/// 4. Re-queues as continued TWAP if fill limit hit, or schedules next TWAP instance
fn trigger_pending_twap_instance(
    pending_requests: &mut BTreeMap<PendingRequestKey, PendingRequest>,
    twap: &PendingTwap,
    remaining_work_units: &mut WorkUnit,
    pending_key: PendingRequestKey,
) {
    work_unit_utils::consume_small_work_units(remaining_work_units);

    let PendingTwap::V1 {
        account,
        order_id,
        orig_size,
        ref instance_remaining_size,
        remaining_size,
        is_buy,
        is_reduce_only,
        twap_start_time_s,
        twap_frequency_s,
        twap_end_time_s,
        ref builder_code,
        ref client_order_id,
    } = *twap;

    // RESOURCE: current_time would come from decibel_time::now_seconds()
    // For native context, we use a placeholder; the actual time comes from the execution context
    let current_time_s = twap_start_time_s; // Placeholder - caller provides actual time

    let num_remaining_twap = if current_time_s >= twap_end_time_s {
        1u64
    } else {
        let remaining_time = twap_end_time_s - current_time_s;
        ((remaining_time + twap_frequency_s / 2) / twap_frequency_s) + 1
    };

    // RESOURCE: market_lot_size and market_min_size would come from perp_market_config
    let market_lot_size = 1u64; // Placeholder - caller provides actual lot size
    let market_min_size = 1u64; // Placeholder - caller provides actual min size

    let twap_size = if instance_remaining_size.is_some() && num_remaining_twap > 1 {
        instance_remaining_size.unwrap()
    } else {
        let mut twap_size = remaining_size / num_remaining_twap;
        twap_size = twap_size / market_lot_size * market_lot_size;
        if twap_size < market_min_size {
            // Size too small, delay by one frequency interval
            // EVENT: TwapEvent with Triggered(order_id, 0)
            // RESOURCE: place_twap_order_helper to re-schedule
            return;
        }
        twap_size
    };

    // RESOURCE: Get slippage price from perp_market::get_slippage_price
    // let price_opt = perp_market::get_slippage_price(market, is_buy, SLIPPAGE_TOLERANCE_FOR_TWAP);
    // For native context, we proceed with the order placement

    // Place the TWAP sub-order via place_order_and_trigger_matching_actions
    let sub_order_id = OrderId { order_id: 0 }; // RESOURCE: next_order_id() in real context
    let result = order_placement_utils::place_order_and_trigger_matching_actions(
        &perp_order::new_order_extended_args(
            account,
            // Use a default common args - in real context, price comes from slippage calculation
            perp_order::new_order_common_args(
                HI_PRICE, // price placeholder
                orig_size,
                is_buy,
                crate::native_perpdex::order_book_types::TimeInForce::IOC,
                None,
            ).unwrap_or_else(|_| panic!("Failed to create order common args")),
            sub_order_id,
            None, // trigger condition
        ),
        twap_size,
        true,  // emit_taker_order_open
        remaining_work_units,
        true,  // cancel_on_stop_matching
    );

    let total_fill_size: u64 = result.fill_sizes.iter().sum();
    // EVENT: TwapEvent with Triggered(sub_order_id, total_fill_size)

    let twap_remaining_size = remaining_size - total_fill_size;
    let twap_instance_remaining_size = (twap_size - total_fill_size) / market_lot_size * market_lot_size;

    // If instance has remaining size and should continue (fill limit or stopped matching)
    if twap_instance_remaining_size >= market_min_size
        && should_continue_order(&result.cancel_reason)
    {
        let twap_order = PendingTwap::V1 {
            account,
            order_id,
            orig_size,
            instance_remaining_size: Some(twap_instance_remaining_size),
            remaining_size: twap_remaining_size,
            is_buy,
            is_reduce_only,
            twap_start_time_s,
            twap_frequency_s,
            twap_end_time_s,
            builder_code: builder_code.clone(),
            client_order_id: client_order_id.clone(),
        };
        pending_requests.insert(pending_key, PendingRequest::Twap(twap_order));
        return;
    }

    let num_remaining_twap = num_remaining_twap - 1;

    // Check if TWAP should be cancelled
    let twap_valid = match &result.cancel_reason {
        None => true,
        Some(reason) => {
            order_placement::is_ioc_violation(*reason)
                || order_placement::is_fill_limit_violation(*reason)
                || order_placement::is_clearinghouse_stopped_matching(*reason)
        }
    };

    if !twap_valid
        || (twap_remaining_size < market_min_size && twap_remaining_size > 0)
        || (num_remaining_twap == 0 && twap_remaining_size > 0)
    {
        // EVENT: TwapEvent with Cancelled
        // TWAP is cancelled due to sub-order failure, remaining size too small, or TWAP ended
        return;
    }

    if num_remaining_twap != 0 {
        // RESOURCE: place_twap_order_helper to schedule next TWAP instance
        // In real context, this calls perp_market::place_order_with_order_id to schedule
        // the next time-based trigger
    }
}

fn process_queue_backstop_liquidations_and_adl(
    pending_requests: &mut BTreeMap<PendingRequestKey, PendingRequest>,
    backstop_liquidations_in_queue: &mut BTreeMap<[u8; 32], bool>,
    payload: &mut QueueBackstopLiquidationsAndADLPayload,
    batch_key: u128,
    remaining_work_units: &mut WorkUnit,
) -> bool {
    let QueueBackstopLiquidationsAndADLPayload::V1 {
        backstop_liquidations,
        backstop_liquidation_keys,
        check_adl_key,
        any_backstop_liquidation_added,
        needs_adl_check,
    } = payload;

    while !backstop_liquidations.is_empty() {
        let account = backstop_liquidations.pop().unwrap();
        let key = backstop_liquidation_keys.pop().unwrap();
        work_unit_utils::consume_small_work_units(remaining_work_units);

        // Skip backstop liquidator and duplicates
        // RESOURCE: In real context, also check account != backstop_liquidator()
        if !backstop_liquidations_in_queue.contains_key(&account) {
            backstop_liquidations_in_queue.insert(account, true);
            pending_requests.insert(key, PendingRequest::BackstopLiquidation { user: account, batch_key });
            *any_backstop_liquidation_added = true;
        }

        if !work_unit_utils::has_more_work(remaining_work_units) {
            return false;
        }
    }

    // Add ADL check if needed
    if *needs_adl_check || *any_backstop_liquidation_added {
        if let Some(adl_key) = check_adl_key.take() {
            pending_requests.insert(adl_key, PendingRequest::CheckADL { batch_key });
        }
    }
    true
}

fn process_queue_margin_call_liquidations(
    pending_requests: &mut BTreeMap<PendingRequestKey, PendingRequest>,
    margin_call_liquidations_in_queue: &mut BTreeMap<[u8; 32], bool>,
    payload: &mut QueueMarginCallLiquidationsPayload,
    batch_key: u128,
    remaining_work_units: &mut WorkUnit,
    starting_slippage_pct: u64,
) -> bool {
    let QueueMarginCallLiquidationsPayload::V1 {
        margin_call_liquidations,
        margin_call_liquidation_keys,
    } = payload;

    while !margin_call_liquidations.is_empty() {
        let account = margin_call_liquidations.pop().unwrap();
        let key = margin_call_liquidation_keys.pop().unwrap();
        work_unit_utils::consume_small_work_units(remaining_work_units);

        // Skip backstop liquidator and duplicates
        // RESOURCE: In real context, also check account != backstop_liquidator()
        if !margin_call_liquidations_in_queue.contains_key(&account) {
            margin_call_liquidations_in_queue.insert(account, true);
            pending_requests.insert(key, PendingRequest::MarginCall {
                user: account,
                continuation: crate::native_perpdex::liquidation::default_margin_call_continuation(starting_slippage_pct),
                batch_key,
            });
        }

        if !work_unit_utils::has_more_work(remaining_work_units) {
            break;
        }
    }
    margin_call_liquidations.is_empty()
}

/// Schedule backstop liquidations and ADL check into the queue.
pub fn schedule_queue_backstop_liquidations_and_adl(
    engine: &mut AsyncMatchingEngine,
    backstop_liquidations: Vec<[u8; 32]>,
    mark_price_updated: bool,
    remaining_work_units: &mut WorkUnit,
    batch_key: u128,
    tie_breaker_start: u128,
) {
    work_unit_utils::consume_small_work_units(remaining_work_units);

    let AsyncMatchingEngine::V1 { pending_requests, .. } = engine;

    if backstop_liquidations.is_empty() {
        if mark_price_updated {
            let key = new_pending_check_adl_key(tie_breaker_start);
            pending_requests.insert(key, PendingRequest::CheckADL { batch_key });
        }
        return;
    }

    // EVENT: ACKEvent - InitialEnqueue

    let mut bl = backstop_liquidations;
    let mut bl_keys: Vec<PendingRequestKey> = (0..bl.len())
        .map(|i| new_pending_liquidation_key(tie_breaker_start + i as u128))
        .collect();

    bl.reverse();
    bl_keys.reverse();

    let check_adl_key = new_pending_check_adl_key(tie_breaker_start + bl.len() as u128);
    let mut split = false;

    // Split into batches if needed
    while bl.len() > MAX_QUEUE_LIQUIDATION_BATCH_SIZE as usize {
        let split_at = bl.len() - MAX_QUEUE_LIQUIDATION_BATCH_SIZE as usize;
        let current = bl.split_off(split_at);
        let current_keys = bl_keys.split_off(split_at);
        let queue_key = new_pending_liquidation_key(tie_breaker_start + bl.len() as u128 + 100);
        pending_requests.insert(queue_key, PendingRequest::QueueBackstopLiquidationsAndADL {
            payload: QueueBackstopLiquidationsAndADLPayload::V1 {
                backstop_liquidations: current,
                backstop_liquidation_keys: current_keys,
                check_adl_key: None,
                any_backstop_liquidation_added: false,
                needs_adl_check: false,
            },
            batch_key,
        });
        split = true;
    }

    let queue_key = new_pending_liquidation_key(tie_breaker_start + 200);
    pending_requests.insert(queue_key, PendingRequest::QueueBackstopLiquidationsAndADL {
        payload: QueueBackstopLiquidationsAndADLPayload::V1 {
            backstop_liquidations: bl,
            backstop_liquidation_keys: bl_keys,
            check_adl_key: Some(check_adl_key),
            any_backstop_liquidation_added: false,
            // if split we need to do the ADL check always,
            // as we don't know if any backstop liquidations were added
            needs_adl_check: mark_price_updated || split,
        },
        batch_key,
    });
}

/// Schedule margin call liquidations into the queue.
pub fn schedule_queue_margin_call_liquidations(
    engine: &mut AsyncMatchingEngine,
    margin_call_liquidations: Vec<[u8; 32]>,
    remaining_work_units: &mut WorkUnit,
    batch_key: u128,
    cur_time_micros: u64,
    tie_breaker_start: u128,
) {
    work_unit_utils::consume_small_work_units(remaining_work_units);

    if margin_call_liquidations.is_empty() {
        return;
    }

    // EVENT: ACKEvent - InitialEnqueue

    let AsyncMatchingEngine::V1 { pending_requests, .. } = engine;

    let mut mcl = margin_call_liquidations;
    let mut mcl_keys: Vec<PendingRequestKey> = (0..mcl.len())
        .map(|i| new_margin_call_key(cur_time_micros, tie_breaker_start + i as u128))
        .collect();

    mcl.reverse();
    mcl_keys.reverse();

    while !mcl.is_empty() {
        let split_at = if mcl.len() > MAX_QUEUE_LIQUIDATION_BATCH_SIZE as usize {
            mcl.len() - MAX_QUEUE_LIQUIDATION_BATCH_SIZE as usize
        } else {
            0
        };
        let current = mcl.split_off(split_at);
        let current_keys = mcl_keys.split_off(split_at);

        let queue_key = new_margin_call_key(cur_time_micros, tie_breaker_start + mcl.len() as u128 + 300);
        pending_requests.insert(queue_key, PendingRequest::QueueMarginCallLiquidations {
            payload: QueueMarginCallLiquidationsPayload::V1 {
                margin_call_liquidations: current,
                margin_call_liquidation_keys: current_keys,
            },
            batch_key,
        });
    }
}

/// Schedule a mark price commit into the queue.
pub fn schedule_commit_mark_price(
    engine: &mut AsyncMatchingEngine,
    mark_px: u64,
    batch_key: u128,
    tie_breaker: u128,
) {
    let AsyncMatchingEngine::V1 { pending_requests, mark_prices_in_queue, .. } = engine;
    let key = new_pending_commit_mark_price_key(tie_breaker);
    pending_requests.insert(key, PendingRequest::CommitMarkPrice { mark_px, batch_key });
    mark_prices_in_queue.push(mark_px);
}

/// Add an ADL check to the pending queue.
pub fn add_adl_to_pending(
    engine: &mut AsyncMatchingEngine,
    batch_key: u128,
    tie_breaker: u128,
) {
    let AsyncMatchingEngine::V1 { pending_requests, .. } = engine;
    let key = new_pending_check_adl_key(tie_breaker);
    pending_requests.insert(key, PendingRequest::CheckADL { batch_key });
}

/// Get the number of pending requests (O(1) for BTreeMap).
pub fn get_async_queue_length(engine: &AsyncMatchingEngine) -> u64 {
    let AsyncMatchingEngine::V1 { pending_requests, .. } = engine;
    pending_requests.len() as u64
}

/// Check if there are pending requests ready to process.
pub fn pending_request_exists(engine: &AsyncMatchingEngine, current_time_micros: u64) -> bool {
    let AsyncMatchingEngine::V1 { pending_requests, .. } = engine;
    if pending_requests.is_empty() {
        return false;
    }
    let key = pending_requests.keys().next().unwrap();
    let PendingRequestKey::V1 { time, .. } = key;
    *time <= current_time_micros
}

/// Get the number of mark prices currently queued for commit.
pub fn view_mark_prices_in_queue_length(engine: &AsyncMatchingEngine) -> u64 {
    let AsyncMatchingEngine::V1 { mark_prices_in_queue, .. } = engine;
    mark_prices_in_queue.len() as u64
}

/// Get the Nth mark price from the queue.
pub fn view_nth_mark_price_in_queue(engine: &AsyncMatchingEngine, n: u64) -> Option<u64> {
    let AsyncMatchingEngine::V1 { mark_prices_in_queue, .. } = engine;
    mark_prices_in_queue.get(n as usize).copied()
}

/// Get the first N mark prices from the queue.
pub fn view_first_n_mark_prices_in_queue(engine: &AsyncMatchingEngine, n: u64) -> Vec<u64> {
    let AsyncMatchingEngine::V1 { mark_prices_in_queue, .. } = engine;
    mark_prices_in_queue.iter().take(n as usize).copied().collect()
}

/// Place a TWAP order. Validates parameters and creates initial TWAP tracking.
pub fn validate_twap_params(
    orig_size: u64,
    twap_frequency_s: u64,
    twap_duration_s: u64,
    market_min_size: u64,
) -> Result<(), u64> {
    if twap_duration_s < MIN_TWAP_DURATION_S {
        return Err(EINVALID_TWAP_DURATION);
    }
    if twap_duration_s > MAX_TWAP_DURATION_S {
        return Err(EINVALID_TWAP_DURATION);
    }
    if twap_frequency_s < MIN_TWAP_FREQUENCY_S {
        return Err(EINVALID_TWAP_FREQUENCY);
    }
    if twap_duration_s < twap_frequency_s {
        return Err(EINVALID_TWAP_DURATION);
    }
    if twap_duration_s % twap_frequency_s != 0 {
        return Err(ETWAP_DURATION_NOT_MULTIPLE_OF_FREQUENCY);
    }
    let num_remaining_twap = (twap_duration_s / twap_frequency_s) + 1;
    if (orig_size / num_remaining_twap) < market_min_size {
        return Err(EINDIVIDUAL_TWAP_INSTANCE_SMALLER_THAN_MIN_SIZE);
    }
    Ok(())
}

/// Trigger price-based conditional orders that are ready at the given mark price.
/// Returns the number of orders triggered.
pub fn trigger_price_based_conditional_orders_count(
    _max_orders: u32,
    _work_units: &mut WorkUnit,
) -> u32 {
    // In native context, the ready orders are fetched from the market
    // and each is re-placed via place_maker_or_queue_taker.
    // The actual implementation delegates to perp_market::take_ready_price_based_orders
    0
}

/// Get the request name for debugging
pub fn request_name(request: &PendingRequest) -> &str {
    match request {
        PendingRequest::Order(_) => "Order",
        PendingRequest::Twap(_) => "Twap",
        PendingRequest::ContinuedOrder(_) => "ContinuedOrder",
        PendingRequest::BackstopLiquidation { .. } => "BackstopLiquidation",
        PendingRequest::MarginCall { .. } => "MarginCall",
        PendingRequest::CheckADL { .. } => "CheckADL",
        PendingRequest::TriggerADL { .. } => "TriggerADL",
        PendingRequest::CommitMarkPrice { .. } => "CommitMarkPrice",
        PendingRequest::QueueBackstopLiquidationsAndADL { .. } => "QueueBackstopLiquidationsAndADL",
        PendingRequest::QueueMarginCallLiquidations { .. } => "QueueMarginCallLiquidations",
    }
}


// ===================== Market-level delegation functions =====================
// These functions take a market address and delegate to the internal functions.
// In the Move source, they resolve the market object to get the AsyncMatchingEngine.
// In native context, the engine resolution is handled by the VM execution layer.

/// Place a maker order directly, or queue a taker order for async processing.
/// Translated from: place_maker_or_queue_taker in Move source.
///
/// Logic:
/// 1. Generate or reuse order_id
/// 2. Validate TP/SL child orders
/// 3. Validate/filter builder code
/// 4. Compute trigger condition from stop_price
/// 5. Create extended order args
/// 6. Check if order is taker (crosses book)
/// 7. If taker: add_taker_order_to_pending
/// 8. If maker: place_order_and_trigger_matching_actions with match_count==0 assertion
pub fn place_maker_or_queue_taker(
    _market: [u8; 32],
    user: [u8; 32],
    order_request: crate::native_perpdex::perp_order::PerpOrderRequestCommonArgs,
    orig_order_id: Option<crate::native_perpdex::order_book_types::OrderId>,
    is_reduce_only: bool,
    stop_price: Option<u64>,
    tpsl_order_request: crate::native_perpdex::perp_order::PerpOrderRequestTpSlArgs,
    _builder_code: Option<crate::native_perpdex::builder_code_registry::BuilderCode>,
    _allow_abort: bool,
) -> Result<crate::native_perpdex::order_book_types::OrderId, u64> {
    let is_buy = perp_order::get_is_buy(&order_request);
    let _price = perp_order::get_price(&order_request);
    let (_tp_trigger_price, _tp_limit_price, _sl_trigger_price, _sl_limit_price) =
        perp_order::tpsl_into_inner(tpsl_order_request);

    // Generate or reuse order_id
    let (order_id, first_placed) = if let Some(id) = orig_order_id {
        (id, false)
    } else {
        // RESOURCE: In real context, order_id_generation::next_order_id()
        (OrderId { order_id: 1 }, true)
    };

    // Validate TP/SL child orders
    // RESOURCE: tp_sl_utils::validate_and_get_child_tp_sl_orders(market, order_id, is_buy, price, ...)
    let tp: Option<ChildTpSlOrder> = None; // Placeholder - real validation in native context
    let sl: Option<ChildTpSlOrder> = None; // Placeholder - real validation in native context

    if tp.is_some() || sl.is_some() {
        if is_reduce_only {
            return Err(EINVALID_TP_SL_FOR_REDUCE_ONLY);
        }
        if stop_price.is_some() {
            return Err(EINVALID_TP_SL_WITH_TRIGGER_CONDITION);
        }
    }

    // Compute trigger condition from stop_price
    let trigger_condition = if let Some(stop_px) = stop_price {
        // RESOURCE: get_mark_price(market) and validate_price(market, stop_px) in real context
        if is_buy {
            // assert!(mark_price < stop_px, EINVALID_STOP_PRICE)
            Some(crate::native_perpdex::order_book_types::TriggerCondition::PriceMoveAbove(stop_px))
        } else {
            // assert!(mark_price > stop_px, EINVALID_STOP_PRICE)
            Some(crate::native_perpdex::order_book_types::TriggerCondition::PriceMoveBelow(stop_px))
        }
    } else {
        None
    };

    let order_args = perp_order::new_order_extended_args(
        user,
        order_request,
        order_id,
        trigger_condition,
    );

    if first_placed {
        // EVENT: emit order ack event via perp_market::emit_event_for_order
    }

    // Check if order is taker (crosses the book)
    // RESOURCE: perp_market::is_taker_order(market, price, is_buy, trigger_condition)
    // In native context, we delegate to the market's order book to determine this.
    // For now, the actual taker/maker determination happens at the VM execution layer.
    let is_taker = false; // Placeholder - determined by market order book state

    if is_taker {
        // Taker order: add to pending queue for async processing
        // RESOURCE: In real context, get engine from market and call add_taker_order_to_pending
        // The engine resolution happens at the VM layer
    } else {
        // Maker order: place directly, should have no matches
        let mut default_work_units = work_unit_utils::get_default_work_units();
        let result = order_placement_utils::place_order_and_trigger_matching_actions(
            &order_args,
            perp_order::get_orig_size(perp_order::get_common_args(&order_args)),
            true,  // emit_taker_order_open
            &mut default_work_units,
            false, // cancel_on_stop_matching
        );
        assert!(
            result.match_count == 0,
            "EMAKER_SHOULD_HAVE_NO_MATCHES: maker order should not match"
        );
    }

    Ok(order_id)
}

/// Trigger matching sometimes, called after each order placement.
/// Translated from Move source: this is a no-op in the current implementation.
/// Matching is done by dedicated transaction at end of block.
pub fn trigger_matching_sometimes(
    _market: [u8; 32],
    _max_work_units: crate::native_perpdex::work_unit_utils::WorkUnit,
) {
    // no-op: matching is done by dedicated txn at end of block
    // This matches the Move source which has this as a no-op
}

/// Process pending requests for a specific market.
/// Translated from: process_pending_requests in Move source.
/// Delegates to trigger_matching_internal which loops through the queue.
pub fn process_pending_requests_for_market(
    _market: [u8; 32],
    _max_work_units: crate::native_perpdex::work_unit_utils::WorkUnit,
) {
    // RESOURCE: In real context:
    // 1. Resolve market address to AsyncMatchingEngine
    // 2. Call trigger_matching_internal(engine, current_time_micros, &mut max_work_units)
    //
    // The Move source does:
    //   fun process_pending_requests(market, max_work_units) {
    //       trigger_matching_internal(market, max_work_units);
    //   }
    //
    // The actual engine resolution and time retrieval happens at the VM execution layer.
}

/// Drain the async queue for a specific market.
/// Removes up to batch_size entries from the pending request queue.
pub fn drain_async_queue_for_market(
    _market: [u8; 32],
    _batch_size: u64,
) {
    // RESOURCE: In real context:
    // 1. Resolve market address to AsyncMatchingEngine
    // 2. Call drain_async_queue(engine, batch_size)
    //
    // The actual engine resolution happens at the VM execution layer.
}

/// Trigger TWAP orders for a specific market.
/// Translated from: trigger_twap_orders in Move source.
///
/// Logic:
/// 1. Take ready time-based orders from the order book (up to DEFAULT_PENDING_TWAPS_TO_TRIGGER)
/// 2. For each ready TWAP order:
///    a. Consume small work units
///    b. Destroy the order to extract fields
///    c. Get TWAP metadata (start_time, frequency, end_time)
///    d. Create PendingTwap with order details
///    e. Add to pending_requests queue with new_pending_transaction_key
pub fn trigger_twap_orders(
    _market: [u8; 32],
    _max_work_units: &mut crate::native_perpdex::work_unit_utils::WorkUnit,
) {
    // RESOURCE: In real context:
    // 1. let twap_orders = perp_market::take_ready_time_based_orders(market, DEFAULT_PENDING_TWAPS_TO_TRIGGER)
    // 2. For each order:
    //    max_work_units.consume_small_work_units();
    //    Destructure order to get user, order_id, orig_size, remaining_size, is_bid, metadata
    //    Get (start_time_s, frequency_s, end_time_s) from metadata
    //    Create PendingTwap::V1 { account, order_id, orig_size, instance_remaining_size: None,
    //           remaining_size, is_buy, is_reduce_only, twap_start_time_s, twap_frequency_s,
    //           twap_end_time_s, builder_code, client_order_id }
    //    Insert into engine.pending_requests with new_pending_transaction_key
}

/// Place a TWAP order.
/// Translated from: place_twap_order in Move source.
///
/// Logic:
/// 1. Validate builder code if present
/// 2. Validate TWAP parameters (duration, frequency, min size)
/// 3. Compute TWAP timing (start, end, num instances)
/// 4. Create order metadata with TWAP metadata
/// 5. Generate order_id
/// 6. Compute price (HI_PRICE for buy, tick_size for sell)
/// 7. Place initial order via perp_market::place_order_with_order_id as IOC
/// 8. Emit TwapEvent::Open
/// 9. Return order_id
pub fn place_twap_order(
    _market: [u8; 32],
    _user: [u8; 32],
    orig_size: u64,
    is_buy: bool,
    is_reduce_only: bool,
    _client_order_id: Option<String>,
    twap_frequency_s: u64,
    twap_duration_s: u64,
    builder_code: Option<crate::native_perpdex::builder_code_registry::BuilderCode>,
) -> Result<crate::native_perpdex::order_book_types::OrderId, u64> {
    // Validate builder code if present
    // RESOURCE: if builder_code.is_some() { validate_builder_code(user, &builder_code) }

    // Validate TWAP parameters
    if twap_duration_s < MIN_TWAP_DURATION_S {
        return Err(EINVALID_TWAP_DURATION);
    }
    if twap_duration_s > MAX_TWAP_DURATION_S {
        return Err(EINVALID_TWAP_DURATION);
    }
    if twap_frequency_s < MIN_TWAP_FREQUENCY_S {
        return Err(EINVALID_TWAP_FREQUENCY);
    }
    if twap_duration_s < twap_frequency_s {
        return Err(EINVALID_TWAP_DURATION);
    }
    if twap_duration_s % twap_frequency_s != 0 {
        return Err(ETWAP_DURATION_NOT_MULTIPLE_OF_FREQUENCY);
    }

    // RESOURCE: twap_start_time_s = decibel_time::now_seconds()
    let twap_start_time_s: u64 = 0; // Placeholder - actual time from execution context
    let twap_end_time_s = twap_start_time_s + twap_duration_s;
    let num_remaining_twap = (twap_duration_s / twap_frequency_s) + 1;

    // RESOURCE: market_min_size = get_min_size(market), tick_size = get_ticker_size(market)
    let market_min_size: u64 = 1; // Placeholder
    let tick_size: u64 = 1;       // Placeholder

    if (orig_size / num_remaining_twap) < market_min_size {
        return Err(EINDIVIDUAL_TWAP_INSTANCE_SMALLER_THAN_MIN_SIZE);
    }

    // Create order metadata with TWAP metadata
    let _metadata = new_order_metadata(
        is_reduce_only,
        Some(new_twap_metadata(twap_start_time_s, twap_frequency_s, twap_end_time_s)),
        None, // tp
        None, // sl
        builder_code,
    );

    // RESOURCE: order_id = next_order_id()
    let order_id = OrderId { order_id: 1 }; // Placeholder - real ID from order_id_generation

    // Compute price: HI_PRICE for buy (rounded down to tick), tick_size for sell
    let _price = if is_buy {
        HI_PRICE / tick_size * tick_size
    } else {
        tick_size
    };

    // RESOURCE: perp_market::place_order_with_order_id(market, order_args, orig_size, metadata, ...)
    // The actual order placement into the order book happens at the VM execution layer

    // EVENT: TwapEvent::Open

    Ok(order_id)
}

/// Cancel a TWAP order for a specific market.
/// Translated from: cancel_twap_order in Move source.
///
/// Logic:
/// 1. Cancel the order on the market via perp_market::cancel_order
/// 2. Destructure the cancelled order to get fields
/// 3. Extract TWAP metadata (start_time, frequency, end_time)
/// 4. Emit TwapEvent with Cancelled status
pub fn cancel_twap_order_for_market(
    _market: [u8; 32],
    _user: [u8; 32],
    _order_id: crate::native_perpdex::order_book_types::OrderId,
) -> Result<(), u64> {
    // RESOURCE: In real context:
    // 1. let order = perp_market::cancel_order(market, user, order_id, false,
    //        order_cancellation_reason_cancelled_by_user(), "", &clearinghouse_perp::market_callbacks(market))
    // 2. Destructure order to get order_request
    // 3. Extract TWAP metadata: (start_time_s, frequency_s, end_time_s) = get_twap_from_metadata(&metadata)
    // 4. Emit TwapEvent::V1 { ..., status: Cancelled("Cancelled by user"), remaining_size: 0 }
    Ok(())
}

/// Schedule a mark price commit for a specific market.
/// Translated from: schedule_commit_mark_price in Move source.
pub fn schedule_commit_mark_price_for_market(
    _market: [u8; 32],
    _mark_px: u64,
    _batch_key: u128,
) {
    // RESOURCE: In real context:
    // 1. Resolve market to engine
    // 2. Call schedule_commit_mark_price(engine, mark_px, batch_key, tie_breaker)
    //    where tie_breaker = monotonically_increasing_counter()
    //
    // The Move source does:
    //   let pending_req_key = new_pending_commit_mark_price_key();
    //   let market_engine = get_perp_market_engine_mut(market);
    //   market_engine.pending_requests.add(pending_req_key, CommitMarkPrice { mark_px, batch_key });
    //   market_engine.mark_prices_in_queue.push_back(mark_px);
}

/// Trigger price-based conditional orders for a specific market.
/// Translated from: trigger_price_based_conditional_orders in Move source.
///
/// Logic:
/// 1. Take ready price-based orders from order book up to limit
/// 2. For each order:
///    a. Consume order placement work units
///    b. Destructure order to get fields
///    c. Call place_maker_or_queue_taker with the order parameters
pub fn trigger_price_based_conditional_orders(
    _market: [u8; 32],
    _mark_price: u64,
    _max_work_units: &mut crate::native_perpdex::work_unit_utils::WorkUnit,
) {
    // RESOURCE: In real context:
    // 1. let ready_orders = perp_market::take_ready_price_based_orders(market, mark_price,
    //        max_work_units.get_max_order_placement_limit(DEFAULT_PENDING_ORDERS_TO_TRIGGER))
    // 2. For each order:
    //    max_work_units.consume_order_placement_work_units();
    //    Destructure order to get user, order_id, price, orig_size, is_bid, time_in_force, metadata
    //    place_maker_or_queue_taker(market, user, new_order_common_args(price, orig_size, is_bid, tif, client_order_id),
    //        Some(order_id), is_reduce_only(&metadata), None, new_empty_order_tp_sl_args(),
    //        get_builder_code_from_metadata(&metadata), false)
}

// ===================== Dispatch stubs (by-addr) for perp_engine delegation =====================

pub fn get_async_queue_length_by_addr(_market: [u8; 32]) -> u64 {
    // Dispatch layer resolves AsyncMatchingEngine resource
    0
}
