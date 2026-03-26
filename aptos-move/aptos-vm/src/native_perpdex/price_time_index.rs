// Copyright (c) Aptos Foundation
// Translated from: aptos_market::price_time_index

#[allow(dead_code)]
use crate::native_perpdex::order_book_types::{
    into_decreasing_idx_type, DecreasingIdx, IncreasingIdx, OrderId, OrderType,
};
use crate::native_perpdex::order_book_utils::BigOrderedMap;
use crate::native_perpdex::order_match_types::{new_active_matched_order, ActiveMatchedOrder};

// ===================== Constants =====================

const EINVALID_MAKER_ORDER: u64 = 1;
const EINTERNAL_INVARIANT_BROKEN: u64 = 2;
const EINVALID_SLIPPAGE_BPS: u64 = 3;

const SLIPPAGE_PCT_PRECISION: u64 = 100;

// ===================== Key Types =====================

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub struct PriceAscTime {
    pub price: u64,
    pub tie_breaker: IncreasingIdx,
}

impl PartialOrd for PriceAscTime {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriceAscTime {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.price
            .cmp(&other.price)
            .then(self.tie_breaker.idx.cmp(&other.tie_breaker.idx))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub struct PriceDescTime {
    pub price: u64,
    pub tie_breaker: DecreasingIdx,
}

impl PartialOrd for PriceDescTime {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriceDescTime {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.price
            .cmp(&other.price)
            .then(self.tie_breaker.idx.cmp(&other.tie_breaker.idx))
    }
}

// ===================== OrderData =====================

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub struct OrderData {
    pub order_id: OrderId,
    pub order_book_type: OrderType,
    pub size: u64,
}

// ===================== PriceTimeIndex =====================

#[derive(Clone, Debug)]
pub enum PriceTimeIndex {
    V1 {
        buys: BigOrderedMap<PriceDescTime, OrderData>,
        sells: BigOrderedMap<PriceAscTime, OrderData>,
    },
}

pub fn new_price_time_idx() -> PriceTimeIndex {
    PriceTimeIndex::V1 {
        buys: BigOrderedMap::new(),
        sells: BigOrderedMap::new(),
    }
}

pub fn best_bid_price(index: &PriceTimeIndex) -> Option<u64> {
    let PriceTimeIndex::V1 { buys, .. } = index;
    if buys.is_empty() {
        None
    } else {
        let (back_key, _) = buys.borrow_back();
        Some(back_key.price)
    }
}

pub fn best_ask_price(index: &PriceTimeIndex) -> Option<u64> {
    let PriceTimeIndex::V1 { sells, .. } = index;
    if sells.is_empty() {
        None
    } else {
        let (front_key, _) = sells.borrow_front();
        Some(front_key.price)
    }
}

pub fn get_mid_price(index: &PriceTimeIndex) -> Option<u64> {
    let PriceTimeIndex::V1 { buys, sells } = index;
    if sells.is_empty() || buys.is_empty() {
        return None;
    }
    let (front_key, _) = sells.borrow_front();
    let best_ask = front_key.price;
    let (back_key, _) = buys.borrow_back();
    let best_bid = back_key.price;
    Some((best_bid + best_ask) / 2)
}

pub fn get_slippage_price(index: &PriceTimeIndex, is_bid_side: bool, slippage_bps: u64) -> Option<u64> {
    if !is_bid_side {
        assert!(
            slippage_bps <= SLIPPAGE_PCT_PRECISION * 100,
            // EINVALID_SLIPPAGE_BPS
        );
    }
    let mid_price = get_mid_price(index)?;
    // mul_div(mid_price, slippage_bps, SLIPPAGE_PCT_PRECISION * 100)
    let slippage = mid_price * slippage_bps / (SLIPPAGE_PCT_PRECISION * 100);
    if is_bid_side {
        Some(mid_price + slippage)
    } else {
        Some(mid_price - slippage)
    }
}

pub fn cancel_active_order(
    index: &mut PriceTimeIndex,
    price: u64,
    unique_priority_idx: IncreasingIdx,
    is_bid_side: bool,
) -> u64 {
    let PriceTimeIndex::V1 { buys, sells } = index;
    if is_bid_side {
        let key = PriceDescTime {
            price,
            tie_breaker: into_decreasing_idx_type(&unique_priority_idx),
        };
        buys.remove(&key).size
    } else {
        let key = PriceAscTime {
            price,
            tie_breaker: unique_priority_idx,
        };
        sells.remove(&key).size
    }
}

pub fn is_taker_order(index: &PriceTimeIndex, price: u64, is_bid_side: bool) -> bool {
    if is_bid_side {
        let best_ask = best_ask_price(index);
        best_ask.is_some() && price >= best_ask.unwrap()
    } else {
        let best_bid = best_bid_price(index);
        best_bid.is_some() && price <= best_bid.unwrap()
    }
}

fn single_match_with_current_active_order_sells(
    remaining_size: u64,
    cur_key: PriceAscTime,
    cur_value: OrderData,
    orders: &mut BigOrderedMap<PriceAscTime, OrderData>,
) -> ActiveMatchedOrder {
    let is_fully_consumed = cur_value.size <= remaining_size;
    let matched_size = if is_fully_consumed {
        orders.remove(&cur_key);
        cur_value.size
    } else {
        let order = orders.borrow_mut(&cur_key);
        order.size -= remaining_size;
        remaining_size
    };
    new_active_matched_order(
        cur_value.order_id,
        matched_size,
        cur_value.size - matched_size,
        cur_value.order_book_type,
    )
}

fn single_match_with_current_active_order_buys(
    remaining_size: u64,
    cur_key: PriceDescTime,
    cur_value: OrderData,
    orders: &mut BigOrderedMap<PriceDescTime, OrderData>,
) -> ActiveMatchedOrder {
    let is_fully_consumed = cur_value.size <= remaining_size;
    let matched_size = if is_fully_consumed {
        orders.remove(&cur_key);
        cur_value.size
    } else {
        let order = orders.borrow_mut(&cur_key);
        order.size -= remaining_size;
        remaining_size
    };
    new_active_matched_order(
        cur_value.order_id,
        matched_size,
        cur_value.size - matched_size,
        cur_value.order_book_type,
    )
}

fn get_single_match_for_buy_order(
    index: &mut PriceTimeIndex,
    price: u64,
    size: u64,
) -> ActiveMatchedOrder {
    let PriceTimeIndex::V1 { sells, .. } = index;
    let (smallest_key, smallest_value) = {
        let (k, v) = sells.borrow_front();
        (*k, *v)
    };
    assert!(price >= smallest_key.price, "EINTERNAL_INVARIANT_BROKEN: buy price < best ask");
    single_match_with_current_active_order_sells(size, smallest_key, smallest_value, sells)
}

fn get_single_match_for_sell_order(
    index: &mut PriceTimeIndex,
    price: u64,
    size: u64,
) -> ActiveMatchedOrder {
    let PriceTimeIndex::V1 { buys, .. } = index;
    let (largest_key, largest_value) = {
        let (k, v) = buys.borrow_back();
        (*k, *v)
    };
    assert!(price <= largest_key.price, "EINTERNAL_INVARIANT_BROKEN: sell price > best bid");
    single_match_with_current_active_order_buys(size, largest_key, largest_value, buys)
}

pub fn get_single_match_result(
    index: &mut PriceTimeIndex,
    price: u64,
    size: u64,
    is_bid_side: bool,
) -> ActiveMatchedOrder {
    if is_bid_side {
        get_single_match_for_buy_order(index, price, size)
    } else {
        get_single_match_for_sell_order(index, price, size)
    }
}

pub fn increase_order_size(
    index: &mut PriceTimeIndex,
    price: u64,
    unique_priority_idx: IncreasingIdx,
    size_delta: u64,
    is_bid_side: bool,
) {
    let PriceTimeIndex::V1 { buys, sells } = index;
    if is_bid_side {
        let key = PriceDescTime {
            price,
            tie_breaker: into_decreasing_idx_type(&unique_priority_idx),
        };
        buys.borrow_mut(&key).size += size_delta;
    } else {
        let key = PriceAscTime {
            price,
            tie_breaker: unique_priority_idx,
        };
        sells.borrow_mut(&key).size += size_delta;
    }
}

pub fn decrease_order_size(
    index: &mut PriceTimeIndex,
    price: u64,
    unique_priority_idx: IncreasingIdx,
    size_delta: u64,
    is_bid_side: bool,
) {
    let PriceTimeIndex::V1 { buys, sells } = index;
    if is_bid_side {
        let key = PriceDescTime {
            price,
            tie_breaker: into_decreasing_idx_type(&unique_priority_idx),
        };
        buys.borrow_mut(&key).size -= size_delta;
    } else {
        let key = PriceAscTime {
            price,
            tie_breaker: unique_priority_idx,
        };
        sells.borrow_mut(&key).size -= size_delta;
    }
}

pub fn place_maker_order(
    index: &mut PriceTimeIndex,
    order_id: OrderId,
    order_book_type: OrderType,
    price: u64,
    unique_priority_idx: IncreasingIdx,
    size: u64,
    is_bid_side: bool,
) {
    let value = OrderData {
        order_id,
        order_book_type,
        size,
    };
    assert!(
        !is_taker_order(index, price, is_bid_side),
        "EINVALID_MAKER_ORDER: order would cross the spread"
    );
    let PriceTimeIndex::V1 { buys, sells } = index;
    if is_bid_side {
        let key = PriceDescTime {
            price,
            tie_breaker: into_decreasing_idx_type(&unique_priority_idx),
        };
        buys.add(key, value);
    } else {
        let key = PriceAscTime {
            price,
            tie_breaker: unique_priority_idx,
        };
        sells.add(key, value);
    }
}
