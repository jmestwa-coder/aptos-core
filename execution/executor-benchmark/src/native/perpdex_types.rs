// Copyright (c) Aptos Foundation
// Licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE

//! BCS-compatible struct mirrors for the perp DEX Move types.
//! These must serialize/deserialize identically to their Move counterparts.
//! Move enums use a variant index prefix byte (ULEB128, but for small indices it's just one byte).
//! Move structs serialize fields in declaration order. Field names do not matter for BCS.

use aptos_types::account_address::AccountAddress;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::marker::PhantomData;

// ============================================================================
// Fundamental types (from aptos_trading::order_book_types)
// ============================================================================

/// Move: `struct OrderId has store, copy, drop { order_id: u128 }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrderId {
    pub order_id: u128,
}

/// Move: `struct AccountClientOrderId has store, copy, drop { account: address, client_order_id: String }`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct AccountClientOrderId {
    pub account: AccountAddress,
    pub client_order_id: String,
}

/// Move: `struct IncreasingIdx has store, copy, drop { idx: u128 }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct IncreasingIdx {
    pub idx: u128,
}

/// Move: `struct DecreasingIdx has store, copy, drop { idx: u128 }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct DecreasingIdx {
    pub idx: u128,
}

/// Move: `struct OrderType has store, drop, copy { type: u16 }`
/// Note: `type` is a Rust keyword; BCS does not use field names so any name works.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct OrderType {
    pub order_type: u16,
}

/// Move: `enum TimeInForce has drop, copy, store { GTC, POST_ONLY, IOC }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum TimeInForce {
    GTC,
    POST_ONLY,
    IOC,
}

/// Move: `enum TriggerCondition has store, drop, copy { PriceMoveAbove(u64), PriceMoveBelow(u64), TimeBased(u64) }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TriggerCondition {
    PriceMoveAbove(u64),
    PriceMoveBelow(u64),
    TimeBased(u64),
}

// ============================================================================
// BigOrderedMap types (from aptos-framework)
// ============================================================================

/// Move: `enum BigOrderedMap<K, V> has store { BPlusTreeMap { ... } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "K: Serialize + DeserializeOwned, V: Serialize + DeserializeOwned")]
pub enum BigOrderedMap<K, V> {
    BPlusTreeMap {
        root: Node<K, V>,
        nodes: StorageSlotsAllocator<Node<K, V>>,
        min_leaf_index: u64,
        max_leaf_index: u64,
        constant_kv_size: bool,
        inner_max_degree: u16,
        leaf_max_degree: u16,
    },
}

/// Move: `enum Node<K, V> has store { V1 { is_leaf: bool, children: OrderedMap<K, Child<V>>, prev: u64, next: u64 } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "K: Serialize + DeserializeOwned, V: Serialize + DeserializeOwned")]
pub enum Node<K, V> {
    V1 {
        is_leaf: bool,
        children: OrderedMap<K, Child<V>>,
        prev: u64,
        next: u64,
    },
}

/// Move: `enum Child<V> has store { Inner { node_index: StoredSlot }, Leaf { value: V } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "V: Serialize + DeserializeOwned")]
pub enum Child<V> {
    Inner { node_index: StoredSlot },
    Leaf { value: V },
}

/// Move: `enum OrderedMap<K, V> has drop, copy, store { SortedVectorMap { entries: vector<Entry<K, V>> } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "K: Serialize + DeserializeOwned, V: Serialize + DeserializeOwned")]
pub enum OrderedMap<K, V> {
    SortedVectorMap { entries: Vec<Entry<K, V>> },
}

/// Move: `struct Entry<K, V> has drop, copy, store { key: K, value: V }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "K: Serialize + DeserializeOwned, V: Serialize + DeserializeOwned")]
pub struct Entry<K, V> {
    pub key: K,
    pub value: V,
}

