# Runtime Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Устранить crash window в write path (SENDING state), исправить crypto executor lifecycle, освободить event loop, починить migration splitter, добавить MARIA_304_NATIVE total_sum guard.

**Architecture:** 7 точечных задач в 4 файлах + 1 новый SQL-файл. Задачи 1–3 связаны (state machine), остальные независимы. Минимальный diff, существующие паттерны сохраняются.

**Tech Stack:** Python 3.11+, FastAPI/Starlette, SQLite WAL, concurrent.futures, pytest

---

## File map

| Файл | Задачи |
|---|---|
| `src/prro_gateway/enums.py` | Task 1 |
| `sql/024_sending_state.sql` | Task 1 |
| `src/prro_gateway/services/write_path.py` | Tasks 2, 3, 4, 7 |
| `src/prro_gateway/runtime/rest_app.py` | Task 5 |
| `src/prro_gateway/migrations/runner.py` | Task 6 |

---

### Task 1: B-1a — DocumentState.SENDING + migration 024

**Goal:** Добавить `SENDING` в enum `DocumentState` и в CHECK constraint `fiscal_documents.state` через SQL migration 024.

**Files:**
- Modify: `src/prro_gateway/enums.py:44-56`
- Create: `sql/024_sending_state.sql`

**Acceptance Criteria:**
- [ ] `DocumentState.SENDING = "SENDING"` существует между `ENCRYPTED` и `SENT`
- [ ] migration 024 содержит `'SENDING'` в CHECK constraint `fiscal_documents.state`
- [ ] migration 024 применяется без ошибок через `apply_migrations`
- [ ] все существующие миграции 001–023 применяются до 024 без ошибок

**Verify:** `pytest tests/test_migration_runner.py -v 2>&1 | tail -10`

**Steps:**

- [ ] **Step 1: Написать тест — migration 024 применяется и state принимает SENDING**

```python
# tests/test_migration_runner.py — добавить в конец файла

def test_migration_024_sending_state_in_constraint(tmp_path):
    db_path = tmp_path / "test.db"
    apply_migrations(db_path, ROOT / "sql")
    conn = sqlite3.connect(db_path)
    try:
        # Проверить что 024 применена
        count = conn.execute(
            "SELECT COUNT(*) FROM schema_migrations WHERE migration_name = '024_sending_state.sql'"
        ).fetchone()[0]
        assert count == 1

        # Проверить что 'SENDING' принимается в CHECK constraint
        conn.execute("""
            INSERT INTO fiscal_documents (
                document_id, request_id, fiscal_number, lnd, doc_type, state,
                backend_profile_id, transport_profile_id, fs_mode, business_ts,
                payload_json, payload_sha256
            ) VALUES (
                'test-sending', 'req-sending', 'FN-TEST', 1, 'SELL', 'SENDING',
                'backend_checkbox_default', 'transport_checkbox_rest_default',
                'ONLINE', '2026-01-01T10:00:00+00:00',
                '{}', 'sha256-test'
            )
        """)
        conn.rollback()  # cleanup
    finally:
        conn.close()
```

- [ ] **Step 2: Запустить тест — убедиться что падает (migration 024 не существует)**

```bash
cd /mnt/d/prro_gate && pytest tests/test_migration_runner.py::test_migration_024_sending_state_in_constraint -v
```

Expected: `FAILED` — `024_sending_state.sql` не существует.

- [ ] **Step 3: Добавить `SENDING` в `DocumentState` enum**

В `src/prro_gateway/enums.py` изменить класс `DocumentState`:

```python
class DocumentState(StrEnum):
    PREPARED = "PREPARED"
    SIGNED = "SIGNED"
    ENCRYPTED = "ENCRYPTED"
    SENDING = "SENDING"
    SENT = "SENT"
    KVT1 = "KVT1"
    KVT2 = "KVT2"
    ACK = "ACK"
    OFFLINE_LOCAL_ACK = "OFFLINE_LOCAL_ACK"
    REJECTED = "REJECTED"
    CANCELLED = "CANCELLED"
    ERROR_RETRYABLE = "ERROR_RETRYABLE"
    REQUIRES_MANUAL_RECONCILIATION = "REQUIRES_MANUAL_RECONCILIATION"
```

- [ ] **Step 4: Создать `sql/024_sending_state.sql`**

Следует паттерну migration 003 (table recreation для изменения CHECK constraint). Добавляет `'SENDING'` и сохраняет `z_report_number` из migration 005.

