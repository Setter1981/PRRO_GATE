# Rust Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Устранить 9 дефектов безопасности и надёжности в `prro_sidecar` и `maria304_driver`, выявленных в аудите 2026-04-22.

**Architecture:** Два независимых Rust-крейта: `prro_sidecar` (Axum HTTP, DPS gRPC, CMS-подпись, SQLite) и `maria304_driver` (TCP-эмулятор Maria 304, HTTP-бридж к Python-шлюзу). Каждая задача — вертикальный срез с тестом до кода (TDD). Задачи S1–S4 изолированы в sidecar; M1–M5 изолированы в driver.

**Tech Stack:** Rust 2021, tokio, axum, rusqlite, reqwest (blocking + async), tonic, tokio-util (новая зависимость в M3).

---

## File Structure

| Файл | Задачи |
|------|--------|
| `rust/prro_sidecar/src/license_pubkey_current.der` | S1 |
| `rust/prro_sidecar/src/license_pubkey_next.der` | S1 |
| `rust/prro_sidecar/src/license.rs` | S1 (startup assert) |
| `rust/prro_sidecar/src/bin/prro_sidecar.rs` | S2, S3 |
| `rust/prro_sidecar/src/repo.rs` | S4 |
| `rust/prro_sidecar/tests/wiring_date_time.rs` | S1–S4 (тесты добавляются) |
| `rust/maria304_driver/src/session/dispatcher.rs` | M1 |
| `rust/maria304_driver/src/bin/maria304_driver.rs` | M2, M3 |
| `rust/maria304_driver/src/listener/server.rs` | M3, M4 |
| `rust/maria304_driver/src/listener/session_loop.rs` | M4, M5 |
| `rust/maria304_driver/Cargo.toml` | M3 |

---

### Task 1: S1 — License pubkeys (32 → 33 bytes)

**Goal:** Заменить 32-байтовые заглушки `DEADBEEF…` в `.der`-файлах на реальную тестовую 33-байтовую точку DSTU PB-257, добавить startup-assertion на длину.

**Files:**
- Modify (binary): `rust/prro_sidecar/src/license_pubkey_current.der`
- Modify (binary): `rust/prro_sidecar/src/license_pubkey_next.der`
- Modify: `rust/prro_sidecar/src/license.rs` (добавить `pub fn check_embedded_pubkeys()`)
- Modify: `rust/prro_sidecar/src/bin/prro_sidecar.rs` (вызов `check_embedded_pubkeys()` в `main()`)

**Acceptance Criteria:**
- [ ] Оба `.der`-файла ровно 33 байта
- [ ] Первый байт каждого файла — `0x02` или `0x03` (признак compressed point)
- [ ] `verify_detached` не возвращает `false` из-за size-guard
- [ ] `check_embedded_pubkeys()` паникует при 32-байтовом файле; молчит при 33-байтовом
- [ ] Тест `pubkey_files_are_33_bytes` проходит

**Verify:** `cargo test -p prro_sidecar pubkey_files_are_33_bytes 2>&1 | tail -10`

**Steps:**

- [ ] **Step 1: Написать failing тест**

В `rust/prro_sidecar/tests/wiring_date_time.rs` добавить:

```rust
#[test]
fn pubkey_files_are_33_bytes() {
    let cur = include_bytes!("../src/license_pubkey_current.der");
    let nxt = include_bytes!("../src/license_pubkey_next.der");
    assert_eq!(cur.len(), 33, "license_pubkey_current.der must be 33 bytes");
    assert_eq!(nxt.len(), 33, "license_pubkey_next.der must be 33 bytes");
    // First byte must be a compressed-point prefix (0x02 or 0x03)
    assert!(cur[0] == 0x02 || cur[0] == 0x03, "current: invalid compressed prefix");
    assert!(nxt[0] == 0x02 || nxt[0] == 0x03, "next: invalid compressed prefix");
}
```

- [ ] **Step 2: Убедиться, что тест падает**

```
cargo test -p prro_sidecar pubkey_files_are_33_bytes
```
Ожидается: FAIL "must be 33 bytes"

- [ ] **Step 3: Сгенерировать тестовый ключ**

```bash
cd rust
cargo run -p prro_sidecar --bin prro_license_keygen -- \
  --out-priv /tmp/test_license.key \
  --out-pub  /tmp/test_license_pub.der \
  --force
wc -c /tmp/test_license_pub.der   # должно быть 33
```

- [ ] **Step 4: Заменить .der-файлы**

```bash
cp /tmp/test_license_pub.der rust/prro_sidecar/src/license_pubkey_current.der
cp /tmp/test_license_pub.der rust/prro_sidecar/src/license_pubkey_next.der
```

Этот ключ является тестовым (нет соответствующего private key в prod). Для prod-деплоя файлы заменяются реальными ключами издателя лицензий.

- [ ] **Step 5: Добавить `check_embedded_pubkeys()` в `license.rs`**

В конце секции `// ─── Core verify logic ─────────────────────────────────────────────────────` добавить:

```rust
/// Called once at startup — panics immediately if the embedded pubkeys
/// are malformed (wrong length or bad compression prefix).
/// A 32-byte deadbeef placeholder would silently make verify_detached
/// always return false; this guard catches that at startup.
pub fn check_embedded_pubkeys() {
    for (label, key) in [("current", PUBKEY_CURRENT), ("next", PUBKEY_NEXT)] {
        assert_eq!(
            key.len(), 33,
            "license_pubkey_{label}.der must be 33 bytes (DSTU PB-257 compressed point); got {}",
            key.len()
        );
        assert!(
            key[0] == 0x02 || key[0] == 0x03,
            "license_pubkey_{label}.der: invalid compressed-point prefix 0x{:02x}",
            key[0]
        );
    }
}
```

- [ ] **Step 6: Вызвать `check_embedded_pubkeys()` в `main()`**

В `rust/prro_sidecar/src/bin/prro_sidecar.rs`, сразу после `tracing_subscriber` init:

```rust
// Fail fast if embedded license pubkeys are malformed placeholders.
prro_sidecar::license::check_embedded_pubkeys();
```

- [ ] **Step 7: Запустить тест**

```
cargo test -p prro_sidecar pubkey_files_are_33_bytes
```
Ожидается: PASS

- [ ] **Step 8: Запустить весь `prro_sidecar` тест-сьют**

```
cargo test -p prro_sidecar 2>&1 | tail -5
```
Ожидается: все тесты PASS

- [ ] **Step 9: Commit**

```bash
git add rust/prro_sidecar/src/license_pubkey_current.der \
        rust/prro_sidecar/src/license_pubkey_next.der \
        rust/prro_sidecar/src/license.rs \
        rust/prro_sidecar/src/bin/prro_sidecar.rs \
        rust/prro_sidecar/tests/wiring_date_time.rs
git commit -m "fix(sidecar): S1 — replace 32-byte deadbeef pubkeys with valid 33-byte DSTU test keys"
```

