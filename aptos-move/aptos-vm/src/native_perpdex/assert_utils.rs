// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::assert_utils
// This module is #[test_only] in Move. Gated behind #[cfg(test)] here.

#[cfg(test)]
pub fn assert_eq<T: PartialEq + std::fmt::Debug>(actual: T, expected: T) {
    if expected != actual {
        eprintln!("assert_eq: actual: {:?}, expected: {:?}", actual, expected);
        panic!("assertion failed with code 1");
    }
}

#[cfg(test)]
pub fn assert_eq_msg<T: PartialEq + std::fmt::Debug, M: std::fmt::Debug>(
    actual: T,
    expected: T,
    message: M,
) {
    if expected != actual {
        eprintln!(
            "assert_eq: actual: {:?}, expected: {:?}, message: {:?}",
            actual, expected, message
        );
        panic!("assertion failed with code 1");
    }
}

#[cfg(test)]
pub fn assert_eq_u64(actual: u64, expected: u64) {
    if expected != actual {
        eprintln!("assert_eq: actual: {}, expected: {}", actual, expected);
        panic!("assertion failed with code {}", actual);
    }
}
