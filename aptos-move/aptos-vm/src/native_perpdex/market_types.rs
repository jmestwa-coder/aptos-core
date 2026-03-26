// Copyright (c) Aptos Foundation
// Translated from: aptos_market::market_types
//
// NOTE: This module contains many event emission functions and clearinghouse callback types.
// In Move, MarketClearinghouseCallbacks uses closures (lambda functions). In Rust, we model
// these as trait objects or function pointers. Since the callbacks are provided by the caller
// and are used to interact with the clearinghouse, we define them as a trait.

use crate::native_perpdex::bulk_order_types::BulkOrder;
use crate::native_perpdex::market_clearinghouse_order_info::MarketClearinghouseOrderInfo;
use crate::native_perpdex::order_book::{self, OrderBook};
use crate::native_perpdex::order_book_types::{
    OrderId, TriggerCondition,
};
use crate::native_perpdex::single_order_types::SingleOrder;

// ===================== OrderCancellationReason =====================

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum OrderCancellationReason {
    PostOnlyViolation,
    IOCViolation,
    PositionUpdateViolation,
    ReduceOnlyViolation,
    ClearinghouseSettleViolation,
    MaxFillLimitViolation,
    DuplicateClientOrderIdViolation,
    OrderPreCancelled,
    PlaceMakerOrderViolation,
    DeadMansSwitchExpired,
    DisallowedSelfTrading,
    OrderCancelledByUser,
    OrderCancelledBySystem,
    OrderCancelledBySystemDueToError,
    ClearinghouseStoppedMatching,
}

pub fn order_cancellation_reason_post_only_violation() -> OrderCancellationReason {
    OrderCancellationReason::PostOnlyViolation
}

pub fn order_cancellation_reason_ioc_violation() -> OrderCancellationReason {
    OrderCancellationReason::IOCViolation
}

pub fn order_cancellation_reason_position_update_violation() -> OrderCancellationReason {
    OrderCancellationReason::PositionUpdateViolation
}

pub fn order_cancellation_reason_clearinghouse_settle_violation() -> OrderCancellationReason {
    OrderCancellationReason::ClearinghouseSettleViolation
}

pub fn order_cancellation_reason_max_fill_limit_violation() -> OrderCancellationReason {
    OrderCancellationReason::MaxFillLimitViolation
}

pub fn order_cancellation_reason_duplicate_client_order_id() -> OrderCancellationReason {
    OrderCancellationReason::DuplicateClientOrderIdViolation
}

pub fn order_cancellation_reason_order_pre_cancelled() -> OrderCancellationReason {
    OrderCancellationReason::OrderPreCancelled
}

pub fn order_cancellation_reason_place_maker_order_violation() -> OrderCancellationReason {
    OrderCancellationReason::PlaceMakerOrderViolation
}

pub fn order_cancellation_reason_dead_mans_switch_expired() -> OrderCancellationReason {
    OrderCancellationReason::DeadMansSwitchExpired
}

pub fn order_cancellation_reason_disallowed_self_trading() -> OrderCancellationReason {
    OrderCancellationReason::DisallowedSelfTrading
}

pub fn order_cancellation_reason_cancelled_by_user() -> OrderCancellationReason {
    OrderCancellationReason::OrderCancelledByUser
}

pub fn order_cancellation_reason_cancelled_by_system() -> OrderCancellationReason {
    OrderCancellationReason::OrderCancelledBySystem
}

pub fn order_cancellation_reason_cancelled_by_system_due_to_error() -> OrderCancellationReason {
    OrderCancellationReason::OrderCancelledBySystemDueToError
}

pub fn order_cancellation_reason_clearinghouse_stopped_matching() -> OrderCancellationReason {
    OrderCancellationReason::ClearinghouseStoppedMatching
}

// ===================== OrderStatus =====================

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum OrderStatus {
    Open,
    Filled,
    Cancelled,
    Rejected,
    SizeReduced,
    Acknowledged,
}

pub fn order_status_open() -> OrderStatus {
    OrderStatus::Open
}
pub fn order_status_filled() -> OrderStatus {
    OrderStatus::Filled
}
pub fn order_status_cancelled() -> OrderStatus {
    OrderStatus::Cancelled
}
pub fn order_status_rejected() -> OrderStatus {
    OrderStatus::Rejected
}
pub fn order_status_size_reduced() -> OrderStatus {
    OrderStatus::SizeReduced
}
pub fn order_status_acknowledged() -> OrderStatus {
    OrderStatus::Acknowledged
}

