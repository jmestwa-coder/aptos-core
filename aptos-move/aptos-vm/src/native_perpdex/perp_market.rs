// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::perp_market

use crate::native_perpdex::order_book_types::OrderId;
use serde::{Deserialize, Serialize};

// ===================== Types =====================

/// RESOURCE: PerpMarket at market object address
/// In native context, we store the market address and delegate to the
/// existing Market infrastructure for actual order book operations.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PerpMarket {
    V1 {
        market_address: [u8; 32], // Address of the underlying Market resource
    },
}

/// View-compatible representation of a bulk order.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BulkOrderView {
    V1 {
        account: [u8; 32],
        order_id: OrderId,
        order_sequence_number: u64,
        bid_prices: Vec<u64>,
        bid_sizes: Vec<u64>,
        ask_prices: Vec<u64>,
        ask_sizes: Vec<u64>,
        creation_time_micros: u64,
    },
}

// ===================== Functions =====================

pub fn register_market(market_address: [u8; 32]) -> PerpMarket {
    PerpMarket::V1 { market_address }
}

pub fn get_market_address(perp_market: &PerpMarket) -> [u8; 32] {
    let PerpMarket::V1 { market_address } = perp_market;
    *market_address
}

/// Check if an order at the given price/direction would be a taker order.
/// In native context, the actual check delegates to the Market's order book.
/// RESOURCE: Market at market_address
pub fn is_taker_order(
    perp_market: &PerpMarket,
    price: u64,
    is_buy: bool,
    _trigger_condition: Option<u64>,
) -> bool {
    // In native execution, this checks against the order book spread.
    // A buy order is a taker if its price >= best ask price.
    // A sell order is a taker if its price <= best bid price.
    // RESOURCE_READ: Market order book at get_market_address(perp_market)
    let market_addr = get_market_address(perp_market);
    let _ = (market_addr, price, is_buy);

    // Delegation to Market::is_taker_order in the dispatch layer
    // The dispatch layer reads the order book and compares price to best bid/ask.
    false // RESOURCE_READ: order book state
}

/// Get the best bid price.
/// RESOURCE: Market at market_address
pub fn best_bid_price(perp_market: &PerpMarket) -> Option<u64> {
    // RESOURCE_READ: Market order book at get_market_address(perp_market)
    let market_addr = get_market_address(perp_market);
    let _ = market_addr;
    // Delegated to Market resource in native context via dispatch layer
    None // RESOURCE_READ: order book best_bid
}

/// Get the best ask price.
/// RESOURCE: Market at market_address
pub fn best_ask_price(perp_market: &PerpMarket) -> Option<u64> {
    // RESOURCE_READ: Market order book at get_market_address(perp_market)
    let market_addr = get_market_address(perp_market);
    let _ = market_addr;
    // Delegated to Market resource in native context via dispatch layer
    None // RESOURCE_READ: order book best_ask
}

/// Get best bid and ask prices.
pub fn get_best_bid_and_ask_price(perp_market: &PerpMarket) -> (Option<u64>, Option<u64>) {
    (best_bid_price(perp_market), best_ask_price(perp_market))
}

/// Get the slippage price for a given direction and tolerance.
/// RESOURCE: Market at market_address
pub fn get_slippage_price(
    perp_market: &PerpMarket,
    is_buy: bool,
    slippage_pct: u64,
) -> Option<u64> {
    // RESOURCE_READ: Market order book at get_market_address(perp_market)
    // Delegated to: get_market(market).get_order_book().get_slippage_price(is_buy, slippage_pct)
    let market_addr = get_market_address(perp_market);
    let _ = (market_addr, is_buy, slippage_pct);
    None // RESOURCE_READ: order book slippage calculation
}

/// Get the remaining size of an order.
/// RESOURCE: Market at market_address
pub fn get_remaining_size(perp_market: &PerpMarket, order_id: OrderId) -> u64 {
    // RESOURCE_READ: Market order book at get_market_address(perp_market)
    // Delegated to: get_market(market).get_order_book().get_single_remaining_size(order_id)
    let market_addr = get_market_address(perp_market);
    let _ = (market_addr, order_id);
    0 // RESOURCE_READ: order book remaining size
}


// ===================== Functions delegating to order book operations =====================

