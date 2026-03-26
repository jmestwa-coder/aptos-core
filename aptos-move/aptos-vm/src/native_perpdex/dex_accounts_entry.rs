// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::dex_accounts_entry
//
// Entry functions for subaccount operations. These are the ENTRY FUNCTIONS
// that will be called from the native dispatch layer.

use crate::native_perpdex::dex_accounts;
use crate::native_perpdex::order_apis;
use crate::native_perpdex::order_book_types;
use crate::native_perpdex::perp_engine_api;
use crate::native_perpdex::perp_order;
use crate::native_perpdex::account_management_apis;

// ===================== Constants =====================

const EBUILDER_SUBACCOUNT_NOT_FOUND: u64 = 1;

// ===================== Entry Functions =====================

/// Create a new non-primary subaccount.
pub fn create_new_subaccount(_owner: [u8; 32]) -> [u8; 32] {
    dex_accounts::create_new_subaccount_object(_owner)
}

// ========== Permissions API ==========

/// Delegate all trading permissions (perp + vault) to another account.
pub fn delegate_all_trading_to_for_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    account_to_delegate_to: [u8; 32],
    expiration_time_s: Option<u64>,
) -> Result<(), u64> {
    dex_accounts::delegate_all_trading_to_for_subaccount(
        auth, subaccount, account_to_delegate_to, expiration_time_s,
    )
}

/// Delegate perp trading permissions.
pub fn delegate_perp_trading_to_for_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    account_to_delegate_to: [u8; 32],
    expiration_time_s: Option<u64>,
) -> Result<(), u64> {
    dex_accounts::delegate_perp_trading_to_for_subaccount(
        auth, subaccount, account_to_delegate_to, expiration_time_s,
    )
}

/// Delegate perp trading permissions for a specific market.
pub fn delegate_perp_trading_to_market_for_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    account_to_delegate_to: [u8; 32],
    market: [u8; 32],
    expiration_time_s: Option<u64>,
) -> Result<(), u64> {
    dex_accounts::delegate_perp_trading_to_market_for_subaccount(
        auth, subaccount, account_to_delegate_to, market, expiration_time_s,
    )
}

/// Delegate sub-delegation permissions.
pub fn delegate_ability_to_sub_delegate_to_for_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    account_to_delegate_to: [u8; 32],
    expiration_time_s: Option<u64>,
) -> Result<(), u64> {
    dex_accounts::delegate_ability_to_sub_delegate_to_for_subaccount(
        auth, subaccount, account_to_delegate_to, expiration_time_s,
    )
}

/// Revoke delegation from a specific account.
pub fn revoke_delegation(
    owner: [u8; 32],
    subaccount: [u8; 32],
    account_to_revoke: [u8; 32],
) -> Result<(), u64> {
    dex_accounts::revoke_delegation(owner, subaccount, account_to_revoke)
}

/// Revoke all delegations.
pub fn revoke_all_delegations(
    owner: [u8; 32],
    subaccount: [u8; 32],
) -> Result<(), u64> {
    dex_accounts::revoke_all_delegations(owner, subaccount)
}

/// Deactivate a subaccount.
pub fn deactivate_subaccount(
    owner: [u8; 32],
    subaccount: [u8; 32],
    revoke_delegations: bool,
) -> Result<(), u64> {
    dex_accounts::deactivate_subaccount(owner, subaccount, revoke_delegations)
}

/// Reactivate a subaccount.
pub fn reactivate_subaccount(
    owner: [u8; 32],
    subaccount: [u8; 32],
) -> Result<(), u64> {
    dex_accounts::reactivate_subaccount(owner, subaccount)
}

// ========== PERP API ==========

/// Deposit to a subaccount at a specific address.
pub fn deposit_to_subaccount_at(
    owner: [u8; 32],
    subaccount: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    dex_accounts::deposit_to_subaccount_at(owner, subaccount, metadata, amount)
}

