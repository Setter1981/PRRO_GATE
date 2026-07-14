# PILOT-GATE CHECKLIST — single source of truth

**Assembled 2026-07-14.** Consolidates the canonical gate docs
(`architecture/PILOT_TEST_MATRIX.md` §5, `architecture/PILOT_REVIEW_PLAYBOOK.md`
exit-criteria, `architecture/W4_Z4_PILOT_READINESS_STABILIZATION.md`,
`PILOT_HARDENING_TRIAGE.md`, `PILOT_ACCEPTANCE_TEST_PLAN.md`,
`operations/LIVE_DPS_SMOKE_RUNBOOK.md` §4.9) into one status list, reconciled
against verified reality on `main` HEAD (post `#284`).

> **⚠️ Doc-hygiene finding.** The **MATRIX and PLAYBOOK bodies are STALE** — they
> still describe DF-1 / DF-2 / DF-3 / WL-1 as OPEN Hard-Blockers, but those are
> CLOSED and **live-proven** against the real DPS test cabinet (2026-07-14). Only
> the runbook §4.9 was refreshed (`#284`). **The MATRIX/PLAYBOOK need the same
> refresh** — until then this checklist overrides their bodies.

> **Gate verdict.** The canonical GO/NO-GO verdict lives in the MATRIX/PLAYBOOK,
> not here — this checklist does not flip it. Factually: the **core fiscal cycle
> (online AND offline) is done and live-proven**; the residual is a **bounded list**
> below. The hard 80% (correctness-vs-reality) is behind us.

### Legend
`✅L` closed + **live-proven** vs real DPS · `✅` closed/merged (code) · `🟡` in progress ·
`🔴` open · `🧪` verification-pending (code exists, not yet proven) · `⏸` post-pilot / risk-accept
Owner: **A**=architect (design/review/wiring-plan) · **I**=implementer (code) · **O**=operator (ops/legal/procedure)

---

## 1 · CLOSED + LIVE-PROVEN — the moat (gate-satisfied)

| ID | Item | Cat | St | Evidence |
|---|---|---|---|---|
| DF-1-online / WL-1 | Online shift lifecycle SHIFT_OPEN→SELL→Z transacts | fiscal | ✅L | live 2026-07-14 sfn `5csPvKsz96s`/`UywxzaNUo_0`/`uIoFs7NDBPQ`; A′.1 #221-225 |
| DF-2 / DF-3 | Native ATTACHED CAdES-BES signer, live-accepted (no -1/-14) | crypto | ✅L | #112; `-8` date fix #262 (+teeth #280) |
| DF-1-offline | Offline cycle: T=112 codes / `<MAC ID>` drain / DocType 9-10 | offline | ✅L | live smoke 8+9 `DyJVrXyeaSg` / `<MAC ID='omyOfQ-gRs0'>`; B8 #248, B9 #249, B10 #252 |
| ACCEPT-Ph6 | Offline lifecycle drains to real-DPS ACK | offline | ✅L | live smoke 9: 3 backlog docs→ACK, failures=0 |
| WebCheck-replay | replay+diff 13/13 PASS (fiscal semantic equivalence) | test | ✅ | U3 corpus; teeth-proven |
| Fuzzer-CI | invariant-fuzzer REQUIRED gate in CI at meaningful N | test | ✅ | TRIAGE-P1 DONE 2026-07-12; #253 single-workflow |
| Fiscal surface | RETURN+ / Z / X-report / service-io / EPZ / cash-ledger / L5 guards | fiscal | ✅ | #233-234, L6 #264, L3 #258, EPZ, L0/L1, L5 (50k/zero/underpay) |
| Crypto-immut / DER-SET / eContent | no-reformat-after-sign, DER SET sort, byte-identical eContent | crypto | ✅ | wired + regression-pinned (MATRIX §3.8) |
| WIRED-suites | concurrency/lease, stress, migration, crash-injection, date/crypto | test | ✅ | MATRIX §3.3-3.8 green |
| Host-allowlist | prod DPS endpoint default-deny allowlist | sec | ✅ | regression-pinned (RUNBOOK §4.2) |
| Secrets-hygiene | JKS pass / key / decrypted bytes never logged | sec | ✅ | harness `--never logged` (RUNBOOK §4.4) |
| Emergency-stop | off-switch documented | ops | ✅ | RUNBOOK §4.8b |

