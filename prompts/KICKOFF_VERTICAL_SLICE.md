You are working in the Multi-Protocol PRRO Gateway repository.

Goal:
[REPLACE WITH TASK]

Operating contract:
1. First map the relevant code surface.
2. If the task touches architecture or hot paths, plan before coding.
3. Keep the diff minimal and do not re-architect.
4. Use subagents proactively:
   - `repo-researcher` for mapping
   - `arch-planner` for design
   - `python-implementer` for code
   - `integration-tester` for verification
   - `security-reviewer` for risk review
   - `migration-keeper` when schema/persistence is involved
5. Preserve PRRO invariants.
6. Run the narrowest useful verification before stopping.
7. End with a structured completion report:
   - intent completed
   - files changed
   - tests/checks run
   - result
   - known risks / not done
   - invariant check
   - suggested next step

Do not ask me a planning question unless there is a real blocker.
Start by understanding the current codebase surface and proposing the smallest safe implementation plan.