```sql
-- Migration 024: Add SENDING to fiscal_documents.state CHECK constraint.
--
-- Background: new intermediate state marking that transport_client.send()
-- was called but SENT was not yet committed to the database.
-- On crash-resume, SENDING → ERROR_RETRYABLE (reconciliation checks DPS).
--
-- SQLite does not support ALTER COLUMN to modify CHECK constraints.
-- This migration recreates fiscal_documents with the updated constraint.
-- No data migration required — no rows in SENDING state can exist before this migration.

PRAGMA foreign_keys = OFF;

BEGIN;

-- NOTE: DDL must remain in sync with the current fiscal_documents schema
-- (001 + 003 + 005 cumulative). Any future migration that alters
-- fiscal_documents must also update this block.
CREATE TABLE fiscal_documents_new (
    document_id                     TEXT PRIMARY KEY,
    request_id                      TEXT NOT NULL UNIQUE,
    fiscal_number                   TEXT NOT NULL,
    shift_id                        TEXT,
    offline_session_id              TEXT,
    lnd                             INTEGER NOT NULL CHECK (lnd >= 1),
    doc_type                        TEXT NOT NULL CHECK (doc_type IN (
        'SHIFT_OPEN','SHIFT_CLOSE','SELL','RETURN','SERVICE_IN','SERVICE_OUT',
        'CASH_WITHDRAWAL','X_REPORT','Z_REPORT','OFFLINE_BEGIN','OFFLINE_END',
        'ASK_OFFLINE_CODES','STATUS'
    )),
    state                           TEXT NOT NULL CHECK (state IN (
        'PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ACK',
        'OFFLINE_LOCAL_ACK',
        'REJECTED','CANCELLED','ERROR_RETRYABLE','REQUIRES_MANUAL_RECONCILIATION'
    )),
    backend_profile_id              TEXT NOT NULL,
    transport_profile_id            TEXT NOT NULL,
    fs_mode                         TEXT NOT NULL CHECK (fs_mode IN ('ONLINE','OFFLINE')),
    receipt_type                    TEXT,
    business_ts                     TEXT NOT NULL,
    offline_fiscal_no               INTEGER,
    offline_fiscal_date             TEXT,
    server_fiscal_no                TEXT,
    server_fiscal_date              TEXT,
    serial                          TEXT,
    control_number                  TEXT,
    previous_hash                   TEXT,
    related_receipt_id              TEXT,
    previous_receipt_id             TEXT,
    technical_return                INTEGER CHECK (technical_return IN (0,1)),
    delivery_json                   TEXT,
    rounding_enabled                INTEGER CHECK (rounding_enabled IN (0,1)),
    channel_lock_ref                TEXT,
    total_sum                       INTEGER,
    round_sum                       INTEGER,
    discounts_sum                   INTEGER,
    extra_charge_sum                INTEGER,
    payload_json                    TEXT NOT NULL,
    payload_sha256                  TEXT NOT NULL,
    response_json                   TEXT,
    transport_request_id            TEXT,
    submission_status               TEXT,
    kvt1_received_at                TEXT,
    kvt2_received_at                TEXT,
    sent_at                         TEXT,
    ack_at                          TEXT,
    canonical_error_code            TEXT,
    error_message                   TEXT,
    recovery_attempts               INTEGER NOT NULL DEFAULT 0 CHECK (recovery_attempts >= 0),
    created_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    z_report_number                 INTEGER,
    FOREIGN KEY (request_id) REFERENCES ingress_inbox(request_id),
    FOREIGN KEY (shift_id) REFERENCES shifts(shift_id),
    FOREIGN KEY (offline_session_id) REFERENCES offline_sessions(offline_session_id),
    FOREIGN KEY (backend_profile_id) REFERENCES backend_profiles(backend_profile_id),
    FOREIGN KEY (transport_profile_id) REFERENCES transport_profiles(transport_profile_id)
);

INSERT INTO fiscal_documents_new SELECT * FROM fiscal_documents;

DROP TABLE fiscal_documents;

ALTER TABLE fiscal_documents_new RENAME TO fiscal_documents;

CREATE UNIQUE INDEX uq_fiscal_documents_lnd
ON fiscal_documents(lnd);

CREATE UNIQUE INDEX uq_fiscal_documents_offline_no
ON fiscal_documents(offline_fiscal_no)
WHERE offline_fiscal_no IS NOT NULL;

CREATE INDEX idx_fiscal_documents_state
ON fiscal_documents(fiscal_number, state, created_at);

CREATE INDEX idx_fiscal_documents_previous_hash
ON fiscal_documents(previous_hash);

CREATE INDEX idx_fiscal_documents_shift
ON fiscal_documents(shift_id, created_at);

COMMIT;

PRAGMA foreign_keys = ON;
```

- [ ] **Step 5: Запустить тесты**

```bash
cd /mnt/d/prro_gate && pytest tests/test_migration_runner.py -v 2>&1 | tail -15
```

Expected: все тесты PASSED.

- [ ] **Step 6: Commit**

```bash
git add src/prro_gateway/enums.py sql/024_sending_state.sql tests/test_migration_runner.py
git commit -m "feat(write_path): add DocumentState.SENDING + migration 024"
```

---

### Task 2: B-1b — SENDING state в write_path

**Goal:** Записывать `SENDING` в БД перед вызовом транспорта; при crash-resume переводить `SENDING → ERROR_RETRYABLE`.

**Files:**
- Modify: `src/prro_gateway/services/write_path.py`

**Acceptance Criteria:**
- [ ] В `_stage_send_or_offline`: `BEGIN IMMEDIATE` → `update_state(SENDING)` → `commit()` — до `transport_client.send()`
- [ ] В `process_next`: `ctx.document.state == SENDING` → `_mark_document_and_inbox_error(ERROR_RETRYABLE)` → return (transport не вызывается)
- [ ] Transport exceptions (Retryable, Rejected, RateLimited) корректно переводят из SENDING
- [ ] Тест: нормальный путь — промежуточное состояние SENDING → финальное SENT
- [ ] Тест: crash-resume — документ в SENDING → после process_next становится ERROR_RETRYABLE

