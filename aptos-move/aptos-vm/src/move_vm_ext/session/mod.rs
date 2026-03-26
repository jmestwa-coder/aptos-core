// Copyright (c) Aptos Foundation
// Licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE

use crate::{
    data_cache::get_resource_group_member_from_metadata,
    move_vm_ext::{
        resource_state_key, write_op_converter::WriteOpConverter, AptosMoveResolver, SessionId,
    },
};
use aptos_framework_natives::{
    aggregator_natives::{AggregatorChangeSet, AggregatorChangeV1, NativeAggregatorContext},
    code::{NativeCodeContext, PublishRequest},
    cryptography::{algebra::AlgebraContext, ristretto255_point::NativeRistrettoPointContext},
    event::NativeEventContext,
    object::NativeObjectContext,
    randomness::RandomnessContext,
    state_storage::NativeStateStorageContext,
    transaction_context::NativeTransactionContext,
};
use aptos_table_natives::{NativeTableContext, TableChangeSet};
use aptos_types::{
    chain_id::ChainId, contract_event::ContractEvent, on_chain_config::Features,
    state_store::state_key::StateKey,
    transaction::user_transaction_context::UserTransactionContext, write_set::WriteOp,
};
use aptos_vm_types::{
    abstract_write_op::AbstractResourceWriteOp,
    change_set::VMChangeSet, module_and_script_storage::module_storage::AptosModuleStorage,
    module_write_set::ModuleWrite, storage::change_set_configs::ChangeSetConfigs,
};
use bytes::Bytes;
use move_binary_format::errors::{Location, PartialVMError, PartialVMResult, VMResult};
use move_core_types::{
    account_address::AccountAddress,
    effects::{AccountChanges, Changes, Op as MoveStorageOp},
    identifier::IdentStr,
    language_storage::{ModuleId, StructTag, TypeTag},
    value::MoveTypeLayout,
    vm_status::StatusCode,
};
use move_vm_runtime::{
    config::VMConfig,
    data_cache::{MoveVmDataCacheAdapter, TransactionDataCache},
    dispatch_loader,
    execution_tracing::TraceRecorder,
    module_traversal::TraversalContext,
    move_vm::{MoveVM, SerializedReturnValues},
    native_extensions::NativeContextExtensions,
    AsFunctionValueExtension, InstantiatedFunctionLoader, LegacyLoaderConfig, LoadedFunction,
    Loader, ModuleStorage, VerifiedModuleBundle,
};
use move_vm_types::{
    gas::GasMeter,
    value_serde::{FunctionValueExtension, ValueSerDeContext},
    values::Value,
};
use std::{borrow::Borrow, collections::BTreeMap};
use triomphe::Arc as TriompheArc;

pub mod respawned_session;
pub mod session_id;
pub(crate) mod user_transaction_sessions;
pub mod view_with_change_set;

pub(crate) enum ResourceGroupChangeSet {
    // Merged resource groups op.
    V0(BTreeMap<StateKey, MoveStorageOp<BytesWithResourceLayout>>),
    // Granular ops to individual resources within a group.
    V1(BTreeMap<StateKey, BTreeMap<StructTag, MoveStorageOp<BytesWithResourceLayout>>>),
}
type AccountChangeSet = AccountChanges<BytesWithResourceLayout>;
type ChangeSet = Changes<BytesWithResourceLayout>;
pub type BytesWithResourceLayout = (Bytes, Option<TriompheArc<MoveTypeLayout>>);

/// Cache for resources modified by native execution code.
/// Maps (address, struct_tag) -> Some(bytes) for modifications/creations,
/// or None for deletions.
pub(crate) struct NativeResourceCache {
    resources: BTreeMap<(AccountAddress, StructTag), Option<Bytes>>,
}

impl NativeResourceCache {
    /// Creates a new empty cache.
    pub(crate) fn new() -> Self {
        Self {
            resources: BTreeMap::new(),
        }
    }

    /// Check if a resource is in the cache. Returns:
    /// - `Some(&Some(bytes))` if set/modified
    /// - `Some(&None)` if deleted
    /// - `None` if not in cache
    pub(crate) fn get(
        &self,
        addr: &AccountAddress,
        tag: &StructTag,
    ) -> Option<&Option<Bytes>> {
        self.resources.get(&(*addr, tag.clone()))
    }

    /// Set or update a resource in the cache.
    pub(crate) fn set(
        &mut self,
        addr: AccountAddress,
        tag: StructTag,
        bytes: Bytes,
    ) {
        if tag.module.as_str() == "async_matching_engine" && tag.name.as_str() == "AsyncMatchingEngine" {
        }
        self.resources.insert((addr, tag), Some(bytes));
    }

