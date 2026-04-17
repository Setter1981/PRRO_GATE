-- Sprint 13 — certificate status monitoring cache.
--
-- Stores the most recently observed OCSP/CRL result per fiscal_number.
-- The cert_watch service reads this to avoid re-hitting the CA on every
-- poll tick and to block shift-open when the signing certificate is
-- revoked at the CA.
--
-- `status` values:
--   'good'        — CA confirmed cert is valid
--   'revoked'     — CA confirmed cert is revoked (hard block on shift-open)
--   'unknown'     — CA responded "unknown" (treated as unreachable; soft fail)
--   'unreachable' — OCSP + CRL both failed; operator must not be locked out
--
-- `source` records the path that produced this row:
--   'ocsp'           — live OCSP response
--   'crl'            — CRL fallback
--   'admin_refresh'  — operator-triggered one-shot refresh
--   'cached'         — reserved (explicit cache seed; not used by runtime today)
CREATE TABLE cert_status_cache (
    fiscal_number      TEXT PRIMARY KEY,
    cert_fingerprint   TEXT NOT NULL,
    status             TEXT NOT NULL CHECK (status IN ('good','revoked','unknown','unreachable')),
    revoked_at         TIMESTAMP,
    this_update        TIMESTAMP NOT NULL,
    next_update        TIMESTAMP,
    last_checked_at    TIMESTAMP NOT NULL,
    last_error         TEXT,
    source             TEXT NOT NULL CHECK (source IN ('ocsp','crl','admin_refresh','cached'))
);
