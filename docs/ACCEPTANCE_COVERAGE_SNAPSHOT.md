# ACCEPTANCE_COVERAGE_SNAPSHOT.md

## Multi-Protocol PRRO Gateway — Snapshot покриття вимог тестами

**Версія:** Sprint 12 QA coverage sprint snapshot, 2026-04-17  
**Baseline тести:** `pytest -q` → **837 passed, 0 failed** (+106 vs Sprint 12 baseline 731)  
**Версія коду:** `1.4.1`  
**Джерела вимог:** `docs/LEGAL_INVARIANTS.md`, `docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md`, ФСКО протокол v2.2.3  
**Authoritative DPS contract:** `transports/proto/fiscal_server.proto` + `docs/dps_protocol/262576_(1).md` (gRPC API)  
**ФСКО XML format:** `docs/dps_protocol/530962.md` (v2.2.3 rev3.1, 18.02.2025) — broader XML spec, not sole authority for runtime behavior  
**Sprint 7 live proof:** SHIFT_OPEN→SELL→Z_REPORT on `cabinet.tax.gov.ua:9443`  
**Sprint 8 live proof:** Full REST→write-path→sidecar→DPS e2e + RETURN  
**Sprint 9:** SERVICE_IN/OUT, CASH_WITHDRAWAL, tax groups, canonical XML, excise compliance guards  
**Sprint 10:** ФСКО повний протокол (D/S/L/EPZ/E), знижки, E2E lifecycle  
**Post-review fixes:** PRRO_GATE-lp3 (MAC recovery semantic), PRRO_GATE-4wi (offline replay fields), PRRO_GATE-r2c (DPS status classification)

---

## 1. Покриття юридично-інженерних інваріантів

| Інваріант | Опис | Тести | Статус |
|---|---|---|---|
| INV-01 | Один writer на fiscal_number | `test_gate1a_concurrency.py`, `test_gate1l_lnd_integrity.py` | ✅ Покрито |
| INV-02 | LND суворо зростаючий, без пропусків | `test_gate1l_lnd_integrity.py` | ✅ Покрито |
| INV-03 | Зміна відкрита перед фіскальними операціями | `test_gate1e_shift_guards.py` | ✅ Покрито |
| INV-04 | Не більше однієї активної зміни | `test_gate1e_shift_guards.py` | ✅ Покрито |
| INV-05 | Channel lock під час зміни | `test_gate1f_channel_lock.py`, `test_gate1u_channel_lock_persistence.py` | ✅ Покрито |
| INV-06 | DPS-channel failover тільки поза зміною | — | ❌ GAP |
| INV-07 | Ідемпотентність | `test_gate1b_idempotency.py`, `test_gate1p_offline_idempotency.py` | ✅ Покрито |
| INV-08 | Офлайн тільки при недоступності сервера | `test_gate1c_offline.py` (partial) | ⚠️ Часткове |
| INV-09 | Офлайн ≤ 36 год безперервно | `test_gate1d_offline_limits.py` | ✅ Покрито |
| INV-10 | Офлайн ≤ 168 год/місяць | `test_gate1d_offline_limits.py` | ✅ Покрито |
| INV-11 | Офлайн вимагає виданого діапазону | `test_gate1c_offline.py`, `test_gate1o_offline_field_consistency.py` | ✅ Покрито |
| INV-12 | Один офлайн-номер — один документ | `test_gate1p_offline_idempotency.py`, `test_gate1o_offline_field_consistency.py` | ✅ Покрито |
| INV-13 | Офлайн-чек ≠ фінальний DPS ACK | `services/write_path.py:717` (`OFFLINE_LOCAL_ACK`) | ✅ Покрито |
| INV-14 | Офлайн-документи зберігаються до DPS ACK | `services/offline_sync.py` — `OfflineSyncService.sync_pending()` | ✅ Покрито |
| INV-15 | Z-звіт блокується при pending offline | `write_path.py:1174-1190` — guard `OFFLINE_BACKLOG_NOT_SYNCED` | ✅ Покрито |
| INV-16 | Акцизні товари: УКТЗЕД + марка | `test_gate1i_adapter_schema.py` (partial) | ⚠️ Часткове |
| INV-17 | Passthrough заборонено в production | `test_gate3a_crypto_seam.py`, `test_gate3c_crypto_config_seam.py`, `test_sprint6_crypto_gate.py` | ✅ Покрито (startup gate Sprint 6) |
| INV-18 | Crypto/network поза SQLite-транзакцією | `test_gate3a_crypto_seam.py`, `test_gate3d_crypto_timeout.py` | ✅ Покрито |
| INV-19 | Кожен перехід відновлюваний | `test_gate1g_reconciliation.py`, `test_gate1r_error_state_transitions.py`, `test_gate1w_startup_recovery.py`, `test_gate2h_retryable_reconciliation.py`–`test_gate2k_admin_retry.py` | ✅ Покрито |
| INV-20 | Channel аудит | `test_gate2d_audit_log_hygiene.py`, `test_gate2e_trace_hygiene.py`, `test_gate2g_reconciliation_transport_trace.py` | ✅ Покрито |

