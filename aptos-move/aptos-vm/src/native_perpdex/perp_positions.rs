// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::perp_positions
//
// This is the core position management module. In Move, positions are stored as
// resources (UserPositions, AccountInfo, CachedPositionStatuses). In native Rust,
// these are passed as parameters.
//
// Key types: PerpPosition, PositionStatus, PositionAndCollateralStatus, AccountStatusDetailed

use crate::native_perpdex::collateral_balance_sheet::CollateralStatus;
use crate::native_perpdex::i64_math;
use crate::native_perpdex::liquidation_config::LiquidationConfig;
use crate::native_perpdex::math;
use crate::native_perpdex::price_management::{self, AccumulativeIndex};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===================== Constants =====================

const EINVALID_LEVERAGE: u64 = 1;
pub const EPOSTION_INSUFFICIENT_MARGIN: u64 = 2;
const EPOSTION_NOT_FOUND: u64 = 3;
const ECANNOT_MODIFY_SETTINGS_WHILE_HOLDING_POSITION: u64 = 4;
const EUSER_NOT_INITIALIZED: u64 = 5;
const ENOT_ADMIN: u64 = 10;
const EMARKET_IS_ISOLATED_ONLY: u64 = 11;

// ===================== Types =====================

pub type PerpMarketRef = [u8; 32];

#[derive(Clone, Debug, Serialize, Deserialize, Copy)]
pub enum TradeTriggerSource {
    OrderFill,
    MarginCall,
    BackStopLiquidation,
    ADL,
    MarketDelisted,
}

pub fn new_trade_trigger_source_order_fill() -> TradeTriggerSource {
    TradeTriggerSource::OrderFill
}

pub fn new_trade_trigger_source_margin_call() -> TradeTriggerSource {
    TradeTriggerSource::MarginCall
}

pub fn new_trade_trigger_source_backstop_liquidation() -> TradeTriggerSource {
    TradeTriggerSource::BackStopLiquidation
}

pub fn new_trade_trigger_source_adl() -> TradeTriggerSource {
    TradeTriggerSource::ADL
}

pub fn new_trade_trigger_source_market_delisted() -> TradeTriggerSource {
    TradeTriggerSource::MarketDelisted
}

/// Core position data for perpetual positions
#[derive(Clone, Debug, Serialize, Deserialize, Copy)]
pub enum PerpPosition {
    V1 {
        size: u64,
        entry_px_times_size_sum: u128,
        avg_acquire_entry_px: u64,
        user_leverage: u8,
        is_long: bool,
        is_isolated: bool,
        funding_index_at_last_update: AccumulativeIndex,
        unrealized_funding_amount_before_last_update: i64,
        timestamp: u64,
    },
}

/// Wrapper bundling a PerpPosition with its market key
#[derive(Clone, Debug)]
pub enum PerpPositionWithMarket {
    V1 {
        market: PerpMarketRef,
        position: PerpPosition,
    },
}

#[derive(Clone, Debug, Copy, Serialize, Deserialize)]
pub enum PositionStatus {
    V1 {
        unrealized_pnl: i64,
        haircutted_upnl: i64,
        margin_for_max_leverage: u64,
        margin_for_free_collateral: u64,
        total_notional_value: u64,
    },
}

/// Combines position metrics with collateral information
#[derive(Clone, Debug)]
pub enum PositionAndCollateralStatus {
    V1 {
        collateral_status: CollateralStatus,
        position_status: PositionStatus,
    },
}

#[derive(Clone, Debug)]
pub enum AccountStatusDetailed {
    V1 {
        account_equity: i64,
        primary_collateral_balance: i64,
        secondary_collateral_balance: u64,
        reserved_collateral_balance: u64,
        margin_for_max_leverage: u64,
        margin_for_free_collateral: u64,
        liquidation_margin: u64,
        backstop_liquidator_margin: u64,
        liquidation_margin_multiplier: u64,
        liquidation_margin_divisor: u64,
        backstop_liquidation_margin_multiplier: u64,
        backstop_liquidation_margin_divisor: u64,
        total_notional_value: u64,
    },
}

/// Per-account positions storage (equivalent to Move's UserPositions resource)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum UserPositions {
    V1 {
        positions: BTreeMap<PerpMarketRef, PerpPosition>,
    },
}

impl UserPositions {
    pub fn positions(&self) -> &BTreeMap<PerpMarketRef, PerpPosition> {
        let UserPositions::V1 { positions } = self;
        positions
    }

