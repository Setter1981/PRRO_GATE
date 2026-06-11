//! GF(2^m) polynomial arithmetic for DSTU 4145 binary-field elliptic curves.
//!
//! This is a direct port of jkurwa/lib/gf2m.js (BSD-licensed, by Ilya Petrov).
//! Semantics and byte-level output must match jkurwa exactly for test-vector parity.
//!
//! Representation: polynomial coefficients stored as little-endian u32 words.
//! - `bytes[0]` holds bits 0..31 (coefficient of x^0..x^31)
//! - `bytes[1]` holds bits 32..63
//! - etc.
//!
//! Irreducible polynomial `p` is represented as exponents in descending order,
//! terminated by 0: e.g. x^257 + x^12 + 1 = [257, 12, 0].
//! The terminating 0 represents the constant term (x^0).

/// Bit width of one word.
const BITS: u32 = 32;

/// Compute the bit length of a polynomial (index of highest set bit + 1).
///
/// Port of jkurwa/gf2m.js:blength.
/// Mirrors the tree of shifts used there to match behavior byte-for-byte.
pub fn blength(bytes: &[u32]) -> u32 {
    let mut r: u32 = 1;
    let mut nz: isize = (bytes.len() as isize) - 1;

    while nz >= 0 && bytes[nz as usize] == 0 {
        nz -= 1;
    }

    if nz < 0 {
        // all zeros: jkurwa returns 1 + (-1) * 32 = -31. For a field element
        // this never happens in practice (operands are non-zero), but we match
        // by returning 0 for true-zero input. Callers must treat 0 as "empty".
        return 0;
    }

    let mut x = bytes[nz as usize];
    let mut t;

    t = x >> 16;
    if t != 0 {
        x = t;
        r += 16;
    }
    t = x >> 8;
    if t != 0 {
        x = t;
        r += 8;
    }
    t = x >> 4;
    if t != 0 {
        x = t;
        r += 4;
    }
    t = x >> 2;
    if t != 0 {
        x = t;
        r += 2;
    }
    t = x >> 1;
    if t != 0 {
        r += 1;
    }

    r + (nz as u32) * 32
}

