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
- **CANCEL-операція** — перевірено по `rust/maria304_driver/src/bridge/dto.rs`: `CommandType` = {Sell, Return, ShiftOpen, ShiftClose, XReport, ZReport, ServiceIn, ServiceOut, PeriodicReport}. **CANCEL Rust не шле** — CANC опкод у session dispatcher лише локально чистить receipt state без побудови envelope. Пункт M7-Py-4 закритий до старту.
- `dual_tax_mode` — лишаємо як annotation у `payload["receipt"]["dual_tax_mode"]`, write-path його не читає. Якщо в пілоті з'явиться вимога читати — окремий sprint.
- **Rich parse `raw_frames` → canonical goods/payments** — у M4 Rust надсилає `goods: []`, `payments: []` (порожні), повна інформація лише в `raw_frames`. Адаптер зберігає raw_frames у payload як є; нормалізації FISC/ARFI/... у canonical goods НЕ робимо. Це окремий sprint M7-Py-5+, якщо з'явиться потреба.
- Admin API розширення (force-close, listener reload) — окремо.

---

## 2. Точний контракт (звірено з Rust DTO)

**Запит (Rust → Python):** поля `CanonicalCommand` з `rust/maria304_driver/src/bridge/dto.rs`:
```
schema_version: str                  # "1.0"
fiscal_number: str
command_type: str                    # SELL | RETURN | SHIFT_OPEN | SHIFT_CLOSE | X_REPORT | Z_REPORT | SERVICE_IN | SERVICE_OUT | PERIODIC_REPORT
idempotency_key: str                 # "maria304:{fn}:{session_uuid}:{receipt_seq}[:opcode]"
cashier_id: str | None
department: str | None
return_check_number: str | None
payload: {
  direction: "SALE" | "RETURN"
  goods: [FiscalLine]                # порожні у M4, заповнюються пізніше
  payments: [CanonicalPayment]       # те саме
  dual_tax_mode: {tax_group_1, tax_group_2} | null
  totals: {sale_kopecks, return_kopecks}
  raw_frames: [{opcode, body}]       # завжди заповнені
}
```
Auth: `Authorization: Bearer <shared_token>`.

**Відповідь успіху (Python → Rust, 200 OK):** `CanonicalResponse`:
```
ok: bool                             # true
document_id: str                     # наш UUID/req_id
fiscal_id: str                       # DPS fiscal number (порожній якщо недоступний)
fiscal_ts: str                       # ISO8601
document_state: str                  # "ACK" | "KVT1" | "KVT2" | ...
sale_total_kopecks: u64              # для COMP payload; 0 для reports
return_total_kopecks: u64            # те саме
```

**Відповідь помилки (non-2xx):** body:
```
{"ok": false, "error_code": "SOFT_*", "error_message": "..."}
```
Rust очікує `error_code` починається з `"SOFT"` — інакше мапить у `SoftBlock`. Pending/voркер не дожував → відповідаємо 503 з `error_code="SOFT_PROCESSING"`.

### Вирішено до старту
1. **Bearer token** = `config.ingress.maria304.shared_token` у YAML + env-override `${MARIA304_BRIDGE_TOKEN}`. Один токен на весь gateway.
2. **Pending case** (`process_result` None або не фінальний стан) → HTTP 503 з `{"ok": false, "error_code": "SOFT_PROCESSING", "error_message": "worker pending"}`. 1С повторить через 3с cooldown.
3. **CANCEL** — знято зі скоупу, Rust не шле (звірено з CommandType enum у DTO).

---

## 3. Фазування

