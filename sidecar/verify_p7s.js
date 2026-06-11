// Verify a .p7s signature with jkurwa (the DPS-equivalent verifier).
// usage: node verify_p7s.js <p7s> <signer-cert.der>
const fs = require('fs');
const jk = require('jkurwa');
const gost89 = require('gost89');

const algos = gost89.compat.algos();
const hashFn = algos.hash;

const p7sPath = process.argv[2];
const certPath = process.argv[3];

const buf = fs.readFileSync(p7sPath);
const certDer = fs.readFileSync(certPath);

// Load signer cert + pubkey.
let cert;
try { cert = jk.Certificate.from_asn1(certDer); }
catch (e) { cert = new jk.models.Certificate(certDer); }
const pubkey = cert.pubkey;

// Parse the signed message.
const msg = new jk.models.Message(buf);

// Hash of the signed-attributes (what was signed).
const hash = msg.mhash(hashFn);
console.log(`${p7sPath}`);
console.log('  signed-attrs hash :', Buffer.from(hash).toString('hex'));
console.log('  signature bytes   :', Buffer.from(msg.signature).length);

// The core DSTU-4145 verify (little-endian signature value).
let ok;
try { ok = pubkey.verify(hash, msg.signature, 'le'); }
catch (e) { ok = 'ERROR: ' + e.message; }
console.log('  DSTU verify (le)  :', ok);

// Also try the other endianness for diagnosis.
try {
  const rev = Buffer.from(msg.signature);
  const half = rev.length / 2;
  const r = Buffer.from(rev.slice(0, half)).reverse();
  const s = Buffer.from(rev.slice(half)).reverse();
  const swapped = Buffer.concat([r, s]);
  console.log('  DSTU verify (be)  :', pubkey.verify(hash, swapped, 'le'));
} catch (e) { console.log('  DSTU verify (be)  : ERROR', e.message); }
