from __future__ import annotations

from pathlib import Path

from prro_gateway.config import AppConfig
from prro_gateway.runtime.container import RuntimeContainer
from prro_gateway.runtime.maria_shell import MariaIngressShell
from prro_gateway.runtime.xmlrpc_shell import XmlRpcIngressShell
from prro_gateway.services.ingress import IngressAcceptService

ROOT = Path(__file__).resolve().parents[1]


def _container(tmp_path: Path) -> RuntimeContainer:
    container = RuntimeContainer(AppConfig.from_mapping({
        'database': {
            'db_path': str(tmp_path / 'shells.sqlite3'),
            'sql_dir': str(ROOT / 'sql'),
            'auto_migrate': True,
        },
        'defaults': {
            'fiscal_number': 'FN-SHELLS',
            'backend_profile_id': 'backend_checkbox_default',
            'transport_profile_id': 'transport_checkbox_rest_default',
            'channel_owner': 'shell-tests',
        },
    }))
    container.initialize()
    return container


def test_xmlrpc_shell_accepts_call(tmp_path: Path) -> None:
    container = _container(tmp_path)
    shell = XmlRpcIngressShell(container)
    context = IngressAcceptService.build_context(
        fiscal_number='FN-SHELLS',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner='shell-tests',
    )
    result = shell.handle_call(
        method='SellCheck',
        params={'rows': [{'name': 'Tea', 'price': 2000, 'quantity': 1000}], 'payments': [{'type': 'CASH', 'amount': 2000}]},
        context=context,
    )
    assert result['ok'] is True
    assert result['status'] == 'NEW'


def test_maria_shell_accepts_envelope(tmp_path: Path) -> None:
    container = _container(tmp_path)
    shell = MariaIngressShell(container)
    context = IngressAcceptService.build_context(
        fiscal_number='FN-SHELLS',
        backend_profile_id='backend_checkbox_default',
        transport_profile_id='transport_checkbox_rest_default',
        channel_owner='shell-tests',
    )
    result = shell.handle_envelope(
        command_name='SALE',
        fields={'items': [{'name': 'Coffee', 'price': 3000, 'quantity': 1000}], 'payments': [{'type': 'CASH', 'amount': 3000}]},
        context=context,
    )
    assert result['ok'] is True
    assert result['status'] == 'NEW'
