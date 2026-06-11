export const meta = {
  name: 'review3-pilot',
  description: 'Round-3 deep code-grounded review: post-fix3 caps convergence + WL-1 Option A-prime pre-implementation soundness',
  phases: [{ title: 'Review3', detail: '6 reviewers, each OPENS code + cites file:line evidence' }],
}

const REPO = '/mnt/d/PRRO_GATE'
const SRC = REPO + '/rust/prro/src'
const MIG = REPO + '/rust/prro/migrations'
const WT = '/mnt/d/prro_gate_m4_w4_z3'
const CAPS = REPO + '/docs/architecture/ALGORITHMIC_MAP.md, ' + REPO + '/docs/architecture/PILOT_TEST_MATRIX.md, ' + REPO + '/docs/architecture/PILOT_REVIEW_PLAYBOOK.md, ' + REPO + '/docs/operations/LIVE_DPS_SMOKE_RUNBOOK.md'
const WL1 = REPO + '/docs/superpowers/plans/2026-05-29-online-shift-lifecycle-wiring.md'
const SPEC = REPO + '/docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md'

const RULES = `
HARD RULES (operator: deep, code-grounded, trust nothing, verify in code):
1. Trust NOTHING — not the docs, not the plan, not prior reviews, not memory. For EVERY claim you assess, OPEN the actual source file (` + SRC + ` / ` + MIG + `) and READ it. Cite the EXACT file:line you opened as evidence for every finding AND every confirmed-good.
2. Code claims verify against rust-gateway HEAD (` + SRC + `). The W4-Z3 native ATTACHED-crypto + live harness live ONLY on the unmerged branch worktree ` + WT + ` — the caps frame them "branch-proven / PENDING MERGE / not on HEAD"; verify that framing.
3. Two migration trees: Python ` + REPO + `/sql/*.sql (DEAD) vs Rust ` + MIG + ` (pilot). A pilot claim on the Python tree without a dead-Python tag is a defect.
4. Background: this gate was through 2 internal review rounds + 1 external review (verdict PILOT NO-GO). The caps were just fixed (fix3) to record NO-GO + a CORRECTED offline-drain framing: the drain does NOT "always crash" — the offline Pattern C shift-safety semantics are SILENTLY NON-FUNCTIONAL (pending-drain shift_state never set; escalate guarded by shift_in_pending_drain at backlog_drain.rs:952 unreachable; commit_finalize Opened->None no-crash at :2399-2418). Re-verify this is stated correctly and not re-broken.
5. If you cannot verify a claim, mark it UNVERIFIED — never assume.
`

const SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['verdict', 'findings', 'confirmed_good', 'unverified'],
  properties: {
    verdict: { type: 'string', enum: ['PASS', 'CHANGES_REQUESTED'] },
    findings: { type: 'array', items: { type: 'object', additionalProperties: false,
      required: ['severity', 'target', 'claim', 'code_reality', 'evidence', 'fix'],
      properties: {
        severity: { type: 'string', enum: ['Critical', 'High', 'Medium', 'Low', 'Info'] },
        target: { type: 'string', description: 'which doc/plan + section/line' },
        claim: { type: 'string' }, code_reality: { type: 'string' },
        evidence: { type: 'string', description: 'EXACT file:line you opened' }, fix: { type: 'string' },
      } } },
    confirmed_good: { type: 'array', items: { type: 'string' } },
    unverified: { type: 'array', items: { type: 'string' } },
  },
}

phase('Review3')

