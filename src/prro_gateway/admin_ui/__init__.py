"""Admin UI package — session-authenticated operator console.

Self-contained HTML (no CDN / Google Fonts / external assets) so the
UI works on isolated networks typical for retail POS deployments.
"""
from .routes import register_admin_ui

__all__ = ["register_admin_ui"]
