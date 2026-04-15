---
name: acceptance-gate
description: Review the current diff and repository state against the current gate. Invoke directly when you want a structured go / no-go answer before moving to the next milestone.
disable-model-invocation: true
allowed-tools: Read, Grep, Glob, Bash, LSP
---

Review the current repository state against the active gate or milestone: $ARGUMENTS

Return:
1. Gate name
2. Evidence found in code/tests/docs
3. Missing pieces
4. Hard blockers
5. Confidence level
6. Recommended next step

Be conservative. This is a gate review, not a motivational speech.
