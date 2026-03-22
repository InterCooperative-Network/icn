---
name: repair-gh-workflow
description: Diagnose and fix GitHub Actions failures with branch protection, token permissions, and repo policy in mind.
argument-hint: "[workflow name | run ID | --main]"
user-invocable: true
allowed-tools: "Bash, Read, Edit"
---

Diagnose GitHub Actions failures that involve branch protection, token permissions, or workflow design.
Start with the protection model, not with the error message alone.

## The Problem This Solves

The transcript's `Sync Website Stats` cron workflow failed daily with `GH006: Protected branch update
failed`. The fix was clear once branch protection was checked: `GITHUB_TOKEN` cannot push to protected
`main` regardless of `permissions: contents: write`. That constraint should be known upfront, not
discovered after a failure has been running for days.

## Steps

### Phase 1: Identify failure context

```bash
# Find recent failures on main
gh run list --branch main --limit 10 --json status,conclusion,name,databaseId,createdAt \
  --jq '.[] | select(.conclusion == "failure") | "\(.databaseId) \(.name) \(.createdAt)"'

# Get failed job and step
gh api repos/InterCooperative-Network/icn/actions/runs/<RUN_ID>/jobs \
  --jq '.jobs[] | select(.conclusion == "failure") | {name:.name, steps:[.steps[] | select(.conclusion=="failure") | .name]}'
```

### Phase 2: Classify failure type

| Error pattern | Class | Fix direction |
|---------------|-------|---------------|
| `GH006: Protected branch update failed` | Permission | Don't push to main directly; use PAT or make non-fatal |
| `remote: Repository not found` | Auth | Token scope or GITHUB_TOKEN missing repo access |
| `Resource not accessible by integration` | Token scope | Add `permissions:` block to workflow |
| Exit code 1 on `git push` | Permission or protection | Check branch protection + token capability |
| Timeout / no logs | Runner contention | Self-hosted runner busy; not a code failure |
| Missing step output | Upstream step skipped | Check `if:` conditions in prior steps |

### Phase 3: Check branch protection before patching

Always verify the actual protection model before deciding on a fix:

```bash
gh api repos/InterCooperative-Network/icn/branches/main/protection \
  --jq '{
    required_checks: .required_status_checks.contexts,
    strict: .required_status_checks.strict,
    enforce_admins: .enforce_admins.enabled,
    required_approvals: .required_pull_request_reviews.required_approving_review_count
  }'
```

**The critical invariant for ICN main**:
- `GITHUB_TOKEN` (even with `contents: write`) cannot push directly to `main` when `required_status_checks`
  are set, because the push bypasses the check gates.
- Only a PAT from an account with admin bypass (and `enforce_admins: false`) can push directly.

### Phase 4: Apply fix by class

**Class: write-back is essential** (data must land in the repo)
→ Use a PR-based flow:
```yaml
- name: Create PR with updated stats
  run: |
    git checkout -b chore/update-stats-$(date +%Y%m%d)
    git commit -m "chore: update stats.json [skip ci]"
    git push -u origin HEAD
    gh pr create --title "chore: update stats.json" --body "Automated." --base main
```

**Class: write-back is a cache warm** (artifact regenerated elsewhere)
→ Make the push step non-fatal:
```yaml
- name: Commit if changed
  continue-on-error: true   # ← push may be rejected by branch protection; that's OK
  run: |
    git add -f path/to/generated-file
    git diff --quiet --cached || git commit -m "chore: update [skip ci]" && git push
```

**Class: write-back requires admin**
→ Add a PAT secret and use it in checkout:
```yaml
- uses: actions/checkout@v4
  with:
    token: ${{ secrets.ADMIN_PAT }}
```
Then document the secret requirement in the workflow comment. Do not add PAT secrets without user approval.

### Phase 5: Verify fix

After patching:
```bash
# Trigger a manual run to confirm
gh workflow run <workflow-name>

# Watch the run
gh run list --workflow <workflow-file> --limit 3 --json status,conclusion,databaseId \
  --jq '.[] | "\(.databaseId) \(.status) \(.conclusion // "running")"'
```

## ICN-specific workflow inventory

| Workflow | File | Failure pattern | Fix class |
|----------|------|----------------|-----------|
| `Sync Website Stats` | `sync-stats.yml` | Push to protected main | Cache warm → `continue-on-error: true` (applied PR #1393) |
| `CI` | `ci.yml` | Various; required gates | Fix code, not workflow |
| `Build and Deploy to K3s` | `deploy.yml` | Registry push / K3s apply | Infra issue; check cluster |
| `Benchmarks` | `benchmarks.yml` | Compare fails on base delta | Non-blocking; ignore unless spike |

## Guardrails

- **Never design a workflow that depends on `GITHUB_TOKEN` pushing to protected `main`.** It will fail.
- **Non-essential write-backs should always have `continue-on-error: true`.** A failed cache warm
  should never turn a cron workflow red.
- **Do not add PAT secrets without explicit user approval.** Propose it, wait for confirmation.
- **Check `enforce_admins` before assuming `--admin` works.** If `enforce_admins: true`, even admin
  merges require passing checks.
- **Workflow security**: Never interpolate untrusted GitHub event data (PR titles, issue bodies,
  commit messages) directly into `run:` commands. Always use `env:` variables with proper quoting.