**Verify:** `pytest tests/test_write_path.py -v -k "sending" 2>&1 | tail -20`

**Steps:**

- [ ] **Step 1: Написать тесты**

```python
# tests/test_write_path.py — добавить в конец файла

def test_document_transitions_through_sending_to_sent(conn):
    """Normal path: SENDING written before transport, SENT after success."""
    conn.execute('BEGIN IMMEDIATE')
    _open_shift(conn)
    _accept_sell_command(conn)
    conn.commit()

    sent_states: list[str] = []
    original_send = StubTransportClient.send

    def tracking_send(self, *, document_id, signed_payload, fiscal_number,
                      backend_profile_id, transport_profile_id, **kwargs):
        # At the moment transport is called, document must be SENDING in DB.
        row = conn.execute(
            "SELECT state FROM fiscal_documents WHERE fiscal_number = 'FN-DEV-0001'"
        ).fetchone()
        if row:
            sent_states.append(row[0])
        return original_send(self, document_id=document_id, signed_payload=signed_payload,
                              fiscal_number=fiscal_number, backend_profile_id=backend_profile_id,
                              transport_profile_id=transport_profile_id, **kwargs)

    transport = StubTransportClient()
    transport.send = tracking_send.__get__(transport, StubTransportClient)  # type: ignore[method-assign]

    worker = WritePathWorker(
        crypto_provider=StubCryptoProvider(),
        transport_client=transport,
    )
    worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert 'SENDING' in sent_states, "SENDING must be persisted before transport call"
    row = conn.execute(
        "SELECT state FROM fiscal_documents WHERE fiscal_number = 'FN-DEV-0001'"
    ).fetchone()
    assert row is not None
    # Final state should not be SENDING
    assert row[0] != 'SENDING'


def test_crash_resume_sending_becomes_error_retryable(conn):
    """Crash-resume: document in SENDING → process_next → ERROR_RETRYABLE, transport NOT called."""
    conn.execute('BEGIN IMMEDIATE')
    _open_shift(conn)
    _accept_sell_command(conn)
    conn.commit()

    # Simulate crash: manually put document in SENDING state
    worker = WritePathWorker(
        crypto_provider=StubCryptoProvider(),
        transport_client=StubTransportClient(),
    )
    # First call to create the document (will go through to SENT normally)
    worker.process_next(conn, fiscal_number='FN-DEV-0001')

    # Manually reset to SENDING to simulate crash after SENDING was written
    conn.execute(
        "UPDATE fiscal_documents SET state = 'SENDING' WHERE fiscal_number = 'FN-DEV-0001'"
    )
    conn.execute(
        "UPDATE ingress_inbox SET status = 'PROCESSING' WHERE fiscal_number = 'FN-DEV-0001'"
    )
    conn.commit()

    transport = StubTransportClient()
    worker2 = WritePathWorker(
        crypto_provider=StubCryptoProvider(),
        transport_client=transport,
    )
    worker2.process_next(conn, fiscal_number='FN-DEV-0001')

    assert transport.calls == 0, "transport must NOT be called on SENDING crash-resume"
    row = conn.execute(
        "SELECT state FROM fiscal_documents WHERE fiscal_number = 'FN-DEV-0001'"
    ).fetchone()
    assert row is not None
    assert row[0] == 'ERROR_RETRYABLE'
```

- [ ] **Step 2: Запустить тесты — убедиться что падают**

```bash
cd /mnt/d/prro_gate && pytest tests/test_write_path.py -v -k "sending" 2>&1 | tail -10
```

Expected: FAILED.

- [ ] **Step 3: Добавить `SENDING` в crash-resume ветку `process_next`**

В `src/prro_gateway/services/write_path.py` найти блок (строки ~143–155):

```python
        assert ctx.inbox is not None and ctx.command is not None and ctx.document is not None
        if ctx.document.state in {DocumentState.SIGNED, DocumentState.ENCRYPTED}:
            # Crash-resume: signature already persisted — skip _stage_sign entirely.
            self.logger.info("stage_sign_skipped_resume", extra={"extra_fields": {"document_id": ctx.document.document_id, "state": ctx.document.state}})
            _t2 = _t1
        elif self._requires_local_sign(conn, ctx):
```

Добавить ветку для `SENDING` **перед** этим блоком:

