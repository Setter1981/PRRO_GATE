-- 040 — chain_seed_transitions: durable witness for NON-DOCUMENT seed advances
--
-- WHY
-- ---
-- A standalone T=112 offline-code replenish advances the online MAC-chain seed
-- (`node_state.last_known_unsigned_xml_sha256`) to `Hs = sha256(request_xml)` — a
-- NON-DOCUMENT seed: no `fiscal_documents` row carries `Hs`.  The shared ledger-walk
-- projection (`fiscal_documents::active_chain_tip_unsigned_xml_sha256`, consumed by
-- NC-03 boot MAC-seed reconstruction, the MacReseed guard-B tip, and the
-- invariant_scan oracle) therefore CANNOT recover `Hs`: after an NC-03 boot
-- (node_state row lost, ledger survives) it recovers the pre-replenish issued-doc
-- hash `Hp` (or genesis `None`) instead of `Hs`.  bd PRRO_GATE-hpc.
--
-- This table is the DURABLE seed-transition record the projection consults for the
-- non-document (T=112) case — the exact shape bd PRRO_GATE-2nk used for the
-- NotAcceptedOffline rewind (a same-tx marker + one projection fold consumed by all
-- three sites).  One row is appended per non-doc seed advance, inside the SAME
-- `with_immediate` envelope as the seed advance (atomicity is load-bearing: there is
-- no window where seed=Hs but no witness row, or vice versa).
--
-- ORDERING FRAME
-- --------------
-- `lnd_at_write` = the FN's `node_state.next_lnd` read inside the write tx = "the lnd
-- the NEXT document would take".  Because the replenish holds `acquire_fn_gate`
-- (single-writer per FN — Frozen invariant #2), no document can interleave the
-- read-of-next_lnd and the insert, so the witness sits in the SAME strictly-monotonic
-- per-FN frame as `fiscal_documents.lnd`.  The projection uses a STRICT `>` tie-break:
-- a witness with `lnd_at_write = k` beats a doc-tip whose producing doc had `lnd < k`,
-- but LOSES to a later doc that consumed lnd `k` (after-SELL case).
--
-- APPEND-ONLY
-- -----------
-- No UPDATE, no DELETE.  A later doc/advance simply appends a higher `lnd_at_write`.
-- A re-run replenish (RULING 2 §2: fresh DI/TS) computes a DIFFERENT `Hs` (different
-- request_xml) → a new row; byte-identical re-run is forbidden by RULING 2 anyway
-- (INV-4 preserved).
--
-- STRICT TABLE NOTE
-- -----------------
-- STRICT (mirror of 028/039): every column typed and NOT NULL except the defaulted
-- `created_at`.  The PRIMARY KEY (fiscal_number, created_at, new_seed) makes a
-- byte-identical re-append at the same second a no-op-by-conflict rather than a silent
-- duplicate, and keeps the table append-only-friendly without a surrogate rowid churn.
--
-- BACKWARD COMPATIBILITY
-- ----------------------
-- Additive, forward-only, NO backfill — no historical standalone-T112 rows exist
-- pre-pilot.  The new table is inert to every existing read; behavior is unchanged for
-- every FN that never ran a standalone replenish (empty table → the projection's
-- doc-only arms are identical to today).
--
-- ROLLBACK REASONING
-- ------------------
-- Forward: CREATE TABLE + CREATE INDEX (atomic in SQLite).
-- Reverse: the table becomes dead weight on a pre-pilot DB reset (mirror of 028/039
-- doctrine); a never-read table causes no correctness issue.
--
-- LIVE FILE SEQUENCE
-- ------------------
-- This file is 040; sqlx applies migrations by filename prefix order.

CREATE TABLE chain_seed_transitions (
    fiscal_number TEXT    NOT NULL,
    -- monotonic per-FN ordinal in the SAME frame as fiscal_documents.lnd:
    -- captured as the FN's current next_lnd at write time (see WHY / ORDERING FRAME).
    lnd_at_write  INTEGER NOT NULL,
    -- the non-doc seed this transition installed (32-byte sha256).
    new_seed      BLOB    NOT NULL,
    -- provenance discriminator; only 'T112' in this slice. Future: 'MACRESEED'.
    source        TEXT    NOT NULL,
    created_at    TEXT    NOT NULL DEFAULT (datetime('now')),
    -- append-only: one row per non-doc advance; a later doc/advance simply
    -- appends a higher lnd_at_write. No UPDATE, no DELETE.
    PRIMARY KEY (fiscal_number, created_at, new_seed)
) STRICT;

CREATE INDEX ix_chain_seed_transitions_fn_lnd
    ON chain_seed_transitions(fiscal_number, lnd_at_write DESC);
