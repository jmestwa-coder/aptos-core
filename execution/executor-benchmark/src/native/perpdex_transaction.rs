// Copyright (c) Aptos Foundation
// Licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE

use aptos_types::{
    account_address::AccountAddress,
    transaction::signature_verified_transaction::SignatureVerifiedTransaction,
};

/// Parsed representation of the 4 perp DEX entry functions used in benchmarks.
#[derive(Debug)]
pub enum PerpDexTransaction {
    /// `{publisher}::admin_apis::update_mark_for_internal_oracle`
    UpdateOraclePrice {
        sender: AccountAddress,
        sequence_number: u64,
        market: AccountAddress,
        price: u64,
        backstop_liquidations: Vec<AccountAddress>,
        margin_call_liquidations: Vec<AccountAddress>,
        update_mark: bool,
    },
    /// `{publisher}::dex_accounts_entry::place_bulk_orders_to_subaccount`
    PlaceBulkOrders {
        sender: AccountAddress,
        sequence_number: u64,
        subaccount_address: AccountAddress,
        market: AccountAddress,
        mm_sequence_number: u64,
        bid_prices: Vec<u64>,
        bid_sizes: Vec<u64>,
        ask_prices: Vec<u64>,
        ask_sizes: Vec<u64>,
        builder_address: Option<AccountAddress>,
        builder_fee: Option<u64>,
    },
    /// `{publisher}::dex_accounts_entry::place_order_to_subaccount`
    PlaceOrder {
        sender: AccountAddress,
        sequence_number: u64,
        subaccount_address: AccountAddress,
        market: AccountAddress,
        price: u64,
        size: u64,
        is_buy: bool,
        time_in_force: u8,
        is_reduce_only: bool,
        client_order_id: Option<String>,
        stop_price: Option<u64>,
        tp_trigger_price: Option<u64>,
        tp_limit_price: Option<u64>,
        sl_trigger_price: Option<u64>,
        sl_limit_price: Option<u64>,
        builder_address: Option<AccountAddress>,
        builder_fee: Option<u64>,
    },
    /// `{publisher}::public_apis::process_perp_market_pending_requests`
    ProcessPendingRequests {
        sender: AccountAddress,
        sequence_number: u64,
        market: AccountAddress,
        max_work_units: u32,
    },
    /// Passthrough for non-DEX transactions (block metadata, code publish, etc.)
    Passthrough,
}

