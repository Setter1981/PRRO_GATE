export const meta = {
  name: 'fix-pilot-caps',
  description: 'Apply self-review fixes to the 4 pilot caps (offline-schema flip, W4-Z3 pending-merge reframe, STOP_MODE/seam re-mark, idempotency citation, INV mis-cites, line drifts)',
  phases: [{ title: 'Fix', detail: '4 agents apply the consolidated review fixes, one per doc' }],
}

const ARCH = '/mnt/d/PRRO_GATE/docs/architecture'
const OPS = '/mnt/d/PRRO_GATE/docs/operations'

const CC = `
CANONICAL CORRECTIONS (apply consistently; these are verified ground truth — the docs currently cite the DEAD Python sql/ tree in places):

CC1 (offline_sessions schema — FLIP the inverted drift note): the Rust PILOT column is "state" (NOT "status") with CHECK (state IN ('OPENING','OPEN','DRAINING','CLOSED','ABORTED')) per migration rust/prro/migrations/015_offline_normalize.sql:140 + repo offline_sessions.rs:225. The "status"/"CLOSING" shape is the DEAD pre-015 schema (migration 004 / Python sql/001) — migration 015 normalized status/CLOSING -> state/DRAINING. So in the pilot there is NO drift: column is "state", value is DRAINING. Rewrite any note claiming the live column is "status" or "CHECK still lists CLOSING" — frame status/CLOSING as the OLD/dead naming that 015 already fixed.

CC2 (W4-Z3 live anchor — reframe as PENDING MERGE, operator chose option A): the harness rust/prro/tests/live_dps_extended_smoke.rs + the "live-dps" Cargo feature + the SHIFT_OPEN->SELL->Z cycle (server_fiscal_no 1g41M3jDt-Q / AOBSkplfIUU / L2AMnY2MkmA, proven 2026-05-29) live ONLY on the UNMERGED branch feat/m4-w4-z3-dps-extended-smoke. They are NOT present on rust-gateway HEAD (where these caps live). So: mark the W4-Z3 anchor as "PROVEN on branch feat/m4-w4-z3 / PENDING MERGE to rust-gateway — NOT on HEAD". The binding live-dps static-gate command (cargo test -p prro --features live-dps --test live_dps_extended_smoke --no-run) is NOT runnable on rust-gateway until that branch merges; the live harness that EXISTS on HEAD is live_smoke_w12_hardening.rs (--features test-support, connect/probe-only, dummy signing, no CMS). Keep the proven server_fiscal_no values but label them branch-proven/pending-merge, not HEAD-WIRED.

CC3 (CodePoolExhausted -> STOP_MODE — split the claim): the typed error CodePoolExhausted is WIRED + tested (offline_sessions.rs:408; test offline_session_code_pool.rs:201). BUT the "-> caller enters STOP_MODE" half is UNWIRED: stage_offline_ack.rs:315 propagates the error via ? ("caller's responsibility to enter STOP_MODE"); no production caller converts CodePoolExhausted to STOP_MODE. The ONLY STOP_MODE driver is drain Tier-2 trigger_tier_2_stop_mode (backlog_drain.rs:2095, fires at consecutive_holds>=50, audit OFFLINE_DRAIN_FN_STOP_MODE) — a DIFFERENT trigger. Re-mark: "CodePoolExhausted typed error WIRED+tested; STOP_MODE caller-routing UNWIRED (no production handler); distinct from drain Tier-2 STOP_MODE".

CC4 (idempotency citation): cite rust/prro/migrations/002_fiscal_documents.sql:91 (ux_inbox_fn_idem, composite UNIQUE(fiscal_number, idempotency_key)). Do NOT cite sql/001:97 — that is the dead Python single-column "idempotency_key TEXT NOT NULL UNIQUE" and contradicts the stated composite. (ALGORITHMIC_MAP already cites migr 002:91 correctly — match it.)

CC5 (force/senior seams — tested-but-undriven): force_to_error_with_audit / force_to_manual_reconciliation_with_audit / senior_cashier_close_shift_with_audit have regression-pin tests but NO production caller (no admin CLI / runtime path invokes them; drain uses shifts::transition_state directly). Re-mark as "primitive WIRED + regression-pinned; NO production driver / operator entry-point today (test-only)" — mirror the W8-probe caveat. Note Manual-recon family (3) "operator force/senior seam" is not operator-reachable on the pilot path today.

CC6 (INV mis-cites): (a) ALGORITHMIC_MAP Sign step tagged "INV-10/18" -> change to "INV-18" (no crypto in tx; INV-10 is the 168h cap, wrong). (b) PILOT_TEST_MATRIX §3.4 concurrency-stress tagged "INV-08" -> "INV-01" (single-writer; optionally + INV-19). (c) WebCheck 36h cert-expiry SHIFT_OPEN gate: MATRIX says INV-19(KeyRotationPending), MAP says INV-09 synergy — make them agree: "INV-09 synergy (KeyRotationPending = INV-19 recovery class)". (d) §3.8 "no XML rebuild after sign" tagged INV-18 -> soften to "Crypto Immutable Rule (enabled-by INV-18)". (e) §3.8 cert-validity-parse INV-17 -> "—" (crypto-correctness, not the passthrough/mock rule). (f) RUNBOOK Appendix: ADD INV-16 (excise goods need UKTZED + excise mark) — pieces 4/6 build excise <CA> + UKTZED CZD.

CC7 (line-citation drifts): force_to_error_with_audit is shifts.rs:444 (NOT 575; 575 = force_to_manual_reconciliation). provider.rs: sign_cms_detached :50, unwrap_envelope :79 (NOT :71), SignCmsRequest.canonical_xml field :33. backlog_drain escalate emit OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL :2191 (block :2138-2191). Python active-shift index sql/001:158 -> tag "(dead Python contour, historical)". Rust shifts column is "state" not "status" (fix any "shifts.status"). Migration runner: Rust uses sqlx::migrate!("./migrations") (db/mod.rs:106), NOT migrations/runner.py. Note lnd = persisted column name, local_number = wire-level name.
`

