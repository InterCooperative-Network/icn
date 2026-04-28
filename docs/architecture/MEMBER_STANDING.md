---
Status: design contract
Canonical: no
Authority: architecture (complements THE_COMMONS.md and KERNEL_APP_SEPARATION.md)
Last Reviewed: 2026-04-24
Owner: Matt Faherty
---

# Member Standing and Accessible Participation

> **`/me/standing` is ICN's canonical answer to: who am I, where do I belong, what can I do, under whose authority, in which scope right now, with what duties waiting, and what do I need in order to understand any of this?**
>
> It is the substrate under the Commons Shell. It is the substrate under Action Cards. It is the substrate under accessible and assisted participation. It is not a convenience endpoint.

This document is the design contract for `GET /me/standing`. It sits directly beneath `docs/architecture/THE_COMMONS.md` and inherits from it: The Commons doctrine says ordinary people must be able to participate in Commons institutions without becoming sysadmins; this document specifies the surface that makes that possible. It complements `docs/architecture/KERNEL_APP_SEPARATION.md` (the Meaning Firewall) and `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` (institution packaging), and takes the `[future design]` marker from `THE_COMMONS.md §9` and turns it into a concrete, implementation-ready contract.

**This is design-only.** No endpoint is authorized for implementation here. Implementation happens in subsequent phases, each with its own PR.

---

## Purpose

`GET /me/standing` is the canonical answer to a set of questions a member should be able to ask once and receive one coherent response to:

- **Who am I?** What cryptographic identity is the caller, and what individual entity does that identity correspond to?
- **Where do I belong?** Which entities (communities, cooperatives, federations) list me as a member, and in what role?
- **What can I do?** What capabilities do I hold, broken down by the scope they apply in?
- **Under what authority?** Which of my capabilities come from membership, which from role assignments in structures, which from explicit authority grants, which from mandates tied to specific decisions, which from delegations held from other individuals?
- **Which hat am I wearing right now?** What is my currently selected active scope? What scopes could I select? What changes when I do?
- **What needs my attention?** What action items, pending votes, upcoming meetings, expiring grants, or membership decisions are assigned to me or open in scopes where I can act?
- **Under which institution am I acting at this moment?** The canonical id, human display label, any aliases in use, and the charter context.
- **What do I need in order to understand any of this?** Plain-language summaries, glossary keys, translations, screen-reader structure, low-bandwidth fallbacks.

`/me/standing` exists so that every member-facing surface — the Commons Shell, Action Cards, the mobile wallet, the pilot PWA, a library-terminal session, a CLI summary, an assisted-participation kiosk — can draw from a single authoritative view of the caller's participatory position. It is not a dashboard feed, not an admin query, not an identity profile, and not a replacement for existing primitives. It is the join point.

> **If `/me/standing` fails, every accessible participation surface above it fails.** This document treats it accordingly.

---

## Relationship to The Commons

`docs/architecture/THE_COMMONS.md` establishes the doctrine: ICN is infrastructure for The Commons; The Commons is plural; the kernel must not bake one social doctrine into its mechanics; the **Meaning Firewall** is how ICN remains usable by Commons institutions that disagree about meaning.

That doctrine implies a specific obligation for the member-facing surface:

- A Commons Shell must give ordinary people — not operators, not developers, not trained admins — a legible way to act inside the institutions they belong to.
- Legibility requires a unified view of standing. Without it, every screen reconstructs authority ad hoc from `RoleAssignment`, `Membership`, `AuthorityGrant`, `Mandate`, and JWT scopes, and each reconstruction drifts.
- The unified view must be **reporting**, not **defining**. It must not invent institutional meaning; it must render what the charters, proposals, grants, and mandates already say.

`/me/standing` is that unified view. It is the bridge from kernel/governance/entity primitives to human-facing participation. It crosses the Meaning Firewall only in one direction: the endpoint computes from typed substrate data, hands the result to the shell, and the shell renders human meaning. The kernel still sees only `ConstraintSet`, `PolicyDecision`, `Capability`, and typed invariants; the endpoint adds no new semantic authority.

> A member should be able to see their standing without becoming literate in ICN's type system. The type system is the *source* of the answer, not the *shape* of the answer.

---

## Relationship to NYCN Bootstrap Reality

The NYCN institutional bootstrap track is the first real pressure test of the entity / structure / program / activity / milestone model. Its lesson for `/me/standing` is concrete and non-negotiable:

1. **Integration tests passing is not the same as live validation.** The repo's integration tests for entities, structures, and proposals pass. Live validation against an actual gateway on port 8085 (after port 8080 conflict diagnosis) has already surfaced route/runtime drift — `apply` completed 5 operations and then 404'd on `/v1/gov/entities/{entity_id}/structures`. That is not a kernel-level invariant violation. It is the gap between typed repo primitives and their routed, escaped, gateway-wired representation.
2. **Member-facing standing must be grounded in live, routable, explicit institutional state.** If `/me/standing` returns role assignments in structures whose routes are 404ing, the member cannot act on them. The endpoint must not paper over route/schema drift; it must surface it as a warning the Commons Shell can render.
3. **Entity IDs contain colons by construction.** The format `entity:icn:<type>:<slug>` (see `icn/crates/icn-entity/src/entity.rs`) means every live path segment containing an entity id must be URL-encoded correctly or the route must be designed to avoid raw-id embedding. This is the proximate class of failure behind PR #1591 (colon-safe proposal index keys) and likely behind the bootstrap 404. `/me/standing` must emit canonical ids **and** route-safe representations, and never trust display labels or aliases for authority decisions.
4. **Aliases from bootstrap apply must be legible but non-authoritative.** NYCN's bootstrap binds aliases like `nycn`, `nycn-organizers`, `summit-cycle-2026`, `summit-2026` to canonical entity ids. `/me/standing` must render the alias for humans and return the canonical id for actions. Aliases are UX; canonical ids are authority.

`/me/standing` is not responsible for fixing the live 404. That work belongs to the `feat/live-nycn-bootstrap-validation` track. But this design absorbs the lesson: a member-facing contract specified only at the type level is insufficient. The contract must include the distinction between canonical id, route-safe representation, alias, display label, and translated label, and it must include warnings for the ways those can drift.

NYCN is used here only as the first concrete example. Identifiers like `nycn`, `nycn-organizers`, `summit-cycle-2026`, and `summit-2026` are illustrative. They are not baked into core semantics and must not be. The generic shape is `entity:icn:<type>:<slug>`; NYCN is one instance.

---

## Non-Goals

`/me/standing` is deliberately scoped. It does not:

- **Implement the endpoint.** This document specifies the contract; implementation is a separate phase.
- **Fix the live NYCN bootstrap 404.** That is a route / runtime concern for `feat/live-nycn-bootstrap-validation`.
- **Replace `/gov/me/scopes` or `/gov/me/work`.** Those remain the authoritative feeds for role assignments and action items. `/me/standing` composes their outputs; it does not duplicate them.
- **Define the final UI.** Rendering is the Commons Shell's job. `/me/standing` is the data contract that multiple renderings can consume.
- **Define the final Action Card ranking.** That is `/me/action-cards` (`[future design]`), which consumes standing + open work + deadlines.
- **Create a global identity hierarchy.** There is no top. `/me/standing` returns the caller's position in a plural, federated landscape, not a place in a global org chart.
- **Centralize membership meaning.** Charters define what membership means. `/me/standing` reports what the charter-backed primitives say; it does not interpret them.
- **Bypass charters.** Every capability returned by `/me/standing` traces to a charter-backed primitive: `Membership`, `RoleAssignment`, `AuthorityGrant`, `Mandate`, `Delegation`. If a capability is not grounded there, it must not appear.
- **Weaken capability checks.** `/me/standing` reports what the caller *could* do at the authorization layer; the enforcement boundary remains unchanged. Reporting is not granting.
- **Make assisted participation unrestricted.** Helpers don't gain authority by helping. The endpoint does not smuggle helper capability into the member's standing.
- **Make every private membership public.** The response is caller-centered. Admin or third-party standing queries are explicitly out of scope and deferred.
- **Turn accessibility into cosmetic UI.** Accessibility metadata is structural. It affects what the endpoint returns, not only how a downstream UI styles it.
- **Turn operator setup into a member-facing requirement.** Bind ports, keystore passphrases, JWT secrets, sled lock files, and coop-id formats are operator concerns. `/me/standing` returns nothing that requires a member to understand any of them.

