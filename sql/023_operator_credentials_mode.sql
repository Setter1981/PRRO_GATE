-- Migration 023: per-row credentials_mode for sidecar_operators
-- Allows each operator row to carry its own password encoding mode
-- instead of relying solely on the global config.security.credentials_mode.
ALTER TABLE sidecar_operators ADD COLUMN credentials_mode TEXT NOT NULL DEFAULT 'plain'
    CHECK (credentials_mode IN ('plain', 'xor_soft'));
