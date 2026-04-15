#!/usr/bin/env node
// Generate GOST 34.311-95 test vectors using jkurwa's gost89 reference.
// Output: tests/vectors/gost3411_jkurwa.json

'use strict';

const path = require('path');
const gostPath = path.resolve(__dirname, '../../../sidecar/node_modules/gost89');
const gost89 = require(gostPath);

function hash(data) {
    const buf = Buffer.isBuffer(data) ? data : Buffer.from(data, 'binary');
    const out = Buffer.alloc(32);
    gost89.gosthash(buf, out);
    return out.toString('hex');
}

const vectors = [];

// Tier 1 — basic cases
vectors.push({
    label: 'empty',
    input_hex: '',
    expected_hex: hash(Buffer.alloc(0)),
});

vectors.push({
    label: 'one_zero_byte',
    input_hex: '00',
    expected_hex: hash(Buffer.from([0])),
});

vectors.push({
    label: 'one_byte_FF',
    input_hex: 'ff',
    expected_hex: hash(Buffer.from([0xff])),
});

// Tier 2 — block boundary sizes (block is 32 bytes for GOST 34.311)
for (const len of [1, 15, 16, 31, 32, 33, 63, 64, 65, 128, 256, 500, 1024, 4096]) {
    const buf = Buffer.alloc(len);
    // Deterministic pattern
    for (let i = 0; i < len; i++) {
        buf[i] = (i * 31 + 7) & 0xFF;
    }
    vectors.push({
        label: `pattern_len_${len}`,
        input_hex: buf.toString('hex'),
        expected_hex: hash(buf),
    });
}

// Tier 3 — common DSTU test strings
vectors.push({
    label: 'string_abc',
    input_hex: Buffer.from('abc').toString('hex'),
    expected_hex: hash(Buffer.from('abc')),
});

vectors.push({
    label: 'string_message_digest',
    input_hex: Buffer.from('message digest').toString('hex'),
    expected_hex: hash(Buffer.from('message digest')),
});

vectors.push({
    label: 'alphabet_lowercase',
    input_hex: Buffer.from('abcdefghijklmnopqrstuvwxyz').toString('hex'),
    expected_hex: hash(Buffer.from('abcdefghijklmnopqrstuvwxyz')),
});

// Tier 4 — large input (16 KB)
const large = Buffer.alloc(16384);
for (let i = 0; i < 16384; i++) large[i] = (i * 1103515245 + 12345) & 0xFF;
vectors.push({
    label: 'pattern_16k',
    input_hex: large.toString('hex'),
    expected_hex: hash(large),
});

process.stdout.write(JSON.stringify(vectors, null, 2));
process.stderr.write(`Generated ${vectors.length} GOST 34.311 test vectors.\n`);