---

## First Principles

Start from the human participation needs enumerated in `THE_COMMONS.md §First Principles` — belonging, agency, memory, recognition, access, voice, repair, continuity, bounded authority, transparent rules — and derive what they require from a standing surface.

### Needs → Endpoint Requirements

| Human / institutional need | Requirement on `/me/standing` |
|----------------------------|-------------------------------|
| Belonging | Identity is cryptographically anchored (`Did`) and human-readable at the surface (display label, individual entity id). Membership is queryable. |
| Agency | The endpoint is callable by the member themselves, from any device holding their key, without central-operator permission. |
| Comprehension | Every role, grant, mandate, and scope is explainable — not as raw ids, but as "what authority this gives you, from whom, until when." |
| Voice | Pending votes, proposals, and amendments the caller can act on are either surfaced here (as pointers) or reachable via linked endpoints. |
| Memory | Receipts and provenance are pointed at, not inlined. The endpoint tells the member where to verify, not what to trust. |
| Trust | Every authority basis is named by primitive type and id so the member (or a third party later) can audit the claim. |
| Repair | Expired, revoked, or ambiguous authorities are surfaced as warnings, not silently dropped. Errors are legible, not swallowed. |
| Continuity | Mandates and grants approaching expiry are surfaced with lead time. Role handoffs are visible. |
| Role clarity | Each scope is explicitly labeled and distinguishable (self vs. member vs. role-holder vs. representative vs. operator). |
| Decision clarity | Active scope is explicit. The caller can see which scope an action would execute under before they act. |
| Authority clarity | Capability is broken down by source: membership, role, grant, mandate, delegation. No "mystery admin" capability. |
| Language access | Human labels are renderable per the caller's preferred language; canonical ids are stable. |
| Disability access | Structural metadata (screen-reader-safe summary, plain-language mode, glossary keys) is first-class, not a downstream skin. |
| Device access | The payload is small enough to render on a low-end phone; there is a text-only fallback shape. |
| Low-bandwidth access | A minimal mode omits optional fields; no field is required to exceed a modest size. |
| Device-loss recovery | The endpoint does not hold state on the member's device; standing is computed server-side and the member can re-enroll on any device with key recovery. |
| Async participation | Pending work includes deadlines; the endpoint can be polled or cached and used offline with clear sync states. |
| Post-hoc verification | Every authority claim points to a receipt or primitive id a third party could re-derive. |

### Derived Requirements (blunt)

1. Identity must be both cryptographically anchored (`Did`) and human-legible (display label, canonical entity id).
2. Memberships must be queryable with role, status, and capability breakdown.
3. Roles must be explainable — not "role:coordinator" without a structure, not "authority_scope" strings without human translation.
4. Authority must trace to one of: membership, role assignment, grant, mandate, delegation. No ambient authority.
5. Active scope must be explicit in the response and reflected in every action the caller subsequently takes.
6. Permissions must be derivable per scope and understandable without consulting external docs.
7. Pending work must be visible here (pointers; details on linked endpoints).
8. Every item must be renderable as an Action Card: it has a subject, a scope, an authority basis, a next-step, and a deadline or none.
9. Plain-language summaries must exist for every authority concept.
10. Translation and glossary hooks must be first-class.
11. Offline / read-only fallback must be supported (the response must be cacheable and safely re-render without a live connection).
12. Assisted participation must leave a receipt trail.
13. Privacy boundaries must be explicit; the response is caller-centered.
14. Canonical route-level identifiers must be distinct from human labels/aliases, and both must appear in the response.

---

## Accessibility as Anti-Capture Infrastructure

> **Accessibility is not charity. Accessibility is anti-capture infrastructure.**

Every barrier to participation creates a ruling class. A technical barrier creates a sysadmin priesthood. An English-only barrier creates a linguistic priesthood. A laptop-only barrier creates a wealth priesthood. A synchronous-meeting-only barrier creates an availability priesthood. A legalese barrier creates a lawyer priesthood. An operator-shell barrier creates an operator priesthood. Each of these, over time, captures the institution by exclusion.

The Commons cannot afford to reproduce those priesthoods inside its own substrate. That means `/me/standing` — and every member-facing surface above it — must be designed for:

- **Old phones.** Three-to-five-year-old Android devices with modest RAM and slow flash.
- **Low bandwidth.** 2G-class connections, metered data, shared tethering.
- **Intermittent connectivity.** Rural, transit-based, disaster-recovery, outage-prone networks.
- **Shared devices.** Library terminals, community-center kiosks, household tablets.
- **Second-language users.** Members whose primary language is not the institution's working language.
- **Screen readers.** Full semantic structure, no meaning conveyed only by visual layout.
- **Keyboard and switch navigation.** Full interaction without pointer devices.
- **Cognitive-load reduction.** Short sentences, one idea per paragraph, glossary on demand, no hidden state.
- **Large text, high contrast, reduced motion.** Required by users with low vision, photosensitivity, or vestibular conditions.
- **Plain-language institutional explanations.** "What does it mean that you're a representative of GreenStar in NYCN?" — answerable in one sentence without jargon.
- **Translated institutional concepts, not just UI labels.** "Proposal," "quorum," "mandate," "delegation," "charter" each need a translated *definition*, not just a translated button.
- **Offline reading and later submission where safe.** Members can read their standing, compose a vote, and submit on reconnect, with clear sync state at every step.
- **Assisted participation with receipts and anti-coercion protections.** When someone helps, the help is visible in provenance, and the action is still the member's.

Accessibility is therefore a **contract-level concern** for `/me/standing`, not a downstream styling concern. The endpoint must return the data that makes accessible rendering possible: language codes, translation availability, glossary keys, screen-reader-safe summaries, low-bandwidth hints. A downstream UI can choose how to render; it cannot retroactively add data the endpoint did not expose.

---

## Operator Complexity Must Not Leak Into Member Participation

The live NYCN bootstrap validation exposed a legitimate operator surface: gateway bind ports (8080 vs. 8085), gateway JWT secret, `identity.age` keystore files, `ICN_KEYSTORE_PASSPHRASE`, `--coop-id` flags, daemon startup and shutdown, local data directories, sled lock files, route availability in a specific gateway build, gateway/auth wiring. Those are real operator concerns.

They must not become member-facing requirements.

A member should open the Commons Shell (PWA, mobile, terminal, CLI summary, kiosk) and see:

- **Identity** — who they are, in human-readable form.
- **Belonging** — which entities they are a member of.
- **Available actions** — what they can do given their current scope.
- **Pending attention** — what is waiting for them.
- **Why** — the authority basis for each capability.
- **Next steps** — what to click, who to contact, what to sign.
- **Verification** — where to check the receipt.

They must **not** see:

- Sled paths, sled locks, or storage-engine details.
- JWT secrets, signing-key material, or session internals.
- Route mount prefixes or gateway version strings.
- Raw `entity:icn:<type>:<slug>` colon-separated identifiers as the primary explanation of where they belong.
- URL escaping concerns.
- Coop-id flag formats.
- Daemon lifecycle or infrastructure state.

`/me/standing` is the firewall between the two audiences. It speaks typed substrate primitives on its input side and human-usable standing on its output side. Operator diagnostics (route drift warnings, schema-version mismatches, stale cache indicators) may appear in the response as typed warnings, but they must be framed for the member's downstream shell to render gracefully — "some information may be out of date; try again later" — not dumped as raw operator noise.

---

## Endpoint Contract: `GET /me/standing`

This section specifies the contract at the level needed to review and implement. It is deliberately restrained about routing and auth specifics; those are implementation choices constrained by what follows.