/// Move: `enum StorageSlotsAllocator<T: store> has store { V1 { slots: Option<TableWithLength<u64, Link<T>>>, new_slot_index: u64, should_reuse: bool, reuse_head_index: u64, reuse_spare_count: u32 } }`
/// CRITICAL: This is an ENUM (not a struct) with a V1 variant.
/// The type parameter `T` is erased in BCS (the table stores values by handle),
/// but we keep it for type safety. PhantomData is skipped during serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageSlotsAllocator<T> {
    V1 {
        slots: Option<TableWithLength>,
        new_slot_index: u64,
        should_reuse: bool,
        reuse_head_index: u64,
        reuse_spare_count: u32,
        #[serde(skip)]
        _phantom: PhantomData<T>,
    },
}

/// Move `TableWithLength<K, V>` = `{ inner: Table<K, V>, length: u64 }`.
/// `Table<K, V>` serializes as just an address (the handle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableWithLength {
    pub inner: TableHandle,
    pub length: u64,
}

/// Move `Table<K, V>` is just a handle (address) in BCS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableHandle {
    pub handle: AccountAddress,
}

/// Move: `struct StoredSlot has store { slot_index: u64 }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSlot {
    pub slot_index: u64,
}

/// Move: `enum Link<T> has store { Occupied { value: T }, Vacant { next: u64 } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "T: Serialize + DeserializeOwned")]
pub enum Link<T> {
    Occupied { value: T },
    Vacant { next: u64 },
}

// ============================================================================
// Price types (stored in ObjectGroup at market address)
// ============================================================================

/// Move: `enum PriceDetails has key, store, drop { V1 { price_config, price_history, price_state, funding_rate_history } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriceDetails {
    V1 {
        price_config: PriceConfig,
        price_history: PriceHistory,
        price_state: PriceState,
        funding_rate_history: FundingRateHistory,
    },
}

/// Move: `enum PriceConfig has store, drop { V1 { size_multiplier: u64, unrealized_pnl_haircut_bps: u64, withdrawable_margin_leverage: u8, max_leverage: u8 } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriceConfig {
    V1 {
        size_multiplier: u64,
        unrealized_pnl_haircut_bps: u64,
        withdrawable_margin_leverage: u8,
        max_leverage: u8,
    },
}

/// Move: `enum PriceHistory has store, drop { V1 { ... } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriceHistory {
    V1 {
        last_oracle_update_us: u64,
        oracle_px: u64,
        mark_prices: Vec<u64>,
        book_mid_px: u64,
        book_mid_30_ema: MovingAverage,
        ratio_mid_vs_oracle_150_ema: DeviationMovingAverage,
        book_oracle_ratio_cap_bps: u64,
    },
}

/// Move: `enum PriceState has store, drop { V1 { short_mark_px: u64, long_mark_px: u64, accumulative_index: AccumulativeIndex } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriceState {
    V1 {
        short_mark_px: u64,
        long_mark_px: u64,
        accumulative_index: AccumulativeIndex,
    },
}

/// Move: `enum FundingRateHistory has store, drop { V1 { ... } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FundingRateHistory {
    V1 {
        funding_rate_pause_timeout_us: u64,
        last_funding_calculated_us: u64,
        instant_rate_adjustment: FundingInstantRateAdjustment,
        charging_mode: FundingChargingMode,
    },
}

/// Move: `enum FundingChargingMode { ContinuousV1, PeriodicV1 { ... } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FundingChargingMode {
    ContinuousV1,
    PeriodicV1 {
        outstanding_funding_index: AccumulativeIndex,
        last_funding_charged_us: u64,
        funding_period_us: u64,
    },
}

/// Move: `enum FundingInstantRateAdjustment { INSTANT }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum FundingInstantRateAdjustment {
    INSTANT,
}

/// Move: `struct AccumulativeIndex has store, copy, drop { index: i128 }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AccumulativeIndex {
    pub index: i128,
}

/// Move: `enum MovingAverage { EMA { ema: u64, lookback_window_seconds: u64, last_observation_time_us: u64, observation_count: u64 } }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MovingAverage {
    EMA {
        ema: u64,
        lookback_window_seconds: u64,
        last_observation_time_us: u64,
        observation_count: u64,
    },
}

