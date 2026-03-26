// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::math

use serde::{Deserialize, Serialize};

// ===================== Error helpers =====================

// Move: std::error::invalid_argument(4) produces (category << 16) | code.
// invalid_argument category = 1, so error = (1 << 16) | 4 = 65540
const EINVALID_ARGUMENT_4: u64 = (1 << 16) | 4;

// ===================== Precision struct =====================
// Note: This is a `struct` in Move (not enum), so BCS is just fields in order.

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub struct Precision {
    pub decimals: u8,
    pub multiplier: u64,
}

// ===================== Internal helpers =====================

/// ceil_div_256: ceil(x / y) for u128 (Move uses u256 but these values fit in u128 for our cases)
/// We use u128 math here. For true u256 we would need a bigger type, but the Move code
/// only ever uses this on products of u128 * u128 which fit in u256.
/// We use Rust's native u128 for the intermediate and handle overflow via checked ops.
fn ceil_div_u128(x: u128, y: u128) -> Result<u128, u64> {
    if x == 0 {
        if y == 0 {
            return Err(EINVALID_ARGUMENT_4);
        }
        return Ok(0);
    }
    // (x - 1) / y + 1
    Ok((x - 1) / y + 1)
}

// For the 256-bit version, we need actual u256 support for ceil_mul_div_128.
// We'll use a simple struct for 256-bit arithmetic.
fn mul_u128_wide(a: u128, b: u128) -> (u128, u128) {
    // Returns (high, low) of a * b as 256-bit
    let a_lo = a as u64 as u128;
    let a_hi = a >> 64;
    let b_lo = b as u64 as u128;
    let b_hi = b >> 64;

    let ll = a_lo * b_lo;
    let lh = a_lo * b_hi;
    let hl = a_hi * b_lo;
    let hh = a_hi * b_hi;

    let mid = lh + (ll >> 64);
    let mid2 = mid as u64 as u128 + hl;

    let low = ((mid2 as u64) as u128) << 64 | (ll as u64 as u128);
    let high = hh + (mid >> 64) + (mid2 >> 64);

    (high, low)
}

/// Divide a 256-bit number (hi, lo) by a u128 divisor, returning u128 quotient.
/// Panics if quotient overflows u128.
fn div_256_by_128(hi: u128, lo: u128, divisor: u128) -> u128 {
    if hi == 0 {
        return lo / divisor;
    }
    // Use long division approach
    // We need (hi * 2^128 + lo) / divisor
    // Since result must fit in u128, hi < divisor
    debug_assert!(hi < divisor, "quotient overflow");

    // We split into two 128-bit divisions
    let (q1, r1) = (hi / divisor, hi % divisor);
    // Now we need (r1 * 2^128 + lo) / divisor
    // r1 < divisor, so r1 * 2^128 + lo fits conceptually
    // We do: r1 * 2^64 chunks
    let r1_shifted_hi = r1 >> 64;
    let r1_shifted_lo = (r1 as u64 as u128) << 64;

    // (r1 * 2^128 + lo) / divisor
    // = ((r1_hi * 2^192 + r1_lo * 2^128) + lo) / divisor
    // This is complex. Let's use a simpler approach with iteration.

    // Actually for our use case, let's use a direct implementation.
    // Since hi < divisor and we need (hi * 2^128 + lo) / divisor:
    // We use the schoolbook division algorithm with 64-bit digits.
    let _ = (q1, r1, r1_shifted_hi, r1_shifted_lo);

    // Simpler: use the fact that Rust has u128 and we can do two rounds
    // Numerator = hi * 2^128 + lo
    // Split 128-bit into two 64-bit halves of hi
    // Actually let's just use a recursive approach
    div_256_impl(hi, lo, divisor)
}

fn div_256_impl(hi: u128, lo: u128, d: u128) -> u128 {
    if hi == 0 {
        return lo / d;
    }

    // Shift-based long division for 256 / 128
    let mut remainder: u128 = 0;
    let mut quotient_hi: u128 = 0;
    let mut quotient_lo: u128 = 0;

    // Process high 128 bits
    for i in (0..128).rev() {
        remainder = (remainder << 1) | ((hi >> i) & 1);
        if remainder >= d {
            remainder -= d;
            quotient_hi |= 1u128 << i;
        }
    }

    // Process low 128 bits
    for i in (0..128).rev() {
        remainder = (remainder << 1) | ((lo >> i) & 1);
        if remainder >= d {
            remainder -= d;
            quotient_lo |= 1u128 << i;
        }
    }

    // quotient = quotient_hi * 2^128 + quotient_lo, must fit in u128
    assert!(quotient_hi == 0, "quotient overflow in div_256");
    quotient_lo
}

fn ceil_div_256_wide(hi: u128, lo: u128, d: u128) -> u128 {
    if hi == 0 && lo == 0 {
        return 0;
    }
    // ceil((hi * 2^128 + lo) / d) = floor((hi * 2^128 + lo - 1) / d) + 1
    // Subtract 1 from (hi, lo)
    let (lo_sub, borrow) = lo.overflowing_sub(1);
    let hi_sub = if borrow { hi - 1 } else { hi };
    div_256_by_128(hi_sub, lo_sub, d) + 1
}

// ===================== Public functions =====================

/// ceil_mul_div_64: Returns ceil(a * b / c) going through u128
pub fn ceil_mul_div_64(a: u64, b: u64, c: u64) -> Result<u64, u64> {
    if c == 0 {
        return Err(EINVALID_ARGUMENT_4);
    }
    let numerator = (a as u128) * (b as u128);
    let denominator = c as u128;
    let result = ceil_div_u128(numerator, denominator)?;
    Ok(result as u64)
}

