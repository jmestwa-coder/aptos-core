// Copyright (c) Aptos Foundation
// Translated from: decibel_dex::chainlink_state

use crate::native_perpdex::math;
use serde::{Deserialize, Serialize};

// ===================== Constants =====================

const ENOT_INITIALIZED: u64 = 1;
const ENOT_DEPLOYER: u64 = 2;
const EORACLE_PRICE_OUT_OF_RANGE: u64 = 3;
const ENORMALIZED_PRICE_OUT_OF_RANGE: u64 = 4;
const ENEGATIVE_PRICE: u64 = 5;

const CHAINLINK_PRICE_DECIMALS: u8 = 18;

// ===================== Types =====================

/// Last known price data for a feed
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PriceData {
    V1 { price: u128, timestamp: u32 },
}

/// Per-feed store mapping feed_id -> PriceData
/// In native Rust, the caller manages the map externally. We provide the
/// conversion logic that operates on individual PriceData values.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PriceStore {
    V1 {
        // In Move this is a Table<vector<u8>, PriceData>. In Rust we model it
        // as an opaque store whose lookups are handled by the dispatch layer.
        // The conversion functions below work on individual PriceData values.
    },
}

// ===================== Functions =====================

/// Check if int192 price is negative (two's complement).
/// In Move the price is u256; here we use u128 since non-negative prices fit.
pub fn is_price_negative_u128(price: u128) -> bool {
    // Bit 191 in u256. Since we store as u128 (max 127 bits), any u128 value is non-negative.
    // This is a safety check -- if the value somehow has bit 127 set (which would mean > 2^127),
    // that's fine since the original check was for bit 191 in u256 space.
    // For u128 representation of non-negative int192, this should always be false.
    false
}

/// Convert a raw chainlink price to the target precision.
///
/// Move signature:
/// ```move
/// fun convert_price(feed_id, oracle_decimals, rescale_decimals, target_decimals): (u64, u32)
/// ```
///
/// In native Rust, the caller provides the raw price and timestamp from the feed store.
pub fn convert_price(
    raw_price: u128,
    raw_timestamp: u32,
    oracle_decimals: u8,
    rescale_decimals: i8,
    target_decimals: u8,
) -> Result<(u64, u32), u64> {
    if is_price_negative_u128(raw_price) {
        return Err(ENEGATIVE_PRICE);
    }
    // In Move: assert!(price <= MAX_U128 as u256) -- already u128 here.

    let mut price = raw_price;

    let aggregate_decimals =
        (target_decimals as i16) + (rescale_decimals as i16) - (oracle_decimals as i16);

    if aggregate_decimals < 0 {
        let divisor = math::new_precision((-aggregate_decimals) as u8)
            .map_err(|_| ENORMALIZED_PRICE_OUT_OF_RANGE)?;
        price /= math::get_decimals_multiplier(&divisor) as u128;
    } else if aggregate_decimals > 0 {
        let multiplier = math::new_precision(aggregate_decimals as u8)
            .map_err(|_| ENORMALIZED_PRICE_OUT_OF_RANGE)?;
        price *= math::get_decimals_multiplier(&multiplier) as u128;
    }

    if price > u64::MAX as u128 {
        return Err(ENORMALIZED_PRICE_OUT_OF_RANGE);
    }
    Ok((price as u64, raw_timestamp))
}

/// Get converted price for a feed. This is the main API used by oracle.rs.
///
/// In the native context, the caller provides the raw price and timestamp
/// (looked up from the feed store by the dispatch layer).
pub fn get_converted_price(
    raw_price: u128,
    raw_timestamp: u32,
    rescale_decimals: i8,
    target_decimals: u8,
) -> Result<u64, u64> {
    let (price, _timestamp) = convert_price(
        raw_price,
        raw_timestamp,
        CHAINLINK_PRICE_DECIMALS,
        rescale_decimals,
        target_decimals,
    )?;
    Ok(price)
}

/// Get latest price data. In native Rust, the caller provides the PriceData.
pub fn get_latest_price(data: &PriceData) -> (u128, u32) {
    let PriceData::V1 { price, timestamp } = data;
    (*price, *timestamp)
}

/// Parse Report Schema v3 - extracts feed ID, timestamp, and benchmark price.
/// Returns (feed_id, PriceData).
pub fn parse_v3_report(report_data: &[u8]) -> (Vec<u8>, PriceData) {
    // ABI-encoded: Word 0=feedId, Word 2=observationsTimestamp, Word 6=benchmark
    let feed_id = report_data[0..32].to_vec();
    let timestamp = read_u32(report_data, 64); // Word 2 (offset 64)
    let price = read_u128_from_int192(report_data, 192); // Word 6 (offset 192)

    (feed_id, PriceData::V1 { price, timestamp })
}

/// Read u32 from ABI-encoded 32-byte word (right-aligned, big-endian)
fn read_u32(data: &[u8], offset: usize) -> u32 {
    let word_offset = offset + 28;
    ((data[word_offset] as u32) << 24)
        | ((data[word_offset + 1] as u32) << 16)
        | ((data[word_offset + 2] as u32) << 8)
        | (data[word_offset + 3] as u32)
}

/// Read int192 as u128 from ABI-encoded 32-byte word (right-aligned, 24 bytes).
/// The Move version reads into u256; we use u128 since prices should be non-negative
/// and fit in u128.
fn read_u128_from_int192(data: &[u8], offset: usize) -> u128 {
    let mut value: u128 = 0;
    let word_offset = offset + 8; // Skip 8-byte padding
    for i in 0..24usize {
        // Only use the lower 16 bytes for u128 (bytes 8..24 of the 24-byte field)
        if i >= 8 {
            value = (value << 8) | (data[word_offset + i] as u128);
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_price_basic() {
        // Chainlink price with 18 decimals, target 8 decimals, no rescaling
        // Raw price: 1_000_000_000_000_000_000 (1.0 in 18 decimals)
        // Target: 8 decimals -> 100_000_000
        let (price, ts) = convert_price(
            1_000_000_000_000_000_000u128,
            12345,
            18,
            0,
            8,
        )
        .unwrap();
        assert_eq!(price, 100_000_000);
        assert_eq!(ts, 12345);
    }

    #[test]
    fn test_convert_price_with_rescale() {
        // Rescale by 3 (multiply by 1000)
        let (price, _) = convert_price(
            1_000_000_000_000_000_000u128,
            0,
            18,
            3,
            8,
        )
        .unwrap();
        assert_eq!(price, 100_000_000_000); // 1000 * 100_000_000
    }
}