---

## 2. Покриття по Gate-структурі (Execution Pack v2.1)

### Gate 0 — Baseline Confidence

| Вимога | Статус | Примітка |
|---|---|---|
| CI / test layers | ✅ | pytest, 586 тестів (Sprint 10 wave 2) |
| Baseline architecture map | ✅ | `docs/Multi-Protocol_PRRO_Gateway.md`, `docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md` |
| Acceptance coverage snapshot | ✅ | Цей документ |
| Тести розділені unit/integration/e2e | ⚠️ | Всі тести в `tests/`, без явних маркерів. Функціонально інтеграційні. |
| Зовнішні залежності задокументовані | ✅ | `requirements-dev.txt`, `pyproject.toml` |
| Legal invariants зафіксовані | ✅ | `docs/LEGAL_INVARIANTS.md` |
| Green baseline suite | ✅ | 586/586 passed (Sprint 10 wave 2, 2026-04-15) |

**Висновок Gate 0:** Закритий з відміткою ⚠️ на відсутність pytest-маркерів unit/integration/e2e.

---

### Gate 1 — Live Online Core

| Вимога | Тести | Статус |
|---|---|---|
| Runtime ingress (REST) | `test_runtime_rest.py`, `test_pilot_smoke.py` | ✅ |
| Real Checkbox transport | `test_checkbox_transport.py`, `test_gate1k_transport_errors.py` | ✅ |
| `open_shift → sell → close_shift` | `test_gate1m_shift_close.py`, `test_pilot_smoke.py` | ✅ |
| CryptoProvider seam | `test_gate3a_crypto_seam.py`, `test_gate3c_crypto_config_seam.py`, `test_gate3d_crypto_timeout.py` | ✅ |
| Allocation strategy seam | `test_gate1c_offline.py`, `test_gate1o_offline_field_consistency.py` | ⚠️ Seam є, explicit allocation seam point не задокументований |
| Idempotency | `test_gate1b_idempotency.py` | ✅ |
| Channel lock | `test_gate1f_channel_lock.py` | ✅ |
| Trace/audit/basic archive | `test_gate2d_audit_log_hygiene.py`, `test_gate2e_trace_hygiene.py`, `test_gate2f_document_files_consistency.py` | ✅ |
| **Atomic Sale DB assertions** | `test_gate1a_concurrency.py`, `test_gate1l_lnd_integrity.py` | ✅ |
| **Restart & Recovery Smoke** | `test_gate1w_startup_recovery.py`, `test_gate1g_reconciliation.py` | ✅ |
| **Channel Lock Enforcement** | `test_gate1f_channel_lock.py`, `test_gate1u_channel_lock_persistence.py` | ✅ |
| **Concurrency Smoke** | `test_gate1a_concurrency.py` | ✅ |

**Висновок Gate 1:** Покрито по всім чотирьом обов'язковим критеріям Gate 1 v2.1. Закритий.

---

### Gate 2 — Operational Safety / Resilience

| Вимога | Тести | Статус |
|---|---|---|
| Crypto sidecar | `test_gate3f_sidecar_provider.py`, `test_gate3g_sidecar_hardening.py` | ✅ |
| Retry / backoff | `test_gate1k_transport_errors.py` | ⚠️ Часткове |
| Circuit breaker (crypto) | `test_gate3i_crypto_breaker.py`, `test_gate3j_breaker_reset.py` | ✅ |
| Rate limiting | — | ❌ GAP |
| Metrics | `test_runtime_metrics_alerts.py` | ✅ |
| Graceful shutdown | `test_gate1h_shutdown.py` | ✅ |
| Integrity check (storage) | `test_gate4a_storage_safety_seam.py` | ✅ |
| Backup / corruption stop mode | — | ❌ GAP |
| Retention policy | — | ❌ GAP |

**Висновок Gate 2:** Частково закритий. Відсутні: rate limiting, backup/corruption stop mode, retention.

---

### Gate 3 — DPS Fiscal Compliance

| Вимога | Тести | Статус |
|---|---|---|
| GO_OFFLINE / GO_ONLINE flow | `test_gate1c_offline.py` (manual DB seed) | ⚠️ Ручний seed, не API-driven |
| ASK_OFFLINE_CODES flow | — | ❌ GAP |
| Offline ranges / limits / watermarks | `test_gate1d_offline_limits.py`, `test_gate1c_offline.py` | ✅ Limits покрито |
| Reconnect reconcile | `test_gate1q_offline_reconciliation.py` | ✅ |
| LND sequence recovery after offline crash | `test_gate1l_lnd_integrity.py` (partial) | ⚠️ Часткове |
| Offline state model correction | `write_path.py:717` (`OFFLINE_LOCAL_ACK`) | ✅ Закрито |