### Method and Path

`GET /me/standing`

Namespace options (pick in an implementation ADR, not here):

- **Option A — new member-facing namespace**: `/me/standing` under a new `/me/*` root. Pros: clear member-facing boundary; easily extended to `/me/action-cards` and future member endpoints. Cons: introduces a new top-level namespace.
- **Option B — under existing governance namespace**: `/gov/me/standing`. Pros: reuses the existing `/gov/me/*` convention used by `/gov/me/scopes` and `/gov/me/work`. Cons: conflates governance standing with broader participatory standing (memberships, representation mandates, operator scopes).
- **Option C — deferred**: ship the data shape under whichever existing namespace is fastest, and rename in a later phase.

**Recommendation**: Option A. The response composes governance, entity, identity, and policy data, which is broader than `/gov/*`. A new `/me/*` namespace signals to the shell team that this is *the* doorway. However, the recommendation is non-binding; the final routing decision belongs to an implementation ADR that verifies there is no conflict with existing `/me/*` routes and that gateway mounts match.

### Authentication

Standard gateway JWT-bearer auth, as used by `/gov/me/scopes` and `/gov/me/work` (`Authorization: Bearer <token>`, `require_scope` check). The caller's `Did` is parsed from the token subject.

### Caller DID Resolution

The endpoint resolves the caller's `Did` from the verified JWT and treats that as the subject of the response. There is no query parameter to ask for another member's standing in the initial design. Admin-scoped third-party queries are explicitly out of scope here and deferred to a separate design with explicit capability checks and privacy review.

### Default Behavior

`GET /me/standing` returns the caller's full current standing: memberships, role assignments, authority grants held, mandates, delegations, currently selected active scope (if any), available active scopes, effective capabilities per scope, pointers to pending work, accessibility metadata, warnings.

### Privacy Behavior

- Caller-centered: only the caller's own standing is returned.
- Entities that list the caller as a member appear in the response with the caller's role inside them — **not** with the full roster of other members.
- Federation views show the caller's representational authority, not the federation's internal structure beyond what membership confers.
- Expired or revoked items are included **if the caller had or holds them**, because surfacing revocations matters for repair. They are clearly marked.

### Error Behavior

- Unauthenticated caller: `401`.
- Authenticated caller with no memberships and no role assignments: `200` with an empty standing shape (identity + empty lists + empty warnings). Empty standing is a legitimate state.
- Internal store errors: `500` with a typed error body; the shell renders "standing temporarily unavailable" and retries.
- Partial data (e.g., memberships resolved but mandates store unreachable): `200` with populated fields **and** an explicit warning entry indicating which subsystem is stale. The shell renders "some information may be out of date"; the caller is never silently misinformed.

### Relationship to Current Active Scope

If the caller has an explicitly selected active scope (per the `[future design]` active-scope primitive — see §Active Scope), the response includes it. Otherwise, `active_scope` is either `null` or the `self` default, and the Commons Shell is expected to prompt the member to select one before authority-bearing actions.

### Relationship to `/gov/me/scopes` and `/gov/me/work`

- `/gov/me/scopes` — authoritative for role assignments. `/me/standing` composes its output in the `roles` field (see §Proposed JSON Shape) and does not duplicate its query semantics.
- `/gov/me/work` — authoritative for assigned action items. `/me/standing` returns pointers and summary counts in the `pending_work` field; the shell fetches details from `/gov/me/work`.

This separation is deliberate: `/me/standing` is the join view; the existing endpoints remain the authoritative feeds. A member-facing shell that wants live, filterable action items continues to call `/gov/me/work`; a shell that wants the one-shot standing snapshot calls `/me/standing`.

### Relationship to Federation / Member Entities

If the caller represents another entity in a federation (via a `Mandate` or an `AuthorityGrant` of class `Representation`), the response enumerates the representable entities and the grants/mandates backing them. The caller can then select a `representative` active scope to act for the represented entity. The federation's own internal composition is **not** returned; only the caller's representational position within it.

### Relationship to NYCN Bootstrap / Live Entities

NYCN entities (e.g., `entity:icn:federation:nycn`, `entity:icn:community:nycn-organizers`, `entity:icn:program:summit-cycle-2026`, `entity:icn:activity:summit-2026`) and their bootstrap-bound aliases (e.g., `nycn`, `nycn-organizers`, `summit-cycle-2026`, `summit-2026`) appear in the response like any other institutional entities. They carry both their canonical id and their display label/alias. The endpoint does not special-case NYCN, and NYCN must not appear in kernel constants. These examples are for illustration only.

---

## Proposed JSON Shape

This is the target shape. Fields that current ICN primitives can populate today are annotated `[shipped]`; fields that require additional primitives are annotated `[future design]` with a brief note.

