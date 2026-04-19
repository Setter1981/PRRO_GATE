"""
Probe real DPS endpoint for TCP/TLS keep-alive behaviour.

Measures:
  1. TCP connect time
  2. TLS handshake time (cold)
  3. ALPN (HTTP/1.1 vs HTTP/2)
  4. Keep-alive duration: how long server holds idle TLS sockets open

Approach:
  - Opens one TLS connection
  - Sleeps in increasing increments
  - Probes socket liveness between sleeps (non-destructive peek)
  - Reports last surviving idle duration as keep-alive timeout estimate

Does NOT send HTTP requests — avoids HTTP/2 vs HTTP/1.1 complications and
avoids any application-level traffic against a production-adjacent endpoint.
"""

from __future__ import annotations

import argparse
import io
import json
import socket
import ssl
import sys
import time
from dataclasses import asdict, dataclass, field


IDLE_STEPS_SEC = [1, 5, 15, 30, 60, 120, 300, 600]


@dataclass
class Report:
    endpoint: str
    timestamp: str
    tcp_connect_ms: float = 0.0
    tls_handshake_ms: float = 0.0
    tls_version: str = ""
    cipher: str = ""
    alpn: str = ""
    cert_subject: str = ""
    cert_issuer: str = ""
    cert_valid_days_left: int = 0
    idle_checks: list[dict] = field(default_factory=list)
    keepalive_timeout_sec: float | None = None
    tls_reuse_handshake_ms: float = 0.0
    errors: list[str] = field(default_factory=list)


def utf8_stdout():
    """Force UTF-8 on Windows console so Cyrillic cert fields don't blow up."""
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass


def open_tls(host: str, port: int, insecure: bool, session: ssl.SSLSession | None = None):
    ctx = ssl.create_default_context()
    if insecure:
        ctx.check_hostname = False
        ctx.verify_mode = ssl.CERT_NONE
    ctx.set_alpn_protocols(["h2", "http/1.1"])

    t0 = time.perf_counter()
    raw = socket.create_connection((host, port), timeout=10)
    t_connect = time.perf_counter()
    ssock = ctx.wrap_socket(
        raw,
        server_hostname=host if not insecure else None,
        session=session,
    )
    t_tls = time.perf_counter()
    return ssock, (t_connect - t0) * 1000, (t_tls - t_connect) * 1000


def is_alive(ssock: ssl.SSLSocket) -> tuple[bool, str]:
    """Non-destructive-ish check: use select to see if socket is readable,
    which for an idle connection means the peer has closed it.

    On TLS sockets MSG_PEEK is disallowed, so we use select + conditional recv.
    """
    import select

    try:
        # Check if there's data waiting (or FIN pending)
        r, _, _ = select.select([ssock.fileno()], [], [], 0)
        if not r:
            # No data waiting → socket is idle and alive
            return True, "idle_ok"

        # Something pending — could be close_notify or HTTP/2 PING
        ssock.settimeout(0.5)
        try:
            data = ssock.recv(4096)
        except ssl.SSLWantReadError:
            return True, "ssl_want_read"
        if not data:
            return False, "peer_EOF"
        return True, f"data_pending({len(data)}b)"
    except (ConnectionError, OSError) as e:
        return False, f"err:{type(e).__name__}:{e}"
    finally:
        try:
            ssock.settimeout(10)
        except OSError:
            pass


def cert_info(ssock: ssl.SSLSocket) -> tuple[str, str, int]:
    """Returns (subject_CN, issuer_CN, days_left)."""
    c = ssock.getpeercert()
    if not c:
        return "", "", 0
    def cn(dn):
        for item in dn or []:
            for k, v in item:
                if k == "commonName":
                    return v
        return ""
    subject = cn(c.get("subject"))
    issuer = cn(c.get("issuer"))
    days = 0
    if c.get("notAfter"):
        try:
            exp = time.mktime(time.strptime(c["notAfter"], "%b %d %H:%M:%S %Y %Z"))
            days = int((exp - time.time()) / 86400)
        except Exception:
            pass
    return subject, issuer, days


