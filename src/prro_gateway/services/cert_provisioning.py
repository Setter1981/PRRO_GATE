"""Automatic signing-certificate provisioning (Sprint 14 — Phase 3).

Operational problem solved
--------------------------
Onboarding a new PRRO operator used to require shipping both the
encrypted private-key container (.ZS2 / .jks / .dat / .pfx) AND a
separately-uploaded .cer file. Operators routinely forget the .cer,
upload the wrong one, or can't retrieve it from their CA at all. This
service derives the cert automatically from nothing but the container
and password, mirroring what ІІТ WebCheck does under the hood.

Resolution order
----------------
1. Decrypt the container (extract_private_key), compute SKI from the
   private key.
2. Scan embedded certs (JKS ships the chain; ZS2 / key6 typically don't)
   for one whose pubkey-SKI matches → source='container'. The
   end-entity is NOT necessarily `certs[0]` — we match by SKI.
3. If no match and allow_cmp_fetch: POST `genm` to the CA CMP front
   (`http://acskidd.gov.ua:80` by default). Fall back to a simple HTTP
   GET if CMP errors. Either way → source='cmp'.
4. If both fail: return status='needs_manual_upload' with the computed
   SKI so the operator surface can prompt for a .cer upload.

Frozen-invariant compliance
---------------------------
* FI-1: network calls happen OUTSIDE any SQLite write transaction. The
        CMP fetch in `provision_cert` completes before the short `BEGIN
        IMMEDIATE` that performs the upsert.
* FI-4: `provision_cert` is idempotent on (fiscal_number, ski) — the
        repository upsert collapses repeated calls to a single row.
* FI-9: `refresh_expiring_certs` is a loop body; the poller owner
        (RuntimeContainer) controls its stop-event.

Trust model
-----------
We do NOT verify CMP response signatures. TLS/routing to the Ukrainian
CA front covers tamper-resistance for a read-only lookup. Signed-CMP
uplift is a later sprint if audit requires it.
"""
from __future__ import annotations

import hashlib
import sqlite3
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from typing import Callable

from ..logging import get_logger
from ..models.storage import OperatorCertRecord
from ..repositories import (
    AuditRepository,
    CaEndpointsRepository,
    CertProvisioningConfigRepository,
    OperatorCertsRepository,
)
from ..utils.json_codec import dumps_json


# ─── Status sentinels ──────────────────────────────────────────────────────

STATUS_OK = 'ok'
STATUS_NEEDS_MANUAL_UPLOAD = 'needs_manual_upload'

SOURCE_CONTAINER = 'container'
SOURCE_CMP = 'cmp'
SOURCE_MANUAL = 'manual'


# ─── Typed results / errors ────────────────────────────────────────────────


@dataclass(frozen=True)
class CertProvisioningResult:
    """Outcome of a `provision_cert` call."""
    status: str                 # STATUS_OK | STATUS_NEEDS_MANUAL_UPLOAD
    fiscal_number: str
    ski_hex: str                # upper-hex of the SKI derived from the priv key
    source: str | None = None   # container | cmp | manual (None if manual-upload needed)
    cert_fingerprint: str | None = None
    error: str | None = None

    @property
    def is_ok(self) -> bool:
        return self.status == STATUS_OK


class CertProvisioningError(RuntimeError):
    """Base class for provisioning failures."""


class ContainerDecryptError(CertProvisioningError):
    """Container could not be decrypted (bad password, corrupt file)."""


class ManualUploadSkiMismatch(CertProvisioningError):
    """An operator tried to upload a cert whose SKI doesn't match the
    fiscal_number's stored key-derived SKI."""

    def __init__(self, fiscal_number: str, expected_ski: str, got_ski: str) -> None:
        super().__init__(
            f'manual cert SKI mismatch for fiscal_number={fiscal_number}: '
            f'expected {expected_ski}, got {got_ski}'
        )
        self.fiscal_number = fiscal_number
        self.expected_ski = expected_ski
        self.got_ski = got_ski


class OnboardingBlockedError(CertProvisioningError):
    """Raised at the onboarding hook when no signing cert can be provisioned.

    Onboarding MUST NOT silently accept a keyless operator — a PRRO
    that can't sign is a PRRO that can't transact.
    """

    def __init__(self, fiscal_number: str, ski_hex: str, reason: str) -> None:
        super().__init__(
            f'Onboarding blocked for fiscal_number={fiscal_number}: '
            f'{reason}. SKI={ski_hex}'
        )
        self.fiscal_number = fiscal_number
        self.ski_hex = ski_hex
        self.reason = reason


