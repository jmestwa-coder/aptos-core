// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::fee_treasury

use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const EINVALID_AMOUNT: u64 = 1;
const EINVALID_ADDRESS: u64 = 2;
const ENOT_IMPLEMENTED: u64 = 3;

// ===================== Types =====================

/// RESOURCE: FeeVault at @decibel_dex
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FeeVault {
    V1 {
        asset_type: [u8; 32], // Object<Metadata>
        store: [u8; 32],      // Object<FungibleStore>
        store_extend_ref_address: [u8; 32], // derived from ExtendRef
    },
}

// ===================== Functions =====================

/// Initialize a fee vault. In native context, the vault address is pre-computed.
/// RESOURCE: FeeVault written to admin address
pub fn initialize(
    admin_addr: [u8; 32],
    decibel_dex_addr: [u8; 32],
    asset_type: [u8; 32],
    fee_vault_addr: [u8; 32],
    fungible_store: [u8; 32],
) -> Result<FeeVault, u64> {
    if admin_addr != decibel_dex_addr {
        return Err(EINVALID_ADDRESS);
    }
    Ok(FeeVault::V1 {
        asset_type,
        store: fungible_store,
        store_extend_ref_address: fee_vault_addr,
    })
}

pub fn get_balance(_vault: &FeeVault) -> Result<u64, u64> {
    Err(ENOT_IMPLEMENTED)
}

pub fn get_fee_vault_address(vault: &FeeVault) -> [u8; 32] {
    match vault {
        FeeVault::V1 { store_extend_ref_address, .. } => *store_extend_ref_address,
    }
}

// ===================== Dispatch stubs (by-addr) for perp_engine delegation =====================

pub fn get_fee_vault_address_global() -> [u8; 32] {
    // Dispatch layer resolves FeeVault resource at @decibel_dex
    [0u8; 32]
}
