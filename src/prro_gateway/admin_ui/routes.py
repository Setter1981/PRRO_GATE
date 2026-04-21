"""Admin UI routes — login + dashboard + documents.

Mounted via `register_admin_ui(app, container)` from the REST app
factory when `config.admin_ui.enabled` is true.  Session auth through
`starlette.middleware.sessions.SessionMiddleware` with an HMAC-signed
cookie.
"""
from __future__ import annotations

import hmac
import json
import logging
import secrets
import sqlite3
from pathlib import Path
from typing import TYPE_CHECKING
from urllib.parse import urlsplit, urlunsplit

from fastapi import FastAPI, File, Form, HTTPException, Request, UploadFile
from fastapi.responses import HTMLResponse, JSONResponse, RedirectResponse
from jinja2 import Environment, FileSystemLoader, select_autoescape
from starlette.middleware.sessions import SessionMiddleware

from ..rendering import format_receipt, render_html
from ..rendering.context_builder import build_render_context
from ..repositories.fiscal_documents import FiscalDocumentRepository
from ..services.onboarding_key_identity import (
    KeyIdentity,
    extract_identity_from_cert,
)

try:
    import prro_crypto as _prro_crypto_module  # optional Rust wheel
except ImportError:
    _prro_crypto_module = None  # type: ignore[assignment]

_MAX_KEY_UPLOAD_BYTES = 5 * 1024 * 1024  # 5 MB
_KEY_EXTENSIONS = frozenset({".jks", ".pfx", ".p12", ".zs2", ".dat", ".pk8"})


def _resolve_keys_dir(cfg_value: str | None) -> Path | None:
    """Return the absolute keys_dir path if the config points at an
    existing readable directory; otherwise None.

    Non-existent path is a soft miss — the UI shows an empty dropdown
    rather than failing startup — because operators often configure
    the path before creating the directory.
    """
    if not cfg_value:
        return None
    try:
        p = Path(cfg_value).expanduser().resolve()
    except (OSError, ValueError):
        return None
    if not p.is_dir():
        return None
    return p


def _list_server_keys(keys_dir: Path | None) -> list[dict[str, str]]:
    if keys_dir is None:
        return []
    try:
        entries = sorted(keys_dir.iterdir(), key=lambda p: p.name.lower())
    except OSError:
        return []
    out: list[dict[str, str]] = []
    for entry in entries:
        # Skip symlinks even if they resolve inside keys_dir — we don't
        # want the listing to include an entry that _read_server_key
        # might later refuse, and symlinks are not how operators stage
        # fiscal keys.
        if entry.is_symlink():
            continue
        if not entry.is_file():
            continue
        if entry.suffix.lower() not in _KEY_EXTENSIONS:
            continue
        out.append({"name": entry.name})
    return out


def _read_server_key(keys_dir: Path | None, basename: str) -> bytes:
    """Read key bytes from keys_dir by basename.

    Raises `ValueError` on every path that is not a simple whitelisted
    basename resolving under `keys_dir` — caller turns that into 400.
    """
    if keys_dir is None:
        raise ValueError("keys_dir not configured")
    if not basename:
        raise ValueError("empty basename")
    # Reject anything that is not a flat name.  `Path(basename).name`
    # would silently strip path components, so check textually.
    if (
        "/" in basename
        or "\\" in basename
        or basename in ("", ".", "..")
        or basename.startswith(".")
    ):
        raise ValueError("invalid basename")
    if Path(basename).suffix.lower() not in _KEY_EXTENSIONS:
        raise ValueError("extension not in whitelist")
    candidate = (keys_dir / basename).resolve()
    # Belt-and-braces: candidate must be a direct child of keys_dir.
    try:
        candidate.relative_to(keys_dir)
    except ValueError as exc:
        raise ValueError("path escapes keys_dir") from exc
    if candidate.parent != keys_dir:
        raise ValueError("path not a direct child")
    if not candidate.is_file():
        raise ValueError("file not found")
    return candidate.read_bytes()

if TYPE_CHECKING:
    from ..runtime.container import RuntimeContainer

_TEMPLATE_DIR = Path(__file__).parent / "templates"
_SESSION_AUTH_KEY = "admin_authenticated"
_SESSION_CSRF_KEY = "admin_csrf_token"
_MIN_SESSION_SECRET_LEN = 16

