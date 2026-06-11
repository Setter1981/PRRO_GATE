export const meta = {
  name: 'fix2-pilot-caps',
  description: 'Apply round-2 review fixes: edge-2 re-tag, wrong audit string, runbook STOP_MODE half-fix, trigger_tier_2 line, NodeMode line, CHECK/sql-path citations, LEGAL source fixes',
  phases: [{ title: 'Fix2', detail: '5 agents apply round-2 fixes, one per doc' }],
}

const ARCH = '/mnt/d/PRRO_GATE/docs/architecture'
const OPS = '/mnt/d/PRRO_GATE/docs/operations'
const DOCS_DIR = '/mnt/d/PRRO_GATE/docs'

const CF = `
CANONICAL CORRECTIONS (round-2 review; all code-verified — apply consistently):

CF-M1 (edge-2 over-claim — re-tag): shift edge 2 CREATED->OPENED_LOCAL_PENDING_DRAIN (offline SHIFT_OPEN ingress, Pattern C) is currently marked WIRED. It is NOT: shift CREATION has NO production driver — shifts::insert_created (shifts.rs:119) has ZERO production callers (only tests/repo_shifts.rs); the only "INSERT INTO shifts" in src is under #[cfg(test)] (backlog_drain.rs:2953, cfg(test) opens at :2753); stage_offline_ack only READS ns.shift_state to GUARD (:268-289), never creates/transitions a shift; node_state.current_shift_id is never set in production. So edge 2 (offline shift CREATION) is UNWIRED — same class as online edges 3/8/10. The drain TRANSITION edges 5/6/7/9/13/14 ARE code-wired + tested (backlog_drain.rs:2169/:2498, prod caller app.rs:620) BUT they only transition pre-existing shift rows — which production never creates. FIX: re-tag edge 2 (and clarify edge 1) so the shift-CREATION step is UNWIRED; keep the drain TRANSITION edges WIRED but add the qualifier "operate on test-seeded shift rows only — production never creates a shift row (insert_created undriven)". Add a one-line note tying this to the WL-1 shift-lifecycle gap (the shifts table is not production-populated today).

CF-M2 (wrong audit string): the audit-event identifier "ONLINE_Z_REPORT_BLOCKED_BACKLOG" does NOT exist in code. The real (ZReport, OpenedLocalPendingDrain) guard audit shape is "OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED" (stage_acquire.rs:782; doc-comment types.rs:213). Replace every "ONLINE_Z_REPORT_BLOCKED_BACKLOG" with "OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED". Keep the INV-15 tag + WIRED status unchanged (those are correct).

CF-M3 (RUNBOOK §4.8 half-applied round-1 fix): the runbook foundations list still asserts "code-pool exhaustion -> STOP_MODE" and the force/senior seams as flat WIRED. Split/qualify like the 3 arch caps: "code-pool exhaustion -> typed CodePoolExhausted (WIRED+tested); the -> STOP_MODE caller-routing is UNWIRED (stage_offline_ack.rs:315; the only wired STOP_MODE driver is the distinct drain Tier-2 trigger_tier_2_stop_mode)"; and "the force/senior reconciliation seams (primitive WIRED + regression-pinned, but NO production driver / operator entry-point today — test-only)".

CF-L1 (trigger_tier_2_stop_mode line): change backlog_drain.rs:2095 -> backlog_drain.rs:2074 (the "async fn trigger_tier_2_stop_mode" definition; :2095 is a mid-body set_mode_stop_mode_tx call). ALGORITHMIC_MAP already uses :2074 — match it everywhere.

CF-L2 (NodeMode pre-guards line): where the NodeMode pre-guards (GoingOnline/Blocked/StopMode/CryptoDegraded refuse) are cited at stage_acquire.rs:383, change to stage_acquire.rs:293-362 (the pre-guard match arms; :293/:315/:331/:347). Keep :383 ONLY for the check_shift_guard CALL (the 162-cell shift-state guard).

CF-L3 (offline CHECK citation): for the offline_sessions "column=state, CHECK=(OPENING/OPEN/DRAINING/CLOSED/ABORTED)" claim, cite migration 015_offline_normalize.sql:140 as the CHECK source (+ enum enums.rs:54 for the value set). The repo line offline_sessions.rs:225 is an UPDATE statement (uses the "state" column but does NOT hold the CHECK) — narrow or drop it for the CHECK-constraint claim (it can stay as repo-uses-state evidence, just not as the CHECK source).

CF-L4 (dead-Python path shorthand): expand "sql/001" / "sql/001:158" to "sql/001_hot_store_init.sql:158" so the dead-Python file resolves unambiguously (it must NOT be confused with the Rust pilot migration rust/prro/migrations/001_core_identities.sql).

CF-I1 (drain-reject doc-vs-shift state): clarify that on a drain TERMINAL-reject the failing DOC goes to REJECTED and only the SHIFT goes to REQUIRES_MANUAL_RECONCILIATION (escalate_drain_to_manual transitions the shift only). The doc-level -> REQUIRES_MANUAL_RECONCILIATION edge applies to the DIFFERENT ER-budget-exhausted subtype (backlog_drain.rs:1552). The escalation + Critical audit + halt are all WIRED+tested — this is precision only.

CF-I2 (stage_offline_ack transition line): point the "Signed -> OfflineLocalAck" Pattern-C transition citation at stage_offline_ack.rs:320 (the transition); keep :350 for the OFFLINE_LOCAL_ACK_APPLIED audit. :165 is the run() doc-comment, not the transition.

CF-I3 (LEGAL_INVARIANTS source fix): in /mnt/d/PRRO_GATE/docs/LEGAL_INVARIANTS.md, the §8 status table row "Production crypto startup gate" (~line 197) is marked "✅ Реалізовано (M3a)" which CONTRADICTS INV-17's own body (~line 142: "GAP — стартовий блокер ... не реалізований"). Correct the §8 row to ⚠/GAP to match INV-17 body. (The caps' INV-17 GAP treatment is correct — do NOT touch the caps for this.)
`