/// Right-shift a polynomial by `right` bits.
///
/// Port of jkurwa/gf2m.js:shiftRight.
/// If `inplace` is true, the input buffer is mutated; otherwise a new buffer
/// of the same length is returned. Semantics and mask behavior must match
/// jkurwa exactly.
pub fn shift_right(bytes: &[u32], right: u32, inplace_buf: Option<&mut Vec<u32>>) -> Vec<u32> {
    let wright = (right / 32) as usize;
    let right_mod = right % 32;
    let left = 32 - right_mod;

    // jkurwa builds a mask equal to (1 << (1 + right_mod)) - 1 unless right_mod == 31
    // then mask_f = 0xffffffff. This is the mask of the low bits that will be
    // shifted into the previous word.
    let mask_f: u32 = if right_mod == 31 {
        0xffff_ffff
    } else {
        // (1 << (1 + right_mod)) - 1 — masks the low (right_mod + 1) bits
        // Note: when right_mod == 0, mask is 0x1, so tmp = bytes[idx] & 1.
        // Then tmp << left == tmp << 32, which in JS is undefined-ish; but jkurwa
        // happens to guard with `if (right)` semantics implicitly because for
        // right_mod==0 the `_rbytes[idx-1] |= tmp << left` is `tmp << 32`
        // which JS computes as `tmp << 0 == tmp`. This is a known jkurwa quirk
        // for right==0 case but in practice shift_right is never called with right==0
        // (the higher-level Field code guards against that). We mirror by using
        // Rust's wrapping_shl to avoid overflow panic; but we also handle
        // right_mod==0 explicitly below.
        (1u32 << (1 + right_mod)) - 1
    };

    let blen = bytes.len();

    // Temporary buffer for computation; use provided inplace buffer if any.
    let out: Vec<u32> = if let Some(buf) = inplace_buf {
        // Caller provided mutable buffer; ensure size matches
        debug_assert_eq!(buf.len(), blen);
        // We'll write into this buffer; copy values first to avoid aliasing issues
        // because jkurwa's inplace path reads _bytes[idx] and writes _rbytes[idx]
        // where _rbytes IS _bytes. We simulate by using a tmp initial copy.
        let v = buf.clone();
        *buf = vec![0; blen];
        // Fall through using `out` as local, then copy back at the end.
        // Actually, jkurwa reads from _bytes[idx] AFTER writing _rbytes[idx] = _bytes[idx] >>> right
        // but it writes to _rbytes[idx-1] |= tmp << left where tmp = _bytes[idx] & mask_f.
        // Since _rbytes[idx-1] was already written in prior iteration, the aliasing is
        // between reads of _bytes[idx] and writes to _rbytes[idx]. Those refer to same slot.
        // Safe because we read _bytes[idx] into `tmp` BEFORE writing _rbytes[idx].
        // But for OUR Rust port we handle inplace by swapping at end; simpler.
        v
    } else {
        bytes.to_vec()
    };

    // First we compute the bit-shift portion (without word-shift).
    // jkurwa code:
    //   _rbytes[0] = _bytes[0] >>> right;
    //   for (idx = 1; idx < blen; idx++) {
    //     tmp = _bytes[idx] & mask_f;
    //     _rbytes[idx] = _bytes[idx] >>> right;
    //     _rbytes[idx - 1] |= tmp << left;
    //   }
    //
    // Here `right` is `right_mod` (after `right %= 32`).
    let src = out.clone();
    let mut dst: Vec<u32> = vec![0; blen];

    if right_mod == 0 {
        // Special case: no bit shift, only word shift (below). Copy as-is.
        dst.copy_from_slice(&src);
    } else {
        dst[0] = src[0] >> right_mod;
        for idx in 1..blen {
            let tmp = src[idx] & mask_f;
            dst[idx] = src[idx] >> right_mod;
            dst[idx - 1] |= tmp << left;
        }
    }

    // Now apply word shift: shift array elements down by `wright` positions,
    // filling top with zeros.
    if wright > 0 {
        for idx in 0..blen {
            dst[idx] = if idx + wright < blen {
                dst[idx + wright]
            } else {
                0
            };
        }
    }

    dst
}

/// Multiply two 32-bit polynomials over GF(2). Result is a 64-bit polynomial
/// returned as `(low, high)` words.
///
/// Port of jkurwa/gf2m.js:mul_1x1.
/// Uses a 3-bit windowed lookup table method identical to the reference.
#[inline]
pub fn mul_1x1(a: u32, b: u32) -> (u32, u32) {
    let top2b = a >> 30;

    let a1 = a & 0x3fff_ffff;
    let a2 = a1 << 1;
    let a4 = a2 << 1;

    let tab: [u32; 8] = [0, a1, a2, a1 ^ a2, a4, a1 ^ a4, a2 ^ a4, a1 ^ a2 ^ a4];

    let mut s;
    let mut l;
    let mut h;

    s = tab[(b & 0x7) as usize];
    l = s;
    // For s==0 or when the shifts exceed 31 we'd invoke wrapping; use
    // wrapping_shl/shr to mirror JS `<<` / `>>>` semantics where over-shifts
    // produce 0 when the shift is 32 (JS does x << 32 == x actually due to
    // modulo 32). But in this algorithm shifts are always 0..32, so no wrap.
    s = tab[((b >> 3) & 0x7) as usize];
    l ^= s << 3;
    h = s >> 29;
    s = tab[((b >> 6) & 0x7) as usize];
    l ^= s << 6;
    h ^= s >> 26;
    s = tab[((b >> 9) & 0x7) as usize];
    l ^= s << 9;
    h ^= s >> 23;
    s = tab[((b >> 12) & 0x7) as usize];
    l ^= s << 12;
    h ^= s >> 20;
    s = tab[((b >> 15) & 0x7) as usize];
    l ^= s << 15;
    h ^= s >> 17;
    s = tab[((b >> 18) & 0x7) as usize];
    l ^= s << 18;
    h ^= s >> 14;
    s = tab[((b >> 21) & 0x7) as usize];
    l ^= s << 21;
    h ^= s >> 11;
    s = tab[((b >> 24) & 0x7) as usize];
    l ^= s << 24;
    h ^= s >> 8;
    s = tab[((b >> 27) & 0x7) as usize];
    l ^= s << 27;
    h ^= s >> 5;
    s = tab[(b >> 30) as usize];
    l ^= s << 30;
    h ^= s >> 2;

    // Handle top two bits of `a` that were masked off by & 0x3fffffff.
    if top2b & 1 != 0 {
        l ^= b << 30;
        h ^= b >> 2;
    }
    if top2b & 2 != 0 {
        l ^= b << 31;
        h ^= b >> 1;
    }

    (l, h)
}