```python
        assert ctx.inbox is not None and ctx.command is not None and ctx.document is not None
        if ctx.document.state == DocumentState.SENDING:
            # Crash-resume: process crashed after SENDING was persisted but before SENT
            # was committed. Transport outcome unknown — do not retry blindly.
            # Transition to ERROR_RETRYABLE so reconciliation can check DPS status.
            self.logger.warning("stage_send_crash_resume_sending", extra={"extra_fields": {
                "document_id": ctx.document.document_id,
                "fiscal_number": ctx.fiscal_number,
            }})
            return self._mark_document_and_inbox_error(
                conn,
                ctx=ctx,
                document_id=ctx.document.document_id,
                state=DocumentState.ERROR_RETRYABLE,
                error=build_canonical_error(
                    CanonicalErrorCode.TRANSPORT_RETRYABLE_ERROR,
                    message='crash-resume: SENDING state detected, transport outcome unknown',
                ),
                technical_status='CRASH_RESUME',
            )
        if ctx.document.state in {DocumentState.SIGNED, DocumentState.ENCRYPTED}:
            # Crash-resume: signature already persisted — skip _stage_sign entirely.
            self.logger.info("stage_sign_skipped_resume", extra={"extra_fields": {"document_id": ctx.document.document_id, "state": ctx.document.state}})
            _t2 = _t1
        elif self._requires_local_sign(conn, ctx):
```

- [ ] **Step 4: Добавить запись SENDING в `_stage_send_or_offline` перед transport call**

В `_stage_send_or_offline` найти строки (~740–757):

```python
        transport_profile = TransportProfileRepository.get_by_id(conn, ctx.document.transport_profile_id)
        ctx.transport_profile = transport_profile
        try:
            send_result = self.transport_client.send(
```

Добавить блок записи SENDING **перед** `try`:

```python
        transport_profile = TransportProfileRepository.get_by_id(conn, ctx.document.transport_profile_id)
        ctx.transport_profile = transport_profile

        # Durable send-attempt marker: written before transport call.
        # On crash-resume, SENDING → ERROR_RETRYABLE (see process_next).
        conn.execute('BEGIN IMMEDIATE')
        ctx.document = FiscalDocumentRepository.update_state(
            conn,
            document_id=ctx.document.document_id,
            state=DocumentState.SENDING,
        )
        conn.commit()

        try:
            send_result = self.transport_client.send(
```

- [ ] **Step 5: Запустить тесты write_path**

```bash
cd /mnt/d/prro_gate && pytest tests/test_write_path.py tests/test_write_path_sidecar.py tests/test_sprint12_write_path_gaps.py -x 2>&1 | tail -20
```

Expected: все тесты PASSED включая новые `test_document_transitions_through_sending_to_sent` и `test_crash_resume_sending_becomes_error_retryable`.

- [ ] **Step 6: Commit**

```bash
git add src/prro_gateway/services/write_path.py tests/test_write_path.py
git commit -m "feat(write_path): SENDING state before transport + crash-resume handling"
```

---

### Task 3: B-2 — State validation перед BEGIN в _stage_finalize_ack

**Goal:** Перенести `DocumentState(ctx.send_result.state)` до `BEGIN IMMEDIATE` в `_stage_finalize_ack` — невалидный state не должен бросать исключение внутри транзакции.

**Files:**
- Modify: `src/prro_gateway/services/write_path.py:_stage_finalize_ack`

**Acceptance Criteria:**
- [ ] `DocumentState(ctx.send_result.state)` вызывается до `conn.execute('BEGIN IMMEDIATE')`
- [ ] При `ValueError` возвращается error result без открытия транзакции
- [ ] Тест: невалидный `send_result.state` → `ERROR_RETRYABLE`, транзакция не открыта

**Verify:** `pytest tests/test_write_path.py -v -k "invalid_send_state" 2>&1 | tail -10`

**Steps:**

- [ ] **Step 1: Написать тест**

```python
# tests/test_write_path.py — добавить в конец файла

def test_invalid_send_result_state_returns_error_before_transaction(conn):
    """send_result.state with invalid value → ERROR_RETRYABLE, no open transaction."""
    from unittest.mock import patch
    from prro_gateway.ports import SendResult
    from datetime import UTC, datetime

    conn.execute('BEGIN IMMEDIATE')
    _open_shift(conn)
    _accept_sell_command(conn)
    conn.commit()

    bad_result = SendResult(
        transport_request_id='tx-bad',
        submission_status='SENT',
        server_fiscal_no='F-1',
        server_fiscal_date='2026-01-01T10:00:00+00:00',
        response_json='{}',
        sent_at=datetime.now(UTC),
        ack_at=datetime.now(UTC),
        state='NOT_A_VALID_STATE',
    )

    class BadStateTransport(StubTransportClient):
        def send(self, **kwargs):
            self.calls += 1
            return bad_result

    worker = WritePathWorker(
        crypto_provider=StubCryptoProvider(),
        transport_client=BadStateTransport(),
    )
    worker.process_next(conn, fiscal_number='FN-DEV-0001')

    assert not conn.in_transaction, "no open transaction after invalid state"
    row = conn.execute(
        "SELECT state FROM fiscal_documents WHERE fiscal_number = 'FN-DEV-0001'"
    ).fetchone()
    assert row is not None
    assert row[0] == 'ERROR_RETRYABLE'
```

- [ ] **Step 2: Запустить тест — убедиться что падает**

```bash
cd /mnt/d/prro_gate && pytest tests/test_write_path.py::test_invalid_send_result_state_returns_error_before_transaction -v 2>&1 | tail -10
```

Expected: FAILED (currently ValueError inside transaction).

- [ ] **Step 3: Добавить pre-validation в `_stage_finalize_ack`**

В `_stage_finalize_ack` (строки ~970–972) найти:

