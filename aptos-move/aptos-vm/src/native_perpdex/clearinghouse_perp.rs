// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::clearinghouse_perp

use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const EINVALID_SIZE_IS_ZERO: u64 = 1;
const EINVALID_PRICE_IS_ZERO: u64 = 2;
const EINVALID_SETTLE_RESULT: u64 = 3;
const ESELF_TRADE_NOT_ALLOWED: u64 = 4;
const ENOT_REDUCE_ONLY: u64 = 5;

/// Maximum value for i64; used as a sentinel for unlimited OI delta.
const MAX_I64: u64 = i64::MAX as u64;

// ===================== Types =====================

/// Result of a trade settlement attempt
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettleTradeInput {
    pub taker: [u8; 32],
    pub maker: [u8; 32],
    pub taker_order_id: u128,
    pub maker_order_id: u128,
    pub taker_client_order_id: Option<String>,
    pub maker_client_order_id: Option<String>,
    pub is_taker_long: bool,
    pub settled_price: u64,
    pub size: u64,
    pub maker_limit_price: u64,
    pub fill_id: u128,
}

/// Simplified result of settling a trade
#[derive(Clone, Debug)]
pub struct SettleTradeOutput {
    pub settled_size: u64,
    pub maker_cancellation_reason: Option<String>,
    pub taker_cancellation_reason: Option<String>,
    pub should_stop_matching: bool,
}

/// Result of a backstop liquidation or ADL settlement
#[derive(Clone, Debug)]
pub struct BackstopSettlementOutput {
    pub filled_size: Option<u64>,
    pub maker_covered_loss: u64,
}

// ===================== Helper Functions =====================

/// Compute the open interest delta for a trade and check against the OI cap.
///
/// Returns (adjusted_size, open_interest_delta, max_oi_reached)
///
/// The open interest delta is the sum of the taker and maker long OI deltas.
/// If the trade would increase OI beyond the cap, the size is reduced.
/// The backstop liquidator is exempt from OI cap enforcement.
///
/// In Move, this calls perp_positions::get_open_interest_delta_for_long for both sides.
/// In native context, those calls are made via the dispatch layer.
pub fn get_adjusted_size_for_open_interest_cap(
    taker: [u8; 32],
    maker: [u8; 32],
    market: [u8; 32],
    _is_taker_long: bool,
    size: u64,
    max_open_interest_delta: u64,
    taker_long_oi_delta: i64,
    maker_long_oi_delta: i64,
    is_backstop_liquidator: bool,
) -> (u64, i64, bool) {
    let open_interest_delta = taker_long_oi_delta + maker_long_oi_delta;
    let _ = (taker, maker, market);

    // Backstop liquidator exempt from OI cap
    if is_backstop_liquidator {
        return (size, open_interest_delta, false);
    }

    if size < max_open_interest_delta {
        // If trade size < max OI delta, no way to exceed the cap
        (size, open_interest_delta, false)
    } else {
        if open_interest_delta > (max_open_interest_delta as i64) {
            // Need to reduce size to stay within OI cap
            let size_adjustment_needed = (open_interest_delta as u64) - max_open_interest_delta;
            if size_adjustment_needed >= size {
                // Trade would exceed cap entirely, abort
                return (0, 0, true);
            }
            (size - size_adjustment_needed, max_open_interest_delta as i64, true)
        } else {
            (size, open_interest_delta, false)
        }
    }
}

/// Validate order placement.
/// Returns None if valid, Some(reason) if invalid.
///
/// In native context, the actual validation is done by accounts_collateral
/// and pending_order_tracker modules. This function provides the orchestration.
///
/// From Move:
/// 1. Check can_place_order (market mode check)
/// 2. If backstop liquidator, skip all checks
/// 3. Check TP/SL limits
/// 4. If IOC, skip margin checks
/// 5. If reduce-only, validate via order_margin::validate_reduce_only_order
/// 6. Otherwise, validate via accounts_collateral::validate_order_placement
pub fn validate_order_placement(
    market: [u8; 32],
    account: [u8; 32],
    is_long: bool,
    limit_price: u64,
    size: u64,
    _is_ioc: bool,
    _is_reduce_only: bool,
    can_place_order: bool,
    is_backstop_liquidator: bool,
) -> Option<String> {
    let _ = (market, account, is_long, limit_price, size);

    if !can_place_order {
        return Some("Market is halted".to_string());
    }
    if is_backstop_liquidator {
        return None; // No validation for backstop liquidator
    }

    // RESOURCE_READ: pending_order_tracker for TP/SL validation
    // RESOURCE_READ: accounts_collateral for margin validation
    // RESOURCE_READ: order_margin for reduce-only validation

    // In native context, the dispatch layer performs the full validation:
    // - perp_market_config::can_place_order check
    // - TP/SL count limits
    // - IOC exemption from margin checks
    // - Reduce-only order validation
    // - Regular order margin validation
    None // DISPATCH_LAYER: full validation
}

