---
name: diag-infra
description: Infrastructure diagnosis with differential-first approach. Gathers evidence, ranks hypotheses, proposes cheapest test.
user-invocable: true
allowed-tools: "Bash, Read"
---

Diagnose infrastructure issues using a differential-first approach. Gather evidence FIRST, then hypothesize.

## Steps

1. Gather evidence (no fixes yet):
   - `dmesg --time-format iso | tail -120`
   - `journalctl -p err --since "24 hours ago" | tail -200`
   - `free -h`
   - `df -h`
   - `top -bn1 | head -40`
   - `ss -tlnp | head -20`
2. Produce ranked differential:
   - 3 hypotheses with evidence FOR, evidence AGAINST, and cheapest test for each
3. Propose 1 cheapest test to run next.

## Important

- Do NOT modify ICN repo code during infra diagnosis.
- Do NOT start fixing things before the differential is complete.
- If the issue appears to be ICN application-level (not infra), say so and recommend switching to a code debugging session.
