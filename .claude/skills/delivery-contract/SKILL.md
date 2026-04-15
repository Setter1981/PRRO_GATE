---
name: delivery-contract
description: Structured completion contract for engineering work. Claude should apply this automatically to all substantial tasks so results are reviewable with minimal operator intervention.
user-invocable: false
---

Every substantial completion must include all of the following:

1. **Intent completed**
2. **Files changed**
3. **Behavioral effect**
4. **Tests or checks run**
5. **Result**
6. **Known risks / not done**
7. **Invariant check**
8. **Suggested next step**

Additional rules:
- do not claim “done” if verification is missing
- do not hide uncertainty
- separate verified facts from inference
- if nothing was run, say so explicitly
