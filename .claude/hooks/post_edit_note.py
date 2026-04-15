#!/usr/bin/env python3
import json
import sys

try:
    payload = json.load(sys.stdin)
except Exception:
    sys.exit(0)

tool_input = payload.get("tool_input", {}) or {}
path = str(tool_input.get("file_path", "")).replace("\\", "/")

high_risk_tokens = [
    "write_path.py",
    "reconciliation.py",
    "transports/",
    "repositories/",
    "alembic",
    "offline",
    "shift",
    "node_state",
]

if any(tok in path for tok in high_risk_tokens):
    print(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": (
                "A high-risk backend file was modified. Before stopping, run targeted tests "
                "and explicitly state how PRRO invariants were preserved."
            )
        }
    }))
