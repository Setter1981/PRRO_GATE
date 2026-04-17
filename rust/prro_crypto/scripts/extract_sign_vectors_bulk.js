#!/usr/bin/env node
// Generate N DSTU 4145 sign vectors from jkurwa via a seeded PRNG.
//
// Companion to the committed `sign_jkurwa.json` (3 hand-picked vectors).
// This script produces a large, reproducible batch for the byte-diff
// acceptance test of prro_crypto vs jkurwa: if every vector reproduces
// byte-for-byte on the Rust side, we treat that as acceptance-equivalent
// evidence that jkurwa production output and prro_crypto output are
// interchangeable for any (d, hash, rand_e) triple the PRNG covers.
//
// Usage:
//   node scripts/extract_sign_vectors_bulk.js [N=10000] [SEED=12345] \
//       > tests/vectors/sign_jkurwa_10k.json
//
// The output file is NOT committed (see .gitignore); regenerate it
// locally before running the opt-in test
// `test_sign_byte_identical_with_jkurwa_10k`.

'use strict';

const path = require('path');
const jk = require(path.resolve(__dirname, '../../../sidecar/node_modules/jkurwa'));

const curve = jk.std_curve('DSTU_PB_257');
const Field = jk.Field;

const MASK64 = (1n << 64n) - 1n;

function* xorshift64(seed) {
    let x = BigInt(seed);
    if (x === 0n) x = 0xDEADBEEF_CAFEBABEn;
    while (true) {
        x ^= (x << 13n) & MASK64;
        x ^= x >> 7n;
        x ^= (x << 17n) & MASK64;
        yield x;
    }
}

function hex256(prng) {
    let hex = '';
    for (let i = 0; i < 4; i++) {
        hex += prng.next().value.toString(16).padStart(16, '0');
    }
    return hex;
}

function fieldToHex(f) {
    let s = '';
    for (let i = f.bytes.length - 1; i >= 0; i--) {
        s += f.bytes[i].toString(16).padStart(8, '0');
    }
    return s;
}

const N_VECTORS = parseInt(process.argv[2] || '10000', 10);
const SEED = BigInt(process.argv[3] || '12345');

const prng = xorshift64(SEED);
const vectors = [];
let skipped_null_sig = 0;
let skipped_err = 0;

for (let i = 0; i < N_VECTORS; i++) {
    const d_hex = hex256(prng);
    const hash_hex = hex256(prng);
    const rand_e_hex = hex256(prng);

    let sig;
    try {
        const d = new Field(d_hex, 'hex', curve);
        const priv = new jk.Priv(curve, d);
        const hash_v = new Field(hash_hex, 'hex', curve);
        const rand_e = new Field(rand_e_hex, 'hex', curve);
        sig = priv.help_sign(hash_v, rand_e);
    } catch (err) {
        skipped_err++;
        continue;
    }
    if (sig === null) {
        // jkurwa returns null for degenerate (eG.x == 0 or r == 0).
        // Rust sign() also returns None in the same cases, so we skip
        // the vector rather than hand Rust an input it will reject.
        skipped_null_sig++;
        continue;
    }

    vectors.push({
        op: 'sign',
        d: d_hex,
        hash: hash_hex,
        rand_e: rand_e_hex,
        expected_r: fieldToHex(sig.r),
        expected_s: fieldToHex(sig.s),
    });
}

// Compact JSON (one object per line is nicer to diff but bigger; use
// minified for size and rely on the test for readable errors).
process.stdout.write(JSON.stringify(vectors));
process.stderr.write(
    `Generated ${vectors.length} sign vectors ` +
    `(skipped ${skipped_null_sig} degenerate, ${skipped_err} errors) ` +
    `from seed ${SEED.toString()}\n`
);