/// Settle a backstop liquidation or ADL.
/// Returns BackstopSettlementOutput with (filled_size, maker_covered_loss).
///
/// From Move:
/// 1. Basic assertions (size > 0, price > 0, taker != maker)
/// 2. Adjust size for OI cap (backstop liquidator exempt)
/// 3. Validate both taker and maker via accounts_collateral::validate_backstop_liquidation_or_adl_update
/// 4. Commit position updates for both sides
/// 5. Update pending_order_tracker for non-backstop accounts
/// 6. Track open interest delta
pub fn settle_backstop_liquidation_or_adl(
    taker: [u8; 32],
    maker: [u8; 32],
    market: [u8; 32],
    is_taker_long: bool,
    price: u64,
    size: u64,
    is_adl: bool,
) -> Result<BackstopSettlementOutput, u64> {
    // Basic assertions
    if size == 0 {
        return Err(EINVALID_SIZE_IS_ZERO);
    }
    if price == 0 {
        return Err(EINVALID_PRICE_IS_ZERO);
    }
    if taker == maker {
        return Err(ESELF_TRADE_NOT_ALLOWED);
    }

    let _ = (market, is_taker_long, is_adl);

    // RESOURCE_READ: accounts_collateral::backstop_liquidator()
    // let backstop_liquidator = accounts_collateral::backstop_liquidator();

    // Step 1: Adjust size for OI cap (with MAX_I64 as max_open_interest_delta)
    // In Move:
    //   let (size, open_interest_delta, _) = get_adjusted_size_for_open_interest_cap(
    //       taker, maker, market, is_taker_long, size, MAX_I64 as u64
    //   );
    // The dispatch layer reads perp_positions::get_open_interest_delta_for_long for both sides
    // and computes the adjusted size.

    // Step 2: Validate taker update
    // RESOURCE_READ/WRITE: accounts_collateral::validate_backstop_liquidation_or_adl_update(
    //     taker, market, price, is_taker_long, true, size)
    // Assert taker validation is successful

    // Step 3: Validate maker update
    // RESOURCE_READ/WRITE: accounts_collateral::validate_backstop_liquidation_or_adl_update(
    //     maker, market, price, !is_taker_long, false, size)
    // If maker validation fails, return (None, 0)

    // Step 4: Commit taker position update
    // RESOURCE_WRITE: accounts_collateral::commit_update_position_with_backstop_liquidator(
    //     price, is_taker_long, size, taker_validation_result, backstop_liquidator,
    //     trade_trigger_source_adl/backstop_liquidation)
    // Returns (taker_size, taker_is_long, taker_leverage, _)

    // Step 5: Update pending_order_tracker for taker (if not backstop)
    // RESOURCE_WRITE: pending_order_tracker::update_position(taker, market, taker_size, taker_is_long, taker_leverage)

    // Step 6: Commit maker position update
    // RESOURCE_WRITE: accounts_collateral::commit_update_position_with_backstop_liquidator(
    //     price, !is_taker_long, size, maker_validation_result, backstop_liquidator,
    //     trade_trigger_source_adl/backstop_liquidation)
    // Returns (maker_size, maker_is_long, maker_leverage, maker_covered_loss)

    // Step 7: Update pending_order_tracker for maker (if not backstop)
    // RESOURCE_WRITE: pending_order_tracker::update_position(maker, market, maker_size, maker_is_long, maker_leverage)

    // Step 8: Update open interest tracker
    // RESOURCE_WRITE: open_interest_tracker::mark_open_interest_delta_for_market(market, open_interest_delta)

    Ok(BackstopSettlementOutput {
        filled_size: Some(size),
        maker_covered_loss: 0, // DISPATCH_LAYER: actual maker_covered_loss from step 6
    })
}