/// Move: `enum DeviationMovingAverage { Ratio { ratio_moving_average: MovingAverage } }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DeviationMovingAverage {
    Ratio {
        ratio_moving_average: MovingAverage,
    },
}

// ============================================================================
// PerpMarket (stored as regular resource at market address)
// ============================================================================

/// Move: `enum PerpMarket has key { V1 { market: Market<OrderMetadata> } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerpMarket {
    V1 {
        market: Market,
    },
}

/// Move: `enum Market<M> has store { V1 { parent: address, market: address, config: MarketConfig, order_book: OrderBook<M>, secondary_resources_ref: ExtendRef } }`
/// Specialized for `M = OrderMetadata`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Market {
    V1 {
        parent: AccountAddress,
        market: AccountAddress,
        config: MarketConfig,
        order_book: OrderBook,
        secondary_resources_ref: ExtendRef,
    },
}

/// Move: `struct ExtendRef has drop, store { self: address }`
/// Note: `self` is a Rust keyword; BCS does not use field names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendRef {
    pub self_address: AccountAddress,
}

/// Move: `enum MarketConfig has store, drop { V1 { allow_self_trade: bool, allow_events_emission: bool, pre_cancellation_window_secs: u64, enable_dead_mans_switch: bool, min_keep_alive_time_secs: u64 } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketConfig {
    V1 {
        allow_self_trade: bool,
        allow_events_emission: bool,
        pre_cancellation_window_secs: u64,
        enable_dead_mans_switch: bool,
        min_keep_alive_time_secs: u64,
    },
}

/// Move: `enum OrderBook<M> has store { UnifiedV1 { single_order_book: SingleOrderBook<M>, bulk_order_book: BulkOrderBook<M>, price_time_idx: PriceTimeIndex } }`
/// Specialized for `M = OrderMetadata`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderBook {
    UnifiedV1 {
        single_order_book: SingleOrderBook,
        bulk_order_book: BulkOrderBook,
        price_time_idx: PriceTimeIndex,
    },
}

// ============================================================================
// SingleOrderBook types
// ============================================================================

/// Move: `enum SingleOrderBook<M> has store { V1 { orders: BigOrderedMap<OrderId, OrderWithState<M>>, client_order_ids: BigOrderedMap<AccountClientOrderId, OrderId>, pending_orders: PendingOrderBookIndex } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SingleOrderBook {
    V1 {
        orders: BigOrderedMap<OrderId, OrderWithState>,
        client_order_ids: BigOrderedMap<AccountClientOrderId, OrderId>,
        pending_orders: PendingOrderBookIndex,
    },
}

/// Move: `enum OrderWithState<M> has store, drop, copy { V1 { order: SingleOrder<M>, is_active: bool } }`
/// NOTE: Cannot derive Copy in Rust because inner types contain String.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderWithState {
    V1 {
        order: SingleOrder,
        is_active: bool,
    },
}

/// Move: `enum SingleOrder<M> has store, copy, drop { V1 { order_request: SingleOrderRequest<M>, unique_priority_idx: IncreasingIdx } }`
/// NOTE: Cannot derive Copy in Rust because inner types contain String.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SingleOrder {
    V1 {
        order_request: SingleOrderRequest,
        unique_priority_idx: IncreasingIdx,
    },
}

/// Move: `enum SingleOrderRequest<M> has store, copy, drop { V1 { account: address, order_id: OrderId, client_order_id: Option<String>, price: u64, orig_size: u64, remaining_size: u64, is_bid: bool, trigger_condition: Option<TriggerCondition>, time_in_force: TimeInForce, creation_time_micros: u64, metadata: M } }`
/// Specialized for `M = OrderMetadata`.
/// NOTE: Cannot derive Copy because of String and Option<String> fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SingleOrderRequest {
    V1 {
        account: AccountAddress,
        order_id: OrderId,
        client_order_id: Option<String>,
        price: u64,
        orig_size: u64,
        remaining_size: u64,
        is_bid: bool,
        trigger_condition: Option<TriggerCondition>,
        time_in_force: TimeInForce,
        creation_time_micros: u64,
        metadata: OrderMetadata,
    },
}

