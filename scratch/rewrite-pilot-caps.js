export const meta = {
  name: 'rewrite-pilot-caps',
  description: 'Rewrite the W4-Z4 pilot caps (MAP/MATRIX/PLAYBOOK) to real code vocab + WIRED/UNWIRED honesty + INV refs, and CREATE the runbook',
  phases: [{ title: 'Write', detail: '4 agents write the 4 pilot gate docs grounded in the STABILIZATION spec + real-code facts' }],
}

const ARCH = '/mnt/d/PRRO_GATE/docs/architecture'
const OPS = '/mnt/d/PRRO_GATE/docs/operations'
const SPEC = ARCH + '/W4_Z4_PILOT_READINESS_STABILIZATION.md'
const WT = '/mnt/d/prro_gate_m4_w4_z3'

const FACTS = `
GROUND-TRUTH FACTS (use verbatim; existing drafts used INVENTED vocab — replace it):

REAL ENUM VOCABULARY (rust/prro/src/db/models/enums.rs):
- DocState: PREPARED, SIGNED, ENCRYPTED, SENDING, SENT, KVT1, KVT2, ACK, OFFLINE_LOCAL_ACK, REJECTED, CANCELLED, ERROR_RETRYABLE, REQUIRES_MANUAL_RECONCILIATION. Terminals: ACK / REJECTED / ERROR_*. (NO "COMPLETE/REQ_RCVD/INPUTS_PINNED/ENVELOPED/TRANSMITTING" — fabricated.)
- ShiftState (9): CREATED, OPENING, OPENED_LOCAL_PENDING_DRAIN, OPENED, CLOSING_LOCAL_PENDING_DRAIN, CLOSING, CLOSED, REQUIRES_MANUAL_RECONCILIATION, ERROR.
- OfflineSessionState: OPENING, OPEN, DRAINING, CLOSED, ABORTED. (NB live DB column = "status", stale CHECK has CLOSING not DRAINING — note drift.)
- NodeMode: ONLINE, GOING_OFFLINE, OFFLINE, GOING_ONLINE, BLOCKED, STOP_MODE, CRYPTO_DEGRADED.
- InboxStatus: NEW, PROCESSING, DONE, REJECTED, ERROR.
- RetryClass: TerminalReject, TransientRetry, FnConfigError, WrapperBug, ProbeRequired, MacRecovery, OperatorEscalation. ErRedriveDecision: Redrive, BudgetExhausted, EscalateManual, EscalateInconsistent, HoldProbeRequired, HoldIndeterminate.
- Operator recovery taxonomy (spec §16.3, NOT Rust ids): AutoOfflineFallback, TechSupportEscalation, KeyRotationPending, MacReseedRecovery, TechSupportRepair.
- Write-path stages: stage_acquire, stage_sign, stage_send, stage_finalize, stage_offline_ack (+ dispatch, signer_guard, mac_recovery, error_routing).

CRYPTO (fix the mislabel "encrypt outbound with DSTU 4145"):
- OUTBOUND to DPS = CMS-detached SIGNED over CP1251 canonical XML (crypto/provider.rs:49). DSTU 4145-2002 SIGNATURE (PB-257) + GOST 34.311 / DSTU 7564 (Kupyna) HASH. DSTU 4145 is SIGNATURE, NOT encryption.
- INBOUND (KVT2) = unwrap_envelope DECRYPT of DPS EnvelopedData (provider.rs:71). Encryption is INBOUND only.
- W4-Z3 live cycle was signed-only (ATTACHED CMS SignedData, sendChkV2 accepted). Proven ФСКО path PREPARED→SIGNED→SENT.

IDEMPOTENCY (fix the "7 keys" invention): ONE column ingress_inbox.idempotency_key, UNIQUE (fiscal_number, idempotency_key) (sql/001:97). Canonical hash is NOT idempotency (runtime/ingress/dto.rs:304-317). Separate "DPS idempotency surface" = server-side local_number / server_fiscal_no.

SHIFT GUARD (WIRED, read-only): check_shift_guard (stage_acquire.rs:845) = 162-cell matrix, oracle test check_shift_guard_matches_oracle_for_all_162_cells. (ShiftOpen,Closed)→allow; (ShiftOpen,*active*)→ShiftAlreadyOpen; (Sell,Closed)→ShiftNotOpen (CORRECT, INV-03); (ZReport,OpenedLocalPendingDrain)→ZReportBlockedBacklogDrainPending (INV-15); (Sell,OpenedLocalPendingDrain,Online)→ShiftOpenPendingDrainOpRefused. NodeMode pre-guards: GoingOnline/Blocked/StopMode/CryptoDegraded refused.

14 shift edges (shifts.rs:67): WIRED via drain/boot = 1,2,5,6,7,9,13,14. UNWIRED (whitelist but NO prod caller) = 3(Opening→Opened),4,8(Opened→Closing),10(Closing→Closed),11,12.

CRITICAL WIRED vs UNWIRED HONESTY (the key correction — mark every gate; do NOT assume Python-era parity):
- WIRED (regression-pin tests exist): 162-cell shift guard; Pattern C OFFLINE_LOCAL_ACK (stage_offline_ack.rs:165, OFFLINE_LOCAL_ACK_APPLIED); code-pool exhaustion → CodePoolExhausted/STOP_MODE (offline_sessions.rs:380); drain-reject of OFFLINE_LOCAL_ACK on pending-drain → RequiresManualReconciliation + OFFLINE_DRAIN_HALTED_ESCALATE_MANUAL Critical (backlog_drain.rs:2147); edge-5 drain-finalize opens shift; force/senior seams (shifts.rs:575/840); native crypto + full live WIRE cycle SHIFT_OPEN→SELL→Z (W4-Z3, server_fiscal_no 1g41M3jDt-Q / AOBSkplfIUU / L2AMnY2MkmA, 2026-05-29).
- UNWIRED (gap-marker/xfail; NO prod driver today): online shift lifecycle drivers (edges 3/4/8/10/11/12 — online SHIFT_OPEN→Opened / Z→Closed NOT driven; W4-Z3 confirmed node_state.shift_state never opens online); active-shift partial-UNIQUE index (only Python sql/001:158 old 3-state; Rust only non-unique ix_shifts_fn_state; INV-04 9-state index aspirational); INV-09 36h continuous-offline ingress freeze (no offline_session_started_at/OFFLINE_LIMIT_EXCEEDED_INGRESS_REFUSED); INV-10 168h monthly cap (column current_month_offline_seconds exists, no enforcement reader); WebCheck 36h cert-expiry SHIFT_OPEN gate (spec §16.10); INV-05 channel-switch-with-open-shift guard (frozen invariant #3 — NOT enforced in Rust); INV-06 failover-outside-shift (explicit GAP, CHANNEL-FAILOVER-01); ambiguous online SHIFT_OPEN/Z timeout→manual (edges 4/12 unreachable, shift_open_recovery.rs "proposed"/absent); FN-deregistered-while-offline classifier.

INVARIANTS (docs/LEGAL_INVARIANTS.md INV-01..20): 01 single-writer/FN; 02 LND monotonic no-gaps (rollback=VOID never reuse); 03 shift open before fiscal ops; 04 no two active shifts/FN; 05 no channel switch with open shift; 06 failover only outside shift (GAP); 07 idempotency mandatory; 08 offline only on DPS-unreachable (auto = stub); 09 ≤36h continuous; 10 ≤168h/month; 11 offline needs pre-issued range; 12 one offline-no = one doc; 13 offline doc provisional until DPS Ack (sign at DRAIN); 14 OFFLINE_LOCAL_ACK retained until Ack; 15 online Z blocked w/ pending backlog; 16 excise needs UKTZED+mark; 17-20 read the file for exact text.

REAL STATIC-GATE COMMANDS (Rust-only; drop --all-features): cargo fmt --check; cargo clippy -p prro --features test-support --tests -- -D warnings; cargo clippy -p prro_crypto --all-targets -- -D warnings; cargo build -p prro --tests --features test-support; cargo test -p prro --features test-support; live-DPS COMPILE-ONLY (NOT in CI): cargo test -p prro --features live-dps --test live_dps_extended_smoke --no-run. STACK = RUST-ONLY (Python src/prro_gateway = dead reference; no pytest/ruff).
`

