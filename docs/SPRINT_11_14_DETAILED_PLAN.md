# Sprint 11–14: Detailed Implementation Plan

> **Python-era plan; superseded by Rust gateway M3b (2026-05-16).**
>
> This document plans Sprint 11–14 for the **Python gateway**.  Several rules below — including step 11.2's "`GO_ONLINE` blocked by existing `OFFLINE_LOCAL_ACK` backlog", step 11.4 Z_REPORT blanket blocker, and other offline-shift rules — reflect the Python implementation and conflict with the Rust gateway M3b return-online + Pattern C drain design (`Offline → GoingOnline → drain in lnd ASC → Online`).
>
> The Rust gateway uses:
> - `node_state.mode` transitions `Offline → GoingOnline` are driven by the W8 return-online probe; `GoingOnline → Online` lands after the W9b backlog drain plus W12 KVT2 confirmation deliver every offline doc to final DPS `ACK`.  There is no blanket "GO_ONLINE refused while backlog exists" — the drain *is* what closes the backlog.
> - W10 offline shift close/open policy guard distinguishes **ONLINE** Z_REPORT (blocked over pending backlog) from **OFFLINE-mode local** Z_REPORT close-of-day (allowed as Pattern C `OFFLINE_LOCAL_ACK` document).  See `docs/OFFLINE_SHIFT_CLOSE_DECISION.md` §0 + `docs/superpowers/plans/2026-05-14-m3b-implementation.md` §Task 10.
>
> The Python-era plan below remains accurate for the Python gateway scope; do not back-port its rules to the Rust gateway without consulting the M3b plan first.

**Версія:** 1.0, 2026-04-15 (Python gateway)  
**На основі:** `ACCEPTANCE_COVERAGE_SNAPSHOT.md` §10, аудиту коду  
**Стан коду:** Sprint 10 wave 2, 586 passed (Python gateway)

---

## Умовні позначки

- `[HOT]` — зміна в hot zone (plan first, targeted test)
- `[NEW]` — новий файл
- `[TEST]` — тільки тест, код вже є
- `[IMPL]` — реалізація відсутня або stub
- `[DB]` — зачіпає схему / міграції (залучати migration-keeper)

---

## Sprint 11 — Offline Full Lifecycle

**Мета:** GO_OFFLINE і ASK_OFFLINE_CODES зараз — тільки enum-и без handlers. Sprint закриває цей gap і повністю покриває offline state machine тестами.

**Передумови перед початком:**
- перечитати `services/write_path.py:202-218` (offline acceptance path)
- перечитати `repositories/offline.py` повністю
- перечитати `enums.py:90-97` (NodeMode state machine)

---

### Крок 11.1 — Реалізувати handler `GO_OFFLINE` у write-path `[IMPL][HOT]`

**Файли:** `src/prro_gateway/services/write_path.py`

**Що зробити:**
1. У методі `_dispatch_operation()` (або аналогу) додати case `OperationType.GO_OFFLINE`.
2. Handler повинен:
   - Перевірити, що `node_state.mode == NodeMode.ONLINE` — інакше `INVALID_STATE`.
   - Перевірити відсутність pending не-offline документів у стані SENT/KVT1/KVT2.
   - Транзакційно:
     - Оновити `node_state.mode = NodeMode.GOING_OFFLINE`.
     - Створити запис `offline_sessions` через `OfflineRepository.create_open_session()`.
   - Поза транзакцією: оновити `node_state.mode = NodeMode.OFFLINE`.
3. Повернути `WorkerProcessResult(outcome='ACK', ...)`.

**Інваріанти до перевірки:**
- Crypto/network не викликаються всередині SQLite IMMEDIATE транзакції.
- Shift може бути відкритою при GO_OFFLINE (офлайн дозволено під час зміни).

**Тест (новий):** `tests/test_sprint11_go_offline_handler.py`
```
test_go_offline_transitions_node_to_offline        # ONLINE → OFFLINE
test_go_offline_creates_session_record             # offline_sessions row
test_go_offline_from_non_online_state_rejected     # OFFLINE → rejected
test_go_offline_with_pending_sent_docs_rejected    # pending SENT → blocked
test_go_offline_with_open_shift_allowed            # shift OK, should pass
```

---

