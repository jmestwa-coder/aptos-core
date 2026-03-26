// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::collateral_balance_sheet
//
// Naming convention:
// - "global" = total funds in the balance sheet across all users
// - "account" = funds belonging to a specific user (cross + isolated combined)
// - "cross" = user's cross-margin collateral domain
// - "isolated" = user's isolated position collateral domain
//
// NOTE: In the native Rust translation, we don't have FungibleAsset/FungibleStore.
// Instead, balances are tracked as numeric values. The CollateralStore and
// FungibleAsset-related operations are simplified to pure balance tracking.

use crate::native_perpdex::i64_aggregator::{
    new_i64_aggregator, I64Aggregator, I64Snapshot,
};
use crate::native_perpdex::math::{self, Precision, get_decimals_multiplier};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===================== Constants =====================

const EINSUFFICIENT_BALANCE: u64 = 1;
const EINVALID_AMOUNT: u64 = 2;
const ENOT_ADMIN: u64 = 4;
const EINVALID_HAIRCUT_BPS: u64 = 5;
const ECOLLATERAL_FUNGIBLE_ASSET_MISMATCH: u64 = 6;
const ECOLLATERAL_AMOUNT_IS_NOT_ZERO: u64 = 7;

const MAX_HAIRCUT_BPS: u64 = 10000; // 100%

// ===================== Types =====================

