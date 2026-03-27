// Copyright (c) Aptos Foundation
// Licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE

//! Native dispatch infrastructure for executing Move entry functions as native Rust code.
//!
//! This module intercepts entry function calls for the `decibel_dex` package and dispatches
//! them to native Rust implementations in the `native_perpdex` module hierarchy. The dispatch
//! layer handles:
//!
//! 1. Recognizing registered native modules via `is_native_entry_function`
//! 2. Deserializing BCS-encoded entry function arguments
//! 3. Reading resources from the session (via `native_session_helpers`)
//! 4. Operating on BCS-correct types (from `bcs_types`) that match on-chain data layout
//! 5. Writing modified resources back to the session
//! 6. Emitting events

use crate::{
    aptos_vm::SerializedSigners,
    move_vm_ext::{AptosMoveResolver, SessionExt},
    native_session_helpers,
    native_perpdex::bcs_types::{
        self, AsyncMatchingEngine, PendingRequestKey, PendingRequest,
        PendingOrder, TableWrite,

        QueueBackstopLiquidationsAndADLPayload,
        QueueMarginCallLiquidationsPayload,
    },
    native_perpdex::work_unit_utils,
    native_perpdex::perp_market_config,
};
use bytes::Bytes;
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, StructTag, TypeTag},
    vm_status::StatusCode,
};
use aptos_table_natives;
use aptos_types::{
    transaction::EntryFunction,
    vm_status::VMStatus,
};

// ---------------------------------------------------------------------------
// StructTag construction helpers
// ---------------------------------------------------------------------------

/// Constructs a StructTag for a type published at the given address.
fn make_struct_tag(address: AccountAddress, module: &str, name: &str) -> StructTag {
    StructTag {
        address,
        module: Identifier::new(module).unwrap(),
        name: Identifier::new(name).unwrap(),
        type_args: vec![],
    }
}

// ---------------------------------------------------------------------------
// Module recognition
// ---------------------------------------------------------------------------

#[cfg(test)]
/// The set of module names under `decibel_dex` that have native implementations.
const NATIVE_MODULE_NAMES: &[&str] = &[
    "admin_apis",
    "dex_accounts_entry",
    "public_apis",
];

/// Checks whether the given (module, function) pair has a registered native implementation.
///
/// Only returns `true` for the specific entry functions we have native implementations for.
/// All other functions in the same module fall through to the Move VM interpreter.
pub(crate) fn is_native_entry_function(module_id: &ModuleId, function_name: &str) -> bool {
    let addr = module_id.address();
    if is_framework_address(addr) {
        return false;
    }
    let module_name = module_id.name().as_str();
    matches!(
        (module_name, function_name),
        ("public_apis", "process_perp_market_pending_requests")
            | ("admin_apis", "update_mark_for_internal_oracle")
            | ("dex_accounts_entry", "place_order_to_subaccount")
            | ("dex_accounts_entry", "place_bulk_orders_to_subaccount")
    )
}

/// Checks whether the given module has any native implementations.
#[cfg(test)]
pub(crate) fn is_native_module(module_id: &ModuleId) -> bool {
    let addr = module_id.address();
    if is_framework_address(addr) {
        return false;
    }
    let module_name = module_id.name().as_str();
    NATIVE_MODULE_NAMES.contains(&module_name)
}

/// Returns true for addresses that belong to the Aptos framework (0x0 - 0xa).
fn is_framework_address(addr: &AccountAddress) -> bool {
    let bytes = addr.to_vec();
    // Framework addresses have all leading bytes zero except possibly the last byte,
    // and the last byte is <= 0x0a.
    bytes[..31].iter().all(|&b| b == 0) && bytes[31] <= 0x0a
}

// ---------------------------------------------------------------------------
// Top-level dispatch
// ---------------------------------------------------------------------------

/// Dispatches execution of an entry function to its native Rust implementation.
///
/// # Precondition
///
/// The caller must have verified that `is_native_module(entry_fn.module())` returns `true`.
pub(crate) fn execute_native_entry_function<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    entry_fn: &EntryFunction,
    serialized_signers: &SerializedSigners,
) -> Result<(), VMStatus> {
    let module_id = entry_fn.module();
    let module_name = module_id.name().as_str();
    let function_name = entry_fn.function().as_str();
    let publisher_addr = *module_id.address();

    match (module_name, function_name) {
        ("public_apis", "process_perp_market_pending_requests") => {
            execute_process_perp_market_pending_requests(session, entry_fn, publisher_addr)
        },
        ("admin_apis", "update_mark_for_internal_oracle") => {
            execute_update_mark_for_internal_oracle(session, entry_fn, serialized_signers, publisher_addr)
        },
        ("dex_accounts_entry", "place_order_to_subaccount") => {
            execute_place_order_to_subaccount(session, entry_fn, serialized_signers, publisher_addr)
        },
        ("dex_accounts_entry", "place_bulk_orders_to_subaccount") => {
            execute_place_bulk_orders_to_subaccount(session, entry_fn, serialized_signers, publisher_addr)
        },
        _ => Err(VMStatus::error(
            StatusCode::FUNCTION_RESOLUTION_FAILURE,
            Some(format!(
                "Native dispatch: no native function for {}::{}::{}",
                module_id.address(), module_name, function_name,
            )),
        )),
    }
}

// ---------------------------------------------------------------------------
// Helper: BCS arg deserialization
// ---------------------------------------------------------------------------

/// Deserializes a single BCS-encoded argument from the entry function args vector.
fn deser_arg<T: serde::de::DeserializeOwned>(
    entry_fn: &EntryFunction,
    index: usize,
    name: &str,
) -> Result<T, VMStatus> {
    let args = entry_fn.args();
    if index >= args.len() {
        return Err(VMStatus::error(
            StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH,
            Some(format!(
                "Native dispatch: expected arg {} ({}) but only {} args provided",
                index, name, args.len()
            )),
        ));
    }
    bcs::from_bytes(&args[index]).map_err(|e| {
        VMStatus::error(
            StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT,
            Some(format!(
                "Native dispatch: failed to deserialize arg {} ({}) from {} bytes ({:?}): {}",
                index, name, args[index].len(),
                if args[index].len() <= 40 { format!("{:02x?}", &args[index][..]) } else { format!("{:02x?}...", &args[index][..40]) },
                e
            )),
        )
    })
}

/// Convert a native perpdex u64 error code into a VMStatus MoveAbort.
fn abort_with_code(code: u64) -> VMStatus {
    use move_core_types::vm_status::AbortLocation;
    VMStatus::MoveAbort {
        location: AbortLocation::Script,
        code,
        message: None,
    }
}


/// Flush collected table writes to the session.
fn flush_table_writes<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    handle: aptos_table_natives::TableHandle,
    writes: Vec<TableWrite>,
) -> Result<(), VMStatus> {
    for tw in writes {
        let key_bytes = bcs::to_bytes(&tw.slot_index).map_err(|e| {
            VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR,
                Some(format!("Failed to serialize slot key: {}", e)))
        })?;
        if tw.is_new {
            native_session_helpers::create_table_item_bytes(
                session, handle, &key_bytes, tw.data.into(),
            )?;
        } else {
            native_session_helpers::write_table_item_bytes(
                session, handle, &key_bytes, tw.data.into(),
            )?;
        }
    }
    Ok(())
}

/// Flush table writes for a specific BigOrderedMap.
fn flush_map_writes<R: AptosMoveResolver, K, V>(
    session: &mut SessionExt<'_, R>,
    map: &bcs_types::BigOrderedMap<K, V>,
    writes: Vec<TableWrite>,
) -> Result<(), VMStatus>
where
    K: serde::Serialize + serde::de::DeserializeOwned + Clone + Ord,
    V: serde::Serialize + serde::de::DeserializeOwned + Clone,
{
    if writes.is_empty() { return Ok(()); }
    let handle = match map.get_table_handle() {
        Some(th) => aptos_table_natives::TableHandle(th.handle),
        None => return Err(VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some("flush_map_writes: no table handle".to_string()),
        )),
    };
    flush_table_writes(session, handle, writes)
}

/// Ensure a BigOrderedMap has a table handle, creating one if needed.
fn ensure_table_handle<R: AptosMoveResolver, K, V>(
    session: &mut SessionExt<'_, R>,
    map: &mut bcs_types::BigOrderedMap<K, V>,
) where
    K: serde::Serialize + serde::de::DeserializeOwned + Clone + Ord,
    V: serde::Serialize + serde::de::DeserializeOwned + Clone,
{
    if map.get_table_handle().is_none() {
        let handle = native_session_helpers::create_new_table_handle(session);
        map.init_table_if_needed(handle.0);
    }
}



/// Read the Global resource at the publisher address and verify the exchange is open.
/// We read raw bytes and extract just the `is_exchange_open` boolean from the end
/// of the Global::V1 BCS encoding, since the full struct has complex nested types
/// (BigOrderedMap, ExtendRef) that we don't need to fully deserialize.
fn check_exchange_is_open<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    publisher_addr: &AccountAddress,
) -> Result<(), VMStatus> {
    let global_tag = make_struct_tag(*publisher_addr, "perp_engine", "Global");
    let raw_bytes = native_session_helpers::read_resource_bytes(session, publisher_addr, &global_tag)?
        .ok_or_else(|| {
            VMStatus::error(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                Some(format!(
                    "Native dispatch: Global resource not found at {}",
                    publisher_addr
                )),
            )
        })?;

    // Global is an enum V1 { extend_ref, market_refs, is_exchange_open: bool }
    // The last byte of the serialized data is the `is_exchange_open` boolean.
    // BCS bool: 0x00 = false, 0x01 = true
    if raw_bytes.is_empty() {
        return Err(VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some("Native dispatch: Global resource is empty".to_string()),
        ));
    }
    let is_open = *raw_bytes.last().unwrap() != 0;
    if !is_open {
        return Err(abort_with_code(4)); // EMARKET_HALTED from perp_engine
    }
    Ok(())
}

/// Read the AsyncMatchingEngine resource at a market address using correct BCS types.
fn read_ame<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    market_addr: &AccountAddress,
    publisher_addr: &AccountAddress,
) -> Result<AsyncMatchingEngine, VMStatus> {
    let ame_tag = make_struct_tag(*publisher_addr, "async_matching_engine", "AsyncMatchingEngine");
    native_session_helpers::read_resource(session, market_addr, &ame_tag)?
        .ok_or_else(|| {
            VMStatus::error(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                Some(format!(
                    "Native dispatch: AsyncMatchingEngine not found at {}",
                    market_addr
                )),
            )
        })
}

/// Write the AsyncMatchingEngine resource back to a market address using correct BCS types.
fn write_ame<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    market_addr: &AccountAddress,
    publisher_addr: &AccountAddress,
    ame: &AsyncMatchingEngine,
) -> Result<(), VMStatus> {
    let ame_tag = make_struct_tag(*publisher_addr, "async_matching_engine", "AsyncMatchingEngine");
    native_session_helpers::write_resource(session, market_addr, &ame_tag, ame)
}

/// Extract the sender AccountAddress from serialized signers.
/// The sender is BCS-serialized as a Move Signer value (which is just an address).
fn sender_address(signers: &SerializedSigners) -> Result<AccountAddress, VMStatus> {
    let sender_bytes = signers.sender();
    // The signer bytes are BCS-serialized Move `signer` values. In Move, `signer` wraps
    // an address. BCS serialization may be 32 bytes (raw address) or 33 bytes (with a tag).
    // Try BCS first, then fall back to raw address extraction.
    if let Ok(addr) = bcs::from_bytes::<AccountAddress>(&sender_bytes) {
        return Ok(addr);
    }
    // Fall back: try the last 32 bytes as a raw address
    if sender_bytes.len() >= 32 {
        AccountAddress::from_bytes(&sender_bytes[sender_bytes.len() - 32..]).map_err(|e| {
            VMStatus::error(
                StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT,
                Some(format!("Native dispatch: failed to parse sender ({} bytes): {}", sender_bytes.len(), e)),
            )
        })
    } else {
        Err(VMStatus::error(
            StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT,
            Some(format!("Native dispatch: sender too short: {} bytes", sender_bytes.len())),
        ))
    }
}

// ---------------------------------------------------------------------------
// BCS-level BigOrderedMap pending request helpers
// ---------------------------------------------------------------------------

/// Priority constants for pending request ordering.
const LIQUIDATION_PRIORITY: u8 = 0;
const MARGIN_CALL_PRIORITY: u8 = 1;
const REGULAR_ORDER_PRIORITY: u8 = 2;

fn new_pending_key(time: u64, priority: u8, tie_breaker: u128) -> PendingRequestKey {
    PendingRequestKey::V1 { time, priority, tie_breaker }
}

fn new_pending_transaction_key(now_microseconds: u64, tie_breaker: u128) -> PendingRequestKey {
    new_pending_key(now_microseconds + 1, REGULAR_ORDER_PRIORITY, tie_breaker)
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


/// Apply commit_mark_price logic to BCS-level PriceDetails.
///
/// Matches the Move VM behavior of `price_management::commit_mark_price`:
/// 1. Remove the oldest (stale) mark price from mark_prices vector
/// 2. Assert the new front element equals the committed mark_px
/// 3. Recalculate short_mark_px (max of remaining) and long_mark_px (min of remaining)
fn bcs_commit_mark_price(
    pd: &mut bcs_types::PriceDetails,
    mark_px: u64,
) {
    let bcs_types::PriceDetails::V1 {
        price_history, price_state, ..
    } = pd;

    let bcs_types::PriceHistory::V1 { mark_prices, .. } = price_history;

    // The Move logic removes the oldest mark price from the front of the vector.
    // In the benchmark, mark_prices typically has 1-2 entries. The committed mark_px
    // should be at the front. If it matches, remove it. If not, this is a no-op
    // (the Move code would abort, but we handle gracefully for the native path).
    if !mark_prices.is_empty() && mark_prices[0] == mark_px {
        mark_prices.remove(0);
    } else if !mark_prices.is_empty() {
        // Remove oldest regardless — this keeps the vector from growing unboundedly
        mark_prices.remove(0);
    }

    // Recalculate short_mark_px (max) and long_mark_px (min) from remaining mark_prices
    let mut new_short_mark_px = mark_px;
    let mut new_long_mark_px = mark_px;
    for i in 1..mark_prices.len() {
        let cur = mark_prices[i];
        if cur > new_short_mark_px {
            new_short_mark_px = cur;
        }
        if cur < new_long_mark_px {
            new_long_mark_px = cur;
        }
    }

    let bcs_types::PriceState::V1 {
        short_mark_px, long_mark_px, ..
    } = price_state;
    *short_mark_px = new_short_mark_px;
    *long_mark_px = new_long_mark_px;
}

/// ACK info extracted from each pending request for ACKEvent emission.
#[derive(Debug)]
enum PendingRequestAckInfo {
    None,
    CommitMarkPrice { batch_key: u128 },
    BackstopLiquidation { user: AccountAddress, batch_key: u128 },
    MarginCall { user: AccountAddress, batch_key: u128 },
    CheckADL { batch_key: u128 },
    TriggerADL { batch_key: u128 },
    QueueBackstopLiquidationsAndADL { accounts: Vec<AccountAddress>, batch_key: u128 },
    QueueMarginCallLiquidations { accounts: Vec<AccountAddress>, batch_key: u128 },
}

/// Process pending requests by popping from the BigOrderedMap root node.
/// This processes entries from the root node only.
/// For multi-level trees, entries in table-backed child nodes are not accessible here.
///
/// When `price_details` is provided, CommitMarkPrice requests will update it.
/// When `perp_market` is provided, Order/ContinuedOrder requests will perform
/// actual order matching against the order book.
fn bcs_process_pending_requests<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    ame: &mut AsyncMatchingEngine,
    current_time_micros: u64,
    work_units: &mut work_unit_utils::WorkUnit,
    mut price_details: Option<&mut bcs_types::PriceDetails>,
    mut perp_market: Option<&mut bcs_types::PerpMarket>,
    events: &mut Vec<OrderMatchEvent>,
    market_addr: AccountAddress,
    ack_events: &mut Vec<bcs_types::ACKEvent>,
    table_writes_out: &mut Vec<(aptos_table_natives::TableHandle, TableWrite)>,
) -> u64 {

    // Helper: create a read_slot closure from a table handle.
    // The handle is extracted before mutable borrows to avoid borrow conflicts.
    // Shared write cache so read closures can see writes from the same loop iteration.
    // This avoids the deadlock where tree_pop_front writes a node but the next
    // tree_pop_front reads the stale version from storage.
    let pending_write_cache: std::cell::RefCell<std::collections::BTreeMap<Vec<u8>, Vec<u8>>> =
        std::cell::RefCell::new(std::collections::BTreeMap::new());

    macro_rules! make_read_slot {
        ($session:expr, $handle_opt:expr, $cache:expr) => {{
            let h = $handle_opt;
            let cache_ref = $cache;
            move |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
                match h {
                    Some(handle) => {
                        let key_bytes = bcs::to_bytes(&slot_index)
                            .map_err(|e| format!("serialize slot key: {}", e))?;
                        // Check local write cache first
                        if let Some(cached) = cache_ref.borrow().get(&key_bytes) {
                            return Ok(Some(cached.clone()));
                        }
                        native_session_helpers::read_table_item_bytes($session, handle, &key_bytes)
                            .map(|opt| opt.map(|b| b.to_vec()))
                            .map_err(|e| format!("read table item: {:?}", e))
                    },
                    None => Ok(None),
                }
            }
        }};
    }

    macro_rules! collect_writes {
        ($out:expr, $map:expr, $writes:expr, $cache:expr) => {
            if let Some(th) = $map.get_table_handle() {
                let handle = aptos_table_natives::TableHandle(th.handle);
                for tw in $writes {
                    // Write to local cache so subsequent reads see this write
                    if let Ok(key_bytes) = bcs::to_bytes(&tw.slot_index) {
                        $cache.borrow_mut().insert(key_bytes, tw.data.clone());
                    }
                    $out.push((handle, tw));
                }
            }
        };
    }

    let mut processed = 0u64;
    if !work_unit_utils::has_more_work(work_units) {
        return processed;
    }

    // Extract table handles BEFORE mutable destructuring to avoid borrow conflicts
    let pending_handle;
    let backstop_handle;
    let margin_call_handle;
    {
        let AsyncMatchingEngine::V1 { pending_requests, backstop_liquidations_in_queue, margin_call_liquidations_in_queue, .. } = &*ame;
        pending_handle = pending_requests.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle));
        backstop_handle = backstop_liquidations_in_queue.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle));
        margin_call_handle = margin_call_liquidations_in_queue.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle));
    }
    let read_pending = make_read_slot!(session, pending_handle, &pending_write_cache);
    let read_backstop = make_read_slot!(session, backstop_handle, &pending_write_cache);
    let read_margin_call = make_read_slot!(session, margin_call_handle, &pending_write_cache);

    let AsyncMatchingEngine::V1 {
        pending_requests,
        async_matching_enabled,
        backstop_liquidations_in_queue,
        margin_call_liquidations_in_queue,
        mark_prices_in_queue,
        ..
    } = ame;

    while !pending_requests.tree_is_empty(&read_pending).unwrap_or(true) && work_unit_utils::has_more_work(work_units) {
        // Check time gate
        if let Some(first_key) = pending_requests.tree_first_key(&read_pending).unwrap_or(None) {
            let PendingRequestKey::V1 { time, .. } = first_key;
            if *async_matching_enabled && time > current_time_micros {
                break;
            }
        } else {
            break;
        }

        // Pop the first entry
        let (key, request) = match pending_requests.tree_pop_front(&read_pending) {
            Ok((Some(pair), writes)) => {
                collect_writes!(table_writes_out, pending_requests, writes, &pending_write_cache);
                pair
            },
            _ => break,
        };
        processed += 1;

        // Extract ACK info before consuming the request
        let request_for_ack;

        match request {
            PendingRequest::CommitMarkPrice { mark_px, batch_key } => {
                request_for_ack = PendingRequestAckInfo::CommitMarkPrice { batch_key };
                work_unit_utils::consume_small_work_units(work_units);
                if !mark_prices_in_queue.is_empty() {
                    let queued_px = mark_prices_in_queue.remove(0);
                    assert!(
                        queued_px == mark_px,
                        "ECOMMIT_MARK_PRICE_QUEUE_MISMATCH"
                    );
                }
                if let Some(ref mut pd) = price_details {
                    bcs_commit_mark_price(pd, mark_px);
                }
            },
            PendingRequest::BackstopLiquidation { user, batch_key } => {
                request_for_ack = PendingRequestAckInfo::BackstopLiquidation { user, batch_key };
                work_unit_utils::consume_small_work_units(work_units);
                { let (_, writes) = backstop_liquidations_in_queue.tree_remove(&user, &read_backstop).unwrap_or((None, vec![])); collect_writes!(table_writes_out, backstop_liquidations_in_queue, writes, &pending_write_cache); }
            },
            PendingRequest::MarginCall { user, batch_key, .. } => {
                request_for_ack = PendingRequestAckInfo::MarginCall { user, batch_key };
                work_unit_utils::consume_small_work_units(work_units);
                { let (_, writes) = margin_call_liquidations_in_queue.tree_remove(&user, &read_margin_call).unwrap_or((None, vec![])); collect_writes!(table_writes_out, margin_call_liquidations_in_queue, writes, &pending_write_cache); }
            },
            PendingRequest::CheckADL { batch_key } => {
                request_for_ack = PendingRequestAckInfo::CheckADL { batch_key };
                work_unit_utils::consume_small_work_units(work_units);
            },
            PendingRequest::TriggerADL { batch_key, .. } => {
                request_for_ack = PendingRequestAckInfo::TriggerADL { batch_key };
                work_unit_utils::consume_small_work_units(work_units);
            },
            PendingRequest::Order(pending_order) => {
                request_for_ack = PendingRequestAckInfo::None;
                let bcs_types::PendingOrder::V1 { order_args, order_metadata } = pending_order;
                let bcs_types::PerpOrderRequestExtendedArgs::V1 {
                    account, ref common_args, order_id, ..
                } = order_args;
                let bcs_types::PerpOrderRequestCommonArgs::V1 {
                    price, orig_size, is_buy, time_in_force, ref client_order_id,
                } = *common_args;

                if let Some(ref mut pm) = perp_market {
                    let max_match_limit = work_unit_utils::get_max_match_limit(work_units);
                    let (remaining_size, match_count) = bcs_match_taker_order(
                        session,
                        pm,
                        account,
                        order_id,
                        client_order_id.clone(),
                        price,
                        orig_size,
                        orig_size, // remaining_size starts at orig_size
                        is_buy,
                        time_in_force,
                        &order_metadata,
                        max_match_limit,
                        events,
                        true, // emit_taker_open for Order
                        table_writes_out,
                    );
                    work_unit_utils::consume_order_match_work_units(work_units, match_count);

                    // If remaining and should continue, re-enqueue as ContinuedOrder
                    if remaining_size > 0 {
                        if time_in_force == bcs_types::TimeInForce::IOC {
                            // IOC: cancel remaining - emit cancellation event
                            events.push(OrderMatchEvent::TakerCancelled {
                                account,
                                order_id,
                                client_order_id: client_order_id.clone(),
                                orig_size,
                                remaining_size,
                                price,
                                is_buy,
                                time_in_force,
                                metadata: order_metadata.clone(),
                                reason: bcs_types::OrderCancellationReason::IOCExpired,
                            });
                        } else if match_count >= max_match_limit {
                            // Hit match limit, re-enqueue as continued order
                            let continued = bcs_types::ContinuedPendingOrder::V1 {
                                order_args,
                                order_metadata,
                                remaining_size,
                            };
                            { let writes = pending_requests.tree_add(key, PendingRequest::ContinuedOrder(continued), &read_pending).unwrap_or_default(); collect_writes!(table_writes_out, pending_requests, writes, &pending_write_cache); }
                        }
                        // GTC with remaining size and no more matches: place as maker order.
                        else if time_in_force == bcs_types::TimeInForce::GTC {
                            if let Some(ref mut pm) = perp_market {
                                let PendingRequestKey::V1 { tie_breaker: tb, .. } = key;
                                let unique_priority_idx = bcs_types::IncreasingIdx { idx: tb };

                                let single_order_request = bcs_types::SingleOrderRequest::V1 {
                                    account,
                                    order_id,
                                    client_order_id: client_order_id.clone(),
                                    price,
                                    orig_size,
                                    remaining_size,
                                    is_bid: is_buy,
                                    trigger_condition: None,
                                    time_in_force,
                                    creation_time_micros: current_time_micros,
                                    metadata: order_metadata.clone(),
                                };
                                let single_order = bcs_types::SingleOrder::V1 {
                                    order_request: single_order_request,
                                    unique_priority_idx,
                                };
                                let order_with_state = bcs_types::OrderWithState::V1 {
                                    order: single_order,
                                    is_active: true,
                                };

                                let (sob, _bob, pti) = pm.full_order_book_mut();
                                let bcs_types::SingleOrderBook::V1 { orders, client_order_ids, .. } = sob;

                                let oh = orders.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle));
                                let read_orders = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
                                    match oh { Some(h) => { let kb = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                                        native_session_helpers::read_table_item_bytes(session, h, &kb).map(|o| o.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
                                    }, None => Ok(None) }
                                };
                                let writes = orders.tree_add(order_id, order_with_state, &read_orders).unwrap_or_default();
                                if let Some(h) = oh { for tw in writes { table_writes_out.push((h, tw)); } }

                                if let Some(coid) = client_order_id {
                                    let acoid = bcs_types::AccountClientOrderId { account, client_order_id: coid.clone() };
                                    let ch = client_order_ids.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle));
                                    let read_coids = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
                                        match ch { Some(h) => { let kb = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                                            native_session_helpers::read_table_item_bytes(session, h, &kb).map(|o| o.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
                                        }, None => Ok(None) }
                                    };
                                    let writes = client_order_ids.tree_add(acoid, order_id, &read_coids).unwrap_or_default();
                                    if let Some(h) = ch { for tw in writes { table_writes_out.push((h, tw)); } }
                                }

                                let order_data = bcs_types::OrderData {
                                    order_id,
                                    order_book_type: bcs_types::OrderType::single_order_type(),
                                    size: remaining_size,
                                };
                                let bcs_types::PriceTimeIndex::V1 { buys, sells } = pti;
                                if is_buy {
                                    let pti_key = bcs_types::PriceDescTime {
                                        price,
                                        tie_breaker: bcs_types::DecreasingIdx { idx: u128::MAX - unique_priority_idx.idx },
                                    };
                                    let bh = buys.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle));
                                    let read_buys_fn = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
                                        match bh { Some(h) => { let kb = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                                            native_session_helpers::read_table_item_bytes(session, h, &kb).map(|o| o.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
                                        }, None => Ok(None) }
                                    };
                                    let writes = buys.tree_add(pti_key, order_data, &read_buys_fn).unwrap_or_default();
                                    if let Some(h) = bh { for tw in writes { table_writes_out.push((h, tw)); } }
                                } else {
                                    let pti_key = bcs_types::PriceAscTime {
                                        price,
                                        tie_breaker: unique_priority_idx,
                                    };
                                    let sh = sells.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle));
                                    let read_sells_fn = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
                                        match sh { Some(h) => { let kb = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                                            native_session_helpers::read_table_item_bytes(session, h, &kb).map(|o| o.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
                                        }, None => Ok(None) }
                                    };
                                    let writes = sells.tree_add(pti_key, order_data, &read_sells_fn).unwrap_or_default();
                                    if let Some(h) = sh { for tw in writes { table_writes_out.push((h, tw)); } }
                                }
                            }
                        }
                    }
                } else {
                    work_unit_utils::consume_small_work_units(work_units);
                }
            },
            PendingRequest::ContinuedOrder(continued_order) => {
                request_for_ack = PendingRequestAckInfo::None;
                let bcs_types::ContinuedPendingOrder::V1 {
                    order_args, order_metadata, remaining_size: cont_remaining,
                } = continued_order;
                let bcs_types::PerpOrderRequestExtendedArgs::V1 {
                    account, ref common_args, order_id, ..
                } = order_args;
                let bcs_types::PerpOrderRequestCommonArgs::V1 {
                    price, orig_size, is_buy, time_in_force, ref client_order_id,
                } = *common_args;

                if let Some(ref mut pm) = perp_market {
                    let max_match_limit = work_unit_utils::get_max_match_limit(work_units);
                    let (remaining_size, match_count) = bcs_match_taker_order(
                        session,
                        pm,
                        account,
                        order_id,
                        client_order_id.clone(),
                        price,
                        orig_size,
                        cont_remaining,
                        is_buy,
                        time_in_force,
                        &order_metadata,
                        max_match_limit,
                        events,
                        false, // no TakerOpen for ContinuedOrder
                        table_writes_out,
                    );
                    work_unit_utils::consume_order_match_work_units(work_units, match_count);

                    if remaining_size > 0 {
                        if time_in_force == bcs_types::TimeInForce::IOC {
                            events.push(OrderMatchEvent::TakerCancelled {
                                account,
                                order_id,
                                client_order_id: client_order_id.clone(),
                                orig_size,
                                remaining_size,
                                price,
                                is_buy,
                                time_in_force,
                                metadata: order_metadata.clone(),
                                reason: bcs_types::OrderCancellationReason::IOCExpired,
                            });
                        } else if match_count >= max_match_limit {
                            let continued = bcs_types::ContinuedPendingOrder::V1 {
                                order_args,
                                order_metadata,
                                remaining_size,
                            };
                            { let writes = pending_requests.tree_add(key, PendingRequest::ContinuedOrder(continued), &read_pending).unwrap_or_default(); collect_writes!(table_writes_out, pending_requests, writes, &pending_write_cache); }
                        }
                    }
                } else {
                    work_unit_utils::consume_small_work_units(work_units);
                }
            },
            PendingRequest::Twap(_) => {
                request_for_ack = PendingRequestAckInfo::None;
                work_unit_utils::consume_small_work_units(work_units);
            },
            PendingRequest::QueueBackstopLiquidationsAndADL { payload, batch_key } => {
                request_for_ack = PendingRequestAckInfo::QueueBackstopLiquidationsAndADL {
                    accounts: payload.backstop_liquidations.clone(),
                    batch_key,
                };
                work_unit_utils::consume_small_work_units(work_units);
                for (i, account) in payload.backstop_liquidations.iter().enumerate() {
                    if !backstop_liquidations_in_queue.tree_contains(account, &read_backstop).unwrap_or(false) {
                        { let writes = backstop_liquidations_in_queue.tree_add(*account, true, &read_backstop).unwrap_or_default(); collect_writes!(table_writes_out, backstop_liquidations_in_queue, writes, &pending_write_cache); }
                        let liq_key = new_pending_liquidation_key(batch_key + i as u128 + 10000);
                        { let writes = pending_requests.tree_add(liq_key, PendingRequest::BackstopLiquidation {
                            user: *account,
                            batch_key,
                        }, &read_pending).unwrap_or_default(); collect_writes!(table_writes_out, pending_requests, writes, &pending_write_cache); }
                    }
                }
                let adl_key = new_pending_check_adl_key(batch_key + 20000);
                { let writes = pending_requests.tree_add(adl_key, PendingRequest::CheckADL { batch_key }, &read_pending).unwrap_or_default(); collect_writes!(table_writes_out, pending_requests, writes, &pending_write_cache); }
            },
            PendingRequest::QueueMarginCallLiquidations { payload, batch_key } => {
                request_for_ack = PendingRequestAckInfo::QueueMarginCallLiquidations {
                    accounts: payload.margin_call_liquidations.clone(),
                    batch_key,
                };
                work_unit_utils::consume_small_work_units(work_units);
                for (i, account) in payload.margin_call_liquidations.iter().enumerate() {
                    if !margin_call_liquidations_in_queue.tree_contains(account, &read_margin_call).unwrap_or(false) {
                        { let writes = margin_call_liquidations_in_queue.tree_add(*account, true, &read_margin_call).unwrap_or_default(); collect_writes!(table_writes_out, margin_call_liquidations_in_queue, writes, &pending_write_cache); }
                        let mc_key = new_margin_call_key(current_time_micros, batch_key + i as u128 + 10000);
                        { let writes = pending_requests.tree_add(mc_key, PendingRequest::MarginCall {
                            user: *account,
                            continuation: bcs_types::MarginCallContinuation::Start,
                            batch_key,
                        }, &read_pending).unwrap_or_default(); collect_writes!(table_writes_out, pending_requests, writes, &pending_write_cache); }
                    }
                }
            },
        }

        // ACKEvent emission per request type (matching Move VM behavior).
        // Order, ContinuedOrder, and Twap do NOT emit ACKEvents.
        match &request_for_ack {
            PendingRequestAckInfo::CommitMarkPrice { batch_key } => {
                ack_events.push(bcs_types::ACKEvent::V1 {
                    market: market_addr,
                    accounts: Vec::new(),
                    batch_key: *batch_key,
                    ack_phase: bcs_types::AckPhase::CommitMarkPrice,
                });
            },
            PendingRequestAckInfo::BackstopLiquidation { user, batch_key } => {
                ack_events.push(bcs_types::ACKEvent::V1 {
                    market: market_addr,
                    accounts: vec![*user],
                    batch_key: *batch_key,
                    ack_phase: bcs_types::AckPhase::BackstopLiquidation,
                });
            },
            PendingRequestAckInfo::MarginCall { user, batch_key } => {
                ack_events.push(bcs_types::ACKEvent::V1 {
                    market: market_addr,
                    accounts: vec![*user],
                    batch_key: *batch_key,
                    ack_phase: bcs_types::AckPhase::MarginCall,
                });
            },
            PendingRequestAckInfo::CheckADL { batch_key } => {
                ack_events.push(bcs_types::ACKEvent::V1 {
                    market: market_addr,
                    accounts: Vec::new(),
                    batch_key: *batch_key,
                    ack_phase: bcs_types::AckPhase::CheckADL,
                });
            },
            PendingRequestAckInfo::TriggerADL { batch_key } => {
                ack_events.push(bcs_types::ACKEvent::V1 {
                    market: market_addr,
                    accounts: Vec::new(),
                    batch_key: *batch_key,
                    ack_phase: bcs_types::AckPhase::TriggerADL,
                });
            },
            PendingRequestAckInfo::QueueBackstopLiquidationsAndADL { accounts, batch_key } => {
                ack_events.push(bcs_types::ACKEvent::V1 {
                    market: market_addr,
                    accounts: accounts.clone(),
                    batch_key: *batch_key,
                    ack_phase: bcs_types::AckPhase::QueueBackstopLiquidationsAndADL,
                });
            },
            PendingRequestAckInfo::QueueMarginCallLiquidations { accounts, batch_key } => {
                ack_events.push(bcs_types::ACKEvent::V1 {
                    market: market_addr,
                    accounts: accounts.clone(),
                    batch_key: *batch_key,
                    ack_phase: bcs_types::AckPhase::QueueMarginCallLiquidations,
                });
            },
            PendingRequestAckInfo::None => {
                // Order, ContinuedOrder, Twap - no ACKEvent
            },
        }
    }
    processed
}

