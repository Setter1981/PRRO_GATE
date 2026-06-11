# M3a CLOSURE LADDER — REVIEW DOSSIER

**Prepared:** 2026-05-14
**Target baseline:** `rust-gateway` HEAD `89e27cc`

---

## Snapshot

| Item | Value |
|---|---|
| Branch | `rust-gateway` |
| HEAD | **`89e27cc`** (2026-05-14 10:34 UTC) |
| Test surface | **470 passed / 0 failed / 1 ignored** (only 1 illustrative markdown doctest) |
| W11 deterministic-replay | 21/21 fixtures green |
| Workspace state | Clean — все follow-up worktrees + local branches удалены, remote branches preserved |

## Что было замержено сегодня (3 follow-up PR)

| PR | Commit | Type | Files | LOC | Test delta |
|---|---|---|---|---|---|
| **#43** | `938e560` | code + tests | `stage_send.rs` docstring + new `tests/boot_phase_raw_cas_edges_are_whitelisted.rs` | +254 / −6 | +3 |
| **#44** | `ed2d52a` | docs only | `docs/M3a-handoff.md §6.3` (Pilot gates) | +28 / −1 | 0 |
| **#45** | `89e27cc` | code + tests | `boot_phase.rs::passive_hold_kvt1` + 4 new tests in `boot_phase_w9_helpers.rs` | +228 / −7 | +4 |

**Total scope**: 2 production files touched (`stage_send.rs` docstring-only, `boot_phase.rs` ~84 lines + new helper), 2 test files (+254/+144), 1 doc file.

## Что закрыто этими PR (review findings из предыдущего senior-architect ревью на baseline `5e1c560`)

- **HIGH-1**: `stage_send.rs:33-40` docstring был stale ("Until W11+ tests + ops scripts MUST manually filter") — заменён на корректное описание (boot dispatcher mitigates via `dispatch_error_retryable_by_class` + `attempts_used` budget cap; caller obligation explicit).
- **HIGH-2**: `boot_phase.rs` 7 raw-CAS sites обходили `allowed_transition` whitelist гейт — добавлен `cargo test`-time scanner который ловит whitelist alignment drift (3 теста: main + 2 scanner self-checks).
- **LOW-Kvt1**: `BOOT_KVT1_HOLD_DEFERRED` всегда `Severity::Info` — stuck doc неотличим от fresh hold; добавлена age-based escalation (Info < 1h ≤ Warning < 24h ≤ Error) + `first_kvt1_at` + `kvt1_age_seconds` в payload.
- **Pilot gates**: 2 P1 carry-forward из M2 (`PRRO_GATE-k54` TLS CA bundle, `PRRO_GATE-0ps` proto drift) повышены до pilot-gating в `docs/M3a-handoff.md §6.3`.

## Hot zones ревьюер должен проверить на новом baseline

| Зона | Что нового | Anchor |
|---|---|---|
| `services/write_path/stage_send.rs` | docstring `run` header — должен корректно описывать дисплейерный layer | `stage_send.rs:33-65` |
| `services/reconciliation/boot_phase.rs::passive_hold_kvt1` | SELECT расширен на `(state, updated_at)`; новая free fn `age_and_severity_for_kvt1`; парсер fallback degrade-and-emit | `boot_phase.rs:729-866` |
| `tests/boot_phase_raw_cas_edges_are_whitelisted.rs` | новый сканер — проверяет ровно 7 raw CAS sites; `EXPECTED_RAW_CAS_COUNT=7` локирован | `tests/boot_phase_raw_cas_edges_are_whitelisted.rs` |
| `tests/boot_phase_w9_helpers.rs` | 4 новых теста + 2 helper'а (`drop_fd_updated_at_trigger`, `read_latest_kvt1_hold_audit`) | `tests/boot_phase_w9_helpers.rs:330-510` |
| `docs/M3a-handoff.md §6.3` | три subsection (6.3.1 ONLINE smoke, 6.3.2 k54, 6.3.3 0ps) с discharge condition triples | `docs/M3a-handoff.md` |

## Invariant check на новом baseline (frozen 1–10)

Все 10 invariants preserved; #8 (recovery preserves state-machine correctness) **structurally strengthened**:
- whitelist drift в `boot_phase` raw CAS теперь fail at `cargo test` time (не at production boot),
- stuck Kvt1 doc теперь даёт escalating-severity audit signal (не uniform Info stream).

## Carry-forward residuals (M3b agenda — НЕ flag заново)

Ревьюер этих не должен re-litigate:

1. **`boot_phase` raw-CAS структурная промоция** — 7 sites всё ещё raw SQL. PR #43 добавил `cargo test`-time guard, не сам helper. Промоция к `transition_state_tx_with_audit` (audit-payload closure) = M3b.
2. **`first_kvt1_at` dedicated column** — PR #45 использует `updated_at` как proxy. Точное приближение сегодня (trigger semantics), но schema change в M3b будет cleaner.
3. **HP2 mutex bypass** — direct `boot_phase::run_boot_reconciliation` обходит App mutex. Acceptable для M3a single-worker pilot; module-level enforcement = M3b.
4. **Canonical hash recompute** на PREPARED replay — нет canonicalization helper'а; deferred to M4 ingress adapter wiring.

## Pilot gates (pre-pilot review, не M3b-gating)

1. **6.3.1 ADR-D3 ONLINE smoke** — Sprint-7 evidence accepted OR fresh Rust-stack cycle OR explicit waiver.
2. **6.3.2 PRRO_GATE-k54 TLS CA bundle** — implement `tls_root_certs` end-to-end OR waiver transferring trust-store rot risk to ops.
3. **6.3.3 PRRO_GATE-0ps proto drift** — recorded decision на pilot method versions + apiver + delLast/lastChk OR waiver narrowing pilot doc types.

## Suggested review scope для свежей сессии

Если запускать новое senior-architect read-only ревью:

> Ты senior Rust architect + security/reliability reviewer на проекте Multi-Protocol PRRO Gateway (`/mnt/d/PRRO_GATE`, branch `rust-gateway`, HEAD `89e27cc`). M3a CLOSED + 3 follow-up PR замержены (#43 stage_send docstring + raw-CAS regression test, #44 pilot gates §6.3, #45 Kvt1 age escalation). Проведи read-only ревью с фокусом на: (a) корректность age-bucket бизнес-логики в `boot_phase::passive_hold_kvt1` + полнота test coverage; (b) полнота `tests/boot_phase_raw_cas_edges_are_whitelisted.rs` scanner (пропускает ли он one-line shapes? multi-line edge cases?); (c) ясность discharge conditions в §6.3 pilot gates; (d) есть ли HIGH/MED findings которые остались open и должны быть закрыты ДО pilot. НЕ flag known carry-forwards: raw-CAS промоция, first_kvt1_at column, HP2 mutex bypass, canonical hash recompute — они уже в M3b agenda. Output: HIGH / MED / LOW таблица + verdict (GO / NO-GO / GO-WITH-CONDITIONS for pilot).

## Suggested next step

- Запустить новый read-only review по prompt выше (`Agent` с `subagent_type=security-reviewer` или прямой главной сессии — на выбор).
- Параллельно: открыть M3b implementation plan opening (carry-forwards = повестка).
