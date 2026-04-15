# Claude Code autonomy kit for PRRO Gateway

This kit is a **project-scoped operating system** for Claude Code, tuned for a high-autonomy engineering workflow with low operator intervention.

It is designed around four principles:

1. **Keep the main session lean.**  
   Use the main thread as the lead engineer. Push high-volume exploration, testing, and review into subagents so their verbose output stays out of the main context.

2. **Optimize for verified progress, not raw speed.**  
   Every delivery must end with exact changed files, tests/checks run, known risks, and explicit confirmation that PRRO invariants were preserved or intentionally changed.

3. **Use autonomy only inside a trusted sandbox.**  
   The included default settings are intentionally balanced. Fully autonomous `auto` mode is provided only as a **local example** because Claude Code does not read `autoMode` from shared project settings.

4. **Preserve the architecture.**  
   This repo already has a strong core. The system prompt, skills, and agents are all biased toward minimal-diff changes, vertical slices, invariant preservation, and staged rollout rather than re-architecture.

---

## Recommended deployment model

### Balanced daily mode
Use this for routine work.

- Project settings from `.claude/settings.json`
- Your local session model: `opusplan` or `sonnet`
- Permission mode: `acceptEdits`
- Sandbox enabled
- Main thread stays default Claude Code
- Claude delegates to the subagents automatically

### High-autonomy mode
Use this **only** in a container, VM, devcontainer, or disposable worktree.

Copy `.claude/settings.local.auto.example.json` to `.claude/settings.local.json`, adjust the prose rules, then launch Claude Code normally.

This is best for:
- long refactors
- batch fixes
- repeated test/fix cycles
- overnight execution in a safe environment

### Never use on a credential-rich host
Do not use unattended high-autonomy sessions on a machine that has:
- production SSH keys
- cloud admin credentials
- production kube contexts
- live customer data
- direct access to real DPS / tax / production systems

---

## File layout

- `.claude/CLAUDE.md` — persistent project instructions
- `.claude/settings.json` — shared balanced configuration
- `.claude/settings.local.auto.example.json` — local-only autonomous configuration
- `.claude/settings.local.balanced.example.json` — local-only quality-first configuration
- `.claude/agents/` — specialized subagents
- `.claude/skills/` — reusable task playbooks and architectural rules
- `.claude/hooks/` — lightweight guardrails
- `prompts/` — operator prompts for common workflows

---

## Suggested working pattern

### For a medium or large change
1. Start Claude Code in the repo root.
2. Paste `prompts/KICKOFF_VERTICAL_SLICE.md`.
3. Let Claude research and plan first.
4. Review the plan once.
5. Approve implementation.
6. Let Claude run until the Stop hook forces a structured completion report.
7. Bring the result back for human review.

### For a hotfix
Use `prompts/HOTFIX.md`.

### For intermediate review
Use `prompts/INTERMEDIATE_REVIEW.md`.

### For acceptance hardening
Use `prompts/ACCEPTANCE_GATE.md`.

---

## Operator policy

The operator should intervene only at these points:

- approve a plan if the change is non-trivial
- reject or refine an implementation if it violates invariants
- decide whether to expand scope
- decide whether a result is good enough to merge

Everything else should be delegated to the agent stack.

---

## Tailoring the kit

You should update these before sustained use:

- secret paths in `.claude/settings.json`
- trusted domains in `.claude/settings*.json`
- test commands and module-specific smoke commands in `.claude/CLAUDE.md`
- hook policies in `.claude/hooks/pre_tool_guard.py`
- agent descriptions so automatic delegation matches your team style

---

## Launch examples

### Balanced
```bash
claude
```

### Quality-first
Copy `.claude/settings.local.balanced.example.json` to `.claude/settings.local.json`, then:
```bash
claude
```

### Autonomous in a safe environment
Copy `.claude/settings.local.auto.example.json` to `.claude/settings.local.json`, then:
```bash
claude --enable-auto-mode
```

### One-shot orchestration session
```bash
claude -p "$(cat prompts/KICKOFF_VERTICAL_SLICE.md)"
```
