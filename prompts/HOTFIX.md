Treat this as a production-grade hotfix.

Goal:
[REPLACE WITH BUG OR SYMPTOM]

Rules:
- optimize for the smallest safe fix
- do not broaden scope
- first localize the fault precisely
- add or update a regression test if feasible
- run only the checks needed to prove the fix
- return the exact failure mechanism, the patch, and residual risk

Use read-only research first, then minimal implementation, then targeted verification.