/// Settle a trade between taker and maker.
///
/// This is the core settlement function. In Move, it:
/// 1. Assert taker != maker, size > 0, price > 0
/// 2. Check can_settle_order (market mode)
/// 3. Check reduce-only constraints for both sides
/// 4. Adjust size for OI cap
/// 5. Validate position updates for both taker and maker
/// 6. Commit position updates (apply PnL, fees, margin changes)
/// 7. Update pending_order_tracker for maker
/// 8. Track backstop liquidator position changes
/// 9. Distribute fees
/// 10. Cancel reduce-only orders if positions closed
/// 11. Place child TP/SL orders
/// 12. Update open interest tracker
pub fn settle_trade(input: SettleTradeInput) -> Result<SettleTradeOutput, u64> {
    // Step 1: Basic assertions
    if input.taker == input.maker {
        return Err(ESELF_TRADE_NOT_ALLOWED);
    }
    if input.size == 0 {
        return Err(EINVALID_SIZE_IS_ZERO);
    }
    if input.settled_price == 0 {
        return Err(EINVALID_PRICE_IS_ZERO);
    }

    // Step 2: Check market mode
    // RESOURCE_READ: perp_market_config::can_settle_order(market, maker, taker)
    // If halted, return SettleTradeOutput with both cancellation reasons = "Market is halted", size = 0

    // Step 3: Check reduce-only constraints
    // For each side, call get_settlement_size_and_reason which checks:
    //   - perp_engine_types::is_reduce_only(metadata)
    //   - perp_market_config::is_reduce_only(market, account)
    //   - accounts_collateral::validate_reduce_only_update
    // Returns (max_settlement_size, taker_cancellation_reason, maker_cancellation_reason)
    // If either side has a reduce-only violation, return early with 0 size

    // Step 4: Adjust for OI cap
    // RESOURCE_READ: open_interest_tracker::get_max_open_interest_delta_for_market(market)
    // Call get_adjusted_size_for_open_interest_cap with the reduce-only-adjusted size
    // If adjusted size = 0, return with "Max open interest violation"

    // Step 5: Validate position updates
    // For taker:
    //   RESOURCE_READ: accounts_collateral::validate_position_update_for_settlement(
    //       taker, market, settled_price, is_taker_long, true, adjusted_size,
    //       taker_builder_code, use_backstop_margin, is_margin_call)
    // For maker:
    //   RESOURCE_READ: accounts_collateral::validate_position_update_for_settlement(
    //       maker, market, settled_price, !is_taker_long, false, adjusted_size,
    //       maker_builder_code, use_backstop_margin, is_margin_call)
    // If either fails, return with the failure reason

    // Step 6: Commit position updates
    // RESOURCE_WRITE: accounts_collateral::commit_update_position for taker
    //   (order_id, client_order_id, price, is_long, size, builder_code, validation_result, fill_id, trigger_source)
    // RESOURCE_WRITE: accounts_collateral::commit_update_position for maker
    //   Returns (maker_size, maker_is_long, maker_leverage)

    // Step 7: Update pending_order_tracker for maker
    // RESOURCE_WRITE: pending_order_tracker::remove_pending_order or remove_bulk_order
    //   depending on maker_order_type

    // Step 8: Track backstop liquidator position changes
    // If taker or maker is backstop_liquidator:
    //   RESOURCE_WRITE: backstop_liquidator_profit_tracker::handle_regular_trade

    // Step 9: Distribute fees
    // RESOURCE_WRITE: accounts_collateral::distribute_fees(taker_fee, maker_fee, market)

    // Step 10: Cancel reduce-only orders if positions closed/flipped
    // If taker position is closed: get reduce_only_orders and create cancel actions
    // If maker position is closed: get reduce_only_orders and create cancel actions
    // If any cancel actions exist, should_stop_matching = true

    // Step 11: Place child TP/SL orders
    // For taker: check if metadata has TP/SL, place child orders
    // For maker: check if metadata has TP/SL, place child orders

    // Step 12: Update open interest
    // RESOURCE_WRITE: open_interest_tracker::mark_open_interest_delta_for_market(market, open_interest_delta)

    Ok(SettleTradeOutput {
        settled_size: input.size,  // DISPATCH_LAYER: actual oi_adjusted_settlement_size
        maker_cancellation_reason: None,  // DISPATCH_LAYER: from validation
        taker_cancellation_reason: None,  // DISPATCH_LAYER: from validation
        should_stop_matching: false,  // DISPATCH_LAYER: true if cancel actions exist
    })
}

