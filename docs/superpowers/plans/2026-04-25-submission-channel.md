# Submission Channel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the implicit 4-field channel lock with an explicit single-field `SubmissionChannel` lock, make `channel_owner` non-nullable, remove dead outbound-Checkbox transport, and add boot-time policy validators.

**Architecture:** Approach B from the spec: tear out the composite lock and rewrite around `SubmissionChannel ∈ {DPS_PRRO_FISCAL_SERVER, DPS_UNIFIED_WINDOW}`. Channel is required on every `CanonicalFiscalCommand`, frozen at shift open, validated at three points (Pydantic, write_path pre-sign, router defense-in-depth), and policy-checked at boot (endpoint whitelist + per-fiscal_number binding consistency).

**Tech Stack:** Python 3.13, Pydantic v2 (StrictModel), SQLite WAL, FastAPI/uvicorn (REST shell), xmlrpc.server (XML-RPC shell), custom binary TCP (Maria), pytest.

**Spec:** `docs/superpowers/specs/2026-04-25-submission-channel-design.md` (v2)

**Pre-step (NOT part of this plan, must land first):** Commit `sql/022_printer_profiles.sql` + `tests/test_admin_ui_printer_profiles.py` to fix checksum mismatch on clean clones. Without this pre-step, baseline pytest is 17 red.

---

## File Map

**Created:**
- `src/prro_gateway/runtime/channel_policy.py`
- `src/prro_gateway/migrations/post_025_submission_channel.py`
- `sql/025_submission_channel.sql`
- `tests/test_submission_channel_lock.py`
- `tests/test_submission_channel_required.py`
- `tests/test_channel_owner_required.py`
- `tests/test_router_channel_mismatch.py`
- `tests/test_boot_endpoint_whitelist.py`
- `tests/test_boot_binding_consistency.py`
- `tests/test_migration_025_submission_channel.py`
- `tests/test_shift_open_writes_submission_channel.py`
- `tests/test_shift_audit_fields_immutable.py`

**Modified:**
- `src/prro_gateway/models/canonical.py` — add `SubmissionChannel` enum, required field on `CanonicalFiscalCommand`
- `src/prro_gateway/adapters/base.py` — `AdapterContext` non-null `channel_owner` + new `submission_channel`
- `src/prro_gateway/services/ingress.py` — `build_context()` requires `submission_channel`
- `src/prro_gateway/config.py` — `DefaultsConfig.submission_channel` required, drop `channel_owner` default
- `src/prro_gateway/repositories/inbox.py` — drop `"runtime"` fallback
- `src/prro_gateway/repositories/shifts.py` — `ChannelLock` single-field; `get_channel_lock` reads `opened_via_submission_channel`
- `src/prro_gateway/migrations/runner.py` — post-step discovery convention
- `src/prro_gateway/services/write_path.py` — shift guard simplified; pre-sign channel/transport mismatch check; `_handle_shift_open` writes `opened_via_submission_channel`
- `src/prro_gateway/services/reconciliation.py` — load `opened_via_submission_channel` for active shifts
- `src/prro_gateway/transports/router.py` — defense-in-depth mismatch check; remove `CHECKBOX_REST_TRANSPORT` registration
- `src/prro_gateway/runtime/container.py` — invoke `channel_policy` validators in `initialize()`
- `src/prro_gateway/runtime/rest_app.py`, `xmlrpc_shell.py`, `maria_shell.py` — shells pass `submission_channel` to `build_context`
- `src/prro_gateway/adapters/maria304_native.py` — pass `submission_channel` and `channel_owner` from context
- `ops/config.example.yaml` — add `submission_channel: DPS_PRRO_FISCAL_SERVER`
- `tests/conftest.py` — fixtures gain `submission_channel`
- ~48 other test files — fixture-only updates (mass update, see Task 9)
- `CLAUDE.md`, `.claude/CLAUDE.md` — FI-10 wording replaced
- `docs/Multi-Protocol_PRRO_Gateway.md`, `docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md`, `docs/ROADMAP_v3_PILOT.md` — remove outbound-Checkbox references

**Deleted:**
- `src/prro_gateway/transports/checkbox_rest.py`
- `tests/test_checkbox_transport.py` (if exists; verify in Task 6)

---

## Task 1: SubmissionChannel enum + canonical model + AdapterContext

**Goal:** Introduce the `SubmissionChannel` enum and add it as a required field to `CanonicalFiscalCommand` and `AdapterContext`. Also make `AdapterContext.channel_owner` non-nullable. This is the foundation; all other changes depend on it.

**Files:**
- Modify: `src/prro_gateway/models/canonical.py` (add enum, add field to command)
- Modify: `src/prro_gateway/adapters/base.py:23-31` (AdapterContext changes)
- Test: `tests/test_submission_channel_required.py` (new)

**Acceptance Criteria:**
- [ ] `SubmissionChannel` enum exists with exactly two values
- [ ] `CanonicalFiscalCommand(submission_channel=...)` is required; missing → `ValidationError`
- [ ] `CanonicalFiscalCommand(channel_owner=None)` is rejected
- [ ] `AdapterContext(channel_owner=None)` raises `ValidationError`
- [ ] `AdapterContext(submission_channel=...)` is required; missing → `ValidationError`

**Verify:** `pytest tests/test_submission_channel_required.py -v` → all pass

**Steps:**

- [ ] **Step 1: Write failing tests for SubmissionChannel and required fields**

Create `tests/test_submission_channel_required.py`:

```python
from datetime import datetime, timezone
import pytest
from pydantic import ValidationError

from prro_gateway.models.canonical import (
    CanonicalFiscalCommand,
    SubmissionChannel,
)
from prro_gateway.adapters.base import AdapterContext
from prro_gateway.enums import OperationType, Protocol


def test_submission_channel_enum_has_only_two_values():
    assert {c.value for c in SubmissionChannel} == {
        "DPS_PRRO_FISCAL_SERVER",
        "DPS_UNIFIED_WINDOW",
    }


def test_canonical_command_requires_submission_channel():
    with pytest.raises(ValidationError) as exc:
        CanonicalFiscalCommand(
            schema_version="1.0.1",
            request_id="r1",
            idempotency_key="k1",
            protocol=Protocol.CHECKBOX_REST,
            operation_type=OperationType.SELL,
            fiscal_number="4000000001",
            backend_profile_id="bp1",
            transport_profile_id="tp1",
            channel_owner="rest-api",
            business_ts=datetime.now(timezone.utc),
            payload={},
            payload_json="{}",
            payload_sha256="0" * 64,
            requires_signature=True,
            requires_shift=True,
            requires_fiscal_number=True,
            requires_offline_code=False,
        )
    assert "submission_channel" in str(exc.value)


def test_canonical_command_rejects_empty_channel_owner():
    with pytest.raises(ValidationError) as exc:
        CanonicalFiscalCommand(
            schema_version="1.0.1",
            request_id="r1",
            idempotency_key="k1",
            protocol=Protocol.CHECKBOX_REST,
            operation_type=OperationType.SELL,
            fiscal_number="4000000001",
            backend_profile_id="bp1",
            transport_profile_id="tp1",
            channel_owner="",
            submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
            business_ts=datetime.now(timezone.utc),
            payload={},
            payload_json="{}",
            payload_sha256="0" * 64,
            requires_signature=True,
            requires_shift=True,
            requires_fiscal_number=True,
            requires_offline_code=False,
        )
    assert "channel_owner" in str(exc.value)


def test_adapter_context_requires_submission_channel():
    with pytest.raises(ValidationError) as exc:
        AdapterContext(
            request_id="r1",
            fiscal_number="4000000001",
            channel_owner="rest-api",
            business_ts=datetime.now(timezone.utc),
        )
    assert "submission_channel" in str(exc.value)


def test_adapter_context_rejects_none_channel_owner():
    with pytest.raises(ValidationError) as exc:
        AdapterContext(
            request_id="r1",
            fiscal_number="4000000001",
            channel_owner=None,
            submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
            business_ts=datetime.now(timezone.utc),
        )
    assert "channel_owner" in str(exc.value)
```

- [ ] **Step 2: Run tests — expect all fail (imports break or fields missing)**

Run: `pytest tests/test_submission_channel_required.py -v`
Expected: All 5 tests fail with `ImportError: cannot import name 'SubmissionChannel'` or similar.

- [ ] **Step 3: Add `SubmissionChannel` enum to `models/canonical.py`**

In `src/prro_gateway/models/canonical.py`, add at the appropriate location (with other enums, near top of file):

```python
from enum import Enum


class SubmissionChannel(str, Enum):
    """Legal channel of submission to DPS.

    DPS_PRRO_FISCAL_SERVER — direct submission to prro.tax.gov.ua
    DPS_UNIFIED_WINDOW — Єдине вікно подання електронної звітності (cabinet.tax.gov.ua)

    No CHECKBOX_COMPAT outbound channel — Checkbox is ingress-only.
    """
    DPS_PRRO_FISCAL_SERVER = "DPS_PRRO_FISCAL_SERVER"
    DPS_UNIFIED_WINDOW = "DPS_UNIFIED_WINDOW"
```

Then in the `CanonicalFiscalCommand` class definition, add:

```python
class CanonicalFiscalCommand(StrictModel):
    submission_channel: SubmissionChannel = Field(...)
    channel_owner: str = Field(..., min_length=1)
```

