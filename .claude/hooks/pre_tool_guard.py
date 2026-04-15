#!/usr/bin/env python3
import json
import re
import sys

def emit(decision=None, reason=None, additional=None):
    out = {"hookSpecificOutput": {"hookEventName": "PreToolUse"}}
    if decision:
        out["hookSpecificOutput"]["permissionDecision"] = decision
    if reason:
        out["hookSpecificOutput"]["permissionDecisionReason"] = reason
    if additional:
        out["hookSpecificOutput"]["additionalContext"] = additional
    print(json.dumps(out))
    sys.exit(0)

try:
    payload = json.load(sys.stdin)
except Exception:
    sys.exit(0)

tool = payload.get("tool_name", "")
tool_input = payload.get("tool_input", {}) or {}

if tool == "Bash":
    cmd = str(tool_input.get("command", "")).strip().lower()

    deny_patterns = [
        r"\brm\s+-rf\s+/\b",
        r"\brm\s+-rf\s+\.git\b",
        r"\bgit\s+push\s+--force\b",
        r"\bgit\s+reset\s+--hard\b",
        r"\bgit\s+clean\s+-fdx\b",
        r"\bcurl\b.*\|\s*(bash|sh)\b",
        r"\bwget\b.*\|\s*(bash|sh)\b",
        r"\bshutdown\b",
        r"\breboot\b",
    ]
    ask_patterns = [
        r"\bgit\s+push\b",
        r"\bdocker\b",
        r"\bkubectl\b",
        r"\bterraform\b",
        r"\balembic\s+upgrade\b",
        r"\bpsql\b",
        r"\bmysql\b",
    ]

    for pat in deny_patterns:
        if re.search(pat, cmd):
            emit(
                "deny",
                "Blocked by project guardrail: destructive or unsafe shell command.",
                "Use a safer local-only alternative or ask for an explicit operator decision."
            )

    for pat in ask_patterns:
        if re.search(pat, cmd):
            emit(
                "ask",
                "High-impact command. Require explicit review or a clearly safe local target.",
                "Check that the command is local, reversible, and within the approved task scope."
            )

    sys.exit(0)

if tool in {"Edit", "Write"}:
    path = str(tool_input.get("file_path", ""))
    hot = [
        "write_path.py",
        "reconciliation.py",
        "alembic",
        "transports/",
        "repositories/",
        "offline",
        "shift",
        "node_state",
        ".claude/settings",
    ]
    if any(token in path.replace("\\", "/") for token in hot):
        emit(
            "allow",
            "High-risk file path detected.",
            "You are touching a PRRO hot zone. Keep the diff minimal, preserve invariants, and run targeted tests before stopping."
        )

sys.exit(0)
