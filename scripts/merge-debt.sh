#!/usr/bin/env bash
# merge-debt.sh — surface DONE-BUT-UNMERGED work so an interrupted session
# can't lose a whole milestone (e.g. M4-W4-Z3 sat unmerged + invisible on a
# separate worktree for 9 days; the offline_fiscal_no guard for 3 weeks).
#
# Run at the START of a working session (and before deleting any branch):
#   bash scripts/merge-debt.sh
#
# Integration target = origin/rust-gateway (where milestones merge via PR).
set -u
INT="${1:-origin/rust-gateway}"
export PATH="/home/setter/.cargo/bin:$PATH"

git fetch origin rust-gateway --quiet 2>/dev/null || true

echo "############ MERGE-DEBT AUDIT  (integration = $INT) ############"

echo
echo "=== WORKTREES (each + commits ahead of $INT) ==="
git worktree list --porcelain | awk '/^worktree /{wt=$2} /^branch /{b=$3; print wt"\t"b}' \
| while IFS=$'\t' read -r wt br; do
    br_short="${br#refs/heads/}"
    n=$(git rev-list --count "$INT".."$br_short" 2>/dev/null || echo "?")
    flag=""; [ "$n" != "0" ] && [ "$n" != "?" ] && flag="   <-- UNMERGED WORK"
    printf "  %-40s %-45s %s ahead%s\n" "$wt" "$br_short" "$n" "$flag"
  done

echo
echo "=== LOCAL branches ahead of $INT (unmerged) ==="
any=0
for b in $(git for-each-ref --format='%(refname:short)' refs/heads/); do
  n=$(git rev-list --count "$INT".."$b" 2>/dev/null || echo 0)
  [ "$n" != "0" ] && { printf "  %-50s %s ahead\n" "$b" "$n"; any=1; }
done
[ "$any" = "0" ] && echo "  (none)"

echo
echo "=== STASHES (work hides here too — a W4-Z4 candidate fix sat here) ==="
git stash list 2>/dev/null | head -20
[ -z "$(git stash list 2>/dev/null)" ] && echo "  (none)"

echo
echo "=== UNCOMMITTED in this worktree ==="
git status --short | grep -vE '^\?\?' | head -20
u=$(git status --short | grep -c '^??')
echo "  (+ $u untracked paths)"

echo
echo "NOTE: a branch ahead is not always real work — use 'git cherry $INT <branch>'"
echo "      ('+' = patch NOT in integration) before deleting any branch."
echo "############################################################"