```jsonc
{
  "subject": {
    "did": "did:icn:alice-pubkey",                                    // [shipped] icn-identity::Did
    "individual_entity_id": "entity:icn:individual:alice-pubkey",     // [shipped] icn-entity::EntityId
    "display_label": "Alice"                                          // [future design] display-label resolution; fallback to did
  },
  "active_scope": {                                                   // [future design] active-scope primitive
    "kind": "representative",                                         // self | member | role | representative | operator
    "represented_entity_id": "entity:icn:cooperative:greenstar",      // canonical id
    "entity_alias": "greenstar",                                      // optional, non-authoritative
    "display_label": "Representing GreenStar in NYCN",                // human label
    "via_grant": "grant:0xabc...",                                    // [shipped] AuthorityGrant id
    "source": "session_selected",                                     // session_selected | default_self | jwt_coop_id_fallback
    "fallback": false                                                 // true if the endpoint had to fall back to a default
  },
  "memberships": [                                                    // [shipped] icn-entity::Membership
    {
      "entity_id": "entity:icn:cooperative:greenstar",                // canonical
      "entity_alias": "greenstar",                                    // [future design] alias rendering
      "entity_display_label": "GreenStar Cooperative",                // [future design] display-label resolution
      "entity_type": "cooperative",                                   // [shipped] Entity discriminant
      "role": "Worker",                                               // [shipped] MembershipRole
      "status": "Active",                                             // [shipped] MembershipStatus
      "shares": 1,                                                    // [shipped]
      "capabilities": ["Vote", "Propose"],                            // [shipped] MembershipCapability
      "joined_at": "2025-06-01T00:00:00Z",                            // [shipped]
      "source": "charter_approved_proposal"                           // [future design] provenance hint
    }
  ],
  "roles": [                                                          // [future design] `/me/standing` projection over governance role assignments. Current `/gov/me/scopes` (`RoleAssignmentResponse`) returns only `{id, structure_id, person_did, role, start_date, end_date}` and uses raw `StructureId` strings (e.g. `struct-<uuid>`); the richer shape below requires endpoint extension or secondary resolution before it can be composed.
    {
      "structure_id": "structure:icn:committee:nycn-finance",         // [future design] normalized/canonical structure identifier; current `/gov/me/scopes` returns raw `StructureId`
      "parent_entity_id": "entity:icn:federation:nycn",               // [future design] not present in current `/gov/me/scopes`
      "structure_display_label": "NYCN Finance Committee",            // [future design] display-label resolution
      "role": "coordinator",                                          // [shipped] role value is available via `/gov/me/scopes`
      "authority_scope": ["approve-budget-<=5000"],                   // [future design] not present in current `/gov/me/scopes`; requires endpoint extension or secondary resolution
      "authority_scope_plain_language": [                             // [future design] plain-language rendering
        "Approve budget proposals up to 5000 units"
      ],
      "valid_from": "2026-02-01T00:00:00Z",                           // [future design] normalized field name; underlying `RoleAssignment.start_date: Timestamp`
      "valid_until": "2026-12-31T23:59:59Z"                           // [future design] normalized field name; underlying `RoleAssignment.end_date: Option<Timestamp>`
    }
  ],
  "grants": [                                                         // [future design] `/me/standing` projection over `icn-governance::AuthorityGrant` (ADR-0014). The shape below is an API view; stored types differ — see notes below.
    {
      "grant_id": "550e8400-e29b-41d4-a716-446655440000",              // [shipped] underlying `AuthorityGrantId(Uuid)`; bare UUID, no `grant:0x…` prefix
      "class": "Representation",                                      // [shipped] `AuthorityClass`: Representation | Execution | Attestation
      "grantor_entity_id": "entity:icn:cooperative:greenstar",        // [shipped] `GrantorEntityId`
      "grantor_display_label": "GreenStar Cooperative",               // [future design] display-label resolution
      "grantee_did": "did:icn:alice-pubkey",                          // [shipped] `Grantee`
      "scope": {                                                      // [shipped] `TypedScope` — actual fields are `domain`, `proposal_class`, `action_kind`, `amount_ceiling`, `time_window`
        "domain": "nycn-federation-gov",                              // [shipped] `TypedScope.domain`
        "proposal_class": ["Treasury", "Membership"],                 // [shipped] `TypedScope.proposal_class` (proposal-payload-variant labels)
        "action_kind": []                                             // [shipped] `TypedScope.action_kind` (Execution-class only); empty for Representation
      },
      "scope_plain_language": "Represent GreenStar when voting or proposing in NYCN federation governance", // [future design] plain-language rendering
      "valid_from": "2026-01-01T00:00:00Z",                           // [shipped] `AuthorityGrant.valid_from: Timestamp`
      "valid_until": "2026-12-31T23:59:59Z",                          // [shipped] `AuthorityGrant.valid_until: Option<Timestamp>`
      "revoked_at": null,                                             // [shipped] `AuthorityGrant.revoked_at: Option<Timestamp>`
      "capability_set": ["Vote", "Propose"]                           // [future design] derived rendering for the standing surface; not stored on `AuthorityGrant`
    }
  ],
  "mandates": [                                                       // [shipped] icn-governance::Mandate
    {
      "mandate_id": "mandate:0xdef...",
      "represented_entity_id": "entity:icn:cooperative:greenstar",
      "decision": {                                                   // DecisionProvenance
        "proposal_id": "prop:0x123...",
        "governance_domain": "greenstar-internal"
      },
      "payload_hash": "0xdeadbeef",
      "grants": ["grant:0xabc..."],                                   // links to AuthorityGrant ids above
      "executor_did": "did:icn:alice-pubkey",
      "deadline": "2026-06-30T23:59:59Z",
      "status": "Active",                                             // Active | Completed | Expired | Revoked
      "issued_at": "2026-04-01T00:00:00Z",
      "summary_plain_language": "Cast GreenStar's vote on the NYCN 2026 summit budget"
    }
  ],
  "delegations": {                                                    // [shipped] icn-governance::Delegation (1717 LOC)
    "held_from": [                                                    // delegations where caller is the delegate
      {
        "delegation_id": "del:0x111...",
        "delegator_did": "did:icn:bob-pubkey",
        "domain": "greenstar-internal",
        "kind": "domain_scoped",
        "valid_until": "2026-12-31T23:59:59Z"
      }
    ],
    "held_to": [                                                      // delegations where caller is the delegator
      {
        "delegation_id": "del:0x222...",
        "delegatee_did": "did:icn:carol-pubkey",
        "domain": "nycn-federation-gov",
        "kind": "proposal_scoped",
        "proposal_id": "prop:0x456...",
        "valid_until": "2026-05-15T23:59:59Z"
      }
    ]
  },
  "available_active_scopes": [                                        // [future design] active-scope primitive
    { "kind": "self", "label": "Acting as yourself",
      "source_primitive": "identity" },
    { "kind": "member", "entity_id": "entity:icn:cooperative:greenstar",
      "label": "Member of GreenStar",
      "source_primitive": "Membership" },
    { "kind": "role", "structure_id": "structure:icn:committee:nycn-finance",
      "label": "Finance Coordinator, NYCN",
      "source_primitive": "RoleAssignment" },
    { "kind": "representative", "represented_entity_id": "entity:icn:cooperative:greenstar",
      "via_grant": "grant:0xabc...",
      "label": "Representing GreenStar in NYCN",
      "source_primitive": "AuthorityGrant" },
    { "kind": "operator", "node_id": "node:home-01",
      "label": "Node operator: home-01",
      "source_primitive": "operator_layer" }                          // [future design] operator scope model
  ],
  "effective_scopes": [                                               // derived: scope strings the kernel would see
    {
      "scope_key": "member:entity:icn:cooperative:greenstar",
      "capabilities": ["Vote", "Propose"],
      "derived_from": ["membership:entity:icn:cooperative:greenstar"]
    }
  ],
  "pending_work": {                                                   // pointers; details via /gov/me/work
    "total": 3,
    "by_urgency": { "overdue": 1, "due_soon": 1, "later": 1 },        // [future design] urgency bucketing
    "feed": "/gov/me/work"                                            // canonical feed
  },
  "action_card_hints": {                                              // [future design] /me/action-cards
    "feed": "/me/action-cards",
    "count_by_scope": {
      "member:entity:icn:cooperative:greenstar": 2,
      "representative:entity:icn:cooperative:greenstar": 1
    }
  },
  "accessibility": {                                                  // [future design] language/glossary model
    "preferred_language": "en",
    "available_translations": ["en", "es"],
    "plain_language_mode": true,
    "glossary_keys": [
      "mandate", "delegation", "representation", "authority_grant",
      "governance_domain", "proposal", "structure"
    ],
    "screen_reader_summary": "You are Alice. You are a Worker member of GreenStar Cooperative and a coordinator on the NYCN Finance Committee. You have one representation mandate for GreenStar in NYCN, valid through December 31, 2026. You have three items pending your attention, one of which is overdue.",
    "low_bandwidth_hints": {
      "omit_optional": true,                                          // shell signals preference
      "text_only_fallback_available": true
    }
  },
  "receipts": {                                                       // [shipped] where verifiable
    "memberships_provenance": "see charter store and accepted membership proposals",
    "grants_provenance": "see AuthorityGrant store",
    "mandates_provenance": "see Mandate store and decision proposals"
  },
  "warnings": [                                                       // typed warning envelope
    { "kind": "expired_grant",    "id": "grant:0x789...", "expired_at": "2026-03-01T00:00:00Z" },
    { "kind": "ambiguous_scope",  "note": "Two representational grants for entity:icn:cooperative:greenstar overlap in domain nycn-federation-gov." },
    { "kind": "missing_translation", "language": "es", "term_keys": ["mandate"] },
    { "kind": "stale_cache",      "subsystem": "mandates", "last_fresh_at": "2026-04-23T18:00:00Z" },
    { "kind": "route_drift",      "note": "Structures index for entity:icn:federation:nycn returned 404 on last probe; capabilities may be stale." },
    { "kind": "reauth_required",  "reason": "token_near_expiry" }
  ]
}
```

Fields carrying `[shipped]` map to present primitives. Fields carrying `[future design]` are called out and enumerated in §Gaps.

---

## Join Plan

How `/me/standing` assembles its response from current repo modules. Real paths are cited. Items requiring new storage or indexing are marked.

1. **Identity lookup** — resolve caller `Did` from JWT subject.
   - Source: `icn-http-kit::auth::BasicClaims` + `icn-identity::Did` (see `icn/apps/governance/src/http/handlers.rs` pattern).
2. **Individual entity lookup** — compute `EntityId::from_did(did)` → `entity:icn:individual:<identifier>`.
   - Source: `icn/crates/icn-entity/src/entity.rs`.
3. **Memberships** — list all `Membership` records where `member_id` matches the caller's individual entity id.
   - Source: `icn/crates/icn-entity/src/membership.rs`.
   - Today: requires a `list_memberships_for_member(member_id)` query on the membership store. If that index does not exist, it is the first `[future design]` storage addition.
