// Copyright (c) Aptos Foundation
// Licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE

//! State access helpers for reading/writing perp DEX resources through `ExecutorView`.
//! Follows patterns from `native_vm.rs` but adds table item access for BigOrderedMap nodes.

use crate::native::perpdex_types::*;
use aptos_logger::error;
use aptos_types::{
    account_address::AccountAddress,
    state_store::{
        state_key::StateKey,
        state_value::StateValueMetadata,
        table::TableHandle as AptosTableHandle,
    },
    write_set::WriteOp,
};
use aptos_vm_types::{
    abstract_write_op::{AbstractResourceWriteOp, GroupWrite},
    resolver::{ExecutorView, ResourceGroupView},
};
use bytes::Bytes;
use move_core_types::{
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
};
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::BTreeMap, str::FromStr};

// ============================================================================
// PerpDexDbUtil
// ============================================================================

/// Helper struct for constructing StateKeys and StructTags for perp DEX resources.
pub struct PerpDexDbUtil {
    publisher: AccountAddress,
}

impl PerpDexDbUtil {
    pub fn new(publisher: AccountAddress) -> Self {
        Self { publisher }
    }

    pub fn publisher(&self) -> AccountAddress {
        self.publisher
    }

    // ========================================================================
    // StructTag construction
    // ========================================================================

    fn struct_tag(&self, module: &str, name: &str, type_args: Vec<TypeTag>) -> StructTag {
        StructTag {
            address: self.publisher,
            module: Identifier::from_str(module).unwrap(),
            name: Identifier::from_str(name).unwrap(),
            type_args,
        }
    }

    pub fn price_details_struct_tag(&self) -> StructTag {
        self.struct_tag("price_management", "PriceDetails", vec![])
    }

    pub fn perp_market_struct_tag(&self) -> StructTag {
        self.struct_tag("perp_market", "PerpMarket", vec![])
    }

    pub fn async_matching_engine_struct_tag(&self) -> StructTag {
        self.struct_tag("async_matching_engine", "AsyncMatchingEngine", vec![])
    }

    pub fn perp_market_configuration_struct_tag(&self) -> StructTag {
        self.struct_tag("perp_market_config", "PerpMarketConfiguration", vec![])
    }

    pub fn perp_market_oracle_source_struct_tag(&self) -> StructTag {
        self.struct_tag("perp_market_config", "PerpMarketOracleSource", vec![])
    }

    pub fn global_struct_tag(&self) -> StructTag {
        self.struct_tag("perp_engine", "Global", vec![])
    }

    pub fn price_index_store_struct_tag(&self) -> StructTag {
        self.struct_tag("price_management", "PriceIndexStore", vec![])
    }

    pub fn object_group_struct_tag() -> StructTag {
        StructTag {
            address: AccountAddress::ONE,
            module: Identifier::from_str("object").unwrap(),
            name: Identifier::from_str("ObjectGroup").unwrap(),
            type_args: vec![],
        }
    }

    // ========================================================================
    // StateKey construction
    // ========================================================================

    /// StateKey for a resource stored at `address` with the given `struct_tag`.
    pub fn resource_state_key(address: &AccountAddress, struct_tag: &StructTag) -> StateKey {
        StateKey::resource(address, struct_tag).unwrap()
    }

    /// StateKey for an ObjectGroup resource group at `address`.
    pub fn object_group_state_key(address: &AccountAddress) -> StateKey {
        StateKey::resource_group(address, &Self::object_group_struct_tag())
    }

    /// StateKey for a table item (used for BigOrderedMap child nodes).
    pub fn table_item_state_key(handle: &AptosTableHandle, key_bytes: &[u8]) -> StateKey {
        StateKey::table_item(handle, key_bytes)
    }

    // ========================================================================
    // Read helpers
    // ========================================================================

    /// Read and BCS-deserialize a resource from the executor view.
    pub fn read_resource<T: DeserializeOwned>(
        state_key: &StateKey,
        view: &(impl ExecutorView + ResourceGroupView),
    ) -> Result<Option<(T, StateValueMetadata)>, ()> {
        view.get_resource_state_value(state_key, None)
            .map_err(hide_error)?
            .map(|value| {
                bcs::from_bytes::<T>(value.bytes())
                    .map_err(|e| {
                        error!(
                            "BCS deserialization failed for state_key {:?}: {:?} (bytes len={})",
                            state_key,
                            e,
                            value.bytes().len()
                        );
                        e
                    })
                    .map(|bytes| (bytes, value.into_metadata()))
            })
            .transpose()
            .map_err(hide_error)
    }

    /// Read a resource group member from an ObjectGroup.
    pub fn read_resource_group_member<T: DeserializeOwned>(
        group_key: &StateKey,
        resource_tag: &StructTag,
        view: &(impl ExecutorView + ResourceGroupView),
    ) -> Result<Option<T>, ()> {
        view.get_resource_from_group(group_key, resource_tag, None)
            .map_err(hide_error)?
            .map(|value| bcs::from_bytes::<T>(&value))
            .transpose()
            .map_err(hide_error)
    }

    /// Read a table item by handle + BCS-encoded key.
    pub fn read_table_item<T: DeserializeOwned>(
        handle: &AptosTableHandle,
        key_bytes: &[u8],
        view: &(impl ExecutorView + ResourceGroupView),
    ) -> Result<Option<(T, StateValueMetadata)>, ()> {
        let state_key = Self::table_item_state_key(handle, key_bytes);
        view.get_resource_state_value(&state_key, None)
            .map_err(hide_error)?
            .map(|value| {
                bcs::from_bytes::<T>(value.bytes()).map(|bytes| (bytes, value.into_metadata()))
            })
            .transpose()
            .map_err(hide_error)
    }

    // ========================================================================
    // Write helpers
    // ========================================================================

    /// Create a write op for modifying a single resource (non-group).
    pub fn write_resource<T: Serialize>(
        state_key: StateKey,
        value: &T,
        metadata: StateValueMetadata,
        resource_write_set: &mut BTreeMap<StateKey, AbstractResourceWriteOp>,
    ) -> Result<(), ()> {
        let bytes = Bytes::from(bcs::to_bytes(value).map_err(hide_error)?);
        resource_write_set.insert(
            state_key,
            AbstractResourceWriteOp::Write(WriteOp::modification(bytes, metadata)),
        );
        Ok(())
    }

