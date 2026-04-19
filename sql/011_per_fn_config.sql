CREATE TABLE fiscal_number_config (
    fiscal_number           TEXT PRIMARY KEY,
    enforce_blocked_mode    INTEGER NOT NULL DEFAULT 0
                                CHECK (enforce_blocked_mode IN (0, 1)),
    min_offline_codes       INTEGER NOT NULL DEFAULT 0
                                CHECK (min_offline_codes >= 0),
    max_offline_codes       INTEGER NOT NULL DEFAULT 0
                                CHECK (max_offline_codes >= 0
                                       AND max_offline_codes >= min_offline_codes),
    created_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
