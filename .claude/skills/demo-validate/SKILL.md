---
name: demo-validate
description: Validate demo flows end-to-end. Confirms branch, ports, and services before running demo scripts.
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
---

Validate demo flows end-to-end. Avoids wrong-port / wrong-branch mistakes.

## Steps

1. Preflight: confirm branch, repo root, and active services/ports:
   - `git branch --show-current`
   - `ss -tlnp | head -20`
   - `docker ps --format "table {{.Names}}\t{{.Ports}}\t{{.Status}}"` (if using devnet)
2. Confirm expected port(s) by reading the demo config/README:
   - Check `docker-compose*.yml` or `scripts/demo-*.sh` for port assignments
   - Verify expected ports match running services
3. Run the demo validation script(s):
   - `scripts/demo-runner.sh` or individual `scripts/demo-flow-*.sh`
4. If failure:
   - Capture logs (gateway logs, daemon logs, docker logs)
   - Fix only what blocks the demo (no scope creep)
   - Re-run until green

## Output

Pass/fail per demo section + the exact command used for each.