    pub fn positions_mut(&mut self) -> &mut BTreeMap<PerpMarketRef, PerpPosition> {
        let UserPositions::V1 { positions } = self;
        positions
    }
}

/// Account info (equivalent to Move's AccountInfo resource)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AccountInfo {
    V1 {
        primary_account_addr: [u8; 32],
    },
}

// ===================== PerpPosition Accessors =====================

impl PerpPosition {
    pub fn get_size(&self) -> u64 {
        match self {
            PerpPosition::V1 { size, .. } => *size,
        }
    }

    pub fn get_timestamp(&self) -> u64 {
        match self {
            PerpPosition::V1 { timestamp, .. } => *timestamp,
        }
    }

    pub fn get_entry_px_times_size_sum(&self) -> u128 {
        match self {
            PerpPosition::V1 {
                entry_px_times_size_sum,
                ..
            } => *entry_px_times_size_sum,
        }
    }

    pub fn get_user_leverage(&self) -> u8 {
        match self {
            PerpPosition::V1 { user_leverage, .. } => *user_leverage,
        }
    }

    pub fn is_long(&self) -> bool {
        match self {
            PerpPosition::V1 { is_long, .. } => *is_long,
        }
    }

    pub fn is_isolated(&self) -> bool {
        match self {
            PerpPosition::V1 { is_isolated, .. } => *is_isolated,
        }
    }

    pub fn get_avg_acquire_entry_px(&self) -> u64 {
        match self {
            PerpPosition::V1 {
                avg_acquire_entry_px,
                ..
            } => *avg_acquire_entry_px,
        }
    }

    pub fn get_funding_index_at_last_update(&self) -> AccumulativeIndex {
        match self {
            PerpPosition::V1 {
                funding_index_at_last_update,
                ..
            } => *funding_index_at_last_update,
        }
    }

    pub fn get_unrealized_funding_amount_before_last_update(&self) -> i64 {
        match self {
            PerpPosition::V1 {
                unrealized_funding_amount_before_last_update,
                ..
            } => *unrealized_funding_amount_before_last_update,
        }
    }
}

// ===================== Position Construction =====================

pub fn new_empty_perp_position(user_leverage: u8, funding_index: AccumulativeIndex) -> PerpPosition {
    PerpPosition::V1 {
        size: 0,
        entry_px_times_size_sum: 0,
        avg_acquire_entry_px: 0,
        user_leverage,
        is_long: true,
        is_isolated: false,
        funding_index_at_last_update: funding_index,
        unrealized_funding_amount_before_last_update: 0,
        timestamp: 0,
    }
}

pub fn new_empty_perp_position_with_mode(
    user_leverage: u8,
    is_isolated: bool,
    funding_index: AccumulativeIndex,
) -> PerpPosition {
    PerpPosition::V1 {
        size: 0,
        entry_px_times_size_sum: 0,
        avg_acquire_entry_px: 0,
        user_leverage,
        is_long: true,
        is_isolated,
        funding_index_at_last_update: funding_index,
        unrealized_funding_amount_before_last_update: 0,
        timestamp: 0,
    }
}

pub fn new_perp_position_with_mode(
    size: u64,
    entry_px_times_size_sum: u128,
    user_leverage: u8,
    is_long: bool,
    is_isolated: bool,
    funding_index: AccumulativeIndex,
    timestamp: u64,
) -> PerpPosition {
    let avg_acquire_entry_px = if size == 0 {
        0
    } else {
        (entry_px_times_size_sum / (size as u128)) as u64
    };
    PerpPosition::V1 {
        size,
        entry_px_times_size_sum,
        avg_acquire_entry_px,
        user_leverage,
        is_long,
        is_isolated,
        funding_index_at_last_update: funding_index,
        unrealized_funding_amount_before_last_update: 0,
        timestamp,
    }
}

// ===================== Margin Calculation =====================

pub fn margin_required_formula(size: u64, price: u64, size_multiplier: u64, leverage: u8) -> u64 {
    math::ceil_mul_div_64(size, price, size_multiplier * (leverage as u64)).expect("margin calc overflow")
}

// ===================== PnL Calculation =====================