```python
    def _stage_finalize_ack(self, conn: sqlite3.Connection, ctx: WorkerContext) -> WorkerProcessResult:
        assert ctx.document is not None and ctx.inbox is not None and ctx.command is not None
        conn.execute('BEGIN IMMEDIATE')
```

Добавить валидацию **перед** `conn.execute('BEGIN IMMEDIATE')`:

```python
    def _stage_finalize_ack(self, conn: sqlite3.Connection, ctx: WorkerContext) -> WorkerProcessResult:
        assert ctx.document is not None and ctx.inbox is not None and ctx.command is not None
        if ctx.document.fs_mode != 'OFFLINE' and ctx.send_result is not None and ctx.send_result.state:
            try:
                DocumentState(ctx.send_result.state)
            except ValueError:
                return self._mark_document_and_inbox_error(
                    conn,
                    ctx=ctx,
                    document_id=ctx.document.document_id,
                    state=DocumentState.ERROR_RETRYABLE,
                    error=build_canonical_error(
                        CanonicalErrorCode.TRANSPORT_RETRYABLE_ERROR,
                        message=f'invalid send_result.state: {ctx.send_result.state!r}',
                    ),
                    technical_status='INVALID_SEND_STATE',
                )
        conn.execute('BEGIN IMMEDIATE')
```

- [ ] **Step 4: Запустить тесты**

```bash
cd /mnt/d/prro_gate && pytest tests/test_write_path.py tests/test_write_path_sidecar.py -x 2>&1 | tail -15
```

Expected: PASSED.

- [ ] **Step 5: Commit**

```bash
git add src/prro_gateway/services/write_path.py tests/test_write_path.py
git commit -m "fix(write_path): validate send_result.state before BEGIN IMMEDIATE in finalize"
```

---

### Task 4: C-1 — Executor abandonment при crypto timeout

**Goal:** Заменить `with ThreadPoolExecutor` на явный `wait + shutdown(wait=False)` — при таймауте тред abandonment без блокировки request.

**Files:**
- Modify: `src/prro_gateway/services/write_path.py:_stage_sign`

**Acceptance Criteria:**
- [ ] Оба блока (sign_raw и sign) используют `wait() + shutdown(wait=False)` вместо `with ThreadPoolExecutor`
- [ ] При таймауте `CryptoProviderUnavailableError` бросается немедленно, без ожидания треда
- [ ] Тест: mock-провайдер с искусственной задержкой > timeout → ошибка возвращается до истечения задержки

**Verify:** `pytest tests/test_write_path.py -v -k "timeout" 2>&1 | tail -15`

**Steps:**

- [ ] **Step 1: Написать тест**

```python
# tests/test_write_path.py — добавить в конец файла

import time as _time_module

def test_crypto_timeout_does_not_block_on_hanging_thread(conn):
    """Crypto timeout must return immediately, not block waiting for the thread."""
    import threading

    thread_started = threading.Event()

    class SlowCryptoProvider:
        def sign(self, *, document_id: str, payload_json: str) -> str:
            thread_started.set()
            _time_module.sleep(10)  # Simulate hung crypto
            return f'signed::{document_id}'

    conn.execute('BEGIN IMMEDIATE')
    _open_shift(conn)
    _accept_sell_command(conn)
    conn.commit()

    worker = WritePathWorker(
        crypto_provider=SlowCryptoProvider(),
        transport_client=StubTransportClient(),
        crypto_timeout_seconds=0.1,
    )
    t0 = _time_module.monotonic()
    worker.process_next(conn, fiscal_number='FN-DEV-0001')
    elapsed = _time_module.monotonic() - t0

    assert elapsed < 1.0, f"process_next took {elapsed:.2f}s — executor.shutdown(wait=True) likely blocking"
    row = conn.execute(
        "SELECT state FROM fiscal_documents WHERE fiscal_number = 'FN-DEV-0001'"
    ).fetchone()
    assert row is not None
    assert row[0] == 'ERROR_RETRYABLE'
```

- [ ] **Step 2: Запустить тест — убедиться что падает (зависает или timeout)**

```bash
cd /mnt/d/prro_gate && pytest tests/test_write_path.py::test_crypto_timeout_does_not_block_on_hanging_thread -v --timeout=15 2>&1 | tail -10
```

Expected: либо FAILED (elapsed > 1.0), либо timeout через 15s.

- [ ] **Step 3: Заменить `with ThreadPoolExecutor` в блоке sign_raw (~строки 609–617)**

Найти в `_stage_sign`:

```python
                if self.crypto_timeout_seconds is not None:
                    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as _executor:
                        _future = _executor.submit(self.crypto_provider.sign_raw, data=sign_data, document_id=_doc_id)
                        try:
                            signed_payload = _future.result(timeout=self.crypto_timeout_seconds)
                        except concurrent.futures.TimeoutError:
                            raise CryptoProviderUnavailableError(
                                f'crypto sign_raw() timed out after {self.crypto_timeout_seconds}s'
                            )
```

Заменить на:

