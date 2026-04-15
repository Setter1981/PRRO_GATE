from __future__ import annotations

from datetime import UTC, datetime

from ..enums import DocumentState
from ..ports import PollResult, SendResult


class CheckboxRestTransportStub:
    def send(self, *, document_id: str, signed_payload: str, fiscal_number: str, backend_profile_id: str, transport_profile_id: str, **kwargs) -> SendResult:
        now = datetime.now(UTC)
        return SendResult(
            state=DocumentState.ACK.value,
            transport_request_id=f'checkbox-{document_id}',
            submission_status='ACK',
            server_fiscal_no=f'CHK-{document_id}',
            server_fiscal_date=now.isoformat(),
            response_json=f'{{"profile":"checkbox","document_id":"{document_id}"}}',
            sent_at=now,
            ack_at=now,
        )

    def poll_status(self, *, document_id: str, fiscal_number: str, backend_profile_id: str, transport_profile_id: str, transport_request_id: str | None, **kwargs) -> PollResult:
        now = datetime.now(UTC)
        return PollResult(
            state=DocumentState.ACK.value,
            submission_status='ACK',
            server_fiscal_no=f'CHK-{document_id}',
            server_fiscal_date=now.isoformat(),
            response_json=f'{{"profile":"checkbox","document_id":"{document_id}"}}',
            ack_at=now,
        )


class DpsGrpcEcabinetTransportStub:
    def send(self, *, document_id: str, signed_payload: str, fiscal_number: str, backend_profile_id: str, transport_profile_id: str, **kwargs) -> SendResult:
        now = datetime.now(UTC)
        return SendResult(
            state=DocumentState.ACK.value,
            transport_request_id=f'grpc-{document_id}',
            submission_status='SENT_TO_DPS',
            response_json=f'{{"profile":"dps_grpc","document_id":"{document_id}","accepted":true}}',
            sent_at=now,
            ack_at=now,
            server_fiscal_no=f'GRPC-{document_id}',
            server_fiscal_date=now.isoformat(),
        )

    def poll_status(self, *, document_id: str, fiscal_number: str, backend_profile_id: str, transport_profile_id: str, transport_request_id: str | None, **kwargs) -> PollResult:
        now = datetime.now(UTC)
        return PollResult(
            state=DocumentState.ACK.value,
            submission_status='ACK_FROM_DPS',
            server_fiscal_no=f'GRPC-{document_id}',
            server_fiscal_date=now.isoformat(),
            response_json=f'{{"profile":"dps_grpc","document_id":"{document_id}","state":"ACK"}}',
            ack_at=now,
        )


class DpsXmlUnifiedWindowTransportStub:
    def __init__(self) -> None:
        self._poll_count: dict[str, int] = {}

    def send(self, *, document_id: str, signed_payload: str, fiscal_number: str, backend_profile_id: str, transport_profile_id: str, **kwargs) -> SendResult:
        now = datetime.now(UTC)
        req_id = f'xml-{document_id}'
        self._poll_count[req_id] = 0
        return SendResult(
            state=DocumentState.SENT.value,
            transport_request_id=req_id,
            submission_status='SUBMITTED_KVT_PENDING',
            response_json=f'{{"profile":"dps_xml","document_id":"{document_id}","state":"SUBMITTED"}}',
            sent_at=now,
            ack_at=now,
            server_fiscal_no=f'XML-{document_id}',
            server_fiscal_date=now.isoformat(),
        )

    def poll_status(self, *, document_id: str, fiscal_number: str, backend_profile_id: str, transport_profile_id: str, transport_request_id: str | None, **kwargs) -> PollResult:
        now = datetime.now(UTC)
        req_id = transport_request_id or f'xml-missing-{document_id}'
        count = self._poll_count.get(req_id, 0) + 1
        self._poll_count[req_id] = count
        if count == 1:
            return PollResult(
                state=DocumentState.SENT.value,
                submission_status='KVT1_RECEIVED',
                response_json=f'{{"profile":"dps_xml","document_id":"{document_id}","state":"KVT1"}}',
                kvt1_received_at=now,
            )
        return PollResult(
            state=DocumentState.ACK.value,
            submission_status='KVT2_ACK',
            server_fiscal_no=f'XML-{document_id}',
            server_fiscal_date=now.isoformat(),
            response_json=f'{{"profile":"dps_xml","document_id":"{document_id}","state":"ACK"}}',
            ack_at=now,
            kvt2_received_at=now,
        )


__all__ = ['CheckboxRestTransportStub', 'DpsGrpcEcabinetTransportStub', 'DpsXmlUnifiedWindowTransportStub']
