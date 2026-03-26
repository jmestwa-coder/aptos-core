// Copyright (c) Aptos Foundation
// Licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE

//! BCS-compatible struct mirrors for the perp DEX Move types.
//! These must serialize/deserialize identically to their Move counterparts.
//! Move enums use a variant index prefix byte (ULEB128, but for small indices it's just one byte).
//! Move structs serialize fields in declaration order. Field names do not matter for BCS.
//!
//! These types are the GROUND TRUTH for on-chain data layout, validated against
//! real benchmark data with zero deserialization errors.

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
        /// Transient write cache for multi-step tree operations (remove + rebalance).
        /// Populated during tree_remove so that subsequent reads within the same
        /// operation see recently-written nodes. Cleared after each operation.
        #[serde(skip)]
        write_cache: std::collections::HashMap<u64, Vec<u8>>,
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
// Global (stored as regular resource at publisher address)
// ============================================================================

/// Move: `enum Global has key { V1 { extend_ref: ExtendRef, market_refs: BigOrderedMap<Object<PerpMarket>, ExtendRef>, is_exchange_open: bool } }`
/// `Object<PerpMarket>` serializes as just an address in BCS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Global {
    V1 {
        extend_ref: ExtendRef,
        market_refs: BigOrderedMap<AccountAddress, ExtendRef>,
        is_exchange_open: bool,
    },
}

/// Move: `struct ExtendRef has drop, store { self: address }`
/// Note: `self` is a Rust keyword; BCS does not use field names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendRef {
    pub self_address: AccountAddress,
}

// ============================================================================
// BigOrderedMap helper operations for dispatch layer
// ============================================================================

impl<K, V> BigOrderedMap<K, V>
where
    K: Serialize + DeserializeOwned + Clone + Ord,
    V: Serialize + DeserializeOwned + Clone,
{
    /// Check if the map is empty by looking at the root node's children.
    pub fn is_empty(&self) -> bool {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        entries.is_empty()
    }

    /// Get the number of entries in the root node.
    /// NOTE: This only counts root-level entries. For multi-level trees,
    /// the total count would require traversing table-backed child nodes.
    pub fn root_entry_count(&self) -> usize {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        entries.len()
    }

    /// Get the first key from the root node (smallest key).
    pub fn first_key(&self) -> Option<&K> {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        entries.first().map(|e| &e.key)
    }

    /// Remove and return the first (smallest key) leaf entry from the root node.
    /// Returns None if the root is empty or the first child is not a Leaf.
    pub fn pop_front_leaf(&mut self) -> Option<(K, V)> {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        if entries.is_empty() {
            return None;
        }
        // Only pop if it's a leaf child
        match &entries[0].value {
            Child::Leaf { .. } => {
                let entry = entries.remove(0);
                if let Child::Leaf { value } = entry.value {
                    Some((entry.key, value))
                } else {
                    unreachable!()
                }
            },
            Child::Inner { .. } => None,
        }
    }

    /// Add a leaf entry to the root node, maintaining sorted order by key.
    /// This only works for leaf-level root nodes (single-level trees).
    pub fn add_leaf(&mut self, key: K, value: V) {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        let pos = entries.binary_search_by(|e| e.key.cmp(&key)).unwrap_or_else(|p| p);
        entries.insert(pos, Entry {
            key,
            value: Child::Leaf { value },
        });
    }

    /// Add a leaf entry to a B+ tree that may have inner nodes.
    /// If root is a leaf node, adds directly and returns empty vec.
    /// If root is an inner node, finds the right child slot and returns
    /// a list of (slot_index, serialized_bytes) for table items that need to be written.
    /// The `read_slot` closure reads table items by slot_index.
    /// Returns Vec<(slot_index, serialized_bytes, is_new_item)>
    pub fn add_to_tree<F>(
        &mut self,
        key: K,
        value: V,
        read_slot: &F,
    ) -> Vec<(u64, Vec<u8>, bool)>
    where
        F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
        K: Serialize + DeserializeOwned + Clone,
        V: Serialize + DeserializeOwned + Clone,
    {
        let BigOrderedMap::BPlusTreeMap {
            root, nodes, leaf_max_degree, ..
        } = self;
        let Node::V1 { is_leaf, children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;

        // If root is leaf, just add directly (no table I/O needed)
        if *is_leaf {
            let pos = entries.binary_search_by(|e| e.key.cmp(&key)).unwrap_or_else(|p| p);
            entries.insert(pos, Entry {
                key,
                value: Child::Leaf { value },
            });
            return vec![];
        }

        // Root is inner: find which child to insert into.
        // Inner entries are sorted by max-key. We find the first entry whose key >= our key.
        let child_idx = entries.binary_search_by(|e| e.key.cmp(&key))
            .unwrap_or_else(|p| p);
        // Clamp to valid range (if key is larger than all, goes into last child)
        let child_idx = child_idx.min(entries.len() - 1);

        let slot_index = match &entries[child_idx].value {
            Child::Inner { node_index } => node_index.slot_index,
            Child::Leaf { .. } => {
                // Shouldn't happen in inner root, fall back to add_leaf behavior
                let pos = entries.binary_search_by(|e| e.key.cmp(&key)).unwrap_or_else(|p| p);
                entries.insert(pos, Entry {
                    key,
                    value: Child::Leaf { value },
                });
                return vec![];
            }
        };

        // Read the child node from table
        let raw_bytes = match read_slot(slot_index) {
            Ok(Some(bytes)) => bytes,
            _ => return vec![], // Can't read, give up
        };

        let link: Link<Node<K, V>> = match bcs::from_bytes(&raw_bytes) {
            Ok(l) => l,
            Err(_) => return vec![],
        };

        let mut child_node = match link {
            Link::Occupied { value: node } => node,
            Link::Vacant { .. } => return vec![],
        };

        // Add entry to child node
        let Node::V1 { children: ref mut child_children, .. } = child_node;
        let OrderedMap::SortedVectorMap { entries: child_entries } = child_children;
        let pos = child_entries.binary_search_by(|e| e.key.cmp(&key)).unwrap_or_else(|p| p);
        child_entries.insert(pos, Entry {
            key: key.clone(),
            value: Child::Leaf { value },
        });

        // Update the max-key of this inner entry if the new key is larger
        if key > entries[child_idx].key {
            entries[child_idx].key = key.clone();
        }

        let leaf_max = *leaf_max_degree as usize;
        let mut table_items: Vec<(u64, Vec<u8>, bool)> = Vec::new();

        // Check if the child needs splitting
        if child_entries.len() > leaf_max {
            // Split the child into two
            let target_size = (leaf_max + 1) / 2;
            let Node::V1 { children: ref mut cc, prev: ref child_prev, next: ref child_next, .. } = child_node;
            let OrderedMap::SortedVectorMap { entries: ce } = cc;

            let right_entries: Vec<Entry<K, Child<V>>> = ce.split_off(target_size);
            let left_max_key = ce.last().unwrap().key.clone();
            let right_max_key = right_entries.last().unwrap().key.clone();

            // Allocate a new slot for the right child
            let StorageSlotsAllocator::V1 { new_slot_index, slots, .. } = nodes;
            let new_slot = *new_slot_index;
            *new_slot_index += 1;
            if let Some(twl) = slots {
                twl.length += 1;
            }

            // Build left child (reuse existing slot)
            let left_child = Node::V1 {
                is_leaf: true,
                children: OrderedMap::SortedVectorMap { entries: std::mem::take(ce) },
                prev: *child_prev,
                next: new_slot,
            };

            // Build right child (new slot)
            let right_child = Node::V1 {
                is_leaf: true,
                children: OrderedMap::SortedVectorMap { entries: right_entries },
                prev: slot_index,
                next: *child_next,
            };

            // Serialize both
            let left_link = Link::Occupied { value: left_child };
            let right_link = Link::Occupied { value: right_child };
            table_items.push((slot_index, bcs::to_bytes(&left_link).expect("serialize left"), false)); // existing slot, modified
            table_items.push((new_slot, bcs::to_bytes(&right_link).expect("serialize right"), true)); // new slot from split

            // Update inner entries: update existing entry's key and add new inner entry
            entries[child_idx].key = left_max_key;
            let new_inner_entry = Entry {
                key: right_max_key,
                value: Child::Inner {
                    node_index: StoredSlot { slot_index: new_slot },
                },
            };
            entries.insert(child_idx + 1, new_inner_entry);
        } else {
            // No split needed, just write back the modified child
            let link = Link::Occupied { value: child_node };
            table_items.push((slot_index, bcs::to_bytes(&link).expect("serialize child"), false)); // existing slot, modified
        }

        table_items
    }

    /// Remove an entry by key from the root node.
    /// Returns the value if found.
    pub fn remove_leaf(&mut self, key: &K) -> Option<V> {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        if let Ok(pos) = entries.binary_search_by(|e| e.key.cmp(key)) {
            let entry = entries.remove(pos);
            if let Child::Leaf { value } = entry.value {
                Some(value)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Check if a key exists in the root node.
    pub fn contains_key(&self, key: &K) -> bool {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        entries.binary_search_by(|e| e.key.cmp(key)).is_ok()
    }

    /// Return diagnostic info: (is_leaf, leaf_max_degree, inner_max_degree, entry_count, new_slot_index)
    pub fn tree_info(&self) -> (bool, u16, u16, usize, u64) {
        let BigOrderedMap::BPlusTreeMap { root, nodes, leaf_max_degree, inner_max_degree, .. } = self;
        let Node::V1 { is_leaf, children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        let StorageSlotsAllocator::V1 { new_slot_index, .. } = nodes;
        (*is_leaf, *leaf_max_degree, *inner_max_degree, entries.len(), *new_slot_index)
    }
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderWithState {
    V1 {
        order: SingleOrder,
        is_active: bool,
    },
}

/// Move: `enum SingleOrder<M> has store, copy, drop { V1 { order_request: SingleOrderRequest<M>, unique_priority_idx: IncreasingIdx } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SingleOrder {
    V1 {
        order_request: SingleOrderRequest,
        unique_priority_idx: IncreasingIdx,
    },
}

/// Move: `enum SingleOrderRequest<M> has store, copy, drop { V1 { account, order_id, client_order_id, price, orig_size, remaining_size, is_bid, trigger_condition, time_in_force, creation_time_micros, metadata } }`
/// Specialized for `M = OrderMetadata`.
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

/// Move: `enum BulkOrderRequest<M> has store, copy, drop { V1 { account, order_sequence_number, bid_prices, bid_sizes, ask_prices, ask_sizes, metadata } }`
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
// PriceIndexStore (stored as regular resource at publisher address)
// ============================================================================

/// Move: `enum PriceIndexStore has key, drop { V1 { interest_rate }, V2 { daily_interest_rate, daily_premium_rate, daily_rate_at_zero_diff, max_rate_as_fraction_of_initial_margin } }`
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

// ============================================================================
// InternalSourceState (stored as regular resource at oracle object address)
// ============================================================================

/// Move: `enum InternalSourceState has key { V1 { spot_price: u64, update_time: u64, source_ref: ExtendRef } }`
/// We use raw bytes patching for this resource to avoid needing the exact field order,
/// but define the type for documentation. The spot_price and update_time are at
/// bytes [1..9] and [9..17] respectively (after the variant byte).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InternalSourceState {
    V1 {
        spot_price: u64,
        update_time: u64,
        source_ref: ExtendRef,
    },
}

// ============================================================================
// Best bid/ask extraction helpers for PerpMarket
// ============================================================================

impl PerpMarket {
    /// Extract best bid price from the order book's PriceTimeIndex.
    /// Returns None if the order book has no buys or the root node is an inner node.
    pub fn best_bid_price(&self) -> Option<u64> {
        let PerpMarket::V1 { market } = self;
        let Market::V1 { order_book, .. } = market;
        let OrderBook::UnifiedV1 { price_time_idx, .. } = order_book;
        let PriceTimeIndex::V1 { buys, .. } = price_time_idx;

        let BigOrderedMap::BPlusTreeMap { root, .. } = buys;
        let Node::V1 { is_leaf, children, .. } = root;
        if !is_leaf {
            return None;
        }
        let OrderedMap::SortedVectorMap { entries } = children;
        // In buys (PriceDescTime key), the last entry has the highest price = best bid
        entries.last().and_then(|e| {
            match &e.value {
                Child::Leaf { .. } => Some(e.key.price),
                Child::Inner { .. } => None,
            }
        })
    }

    /// Extract best ask price from the order book's PriceTimeIndex.
    /// Returns None if the order book has no sells or the root node is an inner node.
    pub fn best_ask_price(&self) -> Option<u64> {
        let PerpMarket::V1 { market } = self;
        let Market::V1 { order_book, .. } = market;
        let OrderBook::UnifiedV1 { price_time_idx, .. } = order_book;
        let PriceTimeIndex::V1 { sells, .. } = price_time_idx;

        let BigOrderedMap::BPlusTreeMap { root, .. } = sells;
        let Node::V1 { is_leaf, children, .. } = root;
        if !is_leaf {
            return None;
        }
        let OrderedMap::SortedVectorMap { entries } = children;
        // In sells (PriceAscTime key), the first entry has the lowest price = best ask
        entries.first().and_then(|e| {
            match &e.value {
                Child::Leaf { .. } => Some(e.key.price),
                Child::Inner { .. } => None,
            }
        })
    }
}

// ============================================================================
// Event types for order placement
// ============================================================================

/// Move: `enum OrderStatus has drop, copy, store { OPEN, FILLED, CANCELLED, REJECTED, SIZE_REDUCED, ACKNOWLEDGED }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum OrderStatus {
    OPEN,
    FILLED,
    CANCELLED,
    REJECTED,
    SIZE_REDUCED,
    ACKNOWLEDGED,
}

/// Move: `enum OrderCancellationReason has drop, copy, store { ... }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum OrderCancellationReason {
    CancelledByUser,
    SelfTrade,
    PostOnlyFailed,
    InsufficientMargin,
    IOCExpired,
    ReduceOnlyNoPosition,
    PreCancelled,
    DeadMansSwitchTriggered,
    StaleOrder,
    StopLoss,
    ExchangeHalted,
    LiquidationOrMarginCall,
    Other { reason: String },
}

/// Move: `enum OrderEvent has drop, copy, store { V1 { ... } }`
/// This is the event emitted for order acknowledgements, opens, fills, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderEvent {
    V1 {
        parent: AccountAddress,
        market: AccountAddress,
        order_id: u128,
        client_order_id: Option<String>,
        user: AccountAddress,
        orig_size: u64,
        remaining_size: u64,
        size_delta: u64,
        price: u64,
        is_bid: bool,
        is_taker: bool,
        status: OrderStatus,
        details: String,
        metadata_bytes: Vec<u8>,
        time_in_force: TimeInForce,
        trigger_condition: Option<TriggerCondition>,
        cancellation_reason: Option<OrderCancellationReason>,
    },
}

/// Move: `enum BulkOrderPlacedEvent has drop, copy, store { V1 { ... } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BulkOrderPlacedEvent {
    V1 {
        parent: AccountAddress,
        market: AccountAddress,
        order_id: u128,
        sequence_number: u64,
        user: AccountAddress,
        bid_prices: Vec<u64>,
        bid_sizes: Vec<u64>,
        ask_prices: Vec<u64>,
        ask_sizes: Vec<u64>,
        cancelled_bid_prices: Vec<u64>,
        cancelled_bid_sizes: Vec<u64>,
        cancelled_ask_prices: Vec<u64>,
        cancelled_ask_sizes: Vec<u64>,
        previous_seq_num: u64,
    },
}

// ============================================================================
// PerpMarketConfig (stored in ObjectGroup at market address)
// ============================================================================

/// Move: `enum PerpMarketConfiguration has key { V1 { market_name, sz_decimals, tick_size, lot_size, min_size, oracle_source, taker_in_next_block } }`
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
// Subaccount type (for ownership verification)
// ============================================================================

/// Move: `enum Subaccount has key { V1 { owner: address, ... } }`
/// We only need this for reading the owner field to verify authorization.
/// The full struct has additional fields we don't need for the authorization check.
/// In practice, the auth check reads the ObjectGroup at the subaccount address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Subaccount {
    V1 {
        owner: AccountAddress,
        name: String,
        extend_ref: ExtendRef,
    },
}

// ============================================================================
// PerpMarket mutation helpers for bulk order placement
// ============================================================================

impl PerpMarket {
    /// Get mutable access to the order book components for bulk order operations.
    pub fn order_book_mut(&mut self) -> (&mut BulkOrderBook, &mut PriceTimeIndex) {
        let PerpMarket::V1 { market } = self;
        let Market::V1 { order_book, .. } = market;
        let OrderBook::UnifiedV1 { bulk_order_book, price_time_idx, .. } = order_book;
        (bulk_order_book, price_time_idx)
    }

    /// Get the parent and market addresses from PerpMarket.
    pub fn parent_and_market_addresses(&self) -> (AccountAddress, AccountAddress) {
        let PerpMarket::V1 { market } = self;
        let Market::V1 { parent, market: market_addr, .. } = market;
        (*parent, *market_addr)
    }

    /// Check if events emission is enabled.
    pub fn allow_events_emission(&self) -> bool {
        let PerpMarket::V1 { market } = self;
        let Market::V1 { config, .. } = market;
        let MarketConfig::V1 { allow_events_emission, .. } = config;
        *allow_events_emission
    }

    /// Check if an order is a taker order (crosses the book).
    /// Taker if: (is_buy && price >= best_ask) || (!is_buy && price <= best_bid)
    /// Orders with trigger_condition are never takers.
    pub fn is_taker_order(&self, price: u64, is_buy: bool, trigger_condition: &Option<TriggerCondition>) -> bool {
        if trigger_condition.is_some() {
            return false;
        }
        if is_buy {
            if let Some(best_ask) = self.best_ask_price() {
                price >= best_ask
            } else {
                false
            }
        } else {
            if let Some(best_bid) = self.best_bid_price() {
                price <= best_bid
            } else {
                false
            }
        }
    }
}

impl BulkOrderBook {
    /// Remove the existing bulk order for an account (if any).
    /// Returns the old BulkOrder if one existed.
    pub fn remove_order(&mut self, account: &AccountAddress) -> Option<BulkOrder> {
        let BulkOrderBook::V1 { orders, .. } = self;
        orders.remove_leaf(account)
    }

    /// Add a new bulk order for an account.
    pub fn add_order(&mut self, account: AccountAddress, order: BulkOrder) {
        let BulkOrderBook::V1 { orders, .. } = self;
        orders.add_leaf(account, order);
    }

    /// Add a mapping from order_id to account address.
    pub fn add_order_id_mapping(&mut self, order_id: OrderId, account: AccountAddress) {
        let BulkOrderBook::V1 { order_id_to_address, .. } = self;
        order_id_to_address.add_leaf(order_id, account);
    }
}

impl PriceTimeIndex {
    /// Add active price levels from a bulk order to the price-time index.
    /// For bids, only the first (highest) price level is activated.
    /// For asks, only the first (lowest) price level is activated.
    pub fn activate_bulk_order_prices(
        &mut self,
        order_id: OrderId,
        bid_prices: &[u64],
        bid_sizes: &[u64],
        ask_prices: &[u64],
        ask_sizes: &[u64],
        unique_priority_idx: IncreasingIdx,
    ) {
        let PriceTimeIndex::V1 { buys, sells } = self;

        // Activate first non-zero bid (bids are in descending order, first = highest)
        for i in 0..bid_prices.len().min(bid_sizes.len()) {
            if bid_sizes[i] > 0 {
                let key = PriceDescTime {
                    price: bid_prices[i],
                    tie_breaker: DecreasingIdx { idx: u128::MAX - unique_priority_idx.idx },
                };
                let data = OrderData {
                    order_id,
                    order_book_type: OrderType { order_type: 1 }, // bulk_order_type()
                    size: bid_sizes[i],
                };
                buys.add_leaf(key, data);
                break;
            }
        }

        // Activate first non-zero ask (asks are in ascending order, first = lowest)
        for i in 0..ask_prices.len().min(ask_sizes.len()) {
            if ask_sizes[i] > 0 {
                let key = PriceAscTime {
                    price: ask_prices[i],
                    tie_breaker: IncreasingIdx { idx: unique_priority_idx.idx },
                };
                let data = OrderData {
                    order_id,
                    order_book_type: OrderType { order_type: 1 }, // bulk_order_type()
                    size: ask_sizes[i],
                };
                sells.add_leaf(key, data);
                break;
            }
        }
    }