```python
                if self.crypto_timeout_seconds is not None:
                    _executor = concurrent.futures.ThreadPoolExecutor(max_workers=1)
                    _future = _executor.submit(self.crypto_provider.sign_raw, data=sign_data, document_id=_doc_id)
                    _done, _ = concurrent.futures.wait([_future], timeout=self.crypto_timeout_seconds)
                    if not _done:
                        _executor.shutdown(wait=False)
                        raise CryptoProviderUnavailableError(
                            f'crypto sign_raw() timed out after {self.crypto_timeout_seconds}s'
                        )
                    signed_payload = _future.result()
                    _executor.shutdown(wait=False)
```

- [ ] **Step 4: Заменить `with ThreadPoolExecutor` в блоке sign (~строки 622–634)**

Найти:

```python
                if self.crypto_timeout_seconds is not None:
                    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as _executor:
                        _future = _executor.submit(
                            self.crypto_provider.sign,
                            document_id=ctx.document.document_id,
                            payload_json=sign_input,
                        )
                        try:
                            signed_payload = _future.result(timeout=self.crypto_timeout_seconds)
                        except concurrent.futures.TimeoutError:
                            raise CryptoProviderUnavailableError(
                                f'crypto sign() timed out after {self.crypto_timeout_seconds}s'
                            )
```

Заменить на:

```python
                if self.crypto_timeout_seconds is not None:
                    _executor = concurrent.futures.ThreadPoolExecutor(max_workers=1)
                    _future = _executor.submit(
                        self.crypto_provider.sign,
                        document_id=ctx.document.document_id,
                        payload_json=sign_input,
                    )
                    _done, _ = concurrent.futures.wait([_future], timeout=self.crypto_timeout_seconds)
                    if not _done:
                        _executor.shutdown(wait=False)
                        raise CryptoProviderUnavailableError(
                            f'crypto sign() timed out after {self.crypto_timeout_seconds}s'
                        )
                    signed_payload = _future.result()
                    _executor.shutdown(wait=False)
```

- [ ] **Step 5: Запустить тесты**

```bash
cd /mnt/d/prro_gate && pytest tests/test_write_path.py tests/test_write_path_sidecar.py -x 2>&1 | tail -15
```

Expected: PASSED.

- [ ] **Step 6: Commit**

```bash
git add src/prro_gateway/services/write_path.py tests/test_write_path.py
git commit -m "fix(write_path): crypto executor shutdown(wait=False) on timeout"
```

---

### Task 5: C-2 — run_in_threadpool для async endpoints

**Goal:** Синхронные вызовы в `ingress_checkbox`, `admin_reconciliation_trigger`, `admin_offline_sync` вынести в `run_in_threadpool` — освободить event loop.

**Files:**
- Modify: `src/prro_gateway/runtime/rest_app.py`

**Acceptance Criteria:**
- [ ] `from starlette.concurrency import run_in_threadpool` добавлен в импорты
- [ ] `accept_checkbox` вызов обёрнут в `run_in_threadpool` через inner function
- [ ] `reconcile_pending` вызов обёрнут в `run_in_threadpool`
- [ ] `sync_pending` вызов обёрнут в `run_in_threadpool`
- [ ] Все три endpoint'а возвращают те же response bodies и status codes что и раньше

**Verify:** `pytest tests/ -x -q 2>&1 | tail -10`

**Steps:**

- [ ] **Step 1: Добавить импорт `run_in_threadpool` в `rest_app.py`**

Найти блок импортов в `src/prro_gateway/runtime/rest_app.py` (в функции `build_rest_app` или вверху файла). Добавить:

```python
from starlette.concurrency import run_in_threadpool
```

- [ ] **Step 2: Обернуть sync-часть `ingress_checkbox`**

Найти в `ingress_checkbox` (~строки 132–137):

```python
        try:
            with container.connect() as conn:
                inbox, command, process_result, is_replay = container.ingress_service.accept_checkbox(
                    conn,
                    raw_request=raw,
                    response_timeout_seconds=container.config.ingress.rest.response_timeout_seconds,
                )
```

Заменить на:

```python
        try:
            def _sync_accept():
                with container.connect() as conn:
                    return container.ingress_service.accept_checkbox(
                        conn,
                        raw_request=raw,
                        response_timeout_seconds=container.config.ingress.rest.response_timeout_seconds,
                    )
            inbox, command, process_result, is_replay = await run_in_threadpool(_sync_accept)
```

- [ ] **Step 3: Обернуть sync-часть `admin_reconciliation_trigger`**

Найти (~строки 617–621):

```python
        try:
            with container.connect() as conn:
                result = container.reconciliation_service.reconcile_pending(
                    conn, fiscal_number=fiscal_number
                )
```

Заменить на:

```python
        try:
            def _sync_reconcile():
                with container.connect() as conn:
                    return container.reconciliation_service.reconcile_pending(
                        conn, fiscal_number=fiscal_number
                    )
            result = await run_in_threadpool(_sync_reconcile)
```

- [ ] **Step 4: Обернуть sync-часть `admin_offline_sync`**

Найти (~строки 664–666):

```python
        try:
            with container.connect() as conn:
                result = container.offline_sync_service.sync_pending(conn, fiscal_number=fiscal_number)
```

Заменить на:

