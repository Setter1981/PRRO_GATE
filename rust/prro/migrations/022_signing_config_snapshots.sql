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

-- Forensic JOIN: "which docs signed with snapshot X" — needed for
-- admin audit ("what did we sign in the 14:30 window") + for
-- monitoring snapshot churn (snapshots referenced by zero docs may
-- indicate adapter misconfiguration).
CREATE INDEX idx_fiscal_documents_signing_config_snapshot_id
    ON fiscal_documents(signing_config_snapshot_id)
    WHERE signing_config_snapshot_id IS NOT NULL;