/// Withdraw from subaccount to owner.
pub fn withdraw_from_subaccount(
    owner: [u8; 32],
    subaccount: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Option<u128> {
    dex_accounts::withdraw_from_subaccount(owner, subaccount, metadata, amount)
}

/// Withdraw from cross collateral.
pub fn withdraw_from_cross_collateral(
    owner: [u8; 32],
    subaccount: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Option<u128> {
    dex_accounts::withdraw_from_cross_collateral(owner, subaccount, metadata, amount)
}

/// Withdraw from non-collateral.
pub fn withdraw_from_non_collateral(
    owner: [u8; 32],
    subaccount: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) {
    dex_accounts::withdraw_from_non_collateral(owner, subaccount, metadata, amount)
}

/// Add delegated trader and deposit to subaccount (combo entry).
pub fn add_delegated_trader_and_deposit_to_subaccount(
    owner: [u8; 32],
    subaccount: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
    account_to_delegate_to: [u8; 32],
    expiration_time_s: Option<u64>,
) -> Result<(), u64> {
    dex_accounts::deposit_to_subaccount_at(owner, subaccount, metadata, amount)?;
    delegate_all_trading_to_for_subaccount(
        owner, subaccount, account_to_delegate_to, expiration_time_s,
    )
}

/// Transfer collateral between subaccounts.
pub fn transfer_collateral_between_subaccounts(
    owner: [u8; 32],
    from: [u8; 32],
    to: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    dex_accounts::transfer_collateral_between_subaccounts(owner, from, to, metadata, amount)
}

/// Deposit to isolated position collateral.
pub fn deposit_to_isolated_position_collateral(
    owner: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner(owner, subaccount)?;
    account_management_apis::deposit_to_isolated_position_collateral(
        subaccount_signer, market, metadata, amount,
    )
}

/// Withdraw from isolated position collateral.
pub fn withdraw_from_isolated_position_collateral(
    owner: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner(owner, subaccount)?;
    account_management_apis::request_withdrawal_from_isolated(
        subaccount_signer, market, metadata, amount, owner,
    )?;
    Ok(())
}

/// Transfer collateral to/from isolated position.
pub fn transfer_collateral_to_isolated_position(
    owner: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
    is_deposit: bool,
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        owner, subaccount, market,
    )?;
    account_management_apis::transfer_collateral_to_isolated_position(
        subaccount_signer, market, is_deposit, metadata, amount,
    )
}

/// Configure user settings for a market.
pub fn configure_user_settings_for_market(
    auth: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
    is_cross: bool,
    user_leverage: u8,
) -> Result<(), u64> {
    let subaccount_object = dex_accounts::get_subaccount_object_unpermissioned(
        subaccount, Some(auth),
    )?;
    let signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount_object, market,
    )?;
    account_management_apis::configure_user_settings_for_market(signer, market, is_cross, user_leverage)
}

/// Place a limit order for a subaccount.
pub fn place_order_to_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
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
    builder_address: Option<[u8; 32]>,
    builder_fees: Option<u64>,
) -> Result<(), u64> {
    let tif = order_book_types::time_in_force_from_index(time_in_force)?;
    let client_order_id_bytes = client_order_id.map(|s| s.into_bytes());
    let order_request = perp_order::new_order_common_args(
        price, size, is_buy, tif, client_order_id_bytes,
    )?;
    let tp_sl_order_request = perp_order::new_order_tp_sl_args(
        tp_trigger_price, tp_limit_price, sl_trigger_price, sl_limit_price,
    );
    let builder_code = perp_engine_api::get_builder_code_if_provided(
        builder_address, builder_fees,
    );
    dex_accounts::place_perp_order_to_subaccount(
        auth, subaccount, market, order_request,
        is_reduce_only, stop_price, tp_sl_order_request, builder_code,
    )?;
    Ok(())
}

