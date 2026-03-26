// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::order_apis

use crate::native_perpdex::builder_code_registry;
use crate::native_perpdex::order_book_types;
use crate::native_perpdex::perp_engine;
use crate::native_perpdex::perp_order;

// ===================== Functions =====================

/// Place a limit order.
/// Delegates to perp_engine::place_order.
pub fn place_order(
    market: [u8; 32],
    user: [u8; 32], // signer
    order_request: perp_order::PerpOrderRequestCommonArgs,
    is_reduce_only: bool,
    stop_price: Option<u64>,
    tpsl_order_request: perp_order::PerpOrderRequestTpSlArgs,
    builder_code: Option<builder_code_registry::BuilderCode>,
) -> Result<order_book_types::OrderId, u64> {
    perp_engine::place_order(
        market, user, order_request, is_reduce_only, stop_price,
        tpsl_order_request, builder_code,
    )
}

/// Place bulk orders for market making.
/// Delegates to perp_engine::place_bulk_order.
pub fn place_bulk_order(
    market: [u8; 32],
    user: [u8; 32], // signer
    sequence_number: u64,
    bid_prices: Vec<u64>,
    bid_sizes: Vec<u64>,
    ask_prices: Vec<u64>,
    ask_sizes: Vec<u64>,
    builder_code: Option<builder_code_registry::BuilderCode>,
) -> Option<order_book_types::OrderId> {
    perp_engine::place_bulk_order(
        market, user, sequence_number, bid_prices, bid_sizes,
        ask_prices, ask_sizes, builder_code,
    )
}

/// Cancel all bulk orders.
/// Delegates to perp_engine::cancel_bulk_order.
pub fn cancel_bulk_order(
    market: [u8; 32],
    user: [u8; 32], // signer
) -> Result<(), u64> {
    perp_engine::cancel_bulk_order(market, user)
}

/// Cancel bulk order at a specific price level.
/// Delegates to perp_engine::cancel_bulk_order_at_price_level.
pub fn cancel_bulk_order_at_price_level(
    market: [u8; 32],
    user: [u8; 32], // signer
    price: u64,
    is_bid: bool,
) -> Result<(), u64> {
    perp_engine::cancel_bulk_order_at_price_level(market, user, price, is_bid)
}

/// Place a market order.
/// Delegates to perp_engine::place_market_order.
pub fn place_market_order(
    market: [u8; 32],
    user: [u8; 32], // signer
    orig_size: u64,
    is_buy: bool,
    is_reduce_only: bool,
    client_order_id: Option<String>,
    stop_price: Option<u64>,
    tpsl_order_request: perp_order::PerpOrderRequestTpSlArgs,
    builder_code: Option<builder_code_registry::BuilderCode>,
) -> Result<order_book_types::OrderId, u64> {
    perp_engine::place_market_order(
        market, user, orig_size, is_buy, is_reduce_only, client_order_id,
        stop_price, tpsl_order_request, builder_code,
    )
}

/// Update an existing order.
/// Delegates to perp_engine::update_order.
pub fn update_order(
    user: [u8; 32], // signer
    order_id: order_book_types::OrderId,
    market: [u8; 32],
    price: u64,
    orig_size: u64,
    is_buy: bool,
    time_in_force: order_book_types::TimeInForce,
    is_reduce_only: bool,
    tpsl_order_request: perp_order::PerpOrderRequestTpSlArgs,
    builder_code: Option<builder_code_registry::BuilderCode>,
) -> Result<(), u64> {
    perp_engine::update_order(
        user, order_id, market, price, orig_size, is_buy, time_in_force,
        is_reduce_only, tpsl_order_request, builder_code,
    )
}

/// Update order by client ID.
/// Delegates to perp_engine::update_client_order.
pub fn update_client_order(
    user: [u8; 32], // signer
    client_order_id: String,
    market: [u8; 32],
    price: u64,
    orig_size: u64,
    is_buy: bool,
    time_in_force: order_book_types::TimeInForce,
    is_reduce_only: bool,
    tpsl_order_request: perp_order::PerpOrderRequestTpSlArgs,
    builder_code: Option<builder_code_registry::BuilderCode>,
) -> Result<(), u64> {
    perp_engine::update_client_order(
        user, client_order_id, market, price, orig_size, is_buy, time_in_force,
        is_reduce_only, tpsl_order_request, builder_code,
    )
}

/// Cancel a single order.
/// Delegates to perp_engine::cancel_order.
pub fn cancel_order(
    market: [u8; 32],
    user: [u8; 32], // signer
    order_id: order_book_types::OrderId,
) -> Result<(), u64> {
    perp_engine::cancel_order(market, user, order_id)
}

/// Cancel order by client ID.
/// Delegates to perp_engine::cancel_client_order.
pub fn cancel_client_order(
    market: [u8; 32],
    user: [u8; 32], // signer
    client_order_id: String,
) -> Result<(), u64> {
    perp_engine::cancel_client_order(market, user, client_order_id)
}

/// Place a TWAP order.
/// Delegates to perp_engine::place_twap_order.
pub fn place_twap_order(
    market: [u8; 32],
    user: [u8; 32], // signer
    orig_size: u64,
    is_buy: bool,
    is_reduce_only: bool,
    client_order_id: Option<String>,
    twap_frequency_s: u64,
    twap_duration_s: u64,
    builder_code: Option<builder_code_registry::BuilderCode>,
) -> Result<order_book_types::OrderId, u64> {
    perp_engine::place_twap_order(
        market, user, orig_size, is_buy, is_reduce_only, client_order_id,
        twap_frequency_s, twap_duration_s, builder_code,
    )
}

/// Cancel a TWAP order.
/// Delegates to perp_engine::cancel_twap_order.
pub fn cancel_twap_order(
    market: [u8; 32],
    user: [u8; 32], // signer
    order_id: order_book_types::OrderId,
) -> Result<(), u64> {
    perp_engine::cancel_twap_order(market, user, order_id)
}

/// Place TP/SL orders for an existing position.
/// Delegates to perp_engine::place_tp_sl_order_for_position.
pub fn place_tp_sl_order_for_position(
    market: [u8; 32],
    user: [u8; 32], // signer
    tp_trigger_price: Option<u64>,
    tp_limit_price: Option<u64>,
    tp_size: Option<u64>,
    sl_trigger_price: Option<u64>,
    sl_limit_price: Option<u64>,
    sl_size: Option<u64>,
    builder_code: Option<builder_code_registry::BuilderCode>,
) -> Result<(Option<order_book_types::OrderId>, Option<order_book_types::OrderId>), u64> {
    perp_engine::place_tp_sl_order_for_position(
        market, user, tp_trigger_price, tp_limit_price, tp_size,
        sl_trigger_price, sl_limit_price, sl_size, builder_code,
    )
}

/// Cancel a TP/SL order for a position.
/// Delegates to perp_engine::cancel_tp_sl_order_for_position.
pub fn cancel_tp_sl_order_for_position(
    market: [u8; 32],
    user: [u8; 32], // signer
    order_id: order_book_types::OrderId,
) -> Result<(), u64> {
    perp_engine::cancel_tp_sl_order_for_position(market, user, order_id)
}