/// Multiply two 64-bit polynomials (each two u32 words) over GF(2).
/// Returns a 4-word (128-bit) result.
///
/// Port of jkurwa/gf2m.js:mul_2x2. Uses Karatsuba-style decomposition.
/// Input: `a = a1:a0`, `b = b1:b0` (each two words, little-endian).
/// Output: `[r0, r1, r2, r3]` (little-endian 4-word result).
///
/// The reference JS uses a 6-element scratch array; we return a 4-element
/// array because positions [4] and [5] are always zeroed before return.
pub fn mul_2x2(a1: u32, a0: u32, b1: u32, b0: u32) -> [u32; 4] {
    // Compute three 64-bit products:
    // low   = a0 * b0
    // high  = a1 * b1
    // mid   = (a0^a1) * (b0^b1)
    let (lo_l, lo_h) = mul_1x1(a0, b0);
    let (hi_l, hi_h) = mul_1x1(a1, b1);
    let (mid_l, mid_h) = mul_1x1(a0 ^ a1, b0 ^ b1);

    // jkurwa layout:
    //   ret[0..2] = a0*b0 (low:high)     -> [lo_l, lo_h, ...]
    //   ret[2..4] = a1*b1 (low:high)     -> [..., ..., hi_l, hi_h]
    //   ret[4..6] = mid (low:high)       -> [..., ..., ..., ..., mid_l, mid_h]
    // then:
    //   ret[2] ^= ret[5] ^ ret[1] ^ ret[3];
    //   ret[1] = ret[3] ^ ret[2] ^ ret[0] ^ ret[4] ^ ret[5];
    //   ret[4] = 0; ret[5] = 0;
    //
    // Let's work it out step by step to match exactly:
    // Initial: ret = [lo_l, lo_h, hi_l, hi_h, mid_l, mid_h]
    //
    // ret[2] ^= ret[5] ^ ret[1] ^ ret[3]
    //        = hi_l ^ mid_h ^ lo_h ^ hi_h
    //
    // NEW ret[1] = ret[3] ^ new_ret[2] ^ ret[0] ^ ret[4] ^ ret[5]
    //            = hi_h ^ (hi_l ^ mid_h ^ lo_h ^ hi_h) ^ lo_l ^ mid_l ^ mid_h
    //            = hi_l ^ lo_h ^ lo_l ^ mid_l
    //   (hi_h cancels, mid_h cancels)
    //
    // Final: [lo_l, (hi_l ^ lo_h ^ lo_l ^ mid_l), (hi_l ^ mid_h ^ lo_h ^ hi_h), hi_h]
    let r2 = hi_l ^ mid_h ^ lo_h ^ hi_h;
    let r1 = hi_h ^ r2 ^ lo_l ^ mid_l ^ mid_h;
    // Double-check: the JS statement evaluates ret[1] using the ALREADY-UPDATED ret[2].
    // So r1 uses r2 (new value). That's what we have above.
    [lo_l, r1, r2, hi_h]
}

/// Public re-export of bit-spreading primitive for the specialized 257-bit backend.
#[inline]
pub fn spread_bits_u32_pub(x: u32) -> u64 {
    spread_bits_u32(x)
}