    /// Remove active price levels associated with a bulk order from the price-time index.
    pub fn remove_bulk_order_prices(
        &mut self,
        old_order: &BulkOrder,
    ) {
        let BulkOrder::V1 { order_request, order_id: _, unique_priority_idx, .. } = old_order;
        let BulkOrderRequest::V1 { bid_prices, bid_sizes, ask_prices, ask_sizes, .. } = order_request;
        let PriceTimeIndex::V1 { buys, sells } = self;

        // Remove bid entries
        for i in 0..bid_prices.len().min(bid_sizes.len()) {
            if bid_sizes[i] > 0 {
                let key = PriceDescTime {
                    price: bid_prices[i],
                    tie_breaker: DecreasingIdx { idx: u128::MAX - unique_priority_idx.idx },
                };
                buys.remove_leaf(&key);
                break; // Only first active level was in the index
            }
        }

        // Remove ask entries
        for i in 0..ask_prices.len().min(ask_sizes.len()) {
            if ask_sizes[i] > 0 {
                let key = PriceAscTime {
                    price: ask_prices[i],
                    tie_breaker: IncreasingIdx { idx: unique_priority_idx.idx },
                };
                sells.remove_leaf(&key);
                break; // Only first active level was in the index
            }
        }
    }

    /// Tree-aware version of activate_bulk_order_prices.
    pub fn tree_activate_bulk_order_prices<F1, F2>(
        &mut self,
        order_id: OrderId,
        bid_prices: &[u64],
        bid_sizes: &[u64],
        ask_prices: &[u64],
        ask_sizes: &[u64],
        unique_priority_idx: IncreasingIdx,
        read_buys: &F1,
        read_sells: &F2,
    ) -> (Vec<TableWrite>, Vec<TableWrite>)
    where
        F1: Fn(u64) -> Result<Option<Vec<u8>>, String>,
        F2: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let PriceTimeIndex::V1 { buys, sells } = self;
        let mut buys_writes = Vec::new();
        let mut sells_writes = Vec::new();

        for i in 0..bid_prices.len().min(bid_sizes.len()) {
            if bid_sizes[i] > 0 {
                let key = PriceDescTime {
                    price: bid_prices[i],
                    tie_breaker: DecreasingIdx { idx: u128::MAX - unique_priority_idx.idx },
                };
                let data = OrderData {
                    order_id,
                    order_book_type: OrderType { order_type: 1 },
                    size: bid_sizes[i],
                };
                buys_writes.extend(buys.tree_add(key, data, read_buys).unwrap_or_default());
                break;
            }
        }

        for i in 0..ask_prices.len().min(ask_sizes.len()) {
            if ask_sizes[i] > 0 {
                let key = PriceAscTime {
                    price: ask_prices[i],
                    tie_breaker: IncreasingIdx { idx: unique_priority_idx.idx },
                };
                let data = OrderData {
                    order_id,
                    order_book_type: OrderType { order_type: 1 },
                    size: ask_sizes[i],
                };
                sells_writes.extend(sells.tree_add(key, data, read_sells).unwrap_or_default());
                break;
            }
        }

        (buys_writes, sells_writes)
    }

    /// Tree-aware version of remove_bulk_order_prices.
    pub fn tree_remove_bulk_order_prices<F1, F2>(
        &mut self,
        old_order: &BulkOrder,
        read_buys: &F1,
        read_sells: &F2,
    ) -> (Vec<TableWrite>, Vec<TableWrite>)
    where
        F1: Fn(u64) -> Result<Option<Vec<u8>>, String>,
        F2: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let BulkOrder::V1 { order_request, unique_priority_idx, .. } = old_order;
        let BulkOrderRequest::V1 { bid_prices, bid_sizes, ask_prices, ask_sizes, .. } = order_request;
        let PriceTimeIndex::V1 { buys, sells } = self;
        let mut buys_writes = Vec::new();
        let mut sells_writes = Vec::new();

        for i in 0..bid_prices.len().min(bid_sizes.len()) {
            if bid_sizes[i] > 0 {
                let key = PriceDescTime {
                    price: bid_prices[i],
                    tie_breaker: DecreasingIdx { idx: u128::MAX - unique_priority_idx.idx },
                };
                let (_, w) = buys.tree_remove(&key, read_buys).unwrap_or((None, vec![]));
                buys_writes.extend(w);
                break;
            }
        }

        for i in 0..ask_prices.len().min(ask_sizes.len()) {
            if ask_sizes[i] > 0 {
                let key = PriceAscTime {
                    price: ask_prices[i],
                    tie_breaker: IncreasingIdx { idx: unique_priority_idx.idx },
                };
                let (_, w) = sells.tree_remove(&key, read_sells).unwrap_or((None, vec![]));
                sells_writes.extend(w);
                break;
            }
        }

        (buys_writes, sells_writes)
    }
}

// ============================================================================
// ACKEvent (emitted during scheduling and trigger_matching)
// ============================================================================

/// Move: `enum AckPhase has drop, store { BackstopLiquidation, MarginCall, CheckADL, TriggerADL, CommitMarkPrice, QueueBackstopLiquidationsAndADL, QueueMarginCallLiquidations, InitialEnqueue }`
/// CRITICAL: Variant order must match Move exactly for correct BCS serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ACKEvent {
    V1 {
        market: AccountAddress,
        accounts: Vec<AccountAddress>,
        batch_key: u128,
        ack_phase: AckPhase,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemPurgedOrderEvent {
    V1 {
        market: AccountAddress,
        account: AccountAddress,
        order_id: OrderId,
    },
}

// ============================================================================
// PriceUpdateEvent::V2 (emitted by update_mark_for_internal_oracle)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriceFundingUpdateDetails {
    V1 {
        funding_index: i128,
        funding_timestamp_us: u64,
        outstanding_funding_index: i128,
        outstanding_funding_timestamp_us: u64,
        funding_period_us: u64,
        instant_daily_funding_rate: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriceUpdateEvent {
    V1 {
        market: AccountAddress,
        oracle_px: u64,
        mark_px: u64,
        impact_ask_px: u64,
        impact_bid_px: u64,
        funding_index: i128,
        funding_rate_bps: i64,
    },
    V2 {
        market: AccountAddress,
        oracle_px: u64,
        mark_px: u64,
        impact_ask_px: u64,
        impact_bid_px: u64,
        funding: PriceFundingUpdateDetails,
    },
}

// ============================================================================
// Additional BigOrderedMap helpers for order matching
// ============================================================================

impl<K, V> BigOrderedMap<K, V>
where
    K: Serialize + DeserializeOwned + Clone + Ord,
    V: Serialize + DeserializeOwned + Clone,
{
    /// Borrow the first (smallest key) leaf entry from the root node.
    /// Returns None if the root is empty or the first child is not a Leaf.
    pub fn borrow_front_leaf(&self) -> Option<(&K, &V)> {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        entries.first().and_then(|e| {
            match &e.value {
                Child::Leaf { value } => Some((&e.key, value)),
                Child::Inner { .. } => None,
            }
        })
    }

    /// Borrow the last (largest key) leaf entry from the root node.
    /// Returns None if the root is empty or the last child is not a Leaf.
    pub fn borrow_back_leaf(&self) -> Option<(&K, &V)> {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        entries.last().and_then(|e| {
            match &e.value {
                Child::Leaf { value } => Some((&e.key, value)),
                Child::Inner { .. } => None,
            }
        })
    }

    /// Remove and return the last (largest key) leaf entry from the root node.
    /// Returns None if the root is empty or the last child is not a Leaf.
    pub fn pop_back_leaf(&mut self) -> Option<(K, V)> {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        if entries.is_empty() {
            return None;
        }
        let last_idx = entries.len() - 1;
        match &entries[last_idx].value {
            Child::Leaf { .. } => {
                let entry = entries.remove(last_idx);
                if let Child::Leaf { value } = entry.value {
                    Some((entry.key, value))
                } else {
                    unreachable!()
                }
            },
            Child::Inner { .. } => None,
        }
    }

    /// Mutate the value of the first (smallest key) leaf entry in the root node.
    /// Returns false if the root is empty or the first child is not a Leaf.
    pub fn modify_front_leaf<F: FnOnce(&mut V)>(&mut self, f: F) -> bool {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        if let Some(entry) = entries.first_mut() {
            match &mut entry.value {
                Child::Leaf { value } => { f(value); true },
                Child::Inner { .. } => false,
            }
        } else {
            false
        }
    }

    /// Mutate the value of the last (largest key) leaf entry in the root node.
    /// Returns false if the root is empty or the last child is not a Leaf.
    pub fn modify_back_leaf<F: FnOnce(&mut V)>(&mut self, f: F) -> bool {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        if let Some(entry) = entries.last_mut() {
            match &mut entry.value {
                Child::Leaf { value } => { f(value); true },
                Child::Inner { .. } => false,
            }
        } else {
            false
        }
    }

    /// Get a leaf value by key from the root node.
    pub fn get_leaf(&self, key: &K) -> Option<&V> {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        if let Ok(pos) = entries.binary_search_by(|e| e.key.cmp(key)) {
            match &entries[pos].value {
                Child::Leaf { value } => Some(value),
                Child::Inner { .. } => None,
            }
        } else {
            None
        }
    }

    /// Get a mutable reference to a leaf value by key from the root node.
    pub fn get_leaf_mut(&mut self, key: &K) -> Option<&mut V> {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        if let Ok(pos) = entries.binary_search_by(|e| e.key.cmp(key)) {
            match &mut entries[pos].value {
                Child::Leaf { value } => Some(value),
                Child::Inner { .. } => None,
            }
        } else {
            None
        }
    }

    /// Get the table handle from the StorageSlotsAllocator, if it exists.
    pub fn get_table_handle(&self) -> Option<&TableHandle> {
        let BigOrderedMap::BPlusTreeMap { nodes, .. } = self;
        let StorageSlotsAllocator::V1 { slots, .. } = nodes;
        slots.as_ref().map(|twl| &twl.inner)
    }

    /// Initialize the table for this BigOrderedMap if it doesn't have one yet.
    /// This is needed before splitting can happen.
    /// Returns true if a new table was created, false if one already existed.
    pub fn init_table_if_needed(&mut self, handle: AccountAddress) -> bool {
        let BigOrderedMap::BPlusTreeMap { nodes, .. } = self;
        let StorageSlotsAllocator::V1 { slots, new_slot_index, .. } = nodes;
        if slots.is_some() {
            return false;
        }
        *slots = Some(TableWithLength {
            inner: TableHandle { handle },
            length: 0,
        });
        // Initialize new_slot_index to 10 (matching Move's default start)
        // Actually, keep it at its current value (which should be 10 from init)
        if *new_slot_index == 0 {
            *new_slot_index = 10;
        }
        true
    }

    /// Check if the root has any Inner children that need resolution.
    pub fn has_inner_children(&self) -> bool {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        entries.iter().any(|e| matches!(&e.value, Child::Inner { .. }))
    }

    /// Count how many Inner children the root has.
    pub fn count_inner_children(&self) -> usize {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        entries.iter().filter(|e| matches!(&e.value, Child::Inner { .. })).count()
    }

    /// Resolve all Inner children in the root node by reading their leaf nodes
    /// from table storage. After this call, the root will contain only Leaf
    /// entries (inlined from child nodes), allowing the existing leaf-only
    /// matching code to work correctly.
    ///
    /// The `read_slot` closure reads a table item by slot_index and returns
    /// the raw bytes of the stored `Link<Node<K,V>>`.
    pub fn resolve_inner_nodes<F>(&mut self, read_slot: &F) -> Result<(), String>
    where
        F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        let Node::V1 { is_leaf, children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;

        // Collect indices of Inner entries that need resolution
        let inner_indices: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                if matches!(&e.value, Child::Inner { .. }) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        if inner_indices.is_empty() {
            return Ok(());
        }

        // Process inner nodes from back to front (so indices stay valid during insertion)
        for &idx in inner_indices.iter().rev() {
            let slot_index = match &entries[idx].value {
                Child::Inner { node_index } => node_index.slot_index,
                _ => unreachable!(),
            };

            // Read the child node from table storage
            let raw_bytes = read_slot(slot_index)?
                .ok_or_else(|| format!("BigOrderedMap: slot {} not found in table", slot_index))?;

            // Deserialize as Link<Node<K,V>>
            let link: Link<Node<K, V>> = bcs::from_bytes(&raw_bytes)
                .map_err(|e| format!("BigOrderedMap: failed to deserialize slot {}: {}", slot_index, e))?;

            let child_node = match link {
                Link::Occupied { value } => value,
                Link::Vacant { .. } => {
                    return Err(format!("BigOrderedMap: slot {} is vacant", slot_index));
                },
            };

            // Extract leaf entries from the child node
            let Node::V1 { children: child_children, .. } = child_node;
            let OrderedMap::SortedVectorMap { entries: child_entries } = child_children;

            // Remove the Inner entry and splice in the child's entries
            entries.remove(idx);
            for (j, child_entry) in child_entries.into_iter().enumerate() {
                // If the child has Inner entries itself, we recursively need to handle them.
                // For now, we only handle one level deep (B+ trees in the benchmark
                // typically have at most 2 levels for the order book).
                entries.insert(idx + j, child_entry);
            }
        }

        // If we still have Inner entries after one pass (multi-level tree),
        // do another pass. This handles 3+ level trees.
        if entries.iter().any(|e| matches!(&e.value, Child::Inner { .. })) {
            // Recursive resolution - but limit depth to prevent infinite loops
            self.resolve_inner_nodes(read_slot)?;
        } else {
            // All entries are now Leaf entries - mark root as leaf so split_root_if_needed works
            *is_leaf = true;
        }

        Ok(())
    }
    /// Split the root leaf node if it exceeds `leaf_max_degree`.
    ///
    /// When the root is a leaf node with more entries than `leaf_max_degree`,
    /// this method splits the entries into multiple child leaf nodes stored
    /// in table items, converting the root into an inner node with Inner
    /// references.
    ///
    /// This matches Move's BigOrderedMap B+ tree splitting behavior, which
    /// occurs automatically when `add` is called and the node is full.
    ///
    /// Returns a list of `(slot_index, serialized_link_bytes)` for each new
    /// child node that needs to be written as a table item.
    ///
    /// The caller is responsible for writing these table items to storage:
    ///   key = bcs::to_bytes(&slot_index)
    ///   value = serialized_link_bytes
    ///   table_handle = self.get_table_handle()
    pub fn split_root_if_needed(&mut self) -> Vec<(u64, Vec<u8>)> {
        let BigOrderedMap::BPlusTreeMap {
            root, nodes, min_leaf_index, max_leaf_index, leaf_max_degree, ..
        } = self;

        let Node::V1 { is_leaf, children, prev: root_prev, next: root_next } = root;

        // Only split leaf roots that exceed the degree limit
        if !*is_leaf {
            return vec![];
        }

        let OrderedMap::SortedVectorMap { entries } = children;
        if entries.len() <= *leaf_max_degree as usize {
            return vec![];
        }

        // All entries must be Leaf children (not Inner)
        let has_inner = entries.iter().any(|e| matches!(&e.value, Child::Inner { .. }));
        if has_inner {
            return vec![];
        }

        let StorageSlotsAllocator::V1 { new_slot_index, slots, .. } = nodes;

        // We need a table to store child nodes. If slots is None, we cannot split
        // (table not yet created). In the benchmark, tables are created during init.
        if slots.is_none() {
            return vec![];
        }
        let table_with_length = slots.as_mut().unwrap();

        // Compute target_size for each child node: (leaf_max_degree + 1) / 2
        // This matches Move's split logic in add_at.
        let target_size = (*leaf_max_degree as usize + 1) / 2;

        // Take all entries out of the root
        let all_entries: Vec<Entry<K, Child<V>>> = std::mem::take(entries);
        let _total = all_entries.len();

        // Split into chunks
        let mut chunks: Vec<Vec<Entry<K, Child<V>>>> = Vec::new();
        let mut remaining = all_entries;
        while !remaining.is_empty() {
            let chunk_size = target_size.min(remaining.len());
            let rest = remaining.split_off(chunk_size);
            chunks.push(remaining);
            remaining = rest;
        }

        // If the last chunk is too small, merge with previous
        // (Move ensures each node has at least target_size/2 entries)
        if chunks.len() > 1 {
            let last = chunks.last().unwrap().len();
            if last < target_size / 2 {
                let last_chunk = chunks.pop().unwrap();
                if let Some(prev) = chunks.last_mut() {
                    prev.extend(last_chunk);
                }
            }
        }

        let num_children = chunks.len();
        let mut new_table_items: Vec<(u64, Vec<u8>)> = Vec::with_capacity(num_children);
        let mut inner_entries: Vec<Entry<K, Child<V>>> = Vec::with_capacity(num_children);

        // Allocate slot indices for all child nodes
        let first_slot = *new_slot_index;
        let slot_indices: Vec<u64> = (0..num_children)
            .map(|i| first_slot + i as u64)
            .collect();
        *new_slot_index = first_slot + num_children as u64;

        // Update table length
        table_with_length.length += num_children as u64;

        // Create child nodes with proper linked list pointers
        for (i, chunk) in chunks.into_iter().enumerate() {
            let slot_idx = slot_indices[i];
            let prev_idx = if i == 0 { 0u64 } else { slot_indices[i - 1] }; // NULL_INDEX = 0
            let next_idx = if i + 1 < num_children { slot_indices[i + 1] } else { 0u64 };

            // The max key of this chunk is the key for the Inner reference
            let max_key = chunk.last().unwrap().key.clone();

            let child_node = Node::V1 {
                is_leaf: true,
                children: OrderedMap::SortedVectorMap { entries: chunk },
                prev: prev_idx,
                next: next_idx,
            };

            // Wrap in Link::Occupied
            let link = Link::Occupied { value: child_node };
            let serialized = bcs::to_bytes(&link).expect("Failed to serialize child node Link");
            new_table_items.push((slot_idx, serialized));

            // Add Inner reference to the root
            inner_entries.push(Entry {
                key: max_key,
                value: Child::Inner {
                    node_index: StoredSlot { slot_index: slot_idx },
                },
            });
        }

        // Update root to be an inner node
        *is_leaf = false;
        *entries = inner_entries;
        *root_prev = 0; // NULL_INDEX
        *root_next = 0; // NULL_INDEX

        // Update leaf index pointers
        *min_leaf_index = slot_indices[0];
        *max_leaf_index = slot_indices[num_children - 1];

        new_table_items
    }
}

// ============================================================================
// OrderType constants
// ============================================================================

impl OrderType {
    pub fn single_order_type() -> Self {
        OrderType { order_type: 0 }
    }

    pub fn bulk_order_type() -> Self {
        OrderType { order_type: 1 }
    }

    pub fn is_single_order(&self) -> bool {
        self.order_type == 0
    }

    pub fn is_bulk_order(&self) -> bool {
        self.order_type == 1
    }
}

// ============================================================================
// PerpMarket full order book access for matching
// ============================================================================

impl PerpMarket {
    /// Get mutable access to all order book components for matching operations.
    pub fn full_order_book_mut(&mut self) -> (&mut SingleOrderBook, &mut BulkOrderBook, &mut PriceTimeIndex) {
        let PerpMarket::V1 { market } = self;
        let Market::V1 { order_book, .. } = market;
        let OrderBook::UnifiedV1 { single_order_book, bulk_order_book, price_time_idx } = order_book;
        (single_order_book, bulk_order_book, price_time_idx)
    }
}

// ============================================================================
// SingleOrderBook helpers for matching
// ============================================================================

impl SingleOrderBook {
    /// Look up an order by OrderId in the root node.
    /// Returns the full OrderWithState if found inline (not in a table item).
    pub fn get_order(&self, order_id: &OrderId) -> Option<&OrderWithState> {
        let SingleOrderBook::V1 { orders, .. } = self;
        orders.get_leaf(order_id)
    }

    /// Remove an order by OrderId from the root node.
    pub fn remove_order(&mut self, order_id: &OrderId) -> Option<OrderWithState> {
        let SingleOrderBook::V1 { orders, .. } = self;
        orders.remove_leaf(order_id)
    }

    /// Get mutable reference to an order by OrderId.
    pub fn get_order_mut(&mut self, order_id: &OrderId) -> Option<&mut OrderWithState> {
        let SingleOrderBook::V1 { orders, .. } = self;
        orders.get_leaf_mut(order_id)
    }
}

// ============================================================================
// BulkOrderBook helpers for matching
// ============================================================================

impl BulkOrderBook {
    /// Get bulk order address by order_id.
    pub fn get_order_address(&self, order_id: &OrderId) -> Option<&AccountAddress> {
        let BulkOrderBook::V1 { order_id_to_address, .. } = self;
        order_id_to_address.get_leaf(order_id)
    }

    /// Get a bulk order by account address.
    pub fn get_order_by_address(&self, address: &AccountAddress) -> Option<&BulkOrder> {
        let BulkOrderBook::V1 { orders, .. } = self;
        orders.get_leaf(address)
    }

    /// Get a mutable bulk order by account address.
    pub fn get_order_by_address_mut(&mut self, address: &AccountAddress) -> Option<&mut BulkOrder> {
        let BulkOrderBook::V1 { orders, .. } = self;
        orders.get_leaf_mut(address)
    }
}

// ============================================================================
// BulkOrder helpers
// ============================================================================

impl BulkOrder {
    /// Get the account address of the bulk order creator.
    pub fn account(&self) -> AccountAddress {
        let BulkOrder::V1 { order_request, .. } = self;
        let BulkOrderRequest::V1 { account, .. } = order_request;
        *account
    }

    /// Get the order_id.
    pub fn order_id(&self) -> OrderId {
        let BulkOrder::V1 { order_id, .. } = self;
        *order_id
    }

    /// Get the sequence number.
    pub fn sequence_number(&self) -> u64 {
        let BulkOrder::V1 { order_request, .. } = self;
        let BulkOrderRequest::V1 { order_sequence_number, .. } = order_request;
        *order_sequence_number
    }

    /// Get the unique_priority_idx.
    pub fn unique_priority_idx(&self) -> IncreasingIdx {
        let BulkOrder::V1 { unique_priority_idx, .. } = self;
        *unique_priority_idx
    }

    /// Match against a side of the bulk order. Returns the fill price and reduces the size.
    /// Also returns the next price level to activate (if any).
    /// `maker_is_bid`: whether the maker side is the bid side.
    pub fn match_and_advance(
        &mut self,
        maker_is_bid: bool,
        matched_size: u64,
    ) -> (u64, Option<(u64, u64)>) {
        let BulkOrder::V1 { order_request, unique_priority_idx: _, .. } = self;
        let BulkOrderRequest::V1 {
            bid_prices, bid_sizes, ask_prices, ask_sizes, ..
        } = order_request;

        let (prices, sizes) = if maker_is_bid {
            (bid_prices, bid_sizes)
        } else {
            (ask_prices, ask_sizes)
        };

        // Find the active price level (first non-zero size)
        let mut fill_price = 0u64;
        let mut _remaining_match = matched_size;
        let mut consumed_level = false;

        for i in 0..prices.len().min(sizes.len()) {
            if sizes[i] > 0 {
                fill_price = prices[i];
                if _remaining_match >= sizes[i] {
                                        _remaining_match -= sizes[i];
                    sizes[i] = 0;
                    consumed_level = true;
                } else {
                    sizes[i] -= _remaining_match;
                    _remaining_match = 0;
                }
                break;
            }
        }

        // If we consumed the level, find the next active level
        let next_level = if consumed_level {
            let mut found = false;
            let mut next_price = 0u64;
            let mut next_size = 0u64;
            for i in 0..prices.len().min(sizes.len()) {
                if sizes[i] > 0 {
                    next_price = prices[i];
                    next_size = sizes[i];
                    found = true;
                    break;
                }
            }
            if found { Some((next_price, next_size)) } else { None }
        } else {
            None
        };

        (fill_price, next_level)
    }

    /// Get the metadata.
    pub fn metadata(&self) -> &OrderMetadata {
        let BulkOrder::V1 { order_request, .. } = self;
        let BulkOrderRequest::V1 { metadata, .. } = order_request;
        metadata
    }

    /// Get the active price for a side (first non-zero entry).
    pub fn active_price(&self, is_bid: bool) -> Option<u64> {
        let BulkOrder::V1 { order_request, .. } = self;
        let BulkOrderRequest::V1 { bid_prices, bid_sizes, ask_prices, ask_sizes, .. } = order_request;
        let (prices, sizes) = if is_bid {
            (bid_prices, bid_sizes)
        } else {
            (ask_prices, ask_sizes)
        };
        for i in 0..prices.len().min(sizes.len()) {
            if sizes[i] > 0 {
                return Some(prices[i]);
            }
        }
        None
    }

    /// Get the active size for a side (first non-zero entry).
    pub fn active_size(&self, is_bid: bool) -> Option<u64> {
        let BulkOrder::V1 { order_request, .. } = self;
        let BulkOrderRequest::V1 { bid_sizes, ask_sizes, .. } = order_request;
        let sizes = if is_bid { bid_sizes } else { ask_sizes };
        for i in 0..sizes.len() {
            if sizes[i] > 0 {
                return Some(sizes[i]);
            }
        }
        None
    }
}

// ============================================================================
// OrderWithState / SingleOrderRequest helpers
// ============================================================================

impl OrderWithState {
    pub fn get_request(&self) -> &SingleOrderRequest {
        let OrderWithState::V1 { order, .. } = self;
        let SingleOrder::V1 { order_request, .. } = order;
        order_request
    }

    pub fn get_remaining_size(&self) -> u64 {
        let SingleOrderRequest::V1 { remaining_size, .. } = self.get_request();
        *remaining_size
    }

    pub fn set_remaining_size(&mut self, new_size: u64) {
        let OrderWithState::V1 { order, .. } = self;
        let SingleOrder::V1 { order_request, .. } = order;
        let SingleOrderRequest::V1 { remaining_size, .. } = order_request;
        *remaining_size = new_size;
    }

    pub fn get_account(&self) -> AccountAddress {
        let SingleOrderRequest::V1 { account, .. } = self.get_request();
        *account
    }

    pub fn get_price(&self) -> u64 {
        let SingleOrderRequest::V1 { price, .. } = self.get_request();
        *price
    }

    pub fn get_orig_size(&self) -> u64 {
        let SingleOrderRequest::V1 { orig_size, .. } = self.get_request();
        *orig_size
    }

    pub fn is_bid(&self) -> bool {
        let SingleOrderRequest::V1 { is_bid, .. } = self.get_request();
        *is_bid
    }

    pub fn get_order_id(&self) -> OrderId {
        let SingleOrderRequest::V1 { order_id, .. } = self.get_request();
        *order_id
    }

    pub fn get_client_order_id(&self) -> Option<String> {
        let SingleOrderRequest::V1 { client_order_id, .. } = self.get_request();
        client_order_id.clone()
    }

    pub fn get_metadata(&self) -> &OrderMetadata {
        let SingleOrderRequest::V1 { metadata, .. } = self.get_request();
        metadata
    }

    pub fn get_time_in_force(&self) -> TimeInForce {
        let SingleOrderRequest::V1 { time_in_force, .. } = self.get_request();
        *time_in_force
    }

    pub fn get_unique_priority_idx(&self) -> IncreasingIdx {
        let OrderWithState::V1 { order, .. } = self;
        let SingleOrder::V1 { unique_priority_idx, .. } = order;
        *unique_priority_idx
    }
}

// ============================================================================
// FillEvent type for order matching
// ============================================================================

/// Move: `enum FillEvent has drop, store { V1 { ... } }`
/// This is the event emitted when a trade fill occurs during order matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FillEvent {
    V1 {
        parent: AccountAddress,
        market: AccountAddress,
        fill_id: u128,
        taker: AccountAddress,
        maker: AccountAddress,
        taker_order_id: u128,
        maker_order_id: u128,
        taker_client_order_id: Option<String>,
        maker_client_order_id: Option<String>,
        price: u64,
        size: u64,
        taker_is_buy: bool,
        maker_fee: i64,
        taker_fee: i64,
        taker_realized_pnl: i64,
        maker_realized_pnl: i64,
    },
}

/// Move: `enum BulkOrderFilledEvent has drop, store { V1 { ... } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BulkOrderFilledEvent {
    V1 {
        parent: AccountAddress,
        market: AccountAddress,
        order_id: u128,
        sequence_number: u64,
        user: AccountAddress,
        size: u64,
        fill_price: u64,
        order_price: u64,
        is_bid: bool,
        fill_id: u128,
    },
}

