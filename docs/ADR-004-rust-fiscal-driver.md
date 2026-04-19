# ADR-004: Rust Fiscal Driver — XML Build + Sign + gRPC Send

**Статус:** Прийнято, реалізація відкладена до окремого спринту
**Дата:** 2026-04-17
**Автори:** audit session

---

## Контекст

Поточна архітектура транспортного шару виглядає так:

```
Python
  └─ dps_xml.py         — будує XML (cp1251, ~570 рядків)
  └─ write_path.py      — викликає crypto_provider.sign(xml)
       └─ HTTP → Rust sidecar  — підписує (GOST 34310)
  └─ transports/dps_fiscal_server.py
       └─ HTTP → DPS REST endpoint  — відправляє підписаний XML
```

Тобто один чек робить **два послідовних мережевих виклики** на Python-стороні:
sign (Python→Rust) → send (Python→DPS).

Аналіз еталонної реалізації **WebCheck PRRO32** (декомпіль зберігається в
`docs/webcheck_reverse/`) виявив, що WebCheck виконує всі три операції в одному
процесі: XML build → sign → gRPC send. Крім того, знайдено критичну проблему
продуктивності в самому WebCheck (детальніше нижче).

---

## Критична знахідка: WebCheck руйнує gRPC channel після кожного чека

Код у `docs/webcheck_reverse/TaxGrpc/TaxGrpc/Client.cs`:

```csharp
private void FillResult(CheckResponse grpcRes)
{
    _result.Id = grpcRes.Id;
    // ...заповнення результату...
    _channel.ShutdownAsync();   // ← ЗАКРИВАЄ CHANNEL ПІСЛЯ КОЖНОГО ЗАПИТУ
}
```

**Наслідки:**
- Нове TCP з'єднання на кожен чек
- Нове TLS handshake на кожен чек
  - TLS 1.2: 2 × RTT (≈ 2 × 50ms = 100ms overhead/чек)
  - TLS 1.3 без session resumption: 1 × RTT + application data
- Нова HTTP/2 SETTINGS negotiation
- При 100 чеках за зміну: ~10 секунд чистих накладних витрат тільки на handshakes

Наш Rust driver зробить правильно з першого разу.

---

## Рішення: Rust Fiscal Driver

Rust sidecar (`prro_crypto`) розширюється до повного **fiscal protocol driver**:

```
Python
  └─ write_path.py
       └─ HTTP POST /fiscal/send  →  Rust sidecar
                                        ├─ XML build (cp1251)
                                        ├─ GOST sign
                                        ├─ gRPC sendChkV2 → DPS
                                        └─ повертає {fiscal_id, status, signed_bytes}
```

Python більше не знає про XML, cp1251, protobuf, TLS, MAC-chain деталі протоколу.

---

## Protobuf-схема ДПС (відновлена з WebCheck)

Джерело: `docs/webcheck_reverse/TaxGrpc/Com.Programika.Rro.Ws.Chk/`

### check.proto (реконструйований)

