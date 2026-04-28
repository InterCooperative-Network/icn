# RFC / ADR Readiness Audit (post-refinery)

> **Purpose.** The idea refinery now exists ([`ops/ideas/`](../ideas/README.md), merged via #1667). Some recent RFCs and ADRs were drafted *before* the refinery layer, when every new concept tended to be promoted directly to RFC or ADR. This audit reclassifies each artifact against the refinery's promotion thresholds. No artifact is deleted. The thinking is preserved; the institutional gravity is adjusted to match the actual evidence.

**Date:** 2026-04-28
**Scope:** ADR-0021 through ADR-0034; RFC-0000, RFC-0001, RFC-0015; `ops/coordination/rfc_candidates.yaml`; `ops/coordination/adr_candidates.yaml`.
**Hard rules** (from `ops/ideas/README.md`):

1. Accepted RFC does not mean implemented.
2. Accepted ADR does not mean implemented.
3. Future map does not mean backlog commitment.
4. Public claim requires evidence.
5. Backfill ADRs are acceptable only when tied to code, tests, merged PRs, or accepted process.
6. Forward-looking ADRs must not decide details still speculative.
7. Do not delete the thinking. Lower the gravity until the idea earns promotion.

This audit changes **no runtime behavior**, **no public website claims**, and **does not add or remove any ADR or RFC document**. It adds a `readiness` annotation to specific rows of `adr_candidates.yaml` where the gravity is wrong, adds a small number of idea cards in `ops/ideas/ideas.yaml` for the framing work needed before forward-looking ADRs should advance to accepted, and produces this audit document for institutional memory.

---

## Audit table

| Artifact | Current status | Recommended classification | Action | Reason | Evidence | Next artifact |
|---|---|---|---|---|---|---|
| ADR-0021 CCL Determinism, Fuel, Capability Safety | accepted / implemented | **keep_accepted_backfill** | leave as-is | Records shipped behavior of `icn-ccl` and executors. | `icn/crates/icn-ccl`, executor tests | none |
| ADR-0022 CCL Schema Bridge | accepted / implemented | **keep_accepted_backfill** | leave as-is | Records shipped schema-bridge in `icn-ccl/src/schema/bridge.rs`. | code path | none |
| ADR-0023 CCL Institutional Process Language | proposed / proposed | **split_principle_from_runtime** | add `readiness: needs_framing_brief`; demote runtime details to idea card | Principle ("CCL grows process surface") is reasonable; runtime details (Process DAG, step-kind taxonomy, deadline-as-data, escalation-as-step) are speculative with no shipping evidence. ADR is too detailed for the evidence it has. | none — type sketches only | idea card `idea-0012` (CCL process layer — framing brief) |
| ADR-0024 Institution Package Manifest Schema | proposed / partial | **split_principle_from_runtime** | add `readiness: needs_framing_brief` | Principle (packages declare a manifest) is settled; `BootstrapSeedManifest` ships seed primitives. Proposed sections (indicators, signal rules, allocation templates, locale metadata, package-level operating intent) are speculative and bundle several different design spaces. | partial: `icn/crates/icn-governance/src/bootstrap.rs` for seed primitives only | idea card `idea-0013` (manifest schema extensions — split into per-section briefs) |
| ADR-0025 Institutional Effect Record Canonical Schema | proposed / proposed | **demote_to_idea_or_framing** | add `readiness: needs_framing_brief`; add idea card | No `EffectRecord` ships today — only mandates. Proposed schema (closed taxonomy, plain-language summary, persistence model, reversal pointer) is forward design without enough shape to decide as an ADR. | none | idea card `idea-0014` (EffectRecord canonical schema — framing) |
| ADR-0026 Receipt & Provenance Proof Envelope | accepted / partial | **keep_accepted_backfill** | leave as-is | Honest backfill of shipped pieces (ADR-0008 receipts, ADR-0011 truth ownership, ADR-0019 mandate persistence, PR #1648 federation provenance). `partial` is honest; the envelope is real where shipped. | merged PRs cited; receipt tests | none |
| ADR-0027 Action Card Contract | proposed / partially implemented | **keep_proposed_adr** with status update | leave decision; refresh `implementation_status` text to reflect 3 shipped paths | The contract (member-facing derived view, no mutation API, prioritization in policy) is settled and partially shipped. The doc honestly scopes "vertical slice; proof-loop verified for proposal/vote". As of #1663/#1666, two more paths shipped: `action_item/complete` and `meeting/attend`. RFC-gated paths (`signal_rule`, `obligation_lifecycle`) remain pending. The status string is now stale. | shipping: PRs #1627 / #1660 / #1661 / #1663 / #1666 | optional small status-update PR (out of scope here) |
| ADR-0028 Accessibility Baseline | proposed / proposed | **split_principle_from_runtime** | add `readiness: needs_split` | Principle ("accessibility is a participation floor") is non-negotiable and well-founded. The five-layer floor (sensory / cognitive / linguistic / economic / temporal) is right framing. Runtime details (per-layer implementation contracts, glossary endpoint plumbing, mobile bandwidth model) are speculative. | partial: `docs/design-language/accessibility.md` (design doctrine), open #1610, #1611, #1366 | idea card `idea-0015` (accessibility runtime details — per-layer briefs) |
| ADR-0029 Conflict Resolution Object Model | proposed / partial | **split_principle_from_runtime** | add `readiness: needs_split` | Principle ("institutional care is first-class; three conflict shapes") is solid and useful as a reference. Compute-dispute path ships. Institutional-conflict object model (EffectChallenge, etc.) is type sketches in `icn-governance/src/appeal.rs` only. | shipping: `icn-ccl/src/disputes.rs`. Type sketch only: `icn-governance/src/appeal.rs` | idea card `idea-0016` (institutional conflict object model — framing) |
| ADR-0030 Compute Workload Manifest | accepted / implemented | **keep_accepted_backfill** | leave as-is | Backfills shipped `icn-compute` workload manifest and authority boundary. | code | none |
| ADR-0031 Commons Compute Admission & Settlement | accepted / implemented | **keep_accepted_backfill** | leave as-is | Backfills the settlement engine producing balanced journal entries. | code + tests | none |
| ADR-0032 Website Truth Boundary | accepted / implemented | **keep_accepted_backfill** | leave as-is | Records the canonical-truth-surface decision. Already cited by the cooperative-domain-infrastructure stack and the PR-stack protocol. | `website/src/`, ADR-0032 itself, PR_STACK_PROTOCOL.md | none |
| ADR-0033 Public Maturity Claims & Evidence Links | proposed / proposed | **keep_proposed_adr** with readiness note | add `readiness: needs_implementation_evidence` | Decision direction is clear (every banded claim cites evidence). Linter is unbuilt. Lower the gravity until the linter exists; do not pretend the convention is enforced. | partial: existing badge components carry some evidence by hand | optional follow-up issue to scope the linter |
| ADR-0034 ADR Candidate Registry as Architectural Memory | accepted / implemented | **keep_accepted_backfill** | leave as-is | Backfills the registry pattern itself. The registries exist and are exercised. | `ops/coordination/*.yaml` | none |
| RFC-0000 RFC Process | accepted | **keep_accepted_backfill** | leave as-is | The process is in use. | `ops/coordination/README.md` cites it | none |
| RFC-0001 Obligation / Allocation / Settlement Primitives | draft | **keep_draft_rfc** | leave as-is | Real unresolved design space with multiple options, real tradeoffs, recommended direction (Option B: external bridge receipts), bounded scope. Exactly the kind of design space RFCs exist for. | RFC structure, ADR-0004/0005/0013/0026/0031 cross-refs | proceed via the RFC's own process |
| RFC-0015 Public Surface and Learning Repo Architecture | draft | **supersede_or_amend** for canonical-surface section; **keep_draft_rfc** for learning-repo direction | add a status note in the RFC that ADR-0032 is the accepted policy for the canonical-truth-surface decision; the learning-repo / `learn.icn.zone` direction remains exploratory | The canonical-truth-surface decision (intercooperative.network) was settled by ADR-0032 (accepted). The RFC was written *before* that ADR landed and so reads as if it is governing on that point. The learning-repo direction (`learn.icn.zone` as a separate repo) was realized by `icn-learn` and is now exploratory framing for the future site. | ADR-0032 (accepted); merged icn-learn repo | optional small RFC amendment PR (out of scope here) |

---

## Findings by category

### Accepted / backfill — okay

These ADRs record shipped reality. They are tied to code, tests, or accepted process. They stay as-is.

- ADR-0021 CCL Determinism, Fuel, Capability Safety
- ADR-0022 CCL Schema Bridge
- ADR-0026 Receipt & Provenance Proof Envelope
- ADR-0030 Compute Workload Manifest
- ADR-0031 Commons Compute Admission & Settlement
- ADR-0032 Website Truth Boundary
- ADR-0034 ADR Candidate Registry
- RFC-0000 RFC Process

### Draft RFC — okay

RFC-0001 is exactly the kind of design space the RFC layer exists for: real tradeoffs, multiple viable options, a recommended direction, follow-up ADRs scoped explicitly, regulatory-safe vocabulary preserved. Leave alone. Continue via RFC process.

### Proposed ADR — okay with readiness annotation

- **ADR-0027 Action Card Contract.** Decision is settled; vertical slice is shipping. Refresh the `implementation_status` text in a small follow-up; do not change the decision.
- **ADR-0033 Public Maturity Claims & Evidence Links.** Decision is clear, linter is unbuilt — `readiness: needs_implementation_evidence` annotation in the candidate registry, no doc change.

### Demote to idea / framing

- **ADR-0025 Institutional Effect Record Canonical Schema.** No shipping evidence; schema details speculative. Add idea card `idea-0014` for the framing-brief work needed; lower the registry's gravity with `readiness: needs_framing_brief`.

### Split principle from runtime

- **ADR-0023 CCL Institutional Process Language.** Principle (CCL grows process surface) reasonable; runtime details (DAG semantics, step kinds, deadline-as-data, escalation-as-step) speculative.
- **ADR-0024 Institution Package Manifest Schema.** Principle settled; speculative sections (indicators, signal rules, allocation templates, locale metadata) bundle several distinct design spaces.
- **ADR-0028 Accessibility Baseline.** Principle (participation floor; five layers) non-negotiable; per-layer runtime contracts speculative.
- **ADR-0029 Conflict Resolution Object Model.** Principle (institutional care first-class; three conflict shapes) solid; institutional-conflict object model details speculative.

### Amend / supersede

- **RFC-0015 Public Surface and Learning Repo Architecture.** Canonical-surface section is superseded by ADR-0032 (accepted); learning-repo direction remains exploratory framing. Recommended: small future PR adds a status note in the RFC pointing to ADR-0032. Out of scope for this audit PR.

---

## Concrete recommendations (per artifact)

For each artifact whose gravity is wrong, this audit recommends one of the following — not direct edits to the ADR/RFC bodies:

| Artifact | Action this PR | Action future PR |
|---|---|---|
| ADR-0023 | `readiness: needs_framing_brief` in `adr_candidates.yaml`; idea-0012 added | optional: amend ADR with explicit "principle vs runtime" demarcation |
| ADR-0024 | `readiness: needs_framing_brief` in `adr_candidates.yaml`; idea-0013 added | per-section follow-up briefs |
| ADR-0025 | `readiness: needs_framing_brief` in `adr_candidates.yaml`; idea-0014 added | none until framing brief lands |
| ADR-0027 | none (decision is fine) | optional small status-text update reflecting #1663/#1666 |
| ADR-0028 | `readiness: needs_split` in `adr_candidates.yaml`; idea-0015 added | per-layer runtime briefs |
| ADR-0029 | `readiness: needs_split` in `adr_candidates.yaml`; idea-0016 added | object model framing brief |
| ADR-0033 | `readiness: needs_implementation_evidence` in `adr_candidates.yaml` | scope the build-time linter as a separate issue |
| RFC-0015 | none (registry already lists it as drafted) | small RFC amendment PR adding ADR-0032 supersession note |

---

## What this PR does NOT change

- **No ADR or RFC body is edited.** Every existing ADR and RFC retains its current status, content, and `implementation_status`. The thinking is preserved.
- **No runtime change.**
- **No public website change.** ADR-0032 is unaffected; the website truth boundary stands.
- **No new ADR or RFC is added.**
- **No GitHub issue is filed by this PR.** Future framing-brief work is captured in `ideas.yaml` only.
- **No existing idea card is rewritten.** New cards (idea-0012 through idea-0016) are added.
- **No private NYCN/Summit data is referenced.** This audit operates on ICN canonical material only.

---

## Hygiene notes for future audits

- Run this audit after any tranche of new ADRs lands. The pattern that produced the gravity mismatch (drafting an ADR before a framing brief exists) is recurring; this audit pattern is the corrective.
- When `readiness: needs_framing_brief` or `needs_split` is set on a row, the corresponding idea card carries the actual next-transform work.
- A `readiness` annotation is a *gravity dampener*. It does not invalidate the ADR; it tells the reader the doc is downstream of work that has not happened yet.

---

## References

- [`ops/ideas/README.md`](../ideas/README.md) — refinery doctrine, statuses, kinds, promotion thresholds.
- [`ops/coordination/README.md`](README.md) — pipeline (ideas → RFC → ADR → issues → tests → website).
- [`ops/coordination/PR_STACK_PROTOCOL.md`](PR_STACK_PROTOCOL.md) — cross-repo merge order.
- [ADR-0032](../../docs/adr/ADR-0032-website-truth-boundary.md), [ADR-0033](../../docs/adr/ADR-0033-public-maturity-claims-and-evidence-links.md), [ADR-0034](../../docs/adr/ADR-0034-adr-candidate-registry-as-architectural-memory.md).