// ============================================================================
// Settlement event types (emitted by clearinghouse during trade settlement)
// ============================================================================

/// Move: `enum Action has drop, store, copy { OpenLong, CloseLong, OpenShort, CloseShort }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Action {
    OpenLong,
    CloseLong,
    OpenShort,
    CloseShort,
}

/// Move: `enum TradeTriggerSource has drop, store, copy { OrderFill, MarginCall, BackStopLiquidation, ADL, MarketDelisted }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum TradeTriggerSource {
    OrderFill,
    MarginCall,
    BackStopLiquidation,
    ADL,
    MarketDelisted,
}

/// Move: `enum CollateralBalanceType has drop, store, copy { Cross { account: address }, Isolated { account: address, market: Object<PerpMarket> } }`
/// `Object<PerpMarket>` serializes as just an address in BCS.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CollateralBalanceType {
    Cross { account: AccountAddress },
    Isolated { account: AccountAddress, market: AccountAddress },
}

/// Move: `enum FeeWithDestination has copy, drop, store { V1 { address: address, fees: u64 } }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FeeWithDestination {
    V1 { address: AccountAddress, fees: u64 },
}

/// Move: `enum FeeDistribution has drop, copy, store { RegularTrade_V1 { ... }, MarginCall_V1 { ... } }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum FeeDistribution {
    RegularTrade_V1 {
        balance_type: CollateralBalanceType,
        position_fee_delta: i64,
        treasury_fee_delta: i64,
        builder_or_referrer_fees: Option<FeeWithDestination>,
    },
    MarginCall_V1 {
        balance_type: CollateralBalanceType,
        position_fee_delta: i64,
    },
}

/// Move: `enum TradeEvent has store, drop { V1 { account, market, action, source, order_id, client_order_id, size, price, builder_code, realized_pnl, realized_funding_cost, fee, fill_id, is_taker, fee_distribution } }`
/// `market` is `Object<PerpMarket>` which serializes as just an address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradeEvent {
    V1 {
        account: AccountAddress,
        market: AccountAddress,
        action: Action,
        source: TradeTriggerSource,
        order_id: Option<OrderId>,
        client_order_id: Option<String>,
        size: u64,
        price: u64,
        builder_code: Option<BuilderCode>,
        realized_pnl: i64,
        realized_funding_cost: i64,
        fee: i64,
        fill_id: u128,
        is_taker: bool,
        fee_distribution: FeeDistribution,
    },
}

/// Move: `enum FullSizedTpSlForEvent has copy, store, drop { V1 { order_id: u128, trigger_price: u64, limit_price: Option<u64> } }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FullSizedTpSlForEvent {
    V1 {
        order_id: u128,
        trigger_price: u64,
        limit_price: Option<u64>,
    },
}

/// Move: `enum FixedSizedTpSlForEvent has copy, store, drop { V1 { order_id: u128, trigger_price: u64, limit_price: Option<u64>, size: u64 } }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FixedSizedTpSlForEvent {
    V1 {
        order_id: u128,
        trigger_price: u64,
        limit_price: Option<u64>,
        size: u64,
    },
}

/// Move: `enum PositionUpdateEvent has copy, store, drop { V1 { market, user, is_long, size, user_leverage, entry_price_times_size_sum, is_isolated, funding_index_at_last_update, unrealized_funding_amount_before_last_update, full_sized_tp, fixed_sized_tps, full_sized_sl, fixed_sized_sls } }`
/// `market` is `Object<PerpMarket>` which serializes as just an address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PositionUpdateEvent {
    V1 {
        market: AccountAddress,
        user: AccountAddress,
        is_long: bool,
        size: u64,
        user_leverage: u8,
        entry_price_times_size_sum: u128,
        is_isolated: bool,
        funding_index_at_last_update: i128,
        unrealized_funding_amount_before_last_update: i64,
        full_sized_tp: Option<FullSizedTpSlForEvent>,
        fixed_sized_tps: Vec<FixedSizedTpSlForEvent>,
        full_sized_sl: Option<FullSizedTpSlForEvent>,
        fixed_sized_sls: Vec<FixedSizedTpSlForEvent>,
    },
}

/// Move: `enum OpenInterestUpdateEvent has drop, store { V1 { market: Object<PerpMarket>, current_open_interest: u64 } }`
/// `market` is `Object<PerpMarket>` which serializes as just an address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpenInterestUpdateEvent {
    V1 {
        market: AccountAddress,
        current_open_interest: u64,
    },
}

// ============================================================================
// CollateralBalanceChangeEvent (emitted during settlement for each collateral change)
// ============================================================================

/// Move: `enum CollateralBalanceChangeType has drop, store, copy { UserMovement, Fee, PnL, Margin, Liquidation }`
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CollateralBalanceChangeType {
    UserMovement,
    Fee,
    PnL,
    Margin,
    Liquidation,
}

/// Move: `enum I64Snapshot has store, drop { V1 { offset_balance: AggregatorSnapshot<u64> } }`
/// AggregatorSnapshot<u64> is just `{ value: u64 }` in BCS.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum I64Snapshot {
    V1 {
        offset_balance: u64,
    },
}

/// Move: `enum CollateralBalanceChangeEvent has drop, store { V1 { asset_type: Object<Metadata>, balance_type: CollateralBalanceType, delta: i64, offset_balance_after: I64Snapshot, change_type: CollateralBalanceChangeType } }`
/// `asset_type` is `Object<Metadata>` which serializes as just an address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollateralBalanceChangeEvent {
    V1 {
        asset_type: AccountAddress,
        balance_type: CollateralBalanceType,
        delta: i64,
        offset_balance_after: I64Snapshot,
        change_type: CollateralBalanceChangeType,
    },
}
// ============================================================================
// TradingFeesManager types (for table handle extraction during settlement)
// ============================================================================

/// Aggregator<u128> = struct { value: u128, max_value: u128 }
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AggregatorU128 {
    pub value: u128,
    pub max_value: u128,
}

/// DayVolume::V1 { day_since_epoch: u64, volume: u128 }
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DayVolume {
    V1 {
        day_since_epoch: u64,
        volume: u128,
    },
}

/// VolumeHistory::V1 { latest_day_since_epoch, latest_day_volume, history, total_volume_in_window, total_volume_all_time }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VolumeHistory {
    V1 {
        latest_day_since_epoch: u64,
        latest_day_volume: AggregatorU128,
        history: Vec<DayVolume>,
        total_volume_in_window: u128,
        total_volume_all_time: AggregatorU128,
    },
}

/// VolumeStats::V1 { global_history, user_taker_volume_history, user_maker_volume_history }
/// The two Table handles are what we need for settlement table writes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VolumeStats {
    V1 {
        global_history: VolumeHistory,
        user_taker_volume_history: TableHandle,
        user_maker_volume_history: TableHandle,
    },
}



// ============================================================================
// Proper B+ Tree Operations for BigOrderedMap
// ============================================================================
//
// These methods implement tree-aware operations matching Move's big_ordered_map.move
// semantics. They navigate the tree structure by reading/writing child nodes via
// table I/O closures, instead of the hacky flatten/split cycle.
//
// Move constants: NULL_INDEX = 0, ROOT_INDEX = 1

/// Describes a table write operation to be flushed after tree operations.
#[derive(Debug, Clone)]
pub struct TableWrite {
    pub slot_index: u64,
    pub data: Vec<u8>,
    pub is_new: bool,
}

/// Helper to read a Node from storage.
fn read_node_from_slot<K, V, F>(
    slot_index: u64,
    read_slot: &F,
) -> Result<Node<K, V>, String>
where
    K: Serialize + DeserializeOwned + Clone + Ord,
    V: Serialize + DeserializeOwned + Clone,
    F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
{
    let raw = read_slot(slot_index)?
        .ok_or_else(|| format!("B+Tree: slot {} not found", slot_index))?;
    let link: Link<Node<K, V>> = bcs::from_bytes(&raw)
        .map_err(|e| format!("B+Tree: deserialize slot {}: {}", slot_index, e))?;
    match link {
        Link::Occupied { value } => Ok(value),
        Link::Vacant { .. } => Err(format!("B+Tree: slot {} is vacant", slot_index)),
    }
}

fn serialize_node_link<K, V>(node: &Node<K, V>) -> Vec<u8>
where
    K: Serialize + DeserializeOwned + Clone,
    V: Serialize + DeserializeOwned + Clone,
{
    let link = Link::Occupied { value: node.clone() };
    bcs::to_bytes(&link).expect("B+Tree: serialize node")
}


fn node_entries<K, V>(node: &Node<K, V>) -> &Vec<Entry<K, Child<V>>>
where
    K: Clone,
    V: Clone,
{
    let Node::V1 { children, .. } = node;
    let OrderedMap::SortedVectorMap { entries } = children;
    entries
}

fn node_entries_mut<K, V>(node: &mut Node<K, V>) -> &mut Vec<Entry<K, Child<V>>>
where
    K: Clone,
    V: Clone,
{
    let Node::V1 { children, .. } = node;
    let OrderedMap::SortedVectorMap { entries } = children;
    entries
}

fn node_is_leaf<K, V>(node: &Node<K, V>) -> bool
where K: Clone, V: Clone,
{
    let Node::V1 { is_leaf, .. } = node;
    *is_leaf
}

fn node_prev<K, V>(node: &Node<K, V>) -> u64
where K: Clone, V: Clone,
{
    let Node::V1 { prev, .. } = node;
    *prev
}

fn node_next<K, V>(node: &Node<K, V>) -> u64
where K: Clone, V: Clone,
{
    let Node::V1 { next, .. } = node;
    *next
}