// ===================== CallbackResult =====================

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallbackResult<R: Clone> {
    NotAvailable,
    ContinueMatching { result: R },
    StopMatching { result: R },
}

pub fn extract_results<R: Clone>(cb: CallbackResult<R>) -> Option<R> {
    match cb {
        CallbackResult::NotAvailable => None,
        CallbackResult::ContinueMatching { result } => Some(result),
        CallbackResult::StopMatching { result } => Some(result),
    }
}

pub fn should_stop_matching<R: Clone>(cb: &CallbackResult<R>) -> bool {
    matches!(cb, CallbackResult::StopMatching { .. })
}

pub fn new_callback_result_continue_matching<R: Clone>(result: R) -> CallbackResult<R> {
    CallbackResult::ContinueMatching { result }
}

pub fn new_callback_result_stop_matching<R: Clone>(result: R) -> CallbackResult<R> {
    CallbackResult::StopMatching { result }
}

pub fn new_callback_result_not_available<R: Clone>() -> CallbackResult<R> {
    CallbackResult::NotAvailable
}

// ===================== SettleTradeResult =====================

#[derive(Clone, Debug)]
pub enum SettleTradeResult<R: Clone> {
    V1 {
        settled_size: u64,
        maker_cancellation_reason: Option<Vec<u8>>,
        taker_cancellation_reason: Option<Vec<u8>>,
        callback_result: CallbackResult<R>,
    },
}

pub fn new_settle_trade_result<R: Clone>(
    settled_size: u64,
    maker_cancellation_reason: Option<Vec<u8>>,
    taker_cancellation_reason: Option<Vec<u8>>,
    callback_result: CallbackResult<R>,
) -> SettleTradeResult<R> {
    SettleTradeResult::V1 {
        settled_size,
        maker_cancellation_reason,
        taker_cancellation_reason,
        callback_result,
    }
}

pub fn get_settled_size<R: Clone>(result: &SettleTradeResult<R>) -> u64 {
    let SettleTradeResult::V1 { settled_size, .. } = result;
    *settled_size
}

pub fn get_maker_cancellation_reason<R: Clone>(
    result: &SettleTradeResult<R>,
) -> Option<Vec<u8>> {
    let SettleTradeResult::V1 {
        maker_cancellation_reason,
        ..
    } = result;
    maker_cancellation_reason.clone()
}

pub fn get_taker_cancellation_reason<R: Clone>(
    result: &SettleTradeResult<R>,
) -> Option<Vec<u8>> {
    let SettleTradeResult::V1 {
        taker_cancellation_reason,
        ..
    } = result;
    taker_cancellation_reason.clone()
}

pub fn get_callback_result<R: Clone>(result: &SettleTradeResult<R>) -> &CallbackResult<R> {
    let SettleTradeResult::V1 {
        callback_result, ..
    } = result;
    callback_result
}

// ===================== ValidationResult =====================

#[derive(Clone, Debug)]
pub enum ValidationResult {
    V1 { failure_reason: Option<Vec<u8>> },
}

pub fn new_validation_result(cancellation_reason: Option<Vec<u8>>) -> ValidationResult {
    ValidationResult::V1 {
        failure_reason: cancellation_reason,
    }
}

pub fn is_validation_result_valid(result: &ValidationResult) -> bool {
    let ValidationResult::V1 { failure_reason } = result;
    failure_reason.is_none()
}

pub fn get_validation_failure_reason(result: &ValidationResult) -> Option<Vec<u8>> {
    let ValidationResult::V1 { failure_reason } = result;
    failure_reason.clone()
}

// ===================== PlaceMakerOrderResult =====================

#[derive(Clone, Debug)]
pub enum PlaceMakerOrderResult<R: Clone> {
    V1 {
        cancellation_reason: Option<Vec<u8>>,
        action: Option<R>,
    },
}

pub fn new_place_maker_order_result<R: Clone>(
    cancellation_reason: Option<Vec<u8>>,
    actions: Option<R>,
) -> PlaceMakerOrderResult<R> {
    PlaceMakerOrderResult::V1 {
        cancellation_reason,
        action: actions,
    }
}

pub fn get_place_maker_order_actions<R: Clone>(result: &PlaceMakerOrderResult<R>) -> Option<R> {
    let PlaceMakerOrderResult::V1 { action, .. } = result;
    action.clone()
}

pub fn get_place_maker_order_cancellation_reason<R: Clone>(
    result: &PlaceMakerOrderResult<R>,
) -> Option<Vec<u8>> {
    let PlaceMakerOrderResult::V1 {
        cancellation_reason, ..
    } = result;
    cancellation_reason.clone()
}

