from __future__ import annotations

from datetime import datetime

from pydantic import Field

from ..enums import (
    BackendType,
    DocumentState,
    FileKind,
    InboxStatus,
    NodeMode,
    OfflineSessionState,
    OperationType,
    Protocol,
    ShiftState,
    TransportKind,
)
from .common import StrictModel


class InboxRecord(StrictModel):
    request_id: str
    idempotency_key: str
    protocol: Protocol
    operation_type: OperationType
    fiscal_number: str
    route_key: str | None = None
    backend_profile_id: str | None = None
    transport_profile_id: str | None = None
    channel_owner: str | None = None
    protocol_session_id: str | None = None
    external_request_id: str | None = None
    payload_json: str
    payload_sha256: str
    status: InboxStatus
    requeue_count: int = Field(ge=0)
    lease_owner: str | None = None
    lease_token: str | None = None
    lease_expires_at: str | None = None
    response_deadline_at: str | None = None
    result_document_id: str | None = None
    canonical_error_code: str | None = None
    error_message: str | None = None
    created_at: datetime | None = None
    started_at: datetime | None = None
    finished_at: datetime | None = None


class FiscalDocumentRecord(StrictModel):
    document_id: str
    request_id: str
    fiscal_number: str
    shift_id: str | None = None
    offline_session_id: str | None = None
    lnd: int = Field(ge=1)
    doc_type: str
    state: DocumentState
    backend_profile_id: str
    transport_profile_id: str
    fs_mode: str
    receipt_type: str | None = None
    business_ts: str
    offline_fiscal_no: int | None = None
    offline_fiscal_date: str | None = None
    server_fiscal_no: str | None = None
    server_fiscal_date: str | None = None
    serial: str | None = None
    control_number: str | None = None
    previous_hash: str | None = None
    related_receipt_id: str | None = None
    previous_receipt_id: str | None = None
    technical_return: bool | None = None
    delivery_json: str | None = None
    rounding_enabled: bool | None = None
    channel_lock_ref: str | None = None
    total_sum: int | None = None
    round_sum: int | None = None
    discounts_sum: int | None = None
    extra_charge_sum: int | None = None
    payload_json: str
    payload_sha256: str
    response_json: str | None = None
    transport_request_id: str | None = None
    submission_status: str | None = None
    kvt1_received_at: str | None = None
    kvt2_received_at: str | None = None
    sent_at: str | None = None
    ack_at: str | None = None
    canonical_error_code: str | None = None
    error_message: str | None = None
    recovery_attempts: int = Field(default=0, ge=0)
    z_report_number: int | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None


class ChannelLock(StrictModel):
    backend_profile_id: str
    transport_profile_id: str
    protocol: Protocol
    integration_owner: str
    acquired_at: str


class ShiftRecord(StrictModel):
    shift_id: str
    fiscal_number: str
    serial: int | None = None
    state: ShiftState
    open_mode: str
    opened_at: str | None = None
    closed_at: str | None = None
    opened_via_backend_profile_id: str
    opened_via_transport_profile_id: str
    opened_via_protocol: Protocol
    opened_via_integration_owner: str
    channel_lock_acquired_at: str
    open_document_id: str | None = None
    close_document_id: str | None = None
    z_report_document_id: str | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None


class NodeStateRecord(StrictModel):
    node_id: str
    fiscal_number: str
    mode: NodeMode
    shift_state: ShiftState
    current_shift_id: str | None = None
    current_offline_session_id: str | None = None
    next_lnd: int = Field(ge=1)
    current_channel_lock: str | None = None
    current_backend_profile_id: str | None = None
    current_transport_profile_id: str | None = None
    current_integration_owner: str | None = None
    readiness_state: str
    recovery_stage: str
    current_month_bucket: str
    current_month_offline_seconds: int = Field(ge=0)
    crypto_state_json: str | None = None
    next_z_report_number: int = Field(default=1, ge=1)
    last_known_mac: str = ''
    excise_allowed: int = Field(default=0, ge=0, le=1)
    last_cash_balance: int = Field(default=0, ge=0)
    cash_balance_mode: str = 'preserve'
    rounding_enabled: int = Field(default=0, ge=0, le=1)
    print_zero_change: int = Field(default=0, ge=0, le=1)
    last_fs_ping_at: str | None = None
    last_integrity_check_at: str | None = None
    last_backup_at: str | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None


class OfflineRangeRecord(StrictModel):
    range_id: str
    fiscal_number: str
    first_fiscal_no: int
    last_fiscal_no: int
    next_fiscal_no: int
    issued_at: str
    status: str
    source_payload_json: str | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None


class OfflineSessionRecord(StrictModel):
    offline_session_id: str
    fiscal_number: str
    started_at: str
    ended_at: str | None = None
    status: OfflineSessionState
    start_notice_document_id: str | None = None
    finish_notice_document_id: str | None = None
    start_fiscal_no: int | None = None
    finish_fiscal_no: int | None = None
    reason: str | None = None
    continuous_limit_seconds: int = Field(ge=0)
    monthly_limit_seconds: int = Field(ge=0)
    accumulated_month_seconds: int = Field(ge=0)
    created_at: datetime | None = None
    updated_at: datetime | None = None


class BackendProfileRecord(StrictModel):
    backend_profile_id: str
    backend_type: BackendType
    name: str
    capability_flags_json: str
    config_json: str
    is_active: bool
    created_at: datetime | None = None
    updated_at: datetime | None = None


class TransportProfileRecord(StrictModel):
    transport_profile_id: str
    kind: TransportKind
    name: str
    endpoint: str | None = None
    tls_policy: str | None = None
    timeout_config_json: str
    retry_policy_json: str
    config_json: str
    is_active: bool
    created_at: datetime | None = None
    updated_at: datetime | None = None