    /// Mark a resource as deleted in the cache.
    pub(crate) fn delete(
        &mut self,
        addr: AccountAddress,
        tag: StructTag,
    ) {
        self.resources.insert((addr, tag), None);
    }

    /// Consume the cache and return an iterator over all entries.
    pub(crate) fn into_iter(self) -> impl Iterator<Item = ((AccountAddress, StructTag), Option<Bytes>)> {
        self.resources.into_iter()
    }

    /// Returns true if the cache is empty (no native writes/deletes).
    pub(crate) fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }
}

/// Cache for table item writes from native execution code.
/// Stores (handle, key, value, is_creation) tuples that will be merged into the table change set
/// at finish time. The `is_creation` flag indicates whether the entry is being created (New) or
/// modified (Modify) in the table, which determines the correct MoveStorageOp used during merging.
pub(crate) struct NativeTableWriteCache {
    items: Vec<(aptos_table_natives::TableHandle, Vec<u8>, Bytes, bool)>,
}

impl NativeTableWriteCache {
    pub(crate) fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Add a table item modification (the entry must already exist in storage).
    pub(crate) fn add(&mut self, handle: aptos_table_natives::TableHandle, key: Vec<u8>, value: Bytes) {
        self.items.push((handle, key, value, false));
    }

    /// Add a table item creation (the entry must NOT already exist in storage).
    pub(crate) fn add_creation(&mut self, handle: aptos_table_natives::TableHandle, key: Vec<u8>, value: Bytes) {
        self.items.push((handle, key, value, true));
    }

    /// Look up a table item by handle and key. Returns the most recent value
    /// written for this (handle, key) pair, if any.
    pub(crate) fn get(&self, handle: &aptos_table_natives::TableHandle, key: &[u8]) -> Option<Bytes> {
        // Search from back to front to find the most recent write for this key.
        for (h, k, v, _) in self.items.iter().rev() {
            if h == handle && k.as_slice() == key {
                return Some(v.clone());
            }
        }
        None
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub(crate) fn into_items(self) -> Vec<(aptos_table_natives::TableHandle, Vec<u8>, Bytes, bool)> {
        self.items
    }
}

pub struct SessionExt<'r, R> {
    data_cache: TransactionDataCache,
    extensions: NativeContextExtensions<'r>,
    pub(crate) resolver: &'r R,
    is_storage_slot_metadata_enabled: bool,
    native_resource_cache: NativeResourceCache,
    native_table_write_cache: NativeTableWriteCache,
}

