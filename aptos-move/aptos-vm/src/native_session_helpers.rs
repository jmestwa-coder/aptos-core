// Copyright (c) Aptos Foundation
// Licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE

//! Session helper functions for native Rust execution of Move entry functions.
//!
//! This module provides a clean API for native Rust code to interact with the Move VM
//! session during native entry function execution. It wraps the session's internal
//! data structures (resource cache, native extensions) so that native implementations
//! can read/write resources, emit events, and access transaction context without
//! needing to know about Move VM internals.
//!
//! ## Resource Read/Write Model
//!
//! Native code operates through a `NativeResourceCache` overlay:
//!
//! - **Reads** check the native overlay first (for read-after-write consistency),
//!   then fall through to the underlying storage resolver.
//! - **Writes** and **deletes** are stored in the native overlay.
//! - At `finish()` time, the overlay is merged into the `VMChangeSet` as `WriteOp`s.
//!
//! All helpers operate through `SessionExt` accessor methods (defined in the session
//! module) that expose `pub(crate)` access to the underlying data cache and extensions.

use crate::move_vm_ext::{AptosMoveResolver, SessionExt};
use aptos_framework_natives::event::NativeEventContext;
use aptos_framework_natives::transaction_context::NativeTransactionContext;
use aptos_types::contract_event::ContractEvent;
use aptos_types::vm_status::VMStatus;
use bytes::Bytes;
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{StructTag, TypeTag},
    vm_status::StatusCode,
};
use aptos_table_natives::TableHandle;
use serde::{de::DeserializeOwned, Serialize};

// ---------------------------------------------------------------------------
// Resource helpers (bytes-level)
// ---------------------------------------------------------------------------

/// Reads a resource's raw bytes. Checks the native cache first (for read-after-write
/// consistency), then falls through to the underlying storage resolver.
///
/// Returns `Ok(Some(bytes))` if the resource exists, `Ok(None)` if it does not,
/// or `Err(VMStatus)` if reading fails.
pub fn read_resource_bytes<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    address: &AccountAddress,
    struct_tag: &StructTag,
) -> Result<Option<Bytes>, VMStatus> {
    // Check the native overlay first.
    if let Some(cached) = session.native_resource_cache().get(address, struct_tag) {
        return Ok(cached.clone());
    }

    // Fall through to the resolver.
    let resolver = session.resolver();
    let (data, _bytes_loaded) = resolver
        .get_resource_bytes_with_metadata_and_layout(address, struct_tag, &[], None)
        .map_err(|e| e.finish(move_binary_format::errors::Location::Undefined).into_vm_status())?;
    Ok(data)
}

/// Writes a resource's raw bytes to the native cache.
///
/// The data will be merged into the `VMChangeSet` when `finish()` is called.
pub fn write_resource_bytes<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    address: &AccountAddress,
    struct_tag: &StructTag,
    data: Bytes,
) -> Result<(), VMStatus> {
    session
        .native_resource_cache_mut()
        .set(*address, struct_tag.clone(), data);
    Ok(())
}

/// Marks a resource as deleted in the native cache.
///
/// The deletion will be merged into the `VMChangeSet` when `finish()` is called.
pub fn delete_resource<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    address: &AccountAddress,
    struct_tag: &StructTag,
) -> Result<(), VMStatus> {
    session
        .native_resource_cache_mut()
        .delete(*address, struct_tag.clone());
    Ok(())
}

/// Checks if a resource exists. Considers the native cache overlay:
/// - If the cache has the resource as `Some(bytes)`, it exists.
/// - If the cache has the resource as `None` (deleted), it does not exist.
/// - Otherwise, checks the underlying storage resolver.
pub fn resource_exists<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    address: &AccountAddress,
    struct_tag: &StructTag,
) -> Result<bool, VMStatus> {
    // Check cache first.
    if let Some(cached) = session.native_resource_cache().get(address, struct_tag) {
        return Ok(cached.is_some());
    }

    // Fall through to resolver.
    let resolver = session.resolver();
    let (data, _bytes_loaded) = resolver
        .get_resource_bytes_with_metadata_and_layout(address, struct_tag, &[], None)
        .map_err(|e| e.finish(move_binary_format::errors::Location::Undefined).into_vm_status())?;
    Ok(data.is_some())
}

// ---------------------------------------------------------------------------
// Resource helpers (typed via serde)
// ---------------------------------------------------------------------------

