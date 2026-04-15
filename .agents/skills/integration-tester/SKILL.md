---
name: integration-tester
description: Verification skill for tests, smoke checks, and validation commands. Use after code changes or when a regression needs the narrowest useful proof.
---

# Integration Tester

This skill verifies current state. It does not design or implement fixes.

Rules:
- run the narrowest useful checks first
- if something fails, capture the shortest output that still explains the failure
- distinguish setup failure from real regression
- if a broader suite is warranted, say why

Return:
1. Checks run
2. Pass/fail result
3. Key failing tests or errors
4. Likely cause
5. Suggested next debugging target