impl PerpPosition {
    /// Calculate PnL with funding cost
    pub fn pnl_with_funding_impl(
        &self,
        size_multiplier: u64,
        updated_funding_index: &AccumulativeIndex,
        mark_price: u64,
    ) -> i64 {
        let PerpPosition::V1 {
            size,
            entry_px_times_size_sum,
            is_long,
            funding_index_at_last_update,
            unrealized_funding_amount_before_last_update,
            ..
        } = self;

        if *size == 0 {
            return 0;
        }

        let current_px_times_size = (mark_price as u128) * (*size as u128);

        let (is_positive, price_diff) = if current_px_times_size >= *entry_px_times_size_sum {
            (*is_long, current_px_times_size - *entry_px_times_size_sum)
        } else {
            (!*is_long, *entry_px_times_size_sum - current_px_times_size)
        };

        let absolute_pnl = math::div_direction_128(price_diff, size_multiplier as u128, !is_positive).expect("pnl calc overflow") as u64;

        let pnl = if is_positive {
            absolute_pnl as i64
        } else {
            -(absolute_pnl as i64)
        };

        // Subtract unrealized funding cost
        let funding_cost = self.get_position_funding_cost_impl(
            size_multiplier,
            updated_funding_index,
        );

        pnl - funding_cost
    }

    /// Get total funding cost from stored + accumulated
    pub fn get_position_funding_cost_impl(
        &self,
        size_multiplier: u64,
        updated_funding_index: &AccumulativeIndex,
    ) -> i64 {
        let PerpPosition::V1 {
            size,
            is_long,
            funding_index_at_last_update,
            unrealized_funding_amount_before_last_update,
            ..
        } = self;

        let unrealized_funding_cost = price_management::get_funding_cost(
            funding_index_at_last_update,
            updated_funding_index,
            *size,
            size_multiplier,
            *is_long,
        );

        *unrealized_funding_amount_before_last_update + unrealized_funding_cost
    }

    /// Get funding cost and updated index
    pub fn get_position_funding_cost_and_index(
        &self,
        size_multiplier: u64,
        updated_funding_index: &AccumulativeIndex,
    ) -> (i64, AccumulativeIndex) {
        let total_funding_cost = self.get_position_funding_cost_impl(
            size_multiplier,
            updated_funding_index,
        );
        (total_funding_cost, *updated_funding_index)
    }
}

// ===================== Position Status =====================

pub fn new_account_status() -> PositionStatus {
    PositionStatus::V1 {
        unrealized_pnl: 0,
        haircutted_upnl: 0,
        margin_for_max_leverage: 0,
        margin_for_free_collateral: 0,
        total_notional_value: 0,
    }
}

impl PositionStatus {
    pub fn get_margin_for_max_leverage(&self) -> u64 {
        match self {
            PositionStatus::V1 {
                margin_for_max_leverage,
                ..
            } => *margin_for_max_leverage,
        }
    }

    pub fn get_margin_for_free_collateral(&self) -> u64 {
        match self {
            PositionStatus::V1 {
                margin_for_free_collateral,
                ..
            } => *margin_for_free_collateral,
        }
    }

    pub fn get_unrealized_pnl(&self) -> i64 {
        match self {
            PositionStatus::V1 { unrealized_pnl, .. } => *unrealized_pnl,
        }
    }

    pub fn get_haircutted_upnl(&self) -> i64 {
        match self {
            PositionStatus::V1 { haircutted_upnl, .. } => *haircutted_upnl,
        }
    }

    pub fn is_account_liquidatable(
        &self,
        total_collateral_balance: i64,
        liquidation_config: &LiquidationConfig,
        backstop_liquidation: bool,
    ) -> bool {
        let liquidation_margin = liquidation_config.get_liquidation_margin(
            self.get_margin_for_max_leverage(),
            backstop_liquidation,
        );
        total_collateral_balance + self.get_unrealized_pnl() < (liquidation_margin as i64)
    }

