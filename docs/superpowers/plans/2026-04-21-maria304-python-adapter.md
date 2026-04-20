# Maria 304 — Python-сторона (M7-Py)

**Дата:** 2026-04-21
**Стан:** draft, чекає згоди оператора
**Мета:** підключити Rust `maria304_driver` до Python write-path через новий
адаптер `maria304_native` + `POST /v1/ingress/maria304`. Rust-бінар уже
б'ється в цей URL із bearer-token'ом, але ендпоінта немає — це блокер
пілота.

---

## 1. Скоуп і не-скоуп

### Входить
- Протокол `Protocol.MARIA_304_NATIVE` в `enums.py`.
- Адаптер `Maria304NativeAdapter` у `adapters/maria304_native.py`.
- Метод `accept_maria304(...)` в `IngressAcceptService` — 4-tuple (як `accept_checkbox`), бо Rust-сторона очікує синхронну відповідь із `document_state` / `fiscal_doc_number`.
- Ендпоінт `POST /v1/ingress/maria304` у `runtime/rest_app.py`.
- Bearer-token аутентифікація (єдиний shared secret із Rust-конфіга) через `fastapi.Depends`.
- Конфігурація: `Maria304IngressConfig` у `config.py` (shared_token, response_timeout_seconds).
- Повна петля: CSIN1→SHIFT_OPEN→SELL→Z_REPORT через Rust-бінар, Python дає `ACK/KVT2` назад.

### Не входить
- CANCEL-операція. Rust може надіслати `command_type="CANCEL"`, але на нашому боці це не сюрвайвить write-path (немає `OperationType.CANCEL`). **Тимчасове рішення:** адаптер відхиляє CANCEL з `UNSUPPORTED_METHOD`. Rust-сторона і так сама очищає незафіскалений чек локально — CANC не повинен долітати до Python. Перевірити в M7-Py-4.
- `dual_tax_mode` — лишаємо як annotation у `payload["receipt"]["dual_tax_mode"]`, write-path його не читає. Якщо в пілоті з'явиться вимога читати — окремий sprint.
- Admin API розширення (force-close, listener reload) — окремо.
- Маппінг `raw_frames` на локальний архів/трейс — у пілоті тримаємо їх у `payload["receipt"]["raw_frames"]`, не піднімаємо в окрему таблицю.

---

## 2. Нерозв'язані питання (вирішити перед стартом)

1. **Bearer token: де живе?**
   - Варіант A: `config.ingress.maria304.shared_token` у YAML + env-override `${MARIA304_BRIDGE_TOKEN}`. Один токен на весь gateway. ✅ моя рекомендація — дзеркалить Rust-конфіг.
   - Варіант B: per-FN у таблиці `fn_config`. Гнучкіше, але зайве для пілоту 1–5 кас.
2. **Форма відповіді.** Rust чекає поля: `status` (ACK/REJECTED/SOFTBLOCK/ERROR_*), `document_state` (KVT1/KVT2/…), `fiscal_doc_number`, `message`, `correlation_id`. Що віддаємо, якщо `_maybe_process` повертає inbox у статусі `NEW` (воркер ще не дожував)?
   - Пропозиція: якщо `process_result is None` або документ ще не у фінальному стані → `status="SOFTBLOCK"`, `message="pending"`. Rust це коректно трансформує в `SOFT_PROCESSING` фрейм і 1С повторить.
3. **CANCEL.** Чи Rust-дрівер **реально** шле `command_type="CANCEL"` у HTTP, чи тільки чистить локально? Потрібно прочитати `rust/maria304_driver/src/session/dispatcher.rs::build_canonical` — якщо там гілка `CommandType::Cancel` будує envelope, то адаптер має хоч щось із ним робити, не просто `UNSUPPORTED_METHOD`.

---

## 3. Фазування

### M7-Py-1 — типи й адаптер (без мережі)
- `enums.py`: додати `Protocol.MARIA_304_NATIVE`.
- `config.py`: `Maria304IngressConfig(BaseModel)` з полями `shared_token: SecretStr`, `response_timeout_seconds: int = 10`; додати у `IngressConfig`.
- `adapters/maria304_native.py`: `Maria304NativeAdapter.map_command(raw) -> CanonicalFiscalCommand`.
  - `raw` = декодований Rust `CanonicalCommand` (див. `rust/maria304_driver/src/bridge/dto.rs`).
  - Мапінг:
    - `command_type` → `OperationType` (SELL/RETURN/SHIFT_OPEN/SHIFT_CLOSE/X_REPORT/Z_REPORT). CANCEL → `AdapterMappingError(code="UNSUPPORTED_METHOD")`.
    - `fiscal_number`, `correlation_id` → `AdapterContext`.
    - `receipt.payment_kind` ({"CASH","CASHLESS_1","CASHLESS_2",…}) → `PaymentType` (CASH/CASHLESS/MIXED/OTHER).
    - `receipt.totals` → `payload["receipt"]["totals"]`.
    - `receipt.raw_frames` → `payload["receipt"]["raw_frames"]` (зберігаємо для аудиту).
    - `receipt.dual_tax_mode` → `payload["receipt"]["dual_tax_mode"]`.
    - `identity` → `payload["identity"]`.
    - `external_request_id` = `correlation_id` (використовуємо як idempotency-hint).