---

## 2 · CLOSED (code merged) — but MATRIX/PLAYBOOK doc-STALE (refresh needed)

| ID | Item | Cat | St | Evidence / note |
|---|---|---|---|---|
| Offline-reachability | mode-setter + session-open + drain loop reachable in prod | offline | ✅ | A′.3 #245; W10a/b T2 #255. **MATRIX/PLAYBOOK still say UNREACHABLE — stale** |
| Edge-2 | Offline SHIFT_OPEN → `OpenedLocalPendingDrain` | offline | ✅ | A′.3 #245 (PR-O3 edge-2) |
| Drain-escalate | drain-reject `OFFLINE_LOCAL_ACK` → RMR reachable | recovery | ✅ | reachable once offline path wired (A′.3); live smoke-9 exercised drain |
| INV-09 / INV-10 / 24h | 36h session + 168h month + 24h shift budgets ENFORCED + auto-Z | legal | ✅ | T3 #256 `time_budget::compute_budgets_for_fn` → enforce `inline.rs:776`, config-toggle. **Doc "UNWIRED" is stale.** Live 24h-auto-Z verify → §5 |
| Edges 4/12 → RMR | ambiguous online SHIFT_OPEN / Z timeout → Manual | recovery | ✅ | A′.2 #232/#235 (`run_staged` 4/12→RMR). **Covers WL-1b/ReconFamily2 — verify depth** |
| Seed-fork | advance-at-SEND (chain seed advances at Sending→Sent CAS) | fiscal | ✅ | A.3 #227-231 `80f8ced` |
| T2-reserve | offline code reserve floor (last code for Z close) | legal | ✅ | #255; live-verify with drain |
| Mutation-ratchet | per-PR mutation diff-gate + teeth discipline in CI/CLAUDE.md | test | ✅ | FW-1 #282 |

---

## 3 · REMAINING GATE-BLOCKERS — correctness / legal / recovery (the real work)

