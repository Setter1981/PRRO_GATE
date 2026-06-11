-- ============================================================================
-- 001_baseline.sql — squashed schema baseline (one-time, pre-pilot).
--
-- Supersedes the original 24-migration chain (001_core_identities …
-- 024_fiscal_documents_source_sha256).  The archaeology of that chain — state
-- renames, W-increment ALTERs, CHECK-list rebuilds — lives in git history at
-- the pre-squash commit; it is not needed to stand the schema up.
--
-- GENERATION (NOT hand-written — see docs/superpowers/plans/
-- 2026-06-11-migrations-baseline-squash-spec.md §2): this body is the literal
-- DDL text SQLite stored in `sqlite_master` after applying the full 001-024
-- chain (at git ref 5c6b00a3a9895fd634c322d02dc6c3d925dfcc4b) to a fresh DB,
-- with `_sqlx_migrations` and internal `sqlite_*` objects
-- (sqlite_sequence, sqlite_autoindex_*) excluded.  Objects are emitted in
-- creation (rowid) order, which is dependency-safe.  Because SQLite stores the
-- CREATE statement verbatim, re-applying this baseline reproduces an identical
-- `sqlite_master` BY CONSTRUCTION.
--
-- EQUIVALENCE GATE: `rust/prro/scripts/verify_baseline.sh` proves
-- (old chain) ≡ (this baseline) via a sorted `sqlite_master` dump diff — it
-- must be EMPTY.  Re-run it on review.
--
-- COMMENTS: the curated `--` blocks below carry the still-true load-bearing
-- rationale (W-/INV-/ADR references, index existence, CAS / fail-closed
-- invariants) lifted from the old migrations.  They are SQL comments OUTSIDE
-- the CREATE statements, so they never enter `sqlite_master` and do not affect
-- the equivalence gate.  Inline comments that were already inside the original
-- CREATE statements are preserved verbatim (they are part of the stored DDL).
--
-- The next project migration is 002.
--
-- `migrations_secure/` is a separate chain and is NOT squashed here.
-- Connection-level pragmas (journal_mode, foreign_keys, busy_timeout,
-- synchronous) live in `db::open_pool` (SqliteConnectOptions), not here.
-- ============================================================================