```protobuf
syntax = "proto3";
package com.programika.rro.ws.chk;

enum CheckType {
  UNKNOWN  = 0;
  CHK      = 1;   // продаж/повернення/сервіс
  ZREPORT  = 2;   // Z-звіт
  SERVICECHK = 3; // службове внесення/видача
}

message Check {
  string     rro_fn       = 1;  // фіскальний номер РРО
  int64      date_time    = 2;  // YYYYMMDDHHmmss як int64
  bytes      check_sign   = 3;  // підписаний XML (cp1251 bytes)
  int32      local_number = 4;  // локальний номер чека
  CheckType  check_type   = 5;
  string     id_offline   = 6;  // UUID офлайн-номера (або "")
  string     id_cancel    = 7;  // UUID для скасування (або "")
}

message CheckRequest {
  bytes rro_fn_sign = 1;  // підписаний FN (для lastChk)
}

message CheckRequestId {
  string id = 1;
}

enum ResponseStatus {
  UNKNOWN                   =   0;
  OK                        =   1;
  ERROR_VEREFY              =  -1;
  ERROR_CHECK               =  -2;
  ERROR_SAVE                =  -3;
  ERROR_UNKNOWN             =  -4;
  ERROR_TYPE                =  -5;
  ERROR_NOT_PREV_ZREPORT    =  -6;
  ERROR_XML                 =  -7;
  ERROR_XML_DATE            =  -8;
  ERROR_XML_CHK             =  -9;
  ERROR_XML_ZREPORT         = -10;
  ERROR_OFFLINE_168         = -11;  // ← перевищено 168 год офлайн
  ERROR_BAD_HASH_PREV       = -12;
  ERROR_NOT_REGISTERED_RRO  = -13;
  ERROR_NOT_REGISTERED_SIGNER = -14;
  ERROR_NOT_OPEN_SHIFT      = -15;
  ERROR_OFFLINE_ID          = -16;
}

message CheckResponse {
  string         id            = 1;  // фіскальний номер від ДПС
  ResponseStatus status        = 2;
  bytes          id_sign       = 3;  // підпис ID (base64-encoded bytes)
  bytes          data_sign     = 4;  // підпис даних
  string         error_message = 5;
}

service ChkIncomeService {
  rpc sendChk    (Check)        returns (CheckResponse);  // API v1
  rpc sendChkV2  (Check)        returns (CheckResponse);  // API v2 ← використовувати
  rpc ping       (Check)        returns (CheckResponse);
  rpc lastChk    (CheckRequest) returns (CheckResponse);
  rpc delLastChk (CheckRequest) returns (CheckResponse);
  rpc delLastChkId(CheckRequestId) returns (CheckResponse);
  rpc statusRro  (CheckRequest) returns (StatusResponse);
  rpc infoRro    (CheckRequest) returns (RroInfoResponse);
}
```

### Ендпоінти DPS
| Середовище | Адреса |
|-----------|--------|
| Production | `prro.tax.gov.ua:443` |
| Test       | `cabinet.tax.gov.ua:9443` |

TLS root cert: `C:\ProgramData\WebCheck\prro-tax-gov-ua-chain.pem`
(або системний CA store — перевірити чи DPS cert в ньому)

---

## Мережеві оптимізації в Rust

### 1. Persistent gRPC channel (виправляє баг WebCheck)

```rust
// Один channel на весь lifetime sidecar (або на FN)
// tonic::transport::Channel з keep-alive
let channel = Channel::from_static("https://prro.tax.gov.ua:443")
    .keep_alive_interval(Duration::from_secs(30))
    .keep_alive_timeout(Duration::from_secs(10))
    .keep_alive_while_idle(true)   // ← тримати alive навіть без трафіку
    .connect_timeout(Duration::from_secs(10))
    .timeout(Duration::from_secs(90))  // per-request deadline
    .connect()
    .await?;
```

**Виграш:** Перший чек платить за TLS handshake (~100ms), всі наступні — ні.
При 100 чеках/зміну: економія ~10 секунд.

### 2. TLS session resumption

```rust
// rustls з session cache
let tls_config = rustls::ClientConfig::builder()
    .with_safe_defaults()
    .with_root_certificates(root_store)
    .with_no_client_auth();

// Session resumption: TLS 1.3 — автоматично через PSK
// TLS 1.2 — через session tickets (rustls підтримує за замовчуванням)
```

**Виграш:** Якщо з якоїсь причини channel перестворюється (reconnect) —
наступний handshake займе 0-RTT замість повних 2 × RTT.

### 3. HTTP/2 multiplexing для паралельних FN