/// Place a market order for a subaccount.
pub fn place_market_order_to_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
    size: u64,
    is_buy: bool,
    is_reduce_only: bool,
    client_order_id: Option<String>,
    stop_price: Option<u64>,
    tp_trigger_price: Option<u64>,
    tp_limit_price: Option<u64>,
    sl_trigger_price: Option<u64>,
    sl_limit_price: Option<u64>,
    builder_address: Option<[u8; 32]>,
    builder_fees: Option<u64>,
) -> Result<(), u64> {
    let builder_code = perp_engine_api::get_builder_code_if_provided(builder_address, builder_fees);
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    let tp_sl_order_request = perp_order::new_order_tp_sl_args(
        tp_trigger_price, tp_limit_price, sl_trigger_price, sl_limit_price,
    );
    order_apis::place_market_order(
        market, subaccount_signer, size, is_buy, is_reduce_only,
        client_order_id, stop_price, tp_sl_order_request, builder_code,
    )?;
    Ok(())
}

/// Place TP/SL order for a position.
pub fn place_tp_sl_order_for_position(
    auth: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
    tp_trigger_price: Option<u64>,
    tp_limit_price: Option<u64>,
    tp_size: Option<u64>,
    sl_trigger_price: Option<u64>,
    sl_limit_price: Option<u64>,
    sl_size: Option<u64>,
    builder_address: Option<[u8; 32]>,
    builder_fees: Option<u64>,
) -> Result<(), u64> {
    let builder_code = perp_engine_api::get_builder_code_if_provided(builder_address, builder_fees);
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    order_apis::place_tp_sl_order_for_position(
        market, subaccount_signer,
        tp_trigger_price, tp_limit_price, tp_size,
        sl_trigger_price, sl_limit_price, sl_size,
        builder_code,
    )?;
    Ok(())
}

/// Cancel TP/SL order for a position.
pub fn cancel_tp_sl_order_for_position(
    auth: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
    order_id: u128,
) -> Result<(), u64> {
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    order_apis::cancel_tp_sl_order_for_position(
        market, subaccount_signer,
        order_book_types::OrderId { order_id },
    )
}

/// Update TP order for a position.
pub fn update_tp_order_for_position(
    auth: [u8; 32],
    subaccount: [u8; 32],
    order_id: u128,
    market: [u8; 32],
    tp_trigger_price: Option<u64>,
    tp_limit_price: Option<u64>,
    tp_size: Option<u64>,
) -> Result<(), u64> {
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    // Cancel existing TP/SL order
    order_apis::cancel_tp_sl_order_for_position(
        market, subaccount_signer,
        order_book_types::OrderId { order_id },
    )?;
    // Place new TP order
    order_apis::place_tp_sl_order_for_position(
        market, subaccount_signer,
        tp_trigger_price, tp_limit_price, tp_size,
        None, None, None, // No SL
        None, // builder_code
    )?;
    Ok(())
}

/// Update SL order for a position.
pub fn update_sl_order_for_position(
    auth: [u8; 32],
    subaccount: [u8; 32],
    order_id: u128,
    market: [u8; 32],
    sl_trigger_price: Option<u64>,
    sl_limit_price: Option<u64>,
    sl_size: Option<u64>,
) -> Result<(), u64> {
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    // Cancel existing TP/SL order
    order_apis::cancel_tp_sl_order_for_position(
        market, subaccount_signer,
        order_book_types::OrderId { order_id },
    )?;
    // Place new SL order
    order_apis::place_tp_sl_order_for_position(
        market, subaccount_signer,
        None, None, None, // No TP
        sl_trigger_price, sl_limit_price, sl_size,
        None, // builder_code
    )?;
    Ok(())
}

