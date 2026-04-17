# Sprint 12 Implementation Plan
**Дата:** 2026-04-15  
**Базова лінія:** 614 passed, 0 failed  
**Мета:** Закрити критичні offline gap-и + операторський API + ops-loop

---

## Зв'язок з GAP_REGISTRY

| Gap | Крок |
|-----|------|
| A1 — GET_STATUS bug | Крок 1 |
| B1 — GO_OFFLINE без активного range | Крок 2 |
| B2 — watermark при GO_OFFLINE | Крок 2 |
| C1 — CRYPTO_DEGRADED при відкритому breaker | Крок 3 |
| C2 — BLOCKED state enforcement | Крок 4 |
| B4/B5 — ops-loop (auto-sync + reconciliation) | Крок 5 |
| E1/E2/E3 — Admin API | Крок 6 |

Gaps B3, B6, B7, B8 — Sprint 13.  
Gaps A2, H1 — post-pilot.

---

## Frozen invariants (актуальні для цього спринту)

- **Invariant 1:** жодних мережевих або crypto-викликів всередині довгих SQLite-транзакцій.
- **Invariant 2:** один `fiscal_number` = один логічний single-writer.
- **Invariant 5:** offline поважає ліміти часу і кодів.
- **Invariant 8:** recovery і reconciliation не порушують state transitions тихо.
- **Invariant 9:** graceful shutdown важливіший за "завершити швидше".

---

## Крок 0 — Міграція 011: per-fiscal-number config

### Файл: `sql/011_per_fn_config.sql`

```sql
CREATE TABLE fiscal_number_config (
    fiscal_number           TEXT PRIMARY KEY,
    enforce_blocked_mode    INTEGER NOT NULL DEFAULT 0
                                CHECK (enforce_blocked_mode IN (0, 1)),
    min_offline_codes       INTEGER NOT NULL DEFAULT 0
                                CHECK (min_offline_codes >= 0),
    max_offline_codes       INTEGER NOT NULL DEFAULT 0
                                CHECK (max_offline_codes >= 0
                                       AND max_offline_codes >= min_offline_codes),
    created_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

**Примітки:**
- `enforce_blocked_mode = 0` за замовчуванням — юридично вимагається, але в умовах воєнних блекаутів податкова формально не штрафує, тому DEFAULT=OFF.
- `min_offline_codes = 0` і `max_offline_codes = 0` означають: watermark вимкнений (поведінка як зараз).
- Таблиця без FOREIGN KEY до `node_state` — записи з'являються при потребі, відсутній запис = дефолтна поведінка.
- Рядок вставляється через `INSERT OR IGNORE` при першому зверненні (lazy-init) або явно при конфігурації ноди.

### Нові файли:

**`src/prro_gateway/models/storage.py`** — додати:
```python
class FiscalNumberConfigRecord(BaseModel):
    fiscal_number: str
    enforce_blocked_mode: int = 0
    min_offline_codes: int = 0
    max_offline_codes: int = 0
    created_at: str = ''
    updated_at: str = ''
```

**`src/prro_gateway/repositories/fn_config.py`** — новий файл:
```python
class FiscalNumberConfigRepository:
    def get(conn, fiscal_number: str) -> FiscalNumberConfigRecord | None:
        # SELECT ... FROM fiscal_number_config WHERE fiscal_number = ?
    
    def get_or_default(conn, fiscal_number: str) -> FiscalNumberConfigRecord:
        # get() or FiscalNumberConfigRecord(fiscal_number=fiscal_number)
    
    def upsert(conn, *, fiscal_number: str, **fields) -> None:
        # INSERT OR REPLACE or UPDATE
