# W4-Z0 — Per-FN Config Storage + Admin CLI + Bootstrap

**Date:** 2026-05-26
**Status:** Ground-truth spec (locked, operator-confirmed)
**Worklet:** W4-Z0 — precondition for W4-Z1 (xml builder), W4-Z2 (stage_sign expansion), W4-Z3 (DPS smoke), W4 PR-A/B/C (DTO conversion), W4-Y0..Y3 (EVPZ future).

This spec captures decisions made 2026-05-26 in operator design session for the M4 ingress → outgress configuration model.  Reference docs:
- `2026-05-26-w4-z1-dps-xml-wire-shape.md` — wire-shape spec, depends on this
- `feedback_operator_ua_fiscal_authority.md` — operator domain authority memory
- `project_m4_outgress_architecture.md` — outgress trait abstraction pin

---

## 0. Scope summary

W4-Z0 builds the **configuration substrate** for M4 conversion + outgress routing:

1. SQLite schema in `var/secure.db`: `tax_groups`, `payment_methods`, `driver_tax_mapping`, `fn_integration_flags`.
2. Default bootstrap inserts mirroring WebCheck defaults (11 tax groups + 4 payment forms).
3. Admin CLI commands for operator-driven config.
4. Listener-stamped `driver_id` plumbing (no DTO change).
5. Trait abstractions: `OutgressProfile` enum, `DpsXmlBuilder` / `DpsTransport` / `SignEnvelope` traits (pilot ships FSCO impl; EVPZ slot present, no impl).

---

## 1. Storage schema (`var/secure.db` — HIGH-AUDIT-01 isolated DB)

### 1.1 Per-FN tax groups

```sql
CREATE TABLE IF NOT EXISTS tax_groups (
  fn       TEXT    NOT NULL,                  -- fiscal_number
  tx_num   INTEGER NOT NULL CHECK (tx_num BETWEEN 1 AND 99),
  letter   TEXT    NOT NULL,                  -- 'А', 'Б', 'В', 'ГА', ... (operator-readable label)
  dtpr     REAL    NOT NULL DEFAULT 0.0,      -- excise rate %
  txpr     REAL    NOT NULL DEFAULT 0.0,      -- PDV rate %
  txal     INTEGER NOT NULL DEFAULT 0 CHECK (txal BETWEEN 0 AND 3),
  txty     INTEGER NOT NULL DEFAULT 0,        -- tax type, default 0 (standard)
  is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (fn, tx_num)
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_tax_groups_fn_letter
  ON tax_groups (fn, letter)
  WHERE is_active = 1;
```

`letter` carries the operator-readable label (for admin reports + audit), kept aligned with `tx_num` for FSCO emit.  Soft-delete via `is_active=0`; partial unique index prevents resurrecting orphaned active letter mapping.

### 1.2 Per-FN payment methods

```sql
CREATE TABLE IF NOT EXISTS payment_methods (
  fn          TEXT    NOT NULL,
  pay_index   INTEGER NOT NULL CHECK (pay_index BETWEEN 0 AND 99),
  name        TEXT    NOT NULL,               -- 'Готівка' / 'Картка' / 'Кредит' / custom
  iscash      INTEGER NOT NULL DEFAULT 0 CHECK (iscash IN (0, 1)),
  -- XML T attribute value derived as (pay_index - 1) per WebCheck behaviour;
  -- pay_index = 1 → T="0" (Готівка), pay_index = 2 → T="1" (Картка), etc.
  -- See `2026-05-26-w4-z1-dps-xml-wire-shape.md` §M-element semantics.
  is_active   INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (fn, pay_index)
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_payment_methods_fn_name
  ON payment_methods (fn, name)
  WHERE is_active = 1;
```

### 1.3 Per-driver tax mapping (driver number ↔ canonical TX number)

```sql
CREATE TABLE IF NOT EXISTS driver_tax_mapping (
  driver_id        TEXT    NOT NULL,          -- 'maria304' / 'webcheck' / 'eccelio' / etc.
  driver_number    INTEGER NOT NULL,          -- what driver sends in W3 DTO tax_group_1
  driver_letter    TEXT,                      -- optional, driver's letter for audit ("А", "ГА")
  canonical_tx_num INTEGER NOT NULL CHECK (canonical_tx_num BETWEEN 1 AND 99),
  is_active        INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
  created_at       TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (driver_id, driver_number)
) STRICT;
```

