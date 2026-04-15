# PRRO Gateway — Context Handoff для соседней LLM

## Что это за проект

**Multi-Protocol PRRO Gateway** — локальный edge-шлюз для украинских программных РРО (касс).
Принимает фискальные команды от POS-систем через несколько ingress-протоколов, нормализует в каноническую модель, подписывает ДСТУ крипто, отправляет напрямую на сервер ДПС.

Это **не игрушечный сервис**. Фискальная система с юридическими рисками и штрафами.

Целевая архитектура: `local PRRO core → direct DPS submission`.
Checkbox — только ingress-совместимость, НЕ целевой egress.

---

## Окружение

- **OS**: WSL2 (Ubuntu) на Windows, `Linux 6.6.87.2-microsoft-standard-WSL2`
- **Python**: 3.12.3
- **Node.js**: 18.19.1 (для крипто-сайдкара)
- **Working dir**: `/mnt/d/PRRO_GATE`
- **Git**: репо без remote (локальный)
- **DB**: SQLite WAL (in-memory для тестов, file для runtime)

---

## RTK Proxy — КРИТИЧНО

В окружении установлен **RTK (Rust Token Killer)** — CLI-прокси для оптимизации токенов.
RTK перехватывает вывод обычных команд и фильтрует его.

**Правило: для тестов ВСЕГДА использовать `rtk proxy pytest`**, не `python -m pytest` и не `pytest` напрямую.

```bash
# Правильно:
rtk proxy pytest tests/ -q
rtk proxy pytest tests/test_write_path.py -q

# Неправильно (вывод будет отфильтрован RTK hooks):
pytest tests/ -q
python -m pytest tests/
```

RTK установлен как hook в Claude Code. Обычные команды (`git`, `ls`) тоже проксируются, но это прозрачно. Проблема только с pytest.

---

## Структура пакета

```
src/prro_gateway/
├── adapters/          # Checkbox REST, WebCheck XML-RPC, Maria TCP → canonical model
├── config.py          # AppConfig (YAML + env overrides)
├── enums.py           # OperationType, DocumentState, TransportKind, CanonicalErrorCode
├── errors.py          # build_canonical_error()
├── migrations/        # SQL migration runner with checksum verification
├── models/            # canonical.py (CanonicalFiscalCommand), storage.py (DB records)
├── ports.py           # Transport/Crypto protocols, error types
├── repositories/      # SQLite repositories (fiscal_documents, inbox, shifts, node_state, ...)
├── runtime/
│   ├── container.py   # DI root, wires all layers, production gates
│   ├── rest_app.py    # FastAPI REST app
│   ├── providers.py   # PassthroughCryptoProvider, SidecarCryptoProvider/Client
│   ├── supervisor.py  # Startup supervisor
│   └── *_shell.py     # XML-RPC, Maria TCP shells
├── serializers/
│   └── dps_xml.py     # ФСКО XML builder (<RQ><DAT><C>...</C><TS>...</TS></DAT><MAC>...</MAC></RQ>)
├── services/
│   ├── ingress.py     # IngressAcceptService
│   ├── write_path.py  # WritePathWorker — 6-stage pipeline (HOT ZONE)
│   ├── reconciliation.py  # Recovery via lastChk (HOT ZONE)
│   └── offline_sync.py    # Offline document sync to DPS
├── transports/
│   ├── dps_fiscal_server.py  # Real DPS gRPC transport (HOT ZONE)
│   ├── checkbox_rest.py      # Checkbox REST transport (compatibility)
│   ├── router.py             # ProfileAwareTransportRouter
│   ├── stubs.py              # Dev/test transport stubs
│   └── proto/                # gRPC proto + generated stubs
├── utils/             # JSON codec
└── validators/        # Ukrainian receipt validator, manifest validator

sidecar/
├── server.js          # Node.js JKS signing sidecar (jkurwa + gost89)
└── test.js            # Sidecar tests

sql/
├── 001_hot_store_init.sql      # Schema
├── 002_seed_reference_data.sql # Seed profiles + node_state
├── 003-006_*.sql               # Incremental migrations

tests/                 # 514 tests, pytest, in-memory SQLite via conftest.py
ops/                   # Config examples, e2e seed SQL, systemd
scripts/               # run_rest.py, run_e2e_dps_test.py
docs/                  # Architecture, legal invariants, roadmap, acceptance
```

---

## Request flow

```
REST/XML-RPC/Maria ingress
  → RuntimeContainer (DI root)
  → Adapter → CanonicalFiscalCommand
  → IngressService → inbox + write-path trigger
  → WritePathWorker (6 stages):
      acquire+validate → guard → sign → send_or_offline → finalize
  → Transport (ProfileAwareTransportRouter → DpsFiscalServerTransport)
  → gRPC sendChkV2 to prro.tax.gov.ua / cabinet.tax.gov.ua:9443
```

---

