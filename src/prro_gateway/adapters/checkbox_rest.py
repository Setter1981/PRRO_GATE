from __future__ import annotations

from typing import Any

from ..enums import AcquiringSource, OperationType, PaymentType, Protocol, ReceiptType
from .base import AdapterContext, CanonicalEnvelopeBuilder


class CheckboxRestAdapter:
    def map_command(self, raw_request: dict[str, Any]) -> Any:
        context = AdapterContext.model_validate(raw_request["context"])
        operation_type = OperationType(raw_request.get("operation", "SELL"))
        request = raw_request["request"]
        payload = self._build_payload(operation_type, request)
        external_request_id = request.get("external_request_id")
        requires_shift = operation_type in {
            OperationType.SELL,
            OperationType.RETURN,
            OperationType.SERVICE_IN,
            OperationType.SERVICE_OUT,
            OperationType.CASH_WITHDRAWAL,
            OperationType.X_REPORT,
            OperationType.Z_REPORT,
        }
        requires_offline_code = bool(request.get("offline", False))
        return CanonicalEnvelopeBuilder.build(
            context=context,
            protocol=Protocol.CHECKBOX_REST,
            operation_type=operation_type,
            payload=payload,
            external_request_id=external_request_id,
            requires_shift=requires_shift,
            requires_offline_code=requires_offline_code,
        )

    def _build_payload(self, operation_type: OperationType, request: dict[str, Any]) -> dict[str, Any]:
        goods = [self._map_good(idx, item) for idx, item in enumerate(request.get("goods", []), start=1)]
        payments = [self._map_payment(idx, item) for idx, item in enumerate(request.get("payments", []), start=1)]
        payload: dict[str, Any] = {
            "currency": request.get("currency", "UAH"),
            "cashier_id": request.get("cashier_id"),
            "goods_count": len(goods),
            "payments_count": len(payments),
            "receipt": {
                "type": self._receipt_type(operation_type),
                "goods": goods,
                "payments": payments,
                "related_receipt_id": request.get("related_receipt_id"),
                "previous_receipt_id": request.get("previous_receipt_id"),
                "technical_return": request.get("technical_return"),
                "delivery": request.get("delivery"),
                "rounding_enabled": request.get("rounding"),
                "totals": self._build_totals(request, goods, payments),
            },
        }
        if operation_type in {OperationType.SERVICE_IN, OperationType.SERVICE_OUT}:
            payload["service_sum"] = int(request.get("value", 0))
        if operation_type == OperationType.CASH_WITHDRAWAL:
            payload["cash_withdrawal_sum"] = sum(int(p.get("amount", 0)) for p in payments)
        if operation_type in {OperationType.SHIFT_OPEN, OperationType.SHIFT_CLOSE, OperationType.X_REPORT, OperationType.Z_REPORT, OperationType.GET_STATUS}:
            payload["receipt"]["goods"] = []
            payload["receipt"]["payments"] = []
            payload["goods_count"] = 0
            payload["payments_count"] = 0
        return payload

    @staticmethod
    def _receipt_type(operation_type: OperationType) -> str:
        return ReceiptType(operation_type.value).value if operation_type.value in ReceiptType._value2member_map_ else operation_type.value

    @staticmethod
    def _build_totals(request: dict[str, Any], goods: list[dict[str, Any]], payments: list[dict[str, Any]]) -> dict[str, Any]:
        totals = request.get("totals") or {}
        if isinstance(totals, dict) and totals:
            return totals
        goods_total = sum(int(item.get("sum", 0)) for item in goods)
        pay_total = sum(int(item.get("amount", 0)) for item in payments)
        return {
            "total_sum": goods_total or pay_total,
            "round_sum": request.get("round_sum"),
            "discounts_sum": request.get("discounts_sum"),
            "extra_charge_sum": request.get("extra_charge_sum"),
        }

    @staticmethod
    def _map_good(idx: int, item: dict[str, Any]) -> dict[str, Any]:
        good = item.get("good") or {}
        price = int(item.get("price", 0))
        quantity = int(item.get("quantity", 1000))
        amount = price * quantity // 1000
        return {
            "item_id": item.get("item_id") or f"item-{idx}",
            "item_no": int(item.get("item_no", idx)),
            "code": good.get("code") or item.get("code"),
            "good_id": good.get("id") or item.get("good_id"),
            "name": good.get("name") or item.get("name") or "UNKNOWN",
            "uktzed": good.get("uktzed") or item.get("uktzed"),
            "barcode": good.get("barcode") or item.get("barcode"),
            "excise_barcodes": list(good.get("excise_barcodes") or item.get("excise_barcodes") or []),
            "price": price,
            "quantity": quantity,
            "sum": amount,
            "header": item.get("header"),
            "footer": item.get("footer"),
            "discounts": list(item.get("discounts") or []),
            "item_attributes": item.get("item_attributes") or item.get("item_attributes_json"),
            "is_return": item.get("is_return"),
            "tax_id": good.get("tax_id") or item.get("tax_id"),
            "tax_id_2": good.get("tax_id_2") or item.get("tax_id_2"),
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


__all__ = ["CheckboxRestAdapter"]