/// Spread the bits of a u32 into a u64 by inserting a zero between each bit.
///
/// Used for fast GF(2) squaring: `(sum a_i * x^i)^2 = sum a_i * x^(2i)` in GF(2),
/// because the cross terms 2*a_i*a_j vanish in characteristic 2. So squaring is
/// just bit-spreading.
///
/// Branchless implementation using the "swizzle" trick from Hacker's Delight.
#[inline]
fn spread_bits_u32(x: u32) -> u64 {
    let mut v = x as u64;
    v = (v | (v << 16)) & 0x0000_FFFF_0000_FFFF;
    v = (v | (v << 8)) & 0x00FF_00FF_00FF_00FF;
    v = (v | (v << 4)) & 0x0F0F_0F0F_0F0F_0F0F;
    v = (v | (v << 2)) & 0x3333_3333_3333_3333;
    v = (v | (v << 1)) & 0x5555_5555_5555_5555;
    v
}

/// Polynomial squaring over GF(2). Specialized: O(n) instead of O(n^2) via
/// bit-spreading. Result lives in `s` (overwritten, NOT XOR-accumulated).
///
/// `s.len()` must be at least `2 * a.len()`.
pub fn fsqr(a: &[u32], s: &mut [u32]) {
    debug_assert!(s.len() >= 2 * a.len(), "output must be at least 2x input");
    for v in s.iter_mut() {
        *v = 0;
    }
    for (i, &word) in a.iter().enumerate() {
        let spread = spread_bits_u32(word);
        s[2 * i] = spread as u32;
        s[2 * i + 1] = (spread >> 32) as u32;
    }
}

/// Polynomial multiplication over GF(2). Writes result to `s` (XOR-accumulated).
///
/// Port of jkurwa/gf2m.js:fmul.
/// Requires `s.len() >= a.len() + b.len() + 2`. The extra slack accommodates
/// the 2-word alignment of the inner loop: for odd-length inputs, the last
/// iteration writes 4 words starting at position `(a.len()-1) + (b.len()-1)`.
pub fn fmul(a: &[u32], b: &[u32], s: &mut [u32]) {
    for v in s.iter_mut() {
        *v = 0;
    }

    let a_len = a.len();
    let b_len = b.len();

    // Process pairs of words from b and a.
    let mut j = 0;
    while j < b_len {
        let y0 = b[j];
        let y1 = if j + 1 == b_len { 0 } else { b[j + 1] };

        let mut i = 0;
        while i < a_len {
            let x0 = a[i];
            let x1 = if i + 1 == a_len { 0 } else { a[i + 1] };

            let x22 = mul_2x2(x1, x0, y1, y0);
            s[j + i] ^= x22[0];
            s[j + i + 1] ^= x22[1];
            s[j + i + 2] ^= x22[2];
            s[j + i + 3] ^= x22[3];

            i += 2;
        }
        j += 2;
    }
}

