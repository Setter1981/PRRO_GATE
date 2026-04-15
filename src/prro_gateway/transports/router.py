from __future__ import annotations

import sqlite3
from typing import Protocol

from ..enums import TransportKind
from ..ports import PollResult, SendResult
from ..models.storage import TransportProfileRecord
from ..repositories.backend_profiles import TransportProfileRepository


class TransportHandler(Protocol):
    def send(
        self,
        *,
        document_id: str,
        signed_payload: str,
        fiscal_number: str,
        backend_profile_id: str,
        transport_profile_id: str,
        operation_type: str | None = None,
        request_payload: dict | None = None,
        request_payload_json: str | None = None,
        external_request_id: str | None = None,
        transport_profile: TransportProfileRecord | None = None,
    ) -> SendResult: ...

    def poll_status(
        self,
        *,
        document_id: str,
        fiscal_number: str,
        backend_profile_id: str,
        transport_profile_id: str,
        transport_request_id: str | None,
        operation_type: str | None = None,
        transport_profile: TransportProfileRecord | None = None,
    ) -> PollResult: ...


class ProfileAwareTransportRouter:
    def __init__(self, *, handlers: dict[TransportKind, TransportHandler], profiles: dict[str, TransportProfileRecord]) -> None:
        self.handlers = handlers
        self.profiles = profiles

    @classmethod
    def from_connection(cls, conn: sqlite3.Connection, *, handlers: dict[TransportKind, TransportHandler]) -> 'ProfileAwareTransportRouter':
        rows = conn.execute('SELECT transport_profile_id FROM transport_profiles WHERE is_active = 1').fetchall()
        profiles: dict[str, TransportProfileRecord] = {}
        for row in rows:
            profile_id = row[0]
            profile = TransportProfileRepository.get_by_id(conn, profile_id)
            if profile is not None:
                profiles[profile_id] = profile
        return cls(handlers=handlers, profiles=profiles)

    def _resolve(self, transport_profile_id: str) -> tuple[TransportHandler, TransportProfileRecord]:
        profile = self.profiles.get(transport_profile_id)
        if profile is None:
            raise LookupError(f'transport profile not found: {transport_profile_id}')
        kind = profile.kind
        handler = self.handlers.get(kind)
        if handler is None:
            raise LookupError(f'no handler for transport kind: {kind.value}')
        return handler, profile

    def send(self, **kwargs) -> SendResult:
        handler, profile = self._resolve(kwargs['transport_profile_id'])
        kwargs.setdefault('transport_profile', profile)
        return handler.send(**kwargs)

    def poll_status(self, **kwargs) -> PollResult:
        handler, profile = self._resolve(kwargs['transport_profile_id'])
        kwargs.setdefault('transport_profile', profile)
        return handler.poll_status(**kwargs)
