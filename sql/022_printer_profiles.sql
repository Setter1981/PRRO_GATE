-- 022_printer_profiles.sql — Phase 13 Step 4a.
--
-- Operator-visible registration of receipt printers per fiscal_number.
-- One retail point usually has ONE thermal printer, but nothing in
-- this table enforces that; the operator picks at print time.
--
-- `profile_key` matches a bundled prro_escpos profile (tm-t88ii,
-- pp8000l, cts310ii) OR an operator-added XML file whose stem is the
-- key.  The daemon validates at print time; here we just store it.
--
-- `destination_type` = 'tcp' initially.  'serial' and 'usb' join the
-- CHECK list when Step 3 of the Rust daemon lands.

CREATE TABLE printer_profiles (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    name                TEXT    NOT NULL,
    fiscal_number       TEXT,                          -- optional binding
    profile_key         TEXT    NOT NULL,              -- prro_escpos profile key
    destination_type    TEXT    NOT NULL DEFAULT 'tcp'
        CHECK (destination_type IN ('tcp')),
    host                TEXT,                          -- for tcp
    port                INTEGER,                       -- for tcp
    paper_width_mm      INTEGER NOT NULL DEFAULT 80
        CHECK (paper_width_mm IN (58, 80, 112)),
    timeout_ms          INTEGER NOT NULL DEFAULT 5000
        CHECK (timeout_ms > 0 AND timeout_ms <= 60000),
    active              INTEGER NOT NULL DEFAULT 1
        CHECK (active IN (0, 1)),
    created_at          TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at          TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (fiscal_number) REFERENCES fiscal_number_config(fiscal_number)
);

CREATE INDEX ix_printer_profiles_active
    ON printer_profiles (active) WHERE active = 1;
CREATE INDEX ix_printer_profiles_fn
    ON printer_profiles (fiscal_number, active);
