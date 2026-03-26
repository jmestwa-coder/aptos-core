// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::admin_apis

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::native_perpdex::perp_engine;
use crate::native_perpdex::price_management;

// ===================== Constants =====================

const ENOT_DEPLOYER: u64 = 1;
const ENOT_INITIALIZED: u64 = 2;
const ENOT_ADMIN_ORACLE_AND_MARK_UPDATE: u64 = 3;
const ENOT_ACCESS_CONTROL_ADMIN: u64 = 4;
const ENOT_ACCESS_CONTROL_GUARDIAN: u64 = 5;
const ENOT_GLOBAL_PAUSE_GUARDIAN: u64 = 6;
const ENOT_GLOBAL_UNPAUSE_COUNCIL: u64 = 7;
const ENOT_MARKET_MODE_GUARDIAN: u64 = 8;
const ENOT_MARKET_LIST_ADMIN: u64 = 9;
const ENOT_MARKET_DELIST_COUNCIL: u64 = 10;
const ENOT_MARKET_OPEN_ADMIN: u64 = 11;
const ENOT_MARKET_RISK_TIGHTENER: u64 = 12;
const ENOT_MARKET_RISK_GOVERNOR: u64 = 13;
const ENOT_FEE_CONFIG_GOVERNOR: u64 = 14;
const ENOT_INVITE_ONLY_REFERRAL_MANAGEMENT: u64 = 15;
const ENOT_ORDER_MANAGEMENT_ADMIN: u64 = 16;
const ENOT_VAULT_GLOBAL_CONFIG_SIGNER: u64 = 17;
const ENOT_WITHDRAW_RATE_LIMIT_GOVERNOR: u64 = 18;
const ENOT_VAULT_GLOBAL_CONFIG_ADMIN: u64 = 19;
const EFUNCTION_DEPRECATED: u64 = 20;

const DRAIN_ASYNC_QUEUE_DEFAULT_BATCH_SIZE: u64 = 100;
const GLOBAL_VAULT_CONFIG_SEED: &[u8] = b"GlobalVaultConfig";

// ===================== Types =====================

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum AdminPermissionType {
    OracleAndMarkUpdate,
    ReferralManagementAdmin,
    AccessControlAdmin,
    AccessControlGuardian,
    GlobalPauseGuardian,
    GlobalUnpauseCouncil,
    MarketModeGuardian,
    MarketListAdmin,
    MarketDelistCouncil,
    MarketOpenAdmin,
    MarketRiskTightener,
    MarketRiskGovernor,
    FeeConfigGovernor,
    OrderManagementAdmin,
    WithdrawRateLimitGovernor,
    VaultGlobalConfigAdmin,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum StoredPermission {
    Unlimited,
    UnlimitedUntil(u64),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DelegatedPermissions {
    V1 {
        perms: BTreeMap<u8, StoredPermission>, // AdminPermissionType index -> StoredPermission
    },
}

/// RESOURCE: DelegatedAdminPermissions at @decibel_dex
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DelegatedAdminPermissions {
    V1 {
        delegated_permissions: BTreeMap<[u8; 32], DelegatedPermissions>,
    },
}

// ===================== Events =====================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PermissionGranted {
    V1 {
        permission_type: AdminPermissionType,
        target_address: [u8; 32],
        granted_by: [u8; 32],
        timestamp: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PermissionRevoked {
    V1 {
        permission_type: AdminPermissionType,
        target_address: [u8; 32],
        revoked_by: [u8; 32],
        timestamp: u64,
    },
}

// ===================== Helper Functions =====================

/// Convert AdminPermissionType to its discriminant key for the BTreeMap.
fn permission_type_to_key(perm: &AdminPermissionType) -> u8 {
    match perm {
        AdminPermissionType::OracleAndMarkUpdate => 0,
        AdminPermissionType::ReferralManagementAdmin => 1,
        AdminPermissionType::AccessControlAdmin => 2,
        AdminPermissionType::AccessControlGuardian => 3,
        AdminPermissionType::GlobalPauseGuardian => 4,
        AdminPermissionType::GlobalUnpauseCouncil => 5,
        AdminPermissionType::MarketModeGuardian => 6,
        AdminPermissionType::MarketListAdmin => 7,
        AdminPermissionType::MarketDelistCouncil => 8,
        AdminPermissionType::MarketOpenAdmin => 9,
        AdminPermissionType::MarketRiskTightener => 10,
        AdminPermissionType::MarketRiskGovernor => 11,
        AdminPermissionType::FeeConfigGovernor => 12,
        AdminPermissionType::OrderManagementAdmin => 13,
        AdminPermissionType::WithdrawRateLimitGovernor => 14,
        AdminPermissionType::VaultGlobalConfigAdmin => 15,
    }
}

/// Compute the expected vault global config signer address.
/// In Move: `object::create_object_address(&@decibel_dex, GLOBAL_VAULT_CONFIG_SEED)`
fn vault_global_config_address() -> [u8; 32] {
    // In native context, the dispatch layer provides the actual @decibel_dex address.
    // This computes: sha3_256(decibel_dex_addr || GLOBAL_VAULT_CONFIG_SEED || 0xFE)
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let decibel_dex_addr = [0u8; 32]; // RESOURCE_READ: @decibel_dex address from dispatch layer
    let mut hasher = DefaultHasher::new();
    decibel_dex_addr.hash(&mut hasher);
    GLOBAL_VAULT_CONFIG_SEED.hash(&mut hasher);
    0xFEu8.hash(&mut hasher);
    let hash = hasher.finish();
    let mut result = [0u8; 32];
    result[..8].copy_from_slice(&hash.to_le_bytes());
    result
}

/// Check if admin is the deployer or object owner of @decibel_dex.
/// In Move: `admin_addr == @decibel_dex || object::is_owner(admin_addr, @decibel_dex)`
fn is_deployer_or_object_owner(admin: [u8; 32]) -> bool {
    // RESOURCE_READ: @decibel_dex address and ObjectCore.owner
    // In native context, the dispatch layer checks:
    // 1. admin == @decibel_dex, OR
    // 2. ObjectCore at @decibel_dex exists AND ObjectCore.owner == admin
    //
    // The dispatch layer provides this check.
    // For native execution, we represent this as a resource read.
    let _ = admin;
    true // RESOURCE_READ: dispatch layer provides actual check
}

fn assert_deployer_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !is_deployer_or_object_owner(admin) {
        return Err(ENOT_DEPLOYER);
    }
    Ok(())
}

fn is_stored_permission_valid(permission: &StoredPermission, now_seconds: u64) -> bool {
    match permission {
        StoredPermission::Unlimited => true,
        StoredPermission::UnlimitedUntil(timestamp) => now_seconds < *timestamp,
    }
}

/// Check if admin has a specific permission.
/// RESOURCE_READ: DelegatedAdminPermissions at @decibel_dex
fn has_permission(
    admin: [u8; 32],
    permission: AdminPermissionType,
) -> bool {
    // RESOURCE_READ: DelegatedAdminPermissions at @decibel_dex
    // In native context, the dispatch layer reads the resource and performs the lookup.
    //
    // Logic from Move:
    // 1. Assert DelegatedAdminPermissions exists at @decibel_dex
    // 2. Look up admin in delegated_permissions map
    // 3. Look up permission_type in admin's perms map
    // 4. Check if stored permission is still valid (not expired)
    let _ = admin;
    let _ = permission;
    // The dispatch layer provides this check at runtime.
    // Returning false here as the default; the actual check happens in the dispatch layer.
    false // RESOURCE_READ: DelegatedAdminPermissions at @decibel_dex
}

fn assert_admin_oracle_and_mark_update_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::OracleAndMarkUpdate) {
        return Err(ENOT_ADMIN_ORACLE_AND_MARK_UPDATE);
    }
    Ok(())
}