// ===================== MarketClearinghouseCallbacks Trait =====================
// In Move, this uses closures. In Rust, we define a trait.

pub trait MarketClearinghouseCallbacks<M: Clone + Copy, R: Clone> {
    fn settle_trade(
        &self,
        market: &mut Market<M>,
        taker: MarketClearinghouseOrderInfo<M>,
        maker: MarketClearinghouseOrderInfo<M>,
        fill_id: u128,
        settled_price: u64,
        settled_size: u64,
    ) -> SettleTradeResult<R>;

    fn validate_order_placement(
        &self,
        order_info: MarketClearinghouseOrderInfo<M>,
        size: u64,
    ) -> ValidationResult;

    fn validate_bulk_order_placement(
        &self,
        account: [u8; 32],
        bids_prices: &[u64],
        bids_sizes: &[u64],
        asks_prices: &[u64],
        asks_sizes: &[u64],
        order_metadata: &M,
    ) -> ValidationResult;

    fn place_maker_order(
        &self,
        order_info: MarketClearinghouseOrderInfo<M>,
        size: u64,
    ) -> PlaceMakerOrderResult<R>;

    fn cleanup_order(
        &self,
        order_info: MarketClearinghouseOrderInfo<M>,
        cleanup_size: u64,
        is_taker: bool,
    );

    fn cleanup_bulk_order_at_price(
        &self,
        account: [u8; 32],
        order_id: OrderId,
        is_bid: bool,
        price: u64,
        cleanup_size: u64,
    );

    fn place_bulk_order(
        &self,
        account: [u8; 32],
        order_id: OrderId,
        bid_prices: &[u64],
        bid_sizes: &[u64],
        ask_prices: &[u64],
        ask_sizes: &[u64],
        cancelled_bid_prices: &[u64],
        cancelled_bid_sizes: &[u64],
        cancelled_ask_prices: &[u64],
        cancelled_ask_sizes: &[u64],
        metadata: &M,
    );

    fn decrease_order_size(
        &self,
        order_info: MarketClearinghouseOrderInfo<M>,
        new_size: u64,
    );

    fn get_order_metadata_bytes(&self, order_metadata: &M) -> Vec<u8>;
}

// ===================== MarketConfig =====================

#[derive(Clone, Debug)]
pub enum MarketConfig {
    V1 {
        allow_self_trade: bool,
        allow_events_emission: bool,
        pre_cancellation_window_secs: u64,
        enable_dead_mans_switch: bool,
        min_keep_alive_time_secs: u64,
    },
}

pub fn new_market_config(
    allow_self_matching: bool,
    allow_events_emission: bool,
    pre_cancellation_window_secs: u64,
    enable_dead_mans_switch: bool,
    min_keep_alive_time_secs: u64,
) -> MarketConfig {
    MarketConfig::V1 {
        allow_self_trade: allow_self_matching,
        allow_events_emission,
        pre_cancellation_window_secs,
        enable_dead_mans_switch,
        min_keep_alive_time_secs,
    }
}

// ===================== Market =====================
// In Move, Market contains an ExtendRef for secondary resources (PreCancellationTracker,
// DeadMansSwitchTracker). In native Rust, we provide these as separate fields since
// there's no on-chain resource storage.

#[derive(Clone, Debug)]
pub enum Market<M: Clone> {
    V1 {
        parent: [u8; 32],
        market: [u8; 32],
        config: MarketConfig,
        order_book: OrderBook<M>,
        // In native Rust, secondary resources are held inline rather than behind ExtendRef
        // These would be provided by the caller in the full implementation
    },
}

pub fn new_market<M: Clone>(
    parent: [u8; 32],
    market_addr: [u8; 32],
    config: MarketConfig,
) -> Market<M> {
    Market::V1 {
        parent,
        market: market_addr,
        config,
        order_book: order_book::new_order_book(),
    }
}

// ===================== Market Getters =====================

pub fn get_order_book<M: Clone>(market: &Market<M>) -> &OrderBook<M> {
    let Market::V1 { order_book, .. } = market;
    order_book
}

pub fn get_order_book_mut<M: Clone>(market: &mut Market<M>) -> &mut OrderBook<M> {
    let Market::V1 { order_book, .. } = market;
    order_book
}

pub fn get_parent<M: Clone>(market: &Market<M>) -> [u8; 32] {
    let Market::V1 { parent, .. } = market;
    *parent
}