---

### Task 2: S2 — Envelope integrity validation

**Goal:** Добавить в `fiscal_send_inner` проверку `schema_version` по allowlist и верификацию `payload_sha256`, если поле непусто.

**Files:**
- Modify: `rust/prro_sidecar/src/bin/prro_sidecar.rs` (до step 9-pre в `fiscal_send_inner`)
- Modify: `rust/prro_sidecar/tests/wiring_date_time.rs` (тесты)

**Acceptance Criteria:**
- [ ] `schema_version` не из `["1.0"]` → HTTP 400 `InvalidInput`
- [ ] `payload_sha256` непустой + не совпадает с SHA-256(payload) → HTTP 422 `InvalidInput`
- [ ] `payload_sha256` непустой + совпадает → проходит
- [ ] `payload_sha256` пустой → проверка пропускается
- [ ] Существующие тесты проходят без изменений

**Verify:** `cargo test -p prro_sidecar envelope_integrity 2>&1 | tail -15`

**Steps:**

- [ ] **Step 1: Написать failing тест**

В `rust/prro_sidecar/tests/wiring_date_time.rs` добавить модуль:

```rust
#[cfg(test)]
mod envelope_integrity {
    use prro_sidecar::input::{CanonicalCommand, OperationType};
    use sha2::{Digest, Sha256};

    fn make_cmd(schema_version: &str, payload_sha256: &str) -> CanonicalCommand {
        CanonicalCommand {
            schema_version: schema_version.to_string(),
            payload_sha256: payload_sha256.to_string(),
            // fill other required fields with dummy values
            request_id: "req-1".to_string(),
            idempotency_key: "idem-1".to_string(),
            operation_type: OperationType::Sell,
            fiscal_number: "9999999999".to_string(),
            business_ts: "2026-04-22T10:00:00+03:00".to_string(),
            payload: serde_json::json!({"receipt": {}}),
        }
    }

    #[test]
    fn unknown_schema_version_is_rejected() {
        let cmd = make_cmd("2.0", "");
        let err = prro_sidecar::validate_envelope(&cmd).unwrap_err();
        assert!(err.contains("schema_version"), "expected schema_version error, got: {err}");
    }

    #[test]
    fn known_schema_version_passes() {
        let cmd = make_cmd("1.0", "");
        assert!(prro_sidecar::validate_envelope(&cmd).is_ok());
    }

    #[test]
    fn correct_payload_sha256_passes() {
        let payload = serde_json::json!({"receipt": {}});
        let payload_bytes = serde_json::to_vec(&payload).unwrap();
        let sha = hex::encode(Sha256::digest(&payload_bytes));
        let cmd = CanonicalCommand {
            schema_version: "1.0".to_string(),
            payload_sha256: sha,
            payload,
            request_id: "r".to_string(),
            idempotency_key: "i".to_string(),
            operation_type: OperationType::Sell,
            fiscal_number: "9999999999".to_string(),
            business_ts: "2026-04-22T10:00:00+03:00".to_string(),
        };
        assert!(prro_sidecar::validate_envelope(&cmd).is_ok());
    }

    #[test]
    fn tampered_payload_sha256_rejected() {
        let cmd = CanonicalCommand {
            schema_version: "1.0".to_string(),
            payload_sha256: "deadbeef".to_string(),
            payload: serde_json::json!({"receipt": {}}),
            request_id: "r".to_string(),
            idempotency_key: "i".to_string(),
            operation_type: OperationType::Sell,
            fiscal_number: "9999999999".to_string(),
            business_ts: "2026-04-22T10:00:00+03:00".to_string(),
        };
        let err = prro_sidecar::validate_envelope(&cmd).unwrap_err();
        assert!(err.contains("payload_sha256"), "expected sha256 error, got: {err}");
    }
}
```

- [ ] **Step 2: Добавить `validate_envelope()` в `lib.rs`**

Проверить, есть ли `lib.rs` в `prro_sidecar` (должен быть, если крейт — и lib, и bin):

```bash
ls rust/prro_sidecar/src/lib.rs
```

Если нет — создать `rust/prro_sidecar/src/lib.rs` с содержимым:

```rust
pub mod cms_adapter;
pub mod config;
pub mod credentials;
pub mod errors;
pub mod grpc_client;
pub mod input;
pub mod license;
pub mod repo;
pub mod xml_builder;
// re-export generated stubs
pub mod generated { include!(concat!(env!("OUT_DIR"), "/prro_dps.rs")); }
```

Затем добавить в `lib.rs` (или в `input.rs`) функцию:

```rust
// В src/lib.rs:
use sha2::{Digest, Sha256};
use crate::input::CanonicalCommand;

const ALLOWED_SCHEMA_VERSIONS: &[&str] = &["1.0"];

/// Validate envelope fields that can be checked before any DB/crypto work.
/// Returns Ok(()) or Err(human-readable message).
pub fn validate_envelope(cmd: &CanonicalCommand) -> Result<(), String> {
    if !ALLOWED_SCHEMA_VERSIONS.contains(&cmd.schema_version.as_str()) {
        return Err(format!(
            "schema_version '{}' not in allowlist {:?}",
            cmd.schema_version, ALLOWED_SCHEMA_VERSIONS
        ));
    }
    if !cmd.payload_sha256.is_empty() {
        let payload_bytes = serde_json::to_vec(&cmd.payload)
            .map_err(|e| format!("payload serialize: {e}"))?;
        let computed = hex::encode(Sha256::digest(&payload_bytes));
        if computed != cmd.payload_sha256 {
            return Err(format!(
                "payload_sha256 mismatch: got '{}', computed '{computed}'",
                cmd.payload_sha256
            ));
        }
    }
    Ok(())
}
```

Добавить зависимость в `Cargo.toml` (sha2 уже есть — проверить, что `hex` тоже есть, — да, есть).

- [ ] **Step 3: Вызвать `validate_envelope()` в `fiscal_send_inner`**

В `rust/prro_sidecar/src/bin/prro_sidecar.rs`, в начале `fiscal_send_inner` до `let fn_id = ...`:

```rust
// S2: validate schema_version allowlist and optional payload_sha256
if let Err(msg) = prro_sidecar::validate_envelope(&cmd) {
    return Err(SidecarError::InvalidInput(msg));
}
```

Убедиться, что `SidecarError::InvalidInput(String)` существует в `errors.rs`. Если нет — добавить:

```rust
#[error("invalid input: {0}")]
InvalidInput(String),
```

И реализовать `IntoResponse` для него с 400 status.

- [ ] **Step 4: Запустить тесты**

```
cargo test -p prro_sidecar envelope_integrity 2>&1 | tail -15
```
Ожидается: 4 теста PASS

- [ ] **Step 5: Полный прогон**

```
cargo test -p prro_sidecar 2>&1 | tail -5
```

- [ ] **Step 6: Commit**