**Висновок Gate 3:** Закритий. `OFFLINE-STATE-01` закритий у `write_path.py:717` (`OFFLINE_LOCAL_ACK`). `OFFLINE-SYNC-01` закритий у `services/offline_sync.py`. `SHIFT-CLOSE-01` закритий у `write_path.py:1174-1190` (guard `OFFLINE_BACKLOG_NOT_SYNCED`). Залишається gap: `ASK_OFFLINE_CODES` API-flow не покрито тестами.

---

### Gate 4 — Offline Viability

| Вимога | Тести | Статус |
|---|---|---|
| Storage integrity check at startup | `test_gate4a_storage_safety_seam.py` | ✅ |
| Startup health signal on failure | `test_gate4b_startup_health_signal.py` | ✅ |
| Migration transaction atomicity | `test_gate4c_migration_transaction_safety.py` | ✅ |
| Migration checksum mismatch detection | `test_gate4d_migration_checksum_mismatch.py` | ✅ |

**Висновок Gate 4 (Storage):** Закритий.

---

## 3. Відомі gap-и по пріоритетах

### P0 — Critical Legal Gaps (Sprint 1–2)

| Gap | Опис | Sprint | Статус |
|---|---|---|---|
| **OFFLINE-STATE-01** | Офлайн-документ повертає `ACK` замість `OFFLINE_LOCAL_ACK`. | Sprint 1 | ✅ Closed — `write_path.py:717` |
| **OFFLINE-SYNC-01** | `OfflineSyncService` відсутній. | Sprint 2 | ✅ Closed — `services/offline_sync.py` |
| **SHIFT-CLOSE-01** | `SHIFT_CLOSE` не перевіряє pending offline-документи. | Sprint 2 | ✅ Closed — `write_path.py:1174-1190` |
| **RECEIPT-VAL-01** | Ukrainian fiscal receipt validator. | Sprint 3 | ✅ Closed — `validators/ua_receipt.py`, `test_sprint3_ua_receipt_validator.py` |

### P1 — Production Readiness Gaps

| Gap | Опис | Sprint |
|---|---|---|
| **DPS-TRANSPORT-01** | `DPS_UNIFIED_WINDOW` transport — stub, не реалізований. | P1 |
| **DPS-TRANSPORT-02** | `DPS_PRRO_FISCAL_SERVER` transport — реалізований Sprint 7. gRPC sendChkV2 для SHIFT_OPEN/SELL/Z_REPORT. Live proof на test host. | ✅ Closed |
| **DPS-RECOVERY-SEAM-01** | Persisted signed outbound artifact seam for transport-neutral recovery/offline sync is implemented; pre-migration documents may still surface explicit warning/fallback paths. | ✅ Closed |
| **CRYPTO-GATE-01** | Startup gate, що блокує production із `passthrough` crypto, реалізований у Sprint 6. | ✅ Closed |
| **CHANNEL-FAILOVER-01** | Explicit DPS-channel failover policy (INV-06) не покрита тестами. | P1 |

### P2 — Operational Gaps

| Gap | Опис | Sprint |
|---|---|---|
| **RATE-LIMIT-01** | Rate limiting для ingress відсутній. | A3 |
| **BACKUP-01** | Backup / corruption stop mode відсутній. | A4 |
| **RETENTION-01** | Retention policy для audit/trace/archive відсутня. | A4 |
| **TEST-MARKER-01** | Pytest-маркери `unit/integration/e2e` відсутні. Всі тести в одному flat `tests/`. | низький |

---

## 4. Версійні невідповідності та примітки

| Пункт | Документ | Фактично | Статус |
|---|---|---|---|
| Версія коду | `pyproject.toml`: `1.4.1` | `1.4.1` | ✅ |
| Відомий baseline issue (gate1p/gate1q) | `PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md` §4 | Виправлено до Sprint 0 completion | ✅ Закрито |
| Gate 0 deliverable: `LEGAL_INVARIANTS.md` | Sprint Plan §9 | Створено | ✅ |
| Gate 0 deliverable: `ACCEPTANCE_COVERAGE_SNAPSHOT.md` | Sprint Plan §9 | Цей документ | ✅ |
| Gate 0 deliverable: P0/P1/P2 backlog | Sprint Plan §9 | Частково — в секції 3 цього документа | ⚠️ Часткове |

---

## 5. Відповідність між тест-файлами і вимогами (повна таблиця)

