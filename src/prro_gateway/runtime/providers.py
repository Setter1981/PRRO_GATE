from __future__ import annotations

import httpx

from ..ports import CryptoProviderUnavailableError


class PassthroughCryptoProvider:
    """Development-friendly provider used when local signing is disabled by profile config."""

    def sign(self, *, document_id: str, payload_json: str) -> str:
        return payload_json

    def sign_raw(self, *, data: bytes) -> bytes:
        return data  # passthrough


class SidecarCryptoClient:
    """HTTP client for a local crypto sidecar service.

    Transport contract:
      POST {base_url}/sign
      Request body (JSON):  {"document_id": str, "payload": str}
      Response body (JSON): {"signed_payload": str}

    All HTTP transport errors and unexpected response shapes are mapped to
    CryptoProviderUnavailableError so the write-path safe failure path is triggered.
    """

    def __init__(
        self,
        base_url: str,
        http_client: httpx.Client | None = None,
        connect_timeout: float = 5.0,
        read_timeout: float = 10.0,
    ) -> None:
        self.base_url = base_url.rstrip('/')
        if http_client is not None:
            self._http = http_client
        else:
            self._http = httpx.Client(
                timeout=httpx.Timeout(timeout=read_timeout, connect=connect_timeout)
            )

    def sign(self, *, document_id: str, payload_json: str) -> str:
        try:
            resp = self._http.post(
                f'{self.base_url}/sign',
                json={'document_id': document_id, 'payload': payload_json},
            )
            resp.raise_for_status()
            data = resp.json()
            return data['signed_payload']
        except httpx.HTTPStatusError as exc:
            raise CryptoProviderUnavailableError(
                f'sidecar HTTP {exc.response.status_code}: {exc}'
            ) from exc
        except httpx.HTTPError as exc:
            raise CryptoProviderUnavailableError(
                f'sidecar transport error: {exc}'
            ) from exc
        except (KeyError, ValueError) as exc:
            raise CryptoProviderUnavailableError(
                f'sidecar response invalid: {exc}'
            ) from exc


    def sign_raw(self, *, data: bytes, document_id: str | None = None) -> bytes:
        """Sign arbitrary bytes and return CMS/PKCS#7 SignedData (DER).

        Sidecar contract:
          POST {base_url}/sign_raw
          Request body (JSON): {"payload_base64": str, "document_id": str | null}
          Response body (JSON): {"signed_base64": str}

        document_id is an optional idempotency hint (M4): sidecars that support it
        can deduplicate concurrent sign_raw calls for the same document (e.g. after
        a timeout+retry). Sidecars that do not recognise the field must ignore it.
        """
        import base64
        body: dict = {'payload_base64': base64.b64encode(data).decode('ascii')}
        if document_id is not None:
            body['document_id'] = document_id
        try:
            resp = self._http.post(
                f'{self.base_url}/sign_raw',
                json=body,
            )
            resp.raise_for_status()
            result = resp.json()
            return base64.b64decode(result['signed_base64'])
        except httpx.HTTPStatusError as exc:
            raise CryptoProviderUnavailableError(
                f'sidecar sign_raw HTTP {exc.response.status_code}: {exc}'
            ) from exc
        except httpx.HTTPError as exc:
            raise CryptoProviderUnavailableError(
                f'sidecar sign_raw transport error: {exc}'
            ) from exc
        except (KeyError, ValueError) as exc:
            raise CryptoProviderUnavailableError(
                f'sidecar sign_raw response invalid: {exc}'
            ) from exc


class SidecarCryptoProvider:
    """CryptoProvider backed by a remote crypto sidecar via SidecarCryptoClient."""

    def __init__(self, client: SidecarCryptoClient) -> None:
        self.client = client

    def sign(self, *, document_id: str, payload_json: str) -> str:
        return self.client.sign(document_id=document_id, payload_json=payload_json)

    def sign_raw(self, *, data: bytes, document_id: str | None = None) -> bytes:
        """Sign arbitrary bytes via sidecar's /sign_raw endpoint. Returns CMS/PKCS#7 DER bytes."""
        return self.client.sign_raw(data=data, document_id=document_id)


__all__ = ["PassthroughCryptoProvider", "SidecarCryptoClient", "SidecarCryptoProvider"]
