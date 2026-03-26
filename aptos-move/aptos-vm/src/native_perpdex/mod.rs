// Copyright (c) Aptos Foundation
// Native Rust translations of decibel_dex Move modules.

pub mod bcs_types;
pub mod assert_utils;
pub mod decibel_time;
pub mod i64_math;
pub mod math;
pub mod moving_average;
pub mod order_book_types;
pub mod order_id_generation;
pub mod perp_order;
pub mod work_unit_utils;
pub mod perp_engine_types;

// --- aptos_market order book modules ---
pub mod order_book_utils;
pub mod market_clearinghouse_order_info;
pub mod order_match_types;
pub mod single_order_types;
pub mod bulk_order_types;
pub mod pending_order_book_index;
pub mod price_time_index;
pub mod bulk_order_utils;
pub mod single_order_book;
pub mod bulk_order_book;
pub mod order_book;
pub mod market_types;
pub mod order_placement;
pub mod order_operations;

// --- oracle and price modules ---
pub mod oracle;
pub mod internal_oracle_state;
pub mod chainlink_state;
pub mod price_management;
pub mod perp_market_config;

// --- collateral modules ---
pub mod i64_aggregator;
pub mod safe_fungible;
pub mod liquidation_config;
pub mod collateral_balance_sheet;

// --- position management modules ---
pub mod position_view_types;
pub mod position_tp_sl_tracker;
pub mod pending_order_tracker;
pub mod order_margin;
pub mod perp_positions;
pub mod position_update;
pub mod position_tp_sl;
pub mod accounts_collateral;

// --- fee modules ---
pub mod fee_treasury;
pub mod builder_code_registry;
pub mod referral_registry;
pub mod volume_tracker;
pub mod fee_distribution;
pub mod trading_fees_manager;

// --- liquidation modules ---
pub mod global_liquidation_state;
pub mod backstop_liquidator_profit_tracker;
pub mod adl_tracker;
pub mod liquidation;

// --- core engine modules ---
pub mod clearinghouse_perp;
pub mod perp_market;
pub mod order_placement_utils;
pub mod async_matching_engine;

// --- utility modules ---
pub mod tp_sl_utils;
pub mod dead_mans_switch_tracker;
pub mod dead_mans_switch_operations;

// --- collateral queue modules ---
pub mod async_withdraw_queue;

// --- core perp engine ---
pub mod perp_engine;

// --- API modules ---
pub mod admin_apis;
pub mod order_apis;
pub mod public_apis;
pub mod perp_engine_api;
pub mod account_management_apis;
pub mod public_read_api;

// --- accounts package ---
pub mod dex_accounts;
pub mod dex_accounts_entry;
