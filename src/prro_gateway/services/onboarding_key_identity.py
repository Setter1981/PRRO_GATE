"""Extract org / operator identity from a Ukrainian X.509 cert.

Used by the admin-UI "Імпорт з ключа" flow (Phase 10) to pre-fill the
add-FN form from a signing key container.  Pure read-only: parses
cert metadata, never invokes DPS.

The cert DER parse delegates to `cert_provisioning._extract_name`
(hand-rolled ASN.1 walker, zero third-party deps).  The RDN-sequence
it produces is a flat string that this module post-processes to
surface Ukrainian identifiers — EDRPOU, DRFO — and human fields
(organization, operator).
"""
from __future__ import annotations

import re
from dataclasses import dataclass

from .cert_provisioning import _parse_cert_metadata  # internal but stable

# OIDs Ukrainian CAs populate inside subject RDN:
#   1.2.804.2.1.1.1.11.1.4.1.1  — КОД ЄДРПОУ (legal entity code, 8 dig)
#   1.2.804.2.1.1.1.11.1.4.2.1  — ДРФО / ІПН (individual, 10 dig)
#   1.2.804.2.1.1.1.11.1.4.11.1 — ДРФО alt (older certs; same semantics)
_OID_EDRPOU = "1.2.804.2.1.1.1.11.1.4.1.1"
_OID_DRFO_PRIMARY = "1.2.804.2.1.1.1.11.1.4.2.1"
_OID_DRFO_ALT = "1.2.804.2.1.1.1.11.1.4.11.1"

_EDRPOU_RE = re.compile(r"^\d{8}$")   # legal-entity code — always 8 digits
_DRFO_RE = re.compile(r"^\d{10}$")


@dataclass
class KeyIdentity:
    subject_dn: str | None
    org_name: str | None
    edrpou: str | None
    drfo: str | None
    operator_name: str | None
    is_legal_entity: bool


def _split_rdn(dn: str) -> list[tuple[str, str]]:
    """Split "CN=Foo, O=Bar, 1.2.3=42, C=UA" into [(label, value), ...].

    Honors double-quoted values: `"Person, Jr."` is one RDN entry.
    Strips whitespace around values.  Unknown input shapes yield empty
    list rather than raising — the caller never crashes on malformed
    cert metadata.
    """
    if not dn:
        return []
    out: list[tuple[str, str]] = []
    i = 0
    n = len(dn)
    while i < n:
        # Skip leading whitespace and leftover comma.
        while i < n and dn[i] in " ,":
            i += 1
        if i >= n:
            break
        # Read label up to '='.
        eq = dn.find("=", i)
        if eq == -1:
            break
        label = dn[i:eq].strip()
        i = eq + 1
        # Value: quoted or up to next unescaped comma at top level.
        if i < n and dn[i] == '"':
            i += 1
            start = i
            while i < n and dn[i] != '"':
                i += 1
            value = dn[start:i]
            if i < n:
                i += 1  # skip closing quote
        else:
            start = i
            while i < n and dn[i] != ",":
                i += 1
            value = dn[start:i].strip()
        out.append((label, value))
    return out


def extract_identity_from_subject_dn(dn: str | None) -> KeyIdentity:
    if dn is None:
        return KeyIdentity(subject_dn=None, org_name=None, edrpou=None,
                           drfo=None, operator_name=None,
                           is_legal_entity=False)
    if dn == "":
        return KeyIdentity(subject_dn="", org_name=None, edrpou=None,
                           drfo=None, operator_name=None,
                           is_legal_entity=False)

    edrpou: str | None = None
    drfo: str | None = None
    org_name: str | None = None
    operator_name: str | None = None

    for label, value in _split_rdn(dn):
        if not value:
            continue
        if label == _OID_EDRPOU:
            if _EDRPOU_RE.fullmatch(value):
                edrpou = value
        elif label in (_OID_DRFO_PRIMARY, _OID_DRFO_ALT):
            if _DRFO_RE.fullmatch(value):
                drfo = value
        elif label == "O":
            if org_name is None:
                org_name = value
        elif label == "CN":
            # Heuristic: CN with only letters/spaces looks like a person;
            # names starting with legal suffixes look like orgs.
            # We assign to `operator_name` always; if `O` is missing and
            # `CN` looks corporate, we promote it to `org_name` below.
            operator_name = value

    if org_name is None and operator_name:
        corporate_markers = ("ТОВ", "ФОП", "АТ", "ПАТ", "ДП", "LLC", "LTD", "Ltd")
        if any(m in operator_name for m in corporate_markers):
            org_name = operator_name
            operator_name = None

    return KeyIdentity(
        subject_dn=dn,
        org_name=org_name,
        edrpou=edrpou,
        drfo=drfo,
        operator_name=operator_name,
        is_legal_entity=bool(edrpou),
    )


def extract_identity_from_cert(cert_der: bytes) -> KeyIdentity:
    """Parse a DER-encoded X.509 and surface Ukrainian identifiers.

    Returns an empty `KeyIdentity` on any parse failure — this is a UI
    pre-fill helper, never load-bearing.
    """
    if not cert_der:
        return KeyIdentity(subject_dn=None, org_name=None, edrpou=None,
                           drfo=None, operator_name=None,
                           is_legal_entity=False)
    subject_dn, _issuer_dn, _nb, _na = _parse_cert_metadata(cert_der)
    return extract_identity_from_subject_dn(subject_dn)


__all__ = [
    "KeyIdentity",
    "extract_identity_from_cert",
    "extract_identity_from_subject_dn",
]