    pub fn update_position_status_to_add_position(
        &mut self,
        position: &PerpPosition,
        mark_price: u64,
        size_multiplier: u64,
        max_leverage: u8,
        haircut_bps: u64,
        funding_index: &AccumulativeIndex,
    ) {
        if position.get_size() == 0 {
            return;
        }
        let user_leverage = position.get_user_leverage();
        let free_collateral_max_leverage = std::cmp::min(user_leverage as u64, max_leverage as u64) as u8;

        let pnl = position.pnl_with_funding_impl(size_multiplier, funding_index, mark_price);
        let pnl_haircutted = apply_upnl_haircut(pnl, haircut_bps);

        let margin_for_max_lev = margin_required_formula(
            position.get_size(),
            mark_price,
            size_multiplier,
            max_leverage,
        );
        let margin_for_free = margin_required_formula(
            position.get_size(),
            mark_price,
            size_multiplier,
            free_collateral_max_leverage,
        );
        let notional = (position.get_size() as u128 * mark_price as u128 / size_multiplier as u128) as u64;

        match self {
            PositionStatus::V1 {
                unrealized_pnl,
                haircutted_upnl,
                margin_for_max_leverage,
                margin_for_free_collateral,
                total_notional_value,
            } => {
                *unrealized_pnl += pnl;
                *haircutted_upnl += pnl_haircutted;
                *margin_for_max_leverage += margin_for_max_lev;
                *margin_for_free_collateral += margin_for_free;
                *total_notional_value += notional;
            },
        }
    }

    pub fn update_position_status_to_remove_position(
        &mut self,
        position: &PerpPosition,
        mark_price: u64,
        size_multiplier: u64,
        max_leverage: u8,
        haircut_bps: u64,
        funding_index: &AccumulativeIndex,
    ) {
        if position.get_size() == 0 {
            return;
        }
        let user_leverage = position.get_user_leverage();
        let free_collateral_max_leverage = std::cmp::min(user_leverage as u64, max_leverage as u64) as u8;

        let pnl = position.pnl_with_funding_impl(size_multiplier, funding_index, mark_price);
        let pnl_haircutted = apply_upnl_haircut(pnl, haircut_bps);

        let margin_for_max_lev = margin_required_formula(
            position.get_size(),
            mark_price,
            size_multiplier,
            max_leverage,
        );
        let margin_for_free = margin_required_formula(
            position.get_size(),
            mark_price,
            size_multiplier,
            free_collateral_max_leverage,
        );
        let notional = (position.get_size() as u128 * mark_price as u128 / size_multiplier as u128) as u64;

        match self {
            PositionStatus::V1 {
                unrealized_pnl,
                haircutted_upnl,
                margin_for_max_leverage,
                margin_for_free_collateral,
                total_notional_value,
            } => {
                *unrealized_pnl -= pnl;
                *haircutted_upnl -= pnl_haircutted;
                *margin_for_max_leverage -= margin_for_max_lev;
                *margin_for_free_collateral -= margin_for_free;
                *total_notional_value -= notional;
            },
        }
    }
}

fn apply_upnl_haircut(pnl: i64, haircut_bps: u64) -> i64 {
    if pnl > 0 {
        i64_math::mul_div(pnl, haircut_bps, 10000).expect("haircut calc overflow")
    } else {
        0
    }
}

// ===================== PositionAndCollateralStatus =====================

impl PositionAndCollateralStatus {
    pub fn get_account_equity(&self) -> i64 {
        match self {
            PositionAndCollateralStatus::V1 {
                position_status,
                collateral_status,
            } => position_status.get_unrealized_pnl() + collateral_status.get_total_balance_from_status(),
        }
    }

    pub fn get_position_status(&self) -> &PositionStatus {
        match self {
            PositionAndCollateralStatus::V1 { position_status, .. } => position_status,
        }
    }

    pub fn get_collateral_status(&self) -> &CollateralStatus {
        match self {
            PositionAndCollateralStatus::V1 {
                collateral_status, ..
            } => collateral_status,
        }
    }

    pub fn unpack(self) -> (CollateralStatus, PositionStatus) {
        match self {
            PositionAndCollateralStatus::V1 {
                collateral_status,
                position_status,
            } => (collateral_status, position_status),
        }
    }

    pub fn is_account_liquidatable_from_combined_status(
        &self,
        liquidation_config: &LiquidationConfig,
        backstop_liquidation: bool,
    ) -> bool {
        match self {
            PositionAndCollateralStatus::V1 {
                collateral_status,
                position_status,
            } => {
                let liquidation_margin = liquidation_config.get_liquidation_margin(
                    position_status.get_margin_for_max_leverage(),
                    backstop_liquidation,
                );
                collateral_status.get_total_balance_from_status()
                    + position_status.get_unrealized_pnl()
                    < (liquidation_margin as i64)
            },
        }
    }