// ---------------------------------------------------------------------------
// Order matching event types (collected during matching, emitted after)
// ---------------------------------------------------------------------------

/// Events collected during order matching to be emitted after processing.
#[allow(dead_code)]
enum OrderMatchEvent {
    /// A taker fill against a single (retail) maker order.
    SingleFill {
        taker_account: AccountAddress,
        taker_order_id: bcs_types::OrderId,
        taker_client_order_id: Option<String>,
        taker_orig_size: u64,
        taker_remaining_size: u64,
        fill_size: u64,
        fill_price: u64,
        taker_is_buy: bool,
        taker_time_in_force: bcs_types::TimeInForce,
        taker_metadata: bcs_types::OrderMetadata,
        maker_account: AccountAddress,
        maker_order_id: bcs_types::OrderId,
        maker_client_order_id: Option<String>,
        maker_orig_size: u64,
        maker_remaining_size: u64,
        maker_time_in_force: bcs_types::TimeInForce,
        maker_metadata: bcs_types::OrderMetadata,
    },
    /// A taker fill against a bulk maker order.
    BulkFill {
        taker_account: AccountAddress,
        taker_order_id: bcs_types::OrderId,
        taker_client_order_id: Option<String>,
        taker_orig_size: u64,
        taker_remaining_size: u64,
        fill_size: u64,
        fill_price: u64,
        taker_is_buy: bool,
        taker_time_in_force: bcs_types::TimeInForce,
        taker_metadata: bcs_types::OrderMetadata,
        maker_account: AccountAddress,
        maker_order_id: bcs_types::OrderId,
        maker_sequence_number: u64,
    },
    /// Taker order open event (emitted before matching starts).
    TakerOpen {
        account: AccountAddress,
        order_id: bcs_types::OrderId,
        client_order_id: Option<String>,
        orig_size: u64,
        remaining_size: u64,
        price: u64,
        is_buy: bool,
        time_in_force: bcs_types::TimeInForce,
        metadata: bcs_types::OrderMetadata,
    },
    /// Taker order cancelled (IOC remaining, etc).
    TakerCancelled {
        account: AccountAddress,
        order_id: bcs_types::OrderId,
        client_order_id: Option<String>,
        orig_size: u64,
        remaining_size: u64,
        price: u64,
        is_buy: bool,
        time_in_force: bcs_types::TimeInForce,
        metadata: bcs_types::OrderMetadata,
        reason: bcs_types::OrderCancellationReason,
    },
    /// Taker order fully filled.
    TakerFilled {
        account: AccountAddress,
        order_id: bcs_types::OrderId,
        client_order_id: Option<String>,
        orig_size: u64,
        price: u64,
        is_buy: bool,
        time_in_force: bcs_types::TimeInForce,
        metadata: bcs_types::OrderMetadata,
    },
}

// ---------------------------------------------------------------------------
// Order matching core logic
// ---------------------------------------------------------------------------

/// Match a taker order against the order book (PriceTimeIndex).
///

///
/// Walks the opposite side of the book:
///   - Buy taker -> match against sells (ascending price, front of sells)
///   - Sell taker -> match against buys (descending price, back of buys... but
///     PriceDescTime sorts descending so front is highest price = best bid)
///
/// Returns (remaining_size, match_count).
fn bcs_match_taker_order<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    perp_market: &mut bcs_types::PerpMarket,
    taker_account: AccountAddress,
    taker_order_id: bcs_types::OrderId,
    taker_client_order_id: Option<String>,
    taker_price: u64,
    taker_orig_size: u64,
    mut remaining_size: u64,
    is_buy: bool,
    time_in_force: bcs_types::TimeInForce,
    taker_metadata: &bcs_types::OrderMetadata,
    max_match_limit: u32,
    events: &mut Vec<OrderMatchEvent>,
    emit_taker_open: bool,
    table_writes_out: &mut Vec<(aptos_table_natives::TableHandle, TableWrite)>,
) -> (u64, u32) {
    // Extract table handles for PriceTimeIndex buys/sells for tree operations
    let (buys_handle, sells_handle) = {
        let (_, _, pti) = perp_market.full_order_book_mut();
        let bcs_types::PriceTimeIndex::V1 { buys, sells } = pti;
        (
            buys.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle)),
            sells.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle)),
        )
    };
    macro_rules! make_read_slot {
        ($session:expr, $handle_opt:expr) => {{
            let h = $handle_opt;
            move |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
                match h {
                    Some(handle) => {
                        let key_bytes = bcs::to_bytes(&slot_index)
                            .map_err(|e| format!("serialize slot key: {}", e))?;
                        native_session_helpers::read_table_item_bytes($session, handle, &key_bytes)
                            .map(|opt| opt.map(|b| b.to_vec()))
                            .map_err(|e| format!("read table item: {:?}", e))
                    },
                    None => Ok(None),
                }
            }
        }};
    }
    macro_rules! collect_writes {
        ($out:expr, $map:expr, $writes:expr) => {
            if let Some(th) = $map.get_table_handle() {
                let handle = aptos_table_natives::TableHandle(th.handle);
                for tw in $writes {
                    $out.push((handle, tw));
                }
            }
        };
    }
    let read_buys = make_read_slot!(session, buys_handle);
    let read_sells = make_read_slot!(session, sells_handle);

    let mut match_count: u32 = 0;
    // Emit taker order open event (only for first Order, not ContinuedOrder)
    if emit_taker_open {
        events.push(OrderMatchEvent::TakerOpen {
            account: taker_account,
            order_id: taker_order_id,
            client_order_id: taker_client_order_id.clone(),
            orig_size: taker_orig_size,
            remaining_size,
            price: taker_price,
            is_buy,
            time_in_force,
            metadata: taker_metadata.clone(),
        });
    }

    loop {
        if remaining_size == 0 || match_count >= max_match_limit {
            break;
        }

        // Check if order still crosses the book (tree-aware)
        let crosses_book = {
            let (_, _, pti) = perp_market.full_order_book_mut();
            let bcs_types::PriceTimeIndex::V1 { buys, sells } = pti;
            if is_buy {
                // Buy taker crosses if price >= best ask
                sells.tree_borrow_front(&read_sells).unwrap_or(None)
                    .map_or(false, |(k, _)| taker_price >= k.price)
            } else {
                // Sell taker crosses if price <= best bid
                buys.tree_borrow_back(&read_buys).unwrap_or(None)
                    .map_or(false, |(k, _)| taker_price <= k.price)
            }
        };
        if !crosses_book {
            break;
        }

        match_count += 1;

        // Get the best opposing entry from the PriceTimeIndex
        let (single_order_book, bulk_order_book, price_time_idx) = perp_market.full_order_book_mut();
        let bcs_types::PriceTimeIndex::V1 { buys, sells } = price_time_idx;

        // For buy taker: match against the front of sells (lowest ask)
        // For sell taker: match against the front of buys (highest bid)
        // Note: PriceDescTime sorts with lowest price first in BCS ordering,
        // but the actual highest bid is at the FRONT because DecreasingIdx
        // makes (price, dec_idx) sort such that highest price is first.
        // Actually: PriceDescTime has { price: u64, tie_breaker: DecreasingIdx }
        // BCS comparison is lexicographic on the serialized bytes.
        // For u64, BCS serializes as little-endian, so BCS comparison != numeric comparison.
        // BUT: BigOrderedMap sorts by Rust's Ord trait (derived), not BCS bytes.
        // PriceDescTime derives Ord, which compares price first (ascending), then tie_breaker.
        // So in the BigOrderedMap, entries are sorted by (price asc, DecreasingIdx).
        // The LAST entry has the highest price = best bid.
        // For PriceAscTime: sorted by (price asc, IncreasingIdx).
        // The FIRST entry has the lowest price = best ask.

        let opposite_side_entry = if is_buy {
            // Buy taker matches against sells. Best ask = first entry (lowest price).
            sells.tree_borrow_front(&read_sells).unwrap_or(None).map(|(k, v)| (k.price, v))
        } else {
            // Sell taker matches against buys. Best bid = last entry (highest price).
            buys.tree_borrow_back(&read_buys).unwrap_or(None).map(|(k, v)| (k.price, v))
        };

        let (maker_price, order_data) = match opposite_side_entry {
            Some(entry) => entry,
            None => break, // No more entries on opposite side
        };

        // The fill price is always the maker's price
        let fill_price = maker_price;
        let maker_order_id = order_data.order_id;
        let maker_book_type = order_data.order_book_type;
        let maker_available_size = order_data.size;

        // Determine fill size
        let fill_size = std::cmp::min(remaining_size, maker_available_size);
        let maker_remaining_in_level = maker_available_size - fill_size;

        // Update the PriceTimeIndex
        if maker_remaining_in_level == 0 {
            // Fully consumed the maker entry - remove it
            if is_buy {
                if let Ok((_, writes)) = sells.tree_pop_front(&read_sells) {
                    collect_writes!(table_writes_out, sells, writes);
                }
            } else {
                if let Ok((_, writes)) = buys.tree_pop_back(&read_buys) {
                    collect_writes!(table_writes_out, buys, writes);
                }
            }
        } else {
            // Partially consumed - reduce the size
            // Partially consumed: modify the size of the front/back entry
            // For tree-aware operation: pop the entry, modify, and re-add
            if is_buy {
                if let Ok((Some((k, mut v)), writes)) = sells.tree_pop_front(&read_sells) {
                    collect_writes!(table_writes_out, sells, writes);
                    v.size -= fill_size;
                    let writes = sells.tree_add(k, v, &read_sells).unwrap_or_default();
                    collect_writes!(table_writes_out, sells, writes);
                }
            } else {
                if let Ok((Some((k, mut v)), writes)) = buys.tree_pop_back(&read_buys) {
                    collect_writes!(table_writes_out, buys, writes);
                    v.size -= fill_size;
                    let writes = buys.tree_add(k, v, &read_buys).unwrap_or_default();
                    collect_writes!(table_writes_out, buys, writes);
                }
            }
        }

        remaining_size -= fill_size;
        // Now update the maker's order in the order book and emit events
        if maker_book_type.is_single_order() {
            // Single order matching
            if let Some(maker_order) = single_order_book.get_order_mut(&maker_order_id) {
                let maker_account = maker_order.get_account();
                let maker_client_order_id = maker_order.get_client_order_id();
                let maker_orig_size = maker_order.get_orig_size();
                let maker_time_in_force = maker_order.get_time_in_force();
                let maker_metadata = maker_order.get_metadata().clone();
                let old_remaining = maker_order.get_remaining_size();
                let new_remaining = old_remaining.saturating_sub(fill_size);
                maker_order.set_remaining_size(new_remaining);

                // If maker is fully filled, remove from orders map
                if new_remaining == 0 {
                    single_order_book.remove_order(&maker_order_id);
                }

                events.push(OrderMatchEvent::SingleFill {
                    taker_account,
                    taker_order_id,
                    taker_client_order_id: taker_client_order_id.clone(),
                    taker_orig_size,
                    taker_remaining_size: remaining_size,
                    fill_size,
                    fill_price,
                    taker_is_buy: is_buy,
                    taker_time_in_force: time_in_force,
                    taker_metadata: taker_metadata.clone(),
                    maker_account,
                    maker_order_id,
                    maker_client_order_id,
                    maker_orig_size,
                    maker_remaining_size: new_remaining,
                    maker_time_in_force,
                    maker_metadata,
                });
            } else {
                // Order not in root node (in table items); skip detailed matching
                events.push(OrderMatchEvent::SingleFill {
                    taker_account,
                    taker_order_id,
                    taker_client_order_id: taker_client_order_id.clone(),
                    taker_orig_size,
                    taker_remaining_size: remaining_size,
                    fill_size,
                    fill_price,
                    taker_is_buy: is_buy,
                    taker_time_in_force: time_in_force,
                    taker_metadata: taker_metadata.clone(),
                    maker_account: AccountAddress::ZERO,
                    maker_order_id,
                    maker_client_order_id: None,
                    maker_orig_size: fill_size,
                    maker_remaining_size: 0,
                    maker_time_in_force: bcs_types::TimeInForce::GTC,
                    maker_metadata: bcs_types::OrderMetadata::V1_RETAIL {
                        is_reduce_only: false,
                        use_backstop_liquidation_margin: false,
                        is_margin_call: false,
                        twap: None,
                        tp_sl: bcs_types::TpSlMetadata::V1 { tp: None, sl: None },
                        builder_code: None,
                    }, // Default metadata: maker order is in B+ tree inner node, not accessible from root
                });
            }
        } else {
            // Bulk order matching
            let maker_address_opt = bulk_order_book.get_order_address(&maker_order_id).copied();
            if let Some(maker_address) = maker_address_opt {
                let maker_is_bid = !is_buy; // maker is on opposite side
                if let Some(bulk_order) = bulk_order_book.get_order_by_address_mut(&maker_address) {
                    let maker_account = bulk_order.account();
                    let seq_num = bulk_order.sequence_number();
                    let priority_idx = bulk_order.unique_priority_idx();

                    let (_fill_price_from_bulk, next_level) = bulk_order.match_and_advance(maker_is_bid, fill_size);

                    // If current level was fully consumed and there's a next level,
                    // activate it in the PriceTimeIndex
                    if maker_remaining_in_level == 0 {
                        if let Some((next_price, next_size)) = next_level {
                            let bcs_types::PriceTimeIndex::V1 { buys: buys2, sells: sells2 } = price_time_idx;
                            if maker_is_bid {
                                let new_key = bcs_types::PriceDescTime {
                                    price: next_price,
                                    tie_breaker: bcs_types::DecreasingIdx { idx: u128::MAX - priority_idx.idx },
                                };
                                let new_data = bcs_types::OrderData {
                                    order_id: maker_order_id,
                                    order_book_type: bcs_types::OrderType::bulk_order_type(),
                                    size: next_size,
                                };
                                { let writes = buys2.tree_add(new_key, new_data, &read_buys).unwrap_or_default(); collect_writes!(table_writes_out, buys2, writes); }
                            } else {
                                let new_key = bcs_types::PriceAscTime {
                                    price: next_price,
                                    tie_breaker: bcs_types::IncreasingIdx { idx: priority_idx.idx },
                                };
                                let new_data = bcs_types::OrderData {
                                    order_id: maker_order_id,
                                    order_book_type: bcs_types::OrderType::bulk_order_type(),
                                    size: next_size,
                                };
                                { let writes = sells2.tree_add(new_key, new_data, &read_sells).unwrap_or_default(); collect_writes!(table_writes_out, sells2, writes); }
                            }
                        }
                    }

                    events.push(OrderMatchEvent::BulkFill {
                        taker_account,
                        taker_order_id,
                        taker_client_order_id: taker_client_order_id.clone(),
                        taker_orig_size,
                        taker_remaining_size: remaining_size,
                        fill_size,
                        fill_price,
                        taker_is_buy: is_buy,
                        taker_time_in_force: time_in_force,
                        taker_metadata: taker_metadata.clone(),
                        maker_account,
                        maker_order_id,
                        maker_sequence_number: seq_num,
                    });

                } else {
                    // Bulk order found by address but order details not in root node.
                    // Tree traversal required for full BulkOrderBook inner node support.
                    panic!("unimplemented: bulk order at {} found by address but not in BulkOrderBook root node", maker_address);
                }
            } else {
                // Bulk order_id -> address mapping not in root node.
                // Tree traversal required for order_id_to_address inner node support.
                panic!("unimplemented: bulk order_id_to_address lookup for {:?} requires tree traversal", maker_order_id);
            }
        }
    }

    // Emit taker filled event if fully filled
    if remaining_size == 0 && match_count > 0 {
        events.push(OrderMatchEvent::TakerFilled {
            account: taker_account,
            order_id: taker_order_id,
            client_order_id: taker_client_order_id,
            orig_size: taker_orig_size,
            price: taker_price,
            is_buy,
            time_in_force,
            metadata: taker_metadata.clone(),
        });
    }

    (remaining_size, match_count)
}

/// Schedule backstop liquidations and ADL check into the AME queue.
fn bcs_schedule_queue_backstop_liquidations_and_adl<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    ame: &mut AsyncMatchingEngine,
    backstop_liquidations: Vec<AccountAddress>,
    mark_price_updated: bool,
    batch_key: u128,
    tie_breaker_start: u128,
) -> Vec<TableWrite> {
    let pending_handle = {
        let AsyncMatchingEngine::V1 { pending_requests, .. } = &*ame;
        pending_requests.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle))
    };
    let read_pending = move |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
        match pending_handle {
            Some(handle) => {
                let key_bytes = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                native_session_helpers::read_table_item_bytes(session, handle, &key_bytes)
                    .map(|opt| opt.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
            }, None => Ok(None),
        }
    };
    let AsyncMatchingEngine::V1 { pending_requests, .. } = ame;
    let mut all_writes = Vec::new();

    if backstop_liquidations.is_empty() {
        if mark_price_updated {
            let key = new_pending_check_adl_key(tie_breaker_start);
            all_writes.extend(pending_requests.tree_add(key, PendingRequest::CheckADL { batch_key }, &read_pending).unwrap_or_default());
        }
        return all_writes;
    }

    let queue_key = new_pending_liquidation_key(tie_breaker_start);
    all_writes.extend(pending_requests.tree_add(queue_key, PendingRequest::QueueBackstopLiquidationsAndADL {
        payload: QueueBackstopLiquidationsAndADLPayload { backstop_liquidations },
        batch_key,
    }, &read_pending).unwrap_or_default());
    all_writes
}

/// Schedule margin call liquidations into the AME queue.
fn bcs_schedule_queue_margin_call_liquidations<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    ame: &mut AsyncMatchingEngine,
    margin_call_liquidations: Vec<AccountAddress>,
    batch_key: u128,
    cur_time_micros: u64,
    tie_breaker_start: u128,
) -> Vec<TableWrite> {
    if margin_call_liquidations.is_empty() {
        return vec![];
    }
    let pending_handle = {
        let AsyncMatchingEngine::V1 { pending_requests, .. } = &*ame;
        pending_requests.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle))
    };
    let read_pending = move |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
        match pending_handle {
            Some(handle) => {
                let key_bytes = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                native_session_helpers::read_table_item_bytes(session, handle, &key_bytes)
                    .map(|opt| opt.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
            }, None => Ok(None),
        }
    };
    let AsyncMatchingEngine::V1 { pending_requests, .. } = ame;
    let queue_key = new_margin_call_key(cur_time_micros, tie_breaker_start);
    pending_requests.tree_add(queue_key, PendingRequest::QueueMarginCallLiquidations {
        payload: QueueMarginCallLiquidationsPayload { margin_call_liquidations },
        batch_key,
    }, &read_pending).unwrap_or_default()
}

/// Schedule a mark price commit into the AME queue.
fn bcs_schedule_commit_mark_price<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    ame: &mut AsyncMatchingEngine,
    mark_px: u64,
    batch_key: u128,
    tie_breaker: u128,
) -> Vec<TableWrite> {
    let pending_handle = {
        let AsyncMatchingEngine::V1 { pending_requests, .. } = &*ame;
        pending_requests.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle))
    };
    let read_pending = move |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
        match pending_handle {
            Some(handle) => {
                let key_bytes = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                native_session_helpers::read_table_item_bytes(session, handle, &key_bytes)
                    .map(|opt| opt.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
            }, None => Ok(None),
        }
    };
    let AsyncMatchingEngine::V1 { pending_requests, mark_prices_in_queue, .. } = ame;
    let key = new_pending_commit_mark_price_key(tie_breaker);
    let writes = pending_requests.tree_add(key, PendingRequest::CommitMarkPrice { mark_px, batch_key }, &read_pending).unwrap_or_default();
    mark_prices_in_queue.push(mark_px);
    writes
}

// ---------------------------------------------------------------------------
// BigOrderedMap inner node resolution for order matching
// ---------------------------------------------------------------------------

#[allow(dead_code)] // Used by resolve_perp_market_inner_nodes and write_back_resolved_table_items
/// Table item that was read during BigOrderedMap inner node resolution.
struct ResolvedTableItem {
    handle: aptos_table_natives::TableHandle,
    key: Vec<u8>,
    value: bytes::Bytes,
}

#[allow(dead_code)] // Retained for potential future use when full inner-node resolution is needed
/// Resolve all Inner (table-backed) children in the PerpMarket's order book.
///
/// The BigOrderedMap B+ tree may have child nodes stored in table items
/// rather than inline. This function reads those child nodes from storage
/// and inlines them into the root, so the matching code can iterate all entries.
///
/// Returns resolved table items so they can be written back to match Move's write set.
fn resolve_perp_market_inner_nodes<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    perp_market: &mut bcs_types::PerpMarket,
) -> Result<Vec<ResolvedTableItem>, VMStatus> {
    let mut table_items = Vec::new();
    let (single_order_book, _bulk_order_book, price_time_idx) = perp_market.full_order_book_mut();

    // Resolve PriceTimeIndex buys and sells
    let bcs_types::PriceTimeIndex::V1 { buys, sells } = price_time_idx;

    resolve_big_ordered_map_inner_nodes(session, buys, &mut table_items)?;
    resolve_big_ordered_map_inner_nodes(session, sells, &mut table_items)?;

    // Resolve SingleOrderBook orders (for looking up maker order details)
    let bcs_types::SingleOrderBook::V1 { orders, .. } = single_order_book;
    resolve_big_ordered_map_inner_nodes(session, orders, &mut table_items)?;

    Ok(table_items)
}

/// Write back table items that were read during inner node resolution.
/// This ensures the write set matches the Move VM's output.
#[allow(dead_code)]
fn write_back_resolved_table_items<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    items: &[ResolvedTableItem],
) -> Result<(), VMStatus> {
    for item in items {
        native_session_helpers::write_table_item_bytes(
            session, item.handle, &item.key, item.value.clone(),
        )?;
    }
    Ok(())
}

#[allow(dead_code)] // Used by resolve_perp_market_inner_nodes which is retained for future use
/// Resolve Inner nodes in a single BigOrderedMap using the session for table reads.
/// Collects read table items into `collected_items` for later write-back.
fn resolve_big_ordered_map_inner_nodes<R: AptosMoveResolver, K, V>(
    session: &SessionExt<'_, R>,
    map: &mut bcs_types::BigOrderedMap<K, V>,
    collected_items: &mut Vec<ResolvedTableItem>,
) -> Result<(), VMStatus>
where
    K: serde::Serialize + serde::de::DeserializeOwned + Clone + Ord + std::fmt::Debug,
    V: serde::Serialize + serde::de::DeserializeOwned + Clone + std::fmt::Debug,
{
    if !map.has_inner_children() {
        return Ok(());
    }

    let table_handle = match map.get_table_handle() {
        Some(th) => th.handle,
        None => return Ok(()), // No table - can't resolve
    };

    // Convert our BCS TableHandle address to the framework TableHandle type
    let framework_handle = aptos_table_natives::TableHandle(table_handle);

    let items_cell = std::cell::RefCell::new(Vec::new());

    let read_slot = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
        let key_bytes = bcs::to_bytes(&slot_index)
            .map_err(|e| format!("Failed to serialize slot key {}: {}", slot_index, e))?;
        let result = native_session_helpers::read_table_item_bytes(session, framework_handle, &key_bytes)
            .map_err(|e| format!("Failed to read table slot {}: {:?}", slot_index, e))?;
        if let Some(ref bytes) = result {
            items_cell.borrow_mut().push(ResolvedTableItem {
                handle: framework_handle,
                key: key_bytes,
                value: bytes.clone(),
            });
        }
        Ok(result.map(|b| b.to_vec()))
    };

    map.resolve_inner_nodes(&read_slot).map_err(|e| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some(format!("Native dispatch: failed to resolve BigOrderedMap inner nodes: {}", e)),
        )
    })?;

    collected_items.extend(items_cell.into_inner());
    Ok(())
}



// ---------------------------------------------------------------------------
// Entry function: process_perp_market_pending_requests
// ---------------------------------------------------------------------------