// ============================================================================
// PendingOrderBookIndex types
// ============================================================================

/// Move: `enum PendingOrderBookIndex has store { V1 { price_move_down_index, price_move_up_index, time_based_index } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PendingOrderBookIndex {
    V1 {
        price_move_down_index: BigOrderedMap<PendingDownOrderKey, OrderId>,
        price_move_up_index: BigOrderedMap<PendingUpOrderKey, OrderId>,
        time_based_index: BigOrderedMap<PendingTimeKey, OrderId>,
    },
}

/// Move: `struct PendingDownOrderKey has store, copy, drop { price: u64, tie_breaker: DecreasingIdx }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PendingDownOrderKey {
    pub price: u64,
    pub tie_breaker: DecreasingIdx,
}

/// Move: `struct PendingUpOrderKey has store, copy, drop { price: u64, tie_breaker: IncreasingIdx }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PendingUpOrderKey {
    pub price: u64,
    pub tie_breaker: IncreasingIdx,
}

/// Move: `struct PendingTimeKey has store, copy, drop { time: u64, tie_breaker: IncreasingIdx }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PendingTimeKey {
    pub time: u64,
    pub tie_breaker: IncreasingIdx,
}

// ============================================================================
// BulkOrderBook types
// ============================================================================

/// Move: `enum BulkOrderBook<M> has store { V1 { orders: BigOrderedMap<address, BulkOrder<M>>, order_id_to_address: BigOrderedMap<OrderId, address> } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BulkOrderBook {
    V1 {
        orders: BigOrderedMap<AccountAddress, BulkOrder>,
        order_id_to_address: BigOrderedMap<OrderId, AccountAddress>,
    },
}

/// Move: `enum BulkOrder<M> has store, copy, drop { V1 { order_request: BulkOrderRequest<M>, order_id: OrderId, unique_priority_idx: IncreasingIdx, creation_time_micros: u64 } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BulkOrder {
    V1 {
        order_request: BulkOrderRequest,
        order_id: OrderId,
        unique_priority_idx: IncreasingIdx,
        creation_time_micros: u64,
    },
}

/// Move: `enum BulkOrderRequest<M> has store, copy, drop { V1 { account: address, order_sequence_number: u64, bid_prices: vector<u64>, bid_sizes: vector<u64>, ask_prices: vector<u64>, ask_sizes: vector<u64>, metadata: M } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BulkOrderRequest {
    V1 {
        account: AccountAddress,
        order_sequence_number: u64,
        bid_prices: Vec<u64>,
        bid_sizes: Vec<u64>,
        ask_prices: Vec<u64>,
        ask_sizes: Vec<u64>,
        metadata: OrderMetadata,
    },
}

// ============================================================================
// PriceTimeIndex types
// ============================================================================

/// Move: `enum PriceTimeIndex has store { V1 { buys: BigOrderedMap<PriceDescTime, OrderData>, sells: BigOrderedMap<PriceAscTime, OrderData> } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriceTimeIndex {
    V1 {
        buys: BigOrderedMap<PriceDescTime, OrderData>,
        sells: BigOrderedMap<PriceAscTime, OrderData>,
    },
}

/// Move: `struct PriceAscTime has store, copy, drop { price: u64, tie_breaker: IncreasingIdx }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PriceAscTime {
    pub price: u64,
    pub tie_breaker: IncreasingIdx,
}

/// Move: `struct PriceDescTime has store, copy, drop { price: u64, tie_breaker: DecreasingIdx }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PriceDescTime {
    pub price: u64,
    pub tie_breaker: DecreasingIdx,
}

/// Move: `struct OrderData has store, copy, drop { order_id: OrderId, order_book_type: OrderType, size: u64 }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct OrderData {
    pub order_id: OrderId,
    pub order_book_type: OrderType,
    pub size: u64,
}

