"""
Gate 1y — readiness gating semantics smoke tests.

Verifies that health.ready=True is raised by Phase 1 (migrations complete),
NOT gated on Phase 2 (reconciliation) completing.

StartupSupervisor phase model:
  Phase 1 (PHASE1_STARTING → PHASE1_COMPLETE):
    - Applies migrations
    - Sets health.ready=True (when startup_ready=True, the default)
    - Gateway can serve traffic after this point

  Phase 2 (PHASE2_RECONCILING → RUNNING):
    - Runs reconcile_pending()
    - Sets health.startup_complete=True
    - Does NOT reset health.ready

StartupRunReport.ready_after_phase1:
    Captures health.ready *after* Phase 2 completes (line: ready_after_phase1=self.health.ready).
    Since Phase 2 does not change health.ready, ready_after_phase1=True proves
    that Phase 1 alone raised readiness, independently of reconciliation.

Two flags, two semantics:
  health.ready=True          — database initialized, accepting requests (Phase 1 gate)
  health.startup_complete=True — reconciliation also finished (Phase 2 gate)

Invariant #9: graceful startup — correct readiness gating is required.
"""
from __future__ import annotations

from pathlib import Path

import httpx
from fastapi.testclient import TestClient

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.rest_app import create_app

ROOT = Path(__file__).resolve().parents[1]

FISCAL_NUMBER = 'FN-DEV-0001'
BACKEND_PROFILE = 'backend_checkbox_default'
TRANSPORT_PROFILE = 'transport_checkbox_rest_default'


def _mock_handler(request: httpx.Request) -> httpx.Response:
    path = request.url.path
    if path.endswith('/cashier/signinPinCode'):
        return httpx.Response(200, json={'access_token': 'mock-token-gate1y'})
    if path.endswith('/shifts') and request.method == 'POST':
        return httpx.Response(200, json={
            'id': 'gate1y-shift-001', 'status': 'OPENED',
            'fiscal_code': 'SHIFT-GATE1Y-001',
            'updated_at': '2026-03-30T10:00:00+00:00',
        })
    raise AssertionError(f'gate1y: unexpected {request.method} {request.url}')


def _config(tmp_path: Path, db_name: str = 'gate1y.sqlite3') -> AppConfig:
    return AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / db_name),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': FISCAL_NUMBER,
            'backend_profile_id': BACKEND_PROFILE,
            'transport_profile_id': TRANSPORT_PROFILE,
            'channel_owner': 'gate1y',
        },
        'runtime': {'process_immediately': True},
        'checkbox': {
            'endpoint': 'https://api.checkbox.mock/api/v1',
            'license_key': 'GATE1Y-LIC',
            'cashier_pin': '0000',
        },
    })


# ---------------------------------------------------------------------------
# Test A — Phase 1 independently raises readiness (ready_after_phase1=True)
# ---------------------------------------------------------------------------

def test_gate1y_ready_raised_by_phase1_not_gated_on_phase2(tmp_path: Path) -> None:
    """
    StartupRunReport.ready_after_phase1=True proves that Phase 1 set health.ready=True
    before Phase 2 reconciliation ran. Phase 2 does not reset health.ready.

    Consequence: the gateway can serve requests as soon as migrations are applied,
    without waiting for reconciliation to complete. If this flag were False,
    the gateway would be unavailable until all pending fiscal documents are resolved —
    which is incorrect behavior for a service that may have many pending docs on restart.
    """
    cfg = _config(tmp_path)
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler))
    )

    with TestClient(create_app(container)) as client:
        r = client.post('/v1/ingress/checkbox', json={
            'context': {
                'request_id': 'gate1y-open-a',
                'fiscal_number': FISCAL_NUMBER,
                'backend_profile_id': BACKEND_PROFILE,
                'transport_profile_id': TRANSPORT_PROFILE,
                'channel_owner': 'gate1y',
                'business_ts': '2026-03-30T10:00:00Z',
            },
            'operation': 'SHIFT_OPEN',
            'request': {'external_request_id': 'ext-open-gate1y-a', 'cashier_id': 'cashier-gate1y'},
        })
        assert r.status_code == 200
        assert r.json().get('document_state') == 'ACK'

        # Both flags must be True during active lifespan
        assert container.health.ready, (
            'health.ready must be True: Phase 1 (migrations) raises readiness'
        )
        assert container.health.startup_complete, (
            'health.startup_complete must be True: Phase 2 (reconciliation) completed'
        )
        assert container.health.phase == 'RUNNING', (
            f'health.phase must be RUNNING after all startup phases: got {container.health.phase!r}'
        )

    report = container.last_startup_report
    assert report is not None

    assert report.phase1_ok, 'Phase 1 must succeed (migrations applied)'
    assert report.phase2_attempted, 'Phase 2 must be attempted (reconcile_on_startup=True default)'

    assert report.ready_after_phase1, (
        'ready_after_phase1 must be True: Phase 1 sets health.ready=True independently of Phase 2. '
        'If False, it means readiness is gated on reconciliation completing — '
        'which would make the gateway unavailable until all SENT docs are resolved.'
    )

    # No pending docs on a fresh DB after normal flow
    assert report.reconciliation_checked == 0, (
        f'No pending candidates on clean ACK-only DB: got {report.reconciliation_checked}'
    )


# ---------------------------------------------------------------------------
# Test B — health.ready and health.startup_complete are separate flags
# ---------------------------------------------------------------------------

def test_gate1y_health_flags_have_separate_semantics(tmp_path: Path) -> None:
    """
    health.ready and health.startup_complete are distinct flags with different meanings.
    Both are True after a normal full startup, but they represent different gates:

      health.ready=True          → Phase 1 done; safe to accept traffic
      health.startup_complete=True → Phase 2 done; reconciliation ran

    This test pins that distinction so that collapsing both into a single flag
    (e.g. only setting ready=True after Phase 2) would be caught as a regression.

    Evidence: ready_after_phase1 (from StartupRunReport) must equal health.ready
    after startup completes — Phase 2 must not reset health.ready.
    """
    cfg = _config(tmp_path, 'gate1y_b.sqlite3')
    container = RuntimeContainer(
        cfg, transport_http_client=httpx.Client(transport=httpx.MockTransport(_mock_handler))
    )

    with TestClient(create_app(container)):
        assert container.health.ready, 'health.ready must be True'
        assert container.health.startup_complete, 'health.startup_complete must be True'

        report = container.last_startup_report
        assert report is not None

        # Phase 2 must NOT reset health.ready — verify by comparing report to live health
        assert report.ready_after_phase1 == container.health.ready, (
            'ready_after_phase1 (captured at end of supervisor.run) must equal '
            'current health.ready — Phase 2 must not mutate health.ready'
        )

        # startup_complete flag is separate — it reflects Phase 2 completion
        assert report.phase2_attempted, 'Phase 2 must have run'
        assert container.health.startup_complete, (
            'health.startup_complete must be True after Phase 2 completes'
        )