4. **Bootstrap-created entities and aliases** — for each membership, resolve the parent entity's canonical id, display label, and bootstrap alias (if any).
   - Source: entity store; institution-bootstrap alias binding (`icn/bins/icnctl/src/institution_bootstrap.rs`, bootstrap apply path around the structures-creation loop at `format!("/v1/gov/entities/{parent}/structures")` — which is also the site of the current live 404).
   - Today: alias binding exists at bootstrap time; a read-side "alias for canonical id" lookup must be exposed to the endpoint. `[future design]` if not already query-able.
5. **Role assignments** — reuse the existing `/gov/me/scopes` implementation: `ctx.manager.list_roles_for_person(&caller)` (`icn/apps/governance/src/http/handlers.rs`).
   - Source: shipped.
6. **Authority grants held by caller** — list `AuthorityGrant` where `grantee == caller_did` and `revoked_at is null` and `valid_until > now`.
   - Source: `icn/crates/icn-governance/src/authority.rs`.
   - Today: requires a `list_grants_for_grantee(did)` query. `[future design]` if not already indexed.
7. **Mandates where caller is executor** — list `Mandate` where `executor == caller_did` and `status != Completed|Expired|Revoked`.
   - Source: `icn/crates/icn-governance/src/mandate.rs`.
   - Today: similar index requirement; `[future design]` if not already indexed.
8. **Delegations held by caller (as delegate)** — list `Delegation` where `delegatee == caller_did` and not expired/revoked.
   - Source: `icn/crates/icn-governance/src/delegation.rs`.
9. **Delegations held from caller (as delegator)** — list `Delegation` where `delegator == caller_did` and not expired/revoked.
   - Source: same as above.
10. **Capability derivation** — for each membership, role assignment, grant, and mandate, emit derived capabilities scoped by the source primitive.
    - Deterministic transform; no new storage.
11. **Active scope / session-selected scope** — read the caller's current selection from a session-scope store.
    - `[future design]`: no session-selected-scope primitive exists today. Today's approximation is JWT `coop_id` + `scopes`. The endpoint must not invent a selection; it must report "no active scope selected" (or default to `self`) when the primitive is absent.
12. **`/gov/me/scopes` + `/gov/me/work` composition** — the existing outputs populate the `roles` and `pending_work` fields. The endpoint calls the same manager methods (`list_roles_for_person`, `list_work_for_person`) rather than round-tripping HTTP.
    - Source: shipped.
13. **Receipt / provenance pointers** — produce references to the stores where the caller (or a third party with authorization) can verify. Not inlined receipts.
    - Source: existing governance receipt/proof surface (`icn-governance::proof`).
14. **Accessibility metadata** — read the caller's language preference and plain-language mode from identity metadata (or default).
    - `[future design]`: accessibility-preference storage is not yet a first-class primitive; default values are returned until it ships.
15. **Glossary keys** — emit the set of terms present in the response that a downstream shell would need definitions for.
    - Deterministic, based on which fields are populated.
16. **Warnings** — collect from: expired grants, revoked grants still referenced by active role assignments, overlapping representational grants, translation gaps, subsystem staleness indicators, route-drift probes, near-expiry tokens.

### Items That Cannot Be Joined Today Without New Storage / Indexing

- By-grantee / by-delegate / by-executor reverse indexes on `AuthorityGrant`, `Mandate`, and `Delegation`, if not already maintained.
- Member-centered membership index (`list_memberships_for_member`), if not already maintained.
- Session-selected active-scope primitive.
- Display-label resolution (entity + structure + role labels for human rendering).
- Alias-binding read API (bootstrap-apply alias → canonical id lookup from read paths).
- Accessibility preference storage (language, plain-language mode, low-bandwidth hint).
- Route-drift probe (the endpoint can surface what it observes, but a first-class probe is out of scope here).

Each of these is `[future design]` and is enumerated in §Gaps.

---

## Active Scope / Session-Selected Scope

The **active scope** is the hat the member is currently wearing. The problem: a member may belong to multiple entities and act in multiple distinct contexts — individual community member, NYCN organizer, structure delegate, cooperative representative in a federation, node operator, summit organizer inside `summit-cycle-2026`, participant inside `summit-2026`. Every action must be bound to one of those hats, and every receipt must record which hat it was taken under.

### Why Implicit `coop_id` Scope Is Insufficient

Today's gateway JWT carries a `coop_id` claim and a `scopes` array. That approximation fails in several ways:

1. **One `coop_id` cannot express representational action.** A member acting as a representative of GreenStar in NYCN is not "in the GreenStar coop context"; the *subject* of the action is GreenStar, not the member.
2. **One `coop_id` cannot express nested context.** A member operating inside `summit-cycle-2026` (a Program under NYCN) acting as a summit organizer is not fully described by `coop_id = nycn`.
3. **A single JWT claim is not visible in the UI.** Members do not see which hat they are wearing; it is implicit in the token they happened to receive from login.
4. **A single JWT claim is not visible in receipts.** Receipts must say *which scope* the action was taken under, not just who signed.
5. **A single JWT claim is not switchable mid-session in a legible way.** Switching scopes requires re-auth or a token refresh the user does not understand; the gesture is invisible.
6. **A single JWT claim cannot express "self."** Personal actions (own ledger, own trust attestations, own key operations) are the base case, not a degenerate value of `coop_id`.

### Requirements on the Active-Scope Primitive

1. **Explicit.** The selected scope is a typed object, not a string claim fished out of a token.
2. **Visible.** Every member-facing screen renders the active scope prominently. Scope-switching is a first-class gesture.
3. **Included in dangerous confirmations.** Authority-bearing actions show "You are about to cast a vote as GreenStar in NYCN" before the member signs.
4. **Switchable.** Switching requires one gesture and does not require re-login unless the new scope has additional security requirements.
5. **Auditable where it affects authority.** Receipts record the active scope. The substrate keeps provenance.
6. **Never silently changes votes or actions.** A background scope change cannot retroactively reinterpret a pending action.
7. **Representable in low-bandwidth / plain text.** Active scope is one line in a text-only fallback.
8. **Distinguishes canonical ids from aliases / display labels.** The scope key uses canonical ids; the rendering uses human labels.

### Surface in `/me/standing`

`/me/standing` returns both:

- The current `active_scope` object if the session has one selected.
- The list of `available_active_scopes` the caller could switch to.

The endpoint does not decide the scope; it reports and enumerates. Selection lives in a session primitive that is explicitly `[future design]` and must be specified in its own design doc.

---

## Canonical IDs, Aliases, Labels, and URL Safety

The live NYCN bootstrap validation produced a 404 on `/v1/gov/entities/{entity_id}/structures` after successfully creating upstream entities. The proximate cause is most likely route-level handling of colon-containing canonical ids (`entity:icn:federation:nycn`) — the same class of hazard that PR #1591 addressed for proposal index keys. This design absorbs that lesson.

### Definitions

- **Canonical id** — the authoritative typed identifier for an entity or structure: `entity:icn:federation:nycn`, `entity:icn:community:nycn-organizers`, `entity:icn:program:summit-cycle-2026`, `entity:icn:activity:summit-2026`, `structure:icn:committee:nycn-finance`. These are stable, comparable, and carry provenance. They are the only form used for authority checks.
- **Route-safe representation** — whatever encoding makes the canonical id safe to embed in URL path segments (percent-encoding, or an alternate key schema that avoids colons at the route layer entirely). This is a transport concern, not an identity concern.
- **Alias** — a short, human-typeable shortcut bound to a canonical id at bootstrap time or by later admin action. `nycn` → `entity:icn:federation:nycn`. Aliases are UX conveniences. They are **not** authoritative.
- **Display label** — a human-readable name for an entity, structure, role, or scope: "NYCN Federation", "Finance Committee", "Summit 2026". Display labels are rendered; they are never parsed into authority.
- **Translated label** — a display label rendered in the caller's preferred language.
- **Human explanation** — a one-sentence plain-language description of what a primitive *means* to the caller: "You represent GreenStar when casting votes in NYCN federation governance."

### Requirements

