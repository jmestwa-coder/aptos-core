// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::position_update
//
// Position state updates and calculations - validates position changes
// and computes PnL, fees, margin requirements.

// Collateral types used by callers
use crate::native_perpdex::i64_math;
use crate::native_perpdex::liquidation_config::LiquidationConfig;
use crate::native_perpdex::math;
use crate::native_perpdex::perp_positions::{PerpPosition, margin_required_formula};
use crate::native_perpdex::price_management::AccumulativeIndex;

// ===================== Constants =====================

const EINVALID_UPDATE_RESULT: u64 = 1;
const EPOSTION_LIQUIDATABLE: u64 = 2;
const EINVALID_SIZE: u64 = 3;
const EPOSTION_INSUFFICIENT_MARGIN: u64 = 4;
const EPOSTION_NOT_SAME_DIRECTION: u64 = 5;
const EINCORRECT_FLIP_PNL: u64 = 6;
const EPOSTION_BECOMES_LIQUIDATABLE: u64 = 7;
const EINVALID_LEVERAGE: u64 = 8;
const EMARGIN_CALL_ERROR: u64 = 9;
const EINSUFFICIENT_MARGIN_FOR_FEE: u64 = 10;

// ===================== Types =====================

pub type PerpMarketRef = [u8; 32];

/// Fee distribution placeholder - in the full system this tracks fee breakdowns
#[derive(Clone, Debug, Copy)]
pub struct FeeDistribution {
    pub position_fee_delta: i64,
}

impl FeeDistribution {
    pub fn zero() -> Self {
        FeeDistribution {
            position_fee_delta: 0,
        }
    }

    pub fn get_position_fee_delta(&self) -> i64 {
        self.position_fee_delta
    }

    pub fn add(self, other: FeeDistribution) -> FeeDistribution {
        FeeDistribution {
            position_fee_delta: self.position_fee_delta + other.position_fee_delta,
        }
    }
}

#[derive(Clone, Debug)]
pub enum UpdatePositionResult {
    Success {
        account: [u8; 32],
        market: PerpMarketRef,
        is_isolated: bool,
        margin_delta: Option<i64>,
        backstop_liquidator_covered_loss: u64,
        fee_distribution: FeeDistribution,
        realized_pnl: Option<i64>,
        realized_funding_cost: Option<i64>,
        unrealized_funding_cost: i64,
        updated_funding_index: AccumulativeIndex,
        volume_delta: u128,
        is_taker: bool,
        is_position_closed_or_flipped: bool,
    },
    Liquidatable,
    InsufficientMargin,
    InvalidLeverage,
    BecomesLiquidatable,
    InsufficientMarginForFee,
}

#[derive(Clone, Debug)]
pub enum ReduceOnlyValidationResult {
    ReduceOnlyViolation,
    Success { size: u64 },
}

// ===================== Functions =====================

pub fn is_reduce_only_violation(result: &ReduceOnlyValidationResult) -> bool {
    matches!(result, ReduceOnlyValidationResult::ReduceOnlyViolation)
}

pub fn get_reduce_only_size(result: &ReduceOnlyValidationResult) -> u64 {
    match result {
        ReduceOnlyValidationResult::Success { size } => *size,
        _ => panic!("Not a success result"),
    }
}

pub fn is_update_successful(result: &UpdatePositionResult) -> bool {
    matches!(result, UpdatePositionResult::Success { .. })
}

pub fn unwrap_is_closed_or_flipped(result: &UpdatePositionResult) -> bool {
    match result {
        UpdatePositionResult::Success {
            is_position_closed_or_flipped,
            ..
        } => *is_position_closed_or_flipped,
        _ => panic!("Not a success result"),
    }
}

pub fn unwrap_failed_update_reason(result: &UpdatePositionResult) -> String {
    match result {
        UpdatePositionResult::Liquidatable => {
            "Existing position is liquidatable".to_string()
        },
        UpdatePositionResult::InsufficientMargin => {
            "Insufficient margin to update position".to_string()
        },
        UpdatePositionResult::InvalidLeverage => "User leverage is invalid".to_string(),
        UpdatePositionResult::BecomesLiquidatable => {
            "Existing position becomes liquidatable".to_string()
        },
        UpdatePositionResult::InsufficientMarginForFee => {
            "Fee exceeds allocated margin for isolated position".to_string()
        },
        UpdatePositionResult::Success { .. } => {
            panic!("Result is successful, not failed");
        },
    }
}