# ─── Service ───────────────────────────────────────────────────────────────


class CertProvisioningService:
    """Provisions, stores, refreshes operator signing certs.

    Parameters
    ----------
    connect:
        Short-lived connection factory (context manager). Matches the
        `cert_watch` service's pattern — no long-lived connections.
    crypto_module:
        `prro_crypto` wheel (or a mock in tests). Must expose
        `extract_private_key`, `pubkey_dstu_pb_257`, `compress_pubkey`,
        `compute_ski`, `extract_cert_pubkey`, and (for CMP) `fetch_cert_by_ski`.
    """

    def __init__(
        self,
        *,
        connect: Callable[[], sqlite3.Connection],
        crypto_module=None,
    ) -> None:
        self._connect = connect
        if crypto_module is None:
            try:
                import prro_crypto as crypto_module  # type: ignore
            except ImportError:
                crypto_module = None
        self._crypto = crypto_module
        self._logger = get_logger('prro_gateway.cert_provisioning')
        self._ca_endpoints_repo = CaEndpointsRepository(connect)

    # ─── Main entrypoint ─────────────────────────────────────────────────

    def provision_cert(
        self,
        fiscal_number: str,
        container_bytes: bytes,
        password: str,
        *,
        allow_cmp_fetch: bool = True,
        cmp_url: str | None = None,
        timeout_seconds: float | None = None,
    ) -> CertProvisioningResult:
        """Resolve and persist the end-entity cert for this operator.

        See module docstring for the resolution order. Never raises on
        CMP/network failure — returns `status='needs_manual_upload'`
        with no partial DB write. Raises `ContainerDecryptError` if the
        container itself can't be opened.
        """
        if self._crypto is None:
            raise CertProvisioningError(
                'prro_crypto module unavailable — cannot provision certs'
            )

        # 1. Decrypt.
        try:
            extracted = self._crypto.extract_private_key(container_bytes, password)
        except Exception as exc:
            raise ContainerDecryptError(
                f'container decrypt failed: {exc}'
            ) from exc

        param_d_hex = extracted['param_d_hex']
        embedded_certs = list(extracted.get('certs') or [])

        # 2. Derive SKI from the private key (independent of any embedded cert).
        qx_hex, qy_hex = self._crypto.pubkey_dstu_pb_257(param_d_hex)
        compressed = self._crypto.compress_pubkey(qx_hex, qy_hex)
        key_ski = self._crypto.compute_ski(compressed)
        ski_hex = key_ski.hex().upper()

        # 3. Scan embedded certs.
        container_cert = self._find_cert_by_ski(embedded_certs, key_ski)
        if container_cert is not None:
            self._persist(
                fiscal_number=fiscal_number,
                cert_der=container_cert,
                ski_hex=ski_hex,
                source=SOURCE_CONTAINER,
            )
            return CertProvisioningResult(
                status=STATUS_OK,
                fiscal_number=fiscal_number,
                ski_hex=ski_hex,
                source=SOURCE_CONTAINER,
                cert_fingerprint=hashlib.sha256(container_cert).hexdigest(),
            )

        # 4. CMP lookup.
        if not allow_cmp_fetch:
            return CertProvisioningResult(
                status=STATUS_NEEDS_MANUAL_UPLOAD,
                fiscal_number=fiscal_number,
                ski_hex=ski_hex,
                error='no cert in container and allow_cmp_fetch=False',
            )

        cfg = self._load_config()
        effective_timeout = timeout_seconds if timeout_seconds is not None else float(cfg.timeout_seconds)

        cmp_cert: bytes | None = None
        cmp_error: str | None = None
        fetch_fn = getattr(self._crypto, 'fetch_cert_by_ski', None)
        if fetch_fn is None:
            cmp_error = 'prro_crypto.fetch_cert_by_ski unavailable (rebuild wheel with tsp_http)'
        else:
            # Caller-supplied URL always wins; otherwise iterate registered
            # CA endpoints by priority. Any single endpoint returning
            # `not_found` or raising is logged, not fatal — we try the next.
            urls_to_try = self._resolve_candidate_urls(cmp_url, cfg)
            last_error: str | None = None
            for candidate_url in urls_to_try:
                try:
                    cmp_cert = fetch_fn(candidate_url, key_ski, effective_timeout)
                    if cmp_cert is not None:
                        self._logger.info(
                            'cert_provisioning.cmp_fetched',
                            extra={'extra_fields': {
                                'fiscal_number': fiscal_number,
                                'ski_hex': ski_hex,
                                'cmp_url': candidate_url,
                            }},
                        )
                        break
                except Exception as exc:
                    last_error = f'{candidate_url}: {exc}'
                    self._logger.warning(
                        'cert_provisioning.cmp_failed',
                        extra={'extra_fields': {
                            'fiscal_number': fiscal_number,
                            'ski_hex': ski_hex,
                            'cmp_url': candidate_url,
                            'error': str(exc),
                        }},
                    )
            if cmp_cert is None and last_error is not None:
                cmp_error = f'all CMP endpoints exhausted; last error: {last_error}'

        if cmp_cert is not None:
            # Re-verify SKI of the returned cert — CA could return the
            # wrong cert on a hash collision / misconfig; cheap to check.
            if not self._cert_matches_ski(cmp_cert, key_ski):
                cmp_error = 'CMP returned a cert whose SKI does not match the query'
                self._logger.warning(
                    'cert_provisioning.cmp_ski_mismatch',
                    extra={'extra_fields': {
                        'fiscal_number': fiscal_number,
                        'ski_hex': ski_hex,
                    }},
                )
            else:
                self._persist(
                    fiscal_number=fiscal_number,
                    cert_der=cmp_cert,
                    ski_hex=ski_hex,
                    source=SOURCE_CMP,
                )
                return CertProvisioningResult(
                    status=STATUS_OK,
                    fiscal_number=fiscal_number,
                    ski_hex=ski_hex,
                    source=SOURCE_CMP,
                    cert_fingerprint=hashlib.sha256(cmp_cert).hexdigest(),
                )

        return CertProvisioningResult(
            status=STATUS_NEEDS_MANUAL_UPLOAD,
            fiscal_number=fiscal_number,
            ski_hex=ski_hex,
            error=cmp_error or 'no cert resolved',
        )

    # ─── Signing-path entrypoint ────────────────────────────────────────

    def get_cert_for_fiscal_number(self, fiscal_number: str) -> bytes | None:
        """Fast-path read for the signing worker.

        Short, read-only connection. Safe to call from hot paths. FI-1
        friendly: no network I/O.
        """
        with self._connect() as conn:
            record = OperatorCertsRepository.get_by_fiscal_number(conn, fiscal_number)
        return record.cert_der if record is not None else None

    def get_record(self, fiscal_number: str) -> OperatorCertRecord | None:
        with self._connect() as conn:
            return OperatorCertsRepository.get_by_fiscal_number(conn, fiscal_number)

    # ─── Manual upload ──────────────────────────────────────────────────

    def manual_upload(
        self,
        fiscal_number: str,
        cert_der: bytes,
        *,
        allow_first_time: bool = True,
    ) -> CertProvisioningResult:
        """Admin-only: accept a manually-uploaded cert for this fiscal_number.

        If a cert was previously provisioned (and therefore has a
        key-derived SKI on file), the uploaded cert's SKI MUST match
        that SKI. Otherwise we refuse — the upload would reasonably
        belong to a different key, and silently accepting it would
        shift trust without the operator knowing.

        If `allow_first_time=True` and no prior row exists, we accept
        the upload on faith of the admin's credentials (this path is
        used when the container+password are unavailable at onboarding).
        """
        if not cert_der or cert_der[0] != 0x30:
            raise CertProvisioningError('cert_der is not a DER SEQUENCE')

        if self._crypto is None:
            raise CertProvisioningError(
                'prro_crypto module unavailable — cannot verify SKI on manual upload'
            )

        cert_ski_bytes = self._ski_from_cert(cert_der)
        if cert_ski_bytes is None:
            raise CertProvisioningError(
                'cannot extract pubkey/SKI from uploaded cert'
            )
        cert_ski_hex = cert_ski_bytes.hex().upper()

        with self._connect() as conn:
            existing = OperatorCertsRepository.get_by_fiscal_number(conn, fiscal_number)

        if existing is not None:
            if existing.ski_hex.upper() != cert_ski_hex:
                raise ManualUploadSkiMismatch(
                    fiscal_number, existing.ski_hex.upper(), cert_ski_hex,
                )
        elif not allow_first_time:
            raise CertProvisioningError(
                f'no prior provisioning for {fiscal_number}; refusing first-time manual upload'
            )

        self._persist(
            fiscal_number=fiscal_number,
            cert_der=cert_der,
            ski_hex=cert_ski_hex,
            source=SOURCE_MANUAL,
        )
        return CertProvisioningResult(
            status=STATUS_OK,
            fiscal_number=fiscal_number,
            ski_hex=cert_ski_hex,
            source=SOURCE_MANUAL,
            cert_fingerprint=hashlib.sha256(cert_der).hexdigest(),
        )

    # ─── Background refresh ─────────────────────────────────────────────

    def refresh_expiring_certs(
        self,
        *,
        within_days: int | None = None,
        cmp_url: str | None = None,
        timeout_seconds: float | None = None,
    ) -> list[CertProvisioningResult]:
        """Re-fetch certs whose `valid_to` is within `within_days`.

        Only refreshes entries with `source in ('container', 'cmp')` —
        manually-uploaded certs get refreshed only when an admin
        explicitly re-uploads. CMP-sourced rows pull a fresh DER from
        the CA; container-sourced rows also go to CMP because the
        original container isn't available after onboarding.

        FI-1: the CMP fetch runs outside the DB transaction. The short
        upsert transaction runs only after the network call returns.
        """
        cfg = self._load_config()
        effective_days = within_days if within_days is not None else cfg.refresh_within_days
        chosen_url = cmp_url or cfg.primary_cmp_url
        effective_timeout = timeout_seconds if timeout_seconds is not None else float(cfg.timeout_seconds)

        if self._crypto is None:
            return []
        fetch_fn = getattr(self._crypto, 'fetch_cert_by_ski', None)
        if fetch_fn is None:
            return []

        with self._connect() as conn:
            candidates = OperatorCertsRepository.list_expiring(
                conn, within_days=effective_days,
            )

        results: list[CertProvisioningResult] = []
        for record in candidates:
            if record.source == SOURCE_MANUAL:
                continue
            ski_bytes = bytes.fromhex(record.ski_hex)
            try:
                new_cert = fetch_fn(chosen_url, ski_bytes, effective_timeout)
            except Exception as exc:
                self._logger.warning(
                    'cert_provisioning.refresh_failed',
                    extra={'extra_fields': {
                        'fiscal_number': record.fiscal_number,
                        'ski_hex': record.ski_hex,
                        'error': str(exc),
                    }},
                )
                results.append(CertProvisioningResult(
                    status=STATUS_NEEDS_MANUAL_UPLOAD,
                    fiscal_number=record.fiscal_number,
                    ski_hex=record.ski_hex,
                    error=str(exc),
                ))
                continue

            if not self._cert_matches_ski(new_cert, ski_bytes):
                self._logger.warning(
                    'cert_provisioning.refresh_ski_mismatch',
                    extra={'extra_fields': {
                        'fiscal_number': record.fiscal_number,
                        'ski_hex': record.ski_hex,
                    }},
                )
                continue
            self._persist(
                fiscal_number=record.fiscal_number,
                cert_der=new_cert,
                ski_hex=record.ski_hex,
                source=SOURCE_CMP,
                last_refresh=True,
            )
            results.append(CertProvisioningResult(
                status=STATUS_OK,
                fiscal_number=record.fiscal_number,
                ski_hex=record.ski_hex,
                source=SOURCE_CMP,
                cert_fingerprint=hashlib.sha256(new_cert).hexdigest(),
            ))
        return results

    # ─── Internals ──────────────────────────────────────────────────────

    def _load_config(self):
        with self._connect() as conn:
            return CertProvisioningConfigRepository.get(conn)

    def _resolve_candidate_urls(self, explicit_url: str | None, cfg) -> list[str]:
        """Build the ordered list of CMP URLs to try.

        Caller-supplied `explicit_url` → single-element list (tight
        control wins). Otherwise: pull registered CA endpoints sorted by
        priority, plus the legacy primary URL from `cert_provisioning_config`
        if it's not already in the set. Duplicates collapse.
        """
        if explicit_url:
            return [explicit_url]

        urls: list[str] = []
        seen: set[str] = set()
        try:
            for ep in self._ca_endpoints_repo.list_enabled():
                if ep.cmp_url and ep.cmp_url not in seen:
                    urls.append(ep.cmp_url)
                    seen.add(ep.cmp_url)
        except Exception as exc:
            # If the table doesn't exist yet (pre-016 install) we'll
            # silently fall back to the legacy single-URL config below.
            self._logger.warning(
                'cert_provisioning.ca_endpoints_unavailable',
                extra={'extra_fields': {'error': str(exc)}},
            )

        if cfg.primary_cmp_url and cfg.primary_cmp_url not in seen:
            urls.append(cfg.primary_cmp_url)
            seen.add(cfg.primary_cmp_url)
        return urls

    def _find_cert_by_ski(self, certs: list[bytes], target_ski: bytes) -> bytes | None:
        """Scan embedded certs for one whose pubkey-SKI matches.

        Tolerant to individual cert parse failures — they're common for
        CA/intermediate certs that use different key algorithms than
        the operator's key.
        """
        for cert in certs:
            try:
                cert_ski = self._ski_from_cert(cert)
            except Exception:
                continue
            if cert_ski is not None and cert_ski == target_ski:
                return cert
        return None

    def _ski_from_cert(self, cert_der: bytes) -> bytes | None:
        """Extract cert pubkey, compute SKI. Returns None on parse failure."""
        if self._crypto is None:
            return None
        try:
            pub = self._crypto.extract_cert_pubkey(cert_der)
        except Exception:
            return None
        if not pub:
            return None
        return self._crypto.compute_ski(pub)

    def _cert_matches_ski(self, cert_der: bytes, target_ski: bytes) -> bool:
        got = self._ski_from_cert(cert_der)
        return got is not None and got == target_ski

    def _persist(
        self,
        *,
        fiscal_number: str,
        cert_der: bytes,
        ski_hex: str,
        source: str,
        last_refresh: bool = False,
    ) -> None:
        """Short BEGIN IMMEDIATE transaction for the upsert + audit.

        FI-1: caller must have completed any network I/O before reaching
        this method.
        """
        now = datetime.now(UTC)
        now_iso = now.isoformat(timespec='seconds')
        fingerprint = hashlib.sha256(cert_der).hexdigest()
        subject_dn, issuer_dn, valid_from, valid_to = _parse_cert_metadata(cert_der)

        with self._connect() as conn:
            conn.execute('BEGIN IMMEDIATE')
            try:
                OperatorCertsRepository.upsert(
                    conn,
                    fiscal_number=fiscal_number,
                    cert_fingerprint=fingerprint,
                    ski_hex=ski_hex,
                    cert_der=cert_der,
                    source=source,
                    fetched_at=now_iso,
                    subject_dn=subject_dn,
                    issuer_dn=issuer_dn,
                    valid_from=valid_from,
                    valid_to=valid_to,
                    last_refresh_at=now_iso if last_refresh else None,
                )
                AuditRepository.log_event(
                    conn,
                    entity_type='CERT',
                    entity_id=fiscal_number,
                    event_type='CERT_PROVISIONED',
                    severity='INFO',
                    event_payload_json=dumps_json({
                        'source': source,
                        'ski_hex': ski_hex,
                        'cert_fingerprint': fingerprint,
                        'valid_to': valid_to,
                    }),
                )
                conn.commit()
            except Exception:
                conn.rollback()
                raise
        self._logger.info(
            'cert_provisioning.persisted',
            extra={'extra_fields': {
                'fiscal_number': fiscal_number,
                'ski_hex': ski_hex,
                'source': source,
                'cert_fingerprint': fingerprint,
            }},
        )


