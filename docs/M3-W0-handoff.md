# M3-W0 Handoff — exit gate before M3a implementation plan

**Status:** W0 research phase closed (commits `18e2247`, `f8ec32c`, `9455ed4`, `dec9f62` on `rust-gateway`).  3 spec docs landed (~3760 lines, ~120 path:line citations).  This handoff is the **gate document** — M3a implementation plan MUST NOT open until this handoff is approved.

**Sources cited (do not re-summarise here):**
- `docs/superpowers/specs/2026-05-06-m3-w0-1-state-sequence.md` (W0-1)
- `docs/superpowers/specs/2026-05-06-m3-w0-2-lock-discipline.md` (W0-2)
- `docs/superpowers/specs/2026-05-06-m3-w0-3-retry-recovery.md` (W0-3)

---

## 1. W0 findings → ADR amendments (A1–A9, all PROPOSED — NOT COMMITTED)

| ADR | From | Decision |
|-----|------|----------|
| **A1** | W0-1 §6.1 | lnd source-of-truth: `node_state.next_lnd` transactional sequencer + UNIQUE(fiscal_number, lnd) |
| **A2** | W0-1 §6.2 | CloseShift→ZReport mapping at Rust XML builder boundary (keep SHIFT_CLOSE internal label).  M3a binding: Z-allocation derives `wire_artifact_kind` first, NOT internal op label |
| **A3** | W0-2 §8.1 | `with_immediate` enforcement = hybrid (Send bound + W5-sibling static scan + `tokio::task_local!` IN_WITH_IMMEDIATE) |
| **A4** | W0-2 §8.2 | tx-witness sealed newtype `WriteTxConn<'_>` (module-private `fn new`, `_seal: ()` private field) + `transition_state` / `shifts::transition` signature change to `&mut WriteTxConn<'_>` |
| **A5** | W0-2 §8.3 | Boundary-pattern selection per stage: A (compute outside, persist inside) at stage 3 sign; B (mark intent, send, persist outcome) **mandatory** at stage 4 send (DPS does not deduplicate); C reserved for M3b |
| **A6** | W0-3 §8.1 | DpsError → retry policy table (8 variants × 12 Server-status sub-codes); WebCheck-derived inheritance set; `max_recovery_attempts=5` + capped exponential backoff |
| **A7** | W0-3 §8.2 | App::boot per-FN decision tree (6 branches a–f) + PRRO_GATE-ah8 acceptance test verbatim (no `shift_state` overwrite) |
| **A8** | W0-3 §8.3 | Pending-set documentation alignment (M2's 7 + M3a's SENDING = 8) + intentional whitelist gaps preserved |
| **A9** | W0-3 §8.4 | New `DocState::Sending` value + migration `008_doc_state_sending.sql` + whitelist additions (Signed→Sending, Encrypted→Sending, Sending→Sent/Kvt1/Rejected/ErrorRetryable, ErrorRetryable→Sending) + crash-resume rule (CAS Sending→ErrorRetryable, never auto-re-send) |

**Pre-requisite to A6** (called out as M2/W3 additive amendment in W0-3 §2 NB block): `DpsError::Authorization { code: i32, kind: AuthorizationKind, message: String }` + `enum AuthorizationKind { DocumentReject, FiscalNumberNotRegistered }`.  Without it, the -1 vs -13/-14 routing collapses to "single safe destination = RequiresManualReconciliation" with documented operational-load trade-off.

---

## 2. W0 findings → M3a implementation plan scope

- **Schema/migration** — `008_doc_state_sending.sql`; `transition_state` + `shifts::transition` signature breaks; UNIQUE(fiscal_number, lnd) index addition; `db::tx::WriteTxConn<'_>` introduction.
- **Write-path 5-stage pipeline** — acquire+validate / guard / sign / **send (Pattern B with 4-pre / 4a / 4b)** / finalize per W0-2 §2 stage table + W0-1 §3 sequence.
- **App::boot reconciliation phase** — added AFTER existing `app.rs:28` boot boundary; 6-branch decision tree per W0-3 §4.3 + PRAGMA quick_check fail-closed without DB writes.
- **DpsError routing** — 8-variant table-driven dispatch per W0-3 §2; 12 Server-status sub-codes; MAC-recovery for -12 per Python `write_path.py:903-994` parity.
- **Test acceptance gate** — 51 explicit fixtures per W0-2 §9 + W0-3 §9 (5 with_immediate guardrails, 5 trybuild compile-fail, 9 App::boot branches, 21 DpsError routing, 9 deterministic-replay, 3 boundary-pattern smoke).
- **Pre-requisite W3 amendment** — `DpsError::Authorization` field extension (additive; lands in M3a impl prep before the §3 SIGNED / SENDING recovery is exercised end-to-end).