```

### Тести кроку 0:
- `test_migration_011.py` або додати до `test_migrations.py` — перевірити що нова таблиця створюється, DEFAULT=0, constraint `max >= min` спрацьовує.
- `test_fn_config_repository.py` — `get()` при відсутньому записі, `get_or_default()`, `upsert()`.

---

## Крок 1 — A1: GET_STATUS bug

### Проблема (вже підтверджена в коді)

`write_path.py:1161`:
```python
_MANAGEMENT_OPS = {OperationType.GO_OFFLINE, OperationType.GO_ONLINE, OperationType.ASK_OFFLINE_CODES}
```
`GET_STATUS` відсутній → `_guard_preconditions()` перевіряє зміну, канал, LND → write_path витрачає LND, намагається підписати та відправити в DPS.

### Зміни

**Файл: `src/prro_gateway/services/write_path.py`**

1. Рядок ~1161 — додати `OperationType.GET_STATUS` до `_MANAGEMENT_OPS`:
   ```python
   _MANAGEMENT_OPS = {
       OperationType.GO_OFFLINE,
       OperationType.GO_ONLINE,
       OperationType.ASK_OFFLINE_CODES,
       OperationType.GET_STATUS,
   }
   ```

2. В `_handle_management_command_locked()` — додати обробник перед `if op == OperationType.GO_OFFLINE:`:
   ```python
   if op == OperationType.GET_STATUS:
       # Return current node state — no side effects, no LND allocation
       state = ctx.node_state
       active_range = OfflineRepository.get_active_range(conn, ctx.fiscal_number)
       codes_remaining = None
       if active_range is not None:
           codes_remaining = active_range.last_fiscal_no - active_range.next_fiscal_no + 1
       result_payload = {
           'mode': state.mode,
           'shift_state': state.shift_state,
           'codes_remaining': codes_remaining,
       }
       result = self._mark_success_locked(conn, ctx=ctx, document_id=None,
                                          payload=result_payload)
       return ctx, result
   ```

   Якщо `_mark_success_locked` не підходить для management ops без документа — використовувати ту ж схему що вже є у `GO_ONLINE`/`GO_OFFLINE` (вони теж не створюють документ).

### Тести кроку 1:
- `GET_STATUS` при ONLINE → повертає `mode=ONLINE`, `codes_remaining=None` або числове значення.
- `GET_STATUS` при відкритій зміні → shift_state відображається коректно.
- `GET_STATUS` не зменшує `next_lnd`.
- `GET_STATUS` не створює запису в `fiscal_documents`.

---

## Крок 2 — B1+B2: GO_OFFLINE guard + watermark

### Проблема

В `_handle_management_command_locked()` рядок 1286-1300:
```python
if op == OperationType.GO_OFFLINE:
    if ctx.node_state.mode != NodeMode.ONLINE:
        ...  # вже є
    # ← тут відсутня перевірка на активний range!
    session_id = ...
    OfflineRepository.create_open_session(...)
    NodeStateRepository.update_mode(..., NodeMode.OFFLINE)
```

Якщо `get_active_range()` повертає `None` → перший SELL впаде з `OFFLINE_CODES_EXHAUSTED`.

### Зміни

**Файл: `src/prro_gateway/services/write_path.py`**

В блоці `GO_OFFLINE`, одразу після перевірки `mode != NodeMode.ONLINE`:

```python
if op == OperationType.GO_OFFLINE:
    if ctx.node_state.mode != NodeMode.ONLINE:
        ...

    # B1: перевірити наявність активного range
    active_range = OfflineRepository.get_active_range(conn, ctx.fiscal_number)
    if active_range is None:
        err = build_canonical_error(
            CanonicalErrorCode.OFFLINE_CODES_EXHAUSTED,
            message='Cannot go offline: no active offline code range. Use ASK_OFFLINE_CODES first.',
        )
        result = self._mark_error_locked(conn, ctx=ctx, error=err, document_id=None, state=None)
        return ctx, result

    # B2: watermark check
    fn_config = FiscalNumberConfigRepository.get_or_default(conn, ctx.fiscal_number)
    if fn_config.min_offline_codes > 0:
        codes_remaining = active_range.last_fiscal_no - active_range.next_fiscal_no + 1
        if codes_remaining < fn_config.min_offline_codes:
            # Sprint 12: audit warning. Sprint 13: auto-fill to max via DPS.
            AuditRepository.log_event(
                conn,
                entity_type='OFFLINE',
                entity_id=ctx.fiscal_number,
                event_type='OFFLINE_CODES_BELOW_WATERMARK',
                severity='WARNING',
                event_payload_json=dumps_json({
                    'codes_remaining': codes_remaining,
                    'min_offline_codes': fn_config.min_offline_codes,
                    'max_offline_codes': fn_config.max_offline_codes,
                    'fiscal_number': ctx.fiscal_number,
                }),
            )
            # В Sprint 13: якщо є DPS client → запит до max, тоді continue

    # Далі — оригінальна логіка create_open_session / update_mode
    session_id = ...