class DocumentFileRecord(StrictModel):
    file_id: str
    document_id: str
    file_kind: FileKind
    path: str
    sha256: str
    size_bytes: int | None = None
    content: bytes | None = None
    created_at: datetime | None = None


class ExciseMarkRecord(StrictModel):
    excise_mark_id: str
    normalized_mark_code: str
    raw_mark_code: str
    mark_type: str | None = None
    status: str
    receipt_document_id: str | None = None
    receipt_item_no: int | None = None
    product_code: str | None = None
    sold_at: str | None = None
    returned_at: str | None = None
    voided_at: str | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None


class PaymentTypeDefRecord(StrictModel):
    fiscal_number: str
    type_code: str
    type_group: int = Field(ge=0, le=2)
    name: str
    is_active: int = Field(default=1, ge=0, le=1)
    created_at: str | None = None
    updated_at: str | None = None


class FiscalNumberConfigRecord(StrictModel):
    fiscal_number: str
    enforce_blocked_mode: int = Field(default=0, ge=0, le=1)
    min_offline_codes: int = Field(default=0, ge=0)
    max_offline_codes: int = Field(default=0, ge=0)
    created_at: str | None = None
    updated_at: str | None = None


class CertStatusCacheRecord(StrictModel):
    fiscal_number: str
    cert_fingerprint: str
    status: str  # 'good' | 'revoked' | 'unknown' | 'unreachable'
    revoked_at: str | None = None
    this_update: str
    next_update: str | None = None
    last_checked_at: str
    last_error: str | None = None
    source: str  # 'ocsp' | 'crl' | 'admin_refresh' | 'cached'


class CertWatchConfigRecord(StrictModel):
    id: int = Field(default=1, ge=1, le=1)
    enabled: int = Field(default=0, ge=0, le=1)
    polling_interval_seconds: int = Field(default=1800, ge=1)
    request_timeout_seconds: int = Field(default=5, ge=1)
    ocsp_url_override: str | None = None
    fallback_to_crl: int = Field(default=1, ge=0, le=1)
    updated_at: str | None = None


class OperatorCertRecord(StrictModel):
    """Resolved end-entity signing certificate for a PRRO fiscal_number.

    Populated by the cert_provisioning service at onboarding and kept
    fresh by `refresh_expiring_certs`. Consumed by the signing path.
    """
    fiscal_number: str
    cert_fingerprint: str
    ski_hex: str
    cert_der: bytes
    subject_dn: str | None = None
    issuer_dn: str | None = None
    valid_from: str | None = None
    valid_to: str | None = None
    fetched_at: str
    source: str  # 'container' | 'cmp' | 'manual'
    last_refresh_at: str | None = None


class CertProvisioningConfigRecord(StrictModel):
    id: int = Field(default=1, ge=1, le=1)
    primary_cmp_url: str = 'http://acskidd.gov.ua:80'
    fallback_cmp_url: str | None = None
    timeout_seconds: int = Field(default=10, ge=1)
    cache_ttl_seconds: int = Field(default=3600, ge=0)
    refresh_within_days: int = Field(default=30, ge=0)
    updated_at: str | None = None


class AuditLogRecord(StrictModel):
    audit_id: int
    created_at: datetime | None = None
    entity_type: str
    entity_id: str
    event_type: str
    severity: str
    event_payload_json: str | None = None


class ProtocolTraceRecord(StrictModel):
    trace_id: str
    created_at: datetime | None = None
    protocol: Protocol
    fiscal_number: str | None = None
    route_key: str | None = None
    connection_id: str | None = None
    session_id: str | None = None
    direction: str
    method_name: str | None = None
    command_code: str | None = None
    trace_level: str
    raw_payload: bytes | None = None
    raw_payload_hex: str | None = None
    parsed_payload_json: str | None = None
    mapped_operation: str | None = None
    canonical_request_id: str | None = None
    result_code: str | None = None
    error_class: str | None = None
    error_message: str | None = None
    duration_ms: int | None = None
    source_ip: str | None = None
    source_port: int | None = None


class CaEndpointRecord(StrictModel):
    id: int | None = None
    name: str
    cmp_url: str | None = None
    tsp_url: str | None = None
    ocsp_url: str | None = None
    issuer_pattern: str | None = None
    priority: int = 100
    enabled: int = 1
    created_at: datetime | None = None


class TransportTraceRecord(StrictModel):
    trace_id: str
    created_at: datetime | None = None
    fiscal_number: str | None = None
    backend_profile_id: str | None = None
    transport_profile_id: str | None = None
    endpoint: str | None = None
    direction: str
    request_envelope: str | None = None
    response_envelope: str | None = None
    technical_status: str | None = None
    business_status: str | None = None
    kvt_meta_json: str | None = None
    correlation_id: str | None = None
    retry_no: int = Field(ge=0)
    duration_ms: int | None = None


__all__ = [
    'InboxRecord', 'FiscalDocumentRecord', 'ChannelLock', 'ShiftRecord', 'NodeStateRecord',
    'OfflineRangeRecord', 'OfflineSessionRecord', 'BackendProfileRecord', 'TransportProfileRecord',
    'DocumentFileRecord', 'ExciseMarkRecord', 'PaymentTypeDefRecord', 'FiscalNumberConfigRecord',
    'CertStatusCacheRecord', 'CertWatchConfigRecord',
    'OperatorCertRecord', 'CertProvisioningConfigRecord', 'CaEndpointRecord',
    'AuditLogRecord', 'ProtocolTraceRecord', 'TransportTraceRecord',
]
