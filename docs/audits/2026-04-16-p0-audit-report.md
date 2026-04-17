# P0 Audit Report — PRRO Gateway
**Дата:** 2026-04-16  
**Аудитор:** 4 паралельних агенти (read-only), результати зведені вручну  
**Scope:** State machine · Idempotency · Offline sequence · Crypto sidecar  
**Статус:** FINAL — всі 4 зони виконані

---

## Зона 1 — State Machine Completeness

### [CRITICAL] CRYPTO_DEGRADED не має fast-path guard

**Файл:** `src/prro_gateway/services/write_path.py` (~рядки 162–245)  
**Зона:** 1  
**Статус:** verified  
**Опис:** Є два fast-path guards перед `_guard_preconditions`:
- STOP_MODE guard (~рядок 162): блокує всі ops крім GET_STATUS
- BLOCKED guard (~рядок 171): дозволяє тільки GO_OFFLINE / GO_ONLINE / ASK_OFFLINE_CODES / GET_STATUS

`CRYPTO_DEGRADED` не охоплений жодним з них. Операції `SELL`, `SHIFT_OPEN`, `SHIFT_CLOSE`, `Z_REPORT` проходять через обидва guards, потрапляють у `_guard_preconditions`, де перевірки на `NodeMode` теж відсутні, і доходять до `_stage_sign` де ламаються або споживають LND/Z-номер.

**Сценарій відтворення:**
1. Sidecar виходить з ладу 5 разів поспіль → node.mode = CRYPTO_DEGRADED
2. Касир відправляє SELL запит
3. Запит проходить STOP_MODE і BLOCKED guards
4. `_guard_preconditions` не перевіряє NodeMode
5. Виконується `_stage_sign` → одразу повертає помилку, але до цього моменту LND вже може бути виділений

**Поточне покриття тестами:** немає тесту  
**Пропозиція:** Додати guard після BLOCKED fast-path:
```python
if node_state is not None and node_state.mode == NodeMode.CRYPTO_DEGRADED:
    result = self._mark_error_locked(conn, ctx=ctx,
        error=build_canonical_error(CanonicalErrorCode.CRYPTO_DEGRADED),
        document_id=None, state=None)
    return ctx, result
```

---

### [HIGH] GOING_OFFLINE і GOING_ONLINE — відсутні guards, але стани ніколи не записуються

**Файл:** `src/prro_gateway/enums.py` (NodeMode.GOING_OFFLINE/GOING_ONLINE), `write_path.py:1454–1573`  
**Зона:** 1  
**Статус:** verified  
**Опис:** `GOING_OFFLINE` і `GOING_ONLINE` визначені в enum і описані в CLAUDE.md як живі стани. Але жодний `update_mode` у всьому codebase їх не записує. Перехід `ONLINE → OFFLINE` виконується атомарно одним комітом (`write_path.py:1573`). Проміжний стан в DB ніколи не з'являється.

Наслідок: документація та enum описують стан-машину яка не існує в реалізації. Майбутній розробник може додати запис `GOING_OFFLINE` і зламати логіку, бо guards для нього немає.

**Пропозиція:** Або прибрати GOING_OFFLINE/GOING_ONLINE з enum і документації, або додати explicit comment в код що цей перехід атомарний і проміжний стан не використовується.

---

### [HIGH] BLOCKED + GO_OFFLINE повертає NODE_ALREADY_OFFLINE замість OFFLINE_LIMIT_REACHED

**Файл:** `src/prro_gateway/services/write_path.py` (~рядок 171 fast-path + ~рядок 1454)  
**Зона:** 1  
**Статус:** verified  
**Опис:** BLOCKED fast-path дозволяє GO_OFFLINE. Але `_handle_management_command_locked` для GO_OFFLINE перевіряє `mode != ONLINE` і повертає `NODE_ALREADY_OFFLINE` — що для BLOCKED режиму є семантично неправильним кодом помилки. Оператор отримує "нода вже в офлайні" замість "операція заблокована через перевищення ліміту".

**Пропозиція:** Або виключити GO_OFFLINE з BLOCKED fast-path (найпростіше), або окремо обробити BLOCKED стан в `_handle_go_offline` з поверненням `BLOCKED_MODE_ACTIVE`.

---

### [MEDIUM] Матриця непокритих шляхів — підтверджені FALLS_THROUGH