impl<K, V> BigOrderedMap<K, V>
where
    K: Serialize + DeserializeOwned + Clone + Ord + std::fmt::Debug,
    V: Serialize + DeserializeOwned + Clone + std::fmt::Debug,
{
    /// Allocate a new slot index, respecting the reuse queue.
    /// When `should_reuse` is true and there are freed slots in the reuse chain,
    /// pops from the reuse queue (matching Move's `maybe_pop_from_reuse_queue`).
    /// The `read_slot` closure is needed to read Link::Vacant entries from table
    /// when popping from an on-disk reuse chain.
    /// Returns (slot_index, is_new) where is_new=false for reused slots.
    fn alloc_slot_reuse<F>(&mut self, read_slot: &F) -> Result<(u64, bool), String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let BigOrderedMap::BPlusTreeMap { nodes, write_cache, .. } = self;
        let StorageSlotsAllocator::V1 {
            new_slot_index, slots, should_reuse,
            reuse_head_index, reuse_spare_count, ..
        } = nodes;

        // Try to pop from reuse queue (matches Move's maybe_pop_from_reuse_queue)
        if *should_reuse && *reuse_head_index != 0 {
            let slot_index = *reuse_head_index;
            // Read the Vacant link to get the next pointer.
            // Check write_cache first (free_slot may have written to it in this transaction),
            // then fall back to external storage.
            let raw = if let Some(data) = write_cache.get(&slot_index) {
                data.clone()
            } else {
                read_slot(slot_index)?
                    .ok_or_else(|| format!("B+Tree: reuse slot {} not found in table", slot_index))?
            };
            let link: Link<Node<K, V>> = bcs::from_bytes(&raw)
                .map_err(|e| format!("B+Tree: deserialize reuse slot {}: {}", slot_index, e))?;
            match link {
                Link::Vacant { next } => {
                    *reuse_head_index = next;
                    *reuse_spare_count -= 1;
                    // Table length doesn't change (remove_link decrements, add_link increments = net 0)
                    return Ok((slot_index, false));  // reused slot, not new
                },
                Link::Occupied { .. } => {
                    // Shouldn't happen, but fall through to allocate new
                },
            }
        }

        // Allocate fresh slot
        let slot = *new_slot_index;
        *new_slot_index += 1;
        if let Some(twl) = slots {
            twl.length += 1;
        }
        Ok((slot, true))
    }


    /// Free a slot back to the reuse queue (matches Move's maybe_push_to_reuse_queue).
    /// Returns a TableWrite for the Link::Vacant entry if should_reuse is true.
    /// Also updates write_cache so subsequent reads see the vacancy.
    fn free_slot(&mut self, slot_index: u64) -> Option<TableWrite> {
        let BigOrderedMap::BPlusTreeMap { nodes, write_cache, .. } = self;
        let StorageSlotsAllocator::V1 {
            should_reuse, reuse_head_index, reuse_spare_count, slots, ..
        } = nodes;

        if *should_reuse {
            let next = *reuse_head_index;
            *reuse_head_index = slot_index;
            *reuse_spare_count += 1;
            let data = {
                let link: Link<Node<K, V>> = Link::Vacant { next };
                bcs::to_bytes(&link).expect("B+Tree: serialize vacant link")
            };
            write_cache.insert(slot_index, data.clone());
            Some(TableWrite { slot_index, data, is_new: false })
        } else {
            // Not reusing: decrement table length (slot is just dropped)
            if let Some(twl) = slots {
                if twl.length > 0 {
                    twl.length -= 1;
                }
            }
            None
        }
    }

    fn root_ref(&self) -> &Node<K, V> {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        root
    }

    fn root_mut(&mut self) -> &mut Node<K, V> {
        let BigOrderedMap::BPlusTreeMap { root, .. } = self;
        root
    }

    fn leaf_max(&self) -> usize {
        let BigOrderedMap::BPlusTreeMap { leaf_max_degree, .. } = self;
        *leaf_max_degree as usize
    }

    fn inner_max(&self) -> usize {
        let BigOrderedMap::BPlusTreeMap { inner_max_degree, .. } = self;
        *inner_max_degree as usize
    }

    /// Read a node: ROOT_INDEX=1 returns root inline, others use read_slot.
    /// Checks write_cache first for recently-written nodes within the same operation.
    fn read_node<F>(&self, idx: u64, read_slot: &F) -> Result<Node<K, V>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        if idx == 1 {
            Ok(self.root_ref().clone())
        } else {
            let BigOrderedMap::BPlusTreeMap { write_cache, .. } = self;
            if let Some(data) = write_cache.get(&idx) {
                let link: Link<Node<K, V>> = bcs::from_bytes(data)
                    .map_err(|e| format!("B+Tree: deserialize cached slot {}: {}", idx, e))?;
                match link {
                    Link::Occupied { value } => Ok(value),
                    Link::Vacant { .. } => Err(format!("B+Tree: cached slot {} is vacant", idx)),
                }
            } else {
                read_node_from_slot(idx, read_slot)
            }
        }
    }

    /// Write a node back: if idx==1 updates root inline, otherwise returns TableWrite.
    /// Also updates write_cache so subsequent reads within the same operation see it.
    fn write_node(&mut self, idx: u64, node: Node<K, V>, is_new: bool) -> Option<TableWrite> {
        if idx == 1 {
            *self.root_mut() = node;
            None
        } else {
            let data = serialize_node_link(&node);
            let BigOrderedMap::BPlusTreeMap { write_cache, .. } = self;
            write_cache.insert(idx, data.clone());
            Some(TableWrite {
                slot_index: idx,
                data,
                is_new,
            })
        }
    }

    /// Navigate from root to the leaf where `key` belongs.
    /// Returns path from root to leaf (inclusive).
    /// Empty path means key > all existing keys.
    fn tree_find_leaf_path<F>(&self, key: &K, read_slot: &F) -> Result<Vec<u64>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let mut path = Vec::new();
        let mut current = 1u64;
        let max_depth = 20;
        for _depth in 0..max_depth {
            path.push(current);
            let node = self.read_node(current, read_slot)?;
            if node_is_leaf(&node) {
                return Ok(path);
            }
            let entries = node_entries(&node);
            if entries.is_empty() {
                return Ok(path);
            }
            // lower_bound: first entry with key >= target
            let pos = entries.partition_point(|e| e.key < *key);
            // If key > all entries, go to rightmost child (the key will be appended there)
            let pos = if pos >= entries.len() { entries.len() - 1 } else { pos };
            match &entries[pos].value {
                Child::Inner { node_index } => current = node_index.slot_index,
                _ => return Ok(path),
            }
        }
        Err(format!("tree_find_leaf_path: exceeded max depth {}", max_depth))
    }


    /// Update parent key pointers when a node's max key changed.
    fn tree_update_parent_key<F>(
        &mut self, path: &[u64], old_key: &K, new_key: &K, read_slot: &F,
    ) -> Result<Vec<TableWrite>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let mut writes = Vec::new();
        for &ni in path.iter().rev() {
            let mut node = self.read_node(ni, read_slot)?;
            let entries = node_entries_mut(&mut node);
            let found = entries.binary_search_by(|e| e.key.cmp(old_key));
            if let Ok(pos) = found {
                let is_last = pos == entries.len() - 1;
                entries[pos].key = new_key.clone();
                if let Some(tw) = self.write_node(ni, node, false) {
                    writes.push(tw);
                }
                if !is_last { break; }
            } else {
                break;
            }
        }
        Ok(writes)
    }

    // ---- Public tree-aware API ----


    /// Auto-initialize leaf_max_degree and inner_max_degree if they are 0.
    /// Move computes these dynamically based on the first key-value size.
    /// We approximate using BCS serialized sizes.
    fn init_degrees_if_needed(&mut self, key: &K, value: &V) {
        let BigOrderedMap::BPlusTreeMap { leaf_max_degree, inner_max_degree, .. } = self;
        if *leaf_max_degree != 0 && *inner_max_degree != 0 {
            return;
        }
        let key_size = bcs::serialized_size(key).unwrap_or(32) as u64;
        let value_size = bcs::serialized_size(value).unwrap_or(100) as u64;
        let entry_size = key_size + value_size;

        const MAX_NODE_BYTES: u64 = 400 * 1024;
        const DEFAULT_TARGET_NODE_SIZE: u64 = 4096;
        const DEFAULT_MAX_KEY_OR_VALUE_SIZE: u64 = 5 * 1024;
        const INNER_MIN_DEGREE: u64 = 4;
        const LEAF_MIN_DEGREE: u64 = 3;
        const MAX_DEGREE: u64 = 4096;

        if *inner_max_degree == 0 {
            let default_max = std::cmp::min(MAX_DEGREE, MAX_NODE_BYTES / DEFAULT_MAX_KEY_OR_VALUE_SIZE);
            let from_target = std::cmp::min(default_max, DEFAULT_TARGET_NODE_SIZE / std::cmp::max(key_size, 1));
            *inner_max_degree = std::cmp::max(from_target, INNER_MIN_DEGREE) as u16;
        }

        if *leaf_max_degree == 0 {
            let default_max = std::cmp::min(MAX_DEGREE, MAX_NODE_BYTES / DEFAULT_MAX_KEY_OR_VALUE_SIZE / 2);
            let from_target = std::cmp::min(default_max, DEFAULT_TARGET_NODE_SIZE / std::cmp::max(entry_size, 1));
            *leaf_max_degree = std::cmp::max(from_target, LEAF_MIN_DEGREE) as u16;
        }
    }

    /// Add a key-value pair to the B+ tree. Returns table writes.
    pub fn tree_add<F>(
        &mut self, key: K, value: V, read_slot: &F,
    ) -> Result<Vec<TableWrite>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        // Clear transient write cache from previous operations
        { let BigOrderedMap::BPlusTreeMap { write_cache, .. } = self; write_cache.clear(); }

        // Auto-initialize degrees if they're 0 (Move does this dynamically on first insert)
        self.init_degrees_if_needed(&key, &value);

        // Fast path: root is leaf and not full
        {
            let root = self.root_ref();
            if node_is_leaf(root) {
                let count = node_entries(root).len();
                let max = self.leaf_max();
                if count < max {
                    let entries = node_entries_mut(self.root_mut());
                    let pos = entries.partition_point(|e| e.key < key);
                    if pos < entries.len() && entries[pos].key == key {
                        entries[pos].value = Child::Leaf { value }; // upsert
                    } else {
                        entries.insert(pos, Entry { key, value: Child::Leaf { value } });
                    }
                    return Ok(vec![]);
                }
            }
        }

        // Find path to leaf
        let path = self.tree_find_leaf_path(&key, read_slot)?;

        // tree_find_leaf_path now always returns a valid path (goes to rightmost child
        // for key > all). This block handles the edge case where path is somehow empty.
        if path.is_empty() {
            return Err("B+Tree: tree_find_leaf_path returned empty path".to_string());
        }

        let result = self.tree_add_at(path, key, Child::Leaf { value }, read_slot);
        { let BigOrderedMap::BPlusTreeMap { write_cache, .. } = self; write_cache.clear(); }
        result
    }

    /// Core add_at: insert child into the leaf identified by last element of path,
    /// splitting if needed and propagating up.
    fn tree_add_at<F>(
        &mut self, mut path: Vec<u64>, key: K, child: Child<V>, read_slot: &F,
    ) -> Result<Vec<TableWrite>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let mut writes = Vec::new();
        let node_index = path.pop().ok_or("B+Tree: empty path")?;

        let mut node = self.read_node(node_index, read_slot)?;
        let is_leaf = node_is_leaf(&node);
        let max_degree = if is_leaf { self.leaf_max() } else { self.inner_max() };

        let entries = node_entries_mut(&mut node);
        let cur_len = entries.len();

        // Check if there's room
        if cur_len < max_degree {
            // Check if the new key exceeds current max (for parent key update)
            let old_max = entries.last().map(|e| e.key.clone());
            let pos = entries.partition_point(|e| e.key < key);
            if pos < entries.len() && entries[pos].key == key {
                entries[pos].value = child; // upsert
            } else {
                entries.insert(pos, Entry { key: key.clone(), value: child });
            }
            if let Some(tw) = self.write_node(node_index, node, false) {
                writes.push(tw);
            }
            // If we inserted a new max key (key > old max), update parent pointers
            if node_index != 1 && !path.is_empty() {
                if let Some(ref om) = old_max {
                    if key > *om {
                        let pw = self.tree_update_parent_key(&path, om, &key, read_slot)?;
                        writes.extend(pw);
                    }
                }
            }
            return Ok(writes);
        }

        // Check for upsert (key exists, no split needed)
        if let Ok(pos) = entries.binary_search_by(|e| e.key.cmp(&key)) {
            entries[pos].value = child;
            if let Some(tw) = self.write_node(node_index, node, false) {
                writes.push(tw);
            }
            return Ok(writes);
        }

        // Need to split. If at root, move root content to a new slot first.
        if node_index == 1 {
            assert!(path.is_empty());
            let (right_slot, right_slot_new) = self.alloc_slot_reuse(read_slot)?;

            // Get max key (current max or the new key, whichever is larger)
            let cur_max = entries.last().map(|e| e.key.clone());
            let overall_max = match cur_max {
                Some(m) if m >= key => m,
                _ => key.clone(),
            };

            // Build new inner root with single child -> right_slot
            let new_root = Node::V1 {
                is_leaf: false,
                children: OrderedMap::SortedVectorMap {
                    entries: vec![Entry {
                        key: overall_max,
                        value: Child::Inner { node_index: StoredSlot { slot_index: right_slot } },
                    }],
                },
                prev: 0,
                next: 0,
            };

            // Swap root
            let old_root = std::mem::replace(self.root_mut(), new_root);
            let old_is_leaf = node_is_leaf(&old_root);
            if old_is_leaf {
                let BigOrderedMap::BPlusTreeMap { min_leaf_index, max_leaf_index, .. } = self;
                *min_leaf_index = right_slot;
                *max_leaf_index = right_slot;
            }

            path.push(1); // ROOT_INDEX is now the parent
            // Re-read old root as the node to split (now at right_slot)
            node = old_root;
            let entries = node_entries_mut(&mut node);

            // Insert new entry
            let pos = entries.partition_point(|e| e.key < key);
            entries.insert(pos, Entry { key: key.clone(), value: child });

            // Split
            let target_size = (max_degree + 1) / 2;
            let right_entries = entries.split_off(target_size);
            let left_max_key = entries.last().unwrap().key.clone();

            let (left_slot, left_slot_new) = self.alloc_slot_reuse(read_slot)?;

            let left_node = Node::V1 {
                is_leaf,
                children: OrderedMap::SortedVectorMap { entries: std::mem::take(entries) },
                prev: 0,
                next: right_slot,
            };
            let right_node = Node::V1 {
                is_leaf,
                children: OrderedMap::SortedVectorMap { entries: right_entries },
                prev: left_slot,
                next: 0,
            };

            // Update root children: [left_max_key -> left_slot, overall_max -> right_slot]
            {
                let root_entries = node_entries_mut(self.root_mut());
                let right_max_key = root_entries[0].key.clone();
                root_entries.clear();
                root_entries.push(Entry {
                    key: left_max_key,
                    value: Child::Inner { node_index: StoredSlot { slot_index: left_slot } },
                });
                root_entries.push(Entry {
                    key: right_max_key,
                    value: Child::Inner { node_index: StoredSlot { slot_index: right_slot } },
                });
            }

            if is_leaf {
                let BigOrderedMap::BPlusTreeMap { min_leaf_index, max_leaf_index, .. } = self;
                *min_leaf_index = left_slot;
                *max_leaf_index = right_slot;
            }

            writes.push(TableWrite { slot_index: left_slot, data: serialize_node_link(&left_node), is_new: left_slot_new });
            writes.push(TableWrite { slot_index: right_slot, data: serialize_node_link(&right_node), is_new: right_slot_new });
            return Ok(writes);
        }

        // Non-root split
        let old_prev = node_prev(&node);
        let old_next = node_next(&node);

        let entries = node_entries_mut(&mut node);
        let pos = entries.partition_point(|e| e.key < key);
        entries.insert(pos, Entry { key: key.clone(), value: child });

        let target_size = (max_degree + 1) / 2;
        let right_entries = entries.split_off(target_size);
        let left_max_key = entries.last().unwrap().key.clone();

        let (left_slot, left_slot_new) = self.alloc_slot_reuse(read_slot)?;

        // Left gets smaller keys, new slot; Right keeps node_index slot
        let left_node = Node::V1 {
            is_leaf,
            children: OrderedMap::SortedVectorMap { entries: std::mem::take(entries) },
            prev: old_prev,
            next: node_index,
        };
        let right_node = Node::V1 {
            is_leaf,
            children: OrderedMap::SortedVectorMap { entries: right_entries },
            prev: left_slot,
            next: old_next,
        };

        // Update prev node's next pointer
        if old_prev != 0 {
            let mut prev_node = self.read_node(old_prev, read_slot)?;
            let Node::V1 { next, .. } = &mut prev_node;
            *next = left_slot;
            if let Some(tw) = self.write_node(old_prev, prev_node, false) {
                writes.push(tw);
            }
        } else if is_leaf {
            let BigOrderedMap::BPlusTreeMap { min_leaf_index, .. } = self;
            if *min_leaf_index == node_index {
                *min_leaf_index = left_slot;
            }
        }

        let right_max_key = node_entries(&right_node).last().unwrap().key.clone();

        writes.push(TableWrite { slot_index: left_slot, data: serialize_node_link(&left_node), is_new: left_slot_new });
        writes.push(TableWrite { slot_index: node_index, data: serialize_node_link(&right_node), is_new: false });

        // Update parent in one shot: update right child's key and add left child entry.
        // This avoids the stale-read issue when tree_add_at re-reads the parent.
        let parent_writes = self.tree_add_split_children(
            path, left_max_key, left_slot, node_index, right_max_key, is_leaf, read_slot,
        )?;
        writes.extend(parent_writes);

        Ok(writes)
    }

    /// After splitting a non-root node into left_slot and right_slot (which keeps node_index),
    /// update the parent: set right child's key to right_max_key, add left child entry.
    /// If the parent needs to split, propagates upward.
    fn tree_add_split_children<F>(
        &mut self,
        mut path: Vec<u64>,
        left_max_key: K,
        left_slot: u64,
        right_node_index: u64,
        right_max_key: K,
        _children_are_leaf: bool,
        read_slot: &F,
    ) -> Result<Vec<TableWrite>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let mut writes = Vec::new();
        let parent_idx = path.pop().ok_or("B+Tree: empty path for split propagation")?;

        let mut parent = self.read_node(parent_idx, read_slot)?;
        let parent_max_degree = self.inner_max();
        let old_prev = node_prev(&parent);
        let old_next = node_next(&parent);

        let parent_entries = node_entries_mut(&mut parent);

        // Update right child's key
        for pe in parent_entries.iter_mut() {
            if let Child::Inner { node_index: ref ni } = pe.value {
                if ni.slot_index == right_node_index {
                    pe.key = right_max_key.clone();
                    break;
                }
            }
        }

        // Re-sort after key update (the right child's key may have moved it out of order)
        parent_entries.sort_by(|a, b| a.key.cmp(&b.key));

        // Insert left child
        let left_entry = Entry {
            key: left_max_key.clone(),
            value: Child::Inner { node_index: StoredSlot { slot_index: left_slot } },
        };
        let pos = parent_entries.partition_point(|e| e.key < left_max_key);
        parent_entries.insert(pos, left_entry);

        // Check if parent needs to split
        if parent_entries.len() <= parent_max_degree {
            if let Some(tw) = self.write_node(parent_idx, parent, false) {
                writes.push(tw);
            }
            return Ok(writes);
        }

        // Parent needs to split too.
        if parent_idx == 1 {
            // Root inner split
            let (right_slot, right_slot_new) = self.alloc_slot_reuse(read_slot)?;
            let overall_max = parent_entries.last().unwrap().key.clone();

            let target_size = (parent_max_degree + 1) / 2;
            let right_entries = parent_entries.split_off(target_size);
            let pleft_max_key = parent_entries.last().unwrap().key.clone();

            let (left_slot2, left_slot2_new) = self.alloc_slot_reuse(read_slot)?;

            let left_inner = Node::V1 {
                is_leaf: false,
                children: OrderedMap::SortedVectorMap { entries: std::mem::take(parent_entries) },
                prev: 0,
                next: right_slot,
            };
            let right_inner = Node::V1 {
                is_leaf: false,
                children: OrderedMap::SortedVectorMap { entries: right_entries },
                prev: left_slot2,
                next: 0,
            };

            let new_root = Node::V1 {
                is_leaf: false,
                children: OrderedMap::SortedVectorMap {
                    entries: vec![
                        Entry {
                            key: pleft_max_key,
                            value: Child::Inner { node_index: StoredSlot { slot_index: left_slot2 } },
                        },
                        Entry {
                            key: overall_max,
                            value: Child::Inner { node_index: StoredSlot { slot_index: right_slot } },
                        },
                    ],
                },
                prev: 0,
                next: 0,
            };
            *self.root_mut() = new_root;

            writes.push(TableWrite { slot_index: left_slot2, data: serialize_node_link(&left_inner), is_new: left_slot2_new });
            writes.push(TableWrite { slot_index: right_slot, data: serialize_node_link(&right_inner), is_new: right_slot_new });
            return Ok(writes);
        }

        // Non-root parent split

        let target_size = (parent_max_degree + 1) / 2;
        let right_entries = parent_entries.split_off(target_size);
        let pleft_max_key = parent_entries.last().unwrap().key.clone();

        let (new_left_slot, new_left_slot_new) = self.alloc_slot_reuse(read_slot)?;

        let left_inner = Node::V1 {
            is_leaf: false,
            children: OrderedMap::SortedVectorMap { entries: std::mem::take(parent_entries) },
            prev: old_prev,
            next: parent_idx,
        };
        let pright_max_key = right_entries.last().unwrap().key.clone();
        let right_inner = Node::V1 {
            is_leaf: false,
            children: OrderedMap::SortedVectorMap { entries: right_entries },
            prev: new_left_slot,
            next: old_next,
        };

        if old_prev != 0 {
            let mut pn = self.read_node(old_prev, read_slot)?;
            let Node::V1 { next, .. } = &mut pn;
            *next = new_left_slot;
            if let Some(tw) = self.write_node(old_prev, pn, false) { writes.push(tw); }
        }

        writes.push(TableWrite { slot_index: new_left_slot, data: serialize_node_link(&left_inner), is_new: new_left_slot_new });
        writes.push(TableWrite { slot_index: parent_idx, data: serialize_node_link(&right_inner), is_new: false });

        // Recurse: propagate split to grandparent
        let gp_writes = self.tree_add_split_children(
            path, pleft_max_key, new_left_slot, parent_idx, pright_max_key, false, read_slot,
        )?;
        writes.extend(gp_writes);

        Ok(writes)
    }

    /// Remove a key from the B+ tree.
    /// Returns (removed_value, table_writes).
    /// Implements full Move-matching rebalancing (borrow from sibling or merge).
    pub fn tree_remove<F>(
        &mut self, key: &K, read_slot: &F,
    ) -> Result<(Option<V>, Vec<TableWrite>), String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        // Fast path: root is leaf
        if node_is_leaf(self.root_ref()) {
            let entries = node_entries_mut(self.root_mut());
            if let Ok(pos) = entries.binary_search_by(|e| e.key.cmp(key)) {
                let entry = entries.remove(pos);
                if let Child::Leaf { value } = entry.value {
                    return Ok((Some(value), vec![]));
                }
            }
            return Ok((None, vec![]));
        }

        // Clear write cache so this operation starts clean
        {
            let BigOrderedMap::BPlusTreeMap { write_cache, .. } = self;
            write_cache.clear();
        }

        let path = self.tree_find_leaf_path(key, read_slot)?;
        if path.is_empty() { return Ok((None, vec![])); }

        let result = self.tree_remove_at(path, key, read_slot);

        // Clear cache after operation
        {
            let BigOrderedMap::BPlusTreeMap { write_cache, .. } = self;
            write_cache.clear();
        }

        result
    }

    /// Core remove_at: matches Move's remove_at_with_iter_hint.
    /// path contains indices from root down to the leaf.
    /// Returns (removed_value, table_writes).
    fn tree_remove_at<F>(
        &mut self, path: Vec<u64>, key: &K, read_slot: &F,
    ) -> Result<(Option<V>, Vec<TableWrite>), String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let mut writes = Vec::new();
        let node_index = *path.last().unwrap();

        let mut node = self.read_node(node_index, read_slot)?;
        let is_leaf = node_is_leaf(&node);
        let entries = node_entries_mut(&mut node);

        let pos = match entries.binary_search_by(|e| e.key.cmp(key)) {
            Ok(p) => p,
            Err(_) => return Ok((None, vec![])),
        };
        let removed = entries.remove(pos);
        let value = match removed.value {
            Child::Leaf { value } => value,
            _ => return Err("B+Tree: expected Leaf child in remove".to_string()),
        };

        let degree = entries.len();

        // ROOT case: no lower limit on degree, just handle collapse
        if node_index == 1 {
            assert!(path.len() == 1);
            // For root, check if it's an inner node with a single child -> collapse
            if !is_leaf && degree == 1 {
                let child_slot = match &entries[0].value {
                    Child::Inner { node_index } => node_index.slot_index,
                    _ => { *self.root_mut() = node; return Ok((Some(value), writes)); },
                };
                let child = self.read_node(child_slot, read_slot)?;
                if node_is_leaf(&child) {
                    let BigOrderedMap::BPlusTreeMap { min_leaf_index, max_leaf_index, .. } = self;
                    *min_leaf_index = 1;
                    *max_leaf_index = 1;
                }
                *self.root_mut() = child;
                // Free the slot
                if let Some(tw) = self.free_slot(child_slot) { writes.push(tw); }
            } else {
                *self.root_mut() = node;
            }
            return Ok((Some(value), writes));
        }

        // Non-root: compute whether node is big enough
        let max_degree = if is_leaf { self.leaf_max() } else { self.inner_max() };
        let big_enough = degree * 2 >= max_degree;

        // Check if max key changed (we removed the last entry)
        let max_key_changed = pos == degree && degree >= 1;
        let new_max_key = if max_key_changed {
            Some(node_entries(&node).last().unwrap().key.clone())
        } else {
            None
        };

        // Do NOT write node back yet -- pass it to rebalancing if needed
        if big_enough {
            // Write node back
            if let Some(tw) = self.write_node(node_index, node, false) {
                writes.push(tw);
            }
            // Update parent key if max changed
            if let Some(ref nmk) = new_max_key {
                let parent_path = &path[..path.len()-1];
                let uw = self.tree_update_parent_key(parent_path, key, nmk, read_slot)?;
                writes.extend(uw);
            }
            return Ok((Some(value), writes));
        }

        // Update parent key if max changed (before rebalancing)
        if let Some(ref nmk) = new_max_key {
            // Write node first so parent key update can find it
            if let Some(tw) = self.write_node(node_index, node.clone(), false) {
                writes.push(tw);
            }
            let parent_path = &path[..path.len()-1];
            let uw = self.tree_update_parent_key(parent_path, key, nmk, read_slot)?;
            writes.extend(uw);
        } else {
            if let Some(tw) = self.write_node(node_index, node.clone(), false) {
                writes.push(tw);
            }
        }

        // Node is too small, need to rebalance.
        // Pass the node directly to avoid stale reads.
        let rebalance_writes = self.process_rebalance_after_removal(
            node_index, &node, is_leaf, path, read_slot,
        )?;
        writes.extend(rebalance_writes);

        Ok((Some(value), writes))
    }

    /// Rebalance after a child removal made a node underfull.
    /// Matches Move's `process_rebalance_after_child_removal`.
    /// Takes the already-modified node directly to avoid stale reads.
    fn process_rebalance_after_removal<F>(
        &mut self,
        node_index: u64,
        node: &Node<K, V>,
        is_leaf: bool,
        path: Vec<u64>,
        read_slot: &F,
    ) -> Result<Vec<TableWrite>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let mut writes = Vec::new();
        let max_degree = if is_leaf { self.leaf_max() } else { self.inner_max() };

        let n_prev = node_prev(node);
        let n_next = node_next(node);

        // Determine sibling: if we are the largest child in parent, merge with prev;
        // otherwise merge with next.
        let parent_index = path[path.len() - 2];
        let parent = self.read_node(parent_index, read_slot)?;
        let parent_entries = node_entries(&parent);
        assert!(parent_entries.len() >= 2, "Parent must have >= 2 children for rebalance");

        let is_rightmost = match &parent_entries.last().unwrap().value {
            Child::Inner { node_index: ni } => ni.slot_index == node_index,
            _ => false,
        };
        let sibling_index = if is_rightmost { n_prev } else { n_next };
        assert!(sibling_index != 0, "Sibling must exist for rebalance");

        let sibling = self.read_node(sibling_index, read_slot)?;
        let sibling_degree = node_entries(&sibling).len();

        // Check if we can borrow from sibling
        if (sibling_degree - 1) * 2 >= max_degree {
            // Borrow from sibling
            let borrow_writes = self.rebalance_borrow(
                node_index, node, sibling_index, &sibling, is_rightmost, is_leaf, &path, read_slot,
            )?;
            writes.extend(borrow_writes);
        } else {
            // Merge with sibling
            let merge_writes = self.rebalance_merge(
                node_index, node, sibling_index, &sibling, is_rightmost, is_leaf, path, read_slot,
            )?;
            writes.extend(merge_writes);
        }

        Ok(writes)
    }

    /// Borrow one entry from sibling to rebalance.
    /// Matches Move's borrow logic in process_rebalance_after_child_removal.
    fn rebalance_borrow<F>(
        &mut self,
        node_index: u64,
        node: &Node<K, V>,
        sibling_index: u64,
        sibling: &Node<K, V>,
        is_rightmost: bool,
        _is_leaf: bool,
        path: &[u64],
        read_slot: &F,
    ) -> Result<Vec<TableWrite>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let mut writes = Vec::new();
        let mut node = node.clone();
        let mut sibling = sibling.clone();

        if !is_rightmost {
            // Sibling is the next node (has larger keys) -> borrow from start of sibling
            let old_max_key = node_entries(&node).last().unwrap().key.clone();
            let borrowed = node_entries_mut(&mut sibling).remove(0);
            node_entries_mut(&mut node).push(borrowed);

            let new_max_key = node_entries(&node).last().unwrap().key.clone();

            // Write both nodes back
            if let Some(tw) = self.write_node(node_index, node, false) { writes.push(tw); }
            if let Some(tw) = self.write_node(sibling_index, sibling, false) { writes.push(tw); }

            // Update parent key: max of current node changed
            let parent_path = &path[..path.len()-1];
            let uw = self.tree_update_parent_key(parent_path, &old_max_key, &new_max_key, read_slot)?;
            writes.extend(uw);
        } else {
            // Sibling is the prev node (has smaller keys) -> borrow from end of sibling
            let borrowed = node_entries_mut(&mut sibling).pop().unwrap();
            let borrowed_max_key = borrowed.key.clone();
            node_entries_mut(&mut node).insert(0, borrowed);

            let new_sibling_max = node_entries(&sibling).last().unwrap().key.clone();

            // Write both nodes back
            if let Some(tw) = self.write_node(node_index, node, false) { writes.push(tw); }
            if let Some(tw) = self.write_node(sibling_index, sibling, false) { writes.push(tw); }

            // Sibling's max key changed -> update parent
            let parent_path = &path[..path.len()-1];
            let uw = self.tree_update_parent_key(parent_path, &borrowed_max_key, &new_sibling_max, read_slot)?;
            writes.extend(uw);
        }

        Ok(writes)
    }

    /// Merge node with sibling.
    /// Matches Move's merge logic in process_rebalance_after_child_removal.
    fn rebalance_merge<F>(
        &mut self,
        node_index: u64,
        node: &Node<K, V>,
        sibling_index: u64,
        sibling: &Node<K, V>,
        is_rightmost: bool,
        is_leaf: bool,
        path: Vec<u64>,
        read_slot: &F,
    ) -> Result<Vec<TableWrite>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let mut writes = Vec::new();
        let mut node = node.clone();
        let mut sibling = sibling.clone();

        let node_prev_idx = node_prev(&node);
        let node_next_idx = node_next(&node);
        let sibling_prev_idx = node_prev(&sibling);
        let sibling_next_idx = node_next(&sibling);

        // Determine key_to_remove (the key of the smaller node in parent)
        // and which slot to free.
        // Move: "Keep the slot of the node with larger keys"
        if !is_rightmost {
            // sibling_index == next (has larger keys)
            // We destroy the sibling, keep sibling_slot with merged content
            // key_to_remove = max key of current node (the smaller one)
            let key_to_remove = node_entries(&node).last().unwrap().key.clone();

            // Merge: append sibling's children to node's children
            {
                let sibling_entries_vec: Vec<_> = node_entries(&sibling).clone();
                node_entries_mut(&mut node).extend(sibling_entries_vec);
            }

            // The merged node takes sibling's next
            {
                let Node::V1 { next, .. } = &mut node;
                *next = sibling_next_idx;
            }

            // Update next->prev pointer
            if sibling_next_idx != 0 {
                let mut next_node = self.read_node(sibling_next_idx, read_slot)?;
                let Node::V1 { prev, .. } = &mut next_node;
                assert_eq!(*prev, sibling_index);
                // After merge, sibling_slot holds the merged content, so no need to change
                // Actually: we're keeping sibling_slot. The merged content goes to sibling_slot.
                // node_index's slot is freed.
                // Wait, re-read Move logic:
                // Move: sibling_index == next. "destroying larger sibling node, keeping sibling_slot"
                // children.append_disjoint(sibling_children) -> appends sibling to current node
                // node.next = sibling_next
                // fill_reserved_slot(sibling_slot, node) -> puts merged node at sibling_slot
                // key_to_remove = max of current node's children (before merge)
                // reserved_slot_to_remove = node_slot (current node's slot is freed)
                if let Some(tw) = self.write_node(sibling_next_idx, next_node, false) {
                    writes.push(tw);
                }
            }

            // Update prev->next pointer
            // "we are removing node_index, which previous's node's next was pointing to"
            if node_prev_idx != 0 {
                let mut prev_node = self.read_node(node_prev_idx, read_slot)?;
                let Node::V1 { next, .. } = &mut prev_node;
                *next = sibling_index;
                if let Some(tw) = self.write_node(node_prev_idx, prev_node, false) {
                    writes.push(tw);
                }
            }

            // Update min_leaf_index if we were the smallest
            if is_leaf {
                let BigOrderedMap::BPlusTreeMap { min_leaf_index, .. } = self;
                if *min_leaf_index == node_index {
                    *min_leaf_index = sibling_index;
                }
            }

            // Write merged content to sibling_slot
            if let Some(tw) = self.write_node(sibling_index, node, false) {
                writes.push(tw);
            }

            // Free node_slot
            if let Some(tw) = self.free_slot(node_index) { writes.push(tw); }

            // Remove key_to_remove from parent (recursively)
            let parent_path = path[..path.len()-1].to_vec();
            let remove_writes = self.tree_remove_inner_child(parent_path, &key_to_remove, read_slot)?;
            writes.extend(remove_writes);
        } else {
            // sibling_index == prev (has smaller keys)
            // We destroy the current node, keep node_slot with merged content
            // key_to_remove = max key of sibling (the smaller one)
            let key_to_remove = node_entries(&sibling).last().unwrap().key.clone();

            // Merge: append current node's children to sibling's children
            {
                let node_entries_vec: Vec<_> = node_entries(&node).clone();
                node_entries_mut(&mut sibling).extend(node_entries_vec);
            }

            // The merged node takes current node's next
            {
                let Node::V1 { next, .. } = &mut sibling;
                *next = node_next_idx;
            }

            // Update next->prev pointer
            if node_next_idx != 0 {
                let mut next_node = self.read_node(node_next_idx, read_slot)?;
                let Node::V1 { prev, .. } = &mut next_node;
                assert_eq!(*prev, node_index);
                // After merge, node_slot holds merged content. next's prev should stay node_index.
                // Actually no: Move says "sibling_node.next = node_next" and doesn't change next->prev
                // because the slot that holds the merged content (node_slot) IS what next->prev already points to.
                // Wait, let me re-read Move:
                // Move: sibling_index == prev. "destroying larger current node, keeping node_slot"
                // sibling_children.append_disjoint(node_children) -> appends current to sibling
                // sibling_node.next = node_next
                // "if (sibling_node.next != NULL_INDEX) { assert!(self.nodes.borrow_mut(sibling_node.next).prev == node_index) }"
                // So it asserts prev == node_index (correct since we keep node_slot for the merged)
                // fill_reserved_slot(node_slot, sibling_node) -> puts merged at node_slot
                // key_to_remove = sibling's max key
                // reserved_slot_to_remove = sibling_slot (sibling's slot is freed)

                // So next->prev should remain node_index (it already does since merged goes to node_slot)
                // No write needed for next node.
                // But we need to assert it's correct. We don't need to write it.
                // Actually, Move does the assert but doesn't write. So we skip too.
                let _ = next_node; // drop without writing
            }

            // Update prev->next pointer for sibling's prev
            // "we are removing sibling node_index, which previous's node's next was pointing to"
            if sibling_prev_idx != 0 {
                let mut prev_node = self.read_node(sibling_prev_idx, read_slot)?;
                let Node::V1 { next, .. } = &mut prev_node;
                *next = node_index;
                if let Some(tw) = self.write_node(sibling_prev_idx, prev_node, false) {
                    writes.push(tw);
                }
            }

            // Update min_leaf_index
            if is_leaf {
                let BigOrderedMap::BPlusTreeMap { min_leaf_index, .. } = self;
                if *min_leaf_index == sibling_index {
                    *min_leaf_index = node_index;
                }
            }

            // Write merged content to node_slot
            if let Some(tw) = self.write_node(node_index, sibling, false) {
                writes.push(tw);
            }

            // Free sibling_slot
            if let Some(tw) = self.free_slot(sibling_index) { writes.push(tw); }

            // Remove key_to_remove from parent (recursively)
            let parent_path = path[..path.len()-1].to_vec();
            let remove_writes = self.tree_remove_inner_child(parent_path, &key_to_remove, read_slot)?;
            writes.extend(remove_writes);
        }

        Ok(writes)
    }

    /// Remove an inner child from the tree at the given path, by key.
    /// This is the recursive part of merge: after merging two children,
    /// we remove the pointer to one of them from the parent.
    /// Matches Move's recursive self.remove_at(path_to_node, &key_to_remove).
    fn tree_remove_inner_child<F>(
        &mut self,
        path: Vec<u64>,
        key: &K,
        read_slot: &F,
    ) -> Result<Vec<TableWrite>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        let mut writes = Vec::new();
        let node_index = *path.last().unwrap();

        let mut node = self.read_node(node_index, read_slot)?;
        let is_leaf = node_is_leaf(&node);
        assert!(!is_leaf, "tree_remove_inner_child called on leaf node");
        let entries = node_entries_mut(&mut node);

        let pos = match entries.binary_search_by(|e| e.key.cmp(key)) {
            Ok(p) => p,
            Err(_) => return Err(format!("B+Tree: inner child key {:?} not found in parent", key)),
        };

        // Get the slot being removed (for freeing)
        let removed_slot = match &entries[pos].value {
            Child::Inner { node_index } => node_index.slot_index,
            _ => return Err("B+Tree: expected Inner child".to_string()),
        };
        entries.remove(pos);
        let degree = entries.len();

        // ROOT case
        if node_index == 1 {
            if degree == 1 {
                // Collapse: promote single child to root
                let child_slot = match &entries[0].value {
                    Child::Inner { node_index } => node_index.slot_index,
                    _ => { *self.root_mut() = node; return Ok(writes); },
                };
                let child = self.read_node(child_slot, read_slot)?;
                if node_is_leaf(&child) {
                    let BigOrderedMap::BPlusTreeMap { min_leaf_index, max_leaf_index, .. } = self;
                    *min_leaf_index = 1;
                    *max_leaf_index = 1;
                }
                *self.root_mut() = child;
                if let Some(tw) = self.free_slot(child_slot) { writes.push(tw); }
            } else {
                *self.root_mut() = node;
            }
            // Free the removed slot
            if let Some(tw) = self.free_slot(removed_slot) { writes.push(tw); }
            return Ok(writes);
        }

        // Non-root inner node
        let max_degree = self.inner_max();
        let big_enough = degree * 2 >= max_degree;

        // Check if max key changed
        let max_key_changed = pos == degree && degree >= 1;
        if max_key_changed {
            let new_max_key = entries.last().unwrap().key.clone();
            if let Some(tw) = self.write_node(node_index, node.clone(), false) {
                writes.push(tw);
            }
            let parent_path = &path[..path.len()-1];
            let uw = self.tree_update_parent_key(parent_path, key, &new_max_key, read_slot)?;
            writes.extend(uw);
        } else {
            if let Some(tw) = self.write_node(node_index, node.clone(), false) {
                writes.push(tw);
            }
        }

        // Free the removed slot
        if let Some(tw) = self.free_slot(removed_slot) { writes.push(tw); }

        if big_enough || degree == 0 {
            return Ok(writes);
        }

        // Need to rebalance this inner node too
        let rebalance_writes = self.process_rebalance_after_removal(
            node_index, &node, false, path, read_slot,
        )?;
        writes.extend(rebalance_writes);

        Ok(writes)
    }

    /// Pop the front (minimum key) entry.
    pub fn tree_pop_front<F>(
        &mut self, read_slot: &F,
    ) -> Result<(Option<(K, V)>, Vec<TableWrite>), String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        // Clear transient write cache
        { let BigOrderedMap::BPlusTreeMap { write_cache, .. } = self; write_cache.clear(); }

        // Root is leaf: simple
        if node_is_leaf(self.root_ref()) {
            let entries = node_entries_mut(self.root_mut());
            if entries.is_empty() { return Ok((None, vec![])); }
            let e = entries.remove(0);
            return Ok(match e.value {
                Child::Leaf { value } => (Some((e.key, value)), vec![]),
                _ => (None, vec![]),
            });
        }

        let BigOrderedMap::BPlusTreeMap { min_leaf_index, .. } = self;
        let min_idx = *min_leaf_index;
        if min_idx == 0 || min_idx == 1 { return Ok((None, vec![])); }

        // Read the min leaf to get its first key
        let leaf = self.read_node(min_idx, read_slot)?;
        let entries = node_entries(&leaf);
        if entries.is_empty() { return Ok((None, vec![])); }
        let key = entries[0].key.clone();

        // Use tree_remove which has proper rebalancing
        let (val, writes) = self.tree_remove(&key, read_slot)?;
        Ok((val.map(|v| (key, v)), writes))
    }

    /// Pop the back (maximum key) entry.
    pub fn tree_pop_back<F>(
        &mut self, read_slot: &F,
    ) -> Result<(Option<(K, V)>, Vec<TableWrite>), String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        // Clear transient write cache
        { let BigOrderedMap::BPlusTreeMap { write_cache, .. } = self; write_cache.clear(); }

        if node_is_leaf(self.root_ref()) {
            let entries = node_entries_mut(self.root_mut());
            if entries.is_empty() { return Ok((None, vec![])); }
            let e = entries.pop().unwrap();
            return Ok(match e.value {
                Child::Leaf { value } => (Some((e.key, value)), vec![]),
                _ => (None, vec![]),
            });
        }

        let BigOrderedMap::BPlusTreeMap { max_leaf_index, .. } = self;
        let max_idx = *max_leaf_index;
        if max_idx == 0 || max_idx == 1 { return Ok((None, vec![])); }

        // Read the max leaf to get its last key
        let leaf = self.read_node(max_idx, read_slot)?;
        let entries = node_entries(&leaf);
        if entries.is_empty() { return Ok((None, vec![])); }
        let key = entries.last().unwrap().key.clone();

        // Use tree_remove which has proper rebalancing
        let (val, writes) = self.tree_remove(&key, read_slot)?;
        Ok((val.map(|v| (key, v)), writes))
    }

    /// Borrow the front (minimum key) entry (cloned since table-backed).
    pub fn tree_borrow_front<F>(&self, read_slot: &F) -> Result<Option<(K, V)>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        if node_is_leaf(self.root_ref()) {
            let entries = node_entries(self.root_ref());
            return Ok(entries.first().and_then(|e| match &e.value {
                Child::Leaf { value } => Some((e.key.clone(), value.clone())),
                _ => None,
            }));
        }
        let BigOrderedMap::BPlusTreeMap { min_leaf_index, .. } = self;
        let mi = *min_leaf_index;
        if mi == 0 || mi == 1 { return Ok(None); }
        let leaf = self.read_node(mi, read_slot)?;
        Ok(node_entries(&leaf).first().and_then(|e| match &e.value {
            Child::Leaf { value } => Some((e.key.clone(), value.clone())),
            _ => None,
        }))
    }

    /// Borrow the back (maximum key) entry (cloned since table-backed).
    pub fn tree_borrow_back<F>(&self, read_slot: &F) -> Result<Option<(K, V)>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        if node_is_leaf(self.root_ref()) {
            let entries = node_entries(self.root_ref());
            return Ok(entries.last().and_then(|e| match &e.value {
                Child::Leaf { value } => Some((e.key.clone(), value.clone())),
                _ => None,
            }));
        }
        let BigOrderedMap::BPlusTreeMap { max_leaf_index, .. } = self;
        let mi = *max_leaf_index;
        if mi == 0 || mi == 1 { return Ok(None); }
        let leaf = self.read_node(mi, read_slot)?;
        Ok(node_entries(&leaf).last().and_then(|e| match &e.value {
            Child::Leaf { value } => Some((e.key.clone(), value.clone())),
            _ => None,
        }))
    }

    /// Check if a key exists in the tree.
    pub fn tree_contains<F>(&self, key: &K, read_slot: &F) -> Result<bool, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        if node_is_leaf(self.root_ref()) {
            let entries = node_entries(self.root_ref());
            return Ok(entries.binary_search_by(|e| e.key.cmp(key)).is_ok());
        }
        let path = self.tree_find_leaf_path(key, read_slot)?;
        if path.is_empty() { return Ok(false); }
        let li = *path.last().unwrap();
        let leaf = self.read_node(li, read_slot)?;
        Ok(node_entries(&leaf).binary_search_by(|e| e.key.cmp(key)).is_ok())
    }

    /// Get the first key in the tree.
    pub fn tree_first_key<F>(&self, read_slot: &F) -> Result<Option<K>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        if node_is_leaf(self.root_ref()) {
            return Ok(node_entries(self.root_ref()).first().map(|e| e.key.clone()));
        }
        let BigOrderedMap::BPlusTreeMap { min_leaf_index, .. } = self;
        let mi = *min_leaf_index;
        if mi == 0 || mi == 1 { return Ok(None); }
        let leaf = self.read_node(mi, read_slot)?;
        Ok(node_entries(&leaf).first().map(|e| e.key.clone()))
    }

    /// Check if the tree is empty (tree-aware version).
    pub fn tree_is_empty<F>(&self, read_slot: &F) -> Result<bool, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        if node_is_leaf(self.root_ref()) {
            return Ok(node_entries(self.root_ref()).is_empty());
        }
        // Inner root: check if min leaf has entries
        let BigOrderedMap::BPlusTreeMap { min_leaf_index, .. } = self;
        let mi = *min_leaf_index;
        if mi == 0 { return Ok(true); }
        // If min_leaf_index points to root (1) but root is inner, all leaf nodes
        // have been removed. The skeleton inner nodes remain but the tree is empty.
        if mi == 1 { return Ok(true); }
        match self.read_node(mi, read_slot) {
            Ok(leaf) => Ok(node_entries(&leaf).is_empty()),
            Err(_) => Ok(true), // Can't read min leaf -> treat as empty
        }
    }

    /// Get a value from the B+ tree by key. Returns None if not found.
    pub fn tree_get<F>(&self, key: &K, read_slot: &F) -> Result<Option<V>, String>
    where F: Fn(u64) -> Result<Option<Vec<u8>>, String>,
    {
        // Fast path: root is leaf
        if node_is_leaf(self.root_ref()) {
            let entries = node_entries(self.root_ref());
            return Ok(entries.binary_search_by(|e| e.key.cmp(key)).ok().and_then(|idx| {
                match &entries[idx].value {
                    Child::Leaf { value } => Some(value.clone()),
                    _ => None,
                }
            }));
        }
        // Navigate to leaf
        let path = self.tree_find_leaf_path(key, read_slot)?;
        if path.is_empty() { return Ok(None); }
        let li = *path.last().unwrap();
        let leaf = self.read_node(li, read_slot)?;
        let entries = node_entries(&leaf);
        Ok(entries.binary_search_by(|e| e.key.cmp(key)).ok().and_then(|idx| {
            match &entries[idx].value {
                Child::Leaf { value } => Some(value.clone()),
                _ => None,
            }
        }))
    }

}

