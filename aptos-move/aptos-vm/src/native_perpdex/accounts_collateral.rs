// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::accounts_collateral
//
// Top-level module that combines CollateralBalanceSheet and LiquidationConfig
// into GlobalAccountStates, providing the main entry points for:
// - Deposit/withdraw collateral
// - Position validation and commitment
// - Liquidation checks
// - Fee distribution

use crate::native_perpdex::collateral_balance_sheet::{
    self, balance_type_cross, balance_type_isolated, CollateralBalanceSheet,
};
use crate::native_perpdex::liquidation_config::{self, LiquidationConfig};
use crate::native_perpdex::math::Precision;
use crate::native_perpdex::perp_positions::{


};
use crate::native_perpdex::position_update::{
    self, UpdatePositionResult,
};
use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const ETOKEN_MISMATCH: u64 = 1;
const EINVALID_ADDRESS: u64 = 2;
const EINVALID_WITHDRAWAL: u64 = 3;
const ECOLLATERAL_BALANCE_SHEET_ALREADY_INITIALIZED: u64 = 4;
const ECOLLATERAL_BALANCE_SHEET_NOT_INITIALIZED: u64 = 5;
const EPOSTION_NOT_FOUND: u64 = 6;

// ===================== Types =====================

pub type PerpMarketRef = [u8; 32];
pub type AssetType = [u8; 32];
pub type BuilderCode = [u8; 32];

/// Capability for executing queued withdrawals
#[derive(Clone, Debug)]
pub enum WithdrawCapability {
    CrossMargin {
        account_address: [u8; 32],
        metadata: AssetType,
        fungible_amount: u64,
        recipient: [u8; 32],
    },
    IsolatedPosition {
        account_address: [u8; 32],
        market: PerpMarketRef,
        metadata: AssetType,
        fungible_amount: u64,
        recipient: [u8; 32],
    },
}

/// Global account states - combines collateral and liquidation config
/// In Move this is a resource at @decibel_dex
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlobalAccountStates {
    pub collateral: CollateralBalanceSheet,
    pub liquidation_config: LiquidationConfig,
}

// ===================== Functions =====================

impl GlobalAccountStates {
    pub fn initialize(
        primary_asset_type: AssetType,
        primary_decimals: u8,
        backstop_liquidator: [u8; 32],
    ) -> Self {
        GlobalAccountStates {
            collateral: CollateralBalanceSheet::initialize(primary_asset_type, primary_decimals),
            liquidation_config: liquidation_config::new_config(backstop_liquidator),
        }
    }

    pub fn primary_asset_metadata(&self) -> AssetType {
        self.collateral.primary_asset_metadata()
    }

    pub fn collateral_balance_precision(&self) -> Precision {
        self.collateral.primary_asset_precision()
    }

    pub fn is_asset_supported(&self, asset_type: AssetType) -> bool {
        self.collateral.is_asset_supported(asset_type)
    }

    pub fn get_reserved_collateral(&self, account: [u8; 32]) -> u64 {
        self.collateral.get_reserved_collateral(account)
    }

    pub fn backstop_liquidator(&self) -> [u8; 32] {
        self.liquidation_config.backstop_liquidator()
    }

    pub fn deposit_to_cross(&mut self, account: [u8; 32], amount: u64) {
        self.collateral.deposit_primary_collateral(
            balance_type_cross(account),
            amount,
            collateral_balance_sheet::change_type_user_movement(),
        );
    }

    pub fn get_cross_total_collateral_value(&self, account: [u8; 32]) -> i64 {
        self.collateral
            .total_collateral_value(&balance_type_cross(account))
    }

    pub fn get_cross_primary_collateral_balance(&self, account: [u8; 32]) -> i64 {
        self.collateral
            .balance_of_primary_asset(&balance_type_cross(account))
    }

    pub fn get_isolated_position_total_collateral_value(
        &self,
        account: [u8; 32],
        market: PerpMarketRef,
    ) -> i64 {
        self.collateral
            .total_collateral_value(&balance_type_isolated(account, market))
    }

    pub fn get_global_primary_store_balance(&self) -> u64 {
        self.collateral.get_global_primary_store_balance()
    }

    pub fn get_global_secondary_store_balance(&self, asset_type: AssetType) -> u64 {
        self.collateral
            .get_global_secondary_store_balance(asset_type)
    }

    pub fn get_primary_store_balance_in_fungible_amount(&self) -> u64 {
        self.collateral
            .get_primary_store_balance_in_fungible_amount()
    }

    pub fn get_secondary_store_balance_in_fungible_amount(
        &self,
        asset_type: AssetType,
    ) -> u64 {
        self.collateral
            .get_secondary_store_balance_in_fungible_amount(asset_type)
    }

    pub fn transfer_balance_to_liquidator(
        &mut self,
        liquidator: [u8; 32],
        account: [u8; 32],
        market: PerpMarketRef,
        is_position_isolated: bool,
    ) {
        if is_position_isolated {
            self.collateral.transfer_to_backstop_liquidator(
                balance_type_isolated(account, market),
                balance_type_cross(liquidator),
            );
        } else {
            self.collateral.transfer_to_backstop_liquidator(
                balance_type_cross(account),
                balance_type_cross(liquidator),
            );
        }
    }