phase('Fix')

const DOCS = [
  {
    key: 'MAP',
    label: 'fix:MAP',
    file: ARCH + '/ALGORITHMIC_MAP.md',
    pointers: 'Apply: CC1 (the §1.3 offline_sessions drift note); CC2 (§1.11 WIRED baseline + §1.2 step-5 + gap-table native-crypto/W4-Z3 rows); CC3 (§1.3 machine-4 + §1.9 + §1.10 audit "CodePoolExhausted -> STOP_MODE"); CC5 (§1.3 + §1.11 force/senior seams); CC6 (§1.2 Sign step INV-10/18 -> INV-18; §1.2 step-5 wire-send INV-08 -> annotate "trigger surface"; gap-table WebCheck cert-gate reconcile to "INV-09 synergy (KeyRotationPending=INV-19)"); CC7 (§1.3 force_to_error -> shifts.rs:444; §1.8 provider.rs :49/:71 -> :50/:79 + canonical_xml :33; line-21 WL-1 cross-link ellipsis -> exact docs/superpowers/plans/2026-05-29-online-shift-lifecycle-wiring.md; lnd-vs-local_number note).',
  },
  {
    key: 'MATRIX',
    label: 'fix:MATRIX',
    file: ARCH + '/PILOT_TEST_MATRIX.md',
    pointers: 'Apply: CC1 (§3.5 drift + the "shifts.status" -> "shifts.state" in §3.4 acceptance); CC2 (the W4-Z3 live anchor in §2/§4/§5 + the binding live-dps --no-run static-gate command -> mark PENDING MERGE / not on HEAD; note the exit-criteria items §5.2/§5.4 cannot pass on HEAD until the branch merges); CC3 (§3.6 code-pool-exhaustion -> STOP_MODE); CC4 (the idempotency citation sql/001:97 -> migrations/002:91 ux_inbox_fn_idem); CC5 (§3.10/§3.11 force/senior seams); CC6 (§3.4 INV-08 -> INV-01; §3.8 no-XML-rebuild INV-18 -> "Crypto Immutable Rule (enabled-by INV-18)"; §3.8 cert-validity-parse INV-17 -> "—"; cert-gate §236 reconcile to "INV-09 synergy (KeyRotationPending=INV-19)"); CC7 (§3.5 migrations/runner.py -> sqlx::migrate! db/mod.rs:106; provider.rs lines).',
  },
  {
    key: 'PLAYBOOK',
    label: 'fix:PLAYBOOK',
    file: ARCH + '/PILOT_REVIEW_PLAYBOOK.md',
    pointers: 'Apply: CC1 (§1 + the §2.1 finding-template that treats querying "state" as a finding — FLIP it so querying "state" is CORRECT and asserting "status"/"CLOSING" is the dead-Python stale pattern); CC2 (§4 WIRED ledger W4-Z3 anchor + §10 static gate -> PENDING MERGE); CC3 (§3.3/§4 code-pool -> STOP_MODE); CC4 (§1 idempotency citation sql/001 ~97 -> migrations/002:91); CC5 (§3.1/§3.4/§4 force/senior seams -> tested-but-undriven); CC7 (provider.rs lines if cited).',
  },
  {
    key: 'RUNBOOK',
    label: 'fix:RUNBOOK',
    file: OPS + '/LIVE_DPS_SMOKE_RUNBOOK.md',
    pointers: 'Apply: CC1 (the Appendix INV-13 / offline_sessions drift line ~343 — column is "state"/DRAINING not "status"/CLOSING); CC2 (THE BIG ONE: the opening "written from the proven harness live_dps_extended_smoke.rs" + §4.5 test table + env contract + triple-gate — the harness + live-dps feature are on branch feat/m4-w4-z3, NOT on rust-gateway HEAD; reframe as "this runbook documents the smoke on branch feat/m4-w4-z3 (pending merge); on rust-gateway HEAD only live_smoke_w12_hardening.rs (connect/probe, --features test-support) exists". Keep the proven server_fiscal_no values labeled branch-proven 2026-05-29); CC6 (ADD INV-16 excise to the Appendix "invariants this smoke touches" — pieces 4/6 build excise <CA> + UKTZED CZD).',
  },
]

const results = await parallel(
  DOCS.map((d) => () =>
    agent('Apply the self-review fixes to the pilot gate document ' + d.file + '. Read the file, then apply ONLY the corrections below via Edit (preserve everything else; keep the doc internally consistent). ' + d.pointers + '\n' + CC + '\nReturn a short confirmation listing which CCs you applied and any finding you could NOT locate in the doc.', { label: d.label, phase: 'Fix' }).then((r) => ({ key: d.key, confirmation: r }))
  )
)

return results.filter(Boolean)