    pub fn free_collateral_from_cross_status(
        &self,
        carry_over_margin: i64,
        deduct_reserved_collateral: bool,
        exclude_unrealized_profit: bool,
    ) -> u64 {
        match self {
            PositionAndCollateralStatus::V1 {
                collateral_status,
                position_status,
            } => {
                let total_balance = collateral_status.get_total_balance_from_status();
                let upnl = position_status.get_unrealized_pnl();
                let haircutted = position_status.get_haircutted_upnl();
                let margin_fc = position_status.get_margin_for_free_collateral() as i64;

                let mut free_collateral = if exclude_unrealized_profit {
                    if upnl > 0 {
                        total_balance - margin_fc - carry_over_margin
                    } else {
                        total_balance + upnl - margin_fc - carry_over_margin
                    }
                } else {
                    total_balance + upnl - std::cmp::max(haircutted, margin_fc)
                        - carry_over_margin
                };

                if deduct_reserved_collateral {
                    let reserved = collateral_status.get_reserved_balance_from_status() as i64;
                    if reserved > 0 {
                        free_collateral -= reserved;
                        free_collateral = std::cmp::min(
                            free_collateral,
                            total_balance - margin_fc - reserved - carry_over_margin,
                        );
                    }
                }

                if free_collateral > 0 {
                    free_collateral as u64
                } else {
                    0
                }
            },
        }
    }

    pub fn add_liquidation_details(
        self,
        liquidation_config: &LiquidationConfig,
    ) -> AccountStatusDetailed {
        match self {
            PositionAndCollateralStatus::V1 {
                collateral_status,
                position_status,
            } => {
                let PositionStatus::V1 {
                    unrealized_pnl,
                    margin_for_max_leverage,
                    margin_for_free_collateral,
                    total_notional_value,
                    ..
                } = position_status;

                AccountStatusDetailed::V1 {
                    account_equity: collateral_status.get_total_balance_from_status()
                        + unrealized_pnl,
                    primary_collateral_balance: collateral_status
                        .get_primary_balance_from_status(),
                    secondary_collateral_balance: collateral_status
                        .get_secondary_balance_from_status(),
                    reserved_collateral_balance: collateral_status
                        .get_reserved_balance_from_status(),
                    margin_for_max_leverage,
                    margin_for_free_collateral,
                    liquidation_margin: liquidation_config
                        .get_liquidation_margin(margin_for_max_leverage, false),
                    backstop_liquidator_margin: liquidation_config
                        .get_liquidation_margin(margin_for_max_leverage, true),
                    liquidation_margin_multiplier: liquidation_config
                        .maintenance_margin_leverage_multiplier(),
                    liquidation_margin_divisor: liquidation_config
                        .maintenance_margin_leverage_divisor(),
                    backstop_liquidation_margin_multiplier: liquidation_config
                        .backstop_margin_maintenance_multiplier(),
                    backstop_liquidation_margin_divisor: liquidation_config
                        .backstop_margin_maintenance_divisor(),
                    total_notional_value,
                }
            },
        }
    }
}

// ===================== AccountStatusDetailed Accessors =====================

impl AccountStatusDetailed {
    pub fn get_account_equity(&self) -> i64 {
        match self {
            AccountStatusDetailed::V1 { account_equity, .. } => *account_equity,
        }
    }

    pub fn get_liquidation_margin(&self) -> u64 {
        match self {
            AccountStatusDetailed::V1 {
                liquidation_margin, ..
            } => *liquidation_margin,
        }
    }

    pub fn is_account_liquidatable_detailed(&self, backstop_liquidation: bool) -> bool {
        match self {
            AccountStatusDetailed::V1 {
                account_equity,
                liquidation_margin,
                backstop_liquidator_margin,
                ..
            } => {
                if backstop_liquidation {
                    *account_equity < (*backstop_liquidator_margin as i64)
                } else {
                    *account_equity < (*liquidation_margin as i64)
                }
            },
        }
    }
}

// ===================== Position Update =====================

