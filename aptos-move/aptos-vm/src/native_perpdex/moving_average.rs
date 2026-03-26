// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::moving_average

use crate::native_perpdex::math;
use serde::{Deserialize, Serialize};

// ===================== Constants =====================

// Move: error::invalid_argument(1) = (1 << 16) | 1
const EINVALID_LOOKBACK_WINDOW: u64 = (1 << 16) | 1;
// Move: error::invalid_argument(2) = (1 << 16) | 2
const EINVALID_PRICE_IS_ZERO: u64 = (1 << 16) | 2;

const ADDITIVE_PRECISION: u64 = 100_000_000; // 10^8
const MULTIPLICATIVE_PRECISION: u64 = 1_000_000_000_000; // 10^12

/// ln(PRECISION) rounded down.
const LN_PRECISION: u64 = 18;

const BASIS_POINTS_MULTIPLIER: u64 = 10_000;

/// Maximum lookback window in seconds (1 year)
const MAX_LOOKBACK_WINDOW: u64 = 31_536_000;

/// Minimum lookback window in seconds (10 seconds)
const MIN_LOOKBACK_WINDOW: u64 = 10;

const US_IN_SECOND: u64 = 1_000_000;

// ===================== Types =====================

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum MovingAverage {
    EMA {
        /// EMA value (fixed-point with PRECISION)
        ema: u64,
        /// Lookback window in seconds
        lookback_window_seconds: u64,
        /// Timestamp of the last observation
        last_observation_time_us: u64,
        /// Number of observations made
        observation_count: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum DeviationMovingAverage {
    Ratio {
        ratio_moving_average: MovingAverage,
    },
}

// ===================== FixedPoint32 helpers =====================
// Move's fixed_point32 is a u64 storing a 32.32 fixed-point number.
// fixed_point32::create_from_rational(a, b) = (a << 32) / b (with rounding)
// math_fixed::exp computes e^x for a FixedPoint32.

/// Represents a 32.32 fixed-point number stored as u64.
/// The value represented is raw_value / 2^32.
fn fixed_point32_create_from_rational(numerator: u64, denominator: u64) -> u64 {
    // Move implementation: ((numerator as u128) << 64) / ((denominator as u128) << 32)
    // Which simplifies to: ((numerator as u128) << 32) / (denominator as u128)
    // Then cast to u64.
    if denominator == 0 {
        panic!("fixed_point32: division by zero");
    }
    let result = ((numerator as u128) << 32) / (denominator as u128);
    // Move aborts if result doesn't fit in u64
    result as u64
}

/// Divide u64 by a fixed-point32 value: a / fp = a * 2^32 / fp.raw
fn fixed_point32_divide_u64(a: u64, fp: u64) -> u64 {
    if fp == 0 {
        panic!("fixed_point32: division by zero");
    }
    (((a as u128) << 32) / (fp as u128)) as u64
}

/// Compute e^x where x is a FixedPoint32.
/// This is a Rust implementation of Move's math_fixed::exp function.
/// Uses Taylor series: e^x = 1 + x + x^2/2! + x^3/3! + ...
/// Input and output are FixedPoint32 (raw u64 with 32 fractional bits).
fn math_fixed_exp(x: u64) -> u64 {
    // FixedPoint32 representation: value = raw / 2^32
    // We need to compute e^(x / 2^32) and return as FixedPoint32
    //
    // Move's implementation uses a lookup table + Taylor series.
    // We replicate the same approach.
    //
    // The Move std::math_fixed::exp splits x into integer and fractional parts:
    // e^x = e^(integer_part) * e^(fractional_part)
    //
    // For the integer part, use precomputed values.
    // For the fractional part, use Taylor series.

    let one: u128 = 1u128 << 32;

    // If x == 0, return 1.0
    if x == 0 {
        return one as u64;
    }

    let x128 = x as u128;

    // Split into integer and fractional parts
    // integer_part = x >> 32 (the whole number part)
    // frac_part = x & 0xFFFFFFFF (the fractional part)
    let int_part = (x128 >> 32) as u32;
    let frac_raw = (x128 & 0xFFFF_FFFF) as u64;

    // Compute e^(integer_part) using precomputed table
    // e^0 = 1, e^1 ≈ 2.718..., e^2 ≈ 7.389..., etc.
    // As FixedPoint32: e^1 = 2.718281828 * 2^32 ≈ 11674931554
    // We limit to reasonable range. If int_part > 19, the result overflows u64.
    let e_powers: [u128; 20] = [
        1u128 << 32,                  // e^0 = 1.0
        11_674_931_554,               // e^1 ≈ 2.71828
        31_723_502_206,               // e^2 ≈ 7.38906
        86_228_223_028,               // e^3 ≈ 20.08554
        234_397_175_891,              // e^4 ≈ 54.59815
        637_168_811_498,              // e^5 ≈ 148.41316
        1_731_782_975_498,            // e^6 ≈ 403.42879
        4_706_897_774_387,            // e^7 ≈ 1096.63316
        12_793_204_786_662,           // e^8 ≈ 2980.95799
        34_770_609_685_449,           // e^9 ≈ 8103.08393
        94_528_032_188_227,           // e^10 ≈ 22026.46579
        256_926_822_858_498,          // e^11 ≈ 59874.14172
        698_413_346_998_011,          // e^12 ≈ 162754.79142
        1_898_556_940_958_583,        // e^13 ≈ 442413.39201
        5_160_415_435_498_686,        // e^14 ≈ 1202604.28416
        14_026_108_428_641_994,       // e^15 ≈ 3269017.37247
        38_127_280_945_498_060,       // e^16 ≈ 8886110.52051
        103_636_334_536_041_578,      // e^17 ≈ 24154952.75358
        281_731_383_689_083_625,      // e^18 ≈ 65659969.13733
        765_714_854_041_498_050,      // e^19 ≈ 178482300.96319
    ];

    if int_part >= 20 {
        // Overflow - Move would abort. Return max.
        return u64::MAX;
    }

    let e_int = e_powers[int_part as usize];

    // Compute e^(fractional_part) using Taylor series
    // e^f = 1 + f + f^2/2! + f^3/3! + ... where f is in [0, 1)
    // f is stored as frac_raw / 2^32
    let f = frac_raw as u128;

    // Taylor series terms (in fixed-point 2^32 representation)
    // term_0 = 1 (= 2^32)
    // term_n = term_{n-1} * f / (n * 2^32)
    let mut result: u128 = one; // 1.0
    let mut term: u128 = one; // current term

    for n in 1u128..=12 {
        // 12 terms gives good precision for f < 1
        term = term * f / (n * one);
        result += term;
        if term == 0 {
            break;
        }
    }

    // Multiply: e^int * e^frac
    // Both are in fixed-point 2^32, so multiply and shift right by 32
    let combined = (e_int * result) >> 32;

    if combined > u64::MAX as u128 {
        u64::MAX
    } else {
        combined as u64
    }
}

// ===================== Functions =====================

pub fn new_ema(lookback_window_seconds: u64) -> Result<MovingAverage, u64> {
    if lookback_window_seconds < MIN_LOOKBACK_WINDOW {
        return Err(EINVALID_LOOKBACK_WINDOW);
    }
    if lookback_window_seconds > MAX_LOOKBACK_WINDOW {
        return Err(EINVALID_LOOKBACK_WINDOW);
    }

    Ok(MovingAverage::EMA {
        ema: 0,
        lookback_window_seconds,
        last_observation_time_us: 0,
        observation_count: 0,
    })
}

pub fn new_ratio_ema(
    lookback_window_seconds: u64,
) -> Result<DeviationMovingAverage, u64> {
    Ok(DeviationMovingAverage::Ratio {
        ratio_moving_average: new_ema(lookback_window_seconds)?,
    })
}

fn has_estimate(ma: &MovingAverage) -> bool {
    let MovingAverage::EMA {
        observation_count, ..
    } = ma;
    *observation_count > 0
}

fn should_discard_observation(ma: &MovingAverage, timestamp_us: u64) -> bool {
    let MovingAverage::EMA {
        observation_count,
        last_observation_time_us,
        ..
    } = ma;
    *observation_count > 0 && timestamp_us <= *last_observation_time_us
}

pub fn add_moving_average_observation(
    ma: &mut MovingAverage,
    observation: u64,
    timestamp_us: u64,
) {
    if should_discard_observation(ma, timestamp_us) {
        return;
    }

    let MovingAverage::EMA {
        ema,
        lookback_window_seconds,
        last_observation_time_us,
        observation_count,
    } = ma;

    if *observation_count == 0 {
        *ema = observation;
    } else {
        let alpha = calculate_alpha(
            *lookback_window_seconds,
            timestamp_us - *last_observation_time_us,
        );

        // alpha_term = alpha * observation / ADDITIVE_PRECISION
        let alpha_term = math::mul_div_64(alpha, observation, ADDITIVE_PRECISION)
            .expect("mul_div_64 failed in EMA calculation");
        // complement_term = (ADDITIVE_PRECISION - alpha) * ema / ADDITIVE_PRECISION
        let complement_term =
            math::mul_div_64(ADDITIVE_PRECISION - alpha, *ema, ADDITIVE_PRECISION)
                .expect("mul_div_64 failed in EMA calculation");

        *ema = alpha_term + complement_term;
    }

    *last_observation_time_us = timestamp_us;
    *observation_count += 1;
}

pub fn get_moving_average_value(ma: &MovingAverage) -> u64 {
    let MovingAverage::EMA { ema, .. } = ma;
    *ema
}

pub fn add_deviation_observation(
    dma: &mut DeviationMovingAverage,
    base_px: u64,
    actual_px: u64,
    timestamp_us: u64,
    deviation_cap_bps: u64,
) -> Result<(), u64> {
    if base_px == 0 {
        return Err(EINVALID_PRICE_IS_ZERO);
    }
    if actual_px == 0 {
        return Err(EINVALID_PRICE_IS_ZERO);
    }

    let min_ratio = math::mul_div_64(
        MULTIPLICATIVE_PRECISION,
        BASIS_POINTS_MULTIPLIER,
        BASIS_POINTS_MULTIPLIER + deviation_cap_bps,
    )?;
    let max_ratio = math::mul_div_64(
        MULTIPLICATIVE_PRECISION,
        BASIS_POINTS_MULTIPLIER + deviation_cap_bps,
        BASIS_POINTS_MULTIPLIER,
    )?;

    let mut ratio_observation =
        math::mul_div_capped_64(actual_px, MULTIPLICATIVE_PRECISION, base_px, max_ratio)?;

    if ratio_observation < min_ratio {
        ratio_observation = min_ratio;
    }

    let DeviationMovingAverage::Ratio {
        ratio_moving_average,
    } = dma;
    add_moving_average_observation(ratio_moving_average, ratio_observation, timestamp_us);
    Ok(())
}

pub fn get_ratio_estimated_value(
    dma: &DeviationMovingAverage,
    base_px: u64,
) -> Result<u64, u64> {
    let DeviationMovingAverage::Ratio {
        ratio_moving_average,
    } = dma;

    if !has_estimate(ratio_moving_average) {
        return Ok(base_px);
    }

    math::mul_div_round_64(
        base_px,
        get_moving_average_value(ratio_moving_average),
        MULTIPLICATIVE_PRECISION,
    )
}

/// Calculate alpha factor for EMA based on lookback window
/// Alpha = 1 - e^(-delta_t/lookback_window)
fn calculate_alpha(lookback_window_seconds: u64, time_elapsed_us: u64) -> u64 {
    let lookback_window_us = lookback_window_seconds * US_IN_SECOND;

    // If time elapsed is much larger than lookback window, return 100%
    if time_elapsed_us > LN_PRECISION * lookback_window_us {
        return ADDITIVE_PRECISION;
    }

    // time_exponent = e^(time_elapsed_us / lookback_window_us) as FixedPoint32
    let time_exponent = math_fixed_exp(fixed_point32_create_from_rational(
        time_elapsed_us,
        lookback_window_us,
    ));

    // alpha = 1 - 1/e^x = 1 - ADDITIVE_PRECISION / time_exponent
    ADDITIVE_PRECISION - fixed_point32_divide_u64(ADDITIVE_PRECISION, time_exponent)
}
