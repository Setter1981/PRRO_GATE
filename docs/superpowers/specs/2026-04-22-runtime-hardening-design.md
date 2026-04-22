# Runtime Hardening Implementation Spec

**Goal:** Устранить crash window в write path, исправить crypto executor lifecycle, освободить event loop от sync-блокировок, починить migration splitter и добавить минимальный guard для MARIA_304_NATIVE документов.

**Architecture:** Шесть точечных правок в четырёх файлах. Все изменения — минимальный diff, без архитектурных перестроений. Группа Б требует SQL migration (024) и изменений в state machine.

**Tech Stack:** Python 3.11+, FastAPI/Starlette, SQLite WAL, concurrent.futures

---

## Группа Б — Write-path durability

### B-1: DocumentState.SENDING + migration 024

**Проблема:** Между вызовом транспорта и записью `SENT` в БД нет дурабельного маркера. При crash-resume документ в состоянии `ENCRYPTED` повторно идёт в send path — потенциальный дубль фискального документа.

**Решение:** Новый state `SENDING`. Переход `ENCRYPTED → SENDING` записывается атомарно до вызова транспорта. При crash-resume `SENDING` → `ERROR_RETRYABLE` (reconciliation проверит DPS).

**State machine после:**
```
PREPARED → SIGNED → ENCRYPTED → SENDING → SENT → KVT1 → KVT2 → ACK
                                    ↓
                              ERROR_RETRYABLE
```

**Файлы:**
- Modify: `src/prro_gateway/enums.py` — добавить `SENDING = "SENDING"` в `DocumentState`
- Create: `sql/024_sending_state.sql` — добавить `'SENDING'` в CHECK constraint (table recreation по паттерну migration 003)
- Modify: `src/prro_gateway/services/write_path.py` — три места (подробнее ниже)

**Acceptance Criteria:**
- [ ] `DocumentState.SENDING` существует в enum
- [ ] migration 024 добавляет 'SENDING' в CHECK constraint `fiscal_documents.state`
- [ ] В `_stage_send_or_offline`: перед `transport_client.send()` выполняется `BEGIN IMMEDIATE` → `update_state(SENDING)` → `commit()`
- [ ] В `process()`: `SENDING` в crash-resume ветке → `update_state(ERROR_RETRYABLE)` + log warning, дальнейший send не производится
- [ ] Existing error handling в `_stage_send_or_offline` (TransportRejectedError, TransportRetryableError и др.) корректно переводит из `SENDING` в `REJECTED`/`ERROR_RETRYABLE` — `_mark_document_and_inbox_error` не должен проверять expected_states при переходе из SENDING
- [ ] Тесты: crash_resume с SENDING → ERROR_RETRYABLE; normal flow ENCRYPTED → SENDING → SENT

### B-2: State validation перед транзакцией (#8)

**Проблема:** В `_stage_finalize` вызов `DocumentState(ctx.send_result.state)` происходит внутри открытой `BEGIN IMMEDIATE`. Невалидное значение бросает `ValueError` внутри транзакции.

**Решение:** Переместить валидацию до `BEGIN IMMEDIATE`. При невалидном state — return error result, транзакция не открывается.

**Файлы:**
- Modify: `src/prro_gateway/services/write_path.py:_stage_finalize`

**Acceptance Criteria:**
- [ ] `DocumentState(ctx.send_result.state)` вызывается до `conn.execute('BEGIN IMMEDIATE')`
- [ ] При `ValueError` возвращается error result без открытия транзакции
- [ ] Тест: `send_result.state` с невалидным значением → error result, нет открытой транзакции

---

## Группа В — Crypto/runtime isolation

### C-1: Executor abandonment при crypto timeout (#4)

**Проблема:** `with ThreadPoolExecutor(max_workers=1)` при `TimeoutError` вызывает `shutdown(wait=True)` в `__exit__`, блокируя request до завершения зависшего треда — несмотря на timeout.

**Решение:** Убрать context manager, использовать `concurrent.futures.wait` + `shutdown(wait=False)` при таймауте. Тред abandonment без ожидания.

```python
executor = ThreadPoolExecutor(max_workers=1)
future = executor.submit(self.crypto_provider.sign_raw, data=sign_data, document_id=_doc_id)
done, _ = concurrent.futures.wait([future], timeout=self.crypto_timeout_seconds)
if not done:
    executor.shutdown(wait=False)
    raise CryptoProviderUnavailableError(
        f'crypto sign_raw() timed out after {self.crypto_timeout_seconds}s'
    )
signed_payload = future.result()
executor.shutdown(wait=False)
```

То же применяется к блоку `sign()` (non-DPS path).

**Файлы:**
- Modify: `src/prro_gateway/services/write_path.py:_stage_sign` — оба блока (sign_raw и sign)

