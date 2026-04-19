# P0 Audit — PRRO Gateway: State Machine, Idempotency, Offline, Crypto

**Призначення:** самодостатній промпт для запуску аудиту субагентом або в окремій сесії.  
**Аудитор:** read-only, звіт без правок. Правки — окремий крок після затвердження.

---

## Контекст системи

**PRRO Gateway** — локальний фіскальний шлюз для України. Edge-система з юридичними та фінансовими наслідками за некоректну роботу. SQLite WAL, один writer per `fiscal_number`, Python 3.12.

**Package root:** `src/prro_gateway/`

**Критичний файл:** `src/prro_gateway/services/write_path.py` (~2100 рядків)  
**Стейт-машини:**
- Document: `PREPARED → SIGNED → ENCRYPTED → SENT → KVT1 → KVT2 → ACK` / `REJECTED` / `ERROR_RETRYABLE` / `REQUIRES_MANUAL_RECONCILIATION`
- Node: `ONLINE / GOING_OFFLINE / OFFLINE / GOING_ONLINE / BLOCKED / STOP_MODE / CRYPTO_DEGRADED`
- Shift: `CREATED → OPENING → OPENED → CLOSING → CLOSED / ERROR`
- Offline session: `OPENING → OPEN → CLOSING → CLOSED / ABORTED`

**Frozen invariants (не можуть бути порушені):**
1. Жодних мережевих або crypto-викликів всередині довгих SQLite write-транзакцій
2. Один `fiscal_number` = один логічний single-writer
3. Channel switch заборонений при відкритій зміні
4. Ідемпотентність обов'язкова
5. Offline поважає часові та кодові ліміти
6. Adapters мають будувати повні canonical payloads

---

## Scope аудиту — 4 зони

---

### Зона 1: State machine completeness (write_path.py)

**Мета:** знайти комбінації `(NodeMode × OperationType)` де немає явного reject і система або тихо продовжує, або падає з неочікуваним стейтом.

**Що читати:**
- `src/prro_gateway/services/write_path.py` — весь файл
- `src/prro_gateway/enums.py` — `NodeMode`, `OperationType`
- `src/prro_gateway/services/write_path.py` — метод `_guard_preconditions` (~рядок 1141)

**Конкретні питання:**

1. **STOP_MODE guard** (рядок ~162): блокує все крім `GET_STATUS`. Але `GET_STATUS` не створює `fiscal_document` — чи правильно обробляється ця гілка далі? Чи є `ctx.document` None коли `GET_STATUS` проходить через STOP_MODE guard?

2. **CRYPTO_DEGRADED mode** (рядок ~245 і ~449): при `CRYPTO_DEGRADED` нода переходить у degraded після crypto-failure. Які операції допустимі в цьому стані? Чи є явний fast-path guard аналогічний BLOCKED/STOP_MODE, чи CRYPTO_DEGRADED просто "потрапляє" в _guard_preconditions і там обробляється? Що відбувається з `SHIFT_OPEN` при CRYPTO_DEGRADED?

3. **GOING_OFFLINE / GOING_ONLINE transitional states**: ці стани — перехідні. Що відбувається якщо прийде `SELL` поки нода в `GOING_OFFLINE`? Чи є guard? Якщо так — де саме? Якщо ні — чи може документ бути створений в некоректному контексті?

4. **BLOCKED mode fast-path** (рядок ~171): дозволяє `GO_OFFLINE`, `GO_ONLINE`, `ASK_OFFLINE_CODES`, `GET_STATUS`. Чи коректно що `GO_OFFLINE` можливий з BLOCKED? Якщо нода BLOCKED через перевищення місячного ліміту, і оператор викликає GO_OFFLINE — що відбувається з offline session? Чи є перевірка що нода не в OFFLINE вже?

5. **Матриця uncovered paths:** побудуй повну таблицю:
   ```
   | NodeMode        | OperationType    | guard? | result |
   |-----------------|------------------|--------|--------|
   | GOING_OFFLINE   | SELL             | ?      | ?      |
   | GOING_OFFLINE   | SHIFT_CLOSE      | ?      | ?      |
   | GOING_ONLINE    | Z_REPORT         | ?      | ?      |
   | CRYPTO_DEGRADED | SHIFT_OPEN       | ?      | ?      |
   | CRYPTO_DEGRADED | GO_OFFLINE       | ?      | ?      |
   | BLOCKED         | SHIFT_CLOSE      | ?      | ?      |
   | BLOCKED         | Z_REPORT         | ?      | ?      |
   ```
   Заповни реальними значеннями з коду. Позначай як `EXPLICIT_REJECT` / `FALLS_THROUGH` / `UNCLEAR`.