### Крок 11.2 — Реалізувати handler `GO_ONLINE` / `GOING_ONLINE` `[IMPL][HOT]`

**Файли:** `src/prro_gateway/services/write_path.py`, `src/prro_gateway/repositories/offline.py`

**Що зробити:**
1. Case `OperationType.GO_ONLINE`:
   - Перевірити `node_state.mode == NodeMode.OFFLINE`.
   - Перевірити відсутність `OFFLINE_LOCAL_ACK` документів — якщо є, повернути `OFFLINE_BACKLOG_NOT_SYNCED`.
   - Закрити поточну offline session: `accumulated_seconds = now - started_at + existing_accumulated`.
   - Оновити `node_state.mode = NodeMode.GOING_ONLINE` → `ONLINE`.
2. Додати `OfflineRepository.close_session(session_id, accumulated_seconds)` якщо немає.

**Тест:** `tests/test_sprint11_go_offline_handler.py` (продовжити той самий файл)
```
test_go_online_transitions_node_back_to_online
test_go_online_closes_session_with_correct_duration
test_go_online_blocked_if_offline_backlog_exists
test_go_online_from_non_offline_state_rejected
```

---

### Крок 11.3 — Реалізувати handler `ASK_OFFLINE_CODES` `[IMPL][HOT]`

**Контекст:** У production це буде результат запиту до DPS API. У поточній архітектурі — команда, що вставляє range у `offline_ranges` від оператора або DPS-відповіді.

**Файли:** `src/prro_gateway/services/write_path.py`, `src/prro_gateway/repositories/offline.py`

**Що зробити:**
1. Case `OperationType.ASK_OFFLINE_CODES`:
   - Прочитати з `payload`: `first_fiscal_no`, `last_fiscal_no`, `source` (optional).
   - Перевірити: `first_fiscal_no < last_fiscal_no`, обидва > 0, no overlap з існуючими ranges (query).
   - Вставити запис у `offline_ranges` зі статусом `ACTIVE`, `next_fiscal_no = first_fiscal_no`.
2. Повернути `WorkerProcessResult(outcome='ACK', ...)`.

**Тест:** `tests/test_sprint11_offline_codes.py`
```
test_ask_offline_codes_creates_active_range
test_ask_offline_codes_range_usable_immediately
test_ask_offline_codes_duplicate_range_rejected
test_ask_offline_codes_invalid_range_rejected       # first >= last
test_offline_range_exhaustion_marks_exhausted       # allocate until last, verify EXHAUSTED
test_offline_range_exhaustion_raises_on_next_alloc  # no active range → LookupError
```

---

### Крок 11.4 — E2E тест повного offline lifecycle `[TEST]`

**Файл:** `tests/test_sprint11_offline_e2e.py`

> **Python-era blanket-blocker; superseded by Rust M3b W10 (2026-05-16).**  Step 5 below treats every Z_REPORT attempt during offline backlog as blocked, without distinguishing online vs offline-mode close-of-day.  The Rust gateway M3b W10 redesign distinguishes **ONLINE Z_REPORT** (blocked over pending backlog — `ONLINE_Z_REPORT_BLOCKED_BACKLOG`) from **OFFLINE-mode local Z_REPORT** (allowed as Pattern C `OFFLINE_LOCAL_ACK` document — `OFFLINE_Z_REPORT_LOCAL_CLOSE_ACCEPTED`).  See `docs/OFFLINE_SHIFT_CLOSE_DECISION.md` §0 + `docs/superpowers/plans/2026-05-14-m3b-implementation.md` §Task 10 for the corrected policy.  The Python sequence below remains accurate for the Python-era gateway scope.

**Послідовність:**
```
1. ASK_OFFLINE_CODES  → range зареєстрований
2. GO_OFFLINE         → node.mode = OFFLINE, session created
3. SELL (offline)     → OFFLINE_LOCAL_ACK, offline_fiscal_no assigned
4. SELL (offline)     → OFFLINE_LOCAL_ACK, наступний номер
5. Z_REPORT attempt   → OFFLINE_BACKLOG_NOT_SYNCED (blocked) — Python-era
                        blanket rule; Rust M3b W10 distinguishes ONLINE Z
                        (blocked) from OFFLINE-mode local Z_REPORT close
                        (allowed as Pattern C OFFLINE_LOCAL_ACK).
6. /admin/offline-sync POST → два документи → ACK
7. GO_ONLINE          → session closed, node.mode = ONLINE
8. Z_REPORT           → ACK
```

