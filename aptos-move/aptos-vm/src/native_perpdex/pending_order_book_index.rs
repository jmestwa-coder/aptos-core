// Copyright (c) Aptos Foundation
// Translated from: aptos_market::pending_order_book_index

use crate::native_perpdex::order_book_types::{
    into_decreasing_idx_type, DecreasingIdx, IncreasingIdx, OrderId, TriggerCondition,
    get_trigger_condition_indices,
};
use crate::native_perpdex::order_book_utils::BigOrderedMap;

// ===================== Key Types =====================

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub struct PendingUpOrderKey {
    pub price: u64,
    pub tie_breaker: IncreasingIdx,
}

impl PartialOrd for PendingUpOrderKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PendingUpOrderKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.price
            .cmp(&other.price)
            .then(self.tie_breaker.idx.cmp(&other.tie_breaker.idx))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub struct PendingDownOrderKey {
    pub price: u64,
    pub tie_breaker: DecreasingIdx,
}

impl PartialOrd for PendingDownOrderKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PendingDownOrderKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.price
            .cmp(&other.price)
            .then(self.tie_breaker.idx.cmp(&other.tie_breaker.idx))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub struct PendingTimeKey {
    pub time: u64,
    pub tie_breaker: IncreasingIdx,
}

impl PartialOrd for PendingTimeKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PendingTimeKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time
            .cmp(&other.time)
            .then(self.tie_breaker.idx.cmp(&other.tie_breaker.idx))
    }
}

// ===================== PendingOrderBookIndex =====================

#[derive(Clone, Debug)]
pub enum PendingOrderBookIndex {
    V1 {
        price_move_down_index: BigOrderedMap<PendingDownOrderKey, OrderId>,
        price_move_up_index: BigOrderedMap<PendingUpOrderKey, OrderId>,
        time_based_index: BigOrderedMap<PendingTimeKey, OrderId>,
    },
}

pub fn new_pending_order_book_index() -> PendingOrderBookIndex {
    PendingOrderBookIndex::V1 {
        price_move_down_index: BigOrderedMap::new(),
        price_move_up_index: BigOrderedMap::new(),
        time_based_index: BigOrderedMap::new(),
    }
}

pub fn cancel_pending_order(
    index: &mut PendingOrderBookIndex,
    trigger_condition: TriggerCondition,
    unique_priority_idx: IncreasingIdx,
) {
    let PendingOrderBookIndex::V1 {
        price_move_down_index,
        price_move_up_index,
        time_based_index,
    } = index;

    let (price_move_down, price_move_up, time_based) =
        get_trigger_condition_indices(&trigger_condition);

    if let Some(price) = price_move_up {
        let key = PendingUpOrderKey {
            price,
            tie_breaker: unique_priority_idx,
        };
        price_move_up_index.remove(&key);
    }
    if let Some(price) = price_move_down {
        let key = PendingDownOrderKey {
            price,
            tie_breaker: into_decreasing_idx_type(&unique_priority_idx),
        };
        price_move_down_index.remove(&key);
    }
    if let Some(time) = time_based {
        let key = PendingTimeKey {
            time,
            tie_breaker: unique_priority_idx,
        };
        time_based_index.remove(&key);
    }
}

pub fn place_pending_order(
    index: &mut PendingOrderBookIndex,
    order_id: OrderId,
    trigger_condition: TriggerCondition,
    unique_priority_idx: IncreasingIdx,
) {
    let PendingOrderBookIndex::V1 {
        price_move_down_index,
        price_move_up_index,
        time_based_index,
    } = index;

    let (price_move_down, price_move_up, time_based) =
        get_trigger_condition_indices(&trigger_condition);

    if let Some(price) = price_move_up {
        price_move_up_index.add(
            PendingUpOrderKey {
                price,
                tie_breaker: unique_priority_idx,
            },
            order_id,
        );
    } else if let Some(price) = price_move_down {
        price_move_down_index.add(
            PendingDownOrderKey {
                price,
                tie_breaker: into_decreasing_idx_type(&unique_priority_idx),
            },
            order_id,
        );
    } else if let Some(time) = time_based {
        time_based_index.add(
            PendingTimeKey {
                time,
                tie_breaker: unique_priority_idx,
            },
            order_id,
        );
    }
}

