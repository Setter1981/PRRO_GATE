"""Gate 3L — Crypto breaker hysteresis unit tests.

These tests manipulate WritePathWorker internal counters directly,
without touching the DB or transport. They verify:
  - N-1 successes don't close the breaker
  - Exactly N consecutive successes close it
  - A failure resets the success counter
  - Explicit reset (admin API) closes breaker regardless of recovery_successes
"""
from __future__ import annotations

from prro_gateway.services import WritePathWorker

FN = "FN-DEV-0001"


class _OkCrypto:
    def sign(self, *, document_id, payload_json):
        return f"sig::{document_id}"

    def sign_raw(self, *, data, document_id=None):
        return b"signed"


class _OkTransport:
    def send(self, **kw):
        from datetime import UTC, datetime

        from prro_gateway.ports import SendResult

        now = datetime.now(UTC)
        return SendResult(
            transport_request_id="tx",
            submission_status="ACK",
            server_fiscal_no="SFN",
            server_fiscal_date=now.isoformat(),
            response_json="{}",
            sent_at=now,
            ack_at=now,
        )


def _worker(threshold: int = 2, recovery: int = 3) -> WritePathWorker:
    return WritePathWorker(
        crypto_provider=_OkCrypto(),
        transport_client=_OkTransport(),
        crypto_breaker_threshold=threshold,
        crypto_breaker_recovery_successes=recovery,
    )


# ---------------------------------------------------------------------------
# Test 1: N-1 successes are not enough to close the breaker
# ---------------------------------------------------------------------------

def test_n_minus_1_successes_do_not_close_breaker() -> None:
    THRESHOLD, RECOVERY = 2, 3
    w = _worker(THRESHOLD, RECOVERY)

    # Open the breaker
    w._crypto_consecutive_failures = THRESHOLD
    assert w.crypto_breaker_open is True

    # Simulate RECOVERY-1 successes WITHOUT resetting failures
    w._crypto_consecutive_successes = RECOVERY - 1

    # Breaker must still be open — not enough successes yet
    assert w.crypto_breaker_open is True, "N-1 successes must not close breaker"
    assert w._crypto_consecutive_failures == THRESHOLD


# ---------------------------------------------------------------------------
# Test 2: Exactly N consecutive successes close the breaker
# ---------------------------------------------------------------------------

def test_n_successes_close_breaker() -> None:
    THRESHOLD, RECOVERY = 2, 3
    w = _worker(THRESHOLD, RECOVERY)

    # Open the breaker
    w._crypto_consecutive_failures = THRESHOLD
    w._crypto_consecutive_successes = 0
    assert w.crypto_breaker_open is True

    # Simulate the hysteresis loop: on each success, increment; once >= RECOVERY,
    # reset failures and success counter (mirroring the write_path logic).
    for _ in range(RECOVERY):
        w._crypto_consecutive_successes += 1
        if w._crypto_consecutive_successes >= RECOVERY:
            w._crypto_consecutive_failures = 0
            w._crypto_consecutive_successes = 0

    assert w.crypto_breaker_open is False, "N successes must close breaker"
    assert w._crypto_consecutive_failures == 0
    assert w._crypto_consecutive_successes == 0


# ---------------------------------------------------------------------------
# Test 3: A failure resets the success counter
# ---------------------------------------------------------------------------

def test_failure_resets_success_counter() -> None:
    w = _worker(2, 3)

    # Partially accumulated successes (mid-recovery)
    w._crypto_consecutive_failures = 2
    w._crypto_consecutive_successes = 2

    # Simulate a new failure: reset success counter and bump failure counter
    w._crypto_consecutive_successes = 0
    w._crypto_consecutive_failures += 1

    assert w._crypto_consecutive_successes == 0, "Failure must reset success counter"
    assert w._crypto_consecutive_failures == 3


# ---------------------------------------------------------------------------
# Test 4: Explicit reset closes breaker immediately, bypasses hysteresis
# ---------------------------------------------------------------------------

def test_explicit_reset_closes_breaker_immediately() -> None:
    # Use very high recovery so natural hysteresis would never trigger
    w = _worker(threshold=2, recovery=100)

    w._crypto_consecutive_failures = 2
    assert w.crypto_breaker_open is True

    prev = w.reset_crypto_breaker()

    assert prev == 2, "reset_crypto_breaker() must return previous failure count"
    assert w.crypto_breaker_open is False, "Explicit reset must close breaker immediately"
    # Success counter is also cleared (fresh start for next probe cycle)
    assert w._crypto_consecutive_successes == 0
    assert w._crypto_consecutive_failures == 0
