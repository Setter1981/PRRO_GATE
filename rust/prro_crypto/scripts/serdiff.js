'use strict';
// Byte-diff: jkurwa short_sign('le') serialization vs prro_crypto r_le(32)||s_le(32),
// for the SAME (r,s). Isolates the CMS signature-value serialization difference.
const path = require('path');
const jk = require(path.resolve(__dirname, '../../../sidecar/node_modules/jkurwa'));
const curve = jk.std_curve('DSTU_PB_257');
const Field = jk.Field;

// jkurwa short_sign('le' => raw): reverse(truncate_buf8(r)) || reverse(truncate_buf8(s)), mlen=max.
function jkurwaSerialize(sig) {
  const tr = sig.r.truncate_buf8();
  const ts = sig.s.truncate_buf8();
  const mlen = Math.max(tr.length, ts.length);
  const out = Buffer.alloc(mlen * 2);
  for (let i = 0; i < mlen; i++) out[i] = (tr[mlen - i - 1] || 0) & 0xff;          // R reversed
  for (let i = 0; i < mlen; i++) out[i + mlen] = (ts[mlen - i - 1] || 0) & 0xff;   // S reversed
  return out;
}
// field big-endian hex (fieldToHex) -> N little-endian bytes (prro fe_to_bytes_le).
function leBytesFromBeHex(beHex, n) {
  const b = Buffer.from(beHex.padStart(n * 2, '0').slice(-n * 2), 'hex'); // big-endian, n bytes
  return Buffer.from(b).reverse(); // -> little-endian
}
function fieldToHex(f) { let s=''; for (let i=f.bytes.length-1;i>=0;i--) s+=f.bytes[i].toString(16).padStart(8,'0'); return s; }
function prroSerialize(sig) {
  return Buffer.concat([leBytesFromBeHex(fieldToHex(sig.r), 32), leBytesFromBeHex(fieldToHex(sig.s), 32)]);
}

const inputs = [
  { d:'DEADBEEFCAFE12345678ABCD90909090ABCDEF1234567890FEDCBA0987654321',
    hash:'01020304050607080910AABBCCDDEEFF1234567890ABCDEF1122334455667788',
    rand_e:'123456789ABCDEF0FEDCBA9876543210ABCDEF1234567890DEADBEEFCAFEBABE' },
];
for (const inp of inputs) {
  const priv = new jk.Priv(curve, new Field(inp.d,'hex',curve));
  const sig = priv.help_sign(new Field(inp.hash,'hex',curve), new Field(inp.rand_e,'hex',curve));
  if (!sig) { console.log('help_sign null'); continue; }
  const jkb = jkurwaSerialize(sig);
  const prb = prroSerialize(sig);
  console.log('r (BE hex):', fieldToHex(sig.r));
  console.log('s (BE hex):', fieldToHex(sig.s));
  console.log('jkurwa serialized (le):', jkb.toString('hex'), `(${jkb.length}B)`);
  console.log('prro   serialized      :', prb.toString('hex'), `(${prb.length}B)`);
  console.log('EQUAL:', Buffer.compare(jkb, prb) === 0);
}