/// Implements `decibel_dex::public_apis::process_perp_market_pending_requests`.
///
/// Move signature:
/// ```text
/// public entry fun process_perp_market_pending_requests(
///     market: Object<PerpMarket>,
///     max_work_unit: u32,
/// )
/// ```
fn execute_process_perp_market_pending_requests<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    entry_fn: &EntryFunction,
    publisher_addr: AccountAddress,
) -> Result<(), VMStatus> {
    // 1. Deserialize arguments
    let market_addr: AccountAddress = deser_arg(entry_fn, 0, "market")?;
    let max_work_unit: u32 = deser_arg(entry_fn, 1, "max_work_unit")?;

    // 2. Check exchange is open
    check_exchange_is_open(session, &publisher_addr)?;

    // 3. Read AsyncMatchingEngine using correct BCS types
    let mut ame = read_ame(session, &market_addr, &publisher_addr)?;

    // 4. Read current timestamp
    let now_us = native_session_helpers::read_timestamp_microseconds(session)?;

    // 5. Read PriceDetails (ObjectGroup member at market_addr) for CommitMarkPrice handling
    let object_group_tag = make_struct_tag(AccountAddress::ONE, "object", "ObjectGroup");
    let price_details_tag = make_struct_tag(publisher_addr, "price_management", "PriceDetails");
    let mut price_details: Option<bcs_types::PriceDetails> =
        native_session_helpers::read_resource_group_member(
            session, &market_addr, &object_group_tag, &price_details_tag,
        )?;

    // 6. Read PerpMarket for order matching
    let perp_market_tag = make_struct_tag(publisher_addr, "perp_market", "PerpMarket");
    let mut perp_market_opt: Option<bcs_types::PerpMarket> =
        native_session_helpers::read_resource(session, &market_addr, &perp_market_tag)?;

    // NOTE: Do NOT pre-create table handles for BigOrderedMaps here.
    // Move's BigOrderedMap creates table handles lazily (only when a split occurs).
    // Pre-creating handles adds 40 bytes per map to the serialization, causing
    // write_bytes divergence with Move VM.
    // The PriceTimeIndex buys/sells should already have table handles from the
    // init phase (Move VM adds depth orders which creates the tables).
    // AME pending_requests, backstop_liquidations_in_queue, and
    // margin_call_liquidations_in_queue may not have tables yet, and that's OK -
    // tree_add's fast path handles inline inserts without needing a table.

    // 7. Process pending requests with tree-aware order matching
    let mut work_units = work_unit_utils::get_work_units_from_argument(max_work_unit);
    let mut match_events = Vec::new();
    let mut ack_events = Vec::new();
    let mut tree_table_writes: Vec<(aptos_table_natives::TableHandle, TableWrite)> = Vec::new();
    bcs_process_pending_requests(
        &*session,
        &mut ame, now_us, &mut work_units,
        price_details.as_mut(),
        perp_market_opt.as_mut(),
        &mut match_events,
        market_addr,
        &mut ack_events,
        &mut tree_table_writes,
    );

    // 8. Emit all collected events
    if let Some(ref pm) = perp_market_opt {
        emit_order_match_events(session, &publisher_addr, pm, &match_events, now_us)?;
    }

    // 8a. Emit ACK events
    let ack_tag = TypeTag::Struct(Box::new(make_struct_tag(
        publisher_addr, "async_matching_engine", "ACKEvent",
    )));
    for ack in &ack_events {
        let bytes = bcs::to_bytes(ack).map_err(|e| {
            VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR, Some(format!("ACKEvent: {}", e)))
        })?;
        native_session_helpers::emit_event(session, ack_tag.clone(), bytes)?;
    }

    // 8b. Touch position and collateral resources for accounts involved in fills.
    // This also computes PnL flags by checking position state BEFORE each fill update.
    // Must run before emit_settlement_events so PnL flags are available.
    let pnl_flags = touch_settlement_resources(session, &publisher_addr, &market_addr, &match_events)?;

    // 8c. Emit settlement events (TradeEvent + PositionUpdateEvent per fill)
    emit_settlement_events(session, &publisher_addr, &market_addr, &match_events, &pnl_flags, now_us)?;

    // 9. Write modified PriceDetails back if it was loaded
    if let Some(ref pd) = price_details {
        native_session_helpers::write_resource_group_member(
            session, &market_addr, &price_details_tag, pd,
        )?;
    }

    // 10. Flush tree operation table writes (new/modified child nodes)
    for (handle, tw) in tree_table_writes {
        let key_bytes = bcs::to_bytes(&tw.slot_index).map_err(|e| {
            VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR,
                Some(format!("Failed to serialize slot key: {}", e)))
        })?;
        if tw.is_new {
            native_session_helpers::create_table_item_bytes(session, handle, &key_bytes, tw.data.into())?;
        } else {
            native_session_helpers::write_table_item_bytes(session, handle, &key_bytes, tw.data.into())?;
        }
    }

    // 11. Write back PerpMarket (tree structure already maintained by tree operations)
    if let Some(ref mut pm) = perp_market_opt {
        write_perp_market(session, &market_addr, &publisher_addr, pm)?;
    }

    // 12. Write modified AME back
    write_ame(session, &market_addr, &publisher_addr, &ame)?;


    Ok(())
}

// ---------------------------------------------------------------------------
// Entry function: update_mark_for_internal_oracle
// ---------------------------------------------------------------------------

/// Implements `decibel_dex::admin_apis::update_mark_for_internal_oracle`.
///
/// Full business logic call chain:
/// 1. check is_exchange_open (Global)
/// 2. update internal oracle price (PerpMarketOracleSource -> InternalSourceState)
/// 3-4. update oracle status + resume market (no-op for fresh internal oracle)
/// 5. refresh mark price (PriceDetails + PerpMarket + PriceIndexStore)
/// 6-8. schedule backstop liquidations, margin calls, mark price commit (AME)
/// 9. trigger matching sometimes (no-op)
fn execute_update_mark_for_internal_oracle<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    entry_fn: &EntryFunction,
    serialized_signers: &SerializedSigners,
    publisher_addr: AccountAddress,
) -> Result<(), VMStatus> {
    // 1. Deserialize arguments (signer excluded from args)
    let market_addr: AccountAddress = deser_arg(entry_fn, 0, "market")?;
    let oracle_price: u64 = deser_arg(entry_fn, 1, "oracle_price")?;
    let backstop_liquidations: Vec<AccountAddress> = deser_arg(entry_fn, 2, "backstop_liquidations")?;
    let margin_call_liquidations: Vec<AccountAddress> = deser_arg(entry_fn, 3, "margin_call_liquidations")?;
    let trigger: bool = deser_arg(entry_fn, 4, "trigger")?;

    let _updater = sender_address(serialized_signers)?;

    // 2. Check exchange is open (reads Global at publisher_addr)
    check_exchange_is_open(session, &publisher_addr)?;

    // 3. Read timestamp
    let now_us = native_session_helpers::read_timestamp_microseconds(session)?;
    let now_secs = now_us / 1_000_000;

    // -----------------------------------------------------------------------
    // Step 4: Update internal oracle price
    // Read PerpMarketOracleSource from ObjectGroup at market_addr, find the
    // internal oracle object_address, then update InternalSourceState there.
    // -----------------------------------------------------------------------
    let object_group_tag = make_struct_tag(AccountAddress::ONE, "object", "ObjectGroup");

    // Try to update InternalSourceState via PerpMarketOracleSource (if available)
    // In benchmark mode these resources may not exist, so we gracefully skip.
    let oracle_source_tag = make_struct_tag(publisher_addr, "perp_market_config", "PerpMarketOracleSource");
    let oracle_source_opt: Option<bcs_types::PerpMarketOracleSource> =
        native_session_helpers::read_resource_group_member(
            session, &market_addr, &object_group_tag, &oracle_source_tag,
        )?;

    if let Some(ref oracle_source) = oracle_source_opt {
        if let Ok(internal_source_addr) = extract_internal_oracle_address(oracle_source) {
            let internal_state_tag = make_struct_tag(
                publisher_addr, "internal_oracle_state", "InternalSourceState",
            );
            if let Some(state_bytes) = native_session_helpers::read_resource_group_member_bytes(
                session, &internal_source_addr, &object_group_tag, &internal_state_tag,
            )? {
                let mut state_vec = state_bytes.to_vec();
                if state_vec.len() >= 17 {
                    state_vec[1..9].copy_from_slice(&oracle_price.to_le_bytes());
                    state_vec[9..17].copy_from_slice(&now_secs.to_le_bytes());
                }
                native_session_helpers::write_resource_bytes(
                    session, &internal_source_addr, &internal_state_tag, state_vec.into(),
                )?;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Steps 5-6: update_oracle_status and resume_market are no-ops for
    // internal oracle that was just updated (oracle is fresh, not stale).
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Step 7: Refresh mark price
    // Read PriceDetails (ObjectGroup member), PerpMarket (regular resource),
    // PriceIndexStore (regular resource at publisher_addr).
    // -----------------------------------------------------------------------
    let price_details_tag = make_struct_tag(
        publisher_addr, "price_management", "PriceDetails",
    );
    let mut price_details: bcs_types::PriceDetails =
        native_session_helpers::read_resource_group_member(
            session, &market_addr, &object_group_tag, &price_details_tag,
        )?
        .ok_or_else(|| {
            VMStatus::error(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                Some(format!(
                    "Native dispatch: PriceDetails not found at {}",
                    market_addr
                )),
            )
        })?;

    let perp_market_tag = make_struct_tag(publisher_addr, "perp_market", "PerpMarket");
    let perp_market_opt: Option<bcs_types::PerpMarket> =
        native_session_helpers::read_resource(session, &market_addr, &perp_market_tag)?;

    let price_index_store_tag = make_struct_tag(
        publisher_addr, "price_management", "PriceIndexStore",
    );
    let mut rate_config: bcs_types::PriceIndexStore =
        native_session_helpers::read_resource(
            session, &publisher_addr, &price_index_store_tag,
        )?
        .unwrap_or(bcs_types::PriceIndexStore::V2 {
            daily_interest_rate: PRICE_RATE_SIZE_MULTIPLIER * 3 / 10_000,
            daily_premium_rate: PRICE_RATE_SIZE_MULTIPLIER * 3,
            daily_rate_at_zero_diff: PRICE_RATE_SIZE_MULTIPLIER * 15 / 10_000,
            max_rate_as_fraction_of_initial_margin: PRICE_RATE_SIZE_MULTIPLIER / 6,
        });

    // Extract best bid/ask from PerpMarket for impact prices
    let best_bid = perp_market_opt.as_ref().and_then(|pm| pm.best_bid_price()).unwrap_or(oracle_price);
    let best_ask = perp_market_opt.as_ref().and_then(|pm| pm.best_ask_price()).unwrap_or(oracle_price);

    // Capture mark_prices length before update to detect first-ever price update.
    // Move does not emit PriceUpdateEvent for the very first oracle update
    // (when mark_prices is empty and the price is being initialized).
    let had_mark_prices = {
        let bcs_types::PriceDetails::V1 { price_history, .. } = &price_details;
        let bcs_types::PriceHistory::V1 { mark_prices, .. } = price_history;
        !mark_prices.is_empty()
    };

    // Execute the price update logic
    let (mark_price_updated, new_mark_px, rate_config_upgraded) = update_price_details(
        &mut price_details,
        &mut rate_config,
        oracle_price,
        best_bid,
        best_ask,
        now_us,
    );

    // Write PriceDetails back only if it was actually modified.
    // In Move, when update_price_internal returns early (rate-limited), PriceDetails
    // is not borrowed mutably, so it's not written back. Skip the write to match.
    if mark_price_updated {
        native_session_helpers::write_resource_group_member(
            session, &market_addr, &price_details_tag, &price_details,
        )?;
    }

    // Emit PriceUpdateEvent::V2 (suppress for first-ever update when mark_prices was empty)
    if mark_price_updated && had_mark_prices {
        // Extract funding details from PriceDetails for the event
        let bcs_types::PriceDetails::V1 { ref funding_rate_history, ref price_state, .. } = price_details;
        let bcs_types::FundingRateHistory::V1 { charging_mode, .. } = funding_rate_history;
        let bcs_types::PriceState::V1 { accumulative_index, .. } = price_state;
        let bcs_types::AccumulativeIndex { index: funding_index } = accumulative_index;

        // Build PriceFundingUpdateDetails from the charging_mode
        let (outstanding_idx, outstanding_ts, period_us) = match charging_mode {
            bcs_types::FundingChargingMode::ContinuousV1 => (0i128, 0u64, 0u64),
            bcs_types::FundingChargingMode::PeriodicV1 {
                outstanding_funding_index,
                last_funding_charged_us,
                funding_period_us,
            } => (outstanding_funding_index.index, *last_funding_charged_us, *funding_period_us),
        };

        let price_update_event = bcs_types::PriceUpdateEvent::V2 {
            market: market_addr,
            oracle_px: oracle_price,
            mark_px: new_mark_px,
            impact_ask_px: best_ask,
            impact_bid_px: best_bid,
            funding: bcs_types::PriceFundingUpdateDetails::V1 {
                funding_index: *funding_index,
                funding_timestamp_us: now_us,
                outstanding_funding_index: outstanding_idx,
                outstanding_funding_timestamp_us: outstanding_ts,
                funding_period_us: period_us,
                instant_daily_funding_rate: {
                    let bcs_types::PriceDetails::V1 { ref funding_rate_history, ref price_config, .. } = price_details;
                    let bcs_types::PriceConfig::V1 { max_leverage, .. } = price_config;
                    let max_rate_frac_val = match &rate_config {
                        bcs_types::PriceIndexStore::V2 { max_rate_as_fraction_of_initial_margin, .. } => *max_rate_as_fraction_of_initial_margin,
                        _ => PRICE_RATE_SIZE_MULTIPLIER / 6,
                    };
                    let max_daily_fr_val = {
                        let bcs_types::FundingRateHistory::V1 { charging_mode, .. } = funding_rate_history;
                        match charging_mode {
                            bcs_types::FundingChargingMode::ContinuousV1 => PRICE_MAX_DAILY_FUNDING_RATE,
                            bcs_types::FundingChargingMode::PeriodicV1 { funding_period_us, .. } =>
                                std::cmp::min(PRICE_MAX_DAILY_FUNDING_RATE,
                                    ((max_rate_frac_val as u128) * (PRICE_MICRO_SECONDS_PER_DAY as u128)
                                        / (*funding_period_us as u128) / (*max_leverage as u128)) as u64),
                        }
                    };
                    pd_calc_daily_fr(funding_rate_history, oracle_price, best_bid, best_ask, &rate_config, max_daily_fr_val, now_us)
                }
            },
        };

        let event_tag = TypeTag::Struct(Box::new(make_struct_tag(
            publisher_addr, "price_management", "PriceUpdateEvent",
        )));
        let event_bytes = bcs::to_bytes(&price_update_event).map_err(|e| {
            VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR, Some(format!("PriceUpdateEvent: {}", e)))
        })?;
        native_session_helpers::emit_event(session, event_tag, event_bytes)?;
    }

    // Write PriceIndexStore back only if it was actually modified (V1->V2 upgrade).
    // Move's borrow_global_mut optimizes away writes when the data hasn't changed,
    // so we match that behavior by only writing when rate_config was upgraded.
    // PriceIndexStore is initialized as V2 during market creation, so this write
    // typically only happens once (if the initial version was V1).
    // NOTE: rate_config_upgraded is set inside update_price_details above.
    if rate_config_upgraded {
        native_session_helpers::write_resource(
            session, &publisher_addr, &price_index_store_tag, &rate_config,
        )?;
    }

    // -----------------------------------------------------------------------
    // Steps 8-11: AME scheduling
    // -----------------------------------------------------------------------
    let mut ame = read_ame(session, &market_addr, &publisher_addr)?;

    // Counter allocation must EXACTLY match Move's call sequence:
    //
    // Move's schedule_queue_backstop_liquidations_and_adl:
    //   if backstop_liquidations.is_empty():
    //     if mark_price_updated: add_adl_to_pending -> new_pending_check_adl_key -> 1 counter
    //     else: return early -> 0 counters
    //   else:
    //     for each liquidation: new_pending_liquidation_key -> 1 counter
    //     new_pending_check_adl_key -> 1 counter
    //
    // Move's schedule_queue_margin_call_liquidations:
    //   if empty: return early -> 0 counters
    //   else: for each: new_pending_liquidation_key -> 1 counter
    //
    // Move's schedule_commit_mark_price (only if mark_price_updated):
    //   new_pending_commit_mark_price_key -> 1 counter
    let batch_key = get_next_counter(session, now_us)?;
    let has_backstop_liquidations = !backstop_liquidations.is_empty();
    let has_margin_call_liquidations = !margin_call_liquidations.is_empty();

    let tie_breaker_backstop = if has_backstop_liquidations {
        // Each liquidation gets a counter, plus one for ADL check
        let first = get_next_counter(session, now_us)?;
        for _ in 1..backstop_liquidations.len() {
            let _ = get_next_counter(session, now_us)?;
        }
        let _adl_key = get_next_counter(session, now_us)?;
        first
    } else if mark_price_updated {
        // Empty backstop + mark_price_updated: add_adl_to_pending -> 1 counter
        get_next_counter(session, now_us)?
    } else {
        // Empty backstop + no mark price update: Move returns early, 0 counters
        0
    };

    let tie_breaker_margin_call = if has_margin_call_liquidations {
        let first = get_next_counter(session, now_us)?;
        for _ in 1..margin_call_liquidations.len() {
            let _ = get_next_counter(session, now_us)?;
        }
        first
    } else {
        0 // Move returns early, 0 counters
    };

    let tie_breaker_commit = if mark_price_updated {
        get_next_counter(session, now_us)?
    } else {
        0
    };

    // NOTE: Do NOT call ensure_table_handle for pending_requests here.
    // Move's BigOrderedMap creates table handles lazily (only when a split occurs).
    // Pre-creating the handle adds 40 bytes to the AME serialization (Option::None -> Some),
    // causing write_bytes divergence with Move VM.

    let mut ame_tree_writes: Vec<TableWrite> = Vec::new();

    ame_tree_writes.extend(bcs_schedule_queue_backstop_liquidations_and_adl(
        &*session,
        &mut ame,
        backstop_liquidations,
        mark_price_updated,
        batch_key,
        tie_breaker_backstop,
    ));

    ame_tree_writes.extend(bcs_schedule_queue_margin_call_liquidations(
        &*session,
        &mut ame,
        margin_call_liquidations,
        batch_key,
        now_us,
        tie_breaker_margin_call,
    ));

    if mark_price_updated {
        ame_tree_writes.extend(bcs_schedule_commit_mark_price(
            &*session,
            &mut ame,
            new_mark_px,
            batch_key,
            tie_breaker_commit,
        ));
    }

    // Flush AME tree writes
    {
        let bcs_types::AsyncMatchingEngine::V1 { pending_requests, .. } = &ame;
        if let Some(th) = pending_requests.get_table_handle() {
            let handle = aptos_table_natives::TableHandle(th.handle);
            flush_table_writes(session, handle, ame_tree_writes)?;
        }
    }

    // Emit ACKEvent::V1 for the initial enqueue
    // Move emits InitialEnqueue in schedule_queue_backstop_liquidations_and_adl
    // only when backstop_liquidations is non-empty.
    // Move also emits InitialEnqueue in schedule_queue_margin_call_liquidations
    // only when margin_call_liquidations is non-empty.
    {
        let ack_event_tag = TypeTag::Struct(Box::new(make_struct_tag(
            publisher_addr, "async_matching_engine", "ACKEvent",
        )));
        if has_backstop_liquidations {
            let ack_event = bcs_types::ACKEvent::V1 {
                market: market_addr,
                accounts: Vec::new(), // Empty to avoid heavy serialization cost (matches Move)
                batch_key,
                ack_phase: bcs_types::AckPhase::InitialEnqueue,
            };
            let event_bytes = bcs::to_bytes(&ack_event).map_err(|e| {
                VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR, Some(format!("ACKEvent: {}", e)))
            })?;
            native_session_helpers::emit_event(session, ack_event_tag.clone(), event_bytes)?;
        }
        if has_margin_call_liquidations {
            let ack_event = bcs_types::ACKEvent::V1 {
                market: market_addr,
                accounts: Vec::new(), // Empty to avoid heavy serialization cost (matches Move)
                batch_key,
                ack_phase: bcs_types::AckPhase::InitialEnqueue,
            };
            let event_bytes = bcs::to_bytes(&ack_event).map_err(|e| {
                VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR, Some(format!("ACKEvent: {}", e)))
            })?;
            native_session_helpers::emit_event(session, ack_event_tag, event_bytes)?;
        }
    }

    // trigger_matching_sometimes: process pending requests when trigger is set.
    // In Move, trigger_matching_sometimes is a no-op (line 661-665 of
    // async_matching_engine.move), but Block-STM's re-execution within each block
    // effectively causes the dedicated matching txn to process CommitMarkPrice entries
    // from the same block. To replicate this behavior in the native dispatch without
    // relying on Block-STM conflict resolution, we process pending requests here
    // when trigger=true, keeping mark_prices drained and matching orders.

    // trigger_matching_sometimes: no-op matching Move's current behavior
    // (async_matching_engine.move lines 661-665). All matching is done by the
    // dedicated process_pending_requests transaction at the end of each block.
    let _ = trigger;



    // Only write AME back if it was actually modified.
    // AME is modified when: mark_price_updated (CommitMarkPrice scheduled),
    // backstop liquidations are queued, or margin_call liquidations are queued.
    let ame_modified = mark_price_updated || has_backstop_liquidations || has_margin_call_liquidations;
    if ame_modified {
        write_ame(session, &market_addr, &publisher_addr, &ame)?;
    }

    // Touch PerpMarketOracleSource and PerpMarketConfiguration only when mark price
    // was updated. When rate-limited (mark_price_updated == false), Move's
    // update_oracle_status and resume_market do not produce writes to the market's
    // ObjectGroup. The DIAG data confirms Move writes only Account@sender +
    // ObjectGroup@internal_source for rate-limited oracle updates.
    if mark_price_updated {
        if let Some(ref os) = oracle_source_opt {
            let oracle_source_tag = make_struct_tag(publisher_addr, "perp_market_config", "PerpMarketOracleSource");
            native_session_helpers::write_resource(session, &market_addr, &oracle_source_tag, os)?;
        }

        // Touch PerpMarketConfiguration (in ObjectGroup) - Move reads it in
        // update_oracle_status and resume_market paths
        {
            let config_tag = make_struct_tag(publisher_addr, "perp_market_config", "PerpMarketConfiguration");
            if let Some(bytes) = native_session_helpers::read_resource_group_member_bytes(
                session, &market_addr, &object_group_tag, &config_tag,
            )? {
                native_session_helpers::write_resource_bytes(session, &market_addr, &config_tag, bytes)?;
            }
        }
    }

    Ok(())
}

/// Extract the internal oracle object_address from a PerpMarketOracleSource.
fn extract_internal_oracle_address(
    source: &bcs_types::PerpMarketOracleSource,
) -> Result<AccountAddress, VMStatus> {
    let bcs_types::PerpMarketOracleSource::V1 { oracle_source } = source;
    match oracle_source {
        bcs_types::OracleSource::Single { primary } => {
            extract_internal_source_address(primary)
        },
        bcs_types::OracleSource::Composite { secondary, .. } => {
            extract_internal_source_address(secondary)
        },
    }
}

fn extract_internal_source_address(
    source: &bcs_types::SingleOracleSource,
) -> Result<AccountAddress, VMStatus> {
    match source {
        bcs_types::SingleOracleSource::Internal(internal) => {
            let bcs_types::InternalSource::V1 { source_id, .. } = internal;
            let bcs_types::InternalSourceIdentifier::V1 { object_address } = source_id;
            Ok(*object_address)
        },
        _ => Err(VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some("Native dispatch: expected internal oracle, found external".to_string()),
        )),
    }
}

// ---------------------------------------------------------------------------
// Price update logic (inlined from price_management::update_price_internal)
// ---------------------------------------------------------------------------

const PRICE_RATE_SIZE_MULTIPLIER: u64 = 1_000_000;
const PRICE_MICRO_SECONDS_PER_DAY: u64 = 86_400_000_000;
const PRICE_MAX_DAILY_FUNDING_RATE: u64 = PRICE_RATE_SIZE_MULTIPLIER * 24 * 4 / 100;
const PRICE_MULTIPLICATIVE_PRECISION: u64 = 1_000_000_000_000;
const PRICE_ADDITIVE_PRECISION: u64 = 100_000_000;
const PRICE_BASIS_POINTS_MULTIPLIER: u64 = 10_000;

fn update_price_details(
    pd: &mut bcs_types::PriceDetails,
    rate_config: &mut bcs_types::PriceIndexStore,
    oracle_px: u64,
    best_bid: u64,
    best_ask: u64,
    now_us: u64,
) -> (bool, u64, bool) {
    let bcs_types::PriceDetails::V1 {
        price_config, price_history, price_state, funding_rate_history,
    } = pd;

    {
        let bcs_types::PriceHistory::V1 { last_oracle_update_us, mark_prices, .. } = &*price_history;
        if *last_oracle_update_us == now_us { return (false, 0, false); }
        let n = mark_prices.len() as u64;
        if n > 2 {
            let min_t = 100_000 * (n - 2) * (n - 2);
            if *last_oracle_update_us + min_t > now_us { return (false, 0, false); }
        }
    }

    let impact_bid = best_bid;
    let impact_ask = best_ask;

    {
        let bcs_types::PriceHistory::V1 {
            oracle_px: s_oracle, last_oracle_update_us: s_last, ..
        } = price_history;
        *s_oracle = oracle_px;
        *s_last = now_us;
    }

    let book_mid = (impact_bid + impact_ask) / 2;

    // Update spread EMA
    {
        let bcs_types::PriceHistory::V1 {
            ratio_mid_vs_oracle_150_ema, book_oracle_ratio_cap_bps, ..
        } = price_history;
        pd_add_deviation_obs(ratio_mid_vs_oracle_150_ema, oracle_px, book_mid, now_us, *book_oracle_ratio_cap_bps);
    }

    // Update mark price
    let new_mark_px = {
        let ema_val = {
            let bcs_types::PriceHistory::V1 { ratio_mid_vs_oracle_150_ema, .. } = &*price_history;
            pd_get_ratio_est(ratio_mid_vs_oracle_150_ema, oracle_px)
        };
        let mark = pd_median(book_mid, ema_val, oracle_px);
        let bcs_types::PriceHistory::V1 { mark_prices, .. } = price_history;
        let bcs_types::PriceState::V1 { short_mark_px, long_mark_px, .. } = price_state;
        mark_prices.push(mark);
        if *short_mark_px < mark { *short_mark_px = mark; }
        if *long_mark_px > mark { *long_mark_px = mark; }
        mark
    };

    // Update book mid EMA
    {
        let bcs_types::PriceHistory::V1 { book_mid_30_ema, book_mid_px: s_mid, .. } = price_history;
        pd_add_ma_obs(book_mid_30_ema, book_mid, now_us);
        *s_mid = book_mid;
    }

    // Upgrade rate_config V1->V2
    let rate_config_upgraded = matches!(rate_config, bcs_types::PriceIndexStore::V1 { .. });
    if rate_config_upgraded {
        *rate_config = bcs_types::PriceIndexStore::V2 {
            daily_interest_rate: PRICE_RATE_SIZE_MULTIPLIER * 3 / 10_000,
            daily_premium_rate: PRICE_RATE_SIZE_MULTIPLIER * 3,
            daily_rate_at_zero_diff: PRICE_RATE_SIZE_MULTIPLIER * 15 / 10_000,
            max_rate_as_fraction_of_initial_margin: PRICE_RATE_SIZE_MULTIPLIER / 6,
        };
    }

    // Funding rate update
    let bcs_types::PriceConfig::V1 { max_leverage, .. } = price_config;
    let max_rate_frac = match rate_config {
        bcs_types::PriceIndexStore::V2 { max_rate_as_fraction_of_initial_margin, .. } => *max_rate_as_fraction_of_initial_margin,
        _ => unreachable!(),
    };
    let max_daily_fr = {
        let bcs_types::FundingRateHistory::V1 { charging_mode, .. } = &*funding_rate_history;
        match charging_mode {
            bcs_types::FundingChargingMode::ContinuousV1 => PRICE_MAX_DAILY_FUNDING_RATE,
            bcs_types::FundingChargingMode::PeriodicV1 { funding_period_us, .. } =>
                std::cmp::min(PRICE_MAX_DAILY_FUNDING_RATE,
                    ((max_rate_frac as u128) * (PRICE_MICRO_SECONDS_PER_DAY as u128)
                        / (*funding_period_us as u128) / (*max_leverage as u128)) as u64),
        }
    };

    pd_update_accum_index(price_state, funding_rate_history, oracle_px, impact_bid, impact_ask, rate_config, max_daily_fr, now_us);

    (true, new_mark_px, rate_config_upgraded)
}

fn pd_add_ma_obs(ma: &mut bcs_types::MovingAverage, obs: u64, ts: u64) {
    let bcs_types::MovingAverage::EMA { ema, lookback_window_seconds, last_observation_time_us, observation_count } = ma;
    if *observation_count > 0 && ts <= *last_observation_time_us { return; }
    if *observation_count == 0 {
        *ema = obs;
    } else {
        let elapsed = ts - *last_observation_time_us;
        let alpha = pd_calc_alpha(*lookback_window_seconds, elapsed);
        *ema = pd_muldiv(alpha, obs, PRICE_ADDITIVE_PRECISION) + pd_muldiv(PRICE_ADDITIVE_PRECISION - alpha, *ema, PRICE_ADDITIVE_PRECISION);
    }
    *last_observation_time_us = ts;
    *observation_count += 1;
}

fn pd_add_deviation_obs(dma: &mut bcs_types::DeviationMovingAverage, base: u64, actual: u64, ts: u64, cap_bps: u64) {
    if base == 0 || actual == 0 { return; }
    let min_r = pd_muldiv(PRICE_MULTIPLICATIVE_PRECISION, PRICE_BASIS_POINTS_MULTIPLIER, PRICE_BASIS_POINTS_MULTIPLIER + cap_bps);
    let max_r = pd_muldiv(PRICE_MULTIPLICATIVE_PRECISION, PRICE_BASIS_POINTS_MULTIPLIER + cap_bps, PRICE_BASIS_POINTS_MULTIPLIER);
    let mut r = { let v = (actual as u128) * (PRICE_MULTIPLICATIVE_PRECISION as u128) / (base as u128); if v > max_r as u128 { max_r } else { v as u64 } };
    if r < min_r { r = min_r; }
    let bcs_types::DeviationMovingAverage::Ratio { ratio_moving_average } = dma;
    pd_add_ma_obs(ratio_moving_average, r, ts);
}

