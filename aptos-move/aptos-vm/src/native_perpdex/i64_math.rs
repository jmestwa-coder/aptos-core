// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::i64_math

use crate::native_perpdex::math::{self, Precision};

// ===================== Constants =====================

// Move inline functions cannot take constants, but we define them here for clarity.
// Division by zero abort code = 4
const EDIVISION_BY_ZERO: u64 = 4;

// ===================== Functions =====================

pub fn from_sign_and_amount(is_positive: bool, amount: i64) -> i64 {
    if is_positive {
        amount
    } else {
        -amount
    }
}

/// Returns a * b / c going through i128 to prevent intermediate overflow
pub fn mul_div(a: i64, b: u64, c: u64) -> Result<i64, u64> {
    if c == 0 {
        return Err(EDIVISION_BY_ZERO);
    }
    Ok(((a as i128) * (b as i128) / (c as i128)) as i64)
}

/// Returns ceil(a * b / c) for positive a, floor(|a| * b / c) negated for negative a
pub fn ceil_mul_div(a: i64, b: u64, c: u64) -> Result<i64, u64> {
    if c == 0 {
        return Err(EDIVISION_BY_ZERO);
    }
    if a >= 0 {
        let result = math::ceil_mul_div_64(a as u64, b, c)?;
        Ok(result as i64)
    } else {
        // For negative a: negate, do floor mul_div, negate back
        let abs_a = (-(a as i128)) as u64;
        let result = math::mul_div_64(abs_a, b, c)?;
        Ok((-(result as i128)) as i64)
    }
}

pub fn min(a: i64, b: i64) -> i64 {
    if a < b {
        a
    } else {
        b
    }
}

pub fn max(a: i64, b: i64) -> i64 {
    if a > b {
        a
    } else {
        b
    }
}

pub fn into_sign_and_amount(value: i64) -> (bool, u64) {
    if value >= 0 {
        (true, value as u64)
    } else {
        (false, (-value) as u64)
    }
}

pub fn into_sign_and_amount_i128(value: i128) -> (bool, u128) {
    if value >= 0 {
        (true, value as u128)
    } else {
        (false, (-value) as u128)
    }
}

/// Ceiling division for signed i64 by unsigned u64
pub fn ceil_div(x: i64, y: u64) -> Result<i64, u64> {
    if y == 0 {
        return Err(EDIVISION_BY_ZERO);
    }
    if x == 0 {
        Ok(0)
    } else if x > 0 {
        // ceil_div(x, y) = (x - 1) / y + 1
        Ok((x - 1) / (y as i64) + 1)
    } else {
        // For negative, truncation IS ceiling (rounds toward zero)
        Ok(x / (y as i64))
    }
}

/// Floor division for signed i64 by unsigned u64
fn floor_div(x: i64, y: u64) -> Result<i64, u64> {
    if y == 0 {
        return Err(EDIVISION_BY_ZERO);
    }
    let y_i64 = y as i64;
    if x >= 0 {
        Ok(x / y_i64)
    } else {
        // Round toward negative infinity
        Ok((x + 1) / y_i64 - 1)
    }
}

pub fn div_direction_64(a: i64, b: u64, ceil: bool) -> Result<i64, u64> {
    if ceil {
        ceil_div(a, b)
    } else {
        floor_div(a, b)
    }
}