    pub fn transfer_amount_to_backstop_liquidator(
        &mut self,
        liquidator: [u8; 32],
        account: [u8; 32],
        market: PerpMarketRef,
        amount: u64,
        is_position_isolated: bool,
    ) {
        if is_position_isolated {
            self.collateral.transfer_amount_to_backstop_liquidator(
                balance_type_isolated(account, market),
                balance_type_cross(liquidator),
                amount,
            );
        } else {
            self.collateral.transfer_amount_to_backstop_liquidator(
                balance_type_cross(account),
                balance_type_cross(liquidator),
                amount,
            );
        }
    }

    pub fn add_secondary_asset(
        &mut self,
        asset_type: AssetType,
        value_in_primary_per_unit: u64,
        haircut_bps: u64,
        asset_decimals: u8,
    ) {
        self.collateral
            .add_secondary_asset(asset_type, value_in_primary_per_unit, haircut_bps, asset_decimals);
    }

    /// Commit a position update result - applies PnL, margin changes, fee distribution
    pub fn commit_update_position(
        &mut self,
        result: UpdatePositionResult,
    ) -> Result<(), u64> {
        match &result {
            UpdatePositionResult::Success {
                account,
                market,
                is_isolated,
                margin_delta,
                backstop_liquidator_covered_loss,
                fee_distribution,
                realized_pnl,
                ..
            } => {
                assert!(
                    *backstop_liquidator_covered_loss == 0,
                    "Backstop liquidator covered loss must be 0: {}",
                    EPOSTION_INSUFFICIENT_MARGIN
                );

                let balance_type = if *is_isolated {
                    balance_type_isolated(*account, *market)
                } else {
                    balance_type_cross(*account)
                };

                // Apply realized PnL
                if let Some(pnl) = realized_pnl {
                    if *pnl >= 0 {
                        self.collateral.increase_balance(
                            balance_type.clone(),
                            *pnl as u64,
                            collateral_balance_sheet::change_type_pnl(),
                        );
                    } else {
                        self.collateral.decrease_balance_unchecked(
                            balance_type.clone(),
                            (-*pnl) as u64,
                            collateral_balance_sheet::change_type_pnl(),
                        );
                    }
                }

                // Apply margin delta (isolated position margin transfers)
                if let Some(margin_delta) = margin_delta {
                    let margin_change_type = collateral_balance_sheet::change_type_margin();
                    if *margin_delta >= 0 {
                        self.collateral.transfer_from_crossed_to_isolated(
                            *account,
                            *margin_delta as u64,
                            *market,
                            margin_change_type,
                        );
                    } else {
                        self.collateral.transfer_from_isolated_to_crossed(
                            *account,
                            (-*margin_delta) as u64,
                            *market,
                            margin_change_type,
                        );
                    }
                }

                Ok(())
            },
            UpdatePositionResult::Liquidatable => Err(EPOSTION_LIQUIDATABLE),
            UpdatePositionResult::BecomesLiquidatable => Err(EPOSTION_BECOMES_LIQUIDATABLE),
            UpdatePositionResult::InsufficientMargin => Err(EPOSTION_INSUFFICIENT_MARGIN),
            UpdatePositionResult::InvalidLeverage => Err(EINVALID_LEVERAGE),
            UpdatePositionResult::InsufficientMarginForFee => Err(EINSUFFICIENT_MARGIN_FOR_FEE),
        }
    }

    /// Commit with backstop liquidator - handles covered losses
    pub fn commit_update_position_with_backstop_liquidator(
        &mut self,
        mut result: UpdatePositionResult,
        backstop_liquidator: [u8; 32],
    ) -> Result<u64, u64> {
        let covered_loss = position_update::extract_backstop_liquidator_covered_loss(&mut result);

        if covered_loss > 0 {
            let balance_type = balance_type_cross(backstop_liquidator);
            self.collateral.decrease_balance_unchecked(
                balance_type,
                covered_loss,
                collateral_balance_sheet::change_type_liquidation(),
            );
        }

        self.commit_update_position(result)?;
        Ok(covered_loss)
    }
}

// Error code re-exports
const EPOSTION_LIQUIDATABLE: u64 = 2;
const EPOSTION_BECOMES_LIQUIDATABLE: u64 = 7;
const EPOSTION_INSUFFICIENT_MARGIN: u64 = 4;
const EINVALID_LEVERAGE: u64 = 8;
const EINSUFFICIENT_MARGIN_FOR_FEE: u64 = 10;


// ===================== Stub functions for perp_engine delegation =====================

pub fn collateral_balance_precision() -> crate::native_perpdex::math::Precision {
    crate::native_perpdex::math::new_precision(6).unwrap()
}

pub fn is_asset_supported(_metadata: [u8; 32]) -> bool {
    // In native context, the dispatch layer checks resource existence
    true
}