| Тест-файл | Покриває |
|---|---|
| `test_gate1a_concurrency.py` | INV-01, INV-07 — concurrency, no duplicate fiscalization |
| `test_gate1b_idempotency.py` | INV-07 — idempotency, replay |
| `test_gate1c_offline.py` | INV-08, INV-11, INV-12 — offline mode, range allocation |
| `test_gate1d_offline_limits.py` | INV-09, INV-10 — 36h / 168h limits |
| `test_gate1e_shift_guards.py` | INV-03, INV-04 — shift pre-condition, no double shift |
| `test_gate1f_channel_lock.py` | INV-05 — channel lock enforcement |
| `test_gate1g_reconciliation.py` | INV-19 — recovery smoke |
| `test_gate1h_shutdown.py` | INV-18 (graceful shutdown) |
| `test_gate1i_adapter_schema.py` | INV-16 (partial), full canonical payload |
| `test_gate1j_migration_idempotency.py` | Migration idempotency |
| `test_gate1k_transport_errors.py` | Transport error semantics |
| `test_gate1l_lnd_integrity.py` | INV-01, INV-02 — atomic LND, no duplicates |
| `test_gate1m_shift_close.py` | INV-03, INV-04 — shift close / Z-report |
| `test_gate1n_payload_hash.py` | Payload hash integrity (INV-02 related) |
| `test_gate1o_offline_field_consistency.py` | INV-11, INV-12 — offline field correctness |
| `test_gate1p_offline_idempotency.py` | INV-07, INV-12 — offline replay idempotency |
| `test_gate1q_offline_reconciliation.py` | INV-19 — offline docs excluded from reconciliation |
| `test_gate1r_error_state_transitions.py` | INV-19 — error states |
| `test_gate1s_rejected_terminality.py` | INV-19 — REJECTED is terminal |
| `test_gate1t_error_rest_visibility.py` | INV-19 — error visibility via REST |
| `test_gate1u_channel_lock_persistence.py` | INV-05 — channel lock persisted correctly |
| `test_gate1v_bootstrap_init.py` | Cold start / first-run initialization |
| `test_gate1w_startup_recovery.py` | INV-19 — startup recovery stages |
| `test_gate1x_reconciliation_idempotency.py` | INV-07, INV-19 — reconciliation idempotency |
| `test_gate1y_readiness_gating.py` | Health / readiness semantics |
| `test_gate1z_reconciliation_candidate_hygiene.py` | INV-19 — candidate selection correctness |
| `test_gate2a_outbox_enqueue_after_reconciliation.py` | Outbox / sync pipeline |
| `test_gate2b_outbox_idempotency.py` | Outbox idempotency |
| `test_gate2c_outbox_candidate_hygiene.py` | Outbox selection |
| `test_gate2d_audit_log_hygiene.py` | INV-20 — audit events |
| `test_gate2e_trace_hygiene.py` | INV-20 — protocol/transport traces |
| `test_gate2f_document_files_consistency.py` | Archive / document_files |
| `test_gate2g_reconciliation_transport_trace.py` | INV-20 — reconciliation traceability |
| `test_gate2h_retryable_reconciliation.py` | INV-19 — RETRYABLE branch |
| `test_gate2i_recovery_ceiling.py` | INV-19 — recovery ceiling |
| `test_gate2j_recovery_ceiling.py` | INV-19 — ceiling implementation |
| `test_gate2k_admin_retry.py` | INV-19 — admin manual retry |
| `test_gate2l_manual_list.py` | Admin visibility |
| `test_gate2m_ops_summary_manual_count.py` | Ops observability |
| `test_gate2n_ops_summary_outbox_count.py` | Ops observability |
| `test_gate2o_ops_summary_recon_pending.py` | Ops observability |
| `test_gate3a_crypto_seam.py` | INV-17, INV-18 — crypto seam audit |
| `test_gate3c_crypto_config_seam.py` | INV-17 — crypto config |
| `test_gate3d_crypto_timeout.py` | INV-18 — crypto timeout |
| `test_gate3f_sidecar_provider.py` | INV-17 — sidecar provider |
| `test_gate3g_sidecar_hardening.py` | INV-17 — sidecar hardening |
| `test_gate3i_crypto_breaker.py` | Crypto circuit breaker |
| `test_gate3j_breaker_reset.py` | Breaker admin reset |
| `test_gate3k_ops_summary_crypto.py` | Crypto state observability |
| `test_gate4a_storage_safety_seam.py` | Startup DB integrity check |
| `test_gate4b_startup_health_signal.py` | Startup failure health signal |
| `test_gate4c_migration_transaction_safety.py` | Migration atomicity |
| `test_gate4d_migration_checksum_mismatch.py` | Migration drift detection |
| `test_migration_runner.py` | Basic migration runner |
| `test_pilot_smoke.py` | End-to-end happy path smoke |
| `test_runtime_rest.py` | REST API contract |
| `test_runtime_container.py` | Container wiring |
| `test_runtime_metrics_alerts.py` | Metrics / alerts |
| `test_runtime_ops_improvements.py` | Ops improvements |
| `test_runtime_shells.py` | XML-RPC / Maria shell init |
| `test_startup_supervisor.py` | Startup supervisor |
| `test_transport_stubs.py` | DPS transport stubs |
| `test_write_path.py` | Write-path core (unit) |
| `test_checkbox_transport.py` | Checkbox transport |
| `test_adapters.py` | Adapter mapping |
| `test_models.py` | Canonical model validation |
| `test_config.py` | Config loading |
| `test_repository.py` | Repository layer |
| `test_reconciliation.py` | Reconciliation service |
| `test_perf_ops.py` | Performance smoke |
| `test_manifest_validation.py` | Schema manifest |
| `test_sprint10_payment_types.py` | Payment type config, CASHLESS guard |
| `test_sprint10_cash_balance.py` | Cash balance carry-over between shifts |
| `test_sprint10_change.py` | Решта (PAID − total_sum) |
| `test_sprint10_rounding.py` | Fiscal rounding rules |
| `test_sprint10_epz.py` | CASH_WITHDRAWAL через ЕПЗ |
| `test_sprint10_xreport.py` | X-звіт (проміжний без закриття) |
| `test_sprint10_canonical_layer.py` | Canonical model coverage (discounts, tax_id) |
| `test_sprint10_discounts.py` | `<D>`/`<S>` discount/surcharge serialization |
| `test_sprint10_wave2_l_epz.py` | `<L>` text comment, `<EPZ>` Z-report element |
| `test_sprint9_full_e_element.py` | `<E>` з FN/NO/SM/TS, `<TX>` blocks, _calc_tax |
| `test_e2e_lifecycle.py` | Full fiscal lifecycle E2E (7 операцій, ACK, LND) |