/// Place a maker order on the book.
/// Returns callback actions (e.g., reduce-only order management).
///
/// From Move:
/// 1. Skip if backstop_liquidator or IOC
/// 2. Track TP/SL orders via pending_order_tracker
/// 3. For reduce-only: call order_margin::add_reduce_only_order (returns cancel actions)
/// 4. For regular: call accounts_collateral::add_pending_order
pub fn place_maker_order(
    market: [u8; 32],
    account: [u8; 32],
    order_id: u128,
    limit_price: u64,
    is_long: bool,
    size: u64,
    is_ioc: bool,
    is_reduce_only: bool,
    is_backstop_liquidator: bool,
) -> Vec<super::order_placement_utils::CallbackAction> {
    let _ = (market, account, order_id, limit_price, is_long, size);

    // Skip for backstop liquidator and IOC orders
    if is_backstop_liquidator || is_ioc {
        return Vec::new();
    }

    // Step 1: Track TP/SL orders if present (from order metadata)
    // RESOURCE_WRITE: pending_order_tracker::add_order_based_tp_sl if metadata has TP or SL

    // Step 2: Handle reduce-only vs regular orders
    if is_reduce_only {
        // RESOURCE_WRITE: order_margin::add_reduce_only_order(account, market, order_id, size, is_long)
        // Returns Vec<SingleOrderAction> which may contain cancel actions for excess reduce-only orders
        // These are converted to CallbackAction::CancelOrder
        Vec::new() // DISPATCH_LAYER: actual actions from add_reduce_only_order
    } else {
        // RESOURCE_WRITE: accounts_collateral::add_pending_order(account, market, order_id, size, is_long, limit_price)
        Vec::new()
    }
}

/// Clean up an order after it's cancelled or expired.
///
/// From Move:
/// 1. Skip if taker, IOC, or has trigger condition
/// 2. Remove TP/SL tracking if metadata has TP/SL
/// 3. If cleanup_size > 0 and not backstop: remove pending order from tracker
pub fn cleanup_order(
    market: [u8; 32],
    account: [u8; 32],
    order_id: u128,
    limit_price: u64,
    cleanup_size: u64,
    is_long: bool,
    is_taker: bool,
    is_ioc: bool,
    has_trigger_condition: bool,
    is_reduce_only: bool,
    is_backstop_liquidator: bool,
) {
    let _ = (market, account, order_id, limit_price, is_long, is_reduce_only, is_backstop_liquidator);

    // Skip cleanup for takers, IOC orders, and triggered orders
    if is_taker || is_ioc || has_trigger_condition {
        return;
    }

    // Step 1: Remove TP/SL tracking
    // RESOURCE_WRITE: pending_order_tracker::remove_order_based_tp_sl if metadata has TP/SL

    // Step 2: Remove pending order from tracker
    if cleanup_size > 0 && !is_backstop_liquidator {
        // RESOURCE_READ: perp_positions::get_position_details_or_default(account, market)
        //   -> (position_size, is_position_long, user_leverage)
        // RESOURCE_WRITE: pending_order_tracker::remove_pending_order(
        //     account, market, order_id, cleanup_size, limit_price, is_long,
        //     is_reduce_only, position_size, is_position_long, user_leverage)
    }
}

/// Reduce order size (for reduce-only orders only).
pub fn reduce_order_size(
    market: [u8; 32],
    account: [u8; 32],
    order_id: u128,
    new_size: u64,
    is_reduce_only: bool,
) -> Result<(), u64> {
    if !is_reduce_only {
        return Err(ENOT_REDUCE_ONLY);
    }
    let _ = (market, account, order_id, new_size);

    // RESOURCE_WRITE: pending_order_tracker::decrease_reduce_only_order_size(
    //     account, market, order_id, new_size)
    Ok(())
}


// ===================== Stub functions for perp_engine delegation =====================

/// Close a delisted position.
/// From Move:
/// 1. Get position size and direction
/// 2. If size == 0, return
/// 3. Get mark price
/// 4. Validate backstop liquidation update
/// 5. Assert successful
/// 6. Commit update with backstop liquidator
pub fn close_delisted_position(address: [u8; 32], market: [u8; 32]) -> Result<(), u64> {
    let _ = (address, market);

    // RESOURCE_READ: perp_positions::get_position_size_and_is_long(address, market)
    //   -> (size, is_long)
    // If size == 0, return Ok(())

    // RESOURCE_READ: price_management::get_mark_price(market) -> mark_px

    // RESOURCE_READ: accounts_collateral::validate_backstop_liquidation_or_adl_update(
    //     address, market, mark_px, !is_long, false, size)
    // Assert validation_result.is_update_successful()

    // RESOURCE_WRITE: accounts_collateral::commit_update_position_with_backstop_liquidator(
    //     mark_px, !is_long, size, validation_result,
    //     accounts_collateral::backstop_liquidator(),
    //     perp_positions::new_trade_trigger_source_market_delisted())

    Ok(())
}