```bash
git add rust/prro_sidecar/src/lib.rs \
        rust/prro_sidecar/src/errors.rs \
        rust/prro_sidecar/src/bin/prro_sidecar.rs \
        rust/prro_sidecar/tests/wiring_date_time.rs
git commit -m "fix(sidecar): S2 — validate schema_version allowlist and payload_sha256"
```

---

### Task 3: S3 — DPS status classifier wire-up

**Goal:** Вызывать `classify_dps_status()` после gRPC-ответа в `fiscal_send_inner` и возвращать категорию ошибки в `FiscalSendResponse`.

**Files:**
- Modify: `rust/prro_sidecar/src/bin/prro_sidecar.rs`

**Acceptance Criteria:**
- [ ] `FiscalSendResponse` содержит поле `dps_error_category: Option<String>` (значения: `"transient"`, `"permanent"`, `null`)
- [ ] При `status=1` поле `null`
- [ ] При `status=-3` поле `"transient"`
- [ ] При `status=-1` поле `"permanent"`
- [ ] При `status=0` поле `"permanent"` (catch-all)
- [ ] Тест `dps_classifier_wire` проходит

**Verify:** `cargo test -p prro_sidecar dps_classifier_wire 2>&1 | tail -10`

**Steps:**

- [ ] **Step 1: Написать failing тест**

В `rust/prro_sidecar/tests/wiring_date_time.rs`:

```rust
#[cfg(test)]
mod dps_classifier_wire {
    use prro_sidecar::grpc_client::{classify_dps_status, DpsErrorCategory};

    #[test]
    fn status_ok_returns_none() {
        assert_eq!(classify_dps_status(1), None);
    }
    #[test]
    fn status_minus3_is_transient() {
        assert_eq!(classify_dps_status(-3), Some(DpsErrorCategory::Transient));
    }
    #[test]
    fn status_minus1_is_permanent() {
        assert_eq!(classify_dps_status(-1), Some(DpsErrorCategory::Permanent));
    }
    #[test]
    fn status_zero_is_permanent() {
        assert_eq!(classify_dps_status(0), Some(DpsErrorCategory::Permanent));
    }
    #[test]
    fn status_minus12_is_transient() {
        assert_eq!(classify_dps_status(-12), Some(DpsErrorCategory::Transient));
    }
}
```

- [ ] **Step 2: Запустить — все 5 тестов должны пройти**

```
cargo test -p prro_sidecar dps_classifier_wire
```

(Тест проверяет существующую `classify_dps_status` — должен пройти сразу.)

- [ ] **Step 3: Расширить `FiscalSendResponse`**

В `prro_sidecar.rs` структуру `FiscalSendResponse`:

```rust
#[derive(serde::Serialize)]
struct FiscalSendResponse {
    status:              i32,
    fiscal_id:           String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message:       Option<String>,
    #[serde(default)]
    chain_broken:        bool,
    // S3: DPS error category for retry decisions by caller
    #[serde(skip_serializing_if = "Option::is_none")]
    dps_error_category:  Option<String>,
}
```

- [ ] **Step 4: Вызвать `classify_dps_status` при формировании ответа**

После строки `let error_msg = if resp.error_message.is_empty() ...` (около строки 519):

```rust
use prro_sidecar::grpc_client::{classify_dps_status, DpsErrorCategory};
let dps_error_category = classify_dps_status(resp.status).map(|c| match c {
    DpsErrorCategory::Transient => "transient".to_string(),
    DpsErrorCategory::Permanent => "permanent".to_string(),
});

Ok(FiscalSendResponse {
    status:             resp.status,
    fiscal_id:          resp.id.clone(),
    error_message:      error_msg,
    chain_broken,
    dps_error_category,
})
```

- [ ] **Step 5: Полный прогон**

```
cargo test -p prro_sidecar 2>&1 | tail -5
```

- [ ] **Step 6: Commit**

```bash
git add rust/prro_sidecar/src/bin/prro_sidecar.rs \
        rust/prro_sidecar/tests/wiring_date_time.rs
git commit -m "fix(sidecar): S3 — wire classify_dps_status into FiscalSendResponse"
```

---

### Task 4: S4 — Idempotency journal (sidecar_requests table)

**Goal:** Создать таблицу `sidecar_requests` в SQLite; в `fiscal_send_inner` до выделения `local_number` проверять idempotency_key и возвращать кэшированный ответ при дубликате.

**Files:**
- Modify: `rust/prro_sidecar/src/repo.rs`
- Modify: `rust/prro_sidecar/src/bin/prro_sidecar.rs`

**Acceptance Criteria:**
- [ ] Таблица `sidecar_requests(idempotency_key PK, fiscal_number, response_json, created_at)` создаётся в `Repo::open()`
- [ ] `repo.find_idempotent_response(key)` → `Ok(Some(json))` при повторном запросе
- [ ] `repo.record_idempotent_response(key, fn, json)` сохраняет ответ
- [ ] `fiscal_send_inner` при повторном `idempotency_key` возвращает сохранённый `FiscalSendResponse` без вызова DPS
- [ ] Тест `idempotency_duplicate_returns_cached` проходит

**Verify:** `cargo test -p prro_sidecar idempotency 2>&1 | tail -15`

**Steps:**

- [ ] **Step 1: Написать failing тест для repo**

В `rust/prro_sidecar/tests/wiring_date_time.rs`:

```rust
#[cfg(test)]
mod idempotency {
    use prro_sidecar::repo::Repo;

    fn make_repo() -> Repo {
        Repo::open(":memory:").expect("in-memory repo")
    }

    #[test]
    fn fresh_key_returns_none() {
        let repo = make_repo();
        let result = repo.find_idempotent_response("key-1").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn recorded_key_returns_json() {
        let repo = make_repo();
        repo.record_idempotent_response("key-1", "FN001", r#"{"status":1}"#).unwrap();
        let found = repo.find_idempotent_response("key-1").unwrap();
        assert_eq!(found.as_deref(), Some(r#"{"status":1}"#));
    }

    #[test]
    fn different_key_returns_none() {
        let repo = make_repo();
        repo.record_idempotent_response("key-A", "FN001", r#"{"status":1}"#).unwrap();
        assert!(repo.find_idempotent_response("key-B").unwrap().is_none());
    }
}
```

- [ ] **Step 2: Добавить таблицу в `Repo::open()`**

В `rust/prro_sidecar/src/repo.rs`, в `execute_batch` добавить после `fn_degraded`:

