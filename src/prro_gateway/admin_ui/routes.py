"""Admin UI routes — login + dashboard + documents.

Mounted via `register_admin_ui(app, container)` from the REST app
factory when `config.admin_ui.enabled` is true.  Session auth through
`starlette.middleware.sessions.SessionMiddleware` with an HMAC-signed
cookie.
"""
from __future__ import annotations

import hmac
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
_MIN_SESSION_SECRET_LEN = 16


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