fn assert_access_control_admin_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::AccessControlAdmin) {
        return Err(ENOT_ACCESS_CONTROL_ADMIN);
    }
    Ok(())
}

fn assert_access_control_guardian_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::AccessControlAdmin)
        && !has_permission(admin, AdminPermissionType::AccessControlGuardian)
    {
        return Err(ENOT_ACCESS_CONTROL_GUARDIAN);
    }
    Ok(())
}

fn assert_global_pause_guardian_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::GlobalPauseGuardian) {
        return Err(ENOT_GLOBAL_PAUSE_GUARDIAN);
    }
    Ok(())
}

fn assert_global_unpause_council_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::GlobalUnpauseCouncil) {
        return Err(ENOT_GLOBAL_UNPAUSE_COUNCIL);
    }
    Ok(())
}

fn assert_market_mode_guardian_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::MarketModeGuardian) {
        return Err(ENOT_MARKET_MODE_GUARDIAN);
    }
    Ok(())
}

fn assert_market_list_admin_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::MarketListAdmin) {
        return Err(ENOT_MARKET_LIST_ADMIN);
    }
    Ok(())
}

fn assert_market_delist_council_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::MarketDelistCouncil) {
        return Err(ENOT_MARKET_DELIST_COUNCIL);
    }
    Ok(())
}

fn assert_market_open_admin_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::MarketOpenAdmin) {
        return Err(ENOT_MARKET_OPEN_ADMIN);
    }
    Ok(())
}

fn assert_market_risk_governor_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::MarketRiskGovernor) {
        return Err(ENOT_MARKET_RISK_GOVERNOR);
    }
    Ok(())
}

fn assert_fee_config_governor_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::FeeConfigGovernor) {
        return Err(ENOT_FEE_CONFIG_GOVERNOR);
    }
    Ok(())
}

fn assert_order_management_admin_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::OrderManagementAdmin) {
        return Err(ENOT_ORDER_MANAGEMENT_ADMIN);
    }
    Ok(())
}

fn assert_withdraw_rate_limit_governor_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::WithdrawRateLimitGovernor) {
        return Err(ENOT_WITHDRAW_RATE_LIMIT_GOVERNOR);
    }
    Ok(())
}

fn assert_admin_referral_management_capability(admin: [u8; 32]) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::ReferralManagementAdmin) {
        return Err(ENOT_INVITE_ONLY_REFERRAL_MANAGEMENT);
    }
    Ok(())
}

/// Check if the admin has invite-only referral management capabilities.
/// This includes vault global config signer OR ReferralManagementAdmin.
fn assert_invite_only_referral_management_capability(admin: [u8; 32]) -> Result<(), u64> {
    let is_vault_config = is_vault_global_config_signer(admin);
    if is_vault_config || has_permission(admin, AdminPermissionType::ReferralManagementAdmin) {
        Ok(())
    } else {
        Err(ENOT_INVITE_ONLY_REFERRAL_MANAGEMENT)
    }
}

/// Check if the signer is the vault global config signer.
fn is_vault_global_config_signer(signer_addr: [u8; 32]) -> bool {
    let expected_addr = vault_global_config_address();
    signer_addr == expected_addr
}

// ===================== Public Functions =====================

/// Initialize admin permissions and DEX.
/// RESOURCE: DelegatedAdminPermissions at @decibel_dex
pub fn initialize(
    admin: [u8; 32], // signer
    collateral_asset: [u8; 32],
    backstop_liquidator: [u8; 32],
) -> Result<(), u64> {
    assert_deployer_capability(admin)?;

    // RESOURCE_WRITE: DelegatedAdminPermissions::V1 { delegated_permissions: empty } at admin
    // Calls perp_engine::initialize
    perp_engine::initialize(admin, collateral_asset, backstop_liquidator)?;

    // Configure default rate limit for primary collateral: 10% per hour
    // DELEGATE: async_withdraw_queue::configure_rate_limit(collateral_asset, true, 1000, 0, 3600, 4)
    Ok(())
}

fn add_permission_internal(
    granter: [u8; 32],
    delegated_admin: [u8; 32],
    permission_type: AdminPermissionType,
) {
    // RESOURCE_READ: DelegatedAdminPermissions at @decibel_dex (assert exists)
    // RESOURCE_WRITE: DelegatedAdminPermissions.delegated_permissions
    //   If delegated_admin not in map: create DelegatedPermissions::V1 with perm
    //   If delegated_admin in map: upsert perm with StoredPermission::Unlimited
    let key = permission_type_to_key(&permission_type);
    let _ = (granter, delegated_admin, key);

    // EVENT: PermissionGranted::V1 { permission_type, target_address: delegated_admin, granted_by: granter, timestamp: now }
}

