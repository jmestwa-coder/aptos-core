// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::work_unit_utils

use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const DEFAULT_WORK_UNITS_PER_TRIGGER: u32 = 500;
const FINISH_OR_ABORT_WORK_UNITS: u32 = 1_000_000_000;

const POSITION_STATUS_WORK_UNITS: u32 = 50;
const POSITION_STATUS_WORK_UNITS_BASE: u32 = 5;
const POSITION_STATUS_WORK_UNITS_PER_MARKET_BY_10: u32 = 5;

const BACKSTOP_LIQUIDATION_OR_ADL_WORK_UNITS: u32 = 50;
const ORDER_MATCH_WORK_UNITS: u32 = 100;

const SMALL_WORK_UNITS: u32 = 5;

const MARGIN_CALL_ONE_MARKET_WORK_UNITS: u32 = 20;
const MARGIN_CALL_OVERHEAD_WORK_UNITS: u32 = 20;
const REFRESH_MARK_PRICE_WORK_UNITS: u32 = 20;
const BULK_ORDER_WORK_UNITS: u32 = 50;
const QUEUE_LIQUIDATION_WORK_UNITS_BY_10: u32 = 5;

// ===================== WorkUnit enum =====================

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum WorkUnit {
    V1 { amount: u32 },
}

// ===================== Functions =====================

pub fn get_default_work_units() -> WorkUnit {
    WorkUnit::V1 {
        amount: DEFAULT_WORK_UNITS_PER_TRIGGER,
    }
}

pub fn get_finish_or_abort_work_units() -> WorkUnit {
    WorkUnit::V1 {
        amount: FINISH_OR_ABORT_WORK_UNITS,
    }
}

pub fn get_work_units_from_argument(amount: u32) -> WorkUnit {
    WorkUnit::V1 { amount }
}

pub fn has_more_work(work_unit: &WorkUnit) -> bool {
    let WorkUnit::V1 { amount } = work_unit;
    *amount > 0
}

pub fn consume_position_status_work_units_from_work_used(
    work_unit: &mut WorkUnit,
    work_used: u32,
) {
    consume_work_units(
        work_unit,
        POSITION_STATUS_WORK_UNITS_BASE
            + work_used * POSITION_STATUS_WORK_UNITS_PER_MARKET_BY_10 / 10,
    );
}

pub fn consume_margin_call_one_market_work_units(work_unit: &mut WorkUnit) {
    consume_work_units(work_unit, MARGIN_CALL_ONE_MARKET_WORK_UNITS);
}

pub fn consume_margin_call_overhead_work_units(work_unit: &mut WorkUnit) {
    consume_work_units(work_unit, MARGIN_CALL_OVERHEAD_WORK_UNITS);
}

pub fn consume_backstop_liquidation_or_adl_work_units(
    work_unit: &mut WorkUnit,
    markets: u32,
) {
    consume_work_units(work_unit, BACKSTOP_LIQUIDATION_OR_ADL_WORK_UNITS * markets);
}

pub fn consume_refresh_mark_price_work_units(work_unit: &mut WorkUnit) {
    consume_work_units(work_unit, REFRESH_MARK_PRICE_WORK_UNITS);
}

pub fn consume_small_work_units(work_unit: &mut WorkUnit) {
    consume_work_units(work_unit, SMALL_WORK_UNITS);
}

pub fn consume_order_placement_work_units(work_unit: &mut WorkUnit) {
    consume_work_units(work_unit, POSITION_STATUS_WORK_UNITS);
}

pub fn get_max_order_placement_limit(work_unit: &WorkUnit, max_value: u32) -> u32 {
    let WorkUnit::V1 { amount } = work_unit;
    let max_order_placement_limit = amount / POSITION_STATUS_WORK_UNITS;
    if max_order_placement_limit == 0 {
        // always make progress with the current action
        1
    } else if max_order_placement_limit > max_value {
        max_value
    } else {
        max_order_placement_limit
    }
}

pub fn consume_order_match_work_units(work_unit: &mut WorkUnit, match_count: u32) {
    consume_work_units(
        work_unit,
        if match_count == 0 {
            POSITION_STATUS_WORK_UNITS
        } else {
            match_count * ORDER_MATCH_WORK_UNITS
        },
    );
}

pub fn get_max_match_limit(work_unit: &WorkUnit) -> u32 {
    let WorkUnit::V1 { amount } = work_unit;
    let max_match_limit = amount / ORDER_MATCH_WORK_UNITS;
    if max_match_limit == 0 {
        1
    } else {
        max_match_limit
    }
}

pub fn consume_bulk_order_work_units(work_unit: &mut WorkUnit) {
    consume_work_units(work_unit, BULK_ORDER_WORK_UNITS);
}

pub fn consume_queue_liquidations_work_units(
    work_unit: &mut WorkUnit,
    num_liquidations: u32,
) {
    consume_work_units(
        work_unit,
        SMALL_WORK_UNITS + num_liquidations * QUEUE_LIQUIDATION_WORK_UNITS_BY_10 / 10,
    );
}

fn consume_work_units(work_unit: &mut WorkUnit, amount_to_consume: u32) {
    let WorkUnit::V1 { amount } = work_unit;
    // If we are left with less work than 2x what we need to consume,
    // consume all remaining work.
    if 2 * amount_to_consume > *amount {
        *amount = 0;
    } else {
        *amount -= amount_to_consume;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_default_work_units() {
        let wu = get_default_work_units();
        let WorkUnit::V1 { amount } = wu;
        assert_eq!(amount, 500);
    }

    #[test]
    fn test_has_more_work() {
        let wu = get_default_work_units();
        assert!(has_more_work(&wu));

        let wu = WorkUnit::V1 { amount: 0 };
        assert!(!has_more_work(&wu));
    }

    #[test]
    fn test_consume_work_units() {
        let mut wu = WorkUnit::V1 { amount: 100 };
        consume_work_units(&mut wu, 10);
        let WorkUnit::V1 { amount } = wu;
        assert_eq!(amount, 90);
    }

    #[test]
    fn test_consume_work_units_exhaustion() {
        let mut wu = WorkUnit::V1 { amount: 15 };
        // 2 * 10 > 15, so amount goes to 0
        consume_work_units(&mut wu, 10);
        let WorkUnit::V1 { amount } = wu;
        assert_eq!(amount, 0);
    }

    #[test]
    fn test_get_max_match_limit() {
        let wu = WorkUnit::V1 { amount: 500 };
        assert_eq!(get_max_match_limit(&wu), 5); // 500 / 100

        let wu = WorkUnit::V1 { amount: 50 };
        assert_eq!(get_max_match_limit(&wu), 1); // 50 / 100 = 0, minimum 1
    }
}