// ============================================================================
// AsyncMatchingEngine (stored as regular resource at market address)
// ============================================================================

/// Move: `enum AsyncMatchingEngine has key { V1 { pending_requests, async_matching_enabled, backstop_liquidations_in_queue, margin_call_liquidations_in_queue, mark_prices_in_queue } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AsyncMatchingEngine {
    V1 {
        pending_requests: BigOrderedMap<PendingRequestKey, PendingRequest>,
        async_matching_enabled: bool,
        backstop_liquidations_in_queue: BigOrderedMap<AccountAddress, bool>,
        margin_call_liquidations_in_queue: BigOrderedMap<AccountAddress, bool>,
        mark_prices_in_queue: Vec<u64>,
    },
}

/// Move: `enum PendingRequestKey has store, copy, drop { V1 { time: u64, priority: u8, tie_breaker: u128 } }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum PendingRequestKey {
    V1 {
        time: u64,
        priority: u8,
        tie_breaker: u128,
    },
}

// ============================================================================
// PendingRequest and its variants
// ============================================================================

/// Move: `enum PendingRequest has store { Order(PendingOrder), Twap(PendingTwap), ContinuedOrder(ContinuedPendingOrder), BackstopLiquidation { ... }, MarginCall { ... }, CheckADL { ... }, TriggerADL { ... }, CommitMarkPrice { ... }, QueueBackstopLiquidationsAndADL { ... }, QueueMarginCallLiquidations { ... } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PendingRequest {
    Order(PendingOrder),
    Twap(PendingTwap),
    ContinuedOrder(ContinuedPendingOrder),
    BackstopLiquidation {
        user: AccountAddress,
        batch_key: u128,
    },
    MarginCall {
        user: AccountAddress,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PendingOrder {
    V1 {
        order_args: PerpOrderRequestExtendedArgs,
        order_metadata: OrderMetadata,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContinuedPendingOrder {
    V1 {
        order_args: PerpOrderRequestExtendedArgs,
        order_metadata: OrderMetadata,
        remaining_size: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PendingTwap {
    V1 {
        account: AccountAddress,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarginCallContinuation {
    Start,
    ReduceOnly {
        remaining_size: u64,
    },
    Liquidate {
        remaining_size: u64,
        liquidation_price: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueBackstopLiquidationsAndADLPayload {
    pub backstop_liquidations: Vec<AccountAddress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMarginCallLiquidationsPayload {
    pub margin_call_liquidations: Vec<AccountAddress>,
}

// ============================================================================
// Order request types
// ============================================================================

/// Move: `enum PerpOrderRequestCommonArgs has store, copy, drop { V1 { price: u64, orig_size: u64, is_buy: bool, time_in_force: TimeInForce, client_order_id: Option<String> } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerpOrderRequestCommonArgs {
    V1 {
        price: u64,
        orig_size: u64,
        is_buy: bool,
        time_in_force: TimeInForce,
        client_order_id: Option<String>,
    },
}

/// Move: `enum PerpOrderRequestExtendedArgs has store, copy, drop { V1 { account: address, common_args: PerpOrderRequestCommonArgs, order_id: OrderId, trigger_condition: Option<TriggerCondition> } }`
/// NOTE: The correct layout has a nested PerpOrderRequestCommonArgs enum, NOT flat fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerpOrderRequestExtendedArgs {
    V1 {
        account: AccountAddress,
        common_args: PerpOrderRequestCommonArgs,
        order_id: OrderId,
        trigger_condition: Option<TriggerCondition>,
    },
}

/// Move: `enum PerpOrderRequestTpSlArgs has store, copy, drop { V1 { tp_trigger_price: Option<u64>, tp_limit_price: Option<u64>, sl_trigger_price: Option<u64>, sl_limit_price: Option<u64> } }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PerpOrderRequestTpSlArgs {
    V1 {
        tp_trigger_price: Option<u64>,
        tp_limit_price: Option<u64>,
        sl_trigger_price: Option<u64>,
        sl_limit_price: Option<u64>,
    },
}

// ============================================================================
// OrderMetadata (perp-specific)
// ============================================================================

/// Move: `enum OrderMetadata has store, copy, drop { V1_RETAIL { ... }, V1_BULK { ... } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum OrderMetadata {
    V1_RETAIL {
        is_reduce_only: bool,
        use_backstop_liquidation_margin: bool,
        is_margin_call: bool,
        twap: Option<TwapMetadata>,
        tp_sl: TpSlMetadata,
        builder_code: Option<BuilderCode>,
    },
    V1_BULK {
        builder_code: Option<BuilderCode>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TpSlMetadata {
    V1 {
        tp: Option<ChildTpSlOrder>,
        sl: Option<ChildTpSlOrder>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TwapMetadata {
    V1 {
        start_time_seconds: u64,
        frequency_seconds: u64,
        end_time_seconds: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChildTpSlOrder {
    V1 {
        trigger_price: u64,
        parent_order_id: OrderId,
        limit_price: Option<u64>,
    },
}

/// Move: `enum BuilderCode has store, copy, drop { V1 { builder_address: address, builder_fee: u64 } }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BuilderCode {
    V1 {
        builder_address: AccountAddress,
        builder_fee: u64,
    },
}

// ============================================================================
// PerpMarketOracleSource (in ObjectGroup at market address)
// ============================================================================

/// Move: `enum PerpMarketOracleSource has key { V1 { oracle_source: OracleSource } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerpMarketOracleSource {
    V1 {
        oracle_source: OracleSource,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OracleSource {
    Single {
        primary: SingleOracleSource,
    },
    Composite {
        primary: SingleOracleSource,
        secondary: SingleOracleSource,
        oracles_deviation_bps: u64,
        consecutive_deviation_count: u8,
        last_primary_price: u64,
        current_deviation_count: u8,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SingleOracleSource {
    Internal(InternalSource),
    Pyth(PythSource),
    Chainlink(ChainlinkSource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InternalSource {
    V1 {
        source_id: InternalSourceIdentifier,
        max_staleness_secs: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InternalSourceIdentifier {
    V1 {
        object_address: AccountAddress,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PythSource {
    V1 {
        price_identifier: PriceIdentifier,
        max_staleness_secs: u64,
        confidence_interval_threshold: u64,
        rescale_decimals: i8,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceIdentifier {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChainlinkSource {
    V1 {
        feed_id: Vec<u8>,
        max_staleness_secs: u64,
        rescale_decimals: i8,
    },
}

// ============================================================================
// PerpMarketConfiguration (in ObjectGroup at market address)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerpMarketConfiguration {
    V1 {
        market_name: String,
        sz_decimals: u8,
        tick_size: u64,
        lot_size: u64,
        min_size: u64,
        oracle_source: OracleSource,
        taker_in_next_block: bool,
    },
}

// ============================================================================
// Global (stored as regular resource at publisher address)
// ============================================================================

/// Move: `enum Global has key { V1 { extend_ref: ExtendRef, market_refs: BigOrderedMap<Object<PerpMarket>, ExtendRef>, is_exchange_open: bool } }`
/// We only need to read `is_exchange_open` for the Block-STM dependency.
/// `Object<PerpMarket>` serializes as just an address in BCS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Global {
    V1 {
        extend_ref: ExtendRef,
        market_refs: BigOrderedMap<AccountAddress, ExtendRef>,
        is_exchange_open: bool,
    },
}

// ============================================================================
// PriceIndexStore (stored as regular resource at publisher address)
// ============================================================================

/// Move: `enum PriceIndexStore has key, drop { V1 { interest_rate: u64 }, V2 { daily_interest_rate, daily_premium_rate, daily_rate_at_zero_diff, max_rate_as_fraction_of_initial_margin } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriceIndexStore {
    V1 {
        interest_rate: u64,
    },
    V2 {
        daily_interest_rate: u64,
        daily_premium_rate: u64,
        daily_rate_at_zero_diff: u64,
        max_rate_as_fraction_of_initial_margin: u64,
    },
}
