// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Thread-local source-location registry for the Move VM debugger.
//!
//! The VM runtime has no dependency on Aptos-specific packages; source maps are
//! stored in on-chain metadata that only the Aptos layer can access.  This
//! module provides a type-erased trait so the Aptos layer can register a
//! provider once per replay and the deep interpreter code can call it without
//! introducing a crate dependency on Aptos.

use move_binary_format::file_format::FunctionDefinitionIndex;
use move_core_types::language_storage::ModuleId;
use std::{cell::RefCell, sync::Arc};

// ── Public trait ─────────────────────────────────────────────────────────────

/// Provides source-level information that is only available to the Aptos layer.
///
/// Implementors live in `aptos-move/aptos-debugger`; the VM runtime only holds
/// a `dyn SourceLocator` behind an `Arc`.
pub trait SourceLocator: Send + Sync {
    /// Return `"filename:line"` for the bytecode position `pc` inside the
    /// function identified by `func_def_idx` of `module_id`, or `None` when
    /// no source map is available for that function.
    fn locate(
        &self,
        module_id: &ModuleId,
        func_def_idx: FunctionDefinitionIndex,
        pc: u16,
    ) -> Option<String>;

    /// Return `(param_count, names)` where `names` is the concatenation of
    /// parameter names followed by local-variable names for the given function.
    /// Returns `None` when no source map is available.
    fn get_function_param_and_local_names(
        &self,
        module_id: &ModuleId,
        func_def_idx: FunctionDefinitionIndex,
    ) -> Option<(usize, Vec<String>)>;

    /// Return the ordered list of field names for the struct `struct_name`
    /// defined in `module_id`, or `None` when no information is available.
    fn get_struct_field_names(
        &self,
        module_id: &ModuleId,
        struct_name: &str,
    ) -> Option<Vec<String>>;
}

// ── Thread-local storage ─────────────────────────────────────────────────────

thread_local! {
    static LOCATOR: RefCell<Option<Arc<dyn SourceLocator>>> = RefCell::new(None);
}

/// Install `loc` as the source locator for the current thread, replacing any
/// previous one.  Call [`clear_source_locator`] after replay finishes to avoid
/// stale state on thread-pool threads.
pub fn set_source_locator(loc: Arc<dyn SourceLocator>) {
    LOCATOR.with(|l| *l.borrow_mut() = Some(loc));
}

/// Remove the source locator for the current thread.
pub fn clear_source_locator() {
    LOCATOR.with(|l| *l.borrow_mut() = None);
}

// ── Accessor helpers (called from interpreter / debug loop) ──────────────────

/// Query the current thread's source locator for a `"file:line"` string.
pub fn get_location(
    module_id: &ModuleId,
    func_def_idx: FunctionDefinitionIndex,
    pc: u16,
) -> Option<String> {
    LOCATOR.with(|l| {
        l.borrow()
            .as_ref()
            .and_then(|loc| loc.locate(module_id, func_def_idx, pc))
    })
}

/// Query the current thread's source locator for parameter / local names.
pub fn get_function_param_and_local_names(
    module_id: &ModuleId,
    func_def_idx: FunctionDefinitionIndex,
) -> Option<(usize, Vec<String>)> {
    LOCATOR.with(|l| {
        l.borrow()
            .as_ref()
            .and_then(|loc| loc.get_function_param_and_local_names(module_id, func_def_idx))
    })
}

/// Query the current thread's source locator for struct field names.
pub fn get_struct_field_names(module_id: &ModuleId, struct_name: &str) -> Option<Vec<String>> {
    LOCATOR.with(|l| {
        l.borrow()
            .as_ref()
            .and_then(|loc| loc.get_struct_field_names(module_id, struct_name))
    })
}