/// ceil_mul_div_128: Returns ceil(a * b / c) going through u256
fn ceil_mul_div_128(a: u128, b: u128, c: u128) -> Result<u128, u64> {
    if c == 0 {
        return Err(EINVALID_ARGUMENT_4);
    }
    let (hi, lo) = mul_u128_wide(a, b);
    if hi == 0 && lo == 0 {
        return Ok(0);
    }
    Ok(ceil_div_256_wide(hi, lo, c))
}

pub fn mul_div_direction_64(a: u64, b: u64, c: u64, ceil: bool) -> Result<u64, u64> {
    if ceil {
        ceil_mul_div_64(a, b, c)
    } else {
        mul_div_64(a, b, c)
    }
}

pub fn mul_div_direction_128(a: u128, b: u128, c: u128, ceil: bool) -> Result<u128, u64> {
    if ceil {
        ceil_mul_div_128(a, b, c)
    } else {
        mul_div_128(a, b, c)
    }
}

/// mul_div_round_64: Returns round(a * b / c) going through u128
pub fn mul_div_round_64(a: u64, b: u64, c: u64) -> Result<u64, u64> {
    if c == 0 {
        return Err(EINVALID_ARGUMENT_4);
    }
    let c128 = c as u128;
    let num = (a as u128) * (b as u128) + c128 / 2;
    Ok((num / c128) as u64)
}

/// mul_div_capped_64: Returns min(a * b / c, cap) going through u128
pub fn mul_div_capped_64(a: u64, b: u64, c: u64, cap: u64) -> Result<u64, u64> {
    if c == 0 {
        return Err(EINVALID_ARGUMENT_4);
    }
    let result = (a as u128) * (b as u128) / (c as u128);
    if result > (cap as u128) {
        Ok(cap)
    } else {
        Ok(result as u64)
    }
}

/// mul_div_64: Returns floor(a * b / c) going through u128. Equivalent to aptos_std::math64::mul_div
pub fn mul_div_64(a: u64, b: u64, c: u64) -> Result<u64, u64> {
    if c == 0 {
        return Err(EINVALID_ARGUMENT_4);
    }
    Ok(((a as u128) * (b as u128) / (c as u128)) as u64)
}

/// mul_div_128: Returns floor(a * b / c) going through u256
pub fn mul_div_128(a: u128, b: u128, c: u128) -> Result<u128, u64> {
    if c == 0 {
        return Err(EINVALID_ARGUMENT_4);
    }
    let (hi, lo) = mul_u128_wide(a, b);
    Ok(div_256_by_128(hi, lo, c))
}

pub fn div_direction_64(a: u64, b: u64, ceil: bool) -> Result<u64, u64> {
    if b == 0 {
        return Err(EINVALID_ARGUMENT_4);
    }
    if ceil {
        // ceil_div for u64
        if a == 0 {
            Ok(0)
        } else {
            Ok((a - 1) / b + 1)
        }
    } else {
        Ok(a / b)
    }
}

pub fn div_direction_128(a: u128, b: u128, ceil: bool) -> Result<u128, u64> {
    if b == 0 {
        return Err(EINVALID_ARGUMENT_4);
    }
    if ceil {
        if a == 0 {
            Ok(0)
        } else {
            Ok((a - 1) / b + 1)
        }
    } else {
        Ok(a / b)
    }
}

// ===================== Precision functions =====================

fn decimals_to_multiplier(decimals: u8) -> Result<u64, u64> {
    if decimals > 19 {
        return Err(EINVALID_ARGUMENT_4);
    }
    Ok(10u64.pow(decimals as u32))
}

pub fn new_precision(decimals: u8) -> Result<Precision, u64> {
    let multiplier = decimals_to_multiplier(decimals)?;
    Ok(Precision {
        decimals,
        multiplier,
    })
}

pub fn get_decimals(precision: &Precision) -> u8 {
    precision.decimals
}

pub fn get_decimals_multiplier(precision: &Precision) -> u64 {
    precision.multiplier
}

pub fn convert_decimals(
    value: u64,
    value_precision: &Precision,
    result_precision: &Precision,
    ceil: bool,
) -> Result<u64, u64> {
    if value_precision.decimals == result_precision.decimals {
        Ok(value)
    } else if value_precision.decimals > result_precision.decimals {
        div_direction_64(
            value,
            value_precision.multiplier / result_precision.multiplier,
            ceil,
        )
    } else {
        Ok(value * (result_precision.multiplier / value_precision.multiplier))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decimals_to_multiplier() {
        assert_eq!(decimals_to_multiplier(3).unwrap(), 1000);
        assert_eq!(decimals_to_multiplier(0).unwrap(), 1);

        assert_eq!(new_precision(3).unwrap().multiplier, 1000);
        assert_eq!(new_precision(0).unwrap().multiplier, 1);
    }

    #[test]
    fn test_convert_decimals() {
        let p3 = new_precision(3).unwrap();
        let p1 = new_precision(1).unwrap();

        assert_eq!(convert_decimals(12345, &p3, &p1, false).unwrap(), 123);
        assert_eq!(convert_decimals(98765, &p3, &p1, false).unwrap(), 987);
        assert_eq!(convert_decimals(98765, &p3, &p1, true).unwrap(), 988);

        assert_eq!(convert_decimals(12345, &p1, &p3, false).unwrap(), 1234500);
        assert_eq!(convert_decimals(12345, &p1, &p3, true).unwrap(), 1234500);
    }
}