| NodeMode        | OperationType | guard?          | result         |
|-----------------|---------------|-----------------|----------------|
| GOING_OFFLINE   | SELL          | FALLS_THROUGH   | (ніколи не буває — стан не записується) |
| GOING_OFFLINE   | SHIFT_CLOSE   | FALLS_THROUGH   | (ніколи не буває) |
| GOING_ONLINE    | Z_REPORT      | FALLS_THROUGH   | (ніколи не буває) |
| CRYPTO_DEGRADED | SHIFT_OPEN    | FALLS_THROUGH   | LND/Z потенційно споживаються |
| CRYPTO_DEGRADED | GO_OFFLINE    | FALLS_THROUGH   | _handle_go_offline перевіряє mode == ONLINE → reject, але через некоректний error code |
| BLOCKED         | SHIFT_CLOSE   | EXPLICIT_REJECT | _guard_preconditions: SHIFT_CLOSE потребує OPENED shift, guard відпрацьовує |
| BLOCKED         | Z_REPORT      | EXPLICIT_REJECT | аналогічно |

---

## Зона 2 — Idempotency End-to-End

### [HIGH] Crash після SIGNED commit → дублікат документа з новим LND

**Файл:** `src/prro_gateway/services/write_path.py:230–239`  
**Зона:** 2  
**Статус:** verified (підтверджено Zone 4 Q3)  
**Опис:** Retry resume logic перевіряє тільки `state == ERROR_RETRYABLE AND transport_request_id IS None`. Документ в стані `SIGNED` не відповідає цій умові. При повторній обробці викликається `create_prepared` → `increment_lnd` → новий документ з новим LND. Попередній SIGNED документ залишається orphaned в DB.

Юридичний ризик: при відправці нового документа на DPS LND буде мати прогалину, або два документи з різними LND але однаковим фіскальним вмістом потраплять на DPS.

**Сценарій відтворення:**
1. `_stage_sign` успішно виконується, `conn.commit()` (SIGNED) — рядок ~561
2. Процес падає до завершення `_stage_send_or_offline`
3. При наступному старті `process_next` отримує той самий inbox row
4. `_existing.state == SIGNED` → resume check не спрацьовує
5. Викликається `create_prepared` з тим самим `request_id` → `IntegrityError` на UNIQUE constraint
6. Виняток не перехоплений → worker падає

**Пропозиція:** Додати в resume logic гілку для `state in {SIGNED, ENCRYPTED}`:
```python
elif _existing.state in {DocumentState.SIGNED, DocumentState.ENCRYPTED}:
    ctx.document = _existing  # відновити контекст
    # перейти прямо до _stage_send_or_offline, пропустити sign
```

---

### [MEDIUM] Inbox idempotency barrier — race між двома ingress протоколами

**Файл:** `src/prro_gateway/repositories/inbox.py:accept_command`  
**Зона:** 2  
**Статус:** SAFE — verified  
**Опис:** `accept_command` використовує `INSERT OR IGNORE` + подальший SELECT за `idempotency_key`. UNIQUE constraint на `(idempotency_key)` гарантує що навіть при паралельних вставках через REST і XML-RPC лише перший INSERT матеріалізується; другий повертає існуючий `request_id`. Race між двома ingress не може призвести до двох різних записів у inbox для одного ключа.

**Статус: SAFE**

---

### [MEDIUM] allocate_z_report_number атомарність

**Файл:** `src/prro_gateway/repositories/node_state.py`  
**Зона:** 2  
**Статус:** SAFE  
**Опис:** `RETURNING next_z_report_number - 1` є атомарним в SQLite WAL + `BEGIN IMMEDIATE`. Single-writer invariant (FI-2) додатково унеможливлює concurrent increment.

**Статус: SAFE**

---

### [MEDIUM] LND gap при failed send — документована поведінка?

**Файл:** `src/prro_gateway/services/write_path.py` (~рядок 269)  
**Зона:** 2  
**Статус:** not tested  
**Опис:** LND виділяється до відправки. При failure LND витрачений. Немає коментаря чи invariant що пояснює DPS-tolerance до gaps в нумерації. DPS spec дозволяє gaps (offline-документи створюють gaps природним чином), але це ніде не задокументовано в коді.

**Пропозиція:** Додати коментар в `increment_lnd` з поясненням що gaps є допустимими.