impl<'r, R> SessionExt<'r, R>
where
    R: AptosMoveResolver,
{
    pub(crate) fn new(
        session_id: SessionId,
        chain_id: ChainId,
        features: &Features,
        vm_config: &VMConfig,
        maybe_user_transaction_context: Option<UserTransactionContext>,
        resolver: &'r R,
    ) -> Self {
        let extensions = make_aptos_extensions(
            resolver,
            chain_id,
            vm_config,
            session_id,
            maybe_user_transaction_context,
        );

        let is_storage_slot_metadata_enabled = features.is_storage_slot_metadata_enabled();
        Self {
            data_cache: TransactionDataCache::empty(),
            extensions,
            resolver,
            is_storage_slot_metadata_enabled,
            native_resource_cache: NativeResourceCache::new(),
            native_table_write_cache: NativeTableWriteCache::new(),
        }
    }

    /// Returns an immutable reference to the native table write cache.
    pub(crate) fn native_table_write_cache(&self) -> &NativeTableWriteCache {
        &self.native_table_write_cache
    }

    /// Returns a mutable reference to the native table write cache.
    pub(crate) fn native_table_write_cache_mut(&mut self) -> &mut NativeTableWriteCache {
        &mut self.native_table_write_cache
    }

    /// Returns an immutable reference to the resolver.
    ///
    /// Used by native execution helpers to read resources directly from storage.
    pub(crate) fn resolver(&self) -> &R {
        self.resolver
    }

    /// Returns a mutable reference to the native context extensions.
    ///
    /// Used by native execution helpers to access event context, transaction
    /// context, and other native extensions during native entry function execution.
    pub(crate) fn extensions_mut(&mut self) -> &mut NativeContextExtensions<'r> {
        &mut self.extensions
    }

    /// Returns an immutable reference to the native context extensions.
    #[allow(dead_code)]
    pub(crate) fn extensions(&self) -> &NativeContextExtensions<'r> {
        &self.extensions
    }

    /// Returns an immutable reference to the native resource cache.
    pub(crate) fn native_resource_cache(&self) -> &NativeResourceCache {
        &self.native_resource_cache
    }

    /// Returns a mutable reference to the native resource cache.
    pub(crate) fn native_resource_cache_mut(&mut self) -> &mut NativeResourceCache {
        &mut self.native_resource_cache
    }


    pub fn execute_function_bypass_visibility(
        &mut self,
        module_id: &ModuleId,
        function_name: &IdentStr,
        ty_args: Vec<TypeTag>,
        args: Vec<impl Borrow<[u8]>>,
        gas_meter: &mut impl GasMeter,
        traversal_context: &mut TraversalContext,
        module_storage: &impl ModuleStorage,
    ) -> VMResult<SerializedReturnValues> {
        dispatch_loader!(module_storage, loader, {
            let func = loader.load_instantiated_function(
                &LegacyLoaderConfig::unmetered(),
                gas_meter,
                traversal_context,
                module_id,
                function_name,
                &ty_args,
            )?;
            MoveVM::execute_loaded_function(
                func,
                args,
                &mut MoveVmDataCacheAdapter::new(&mut self.data_cache, self.resolver, &loader),
                gas_meter,
                traversal_context,
                &mut self.extensions,
                &loader,
            )
        })
    }

    pub fn execute_loaded_function(
        &mut self,
        func: LoadedFunction,
        args: Vec<impl Borrow<[u8]>>,
        gas_meter: &mut impl GasMeter,
        traversal_context: &mut TraversalContext,
        loader: &impl Loader,
        trace_recorder: &mut impl TraceRecorder,
    ) -> VMResult<SerializedReturnValues> {
        MoveVM::execute_loaded_function_with_tracing(
            func,
            args,
            &mut MoveVmDataCacheAdapter::new(&mut self.data_cache, self.resolver, loader),
            gas_meter,
            traversal_context,
            &mut self.extensions,
            loader,
            trace_recorder,
        )
    }

    pub fn finish(
        self,
        configs: &ChangeSetConfigs,
        module_storage: &impl ModuleStorage,
    ) -> VMResult<VMChangeSet> {
        // Note: enabled by 1.38 gas feature version.
        let is_1_38_release = module_storage
            .runtime_environment()
            .vm_config()
            .propagate_dependency_limit_error;
        let function_extension = module_storage.as_function_value_extension();

        let resource_converter = |value: Value,
                                  layout: TriompheArc<MoveTypeLayout>,
                                  has_aggregator_lifting: bool|
         -> PartialVMResult<BytesWithResourceLayout> {
            let serialization_result = if has_aggregator_lifting {
                // We allow serialization of native values here because we want to
                // temporarily store native values (via encoding to ensure deterministic
                // gas charging) in block storage.
                ValueSerDeContext::new(function_extension.max_value_nest_depth())
                    .with_delayed_fields_serde()
                    .with_func_args_deserialization(&function_extension)
                    .serialize(&value, &layout)?
                    .map(|bytes| (bytes.into(), Some(layout)))
            } else {
                // Otherwise, there should be no native values so ensure
                // serialization fails here if there are any.
                ValueSerDeContext::new(function_extension.max_value_nest_depth())
                    .with_func_args_deserialization(&function_extension)
                    .serialize(&value, &layout)?
                    .map(|bytes| (bytes.into(), None))
            };
            serialization_result.ok_or_else(|| {
                let status_code = if is_1_38_release {
                    StatusCode::VALUE_SERIALIZATION_ERROR
                } else {
                    StatusCode::INTERNAL_TYPE_ERROR
                };
                // Note: When enable_closure_depth_check is enabled, do not format
                // `value` here - deeply nested closures can cause stack overflow
                // during Display formatting.
                let enable_closure_depth_check = module_storage
                    .runtime_environment()
                    .vm_config()
                    .enable_closure_depth_check;
                let message = if enable_closure_depth_check {
                    "Error when serializing resource.".to_string()
                } else {
                    format!("Error when serializing resource {}.", value)
                };
                PartialVMError::new(status_code).with_message(message)
            })
        };

        let Self {
            data_cache,
            mut extensions,
            resolver,
            is_storage_slot_metadata_enabled,
            native_resource_cache,
            native_table_write_cache,
        } = self;

        let change_set = data_cache
            .into_custom_effects(&resource_converter)
            .map_err(|e| e.finish(Location::Undefined))?;

        let (change_set, resource_group_change_set) =
            Self::split_and_merge_resource_groups(resolver, module_storage, change_set)
                .map_err(|e| e.finish(Location::Undefined))?;

        let table_context: NativeTableContext = extensions.remove();
        let table_change_set = table_context
            .into_change_set(&function_extension)
            .map_err(|e| e.finish(Location::Undefined))?;

        let aggregator_context: NativeAggregatorContext = extensions.remove();
        let aggregator_change_set = aggregator_context
            .into_change_set()
            .map_err(|e| e.finish(Location::Undefined))?;

        let event_context: NativeEventContext = extensions.remove();
        let events = event_context.legacy_into_events();

        let woc = WriteOpConverter::new(resolver, is_storage_slot_metadata_enabled);

        // Merge native resource cache entries into the change set.
        // Resource group members (e.g., PerpMarket in ObjectGroup) are handled by
        // building proper resource group writes that match the Move VM format.
        let native_resource_write_set = if !native_resource_cache.is_empty() {
            Self::convert_native_resource_cache_with_groups(
                &woc,
                native_resource_cache,
                resolver,
                module_storage,
            )?
        } else {
            BTreeMap::new()
        };

        let mut change_set = Self::convert_change_set(
            &woc,
            change_set,
            resource_group_change_set,
            events,
            table_change_set,
            aggregator_change_set,
            configs.legacy_resource_creation_as_modification(),
        )
        .map_err(|e| e.finish(Location::Undefined))?;

        // Merge native table item writes into the change set.
        if !native_table_write_cache.is_empty() {
            use aptos_vm_types::abstract_write_op::AbstractResourceWriteOp;

            let mut native_table_writes: BTreeMap<StateKey, AbstractResourceWriteOp> = BTreeMap::new();
            for (handle, key, value, is_creation) in native_table_write_cache.into_items() {
                let state_key = StateKey::table_item(&aptos_types::state_store::table::TableHandle(handle.0), &key);
                let storage_op = if is_creation {
                    MoveStorageOp::New((value, None))
                } else {
                    MoveStorageOp::Modify((value, None))
                };
                let (write_op, _layout) = woc.convert_resource(
                    &state_key,
                    storage_op,
                    false, // legacy_creation_as_modification
                ).map_err(|e| e.finish(Location::Undefined))?;
                native_table_writes.insert(state_key, AbstractResourceWriteOp::Write(write_op));
            }
            if !native_table_writes.is_empty() {
                let native_table_change_set = VMChangeSet::new(
                    native_table_writes,
                    vec![],
                    BTreeMap::new(),
                    BTreeMap::new(),
                    BTreeMap::new(),
                );
                change_set
                    .squash_additional_change_set(native_table_change_set)
                    .map_err(|e| e.finish(Location::Undefined))?;
            }
        }

        // Squash native resource writes into the final change set.
        if !native_resource_write_set.is_empty() {
            let native_change_set = VMChangeSet::new(
                native_resource_write_set,
                vec![],
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
            );
            change_set
                .squash_additional_change_set(native_change_set)
                .map_err(|e| e.finish(Location::Undefined))?;
        }

        Ok(change_set)
    }

    /// Returns the publish request if it exists. If the provided flag is set to true, disables any
    /// subsequent module publish requests.
    pub(crate) fn extract_publish_request(&mut self) -> Option<PublishRequest> {
        let ctx = self.extensions.get_mut::<NativeCodeContext>();
        ctx.extract_publish_request()
    }

    pub(crate) fn mark_unbiasable(&mut self) {
        let txn_context = self.extensions.get_mut::<RandomnessContext>();
        txn_context.mark_unbiasable();
    }


    /// Converts native resource cache entries into AbstractResourceWriteOps.
    ///
    /// Native resources bypass the Move data cache and are written directly
    /// as concrete WriteOps. Each cache entry is converted using the
    /// WriteOpConverter which determines the correct op type (create/modify/delete)
    /// based on whether the resource previously existed in storage.
    fn convert_native_resource_cache(
        woc: &WriteOpConverter,
        native_resource_cache: NativeResourceCache,
        resolver: &R,
    ) -> VMResult<BTreeMap<StateKey, AbstractResourceWriteOp>> {
        let mut result = BTreeMap::new();

        for ((addr, tag), maybe_bytes) in native_resource_cache.into_iter() {
            let state_key = resource_state_key(&addr, &tag)
                .map_err(|e| e.finish(Location::Undefined))?;

            // Determine if the resource previously existed to choose New vs Modify vs Delete.
            let existed = resolver
                .as_executor_view()
                .get_resource_state_value_metadata(&state_key)
                .map_err(|e| e.finish(Location::Undefined))?
                .is_some();

            let op: MoveStorageOp<BytesWithResourceLayout> = match maybe_bytes {
                Some(bytes) => {
                    if existed {
                        MoveStorageOp::Modify((bytes, None))
                    } else {
                        MoveStorageOp::New((bytes, None))
                    }
                },
                None => {
                    // Deletion: resource must have existed.
                    if !existed {
                        return Err(
                            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                                .with_message(format!(
                                    "Native cache: deleting non-existent resource {:?} at {}",
                                    tag, addr
                                ))
                                .finish(Location::Undefined),
                        );
                    }
                    MoveStorageOp::Delete
                },
            };

            let (write_op, layout) = woc
                .convert_resource(&state_key, op, false)
                .map_err(|e| e.finish(Location::Undefined))?;

            result.insert(
                state_key,
                AbstractResourceWriteOp::from_resource_write_with_maybe_layout(write_op, layout),
            );
        }

        Ok(result)
    }

    /// Enhanced version of convert_native_resource_cache that properly handles
    /// resource group members. Resource group members (e.g., PerpMarket in ObjectGroup)
    /// are grouped by their parent resource group, and the full group is read from
    /// storage, updated with the new member bytes, and written as a single ResourceGroup
    /// write op. This matches the Move VM's behavior where all members of a resource
    /// group are written together.
    fn convert_native_resource_cache_with_groups(
        woc: &WriteOpConverter,
        native_resource_cache: NativeResourceCache,
        resolver: &R,
        module_storage: &impl ModuleStorage,
    ) -> VMResult<BTreeMap<StateKey, AbstractResourceWriteOp>> {
        let mut result = BTreeMap::new();

        // First pass: separate entries into resource group members and standalone resources.
        // group_updates: (addr, group_tag) -> Vec<(member_tag, member_bytes)>
        let mut group_updates: BTreeMap<(AccountAddress, StructTag), Vec<(StructTag, Option<Bytes>)>> = BTreeMap::new();
        let mut standalone_entries: Vec<((AccountAddress, StructTag), Option<Bytes>)> = Vec::new();

        for ((addr, tag), maybe_bytes) in native_resource_cache.into_iter() {
            // Check if this struct tag is a resource group member.
            // First try module metadata (works when module is in cache).
            // If that fails (native dispatch may not load modules), check known
            // resource group members by name.
            let resource_group_tag = module_storage
                .unmetered_get_existing_deserialized_module(&tag.address, &tag.module)
                .ok()
                .and_then(|module| {
                    get_resource_group_member_from_metadata(&tag, &module.metadata)
                })
                .or_else(|| {
                    // Fallback: known ObjectGroup members from the etna DEX contracts.
                    // These structs are annotated with #[resource_group_member(group = ObjectGroup)]
                    // in their Move source.
                    let name = tag.name.as_str();
                    let is_known_object_group_member = matches!(name,
                        "PriceDetails" | "Price" | "PerpMarketConfig" | "PerpMarketConfiguration"
                        | "PerpMarketOracleSource" | "Subaccount" | "ObjectCore" | "ObjectGroup"
                        | "DelegatedAdminPermissions"
                        | "InternalSourceState"
                    );
                    if is_known_object_group_member {
                        Some(StructTag {
                            address: AccountAddress::ONE,
                            module: move_core_types::identifier::Identifier::new("object").unwrap(),
                            name: move_core_types::identifier::Identifier::new("ObjectGroup").unwrap(),
                            type_args: vec![],
                        })
                    } else {
                        None
                    }
                });


            if let Some(group_tag) = resource_group_tag {
                group_updates
                    .entry((addr, group_tag))
                    .or_default()
                    .push((tag, maybe_bytes));
            } else {
                standalone_entries.push(((addr, tag), maybe_bytes));
            }
        }


        // Process standalone resources (same as convert_native_resource_cache)
        for ((addr, tag), maybe_bytes) in standalone_entries {
            let state_key = resource_state_key(&addr, &tag)
                .map_err(|e| e.finish(Location::Undefined))?;
            let existed = resolver
                .as_executor_view()
                .get_resource_state_value_metadata(&state_key)
                .map_err(|e| e.finish(Location::Undefined))?
                .is_some();
            let op: MoveStorageOp<BytesWithResourceLayout> = match maybe_bytes {
                Some(bytes) => {
                    if existed { MoveStorageOp::Modify((bytes, None)) }
                    else { MoveStorageOp::New((bytes, None)) }
                },
                None => {
                    if !existed {
                        return Err(
                            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                                .with_message(format!(
                                    "Native cache: deleting non-existent resource {:?} at {}", tag, addr
                                ))
                                .finish(Location::Undefined),
                        );
                    }
                    MoveStorageOp::Delete
                },
            };
            let (write_op, layout) = woc.convert_resource(&state_key, op, false)
                .map_err(|e| e.finish(Location::Undefined))?;

            result.insert(state_key, AbstractResourceWriteOp::from_resource_write_with_maybe_layout(write_op, layout));
        }

        // Process resource group members: build GroupWrite for each group
        for ((addr, group_tag), members) in group_updates {
            let group_state_key = StateKey::resource_group(&addr, &group_tag);

            // Build individual member changes for convert_resource_group_v1.
            // Since native dispatch only touches existing members (read-then-write-back),
            // all changes are Modify operations.
            let mut group_changes: BTreeMap<StructTag, MoveStorageOp<BytesWithResourceLayout>> = BTreeMap::new();
            for (member_tag, maybe_bytes) in members {
                match maybe_bytes {
                    Some(bytes) => {
                        group_changes.insert(member_tag, MoveStorageOp::Modify((bytes, None)));
                    },
                    None => {
                        group_changes.insert(member_tag, MoveStorageOp::Delete);
                    },
                }
            }

            let group_write = woc.convert_resource_group_v1(&group_state_key, group_changes.clone())
                .map_err(|e| e.finish(Location::Undefined))?;
            result.insert(group_state_key, AbstractResourceWriteOp::WriteResourceGroup(group_write));
        }

        Ok(result)
    }

    fn populate_v0_resource_group_change_set(
        change_set: &mut BTreeMap<StateKey, MoveStorageOp<BytesWithResourceLayout>>,
        state_key: StateKey,
        mut source_data: BTreeMap<StructTag, Bytes>,
        resources: BTreeMap<StructTag, MoveStorageOp<BytesWithResourceLayout>>,
    ) -> PartialVMResult<()> {
        let common_error = || {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("populate v0 resource group change set error".to_string())
        };

        let create = source_data.is_empty();

        for (struct_tag, current_op) in resources {
            match current_op {
                MoveStorageOp::Delete => {
                    source_data.remove(&struct_tag).ok_or_else(common_error)?;
                },
                MoveStorageOp::Modify((new_data, _)) => {
                    let data = source_data.get_mut(&struct_tag).ok_or_else(common_error)?;
                    *data = new_data;
                },
                MoveStorageOp::New((data, _)) => {
                    let data = source_data.insert(struct_tag, data);
                    if data.is_some() {
                        return Err(common_error());
                    }
                },
            }
        }

        let op = if source_data.is_empty() {
            MoveStorageOp::Delete
        } else if create {
            MoveStorageOp::New((
                bcs::to_bytes(&source_data)
                    .map_err(|_| common_error())?
                    .into(),
                None,
            ))
        } else {
            MoveStorageOp::Modify((
                bcs::to_bytes(&source_data)
                    .map_err(|_| common_error())?
                    .into(),
                None,
            ))
        };
        change_set.insert(state_key, op);
        Ok(())
    }

    /// * Separate the resource groups from the non-resource.
    /// * non-resource groups are kept as is
    /// * resource groups are merged into the correct format as deltas to the source data
    ///   * Remove resource group data from the deltas
    ///   * Attempt to read the existing resource group data or create a new empty container
    ///   * Apply the deltas to the resource group data
    /// The process for translating Move deltas of resource groups to resources is
    /// * Add -- insert element in container
    ///   * If entry exists, Unreachable
    ///   * If group exists, Modify
    ///   * If group doesn't exist, Add
    /// * Modify -- update element in container
    ///   * If group or data doesn't exist, Unreachable
    ///   * Otherwise modify
    /// * Delete -- remove element from container
    ///   * If group or data doesn't exist, Unreachable
    ///   * If elements remain, Modify
    ///   * Otherwise delete
    ///
    /// V1 Resource group change set behavior keeps ops for individual resources separate, not
    /// merging them into a single op corresponding to the whole resource group (V0).
    fn split_and_merge_resource_groups(
        resolver: &impl AptosMoveResolver,
        module_storage: &impl ModuleStorage,
        change_set: ChangeSet,
    ) -> PartialVMResult<(ChangeSet, ResourceGroupChangeSet)> {
        // The use of this implies that we could theoretically call unwrap with no consequences,
        // but using unwrap means the code panics if someone can come up with an attack.
        let common_error = || {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("split_and_merge_resource_groups error".to_string())
        };
        let mut change_set_filtered = ChangeSet::new();

        let mut maybe_resource_group_cache = resolver.release_resource_group_cache().map(|v| {
            v.into_iter()
                .map(|(k, v)| (k, v.into_iter().collect::<BTreeMap<_, _>>()))
                .collect::<BTreeMap<_, _>>()
        });
        let mut resource_group_change_set = if maybe_resource_group_cache.is_some() {
            ResourceGroupChangeSet::V0(BTreeMap::new())
        } else {
            ResourceGroupChangeSet::V1(BTreeMap::new())
        };
        for (addr, account_changeset) in change_set.into_inner() {
            let mut resource_groups: BTreeMap<
                StructTag,
                BTreeMap<StructTag, MoveStorageOp<BytesWithResourceLayout>>,
            > = BTreeMap::new();
            let mut resources_filtered = BTreeMap::new();
            let resources = account_changeset.into_resources();

            for (struct_tag, blob_op) in resources {
                let resource_group_tag = {
                    // INVARIANT:
                    //   We do not need to meter metadata access here. If this resource is in data
                    //   cache, we must have already fetched metadata for its tag.
                    let module = module_storage
                        .unmetered_get_existing_deserialized_module(
                            &struct_tag.address,
                            &struct_tag.module,
                        )
                        .map_err(|e| e.to_partial())?;

                    get_resource_group_member_from_metadata(&struct_tag, &module.metadata)
                };

                if let Some(resource_group_tag) = resource_group_tag {
                    if resource_groups
                        .entry(resource_group_tag)
                        .or_default()
                        .insert(struct_tag, blob_op)
                        .is_some()
                    {
                        return Err(common_error());
                    }
                } else {
                    resources_filtered.insert(struct_tag, blob_op);
                }
            }

            change_set_filtered
                .add_account_changeset(addr, AccountChangeSet::from_resources(resources_filtered))
                .map_err(|_| common_error())?;

            for (resource_group_tag, resources) in resource_groups {
                let state_key = StateKey::resource_group(&addr, &resource_group_tag);
                match &mut resource_group_change_set {
                    ResourceGroupChangeSet::V0(v0_changes) => {
                        let source_data = maybe_resource_group_cache
                            .as_mut()
                            .expect("V0 cache must be set")
                            .remove(&state_key)
                            .unwrap_or_default();
                        Self::populate_v0_resource_group_change_set(
                            v0_changes,
                            state_key,
                            source_data,
                            resources,
                        )?;
                    },
                    ResourceGroupChangeSet::V1(v1_changes) => {
                        // Maintain the behavior of failing the transaction on resource
                        // group member existence invariants.
                        for (struct_tag, current_op) in resources.iter() {
                            let exists =
                                resolver.resource_exists_in_group(&state_key, struct_tag)?;
                            if matches!(current_op, MoveStorageOp::New(_)) == exists {
                                // Deletion and Modification require resource to exist,
                                // while creation requires the resource to not exist.
                                return Err(common_error());
                            }
                        }
                        v1_changes.insert(state_key, resources);
                    },
                }
            }
        }

        Ok((change_set_filtered, resource_group_change_set))
    }

    fn convert_change_set(
        woc: &WriteOpConverter,
        change_set: ChangeSet,
        resource_group_change_set: ResourceGroupChangeSet,
        events: Vec<(ContractEvent, Option<MoveTypeLayout>)>,
        table_change_set: TableChangeSet,
        aggregator_change_set: AggregatorChangeSet,
        legacy_resource_creation_as_modification: bool,
    ) -> PartialVMResult<VMChangeSet> {
        let mut resource_write_set = BTreeMap::new();
        let mut resource_group_write_set = BTreeMap::new();

        let mut aggregator_v1_write_set = BTreeMap::new();
        let mut aggregator_v1_delta_set = BTreeMap::new();

        for (addr, account_changeset) in change_set.into_inner() {
            let resources = account_changeset.into_resources();
            for (struct_tag, blob_and_layout_op) in resources {
                let state_key = resource_state_key(&addr, &struct_tag)?;
                let op = woc.convert_resource(
                    &state_key,
                    blob_and_layout_op,
                    legacy_resource_creation_as_modification,
                )?;

                resource_write_set.insert(state_key, op);
            }
        }

        match resource_group_change_set {
            ResourceGroupChangeSet::V0(v0_changes) => {
                for (state_key, blob_op) in v0_changes {
                    let op = woc.convert_resource(&state_key, blob_op, false)?;
                    resource_write_set.insert(state_key, op);
                }
            },
            ResourceGroupChangeSet::V1(v1_changes) => {
                for (state_key, resources) in v1_changes {
                    let group_write = woc.convert_resource_group_v1(&state_key, resources)?;
                    resource_group_write_set.insert(state_key, group_write);
                }
            },
        }

        for (handle, change) in table_change_set.changes {
            for (key, value_op) in change.entries {
                let state_key = StateKey::table_item(&handle.into(), &key);
                let op = woc.convert_resource(&state_key, value_op, false)?;
                resource_write_set.insert(state_key, op);
            }
        }

        for (state_key, change) in aggregator_change_set.aggregator_v1_changes {
            match change {
                AggregatorChangeV1::Write(value) => {
                    let write_op = woc.convert_aggregator_modification(&state_key, value)?;
                    aggregator_v1_write_set.insert(state_key, write_op);
                },
                AggregatorChangeV1::Merge(delta_op) => {
                    aggregator_v1_delta_set.insert(state_key, delta_op);
                },
                AggregatorChangeV1::Delete => {
                    let write_op =
                        woc.convert_aggregator(&state_key, MoveStorageOp::Delete, false)?;
                    aggregator_v1_write_set.insert(state_key, write_op);
                },
            }
        }

        // We need to remove values that are already in the writes.
        let reads_needing_exchange = aggregator_change_set
            .reads_needing_exchange
            .into_iter()
            .filter(|(state_key, _)| !resource_write_set.contains_key(state_key))
            .collect();

        let group_reads_needing_change = aggregator_change_set
            .group_reads_needing_exchange
            .into_iter()
            .filter(|(state_key, _)| !resource_group_write_set.contains_key(state_key))
            .collect();

        let change_set = VMChangeSet::new_expanded(
            resource_write_set,
            resource_group_write_set,
            aggregator_v1_write_set,
            aggregator_v1_delta_set,
            aggregator_change_set.delayed_field_changes,
            reads_needing_exchange,
            group_reads_needing_change,
            events,
        )?;

        Ok(change_set)
    }
}

