// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::dex_accounts

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::native_perpdex::order_book_types;
use crate::native_perpdex::builder_code_registry;
use crate::native_perpdex::perp_order;

// ===================== Error Codes =====================

const ENOT_SUBACCOUNT_OWNER: u64 = 1;
const ESUBACCOUNT_DOESNT_EXIST: u64 = 2;
const ENOT_SUBACCOUNT_OWNER_OR_LACKS_PERP_TRADING_PERMISSIONS: u64 = 3;
const ENOT_SUBACCOUNT_OWNER_OR_LACKS_VAULT_TRADING_PERMISSIONS: u64 = 4;
const ENOT_SUBACCOUNT_OWNER_OR_LACKS_FUNDS_MOVEMENT_PERMISSIONS: u64 = 5;
const ENOT_SUBACCOUNT_OWNER_OR_LACKS_SUB_DELEGATION_PERMISSIONS: u64 = 6;
const ECANNOT_TRANSFER_BETWEEN_DIFFERENT_OWNERS: u64 = 7;
const ESUBACCOUNT_IS_NOT_ACTIVE: u64 = 8;
const ESUBACCOUNT_HAS_ASSETS_OR_POSITIONS: u64 = 9;
const ESEED_CANNOT_BE_PRIMARY_SUBACCOUNT_SEED: u64 = 10;
const EINVALID_PUBLISHER: u64 = 11;
const ERESTRICTED_API_ALREADY_REGISTERED: u64 = 12;
const ERESTRICTED_API_NOT_REGISTERED: u64 = 13;

// ===================== Constants =====================

const PRIMARY_SUBACCOUNT_SEED: &[u8] = b"primary_subaccount";
const SUBACCOUNT_MANAGER_SEED: &[u8] = b"GlobalSubaccountManager";
const MAX_DELEGATION_DEPTH: u64 = 2;

// ===================== Types =====================