## DPS Fiscal Server (прямой transport)

- **gRPC service**: `com.programika.rro.ws.chk.ChkIncomeService`
- **Methods**: `sendChkV2`, `lastChk`, `statusRro`, `infoRro`, `ping`
- **Production**: `prro.tax.gov.ua:443`
- **Test**: `cabinet.tax.gov.ua:9443`
- **Тестовый FN**: `4000162280`, TN `13667753`
- **Signer**: ГАЛЬЧУН МИКОЛА ДМИТРОВИЧ (JKS keystore, password `Jrcfyf123`)

### DPS XML формат (ФСКО, канонічна форма):
```xml
<RQ NDv="ПРО_каса" PrV="1.1" V="1">
  <DAT DI="0" FN="4000162280" TN="13667753" V="1" ZN="0">
    <C T="108">...</C>   <!-- SHIFT_OPEN -->
    <C T="0">...</C>     <!-- SELL -->
    <C T="1">...</C>     <!-- RETURN -->
    <C T="2">...</C>     <!-- SERVICE_IN/OUT -->
    <C T="8">...</C>     <!-- CASH_WITHDRAWAL -->
    <Z NO="1">...</Z>    <!-- Z_REPORT -->
    <TS>20260413213320</TS>
  </DAT>
  <MAC>{sha256_of_previous_xml}</MAC>
</RQ>
```
Канонічна форма: атрибути в алфавітному порядку, `<tag></tag>` (не self-closing).

### Proto check_type mapping:
- CHK=1 (SELL, RETURN, SERVICE_IN/OUT, CASH_WITHDRAWAL)
- ZREPORT=2
- SERVICECHK=3 (SHIFT_OPEN, GO_OFFLINE, GO_ONLINE, PING, ASK_CODES)

### DPS error codes (aligned with proto, PRRO_GATE-r2c):

**Authoritative source:** `transports/proto/fiscal_server.proto`

| Status | Proto name | Classification |
|--------|-----------|---------------|
| 1 | OK | ACK |
| -1 | ERROR_VEREFY | Rejected (signature) |
| -2 | ERROR_CHECK | Rejected (RRO verification) |
| -3 | ERROR_SAVE | Rejected (write/duplicate) |
| -4 | ERROR_UNKNOWN | Rejected (general error) |
| -5 | ERROR_TYPE | Rejected (wrong check_type) |
| -6 | ERROR_NOT_PREV_ZREPORT | Rejected (missing Z-report) |
| -7 | ERROR_XML | Rejected (invalid XML) |
| -8 | ERROR_XML_DATE | Rejected (date mismatch) |
| -9 | ERROR_XML_CHK | Rejected (check format) |
| -10 | ERROR_XML_ZREPORT | Rejected (Z-report format) |
| -11 | ERROR_OFFLINE_168 | Rejected (168h limit) |
| -12 | ERROR_BAD_HASH_PREV | MAC recovery if hash extractable, else Rejected |
| -13 | ERROR_NOT_REGISTERED_RRO | Rejected |
| -14 | ERROR_NOT_REGISTERED_SIGNER | Rejected |
| -15 | ERROR_NOT_OPEN_SHIFT | Rejected |
| -16 | ERROR_OFFLINE_ID | Rejected |
| gRPC/network exception | — | Retryable |

**Policy:** All negative proto statuses = DPS rejection (terminal). Only network/gRPC failures = retryable. `TransportRateLimitedError` NOT derived from CheckResponse.status.

---

## Крипто

- **Dev**: `PassthroughCryptoProvider` (ничего не подписывает)
- **Production**: `SidecarCryptoProvider` → HTTP к `sidecar/server.js`
- **Sidecar**: Node.js, `jkurwa` + `gost89`, DSTU 4145-2002 curve 6
- **Подпись**: CMS/PKCS#7 SignedData (DER, attached)
- **JKS**: manual SHA1 XOR keystream decryption (feedfeed format)
- **Sidecar endpoint**: `POST /sign_raw` (payload_base64 → signed_base64)
- **Default port**: 8091

Запуск сайдкара:
```bash
cd sidecar && JKS_PASSWORD='Jrcfyf123' PORT=8091 node server.js
```

---

## Ключевые invariants (нарушение = баг)

1. Нет network/crypto вызовов внутри SQLite write transactions
2. Один fiscal_number = один logical single-writer
3. Channel switch запрещён при открытой смене
4. Idempotency обязательна
5. Offline ≤ 36ч непрерывно, ≤ 168ч/месяц
6. Passthrough crypto запрещён в production
7. RETURN требует `related_receipt_id` (production gate)

---

## State machines

- **Document**: PREPARED → SIGNED → SENT → ACK / REJECTED / ERROR_RETRYABLE
- **Shift**: CREATED → OPENING → OPENED → CLOSING → CLOSED / ERROR
- **Node**: ONLINE / GOING_OFFLINE / OFFLINE / GOING_ONLINE

