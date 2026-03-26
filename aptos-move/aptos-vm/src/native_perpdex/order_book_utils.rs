// Copyright (c) Aptos Foundation
// Translated from: aptos_market::order_book_utils

/// In Move, BigOrderedMap is a B-tree backed by table storage.
/// In native Rust, we use BTreeMap as a placeholder. Actual table operations
/// are annotated with TABLE_OP comments.
///
/// The Move version configures BigOrderedMap with inner_degree=64, leaf_degree=32.
/// Here we use BTreeMap which has its own internal branching factor.

use std::collections::BTreeMap;

/// TableHandle represents a handle to an on-chain table resource.
/// In the native context, this is a placeholder for the actual storage handle.
pub type TableHandle = u64;

/// A monomorphized BigOrderedMap replacement using BTreeMap.
/// In production, this would interact with the table storage layer.
/// For now, BTreeMap provides the same ordered-map semantics.
#[derive(Clone, Debug)]
pub struct BigOrderedMap<K: Ord + Clone, V: Clone> {
    pub inner: BTreeMap<K, V>,
}

impl<K: Ord + Clone, V: Clone> BigOrderedMap<K, V> {
    pub fn new() -> Self {
        BigOrderedMap {
            inner: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn contains(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    pub fn add(&mut self, key: K, value: V) {
        // TABLE_OP: write table_handle[key] = value
        self.inner.insert(key, value);
    }

    pub fn remove(&mut self, key: &K) -> V {
        // TABLE_OP: remove table_handle[key]
        self.inner.remove(key).expect("key not found in BigOrderedMap::remove")
    }

    /// Safe remove that returns None if key doesn't exist.
    /// Replaces the Move inline `remove_or_none` function.
    pub fn remove_or_none(&mut self, key: &K) -> Option<V> {
        // TABLE_OP: read/remove table_handle[key]
        self.inner.remove(key)
    }

    pub fn get(&self, key: &K) -> Option<V> {
        // TABLE_OP: read table_handle[key]
        self.inner.get(key).cloned()
    }

    pub fn borrow(&self, key: &K) -> &V {
        // TABLE_OP: read table_handle[key]
        self.inner.get(key).expect("key not found in BigOrderedMap::borrow")
    }

    pub fn borrow_mut(&mut self, key: &K) -> &mut V {
        // TABLE_OP: read/write table_handle[key]
        self.inner.get_mut(key).expect("key not found in BigOrderedMap::borrow_mut")
    }

    /// Borrow the front (smallest key) entry.
    pub fn borrow_front(&self) -> (&K, &V) {
        // TABLE_OP: read table_handle[min_key]
        self.inner.iter().next().expect("BigOrderedMap is empty in borrow_front")
    }

    /// Borrow the back (largest key) entry.
    pub fn borrow_back(&self) -> (&K, &V) {
        // TABLE_OP: read table_handle[max_key]
        self.inner.iter().next_back().expect("BigOrderedMap is empty in borrow_back")
    }

    /// Upsert: insert or replace. Returns the old value if it existed.
    pub fn upsert(&mut self, key: K, value: V) -> Option<V> {
        // TABLE_OP: write table_handle[key] = value
        self.inner.insert(key, value)
    }

    /// Apply a function to the value if the key is present. Returns true if key was present.
    pub fn modify_if_present<F: FnOnce(&mut V)>(&mut self, key: &K, f: F) -> bool {
        // TABLE_OP: read/write table_handle[key]
        if let Some(v) = self.inner.get_mut(key) {
            f(v);
            true
        } else {
            false
        }
    }

    /// Apply a function to the value and return a copy, if the key is present.
    pub fn modify_if_present_and_return<F: FnOnce(&mut V) -> V>(
        &mut self,
        key: &K,
        f: F,
    ) -> Option<V> {
        // TABLE_OP: read/write table_handle[key]
        if let Some(v) = self.inner.get_mut(key) {
            let result = f(v);
            Some(result)
        } else {
            None
        }
    }

    /// Apply a function to the value and return a clone of the result.
    pub fn modify_and_return<F: FnOnce(&mut V) -> V>(&mut self, key: &K, f: F) -> V {
        // TABLE_OP: read/write table_handle[key]
        let v = self.inner.get_mut(key).expect("key not found in modify_and_return");
        f(v)
    }

    /// Get and map: apply a function to the value if present, return the result as Option.
    pub fn get_and_map<R, F: FnOnce(&V) -> R>(&self, key: &K, f: F) -> Option<R> {
        // TABLE_OP: read table_handle[key]
        self.inner.get(key).map(f)
    }

    /// Destroy (consume) the map, calling a function for each value.
    pub fn destroy<F: FnMut(V)>(self, mut f: F) {
        for (_k, v) in self.inner {
            f(v);
        }
    }

    /// Return all keys (for test purposes mainly).
    pub fn keys(&self) -> Vec<K> {
        self.inner.keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<K: Ord + Clone, V: Clone> Default for BigOrderedMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

/// Creates a new BigOrderedMap with default configuration.
/// In Move: big_ordered_map::new_with_config(64, 32, true)
pub fn new_default_big_ordered_map<K: Ord + Clone, V: Clone>() -> BigOrderedMap<K, V> {
    BigOrderedMap::new()
}