### M7-Py-1 — типи й адаптер (без мережі)
- `enums.py`: додати `Protocol.MARIA_304_NATIVE`.
- `config.py`: `Maria304IngressConfig(BaseModel)` з полями `shared_token: SecretStr`, `response_timeout_seconds: int = 10`; додати у `IngressConfig`.
- `adapters/maria304_native.py`: `Maria304NativeAdapter.map_command(raw) -> CanonicalFiscalCommand`.
  - `raw` = декодований Rust `CanonicalCommand` (див. `rust/maria304_driver/src/bridge/dto.rs`).
  - Context будуємо всередині адаптера з полів Rust (немає окремого `context` блоку, як у checkbox):
    - `AdapterContext(request_id=idempotency_key, fiscal_number=raw["fiscal_number"], business_ts=now_utc(), channel_owner="maria304-driver")`.
  - Мапінг:
    - `command_type` → `OperationType` через просту таблицю (SELL→SELL, SHIFT_OPEN→SHIFT_OPEN, X_REPORT→X_REPORT, Z_REPORT→Z_REPORT, SERVICE_IN→SERVICE_IN, SERVICE_OUT→SERVICE_OUT, PERIODIC_REPORT→`AdapterMappingError(code="UNSUPPORTED_METHOD")` поки не wire-нули).
    - `idempotency_key` з Rust → `external_request_id` (вже префіксований `"maria304:"` на Rust-боці; зберігаємо префікс).
    - `payload` (Rust ReceiptPayload) → `payload["receipt"]` плюс cashier_id/department/return_check_number на верхньому рівні `payload`.
    - `payload["receipt"]["raw_frames"]`, `payload["receipt"]["direction"]`, `payload["receipt"]["totals"]`, `payload["receipt"]["dual_tax_mode"]` — прямо з Rust.
    - `payload["receipt"]["goods"]`, `payload["receipt"]["payments"]` — прямо з Rust (у M4 порожні; залишаємо, не падаємо).
- **Юніт-тести** (`tests/test_adapter_maria304_native.py` — паттерн інших adapter-тестів):
  - SELL із порожніми goods/payments, але заповненим raw_frames → валідний envelope, protocol=`MARIA_304_NATIVE`, operation_type=`SELL`, raw_frames передані.
  - SELL із непорожніми goods+payments → зберігаються в payload.
  - RETURN → operation_type=`RETURN`.
  - SHIFT_OPEN/SHIFT_CLOSE/X_REPORT/Z_REPORT/SERVICE_IN/SERVICE_OUT → правильний OperationType; payload["receipt"] залишається (бо Rust завжди його шле).
  - PERIODIC_REPORT → `AdapterMappingError(code="UNSUPPORTED_METHOD")`.
  - Невідомий command_type ("ZZZZ") → `AdapterMappingError`.
  - idempotency_key з Rust → в `external_request_id`, `CanonicalEnvelopeBuilder` формує canonical idempotency як `{op}:{fiscal}:{external_request_id}`.
  - `schema_version` (з глобальної константи) стемпнутий на envelope незалежно від Rust-`schema_version` (Rust-`schema_version` — це версія Maria-протоколу, не canonical).
  - `dual_tax_mode` передається як є або лишається `None`.
  - Валідний мінімум: відсутність `cashier_id`/`department` не валить адаптер.

**Acceptance M7-Py-1:** `pytest tests/adapters/test_maria304_native.py -v` зелене, `mypy src/prro_gateway/adapters/maria304_native.py` чисто.

### M7-Py-2 — сервіс + ендпоінт + auth
- `services/ingress.py`:
  - `self.maria304_adapter = Maria304NativeAdapter()` у `__init__`.
  - `accept_maria304(self, conn, *, raw_request, response_timeout_seconds) -> tuple[InboxRecord, CanonicalFiscalCommand, Any, bool]` — копія `accept_checkbox`, просто інший адаптер.
  - `DEFAULT_RESPONSE_TIMEOUT_MARIA304_SECONDS` у `constants.py` (дефолт 10с, як у Rust бріджа request_timeout).
- `runtime/rest_app.py`:
  - `_require_maria304_token(request: Request) -> None` (FastAPI Depends). Читає `Authorization: Bearer <token>`, compare з `container.config.ingress.maria304.shared_token` через `hmac.compare_digest`. Відхиляє 401 якщо відсутній, 403 якщо не збігається.
  - `@app.post("/v1/ingress/maria304")` — віддає JSON у формі Rust `CanonicalResponse`:
    - `ok: bool`, `document_id: str`, `fiscal_id: str`, `fiscal_ts: str` (ISO8601), `document_state: str`, `sale_total_kopecks: int`, `return_total_kopecks: int`.
  - Мапінг `process_result` → відповідь:
    - Документ у `KVT2/ACK` → 200, `ok=true`, `document_id=inbox.request_id`, `fiscal_id=document.server_fiscal_no or ""`, `document_state="KVT2"/"ACK"`, суми з документа.
    - `REJECTED/CANCELLED` → 400, `{"ok": false, "error_code": "SOFT_<...>", "error_message": ...}` (mapping codes→SOFT_*).
    - `ERROR_RETRYABLE` або inbox у `NEW/PROCESSING` → 503, `{"ok": false, "error_code": "SOFT_PROCESSING", "error_message": "worker pending"}`.
    - Адаптер кинув `AdapterMappingError` → 400, `{"ok": false, "error_code": "SOFT_UNSUPPORTED", "error_message": str}`.