| ID | Item | Cat | St | Owner | To-close |
|---|---|---|---|---|---|
| **B11 / AutoOffline** | automatic GO-OFFLINE trigger (entry is manual-CLI only) | offline | 🟡 | A→I | design-frame LOCKED ([[project_b11_offline_transition_design_frame]]); synthesis running → impl |
| **INV-05 / INV-06** | channel-switch-with-open-shift + failover-outside-shift guards | fiscal | 🔴 | A→I | wire guard (frozen inv #3); or `bd` risk-accept + ops freeze |
| **DF-5** | `PRRO_FISCAL_MODE` hard harness-enforce (not manual preflight) | ops/legal | 🔴 | I | add fail-closed `PRRO_FISCAL_MODE=TEST` check |
| **Cert-expiry gate** | 36h cert-window SHIFT_OPEN block (spec §16.10) | legal | 🔴 | I | `NotAfter - now < 2160min` refuse |
| **DF-4** | forensic-snapshot + operator pager on RMR landing | recovery | 🔴 | A→I | snapshot-capture + alert (critical audit emits today) |
| **Byzantine-decode** | fail-closed hardening on garbage/partial DPS response | recovery | 🔴 | A→I | TRIAGE-P6; feeds B11 forward-progress signal |
| **CodePool-STOP** | `CodePoolExhausted → STOP_MODE` prod caller-routing | offline | 🔴 | I | wire caller (typed error exists) |
| INV-04 | active-shift partial-UNIQUE index (only non-unique exists) | fiscal | 🔴 | I | migration (defense-in-depth) |
| FN-deregistered | FN-deregistered-while-offline drain-halt classifier | recovery | 🔴 | A→I | classifier subtype |
| ForceSeam-CLI | force/senior reconciliation operator-reachable (admin CLI) | recovery | 🔴 | I | admin entrypoint (test-only today) or ⏸ |

---

## 4 · VERIFICATION-PENDING — code exists, not yet proven

| ID | Item | Cat | St | To-close |
|---|---|---|---|---|
| **24h-auto-Z live** | T3 auto-Z closes over-limit shift on real DPS (task #19) | legal | 🧪 | live smoke |
| Static-gate | `cargo fmt --check` + scoped `clippy -D warnings` fresh pass | test | 🧪 | run (no reason to believe broken) |
| ACCEPT-Ph7 | restart/recovery after PREPARED/SIGNED/SENT + DPS-timeout | recovery | 🧪 | full scenario pass |
| ACCEPT-Ph8 | two fiscal-number isolation | fiscal | 🧪 | scenario pass |
| TRIAGE-P3 | mutation-density snapshot on shipped surface (EPZ/L5/shift/offline) | test | 🧪 | run FW-1 full on surface |
| TRIAGE-P0 | QUALITY_CHARTER §8 → CLAUDE.md + CI rules (beyond mutation ratchet) | ops | 🧪 | wire remaining charter rules |

---

## 5 · OPERATIONAL MUST-HAVES (pilot-adjacent, not core-correctness)

| ID | Item | Cat | St | Owner |
|---|---|---|---|---|
| Monitoring | remote monitoring, off-by-default | ops | 🔴 | I |
| Windows-install | installer acceptance: service/autostart/upgrade/uninstall (ACCEPT-Ph10) | ops | 🔴 | I |
| Printing | receipt printing (Windows) | ops | 🔴 | I |
| Env-segregation | demo/prod separate DBs + test-marker on demo checks | ops | 🔴 | I/O |
| Operator-recovery | `prro doctor --repair` runtime invariant net (TRIAGE-P5) | ops | 🔴 | A→I |
| Rollback-rehearsal | rollback-to-WebCheck documented + rehearsed (ACCEPT-Ph9) | ops | 🔴 | O |
| ACCEPT-Ph9 | health/readiness/metrics/backup/key-replacement ops docs | ops | 🔴 | I/O |
| Maria-STOP-R1 | `return_check_number` sent on every return → 422 (if Maria ingress) | compat | 🟡 | I |

---

## 6 · POST-PILOT / explicit risk-accept

| ID | Item | Note |
|---|---|---|
| JKS-rotate | rotate COMPROMISED live JKS key + JKS-pass plaintext→KMS | ⏸ security finding; key exposed in chat |
| Reference-gaps | STORNO (ORDERSTORNUM) / CAdES-T-on-offline / part-pay (PaymentOrder 5/6/7) | ⏸ verify-vs-our-code first ([[reference_official_dps_prro_source]]) |
| UI / Licensing / TSP | visual config, licensing (ed25519), RFC-3161 timestamp | ⏸ backlog |

---

## Critical path to GO (recommended sequence)

1. **B11** — finish design (synthesis) → implement auto GO-OFFLINE + forward-progress return + fleet-command. *(closes the flagship offline residual; pulls in Byzantine-decode + CodePool-STOP naturally)*
2. **Small wirings** — INV-05/06 channel guards · DF-5 fiscal-mode harness · Cert-expiry gate.
3. **Live verification** — 24h-auto-Z smoke · ACCEPT-Ph7/Ph8 · static gate.
4. **Recovery finish** — DF-4 snapshot+pager · FN-deregistered classifier.
5. **Operational must-haves** — monitoring · Windows installer · env-segregation · printing · operator-recovery · rollback rehearsal.
6. **Doc refresh** — bring MATRIX/PLAYBOOK bodies in line with reality (as `#284` did the runbook).
7. **Final gate re-assessment** — flip verdict in MATRIX/PLAYBOOK once 1-5 clear.

## Evidence backbone (what proves the moat)
live-DPS full-cycle proof (online+offline, 2026-07-14) · WebCheck replay+diff 13/13 · invariant-fuzzer (required CI) · scenario-harness S1-S13 · mutation-testing FW-1 (ratchet) · teeth-canary discipline.