```

**Важливо:** audit event логується в тому ж рядку до commit основної транзакції — нормально, бо `AuditRepository.log_event()` в межах того ж `conn`. Якщо транзакція відкочується — audit теж відкочується. Це прийнятна поведінка.

### Тести кроку 2:
- `GO_OFFLINE` без активного range → `OFFLINE_CODES_EXHAUSTED`, mode залишається `ONLINE`.
- `GO_OFFLINE` з активним range → проходить, mode = `OFFLINE`.
- `GO_OFFLINE` нижче watermark (`min=10`, `codes_remaining=5`) → проходить, але audit-event `OFFLINE_CODES_BELOW_WATERMARK` присутній.
- `GO_OFFLINE` з watermark=0 (DEFAULT) → no warning event.

---

## Крок 3 — C1: CRYPTO_DEGRADED при відкритому breaker

### Проблема

`_stage_sign()` рядок 376-403: breaker відкрито → документ помічається `ERROR_RETRYABLE`, аудит-подія `CRYPTO_BREAKER_BLOCKED` — але `node_state.mode` залишається `ONLINE`. Зовнішній моніторинг не бачить деградації.

### Зміни

**Файл: `src/prro_gateway/services/write_path.py`**

В `_stage_sign()`, після commit audit-транзакції (рядок ~402), перед `return ctx, result`:

```python
# C1: set CRYPTO_DEGRADED mode so external monitoring sees degradation
with conn:  # NOTE: окрема коротка транзакція поза основним write-path
    # Читаємо поточний стан щоб не перезаписувати якщо вже CRYPTO_DEGRADED
    current = NodeStateRepository.get_state(conn, ctx.fiscal_number)
    if current is not None and current.mode == NodeMode.ONLINE:
        NodeStateRepository.update_mode(conn, fiscal_number=ctx.fiscal_number, mode=NodeMode.CRYPTO_DEGRADED)