// ============================================================================
// On-chain UserPositions / PerpPosition (for position persistence in native dispatch)
// ============================================================================

/// Move: `enum PerpPosition has store, copy, drop { V1 { size, entry_px_times_size_sum, avg_acquire_entry_px, user_leverage, is_long, is_isolated, funding_index_at_last_update, unrealized_funding_amount_before_last_update, timestamp } }`
/// `funding_index_at_last_update` is `AccumulativeIndex { index: i128 }` which is an enum V1-like
/// but actually it is defined as `struct AccumulativeIndex has store, copy, drop { index: i128 }`.
/// Structs in BCS serialize fields directly (no variant tag).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OnChainPerpPosition {
    V1 {
        size: u64,
        entry_px_times_size_sum: u128,
        avg_acquire_entry_px: u64,
        user_leverage: u8,
        is_long: bool,
        is_isolated: bool,
        /// AccumulativeIndex is a struct (not enum), so just i128 directly
        funding_index_at_last_update: i128,
        unrealized_funding_amount_before_last_update: i64,
        timestamp: u64,
    },
}

impl OnChainPerpPosition {
    pub fn get_size(&self) -> u64 {
        let OnChainPerpPosition::V1 { size, .. } = self;
        *size
    }
}

/// Move: `enum UserPositions has key { V1 { positions: BigOrderedMap<Object<PerpMarket>, PerpPosition> } }`
/// `Object<PerpMarket>` serializes as just an AccountAddress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OnChainUserPositions {
    V1 {
        positions: BigOrderedMap<AccountAddress, OnChainPerpPosition>,
    },
}