**Assertions:**
- `offline_sessions` запис закритий, `accumulated_seconds > 0`
- обидва SELL документи мають різні `offline_fiscal_no` з range
- LND після sync монотонний
- range `next_fiscal_no` збільшено на 2

---

### Крок 11.5 — LND crash recovery test `[TEST]`

**Файл:** `tests/test_sprint11_lnd_crash_recovery.py`

**Сценарій:**
1. Відкрити зміну, виконати SELL → LND=1.
2. Виконати SELL → LND=2.
3. Зімітувати crash: встановити документ у стан SIGNED (не фінальний) — "crashed after sign".
4. Створити новий `WritePathWorker`, викликати `process_next` для того ж FN.
5. Воркер підбирає stuck document через reconciliation path.
6. Виконати ще один SELL → новий LND > 2, без пропусків.

**Assertions:**
- LND монотонний і без дублікатів після recovery
- Stuck document або переходить у ACK/REJECTED або в REQUIRES_MANUAL_RECONCILIATION (terminal)
- Новий SELL не отримав LND < попереднього

---

### Крок 11.6 — Channel failover guard test `[TEST]`

**Контекст:** Логіка вже є у `write_path.py:1157-1159`. Потрібен explicit тест для INV-06.

**Файл:** `tests/test_gate1f_channel_lock.py` (додати тести) або `tests/test_sprint11_channel_failover.py`

```
test_channel_switch_during_open_shift_rejected          # backend_profile_id mismatch
test_transport_switch_during_open_shift_rejected        # transport_profile_id mismatch
test_protocol_switch_during_open_shift_rejected         # protocol mismatch
test_channel_owner_switch_during_open_shift_rejected    # channel_owner mismatch
test_same_channel_during_open_shift_accepted            # all same → passes
test_channel_switch_after_z_report_accepted             # no open shift → passes
```

---

### Sprint 11 — Acceptance Gate

- [ ] `pytest tests/test_sprint11_*.py` — 0 failed
- [ ] `pytest tests/test_gate1c_offline.py tests/test_gate1d_offline_limits.py` — 0 failed
- [ ] `pytest tests/test_gate1f_channel_lock.py` — 0 failed
- [ ] Перевірити: `NodeMode` transition diagram у `CLAUDE.md` відповідає реалізації

---

---

## Sprint 12 — Fiscal Compliance Completeness

**Контекст аудиту:**
- Excise pipeline (`uktzed`, `excise_barcodes`) **повністю реалізований**: adapter (`checkbox_rest.py:98-100`) → write-path guards → serializer (`dps_xml.py`, `<CZD>`, `<CA>`) → repository (`excise.py`). Gap — відсутній E2E pipeline тест.
- Cash balance: логіка `_get_shift_cash_balance()` і `_enrich_payload_for_dps()` існує. `last_cash_balance` оновлюється на Z_REPORT ACK. Gap — немає тесту для carry-over: shift 1 close → shift 2 open inherits balance.

---

### Крок 12.1 — Excise goods E2E pipeline тест `[TEST]`

**Файл:** `tests/test_sprint12_excise_pipeline.py`

**Сценарій (adapter → write-path → signed XML):**
```
test_excise_good_uktzed_reaches_xml_czd        # uktzed → CZD="1234567890" у <P>
test_excise_good_barcode_reaches_xml_ca        # excise_barcodes → <CA CA="..."/> у <P>
test_excise_good_multiple_marks               # 2 marks → 2 <CA> elements
test_excise_sell_without_uktzed_rejected      # group requires_uktzed=1 → rejected
test_excise_sell_without_mark_rejected        # group requires_excise_mark=1 → rejected
test_excise_mark_duplicate_blocked            # same mark twice → DuplicateExciseMarkError
test_excise_mark_sold_after_ack               # mark status = SOLD after ACK
test_excise_mark_returned_after_return_ack    # mark status = RETURNED after RETURN ACK
```

**Примітка:** Тести через `WritePathWorker` зі `StubCryptoProvider` — інспектувати payload, що передається в `sign()`.

---

### Крок 12.2 — Cash balance carry-over тест `[TEST]`