- `runtime/container.py`: нічого не міняємо (`IngressAcceptService` уже живе в контейнері).
- **Контракт-тести** (`tests/runtime/test_maria304_endpoint.py`, FastAPI `TestClient`):
  - 401 без заголовка, 403 на невалідному токені (constant-time).
  - 200 + правильна shape на SHIFT_OPEN-happy.
  - 400 + `UNSUPPORTED_METHOD` на CANCEL.
  - idempotency: повторний POST з тим же `idempotency_key` → той самий `document_id`, `is_replay=true` не ламає відповідь.

**Acceptance M7-Py-2:** `pytest tests/runtime/test_maria304_endpoint.py -v` зелене, `mypy` на зміненому коді чисто, ручний `curl -H "Authorization: Bearer …" … /v1/ingress/maria304` на запущеному gateway повертає 200 з очікуваною shape.

### M7-Py-3 — e2e з Rust-драйвером (пілот-парі)
- Окремий `tests/integration/test_maria304_rust_pilot.py` який:
  1. Піднімає FastAPI в TestClient + підмінений `CommandProcessor` (in-memory SQLite write-path через conftest).
  2. Стартує `cargo run --release -p maria304_driver -- --config <tmp>` у фікстурі (або робить skip якщо `cargo` недоступний — pytest marker `requires_cargo`).
  3. Пише Maria 304 wire-фрейми в TCP-сокет дрівера, зчитує відповіді, перевіряє що в інбоксі Python з'явилися канонічні команди у правильній послідовності.
- Fallback якщо e2e занадто важкий: **записаний wire-log** (reproduction-fixture) → подача напряму у Rust-дрівер через `maria304_driver::listener::session_loop::run_connection` як lib-call з `MockBridge` замість HTTP. Але це не ловить HTTP-рівень.
- Сценарій пілоту: CSIN1 → UPAS (login) → ZREP open → CSHN (SELL CASH 100.00 ПДВ20) → CSHZ (Z-report) → UPAS logoff. Очікуємо у Python: 1×SHIFT_OPEN (ACK), 1×SELL (ACK+KVT2), 1×Z_REPORT (ACK).

**Acceptance M7-Py-3:** повна послідовність пройшла, Python-архів має 3 документи в `KVT2`.

### ~~M7-Py-4 — ревізія CANCEL~~ (знято)
Звірено до старту: CANC опкод у Rust dispatcher лише локально скидає receipt state, не будує envelope. CANCEL відсутній у `CommandType` enum. Фази 3.

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

## 5a. Блокер M7-Py-2 (виявлено security-review у фазі M7-Py-1)

`services/write_path.py` маршрутизує SELL/RETURN через `validators/ua_receipt.py::validate_sell_return_receipt`, який відхиляє envelope із порожнім `receipt.goods` з `INVALID_RECEIPT_DATA`. У M4 Rust-драйвер шле `goods: []` (rich parse у Python поки не реалізовано). Без додаткової роботи всі SELL/RETURN від Maria 304 впадуть на validator-stage.

**Варіанти перед М7-Py-2:**
1. **Rich parse raw_frames → canonical goods** у адаптері (найчесніший, але великий обсяг: опкоди FISC/BFIS/ARFI/ARBF/FICD/BFCD/FINF/TGCD/GRBG/ACLD/PSDt/CSHG + агрегація в CanonicalReceiptItem).
2. **Protocol-aware validator dispatch** у `write_path.py`: якщо `protocol == MARIA_304_NATIVE`, пропускати goods-validator, бо аудит вже в raw_frames.
3. **Compromise:** синтетичний single-item `"[Maria 304 receipt — see raw_frames]"` у адаптері, сума = totals.sale_kopecks, tax_group з dual_tax_mode[0].

Рекомендую (2) — найменший діф, чіткий контракт, raw_frames лишаються джерелом правди для аудиту. Вибір остаточно — перед стартом M7-Py-2.

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