---

### [LOW] DUPLICATE_EXCISE_MARK race через різні ingress

**Файл:** write_path.py + inbox.py  
**Зона:** 2  
**Статус:** SAFE  
**Опис:** UNIQUE constraint на DB рівні для excise marks + single-writer per FN виключає race. Application-level check достатній.

---

## Зона 3 — Offline Sequence Completeness

### [MEDIUM] Stale state machine документація — GOING_OFFLINE/GOING_ONLINE/OPENING/CLOSING

**Файл:** `src/prro_gateway/enums.py:92,94`, `sql/001_hot_store_init.sql:199-201`  
**Зона:** 3  
**Статус:** verified  
**Опис:** `GOING_OFFLINE`, `GOING_ONLINE`, `OPENING`, `CLOSING` визначені в enum та схемі, описані в CLAUDE.md і Multi-Protocol spec. Але жодний `update_mode` або session update ніколи їх не записує. Перехід `ONLINE → OFFLINE` є атомарним. Перехід сесії `create → OPEN → CLOSED` теж атомарний.

Документація стан-машини є стейлою і може ввести в оману майбутніх розробників.

**Пропозиція:** Прибрати GOING_OFFLINE/GOING_ONLINE/OPENING/CLOSING з документації або додати explicit "NOT USED" коментар в enum.

---

### [MEDIUM] Orphaned OPEN offline session після краш — відновлення відсутнє

**Файл:** `src/prro_gateway/runtime/supervisor.py:38–74`, `offline.py:58–97`  
**Зона:** 3  
**Статус:** verified  
**Опис:** Краш при mode=OFFLINE залишає OPEN offline session в DB. `supervisor.py` виконує тільки migrations + `reconcile_pending` — немає recovery для orphaned sessions. На рестарті mode залишається OFFLINE (persisted), тому GO_OFFLINE буде заблокований через mode check — це коректно. Але накопичений elapsed time сесії рахується від `started_at` до нескінченності, включаючи час downtime. Результат: `current_month_offline_seconds` буде завищений (включає час коли сервіс не працював), що може призвести до передчасного BLOCKED через місячний ліміт.

**Пропозиція:** В `supervisor.py` додати перевірку: якщо є OPEN session і mode=OFFLINE, і процес щойно стартував — записати час downtime як "паузу" або обнулити `last_heartbeat_at` щоб уникнути накопичення.

---

### [MEDIUM] X_REPORT з unsynced offline backlog — misleading totals

**Файл:** `src/prro_gateway/services/write_path.py:1278–1294`  
**Зона:** 3  
**Статус:** verified  
**Опис:** `OFFLINE_BACKLOG_NOT_SYNCED` gate блокує SHIFT_CLOSE і Z_REPORT (крім mode=OFFLINE). Але X_REPORT не блокується. Результат: X_REPORT повертає поточні running totals без offline документів — цифри неповні. X_REPORT є нефіскальним документом, юридичного порушення немає, але оператор може прийняти неправильне рішення на основі хибних totals.

**Пропозиція:** Додати warning field в X_REPORT response коли є unsynced backlog, або додати informational error.

---

### [LOW] Timer race — offline limit check лише на початку

**Файл:** `write_path.py:251–259, 1379–1417`  
**Зона:** 3  
**Статус:** DESIGN_DECISION  
**Опис:** `_check_offline_limits` перевіряє місячний ліміт на початку обробки. Немає re-check на момент коміту. Гонка виключена single-writer invariant (FI-2) — concurrent обробка одного FN неможлива. Дрейф між check і commit становить <100ms для passthrough. **Прийнятно.**

---

### [LOW] Offline sync — залежність від DPS idempotency на offline_fiscal_no

**Файл:** `src/prro_gateway/services/offline_sync.py:133–190`  
**Зона:** 3  
**Статус:** DESIGN_DECISION  
**Опис:** Після краш під час sync, вже відправлені документи будуть відправлені повторно. Local guard через `expected_states=(OFFLINE_LOCAL_ACK,)` захищає від подвійного оновлення стану. Захист від подвійної реєстрації на DPS покладається на `offline_fiscal_no` як idempotency key на стороні DPS. Це стандартна практика для фіскальних систем і відповідає DPS spec.

---

## Зона 4 — Crypto Sidecar Failure Modes

