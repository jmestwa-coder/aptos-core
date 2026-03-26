// Copyright (c) Aptos Foundation
// Translated from: aptos_market::order_id_generation

use crate::native_perpdex::order_book_types::{new_order_id_type, OrderId};

/// Generate the next order ID from a monotonically increasing counter.
/// In Move, this uses transaction_context::monotonically_increasing_counter().
/// In native Rust, the caller must provide the counter value.
pub fn next_order_id(counter: u128) -> OrderId {
    new_order_id_type(reverse_bits(counter))
}

/// Reverse the bits in a u128 value using divide and conquer approach.
/// This is more efficient than the bit-by-bit approach, reducing from O(n) to O(log n).
pub fn reverse_bits(value: u128) -> u128 {
    let mut v = value;

    // Swap odd and even bits
    v = ((v & 0x55555555555555555555555555555555) << 1)
        | ((v >> 1) & 0x55555555555555555555555555555555);

    // Swap consecutive pairs
    v = ((v & 0x33333333333333333333333333333333) << 2)
        | ((v >> 2) & 0x33333333333333333333333333333333);

    // Swap nibbles
    v = ((v & 0x0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f) << 4)
        | ((v >> 4) & 0x0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f);

    // Swap bytes
    v = ((v & 0x00ff00ff00ff00ff00ff00ff00ff00ff) << 8)
        | ((v >> 8) & 0x00ff00ff00ff00ff00ff00ff00ff00ff);

    // Swap 2-byte chunks
    v = ((v & 0x0000ffff0000ffff0000ffff0000ffff) << 16)
        | ((v >> 16) & 0x0000ffff0000ffff0000ffff0000ffff);

    // Swap 4-byte chunks
    v = ((v & 0x00000000ffffffff00000000ffffffff) << 32)
        | ((v >> 32) & 0x00000000ffffffff00000000ffffffff);

    // Swap 8-byte chunks
    v = (v << 64) | (v >> 64);

    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverse_bits_order_id_type() {
        let order_id_1: u128 = 1;
        let order_id_2: u128 = 2;
        let order_id_3: u128 = 0x12345678;
        let order_id_4: u128 = 0x87654321ABCDEF00;

        let reversed_1 = reverse_bits(order_id_1);
        let reversed_2 = reverse_bits(order_id_2);
        let reversed_3 = reverse_bits(order_id_3);
        let reversed_4 = reverse_bits(order_id_4);

        // Test that conversion back gives original value
        assert_eq!(order_id_1, reverse_bits(reversed_1));
        assert_eq!(order_id_2, reverse_bits(reversed_2));
        assert_eq!(order_id_3, reverse_bits(reversed_3));
        assert_eq!(order_id_4, reverse_bits(reversed_4));

        // Test that reversed values are different from originals
        assert_ne!(reversed_1, order_id_1);
        assert_ne!(reversed_2, order_id_2);
        assert_ne!(reversed_3, order_id_3);
        assert_ne!(reversed_4, order_id_4);

        // Test specific bit reversal cases
        // 1 in binary: 0...0001, reversed should be 1000...0000 (high bit set)
        assert_eq!(reversed_1, 1u128 << 127);

        // 2 in binary: 0...0010, reversed should be 0100...0000
        assert_eq!(reversed_2, 1u128 << 126);

        // Test edge cases
        let order_id_zero: u128 = 0;
        let reversed_zero = reverse_bits(order_id_zero);
        assert_eq!(order_id_zero, reverse_bits(reversed_zero));
        assert_eq!(reversed_zero, 0);

        // Test maximum value
        let order_id_max: u128 = 0xffffffffffffffffffffffffffffffff;
        let reversed_max = reverse_bits(order_id_max);
        assert_eq!(order_id_max, reverse_bits(reversed_max));
        assert_eq!(reversed_max, 0xffffffffffffffffffffffffffffffff);

        // Test alternating pattern
        let order_id_alt: u128 = 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa;
        let reversed_alt = reverse_bits(order_id_alt);
        assert_eq!(order_id_alt, reverse_bits(reversed_alt));
        assert_eq!(reversed_alt, 0x55555555555555555555555555555555);

        let order_id_alt2: u128 = 0x64328946124712951320956108326756;
        assert_eq!(order_id_alt2, reverse_bits(reverse_bits(order_id_alt2)));
    }
}