```

**УВАГА — invariant check:**
- Це окрема коротка транзакція ПІСЛЯ commit основної помилки документа — не порушує invariant 1.
- `CRYPTO_DEGRADED` → `ONLINE` переходу немає в коді. Потрібно або: (a) автоматично при успішному підписі скидати назад у `ONLINE`, або (b) ручний `GO_ONLINE`-like endpoint. Для Sprint 12 — варіант (a): в `_stage_sign()` при успішному підписі, якщо `mode == CRYPTO_DEGRADED`:
  ```python
  # При успіху: crypto recovered → back to ONLINE
  current = NodeStateRepository.get_state(conn, ctx.fiscal_number)
  if current is not None and current.mode == NodeMode.CRYPTO_DEGRADED:
      NodeStateRepository.update_mode(conn, fiscal_number=ctx.fiscal_number, mode=NodeMode.ONLINE)
  ```

### Тести кроку 3:
- `_crypto_consecutive_failures >= threshold` → mode переходить у `CRYPTO_DEGRADED`.
- Наступний успішний підпис → mode повертається у `ONLINE`.
- Якщо mode вже `CRYPTO_DEGRADED` (повторний блок) — не дублює transition.
- `CRYPTO_DEGRADED` не блокує `GET_STATUS`, `GO_OFFLINE`, `GO_ONLINE` (вони в `_MANAGEMENT_OPS`).

---

## Крок 4 — BLOCKED state enforcement (enforce_blocked_mode)

### Проблема

`_check_offline_limits()` рядок 1260-1265: при `monthly_seconds >= MAX_OFFLINE_MONTH_SECONDS` повертає `OFFLINE_LIMIT_REACHED` error але mode залишається `OFFLINE`. Юридично необхідно заблокувати операції (NodeMode.BLOCKED), але DEFAULT=OFF через воєнний стан і позицію ДПС.

### Зміни

**Файл: `src/prro_gateway/services/write_path.py`**

Сигнатура `_check_offline_limits()` — зараз `@classmethod` з `(*, node_state, offline_session)`. Додаємо параметр `fn_config`:

```python
@classmethod
def _check_offline_limits(cls, *, node_state, offline_session, fn_config) -> tuple[CanonicalError | None, bool]:
    # returns (error_or_None, should_block_mode)
```

Або простіше — повертати bool окремо через out-параметр. Але чистіший варіант: метод повертає `CanonicalError | None`, а `should_block` — атрибут result або side-channel через виняток. Найпростіше: зробити окремий метод `_should_apply_blocked_mode()`.

**Рекомендований мінімальний diff:**

В `_check_offline_limits()` замінити рядок 1260-1265:
```python
if monthly_seconds >= cls.MAX_OFFLINE_MONTH_SECONDS:
    return build_canonical_error(
        CanonicalErrorCode.OFFLINE_LIMIT_REACHED,
        message='Monthly offline time limit exceeded.',
        details={'monthly_seconds': monthly_seconds, 'limit_seconds': cls.MAX_OFFLINE_MONTH_SECONDS},
    )
```
на:
```python
if monthly_seconds >= cls.MAX_OFFLINE_MONTH_SECONDS:
    return build_canonical_error(
        CanonicalErrorCode.OFFLINE_LIMIT_REACHED,
        message='Monthly offline time limit exceeded.',
        details={
            'monthly_seconds': monthly_seconds,
            'limit_seconds': cls.MAX_OFFLINE_MONTH_SECONDS,
            'enforce_blocked_mode': fn_config.enforce_blocked_mode if fn_config else 0,
        },
    ), fn_config.enforce_blocked_mode if fn_config else False
```

Тобто `_check_offline_limits()` тепер повертає `tuple[CanonicalError | None, bool]` де `bool` = треба переходити в BLOCKED.

**Всі місця виклику** (знайти через `Grep "cls._check_offline_limits\|_check_offline_limits"`) — оновити щоб розпаковували tuple.

**В місці де виявляємо `should_block=True`** — перед поверненням error:
```python
error, should_block = cls._check_offline_limits(...)
if error:
    if should_block:
        NodeStateRepository.update_mode(conn, fiscal_number=fiscal_number, mode=NodeMode.BLOCKED)
    return error