**Файл:** `tests/test_sprint12_cash_balance_carryover.py`

**Сценарій:**
```
1. SHIFT_OPEN (shift-1)
2. SELL cash 7000 → balance = 7000
3. SERVICE_IN 5000 → balance = 12000
4. Z_REPORT → last_cash_balance persisted = 12000 у node_state
5. SHIFT_OPEN (shift-2) → DPS XML SM field = 12000
6. SELL cash 3000 → balance = 3000 (відносно shift-2)
```

**Assertions:**
- `node_state.last_cash_balance == 12000` після Z_REPORT
- Payload переданий у `sign()` при shift-2 SHIFT_OPEN: `<O SM="12000">`
- REST response для sell у shift-2: `cash_balance == 3000`

---

### Крок 12.3 — Перевірити покриття `cash_balance_mode='reset'` `[TEST]`

**Файл:** `tests/test_sprint12_cash_balance_carryover.py`

```
test_cash_balance_reset_mode_opens_shift_with_zero
    # node_state.cash_balance_mode='reset' → shift-2 SM=0 незалежно від last_cash_balance
```

---

### Sprint 12 — Acceptance Gate

- [ ] `pytest tests/test_sprint12_*.py` — 0 failed
- [ ] `pytest tests/test_sprint10_cash_balance.py tests/test_sprint3_excise_validator.py` — 0 failed (регресія)
- [ ] Перевірити, що нові тести не дублюють `test_sprint10_cash_balance.py`

---

---

## Sprint 13 — Production Infrastructure

---

### Крок 13.1 — Реалізувати `DPS_UNIFIED_WINDOW` transport `[IMPL][NEW]`

**Файли:**
- `src/prro_gateway/transports/dps_unified_window.py` — реалізація (замість stub у `stubs.py`)
- `src/prro_gateway/runtime/container.py` — замінити stub реєстрацію на реальний клас

**Що зробити:**
1. Клас `DpsUnifiedWindowTransport` з методом `send(*, document_id, signed_payload, fiscal_number, backend_profile_id, transport_profile_id, **kwargs) -> SendResult`.
2. HTTP POST до endpoint з `config.transport_profiles[id].base_url` + `/v1/docs/` (або per spec).
3. Парсити HTTP-відповідь → `SendResult(submission_status='SUBMITTED_KVT_PENDING')`.
4. `poll_status(transport_request_id)` → повертати поточний статус з DPS.
5. Маппінг HTTP-помилок → `TransportRetryableError` / `TransportTerminalError`.

**Тести:** `tests/test_sprint13_dps_unified_window.py`
```
test_send_posts_to_correct_endpoint              # mock HTTP → verify URL
test_send_returns_submitted_kvt_pending          # 200 → SUBMITTED_KVT_PENDING
test_send_http_500_raises_retryable_error
test_send_http_400_raises_terminal_error
test_poll_status_kvt1_on_first_call
test_poll_status_ack_on_second_call
test_router_resolves_unified_window_profile      # container wiring
```

**Risk:** Якщо DPS Unified Window потребує SOAP/XML замість JSON — потребує окремого research спочатку. Підтвердити endpoint shape перед реалізацією.

---

### Крок 13.2 — Hardening crypto sidecar: TLS + auth `[IMPL]`

**Файли:**
- `sidecar/server.js`
- `sidecar/` (можливо нові config files для TLS)

**Що зробити:**
1. Додати `--tls-cert` / `--tls-key` / `--ca-cert` CLI args.
2. `https.createServer()` замість `http.createServer()` при наявності cert.
3. Mutual TLS: перевіряти клієнтський сертифікат (`requestCert: true, rejectUnauthorized: true`).
4. Graceful shutdown: `server.close()` на `SIGTERM` + drain in-flight requests.
5. Multi-thread: `cluster` або `worker_threads` для паралельних запитів.

**Тест:** `tests/test_gate3g_sidecar_hardening.py` — розширити:
```
test_sidecar_rejects_without_client_cert
test_sidecar_accepts_valid_client_cert
test_sidecar_graceful_shutdown_drains_requests
```

**Примітка:** Sidecar — Node.js, тести через subprocess або mock HTTP. Якщо Python-тести неможливі — integration test у `sidecar/test/`.

---

### Крок 13.3 — Ingress rate limiting `[IMPL]`

