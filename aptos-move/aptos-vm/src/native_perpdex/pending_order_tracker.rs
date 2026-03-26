// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::pending_order_tracker
//
// This module tracks pending orders per account per market, including:
// - Single order summaries (longs/shorts)
// - Bulk order summaries
// - Reduce-only orders
// - TP/SL requests
// - Optional order ID tracking

use crate::native_perpdex::order_book_types::OrderId;
use crate::native_perpdex::builder_code_registry::BuilderCode;
use crate::native_perpdex::position_tp_sl_tracker::{
    self, PendingOrderTracker as TpSlTracker, PriceIndexKey,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===================== Constants =====================

const EINVALID_ADDRESS: u64 = 1;
const E_MARKET_NOT_FOUND: u64 = 2;
const E_INVALID_ORDER_CLEANUP_SIZE: u64 = 3;
const E_INVALID_REDUCE_ONLY_ORDER: u64 = 4;
const EINVALID_TRIGGER_PRICE: u64 = 5;
const EMAX_FIXED_SIZED_PENDING_REQS_HIT: u64 = 6;
const EINVALID_TP_SL_SIZE: u64 = 7;
const E_ORDER_BASED_TP_SL_COUNT_UNDERFLOW: u64 = 8;
const E_PENDING_ORDERS_EXIST: u64 = 9;
const E_GLOBAL_SUMMARY_NOT_FOUND: u64 = 10;

pub const MAX_REDUCE_ONLY_ORDERS_PER_MARKET: u8 = 10;
const MAX_FIXED_SIZED_PENDING_REQS_PER_POSITION: u64 = 5;
const MAX_ORDER_BASED_TP_SL_PER_MARKET: u64 = 5;

// ===================== Types =====================

pub type PerpMarketRef = [u8; 32];

#[derive(Clone, Debug, Serialize, Deserialize, Copy)]
pub enum PendingOrderSummary {
    V1 {
        price_size_sum: u128,
        size_sum: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Copy)]
pub enum ReduceOnlyOrderInfo {
    V1 { order_id: OrderId, size: u64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ReduceOnlyOrders {
    V1 {
        total_size: u64,
        orders: Vec<ReduceOnlyOrderInfo>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PendingTpSlKey {
    V1 {
        price_index: PriceIndexKey,
        order_id: OrderId,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PendingTpSLs {
    V1 {
        full_sized: Option<PendingTpSlKey>,
        fixed_sized: Vec<PendingTpSlKey>,
        pending_order_based_tp_sl_count: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TrackedOrderInfo {
    V1 {
        market: PerpMarketRef,
        order_id: OrderId,
        size: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PendingMarketState {
    V1 {
        pending_margin: u64,
        pending_single_longs: PendingOrderSummary,
        pending_single_shorts: PendingOrderSummary,
        pending_bulk_longs: PendingOrderSummary,
        pending_bulk_shorts: PendingOrderSummary,
        reduce_only_orders: ReduceOnlyOrders,
        tp_reqs: PendingTpSLs,
        sl_reqs: PendingTpSLs,
        pending_order_ids: Option<BTreeMap<OrderId, u64>>,
    },
}


impl PendingMarketState {
    pub(crate) fn pending_margin(&self) -> u64 {
        let PendingMarketState::V1 { pending_margin, .. } = self;
        *pending_margin
    }

    pub(crate) fn pending_margin_mut(&mut self) -> &mut u64 {
        let PendingMarketState::V1 { pending_margin, .. } = self;
        pending_margin
    }

    pub(crate) fn pending_single_longs(&self) -> &PendingOrderSummary {
        let PendingMarketState::V1 { pending_single_longs, .. } = self;
        pending_single_longs
    }

    pub(crate) fn pending_single_longs_mut(&mut self) -> &mut PendingOrderSummary {
        let PendingMarketState::V1 { pending_single_longs, .. } = self;
        pending_single_longs
    }

    pub(crate) fn pending_single_shorts(&self) -> &PendingOrderSummary {
        let PendingMarketState::V1 { pending_single_shorts, .. } = self;
        pending_single_shorts
    }

    pub(crate) fn pending_single_shorts_mut(&mut self) -> &mut PendingOrderSummary {
        let PendingMarketState::V1 { pending_single_shorts, .. } = self;
        pending_single_shorts
    }

    pub(crate) fn pending_bulk_longs(&self) -> &PendingOrderSummary {
        let PendingMarketState::V1 { pending_bulk_longs, .. } = self;
        pending_bulk_longs
    }

    pub(crate) fn pending_bulk_shorts(&self) -> &PendingOrderSummary {
        let PendingMarketState::V1 { pending_bulk_shorts, .. } = self;
        pending_bulk_shorts
    }

    pub(crate) fn reduce_only_orders(&self) -> &ReduceOnlyOrders {
        let PendingMarketState::V1 { reduce_only_orders, .. } = self;
        reduce_only_orders
    }

    pub(crate) fn reduce_only_orders_mut(&mut self) -> &mut ReduceOnlyOrders {
        let PendingMarketState::V1 { reduce_only_orders, .. } = self;
        reduce_only_orders
    }

    pub(crate) fn tp_reqs_mut(&mut self) -> &mut PendingTpSLs {
        let PendingMarketState::V1 { tp_reqs, .. } = self;
        tp_reqs
    }

    pub(crate) fn sl_reqs_mut(&mut self) -> &mut PendingTpSLs {
        let PendingMarketState::V1 { sl_reqs, .. } = self;
        sl_reqs
    }

    pub(crate) fn pending_order_ids(&self) -> &Option<BTreeMap<OrderId, u64>> {
        let PendingMarketState::V1 { pending_order_ids, .. } = self;
        pending_order_ids
    }

    pub(crate) fn pending_order_ids_mut(&mut self) -> &mut Option<BTreeMap<OrderId, u64>> {
        let PendingMarketState::V1 { pending_order_ids, .. } = self;
        pending_order_ids
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AccountSummary {
    V1 {
        markets: BTreeMap<PerpMarketRef, PendingMarketState>,
        is_order_id_tracking_enabled: bool,
    },
}


impl AccountSummary {
    pub(crate) fn markets(&self) -> &BTreeMap<PerpMarketRef, PendingMarketState> {
        let AccountSummary::V1 { markets, .. } = self;
        markets
    }

    pub(crate) fn markets_mut(&mut self) -> &mut BTreeMap<PerpMarketRef, PendingMarketState> {
        let AccountSummary::V1 { markets, .. } = self;
        markets
    }

    pub(crate) fn is_order_id_tracking_enabled(&self) -> bool {
        let AccountSummary::V1 { is_order_id_tracking_enabled, .. } = self;
        *is_order_id_tracking_enabled
    }

    pub(crate) fn set_order_id_tracking_enabled(&mut self, enabled: bool) {
        let AccountSummary::V1 { is_order_id_tracking_enabled, .. } = self;
        *is_order_id_tracking_enabled = enabled;
    }
}

/// Global summary - in Move this is a resource at @decibel_dex.
/// In Rust we pass it as a parameter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GlobalSummary {
    V1 {
        summary: BTreeMap<[u8; 32], AccountSummary>, // address -> AccountSummary
    },
}

#[derive(Clone, Debug)]
pub enum PendingTpSlInfo {
    V1 {
        order_id: OrderId,
        trigger_price: u64,
        account: [u8; 32],
        limit_price: Option<u64>,
        size: Option<u64>,
    },
}

// EVENT types
#[derive(Clone, Debug)]
pub enum FullSizedTpSlForEvent {
    V1 {
        order_id: u128,
        trigger_price: u64,
        limit_price: Option<u64>,
    },
}

#[derive(Clone, Debug)]
pub enum FixedSizedTpSlForEvent {
    V1 {
        order_id: u128,
        trigger_price: u64,
        limit_price: Option<u64>,
        size: u64,
    },
}

// ===================== Helper constructors =====================

fn new_reduce_only_orders() -> ReduceOnlyOrders {
    ReduceOnlyOrders::V1 {
        total_size: 0,
        orders: Vec::new(),
    }
}

fn new_pending_tp_sls() -> PendingTpSLs {
    PendingTpSLs::V1 {
        full_sized: None,
        fixed_sized: Vec::new(),
        pending_order_based_tp_sl_count: 0,
    }
}

pub fn new_market_state() -> PendingMarketState {
    PendingMarketState::V1 {
        pending_margin: 0,
        pending_single_longs: PendingOrderSummary::V1 {
            price_size_sum: 0,
            size_sum: 0,
        },
        pending_single_shorts: PendingOrderSummary::V1 {
            price_size_sum: 0,
            size_sum: 0,
        },
        pending_bulk_longs: PendingOrderSummary::V1 {
            price_size_sum: 0,
            size_sum: 0,
        },
        pending_bulk_shorts: PendingOrderSummary::V1 {
            price_size_sum: 0,
            size_sum: 0,
        },
        reduce_only_orders: new_reduce_only_orders(),
        tp_reqs: new_pending_tp_sls(),
        sl_reqs: new_pending_tp_sls(),
        pending_order_ids: None,
    }
}

impl GlobalSummary {
    pub fn new() -> Self {
        GlobalSummary::V1 {
            summary: BTreeMap::new(),
        }
    }

    pub(crate) fn summary(&self) -> &BTreeMap<[u8; 32], AccountSummary> {
        let GlobalSummary::V1 { summary } = self;
        summary
    }

    pub(crate) fn summary_mut(&mut self) -> &mut BTreeMap<[u8; 32], AccountSummary> {
        let GlobalSummary::V1 { summary } = self;
        summary
    }

    pub fn initialize_account_summary(&mut self, account: [u8; 32]) {
        if !self.summary().contains_key(&account) {
            self.summary_mut().insert(
                account,
                AccountSummary::V1 {
                    markets: BTreeMap::new(),
                    is_order_id_tracking_enabled: false,
                },
            );
        }
    }

    pub fn get_pending_order_margin(&self, account: [u8; 32]) -> u64 {
        if let Some(account_summary) = self.summary().get(&account) {
            let mut total = 0u64;
            for (_market, state) in account_summary.markets() {
                total += state.pending_margin();
            }
            total
        } else {
            0
        }
    }

    pub fn get_pending_order_margin_for_market(
        &self,
        account: [u8; 32],
        market: PerpMarketRef,
    ) -> u64 {
        if let Some(account_summary) = self.summary().get(&account) {
            if let Some(state) = account_summary.markets().get(&market) {
                return state.pending_margin();
            }
        }
        0
    }

    pub fn bulk_order_using_margin(&self, account: [u8; 32], market: PerpMarketRef) -> bool {
        if let Some(account_summary) = self.summary().get(&account) {
            if let Some(market_state) = account_summary.markets().get(&market) {
                let PendingOrderSummary::V1 {
                    size_sum: bulk_long_size,
                    ..
                } = *market_state.pending_bulk_longs();
                let PendingOrderSummary::V1 {
                    size_sum: bulk_short_size,
                    ..
                } = *market_state.pending_bulk_shorts();
                return bulk_long_size > 0 || bulk_short_size > 0;
            }
        }
        false
    }

    pub fn get_bulk_order_pending_sizes(
        &self,
        account: [u8; 32],
        market: PerpMarketRef,
    ) -> (u64, u64) {
        if let Some(account_summary) = self.summary().get(&account) {
            if let Some(market_state) = account_summary.markets().get(&market) {
                let PendingOrderSummary::V1 {
                    size_sum: bulk_long_size,
                    ..
                } = *market_state.pending_bulk_longs();
                let PendingOrderSummary::V1 {
                    size_sum: bulk_short_size,
                    ..
                } = *market_state.pending_bulk_shorts();
                return (bulk_long_size, bulk_short_size);
            }
        }
        (0, 0)
    }

    pub fn has_any_pending_orders(&self, account: [u8; 32]) -> bool {
        if let Some(account_summary) = self.summary().get(&account) {
            for (_market, state) in account_summary.markets() {
                let PendingOrderSummary::V1 {
                    size_sum: sl_size, ..
                } = *state.pending_single_longs();
                let PendingOrderSummary::V1 {
                    size_sum: ss_size, ..
                } = *state.pending_single_shorts();
                let PendingOrderSummary::V1 {
                    size_sum: bl_size, ..
                } = *state.pending_bulk_longs();
                let PendingOrderSummary::V1 {
                    size_sum: bs_size, ..
                } = *state.pending_bulk_shorts();
                let ReduceOnlyOrders::V1 {
                    total_size: ro_size,
                    ..
                } = state.reduce_only_orders();
                if sl_size > 0 || ss_size > 0 || bl_size > 0 || bs_size > 0 || *ro_size > 0 {
                    return true;
                }
            }
        }
        false
    }
}

// ===================== Margin calculation helpers =====================

fn pending_price_size_for_market(
    position_size: u64,
    position_is_long: bool,
    pending_single_longs: &PendingOrderSummary,
    pending_single_shorts: &PendingOrderSummary,
    pending_bulk_longs: &PendingOrderSummary,
    pending_bulk_shorts: &PendingOrderSummary,
) -> u128 {
    let PendingOrderSummary::V1 {
        price_size_sum: sl_pss,
        size_sum: sl_ss,
    } = pending_single_longs;
    let PendingOrderSummary::V1 {
        price_size_sum: ss_pss,
        size_sum: ss_ss,
    } = pending_single_shorts;
    let PendingOrderSummary::V1 {
        price_size_sum: bl_pss,
        size_sum: bl_ss,
    } = pending_bulk_longs;
    let PendingOrderSummary::V1 {
        price_size_sum: bs_pss,
        size_sum: bs_ss,
    } = pending_bulk_shorts;

    let total_longs_size = sl_ss + bl_ss;
    let total_longs_price_size = sl_pss + bl_pss;
    let total_shorts_size = ss_ss + bs_ss;
    let total_shorts_price_size = ss_pss + bs_pss;

    if position_is_long {
        let effective_pending_short_size = if total_shorts_size > 2 * position_size {
            total_shorts_size - 2 * position_size
        } else {
            0
        };
        let short_notional = if total_shorts_size == 0 {
            0
        } else {
            // math128::mul_div
            (effective_pending_short_size as u128)
                .checked_mul(total_shorts_price_size)
                .expect("overflow")
                / (total_shorts_size as u128)
        };
        std::cmp::max(total_longs_price_size, short_notional)
    } else {
        let effective_pending_long_size = if total_longs_size > 2 * position_size {
            total_longs_size - 2 * position_size
        } else {
            0
        };
        let long_notional = if total_longs_size == 0 {
            0
        } else {
            (effective_pending_long_size as u128)
                .checked_mul(total_longs_price_size)
                .expect("overflow")
                / (total_longs_size as u128)
        };
        std::cmp::max(total_shorts_price_size, long_notional)
    }
}

fn update_required_margin_for_market(
    market_state: &mut PendingMarketState,
    position_size: u64,
    position_is_long: bool,
    user_leverage: u8,
    size_multiplier: u64,
) {
    let pending_price_size = pending_price_size_for_market(
        position_size,
        position_is_long,
        market_state.pending_single_longs(),
        market_state.pending_single_shorts(),
        market_state.pending_bulk_longs(),
        market_state.pending_bulk_shorts(),
    );
    let divisor = (size_multiplier as u128) * (user_leverage as u128);
    *market_state.pending_margin_mut() = if divisor == 0 {
        0
    } else {
        ((pending_price_size + divisor - 1) / divisor) as u64
    };
}

fn add_to_pending_orders(
    pending_orders: &mut PendingOrderSummary,
    order_size: u64,
    limit_price: u64,
) {
    match pending_orders {
        PendingOrderSummary::V1 {
            price_size_sum,
            size_sum,
        } => {
            *price_size_sum += (order_size as u128) * (limit_price as u128);
            *size_sum += order_size;
        },
    }
}

fn calculate_pending_orders_from_prices_sizes(
    prices: &[u64],
    sizes: &[u64],
) -> PendingOrderSummary {
    let mut price_size_sum: u128 = 0;
    let mut size_sum: u64 = 0;
    for i in 0..prices.len() {
        price_size_sum += (prices[i] as u128) * (sizes[i] as u128);
        size_sum += sizes[i];
    }
    PendingOrderSummary::V1 {
        price_size_sum,
        size_sum,
    }
}

// ===================== Order tracking helpers =====================

fn add_order_id_tracking(market_state: &mut PendingMarketState, order_id: OrderId, size: u64) {
    if market_state.pending_order_ids().is_none() {
        *market_state.pending_order_ids_mut() = Some(BTreeMap::new());
    }
    let order_ids = market_state.pending_order_ids_mut().as_mut().unwrap();
    order_ids.insert(order_id, size);
}

fn remove_order_id_tracking(
    market_state: &mut PendingMarketState,
    order_id: OrderId,
    cleanup_size: u64,
) {
    if let Some(order_ids) = market_state.pending_order_ids_mut().as_mut() {
        if let Some(current_size) = order_ids.get(&order_id).copied() {
            if current_size == cleanup_size {
                order_ids.remove(&order_id);
            } else {
                *order_ids.get_mut(&order_id).unwrap() -= cleanup_size;
            }
        }
    }
}

// ===================== Core functions =====================

impl GlobalSummary {
    pub fn add_non_reduce_only_order(
        &mut self,
        account: [u8; 32],
        market: PerpMarketRef,
        order_id: OrderId,
        order_size: u64,
        limit_price: u64,
        is_long: bool,
        position_size: u64,
        position_is_long: bool,
        user_leverage: u8,
        size_multiplier: u64,
    ) {
        let account_summary = self.summary_mut().get_mut(&account).expect("account not found");
        let is_tracking_enabled = account_summary.is_order_id_tracking_enabled();
        if !account_summary.markets().contains_key(&market) {
            account_summary.markets_mut().insert(market, new_market_state());
        }
        let market_state = account_summary.markets_mut().get_mut(&market).unwrap();

        if is_long {
            add_to_pending_orders(
                market_state.pending_single_longs_mut(),
                order_size,
                limit_price,
            );
        } else {
            add_to_pending_orders(
                market_state.pending_single_shorts_mut(),
                order_size,
                limit_price,
            );
        }
        if is_tracking_enabled {
            add_order_id_tracking(market_state, order_id, order_size);
        }
        update_required_margin_for_market(
            market_state,
            position_size,
            position_is_long,
            user_leverage,
            size_multiplier,
        );
    }

    pub fn remove_pending_order(
        &mut self,
        account: [u8; 32],
        market: PerpMarketRef,
        order_id: OrderId,
        cleanup_size: u64,
        limit_price: u64,
        is_long: bool,
        is_reduce_only: bool,
        position_size: u64,
        position_is_long: bool,
        user_leverage: u8,
        size_multiplier: u64,
    ) {
        if is_reduce_only {
            self.remove_reduce_only_order(account, market, order_id, cleanup_size);
            return;
        }
        let account_summary = self.summary_mut().get_mut(&account).expect("account not found");
        let is_tracking_enabled = account_summary.is_order_id_tracking_enabled();
        let market_state = account_summary
            .markets_mut()
            .get_mut(&market)
            .expect("market not found");

        if is_long {
            match market_state.pending_single_longs_mut() {
                PendingOrderSummary::V1 {
                    price_size_sum,
                    size_sum,
                } => {
                    assert!(*size_sum >= cleanup_size, "Invalid cleanup size: {}", E_INVALID_ORDER_CLEANUP_SIZE);
                    *size_sum -= cleanup_size;
                    if *size_sum == 0 {
                        *price_size_sum = 0;
                    } else {
                        *price_size_sum -= (cleanup_size as u128) * (limit_price as u128);
                    }
                },
            }
        } else {
            match market_state.pending_single_shorts_mut() {
                PendingOrderSummary::V1 {
                    price_size_sum,
                    size_sum,
                } => {
                    assert!(*size_sum >= cleanup_size, "Invalid cleanup size: {}", E_INVALID_ORDER_CLEANUP_SIZE);
                    *size_sum -= cleanup_size;
                    if *size_sum == 0 {
                        *price_size_sum = 0;
                    } else {
                        *price_size_sum -= (cleanup_size as u128) * (limit_price as u128);
                    }
                },
            }
        }

        if is_tracking_enabled {
            remove_order_id_tracking(market_state, order_id, cleanup_size);
        }

        update_required_margin_for_market(
            market_state,
            position_size,
            position_is_long,
            user_leverage,
            size_multiplier,
        );
    }

    pub fn update_position(
        &mut self,
        account: [u8; 32],
        market: PerpMarketRef,
        position_size: u64,
        position_is_long: bool,
        user_leverage: u8,
        size_multiplier: u64,
    ) {
        if let Some(account_summary) = self.summary_mut().get_mut(&account) {
            if let Some(market_state) = account_summary.markets_mut().get_mut(&market) {
                update_required_margin_for_market(
                    market_state,
                    position_size,
                    position_is_long,
                    user_leverage,
                    size_multiplier,
                );
            }
        }
    }

    fn remove_reduce_only_order(
        &mut self,
        account: [u8; 32],
        market: PerpMarketRef,
        order_id: OrderId,
        cleanup_size: u64,
    ) {
        if let Some(account_summary) = self.summary_mut().get_mut(&account) {
            let is_tracking_enabled = account_summary.is_order_id_tracking_enabled();
            if let Some(market_state) = account_summary.markets_mut().get_mut(&market) {
                let ReduceOnlyOrders::V1 {
                    total_size,
                    orders,
                } = market_state.reduce_only_orders_mut();
                for i in 0..orders.len() {
                    let ReduceOnlyOrderInfo::V1 {
                        order_id: oid,
                        size,
                    } = &mut orders[i];
                    if *oid == order_id {
                        *total_size -= cleanup_size;
                        if cleanup_size < *size {
                            *size -= cleanup_size;
                        } else {
                            assert!(cleanup_size == *size, "Invalid cleanup size: {}", E_INVALID_ORDER_CLEANUP_SIZE);
                            orders.remove(i);
                        }
                        if is_tracking_enabled {
                            remove_order_id_tracking(market_state, order_id, cleanup_size);
                        }
                        break;
                    }
                }
            }
        }
    }

    pub fn get_reduce_only_orders(&self, account: [u8; 32], market: PerpMarketRef) -> Vec<OrderId> {
        if let Some(account_summary) = self.summary().get(&account) {
            if let Some(market_state) = account_summary.markets().get(&market) {
                let ReduceOnlyOrders::V1 { orders, .. } = market_state.reduce_only_orders();
                return orders
                    .iter()
                    .map(|o| match o {
                        ReduceOnlyOrderInfo::V1 { order_id, .. } => *order_id,
                    })
                    .collect();
            }
        }
        Vec::new()
    }

    pub fn cancel_all_tp_sl_for_position(
        &mut self,
        tp_sl_tracker: &mut TpSlTracker,
        market: PerpMarketRef,
        account: [u8; 32],
        position_is_long: bool,
    ) {
        if let Some(account_summary) = self.summary_mut().get_mut(&account) {
            if let Some(market_state) = account_summary.markets_mut().get_mut(&market) {
                // Cancel full-sized TP
                cancel_full_sized_tp_sl_internal(
                    tp_sl_tracker,
                    market_state,
                    market,
                    true,
                    position_is_long,
                );
                // Cancel full-sized SL
                cancel_full_sized_tp_sl_internal(
                    tp_sl_tracker,
                    market_state,
                    market,
                    false,
                    position_is_long,
                );
                // Cancel fixed-sized TPs
                let PendingTpSLs::V1 { fixed_sized, .. } = market_state.tp_reqs_mut();
                for key in fixed_sized.drain(..) {
                    match key {
                        PendingTpSlKey::V1 { price_index, .. } => {
                            position_tp_sl_tracker::cancel_pending_tp_sl(
                                tp_sl_tracker,
                                &price_index,
                                true,
                                position_is_long,
                            );
                        },
                    }
                }
                // Cancel fixed-sized SLs
                let PendingTpSLs::V1 { fixed_sized, .. } = market_state.sl_reqs_mut();
                for key in fixed_sized.drain(..) {
                    match key {
                        PendingTpSlKey::V1 { price_index, .. } => {
                            position_tp_sl_tracker::cancel_pending_tp_sl(
                                tp_sl_tracker,
                                &price_index,
                                false,
                                position_is_long,
                            );
                        },
                    }
                }
            }
        }
    }

    pub fn enable_order_id_tracking(&mut self, account: [u8; 32]) {
        if !self.summary().contains_key(&account) {
            self.summary_mut().insert(
                account,
                AccountSummary::V1 {
                    markets: BTreeMap::new(),
                    is_order_id_tracking_enabled: true,
                },
            );
            return;
        }
        let account_summary = self.summary_mut().get_mut(&account).unwrap();
        if account_summary.is_order_id_tracking_enabled() {
            return;
        }
        assert!(
            !has_any_pending_orders_internal(account_summary),
            "Pending orders exist: {}",
            E_PENDING_ORDERS_EXIST
        );
        account_summary.set_order_id_tracking_enabled(true);
    }

    pub fn is_order_id_tracking_enabled(&self, account: [u8; 32]) -> bool {
        self.summary()
            .get(&account)
            .map_or(false, |s| s.is_order_id_tracking_enabled())
    }

    pub fn get_tracked_order_ids_with_sizes(
        &self,
        account: [u8; 32],
        market: PerpMarketRef,
        max_orders: u64,
    ) -> (Vec<OrderId>, Vec<u64>) {
        if let Some(account_summary) = self.summary().get(&account) {
            if let Some(market_state) = account_summary.markets().get(&market) {
                if let Some(order_ids_map) = market_state.pending_order_ids() {
                    let mut order_ids = Vec::new();
                    let mut sizes = Vec::new();
                    let mut count = 0u64;
                    for (oid, size) in order_ids_map {
                        if count >= max_orders {
                            break;
                        }
                        order_ids.push(*oid);
                        sizes.push(*size);
                        count += 1;
                    }
                    return (order_ids, sizes);
                }
            }
        }
        (Vec::new(), Vec::new())
    }

    pub fn get_next_tracked_order_using_margin(&self, account: [u8; 32]) -> Option<TrackedOrderInfo> {
        if let Some(account_summary) = self.summary().get(&account) {
            for (market, market_state) in account_summary.markets() {
                if market_state.pending_margin() > 0 {
                    if let Some(order_ids_map) = market_state.pending_order_ids() {
                        if let Some((order_id, size)) = order_ids_map.iter().next() {
                            return Some(TrackedOrderInfo::V1 {
                                market: *market,
                                order_id: *order_id,
                                size: *size,
                            });
                        }
                    }
                }
            }
        }
        None
    }
}

fn has_any_pending_orders_internal(account_summary: &AccountSummary) -> bool {
    for (_market, state) in account_summary.markets() {
        let PendingOrderSummary::V1 {
            size_sum: sl_size, ..
        } = *state.pending_single_longs();
        let PendingOrderSummary::V1 {
            size_sum: ss_size, ..
        } = *state.pending_single_shorts();
        let PendingOrderSummary::V1 {
            size_sum: bl_size, ..
        } = *state.pending_bulk_longs();
        let PendingOrderSummary::V1 {
            size_sum: bs_size, ..
        } = *state.pending_bulk_shorts();
        let ReduceOnlyOrders::V1 {
            total_size: ro_size,
            ..
        } = state.reduce_only_orders();
        if sl_size > 0 || ss_size > 0 || bl_size > 0 || bs_size > 0 || *ro_size > 0 {
            return true;
        }
    }
    false
}

fn cancel_full_sized_tp_sl_internal(
    tp_sl_tracker: &mut TpSlTracker,
    market_state: &mut PendingMarketState,
    _market: PerpMarketRef,
    is_tp: bool,
    position_is_long: bool,
) {
    let reqs = if is_tp {
        market_state.tp_reqs_mut()
    } else {
        market_state.sl_reqs_mut()
    };
    let PendingTpSLs::V1 { full_sized, .. } = reqs;
    if let Some(key) = full_sized.take() {
        match key {
            PendingTpSlKey::V1 { price_index, .. } => {
                position_tp_sl_tracker::cancel_pending_tp_sl(
                    tp_sl_tracker,
                    &price_index,
                    is_tp,
                    position_is_long,
                );
            },
        }
    }
}

// ===================== TrackedOrderInfo accessors =====================

impl TrackedOrderInfo {
    pub fn get_market(&self) -> PerpMarketRef {
        match self {
            TrackedOrderInfo::V1 { market, .. } => *market,
        }
    }

    pub fn get_order_id(&self) -> OrderId {
        match self {
            TrackedOrderInfo::V1 { order_id, .. } => *order_id,
        }
    }

    pub fn get_size(&self) -> u64 {
        match self {
            TrackedOrderInfo::V1 { size, .. } => *size,
        }
    }
}

// ===================== Dispatch stubs (by-addr) for perp_engine delegation =====================

pub fn has_any_pending_orders_by_addr(_account: [u8; 32]) -> bool {
    // Dispatch layer resolves PendingOrderTracker resource
    false
}