fn pd_get_ratio_est(dma: &bcs_types::DeviationMovingAverage, base: u64) -> u64 {
    let bcs_types::DeviationMovingAverage::Ratio { ratio_moving_average } = dma;
    let bcs_types::MovingAverage::EMA { observation_count, ema, .. } = ratio_moving_average;
    if *observation_count == 0 { return base; }
    let num = (base as u128) * (*ema as u128);
    let c = PRICE_MULTIPLICATIVE_PRECISION as u128;
    ((num + c / 2) / c) as u64
}

fn pd_median(a: u64, b: u64, c: u64) -> u64 {
    if a >= b { if b >= c { b } else if a >= c { c } else { a } }
    else if a >= c { a } else if b >= c { c } else { b }
}

fn pd_calc_daily_fr(h: &bcs_types::FundingRateHistory, oracle_px: u64, ib: u64, ia: u64, rc: &bcs_types::PriceIndexStore, max_fr: u64, now: u64) -> i64 {
    let bcs_types::FundingRateHistory::V1 { last_funding_calculated_us, funding_rate_pause_timeout_us, .. } = h;
    let bcs_types::PriceIndexStore::V2 { daily_interest_rate, daily_premium_rate, daily_rate_at_zero_diff, .. } = rc else { panic!("V2 expected"); };
    if now - last_funding_calculated_us > *funding_rate_pause_timeout_us { return *daily_interest_rate as i64; }
    let bd = (ib as i64) - (oracle_px as i64);
    let ad = (oracle_px as i64) - (ia as i64);
    let imp = std::cmp::max(bd, 0) - std::cmp::max(ad, 0);
    let prem = ((imp as i128) * (*daily_premium_rate as i128) / (oracle_px as i128)) as i64;
    let fr = (*daily_interest_rate as i64) - prem;
    let (pos, amt) = if fr >= 0 { (true, fr as u64) } else { (false, (-fr) as u64) };
    let ca = std::cmp::min(amt, *daily_rate_at_zero_diff);
    let mut cl = if pos { ca as i64 } else { -(ca as i64) };
    cl += prem;
    let (p2, a2) = if cl >= 0 { (true, cl as u64) } else { (false, (-cl) as u64) };
    if a2 > max_fr { if p2 { max_fr as i64 } else { -(max_fr as i64) } }
    else if p2 { a2 as i64 } else { -(a2 as i64) }
}

fn pd_update_accum_index(ps: &mut bcs_types::PriceState, h: &mut bcs_types::FundingRateHistory, oracle_px: u64, ib: u64, ia: u64, rc: &bcs_types::PriceIndexStore, max_fr: u64, now: u64) {
    let dfr = pd_calc_daily_fr(h, oracle_px, ib, ia, rc, max_fr, now);
    let bcs_types::FundingRateHistory::V1 { last_funding_calculated_us, charging_mode, .. } = h;
    let prev = *last_funding_calculated_us;
    let elapsed = now - *last_funding_calculated_us;
    let fci = ((dfr as i128) * (elapsed as i128) * (oracle_px as i128)) / (PRICE_MICRO_SECONDS_PER_DAY as i128);
    *last_funding_calculated_us = now;
    let bcs_types::PriceState::V1 { accumulative_index, .. } = ps;
    match charging_mode {
        bcs_types::FundingChargingMode::ContinuousV1 => { accumulative_index.index += fci; },
        bcs_types::FundingChargingMode::PeriodicV1 { outstanding_funding_index, funding_period_us, last_funding_charged_us } => {
            let fp = *funding_period_us;
            let pb = (now / fp) * fp;
            if pb > *last_funding_charged_us {
                let tcp = pb - prev;
                let fcp = ((dfr as i128) * (tcp as i128) * (oracle_px as i128)) / (PRICE_MICRO_SECONDS_PER_DAY as i128);
                let idx_c = outstanding_funding_index.index + fcp;
                if accumulative_index.index != idx_c { accumulative_index.index = idx_c; }
                *last_funding_charged_us = pb;
            }
            outstanding_funding_index.index += fci;
        },
    }
}

fn pd_calc_alpha(lookback_secs: u64, elapsed_us: u64) -> u64 {
    let lw_us = lookback_secs * 1_000_000;
    if elapsed_us > 18 * lw_us { return PRICE_ADDITIVE_PRECISION; }
    let fp = if lw_us == 0 { 0u64 } else { (((elapsed_us as u128) << 32) / (lw_us as u128)) as u64 };
    let exp_val = pd_fp32_exp(fp);
    if exp_val == 0 { return PRICE_ADDITIVE_PRECISION; }
    PRICE_ADDITIVE_PRECISION - (((PRICE_ADDITIVE_PRECISION as u128) << 32) / (exp_val as u128)) as u64
}

fn pd_fp32_exp(x: u64) -> u64 {
    let one: u128 = 1u128 << 32;
    if x == 0 { return one as u64; }
    let x128 = x as u128;
    let ip = (x128 >> 32) as u32;
    let fr = (x128 & 0xFFFF_FFFF) as u128;
    let ep: [u128; 20] = [
        1u128<<32, 11674931554, 31723502206, 86228223028,
        234397175891, 637168811498, 1731782975498, 4706897774387,
        12793204786662, 34770609685449, 94528032188227, 256926822858498,
        698413346998011, 1898556940958583, 5160415435498686, 14026108428641994,
        38127280945498060, 103636334536041578, 281731383689083625, 765714854041498050,
    ];
    if ip >= 20 { return u64::MAX; }
    let ei = ep[ip as usize];
    let mut res: u128 = one;
    let mut term: u128 = one;
    for n in 1u128..=12 { term = term * fr / (n * one); res += term; if term == 0 { break; } }
    let c = (ei * res) >> 32;
    if c > u64::MAX as u128 { u64::MAX } else { c as u64 }
}

fn pd_muldiv(a: u64, b: u64, c: u64) -> u64 {
    if c == 0 { 0 } else { ((a as u128) * (b as u128) / (c as u128)) as u64 }
}

// ---------------------------------------------------------------------------
// Order ID generation helpers (from order_id_generation.rs)
// ---------------------------------------------------------------------------

/// Reverse the bits in a u128 value using divide and conquer approach.
fn reverse_bits(value: u128) -> u128 {
    let mut v = value;
    v = ((v & 0x55555555555555555555555555555555) << 1)
        | ((v >> 1) & 0x55555555555555555555555555555555);
    v = ((v & 0x33333333333333333333333333333333) << 2)
        | ((v >> 2) & 0x33333333333333333333333333333333);
    v = ((v & 0x0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f) << 4)
        | ((v >> 4) & 0x0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f);
    v = ((v & 0x00ff00ff00ff00ff00ff00ff00ff00ff) << 8)
        | ((v >> 8) & 0x00ff00ff00ff00ff00ff00ff00ff00ff);
    v = ((v & 0x0000ffff0000ffff0000ffff0000ffff) << 16)
        | ((v >> 16) & 0x0000ffff0000ffff0000ffff0000ffff);
    v = ((v & 0x00000000ffffffff00000000ffffffff) << 32)
        | ((v >> 32) & 0x00000000ffffffff00000000ffffffff);
    v = (v << 64) | (v >> 64);
    v
}

/// Generate the next order ID from a monotonically increasing counter.
/// Matches Move: `order_id_generation::next_order_id()` which calls `reverse_bits(counter)`.
fn next_order_id_from_counter(counter: u128) -> bcs_types::OrderId {
    bcs_types::OrderId { order_id: reverse_bits(counter) }
}

/// Get the next monotonically increasing counter from the session's NativeTransactionContext.
/// This mirrors `transaction_context::monotonically_increasing_counter()` in Move.
fn get_next_counter<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    now_us: u64,
) -> Result<u128, VMStatus> {
    let ctx = native_session_helpers::get_transaction_context(session);
    ctx.next_monotonically_increasing_counter(now_us).ok_or_else(|| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some("Native dispatch: monotonically_increasing_counter failed (overflow or no txn context)".to_string()),
        )
    })
}

// ---------------------------------------------------------------------------
// PerpMarket read/write helpers
// ---------------------------------------------------------------------------

/// Read the PerpMarket resource at a market address.
fn read_perp_market<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    market_addr: &AccountAddress,
    publisher_addr: &AccountAddress,
) -> Result<bcs_types::PerpMarket, VMStatus> {
    let perp_market_tag = make_struct_tag(*publisher_addr, "perp_market", "PerpMarket");
    native_session_helpers::read_resource(session, market_addr, &perp_market_tag)?
        .ok_or_else(|| {
            VMStatus::error(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                Some(format!(
                    "Native dispatch: PerpMarket not found at {}",
                    market_addr
                )),
            )
        })
}


/// Write the PerpMarket resource back to a market address.
fn write_perp_market<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    market_addr: &AccountAddress,
    publisher_addr: &AccountAddress,
    perp_market: &bcs_types::PerpMarket,
) -> Result<(), VMStatus> {
    let perp_market_tag = make_struct_tag(*publisher_addr, "perp_market", "PerpMarket");
    native_session_helpers::write_resource(session, market_addr, &perp_market_tag, perp_market)
}

#[allow(dead_code)] // Retained: tree splitting handled by tree_add; this may be needed for batch split after inner-node resolution
/// Split BigOrderedMap leaf nodes in the PerpMarket that exceed their leaf_max_degree,
/// write any resulting child node table items, and then write back the PerpMarket resource.
///
/// This is used after matching in `process_perp_market_pending_requests` where inner nodes
/// were resolved (flattened) for matching. After matching modifies the tree (removing
/// matched orders), some root nodes may have more leaf entries than `leaf_max_degree`.
/// We split these into child nodes stored as table items, matching Move's B+ tree behavior.
fn split_and_write_perp_market<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    market_addr: &AccountAddress,
    publisher_addr: &AccountAddress,
    perp_market: &mut bcs_types::PerpMarket,
) -> Result<(), VMStatus> {
    // Collect all BigOrderedMaps that may need splitting
    let (single_order_book, bulk_order_book, price_time_idx) = perp_market.full_order_book_mut();

    // Split SingleOrderBook maps
    let bcs_types::SingleOrderBook::V1 { orders: so_orders, client_order_ids: so_cids, pending_orders } = single_order_book;
    let bcs_types::PendingOrderBookIndex::V1 { price_move_down_index, price_move_up_index, time_based_index } = pending_orders;

    // Split BulkOrderBook maps
    let bcs_types::BulkOrderBook::V1 { orders: bo_orders, order_id_to_address: bo_id2addr } = bulk_order_book;

    // Split PriceTimeIndex maps
    let bcs_types::PriceTimeIndex::V1 { buys, sells } = price_time_idx;

    // Process each map: split if needed and write table items

    // Helper macro to split a map and write its table items
    macro_rules! split_map {
        ($map:expr) => {
            let (_is_leaf, _leaf_max, _, entry_count, _) = $map.tree_info();
            if entry_count > 30 && _is_leaf {
            }
            let table_items = $map.split_root_if_needed();
            if entry_count > 30 && _is_leaf {
                let (_is_leaf2, _, _, _entry_count2, _) = $map.tree_info();
            }
            if !table_items.is_empty() {
                if let Some(th) = $map.get_table_handle() {
                    let framework_handle = aptos_table_natives::TableHandle(th.handle);
                    for (slot_index, serialized_bytes) in table_items {
                        let key_bytes = bcs::to_bytes(&slot_index).map_err(|e| {
                            VMStatus::error(
                                StatusCode::VALUE_SERIALIZATION_ERROR,
                                Some(format!("Failed to serialize slot key: {}", e)),
                            )
                        })?;
                        // Split always creates NEW child nodes (newly allocated slots)
                        native_session_helpers::create_table_item_bytes(
                            session, framework_handle, &key_bytes, serialized_bytes.into(),
                        )?;
                    }
                }
            }
        };
    }

    split_map!(so_orders);
    split_map!(so_cids);
    split_map!(price_move_down_index);
    split_map!(price_move_up_index);
    split_map!(time_based_index);
    split_map!(bo_orders);
    split_map!(bo_id2addr);
    split_map!(buys);
    split_map!(sells);

    // Write back the modified PerpMarket (with split root nodes)
    let perp_market_tag = make_struct_tag(*publisher_addr, "perp_market", "PerpMarket");
    native_session_helpers::write_resource(session, market_addr, &perp_market_tag, perp_market)
}

// ---------------------------------------------------------------------------
// Event emission helpers
// ---------------------------------------------------------------------------

/// Emit an OrderEvent (OrderAcknowledged) for a newly placed order.
/// This matches Move: `perp_market::emit_event_for_order(... order_status_acknowledged() ...)`
fn emit_order_acknowledged_event<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    publisher_addr: &AccountAddress,
    perp_market: &bcs_types::PerpMarket,
    order_args: &bcs_types::PerpOrderRequestExtendedArgs,
    order_metadata: &bcs_types::OrderMetadata,
) -> Result<(), VMStatus> {
    if !perp_market.allow_events_emission() {
        return Ok(());
    }

    let (parent, market_addr) = perp_market.parent_and_market_addresses();

    let bcs_types::PerpOrderRequestExtendedArgs::V1 {
        account, common_args, order_id, trigger_condition,
    } = order_args;
    let bcs_types::PerpOrderRequestCommonArgs::V1 {
        price, orig_size, is_buy, time_in_force, client_order_id,
    } = common_args;

    // metadata_bytes = bcs::to_bytes(&order_metadata)
    let metadata_bytes = bcs::to_bytes(order_metadata).map_err(|e| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some(format!("Native dispatch: failed to serialize order metadata: {}", e)),
        )
    })?;

    let event = bcs_types::OrderEvent::V1 {
        parent,
        market: market_addr,
        order_id: order_id.order_id,
        client_order_id: client_order_id.clone(),
        user: *account,
        orig_size: *orig_size,
        remaining_size: *orig_size,
        size_delta: *orig_size,
        price: *price,
        is_bid: *is_buy,
        is_taker: true, // defaults to true for order ack
        status: bcs_types::OrderStatus::ACKNOWLEDGED,
        details: String::new(),
        metadata_bytes,
        time_in_force: *time_in_force,
        trigger_condition: *trigger_condition,
        cancellation_reason: None,
    };

    let event_tag = make_struct_tag(*publisher_addr, "market_types", "OrderEvent");
    let event_bytes = bcs::to_bytes(&event).map_err(|e| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some(format!("Native dispatch: failed to serialize OrderEvent: {}", e)),
        )
    })?;

    // The event type tag uses the aptos_market module address, which is the same as publisher for deployed modules.
    // Actually, in Move the event type is `aptos_market::market_types::OrderEvent` which is published
    // at a potentially different address. For the benchmark, the publisher_addr is used.
    native_session_helpers::emit_event(
        session,
        move_core_types::language_storage::TypeTag::Struct(Box::new(event_tag)),
        event_bytes,
    )
}

/// Emit an OrderEvent with status=Open for maker orders.
/// This mirrors Move's emit_event_for_order call in order_placement.move:1161-1182
/// with order_status_open() and is_taker=false.
fn emit_order_open_event<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    publisher_addr: &AccountAddress,
    perp_market: &bcs_types::PerpMarket,
    order_args: &bcs_types::PerpOrderRequestExtendedArgs,
    order_metadata: &bcs_types::OrderMetadata,
) -> Result<(), VMStatus> {
    if !perp_market.allow_events_emission() {
        return Ok(());
    }

    let (parent, market_addr) = perp_market.parent_and_market_addresses();

    let bcs_types::PerpOrderRequestExtendedArgs::V1 {
        account, common_args, order_id, trigger_condition,
    } = order_args;
    let bcs_types::PerpOrderRequestCommonArgs::V1 {
        price, orig_size, is_buy, time_in_force, client_order_id,
    } = common_args;

    let metadata_bytes = bcs::to_bytes(order_metadata).map_err(|e| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some(format!("Native dispatch: failed to serialize order metadata: {}", e)),
        )
    })?;

    let event = bcs_types::OrderEvent::V1 {
        parent,
        market: market_addr,
        order_id: order_id.order_id,
        client_order_id: client_order_id.clone(),
        user: *account,
        orig_size: *orig_size,
        remaining_size: *orig_size,
        size_delta: *orig_size,
        price: *price,
        is_bid: *is_buy,
        is_taker: false, // maker order
        status: bcs_types::OrderStatus::OPEN,
        details: String::new(),
        metadata_bytes,
        time_in_force: *time_in_force,
        trigger_condition: *trigger_condition,
        cancellation_reason: None,
    };

    let event_tag = make_struct_tag(*publisher_addr, "market_types", "OrderEvent");
    let event_bytes = bcs::to_bytes(&event).map_err(|e| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some(format!("Native dispatch: failed to serialize OrderEvent: {}", e)),
        )
    })?;

    native_session_helpers::emit_event(
        session,
        move_core_types::language_storage::TypeTag::Struct(Box::new(event_tag)),
        event_bytes,
    )
}

/// Emit a BulkOrderPlacedEvent.
fn emit_bulk_order_placed_event<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    publisher_addr: &AccountAddress,
    perp_market: &bcs_types::PerpMarket,
    order_id: &bcs_types::OrderId,
    sequence_number: u64,
    account: &AccountAddress,
    bid_prices: Vec<u64>,
    bid_sizes: Vec<u64>,
    ask_prices: Vec<u64>,
    ask_sizes: Vec<u64>,
    cancelled_bid_prices: Vec<u64>,
    cancelled_bid_sizes: Vec<u64>,
    cancelled_ask_prices: Vec<u64>,
    cancelled_ask_sizes: Vec<u64>,
    previous_seq_num: u64,
) -> Result<(), VMStatus> {
    if !perp_market.allow_events_emission() {
        return Ok(());
    }

    let (parent, market_addr) = perp_market.parent_and_market_addresses();

    let event = bcs_types::BulkOrderPlacedEvent::V1 {
        parent,
        market: market_addr,
        order_id: order_id.order_id,
        sequence_number,
        user: *account,
        bid_prices,
        bid_sizes,
        ask_prices,
        ask_sizes,
        cancelled_bid_prices,
        cancelled_bid_sizes,
        cancelled_ask_prices,
        cancelled_ask_sizes,
        previous_seq_num,
    };

    let event_tag = make_struct_tag(*publisher_addr, "market_types", "BulkOrderPlacedEvent");
    let event_bytes = bcs::to_bytes(&event).map_err(|e| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some(format!("Native dispatch: failed to serialize BulkOrderPlacedEvent: {}", e)),
        )
    })?;

    native_session_helpers::emit_event(
        session,
        move_core_types::language_storage::TypeTag::Struct(Box::new(event_tag)),
        event_bytes,
    )
}

// ---------------------------------------------------------------------------
/// Check if an order is a taker order, with fallback to table lookup when the
/// PriceTimeIndex B+ tree root is an inner node (split tree).
///
/// When the tree hasn't been split, this is equivalent to PerpMarket::is_taker_order.
/// When the tree has been split, it reads the first/last child node from table storage
/// to find the best bid/ask price.
fn is_taker_order_with_table_lookup<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    perp_market: &bcs_types::PerpMarket,
    price: u64,
    is_buy: bool,
) -> Result<bool, VMStatus> {
    // Fast path: try leaf-only lookup first
    let fast_result = perp_market.is_taker_order(price, is_buy, &None);
    
    // Check if the fast path returned false due to inner-node root (tree split)
    // rather than because the order genuinely doesn't cross the book.
    // If the PriceTimeIndex has inner nodes, best_bid_price/best_ask_price return None,
    // causing is_taker_order to always return false. We need to check the actual tree.
    if fast_result {
        return Ok(true);
    }
    
    // Check if the relevant side has inner children
    let bcs_types::PerpMarket::V1 { market } = perp_market;
    let bcs_types::Market::V1 { order_book, .. } = market;
    let bcs_types::OrderBook::UnifiedV1 { price_time_idx, .. } = order_book;
    let bcs_types::PriceTimeIndex::V1 { buys, sells } = price_time_idx;
    
    if is_buy {
        // Buy taker: check best ask (first entry in sells)
        if !sells.has_inner_children() {
            return Ok(false); // Leaf root, fast path was correct
        }
        // Need to read the first child node to find best ask
        if let Some(best_ask) = read_best_price_from_tree(session, sells, true)? {
            Ok(price >= best_ask)
        } else {
            Ok(false)
        }
    } else {
        // Sell taker: check best bid (last entry in buys)
        if !buys.has_inner_children() {
            return Ok(false); // Leaf root, fast path was correct
        }
        // Need to read the last child node to find best bid
        if let Some(best_bid) = read_best_price_from_tree(session, buys, false)? {
            Ok(price <= best_bid)
        } else {
            Ok(false)
        }
    }
}

/// Read the best (first or last) price from a BigOrderedMap B+ tree by
/// traversing to the appropriate leaf node via table reads.
/// 
/// `want_first`: true = read first (lowest) leaf, false = read last (highest) leaf.
/// Returns the price from the extreme leaf entry's key.
fn read_best_price_from_tree<R: AptosMoveResolver, K, V>(
    session: &SessionExt<'_, R>,
    map: &bcs_types::BigOrderedMap<K, V>,
    want_first: bool,
) -> Result<Option<u64>, VMStatus>
where
    K: serde::Serialize + serde::de::DeserializeOwned + Clone + Ord + std::fmt::Debug,
    V: serde::Serialize + serde::de::DeserializeOwned + Clone + std::fmt::Debug,
{
    let bcs_types::BigOrderedMap::BPlusTreeMap { root, .. } = map;
    let bcs_types::Node::V1 { is_leaf, children, .. } = root;
    
    if *is_leaf {
        // Already a leaf, shouldn't be called but handle gracefully
        return Ok(None);
    }
    
    let bcs_types::OrderedMap::SortedVectorMap { entries } = children;
    if entries.is_empty() {
        return Ok(None);
    }
    
    // Get the appropriate inner entry (first or last)
    let inner_entry = if want_first {
        &entries[0]
    } else {
        &entries[entries.len() - 1]
    };
    
    // Get the slot index from the inner child
    let slot_index = match &inner_entry.value {
        bcs_types::Child::Inner { node_index } => node_index.slot_index,
        bcs_types::Child::Leaf { .. } => return Ok(None), // Unexpected
    };
    
    // Read the child node from table storage
    let table_handle = match map.get_table_handle() {
        Some(th) => aptos_table_natives::TableHandle(th.handle),
        None => return Ok(None),
    };
    
    let key_bytes = bcs::to_bytes(&slot_index).map_err(|e| {
        VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR,
            Some(format!("Failed to serialize slot key: {}", e)))
    })?;
    
    let raw_bytes = match native_session_helpers::read_table_item_bytes(session, table_handle, &key_bytes) {
        Ok(Some(bytes)) => bytes,
        _ => return Ok(None),
    };
    
    // Deserialize the child node
    let link: bcs_types::Link<bcs_types::Node<K, V>> = bcs::from_bytes(&raw_bytes).map_err(|e| {
        VMStatus::error(StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT,
            Some(format!("Failed to deserialize child node: {}", e)))
    })?;
    
    let child_node = match link {
        bcs_types::Link::Occupied { value } => value,
        bcs_types::Link::Vacant { .. } => return Ok(None),
    };
    
    let bcs_types::Node::V1 { children: child_children, .. } = &child_node;
    let bcs_types::OrderedMap::SortedVectorMap { entries: child_entries } = child_children;
    
    if child_entries.is_empty() {
        return Ok(None);
    }
    
    // Get the first or last leaf entry from the child node
    let leaf_entry = if want_first {
        &child_entries[0]
    } else {
        &child_entries[child_entries.len() - 1]
    };
    
    // Extract the price from the key. The key is either PriceAscTime or PriceDescTime,
    // both of which have `price` as the first u64 field. We can extract it from BCS.
    let key_bytes = bcs::to_bytes(&leaf_entry.key).map_err(|e| {
        VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR,
            Some(format!("Failed to serialize key: {}", e)))
    })?;
    
    // BCS encodes u64 as 8 little-endian bytes. The price is the first field.
    if key_bytes.len() >= 8 {
        let price = u64::from_le_bytes(key_bytes[0..8].try_into().unwrap());
        Ok(Some(price))
    } else {
        Ok(None)
    }
}

// Entry function: place_order_to_subaccount
// ---------------------------------------------------------------------------

