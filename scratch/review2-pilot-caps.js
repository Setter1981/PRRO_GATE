export const meta = {
  name: 'review2-pilot-caps',
  description: 'Round-2 deep code-grounded adversarial self-review of the 4 pilot caps — trust NOTHING, open every source file, cite exact evidence',
  phases: [{ title: 'Review2', detail: '7 reviewers each OPEN the real code and verify every claim with file:line evidence' }],
}

const REPO = '/mnt/d/PRRO_GATE'
const SRC = REPO + '/rust/prro/src'
const MIG = REPO + '/rust/prro/migrations'
const WT = '/mnt/d/prro_gate_m4_w4_z3'
const CAPS = REPO + '/docs/architecture/ALGORITHMIC_MAP.md, ' + REPO + '/docs/architecture/PILOT_TEST_MATRIX.md, ' + REPO + '/docs/architecture/PILOT_REVIEW_PLAYBOOK.md, ' + REPO + '/docs/operations/LIVE_DPS_SMOKE_RUNBOOK.md'

const RULES = `
HARD RULES (operator: "не доверяй памяти, всё проверяй в коде, подробно, медленно, очень качественно"):
1. Trust NOTHING — not the docs' own claims, not any summary, not any prior review, not memory. For EVERY claim you assess, OPEN the actual source file with Read/Grep and READ it.
2. The caps describe the PILOT = rust-gateway HEAD. Verify code claims against ` + SRC + ` and ` + MIG + ` (NOT the worktree). Exception: the W4-Z3 anchor is framed "on branch feat/m4-w4-z3, not HEAD" — for that, also check ` + WT + `/rust/prro and git.
3. There are TWO migration trees: Python ` + REPO + `/sql/*.sql (DEAD reference) and Rust ` + MIG + `/*.sql (the pilot). A citation to sql/00x is only valid if explicitly tagged dead-Python. Catch any pilot claim resting on the Python tree.
4. For EVERY finding AND every confirmed-good item, cite the EXACT file:line you opened as evidence. If you could not open/verify a claim, mark it UNVERIFIED — never assume.
5. Re-verify the round-1 fixes from scratch (offline_sessions=state/DRAINING; idempotency=migrations/002:91; CodePoolExhausted/STOP_MODE split; W4-Z3 pending-merge; force/senior tested-but-undriven). Did the fix land correctly AND not introduce a NEW error or internal contradiction?
`

const SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['verdict', 'findings', 'confirmed_good', 'unverified'],
  properties: {
    verdict: { type: 'string', enum: ['PASS', 'CHANGES_REQUESTED'] },
    findings: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['severity', 'doc_loc', 'doc_claim', 'code_reality', 'evidence', 'fix'],
        properties: {
          severity: { type: 'string', enum: ['Critical', 'High', 'Medium', 'Low', 'Info'] },
          doc_loc: { type: 'string', description: 'which cap + section/line' },
          doc_claim: { type: 'string', description: 'what the doc asserts' },
          code_reality: { type: 'string', description: 'what the code actually shows' },
          evidence: { type: 'string', description: 'EXACT file:line you opened to confirm the code reality' },
          fix: { type: 'string' },
        },
      },
    },
    confirmed_good: { type: 'array', items: { type: 'string' }, description: 'claim — verified accurate, with file:line evidence inline' },
    unverified: { type: 'array', items: { type: 'string' }, description: 'claims you could not confirm/refute and why' },
  },
}

phase('Review2')