```

**BLOCKED mode guards:**
- В `_guard_preconditions()` — якщо `mode == NodeMode.BLOCKED` і op не в `_MANAGEMENT_OPS` → `OFFLINE_LIMIT_REACHED`.
- GO_ONLINE з `BLOCKED` → дозволений якщо `enforce_blocked_mode=False` (тобто оператор може виводити ноду вручну), або тільки після нового місяця якщо `True`. Для Sprint 12: дозволити GO_ONLINE з BLOCKED режиму завжди (адмін вирішує).

### Тести кроку 4:
- `enforce_blocked_mode=True` + 168h exceeded → mode = `BLOCKED`, SELL blocked.
- `enforce_blocked_mode=False` (DEFAULT) → mode залишається `OFFLINE`, SELL blocked з OFFLINE_LIMIT_REACHED (поведінка без змін).
- GO_ONLINE з `BLOCKED` → дозволений.
- `fn_config = None` (немає запису) → поведінка як DEFAULT=False.

---

## Крок 5 — ops-loop: background reconciliation + offline sync

### Архітектура

Єдиний `threading.Thread(daemon=True, name="prro-ops-loop")` в `RuntimeContainer`.  
uvicorn запускається в main thread (async event loop), ops-loop — окремий OS thread. SQLite WAL підтримує concurrent reads з окремих connections. Запис відбувається через write-path worker — але ops-loop виконує reconciliation і offline_sync, які теж є writer'ами. Треба переконатися що вони не конфліктують з write-path (обидва використовують `IMMEDIATE` transactions, SQLite серіалізує через `busy_timeout=5000`).

### Конфіг

**Файл: `src/prro_gateway/config.py`** — в `RuntimeConfig` або окремий клас `OpsLoopConfig`:
```python
ops_loop_enabled: bool = True
ops_loop_interval_seconds: int = 60
```

### Зміни в container.py

```python
import threading

class RuntimeContainer:
    def __init__(self, ...):
        ...
        self._ops_loop_stop: threading.Event = threading.Event()
        self._ops_loop_thread: threading.Thread | None = None

    def initialize(self) -> None:
        ...
        self.last_startup_report = supervisor.run()
        ...
        # Запускаємо після supervisor щоб reconciliation startup вже відбувся
        if self.config.runtime.ops_loop_enabled:
            self._start_ops_loop()

    def shutdown(self) -> None:
        # Зупинити loop ДО drain ingress щоб уникнути race
        self._ops_loop_stop.set()
        if self._ops_loop_thread is not None:
            self._ops_loop_thread.join(timeout=10)
            self._ops_loop_thread = None
        # ... далі існуючий код shutdown ...
        self.health.live = False
        ...

    def _start_ops_loop(self) -> None:
        self._ops_loop_stop.clear()
        t = threading.Thread(
            target=self._ops_loop_body,
            name="prro-ops-loop",
            daemon=True,
        )
        t.start()
        self._ops_loop_thread = t
        self.logger.info("ops_loop_started", extra={"extra_fields": {
            "interval_seconds": self.config.runtime.ops_loop_interval_seconds,
        }})

    def _ops_loop_body(self) -> None:
        interval = self.config.runtime.ops_loop_interval_seconds
        while not self._ops_loop_stop.wait(timeout=interval):
            try:
                self._ops_tick()
            except Exception as exc:
                self.logger.error("ops_loop_tick_error", extra={"extra_fields": {"error": str(exc)}})

    def _ops_tick(self) -> None:
        """One periodic tick: reconcile pending docs + sync offline ACK backlog."""
        if self.reconciliation_service is None and self.offline_sync_service is None:
            return
        with self.connect() as conn:
            fiscal_numbers = self._list_active_fiscal_numbers(conn)

        for fn in fiscal_numbers:
            try:
                self._ops_tick_for_fn(fn)
            except Exception as exc:
                self.logger.error("ops_loop_fn_error", extra={"extra_fields": {
                    "fiscal_number": fn, "error": str(exc),
                }})

    def _ops_tick_for_fn(self, fiscal_number: str) -> None:
        with self.connect() as conn:
            node_state = NodeStateRepository.get_state(conn, fiscal_number)
        if node_state is None:
            return
        mode = NodeMode(node_state.mode)

        if mode == NodeMode.ONLINE:
            # 1. Reconcile SENT/KVT1/KVT2/ERROR_RETRYABLE documents
            if self.reconciliation_service is not None:
                with self.connect() as conn:
                    self.reconciliation_service.reconcile_pending(conn)

            # 2. Sync offline backlog якщо є
            if self.offline_sync_service is not None:
                with self.connect() as conn:
                    from .repositories.fiscal_documents import FiscalDocumentRepository
                    pending = FiscalDocumentRepository.count_pending_for_offline_sync(conn, fiscal_number=fiscal_number)
                if pending > 0:
                    with self.connect() as conn:
                        self.offline_sync_service.sync_pending(conn, fiscal_number=fiscal_number)

        elif mode in (NodeMode.OFFLINE, NodeMode.BLOCKED):
            # Sprint 13: DPS ping → auto-GO_ONLINE
            pass

    def _list_active_fiscal_numbers(self, conn) -> list[str]:
        """Return all fiscal_numbers in node_state."""
        rows = conn.execute("SELECT fiscal_number FROM node_state").fetchall()
        return [row[0] for row in rows]
