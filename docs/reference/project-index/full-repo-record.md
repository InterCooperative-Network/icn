---
Status: operational
Canonical: no
Last Reviewed: 2026-05-01
---

# Full Repository Record Protocol

> This document defines how ICN records **everything in the repositories** without confusing a mechanical file inventory with architectural truth.

The living repo atlas needs two layers:

1. **Mechanical record** — every tracked file and directory, generated from Git, with stable metadata.
2. **Interpretive atlas** — what those files mean, which subsystem they belong to, whether they are current, stale, generated, private-boundary, implementation, design direction, or archive.

A repository map that only summarizes major folders is not enough. A repository map that only dumps `git ls-files` is also not enough. ICN needs both: the ledger of what exists and the institutional memory of why it exists.

## Scope

The full record covers, in order:

1. `InterCooperative-Network/icn`
2. `InterCooperative-Network/nycn`
3. `InterCooperative-Network/icn-learn` where relevant

For each repo, produce a generated file record plus a human-curated atlas section.

## Output artifacts

Generated artifacts live under:

```text
docs/reference/project-index/generated/
```

Expected generated files:

```text
docs/reference/project-index/generated/icn-file-record.json
docs/reference/project-index/generated/icn-file-record.md
docs/reference/project-index/generated/nycn-file-record.json
docs/reference/project-index/generated/nycn-file-record.md
docs/reference/project-index/generated/icn-learn-file-record.json
docs/reference/project-index/generated/icn-learn-file-record.md
```

Human-authored atlas files live beside the existing project-index maps:

```text
docs/reference/project-index/repo-atlas.md
docs/reference/project-index/capability-map.md
docs/reference/project-index/source-of-truth-map.md
docs/reference/project-index/stale-and-archived-map.md
docs/reference/project-index/tool-commons-map.md
docs/reference/project-index/nycn-package-map.md
```

## Generator

Use:

```bash
python3 scripts/generate_repo_record.py \
  --repo icn=. \
  --repo nycn=../nycn \
  --repo icn-learn=../icn-learn \
  --out docs/reference/project-index/generated
```

For a local archaeology pass that includes untracked non-ignored files:

```bash
python3 scripts/generate_repo_record.py \
  --repo icn=. \
  --out docs/reference/project-index/generated \
  --include-untracked
```

Do **not** commit untracked/local archaeology output without review. It may include private, generated, or environment-specific files.

## What the mechanical record contains

For every tracked file:

| Field | Meaning |
|---|---|
| `path` | Repo-relative path |
| `directory` | Parent directory |
| `name` | File name |
| `extension` | File suffix, or `(none)` |
| `language` | Coarse language guess from extension |
| `role_guess` | Path-based role guess |
| `size_bytes` | File size at generation time |
| `sha256` | File content hash |
| `tracked` | Whether Git tracks it |

For every directory:

| Field | Meaning |
|---|---|
| `path` | Repo-relative directory path |
| `depth` | Directory depth |
| `file_count` | Number of files under the directory recursively |
| `total_size_bytes` | Total tracked bytes under directory |
| `child_directory_count` | Number of immediate child directories detected |
| `extensions` | Extension count under directory |
| `role_guess` | Path-based role guess |

## What the interpretive atlas must add

The generator cannot know truth. The human/agent atlas must classify files and directories by meaning:

| Classification | Use when |
|---|---|
| `implemented` | Current code or docs match shipped behavior |
| `implemented but partial` | Real surface exists but has known gaps |
| `feature-gated` | Exists behind feature flag or compile/runtime gate |
| `docs-only/design-direction` | Describes future or intended behavior, not shipped behavior |
| `generated` | Machine-generated artifact; do not hand-edit |
| `test-only` | Test fixture, test helper, or verification artifact |
| `ops-only` | Deployment, runbook, monitoring, CI, or operator surface |
| `package-local` | Institution-specific package data, not ICN core |
| `private-boundary` | Must not expose private organizer/member/sponsor/contact/source data |
| `stale/historical` | Useful history but not current truth |
| `contradicted by current code` | Explicitly wrong relative to current source |
| `unknown / needs local verification` | Cannot be classified safely from current evidence |

## First-pass directory families for `icn`

The `icn` repository should be interpreted through these families:

| Family | Paths |
|---|---|
| Repo control | `README.md`, `AGENTS.md`, `CONTRIBUTING.md`, `CLAUDE.md`, `.github/` |
| Rust workspace | `icn/`, `icn/crates/`, `icn/apps/`, `icn/bins/` |
| Legacy/top-level app surface | `apps/` |
| Documentation control plane | `docs/`, `docs/registry.toml`, `docs/scripts/`, `docs/reference/project-index/` |
| ADR/RFC corpus | `docs/adr/`, `docs/rfcs/`, `ops/coordination/` |
| Public website | `website/` |
| Demo/member UI surfaces | `web/pilot-ui/`, `web/dashboard/`, `web/api-docs/` |
| SDKs | `sdk/typescript/`, `sdk/react-native/` |
| Institution packages | `institutions/`, `institutions/nycn/` |
| Contracts and examples | `contracts/`, `examples/` |
| Deployment and operations | `deploy/`, `ops/`, `monitoring/`, `docker/`, `config/`, `scripts/` |
| Simulations | `sims/` |
| Archives / historical material | `docs/archive/`, older `docs/planning/`, older issue waves |

## File-record review procedure

For each repo:

1. Generate the mechanical record.
2. Commit the generated JSON and Markdown only if they do not include private or oversized material.
3. Read directory summaries first.
4. Assign directory-level classifications.
5. Drill down into suspicious or high-importance file clusters.
6. Identify stale docs, duplicated concepts, unsafe vocabulary, and generated artifacts.
7. Promote stable findings into the human-authored atlas files.
8. Keep generated records clearly marked as generated snapshots.

## Privacy and safety boundary

Do not expose private organizer, sponsor, attendee, member, contact, Drive, email, credential, key, or infrastructure-secret data.

The NYCN repo must be treated with stricter review than the public `icn` repo. If a generated record includes private names, emails, contact data, raw meeting material, sponsor details, or credentials, do not commit it. Instead, commit only directory-level summaries or redacted records.

## Relationship to the living atlas issue

Issue `#1689` remains the live control document. Generated records are snapshots. Human-authored atlas docs are stable reviewed interpretations. If they disagree, prefer this order:

1. Current source code and tests
2. `docs/STATE.md` / `docs/PHASE_PROGRESS.md`
3. Current ADRs/RFCs
4. Generated file records
5. Human-authored atlas docs
6. Historical archives

## Definition of done

The full repo-record effort is done only when a contributor can answer:

1. What files and directories exist in each repo?
2. Which are source, docs, generated, ops, tests, packages, or archives?
3. Which directories are current truth and which are historical?
4. Which files define shipped behavior?
5. Which files describe future/design-direction behavior?
6. Which files are institution-specific and must not leak into ICN core?
7. Which files are private-boundary sensitive?
8. Which files are stale, duplicated, or contradicted?
9. Which components need follow-up issues or PRs?
10. How can the record be regenerated later without redoing archaeology by hand?
