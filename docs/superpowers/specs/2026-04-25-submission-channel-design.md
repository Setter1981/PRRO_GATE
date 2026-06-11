# Submission Channel as First-Class Domain Concept — Design

**Date:** 2026-04-25
**Branch:** `feature/rust-security-findings-2026-04-22` (or successor sprint branch)
**Status:** Draft v2 (post-rigorous-review), pending implementation
**Approach:** B (Replace 4-field channel lock with single `submission_channel` lock; add boot-time policy validators)
**Closes:** Pilot blockers #1 (no submission_channel domain) and #2 (channel_owner nullable, silent guard bypass)
**Also requires:** updating CLAUDE.md FI-10 wording to reflect Checkbox-as-ingress-only (see §18)

---

## 1. Goal

Introduce an explicit, persisted, mandatory **submission channel** concept on every fiscal command and active shift, and make channel-switching during an open shift a hard, single-field check at the write_path. Remove the implicit 4-field channel lock and the dead outbound-Checkbox transport.

## 2. Background

The PRRO Gateway today uses a composite channel lock built from `(backend_profile_id, transport_profile_id, protocol, integration_owner)`. There is no domain-level concept of "channel of submission to DPS." This is legally fragile:

- Operators cannot reason about the legal invariant "one shift = one submission channel" from the canonical model.
- `channel_owner` is nullable on `CanonicalFiscalCommand`, `InboxRecord`, and `ingress_inbox`. The shift guard's `_channel_matches` allows `None == None` to silently pass, bypassing the lock.
- The system carries dead outbound-Checkbox infrastructure (`transports/checkbox_rest.py`, `transport_profiles.kind = 'CHECKBOX_REST_TRANSPORT'`, `backend_profiles.backend_type = 'CHECKBOX_CLOUD_COMPAT'`) that cannot exist in production: there is no legal outbound submission channel via Checkbox.

This design closes both blockers identified in the 2026-04-25 pilot-readiness audit.

## 3. Out of scope