/// Reduce polynomial `a` modulo irreducible polynomial `p`.
///
/// Port of jkurwa/gf2m.js:fmod.
/// `p` is an array of exponents in descending order terminated by 0, e.g.
/// x^257 + x^12 + 1 = [257, 12, 0]. The `0` terminator marks the constant
/// term and also serves as a "stop" sentinel for the reducing loop.
///
/// `ret` is mutated in place; it must initially contain `a` (caller responsibility,
/// or pass `None` for a fresh copy — but we require the buffer to match `a.len()`).
///
/// Returns nothing; result lives in `ret`.
pub fn fmod(a: &[u32], p: &[u32], ret: &mut [u32]) {
    let ret_len = a.len();
    debug_assert_eq!(ret.len(), ret_len);

    // Copy input into ret (the reference lets the caller pre-populate, but
    // we require explicit initialization here).
    ret[..ret_len].copy_from_slice(&a[..ret_len]);

    let dn = (p[0] / BITS) as usize;

    // Main reduction loop: walk down from high words, clearing each non-zero word
    // by XORing in its contribution shifted to the appropriate positions.
    let mut j: isize = (ret_len as isize) - 1;
    while j > dn as isize {
        let zz = ret[j as usize];
        if zz == 0 {
            j -= 1;
            continue;
        }
        ret[j as usize] = 0;

        // Reduce via each non-leading term of p (i.e., p[1], p[2], ...) until
        // we hit the 0 terminator which represents x^0 (constant term).
        let mut k = 1usize;
        while k < p.len() && p[k] != 0 {
            // Reducing component t^p[k]: the leading coefficient zz*x^(word*32)
            // must be XORed back in at x^(word*32 - (p[0] - p[k])) after reduction.
            let n_exp = p[0] - p[k];
            let d0 = n_exp % BITS;
            let d1 = BITS - d0;
            let n = (n_exp / BITS) as usize;
            ret[(j as usize) - n] ^= zz >> d0;
            if d0 != 0 {
                ret[(j as usize) - n - 1] ^= zz << d1;
            }
            k += 1;
        }

        // Reducing component t^0 (the constant term from the implicit +1).
        let n = dn;
        let d0 = p[0] % BITS;
        let d1 = BITS - d0;
        ret[(j as usize) - n] ^= zz >> d0;
        if d0 != 0 {
            ret[(j as usize) - n - 1] ^= zz << d1;
        }
        // j is not decremented here: the jkurwa loop re-tests ret[j] (now possibly
        // non-zero again if the reduction contributed back). Actually wait — in
        // jkurwa, ret[j] was set to 0 at the top of the loop. The reduction adds
        // back XORs at positions ret[j-n], ret[j-n-1], etc. — all at positions < j
        // (for k=1, n_exp = p[0] - p[k] > 0 so n could equal dN-1 at most,
        // and for k=0 term, n == dN so j-n == j-dN > 0 since j > dN).
        // So ret[j] stays 0 and we move on. jkurwa advances via the implicit j-- in
        // the `if (ret[j] === 0) { j--; continue; }` path on next iteration.
        // But actually: after setting ret[j]=0, the next iteration hits the
        // `if (ret[j] === 0)` branch and decrements. Let's mirror precisely:
        // we don't decrement here either; the next loop iteration will.
        // BUT: we're in an infinite loop risk if the zero check is the only way out.
        // The jkurwa `for` loop has no natural end; it relies on ret[j] becoming zero
        // and j-- via the continue. We mirror exactly.
    }

    // Final round of reduction for j == dN (the word containing the high bit of p[0]).
    // Clears any bits above p[0] % BITS in ret[dN] by folding them down using the
    // full polynomial p.
    loop {
        let d0_full = p[0] % BITS;
        let zz = ret[dn] >> d0_full;
        if zz == 0 {
            break;
        }
        let d1_full = BITS - d0_full;

        // Clear top d1 bits of ret[dN].
        if d0_full != 0 {
            ret[dn] = (ret[dn] << d1_full) >> d1_full;
        } else {
            ret[dn] = 0;
        }

        // Apply reduction: t^0 component first.
        ret[0] ^= zz;

        // Each non-leading term of p: t^p[k] component.
        let mut k = 1usize;
        while k < p.len() && p[k] != 0 {
            let n = (p[k] / BITS) as usize;
            let d0 = p[k] % BITS;
            let d1 = BITS - d0;
            ret[n] ^= zz << d0;
            // tmp = zz >>> d1. In jkurwa, d1 can be 32 when d0==0, which in JS
            // becomes a no-op shift (>>> 32 == >>> 0 in JS). We mirror by
            // guarding with `if d0 != 0` like jkurwa does: `if (d0 && tmp_ulong)`.
            if d0 != 0 {
                let tmp_ulong = zz >> d1;
                if tmp_ulong != 0 {
                    ret[n + 1] ^= tmp_ulong;
                }
            }
            k += 1;
        }
    }
}