Cross-vendor coding-scheme normalization.  Maria → driver_number 4 = canonical_tx_num 4 (1:1 default).  Eccelio → driver_number 5 = canonical_tx_num 4 (configured by operator if Eccelio's scheme differs).

### 1.4 Per-FN integration flags

```sql
CREATE TABLE IF NOT EXISTS fn_integration_flags (
  fn          TEXT    NOT NULL,
  flag_name   TEXT    NOT NULL,               -- 'useecheckmegovua' (національний чек), future others
  flag_value  TEXT    NOT NULL,               -- '1' = on, '0' = off; or string for non-boolean flags
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (fn, flag_name)
) STRICT;
```

### 1.5 Per-FN outgress profile (NEW — architectural slot)

```sql
CREATE TABLE IF NOT EXISTS fn_outgress_profile (
  fn        TEXT    NOT NULL PRIMARY KEY,
  profile   TEXT    NOT NULL CHECK (profile IN ('FSCO_ZZD', 'EVPZ_DPS')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
) STRICT;
```

Pilot all FNs default to `FSCO_ZZD`.  `EVPZ_DPS` slot reserved for post-pilot work (W4-Y series).  Selecting `EVPZ_DPS` for an FN that has no EVPZ impl shipped → typed error at supervisor router.

---

## 2. Default bootstrap (per new FN registration)

When `prro admin add-operator` registers a new FN, gateway must auto-populate:

### 2.1 tax_groups — 11 WebCheck-standard defaults

Per `Directorys.cs:346` in WebCheckMain:

| tx_num | letter | dtpr (excise) | txpr (PDV) | txal | UA semantic |
|--------|--------|---------------|------------|------|-------------|
| 1 | А | 0.0 | 20.0 | 0 | ПДВ 20% standard |
| 2 | Б | 0.0 | 0.0 | 0 | Звільнено від ПДВ |
| 3 | В | 0.0 | 7.0 | 0 | ПДВ 7% (медичні) |
| 4 | ГА | 5.0 | 20.0 | 2 | ПДВ 20% + акциз 5% |
| 5 | ГБ | 5.0 | 0.0 | 2 | Звільнено + акциз 5% |
| 6 | ДА | 7.5 | 20.0 | 2 | ПДВ 20% + акциз 7.5% |
| 7 | ДБ | 7.5 | 0.0 | 2 | Звільнено + акциз 7.5% |
| 8 | Е | 0.0 | 0.0 | 0 | (резерв) |
| 9 | Ж | 0.0 | 0.0 | 0 | (резерв) |
| 10 | З | 0.0 | 0.0 | 0 | (резерв) |
| 11 | К | 0.0 | 14.0 | 0 | ПДВ 14% (спец) |

### 2.2 payment_methods — 4 WebCheck-standard defaults

Per `CreateDB.cs:459`:

| pay_index | name | iscash | XML T= |
|-----------|------|--------|--------|
| 1 | Готівка | 1 | 0 |
| 2 | Картка | 0 | 1 |
| 3 | Кредит | 0 | 2 |
| 4 | Сертифікат | 0 | 3 |

### 2.3 fn_integration_flags — all OFF by default

`useecheckmegovua` (національний чек) = `'0'` until operator explicitly turns on via admin CLI.

### 2.4 fn_outgress_profile — `FSCO_ZZD` default

For pilot all FNs.  Operator sets `EVPZ_DPS` post-pilot when EVPZ impl ships.

### 2.5 driver_tax_mapping — populated per driver vendor at install

For Maria304: 1:1 default mapping (1→1, 2→2, ..., 11→11, 12→12 with appropriate letter labels).  Triggered once-per-driver at driver registration (separate from per-FN bootstrap).

---

## 3. Admin CLI commands (new)

Extending W2 PR-B `prro admin` subcommand surface:

### 3.1 Tax-groups management

```
prro admin add-tax-group   --fn FN --letter А --num 1 --dtpr 0.0 --txpr 20.0 --txal 0
prro admin update-tax-rate --fn FN --num 1 [--dtpr X] [--txpr Y] [--txal Z]
prro admin remove-tax-group --fn FN --num 1            # soft-delete (is_active=0)
prro admin list-tax-groups --fn FN
```

### 3.2 Payment-methods management

```
prro admin add-payment     --fn FN --name "Visa" --cash 0
prro admin update-payment  --fn FN --index 2 [--name X] [--cash 0|1]
prro admin remove-payment  --fn FN --index 2          # soft-delete
prro admin list-payments   --fn FN
```

### 3.3 Integration flags

```
prro admin set-flag        --fn FN --name useecheckmegovua --value 1
prro admin set-national-receipt --fn FN --enabled true     # convenience alias
prro admin list-flags      --fn FN
```

### 3.4 Driver mapping

```
prro admin add-driver-mapping --driver-id maria304 --driver-number 1 --canonical 1 [--letter А]
prro admin remove-driver-mapping --driver-id maria304 --driver-number 1
prro admin list-driver-mappings [--driver-id maria304]
```

### 3.5 Outgress profile

```
prro admin set-outgress-profile --fn FN --profile FSCO_ZZD
prro admin show-outgress-profile --fn FN
```

`EVPZ_DPS` value accepted at admin layer but produces error at runtime supervisor until W4-Y impl ships.

All commands write to `var/secure.db`.  All commands write `audit_log` entries (`tax_group.added`, `payment.removed`, `flag.set`, etc.) per W2 PR-B audit pattern.

---

## 4. Runtime: listener-stamped `driver_id` plumbing

`driver_id` is **NEVER** in W3 wire DTO — it is **runtime context** stamped by the ingress listener after parsing.

### 4.1 Config

`ops/config.yaml`:

```yaml
listeners:
  - type: maria304_tcp
    port: 9099
    driver_id: maria304       # ← listener identity
    fn: "4538765845"          # ← FN this listener serves
  - type: maria304_tcp
    port: 9100
    driver_id: maria304       # same vendor, different FN
    fn: "4538765846"
  - type: webcheck_xmlrpc
    port: 8081
    driver_id: webcheck
    fn: "1234567890"
```

### 4.2 Listener responsibility

Listener parses W3 wire DTO → calls W3 mapping helper with `(wire_dto, listener.driver_id, listener.fn)`:

```rust
let cmd = to_canonical_fiscal_command(
    &wire_dto,
    DriverId::new(&listener.driver_id),
    FiscalNumber::new(&listener.fn),
)?;
```

Listener **validates** `wire_dto.fiscal_number == listener.fn`; mismatch → typed `MappingError::FnConfigMismatch { wire_fn, listener_fn }` + audit_log.

### 4.3 W3 DTO surface — NO change

`CanonicalCommand.fiscal_number: String` field stays as audit data.  No new field needed.  `to_canonical_fiscal_command` signature extended with two new parameters — backward-compatible via default values or method overload.

### 4.4 Internal `CanonicalFiscalCommand` — adds 2 fields

```rust
pub struct CanonicalFiscalCommand {
    // existing W3 fields...
    pub driver_id: DriverId,    // NEW — listener-stamped
    pub fiscal_number: FiscalNumber,  // moved from wire-DTO-only to validated/canonical
    // remainder...
}
```

---

## 5. Trait abstractions for outgress (architectural pin)

Per `project_m4_outgress_architecture` memory.  Built into W4 supervisor (M4 W4) but defined in W4-Z0.

### 5.1 `OutgressProfile` enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutgressProfile {
    FscoZzd,    // pilot impl
    EvpzDps,    // architectural slot; no impl in pilot
}
```

Loaded from `fn_outgress_profile` table per FN at supervisor spawn.

### 5.2 `DpsXmlBuilder` trait

```rust
pub trait DpsXmlBuilder: Send + Sync {
    fn build_check(
        &self,
        cmd: &CanonicalFiscalCommand,
        ctx: &BuilderContext,
    ) -> Result<Vec<u8>, BuildError>;