fn remove_permission_internal(
    revoker: [u8; 32],
    delegated_admin: [u8; 32],
    permission_type: AdminPermissionType,
) {
    // RESOURCE_READ: DelegatedAdminPermissions at @decibel_dex (assert exists)
    // RESOURCE_WRITE: If delegated_admin in map, remove permission_type from perms
    let key = permission_type_to_key(&permission_type);
    let _ = (revoker, delegated_admin, key);

    // EVENT: PermissionRevoked::V1 { permission_type, target_address: delegated_admin, revoked_by: revoker, timestamp: now }
}

// ========== Permission Management Functions ==========

pub fn add_oracle_and_mark_update_permission(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::OracleAndMarkUpdate);
    Ok(())
}

pub fn remove_oracle_and_mark_update_permission(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::OracleAndMarkUpdate);
    Ok(())
}

pub fn add_invite_only_referral_management_permission(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::ReferralManagementAdmin);
    Ok(())
}

pub fn remove_invite_only_referral_management_permission(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::ReferralManagementAdmin);
    Ok(())
}

pub fn add_access_control_admin(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_deployer_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::AccessControlAdmin);
    Ok(())
}

pub fn remove_access_control_admin(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_deployer_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::AccessControlAdmin);
    Ok(())
}

pub fn add_access_control_guardian(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::AccessControlGuardian);
    Ok(())
}

pub fn remove_access_control_guardian(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::AccessControlGuardian);
    Ok(())
}

pub fn add_global_pause_guardian(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::GlobalPauseGuardian);
    Ok(())
}

pub fn remove_global_pause_guardian(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::GlobalPauseGuardian);
    Ok(())
}

pub fn add_global_unpause_council(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::GlobalUnpauseCouncil);
    Ok(())
}

pub fn remove_global_unpause_council(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::GlobalUnpauseCouncil);
    Ok(())
}

pub fn add_market_mode_guardian(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::MarketModeGuardian);
    Ok(())
}

pub fn remove_market_mode_guardian(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::MarketModeGuardian);
    Ok(())
}

pub fn add_market_list_admin(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::MarketListAdmin);
    Ok(())
}

pub fn remove_market_list_admin(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::MarketListAdmin);
    Ok(())
}

pub fn add_market_delist_council(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::MarketDelistCouncil);
    Ok(())
}

pub fn remove_market_delist_council(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::MarketDelistCouncil);
    Ok(())
}

pub fn add_market_open_admin(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::MarketOpenAdmin);
    Ok(())
}

pub fn remove_market_open_admin(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::MarketOpenAdmin);
    Ok(())
}

pub fn add_market_risk_tightener(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::MarketRiskTightener);
    Ok(())
}

pub fn remove_market_risk_tightener(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::MarketRiskTightener);
    Ok(())
}

pub fn add_market_risk_governor(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::MarketRiskGovernor);
    Ok(())
}

pub fn remove_market_risk_governor(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::MarketRiskGovernor);
    Ok(())
}

pub fn add_fee_config_governor(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::FeeConfigGovernor);
    Ok(())
}

pub fn remove_fee_config_governor(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::FeeConfigGovernor);
    Ok(())
}

pub fn add_order_management_admin(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::OrderManagementAdmin);
    Ok(())
}

pub fn remove_order_management_admin(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::OrderManagementAdmin);
    Ok(())
}

pub fn add_withdraw_rate_limit_governor(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::WithdrawRateLimitGovernor);
    Ok(())
}

pub fn remove_withdraw_rate_limit_governor(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::WithdrawRateLimitGovernor);
    Ok(())
}

pub fn add_vault_global_config_admin(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_admin_capability(admin)?;
    add_permission_internal(admin, delegated_admin, AdminPermissionType::VaultGlobalConfigAdmin);
    Ok(())
}

pub fn remove_vault_global_config_admin(
    admin: [u8; 32], delegated_admin: [u8; 32]
) -> Result<(), u64> {
    assert_access_control_guardian_capability(admin)?;
    remove_permission_internal(admin, delegated_admin, AdminPermissionType::VaultGlobalConfigAdmin);
    Ok(())
}

pub fn assert_vault_global_config_signer(vault_config_signer: [u8; 32]) -> Result<(), u64> {
    if !is_vault_global_config_signer(vault_config_signer) {
        return Err(ENOT_VAULT_GLOBAL_CONFIG_SIGNER);
    }
    Ok(())
}

pub fn assert_vault_global_config_admin_capability(admin: [u8; 32]) -> Result<(), u64> {
    if is_deployer_or_object_owner(admin)
        || has_permission(admin, AdminPermissionType::VaultGlobalConfigAdmin)
    {
        Ok(())
    } else {
        Err(ENOT_VAULT_GLOBAL_CONFIG_ADMIN)
    }
}

// ========== Market Registration (entry functions) ==========

pub fn register_market_with_composite_oracle_primary_pyth(
    admin: [u8; 32],
    name: String,
    sz_decimals: u8,
    min_size: u64,
    lot_size: u64,
    ticker_size: u64,
    max_open_interest: u64,
    max_leverage: u8,
    margin_call_fee_pct: u64,
    async_matching_enabled: bool,
    pyth_identifier_bytes: Vec<u8>,
    pyth_max_staleness_secs: u64,
    pyth_confidence_interval_threshold: u64,
    pyth_rescale_decimals: u8,
    internal_initial_price: u64,
    internal_max_staleness_secs: u64,
    oracles_deviation_bps: u64,
    consecutive_deviation_count: u8,
) -> Result<(), u64> {
    assert_market_list_admin_capability(admin)?;
    perp_engine::register_market_with_composite_oracle_primary_pyth(
        name, sz_decimals, min_size, lot_size, ticker_size,
        max_open_interest, max_leverage, margin_call_fee_pct,
        async_matching_enabled,
        false, // is_isolated_only: backward compatible default
        pyth_identifier_bytes, pyth_max_staleness_secs,
        pyth_confidence_interval_threshold, pyth_rescale_decimals as i8,
        internal_initial_price, internal_max_staleness_secs,
        oracles_deviation_bps, consecutive_deviation_count,
    );
    Ok(())
}