/// Reads a resource and deserializes it from BCS.
///
/// Returns `Ok(Some(value))` if the resource exists and can be deserialized,
/// `Ok(None)` if the resource does not exist, or `Err(VMStatus)` on failure.
pub fn read_resource<T: DeserializeOwned, R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    address: &AccountAddress,
    struct_tag: &StructTag,
) -> Result<Option<T>, VMStatus> {
    let bytes = read_resource_bytes(session, address, struct_tag)?;
    match bytes {
        Some(b) => {
            let value = bcs::from_bytes(&b).map_err(|e| {
                VMStatus::error(
                    StatusCode::FAILED_TO_DESERIALIZE_RESOURCE,
                    Some(format!(
                        "Native dispatch: failed to deserialize resource {:?} at {}: {}",
                        struct_tag, address, e
                    )),
                )
            })?;
            Ok(Some(value))
        },
        None => Ok(None),
    }
}

/// Serializes a value to BCS and writes it to the native cache.
pub fn write_resource<T: Serialize, R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    address: &AccountAddress,
    struct_tag: &StructTag,
    value: &T,
) -> Result<(), VMStatus> {
    let bytes = bcs::to_bytes(value).map_err(|e| {
        VMStatus::error(
            StatusCode::VALUE_SERIALIZATION_ERROR,
            Some(format!(
                "Native dispatch: failed to serialize resource {:?} at {}: {}",
                struct_tag, address, e
            )),
        )
    })?;
    write_resource_bytes(session, address, struct_tag, bytes.into())
}

// ---------------------------------------------------------------------------
// Table helpers
// ---------------------------------------------------------------------------

/// Reads a table item by key. Goes directly through the session's resolver
/// (storage layer), which implements `TableResolver`.
///
/// Returns `Ok(Some(bytes))` if the item exists, `Ok(None)` if not.
pub fn read_table_item_bytes<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    table_handle: TableHandle,
    key: &[u8],
) -> Result<Option<Bytes>, VMStatus> {
    // Check the native table write cache first (for read-after-write consistency).
    if let Some(cached) = session.native_table_write_cache().get(&table_handle, key) {
        return Ok(Some(cached.clone()));
    }
    // Fall through to the resolver.
    session
        .resolver()
        .resolve_table_entry_bytes_with_layout(&table_handle, key, None)
        .map_err(|e| e.finish(move_binary_format::errors::Location::Undefined).into_vm_status())
}

/// Writes a table item by key to the native table write cache.
/// The data will be merged into the table change set when the session is finalized.
pub fn write_table_item_bytes<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    table_handle: TableHandle,
    key: &[u8],
    value: Bytes,
) -> Result<(), VMStatus> {
    session
        .native_table_write_cache_mut()
        .add(table_handle, key.to_vec(), value);
    Ok(())
}

/// Creates a new table item by key in the native table write cache.
/// This is used for table entries that don't exist yet (first-time creation).
/// The data will be merged into the table change set as a New (creation) op.
pub fn create_table_item_bytes<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    table_handle: TableHandle,
    key: &[u8],
    value: Bytes,
) -> Result<(), VMStatus> {
    session
        .native_table_write_cache_mut()
        .add_creation(table_handle, key.to_vec(), value);
    Ok(())
}

/// Creates a new table handle via the NativeTableContext.
/// This is used when a BigOrderedMap needs to be split into child nodes
/// but no table has been allocated yet (slots = None).
pub fn create_new_table_handle<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
) -> TableHandle {
    let table_ctx = session.extensions().get::<aptos_table_natives::NativeTableContext>();
    table_ctx.create_table_handle()
}

// ---------------------------------------------------------------------------
// Event helpers
// ---------------------------------------------------------------------------

/// Emits a V2 contract event through the session's `NativeEventContext`.
///
/// This is the native equivalent of the Move `event::emit<T>(msg)` function.
/// The event will be included in the transaction's output change set when the
/// session is finalized.
pub fn emit_event<R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    type_tag: TypeTag,
    event_data: Vec<u8>,
) -> Result<(), VMStatus> {
    let event = ContractEvent::new_v2(type_tag, event_data).map_err(|_| {
        VMStatus::error(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            Some("Native dispatch: failed to create V2 event".to_string()),
        )
    })?;
    let ctx = session.extensions_mut().get_mut::<NativeEventContext>();
    ctx.push_event(event, None);
    Ok(())
}

// ---------------------------------------------------------------------------
// Transaction context helpers
// ---------------------------------------------------------------------------