```

**Перевірити сигнатуру `reconciliation_service.reconcile_pending(conn)` і `offline_sync_service.sync_pending(conn, fiscal_number=...)`** — якщо сигнатури відрізняються, адаптувати виклики.

### Тести кроку 5:
- ops-loop не запускається якщо `ops_loop_enabled=False`.
- `shutdown()` зупиняє loop і join() завершується в 10с.
- ops-loop викликає `reconciliation_service.reconcile_pending()` при ONLINE mode.
- ops-loop викликає `offline_sync_service.sync_pending()` при наявності pending docs.
- ops-loop НЕ викликає sync при порожньому backlog.
- Виняток в одному fiscal_number не зупиняє loop і не блокує інші.

---

## Крок 6 — Admin API endpoints

### Три нові endpoint'и

**Файл: `src/prro_gateway/rest_app.py`** (або окремий `admin_routes.py` якщо rest_app.py вже великий).

#### GET /v1/admin/node-state

```
Response 200:
{
  "fiscal_number": "...",
  "mode": "ONLINE",
  "shift_state": "OPENED",
  "crypto_breaker_open": false,
  "crypto_consecutive_failures": 0,
  "current_month_offline_seconds": 12345,
  "codes_remaining": 150,
  "active_range_id": "range-uuid-...",
  "active_offline_session_id": null,
  "enforce_blocked_mode": false,
  "min_offline_codes": 10,
  "max_offline_codes": 100
}
```

Потрібно: `NodeStateRepository.get_state()`, `OfflineRepository.get_active_range()`, `FiscalNumberConfigRepository.get_or_default()`. `crypto_breaker_open` — від `WritePathWorker` (якщо доступний через container).

#### GET /v1/admin/offline-ranges

```
Response 200:
{
  "fiscal_number": "...",
  "ranges": [
    {
      "range_id": "...",
      "first_fiscal_no": 1001,
      "last_fiscal_no": 1200,
      "next_fiscal_no": 1056,
      "codes_remaining": 145,
      "status": "ACTIVE",
      "issued_at": "2026-04-10T12:00:00"
    }
  ],
  "total_remaining": 145
}
```

Query: `SELECT * FROM offline_ranges WHERE fiscal_number = ? ORDER BY created_at ASC`.

#### GET /v1/admin/offline-sessions

```
Response 200:
{
  "fiscal_number": "...",
  "active_session": {
    "offline_session_id": "...",
    "started_at": "2026-04-15T10:00:00",
    "elapsed_seconds": 3600,
    "accumulated_month_seconds": 7200,
    "status": "OPEN",
    "reason": "manual"
  },
  "current_month_offline_seconds": 7200,
  "month_limit_seconds": 604800,
  "continuous_limit_seconds": 129600
}
```

### Авторизація

Для Sprint 12: без авторизації (або `X-Admin-Token` простий порівняльний check якщо вже є в конфізі). Повноцінний auth — post-pilot.

### Помилки

- `fiscal_number` береться з URL або config якщо один. Якщо `node_state` відсутній → 404.
- Усі endpoints read-only, без side effects.

### Тести кроку 6:
- Кожен endpoint з присутнім node_state → 200 з правильною структурою.
- Endpoint при відсутньому node_state → 404.
- `codes_remaining` коректно рахується.
- `elapsed_seconds` в offline-sessions відображає реальний час.

---

## Порядок реалізації

```
Крок 0 (міграція)  →  Крок 1 (A1)  →  Крок 2 (B1/B2)
                   →  Крок 3 (C1)  →  Крок 4 (BLOCKED)
                                    →  Крок 5 (ops-loop)
                                    →  Крок 6 (Admin API)