pub fn unwrap_fee_distribution(result: &UpdatePositionResult) -> FeeDistribution {
    match result {
        UpdatePositionResult::Success {
            fee_distribution, ..
        } => *fee_distribution,
        _ => panic!("Not a success result"),
    }
}

pub fn extract_backstop_liquidator_covered_loss(result: &mut UpdatePositionResult) -> u64 {
    match result {
        UpdatePositionResult::Success {
            backstop_liquidator_covered_loss,
            ..
        } => {
            let loss = *backstop_liquidator_covered_loss;
            *backstop_liquidator_covered_loss = 0;
            loss
        },
        _ => panic!("Not a success result"),
    }
}

/// Validate reduce-only update
pub fn validate_reduce_only_update(
    position: Option<&PerpPosition>,
    is_long: bool,
    size: u64,
) -> ReduceOnlyValidationResult {
    if let Some(pos) = position {
        let position_size = pos.get_size();
        let position_is_long = pos.is_long();
        if position_size == 0 {
            return ReduceOnlyValidationResult::ReduceOnlyViolation;
        }
        if position_is_long == is_long {
            return ReduceOnlyValidationResult::ReduceOnlyViolation;
        }
        if position_size < size {
            return ReduceOnlyValidationResult::Success {
                size: position_size,
            };
        }
        return ReduceOnlyValidationResult::Success { size };
    }
    ReduceOnlyValidationResult::ReduceOnlyViolation
}

/// Get PnL and funding cost for a decrease in position
pub fn get_pnl_and_funding_for_decrease(
    position: &PerpPosition,
    current_px: u64,
    size: u64,
    size_multiplier: u64,
    funding_index: &AccumulativeIndex,
) -> (i64, i64, i64, AccumulativeIndex) {
    let current_px_times_decrease_size = (current_px as u128) * (size as u128);
    let entry_px_times_size_sum = position.get_entry_px_times_size_sum();
    let position_size = position.get_size();
    let position_is_long = position.is_long();

    let entry_px_times_decrease_size = math::mul_div_direction_128(
        entry_px_times_size_sum,
        size as u128,
        position_size as u128,
        position_is_long,
    ).expect("mul_div_direction_128 overflow");

    let is_profit = if position_is_long {
        current_px_times_decrease_size > entry_px_times_decrease_size
    } else {
        current_px_times_decrease_size < entry_px_times_decrease_size
    };

    let pnl = {
        let delta_times_size = if current_px_times_decrease_size > entry_px_times_decrease_size {
            current_px_times_decrease_size - entry_px_times_decrease_size
        } else {
            entry_px_times_decrease_size - current_px_times_decrease_size
        };
        math::div_direction_128(delta_times_size, size_multiplier as u128, !is_profit).expect("div_direction_128 overflow") as i64
    };

    let (total_funding_cost, updated_funding_index) =
        position.get_position_funding_cost_and_index(size_multiplier, funding_index);

    let realized_funding_cost = i64_math::mul_div(total_funding_cost, size, position.get_size()).expect("funding mul_div overflow");
    let remaining_funding_cost = total_funding_cost - realized_funding_cost;

    let net_pnl = i64_math::from_sign_and_amount(is_profit, pnl) - realized_funding_cost;

    (
        net_pnl,
        -realized_funding_cost,
        remaining_funding_cost,
        updated_funding_index,
    )
}

/// Check if settle price is inside the guaranteed range
pub fn is_settle_price_inside_guaranteed_range(
    settle_price: u64,
    mark_price: u64,
    liquidation_config: &LiquidationConfig,
    is_long: bool,
    size: u64,
    fee: i64,
    max_leverage: u8,
    size_multiplier: u64,
) -> bool {
    let max_slippage =
        liquidation_config.get_liquidation_price(mark_price, max_leverage, false);

    let adjusted_max_slippage = if size > 0 && fee > 0 {
        let fee_equivalent_slippage_abs =
            math::ceil_mul_div_64(fee as u64, size_multiplier, size).expect("fee slippage calc overflow");
        if fee_equivalent_slippage_abs > max_slippage {
            return false;
        }
        max_slippage - fee_equivalent_slippage_abs
    } else {
        max_slippage
    };

    if is_long {
        settle_price <= mark_price + adjusted_max_slippage
    } else {
        settle_price >= mark_price.saturating_sub(adjusted_max_slippage)
    }
}