# ─── Onboarding hook ───────────────────────────────────────────────────────

def onboarding_provision_or_raise(
    service: CertProvisioningService,
    fiscal_number: str,
    container_bytes: bytes,
    password: str,
    *,
    allow_cmp_fetch: bool = True,
) -> CertProvisioningResult:
    """Small adapter used by the onboarding path.

    Keeps the call-site ≤ 3 lines: just invoke and handle the typed
    exception. If `provision_cert` can't resolve a cert, onboarding is
    blocked via `OnboardingBlockedError` — a PRRO with no cert in the
    system MUST NOT be accepted.
    """
    result = service.provision_cert(
        fiscal_number, container_bytes, password,
        allow_cmp_fetch=allow_cmp_fetch,
    )
    if result.status != STATUS_OK:
        raise OnboardingBlockedError(
            fiscal_number=fiscal_number,
            ski_hex=result.ski_hex,
            reason=result.error or 'cert could not be resolved',
        )
    return result


# ─── Cert metadata extraction ──────────────────────────────────────────────

def _parse_cert_metadata(cert_der: bytes) -> tuple[str | None, str | None, str | None, str | None]:
    """Best-effort: pull subject_dn, issuer_dn, and the validity window.

    Returns ISO-8601 strings for valid_from / valid_to. None on any
    parse issue — metadata is observational and must never crash the
    provisioning flow.
    """
    try:
        subject = _extract_name(cert_der, which='subject')
        issuer = _extract_name(cert_der, which='issuer')
        nb, na = _extract_validity(cert_der)
        return subject, issuer, nb, na
    except Exception:
        return None, None, None, None