def probe(host: str, port: int, insecure: bool) -> Report:
    r = Report(endpoint=f"{host}:{port}", timestamp=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()))

    print(f"=== probe {r.endpoint} ===")
    try:
        ssock, t_conn, t_tls = open_tls(host, port, insecure)
    except Exception as e:
        r.errors.append(f"cold connect failed: {e}")
        print(f"FATAL: {e}")
        return r

    r.tcp_connect_ms = t_conn
    r.tls_handshake_ms = t_tls
    r.tls_version = ssock.version() or ""
    cipher_tuple = ssock.cipher()
    r.cipher = cipher_tuple[0] if cipher_tuple else ""
    r.alpn = ssock.selected_alpn_protocol() or "(none)"
    r.cert_subject, r.cert_issuer, r.cert_valid_days_left = cert_info(ssock)
    tls_session = ssock.session  # for session-resumption handshake timing

    print(f"TCP connect:    {t_conn:.1f} ms")
    print(f"TLS handshake:  {t_tls:.1f} ms  [{r.tls_version} / {r.cipher}]")
    print(f"ALPN:           {r.alpn}")
    print(f"Cert subject:   {r.cert_subject}")
    print(f"Cert issuer:    {r.cert_issuer}")
    print(f"Cert days left: {r.cert_valid_days_left}")
    print()

    # Idle probing on the same socket
    print(f"Idle-probe schedule: {IDLE_STEPS_SEC} sec")
    last_surviving = 0.0
    for idle in IDLE_STEPS_SEC:
        print(f"  idle {idle:4d}s ...", end="", flush=True)
        time.sleep(idle)
        alive, note = is_alive(ssock)
        r.idle_checks.append({"idle_sec": idle, "alive": alive, "note": note})
        print(f" alive={alive}  ({note})")
        if not alive:
            break
        last_surviving = idle

    r.keepalive_timeout_sec = last_surviving
    try:
        ssock.close()
    except Exception:
        pass

    # Second connection with session resumption
    print()
    print("Measuring TLS session reuse...")
    try:
        ssock2, t_conn2, t_tls2 = open_tls(host, port, insecure, session=tls_session)
        r.tls_reuse_handshake_ms = t_tls2
        reused = getattr(ssock2, "session_reused", False)
        print(f"  TCP connect:       {t_conn2:.1f} ms")
        print(f"  TLS handshake:     {t_tls2:.1f} ms  (reused={reused})")
        if reused:
            saved = r.tls_handshake_ms - t_tls2
            print(f"  TLS session-reuse saves: {saved:.1f} ms per connection")
        try:
            ssock2.close()
        except Exception:
            pass
    except Exception as e:
        r.errors.append(f"session resume test failed: {e}")

    return r


def main() -> int:
    utf8_stdout()
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", required=True)
    ap.add_argument("--port", type=int, default=443)
    ap.add_argument("--insecure", action="store_true")
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    r = probe(args.host, args.port, args.insecure)

    print()
    print("=== summary ===")
    for e in r.errors:
        print(f"  ERROR: {e}")
    print(f"  cold_tls_handshake:   {r.tls_handshake_ms:.1f} ms")
    if r.tls_reuse_handshake_ms:
        print(f"  resumed_handshake:    {r.tls_reuse_handshake_ms:.1f} ms")
    if r.keepalive_timeout_sec is not None:
        if r.keepalive_timeout_sec == 0:
            print(f"  keepalive:            server dropped within 1s of idle")
        else:
            nxt = IDLE_STEPS_SEC[IDLE_STEPS_SEC.index(int(r.keepalive_timeout_sec)) + 1] if int(r.keepalive_timeout_sec) in IDLE_STEPS_SEC and IDLE_STEPS_SEC.index(int(r.keepalive_timeout_sec)) < len(IDLE_STEPS_SEC) - 1 else None
            if nxt:
                print(f"  keepalive:            >= {r.keepalive_timeout_sec:.0f}s, < {nxt}s")
            else:
                print(f"  keepalive:            >= {r.keepalive_timeout_sec:.0f}s (ceiling of test)")

    if args.out:
        import os
        os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
        with open(args.out, "w", encoding="utf-8") as f:
            json.dump(asdict(r), f, indent=2, default=str, ensure_ascii=False)
        print(f"  full report:          {args.out}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