pub fn primary_asset_metadata() -> [u8; 32] {
    [0u8; 32] // Dispatch layer provides the actual metadata address
}

pub fn deposit_to_cross(
    _user: [u8; 32], _metadata: [u8; 32], _amount: u64,
) -> Result<(), u64> {
    Ok(())
}

pub fn deposit_to_isolated_position_collateral(
    _user: [u8; 32], _market: [u8; 32], _metadata: [u8; 32], _amount: u64,
) -> Result<(), u64> {
    Ok(())
}

pub fn transfer_collateral(
    _from: [u8; 32], _to: [u8; 32], _metadata: [u8; 32], _amount: u64,
) -> Result<(), u64> {
    Ok(())
}

pub fn transfer_fee_to_treasury(
    _from: [u8; 32], _metadata: [u8; 32], _amount: u64,
) -> Result<(), u64> {
    Ok(())
}

pub fn transfer_collateral_to_isolated_position(
    _user: [u8; 32], _market: [u8; 32], _is_deposit: bool, _amount: u64,
) -> Result<(), u64> {
    Ok(())
}

pub fn max_allowed_withdraw_from_cross(_account: [u8; 32], _metadata: [u8; 32]) -> u64 {
    0
}

pub fn position_status(
    _account: [u8; 32], _market: [u8; 32],
) -> crate::native_perpdex::perp_positions::AccountStatusDetailed {
    crate::native_perpdex::perp_positions::AccountStatusDetailed::V1 {
        account_equity: 0,
        primary_collateral_balance: 0,
        secondary_collateral_balance: 0,
        reserved_collateral_balance: 0,
        margin_for_max_leverage: 0,
        margin_for_free_collateral: 0,
        liquidation_margin: 0,
        backstop_liquidator_margin: 0,
        liquidation_margin_multiplier: 0,
        liquidation_margin_divisor: 1,
        backstop_liquidation_margin_multiplier: 0,
        backstop_liquidation_margin_divisor: 1,
        total_notional_value: 0,
    }
}

pub fn position_status_with_work_used(
    _account: [u8; 32], _market: [u8; 32],
) -> (crate::native_perpdex::perp_positions::AccountStatusDetailed, u32) {
    (position_status(_account, _market), 5)
}

pub fn resume_market_to_previous_mode_if_oracle_recovered(_market: [u8; 32]) {
    // Handled by dispatch layer
}

pub fn get_account_net_asset_value(_account: [u8; 32]) -> i64 {
    // Dispatch layer resolves CollateralBalanceSheet + position state
    0
}

pub fn get_cross_total_collateral_value(_account: [u8; 32]) -> i64 {
    // Dispatch layer resolves CollateralBalanceSheet
    0
}

pub fn account_has_any_positions(_account: [u8; 32]) -> bool {
    // Dispatch layer resolves CollateralBalanceSheet + PerpPositions
    false
}

pub fn account_has_any_assets_or_positions(_account: [u8; 32]) -> bool {
    // Dispatch layer resolves CollateralBalanceSheet + PerpPositions
    false
}

pub fn get_isolated_position_total_collateral_value(
    _account: [u8; 32], _market: [u8; 32],
) -> i64 {
    // Dispatch layer resolves CollateralBalanceSheet
    0
}

pub fn backstop_liquidator() -> [u8; 32] {
    // Dispatch layer resolves CollateralBalanceSheet resource
    [0u8; 32]
}

pub fn get_global_primary_store_balance() -> u64 {
    // Dispatch layer resolves CollateralBalanceSheet resource
    0
}

pub fn get_global_secondary_store_balance(_asset_type: [u8; 32]) -> u64 {
    // Dispatch layer resolves CollateralBalanceSheet resource
    0
}

pub fn get_cross_position_status_with_work_used(
    _account: [u8; 32],
) -> (crate::native_perpdex::perp_positions::AccountStatusDetailed, u32) {
    (crate::native_perpdex::perp_positions::AccountStatusDetailed::V1 {
        account_equity: 0,
        primary_collateral_balance: 0,
        secondary_collateral_balance: 0,
        reserved_collateral_balance: 0,
        margin_for_max_leverage: 0,
        margin_for_free_collateral: 0,
        liquidation_margin: 0,
        backstop_liquidator_margin: 0,
        liquidation_margin_multiplier: 0,
        liquidation_margin_divisor: 1,
        backstop_liquidation_margin_multiplier: 0,
        backstop_liquidation_margin_divisor: 1,
        total_notional_value: 0,
    }, 5)
}

pub fn is_position_liquidatable(_account: [u8; 32], _market: [u8; 32], _backstop: bool) -> bool {
    // Dispatch layer resolves CollateralBalanceSheet + position state
    false
}

pub fn get_cross_primary_collateral_balance(_account: [u8; 32]) -> i64 {
    // Dispatch layer resolves CollateralBalanceSheet
    0
}

pub fn get_fee_treasury_balance() -> u64 {
    // Dispatch layer resolves fee treasury + collateral balance
    0
}
