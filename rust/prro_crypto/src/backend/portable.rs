//! Portable Rust implementation of GF(2^257) field multiplication.
//!
//! Schoolbook 2x2 multiplication. Same algorithm as the original
//! `gf2m_257::fmul_257`, lifted into the backend layer for dispatch.

use crate::fe::{Fe, FeWide, FE_WORDS};
use crate::gf2m::mul_2x2;

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