---

---

## 6. Sprint 7 — DPS Fiscal Server Transport (39 тестів)

**Sprint 7 scope:** перший реальний прямий DPS transport contour (gRPC sendChkV2).

**Live proof (2026-04-13, `cabinet.tax.gov.ua:9443`):**
- `statusRro` → OK, online, registered
- `SHIFT_OPEN` → status=1, id=`fEcR0cFuVnI`
- `SELL` → status=1, id=`YseZpbfpCCc`
- `Z_REPORT` → status=1, id=`Jo5osiB3yNo`

| Deliverable | Code | Tests | Live | Notes |
|---|---|---|---|---|
| DPS fiscal-server transport (gRPC sendChkV2) | ✅ | 15 tests | ✅ | `transports/dps_fiscal_server.py` |
| ФСКО XML serializer (build_dps_xml) | ✅ | 10 tests | ✅ | `serializers/dps_xml.py`, SHIFT_OPEN/SELL/Z_REPORT |
| JKS crypto sidecar (jkurwa/gost89) | ✅ | 5 tests | ✅ | `sidecar/server.js`, CMS/PKCS#7 DER over DSTU 4145 |
| DPS probe (statusRro/infoRro) | ✅ | 5 tests | ✅ | REST `/v1/admin/dps-probe` |
| Recovery via lastChk | ✅ | 1 test | — | `poll_status()`, reconciliation integration |
| Write-path DPS integration | ✅ | covers dx6-dx9 | — | `_resolve_sign_input`, `_resolve_dps_mac`, `_stage_sign` DPS branch |
| Z-report numbering | ✅ | 3 tests | ✅ | `allocate_z_report_number`, retry stability |
| Proto stubs (official spec) | ✅ | — | ✅ | Package `com.programika.rro.ws.chk` |
| Production gates (crypto + transport) | ✅ | fs9 | — | `_enforce_production_crypto_gate`, `_enforce_production_transport_gate` |
| MAC hash chain (core-owned) | ✅ | dx8, dx9 | ✅ | SHA-256 of PAYLOAD_XML, server-validated |
| date_time ↔ XML TS alignment | ✅ | dx10 | ✅ | Kyiv-local-as-epoch via `_kyiv_local_epoch` |

**Sprint 7 test files (39 tests total):**

| Тест-файл | Count | Покриває |
|---|---|---|
| `test_sprint7_dps_fiscal_server.py` | 15 | Transport send/poll/container wiring, error mapping |
| `test_sprint7_dps_xml_serializer.py` | 10 | XML shape, write-path integration, MAC chain, date_time |
| `test_sprint7_crypto_sidecar_signing.py` | 5 | sign_raw client/provider, bytes passthrough |
| `test_sprint7_dps_probe.py` | 5 | statusRro/infoRro shape, REST endpoint, profile routing |
| `test_sprint7_z_numbering.py` | 3 | Z-report number allocation, retry stability |
| `test_sprint7_dps_recovery.py` | 1 | lastChk reconciliation integration |

### Sprint 7 — Residual Gaps