- Maria304 D-2 zero-sum guard correction (separate concern, tracked in memory).
- Migration 022 (`sql/022_printer_profiles.sql`) tracking — must be committed in a pre-step before this work, but is not part of this spec.
- `/health/ready` early-true fix (separate blocker #4).
- Sidecar authentication (separate blocker #5).
- Closing acceptance gates G2/G4/G5.

## 4. Domain model

### 4.1 SubmissionChannel enum

New enum in `src/prro_gateway/models/canonical.py`:

```python
class SubmissionChannel(str, Enum):
    DPS_PRRO_FISCAL_SERVER = "DPS_PRRO_FISCAL_SERVER"   # prro.tax.gov.ua direct
    DPS_UNIFIED_WINDOW     = "DPS_UNIFIED_WINDOW"       # cabinet.tax.gov.ua
```

Two values only. **No `CHECKBOX_COMPAT` outbound channel** — Checkbox exists only as an emulated incoming protocol (like Maria), never as a submission destination. This rule is permanent and architectural.

### 4.2 CanonicalFiscalCommand changes

```python
class CanonicalFiscalCommand(StrictModel):
    ...
    submission_channel: SubmissionChannel               # NEW, required, no default
    channel_owner: str = Field(..., min_length=1)       # was Optional[str], now required
```

Pydantic rejects construction without `submission_channel` or with empty `channel_owner`.

### 4.3 ChannelLock model

Currently:
```
backend_profile_id, transport_profile_id, protocol, integration_owner, acquired_at
```

After:
```python
@dataclass(frozen=True)
class ChannelLock:
    submission_channel: SubmissionChannel
    acquired_at: str
```

The four removed fields stay in `ShiftRecord` as **audit-only snapshots** of how the shift was opened. They are written once at `_handle_shift_open` and never compared or updated thereafter. Operators can change `backend_profile_id`/`transport_profile_id`/`channel_owner` mid-shift as long as the resolved `submission_channel` stays the same.

### 4.4 AdapterContext

`src/prro_gateway/adapters/base.py`:

```python
@dataclass(frozen=True)
class AdapterContext:
    fiscal_number: str
    backend_profile_id: str
    transport_profile_id: str
    submission_channel: SubmissionChannel   # NEW
    schema_version: str = "1.0.1"
```

Initialized in `RuntimeContainer.initialize()` from `config.defaults`.

## 5. Configuration

`src/prro_gateway/config.py`:

```python
class DefaultsConfig(BaseModel):
    fiscal_number: str
    backend_profile_id: str
    transport_profile_id: str
    channel_owner: str                       # default "runtime" REMOVED
    submission_channel: SubmissionChannel    # NEW, required
```

`ops/config.example.yaml` updated with explicit `submission_channel: DPS_PRRO_FISCAL_SERVER`.

Loading a config without `submission_channel` fails Pydantic validation at startup with a clear error message — operators must declare the channel for every deployment.

## 6. Boot-time policy validators

New file `src/prro_gateway/runtime/channel_policy.py`:

### 6.1 Endpoint whitelist

```python
CHANNEL_ENDPOINT_WHITELIST: dict[SubmissionChannel, list[str]] = {
    SubmissionChannel.DPS_PRRO_FISCAL_SERVER: ["prro.tax.gov.ua"],
    SubmissionChannel.DPS_UNIFIED_WINDOW:     ["cabinet.tax.gov.ua"],
}
```

`validate_transport_profiles(profiles)` for each active profile:
1. Parses `endpoint` via `urlparse`.
2. Extracts hostname; **lowercases** it; **strips port** (urlparse already separates, but assert no IP literal — reject if `ipaddress.ip_address(host)` succeeds).
3. Asserts `host == w` or `host.endswith("." + w)` for some `w` in the matching whitelist (whitelist values must also be lowercase).

Any mismatch → `ChannelPolicyViolation`. Whitelist domains are best-known values as of 2026-04-25; verify with ops before pilot. Whitelist is a code-side data structure — updating requires code change + redeploy (no schema change), which is intentional (defence against runtime-mutable allowlists).

### 6.2 Binding consistency

`validate_bindings_consistency(bindings, profiles_by_id)` groups all `prro_bindings` by `fiscal_number`. For each group, asserts that all referenced `transport_profiles` declare the same `submission_channel`. A `fiscal_number` cannot be bound to two channels.

### 6.3 Wiring

Both validators run in `RuntimeContainer.initialize()` **before** `health.ready` is set. Any violation raises `ChannelPolicyViolation`, which propagates as a startup failure — the node refuses traffic.

## 7. Database schema migration

New file `sql/025_submission_channel.sql`. Applied by the existing checksum-based migration runner.

### 7.1 Pre-flight asserts

Pre-flight scans **all** rows regardless of `is_active` — historical inactive rows still need to satisfy the new CHECK constraints after rebuild:

```sql
SELECT RAISE(ABORT, 'CHECKBOX_REST_TRANSPORT rows must be removed before migrating')
  FROM transport_profiles WHERE kind = 'CHECKBOX_REST_TRANSPORT' LIMIT 1;
SELECT RAISE(ABORT, 'CHECKBOX_CLOUD_COMPAT rows must be removed before migrating')
  FROM backend_profiles  WHERE backend_type = 'CHECKBOX_CLOUD_COMPAT' LIMIT 1;
SELECT RAISE(ABORT, 'CUSTOM_TRANSPORT rows must be removed before migrating')
  FROM transport_profiles WHERE kind = 'CUSTOM_TRANSPORT' LIMIT 1;
SELECT RAISE(ABORT, 'orphan shifts.opened_via_transport_profile_id detected')
  FROM shifts WHERE opened_via_transport_profile_id NOT IN
       (SELECT transport_profile_id FROM transport_profiles) LIMIT 1;
SELECT RAISE(ABORT, 'shifts in OPENING/CLOSING transient state — stop runtime, complete or revert manually')
  FROM shifts WHERE state IN ('OPENING', 'CLOSING') LIMIT 1;
```

Operators must clean up offending rows manually before this migration runs. The OPENING/CLOSING guard prevents the migration from racing with a transient state-machine transition.

### 7.2 Add submission_channel to transport_profiles

```sql
ALTER TABLE transport_profiles ADD COLUMN submission_channel TEXT;
UPDATE transport_profiles SET submission_channel = CASE kind
    WHEN 'DPS_PRRO_GRPC_ECABINET'      THEN 'DPS_PRRO_FISCAL_SERVER'
    WHEN 'DPS_PRRO_XML_UNIFIED_WINDOW' THEN 'DPS_UNIFIED_WINDOW'
END;
SELECT RAISE(ABORT, 'transport_profiles backfill incomplete')
  FROM transport_profiles WHERE submission_channel IS NULL LIMIT 1;
```

Then rebuild the table to enforce NOT NULL and drop CHECKBOX/CUSTOM from the `kind` CHECK constraint. **Foreign keys must be disabled during rebuild** — `transport_profiles` is referenced by `prro_bindings`, `shifts`, `fiscal_documents`, `ingress_inbox`. Standard SQLite recipe:

```sql
PRAGMA foreign_keys = OFF;
BEGIN;
CREATE TABLE transport_profiles__new (
    transport_profile_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('DPS_PRRO_GRPC_ECABINET','DPS_PRRO_XML_UNIFIED_WINDOW')),
    submission_channel TEXT NOT NULL CHECK (submission_channel IN ('DPS_UNIFIED_WINDOW','DPS_PRRO_FISCAL_SERVER')),
    endpoint TEXT,
    tls_policy TEXT,
    timeout_config_json TEXT,
    retry_policy_json TEXT,
    config_json TEXT,
    is_active INTEGER DEFAULT 1
);
INSERT INTO transport_profiles__new
  SELECT transport_profile_id, kind, submission_channel, endpoint, tls_policy,
         timeout_config_json, retry_policy_json, config_json, is_active
  FROM transport_profiles;
DROP TABLE transport_profiles;
ALTER TABLE transport_profiles__new RENAME TO transport_profiles;
-- recreate any indexes that existed on the old table
COMMIT;
PRAGMA foreign_key_check;   -- runner aborts on any violation
PRAGMA foreign_keys = ON;
```

The migration runner already supports executing PRAGMA statements; verify before merge that the runner does not wrap the entire migration file in an implicit transaction (which would conflict with the explicit BEGIN/COMMIT here). If it does, split the rebuild into a Python step (see §7.6).

### 7.3 backend_profiles cleanup

Same rebuild approach: drop `CHECKBOX_CLOUD_COMPAT` from `backend_type` CHECK. The `submission_channel` column is **not** added to backend_profiles — backend type is orthogonal to where data is submitted.

### 7.4 shifts.opened_via_submission_channel

```sql
ALTER TABLE shifts ADD COLUMN opened_via_submission_channel TEXT;
UPDATE shifts SET opened_via_submission_channel = (
    SELECT submission_channel FROM transport_profiles
    WHERE transport_profile_id = shifts.opened_via_transport_profile_id
);
SELECT RAISE(ABORT, 'shifts backfill incomplete')
  FROM shifts WHERE opened_via_submission_channel IS NULL LIMIT 1;
-- rebuild shifts table to add NOT NULL
```

### 7.5 ingress_inbox.submission_channel (strategy β — universal backfill)

Pure-SQL part — JOIN-based backfill for rows that have `transport_profile_id`:

```sql
ALTER TABLE ingress_inbox ADD COLUMN submission_channel TEXT;
UPDATE ingress_inbox SET submission_channel = (
    SELECT submission_channel FROM transport_profiles
    WHERE transport_profile_id = ingress_inbox.transport_profile_id
)
WHERE transport_profile_id IS NOT NULL;
```

Orphan rows (`transport_profile_id IS NULL`) are handled by a Python post-step (see §7.6). The pure-SQL file does **not** rebuild `ingress_inbox` to NOT NULL — that happens in the Python step after the orphan backfill, ensuring NOT NULL is added only when no NULLs remain.

### 7.6 Python post-step for migration 025

To avoid mutating the migration runner to support SQL parameter binding (which would change behavior for all migrations), migration 025 ships **two artifacts**:

1. `sql/025_submission_channel.sql` — pure-SQL: pre-flight asserts, ALTER ADD COLUMN, JOIN-based UPDATEs, rebuild of `transport_profiles`, `backend_profiles`, `shifts`. NOT NULL is enforced for these three.

2. `src/prro_gateway/migrations/post_025_submission_channel.py` — a Python post-hook discovered and invoked by the migration runner **after** the SQL file applies. It receives `(connection, app_config)` and:
   - Backfills `ingress_inbox.submission_channel` for orphan rows using `app_config.defaults.submission_channel`.
   - Rebuilds `ingress_inbox` to add NOT NULL CHECK on the new column (with the same FK-OFF/COMMIT/FK-CHECK/FK-ON pattern).
   - Refuses to run if `app_config` is None (i.e., CLI invocation without config) — emits a clear error explaining that 025 requires a loaded config.

The runner's discovery convention: any file matching `post_<NNN>_<name>.py` next to the SQL file is invoked after the SQL applies, with checksum included in the same `schema_migrations` row. This is a **new** but **scoped** runner capability — covered by `tests/test_migration_runner.py` extension. No existing migration is affected.

### 7.7 Final integrity assert

After the file applies, the runner runs a smoke check:
```sql
SELECT COUNT(*) FROM transport_profiles WHERE submission_channel IS NULL;
SELECT COUNT(*) FROM shifts WHERE opened_via_submission_channel IS NULL;
SELECT COUNT(*) FROM ingress_inbox WHERE submission_channel IS NULL;
```
All three must be 0. Otherwise → `MigrationIntegrityError`, `health.ready` is not set.

## 8. Adapters and ingress shells

**Correction from review:** `channel_owner` is **per-shell integration identity** (e.g., `"rest-api"`, `"maria304-driver"`), not a per-request operator field extracted from the payload. It is set by the ingress shell when constructing `AdapterContext` via `IngressService.build_context()`, and the adapter passes it through.

### 8.1 Today (verified by reading code)

- `AdapterContext.channel_owner: str | None = None` (allows None — bug)
- `IngressService.build_context()` already requires `channel_owner: str` (no default in signature) — but Pydantic on AdapterContext silently accepts None passed in
- `repositories/inbox.py:24` defaults missing `channel_owner` to `"runtime"` — silent fallback
- `adapters/maria304_native.py:157` builds its own AdapterContext with hardcoded `channel_owner="maria304-driver"`
- All other adapters route through `IngressService.build_context()`

### 8.2 Changes

- `AdapterContext.channel_owner: str = Field(..., min_length=1)` — required, non-empty
- `AdapterContext.submission_channel: SubmissionChannel = Field(...)` — new, required
- `repositories/inbox.py:24` — remove the `"runtime"` default; row insert requires non-null
- `IngressService.build_context()` — add required `submission_channel: SubmissionChannel` parameter; reject if missing
- `adapters/maria304_native.py:154-159` — also add `submission_channel=context_default.submission_channel` (passed in from runtime)

### 8.3 Per-shell ingress wiring

Each ingress shell (`runtime/rest_app.py`, `runtime/xmlrpc_shell.py`, `runtime/maria_shell.py`) reads `submission_channel` and `channel_owner` from `config.defaults` at startup and passes them to `IngressService.build_context()` for every request. **No per-request override** in this sprint — that is a future capability for multi-tenant deployments. Shell-level identity is enough for pilot.

If a future per-request override is needed (e.g., per-cashier `channel_owner`), the shell can read it from a request header / param, validate non-empty, and pass to `build_context`. The adapter contract stays the same.

### 8.4 Rejection on null/empty

The new `Field(..., min_length=1)` constraints make Pydantic reject construction. The shell catches `pydantic.ValidationError` and surfaces a protocol-appropriate error to the caller (REST 400, XML-RPC fault, Maria error frame). Concrete error mapping is implementation-detail — design contract: **never a silent fallback, never a default of `"runtime"`**.

## 9. Write path

### 9.1 Shift guard simplification (`services/write_path.py`)

```python
# Replace the 5-line _resolve_channel_lock_dimensions + _channel_matches block with:
if command.submission_channel != lock.submission_channel:
    return WriteResult.error(
        code=ErrorCode.SHIFT_CHANNEL_SWITCH_FORBIDDEN,
        ctx={
            "expected": lock.submission_channel,
            "got": command.submission_channel,
            "fiscal_number": command.fiscal_number,
            "lock_acquired_at": lock.acquired_at,
        },
    )
```

`_resolve_channel_lock_dimensions` and `_channel_matches` are deleted entirely.

### 9.2 Shift open (`_handle_shift_open`)

Writes 5 fields to the shift row at open time:
- `opened_via_submission_channel` (the new lock dimension)
- `opened_via_backend_profile_id`, `opened_via_transport_profile_id`, `opened_via_protocol`, `opened_via_integration_owner` (audit-only snapshots, frozen for shift lifetime)

### 9.3 Recovery / reconciliation

`services/reconciliation.py` loads active shifts including `opened_via_submission_channel`. If NULL (theoretically impossible after migration but defensive), the shift is marked `REQUIRES_MANUAL_RECONCILIATION` — recovery never guesses a channel. Inbox replay reads `submission_channel` as-is, no recomputation.

### 9.4 Idempotency

`idempotency_key` is unchanged. `submission_channel` is **not** included in the key.

Cases:
- Original call accepted (DONE in inbox), retry with different `submission_channel` → idempotency layer returns the cached result of the first call. The retry's `submission_channel` is irrelevant; the second call never reaches shift-guard or router. This is correct: the operation is logically the same.
- Original call failed at shift-guard (not stored as DONE), retry with different `submission_channel` → not a duplicate; processed as a new command. May succeed if the new channel is consistent with the open shift.
- Original call accepted, retry with different `transport_profile_id` but same `submission_channel` → idempotency returns cached result; transport mismatch never gets a chance to fire.

The deliberate omission of `submission_channel` from the key prevents accidental double-fiscalization across channels via reissue with adjusted metadata.

## 10. Channel/transport mismatch check — two layers

**Correction from review:** `router.route()` is invoked at the `send_or_offline` stage, which is **after** crypto sign. To meet the spec's "before signing" guarantee, the check is split:

### 10.1 Primary: write_path validate/guard stage (before sign)

In `services/write_path.py`, in the `acquire+validate` or `guard` stage (whichever loads the routed transport profile metadata first), add:

```python
profile = self._transport_profiles_repo.get(command.transport_profile_id)
if profile is None:
    return WriteResult.error(code=ErrorCode.UNKNOWN_TRANSPORT_PROFILE, ...)
if profile.submission_channel != command.submission_channel:
    return WriteResult.error(
        code=ErrorCode.SUBMISSION_CHANNEL_TRANSPORT_MISMATCH,
        ctx={"expected": command.submission_channel,
             "got": profile.submission_channel,
             "transport_profile_id": command.transport_profile_id},
    )
```

This runs **before** `sign` — no crypto work wasted, no transport invoked.

### 10.2 Defense-in-depth: router.route() at send stage

`transports/router.py` keeps the same check inside `route()` as a final assertion. If the write_path guard somehow failed to catch (regression, refactor), the router refuses to dispatch the transport. Raises `SubmissionChannelTransportMismatch` exception, which the send stage catches and surfaces as `WriteResult.error(code=SUBMISSION_CHANNEL_TRANSPORT_MISMATCH)`.

Both layers exist intentionally — the design accepts a small duplication for crash-safety. Tests verify both paths fire correctly.

## 11. Outbound Checkbox cleanup

Files deleted:
- `src/prro_gateway/transports/checkbox_rest.py`
- `tests/test_checkbox_transport.py`

Code references removed:
- Registration of `CHECKBOX_REST_TRANSPORT` in `transports/router.py` and `runtime/container.py`
- Any factory/dispatch tables referencing `CHECKBOX_CLOUD_COMPAT` backend type

Documentation updated:
- `docs/Multi-Protocol_PRRO_Gateway.md`
- `docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md`
- `docs/ROADMAP_v3_PILOT.md`
- `CLAUDE.md` and `.claude/CLAUDE.md` — invariant FI-10 wording updated (see §18)

(Historical audit reports under `docs/audits/` are not modified — they are records. Exact diffs for the doc files above are determined during implementation; this spec only mandates that all `CHECKBOX_CLOUD_COMPAT` / `CHECKBOX_REST_TRANSPORT` references in current-state documentation are removed or annotated as historical.)

Final grep verification: `'CHECKBOX_CLOUD_COMPAT|CHECKBOX_REST_TRANSPORT|CHECKBOX_COMPAT'` over `src/`, `sql/`, and `ops/` returns zero hits.

## 12. Error codes

| Code | Where raised | When |
|---|---|---|
| `SHIFT_CHANNEL_SWITCH_FORBIDDEN` | `services/write_path.py` | Existing code, new semantics: `command.submission_channel != active_shift.opened_via_submission_channel` |
| `SUBMISSION_CHANNEL_TRANSPORT_MISMATCH` | `transports/router.py` | New code: command's channel does not match the routed transport_profile's declared channel |
| `CHANNEL_OWNER_REQUIRED` | adapter layer | New code: adapter could not extract `channel_owner` from the request source |
| `SUBMISSION_CHANNEL_REQUIRED` | Pydantic validation on `CanonicalFiscalCommand` | Auto-raised on missing/None `submission_channel` |
| `ChannelPolicyViolation` (exception) | `runtime/channel_policy.py` | Boot-time: endpoint mismatch with whitelist, or fiscal_number bound to multiple channels |

## 13. Tests

### 13.1 Deleted
- `tests/test_checkbox_transport.py`
- Tests asserting "different protocol/backend/transport mid-shift → reject" (semantics changed)

### 13.2 Updated

A grep for `SHIFT_CHANNEL_SWITCH_FORBIDDEN|_channel_matches|ChannelLock|channel_lock` over `tests/` returns **50 files** as of 2026-04-25. Most need only fixture updates (added `submission_channel` keyword); a smaller subset need behavioral rewrites.

- `tests/test_gate1f_channel_lock.py`, `tests/test_gate1u_channel_lock_persistence.py` — primary channel-lock test files: behavioral rewrite from 4-field mismatch to 1-field channel mismatch
- All 48 other matching files — fixture-only updates: `make_command(..., submission_channel=...)`, `AdapterContext(..., submission_channel=...)`
- `tests/conftest.py` — `AdapterContext` and `make_command` fixtures gain `submission_channel`
- `tests/test_pilot_smoke.py` — config gains explicit `submission_channel`
- `tests/test_migration_runner.py` — extended for the new post-step (`post_<NNN>_<name>.py`) discovery convention from §7.6

The full list of 50 files is enumerated in the implementation plan, not this spec.

### 13.3 New
| File | Coverage |
|---|---|
| `tests/test_submission_channel_lock.py` | Open shift on channel A, command on channel B → `SHIFT_CHANNEL_SWITCH_FORBIDDEN` |
| `tests/test_submission_channel_required.py` | Pydantic rejects `CanonicalFiscalCommand` without `submission_channel` |
| `tests/test_channel_owner_required.py` | Each adapter rejects request without `channel_owner` (REST 400, XML-RPC fault, Maria AUTH_REQUIRED) |
| `tests/test_router_channel_mismatch.py` | `command.submission_channel != transport.submission_channel` → `SUBMISSION_CHANNEL_TRANSPORT_MISMATCH` raised before signing |
| `tests/test_boot_endpoint_whitelist.py` | `transport_profile` with non-whitelisted endpoint → boot fails with `ChannelPolicyViolation` |
| `tests/test_boot_binding_consistency.py` | `fiscal_number` bound to two transports with different channels → boot fails |
| `tests/test_migration_025_submission_channel.py` | Migration backfill correctness; CHECKBOX/CUSTOM rows trigger pre-flight RAISE; final NULL count = 0 |
| `tests/test_shift_open_writes_submission_channel.py` | After `SHIFT_OPEN`, `shifts.opened_via_submission_channel` is set correctly |
| `tests/test_shift_audit_fields_immutable.py` | Mid-shift command with different `backend_profile_id` (same channel) succeeds; audit fields unchanged |

## 14. Risks

| Risk | Mitigation |
|---|---|
| Existing prod DB has any (active or inactive) `CHECKBOX_REST_TRANSPORT` / `CHECKBOX_CLOUD_COMPAT` / `CUSTOM_TRANSPORT` rows | Pre-flight RAISE in migration scans all rows regardless of `is_active` (§7.1); operator deletes manually |
| `shifts.opened_via_transport_profile_id` orphan reference (no matching transport_profile) | Pre-flight assert in migration |
| Shift in transient `OPENING` or `CLOSING` state at migration time | Pre-flight assert (§7.1): operator must complete or revert the transition before migrating; runtime should be stopped before `auto_migrate` |
| Untracked `sql/022_printer_profiles.sql` causes checksum mismatch on clean clones | Pre-step (separate commit before this sprint): `git add sql/022_printer_profiles.sql` and its tests |
| Live whitelist hostnames may differ from assumed `prro.tax.gov.ua` / `cabinet.tax.gov.ua` | Whitelist is data-only (in `channel_policy.py`); update before pilot once confirmed; no schema change needed |
| New post-step (`post_<NNN>_<name>.py`) runner convention — first instance | Scoped: only applies if a Python file matching the convention exists alongside the SQL; existing migrations unaffected; covered by `test_migration_runner.py` extension |
| Shift behavior change (mid-shift backend_profile_id swap now allowed) may surprise operators | Documented in `OPERATIONS.md`; ROADMAP changelog entry |
| FK rebuild of `transport_profiles` could cascade-affect dependent rows if FK pragmas not handled | §7.2 includes full PRAGMA foreign_keys=OFF/COMMIT/foreign_key_check/ON pattern; runner verified compatible before merge |
| Two-layer mismatch check (write_path + router) could drift if one is updated and the other forgotten | Both checks share the same comparison logic via a small helper `assert_command_transport_channel_match()` in `runtime/channel_policy.py`; tests verify both paths |

## 15. Rollback

No down-migrations exist in this project. Rollback procedure:

1. Code-level revert via git revert of the sprint commits.
2. Database-level: restore from pre-migration backup (standard project practice; `auto_migrate=true` deployments are expected to back up beforehand).
3. Existing data in tracked production-like environments is small; restore is quick.

In development, `pytest` recreates the schema from scratch each run — no rollback needed.

## 16. Commit sequence

**Realistic policy:** the PR as a whole passes `pytest tests/` after the final commit. Intermediate commits may have temporary failures while the change propagates across layers — Pydantic model changes break existing fixtures until §13.2 fixture-update commit lands. We do **not** use feature flags to keep every commit green; the cost of flag scaffolding outweighs the bisect benefit for a single-PR sprint.

Sequence:

1. **Pre-step (separate PR, before this sprint):** track `sql/022_printer_profiles.sql` and its tests; resolve checksum discrepancy.
2. `feat(models): SubmissionChannel enum + required submission_channel/channel_owner on CanonicalFiscalCommand and AdapterContext`
3. `feat(config): require defaults.submission_channel; remove channel_owner default`
4. `feat(migrations+repositories): 025 SQL + post-step + ChannelLock single-field + repository contract changes` — combined commit (see note below)
5. `feat(write_path): shift guard simplified; channel/transport mismatch guard added pre-sign`
6. `feat(transports): router defense-in-depth check; remove checkbox_rest outbound; cleanup registrations`
7. `feat(runtime): channel_policy module (whitelist + binding consistency); wired into RuntimeContainer.initialize()`
8. `test: submission_channel suite + fixture updates across ~50 channel-touching test files; remove test_checkbox_transport.py`
9. `docs: spec/roadmap/CLAUDE.md updates; remove CHECKBOX_CLOUD_COMPAT mentions; update FI-10 wording`

**Why commits 4 and 5 are separate but tightly coupled:** the migration adds DB columns, the repository contract changes mean `ChannelLock` model loses fields, and `services/write_path.py` reads via the repository. If migration lands in commit N and repository changes in N+1, any restart between them with `auto_migrate=true` leaves the runtime trying to read the old `ChannelLock` shape from the new schema — broken state. To avoid this:
- Commit 4 includes both the SQL/Python migration AND the repository changes. Single atomic schema-and-contract change.
- Commit 5 (write_path) consumes the new contract; can land separately because old contract still works for reads (it just doesn't enforce the new lock semantics).

## 17. Verification before merge

Baseline state at sprint start (2026-04-25): 1430 tests collected, 17 failed (all in `tests/test_admin_ui_printer_profiles.py`, untracked feature), 1413 passed. The 17 failures are unrelated to this sprint and resolved by the §16 pre-step.

- [ ] Pre-step landed: `sql/022_printer_profiles.sql` tracked, all 1430 tests pass.
- [ ] After this sprint: `pytest tests/` green (1430 existing-after-pre-step tests pass; ~9 new test files from §13.3 add coverage).
- [ ] `python scripts/run_rest.py` with old config (no `submission_channel`) → fails with clear Pydantic error message naming the missing field.
- [ ] `python scripts/run_rest.py` with new config → boot succeeds; `/health/ready` only true post-recovery (note: `/health/ready` early-true is a separate blocker not closed by this sprint).
- [ ] Manual scenario: `SHIFT_OPEN` with `submission_channel=DPS_PRRO_FISCAL_SERVER` → `SELL` with `submission_channel=DPS_UNIFIED_WINDOW` → HTTP 422 `SHIFT_CHANNEL_SWITCH_FORBIDDEN`.
- [ ] Manual scenario: `transport_profile` row with `endpoint=https://evil.com/`, `submission_channel=DPS_PRRO_FISCAL_SERVER` → boot fails with `ChannelPolicyViolation` naming the host and the whitelist.
- [ ] Manual scenario: command with `submission_channel=DPS_UNIFIED_WINDOW` and `transport_profile_id` whose profile declares `DPS_PRRO_FISCAL_SERVER` → write_path returns `SUBMISSION_CHANNEL_TRANSPORT_MISMATCH` **before** any signing call.
- [ ] `grep -r 'CHECKBOX_CLOUD_COMPAT\|CHECKBOX_REST_TRANSPORT\|CHECKBOX_COMPAT' src/ sql/ ops/` → zero hits.
- [ ] CLAUDE.md FI-10 updated; spec reviewed and approved by user.

## 18. Frozen invariants — impact statement

| Invariant | Impact | Status |
|---|---|---|
| FI-1 No network/crypto in long SQLite tx | All new checks (shift guard, router mismatch) run **before** `BEGIN IMMEDIATE` | Preserved |
| FI-2 One fiscal_number = one writer | Unchanged | Preserved |
| FI-3 Channel switch forbidden with open shift | **Strengthened** — now legally explicit, single field, no None==None bypass | Strengthened |
| FI-4 Idempotency mandatory | `submission_channel` deliberately not in idempotency key; duplicates with different channel rejected by guard, not silently routed | Preserved |
| FI-5 Offline limits | Unchanged | Preserved |
| FI-6 Adapters build full canonical payloads | `submission_channel` and `channel_owner` now mandatory in canonical; adapters must supply them | Strengthened |
| FI-7 schema_version on envelopes | Unchanged | Preserved |
| FI-8 Recovery preserves state-machine correctness | Recovery reads stored `submission_channel`; null → `REQUIRES_MANUAL_RECONCILIATION` instead of guessing | Preserved |
| FI-9 Graceful shutdown | Unchanged | Preserved |
| FI-10 Checkbox-compatible flow signing bypass only by explicit config | **Invariant text in CLAUDE.md needs replacement.** Outbound Checkbox no longer exists; the original wording assumed a Checkbox cloud destination. New wording (proposed): *"Checkbox is supported only as an emulated ingress protocol; the canonical command from any ingress (including Checkbox-shaped REST) goes through the same write_path with the same crypto rules. There is no signing-bypass path."* This is a docs-only update in commit 9 of §16. | Replaced |

---

**End of design.**
