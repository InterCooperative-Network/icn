# Pull Request

## Delivery

Three lines a human writes; the rest an agent keeps current. Semantics are owned by
[`ops/state/truth/delivery.json`](../ops/state/truth/delivery.json) — lane definitions, what the
states mean, and when comprehensive review ends.

- **Delivery lane**: FAST | STANDARD | DEEP <!-- DEEP is the maintainer's call -->
- **Acceptance contract**: what this PR claims to deliver.
- **Explicit non-goals**: what it deliberately does not do.

<!-- ICN-DELIVERY-LIFECYCLE:BEGIN -->
```
ICN DELIVERY LIFECYCLE
State:                IMPLEMENTING
Lane:                 <lane>
Acceptance contract:  <one line, or a pointer into the Delivery section above>
Review generation:    not yet requested
Freeze head:          -
Known blockers:       -
Follow-up ledger:     -
```
<!-- ICN-DELIVERY-LIFECYCLE:END -->

## Summary
What changed and why?

## Layer classification
Mark one (see [`ops/coordination/PR_STACK_PROTOCOL.md`](../ops/coordination/PR_STACK_PROTOCOL.md)):
- [ ] ICN core (generic primitive, kernel, runtime)
- [ ] ICN app (PolicyOracle / state model)
- [ ] NYCN package (institution-specific application)
- [ ] icn-learn (teaching)
- [ ] Public website (claim)
- [ ] ops/coordination (process, refinery, hygiene)

## Boundary check
What this PR intentionally avoids — explicit non-goals:
- Institution-specific meaning out of ICN core.
- Private operational data out of any repo.
- Public website claim without evidence (per ADR-0033).
- New ICN primitives invented by an institution package.

## What changed
Files touched and one-line reason per file (or per cluster of files).

## What did not change
Adjacent surfaces a reader might expect to change but should not.

## Related
- Issues: <!-- USE `Refs #NNNN`. NEVER use Fixes/Closes/Resolves near issue numbers unless intentionally closing. See PR_STACK_PROTOCOL.md. -->
- Specs / ADRs / RFCs: <!-- paths under docs/ -->
- Idea card (if from refinery): <!-- ops/ideas/ideas.yaml#idea-NNNN -->

## Cross-repo dependency status
If this PR is part of a multi-repo stack, name the upstream/downstream PRs and their state. Cross-repo merge order is **ICN canonical first → NYCN application → ICN Academy teaching** (per [`PR_STACK_PROTOCOL.md`](../ops/coordination/PR_STACK_PROTOCOL.md)).

- Upstream PR(s): <!-- e.g. InterCooperative-Network/icn#NNNN — merged / open / blocked -->
- Downstream PR(s): <!-- e.g. InterCooperative-Network/nycn#NN — open / queued -->
- This PR depends on upstream merging first: yes / no

## Review-thread status
Before requesting merge:
- [ ] Prior review threads resolved or marked outdated, with reason.
- [ ] PR body matches the current diff (no stale "in flight" wording).

## Work mode
- [ ] Discovery output
- [ ] Delivery tranche
- [ ] Mixed, but bounded

## Risk
What could break? Edge cases or known limitations?

## Documentation control (required when `docs/**` changes)
- [ ] New or moved docs declare **truth class** and **role** (see `docs/DOCUMENTATION_CONTROL_SYSTEM.md`; vocabulary: `normative` / `descriptive` / `operational` / `historical` / `draft`)
- [ ] `docs/registry.toml` updated (explicit `[docs."path"]` entry when defaults are wrong)
- [ ] Placement matches allowlisted `docs/` subtree (see `[control].allowlisted_docs_subdirs` in `registry.toml`)
- [ ] Control-plane canonical paths (exact set: `[control].canonical_doc_paths` — same four files as below) unchanged **or** YAML headers + merged registry row updated **together**
- [ ] Ran: `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml` (add `--strict` if you touched registry structure, canon paths, or supersession; see `docs/DOCUMENTATION_MAINTENANCE.md`)
- [ ] If `DOCUMENT_REGISTRY.md` should reflect corpus stats: same command with `--write-document-registry docs/DOCUMENT_REGISTRY.md`

## Organizer / member accessibility gate (required when an organizer/member-facing surface changes)

If this PR changes an organizer/member-facing UI, rehearsal surface, action-card surface, preview/review surface, evidence/receipt/provenance review surface, or member-facing governance flow, run the gate at [`docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) and:
- [ ] Attach or link the completed §5 checklist (one outcome per category: **Pass** / **Pass with documented follow-ups** / **Blocked** / **N/A with reason**)
- [ ] State the surface-readiness conclusion (organizer-ready / member-facing / pilot-ready / steward-only-experimental)
- [ ] Open follow-up issues for any **Pass with documented follow-ups** categories before merge

This section is conditional. PRs that do not change an organizer/member-facing surface can skip it.

## Structural changes (docs migrations)
- Files added:
- Files moved:
- Files archived:
- Files marked superseded:

## Type of change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation
- [ ] Tests
- [ ] Refactor (no behavior change)

## Verification
What you actually ran (commands and result):
- [ ] `python3 docs/scripts/doc_control_check.py` (if docs touched)
- [ ] Tests / clippy / fmt as applicable

## Non-goals
What this PR intentionally avoids.

## Remaining unknowns
What still needs human review?
