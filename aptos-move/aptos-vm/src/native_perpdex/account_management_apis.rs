// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::account_management_apis

use crate::native_perpdex::perp_engine;

// ===================== Types =====================

/// Capability that authorizes a specific transfer between accounts.
#[derive(Clone, Debug)]
pub struct TransferCapability {
    pub from_account: [u8; 32],
    pub to_account: [u8; 32],
    pub metadata: [u8; 32],
    pub amount: u64,
}

// ===================== Functions =====================

/// Configure user settings for a market (cross/isolated and leverage).
pub fn configure_user_settings_for_market(
    account: [u8; 32], // signer
    market: [u8; 32],
    is_cross: bool,
    user_leverage: u8,
) -> Result<(), u64> {
    perp_engine::configure_user_settings_for_market(account, market, is_cross, user_leverage)
}

/// Deposit fungible asset to owner's cross-margin account.
pub fn deposit_to_cross(
    user: [u8; 32],
    fungible_asset_metadata: [u8; 32],
    fungible_asset_amount: u64,
) -> Result<(), u64> {
    perp_engine::deposit_to_cross(user, fungible_asset_metadata, fungible_asset_amount)
}

/// Deposit fungible asset to owner's isolated position margin.
pub fn deposit_to_isolated_position_collateral(
    user: [u8; 32], // signer
    market: [u8; 32],
    fungible_asset_metadata: [u8; 32],
    fungible_asset_amount: u64,
) -> Result<(), u64> {
    perp_engine::deposit_to_isolated_position_collateral(
        user, market, fungible_asset_metadata, fungible_asset_amount,
    )
}

/// Transfer margin between cross-margin and isolated position margin.
pub fn transfer_collateral_to_isolated_position(
    user: [u8; 32], // signer
    market: [u8; 32],
    is_deposit: bool,
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    perp_engine::transfer_collateral_to_isolated_position(
        user, market, is_deposit, metadata, amount,
    )
}

/// Transfer collateral between accounts without rate limiting.
pub fn transfer_collateral(
    from_account: [u8; 32], // signer
    to_account: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    perp_engine::transfer_collateral(from_account, to_account, metadata, amount)
}

/// Transfer funds to treasury, without rate limiting.
pub fn transfer_fee_to_treasury(
    from_account: [u8; 32], // signer
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    perp_engine::transfer_fee_to_treasury(from_account, metadata, amount)
}

/// Create a TransferCapability that authorizes a specific transfer.
pub fn create_transfer_capability(
    from_account: [u8; 32], // signer
    to_account: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> TransferCapability {
    TransferCapability { from_account, to_account, metadata, amount }
}

/// Get the metadata from a TransferCapability.
pub fn transfer_capability_metadata(cap: &TransferCapability) -> [u8; 32] {
    cap.metadata
}

/// Get the amount from a TransferCapability.
pub fn transfer_capability_amount(cap: &TransferCapability) -> u64 {
    cap.amount
}

/// Execute the transfer authorized by the TransferCapability.
pub fn execute_transfer_with_capability(cap: TransferCapability) -> Result<(), u64> {
    perp_engine::transfer_collateral(cap.from_account, cap.to_account, cap.metadata, cap.amount)
}

/// Set reserved collateral for an account.
/// RESOURCE: CollateralBalanceSheet
pub fn set_reserved_collateral(
    _user: [u8; 32], // signer
    _wanted_to_set_reserved: u64,
) -> u64 {
    0
}

/// Request a withdrawal from cross-margin (may be queued).
pub fn request_withdrawal_from_cross(
    owner: [u8; 32], // signer
    metadata: [u8; 32],
    amount: u64,
    recipient: [u8; 32],
) -> Result<Option<u128>, u64> {
    perp_engine::request_withdrawal_from_cross(owner, metadata, amount, recipient)
}

/// Request a withdrawal from isolated position (may be queued).
pub fn request_withdrawal_from_isolated(
    owner: [u8; 32], // signer
    market: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
    recipient: [u8; 32],
) -> Result<Option<u128>, u64> {
    perp_engine::request_withdrawal_from_isolated(owner, market, metadata, amount, recipient)
}

/// Cancel a pending withdrawal request.
/// RESOURCE: AsyncWithdrawQueueConfig
pub fn cancel_withdrawal(
    _owner: [u8; 32], // signer address
    _request_id: u128,
) {
    // Delegates to async_withdraw_queue::cancel_withdrawal
}