# Per-field length ceilings for admin-UI form inputs — must match the
# `maxlength=` attributes in the template and not exceed storage column
# widths. A missing ceiling would let a logged-in operator post an
# unbounded body and waste memory.
_MAX_LEN = {
    "org_name": 256,
    "point_name": 256,
    "point_address": 256,
    "operator_name": 256,
    "operator_inn": 10,
    "jks_path": 512,
    "jks_password": 512,
    "vat_payer_inn": 12,
}

_log = logging.getLogger(__name__)


_ALLOWED_FISCAL_MODES = {"test", "prod"}


def _get_or_create_csrf(request: "Request") -> str:
    token = request.session.get(_SESSION_CSRF_KEY)
    if not token:
        token = secrets.token_urlsafe(32)
        request.session[_SESSION_CSRF_KEY] = token
    return token


def _check_csrf(request: "Request", submitted: str) -> bool:
    expected = request.session.get(_SESSION_CSRF_KEY, "")
    if not expected or not submitted:
        return False
    return hmac.compare_digest(expected, submitted)


def _validate_new_fn(form: dict[str, str]) -> list[str]:
    errors: list[str] = []

    # Length caps — defense-in-depth; template has `maxlength=` but that
    # is client-side only.
    for field, cap in _MAX_LEN.items():
        if len(form.get(field, "")) > cap:
            errors.append(f"Поле {field}: максимум {cap} символів.")

    fn = form.get("fiscal_number", "")
    if len(fn) != 10 or not fn.isdigit():
        errors.append("Невірний формат фіскального номера (рівно 10 цифр).")

    tin = form.get("tax_number", "")
    if not tin or not tin.isdigit() or len(tin) not in (8, 10):
        errors.append("Невірний ЄДРПОУ / ІПН (8 або 10 цифр).")

    mode = form.get("fiscal_mode", "")
    if mode not in _ALLOWED_FISCAL_MODES:
        errors.append("Режим має бути 'test' або 'prod'.")

    vat = form.get("vat_payer_inn", "") or ""
    if vat and (not vat.isdigit() or len(vat) != 12):
        errors.append("ІПН платника ПДВ має бути рівно 12 цифр (або порожньо).")

    for name in ("min_offline_codes", "max_offline_codes"):
        raw = form.get(name, "0") or "0"
        try:
            val = int(raw)
            if val < 0:
                raise ValueError
        except ValueError:
            errors.append(f"Поле {name} має бути невід'ємним цілим.")
            form[name] = "0"

    try:
        if int(form.get("max_offline_codes", "0") or "0") < int(
            form.get("min_offline_codes", "0") or "0"
        ):
            errors.append("max_offline_codes не може бути меншим за min_offline_codes.")
    except ValueError:
        pass  # already reported above

    cashier_any = any(
        form.get(k) for k in ("operator_name", "operator_inn", "jks_path")
    )
    if cashier_any:
        inn = form.get("operator_inn", "")
        if not inn or not inn.isdigit() or len(inn) != 10:
            errors.append("ІНН касира має бути рівно 10 цифр.")
        if not form.get("jks_path"):
            errors.append("Шлях до ключа касира обов'язковий.")

    return errors


def _redact_url_userinfo(url: str | None) -> str:
    if not url:
        return "—"
    try:
        parts = urlsplit(url)
        username = parts.username
        password = parts.password
    except ValueError:
        # Malformed userinfo/port — redact entirely rather than leak or 500.
        return "***"
    if not username and not password:
        return url
    netloc = parts.netloc
    at = netloc.rfind("@")
    host_port = netloc[at + 1:] if at >= 0 else netloc
    redacted_netloc = f"***@{host_port}" if host_port else "***"
    return urlunsplit((parts.scheme, redacted_netloc, parts.path, parts.query, parts.fragment))