    fn build_shift_open(&self, ...) -> Result<Vec<u8>, BuildError>;
    fn build_z_report(&self, ...) -> Result<Vec<u8>, BuildError>;
    // ... per doc_type
}
```

`BuilderContext` carries `tax_groups`, `payment_methods`, `driver_tax_mapping`, `integration_flags` looked up from `secure.db` per FN.  Builder consumes context + canonical command → emits bytes.

Pilot ships `FscoXmlBuilder` impl.  `EvpzXmlBuilder` defined as `unimplemented!()` placeholder in W4-Z0 to surface trait usage; replaced with real impl in post-pilot W4-Y1.

### 5.3 `DpsTransport` trait

```rust
#[async_trait]
pub trait DpsTransport: Send + Sync {
    async fn submit(
        &self,
        payload: &[u8],
        target: &TargetEndpoint,
    ) -> Result<DpsResponse, TransportError>;
}
```

Pilot ships `GrpcSendChkV2Transport`.  `HttpsRestTransport` defined as `unimplemented!()` placeholder.

### 5.4 `SignEnvelope` trait

```rust
pub trait SignEnvelope: Send + Sync {
    fn wrap(
        &self,
        content: &[u8],
        sign_ctx: &SignContext,
    ) -> Result<Vec<u8>, SignError>;
}
```

Pilot ships `CmsOverDatEnvelope` (CMS over `<DAT>` content per FSCO).  `CmsOverCheckSignedFileEnvelope` placeholder.

### 5.5 `DpsResponseParser` trait

```rust
pub trait DpsResponseParser: Send + Sync {
    /// Parses a raw DPS response byte payload into a typed canonical
    /// outcome.  Implementation handles outgress-specific shape (FSCO
    /// KVT1/KVT2 split, EVPZ single-ACK, etc.) and normalises to the
    /// `DpsOutcome` enum which the supervisor consumes outgress-
    /// agnostically.
    fn parse_response(
        &self,
        raw: &[u8],
        cmd: &CanonicalFiscalCommand,
    ) -> Result<DpsOutcome, ParseError>;
}

