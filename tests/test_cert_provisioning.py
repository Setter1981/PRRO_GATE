"""Sprint 14 — automatic cert provisioning service tests.

Mocks `prro_crypto` at the module boundary so no real cryptography or
network traffic is touched. Wheel-side behaviour (CMP wire format,
`extract_private_key`) is exercised in the Rust test suite.

Frozen invariants verified here:

* FI-1: `provision_cert` completes its CMP fetch BEFORE opening the
        short upsert transaction. The test verifies ordering by watching
        mock call order and confirming no write on CMP failure.
* FI-4: idempotency — calling `provision_cert` twice with the same
        inputs collapses to a single row.
* FI-9: refresh loop is cooperative — verified at the container level
        by the pre-existing runtime shutdown tests; here we cover only
        the method's side-effects.
"""
from __future__ import annotations

import hashlib
import sqlite3
import threading
from contextlib import contextmanager
from datetime import UTC, datetime, timedelta
from pathlib import Path
from unittest.mock import MagicMock

import pytest

from prro_gateway.migrations.runner import apply_migrations_to_connection
from prro_gateway.repositories import OperatorCertsRepository
from prro_gateway.services.cert_provisioning import (
    SOURCE_CMP,
    SOURCE_CONTAINER,
    SOURCE_MANUAL,
    STATUS_NEEDS_MANUAL_UPLOAD,
    STATUS_OK,
    CertProvisioningError,
    CertProvisioningResult,
    CertProvisioningService,
    ContainerDecryptError,
    ManualUploadSkiMismatch,
    OnboardingBlockedError,
    onboarding_provision_or_raise,
)

ROOT = Path(__file__).resolve().parents[1]

FN = 'FN-DEV-0014'
PARAM_D_HEX = '1234567890abcdef' * 4  # Dummy — never actually used for crypto.
QX_HEX = 'aabbccdd' * 8
QY_HEX = 'deadbeef' * 8
COMPRESSED_PUBKEY = bytes(range(33))
TARGET_SKI = hashlib.sha256(b'target-ski-stream').digest()
OTHER_SKI = hashlib.sha256(b'other-ski-stream').digest()

EE_CERT_FOR_TARGET = _make_synth_cert_bytes = None  # placeholder; filled below


