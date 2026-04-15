#!/usr/bin/env node
// Generate point arithmetic test vectors from jkurwa.
//
// Note: jkurwa's Point.mul has an inconsistent API (requires monkey-patched
// scalar with signum()), so we only use add() and twice() here. Scalar
// multiplication in our Rust port is verified via algebraic properties
// (k*G == G added k times, n*G == infinity) and ultimately by signature
// byte-identity at the sign step.

'use strict';

const path = require('path');
const jkurwaPath = path.resolve(__dirname, '../../../sidecar/node_modules/jkurwa/lib');
const { Curve } = require(path.join(jkurwaPath, 'curve'));

const curve = Curve.from_id('DSTU_PB_257');

function fieldToHex(f) {
    let s = '';
    for (let i = f.bytes.length - 1; i >= 0; i--) {
        s += f.bytes[i].toString(16).padStart(8, '0');
    }
    return s;
}

function pointToObj(p) {
    return {
        x: fieldToHex(p.x),
        y: fieldToHex(p.y),
    };
}

const vectors = [];

// 1. Base point (sanity)
vectors.push({
    op: 'base_point',
    expected: pointToObj(curve.base),
});

// 2. Doubling: 2G via twice()
const g2 = curve.base.twice();
vectors.push({
    op: 'twice',
    p: pointToObj(curve.base),
    expected: pointToObj(g2),
});

// 3. add: G + G == 2G
vectors.push({
    op: 'add',
    a: pointToObj(curve.base),
    b: pointToObj(curve.base),
    expected: pointToObj(curve.base.add(curve.base)),
});

// 4. add: G + 2G == 3G
const g3 = curve.base.add(g2);
vectors.push({
    op: 'add',
    a: pointToObj(curve.base),
    b: pointToObj(g2),
    expected: pointToObj(g3),
});

// 5. Iterated doublings: chain 2G -> 4G -> 8G -> ... -> 256G.
// Each entry stores the input point and the doubled output, so Rust can verify
// each step independently.
let acc = g2;
for (let i = 2; i <= 8; i++) {
    const prev = acc;
    acc = acc.twice();
    vectors.push({
        op: 'twice',
        p: pointToObj(prev),
        expected: pointToObj(acc),
        result_label: `${1 << i}G`,
    });
}

// 6. Sequential additions: kG = G + G + G ... (for small k)
// Useful to verify our scalar mul against algebraic ground truth.
let kG = curve.base;
const sequential = [{ k: 1, p: pointToObj(kG) }];
for (let k = 2; k <= 20; k++) {
    kG = kG.add(curve.base);
    sequential.push({ k, p: pointToObj(kG) });
}
vectors.push({
    op: 'sequential_kG',
    expected: sequential,
});

// 7. negate: -G
vectors.push({
    op: 'negate',
    p: pointToObj(curve.base),
    expected: pointToObj(curve.base.negate()),
});

// 8. add: G + (-G) == infinity
const negG = curve.base.negate();
const sum = curve.base.add(negG);
vectors.push({
    op: 'add',
    a: pointToObj(curve.base),
    b: pointToObj(negG),
    expected: pointToObj(sum),
    note: 'should be point at infinity (0,0)',
});

process.stdout.write(JSON.stringify(vectors, null, 2));
process.stderr.write(`Generated ${vectors.length} point vectors.\n`);