phase('Write')

const RUN_CMD = 'PRRO_LIVE_DPS=1 PRRO_LIVE_DPS_JKS_PATH=... PRRO_LIVE_DPS_JKS_PASS=... cargo test -p prro --features live-dps --test live_dps_extended_smoke <name> -- --ignored --nocapture'

const DOCS = [
  {
    key: 'ALGORITHMIC_MAP',
    label: 'write:MAP',
    q: 'Rewrite ' + ARCH + '/ALGORITHMIC_MAP.md as a pilot GATE document. FIRST read the existing draft ' + ARCH + '/ALGORITHMIC_MAP.md (preserve its §3 Tax Mapping Invariants + the "Crypto Immutable Rule" VERBATIM) and read the authoritative template ' + SPEC + ' §1.1-§1.11 (mirror its structure). Then WRITE the rewritten file.\n'
      + 'Realize (STABILIZATION §1): §1.1 online/offline/rejoin branches; §1.2 end-to-end fiscal flow using REAL write-path stages (stage_acquire→stage_sign→stage_send→stage_finalize, stage_offline_ack Pattern C) + REAL DocState transitions + an "Enforces (INV-NN)" column; §1.3 the 5 REAL state-machine tables (ingress_inbox.status / fiscal_documents.state=DocState / shifts.state=ShiftState 9-state with the 14-edge table marking WIRED vs UNWIRED / offline_sessions.status=OfflineSessionState / node_state.mode=NodeMode); §1.4 cross-machine invariants (INV-03 closed shift forbids signing; INV-13 offline ACK ≠ DPS accept; INV-15 online Z blocked w/ backlog; INV-05 channel pinned w/ open shift); §1.5 SQLite tx map (with_immediate = no network/crypto/fs IO — frozen #1); §1.6 external-IO; §1.7 time/date (UTC internal, Kyiv-local only at wire); §1.8 crypto/wire profile (FIX: outbound CMS SignedData DSTU4145-sig+Kupyna-hash NOT encryption; inbound KVT2 decrypt); §1.9 recovery algorithms; §1.10 audit/forensics; §1.11 known-deferrals + a prominent WIRED/UNWIRED gap table. CRITICAL: replace ALL invented states with real enum strings; replace the 7 invented idempotency keys with the single ingress_inbox.idempotency_key + DPS-surface note; LND-rollback semantics (INV-02 VOID-not-reuse; OFFLINE_LOCAL_ACK durable local-commit → drain-reject routes to REQUIRES_MANUAL_RECONCILIATION). Cross-link docs/architecture/2026-05-29-pilot-integration-map.md + the WL-1 shift-lifecycle plan.\n' + FACTS,
  },
  {
    key: 'PILOT_TEST_MATRIX',
    label: 'write:MATRIX',
    q: 'Rewrite ' + ARCH + '/PILOT_TEST_MATRIX.md as a pilot GATE document. FIRST read the existing draft (preserve concrete named test ideas) + ' + SPEC + ' §3.1-§3.9. Then WRITE the rewritten file.\n'
      + 'Realize (STABILIZATION §3): §3.1/§3.2 Static Gate with REAL per-crate/feature commands (FACTS — Rust-only, drop --all-features; include the live-dps --no-run COMPILE-ONLY gate that must NOT execute in CI); §3.3 concurrency/lease; §3.4 concurrency-stress acceptance (no corruption / no duplicate fiscal doc / no raw SQLITE_BUSY / no stuck intermediate); §3.5 migration verification; §3.6 Offline & Drain FSM gate; §3.7 rollback/crash-injection (fail during acquire/pin/post-XML/post-CMS/post-send-timeout); §3.8 date/crypto (Kyiv DST EEST→EET, 2049/2050 UTCTIME cliff, DER SET OF sorting, attached-CMS eContent, NO XML rebuild after sign); plus categories Shift Lifecycle / Manual-Recon / Channel-Lock.\n'
      + 'CRITICAL: split EVERY dynamic test into WIRED (regression-pin, exists/can pass now) vs UNWIRED (gap-marker, would FAIL / no driver — name them: test_online_shift_open_creates_shift_and_advances_to_opened, test_active_shift_unique_index_present_in_rust_schema, test_offline_duration_36h_limit_freezes_ingress, test_offline_monthly_168h_cap_blocks, test_shift_open_refused_cert_expiry_under_36h, test_channel_switch_refused_with_open_shift, test_failover_allowed_only_outside_shift, test_ambiguous_online_shift_open_timeout_routes_manual). Add an "Enforces (INV-NN)" column. Fix the dangling runbook reference to ' + OPS + '/LIVE_DPS_SMOKE_RUNBOOK.md.\n' + FACTS,
  },
  {
    key: 'PILOT_REVIEW_PLAYBOOK',
    label: 'write:PLAYBOOK',
    q: 'Rewrite ' + ARCH + '/PILOT_REVIEW_PLAYBOOK.md as a pilot GATE document. FIRST read the existing draft (preserve its with_immediate-no-IO / UTC-internal / no-reformat-after-sign rules VERBATIM) + ' + SPEC + ' §2.1-§2.8. Then WRITE the rewritten file.\n'
      + 'Realize (STABILIZATION §2): §2.1 5-level severity (Critical/High/Medium/Low/Info, fiscal examples); §2.2 explicit Pilot-Blocker (P0/P1) vs Non-Blocker (P3/P4) lists; §2.3 Sensitive-Data Hygiene (NEVER log JKS password / param_d private scalar / decrypted container bytes / decrypted XML; cert fingerprint/SKI/hashes OK; "no tracing::debug! on SignerKey/P12Pass/PrivateKey"); §2.4 SQLITE_BUSY review (no raw leak; PRAGMA busy_timeout / journal_mode=WAL / synchronous=NORMAL / BEGIN IMMEDIATE); §2.5 Review Rounds A-E (A fiscal/state-machine, B SQLite/concurrency/recovery, C crypto/date/ASN.1/XML, D DPS/live-ops/security, E tests/coverage); §2.6 chaos/fault-injection; §2.7 required-evidence template; §2.8 exit criteria. ADD review focuses: Shift Lifecycle Guards (162-cell WIRED; online drivers UNWIRED), Channel-Pinning (INV-05/06 frozen #3, UNWIRED → flag P1), Offline Limits & Drain (INV-09/10 UNWIRED), Manual-Recon (drain-reject WIRED; ambiguous-timeout UNWIRED). Add "Enforces (INV-NN)" tags. Tighten crypto wording (DSTU 4145 = signature, not encryption).\n' + FACTS,
  },
  {
    key: 'LIVE_DPS_SMOKE_RUNBOOK',
    label: 'write:RUNBOOK',
    q: 'CREATE ' + OPS + '/LIVE_DPS_SMOKE_RUNBOOK.md (does NOT exist yet). FIRST read ' + SPEC + ' §4.1-§4.8 AND the PROVEN live harness ' + WT + '/rust/prro/tests/live_dps_extended_smoke.rs. Then WRITE the runbook from the PROVEN procedure.\n'
      + 'Content (STABILIZATION §4 + the real harness): §4.1 purpose (operator-run live DPS smoke proving the native fiscal cycle); §4.2 the TRIPLE GATE (feature live-dps + #[ignore] + PRRO_LIVE_DPS=1 kill-switch) + test-host default-deny allowlist (cabinet.tax.gov.ua only; refuses prod prro/prro2/fs); §4.3 env contract table (PRRO_LIVE_DPS, PRRO_LIVE_DPS_HOST default https://cabinet.tax.gov.ua:9443, PRRO_LIVE_DPS_FN default 4000162280, PRRO_LIVE_DPS_JKS_PATH, PRRO_LIVE_DPS_JKS_PASS never-logged); §4.4 secrets hygiene (JKS pass via env only, never logged; key files gitignored); §4.5 the exact cargo invocations per smoke (1 connect; 2 lastChk; 3 MAC-seed; 4 extended-SELL offline-stub; 5a status-probe read-only; 5b SHIFT_OPEN; 6 extended SELL; 7 Z_REPORT) with the run command: ' + RUN_CMD + ' ; §4.6 the PROVEN full-cycle result (2026-05-29: SHIFT_OPEN server_fiscal_no=1g41M3jDt-Q, extended SELL=AOBSkplfIUU, Z_REPORT=L2AMnY2MkmA; MAC seeded from live lastChk per doc); §4.7 rate-limit guard (DPS status=-4 after too many errors → 5+ min per-FN cooldown; run sparsely, NEVER in CI, NEVER in a loop); §4.8 troubleshooting (DPS codes: -12 BAD_HASH_PREV, -14 NOT_REGISTERED_SIGNER=was the cert bug, -15 NOT_OPEN_SHIFT; the transient-Z-reject observation 1st REJECTED retry ACCEPTED). NOTE explicitly: this smoke proves the WIRE only (seed-PREPARED bypasses stage_acquire); the gateway online shift-lifecycle drivers are UNWIRED (WL-1).\n' + FACTS,
  },
]

const results = await parallel(
  DOCS.map((d) => () =>
    agent(d.q, { label: d.label, phase: 'Write' }).then((r) => ({ key: d.key, confirmation: r }))
  )
)

return results.filter(Boolean)