pub fn convert_decimals(
    value: i64,
    value_precision: &Precision,
    result_precision: &Precision,
    ceil: bool,
) -> Result<i64, u64> {
    if math::get_decimals(value_precision) == math::get_decimals(result_precision) {
        Ok(value)
    } else if math::get_decimals(value_precision) > math::get_decimals(result_precision) {
        div_direction_64(
            value,
            math::get_decimals_multiplier(value_precision)
                / math::get_decimals_multiplier(result_precision),
            ceil,
        )
    } else {
        Ok(value
            * ((math::get_decimals_multiplier(result_precision)
                / math::get_decimals_multiplier(value_precision)) as i64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_I64: i64 = i64::MAX;
    const MAX_U64: u64 = u64::MAX;

    #[test]
    fn test_ceil_div_positive_values() {
        assert_eq!(ceil_div(10, 2).unwrap(), 5);
        assert_eq!(ceil_div(100, 10).unwrap(), 10);
        assert_eq!(ceil_div(10, 3).unwrap(), 4);
        assert_eq!(ceil_div(7, 2).unwrap(), 4);
        assert_eq!(ceil_div(1, 3).unwrap(), 1);
        assert_eq!(ceil_div(1000000, 7).unwrap(), 142858);
    }

    #[test]
    fn test_ceil_div_negative_values() {
        assert_eq!(ceil_div(-10, 2).unwrap(), -5);
        assert_eq!(ceil_div(-10, 3).unwrap(), -3);
        assert_eq!(ceil_div(-7, 2).unwrap(), -3);
        assert_eq!(ceil_div(-1, 3).unwrap(), 0);
        assert_eq!(ceil_div(-1000000, 7).unwrap(), -142857);
    }

    #[test]
    fn test_ceil_div_zero() {
        assert_eq!(ceil_div(0, 1).unwrap(), 0);
        assert_eq!(ceil_div(0, 100).unwrap(), 0);
    }

    #[test]
    fn test_ceil_div_divide_by_zero() {
        assert!(ceil_div(10, 0).is_err());
    }

    #[test]
    fn test_floor_div_positive_values() {
        assert_eq!(floor_div(10, 3).unwrap(), 3);
        assert_eq!(floor_div(7, 2).unwrap(), 3);
        assert_eq!(floor_div(10, 2).unwrap(), 5);
        assert_eq!(floor_div(1000000, 7).unwrap(), 142857);
    }

    #[test]
    fn test_floor_div_negative_values() {
        assert_eq!(floor_div(-10, 2).unwrap(), -5);
        assert_eq!(floor_div(-10, 3).unwrap(), -4);
        assert_eq!(floor_div(-7, 2).unwrap(), -4);
        assert_eq!(floor_div(-1, 3).unwrap(), -1);
        assert_eq!(floor_div(-1000000, 7).unwrap(), -142858);
    }

    #[test]
    fn test_floor_div_zero() {
        assert_eq!(floor_div(0, 1).unwrap(), 0);
        assert_eq!(floor_div(0, 100).unwrap(), 0);
    }

    #[test]
    fn test_floor_div_divide_by_zero() {
        assert!(floor_div(10, 0).is_err());
    }

    #[test]
    fn test_div_direction_64_floor() {
        assert_eq!(div_direction_64(10, 3, false).unwrap(), 3);
        assert_eq!(div_direction_64(7, 2, false).unwrap(), 3);
        assert_eq!(div_direction_64(-10, 3, false).unwrap(), -4);
        assert_eq!(div_direction_64(-7, 2, false).unwrap(), -4);
    }

    #[test]
    fn test_div_direction_64_ceil() {
        assert_eq!(div_direction_64(10, 3, true).unwrap(), 4);
        assert_eq!(div_direction_64(7, 2, true).unwrap(), 4);
        assert_eq!(div_direction_64(-10, 3, true).unwrap(), -3);
        assert_eq!(div_direction_64(-7, 2, true).unwrap(), -3);
    }

    #[test]
    fn test_div_direction_64_exact() {
        assert_eq!(div_direction_64(10, 2, false).unwrap(), 5);
        assert_eq!(div_direction_64(10, 2, true).unwrap(), 5);
        assert_eq!(div_direction_64(-10, 2, false).unwrap(), -5);
        assert_eq!(div_direction_64(-10, 2, true).unwrap(), -5);
    }

    #[test]
    fn test_ceil_mul_div_positive() {
        assert_eq!(ceil_mul_div(10, 3, 2).unwrap(), 15);
        assert_eq!(ceil_mul_div(10, 3, 4).unwrap(), 8);
        assert_eq!(ceil_mul_div(7, 5, 3).unwrap(), 12);
        assert_eq!(ceil_mul_div(1000000, 1000000, 1000000).unwrap(), 1000000);
    }

    #[test]
    fn test_ceil_mul_div_negative() {
        assert_eq!(ceil_mul_div(-10, 3, 2).unwrap(), -15);
        assert_eq!(ceil_mul_div(-10, 3, 4).unwrap(), -7);
        assert_eq!(ceil_mul_div(-7, 5, 3).unwrap(), -11);

        assert_eq!(MAX_I64 as u64, MAX_U64 / 2);
        assert_eq!(
            ceil_mul_div(-1, (MAX_I64 as u64) + 1, 1).unwrap(),
            -MAX_I64 - 1
        );
    }

    #[test]
    fn test_ceil_mul_div_zero() {
        assert_eq!(ceil_mul_div(0, 100, 50).unwrap(), 0);
        assert_eq!(ceil_mul_div(100, 0, 50).unwrap(), 0);
    }

    #[test]
    fn test_ceil_mul_div_divide_by_zero() {
        assert!(ceil_mul_div(10, 5, 0).is_err());
    }

    #[test]
    fn test_convert_decimals_same_precision() {
        let p3 = math::new_precision(3).unwrap();
        assert_eq!(convert_decimals(12345, &p3, &p3, false).unwrap(), 12345);
        assert_eq!(convert_decimals(-12345, &p3, &p3, false).unwrap(), -12345);
        assert_eq!(convert_decimals(0, &p3, &p3, false).unwrap(), 0);
    }

    #[test]
    fn test_convert_decimals_decrease_precision() {
        let p3 = math::new_precision(3).unwrap();
        let p1 = math::new_precision(1).unwrap();
        assert_eq!(convert_decimals(12345, &p3, &p1, false).unwrap(), 123);
        assert_eq!(convert_decimals(12345, &p3, &p1, true).unwrap(), 124);
        assert_eq!(convert_decimals(12300, &p3, &p1, false).unwrap(), 123);
        assert_eq!(convert_decimals(12300, &p3, &p1, true).unwrap(), 123);
    }

    #[test]
    fn test_convert_decimals_decrease_precision_negative() {
        let p3 = math::new_precision(3).unwrap();
        let p1 = math::new_precision(1).unwrap();
        assert_eq!(convert_decimals(-12345, &p3, &p1, false).unwrap(), -124);
        assert_eq!(convert_decimals(-12345, &p3, &p1, true).unwrap(), -123);
        assert_eq!(convert_decimals(-12300, &p3, &p1, false).unwrap(), -123);
        assert_eq!(convert_decimals(-12300, &p3, &p1, true).unwrap(), -123);
    }

    #[test]
    fn test_convert_decimals_increase_precision() {
        let p3 = math::new_precision(3).unwrap();
        let p1 = math::new_precision(1).unwrap();
        assert_eq!(convert_decimals(123, &p1, &p3, false).unwrap(), 12300);
        assert_eq!(convert_decimals(123, &p1, &p3, true).unwrap(), 12300);
        assert_eq!(convert_decimals(-123, &p1, &p3, false).unwrap(), -12300);
        assert_eq!(convert_decimals(-123, &p1, &p3, true).unwrap(), -12300);
    }

    #[test]
    fn test_convert_decimals_zero() {
        let p3 = math::new_precision(3).unwrap();
        let p1 = math::new_precision(1).unwrap();
        assert_eq!(convert_decimals(0, &p3, &p1, false).unwrap(), 0);
        assert_eq!(convert_decimals(0, &p3, &p1, true).unwrap(), 0);
        assert_eq!(convert_decimals(0, &p1, &p3, false).unwrap(), 0);
    }
}
