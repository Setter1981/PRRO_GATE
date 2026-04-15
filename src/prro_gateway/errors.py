from __future__ import annotations

from .enums import CanonicalErrorCode
from .models.common import CanonicalError


def build_canonical_error(
    code: CanonicalErrorCode,
    *,
    message: str | None = None,
    retry_after_seconds: int | None = None,
    details: dict | None = None,
) -> CanonicalError:
    return CanonicalError(
        code=code,
        message=message or code.default_message,
        retryable=code.retryable,
        retry_after_seconds=retry_after_seconds,
        details=details,
    )


__all__ = ['build_canonical_error']
