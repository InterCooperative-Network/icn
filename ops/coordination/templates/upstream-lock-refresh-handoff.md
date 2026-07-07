# Handoff template — upstream lock refresh

For the PR that refreshes a downstream repo's `ops/upstream/icn.lock.yaml`
(format `icn-upstream-lock/v1` — see `docs/reference/upstream-lock-format.md`
in icn). A lock is a **human-signed review attestation**, not an automated
dependency constraint: agents draft the drift review, a human signs `reviewer:`.

---

**Repo:** `InterCooperative-Network/<repo>` · **Branch:** `chore/upstream-lock-refresh`
**Stack:** `<stack_id>` (anchor: Refs `<org/repo#NNNN>`)

**Scope (exactly this, nothing else):**

1. `ops/upstream/icn.lock.yaml` — pin moves to `<icn merge commit SHA>`.
   Cite the exact upstream merge commit reviewed against, never a branch name.
2. The dependency drift memo (e.g. `docs/sync/ICN_DEPENDENCY_STATUS.md`) —
   per-surface verdicts, one of: **reviewed** / **not adopted** /
   **action-needed**. Never a bare "none blocking" without the surface list.

**Rules:**

- `reviewed_at` is the date the review actually happened; `reviewer:` is a
  human, left blank until they sign.
- Existing proof references that are no longer current get re-marked as
  historical proof artifacts (ref + date + evidence pointer) — preserved,
  never scrubbed, never re-claimed as current.
- A refreshed lock means "someone looked at the delta", not "the delta is
  adopted". Adoption is separate downstream work with its own status.

**Non-goals:** no workflow changes, no status envelope, no content rewrites,
no new claims of any kind.

**Acceptance:** lock parses; drift memo lists every reviewed surface; PR body
follows `stack-pr-body-template.md`; `Refs` only, no close keywords.
