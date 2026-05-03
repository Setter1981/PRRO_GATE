-- 003 — operators, certs, printer profiles, tax/payment defs.

CREATE TABLE sidecar_operators (
    id                BLOB    PRIMARY KEY  CHECK (length(id) = 16),
    fiscal_number     TEXT    NOT NULL,
    operator_name     TEXT,
    operator_inn      TEXT    NOT NULL  CHECK (length(operator_inn) = 10 AND operator_inn GLOB '[0-9]*'),
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

-- operator_certs: cert cache keyed by ski_hex (cert is uniquely identified
-- by its Subject Key Identifier).  At most one row per FN may carry
-- active=1 — enforced by a partial unique index, not by PK.  This supports
-- rolling refresh: stage a new cert (active=0), then flip in one tx.
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
