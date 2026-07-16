# CS-1R3 A4 — production SQL-text delta catalog (`f2c17b1` → `f2628ba`)

**Purpose.** The CS-1 closing dossier + the test-provenance artifact FALSELY claimed
the SQL was "byte-identical" and that the `sqlx::query!` macro "strips `: Type` at
compile time". Both are **wrong** (proven below). The CS-1 source-API refactor
**changed the executed SQL statement text** in **24 production read sites across 6
`db/repositories/*` files** — a deliberate, fiscal-neutral source refactor, **not
byte-identical SQL**. This file is the honest, verified catalog of every such site.

**Scope note.** The syn provenance tool (`cs1_test_provenance.rs`) audits the **79
CS-1 test files**, not `src/`. These production `src/` deltas are therefore **out of
that tool's scope** and are catalogued *here* instead. They are disclosed, not
hidden.

---

## §1 · The empirical proof that `query!` sends SQL VERBATIM (macro does NOT strip `: Type`)

The false claim was "the `query!`-MACRO sites are separately SQL-byte-identical (the
macro strips `: Type` at compile time; the alias name is unchanged)". Verified FALSE
two ways:

### (a) sqlx-macros-core 0.8.6 source (the emitter)
- `src/query/input.rs:35-58,93-108` — `source =` concatenates the `LitStr::value` of
  the SQL literal(s) into `QueryMacroInput.sql: String`. **No `as "col: T"` stripping
  happens here** — the string is stored verbatim.
- `src/query/output.rs:176-181` (`query!` / `query_as!`) —
  `let sql = &input.sql; quote! { #sql }` → emitted into
  `::sqlx::__query_with_result::<DB, _>(#sql, …)`. The runtime call receives the SQL
  **verbatim**.
- `src/query/output.rs:224-227` (`query_scalar!`) —
  `let query = &input.sql; … ::sqlx::__query_scalar_with_result::<DB, #ty, _>(#query, …)`.
  Same — verbatim.
- The `: Type` override is parsed from the **column NAME the DESCRIBE step returns**
  (`column_to_rust`), i.e. AFTER SQLite echoed the alias back — which is only possible
  because the alias was sent to SQLite verbatim in the first place.

### (b) The machine-generated `.sqlx` describe cache (stronger than macro expansion)
- `src/query/data.rs:193-197` — the `.sqlx` cache filename is
  `sha256(RAW query string)` (`hash_string`). Verified: every committed cache file's
  name == `sha256(its stored query)`, alias clause included.
- The **active** crate cache `rust/prro/.sqlx/query-06a1c31….json` (HEAD) stores:
  ```
  SELECT offline_fiscal_no,
         offline_fiscal_date,
         offline_session_id as "offline_session_id: DbOfflineSessionId",
         offline_dps_code
  FROM fiscal_documents WHERE document_id = ?
  ```
  and its recorded **describe column names** are
  `[..., 'offline_session_id: DbOfflineSessionId', ...]` — i.e. **SQLite received the
  whole `as "offline_session_id: DbOfflineSessionId"` clause verbatim and used it as
  the output column alias name**. If the macro stripped `: Type`, the column name
  would be `offline_session_id`, not `offline_session_id: DbOfflineSessionId`.
- CS-1 changed this alias `OfflineSessionId` → `DbOfflineSessionId` (base vs head), so
  the executed statement bytes **changed**.

**Conclusion.** For BOTH the compile-time `query!` MACRO and the RUNTIME `query_scalar`
API, the SQL — including `as "col: Type"` — is executed verbatim. The refactor changed
the SQL text. The prior "byte-identical / macro strips `: Type`" claim is retracted.

---

## §2 · What is fiscal-neutral about these deltas (why they are safe)

Two mutually-exclusive delta shapes; **neither changes the fetched VALUE or the stored
bytes**:

- **Class A — macro/runtime alias TYPE renamed `X` → `DbX`** (18 sites). The
  `as "col: X"` decode-annotation string changed (`DocState` → `DbDocState`, …). The
  column **VALUE** is decoded by name/position into the same Rust value; the alias is
  only the output-column *name* of a read. Fiscal result + stored bytes unchanged.
- **Class B — runtime alias REMOVED, type moved to a turbofish** (6 sites).
  `SELECT state as "state: ShiftState"` → `SELECT state` with
  `query_scalar::<_, DbShiftState>` / `query_as::<_, (DbShiftState, String)>`. The
  SELECT clause literally dropped the `as "…"` alias; same fetched value, same stored
  bytes.

**RESULT-VALUE identity is already pinned** by **RP-CS1-5** (`Db*` round-trip:
`rp_cs1_5_db_enum_roundtrip.rs`, `rp_cs1_5_db_id_roundtrip.rs` — decode(value) ==
domain-type value, encode(value) == the same TEXT/BLOB bytes). So the decode of a
`DbDocState`-aliased column yields exactly the `DocState` value the base produced. The
delta is confined to the **statement text**, not the **result** or the **persisted
representation**.

---

## §3 · The catalog — 24 sites, 6 files

Class A = alias TYPE renamed `X`→`DbX` (string changed, verbatim to SQLite).
Class B = alias clause REMOVED, type moved to turbofish (SELECT text changed).

