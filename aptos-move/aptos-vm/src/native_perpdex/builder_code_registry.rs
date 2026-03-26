// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::builder_code_registry

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===================== Constants =====================

const EINVALID_AMOUNT: u64 = 1;
const EBUILDER_NOT_REGISTERED: u64 = 2;
const EINVALID_ADDRESS: u64 = 3;
const EINVALID_MAX_FEE: u64 = 4;

const FEE_PRECISION: u64 = 10000; // 10000 => 1%

// ===================== Types =====================

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct BuilderCode {
    pub builder: [u8; 32],
    pub fees: u64, // fee in one thousand of a basis point 1 = 0.0001%
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct BuilderAndAccount {
    pub account: [u8; 32],
    pub builder: [u8; 32],
}

/// RESOURCE: Registry at @decibel_dex
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Registry {
    V1 {
        global_max_fee: u64,
        approved_max_fees: BTreeMap<BuilderAndAccount, u64>,
    },
}

// ===================== Functions =====================

pub fn initialize(
    admin_addr: [u8; 32],
    decibel_dex_addr: [u8; 32],
    global_max_fee: u64,
) -> Result<Registry, u64> {
    if admin_addr != decibel_dex_addr {
        return Err(EINVALID_ADDRESS);
    }
    Ok(Registry::V1 {
        global_max_fee,
        approved_max_fees: BTreeMap::new(),
    })
}

pub fn new_builder_code(
    registry: &Registry,
    builder: [u8; 32],
    fees: u64,
) -> Result<BuilderCode, u64> {
    let Registry::V1 { global_max_fee, .. } = registry;
    if fees == 0 {
        return Err(EINVALID_AMOUNT);
    }
    if fees > *global_max_fee {
        return Err(EINVALID_MAX_FEE);
    }
    Ok(BuilderCode { builder, fees })
}

pub fn set_global_max_fee(registry: &mut Registry, new_max_fee: u64) {
    let Registry::V1 { global_max_fee, .. } = registry;
    *global_max_fee = new_max_fee;
}

pub fn approve_max_fee(
    registry: &mut Registry,
    user_address: [u8; 32],
    builder: [u8; 32],
    max_fee: u64,
) -> Result<(), u64> {
    let Registry::V1 { global_max_fee, approved_max_fees } = registry;
    if max_fee > *global_max_fee {
        return Err(EINVALID_MAX_FEE);
    }
    let key = BuilderAndAccount { account: user_address, builder };
    approved_max_fees.insert(key, max_fee);
    Ok(())
}

pub fn revoke_max_fee(
    registry: &mut Registry,
    user_address: [u8; 32],
    builder: [u8; 32],
) -> Result<(), u64> {
    let Registry::V1 { approved_max_fees, .. } = registry;
    let key = BuilderAndAccount { account: user_address, builder };
    if approved_max_fees.remove(&key).is_none() {
        return Err(EBUILDER_NOT_REGISTERED);
    }
    Ok(())
}

pub fn get_builder_fee_for_notional(
    registry: &Registry,
    account: [u8; 32],
    code: BuilderCode,
    notional_value: u128,
) -> u64 {
    let approved_max_fee = get_approved_max_fee(registry, account, code.builder);
    match approved_max_fee {
        None => 0,
        Some(max_fee) => {
            if max_fee == 0 {
                return 0;
            }
            let fee = std::cmp::min(max_fee, code.fees);
            compute_fee_from_basis_points(notional_value, fee)
        }
    }
}

pub fn get_fees_from_builder_code(code: &BuilderCode) -> u64 {
    code.fees
}

pub fn get_builder_from_builder_code(code: &BuilderCode) -> [u8; 32] {
    code.builder
}

fn compute_fee_from_basis_points(notional_value: u128, fee_basis_points: u64) -> u64 {
    let numerator = notional_value * (fee_basis_points as u128);
    let denominator = (FEE_PRECISION * 100) as u128;
    (numerator / denominator) as u64
}

pub fn validate_builder_code(
    registry: &Registry,
    account: [u8; 32],
    code: &BuilderCode,
) -> Result<(), u64> {
    let fees = code.fees;
    let approved_max_fee = get_approved_max_fee(registry, account, code.builder);
    match approved_max_fee {
        None => Err(EBUILDER_NOT_REGISTERED),
        Some(max_fee) => {
            if fees > max_fee {
                Err(EINVALID_MAX_FEE)
            } else {
                Ok(())
            }
        }
    }
}

pub fn is_builder_code_valid(
    registry: &Registry,
    account: [u8; 32],
    code: &BuilderCode,
) -> bool {
    let fees = code.fees;
    let approved_max_fee = get_approved_max_fee(registry, account, code.builder);
    match approved_max_fee {
        None => false,
        Some(max_fee) => fees <= max_fee,
    }
}

pub fn get_approved_max_fee(
    registry: &Registry,
    user: [u8; 32],
    builder: [u8; 32],
) -> Option<u64> {
    let Registry::V1 { global_max_fee, approved_max_fees } = registry;
    let key = BuilderAndAccount { account: user, builder };
    match approved_max_fees.get(&key) {
        None => None,
        Some(&fee) => Some(std::cmp::min(fee, *global_max_fee)),
    }
}


