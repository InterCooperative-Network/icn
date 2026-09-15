#!/bin/bash
# Hook: PreToolUse session-startup health check
# Runs ONCE per session (temp-file guard). Checks tool dependencies
# required by blocking and advisory hooks.
#
# Exit codes:
#   0 — all critical deps present (or already checked this session)
#   2 — critical dependency missing; blocking hooks would silently pass

# Only run once per session (keyed to shell PID parent)
PARENT_PID=$(ps -o ppid= $$ 2>/dev/null | tr -d ' ')
GUARD_FILE="/tmp/.icn-hook-health-done-${PARENT_PID:-$$}"
if [[ -f "$GUARD_FILE" ]]; then
  exit 0
fi

WARNINGS=""
CRITICAL=""

# Critical: jq — required by all stdin-parsing hooks (firewall-guard, panic-guard,
# scope-guard, dep-guard, todo-guard, openapi-sync-guard). Without jq, both blocking
# hooks (firewall-guard, panic-guard) silently pass and advisory hooks do not function.
if ! command -v jq &>/dev/null; then
  CRITICAL="${CRITICAL}\n  CRITICAL: jq not found — blocking hooks (firewall-guard, panic-guard) will silently pass; advisory hooks will not function"
fi

# Critical: git — required by scope-guard.sh, pre-bash-guard.py
if ! command -v git &>/dev/null; then
  CRITICAL="${CRITICAL}\n  CRITICAL: git not found — scope-guard.sh and branch checks will fail"
fi

# Advisory: cargo — needed for build verification
if ! command -v cargo &>/dev/null; then
  WARNINGS="${WARNINGS}\n  WARN: cargo not found — build verification unavailable"
fi

# Advisory: rg (ripgrep) — used by advisory hooks and search workflows
if ! command -v rg &>/dev/null; then
  WARNINGS="${WARNINGS}\n  WARN: rg (ripgrep) not found — some advisory checks degraded"
fi

# Advisory: gh — used by PR workflows
if ! command -v gh &>/dev/null; then
  WARNINGS="${WARNINGS}\n  WARN: gh (GitHub CLI) not found — PR workflows unavailable"
fi

# Mark as checked for this session
touch "$GUARD_FILE"

# Report results
if [[ -n "$CRITICAL" ]]; then
  echo -e "HOOK HEALTH: BLOCKING DEPENDENCIES MISSING${CRITICAL}" >&2
  if [[ -n "$WARNINGS" ]]; then
    echo -e "${WARNINGS}" >&2
  fi
  echo "Install missing critical tools before proceeding. Blocking hooks cannot enforce without them." >&2
  # Remove guard so re-check happens after install
  rm -f "$GUARD_FILE"
  exit 2
fi

if [[ -n "$WARNINGS" ]]; then
  # Emit advisory as JSON systemMessage so Claude sees it
  echo "{\"continue\":true,\"systemMessage\":\"Hook health: some advisory tools missing.${WARNINGS}\"}"
fi

exit 0
