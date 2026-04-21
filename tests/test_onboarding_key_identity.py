"""Proof tests for Phase 10 — identity extraction from a Ukrainian X.509.

Two layers under test:

1. `extract_identity_from_subject_dn(dn_string)` — pure string parse.
   All common edge cases (missing fields, foreign OID-order, empty).

2. `extract_identity_from_cert(cert_der)` — wraps `_extract_name` for a
   minimal hand-crafted DER cert.  Also returns `KeyIdentity` with None
   fields on unparseable / non-UA certs.

The end-to-end route test lives in test_admin_ui_import_key.py —
mocks `prro_crypto.extract_private_key` so no real JKS is required.
"""
from __future__ import annotations

import pytest

from prro_gateway.services.onboarding_key_identity import (
    KeyIdentity,
    extract_identity_from_subject_dn,
    extract_identity_from_cert,
)


# ─── 1. Subject-DN string parser ─────────────────────────────────────


def test_empty_dn_returns_all_none() -> None:
    k = extract_identity_from_subject_dn("")
    assert k == KeyIdentity(subject_dn="", org_name=None, edrpou=None,
                            drfo=None, operator_name=None,
                            is_legal_entity=False)


def test_none_dn_returns_empty_identity() -> None:
    k = extract_identity_from_subject_dn(None)  # type: ignore[arg-type]
    assert k.subject_dn is None
    assert k.edrpou is None and k.drfo is None


def test_legal_entity_with_edrpou() -> None:
    dn = 'CN=ТОВ "Демо", O=ТОВ "Демо", 1.2.804.2.1.1.1.11.1.4.1.1=12345678, C=UA'
    k = extract_identity_from_subject_dn(dn)
    assert k.edrpou == "12345678"
    assert k.org_name == 'ТОВ "Демо"'
    assert k.drfo is None
    assert k.is_legal_entity is True


def test_individual_with_drfo() -> None:
    dn = "CN=Іванов Іван Іванович, 1.2.804.2.1.1.1.11.1.4.2.1=1234567890, C=UA"
    k = extract_identity_from_subject_dn(dn)
    assert k.drfo == "1234567890"
    assert k.edrpou is None
    assert k.operator_name == "Іванов Іван Іванович"
    assert k.is_legal_entity is False


def test_cashier_of_legal_entity_carries_both_ids() -> None:
    # Typical cashier cert: CN=person, O=legal entity, EDRPOU=legal, DRFO=person
    dn = (
        "CN=Петров П. П., O=ТОВ Демо, "
        "1.2.804.2.1.1.1.11.1.4.1.1=12345678, "
        "1.2.804.2.1.1.1.11.1.4.2.1=1234567890, C=UA"
    )
    k = extract_identity_from_subject_dn(dn)
    assert k.edrpou == "12345678"
    assert k.drfo == "1234567890"
    assert k.org_name == "ТОВ Демо"
    assert k.operator_name == "Петров П. П."
    assert k.is_legal_entity is True  # EDRPOU presence


def test_edrpou_must_be_digits_only() -> None:
    dn = "CN=x, 1.2.804.2.1.1.1.11.1.4.1.1=abc, C=UA"
    k = extract_identity_from_subject_dn(dn)
    assert k.edrpou is None


def test_drfo_must_be_digits_only() -> None:
    dn = "CN=x, 1.2.804.2.1.1.1.11.1.4.2.1=12-34, C=UA"
    k = extract_identity_from_subject_dn(dn)
    assert k.drfo is None


def test_alt_drfo_oid_11_1_is_also_recognised() -> None:
    # Some older Ukrainian CAs emit DRFO as 1.2.804.2.1.1.1.11.1.4.11.1.
    dn = "CN=Person, 1.2.804.2.1.1.1.11.1.4.11.1=1234567890, C=UA"
    k = extract_identity_from_subject_dn(dn)
    assert k.drfo == "1234567890"


def test_subject_dn_is_always_echoed() -> None:
    dn = "CN=Test, O=Test Org, C=UA"
    k = extract_identity_from_subject_dn(dn)
    assert k.subject_dn == dn


def test_strips_surrounding_whitespace_in_values() -> None:
    dn = "CN=  Іванов І.  , O= ТОВ Демо  , C=UA"
    k = extract_identity_from_subject_dn(dn)
    assert k.org_name == "ТОВ Демо"
    assert k.operator_name == "Іванов І."


def test_comma_inside_quoted_value_does_not_split() -> None:
    # RDN-renderer produces values with commas quoted — parser must honor.
    dn = 'CN="Person, Jr.", O=Firm, 1.2.804.2.1.1.1.11.1.4.1.1=12345678, C=UA'
    k = extract_identity_from_subject_dn(dn)
    assert k.operator_name == "Person, Jr."
    assert k.edrpou == "12345678"


# ─── 2. Cert-DER wrapper ─────────────────────────────────────────────


def test_cert_der_invalid_returns_none_fields() -> None:
    k = extract_identity_from_cert(b"\x00\x01\x02garbage")
    assert k.subject_dn is None
    assert k.edrpou is None and k.drfo is None


def test_cert_der_empty_returns_empty_identity() -> None:
    k = extract_identity_from_cert(b"")
    assert k.subject_dn is None


# ─── 3. Public types ─────────────────────────────────────────────────


def test_key_identity_is_dataclass_with_expected_fields() -> None:
    k = KeyIdentity(subject_dn=None, org_name=None, edrpou=None,
                    drfo=None, operator_name=None, is_legal_entity=False)
    assert hasattr(k, "subject_dn")
    assert hasattr(k, "edrpou")
    assert hasattr(k, "drfo")
    assert hasattr(k, "operator_name")
    assert hasattr(k, "org_name")
    assert hasattr(k, "is_legal_entity")