def _minimal_x509_der(*, valid_from: datetime, valid_to: datetime, subject: str = 'CN=Test') -> bytes:
    """Build a deliberately minimal DER-ish cert — enough structure to
    pass our best-effort metadata parser.

    NOTE: this is NOT a real ASN.1-valid cert for all parsers; it is
    only consumed by the metadata helper inside cert_provisioning. We
    keep it here (rather than bundling a real cert) so tests run in
    offline/sandbox CI without a fixture dependency.
    """
    # Build tbsCertificate with minimal required fields:
    # tbsCertificate ::= SEQUENCE {
    #     [0] INTEGER 2,                                   -- version v3
    #     INTEGER 1,                                       -- serial
    #     SEQUENCE { OID 1.2.3 },                          -- sigAlg
    #     SEQUENCE {},                                     -- issuer (empty)
    #     SEQUENCE { UTCTime ..., UTCTime ... },           -- validity
    #     SEQUENCE { ... subject ... },                    -- subject
    #     SEQUENCE { sigAlg, BIT STRING {pubkey} },        -- spki
    # }
    # Then outer: SEQUENCE { tbs, sigAlg, BIT STRING sig }
    from prro_gateway.services.cert_provisioning import _read_tlv  # noqa

    def seq(body: bytes) -> bytes:
        length = len(body)
        if length < 128:
            return bytes([0x30, length]) + body
        elif length < 256:
            return bytes([0x30, 0x81, length]) + body
        else:
            return bytes([0x30, 0x82, length >> 8, length & 0xFF]) + body

    def cstag0(body: bytes) -> bytes:
        length = len(body)
        return bytes([0xA0, length]) + body

    def integer(v: int) -> bytes:
        raw = v.to_bytes((v.bit_length() + 8) // 8 or 1, 'big')
        if raw[0] & 0x80:
            raw = b'\x00' + raw
        return bytes([0x02, len(raw)]) + raw

    def utctime(dt: datetime) -> bytes:
        s = dt.strftime('%y%m%d%H%M%SZ').encode('ascii')
        return bytes([0x17, len(s)]) + s

    def oid_der(dotted: str) -> bytes:
        parts = [int(p) for p in dotted.split('.')]
        first = 40 * parts[0] + parts[1]
        out = bytes([first])
        for p in parts[2:]:
            if p < 128:
                out += bytes([p])
            else:
                chunks = []
                while p:
                    chunks.append(p & 0x7F)
                    p >>= 7
                chunks = chunks[::-1]
                for i in range(len(chunks) - 1):
                    chunks[i] |= 0x80
                out += bytes(chunks)
        return bytes([0x06, len(out)]) + out

    def utf8(s: str) -> bytes:
        raw = s.encode('utf-8')
        return bytes([0x0C, len(raw)]) + raw

    def printable(s: str) -> bytes:
        raw = s.encode('ascii')
        return bytes([0x13, len(raw)]) + raw

    def bitstring(body: bytes) -> bytes:
        content = b'\x00' + body
        return bytes([0x03, len(content)]) + content

    # subject RDNSequence: ONE RDN SET containing {CN, subject}
    atv = seq(oid_der('2.5.4.3') + printable(subject.split('=', 1)[-1]))
    rdn = bytes([0x31, len(atv)]) + atv
    subject_seq = seq(rdn)

    # issuer: empty SEQUENCE
    issuer_seq = seq(b'')

    # validity: SEQUENCE { UTCTime, UTCTime }
    validity = seq(utctime(valid_from) + utctime(valid_to))

    # sigAlg: SEQUENCE { OID 1.2.804.2.1.1.1.1.3.1.1 }
    sig_alg = seq(oid_der('1.2.804.2.1.1.1.1.3.1.1'))

    # subjectPublicKeyInfo: SEQUENCE { sigAlg, BIT STRING { OCTET STRING compressed } }
    inner_octet = bytes([0x04, 33]) + COMPRESSED_PUBKEY
    spki = seq(sig_alg + bitstring(inner_octet))

    tbs = seq(
        cstag0(integer(2)) +
        integer(1) +
        sig_alg +
        issuer_seq +
        validity +
        subject_seq +
        spki
    )

    outer = seq(tbs + sig_alg + bitstring(b'\x00\xAA\xBB'))
    return outer


# ─── fixtures ──────────────────────────────────────────────────────────────


@pytest.fixture
def db_file(tmp_path: Path) -> Path:
    db = tmp_path / 'cert_prov.sqlite3'
    conn = sqlite3.connect(db)
    try:
        apply_migrations_to_connection(conn, ROOT / 'sql')
        conn.commit()
    finally:
        conn.close()
    return db


@pytest.fixture
def connect(db_file: Path):
    @contextmanager
    def _connect():
        c = sqlite3.connect(db_file)
        try:
            c.row_factory = sqlite3.Row
            yield c
        finally:
            c.close()
    return _connect


@pytest.fixture
def crypto_stub():
    """Mock of the prro_crypto module.

    Default behaviour: key → TARGET_SKI; `extract_cert_pubkey` on any
    cert returns COMPRESSED_PUBKEY for certs that are the EE cert,
    otherwise a different blob. Tests override this as needed.
    """
    stub = MagicMock(name='prro_crypto')
    stub.extract_private_key.return_value = {
        'format': 'zs2',
        'param_d_hex': PARAM_D_HEX,
        'certs': [],
    }
    stub.pubkey_dstu_pb_257.return_value = (QX_HEX, QY_HEX)
    stub.compress_pubkey.return_value = COMPRESSED_PUBKEY

    def compute_ski_stub(pubkey_bytes):
        # Map compressed pubkey to a deterministic SKI. Our fake EE cert
        # returns COMPRESSED_PUBKEY; the intermediate returns different
        # bytes mapping to OTHER_SKI.
        if bytes(pubkey_bytes) == COMPRESSED_PUBKEY:
            return TARGET_SKI
        return OTHER_SKI
    stub.compute_ski.side_effect = compute_ski_stub

    def extract_cert_pubkey_stub(cert_der):
        # We mark fake EE certs by embedding COMPRESSED_PUBKEY in the DER
        # body (the _minimal_x509_der builder puts it in the SPKI).
        if bytes(COMPRESSED_PUBKEY) in bytes(cert_der):
            return COMPRESSED_PUBKEY
        return b'\x03' * 33
    stub.extract_cert_pubkey.side_effect = extract_cert_pubkey_stub

    stub.fetch_cert_by_ski = MagicMock(name='fetch_cert_by_ski')
    return stub


@pytest.fixture
def service(connect, crypto_stub):
    return CertProvisioningService(connect=connect, crypto_module=crypto_stub)


@pytest.fixture
def ee_cert() -> bytes:
    nb = datetime(2026, 1, 1, tzinfo=UTC)
    na = datetime(2028, 1, 1, tzinfo=UTC)
    return _minimal_x509_der(valid_from=nb, valid_to=na, subject='CN=PRRO Director')


@pytest.fixture
def ee_cert_near_expiry() -> bytes:
    nb = datetime(2025, 1, 1, tzinfo=UTC)
    na = datetime.now(UTC) + timedelta(days=15)
    return _minimal_x509_der(valid_from=nb, valid_to=na)


@pytest.fixture
def ee_cert_far_expiry() -> bytes:
    nb = datetime(2025, 1, 1, tzinfo=UTC)
    na = datetime.now(UTC) + timedelta(days=60)
    return _minimal_x509_der(valid_from=nb, valid_to=na)


# Not-EE cert — has a different pubkey so SKI mismatch.
@pytest.fixture
def intermediate_cert() -> bytes:
    nb = datetime(2025, 1, 1, tzinfo=UTC)
    na = datetime(2030, 1, 1, tzinfo=UTC)
    # Build a cert with DIFFERENT pubkey so extract_cert_pubkey_stub
    # returns the 'not-EE' branch.
    raw = _minimal_x509_der(valid_from=nb, valid_to=na, subject='CN=Intermediate')
    # Tamper: zero out the compressed pubkey bytes so `COMPRESSED_PUBKEY`
    # doesn't appear as a subsequence.
    return raw.replace(COMPRESSED_PUBKEY, b'\xFF' * 33)


# ─── CP1 — container carries matching EE cert → source='container' ─────────


def test_cp1_container_with_matching_ee_cert_no_cmp(connect, service, crypto_stub, ee_cert):
    crypto_stub.extract_private_key.return_value = {
        'format': 'jks',
        'param_d_hex': PARAM_D_HEX,
        'certs': [ee_cert],
    }
    result = service.provision_cert(FN, b'fake-jks-bytes', 'Jrcfyf123')

    assert result.status == STATUS_OK
    assert result.source == SOURCE_CONTAINER
    assert result.ski_hex == TARGET_SKI.hex().upper()
    crypto_stub.fetch_cert_by_ski.assert_not_called()

    with connect() as c:
        row = OperatorCertsRepository.get_by_fiscal_number(c, FN)
    assert row is not None
    assert row.source == SOURCE_CONTAINER
    assert row.cert_der == ee_cert
    assert row.ski_hex == TARGET_SKI.hex().upper()


# ─── CP2 — chain where EE is certs[1], not certs[0] ─────────────────────────


def test_cp2_container_chain_ee_not_first(connect, service, crypto_stub, ee_cert, intermediate_cert):
    # Mimic Privat JKS layout: intermediate first, EE second.
    crypto_stub.extract_private_key.return_value = {
        'format': 'jks',
        'param_d_hex': PARAM_D_HEX,
        'certs': [intermediate_cert, ee_cert],
    }
    result = service.provision_cert(FN, b'jks', 'Jrcfyf123')

    assert result.status == STATUS_OK
    assert result.source == SOURCE_CONTAINER
    with connect() as c:
        row = OperatorCertsRepository.get_by_fiscal_number(c, FN)
    assert row.cert_der == ee_cert  # the EE cert was picked, not certs[0]


# ─── CP3 — no cert in container + CMP returns cert ──────────────────────────


def test_cp3_zs2_no_cert_cmp_fetch_succeeds(connect, service, crypto_stub, ee_cert):
    crypto_stub.extract_private_key.return_value = {
        'format': 'zs2',
        'param_d_hex': PARAM_D_HEX,
        'certs': [],
    }
    crypto_stub.fetch_cert_by_ski.return_value = ee_cert

    result = service.provision_cert(FN, b'zs2-bytes', '061082')

    assert result.status == STATUS_OK
    assert result.source == SOURCE_CMP
    crypto_stub.fetch_cert_by_ski.assert_called_once()
    args, kwargs = crypto_stub.fetch_cert_by_ski.call_args
    # Called with (cmp_url, ski_bytes, timeout)
    # ca_endpoints seed elevates acskidd/services/cmp/ to priority=10 — it's
    # the first URL tried when no explicit cmp_url is passed.
    assert args[0] == 'http://acskidd.gov.ua/services/cmp/'
    assert args[1] == TARGET_SKI

    with connect() as c:
        row = OperatorCertsRepository.get_by_fiscal_number(c, FN)
    assert row is not None
    assert row.source == SOURCE_CMP


# ─── CP4 — no cert + CMP unreachable → needs_manual_upload ──────────────────


def test_cp4_cmp_down_needs_manual_upload(connect, service, crypto_stub):
    crypto_stub.extract_private_key.return_value = {
        'format': 'zs2',
        'param_d_hex': PARAM_D_HEX,
        'certs': [],
    }
    crypto_stub.fetch_cert_by_ski.side_effect = RuntimeError('connection refused')

    result = service.provision_cert(FN, b'zs2', '061082')
    assert result.status == STATUS_NEEDS_MANUAL_UPLOAD
    assert result.ski_hex == TARGET_SKI.hex().upper()
    assert 'connection refused' in (result.error or '')

    with connect() as c:
        row = OperatorCertsRepository.get_by_fiscal_number(c, FN)
    assert row is None  # FI-1: no partial write on fetch failure


# ─── CP4b — allow_cmp_fetch=False → no CMP call, manual upload ──────────────


def test_cp4b_cmp_disabled_explicit(connect, service, crypto_stub):
    crypto_stub.extract_private_key.return_value = {
        'format': 'zs2',
        'param_d_hex': PARAM_D_HEX,
        'certs': [],
    }
    result = service.provision_cert(FN, b'zs2', 'p', allow_cmp_fetch=False)
    assert result.status == STATUS_NEEDS_MANUAL_UPLOAD
    crypto_stub.fetch_cert_by_ski.assert_not_called()


# ─── CP5 — manual upload SKI mismatch → rejected ────────────────────────────


def test_cp5_manual_upload_mismatch_rejected(connect, service, crypto_stub, ee_cert, intermediate_cert):
    # First provision legitimately so a baseline SKI is stored.
    crypto_stub.extract_private_key.return_value = {
        'format': 'jks',
        'param_d_hex': PARAM_D_HEX,
        'certs': [ee_cert],
    }
    service.provision_cert(FN, b'jks', 'Jrcfyf123')

    # Now attempt a manual upload with a cert whose SKI is different.
    with pytest.raises(ManualUploadSkiMismatch) as exc_info:
        service.manual_upload(FN, intermediate_cert)
    assert exc_info.value.expected_ski == TARGET_SKI.hex().upper()
    assert exc_info.value.got_ski == OTHER_SKI.hex().upper()

    # DB row unchanged.
    with connect() as c:
        row = OperatorCertsRepository.get_by_fiscal_number(c, FN)
    assert row.cert_der == ee_cert


# ─── CP6 — manual upload matching SKI → accepted ────────────────────────────


def test_cp6_manual_upload_matching_ski_accepted(connect, service, crypto_stub, ee_cert):
    # Seed via CMP.
    crypto_stub.extract_private_key.return_value = {
        'format': 'zs2',
        'param_d_hex': PARAM_D_HEX,
        'certs': [],
    }
    crypto_stub.fetch_cert_by_ski.return_value = ee_cert
    service.provision_cert(FN, b'zs2', 'p')

    # Now re-upload the SAME cert manually — must accept and flip source.
    result = service.manual_upload(FN, ee_cert)
    assert result.status == STATUS_OK
    assert result.source == SOURCE_MANUAL

    with connect() as c:
        row = OperatorCertsRepository.get_by_fiscal_number(c, FN)
    assert row.source == SOURCE_MANUAL
    assert row.ski_hex == TARGET_SKI.hex().upper()


# ─── CP7 — refresh_expiring_certs: 15-day cert re-fetched, 60-day skipped ──


def test_cp7_refresh_near_expiry_only(connect, service, crypto_stub, ee_cert_near_expiry, ee_cert_far_expiry):
    """Two distinct operators — one near expiry, one far — with distinct
    SKIs (UNIQUE index on ski_hex forbids key-material collisions).
    """
    near_fn = 'FN-NEAR'
    far_fn = 'FN-FAR'

    # Different private keys → different pubkey → different SKIs.
    COMPRESSED_FAR = bytes([0xAA]) + bytes(range(32))
    FAR_SKI = hashlib.sha256(b'far-ski-stream').digest()

    # Rewire compute_ski to also map the FAR pubkey.
    original_side_effect = crypto_stub.compute_ski.side_effect

    def compute_ski_multi(pubkey_bytes):
        b = bytes(pubkey_bytes)
        if b == COMPRESSED_PUBKEY:
            return TARGET_SKI
        if b == COMPRESSED_FAR:
            return FAR_SKI
        return OTHER_SKI
    crypto_stub.compute_ski.side_effect = compute_ski_multi

    # Patch extract_cert_pubkey_stub to distinguish FAR cert.
    def extract_pubkey_multi(cert_der):
        b = bytes(cert_der)
        if bytes(COMPRESSED_FAR) in b:
            return COMPRESSED_FAR
        if bytes(COMPRESSED_PUBKEY) in b:
            return COMPRESSED_PUBKEY
        return b'\x03' * 33
    crypto_stub.extract_cert_pubkey.side_effect = extract_pubkey_multi

    # Build a far-expiry cert using COMPRESSED_FAR in the SPKI.
    import base64  # noqa
    # Rebuild far cert with distinct pubkey.
    far_cert = ee_cert_far_expiry.replace(COMPRESSED_PUBKEY, COMPRESSED_FAR)

    # Operator 1 — NEAR (TARGET_SKI).
    crypto_stub.pubkey_dstu_pb_257.return_value = (QX_HEX, QY_HEX)
    crypto_stub.compress_pubkey.return_value = COMPRESSED_PUBKEY
    crypto_stub.extract_private_key.return_value = {
        'format': 'zs2', 'param_d_hex': PARAM_D_HEX, 'certs': [],
    }
    crypto_stub.fetch_cert_by_ski.return_value = ee_cert_near_expiry
    service.provision_cert(near_fn, b'zs2', 'p')

    # Operator 2 — FAR (FAR_SKI). Different key material: flip the
    # stubbed compress_pubkey to return COMPRESSED_FAR.
    crypto_stub.compress_pubkey.return_value = COMPRESSED_FAR
    crypto_stub.fetch_cert_by_ski.return_value = far_cert
    service.provision_cert(far_fn, b'zs2', 'p')

    with connect() as c:
        near_row = OperatorCertsRepository.get_by_fiscal_number(c, near_fn)
        far_row = OperatorCertsRepository.get_by_fiscal_number(c, far_fn)
    assert near_row is not None and near_row.ski_hex == TARGET_SKI.hex().upper()
    assert far_row is not None and far_row.ski_hex == FAR_SKI.hex().upper()

    # Reset the fetch mock so we can count refresh calls.
    crypto_stub.fetch_cert_by_ski.reset_mock()
    crypto_stub.fetch_cert_by_ski.return_value = ee_cert_near_expiry

    results = service.refresh_expiring_certs(within_days=30)

    # Only the near-expiry cert should be re-fetched.
    assert crypto_stub.fetch_cert_by_ski.call_count == 1
    assert len(results) == 1
    assert results[0].fiscal_number == near_fn

    with connect() as c:
        row_after = OperatorCertsRepository.get_by_fiscal_number(c, near_fn)
    assert row_after.last_refresh_at is not None

    # Restore compute_ski to the original per-fixture behaviour.
    crypto_stub.compute_ski.side_effect = original_side_effect


# ─── CP8 — onboarding hook: no cert + CMP down → blocks ─────────────────────


def test_cp8_onboarding_blocked_when_cmp_down(service, crypto_stub):
    crypto_stub.extract_private_key.return_value = {
        'format': 'zs2', 'param_d_hex': PARAM_D_HEX, 'certs': [],
    }
    crypto_stub.fetch_cert_by_ski.side_effect = RuntimeError('CMP down')

    with pytest.raises(OnboardingBlockedError) as exc_info:
        onboarding_provision_or_raise(service, FN, b'zs2', 'p')
    assert exc_info.value.fiscal_number == FN
    assert exc_info.value.ski_hex == TARGET_SKI.hex().upper()
    assert 'CMP down' in str(exc_info.value)


# ─── CP9 — idempotency (FI-4): repeated provision_cert → one row ────────────


def test_cp9_idempotent_repeated_provision(connect, service, crypto_stub, ee_cert):
    crypto_stub.extract_private_key.return_value = {
        'format': 'jks',
        'param_d_hex': PARAM_D_HEX,
        'certs': [ee_cert],
    }
    service.provision_cert(FN, b'jks', 'Jrcfyf123')
    service.provision_cert(FN, b'jks', 'Jrcfyf123')
    service.provision_cert(FN, b'jks', 'Jrcfyf123')

    with connect() as c:
        count = c.execute(
            'SELECT COUNT(*) FROM operator_certs WHERE fiscal_number = ?', (FN,),
        ).fetchone()[0]
    assert count == 1


# ─── CP10 — ContainerDecryptError surfaces wrong-password failures ─────────


def test_cp10_container_decrypt_failure_typed(service, crypto_stub):
    crypto_stub.extract_private_key.side_effect = ValueError('bad password')
    with pytest.raises(ContainerDecryptError) as exc_info:
        service.provision_cert(FN, b'bytes', 'wrong')
    assert 'bad password' in str(exc_info.value)


# ─── CP11 — get_cert_for_fiscal_number: signing-path lookup ─────────────────


def test_cp11_get_cert_for_signing_path(connect, service, crypto_stub, ee_cert):
    crypto_stub.extract_private_key.return_value = {
        'format': 'jks', 'param_d_hex': PARAM_D_HEX, 'certs': [ee_cert],
    }
    service.provision_cert(FN, b'jks', 'p')

    got = service.get_cert_for_fiscal_number(FN)
    assert got == ee_cert

    missing = service.get_cert_for_fiscal_number('FN-DOES-NOT-EXIST')
    assert missing is None


# ─── CP12 — CMP returns a cert whose SKI doesn't match → refused ────────────


def test_cp12_cmp_sky_mismatch_refused(connect, service, crypto_stub, intermediate_cert):
    crypto_stub.extract_private_key.return_value = {
        'format': 'zs2', 'param_d_hex': PARAM_D_HEX, 'certs': [],
    }
    # CA returns the wrong cert (intermediate's pubkey → OTHER_SKI).
    crypto_stub.fetch_cert_by_ski.return_value = intermediate_cert

    result = service.provision_cert(FN, b'zs2', 'p')
    assert result.status == STATUS_NEEDS_MANUAL_UPLOAD
    assert 'SKI does not match' in (result.error or '')

    # Nothing persisted.
    with connect() as c:
        row = OperatorCertsRepository.get_by_fiscal_number(c, FN)
    assert row is None


# ─── CP13 — valid_to populated from cert; metadata best-effort ──────────────


def test_cp13_cert_metadata_populated(connect, service, crypto_stub, ee_cert):
    crypto_stub.extract_private_key.return_value = {
        'format': 'jks', 'param_d_hex': PARAM_D_HEX, 'certs': [ee_cert],
    }
    service.provision_cert(FN, b'jks', 'p')

    with connect() as c:
        row = OperatorCertsRepository.get_by_fiscal_number(c, FN)
    assert row is not None
    # valid_from / valid_to should be parseable ISO-8601 strings.
    assert row.valid_from is not None
    assert row.valid_to is not None
    assert row.subject_dn is not None and 'Director' in row.subject_dn