| # | file | fn | class | base → head |
|---|------|----|-------|-------------|
| 1 | audit_log.rs | list_for_entity | A | `severity as "severity: Severity"` → `… DbSeverity` |
| 2 | fiscal_documents.rs | read_offline_stamp_tx | A | `offline_session_id as "…: OfflineSessionId"` → `… DbOfflineSessionId` |
| 3 | fiscal_documents.rs | doc_state_by_request_id_tx | B | `query_scalar("SELECT state …")` (no alias; runtime kind change to `::<_, DbDocState>`) |
| 4 | fiscal_documents.rs | transition_signed_to_offline_local_ack_tx | A | `document_id/state/doc_type/…: X` → `…: DbX` |
| 5 | fiscal_documents.rs | list_pending_for_fn | A | `signed_by_cashier_id as "…: CashierId"` → `… DbCashierId` |
| 6 | fiscal_documents.rs | list_drain_candidates_for_fn_ordered_by_lnd | A | `document_id/state/doc_type/cashier: X` → `…: DbX` |
| 7 | fiscal_documents.rs | get_pending_by_request_id_tx | A | `document_id/state/doc_type/cashier: X` → `…: DbX` |
| 8 | fiscal_documents.rs | peek_pending_doc_id_and_snapshot_id_by_request_id | A | `document_id as "…: DocumentId"` → `… DbDocumentId` |
| 9 | fiscal_documents.rs | get_signing_inputs_tx | A | `state as "state: DocState"` → `… DbDocState` |
| 10 | fiscal_documents.rs | fetch_send_inputs_tx | A | `state/doc_type/shift_id/cashier: X` → `…: DbX` |
| 11 | fiscal_documents.rs | fetch_offline_ack_inputs_tx | A | `state/doc_type: X` → `…: DbX` |
| 12 | fiscal_number_config.rs | get | A | `fiscal_mode as "…: FiscalMode"` → `… DbFiscalMode` |
| 13 | fiscal_number_config.rs | list_all | A | `fiscal_mode as "…: FiscalMode"` → `… DbFiscalMode` |
| 14 | ingress_inbox.rs | insert | A | `protocol as "protocol: Protocol"` → `… DbProtocol` |
| 15 | ingress_inbox.rs | acquire_lease | A | `protocol as "protocol: Protocol"` → `… DbProtocol` |
| 16 | node_state.rs | set_mode_going_online_tx | A | `mode/shift_state/current_shift_id: X` → `…: DbX` |
| 17 | node_state.rs | get | A | `mode/shift_state/current_shift_id: X` → `…: DbX` |
| 18 | shifts.rs | update_cash_balance_tx | A | `shift_id/state/opened_by_cashier_id: X` → `…: DbX` |
| 19 | shifts.rs | active_shift_for_fn | A | `shift_id/state/opened_by_cashier_id: X` → `…: DbX` |
| 20 | shifts.rs | transition_state | B | `query_scalar(r#"SELECT state as "state: ShiftState" …"#)` → `query_scalar::<_, DbShiftState>(r#"SELECT state …"#)` |
| 21 | shifts.rs | force_to_error_with_audit | B | `query_as(r#"SELECT state as "state: ShiftState", fiscal_number …"#)` → `query_as::<_, (DbShiftState, String)>(r#"SELECT state, fiscal_number …"#)` |
| 22 | shifts.rs | force_to_manual_reconciliation_with_audit | B | same shape as #21 |
| 23 | shifts.rs | emit_cas_isolation_violation_audit | B | `query_scalar(r#"SELECT state as "state: ShiftState" …"#)` → `query_scalar::<_, DbShiftState>(r#"SELECT state …"#)` |
| 24 | shifts.rs | senior_cashier_close_shift_with_audit | B | same shape as #21 |

**Totals:** 24 distinct `(file, fn)` sites — **18 Class A** + **6 Class B** — across
**6 files** (`audit_log.rs`, `fiscal_documents.rs`, `fiscal_number_config.rs`,
`ingress_inbox.rs`, `node_state.rs`, `shifts.rs`). (Counting individual `+/-`
annotation/query lines rather than enclosing fns gives the ~43-line figure the audit
cited; both are the same delta set at different granularity.)

---

## §4 · Honest one-line claim (supersedes "byte-identical")

> CS-1 is **fiscal-runtime and persisted-representation compatible** — decode VALUE and
> stored bytes are unchanged (pinned by RP-CS1-5) — but it is **NOT byte-identical
> SQL**: it deliberately changed the executed statement text in 24 production read
> sites (18 alias-type renames `X`→`DbX`, 6 runtime alias removals) across 6
> `db/repositories/*` files, plus 3 catalogued test-side runtime-query deltas
> (`docs/cs1r/pins/runtime_sql_deltas.tsv`). This is a source-API refactor, verified
> fiscal-neutral, not a storage/serde change.

---

## §5 · Reproduce

```
# the production delta set (this catalog)
git -C rust diff f2c17b1 f2628ba -- prro/src/db/repositories/ | grep -nE 'as "|query_scalar::<|query_as::<'

# the macro-verbatim proof (active crate cache; column names carry the : DbType alias)
python3 - <<'PY'
import json
d = json.load(open("rust/prro/.sqlx/query-06a1c3119cee09ea5f097fd68642b641a3cb7c29b082f9fc3d8cbf10026ebc13.json"))
print([c["name"] for c in d["describe"]["columns"]])   # ['…', 'offline_session_id: DbOfflineSessionId', '…']
PY
```