```rust
// tonic з HTTP/2 нативно — один TCP connection, декілька concurrent streams
// Якщо у нас кілька fiscal_number — всі йдуть через один channel
// HTTP/2 SETTINGS: збільшити MAX_CONCURRENT_STREAMS
let channel = Channel::from_static("...")
    .initial_connection_window_size(1024 * 1024)  // 1MB flow control
    .initial_stream_window_size(1024 * 1024)
    .connect().await?;
```

**Виграш:** При паралельних чеках (різні FN) — один TCP замість N.

### 4. TCP_NODELAY

```rust
// tonic використовує hyper, який за замовчуванням вмикає TCP_NODELAY
// Явно підтвердити:
let endpoint = Channel::from_static("...")
    .tcp_nodelay(true);    // вимикаємо Nagle — менша затримка на маленьких пакетах
```

**Виграш:** Protobuf чек (~2-5KB) — без Nagle iде одразу, без 40ms затримки.

### 5. Connection health check при старті

```rust
// При ініціалізації sidecar — ping до ДПС
// Використати rpc ping() з Check { check_type: UNKNOWN }
// Якщо відповідь прийшла — channel живий, TLS сесія прогріта
async fn warmup_connection(&self) -> Result<()> {
    self.client.ping(PingRequest::default()).await?;
    Ok(())
}
```

**Виграш:** Перший реальний чек не платить за "холодний старт".

### 6. Reconnect policy

```rust
// tonic має вбудований retry/reconnect через tower middleware
// При ERROR_BAD_HASH_PREV або network error — reconnect, не panic
let channel = Channel::from_static("...")
    .connect_timeout(Duration::from_secs(10));

// tower::retry для ідемпотентних запитів (ping, statusRro)
// НЕ retry для sendChkV2 — нам не потрібне подвійне відправлення
```

---

## Що переїжджає в Rust

| Компонент | Зараз | Після |
|-----------|-------|-------|
| XML build | `serializers/dps_xml.py` (570 рядків) | `rust/prro_crypto/src/dps/xml_builder.rs` |
| cp1251 encode | Python `str.encode('cp1251')` | `encoding_rs` crate |
| GOST sign | Вже в Rust | Без змін |
| MAC chain context | Частково в Python | Повністю в Rust (per-FN state) |
| gRPC transport | Python → DPS REST | Rust → DPS gRPC (`sendChkV2`) |
| TLS | Python `requests` | `rustls` / `tonic` |

## Що залишається в Python

| Компонент | Обґрунтування |
|-----------|---------------|
| Canonical model | Бізнес-логіка, адаптери, валідація |
| Write-path стейт машина | Транзакції SQLite |
| Reconciliation | Логіка відновлення |
| Offline session management | Стан в БД |
| REST/XML-RPC/Maria ingress | Протоколи клієнтів |

Python API до Rust sidecar стає простішим:
```
POST /fiscal/send
{
  "fiscal_number": "...",
  "local_number": 42,
  "check_type": "CHK",         // CHK | ZREPORT | SERVICECHK
  "offline_id": "",
  "cancel_id": "",
  "canonical": { ... }         // canonical receipt/shift/service payload
}

→ 200 OK
{
  "fiscal_id": "...",
  "status": "OK",
  "id_sign_b64": "...",
  "data_sign_b64": "...",
  "xml_cp1251_b64": "..."      // для архіву
}
```

---

## Де взяти вихідні матеріали

### 1. Protobuf schema
Реконструйована вище з `docs/webcheck_reverse/TaxGrpc/Com.Programika.Rro.Ws.Chk/*.cs`.
Теги полів (field numbers) витягнуті з `WriteTo`/`MergeFrom` методів.
Файл `.proto` треба створити вручну — бінарний дескриптор в `GreetReflection.cs`
але сам `.proto` не відновлюється автоматично.

### 2. XML структура
`docs/webcheck_reverse/WebCheckMain/WebCheck/StringXML.cs` — повна еталонна
реалізація XML builder включаючи edge cases (cp1251, T='108', NI= для знижок,
TXAL формули для акцизу).