**Acceptance Criteria:**
- [ ] `with ThreadPoolExecutor` заменён на явный `executor` + `wait()` + `shutdown(wait=False)` в обоих блоках
- [ ] При timeout executor abandonement не блокирует
- [ ] CryptoProviderUnavailableError по-прежнему бросается при таймауте
- [ ] Тест: mock crypto provider с задержкой > timeout → ошибка возвращается сразу, не ждёт тред

### C-2: run_in_threadpool для async endpoints (#7)

**Проблема:** `ingress_checkbox`, `admin_reconciliation_trigger`, `admin_offline_sync` объявлены `async def` но вызывают синхронный write path (DB + crypto + transport). Долгий вызов блокирует event loop.

**Решение:** `await request.json()` остаётся async, синхронная часть выносится в `run_in_threadpool`:

```python
from starlette.concurrency import run_in_threadpool

@app.post("/v1/ingress/checkbox")
async def ingress_checkbox(request: Request):
    raw = await request.json()
    with container.connect() as conn:
        inbox, command, process_result, is_replay = await run_in_threadpool(
            container.ingress_service.accept_checkbox,
            conn,
            raw_request=raw,
            response_timeout_seconds=container.config.ingress.rest.response_timeout_seconds,
        )
    ...
```

Аналогично для `admin_reconciliation_trigger` и `admin_offline_sync` — оборачивается синхронный вызов сервиса.

**Файлы:**
- Modify: `src/prro_gateway/runtime/rest_app.py` — 3 endpoint handler'а

**Acceptance Criteria:**
- [ ] `from starlette.concurrency import run_in_threadpool` добавлен
- [ ] Синхронный вызов `accept_checkbox` обёрнут в `run_in_threadpool`
- [ ] `reconciliation_service.reconcile_pending` обёрнут в `run_in_threadpool`
- [ ] `offline_sync_service` вызов обёрнут в `run_in_threadpool`
- [ ] Поведение эндпоинтов не меняется (те же response bodies, те же status codes)
- [ ] Тест: endpoint по-прежнему возвращает корректный ответ

---

## Группа Г — Hygiene

### D-1: Migration splitter (#10)

**Проблема:** `_split_sql_statements` режет SQL по `;` после strip комментариев. Сломается на триггерах, строковых литералах со `;`, сложных DDL.

**Решение:** Заменить regex-splitter на `sqlite3.complete_statement()` — SQLite-нативный токенайзер, который корректно определяет границы statements с учётом строковых литералов, комментариев и вложенных блоков. Атомарность с INSERT INTO schema_migrations сохраняется (оба в одном `BEGIN IMMEDIATE`).

```python
def _split_sql_statements(sql_text: str) -> list[str]:
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

**Файлы:**
- Modify: `src/prro_gateway/migrations/runner.py` — заменить тело `_split_sql_statements`

**Acceptance Criteria:**
- [ ] `_split_sql_statements` использует `sqlite3.complete_statement()` вместо `sql.split(';')`
- [ ] Существующая транзакционная логика (`BEGIN IMMEDIATE` + `schema_migrations` insert) сохранена без изменений
- [ ] PRAGMA-фильтрация сохранена
- [ ] Все существующие миграции (001–023) применяются корректно
- [ ] Тест: миграция с `;` в строковом литерале применяется без ошибки

### D-2: Maria304 native total_sum guard (#5)

**Проблема:** `MARIA_304_NATIVE` полностью пропускает валидацию `goods`/`payments`. Документ с `total_sum = 0` или `total_sum = NULL` принимается без ошибки.

**Решение:** Минимальный guard — перед созданием документа проверить что `total_sum` присутствует и `> 0` для SELL/RETURN/SERVICE_IN/OUT. Проверка добавляется в `_stage_acquire_and_validate` рядом с существующим `maria304_bypass` блоком.

```python
if maria304_bypass and command.operation_type in {
    OperationType.SELL, OperationType.RETURN,
    OperationType.SERVICE_IN, OperationType.SERVICE_OUT,
}:
    total = command.payload.get('total_sum') if isinstance(command.payload, dict) else None
    if not total or int(total) <= 0:
        violations = ['total_sum must be > 0 for MARIA_304_NATIVE fiscal documents']
```

**Файлы:**
- Modify: `src/prro_gateway/services/write_path.py:_stage_acquire_and_validate`

**Acceptance Criteria:**
- [ ] `total_sum = 0` для MARIA_304_NATIVE SELL → rejected с validation error
- [ ] `total_sum = NULL` для MARIA_304_NATIVE SELL → rejected
- [ ] `total_sum > 0` → документ принимается как раньше
- [ ] STATUS, SHIFT_OPEN/CLOSE, X_REPORT, Z_REPORT не затронуты guard'ом
- [ ] Тест: параметризованный тест по operation_type × total_sum

---

## Out of scope

- **#6 JKS password plaintext** — backlog пост-пилот (требует OS keystore / KMS)
- **#1, #2, #9 Auth hardening** — отложено, только если сеть закрытая
- **Maria304 rich parser** — отдельный будущий спринт; D-2 — минимальный guard до тех пор