### [CRITICAL] SIGNED-стан після краш — дублікат документа з новим LND

*(Дубльовано в Зоні 2 — consolidated finding)*

**Файл:** `src/prro_gateway/services/write_path.py:230–239, 416–563`  
**Зона:** 4  
**Статус:** verified  
**Деталі:** Немає resume branch для `state=SIGNED`. `_stage_sign` немає guard `if document.state == SIGNED: skip`. На retry після краш між SIGNED commit і SENT commit: `create_prepared` потрапляє в `UNIQUE` constraint на `request_id` → некероване виключення, або якщо немає constraint — новий LND.

---

### [HIGH] CRYPTO_DEGRADED — постійний без рестарту, немає half-open

**Файл:** `src/prro_gateway/services/write_path.py:418–451, 531–537`, `write_path.py:107–111`  
**Зона:** 4  
**Статус:** verified  
**Опис:** Після відкриття breaker (`_crypto_consecutive_failures >= breaker_threshold`):
- `_stage_sign` повертає помилку одразу, без виклику sidecar (рядок ~418–451)
- `_crypto_consecutive_failures` ніколи не скидається (немає успішного sign → немає reset на рядку 531)
- `NodeStateRepository.update_mode(..., CRYPTO_DEGRADED)` записано в DB
- `CRYPTO_DEGRADED` залишається назавжди

Метод `reset_crypto_breaker()` існує (рядки 107–111), але немає evidence що він підключений до API endpoint або ops_tick.

**Сценарій відтворення:**
1. Sidecar недоступний 5 секунд → breaker відкривається, mode = CRYPTO_DEGRADED
2. Sidecar відновлюється
3. Система не відновлюється автоматично — всі подальші документи отримують помилку
4. Потрібен ручний рестарт процесу

**Пропозиція:** Додати half-open probe в `_ops_tick` або підключити `reset_crypto_breaker()` до management API endpoint.

---

### [HIGH] Два коміти при CRYPTO_DEGRADED → втрата стану breaker на краш

**Файл:** `src/prro_gateway/services/write_path.py:494–507, 430–450`  
**Зона:** 4  
**Статус:** verified  
**Опис:** Після failed sign:
1. Перший commit: document і inbox оновлюються в `_mark_document_and_inbox_error`
2. **Вікно краш тут**
3. Другий commit: `node_state.mode = CRYPTO_DEGRADED`

При краші між першим і другим комітом документ коректно позначений як ERROR_RETRYABLE, але node mode залишається ONLINE, а `_crypto_consecutive_failures` скидається (in-memory). Наступний старт продовжує без CRYPTO_DEGRADED, threshold counter починається з нуля.

**Пропозиція:** Зберігати `consecutive_crypto_failures` в `node_state` (persistent), або об'єднати обидва update в одну транзакцію.

---

### [MEDIUM] Sidecar timeout + retry → два паралельних sign запити

**Файл:** `src/prro_gateway/runtime/providers.py:45–65, 77–95`, `write_path.py:490`  
**Зона:** 4  
**Статус:** inferred (частково verified)  
**Опис:** При `TimeoutError` executor thread продовжує виконуватись у фоні. На retry відправляється новий sign запит. Можливі два паралельних запити до sidecar для одного документа. На JSON-шляху передається `document_id`, що дозволяє sidecar deduplicate. На `sign_raw` шляху (DPS) передається тільки `payload_base64` — без будь-якого idempotency key. Подвійний підпис DPS-документа можливий.

**Пропозиція:** На шляху `sign_raw` передавати `document_id` як idempotency hint. Уточнити sidecar contract щодо deduplication.

---

### [MEDIUM] require_local_sign: false в production — конфігураційний bypass

**Файл:** `src/prro_gateway/services/write_path.py:320–330`, `runtime/container.py:710–755`  
**Зона:** 4  
**Статус:** DESIGN_DECISION / operational risk  
**Опис:** `_enforce_production_crypto_gate` блокує `PassthroughCryptoProvider` в production. Але `require_local_sign: false` в transport profile теж обходить підписання — і це не блокується gate. Будь-який profile з `require_local_sign: false` в prod DB дозволяє відправку без підпису.

Крім того, gate спрацьовує тільки якщо `environment=production` явно задане. Default = `development`. Deployment без явного env flag мовчки обходить gate.