/// Returns a mutable reference to the `NativeTransactionContext`.
///
/// This provides access to the monotonically increasing counter that native code
/// can use for generating unique identifiers within a transaction, as well as
/// the session hash, chain ID, and other transaction metadata.
pub fn get_transaction_context<'a, R: AptosMoveResolver>(
    session: &'a mut SessionExt<'_, R>,
) -> &'a mut NativeTransactionContext {
    session.extensions_mut().get_mut::<NativeTransactionContext>()
}

// ---------------------------------------------------------------------------
// Resource group member helpers
// ---------------------------------------------------------------------------

/// Reads a resource group member's raw bytes from storage.
///
/// Resource group members in Aptos (e.g., resources stored in `ObjectGroup`)
/// are accessed through the `ResourceGroupView` interface, using a group key
/// constructed from the address and the group tag (e.g., `0x1::object::ObjectGroup`).
///
/// Returns `Ok(Some(bytes))` if the member exists, `Ok(None)` if not.
pub fn read_resource_group_member_bytes<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    address: &AccountAddress,
    group_tag: &StructTag,
    resource_tag: &StructTag,
) -> Result<Option<Bytes>, VMStatus> {
    let group_key = aptos_types::state_store::state_key::StateKey::resource_group(address, group_tag);
    let view = session.resolver().as_resource_group_view();
    view.get_resource_from_group(&group_key, resource_tag, None)
        .map_err(|e| e.finish(move_binary_format::errors::Location::Undefined).into_vm_status())
}

/// Reads a resource group member and deserializes it from BCS.
///
/// This is a typed wrapper around `read_resource_group_member_bytes`.
pub fn read_resource_group_member<T: DeserializeOwned, R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
    address: &AccountAddress,
    group_tag: &StructTag,
    resource_tag: &StructTag,
) -> Result<Option<T>, VMStatus> {
    let bytes = read_resource_group_member_bytes(session, address, group_tag, resource_tag)?;
    match bytes {
        Some(b) => {
            let value = bcs::from_bytes(&b).map_err(|e| {
                VMStatus::error(
                    StatusCode::FAILED_TO_DESERIALIZE_RESOURCE,
                    Some(format!(
                        "Native dispatch: failed to deserialize resource group member {:?} at {}: {}",
                        resource_tag, address, e
                    )),
                )
            })?;
            Ok(Some(value))
        },
        None => Ok(None),
    }
}

/// Writes a resource group member back to the native cache.
///
/// Resource group members are stored as individual resources keyed by their
/// own struct tag. The native cache overlay tracks them the same way as
/// top-level resources, and the session's `finish()` logic handles the
/// resource-group re-assembly.
///
/// NOTE: We write the member as a standalone resource in the native overlay.
/// The session's `split_and_merge_resource_groups` logic in `finish()` will
/// detect (via module metadata) that this struct tag is a resource group member
/// and will merge it into the correct resource group write set.
pub fn write_resource_group_member<T: Serialize, R: AptosMoveResolver>(
    session: &mut SessionExt<'_, R>,
    address: &AccountAddress,
    resource_tag: &StructTag,
    value: &T,
) -> Result<(), VMStatus> {
    // Write as a regular resource. The session finish() path handles the
    // resource group re-assembly.
    write_resource(session, address, resource_tag, value)
}

// ---------------------------------------------------------------------------
// Timestamp helpers
// ---------------------------------------------------------------------------

/// Reads the current block timestamp in microseconds from the
/// `0x1::timestamp::CurrentTimeMicroseconds` resource.
pub fn read_timestamp_microseconds<R: AptosMoveResolver>(
    session: &SessionExt<'_, R>,
) -> Result<u64, VMStatus> {
    #[derive(serde::Deserialize)]
    struct CurrentTimeMicroseconds {
        microseconds: u64,
    }

    let timestamp_tag = StructTag {
        address: AccountAddress::ONE,
        module: move_core_types::identifier::Identifier::new("timestamp").unwrap(),
        name: move_core_types::identifier::Identifier::new("CurrentTimeMicroseconds").unwrap(),
        type_args: vec![],
    };

    let ts: CurrentTimeMicroseconds = read_resource(session, &AccountAddress::ONE, &timestamp_tag)?
        .ok_or_else(|| {
            VMStatus::error(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                Some("Native dispatch: CurrentTimeMicroseconds resource not found at 0x1".to_string()),
            )
        })?;
    Ok(ts.microseconds)
}