phase('Fix2')

const DOCS = [
  {
    key: 'MAP',
    label: 'fix2:MAP',
    file: ARCH + '/ALGORITHMIC_MAP.md',
    pointers: 'Apply: CF-M1 (edge-2 re-tag in §1.1 + §1.3 shift table + §1.9 + the §1.11 WIRED baseline edge list); CF-M2 (the ONLINE_Z_REPORT_BLOCKED_BACKLOG string in §1.4 + §1.10 audit table); CF-L3 (the §1.3 machine-4 offline CHECK citation offline_sessions.rs:225 -> migration 015:140 + enums.rs:54); CF-L4 (sql/001 -> sql/001_hot_store_init.sql in §1.4 + §1.11); CF-I1 (drain-reject doc/shift clarify in §1.9 + §1.1); CF-I2 (stage_offline_ack transition cite :165 -> :320 in §1.2 + §1.11). NOTE: this doc ALREADY cites trigger_tier_2 at :2074 (correct, no change) and ALREADY cites NodeMode pre-guards correctly (no change).',
  },
  {
    key: 'MATRIX',
    label: 'fix2:MATRIX',
    file: ARCH + '/PILOT_TEST_MATRIX.md',
    pointers: 'Apply: CF-M1 (edge-2 in §3.6 + §3.10 edge list); CF-M2 (ONLINE_Z_REPORT_BLOCKED_BACKLOG in §3.10); CF-L1 (backlog_drain.rs:2095 -> :2074 in §3.6); CF-L2 (NodeMode pre-guards row §3.10: stage_acquire.rs:383 -> :293-362; keep :383 for the shift-guard call); CF-L3 (§3.5 offline CHECK cite); CF-L4 (sql/001 -> sql/001_hot_store_init.sql in §3.10).',
  },
  {
    key: 'PLAYBOOK',
    label: 'fix2:PLAYBOOK',
    file: ARCH + '/PILOT_REVIEW_PLAYBOOK.md',
    pointers: 'Apply: CF-M1 (edge-2 in §3.1 WIRED-edges ledger + §4 ledger); CF-L1 (backlog_drain.rs:2095 -> :2074 in §3.3 + §4); CF-L4 (sql/001 -> sql/001_hot_store_init.sql in §4). NOTE: §3.1 cites :383 for the check_shift_guard CALL correctly (keep). Check whether the offline CHECK note (§1) needs CF-L3; if it cites offline_sessions.rs:225 as the CHECK source, narrow it to migration 015:140.',
  },
  {
    key: 'RUNBOOK',
    label: 'fix2:RUNBOOK',
    file: OPS + '/LIVE_DPS_SMOKE_RUNBOOK.md',
    pointers: 'Apply: CF-M3 (§4.8 foundations list: split "code-pool exhaustion -> STOP_MODE" + add the force/senior "no production driver / test-only" qualifier — THE BIG ONE for this doc); CF-L3 (the INV-13 / offline_sessions appendix line ~370: cite migration 015:140 for the CHECK, narrow offline_sessions.rs:225); CF-L4 (sql/001 -> sql/001_hot_store_init.sql in the appendix if cited).',
  },
  {
    key: 'LEGAL',
    label: 'fix2:LEGAL',
    file: DOCS_DIR + '/LEGAL_INVARIANTS.md',
    pointers: 'Apply ONLY two precise factual corrections, preserve everything else: CF-M2 (in INV-15, replace the audit string ONLINE_Z_REPORT_BLOCKED_BACKLOG with OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED — this doc is the upstream origin of the wrong string the caps inherited); CF-I3 (§8 status table row "Production crypto startup gate" ~line 197: change "✅ Реалізовано (M3a)" to ⚠/GAP to match INV-17 body ~line 142). Do NOT make any other change.',
  },
]

const results = await parallel(
  DOCS.map((d) => () =>
    agent('Apply the round-2 review fixes to ' + d.file + '. Read the file, then apply ONLY the corrections below via Edit (preserve everything else; keep the doc internally consistent). ' + d.pointers + '\n' + CF + '\nReturn a short confirmation listing which CFs you applied + any you could NOT locate.', { label: d.label, phase: 'Fix2' }).then((r) => ({ key: d.key, confirmation: r }))
  )
)

return results.filter(Boolean)