/// Update an existing order for a subaccount.
pub fn update_order_to_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    order_id: u128,
    market: [u8; 32],
    price: u64,
    orig_size: u64,
    is_buy: bool,
    time_in_force: u8,
    is_reduce_only: bool,
    tp_trigger_price: Option<u64>,
    tp_limit_price: Option<u64>,
    sl_trigger_price: Option<u64>,
    sl_limit_price: Option<u64>,
    builder_address: Option<[u8; 32]>,
    builder_fees: Option<u64>,
) -> Result<(), u64> {
    let builder_code = perp_engine_api::get_builder_code_if_provided(builder_address, builder_fees);
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    let tif = order_book_types::time_in_force_from_index(time_in_force)?;
    let tp_sl_order_request = perp_order::new_order_tp_sl_args(
        tp_trigger_price, tp_limit_price, sl_trigger_price, sl_limit_price,
    );
    order_apis::update_order(
        subaccount_signer,
        order_book_types::OrderId { order_id },
        market, price, orig_size, is_buy, tif, is_reduce_only,
        tp_sl_order_request, builder_code,
    )
}

/// Update an existing order by client ID.
pub fn update_client_order_to_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    client_order_id: String,
    market: [u8; 32],
    price: u64,
    orig_size: u64,
    is_buy: bool,
    time_in_force: u8,
    is_reduce_only: bool,
    tp_trigger_price: Option<u64>,
    tp_limit_price: Option<u64>,
    sl_trigger_price: Option<u64>,
    sl_limit_price: Option<u64>,
    builder_address: Option<[u8; 32]>,
    builder_fees: Option<u64>,
) -> Result<(), u64> {
    let builder_code = perp_engine_api::get_builder_code_if_provided(builder_address, builder_fees);
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    let tif = order_book_types::time_in_force_from_index(time_in_force)?;
    let tp_sl_order_request = perp_order::new_order_tp_sl_args(
        tp_trigger_price, tp_limit_price, sl_trigger_price, sl_limit_price,
    );
    order_apis::update_client_order(
        subaccount_signer, client_order_id, market, price, orig_size,
        is_buy, tif, is_reduce_only, tp_sl_order_request, builder_code,
    )
}

/// Cancel an order for a subaccount.
pub fn cancel_order_to_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    order_id: u128,
    market: [u8; 32],
) -> Result<(), u64> {
    dex_accounts::cancel_perp_order_to_subaccount(auth, subaccount, order_id, market)
}

/// Cancel order by client ID.
pub fn cancel_client_order_to_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    client_order_id: String,
    market: [u8; 32],
) -> Result<(), u64> {
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    order_apis::cancel_client_order(market, subaccount_signer, client_order_id)
}

/// Place bulk orders for a subaccount.
pub fn place_bulk_orders_to_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
    sequence_number: u64,
    bid_prices: Vec<u64>,
    bid_sizes: Vec<u64>,
    ask_prices: Vec<u64>,
    ask_sizes: Vec<u64>,
    builder_address: Option<[u8; 32]>,
    builder_fees: Option<u64>,
) -> Result<(), u64> {
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    let builder_code = perp_engine_api::get_builder_code_if_provided(builder_address, builder_fees);
    order_apis::place_bulk_order(
        market, subaccount_signer, sequence_number,
        bid_prices, bid_sizes, ask_prices, ask_sizes, builder_code,
    );
    Ok(())
}

/// Cancel bulk orders for a subaccount.
pub fn cancel_bulk_order_to_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
) -> Result<(), u64> {
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    order_apis::cancel_bulk_order(market, subaccount_signer)
}

/// Place a TWAP order for a subaccount.
pub fn place_twap_order_to_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
    size: u64,
    is_buy: bool,
    is_reduce_only: bool,
    twap_frequency_s: u64,
    twap_duration_s: u64,
    builder_address: Option<[u8; 32]>,
    builder_fees: Option<u64>,
) -> Result<(), u64> {
    let builder_code = perp_engine_api::get_builder_code_if_provided(builder_address, builder_fees);
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    order_apis::place_twap_order(
        market, subaccount_signer, size, is_buy, is_reduce_only,
        None, twap_frequency_s, twap_duration_s, builder_code,
    )?;
    Ok(())
}