impl PerpDexTransaction {
    /// Parse a `SignatureVerifiedTransaction` into a `PerpDexTransaction`.
    /// Returns `Passthrough` for non-DEX transactions (block metadata, epilogue,
    /// module publish, etc.) so the caller can delegate to the base NativeTransaction.
    pub fn parse(txn: &SignatureVerifiedTransaction) -> Self {
        match &txn.expect_valid() {
            aptos_types::transaction::Transaction::UserTransaction(user_txn) => {
                match user_txn.payload().executable_ref() {
                    Ok(aptos_types::transaction::TransactionExecutableRef::EntryFunction(f))
                        if !user_txn.payload().is_multisig() =>
                    {
                        let module_name = f.module().name().as_str();
                        let function_name = f.function().as_str();

                        match (module_name, function_name) {
                            ("admin_apis", "update_mark_for_internal_oracle") => {
                                Self::UpdateOraclePrice {
                                    sender: user_txn.sender(),
                                    sequence_number: user_txn.sequence_number(),
                                    market: bcs::from_bytes(&f.args()[0]).unwrap(),
                                    price: bcs::from_bytes(&f.args()[1]).unwrap(),
                                    backstop_liquidations: bcs::from_bytes(&f.args()[2]).unwrap(),
                                    margin_call_liquidations: bcs::from_bytes(&f.args()[3])
                                        .unwrap(),
                                    update_mark: bcs::from_bytes(&f.args()[4]).unwrap(),
                                }
                            },
                            ("dex_accounts_entry", "place_bulk_orders_to_subaccount") => {
                                Self::PlaceBulkOrders {
                                    sender: user_txn.sender(),
                                    sequence_number: user_txn.sequence_number(),
                                    subaccount_address: bcs::from_bytes(&f.args()[0]).unwrap(),
                                    market: bcs::from_bytes(&f.args()[1]).unwrap(),
                                    mm_sequence_number: bcs::from_bytes(&f.args()[2]).unwrap(),
                                    bid_prices: bcs::from_bytes(&f.args()[3]).unwrap(),
                                    bid_sizes: bcs::from_bytes(&f.args()[4]).unwrap(),
                                    ask_prices: bcs::from_bytes(&f.args()[5]).unwrap(),
                                    ask_sizes: bcs::from_bytes(&f.args()[6]).unwrap(),
                                    builder_address: bcs::from_bytes(&f.args()[7]).unwrap(),
                                    builder_fee: bcs::from_bytes(&f.args()[8]).unwrap(),
                                }
                            },
                            ("dex_accounts_entry", "place_order_to_subaccount") => {
                                Self::PlaceOrder {
                                    sender: user_txn.sender(),
                                    sequence_number: user_txn.sequence_number(),
                                    subaccount_address: bcs::from_bytes(&f.args()[0]).unwrap(),
                                    market: bcs::from_bytes(&f.args()[1]).unwrap(),
                                    price: bcs::from_bytes(&f.args()[2]).unwrap(),
                                    size: bcs::from_bytes(&f.args()[3]).unwrap(),
                                    is_buy: bcs::from_bytes(&f.args()[4]).unwrap(),
                                    time_in_force: bcs::from_bytes(&f.args()[5]).unwrap(),
                                    is_reduce_only: bcs::from_bytes(&f.args()[6]).unwrap(),
                                    client_order_id: bcs::from_bytes(&f.args()[7]).unwrap(),
                                    stop_price: bcs::from_bytes(&f.args()[8]).unwrap(),
                                    tp_trigger_price: bcs::from_bytes(&f.args()[9]).unwrap(),
                                    tp_limit_price: bcs::from_bytes(&f.args()[10]).unwrap(),
                                    sl_trigger_price: bcs::from_bytes(&f.args()[11]).unwrap(),
                                    sl_limit_price: bcs::from_bytes(&f.args()[12]).unwrap(),
                                    builder_address: bcs::from_bytes(&f.args()[13]).unwrap(),
                                    builder_fee: bcs::from_bytes(&f.args()[14]).unwrap(),
                                }
                            },
                            ("public_apis", "process_perp_market_pending_requests") => {
                                Self::ProcessPendingRequests {
                                    sender: user_txn.sender(),
                                    sequence_number: user_txn.sequence_number(),
                                    market: bcs::from_bytes(&f.args()[0]).unwrap(),
                                    max_work_units: bcs::from_bytes(&f.args()[1]).unwrap(),
                                }
                            },
                            // Non-DEX entry functions (e.g. code::publish_package_txn, account creation)
                            _ => Self::Passthrough,
                        }
                    },
                    _ => Self::Passthrough,
                }
            },
            // Block metadata, epilogue, genesis, etc.
            _ => Self::Passthrough,
        }
    }

    pub fn sender(&self) -> Option<AccountAddress> {
        match self {
            Self::UpdateOraclePrice { sender, .. }
            | Self::PlaceBulkOrders { sender, .. }
            | Self::PlaceOrder { sender, .. }
            | Self::ProcessPendingRequests { sender, .. } => Some(*sender),
            Self::Passthrough => None,
        }
    }

    pub fn sequence_number(&self) -> Option<u64> {
        match self {
            Self::UpdateOraclePrice {
                sequence_number, ..
            }
            | Self::PlaceBulkOrders {
                sequence_number, ..
            }
            | Self::PlaceOrder {
                sequence_number, ..
            }
            | Self::ProcessPendingRequests {
                sequence_number, ..
            } => Some(*sequence_number),
            Self::Passthrough => None,
        }
    }

    pub fn market(&self) -> Option<AccountAddress> {
        match self {
            Self::UpdateOraclePrice { market, .. }
            | Self::PlaceBulkOrders { market, .. }
            | Self::PlaceOrder { market, .. }
            | Self::ProcessPendingRequests { market, .. } => Some(*market),
            Self::Passthrough => None,
        }
    }
}