- **Юніт-тести** (`tests/adapters/test_maria304_native.py`):
  - SHIFT_OPEN/CLOSE → правильний `OperationType` і мінімальний `payload`.
  - SELL із товарами + CASH payment → `payload` має `receipt.goods`, `receipt.payments`, `raw_frames`.
  - RETURN без `related_receipt_id` → пропускаємо (DPS теж пускає); перевіряємо що envelope валідний.
  - CANCEL → `AdapterMappingError("UNSUPPORTED_METHOD")`.
  - schema_version стемпнутий, idempotency_key детермінистичний від `correlation_id`.

**Acceptance M7-Py-1:** `pytest tests/adapters/test_maria304_native.py -v` зелене, `mypy src/prro_gateway/adapters/maria304_native.py` чисто.

### M7-Py-2 — сервіс + ендпоінт + auth
- `services/ingress.py`:
  - `self.maria304_adapter = Maria304NativeAdapter()` у `__init__`.
  - `accept_maria304(self, conn, *, raw_request, response_timeout_seconds) -> tuple[InboxRecord, CanonicalFiscalCommand, Any, bool]` — копія `accept_checkbox`, просто інший адаптер.
  - `DEFAULT_RESPONSE_TIMEOUT_MARIA304_SECONDS` у `constants.py` (дефолт 10с, як у Rust бріджа request_timeout).
- `runtime/rest_app.py`:
  - `_require_maria304_token(request: Request) -> None` (FastAPI Depends). Читає `Authorization: Bearer <token>`, compare з `container.config.ingress.maria304.shared_token` через `hmac.compare_digest`. Відхиляє 401 якщо відсутній, 403 якщо не збігається.
  - `@app.post("/v1/ingress/maria304")` — віддає JSON у формі Rust `CanonicalResponse` (fields: `status`, `document_state`, `fiscal_doc_number`, `correlation_id`, `message`).
  - Мапінг `process_result` → відповідь:
    - Документ у `KVT2/ACK` → `status="ACK"`, `document_state="KVT2"`, `fiscal_doc_number=document.server_fiscal_no`.
    - `REJECTED/CANCELLED` → `status="REJECTED"`, `document_state=state`, `message=error`.
    - `ERROR_RETRYABLE` або inbox у `NEW/PROCESSING` → `status="SOFTBLOCK"`, `message="pending"`.
- `runtime/container.py`: нічого не міняємо (`IngressAcceptService` уже живе в контейнері).
- **Контракт-тести** (`tests/runtime/test_maria304_endpoint.py`, FastAPI `TestClient`):
  - 401 без заголовка, 403 на невалідному токені (constant-time).
  - 200 + правильна shape на SHIFT_OPEN-happy.
  - 400 + `UNSUPPORTED_METHOD` на CANCEL.
  - idempotency: повторний POST з тим же `correlation_id` → той самий `inbox.request_id`, `is_replay=true` не ламає відповідь.

**Acceptance M7-Py-2:** `pytest tests/runtime/test_maria304_endpoint.py -v` зелене, `mypy` на зміненому коді чисто, ручний `curl -H "Authorization: Bearer …" … /v1/ingress/maria304` на запущеному gateway повертає 200 з очікуваною shape.

### M7-Py-3 — e2e з Rust-драйвером (пілот-парі)
- Окремий `tests/integration/test_maria304_rust_pilot.py` який:
  1. Піднімає FastAPI в TestClient + підмінений `CommandProcessor` (in-memory SQLite write-path через conftest).
  2. Стартує `cargo run --release -p maria304_driver -- --config <tmp>` у фікстурі (або робить skip якщо `cargo` недоступний — pytest marker `requires_cargo`).
  3. Пише Maria 304 wire-фрейми в TCP-сокет дрівера, зчитує відповіді, перевіряє що в інбоксі Python з'явилися канонічні команди у правильній послідовності.
- Fallback якщо e2e занадто важкий: **записаний wire-log** (reproduction-fixture) → подача напряму у Rust-дрівер через `maria304_driver::listener::session_loop::run_connection` як lib-call з `MockBridge` замість HTTP. Але це не ловить HTTP-рівень.
- Сценарій пілоту: CSIN1 → UPAS (login) → ZREP open → CSHN (SELL CASH 100.00 ПДВ20) → CSHZ (Z-report) → UPAS logoff. Очікуємо у Python: 1×SHIFT_OPEN (ACK), 1×SELL (ACK+KVT2), 1×Z_REPORT (ACK).

