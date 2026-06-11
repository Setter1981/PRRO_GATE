export const meta = {
  name: 'fix3-pilot-caps',
  description: 'Apply external-review fixes (NO-GO): corrected drain re-tag, stale CryptBadSign, detached-vs-attached crypto, pager/snapshot split, PRRO_FISCAL_MODE, INV-09/10 framing, secrets hygiene, PILOT NO-GO verdict',
  phases: [{ title: 'Fix3', detail: '4 agents apply the external-review fixes, one per cap' }],
}

const ARCH = '/mnt/d/PRRO_GATE/docs/architecture'
const OPS = '/mnt/d/PRRO_GATE/docs/operations'

const DF = `
CANONICAL CORRECTIONS (external review, NO-GO verdict; code-verified — the drain mechanism below is the CORRECTED one, NOT the reviewer's "always crashes"):

DF-1 (drain transition edges 5/6/7/9/13/14 — CORRECTED re-tag; the load-bearing fix): these are currently tagged either "WIRED (test-seeded rows only)" or (per the external reviewer) "always crashes at 2155". BOTH are wrong. The VERIFIED reality: the drain's shift-transition + manual-escalation logic is keyed on a PENDING-DRAIN shift_state (OpenedLocalPendingDrain / ClosingLocalPendingDrain) which PRODUCTION NEVER SETS (offline shift-creation edge 2 is UNWIRED; stage_offline_ack only READS shift_state, never sets it; node_state.current_shift_id is never set in prod). Concretely: escalate_drain_to_manual (the current_shift_id check / "crash" at backlog_drain.rs:2155) is only reached "if shift_in_pending_drain" (backlog_drain.rs:952) -> UNREACHABLE in prod. commit_finalize (backlog_drain.rs:2399-2418): Opened -> None (NO transition, NO crash); OpenedLocalPendingDrain/ClosingLocalPendingDrain -> needs current_shift_id (never reached); any OTHER shift_state -> BootError::Internal. So in the realistic prod flow (shift_state statically seeded to Opened so SELLs are admitted) the drain FINALIZES the OFFLINE_LOCAL_ACK backlog docs to Ack (advances the MAC chain) WITHOUT any shift transition and WITHOUT escalation -- it does NOT crash. RE-TAG: mark these edges UNWIRED in production -- "the offline Pattern C shift SAFETY semantics (pending-drain online-ops lockout per §3.3, and drain-reject -> RequiresManualReconciliation escalation INV-19) are NON-FUNCTIONAL because the pending-drain states they key on are never set; the drain's doc-finalize works but performs no shift transition/escalation. NOT a crash (fail-stop) -- a silent absence of the safety machinery, which for a fiscal system is worse than a crash." Evidence: backlog_drain.rs:952 (escalate guarded by shift_in_pending_drain), :2399-2418 (finalize match: Opened->None / pending-drain->current_shift_id / other->Internal). Add this to the Hard-Blocker / pilot-NO-GO list.

DF-2 (stale CryptBadSign row): ALGORITHMIC_MAP §1.11 (and anywhere else) has a STALE row asserting "native prro_crypto CMS signature REJECTED by DPS (CryptBadSign)". That blocker is RESOLVED. Replace with: "RESOLVED -- native ATTACHED CAdES-BES signature is branch-proven (W4-Z3 live cycle accepted by DPS); the fix (signing-cert selection + detached->attached) lives on the unmerged feat/m4-w4-z3 branch; HEAD's in-process signer is still DETACHED and NOT live-DPS-accepted."

DF-3 (detached-vs-attached crypto on HEAD): §1.8 (MAP) + §3.8 (MATRIX) describe outbound as "CMS-detached SIGNED" while also citing the W4-Z3 live-accepted path. These are DIFFERENT: rust-gateway HEAD's InProcessProvider signs DETACHED CMS (no eContent) and is NOT live-DPS-accepted; the ATTACHED CMS + signingTime signer that DPS actually accepted exists ONLY on the unmerged feat/m4-w4-z3 branch. CLARIFY explicitly: "HEAD = detached signer (not live-accepted); pilot-accepted native ATTACHED signer is branch-only, pending merge + external review." (Evidence: HEAD rust/prro/src/crypto/in_process.rs detached; branch in_process.rs attached.)

DF-4 (manual-recon pager/snapshot split): MATRIX §3.11 + ALGORITHMIC_MAP §1.10 claim "every Manual landing emits Critical audit + forensic snapshot + pager". Code confirms ONLY the Critical audit (backlog_drain.rs:2191 OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL; force seam shifts.rs). NO code evidence for a forensic-snapshot capture or an operator pager. SPLIT: "Critical audit WIRED; forensic snapshot + pager UNWIRED (no code -- operator-procedure only / deferred)."

DF-5 (PRRO_FISCAL_MODE not enforced): the RUNBOOK preflight + PLAYBOOK exit gate present "PRRO_FISCAL_MODE=TEST" as an enforced test/prod guard. The W4-Z3 harness does NOT define or check this env var -- it gates only on PRRO_LIVE_DPS=1 + the host allowlist (the local DB is seeded test-mode internally). DOWNGRADE to "manual operator preflight (NOT harness-enforced)" and flag "a hard harness check for PRRO_FISCAL_MODE=TEST is a required pilot fix (deferred to the W4-Z3 branch)".

DF-6 (INV-09/10 offline-limit framing): PLAYBOOK §2.1 + MATRIX §5 say INV-09 36h is "accepted for pilot scope". Code has the storage fields but NO production enforcement gate. REWRITE to: "risk-acceptable ONLY with explicit bd pilot sign-off AND offline disabled / operationally controlled -- no production 36h-freeze (INV-09) or 168h-cap (INV-10) enforcement exists."

DF-7 (secrets hygiene): RUNBOOK -- the primary command example shows inline PRRO_LIVE_DPS_JKS_PASS=.... Replace with read -rs into an exported var + a trap to unset it; remove copy-pastable inline-secret examples (the code itself never logs the pass; this is about not training a risky operator habit).

DF-8 (record the PILOT NO-GO verdict + Hard-Blocker list): the gate's current honest verdict is PILOT NO-GO. Record it prominently (MATRIX §5 exit gate + PLAYBOOK exit criteria; reference from MAP). Hard blockers: (1) shift lifecycle NON-FUNCTIONAL on HEAD -- static seeded shift_state, no online open/close drivers, offline Pattern C safety silently absent (DF-1); (2) W4-Z3 native ATTACHED crypto unmerged + not externally reviewed -- HEAD signer detached/not-live-accepted (DF-2/DF-3); (3) PRRO_FISCAL_MODE not harness-enforced (DF-5); (4) INV-05/06 channel guards UNWIRED (risk-accept only with ops freeze); (5) INV-09/10 offline limits UNWIRED (risk-accept only with offline descoped/controlled). Path to GO: WL-1 full shift lifecycle (incl. offline current_shift_id, NOT online-only) OR explicit offline descope + WL-3 MAC internal-advance + W4-Z3 merge & external review.
`