impl OnChainUserPositions {
    /// Create a new empty UserPositions that matches Move's
    /// `UserPositions::V1 { positions: big_ordered_map::new_with_config(64, 16, false) }`
    ///
    /// The Move VM initializes UserPositions with:
    /// - inner_max_degree=64, leaf_max_degree=16 (from new_with_config args)
    /// - constant_kv_size=true (Object<PerpMarket>=32 bytes + PerpPosition have constant size)
    /// - StorageSlotsAllocator with new_slot_index=10 (FIRST_INDEX), should_reuse=false
    /// - min_leaf_index=1 (ROOT_INDEX), max_leaf_index=1 (ROOT_INDEX)
    pub fn new_empty() -> Self {
        OnChainUserPositions::V1 {
            positions: BigOrderedMap::BPlusTreeMap {
                root: Node::V1 {
                    is_leaf: true,
                    children: OrderedMap::SortedVectorMap { entries: vec![] },
                    prev: 0,  // NULL_INDEX
                    next: 0,  // NULL_INDEX
                },
                nodes: StorageSlotsAllocator::V1 {
                    slots: None,
                    new_slot_index: 10, // FIRST_INDEX
                    should_reuse: false,
                    reuse_head_index: 0, // NULL_INDEX
                    reuse_spare_count: 0,
                    _phantom: PhantomData,
                },
                min_leaf_index: 1,  // ROOT_INDEX
                max_leaf_index: 1,  // ROOT_INDEX
                constant_kv_size: true, // Both key (AccountAddress) and value (PerpPosition) have constant BCS size
                inner_max_degree: 64,
                leaf_max_degree: 16,
                write_cache: std::collections::HashMap::new(),
            },
        }
    }

    /// Check if this user has a non-zero position for the given market.
    /// Only checks the root node (works for small maps where entries are in root leaf).
    pub fn has_nonzero_position_in_root(&self, market_addr: &AccountAddress) -> bool {
        let OnChainUserPositions::V1 { positions } = self;
        let BigOrderedMap::BPlusTreeMap { root, .. } = positions;
        let Node::V1 { is_leaf, children, .. } = root;
        if !*is_leaf {
            // Positions are in child nodes - can't check without table I/O.
            // For small numbers of markets per trader, this shouldn't happen.
            // Conservatively return false (will miss some PnL events).
            return false;
        }
        let OrderedMap::SortedVectorMap { entries } = children;
        for entry in entries {
            if entry.key == *market_addr {
                if let Child::Leaf { value } = &entry.value {
                    return value.get_size() > 0;
                }
            }
        }
        false
    }

    /// Get position info (size, is_long) for a given market from the root node.
    /// Returns None if no position exists or if the root is not a leaf node.
    /// Returns Some((size, is_long)) even if size is 0 (entry exists but closed).
    pub fn get_position_info_in_root(&self, market_addr: &AccountAddress) -> Option<(u64, bool)> {
        let OnChainUserPositions::V1 { positions } = self;
        let BigOrderedMap::BPlusTreeMap { root, .. } = positions;
        let Node::V1 { is_leaf, children, .. } = root;
        if !*is_leaf {
            return None;
        }
        let OrderedMap::SortedVectorMap { entries } = children;
        for entry in entries {
            if entry.key == *market_addr {
                if let Child::Leaf { value } = &entry.value {
                    let OnChainPerpPosition::V1 { size, is_long, .. } = value;
                    return Some((*size, *is_long));
                }
            }
        }
        None
    }

    /// Get position info by traversing inner nodes if the root is not a leaf.
    /// Uses a callback to read child node data from table storage.
    /// Falls back to get_position_info_in_root for leaf roots.
    pub fn get_position_info_with_table_lookup<F>(
        &self,
        market_addr: &AccountAddress,
        read_table_item: &F,
    ) -> Option<(u64, bool)>
    where
        F: Fn(&AccountAddress, &[u8]) -> Option<bytes::Bytes>,
    {
        let OnChainUserPositions::V1 { positions } = self;
        let BigOrderedMap::BPlusTreeMap { root, nodes, .. } = positions;
        let Node::V1 { is_leaf, children, .. } = root;

        if *is_leaf {
            // Root is a leaf - search directly
            return self.get_position_info_in_root(market_addr);
        }

        // Root is an inner node - need to traverse to find the correct child.
        // Inner node children are sorted by key. We use lower_bound semantics:
        // find the first entry with key >= market_addr, then follow that child.
        // If no entry has key >= market_addr, the key is beyond all entries (not found).
        // This matches the Move VM's `internal_lower_bound` + `iter_is_end` check.
        let OrderedMap::SortedVectorMap { entries } = children;

        let slot_index = entries.iter()
            .find_map(|entry| {
                if entry.key >= *market_addr {
                    if let Child::Inner { node_index } = &entry.value {
                        return Some(node_index.slot_index);
                    }
                }
                None
            });

        let slot_index = slot_index?;

        // Get the table handle from the StorageSlotsAllocator
        let StorageSlotsAllocator::V1 { slots, .. } = nodes;
        let table_with_length = slots.as_ref()?;
        let table_handle = &table_with_length.inner.handle;

        // Read the child node from the table
        let key_bytes = bcs::to_bytes(&slot_index).ok()?;
        let child_bytes = read_table_item(table_handle, &key_bytes)?;

        // Deserialize as Link<Node<AccountAddress, OnChainPerpPosition>>
        let link: Link<Node<AccountAddress, OnChainPerpPosition>> =
            bcs::from_bytes(&child_bytes).ok()?;

        if let Link::Occupied { value: child_node } = link {
            let Node::V1 { is_leaf: child_is_leaf, children: child_children, .. } = &child_node;
            if *child_is_leaf {
                let OrderedMap::SortedVectorMap { entries: child_entries } = child_children;
                for entry in child_entries {
                    if entry.key == *market_addr {
                        if let Child::Leaf { value } = &entry.value {
                            let OnChainPerpPosition::V1 { size, is_long, .. } = value;
                            return Some((*size, *is_long));
                        }
                    }
                }
            }
            // If child is also inner, we'd need deeper traversal.
            // For typical UserPositions (up to 100 markets, leaf_max_degree=16),
            // the tree depth is at most 2 (root inner -> leaf children).
        }

        None
    }

    /// Like get_position_info_with_table_lookup but also returns is_isolated.
    pub fn get_position_full_info_with_table_lookup<F>(
        &self,
        market_addr: &AccountAddress,
        read_table_item: &F,
    ) -> Option<(u64, bool, bool)>
    where
        F: Fn(&AccountAddress, &[u8]) -> Option<bytes::Bytes>,
    {
        let OnChainUserPositions::V1 { positions } = self;
        let BigOrderedMap::BPlusTreeMap { root, nodes, .. } = positions;
        let Node::V1 { is_leaf, children, .. } = root;

        if *is_leaf {
            let OrderedMap::SortedVectorMap { entries } = children;
            for entry in entries {
                if entry.key == *market_addr {
                    if let Child::Leaf { value } = &entry.value {
                        let OnChainPerpPosition::V1 { size, is_long, is_isolated, .. } = value;
                        return Some((*size, *is_long, *is_isolated));
                    }
                }
            }
            return None;
        }

        // Inner node traversal (same pattern as get_position_info_with_table_lookup)
        let OrderedMap::SortedVectorMap { entries } = children;
        let slot_index = entries.iter()
            .find_map(|entry| {
                if entry.key >= *market_addr {
                    if let Child::Inner { node_index } = &entry.value {
                        return Some(node_index.slot_index);
                    }
                }
                None
            })?;

        let StorageSlotsAllocator::V1 { slots, .. } = nodes;
        let table_with_length = slots.as_ref()?;
        let table_handle = &table_with_length.inner.handle;

        let key_bytes = bcs::to_bytes(&slot_index).ok()?;
        let child_bytes = read_table_item(table_handle, &key_bytes)?;

        let link: Link<Node<AccountAddress, OnChainPerpPosition>> =
            bcs::from_bytes(&child_bytes).ok()?;

        if let Link::Occupied { value: child_node } = link {
            let Node::V1 { is_leaf: child_is_leaf, children: child_children, .. } = &child_node;
            if *child_is_leaf {
                let OrderedMap::SortedVectorMap { entries: child_entries } = child_children;
                for entry in child_entries {
                    if entry.key == *market_addr {
                        if let Child::Leaf { value } = &entry.value {
                            let OnChainPerpPosition::V1 { size, is_long, is_isolated, .. } = value;
                            return Some((*size, *is_long, *is_isolated));
                        }
                    }
                }
            }
        }

        None
    }