| Gap | Severity | Notes |
|---|---|---|
| **DPS-SIDECAR-PROD-01** | P1 | Sidecar PoC: single-threaded, no TLS, no auth, no graceful shutdown |
| **DPS-OPS-RETURN-01** | ✅ Closed | RETURN supported: T=1, check_type=CHK(1), id_cancel for linkage, production gate |
| **DPS-OPS-SERVICE-01** | ✅ Closed | SERVICE_IN/SERVICE_OUT supported: T=2, <I>/<O>, check_type=CHK(1) |
| **DPS-UNIFIED-01** | P1 | `DPS_UNIFIED_WINDOW` transport remains stub |
| **DPS-STATUSRRO-POST-01** | P3 | post-cleanup `statusRro` probe not captured (JKS not in repo — expected) |
| **DPS-TYPING-01** | P3 | `signed_payload` type drift: `str` annotation vs `bytes` in DPS path |
| **DOCS-MISSING-01** | P2 | Sprint Plan §11 requires `PROTOCOL_SHAPE_AUDIT.md`, `DPS_TRANSPORT.md`, `OFFLINE_SYNC.md`, `ARCHIVE_POLICY.md` — none exist in `docs/` |

---

## 7. Sprint 8 — DPS Error Handling & Rate Limit (22 тестів)

| Deliverable | Tests | Status |
|---|---|---|
| DPS error classification (MAC recovery, rate limit, rejected) | 16 | ✅ |
| retry_after_seconds propagation to reconciliation + REST | 7 | ✅ |

---

## 8. Sprint 9 — Fiscal Compliance & Guards (66 тестів)

**ФСКО протокол v2.2.3 (18.02.2025) скачано та проаналізовано.**

### Operations supported:

| Operation | XML type | check_type | Status |
|---|---|---|---|
| SHIFT_OPEN | T=108 | SERVICECHK(3) | ✅ Live proven |
| SELL | T=0 | CHK(1) | ✅ Live proven |
| RETURN | T=1 | CHK(1) | ✅ Live proven |
| SERVICE_IN | T=2 `<I>` | CHK(1) | ✅ Unit tested |
| SERVICE_OUT | T=2 `<O>` | CHK(1) | ✅ Unit tested |
| Z_REPORT | `<Z>` | ZREPORT(2) | ✅ Live proven |

### Tax groups (Sprint 9 step 2-3):