1. **Member-facing UI must not expose raw canonical ids as primary explanation.** Canonical ids appear as diagnostic / deep-link values, not as "where you belong" prose.
2. **Action execution must not rely on translated or display labels.** Labels can diverge across languages and renderings; only canonical ids bind authority.
3. **Route paths must use safe encoded ids or route designs that cannot be broken by colon-containing ids.** This is a gateway contract, not a member-facing concern; `/me/standing` surfaces it as a warning when drift is detected.
4. **Alias bindings from bootstrap apply must be visible in diagnostics but not in authority decisions.** `/me/standing` returns `entity_alias` alongside `entity_id`; the Commons Shell may render the alias; every subsequent action submitted by the shell uses the canonical id.
5. **`/me/standing` returns both canonical ids and human labels.** Every entity / structure / scope entry carries its canonical id and its display label. Aliases are included when present.
6. **Warnings surface ambiguous or unresolved aliases.** If an alias fails to resolve to a canonical id, the warning envelope carries the reason; the field falls back to the canonical id alone.

This design does not fix the 404. Routing fixes are the responsibility of the live-validation track. This design requires that `/me/standing` be **robust to** routing fragility: if a downstream route is 404ing, the endpoint returns the best-available standing with a `route_drift` warning, not an empty response or a 500.

---

## Relationship to Action Cards

`/me/standing` is not `/me/action-cards`, but it supplies the standing context an action-card generator needs to produce cards for this member, in this scope, with this authority.

### Action Card Defined

An **action card** is a human-facing rendering of an institutional action. Every card carries:

- **What** — the proposed action, in plain language.
- **Who** — the subject of the action (the member as self, or an entity represented by the member).
- **Authority** — the primitive(s) that authorize this action: membership capability, role assignment scope, grant, mandate.
- **Why** — the charter clause, proposal, mandate, or request that triggered the card.
- **Choices** — the set of valid responses (Yes / No / Abstain / Delegate / Dismiss / etc., per the action type).
- **Consequences** — what happens if the member acts vs. if they do not.
- **Deadline** — the time bound, if any.
- **Supporting records** — links to the proposal, meeting minutes, charter clauses, related receipts.
- **Receipts** — pointers to the provenance of the authority under which the action would execute.
- **Language and glossary** — preferred language, glossary keys for any jargon.
- **Accessibility hints** — screen-reader summary, plain-language mode, low-bandwidth alternative.
- **Risk** — low / medium / high, tied to the action type and the authority basis.
- **Confirmation** — whether this action requires a confirmation step before signing.
- **Active scope** — which of the member's scopes this card belongs to.
- **Canonical target** — canonical ids for every entity, structure, proposal involved (never labels).

### Semantic Action Is the Source of Truth

The card is a **rendering** of the underlying action, not the action itself. The same semantic action (e.g., "vote Yes on proposal X as representative of GreenStar") must be renderable as:

- A rich mobile card (CoopWallet class).
- A plain web page (pilot-ui PWA).
- A screen-reader-safe document.
- A low-bandwidth text view.
- A printed notice (for members without devices, routed via stewards).
- A translated message (institutional glossary included).
- A kiosk / facilitator flow (public terminal, library, community center).
- A CLI summary (`icnctl me action-cards`, future).

`/me/standing` does not produce cards. It returns the standing context — memberships, roles, grants, mandates, available scopes, effective capabilities — that a card generator consumes. The generator (the `/me/action-cards` endpoint, `[future design]`) joins standing with open proposals, assigned work, upcoming meetings, expiring authorities, and care requests to produce the ranked list.

---

## Accessibility Requirements

Accessibility is a legitimacy requirement, not a UI polish concern. `/me/standing` is the contract under every accessible participation surface; the data it returns determines what accessible rendering is possible.

### Requirements

1. **Plain-language summaries.** Every authority concept (membership, role, grant, mandate, delegation, representation) has a one-sentence plain-language rendering in the response (`authority_scope_plain_language`, `scope_plain_language`, `summary_plain_language`).
2. **Glossary-backed concepts.** The response enumerates the glossary keys present. A downstream shell can fetch definitions and attach them to terms.
3. **Multilingual rendering.** The response declares the caller's preferred language and the languages in which translations are available. Missing translations produce a typed warning, not a silent English fallback.
4. **RTL readiness.** Text fields must not assume left-to-right order; display labels and plain-language strings are stored in a form that supports RTL scripts.
5. **Non-English-first participation.** Members whose first language is not the institution's working language receive translated labels and translated plain-language summaries; canonical ids are unaffected.
6. **Screen-reader semantic structure.** The response includes a `screen_reader_summary`: a single-paragraph standing summary specifically composed for linear reading. Not all shells will use it, but it must be available.
7. **Keyboard / switch navigation.** Not a server-side concern directly, but the endpoint must not return data that requires pointer interaction to understand (no "hover for details" semantics baked into the contract).
8. **No color-only meaning.** Urgency buckets, statuses, and warnings are all labeled by typed enum values, never by hex color. Color is a rendering choice.
9. **High contrast, large text, reduced motion.** Rendering-side concerns; the endpoint supports them by returning plain text, structural enums, and no animation-dependent flags.
10. **Low-bandwidth text fallback.** The endpoint honors a low-bandwidth hint from the caller, omitting optional fields (display labels, extended summaries, repeated provenance pointers) and returning a compact form.
11. **Old Android / low-end mobile assumptions.** The payload is bounded in size; the endpoint does not assume client-side computation beyond simple rendering.
12. **No mandatory app-store install for core participation.** The same data is consumable from a browser, a terminal, and a CLI. A member denied access to an app store can still participate.
13. **No mandatory video / audio for core governance.** Every action card derived from standing must have a text-only rendering path.
14. **Async participation.** The response is cacheable and safely re-renderable offline. Pending work items include deadlines for asynchronous planning.
15. **Offline read / draft / later-submit where safe.** The shell can cache `/me/standing`, draft a vote, and submit on reconnect. The sync state machine at the shell level — saved locally / not submitted / submitted / accepted by node / receipt issued / finalized — must be legible to the member.
16. **Clear sync states.** The response includes indicators (`stale_cache`, `reauth_required`, `route_drift`) that a shell translates into sync-state messaging without inventing meaning.
17. **High-risk action confirmations.** Risk is a typed field on cards derived from standing; shells render confirmations proportionate to risk.
18. **Deadlines and reminders that don't punish disability or bad connectivity.** Institutions may define grace policies in their charters; `/me/standing` surfaces the charter-backed deadline and any grace window, not a wall-clock fact.

---

## Language and Glossary Model

Translating ICN is not just translating buttons. The hard part is translating **institutional meaning**. Ordinary governance terms — proposal, quorum, mandate, delegation, representation, charter, structure, program, milestone — carry very different weights across languages, legal systems, and cooperative traditions. A Spanish rendering of "mandate" that reads as "mandato" may be legally stronger than the English original intended; a rendering that reads as "encargo" may be too weak.

### Requirements