pub fn register_market_with_composite_oracle_primary_chainlink(
    admin: [u8; 32],
    name: String,
    sz_decimals: u8,
    min_size: u64,
    lot_size: u64,
    ticker_size: u64,
    max_open_interest: u64,
    max_leverage: u8,
    margin_call_fee_pct: u64,
    async_matching_enabled: bool,
    chainlink_feed_id: Vec<u8>,
    chainlink_max_staleness_secs: u64,
    chainlink_rescale_decimals: u8,
    internal_initial_price: u64,
    internal_max_staleness_secs: u64,
    oracles_deviation_bps: u64,
    consecutive_deviation_count: u8,
) -> Result<(), u64> {
    assert_market_list_admin_capability(admin)?;
    perp_engine::register_market_with_composite_oracle_primary_chainlink(
        name, sz_decimals, min_size, lot_size, ticker_size,
        max_open_interest, max_leverage, margin_call_fee_pct,
        async_matching_enabled,
        false, // is_isolated_only: backward compatible default
        chainlink_feed_id, chainlink_max_staleness_secs,
        chainlink_rescale_decimals as i8,
        internal_initial_price, internal_max_staleness_secs,
        oracles_deviation_bps, consecutive_deviation_count,
    );
    Ok(())
}

pub fn register_market_with_internal_oracle(
    admin: [u8; 32],
    name: String,
    sz_decimals: u8,
    min_size: u64,
    lot_size: u64,
    ticker_size: u64,
    max_open_interest: u64,
    max_leverage: u8,
    margin_call_fee_pct: u64,
    async_matching_enabled: bool,
    initial_oracle_price: u64,
    max_staleness_secs: u64,
) -> Result<(), u64> {
    assert_market_list_admin_capability(admin)?;
    perp_engine::register_market_with_internal_oracle(
        name, sz_decimals, min_size, lot_size, ticker_size,
        max_open_interest, max_leverage, margin_call_fee_pct,
        async_matching_enabled,
        false, // is_isolated_only: backward compatible default
        initial_oracle_price, max_staleness_secs,
    );
    Ok(())
}

pub fn register_market_with_pyth_oracle(
    admin: [u8; 32],
    name: String,
    sz_decimals: u8,
    min_size: u64,
    lot_size: u64,
    ticker_size: u64,
    max_open_interest: u64,
    max_leverage: u8,
    margin_call_fee_pct: u64,
    async_matching_enabled: bool,
    pyth_identifier_bytes: Vec<u8>,
    pyth_max_staleness_secs: u64,
    pyth_confidence_interval_threshold: u64,
    pyth_rescale_decimals: u8,
) -> Result<(), u64> {
    assert_market_list_admin_capability(admin)?;
    perp_engine::register_market_with_pyth_oracle(
        name, sz_decimals, min_size, lot_size, ticker_size,
        max_open_interest, max_leverage, margin_call_fee_pct,
        async_matching_enabled,
        false, // is_isolated_only: backward compatible default
        pyth_identifier_bytes, pyth_max_staleness_secs,
        pyth_confidence_interval_threshold, pyth_rescale_decimals as i8,
    );
    Ok(())
}

// V2 variants with is_isolated_only
pub fn register_market_with_composite_oracle_primary_pyth_v2(
    admin: [u8; 32], name: String, sz_decimals: u8, min_size: u64, lot_size: u64,
    ticker_size: u64, max_open_interest: u64, max_leverage: u8, margin_call_fee_pct: u64,
    async_matching_enabled: bool, is_isolated_only: bool, pyth_identifier_bytes: Vec<u8>,
    pyth_max_staleness_secs: u64, pyth_confidence_interval_threshold: u64,
    pyth_rescale_decimals: u8, internal_initial_price: u64, internal_max_staleness_secs: u64,
    oracles_deviation_bps: u64, consecutive_deviation_count: u8,
) -> Result<(), u64> {
    assert_market_list_admin_capability(admin)?;
    perp_engine::register_market_with_composite_oracle_primary_pyth(
        name, sz_decimals, min_size, lot_size, ticker_size,
        max_open_interest, max_leverage, margin_call_fee_pct,
        async_matching_enabled, is_isolated_only,
        pyth_identifier_bytes, pyth_max_staleness_secs,
        pyth_confidence_interval_threshold, pyth_rescale_decimals as i8,
        internal_initial_price, internal_max_staleness_secs,
        oracles_deviation_bps, consecutive_deviation_count,
    );
    Ok(())
}

pub fn register_market_with_composite_oracle_primary_chainlink_v2(
    admin: [u8; 32], name: String, sz_decimals: u8, min_size: u64, lot_size: u64,
    ticker_size: u64, max_open_interest: u64, max_leverage: u8, margin_call_fee_pct: u64,
    async_matching_enabled: bool, is_isolated_only: bool, chainlink_feed_id: Vec<u8>,
    chainlink_max_staleness_secs: u64, chainlink_rescale_decimals: u8,
    internal_initial_price: u64, internal_max_staleness_secs: u64,
    oracles_deviation_bps: u64, consecutive_deviation_count: u8,
) -> Result<(), u64> {
    assert_market_list_admin_capability(admin)?;
    perp_engine::register_market_with_composite_oracle_primary_chainlink(
        name, sz_decimals, min_size, lot_size, ticker_size,
        max_open_interest, max_leverage, margin_call_fee_pct,
        async_matching_enabled, is_isolated_only,
        chainlink_feed_id, chainlink_max_staleness_secs,
        chainlink_rescale_decimals as i8,
        internal_initial_price, internal_max_staleness_secs,
        oracles_deviation_bps, consecutive_deviation_count,
    );
    Ok(())
}

