from __future__ import annotations

from prro_gateway.enums import DocumentState, TransportKind
from prro_gateway.transports.router import ProfileAwareTransportRouter
from prro_gateway.transports.stubs import CheckboxRestTransportStub, DpsGrpcEcabinetTransportStub, DpsXmlUnifiedWindowTransportStub


def test_transport_stubs_router_dispatches_by_profile(conn) -> None:
    router = ProfileAwareTransportRouter.from_connection(conn, handlers={
        TransportKind.CHECKBOX_REST_TRANSPORT: CheckboxRestTransportStub(),
        TransportKind.DPS_PRRO_GRPC_ECABINET: DpsGrpcEcabinetTransportStub(),
        TransportKind.DPS_PRRO_XML_UNIFIED_WINDOW: DpsXmlUnifiedWindowTransportStub(),
    })
    r1 = router.send(document_id='doc-1', signed_payload='x', fiscal_number='FN-DEV-0001', backend_profile_id='backend_checkbox_default', transport_profile_id='transport_checkbox_rest_default')
    assert r1.submission_status == 'ACK'
    r2 = router.send(document_id='doc-2', signed_payload='x', fiscal_number='FN-DEV-0001', backend_profile_id='backend_dps_direct', transport_profile_id='transport_dps_grpc_default')
    assert r2.submission_status == 'SENT_TO_DPS'
    r3 = router.send(document_id='doc-3', signed_payload='x', fiscal_number='FN-DEV-0001', backend_profile_id='backend_dps_direct', transport_profile_id='transport_dps_xml_default')
    assert r3.submission_status == 'SUBMITTED_KVT_PENDING'


def test_dps_xml_stub_emits_kvt1_then_ack() -> None:
    stub = DpsXmlUnifiedWindowTransportStub()
    send = stub.send(document_id='doc-xml', signed_payload='x', fiscal_number='FN-DEV-0001', backend_profile_id='backend_dps_direct', transport_profile_id='transport_dps_xml_default')
    p1 = stub.poll_status(document_id='doc-xml', fiscal_number='FN-DEV-0001', backend_profile_id='backend_dps_direct', transport_profile_id='transport_dps_xml_default', transport_request_id=send.transport_request_id)
    p2 = stub.poll_status(document_id='doc-xml', fiscal_number='FN-DEV-0001', backend_profile_id='backend_dps_direct', transport_profile_id='transport_dps_xml_default', transport_request_id=send.transport_request_id)
    assert p1.submission_status == 'KVT1_RECEIVED'
    assert p1.state == DocumentState.SENT.value
    assert p2.submission_status == 'KVT2_ACK'
    assert p2.state == DocumentState.ACK.value