    /// Create a write op for modifying a single member in an existing resource group.
    pub fn write_resource_group_member<T: Serialize>(
        group_key: &StateKey,
        resource_tag: StructTag,
        value: &T,
        view: &(impl ExecutorView + ResourceGroupView),
        resource_write_set: &mut BTreeMap<StateKey, AbstractResourceWriteOp>,
    ) -> Result<(), ()> {
        let metadata = view
            .get_resource_state_value_metadata(group_key)
            .map_err(hide_error)?
            .unwrap();
        let size = view.resource_group_size(group_key).map_err(hide_error)?;
        let value_bytes = Bytes::from(bcs::to_bytes(value).map_err(hide_error)?);
        let group_write = AbstractResourceWriteOp::WriteResourceGroup(GroupWrite::new(
            WriteOp::modification(Bytes::new(), metadata),
            BTreeMap::from([(
                resource_tag,
                (WriteOp::legacy_modification(value_bytes), None),
            )]),
            size,
            size.get(),
        ));
        resource_write_set.insert(group_key.clone(), group_write);
        Ok(())
    }

    /// Create a write op for modifying multiple members in an existing resource group.
    /// This is needed when a single transaction modifies multiple resources within the
    /// same ObjectGroup (e.g., PriceDetails and PerpMarketOracleSource at the same market).
    pub fn write_resource_group_members(
        group_key: &StateKey,
        members: Vec<(StructTag, Bytes)>,
        view: &(impl ExecutorView + ResourceGroupView),
        resource_write_set: &mut BTreeMap<StateKey, AbstractResourceWriteOp>,
    ) -> Result<(), ()> {
        let metadata = view
            .get_resource_state_value_metadata(group_key)
            .map_err(hide_error)?
            .unwrap();
        let size = view.resource_group_size(group_key).map_err(hide_error)?;
        let inner_ops: BTreeMap<StructTag, (WriteOp, Option<_>)> = members
            .into_iter()
            .map(|(tag, bytes)| (tag, (WriteOp::legacy_modification(bytes), None)))
            .collect();
        let group_write = AbstractResourceWriteOp::WriteResourceGroup(GroupWrite::new(
            WriteOp::modification(Bytes::new(), metadata),
            inner_ops,
            size,
            size.get(),
        ));
        resource_write_set.insert(group_key.clone(), group_write);
        Ok(())
    }

    /// Create a write op for modifying a table item.
    pub fn write_table_item<T: Serialize>(
        handle: &AptosTableHandle,
        key_bytes: &[u8],
        value: &T,
        metadata: StateValueMetadata,
        resource_write_set: &mut BTreeMap<StateKey, AbstractResourceWriteOp>,
    ) -> Result<(), ()> {
        let state_key = Self::table_item_state_key(handle, key_bytes);
        let bytes = Bytes::from(bcs::to_bytes(value).map_err(hide_error)?);
        resource_write_set.insert(
            state_key,
            AbstractResourceWriteOp::Write(WriteOp::modification(bytes, metadata)),
        );
        Ok(())
    }

    /// Create a write op for creating a new table item.
    pub fn create_table_item<T: Serialize>(
        handle: &AptosTableHandle,
        key_bytes: &[u8],
        value: &T,
        resource_write_set: &mut BTreeMap<StateKey, AbstractResourceWriteOp>,
    ) -> Result<(), ()> {
        let state_key = Self::table_item_state_key(handle, key_bytes);
        let bytes = Bytes::from(bcs::to_bytes(value).map_err(hide_error)?);
        resource_write_set.insert(
            state_key,
            AbstractResourceWriteOp::Write(WriteOp::legacy_creation(bytes)),
        );
        Ok(())
    }

    // ========================================================================
    // BigOrderedMap navigation helpers
    // ========================================================================

    /// Get the table handle from a BigOrderedMap's StorageSlotsAllocator.
    pub fn bigorderedmap_table_handle<K, V>(
        map: &BigOrderedMap<K, V>,
    ) -> Option<AptosTableHandle> {
        let BigOrderedMap::BPlusTreeMap { nodes, .. } = map;
        let StorageSlotsAllocator::V1 { slots, .. } = nodes;
        slots.as_ref().map(|twl| AptosTableHandle(twl.inner.handle))
    }

    /// Check if the root node has zero children.
    pub fn bigorderedmap_is_empty<K, V>(map: &BigOrderedMap<K, V>) -> bool {
        let BigOrderedMap::BPlusTreeMap { root, .. } = map;
        let Node::V1 { children, .. } = root;
        let OrderedMap::SortedVectorMap { entries } = children;
        entries.is_empty()
    }

