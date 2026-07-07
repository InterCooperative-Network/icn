---
Status: descriptive
Canonical: yes
Last Reviewed: 2026-07-07
---

# ICN Upstream Lock Format (`icn-upstream-lock/v1`)

Defines the lock-file format downstream repos use to record which ICN ref they were last
**reviewed against**. Generalized from the format already in production in the `nycn` repo
(`ops/upstream/icn.lock.yaml`); `icn-learn` and `icn-community-bridge` adopt the same file at the
same path when their locks are introduced.

**A lock is a review attestation, not an automated dependency constraint.** It records that a
named human reviewed the downstream repo's ICN-facing assumptions against a specific ICN commit on
a specific date. Nothing enforces the pin at build time; drift against it is *measured* (see
"Auditing drift" below) and burned down by a deliberate review, never by a silent bump.

## File location and registration

- Path in each downstream repo: `ops/upstream/icn.lock.yaml`
- Registered in icn's coordination registry: `ops/state/config/repo-map.json#org_repos`
  (`lock.path` / `lock.format`)
- Truth-spine domain: `downstream_dependency_locks` in `ops/state/truth/sources.json`
  (the lock files themselves live in the downstream repos, never in icn)

## Format (v1)

```yaml
icn:
  repo: InterCooperative-Network/icn
  pinned_ref: <full 40-char commit SHA on icn main>
  pinned_ref_short: "<8-char short SHA>"
  pinned_ref_subject: "<subject line of the pinned commit>"
  reviewed_at: <YYYY-MM-DD>
  reviewer: <human identity — a person, not an agent>
```

Field rules:

- `pinned_ref` — full SHA of a commit on `icn` `main`. Never a branch name, never a tag.
- `reviewed_at` — the date the *review* happened. Refreshing this date without re-reviewing is
  readiness laundering; do not do it.
- `reviewer` — a human signs the attestation. Agents may prepare the diff analysis; a person
  owns the judgment.
- Free-form YAML comments are encouraged for review context (what changed since the previous
  pin, what was deliberately not adopted) — the nycn lock demonstrates this style.

## Bump procedure (mirrors the nycn header)

1. Update `pinned_ref` (+ short/subject) to the new ICN main SHA.
2. Update `reviewed_at` to today; `reviewer` signs.
3. Review the downstream repo's dependency-status doc (e.g. nycn
   `docs/sync/ICN_DEPENDENCY_STATUS.md`) and update drift rows: each upstream change relevant to
   the repo is classified **reviewed / not adopted / action-needed**.
4. Land the lock bump and the dependency-status update together in one PR, `Refs`-linked —
   never a bare pin bump.

## Auditing drift

From any checkout with the local bare stores available:

```bash
git --git-dir=$HOME/icn-dev/repos/icn.git rev-list --count <pinned_ref>..origin/main
```

A large count is not itself a failure — it is unreviewed exposure. The failure mode this format
exists to prevent is a stale `reviewed_at` silently coexisting with claims that downstream
assumptions still hold.