**Acceptance M7-Py-3:** повна послідовність пройшла, Python-архів має 3 документи в `KVT2`.

### M7-Py-4 — ревізія CANCEL
- Прочитати `rust/maria304_driver/src/session/dispatcher.rs` гілку CANC → якщо Rust реально шле CANCEL у HTTP (а не тільки очищає локально), запроєктувати окрему під-задачу: `OperationType.CANCEL`, обробник у `write_path`, або гарантувати на Rust-боці що CANC ніколи не конвертується в CanonicalCommand.
- Це гілка "якщо": в кращому разі пункт закривається одним коміт-правкою на Rust-стороні (не будувати envelope для CANC).

---

## 4. Порушені інваріанти

Перевіряємо для кожної фази:

| # | Інваріант | Вплив M7-Py |
|---|-----------|-------------|
| 1 | Без crypto/network у довгих SQLite tx | не зачіпаємо, адаптер — чиста функція |
| 2 | 1 fiscal_number = один writer | не зачіпаємо, використовуємо існуючий `_maybe_process` |
| 3 | Channel switch заборонено при відкритій зміні | не зачіпаємо |
| 4 | Idempotency обов'язкова | **критично** — `correlation_id` з Rust стає `external_request_id`, це джерело ключа |
| 5 | Offline limits | не зачіпаємо (у M7-Py нема offline гілки) |
| 6 | Адаптери будують full payload | **пильно** — `raw_frames` зберігаємо повністю, totals витягуємо з Rust |
| 7 | `schema_version` на envelope | `CanonicalEnvelopeBuilder.build` це робить автоматично |
| 8 | Recovery/reconciliation без silent violations | не зачіпаємо |
| 9 | Graceful shutdown | не зачіпаємо |
| 10 | Checkbox-compat bypass тільки через конфіг | не зачіпаємо, Maria 304 має власний backend-profile |

---

## 5. Файли

### Нові
- `src/prro_gateway/adapters/maria304_native.py`
- `tests/adapters/test_maria304_native.py`
- `tests/runtime/test_maria304_endpoint.py`
- `tests/integration/test_maria304_rust_pilot.py` (опційно, marker)

### Змінені
- `src/prro_gateway/enums.py` — `Protocol.MARIA_304_NATIVE`
- `src/prro_gateway/config.py` — `Maria304IngressConfig`
- `src/prro_gateway/constants.py` — `DEFAULT_RESPONSE_TIMEOUT_MARIA304_SECONDS`
- `src/prro_gateway/services/ingress.py` — `accept_maria304`
- `src/prro_gateway/runtime/rest_app.py` — endpoint + `_require_maria304_token`
- `ops/config.example.yaml` — секція `ingress.maria304`

---

## 6. Ризики

1. **Rust-сторона шле поле, якого немає в мапінгу** → адаптер падає з `KeyError`. **Мітігація:** у `map_command` використовуємо `.get(...)` з явними дефолтами; невідомі поля в `payload["receipt"]["extras"]`.
2. **`correlation_id` колізія між різними Rust-інстансами** → duplicate idempotency key. **Мітігація:** Rust уже генерує UUIDv4 per-session, колізія астрономічно малоймовірна. Додатково — `external_request_id` префіксуємо `"mry304-"` щоб відрізнити від checkbox.
3. **Synchronous response блокує Rust-драйвер на повільному DPS** → 1С таймаутить. **Мітігація:** `response_timeout_seconds=10`; якщо write-path не встигає — повертаємо `SOFTBLOCK`, 1С і так ретраїть через 3с.
4. **Bearer token у plaintext YAML** → оператор коммітить його у приватний git. **Мітігація:** підтримуємо `${MARIA304_BRIDGE_TOKEN}` env-substitution (уже є в Rust); доки рекомендують systemd `EnvironmentFile`.

---

## 7. Послідовність робіт і контроль

Кожна фаза = гілка → PR → self-review → фікси → доказові тести → контрольний security-review → merge. Між фазами короткий чек-лист оператору "це готово, наступне — X".

Орієнтовний обсяг: M7-Py-1 ~2–3 год, M7-Py-2 ~3–4 год, M7-Py-3 ~4 год (залежить від Rust-e2e), M7-Py-4 ~30 хв (ревізія).

---

## 8. Після влиття

- Оновити `docs/maria304/INSTALL.md` крок "Verify": тепер `curl /v1/ingress/maria304` має повертати 200 на фейковий payload (не 404).
- Оновити `ops/config.example.yaml` з прикладом `ingress.maria304.shared_token`.
- У `docs/Multi-Protocol_PRRO_Gateway.md` додати рядок у таблицю "Supported ingress protocols".
