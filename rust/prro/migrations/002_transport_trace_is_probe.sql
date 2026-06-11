-- 002 — transport_trace.is_probe discriminator (M1 legacy-review item 4, 2026-06-11).
--
-- The boot SENT-recovery probe (`dispatch_sent_via_probe`) and the drain KVT2
-- confirm probe (`kvt2_confirm`) each allocate a `transport_trace` row for a
-- READ-ONLY `last_chk` query; the real wire submit (`stage_send`) allocates one
-- for a `send_chk`.  Before this column the two were told apart only by the
-- IMPLICIT `request_envelope_sha256 == zeroblob(32)` sentinel — which the test
-- corpus itself violated (send rows seeded with a zero envelope), so it was NOT
-- a reliable discriminator.  M1-04: `attempts_used` (the ER-redrive send budget)
-- counted probe rows, prematurely exhausting `MAX_BOOT_ATTEMPTS`.
--
-- `is_probe` is now the AUTHORITATIVE probe/send discriminator: set `1` at the
-- two probe allocate-sites, `0` at the send site.  Existing rows default to `0`
-- (= send), preserving every prior budget count.  The zero-`request_envelope_
-- sha256` convention is left untouched for its other readers but is no longer
-- load-bearing for budget accounting.
ALTER TABLE transport_trace
  ADD COLUMN is_probe INTEGER NOT NULL DEFAULT 0 CHECK (is_probe IN (0, 1));