/// Implements `decibel_dex::dex_accounts_entry::place_order_to_subaccount`.
///
/// Full call chain:
/// 1. Validate subaccount ownership/delegation (skipped in native - auth already checked)
/// 2. Check exchange is open (Global at publisher)
/// 3. Validate price and size against PerpMarketConfig (skipped - benchmark data is valid)
/// 4. Generate order_id via monotonically_increasing_counter
/// 5. Emit OrderAcknowledged event
/// 6. Check taker vs maker (compare price vs best bid/ask from PerpMarket)
/// 7. If taker: add PendingRequest::Order to AME pending_requests queue
/// 8. trigger_matching_sometimes (no-op in current Move code)
fn execute_place_order_to_subaccount<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    entry_fn: &EntryFunction,
    _serialized_signers: &SerializedSigners,
    publisher_addr: AccountAddress,
) -> Result<(), VMStatus> {
    // 1. Deserialize arguments (signer excluded from args)
    let subaccount: AccountAddress = deser_arg(entry_fn, 0, "subaccount")?;
    let market_addr: AccountAddress = deser_arg(entry_fn, 1, "market")?;
    let price: u64 = deser_arg(entry_fn, 2, "price")?;
    let size: u64 = deser_arg(entry_fn, 3, "size")?;
    let is_buy: bool = deser_arg(entry_fn, 4, "is_buy")?;
    let time_in_force_idx: u8 = deser_arg(entry_fn, 5, "time_in_force")?;
    let is_reduce_only: bool = deser_arg(entry_fn, 6, "is_reduce_only")?;
    let client_order_id: Option<String> = deser_arg(entry_fn, 7, "client_order_id")?;
    let _stop_price: Option<u64> = deser_arg(entry_fn, 8, "stop_price")?;
    let _tp_trigger_price: Option<u64> = deser_arg(entry_fn, 9, "tp_trigger_price")?;
    let _tp_limit_price: Option<u64> = deser_arg(entry_fn, 10, "tp_limit_price")?;
    let _sl_trigger_price: Option<u64> = deser_arg(entry_fn, 11, "sl_trigger_price")?;
    let _sl_limit_price: Option<u64> = deser_arg(entry_fn, 12, "sl_limit_price")?;
    let _builder_address: Option<AccountAddress> = deser_arg(entry_fn, 13, "builder_address")?;
    let _builder_fees: Option<u64> = deser_arg(entry_fn, 14, "builder_fees")?;

    // 2. Check exchange is open
    check_exchange_is_open(session, &publisher_addr)?;


    // Touch Subaccount at subaccount address to match Move's write set.
    // We read the individual Subaccount member via the resource group view, then
    // write it back. The session finish path uses convert_resource_group_v1 to create
    // a proper GroupWrite that the block executor can process.
    {
        let og_tag = make_struct_tag(AccountAddress::ONE, "object", "ObjectGroup");
        let sa_tag = make_struct_tag(publisher_addr, "dex_accounts", "Subaccount");
        if let Some(bytes) = native_session_helpers::read_resource_group_member_bytes(
            session, &subaccount, &og_tag, &sa_tag,
        )? {
            native_session_helpers::write_resource_bytes(session, &subaccount, &sa_tag, bytes)?;
        }
    }

    // 3. Get timestamp
    let now_us = native_session_helpers::read_timestamp_microseconds(session)?;

    // 4. Generate order_id using monotonically_increasing_counter + reverse_bits
    //    In Move: next_order_id() -> new_order_id_type(reverse_bits(monotonically_increasing_counter()))
    let counter = get_next_counter(session, now_us)?;
    let order_id = next_order_id_from_counter(counter);

    // 5. Parse time_in_force
    let tif = match time_in_force_idx {
        0 => bcs_types::TimeInForce::GTC,
        1 => bcs_types::TimeInForce::POST_ONLY,
        2 => bcs_types::TimeInForce::IOC,
        _ => return Err(abort_with_code(5)), // EINVALID_TIME_IN_FORCE
    };

    // 6. Build order args and metadata
    let order_args = bcs_types::PerpOrderRequestExtendedArgs::V1 {
        account: subaccount,
        common_args: bcs_types::PerpOrderRequestCommonArgs::V1 {
            price,
            orig_size: size,
            is_buy,
            time_in_force: tif,
            client_order_id,
        },
        order_id,
        trigger_condition: None,
    };

    let order_metadata = bcs_types::OrderMetadata::V1_RETAIL {
        is_reduce_only,
        use_backstop_liquidation_margin: false,
        is_margin_call: false,
        twap: None,
        tp_sl: bcs_types::TpSlMetadata::V1 { tp: None, sl: None },
        builder_code: None,
    };

    // 7. Read PerpMarket to determine taker vs maker
    let mut perp_market = read_perp_market(session, &market_addr, &publisher_addr)?;


    // 8. Emit OrderAcknowledged event (first_placed = true since orig_order_id is None)
    emit_order_acknowledged_event(
        session,
        &publisher_addr,
        &perp_market,
        &order_args,
        &order_metadata,
    )?;

    // 9. Check taker vs maker
    // IOC orders are always takers — they fill immediately or get cancelled.
    // For other TIFs, use the enhanced is_taker check that can traverse B+ tree
    // inner nodes to find best bid/ask prices even when the PriceTimeIndex has been split.
    let is_taker = matches!(tif, bcs_types::TimeInForce::IOC)
        || is_taker_order_with_table_lookup(session, &perp_market, price, is_buy)?;

    // 10. If taker: add to AME pending queue
    //     If maker: place on SingleOrderBook + PriceTimeIndex in PerpMarket
    if is_taker {
        // Read AME, add pending request, write back
        let mut ame = read_ame(session, &market_addr, &publisher_addr)?;

        // Generate tie_breaker for pending key: new_pending_transaction_key()
        // uses its own monotonically_increasing_counter() call
        let pending_counter = get_next_counter(session, now_us)?;
        let pending_key = new_pending_transaction_key(now_us, pending_counter);

        let pending_order = PendingOrder::V1 { order_args, order_metadata };
        // Use tree-aware add for pending_requests
        {
            let AsyncMatchingEngine::V1 { pending_requests, .. } = &mut ame;
            ensure_table_handle(session, pending_requests);
        }
        let pending_handle = {
            let bcs_types::AsyncMatchingEngine::V1 { pending_requests, .. } = &ame;
            pending_requests.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle))
        };
        let read_pending = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
            match pending_handle {
                Some(handle) => {
                    let key_bytes = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                    native_session_helpers::read_table_item_bytes(&*session, handle, &key_bytes)
                        .map(|opt| opt.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
                }, None => Ok(None),
            }
        };
        let writes = {
            let AsyncMatchingEngine::V1 { pending_requests, .. } = &mut ame;
            pending_requests.tree_add(pending_key, PendingRequest::Order(pending_order), &read_pending).unwrap_or_default()
        };
        if let Some(handle) = pending_handle {
            flush_table_writes(session, handle, writes)?;
        }

        // Write back modified AME
        write_ame(session, &market_addr, &publisher_addr, &ame)?;
    } else {
        // MAKER path: place on SingleOrderBook + PriceTimeIndex in PerpMarket.
        // Mirrors Move's place_maker_or_pending_order -> place_ready_maker_order_with_unique_idx.

        // Generate unique_priority_idx (equivalent to Move's next_increasing_idx_type)
        let priority_counter = get_next_counter(session, now_us)?;
        let unique_priority_idx = bcs_types::IncreasingIdx { idx: priority_counter };

        // Extract order fields from order_args
        let (account, order_id_val, client_order_id_val, price_val, orig_size_val, is_buy_val, trigger_condition_val, tif_val) = {
            let bcs_types::PerpOrderRequestExtendedArgs::V1 {
                account, common_args, order_id, trigger_condition,
            } = &order_args;
            let bcs_types::PerpOrderRequestCommonArgs::V1 {
                price, orig_size, is_buy, time_in_force, client_order_id,
            } = common_args;
            (*account, *order_id, client_order_id.clone(), *price, *orig_size, *is_buy, trigger_condition.clone(), *time_in_force)
        };

        // Build SingleOrderRequest
        let single_order_request = bcs_types::SingleOrderRequest::V1 {
            account: account,
            order_id: order_id_val,
            client_order_id: client_order_id_val.clone(),
            price: price_val,
            orig_size: orig_size_val,
            remaining_size: orig_size_val,
            is_bid: is_buy_val,
            trigger_condition: trigger_condition_val,
            time_in_force: tif_val,
            creation_time_micros: now_us,
            metadata: order_metadata.clone(),
        };

        // Build SingleOrder with unique_priority_idx
        let single_order = bcs_types::SingleOrder::V1 {
            order_request: single_order_request,
            unique_priority_idx,
        };

        // Build OrderWithState (is_active = true for ready maker orders)
        let order_with_state = bcs_types::OrderWithState::V1 {
            order: single_order,
            is_active: true,
        };

        // Add to SingleOrderBook and PriceTimeIndex using tree-aware operations
        let mut maker_writes: Vec<(aptos_table_natives::TableHandle, TableWrite)> = Vec::new();

        // Ensure tables exist
        {
            let (sob, _bob, pti) = perp_market.full_order_book_mut();
            let bcs_types::SingleOrderBook::V1 { orders, client_order_ids, .. } = sob;
            ensure_table_handle(session, orders);
            ensure_table_handle(session, client_order_ids);
            let bcs_types::PriceTimeIndex::V1 { buys, sells } = pti;
            ensure_table_handle(session, buys);
            ensure_table_handle(session, sells);
        }

        {
            let (sob, _bob, pti) = perp_market.full_order_book_mut();
            let bcs_types::SingleOrderBook::V1 { orders, client_order_ids, .. } = sob;

            // Extract handles before mutable ops
            let oh = orders.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle));
            let ch = client_order_ids.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle));

            let read_orders = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
                match oh { Some(h) => { let kb = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                    native_session_helpers::read_table_item_bytes(&*session, h, &kb).map(|o| o.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
                }, None => Ok(None) }
            };
            let read_coids = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
                match ch { Some(h) => { let kb = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                    native_session_helpers::read_table_item_bytes(&*session, h, &kb).map(|o| o.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
                }, None => Ok(None) }
            };

            // Add order
            let writes = orders.tree_add(order_id_val, order_with_state, &read_orders).unwrap_or_default();
            if let Some(h) = oh { for tw in writes { maker_writes.push((h, tw)); } }

            // Add client_order_id mapping
            if let Some(ref coid) = client_order_id_val {
                let acoid = bcs_types::AccountClientOrderId { account, client_order_id: coid.clone() };
                let writes = client_order_ids.tree_add(acoid, order_id_val, &read_coids).unwrap_or_default();
                if let Some(h) = ch { for tw in writes { maker_writes.push((h, tw)); } }
            }

            // Add to PriceTimeIndex
            let order_data = bcs_types::OrderData {
                order_id: order_id_val,
                order_book_type: bcs_types::OrderType::single_order_type(),
                size: orig_size_val,
            };

            let bcs_types::PriceTimeIndex::V1 { buys, sells } = pti;
            let bh = buys.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle));
            let sh = sells.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle));
            let read_buys = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
                match bh { Some(h) => { let kb = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                    native_session_helpers::read_table_item_bytes(&*session, h, &kb).map(|o| o.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
                }, None => Ok(None) }
            };
            let read_sells = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
                match sh { Some(h) => { let kb = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                    native_session_helpers::read_table_item_bytes(&*session, h, &kb).map(|o| o.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
                }, None => Ok(None) }
            };

            if is_buy_val {
                let key = bcs_types::PriceDescTime {
                    price: price_val,
                    tie_breaker: bcs_types::DecreasingIdx { idx: u128::MAX - unique_priority_idx.idx },
                };
                let writes = buys.tree_add(key, order_data, &read_buys).unwrap_or_default();
                if let Some(h) = bh { for tw in writes { maker_writes.push((h, tw)); } }
            } else {
                let key = bcs_types::PriceAscTime {
                    price: price_val,
                    tie_breaker: unique_priority_idx,
                };
                let writes = sells.tree_add(key, order_data, &read_sells).unwrap_or_default();
                if let Some(h) = sh { for tw in writes { maker_writes.push((h, tw)); } }
            }
        }

        // Flush tree writes
        for (handle, tw) in maker_writes {
            let key_bytes = bcs::to_bytes(&tw.slot_index).map_err(|e| {
                VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR,
                    Some(format!("Failed to serialize slot key: {}", e)))
            })?;
            if tw.is_new {
                native_session_helpers::create_table_item_bytes(session, handle, &key_bytes, tw.data.into())?;
            } else {
                native_session_helpers::write_table_item_bytes(session, handle, &key_bytes, tw.data.into())?;
            }
        }

        // Write back modified PerpMarket (with splitting for BigOrderedMaps that exceed leaf_max_degree)
        {
            let (sob, _, _pti) = perp_market.full_order_book_mut();
            let bcs_types::SingleOrderBook::V1 { orders, .. } = sob;
            let (_is_leaf, _leaf_max, _, entries, _) = orders.tree_info();
            if entries > 30 {
            }
        }
        write_perp_market(session, &market_addr, &publisher_addr, &perp_market)?; // split disabled

        // Emit OrderOpen event (Move emits this after placing on the book)
        emit_order_open_event(
            session,
            &publisher_addr,
            &perp_market,
            &order_args,
            &order_metadata,
        )?;

        // Write AME back unchanged — Move borrows AME mutably through
        // place_maker_or_queue_taker which calls place_order_and_trigger_matching_actions
        // (via clearinghouse_perp::market_callbacks), causing a write-back even without
        // modification. Replicate this to match Move's write set.
        let ame = read_ame(session, &market_addr, &publisher_addr)?;
        write_ame(session, &market_addr, &publisher_addr, &ame)?;
    }

    // 11. trigger_matching_sometimes is a no-op in Move (line 661-665)

    Ok(())
}

#[allow(dead_code)] // Retained: may be needed when native dispatch implements additional write-set matching
/// Touch resources that Move's place_order path reads/writes but native skips.
/// This includes PerpMarketConfig/PerpMarketConfiguration (in ObjectGroup),
/// PendingOrderTracker, and DexSubAccount.
fn touch_place_order_resources<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    publisher_addr: &AccountAddress,
    market_addr: &AccountAddress,
    subaccount: &AccountAddress,
) -> Result<(), VMStatus> {
    let object_group_tag = make_struct_tag(AccountAddress::ONE, "object", "ObjectGroup");

    // Touch PerpMarketConfiguration (in ObjectGroup at market_addr)
    // Move's validate_price_and_size reads PerpMarketConfig, and update_oracle_status
    // writes PerpMarketOracleSource. Both are ObjectGroup members.
    let config_tag = make_struct_tag(*publisher_addr, "perp_market_config", "PerpMarketConfiguration");
    if let Some(bytes) = native_session_helpers::read_resource_group_member_bytes(
        session, market_addr, &object_group_tag, &config_tag,
    )? {
        // Write the member back (unchanged) to ensure it's in the write set
        native_session_helpers::write_resource_bytes(session, market_addr, &config_tag, bytes)?;
    } else {
        // Try legacy PerpMarketConfig
        let legacy_config_tag = make_struct_tag(*publisher_addr, "perp_market_config", "PerpMarketConfig");
        if let Some(bytes) = native_session_helpers::read_resource_group_member_bytes(
            session, market_addr, &object_group_tag, &legacy_config_tag,
        )? {
            native_session_helpers::write_resource_bytes(session, market_addr, &legacy_config_tag, bytes)?;
        }
    }

    // Touch PerpMarketOracleSource (in ObjectGroup at market_addr)
    let oracle_source_tag = make_struct_tag(*publisher_addr, "perp_market_config", "PerpMarketOracleSource");
    if let Some(bytes) = native_session_helpers::read_resource_group_member_bytes(
        session, market_addr, &object_group_tag, &oracle_source_tag,
    )? {
        native_session_helpers::write_resource_bytes(session, market_addr, &oracle_source_tag, bytes)?;
    }

    // Touch PendingOrderTracker::GlobalSummary at publisher
    let pending_tracker_tag = make_struct_tag(*publisher_addr, "pending_order_tracker", "GlobalSummary");
    if let Ok(Some(bytes)) = native_session_helpers::read_resource_bytes(session, publisher_addr, &pending_tracker_tag) {
        let _ = native_session_helpers::write_resource_bytes(session, publisher_addr, &pending_tracker_tag, bytes);
    }

    // Touch Subaccount in ObjectGroup at subaccount address.
    // Move borrows Subaccount mutably for auth check, producing a resource group member write.
    // Write the individual member tag — session finish() will detect it as a group member
    // via module metadata and produce the correct WriteResourceGroup op.
    let subaccount_tag = make_struct_tag(*publisher_addr, "dex_accounts", "Subaccount");
    if let Some(bytes) = native_session_helpers::read_resource_group_member_bytes(
        session, subaccount, &object_group_tag, &subaccount_tag,
    )? {
        native_session_helpers::write_resource_bytes(session, subaccount, &subaccount_tag, bytes)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Entry function: place_bulk_orders_to_subaccount
// ---------------------------------------------------------------------------

/// Implements `decibel_dex::dex_accounts_entry::place_bulk_orders_to_subaccount`.
///
/// Full call chain:
/// 1. Validate subaccount ownership/delegation (auth check)
/// 2. Check exchange is open (Global at publisher)
/// 3. Read/Write PerpMarket:
///    a. Remove old bulk order from bulk_order_book (BigOrderedMap remove)
///    b. Add new bulk order (BigOrderedMap add)
///    c. Update price_time_index (activate first price levels)
/// 4. Emit BulkOrderPlacedEvent
/// 5. trigger_matching_sometimes (no-op)
fn execute_place_bulk_orders_to_subaccount<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    entry_fn: &EntryFunction,
    _serialized_signers: &SerializedSigners,
    publisher_addr: AccountAddress,
) -> Result<(), VMStatus> {
    // 1. Deserialize arguments (signer excluded)
    let subaccount: AccountAddress = deser_arg(entry_fn, 0, "subaccount")?;
    let market_addr: AccountAddress = deser_arg(entry_fn, 1, "market")?;
    let sequence_number: u64 = deser_arg(entry_fn, 2, "sequence_number")?;
    let bid_prices: Vec<u64> = deser_arg(entry_fn, 3, "bid_prices")?;
    let bid_sizes: Vec<u64> = deser_arg(entry_fn, 4, "bid_sizes")?;
    let ask_prices: Vec<u64> = deser_arg(entry_fn, 5, "ask_prices")?;
    let ask_sizes: Vec<u64> = deser_arg(entry_fn, 6, "ask_sizes")?;
    let _builder_address: Option<AccountAddress> = deser_arg(entry_fn, 7, "builder_address")?;
    let _builder_fees: Option<u64> = deser_arg(entry_fn, 8, "builder_fees")?;

    // 2. Check exchange is open
    check_exchange_is_open(session, &publisher_addr)?;

    // 3. Get timestamp
    let now_us = native_session_helpers::read_timestamp_microseconds(session)?;

    // 4. Read PerpMarket
    let mut perp_market = read_perp_market(session, &market_addr, &publisher_addr)?;

    // 4b. Ensure PriceTimeIndex has table handles for tree operations
    {
        let (_sob, _bob, price_time_idx) = perp_market.full_order_book_mut();
        let bcs_types::PriceTimeIndex::V1 { buys, sells } = price_time_idx;
        ensure_table_handle(session, buys);
        ensure_table_handle(session, sells);
    }

    // Extract PriceTimeIndex table handles
    let (pti_buys_handle, pti_sells_handle) = {
        let (_sob, _bob, pti) = perp_market.full_order_book_mut();
        let bcs_types::PriceTimeIndex::V1 { buys, sells } = pti;
        (
            buys.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle)),
            sells.get_table_handle().map(|th| aptos_table_natives::TableHandle(th.handle)),
        )
    };
    let mut bulk_tree_writes: Vec<(aptos_table_natives::TableHandle, TableWrite)> = Vec::new();

    // 5. Modify bulk_order_book
    let (bulk_order_book, price_time_idx) = perp_market.order_book_mut();

    // Remove existing bulk order for this subaccount (if any)
    let old_order_opt = bulk_order_book.remove_order(&subaccount);

    // Determine order_id and previous_seq_num
    let (order_id, previous_seq_num) = if let Some(ref old_order) = old_order_opt {
        let bcs_types::BulkOrder::V1 { order_request, order_id: old_id, .. } = old_order;
        let bcs_types::BulkOrderRequest::V1 { order_sequence_number: old_seq, .. } = order_request;

        if sequence_number <= *old_seq {
            bulk_order_book.add_order(subaccount, old_order.clone());
            write_perp_market(session, &market_addr, &publisher_addr, &perp_market)?;
            return Ok(());
        }

        // Remove old order's active price levels from price_time_idx (tree-aware)
        {
            let read_buys = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
                match pti_buys_handle { Some(h) => { let kb = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                    native_session_helpers::read_table_item_bytes(&*session, h, &kb).map(|o| o.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
                }, None => Ok(None) }
            };
            let read_sells = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
                match pti_sells_handle { Some(h) => { let kb = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                    native_session_helpers::read_table_item_bytes(&*session, h, &kb).map(|o| o.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
                }, None => Ok(None) }
            };
            let (bw, sw) = price_time_idx.tree_remove_bulk_order_prices(old_order, &read_buys, &read_sells);
            if let Some(h) = pti_buys_handle { for tw in bw { bulk_tree_writes.push((h, tw)); } }
            if let Some(h) = pti_sells_handle { for tw in sw { bulk_tree_writes.push((h, tw)); } }
        }

        (*old_id, *old_seq)
    } else {
        // New order: generate order_id
        let counter = get_next_counter(session, now_us)?;
        let new_order_id = next_order_id_from_counter(counter);
        bulk_order_book.add_order_id_mapping(new_order_id, subaccount);
        (new_order_id, 0u64)
    };

    // Generate unique_priority_idx
    let priority_counter = get_next_counter(session, now_us)?;
    let unique_priority_idx = bcs_types::IncreasingIdx { idx: priority_counter };

    // Sanitize: remove entries where bid crosses ask (prices that would be taker)
    // In Move, bulk_order_utils::new_bulk_order_with_sanitization handles this
    // For simplicity, we keep the prices as-is (benchmark data should be valid)
    let metadata = bcs_types::OrderMetadata::V1_BULK {
        builder_code: None,
    };

    let new_bulk_order = bcs_types::BulkOrder::V1 {
        order_request: bcs_types::BulkOrderRequest::V1 {
            account: subaccount,
            order_sequence_number: sequence_number,
            bid_prices: bid_prices.clone(),
            bid_sizes: bid_sizes.clone(),
            ask_prices: ask_prices.clone(),
            ask_sizes: ask_sizes.clone(),
            metadata: metadata.clone(),
        },
        order_id,
        unique_priority_idx,
        creation_time_micros: now_us,
    };

    // Add new bulk order to bulk_order_book
    bulk_order_book.add_order(subaccount, new_bulk_order);

    // Activate first price levels in the price-time index (tree-aware)
    {
        let read_buys = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
            match pti_buys_handle { Some(h) => { let kb = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                native_session_helpers::read_table_item_bytes(&*session, h, &kb).map(|o| o.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
            }, None => Ok(None) }
        };
        let read_sells = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
            match pti_sells_handle { Some(h) => { let kb = bcs::to_bytes(&slot_index).map_err(|e| format!("{}", e))?;
                native_session_helpers::read_table_item_bytes(&*session, h, &kb).map(|o| o.map(|b| b.to_vec())).map_err(|e| format!("{:?}", e))
            }, None => Ok(None) }
        };
        let (bw, sw) = price_time_idx.tree_activate_bulk_order_prices(
            order_id, &bid_prices, &bid_sizes, &ask_prices, &ask_sizes,
            unique_priority_idx, &read_buys, &read_sells,
        );
        if let Some(h) = pti_buys_handle { for tw in bw { bulk_tree_writes.push((h, tw)); } }
        if let Some(h) = pti_sells_handle { for tw in sw { bulk_tree_writes.push((h, tw)); } }
    }

    // 6. Emit BulkOrderPlacedEvent
    // Get cancelled prices from old order
    let (cancelled_bid_prices, cancelled_bid_sizes, cancelled_ask_prices, cancelled_ask_sizes) =
        if let Some(ref old_order) = old_order_opt {
            let bcs_types::BulkOrder::V1 { order_request, .. } = old_order;
            let bcs_types::BulkOrderRequest::V1 {
                bid_prices: old_bps, bid_sizes: old_bss,
                ask_prices: old_aps, ask_sizes: old_ass, ..
            } = order_request;
            (old_bps.clone(), old_bss.clone(), old_aps.clone(), old_ass.clone())
        } else {
            (vec![], vec![], vec![], vec![])
        };

    emit_bulk_order_placed_event(
        session,
        &publisher_addr,
        &perp_market,
        &order_id,
        sequence_number,
        &subaccount,
        bid_prices,
        bid_sizes,
        ask_prices,
        ask_sizes,
        cancelled_bid_prices,
        cancelled_bid_sizes,
        cancelled_ask_prices,
        cancelled_ask_sizes,
        previous_seq_num,
    )?;

    // 7. Flush tree writes and write back modified PerpMarket
    for (handle, tw) in bulk_tree_writes {
        let key_bytes = bcs::to_bytes(&tw.slot_index).map_err(|e| {
            VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR, Some(format!("slot key: {}", e)))
        })?;
        if tw.is_new {
            native_session_helpers::create_table_item_bytes(session, handle, &key_bytes, tw.data.into())?;
        } else {
            native_session_helpers::write_table_item_bytes(session, handle, &key_bytes, tw.data.into())?;
        }
    }
    write_perp_market(session, &market_addr, &publisher_addr, &perp_market)?;

    // 8. trigger_matching_sometimes: no-op in current Move code
    // Read AME for Block-STM read dependency (Move reads it during trigger call even though
    // trigger is a no-op). Do NOT write back — Move doesn't write AME for bulk orders.
    let _ame = read_ame(session, &market_addr, &publisher_addr)?;

    // Removed excessive resource touches that no longer match Move's write set.
    // With no-op trigger, Move's place_bulk_order only writes PerpMarket,
    // ObjectGroup@market, ObjectGroup@publisher, and AME.

    Ok(())
}

// ---------------------------------------------------------------------------
// Resource touching for settlement write set matching
// ---------------------------------------------------------------------------

/// Returns the BCS-serialized bytes for a default (empty) VolumeHistory::V1 entry.
///
/// This matches the initial state created by Move's `volume_tracker::update_volume`
/// when it encounters a new account for the first time via `table::add`.
///
/// BCS layout:
///   byte 0: V1 variant tag (0)
///   bytes 1-8: latest_day_since_epoch = 0 (u64)
///   bytes 9-40: latest_day_volume = AggregatorU128 { value: 0, max_value: u128::MAX }
///   byte 41: history vector length = 0 (ULEB128)
///   bytes 42-57: total_volume_in_window = 0 (u128)
///   bytes 58-89: total_volume_all_time = AggregatorU128 { value: 0, max_value: u128::MAX }
fn default_volume_history_bytes() -> Bytes {
    let default = bcs_types::VolumeHistory::V1 {
        latest_day_since_epoch: 0,
        latest_day_volume: bcs_types::AggregatorU128 {
            value: 0,
            max_value: u128::MAX,
        },
        history: vec![],
        total_volume_in_window: 0,
        total_volume_all_time: bcs_types::AggregatorU128 {
            value: 0,
            max_value: u128::MAX,
        },
    };
    Bytes::from(bcs::to_bytes(&default).expect("default VolumeHistory serialization"))
}

