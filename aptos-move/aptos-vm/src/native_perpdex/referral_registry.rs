// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::referral_registry

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===================== Constants =====================

const E_REFERRAL_CODE_TAKEN: u64 = 1;
const EUSER_ALREADY_REGISTERED: u64 = 2;
const EINVALID_REFERRAL_CODE: u64 = 3;
const ENOT_ALPHANUMERIC: u64 = 4;
const ESELF_REFERENCE: u64 = 5;
const EREFERRER_LIMIT_REACHED: u64 = 6;
const EREFERRAL_CODE_LIMIT_REACHED: u64 = 7;

const DEFAULT_MAX_USAGE_PER_REFERRAL_CODE: u64 = 1;
const DEFAULT_MAX_REFERRAL_CODE_PER_ADDRESS: u64 = 5;

// ===================== Types =====================

// EVENT types (collected but not emitted in native context)
// EVENT: ReferralCodeRegisteredEvent
// EVENT: ReferralRegisteredEvent
// EVENT: AffiliateRegisteredEvent

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReferralCode {
    V1 {
        code: String,
        num_referred: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReferrerState {
    V1 {
        codes: Vec<String>,
        max_referral_codes: Option<u64>,
        max_usage_per_referral_code: Option<u64>,
        is_affiliate: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Referrals {
    V1 {
        addr_to_referrer_state: BTreeMap<[u8; 32], ReferrerState>,
        referral_code_to_addr: BTreeMap<String, [u8; 32]>,
        referral_code_to_state: BTreeMap<String, ReferralCode>,
        addr_to_referrer_addr: BTreeMap<[u8; 32], [u8; 32]>,
    },
}

// ===================== Functions =====================

pub fn initialize() -> Referrals {
    Referrals::V1 {
        addr_to_referrer_state: BTreeMap::new(),
        referral_code_to_addr: BTreeMap::new(),
        referral_code_to_state: BTreeMap::new(),
        addr_to_referrer_addr: BTreeMap::new(),
    }
}

fn new_referral_code(code: String) -> ReferralCode {
    ReferralCode::V1 { code, num_referred: 0 }
}

fn is_ascii_alphanumeric(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphanumeric())
}

pub fn register_referral_code(
    referrals: &mut Referrals,
    referrer_addr: [u8; 32],
    referral_code: String,
    bypass_max_limit: bool,
) -> Result<(), u64> {
    let Referrals::V1 {
        addr_to_referrer_state,
        referral_code_to_addr,
        referral_code_to_state,
        ..
    } = referrals;

    if referral_code.is_empty() || referral_code.len() > 32 {
        return Err(EINVALID_REFERRAL_CODE);
    }
    if !is_ascii_alphanumeric(&referral_code) {
        return Err(ENOT_ALPHANUMERIC);
    }
    if referral_code_to_addr.contains_key(&referral_code) {
        return Err(E_REFERRAL_CODE_TAKEN);
    }

    let referrer_state = addr_to_referrer_state
        .remove(&referrer_addr)
        .unwrap_or(ReferrerState::V1 {
            codes: Vec::new(),
            max_referral_codes: None,
            max_usage_per_referral_code: None,
            is_affiliate: false,
        });

    let ReferrerState::V1 {
        mut codes,
        max_referral_codes,
        max_usage_per_referral_code,
        is_affiliate,
    } = referrer_state;

    if !bypass_max_limit {
        let max_limit = max_referral_codes.unwrap_or(DEFAULT_MAX_REFERRAL_CODE_PER_ADDRESS);
        if codes.len() as u64 >= max_limit {
            // Re-insert state before returning error
            addr_to_referrer_state.insert(referrer_addr, ReferrerState::V1 {
                codes,
                max_referral_codes,
                max_usage_per_referral_code,
                is_affiliate,
            });
            return Err(EREFERRAL_CODE_LIMIT_REACHED);
        }
    }

    codes.push(referral_code.clone());

    let referral_code_state = new_referral_code(referral_code.clone());

    let updated_state = ReferrerState::V1 {
        codes,
        max_referral_codes,
        max_usage_per_referral_code,
        is_affiliate,
    };
    addr_to_referrer_state.insert(referrer_addr, updated_state);
    referral_code_to_addr.insert(referral_code.clone(), referrer_addr);
    referral_code_to_state.insert(referral_code, referral_code_state);

    // EVENT: ReferralCodeRegisteredEvent
    Ok(())
}

pub fn register_referral(
    referrals: &mut Referrals,
    referree_addr: [u8; 32],
    referral_code: String,
    bypass_limit: bool,
) -> Result<(), u64> {
    let Referrals::V1 {
        addr_to_referrer_state,
        referral_code_to_addr,
        referral_code_to_state,
        addr_to_referrer_addr,
    } = referrals;

    if addr_to_referrer_addr.contains_key(&referree_addr) {
        return Err(EUSER_ALREADY_REGISTERED);
    }
    let referrer_addr = match referral_code_to_addr.get(&referral_code) {
        Some(addr) => *addr,
        None => return Err(EINVALID_REFERRAL_CODE),
    };
    if referrer_addr == referree_addr {
        return Err(ESELF_REFERENCE);
    }

    let referral_code_state = match referral_code_to_state.get(&referral_code) {
        Some(state) => state.clone(),
        None => return Err(EINVALID_REFERRAL_CODE),
    };

    let ReferralCode::V1 { code, num_referred } = &referral_code_state;

    if !bypass_limit {
        let referrer_state = addr_to_referrer_state.get(&referrer_addr);
        let max_usage_limit = match referrer_state {
            Some(ReferrerState::V1 { max_usage_per_referral_code, .. }) => {
                max_usage_per_referral_code.unwrap_or(DEFAULT_MAX_USAGE_PER_REFERRAL_CODE)
            }
            None => DEFAULT_MAX_USAGE_PER_REFERRAL_CODE,
        };
        if *num_referred >= max_usage_limit {
            return Err(EREFERRER_LIMIT_REACHED);
        }
    }

    let updated_state = ReferralCode::V1 {
        code: code.clone(),
        num_referred: num_referred + 1,
    };

    referral_code_to_state.insert(referral_code.clone(), updated_state);
    addr_to_referrer_addr.insert(referree_addr, referrer_addr);

    // EVENT: ReferralRegisteredEvent
    Ok(())
}

pub fn register_affiliate(
    referrals: &mut Referrals,
    affiliate_addr: [u8; 32],
) {
    let Referrals::V1 { addr_to_referrer_state, .. } = referrals;

    let mut referrer_state = addr_to_referrer_state
        .remove(&affiliate_addr)
        .unwrap_or(ReferrerState::V1 {
            codes: Vec::new(),
            max_referral_codes: None,
            max_usage_per_referral_code: None,
            is_affiliate: false,
        });

    let ReferrerState::V1 {
        ref mut is_affiliate,
        ref mut max_usage_per_referral_code,
        ..
    } = referrer_state;
    *is_affiliate = true;
    *max_usage_per_referral_code = Some(u64::MAX);

    addr_to_referrer_state.insert(affiliate_addr, referrer_state);
    // EVENT: AffiliateRegisteredEvent
}

pub fn get_referral_codes(referrals: &Referrals, user_addr: [u8; 32]) -> Vec<String> {
    let Referrals::V1 { addr_to_referrer_state, .. } = referrals;
    match addr_to_referrer_state.get(&user_addr) {
        Some(ReferrerState::V1 { codes, .. }) => codes.clone(),
        None => Vec::new(),
    }
}

pub fn get_referrer_addr(referrals: &Referrals, user_addr: [u8; 32]) -> Option<[u8; 32]> {
    let Referrals::V1 { addr_to_referrer_addr, .. } = referrals;
    addr_to_referrer_addr.get(&user_addr).copied()
}

pub fn get_referrer_state(referrals: &Referrals, user_addr: [u8; 32]) -> Option<ReferrerState> {
    let Referrals::V1 { addr_to_referrer_state, .. } = referrals;
    addr_to_referrer_state.get(&user_addr).cloned()
}

pub fn get_referral_code_state(referrals: &Referrals, referral_code: &str) -> Option<ReferralCode> {
    let Referrals::V1 { referral_code_to_state, .. } = referrals;
    referral_code_to_state.get(referral_code).cloned()
}

pub fn get_referrer_for_code(referrals: &Referrals, referral_code: &str) -> Option<[u8; 32]> {
    let Referrals::V1 { referral_code_to_addr, .. } = referrals;
    referral_code_to_addr.get(referral_code).copied()
}

pub fn set_max_referral_codes_for_address(
    referrals: &mut Referrals,
    user_addr: [u8; 32],
    max: u64,
) {
    let Referrals::V1 { addr_to_referrer_state, .. } = referrals;

    let mut referrer_state = addr_to_referrer_state
        .remove(&user_addr)
        .unwrap_or(ReferrerState::V1 {
            codes: Vec::new(),
            max_referral_codes: None,
            max_usage_per_referral_code: None,
            is_affiliate: false,
        });

    let ReferrerState::V1 { ref mut max_referral_codes, .. } = referrer_state;
    *max_referral_codes = Some(max);
    addr_to_referrer_state.insert(user_addr, referrer_state);
}

pub fn set_max_usage_per_referral_code_for_address(
    referrals: &mut Referrals,
    user_addr: [u8; 32],
    max: u64,
) {
    let Referrals::V1 { addr_to_referrer_state, .. } = referrals;

    let mut referrer_state = addr_to_referrer_state
        .remove(&user_addr)
        .unwrap_or(ReferrerState::V1 {
            codes: Vec::new(),
            max_referral_codes: None,
            max_usage_per_referral_code: None,
            is_affiliate: false,
        });

    let ReferrerState::V1 { ref mut max_usage_per_referral_code, .. } = referrer_state;
    *max_usage_per_referral_code = Some(max);
    addr_to_referrer_state.insert(user_addr, referrer_state);
}

pub fn get_max_referral_codes_for_address(
    referrals: &Referrals,
    user_addr: [u8; 32],
) -> Option<u64> {
    let Referrals::V1 { addr_to_referrer_state, .. } = referrals;
    match addr_to_referrer_state.get(&user_addr) {
        Some(ReferrerState::V1 { max_referral_codes, .. }) => *max_referral_codes,
        None => None,
    }
}

pub fn get_max_usage_per_referral_code_for_address(
    referrals: &Referrals,
    user_addr: [u8; 32],
) -> Option<u64> {
    let Referrals::V1 { addr_to_referrer_state, .. } = referrals;
    match addr_to_referrer_state.get(&user_addr) {
        Some(ReferrerState::V1 { max_usage_per_referral_code, .. }) => *max_usage_per_referral_code,
        None => None,
    }
}
