from __future__ import annotations

from typing import Any

from ..enums import AcquiringSource, OperationType, PaymentType, Protocol, ReceiptType
from .base import AdapterContext, AdapterMappingError, CanonicalEnvelopeBuilder

_COMMAND_TO_OPERATION = {
    "SALE": OperationType.SELL,
    "RETURN": OperationType.RETURN,
    "SERVICE_IN": OperationType.SERVICE_IN,
    "SERVICE_OUT": OperationType.SERVICE_OUT,
    "CASH_WITHDRAWAL": OperationType.CASH_WITHDRAWAL,
    "OPEN_SHIFT": OperationType.SHIFT_OPEN,
    "CLOSE_SHIFT": OperationType.SHIFT_CLOSE,
    "X_REPORT": OperationType.X_REPORT,
    "Z_REPORT": OperationType.Z_REPORT,
    "STATUS": OperationType.GET_STATUS,
}


class MariaTcpAdapter:
    def map_command(self, raw_request: dict[str, Any]) -> Any:
        context = AdapterContext.model_validate(raw_request["context"])
        command = raw_request["command"]
        fields = raw_request.get("fields", {})
        if command not in _COMMAND_TO_OPERATION:
            raise AdapterMappingError(f"Unsupported Maria command: {command}", code="UNSUPPORTED_METHOD")
        operation_type = _COMMAND_TO_OPERATION[command]
        payload = self._build_payload(command, operation_type, fields)
        external_request_id = fields.get("external_request_id") or fields.get("ticket_no")
        requires_shift = operation_type not in {OperationType.SHIFT_OPEN, OperationType.GET_STATUS}
        return CanonicalEnvelopeBuilder.build(
            context=context,
            protocol=Protocol.MARIA_TCP,
            operation_type=operation_type,
            payload=payload,
            external_request_id=external_request_id,
            requires_shift=requires_shift,
            requires_offline_code=False,
            idempotency_hint=f"{command.lower()}:{fields.get('ticket_no') or fields.get('receipt_no') or 'no-ref'}",
        )

    def _build_payload(self, command: str, operation_type: OperationType, fields: dict[str, Any]) -> dict[str, Any]:
        goods = [self._map_item(idx, row) for idx, row in enumerate(fields.get("items", []), start=1)]
        payments = [self._map_payment(idx, item) for idx, item in enumerate(fields.get("payments", []), start=1)]
        payload: dict[str, Any] = {
            "command": command,
            "currency": fields.get("currency", "UAH"),
            "cashier_id": fields.get("cashier_id"),
            "goods_count": len(goods),
            "payments_count": len(payments),
            "receipt": {
                "type": ReceiptType(operation_type.value).value if operation_type.value in ReceiptType._value2member_map_ else operation_type.value,
                "goods": goods,
                "payments": payments,
                "related_receipt_id": fields.get("related_receipt_id"),
                "previous_receipt_id": fields.get("previous_receipt_id"),
                "technical_return": fields.get("technical_return"),
                "delivery": fields.get("delivery"),
                "rounding_enabled": fields.get("rounding"),
                "totals": self._build_totals(fields, goods, payments),
            },
        }
        if operation_type in {OperationType.SHIFT_OPEN, OperationType.SHIFT_CLOSE, OperationType.X_REPORT, OperationType.Z_REPORT, OperationType.GET_STATUS}:
            payload["receipt"]["goods"] = []
            payload["receipt"]["payments"] = []
            payload["goods_count"] = 0
            payload["payments_count"] = 0
        if operation_type in {OperationType.SERVICE_IN, OperationType.SERVICE_OUT}:
            payload["service_sum"] = int(fields.get("value", 0))
        if operation_type == OperationType.CASH_WITHDRAWAL:
            payload["cash_withdrawal_sum"] = sum(int(p.get("amount", 0)) for p in payments)
        return payload

    @staticmethod
    def _build_totals(fields: dict[str, Any], goods: list[dict[str, Any]], payments: list[dict[str, Any]]) -> dict[str, Any]:
        totals = fields.get("totals") or {}
        if isinstance(totals, dict) and totals:
            return totals
        goods_total = sum(int(item.get("sum", 0)) for item in goods)
        pay_total = sum(int(item.get("amount", 0)) for item in payments)
        return {
            "total_sum": goods_total or pay_total,
            "round_sum": fields.get("round_sum"),
            "discounts_sum": fields.get("discounts_sum"),
            "extra_charge_sum": fields.get("extra_charge_sum"),
        }

    @staticmethod
    def _map_item(idx: int, row: dict[str, Any]) -> dict[str, Any]:
        price = int(row.get("price", 0))
        quantity = int(row.get("quantity", 1000))
        amount = int(row.get("sum", price * quantity // 1000))
        return {
            "item_id": row.get("item_id") or f"item-{idx}",
            "item_no": int(row.get("item_no", idx)),
            "code": row.get("code"),
            "good_id": row.get("good_id"),
            "name": row.get("name") or "UNKNOWN",
            "uktzed": row.get("uktzed"),
            "barcode": row.get("barcode"),
            "excise_barcodes": list(row.get("excise_barcodes") or []),
            "price": price,
            "quantity": quantity,
            "sum": amount,
            "header": row.get("header"),
            "footer": row.get("footer"),
            "discounts": list(row.get("discounts") or []),
            "item_attributes": row.get("item_attributes") or row.get("item_attributes_json"),
            "is_return": row.get("is_return"),
        }

    @staticmethod
    def _map_payment(idx: int, item: dict[str, Any]) -> dict[str, Any]:
        payment_type = PaymentType(item.get("type", "OTHER"))
        return {
            "payment_id": item.get("payment_id") or f"payment-{idx}",
            "payment_type": payment_type.value,
            "provider_type": item.get("provider_type"),
            "label": item.get("label"),
            "payment_code": item.get("code") or item.get("payment_code"),
            "amount": int(item.get("value", item.get("amount", 0))),
            "commission": item.get("commission"),
            "card_mask": item.get("card_mask"),
            "bank_name": item.get("bank_name"),
            "auth_code": item.get("auth_code"),
            "rrn": item.get("rrn"),
            "payment_system": item.get("payment_system"),
            "owner_name": item.get("owner_name"),
            "terminal": item.get("terminal"),
            "acquirer_and_seller": item.get("acquirer_and_seller"),
            "receipt_no": item.get("receipt_no"),
            "signature_required": item.get("signature_required"),
            "tapxphone_terminal": item.get("tapxphone_terminal"),
            "acquiring_source": item.get("acquiring_source") or (AcquiringSource.MANUAL_FRONT.value if payment_type == PaymentType.CASHLESS else None),
            "acquiring_payload_json": item.get("acquiring_payload_json"),
        }


__all__ = ["MariaTcpAdapter"]
