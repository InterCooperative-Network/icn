---
id: "0020"
title: "Institutional Bootstrap Activation and Standing Read Model"
status: "accepted"
date: "2026-04-26"
deciders: ["Matt Faherty"]
tags: ["bootstrap", "activation", "standing", "institution-package", "charter", "person-directory", "authority-scope", "nycn-dogfood"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "partially implemented (bootstrap → charter activation → standing → action cards vertical slice landed; signal-rule and obligation-lifecycle source paths still pending)"
references:
  - "ADR-0011 (Canonical Truth Ownership)"
  - "ADR-0014 (Constitutional Object Model)"
  - "ADR-0017 (Monorepo Consolidation)"
  - "ADR-0018 (ADR Lifecycle)"
  - "ADR-0019 (Authority Grant Minting Seam)"
  - "docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md"
  - "PR #1586 (generic institution bootstrap package path)"
  - "PR #1617 (bootstrap-apply 409 idempotency)"
  - "PR #1621 (persistent governance domains across gateway restart)"
  - "PR #1624 (live charter activation endpoint)"
  - "PR #1626 (person-directory overlay for bootstrap role assignment)"
  - "PR #1627 (GET /me/standing read model)"
  - "PR #1630 (authority_scope plumbed through assign_role end-to-end)"
---

# ADR 0020: Institutional Bootstrap Activation and Standing Read Model

## Status

**Accepted (2026-04-26).** Records the reusable activation path that an institution package follows from manifest-on-disk to a member-facing standing surface on a running ICN node. Separates ICN's **runtime contract** (the activation pipeline, the standing read model) from the **institution package layer** (local vocabulary, fixtures, real-DID overlays).

## Context

ICN now ships a working institutional-operability runtime: a package can be applied to a node, a charter can be activated against that node, real DIDs can be bound to package-side person ids, role assignments carry typed authority scope, and the member-facing `/me/standing` read model surfaces what a holder is allowed to do today. NYCN dogfoods this end-to-end (see [InterCooperative-Network/nycn](https://github.com/InterCooperative-Network/nycn) — institution package, person-directory overlays, `/me/standing` smoke verifier).

The pieces are in code; what is not yet recorded as a decision is **the contract between the institution package layer and the ICN runtime**. NYCN's package shape, fixture content, and verifier tools have been growing without a written rule for "what does ICN promise to consume, and what stays on the package side?" That ambiguity is the same drift hazard ADR-0011 (canonical truth ownership) and the institution-package boundary doc were filed against. Without an ADR locking the activation path, package authors will start expecting whatever the gateway happened to do this week.

Two pressures forced the decision now:

1. **NYCN as reference institution package** is shipping fixtures (signals, indicators, action-card templates, temporary authority, support institutions — see NYCN PR #7) ahead of the corresponding ICN runtime primitives (icn#1631, #1632, #1633, #1634, #1635). The package marks those `icn_target_status: planned`; that discipline is correct, but it depends on a stable ICN-side activation contract for the things that *are* implemented today. This ADR provides the stable spine.

2. **Action cards (icn#1608)** are about to ship and will be the next member-facing surface plugged into this same path. Recording the activation chain now means action cards plug into a written contract instead of an inferred one.

## Decision

Institution package activation follows a single reusable path. Each step has a written contract surface; ICN owns each surface; institution packages bind their local vocabulary to it.

### Activation chain

```text
[1] package manifest (institution/package.yaml + bootstrap fixtures)
        ↓
[2] private operator overlay (real DIDs, contact bindings — never in public repo)
        ↓
[3] bootstrap apply (icnctl institution bootstrap apply)
        ↓
[4] live charter activation endpoint (POST /v1/charters/...)
        ↓
[5] entity / structure / role creation (governance domains persisted across restart)
        ↓
[6] /me/standing read model (member-facing standing surface)
        ↓
[7] action cards (FUTURE — icn#1608)
```

### Contract surfaces (ICN owns; packages bind to)

| # | Surface | ICN owns | Package owns | Status |
|---|---|---|---|---|
| 1 | Package manifest schema (entity / structure / activity / role primitives, `operating_purpose` metadata) | The four-primitive ontology; the `BootstrapEntityRecord` shape; `operating_purpose: Option<String>` as the only sanctioned home for local taxonomy | The manifest content (which entities, which structures, which `purpose` strings) | implemented (`icn-governance::bootstrap`) |
| 2 | Private overlay binding | The expectation that real DIDs bind through a private overlay outside the public repo; the placeholder-id-to-DID resolution pattern | The overlay format, the storage location, the operator workflow | partially implemented; NYCN ships templates and the `tools/export-private-activation.py` resolver |
| 3 | Bootstrap apply path | `icnctl institution bootstrap apply`, idempotency on re-runs (`PR #1617`), persistent governance domains across restart (`PR #1621`) | Calling the CLI / API at activation time | implemented |
| 4 | Live charter activation endpoint | `POST /v1/charters/...` taking ratified charter content and binding it to a governance domain (`PR #1624`) | The charter document content (CCL YAML or equivalent) | implemented |
| 5 | Entity / structure / role creation | `BootstrapEntityRecord` parsing, person-directory overlay for DID binding (`PR #1626`), `authority_scope` plumbed end-to-end through `assign_role` (`PR #1630`) | Per-package ids, per-package authority scope strings | implemented |
| 6 | `/me/standing` read model | The endpoint shape, the standing schema, the smoke contract (`PR #1627`) | Smoke verifier on the package side that asserts each holder's expected standing matches the live response | implemented; NYCN ships `tools/verify-standing.py` against the contract |
| 7 | Action cards | `GET /v1/gov/me/action-cards` returning a generic `ActionCard` shape (id, source_kind, action_kind, scope, title, summary, authority_basis, required_authority_scope, deadline, risk_level, accessibility_hint, receipt_expected, source_id, domain_id) | Action-card template catalog, holder-side rendering | partially implemented (icn#1646 vertical slice). `proposal`/`vote`, `meeting`/`attend`, and `action_item`/`complete` source paths emit cards. The full proof loop `standing → action card → authorized action → receipt` is verified end-to-end **for the `proposal`/`vote` source path only** — the close path persists a `GovernanceDecisionReceipt` (ADR-0026 Layer 2) keyed by `proposal_id`, which is the same string the action card carries as `source_id`. The `meeting`/`attend` and `action_item`/`complete` source paths still advertise `receipt_expected: true` but their write paths are not yet receipt-bearing; tracked as follow-up. `signal_rule` and `obligation_lifecycle` are reserved enum variants (gating issues icn#1631 and icn#1634). NYCN's `institution/action-card-templates.example.yaml` remains the package-side template catalog. |

### Boundary rules

- **ICN owns runtime and standing contract.** The activation pipeline shape, the persistence guarantees, and the `/me/standing` schema are runtime contracts. Institution packages may not redefine them.
- **Institution packages own local vocabulary and fixtures.** Per-coop role names, per-program structures, package-specific `operating_purpose` strings, indicator definitions, signal rules, support-institution patterns. None of these are ICN core primitives. They live in the package; they do not infect ICN core.
- **Private overlays bind real DIDs and sensitive data outside the public repo.** Real names, emails, phone numbers, demographic data, payment identifiers, and pre-commit sponsor pipelines never enter the public package repo. The activation path resolves them at apply time from operator-controlled overlay files.
- **Action cards are future surface.** No package may claim `icn_target_status: supported` for action cards until icn#1608 ships and a stable schema is published.
- **NYCN-specific nouns stay on the NYCN side.** ICN core does not learn what a "Summit," a "sponsor tier," a "education-onboarding cohort," or a "community contract amendment" is. The package layer translates those into ICN's four entity primitives + structure + activity + role records.

### What `/me/standing` promises today (the read-model contract)

The `/me/standing` response is the canonical answer to "what am I currently authorized to do, in which entities, under what scope?" for the calling DID. Its current shape:

- The caller's DID and the entities they are a member of (with `operating_purpose` metadata).
- For each membership, the role assignments held, with their `authority_scope` strings.
- For each role assignment, the structure (committee / working_group) the role is held in.

The schema is the runtime contract. NYCN's smoke verifier (`tools/verify-standing.py`) reads its expectations from `institution/standing-smoke-test.example.yaml` and asserts a superset match against the live response. The contract is `match_mode: superset` — ICN may add fields to the response without breaking package-side verifiers; ICN may not remove or rename fields without superseding this ADR.

### What does NOT cross the contract

- **Domain semantics.** ICN does not know what "education-onboarding completion" means. It knows `Activity { kind: Program, parent_entity_id: X }` and `RoleAssignment { authority_scope: [...] }`. The `operating_purpose` and authority-scope strings are opaque to ICN.
- **Real personal data.** Names, emails, contact channels, accommodation needs are never in the public package or in the activation request body. They live in the operator overlay.
- **Federation-side recognition.** A package activated on coop A is not automatically visible to coop B. Cross-cooperative recognition is governed by federation primitives (ADR-0012, ADR-0013) and is independent of this ADR.

## Implementation status

| Step | Status | Evidence |
|---|---|---|
| 1. Manifest + bootstrap fixtures | implemented | `icn/crates/icn-governance/src/bootstrap.rs` (`BootstrapEntityRecord`, `BootstrapEntityType` enum, `operating_purpose: Option<String>`); NYCN `institution/package.yaml` |
| 2. Private overlay binding | implemented | NYCN `tools/export-private-activation.py`; `summit/2026/sponsors.private.example.yaml` template |
| 3. Bootstrap apply (idempotent) | implemented | `icn/bins/icnctl/src/institution_bootstrap.rs`; PR #1586 (generic path), PR #1617 (idempotency) |
| 4. Live charter activation endpoint | implemented | `POST /v1/charters/...` (PR #1624) |
| 5. Entity / structure / role creation, persistent | implemented | PR #1621 (persist domains across restart), PR #1626 (person-directory overlay), PR #1630 (authority_scope plumbing) |
| 6. `/me/standing` read model | implemented | PR #1627; NYCN `tools/verify-standing.py` |
| 7. Action cards | partially implemented (vertical slice) | `GET /v1/gov/me/action-cards` handler at `icn/apps/governance/src/http/handlers.rs::get_my_action_cards`; runtime types in `icn/apps/governance/src/http/models.rs::ActionCard*`; JSON schema at `docs/contracts/institution-package/action-card.schema.json` (`x-icn-status: rfc`); integration coverage at `icn/apps/governance/tests/me_action_cards.rs`. End-to-end proof-loop coverage for the `proposal`/`vote` source path at `icn/apps/governance/tests/me_action_card_receipt_chain.rs` — pins that the action card's `source_id` equals the `proposal_id` on the persisted `GovernanceDecisionReceipt` (ADR-0026 Layer 2). icn#1646 closes once the reserved `signal_rule` (icn#1631) and `obligation_lifecycle` (icn#1634) source paths land **and** receipt-bearing paths exist for `meeting`/`attend` and `action_item`/`complete`. |

## Consequences

- The activation path has a written contract that NYCN (and any future institution package) can pin to. Drift between "what NYCN expects" and "what ICN does" becomes detectable at the contract surface, not by reading code.
- The boundary between ICN core (runtime) and institution packages (local vocabulary) is locked. ICN does not accumulate NYCN-specific nouns; NYCN does not redefine ICN runtime contracts.
- Action cards (icn#1608) have a written place to plug in without inventing a new activation chain.
- The "private overlay" pattern is acknowledged as a first-class architectural concern: real-DID and sensitive-data binding happens outside the public package repo by design. Future operator-tooling work has a reference model.
- Future federation-side recognition work (ADR-0012/0013 territory) is explicitly out of scope here, which prevents this ADR from over-reaching into territory those ADRs already own.

## Non-decisions (out of scope)

- Action card schema, semantics, lifecycle, or rendering. Those decisions go in a separate ADR after icn#1608 lands.
- Federation-side mandate or membership recognition. ADR-0012 and ADR-0013 own that.
- The CCL charter-document grammar. Charters are inputs to step 4; their internal shape is governed elsewhere.
- Authority enforcement (kernel dispatch gating on mandates). ADR-0019 records the minting seam; enforcement gating is its own future ADR.
- Signals, indicators, temporary-authority, obligation/allocation/settlement primitives. Each is open as a separate ICN issue (#1631 / #1632 / #1633 / #1634) and will graduate via its own ADR when accepted.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Let NYCN's expectations *be* the runtime contract | Folds institution-specific vocabulary into ICN core. The whole point of the package boundary is that ICN runs on more than NYCN. |
| Skip writing this ADR — the code already implements the path | The code implements the path; the boundary contract between ICN runtime and packages is what was missing. Without a written contract, every PR risks redrawing the line. |
| Defer until action cards (icn#1608) ship | Action cards plug into this path. Recording the path *before* action cards land means action cards plug into a written contract; recording after means action cards risk redrawing the line first. |
| Bundle this ADR with ADR-0019 | Two distinct decisions (mint/persist authority records vs activate institution packages). Bundling would make either decision harder to review and harder to supersede independently. |
