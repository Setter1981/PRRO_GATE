export const meta = {
  name: 'verify-rsq2',
  description: 'RS-Q2: does a PRODUCTION constructor for Arc<dyn DpsChannel> + OperatorKeyLoader (SigningContext/CheckSignBlob) exist, or must RS-1 build it from scratch? The W4-Z3 live smoke built a live channel — find where + whether it is reusable.',
  phases: [{ title: 'Verify', detail: '4 searchers: DpsChannel impls / live-smoke construction / key loader / config-transport wiring' }],
}

const RUST = '/mnt/d/PRRO_GATE/rust'
const SRC = RUST + '/prro/src'
const CRYPTO = RUST + '/prro_crypto'
const Z3 = '/mnt/d/prro_gate_m4_w4_z3'

const CONTEXT = `
CONTEXT (established): The RS-1 supervisor (runtime spine) must build, at boot, a per-FN dependency bundle:
- Arc<dyn DpsChannel> (the live DPS transport — TLS client cert / keystore, talks to cabinet.tax.gov.ua:9443 sendChkV2/lastChk/statusRro)
- a SigningContext { provider: Arc<dyn CryptoProvider>, session: SigningSession, profile: CmsProfile } + per-FN CheckSignBlob
The gather sweep claimed both exist ONLY as test fixtures (dps(), AlwaysOkLoader, SigningSession::new_for_test) and that no production builder is wired under \`prro serve\`. BUT the W4-Z3 live smoke (worktree ` + Z3 + `, branch feat/m4-w4-z3-dps-extended-smoke) drove a FULL live fiscal cycle (SHIFT_OPEN/SELL/Z) against the real test cabinet using native Rust crypto — so the construction LOGIC exists somewhere. The decisive question: is it (A) reusable PRODUCTION code in src/ (a from_config / builder the supervisor can call), (B) PARTIAL (some real builders, missing glue), or (C) TEST-HARNESS-ONLY (hard-coded in tests/helpers — logic proven live but must be productionized for RS-1)?
`

const RULES = CONTEXT + `
HARD RULES: OPEN files, cite EXACT file:line. Classify every construction site as PROD (non-test src/) vs TEST (#[cfg(test)] / tests/ / *_for_test / fixtures). The answer RS-1 needs is binary per dependency: does a production constructor exist (cite it) or must it be built (cite the test-only logic that proves it is buildable). Do NOT confuse "the trait/impl exists" with "a production CONSTRUCTOR that wires it from config exists" — those are different.
`

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'summary', 'verdict', 'construction_sites', 'rs1_needs', 'gaps'],
  properties: {
    area: { type: 'string' },
    summary: { type: 'string', description: 'Dense answer. Facts only.' },
    verdict: { type: 'string', description: 'EXISTS_PROD (cite) / PARTIAL (what exists + what missing) / TEST_ONLY (cite the proven-but-test logic) / FROM_SCRATCH — with decisive evidence.' },
    construction_sites: { type: 'array', items: { type: 'string' }, description: 'Each: "file:line — what is constructed — PROD|TEST".' },
    rs1_needs: { type: 'array', items: { type: 'string' }, description: 'Concretely, what RS-1 must build/call to obtain this dependency live, in order.' },
    gaps: { type: 'array', items: { type: 'string' } },
  },
}

phase('Verify')

const SEARCHERS = [
  {
    key: 'dpschannel_impls',
    q: 'On HEAD (' + SRC + '), find the `DpsChannel` trait definition + EVERY implementor. For each impl: is it the native-CMS/gRPC path, an HTTP sidecar, or a test mock? Where is each CONSTRUCTED — is there a non-test `from_config` / `connect` / `new` builder in src/ that takes config (endpoint, TLS client identity, certs) and yields an `Arc<dyn DpsChannel>` ready to hit cabinet.tax.gov.ua:9443? Open the transports module (search ' + SRC + ' for `DpsChannel`, `transports`, `tonic`/`tonic_build`, `ClientTlsConfig`, `Channel::`, `sendChkV2`). Classify each construction site PROD vs TEST. Decisive: does a production DpsChannel builder exist on HEAD, or only test fixtures like `dps()`?',
  },
  {
    key: 'live_smoke_construction',
    q: 'Open the W4-Z3 worktree at ' + Z3 + ' (branch feat/m4-w4-z3-dps-extended-smoke). Find the live-DPS smoke test (e.g. ' + Z3 + '/rust/prro/tests/live_dps_extended_smoke.rs) and trace EXACTLY how it constructs (1) the live DpsChannel that reaches cabinet.tax.gov.ua:9443, and (2) the SigningContext (the helper `live_signing_ctx`, InProcessProvider::new, SigningSession::new_for_test, the JKS/cert load). For EACH construction step quote the file:line and say whether the code it calls lives in PRODUCTION src/ (reusable) or in test helpers/fixtures (must be productionized). Specifically: what builds the TLS/transport to the cabinet, and what parses the JKS / loads param_d + signing_cert? Is there a real channel-builder being called, or is the channel hand-assembled in the test? This is the proof-of-construction lead — resolve whether RS-1 can REUSE it or must EXTRACT it.',
  },
  {
    key: 'key_loader',
    q: 'On HEAD, find the `OperatorKeyLoader` trait (referenced at ' + SRC + '/runtime/bindings.rs ~142) + EVERY impl. Is there a PRODUCTION loader that reads the secure `operators` table (or a JKS/PFX file) and produces a real SigningContext { provider, session, profile } + CheckSignBlob + signing_cert, or only the test `AlwaysOkLoader`? Open ' + SRC + '/runtime/bindings.rs (OperatorBindings, build_from_db, the loader trait) and ' + CRYPTO + ' (the JKS/DSTU-4145 key parse, InProcessProvider, SigningSession, signing_cert()). Where does a SigningSession get its real param_d + cert in NON-test code? Classify PROD vs TEST. Decisive: does a production key/cert loader exist, or must RS-1 build the operators-table→SigningContext loader?',
  },
  {
    key: 'config_transport',
    q: 'On HEAD, map the config→transport→crypto wiring a supervisor would need. Open ' + SRC + '/runtime/bootstrap.rs (what config/operators tables it seeds), the secure `operators` table schema (search migrations for `operators`), and any per-FN binding config. Answer: (1) what is stored per fiscal_number for the DPS endpoint + TLS client identity + crypto profile (is the cert/key in the DB, a file path, a keystore ref?); (2) is there a "two-channel DPS architecture" builder, and what is built vs stubbed; (3) what would the supervisor read to go from a fiscal_number to a live `Arc<dyn DpsChannel>` + signer? Cite file:line. Decisive: is the config/DB plumbing present to drive a production transport+crypto builder, or is it also missing?',
  },
]

const results = await parallel(
  SEARCHERS.map((s) => () =>
    agent(s.q + '\n' + RULES, { label: 'rsq2:' + s.key, phase: 'Verify', schema: SCHEMA }).then((x) => ({ key: s.key, ...x }))
  )
)

return results.filter(Boolean)