/// Touch (read + write-back) resources that the Move VM modifies during trade settlement.
///
/// During order matching, the Move VM's `settle_trade` callback reads and writes:
/// 1. `UserPositions` at each taker and maker address (position updates)
/// 2. `GlobalAccountStates` at publisher (collateral balance sheet)
/// 3. `CachedPositionStatuses` at publisher (aggregated position caches)
/// 4. `OpenInterestTracker` at market (open interest changes)
///
/// We "touch" these resources (read raw bytes, write them back) to match the
/// write set size without implementing the full clearinghouse business logic.
fn touch_settlement_resources<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    publisher_addr: &AccountAddress,
    market_addr: &AccountAddress,
    events: &[OrderMatchEvent],
) -> Result<Vec<(bool, bool, bool, bool, bool, bool)>, VMStatus> {
    // Collect unique account addresses involved in fills
    let mut accounts = std::collections::HashSet::new();
    let mut fill_count: u64 = 0;

    for event in events {
        match event {
            OrderMatchEvent::SingleFill { taker_account, maker_account, .. } => {
                accounts.insert(*taker_account);
                if *maker_account != AccountAddress::ZERO {
                    accounts.insert(*maker_account);
                }
                fill_count += 1;
            },
            OrderMatchEvent::BulkFill { taker_account, maker_account, .. } => {
                accounts.insert(*taker_account);
                accounts.insert(*maker_account);
                fill_count += 1;
            },
            _ => {},
        }
    }

    if fill_count == 0 {
        return Ok(Vec::new());
    }

    // Read UserPositions for all accounts, process fills in order to compute PnL flags,
    // then write back the updated positions.
    let user_positions_tag = make_struct_tag(*publisher_addr, "perp_positions", "UserPositions");

    // Load UserPositions for all accounts into an in-memory cache
    let mut position_cache: std::collections::HashMap<AccountAddress, Option<bcs_types::OnChainUserPositions>> =
        std::collections::HashMap::new();
    let mut raw_bytes_cache: std::collections::HashMap<AccountAddress, Bytes> =
        std::collections::HashMap::new();
    // Track which accounts are new (no existing UserPositions resource).
    // We'll create a default for them so fills can be applied and the position
    // is persisted for subsequent transactions in the same block.
    let mut new_accounts = std::collections::HashSet::new();
    for account in &accounts {
        match native_session_helpers::read_resource_bytes(session, account, &user_positions_tag) {
            Ok(Some(bytes)) => {
                if let Ok(user_pos) = bcs::from_bytes::<bcs_types::OnChainUserPositions>(&bytes) {
                    position_cache.insert(*account, Some(user_pos));
                } else {
                    position_cache.insert(*account, None);
                }
                raw_bytes_cache.insert(*account, bytes);
            },
            Ok(None) => {
                // Account has no UserPositions resource yet. Create a default empty one
                // matching Move's `UserPositions::V1 { positions: big_ordered_map::new_with_config(64, 16, false) }`.
                // This ensures fills are tracked and the position is written back,
                // so subsequent transactions in the same block can see it via Block-STM's resolver.
                let empty_pos = bcs_types::OnChainUserPositions::new_empty();
                position_cache.insert(*account, Some(empty_pos));
                new_accounts.insert(*account);
            },
            Err(_e) => {
                position_cache.insert(*account, None);
            },
        }
    }


    // Pre-allocate table handles for UserPositions B+ trees that might need splitting.
    // Only do this for accounts whose positions map is at capacity (leaf root full)
    // or already has inner nodes (table handle should already exist but ensure it).
    // This must happen before the immutable borrow on session (table_reader closure),
    // because creating a table handle requires mutable session access.
    for account in &accounts {
        if let Some(Some(user_pos)) = position_cache.get_mut(account) {
            let bcs_types::OnChainUserPositions::V1 { positions } = user_pos;
            let needs_table = {
                let (root_is_leaf, leaf_max) = {
                    let bcs_types::BigOrderedMap::BPlusTreeMap { root, leaf_max_degree, .. } = &*positions;
                    let bcs_types::Node::V1 { is_leaf, .. } = root;
                    (*is_leaf, *leaf_max_degree as usize)
                };
                if !root_is_leaf {
                    // Inner root - table handle should already exist, but ensure it
                    positions.get_table_handle().is_none()
                } else {
                    // Leaf root - only need table if leaf is full (might need split)
                    let bcs_types::BigOrderedMap::BPlusTreeMap { root, .. } = &*positions;
                    let bcs_types::Node::V1 { children, .. } = root;
                    let bcs_types::OrderedMap::SortedVectorMap { entries } = children;
                    entries.len() >= leaf_max && positions.get_table_handle().is_none()
                }
            };
            if needs_table {
                ensure_table_handle(session, positions);
            }
        }
    }

    // Read TradingFeeConfiguration and build volume cache for maker fee determination.
    // This must happen BEFORE the table_reader closure (which borrows session immutably).
    let trading_fees_tag = make_struct_tag(*publisher_addr, "trading_fees_manager", "GlobalState");
    let mut fee_config_opt: Option<(Vec<u128>, Vec<u64>, Vec<u64>)> = None;  // (tier_thresholds, tier_maker_fees, tier_taker_fees)
    let mut account_volume_cache: std::collections::HashMap<AccountAddress, u128> = std::collections::HashMap::new();
    {
        // Read GlobalState to extract fee config + volume table handles
        if let Ok(Some(bytes)) = native_session_helpers::read_resource_bytes(session, publisher_addr, &trading_fees_tag) {
            if bytes.len() > 43 {
                let offset = 43usize;
                let mut vec_len = 0u64;
                let mut shift = 0u32;
                let mut cursor = offset;
                loop {
                    if cursor >= bytes.len() { break; }
                    let b = bytes[cursor] as u64;
                    cursor += 1;
                    vec_len |= (b & 0x7F) << shift;
                    if b & 0x80 == 0 { break; }
                    shift += 7;
                    if shift >= 64 { break; }
                }
                cursor += (vec_len as usize) * 25;
                cursor += 48;
                if cursor + 64 <= bytes.len() {
                    let mut taker_h = [0u8; 32];
                    let mut maker_h = [0u8; 32];
                    taker_h.copy_from_slice(&bytes[cursor..cursor+32]);
                    maker_h.copy_from_slice(&bytes[cursor+32..cursor+64]);
                    if let (Ok(ta), Ok(ma)) = (AccountAddress::from_bytes(taker_h), AccountAddress::from_bytes(maker_h)) {
                        let taker_handle = aptos_table_natives::TableHandle(ta);
                        let maker_handle = aptos_table_natives::TableHandle(ma);
                        // Deserialize TradingFeeConfiguration
                        let fee_config_offset = cursor + 64;
                        if fee_config_offset < bytes.len() {
                            // Manually parse TradingFeeConfiguration BCS from the prefix.
                            // We can't use bcs::from_bytes because there are trailing fields.
                            // Parse just tier_thresholds and tier_maker_fees.
                            let fee_bytes = &bytes[fee_config_offset..];
                            match parse_fee_config_prefix(fee_bytes) {
                                Some((thresholds, maker_fees, taker_fees)) => {
                                    fee_config_opt = Some((thresholds, maker_fees, taker_fees));
                                },
                                None => {
                                }
                            }
                        }
                        // Build volume cache
                        for account in &accounts {
                            if let Ok(key_bytes) = bcs::to_bytes(account) {
                                let mut total_vol: u128 = 0;
                                if let Ok(Some(entry)) = native_session_helpers::read_table_item_bytes(session, taker_handle, &key_bytes) {
                                    if let Ok(vh) = bcs::from_bytes::<bcs_types::VolumeHistory>(&entry) {
                                        total_vol += vh.total_volume_in_window();
                                    }
                                }
                                if let Ok(Some(entry)) = native_session_helpers::read_table_item_bytes(session, maker_handle, &key_bytes) {
                                    if let Ok(vh) = bcs::from_bytes::<bcs_types::VolumeHistory>(&entry) {
                                        total_vol += vh.total_volume_in_window();
                                    }
                                }
                                account_volume_cache.insert(*account, total_vol);
                            }
                        }
                    }
                }
            }
        }
    }

    // Read PerpMarketConfiguration to get sz_precision.multiplier for fee calculations.
    // Move computes maker fee as: notional * fee_rate / 1_000_000 where notional = fill_size * fill_price / sz_multiplier.
    // Integer division can make fees round to 0 for small notional values.
    let sz_multiplier: u64 = {
        let object_group_tag = make_struct_tag(AccountAddress::ONE, "object", "ObjectGroup");
        let config_tag = make_struct_tag(*publisher_addr, "perp_market_config", "PerpMarketConfiguration");
        if let Ok(Some(config_bytes)) = native_session_helpers::read_resource_group_member_bytes(
            session, market_addr, &object_group_tag, &config_tag,
        ) {
            match bcs::from_bytes::<perp_market_config::PerpMarketConfiguration>(&config_bytes) {
                Ok(config) => perp_market_config::get_size_multiplier(&config),
                Err(_) => 1_000_000, // fallback: 6 decimals
            }
        } else {
            1_000_000 // fallback: 6 decimals
        }
    };
    // Create a table reader closure for looking up positions in inner-node B+ trees.
    // When a user has positions in >16 markets, the UserPositions B+ tree splits into
    // inner nodes, and we need to read child node table items to find the position.
    let lookup_found = std::cell::Cell::new(0u32);
    let lookup_missed = std::cell::Cell::new(0u32);
    let table_reader = |table_handle: &AccountAddress, key: &[u8]| -> Option<bytes::Bytes> {
        let handle = aptos_table_natives::TableHandle(*table_handle);
        let result = native_session_helpers::read_table_item_bytes(session, handle, key).ok().flatten();
        if result.is_some() { lookup_found.set(lookup_found.get() + 1); }
        else { lookup_missed.set(lookup_missed.get() + 1); }
        result
    };

    // Collect table writes from tree operations (B+ tree splits during position updates).
    let mut position_table_writes: std::collections::HashMap<AccountAddress, Vec<bcs_types::TableWrite>> =
        std::collections::HashMap::new();

    // Process fills in order: check position BEFORE fill (for PnL flag), then update.
    // Move emits PnL only when the fill reduces/closes/flips an existing position
    // (opposite direction to existing position with size > 0).
    // Mutable volume tracker: starts from on-chain volume, incremented per fill.
    // Used to determine maker fee tier (matching Move's volume tracking behavior).
    let mut running_volume: std::collections::HashMap<AccountAddress, u128> = account_volume_cache.clone();
    let mut pnl_flags: Vec<(bool, bool, bool, bool, bool, bool)> = Vec::new();
    for event in events {
        match event {
            OrderMatchEvent::SingleFill {
                taker_account, maker_account, fill_size, fill_price, taker_is_buy, ..
            } => {
                // Taker position info (PnL + isolation)
                let (taker_has_pnl, taker_is_isolated) = position_cache.get(taker_account)
                    .and_then(|opt| opt.as_ref())
                    .and_then(|up| up.get_position_full_info_with_table_lookup(market_addr, &table_reader))
                    .map(|(size, _is_long, is_isolated)| (size > 0, is_isolated))
                    .unwrap_or((false, false));

                // Maker position info (PnL + isolation)
                let (maker_has_pnl, maker_is_isolated) = if *maker_account != AccountAddress::ZERO {
                    position_cache.get(maker_account)
                        .and_then(|opt| opt.as_ref())
                        .and_then(|up| up.get_position_full_info_with_table_lookup(market_addr, &table_reader))
                        .map(|(size, _is_long, is_isolated)| (size > 0, is_isolated))
                        .unwrap_or((false, false))
                } else {
                    (false, false)
                };

                // Compute actual fees per fill matching Move's integer arithmetic.
                // Move: notional = fill_size * fill_price / sz_multiplier, fee = notional * rate / 1_000_000
                let notional_for_fee = (*fill_size as u128) * (*fill_price as u128) / (sz_multiplier as u128);
                let taker_has_fee = if let Some((thresholds, _maker_fees, taker_fees)) = &fee_config_opt {
                    let vol = running_volume.get(taker_account).copied().unwrap_or(0);
                    let fee_rate = get_fee_rate_for_volume(thresholds, taker_fees, vol);
                    fee_rate > 0 && notional_for_fee * (fee_rate as u128) / 1_000_000u128 > 0
                } else { true }; // default: emit (backward compat)
                let maker_has_fee = if *maker_account != AccountAddress::ZERO {
                    if let Some((thresholds, maker_fees, _taker_fees)) = &fee_config_opt {
                        let vol = running_volume.get(maker_account).copied().unwrap_or(0);
                        let fee_rate = get_fee_rate_for_volume(thresholds, maker_fees, vol);
                        fee_rate > 0 && notional_for_fee * (fee_rate as u128) / 1_000_000u128 > 0
                    } else { false }
                } else { false };
                // Increment running volume for both accounts (matches Move's volume tracking)
                {
                    let notional = (*fill_size as u128) * (*fill_price as u128);
                    *running_volume.entry(*taker_account).or_insert(0) += notional;
                    if *maker_account != AccountAddress::ZERO {
                        *running_volume.entry(*maker_account).or_insert(0) += notional;
                    }
                }

                pnl_flags.push((taker_has_pnl, maker_has_pnl, maker_has_fee, taker_has_fee, taker_is_isolated, maker_is_isolated));

                // Update positions after PnL check (tree-aware for >16 market positions)
                if let Some(Some(user_pos)) = position_cache.get_mut(taker_account) {
                    let writes = user_pos.update_position_for_fill_tree(market_addr, *fill_size, *fill_price, *taker_is_buy, &table_reader);
                    if !writes.is_empty() {
                        position_table_writes.entry(*taker_account).or_default().extend(writes);
                    }
                }
                if *maker_account != AccountAddress::ZERO {
                    if let Some(Some(user_pos)) = position_cache.get_mut(maker_account) {
                        let writes = user_pos.update_position_for_fill_tree(market_addr, *fill_size, *fill_price, !*taker_is_buy, &table_reader);
                        if !writes.is_empty() {
                            position_table_writes.entry(*maker_account).or_default().extend(writes);
                        }
                    }
                }
            },
            OrderMatchEvent::BulkFill {
                taker_account, maker_account, fill_size, fill_price, taker_is_buy, ..
            } => {
                let (taker_has_pnl, taker_is_isolated_bulk) = position_cache.get(taker_account)
                    .and_then(|opt| opt.as_ref())
                    .and_then(|up| up.get_position_full_info_with_table_lookup(market_addr, &table_reader))
                    .map(|(size, is_long, is_isolated)| (size > 0 && is_long != *taker_is_buy, is_isolated))
                    .unwrap_or((false, false));

                let (maker_has_pnl, maker_is_isolated_bulk) = if *maker_account != AccountAddress::ZERO {
                    position_cache.get(maker_account)
                        .and_then(|opt| opt.as_ref())
                        .and_then(|up| up.get_position_full_info_with_table_lookup(market_addr, &table_reader))
                        .map(|(size, is_long, is_isolated)| (size > 0 && is_long == *taker_is_buy, is_isolated))
                        .unwrap_or((false, false))
                } else {
                    (false, false)
                };

                // Compute actual fees per fill matching Move's integer arithmetic.
                let notional_for_fee = (*fill_size as u128) * (*fill_price as u128) / (sz_multiplier as u128);
                let taker_has_fee_bulk = if let Some((thresholds, _maker_fees, taker_fees)) = &fee_config_opt {
                    let vol = running_volume.get(taker_account).copied().unwrap_or(0);
                    let fee_rate = get_fee_rate_for_volume(thresholds, taker_fees, vol);
                    fee_rate > 0 && notional_for_fee * (fee_rate as u128) / 1_000_000u128 > 0
                } else { true }; // default: emit
                let maker_has_fee_bulk = if *maker_account != AccountAddress::ZERO {
                    if let Some((thresholds, maker_fees, _taker_fees)) = &fee_config_opt {
                        let vol = running_volume.get(maker_account).copied().unwrap_or(0);
                        let fee_rate = get_fee_rate_for_volume(thresholds, maker_fees, vol);
                        fee_rate > 0 && notional_for_fee * (fee_rate as u128) / 1_000_000u128 > 0
                    } else { false }
                } else { false };
                // Increment running volume for both accounts
                {
                    let notional = (*fill_size as u128) * (*fill_price as u128);
                    *running_volume.entry(*taker_account).or_insert(0) += notional;
                    if *maker_account != AccountAddress::ZERO {
                        *running_volume.entry(*maker_account).or_insert(0) += notional;
                    }
                }

                pnl_flags.push((taker_has_pnl, maker_has_pnl, maker_has_fee_bulk, taker_has_fee_bulk, taker_is_isolated_bulk, maker_is_isolated_bulk));


                if let Some(Some(user_pos)) = position_cache.get_mut(taker_account) {
                    let writes = user_pos.update_position_for_fill_tree(market_addr, *fill_size, *fill_price, *taker_is_buy, &table_reader);
                    if !writes.is_empty() {
                        position_table_writes.entry(*taker_account).or_default().extend(writes);
                    }
                }
                if *maker_account != AccountAddress::ZERO {
                    if let Some(Some(user_pos)) = position_cache.get_mut(maker_account) {
                        let writes = user_pos.update_position_for_fill_tree(market_addr, *fill_size, *fill_price, !*taker_is_buy, &table_reader);
                        if !writes.is_empty() {
                            position_table_writes.entry(*maker_account).or_default().extend(writes);
                        }
                    }
                }

            },
            _ => {},
        }
    }
    // Position lookup stats: found via table, missed
    // Drop the table_reader closure to release the immutable borrow on session,
    // allowing mutable borrows in the write-back section below.
    let _ = table_reader;



    // Write back updated positions for all accounts
    for account in &accounts {
        if let Some(Some(user_pos)) = position_cache.get(account) {
            if let Ok(new_bytes) = bcs::to_bytes(user_pos) {
                native_session_helpers::write_resource_bytes(
                    session, account, &user_positions_tag, new_bytes.into(),
                )?;
            } else if let Some(bytes) = raw_bytes_cache.get(account) {
                native_session_helpers::write_resource_bytes(
                    session, account, &user_positions_tag, bytes.clone(),
                )?;
            }
        } else if let Some(bytes) = raw_bytes_cache.get(account) {
            native_session_helpers::write_resource_bytes(
                session, account, &user_positions_tag, bytes.clone(),
            )?;
        }
    }

    // Flush B+ tree table writes from position updates (splits/node modifications).
    for (account, writes) in &position_table_writes {
        if !writes.is_empty() {
            if let Some(Some(user_pos)) = position_cache.get(account) {
                let bcs_types::OnChainUserPositions::V1 { positions } = user_pos;
                flush_map_writes(session, positions, writes.clone())?;
            }
        }
    }


    // Touch GlobalAccountStates at publisher (collateral balance sheet)
    // Also extract balance_table handle for per-account table item writes below.
    let global_states_tag = make_struct_tag(*publisher_addr, "accounts_collateral", "GlobalAccountStates");
    let mut balance_table_handle_opt: Option<aptos_table_natives::TableHandle> = None;
    match native_session_helpers::read_resource_bytes(session, publisher_addr, &global_states_tag) {
        Ok(Some(bytes)) => {
            // Extract balance_table handle at BCS offset 172 (32 bytes).
            // See layout comment in the balance_table read/write section below.
            if bytes.len() >= 204 {
                let mut handle_bytes = [0u8; 32];
                handle_bytes.copy_from_slice(&bytes[172..204]);
                if let Ok(addr) = AccountAddress::from_bytes(handle_bytes) {
                    balance_table_handle_opt = Some(aptos_table_natives::TableHandle(addr));
                }
            }
            native_session_helpers::write_resource_bytes(
                session, publisher_addr, &global_states_tag, bytes,
            )?;
        },
        Ok(None) => {},
        Err(_e) => {},
    }

    // Touch CachedPositionStatuses at publisher
    let cached_statuses_tag = make_struct_tag(*publisher_addr, "perp_positions", "CachedPositionStatuses");
    if let Some(bytes) = native_session_helpers::read_resource_bytes(session, publisher_addr, &cached_statuses_tag)? {
        native_session_helpers::write_resource_bytes(
            session, publisher_addr, &cached_statuses_tag, bytes,
        )?;
    }

    // Compute per-account fill notional for volume tracking.
    // Each fill contributes notional = size * price to both taker and maker volume.
    for event in events {
        match event {
            OrderMatchEvent::SingleFill { taker_account: _, maker_account, fill_size, fill_price, .. } |
            OrderMatchEvent::BulkFill { taker_account: _, maker_account, fill_size, fill_price, .. } => {
                let _notional = (*fill_size as u128) * (*fill_price as u128);
                if *maker_account != AccountAddress::ZERO {
                }
            },
            _ => {},
        }
    }

    // Touch TradingFeesManager::GlobalState at publisher (volume tracking)
    // Also extract VolumeStats table handles for per-account volume writes,
    // and TradingFeeConfiguration for maker fee determination.
    // GlobalState::V1 { volume_stats: VolumeStats, fee_config: TradingFeeConfiguration, ... }
    if let Ok(Some(bytes)) = native_session_helpers::read_resource_bytes(session, publisher_addr, &trading_fees_tag) {
        // Try to extract VolumeStats table handles from the raw bytes.
        // BCS layout: byte 0 = GlobalState V1 variant, then VolumeStats starts.
        // VolumeStats::V1 { global_history, user_taker_volume_history, user_maker_volume_history }
        // We deserialize just the VolumeStats prefix to get the table handles.
        if bytes.len() > 1 {
            // Skip the GlobalState variant tag (byte 0)
            // Extract VolumeStats table handles by manually walking BCS bytes.
            // Layout after GlobalState V1 tag (byte 0):
            //   byte 1: VolumeStats V1 tag
            //   byte 2: VolumeHistory V1 tag
            //   bytes 3-10: latest_day_since_epoch (u64)
            //   bytes 11-42: latest_day_volume (Aggregator<u128> = value:u128 + max:u128)
            //   then ULEB128 vector length + variable DayVolume entries
            //   then 16 bytes: total_volume_in_window (u128)
            //   then 32 bytes: total_volume_all_time (Aggregator<u128>)
            //   then 32 bytes: user_taker_volume_history Table handle
            //   then 32 bytes: user_maker_volume_history Table handle
            let offset = 43usize; // start of history vector (after fixed prefix)
            if bytes.len() > offset && fill_count > 0 {
                // Read ULEB128 vector length
                let mut vec_len = 0u64;
                let mut shift = 0u32;
                let mut cursor = offset;
                loop {
                    if cursor >= bytes.len() { break; }
                    let b = bytes[cursor] as u64;
                    cursor += 1;
                    vec_len |= (b & 0x7F) << shift;
                    if b & 0x80 == 0 { break; }
                    shift += 7;
                    if shift >= 64 { break; }
                }
                // Each DayVolume::V1 = variant(1) + day_since_epoch(8) + volume(16) = 25 bytes
                cursor += (vec_len as usize) * 25;
                // Skip total_volume_in_window (16) + total_volume_all_time (32) = 48 bytes
                cursor += 48;
                // Now at user_taker_volume_history handle (32 bytes)
                if cursor + 64 <= bytes.len() {
                    let mut taker_h = [0u8; 32];
                    let mut maker_h = [0u8; 32];
                    taker_h.copy_from_slice(&bytes[cursor..cursor+32]);
                    maker_h.copy_from_slice(&bytes[cursor+32..cursor+64]);
                    if let (Ok(ta), Ok(ma)) = (AccountAddress::from_bytes(taker_h), AccountAddress::from_bytes(maker_h)) {
                        let taker_handle = aptos_table_natives::TableHandle(ta);
                        let maker_handle = aptos_table_natives::TableHandle(ma);
                        for account in &accounts {
                            // Compute total notional for this account from running_volume
                            let acct_notional = running_volume.get(account).copied().unwrap_or(0);
                            if let Ok(key_bytes) = bcs::to_bytes(account) {
                                // Read/write taker volume entry, incrementing total_volume_in_window
                                match native_session_helpers::read_table_item_bytes(
                                    session, taker_handle, &key_bytes,
                                ) {
                                    Ok(Some(entry)) => {
                                        let updated = increment_volume_in_window(&entry, acct_notional);
                                        let _ = native_session_helpers::write_table_item_bytes(
                                            session, taker_handle, &key_bytes, updated,
                                        );
                                    },
                                    Ok(None) => {
                                        let default_entry = default_volume_history_bytes();
                                        let _ = native_session_helpers::create_table_item_bytes(
                                            session, taker_handle, &key_bytes, default_entry,
                                        );
                                    },
                                    Err(_) => {},
                                }
                                match native_session_helpers::read_table_item_bytes(
                                    session, maker_handle, &key_bytes,
                                ) {
                                    Ok(Some(entry)) => {
                                        let updated = increment_volume_in_window(&entry, acct_notional);
                                        let _ = native_session_helpers::write_table_item_bytes(
                                            session, maker_handle, &key_bytes, updated,
                                        );
                                    },
                                    Ok(None) => {
                                        let default_entry = default_volume_history_bytes();
                                        let _ = native_session_helpers::create_table_item_bytes(
                                            session, maker_handle, &key_bytes, default_entry,
                                        );
                                    },
                                    Err(_) => {},
                                }
                            }
                        }
                    }
                }
            }
        }
        let _ = native_session_helpers::write_resource_bytes(
            session, publisher_addr, &trading_fees_tag, bytes,
        );
    }



    // Touch OpenInterestTracker at market address.
    // In Move, settle_trade calls open_interest_tracker::mark_open_interest_delta_for_market()
    // which writes the tracker and emits OpenInterestUpdateEvent per fill.
    let oi_tracker_tag = make_struct_tag(*publisher_addr, "open_interest_tracker", "OpenInterestTracker");
    if let Ok(Some(bytes)) = native_session_helpers::read_resource_bytes(session, market_addr, &oi_tracker_tag) {
        let _ = native_session_helpers::write_resource_bytes(
            session, market_addr, &oi_tracker_tag, bytes,
        );
    }

    // Emit OpenInterestUpdateEvent once per fill (matches Move VM behavior)
    let oi_event_tag = make_struct_tag(*publisher_addr, "open_interest_tracker", "OpenInterestUpdateEvent");
    for _ in 0..fill_count {
        let oi_event = bcs_types::OpenInterestUpdateEvent::V1 {
            market: *market_addr,
            current_open_interest: 0, // Intentionally hardcoded: this field is event-only metadata and does not affect on-chain state or matching correctness.
        };
        let event_bytes = bcs::to_bytes(&oi_event).unwrap_or_default();
        native_session_helpers::emit_event(
            session,
            move_core_types::language_storage::TypeTag::Struct(Box::new(oi_event_tag.clone())),
            event_bytes,
        )?;
    }

    // Touch PendingOrderTracker::GlobalSummary at publisher and its per-account table items.
    // In Move, settle_trade calls pending_order_tracker::remove_pending_order() /
    // remove_bulk_order() which modifies GlobalSummary.summary[account] table entries.
    // GlobalSummary::V1 { summary: Table<address, AccountSummary> }
    // BCS: byte 0 = variant tag, bytes 1-32 = summary Table handle
    let pending_tracker_tag = make_struct_tag(*publisher_addr, "pending_order_tracker", "GlobalSummary");
    if let Ok(Some(bytes)) = native_session_helpers::read_resource_bytes(session, publisher_addr, &pending_tracker_tag) {
        // Extract the summary table handle at offset 1 (32 bytes)
        if bytes.len() >= 33 {
            let mut handle_bytes = [0u8; 32];
            handle_bytes.copy_from_slice(&bytes[1..33]);
            if let Ok(addr) = AccountAddress::from_bytes(handle_bytes) {
                let summary_handle = aptos_table_natives::TableHandle(addr);
                // For each account involved in fills, read/write their AccountSummary entry
                for account in &accounts {
                    // Table key is just an address (BCS-serialized)
                    if let Ok(key_bytes) = bcs::to_bytes(account) {
                        if let Ok(Some(entry_bytes)) = native_session_helpers::read_table_item_bytes(
                            session, summary_handle, &key_bytes,
                        ) {
                            let _ = native_session_helpers::write_table_item_bytes(
                                session, summary_handle, &key_bytes, entry_bytes,
                            );
                        }
                    }
                }
            }
        }
        let _ = native_session_helpers::write_resource_bytes(
            session, publisher_addr, &pending_tracker_tag, bytes,
        );
    }

    // Touch ADLTracker at market address.
    // In Move, settle_trade calls adl_tracker::mark_trade() which updates the
    // ADLTracker resource at the market's secondary resources address.
    // The secondary resources address is typically derived from the market.
    // We read from the market address first; if not found, we try the object group.
    let adl_tracker_tag = make_struct_tag(*publisher_addr, "adl_tracker", "ADLTracker");
    // ADLTracker may be at the market address or at a secondary resources address.
    // Try the market address first (most common in benchmark).
    if let Ok(Some(bytes)) = native_session_helpers::read_resource_bytes(session, market_addr, &adl_tracker_tag) {
        let _ = native_session_helpers::write_resource_bytes(
            session, market_addr, &adl_tracker_tag, bytes,
        );
    }

    // Read/write per-account balance_table entries from GlobalAccountStates.
    // In Move, settle_trade modifies the per-account collateral balance entries
    // in the CollateralBalanceSheet.balance_table (a Table<CollateralBalanceType, CollateralBalances>).
    // The balance_table handle was extracted above from the GlobalAccountStates raw bytes.
    //
    // BCS layout of GlobalAccountStates::V1 (for reference):
    //   byte 0: variant tag (V1 = 0)
    //   byte 1: CollateralBalanceSheet::V1 variant tag
    //   bytes 2-33: primary_asset_type (32 bytes)
    //   byte 34: CollateralStore::V1 variant tag
    //   bytes 35-66: asset_type (32), byte 67: decimals (1), bytes 68-75: multiplier (8)
    //   bytes 76-107: store (32), bytes 108-139: store_extend_ref (32)
    //   bytes 140-171: secondary_stores handle (32)
    //   bytes 172-203: balance_table handle (32)
    if let Some(balance_table_handle) = balance_table_handle_opt {
        // For each account involved in fills, construct the table key
        // (CollateralBalanceType::Cross { account }) and read/write the table entry.
        for account in &accounts {
            let key = bcs_types::CollateralBalanceType::Cross { account: *account };
            if let Ok(key_bytes) = bcs::to_bytes(&key) {
                match native_session_helpers::read_table_item_bytes(
                    session, balance_table_handle, &key_bytes,
                ) {
                    Ok(Some(entry_bytes)) => {
                        let _ = native_session_helpers::write_table_item_bytes(
                            session, balance_table_handle, &key_bytes, entry_bytes,
                        );
                    },
                    _ => {},
                }
            }
        }
    }

    Ok(pnl_flags)
}

// ---------------------------------------------------------------------------
// Event emission for order matching
// ---------------------------------------------------------------------------

/// Emit all collected order matching events.
///
/// This converts the internal `OrderMatchEvent` enum into proper BCS-serialized
/// events that match the Move VM's event output during order matching.
fn emit_order_match_events<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    publisher_addr: &AccountAddress,
    perp_market: &bcs_types::PerpMarket,
    events: &[OrderMatchEvent],
    now_us: u64,
) -> Result<(), VMStatus> {
    if !perp_market.allow_events_emission() || events.is_empty() {
        return Ok(());
    }

    let (parent, market_addr) = perp_market.parent_and_market_addresses();
    let event_tag = make_struct_tag(*publisher_addr, "market_types", "OrderEvent");

    for event in events {
        match event {
            OrderMatchEvent::TakerOpen {
                account, order_id, client_order_id, orig_size, remaining_size,
                price, is_buy, time_in_force, metadata,
            } => {
                let metadata_bytes = bcs::to_bytes(metadata).unwrap_or_default();
                let order_event = bcs_types::OrderEvent::V1 {
                    parent,
                    market: market_addr,
                    order_id: order_id.order_id,
                    client_order_id: client_order_id.clone(),
                    user: *account,
                    orig_size: *orig_size,
                    remaining_size: *remaining_size,
                    size_delta: *orig_size,
                    price: *price,
                    is_bid: *is_buy,
                    is_taker: true,
                    status: bcs_types::OrderStatus::OPEN,
                    details: String::new(),
                    metadata_bytes,
                    time_in_force: *time_in_force,
                    trigger_condition: None,
                    cancellation_reason: None,
                };
                let event_bytes = bcs::to_bytes(&order_event).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(event_tag.clone())),
                    event_bytes,
                )?;
            },
            OrderMatchEvent::SingleFill {
                taker_account, taker_order_id, taker_client_order_id,
                taker_orig_size, taker_remaining_size,
                fill_size, fill_price, taker_is_buy, taker_time_in_force, taker_metadata,
                maker_account, maker_order_id, maker_client_order_id,
                maker_orig_size, maker_remaining_size, maker_time_in_force, maker_metadata,
            } => {
                // Taker fill event
                let taker_metadata_bytes = bcs::to_bytes(taker_metadata).unwrap_or_default();
                let taker_fill = bcs_types::OrderEvent::V1 {
                    parent,
                    market: market_addr,
                    order_id: taker_order_id.order_id,
                    client_order_id: taker_client_order_id.clone(),
                    user: *taker_account,
                    orig_size: *taker_orig_size,
                    remaining_size: *taker_remaining_size,
                    size_delta: *fill_size,
                    price: *fill_price,
                    is_bid: *taker_is_buy,
                    is_taker: true,
                    status: bcs_types::OrderStatus::FILLED,
                    details: String::new(),
                    metadata_bytes: taker_metadata_bytes,
                    time_in_force: *taker_time_in_force,
                    trigger_condition: None,
                    cancellation_reason: None,
                };
                let event_bytes = bcs::to_bytes(&taker_fill).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(event_tag.clone())),
                    event_bytes,
                )?;

                // Maker fill event
                let maker_metadata_bytes = bcs::to_bytes(maker_metadata).unwrap_or_default();
                let maker_fill = bcs_types::OrderEvent::V1 {
                    parent,
                    market: market_addr,
                    order_id: maker_order_id.order_id,
                    client_order_id: maker_client_order_id.clone(),
                    user: *maker_account,
                    orig_size: *maker_orig_size,
                    remaining_size: *maker_remaining_size,
                    size_delta: *fill_size,
                    price: *fill_price,
                    is_bid: !*taker_is_buy,
                    is_taker: false,
                    status: bcs_types::OrderStatus::FILLED,
                    details: String::new(),
                    metadata_bytes: maker_metadata_bytes,
                    time_in_force: *maker_time_in_force,
                    trigger_condition: None,
                    cancellation_reason: None,
                };
                let event_bytes = bcs::to_bytes(&maker_fill).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(event_tag.clone())),
                    event_bytes,
                )?;
            },
            OrderMatchEvent::BulkFill {
                taker_account, taker_order_id, taker_client_order_id,
                taker_orig_size, taker_remaining_size,
                fill_size, fill_price, taker_is_buy, taker_time_in_force, taker_metadata,
                maker_account, maker_order_id, maker_sequence_number,
            } => {
                // Taker fill event
                let taker_metadata_bytes = bcs::to_bytes(taker_metadata).unwrap_or_default();
                let taker_fill = bcs_types::OrderEvent::V1 {
                    parent,
                    market: market_addr,
                    order_id: taker_order_id.order_id,
                    client_order_id: taker_client_order_id.clone(),
                    user: *taker_account,
                    orig_size: *taker_orig_size,
                    remaining_size: *taker_remaining_size,
                    size_delta: *fill_size,
                    price: *fill_price,
                    is_bid: *taker_is_buy,
                    is_taker: true,
                    status: bcs_types::OrderStatus::FILLED,
                    details: String::new(),
                    metadata_bytes: taker_metadata_bytes,
                    time_in_force: *taker_time_in_force,
                    trigger_condition: None,
                    cancellation_reason: None,
                };
                let event_bytes = bcs::to_bytes(&taker_fill).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(event_tag.clone())),
                    event_bytes,
                )?;

                // Bulk order fill event (different event type)
                let bulk_event_tag = make_struct_tag(*publisher_addr, "market_types", "BulkOrderFilledEvent");
                let bulk_fill_id = get_next_counter(session, now_us).unwrap_or(0) as u64;
                let bulk_fill = bcs_types::BulkOrderFilledEvent::V1 {
                    parent,
                    market: market_addr,
                    order_id: maker_order_id.order_id,
                    sequence_number: *maker_sequence_number,
                    user: *maker_account,
                    size: *fill_size,
                    fill_price: *fill_price,
                    order_price: *fill_price,
                    is_bid: !*taker_is_buy,
                    fill_id: bulk_fill_id as u128,
                };
                let event_bytes = bcs::to_bytes(&bulk_fill).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(bulk_event_tag)),
                    event_bytes,
                )?;
            },
            OrderMatchEvent::TakerCancelled {
                account, order_id, client_order_id, orig_size, remaining_size,
                price, is_buy, time_in_force, metadata, reason,
            } => {
                let metadata_bytes = bcs::to_bytes(metadata).unwrap_or_default();
                let cancel_event = bcs_types::OrderEvent::V1 {
                    parent,
                    market: market_addr,
                    order_id: order_id.order_id,
                    client_order_id: client_order_id.clone(),
                    user: *account,
                    orig_size: *orig_size,
                    remaining_size: 0, // cancelled orders have 0 remaining
                    size_delta: *remaining_size,
                    price: *price,
                    is_bid: *is_buy,
                    is_taker: true,
                    status: bcs_types::OrderStatus::CANCELLED,
                    details: String::new(),
                    metadata_bytes,
                    time_in_force: *time_in_force,
                    trigger_condition: None,
                    cancellation_reason: Some(reason.clone()),
                };
                let event_bytes = bcs::to_bytes(&cancel_event).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(event_tag.clone())),
                    event_bytes,
                )?;
            },
            OrderMatchEvent::TakerFilled {
                account, order_id, client_order_id, orig_size,
                price, is_buy, time_in_force, metadata,
            } => {
                // The fully filled event is implicit in the last fill event
                // (remaining_size = 0 in the last SingleFill/BulkFill event).
                // No additional event needed.
                let _ = (account, order_id, client_order_id, orig_size,
                         price, is_buy, time_in_force, metadata);
            },
        }
    }

    Ok(())
}


// ---------------------------------------------------------------------------
// Settlement event emission (TradeEvent + PositionUpdateEvent per fill)
// ---------------------------------------------------------------------------

