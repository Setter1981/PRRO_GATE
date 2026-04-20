from __future__ import annotations

from enum import StrEnum


class Protocol(StrEnum):
    CHECKBOX_REST = "CHECKBOX_REST"
    WEBCHECK_XMLRPC = "WEBCHECK_XMLRPC"
    MARIA_TCP = "MARIA_TCP"
    MARIA_304_NATIVE = "MARIA_304_NATIVE"
    INTERNAL = "INTERNAL"


class OperationType(StrEnum):
    SHIFT_OPEN = "SHIFT_OPEN"
    SHIFT_CLOSE = "SHIFT_CLOSE"
    SELL = "SELL"
    RETURN = "RETURN"
    SERVICE_IN = "SERVICE_IN"
    SERVICE_OUT = "SERVICE_OUT"
    CASH_WITHDRAWAL = "CASH_WITHDRAWAL"
    X_REPORT = "X_REPORT"
    Z_REPORT = "Z_REPORT"
    GO_OFFLINE = "GO_OFFLINE"
    GO_ONLINE = "GO_ONLINE"
    ASK_OFFLINE_CODES = "ASK_OFFLINE_CODES"
    GET_STATUS = "GET_STATUS"
    PING = "PING"
    CUSTOM_PROTOCOL_METHOD = "CUSTOM_PROTOCOL_METHOD"


class ReceiptType(StrEnum):
    SHIFT_OPEN = "SHIFT_OPEN"
    SHIFT_CLOSE = "SHIFT_CLOSE"
    SELL = "SELL"
    RETURN = "RETURN"
    SERVICE_IN = "SERVICE_IN"
    SERVICE_OUT = "SERVICE_OUT"
    CASH_WITHDRAWAL = "CASH_WITHDRAWAL"
    X_REPORT = "X_REPORT"
    Z_REPORT = "Z_REPORT"


class DocumentState(StrEnum):
    PREPARED = "PREPARED"
    SIGNED = "SIGNED"
    ENCRYPTED = "ENCRYPTED"
    SENT = "SENT"
    KVT1 = "KVT1"
    KVT2 = "KVT2"
    ACK = "ACK"
    OFFLINE_LOCAL_ACK = "OFFLINE_LOCAL_ACK"
    REJECTED = "REJECTED"
    CANCELLED = "CANCELLED"
    ERROR_RETRYABLE = "ERROR_RETRYABLE"
    REQUIRES_MANUAL_RECONCILIATION = "REQUIRES_MANUAL_RECONCILIATION"


class PaymentType(StrEnum):
    CASH = "CASH"
    CASHLESS = "CASHLESS"
    MIXED = "MIXED"
    OTHER = "OTHER"


class InboxStatus(StrEnum):
    NEW = "NEW"
    PROCESSING = "PROCESSING"
    DONE = "DONE"
    ERROR = "ERROR"
    DEAD = "DEAD"


class ShiftState(StrEnum):
    CREATED = "CREATED"
    OPENING = "OPENING"
    OPENED = "OPENED"
    CLOSING = "CLOSING"
    CLOSED = "CLOSED"
    ERROR = "ERROR"


class OfflineSessionState(StrEnum):
    # NOTE: OPENING and CLOSING are defined for schema compatibility but are intentionally
    # never written by the implementation. Sessions are created as OPEN and closed as CLOSED
    # atomically. Do not add writes for these values without also adding startup recovery.
    OPENING = "OPENING"
    OPEN = "OPEN"
    CLOSING = "CLOSING"
    CLOSED = "CLOSED"
    ABORTED = "ABORTED"


class NodeMode(StrEnum):
    ONLINE = "ONLINE"
    # NOTE: GOING_OFFLINE and GOING_ONLINE are defined for schema compatibility but are
    # intentionally never written by the implementation. The ONLINE→OFFLINE transition is
    # atomic (single commit). No transitional state is persisted. Do not add writes for
    # these values without also adding startup-supervisor recovery and fast-path guards.
    GOING_OFFLINE = "GOING_OFFLINE"
    OFFLINE = "OFFLINE"
    GOING_ONLINE = "GOING_ONLINE"
    BLOCKED = "BLOCKED"
    STOP_MODE = "STOP_MODE"
    CRYPTO_DEGRADED = "CRYPTO_DEGRADED"


class AcquiringSource(StrEnum):
    MANUAL_FRONT = "MANUAL_FRONT"
    ERP = "ERP"
    POS_INTEGRATION = "POS_INTEGRATION"
    BACKEND_ENRICHED = "BACKEND_ENRICHED"