/// Canonical DPS outcome, normalised across outgress variants.
#[derive(Debug, Clone)]
pub enum DpsOutcome {
    /// Receipt accepted; fiscal_number assigned + payload hash echoed.
    /// FSCO: KVT1 with fiscal_number; KVT2 not yet arrived (W12 wiring).
    /// EVPZ: single ACK with assigned fiscal_number.
    Acked {
        fiscal_number: String,
        local_number: u32,
        kvt1_payload_hash: Option<[u8; 32]>,  // FSCO has it, EVPZ may not
    },

    /// FSCO-only: KVT2 confirmation arrived (delayed-final ack).
    /// EVPZ: never emitted; only Acked from EVPZ parser.
    Kvt2Confirmed {
        fiscal_number: String,
        kvt2_payload_hash: [u8; 32],
    },

    /// Receipt rejected by DPS with typed error code.
    Rejected {
        code: String,
        message: Option<String>,
    },

    /// Transport-level failure surfaced as retryable.  Distinct from
    /// `Rejected` (which is DPS-side semantic refusal).
    RetryablePending {
        reason: String,
    },
}
```

Pilot ships `FscoResponseParser` (FSCO KVT1/KVT2 protobuf decode per `sendChkV2` proto3 schema).  `EvpzResponseParser` placeholder.

The parser is the 4th trait in the outgress quartet (builder + sign + transport + parser).  The split exists because response shapes differ structurally per outgress (FSCO has dual-phase KVT1/KVT2; EVPZ may emit single-phase ACK) — handling this in the supervisor body would force outgress-conditional branches; the trait isolates it cleanly.

### 5.6 Router dispatch

Supervisor step after `stage_sign`:

```rust
async fn dispatch_to_outgress(
    fn_id: &str,
    cmd: &CanonicalFiscalCommand,
    runtime: &AppRuntime,
) -> Result<DpsOutcome, OutgressError> {
    let profile = runtime.outgress_profile_for(fn_id).await?;
    let (builder, envelope, transport, parser) = match profile {
        OutgressProfile::FscoZzd => runtime.fsco_zzd_quartet(),
        OutgressProfile::EvpzDps => runtime.evpz_dps_quartet(),  // returns OutgressError::ProfileNotImplemented in pilot
    };
    let payload = builder.build_check(cmd, &runtime.builder_context_for(fn_id))?;
    let signed = envelope.wrap(&payload, &runtime.sign_ctx_for(fn_id))?;
    let raw_response = transport.submit(&signed, &runtime.target_for(fn_id)).await?;
    parser.parse_response(&raw_response, cmd)
        .map_err(OutgressError::ParseFailure)
}
```

The **quartet** (builder + envelope + transport + parser) is plumbed by `runtime.{fsco_zzd,evpz_dps}_quartet()` accessors.  Each accessor returns the per-outgress trait-object set.  Pilot's `evpz_dps_quartet()` returns `OutgressError::ProfileNotImplemented` until W4-Y series lands real impls.

---

## 6. Migration semantics

Schema migrations go in `migrations_secure/` (HIGH-AUDIT-01 secure DB has dedicated migration dir per W2 PR-A).

W4-Z0 migration filename: `migrations_secure/021_w4_z0_config_tables.sql`.

Each table creation is idempotent (`CREATE TABLE IF NOT EXISTS`).  Bootstrap inserts run **only on new FN registration** via admin CLI add-operator — not at migration time.  Migration adds **empty** tables + indices.

---

## 7. Test surface (W4-Z0)

- Repository unit tests for each table (insert/update/list/soft-delete).
- Bootstrap unit test: register new FN → assert 11 tax_groups + 4 payment_methods + flags off + outgress_profile=FSCO_ZZD inserted.
- Admin CLI integration tests (one per command).
- Listener-stamped `driver_id` integration test: mock listener config → call mapping helper → assert `CanonicalFiscalCommand.driver_id` populated correctly.
- FN-config-mismatch test: wire `fn = X`, listener `fn = Y` → typed error.
- Outgress profile router test: `EVPZ_DPS` config → router returns `OutgressError::ProfileNotImplemented`.
- Trait abstraction compile test: assert `FscoXmlBuilder` implements `DpsXmlBuilder`.

---

## 8. Out of scope for W4-Z0

These wait for follow-up worklets (named here for traceability):

- **`xml/mod.rs` builder extension** — W4-Z1.
- **`stage_sign` payload struct extension** — W4-Z2.
- **DPS gRPC live smoke** — W4-Z3.
- **EVPZ XML builder (`xml_evpz/mod.rs`)** — W4-Y1 post-pilot.
- **EVPZ transport** — W4-Y3 post-pilot.
- **EVPZ sign envelope** — W4-Y2 post-pilot.
- **EVPZ response parser** — W4-Y2 post-pilot (alongside sign envelope, since both decode the same response wire shape).

---

## 9. Frozen invariants impact

- **#1 (no network/crypto inside SQLite write tx)**: preserved — all schema operations are short writes; admin CLI does its own short transactions.
- **#4 (idempotency mandatory)**: preserved — admin CLI uses upsert/soft-delete + unique partial indices.
- **#6 (adapters produce full canonical payloads)**: extends — DTO conversion (W4 step 0) becomes a richer mapping that pulls config from tax_groups/payment_methods/driver_tax_mapping per FN.
- **#7 (schema_version on canonical envelopes)**: preserved — `CanonicalFiscalCommand` carries existing schema_version.
- **#10 (local signing bypass only via explicit profile)**: this PIN now has a real meaning — `OutgressProfile::EvpzDps` is "explicit profile" for EVPZ; pilot uses FSCO with local sign.

---

## 10. Sign-off

Spec built from operator design session 2026-05-26 with:
- WebCheck decompile (`docs/webcheck_reverse/WebCheckMain/WebCheck/`) — ground truth for tax_groups / payment_methods / national_receipt
- ФСКО v2.2.3 spec (`docs/dps_protocol/530962.md`) — wire format upstream
- gRPC API spec (`docs/dps_protocol/262576_(1).md`) — transport upstream
- EVPZ docs (`docs/evpz_dps_protokol/`) — architectural pin for post-pilot

Author: Claude (autonomous session 2026-05-26)
Operator: 4-year UA PRRO production experience (50 points / 70 cash registers, retail+HoReCa, WebCheck+1C)
Pilot scope: FSCO/ZZD outgress only; EVPZ traits + slots present, no impl.
