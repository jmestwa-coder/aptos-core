// Copyright (c) Aptos Foundation
// Licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE

use crate::pipeline::{CommitBlockMessage, LedgerUpdateMessage};
use aptos_executor::block_executor::BlockExecutor;
use aptos_executor_types::BlockExecutorTrait;
use aptos_infallible::Mutex;
use aptos_vm::VMBlockExecutor;
use aptos_types::write_set::TransactionWrite;
use move_core_types::language_storage::StructTag;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{mpsc, Arc},
};

pub enum CommitProcessing {
    SendToQueue(mpsc::SyncSender<CommitBlockMessage>),
    #[allow(dead_code)]
    ExecuteInline,
    Skip,
}

pub struct LedgerUpdateStage<V> {
    executor: Arc<BlockExecutor<V>>,
    commit_processing: CommitProcessing,
    allow_aborts: bool,
    allow_discards: bool,
    allow_retries: bool,
    event_summary: Arc<Mutex<BTreeMap<(usize, StructTag), usize>>>,
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

impl<V> LedgerUpdateStage<V>
where
    V: VMBlockExecutor,
{
    pub fn new(
        executor: Arc<BlockExecutor<V>>,
        commit_processing: CommitProcessing,
        allow_aborts: bool,
        allow_discards: bool,
        allow_retries: bool,
        event_summary: Arc<Mutex<BTreeMap<(usize, StructTag), usize>>>,
    ) -> Self {
        Self {
            executor,
            commit_processing,
            allow_aborts,
            allow_discards,
            allow_retries,
            event_summary,
        }
    }

    pub fn ledger_update(&mut self, ledger_update_message: LedgerUpdateMessage) {
        // let ledger_update_start_time = Instant::now();
        let LedgerUpdateMessage {
            first_block_start_time,
            current_block_start_time,
            partition_time,
            execution_time,
            block_id,
            parent_block_id,
            num_input_txns,
            stage,
        } = ledger_update_message;

        let output = self
            .executor
            .ledger_update(block_id, parent_block_id)
            .unwrap();
        output.execution_output.check_aborts_discards_retries(
            self.allow_aborts,
            self.allow_discards,
            self.allow_retries,
        );

        if !self.allow_retries {
            if output.epoch_state().is_none() {
                assert_eq!(output.num_transactions_to_commit(), num_input_txns + 1);
            } else {
                assert_eq!(output.num_transactions_to_commit(), num_input_txns);
            }
        }

        let mut event_summary = self.event_summary.lock();
        let mut collateral_fee_count = 0u32;
        let mut collateral_pnl_count = 0u32;
        let mut collateral_other_count = 0u32;
        for output in &output.execution_output.to_commit.transaction_outputs {
            for event in output.events() {
                let tag = event.type_tag().struct_tag().unwrap().clone();
                let count = event_summary
                    .entry((stage, tag.clone()))
                    .or_insert(0);
                *count += 1;
                // Decode CollateralBalanceChangeEvent change_type for diagnostics
                // BCS layout: V1(1) + asset_type(32) + balance_type + delta(8) + I64Snapshot(9) + change_type(1)
                // balance_type: Cross = tag(1) + addr(32) = 33 bytes; Isolated = tag(1) + addr(32) + market(32) = 65 bytes
                if tag.module.as_str() == "collateral_balance_sheet" && tag.name.as_str() == "CollateralBalanceChangeEvent" {
                    let data = event.event_data();
                    // Determine balance_type variant from byte 33 (right after V1 tag + asset_type)
                    let change_type_offset = if data.len() > 33 {
                        match data[33] {
                            0 => 83,  // Cross: 1+32+33+8+9 = 83
                            1 => 115, // Isolated: 1+32+65+8+9 = 115
                            _ => 83,  // fallback
                        }
                    } else { 83 };
                    if data.len() > change_type_offset {
                        let change_type_byte = data[change_type_offset];
                        match change_type_byte {
                            0 => collateral_other_count += 1,  // UserMovement
                            1 => collateral_fee_count += 1,    // Fee
                            2 => collateral_pnl_count += 1,    // PnL
                            3 => collateral_other_count += 1,  // Margin
                            4 => collateral_other_count += 1,  // Liquidation
                            _ => collateral_other_count += 1,
                        }
                    } else {
                        collateral_other_count += 1;
                    }
                }
            }
        }
        drop(event_summary);

        // Per-transaction write set hash dumping for Move VM vs Native dispatch comparison.
        // Controlled by PERPDEX_DUMP_TXNS=1 environment variable.
        // Set PERPDEX_DUMP_DETAIL=1 for per-key write details.
        if std::env::var("PERPDEX_DUMP_TXNS").map_or(false, |v| v == "1") {
            let dump_detail = std::env::var("PERPDEX_DUMP_DETAIL").map_or(false, |v| v == "1");
            let first_version = output.execution_output.first_version;
            for (idx, txn_output) in output
                .execution_output
                .to_commit
                .transaction_outputs
                .iter()
                .enumerate()
            {
                let mut write_hasher = Sha256::new();
                let mut write_count = 0usize;
                let mut write_bytes = 0usize;
                let mut create_count = 0usize;
                let mut modify_count = 0usize;
                let mut delete_count = 0usize;
                for (key, op) in txn_output.write_set().write_op_iter() {
                    write_hasher.update(bcs::to_bytes(key).unwrap_or_default());
                    let op_bytes = op.bytes().map_or(0, |b| b.len());
                    write_bytes += op_bytes;
                    if let Some(bytes) = op.bytes() {
                        write_hasher.update(bytes.as_ref());
                    }
                    write_count += 1;
                    match op.write_op_kind() {
                        aptos_types::write_set::WriteOpKind::Creation => create_count += 1,
                        aptos_types::write_set::WriteOpKind::Modification => modify_count += 1,
                        aptos_types::write_set::WriteOpKind::Deletion => delete_count += 1,
                    }
                    if dump_detail {
                        let kind = match op.write_op_kind() {
                            aptos_types::write_set::WriteOpKind::Creation => "C",
                            aptos_types::write_set::WriteOpKind::Modification => "M",
                            aptos_types::write_set::WriteOpKind::Deletion => "D",
                        };
                        eprintln!(
                            "[WRITE] version={} key={:?} kind={} bytes={}",
                            first_version + idx as u64,
                            key,
                            kind,
                            op_bytes,
                        );
                    }
                }
                let write_hash = write_hasher.finalize();

                let mut event_hasher = Sha256::new();
                let mut event_count = 0usize;
                let mut event_bytes = 0usize;
                for event in txn_output.events() {
                    event_hasher.update(event.event_data());
                    event_count += 1;
                    event_bytes += event.event_data().len();
                }
                let event_hash = event_hasher.finalize();

                let version = first_version + idx as u64;
                eprintln!(
                    "[TXN] stage={} version={} writes={} write_bytes={} creates={} modifies={} deletes={} events={} event_bytes={} write_hash={:.16} event_hash={:.16} gas={}",
                    stage,
                    version,
                    write_count,
                    write_bytes,
                    create_count,
                    modify_count,
                    delete_count,
                    event_count,
                    event_bytes,
                    hex_encode(&write_hash),
                    hex_encode(&event_hash),
                    txn_output.gas_used(),
                );
            }
        }

        match &self.commit_processing {
            CommitProcessing::SendToQueue(commit_sender) => {
                let msg = CommitBlockMessage {
                    block_id,
                    first_block_start_time,
                    current_block_start_time,
                    partition_time,
                    execution_time,
                    output,
                };
                commit_sender.send(msg).unwrap();
            },
            CommitProcessing::ExecuteInline => {
                let ledger_info_with_sigs = super::transaction_committer::gen_li_with_sigs(
                    block_id,
                    output.root_hash(),
                    output.expect_last_version(),
                );
                self.executor.pre_commit_block(block_id).unwrap();
                self.executor.commit_ledger(ledger_info_with_sigs).unwrap();
            },
            CommitProcessing::Skip => {},
        }
    }
}