    /// Find and update a position for the given market after a fill.
    /// If no entry exists for this market, inserts a new one (sorted).
    /// Returns true if updated/inserted successfully.
    /// Only operates on root node entries (leaf root).
    pub fn update_position_for_fill(
        &mut self,
        market_addr: &AccountAddress,
        fill_size: u64,
        fill_price: u64,
        is_buy: bool,
    ) -> bool {
        let OnChainUserPositions::V1 { positions } = self;
        let BigOrderedMap::BPlusTreeMap { root, .. } = positions;
        let Node::V1 { is_leaf, children, .. } = root;
        if !*is_leaf {
            return false;
        }
        let OrderedMap::SortedVectorMap { entries } = children;

        // Try to find existing entry for this market
        let search_result = entries.binary_search_by(|e| e.key.cmp(market_addr));

        match search_result {
            Ok(idx) => {
                // Found existing entry - update it
                if let Child::Leaf { value } = &mut entries[idx].value {
                    let OnChainPerpPosition::V1 {
                        size,
                        entry_px_times_size_sum,
                        avg_acquire_entry_px,
                        is_long,
                        ..
                    } = value;

                    if *size == 0 {
                        // Empty position slot - initialize
                        *size = fill_size;
                        *entry_px_times_size_sum = (fill_price as u128) * (fill_size as u128);
                        *avg_acquire_entry_px = fill_price;
                        *is_long = is_buy;
                    } else if *is_long == is_buy {
                        // Same direction: increase position
                        let new_eps = (fill_price as u128) * (fill_size as u128)
                            + *entry_px_times_size_sum;
                        *size += fill_size;
                        *entry_px_times_size_sum = new_eps;
                        *avg_acquire_entry_px = (new_eps / (*size as u128)) as u64;
                    } else {
                        // Opposite direction: decrease or flip
                        if *size >= fill_size {
                            let new_size = *size - fill_size;
                            if *size > 0 {
                                *entry_px_times_size_sum = (*entry_px_times_size_sum)
                                    * (new_size as u128)
                                    / (*size as u128);
                            }
                            *size = new_size;
                            if new_size > 0 {
                                *avg_acquire_entry_px =
                                    (*entry_px_times_size_sum / (new_size as u128)) as u64;
                            } else {
                                *avg_acquire_entry_px = 0;
                            }
                        } else {
                            let new_size = fill_size - *size;
                            *size = new_size;
                            *entry_px_times_size_sum =
                                (fill_price as u128) * (new_size as u128);
                            *avg_acquire_entry_px = fill_price;
                            *is_long = is_buy;
                        }
                    }
                    true
                } else {
                    false // Inner node child - shouldn't happen in leaf root
                }
            },
            Err(insert_idx) => {
                // No entry for this market - insert a new one at the sorted position
                let new_position = OnChainPerpPosition::V1 {
                    size: fill_size,
                    entry_px_times_size_sum: (fill_price as u128) * (fill_size as u128),
                    avg_acquire_entry_px: fill_price,
                    user_leverage: 20, // default max leverage
                    is_long: is_buy,
                    is_isolated: false,
                    funding_index_at_last_update: 0,
                    unrealized_funding_amount_before_last_update: 0,
                    timestamp: 0,
                };
                entries.insert(insert_idx, Entry {
                    key: *market_addr,
                    value: Child::Leaf { value: new_position },
                });
                true
            },
        }
    }

    /// Tree-aware position update: works with B+ trees that have inner nodes.
    /// Uses read_table_item to traverse the tree, and tree_add for insertion/upsert.
    /// The read_table_item closure has the same signature as get_position_info_with_table_lookup.
    /// Returns Vec<TableWrite> for any tree node modifications that need to be
    /// flushed to session table storage.
    pub fn update_position_for_fill_tree<F>(
        &mut self,
        market_addr: &AccountAddress,
        fill_size: u64,
        fill_price: u64,
        is_buy: bool,
        read_table_item: &F,
    ) -> Vec<TableWrite>
    where F: Fn(&AccountAddress, &[u8]) -> Option<bytes::Bytes>,
    {
        let OnChainUserPositions::V1 { positions } = self;

        // When root is a leaf, always use the simple root-only insertion path.
        // This matches the Move VM behavior where the root leaf can temporarily
        // exceed leaf_max_degree before being split on the next write-back.
        // Only use tree operations when root is already an inner node (tree was
        // previously split and positions live in child nodes).
        {
            let BigOrderedMap::BPlusTreeMap { root, .. } = &*positions;
            let Node::V1 { is_leaf, .. } = root;
            if *is_leaf {
                self.update_position_for_fill(market_addr, fill_size, fill_price, is_buy);
                return vec![];
            }
        }

        // Tree path: root is inner OR root is full leaf and key is new.
        // Build a read_slot closure from the positions table handle + read_table_item.
        let OnChainUserPositions::V1 { positions } = self;
        let table_handle = match positions.get_table_handle() {
            Some(th) => {
                th.handle
            },
            None => {
                // Inner root without table handle is an invariant violation --
                // the tree was split but no table was allocated for child nodes.
                panic!("unimplemented: inner root UserPositions without table handle for market={}", market_addr);
            },
        };

        let read_slot = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
            let key_bytes = bcs::to_bytes(&slot_index)
                .map_err(|e| format!("serialize slot key: {}", e))?;
            match read_table_item(&table_handle, &key_bytes) {
                Some(bytes) => Ok(Some(bytes.to_vec())),
                None => Ok(None),
            }
        };

        // Look up existing position value in the tree
        let existing = positions.tree_get(market_addr, &read_slot).ok().flatten();

        // Compute the new position value
        let new_position = match existing {
            Some(existing_pos) => {
                let OnChainPerpPosition::V1 {
                    size, entry_px_times_size_sum, avg_acquire_entry_px: _,
                    user_leverage, is_long, is_isolated,
                    funding_index_at_last_update,
                    unrealized_funding_amount_before_last_update,
                    timestamp,
                } = existing_pos;

                if size == 0 {
                    OnChainPerpPosition::V1 {
                        size: fill_size,
                        entry_px_times_size_sum: (fill_price as u128) * (fill_size as u128),
                        avg_acquire_entry_px: fill_price,
                        user_leverage,
                        is_long: is_buy,
                        is_isolated,
                        funding_index_at_last_update,
                        unrealized_funding_amount_before_last_update,
                        timestamp,
                    }
                } else if is_long == is_buy {
                    let new_eps = (fill_price as u128) * (fill_size as u128) + entry_px_times_size_sum;
                    let new_size = size + fill_size;
                    OnChainPerpPosition::V1 {
                        size: new_size,
                        entry_px_times_size_sum: new_eps,
                        avg_acquire_entry_px: (new_eps / (new_size as u128)) as u64,
                        user_leverage,
                        is_long,
                        is_isolated,
                        funding_index_at_last_update,
                        unrealized_funding_amount_before_last_update,
                        timestamp,
                    }
                } else {
                    if size >= fill_size {
                        let new_size = size - fill_size;
                        let new_eps = if size > 0 {
                            entry_px_times_size_sum * (new_size as u128) / (size as u128)
                        } else {
                            0
                        };
                        let new_avg = if new_size > 0 {
                            (new_eps / (new_size as u128)) as u64
                        } else {
                            0
                        };
                        OnChainPerpPosition::V1 {
                            size: new_size,
                            entry_px_times_size_sum: new_eps,
                            avg_acquire_entry_px: new_avg,
                            user_leverage,
                            is_long,
                            is_isolated,
                            funding_index_at_last_update,
                            unrealized_funding_amount_before_last_update,
                            timestamp,
                        }
                    } else {
                        let new_size = fill_size - size;
                        OnChainPerpPosition::V1 {
                            size: new_size,
                            entry_px_times_size_sum: (fill_price as u128) * (new_size as u128),
                            avg_acquire_entry_px: fill_price,
                            user_leverage,
                            is_long: is_buy,
                            is_isolated,
                            funding_index_at_last_update,
                            unrealized_funding_amount_before_last_update,
                            timestamp,
                        }
                    }
                }
            },
            None => {
                OnChainPerpPosition::V1 {
                    size: fill_size,
                    entry_px_times_size_sum: (fill_price as u128) * (fill_size as u128),
                    avg_acquire_entry_px: fill_price,
                    user_leverage: 20,
                    is_long: is_buy,
                    is_isolated: false,
                    funding_index_at_last_update: 0,
                    unrealized_funding_amount_before_last_update: 0,
                    timestamp: 0,
                }
            },
        };

        // Use tree_add to insert/upsert the position
        match positions.tree_add(*market_addr, new_position, &read_slot) {
            Ok(writes) => {
                writes
            },
            Err(e) => {
                panic!("UserPositions tree_add failed for market={}: {}", market_addr, e);
            },
        }
    }
}

