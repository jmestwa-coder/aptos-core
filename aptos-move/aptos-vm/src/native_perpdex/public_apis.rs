// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::public_apis

use crate::native_perpdex::perp_engine;
use crate::native_perpdex::work_unit_utils;

// ===================== Functions =====================

/// Close a position when the market is delisted.
/// Anyone can call this to close positions in delisted markets.
pub fn close_delisted_position(
    address: [u8; 32],
    market: [u8; 32],
) -> Result<(), u64> {
    perp_engine::close_delisted_position(address, market)
}

/// Trigger pending requests for a market.
/// Used by keeper services to process pending orders.
pub fn process_perp_market_pending_requests(
    market: [u8; 32],
    max_work_unit: u32,
) -> Result<(), u64> {
    perp_engine::process_pending_requests(
        market,
        work_unit_utils::get_work_units_from_argument(max_work_unit),
    )
}

/// Process pending withdrawal requests in the queue.
/// Anyone can call this to help process the queue.
pub fn process_perp_collateral_withdrawals(
    max_work_units: u32,
) -> Result<(), u64> {
    let mut work_units = max_work_units;
    perp_engine::process_pending_withdrawals(&mut work_units)
}

/// Liquidate a single position.
/// Used by liquidators to liquidate undercollateralized positions.
pub fn liquidate_position(
    account: [u8; 32],
    market: [u8; 32],
) -> Result<(), u64> {
    perp_engine::liquidate_positions(vec![account], market)
}

/// Liquidate multiple positions.
/// Used by liquidators to liquidate undercollateralized positions.
pub fn liquidate_positions(
    accounts: Vec<[u8; 32]>,
    market: [u8; 32],
) -> Result<(), u64> {
    perp_engine::liquidate_positions(accounts, market)
}
