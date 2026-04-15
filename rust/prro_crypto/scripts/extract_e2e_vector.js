#!/usr/bin/env node
// End-to-end vector: read real JKS, extract priv key, sign known hash with
// known rand_e, dump (r, s). Our Rust port must produce byte-identical output.

'use strict';

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const jk = require(path.resolve(__dirname, '../../../sidecar/node_modules/jkurwa'));

// ─── JKS Decrypt (mirror sidecar/server.js) ───
function decryptJksKey(jksData, password) {
    if (jksData.readUInt32BE(0) !== 0xFEEDFEED) throw new Error('Not a JKS file');
    let offset = 12;
    const tag = jksData.readUInt32BE(offset); offset += 4;
    if (tag !== 1) throw new Error('JKS entry is not a private key');
    const aliasLen = jksData.readUInt16BE(offset); offset += 2;
    offset += aliasLen; offset += 8;
    const pkLen = jksData.readUInt32BE(offset); offset += 4;
    const pkBlob = jksData.slice(offset, offset + pkLen);
    const encOctetLen = pkBlob.readUInt16BE(22);
    const encrypted = pkBlob.slice(24, 24 + encOctetLen);
    const salt = encrypted.slice(0, 20);
    const encData = encrypted.slice(20, encrypted.length - 20);
    const pwdBuf = Buffer.alloc(password.length * 2);
    for (let i = 0; i < password.length; i++) pwdBuf.writeUInt16BE(password.charCodeAt(i), i * 2);
    const result = Buffer.alloc(encData.length);
    let xo = 0, counter = Buffer.from(salt);
    while (xo < encData.length) {
        const h = crypto.createHash('sha1'); h.update(pwdBuf); h.update(counter);
        const ks = h.digest();
        for (let i = 0; i < ks.length && xo < encData.length; i++) result[xo] = encData[xo] ^ ks[i], xo++;
        counter = ks;
    }
    return result;
}

const JKS_PATH = '/mnt/d/PRRO_GATE/key_13667753_13667753 (2).jks';
const PASSWORD = 'Jrcfyf123';

const jksData = fs.readFileSync(JKS_PATH);
const keyDer = decryptJksKey(jksData, PASSWORD);

// Parse to get priv with d
const parsed = jk.models.Priv.from_asn1(keyDer, true);
const priv = parsed.keys[0];
const d = priv.d;
const curve = priv.curve;
const Field = jk.Field;

// Get curve name to confirm
console.error('Curve m:', curve.m);
console.error('Curve name:', curve.name());
console.error('d (hex):', d.toString(true));

// Sign with deterministic inputs
const hash_hex = '0102030405060708091011121314151617181920212223242526272829303132';
const rand_e_hex = 'CAFEBABEDEADBEEFFACADE0011223344556677889900AABBCCDDEEFFCAFEBABE';
const hash_v = new Field(hash_hex, 'hex', curve);
const rand_e = new Field(rand_e_hex, 'hex', curve);

const sig = priv.help_sign(hash_v, rand_e);
if (!sig) throw new Error('help_sign returned null');

const fieldToHex = (f) => {
    let s = '';
    for (let i = f.bytes.length - 1; i >= 0; i--) {
        s += f.bytes[i].toString(16).padStart(8, '0');
    }
    return s;
};

const result = {
    op: 'e2e_jks_sign',
    note: 'Real JKS key + deterministic sign — Rust must produce identical r, s',
    jks_path: JKS_PATH,
    password: PASSWORD,
    curve: 'DSTU_PB_257',
    d: fieldToHex(d),
    hash: hash_hex,
    rand_e: rand_e_hex,
    expected_r: fieldToHex(sig.r),
    expected_s: fieldToHex(sig.s),
};

console.error('\nGenerated e2e vector:');
console.error('  d (real key):', result.d.substr(0, 16) + '...');
console.error('  expected r:  ', result.expected_r);
console.error('  expected s:  ', result.expected_s);

process.stdout.write(JSON.stringify([result], null, 2));
