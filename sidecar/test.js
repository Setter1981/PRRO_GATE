/**
 * Sidecar automated proof: boot once, sign twice, verify load_count stays 1.
 */
const http = require('http');
const { spawn } = require('child_process');
const path = require('path');

const PORT = 18090; // avoid conflict with default
let passed = 0;
let failed = 0;

function request(method, urlPath, body) {
    return new Promise((resolve, reject) => {
        const opts = { hostname: '127.0.0.1', port: PORT, path: urlPath, method, headers: {} };
        if (body) opts.headers['Content-Type'] = 'application/json';
        const req = http.request(opts, res => {
            let data = '';
            res.on('data', c => data += c);
            res.on('end', () => {
                try { resolve({ status: res.statusCode, body: JSON.parse(data) }); }
                catch (e) { resolve({ status: res.statusCode, body: data }); }
            });
        });
        req.on('error', reject);
        if (body) req.write(JSON.stringify(body));
        req.end();
    });
}

function assert(cond, msg) {
    if (cond) { passed++; console.log(`  PASS: ${msg}`); }
    else { failed++; console.error(`  FAIL: ${msg}`); }
}

async function runTests() {
    // T1: healthz ready, load_count=1
    const h1 = await request('GET', '/healthz');
    assert(h1.body.ready === true, 'healthz ready');
    assert(h1.body.load_count === 1, 'load_count=1 after boot');
    assert(h1.body.sign_count === 0, 'sign_count=0 before any sign');

    // T2: sign_raw produces non-empty signed_base64
    const s1 = await request('POST', '/sign_raw', { payload_base64: Buffer.from('<RQ>test1</RQ>').toString('base64') });
    assert(s1.status === 200, 'sign_raw returns 200');
    assert(s1.body.signed_base64 && s1.body.signed_base64.length > 100, 'signed_base64 is substantial');
    assert(parseFloat(s1.body.sign_ms) > 0, 'sign_ms is positive');

    // T3: second sign — load_count must stay 1
    const s2 = await request('POST', '/sign_raw', { payload_base64: Buffer.from('4001017042').toString('base64') });
    assert(s2.status === 200, 'second sign_raw returns 200');

    const h2 = await request('GET', '/healthz');
    assert(h2.body.load_count === 1, 'load_count still 1 after two signs (no reopen)');
    assert(h2.body.sign_count === 2, 'sign_count=2 after two signs');

    // T4: signed output differs from input (not passthrough)
    const inputB64 = Buffer.from('<RQ>test1</RQ>').toString('base64');
    assert(s1.body.signed_base64 !== inputB64, 'signed output differs from input');

    // T5: signed output starts with CMS/PKCS#7 DER marker (0x30 = SEQUENCE)
    const signedBuf = Buffer.from(s1.body.signed_base64, 'base64');
    assert(signedBuf[0] === 0x30, 'signed DER starts with SEQUENCE tag 0x30');

    console.log(`\nResults: ${passed} passed, ${failed} failed`);
    return failed === 0;
}

// Boot sidecar as subprocess
const serverPath = path.join(__dirname, 'server.js');
const child = spawn(process.execPath, [serverPath], {
    env: { ...process.env, PORT: String(PORT), JKS_PASSWORD: process.env.JKS_PASSWORD || '' },
    stdio: ['ignore', 'pipe', 'pipe'],
});

let started = false;
child.stdout.on('data', d => {
    const line = d.toString();
    if (!started) process.stderr.write(line);
    if (line.includes('Ready on') && !started) {
        started = true;
        runTests()
            .then(ok => { child.kill(); process.exit(ok ? 0 : 1); })
            .catch(e => { console.error('Test error:', e); child.kill(); process.exit(1); });
    }
});
child.stderr.on('data', d => process.stderr.write(d));
child.on('exit', (code) => { if (!started) { console.error('Sidecar failed to start, exit:', code); process.exit(1); } });

setTimeout(() => { if (!started) { console.error('Timeout waiting for sidecar'); child.kill(); process.exit(1); } }, 10000);