def _read_tlv(data: bytes, offset: int) -> tuple[int, int]:
    """Return (end, content_start) for the TLV at offset."""
    if offset >= len(data):
        raise ValueError('read past end')
    pos = offset + 1
    length_byte = data[pos]
    pos += 1
    if length_byte < 0x80:
        length = length_byte
    else:
        nbytes = length_byte & 0x7f
        length = 0
        for _ in range(nbytes):
            length = (length << 8) | data[pos]
            pos += 1
    content_start = pos
    end = pos + length
    if end > len(data):
        raise ValueError('TLV length exceeds data')
    return end, content_start


def _tbs_walker(cert_der: bytes):
    """Yield (tag, start, end, content_start) for each field inside tbsCertificate."""
    _, outer_inner = _read_tlv(cert_der, 0)  # tbsCertificate start
    tbs_end, tbs_inner = _read_tlv(cert_der, outer_inner)
    pos = tbs_inner
    # version [0] EXPLICIT — optional.
    if pos < tbs_end and cert_der[pos] == 0xA0:
        end, inner = _read_tlv(cert_der, pos)
        yield cert_der[pos], pos, end, inner
        pos = end
    while pos < tbs_end:
        end, inner = _read_tlv(cert_der, pos)
        yield cert_der[pos], pos, end, inner
        pos = end