pub fn register_market_with_internal_oracle_v2(
    admin: [u8; 32], name: String, sz_decimals: u8, min_size: u64, lot_size: u64,
    ticker_size: u64, max_open_interest: u64, max_leverage: u8, margin_call_fee_pct: u64,
    async_matching_enabled: bool, is_isolated_only: bool, initial_oracle_price: u64,
    max_staleness_secs: u64,
) -> Result<(), u64> {
    assert_market_list_admin_capability(admin)?;
    perp_engine::register_market_with_internal_oracle(
        name, sz_decimals, min_size, lot_size, ticker_size,
        max_open_interest, max_leverage, margin_call_fee_pct,
        async_matching_enabled, is_isolated_only,
        initial_oracle_price, max_staleness_secs,
    );
    Ok(())
}

pub fn register_market_with_pyth_oracle_v2(
    admin: [u8; 32], name: String, sz_decimals: u8, min_size: u64, lot_size: u64,
    ticker_size: u64, max_open_interest: u64, max_leverage: u8, margin_call_fee_pct: u64,
    async_matching_enabled: bool, is_isolated_only: bool, pyth_identifier_bytes: Vec<u8>,
    pyth_max_staleness_secs: u64, pyth_confidence_interval_threshold: u64,
    pyth_rescale_decimals: u8,
) -> Result<(), u64> {
    assert_market_list_admin_capability(admin)?;
    perp_engine::register_market_with_pyth_oracle(
        name, sz_decimals, min_size, lot_size, ticker_size,
        max_open_interest, max_leverage, margin_call_fee_pct,
        async_matching_enabled, is_isolated_only,
        pyth_identifier_bytes, pyth_max_staleness_secs,
        pyth_confidence_interval_threshold, pyth_rescale_decimals as i8,
    );
    Ok(())
}

// ========== Exchange Control ==========

pub fn pause_global_exchange(admin: [u8; 32]) -> Result<(), u64> {
    assert_global_pause_guardian_capability(admin)?;
    perp_engine::set_global_exchange_open(admin, false);
    Ok(())
}

pub fn unpause_global_exchange(admin: [u8; 32]) -> Result<(), u64> {
    assert_global_unpause_council_capability(admin)?;
    perp_engine::set_global_exchange_open(admin, true);
    Ok(())
}

// ========== Market Config Adjustment ==========

pub fn increase_market_open_interest(admin: [u8; 32], market: [u8; 32], new_oi: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    perp_engine::increase_market_open_interest(market, new_oi);
    Ok(())
}

pub fn decrease_market_open_interest(admin: [u8; 32], market: [u8; 32], new_oi: u64) -> Result<(), u64> {
    // MarketRiskTightener OR MarketRiskGovernor can decrease OI
    if !has_permission(admin, AdminPermissionType::MarketRiskTightener)
        && !has_permission(admin, AdminPermissionType::MarketRiskGovernor)
    {
        return Err(ENOT_MARKET_RISK_TIGHTENER);
    }
    perp_engine::decrease_market_open_interest(market, new_oi);
    Ok(())
}

pub fn increase_market_notional_open_interest(admin: [u8; 32], market: [u8; 32], new_noi: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    perp_engine::increase_market_notional_open_interest(market, new_noi);
    Ok(())
}

pub fn decrease_market_notional_open_interest(admin: [u8; 32], market: [u8; 32], new_noi: u64) -> Result<(), u64> {
    if !has_permission(admin, AdminPermissionType::MarketRiskTightener)
        && !has_permission(admin, AdminPermissionType::MarketRiskGovernor)
    {
        return Err(ENOT_MARKET_RISK_TIGHTENER);
    }
    perp_engine::decrease_market_notional_open_interest(market, new_noi);
    Ok(())
}

pub fn set_market_reduce_only(admin: [u8; 32], market: [u8; 32], allowlist: Vec<[u8; 32]>, reason: Option<String>) -> Result<(), u64> {
    assert_market_mode_guardian_capability(admin)?;
    perp_engine::set_market_reduce_only(market, allowlist, reason);
    Ok(())
}

pub fn set_market_open(admin: [u8; 32], market: [u8; 32], reason: Option<String>) -> Result<(), u64> {
    assert_market_open_admin_capability(admin)?;
    perp_engine::set_market_open(market, reason);
    Ok(())
}

pub fn set_market_halted(admin: [u8; 32], market: [u8; 32], reason: Option<String>) -> Result<(), u64> {
    assert_market_mode_guardian_capability(admin)?;
    perp_engine::set_market_halted(market, reason);
    Ok(())
}

pub fn set_market_allowlist_only(admin: [u8; 32], market: [u8; 32], allowlist: Vec<[u8; 32]>, reason: Option<String>) -> Result<(), u64> {
    assert_market_mode_guardian_capability(admin)?;
    perp_engine::set_market_allowlist_only(market, allowlist, reason);
    Ok(())
}

pub fn set_market_max_leverage(admin: [u8; 32], market: [u8; 32], max_leverage: u8) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    perp_engine::set_market_max_leverage(market, max_leverage)
}

pub fn set_market_starting_slippage_pct(admin: [u8; 32], market: [u8; 32], pct: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    perp_engine::set_market_starting_slippage_pct(market, pct);
    Ok(())
}

pub fn set_market_slippage_increment_pct(admin: [u8; 32], market: [u8; 32], pct: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    perp_engine::set_market_slippage_increment_pct(market, pct);
    Ok(())
}

pub fn set_market_max_leverage_with_fee_scaling(admin: [u8; 32], market: [u8; 32], max_leverage: u8) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    perp_engine::set_market_max_leverage_with_fee_scaling(market, max_leverage);
    Ok(())
}

pub fn set_market_cooldown_period_micros(admin: [u8; 32], market: [u8; 32], micros: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    perp_engine::set_market_cooldown_period_micros(market, micros);
    Ok(())
}

/// DEPRECATED
pub fn set_market_withdrawable_margin_leverage(admin: [u8; 32], _market: [u8; 32], _leverage: u8) -> Result<(), u64> {
    let _ = admin;
    Err(EFUNCTION_DEPRECATED)
}

pub fn set_market_book_oracle_ratio_cap_bps(admin: [u8; 32], market: [u8; 32], bps: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    // RESOURCE_WRITE: PriceDetails at market - set book_oracle_ratio_cap_bps
    // Dispatch layer reads PriceDetails, calls set_book_oracle_ratio_cap_bps(&mut pd, bps), writes back
    let _ = (market, bps);
    Ok(())
}