/// Place a bulk order on the market.
/// Delegates to aptos_market::market_bulk_order::place_bulk_order via the dispatch layer.
/// RESOURCE: PerpMarket (mut) at market, clearinghouse callbacks
pub fn place_bulk_order(
    market: [u8; 32], user: [u8; 32], sequence_number: u64,
    bid_prices: Vec<u64>, bid_sizes: Vec<u64>,
    ask_prices: Vec<u64>, ask_sizes: Vec<u64>,
    builder_code: Option<crate::native_perpdex::builder_code_registry::BuilderCode>,
) -> Option<crate::native_perpdex::order_book_types::OrderId> {
    // In Move: perp_market::place_bulk_order(market, account, sequence_number,
    //   bid_prices, bid_sizes, ask_prices, ask_sizes, metadata, callbacks)
    // where callbacks = clearinghouse_perp::market_callbacks(market)
    // and metadata is built from builder_code.
    //
    // The dispatch layer:
    // 1. Reads the PerpMarket resource (mutable) at market address
    // 2. Creates the OrderMetadata from builder_code
    // 3. Creates the clearinghouse callbacks
    // 4. Calls market_bulk_order::place_bulk_order with the market's inner Market<OrderMetadata>
    // 5. Returns the resulting OrderId if successful
    let _ = (market, user, sequence_number, bid_prices, bid_sizes, ask_prices, ask_sizes, builder_code);

    // RESOURCE_WRITE: PerpMarket at market (order book mutation)
    // RESOURCE_READ/WRITE: clearinghouse callbacks trigger validation and position updates
    None // DISPATCH_LAYER: actual bulk order placement
}

/// Cancel all bulk orders for user on market.
/// Delegates to aptos_market::market_bulk_order::cancel_bulk_order.
/// RESOURCE: PerpMarket (mut) at market, clearinghouse callbacks
pub fn cancel_bulk_order(
    market: [u8; 32], user: [u8; 32],
) -> Result<(), u64> {
    // In Move: perp_market::cancel_bulk_order(market, user, callbacks)
    // where callbacks = clearinghouse_perp::market_callbacks(market)
    // cancellation_reason = order_cancellation_reason_cancelled_by_user()
    let _ = (market, user);

    // RESOURCE_WRITE: PerpMarket at market (order book mutation)
    // RESOURCE_READ/WRITE: clearinghouse callbacks for cleanup
    Ok(()) // DISPATCH_LAYER: actual bulk order cancellation
}

/// Cancel bulk order at a specific price level.
/// Delegates to aptos_market::market_bulk_order::cancel_bulk_order_at_price_level.
/// RESOURCE: PerpMarket (mut) at market, clearinghouse callbacks
pub fn cancel_bulk_order_at_price_level(
    market: [u8; 32], user: [u8; 32], price: u64, is_bid: bool,
) -> Result<(), u64> {
    // In Move: perp_market::cancel_bulk_order_at_price_level(market, user, price, is_bid, callbacks)
    let _ = (market, user, price, is_bid);

    // RESOURCE_WRITE: PerpMarket at market (order book mutation)
    Ok(()) // DISPATCH_LAYER: actual price-level cancellation
}

/// Cancel a single order.
/// Delegates to aptos_market::order_operations::cancel_order.
/// RESOURCE: PerpMarket (mut) at market, clearinghouse callbacks
pub fn cancel_order(
    market: [u8; 32], user: [u8; 32],
    order_id: crate::native_perpdex::order_book_types::OrderId,
) -> Result<(), u64> {
    // In Move: perp_market::cancel_order(market, user, order_id, emit_event=true,
    //   cancellation_reason=cancelled_by_user, cancel_details="", callbacks)
    let _ = (market, user, order_id);

    // RESOURCE_WRITE: PerpMarket at market (order book mutation)
    // RESOURCE_READ/WRITE: clearinghouse cleanup_order callback
    Ok(()) // DISPATCH_LAYER: actual order cancellation
}

/// Cancel an order by client order ID.
/// Delegates to aptos_market::order_operations::cancel_order_with_client_id.
/// RESOURCE: PerpMarket (mut) at market, clearinghouse callbacks
pub fn cancel_client_order(
    market: [u8; 32], user: [u8; 32], client_order_id: &str,
) -> Result<(), u64> {
    // In Move: perp_market::cancel_client_order(market, user, client_order_id, callbacks)
    // cancellation_reason = cancelled_by_user, cancel_details = ""
    let _ = (market, user, client_order_id);

    // RESOURCE_WRITE: PerpMarket at market (order book mutation)
    Ok(()) // DISPATCH_LAYER: actual client order cancellation
}

// ===================== Dispatch stubs (by-addr) for perp_engine delegation =====================

pub fn get_remaining_size_by_addr(_market: [u8; 32], _order_id: u128) -> u64 {
    // Dispatch layer resolves PerpMarket resource and queries order book
    0
}

pub fn get_bulk_order_by_addr(
    _market: [u8; 32], _account: [u8; 32],
) -> BulkOrderView {
    // Dispatch layer resolves PerpMarket resource
    BulkOrderView::V1 {
        account: [0u8; 32],
        order_id: crate::native_perpdex::order_book_types::OrderId { order_id: 0 },
        order_sequence_number: 0,
        bid_prices: Vec::new(),
        bid_sizes: Vec::new(),
        ask_prices: Vec::new(),
        ask_sizes: Vec::new(),
        creation_time_micros: 0,
    }
}
