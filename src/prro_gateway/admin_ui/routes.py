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

from fastapi import FastAPI, Form, HTTPException, Request
from fastapi.responses import HTMLResponse, RedirectResponse
from jinja2 import Environment, FileSystemLoader, select_autoescape
from starlette.middleware.sessions import SessionMiddleware

from ..repositories.fiscal_documents import FiscalDocumentRepository

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
                          min_offline_codes, max_offline_codes
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
                                min_offline_codes, max_offline_codes
                           ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                        (
                            form["fiscal_number"], form["tax_number"], form["fiscal_mode"],
                            1 if form["tsp_enabled"] == "1" else 0,
                            1 if form["offline_enabled"] == "1" else 0,
                            form["org_name"] or None,
                            form["point_address"] or None,
                            int(form["min_offline_codes"]),
                            int(form["max_offline_codes"]),
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


__all__ = ["register_admin_ui"]