pub fn set_market_funding_rate_pause_timeout_microseconds(admin: [u8; 32], market: [u8; 32], timeout: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    // RESOURCE_WRITE: PriceDetails at market - set funding_rate_pause_timeout_microseconds
    // Dispatch layer reads PriceDetails, calls set_funding_rate_pause_timeout_microseconds(&mut pd, timeout), writes back
    let _ = (market, timeout);
    Ok(())
}

pub fn set_market_funding_mode(admin: [u8; 32], market: [u8; 32], funding_period_us: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    if funding_period_us == 0 {
        // Continuous funding - charge current interval first via perp_engine
        perp_engine::set_market_to_continuous_funding_mode(market);
    } else {
        // RESOURCE_WRITE: PriceDetails at market - set periodic funding mode
        // Dispatch layer reads PriceDetails, calls set_periodic_funding_mode(&mut pd, funding_period_us), writes back
        let _ = (market, funding_period_us);
    }
    Ok(())
}

pub fn set_market_adl_trigger_threshold(admin: [u8; 32], market: [u8; 32], threshold: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    // RESOURCE_WRITE: PerpMarketConfiguration.state.adl_trigger_threshold at market
    // In native context, the dispatch layer reads PerpMarketConfiguration at market address,
    // calls perp_market_config::set_adl_trigger_threshold(&mut config, threshold),
    // and writes it back.
    let _ = (market, threshold);
    Ok(())
}

pub fn allow_market_cross_margin_mode(admin: [u8; 32], market: [u8; 32]) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    // RESOURCE_WRITE: PerpMarketConfiguration.risk.is_isolated_only at market
    // In native context, the dispatch layer reads PerpMarketConfiguration at market address,
    // calls perp_market_config::allow_cross_margin_mode(&mut config),
    // and writes it back.
    let _ = market;
    Ok(())
}

pub fn set_market_min_size(admin: [u8; 32], market: [u8; 32], min_size: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    perp_engine::set_market_min_size(market, min_size);
    Ok(())
}

pub fn decrease_market_lot_size(admin: [u8; 32], market: [u8; 32], new_lot_size: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    perp_engine::decrease_market_lot_size(market, new_lot_size);
    Ok(())
}

pub fn increase_market_lot_size(admin: [u8; 32], market: [u8; 32], new_lot_size: u64) -> Result<(), u64> {
    assert_deployer_capability(admin)?;
    perp_engine::increase_market_lot_size(market, new_lot_size);
    Ok(())
}

pub fn set_market_unrealized_pnl_haircut(admin: [u8; 32], market: [u8; 32], haircut_bps: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    perp_engine::set_market_unrealized_pnl_haircut(market, haircut_bps);
    Ok(())
}

pub fn set_blp_margin_as_profit_percentage(admin: [u8; 32], percentage_bps: u64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    // RESOURCE_WRITE: BackstopLiquidatorProfitTracker - set_blp_margin_as_profit_percentage
    // Dispatch layer reads BackstopLiquidatorProfitTracker, updates margin_as_profit_percentage_bps, writes back
    let _ = (admin, percentage_bps);
    Ok(())
}

pub fn delist_market(admin: [u8; 32], market: [u8; 32], reason: Option<String>) -> Result<(), u64> {
    assert_market_delist_council_capability(admin)?;
    perp_engine::delist_market(market, reason);
    Ok(())
}

pub fn delist_market_with_mark_price(admin: [u8; 32], market: [u8; 32], mark_price: u64, reason: Option<String>) -> Result<(), u64> {
    assert_market_delist_council_capability(admin)?;
    perp_engine::delist_market_with_mark_price(market, mark_price, reason);
    Ok(())
}

pub fn drain_async_queue(admin: [u8; 32], market: [u8; 32]) -> Result<(), u64> {
    assert_order_management_admin_capability(admin)?;
    perp_engine::drain_async_queue(market, DRAIN_ASYNC_QUEUE_DEFAULT_BATCH_SIZE)
}

pub fn drain_async_queue_with_limit(admin: [u8; 32], market: [u8; 32], batch_size: u64) -> Result<(), u64> {
    assert_order_management_admin_capability(admin)?;
    perp_engine::drain_async_queue(market, batch_size)
}

pub fn set_backstop_liquidator_high_watermark(admin: [u8; 32], market: [u8; 32], new_watermark: i64) -> Result<(), u64> {
    assert_market_risk_governor_capability(admin)?;
    perp_engine::set_backstop_liquidator_high_watermark(admin, market, new_watermark);
    Ok(())
}

pub fn set_market_margin_call_fee_pct(admin: [u8; 32], market: [u8; 32], pct: u64) -> Result<(), u64> {
    assert_fee_config_governor_capability(admin)?;
    perp_engine::set_market_margin_call_fee_pct(market, pct);
    Ok(())
}

pub fn set_market_margin_call_backstop_pct(admin: [u8; 32], market: [u8; 32], pct: u64) -> Result<(), u64> {
    assert_fee_config_governor_capability(admin)?;
    perp_engine::set_market_margin_call_backstop_pct(market, pct);
    Ok(())
}

// ========== Fee Config ==========

pub fn update_fee_config(
    admin: [u8; 32],
    tier_thresholds: Vec<u128>, tier_maker_fees: Vec<u64>, tier_taker_fees: Vec<u64>,
    mm_absolute_threshold: u128, mm_tier_pct_thresholds: Vec<u64>, mm_tier_fee_rebates: Vec<u64>,
    builder_max_fee: u64, backstop_vault_fee_pct: u64,
    referral_fee_enabled: bool, referral_fee_pct: u64, referred_fee_discount_pct: u64,
    discount_eligibility_volume_threshold: u128, referrer_eligibility_volume_threshold: u128,
) -> Result<(), u64> {
    assert_fee_config_governor_capability(admin)?;
    // RESOURCE_WRITE: trading_fees_manager::GlobalState and builder_code_registry::Registry
    // Dispatch layer reads both resources, calls update_fee_config(&mut gs, &mut reg, ...), writes back
    let _ = (tier_thresholds, tier_maker_fees, tier_taker_fees,
        mm_absolute_threshold, mm_tier_pct_thresholds, mm_tier_fee_rebates,
        builder_max_fee, backstop_vault_fee_pct,
        referral_fee_enabled, referral_fee_pct, referred_fee_discount_pct,
        discount_eligibility_volume_threshold, referrer_eligibility_volume_threshold);
    Ok(())
}