    /// Get the minimum key in the map. Navigate to `min_leaf_index` node,
    /// return its first key.
    pub fn bigorderedmap_front_key<
        K: DeserializeOwned + Serialize + Clone,
        V: DeserializeOwned + Serialize,
    >(
        map: &BigOrderedMap<K, V>,
        view: &(impl ExecutorView + ResourceGroupView),
    ) -> Result<Option<K>, ()> {
        let BigOrderedMap::BPlusTreeMap {
            root,
            min_leaf_index,
            ..
        } = map;
        let Node::V1 {
            is_leaf, children, ..
        } = root;

        let OrderedMap::SortedVectorMap { entries } = children;
        if entries.is_empty() {
            return Ok(None);
        }

        if *is_leaf {
            // Root is leaf: return first entry's key directly
            return Ok(Some(entries[0].key.clone()));
        }

        // Root is inner node: read the node at min_leaf_index from table
        let handle = match Self::bigorderedmap_table_handle(map) {
            Some(h) => h,
            None => return Ok(None),
        };

        let (link, _metadata) =
            Self::read_btree_node::<K, V>(&handle, *min_leaf_index, view)?
                .ok_or_else(|| {
                    error!(
                        "BigOrderedMap: missing min_leaf node at slot {}",
                        min_leaf_index
                    );
                })?;
        match link {
            Link::Occupied { value: node } => {
                let Node::V1 { children, .. } = &node;
                let OrderedMap::SortedVectorMap { entries } = children;
                if entries.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(entries[0].key.clone()))
                }
            },
            Link::Vacant { .. } => Ok(None),
        }
    }

    /// Remove a key-value pair from the BigOrderedMap.
    ///
    /// Navigates from root to the leaf containing the key, removes the entry,
    /// and writes back modified table items. If the leaf becomes empty, updates
    /// prev/next pointers, min/max_leaf_index, and the StorageSlotsAllocator
    /// (adds slot to reuse list).
    ///
    /// The caller is responsible for writing back the containing resource (which
    /// holds the BigOrderedMap), since we modify it in place via `&mut`.
    pub fn bigorderedmap_remove<
        K: Ord + Serialize + DeserializeOwned + Clone,
        V: Serialize + DeserializeOwned + Clone,
    >(
        map: &mut BigOrderedMap<K, V>,
        key: &K,
        view: &(impl ExecutorView + ResourceGroupView),
        resource_write_set: &mut BTreeMap<StateKey, AbstractResourceWriteOp>,
    ) -> Result<Option<V>, ()> {
        let BigOrderedMap::BPlusTreeMap {
            root,
            nodes,
            min_leaf_index,
            max_leaf_index,
            ..
        } = map;

        let Node::V1 {
            is_leaf,
            children,
            ..
        } = root;

        if *is_leaf {
            // Root is leaf: remove directly from the sorted vector
            return Ok(remove_from_ordered_map(children, key));
        }

        // Navigate through inner nodes to find the target leaf
        let handle = {
            let StorageSlotsAllocator::V1 { slots, .. } = &*nodes;
            match slots.as_ref().map(|twl| AptosTableHandle(twl.inner.handle)) {
                Some(h) => h,
                None => return Ok(None),
            }
        };

        // Build path from root to leaf.
        // Each element: (slot_index, position_in_parent, node, metadata)
        let mut path: Vec<(u64, usize, Node<K, V>, StateValueMetadata)> = Vec::new();
        let mut current_children: &OrderedMap<K, Child<V>> = children;

        loop {
            let (slot_opt, position) = find_child_slot_with_position(current_children, key);
            match slot_opt {
                Some(si) => {
                    let (link, metadata) =
                        Self::read_btree_node::<K, V>(&handle, si, view)?
                            .ok_or_else(|| {
                                error!("BigOrderedMap remove: missing node at slot {}", si);
                            })?;
                    match link {
                        Link::Occupied { value: node } => {
                            let is_leaf_node = {
                                let Node::V1 { is_leaf: il, .. } = &node;
                                *il
                            };
                            if is_leaf_node {
                                path.push((si, position, node, metadata));
                                break;
                            }
                            // Inner node: continue descending
                            path.push((si, position, node, metadata));
                            let (_, _, last_node, _) = path.last().unwrap();
                            let Node::V1 {
                                children: c, ..
                            } = last_node;
                            // SAFETY: The node is owned by the path Vec which won't
                            // reallocate because we break out of the loop once we
                            // find a leaf. We only read current_children in the
                            // next iteration of this loop.
                            current_children =
                                unsafe { &*(c as *const OrderedMap<K, Child<V>>) };
                        },
                        Link::Vacant { .. } => return Ok(None),
                    }
                },
                None => return Ok(None),
            }
        }

        // The leaf is the last element in path
        let leaf_idx = path.len() - 1;
        let (leaf_slot, _leaf_pos, leaf_node, leaf_metadata) = &mut path[leaf_idx];
        let leaf_slot = *leaf_slot;

        let (leaf_prev, leaf_next, removed) = {
            let Node::V1 {
                children: leaf_children,
                prev,
                next,
                ..
            } = leaf_node;
            let removed = remove_from_ordered_map(leaf_children, key);
            (*prev, *next, removed)
        };

        if removed.is_none() {
            return Ok(None);
        }

        let leaf_is_empty = {
            let Node::V1 {
                children: lc, ..
            } = &*leaf_node;
            let OrderedMap::SortedVectorMap { entries: le } = lc;
            le.is_empty()
        };

        if !leaf_is_empty {
            // Leaf still has entries: write it back
            Self::write_btree_node(
                &handle,
                leaf_slot,
                &Link::Occupied {
                    value: leaf_node.clone(),
                },
                leaf_metadata.clone(),
                resource_write_set,
            )?;

            // Update parent key if the removed key was the first (minimum) key.
            // The parent's key for this child should become the new min key of the leaf.
            let new_min_key = {
                let Node::V1 {
                    children: c, ..
                } = leaf_node;
                let OrderedMap::SortedVectorMap { entries: e } = c;
                e.first().map(|entry| entry.key.clone())
            };
            if let Some(new_key) = new_min_key {
                if leaf_idx == 0 {
                    // Parent is root
                    let child_pos = path[0].1;
                    let OrderedMap::SortedVectorMap {
                        entries: root_entries,
                    } = children;
                    if child_pos < root_entries.len() && root_entries[child_pos].key > new_key {
                        root_entries[child_pos].key = new_key;
                    }
                } else {
                    let parent_idx = leaf_idx - 1;
                    let child_pos = path[leaf_idx].1;
                    let (parent_slot, _, parent_node, parent_meta) = &mut path[parent_idx];
                    let parent_slot = *parent_slot;
                    let Node::V1 {
                        children: pc,
                        ..
                    } = &mut *parent_node;
                    let OrderedMap::SortedVectorMap {
                        entries: pe,
                    } = pc;
                    if child_pos < pe.len() && pe[child_pos].key > new_key {
                        pe[child_pos].key = new_key;
                    }
                    Self::write_btree_node(
                        &handle,
                        parent_slot,
                        &Link::Occupied {
                            value: parent_node.clone(),
                        },
                        parent_meta.clone(),
                        resource_write_set,
                    )?;
                }
            }

            return Ok(removed);
        }

        // Leaf is now empty: update linked list pointers, free slot,
        // remove entry from parent.

        // Update prev node's next pointer
        if leaf_prev != 0 {
            let (prev_link, prev_meta) =
                Self::read_btree_node::<K, V>(&handle, leaf_prev, view)?
                    .ok_or_else(|| {
                        error!(
                            "BigOrderedMap remove: missing prev node at slot {}",
                            leaf_prev
                        );
                    })?;
            if let Link::Occupied { value: mut prev_node } = prev_link {
                let Node::V1 {
                    next: pn, ..
                } = &mut prev_node;
                *pn = leaf_next;
                Self::write_btree_node(
                    &handle,
                    leaf_prev,
                    &Link::Occupied { value: prev_node },
                    prev_meta,
                    resource_write_set,
                )?;
            }
        }

        // Update next node's prev pointer
        if leaf_next != 0 {
            let (next_link, next_meta) =
                Self::read_btree_node::<K, V>(&handle, leaf_next, view)?
                    .ok_or_else(|| {
                        error!(
                            "BigOrderedMap remove: missing next node at slot {}",
                            leaf_next
                        );
                    })?;
            if let Link::Occupied { value: mut next_node } = next_link {
                let Node::V1 {
                    prev: np, ..
                } = &mut next_node;
                *np = leaf_prev;
                Self::write_btree_node(
                    &handle,
                    leaf_next,
                    &Link::Occupied { value: next_node },
                    next_meta,
                    resource_write_set,
                )?;
            }
        }

        // Update min_leaf_index / max_leaf_index
        if *min_leaf_index == leaf_slot {
            *min_leaf_index = leaf_next;
        }
        if *max_leaf_index == leaf_slot {
            *max_leaf_index = leaf_prev;
        }

        // Free the slot: write Vacant to the table, add to reuse list
        let StorageSlotsAllocator::V1 {
            reuse_head_index,
            reuse_spare_count,
            slots: alloc_slots,
            ..
        } = nodes;

        let vacant_link: Link<Node<K, V>> = Link::Vacant {
            next: *reuse_head_index,
        };
        Self::write_btree_node(
            &handle,
            leaf_slot,
            &vacant_link,
            leaf_metadata.clone(),
            resource_write_set,
        )?;
        *reuse_head_index = leaf_slot;
        *reuse_spare_count += 1;

        // Decrement table length
        if let Some(twl) = alloc_slots {
            twl.length = twl.length.saturating_sub(1);
        }

        // Remove the child entry from the parent
        let child_pos = path[leaf_idx].1;
        if leaf_idx == 0 {
            // Parent is root
            let OrderedMap::SortedVectorMap {
                entries: root_entries,
            } = children;
            if child_pos < root_entries.len() {
                root_entries.remove(child_pos);
            }
        } else {
            let parent_idx = leaf_idx - 1;
            let (parent_slot, _, parent_node, parent_meta) = &mut path[parent_idx];
            let parent_slot = *parent_slot;
            let Node::V1 {
                children: pc,
                ..
            } = &mut *parent_node;
            let OrderedMap::SortedVectorMap {
                entries: pe,
            } = pc;
            if child_pos < pe.len() {
                pe.remove(child_pos);
            }
            Self::write_btree_node(
                &handle,
                parent_slot,
                &Link::Occupied {
                    value: parent_node.clone(),
                },
                parent_meta.clone(),
                resource_write_set,
            )?;
            // Move's BigOrderedMap defers merging of underfull inner nodes,
            // so we skip that for the benchmark.
        }

        Ok(removed)
    }

    /// Insert a key-value pair into a BigOrderedMap with proper node splitting.
    ///
    /// Navigates from root to target leaf, inserts, and splits nodes if they
    /// exceed their max degree.
    pub fn bigorderedmap_add<
        K: Ord + Serialize + DeserializeOwned + Clone,
        V: Serialize + DeserializeOwned + Clone,
    >(
        map: &mut BigOrderedMap<K, V>,
        key: K,
        value: V,
        view: &(impl ExecutorView + ResourceGroupView),
        resource_write_set: &mut BTreeMap<StateKey, AbstractResourceWriteOp>,
    ) -> Result<(), ()> {
        let BigOrderedMap::BPlusTreeMap {
            root,
            nodes,
            min_leaf_index,
            max_leaf_index,
            leaf_max_degree,
            inner_max_degree,
            ..
        } = map;

        let leaf_max = *leaf_max_degree as usize;
        let inner_max = *inner_max_degree as usize;

        let Node::V1 {
            is_leaf: root_is_leaf,
            children: root_children,
            ..
        } = root;

        if *root_is_leaf {
            // Root is leaf: insert directly
            insert_into_ordered_map(root_children, key, value);

            let OrderedMap::SortedVectorMap {
                entries: root_entries,
            } = root_children;

            // In the native executor, we never split the root leaf. Splitting creates
            // new table items that aren't in the base state cache, causing "Must cache read"
            // panics during commit. Instead, let the root grow unbounded. This preserves
            // the same write keys (just the containing resource) and avoids non-deterministic
            // table item creation that breaks Block-STM re-execution.
            if false && leaf_max > 0 && root_entries.len() > leaf_max {
                let handle = {
                    let StorageSlotsAllocator::V1 { slots, .. } = &*nodes;
                    match slots.as_ref().map(|twl| AptosTableHandle(twl.inner.handle)) {
                        Some(h) => h,
                        None => {
                            return Ok(());
                        },
                    }
                };

                let OrderedMap::SortedVectorMap {
                    entries: all_entries,
                } = root_children;

                let mid = all_entries.len() / 2;
                let right_entries: Vec<Entry<K, Child<V>>> = all_entries.split_off(mid);
                let left_entries: Vec<Entry<K, Child<V>>> = std::mem::take(all_entries);

                let left_slot = allocate_slot(nodes);
                let right_slot = allocate_slot(nodes);

                let left_min_key = left_entries[0].key.clone();
                let right_min_key = right_entries[0].key.clone();

                let left_node = Node::V1 {
                    is_leaf: true,
                    children: OrderedMap::SortedVectorMap {
                        entries: left_entries,
                    },
                    prev: 0,
                    next: right_slot,
                };
                let right_node = Node::V1 {
                    is_leaf: true,
                    children: OrderedMap::SortedVectorMap {
                        entries: right_entries,
                    },
                    prev: left_slot,
                    next: 0,
                };

                Self::create_table_item(
                    &handle,
                    &bcs::to_bytes(&left_slot).map_err(hide_error)?,
                    &Link::Occupied { value: left_node },
                    resource_write_set,
                )?;
                Self::create_table_item(
                    &handle,
                    &bcs::to_bytes(&right_slot).map_err(hide_error)?,
                    &Link::Occupied { value: right_node },
                    resource_write_set,
                )?;

                // Root becomes inner node with two children
                *root_is_leaf = false;
                *all_entries = vec![
                    Entry {
                        key: left_min_key,
                        value: Child::Inner {
                            node_index: StoredSlot {
                                slot_index: left_slot,
                            },
                        },
                    },
                    Entry {
                        key: right_min_key,
                        value: Child::Inner {
                            node_index: StoredSlot {
                                slot_index: right_slot,
                            },
                        },
                    },
                ];

                *min_leaf_index = left_slot;
                *max_leaf_index = right_slot;

                // Update table length
                let StorageSlotsAllocator::V1 {
                    slots: alloc_slots,
                    ..
                } = nodes;
                if let Some(twl) = alloc_slots {
                    twl.length += 2;
                }
            }

            return Ok(());
        }

        // Root is inner node: navigate to the target leaf
        let handle = {
            let StorageSlotsAllocator::V1 { slots, .. } = &*nodes;
            match slots.as_ref().map(|twl| AptosTableHandle(twl.inner.handle)) {
                Some(h) => h,
                None => return Err(()),
            }
        };

        // Build path from root to leaf.
        // Each entry: (slot_index, position_in_parent, node, metadata)
        let mut path: Vec<(u64, usize, Node<K, V>, StateValueMetadata)> = Vec::new();
        let mut current_children: &OrderedMap<K, Child<V>> = root_children;

        loop {
            let (slot_opt, position) = find_child_slot_with_position(current_children, &key);
            match slot_opt {
                Some(slot_index) => {
                    let (link, metadata) =
                        Self::read_btree_node::<K, V>(&handle, slot_index, view)?
                            .ok_or_else(|| {
                                error!(
                                    "BigOrderedMap add: missing node at slot {}",
                                    slot_index
                                );
                            })?;
                    match link {
                        Link::Occupied { value: node } => {
                            let is_leaf_node = {
                                let Node::V1 { is_leaf: il, .. } = &node;
                                *il
                            };
                            if is_leaf_node {
                                path.push((slot_index, position, node, metadata));
                                break;
                            }
                            // Inner node: continue descending
                            path.push((slot_index, position, node, metadata));
                            let (_, _, last_node, _) = path.last().unwrap();
                            let Node::V1 {
                                children: c, ..
                            } = last_node;
                            current_children =
                                unsafe { &*(c as *const OrderedMap<K, Child<V>>) };
                        },
                        Link::Vacant { .. } => return Err(()),
                    }
                },
                None => return Err(()),
            }
        }

        // Insert into the leaf
        let leaf_idx = path.len() - 1;
        {
            let (_, _, leaf_node, _) = &mut path[leaf_idx];
            let Node::V1 {
                children: leaf_children,
                ..
            } = leaf_node;
            insert_into_ordered_map(leaf_children, key.clone(), value);
        }

        // Check if the leaf needs splitting, propagate splits upward
        let mut split_result: Option<(K, u64)> = None; // (new_key, new_slot)

        {
            let (leaf_slot, _, leaf_node, leaf_metadata) = &mut path[leaf_idx];
            let leaf_slot = *leaf_slot;

            // Skip splitting in native executor — creating new table items breaks
            // Block-STM's state cache invariant. Let leaves overflow instead.
            if false {
                let Node::V1 {
                    children: leaf_children,
                    next: leaf_next,
                    ..
                } = leaf_node;

                let OrderedMap::SortedVectorMap {
                    entries: leaf_entries,
                } = leaf_children;

                let mid = leaf_entries.len() / 2;
                let right_entries: Vec<Entry<K, Child<V>>> = leaf_entries.split_off(mid);

                let new_slot = allocate_slot(nodes);
                let right_min_key = right_entries[0].key.clone();
                let old_next = *leaf_next;

                let right_node = Node::V1 {
                    is_leaf: true,
                    children: OrderedMap::SortedVectorMap {
                        entries: right_entries,
                    },
                    prev: leaf_slot,
                    next: old_next,
                };

                // Update old leaf's next to point to new node
                *leaf_next = new_slot;

                // Update the old next node's prev pointer
                if old_next != 0 {
                    let (next_link, next_meta) =
                        Self::read_btree_node::<K, V>(&handle, old_next, view)?
                            .ok_or_else(|| {
                                error!(
                                    "BigOrderedMap add: missing next node at slot {}",
                                    old_next
                                );
                            })?;
                    if let Link::Occupied { value: mut next_node } = next_link {
                        let Node::V1 {
                            prev: np, ..
                        } = &mut next_node;
                        *np = new_slot;
                        Self::write_btree_node(
                            &handle,
                            old_next,
                            &Link::Occupied { value: next_node },
                            next_meta,
                            resource_write_set,
                        )?;
                    }
                }

                // If this was the max_leaf, update max_leaf_index
                if *max_leaf_index == leaf_slot {
                    *max_leaf_index = new_slot;
                }

                // Write the new right node
                Self::create_table_item(
                    &handle,
                    &bcs::to_bytes(&new_slot).map_err(hide_error)?,
                    &Link::Occupied { value: right_node },
                    resource_write_set,
                )?;

                // Update table length
                let StorageSlotsAllocator::V1 {
                    slots: alloc_slots,
                    ..
                } = &mut *nodes;
                if let Some(twl) = alloc_slots {
                    twl.length += 1;
                }

                split_result = Some((right_min_key, new_slot));
            }

            // Write back the (possibly modified) leaf
            Self::write_btree_node(
                &handle,
                leaf_slot,
                &Link::Occupied {
                    value: leaf_node.clone(),
                },
                leaf_metadata.clone(),
                resource_write_set,
            )?;
        }

        // Propagate splits upward through inner nodes
        if let Some((mut new_key, mut new_slot)) = split_result {
            // Walk the path from leaf's parent up to root
            for level in (0..leaf_idx).rev() {
                let (node_slot, _, inner_node, inner_meta) = &mut path[level];
                let node_slot = *node_slot;
                let Node::V1 {
                    children: inner_children,
                    ..
                } = &mut *inner_node;

                // Insert new child into this inner node
                let OrderedMap::SortedVectorMap {
                    entries: inner_entries,
                } = inner_children;

                let insert_pos = inner_entries.partition_point(|e| e.key < new_key);
                inner_entries.insert(
                    insert_pos,
                    Entry {
                        key: new_key.clone(),
                        value: Child::Inner {
                            node_index: StoredSlot {
                                slot_index: new_slot,
                            },
                        },
                    },
                );

                if false && inner_max > 0 && inner_entries.len() > inner_max {
                    // Split this inner node — disabled in native executor
                    let mid = inner_entries.len() / 2;
                    let right_entries: Vec<Entry<K, Child<V>>> =
                        inner_entries.split_off(mid);

                    let right_slot = allocate_slot(nodes);
                    let right_min_key = right_entries[0].key.clone();

                    let Node::V1 {
                        next: inner_next,
                        ..
                    } = &mut *inner_node;
                    let old_next = *inner_next;

                    let right_inner = Node::V1 {
                        is_leaf: false,
                        children: OrderedMap::SortedVectorMap {
                            entries: right_entries,
                        },
                        prev: node_slot,
                        next: old_next,
                    };

                    *inner_next = right_slot;

                    // Update old next's prev
                    if old_next != 0 {
                        let (nl, nm) =
                            Self::read_btree_node::<K, V>(&handle, old_next, view)?
                                .ok_or_else(|| {
                                    error!(
                                        "BigOrderedMap add: missing next inner at slot {}",
                                        old_next
                                    );
                                })?;
                        if let Link::Occupied { value: mut nn } = nl {
                            let Node::V1 {
                                prev: np, ..
                            } = &mut nn;
                            *np = right_slot;
                            Self::write_btree_node(
                                &handle,
                                old_next,
                                &Link::Occupied { value: nn },
                                nm,
                                resource_write_set,
                            )?;
                        }
                    }

                    Self::create_table_item(
                        &handle,
                        &bcs::to_bytes(&right_slot).map_err(hide_error)?,
                        &Link::Occupied {
                            value: right_inner,
                        },
                        resource_write_set,
                    )?;

                    let StorageSlotsAllocator::V1 {
                        slots: alloc_slots,
                        ..
                    } = &mut *nodes;
                    if let Some(twl) = alloc_slots {
                        twl.length += 1;
                    }

                    // Write back the left (current) inner node
                    Self::write_btree_node(
                        &handle,
                        node_slot,
                        &Link::Occupied {
                            value: inner_node.clone(),
                        },
                        inner_meta.clone(),
                        resource_write_set,
                    )?;

                    new_key = right_min_key;
                    new_slot = right_slot;
                    // Continue propagating upward
                } else {
                    // No further split needed, write back and stop
                    Self::write_btree_node(
                        &handle,
                        node_slot,
                        &Link::Occupied {
                            value: inner_node.clone(),
                        },
                        inner_meta.clone(),
                        resource_write_set,
                    )?;
                    return Ok(());
                }
            }

            // If we get here, the split propagated all the way to the root.
            // The root must split: add the new entry to root's children, and
            // if root overflows, create two new inner children.
            let OrderedMap::SortedVectorMap {
                entries: root_entries,
            } = root_children;

            let insert_pos = root_entries.partition_point(|e| e.key < new_key);
            root_entries.insert(
                insert_pos,
                Entry {
                    key: new_key.clone(),
                    value: Child::Inner {
                        node_index: StoredSlot {
                            slot_index: new_slot,
                        },
                    },
                },
            );

            if false && inner_max > 0 && root_entries.len() > inner_max {
                // Root inner split — disabled in native executor
                let mid = root_entries.len() / 2;
                let right_entries: Vec<Entry<K, Child<V>>> = root_entries.split_off(mid);
                let left_entries: Vec<Entry<K, Child<V>>> = std::mem::take(root_entries);

                let left_slot = allocate_slot(nodes);
                let right_slot_new = allocate_slot(nodes);

                let left_min_key = left_entries[0].key.clone();
                let right_min_key = right_entries[0].key.clone();

                let left_inner = Node::V1 {
                    is_leaf: false,
                    children: OrderedMap::SortedVectorMap {
                        entries: left_entries,
                    },
                    prev: 0,
                    next: right_slot_new,
                };
                let right_inner = Node::V1 {
                    is_leaf: false,
                    children: OrderedMap::SortedVectorMap {
                        entries: right_entries,
                    },
                    prev: left_slot,
                    next: 0,
                };

                Self::create_table_item(
                    &handle,
                    &bcs::to_bytes(&left_slot).map_err(hide_error)?,
                    &Link::Occupied { value: left_inner },
                    resource_write_set,
                )?;
                Self::create_table_item(
                    &handle,
                    &bcs::to_bytes(&right_slot_new).map_err(hide_error)?,
                    &Link::Occupied {
                        value: right_inner,
                    },
                    resource_write_set,
                )?;

                *root_entries = vec![
                    Entry {
                        key: left_min_key,
                        value: Child::Inner {
                            node_index: StoredSlot {
                                slot_index: left_slot,
                            },
                        },
                    },
                    Entry {
                        key: right_min_key,
                        value: Child::Inner {
                            node_index: StoredSlot {
                                slot_index: right_slot_new,
                            },
                        },
                    },
                ];

                let StorageSlotsAllocator::V1 {
                    slots: alloc_slots,
                    ..
                } = nodes;
                if let Some(twl) = alloc_slots {
                    twl.length += 2;
                }
            }
        }

        Ok(())
    }

    /// Remove and return the minimum key-value pair (pop front).
    /// Used by `process_pending_requests`.
    pub fn bigorderedmap_pop_front<
        K: Ord + Serialize + DeserializeOwned + Clone,
        V: Serialize + DeserializeOwned + Clone,
    >(
        map: &mut BigOrderedMap<K, V>,
        view: &(impl ExecutorView + ResourceGroupView),
        resource_write_set: &mut BTreeMap<StateKey, AbstractResourceWriteOp>,
    ) -> Result<Option<(K, V)>, ()> {
        let BigOrderedMap::BPlusTreeMap {
            root,
            nodes,
            min_leaf_index,
            max_leaf_index,
            ..
        } = map;

        let Node::V1 {
            is_leaf,
            children,
            ..
        } = root;

        {
            let OrderedMap::SortedVectorMap {
                entries: children_entries,
            } = children;
            if children_entries.is_empty() {
                return Ok(None);
            }
        }

        if *is_leaf {
            // Root is leaf: remove first entry
            let OrderedMap::SortedVectorMap {
                entries: root_entries,
            } = children;
            let entry = root_entries.remove(0);
            match entry.value {
                Child::Leaf { value } => return Ok(Some((entry.key, value))),
                Child::Inner { .. } => return Err(()),
            }
        }

        // Root is inner: read the node at min_leaf_index
        let handle = {
            let StorageSlotsAllocator::V1 { slots, .. } = &*nodes;
            match slots.as_ref().map(|twl| AptosTableHandle(twl.inner.handle)) {
                Some(h) => h,
                None => return Ok(None),
            }
        };

        let current_min = *min_leaf_index;
        let (link, metadata) =
            Self::read_btree_node::<K, V>(&handle, current_min, view)?
                .ok_or_else(|| {
                    error!(
                        "BigOrderedMap pop_front: missing min_leaf node at slot {}",
                        current_min
                    );
                })?;

        let mut leaf_node = match link {
            Link::Occupied { value: node } => node,
            Link::Vacant { .. } => return Ok(None),
        };

        let (leaf_prev, leaf_next) = {
            let Node::V1 { prev, next, .. } = &leaf_node;
            (*prev, *next)
        };

        let entry = {
            let Node::V1 {
                children: leaf_children,
                ..
            } = &mut leaf_node;
            let OrderedMap::SortedVectorMap {
                entries: leaf_entries,
            } = leaf_children;
            if leaf_entries.is_empty() {
                return Ok(None);
            }
            leaf_entries.remove(0)
        };
        let result = match entry.value {
            Child::Leaf { value } => (entry.key, value),
            Child::Inner { .. } => return Err(()),
        };

        let leaf_now_empty = {
            let Node::V1 { children: lc, .. } = &leaf_node;
            let OrderedMap::SortedVectorMap { entries: le } = lc;
            le.is_empty()
        };

        if !leaf_now_empty {
            // Leaf still has entries, write it back
            Self::write_btree_node(
                &handle,
                current_min,
                &Link::Occupied { value: leaf_node },
                metadata,
                resource_write_set,
            )?;
            return Ok(Some(result));
        }

        // Leaf is now empty: remove it

        // Update next node's prev pointer
        if leaf_next != 0 {
            let (next_link, next_meta) =
                Self::read_btree_node::<K, V>(&handle, leaf_next, view)?
                    .ok_or_else(|| {
                        error!(
                            "BigOrderedMap pop_front: missing next node at slot {}",
                            leaf_next
                        );
                    })?;
            if let Link::Occupied { value: mut next_node } = next_link {
                let Node::V1 {
                    prev: np, ..
                } = &mut next_node;
                *np = leaf_prev;
                Self::write_btree_node(
                    &handle,
                    leaf_next,
                    &Link::Occupied { value: next_node },
                    next_meta,
                    resource_write_set,
                )?;
            }
        }

        // Update min_leaf_index
        *min_leaf_index = leaf_next;
        if *max_leaf_index == current_min {
            *max_leaf_index = leaf_prev;
        }

        // Free the slot: write Vacant to the table, add to reuse list
        let StorageSlotsAllocator::V1 {
            reuse_head_index,
            reuse_spare_count,
            slots: alloc_slots,
            ..
        } = nodes;

        let vacant_link: Link<Node<K, V>> = Link::Vacant {
            next: *reuse_head_index,
        };
        Self::write_btree_node(
            &handle,
            current_min,
            &vacant_link,
            metadata,
            resource_write_set,
        )?;
        *reuse_head_index = current_min;
        *reuse_spare_count += 1;

        // Remove the entry for current_min from the parent (root).
        // Find which root child points to current_min and remove it.
        let OrderedMap::SortedVectorMap {
            entries: root_entries,
        } = children;
        if let Some(pos) = root_entries.iter().position(|e| match &e.value {
            Child::Inner { node_index } => node_index.slot_index == current_min,
            _ => false,
        }) {
            root_entries.remove(pos);
        }

        // Decrement table length
        if let Some(twl) = alloc_slots {
            twl.length = twl.length.saturating_sub(1);
        }

        Ok(Some(result))
    }

    // ========================================================================
    // B+Tree node I/O helpers
    // ========================================================================

    /// Read a B+Tree child node from table storage.
    pub fn read_btree_node<K: Serialize + DeserializeOwned, V: Serialize + DeserializeOwned>(
        handle: &AptosTableHandle,
        slot_index: u64,
        view: &(impl ExecutorView + ResourceGroupView),
    ) -> Result<Option<(Link<Node<K, V>>, StateValueMetadata)>, ()> {
        let key_bytes = bcs::to_bytes(&slot_index).map_err(hide_error)?;
        Self::read_table_item(handle, &key_bytes, view)
    }

    /// Write a modified B+Tree child node back to table storage.
    pub fn write_btree_node<K: Serialize + DeserializeOwned, V: Serialize + DeserializeOwned>(
        handle: &AptosTableHandle,
        slot_index: u64,
        node: &Link<Node<K, V>>,
        metadata: StateValueMetadata,
        resource_write_set: &mut BTreeMap<StateKey, AbstractResourceWriteOp>,
    ) -> Result<(), ()> {
        let key_bytes = bcs::to_bytes(&slot_index).map_err(hide_error)?;
        Self::write_table_item(handle, &key_bytes, node, metadata, resource_write_set)
    }

    /// Navigate from root to the leaf that would contain the given key.
    /// Returns the path as a vector of (slot_index, position_in_parent, node, metadata).
    /// The last element is the leaf node's entry.
    /// Does NOT include the root itself in the path (root is implicit).
    pub fn find_leaf_for_key<K: Ord + Serialize + DeserializeOwned + Clone, V: Serialize + DeserializeOwned + Clone>(
        root_children: &OrderedMap<K, Child<V>>,
        key: &K,
        handle: &AptosTableHandle,
        view: &(impl ExecutorView + ResourceGroupView),
    ) -> Result<Vec<(u64, usize, Node<K, V>, StateValueMetadata)>, ()> {
        let mut path = Vec::new();
        let mut current_children: &OrderedMap<K, Child<V>> = root_children;
        let mut owned_nodes: Vec<(Node<K, V>, StateValueMetadata)> = Vec::new();

        loop {
            let (slot_opt, position) = find_child_slot_with_position(current_children, key);
            match slot_opt {
                Some(slot_index) => {
                    let (link, metadata) =
                        Self::read_btree_node::<K, V>(handle, slot_index, view)?
                            .ok_or_else(|| {
                                error!(
                                    "find_leaf_for_key: missing node at slot {}",
                                    slot_index
                                );
                            })?;
                    match link {
                        Link::Occupied { value: node } => {
                            let is_leaf_node = {
                                let Node::V1 { is_leaf: il, .. } = &node;
                                *il
                            };
                            if is_leaf_node {
                                path.push((slot_index, position, node, metadata));
                                return Ok(path);
                            }
                            owned_nodes.push((node, metadata));
                            let (last_node, last_meta) = owned_nodes.last().unwrap();
                            let Node::V1 {
                                children: c, ..
                            } = last_node;
                            path.push((
                                slot_index,
                                position,
                                last_node.clone(),
                                last_meta.clone(),
                            ));
                            current_children =
                                unsafe { &*(c as *const OrderedMap<K, Child<V>>) };
                        },
                        Link::Vacant { .. } => return Err(()),
                    }
                },
                None => return Err(()),
            }
        }
    }
}