const REVIEWERS = [
  {
    key: 'caps_fix3_integrity',
    q: 'ROUND-3 review of the post-fix3 caps (' + CAPS + '). Verify each of the just-applied external-review fixes is CODE-CORRECT and introduced NO new error/inconsistency. OPEN the cited source. Check: DF-1 the CORRECTED drain re-tag (drain edges 5/6/7/9/13/14 now UNWIRED-in-prod with "silent non-functional, not a crash" framing — verify against backlog_drain.rs:952 / :2399-2418 it is stated correctly, the doc-finalize-works vs shift-transition-unreachable split is accurate, and NO residual "WIRED test-seeded" or "always crashes" over-claim survives except as refutation); DF-2 stale CryptBadSign row replaced with RESOLVED/branch-proven attached; DF-3 detached(HEAD)-vs-attached(branch) crypto — verify HEAD in_process.rs is detached and the attached signer is branch-only; DF-4 manual pager/snapshot split (Critical audit WIRED vs snapshot+pager UNWIRED — confirm no pager/snapshot code on HEAD); DF-8 the NO-GO verdict + 5 hard blockers are accurate. Flag any fix that is wrong, half-applied, or created an internal contradiction.',
  },
  {
    key: 'caps_consistency_completeness',
    q: 'ROUND-3 review of the post-fix3 caps (' + CAPS + ') for CROSS-DOC CONSISTENCY + COMPLETENESS + ANYTHING STILL MISSED. (1) Do all 4 caps now agree on: the NO-GO verdict, the 5 hard blockers, the corrected drain framing, every WIRED/UNWIRED tag, the W4-Z3 pending-merge + detached/attached crypto framing? Any doc still contradicting another (e.g. a stale WIRED tag, an old "test-seeded" phrasing, a forensic-snapshot claim left unsplit)? (2) Is the NO-GO hard-blocker list COMPLETE — given the external review found issues 2 internal rounds missed, go deeper: is there ANY other production over-claim or legally-significant gap (INV-01..20) not yet flagged? Read CLAUDE.md frozen invariants + LEGAL_INVARIANTS.md and cross-check coverage. Flag inconsistencies + any newly-found gap with code evidence.',
  },
  {
    key: 'caps_wired_unwired_reaudit',
    q: 'ROUND-3 fresh WIRED/UNWIRED re-audit of the caps (' + CAPS + ') against ' + SRC + '. Re-verify EVERY WIRED claim has a real production driver + test, and EVERY UNWIRED claim genuinely has no production driver — with fresh eyes, do NOT trust prior rounds. Focus hardest on the recently re-tagged shift/drain area (the whole shift lifecycle, the drain edges, current_shift_id, online edges 3/4/8/10/11/12, offline edges 1/2, the static-seeded shift_state claim). Also re-check: 162-cell guard, Pattern C OFFLINE_LOCAL_ACK, CodePoolExhausted/STOP_MODE split, force/senior seams. FLAG the dangerous direction hardest: anything marked WIRED that is actually unwired/unreachable in production. Cite grep + file:line evidence.',
  },
  {
    key: 'wl1_design_correctness',
    q: 'ROUND-3 PRE-IMPLEMENTATION review of the WL-1 plan ' + WL1 + ' — specifically the §0 REVISION Option A-prime (operator just chose A-prime). Does A-prime ACTUALLY fix offline Pattern C? OPEN backlog_drain.rs + stage_offline_ack.rs + stage_acquire.rs + shifts.rs + the spec ' + SPEC + '. Verify: (1) If stage_acquire creates the shift row (insert_created -> Created) + sets node_state.current_shift_id on SHIFT_OPEN for BOTH channels, and stage_offline_ack drives edge 2 (Created->OpenedLocalPendingDrain) on offline SHIFT_OPEN + edge 9 (Opened->ClosingLocalPendingDrain) on offline Z — does the EXISTING drain code (escalate_drain_to_manual :2147, commit_finalize :2399) then become reachable + correct (current_shift_id now Some, pending-drain states now set)? (2) Is edge 2/9 the right edge per the spec, and is stage_offline_ack the right hook (it currently only READS shift_state)? (3) Are there HOLES: does the plan account for the shift ROW needing to exist before edge 2 fires? does mirror_node_state_shift_state_tx (the drain uses it) work once current_shift_id is set? (4) Is "drain unchanged" actually true, or does the drain need any change to consume the now-set state? Flag any gap that would leave Pattern C still broken after A-prime.',
  },
  {
    key: 'wl1_invariants_edgecases',
    q: 'ROUND-3 review of WL-1 ' + WL1 + ' Option A-prime for INVARIANT PRESERVATION + EDGE CASES (hot-zone, pre-implementation). OPEN the relevant code. Assess: (1) Frozen invariants — #1 no network/crypto in write-tx (the new shift writes are DB-only inside existing with_immediate envelopes?), #2 single-writer per fiscal_number (stage_acquire/stage_send/stage_offline_ack under the lease?), INV-03 shift-open-before-ops preserved, idempotence on reconcile re-drive (a re-driven SHIFT_OPEN must not double-create the shift / double-set current_shift_id — is there a CAS guard?). (2) THE MIGRATION-FROM-BROKEN-STATE problem: production FNs TODAY have node_state.shift_state statically = Opened (seeded) with NO shift row and current_shift_id = NULL. When A-prime ships, how does an already-"open" FN reconcile — does the first SHIFT_OPEN create a shift while shift_state is already Opened (guard (ShiftOpen,Opened)->ShiftAlreadyOpen)? Is there a backfill/migration story? This is a real deployment hole — verify the plan addresses it or flag it. (3) Edge cases: double SHIFT_OPEN, SHIFT_OPEN offline then mode flips online mid-shift, crash between shift-create and the confirm edge, Z on a shift with no row. Flag each unhandled case.',
  },
  {
    key: 'wl1_spec_completeness',
    q: 'ROUND-3 review of WL-1 ' + WL1 + ' Option A-prime for SPEC-ALIGNMENT + COMPLETENESS vs the authoritative M3b 9-state shift spec ' + SPEC + '. OPEN the spec (esp. §3.3 online-ops-resume rule, §4.1 the 14-edge table, §16 reality-alignment). Verify A-prime covers ALL required edges (1 Created->Opening, 2 Created->OpenedLocalPendingDrain, 3 Opening->Opened, 8 Opened->Closing, 9 Opened->ClosingLocalPendingDrain, 10 Closing->Closed, + drain 5/6/13/14) at the right hooks, and respects §3.3 (online ops resume only after full backlog drain + mode GoingOnline->Online — does A-prime\'s edge-5 OpenedLocalPendingDrain->Opened honor that the drain must complete first?). Check: does A-prime correctly handle the OpenedLocalPendingDrain online-ops lockout (the spec refuses online SELL on OpenedLocalPendingDrain)? Does the plan\'s hook table match the spec\'s edge->trigger mapping? Flag any spec edge the plan omits/mis-hooks, and any §16 reality-alignment rule the plan violates.',
  },
]

const results = await parallel(
  REVIEWERS.map((r) => () =>
    agent(r.q + '\n' + RULES, { label: 'rev3:' + r.key, phase: 'Review3', schema: SCHEMA }).then((x) => ({ key: r.key, ...x }))
  )
)

return results.filter(Boolean)
