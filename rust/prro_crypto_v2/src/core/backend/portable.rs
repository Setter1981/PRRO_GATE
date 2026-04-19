//! Portable Rust implementation of GF(2^257) field multiplication.
//!
//! Schoolbook 2x2 multiplication. Same algorithm as the original
//! `gf2m_257::fmul_257`, lifted into the backend layer for dispatch.

use crate::core::fe::{Fe, FeWide, FE_WORDS};
use crate::core::gf2m::{mul_2x2, spread_bits_u32_pub};

/// Square an Fe value via bit-spreading table. Always-correct fallback.
///
/// Each 32-bit word is spread so that bit `i` moves to bit `2*i` of a 64-bit
/// result, which is the definition of polynomial squaring in GF(2).
#[inline]
pub fn fsqr_257(a: &Fe, out: &mut FeWide) {
    for v in out.0.iter_mut() {
        *v = 0;
    }
    for i in 0..FE_WORDS {
        let spread = spread_bits_u32_pub(a.0[i]);
        out.0[2 * i] = spread as u32;
        out.0[2 * i + 1] = (spread >> 32) as u32;
    }
}

/// Multiply two Fe values via portable schoolbook. Always-correct fallback.
#[inline]
pub fn fmul_257(a: &Fe, b: &Fe, out: &mut FeWide) {
    for v in out.0.iter_mut() {
        *v = 0;
    }
    let a_words = &a.0;
    let b_words = &b.0;

    let mut j = 0;
    while j < FE_WORDS {
        let y0 = b_words[j];
        let y1 = if j + 1 == FE_WORDS { 0 } else { b_words[j + 1] };

        let mut i = 0;
        while i < FE_WORDS {
            let x0 = a_words[i];
            let x1 = if i + 1 == FE_WORDS { 0 } else { a_words[i + 1] };

            let x22 = mul_2x2(x1, x0, y1, y0);
            out.0[j + i + 0] ^= x22[0];
            out.0[j + i + 1] ^= x22[1];
            out.0[j + i + 2] ^= x22[2];
            out.0[j + i + 3] ^= x22[3];

            i += 2;
        }
        j += 2;
    }
}