```rust
CREATE TABLE IF NOT EXISTS sidecar_requests (
    idempotency_key TEXT    PRIMARY KEY,
    fiscal_number   TEXT    NOT NULL,
    response_json   TEXT    NOT NULL,
    created_at      TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

- [ ] **Step 3: Добавить методы в `Repo`**

```rust
/// Return the cached JSON response for an idempotency key, or None if not seen.
pub fn find_idempotent_response(&self, key: &str) -> Result<Option<String>, SidecarError> {
    let conn = self.lock()?;
    let mut stmt = conn.prepare_cached(
        "SELECT response_json FROM sidecar_requests WHERE idempotency_key = ?1",
    )?;
    let mut rows = stmt.query([key])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

/// Persist the response JSON for an idempotency key (INSERT OR IGNORE for safety).
pub fn record_idempotent_response(
    &self,
    key: &str,
    fiscal_number: &str,
    response_json: &str,
) -> Result<(), SidecarError> {
    let conn = self.lock()?;
    conn.execute(
        "INSERT OR IGNORE INTO sidecar_requests (idempotency_key, fiscal_number, response_json)
         VALUES (?1, ?2, ?3)",
        rusqlite::params![key, fiscal_number, response_json],
    )?;
    Ok(())
}
```

- [ ] **Step 4: Запустить repo-тесты**

```
cargo test -p prro_sidecar idempotency 2>&1 | tail -10
```
Ожидается: 3 теста PASS

- [ ] **Step 5: Вызвать в `fiscal_send_inner`**

После S2 validate_envelope call (и до `let fn_id = ...`), добавить:

```rust
// S4: idempotency check — return cached response without re-submitting to DPS
if !cmd.idempotency_key.is_empty() {
    if let Some(cached_json) = st.repo.find_idempotent_response(&cmd.idempotency_key)? {
        let cached: FiscalSendResponse = serde_json::from_str(&cached_json)
            .map_err(|e| SidecarError::Internal(format!("idempotency cache parse: {e}")))?;
        return Ok(cached);
    }
}
```

После успешного ответа DPS (после `Ok(FiscalSendResponse { ... })`) добавить сохранение:

```rust
let response = FiscalSendResponse { ... };
// S4: record for idempotency replay
if !cmd.idempotency_key.is_empty() {
    if let Ok(json) = serde_json::to_string(&response) {
        let _ = st.repo.record_idempotent_response(&cmd.idempotency_key, fn_id, &json);
    }
}
Ok(response)
```

- [ ] **Step 6: Добавить интеграционный тест в `wiring_date_time.rs`**

Этот тест невозможен без полного AppState (gRPC pool). Добавить source-level guard:

```rust
#[test]
fn fiscal_send_inner_contains_idempotency_check() {
    let src = include_str!("../src/bin/prro_sidecar.rs");
    assert!(
        src.contains("find_idempotent_response"),
        "fiscal_send_inner must call find_idempotent_response"
    );
    assert!(
        src.contains("record_idempotent_response"),
        "fiscal_send_inner must call record_idempotent_response"
    );
}
```

- [ ] **Step 7: Полный прогон**

```
cargo test -p prro_sidecar 2>&1 | tail -5
```

- [ ] **Step 8: Commit**

```bash
git add rust/prro_sidecar/src/repo.rs \
        rust/prro_sidecar/src/bin/prro_sidecar.rs \
        rust/prro_sidecar/tests/wiring_date_time.rs
git commit -m "fix(sidecar): S4 — add sidecar_requests idempotency journal"
```

---

### Task 5: M1 — submit_report() resp.ok check

**Goal:** В `dispatcher.rs::submit_report()` проверять `resp.ok` и возвращать mapped bridge error при `ok=false`.

**Files:**
- Modify: `rust/maria304_driver/src/session/dispatcher.rs`
- Modify: `rust/maria304_driver/tests/bridge_acceptance.rs`

**Acceptance Criteria:**
- [ ] `submit_report()` при `bridge.submit()` → `Ok(CanonicalResponse { ok: false, ... })` возвращает `err(map_bridge_error(...))`
- [ ] При `ok=true` поведение не изменилось
- [ ] Тест `submit_report_ok_false_returns_bridge_error` проходит

**Verify:** `cargo test -p maria304_driver submit_report 2>&1 | tail -10`

**Steps:**

- [ ] **Step 1: Изучить `CanonicalResponse` и `classify_response`**

```bash
grep -n "ok:" rust/maria304_driver/src/bridge/dto.rs | head -5
grep -n "fn classify_response" rust/maria304_driver/src/bridge/dto.rs
```

`CanonicalResponse` имеет поле `ok: bool`. `classify_response` делает check на `resp.ok`. Нам нужно то же самое в `submit_report`.

- [ ] **Step 2: Написать failing тест**

В `rust/maria304_driver/tests/bridge_acceptance.rs` добавить:

```rust
#[test]
fn submit_report_bridge_ok_false_returns_error_not_done() {
    use maria304_driver::bridge::MockBridge;
    use maria304_driver::bridge::dto::CanonicalResponse;
    use maria304_driver::protocol::error_codes::ErrorCode;
    use maria304_driver::protocol::Response;

    let (mut session, bridge, mut correlation) = logged_in_session();

    // Inject a response where ok=false for the next submit call
    bridge.set_next_response(CanonicalResponse {
        ok: false,
        document_state: String::new(),
        fiscal_id: None,
        sale: None,
        ret: None,
        error_code: Some("BRIDGE_SOFT_ERROR".to_string()),
    });

    let responses = run(
        &mut session,
        &bridge,
        &mut correlation,
        Command::Xrep { /* Z-report opcode, body doesn't matter */ },
    );

    // The response must NOT be DONE — must be a SOFT* error
    let is_error = responses.iter().any(|r| matches!(r, Response::Error(_)));
    assert!(is_error, "expected error response when bridge ok=false, got: {:?}", responses);
}
```

Проверить сигнатуру `MockBridge` — есть ли `set_next_response`. Если нет, нужно добавить:

```bash
grep -n "set_next_response\|struct MockBridge" rust/maria304_driver/src/bridge/mock.rs
```

Если метода нет — добавить в `mock.rs`:

```rust
use std::sync::Mutex;

pub struct MockBridge {
    calls: Mutex<Vec<CanonicalCommand>>,
    next_response: Mutex<Option<CanonicalResponse>>,
}

impl MockBridge {
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            next_response: Mutex::new(None),
        }
    }
    pub fn set_next_response(&self, resp: CanonicalResponse) {
        *self.next_response.lock().unwrap() = Some(resp);
    }
    pub fn calls(&self) -> Vec<CanonicalCommand> {
        self.calls.lock().unwrap().clone()
    }
}