```

Кроки 1-4 незалежні (після кроку 0), можна паралелити.  
Крок 5 (ops-loop) залежить від коректної роботи reconciliation і offline_sync.  
Крок 6 (Admin API) незалежний від кроків 3-5, але потребує кроку 0 (fn_config).

---

## Тести які треба написати (загальний список)

| Тест-файл | Що покриває |
|-----------|-------------|
| `test_migration_011.py` | Нова таблиця, constraints, defaults |
| `test_fn_config_repository.py` | CRUD fn_config, get_or_default |
| `test_get_status_command.py` | A1: GET_STATUS не витрачає LND |
| `test_go_offline_guard.py` | B1: блок без range; B2: watermark warning |
| `test_crypto_degraded_mode.py` | C1: mode transition при відкритому breaker |
| `test_blocked_mode_enforcement.py` | Крок 4: BLOCKED ON vs OFF |
| `test_ops_loop.py` | Крок 5: запуск/зупинка, reconcile trigger, sync trigger |
| `test_admin_api.py` | Крок 6: всі три endpoints |

**Baseline:** 614 passed → після Sprint 12 очікується 614 + ~40-50 нових тестів.

---

## Відкриті питання (треба вирішити до початку реалізації)

1. ~~**`reconciliation_service.reconcile_pending(conn)` сигнатура**~~ — **ПІДТВЕРДЖЕНО:** `reconcile_pending(self, conn) -> ReconciliationRunResult`. Приймає `conn` напряму.

2. ~~**`offline_sync_service.sync_pending(conn, fiscal_number=...)` сигнатура**~~ — **ПІДТВЕРДЖЕНО:** `sync_pending(self, conn, *, fiscal_number: str | None = None) -> OfflineSyncRunResult`. Приймає `conn` + опціональний `fiscal_number`.

3. **Де зберігається `crypto_breaker_open`** — в `WritePathWorker` як instance-variable. Щоб Admin API міг читати, container повинен тримати посилання на worker. Вже є: `container.command_processor` = `WritePathWorker`. Достукатися через `container.command_processor._crypto_consecutive_failures >= container.command_processor.crypto_breaker_threshold`.

4. **`GET_STATUS` — `_mark_success_locked` з `document_id=None`** — перевірити сигнатуру, можливо потрібний новий хелпер або той самий що у GO_ONLINE.

5. ~~**`_check_offline_limits` caller**~~ — **ПІДТВЕРДЖЕНО:** єдиний caller — `write_path.py:211`. Зміна сигнатури зачіпає одне місце.

---

## Критерій завершення Sprint 12

- [ ] 614 passed → 614+ passed (без регресій)
- [ ] GET_STATUS не витрачає LND (A1 закрито)
- [ ] GO_OFFLINE без range → OFFLINE_CODES_EXHAUSTED (B1 закрито)
- [ ] `crypto_breaker_open` → `CRYPTO_DEGRADED` mode (C1 закрито)
- [ ] `enforce_blocked_mode=True` → BLOCKED при 168h (Крок 4 закрито)
- [ ] ops-loop запускається і reconcile/sync відбуваються автоматично (B4/B5 закрито)
- [ ] Три admin API endpoints відповідають коректними даними (E1/E2/E3 закрито)
- [ ] Міграція 011 застосовується ідемпотентно