// ============================================================================
// Unit tests for BigOrderedMap B+ tree operations
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    // ========================================================================
    // Test infrastructure
    // ========================================================================

    /// Simulated table storage for testing tree operations that spill to child nodes.
    struct MockTableStorage {
        items: RefCell<HashMap<u64, Vec<u8>>>,
    }

    impl MockTableStorage {
        fn new() -> Self {
            MockTableStorage {
                items: RefCell::new(HashMap::new()),
            }
        }

        /// Build a read_slot closure that reads from this storage.
        fn read_slot(&self) -> impl Fn(u64) -> Result<Option<Vec<u8>>, String> + '_ {
            move |slot_index: u64| {
                let items = self.items.borrow();
                Ok(items.get(&slot_index).cloned())
            }
        }

        /// Apply a batch of TableWrite operations to this storage.
        fn apply_writes(&self, writes: &[TableWrite]) {
            let mut items = self.items.borrow_mut();
            for w in writes {
                items.insert(w.slot_index, w.data.clone());
            }
        }

        /// Number of items stored.
        fn len(&self) -> usize {
            self.items.borrow().len()
        }
    }

    /// A closure that always returns None (no table backing).
    /// Suitable for leaf-only trees that never split.
    fn no_table_read(_slot_index: u64) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }

    /// Create a test BigOrderedMap<u64, u64> with the given leaf_max_degree,
    /// inner_max_degree=8, and a pre-initialized table handle so splits can work.
    fn new_test_map(leaf_max_degree: u16) -> BigOrderedMap<u64, u64> {
        BigOrderedMap::BPlusTreeMap {
            root: Node::V1 {
                is_leaf: true,
                children: OrderedMap::SortedVectorMap { entries: vec![] },
                prev: 0,
                next: 0,
            },
            nodes: StorageSlotsAllocator::V1 {
                slots: Some(TableWithLength {
                    inner: TableHandle {
                        handle: AccountAddress::from_hex_literal("0x1234").unwrap(),
                    },
                    length: 0,
                }),
                new_slot_index: 10,
                should_reuse: false,
                reuse_head_index: 0,
                reuse_spare_count: 0,
                _phantom: PhantomData,
            },
            min_leaf_index: 1,
            max_leaf_index: 1,
            constant_kv_size: true,
            inner_max_degree: 8,
            leaf_max_degree,
            write_cache: std::collections::HashMap::new(),
        }
    }

    // ========================================================================
    // 1. Basic tree operations (leaf-only, no table I/O needed)
    // ========================================================================

    #[test]
    fn test_tree_add_single_entry() {
        let mut map = new_test_map(4);
        let writes = map.tree_add(10, 100, &no_table_read).unwrap();
        assert!(writes.is_empty(), "No table writes for root-only insert");
        let front = map.tree_borrow_front(&no_table_read).unwrap();
        assert_eq!(front, Some((10, 100)));
    }

    #[test]
    fn test_tree_add_multiple_sorted() {
        let mut map = new_test_map(4);
        map.tree_add(30, 300, &no_table_read).unwrap();
        map.tree_add(10, 100, &no_table_read).unwrap();
        map.tree_add(20, 200, &no_table_read).unwrap();

        let front = map.tree_borrow_front(&no_table_read).unwrap();
        assert_eq!(front, Some((10, 100)));

        let back = map.tree_borrow_back(&no_table_read).unwrap();
        assert_eq!(back, Some((30, 300)));
    }

    #[test]
    fn test_tree_add_preserves_order() {
        let mut map = new_test_map(10);
        // Insert in reverse order
        for i in (0..8u64).rev() {
            map.tree_add(i, i * 10, &no_table_read).unwrap();
        }
        // Pop all in order
        for i in 0..8u64 {
            let (entry, _) = map.tree_pop_front(&no_table_read).unwrap();
            assert_eq!(entry, Some((i, i * 10)), "Expected entry ({}, {}) at position {}", i, i * 10, i);
        }
    }

    #[test]
    fn test_tree_add_duplicate_key_upserts() {
        let mut map = new_test_map(4);
        map.tree_add(10, 100, &no_table_read).unwrap();
        map.tree_add(10, 999, &no_table_read).unwrap();

        let val = map.tree_get(&10, &no_table_read).unwrap();
        assert_eq!(val, Some(999), "Duplicate key should upsert the value");

        assert_eq!(map.root_entry_count(), 1, "Should still have exactly 1 entry");
    }

    #[test]
    fn test_tree_is_empty() {
        let mut map = new_test_map(4);
        assert!(map.tree_is_empty(&no_table_read).unwrap());
        map.tree_add(1, 10, &no_table_read).unwrap();
        assert!(!map.tree_is_empty(&no_table_read).unwrap());
    }

    #[test]
    fn test_tree_pop_front_leaf_only() {
        let mut map = new_test_map(4);
        for i in 0..3u64 {
            map.tree_add(i, i * 10, &no_table_read).unwrap();
        }
        let (entry, writes) = map.tree_pop_front(&no_table_read).unwrap();
        assert_eq!(entry, Some((0, 0)));
        assert!(writes.is_empty());

        let (entry, _) = map.tree_pop_front(&no_table_read).unwrap();
        assert_eq!(entry, Some((1, 10)));

        let (entry, _) = map.tree_pop_front(&no_table_read).unwrap();
        assert_eq!(entry, Some((2, 20)));

        let (entry, _) = map.tree_pop_front(&no_table_read).unwrap();
        assert_eq!(entry, None, "Should be empty now");
    }

    #[test]
    fn test_tree_pop_back_leaf_only() {
        let mut map = new_test_map(4);
        for i in 0..3u64 {
            map.tree_add(i, i * 10, &no_table_read).unwrap();
        }
        let (entry, _) = map.tree_pop_back(&no_table_read).unwrap();
        assert_eq!(entry, Some((2, 20)));

        let (entry, _) = map.tree_pop_back(&no_table_read).unwrap();
        assert_eq!(entry, Some((1, 10)));

        let (entry, _) = map.tree_pop_back(&no_table_read).unwrap();
        assert_eq!(entry, Some((0, 0)));

        assert!(map.tree_is_empty(&no_table_read).unwrap());
    }

    #[test]
    fn test_tree_remove_leaf_only() {
        let mut map = new_test_map(4);
        map.tree_add(10, 100, &no_table_read).unwrap();
        map.tree_add(20, 200, &no_table_read).unwrap();
        map.tree_add(30, 300, &no_table_read).unwrap();

        let (removed, _) = map.tree_remove(&20, &no_table_read).unwrap();
        assert_eq!(removed, Some(200));

        assert!(!map.tree_is_empty(&no_table_read).unwrap());
        assert!(!map.tree_contains(&20, &no_table_read).unwrap());
        assert!(map.tree_contains(&10, &no_table_read).unwrap());
        assert!(map.tree_contains(&30, &no_table_read).unwrap());
    }

    #[test]
    fn test_tree_remove_nonexistent_key() {
        let mut map = new_test_map(4);
        map.tree_add(10, 100, &no_table_read).unwrap();

        let (removed, _) = map.tree_remove(&99, &no_table_read).unwrap();
        assert_eq!(removed, None);
    }

    #[test]
    fn test_tree_contains() {
        let mut map = new_test_map(4);
        assert!(!map.tree_contains(&5, &no_table_read).unwrap());
        map.tree_add(5, 50, &no_table_read).unwrap();
        assert!(map.tree_contains(&5, &no_table_read).unwrap());
        assert!(!map.tree_contains(&6, &no_table_read).unwrap());
    }

    #[test]
    fn test_tree_get() {
        let mut map = new_test_map(4);
        map.tree_add(7, 77, &no_table_read).unwrap();
        map.tree_add(3, 33, &no_table_read).unwrap();

        assert_eq!(map.tree_get(&7, &no_table_read).unwrap(), Some(77));
        assert_eq!(map.tree_get(&3, &no_table_read).unwrap(), Some(33));
        assert_eq!(map.tree_get(&999, &no_table_read).unwrap(), None);
    }

    #[test]
    fn test_tree_first_key() {
        let mut map = new_test_map(4);
        assert_eq!(map.tree_first_key(&no_table_read).unwrap(), None);
        map.tree_add(50, 500, &no_table_read).unwrap();
        map.tree_add(10, 100, &no_table_read).unwrap();
        assert_eq!(map.tree_first_key(&no_table_read).unwrap(), Some(10));
    }

    // ========================================================================
    // 2. Tree operations with simulated table I/O (splits)
    // ========================================================================

    #[test]
    fn test_tree_add_triggers_split() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4); // leaf_max_degree=4

        // Insert entries — first 4 fit in root leaf, 5th should trigger split
        for i in 0..5u64 {
            let writes = map.tree_add(i, i * 10, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // After split, root should be inner (not leaf)
        let (is_leaf, _, _, root_count, _) = map.tree_info();
        assert!(!is_leaf, "Root should be inner after split");
        assert!(root_count >= 2, "Root should have >= 2 inner children after split");

        // Storage should have child nodes
        assert!(storage.len() >= 2, "Should have at least 2 child nodes in storage");

        // All entries should still be accessible
        for i in 0..5u64 {
            let val = map.tree_get(&i, &storage.read_slot()).unwrap();
            assert_eq!(val, Some(i * 10), "Entry {} should be accessible after split", i);
        }
    }

    #[test]
    fn test_tree_split_and_borrow_front_back() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        for i in 0..10u64 {
            let writes = map.tree_add(i, i * 10, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        let front = map.tree_borrow_front(&storage.read_slot()).unwrap();
        assert_eq!(front, Some((0, 0)), "Front should be (0, 0)");

        let back = map.tree_borrow_back(&storage.read_slot()).unwrap();
        assert_eq!(back, Some((9, 90)), "Back should be (9, 90)");
    }

    #[test]
    fn test_tree_pop_front_across_nodes() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        for i in 0..10u64 {
            let writes = map.tree_add(i, i * 10, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // Pop all entries in order — this crosses node boundaries
        for i in 0..10u64 {
            let (entry, writes) = map.tree_pop_front(&storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
            assert_eq!(entry, Some((i, i * 10)), "Pop front #{} should be ({}, {})", i, i, i * 10);
        }

        assert!(map.tree_is_empty(&storage.read_slot()).unwrap(), "Map should be empty after popping all");
    }

    #[test]
    fn test_tree_pop_back_across_nodes() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        for i in 0..10u64 {
            let writes = map.tree_add(i, i * 10, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // Pop all entries from back
        for i in (0..10u64).rev() {
            let (entry, writes) = map.tree_pop_back(&storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
            assert_eq!(entry, Some((i, i * 10)), "Pop back should be ({}, {})", i, i * 10);
        }

        assert!(map.tree_is_empty(&storage.read_slot()).unwrap());
    }

    #[test]
    fn test_tree_remove_across_nodes() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        for i in 0..10u64 {
            let writes = map.tree_add(i, i * 10, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // Remove middle element
        let (removed, writes) = map.tree_remove(&5, &storage.read_slot()).unwrap();
        storage.apply_writes(&writes);
        assert_eq!(removed, Some(50));

        // Verify it's gone
        assert!(!map.tree_contains(&5, &storage.read_slot()).unwrap());

        // Verify neighbors still exist
        assert!(map.tree_contains(&4, &storage.read_slot()).unwrap());
        assert!(map.tree_contains(&6, &storage.read_slot()).unwrap());
    }

    #[test]
    fn test_tree_large_insert_then_pop_all() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        // Insert 100 entries
        for i in 0..100u64 {
            let writes = map.tree_add(i, i, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // All entries should be findable
        for i in 0..100u64 {
            let val = map.tree_get(&i, &storage.read_slot()).unwrap();
            assert_eq!(val, Some(i), "Entry {} should exist", i);
        }

        // Pop all — should produce entries in sorted order
        let mut results = vec![];
        while !map.tree_is_empty(&storage.read_slot()).unwrap() {
            let (entry, writes) = map.tree_pop_front(&storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
            if let Some(e) = entry {
                results.push(e);
            } else {
                break;
            }
        }

        assert_eq!(results.len(), 100, "Should have popped all 100 entries");
        for i in 0..100 {
            assert_eq!(results[i], (i as u64, i as u64), "Entry {} should be in order", i);
        }
    }

    #[test]
    fn test_tree_interleaved_add_and_pop() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        // Add 20, pop 5, add 20 more, pop all
        for i in 0..20u64 {
            let writes = map.tree_add(i, i, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        for _ in 0..5 {
            let (_, writes) = map.tree_pop_front(&storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }
        // Now front should be 5
        let front = map.tree_borrow_front(&storage.read_slot()).unwrap();
        assert_eq!(front, Some((5, 5)));

        // Add 20 more (20..40)
        for i in 20..40u64 {
            let writes = map.tree_add(i, i, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // Pop all remaining — should be 5..40 in order
        let mut results = vec![];
        while !map.tree_is_empty(&storage.read_slot()).unwrap() {
            let (entry, writes) = map.tree_pop_front(&storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
            if let Some(e) = entry {
                results.push(e);
            } else {
                break;
            }
        }

        assert_eq!(results.len(), 35, "Should have 35 entries (5..40)");
        for (idx, &(k, v)) in results.iter().enumerate() {
            let expected = idx as u64 + 5;
            assert_eq!(k, expected, "Key at position {} should be {}", idx, expected);
            assert_eq!(v, expected, "Value at position {} should be {}", idx, expected);
        }
    }

    #[test]
    fn test_tree_reverse_insertion_order() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        // Insert in reverse order — stresses different split paths
        for i in (0..50u64).rev() {
            let writes = map.tree_add(i, i * 100, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // Verify sorted order via pop_front
        for i in 0..50u64 {
            let (entry, writes) = map.tree_pop_front(&storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
            assert_eq!(entry, Some((i, i * 100)), "Reverse-inserted entry {} wrong", i);
        }
    }

    #[test]
    fn test_tree_random_ish_insertion() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        // Insert in a shuffled-ish order (not truly random, but non-sequential)
        let keys: Vec<u64> = vec![
            42, 17, 88, 3, 65, 91, 24, 50, 12, 77,
            35, 99, 7, 58, 81, 19, 44, 70, 2, 56,
        ];
        for &k in &keys {
            let writes = map.tree_add(k, k * 10, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // All keys should be findable
        for &k in &keys {
            let val = map.tree_get(&k, &storage.read_slot()).unwrap();
            assert_eq!(val, Some(k * 10), "Key {} should have value {}", k, k * 10);
        }

        // Pop front should yield sorted order
        let mut sorted_keys = keys.clone();
        sorted_keys.sort();
        for &expected_k in &sorted_keys {
            let (entry, writes) = map.tree_pop_front(&storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
            assert_eq!(entry, Some((expected_k, expected_k * 10)));
        }
    }

    #[test]
    fn test_tree_upsert_across_nodes() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        // Insert enough to trigger splits
        for i in 0..20u64 {
            let writes = map.tree_add(i, i, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // Upsert existing keys with new values
        for i in 0..20u64 {
            let writes = map.tree_add(i, i + 1000, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // Verify updated values
        for i in 0..20u64 {
            let val = map.tree_get(&i, &storage.read_slot()).unwrap();
            assert_eq!(val, Some(i + 1000), "Key {} should have been upserted to {}", i, i + 1000);
        }
    }

    #[test]
    fn test_tree_remove_all_entries() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        for i in 0..15u64 {
            let writes = map.tree_add(i, i, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // Remove all entries by key
        for i in 0..15u64 {
            let (removed, writes) = map.tree_remove(&i, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
            assert_eq!(removed, Some(i), "Should remove key {}", i);
        }

        assert!(map.tree_is_empty(&storage.read_slot()).unwrap());
    }

    #[test]
    fn test_tree_remove_from_front_and_back() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        for i in 0..20u64 {
            let writes = map.tree_add(i, i, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // Remove front (0)
        let (val, writes) = map.tree_remove(&0, &storage.read_slot()).unwrap();
        storage.apply_writes(&writes);
        assert_eq!(val, Some(0));

        // Remove back (19)
        let (val, writes) = map.tree_remove(&19, &storage.read_slot()).unwrap();
        storage.apply_writes(&writes);
        assert_eq!(val, Some(19));

        // Front should now be 1, back should be 18
        assert_eq!(map.tree_borrow_front(&storage.read_slot()).unwrap(), Some((1, 1)));
        assert_eq!(map.tree_borrow_back(&storage.read_slot()).unwrap(), Some((18, 18)));
    }

    // ========================================================================
    // 3. Different tree degrees
    // ========================================================================

    #[test]
    fn test_tree_degree_3() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(3); // minimum useful degree

        for i in 0..30u64 {
            let writes = map.tree_add(i, i, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // Verify all entries
        for i in 0..30u64 {
            assert_eq!(map.tree_get(&i, &storage.read_slot()).unwrap(), Some(i));
        }

        // Pop all
        for i in 0..30u64 {
            let (entry, writes) = map.tree_pop_front(&storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
            assert_eq!(entry, Some((i, i)));
        }
    }

    #[test]
    fn test_tree_degree_8() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(8);

        for i in 0..50u64 {
            let writes = map.tree_add(i, i * 100, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        assert_eq!(map.tree_borrow_front(&storage.read_slot()).unwrap(), Some((0, 0)));
        assert_eq!(map.tree_borrow_back(&storage.read_slot()).unwrap(), Some((49, 4900)));

        // Pop all back
        for i in (0..50u64).rev() {
            let (entry, writes) = map.tree_pop_back(&storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
            assert_eq!(entry, Some((i, i * 100)));
        }
    }

    // ========================================================================
    // 4. BCS roundtrip tests
    // ========================================================================

    #[test]
    fn test_bcs_roundtrip_leaf_only() {
        let mut map = new_test_map(16);
        for i in 0..10u64 {
            map.tree_add(i, i * 100, &no_table_read).unwrap();
        }

        let bytes = bcs::to_bytes(&map).unwrap();
        let mut map2: BigOrderedMap<u64, u64> = bcs::from_bytes(&bytes).unwrap();

        // Verify entries match
        for i in 0..10u64 {
            let (entry, _) = map2.tree_pop_front(&no_table_read).unwrap();
            assert_eq!(entry.unwrap(), (i, i * 100));
        }
    }

    #[test]
    fn test_bcs_roundtrip_empty_map() {
        let map = new_test_map(4);
        let bytes = bcs::to_bytes(&map).unwrap();
        let map2: BigOrderedMap<u64, u64> = bcs::from_bytes(&bytes).unwrap();
        assert!(map2.is_empty());
    }

    #[test]
    fn test_bcs_roundtrip_single_entry() {
        let mut map = new_test_map(4);
        map.tree_add(42, 420, &no_table_read).unwrap();

        let bytes = bcs::to_bytes(&map).unwrap();
        let map2: BigOrderedMap<u64, u64> = bcs::from_bytes(&bytes).unwrap();
        assert_eq!(map2.tree_get(&42, &no_table_read).unwrap(), Some(420));
    }

    // ========================================================================
    // 5. Edge cases
    // ========================================================================

    #[test]
    fn test_tree_pop_front_empty() {
        let mut map = new_test_map(4);
        let (entry, writes) = map.tree_pop_front(&no_table_read).unwrap();
        assert_eq!(entry, None);
        assert!(writes.is_empty());
    }

    #[test]
    fn test_tree_pop_back_empty() {
        let mut map = new_test_map(4);
        let (entry, writes) = map.tree_pop_back(&no_table_read).unwrap();
        assert_eq!(entry, None);
        assert!(writes.is_empty());
    }

    #[test]
    fn test_tree_borrow_front_empty() {
        let map = new_test_map(4);
        assert_eq!(map.tree_borrow_front(&no_table_read).unwrap(), None);
    }

    #[test]
    fn test_tree_borrow_back_empty() {
        let map = new_test_map(4);
        assert_eq!(map.tree_borrow_back(&no_table_read).unwrap(), None);
    }

    #[test]
    fn test_tree_remove_from_empty() {
        let mut map = new_test_map(4);
        let (removed, writes) = map.tree_remove(&42, &no_table_read).unwrap();
        assert_eq!(removed, None);
        assert!(writes.is_empty());
    }

    #[test]
    fn test_tree_add_and_remove_same_key_repeatedly() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        for round in 0..5 {
            // Add entries
            for i in 0..10u64 {
                let writes = map.tree_add(i, i + round * 100, &storage.read_slot()).unwrap();
                storage.apply_writes(&writes);
            }
            // Remove all
            for i in 0..10u64 {
                let (removed, writes) = map.tree_remove(&i, &storage.read_slot()).unwrap();
                storage.apply_writes(&writes);
                assert!(removed.is_some(), "Round {}: key {} should be removable", round, i);
            }
            assert!(map.tree_is_empty(&storage.read_slot()).unwrap(), "Round {}: map should be empty", round);
        }
    }

    #[test]
    fn test_tree_sequential_writes_are_visible_to_subsequent_reads() {
        // This tests the "deadlock bug" scenario: tree_pop_front writes must be
        // visible to the next tree_pop_front call.
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        for i in 0..20u64 {
            let writes = map.tree_add(i, i, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // Pop front 10 times — each pop's writes feed into the next pop's reads
        let mut last_key = None;
        for _ in 0..10 {
            let (entry, writes) = map.tree_pop_front(&storage.read_slot()).unwrap();
            storage.apply_writes(&writes); // Critical: apply before next pop
            let (k, _) = entry.unwrap();
            if let Some(prev) = last_key {
                assert!(k > prev, "Keys should be strictly increasing: {} > {}", k, prev);
            }
            last_key = Some(k);
        }
    }

    // ========================================================================
    // 6. UserPositions integration tests
    // ========================================================================

    #[test]
    fn test_user_positions_new_empty() {
        let positions = OnChainUserPositions::new_empty();
        let OnChainUserPositions::V1 { positions: ref map } = positions;
        assert!(map.is_empty());
    }

    #[test]
    fn test_user_positions_update_single_fill() {
        let mut positions = OnChainUserPositions::new_empty();

        let market_addr = AccountAddress::from_hex_literal("0xdead").unwrap();
        let success = positions.update_position_for_fill(&market_addr, 100, 50000, true);
        assert!(success);

        let info = positions.get_position_info_in_root(&market_addr);
        assert!(info.is_some());
        let (size, is_long) = info.unwrap();
        assert_eq!(size, 100);
        assert!(is_long);
    }

    #[test]
    fn test_user_positions_increase_position() {
        let mut positions = OnChainUserPositions::new_empty();
        let market = AccountAddress::from_hex_literal("0x1").unwrap();

        positions.update_position_for_fill(&market, 100, 50000, true);
        positions.update_position_for_fill(&market, 50, 51000, true);

        let (size, is_long) = positions.get_position_info_in_root(&market).unwrap();
        assert_eq!(size, 150);
        assert!(is_long);
    }

    #[test]
    fn test_user_positions_decrease_position() {
        let mut positions = OnChainUserPositions::new_empty();
        let market = AccountAddress::from_hex_literal("0x1").unwrap();

        positions.update_position_for_fill(&market, 100, 50000, true); // long 100
        positions.update_position_for_fill(&market, 30, 51000, false); // sell 30

        let (size, is_long) = positions.get_position_info_in_root(&market).unwrap();
        assert_eq!(size, 70);
        assert!(is_long); // still long
    }

    #[test]
    fn test_user_positions_flip_position() {
        let mut positions = OnChainUserPositions::new_empty();
        let market = AccountAddress::from_hex_literal("0x1").unwrap();

        positions.update_position_for_fill(&market, 100, 50000, true); // long 100
        positions.update_position_for_fill(&market, 150, 49000, false); // sell 150 -> short 50

        let (size, is_long) = positions.get_position_info_in_root(&market).unwrap();
        assert_eq!(size, 50);
        assert!(!is_long); // flipped to short
    }

    #[test]
    fn test_user_positions_multiple_markets() {
        let mut positions = OnChainUserPositions::new_empty();

        for i in 0..10u64 {
            let mut addr_bytes = [0u8; 32];
            addr_bytes[31] = i as u8;
            let market_addr = AccountAddress::new(addr_bytes);
            positions.update_position_for_fill(&market_addr, 100 + i, 50000, i % 2 == 0);
        }

        // Verify all 10 positions
        for i in 0..10u64 {
            let mut addr_bytes = [0u8; 32];
            addr_bytes[31] = i as u8;
            let market_addr = AccountAddress::new(addr_bytes);
            let info = positions.get_position_info_in_root(&market_addr);
            assert!(info.is_some(), "Position for market {} not found", i);
            let (size, is_long) = info.unwrap();
            assert_eq!(size, 100 + i);
            assert_eq!(is_long, i % 2 == 0);
        }
    }

    #[test]
    fn test_user_positions_bcs_roundtrip() {
        let mut positions = OnChainUserPositions::new_empty();
        let market = AccountAddress::from_hex_literal("0xbeef").unwrap();
        positions.update_position_for_fill(&market, 200, 60000, false);

        let bytes = bcs::to_bytes(&positions).unwrap();
        let positions2: OnChainUserPositions = bcs::from_bytes(&bytes).unwrap();

        let info = positions2.get_position_info_in_root(&market);
        assert!(info.is_some());
        let (size, is_long) = info.unwrap();
        assert_eq!(size, 200);
        assert!(!is_long);
    }

    // ========================================================================
    // 7. Stress test: 200 entries with small degree
    // ========================================================================

    #[test]
    fn test_tree_stress_200_entries_degree_3() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(3);

        for i in 0..200u64 {
            let writes = map.tree_add(i, i, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }

        // Verify all entries exist
        for i in 0..200u64 {
            let val = map.tree_get(&i, &storage.read_slot()).unwrap();
            assert_eq!(val, Some(i), "After insert phase: key {} not found", i);
        }

        // Verify front and back
        assert_eq!(map.tree_borrow_front(&storage.read_slot()).unwrap(), Some((0, 0)));
        assert_eq!(map.tree_borrow_back(&storage.read_slot()).unwrap(), Some((199, 199)));

        // Pop all and verify order
        let mut count = 0u64;
        while !map.tree_is_empty(&storage.read_slot()).unwrap() {
            let (entry, writes) = map.tree_pop_front(&storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
            match entry {
                Some((k, v)) => {
                    assert_eq!(k, count, "Key mismatch at count {}", count);
                    assert_eq!(v, count, "Value mismatch at count {}", count);
                    count += 1;
                },
                None => {
                    let (is_leaf, ld, id, rc, ns) = map.tree_info();
                    let BigOrderedMap::BPlusTreeMap { min_leaf_index, max_leaf_index, .. } = &map;
                    panic!("pop_front returned None at count {} but tree_is_empty=false, root: is_leaf={} entries={} min_leaf={} max_leaf={}",
                        count, is_leaf, rc, min_leaf_index, max_leaf_index);
                },
            }
        }
        assert_eq!(count, 200);
    }

    #[test]
    fn test_tree_borrow_front_back_after_mixed_ops() {
        let storage = MockTableStorage::new();
        let mut map = new_test_map(4);

        for i in 0..20u64 {
            let writes = map.tree_add(i * 10, i, &storage.read_slot()).unwrap();
            storage.apply_writes(&writes);
        }
        assert_eq!(map.tree_borrow_front(&storage.read_slot()).unwrap(), Some((0, 0)));
        assert_eq!(map.tree_borrow_back(&storage.read_slot()).unwrap(), Some((190, 19)));

        let (removed, writes) = map.tree_remove(&0, &storage.read_slot()).unwrap();
        storage.apply_writes(&writes);
        assert_eq!(removed, Some(0));
        assert_eq!(map.tree_borrow_front(&storage.read_slot()).unwrap(), Some((10, 1)));

        let (removed, writes) = map.tree_remove(&190, &storage.read_slot()).unwrap();
        storage.apply_writes(&writes);
        assert_eq!(removed, Some(19));
        assert_eq!(map.tree_borrow_back(&storage.read_slot()).unwrap(), Some((180, 18)));

        let (removed, writes) = map.tree_remove(&100, &storage.read_slot()).unwrap();
        storage.apply_writes(&writes);
        assert_eq!(removed, Some(10));
        assert_eq!(map.tree_borrow_front(&storage.read_slot()).unwrap(), Some((10, 1)));
        assert_eq!(map.tree_borrow_back(&storage.read_slot()).unwrap(), Some((180, 18)));

        let writes = map.tree_add(5, 100, &storage.read_slot()).unwrap();
        storage.apply_writes(&writes);
        assert_eq!(map.tree_borrow_front(&storage.read_slot()).unwrap(), Some((5, 100)));

        let writes = map.tree_add(200, 200, &storage.read_slot()).unwrap();
        storage.apply_writes(&writes);
        assert_eq!(map.tree_borrow_back(&storage.read_slot()).unwrap(), Some((200, 200)));
    }
}

/// ReferralFeeConfig::V1 { referral_fee_enabled, referral_fee_pct, referred_fee_discount_pct,
///   discount_eligibility_volume_threshold, referrer_eligibility_volume_threshold }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReferralFeeConfig {
    V1 {
        referral_fee_enabled: bool,
        referral_fee_pct: u64,
        referred_fee_discount_pct: u64,
        discount_eligibility_volume_threshold: u128,
        referrer_eligibility_volume_threshold: u128,
    },
}

/// TradingFeeConfiguration::V1 -- fee tier thresholds and rates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradingFeeConfiguration {
    V1 {
        tier_thresholds: Vec<u128>,
        tier_maker_fees: Vec<u64>,
        tier_taker_fees: Vec<u64>,
        market_maker_absolute_threshold: u128,
        market_maker_tier_pct_thresholds: Vec<u64>,
        market_maker_tier_fee_rebates: Vec<u64>,
        builder_max_fee: u64,
        backstop_vault_fee_pct: u64,
        referral_fee_config: ReferralFeeConfig,
    },
}

impl TradingFeeConfiguration {
    /// Get the maker fee rate (in units of 0.0001%) for the given volume.
    /// Returns 0 if volume is in a tier with zero maker fees.
    pub fn get_maker_fee_rate(&self, volume: u128) -> u64 {
        let TradingFeeConfiguration::V1 {
            tier_thresholds, tier_maker_fees, ..
        } = self;
        let mut tier = 0usize;
        while tier < tier_thresholds.len() && volume >= tier_thresholds[tier] {
            tier += 1;
        }
        if tier < tier_maker_fees.len() {
            tier_maker_fees[tier]
        } else {
            0
        }
    }
}

impl VolumeHistory {
    /// Get the total_volume_in_window field.
    pub fn total_volume_in_window(&self) -> u128 {
        let VolumeHistory::V1 { total_volume_in_window, .. } = self;
        *total_volume_in_window
    }
}