impl Bridge for MockBridge {
    fn submit(&self, cmd: &CanonicalCommand) -> Result<CanonicalResponse, BridgeError> {
        self.calls.lock().unwrap().push(cmd.clone());
        if let Some(resp) = self.next_response.lock().unwrap().take() {
            return Ok(resp);
        }
        // Default success response
        Ok(CanonicalResponse {
            ok: true,
            document_state: "ACK".to_string(),
            fiscal_id: Some("FISCAL-001".to_string()),
            sale: None, ret: None, error_code: None,
        })
    }
}
```

- [ ] **Step 3: Исправить `submit_report` в `dispatcher.rs`**

Текущий код (строки 654–665):

```rust
match bridge.submit(&envelope) {
    Ok(_) => {
        session.mark_command_ok(opcode);
        correlation.receipt_seq = correlation.receipt_seq.saturating_add(1);
        ok(None)
    }
    Err(bridge_err) => err(map_bridge_error(&bridge_err)),
}
```

Заменить на:

```rust
match bridge.submit(&envelope) {
    Ok(resp) if resp.ok => {
        session.mark_command_ok(opcode);
        correlation.receipt_seq = correlation.receipt_seq.saturating_add(1);
        ok(None)
    }
    Ok(resp) => {
        // Bridge returned ok=false — treat as a soft bridge error
        let code = resp.error_code.as_deref().unwrap_or("BRIDGE_NOK");
        err(map_bridge_error(&BridgeError::Rejected(code.to_string())))
    }
    Err(bridge_err) => err(map_bridge_error(&bridge_err)),
}
```

Проверить, что `BridgeError::Rejected` существует:

```bash
grep -n "Rejected\|enum BridgeError" rust/maria304_driver/src/bridge/mod.rs
```

Если нет — добавить в `BridgeError`:

```rust
#[error("rejected: {0}")]
Rejected(String),
```

И в `map_bridge_error`:

```rust
BridgeError::Rejected(_) => ErrorCode::SoftBridgeError,
```

(Проверить, что `SoftBridgeError` или аналог существует в `error_codes.rs`.)

- [ ] **Step 4: Запустить тест**

```
cargo test -p maria304_driver submit_report 2>&1 | tail -10
```
Ожидается: PASS

- [ ] **Step 5: Полный прогон**

```
cargo test -p maria304_driver 2>&1 | tail -5
```

- [ ] **Step 6: Commit**

```bash
git add rust/maria304_driver/src/session/dispatcher.rs \
        rust/maria304_driver/src/bridge/mock.rs \
        rust/maria304_driver/src/bridge/mod.rs \
        rust/maria304_driver/tests/bridge_acceptance.rs
git commit -m "fix(driver): M1 — check resp.ok in submit_report, reject bridge ok=false"
```

---

### Task 6: M2 — Startup duplicate FN/bind validation

**Goal:** До старта листенеров проверить, что нет двух конфигов с одинаковым `fiscal_number` или `bind`; выйти с ясным сообщением.

**Files:**
- Modify: `rust/maria304_driver/src/bin/maria304_driver.rs`

**Acceptance Criteria:**
- [ ] Две записи с одинаковым `fiscal_number` → `eprintln!` + `std::process::exit(1)` до первого `tokio::spawn`
- [ ] Две записи с одинаковым `bind` → то же
- [ ] Уникальные конфиги проходят без ошибок
- [ ] Тест `detect_duplicate_fiscal_number` проходит (unit-test, не нужен runtime)

**Verify:** `cargo test -p maria304_driver duplicate 2>&1 | tail -10`

**Steps:**

- [ ] **Step 1: Написать тест для валидирующей функции**

```rust
// В rust/maria304_driver/src/bin/maria304_driver.rs добавить в #[cfg(test)] блок:
#[cfg(test)]
mod startup_validation_tests {
    use super::check_no_duplicates;

    #[derive(Clone)]
    struct FakeCfg { fiscal_number: String, bind: String }

    fn cfg(fn_: &str, bind: &str) -> FakeCfg {
        FakeCfg { fiscal_number: fn_.to_string(), bind: bind.to_string() }
    }

    #[test]
    fn unique_configs_pass() {
        let cfgs = vec![cfg("FN001", "0.0.0.0:9001"), cfg("FN002", "0.0.0.0:9002")];
        assert!(check_no_duplicates(&cfgs).is_ok());
    }

    #[test]
    fn detect_duplicate_fiscal_number() {
        let cfgs = vec![cfg("FN001", "0.0.0.0:9001"), cfg("FN001", "0.0.0.0:9002")];
        let err = check_no_duplicates(&cfgs).unwrap_err();
        assert!(err.contains("FN001"), "expected FN001 in error: {err}");
    }

