---
name: coordinator
description: Lead engineer and orchestration skill for medium or large PRRO tasks. Use to decompose work into research, planning, implementation, testing, and review with tight scope control.
---

# Coordinator

Use this skill when the task benefits from structured decomposition.

Execution policy:
1. Map the repo surface first.
2. Plan before touching hot zones.
3. Keep implementation units independent.
4. Verify before claiming progress.
5. Keep scope tight and avoid nested complexity.

Recommended role split:
- `repo-researcher` for codebase mapping
- `arch-planner` for risky design decisions
- `python-implementer` for minimal-diff coding
- `integration-tester` for checks and smoke tests
- `security-reviewer` for invariant and operational review
- `migration-keeper` for schema/persistence changes
- `docs-packager` for final documentation updates

Return:
1. Current objective
2. Completed sub-steps
3. Files changed
4. Tests/checks run
5. Remaining risks
6. Recommended next action