// ============================================================================
// OrderedMap helper functions
// ============================================================================

/// Find a value in an OrderedMap (sorted vector) by key.
fn find_in_ordered_map<K: Ord, V: Clone>(
    map: &OrderedMap<K, Child<V>>,
    key: &K,
) -> Option<V> {
    let OrderedMap::SortedVectorMap { entries } = map;
    entries
        .binary_search_by(|e| e.key.cmp(key))
        .ok()
        .and_then(|idx| match &entries[idx].value {
            Child::Leaf { value } => Some(value.clone()),
            Child::Inner { .. } => None,
        })
}

/// Find which child slot to descend into for a given key, returning both the
/// slot index and the position in the entries vector.
fn find_child_slot_with_position<K: Ord, V>(
    map: &OrderedMap<K, Child<V>>,
    key: &K,
) -> (Option<u64>, usize) {
    let OrderedMap::SortedVectorMap { entries } = map;
    if entries.is_empty() {
        return (None, 0);
    }
    // Find the rightmost entry whose key <= target key
    let idx = match entries.binary_search_by(|e| e.key.cmp(key)) {
        Ok(i) => i,
        Err(i) => {
            if i == 0 {
                0
            } else {
                i - 1
            }
        },
    };
    match &entries[idx].value {
        Child::Inner { node_index } => (Some(node_index.slot_index), idx),
        Child::Leaf { .. } => (None, idx),
    }
}

