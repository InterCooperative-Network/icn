---
Status: descriptive
Authority: dev checklist
Canonical: no
Last verified: 2026-06-12
---

# OpenAPI member-surface gaps — annotation checklist

**What this is:** a checklist comparing the utoipa `paths(...)` list in
`icn/crates/icn-gateway/src/openapi.rs` against the member-facing routes
actually registered by the governance app
(`icn/apps/governance/src/http/configure.rs`, handlers in
`icn/apps/governance/src/http/handlers.rs`, mounted by the gateway under
`/v1/gov`). **Checklist only — no annotations are added here.**

**Why it matters:** the gateway serves Swagger UI at `/swagger-ui/` backed
by `/api-docs/openapi.json` (`icn/crates/icn-gateway/src/server.rs`,
`SwaggerUi::new(...).url("/api-docs/openapi.json", ...)`). Once a handler
carries a `#[utoipa::path(...)]` annotation and is listed in
`openapi.rs` `paths(...)`, it lands in that served document automatically.
The static `web/api-docs/openapi.yaml` is stale (last touched 2026-03-03;
zero member endpoints) — handler source is truth.

**Current `paths(...)` contents (verified 2026-06-12):** exactly one entry,
`crate::api::federation::propose_clearing_adoption`. Every member-facing
route below is therefore missing from the served OpenAPI document.

**Mechanism note:** these handlers live in the `icn-governance-actor` app
crate, not in `icn-gateway`. Annotating them means adding
`#[utoipa::path]` to the app-crate handlers (the response models in
`icn/apps/governance/src/http/models.rs` already derive `ToSchema`) and
referencing them from the gateway's `paths(...)` / `components(schemas(...))`
lists, or merging a second `OpenApi` doc from the app crate. Generic
handlers (`<E: GovernanceEventEmitter>`) may need concrete-fn shims for
utoipa registration — that design choice belongs to the implementation PR,
not this checklist.

## Missing member-facing endpoints (12)

### Standing and caller-scoped views

- [ ] `GET /v1/gov/me/standing` — handler `get_my_standing` — scope
      `governance:read` — response `StandingResponse`
      (`models.rs`). Annotation needed: document the `?domain_id` filter and
      the empty-standing-is-200 contract.
- [ ] `GET /v1/gov/me/action-cards` — handler `get_my_action_cards` — scope
      `governance:read` — response `ActionCardsResponse` **wrapper**
      `{did, cards, generated_at}`, not a bare array. Annotation needed:
      schema for `ActionCard` + the four closed enums.
- [ ] `GET /v1/gov/me/scopes` — handler `get_my_scopes` — scope
      `governance:read` — response `Vec<RoleAssignmentResponse>`.
      Annotation needed: flat-list shape across all structures.
- [ ] `GET /v1/gov/me/work` — handler `get_my_work` — scope
      `governance:read` — response `Vec<ActionItemResponse>`. Annotation
      needed: document the filter query params (`build_my_work_filter`).

### Action items + completion receipts

- [ ] `GET /v1/gov/domains/{domain_id}/action-items` — handler
      `list_action_items` — scope `governance:read` — response
      `Vec<ActionItemResponse>`. Annotation needed: filter params.
- [ ] `GET /v1/gov/domains/{domain_id}/action-items/{item_id}` — handler
      `get_action_item` — scope `governance:read` — response
      `ActionItemResponse`. Annotation needed: 404 contract.
- [ ] `PUT /v1/gov/domains/{domain_id}/action-items/{item_id}/status` —
      handler `update_action_item_status` — scopes
      `governance:meeting:write` | `governance:write` (narrowest-first via
      `require_any_scope_matched`; creator/assignee + domain membership also
      enforced) — request `StatusUpdateRequest` (`{"status": "pending" |
      "in_progress" | "completed" | "deferred" | "cancelled"}`) — response
      `ActionItemResponse`. Annotation needed: note that first transition to
      `completed` emits an `ActionItemCompletionReceipt`.
- [ ] `GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt`
      — handler `get_action_item_completion_receipt` — scope
      `governance:read` — response `icn_governance::ActionItemCompletionReceipt`
      (`icn/crates/icn-governance/src/proof.rs`; would need a `ToSchema`
      derive or a gateway-side mirror type). Annotation needed: 404-when-
      absent/cross-domain contract.

### Meetings / attendance receipts

- [ ] `PUT /v1/gov/meetings/{meeting_id}/attendance` — handler
      `mark_attendance` — scopes `governance:meeting:write` |
      `governance:write` (+ domain membership) — response `MeetingResponse`.
      Annotation needed: note that a transition into `Present`/`Remote`
      emits a `MeetingAttendanceReceipt` (ADR-0026 Layer 2).
      **Adjacent observation (not an annotation gap):** there is no GET
      retrieval endpoint for attendance receipts today — the receipt is
      emitted and stored (`receipt_backend.rs`) but a member shell has no
      route to fetch it. That is an endpoint gap, tracked by the
      `meeting`/`attend` follow-up noted in ADR-0020 §7.

### Proposals / votes / decision receipts

- [ ] `POST /v1/gov/proposals/{proposal_id}/vote` — handler `cast_vote` —
      scopes `governance:proposal:write` | `governance:write` — request
      `CastVoteRequest` — response: the updated proposal object (re-fetched
      after the vote). Annotation needed: choice strings
      `for`/`against`/`abstain`.
- [ ] `GET /v1/gov/proposals/{proposal_id}/tally` — handler
      `get_vote_tally` — scope `governance:read` — response: handler-local
      `VoteTallyResponse` struct (`for_votes`, `against_votes`,
      `abstain_votes`, `total_votes`). Annotation needed: the struct is
      private to the handler body and would need promotion to `models.rs`
      for `ToSchema`.
- [ ] `GET /v1/gov/proposals/{proposal_id}/proof` — handler `get_proof` —
      scope `governance:read` — response: `GovernanceProof` (decision
      receipt + attestations; served only after signature/hash
      verification). Annotation needed: schema exposure for the proof
      envelope.
- [ ] `GET /v1/gov/proposals/{proposal_id}/chain` — handler `get_chain` —
      scope `governance:read` — response: `ProvenanceChain`
      (`apps/governance/src/manager.rs::get_chain`; governance decision
      receipt + linked allocation receipts + effect records). Annotation
      needed: schema exposure for the chain object.

## Count

12 member-facing endpoints missing from `paths(...)`, plus 1 adjacent
observation (no attendance-receipt retrieval endpoint exists to annotate).

## Out of scope for this checklist

- Adding the annotations themselves.
- Non-member-facing governance routes (domain admin, charters, programs,
  delegations, discussion, federation/SDIS proposals) — also unannotated,
  but not part of the member-shell surface this checklist tracks.
- Regenerating `web/api-docs/openapi.yaml` or the TypeScript SDK types
  (that follows the annotation PR per the drift chain in `CLAUDE.md`).