**Файли:**
- `src/prro_gateway/runtime/rest_app.py`
- `src/prro_gateway/runtime/container.py`

**Що зробити:**
1. FastAPI middleware або dependency з sliding-window rate limiter (per source IP або per fiscal_number).
2. Config key: `ingress.rate_limit.requests_per_minute` (default 0 = off, backward compat).
3. Перевищення → HTTP 429 + `{"error": "RATE_LIMIT_EXCEEDED"}` + audit event.

**Тест:** `tests/test_sprint13_rate_limit.py`
```
test_rate_limit_passes_under_threshold
test_rate_limit_blocks_at_threshold             # N+1 request → 429
test_rate_limit_resets_after_window             # wait 1 window → passes again
test_rate_limit_disabled_by_default             # config=0 → no 429
test_rate_limit_audit_event_on_block            # 429 → audit log entry
```

---

### Крок 13.4 — Request size limits `[IMPL]`

**Файл:** `src/prro_gateway/runtime/rest_app.py`

**Що зробити:**
- FastAPI `max_request_body_size` або middleware.
- Config key: `ingress.max_request_body_bytes` (default: 1MB).
- Перевищення → HTTP 413 + canonical error.

**Тест:** `tests/test_sprint13_rate_limit.py`
```
test_request_size_limit_rejects_oversized_body
test_request_size_limit_accepts_normal_body
```

---

### Sprint 13 — Acceptance Gate

- [ ] `pytest tests/test_sprint13_*.py` — 0 failed
- [ ] `pytest tests/test_sprint7_dps_fiscal_server.py` — 0 failed (regression)
- [ ] `pytest tests/test_gate3f_sidecar_provider.py tests/test_gate3g_sidecar_hardening.py` — 0 failed
- [ ] `docker compose up --build` — сервіс стартує без помилок
- [ ] Перевірити: `transports/stubs.py` — stub `DpsXmlUnifiedWindowTransportStub` можна прибрати або залишити для dev profile

---

---

## Sprint 14 — Operational Safety + Pilot

---

### Крок 14.1 — SQLite backup job `[IMPL][NEW]`

**Файли:**
- `scripts/backup_db.py` — backup скрипт
- `src/prro_gateway/services/backup.py` — backup service (або функція в scripts)

**Що зробити:**
1. `sqlite3.Connection.backup(target)` — атомарний hot backup.
2. Зберігати в `var/backups/prro_{fiscal_number}_{timestamp}.db`.
3. Після backup: відкрити copy, запустити `PRAGMA integrity_check` → якщо fail, log + raise.
4. Rotation: зберігати N останніх копій (config: `backup.keep_count = 7`).
5. На corruption detect: оновити `node_state.mode = NodeMode.STOP_MODE`, видати health сигнал.

**Тест:** `tests/test_sprint14_backup.py`
```
test_backup_creates_valid_sqlite_copy
test_backup_integrity_check_passes
test_backup_rotation_keeps_n_copies
test_backup_corruption_triggers_stop_mode         # corrupt source → STOP_MODE
test_backup_stop_mode_visible_in_health_endpoint
```

---

### Крок 14.2 — Retention / purge policy `[IMPL][DB]`

**Файли:**
- `sql/011_retention_config.sql` — нова таблиця або column у config `[DB]`
- `scripts/purge_old_records.py`
- `src/prro_gateway/services/retention.py`

**Що зробити:**
1. Config keys: `retention.audit_log_days`, `retention.trace_days`, `retention.closed_document_days` (default: 0 = off).
2. Purge query для кожної таблиці: DELETE WHERE `created_at < NOW - TTL` AND (status є terminal).
3. Логувати кількість видалених рядків.
4. Fiscal documents ніколи не видаляти — тільки audit/trace/inbox entries.

**Тест:** `tests/test_sprint14_retention.py`
```
test_purge_audit_log_deletes_old_entries
test_purge_trace_deletes_old_entries
test_purge_does_not_delete_fiscal_documents      # fiscal docs immune
test_purge_disabled_by_default                  # TTL=0 → nothing deleted
test_purge_respects_terminal_state_only         # active docs immune
```

**Risk `[DB]`:** Додавання нових таблиць або columns — migration-keeper має перевірити checksum і idempotency.

---