CREATE TABLE fiscal_number_config (
    -- All-digit checks use `NOT GLOB '*[^0-9]*'` because GLOB '[0-9]*'
    -- only constrains the FIRST character ('*' = anything-after).  See
    -- migrations_apply::* tests for behavioural proof.
    fiscal_number          TEXT    PRIMARY KEY  CHECK (length(fiscal_number) = 10 AND NOT fiscal_number GLOB '*[^0-9]*'),
    tax_number             TEXT    NOT NULL,
    vat_payer_inn          TEXT    CHECK (vat_payer_inn IS NULL OR (length(vat_payer_inn) = 12 AND NOT vat_payer_inn GLOB '*[^0-9]*')),
    fiscal_mode            TEXT    NOT NULL  CHECK (fiscal_mode IN ('test','prod')),
    org_name               TEXT,
    point_name             TEXT,
    org_address            TEXT,
    tsp_enabled            INTEGER NOT NULL DEFAULT 0  CHECK (tsp_enabled IN (0,1)),
    offline_enabled        INTEGER NOT NULL DEFAULT 1  CHECK (offline_enabled IN (0,1)),
    national_check_enabled INTEGER NOT NULL DEFAULT 0  CHECK (national_check_enabled IN (0,1)),
    min_offline_codes      INTEGER NOT NULL DEFAULT 0  CHECK (min_offline_codes >= 0),
    max_offline_codes      INTEGER NOT NULL DEFAULT 0  CHECK (max_offline_codes >= 0 AND max_offline_codes >= min_offline_codes),
    created_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;


CREATE TRIGGER fnc_updated_at
AFTER UPDATE ON fiscal_number_config
BEGIN
    UPDATE fiscal_number_config SET updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = NEW.fiscal_number;
END;


CREATE TABLE audit_log (
    audit_id           INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type        TEXT    NOT NULL,
    entity_id          TEXT    NOT NULL,
    event_type         TEXT    NOT NULL,
    severity           TEXT    NOT NULL  CHECK (severity IN ('INFO','WARNING','ERROR','CRITICAL')),
    actor              TEXT,
    event_payload_json TEXT,
    created_at         TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;


CREATE INDEX ix_audit_entity ON audit_log(entity_type, entity_id, audit_id DESC);


CREATE TABLE ingress_inbox (
    request_id               BLOB    PRIMARY KEY  CHECK (length(request_id) = 16),
    fiscal_number            TEXT    NOT NULL,
    protocol                 TEXT    NOT NULL  CHECK (protocol IN ('REST','XMLRPC','MARIA','MARIA304','CHECKBOX_COMPAT','INTERNAL')),
    operation_type           TEXT    NOT NULL,
    idempotency_key          TEXT    NOT NULL,
    status                   TEXT    NOT NULL DEFAULT 'NEW'  CHECK (status IN ('NEW','PROCESSING','DONE','REJECTED','ERROR')),
    payload_json             TEXT    NOT NULL,
    payload_sha256_canonical BLOB    NOT NULL  CHECK (length(payload_sha256_canonical) = 32),
    correlation_id           TEXT,
    received_at              TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    processed_at             TEXT,
    error_text               TEXT
, signed_by_cashier_id TEXT, driver_id TEXT, business_ts TEXT, total_sum_kop INTEGER) STRICT;


CREATE UNIQUE INDEX ux_inbox_fn_idem ON ingress_inbox(fiscal_number, idempotency_key);


CREATE INDEX ix_inbox_pending ON ingress_inbox(fiscal_number, received_at)
    WHERE status IN ('NEW','PROCESSING');


CREATE TABLE sidecar_operators (
    id                BLOB    PRIMARY KEY  CHECK (length(id) = 16),
    fiscal_number     TEXT    NOT NULL,
    operator_name     TEXT,
    -- All-digit check: `NOT GLOB '*[^0-9]*'` rejects any non-digit anywhere.
    operator_inn      TEXT    NOT NULL  CHECK (length(operator_inn) = 10 AND NOT operator_inn GLOB '*[^0-9]*'),
    jks_path          TEXT    NOT NULL,
    jks_password_hex  TEXT    NOT NULL,                  -- always XOR-soft sealed (spec decision #16)
    cred_salt         BLOB    NOT NULL  CHECK (length(cred_salt) = 16),
    active            INTEGER NOT NULL DEFAULT 1  CHECK (active IN (0,1)),
    created_at        TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at        TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;


CREATE UNIQUE INDEX ux_op_fn_inn_active
    ON sidecar_operators(fiscal_number, operator_inn) WHERE active = 1;


CREATE TRIGGER ops_updated_at
AFTER UPDATE ON sidecar_operators
BEGIN
    UPDATE sidecar_operators SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;


CREATE TABLE operator_certs (
    ski_hex          TEXT    PRIMARY KEY  CHECK (length(ski_hex) = 64),
    fiscal_number    TEXT    NOT NULL,
    cert_fingerprint TEXT    NOT NULL,
    cert_der         BLOB    NOT NULL,
    subject_dn       TEXT,
    issuer_dn        TEXT,
    valid_from       TEXT,
    valid_to         TEXT,
    fetched_at       TEXT    NOT NULL,
    source           TEXT    NOT NULL  CHECK (source IN ('container','cmp','manual')),
    active           INTEGER NOT NULL DEFAULT 0  CHECK (active IN (0,1)),
    last_refresh_at  TEXT,
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;


CREATE INDEX ix_op_certs_fn ON operator_certs(fiscal_number);


CREATE UNIQUE INDEX ux_op_certs_active_per_fn
    ON operator_certs(fiscal_number) WHERE active = 1;


CREATE TABLE cert_provisioning_config (
    id                  INTEGER PRIMARY KEY  CHECK (id = 1),
    primary_cmp_url     TEXT    NOT NULL DEFAULT 'http://acskidd.gov.ua:80',
    fallback_cmp_url    TEXT,
    timeout_seconds     INTEGER NOT NULL DEFAULT 10,
    cache_ttl_seconds   INTEGER NOT NULL DEFAULT 3600,
    refresh_within_days INTEGER NOT NULL DEFAULT 30,
    updated_at          TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;

-- ── seed data (carried verbatim from 003 + 006; the chain's result includes
--    these rows — schema alone is not the migration's output)
INSERT INTO cert_provisioning_config (id) VALUES (1);


CREATE TABLE printer_profiles (
    id                BLOB    PRIMARY KEY  CHECK (length(id) = 16),
    name              TEXT    NOT NULL,
    fiscal_number     TEXT,
    profile_key       TEXT    NOT NULL,
    destination_type  TEXT    NOT NULL  CHECK (destination_type IN ('tcp','serial','usb')),
    host              TEXT,
    port              INTEGER,
    serial_device     TEXT,
    serial_baud       INTEGER,
    usb_vendor_id     INTEGER,
    usb_product_id    INTEGER,
    paper_width_mm    INTEGER NOT NULL DEFAULT 80  CHECK (paper_width_mm IN (58,80,112)),
    timeout_ms        INTEGER NOT NULL DEFAULT 5000,
    active            INTEGER NOT NULL DEFAULT 1  CHECK (active IN (0,1)),
    created_at        TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at        TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE SET NULL
) STRICT;


CREATE TABLE tax_group_definitions (
    fiscal_number        TEXT    NOT NULL,
    tax_id               TEXT    NOT NULL,
    name                 TEXT    NOT NULL,
    tax_rate             REAL    NOT NULL DEFAULT 0,
    additional_rate      REAL    NOT NULL DEFAULT 0,
    tax_type             INTEGER NOT NULL DEFAULT 0,
    tax_algorithm        INTEGER NOT NULL DEFAULT 0,
    requires_uktzed      INTEGER NOT NULL DEFAULT 0,
    requires_excise_mark INTEGER NOT NULL DEFAULT 0,
    is_active            INTEGER NOT NULL DEFAULT 1,
    created_at           TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at           TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    PRIMARY KEY (fiscal_number, tax_id),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;


CREATE TABLE payment_type_definitions (
    fiscal_number    TEXT    NOT NULL,
    payment_id       TEXT    NOT NULL,
    name             TEXT    NOT NULL,
    payment_kind     TEXT    NOT NULL,
    is_active        INTEGER NOT NULL DEFAULT 1,
    created_at       TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    PRIMARY KEY (fiscal_number, payment_id),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;


CREATE TABLE backend_profiles (
    backend_profile_id TEXT PRIMARY KEY,
    name               TEXT NOT NULL,
    kind               TEXT NOT NULL  CHECK (kind IN ('DPS_PRRO','CHECKBOX','OTHER')),
    config_json        TEXT,
    created_at         TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;


CREATE TABLE transport_profiles (
    transport_profile_id TEXT PRIMARY KEY,
    name                 TEXT NOT NULL,
    channel_kind         TEXT NOT NULL  CHECK (channel_kind IN ('grpc_cabinet','edyne_vikno','soap_dps','checkbox_rest','sidecar_v2')),
    test_mode            INTEGER NOT NULL DEFAULT 0  CHECK (test_mode IN (0,1)),
    config_json          TEXT,
    created_at           TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;


CREATE TABLE prro_bindings (
    fiscal_number        TEXT PRIMARY KEY,
    backend_profile_id   TEXT NOT NULL,
    transport_profile_id TEXT NOT NULL,
    created_at           TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at           TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number)        REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (backend_profile_id)   REFERENCES backend_profiles(backend_profile_id) ON DELETE RESTRICT,
    FOREIGN KEY (transport_profile_id) REFERENCES transport_profiles(transport_profile_id) ON DELETE RESTRICT
) STRICT;


CREATE TABLE licenses (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    tin              TEXT    NOT NULL,
    fn_numbers_json  TEXT    NOT NULL,
    issued_at        TEXT    NOT NULL,
    expires_at       TEXT    NOT NULL,
    tier             TEXT    NOT NULL  CHECK (tier IN ('demo','basic','pro','enterprise')),
    org_name         TEXT,
    demo_limits_json TEXT,
    payload_b64      TEXT    NOT NULL,
    signature_b64    TEXT    NOT NULL,
    installed_at     TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    active           INTEGER NOT NULL DEFAULT 1  CHECK (active IN (0,1))
) STRICT;


CREATE UNIQUE INDEX ux_lic_active ON licenses(active) WHERE active = 1;


CREATE TABLE ca_endpoints (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT    NOT NULL UNIQUE,
    cmp_url         TEXT    NOT NULL,
    issuer_pattern  TEXT,           -- case-insensitive substring vs cert issuer DN; nullable
    priority        INTEGER NOT NULL DEFAULT 0,
    enabled         INTEGER NOT NULL DEFAULT 1  CHECK (enabled IN (0,1)),
    created_at      TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at      TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP)
) STRICT;

-- ── seed data (carried verbatim from 003 + 006; the chain's result includes
--    these rows — schema alone is not the migration's output)
INSERT INTO ca_endpoints (name, cmp_url, issuer_pattern, priority) VALUES
    ('acskidd', 'http://acskidd.gov.ua:80/services/cmp/', 'acskidd', 10),
    ('ca.tax.gov.ua', 'http://ca.tax.gov.ua:80/services/cmp/', 'tax', 20);


CREATE INDEX ix_ca_endpoints_priority ON ca_endpoints(priority) WHERE enabled = 1;


-- [from 002/008/…] Ledger of issued receipts.  The `state` CHECK is the
-- 13-state document machine; `SENDING` is the ADR-M3-A9 Pattern-B intent marker
-- (added by 008 via a full table rebuild — SQLite cannot ALTER a CHECK).  Later
-- columns (z_report_number, signing_inputs_pinned_at, mac_recovery_attempts,
-- first_kvt1_at, signed_by_cashier_id, consecutive_holds, signing_config_
-- snapshot_id, source_sha256) were additive ALTERs (009/013/014/017/018/020/024),
-- squashed inline here.
CREATE TABLE fiscal_documents (
    document_id                BLOB    PRIMARY KEY  CHECK (length(document_id) = 16),
    request_id                 BLOB    NOT NULL UNIQUE  CHECK (length(request_id) = 16),
    fiscal_number              TEXT    NOT NULL,
    shift_id                   BLOB,
    offline_session_id         BLOB,
    lnd                        INTEGER NOT NULL  CHECK (lnd >= 1),
    doc_type                   TEXT    NOT NULL  CHECK (doc_type IN (
        'SHIFT_OPEN','SHIFT_CLOSE','SELL','RETURN','SERVICE_IN','SERVICE_OUT',
        'CASH_WITHDRAWAL','X_REPORT','Z_REPORT'
    )),
    state                      TEXT    NOT NULL  CHECK (state IN (
        'PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ACK',
        'OFFLINE_LOCAL_ACK','REJECTED','CANCELLED','ERROR_RETRYABLE',
        'REQUIRES_MANUAL_RECONCILIATION'
    )),
    backend_profile_id         TEXT    NOT NULL,
    transport_profile_id       TEXT    NOT NULL,
    fs_mode                    TEXT    NOT NULL  CHECK (fs_mode IN ('ONLINE','OFFLINE')),
    business_ts                TEXT    NOT NULL,
    server_fiscal_no           TEXT,
    server_fiscal_date         TEXT,
    offline_fiscal_no          INTEGER,
    offline_fiscal_date        TEXT,
    total_sum_kop              INTEGER,
    payload_json               TEXT    NOT NULL,
    payload_sha256_canonical   BLOB    NOT NULL  CHECK (length(payload_sha256_canonical) = 32),
    unsigned_xml_sha256        BLOB    CHECK (unsigned_xml_sha256 IS NULL OR length(unsigned_xml_sha256) = 32),
    previous_hash              BLOB    CHECK (previous_hash IS NULL OR length(previous_hash) = 32),
    submission_attempted_at    TEXT,
    technical_return           INTEGER  CHECK (technical_return IS NULL OR technical_return IN (0,1)),
    related_receipt_id         BLOB,
    created_at                 TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at                 TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP), z_report_number INTEGER
  CHECK (z_report_number IS NULL OR z_report_number >= 1), signing_inputs_pinned_at TEXT, mac_recovery_attempts INTEGER NOT NULL DEFAULT 0
        CHECK (mac_recovery_attempts IN (0, 1)), first_kvt1_at TEXT, signed_by_cashier_id TEXT, consecutive_holds INTEGER NOT NULL DEFAULT 0
    CHECK (consecutive_holds >= 0), signing_config_snapshot_id INTEGER
    REFERENCES signing_config_snapshots(id), source_sha256 BLOB
    CHECK (source_sha256 IS NULL OR length(source_sha256) = 32),
    FOREIGN KEY (fiscal_number)       REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (shift_id)            REFERENCES shifts(shift_id)                    ON DELETE RESTRICT,
    FOREIGN KEY (offline_session_id)  REFERENCES offline_sessions(offline_session_id) ON DELETE RESTRICT,
    FOREIGN KEY (related_receipt_id)  REFERENCES fiscal_documents(document_id)        ON DELETE RESTRICT
) STRICT;


CREATE INDEX ix_fd_fn_lnd            ON fiscal_documents(fiscal_number, lnd);


-- [from 007] ADR-M3-A1: `node_state.next_lnd` is the single source of the local
-- numerator; this UNIQUE index is the fail-closed guard against drift — any
-- double-allocation or recovery bug surfaces at INSERT time, not silently
-- downstream.  (`ix_fd_fn_lnd` above is the now-redundant non-unique companion
-- from 002, kept; removal is a separate hygiene concern.)
CREATE UNIQUE INDEX ux_fd_fn_lnd     ON fiscal_documents(fiscal_number, lnd);


CREATE INDEX ix_fd_state_pending     ON fiscal_documents(state, created_at)
    WHERE state IN ('PREPARED','SIGNED','ENCRYPTED','SENDING','SENT','KVT1','KVT2','ERROR_RETRYABLE');


CREATE INDEX ix_fd_recon_manual      ON fiscal_documents(state)
    WHERE state = 'REQUIRES_MANUAL_RECONCILIATION';


CREATE TABLE document_files (
    document_id BLOB    NOT NULL,
    kind        TEXT    NOT NULL  CHECK (kind IN ('PAYLOAD_XML','SIGNED_XML','KVT1_RAW','KVT2_RAW','PAYLOAD_JSON_CANONICAL','RECEIPT_PDF')),
    content     BLOB    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    PRIMARY KEY (document_id, kind),
    FOREIGN KEY (document_id) REFERENCES fiscal_documents(document_id) ON DELETE CASCADE
) STRICT;


-- [from 009] W6 / ADR-M3-A2 Z-report sequencer.  Mirrors the W1 fail-closed
-- pattern (ux_fd_fn_lnd): the per-FN Z number is allocated from
-- `node_state.next_z_report_number` and persisted on the doc row (retry reuses
-- the same number — no second advance, no gap); this partial UNIQUE surfaces any
-- double-allocation / recovery bug at INSERT/UPDATE time.
CREATE UNIQUE INDEX ux_fd_fn_zrn
  ON fiscal_documents(fiscal_number, z_report_number)
  WHERE z_report_number IS NOT NULL;


CREATE TABLE outbox (
    document_id            BLOB    NOT NULL CHECK (length(document_id) = 16)
        REFERENCES fiscal_documents(document_id) ON DELETE RESTRICT,
    fiscal_number          TEXT    NOT NULL,
    sequence_no            INTEGER NOT NULL CHECK (sequence_no >= 1),
                                                       -- = lnd at finalize time; monotonic per FN
    payload_sha256         BLOB    NOT NULL CHECK (length(payload_sha256) = 32),
                                                       -- canonical payload sha256 from
                                                       -- fiscal_documents.payload_sha256_canonical
    enqueued_at            TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    status                 TEXT    NOT NULL DEFAULT 'PENDING'
        CHECK (status IN ('PENDING', 'PUBLISHED')),
    published_at           TEXT,
    -- Status / published_at consistency: PENDING ⇒ published_at NULL;
    -- PUBLISHED ⇒ published_at NOT NULL.  Mirrors W7.1 transport_trace
    -- all-or-none completion CHECK.
    CHECK (
        (status = 'PENDING' AND published_at IS NULL)
        OR
        (status = 'PUBLISHED' AND published_at IS NOT NULL)
    ),
    PRIMARY KEY (document_id)
) STRICT;


CREATE INDEX ix_outbox_pending
  ON outbox(enqueued_at)
  WHERE status = 'PENDING';


CREATE TABLE offline_sessions (
    offline_session_id  BLOB    PRIMARY KEY  CHECK (length(offline_session_id) = 16),
    fiscal_number       TEXT    NOT NULL,
    state               TEXT    NOT NULL CHECK (state IN ('OPENING','OPEN','DRAINING','CLOSED','ABORTED')),
    opened_at           TEXT    NOT NULL,
    drained_at          TEXT,                       -- NEW (M3b W4): timestamp at DRAINING entry
    closed_at           TEXT,
    reason_abort        TEXT,                       -- NEW (M3b W4): rationale for ABORTED
    last_known_unsigned_xml_sha256 BLOB,            -- preserved from 004
    docs_count          INTEGER NOT NULL DEFAULT 0, -- preserved from 004
    created_at          TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at          TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;


CREATE UNIQUE INDEX ux_offline_active
    ON offline_sessions(fiscal_number)
    WHERE state IN ('OPENING','OPEN','DRAINING');


CREATE TABLE offline_codes (
    fiscal_number            TEXT    NOT NULL,
    code_lnd                 INTEGER NOT NULL  CHECK (code_lnd > 0),
    consumed_at              TEXT,
    consumed_by_document_id  BLOB,
    PRIMARY KEY (fiscal_number, code_lnd),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (consumed_by_document_id) REFERENCES fiscal_documents(document_id) ON DELETE RESTRICT
) STRICT;


CREATE INDEX ix_offline_codes_available
    ON offline_codes(fiscal_number, code_lnd)
    WHERE consumed_at IS NULL;


CREATE UNIQUE INDEX ux_offline_codes_consumed_by_doc
    ON offline_codes(consumed_by_document_id)
    WHERE consumed_by_document_id IS NOT NULL;


-- [from 015, M3b W4] DUR-1 immutability: once an offline code is consumed
-- (consumed_at set), its consumption binding cannot be rewritten — a consumed
-- code is single-use forever; admin repair must DROP + recreate the trigger.
CREATE TRIGGER offline_codes_consumed_immutable
    BEFORE UPDATE OF consumed_by_document_id, consumed_at ON offline_codes
    WHEN OLD.consumed_at IS NOT NULL
     AND (NEW.consumed_by_document_id IS NOT OLD.consumed_by_document_id
          OR NEW.consumed_at IS NOT OLD.consumed_at)
    BEGIN
        SELECT RAISE(ABORT, 'offline_codes consumed row is immutable; admin repair must DROP + recreate trigger');
    END;


-- [from 001/016, M3b W016] Shift lifecycle.  The 9-state `state` CHECK is the
-- M3b expansion (per docs/superpowers/specs/2026-05-17-m3b-shift-state-
-- expansion.md); the SAME 9-state vocabulary is mirrored on
-- `node_state.shift_state` (mirror invariant).  `opened_by_cashier_id`
-- (W14a-1, spec §16.8) is required for the 1-cashier-per-shift invariant;
-- `closed_by_cashier_id` (§16.9) supports senior-cashier close.
CREATE TABLE shifts (
    shift_id               BLOB    PRIMARY KEY  CHECK (length(shift_id) = 16),
    fiscal_number          TEXT    NOT NULL,
    serial                 INTEGER,
    state                  TEXT    NOT NULL  CHECK (state IN (
        'CREATED',
        'OPENING',
        'OPENED_LOCAL_PENDING_DRAIN',
        'OPENED',
        'CLOSING_LOCAL_PENDING_DRAIN',
        'CLOSING',
        'CLOSED',
        'REQUIRES_MANUAL_RECONCILIATION',
        'ERROR'
    )),
    open_mode              TEXT    NOT NULL  CHECK (open_mode IN ('ONLINE','OFFLINE')),
    opened_at              TEXT,
    closed_at              TEXT,
    open_document_id       BLOB,
    close_document_id      BLOB,
    z_report_document_id   BLOB,
    cash_balance_kop       INTEGER NOT NULL DEFAULT 0,
    opened_by_cashier_id   TEXT    NOT NULL,            -- NEW (W14a-1, spec §16.8 + §16.17): cashier identity required for 1-cashier-per-shift invariant; FK to cashier registry deferred to W14a-2 (registry table not yet in schema)
    closed_by_cashier_id   TEXT,                        -- NEW (W14a-1, spec §16.9): senior cashier close support; NULL = not yet closed OR same cashier as opener; populated via senior_cashier_close_shift_with_audit seam (W14a-2)
    created_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at             TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;


CREATE TRIGGER fd_updated_at
AFTER UPDATE ON fiscal_documents
BEGIN
    UPDATE fiscal_documents SET updated_at = CURRENT_TIMESTAMP WHERE document_id = NEW.document_id;
END;


CREATE TRIGGER shifts_updated_at
AFTER UPDATE ON shifts
BEGIN
    UPDATE shifts SET updated_at = CURRENT_TIMESTAMP WHERE shift_id = NEW.shift_id;
END;


CREATE INDEX ix_shifts_fn_state ON shifts(fiscal_number, state);


CREATE TABLE node_state (
    fiscal_number               TEXT    PRIMARY KEY,
    mode                        TEXT    NOT NULL  CHECK (mode IN ('ONLINE','GOING_OFFLINE','OFFLINE','GOING_ONLINE','BLOCKED','STOP_MODE','CRYPTO_DEGRADED')),
    shift_state                 TEXT    NOT NULL  CHECK (shift_state IN (
        'CREATED',
        'OPENING',
        'OPENED_LOCAL_PENDING_DRAIN',
        'OPENED',
        'CLOSING_LOCAL_PENDING_DRAIN',
        'CLOSING',
        'CLOSED',
        'REQUIRES_MANUAL_RECONCILIATION',
        'ERROR'
    )),
    current_shift_id            BLOB,
    current_offline_session_id  BLOB,
    next_lnd                    INTEGER NOT NULL  CHECK (next_lnd >= 1),
    backend_profile_id          TEXT,
    transport_profile_id        TEXT,
    readiness_state             TEXT    NOT NULL DEFAULT 'STARTING'  CHECK (readiness_state IN ('STARTING','RECOVERING','READY','DEGRADED','STOPPED')),
    recovery_stage              TEXT    NOT NULL DEFAULT 'BOOT'  CHECK (recovery_stage IN ('BOOT','PHASE1','PHASE2','DONE','FAILED')),
    current_month_offline_seconds INTEGER NOT NULL DEFAULT 0,
    last_known_unsigned_xml_sha256 BLOB  CHECK (last_known_unsigned_xml_sha256 IS NULL OR length(last_known_unsigned_xml_sha256) = 32),
    last_fs_ping_at             TEXT,
    updated_at                  TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    next_z_report_number        INTEGER NOT NULL DEFAULT 1  CHECK (next_z_report_number >= 1),  -- added by 009
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT
) STRICT;


CREATE TRIGGER node_state_updated_at
AFTER UPDATE ON node_state
BEGIN
    UPDATE node_state SET updated_at = CURRENT_TIMESTAMP WHERE fiscal_number = NEW.fiscal_number;
END;


CREATE UNIQUE INDEX ux_op_certs_ski_fn ON operator_certs(ski_hex, fiscal_number);


CREATE TABLE cashier_certs (
    cashier_id              TEXT    NOT NULL,
    fiscal_number           TEXT    NOT NULL,
    primary_cert_ski_hex    TEXT    NOT NULL  CHECK (length(primary_cert_ski_hex) = 64),
    deferred_cert_ski_hex   TEXT              CHECK (deferred_cert_ski_hex IS NULL OR length(deferred_cert_ski_hex) = 64),
    created_at              TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    updated_at              TEXT    NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    PRIMARY KEY (cashier_id, fiscal_number),
    -- Same-cert defence (PR #65 R2 H2): deferred MUST be physically
    -- different from primary; auto-swap of cert to itself = no-op trap.
    CHECK (deferred_cert_ski_hex IS NULL OR deferred_cert_ski_hex <> primary_cert_ski_hex),
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number) ON DELETE RESTRICT,
    -- Composite FK locks cert ownership to the binding FN — cross-FN
    -- cert binding is impossible at schema level (PR #65 R2 H2).
    FOREIGN KEY (primary_cert_ski_hex, fiscal_number)
        REFERENCES operator_certs(ski_hex, fiscal_number) ON DELETE RESTRICT,
    FOREIGN KEY (deferred_cert_ski_hex, fiscal_number)
        REFERENCES operator_certs(ski_hex, fiscal_number) ON DELETE RESTRICT
) STRICT;


CREATE INDEX ix_cashier_certs_fn ON cashier_certs(fiscal_number);


CREATE UNIQUE INDEX ux_cashier_certs_deferred_ski
    ON cashier_certs(deferred_cert_ski_hex) WHERE deferred_cert_ski_hex IS NOT NULL;


CREATE TRIGGER cashier_certs_updated_at
AFTER UPDATE ON cashier_certs
BEGIN
    UPDATE cashier_certs SET updated_at = CURRENT_TIMESTAMP
    WHERE cashier_id = NEW.cashier_id AND fiscal_number = NEW.fiscal_number;
END;


-- [from 010/012/019] W7 stage-4 send trace.  The two CHECKs are load-bearing:
-- (1) all-or-none completion (a row is either fully incomplete or fully
-- complete) and (2) OK ⇒ server_fiscal_no present.  `retry_class` (012) is the
-- durable routing-decision encoding; `SYSTEM_CRASH` outcome (019) is the
-- boot orphan-scanner close.  Name is quoted (`"transport_trace"`) — a 019
-- table rebuild artifact, preserved verbatim for sqlite_master equivalence.
CREATE TABLE "transport_trace" (
    document_id              BLOB    NOT NULL CHECK (length(document_id) = 16)
        REFERENCES fiscal_documents(document_id) ON DELETE CASCADE,
    attempt_no               INTEGER NOT NULL CHECK (attempt_no >= 1),
    started_at               TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    backend_profile_id       TEXT    NOT NULL,
    transport_profile_id     TEXT    NOT NULL,
    request_envelope_sha256  BLOB    NOT NULL
        CHECK (length(request_envelope_sha256) = 32),
    -- Completion columns — NULL until 4b UPDATE.
    completed_at             TEXT,
    wire_call_started_at     TEXT,
    wire_call_finished_at    TEXT,
    outcome_kind             TEXT
        CHECK (outcome_kind IS NULL
               OR outcome_kind IN (
                  'OK',
                  'REJECTED',
                  'RETRYABLE_TRANSPORT',
                  'RETRYABLE_SERVER',
                  'RETRYABLE_AUTH_FN',
                  'RETRYABLE_MAC_HASH_MISMATCH',
                  -- Phase 3 REC-3 addition (2026-05-24): orphan row
                  -- closed by boot scanner after crash mid-DPS-call.
                  'SYSTEM_CRASH'
               )),
    server_fiscal_no         TEXT,
    server_status_code       INTEGER,
    error_kind               TEXT,
    error_message            TEXT
        CHECK (error_message IS NULL OR length(error_message) <= 512),
    retry_class              TEXT,
    -- Self-consistency: a row is either incomplete or complete.
    -- Identical to migration 010.
    CHECK (
        (completed_at IS NULL
         AND wire_call_started_at IS NULL
         AND wire_call_finished_at IS NULL
         AND outcome_kind IS NULL)
        OR
        (completed_at IS NOT NULL
         AND wire_call_started_at IS NOT NULL
         AND wire_call_finished_at IS NOT NULL
         AND outcome_kind IS NOT NULL)
    ),
    -- OK ⇒ server_fiscal_no NOT NULL AND length > 0.  Identical to
    -- migration 010 (SYSTEM_CRASH does NOT require server_fiscal_no —
    -- по definition we don't know what server returned).
    CHECK (
        outcome_kind IS NULL
        OR outcome_kind != 'OK'
        OR (server_fiscal_no IS NOT NULL AND length(server_fiscal_no) > 0)
    ),
    PRIMARY KEY (document_id, attempt_no)
) STRICT;


CREATE INDEX ix_transport_trace_started ON transport_trace(started_at);


CREATE INDEX ix_transport_trace_unfinished
  ON transport_trace(document_id)
  WHERE completed_at IS NULL;


CREATE INDEX idx_transport_trace_doc_retry_class
  ON transport_trace(document_id, attempt_no DESC, retry_class);


-- [from 020] W4-Z2a append-only signing-config ledger.  Lives in the MAIN pool
-- (not secure) because the `fiscal_documents.signing_config_snapshot_id` FK only
-- works within the same DB.  `UNIQUE(fn, driver_id, payload_sha256)` dedups
-- identical snapshots.
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


CREATE INDEX idx_signing_config_snapshots_fn_driver
    ON signing_config_snapshots(fn, driver_id);


CREATE INDEX idx_fiscal_documents_signing_config_snapshot_id
    ON fiscal_documents(signing_config_snapshot_id)
    WHERE signing_config_snapshot_id IS NOT NULL;


-- [from 023] RS-3 C2 / WL-1: enforces the legal/state invariant "at most ONE
-- active shift per fiscal_number" at the DB level.  ACTIVE = the 6 non-terminal
-- M3b states below (CLOSED / REQUIRES_MANUAL_RECONCILIATION / ERROR excluded).
-- FAIL-CLOSED: on a DB with a pre-existing duplicate active shift this CREATE
-- fails loud → migration fails → boot fails; an operator must reconcile
-- manually (the gateway must NOT silently violate the invariant).
CREATE UNIQUE INDEX uq_active_shift_per_fiscal
    ON shifts(fiscal_number)
    WHERE state IN (
        'CREATED',
        'OPENING',
        'OPENED_LOCAL_PENDING_DRAIN',
        'OPENED',
        'CLOSING_LOCAL_PENDING_DRAIN',
        'CLOSING'
    );