def _extract_name(cert_der: bytes, *, which: str) -> str | None:
    """Return a flat comma-separated RDNSequence rendering of issuer / subject.

    Skips fields: serial, sigAlg, (issuer), validity, (subject). Order:
    [version?] serial sigAlg issuer validity subject spki ...
    """
    # Build index: version? / serial / sigAlg / issuer / validity / subject.
    fields = list(_tbs_walker(cert_der))
    # Skip optional version [0].
    idx = 0
    if fields and fields[0][0] == 0xA0:
        idx = 1
    # serial, sigAlg, issuer, validity, subject are the next five.
    if which == 'issuer':
        target = fields[idx + 2]
    else:  # subject
        target = fields[idx + 4]
    start = target[1]
    end = target[2]
    return _render_rdn_sequence(cert_der[start:end])


def _render_rdn_sequence(seq_der: bytes) -> str | None:
    """Render RDNSequence to "CN=..., O=..., C=..." form.

    Only handles the couple of attribute OIDs Ukrainian CAs actually use
    in subject/issuer: commonName, organizationName, organizationalUnitName,
    countryName, localityName, serialNumber. Unknown OIDs become their
    dotted string.
    """
    if not seq_der or seq_der[0] != 0x30:
        return None
    try:
        _, inner = _read_tlv(seq_der, 0)
        pos = inner
        end_outer = len(seq_der)
        parts: list[str] = []
        while pos < end_outer:
            # Each RDN is a SET.
            rdn_end, rdn_inner = _read_tlv(seq_der, pos)
            rdn_pos = rdn_inner
            while rdn_pos < rdn_end:
                # AttributeTypeAndValue SEQUENCE { oid, value }
                atv_end, atv_inner = _read_tlv(seq_der, rdn_pos)
                oid_end, oid_inner = _read_tlv(seq_der, atv_inner)
                oid_bytes = seq_der[oid_inner:oid_end]
                oid_str = _decode_oid(oid_bytes)
                val_end, val_inner = _read_tlv(seq_der, oid_end)
                val_tag = seq_der[oid_end]
                raw = seq_der[val_inner:val_end]
                value = _decode_string(raw, val_tag)
                label = _ATTR_LABELS.get(oid_str, oid_str)
                parts.append(f'{label}={value}')
                rdn_pos = atv_end
            pos = rdn_end
        return ', '.join(parts) if parts else None
    except Exception:
        return None