fn take_ready_price_move_up_orders(
    price_move_up_index: &mut BigOrderedMap<PendingUpOrderKey, OrderId>,
    current_price: u64,
    orders: &mut Vec<OrderId>,
    limit: u64,
) {
    while !price_move_up_index.is_empty() && (orders.len() as u64) < limit {
        let (key, order_id) = {
            let (k, v) = price_move_up_index.borrow_front();
            (*k, *v)
        };
        if current_price >= key.price {
            orders.push(order_id);
            price_move_up_index.remove(&key);
        } else {
            break;
        }
    }
}

fn take_ready_price_move_down_orders(
    price_move_down_index: &mut BigOrderedMap<PendingDownOrderKey, OrderId>,
    current_price: u64,
    orders: &mut Vec<OrderId>,
    limit: u64,
) {
    while !price_move_down_index.is_empty() && (orders.len() as u64) < limit {
        let (key, order_id) = {
            let (k, v) = price_move_down_index.borrow_back();
            (*k, *v)
        };
        if current_price <= key.price {
            orders.push(order_id);
            price_move_down_index.remove(&key);
        } else {
            break;
        }
    }
}

pub fn take_ready_price_based_orders(
    index: &mut PendingOrderBookIndex,
    current_price: u64,
    order_limit: u64,
) -> Vec<OrderId> {
    let PendingOrderBookIndex::V1 {
        price_move_down_index,
        price_move_up_index,
        ..
    } = index;

    let mut orders = Vec::new();
    // ceil_div(order_limit, 2)
    let half_limit = (order_limit + 1) / 2;
    take_ready_price_move_up_orders(price_move_up_index, current_price, &mut orders, half_limit);
    take_ready_price_move_down_orders(
        price_move_down_index,
        current_price,
        &mut orders,
        order_limit,
    );
    // Try to fill the rest of the space if available.
    take_ready_price_move_up_orders(price_move_up_index, current_price, &mut orders, order_limit);
    orders
}

/// Takes orders ready based on time. Requires the current time in seconds.
pub fn take_ready_time_based_orders(
    index: &mut PendingOrderBookIndex,
    order_limit: u64,
    current_time_secs: u64,
) -> Vec<OrderId> {
    let PendingOrderBookIndex::V1 {
        time_based_index, ..
    } = index;

    let mut orders = Vec::new();
    while !time_based_index.is_empty() && (orders.len() as u64) < order_limit {
        let (key, order_id) = {
            let (k, v) = time_based_index.borrow_front();
            (*k, *v)
        };
        if current_time_secs >= key.time {
            orders.push(order_id);
            time_based_index.remove(&key);
        } else {
            break;
        }
    }
    orders
}

// ===================== Test helpers =====================

pub fn get_price_move_down_index(
    index: &PendingOrderBookIndex,
) -> &BigOrderedMap<PendingDownOrderKey, OrderId> {
    let PendingOrderBookIndex::V1 {
        price_move_down_index,
        ..
    } = index;
    price_move_down_index
}

pub fn get_price_move_up_index(
    index: &PendingOrderBookIndex,
) -> &BigOrderedMap<PendingUpOrderKey, OrderId> {
    let PendingOrderBookIndex::V1 {
        price_move_up_index,
        ..
    } = index;
    price_move_up_index
}

pub fn get_time_based_index(
    index: &PendingOrderBookIndex,
) -> &BigOrderedMap<PendingTimeKey, OrderId> {
    let PendingOrderBookIndex::V1 {
        time_based_index, ..
    } = index;
    time_based_index
}
