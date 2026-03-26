// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::i64_aggregator
//
// In Move, I64Aggregator wraps an Aggregator<u64> with a signed offset.
// In native Rust, we simply store the i64 value directly since we don't
// need the parallelism features of Aggregator.

use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const SIGNED_ZERO: u64 = 9223372036854775808; // 2^63 or (i64::MAX as u64 + 1)

// ===================== Types =====================

/// Native equivalent of Move's I64Aggregator.
/// In Move this uses Aggregator<u64> with offset encoding for signed values.
/// In Rust we just store the i64 directly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct I64Aggregator {
    value: i64,
}

/// Native equivalent of Move's I64Snapshot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum I64Snapshot {
    V1 { value: i64 },
}

// ===================== Functions =====================

pub fn new_i64_aggregator() -> I64Aggregator {
    I64Aggregator { value: 0 }
}

pub fn new_i64_aggregator_with_value(value: i64) -> I64Aggregator {
    I64Aggregator { value }
}

pub fn create_i64_snapshot(value: i64) -> I64Snapshot {
    I64Snapshot::V1 { value }
}

impl I64Aggregator {
    pub fn read(&self) -> i64 {
        self.value
    }

    pub fn add(&mut self, amount: i64) {
        self.value += amount;
    }

    pub fn is_at_least(&self, amount: i64) -> bool {
        self.value >= amount
    }

    pub fn snapshot(&self) -> I64Snapshot {
        I64Snapshot::V1 {
            value: self.value,
        }
    }
}

impl I64Snapshot {
    pub fn get_value(&self) -> i64 {
        match self {
            I64Snapshot::V1 { value } => *value,
        }
    }
}

// ===================== Tests =====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_with_value() {
        let balance = new_i64_aggregator_with_value(100);
        assert_eq!(balance.read(), 100);

        let balance = new_i64_aggregator_with_value(-50);
        assert_eq!(balance.read(), -50);

        let balance = new_i64_aggregator_with_value(0);
        assert_eq!(balance.read(), 0);

        let balance = new_i64_aggregator_with_value(i64::MAX);
        assert_eq!(balance.read(), i64::MAX);

        let balance = new_i64_aggregator_with_value(i64::MIN);
        assert_eq!(balance.read(), i64::MIN);
    }

    #[test]
    fn test_add_complex_sequence() {
        let mut balance = new_i64_aggregator();
        balance.add(100);
        assert_eq!(balance.read(), 100);
        balance.add(50);
        assert_eq!(balance.read(), 150);
        balance.add(-80);
        assert_eq!(balance.read(), 70);
        balance.add(-120);
        assert_eq!(balance.read(), -50);
        balance.add(-30);
        assert_eq!(balance.read(), -80);
        balance.add(80);
        assert_eq!(balance.read(), 0);
    }

    #[test]
    fn test_is_at_least() {
        let mut balance = new_i64_aggregator();
        assert!(balance.is_at_least(0));
        assert!(!balance.is_at_least(1));

        balance.add(100);
        assert!(balance.is_at_least(50));
        assert!(balance.is_at_least(100));
        assert!(!balance.is_at_least(150));

        balance.add(-200);
        assert!(!balance.is_at_least(0));
    }
}