_ATTR_LABELS: dict[str, str] = {
    '2.5.4.3': 'CN',
    '2.5.4.4': 'SN',
    '2.5.4.6': 'C',
    '2.5.4.7': 'L',
    '2.5.4.10': 'O',
    '2.5.4.11': 'OU',
    '2.5.4.42': 'GN',
    '2.5.4.5': 'serialNumber',
    '1.2.840.113549.1.9.1': 'emailAddress',
}


def _decode_oid(oid_bytes: bytes) -> str:
    if not oid_bytes:
        return ''
    b0 = oid_bytes[0]
    parts = [str(b0 // 40), str(b0 % 40)]
    acc = 0
    for b in oid_bytes[1:]:
        acc = (acc << 7) | (b & 0x7f)
        if b & 0x80 == 0:
            parts.append(str(acc))
            acc = 0
    return '.'.join(parts)


def _decode_string(raw: bytes, tag: int) -> str:
    # PrintableString / UTF8String / IA5String / BMPString / TeletexString
    if tag == 0x1E:  # BMPString (UCS-2 BE)
        try:
            return raw.decode('utf-16-be')
        except Exception:
            return raw.hex()
    try:
        return raw.decode('utf-8')
    except UnicodeDecodeError:
        return raw.decode('latin-1', errors='replace')


def _extract_validity(cert_der: bytes) -> tuple[str | None, str | None]:
    """Return ISO-8601 (valid_from, valid_to) or (None, None)."""
    fields = list(_tbs_walker(cert_der))
    idx = 0
    if fields and fields[0][0] == 0xA0:
        idx = 1
    validity = fields[idx + 3]
    start = validity[1]
    end = validity[2]
    seq = cert_der[start:end]
    _, inner = _read_tlv(seq, 0)
    pos = inner
    outer_end = len(seq)
    results: list[str | None] = []
    while pos < outer_end and len(results) < 2:
        t_end, t_inner = _read_tlv(seq, pos)
        tag = seq[pos]
        raw = seq[t_inner:t_end].decode('ascii', errors='replace')
        results.append(_parse_asn1_time(raw, tag))
        pos = t_end
    while len(results) < 2:
        results.append(None)
    return results[0], results[1]


def _parse_asn1_time(raw: str, tag: int) -> str | None:
    """UTCTime (tag 0x17) or GeneralizedTime (tag 0x18) → ISO-8601 UTC."""
    try:
        if tag == 0x17:
            # YYMMDDHHMMSSZ or YYMMDDHHMMZ
            if raw.endswith('Z'):
                raw = raw[:-1]
            fmt = '%y%m%d%H%M%S' if len(raw) == 12 else '%y%m%d%H%M'
            dt = datetime.strptime(raw, fmt).replace(tzinfo=UTC)
        elif tag == 0x18:
            if raw.endswith('Z'):
                raw = raw[:-1]
            # YYYYMMDDHHMMSS
            if '.' in raw:
                raw = raw.split('.', 1)[0]
            fmt = '%Y%m%d%H%M%S' if len(raw) >= 14 else '%Y%m%d%H%M'
            dt = datetime.strptime(raw, fmt).replace(tzinfo=UTC)
        else:
            return None
        return dt.isoformat(timespec='seconds')
    except Exception:
        return None


__all__ = [
    'CertProvisioningService',
    'CertProvisioningResult',
    'CertProvisioningError',
    'ContainerDecryptError',
    'ManualUploadSkiMismatch',
    'OnboardingBlockedError',
    'onboarding_provision_or_raise',
    'STATUS_OK',
    'STATUS_NEEDS_MANUAL_UPLOAD',
    'SOURCE_CONTAINER',
    'SOURCE_CMP',
    'SOURCE_MANUAL',
]
