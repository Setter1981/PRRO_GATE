import pytest
from prro_gateway.validators.ua_receipt import validate_sell_return_receipt

def test_receipt_with_discount_is_valid():
    # 1 item: 100.00 UAH
    # discount: 10% (10.00 UAH)
    # total to pay: 90.00 UAH
    payload = {
        "receipt": {
            "goods": [
                {
                    "price": 10000, 
                    "quantity": 1000, 
                    "sum": 10000,
                    "discounts": [
                        {"type": "DISCOUNT", "mode": "PERCENT", "value": 10}
                    ]
                }
            ],
            "payments": [
                {"type": "CASH", "amount": 9000}
            ],
            "totals": {
                "total_sum": 9000
            }
        }
    }
    errors = validate_sell_return_receipt(payload)
    assert not errors, f"Should be valid, got errors: {errors}"