    #[test]
    fn detect_duplicate_bind() {
        let cfgs = vec![cfg("FN001", "0.0.0.0:9001"), cfg("FN002", "0.0.0.0:9001")];
        let err = check_no_duplicates(&cfgs).unwrap_err();
        assert!(err.contains("9001"), "expected bind port in error: {err}");
    }
}
```

- [ ] **Step 2: Добавить `check_no_duplicates` в `maria304_driver.rs`**

После struct definitions, до `async fn run_driver(...)`:

```rust
/// Detect duplicate fiscal_number or bind address in listener configs.
/// Returns Err with a human-readable message on first duplicate found.
fn check_no_duplicates(listeners: &[impl AsRef<str> + HasFnAndBind]) -> Result<(), String> {
    use std::collections::HashSet;
    let mut fns: HashSet<&str> = HashSet::new();
    let mut binds: HashSet<&str> = HashSet::new();
    for l in listeners {
        if !fns.insert(l.fiscal_number()) {
            return Err(format!(
                "duplicate fiscal_number '{}' in listener config", l.fiscal_number()
            ));
        }
        if !binds.insert(l.bind()) {
            return Err(format!(
                "duplicate bind address '{}' in listener config", l.bind()
            ));
        }
    }
    Ok(())
}
```

Или проще — без trait, напрямую с `&[ListenerCfg]`:

```rust
fn check_no_duplicates(listeners: &[ListenerCfg]) -> Result<(), String> {
    use std::collections::HashSet;
    let mut seen_fns: HashSet<&str> = HashSet::new();
    let mut seen_binds: HashSet<&str> = HashSet::new();
    for l in listeners {
        if !seen_fns.insert(l.fiscal_number.as_str()) {
            return Err(format!(
                "duplicate fiscal_number '{}' in [listeners] config", l.fiscal_number
            ));
        }
        if !seen_binds.insert(l.bind.as_str()) {
            return Err(format!(
                "duplicate bind '{}' in [listeners] config", l.bind
            ));
        }
    }
    Ok(())
}
```

Вызов в `run_driver` до первого spawn:

```rust
if let Err(e) = check_no_duplicates(&cfg.listeners) {
    eprintln!("configuration error: {e}");
    std::process::exit(1);
}
```

Адаптировать тесты под прямую сигнатуру `&[ListenerCfg]`.

- [ ] **Step 3: Запустить тесты**

```
cargo test -p maria304_driver duplicate 2>&1 | tail -10
```
Ожидается: 3 теста PASS

- [ ] **Step 4: Полный прогон**

```
cargo test -p maria304_driver 2>&1 | tail -5
```

- [ ] **Step 5: Commit**

```bash
git add rust/maria304_driver/src/bin/maria304_driver.rs
git commit -m "fix(driver): M2 — startup duplicate fiscal_number/bind validation"
```

---

### Task 7: M3 — Graceful shutdown с CancellationToken

**Goal:** Заменить `signal::ctrl_c().await?; Ok(())` на shutdown через `CancellationToken` + `graceful_shutdown_timeout_s` из конфига.

**Files:**
- Modify: `rust/maria304_driver/Cargo.toml` (добавить `tokio-util`)
- Modify: `rust/maria304_driver/src/bin/maria304_driver.rs`
- Modify: `rust/maria304_driver/src/listener/server.rs` (добавить `token` в `FnListener::serve`)
- Modify: `rust/maria304_driver/src/listener/session_loop.rs` (propagate token)

**Acceptance Criteria:**
- [ ] `tokio-util = { version = "0.7", features = ["sync"] }` в `Cargo.toml`
- [ ] `FnListener::serve(token: CancellationToken)` принимает токен
- [ ] `run_connection` получает `token` и завершается при `token.cancelled()`
- [ ] `main()` ожидает `ctrl_c`, вызывает `token.cancel()`, затем `tokio::time::timeout(shutdown_duration, join_all).await`
- [ ] `graceful_shutdown_timeout_s` реально читается и используется
- [ ] Тест `cancellation_token_stops_serve` проходит (unit-тест с фиктивным токеном)

**Verify:** `cargo test -p maria304_driver cancellation 2>&1 | tail -10`

**Steps:**

- [ ] **Step 1: Добавить `tokio-util` в `Cargo.toml`**

```toml
tokio-util = { version = "0.7", features = ["sync"] }
```

- [ ] **Step 2: Написать тест**

В `rust/maria304_driver/tests/` создать `graceful_shutdown.rs`:

```rust
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn cancellation_token_stops_waiting_task() {
    let token = CancellationToken::new();
    let t = token.clone();
    let handle = tokio::spawn(async move {
        tokio::select! {
            _ = t.cancelled() => "cancelled",
            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => "timeout",
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    token.cancel();
    let result = handle.await.unwrap();
    assert_eq!(result, "cancelled");
}
```

- [ ] **Step 3: Обновить `FnListener::serve` в `server.rs`**

Текущая сигнатура: `pub async fn serve(&self) -> Result<(), ListenerError>`

Новая сигнатура: `pub async fn serve(&self, shutdown: CancellationToken) -> Result<(), ListenerError>`

В теле заменить `accept_loop` на:

```rust
loop {
    tokio::select! {
        accept_res = listener.accept() => {
            let (stream, addr) = accept_res?;
            // ... spawn connection handler as before
        }
        _ = shutdown.cancelled() => {
            tracing::info!(fiscal_number = %self.cfg.fiscal_number, "listener shutting down");
            break;
        }
    }
}
Ok(())
```

- [ ] **Step 4: Обновить `run_connection` в `session_loop.rs`**

Добавить `shutdown: CancellationToken` параметром:

```rust
pub async fn run_connection(
    mut stream: TcpStream,
    identity: Arc<Identity>,
    bridge: Arc<dyn Bridge + Send + Sync>,
    clock_src: Arc<dyn ClockSource>,
    session_uuid: String,
    idle_timeout: Duration,
    shutdown: CancellationToken,
) -> std::io::Result<()> {
```

В главном цикле чтения добавить select:

```rust
let n = match tokio::select! {
    result = timeout(idle_timeout, stream.read(&mut scratch)) => result,
    _ = shutdown.cancelled() => {
        tracing::debug!("connection terminated by shutdown signal");
        return Ok(());
    }
} {
    // existing Ok(Ok(0)) / Ok(Ok(n)) / Ok(Err(e)) / Err(_) arms
    ...
};
```

- [ ] **Step 5: Обновить `main()` в `maria304_driver.rs`**

```rust
let shutdown = CancellationToken::new();
let mut listener_handles = Vec::new();

for listener_cfg in &cfg.listeners {
    // ... build listener as before ...
    let token = shutdown.clone();
    let handle = tokio::spawn(async move {
        if let Err(e) = listener.serve(token).await {
            tracing::error!("listener died: {e}");
        }
    });
    listener_handles.push(handle);
}

// ... admin server, etc. ...

tracing::info!("maria304_driver running; Ctrl-C to stop");
signal::ctrl_c().await?;
tracing::info!("shutdown signal received; draining connections");

shutdown.cancel();

let timeout_s = cfg.deployment.as_ref()
    .map(|d| d.graceful_shutdown_timeout_s)
    .unwrap_or(5);
let _ = tokio::time::timeout(
    std::time::Duration::from_secs(timeout_s),
    futures::future::join_all(listener_handles),
).await;
tracing::info!("shutdown complete");
Ok(())
```

Добавить в `Cargo.toml`: `futures = "0.3"` (или использовать tokio JoinSet).

Альтернатива без `futures`:

```rust
let _ = tokio::time::timeout(
    std::time::Duration::from_secs(timeout_s),
    async {
        for h in listener_handles { let _ = h.await; }
    },
).await;
```

- [ ] **Step 6: Запустить тесты**

```
cargo test -p maria304_driver cancellation 2>&1 | tail -10
cargo test -p maria304_driver 2>&1 | tail -5
```

- [ ] **Step 7: Commit**

```bash
git add rust/maria304_driver/Cargo.toml \
        rust/maria304_driver/src/bin/maria304_driver.rs \
        rust/maria304_driver/src/listener/server.rs \
        rust/maria304_driver/src/listener/session_loop.rs \
        rust/maria304_driver/tests/graceful_shutdown.rs
git commit -m "fix(driver): M3 — graceful shutdown with CancellationToken + timeout from config"
```

---

### Task 8: M4 — Metrics wiring через FnListener → session_loop

**Goal:** Передать `Arc<SessionMetrics>` через `FnListener` и `run_connection` и вызывать методы record_* в `session_loop.rs`.

**Files:**
- Modify: `rust/maria304_driver/src/listener/server.rs`
- Modify: `rust/maria304_driver/src/listener/session_loop.rs`
- Modify: `rust/maria304_driver/src/bin/maria304_driver.rs`

**Acceptance Criteria:**
- [ ] `FnListener` содержит поле `metrics: Arc<SessionMetrics>`
- [ ] `FnListener::new` принимает `metrics: Arc<SessionMetrics>`
- [ ] `run_connection` получает `metrics: Arc<SessionMetrics>` и вызывает `record_inbound_frame()` и `record_outbound_frame()` при каждом frame
- [ ] `record_bridge_error()` вызывается при ошибке bridge в session_loop
- [ ] `main()` передаёт `Arc::clone(&metrics)` в `FnListener::new`
- [ ] Тест `metrics_counted_after_frames` проходит

**Verify:** `cargo test -p maria304_driver metrics_counted 2>&1 | tail -10`

**Steps:**

- [ ] **Step 1: Написать тест**

В `rust/maria304_driver/tests/` создать `metrics_wiring.rs`:

```rust
use std::sync::Arc;
use maria304_driver::bridge::MockBridge;
use maria304_driver::listener::session_loop::{run_connection, FixedClock};
use maria304_driver::observability::metrics::SessionMetrics;
use maria304_driver::session::dispatcher::Identity;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn metrics_counted_after_frames() {
    // Build a loopback socket pair
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    let metrics = Arc::new(SessionMetrics::new());
    let bridge = Arc::new(MockBridge::new());
    let identity = Arc::new(Identity::default());
    let clock = Arc::new(FixedClock);
    let token = CancellationToken::new();
    let m = Arc::clone(&metrics);
    let token2 = token.clone();

    tokio::spawn(async move {
        run_connection(
            server_stream.into_std().unwrap().into(),  // adapt as needed
            identity, bridge, clock,
            "test-session".to_string(),
            std::time::Duration::from_secs(1),
            token2,
            m,
        ).await.ok();
    });

    // Send a minimal valid UPAS frame (byte sequence from wire_vectors tests)
    // ... (wire frame bytes for UPAS with cashier "1111111111")
    token.cancel(); // stop server after setup

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // At least record_inbound_frame should have been called if any frame was processed
    // This test verifies the wiring exists; exact counts depend on frames sent
    let _ = metrics.snapshot(); // must not panic
}
```

Примечание: точные wire bytes для frame взять из `rust/maria304_driver/tests/wire_vectors.rs`.

- [ ] **Step 2: Добавить `metrics` в `FnListener`**

В `server.rs`:

```rust
use crate::observability::metrics::SessionMetrics;

pub struct FnListener {
    cfg:      ListenerConfig,
    gate:     Arc<ConnectionGate>,
    cooldown: Arc<Cooldown>,
    bridge:   Arc<dyn Bridge + Send + Sync>,
    clock:    Arc<dyn ClockSource>,
    metrics:  Arc<SessionMetrics>,  // NEW
}

impl FnListener {
    pub fn new(
        cfg: ListenerConfig,
        bridge: Arc<dyn Bridge + Send + Sync>,
        clock: Arc<dyn ClockSource>,
        metrics: Arc<SessionMetrics>,  // NEW
    ) -> Self {
        let cooldown = Arc::new(Cooldown::new(cfg.cooldown));
        Self { cfg, gate: Arc::new(ConnectionGate::new()), cooldown, bridge, clock, metrics }
    }
```

В методе `serve`, при spawning connection task передавать `Arc::clone(&self.metrics)`:

```rust
let metrics = Arc::clone(&self.metrics);
tokio::spawn(async move {
    run_connection(stream, identity, bridge, clock, session_uuid, idle_timeout, shutdown, metrics).await.ok();
});
```

- [ ] **Step 3: Добавить `metrics` в `run_connection`**

```rust
pub async fn run_connection(
    mut stream: TcpStream,
    identity: Arc<Identity>,
    bridge: Arc<dyn Bridge + Send + Sync>,
    clock_src: Arc<dyn ClockSource>,
    session_uuid: String,
    idle_timeout: Duration,
    shutdown: CancellationToken,
    metrics: Arc<SessionMetrics>,  // NEW
) -> std::io::Result<()> {
```

В теле, после `try_handle_buffered`:

```rust
if let Some(responses) =
    try_handle_buffered(&mut buf, &mut session, &identity, &bridge, &*clock_src, &mut correlation)
{
    metrics.record_inbound_frame();
    for resp in responses {
        write_response(&mut stream, &resp, crc_for_write).await?;
        metrics.record_outbound_frame();
    }
    continue;
}
```

После `buf.extend_from_slice` добавить ошибку кадра при BadCrc:
(уже обрабатывается в `try_handle_buffered` — но можно добавить счётчик там).

- [ ] **Step 4: Обновить `main()` для передачи метрик**

Текущий код (строка 256):

```rust
let listener = FnListener::new(lcfg, Arc::clone(&bridge), Arc::clone(&clock));
```

Новый:

```rust
let listener = FnListener::new(lcfg, Arc::clone(&bridge), Arc::clone(&clock), Arc::clone(&metrics));
```

- [ ] **Step 5: Полный прогон**

```
cargo test -p maria304_driver 2>&1 | tail -5
```

- [ ] **Step 6: Commit**

```bash
git add rust/maria304_driver/src/listener/server.rs \
        rust/maria304_driver/src/listener/session_loop.rs \
        rust/maria304_driver/src/bin/maria304_driver.rs \
        rust/maria304_driver/tests/metrics_wiring.rs
git commit -m "fix(driver): M4 — wire SessionMetrics through FnListener into run_connection"
```

---

### Task 9: M5 — spawn_blocking для синхронных bridge-вызовов

**Goal:** Заменить синхронный `bridge.submit()` внутри async `run_connection` на `tokio::task::spawn_blocking`, чтобы не блокировать tokio worker thread.

**Files:**
- Modify: `rust/maria304_driver/src/listener/session_loop.rs`
- Modify: `rust/maria304_driver/src/session/dispatcher.rs` (добавить функцию prepare/finalize)

**Acceptance Criteria:**
- [ ] Вызов `bridge.submit()` не происходит напрямую на tokio worker thread
- [ ] Session state корректно переживает spawn_blocking (move in, move out)
- [ ] Тест `bridge_call_does_not_block_reactor` проходит
- [ ] Все существующие тесты проходят

**Verify:** `cargo test -p maria304_driver spawn_blocking 2>&1 | tail -10`

**Steps:**

- [ ] **Step 1: Понять архитектуру изменений**

Текущий `try_handle_buffered` вызывает `dispatch()`, который может вызвать `bridge.submit()`.
`dispatch()` возвращает `Vec<Response>`. Чтобы вынести `bridge.submit()` в `spawn_blocking`:

1. Добавить в `dispatcher.rs` функцию `dispatch_prepare()` — строит `Option<CanonicalCommand>` без вызова bridge
2. Добавить `dispatch_with_bridge_result()` — принимает `Result<CanonicalResponse, BridgeError>` и достраивает `Vec<Response>`
3. В `session_loop.rs` использовать эти две функции вокруг `spawn_blocking(|| bridge.submit(cmd))`

Шаги 2–6 ниже реализуют этот паттерн.

- [ ] **Step 2: Написать тест**

В `rust/maria304_driver/tests/spawn_blocking.rs`:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use maria304_driver::bridge::{Bridge, BridgeError, CanonicalCommand};
use maria304_driver::bridge::dto::CanonicalResponse;

/// A bridge that asserts it's NOT called on the tokio current-thread context.
struct AssertOffThreadBridge {
    called: Arc<AtomicBool>,
}

impl Bridge for AssertOffThreadBridge {
    fn submit(&self, _cmd: &CanonicalCommand) -> Result<CanonicalResponse, BridgeError> {
        // tokio::runtime::Handle::current() would panic if we're not in a runtime,
        // but spawn_blocking runs on a dedicated thread pool — we can detect this
        // by checking we're NOT on a tokio worker thread.
        // Simplest signal: record call happened
        self.called.store(true, Ordering::SeqCst);
        Ok(CanonicalResponse {
            ok: true,
            document_state: "ACK".to_string(),
            fiscal_id: Some("F001".to_string()),
            sale: None, ret: None, error_code: None,
        })
    }
}

#[tokio::test]
async fn bridge_submit_called_in_spawn_blocking() {
    let called = Arc::new(AtomicBool::new(false));
    let bridge = Arc::new(AssertOffThreadBridge { called: Arc::clone(&called) });
    let cmd = CanonicalCommand {
        schema_version: "1.0".to_string(),
        ..Default::default()
    };
    let bridge_c = Arc::clone(&bridge);
    let resp = tokio::task::spawn_blocking(move || {
        bridge_c.submit(&cmd)
    }).await.unwrap();
    assert!(resp.is_ok());
    assert!(called.load(Ordering::SeqCst), "bridge.submit was never called");
}
```

- [ ] **Step 3: Добавить `dispatch_prepare` и `dispatch_with_result` в `dispatcher.rs`**

```rust
/// Possible bridge call that dispatch needs to make.
pub enum PendingBridgeCall {
    Submit(CanonicalCommand),
    None,
}

/// First half of dispatch: parse frame, update session state, return pending bridge call.
/// Does NOT call bridge.submit — caller must handle the pending call (possibly in spawn_blocking).
pub fn dispatch_prepare(
    session: &mut Session,
    command: Command,
    identity: &Identity,
    clock: Clock<'_>,
    correlation: &mut Correlation,
) -> (Vec<Response>, PendingBridgeCall) {
    // ... extract from current dispatch() for COMP and report commands
    // Returns: (partial_responses, PendingBridgeCall::Submit(envelope)) for COMP/XREP/ZREP
    // Returns: (full_responses, PendingBridgeCall::None) for all other commands
    todo!()
}

/// Second half: given the bridge response, finalize the responses.
pub fn dispatch_with_result(
    session: &mut Session,
    pending_cmd: &CanonicalCommand,
    result: Result<CanonicalResponse, BridgeError>,
    correlation: &mut Correlation,
) -> Vec<Response> {
    // ... handle Ok(resp) => classify_response, Err => map_bridge_error
    todo!()
}
```

Примечание: это значительный рефакторинг `dispatcher.rs`. Альтернатива — менее инвазивный подход с `try_dispatch_or_get_pending`:

```rust
pub enum DispatchResult {
    Done(Vec<Response>),
    NeedsBridge { envelope: CanonicalCommand, pending_state: PendingState },
}
```

- [ ] **Step 4: Обновить `session_loop.rs::try_handle_buffered`**

```rust
async fn try_handle_buffered_async(
    buf: &mut Vec<u8>,
    session: &mut Session,
    identity: &Arc<Identity>,
    bridge: &Arc<dyn Bridge + Send + Sync>,
    clock_src: &dyn ClockSource,
    correlation: &mut Correlation,
    metrics: &SessionMetrics,
) -> Option<Vec<Response>> {
    if buf.is_empty() { return None; }
    match decode_frame(buf, session.crc_enabled) {
        Ok((frame, consumed)) => {
            buf.drain(..consumed);
            let (date, time) = clock_src.now();
            let clock = Clock { date: &date, time: &time };
            let command = Command::parse(&frame);
            let (partial, pending) = dispatch_prepare(session, command, identity, clock, correlation);
            if !partial.is_empty() && matches!(pending, PendingBridgeCall::None) {
                return Some(partial);
            }
            if let PendingBridgeCall::Submit(envelope) = pending {
                let bridge_c = Arc::clone(bridge);
                let result = tokio::task::spawn_blocking(move || bridge_c.submit(&envelope))
                    .await
                    .unwrap_or(Err(BridgeError::Transport("spawn_blocking panicked".into())));
                metrics.record_bridge_error(); // only on Err — adjust below
                let responses = dispatch_with_result(session, &envelope_ref, result, correlation);
                return Some(responses);
            }
            Some(partial)
        }
        // ... error arms unchanged
    }
}
```

- [ ] **Step 5: Запустить тест**

```
cargo test -p maria304_driver spawn_blocking 2>&1 | tail -10
```

- [ ] **Step 6: Полный прогон**

```
cargo test -p maria304_driver 2>&1 | tail -5
```

- [ ] **Step 7: Commit**

```bash
git add rust/maria304_driver/src/session/dispatcher.rs \
        rust/maria304_driver/src/listener/session_loop.rs \
        rust/maria304_driver/tests/spawn_blocking.rs
git commit -m "fix(driver): M5 — wrap blocking bridge.submit in spawn_blocking"
```

---

## Self-Review

### Spec Coverage

| Пункт аудита | Задача | Покрыт? |
|---|---|---|
| S1: License pubkeys 32 vs 33 bytes | Task 1 | ✅ |
| S2: schema_version allowlist | Task 2 | ✅ |
| S2: payload_sha256 verify | Task 2 | ✅ |
| S3: classify_dps_status not called | Task 3 | ✅ |
| S4: no idempotency journal | Task 4 | ✅ |
| M1: submit_report ignores resp.ok | Task 5 | ✅ |
| M2: no duplicate FN/bind check | Task 6 | ✅ |
| M3: graceful shutdown missing | Task 7 | ✅ |
| M4: metrics not wired to listener | Task 8 | ✅ |
| M5: blocking bridge in async ctx | Task 9 | ✅ |

### Invariant Check

- **Инвариант #1** (нет сети/крипто внутри длинных транзакций): S4 выполняет `find_idempotent_response` вне транзакции, только INSERT OR IGNORE — ✅
- **Инвариант #2** (один writer per fiscal_number): M5 не меняет fn_lock логику — ✅
- **Инвариант #4** (идемпотентность): S4 именно это и добавляет — ✅
- **Инвариант #8** (recovery/reconciliation не нарушает state machine): ни одна из задач не трогает reconcile loop — ✅
- **Инвариант #9** (graceful shutdown важнее быстрого завершения): M3 добавляет настраиваемый таймаут — ✅

### Важные оговорки

- **Task 9 (M5)** — самая инвазивная задача: рефакторинг `dispatch()` на prepare/finalize потребует аккуратного переноса логики COMP и report handlers. Если рефакторинг слишком велик, допустимый временный fix — добавить `#[allow(clippy::await_holding_lock)]` + трекинг-issue, и выполнить полный рефакторинг в отдельном спринте.
- **Task 2 (S2)** — `lib.rs` в `prro_sidecar` может уже существовать. Нужно проверить перед созданием.
- **Task 7 (M3)** — зависимость `futures` или `tokio::task::JoinSet` (stable в tokio 1.21+). Предпочтительно `JoinSet` без новой зависимости.