---

## Текущий статус (Sprint 10 post-review, 2026-04-14)

**Доказано live на `cabinet.tax.gov.ua:9443`:**
- SHIFT_OPEN, SELL, RETURN, SERVICE_IN/OUT, Z_REPORT — все ACK
- Full e2e: REST → write-path → sidecar → DPS gRPC
- MAC auto-recovery из ERROR_BAD_HASH_PREV (-12 only)

**563 тестів** (`rtk proxy pytest tests/ -q` → 563 passed, 2026-04-14).

**Sprint 10 завершений (Steps 1-10):**
- Step 1: Migrations 009, 010 (cash balance + payment type definitions)
- Step 2: PaymentTypeRepository + guard (reject unknown types before sign)
- Step 3: Cash balance calculation (derived from fiscal_documents)
- Step 4: Guards (SERVICE_OUT/CASH_WITHDRAWAL > balance → reject)
- Step 5: Cash carry-over SHIFT_OPEN/Z_REPORT (реалізовано в Step 3)
- Step 6: Решта (RM attribute) на готівковому M
- Step 7: Заокруглення (SMP attribute) per Постанова НБУ №115
- Step 8: Реквізити ЕПЗ на `<M>` + payment type resolution
- Step 9: X-report endpoint (GET /v1/shifts/current/x-report)
- Step 10: Canonical layer (cash_balance/change/rounded_sum/rounding в REST response)
- Step 11: Proof tests — відкладено, покриття достатнє

**Ключовий architectural fix:** `_enrich_payload_for_dps()` shared helper. І `_resolve_sign_input`, і `_try_mac_recovery_and_resend` викликають один метод для payload enrichment — закриває recurring class bugs де MAC recovery path розходився з normal path.

**Відкладені пункти (backlog):**
- Persist canonical fields на fiscal_documents при finalize (замість recompute на replay)
- F4 replay-after-flag-flip tests
- Operator runbook: не міняти rounding_enabled/print_zero_change при in-flight requests

**Закриті critical review fixes Sprint 9:**
- PRRO_GATE-lp3: MAC recovery semantic equivalence
- PRRO_GATE-4wi: offline replay transport fields
- PRRO_GATE-r2c: DPS status classification aligned with proto

**Pilot scope exclusions:** TXAL=3 (guard blocks), DPS_UNIFIED_WINDOW (stub), offline lifecycle (Sprint 11).

**Наступний етап:** Rust crypto crate (`prro_crypto`) — portування jkurwa в Rust, прибирає Node.js sidecar, відкриває Android. Деталі в `docs/RUST_CRYPTO_PLAN.md`.

---

## Config

```yaml
# ops/config.example.yaml — development
runtime:
  environment: development  # development | test | production
crypto:
  provider: passthrough     # passthrough | sidecar
  sidecar_url: http://localhost:8091

# ops/config.e2e_dps_test.yaml — для live e2e
runtime:
  environment: test
crypto:
  provider: sidecar
  sidecar_url: http://localhost:8091
defaults:
  fiscal_number: "4000162280"
  tax_number: "13667753"
```

Container wiring по environment:
- `development`: PassthroughCrypto, DpsGrpcEcabinetTransportStub, RETURN без linkage разрешён
- `test`: SidecarCrypto (если sidecar_url задан), DpsFiscalServerTransport (реальный), RETURN требует linkage
- `production`: SidecarCrypto (обязательно), DpsFiscalServerTransport, production gates (crypto + transport)

---

## Hot zones (high-risk, тестировать после изменений)

- `services/write_path.py` — 6-stage pipeline
- `services/reconciliation.py` — recovery
- `repositories/*` — persistence
- `transports/dps_fiscal_server.py` — DPS transport
- `runtime/container.py` — DI root + gates
- Schema/migrations

---

## Команды для быстрого старта

```bash
# Тесты (ВСЕГДА через rtk proxy)
rtk proxy pytest tests/ -q
rtk proxy pytest tests/test_sprint8_dps_return.py -q
rtk proxy pytest -k "mac_recovery" -q

# Проверить структуру
ls src/prro_gateway/
ls tests/

# Beads (issue tracker)
bd list --status open
bd list --status closed | tail -10
bd show PRRO_GATE-xxx

# Сайдкар
curl http://localhost:8091/healthz

# DPS probe (если runtime запущен)
curl -X POST http://localhost:8080/v1/admin/dps-probe -H 'Content-Type: application/json' -d '{"fiscal_number":"4000162280"}'
```

---

## Что НЕ делать

- Не запускать `pytest` без `rtk proxy`
- Не трогать production endpoint `prro.tax.gov.ua:443`
- Не делать sweeping refactors в hot zones
- Не добавлять network calls внутри SQLite transactions
- Не коммитить секреты (JKS, пароли)
- Не использовать `git push --force`
- Не выдумывать DPS XML tags без evidence