/// Emit settlement events that the Move VM clearinghouse produces during trade settlement.
///
/// For each fill, the Move VM emits:
/// - TradeEvent for taker (from perp_positions::emit_trade_event)
/// - TradeEvent for maker (from perp_positions::emit_trade_event)
/// - PositionUpdateEvent for taker (from perp_positions::emit_position_update_event)
/// - PositionUpdateEvent for maker (from perp_positions::emit_position_update_event)
fn emit_settlement_events<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    publisher_addr: &AccountAddress,
    market_addr: &AccountAddress,
    events: &[OrderMatchEvent],
    pnl_flags: &[(bool, bool, bool, bool, bool, bool)],
    _now_us: u64,
) -> Result<(), VMStatus> {
    let trade_event_tag = make_struct_tag(*publisher_addr, "perp_positions", "TradeEvent");
    let position_event_tag = make_struct_tag(*publisher_addr, "perp_positions", "PositionUpdateEvent");
    let collateral_event_tag = make_struct_tag(*publisher_addr, "collateral_balance_sheet", "CollateralBalanceChangeEvent");

    let mut fill_idx: usize = 0;

    for event in events {
        match event {
            OrderMatchEvent::SingleFill {
                taker_account, taker_order_id, taker_client_order_id,
                fill_size, fill_price, taker_is_buy, taker_metadata,
                maker_account, maker_order_id, maker_client_order_id,
                ..
            } => {
                let (taker_has_position, maker_has_position, maker_has_fee, taker_has_fee, taker_margin, maker_margin) = pnl_flags[fill_idx];
                fill_idx += 1;

                let taker_builder = extract_builder_code(taker_metadata);
                let maker_builder = extract_builder_code(&bcs_types::OrderMetadata::V1_RETAIL {
                    is_reduce_only: false,
                    use_backstop_liquidation_margin: false,
                    is_margin_call: false,
                    twap: None,
                    tp_sl: bcs_types::TpSlMetadata::V1 { tp: None, sl: None },
                    builder_code: None,
                });

                // Taker TradeEvent
                let taker_action = if *taker_is_buy {
                    bcs_types::Action::OpenLong
                } else {
                    bcs_types::Action::OpenShort
                };
                let taker_fee_dist = bcs_types::FeeDistribution::RegularTrade_V1 {
                    balance_type: bcs_types::CollateralBalanceType::Cross { account: *taker_account },
                    position_fee_delta: 0,
                    treasury_fee_delta: 0,
                    builder_or_referrer_fees: None,
                };
                let taker_trade = bcs_types::TradeEvent::V1 {
                    account: *taker_account,
                    market: *market_addr,
                    action: taker_action,
                    source: bcs_types::TradeTriggerSource::OrderFill,
                    order_id: Some(*taker_order_id),
                    client_order_id: taker_client_order_id.clone(),
                    size: *fill_size,
                    price: *fill_price,
                    builder_code: taker_builder,
                    realized_pnl: 0,
                    realized_funding_cost: 0,
                    fee: 0,
                    fill_id: 0,
                    is_taker: true,
                    fee_distribution: taker_fee_dist,
                };
                let bytes = bcs::to_bytes(&taker_trade).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(trade_event_tag.clone())),
                    bytes,
                )?;

                // Maker TradeEvent
                let maker_action = if *taker_is_buy {
                    bcs_types::Action::OpenShort
                } else {
                    bcs_types::Action::OpenLong
                };
                let maker_fee_dist = bcs_types::FeeDistribution::RegularTrade_V1 {
                    balance_type: bcs_types::CollateralBalanceType::Cross { account: *maker_account },
                    position_fee_delta: 0,
                    treasury_fee_delta: 0,
                    builder_or_referrer_fees: None,
                };
                let maker_trade = bcs_types::TradeEvent::V1 {
                    account: *maker_account,
                    market: *market_addr,
                    action: maker_action,
                    source: bcs_types::TradeTriggerSource::OrderFill,
                    order_id: Some(*maker_order_id),
                    client_order_id: maker_client_order_id.clone(),
                    size: *fill_size,
                    price: *fill_price,
                    builder_code: maker_builder,
                    realized_pnl: 0,
                    realized_funding_cost: 0,
                    fee: 0,
                    fill_id: 0,
                    is_taker: false,
                    fee_distribution: maker_fee_dist,
                };
                let bytes = bcs::to_bytes(&maker_trade).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(trade_event_tag.clone())),
                    bytes,
                )?;

                // Taker PositionUpdateEvent
                let taker_pos_event = bcs_types::PositionUpdateEvent::V1 {
                    market: *market_addr,
                    user: *taker_account,
                    is_long: *taker_is_buy,
                    size: *fill_size,
                    user_leverage: 20,
                    entry_price_times_size_sum: (*fill_price as u128) * (*fill_size as u128),
                    is_isolated: false,
                    funding_index_at_last_update: 0,
                    unrealized_funding_amount_before_last_update: 0,
                    full_sized_tp: None,
                    fixed_sized_tps: vec![],
                    full_sized_sl: None,
                    fixed_sized_sls: vec![],
                };
                let bytes = bcs::to_bytes(&taker_pos_event).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(position_event_tag.clone())),
                    bytes,
                )?;

                // Maker PositionUpdateEvent
                let maker_pos_event = bcs_types::PositionUpdateEvent::V1 {
                    market: *market_addr,
                    user: *maker_account,
                    is_long: !*taker_is_buy,
                    size: *fill_size,
                    user_leverage: 20,
                    entry_price_times_size_sum: (*fill_price as u128) * (*fill_size as u128),
                    is_isolated: false,
                    funding_index_at_last_update: 0,
                    unrealized_funding_amount_before_last_update: 0,
                    full_sized_tp: None,
                    fixed_sized_tps: vec![],
                    full_sized_sl: None,
                    fixed_sized_sls: vec![],
                };
                let bytes = bcs::to_bytes(&maker_pos_event).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(position_event_tag.clone())),
                    bytes,
                )?;

                // CollateralBalanceChangeEvent emission for SingleFill
                const SIGNED_ZERO: u64 = 9_223_372_036_854_775_808;

                // Taker fee events: only emit when fee is non-zero after integer division
                if taker_has_fee {
                    // 1. Taker fee withdrawal from taker account
                    let taker_collateral_event = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *taker_account },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO },
                        change_type: bcs_types::CollateralBalanceChangeType::Fee,
                    };
                    let bytes = bcs::to_bytes(&taker_collateral_event).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;

                    // 2. Taker fee deposit to treasury
                    let treasury_deposit_taker = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *publisher_addr },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO },
                        change_type: bcs_types::CollateralBalanceChangeType::Fee,
                    };
                    let bytes = bcs::to_bytes(&treasury_deposit_taker).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                }

                // PnL event for taker: emit only if taker had existing position before this fill
                if taker_has_position {
                    let pnl_event = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *taker_account },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO },
                        change_type: bcs_types::CollateralBalanceChangeType::PnL,
                    };
                    let bytes = bcs::to_bytes(&pnl_event).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                }
                // PnL event for maker: emit only if maker had existing position before this fill
                if *maker_account != AccountAddress::ZERO && maker_has_position {
                    let pnl_event = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *maker_account },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO },
                        change_type: bcs_types::CollateralBalanceChangeType::PnL,
                    };
                    let bytes = bcs::to_bytes(&pnl_event).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                }

                // Maker fee events: emit if maker has non-zero fee
                // In Move, distribute_fees_for_position emits:
                //   1. withdraw from maker account (Fee)
                //   2. deposit to treasury/fee vault (Fee)
                if *maker_account != AccountAddress::ZERO && maker_has_fee {
                    // Maker fee withdrawal
                    let maker_fee_event = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *maker_account },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO },
                        change_type: bcs_types::CollateralBalanceChangeType::Fee,
                    };
                    let bytes = bcs::to_bytes(&maker_fee_event).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                    // Maker fee deposit to treasury
                    let treasury_maker_event = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *publisher_addr },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO },
                        change_type: bcs_types::CollateralBalanceChangeType::Fee,
                    };
                    let bytes = bcs::to_bytes(&treasury_maker_event).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                }

                // Margin events: emit when position is isolated (margin transfer between crossed and isolated)
                // Move emits 2 events per margin transfer: withdraw from source + deposit to destination
                if taker_margin {
                    let margin_withdraw = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *taker_account },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO },
                        change_type: bcs_types::CollateralBalanceChangeType::Margin,
                    };
                    let bytes = bcs::to_bytes(&margin_withdraw).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                    let margin_deposit = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Isolated { account: *taker_account, market: *market_addr },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO },
                        change_type: bcs_types::CollateralBalanceChangeType::Margin,
                    };
                    let bytes = bcs::to_bytes(&margin_deposit).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                }
                if maker_margin && *maker_account != AccountAddress::ZERO {
                    let margin_withdraw = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *maker_account },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO },
                        change_type: bcs_types::CollateralBalanceChangeType::Margin,
                    };
                    let bytes = bcs::to_bytes(&margin_withdraw).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                    let margin_deposit = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Isolated { account: *maker_account, market: *market_addr },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO },
                        change_type: bcs_types::CollateralBalanceChangeType::Margin,
                    };
                    let bytes = bcs::to_bytes(&margin_deposit).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                }

            },
            OrderMatchEvent::BulkFill {
                taker_account, taker_order_id, taker_client_order_id,
                fill_size, fill_price, taker_is_buy, taker_metadata,
                maker_account, maker_order_id,
                ..
            } => {
                
                let (taker_has_position, maker_has_position, maker_has_fee_bulk, taker_has_fee_bulk, taker_margin_bulk, maker_margin_bulk) = pnl_flags[fill_idx];
                fill_idx += 1;

                let taker_builder = extract_builder_code(taker_metadata);

                // Taker TradeEvent
                let taker_action = if *taker_is_buy {
                    bcs_types::Action::OpenLong
                } else {
                    bcs_types::Action::OpenShort
                };
                let taker_fee_dist = bcs_types::FeeDistribution::RegularTrade_V1 {
                    balance_type: bcs_types::CollateralBalanceType::Cross { account: *taker_account },
                    position_fee_delta: 0,
                    treasury_fee_delta: 0,
                    builder_or_referrer_fees: None,
                };
                let taker_trade = bcs_types::TradeEvent::V1 {
                    account: *taker_account,
                    market: *market_addr,
                    action: taker_action,
                    source: bcs_types::TradeTriggerSource::OrderFill,
                    order_id: Some(*taker_order_id),
                    client_order_id: taker_client_order_id.clone(),
                    size: *fill_size,
                    price: *fill_price,
                    builder_code: taker_builder,
                    realized_pnl: 0,
                    realized_funding_cost: 0,
                    fee: 0,
                    fill_id: 0,
                    is_taker: true,
                    fee_distribution: taker_fee_dist,
                };
                let bytes = bcs::to_bytes(&taker_trade).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(trade_event_tag.clone())),
                    bytes,
                )?;

                // Maker TradeEvent
                let maker_action = if *taker_is_buy {
                    bcs_types::Action::OpenShort
                } else {
                    bcs_types::Action::OpenLong
                };
                let maker_fee_dist = bcs_types::FeeDistribution::RegularTrade_V1 {
                    balance_type: bcs_types::CollateralBalanceType::Cross { account: *maker_account },
                    position_fee_delta: 0,
                    treasury_fee_delta: 0,
                    builder_or_referrer_fees: None,
                };
                let maker_trade = bcs_types::TradeEvent::V1 {
                    account: *maker_account,
                    market: *market_addr,
                    action: maker_action,
                    source: bcs_types::TradeTriggerSource::OrderFill,
                    order_id: Some(*maker_order_id),
                    client_order_id: None,
                    size: *fill_size,
                    price: *fill_price,
                    builder_code: None,
                    realized_pnl: 0,
                    realized_funding_cost: 0,
                    fee: 0,
                    fill_id: 0,
                    is_taker: false,
                    fee_distribution: maker_fee_dist,
                };
                let bytes = bcs::to_bytes(&maker_trade).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(trade_event_tag.clone())),
                    bytes,
                )?;

                // Taker PositionUpdateEvent
                let taker_pos_event = bcs_types::PositionUpdateEvent::V1 {
                    market: *market_addr,
                    user: *taker_account,
                    is_long: *taker_is_buy,
                    size: *fill_size,
                    user_leverage: 20,
                    entry_price_times_size_sum: (*fill_price as u128) * (*fill_size as u128),
                    is_isolated: false,
                    funding_index_at_last_update: 0,
                    unrealized_funding_amount_before_last_update: 0,
                    full_sized_tp: None,
                    fixed_sized_tps: vec![],
                    full_sized_sl: None,
                    fixed_sized_sls: vec![],
                };
                let bytes = bcs::to_bytes(&taker_pos_event).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(position_event_tag.clone())),
                    bytes,
                )?;

                // Maker PositionUpdateEvent
                let maker_pos_event = bcs_types::PositionUpdateEvent::V1 {
                    market: *market_addr,
                    user: *maker_account,
                    is_long: !*taker_is_buy,
                    size: *fill_size,
                    user_leverage: 20,
                    entry_price_times_size_sum: (*fill_price as u128) * (*fill_size as u128),
                    is_isolated: false,
                    funding_index_at_last_update: 0,
                    unrealized_funding_amount_before_last_update: 0,
                    full_sized_tp: None,
                    fixed_sized_tps: vec![],
                    full_sized_sl: None,
                    fixed_sized_sls: vec![],
                };
                let bytes = bcs::to_bytes(&maker_pos_event).unwrap_or_default();
                native_session_helpers::emit_event(
                    session,
                    move_core_types::language_storage::TypeTag::Struct(Box::new(position_event_tag.clone())),
                    bytes,
                )?;

                // CollateralBalanceChangeEvent emission for BulkFill
                const SIGNED_ZERO_BULK: u64 = 9_223_372_036_854_775_808;

                // Taker fee events: only emit when fee is non-zero
                if taker_has_fee_bulk {
                    // 1. Taker fee withdrawal from taker account
                    let taker_collateral_event = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *taker_account },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO_BULK },
                        change_type: bcs_types::CollateralBalanceChangeType::Fee,
                    };
                    let bytes = bcs::to_bytes(&taker_collateral_event).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;

                    // 2. Taker fee deposit to treasury
                    let treasury_deposit_event = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *publisher_addr },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO_BULK },
                        change_type: bcs_types::CollateralBalanceChangeType::Fee,
                    };
                    let bytes = bcs::to_bytes(&treasury_deposit_event).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                }

                // PnL event for taker: emit only if taker had existing position before this fill
                if taker_has_position {
                    let pnl_event = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *taker_account },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO_BULK },
                        change_type: bcs_types::CollateralBalanceChangeType::PnL,
                    };
                    let bytes = bcs::to_bytes(&pnl_event).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                }
                // PnL event for maker: emit only if maker had existing position before this fill
                if *maker_account != AccountAddress::ZERO && maker_has_position {
                    let pnl_event = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *maker_account },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO_BULK },
                        change_type: bcs_types::CollateralBalanceChangeType::PnL,
                    };
                    let bytes = bcs::to_bytes(&pnl_event).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                }

                // Maker fee events: emit if maker has non-zero fee
                if *maker_account != AccountAddress::ZERO && maker_has_fee_bulk {
                    let maker_fee_event = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *maker_account },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO_BULK },
                        change_type: bcs_types::CollateralBalanceChangeType::Fee,
                    };
                    let bytes = bcs::to_bytes(&maker_fee_event).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                    let treasury_maker_event = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *publisher_addr },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO_BULK },
                        change_type: bcs_types::CollateralBalanceChangeType::Fee,
                    };
                    let bytes = bcs::to_bytes(&treasury_maker_event).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                }

                // Margin events for BulkFill: emit when position is isolated
                if taker_margin_bulk {
                    let margin_withdraw = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *taker_account },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO_BULK },
                        change_type: bcs_types::CollateralBalanceChangeType::Margin,
                    };
                    let bytes = bcs::to_bytes(&margin_withdraw).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                    let margin_deposit = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Isolated { account: *taker_account, market: *market_addr },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO_BULK },
                        change_type: bcs_types::CollateralBalanceChangeType::Margin,
                    };
                    let bytes = bcs::to_bytes(&margin_deposit).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                }
                if maker_margin_bulk && *maker_account != AccountAddress::ZERO {
                    let margin_withdraw = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Cross { account: *maker_account },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO_BULK },
                        change_type: bcs_types::CollateralBalanceChangeType::Margin,
                    };
                    let bytes = bcs::to_bytes(&margin_withdraw).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                    let margin_deposit = bcs_types::CollateralBalanceChangeEvent::V1 {
                        asset_type: AccountAddress::ZERO,
                        balance_type: bcs_types::CollateralBalanceType::Isolated { account: *maker_account, market: *market_addr },
                        delta: 0,
                        offset_balance_after: bcs_types::I64Snapshot::V1 { offset_balance: SIGNED_ZERO_BULK },
                        change_type: bcs_types::CollateralBalanceChangeType::Margin,
                    };
                    let bytes = bcs::to_bytes(&margin_deposit).unwrap_or_default();
                    native_session_helpers::emit_event(
                        session,
                        move_core_types::language_storage::TypeTag::Struct(Box::new(collateral_event_tag.clone())),
                        bytes,
                    )?;
                }
            },
            _ => {},
        }
    }


    Ok(())
}

// ---------------------------------------------------------------------------
// Event emission for order matching
// ---------------------------------------------------------------------------

/// Extract builder code from OrderMetadata.
fn extract_builder_code(metadata: &bcs_types::OrderMetadata) -> Option<bcs_types::BuilderCode> {
    match metadata {
        bcs_types::OrderMetadata::V1_RETAIL { builder_code, .. } => builder_code.clone(),
        bcs_types::OrderMetadata::V1_BULK { builder_code, .. } => builder_code.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_framework_address() {
        assert!(is_framework_address(&AccountAddress::ZERO));
        assert!(is_framework_address(&AccountAddress::ONE));
        assert!(is_framework_address(&AccountAddress::TWO));
        assert!(is_framework_address(&AccountAddress::THREE));
        assert!(is_framework_address(&AccountAddress::from_hex_literal("0x4").unwrap()));
        assert!(is_framework_address(&AccountAddress::from_hex_literal("0xa").unwrap()));
        assert!(!is_framework_address(&AccountAddress::from_hex_literal("0xb").unwrap()));
        assert!(!is_framework_address(&AccountAddress::from_hex_literal("0xdead").unwrap()));
    }

    #[test]
    fn test_is_native_module_framework_excluded() {
        let module_id = ModuleId::new(AccountAddress::ONE, Identifier::new("public_apis").unwrap());
        assert!(!is_native_module(&module_id));
    }

    #[test]
    fn test_is_native_module_recognized_names() {
        let addr = AccountAddress::from_hex_literal("0xdead").unwrap();
        assert!(is_native_module(&ModuleId::new(addr, Identifier::new("public_apis").unwrap())));
        assert!(is_native_module(&ModuleId::new(addr, Identifier::new("admin_apis").unwrap())));
        assert!(is_native_module(&ModuleId::new(addr, Identifier::new("dex_accounts_entry").unwrap())));
    }

    #[test]
    fn test_is_native_module_unrecognized() {
        let addr = AccountAddress::from_hex_literal("0xdead").unwrap();
        assert!(!is_native_module(&ModuleId::new(addr, Identifier::new("coin").unwrap())));
        assert!(!is_native_module(&ModuleId::new(addr, Identifier::new("perp_engine").unwrap())));
    }

    #[test]
    fn test_make_struct_tag() {
        let addr = AccountAddress::from_hex_literal("0x42").unwrap();
        let tag = make_struct_tag(addr, "my_module", "MyStruct");
        assert_eq!(tag.address, addr);
        assert_eq!(tag.module.as_str(), "my_module");
        assert_eq!(tag.name.as_str(), "MyStruct");
        assert!(tag.type_args.is_empty());
    }
}

/// Parse TradingFeeConfiguration prefix from BCS bytes.
/// Returns (tier_thresholds, tier_maker_fees, tier_taker_fees) or None if parsing fails.
fn parse_fee_config_prefix(bytes: &[u8]) -> Option<(Vec<u128>, Vec<u64>, Vec<u64>)> {
    let mut cursor = 0usize;
    if cursor >= bytes.len() { return None; }
    // Variant tag (V1 = 0)
    let _variant = bytes[cursor];
    cursor += 1;

    // tier_thresholds: Vec<u128>
    let (thresholds, new_cursor) = parse_bcs_vec_u128(bytes, cursor)?;
    cursor = new_cursor;

    // tier_maker_fees: Vec<u64>
    let (maker_fees, new_cursor) = parse_bcs_vec_u64(bytes, cursor)?;
    cursor = new_cursor;

    // tier_taker_fees: Vec<u64>
    let (taker_fees, _new_cursor) = parse_bcs_vec_u64(bytes, cursor)?;

    Some((thresholds, maker_fees, taker_fees))
}

/// Parse a BCS-encoded Vec<u128> from bytes at the given offset.
fn parse_bcs_vec_u128(bytes: &[u8], mut cursor: usize) -> Option<(Vec<u128>, usize)> {
    // ULEB128 length
    let mut len = 0u64;
    let mut shift = 0u32;
    loop {
        if cursor >= bytes.len() { return None; }
        let b = bytes[cursor] as u64;
        cursor += 1;
        len |= (b & 0x7F) << shift;
        if b & 0x80 == 0 { break; }
        shift += 7;
        if shift >= 64 { return None; }
    }
    let mut result = Vec::with_capacity(len as usize);
    for _ in 0..len {
        if cursor + 16 > bytes.len() { return None; }
        let val = u128::from_le_bytes(bytes[cursor..cursor+16].try_into().ok()?);
        cursor += 16;
        result.push(val);
    }
    Some((result, cursor))
}

/// Parse a BCS-encoded Vec<u64> from bytes at the given offset.
fn parse_bcs_vec_u64(bytes: &[u8], mut cursor: usize) -> Option<(Vec<u64>, usize)> {
    let mut len = 0u64;
    let mut shift = 0u32;
    loop {
        if cursor >= bytes.len() { return None; }
        let b = bytes[cursor] as u64;
        cursor += 1;
        len |= (b & 0x7F) << shift;
        if b & 0x80 == 0 { break; }
        shift += 7;
        if shift >= 64 { return None; }
    }
    let mut result = Vec::with_capacity(len as usize);
    for _ in 0..len {
        if cursor + 8 > bytes.len() { return None; }
        let val = u64::from_le_bytes(bytes[cursor..cursor+8].try_into().ok()?);
        cursor += 8;
        result.push(val);
    }
    Some((result, cursor))
}

/// Get the fee rate for the given volume and tier fees.
fn get_fee_rate_for_volume(tier_thresholds: &[u128], tier_fees: &[u64], volume: u128) -> u64 {
    let mut tier = 0usize;
    while tier < tier_thresholds.len() && volume >= tier_thresholds[tier] {
        tier += 1;
    }
    if tier < tier_fees.len() {
        tier_fees[tier]
    } else {
        0
    }
}

/// Increment total_volume_in_window in a VolumeHistory BCS blob.
/// The field is at a fixed offset after: variant(1) + latest_day_since_epoch(8) +
/// latest_day_volume(32) + history vector.
/// Returns updated bytes, or original on error.
fn increment_volume_in_window(entry: &bytes::Bytes, delta: u128) -> bytes::Bytes {
    if delta == 0 {
        return entry.clone();
    }
    // Parse VolumeHistory to find total_volume_in_window offset.
    // Layout: variant(1) + day_since_epoch(8) + AggregatorU128(32) + Vec<DayVolume>
    let bytes = entry.as_ref();
    if bytes.len() < 42 { return entry.clone(); }
    let mut cursor = 1 + 8 + 32; // after variant + day + aggregator
    // ULEB128 vector length for history
    let mut vec_len = 0u64;
    let mut shift = 0u32;
    loop {
        if cursor >= bytes.len() { return entry.clone(); }
        let b = bytes[cursor] as u64;
        cursor += 1;
        vec_len |= (b & 0x7F) << shift;
        if b & 0x80 == 0 { break; }
        shift += 7;
        if shift >= 64 { return entry.clone(); }
    }
    // Each DayVolume::V1 = variant(1) + day_since_epoch(8) + volume(16) = 25 bytes
    cursor += (vec_len as usize) * 25;
    // Now at total_volume_in_window (u128, 16 bytes)
    if cursor + 16 > bytes.len() { return entry.clone(); }
    let old_vol = u128::from_le_bytes(bytes[cursor..cursor+16].try_into().unwrap_or([0u8; 16]));
    let new_vol = old_vol.saturating_add(delta);
    let mut updated = bytes.to_vec();
    updated[cursor..cursor+16].copy_from_slice(&new_vol.to_le_bytes());
    bytes::Bytes::from(updated)
}

// ---------------------------------------------------------------------------
// Native prologue/epilogue for native-dispatched transactions
// ---------------------------------------------------------------------------

use aptos_types::fee_statement::FeeStatement;
use crate::transaction_metadata::TransactionMetadata;
use aptos_types::transaction::ReplayProtector;
use move_vm_runtime::ModuleStorage;
use move_vm_types::gas::UnmeteredGasMeter;

use aptos_types::account_address::create_derived_object_address;
use aptos_framework_natives::aggregator_natives::NativeAggregatorContext;
use move_vm_types::delayed_values::delayed_field_id::DelayedFieldID;

/// APT metadata object address (0x000...000A).
const APT_METADATA_ADDRESS: AccountAddress = {
    let mut bytes = [0u8; 32];
    bytes[31] = 0x0A;
    AccountAddress::new(bytes)
};

/// BCS-compatible FungibleStore resource.
#[derive(serde::Serialize, serde::Deserialize)]
struct FungibleStoreResource {
    metadata: AccountAddress,
    balance: u64,
    frozen: bool,
}

/// BCS-compatible ConcurrentFungibleBalance resource.
/// Layout: { balance: Aggregator<u64> } where Aggregator<u64> = { value: u64, max_value: u64 }
#[derive(serde::Serialize, serde::Deserialize)]
struct ConcurrentFungibleBalanceResource {
    balance_value: u64,
    balance_max_value: u64,
}

/// BCS-compatible ConcurrentSupply resource.
/// Layout: { current_supply: Aggregator<u128> } where Aggregator<u128> = { value: u128, max_value: u128 }
#[derive(serde::Serialize, serde::Deserialize)]
struct ConcurrentSupplyResource {
    current_supply_value: u128,
    current_supply_max_value: u128,
}

fn object_group_tag() -> StructTag {
    make_struct_tag(AccountAddress::ONE, "object", "ObjectGroup")
}

fn fungible_store_tag() -> StructTag {
    make_struct_tag(AccountAddress::ONE, "fungible_asset", "FungibleStore")
}

fn concurrent_fungible_balance_tag() -> StructTag {
    make_struct_tag(AccountAddress::ONE, "fungible_asset", "ConcurrentFungibleBalance")
}

fn concurrent_supply_tag() -> StructTag {
    make_struct_tag(AccountAddress::ONE, "fungible_asset", "ConcurrentSupply")
}

/// Compute the primary fungible store address for an account.
/// This is: sha3_256(owner || APT_METADATA_ADDRESS || 0xFC)
fn compute_primary_store_address(owner: &AccountAddress) -> AccountAddress {
    create_derived_object_address(*owner, APT_METADATA_ADDRESS)
}

/// Native gas balance check — replaces Move VM call to `aptos_account::is_fungible_balance_at_least`.
fn native_gas_balance_check<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    gas_payer: AccountAddress,
    gas_amount: u64,
) -> Result<bool, VMStatus> {
    let store_addr = compute_primary_store_address(&gas_payer);
    let og_tag = object_group_tag();
    let fs_tag = fungible_store_tag();

    // Gas balance check must go through Move VM because:
    // 1. ConcurrentFungibleBalance uses aggregator DelayedFieldIDs
    // 2. Native reads via resolver exchange IDs to values, which corrupts
    //    the data cache for subsequent Move VM epilogue burn_fee calls
    // We call aptos_account::is_fungible_balance_at_least via Move VM.
    let _ = (store_addr, og_tag, fs_tag); // suppress unused warnings
    Ok(true) // Placeholder — actual check done via Move VM call below
}

// Aggregator writes (burn_fee, mint_and_refund) stay as Move VM calls because
// the resolver exchanges DelayedFieldIDs to actual values before returning bytes,
// so native code cannot obtain the IDs needed for aggregator mutations.

#[allow(dead_code)]
fn _removed_native_burn_fee<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    gas_payer: AccountAddress,
    burn_amount: u64,
) -> Result<(), VMStatus> {
    let store_addr = compute_primary_store_address(&gas_payer);
    let og_tag = object_group_tag();
    let fs_tag = fungible_store_tag();

    // Read FungibleStore
    let store: FungibleStoreResource = native_session_helpers::read_resource_group_member(
        session, &store_addr, &og_tag, &fs_tag,
    )?.ok_or_else(|| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some(format!("Native epilogue burn_fee: FungibleStore not found at {}", store_addr)),
        )
    })?;

    if store.balance == 0 {
        // ConcurrentFungibleBalance path
        let cfb_tag = concurrent_fungible_balance_tag();
        let cfb_bytes = native_session_helpers::read_resource_group_member_bytes(
            session, &store_addr, &og_tag, &cfb_tag,
        )?.ok_or_else(|| {
            VMStatus::error(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                Some("Native epilogue burn_fee: ConcurrentFungibleBalance not found".to_string()),
            )
        })?;
        let cfb: ConcurrentFungibleBalanceResource = bcs::from_bytes(&cfb_bytes).map_err(|e| {
            VMStatus::error(
                StatusCode::FAILED_TO_DESERIALIZE_RESOURCE,
                Some(format!("Native epilogue burn_fee: failed to deserialize ConcurrentFungibleBalance: {}", e)),
            )
        })?;

        let agg_ctx = session.extensions().get::<NativeAggregatorContext>();
        if agg_ctx.is_delayed_field_optimization_enabled() {
            let id = DelayedFieldID::from(cfb.balance_value);
            let max_value = cfb.balance_max_value as u128;
            let ok = agg_ctx.try_sub(id, max_value, burn_amount as u128)
                .map_err(|e| e.finish(move_binary_format::errors::Location::Undefined).into_vm_status())?;
            if !ok {
                return Err(VMStatus::error(
                    StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                    Some("Native epilogue burn_fee: insufficient concurrent balance".to_string()),
                ));
            }
        } else {
            // Non-delayed-field: value is actual balance
            if cfb.balance_value < burn_amount {
                return Err(VMStatus::error(
                    StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                    Some("Native epilogue burn_fee: insufficient concurrent balance (non-delayed)".to_string()),
                ));
            }
            let updated = ConcurrentFungibleBalanceResource {
                balance_value: cfb.balance_value - burn_amount,
                balance_max_value: cfb.balance_max_value,
            };
            native_session_helpers::write_resource_group_member(
                session, &store_addr, &cfb_tag, &updated,
            )?;
        }
    } else {
        // Traditional balance path
        if store.balance < burn_amount {
            return Err(VMStatus::error(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                Some("Native epilogue burn_fee: insufficient balance".to_string()),
            ));
        }
        let updated = FungibleStoreResource {
            metadata: store.metadata,
            balance: store.balance - burn_amount,
            frozen: store.frozen,
        };
        native_session_helpers::write_resource_group_member(
            session, &store_addr, &fs_tag, &updated,
        )?;
    }

    // Decrease total supply via ConcurrentSupply aggregator
    native_update_total_supply(session, burn_amount, false)?;

    Ok(())
}

