export const meta = {
  name: 'rs1-crypto-review',
  description: 'Independent fresh-eyes review of the RS-1 crypto seam (pieces 2/3/3b + trait change) — 2 diverse lenses',
  phases: [{ title: 'Review', detail: '2 reviewers: crypto+secret-discipline / correctness+invariants+API' }],
}

const P = '/mnt/d/PRRO_GATE/rust/prro'

const CONTEXT = `
REVIEW TARGET: the RS-1 crypto seam on branch feat/rs1-runtime-supervisor, commits ad76646 (pieces 2/3/3b) + 2c25351 (a PII fix). Run \`git -C /mnt/d/PRRO_GATE show ad76646 2c25351 --stat\` and read the actual files. The crypto seam is the highest-care part of the runtime-spine work.

CHANGED (read these):
- ${P}/src/crypto/session.rs — NEW SigningSession::from_extracted (production ctor) + 2 unit tests (mod from_extracted_tests).
- ${P}/src/runtime/key_loader.rs — NEW JksOperatorKeyLoader (impl OperatorKeyLoader) + build_fn_sign.
- ${P}/src/runtime/bindings.rs — OperatorKeyLoader::load trait gained operator_id: &str (first param); build_from_db (~line 277) passes &row.operator_id.
- 4 test loaders updated + ${P}/tests/common/mod.rs det_signing_ctx_for + a registry test asserting operator_id reaches the session.

DOMAIN FACTS:
- The -14 CryptBadSign trap (fixed 2026-05-29, live-confirmed): a UA EDS JKS ships a digitalSignature cert AND a keyAgreement (encryption) cert + CA chain; embedding certs[0] (often the encryption cert) makes DPS reject the sig. Fix = ExtractedKey::signing_cert() (prro_crypto containers.rs:105, selects KeyUsage=digitalSignature, falls back to certs.first()).
- Secret discipline (bindings.rs ~119-140, audit R2-4): password: &[u8] borrows a caller-owned Zeroizing<Vec<u8>> wiped on drop; impls MUST NOT clone it into un-zeroized heap; the returned SigningContext MUST NOT retain plaintext password; secret-bearing types MUST NOT #[derive(Debug)] (manual redacted Debug, ADR-M2-5 §4d).
- operator_id = cashier INN (ІПН) = PII — must not reach process logs (journald/Loki), only audit_log.
- Operator decisions: Route 1 (loader does extract_private_key directly, NOT unseal_jks — JKS password handled FLAT/unsealed here; sealing-at-rest is a SEPARATE tracked follow-up, deliberately NOT in this piece); operator_id stored verbatim, no fallback/placeholder.
- Frozen invariant #1: no network/crypto inside a long SQLite write transaction.
`

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['lens', 'verdict', 'findings', 'confirmed_safe', 'gaps'],
  properties: {
    lens: { type: 'string' },
    verdict: { type: 'string', enum: ['MERGE', 'FIX_THEN_MERGE', 'BLOCK'] },
    findings: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false,
        required: ['severity', 'location', 'problem', 'suggested_fix'],
        properties: {
          severity: { type: 'string', enum: ['CRITICAL', 'HIGH', 'MEDIUM', 'LOW', 'INFO'] },
          location: { type: 'string', description: 'file:line' },
          problem: { type: 'string' },
          suggested_fix: { type: 'string' },
        },
      },
    },
    confirmed_safe: { type: 'array', items: { type: 'string' }, description: 'load-bearing things you VERIFIED correct, with file:line.' },
    gaps: { type: 'array', items: { type: 'string' }, description: 'what you could not check + why.' },
  },
}

phase('Review')

const LENSES = [
  {
    key: 'crypto_secret',
    q: 'LENS: cryptography + secret-material discipline. OPEN the changed files. Verify adversarially: (1) The -14 trap is closed in BOTH from_extracted (must use signing_cert(), NEVER certs[0]) AND build_fn_sign (uses session.cert_der() — confirm the cert stored in the session was the signing-cert-selected one). (2) Secret leaks: is the password ever cloned into a plain String/Vec instead of Zeroizing? is param_d moved (not cloned)? does the returned SigningContext/SigningSession retain the plaintext password? does any Debug/format!/tracing/println emit operator_id (PII), password, param_d, or JKS bytes? does map_container_err\'s format!("{other:?}") risk leaking secret bytes (check what ContainerError Debug carries)? (3) build_fn_sign CMS correctness: attached:true present (load-bearing), profile Dstu4145WithGost34311Pb, content = FN string bytes, no second extract_private_key. (4) Confirm the JKS password is genuinely handled FLAT here (Route 1) and that NO sealing/hardening crept in (it must be a separate follow-up). Give a verdict + findings with file:line.',
  },
  {
    key: 'correctness_api',
    q: 'LENS: correctness + invariants + API/test soundness. OPEN the changed files. Verify adversarially: (1) The OperatorKeyLoader::load trait signature change — is operator_id threaded correctly (build_from_db passes the authoritative row.operator_id)? are ALL impls updated (4 test loaders)? any other caller of load/build_from_db missed (grep)? (2) from_extracted error: the CryptoError::JksUnseal{reason: KeyExtractionFailed} reuse for "no signing cert" — acceptable or misleading? the partial move `extracted.param_d` after `extracted.signing_cert()` — sound (no use-after-move; ExtractedKey has no Drop)? (3) build_fn_sign as a boot-time helper — return type appropriate; any .expect()/.unwrap()/panic on a PRODUCTION path (vs tests)? (4) Invariant #1: from_extracted/build_fn_sign/the loader run at boot — confirm none is invoked inside a held SQLite write transaction. (5) Test adequacy: does a test actually prove operator_id reaches the SigningSession end-to-end? are the failure paths (no-cert, bad-password) covered? (6) Minimal-diff: any scope creep / speculative abstraction / style churn? Give a verdict + findings with file:line.',
  },
]

const results = await parallel(
  LENSES.map((l) => () =>
    agent(l.q + '\n' + CONTEXT, { label: 'review:' + l.key, phase: 'Review', schema: SCHEMA }).then((x) => ({ key: l.key, ...x }))
  )
)

return results.filter(Boolean)