```python
        try:
            def _sync_offline():
                with container.connect() as conn:
                    return container.offline_sync_service.sync_pending(conn, fiscal_number=fiscal_number)
            result = await run_in_threadpool(_sync_offline)
```

- [ ] **Step 5: Запустить полный тест-сюит**

```bash
cd /mnt/d/prro_gate && pytest tests/ -x -q 2>&1 | tail -15
```

Expected: PASSED (без регрессий).

- [ ] **Step 6: Commit**

```bash
git add src/prro_gateway/runtime/rest_app.py
git commit -m "fix(rest_app): run_in_threadpool for checkbox ingress and admin endpoints"
```

---

### Task 6: D-1 — Migration splitter fix

**Goal:** Заменить regex-сплиттер в `runner.py` на `sqlite3.complete_statement()` — корректная обработка строковых литералов, триггеров и сложных DDL.

**Files:**
- Modify: `src/prro_gateway/migrations/runner.py`

**Acceptance Criteria:**
- [ ] `_split_sql_statements` использует `sqlite3.complete_statement()` вместо `sql.split(';')`
- [ ] `import re` удалён (больше не нужен)
- [ ] Транзакционная логика (`BEGIN IMMEDIATE` + `schema_migrations` insert) сохранена без изменений
- [ ] Тест: SQL с `;` внутри строкового литерала корректно разбивается на один statement
- [ ] Все существующие миграции 001–024 применяются без ошибок

**Verify:** `pytest tests/test_migration_runner.py tests/test_gate4c_migration_transaction_safety.py tests/test_gate4d_migration_checksum_mismatch.py -v 2>&1 | tail -15`

**Steps:**

- [ ] **Step 1: Написать тест**

```python
# tests/test_migration_runner.py — добавить в конец файла

from prro_gateway.migrations.runner import _split_sql_statements


def test_split_sql_handles_semicolon_in_string_literal():
    """_split_sql_statements must not split on ; inside string literals."""
    sql = "INSERT INTO t (col) VALUES ('a;b;c');"
    stmts = _split_sql_statements(sql)
    assert len(stmts) == 1
    assert "a;b;c" in stmts[0]


def test_split_sql_handles_multiple_statements():
    sql = "CREATE TABLE a (id INTEGER);\nCREATE TABLE b (id INTEGER);"
    stmts = _split_sql_statements(sql)
    assert len(stmts) == 2


def test_split_sql_handles_comment_with_semicolon():
    sql = "-- comment; not a statement\nCREATE TABLE c (id INTEGER);"
    stmts = _split_sql_statements(sql)
    assert len(stmts) == 1
```

- [ ] **Step 2: Запустить тесты — убедиться что `test_split_sql_handles_semicolon_in_string_literal` падает**

```bash
cd /mnt/d/prro_gate && pytest tests/test_migration_runner.py -v -k "split_sql" 2>&1 | tail -10
```

Expected: `test_split_sql_handles_semicolon_in_string_literal` FAILS.

- [ ] **Step 3: Заменить тело `_split_sql_statements` в `runner.py`**

В `src/prro_gateway/migrations/runner.py` найти строки 33–37:

```python
def _split_sql_statements(sql_text: str) -> list[str]:
    """Strip SQL comments and split into individual statements on semicolons."""
    sql = re.sub(r'--[^\n]*', '', sql_text)
    sql = re.sub(r'/\*.*?\*/', '', sql, flags=re.DOTALL)
    return [s.strip() for s in sql.split(';') if s.strip()]
```

Заменить на:

```python
def _split_sql_statements(sql_text: str) -> list[str]:
    """Split SQL into complete statements using SQLite's own tokenizer."""
    statements = []
    buf = ''
    for char in sql_text:
        buf += char
        if sqlite3.complete_statement(buf):
            stmt = buf.strip()
            if stmt:
                statements.append(stmt)
            buf = ''
    return statements
```

- [ ] **Step 4: Удалить `import re` из `runner.py`**

Найти строку `import re` вверху файла и удалить её (больше не используется).

- [ ] **Step 5: Запустить тесты миграций**

```bash
cd /mnt/d/prro_gate && pytest tests/test_migration_runner.py tests/test_gate1j_migration_idempotency.py tests/test_gate4c_migration_transaction_safety.py tests/test_gate4d_migration_checksum_mismatch.py -v 2>&1 | tail -15
```

Expected: все тесты PASSED.

- [ ] **Step 6: Commit**

```bash
git add src/prro_gateway/migrations/runner.py tests/test_migration_runner.py
git commit -m "fix(migrations): replace regex splitter with sqlite3.complete_statement()"
```

---

### Task 7: D-2 — Maria304 total_sum guard

**Goal:** Для `MARIA_304_NATIVE` SELL/RETURN документов проверять что `total_sum > 0` — минимальный guard до реализации full rich parser.

**Files:**
- Modify: `src/prro_gateway/services/write_path.py:_stage_acquire_and_validate`

**Acceptance Criteria:**
- [ ] MARIA_304_NATIVE SELL с `total_sum = 0` → rejected с `INVALID_RECEIPT_DATA`
- [ ] MARIA_304_NATIVE SELL с `total_sum = None` → rejected
- [ ] MARIA_304_NATIVE SELL с `total_sum = 1000` → принимается
- [ ] MARIA_304_NATIVE RETURN с `total_sum = 0` → rejected
- [ ] STATUS/SHIFT_OPEN/CLOSE/X_REPORT/Z_REPORT не затронуты guard'ом

