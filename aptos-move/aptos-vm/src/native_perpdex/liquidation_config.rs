// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::liquidation_config

use serde::{Deserialize, Serialize};

// ===================== Types =====================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LiquidationConfig {
    V1 {
        backstop_liquidator: [u8; 32], // address
        // maintenance_margin = 1 / max_leverage * multiplier/divisor
        maintenance_margin_leverage_multiplier: u64,
        maintenance_margin_leverage_divisor: u64,
        // backstop_margin = 1 / max_leverage * multiplier/divisor
        backstop_margin_maintenance_multiplier: u64,
        backstop_margin_maintenance_divisor: u64,
    },
}

// ===================== Functions =====================

pub fn new_config(backstop_liquidator: [u8; 32]) -> LiquidationConfig {
    LiquidationConfig::V1 {
        backstop_liquidator,
        maintenance_margin_leverage_multiplier: 1,
        maintenance_margin_leverage_divisor: 2,
        // (1 / 2) * (2 / 3) = 1 / 3
        backstop_margin_maintenance_multiplier: 1,
        backstop_margin_maintenance_divisor: 3,
    }
}

impl LiquidationConfig {
    fn fields(
        &self,
    ) -> (
        [u8; 32],
        u64,
        u64,
        u64,
        u64,
    ) {
        match self {
            LiquidationConfig::V1 {
                backstop_liquidator,
                maintenance_margin_leverage_multiplier,
                maintenance_margin_leverage_divisor,
                backstop_margin_maintenance_multiplier,
                backstop_margin_maintenance_divisor,
            } => (
                *backstop_liquidator,
                *maintenance_margin_leverage_multiplier,
                *maintenance_margin_leverage_divisor,
                *backstop_margin_maintenance_multiplier,
                *backstop_margin_maintenance_divisor,
            ),
        }
    }

    pub fn get_liquidation_margin(&self, margin_for_max_leverage: u64, is_backstop: bool) -> u64 {
        let (_, mm_mult, mm_div, bm_mult, bm_div) = self.fields();
        if is_backstop {
            ceil_div(
                margin_for_max_leverage
                    .checked_mul(bm_mult)
                    .expect("overflow"),
                bm_div,
            )
        } else {
            ceil_div(
                margin_for_max_leverage
                    .checked_mul(mm_mult)
                    .expect("overflow"),
                mm_div,
            )
        }
    }

    pub fn get_liquidation_price(
        &self,
        mark_price: u64,
        market_max_leverage: u8,
        is_backstop: bool,
    ) -> u64 {
        let (_, mm_mult, mm_div, bm_mult, bm_div) = self.fields();
        if is_backstop {
            ceil_div(
                mark_price.checked_mul(bm_mult).expect("overflow"),
                (market_max_leverage as u64)
                    .checked_mul(bm_div)
                    .expect("overflow"),
            )
        } else {
            ceil_div(
                mark_price.checked_mul(mm_mult).expect("overflow"),
                (market_max_leverage as u64)
                    .checked_mul(mm_div)
                    .expect("overflow"),
            )
        }
    }

    pub fn backstop_liquidator(&self) -> [u8; 32] {
        self.fields().0
    }

    pub fn maintenance_margin_leverage_multiplier(&self) -> u64 {
        self.fields().1
    }

    pub fn maintenance_margin_leverage_divisor(&self) -> u64 {
        self.fields().2
    }

    pub fn backstop_margin_maintenance_multiplier(&self) -> u64 {
        self.fields().3
    }

    pub fn backstop_margin_maintenance_divisor(&self) -> u64 {
        self.fields().4
    }
}

// ===================== Helpers =====================

/// std::math64::ceil_div equivalent
fn ceil_div(a: u64, b: u64) -> u64 {
    if b == 0 {
        panic!("division by zero");
    }
    (a + b - 1) / b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_config_defaults() {
        let mut addr = [0u8; 32];
        addr[31] = 0x99;
        let config = new_config(addr);
        assert_eq!(config.backstop_liquidator(), addr);
        assert_eq!(config.maintenance_margin_leverage_multiplier(), 1);
        assert_eq!(config.maintenance_margin_leverage_divisor(), 2);
        assert_eq!(config.backstop_margin_maintenance_multiplier(), 1);
        assert_eq!(config.backstop_margin_maintenance_divisor(), 3);
    }

    #[test]
    fn test_get_liquidation_margin() {
        let config = new_config([0u8; 32]);
        // margin_for_max_leverage = 100
        // maintenance: ceil(100 * 1 / 2) = 50
        assert_eq!(config.get_liquidation_margin(100, false), 50);
        // backstop: ceil(100 * 1 / 3) = 34
        assert_eq!(config.get_liquidation_margin(100, true), 34);
    }

    #[test]
    fn test_get_liquidation_price() {
        let config = new_config([0u8; 32]);
        // mark_price=1000, max_leverage=20, not backstop
        // ceil(1000 * 1 / (20 * 2)) = ceil(1000/40) = 25
        assert_eq!(config.get_liquidation_price(1000, 20, false), 25);
        // backstop: ceil(1000 * 1 / (20 * 3)) = ceil(1000/60) = 17
        assert_eq!(config.get_liquidation_price(1000, 20, true), 17);
    }
}