### 3. TXSM/TXAL формули
Підтверджені і збігаються з нашим `_calc_tax()` в `serializers/dps_xml.py`.
Деталі в `docs/webcheck_reverse/WebCheckMain/WebCheck/StringXML.cs:1076-1133`.

### 4. TLS сертифікат ДПС
`C:\ProgramData\WebCheck\prro-tax-gov-ua-chain.pem` — chain cert для TLS до ДПС.
Потрібно перевірити чи він вже в системному CA store Linux/Docker.

### 5. Rust crates
```toml
[dependencies]
tonic       = { version = "0.12", features = ["tls", "tls-roots"] }
prost       = "0.13"
encoding_rs = "0.8"   # cp1251
tokio       = { version = "1", features = ["full"] }
rustls      = "0.23"
tower       = "0.5"   # middleware (timeout, reconnect)
```

### 6. Поточний XML код (міграційний референс)
`src/prro_gateway/serializers/dps_xml.py` — повна Python реалізація з тестами.
При переносі в Rust — тести залишаються як golden-file тести: Python XML == Rust XML.

---

## Критичні деталі реалізації

### cp1251 encoding
WebCheck: `Encoding.GetEncoding(1251)` при читанні і записі XML файлу.
Protobuf поле `check_sign: bytes` містить XML у cp1251, не UTF-8.

```rust
use encoding_rs::WINDOWS_1251;
let (encoded, _, _) = WINDOWS_1251.encode(&xml_string);
let bytes: Vec<u8> = encoded.into_owned();
```

### DateTime format
`int64` у форматі `YYYYMMDDHHmmss` (не Unix timestamp!):
```rust
// Приклад: 20240101143022 = 2024-01-01 14:30:22
let dt: i64 = format!("{}", chrono::Local::now().format("%Y%m%d%H%M%S"))
    .parse().unwrap();
```

### CheckType mapping
```
T='0' (SELL/RETURN check)    → CheckType::Chk      (= 1)
T='2' (SERVICE IN/OUT)       → CheckType::Servicechk (= 3)
T='8'/'108' (SHIFT OPEN)     → CheckType::Chk      (= 1) — shift open це теж чек
Z-report                     → CheckType::Zreport  (= 2)
```

### WebCheck баг: channel.ShutdownAsync() після кожного запиту
Це підтверджено в `FillResult()`. Наш код НЕ повинен так робити.
Channel живе весь час роботи sidecar.

---

## Що НЕ треба переносити в Rust

- Offline UUID management (стан в SQLite, логіка в Python)
- Shift state machine (SQLite транзакції)
- Reconciliation (складна бізнес-логіка)
- Валідація canonical payload (Python, вже покрита тестами)

---

## Умови запуску спринту

1. Поточний Python XML + REST transport стабільно пройшов пілот (≥1 реальний РРО)
2. ДПС підтвердив відсутність deprecation REST endpoint (альтернатива gRPC, не вимога)
3. Є `.pem` файл або спосіб отримати TLS cert для `prro.tax.gov.ua`

---

## Очікуваний виграш після реалізації

| Метрика | Зараз | Після |
|---------|-------|-------|
| Round-trips на чек | 2 (sign + send) | 1 (Rust все робить) |
| TLS handshake | 1/чек (WebCheck стиль якщо reconnect) | 1/сесія |
| Мережеві виклики Python→Rust | 1/чек | 1/чек (без змін, але легший payload) |
| XML у Python | 570 рядків складного коду | 0 рядків |
| cp1251 проблеми | Потенційні | Неможливі |
| Час першого чека після старту | ~200ms (TLS cold) | ~100ms (з warmup) |
| Час наступних чеків | ~100ms (TLS/чек) | ~50ms (persistent channel) |

Оцінка: **-50ms на чек** для 2+ чека в сесії. При касовому навантаженні
50-200 чеків/зміну це відчутно і усуває найбільшу варіативність затримок.