**Verify:** `pytest tests/ -v -k "maria304" 2>&1 | tail -20`

**Steps:**

- [ ] **Step 1: Написать тесты**

```python
# tests/test_write_path.py — добавить в конец файла

def _accept_maria304_command(conn: sqlite3.Connection, *, request_id: str = 'req-m304', operation_type: OperationType = OperationType.SELL, total_sum: int | None = 1000) -> None:
    cmd = CanonicalFiscalCommand(
        request_id=request_id,
        idempotency_key=f'idem-{request_id}',
        protocol=Protocol.MARIA_304_NATIVE,
        operation_type=operation_type,
        fiscal_number='FN-DEV-0001',
        route_key='main',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner='front-a',
        external_request_id=f'ext-{request_id}',
        business_ts=datetime(2026, 1, 1, 10, 0, 0, tzinfo=UTC),
        payload={
            'total_sum': total_sum,
            'receipt': {'raw_frames': []},
        },
        payload_sha256=f'sha-{request_id}',
        trace_context=TraceContext(source_ip='10.0.0.10', source_port=12000, session_id='sess-1', correlation_id=f'corr-{request_id}'),
        correlation_id=f'corr-{request_id}',
    )
    InboxRepository.accept_command(
        conn,
        request_id=request_id,
        idempotency_key=cmd.idempotency_key,
        protocol=cmd.protocol,
        operation_type=cmd.operation_type,
        fiscal_number=cmd.fiscal_number,
        backend_profile_id=cmd.backend_profile_id,
        transport_profile_id=cmd.transport_profile_id,
        channel_owner=cmd.channel_owner,
        external_request_id=cmd.external_request_id,
        protocol_session_id='proto-session-1',
        payload_json=dumps_json(cmd.model_dump(mode='json')),
        payload_sha256=cmd.payload_sha256,
    )


import pytest as _pytest

@_pytest.mark.parametrize("total_sum,should_reject", [
    (0, True),
    (None, True),
    (1000, False),
    (1, False),
])
def test_maria304_total_sum_guard(conn, total_sum, should_reject):
    """MARIA_304_NATIVE SELL with total_sum <= 0 must be rejected."""
    conn.execute('BEGIN IMMEDIATE')
    _open_shift(conn)
    _accept_maria304_command(conn, total_sum=total_sum)
    conn.commit()

    worker = WritePathWorker(
        crypto_provider=StubCryptoProvider(),
        transport_client=StubTransportClient(),
    )
    worker.process_next(conn, fiscal_number='FN-DEV-0001')

    row = conn.execute(
        "SELECT state, canonical_error_code FROM fiscal_documents WHERE fiscal_number = 'FN-DEV-0001'"
    ).fetchone()
    assert row is not None
    if should_reject:
        assert row[0] in ('REJECTED', 'ERROR_RETRYABLE'), f"expected rejection, got state={row[0]}"
        assert row[1] == 'INVALID_RECEIPT_DATA'
    else:
        assert row[0] not in ('REJECTED', 'ERROR_RETRYABLE'), f"expected acceptance, got state={row[0]}"
```

- [ ] **Step 2: Запустить тесты — убедиться что падают**

```bash
cd /mnt/d/prro_gate && pytest tests/test_write_path.py -v -k "maria304_total_sum" 2>&1 | tail -15
```

Expected: FAILED (total_sum=0 и None не отклоняются).

- [ ] **Step 3: Добавить guard в `_stage_acquire_and_validate`**

В `src/prro_gateway/services/write_path.py` найти строки около 307:

```python
        elif command.operation_type == OperationType.CASH_WITHDRAWAL:
            # MARIA_304_NATIVE adapter parses 19-param CSHG body from
            # raw_frames and synthesises cash_withdrawal_sum + CASHLESS
            # payment (see _enrich_cashwithdrawal_payload).
            from ..validators.ua_receipt import validate_cash_withdrawal_receipt
            violations = validate_cash_withdrawal_receipt(command.payload)
        if violations:
```

Добавить guard **после** блока `elif CASH_WITHDRAWAL` и **перед** `if violations:`:

```python
        if maria304_bypass and command.operation_type in {OperationType.SELL, OperationType.RETURN}:
            total = command.payload.get('total_sum') if isinstance(command.payload, dict) else None
            if not total or int(total) <= 0:
                violations = ['total_sum must be > 0 for MARIA_304_NATIVE fiscal documents']
        if violations:
```

- [ ] **Step 4: Запустить тесты**

```bash
cd /mnt/d/prro_gate && pytest tests/test_write_path.py tests/test_sprint12_write_path_gaps.py -x 2>&1 | tail -15
```

Expected: все тесты PASSED.

- [ ] **Step 5: Commit**

```bash
git add src/prro_gateway/services/write_path.py tests/test_write_path.py
git commit -m "fix(write_path): total_sum guard for MARIA_304_NATIVE SELL/RETURN"
```