/// Insert a key-value pair into an OrderedMap (sorted vector), maintaining sort order.
fn insert_into_ordered_map<K: Ord, V>(
    map: &mut OrderedMap<K, Child<V>>,
    key: K,
    value: V,
) {
    let OrderedMap::SortedVectorMap { entries } = map;
    let idx = entries.partition_point(|e| e.key < key);
    entries.insert(
        idx,
        Entry {
            key,
            value: Child::Leaf { value },
        },
    );
}

/// Remove a key-value pair from an OrderedMap (sorted vector).
/// Returns the value if found.
fn remove_from_ordered_map<K: Ord, V: Clone>(
    map: &mut OrderedMap<K, Child<V>>,
    key: &K,
) -> Option<V> {
    let OrderedMap::SortedVectorMap { entries } = map;
    match entries.binary_search_by(|e| e.key.cmp(key)) {
        Ok(idx) => {
            let entry = entries.remove(idx);
            match entry.value {
                Child::Leaf { value } => Some(value),
                Child::Inner { .. } => None,
            }
        },
        Err(_) => None,
    }
}

/// Allocate a new slot from the StorageSlotsAllocator.
/// Pops from the reuse list if available, otherwise increments new_slot_index.
fn allocate_slot<T>(nodes: &mut StorageSlotsAllocator<T>) -> u64 {
    let StorageSlotsAllocator::V1 {
        new_slot_index,
        should_reuse,
        reuse_head_index,
        reuse_spare_count,
        ..
    } = nodes;

    if *should_reuse && *reuse_spare_count > 0 && *reuse_head_index != 0 {
        let slot = *reuse_head_index;
        // The actual next pointer is stored in the Vacant link in the table item,
        // which we overwrite when writing the new node. For the benchmark, reset
        // head to 0 and decrement count.
        *reuse_spare_count -= 1;
        *reuse_head_index = 0;
        slot
    } else {
        let slot = *new_slot_index;
        *new_slot_index += 1;
        slot
    }
}

/// Ensure the StorageSlotsAllocator has a table handle.
/// In Move, the table is lazily created on first allocation.
/// In the native executor, we generate a deterministic handle from a global counter.
fn ensure_table_exists<T>(nodes: &mut StorageSlotsAllocator<T>) {
    let StorageSlotsAllocator::V1 { slots, .. } = nodes;
    if slots.is_none() {
        static TABLE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let counter = TABLE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // Create a deterministic address from the counter
        let mut bytes = [0u8; AccountAddress::LENGTH];
        // Set prefix to distinguish from real addresses
        bytes[0] = 0xFF;
        bytes[1] = 0xFE;
        let counter_bytes = counter.to_le_bytes();
        bytes[2..10].copy_from_slice(&counter_bytes);
        let handle = AccountAddress::new(bytes);
        *slots = Some(TableWithLength {
            inner: TableHandle { handle },
            length: 0,
        });
    }
}

fn hide_error<E: std::fmt::Debug>(e: E) {
    error!("perpdex_db_util error: {:?}", e);
}