const REVIEWERS = [
  {
    key: 'enums_crypto',
    q: 'ROUND-2 deep review of caps (' + CAPS + ') — ENUM VOCABULARY + CRYPTO. OPEN ' + SRC + '/db/models/enums.rs and READ every enum (DocState, ShiftState, OfflineSessionState, NodeMode, InboxStatus, plus RetryClass/ErRedriveDecision wherever defined). Verify EVERY state string in all 4 caps matches the source EXACTLY (spelling, count, the str_enum! mapping). Then OPEN ' + SRC + '/crypto/provider.rs + in_process.rs and verify: outbound = CMS-detached SIGNED (cite the fn + line), inbound KVT2 = decrypt/unwrap_envelope (cite line), DSTU 4145 = signature not encryption. Check every provider.rs:NN line citation in the caps lands on what is claimed (round-1 changed them to :50/:79/:33 — verify those are right). Flag any mismatch.',
  },
  {
    key: 'migrations_schema',
    q: 'ROUND-2 deep review of caps (' + CAPS + ') — MIGRATIONS + SCHEMA reality. OPEN the Rust migrations in ' + MIG + '/ and verify EVERY schema claim. Specifically RE-VERIFY the round-1 fixes from scratch: (1) offline_sessions — does ' + MIG + '/015_offline_normalize.sql actually rebuild it with column "state" and CHECK including DRAINING (not status/CLOSING)? Open it and quote the CHECK. Confirm the docs now say state/DRAINING-live + status/CLOSING-dead-pre-015. (2) idempotency — does ' + MIG + '/002_fiscal_documents.sql actually define ux_inbox_fn_idem as composite UNIQUE(fiscal_number, idempotency_key) at ~line 91? Open + quote it. Confirm caps cite migrations/002 not sql/001. (3) active-shift index — confirm Rust migrations have ONLY non-unique ix_shifts_fn_state (open 001 + 016), no UNIQUE active-shift index. (4) DocState/ShiftState SQL CHECK constraints — do they match the enums (the grounding noted SQL CHECK lags enum)? Flag any doc schema claim that does not match the actual Rust migration.',
  },
  {
    key: 'wired_claims',
    q: 'ROUND-2 deep review of caps (' + CAPS + ') — every WIRED claim. For EACH item the caps mark WIRED, OPEN the cited source fn AND the cited test and verify both exist + do what is claimed. Cover at least: check_shift_guard 162-cell matrix (stage_acquire.rs — open it, confirm the fn, the production caller, and the oracle test asserting 162); Pattern C OFFLINE_LOCAL_ACK (stage_offline_ack.rs run fn + dispatch.rs caller + the audit event + the test); CodePoolExhausted typed error (offline_sessions.rs + test) — AND verify the round-1 STOP_MODE-split is correct (open stage_offline_ack.rs around the propagation + backlog_drain.rs trigger_tier_2_stop_mode); drain-reject→manual escalate_drain_to_manual (backlog_drain.rs + the Critical audit + the test); force/senior seams (shifts.rs) — verify the round-1 "tested-but-undriven / no production caller" claim by grepping for callers across ' + SRC + '. Flag any WIRED item whose driver or test does NOT exist / does NOT match.',
  },
  {
    key: 'unwired_claims',
    q: 'ROUND-2 deep review of caps (' + CAPS + ') — every UNWIRED claim. For EACH item the caps mark UNWIRED, GREP ' + SRC + ' (+ migrations + tests) to confirm there is genuinely NO production driver/enforcement. Cover: online shift lifecycle edges 3/4/8/10/11/12 (grep shifts::transition_state + shifts::insert_created callers — are they really only backlog_drain + boot_phase + tests, with NO write-path/ingress driver?); active-shift partial-UNIQUE index (no CREATE UNIQUE INDEX on shifts in any Rust migration); INV-09 36h freeze (no offline_session_started_at column + no OFFLINE_LIMIT_EXCEEDED_INGRESS_REFUSED audit — grep both); INV-10 168h cap (current_month_offline_seconds column exists but no enforcement reader — grep); WebCheck cert-expiry SHIFT_OPEN gate (no NotAfter/2160 block); INV-05 channel-switch guard + INV-06 failover (no guard code). FLAG any item marked UNWIRED that is ACTUALLY wired (a real production caller exists) — that is the dangerous direction. Cite the grep evidence.',
  },
  {
    key: 'inv_citations',
    q: 'ROUND-2 deep review of caps (' + CAPS + ') — INVARIANT citations. OPEN ' + REPO + '/docs/LEGAL_INVARIANTS.md and READ all INV-01..INV-20. For EVERY "Enforces (INV-NN)" tag / INV reference across the 4 caps, verify it cites the CORRECT invariant for that control. Re-verify the round-1 fixes: Sign step should be INV-18 (not INV-10); concurrency-stress should be INV-01 (not INV-08); WebCheck cert-gate consistent between MAP + MATRIX; INV-16 present in the RUNBOOK excise appendix. Flag any wrong INV number, any INV cited that does not exist, any major invariant (INV-02/03/04/05/06/09/10/13/15/16) missing where it should appear, and any remaining MAP-vs-MATRIX disagreement on the same control.',
  },
  {
    key: 'citation_audit',
    q: 'ROUND-2 deep review of caps (' + CAPS + ') — LINE-CITATION AUDIT. Round 1 found many file:line citations had drifted. Systematically SAMPLE at least 25 distinct file:line citations across the 4 caps (spread over enums.rs, stage_acquire.rs, stage_offline_ack.rs, backlog_drain.rs, shifts.rs, provider.rs, dispatch.rs, offline_sessions.rs, ingress_inbox.rs, the migrations). For EACH: OPEN the cited file at the cited line and confirm it lands on what the doc claims (the fn / the audit event / the CHECK / the transition). Report each citation as CORRECT or DRIFTED (with the actual line where the thing really is). This is a precision audit — be exhaustive and exact; cite what you actually found at each line.',
  },
  {
    key: 'w4z3_consistency',
    q: 'ROUND-2 deep review of caps (' + CAPS + ') — W4-Z3 HEAD-vs-BRANCH reality + cross-doc consistency + round-1-fix integrity. (1) Verify against rust-gateway HEAD: does ' + SRC + '/../tests/live_dps_extended_smoke.rs exist on HEAD? (run: ls ' + REPO + '/rust/prro/tests/ + git -C ' + REPO + ' ls-files | grep live_dps). Is the "live-dps" Cargo feature in ' + REPO + '/rust/prro/Cargo.toml? Confirm the caps now correctly frame W4-Z3 as branch-proven/PENDING-MERGE (feat/m4-w4-z3) and NOT runnable on HEAD, with live_smoke_w12_hardening.rs as the HEAD harness. Verify the harness DOES exist on the branch (' + WT + '/rust/prro/tests/live_dps_extended_smoke.rs) and the 3 server_fiscal_no values appear there. (2) Cross-doc consistency: do all 4 caps agree on every WIRED/UNWIRED tag, enum vocab, and the W4-Z3 framing post-fix? Any doc still calling W4-Z3 WIRED-on-HEAD? (3) Round-1-fix integrity: did any round-1 edit introduce a NEW internal contradiction, a broken cross-link, or a half-applied fix (e.g. flipped in one section but not another)? Read all 4 docs end-to-end for this. Flag inconsistencies with exact loc.',
  },
]

const results = await parallel(
  REVIEWERS.map((r) => () =>
    agent(r.q + '\n' + RULES, { label: 'rev2:' + r.key, phase: 'Review2', schema: SCHEMA }).then((x) => ({ key: r.key, ...x }))
  )
)

return results.filter(Boolean)