pub fn get_market_addr<M: Clone>(market: &Market<M>) -> [u8; 32] {
    let Market::V1 { market: m, .. } = market;
    *m
}

pub fn is_allowed_self_trade<M: Clone>(market: &Market<M>) -> bool {
    let Market::V1 { config, .. } = market;
    let MarketConfig::V1 {
        allow_self_trade, ..
    } = config;
    *allow_self_trade
}

pub fn is_dead_mans_switch_enabled<M: Clone>(market: &Market<M>) -> bool {
    let Market::V1 { config, .. } = market;
    let MarketConfig::V1 {
        enable_dead_mans_switch,
        ..
    } = config;
    *enable_dead_mans_switch
}

pub fn is_events_emission_enabled<M: Clone>(market: &Market<M>) -> bool {
    let Market::V1 { config, .. } = market;
    let MarketConfig::V1 {
        allow_events_emission,
        ..
    } = config;
    *allow_events_emission
}

pub fn set_allow_self_trade<M: Clone>(market: &mut Market<M>, allow: bool) {
    let Market::V1 { config, .. } = market;
    match config {
        MarketConfig::V1 {
            allow_self_trade, ..
        } => *allow_self_trade = allow,
    }
}

pub fn set_allow_events_emission<M: Clone>(market: &mut Market<M>, allow: bool) {
    let Market::V1 { config, .. } = market;
    match config {
        MarketConfig::V1 {
            allow_events_emission,
            ..
        } => *allow_events_emission = allow,
    }
}

pub fn set_allow_dead_mans_switch<M: Clone>(market: &mut Market<M>, enable: bool) {
    let Market::V1 { config, .. } = market;
    match config {
        MarketConfig::V1 {
            enable_dead_mans_switch,
            ..
        } => *enable_dead_mans_switch = enable,
    }
}

// ===================== Market Delegation APIs =====================

pub fn best_bid_price<M: Clone>(market: &Market<M>) -> Option<u64> {
    order_book::best_bid_price(get_order_book(market))
}

pub fn best_ask_price<M: Clone>(market: &Market<M>) -> Option<u64> {
    order_book::best_ask_price(get_order_book(market))
}

pub fn market_is_taker_order<M: Clone>(
    market: &Market<M>,
    price: u64,
    is_bid_side: bool,
    trigger_condition: Option<TriggerCondition>,
) -> bool {
    order_book::is_taker_order(get_order_book(market), price, is_bid_side, trigger_condition)
}

pub fn get_remaining_size<M: Clone>(market: &Market<M>, order_id: OrderId) -> u64 {
    order_book::get_single_remaining_size(get_order_book(market), order_id)
}

pub fn get_bulk_order_remaining_size<M: Clone>(
    market: &Market<M>,
    user: [u8; 32],
    is_bid_side: bool,
) -> u64 {
    order_book::get_bulk_order_remaining_size(get_order_book(market), user, is_bid_side)
}

pub fn get_order_metadata<M: Clone + Copy>(
    market: &Market<M>,
    order_id: OrderId,
) -> Option<M> {
    order_book::get_single_order_metadata(get_order_book(market), order_id)
}

pub fn set_order_metadata<M: Clone + Copy>(
    market: &mut Market<M>,
    order_id: OrderId,
    metadata: M,
) {
    order_book::set_single_order_metadata(get_order_book_mut(market), order_id, metadata);
}

pub fn take_ready_price_based_orders<M: Clone + Copy>(
    market: &mut Market<M>,
    oracle_price: u64,
    order_limit: u64,
) -> Vec<SingleOrder<M>> {
    order_book::take_ready_price_based_orders(get_order_book_mut(market), oracle_price, order_limit)
}

pub fn take_ready_time_based_orders<M: Clone + Copy>(
    market: &mut Market<M>,
    order_limit: u64,
    current_time_secs: u64,
) -> Vec<SingleOrder<M>> {
    order_book::take_ready_time_based_orders(
        get_order_book_mut(market),
        order_limit,
        current_time_secs,
    )
}

pub fn get_bulk_order<M: Clone>(market: &Market<M>, account: [u8; 32]) -> BulkOrder<M> {
    order_book::get_bulk_order(get_order_book(market), account)
}

// ===================== Event Emission (Stub) =====================
// In Move, events are emitted via `event::emit`. In native Rust, we provide
// stub functions that could be connected to an event system.
// These are no-ops that check allow_events_emission.

// Events are emitted by the caller in the native context.
// The functions below are placeholders matching the Move API signatures.
// In a full implementation, they would push events into an event queue.