def register_admin_ui(app: FastAPI, container: "RuntimeContainer") -> None:
    """Wire admin UI routes onto the FastAPI app.

    Returns silently when `admin_ui.enabled` is false — calling routes
    then 404 naturally since nothing was registered.
    """
    cfg = container.config.admin_ui
    if not cfg.enabled:
        return

    if not cfg.session_secret or len(cfg.session_secret) < _MIN_SESSION_SECRET_LEN:
        # Refuse to mount insecure UI — blank/short secret would let
        # any client forge a session cookie.
        @app.get("/admin/ui/{path:path}", include_in_schema=False)
        async def _misconfigured_admin_ui(path: str) -> HTMLResponse:
            return HTMLResponse(
                content=(
                    "<h1>Admin UI misconfigured</h1>"
                    "<p>Set <code>admin_ui.session_secret</code> to at least "
                    f"{_MIN_SESSION_SECRET_LEN} characters.</p>"
                ),
                status_code=503,
            )
        return

    app.add_middleware(
        SessionMiddleware,
        secret_key=cfg.session_secret,
        session_cookie=cfg.session_cookie,
        max_age=cfg.session_max_age_seconds,
        same_site="lax",
        https_only=False,  # set True in prod via reverse-proxy TLS
    )

    env = Environment(
        loader=FileSystemLoader(str(_TEMPLATE_DIR)),
        autoescape=select_autoescape(default_for_string=True, default=True),
    )

    def _render(template: str, request: Request, **ctx) -> HTMLResponse:
        tpl = env.get_template(template)
        rendered = tpl.render(
            request=request,
            authenticated=request.session.get(_SESSION_AUTH_KEY, False),
            **ctx,
        )
        return HTMLResponse(rendered)

    def _require_auth(request: Request) -> RedirectResponse | None:
        if not request.session.get(_SESSION_AUTH_KEY):
            return RedirectResponse("/admin/ui/login", status_code=303)
        return None

    @app.get("/admin/ui/login", include_in_schema=False)
    async def login_form(request: Request) -> HTMLResponse:
        return _render("login.html.j2", request, error=None)

    @app.post("/admin/ui/login", include_in_schema=False, response_model=None)
    async def login_submit(
        request: Request,
        password: str = Form(""),
    ):
        if hmac.compare_digest(password, cfg.password) and cfg.password:
            request.session[_SESSION_AUTH_KEY] = True
            return RedirectResponse("/admin/ui/", status_code=303)
        # Wrong / empty password — re-render form with error message.
        return HTMLResponse(
            content=env.get_template("login.html.j2").render(
                request=request,
                authenticated=False,
                error="Невірний пароль.",
            ),
            status_code=401,
        )

    @app.get("/admin/ui/logout", include_in_schema=False)
    async def logout(request: Request) -> RedirectResponse:
        request.session.pop(_SESSION_AUTH_KEY, None)
        return RedirectResponse("/admin/ui/login", status_code=303)

    @app.get("/admin/ui/", include_in_schema=False, response_model=None)
    @app.get("/admin/ui", include_in_schema=False, response_model=None)
    async def dashboard(request: Request):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        # Counters for dashboard cards.
        with container.connect() as conn:
            total_docs_row = conn.execute(
                "SELECT COUNT(*) FROM fiscal_documents"
            ).fetchone()
            inbox_new_row = conn.execute(
                "SELECT COUNT(*) FROM ingress_inbox WHERE status = 'NEW'"
            ).fetchone()
            manual_count = FiscalDocumentRepository.count_requires_manual_reconciliation(conn)
        return _render(
            "dashboard.html.j2",
            request,
            total_docs=int(total_docs_row[0]) if total_docs_row else 0,
            inbox_new=int(inbox_new_row[0]) if inbox_new_row else 0,
            manual_count=int(manual_count),
        )

    @app.get("/admin/ui/documents/", include_in_schema=False, response_model=None)
    @app.get("/admin/ui/documents", include_in_schema=False, response_model=None)
    async def documents_list(request: Request):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        with container.connect() as conn:
            rows = conn.execute(
                """SELECT document_id, fiscal_number, doc_type, state,
                          server_fiscal_no, total_sum, business_ts
                   FROM fiscal_documents
                   ORDER BY created_at DESC
                   LIMIT 50"""
            ).fetchall()
        return _render("document_list.html.j2", request, documents=rows)

    # ── Settings (Phase 5 — read-only) ──────────────────────────────
    _SETTINGS_TABS = [
        ("fns",       "/admin/ui/settings/fns",       "Фіскальні номери"),
        ("operators", "/admin/ui/settings/operators", "Касири"),
        ("node",      "/admin/ui/settings/node",      "Стан вузла"),
        ("dps",       "/admin/ui/settings/dps",       "ДПС"),
    ]

    def _render_settings(template: str, request: Request, active: str, **ctx) -> HTMLResponse:
        return _render(template, request, active_tab=active, tabs=_SETTINGS_TABS, **ctx)

    @app.get("/admin/ui/settings/", include_in_schema=False, response_model=None)
    @app.get("/admin/ui/settings", include_in_schema=False, response_model=None)
    async def settings_landing(request: Request):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        return _render_settings("settings_landing.html.j2", request, active="")

    @app.get("/admin/ui/settings/fns", include_in_schema=False, response_model=None)
    async def settings_fns(request: Request):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        with container.connect() as conn:
            rows = conn.execute(
                """SELECT fiscal_number, tax_number, fiscal_mode,
                          tsp_enabled, offline_enabled,
                          COALESCE(org_name, ''),
                          min_offline_codes, max_offline_codes,
                          COALESCE(vat_payer_inn, '')
                   FROM fiscal_number_config ORDER BY fiscal_number"""
            ).fetchall()
        return _render_settings("settings_fns.html.j2", request, active="fns", fns=rows)

    @app.get("/admin/ui/settings/fns/new", include_in_schema=False, response_model=None)
    async def new_fn_form(request: Request):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        return _render_settings(
            "settings_fn_new.html.j2", request, active="fns",
            form={}, errors=[], csrf_token=_get_or_create_csrf(request),
        )

    @app.post("/admin/ui/settings/fns/new", include_in_schema=False, response_model=None)
    async def new_fn_submit(
        request: Request,
        csrf_token: str = Form(""),
        fiscal_number: str = Form(""),
        tax_number: str = Form(""),
        fiscal_mode: str = Form("test"),
        org_name: str = Form(""),
        point_name: str = Form(""),
        point_address: str = Form(""),
        tsp_enabled: str = Form("0"),
        offline_enabled: str = Form("1"),
        min_offline_codes: str = Form("0"),
        max_offline_codes: str = Form("0"),
        operator_name: str = Form(""),
        operator_inn: str = Form(""),
        jks_path: str = Form(""),
        jks_password: str = Form(""),
        vat_payer_inn: str = Form(""),
    ):
        redirect = _require_auth(request)
        if redirect:
            return redirect

        # CSRF guard — reject forged cross-origin form POSTs even if the
        # session cookie is replayed (SameSite=Lax permits top-level POST).
        if not _check_csrf(request, csrf_token):
            return HTMLResponse(
                content="<h1>CSRF validation failed</h1>",
                status_code=403,
            )

        form = {
            "fiscal_number": fiscal_number.strip(),
            "tax_number": tax_number.strip(),
            "fiscal_mode": fiscal_mode.strip(),
            "org_name": org_name.strip(),
            "point_name": point_name.strip(),
            "point_address": point_address.strip(),
            "tsp_enabled": tsp_enabled.strip(),
            "offline_enabled": offline_enabled.strip(),
            "min_offline_codes": min_offline_codes.strip(),
            "max_offline_codes": max_offline_codes.strip(),
            "operator_name": operator_name.strip(),
            "operator_inn": operator_inn.strip(),
            "jks_path": jks_path.strip(),
            "vat_payer_inn": vat_payer_inn.strip(),
            # Password intentionally NOT echoed back to the re-render and
            # never returned to the template dict.
        }
        errors = _validate_new_fn(form)

        def _bad(status: int) -> HTMLResponse:
            return HTMLResponse(
                content=env.get_template("settings_fn_new.html.j2").render(
                    request=request,
                    authenticated=True,
                    active_tab="fns",
                    tabs=_SETTINGS_TABS,
                    form=form,
                    errors=errors,
                    csrf_token=_get_or_create_csrf(request),
                ),
                status_code=status,
            )

        if errors:
            return _bad(400)

        cashier_provided = any((operator_name, operator_inn, jks_path, jks_password))
        audit_payload = json.dumps(
            {
                "fiscal_mode": form["fiscal_mode"],
                "has_cashier": cashier_provided,
                "org_name": form["org_name"] or None,
                "point_name": form["point_name"] or None,
                "vat_payer_inn_set": bool(form["vat_payer_inn"]),
            },
            ensure_ascii=False,
        )

        try:
            with container.connect() as conn:
                # Explicit transaction boundary — do not rely on
                # close-without-commit semantics. `with conn:` commits
                # on success, rolls back on exception.
                with conn:
                    conn.execute(
                        """INSERT INTO fiscal_number_config (
                                fiscal_number, tax_number, fiscal_mode,
                                tsp_enabled, offline_enabled,
                                org_name, org_address,
                                min_offline_codes, max_offline_codes,
                                vat_payer_inn
                           ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                        (
                            form["fiscal_number"], form["tax_number"], form["fiscal_mode"],
                            1 if form["tsp_enabled"] == "1" else 0,
                            1 if form["offline_enabled"] == "1" else 0,
                            form["org_name"] or None,
                            form["point_address"] or None,
                            int(form["min_offline_codes"]),
                            int(form["max_offline_codes"]),
                            form["vat_payer_inn"] or None,
                        ),
                    )
                    if cashier_provided:
                        conn.execute(
                            """INSERT INTO sidecar_operators (
                                    fiscal_number, operator_name, operator_inn,
                                    jks_path, jks_password, active
                               ) VALUES (?, ?, ?, ?, ?, 1)""",
                            (
                                form["fiscal_number"], operator_name or None,
                                operator_inn, jks_path, jks_password,
                            ),
                        )
                    conn.execute(
                        """INSERT INTO audit_log (
                                entity_type, entity_id, event_type, severity,
                                event_payload_json
                           ) VALUES (?, ?, 'fn_registered', 'INFO', ?)""",
                        (
                            "fiscal_number_config",
                            form["fiscal_number"],
                            audit_payload,
                        ),
                    )
        except sqlite3.IntegrityError as exc:
            _log.warning("add_fn_integrity_error fn=%s err=%s",
                         form["fiscal_number"], exc)
            errors = ["Такий фіскальний номер вже зареєстровано."]
            return _bad(409)
        except sqlite3.DatabaseError:
            _log.exception("add_fn_db_error fn=%s", form["fiscal_number"])
            errors = ["Помилка збереження. Зверніться до адміністратора (логи сервера)."]
            return _bad(500)

        return RedirectResponse("/admin/ui/settings/fns", status_code=303)

    @app.get("/admin/ui/settings/keys/list",
             include_in_schema=False, response_model=None)
    async def keys_list(request: Request):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        kd = _resolve_keys_dir(container.config.admin_ui.keys_dir)
        return JSONResponse({"keys": _list_server_keys(kd)})

    @app.post("/admin/ui/settings/fns/parse-key", include_in_schema=False, response_model=None)
    async def parse_key(request: Request):
        # Auth FIRST — before touching the multipart body.  A declarative
        # `UploadFile = File(...)` parameter would force Starlette to
        # parse the body during dependency injection, making the body
        # read occur BEFORE this guard.  Reading `request.form()`
        # manually after auth/CSRF closes that window (1st-review HIGH).
        redirect = _require_auth(request)
        if redirect:
            return redirect

        crypto_mod = globals().get("_prro_crypto_module")
        if crypto_mod is None:
            # Short-circuit before reading body — no need to buffer a
            # 5 MB upload just to tell the operator the wheel is missing.
            return JSONResponse(
                {"error": "prro_crypto wheel is not installed; "
                          "key parsing unavailable on this host."},
                status_code=503,
            )

        try:
            form = await request.form(max_files=1, max_fields=10)
        except Exception:  # noqa: BLE001 — malformed multipart
            return JSONResponse(
                {"error": "malformed multipart body"}, status_code=400,
            )

        csrf_token = str(form.get("csrf_token") or "")
        if not _check_csrf(request, csrf_token):
            return JSONResponse({"error": "csrf"}, status_code=403)

        password = str(form.get("password") or "")
        key_path = str(form.get("key_path") or "").strip()
        key_file = form.get("key_file")

        if key_path:
            # Server-side key selected from keys_dir dropdown — no
            # multipart upload needed.
            kd = _resolve_keys_dir(container.config.admin_ui.keys_dir)
            try:
                raw = _read_server_key(kd, key_path)
            except ValueError as exc:
                _log.info("parse_key_keypath_rejected reason=%s",
                          type(exc).__name__)
                return JSONResponse(
                    {"error": "Неприпустимий шлях до ключа або файл не знайдено."},
                    status_code=400,
                )
        else:
            if key_file is None or not hasattr(key_file, "read"):
                return JSONResponse(
                    {"error": "key_file missing"}, status_code=400,
                )
            raw = await key_file.read(_MAX_KEY_UPLOAD_BYTES + 1)
            if len(raw) > _MAX_KEY_UPLOAD_BYTES:
                return JSONResponse(
                    {"error": f"file too large (> {_MAX_KEY_UPLOAD_BYTES} bytes)"},
                    status_code=400,
                )
        try:
            parsed = crypto_mod.extract_private_key(raw, password)
        except Exception as exc:  # noqa: BLE001 — library exception, opaque
            _log.info("parse_key_rejected reason=%s",
                      type(exc).__name__)
            return JSONResponse(
                {"error": "Неможливо відкрити контейнер: перевірте пароль та формат файлу."},
                status_code=400,
            )
        certs = parsed.get("certs") if isinstance(parsed, dict) else None
        if not isinstance(certs, list) or not certs:
            # Key-6.dat and similar: no embedded cert chain.
            return JSONResponse(
                {"error": "Контейнер не містить сертифіката (Key-6.dat / zs2 without chain). "
                          "Завантажте .cer окремо або оберіть інший контейнер."},
                status_code=400,
            )
        first_cert = certs[0]
        if not isinstance(first_cert, (bytes, bytearray)):
            return JSONResponse(
                {"error": "cert payload is not bytes"}, status_code=400,
            )
        ident: KeyIdentity = extract_identity_from_cert(bytes(first_cert))
        # This route performs NO DB writes by design — a pre-fill attempt
        # is not a fiscal event and does not go in audit_log.
        return JSONResponse({
            "subject_dn": ident.subject_dn,
            "org_name": ident.org_name,
            "edrpou": ident.edrpou,
            "drfo": ident.drfo,
            "operator_name": ident.operator_name,
            "is_legal_entity": ident.is_legal_entity,
        })

    def _load_fn_row(fn: str) -> dict[str, object] | None:
        with container.connect() as conn:
            row = conn.execute(
                """SELECT fiscal_number, tax_number, fiscal_mode,
                          tsp_enabled, offline_enabled,
                          COALESCE(org_name, '') AS org_name,
                          COALESCE(org_address, '') AS org_address,
                          min_offline_codes, max_offline_codes,
                          COALESCE(vat_payer_inn, '') AS vat_payer_inn
                   FROM fiscal_number_config WHERE fiscal_number = ?""",
                (fn,),
            ).fetchone()
        if row is None:
            return None
        return {
            "fiscal_number": row[0],
            "tax_number": row[1],
            "fiscal_mode": row[2],
            "tsp_enabled": "1" if row[3] else "0",
            "offline_enabled": "1" if row[4] else "0",
            "org_name": row[5],
            "point_address": row[6],   # stored as org_address; rendered as point_address
            "point_name": "",          # no schema column yet — accepted, audited, lost
            "min_offline_codes": str(row[7]),
            "max_offline_codes": str(row[8]),
            "vat_payer_inn": row[9],
        }

    # States that block a mode flip:
    # - OPENING/OPENED/CLOSING: an active shift is inconsistent with a
    #   channel switch (frozen invariant #3).
    # - ERROR: recovery is pending; flipping profiles would silently
    #   violate invariant #8 (recovery safety) — operator must reconcile
    #   or manually reset before switching.
    _MODE_FLIP_BLOCKING_STATES = ("OPENING", "OPENED", "CLOSING", "ERROR")

    @app.get("/admin/ui/settings/fns/{fn}/edit", include_in_schema=False, response_model=None)
    async def edit_fn_form(request: Request, fn: str):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        existing = _load_fn_row(fn)
        if existing is None:
            raise HTTPException(status_code=404, detail="FN not found")
        return _render_settings(
            "settings_fn_edit.html.j2", request, active="fns",
            form=existing, errors=[], csrf_token=_get_or_create_csrf(request),
        )

    @app.post("/admin/ui/settings/fns/{fn}/edit", include_in_schema=False, response_model=None)
    async def edit_fn_submit(
        request: Request,
        fn: str,
        csrf_token: str = Form(""),
        fiscal_mode: str = Form("test"),
        org_name: str = Form(""),
        point_name: str = Form(""),
        point_address: str = Form(""),
        tsp_enabled: str = Form("0"),
        offline_enabled: str = Form("1"),
        min_offline_codes: str = Form("0"),
        max_offline_codes: str = Form("0"),
        vat_payer_inn: str = Form(""),
    ):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        if not _check_csrf(request, csrf_token):
            return HTMLResponse(
                content="<h1>CSRF validation failed</h1>", status_code=403,
            )

        existing = _load_fn_row(fn)
        if existing is None:
            raise HTTPException(status_code=404, detail="FN not found")

        form = {
            # Primary-identity fields are sourced from the existing DB
            # row — NEVER from the POST body.  fiscal_number is keyed
            # by URL; tax_number is the legal-entity code which does
            # not change over the life of a PRRO.
            "fiscal_number": fn,
            "tax_number": str(existing["tax_number"]),
            "fiscal_mode": fiscal_mode.strip(),
            "org_name": org_name.strip(),
            "point_name": point_name.strip(),
            "point_address": point_address.strip(),
            "tsp_enabled": tsp_enabled.strip(),
            "offline_enabled": offline_enabled.strip(),
            "min_offline_codes": min_offline_codes.strip(),
            "max_offline_codes": max_offline_codes.strip(),
            "vat_payer_inn": vat_payer_inn.strip(),
            "operator_name": "",
            "operator_inn": "",
            "jks_path": "",
        }
        errors = _validate_new_fn(form)

        def _bad(status: int) -> HTMLResponse:
            return HTMLResponse(
                content=env.get_template("settings_fn_edit.html.j2").render(
                    request=request,
                    authenticated=True,
                    active_tab="fns",
                    tabs=_SETTINGS_TABS,
                    form=form,
                    errors=errors,
                    csrf_token=_get_or_create_csrf(request),
                ),
                status_code=status,
            )

        if errors:
            return _bad(400)

        mode_flipping = form["fiscal_mode"] != existing["fiscal_mode"]
        prev_vat = str(existing.get("vat_payer_inn") or "") or None
        new_vat = form["vat_payer_inn"] or None
        vat_changed = prev_vat != new_vat
        audit_payload = json.dumps(
            {
                "previous_fiscal_mode": existing["fiscal_mode"],
                "fiscal_mode": form["fiscal_mode"],
                "org_name": form["org_name"] or None,
                "point_name": form["point_name"] or None,
                "mode_flipped": mode_flipping,
                "previous_vat_payer_inn": prev_vat,
                "vat_payer_inn": new_vat,
                "vat_changed": vat_changed,
            },
            ensure_ascii=False,
        )

        try:
            with container.connect() as conn:
                # BEGIN IMMEDIATE acquires a RESERVED lock up-front so
                # the shift-state check and the UPDATE see the same
                # consistent snapshot — closing the read-after-check
                # race on invariant #3 (channel switch forbidden with
                # open shift).
                conn.execute("BEGIN IMMEDIATE")
                try:
                    if mode_flipping:
                        r = conn.execute(
                            "SELECT shift_state FROM node_state WHERE fiscal_number = ?",
                            (fn,),
                        ).fetchone()
                        if r is not None and r[0] in _MODE_FLIP_BLOCKING_STATES:
                            conn.execute("ROLLBACK")
                            errors = [
                                "Зміна режиму test↔prod заборонена при відкритій/нестабільній зміні "
                                f"(shift_state={r[0]}). Закрийте Z-звітом і повторіть."
                            ]
                            return _bad(409)

                    conn.execute(
                        # tax_number intentionally excluded — identity of
                        # the legal entity is immutable post-registration.
                        """UPDATE fiscal_number_config
                           SET fiscal_mode = ?,
                               tsp_enabled = ?, offline_enabled = ?,
                               org_name = ?, org_address = ?,
                               min_offline_codes = ?, max_offline_codes = ?,
                               vat_payer_inn = ?,
                               updated_at = CURRENT_TIMESTAMP
                           WHERE fiscal_number = ?""",
                        (
                            form["fiscal_mode"],
                            1 if form["tsp_enabled"] == "1" else 0,
                            1 if form["offline_enabled"] == "1" else 0,
                            form["org_name"] or None,
                            form["point_address"] or None,
                            int(form["min_offline_codes"]),
                            int(form["max_offline_codes"]),
                            form["vat_payer_inn"] or None,
                            fn,
                        ),
                    )
                    conn.execute(
                        """INSERT INTO audit_log (
                                entity_type, entity_id, event_type, severity,
                                event_payload_json
                           ) VALUES (
                                'fiscal_number_config', ?, 'fn_updated',
                                'INFO', ?
                           )""",
                        (fn, audit_payload),
                    )
                    conn.execute("COMMIT")
                except Exception:
                    conn.execute("ROLLBACK")
                    raise
        except sqlite3.DatabaseError:
            _log.exception("edit_fn_db_error fn=%s", fn)
            errors = ["Помилка збереження. Зверніться до адміністратора (логи сервера)."]
            return _bad(500)

        return RedirectResponse("/admin/ui/settings/fns", status_code=303)

    @app.get("/admin/ui/settings/operators", include_in_schema=False, response_model=None)
    async def settings_operators(request: Request):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        with container.connect() as conn:
            rows = conn.execute(
                """SELECT fiscal_number, ski_hex,
                          COALESCE(subject_dn, ''),
                          source,
                          COALESCE(fetched_at, '—')
                   FROM operator_certs
                   ORDER BY fiscal_number"""
            ).fetchall()
        return _render_settings("settings_operators.html.j2", request,
                                 active="operators", operators=rows)

    @app.get("/admin/ui/settings/node", include_in_schema=False, response_model=None)
    async def settings_node(request: Request):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        with container.connect() as conn:
            rows = conn.execute(
                """SELECT fiscal_number, mode, shift_state,
                          next_lnd,
                          current_month_offline_seconds,
                          COALESCE(last_fs_ping_at, '—')
                   FROM node_state ORDER BY fiscal_number"""
            ).fetchall()
        return _render_settings("settings_node.html.j2", request,
                                 active="node", nodes=rows)

    @app.get("/admin/ui/settings/dps", include_in_schema=False, response_model=None)
    async def settings_dps(request: Request):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        cfg = container.config
        return _render_settings(
            "settings_dps.html.j2",
            request,
            active="dps",
            crypto_provider=cfg.crypto.provider,
            crypto_sidecar_url=_redact_url_userinfo(cfg.crypto.sidecar_url),
            rest_port=cfg.ingress.rest.port,
            maria304_enabled=cfg.ingress.maria304.enabled,
            # shared_token redacted — show presence only (review finding).
            maria304_token_configured=bool(cfg.ingress.maria304.shared_token),
            maria304_auto_open_shift=cfg.ingress.maria304.auto_open_shift,
        )

    @app.get("/admin/ui/documents/{document_id}", include_in_schema=False, response_model=None)
    async def document_detail(
        request: Request,
        document_id: str,
    ):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        with container.connect() as conn:
            doc = FiscalDocumentRepository.get_by_id(conn, document_id)
        if doc is None:
            raise HTTPException(status_code=404, detail="Document not found")
        return _render("document_detail.html.j2", request, doc=doc)

    @app.get("/admin/ui/documents/{document_id}/receipt.html",
             include_in_schema=False, response_model=None)
    async def document_receipt_html(request: Request, document_id: str):
        redirect = _require_auth(request)
        if redirect:
            return redirect
        with container.connect() as conn:
            ctx = build_render_context(conn, document_id)
        if ctx is None:
            raise HTTPException(status_code=404, detail="Document not found")
        lines = format_receipt(ctx)
        return HTMLResponse(content=render_html(lines))


__all__ = ["register_admin_ui"]
