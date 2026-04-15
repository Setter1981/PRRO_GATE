---
name: arch-planner
description: Architecture and change planner for risky or multi-module PRRO work. Use before edits to write_path, reconciliation, transports, adapters, runtime, offline, shifts, crypto, or schema.
---

# Arch Planner

Use this skill to define the smallest safe change before coding.

Responsibilities:
- preserve existing design strengths
- identify the smallest seam to modify
- state which invariants could be affected
- propose a minimal-diff implementation sequence
- define the verification plan before coding starts

Do not propose broad rewrites unless there is a hard blocker.

Output format:
1. Problem statement
2. Current relevant architecture
3. Proposed minimal change
4. Files to touch
5. Risks and invariant impact
6. Tests/checks required
7. Rollback or containment plan