/// Place a TWAP order v2 (with client_order_id).
pub fn place_twap_order_to_subaccount_v2(
    auth: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
    size: u64,
    is_buy: bool,
    is_reduce_only: bool,
    client_order_id: Option<String>,
    twap_frequency_s: u64,
    twap_duration_s: u64,
    builder_address: Option<[u8; 32]>,
    builder_fees: Option<u64>,
) -> Result<(), u64> {
    let builder_code = perp_engine_api::get_builder_code_if_provided(builder_address, builder_fees);
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    order_apis::place_twap_order(
        market, subaccount_signer, size, is_buy, is_reduce_only,
        client_order_id, twap_frequency_s, twap_duration_s, builder_code,
    )?;
    Ok(())
}

/// Cancel a TWAP order for a subaccount.
pub fn cancel_twap_orders_to_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    market: [u8; 32],
    order_id: u128,
) -> Result<(), u64> {
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    order_apis::cancel_twap_order(
        market, subaccount_signer,
        order_book_types::OrderId { order_id },
    )
}

// ========== VAULT API ==========

pub fn contribute_to_vault(
    auth: [u8; 32],
    subaccount: [u8; 32],
    vault: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    // Delegates to dex_accounts_vault_extension (handled by dispatch layer)
    // In native context, vault operations are handled externally
    let _ = (auth, subaccount, vault, metadata, amount);
    Ok(())
}

pub fn redeem_from_vault(
    auth: [u8; 32],
    subaccount: [u8; 32],
    vault: [u8; 32],
    shares: u64,
) -> Result<(), u64> {
    // Delegates to dex_accounts_vault_extension (handled by dispatch layer)
    let _ = (auth, subaccount, vault, shares);
    Ok(())
}

// ========== REFERRAL & BUILDER CODE API ==========

/// Register a referral code.
pub fn register_referral_code(
    owner: [u8; 32],
    referral_code: String,
) -> Result<(), u64> {
    perp_engine_api::register_referral_code(owner, referral_code)
}

/// Register a referrer and create primary subaccount if needed.
pub fn register_referrer(
    owner: [u8; 32],
    referrer_code: String,
) -> Result<(), u64> {
    perp_engine_api::register_referrer(owner, referrer_code)?;
    let primary = dex_accounts::primary_subaccount(owner);
    if !dex_accounts::subaccount_exists(primary) {
        dex_accounts::create_primary_subaccount_object(owner);
    }
    Ok(())
}

/// Admin-create a primary subaccount.
pub fn admin_create_new_subaccount(
    admin: [u8; 32],
    user_addr: [u8; 32],
) -> [u8; 32] {
    dex_accounts::admin_create_new_primary_subaccount(admin, user_addr)
}

/// Revoke max builder fee for a subaccount.
pub fn revoke_max_builder_fee_for_subaccount(
    owner: [u8; 32],
    subaccount: [u8; 32],
    builder: [u8; 32],
) -> Result<(), u64> {
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner(owner, subaccount)?;
    perp_engine_api::revoke_max_fee(subaccount_signer, builder);
    Ok(())
}

/// Approve max builder fee for a subaccount.
pub fn approve_max_builder_fee_for_subaccount(
    auth: [u8; 32],
    subaccount: [u8; 32],
    builder: [u8; 32],
    max_fee: u64,
) -> Result<(), u64> {
    if !dex_accounts::subaccount_exists(builder) {
        return Err(EBUILDER_SUBACCOUNT_NOT_FOUND);
    }
    let subaccount_signer = dex_accounts::get_subaccount_signer_if_owner(auth, subaccount)?;
    perp_engine_api::approve_max_fee(subaccount_signer, builder, max_fee);
    Ok(())
}
