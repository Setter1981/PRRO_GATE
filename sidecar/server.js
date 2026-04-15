/**
 * DPS Signing Sidecar PoC — low-latency local DSTU signer.
 *
 * Lifecycle:
 *   1. Boot: loads JKS once, decrypts key, pairs with signing cert, keeps Box warm
 *   2. Serve: HTTP endpoints for signing — no per-request keystore reopen
 *
 * Endpoints:
 *   POST /sign_raw  — sign arbitrary bytes → CMS/PKCS#7 DER (base64)
 *   POST /sign      — sign document payload → CMS/PKCS#7 DER (base64)
 *   GET  /healthz   — signer readiness
 *
 * Config (env vars):
 *   JKS_PATH     — path to .jks file
 *   JKS_PASSWORD — keystore password
 *   PORT         — HTTP port (default 8090)
 */

const http = require('http');
const fs = require('fs');
const crypto = require('crypto');
const jk = require('jkurwa');
const gost89 = require('gost89');
const rfc3280 = require('jkurwa/lib/spec/rfc3280');

const path = require('path');
const JKS_PATH = process.env.JKS_PATH || path.resolve(__dirname, '..', 'key_13667753_13667753 (2).jks');
const JKS_PASSWORD = process.env.JKS_PASSWORD || '';
const PORT = parseInt(process.env.PORT || '8090', 10);

let signerReady = false;
let signerBox = null;
let loadCount = 0;
let signCount = 0;

// ─── JKS Decrypt ───
function decryptJksKey(jksData, password) {
    if (jksData.readUInt32BE(0) !== 0xFEEDFEED) throw new Error('Not a JKS file');
    let offset = 12;
    const tag = jksData.readUInt32BE(offset); offset += 4;
    if (tag !== 1) throw new Error('JKS entry is not a private key');
    const aliasLen = jksData.readUInt16BE(offset); offset += 2;
    const alias = jksData.slice(offset, offset + aliasLen).toString('utf8');
    offset += aliasLen; offset += 8;

    const pkLen = jksData.readUInt32BE(offset); offset += 4;
    const pkBlob = jksData.slice(offset, offset + pkLen); offset += pkLen;

    const encOctetLen = pkBlob.readUInt16BE(22);
    const encrypted = pkBlob.slice(24, 24 + encOctetLen);
    const salt = encrypted.slice(0, 20);
    const encData = encrypted.slice(20, encrypted.length - 20);
    const checkHash = encrypted.slice(encrypted.length - 20);

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
    const intCheck = crypto.createHash('sha1'); intCheck.update(pwdBuf); intCheck.update(result);
    if (!intCheck.digest().equals(checkHash)) throw new Error('JKS integrity failed — wrong password');

    const certCount = jksData.readUInt32BE(offset); offset += 4;
    const certs = [];
    for (let c = 0; c < certCount; c++) {
        const ctLen = jksData.readUInt16BE(offset); offset += 2; offset += ctLen;
        const cLen = jksData.readUInt32BE(offset); offset += 4;
        certs.push(jksData.slice(offset, offset + cLen)); offset += cLen;
    }
    return { keyDer: result, certs, alias };
}

function loadKey() {
    const data = fs.readFileSync(JKS_PATH);
    const { keyDer, certs, alias } = decryptJksKey(data, JKS_PASSWORD);

    // Parse DSTU private key via jkurwa
    const parsed = jk.models.Priv.from_asn1(keyDer, true);
    const priv = parsed.keys[0];
    if (!priv || !priv.d) throw new Error('Failed to extract signing private key');

    // Find matching signing cert
    const algos = gost89.compat.algos();
    const privKeyId = priv.pub().keyid(algos);
    let signingCert = null;
    for (const certDer of certs) {
        try {
            const decoded = rfc3280.Certificate.decode(certDer, 'der');
            decoded._raw = certDer;
            const cert = new jk.models.Certificate(decoded);
            if (cert.canUseFor('sign')) {
                const certKeyId = cert.extension && cert.extension.subjectKeyIdentifier;
                if (certKeyId && privKeyId.equals(certKeyId)) {
                    signingCert = cert;
                    break;
                }
            }
        } catch (e) { /* skip unparseable certs */ }
    }
    if (!signingCert) throw new Error('No matching signing certificate found');

    // Build Box — key material stays resident in memory
    signerBox = new jk.Box({ algo: algos });
    signerBox.load({ priv });
    signerBox.load({ cert: signingCert });
    signerBox._indexKeys();

    signerReady = true;
    loadCount++;
    console.log(`[sidecar] Key loaded from ${JKS_PATH}, alias="${alias}", loadCount=${loadCount}`);
    console.log(`[sidecar] Signer: ${signingCert.subject.commonName}`);
}

async function signPayload(payloadBytes) {
    if (!signerReady || !signerBox) throw new Error('Signer not ready');
    signCount++;
    const msg = await signerBox.sign(payloadBytes, null, null, {});
    return Buffer.from(msg.as_asn1());
}

// ─── HTTP Server ───
const server = http.createServer(async (req, res) => {
    if (req.method === 'GET' && req.url === '/healthz') {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        return res.end(JSON.stringify({ ready: signerReady, load_count: loadCount, sign_count: signCount }));
    }
    if (req.method === 'POST' && (req.url === '/sign_raw' || req.url === '/sign')) {
        let body = '';
        for await (const chunk of req) body += chunk;
        try {
            const parsed = JSON.parse(body);
            const payloadB64 = parsed.payload_base64 || parsed.payload;
            if (!payloadB64) {
                res.writeHead(400, { 'Content-Type': 'application/json' });
                return res.end(JSON.stringify({ error: 'payload_base64 or payload required' }));
            }
            const isRaw = !!parsed.payload_base64;
            const payloadBuf = isRaw ? Buffer.from(payloadB64, 'base64') : Buffer.from(payloadB64, 'utf8');
            const t0 = process.hrtime.bigint();
            const signedBuf = await signPayload(payloadBuf);
            const dt = Number(process.hrtime.bigint() - t0) / 1e6;
            const resp = isRaw
                ? { signed_base64: signedBuf.toString('base64') }
                : { signed_payload: signedBuf.toString('base64') };
            resp.sign_ms = dt.toFixed(2);
            res.writeHead(200, { 'Content-Type': 'application/json' });
            return res.end(JSON.stringify(resp));
        } catch (e) {
            res.writeHead(500, { 'Content-Type': 'application/json' });
            return res.end(JSON.stringify({ error: e.message }));
        }
    }
    res.writeHead(404); res.end('not found');
});

try {
    loadKey();
    server.listen(PORT, () => console.log(`[sidecar] Ready on http://localhost:${PORT}`));
} catch (e) {
    console.error(`[sidecar] FATAL: ${e.message}`);
    process.exit(1);
}
