-- 005 — licenses (commercial tiering, kept per spec decision).

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

-- At most one active license at a time.
CREATE UNIQUE INDEX ux_lic_active ON licenses(active) WHERE active = 1;
