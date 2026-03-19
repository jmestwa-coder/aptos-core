// Copyright (c) Aptos Foundation
// Licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE

//! `PerpDexNativeVMBlockExecutor` — VMBlockExecutor wrapper for the perp DEX native executor.
//! Same pattern as `NativeVMBlockExecutor` but uses `PerpDexNativeVMExecutorTask`.

use crate::native::{
    native_config::{NativeConfig, NATIVE_EXECUTOR_POOL},
    perpdex_executor::PerpDexNativeVMExecutorTask,
};
use aptos_block_executor::{
    code_cache_global_manager::AptosModuleCacheManager,
    txn_commit_hook::NoOpTransactionCommitHook,
    txn_provider::default::DefaultTxnProvider,
};
use aptos_types::{
    block_executor::{
        config::{BlockExecutorConfig, BlockExecutorConfigFromOnchain, BlockExecutorLocalConfig},
        transaction_slice_metadata::TransactionSliceMetadata,
    },
    state_store::StateView,
    transaction::{
        signature_verified_transaction::SignatureVerifiedTransaction, AuxiliaryInfo, BlockOutput,
        TransactionOutput,
    },
};
use aptos_vm::{block_executor::AptosBlockExecutorWrapper, VMBlockExecutor};
use move_core_types::vm_status::VMStatus;
use std::sync::Arc;

pub struct PerpDexNativeVMBlockExecutor;

impl VMBlockExecutor for PerpDexNativeVMBlockExecutor {
    fn new() -> Self {
        Self
    }

    fn execute_block(
        &self,
        txn_provider: &DefaultTxnProvider<SignatureVerifiedTransaction, AuxiliaryInfo>,
        state_view: &(impl StateView + Sync),
        onchain_config: BlockExecutorConfigFromOnchain,
        transaction_slice_metadata: TransactionSliceMetadata,
    ) -> Result<BlockOutput<SignatureVerifiedTransaction, TransactionOutput>, VMStatus> {
        AptosBlockExecutorWrapper::<PerpDexNativeVMExecutorTask>::execute_block_on_thread_pool::<
            _,
            NoOpTransactionCommitHook<VMStatus>,
            _,
        >(
            Arc::clone(&NATIVE_EXECUTOR_POOL),
            txn_provider,
            state_view,
            &AptosModuleCacheManager::new(),
            BlockExecutorConfig {
                local: BlockExecutorLocalConfig::default_with_concurrency_level(
                    NativeConfig::get_concurrency_level(),
                ),
                onchain: onchain_config,
            },
            transaction_slice_metadata,
            None,
        )
    }
}