class FileKind(StrEnum):
    REQUEST_JSON = "REQUEST_JSON"
    PAYLOAD_XML = "PAYLOAD_XML"
    SIGNED_XML = "SIGNED_XML"
    PROTO_REQUEST = "PROTO_REQUEST"
    PROTO_RESPONSE = "PROTO_RESPONSE"
    XML_CONTAINER = "XML_CONTAINER"
    KVT_1 = "KVT_1"
    KVT_2 = "KVT_2"
    RECEIPT_HTML = "RECEIPT_HTML"
    RECEIPT_TEXT = "RECEIPT_TEXT"
    RECEIPT_PNG = "RECEIPT_PNG"
    QRCODE = "QRCODE"
    ZIP_PACKAGE = "ZIP_PACKAGE"


class BackendType(StrEnum):
    CHECKBOX_CLOUD_COMPAT = "CHECKBOX_CLOUD_COMPAT"
    LOCAL_PRRO_CORE = "LOCAL_PRRO_CORE"
    DPS_DIRECT = "DPS_DIRECT"
    CUSTOM_VENDOR_BACKEND = "CUSTOM_VENDOR_BACKEND"


class TransportKind(StrEnum):
    CHECKBOX_REST_TRANSPORT = "CHECKBOX_REST_TRANSPORT"
    DPS_PRRO_GRPC_ECABINET = "DPS_PRRO_GRPC_ECABINET"
    DPS_PRRO_XML_UNIFIED_WINDOW = "DPS_PRRO_XML_UNIFIED_WINDOW"
    CUSTOM_TRANSPORT = "CUSTOM_TRANSPORT"
    DPS_PRRO_FISCAL_SIDECAR_V2 = "DPS_PRRO_FISCAL_SIDECAR_V2"


class CanonicalErrorCode(StrEnum):
    SHIFT_ALREADY_OPEN = "SHIFT_ALREADY_OPEN"
    SHIFT_NOT_OPEN = "SHIFT_NOT_OPEN"
    SHIFT_CHANNEL_SWITCH_FORBIDDEN = "SHIFT_CHANNEL_SWITCH_FORBIDDEN"
    OFFLINE_LIMIT_REACHED = "OFFLINE_LIMIT_REACHED"
    OFFLINE_CODES_EXHAUSTED = "OFFLINE_CODES_EXHAUSTED"
    OFFLINE_MODE_REQUIRED = "OFFLINE_MODE_REQUIRED"
    DUPLICATE_IDEMPOTENCY_KEY = "DUPLICATE_IDEMPOTENCY_KEY"
    DUPLICATE_EXCISE_MARK = "DUPLICATE_EXCISE_MARK"
    UNSUPPORTED_METHOD = "UNSUPPORTED_METHOD"
    UNKNOWN_METHOD = "UNKNOWN_METHOD"
    UNSUPPORTED_BY_BACKEND = "UNSUPPORTED_BY_BACKEND"
    INVALID_PAYMENT_DATA = "INVALID_PAYMENT_DATA"
    INVALID_ACQUIRING_DATA = "INVALID_ACQUIRING_DATA"
    INVALID_RECEIPT_SEQUENCE = "INVALID_RECEIPT_SEQUENCE"
    INVALID_FISCAL_DATE = "INVALID_FISCAL_DATE"
    BACKEND_TEMPORARY_UNAVAILABLE = "BACKEND_TEMPORARY_UNAVAILABLE"
    TRANSPORT_RETRYABLE_ERROR = "TRANSPORT_RETRYABLE_ERROR"
    CRYPTO_PROVIDER_UNAVAILABLE = "CRYPTO_PROVIDER_UNAVAILABLE"
    WORKER_BUSY_TIMEOUT = "WORKER_BUSY_TIMEOUT"
    OFFLINE_BACKLOG_NOT_SYNCED = "OFFLINE_BACKLOG_NOT_SYNCED"
    INVALID_RECEIPT_DATA = "INVALID_RECEIPT_DATA"
    RETURN_LINKAGE_REQUIRED = "RETURN_LINKAGE_REQUIRED"
    NODE_ALREADY_OFFLINE = "NODE_ALREADY_OFFLINE"
    NODE_NOT_OFFLINE = "NODE_NOT_OFFLINE"
    OFFLINE_RANGE_CONFLICT = "OFFLINE_RANGE_CONFLICT"
    STOP_MODE_ACTIVE = "STOP_MODE_ACTIVE"

    @property
    def retryable(self) -> bool:
        return self in {
            CanonicalErrorCode.OFFLINE_MODE_REQUIRED,
            CanonicalErrorCode.BACKEND_TEMPORARY_UNAVAILABLE,
            CanonicalErrorCode.TRANSPORT_RETRYABLE_ERROR,
            CanonicalErrorCode.CRYPTO_PROVIDER_UNAVAILABLE,
            CanonicalErrorCode.WORKER_BUSY_TIMEOUT,
        }

    @property
    def default_message(self) -> str:
        return _CANONICAL_ERROR_MESSAGES[self]