---

## 3. Explicitly deferred to M3b

- Offline session lifecycle (open/drain/close OFFLINE_LOCAL_ACK pool) — W0-3 §5 trigger map carve-out.
- OFFLINE_LOCAL_ACK whitelist extension to 6 targets + retry self-loop per `services/offline_sync.py:10-14` — W0-1 §6.3 amendment, W0-3 §3 explicit M3b extension.
- `ix_offline_active` UNIQUE migration — W0-1 §6.3 (M3b blocker, M3a non-blocker because M3a never opens offline sessions).
- Pattern C "stage and flip" — W0-2 §5.3 reservation.
- Operator recovery UI / manual-reconciliation flows — out of M3a scope.
- M3b automated reconciliation worker (`last_chk` with cooldown / rate-limiting for SENDING-state docs) — W0-3 §6.3.
- Auto-flip OFFLINE→ONLINE via `ping(fn_sign)` confirmation — W0-3 §4.3 branch (d) option (iii).

---

## 4. bd issues open until implementation proof

All 5 entry-decision issues remain `open` per W0 exit criteria.  Each has a "research-addressed by spec" comment but bd-issue closure happens **only when M3a impl lands the contract in code** with the §9 test acceptance gates green.

| bd | Research-addressed by | Closure gate |
|----|-----------------------|--------------|
| PRRO_GATE-ddn | W0-1 §6.1 / ADR-M3-A1 | UNIQUE index migration + `next_lnd` sequencer in code; tests green |
| PRRO_GATE-zti | W0-1 §6.2 / ADR-M3-A2 | XML builder boundary mapping in code; M3a stage-1 test asserts `wire_artifact_kind == ZReport` for both internal labels |
| PRRO_GATE-k99 | W0-2 §8.2 / ADR-M3-A4 | `WriteTxConn<'_>` sealed newtype in code; trybuild §9.2 5 compile-fail fixtures green |
| PRRO_GATE-6bj | W0-3 §8.1 + §8.4 / ADR-M3-A6 + A9 | DpsError routing dispatch in code; §9.2 21 routing fixtures green; SENDING state + recovery rule in code |
| PRRO_GATE-ah8 | W0-3 §8.2 / ADR-M3-A7 | App::boot 6-branch decision tree in code; §9.1 9 branch fixtures green; verbatim `shift_state=Opened` no-overwrite assertion green |

Plus 2 cross-link issues observed during W0:
- PRRO_GATE-9qd (M3 epic) — closes when all 5 children close + M3 handoff document drafted.
- PRRO_GATE-iap (COM/1C compat) — pilot decision; ADR-M3-A2 preserves the constraint but doesn't close the issue.

---

## 5. M3a entry gate

**M3a implementation plan MUST NOT be drafted until this handoff is approved.**  Specifically:

1. **User approval of A1–A9 (this document § 1).**  ADR amendments are PROPOSED — NOT COMMITTED in W0 spec text; they become committed amendments to `docs/superpowers/specs/2026-05-04-m2-pre-plan-adr.md` only after explicit user GO.  Approval can be all-or-nothing or per-ADR; deferred ADRs stay in PROPOSED state and the corresponding M3a tasks fall out of scope.
2. **Decision on M2/W3 `DpsError::Authorization` amendment** (pre-requisite to A6).  Either approve the additive variant extension OR document the fallback ("single safe destination") in the M3a plan.
3. **Decision on §1 numbering scope** — confirm A1–A9 numbering is canonical at amendment commit time, OR re-number per M2 ADR convention.
4. **Confirmation that scope §3 ("explicitly deferred to M3b") is acceptable for M3a** — i.e. M3a ships ONLINE-only with no offline lifecycle, and that is the agreed first-phase exit.

After these 4 are confirmed, the M3a implementation plan is drafted (separate file under `docs/superpowers/plans/`) anchored to: (a) the approved ADR amendments, (b) the §9 test acceptance contracts (51 fixtures), (c) the 6-stage pipeline + App::boot reconciliation phase + Pattern B SENDING marker.

Without this handoff approval, opening the M3a plan is premature and risks re-litigating decisions W0 already closed.