6. **Shift state vs Node mode desync**: чи може `node_state.mode == ONLINE` при `shift.state == CLOSING`? Якщо так — що відбувається з `SELL` в такому стані? Де перевіряється shift state в `_guard_preconditions`?

---

### Зона 2: Idempotency end-to-end

**Мета:** знайти вікно між `SENT` і `ACK` де retry може створити дублікат на стороні DPS.

**Що читати:**
- `src/prro_gateway/repositories/inbox.py` — `accept_command`, `get_by_idempotency_key`
- `src/prro_gateway/services/write_path.py` — `_stage_send_or_offline`, `_stage_finalize_ack`
- `src/prro_gateway/services/reconciliation.py` — весь файл
- `src/prro_gateway/repositories/fiscal_documents.py` — методи update_state

**Конкретні питання:**

1. **Inbox-level barrier**: `accept_command` перехоплює duplicate `idempotency_key` через `UNIQUE` constraint і повертає existing request_id. Але що якщо той самий запит прийде через **інший ingress протокол** (наприклад, REST і XML-RPC одночасно)? `idempotency_key` однаковий — чи barrier спрацює, чи є race де обидва запити пройдуть accept?

2. **Post-SENT window**: документ переходить в `SENT`. Перед тим як прийде `KVT1`/`ACK` процес крашується. При reconciliation — чи перевіряє `reconcile_pending` що документ **вже не відправлений** на сторону DPS перед повторним відправленням? Де ця перевірка? Чи є `correlation_id` або інший механізм на стороні DPS щоб deduplicate?

3. **`allocate_z_report_number` atomicity** (`repositories/node_state.py`): використовується `RETURNING next_z_report_number - 1`. Чи є вікно між `UPDATE` і `RETURNING` де два concurrent writer'и можуть отримати однаковий номер? (Технічно неможливо в SQLite з WAL + BEGIN IMMEDIATE, але потрібна явна перевірка що ця функція завжди викликається всередині транзакції.)

4. **`increment_lnd` в offline режимі**: LND виділяється до відправки. Якщо відправка falls — LND вже витрачений. Чи є механізм повернення/повторного використання? Чи це задокументована втрата (gap в нумерації)? Що каже DPS spec про gaps?

5. **DUPLICATE_EXCISE_MARK race**: якщо два SELL з однаковим excise mark прийдуть одночасно через різні ingress — перша транзакція ставить mark як `SOLD`, але якщо обидва пройдуть `_guard_preconditions` до коміту першої — чи є constraint на рівні DB що спіймає це, чи тільки application-level check?

---

### Зона 3: Offline sequence completeness

**Мета:** знайти edge cases де offline-послідовність ламається і документи або губляться, або лімітні перевірки обходяться.

**Що читати:**
- `src/prro_gateway/services/write_path.py` — методи `_handle_go_offline`, `_handle_go_online`, `_stage_send_or_offline` (рядки ~1380–1540)
- `src/prro_gateway/repositories/offline.py` — весь файл
- `src/prro_gateway/services/offline_sync.py` — весь файл
- `src/prro_gateway/enums.py` — `OfflineSessionState`

**Конкретні питання:**

1. **Таймер vs. операція в процесі**: перевірка місячного ліміту (`current_month_offline_seconds`) відбувається на початку `_stage_send_or_offline`. Якщо ліміт перевищується **між** початком і кінцем обробки документа (тобто документ починається in-limit, але під час обробки — вже over-limit) — що відбувається? Чи є перевірка на момент фіналізації, а не тільки на початку?

2. **Offline codes exhaustion**: `next_lnd` збільшується атомарно. Але чи є перевірка що `next_lnd <= max_lnd_for_offline_session` до виділення коду? Де ця перевірка? Що відбувається якщо кодів рівно 0 залишилось — чи повертається `OFFLINE_CODES_EXHAUSTED` до того як LND виділено, чи після?

