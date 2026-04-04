---
name: pr-create
description: Create a PR following ICN conventions with invariants checklist, proper commit format, and change routing verification
argument-hint: "[base-branch]"
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
truth_contract:
  canonical_sources:
    - ops/state/truth/policy.json       # required checks, branch naming conventions
    - ops/state/config/repo-map.json    # workspace root
  live_load_required:
    - "git branch --show-current"
    - "git diff --name-only $(git merge-base HEAD origin/main)..HEAD"
  examples_only: []
  never_hardcode:
    - branch name
    - base branch (always confirm with git)
    - required check list
---

Create a pull request following ICN conventions.

## Steps

### 1. Analyze changes

Run these in parallel:
- `git status` (untracked files)
- `git diff --stat origin/main...HEAD` (change summary)
- `git log --oneline origin/main..HEAD` (commits on branch)
- `git diff origin/main...HEAD` (full diff)

### 2. Run verification

Before creating the PR, run the appropriate checks based on changed files (follow /verify routing). If checks fail, fix issues first.

### 3. Classify the change

Determine the type: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `ci`
Determine the scope from ICN scopes: `core`, `identity`, `trust`, `net`, `gossip`, `ledger`, `ccl`, `governance`, `compute`, `gateway`, `rpc`, `store`, `obs`, `federation`, `privacy`, `security`, `crypto`, `sdk`, `web`, `deploy`

### 4. Create PR with template

Use `gh pr create` with this body structure:

```markdown
## Summary

<1-2 sentence description>

## Changes

- <bullet list of changes>

## Motivation

<why this change is needed>

## Testing

- [ ] Unit tests added/updated
- [ ] Integration tests pass
- [ ] Manual testing: <describe>

## Invariants

- [ ] No panics introduced in protocol paths
- [ ] Determinism preserved
- [ ] Canonical encodings unchanged (or versioned)
- [ ] Trust gates not weakened
- [ ] Kernel/app boundaries respected

## Verification

```
<paste verification command output summary>
```

## Documentation

- [ ] Docs updated (or N/A)
- [ ] API changes reflected in OpenAPI (or N/A)
```

### 5. Commit message format

Title: `<type>(<scope>): <description>` (under 70 chars)

### Important

- Base branch defaults to `main` unless `$ARGUMENTS` specifies otherwise
- Push the branch with `-u` flag before creating PR
- Include `Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>` in commits
- Never create PRs without running verification first
