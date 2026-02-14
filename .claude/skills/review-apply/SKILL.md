---
name: review-apply
description: Apply PR review feedback with no scope drift. Fetches comments, applies requested changes, runs gates, pushes.
argument-hint: "[PR number]"
user-invocable: true
allowed-tools: "Bash, Read, Edit, Write, Grep, Glob"
---

Apply PR review feedback. No scope drift - only address what reviewers asked for.

## Steps

1. Fetch all review comments for the current PR:
   - `gh pr view --json number,title,headRefName,baseRefName`
   - `gh api repos/{owner}/{repo}/pulls/{pr}/comments`
   - `gh api repos/{owner}/{repo}/pulls/{pr}/reviews`
   - If `$ARGUMENTS` specifies a PR number, use that instead of the current branch's PR
2. Summarize all requested changes before making any edits.
3. Apply requested changes only. Do NOT fix unrelated issues.
4. Run gates (must all pass before pushing):
   - `cd icn && cargo fmt --all --check`
   - `cd icn && cargo clippy --workspace --all-targets --all-features -- -D warnings`
   - `cd icn && cargo test --workspace`
5. Fix any failures from step 4 that were caused by changes in step 3.
6. Commit: `fix: address review feedback` (or more specific if appropriate)
7. Re-run gates from step 4 (fmt + clippy + test). Do NOT push if any fail.
8. Push to the PR branch.
9. Do NOT merge. Report status + remaining reviewer asks (if any).

## Important

- If a reviewer comment is ambiguous, ask the user before acting.
- If a comment asks for a design change that's out of scope, report it as a NOTE but do not act on it.