pub fn update_single_position_struct(
    position: &mut PerpPosition,
    settle_price: u64,
    is_long: bool,
    size: u64,
    unrealized_funding_amount_before_last_update: i64,
    updated_funding_index: AccumulativeIndex,
    cur_time: u64,
) {
    match position {
        PerpPosition::V1 {
            size: pos_size,
            entry_px_times_size_sum,
            avg_acquire_entry_px,
            is_long: pos_is_long,
            funding_index_at_last_update,
            unrealized_funding_amount_before_last_update: pos_funding,
            timestamp,
            ..
        } => {
            if *pos_is_long != is_long {
                if *pos_size >= size {
                    let new_size = *pos_size - size;
                    *entry_px_times_size_sum = math::mul_div_direction_128(
                        *entry_px_times_size_sum,
                        new_size as u128,
                        *pos_size as u128,
                        *pos_is_long,
                    ).expect("position update overflow");
                    *pos_size = new_size;
                } else {
                    *pos_size = size - *pos_size;
                    *entry_px_times_size_sum = (settle_price as u128) * (*pos_size as u128);
                    *avg_acquire_entry_px = settle_price;
                    *pos_is_long = is_long;
                }
            } else {
                let price_size = (settle_price as u128) * (size as u128) + *entry_px_times_size_sum;
                *pos_size = size + *pos_size;
                *avg_acquire_entry_px = (price_size / (*pos_size as u128)) as u64;
                *entry_px_times_size_sum = price_size;
            }
            *pos_funding = unrealized_funding_amount_before_last_update;
            *funding_index_at_last_update = updated_funding_index;
            *timestamp = cur_time;
        },
    }
}

// ===================== PerpPositionWithMarket Accessors =====================

impl PerpPositionWithMarket {
    pub fn get_market(&self) -> PerpMarketRef {
        match self {
            PerpPositionWithMarket::V1 { market, .. } => *market,
        }
    }

    pub fn get_perp_position(&self) -> &PerpPosition {
        match self {
            PerpPositionWithMarket::V1 { position, .. } => position,
        }
    }
}


// ===================== Stub functions for perp_engine delegation =====================

pub fn assert_user_initialized(_user: [u8; 32]) -> Result<(), u64> {
    Ok(())
}

pub fn init_user_if_new(_account: [u8; 32], _fee_tracking_addr: [u8; 32]) {
    // Dispatch layer handles resource initialization
}

pub fn configure_user_settings_for_market(
    _account: [u8; 32], _market: [u8; 32], _is_cross: bool, _user_leverage: u8,
) -> Result<(), u64> {
    Ok(())
}

pub fn get_position_size(_account: [u8; 32], _market: [u8; 32]) -> u64 {
    0
}

pub fn get_position_is_long(_account: [u8; 32], _market: [u8; 32]) -> bool {
    true
}

pub fn update_account_status_cache_on_market_state_change(
    _market: [u8; 32],
    _old_market_state: crate::native_perpdex::price_management::MarketState,
    _new_market_state: crate::native_perpdex::price_management::MarketState,
) {
    // Dispatch layer updates the cache
}

pub fn init_account_status_cache(_account_addr: [u8; 32]) {
    // Dispatch layer handles
}

pub fn list_positions(_account: [u8; 32]) -> Vec<crate::native_perpdex::position_view_types::PositionViewInfo> {
    // Dispatch layer resolves PerpPositions resource and returns position views
    Vec::new()
}

pub fn view_position(
    _account: [u8; 32], _market: [u8; 32],
) -> Option<crate::native_perpdex::position_view_types::PositionViewInfo> {
    // Dispatch layer resolves PerpPositions resource
    None
}

pub fn is_position_isolated(_account: [u8; 32], _market: [u8; 32]) -> bool {
    // Dispatch layer resolves PerpPositions resource
    false
}

pub fn has_position(_account: [u8; 32], _market: [u8; 32]) -> bool {
    // Dispatch layer resolves PerpPositions resource
    false
}

pub fn get_position_entry_px_times_size_sum(_account: [u8; 32], _market: [u8; 32]) -> u128 {
    // Dispatch layer resolves PerpPositions resource
    0
}

pub fn get_position_unrealized_funding_cost(_account: [u8; 32], _market: [u8; 32]) -> i64 {
    // Dispatch layer resolves PerpPositions resource
    0
}

pub fn get_position_funding_index_at_last_update(
    _account: [u8; 32], _market: [u8; 32],
) -> crate::native_perpdex::price_management::AccumulativeIndex {
    // Dispatch layer resolves PerpPositions resource
    crate::native_perpdex::price_management::AccumulativeIndex { index: 0 }
}

pub fn get_position_unrealized_funding_amount_before_last_update(
    _account: [u8; 32], _market: [u8; 32],
) -> i64 {
    // Dispatch layer resolves PerpPositions resource
    0
}
