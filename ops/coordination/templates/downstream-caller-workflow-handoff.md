# Handoff template — downstream claim-lint caller workflow

For the PR that gives a downstream repo (`nycn`, `icn-learn`,
`icn-community-bridge`) its claim-firewall caller. Fill the angle brackets;
keep the non-goals verbatim.

---

**Repo:** `InterCooperative-Network/<repo>` · **Branch:** `chore/claim-lint-caller`
**Stack:** `<stack_id>` (anchor: Refs `<org/repo#NNNN>`)

**Scope (exactly this, nothing else):**

1. `.github/workflows/claim-lint.yml` — a ~12-line caller invoking icn's
   `.github/workflows/reusable-claim-lint.yml@<icn-ref>` (warning-mode by
   construction; the reusable side never fails the caller).
2. Optional `.claim-lint.json` naming this repo's claim-sensitive dirs.

**Non-goals / non-claims:**

- No blocking mode. The linter warns; it does not gate.
- No content rewrites, no lock changes, no envelope changes in this PR.
- Adding the caller adopts the *linter*, not any claim it scans: it is not
  evidence that any readiness statement in this repo is true.
- The stack manifest stage flips to `adopted` only after this workflow exists
  on the repo's default branch — not when this PR opens.

**Acceptance:**

- Workflow syntax valid (`gh workflow view` after push, or actionlint).
- The caller run completes on a PR in this repo and surfaces linter output
  as warnings.
- PR body follows `stack-pr-body-template.md`; `Refs` only, no close keywords.
