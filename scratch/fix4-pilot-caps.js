export const meta = {
  name: 'fix4-pilot-caps',
  description: 'Round-3 caps residual fixes: PLAYBOOK/RUNBOOK flat-WIRED drain-reject, forensic-snapshot, shift_state Closed-not-Opened + offline-unreachable-end-to-end, 24h limit, RESOLVED-token',
  phases: [{ title: 'Fix4', detail: '4 agents apply round-3 caps fixes, one per cap' }],
}

const ARCH = '/mnt/d/PRRO_GATE/docs/architecture'
const OPS = '/mnt/d/PRRO_GATE/docs/operations'

const CFR = `
CANONICAL CORRECTIONS (round-3 review residuals; code-verified):

CF-R1 (PLAYBOOK §3.4 + §4-WIRED-list + RUNBOOK §4.8 — residual flat-WIRED that fix3 MISSED): the drain-reject of an OFFLINE_LOCAL_ACK backlog on a pending-drain shift -> REQUIRES_MANUAL_RECONCILIATION + OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL, AND the edge-5 drain-finalize-opens-shift, are still tagged flat WIRED in these spots — contradicting the SAME doc's §3.1/§4-UNWIRED-ledger and the other caps (which fix3 already corrected to UNWIRED-in-prod). Re-tag them UNWIRED IN PRODUCTION consistent with DF-1: the escalation is reached only "if shift_in_pending_drain" (backlog_drain.rs:952), keyed on a pending-drain shift_state production never sets; code-present + test-pinned (backlog_drain.rs:2191) but UNREACHABLE in prod. Keep ONLY the doc-finalize-to-Ack as the (narrowly) wired part — and even that is moot in prod (see CF-R3). Add a DF-1 cross-reference to the RUNBOOK §4.9 hard-blocker list so it matches MATRIX §5 / PLAYBOOK §9.

CF-R2 (PLAYBOOK §3.4 line ~260 + discipline note ~280 — forensic snapshot): these still read as asserting a forensic snapshot EXISTS/emits ('Verify the Critical audit + forensic snapshot actually emit'; 'Every Manual landing must carry the Critical audit + forensic snapshot + pager'). Per DF-4 (verified) there is NO forensic-snapshot-capture or operator-pager code on HEAD — only the Critical audit emits. Qualify: drop 'forensic snapshot' from the 'actually emit and survive' verification, or mark '(forensic snapshot + pager UNWIRED per DF-4 — only the Critical audit emits)'. The §280 discipline note may stay as an aspirational acceptance criterion but must be flagged '(snapshot + pager are UNWIRED today — DF-4)'.

CF-R3 (shift_state Closed-not-Opened + offline-unreachable-END-TO-END — corrects a recurring premise across MAP §1.3/§1.11, MATRIX §3.6/§3.10, PLAYBOOK §3.1/§4, RUNBOOK): several caps repeat 'in the realistic prod flow (shift_state statically seeded to Opened so SELLs are admitted) the drain finalizes the OFFLINE_LOCAL_ACK backlog to Ack via the Opened->None arm'. THIS PREMISE IS WRONG. The ONLY production upsert_initial seeds ShiftState::CLOSED (boot_phase.rs:1304: upsert_initial(pool, fn, NodeMode::Online, ShiftState::Closed, 1)); orphan boot resolution only drives toward CLOSED (boot_phase.rs:1491). The only 'OPENED' write in src is a #[cfg(test)] fixture (admin.rs:903). Under CLOSED: (Sell, Closed) -> ShiftNotOpen REFUSE (stage_acquire.rs:897) — SELL is NOT admitted on either channel. Moreover offline is unreachable END-TO-END: node_state has NO Offline/GoingOffline mode setter (only set_mode_blocked_tx/set_mode_stop_mode_tx); OfflineSessionService::open_session has ZERO production callers; stage_offline_ack requires Opened + an active offline session (stage_offline_ack.rs:268-318). So mode never flips Offline, no offline session opens, no OFFLINE_LOCAL_ACK doc forms, no backlog exists, and drain() early-returns 'no active session'/'empty backlog' — the Opened->None finalize arm is itself unreachable in prod. REWORD: prod bootstrap is CLOSED (boot_phase.rs:1304), so the gateway CANNOT transact at all today (online SELL refused on Closed; offline path unreachable end-to-end — no mode setter, no session, no backlog). The 'drain finalizes backlog to Ack without escalation' scenario does not occur in prod (no backlog forms). This STRENGTHENS the NO-GO; the 'silent non-functional safety' conclusion stands but the stated mechanism (Opened->None) must be replaced with this stronger end-to-end-unreachable framing.

CF-R4 (24h continuous-shift-duration limit — distinct missing legal limit, MATRIX): the caps surface INV-09 (36h continuous offline) + INV-10 (168h monthly) but NOT the separate 24h continuous-SHIFT-duration wall (LEGAL_INVARIANTS.md §8 compliance-gate item 1, 'Active engineering risk'). There is NO 24h enforcement in src (grep 24*3600/86400/MAX_SHIFT/shift_duration empty). ADD an explicit UNWIRED row for the 24h continuous-shift limit in MATRIX §3.6/§3.10 with the same risk-accept-or-enforce framing as INV-09/10, and note in the §5 hard-blocker that the 24h shift wall is a third distinct limit whose only compliant exit is an offline Z_REPORT local close (W10) — itself UNWIRED.

CF-R5 (ALGORITHMIC_MAP §1.11 gap-table 'RESOLVED' token for native crypto): the Severity cell reads 'RESOLVED on branch; HEAD detached / pending merge', but Hard-Blocker (2) lists the same item as an open NO-GO blocker — the only place a headline token understates an active blocker. Change the §1.11 gap-table severity token to 'Hard-Blocker (2) — branch-resolved, HEAD-blocked' so the severity column never reads RESOLVED for an open pilot blocker.

CF-R6 (minor citation precision, MATRIX/PLAYBOOK): stage_offline_ack.rs:165 (cited for 'emits OFFLINE_LOCAL_ACK_APPLIED / lands OFFLINE_LOCAL_ACK') is the fn-entry line — point the transition at :327 and the audit at :350 (match ALGORITHMIC_MAP's precise anchors). Optionally note the drain Tier-2 'consecutive_holds >= 50' also requires the HeldAtSent/HeldAtKvt1 projection co-condition (backlog_drain.rs:931-937).
`