3. **`OFFLINE_BACKLOG_NOT_SYNCED` gate**: де саме перевіряється наявність unsynced offline documents? Чи охоплює ця перевірка всі операції, чи тільки деякі `OperationType`? Конкретно — чи блокує вона `SHIFT_CLOSE` якщо є unsynced backlog? (Це критично: Z_REPORT не можна генерувати з "дирами" в нумерації.)

4. **Crash під час GO_ONLINE**: нода в `GOING_ONLINE`. Процес падає. При наступному старті — яка логіка відновлення? Чи є в startup supervisor перевірка transitional states (`GOING_ONLINE`, `GOING_OFFLINE`) і їх reset? Де?

5. **Orphan offline session**: `OfflineSessionState.OPENING` або `CLOSING` при старті — чи є explicit recovery, чи нода назавжди залишається в перехідному стані?

6. **Offline sync idempotency**: `offline_sync.py` відправляє offline документи на DPS після відновлення з'єднання. Якщо sync перервано після часткового відправлення — чи повторний sync безпечний? Чи може один документ бути відправлений двічі?

---

### Зона 4: Crypto sidecar failure modes

**Мета:** знайти стани де sidecar failure призводить до застрягання документів або некоректних mode transitions.

**Що читати:**
- `src/prro_gateway/runtime/providers.py` — `SidecarCryptoProvider`, `SidecarCryptoClient`
- `src/prro_gateway/services/write_path.py` — `_stage_sign`, рядки ~416–560
- `src/prro_gateway/config.py` — `CryptoConfig`

**Конкретні питання:**

1. **Breaker threshold**: `breaker_threshold` (default 5) — після N consecutive failures відкривається breaker і нода переходить в `CRYPTO_DEGRADED`. Чи "consecutive" означає підряд без жодного успіху, чи є вікно часу? Що якщо failures чергуються з successes — breaker ніколи не відкриється?

2. **Half-open state**: чи є half-open state у breaker? Тобто після N секунд після відкриття breaker пробує один запит. Якщо так — де цей timeout визначений? Якщо ні — як оператор може відновити CRYPTO_DEGRADED без рестарту сервісу?

3. **Документ в SIGNED, sidecar впав**: документ підписаний (є підпис у `signed_xml`), але ще не відправлений (`SENT`). Sidecar впав. При наступному `process_next` — чи перевіряється що підпис вже є і sign stage пропускається? Або документ буде підписаний повторно (можливо з іншим підписом → some DPS reject)?

4. **`_requires_local_sign` logic** (рядок ~126): де визначається чи потрібен локальний підпис? Чи може профіль транспорту вказати `no_sign`, і якщо так — чи є тест що в production (non-development environment) цей bypass неможливий без явного config?

5. **Sidecar timeout vs. transaction**: виклик sidecar відбувається **поза** транзакцією (інваріант 1). Але якщо `timeout_seconds=5` і sidecar відповідає через 4.9 секунди — чи є вікно де Python-side timeout спрацьовує, але sidecar вже почав обробку і поверне відповідь в нікуди? Чи є retry після timeout і якщо є — чи може це призвести до подвійного підпису?

---

## Формат звіту

Для кожного знайденого issue:

```
### [CRITICAL|HIGH|MEDIUM|LOW] Назва проблеми

**Файл:** path/to/file.py:LINE
**Зона:** 1 / 2 / 3 / 4
**Опис:** що саме не так, яка інваріанта порушується
**Сценарій відтворення:** мінімальна послідовність дій що призводить до проблеми
**Поточне покриття тестами:** є тест / немає тесту / тест є але не доводить (false positive)
**Пропозиція:** конкретний fix або перевірка
```

В кінці — зведена таблиця:
```
| Зона | Severity | Issue | Тест є? |
|------|----------|-------|---------|
```

---

## Що НЕ входить в scope цього аудиту

- Адаптери (REST/XML-RPC/Maria) — окремий аудит
- SQL injection / input validation — окремий аудит
- Migrations — окремий аудит
- Backup/retention — вже проаудитовано (2026-04-16)
- Performance / profiling

---

## Preconditions для запуску

1. Прочитати файли перед тим як робити висновки — не покладатися на назви методів
2. Трасувати реальні code paths, а не тільки docstrings
3. Якщо знайдено неочевидний баг — навести конкретний рядок і minimal trace
4. Позначати: **verified** (знайдено в коді), **inferred** (логічний висновок), **not tested** (підозра без підтвердження)
5. Пріоритет: CRITICAL і HIGH findings повинні мати конкретний рядок коду