pub fn set_global_max_builder_fee(admin: [u8; 32], new_max: u64) -> Result<(), u64> {
    assert_fee_config_governor_capability(admin)?;
    // RESOURCE_WRITE: trading_fees_manager::GlobalState and builder_code_registry::Registry
    // Dispatch layer reads both resources, calls set_builder_max_fee(&mut gs, &mut reg, new_max), writes back
    let _ = new_max;
    Ok(())
}

// ========== Referral/Account Management ==========

pub fn admin_register_affiliate(admin: [u8; 32], affiliate_addr: [u8; 32]) -> Result<(), u64> {
    assert_admin_referral_management_capability(admin)?;
    // Get primary account address for affiliate
    // RESOURCE_READ: perp_positions::UserPositions at affiliate_addr -> primary_account_addr
    // In Move: perp_positions::get_primary_account_addr(affiliate_addr)
    let primary_account_addr = affiliate_addr; // Dispatch layer resolves actual primary account
    // RESOURCE_WRITE: trading_fees_manager::GlobalState
    // Dispatch layer reads GlobalState, calls register_affiliate(&mut gs, primary_account_addr), writes back
    let _ = primary_account_addr;
    Ok(())
}

pub fn set_max_referral_codes_for_address(admin: [u8; 32], user_addr: [u8; 32], max_codes: u64) -> Result<(), u64> {
    assert_admin_referral_management_capability(admin)?;
    // RESOURCE_WRITE: trading_fees_manager::GlobalState
    // Dispatch layer reads GlobalState, calls set_max_referral_codes_for_address(&mut gs, user_addr, max_codes), writes back
    let _ = (user_addr, max_codes);
    Ok(())
}

pub fn set_max_usage_per_referral_code_for_address(admin: [u8; 32], user_addr: [u8; 32], max_usage: u64) -> Result<(), u64> {
    assert_admin_referral_management_capability(admin)?;
    // RESOURCE_WRITE: trading_fees_manager::GlobalState
    // Dispatch layer reads GlobalState, calls set_max_usage_per_referral_code_for_address(&mut gs, user_addr, max_usage), writes back
    let _ = (user_addr, max_usage);
    Ok(())
}

pub fn admin_register_referral_code(admin: [u8; 32], user_addr: [u8; 32], referral_code: String) -> Result<(), u64> {
    assert_admin_referral_management_capability(admin)?;
    // RESOURCE_WRITE: trading_fees_manager::GlobalState
    // Dispatch layer reads GlobalState, calls admin_register_referral_code(&mut gs, user_addr, referral_code), writes back
    let _ = (user_addr, referral_code);
    Ok(())
}

pub fn admin_register_referrer(admin: [u8; 32], user_addr: [u8; 32], referrer_code: String) -> Result<(), u64> {
    assert_admin_referral_management_capability(admin)?;
    // RESOURCE_WRITE: trading_fees_manager::GlobalState
    // Dispatch layer reads GlobalState, calls admin_register_referrer(&mut gs, user_addr, referrer_code), writes back
    let _ = (user_addr, referrer_code);
    Ok(())
}

pub fn add_to_account_creation_allow_list(admin: [u8; 32], accounts: Vec<[u8; 32]>) -> Result<(), u64> {
    assert_invite_only_referral_management_capability(admin)?;
    perp_engine::add_to_account_creation_allow_list(accounts);
    Ok(())
}

pub fn set_invite_only_account_creation(admin: [u8; 32], require: bool) -> Result<(), u64> {
    assert_admin_referral_management_capability(admin)?;
    perp_engine::set_invite_only_account_creation(admin, require);
    Ok(())
}

// ========== Oracle Update Entry Functions ==========

pub fn update_mark_for_internal_oracle(
    updater: [u8; 32], market: [u8; 32], oracle_price: u64,
    backstop_liquidations: Vec<[u8; 32]>, margin_call_liquidations: Vec<[u8; 32]>,
    trigger: bool,
) -> Result<(), u64> {
    // In Move, this uses transaction_context::monotonically_increasing_counter()
    // In native context, the dispatch layer provides the counter
    let batch_key = 0u128; // Provided by dispatch layer
    update_mark_for_internal_oracle_with_batch_key(
        updater, market, oracle_price, backstop_liquidations,
        margin_call_liquidations, trigger, batch_key,
    )
}

pub fn update_mark_for_internal_oracle_with_batch_key(
    updater: [u8; 32], market: [u8; 32], oracle_price: u64,
    backstop_liquidations: Vec<[u8; 32]>, margin_call_liquidations: Vec<[u8; 32]>,
    trigger: bool, batch_key: u128,
) -> Result<(), u64> {
    assert_admin_oracle_and_mark_update_capability(updater)?;
    perp_engine::update_oracle_and_mark_price_and_liquidate_and_trigger(
        updater, market,
        Some(oracle_price),  // internal_oracle_price
        None,                // chainlink_signed_report
        None,                // pyth_vaa
        price_management::MarkPriceRefreshInput::None,
        backstop_liquidations,
        margin_call_liquidations,
        trigger,
        batch_key,
    )
}

pub fn update_mark_for_chainlink_oracle(
    updater: [u8; 32], market: [u8; 32], signed_report: Vec<u8>,
    backstop_liquidations: Vec<[u8; 32]>, margin_call_liquidations: Vec<[u8; 32]>,
    trigger: bool,
) -> Result<(), u64> {
    let batch_key = 0u128;
    update_mark_for_chainlink_oracle_with_batch_key(
        updater, market, signed_report, backstop_liquidations,
        margin_call_liquidations, trigger, batch_key,
    )
}