### Крок 14.3 — Pytest marker taxonomy `[TEST]`

**Файли:**
- `pyproject.toml` — registered markers
- кожен тест-файл — `@pytest.mark.unit` / `@pytest.mark.integration` / `@pytest.mark.e2e`

**Маппінг:**

| Маркер | Критерій | Приклади |
|---|---|---|
| `unit` | Без DB, без network, без I/O | `test_models.py`, `test_sprint9_full_e_element.py`, `_calc_tax` |
| `integration` | In-memory SQLite, stub transport/crypto | Більшість `test_gate*.py`, `test_write_path.py` |
| `e2e` | Full worker + DB + реальний або mock network | `test_e2e_lifecycle.py`, `test_pilot_smoke.py` |

**Що зробити:**
1. Зареєструвати markers у `pyproject.toml`:
   ```toml
   [tool.pytest.ini_options]
   markers = ["unit", "integration", "e2e"]
   ```
2. Пройтися по тест-файлах і додати декоратори. Починати з нових файлів (Sprint 11-14), потім існуючі.
3. Перевірити: `pytest -m unit` не потребує DB fixture.

---

### Крок 14.4 — Operational documentation `[NEW]`

**Файли для створення:**

| Файл | Мінімальний зміст |
|---|---|
| `docs/PROTOCOL_SHAPE_AUDIT.md` | Maria/WebCheck session structure, command mapping, known deviations |
| `docs/DPS_TRANSPORT.md` | DPS_PRRO_FISCAL_SERVER profile, gRPC contract, error codes, live proof dates |
| `docs/OFFLINE_SYNC.md` | Offline state machine diagram, sync flow, limits, errors, recovery |
| `docs/ARCHIVE_POLICY.md` | `document_files` layout, retention rules, integrity check commands |

**Підхід:** Кожен файл = 1-2 сторінки. Актуалізувати з коду, не копіювати spec. Описувати поточну поведінку + відомі deviations.

---

### Крок 14.5 — Pilot acceptance matrix run `[TEST]`

Запустити повний `§10 Acceptance Test Matrix` з `PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md` вручну або автоматизовано.

Мінімальний checklist:
- [ ] open shift online
- [ ] sell online (cash + card)
- [ ] return online
- [ ] service in/out
- [ ] cash withdrawal
- [ ] close shift / Z-report
- [ ] idempotent replay of sale
- [ ] channel switch forbidden during shift
- [ ] offline range allocation (one number once)
- [ ] offline 36h limit enforced
- [ ] offline sync sends in order
- [ ] DPS PRRO fiscal server E2E (live or mock)
- [ ] DPS Unified Window E2E (mock)
- [ ] reconciliation preserves channel

---

### Sprint 14 — Acceptance Gate

- [ ] `pytest tests/test_sprint14_*.py` — 0 failed
- [ ] `pytest -m unit` — 0 failed, no DB fixture required
- [ ] `pytest -m integration` — 0 failed
- [ ] `pytest -m e2e` — 0 failed
- [ ] `pytest -q` — 0 failed (full suite)
- [ ] Всі 4 operational docs створені
- [ ] Pilot acceptance matrix checklist повністю зелений

---

---

## Hygiene (continuous)

Виконувати у будь-якому спринті за нагоди:

| ID | Файл | Дія |
|---|---|---|
| `DPS-TYPING-01` | `src/prro_gateway/transports/dps_fiscal_server.py` | Змінити `signed_payload: str` → `bytes` у DPS transport send path |
| `DPS-STATUSRRO-POST-01` | `tests/test_sprint7_dps_probe.py` | Позначити тест як `xfail` або видалити якщо JKS не в репо назавжди |

---

## Залежності між Sprint-ами

```
Sprint 11 (Offline) ─────────────────────────────────────────────┐
                                                                   ↓
Sprint 12 (Fiscal) ──── незалежний, паралельно з 11 ────────────→ Sprint 14 (Pilot)
                                                                   ↑
Sprint 13 (Infra)  ──── незалежний, паралельно з 11/12 ──────────┘
```

Sprint 14 чекає на всі три попередні (pilot checklist вимагає повної системи).

Sprint 12 і Sprint 13 не мають залежності між собою — можуть іти паралельно.

---

*Документ оновлювати після кожного завершеного кроку.*
