#!/usr/bin/env node
// Generate DSTU 4145 sign test vectors from jkurwa.
//
// We bypass Priv.sign (which uses random rand_e) and directly call help_sign
// with controlled inputs (known d, hash, rand_e). This produces deterministic
// (r, s) outputs that our Rust port must match byte-for-byte.

'use strict';

const path = require('path');
const jk = require(path.resolve(__dirname, '../../../sidecar/node_modules/jkurwa'));

const curve = jk.std_curve('DSTU_PB_257');
const Field = jk.Field;

function fieldToHex(f) {
    let s = '';
    for (let i = f.bytes.length - 1; i >= 0; i--) {
        s += f.bytes[i].toString(16).padStart(8, '0');
    }
    return s;
}

const vectors = [];

// Test vector inputs: deterministic and large-enough that windowNaf path is used
// (not compactNaf, which has the broken signum() dependency).
const testInputs = [
    {
        d: 'DEADBEEFCAFE12345678ABCD90909090ABCDEF1234567890FEDCBA0987654321',
        hash: '01020304050607080910AABBCCDDEEFF1234567890ABCDEF1122334455667788',
        rand_e: '123456789ABCDEF0FEDCBA9876543210ABCDEF1234567890DEADBEEFCAFEBABE',
    },
    {
        d: '7F8E9D2A4C6B5D8E1F2A3B4C5D6E7F8090A1B2C3D4E5F60718293A4B5C6D7E8F',
        hash: 'FFFFFFFFEEEEDDDDCCCCBBBBAAAA999988887777666655554444333322221111',
        rand_e: '111122223333444455556666777788889999AAAABBBBCCCCDDDDEEEEFFFF0000',
    },
    {
        d: '1A2B3C4D5E6F7081A1B1C1D1E1F101112131415161718191A1B1C1D1E1F12122',
        hash: '0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF',
        rand_e: 'CAFEBABEDEADBEEFFACADE0011223344556677889900AABBCCDDEEFFCAFEBABE',
    },
];

for (const inp of testInputs) {
    const d = new Field(inp.d, 'hex', curve);
    const priv = new jk.Priv(curve, d);

    const hash_v = new Field(inp.hash, 'hex', curve);
    const rand_e = new Field(inp.rand_e, 'hex', curve);

    const sig = priv.help_sign(hash_v, rand_e);
    if (sig === null) {
        process.stderr.write(`WARN: help_sign returned null for d=${inp.d.substr(0,16)}...\n`);
        continue;
    }

    vectors.push({
        op: 'sign',
        d: inp.d,
        hash: inp.hash,
        rand_e: inp.rand_e,
        expected_r: fieldToHex(sig.r),
        expected_s: fieldToHex(sig.s),
    });
}

// Also include a verify vector: produce a full sign+verify pair so Rust can
// verify the output against jkurwa's verify too.
for (const inp of testInputs) {
    const d = new Field(inp.d, 'hex', curve);
    const priv = new jk.Priv(curve, d);
    const pub = priv.pub();

    const hash_v = new Field(inp.hash, 'hex', curve);
    const rand_e = new Field(inp.rand_e, 'hex', curve);

    const sig = priv.help_sign(hash_v, rand_e);
    if (!sig) continue;

    // Verify with jkurwa to confirm pubkey is set up correctly
    const ok = pub.help_verify(hash_v, sig.s, sig.r);
    vectors.push({
        op: 'verify',
        d: inp.d,
        hash: inp.hash,
        r: fieldToHex(sig.r),
        s: fieldToHex(sig.s),
        // Pub point coords (Q = -d*G, jkurwa convention)
        pub_x: fieldToHex(pub.point.x),
        pub_y: fieldToHex(pub.point.y),
        expected_valid: ok,
    });
}

process.stdout.write(JSON.stringify(vectors, null, 2));
process.stderr.write(`Generated ${vectors.length} sign/verify vectors.\n`);
