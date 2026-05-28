-- W4-Z2a piece 2 — signing_config_snapshots ledger (append-only)
--
-- Per locked design at memory `project_m4_w4_z2a_locked_design`:
--   - Main pool (NOT secure) because FK only works within same DB +
--     this is pinned wire history, not live config.
--   - Append-only: NEVER delete rows.  Snapshots are forensic
--     evidence of "this doc was signed with THIS tax config".
--   - Unique (fn, driver_id, payload_sha256) deduplicates identical
--     configs — same hash → same row, content-addressable.
--   - driver_id NOT NULL, no sentinel.  W4-Z0 listener-stamped
--     architecture pin: production callers MUST populate.
--   - kind = "check_tax_mapping_v1" → future EVPZ / Z-report
--     schema bumps get distinct kinds without breaking rows.
--   - payload_json is canonical (sorted keys, no whitespace);
--     payload_sha256 = SHA256(canonical_bytes).
--   - SHA256 verify on read = defensive integrity check
--     (corrupted snapshot → typed critical error, never silent
--     serve).

CREATE TABLE signing_config_snapshots (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    fn              TEXT    NOT NULL,
    driver_id       TEXT    NOT NULL,
    kind            TEXT    NOT NULL,
    payload_json    TEXT    NOT NULL,
    payload_sha256  BLOB    NOT NULL  CHECK (length(payload_sha256) = 32),
    created_at      TEXT    NOT NULL  DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (fn, driver_id, payload_sha256)
) STRICT;

-- Lookup by (fn, driver_id) for admin / forensic queries.
CREATE INDEX idx_signing_config_snapshots_fn_driver
    ON signing_config_snapshots(fn, driver_id);

-- fiscal_documents references the snapshot pinned at 3-PRE.
-- Nullable for: (a) pre-W4-Z2a rows already in production, (b)
-- documents that genuinely have no tax_group_1 items (no <TX>
-- emit needed; snapshot lookup itself is skipped).
--
-- Semantic constraint enforced at application layer (NOT DDL):
--   NULL + no tax_group_1 items → ALLOW (info audit)
--   NULL + any  tax_group_1 item → RequiresManualReconciliation
--
-- ON DELETE behavior: NO ACTION (default).  Append-only contract
-- means snapshot rows never deleted; if violated via raw SQL, the
-- next snapshot fetch surfaces NotFound and the doc routes to
-- RequiresManualReconciliation.
ALTER TABLE fiscal_documents
    ADD COLUMN signing_config_snapshot_id INTEGER
    REFERENCES signing_config_snapshots(id);

-- Forensic reverse-lookup affordance, NOT a runtime hot query.
-- Used by future admin / audit / monitoring tools (NOT by stage_sign
-- or recovery hot paths — those key into snapshots BY id, never the
-- reverse direction).  Use cases per operator (4-year UA PRRO
-- production):
--   - "Which fiscal_documents are signed with snapshot X?" (audit
--     window investigation after tax rate / driver mapping incident)
--   - "Find orphan snapshots referenced by zero docs" (monitor
--     adapter misconfiguration; expected count ≈ 0 at steady state)
--   - "List docs that crossed an admin tax_groups update boundary"
--
-- Trade-off: minor write amplification (1 INTEGER partial-index
-- entry per fiscal_document with non-NULL FK).  Filter
-- `WHERE signing_config_snapshot_id IS NOT NULL` skips pre-W4-Z2a
-- NULL-FK rows.  For pilot scale (50 points × 70 cashboxes × ~365
-- docs/cashbox/day) the write cost is < 0.1% — acceptable.
--
-- Admin / monitoring tooling that lights up this index: tracked in
-- Beads `PRRO_GATE-p4e` (W4-Z2a follow-up, P3 chore; not pilot-gating).
CREATE INDEX idx_fiscal_documents_signing_config_snapshot_id
    ON fiscal_documents(signing_config_snapshot_id)
    WHERE signing_config_snapshot_id IS NOT NULL;