---

### [LOW] Flapping sidecar ніколи не відкриває breaker

**Файл:** `write_path.py:489, 510, 531`  
**Зона:** 4  
**Статус:** DESIGN_DECISION  
**Опис:** Counter скидається на кожен успіх. Pattern fail-success-fail-success тримає counter між 0 і 1, ніколи не досягає threshold. Breaker захищає від sustained outage, не від intermittent degradation.

---

## Зведена таблиця

| Зона | Severity | Issue | Тест є? | Статус |
|------|----------|-------|---------|--------|
| 1 | CRITICAL | CRYPTO_DEGRADED — немає fast-path guard | ні | verified |
| 2/4 | CRITICAL | SIGNED-стан crash → дублікат документа + новий LND | ні | verified |
| 1 | HIGH | GOING_OFFLINE/GOING_ONLINE — enum/doc stale, не записуються | ні | verified |
| 1 | HIGH | BLOCKED + GO_OFFLINE → NODE_ALREADY_OFFLINE (хибний error code) | ні | verified |
| 2 | HIGH | Crash після SIGNED → IntegrityError або orphaned document | ні | verified |
| 4 | HIGH | CRYPTO_DEGRADED — постійний без рестарту, немає half-open | ні | verified |
| 4 | HIGH | Два коміти → breaker state втрачається при краші | ні | verified |
| 2 | MEDIUM | LND gap — відсутній коментар про DPS tolerance | ні | not tested |
| 3 | MEDIUM | Orphaned OPEN session — downtime включається в offline-час | ні | verified |
| 3 | MEDIUM | X_REPORT з unsynced backlog — misleading totals | ні | verified |
| 3 | MEDIUM | GOING_OFFLINE/OPENING/CLOSING — stale state machine doc | ні | verified |
| 4 | MEDIUM | sign_raw timeout+retry → подвійний підпис DPS | ні | inferred |
| 4 | MEDIUM | require_local_sign: false + environment default → bypass | ні | verified |
| 2 | LOW | DUPLICATE_EXCISE_MARK — SAFE за рахунок UNIQUE + FI-2 | SAFE | verified |
| 2 | LOW | allocate_z_report_number — SAFE за рахунок BEGIN IMMEDIATE | SAFE | verified |
| 2 | LOW | Inbox idempotency бар'єр між ingress — SAFE | SAFE | verified |
| 3 | LOW | Timer race vs limit — SAFE за рахунок FI-2 | SAFE | verified |
| 3 | LOW | Offline sync — DPS idempotency on offline_fiscal_no | DESIGN_DECISION | verified |
| 4 | LOW | Flapping sidecar — breaker не відкривається | DESIGN_DECISION | verified |

---

## Пріоритизований список fixes

### Блок 1 — КРИТИЧНІ (мають бути виправлені до пілоту)

1. **SIGNED-стан resume path** (`write_path.py:230–239`)  
   Додати гілку `elif _existing.state in {SIGNED, ENCRYPTED}: resume_send`  
   Це виключає duplicate LND при crash-retry.

2. **CRYPTO_DEGRADED fast-path guard** (`write_path.py` після BLOCKED guard)  
   Аналогічно STOP_MODE — explicit reject всіх fiscal ops в CRYPTO_DEGRADED.

### Блок 2 — HIGH (мають бути виправлені до першого production deployment)

3. **Half-open probe або operator API для crypto breaker recovery** (`providers.py` / `ops_tick`)

4. **Persistent consecutive_crypto_failures** або одна транзакція для document+node mode update

5. **BLOCKED + GO_OFFLINE error code** → замінити NODE_ALREADY_OFFLINE на BLOCKED_MODE_ACTIVE

### Блок 3 — MEDIUM (backlog, не блокують пілот але потребують рішення)

6. **Orphaned session downtime** — supervisor recovery

7. **X_REPORT warning** при unsynced backlog

8. **sign_raw idempotency key** — передавати document_id

9. **Стейл документація** — прибрати GOING_OFFLINE/GOING_ONLINE/OPENING/CLOSING або позначити NOT_USED

10. **require_local_sign + environment default** — документувати і додати deployment checklist

---

*Всі findings позначені як verified (знайдено в коді) або inferred (логічний висновок). Жодне finding не базується тільки на назвах методів — всі трасовані через реальні code paths.*
