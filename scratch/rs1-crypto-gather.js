export const meta = {
  name: 'rs1-crypto-gather',
  description: 'Extract exact signatures + verbatim port snippets for RS-1 crypto pieces 2/3/3b (from_extracted ctor, OperatorKeyLoader, build_fn_sign)',
  phases: [{ title: 'Gather', detail: '4 searchers: W4-Z3 port source / SigningSession+InProcess / prro_crypto containers / loader contract+CheckSignBlob' }],
}

const WT = '/mnt/d/prro_gate_m4_w4_z3/rust/prro'
const SRC = '/mnt/d/PRRO_GATE/rust/prro/src'
const CRYPTO = '/mnt/d/PRRO_GATE/rust/prro_crypto'

const GOAL = `
GOAL: I am implementing RS-1 crypto pieces in the rust-gateway tree:
- Piece 2: a NEW production ctor SigningSession::from_extracted (in crypto/session.rs) that builds a SigningSession from an already-extracted key WITHOUT the test-only new_for_test path.
- Piece 3: a production OperatorKeyLoader impl (new runtime/key_loader.rs) that reads a per-FN JKS path + password, calls prro_crypto extract_private_key, and assembles a SigningContext { provider: Arc<dyn CryptoProvider>=InProcessProvider, session: SigningSession, profile: CmsProfile }.
- Piece 3b: build_fn_sign producing the per-FN CheckSignBlob (native attached CAdES-BES CMS over the FN string, selecting signing_cert NOT certs[0]).
The live-proven reference is the W4-Z3 smoke test (test-only); I must port the PROVEN logic to src/ swapping new_for_test -> from_extracted. The signing_cert()-vs-certs[0] choice is the -14 CryptBadSign trap (fixed 2026-05-29). I need EXACT signatures, struct fields, and verbatim code.
`

const RULES = GOAL + `
HARD RULES: OPEN the files, QUOTE VERBATIM the exact fn signatures, struct field lists (name: type), and the bodies/snippets I must port — with file:line. Do NOT paraphrase signatures or types. Flag anything test-only (new_for_test, fixtures) that must NOT be ported. If a type is re-exported or aliased, give the canonical path.
`

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'signatures', 'shapes', 'verbatim', 'notes', 'gaps'],
  properties: {
    area: { type: 'string' },
    signatures: { type: 'array', items: { type: 'string' }, description: 'Each: file:line — exact fn/method signature, verbatim.' },
    shapes: { type: 'array', items: { type: 'string' }, description: 'Each: file:line — struct/enum with exact `field: Type` list, verbatim.' },
    verbatim: { type: 'array', items: { type: 'string' }, description: 'Verbatim code snippets to port or call against, with file:line.' },
    notes: { type: 'array', items: { type: 'string' }, description: 'gotchas, test-only-do-not-port flags, the signing_cert trap.' },
    gaps: { type: 'array', items: { type: 'string' } },
  },
}

phase('Gather')

const SEARCHERS = [
  {
    key: 'port_source',
    q: 'In the W4-Z3 worktree test ' + WT + '/tests/live_dps_extended_smoke.rs, extract VERBATIM the three port-source helpers + their dependencies: (1) `load_signing_key` (~line 277) — how it reads the JKS path + password from env and calls extract_private_key; (2) `live_signing_ctx` (~line 609) — how it assembles the SigningContext (the exact struct literal, the SigningSession::new_for_test call + its args, the provider + profile); (3) `sign_fn_blob` (~line 304) — how it builds the CheckSignBlob (the CMS attached CAdES-BES over the FN string, signer construction, signing_cert selection). Quote the exact code blocks with file:line. Mark which parts are TEST-ONLY (env reads, new_for_test, hard-coded test values) and which are the PROVEN crypto logic to keep. Also quote any helper consts (curve/profile/op) they reference.',
  },
  {
    key: 'signing_session',
    q: 'In ' + SRC + '/crypto/session.rs extract VERBATIM: the `SigningSession` struct (every field: name + type, with file:line); the `SigningSession::new_for_test` fn (~:104) signature + body (so I can build a from_extracted twin WITHOUT the test bits); the `unseal_jks` fn (~:130) signature + what it returns + the SealedMaterial it consumes. Then in ' + SRC + '/crypto/in_process.rs: `InProcessProvider::new` (~:24) signature + the CryptoProvider trait it implements. And the `SigningContext` struct (' + SRC + '/services/write_path/stage_sign.rs:66) + the `CmsProfile` enum (variants, where defined). Quote each verbatim with file:line. I need to know EXACTLY what fields SigningSession has so from_extracted constructs them all correctly.',
  },
  {
    key: 'crypto_containers',
    q: 'In ' + CRYPTO + ' (start ' + CRYPTO + '/src/containers.rs) extract VERBATIM: `extract_private_key` (~:196) full signature (params + return type incl. error type); the `ExtractedKey` struct (every field: name + type — esp. `param_d` ~:79 and any cert fields); the `signing_cert()` method (~:105) signature + what it returns + HOW it selects the signing cert vs certs[0] (the -14 trap — quote the selection logic exactly); any `param_d` accessor. Also: how is the JKS file read (path + password) — what bytes/format does extract_private_key expect? Quote with file:line. I need the exact inputs/outputs to wire the loader.',
  },
  {
    key: 'loader_contract',
    q: 'In ' + SRC + '/runtime/bindings.rs extract VERBATIM: the `OperatorKeyLoader` trait (~:142) — the exact `load` method signature (params incl. how the JKS path + password arrive, return type incl. error); `build_from_db` (~:179) signature + the part of its body that CALLS the loader + ASSEMBLES `OperatorBindings`; the `OperatorBindings` struct (~:55-58, exact fields); the secret-handling/zeroize discipline (~:119-140, quote it). Then find the `CheckSignBlob` type (grep the tree) — its definition (fields/alias) + where RuntimeView.fn_sign uses it (' + SRC + '/services/reconciliation/runtime.rs:64). And where per-FN JKS path + password are stored in the secure operators table (the operators schema/row — search migrations_secure + the operators repo). Quote with file:line. I need the exact trait contract to implement + where the key material comes from.',
  },
]

const results = await parallel(
  SEARCHERS.map((s) => () =>
    agent(s.q + '\n' + RULES, { label: 'cr:' + s.key, phase: 'Gather', schema: SCHEMA }).then((x) => ({ key: s.key, ...x }))
  )
)

return results.filter(Boolean)