(Replace whatever `channel_owner` currently is with `Field(..., min_length=1)`. Verify the field already exists; if it doesn't, add it.)

- [ ] **Step 4: Update `AdapterContext` in `adapters/base.py:23-31`**

Replace:

```python
class AdapterContext(StrictModel):
    request_id: str
    fiscal_number: str
    route_key: str | None = None
    backend_profile_id: str | None = None
    transport_profile_id: str | None = None
    channel_owner: str | None = None
    business_ts: datetime
    trace_context: TraceContext | None = None
```

With:

```python
class AdapterContext(StrictModel):
    request_id: str
    fiscal_number: str
    submission_channel: SubmissionChannel = Field(...)
    channel_owner: str = Field(..., min_length=1)
    route_key: str | None = None
    backend_profile_id: str | None = None
    transport_profile_id: str | None = None
    business_ts: datetime
    trace_context: TraceContext | None = None
```

Add the import at top:

```python
from ..models.canonical import CanonicalFiscalCommand, SubmissionChannel, TraceContext
```

Also update `CanonicalEnvelopeBuilder.build()` to forward `submission_channel`:

```python
return CanonicalFiscalCommand(
    submission_channel=context.submission_channel,
    channel_owner=context.channel_owner,
)
```

- [ ] **Step 5: Run tests — expect 5 pass**

Run: `pytest tests/test_submission_channel_required.py -v`
Expected: All 5 pass.

- [ ] **Step 6: Run full pytest — expect many failures elsewhere (intentional — handled by later tasks)**

Run: `pytest tests/ --tb=no -q 2>&1 | tail -10`
Expected: Many failures across `tests/` because existing code constructs `CanonicalFiscalCommand` and `AdapterContext` without the new required fields. Note this baseline; downstream tasks fix it.

- [ ] **Step 7: Commit**

```bash
git add src/prro_gateway/models/canonical.py \
        src/prro_gateway/adapters/base.py \
        tests/test_submission_channel_required.py
git commit -m "feat(models): SubmissionChannel enum + required submission_channel/channel_owner

- Add SubmissionChannel enum with two values (DPS_PRRO_FISCAL_SERVER, DPS_UNIFIED_WINDOW)
- Make submission_channel required on CanonicalFiscalCommand and AdapterContext
- Make channel_owner non-nullable with min_length=1 on both
- Add regression tests for Pydantic rejection of missing/empty values

Closes pilot blocker #1 (no submission_channel domain) at the model layer.
Other layers (config, adapters, write_path, migrations) follow in subsequent commits.
Downstream test failures are expected until Task 9 fixture-update pass."
```

---

## Task 2: Configuration changes

**Goal:** Make `defaults.submission_channel` required in config schema; drop the `"runtime"` default for `channel_owner`. Update `ops/config.example.yaml`.

**Files:**
- Modify: `src/prro_gateway/config.py` (`DefaultsConfig`)
- Modify: `ops/config.example.yaml`
- Test: `tests/test_config.py` (extend existing)

**Acceptance Criteria:**
- [ ] `DefaultsConfig.submission_channel: SubmissionChannel` required, no default
- [ ] `DefaultsConfig.channel_owner: str` required (no `"runtime"` default)
- [ ] Loading a YAML without `defaults.submission_channel` → `ValidationError` naming the field
- [ ] `ops/config.example.yaml` contains `submission_channel: DPS_PRRO_FISCAL_SERVER`

**Verify:** `pytest tests/test_config.py -v` → all pass

**Steps:**

- [ ] **Step 1: Read current `DefaultsConfig` and existing config test**

Read: `src/prro_gateway/config.py` — find the `DefaultsConfig` class. Read `tests/test_config.py` — understand existing test patterns.

- [ ] **Step 2: Write failing tests**

Append to `tests/test_config.py`:

```python
import pytest
from pydantic import ValidationError

from prro_gateway.config import DefaultsConfig
from prro_gateway.models.canonical import SubmissionChannel


def test_defaults_config_requires_submission_channel():
    with pytest.raises(ValidationError) as exc:
        DefaultsConfig(
            fiscal_number="4000000001",
            backend_profile_id="bp1",
            transport_profile_id="tp1",
            channel_owner="rest-api",
        )
    assert "submission_channel" in str(exc.value)


def test_defaults_config_requires_channel_owner_no_default():
    with pytest.raises(ValidationError) as exc:
        DefaultsConfig(
            fiscal_number="4000000001",
            backend_profile_id="bp1",
            transport_profile_id="tp1",
            submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
        )
    assert "channel_owner" in str(exc.value)


def test_defaults_config_accepts_valid_submission_channel():
    cfg = DefaultsConfig(
        fiscal_number="4000000001",
        backend_profile_id="bp1",
        transport_profile_id="tp1",
        channel_owner="rest-api",
        submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
    )
    assert cfg.submission_channel == SubmissionChannel.DPS_PRRO_FISCAL_SERVER
    assert cfg.channel_owner == "rest-api"
```

- [ ] **Step 3: Run tests — expect 3 fail**

Run: `pytest tests/test_config.py -v -k "submission_channel or channel_owner_no_default"`
Expected: All 3 new tests fail.

- [ ] **Step 4: Update `DefaultsConfig` in `config.py`**

In `src/prro_gateway/config.py`, locate `DefaultsConfig` and update:

```python
from prro_gateway.models.canonical import SubmissionChannel


class DefaultsConfig(BaseModel):
    fiscal_number: str
    backend_profile_id: str
    transport_profile_id: str
    channel_owner: str = Field(..., min_length=1)
    submission_channel: SubmissionChannel = Field(...)
```

If `channel_owner` previously had `= "runtime"`, remove that. If it was already required, just verify.

- [ ] **Step 5: Run tests — expect 3 pass**

Run: `pytest tests/test_config.py -v -k "submission_channel or channel_owner_no_default"`
Expected: All 3 pass.

- [ ] **Step 6: Update `ops/config.example.yaml`**

Read current file. Find the `defaults:` block. Add:

```yaml
defaults:
  fiscal_number: "4000000001"
  backend_profile_id: "default-backend"
  transport_profile_id: "default-transport"
  channel_owner: "rest-api"
  submission_channel: DPS_PRRO_FISCAL_SERVER
```

(Use the existing field values; only add `submission_channel` line.)

- [ ] **Step 7: Verify boot fails clearly with old config**

Manual: temporarily comment out `submission_channel` in `ops/config.example.yaml`, run:
```bash
PRRO_GATEWAY_CONFIG=./ops/config.example.yaml python -c "from prro_gateway.config import AppConfig; AppConfig.from_env()"
```
Expected: `ValidationError` mentioning `submission_channel`. Then uncomment.

- [ ] **Step 8: Commit**

```bash
git add src/prro_gateway/config.py tests/test_config.py ops/config.example.yaml
git commit -m "feat(config): require defaults.submission_channel; remove channel_owner default"
```

---

## Task 3: Migration runner post-step convention

**Goal:** Extend the migration runner to discover and execute `post_<NNN>_<name>.py` Python files alongside SQL migrations. The Python file is loaded via `runpy.run_path()` (no eval/compile of arbitrary strings) and its `run(connection, app_config)` function is invoked after the SQL file applies. Combined checksum is recorded.

**Files:**
- Modify: `src/prro_gateway/migrations/runner.py`
- Test: `tests/test_migration_runner.py` (extend)

**Acceptance Criteria:**
- [ ] Runner discovers `post_<NNN>_<name>.py` files in the SQL directory
- [ ] Post-step is loaded via `runpy.run_path()` and its `run(conn, app_config)` is called after SQL applies
- [ ] If post-step exists but `app_config` is None → migration aborts with clear message
- [ ] Combined checksum (sql + py) is recorded in the same `schema_migrations` row
- [ ] Migration without a post-step still works unchanged

**Verify:** `pytest tests/test_migration_runner.py -v` → all pass

**Steps:**

- [ ] **Step 1: Read `migrations/runner.py` to understand existing structure**

Read: `src/prro_gateway/migrations/runner.py` end-to-end. Identify:
- Where SQL files are discovered
- How checksum is computed
- How `schema_migrations` row is inserted
- Function signatures used

- [ ] **Step 2: Write failing tests**

Append to `tests/test_migration_runner.py`:

```python
import hashlib
import sqlite3
import textwrap
from pathlib import Path

import pytest

from prro_gateway.migrations.runner import apply_pending_migrations


def test_post_step_runs_after_sql(tmp_path: Path):
    sql_dir = tmp_path / "sql"
    sql_dir.mkdir()
    (sql_dir / "099_test_post.sql").write_text(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, val TEXT);\n"
        "INSERT INTO t VALUES (1, 'sql');"
    )
    (sql_dir / "post_099_test_post.py").write_text(textwrap.dedent("""
        def run(conn, app_config):
            conn.execute("INSERT INTO t VALUES (2, ?)", (app_config["marker"],))
    """))

    db_path = tmp_path / "test.db"
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    apply_pending_migrations(conn, sql_dir, app_config={"marker": "from-post"})

    rows = conn.execute("SELECT id, val FROM t ORDER BY id").fetchall()
    assert [(r["id"], r["val"]) for r in rows] == [(1, "sql"), (2, "from-post")]


def test_post_step_aborts_when_app_config_none(tmp_path: Path):
    sql_dir = tmp_path / "sql"
    sql_dir.mkdir()
    (sql_dir / "099_test_post.sql").write_text("CREATE TABLE t (id INTEGER);")
    (sql_dir / "post_099_test_post.py").write_text("def run(conn, app_config): pass")

    db_path = tmp_path / "test.db"
    conn = sqlite3.connect(db_path)
    with pytest.raises(RuntimeError, match="post-step.*requires.*app_config"):
        apply_pending_migrations(conn, sql_dir, app_config=None)


def test_migration_without_post_step_works_unchanged(tmp_path: Path):
    sql_dir = tmp_path / "sql"
    sql_dir.mkdir()
    (sql_dir / "099_plain.sql").write_text("CREATE TABLE t (id INTEGER);")

    db_path = tmp_path / "test.db"
    conn = sqlite3.connect(db_path)
    apply_pending_migrations(conn, sql_dir, app_config=None)
    conn.execute("SELECT id FROM t")  # no error → table exists


def test_post_step_checksum_recorded(tmp_path: Path):
    sql_dir = tmp_path / "sql"
    sql_dir.mkdir()
    sql_text = "CREATE TABLE t (id INTEGER);"
    py_text = "def run(conn, app_config): pass"
    (sql_dir / "099_with_post.sql").write_text(sql_text)
    (sql_dir / "post_099_with_post.py").write_text(py_text)

    db_path = tmp_path / "test.db"
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    apply_pending_migrations(conn, sql_dir, app_config={})

    row = conn.execute(
        "SELECT name, checksum FROM schema_migrations WHERE name = ?",
        ("099_with_post.sql",),
    ).fetchone()
    expected_checksum = hashlib.sha256((sql_text + "\n---POST---\n" + py_text).encode()).hexdigest()
    assert row["checksum"] == expected_checksum
```

- [ ] **Step 3: Run tests — expect 4 fail**

Run: `pytest tests/test_migration_runner.py -k "post_step or without_post_step" -v`
Expected: All 4 fail.

- [ ] **Step 4: Implement post-step discovery in runner**

In `src/prro_gateway/migrations/runner.py`, modify `apply_pending_migrations`:

1. Add `app_config` parameter to the function signature: `def apply_pending_migrations(conn, sql_dir, app_config=None):`
2. After reading each SQL file, check for `post_<basename_without_sql>.py` in the same directory.
3. If post-step exists:
   - Read its content
   - Combined checksum: `sha256(sql_content + "\n---POST---\n" + py_content)`
   - Apply SQL first
   - Then load the file via `runpy.run_path()` and call its `run(conn, app_config)`
   - If `app_config is None` → raise `RuntimeError("post-step '<name>' requires app_config; pass --config to CLI or run via runtime")`
4. If no post-step: existing checksum (sql_content only) and no Python invocation.

Concrete sketch:

```python
import hashlib
import runpy
import sqlite3
from pathlib import Path


def _compute_checksum(sql_content: str, post_content: str | None) -> str:
    if post_content is None:
        return hashlib.sha256(sql_content.encode()).hexdigest()
    return hashlib.sha256(
        (sql_content + "\n---POST---\n" + post_content).encode()
    ).hexdigest()


def _invoke_post_step(post_path: Path, conn, app_config) -> None:
    namespace = runpy.run_path(str(post_path))
    run_fn = namespace.get("run")
    if run_fn is None:
        raise RuntimeError(f"post-step '{post_path.name}' missing run() function")
    run_fn(conn, app_config)


def apply_pending_migrations(
    conn: sqlite3.Connection,
    sql_dir: Path,
    app_config: object | None = None,
) -> None:
    _ensure_schema_migrations_table(conn)
    applied = _load_applied_names(conn)

    for sql_file in sorted(sql_dir.glob("[0-9][0-9][0-9]_*.sql")):
        name = sql_file.name
        if name in applied:
            _verify_checksum(conn, sql_file, name)
            continue

        sql_content = sql_file.read_text()
        post_path = sql_file.with_name(f"post_{sql_file.stem}.py")
        post_content = post_path.read_text() if post_path.exists() else None

        if post_content is not None and app_config is None:
            raise RuntimeError(
                f"post-step '{post_path.name}' requires app_config; "
                f"pass --config to CLI or run via runtime"
            )

        checksum = _compute_checksum(sql_content, post_content)

        conn.executescript(sql_content)

        if post_content is not None:
            _invoke_post_step(post_path, conn, app_config)

        conn.execute(
            "INSERT INTO schema_migrations (name, checksum, applied_at) VALUES (?, ?, datetime('now'))",
            (name, checksum),
        )
        conn.commit()
```

(Adapt to actual existing function structure — preserve other behavior like checksum verification on already-applied rows.)

- [ ] **Step 5: Update existing migration runner CLI entry**

Find the CLI entry (e.g., `python -m prro_gateway.migrations`). Add a `--config` flag that loads `AppConfig` and passes it to `apply_pending_migrations`. If migrations needing post-steps are pending and no config provided → existing error message surfaces.

- [ ] **Step 6: Run tests — expect 4 pass**

Run: `pytest tests/test_migration_runner.py -v`
Expected: All tests pass (existing + 4 new).

- [ ] **Step 7: Commit**

```bash
git add src/prro_gateway/migrations/runner.py tests/test_migration_runner.py
git commit -m "feat(migrations): post-step convention for runner"
```

---

## Task 4: Migration 025 + ChannelLock + shifts repository

**Goal:** Land the schema migration (with FK-aware rebuild + post-step), update `ChannelLock` model to single field, and rewrite `get_channel_lock` and `open_shift` in the shifts repository to use `opened_via_submission_channel`. This is one atomic commit because schema and repository contract must change together.

**Files:**
- Create: `sql/025_submission_channel.sql`
- Create: `src/prro_gateway/migrations/post_025_submission_channel.py`
- Modify: `src/prro_gateway/models/storage.py` (`ChannelLock` model + `ShiftRecord` add `opened_via_submission_channel`)
- Modify: `src/prro_gateway/repositories/shifts.py` (`get_channel_lock`, `open_shift`)
- Modify: `src/prro_gateway/repositories/inbox.py:24` (drop `"runtime"` default)
- Test: `tests/test_migration_025_submission_channel.py` (new)
- Test: `tests/test_repository.py` (extend)

**Acceptance Criteria:**
- [ ] `sql/025_submission_channel.sql` applies cleanly to a fresh schema (after migrations 001-024)
- [ ] All 5 pre-flight asserts fire on offending data
- [ ] After migration: `transport_profiles.submission_channel` is NOT NULL with CHECK
- [ ] After migration: `backend_profiles.backend_type` CHECK no longer permits `CHECKBOX_CLOUD_COMPAT`
- [ ] After migration: `shifts.opened_via_submission_channel` is NOT NULL
- [ ] After migration: `ingress_inbox.submission_channel` is NOT NULL
- [ ] `ChannelLock` model has only `submission_channel` and `acquired_at`
- [ ] `get_channel_lock(conn, fn)` reads `opened_via_submission_channel` for state='OPENED' shift
- [ ] `open_shift(...)` writes `opened_via_submission_channel`
- [ ] `inbox.py` no longer has `"runtime"` fallback for channel_owner

**Verify:** `pytest tests/test_migration_025_submission_channel.py tests/test_repository.py -v`

**Steps:**

- [ ] **Step 1: Verify migration 022 is committed (pre-step) and current schema state**

```bash
git ls-files sql/ | grep 022
ls sql/
```
Expected: `sql/022_printer_profiles.sql` is tracked. Migrations go up to `024_sending_state.sql`.

If 022 is still untracked: STOP, do the pre-step first.

- [ ] **Step 2: Read current `transport_profiles`, `backend_profiles`, `shifts`, `ingress_inbox` schemas**

Read: `sql/001_hot_store_init.sql` lines 53-76 (transport_profiles), lines 144-146 (shifts), and the `ingress_inbox` and `backend_profiles` definitions. Note exact column types and any existing indexes for the rebuild step. The implementation MUST reproduce all columns exactly.

- [ ] **Step 3: Write failing migration tests**

Create `tests/test_migration_025_submission_channel.py`:

```python
import sqlite3
from pathlib import Path
from types import SimpleNamespace

import pytest

from prro_gateway.migrations.runner import apply_pending_migrations


SQL_DIR = Path("sql")


def _fresh_db(tmp_path: Path) -> sqlite3.Connection:
    db = sqlite3.connect(tmp_path / "test.db")
    db.row_factory = sqlite3.Row
    return db


def _apply_through_024(conn):
    files_through_024 = sorted([
        f for f in SQL_DIR.glob("[0-9][0-9][0-9]_*.sql")
        if int(f.name[:3]) <= 24
    ])
    for f in files_through_024:
        conn.executescript(f.read_text())


def _make_app_config(channel="DPS_PRRO_FISCAL_SERVER"):
    return SimpleNamespace(defaults=SimpleNamespace(submission_channel=channel))


def test_migration_025_succeeds_on_clean_db(tmp_path):
    conn = _fresh_db(tmp_path)
    _apply_through_024(conn)
    conn.execute(
        "INSERT INTO transport_profiles (transport_profile_id, kind, endpoint, is_active) "
        "VALUES (?, ?, ?, 1)",
        ("tp-prro-1", "DPS_PRRO_GRPC_ECABINET", "https://prro.tax.gov.ua/api"),
    )
    conn.commit()

    apply_pending_migrations(conn, SQL_DIR, app_config=_make_app_config())

    row = conn.execute(
        "SELECT submission_channel FROM transport_profiles WHERE transport_profile_id = ?",
        ("tp-prro-1",),
    ).fetchone()
    assert row["submission_channel"] == "DPS_PRRO_FISCAL_SERVER"


def test_migration_025_rejects_checkbox_rest_transport(tmp_path):
    conn = _fresh_db(tmp_path)
    _apply_through_024(conn)
    conn.execute(
        "INSERT INTO transport_profiles (transport_profile_id, kind, endpoint, is_active) "
        "VALUES (?, ?, ?, ?)",
        ("tp-cb", "CHECKBOX_REST_TRANSPORT", "https://checkbox.in.ua/api", 1),
    )
    conn.commit()

    with pytest.raises(sqlite3.OperationalError, match="CHECKBOX_REST_TRANSPORT"):
        apply_pending_migrations(conn, SQL_DIR, app_config=_make_app_config())


def test_migration_025_rejects_checkbox_cloud_compat_backend(tmp_path):
    conn = _fresh_db(tmp_path)
    _apply_through_024(conn)
    conn.execute(
        "INSERT INTO backend_profiles (backend_profile_id, backend_type, is_active) "
        "VALUES (?, ?, ?)",
        ("bp-cb", "CHECKBOX_CLOUD_COMPAT", 1),
    )
    conn.commit()

    with pytest.raises(sqlite3.OperationalError, match="CHECKBOX_CLOUD_COMPAT"):
        apply_pending_migrations(conn, SQL_DIR, app_config=_make_app_config())


def test_migration_025_rejects_inactive_checkbox_rows(tmp_path):
    conn = _fresh_db(tmp_path)
    _apply_through_024(conn)
    conn.execute(
        "INSERT INTO transport_profiles (transport_profile_id, kind, endpoint, is_active) "
        "VALUES (?, ?, ?, 0)",
        ("tp-cb-old", "CHECKBOX_REST_TRANSPORT", "https://old.example", 0),
    )
    conn.commit()

    with pytest.raises(sqlite3.OperationalError, match="CHECKBOX_REST_TRANSPORT"):
        apply_pending_migrations(conn, SQL_DIR, app_config=_make_app_config())


def test_migration_025_rejects_opening_shift(tmp_path):
    conn = _fresh_db(tmp_path)
    _apply_through_024(conn)
    conn.execute(
        "INSERT INTO transport_profiles (transport_profile_id, kind, endpoint, is_active) "
        "VALUES (?, ?, ?, 1)",
        ("tp-prro-1", "DPS_PRRO_GRPC_ECABINET", "https://prro.tax.gov.ua/api"),
    )
    conn.execute(
        "INSERT INTO shifts (fiscal_number, shift_id, state, opened_via_backend_profile_id, "
        "opened_via_transport_profile_id, opened_via_protocol, opened_via_integration_owner, "
        "channel_lock_acquired_at) "
        "VALUES (?, ?, 'OPENING', ?, ?, ?, ?, ?)",
        ("4000000001", "sh-1", "bp-1", "tp-prro-1", "MARIA_TCP", "rest-api", "2026-04-25T00:00:00Z"),
    )
    conn.commit()

    with pytest.raises(sqlite3.OperationalError, match="OPENING/CLOSING"):
        apply_pending_migrations(conn, SQL_DIR, app_config=_make_app_config())


def test_migration_025_orphan_inbox_backfilled_from_config(tmp_path):
    conn = _fresh_db(tmp_path)
    _apply_through_024(conn)
    conn.execute(
        "INSERT INTO transport_profiles (transport_profile_id, kind, endpoint, is_active) "
        "VALUES (?, ?, ?, 1)",
        ("tp-prro-1", "DPS_PRRO_GRPC_ECABINET", "https://prro.tax.gov.ua/api"),
    )
    conn.execute(
        "INSERT INTO ingress_inbox (request_id, idempotency_key, protocol, operation_type, "
        "fiscal_number, status, transport_profile_id, channel_owner, accepted_at) "
        "VALUES (?, ?, 'CHECKBOX_REST', 'SELL', ?, 'DONE', NULL, 'rest-api', '2026-04-25T00:00:00Z')",
        ("r-orphan", "k-orphan", "4000000001"),
    )
    conn.commit()

    apply_pending_migrations(
        conn, SQL_DIR, app_config=_make_app_config(channel="DPS_UNIFIED_WINDOW")
    )

    row = conn.execute(
        "SELECT submission_channel FROM ingress_inbox WHERE request_id = ?",
        ("r-orphan",),
    ).fetchone()
    assert row["submission_channel"] == "DPS_UNIFIED_WINDOW"


def test_migration_025_post_step_aborts_without_config(tmp_path):
    conn = _fresh_db(tmp_path)
    _apply_through_024(conn)
    conn.execute(
        "INSERT INTO transport_profiles (transport_profile_id, kind, endpoint, is_active) "
        "VALUES (?, ?, ?, 1)",
        ("tp-prro-1", "DPS_PRRO_GRPC_ECABINET", "https://prro.tax.gov.ua/api"),
    )
    conn.commit()

    with pytest.raises(RuntimeError, match="post-step.*requires.*app_config"):
        apply_pending_migrations(conn, SQL_DIR, app_config=None)
```

- [ ] **Step 4: Run tests — expect all fail (migration doesn't exist)**

Run: `pytest tests/test_migration_025_submission_channel.py -v`
Expected: 7 fail with `FileNotFoundError` or migration-not-applied errors.

- [ ] **Step 5: Create `sql/025_submission_channel.sql`**

Create the file with all sections from spec §7. The implementation MUST read `sql/001_hot_store_init.sql` first to get the EXACT column list for `shifts`, `transport_profiles`, `backend_profiles`, and `ingress_inbox`, then reproduce ALL columns in the rebuild blocks. Concrete content (column lists below are placeholders — replace with real schema):

```sql
-- Migration 025: submission_channel as first-class domain concept
-- See: docs/superpowers/specs/2026-04-25-submission-channel-design.md

-- ===== 7.1 Pre-flight asserts =====
SELECT RAISE(ABORT, 'CHECKBOX_REST_TRANSPORT rows must be removed before migrating')
  FROM transport_profiles WHERE kind = 'CHECKBOX_REST_TRANSPORT' LIMIT 1;
SELECT RAISE(ABORT, 'CHECKBOX_CLOUD_COMPAT rows must be removed before migrating')
  FROM backend_profiles WHERE backend_type = 'CHECKBOX_CLOUD_COMPAT' LIMIT 1;
SELECT RAISE(ABORT, 'CUSTOM_TRANSPORT rows must be removed before migrating')
  FROM transport_profiles WHERE kind = 'CUSTOM_TRANSPORT' LIMIT 1;
SELECT RAISE(ABORT, 'orphan shifts.opened_via_transport_profile_id detected')
  FROM shifts WHERE opened_via_transport_profile_id IS NOT NULL
    AND opened_via_transport_profile_id NOT IN
        (SELECT transport_profile_id FROM transport_profiles) LIMIT 1;
SELECT RAISE(ABORT, 'shifts in OPENING/CLOSING transient state — stop runtime, complete or revert manually')
  FROM shifts WHERE state IN ('OPENING', 'CLOSING') LIMIT 1;

-- ===== 7.2 transport_profiles: add column + backfill + rebuild =====
ALTER TABLE transport_profiles ADD COLUMN submission_channel TEXT;
UPDATE transport_profiles SET submission_channel = CASE kind
    WHEN 'DPS_PRRO_GRPC_ECABINET'      THEN 'DPS_PRRO_FISCAL_SERVER'
    WHEN 'DPS_PRRO_XML_UNIFIED_WINDOW' THEN 'DPS_UNIFIED_WINDOW'
END;
SELECT RAISE(ABORT, 'transport_profiles backfill incomplete')
  FROM transport_profiles WHERE submission_channel IS NULL LIMIT 1;

PRAGMA foreign_keys = OFF;
CREATE TABLE transport_profiles__new (
    transport_profile_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('DPS_PRRO_GRPC_ECABINET','DPS_PRRO_XML_UNIFIED_WINDOW')),
    submission_channel TEXT NOT NULL CHECK (submission_channel IN ('DPS_PRRO_FISCAL_SERVER','DPS_UNIFIED_WINDOW')),
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
PRAGMA foreign_keys = ON;

-- ===== 7.3 backend_profiles: rebuild without CHECKBOX_CLOUD_COMPAT =====
PRAGMA foreign_keys = OFF;
CREATE TABLE backend_profiles__new (
    backend_profile_id TEXT PRIMARY KEY,
    backend_type TEXT NOT NULL CHECK (backend_type IN ('LOCAL_PRRO_CORE','DPS_DIRECT','CUSTOM_VENDOR_BACKEND')),
    capability_flags_json TEXT,
    config_json TEXT,
    is_active INTEGER DEFAULT 1
);
INSERT INTO backend_profiles__new SELECT backend_profile_id, backend_type, capability_flags_json, config_json, is_active FROM backend_profiles;
DROP TABLE backend_profiles;
ALTER TABLE backend_profiles__new RENAME TO backend_profiles;
PRAGMA foreign_keys = ON;

-- ===== 7.4 shifts: add submission_channel + JOIN backfill + rebuild =====
ALTER TABLE shifts ADD COLUMN opened_via_submission_channel TEXT;
UPDATE shifts SET opened_via_submission_channel = (
    SELECT submission_channel FROM transport_profiles
    WHERE transport_profile_id = shifts.opened_via_transport_profile_id
);
SELECT RAISE(ABORT, 'shifts backfill incomplete')
  FROM shifts WHERE opened_via_submission_channel IS NULL LIMIT 1;
PRAGMA foreign_keys = OFF;
-- IMPORTANT: implementation must read sql/001_hot_store_init.sql shifts table and
-- reproduce EVERY column below; the placeholder list is incomplete.
CREATE TABLE shifts__new (
    fiscal_number TEXT NOT NULL,
    shift_id TEXT PRIMARY KEY,
    state TEXT NOT NULL,
    opened_via_backend_profile_id TEXT,
    opened_via_transport_profile_id TEXT,
    opened_via_protocol TEXT,
    opened_via_integration_owner TEXT NOT NULL,
    opened_via_submission_channel TEXT NOT NULL CHECK (opened_via_submission_channel IN ('DPS_PRRO_FISCAL_SERVER','DPS_UNIFIED_WINDOW')),
    channel_lock_acquired_at TEXT NOT NULL
);
INSERT INTO shifts__new SELECT
    fiscal_number, shift_id, state,
    opened_via_backend_profile_id, opened_via_transport_profile_id,
    opened_via_protocol, opened_via_integration_owner, opened_via_submission_channel,
    channel_lock_acquired_at
FROM shifts;
DROP TABLE shifts;
ALTER TABLE shifts__new RENAME TO shifts;
PRAGMA foreign_keys = ON;

-- ===== 7.5 ingress_inbox: add column + JOIN backfill (orphans handled in post-step) =====
ALTER TABLE ingress_inbox ADD COLUMN submission_channel TEXT;
UPDATE ingress_inbox SET submission_channel = (
    SELECT submission_channel FROM transport_profiles
    WHERE transport_profile_id = ingress_inbox.transport_profile_id
)
WHERE transport_profile_id IS NOT NULL;
-- Orphan rows (transport_profile_id IS NULL) are backfilled by post_025_submission_channel.py
-- (Rebuild of ingress_inbox is also done in post-step, AFTER orphan backfill, to enforce NOT NULL.)

PRAGMA foreign_key_check;
```

**IMPORTANT:** Before committing, read `sql/001_hot_store_init.sql` and reproduce all columns exactly in the rebuild blocks. Same for any indexes or triggers.

- [ ] **Step 6: Create `src/prro_gateway/migrations/post_025_submission_channel.py`**

```python
"""Post-step for migration 025: backfill orphan ingress_inbox rows and enforce NOT NULL."""
from __future__ import annotations

import sqlite3


def run(conn: sqlite3.Connection, app_config) -> None:
    if app_config is None:
        raise RuntimeError(
            "post_025_submission_channel requires app_config to backfill orphan ingress_inbox rows"
        )

    default_channel = app_config.defaults.submission_channel
    if hasattr(default_channel, "value"):
        default_channel = default_channel.value

    conn.execute(
        "UPDATE ingress_inbox SET submission_channel = ? WHERE submission_channel IS NULL",
        (default_channel,),
    )

    leftover = conn.execute(
        "SELECT COUNT(*) FROM ingress_inbox WHERE submission_channel IS NULL"
    ).fetchone()[0]
    if leftover != 0:
        raise RuntimeError(
            f"ingress_inbox backfill incomplete: {leftover} NULL rows remain"
        )

    # Rebuild ingress_inbox to enforce NOT NULL CHECK on submission_channel.
    # IMPORTANT: read sql/001_hot_store_init.sql ingress_inbox schema and
    # reproduce EVERY column in the CREATE TABLE and INSERT below.
    conn.execute("PRAGMA foreign_keys = OFF")
    conn.executescript("""
        CREATE TABLE ingress_inbox__new (
            request_id TEXT PRIMARY KEY,
            idempotency_key TEXT NOT NULL,
            protocol TEXT NOT NULL,
            operation_type TEXT NOT NULL,
            fiscal_number TEXT NOT NULL,
            status TEXT NOT NULL,
            transport_profile_id TEXT,
            backend_profile_id TEXT,
            channel_owner TEXT NOT NULL,
            submission_channel TEXT NOT NULL CHECK (submission_channel IN ('DPS_PRRO_FISCAL_SERVER','DPS_UNIFIED_WINDOW')),
            accepted_at TEXT NOT NULL
        );
        INSERT INTO ingress_inbox__new SELECT
            request_id, idempotency_key, protocol, operation_type, fiscal_number,
            status, transport_profile_id, backend_profile_id, channel_owner,
            submission_channel, accepted_at
        FROM ingress_inbox;
        DROP TABLE ingress_inbox;
        ALTER TABLE ingress_inbox__new RENAME TO ingress_inbox;
    """)
    fk_violations = conn.execute("PRAGMA foreign_key_check").fetchall()
    if fk_violations:
        raise RuntimeError(f"FK check failed after ingress_inbox rebuild: {fk_violations}")
    conn.execute("PRAGMA foreign_keys = ON")
```

**IMPORTANT:** Same as Step 5 — fill in actual `ingress_inbox` columns from `sql/001_hot_store_init.sql`.

- [ ] **Step 7: Run migration tests — expect 7 pass**

Run: `pytest tests/test_migration_025_submission_channel.py -v`
Expected: All 7 pass.

- [ ] **Step 8: Update `ChannelLock` model in `models/storage.py`**

Find `ChannelLock` (per research, around line 97). Replace:

```python
@dataclass(frozen=True)
class ChannelLock:
    backend_profile_id: str
    transport_profile_id: str
    protocol: Protocol
    integration_owner: str
    acquired_at: str
```

With:

```python
from prro_gateway.models.canonical import SubmissionChannel


@dataclass(frozen=True)
class ChannelLock:
    submission_channel: SubmissionChannel
    acquired_at: str
```

Also add `opened_via_submission_channel: SubmissionChannel` to `ShiftRecord` (around line 105-122).

- [ ] **Step 9: Update `repositories/shifts.py`**

Update `get_channel_lock`:

```python
def get_channel_lock(conn: sqlite3.Connection, fiscal_number: str) -> ChannelLock | None:
    row = conn.execute(
        "SELECT opened_via_submission_channel, channel_lock_acquired_at "
        "FROM shifts WHERE fiscal_number = ? AND state = 'OPENED' LIMIT 1",
        (fiscal_number,),
    ).fetchone()
    if row is None:
        return None
    return ChannelLock(
        submission_channel=SubmissionChannel(row["opened_via_submission_channel"]),
        acquired_at=row["channel_lock_acquired_at"],
    )
```

Update `open_shift` to accept and write `opened_via_submission_channel`:

```python
def open_shift(
    conn: sqlite3.Connection,
    *,
    fiscal_number: str,
    shift_id: str,
    opened_via_backend_profile_id: str,
    opened_via_transport_profile_id: str,
    opened_via_protocol: str,
    opened_via_integration_owner: str,
    opened_via_submission_channel: SubmissionChannel,
    channel_lock_acquired_at: str,
) -> None:
    conn.execute(
        "INSERT INTO shifts (fiscal_number, shift_id, state, "
        "opened_via_backend_profile_id, opened_via_transport_profile_id, opened_via_protocol, "
        "opened_via_integration_owner, opened_via_submission_channel, channel_lock_acquired_at) "
        "VALUES (?, ?, 'OPENING', ?, ?, ?, ?, ?, ?)",
        (
            fiscal_number, shift_id,
            opened_via_backend_profile_id, opened_via_transport_profile_id,
            opened_via_protocol, opened_via_integration_owner,
            opened_via_submission_channel.value, channel_lock_acquired_at,
        ),
    )
```

(Adapt to actual existing `open_shift` signature — preserve any other parameters.)

- [ ] **Step 10: Update `repositories/inbox.py:24` — drop `"runtime"` default**

Find the line that sets `channel_owner = channel_owner or "runtime"` (or similar). Replace with strict requirement:

```python
def accept_command(conn, *, channel_owner: str, ...):
    if not channel_owner:
        raise ValueError("channel_owner is required and must be non-empty")
```

- [ ] **Step 11: Run repository tests — expect pass**

Run: `pytest tests/test_repository.py -v`
Expected: Existing tests pass; if any fail because they passed `channel_owner=None`, fix them by passing a real value.

- [ ] **Step 12: Run migration tests again — verify still green**

Run: `pytest tests/test_migration_025_submission_channel.py -v`
Expected: All 7 still pass.

- [ ] **Step 13: Commit**

```bash
git add sql/025_submission_channel.sql \
        src/prro_gateway/migrations/post_025_submission_channel.py \
        src/prro_gateway/models/storage.py \
        src/prro_gateway/repositories/shifts.py \
        src/prro_gateway/repositories/inbox.py \
        tests/test_migration_025_submission_channel.py \
        tests/test_repository.py
git commit -m "feat(migrations+repo): 025 submission_channel + ChannelLock single-field"
```

---

## Task 5: Write_path shift guard + pre-sign mismatch check

**Goal:** Replace the 4-field `_channel_matches` with a single-field comparison; delete `_resolve_channel_lock_dimensions`; add the channel-vs-transport mismatch guard at the validate/guard stage (before sign); update `_handle_shift_open` to write `opened_via_submission_channel`.

**Files:**
- Modify: `src/prro_gateway/services/write_path.py`
- Modify: `src/prro_gateway/services/reconciliation.py` (load opened_via_submission_channel)
- Test: `tests/test_submission_channel_lock.py` (new)
- Test: `tests/test_shift_open_writes_submission_channel.py` (new)
- Test: `tests/test_shift_audit_fields_immutable.py` (new)

**Acceptance Criteria:**
- [ ] `_channel_matches` and `_resolve_channel_lock_dimensions` are deleted
- [ ] Shift guard: `command.submission_channel != lock.submission_channel` → `SHIFT_CHANNEL_SWITCH_FORBIDDEN`
- [ ] Mid-shift command with different `backend_profile_id`/`channel_owner` but same `submission_channel` → succeeds
- [ ] Pre-sign check: `command.submission_channel != routed_transport_profile.submission_channel` → `SUBMISSION_CHANNEL_TRANSPORT_MISMATCH` (before any crypto call)
- [ ] `_handle_shift_open` writes `opened_via_submission_channel` to shift row
- [ ] Reconciliation reads `opened_via_submission_channel`; NULL → `REQUIRES_MANUAL_RECONCILIATION`

**Verify:** `pytest tests/test_submission_channel_lock.py tests/test_shift_open_writes_submission_channel.py tests/test_shift_audit_fields_immutable.py tests/test_write_path.py -v`

**Steps:**

- [ ] **Step 1: Write failing tests**

Create `tests/test_submission_channel_lock.py`:

```python
import pytest

from prro_gateway.models.canonical import SubmissionChannel
from tests.helpers import make_command, make_inbox, open_shift_for_test  # extend helpers in Task 9


def test_open_shift_then_other_channel_command_rejected(write_path, fresh_db):
    open_shift_for_test(
        fresh_db,
        fiscal_number="4000000001",
        submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
    )
    cmd = make_command(
        operation_type="SELL",
        fiscal_number="4000000001",
        submission_channel=SubmissionChannel.DPS_UNIFIED_WINDOW,
    )
    inbox = make_inbox(cmd)
    result = write_path.process(fresh_db, command=cmd, inbox=inbox)
    assert result.error_code == "SHIFT_CHANNEL_SWITCH_FORBIDDEN"
    assert result.error_ctx["expected"] == "DPS_PRRO_FISCAL_SERVER"
    assert result.error_ctx["got"] == "DPS_UNIFIED_WINDOW"


def test_same_channel_different_profile_succeeds(write_path, fresh_db):
    open_shift_for_test(
        fresh_db,
        fiscal_number="4000000001",
        submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
        backend_profile_id="bp-primary",
    )
    cmd = make_command(
        operation_type="SELL",
        fiscal_number="4000000001",
        submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
        backend_profile_id="bp-failover",
    )
    inbox = make_inbox(cmd)
    result = write_path.process(fresh_db, command=cmd, inbox=inbox)
    assert result.error_code != "SHIFT_CHANNEL_SWITCH_FORBIDDEN"
```

Create `tests/test_shift_open_writes_submission_channel.py`:

```python
def test_shift_open_persists_submission_channel(write_path, fresh_db):
    cmd = make_command(
        operation_type="SHIFT_OPEN",
        fiscal_number="4000000001",
        submission_channel=SubmissionChannel.DPS_UNIFIED_WINDOW,
    )
    inbox = make_inbox(cmd)
    write_path.process(fresh_db, command=cmd, inbox=inbox)

    row = fresh_db.execute(
        "SELECT opened_via_submission_channel FROM shifts WHERE fiscal_number = ?",
        ("4000000001",),
    ).fetchone()
    assert row["opened_via_submission_channel"] == "DPS_UNIFIED_WINDOW"
```

Create `tests/test_shift_audit_fields_immutable.py`:

```python
def test_audit_fields_unchanged_after_subsequent_command(write_path, fresh_db):
    cmd_open = make_command(
        operation_type="SHIFT_OPEN",
        fiscal_number="4000000001",
        submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
        backend_profile_id="bp-original",
        transport_profile_id="tp-original",
        channel_owner="rest-api",
    )
    write_path.process(fresh_db, command=cmd_open, inbox=make_inbox(cmd_open))

    cmd_sell = make_command(
        operation_type="SELL",
        fiscal_number="4000000001",
        submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
        backend_profile_id="bp-failover",
        transport_profile_id="tp-failover",
        channel_owner="rest-api-v2",
    )
    write_path.process(fresh_db, command=cmd_sell, inbox=make_inbox(cmd_sell))

    row = fresh_db.execute(
        "SELECT opened_via_backend_profile_id, opened_via_transport_profile_id, "
        "opened_via_integration_owner FROM shifts WHERE fiscal_number = ?",
        ("4000000001",),
    ).fetchone()
    assert row["opened_via_backend_profile_id"] == "bp-original"
    assert row["opened_via_transport_profile_id"] == "tp-original"
    assert row["opened_via_integration_owner"] == "rest-api"
```

- [ ] **Step 2: Run tests — expect fail (helpers not updated, write_path uses old guard)**

Run: `pytest tests/test_submission_channel_lock.py tests/test_shift_open_writes_submission_channel.py tests/test_shift_audit_fields_immutable.py -v`
Expected: Fail. The helper updates land in Task 9; for now, write minimal test helpers inline.

- [ ] **Step 3: Update `services/write_path.py` — replace `_channel_matches` with single-field check**

Find `_channel_matches` and `_resolve_channel_lock_dimensions`. Delete both. Find the call site (per research, around lines 1654-1663). Replace:

```python
backend_profile_id = command.backend_profile_id or inbox.backend_profile_id
transport_profile_id = command.transport_profile_id or inbox.transport_profile_id
protocol = command.protocol
channel_owner = command.channel_owner or inbox.channel_owner
if not _channel_matches(lock, backend_profile_id, transport_profile_id, protocol, channel_owner):
    return WriteResult.error(SHIFT_CHANNEL_SWITCH_FORBIDDEN, ...)
```

With:

```python
if command.submission_channel != lock.submission_channel:
    return WriteResult.error(
        code=ErrorCode.SHIFT_CHANNEL_SWITCH_FORBIDDEN,
        ctx={
            "expected": lock.submission_channel.value,
            "got": command.submission_channel.value,
            "fiscal_number": command.fiscal_number,
            "lock_acquired_at": lock.acquired_at,
        },
    )
```

- [ ] **Step 4: Add pre-sign channel/transport mismatch check**

Find the validate/guard stage in `services/write_path.py` (just before the sign stage). Add:

```python
profile = self._transport_profiles_repo.get(command.transport_profile_id)
if profile is None:
    return WriteResult.error(
        code=ErrorCode.UNKNOWN_TRANSPORT_PROFILE,
        ctx={"transport_profile_id": command.transport_profile_id},
    )
if profile.submission_channel != command.submission_channel:
    return WriteResult.error(
        code=ErrorCode.SUBMISSION_CHANNEL_TRANSPORT_MISMATCH,
        ctx={
            "expected": command.submission_channel.value,
            "got": profile.submission_channel.value,
            "transport_profile_id": command.transport_profile_id,
        },
    )
```

(`_transport_profiles_repo` may need to be added to write_path's constructor — wire it via `RuntimeContainer`.)

Also add `SUBMISSION_CHANNEL_TRANSPORT_MISMATCH` and `UNKNOWN_TRANSPORT_PROFILE` to the `ErrorCode` enum in `enums.py` (if not present).

- [ ] **Step 5: Update `_handle_shift_open` to write `opened_via_submission_channel`**

Find `_handle_shift_open`. Add `opened_via_submission_channel=command.submission_channel` to the `open_shift(...)` call.

- [ ] **Step 6: Update `services/reconciliation.py` to read `opened_via_submission_channel`**

Find the place where active shifts are loaded. Add a check: if `opened_via_submission_channel` is NULL (defensive), mark shift as `REQUIRES_MANUAL_RECONCILIATION` and skip recovery for it.

- [ ] **Step 7: Run new tests — expect pass**

Run: `pytest tests/test_submission_channel_lock.py tests/test_shift_open_writes_submission_channel.py tests/test_shift_audit_fields_immutable.py -v`
Expected: All pass.

- [ ] **Step 8: Run write_path test suite (existing tests will fail until Task 9 fixture-update; that's OK for this commit)**

Run: `pytest tests/test_write_path.py --tb=line -q 2>&1 | tail -20`
Note expected failures for fixture-update later.

- [ ] **Step 9: Commit**

```bash
git add src/prro_gateway/services/write_path.py \
        src/prro_gateway/services/reconciliation.py \
        src/prro_gateway/enums.py \
        tests/test_submission_channel_lock.py \
        tests/test_shift_open_writes_submission_channel.py \
        tests/test_shift_audit_fields_immutable.py
git commit -m "feat(write_path): submission_channel single-field shift guard + pre-sign mismatch"
```

---

## Task 6: Router defense-in-depth + delete outbound checkbox

**Goal:** Add the same `submission_channel != transport.submission_channel` check to `transports/router.py` as defense-in-depth (catches anything missed upstream). Delete `transports/checkbox_rest.py` and remove all `CHECKBOX_REST_TRANSPORT` / `CHECKBOX_CLOUD_COMPAT` registrations.

**Files:**
- Modify: `src/prro_gateway/transports/router.py`
- Delete: `src/prro_gateway/transports/checkbox_rest.py`
- Delete: `tests/test_checkbox_transport.py` (if exists)
- Modify: `src/prro_gateway/runtime/container.py` (remove CHECKBOX registrations)
- Test: `tests/test_router_channel_mismatch.py` (new)

**Acceptance Criteria:**
- [ ] `router.route()` raises `SubmissionChannelTransportMismatch` if command/transport channels disagree
- [ ] `transports/checkbox_rest.py` is deleted
- [ ] `tests/test_checkbox_transport.py` is deleted (if present)
- [ ] No code reference to `CHECKBOX_REST_TRANSPORT` or `CHECKBOX_CLOUD_COMPAT` remains in `src/`
- [ ] `grep -r 'CHECKBOX_CLOUD_COMPAT\|CHECKBOX_REST_TRANSPORT' src/ ops/` returns zero hits (sql/025 is allowed)

**Verify:** `pytest tests/test_router_channel_mismatch.py -v`

**Steps:**

- [ ] **Step 1: Write failing test**

Create `tests/test_router_channel_mismatch.py`:

```python
import pytest

from prro_gateway.models.canonical import SubmissionChannel
from prro_gateway.transports.router import (
    ProfileAwareTransportRouter,
    SubmissionChannelTransportMismatch,
)


def test_router_raises_on_channel_mismatch():
    router = ProfileAwareTransportRouter(
        transports={"tp-prro": _StubTransport()},
        transport_profiles={
            "tp-prro": _StubProfile(submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER),
        },
    )
    cmd = _make_cmd(
        transport_profile_id="tp-prro",
        submission_channel=SubmissionChannel.DPS_UNIFIED_WINDOW,
    )
    with pytest.raises(SubmissionChannelTransportMismatch) as exc:
        router.route(cmd)
    assert exc.value.expected == SubmissionChannel.DPS_UNIFIED_WINDOW
    assert exc.value.got == SubmissionChannel.DPS_PRRO_FISCAL_SERVER


def test_router_passes_on_channel_match():
    router = ProfileAwareTransportRouter(
        transports={"tp-prro": _StubTransport()},
        transport_profiles={
            "tp-prro": _StubProfile(submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER),
        },
    )
    cmd = _make_cmd(
        transport_profile_id="tp-prro",
        submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
    )
    transport = router.route(cmd)
    assert transport is not None
```

(Define `_StubTransport`, `_StubProfile`, `_make_cmd` helpers inline or in `tests/conftest.py`.)

- [ ] **Step 2: Run test — expect fail**

Run: `pytest tests/test_router_channel_mismatch.py -v`
Expected: Fail (no `SubmissionChannelTransportMismatch` exception class).

- [ ] **Step 3: Add `SubmissionChannelTransportMismatch` exception and router check**

In `src/prro_gateway/transports/router.py`:

```python
class SubmissionChannelTransportMismatch(Exception):
    def __init__(self, expected, got, transport_profile_id):
        self.expected = expected
        self.got = got
        self.transport_profile_id = transport_profile_id
        super().__init__(
            f"submission_channel mismatch: command={expected.value}, "
            f"transport_profile {transport_profile_id} declares {got.value}"
        )


class ProfileAwareTransportRouter:
    def route(self, command):
        profile = self._transport_profiles[command.transport_profile_id]
        if profile.submission_channel != command.submission_channel:
            raise SubmissionChannelTransportMismatch(
                expected=command.submission_channel,
                got=profile.submission_channel,
                transport_profile_id=command.transport_profile_id,
            )
        return self._transports[command.transport_profile_id]
```

(Adapt to existing router structure; preserve other behavior.)

- [ ] **Step 4: Run test — expect pass**

Run: `pytest tests/test_router_channel_mismatch.py -v`
Expected: 2 tests pass.

- [ ] **Step 5: Delete `transports/checkbox_rest.py`**

```bash
rm src/prro_gateway/transports/checkbox_rest.py
```

- [ ] **Step 6: Delete `tests/test_checkbox_transport.py` if exists**

```bash
test -f tests/test_checkbox_transport.py && rm tests/test_checkbox_transport.py || echo "not present"
```

- [ ] **Step 7: Remove all CHECKBOX registrations from `runtime/container.py` and `transports/router.py`**

```bash
grep -rn 'CHECKBOX_REST_TRANSPORT\|CHECKBOX_CLOUD_COMPAT\|CheckboxRestTransport\|checkbox_rest' src/prro_gateway/
```

For each match: remove the import, remove the registration (e.g., from a transport factory dict), remove any conditional branches. If a function/class becomes unused after removal, delete it.

- [ ] **Step 8: Verify zero CHECKBOX references**

```bash
grep -rn 'CHECKBOX_CLOUD_COMPAT\|CHECKBOX_REST_TRANSPORT' src/ ops/
```
Expected: zero output.

```bash
grep -rn 'CHECKBOX_CLOUD_COMPAT\|CHECKBOX_REST_TRANSPORT' sql/ | grep -v '025_submission_channel.sql'
```
Expected: zero output (025 is allowed because it RAISEs ABORT on these strings).

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(transports): router channel-mismatch defense + delete outbound checkbox"
```

---

## Task 7: channel_policy boot validators + RuntimeContainer wiring

**Goal:** Create `runtime/channel_policy.py` with the endpoint whitelist and binding-consistency validators. Wire both into `RuntimeContainer.initialize()` to run before `health.ready` is set. Provide a shared helper used by both write_path and router.

**Files:**
- Create: `src/prro_gateway/runtime/channel_policy.py`
- Modify: `src/prro_gateway/runtime/container.py`
- Modify: `src/prro_gateway/services/write_path.py` (use shared helper)
- Modify: `src/prro_gateway/transports/router.py` (use shared helper)
- Test: `tests/test_boot_endpoint_whitelist.py` (new)
- Test: `tests/test_boot_binding_consistency.py` (new)

**Acceptance Criteria:**
- [ ] `validate_transport_profiles(profiles)` accepts canonical hosts; rejects others; rejects IP literals
- [ ] `validate_bindings_consistency(bindings, profiles_by_id)` rejects fiscal_number with multiple channels in bindings
- [ ] Both run in `RuntimeContainer.initialize()` before health.ready
- [ ] Helper `assert_command_transport_channel_match(command, profile)` exists and is reused by write_path + router

**Verify:** `pytest tests/test_boot_endpoint_whitelist.py tests/test_boot_binding_consistency.py -v`

**Steps:**

- [ ] **Step 1: Write failing tests**

Create `tests/test_boot_endpoint_whitelist.py`:

```python
import pytest

from prro_gateway.models.canonical import SubmissionChannel
from prro_gateway.runtime.channel_policy import (
    CHANNEL_ENDPOINT_WHITELIST,
    ChannelPolicyViolation,
    validate_transport_profiles,
)


def _profile(tp_id, channel, endpoint):
    return type("P", (), {
        "transport_profile_id": tp_id,
        "submission_channel": channel,
        "endpoint": endpoint,
        "is_active": True,
    })()


def test_whitelist_accepts_canonical_host():
    p = _profile("tp1", SubmissionChannel.DPS_PRRO_FISCAL_SERVER, "https://prro.tax.gov.ua/api")
    validate_transport_profiles([p])  # no raise


def test_whitelist_accepts_subdomain():
    p = _profile("tp1", SubmissionChannel.DPS_PRRO_FISCAL_SERVER, "https://prro-test.prro.tax.gov.ua/api")
    validate_transport_profiles([p])  # no raise


def test_whitelist_rejects_unknown_host():
    p = _profile("tp1", SubmissionChannel.DPS_PRRO_FISCAL_SERVER, "https://evil.com/api")
    with pytest.raises(ChannelPolicyViolation, match="evil.com"):
        validate_transport_profiles([p])


def test_whitelist_rejects_ip_literal():
    p = _profile("tp1", SubmissionChannel.DPS_PRRO_FISCAL_SERVER, "https://192.168.1.1/api")
    with pytest.raises(ChannelPolicyViolation, match="IP literal"):
        validate_transport_profiles([p])


def test_whitelist_normalizes_uppercase_host():
    p = _profile("tp1", SubmissionChannel.DPS_PRRO_FISCAL_SERVER, "https://PRRO.Tax.Gov.UA/api")
    validate_transport_profiles([p])  # no raise — host lowercased


def test_whitelist_skips_inactive_profile():
    p = _profile("tp1", SubmissionChannel.DPS_PRRO_FISCAL_SERVER, "https://evil.com/api")
    p.is_active = False
    validate_transport_profiles([p])  # no raise — inactive
```

Create `tests/test_boot_binding_consistency.py`:

```python
import pytest

from prro_gateway.models.canonical import SubmissionChannel
from prro_gateway.runtime.channel_policy import (
    ChannelPolicyViolation,
    validate_bindings_consistency,
)


def _binding(fn, tp_id):
    return type("B", (), {"fiscal_number": fn, "transport_profile_id": tp_id})()


def _profile(tp_id, channel):
    return type("P", (), {"transport_profile_id": tp_id, "submission_channel": channel})()


def test_single_channel_per_fiscal_number_passes():
    profiles_by_id = {
        "tp1": _profile("tp1", SubmissionChannel.DPS_PRRO_FISCAL_SERVER),
        "tp2": _profile("tp2", SubmissionChannel.DPS_PRRO_FISCAL_SERVER),
    }
    bindings = [_binding("4000000001", "tp1"), _binding("4000000001", "tp2")]
    validate_bindings_consistency(bindings, profiles_by_id)  # no raise


def test_multiple_channels_per_fiscal_number_rejected():
    profiles_by_id = {
        "tp1": _profile("tp1", SubmissionChannel.DPS_PRRO_FISCAL_SERVER),
        "tp2": _profile("tp2", SubmissionChannel.DPS_UNIFIED_WINDOW),
    }
    bindings = [_binding("4000000001", "tp1"), _binding("4000000001", "tp2")]
    with pytest.raises(ChannelPolicyViolation, match="4000000001"):
        validate_bindings_consistency(bindings, profiles_by_id)
```

- [ ] **Step 2: Run tests — expect fail (module doesn't exist)**

Run: `pytest tests/test_boot_endpoint_whitelist.py tests/test_boot_binding_consistency.py -v`
Expected: All fail with `ImportError`.

- [ ] **Step 3: Create `src/prro_gateway/runtime/channel_policy.py`**

```python
"""Boot-time channel policy validators."""
from __future__ import annotations

import ipaddress
from collections import defaultdict
from urllib.parse import urlparse

from prro_gateway.models.canonical import SubmissionChannel


CHANNEL_ENDPOINT_WHITELIST: dict[SubmissionChannel, list[str]] = {
    SubmissionChannel.DPS_PRRO_FISCAL_SERVER: ["prro.tax.gov.ua"],
    SubmissionChannel.DPS_UNIFIED_WINDOW: ["cabinet.tax.gov.ua"],
}


class ChannelPolicyViolation(Exception):
    pass


def _extract_host(endpoint: str) -> str:
    parsed = urlparse(endpoint)
    return (parsed.hostname or "").lower()


def _is_ip_literal(host: str) -> bool:
    try:
        ipaddress.ip_address(host)
        return True
    except ValueError:
        return False


def validate_transport_profiles(profiles) -> None:
    """Raise ChannelPolicyViolation if any active profile's endpoint mismatches whitelist."""
    for p in profiles:
        if not getattr(p, "is_active", True):
            continue
        host = _extract_host(p.endpoint)
        if not host:
            raise ChannelPolicyViolation(
                f"transport_profile {p.transport_profile_id}: endpoint {p.endpoint!r} has no hostname"
            )
        if _is_ip_literal(host):
            raise ChannelPolicyViolation(
                f"transport_profile {p.transport_profile_id}: endpoint host is IP literal {host!r}, "
                f"only DNS hosts allowed"
            )
        whitelist = CHANNEL_ENDPOINT_WHITELIST.get(p.submission_channel, [])
        whitelist_lower = [w.lower() for w in whitelist]
        if not any(host == w or host.endswith("." + w) for w in whitelist_lower):
            raise ChannelPolicyViolation(
                f"transport_profile {p.transport_profile_id}: declares submission_channel="
                f"{p.submission_channel.value} but endpoint host {host!r} not in whitelist "
                f"{whitelist}"
            )


def validate_bindings_consistency(bindings, profiles_by_id) -> None:
    """Raise ChannelPolicyViolation if any fiscal_number is bound to multiple submission_channels."""
    by_fn: dict[str, set[SubmissionChannel]] = defaultdict(set)
    for b in bindings:
        profile = profiles_by_id[b.transport_profile_id]
        by_fn[b.fiscal_number].add(profile.submission_channel)
    for fn, channels in by_fn.items():
        if len(channels) > 1:
            raise ChannelPolicyViolation(
                f"fiscal_number {fn} has bindings to multiple submission_channels: "
                f"{sorted(c.value for c in channels)}"
            )


def assert_command_transport_channel_match(command, transport_profile) -> None:
    """Raise ChannelPolicyViolation if command vs transport channels disagree."""
    if command.submission_channel != transport_profile.submission_channel:
        raise ChannelPolicyViolation(
            f"command.submission_channel={command.submission_channel.value} != "
            f"transport_profile {transport_profile.transport_profile_id}."
            f"submission_channel={transport_profile.submission_channel.value}"
        )
```

- [ ] **Step 4: Run tests — expect pass**

Run: `pytest tests/test_boot_endpoint_whitelist.py tests/test_boot_binding_consistency.py -v`
Expected: All pass.

- [ ] **Step 5: Wire validators into `RuntimeContainer.initialize()`**

In `src/prro_gateway/runtime/container.py`, find `initialize()` (or equivalent boot phase). Add **before** `health.ready = True`:

```python
from prro_gateway.runtime.channel_policy import (
    validate_transport_profiles,
    validate_bindings_consistency,
)


# In initialize():
profiles = self._transport_profiles_repo.list_all()
bindings = self._bindings_repo.list_all()
profiles_by_id = {p.transport_profile_id: p for p in profiles}
validate_transport_profiles(profiles)
validate_bindings_consistency(bindings, profiles_by_id)
```

If `transport_profiles_repo` doesn't have `list_all`, add it as a thin SELECT.

- [ ] **Step 6: Refactor write_path and router to use shared `assert_command_transport_channel_match`**

In `services/write_path.py` and `transports/router.py`, replace inline channel-mismatch checks with calls to the helper. This consolidates the comparison logic into one place.

- [ ] **Step 7: Run all related tests**

Run: `pytest tests/test_boot_endpoint_whitelist.py tests/test_boot_binding_consistency.py tests/test_router_channel_mismatch.py tests/test_submission_channel_lock.py -v`
Expected: All pass.

- [ ] **Step 8: Commit**

```bash
git add src/prro_gateway/runtime/channel_policy.py \
        src/prro_gateway/runtime/container.py \
        src/prro_gateway/services/write_path.py \
        src/prro_gateway/transports/router.py \
        tests/test_boot_endpoint_whitelist.py \
        tests/test_boot_binding_consistency.py
git commit -m "feat(runtime): channel_policy boot validators + shared mismatch helper"
```

---

## Task 8: Adapter / shell wiring

**Goal:** Update `IngressService.build_context()` to require `submission_channel`. Update each ingress shell (REST, XML-RPC, Maria) to pass it from `config.defaults`. Update `maria304_native.py` to also pass it. Add `tests/test_channel_owner_required.py`.

**Files:**
- Modify: `src/prro_gateway/services/ingress.py:85-107` (`build_context` signature)
- Modify: `src/prro_gateway/runtime/rest_app.py`
- Modify: `src/prro_gateway/runtime/xmlrpc_shell.py`
- Modify: `src/prro_gateway/runtime/maria_shell.py`
- Modify: `src/prro_gateway/adapters/maria304_native.py:154-159`
- Test: `tests/test_channel_owner_required.py` (new)

**Acceptance Criteria:**
- [ ] `IngressService.build_context()` requires `submission_channel: SubmissionChannel`
- [ ] All three shells pass `submission_channel` from `config.defaults`
- [ ] `maria304_native.py` builds AdapterContext with `submission_channel` from a passed-in default
- [ ] Adapter rejects request with empty/missing channel_owner before writing to inbox

**Verify:** `pytest tests/test_channel_owner_required.py tests/test_runtime_*.py -v`

**Steps:**

- [ ] **Step 1: Write failing tests**

Create `tests/test_channel_owner_required.py`:

```python
import pytest
from pydantic import ValidationError

from prro_gateway.models.canonical import SubmissionChannel
from prro_gateway.services.ingress import IngressService


def test_build_context_requires_submission_channel():
    with pytest.raises(TypeError, match="submission_channel"):
        IngressService.build_context(
            fiscal_number="4000000001",
            backend_profile_id="bp1",
            transport_profile_id="tp1",
            channel_owner="rest-api",
        )


def test_build_context_rejects_empty_channel_owner():
    with pytest.raises((ValidationError, ValueError), match="channel_owner"):
        IngressService.build_context(
            fiscal_number="4000000001",
            backend_profile_id="bp1",
            transport_profile_id="tp1",
            channel_owner="",
            submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
        )


def test_build_context_succeeds_with_all_required():
    ctx_dict = IngressService.build_context(
        fiscal_number="4000000001",
        backend_profile_id="bp1",
        transport_profile_id="tp1",
        channel_owner="rest-api",
        submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
    )
    assert ctx_dict["channel_owner"] == "rest-api"
    assert ctx_dict["submission_channel"] == "DPS_PRRO_FISCAL_SERVER"
```

- [ ] **Step 2: Run test — expect fail**

Run: `pytest tests/test_channel_owner_required.py -v`
Expected: Fail.

- [ ] **Step 3: Update `IngressService.build_context()` in `services/ingress.py:85-107`**

Add `submission_channel` parameter:

```python
@staticmethod
def build_context(
    *,
    fiscal_number: str,
    backend_profile_id: str,
    transport_profile_id: str,
    channel_owner: str,
    submission_channel: SubmissionChannel,
    route_key: str | None = None,
    source_ip: str | None = None,
    source_port: int | None = None,
    session_id: str | None = None,
    correlation_id: str | None = None,
) -> dict[str, Any]:
    return AdapterContext(
        request_id=f"req-{fiscal_number}-{int(datetime.now(UTC).timestamp() * 1000000)}",
        fiscal_number=fiscal_number,
        route_key=route_key,
        backend_profile_id=backend_profile_id,
        transport_profile_id=transport_profile_id,
        channel_owner=channel_owner,
        submission_channel=submission_channel,
        business_ts=datetime.now(UTC),
        trace_context=TraceContext(source_ip=source_ip, source_port=source_port, session_id=session_id, correlation_id=correlation_id),
    ).model_dump(mode="json", exclude_none=True)
```

- [ ] **Step 4: Update each shell to pass `submission_channel`**

In each shell (`rest_app.py`, `xmlrpc_shell.py`, `maria_shell.py`), find the call to `build_context(...)` and add `submission_channel=config.defaults.submission_channel`.

Example for `rest_app.py`:

```python
ctx = IngressService.build_context(
    fiscal_number=request.fiscal_number,
    backend_profile_id=config.defaults.backend_profile_id,
    transport_profile_id=config.defaults.transport_profile_id,
    channel_owner=config.defaults.channel_owner,
    submission_channel=config.defaults.submission_channel,
    source_ip=request.client.host,
)
```

- [ ] **Step 5: Update `adapters/maria304_native.py:154-159`**

Find the `AdapterContext(...)` construction. Add `submission_channel`:

```python
context = AdapterContext(
    request_id=self._build_request_id(fiscal_number, now_utc),
    fiscal_number=fiscal_number,
    channel_owner="maria304-driver",
    submission_channel=self._submission_channel,
    business_ts=now_utc,
)
```

Add `submission_channel` to `__init__` of the class (or the relevant adapter factory).

- [ ] **Step 6: Update RuntimeContainer to pass `submission_channel` to maria304 adapter**

Where the maria304 adapter is constructed (in `runtime/container.py` or `runtime/maria_shell.py`), pass `submission_channel=self._config.defaults.submission_channel`.

- [ ] **Step 7: Run tests — expect pass**

Run: `pytest tests/test_channel_owner_required.py tests/test_runtime_*.py -v`
Expected: New tests pass; runtime tests should pass once fixtures are updated (some may still fail until Task 9).

- [ ] **Step 8: Commit**

```bash
git add src/prro_gateway/services/ingress.py \
        src/prro_gateway/runtime/rest_app.py \
        src/prro_gateway/runtime/xmlrpc_shell.py \
        src/prro_gateway/runtime/maria_shell.py \
        src/prro_gateway/adapters/maria304_native.py \
        src/prro_gateway/runtime/container.py \
        tests/test_channel_owner_required.py
git commit -m "feat(adapters+shells): submission_channel from config; channel_owner non-null"
```

---

## Task 9: Test fixture mass update + remaining test files

**Goal:** Update all test fixtures and helpers to include `submission_channel` and a real `channel_owner`. ~50 test files reference channel-related symbols. Most need only fixture-keyword additions; `tests/test_gate1f_channel_lock.py` and `tests/test_gate1u_channel_lock_persistence.py` need behavioral rewrites.

**Files:**
- Modify: `tests/conftest.py` (primary fixture updates)
- Modify: ~48 test files for fixture-only updates
- Modify: `tests/test_gate1f_channel_lock.py` (behavioral rewrite)
- Modify: `tests/test_gate1u_channel_lock_persistence.py` (behavioral rewrite)
- Modify: `tests/test_pilot_smoke.py` (config update)

**Acceptance Criteria:**
- [ ] `pytest tests/ --tb=no -q` returns 100% pass (no failures)
- [ ] No test file passes `channel_owner=None` or omits `submission_channel` to `make_command` / `AdapterContext`

**Verify:** `pytest tests/ --tb=no -q`

**Steps:**

- [ ] **Step 1: Update `tests/conftest.py` and any helper modules**

Read `tests/conftest.py` and find any factory helpers (e.g., `make_command`, `make_adapter_context`, `open_shift_for_test`, fixtures named `command`, `adapter_context`, `channel_lock`, `shift`).

Update each helper to:
- Accept a `submission_channel` keyword (default `SubmissionChannel.DPS_PRRO_FISCAL_SERVER`)
- Accept a `channel_owner` keyword (default `"rest-api"`)
- Pass both to underlying constructors

Example update:

```python
from prro_gateway.models.canonical import SubmissionChannel


def make_command(
    *,
    operation_type="SELL",
    fiscal_number="4000000001",
    submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER,
    channel_owner="rest-api",
    backend_profile_id="bp-default",
    transport_profile_id="tp-default",
    **kwargs,
):
    return CanonicalFiscalCommand(
        submission_channel=submission_channel,
        channel_owner=channel_owner,
        backend_profile_id=backend_profile_id,
        transport_profile_id=transport_profile_id,
        **kwargs,
    )
```

If `transport_profiles` fixture is used, ensure each seeded profile has `submission_channel` matching its kind:

```python
@pytest.fixture
def transport_profiles(fresh_db):
    fresh_db.execute(
        "INSERT INTO transport_profiles (transport_profile_id, kind, submission_channel, "
        "endpoint, is_active) VALUES (?, ?, ?, ?, 1)",
        ("tp-default", "DPS_PRRO_GRPC_ECABINET", "DPS_PRRO_FISCAL_SERVER",
         "https://prro.tax.gov.ua/api"),
    )
    fresh_db.commit()
```

- [ ] **Step 2: Run pytest, identify all remaining failures**

Run: `pytest tests/ --tb=no -q 2>&1 | tail -100`
Note all failing tests. Group by failure pattern (e.g., "missing submission_channel" vs "channel_owner=None" vs "shift guard semantics").

- [ ] **Step 3: Sweep through fixture-only update files (~48 files)**

For each file in the failure list that fails only due to missing keyword: add `submission_channel=SubmissionChannel.DPS_PRRO_FISCAL_SERVER` (and `channel_owner="rest-api"` if needed) to each `make_command` / `AdapterContext(...)` / similar call.

Use Grep + targeted Edit. Pattern: search for `CanonicalFiscalCommand(` or `make_command(` calls and add the keyword.

- [ ] **Step 4: Behavioral rewrite — `tests/test_gate1f_channel_lock.py`**

Read the existing tests. Categorize each:
- Tests that assert "different `backend_profile_id` mid-shift → reject" → REWRITE: change to "different `submission_channel` mid-shift → reject"
- Tests that assert "different `protocol` mid-shift → reject" → REMOVE (semantics changed; protocol no longer locked)
- Tests that assert "same lock fields → succeed" → UPDATE: ensure same `submission_channel`, possibly different other fields

For each rewritten test, ensure the test name reflects the new semantics.

- [ ] **Step 5: Behavioral rewrite — `tests/test_gate1u_channel_lock_persistence.py`**

Same approach. The persistence tests should now verify:
- `opened_via_submission_channel` survives serialization/restart
- Audit fields (`opened_via_backend_profile_id`, etc.) also survive but are not used for guard

- [ ] **Step 6: Update `tests/test_pilot_smoke.py`**

Find the test config dict/YAML. Add `submission_channel: DPS_PRRO_FISCAL_SERVER` to defaults. Verify the smoke test passes.

- [ ] **Step 7: Run full pytest — expect 100% green**

Run: `pytest tests/ --tb=line -q 2>&1 | tail -30`
Expected: 0 failed.

If any failure remains: fix it. Don't claim done with red tests.

- [ ] **Step 8: Commit**

```bash
git add tests/
git commit -m "test: fixture mass update for submission_channel + channel-lock rewrites"
```

---

## Task 10: Documentation + CLAUDE.md FI-10 update

**Goal:** Update CLAUDE.md (project root + `.claude/`) FI-10 wording to reflect Checkbox-as-ingress-only. Remove outbound-Checkbox references from current-state docs. Add a changelog/roadmap note about the mid-shift backend_profile_id swap behavior change.

**Files:**
- Modify: `CLAUDE.md`
- Modify: `.claude/CLAUDE.md`
- Modify: `docs/Multi-Protocol_PRRO_Gateway.md`
- Modify: `docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md`
- Modify: `docs/ROADMAP_v3_PILOT.md`
- Modify: `docs/OPERATIONS.md` (changelog note)

**Acceptance Criteria:**
- [ ] FI-10 in both CLAUDE.md files reflects new wording
- [ ] `grep -r 'CHECKBOX_CLOUD_COMPAT\|CHECKBOX_REST_TRANSPORT' docs/Multi-Protocol_PRRO_Gateway.md docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md docs/ROADMAP_v3_PILOT.md` → zero hits (or only annotated as historical)
- [ ] OPERATIONS.md notes that mid-shift profile swap now allowed if channel matches

**Verify:** Manual review + grep

**Steps:**

- [ ] **Step 1: Update FI-10 in `CLAUDE.md`**

Find the line:
```
10. For Checkbox-compatible flows, local signing may be bypassed only by explicit profile/config behavior, not by accidental code drift.
```

Replace with:

```
10. Checkbox is supported only as an emulated ingress protocol; the canonical command from any ingress (including Checkbox-shaped REST) goes through the same write_path with the same crypto rules. There is no signing-bypass path. Outbound Checkbox transport is forbidden architecturally.
```

- [ ] **Step 2: Same update in `.claude/CLAUDE.md`**

Mirror the change.

- [ ] **Step 3: Update `docs/Multi-Protocol_PRRO_Gateway.md`**

Read the file. Find sections referencing:
- `CHECKBOX_CLOUD_COMPAT` backend type → remove (or annotate as removed in 2026-04-25)
- `CHECKBOX_REST_TRANSPORT` transport kind → same
- "Checkbox cloud as backend" architecture diagrams/text → revise to indicate Checkbox is ingress only

- [ ] **Step 4: Update `docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md` and `docs/ROADMAP_v3_PILOT.md`**

Same: search for outbound-Checkbox references and revise.

- [ ] **Step 5: Add changelog/note to `docs/OPERATIONS.md`**

Append a section:

```markdown
## 2026-04-25 — Submission channel as first-class concept

Channel lock semantics changed:
- Previously: shift was locked to a 4-tuple (backend_profile_id, transport_profile_id, protocol, integration_owner). Mid-shift swap of any field was rejected.
- Now: shift is locked to a single `submission_channel` (DPS_PRRO_FISCAL_SERVER or DPS_UNIFIED_WINDOW). Mid-shift swap of `backend_profile_id`, `transport_profile_id`, or `channel_owner` is **allowed** as long as the resolved `submission_channel` stays the same. This enables transport failover (primary → backup endpoint) without closing the shift.

Operators must:
- Set `defaults.submission_channel` explicitly in `config.yaml` (no fallback)
- Ensure all `transport_profiles` for one fiscal_number declare the same `submission_channel`
- Review the new endpoint whitelist in `runtime/channel_policy.py` if using non-canonical DPS hostnames (requires code change to extend whitelist)

Outbound Checkbox transport (`CHECKBOX_REST_TRANSPORT`, `CHECKBOX_CLOUD_COMPAT`) has been removed. Checkbox remains supported as an ingress protocol only.

Migration 025 will refuse to apply if any `CHECKBOX_*` rows or `CUSTOM_TRANSPORT` rows exist (active or inactive). Clean these up before upgrading.
```

- [ ] **Step 6: Final grep verification**

```bash
grep -rn 'CHECKBOX_CLOUD_COMPAT\|CHECKBOX_REST_TRANSPORT' docs/Multi-Protocol_PRRO_Gateway.md docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md docs/ROADMAP_v3_PILOT.md
```
Expected: zero hits, or only inside historical annotations clearly marked.

- [ ] **Step 7: Commit**

```bash
git add CLAUDE.md .claude/CLAUDE.md docs/
git commit -m "docs: FI-10 update for Checkbox-ingress-only; OPERATIONS changelog"
```

---

## Final Verification (after Task 10)

Run from clean checkout:

- [ ] `pytest tests/ --tb=line -q` → 0 failed
- [ ] `python scripts/run_rest.py` with `ops/config.example.yaml` → boot succeeds
- [ ] Temporarily comment `submission_channel` in config → boot fails with clear Pydantic error
- [ ] Manual scenario: create a transport_profile with `endpoint=https://evil.com/`, restart → boot fails with `ChannelPolicyViolation`
- [ ] Manual scenario: SHIFT_OPEN with submission_channel=A → SELL with submission_channel=B → 422 `SHIFT_CHANNEL_SWITCH_FORBIDDEN`
- [ ] `grep -r 'CHECKBOX_CLOUD_COMPAT\|CHECKBOX_REST_TRANSPORT' src/ ops/` → zero hits
- [ ] `grep -r 'CHECKBOX_CLOUD_COMPAT\|CHECKBOX_REST_TRANSPORT' sql/ | grep -v '025_submission_channel.sql'` → zero hits
- [ ] `grep -r 'CHECKBOX_CLOUD_COMPAT\|CHECKBOX_REST_TRANSPORT' docs/ | grep -v 'audits/'` → zero hits or annotated historical

If all pass: PR ready. Open PR with title `feat: submission_channel as first-class concept (closes pilot blockers #1, #2)` and link spec + plan in description.