1. **UI string translation.** Standard localization of labels, buttons, status words. Table stakes.
2. **Institution-specific glossary packs.** Each institution can ship a glossary pack with its package (e.g., NYCN's glossary defines what "organizer" means inside NYCN). Glossary packs are namespaced; they do not override core ICN terminology.
3. **Plain-language definitions.** Every glossary entry includes a plain-language definition aimed at a reader with no prior governance exposure.
4. **Examples attached to terms.** "Mandate — example: Alice holds a mandate to cast GreenStar's vote on the NYCN 2026 summit budget, valid until June 30, 2026."
5. **Translator notes.** Where a term has translation nuance (e.g., "representation" vs. "delegation"), translator notes explain what to preserve and what to adapt.
6. **Charter-specific terms.** An institution's charter may coin or redefine terms. Those definitions are charter-backed and surface in the glossary alongside core ICN terms, with the charter id as provenance.
7. **Governance primitive terms.** `role`, `proposal`, `vote`, `abstain`, `delegation`, `mandate`, `grant`, `federation`, `bootstrap`, `entity`, `structure`, `program`, `milestone` all require institution-neutral definitions in every supported language.
8. **Fallback if translation missing.** The response includes a typed `missing_translation` warning rather than silently returning English. The shell renders the English form with a visible "translation unavailable" marker.
9. **Warning when formal record and translated summary diverge.** If the plain-language summary could be read as materially different from the canonical (e.g., a mandate's scope is narrower than the summary suggests), the warning surface makes that visible.
10. **Preserving formal canonical record.** The canonical primitive (the `Mandate`, the `AuthorityGrant`, the `Charter` clause) is the record. Translations are renderings of the record. The record does not change because a translation does.

> **The system must translate governance meaning, not just interface labels.** If the glossary layer is skipped, accessibility layered on top collapses into cosmetic styling, and the institution is captured by whoever controls the terminology.

---

## Assisted Participation

Shared devices, public library computers, community kiosks, meeting facilitators, translators, stewards, printed packets, phone and in-person support — these are how most real Commons institutions actually work. Designing as though every member is a solitary power user on a dedicated device reproduces the laptop priesthood.

### Requirements

1. **Assistance must be explicit when it affects an action.** If a helper operates the device on behalf of a member, the action's provenance records the helper's role. Silent help is prohibited for authority-bearing actions.
2. **Member confirmation remains central.** The member — not the helper — is the authority. For authority-bearing actions, confirmation is a member gesture (a key touch, a spoken confirmation recorded via witness protocol, a written sign-off co-signed by a second member, per charter policy).
3. **Helper must not silently gain authority.** Operating the device does not confer capability. A helper with zero membership cannot invoke actions on their own standing; they are channeling the member's standing, with provenance.
4. **Assisted action produces a receipt or audit marker.** The receipt for an assisted action carries a typed `assisted_by` field where applicable, anchored to charter-backed assistance policy. Charters that do not recognize assistance as a distinct category produce receipts without that field.
5. **Privacy preserved.** The helper sees only what the charter permits them to see. A translator is not thereby a reader of the member's ledger; a facilitator is not thereby a viewer of the member's private memberships.
6. **Coercion risk acknowledged.** Assisted participation raises the coercion floor — especially for members in vulnerable positions. Charters may require anti-coercion protections (witnesses, cool-off periods, private revocation paths). `/me/standing` surfaces charter-backed assistance policy as a member-visible field so members know what protections apply.
7. **Institutions may define assistance policy in charters / packages.** The kernel enforces the constraints the charter produces. The kernel does not define what "assistance" means; it enforces what the charter says.
8. **Kernel enforces constraints, not social doctrine.** `/me/standing` reports assistance policy as data; it does not prescribe what assistance must look like.

---

## Privacy and Safety

`/me/standing` is a caller-centered surface. Privacy properties are enforced at the contract level, not left to downstream shells.

1. **Standing must not expose unnecessary private memberships.** The response lists entities the caller is a member of; it does not list other members of those entities. Roster disclosure is the responsibility of entity-scoped endpoints with their own authorization.
2. **Admin queries are future design, requiring explicit capability.** "Show me Alice's standing" is not an authorized operation under this design. It requires a separate endpoint with charter-backed admin capability and privacy review.
3. **Member-facing standing is caller-centered.** The response talks about the caller. It does not talk about third parties beyond what the caller's own role requires (e.g., a representational grant names the grantor entity).
4. **Federation views don't leak unrelated internal roles.** If the caller represents GreenStar in NYCN, the response says so. It does not list the other GreenStar representatives, the other NYCN members, or the internal committee composition of either entity.
5. **Receipts prove authority without oversharing.** Receipt pointers name the primitive ids (grant id, mandate id, proposal id) the caller can use to verify. They do not inline third-party payloads.
6. **Assisted participation must not become surveillance.** Helper attribution is provenance for authority-bearing actions; it is not a general watcher log of everything a member does.
7. **Accessibility metadata must not become disability profiling.** A preferred-language value, a plain-language-mode flag, and a low-bandwidth hint are rendering preferences, not protected-class indicators. They are not emitted to third parties and not used for sorting or filtering.
8. **Language preference must not be exclusion proxy.** An institution cannot use preferred-language values to gate membership or capabilities. If it tries to, the charter would have to explicitly state it, which makes the exclusion visible; kernel enforcement remains language-agnostic.
9. **Operator diagnostics must not leak into member-facing standing unless relevant and safe.** The `warnings` envelope carries typed, member-safe operator-layer signals (`stale_cache`, `route_drift`, `reauth_required`). It does not carry secrets, internal addresses, sled paths, or route mount prefixes.

---

## Readiness Ladder: From Docs to Tests to Live Proof

`/me/standing` is not pilot-ready until it has crossed each of the following rungs. Passing a lower rung does not substitute for a higher one.

1. **Design / documented contract.** This document. The contract specifies the shape, the privacy model, the accessibility model, the warning envelope, and the join plan.
2. **Unit tests.** Core data transformations (capability derivation, scope enumeration, warning generation) are unit-tested against typed fixtures.
3. **Integration tests.** Multi-node, cross-crate tests with `icn-testkit`, including cases for expired grants, revoked mandates, ambiguous scopes, and missing translations.
4. **Live local gateway validation.** A local `icnd` + gateway running against a bootstrap-applied institution (NYCN or an equivalent generic package). The endpoint returns standing, the Commons Shell renders it, the member can select an active scope, and the selected scope appears in subsequent receipts.
5. **Repeatable operator runbook.** An operator can, from a clean machine, bring up the stack, apply a bootstrap package, authenticate as a member, call `/me/standing`, and see standing that matches the package definition. The runbook is tested, not theoretical.
6. **Pilot institution validation.** A real institution (NYCN is the first candidate) runs a session where its members authenticate, check standing, act under an explicit active scope, and the resulting receipts match the charter. Pilot validation is the final rung.

**`/me/standing` is not pilot-ready merely because integration tests pass.** The NYCN bootstrap track has already demonstrated that integration-passing code can still 404 live. Each rung is independently necessary. This design connects to the active `feat/live-nycn-bootstrap-validation` track without taking it over: that track owns rungs 4 through 6 for the bootstrap surface; `/me/standing` inherits rungs 1 through 3 first and then joins the live-validation track at rung 4.

---

## Acceptance Criteria

This design is ready to implement when:

- [x] Real repo primitives are mapped to response fields (identity, entity, membership, structure, role assignment, authority grant, mandate, delegation, governance domain, proposal pointer, action-item pointer).
- [x] Unavailable pieces are marked `[future design]` with the required addition named (by-X reverse indexes, session-scope primitive, display-label resolution, alias-read API, accessibility-preference storage).
- [x] JSON shape is concrete enough for an implementer to produce an OpenAPI schema and a handler skeleton without inventing fields.
- [x] Explicit relationship to `/gov/me/scopes` and `/gov/me/work` is specified (they remain authoritative; `/me/standing` composes).
- [x] NYCN bootstrap / live-validation reality is acknowledged and its lessons are absorbed (route drift warnings, canonical-id vs. alias discipline, live-proof rung in the readiness ladder).
- [x] The active-scope problem is clearly specified with requirements and surface, without authorizing its implementation.
- [x] Canonical ids vs. aliases vs. display labels vs. translated labels are distinguished at the contract level and mirrored in the response shape.
- [x] Accessibility is first-class in the contract, not a downstream UI concern.
- [x] Language and glossary model is first-class.
- [x] Assisted participation is included with coercion-awareness and charter-deferral.
- [x] Meaning Firewall and kernel/app separation are preserved (no domain semantics in the kernel; reporting, not defining).
- [x] Commons Shell relationship is clear (standing is the data contract; cards and shells are downstream).
- [x] No implementation is authorized beyond this design unless explicitly scoped in a subsequent doc or phase.

---

## Implementation Slices

Suggested order for future phases. Each slice is a separately reviewable PR.

1. **Design-only doc** (this PR).
2. **Minimal `/me/standing`** returning identity + memberships + current `/gov/me/scopes` output, nothing more. Empty `mandates`, `grants`, `delegations`, `active_scope`, `accessibility`. A warning envelope exists but is empty.
3. **Join role assignments / authority grants / mandates** with the necessary reverse indexes. Warnings surface expired and revoked items.
4. **Active-scope selector primitive** — server-side session store for the caller's currently selected scope, with switch, audit trail, and inclusion in subsequent action receipts.
5. **Standing-derived pending work references** — the `pending_work` field is populated by direct `manager.list_work_for_person` + urgency bucketing.
6. **Action Card design doc** — a separate document specifying card generation, ranking, dismissal, expiry, cross-scope deduplication.
7. **`GET /me/action-cards`** — implementation of the card generator, consuming `/me/standing`.
8. **Language and glossary packs** — per-institution glossary packs, translated plain-language summaries, translator notes, `missing_translation` warnings.
9. **Low-bandwidth / text renderer** — text-only fallback shape; a CLI summary (`icnctl me standing`, future); a screen-reader-first HTML rendering in pilot-ui.
10. **Assisted participation receipt model** — typed `assisted_by` provenance, charter-backed assistance policy surfaced in `/me/standing`, anti-coercion protections per charter.
11. **Validate against NYCN bootstrap-created institution state** — standing queries resolve correctly against entities, structures, aliases, and grants produced by `icnctl institution apply`.
12. **Live-gateway validation and runbook** — the final rung before any pilot-facing release. The runbook is tested, not theoretical.

Each slice has its own readiness ladder: design, unit, integration, live-local, runbook, pilot. No slice skips rungs.

---

## Gaps Identified `[future design]`

Enumerated here so later docs and issues can pick them up without re-deriving.

1. **Member-centered membership index.** `list_memberships_for_member(member_id)` or equivalent query.
2. **Reverse indexes on authority primitives.** `list_grants_for_grantee(did)`, `list_mandates_for_executor(did)`, `list_delegations_for_delegate(did)`, `list_delegations_for_delegator(did)` — ideally maintained incrementally, not scanned.
3. **Session-selected active-scope primitive.** Typed object, server-side selection, switchable, auditable, included in subsequent receipts. Successor to JWT `coop_id` approximation.
4. **Display-label resolution.** A stable way to resolve entity / structure / role canonical ids to human display labels, with fallback when labels are missing.
5. **Alias read API.** A first-class lookup from alias to canonical id and back, exposed to the standing endpoint (not only to bootstrap apply).
6. **Accessibility preference storage.** Language preference, plain-language mode, low-bandwidth hint — stored per-member, surfaced in `/me/standing`, respected by Commons Shell.
7. **Glossary packs and translation infrastructure.** Per-institution glossary packs, translator notes, `missing_translation` warnings, charter-backed term definitions.
8. **Plain-language rendering of authority scopes.** Converting `authority_scope` strings and `TypedScope` values into plain-language sentences.
9. **Assisted-participation provenance.** Typed `assisted_by` field on receipts where charter policy recognizes assistance; anti-coercion protections per charter.
10. **Route-drift probe.** A first-class health probe for the routes `/me/standing` depends on, so warnings are grounded in active observation, not passive inference.
11. **Representative / entity-vote primitive.** A `RepresentativeVote` / `EntityAction` wrapper that binds `(represented_entity, human_signer, authority_source, action_payload, scope, provenance, receipt)` — referenced by `THE_COMMONS.md §14` and required for clean representational semantics in federations.
12. **Federation tally semantics.** When entity members vote in a federation, how are weights resolved, how do multiple representatives of the same entity interact, what happens on conflicting signatures. Not strictly a `/me/standing` concern, but necessary before representational action cards can be produced correctly.
13. **Mandate expiry UX.** Member-facing view of a held mandate approaching expiry, a revoked grant, a superseded representational role.
14. **Operator-scope primitive for member-facing standing.** Node operators are members of their communities. How does their operator scope appear in `/me/standing` alongside their member scope without leaking operator diagnostics into the member view.
15. **Commons OS / kiosk-mode derivations.** How `/me/standing` serves a public-terminal short-session flow with safe defaults (no residue, explicit sign-out, assisted participation where applicable).

---

## References

**Canonical anchors (consulted while drafting):**

- `docs/architecture/THE_COMMONS.md` — sibling doctrine, direct parent of this design.
- `docs/ARCHITECTURE.md` — ICN architecture reference.
- `docs/architecture/KERNEL_APP_SEPARATION.md` — Meaning Firewall, PolicyOracle pattern.
- `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` — authoritative routing between ICN platform and institution packages.
- `docs/architecture/CELLS_AND_SCOPES.md` — scope primitives at the substrate layer.
- `docs/architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md` — identity and membership model.
- `docs/design/ICN_VISUAL_SYSTEM.md` — visual doctrine, scope-aware membership, proof-backed coordination.
- `docs/mobile/icn-mobile-ux-spec-v1.md` — current mobile UX spec.
- `docs/archive/2026/plans-scratch/human-interface.md` — archived raw material for the Commons Shell concept.
- `docs/reference/institution-package-bootstrap.md` — institution-package bootstrap reference.
- `institutions/nycn/docs/bootstrap-runbook.md` — NYCN bootstrap runbook (live-validation track; this design references but does not modify it).
- `docs/adr/ADR-0014-constitutional-object-model.md` — typed authority model (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`).
- `docs/STATE.md` — current project state.
- `AGENTS.md` — agent coding instructions and app topology rule.

**Code anchors (types and endpoints referenced by name):**

- `icn/crates/icn-identity/src/lib.rs` — `Did`.
- `icn/crates/icn-entity/src/entity.rs` — `EntityId`, `Individual`, `Cooperative(CooperativeProfile)`, `Community(CommunityProfile)`, `Federation(FederationProfile)`.
- `icn/crates/icn-entity/src/membership.rs` — `Membership`, `MembershipRole`, `MembershipStatus`, `MembershipCapability`.
- `icn/crates/icn-governance/src/structure.rs` — `Structure`, `RoleAssignment`.
- `icn/crates/icn-governance/src/activity.rs` — `Activity`.
- `icn/crates/icn-governance/src/program.rs` — `Program`, `Milestone`.
- `icn/crates/icn-governance/src/parent.rs` — `InstitutionalParent`.
- `icn/crates/icn-governance/src/domain.rs` — `GovernanceDomain`.
- `icn/crates/icn-governance/src/proposal.rs` — `Proposal`, `ProposalPayload`.
- `icn/crates/icn-governance/src/vote.rs` — `Vote`.
- `icn/crates/icn-governance/src/delegation.rs` — `Delegation`.
- `icn/crates/icn-governance/src/authority.rs` — `AuthorityGrant`.
- `icn/crates/icn-governance/src/mandate.rs` — `Mandate`.
- `icn/crates/icn-governance/src/meeting.rs` — `Meeting`.
- `icn/crates/icn-governance/src/action_item.rs` — `ActionItem`, `ActionItemSpec`.
- `icn/crates/icn-governance/src/charter.rs` — `Charter`.
- `icn/crates/icn-governance/src/proof.rs` — governance receipts / proof.
- `icn/crates/icn-kernel-api/src/invariants.rs` — frozen-core invariants.
- `icn/crates/icn-kernel-api/src/bootstrap.rs` — `Capability`, `CapabilitySet`.
- `icn/apps/governance/src/http/handlers.rs` — `GET /gov/me/scopes` (`get_my_scopes`), `GET /gov/me/work` (`get_my_work`).
- `icn/bins/icnctl/src/institution_bootstrap.rs` — institution bootstrap apply; the `format!("/v1/gov/entities/{parent}/structures")` call site is the current live-validation 404 target and the proximate motivation for the canonical-id-vs-route-safe discipline in this doc.

---

*This doc is a design contract, not a plan. No endpoint is authorized for implementation on the basis of this document alone. Each implementation slice requires its own design doc, issue, and review. `[future design]` items require their own design docs before code lands.*
