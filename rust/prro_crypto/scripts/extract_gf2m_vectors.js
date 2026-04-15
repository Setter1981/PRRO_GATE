#!/usr/bin/env node
// Extract GF(2^m) test vectors from jkurwa for byte-identical proof.
//
// Generates 100+ test vectors for mul, mod, inv operations using DSTU curve 6
// modulus (x^257 + x^12 + 1). Output is JSON consumable by the Rust test suite.
//
// Usage: node extract_gf2m_vectors.js > ../tests/vectors/gf2m_jkurwa.json

'use strict';

const path = require('path');
const jkurwaPath = path.resolve(__dirname, '../../../sidecar/node_modules/jkurwa/lib');
const gf2m = require(path.join(jkurwaPath, 'gf2m'));

// DSTU curve 6 irreducible polynomial: x^257 + x^12 + 1
// Word form (little-endian u32): word 0 holds bits 0..31, word 8 holds bit 257
// bit 0 (x^0): word 0 bit 0
// bit 12 (x^12): word 0 bit 12
// bit 257 (x^257): word 8 bit 1
// = [0x1001, 0, 0, 0, 0, 0, 0, 0, 0x2]
const MOD_WORDS = 9;
const P_EXP = [257, 12, 0];  // exponent form for fmod
const P_BITS = new Uint32Array([0x1001, 0, 0, 0, 0, 0, 0, 0, 0x2]);  // word form for finv

// Deterministic PRNG for reproducibility (xorshift32, seed chosen at top).
let _rng_state = 0x12345678;
function rand32() {
    let x = _rng_state;
    x ^= x << 13;
    x ^= x >>> 17;
    x ^= x << 5;
    _rng_state = x >>> 0;
    return _rng_state;
}

function randomElement() {
    // Random element in GF(2^257) — fill 9 words, mask top bits so < 2^257.
    const v = new Uint32Array(MOD_WORDS);
    for (let i = 0; i < MOD_WORDS; i++) v[i] = rand32();
    // Keep only bits 0..256 — clear all bits from position 257 up.
    // bit 257 is word 8 bit 1. So mask word 8 to keep only bit 0 (value 0x1).
    v[8] &= 0x1;
    return v;
}

function wordsToHex(v) {
    // Little-endian words to big-endian hex string (most significant word first, padded)
    let s = '';
    for (let i = v.length - 1; i >= 0; i--) {
        s += v[i].toString(16).padStart(8, '0');
    }
    return s;
}

function hexToWords(hex, words) {
    // Big-endian hex to little-endian u32 array of given word count.
    const v = new Uint32Array(words);
    // Pad to multiple of 8 hex chars
    while (hex.length < words * 8) hex = '0' + hex;
    for (let i = 0; i < words; i++) {
        // Word i (little-endian) sits at position (words - 1 - i) in big-endian hex
        const offset = (words - 1 - i) * 8;
        v[i] = parseInt(hex.substr(offset, 8), 16) >>> 0;
    }
    return v;
}

const vectors = [];

// 1. mul vectors
for (let k = 0; k < 50; k++) {
    const a = randomElement();
    const b = randomElement();
    const sLen = 2 * MOD_WORDS + 2;
    const s = new Uint32Array(sLen);
    gf2m.mul(a, b, s);
    vectors.push({
        op: 'mul',
        a: wordsToHex(a),
        b: wordsToHex(b),
        a_words: MOD_WORDS,
        b_words: MOD_WORDS,
        expected: wordsToHex(s),
        expected_words: sLen,
    });
}

// 2. mod vectors (reduce a 2n-word polynomial using P_EXP)
// Note: jkurwa's gf2m.mod RETURNS a new array if ret is omitted; mutating
// requires capturing the return value.
for (let k = 0; k < 50; k++) {
    const big = new Uint32Array(2 * MOD_WORDS + 2);
    for (let i = 0; i < big.length; i++) big[i] = rand32();

    const ret = gf2m.mod(big, P_EXP);

    vectors.push({
        op: 'mod',
        a: wordsToHex(big),
        a_words: big.length,
        p_exp: P_EXP,
        expected: wordsToHex(ret),
        expected_words: ret.length,
    });
}

// 3. inv vectors — harder to set up because jkurwa's finv takes p in WORD form.
// finv signature: finv(a, p, ret). a and p must be same length.
// NOTE: in field.js, inv is called as: impl.inv(a, p, a) where p = curve.calc_modulus(mod_bits)
// which returns a word-form array. So we pass P_BITS.
//
// We mirror: first reduce a random value, then invert it.
for (let k = 0; k < 30; k++) {
    const raw = new Uint32Array(2 * MOD_WORDS + 2);
    for (let i = 0; i < raw.length; i++) raw[i] = rand32();

    // Reduce first to get a < p (capture return value: jkurwa.mod returns new array).
    const reduced_raw = gf2m.mod(raw, P_EXP);
    const a = new Uint32Array(MOD_WORDS);
    for (let i = 0; i < MOD_WORDS; i++) a[i] = reduced_raw[i];

    // Skip if a is zero (inverse undefined)
    let isZero = true;
    for (let i = 0; i < MOD_WORDS; i++) {
        if (a[i] !== 0) { isZero = false; break; }
    }
    if (isZero) continue;

    const ret = new Uint32Array(MOD_WORDS);
    const p_copy = new Uint32Array(MOD_WORDS);
    for (let i = 0; i < MOD_WORDS; i++) p_copy[i] = P_BITS[i];

    // finv modifies `a` in place (it uses `a` as the working `u` buffer).
    // We must pass a copy.
    const a_for_inv = new Uint32Array(MOD_WORDS);
    for (let i = 0; i < MOD_WORDS; i++) a_for_inv[i] = a[i];
    gf2m.inv(a_for_inv, p_copy, ret);

    vectors.push({
        op: 'inv',
        a: wordsToHex(a),
        a_words: MOD_WORDS,
        p_bits: wordsToHex(P_BITS),
        p_words: MOD_WORDS,
        expected: wordsToHex(ret),
        expected_words: MOD_WORDS,
    });
}

// 4. blength vectors
for (let k = 0; k < 20; k++) {
    const v = randomElement();
    // Sometimes zero out top words to vary bit length
    if ((rand32() & 1) && k % 3 === 0) {
        const zeroFrom = 2 + (rand32() % (MOD_WORDS - 3));
        for (let i = zeroFrom; i < MOD_WORDS; i++) v[i] = 0;
    }
    const bl = gf2m.blength(v);
    vectors.push({
        op: 'blength',
        a: wordsToHex(v),
        a_words: MOD_WORDS,
        expected: bl,
    });
}

// 5. mul_2x2 spot checks
for (let k = 0; k < 20; k++) {
    const a0 = rand32();
    const a1 = rand32();
    const b0 = rand32();
    const b1 = rand32();
    const ret = new Uint32Array(6);
    gf2m.mul_2x2(a1, a0, b1, b0, ret);
    vectors.push({
        op: 'mul_2x2',
        a0: a0 >>> 0,
        a1: a1 >>> 0,
        b0: b0 >>> 0,
        b1: b1 >>> 0,
        expected: [ret[0], ret[1], ret[2], ret[3]],  // [4] and [5] always 0
    });
}

process.stdout.write(JSON.stringify(vectors, null, 2));
process.stderr.write(`Generated ${vectors.length} test vectors.\n`);