/// Represents an Object<Metadata> address - asset type identifier
pub type AssetType = [u8; 32];
/// Represents an Object<PerpMarket> address
pub type PerpMarketRef = [u8; 32];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AssetBalance {
    V1 { asset_type: AssetType, balance: u64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollateralBalances {
    pub primary_balance: I64Aggregator,
    pub secondary_balances: Vec<AssetBalance>,
    pub reserved_amount: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum CollateralBalanceType {
    Cross { account: [u8; 32] },
    Isolated { account: [u8; 32], market: PerpMarketRef },
}

#[derive(Clone, Debug, Copy, Serialize, Deserialize)]
pub enum CollateralBalanceChangeType {
    UserMovement,
    Fee,
    PnL,
    Margin,
    Liquidation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecondaryAssetInfo {
    pub value_in_primary_per_unit: u64,
    pub haircut_bps: u64,
    pub asset_precision: Precision,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollateralBalanceSheet {
    pub primary_asset_type: AssetType,
    pub primary_asset_precision: Precision,
    /// Global primary store balance (total fungible assets)
    pub global_primary_store_balance: u64,
    /// Secondary asset info
    pub secondary_stores: BTreeMap<AssetType, SecondaryAssetInfo>,
    /// Global secondary store balances
    pub global_secondary_store_balances: BTreeMap<AssetType, u64>,
    /// Balance table for per-account balances
    pub balance_table: BTreeMap<CollateralBalanceType, CollateralBalances>,
}

/// EVENT: CollateralBalanceChangeEvent
#[derive(Clone, Debug)]
pub enum CollateralBalanceChangeEvent {
    V1 {
        asset_type: AssetType,
        balance_type: CollateralBalanceType,
        delta: i64,
        offset_balance_after: I64Snapshot,
        change_type: CollateralBalanceChangeType,
    },
}

/// EVENT: ReservedCollateralChangedEvent
#[derive(Clone, Debug)]
pub struct ReservedCollateralChangedEvent {
    pub account: [u8; 32],
    pub previous_amount: u64,
    pub new_amount: u64,
}

#[derive(Clone, Debug, Copy, Serialize, Deserialize)]
pub enum CollateralStatus {
    V1 {
        primary: i64,
        secondary: u64,
        reserved: u64,
    },
}

/// Represents a fungible asset that is in the balance sheet fungible store,
/// but not on any account in the balance sheet.
#[derive(Clone, Debug)]
pub struct CollateralFungibleAsset {
    pub metadata: AssetType,
    pub amount: u64,
}

// ===================== Helper Functions =====================

fn create_empty_collateral_balances() -> CollateralBalances {
    CollateralBalances {
        primary_balance: new_i64_aggregator(),
        secondary_balances: Vec::new(),
        reserved_amount: 0,
    }
}

fn get_secondary_balance(secondary_balances: &[AssetBalance], asset_type: AssetType) -> u64 {
    for ab in secondary_balances {
        match ab {
            AssetBalance::V1 {
                asset_type: at,
                balance,
            } => {
                if *at == asset_type {
                    return *balance;
                }
            },
        }
    }
    0
}

fn get_secondary_balance_mut(
    secondary_balances: &mut Vec<AssetBalance>,
    asset_type: AssetType,
) -> &mut u64 {
    // Find existing
    for i in 0..secondary_balances.len() {
        match &secondary_balances[i] {
            AssetBalance::V1 {
                asset_type: at, ..
            } => {
                if *at == asset_type {
                    match &mut secondary_balances[i] {
                        AssetBalance::V1 { balance, .. } => return balance,
                    }
                }
            },
        }
    }
    // Add new
    secondary_balances.push(AssetBalance::V1 {
        asset_type,
        balance: 0,
    });
    let last = secondary_balances.len() - 1;
    match &mut secondary_balances[last] {
        AssetBalance::V1 { balance, .. } => balance,
    }
}

// ===================== CollateralBalanceSheet Implementation =====================

impl CollateralBalanceSheet {
    // RESOURCE: CollateralBalanceSheet at signer address
    pub fn initialize(primary_asset_type: AssetType, primary_decimals: u8) -> Self {
        CollateralBalanceSheet {
            primary_asset_type,
            primary_asset_precision: math::new_precision(primary_decimals).expect("invalid decimals"),
            global_primary_store_balance: 0,
            secondary_stores: BTreeMap::new(),
            global_secondary_store_balances: BTreeMap::new(),
            balance_table: BTreeMap::new(),
        }
    }

    pub fn primary_asset_metadata(&self) -> AssetType {
        self.primary_asset_type
    }

    pub fn primary_asset_precision(&self) -> Precision {
        self.primary_asset_precision
    }

    pub fn is_asset_supported(&self, asset_type: AssetType) -> bool {
        asset_type == self.primary_asset_type || self.secondary_stores.contains_key(&asset_type)
    }

    pub fn value_in_primary_per_unit(&self, asset_type: AssetType) -> u64 {
        self.secondary_stores
            .get(&asset_type)
            .expect("asset not registered")
            .value_in_primary_per_unit
    }

    pub fn secondary_asset_amount_from_value_in_primary(
        &self,
        value_in_primary: u64,
        asset_type: AssetType,
    ) -> u64 {
        let asset_info = self
            .secondary_stores
            .get(&asset_type)
            .expect("asset not registered");
        // math64::mul_div(value_in_primary, decimals_multiplier, value_in_primary_per_unit)
        (value_in_primary as u128)
            .checked_mul(get_decimals_multiplier(&asset_info.asset_precision) as u128)
            .expect("overflow")
            .checked_div(asset_info.value_in_primary_per_unit as u128)
            .expect("div by zero") as u64
    }

    pub fn haircut_bps(&self, asset_type: AssetType) -> u64 {
        self.secondary_stores
            .get(&asset_type)
            .expect("asset not registered")
            .haircut_bps
    }

    pub fn update_haircut_bps(&mut self, asset_type: AssetType, new_haircut_bps: u64) {
        assert!(
            asset_type != self.primary_asset_type,
            "Cannot update primary asset"
        );
        assert!(new_haircut_bps <= MAX_HAIRCUT_BPS, "Invalid haircut bps: {}", EINVALID_HAIRCUT_BPS);
        let asset_info = self
            .secondary_stores
            .get_mut(&asset_type)
            .expect("asset not registered");
        asset_info.haircut_bps = new_haircut_bps;
    }

    pub fn add_secondary_asset(
        &mut self,
        asset_type: AssetType,
        value_in_primary_per_unit: u64,
        haircut_bps: u64,
        asset_decimals: u8,
    ) {
        self.secondary_stores.insert(
            asset_type,
            SecondaryAssetInfo {
                value_in_primary_per_unit,
                haircut_bps,
                asset_precision: math::new_precision(asset_decimals).expect("invalid decimals"),
            },
        );
        self.global_secondary_store_balances.insert(asset_type, 0);
    }

    pub fn balance_of_primary_asset(&self, balance_type: &CollateralBalanceType) -> i64 {
        if let Some(balances) = self.balance_table.get(balance_type) {
            balances.primary_balance.read()
        } else {
            0
        }
    }

    pub fn balance_of_primary_asset_at_least(
        &self,
        balance_type: &CollateralBalanceType,
        threshold: u64,
    ) -> bool {
        if let Some(balances) = self.balance_table.get(balance_type) {
            balances.primary_balance.is_at_least(threshold as i64)
        } else {
            false
        }
    }

    pub fn get_global_primary_store_balance(&self) -> u64 {
        self.global_primary_store_balance
    }

    pub fn get_global_secondary_store_balance(&self, asset_type: AssetType) -> u64 {
        *self
            .global_secondary_store_balances
            .get(&asset_type)
            .unwrap_or(&0)
    }

    pub fn get_primary_store_balance_in_fungible_amount(&self) -> u64 {
        self.global_primary_store_balance
    }

    pub fn get_secondary_store_balance_in_fungible_amount(&self, asset_type: AssetType) -> u64 {
        *self
            .global_secondary_store_balances
            .get(&asset_type)
            .unwrap_or(&0)
    }

    pub fn secondary_asset_amount(
        &self,
        balance_type: &CollateralBalanceType,
        asset_type: AssetType,
    ) -> u64 {
        if let Some(balances) = self.balance_table.get(balance_type) {
            get_secondary_balance(&balances.secondary_balances, asset_type)
        } else {
            0
        }
    }

    pub fn get_secondary_assets_value_in_primary(
        balances: &CollateralBalances,
        secondary_stores: &BTreeMap<AssetType, SecondaryAssetInfo>,
    ) -> u64 {
        let mut secondary_value_in_primary: u64 = 0;
        for ab in &balances.secondary_balances {
            match ab {
                AssetBalance::V1 {
                    asset_type,
                    balance,
                } => {
                    if *balance > 0 {
                        if let Some(store) = secondary_stores.get(asset_type) {
                            let value_in_primary = (*balance as u128)
                                .checked_mul(store.value_in_primary_per_unit as u128)
                                .expect("overflow")
                                .checked_div(
                                    get_decimals_multiplier(&store.asset_precision) as u128,
                                )
                                .expect("div by zero")
                                as u64;
                            let haircut_amount = (value_in_primary as u128)
                                .checked_mul(store.haircut_bps as u128)
                                .expect("overflow")
                                .checked_div(MAX_HAIRCUT_BPS as u128)
                                .expect("div by zero")
                                as u64;
                            let final_value = value_in_primary - haircut_amount;
                            secondary_value_in_primary += final_value;
                        }
                    }
                },
            }
        }
        secondary_value_in_primary
    }

    pub fn total_collateral_value(&self, balance_type: &CollateralBalanceType) -> i64 {
        if let Some(balances) = self.balance_table.get(balance_type) {
            let primary_balance = balances.primary_balance.read();
            let secondary_balance = Self::get_secondary_assets_value_in_primary(
                balances,
                &self.secondary_stores,
            ) as i64;
            primary_balance + secondary_balance
        } else {
            0
        }
    }

    pub fn get_collateral_status(
        &self,
        balance_type: &CollateralBalanceType,
    ) -> CollateralStatus {
        if let Some(balances) = self.balance_table.get(balance_type) {
            CollateralStatus::V1 {
                primary: balances.primary_balance.read(),
                secondary: Self::get_secondary_assets_value_in_primary(
                    balances,
                    &self.secondary_stores,
                ),
                reserved: balances.reserved_amount,
            }
        } else {
            empty_collateral_status()
        }
    }

    pub fn has_any_collateral(&self, balance_type: &CollateralBalanceType) -> bool {
        if let Some(balances) = self.balance_table.get(balance_type) {
            if balances.primary_balance.read() != 0 {
                return true;
            }
            Self::get_secondary_assets_value_in_primary(balances, &self.secondary_stores) != 0
        } else {
            false
        }
    }

    // ========== RESERVED COLLATERAL FUNCTIONS ==========

    pub fn get_reserved_collateral(&self, account: [u8; 32]) -> u64 {
        let balance_type = balance_type_cross(account);
        if let Some(balances) = self.balance_table.get(&balance_type) {
            balances.reserved_amount
        } else {
            0
        }
    }

    pub fn set_reserved_collateral(&mut self, account: [u8; 32], amount: u64) {
        // EVENT: ReservedCollateralChangedEvent
        let balance_type = balance_type_cross(account);
        let balances = self
            .balance_table
            .entry(balance_type)
            .or_insert_with(create_empty_collateral_balances);
        // let _previous_amount = balances.reserved_amount;
        balances.reserved_amount = amount;
    }

    // ========== ASSET MOVEMENT FUNCTIONS ==========

    /// Deposit funds, crediting to the specified balance type.
    /// In native Rust, we just add to the balance directly (no FungibleAsset).
    pub fn deposit_primary_collateral(
        &mut self,
        to: CollateralBalanceType,
        amount: u64,
        _change_type: CollateralBalanceChangeType,
    ) {
        // Credit the global store
        self.global_primary_store_balance += amount;
        // Credit the account
        let balances = self
            .balance_table
            .entry(to)
            .or_insert_with(create_empty_collateral_balances);
        balances.primary_balance.add(amount as i64);
        // EVENT: CollateralBalanceChangeEvent
    }

    /// Deposit secondary asset
    pub fn deposit_secondary_collateral(
        &mut self,
        to: CollateralBalanceType,
        asset_type: AssetType,
        amount: u64,
        _change_type: CollateralBalanceChangeType,
    ) {
        assert!(
            self.secondary_stores.contains_key(&asset_type),
            "Asset type must be registered"
        );
        *self
            .global_secondary_store_balances
            .entry(asset_type)
            .or_insert(0) += amount;
        let balances = self
            .balance_table
            .entry(to)
            .or_insert_with(create_empty_collateral_balances);
        let balance_ref = get_secondary_balance_mut(&mut balances.secondary_balances, asset_type);
        *balance_ref += amount;
        // EVENT: CollateralBalanceChangeEvent
    }

    /// Deposit collateral (auto-detects primary/secondary)
    pub fn deposit_collateral_amount(
        &mut self,
        to: CollateralBalanceType,
        asset_type: AssetType,
        amount: u64,
        change_type: CollateralBalanceChangeType,
    ) {
        if asset_type == self.primary_asset_type {
            self.deposit_primary_collateral(to, amount, change_type);
        } else {
            self.deposit_secondary_collateral(to, asset_type, amount, change_type);
        }
    }

    pub fn withdraw_primary_collateral_unchecked(
        &mut self,
        from: CollateralBalanceType,
        amount: u64,
        _change_type: CollateralBalanceChangeType,
    ) {
        assert!(amount > 0, "Invalid amount: {}", EINVALID_AMOUNT);
        let balances = self
            .balance_table
            .get_mut(&from)
            .expect("balance type not found");
        balances.primary_balance.add(-(amount as i64));
        self.global_primary_store_balance -= amount;
        // EVENT: CollateralBalanceChangeEvent
    }

    pub fn withdraw_collateral_unchecked_for_asset(
        &mut self,
        from: CollateralBalanceType,
        amount: u64,
        asset_type: AssetType,
        change_type: CollateralBalanceChangeType,
    ) {
        if asset_type == self.primary_asset_type {
            self.withdraw_primary_collateral_unchecked(from, amount, change_type);
        } else {
            assert!(amount > 0, "Invalid amount: {}", EINVALID_AMOUNT);
            assert!(
                self.secondary_stores.contains_key(&asset_type),
                "Asset type must be registered"
            );
            let balances = self
                .balance_table
                .get_mut(&from)
                .expect("balance type not found");
            let balance_ref =
                get_secondary_balance_mut(&mut balances.secondary_balances, asset_type);
            assert!(
                *balance_ref >= amount,
                "Insufficient balance: {}",
                EINSUFFICIENT_BALANCE
            );
            *balance_ref -= amount;
            *self
                .global_secondary_store_balances
                .get_mut(&asset_type)
                .expect("secondary store not found") -= amount;
            // EVENT: CollateralBalanceChangeEvent
        }
    }

    fn transfer_primary_asset(
        &mut self,
        from: CollateralBalanceType,
        to: CollateralBalanceType,
        amount: u64,
        from_change_type: CollateralBalanceChangeType,
        to_change_type: CollateralBalanceChangeType,
    ) {
        if amount == 0 {
            return;
        }
        // Withdraw from source (no global store change - internal transfer)
        {
            let from_balances = self
                .balance_table
                .get_mut(&from)
                .expect("from balance type not found");
            from_balances.primary_balance.add(-(amount as i64));
        }
        // Deposit to destination
        {
            let to_balances = self
                .balance_table
                .entry(to)
                .or_insert_with(create_empty_collateral_balances);
            to_balances.primary_balance.add(amount as i64);
        }
        // EVENT: CollateralBalanceChangeEvent x2
    }

    pub fn transfer_from_crossed_to_isolated(
        &mut self,
        account: [u8; 32],
        amount: u64,
        market: PerpMarketRef,
        change_type: CollateralBalanceChangeType,
    ) {
        self.transfer_primary_asset(
            balance_type_cross(account),
            balance_type_isolated(account, market),
            amount,
            change_type,
            change_type,
        );
    }

    pub fn transfer_from_isolated_to_crossed(
        &mut self,
        account: [u8; 32],
        amount: u64,
        market: PerpMarketRef,
        change_type: CollateralBalanceChangeType,
    ) {
        self.transfer_primary_asset(
            balance_type_isolated(account, market),
            balance_type_cross(account),
            amount,
            change_type,
            change_type,
        );
    }

    pub fn transfer_amount_to_backstop_liquidator(
        &mut self,
        from: CollateralBalanceType,
        liquidator: CollateralBalanceType,
        amount: u64,
    ) {
        if amount == 0 {
            return;
        }
        self.transfer_primary_asset(
            from,
            liquidator,
            amount,
            CollateralBalanceChangeType::Liquidation,
            CollateralBalanceChangeType::Liquidation,
        );
    }

    pub fn transfer_to_backstop_liquidator(
        &mut self,
        from: CollateralBalanceType,
        liquidator: CollateralBalanceType,
    ) {
        if !self.balance_table.contains_key(&from) {
            return;
        }
        let from_balances = self.balance_table.remove(&from).unwrap();
        let from_balance_i64 = from_balances.primary_balance.read();

        // Get or create liquidator's balance entry
        let liquidator_balances = self
            .balance_table
            .entry(liquidator)
            .or_insert_with(create_empty_collateral_balances);

        // Transfer primary balance
        if from_balance_i64 != 0 {
            liquidator_balances.primary_balance.add(from_balance_i64);
        }

        // Transfer secondary balances
        for ab in &from_balances.secondary_balances {
            match ab {
                AssetBalance::V1 {
                    asset_type,
                    balance,
                } => {
                    if *balance > 0 {
                        let liq_balance = get_secondary_balance_mut(
                            &mut liquidator_balances.secondary_balances,
                            *asset_type,
                        );
                        *liq_balance += *balance;
                    }
                },
            }
        }
        // EVENT: CollateralBalanceChangeEvent x N
    }

    pub fn decrease_balance_unchecked(
        &mut self,
        from: CollateralBalanceType,
        amount: u64,
        _from_change_type: CollateralBalanceChangeType,
    ) {
        if amount == 0 {
            return;
        }
        let balances = self
            .balance_table
            .get_mut(&from)
            .expect("balance type not found");
        balances.primary_balance.add(-(amount as i64));
        // EVENT: CollateralBalanceChangeEvent
        // NOTE: This does NOT change global_primary_store_balance because
        // funds are "destroyed" against unrealized loss of opposite side.
    }

    pub fn increase_balance(
        &mut self,
        to: CollateralBalanceType,
        amount: u64,
        _to_change_type: CollateralBalanceChangeType,
    ) {
        if amount == 0 {
            return;
        }
        let balances = self
            .balance_table
            .entry(to)
            .or_insert_with(create_empty_collateral_balances);
        balances.primary_balance.add(amount as i64);
        // EVENT: CollateralBalanceChangeEvent
        // NOTE: This does NOT change global_primary_store_balance because
        // funds are "created" against unrealized profit of opposite side.
    }
}

// ===================== CollateralStatus accessors =====================

impl CollateralStatus {
    pub fn get_primary_balance_from_status(&self) -> i64 {
        match self {
            CollateralStatus::V1 { primary, .. } => *primary,
        }
    }

    pub fn get_secondary_balance_from_status(&self) -> u64 {
        match self {
            CollateralStatus::V1 { secondary, .. } => *secondary,
        }
    }

    pub fn get_total_balance_from_status(&self) -> i64 {
        match self {
            CollateralStatus::V1 {
                primary, secondary, ..
            } => *primary + (*secondary as i64),
        }
    }

    pub fn get_reserved_balance_from_status(&self) -> u64 {
        match self {
            CollateralStatus::V1 { reserved, .. } => *reserved,
        }
    }
}

pub fn empty_collateral_status() -> CollateralStatus {
    CollateralStatus::V1 {
        primary: 0,
        secondary: 0,
        reserved: 0,
    }
}

// ===================== CollateralFungibleAsset =====================

impl CollateralFungibleAsset {
    pub fn zero(metadata: AssetType) -> Self {
        CollateralFungibleAsset {
            metadata,
            amount: 0,
        }
    }

    pub fn extract(&mut self, amount: u64) -> Result<CollateralFungibleAsset, u64> {
        if self.amount < amount {
            return Err(EINSUFFICIENT_BALANCE);
        }
        self.amount -= amount;
        Ok(CollateralFungibleAsset {
            metadata: self.metadata,
            amount,
        })
    }

    pub fn merge(&mut self, src: CollateralFungibleAsset) -> Result<(), u64> {
        if src.metadata != self.metadata {
            return Err(ECOLLATERAL_FUNGIBLE_ASSET_MISMATCH);
        }
        self.amount += src.amount;
        Ok(())
    }

    pub fn destroy_zero(self) -> Result<(), u64> {
        if self.amount != 0 {
            return Err(ECOLLATERAL_AMOUNT_IS_NOT_ZERO);
        }
        Ok(())
    }

    pub fn amount(&self) -> u64 {
        self.amount
    }
}

// ===================== Balance Type constructors =====================

pub fn change_type_user_movement() -> CollateralBalanceChangeType {
    CollateralBalanceChangeType::UserMovement
}

pub fn change_type_fee() -> CollateralBalanceChangeType {
    CollateralBalanceChangeType::Fee
}

pub fn change_type_pnl() -> CollateralBalanceChangeType {
    CollateralBalanceChangeType::PnL
}

pub fn change_type_margin() -> CollateralBalanceChangeType {
    CollateralBalanceChangeType::Margin
}

pub fn change_type_liquidation() -> CollateralBalanceChangeType {
    CollateralBalanceChangeType::Liquidation
}

pub fn balance_type_isolated(account: [u8; 32], market: PerpMarketRef) -> CollateralBalanceType {
    CollateralBalanceType::Isolated { account, market }
}

pub fn balance_type_cross(account: [u8; 32]) -> CollateralBalanceType {
    CollateralBalanceType::Cross { account }
}

pub fn get_account_from_balance_type(balance_type: &CollateralBalanceType) -> [u8; 32] {
    match balance_type {
        CollateralBalanceType::Cross { account } => *account,
        CollateralBalanceType::Isolated { account, .. } => *account,
    }
}