phase('Fix3')

const DOCS = [
  {
    key: 'MAP',
    label: 'fix3:MAP',
    file: ARCH + '/ALGORITHMIC_MAP.md',
    pointers: 'Apply: DF-1 (re-tag drain edges in §1.3 shift table + §1.9 + §1.11 WIRED-baseline -- use the CORRECTED "non-functional, not a crash" framing); DF-2 (stale CryptBadSign row in §1.11 gap table); DF-3 (crypto §1.8 HEAD-detached vs branch-attached); DF-4 (§1.10 audit table manual pager/snapshot split); DF-8 (add/point to the PILOT NO-GO + hard-blocker note, e.g. in §1.11).',
  },
  {
    key: 'MATRIX',
    label: 'fix3:MATRIX',
    file: ARCH + '/PILOT_TEST_MATRIX.md',
    pointers: 'Apply: DF-1 (re-tag drain edges in §3.6 + §3.10 -- CORRECTED framing); DF-3 (§3.8 crypto HEAD-detached vs branch-attached); DF-4 (§3.11 manual pager/snapshot split); DF-5 (§5 / static gate: PRRO_FISCAL_MODE not harness-enforced); DF-6 (§5 INV-09/10 framing); DF-8 (§5 exit gate: record PILOT NO-GO + the hard-blocker list prominently).',
  },
  {
    key: 'PLAYBOOK',
    label: 'fix3:PLAYBOOK',
    file: ARCH + '/PILOT_REVIEW_PLAYBOOK.md',
    pointers: 'Apply: DF-1 (re-tag drain edges in §3.1 / §4 ledger -- CORRECTED framing); DF-5 (exit gate: PRRO_FISCAL_MODE not harness-enforced); DF-6 (§2.1 INV-09/10 framing); DF-8 (exit criteria: record PILOT NO-GO + hard blockers).',
  },
  {
    key: 'RUNBOOK',
    label: 'fix3:RUNBOOK',
    file: OPS + '/LIVE_DPS_SMOKE_RUNBOOK.md',
    pointers: 'Apply: DF-5 (preflight: PRRO_FISCAL_MODE documented but NOT harness-enforced -> downgrade to manual operator preflight + flag the required hard-check fix); DF-7 (secrets hygiene: replace inline PRRO_LIVE_DPS_JKS_PASS=... with read -rs + export + trap). NOTE: this runbook is otherwise sound; do not over-edit. Add a one-line pointer that the pilot gate verdict is NO-GO (this smoke is a branch/technical smoke, not a pilot authorization).',
  },
]

const results = await parallel(
  DOCS.map((d) => () =>
    agent('Apply the external-review fixes to ' + d.file + '. Read the file, then apply ONLY the corrections below via Edit (preserve everything else; keep the doc internally consistent). ' + d.pointers + '\n' + DF + '\nReturn a short confirmation listing which DFs you applied + any you could NOT locate.', { label: d.label, phase: 'Fix3' }).then((r) => ({ key: d.key, confirmation: r }))
  )
)

return results.filter(Boolean)