/// Modular inverse in GF(2^m) using the binary Euclidean algorithm.
///
/// Port of jkurwa/gf2m.js:finv.
/// `a` is the element to invert (modulo the irreducible polynomial represented
/// by `p`); result is written to `ret`.
///
/// Pre-condition: `a` must be already reduced (a.len() == ret.len()).
/// `p` is expressed in the SAME word form as `a` (i.e., the polynomial itself
/// as a u32 array), NOT as an exponent list! This is a different convention
/// from `fmod`.
///
/// NOTE: This differs from jkurwa — in jkurwa, `p` is also a word-form array
/// for finv specifically (see field.js: `var p = this.curve.calc_modulus(this.mod_bits);`).
/// The exponent-form is only for fmod.
pub fn finv(a_in: &[u32], p: &[u32], ret: &mut [u32]) {
    let n = a_in.len();
    debug_assert_eq!(ret.len(), n);
    debug_assert_eq!(p.len(), n);

    // Working buffers (mutable).
    let mut u: Vec<u32> = a_in.to_vec();
    let mut v: Vec<u32> = p.to_vec();
    let mut b: Vec<u32> = vec![0; n];
    let mut c: Vec<u32> = vec![0; n];
    b[0] = 1;

    let mut ubits = blength(&u) as i64;
    let mut vbits = blength(&v) as i64;

    loop {
        if ubits < 0 {
            panic!("finv: internal error (ubits negative)");
        }

        // While u is even (low bit 0), divide u by x and b by x (XORing p into b first if b is odd).
        while ubits > 0 && (u[0] & 1) == 0 {
            let mut u0 = u[0];
            let mut b0 = b[0];

            let mask: u32 = if (b0 & 1) != 0 { 0xffff_ffff } else { 0 };
            b0 ^= p[0] & mask;

            let mut idx = 0usize;
            while idx < p.len() - 1 {
                let u1 = u[idx + 1];
                u[idx] = (u0 >> 1) | (u1 << 31);
                u0 = u1;
                let b1 = b[idx + 1] ^ (p[idx + 1] & mask);
                b[idx] = (b0 >> 1) | (b1 << 31);
                b0 = b1;
                idx += 1;
            }

            u[idx] = u0 >> 1;
            b[idx] = b0 >> 1;
            ubits -= 1;
        }

        if ubits <= 32 && u[0] == 1 {
            break;
        }

        if ubits < vbits {
            std::mem::swap(&mut ubits, &mut vbits);
            std::mem::swap(&mut u, &mut v);
            std::mem::swap(&mut b, &mut c);
        }

        for idx in 0..n {
            u[idx] ^= v[idx];
            b[idx] ^= c[idx];
        }

        if ubits == vbits {
            ubits = blength(&u) as i64;
        }
    }

    ret[..n].copy_from_slice(&b[..n]);
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blength_zero() {
        assert_eq!(blength(&[0, 0, 0]), 0);
    }

    #[test]
    fn test_blength_one() {
        assert_eq!(blength(&[1]), 1);
        assert_eq!(blength(&[2]), 2);
        assert_eq!(blength(&[0x8000_0000]), 32);
    }

    #[test]
    fn test_blength_multi_word() {
        assert_eq!(blength(&[0, 1]), 33);
        assert_eq!(blength(&[0xffff_ffff, 1]), 33);
        assert_eq!(blength(&[0, 0, 1]), 65);
    }

    #[test]
    fn test_mul_1x1_simple() {
        // 1 * 1 = 1
        let (lo, hi) = mul_1x1(1, 1);
        assert_eq!(lo, 1);
        assert_eq!(hi, 0);

        // 0 * anything = 0
        let (lo, hi) = mul_1x1(0, 0xdeadbeef);
        assert_eq!(lo, 0);
        assert_eq!(hi, 0);

        // x * x = x^2 in GF(2)
        let (lo, hi) = mul_1x1(2, 2);
        assert_eq!(lo, 4);
        assert_eq!(hi, 0);

        // (x+1)(x+1) = x^2 + 1 in GF(2) (not x^2 + 2x + 1 — 2x vanishes)
        let (lo, hi) = mul_1x1(3, 3);
        assert_eq!(lo, 5);
        assert_eq!(hi, 0);
    }

    #[test]
    fn test_mul_1x1_top_bits() {
        // Verify top2b handling: 0x80000000 * 1 = 0x80000000
        let (lo, hi) = mul_1x1(0x8000_0000, 1);
        assert_eq!(lo, 0x8000_0000);
        assert_eq!(hi, 0);

        // 0x80000000 * 0x80000000 should produce x^62 (bit 62 set in high word)
        let (lo, hi) = mul_1x1(0x8000_0000, 0x8000_0000);
        assert_eq!(lo, 0);
        assert_eq!(hi, 0x4000_0000); // bit 62 == (62 - 32) == bit 30 of high word
    }

    #[test]
    fn test_mul_2x2_zero() {
        let r = mul_2x2(0, 0, 0, 0);
        assert_eq!(r, [0, 0, 0, 0]);
    }

    #[test]
    fn test_mul_2x2_identity() {
        // 1 * 1 = 1 (as 2x2)
        let r = mul_2x2(0, 1, 0, 1);
        assert_eq!(r, [1, 0, 0, 0]);
    }

    #[test]
    fn test_fmul_simple() {
        // (x+1) * (x+1) = x^2 + 1 in GF(2)
        let a = [3u32, 0];
        let b = [3u32, 0];
        let mut s = [0u32; 4];
        fmul(&a, &b, &mut s);
        assert_eq!(s[0], 5); // x^2 + 1 = 0b101 = 5
        assert_eq!(s[1], 0);
        assert_eq!(s[2], 0);
        assert_eq!(s[3], 0);
    }

    #[test]
    fn test_fmod_no_reduction_needed() {
        // Polynomial x^2 + 1, modulus x^257 + x^12 + 1. No reduction since degree is small.
        let mut ret = vec![0u32; 9]; // 288 bits / 32 = 9 words
        let a = {
            let mut v = vec![0u32; 9];
            v[0] = 5;
            v
        };
        let p = [257u32, 12, 0];
        fmod(&a, &p, &mut ret);
        assert_eq!(ret[0], 5);
        for i in 1..ret.len() {
            assert_eq!(ret[i], 0);
        }
    }

    #[test]
    fn test_fmod_reduction_257() {
        // a = x^257, modulus x^257 + x^12 + 1
        // After reduction: x^257 = x^12 + 1
        let mut a = vec![0u32; 9];
        a[8] = 1 << (257 - 8 * 32); // bit 257 in word 8 at position 1
        assert_eq!(a[8], 1 << 1); // 2

        let p = [257u32, 12, 0];
        let mut ret = vec![0u32; 9];
        fmod(&a, &p, &mut ret);

        // Expected: x^12 + 1 = 0x1001
        assert_eq!(ret[0], 0x1001);
        for i in 1..ret.len() {
            assert_eq!(ret[i], 0, "word {} should be zero", i);
        }
    }

    #[test]
    fn test_finv_roundtrip() {
        // For DSTU curve 6 (m=257, p = x^257 + x^12 + 1), compute inverse of a small value
        // and verify a * inv(a) = 1.

        // Irreducible polynomial in word form: x^257 + x^12 + 1
        // Word 8: bit 1 (x^257), Word 0: bit 12 (x^12), Word 0: bit 0 (1)
        // = [0x1001, 0, 0, 0, 0, 0, 0, 0, 0x2]
        let mut p_words = vec![0u32; 9];
        p_words[0] = 0x1001;
        p_words[8] = 0x2;

        // a = x + 1 = 0x3
        let mut a = vec![0u32; 9];
        a[0] = 3;

        let mut inv = vec![0u32; 9];
        finv(&a, &p_words, &mut inv);

        // Verify: (a * inv) mod p == 1
        // fmul requires a.len() + b.len() + 2 slack for odd-sized operands.
        let mut product = vec![0u32; 20];
        fmul(&a, &inv, &mut product);

        let p_exp = [257u32, 12, 0];
        let mut reduced = vec![0u32; 20];
        fmod(&product, &p_exp, &mut reduced);

        assert_eq!(
            reduced[0], 1,
            "inverse verification failed: a * inv(a) should be 1"
        );
        for i in 1..reduced.len() {
            assert_eq!(reduced[i], 0, "word {} should be zero after reduction", i);
        }
    }
}