_CANONICAL_ERROR_MESSAGES = {
    CanonicalErrorCode.SHIFT_ALREADY_OPEN: "Shift is already open.",
    CanonicalErrorCode.SHIFT_NOT_OPEN: "Shift is not open.",
    CanonicalErrorCode.SHIFT_CHANNEL_SWITCH_FORBIDDEN: "Channel switch is forbidden while shift is active.",
    CanonicalErrorCode.OFFLINE_LIMIT_REACHED: "Offline mode time limit reached.",
    CanonicalErrorCode.OFFLINE_CODES_EXHAUSTED: "Offline codes exhausted.",
    CanonicalErrorCode.OFFLINE_MODE_REQUIRED: "Operation requires offline mode.",
    CanonicalErrorCode.DUPLICATE_IDEMPOTENCY_KEY: "Duplicate idempotency key.",
    CanonicalErrorCode.DUPLICATE_EXCISE_MARK: "Duplicate excise mark.",
    CanonicalErrorCode.UNSUPPORTED_METHOD: "Unsupported method.",
    CanonicalErrorCode.UNKNOWN_METHOD: "Unknown method.",
    CanonicalErrorCode.UNSUPPORTED_BY_BACKEND: "Operation is not supported by current backend.",
    CanonicalErrorCode.INVALID_PAYMENT_DATA: "Invalid payment data.",
    CanonicalErrorCode.INVALID_ACQUIRING_DATA: "Invalid acquiring data.",
    CanonicalErrorCode.INVALID_RECEIPT_SEQUENCE: "Invalid receipt sequence.",
    CanonicalErrorCode.INVALID_FISCAL_DATE: "Invalid fiscal date.",
    CanonicalErrorCode.BACKEND_TEMPORARY_UNAVAILABLE: "Backend is temporarily unavailable.",
    CanonicalErrorCode.TRANSPORT_RETRYABLE_ERROR: "Retryable transport error.",
    CanonicalErrorCode.CRYPTO_PROVIDER_UNAVAILABLE: "Crypto provider is unavailable.",
    CanonicalErrorCode.WORKER_BUSY_TIMEOUT: "Worker busy timeout.",
    CanonicalErrorCode.OFFLINE_BACKLOG_NOT_SYNCED: "Operation blocked: unsynced offline documents pending DPS submission.",
    CanonicalErrorCode.INVALID_RECEIPT_DATA: "Receipt data failed fiscal validation.",
    CanonicalErrorCode.RETURN_LINKAGE_REQUIRED: "RETURN requires related_receipt_id (original receipt reference). Missing linkage is a compliance risk.",
    CanonicalErrorCode.NODE_ALREADY_OFFLINE: "Node is already in OFFLINE mode.",
    CanonicalErrorCode.NODE_NOT_OFFLINE: "Node is not in OFFLINE mode.",
    CanonicalErrorCode.OFFLINE_RANGE_CONFLICT: "Offline range conflicts with an existing active range.",
    CanonicalErrorCode.STOP_MODE_ACTIVE: "Node is in STOP_MODE: DB integrity check failed. Contact operator.",
}


class HubErrorCode(StrEnum):
    INVALID_SCHEMA_VERSION = "INVALID_SCHEMA_VERSION"
    DUPLICATE_DOCUMENT = "DUPLICATE_DOCUMENT"
    UNKNOWN_FISCAL_NUMBER = "UNKNOWN_FISCAL_NUMBER"
    PAYLOAD_VALIDATION_FAILED = "PAYLOAD_VALIDATION_FAILED"
    HUB_INTERNAL_ERROR = "HUB_INTERNAL_ERROR"


__all__ = [
    'Protocol', 'OperationType', 'ReceiptType', 'DocumentState', 'PaymentType',
    'InboxStatus', 'ShiftState', 'OfflineSessionState', 'NodeMode', 'AcquiringSource',
    'FileKind', 'BackendType', 'TransportKind', 'CanonicalErrorCode', 'HubErrorCode'
]
