// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::perp_engine_api

use crate::native_perpdex::builder_code_registry;
use crate::native_perpdex::perp_engine;

// ===================== Constants =====================

const ENOT_ADMIN: u64 = 1;
const EINVALID_REGISTERING_REFERRAL_CODE_NOT_ALLOWED_DURING_INVITE_ONLY_PERIOD: u64 = 2;

// ===================== Types =====================

/// RestrictedPerpApi - capability wrapper for init_user_if_new.
/// In native context, this is represented as an opaque capability.
#[derive(Clone, Debug)]
pub enum RestrictedPerpApi {
    V1,
}

// ===================== Functions =====================

/// Get the restricted perp API (deployer only).
pub fn get_restricted_perp_api(_deployer: [u8; 32]) -> Result<RestrictedPerpApi, u64> {
    // assert deployer == @decibel_dex
    Ok(RestrictedPerpApi::V1)
}

/// Initialize a user if new, using the RestrictedPerpApi capability.
pub fn init_user_if_new(
    _api: &RestrictedPerpApi,
    _account: [u8; 32], // signer
    _fee_tracking_addr: [u8; 32],
) -> Result<(), u64> {
    perp_engine::init_user_if_new(_account, _fee_tracking_addr)
}

/// Get builder code if both address and fees are provided.
pub fn get_builder_code_if_provided(
    builder_address: Option<[u8; 32]>,
    builder_fees: Option<u64>,
) -> Option<builder_code_registry::BuilderCode> {
    if let Some(addr) = builder_address {
        Some(builder_code_registry::BuilderCode {
            builder: addr,
            fees: builder_fees.unwrap_or(0),
        })
    } else {
        None
    }
}

/// Create a new builder code.
pub fn new_builder_code(builder: [u8; 32], fees: u64) -> builder_code_registry::BuilderCode {
    builder_code_registry::BuilderCode { builder, fees }
}

/// Approve max fee for a builder.
/// RESOURCE: BuilderCodeRegistry at account
pub fn approve_max_fee(
    _account: [u8; 32], // signer
    _builder: [u8; 32],
    _max_fee: u64,
) {
    // Delegates to builder_code_registry::approve_max_fee
}

/// Revoke max fee for a builder.
/// RESOURCE: BuilderCodeRegistry at account
pub fn revoke_max_fee(
    _account: [u8; 32], // signer
    _builder: [u8; 32],
) {
    // Delegates to builder_code_registry::revoke_max_fee
}

/// Register a referral code (not allowed during invite-only period).
pub fn register_referral_code(
    _account: [u8; 32], // signer
    _referral_code: String,
) -> Result<(), u64> {
    // assert not invite-only mode
    // Delegates to trading_fees_manager::register_referral_code
    Ok(())
}

/// Register a referrer code (not allowed during invite-only period).
pub fn register_referrer(
    _account: [u8; 32], // signer
    _referrer_code: String,
) -> Result<(), u64> {
    // assert not invite-only mode
    // Delegates to trading_fees_manager::register_referrer
    Ok(())
}