| Feature | Tests | Status |
|---|---|---|
| TX attribute on `<P>` (ПДВ група) | TG1-TG7 | ✅ |
| TX="0" (звільнено) та TX="-1" (не об'єкт) | TG3b, TG6 | ✅ |
| TX1 attribute (акциз/другий податок) | TG8-TG10 | ✅ |
| Tax group definitions table (per FN) | TGD1, migration 007 | ✅ |

### Compliance guards (Sprint 9 step 3):

| Guard | Що блокує | Tests |
|---|---|---|
| `excise_allowed=0` master switch | Продаж підакцизу без ліцензії | TGD25-27 |
| Незапрограмована податкова група | Помилка конфігурації | TGD28 |
| УКТЗЕД обов'язковий (per group) | Фіскальний контроль | TGD2-3 |
| Акцизна марка обов'язкова (per group) | Фіскальний контроль | TGD3-4 |
| Кількість марок = кількість одиниць | Фіскальний контроль | TGD13-15 |
| Дробова кількість + обов'язкова марка | Пляшковий vs розлив | TGD16-17 |
| Формат марки `[A-Z]{4}[0-9]{6}` | Помилки введення | TGD22-24 |
| Готівка ≥ 50 000 грн | Закон | TGD10-12 |
| Порожнє ім'я товару | Некоректний чек | TGD30 |
| Зміна > 24 години | Закон | TGD31 |
| business_ts в майбутньому | Перевірка годинника POS | TGD29 |
| Суму рахуємо самі (price×qty/1000) | Не довіряємо POS | TGD18 |
| RETURN без related_receipt_id | Compliance gate | RG1-4 |

### Full `<E>` element (Sprint 9 step 4):

| Feature | Tests | Status |
|---|---|---|
| `<E>` з FN, NO, SM, TS атрибутами | FE2, FE7 | ✅ |
| `<TX>` sub-elements з TXSM/DTSM розрахунком | FE2, FE3 | ✅ |
| TXAL=0 (ПДВ в ціні) | FE4 | ✅ |
| TXAL=2 (акциз на ціну з ПДВ) | FE5 | ✅ |
| tax_groups з DB до `<E>` через pipeline | FE6 | ✅ |
| Без tax_groups → `<E>` з повними атрибутами FN/NO/SM/TS (без `<TX>`) | FE1 | ✅ |

### Full Z-звіт (Sprint 9 step 5):

| Feature | Tests | Status |
|---|---|---|
| `<TXS>` — податкові підсумки за зміну з TXI/TXO | ZR2 | ✅ |
| `<M>` — оберти по типах оплати (CASH/CASHLESS) | ZR3 | ✅ |
| `<IO>` — суми внесення/видачі (SERVICE_IN/OUT) | ZR4 | ✅ |
| `<NC>` — кількість чеків продажу/повернення | ZR5 | ✅ |
| Full pipeline: SELL+RETURN+SERVICE_IN → Z агрегація | ZR6 | ✅ |
| Backward compat (без даних → мінімальний Z) | ZR1 | ✅ |

### Sprint 9 — Residual Gaps

| Gap | Severity | Notes |
|---|---|---|
| **DPS-XML-E-01** | ✅ Closed | Повний `<E>` з `<TX>` блоками та розрахунком TXSM/DTSM |
| **DPS-XML-Z-01** | ✅ Closed | Повний Z-звіт з TXS, M, IO, NC |
| **DPS-XML-CA-01** | ✅ Closed | `<CA>` серіалізується в XML (Sprint 9 step 6) |
| **DPS-XML-CZD-01** | ✅ Closed | CZD серіалізується в XML (Sprint 9 step 6) |
| **CASH-BALANCE-01** | P1 | Залишок готівки не трекається / не переноситься між змінами |
| **OFFLINE-STATE-01** | ✅ Closed | `write_path.py:717` (`OFFLINE_LOCAL_ACK`) — закритий |
| **DPS-SIDECAR-PROD-01** | P1 | Sidecar PoC: single-threaded, no TLS |
| **DPS-UNIFIED-01** | P1 | DPS_UNIFIED_WINDOW transport — stub |
| **CASH-WITHDRAWAL-01** | ✅ Closed | CASH_WITHDRAWAL T=8 реалізований (Sprint 9 step 7) |

### Post-review critical fixes

| Fix | Bead | Status |
|---|---|---|
| MAC recovery semantic equivalence | PRRO_GATE-lp3 | ✅ Fixed — tax_groups + Z aggregation + related_receipt_id in recovery |
| Offline replay transport fields | PRRO_GATE-4wi | ✅ Fixed — business_ts + id_offline + related_receipt_id + transport_profile |
| DPS status classification | PRRO_GATE-r2c | ✅ Fixed — aligned with proto, -4/-8 rejected, MAC recovery only -12 |
| TXAL=3 guard | — | ✅ Blocked — explicit guard before sign, not in pilot scope |

### Pilot scope exclusions

| Exclusion | Reason | Guard |
|---|---|---|
| **TXAL=3** (absolute excise per quantity) | АЗС/паливо, not retail/HoReCa | `_guard_tax_group_compliance` rejects before sign |
| **DPS_UNIFIED_WINDOW** | Second contour, not needed for pilot | Transport stub |
| **Offline full lifecycle** | Separate sprint (Sprint 11) | — |

### Authoritative sources for DPS runtime

| Source | Authority for |
|---|---|
| `transports/proto/fiscal_server.proto` | gRPC contract, status codes, message fields |
| `docs/dps_protocol/262576_(1).md` | API methods, field semantics, check_type enum |
| `docs/dps_protocol/530962.md` | XML format (ФСКО v2.2.3), broader than gRPC runtime |

---

## 9. Sprint 10 — ФСКО Full Protocol + Discounts + E2E (completed)

**Sprint 10 wave 1** (canonical layer, payment types, cash balance, X-report, EPZ):

| Deliverable | Tests | Status |
|---|---|---|
| Payment type config (per FN) | `test_sprint10_payment_types.py` | ✅ |
| Cash balance carry-over | `test_sprint10_cash_balance.py` | ✅ |
| Решта (PAID − total_sum) serialization | `test_sprint10_change.py` | ✅ |
| Fiscal rounding rules | `test_sprint10_rounding.py` | ✅ |
| CASH_WITHDRAWAL (T=8) via ЕПЗ | `test_sprint10_epz.py` | ✅ |
| X-звіт (X_REPORT — проміжний) | `test_sprint10_xreport.py` | ✅ |
| Canonical layer (discounts, tax_id/tax_id_2) | `test_sprint10_canonical_layer.py` | ✅ |

**Sprint 10 wave 2** (ФСКО protocol elements + E2E lifecycle):

| Feature | Tests | Code | Status |
|---|---|---|---|
| `<D>/<S>` — знижка/надбавка на `<P>` | `test_sprint10_discounts.py` | `dps_xml.py` | ✅ |
| `<L>` — текстовий коментар (header/footer) | L1-L5 | `dps_xml.py:_build_check/_build_service` | ✅ |
| `<EPZ>` — підсумки ЕПЗ у Z-звіті | EPZ1-EPZ3 | `dps_xml.py:_build_z_report` | ✅ |
| `<E>` завжди з FN/NO/SM/TS (без tax_groups) | FE1 | `dps_xml.py:_build_e_element` | ✅ |
| `Discount.value ge=0` в canonical model | `test_models.py` | `models/canonical.py` | ✅ |
| `tax_id`/`tax_id_2` в `CanonicalReceiptItem` | `test_sprint10_canonical_layer.py` | `models/canonical.py` | ✅ |
| Full E2E lifecycle (7 ops, ACK, LND, outbox) | `test_e2e_lifecycle.py` | `services/write_path.py` | ✅ |

### Sprint 10 — Known Gaps

| Gap | Severity | Notes |
|---|---|---|
| **CASH-BALANCE-01** | P1 | Cash balance не переноситься в DPS XML (тільки локально) |
| **ASK-OFFLINE-CODES-01** | P1 | API-flow запиту діапазону офлайн-кодів не покрито E2E |
| **RATE-LIMIT-01** | P2 | Rate limiting для ingress відсутній |
| **BACKUP-01** | P2 | Backup / corruption stop mode відсутній |

---

## 10. Roadmap — Sprint 11–14

> Детальний покроковий план: `docs/SPRINT_11_14_DETAILED_PLAN.md`

### Sprint 11 — Offline Full Lifecycle

**Мета:** закрити всі offline-state gap-и одним вертикальним зрізом.

| Gap | Що закриває | Пріоритет |
|---|---|---|
| **INV-08** | API-driven GO_OFFLINE flow (не manual DB seed) | P1 |
| **ASK-OFFLINE-CODES-01** | E2E тест: запит діапазону → зберігання → використання | P1 |
| **LND-OFFLINE-CRASH-01** | LND crash+recovery під час офлайн-сесії | P1 |
| **INV-06 / CHANNEL-FAILOVER-01** | Тест: failover заборонений під час активної зміни | P1 |

**Deliverables:** тести для GO_OFFLINE API-flow, offline code range request E2E, LND crash recovery scenario, channel failover guard test.

**Acceptance criteria:**
- GO_OFFLINE і GO_ONLINE ініціюються через API, а не через DB seed;
- offline номер запитується, зберігається і витрачається атомарно;
- LND після crash + recovery залишається монотонним;
- спроба channel switch з відкритою зміною = explicit rejected.

---

### Sprint 12 — Fiscal Compliance Completeness

**Мета:** закрити залишкові фіскальні accuracy gap-и.

| Gap | Що закриває | Пріоритет |
|---|---|---|
| **INV-16** | Акцизні товари: adapter → write-path → XML (УКТЗЕД + марка) E2E | P1 |
| **CASH-BALANCE-01** | Залишок готівки: carry-over між змінами + серіалізація в DPS XML | P1 |

**Deliverables:** повний excise pipeline тест (adapter→serializer), cash balance carry-over в shift open/close flow.

**Acceptance criteria:**
- підакцизний товар з УКТЗЕД і маркою проходить повний pipeline до XML без помилок;
- залишок готівки на початку зміни відображає кінець попередньої;
- спроба продати підакцизний товар без УКТЗЕД = rejected до sign.

---

### Sprint 13 — Production Infrastructure

**Мета:** зробити систему production-deployable.

| Gap | Що закриває | Пріоритет |
|---|---|---|
| **DPS-SIDECAR-PROD-01** | Crypto sidecar: TLS, auth, graceful shutdown | P1 |
| **DPS-UNIFIED-01** | `DPS_UNIFIED_WINDOW` transport (другий DPS контур) | P1 |
| **RATE-LIMIT-01** | Ingress rate limiting (REST + XML-RPC) | P2 |

**Deliverables:** sidecar з TLS і mutual auth, DPS_UNIFIED_WINDOW transport handler, rate limit middleware.

**Acceptance criteria:**
- sidecar відхиляє запити без клієнтського сертифіката;
- DPS_UNIFIED_WINDOW успішно подає mock fiscal document;
- ingress відхиляє перевищення rate limit з 429 і audit event.

---

### Sprint 14 — Operational Safety + Pilot

**Мета:** operational correctness, data lifecycle, pilot readiness.

| Gap | Що закриває | Пріоритет |
|---|---|---|
| **BACKUP-01** | SQLite snapshot job + corruption → STOP_MODE | P2 |
| **RETENTION-01** | Retention/purge policy для audit/trace/archive | P2 |
| **DOCS-MISSING-01** | `PROTOCOL_SHAPE_AUDIT.md`, `DPS_TRANSPORT.md`, `OFFLINE_SYNC.md`, `ARCHIVE_POLICY.md` | P2 |
| **TEST-MARKER-01** | Pytest markers `unit / integration / e2e` | low |

**Deliverables:** backup job + runbook, retention config + purge script, операційні docs, pytest marker taxonomy.

**Acceptance criteria:**
- автоматичний backup запускається за розкладом і перевіряє цілісність;
- корупція SQLite переводить ноду в STOP_MODE з видимим health signal;
- старі audit/trace записи purge'уються за configured TTL;
- всі 4 docs написані та відповідають поточному коду.

---

### Hygiene (continuous)

| Gap | Severity | Дія |
|---|---|---|
| **DPS-TYPING-01** | P3 | `signed_payload: bytes` annotation у DPS transport path |
| **DPS-STATUSRRO-POST-01** | P3 | post-cleanup `statusRro` probe (JKS не в репо — lower priority) |

---

*Snapshot оновлений на дату Sprint 10 wave 2 (2026-04-15). Повинен оновлюватись після кожного Sprint.*
