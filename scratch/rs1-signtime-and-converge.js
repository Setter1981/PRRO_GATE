export const meta = {
  name: 'rs1-signtime-and-converge',
  description: 'Resolve the read-RPC signingTime-freshness question empirically from reference products + a convergence review of the crypto-seam review-fixes (0b426bc)',
  phases: [{ title: 'Resolve+Review', detail: 'A: signingTime empirics (protocol + WebCheck + PRRODPS) · B: convergence review of the fixes' }],
}

const PROTO = '/mnt/d/PRRO_GATE/.claude/worktrees/agent-a624911a/docs/dps_protocol'
const WC = '/mnt/d/PRRO_GATE/.claude/worktrees/agent-a624911a/docs/webcheck_reverse'
const PDPS = '/mnt/d/prrodps_src'
const PRRO = '/mnt/d/PRRO_GATE/rust/prro'

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'conclusion', 'evidence', 'findings', 'gaps'],
  properties: {
    area: { type: 'string' },
    conclusion: { type: 'string', description: 'The decisive answer/verdict for your lens.' },
    evidence: { type: 'array', items: { type: 'string' }, description: 'Each a fact with file:line.' },
    findings: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false,
        required: ['severity', 'location', 'problem', 'fix'],
        properties: {
          severity: { type: 'string', enum: ['CRITICAL', 'HIGH', 'MEDIUM', 'LOW', 'INFO'] },
          location: { type: 'string' },
          problem: { type: 'string' },
          fix: { type: 'string' },
        },
      },
    },
    gaps: { type: 'array', items: { type: 'string' } },
  },
}

phase('Resolve+Review')

const AGENTS = [
  {
    key: 'signtime_empirics',
    q: `DECIDE: must the read-RPC fiscal-number signature (the rro_fn_sign / CMS blob attached to lastChk / statusRro / infoRro requests, proving the caller owns the FN's key) carry a FRESH signingTime PER CALL, or can it be cached at boot / have signing_time omitted? This decides whether the Rust gateway must rebuild that blob per-RPC vs once at boot (RS-1 build_fn_sign). Investigate three sources and quote file:line:

(1) ФСКО PROTOCOL at ${PROTO} (read 251051_(1).md + siblings): does it specify/require a signingTime on the read-request signature, and any freshness / staleness / replay window? Search for signingTime, час/часу, мітка часу, lastChk, statusRro, "перевірка зв'язку" / ping, and the signature requirements on read vs receipt requests.

(2) WebCheck decompile at ${WC} (WebCheckMain/WebCheck/ClassFiscal.cs + Signature.cs + StringXML.cs): how does WebCheck build + sign the lastChk / statusRro / ping request? Does it construct + sign the blob FRESH per call (current time) or reuse a cached signature? Find the signing call(s) for read RPCs and whether a timestamp is stamped at call time.

(3) PRRODPS at ${PDPS} (PRRODPS.DFS/DFSApi.cs + CmdCashRegisterStateReq.cs + the connectivity/state RPCs): does it sign the state/last-check request per call with the current time, or cache it?

CONCLUDE decisively: does real-world practice + the protocol REQUIRE per-RPC fresh signingTime (→ RS-1 MUST build fn_sign per-RPC/per-tick, the cached-at-boot map is unsafe), OR is a cached / signing_time=None blob tolerated for these read RPCs? Put the verdict in 'conclusion'.`,
  },
  {
    key: 'converge_review',
    q: `CONVERGENCE REVIEW (fresh eyes) of the crypto-seam review-fixes at commit 0b426bc on branch feat/rs1-runtime-supervisor (run \`git -C /mnt/d/PRRO_GATE show 0b426bc\`). Two prior external reviews found PII-Debug + password-copy + signingTime-doc + error-semantics issues; this commit addressed them. VERIFY each fix is correct + complete, and find any RESIDUAL a third fresh reviewer would catch. OPEN the files:
- ${PRRO}/src/crypto/session.rs — SigningSession::Debug must now redact operator_id (line ~63); from_extracted uses SealKind::MissingSigningCert.
- ${PRRO}/src/crypto/errors.rs — CryptoError::JksUnseal Display ("{reason:?}", operator_id omitted) + manual Debug (operator_id "<redacted>"); new SealKind::MissingSigningCert.
- ${PRRO}/src/runtime/key_loader.rs — password is now a borrowed &str (no Zeroizing<String>/to_string copy); build_fn_sign has a FRESHNESS doc warning (don't cache; per-RPC).
- ${PRRO}/tests/crypto_provider_smoke.rs — the redaction test now asserts operator_id is NOT visible.
Check: (a) Is operator_id (PII) now fully redacted on EVERY Debug/Display surface, or does any path still leak it (grep for operator_id in fmt/tracing/error contexts)? (b) Does dropping operator_id from the Display message break any caller/test, or leave the error too vague to debug a real boot failure? (c) Is the borrowed-&str password change sound (lifetimes, still UTF-8-validated, no use-after-free)? (d) Is SealKind::MissingSigningCert wired correctly (any exhaustive match on SealKind elsewhere now non-exhaustive)? (e) Any NEW issue the fixes introduced. Give a verdict (MERGE / FIX_THEN_MERGE / BLOCK) in 'conclusion' + findings with file:line.`,
  },
]

const results = await parallel(
  AGENTS.map((a) => () =>
    agent(a.q, { label: 'rs1:' + a.key, phase: 'Resolve+Review', schema: SCHEMA }).then((x) => ({ key: a.key, ...x }))
  )
)

return results.filter(Boolean)