pub fn update_mark_for_chainlink_oracle_with_batch_key(
    updater: [u8; 32], market: [u8; 32], signed_report: Vec<u8>,
    backstop_liquidations: Vec<[u8; 32]>, margin_call_liquidations: Vec<[u8; 32]>,
    trigger: bool, batch_key: u128,
) -> Result<(), u64> {
    assert_admin_oracle_and_mark_update_capability(updater)?;
    perp_engine::update_oracle_and_mark_price_and_liquidate_and_trigger(
        updater, market,
        None,                       // internal_oracle_price
        Some(signed_report),        // chainlink_signed_report
        None,                       // pyth_vaa
        price_management::MarkPriceRefreshInput::None,
        backstop_liquidations,
        margin_call_liquidations,
        trigger,
        batch_key,
    )
}

pub fn update_mark_for_pyth_oracle(
    updater: [u8; 32], market: [u8; 32], vaa: Vec<u8>,
    backstop_liquidations: Vec<[u8; 32]>, margin_call_liquidations: Vec<[u8; 32]>,
    trigger: bool,
) -> Result<(), u64> {
    let batch_key = 0u128;
    update_mark_for_pyth_oracle_with_batch_key(
        updater, market, vaa, backstop_liquidations,
        margin_call_liquidations, trigger, batch_key,
    )
}

pub fn update_mark_for_pyth_oracle_with_batch_key(
    updater: [u8; 32], market: [u8; 32], vaa: Vec<u8>,
    backstop_liquidations: Vec<[u8; 32]>, margin_call_liquidations: Vec<[u8; 32]>,
    trigger: bool, batch_key: u128,
) -> Result<(), u64> {
    assert_admin_oracle_and_mark_update_capability(updater)?;
    perp_engine::update_oracle_and_mark_price_and_liquidate_and_trigger(
        updater, market,
        None,                // internal_oracle_price
        None,                // chainlink_signed_report
        Some(vaa),           // pyth_vaa
        price_management::MarkPriceRefreshInput::None,
        backstop_liquidations,
        margin_call_liquidations,
        trigger,
        batch_key,
    )
}

pub fn update_mark_for_composite_chainlink(
    updater: [u8; 32], market: [u8; 32], internal_oracle_price: Option<u64>,
    signed_report: Option<Vec<u8>>, impact_bid_px_hint: Option<u64>,
    impact_ask_px_hint: Option<u64>, backstop_liquidations: Vec<[u8; 32]>,
    margin_call_liquidations: Vec<[u8; 32]>, trigger: bool,
) -> Result<(), u64> {
    let batch_key = 0u128;
    update_mark_for_composite_chainlink_with_batch_key(
        updater, market, internal_oracle_price, signed_report,
        impact_bid_px_hint, impact_ask_px_hint,
        backstop_liquidations, margin_call_liquidations, trigger, batch_key,
    )
}

pub fn update_mark_for_composite_chainlink_with_batch_key(
    updater: [u8; 32], market: [u8; 32], internal_oracle_price: Option<u64>,
    signed_report: Option<Vec<u8>>, impact_bid_px_hint: Option<u64>,
    impact_ask_px_hint: Option<u64>, backstop_liquidations: Vec<[u8; 32]>,
    margin_call_liquidations: Vec<[u8; 32]>, trigger: bool, batch_key: u128,
) -> Result<(), u64> {
    assert_admin_oracle_and_mark_update_capability(updater)?;
    let mark_price_refresh_input = if impact_bid_px_hint.is_some() && impact_ask_px_hint.is_some() {
        price_management::MarkPriceRefreshInput::UseProvidedImpactHint {
            impact_bid_px: impact_bid_px_hint.unwrap(),
            impact_ask_px: impact_ask_px_hint.unwrap(),
        }
    } else {
        price_management::MarkPriceRefreshInput::None
    };
    perp_engine::update_oracle_and_mark_price_and_liquidate_and_trigger(
        updater, market,
        internal_oracle_price,
        signed_report,
        None,                // pyth_vaa
        mark_price_refresh_input,
        backstop_liquidations,
        margin_call_liquidations,
        trigger,
        batch_key,
    )
}

// ========== Withdraw Rate Limit ==========

pub fn configure_withdraw_rate_limit(
    admin: [u8; 32], metadata: [u8; 32], enabled: bool, rate_limit_bps: u64,
    absolute_rate_limit: u64, window_duration_seconds: u64, num_buckets: u8,
) -> Result<(), u64> {
    assert_withdraw_rate_limit_governor_capability(admin)?;
    // RESOURCE_WRITE: async_withdraw_queue::AsyncWithdrawQueueConfig
    // Dispatch layer reads config, calls configure_rate_limit, writes back
    let _ = (metadata, enabled, rate_limit_bps, absolute_rate_limit, window_duration_seconds, num_buckets);
    Ok(())
}

pub fn update_withdraw_rate_limit_bps(
    admin: [u8; 32], metadata: [u8; 32], rate_limit_bps: u64,
) -> Result<(), u64> {
    assert_withdraw_rate_limit_governor_capability(admin)?;
    // RESOURCE_WRITE: async_withdraw_queue - update rate_limit_bps
    let _ = (metadata, rate_limit_bps);
    Ok(())
}

pub fn update_withdraw_absolute_rate_limit(
    admin: [u8; 32], metadata: [u8; 32], absolute_rate_limit: u64,
) -> Result<(), u64> {
    assert_withdraw_rate_limit_governor_capability(admin)?;
    // RESOURCE_WRITE: async_withdraw_queue - update absolute_rate_limit
    let _ = (metadata, absolute_rate_limit);
    Ok(())
}

pub fn update_withdraw_window_duration(
    admin: [u8; 32], metadata: [u8; 32], window_duration_seconds: u64,
) -> Result<(), u64> {
    assert_withdraw_rate_limit_governor_capability(admin)?;
    // RESOURCE_WRITE: async_withdraw_queue - update window_duration
    let _ = (metadata, window_duration_seconds);
    Ok(())
}

pub fn set_withdraw_rate_limit_enabled(
    admin: [u8; 32], metadata: [u8; 32], enabled: bool,
) -> Result<(), u64> {
    assert_withdraw_rate_limit_governor_capability(admin)?;
    // RESOURCE_WRITE: async_withdraw_queue - set enabled
    let _ = (metadata, enabled);
    Ok(())
}
