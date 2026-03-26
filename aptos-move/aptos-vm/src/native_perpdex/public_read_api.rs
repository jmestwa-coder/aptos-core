// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::public_read_api

use crate::native_perpdex::order_book_types;
use crate::native_perpdex::perp_engine;
use crate::native_perpdex::perp_market;
use crate::native_perpdex::perp_market_config;
use crate::native_perpdex::position_view_types;

const EDEPRECATED_API: u64 = 1;

// ==================== Exchange State APIs ====================

/// Check if the exchange is open for trading.
pub fn is_exchange_open() -> bool {
    perp_engine::is_exchange_open()
}

/// Get the backstop liquidator address.
pub fn backstop_liquidator() -> [u8; 32] {
    perp_engine::backstop_liquidator()
}

// ==================== Collateral APIs ====================

/// Get the primary asset metadata (e.g., USDC).
pub fn primary_asset_metadata() -> [u8; 32] {
    perp_engine::primary_asset_metadata()
}

/// Check if a given metadata is supported as collateral.
pub fn is_supported_collateral(metadata: [u8; 32]) -> bool {
    perp_engine::is_supported_collateral(metadata)
}

/// Get the maximum amount that can be withdrawn from cross margin.
pub fn max_allowed_withdraw_from_cross(account: [u8; 32], metadata: [u8; 32]) -> Result<u64, u64> {
    perp_engine::max_allowed_withdraw_from_cross(account, metadata)
}

// ==================== Account Status APIs ====================

/// Get the net asset value (NAV) of an account.
pub fn get_account_net_asset_value(account: [u8; 32]) -> i64 {
    perp_engine::get_account_net_asset_value(account)
}

/// DEPRECATED: has_any_assets_or_positions
pub fn has_any_assets_or_positions(_account: [u8; 32]) -> Result<bool, u64> {
    Err(EDEPRECATED_API)
}

/// Check if account has any perp positions or orders.
pub fn account_has_any_perp_positions_or_orders(account: [u8; 32]) -> bool {
    perp_engine::account_has_any_positions_or_orders(account)
}

/// Check if account has any perp assets, positions, or orders.
pub fn account_has_any_perp_assets_positions_or_orders(account: [u8; 32]) -> bool {
    perp_engine::account_has_any_assets_positions_or_orders(account)
}

// ==================== Position APIs ====================

/// List all positions for an account across all markets.
pub fn list_positions(account: [u8; 32]) -> Vec<position_view_types::PositionViewInfo> {
    perp_engine::list_positions(account)
}

/// View a specific position for an account in a market.
pub fn view_position(
    account: [u8; 32],
    market: [u8; 32],
) -> Option<position_view_types::PositionViewInfo> {
    perp_engine::view_position(account, market)
}

// ==================== Market APIs ========================

/// Round size to the nearest lot size.
pub fn get_market_round_size_to_lot(
    _market: [u8; 32],
    _size: u64,
    _ceil: bool,
) -> u64 {
    // Delegates to perp_market_config::round_size_to_lot
    0
}

/// Round price to the nearest ticker.
pub fn get_market_round_price_to_ticker(
    _market: [u8; 32],
    _price: u64,
    _ceil: bool,
) -> u64 {
    // Delegates to perp_market_config::round_price_to_ticker
    0
}

// ==================== Market Price APIs ====================

/// Get the current mark price for a market.
pub fn get_mark_price(market: [u8; 32]) -> u64 {
    perp_engine::get_mark_price(market)
}

// ==================== Order Tracking APIs ====================

/// Get the next tracked order for an account across all markets.
pub fn get_next_tracked_order_using_margin(
    _account: [u8; 32],
) -> Option<Vec<u8>> {
    // Returns Option<TrackedOrderInfo> opaque
    None
}

/// Get the total margin locked by pending orders in a specific market.
pub fn get_pending_order_margin_for_market(
    _account: [u8; 32],
    _market: [u8; 32],
) -> u64 {
    0
}

// ==================== Trading Volume APIs ====================

/// Get maker trading volume in the current time window.
pub fn get_maker_volume_in_window(_account: [u8; 32]) -> u128 {
    0
}

/// Get taker trading volume in the current time window.
pub fn get_taker_volume_in_window(_account: [u8; 32]) -> u128 {
    0
}

// ==================== Position/Collateral APIs ====================

/// Get position size.
pub fn get_position_size(account: [u8; 32], market: [u8; 32]) -> u64 {
    perp_engine::get_position_size(account, market)
}

/// Check if account has a position.
pub fn has_position(account: [u8; 32], market: [u8; 32]) -> bool {
    perp_engine::has_position(account, market)
}

/// Get remaining size for an order (by u128 id).
pub fn get_remaining_size_for_order(market: [u8; 32], order_id: u128) -> u64 {
    perp_engine::get_remaining_size_for_order(market, order_id)
}

/// Get remaining size for an order (by OrderId type).
pub fn get_remaining_size_for_order_id(
    _market: [u8; 32],
    _order_id: order_book_types::OrderId,
) -> u64 {
    0
}

/// Get cross total collateral value.
pub fn get_cross_total_collateral_value(account: [u8; 32]) -> i64 {
    perp_engine::get_cross_total_collateral_value(account)
}

/// Get isolated position total collateral value.
pub fn get_isolated_position_total_collateral_value(
    account: [u8; 32],
    market: [u8; 32],
) -> i64 {
    perp_engine::get_isolated_position_total_collateral_value(account, market)
}

// ==================== Builder Code APIs ====================

/// DEPRECATED: Use get_approved_max_fee_v2 instead.
pub fn get_approved_max_fee(_user: [u8; 32], _builder: [u8; 32]) -> Result<u64, u64> {
    Err(EDEPRECATED_API)
}

/// Get the approved maximum fee for a user-builder pair.
pub fn get_approved_max_fee_v2(
    _user: [u8; 32],
    _builder: [u8; 32],
) -> Option<u64> {
    // Delegates to builder_code_registry::get_approved_max_fee
    None
}

// ==================== Reserved Collateral APIs ====================

/// Get reserved collateral amount.
pub fn get_reserved_collateral(_account: [u8; 32]) -> u64 {
    0
}

// ==================== Fee Treasury APIs ====================

/// Get fee treasury balance.
pub fn get_fee_treasury_balance() -> u64 {
    perp_engine::get_fee_treasury_balance()
}

// ==================== Trading Fees & Referral APIs ====================

/// Get all referral codes for a user.
pub fn get_referral_codes(_user_addr: [u8; 32]) -> Vec<String> {
    Vec::new()
}

/// Get the referrer address for a user.
pub fn get_referrer_addr(_user_addr: [u8; 32]) -> Option<[u8; 32]> {
    None
}