/// RESOURCE: Subaccount at subaccount object address
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Subaccount {
    V1 {
        extend_ref: Vec<u8>, // ExtendRef opaque
        delegated_permissions: Vec<([u8; 32], DelegatedPermissions)>, // BigOrderedMap
        is_active: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubaccountSeed {
    pub owner_addr: [u8; 32],
    pub seed: Vec<u8>,
}

// Events

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SubaccountCreatedEvent {
    V1 {
        subaccount: [u8; 32],
        owner: [u8; 32],
        is_primary: bool,
        seed: Option<Vec<u8>>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DelegationChangedEvent {
    V1 {
        subaccount: [u8; 32],
        delegated_account: [u8; 32],
        delegation: Option<PermissionType>,
        expiration_time_s: Option<u64>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SubaccountActiveChangedEvent {
    V1 {
        subaccount: [u8; 32],
        owner: [u8; 32],
        is_active: bool,
    },
}

// Permission Types

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum PermissionType {
    TradePerpsAllMarkets,
    TradePerpsOnMarket { market: [u8; 32] },
    SubaccountFundsMovement,
    SubDelegate,
    TradeVaultTokens,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum StoredPermission {
    Unlimited,
    UnlimitedUntil(u64),
    UnlimitedVia { authorized_by: [u8; 32] },
    UnlimitedUntilVia { authorized_by: [u8; 32], expiration_time_s: u64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DelegatedPermissions {
    V1 {
        perms: BTreeMap<u8, StoredPermission>, // PermissionType index -> StoredPermission
    },
}

/// RESOURCE: GlobalDexAccountsConfig at @decibel_dex
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GlobalDexAccountsConfig {
    V1 {
        subaccount_manager_extend_ref: Vec<u8>, // ExtendRef opaque
        // restricted_perp_api is a stored capability, opaque in native
    },
}

// ===================== Helper Functions =====================

/// Encode a PermissionType to its BTreeMap key (discriminant index).
fn permission_type_to_key(perm: &PermissionType) -> u8 {
    match perm {
        PermissionType::TradePerpsAllMarkets => 0,
        PermissionType::TradePerpsOnMarket { .. } => 1,
        PermissionType::SubaccountFundsMovement => 2,
        PermissionType::SubDelegate => 3,
        PermissionType::TradeVaultTokens => 4,
    }
}

/// Compute a deterministic address from a creator address and seed.
/// In native context, this emulates `object::create_object_address`.
/// The actual derivation is: sha3_256(creator || seed || 0xFE)
fn create_object_address(creator: &[u8; 32], seed: &[u8]) -> [u8; 32] {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // In the real framework this is sha3_256(creator || seed || 0xFE).
    // We replicate the deterministic derivation here.
    let mut hasher = DefaultHasher::new();
    creator.hash(&mut hasher);
    seed.hash(&mut hasher);
    0xFEu8.hash(&mut hasher);
    let hash = hasher.finish();
    let mut result = [0u8; 32];
    result[..8].copy_from_slice(&hash.to_le_bytes());
    // Mix in more bytes for uniqueness
    let mut hasher2 = DefaultHasher::new();
    hash.hash(&mut hasher2);
    seed.hash(&mut hasher2);
    let hash2 = hasher2.finish();
    result[8..16].copy_from_slice(&hash2.to_le_bytes());
    result
}

/// Get the address of the global subaccount manager named object.
/// In Move: `object::create_object_address(&@decibel_dex, SUBACCOUNT_MANAGER_SEED)`
fn global_subaccount_manager_address() -> [u8; 32] {
    // @decibel_dex is the deployer address; in native context we use a sentinel.
    // The dispatch layer provides the actual @decibel_dex address.
    // For deterministic derivation, we use a fixed deployer placeholder.
    let decibel_dex_addr = [0u8; 32]; // RESOURCE_READ: @decibel_dex address from dispatch layer
    create_object_address(&decibel_dex_addr, SUBACCOUNT_MANAGER_SEED)
}

/// BCS-encode a SubaccountSeed for object address derivation.
/// In Move: `bcs::to_bytes(&SubaccountSeed { owner_addr, seed })`
fn bcs_encode_subaccount_seed(owner_addr: [u8; 32], seed: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::new();
    encoded.extend_from_slice(&owner_addr);
    // BCS encodes vector<u8> as ULEB128 length prefix + raw bytes
    let len = seed.len();
    // Simple ULEB128 encoding for lengths < 128
    if len < 128 {
        encoded.push(len as u8);
    } else {
        let mut val = len;
        while val >= 0x80 {
            encoded.push((val as u8) | 0x80);
            val >>= 7;
        }
        encoded.push(val as u8);
    }
    encoded.extend_from_slice(seed);
    encoded
}

/// Compute the deterministic address for a subaccount with a given seed.
fn compute_subaccount_address(owner_addr: [u8; 32], seed: &[u8]) -> [u8; 32] {
    let manager_addr = global_subaccount_manager_address();
    let full_seed = bcs_encode_subaccount_seed(owner_addr, seed);
    create_object_address(&manager_addr, &full_seed)
}

/// Check if a stored permission is valid at the given check time.
fn is_stored_permission_valid_at(permission: &StoredPermission, check_time: u64) -> bool {
    match permission {
        StoredPermission::Unlimited => true,
        StoredPermission::UnlimitedUntil(expiration) => check_time < *expiration,
        StoredPermission::UnlimitedVia { .. } => true, // validity depends on the authorizer chain
        StoredPermission::UnlimitedUntilVia { expiration_time_s, .. } => check_time < *expiration_time_s,
    }
}

/// Look up the Subaccount resource at a given address.
/// RESOURCE_READ: Subaccount at subaccount_addr
fn get_subaccount_resource(subaccount_addr: [u8; 32]) -> Option<Subaccount> {
    // In native context, the dispatch layer provides access to stored resources.
    // This function reads the Subaccount resource at the given address.
    // RESOURCE_READ: Subaccount at subaccount_addr
    // Returns None if the resource doesn't exist.
    None // Dispatch layer fills this in at runtime
}

/// Look up the owner of a subaccount object.
/// RESOURCE_READ: ObjectCore at subaccount_addr
fn get_subaccount_owner(subaccount_addr: [u8; 32]) -> [u8; 32] {
    // In native context, the dispatch layer reads the ObjectCore resource to get the owner.
    // RESOURCE_READ: ObjectCore.owner at subaccount_addr
    [0u8; 32] // Dispatch layer fills this in at runtime
}

/// Check if the given address is the owner of the subaccount.
fn is_subaccount_owner(owner_addr: [u8; 32], subaccount_addr: [u8; 32]) -> bool {
    // RESOURCE_READ: ObjectCore.owner at subaccount_addr
    get_subaccount_owner(subaccount_addr) == owner_addr
}

/// Check if any of the given permissions is granted to auth_addr on the subaccount.
/// Recursively checks delegation chains up to max_depth levels.
fn is_any_permission_granted(
    auth_addr: [u8; 32],
    subaccount_addr: [u8; 32],
    subaccount_resource: &Subaccount,
    permissions: &[PermissionType],
    doesnt_expire_before_s: u64,
    max_depth: u64,
) -> bool {
    if max_depth == 0 {
        return false;
    }
    // RESOURCE_READ: decibel_time::now_seconds(state) - dispatch layer provides current time
    let now: u64 = 0; // Dispatch layer provides current timestamp in seconds
    let check_time = if now > doesnt_expire_before_s { now } else { doesnt_expire_before_s };

    let delegated_permissions = match subaccount_resource {
        Subaccount::V1 { delegated_permissions, .. } => delegated_permissions,
    };

    // Find the DelegatedPermissions entry for auth_addr
    let entry = delegated_permissions.iter().find(|(addr, _)| *addr == auth_addr);
    let entry = match entry {
        Some((_, dp)) => dp,
        None => return false,
    };

    let perms = match entry {
        DelegatedPermissions::V1 { perms } => perms,
    };

    for permission in permissions {
        let key = permission_type_to_key(permission);
        if let Some(stored_perm) = perms.get(&key) {
            match stored_perm {
                StoredPermission::Unlimited => return true,
                StoredPermission::UnlimitedUntil(expiration_time_s) => {
                    if *expiration_time_s > check_time {
                        return true;
                    }
                },
                StoredPermission::UnlimitedVia { authorized_by } => {
                    if is_owner_or_any_permission_granted(
                        *authorized_by, subaccount_addr, subaccount_resource,
                        permissions, doesnt_expire_before_s, max_depth - 1,
                    ) {
                        return true;
                    }
                },
                StoredPermission::UnlimitedUntilVia { authorized_by, expiration_time_s } => {
                    if *expiration_time_s > check_time
                        && is_owner_or_any_permission_granted(
                            *authorized_by, subaccount_addr, subaccount_resource,
                            permissions, doesnt_expire_before_s, max_depth - 1,
                        )
                    {
                        return true;
                    }
                },
            }
        }
    }
    false
}

/// Check if auth_addr is either the owner or has any of the given permissions.
fn is_owner_or_any_permission_granted(
    auth_addr: [u8; 32],
    subaccount_addr: [u8; 32],
    subaccount_resource: &Subaccount,
    permissions: &[PermissionType],
    doesnt_expire_before_s: u64,
    max_depth: u64,
) -> bool {
    is_subaccount_owner(auth_addr, subaccount_addr)
        || is_any_permission_granted(
            auth_addr, subaccount_addr, subaccount_resource,
            permissions, doesnt_expire_before_s, max_depth,
        )
}

/// Assert that the subaccount is active; otherwise return error.
fn assert_active(subaccount_resource: &Subaccount) -> Result<(), u64> {
    let is_active = match subaccount_resource {
        Subaccount::V1 { is_active, .. } => *is_active,
    };
    if !is_active {
        Err(ESUBACCOUNT_IS_NOT_ACTIVE)
    } else {
        Ok(())
    }
}

/// Assert the auth is owner and the subaccount is active, then return the signer address
/// (which is the subaccount address itself, derived from the ExtendRef).
fn assert_owner_and_active_then_get_signer(
    owner: [u8; 32],
    subaccount_addr: [u8; 32],
) -> Result<[u8; 32], u64> {
    if !is_subaccount_owner(owner, subaccount_addr) {
        return Err(ENOT_SUBACCOUNT_OWNER);
    }
    // RESOURCE_READ: Subaccount at subaccount_addr
    let subaccount_resource = get_subaccount_resource(subaccount_addr);
    let subaccount_resource = subaccount_resource.ok_or(ESUBACCOUNT_DOESNT_EXIST)?;
    assert_active(&subaccount_resource)?;
    // The signer for the subaccount is derived from its ExtendRef;
    // in native context the subaccount address itself serves as the signer identity.
    Ok(subaccount_addr)
}

/// Assert auth has the given permissions (or is owner) and is active.
fn assert_has_permission_and_get_signer(
    auth: [u8; 32],
    subaccount_addr: [u8; 32],
    permissions: &[PermissionType],
    error_code: u64,
) -> Result<[u8; 32], u64> {
    // RESOURCE_READ: Subaccount at subaccount_addr
    let subaccount_resource = get_subaccount_resource(subaccount_addr);
    let subaccount_resource = subaccount_resource.ok_or(ESUBACCOUNT_DOESNT_EXIST)?;
    assert_active(&subaccount_resource)?;

    if !is_owner_or_any_permission_granted(
        auth, subaccount_addr, &subaccount_resource,
        permissions, 0, MAX_DELEGATION_DEPTH,
    ) {
        return Err(error_code);
    }
    // The signer identity is the subaccount address itself.
    Ok(subaccount_addr)
}

/// Assert that auth has sub-delegation permissions for the given permission type.
/// This is used when delegating permissions via a sub-delegator.
fn assert_sub_delegation_permissions(
    auth: [u8; 32],
    subaccount_addr: [u8; 32],
    permission: PermissionType,
    doesnt_expire_before_s: u64,
) -> Result<(), u64> {
    // RESOURCE_READ: Subaccount at subaccount_addr
    let subaccount_resource = get_subaccount_resource(subaccount_addr);
    let subaccount_resource = subaccount_resource.ok_or(ESUBACCOUNT_DOESNT_EXIST)?;
    assert_active(&subaccount_resource)?;

    // Build the required permission set (for TradePerpsOnMarket, also accept TradePerpsAllMarkets)
    let required_permissions = match &permission {
        PermissionType::TradePerpsOnMarket { .. } => {
            vec![permission, PermissionType::TradePerpsAllMarkets]
        },
        _ => vec![permission],
    };

    let is_authorized = is_subaccount_owner(auth, subaccount_addr)
        || (
            // Cannot sub-delegate the SubDelegate permission itself
            permission != PermissionType::SubDelegate
            // Must currently have SubDelegate permission
            && is_any_permission_granted(
                auth, subaccount_addr, &subaccount_resource,
                &[PermissionType::SubDelegate], 0, MAX_DELEGATION_DEPTH,
            )
            // Must have the requested permission until the expiration time
            && is_any_permission_granted(
                auth, subaccount_addr, &subaccount_resource,
                &required_permissions, doesnt_expire_before_s, MAX_DELEGATION_DEPTH,
            )
        );

    if !is_authorized {
        return Err(ENOT_SUBACCOUNT_OWNER_OR_LACKS_SUB_DELEGATION_PERMISSIONS);
    }
    Ok(())
}

/// Add a delegated permission to the subaccount's permission table.
/// If a permission already exists and the new one doesn't extend the expiration, skip update.
fn add_delegated_permission(
    auth: [u8; 32],
    subaccount_addr: [u8; 32],
    account_to_delegate_to: [u8; 32],
    permission: PermissionType,
    expiration_time_s: Option<u64>,
) {
    // RESOURCE_WRITE: Subaccount.delegated_permissions at subaccount_addr
    // Build the stored permission
    let stored_permission = match expiration_time_s {
        None => StoredPermission::UnlimitedVia { authorized_by: auth },
        Some(exp) => StoredPermission::UnlimitedUntilVia { authorized_by: auth, expiration_time_s: exp },
    };

    let key = permission_type_to_key(&permission);

    // In native context, the dispatch layer handles the actual resource mutation.
    // The logic mirrors the Move code:
    // 1. If delegated_permissions doesn't contain account_to_delegate_to, create entry
    // 2. Check existing permission - only overwrite if new one extends expiration
    // 3. Emit DelegationChangedEvent if updated

    // RESOURCE_WRITE: Subaccount.delegated_permissions[account_to_delegate_to].perms[key] = stored_permission
    // EVENT: DelegationChangedEvent::V1 { subaccount: subaccount_addr, delegated_account: account_to_delegate_to, delegation: Some(permission), expiration_time_s }
}

/// Internal: check_and_delegate_permission - validates sub-delegation then adds.
fn check_and_delegate_permission(
    auth: [u8; 32],
    subaccount_addr: [u8; 32],
    account_to_delegate_to: [u8; 32],
    permission: PermissionType,
    expiration_time_s: Option<u64>,
) -> Result<(), u64> {
    // First ensure the subaccount exists (create primary if needed)
    let subaccount_obj = get_subaccount_object_unpermissioned(
        subaccount_addr, Some(auth),
    )?;

    // Check that auth has sub-delegation permissions
    let doesnt_expire_before_s = expiration_time_s.unwrap_or(u64::MAX);
    assert_sub_delegation_permissions(auth, subaccount_obj, permission, doesnt_expire_before_s)?;

    // Add the permission
    add_delegated_permission(auth, subaccount_obj, account_to_delegate_to, permission, expiration_time_s);
    Ok(())
}

// ===================== Public Functions =====================

/// Initialize global config (called at module init).
/// RESOURCE: GlobalDexAccountsConfig at @decibel_dex
pub fn init_global_config(publisher: [u8; 32]) -> Result<(), u64> {
    // RESOURCE_READ: Check publisher == @decibel_dex
    // In native context, the dispatch layer verifies the publisher address.
    // RESOURCE_WRITE: Create named object with SUBACCOUNT_MANAGER_SEED
    // RESOURCE_WRITE: Move GlobalDexAccountsConfig to publisher
    // The restricted_perp_api is obtained via perp_engine_api::get_restricted_perp_api
    Ok(())
}

/// Create a primary subaccount object for a user.
/// RESOURCE: Subaccount at computed address
pub fn create_primary_subaccount_object(
    user_addr: [u8; 32],
) -> [u8; 32] {
    // Compute the deterministic address for the primary subaccount
    let subaccount_addr = compute_subaccount_address(user_addr, PRIMARY_SUBACCOUNT_SEED);
    let is_primary = true;

    // FRAMEWORK: create_named_object under global_subaccount_manager with full_seed
    // FRAMEWORK: transfer object to user_addr
    // FRAMEWORK: disable_ungated_transfer
    // RESOURCE_WRITE: Subaccount::V1 { extend_ref, delegated_permissions: empty, is_active: true }
    // RESOURCE_WRITE: init_user_if_new via restricted_perp_api

    // EVENT: SubaccountCreatedEvent::V1 { subaccount: subaccount_addr, owner: user_addr, is_primary: true, seed: None }
    subaccount_addr
}

/// Create a new (secondary) subaccount.
/// RESOURCE: Subaccount at new object address
pub fn create_new_subaccount_object(
    owner: [u8; 32], // signer
) -> [u8; 32] {
    // FRAMEWORK: create_object(owner) - non-deterministic address
    // FRAMEWORK: disable_ungated_transfer
    // RESOURCE_WRITE: Subaccount::V1 { extend_ref, delegated_permissions: empty, is_active: true }
    // RESOURCE_WRITE: init_user_if_new via restricted_perp_api

    // EVENT: SubaccountCreatedEvent::V1 { subaccount: addr, owner, is_primary: false, seed: None }

    // In native context, the dispatch layer creates the object and returns the address.
    // FRAMEWORK_CALL: object::create_object(owner) -> object_addr
    [0u8; 32] // Dispatch layer provides actual address
}

/// Create a new seeded non-primary subaccount.
/// RESOURCE: Subaccount at computed address
pub fn create_new_seeded_subaccount(
    owner: [u8; 32], // signer
    seed: Vec<u8>,
) -> Result<[u8; 32], u64> {
    // Validate seed is not the primary subaccount seed
    if seed == PRIMARY_SUBACCOUNT_SEED {
        return Err(ESEED_CANNOT_BE_PRIMARY_SUBACCOUNT_SEED);
    }

    let subaccount_addr = compute_subaccount_address(owner, &seed);

    // FRAMEWORK: create_named_object under global_subaccount_manager with bcs(SubaccountSeed{owner, seed})
    // FRAMEWORK: transfer object to owner
    // FRAMEWORK: disable_ungated_transfer
    // RESOURCE_WRITE: Subaccount::V1 { extend_ref, delegated_permissions: empty, is_active: true }
    // RESOURCE_WRITE: init_user_if_new via restricted_perp_api

    // EVENT: SubaccountCreatedEvent::V1 { subaccount: subaccount_addr, owner, is_primary: false, seed: Some(seed) }
    Ok(subaccount_addr)
}

/// Admin-create a primary subaccount (idempotent, bypasses invite-only).
pub fn admin_create_new_primary_subaccount(
    admin: [u8; 32], // signer
    user_addr: [u8; 32],
) -> [u8; 32] {
    // Always enforce admin permission via add_to_account_creation_allow_list,
    // even on early-return paths.
    crate::native_perpdex::admin_apis::add_to_account_creation_allow_list(admin, vec![user_addr])
        .expect("admin permission check failed for add_to_account_creation_allow_list");

    // Idempotent: if user already has a primary subaccount, return it
    let primary_addr = primary_subaccount(user_addr);
    if subaccount_exists(primary_addr) {
        return primary_addr;
    }

    create_primary_subaccount_object(user_addr)
}

/// Delegate all trading permissions to another account for a subaccount.
/// RESOURCE: Subaccount at subaccount
pub fn delegate_all_trading_to_for_subaccount(
    auth: [u8; 32], // signer
    subaccount: [u8; 32],
    account_to_delegate_to: [u8; 32],
    expiration_time_s: Option<u64>,
) -> Result<(), u64> {
    // Delegate TradePerpsAllMarkets
    check_and_delegate_permission(
        auth, subaccount, account_to_delegate_to,
        PermissionType::TradePerpsAllMarkets, expiration_time_s,
    )?;
    // Delegate TradeVaultTokens
    check_and_delegate_permission(
        auth, subaccount, account_to_delegate_to,
        PermissionType::TradeVaultTokens, expiration_time_s,
    )?;
    // EVENT: DelegationChangedEvent for each permission
    Ok(())
}

/// Delegate perp trading permissions.
/// RESOURCE: Subaccount at subaccount
pub fn delegate_perp_trading_to_for_subaccount(
    auth: [u8; 32], // signer
    subaccount: [u8; 32],
    account_to_delegate_to: [u8; 32],
    expiration_time_s: Option<u64>,
) -> Result<(), u64> {
    check_and_delegate_permission(
        auth, subaccount, account_to_delegate_to,
        PermissionType::TradePerpsAllMarkets, expiration_time_s,
    )
}

/// Delegate perp trading permissions for a specific market.
/// RESOURCE: Subaccount at subaccount
pub fn delegate_perp_trading_to_market_for_subaccount(
    auth: [u8; 32], // signer
    subaccount: [u8; 32],
    account_to_delegate_to: [u8; 32],
    market: [u8; 32],
    expiration_time_s: Option<u64>,
) -> Result<(), u64> {
    check_and_delegate_permission(
        auth, subaccount, account_to_delegate_to,
        PermissionType::TradePerpsOnMarket { market }, expiration_time_s,
    )
}

/// Delegate sub-delegation permissions.
/// RESOURCE: Subaccount at subaccount
pub fn delegate_ability_to_sub_delegate_to_for_subaccount(
    auth: [u8; 32], // signer
    subaccount: [u8; 32],
    account_to_delegate_to: [u8; 32],
    expiration_time_s: Option<u64>,
) -> Result<(), u64> {
    check_and_delegate_permission(
        auth, subaccount, account_to_delegate_to,
        PermissionType::SubDelegate, expiration_time_s,
    )
}

/// Delegate onchain account permissions (via ExtendRef).
/// RESOURCE: Subaccount at subaccount
pub fn delegate_onchain_account_permissions(
    auth_ref: Vec<u8>, // ExtendRef opaque
    subaccount: [u8; 32],
    account_to_delegate_to: [u8; 32],
    delegate_perp_trading: bool,
    delegate_vault_trading: bool,
    delegate_sub_delegation: bool,
    delegate_subaccount_funds_movement: bool,
    expiration_time_s: Option<u64>,
) -> Result<(), u64> {
    // FRAMEWORK: generate_signer_for_extending from auth_ref -> auth_addr
    // In native context, the dispatch layer extracts the address from the ExtendRef.
    let auth_addr = [0u8; 32]; // FRAMEWORK_CALL: ExtendRef.generate_signer_for_extending() -> address

    // Get the subaccount object (create primary if needed)
    let subaccount_obj = get_subaccount_object_unpermissioned(subaccount, Some(auth_addr))?;

    // Assert owner and active
    if !is_subaccount_owner(auth_addr, subaccount_obj) {
        return Err(ENOT_SUBACCOUNT_OWNER);
    }
    // RESOURCE_READ: Subaccount at subaccount_obj
    let subaccount_resource = get_subaccount_resource(subaccount_obj);
    let subaccount_resource = subaccount_resource.ok_or(ESUBACCOUNT_DOESNT_EXIST)?;
    assert_active(&subaccount_resource)?;

    if delegate_perp_trading {
        add_delegated_permission(
            auth_addr, subaccount_obj, account_to_delegate_to,
            PermissionType::TradePerpsAllMarkets, expiration_time_s,
        );
    }
    if delegate_vault_trading {
        add_delegated_permission(
            auth_addr, subaccount_obj, account_to_delegate_to,
            PermissionType::TradeVaultTokens, expiration_time_s,
        );
    }
    if delegate_sub_delegation {
        add_delegated_permission(
            auth_addr, subaccount_obj, account_to_delegate_to,
            PermissionType::SubDelegate, expiration_time_s,
        );
    }
    if delegate_subaccount_funds_movement {
        add_delegated_permission(
            auth_addr, subaccount_obj, account_to_delegate_to,
            PermissionType::SubaccountFundsMovement, expiration_time_s,
        );
    }
    Ok(())
}

/// Revoke trading permissions from a delegated account.
/// RESOURCE: Subaccount at subaccount
pub fn revoke_delegation(
    owner: [u8; 32], // signer
    subaccount: [u8; 32],
    account_to_revoke: [u8; 32],
) -> Result<(), u64> {
    // Assert owner and active
    if !is_subaccount_owner(owner, subaccount) {
        return Err(ENOT_SUBACCOUNT_OWNER);
    }
    let subaccount_resource = get_subaccount_resource(subaccount);
    let subaccount_resource = subaccount_resource.ok_or(ESUBACCOUNT_DOESNT_EXIST)?;
    assert_active(&subaccount_resource)?;

    // RESOURCE_WRITE: Remove account_to_revoke from Subaccount.delegated_permissions
    // EVENT: DelegationChangedEvent::V1 { subaccount, delegated_account: account_to_revoke, delegation: None, expiration_time_s: None }
    Ok(())
}

/// Revoke all delegated trading permissions.
/// RESOURCE: Subaccount at subaccount
pub fn revoke_all_delegations(
    owner: [u8; 32], // signer
    subaccount: [u8; 32],
) -> Result<(), u64> {
    if !is_subaccount_owner(owner, subaccount) {
        return Err(ENOT_SUBACCOUNT_OWNER);
    }
    let subaccount_resource = get_subaccount_resource(subaccount);
    let subaccount_resource = subaccount_resource.ok_or(ESUBACCOUNT_DOESNT_EXIST)?;
    assert_active(&subaccount_resource)?;

    // RESOURCE_WRITE: Clear all entries from Subaccount.delegated_permissions
    // For each removed entry, emit:
    // EVENT: DelegationChangedEvent::V1 { subaccount, delegated_account, delegation: None, expiration_time_s: None }
    let delegated_permissions = match &subaccount_resource {
        Subaccount::V1 { delegated_permissions, .. } => delegated_permissions,
    };
    for (account_to_revoke, _) in delegated_permissions {
        // EVENT: DelegationChangedEvent::V1 for each revoked account
        let _ = account_to_revoke; // Used by event emission in dispatch layer
    }
    // RESOURCE_WRITE: Subaccount.delegated_permissions = empty
    Ok(())
}

/// Deactivate a subaccount.
/// RESOURCE: Subaccount at subaccount
pub fn deactivate_subaccount(
    owner: [u8; 32], // signer
    subaccount: [u8; 32],
    revoke_delegations: bool,
) -> Result<(), u64> {
    if !is_subaccount_owner(owner, subaccount) {
        return Err(ENOT_SUBACCOUNT_OWNER);
    }
    let subaccount_resource = get_subaccount_resource(subaccount);
    let subaccount_resource = subaccount_resource.ok_or(ESUBACCOUNT_DOESNT_EXIST)?;
    assert_active(&subaccount_resource)?;

    // Check that the subaccount has no assets or positions
    if crate::native_perpdex::public_read_api::account_has_any_perp_assets_positions_or_orders(subaccount) {
        return Err(ESUBACCOUNT_HAS_ASSETS_OR_POSITIONS);
    }

    if revoke_delegations {
        revoke_all_delegations(owner, subaccount)?;
    }

    // RESOURCE_WRITE: Subaccount.is_active = false
    // EVENT: SubaccountActiveChangedEvent::V1 { subaccount, owner, is_active: false }
    Ok(())
}

/// Reactivate a subaccount.
/// RESOURCE: Subaccount at subaccount
pub fn reactivate_subaccount(
    owner: [u8; 32], // signer
    subaccount: [u8; 32],
) -> Result<(), u64> {
    // assert_subaccount_owner_maybe_active: only checks owner, not active status
    if !is_subaccount_owner(owner, subaccount) {
        return Err(ENOT_SUBACCOUNT_OWNER);
    }

    // RESOURCE_WRITE: Subaccount.is_active = true
    // EVENT: SubaccountActiveChangedEvent::V1 { subaccount, owner, is_active: true }
    Ok(())
}

/// Deposit funds to a subaccount at a specific address.
pub fn deposit_to_subaccount_at(
    owner: [u8; 32], // signer
    subaccount: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    // FRAMEWORK: primary_fungible_store::withdraw(owner, metadata, amount) -> FungibleAsset
    // Then call deposit_funds_to_subaccount with the withdrawn funds
    deposit_funds_to_subaccount(subaccount, metadata, amount, Some(owner))
}

/// Deposit funds to a subaccount (unpermissioned).
pub fn deposit_funds_to_subaccount(
    subaccount: [u8; 32],
    funds_metadata: [u8; 32],
    funds_amount: u64,
    init_primary_if_not_exists_for_owner: Option<[u8; 32]>,
) -> Result<(), u64> {
    // Get the subaccount object, creating primary if needed
    let subaccount_obj = get_subaccount_object_unpermissioned(
        subaccount, init_primary_if_not_exists_for_owner,
    )?;

    // Assert subaccount is active
    assert_subaccount_is_active(subaccount_obj)?;

    // If init_primary_if_not_exists_for_owner is Some, verify ownership
    if let Some(expected_owner) = init_primary_if_not_exists_for_owner {
        if !is_subaccount_owner(expected_owner, subaccount_obj) {
            return Err(ENOT_SUBACCOUNT_OWNER);
        }
    }

    // Deposit to cross collateral
    // DELEGATE: account_management_apis::deposit_to_cross(subaccount, funds_metadata, funds_amount)
    crate::native_perpdex::account_management_apis::deposit_to_cross(
        subaccount, funds_metadata, funds_amount,
    )
}

/// Place a perp order for a subaccount.
/// RESOURCE: Subaccount, PerpMarket, AsyncMatchingEngine
pub fn place_perp_order_to_subaccount(
    auth: [u8; 32], // signer
    subaccount: [u8; 32],
    market: [u8; 32],
    order_request: perp_order::PerpOrderRequestCommonArgs,
    is_reduce_only: bool,
    stop_price: Option<u64>,
    tpsl_order_request: perp_order::PerpOrderRequestTpSlArgs,
    builder_code: Option<builder_code_registry::BuilderCode>,
) -> Result<order_book_types::OrderId, u64> {
    let subaccount_signer = get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    // Delegate to order_apis::place_order using the subaccount signer
    crate::native_perpdex::order_apis::place_order(
        market, subaccount_signer, order_request,
        is_reduce_only, stop_price, tpsl_order_request, builder_code,
    )
}

/// Cancel a perp order for a subaccount.
/// RESOURCE: Subaccount, PerpMarket
pub fn cancel_perp_order_to_subaccount(
    auth: [u8; 32], // signer
    subaccount: [u8; 32],
    order_id: u128,
    market: [u8; 32],
) -> Result<(), u64> {
    let subaccount_signer = get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
        auth, subaccount, market,
    )?;
    crate::native_perpdex::order_apis::cancel_order(
        market, subaccount_signer,
        order_book_types::OrderId { order_id },
    )
}

/// Withdraw from subaccount to owner's primary store.
/// First tries non-collateral, then cross collateral.
pub fn withdraw_from_subaccount(
    owner: [u8; 32], // signer
    subaccount: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Option<u128> {
    // Get non-collateral balance at subaccount
    // FRAMEWORK_CALL: primary_fungible_store::balance(subaccount, metadata) -> non_collateral_balance
    let non_collateral_balance: u64 = 0; // FRAMEWORK_READ: primary_fungible_store::balance

    let left_to_withdraw = if non_collateral_balance > 0 {
        if non_collateral_balance >= amount {
            withdraw_from_non_collateral(owner, subaccount, metadata, amount);
            return None; // Immediate withdrawal
        } else {
            withdraw_from_non_collateral(owner, subaccount, metadata, non_collateral_balance);
            amount - non_collateral_balance
        }
    } else {
        amount
    };

    withdraw_from_cross_collateral(owner, subaccount, metadata, left_to_withdraw)
}

/// Withdraw from cross collateral.
pub fn withdraw_from_cross_collateral(
    owner: [u8; 32], // signer
    subaccount: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Option<u128> {
    // Get subaccount signer (owner check + active check)
    let subaccount_signer = match assert_owner_and_active_then_get_signer(owner, subaccount) {
        Ok(signer) => signer,
        Err(_) => return None,
    };

    // DELEGATE: account_management_apis::request_withdrawal_from_cross(subaccount_signer, metadata, amount, owner)
    match crate::native_perpdex::account_management_apis::request_withdrawal_from_cross(
        subaccount_signer, metadata, amount, owner,
    ) {
        Ok(request_id) => request_id,
        Err(_) => None,
    }
}

/// Withdraw from non-collateral primary store.
pub fn withdraw_from_non_collateral(
    owner: [u8; 32], // signer
    subaccount: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) {
    // Get subaccount signer (owner check + active check)
    let _subaccount_signer = match assert_owner_and_active_then_get_signer(owner, subaccount) {
        Ok(signer) => signer,
        Err(_) => return,
    };
    // FRAMEWORK: primary_fungible_store::transfer(subaccount_signer, metadata, owner, amount)
}

/// Transfer from subaccount (onchain account, via ExtendRef).
pub fn transfer_onchain_account_funds_from_subaccount(
    owner_ref: Vec<u8>, // ExtendRef opaque
    subaccount: [u8; 32],
    to_account: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    // FRAMEWORK: generate_signer_for_extending from owner_ref -> owner_addr
    let owner_addr = [0u8; 32]; // FRAMEWORK_CALL: ExtendRef -> address

    let subaccount_signer = assert_owner_and_active_then_get_signer(owner_addr, subaccount)?;

    // DELEGATE: account_management_apis::transfer_collateral(subaccount_signer, to_account, metadata, amount)
    crate::native_perpdex::account_management_apis::transfer_collateral(
        subaccount_signer, to_account, metadata, amount,
    )
}

/// Request withdrawal from onchain account's subaccount.
pub fn request_withdrawal_onchain_account_from_subaccount(
    owner_ref: Vec<u8>, // ExtendRef opaque
    subaccount: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
    recipient: [u8; 32],
) -> Option<u128> {
    // FRAMEWORK: generate_signer_for_extending from owner_ref -> owner_addr
    let owner_addr = [0u8; 32]; // FRAMEWORK_CALL: ExtendRef -> address

    let subaccount_signer = match assert_owner_and_active_then_get_signer(owner_addr, subaccount) {
        Ok(signer) => signer,
        Err(_) => return None,
    };

    // DELEGATE: account_management_apis::request_withdrawal_from_cross(subaccount_signer, metadata, amount, recipient)
    match crate::native_perpdex::account_management_apis::request_withdrawal_from_cross(
        subaccount_signer, metadata, amount, recipient,
    ) {
        Ok(request_id) => request_id,
        Err(_) => None,
    }
}

/// Transfer collateral between subaccounts of the same owner.
pub fn transfer_collateral_between_subaccounts(
    owner: [u8; 32], // signer
    from: [u8; 32],
    to: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    // Verify both subaccounts have the same owner
    let from_owner = get_subaccount_owner(from);
    let to_owner = get_subaccount_owner(to);
    if from_owner != to_owner {
        return Err(ECANNOT_TRANSFER_BETWEEN_DIFFERENT_OWNERS);
    }

    // Get signer with funds movement permission check
    let from_signer = assert_has_permission_and_get_signer(
        owner, from,
        &[PermissionType::SubaccountFundsMovement],
        ENOT_SUBACCOUNT_OWNER_OR_LACKS_FUNDS_MOVEMENT_PERMISSIONS,
    )?;

    // DELEGATE: account_management_apis::transfer_collateral(from_signer, to, metadata, amount)
    crate::native_perpdex::account_management_apis::transfer_collateral(
        from_signer, to, metadata, amount,
    )
}

/// Transfer fee to treasury from a subaccount.
pub fn transfer_fee_to_treasury_from_subaccount(
    owner: [u8; 32], // signer
    from: [u8; 32],
    metadata: [u8; 32],
    amount: u64,
) -> Result<(), u64> {
    let from_signer = assert_owner_and_active_then_get_signer(owner, from)?;

    // DELEGATE: account_management_apis::transfer_fee_to_treasury(from_signer, metadata, amount)
    crate::native_perpdex::account_management_apis::transfer_fee_to_treasury(
        from_signer, metadata, amount,
    )
}

/// Set reserved collateral for a subaccount (vault config signer required).
pub fn set_reserved_collateral_for_subaccount_for_vault(
    vault_config_signer: [u8; 32], // signer
    owner: [u8; 32], // signer
    subaccount: [u8; 32],
    wanted_to_set_reserved: u64,
) -> u64 {
    // Assert vault config signer authority
    if let Err(_) = crate::native_perpdex::admin_apis::assert_vault_global_config_signer(vault_config_signer) {
        return 0;
    }

    let subaccount_signer = match assert_owner_and_active_then_get_signer(owner, subaccount) {
        Ok(signer) => signer,
        Err(_) => return 0,
    };

    // DELEGATE: account_management_apis::set_reserved_collateral(subaccount_signer, wanted_to_set_reserved)
    crate::native_perpdex::account_management_apis::set_reserved_collateral(
        subaccount_signer, wanted_to_set_reserved,
    )
}

// ========== View Functions ==========

/// Get the deterministic address for a user's primary subaccount.
pub fn primary_subaccount(owner_addr: [u8; 32]) -> [u8; 32] {
    // Computed from GlobalSubaccountManager named object + BCS(SubaccountSeed)
    compute_subaccount_address(owner_addr, PRIMARY_SUBACCOUNT_SEED)
}

/// Get the primary subaccount as an object address.
pub fn primary_subaccount_object(owner: [u8; 32]) -> [u8; 32] {
    primary_subaccount(owner)
}

/// Public wrapper for primary_subaccount.
pub fn primary_subaccount_public(owner_addr: [u8; 32]) -> [u8; 32] {
    primary_subaccount(owner_addr)
}

/// Public wrapper for primary_subaccount_object.
pub fn primary_subaccount_object_public(owner: [u8; 32]) -> [u8; 32] {
    primary_subaccount_object(owner)
}

/// Get seeded subaccount address.
pub fn seeded_subaccount_address(
    owner_addr: [u8; 32],
    seed: Vec<u8>,
) -> [u8; 32] {
    compute_subaccount_address(owner_addr, &seed)
}

/// View delegated permissions for a subaccount.
/// RESOURCE: Subaccount at subaccount
pub fn view_delegated_permissions(
    subaccount: [u8; 32],
) -> Vec<([u8; 32], DelegatedPermissions)> {
    // RESOURCE_READ: Subaccount at subaccount
    let subaccount_resource = get_subaccount_resource(subaccount);
    match subaccount_resource {
        Some(Subaccount::V1 { delegated_permissions, .. }) => delegated_permissions,
        None => Vec::new(),
    }
}

/// View if subaccount is active.
/// RESOURCE: Subaccount at subaccount
pub fn view_is_subaccount_active(subaccount: [u8; 32]) -> bool {
    // RESOURCE_READ: Subaccount at subaccount
    let subaccount_resource = get_subaccount_resource(subaccount);
    match subaccount_resource {
        Some(Subaccount::V1 { is_active, .. }) => is_active,
        None => false,
    }
}

/// Check if a subaccount exists.
pub fn subaccount_exists(subaccount_addr: [u8; 32]) -> bool {
    // RESOURCE_READ: Check if Subaccount resource exists at subaccount_addr
    // In native context, this is checked by the dispatch layer.
    get_subaccount_resource(subaccount_addr).is_some()
}

/// Get subaccount signer if caller is the owner.
/// Returns the subaccount signer address.
/// RESOURCE: Subaccount at subaccount
pub fn get_subaccount_signer_if_owner(
    owner: [u8; 32], // signer
    subaccount: [u8; 32],
) -> Result<[u8; 32], u64> {
    assert_owner_and_active_then_get_signer(owner, subaccount)
}

/// Get subaccount signer if caller has perp trading permissions.
/// RESOURCE: Subaccount at subaccount
pub fn get_subaccount_signer_if_owner_or_delegated_for_perp_trading(
    auth: [u8; 32], // signer
    subaccount: [u8; 32],
    market: [u8; 32],
) -> Result<[u8; 32], u64> {
    assert_has_permission_and_get_signer(
        auth, subaccount,
        &[PermissionType::TradePerpsAllMarkets, PermissionType::TradePerpsOnMarket { market }],
        ENOT_SUBACCOUNT_OWNER_OR_LACKS_PERP_TRADING_PERMISSIONS,
    )
}

/// Get subaccount signer if caller has vault trading permissions.
/// RESOURCE: Subaccount at subaccount
pub fn get_subaccount_signer_if_owner_or_delegated_for_vault_trading(
    auth: [u8; 32], // signer
    subaccount: [u8; 32],
) -> Result<[u8; 32], u64> {
    assert_has_permission_and_get_signer(
        auth, subaccount,
        &[PermissionType::TradeVaultTokens],
        ENOT_SUBACCOUNT_OWNER_OR_LACKS_VAULT_TRADING_PERMISSIONS,
    )
}

/// Get subaccount object (unpermissioned, creates primary if needed).
/// RESOURCE: Subaccount at subaccount_addr
pub fn get_subaccount_object_unpermissioned(
    subaccount_addr: [u8; 32],
    init_primary_if_not_exists_for_owner: Option<[u8; 32]>,
) -> Result<[u8; 32], u64> {
    // Check if the subaccount already exists
    if subaccount_exists(subaccount_addr) {
        return Ok(subaccount_addr);
    }

    // If init_primary_if_not_exists_for_owner is provided and the subaccount_addr
    // matches the primary subaccount for that owner, create it
    if let Some(owner) = init_primary_if_not_exists_for_owner {
        let expected_primary = primary_subaccount(owner);
        if subaccount_addr == expected_primary {
            let created_addr = create_primary_subaccount_object(owner);
            return Ok(created_addr);
        }
    }

    Err(ESUBACCOUNT_DOESNT_EXIST)
}

/// Assert subaccount is active.
/// RESOURCE: Subaccount at subaccount
pub fn assert_subaccount_is_active(subaccount: [u8; 32]) -> Result<(), u64> {
    // RESOURCE_READ: Subaccount at subaccount
    let subaccount_resource = get_subaccount_resource(subaccount);
    let subaccount_resource = subaccount_resource.ok_or(ESUBACCOUNT_DOESNT_EXIST)?;
    assert_active(&subaccount_resource)
}