/// Converts module bytes and their compiled representation extracted from publish request into
/// write ops. Only used by V2 loader implementation.
pub fn convert_modules_into_write_ops(
    resolver: &impl AptosMoveResolver,
    features: &Features,
    module_storage: &impl AptosModuleStorage,
    verified_module_bundle: VerifiedModuleBundle<ModuleId, Bytes>,
) -> PartialVMResult<BTreeMap<StateKey, ModuleWrite<WriteOp>>> {
    let woc = WriteOpConverter::new(resolver, features.is_storage_slot_metadata_enabled());
    woc.convert_modules_into_write_ops(module_storage, verified_module_bundle.into_iter())
}

/// Initializes and returns Aptos native extensions.
pub(crate) fn make_aptos_extensions<'a, DataView>(
    data_view: &'a DataView,
    chain_id: ChainId,
    vm_config: &VMConfig,
    session_id: SessionId,
    user_transaction_context: Option<UserTransactionContext>,
) -> NativeContextExtensions<'a>
where
    DataView: AptosMoveResolver,
{
    let mut extensions = NativeContextExtensions::default();
    let session_counter = session_id.session_counter();
    let txn_hash = session_id.txn_hash();

    // Note: if any new native functions that return references are added,
    // then runtime reference check models need to be added for them with
    // `extensions.add_native_runtime_ref_checks_model`.
    // See documentation for `NativeRuntimeRefChecksModel` for details.
    extensions.add(NativeTableContext::new(txn_hash, data_view));
    extensions.add(NativeRistrettoPointContext::new());
    extensions.add(AlgebraContext::new());
    extensions.add(NativeAggregatorContext::new(
        txn_hash,
        data_view,
        vm_config.delayed_field_optimization_enabled,
        data_view,
    ));
    extensions.add(RandomnessContext::new());
    extensions.add(NativeTransactionContext::new(
        txn_hash.to_vec(),
        session_id.into_script_hash(),
        chain_id.id(),
        user_transaction_context,
        session_counter,
    ));
    extensions.add(NativeCodeContext::new());
    extensions.add(NativeStateStorageContext::new(data_view));
    extensions.add(NativeEventContext::default());
    extensions.add(NativeObjectContext::default());
    extensions
}