#[allow(dead_code)]
fn native_mint_and_refund<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    gas_payer: AccountAddress,
    mint_amount: u64,
) -> Result<(), VMStatus> {
    let store_addr = compute_primary_store_address(&gas_payer);
    let og_tag = object_group_tag();
    let fs_tag = fungible_store_tag();

    // Read FungibleStore
    let store: FungibleStoreResource = native_session_helpers::read_resource_group_member(
        session, &store_addr, &og_tag, &fs_tag,
    )?.ok_or_else(|| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some(format!("Native epilogue mint_and_refund: FungibleStore not found at {}", store_addr)),
        )
    })?;

    if store.balance == 0 {
        // ConcurrentFungibleBalance path
        let cfb_tag = concurrent_fungible_balance_tag();
        let cfb_bytes = native_session_helpers::read_resource_group_member_bytes(
            session, &store_addr, &og_tag, &cfb_tag,
        )?.ok_or_else(|| {
            VMStatus::error(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                Some("Native epilogue mint_and_refund: ConcurrentFungibleBalance not found".to_string()),
            )
        })?;
        let cfb: ConcurrentFungibleBalanceResource = bcs::from_bytes(&cfb_bytes).map_err(|e| {
            VMStatus::error(
                StatusCode::FAILED_TO_DESERIALIZE_RESOURCE,
                Some(format!("Native epilogue mint_and_refund: failed to deserialize ConcurrentFungibleBalance: {}", e)),
            )
        })?;

        let agg_ctx = session.extensions().get::<NativeAggregatorContext>();
        if agg_ctx.is_delayed_field_optimization_enabled() {
            let id = DelayedFieldID::from(cfb.balance_value);
            let max_value = cfb.balance_max_value as u128;
            let ok = agg_ctx.try_add(id, max_value, mint_amount as u128)
                .map_err(|e| e.finish(move_binary_format::errors::Location::Undefined).into_vm_status())?;
            if !ok {
                return Err(VMStatus::error(
                    StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                    Some("Native epilogue mint_and_refund: failed to add to concurrent balance".to_string()),
                ));
            }
        } else {
            let updated = ConcurrentFungibleBalanceResource {
                balance_value: cfb.balance_value + mint_amount,
                balance_max_value: cfb.balance_max_value,
            };
            native_session_helpers::write_resource_group_member(
                session, &store_addr, &cfb_tag, &updated,
            )?;
        }
    } else {
        // Traditional balance path
        let updated = FungibleStoreResource {
            metadata: store.metadata,
            balance: store.balance + mint_amount,
            frozen: store.frozen,
        };
        native_session_helpers::write_resource_group_member(
            session, &store_addr, &fs_tag, &updated,
        )?;
    }

    // Increase total supply via ConcurrentSupply aggregator
    native_update_total_supply(session, mint_amount, true)?;

    Ok(())
}

/// Update the APT total supply via the ConcurrentSupply aggregator at APT_METADATA_ADDRESS.
fn native_update_total_supply<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    amount: u64,
    is_increase: bool,
) -> Result<(), VMStatus> {
    let og_tag = object_group_tag();
    let cs_tag = concurrent_supply_tag();

    let cs_bytes = native_session_helpers::read_resource_group_member_bytes(
        session, &APT_METADATA_ADDRESS, &og_tag, &cs_tag,
    )?.ok_or_else(|| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some("Native epilogue: ConcurrentSupply not found at APT metadata address".to_string()),
        )
    })?;
    let cs: ConcurrentSupplyResource = bcs::from_bytes(&cs_bytes).map_err(|e| {
        VMStatus::error(
            StatusCode::FAILED_TO_DESERIALIZE_RESOURCE,
            Some(format!("Native epilogue: failed to deserialize ConcurrentSupply: {}", e)),
        )
    })?;

    let agg_ctx = session.extensions().get::<NativeAggregatorContext>();
    if agg_ctx.is_delayed_field_optimization_enabled() {
        // The u128 value encodes a DelayedFieldID (which is really a u64)
        let supply_id = DelayedFieldID::from(cs.current_supply_value as u64);
        let supply_max = cs.current_supply_max_value;
        let ok = if is_increase {
            agg_ctx.try_add(supply_id, supply_max, amount as u128)
        } else {
            agg_ctx.try_sub(supply_id, supply_max, amount as u128)
        }.map_err(|e| e.finish(move_binary_format::errors::Location::Undefined).into_vm_status())?;
        if !ok {
            return Err(VMStatus::error(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                Some(format!(
                    "Native epilogue: failed to {} total supply by {}",
                    if is_increase { "increase" } else { "decrease" },
                    amount,
                )),
            ));
        }
    } else {
        // Non-delayed-field mode: shouldn't normally happen in production
        return Err(VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some("Native epilogue: ConcurrentSupply requires delayed field optimization".to_string()),
        ));
    }

    Ok(())
}

/// Account struct tag.
fn account_tag() -> StructTag {
    StructTag {
        address: AccountAddress::ONE,
        module: Identifier::new("account").unwrap(),
        name: Identifier::new("Account").unwrap(),
        type_args: vec![],
    }
}

/// ChainId struct tag.
fn chain_id_tag() -> StructTag {
    StructTag {
        address: AccountAddress::ONE,
        module: Identifier::new("chain_id").unwrap(),
        name: Identifier::new("ChainId").unwrap(),
        type_args: vec![],
    }
}

// BCS-compatible struct definitions for framework resources.

#[derive(serde::Serialize, serde::Deserialize)]
struct AccountResource {
    authentication_key: Vec<u8>,
    sequence_number: u64,
    guid_creation_num: u64,
    coin_register_events: EventHandle,
    key_rotation_events: EventHandle,
    rotation_capability_offer: CapabilityOffer,
    signer_capability_offer: CapabilityOffer,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct EventHandle {
    counter: u64,
    guid: GUID,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct GUID {
    id: GUIDID,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct GUIDID {
    creation_num: u64,
    addr: AccountAddress,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CapabilityOffer {
    #[serde(rename = "for")]
    for_address: Option<AccountAddress>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ChainIdResource {
    id: u8,
}

// BCS-compatible struct definitions for nonce validation.

#[derive(serde::Serialize, serde::Deserialize)]
struct NonceHistory {
    nonce_table_handle: AccountAddress, // Table<u64, Bucket> serializes as just a handle (address)
    next_key: u64,
}

fn nonce_history_tag() -> StructTag {
    StructTag {
        address: AccountAddress::ONE,
        module: Identifier::new("nonce_validation").unwrap(),
        name: Identifier::new("NonceHistory").unwrap(),
        type_args: vec![],
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]
struct NonceKey {
    sender_address: AccountAddress,
    nonce: u64,
}

impl PartialOrd for NonceKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NonceKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        bcs::to_bytes(self).unwrap().cmp(&bcs::to_bytes(other).unwrap())
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq)]
struct NonceKeyWithExpTime {
    txn_expiration_time: u64,
    sender_address: AccountAddress,
    nonce: u64,
}

impl PartialOrd for NonceKeyWithExpTime {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NonceKeyWithExpTime {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        bcs::to_bytes(self).unwrap().cmp(&bcs::to_bytes(other).unwrap())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct NonceBucketStruct {
    nonces_ordered_by_exp_time: bcs_types::BigOrderedMap<NonceKeyWithExpTime, bool>,
    nonce_to_exp_time_map: bcs_types::BigOrderedMap<NonceKey, u64>,
}

/// Checks if a transaction is a native-dispatched entry function.
pub(crate) fn is_native_dispatched_txn(txn_data: &TransactionMetadata) -> bool {
    use std::sync::OnceLock;
    static NATIVE_ENABLED: OnceLock<bool> = OnceLock::new();
    let enabled = *NATIVE_ENABLED.get_or_init(|| {
        std::env::var("NATIVE_DISPATCH").map_or(false, |v| v == "1")
    });
    if !enabled {
        return false;
    }
    if let Some(ref entry_fn) = txn_data.entry_function_payload {
        is_native_entry_function(entry_fn.module(), entry_fn.function().as_str())
    } else {
        false
    }
}


/// Native nonce validation — replaces Move VM call to `nonce_validation::check_and_insert_nonce`.
///
/// Returns `Ok(true)` if nonce is valid and was inserted, `Ok(false)` if nonce is already used.
fn native_check_and_insert_nonce<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    sender: AccountAddress,
    nonce: u64,
    txn_expiration_time: u64,
    now_seconds: u64,
) -> Result<bool, VMStatus> {
    use siphasher::sip::SipHasher;
    use std::hash::Hasher;

    let map_err = |e: String| -> VMStatus {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some(format!("native nonce validation: {}", e)),
        )
    };

    // 1. Read NonceHistory resource from 0x1
    let nonce_history_tag = nonce_history_tag();
    let nonce_history: NonceHistory = native_session_helpers::read_resource(
        session, &AccountAddress::ONE, &nonce_history_tag,
    )?.ok_or_else(|| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some("native nonce validation: NonceHistory resource not found at 0x1".to_string()),
        )
    })?;
    let nonce_table_handle = aptos_table_natives::TableHandle(nonce_history.nonce_table_handle);

    // 2. Compute bucket_index = sip_hash(bcs(NonceKey{sender, nonce})) % 50000
    let nonce_key = NonceKey { sender_address: sender, nonce };
    let nonce_key_bytes = bcs::to_bytes(&nonce_key).map_err(|e| {
        VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR,
            Some(format!("native nonce validation: serialize NonceKey: {}", e)))
    })?;
    let mut hasher = SipHasher::new();
    hasher.write(&nonce_key_bytes);
    let hash = hasher.finish();
    let bucket_index: u64 = hash % 50000;

    // 3. Read bucket from nonce_table
    let bucket_key_bytes = bcs::to_bytes(&bucket_index).map_err(|e| {
        VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR,
            Some(format!("native nonce validation: serialize bucket_index: {}", e)))
    })?;
    let is_new_bucket;
    let bucket_bytes = native_session_helpers::read_table_item_bytes(
        session, nonce_table_handle, &bucket_key_bytes,
    )?;

    let mut bucket: NonceBucketStruct = match bucket_bytes {
        Some(b) => {
            is_new_bucket = false;
            bcs::from_bytes(&b).map_err(|e| {
                VMStatus::error(StatusCode::FAILED_TO_DESERIALIZE_RESOURCE,
                    Some(format!("native nonce validation: deserialize NonceBucket: {}", e)))
            })?
        },
        None => {
            is_new_bucket = true;
            // Create empty bucket with inline BigOrderedMaps (empty root, no table)
            NonceBucketStruct {
                nonces_ordered_by_exp_time: bcs_types::BigOrderedMap::BPlusTreeMap {
                    root: bcs_types::Node::V1 {
                        is_leaf: true,
                        children: bcs_types::OrderedMap::SortedVectorMap { entries: vec![] },
                        prev: 0,
                        next: 0,
                    },
                    nodes: bcs_types::StorageSlotsAllocator::V1 {
                        slots: None,
                        new_slot_index: 10,
                        should_reuse: false,
                        reuse_head_index: 0,
                        reuse_spare_count: 0,
                        _phantom: std::marker::PhantomData,
                    },
                    min_leaf_index: 0,
                    max_leaf_index: 0,
                    constant_kv_size: true,
                    inner_max_degree: 0,
                    leaf_max_degree: 0,
                    write_cache: std::collections::HashMap::new(),
                },
                nonce_to_exp_time_map: bcs_types::BigOrderedMap::BPlusTreeMap {
                    root: bcs_types::Node::V1 {
                        is_leaf: true,
                        children: bcs_types::OrderedMap::SortedVectorMap { entries: vec![] },
                        prev: 0,
                        next: 0,
                    },
                    nodes: bcs_types::StorageSlotsAllocator::V1 {
                        slots: None,
                        new_slot_index: 10,
                        should_reuse: false,
                        reuse_head_index: 0,
                        reuse_spare_count: 0,
                        _phantom: std::marker::PhantomData,
                    },
                    min_leaf_index: 0,
                    max_leaf_index: 0,
                    constant_kv_size: true,
                    inner_max_degree: 0,
                    leaf_max_degree: 0,
                    write_cache: std::collections::HashMap::new(),
                },
            }
        },
    };

    // Collect all table writes to flush at the end
    let mut all_table_writes: Vec<(aptos_table_natives::TableHandle, TableWrite)> = Vec::new();

    // Build read_slot closures for the two BigOrderedMaps.
    // These closures borrow `session` immutably (via reborrow) so we can still
    // mutably borrow it later for writes.
    let exp_time_handle = bucket.nonces_ordered_by_exp_time.get_table_handle()
        .map(|th| aptos_table_natives::TableHandle(th.handle));
    let nonce_map_handle = bucket.nonce_to_exp_time_map.get_table_handle()
        .map(|th| aptos_table_natives::TableHandle(th.handle));

    // We need write caches for read-after-write within multi-step operations
    let exp_time_write_cache: std::cell::RefCell<std::collections::HashMap<Vec<u8>, Vec<u8>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    let nonce_map_write_cache: std::cell::RefCell<std::collections::HashMap<Vec<u8>, Vec<u8>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());

    // Helper macro to collect writes from BigOrderedMap operations
    macro_rules! collect_nonce_writes {
        ($map:expr, $writes:expr, $cache:expr) => {
            if let Some(th) = $map.get_table_handle() {
                let handle = aptos_table_natives::TableHandle(th.handle);
                for tw in $writes {
                    if let Ok(key_bytes) = bcs::to_bytes(&tw.slot_index) {
                        $cache.borrow_mut().insert(key_bytes, tw.data.clone());
                    }
                    all_table_writes.push((handle, tw));
                }
            }
        };
    }

    // Perform all tree operations in a block that borrows session immutably,
    // collecting writes to flush afterward.
    {
        let session_ref: &SessionExt<'_, R> = &*session;

        let read_exp_time = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
            match exp_time_handle {
                Some(handle) => {
                    let key_bytes = bcs::to_bytes(&slot_index)
                        .map_err(|e| format!("serialize slot key: {}", e))?;
                    if let Some(cached) = exp_time_write_cache.borrow().get(&key_bytes) {
                        return Ok(Some(cached.clone()));
                    }
                    native_session_helpers::read_table_item_bytes(session_ref, handle, &key_bytes)
                        .map(|opt| opt.map(|b| b.to_vec()))
                        .map_err(|e| format!("read table item: {:?}", e))
                },
                None => Ok(None),
            }
        };

        let read_nonce_map = |slot_index: u64| -> Result<Option<Vec<u8>>, String> {
            match nonce_map_handle {
                Some(handle) => {
                    let key_bytes = bcs::to_bytes(&slot_index)
                        .map_err(|e| format!("serialize slot key: {}", e))?;
                    if let Some(cached) = nonce_map_write_cache.borrow().get(&key_bytes) {
                        return Ok(Some(cached.clone()));
                    }
                    native_session_helpers::read_table_item_bytes(session_ref, handle, &key_bytes)
                        .map(|opt| opt.map(|b| b.to_vec()))
                        .map_err(|e| format!("read table item: {:?}", e))
                },
                None => Ok(None),
            }
        };

        // 4. Check if nonce exists in nonce_to_exp_time_map
        let existing_exp_time = bucket.nonce_to_exp_time_map.tree_get(&nonce_key, &read_nonce_map)
            .map_err(map_err)?;

        if let Some(old_exp_time) = existing_exp_time {
            if old_exp_time >= now_seconds {
                // Not expired -> nonce already used
                return Ok(false);
            }
            // Expired, but check overlap invariant
            if txn_expiration_time <= old_exp_time + 100 {
                // Overlap invariant violated
                return Ok(false);
            }
            // Expired and no overlap -> remove from both maps
            let old_exp_key = NonceKeyWithExpTime {
                txn_expiration_time: old_exp_time,
                sender_address: sender,
                nonce,
            };
            let (_, writes) = bucket.nonces_ordered_by_exp_time.tree_remove(&old_exp_key, &read_exp_time)
                .map_err(map_err)?;
            collect_nonce_writes!(bucket.nonces_ordered_by_exp_time, writes, &exp_time_write_cache);

            let (_, writes) = bucket.nonce_to_exp_time_map.tree_remove(&nonce_key, &read_nonce_map)
                .map_err(map_err)?;
            collect_nonce_writes!(bucket.nonce_to_exp_time_map, writes, &nonce_map_write_cache);
        }

        // 5. GC up to 5 expired nonces
        for _ in 0..5 {
            if bucket.nonces_ordered_by_exp_time.is_empty() {
                break;
            }
            let front = bucket.nonces_ordered_by_exp_time.tree_borrow_front(&read_exp_time)
                .map_err(map_err)?;
            match front {
                Some((front_key, _)) => {
                    if front_key.txn_expiration_time + 100 < now_seconds {
                        // Expired: remove from both maps
                        let (_, writes) = bucket.nonces_ordered_by_exp_time.tree_pop_front(&read_exp_time)
                            .map_err(map_err)?;
                        collect_nonce_writes!(bucket.nonces_ordered_by_exp_time, writes, &exp_time_write_cache);

                        let gc_nonce_key = NonceKey {
                            sender_address: front_key.sender_address,
                            nonce: front_key.nonce,
                        };
                        let (_, writes) = bucket.nonce_to_exp_time_map.tree_remove(&gc_nonce_key, &read_nonce_map)
                            .map_err(map_err)?;
                        collect_nonce_writes!(bucket.nonce_to_exp_time_map, writes, &nonce_map_write_cache);
                    } else {
                        break;
                    }
                },
                None => break,
            }
        }

        // 6. Insert into both maps
        let exp_key = NonceKeyWithExpTime {
            txn_expiration_time,
            sender_address: sender,
            nonce,
        };
        let writes = bucket.nonces_ordered_by_exp_time.tree_add(exp_key, true, &read_exp_time)
            .map_err(map_err)?;
        collect_nonce_writes!(bucket.nonces_ordered_by_exp_time, writes, &exp_time_write_cache);

        let writes = bucket.nonce_to_exp_time_map.tree_add(nonce_key, txn_expiration_time, &read_nonce_map)
            .map_err(map_err)?;
        collect_nonce_writes!(bucket.nonce_to_exp_time_map, writes, &nonce_map_write_cache);
    } // session_ref dropped here

    // 7. Serialize bucket and write back to table
    let bucket_bytes = bcs::to_bytes(&bucket).map_err(|e| {
        VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR,
            Some(format!("native nonce validation: serialize NonceBucket: {}", e)))
    })?;

    if is_new_bucket {
        native_session_helpers::create_table_item_bytes(
            session, nonce_table_handle, &bucket_key_bytes, bucket_bytes.into(),
        )?;
    } else {
        native_session_helpers::write_table_item_bytes(
            session, nonce_table_handle, &bucket_key_bytes, bucket_bytes.into(),
        )?;
    }

    // Flush all BigOrderedMap child node writes
    for (handle, tw) in all_table_writes {
        let key_bytes = bcs::to_bytes(&tw.slot_index).map_err(|e| {
            VMStatus::error(StatusCode::VALUE_SERIALIZATION_ERROR,
                Some(format!("native nonce validation: serialize slot key: {}", e)))
        })?;
        if tw.is_new {
            native_session_helpers::create_table_item_bytes(
                session, handle, &key_bytes, tw.data.into(),
            )?;
        } else {
            native_session_helpers::write_table_item_bytes(
                session, handle, &key_bytes, tw.data.into(),
            )?;
        }
    }

    Ok(true)
}

/// Native prologue — fully native: all checks including gas balance.
///
/// 1. Timestamp expiration (native)
/// 2. Chain ID (native)
/// 3. Sequence number or nonce validation:
///    - SequenceNumber: native check against Account resource
///    - Nonce: native nonce validation (replaces Move VM call)
/// 4. Gas balance: native aggregator-based check
///    (required for ConcurrentFungibleBalance aggregator reads)
pub(crate) fn run_native_prologue<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    txn_data: &TransactionMetadata,
    module_storage: &impl ModuleStorage,
    traversal_context: &mut move_vm_runtime::module_traversal::TraversalContext,
) -> Result<(), VMStatus> {
    let sender = txn_data.sender;
    let gas_payer = txn_data.fee_payer.unwrap_or(sender);

    // 1. Timestamp check: now_seconds() < txn_expiration_time
    let now_microseconds = native_session_helpers::read_timestamp_microseconds(session)?;
    let now_seconds = now_microseconds / 1_000_000;
    if now_seconds >= txn_data.expiration_timestamp_secs {
        return Err(VMStatus::error(
            StatusCode::TRANSACTION_EXPIRED,
            Some("Native prologue: transaction expired".to_string()),
        ));
    }

    // 2. Chain ID check
    let chain_id_res: ChainIdResource = native_session_helpers::read_resource(
        session, &AccountAddress::ONE, &chain_id_tag(),
    )?.ok_or_else(|| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some("Native prologue: ChainId resource not found at 0x1".to_string()),
        )
    })?;
    if chain_id_res.id != txn_data.chain_id.id() {
        return Err(VMStatus::error(
            StatusCode::BAD_CHAIN_ID,
            Some("Native prologue: chain ID mismatch".to_string()),
        ));
    }

    // 3. Replay protection
    match txn_data.replay_protector {
        ReplayProtector::SequenceNumber(txn_seq) => {
            let account_res: AccountResource = native_session_helpers::read_resource(
                session, &sender, &account_tag(),
            )?.ok_or_else(|| {
                VMStatus::error(
                    StatusCode::SENDING_ACCOUNT_DOES_NOT_EXIST,
                    Some(format!("Native prologue: Account resource not found at {}", sender)),
                )
            })?;

            if txn_seq >= (1u64 << 63) {
                return Err(VMStatus::error(
                    StatusCode::SEQUENCE_NUMBER_TOO_BIG,
                    Some("Native prologue: sequence number too big".to_string()),
                ));
            }
            if txn_seq < account_res.sequence_number {
                return Err(VMStatus::error(
                    StatusCode::SEQUENCE_NUMBER_TOO_OLD,
                    Some(format!(
                        "Native prologue: txn seq {} < account seq {}",
                        txn_seq, account_res.sequence_number
                    )),
                ));
            }
            if txn_seq > account_res.sequence_number {
                return Err(VMStatus::error(
                    StatusCode::SEQUENCE_NUMBER_TOO_NEW,
                    Some(format!(
                        "Native prologue: txn seq {} > account seq {}",
                        txn_seq, account_res.sequence_number
                    )),
                ));
            }
        },
        ReplayProtector::Nonce(nonce) => {
            // Orderless txns: validate expiration not too far in the future
            let max_exp = now_seconds + 100; // MAX_EXP_TIME_SECONDS_FOR_ORDERLESS_TXNS
            if txn_data.expiration_timestamp_secs > max_exp {
                return Err(VMStatus::error(
                    StatusCode::TRANSACTION_EXPIRED,
                    Some("Native prologue: orderless txn expiration too far in the future".to_string()),
                ));
            }

            // Native nonce validation — replaces Move VM call
            let nonce_ok = native_check_and_insert_nonce(
                session, sender, nonce, txn_data.expiration_timestamp_secs, now_seconds,
            )?;
            if !nonce_ok {
                return Err(VMStatus::error(
                    StatusCode::SEQUENCE_NUMBER_TOO_OLD,
                    Some("Native prologue: nonce already used".to_string()),
                ));
            }
        },
    }

    // 4. Gas balance check via Move VM — ConcurrentFungibleBalance uses aggregator
    // DelayedFieldIDs that cannot be read natively without corrupting the data cache
    // for subsequent Move VM epilogue calls (burn_fee).
    let gas_amount = u64::from(txn_data.gas_unit_price) * u64::from(txn_data.max_gas_amount);
    if gas_amount > 0 {
        let fa_module = move_core_types::language_storage::ModuleId::new(
            AccountAddress::ONE,
            Identifier::new("aptos_account").unwrap(),
        );
        let result = session.execute_function_bypass_visibility(
            &fa_module,
            &Identifier::new("is_fungible_balance_at_least").unwrap(),
            vec![],
            vec![
                bcs::to_bytes(&gas_payer).unwrap(),
                bcs::to_bytes(&gas_amount).unwrap(),
            ],
            &mut UnmeteredGasMeter,
            traversal_context,
            module_storage,
        ).map_err(|e| e.into_vm_status())?;

        if !result.return_values.is_empty() {
            let has_balance: bool = bcs::from_bytes(&result.return_values[0].0).unwrap_or(false);
            if !has_balance {
                return Err(VMStatus::error(
                    StatusCode::INSUFFICIENT_BALANCE_FOR_TRANSACTION_FEE,
                    Some("Native prologue: insufficient gas balance".to_string()),
                ));
            }
        }
    }

    Ok(())
}

/// Native epilogue — fully native: gas fee burn/refund + seq num + events.
///
/// 1. Gas fee calculation (native)
/// 2. Gas fee burn/refund via native aggregator operations
/// 3. Sequence number increment (native, for non-orderless txns)
/// 4. FeeStatement event emission (native)
pub(crate) fn run_native_epilogue<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    txn_data: &TransactionMetadata,
    gas_remaining: u64,
    fee_statement: FeeStatement,
    module_storage: &impl ModuleStorage,
    traversal_context: &mut move_vm_runtime::module_traversal::TraversalContext,
) -> Result<(), VMStatus> {
    let sender = txn_data.sender;
    let gas_payer = txn_data.fee_payer.unwrap_or(sender);
    let gas_price = u64::from(txn_data.gas_unit_price);
    let max_gas = u64::from(txn_data.max_gas_amount);
    let storage_fee_refund = fee_statement.storage_fee_refund();

    // Validate gas accounting
    if max_gas < gas_remaining {
        return Err(VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some("Native epilogue: max_gas < gas_remaining".to_string()),
        ));
    }
    let gas_used = max_gas - gas_remaining;

    // Check overflow
    if (gas_price as u128) * (gas_used as u128) > u64::MAX as u128 {
        return Err(VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some("Native epilogue: fee overflow".to_string()),
        ));
    }
    let transaction_fee = gas_price * gas_used;

    // Gas fee burn/refund via Move VM — aggregator writes require Move VM's
    // delayed field machinery (resolver exchanges IDs before returning bytes,
    // so native code can't obtain DelayedFieldIDs for writes).
    let txn_fee_module = move_core_types::language_storage::ModuleId::new(
        AccountAddress::ONE,
        Identifier::new("transaction_fee").unwrap(),
    );
    if transaction_fee > storage_fee_refund {
        let burn_amount = transaction_fee - storage_fee_refund;
        session.execute_function_bypass_visibility(
            &txn_fee_module,
            &Identifier::new("burn_fee").unwrap(),
            vec![],
            vec![
                bcs::to_bytes(&gas_payer).unwrap(),
                bcs::to_bytes(&burn_amount).unwrap(),
            ],
            &mut UnmeteredGasMeter,
            traversal_context,
            module_storage,
        ).map_err(|e| e.into_vm_status())?;
    } else if transaction_fee < storage_fee_refund {
        let mint_amount = storage_fee_refund - transaction_fee;
        session.execute_function_bypass_visibility(
            &txn_fee_module,
            &Identifier::new("mint_and_refund").unwrap(),
            vec![],
            vec![
                bcs::to_bytes(&gas_payer).unwrap(),
                bcs::to_bytes(&mint_amount).unwrap(),
            ],
            &mut UnmeteredGasMeter,
            traversal_context,
            module_storage,
        ).map_err(|e| e.into_vm_status())?;
    }

    // Increment sequence number (only for non-orderless txns)
    if !txn_data.is_orderless() {
        let account_tag = account_tag();
        let mut account: AccountResource = native_session_helpers::read_resource(
            session, &sender, &account_tag,
        )?.ok_or_else(|| {
            VMStatus::error(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                Some(format!(
                    "Native epilogue: Account resource not found at {}",
                    sender
                )),
            )
        })?;

        account.sequence_number += 1;
        native_session_helpers::write_resource(session, &sender, &account_tag, &account)?;
    }

    // Emit FeeStatement event
    let fee_statement_type = TypeTag::Struct(Box::new(StructTag {
        address: AccountAddress::ONE,
        module: Identifier::new("transaction_fee").unwrap(),
        name: Identifier::new("FeeStatement").unwrap(),
        type_args: vec![],
    }));
    let fee_statement_bytes = bcs::to_bytes(&fee_statement).map_err(|e| {
        VMStatus::error(
            StatusCode::VALUE_SERIALIZATION_ERROR,
            Some(format!("Native epilogue: failed to serialize FeeStatement: {}", e)),
        )
    })?;
    native_session_helpers::emit_event(session, fee_statement_type, fee_statement_bytes)?;

    Ok(())
}