phase('Fix4')

const DOCS = [
  {
    key: 'MAP',
    label: 'fix4:MAP',
    file: ARCH + '/ALGORITHMIC_MAP.md',
    pointers: 'Apply: CF-R3 (correct the shift_state-statically-Opened premise in §1.3/§1.11 -> CLOSED + offline-unreachable-end-to-end, stronger NO-GO); CF-R5 (§1.11 gap-table RESOLVED token -> Hard-Blocker(2) branch-resolved/HEAD-blocked). NOTE: §1.10 forensic-snapshot was already split by DF-4 (verify it stayed split). CF-R4 24h: add a one-line note in the §1.11 gap table that the 24h continuous-SHIFT limit is a third distinct UNWIRED limit (LEGAL §8 item 1).',
  },
  {
    key: 'MATRIX',
    label: 'fix4:MATRIX',
    file: ARCH + '/PILOT_TEST_MATRIX.md',
    pointers: 'Apply: CF-R3 (shift_state Closed + offline-unreachable-end-to-end in §3.6/§3.10); CF-R4 (add an explicit 24h continuous-shift-duration UNWIRED row in §3.6/§3.10 + note in §5 hard-blocker); CF-R6 (stage_offline_ack.rs:165 -> :327/:350; consecutive_holds projection co-condition).',
  },
  {
    key: 'PLAYBOOK',
    label: 'fix4:PLAYBOOK',
    file: ARCH + '/PILOT_REVIEW_PLAYBOOK.md',
    pointers: 'Apply: CF-R1 (§3.4 first bullet + §4-WIRED-list line ~297: re-tag the drain-reject->manual escalation UNWIRED-in-prod, consistent with the same doc §3.1/§4-UNWIRED-ledger — fix3 missed these two spots); CF-R2 (§3.4 ~260 + discipline note ~280: forensic-snapshot/pager UNWIRED qualifier); CF-R3 (§3.1/§4: shift_state Closed + offline-unreachable-end-to-end); CF-R6 (stage_offline_ack cite :327/:350 if present).',
  },
  {
    key: 'RUNBOOK',
    label: 'fix4:RUNBOOK',
    file: OPS + '/LIVE_DPS_SMOKE_RUNBOOK.md',
    pointers: 'Apply: CF-R1 (§4.8 ARE-wired list: move drain-reject->manual + edge-5 OUT of flat-WIRED into the UNWIRED-in-prod/DF-1 qualifier the other 3 caps carry; the runbook currently has 0 DF-1 mentions — add the correction; add a DF-1 cross-ref to §4.9 so the blocker list matches MATRIX §5/PLAYBOOK §9, incl. the offline drain-safety silent-absence); CF-R3 (correct any shift_state-Opened/realistic-prod-flow wording to CLOSED + offline-unreachable-end-to-end).',
  },
]

const results = await parallel(
  DOCS.map((d) => () =>
    agent('Apply the round-3 review residual fixes to ' + d.file + '. Read the file, then apply ONLY the corrections below via Edit (preserve everything else; keep the doc internally consistent). ' + d.pointers + '\n' + CFR + '\nReturn a short confirmation listing which CF-Rs you applied + any you could NOT locate.', { label: d.label, phase: 'Fix4' }).then((r) => ({ key: d.key, confirmation: r }))
  )
)

return results.filter(Boolean)
