//! HTTP request/response models for governance endpoints.
//!
//! These were previously in `icn-gateway/src/models.rs`. Moving them here
//! ensures governance domain logic (including its wire format) lives entirely
//! in the app layer.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use icn_governance::{
    BootstrapEntityType, DeliberationEntryKind, ProcessGateKind, ProcessGateResult,
};

// ============================================================================
// Domain
// ============================================================================

/// Create a new governance domain
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateDomainRequest {
    pub id: String,
    pub name: String,
    pub profile: String,
    pub quorum_percent: u8,
    pub approval_percent: u8,
    pub voting_period_days: u64,
    pub members: Vec<String>,

    /// Decision mode: `"majority"` (default) or `"consent"`.
    ///
    /// In consent mode, proposals pass unless objections exceed `max_objections`.
    #[serde(default)]
    pub decision_mode: Option<String>,

    /// Maximum number of objections (against-votes) allowed in consent mode.
    /// Default `0` means any objection blocks the proposal (strict consensus).
    /// Only meaningful when `decision_mode` is `"consent"`.
    #[serde(default)]
    pub max_objections: Option<u8>,
}

/// Add a member to a governance domain
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddDomainMemberRequest {
    pub did: String,
    #[serde(default = "default_weight")]
    pub weight: f64,
}

fn default_weight() -> f64 {
    1.0
}

// ============================================================================
// Process gate results (#2144 — first ProcessTransitionReceipt class)
// ============================================================================

/// Request body for `POST /gov/domains/{domain_id}/process-sessions/{session_id}/gate-results`.
///
/// `gate_kind` and `result` are the closed `icn_governance` taxonomies
/// ([`ProcessGateKind`], [`ProcessGateResult`]); an out-of-taxonomy value is
/// rejected by JSON deserialization (400) rather than silently coerced. The
/// `session_id` and `domain_id` come from the path, and the recording actor
/// comes from the authenticated token — not the body.
///
/// **Fails closed on unknown fields** (`deny_unknown_fields`): a client that
/// posts extra fields alongside `gate_kind`/`result` is rejected with 400
/// rather than having them silently discarded, so this surface's contract is
/// enforced by rejection, not omission (mirrors the `RecordDecisionRequest`
/// hardening in PR #2282).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordProcessGateResultRequest {
    /// Closed-taxonomy gate kind being recorded (e.g. `"privacy_review"`).
    pub gate_kind: ProcessGateKind,
    /// Pass/fail result of the gate evaluation (`"pass"` | `"fail"`).
    pub result: ProcessGateResult,
}

// ============================================================================
// Deliberation entries (#2277/#2278 — third ProcessTransitionReceipt class)
// ============================================================================

/// Request body for
/// `POST /gov/domains/{domain_id}/process-sessions/{session_id}/deliberation-entries/{entry_id}/record`.
///
/// `entry_kind` is the closed `icn_governance` taxonomy
/// ([`DeliberationEntryKind`], the #2278 v1 list); an out-of-taxonomy value
/// is rejected by JSON deserialization (400) rather than silently coerced.
/// `body_hash` is the 64-hex-character blake3 fingerprint of the entry body
/// — **the body itself is never sent to or stored by this surface**. The
/// `domain_id`, `session_id`, and `entry_id` come from the path, and the
/// author comes from the authenticated token — not the body.
///
/// **Fails closed on unknown fields** (`deny_unknown_fields`): a client that
/// posts extra fields (e.g. a raw body, or decision semantics) alongside
/// `entry_kind`/`body_hash` is rejected with 400 rather than having them
/// silently discarded, so this surface's contract is enforced by rejection,
/// not omission (mirrors the `RecordDecisionRequest` hardening in PR #2282).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordDeliberationEntryRequest {
    /// Closed-taxonomy entry kind being recorded (e.g. `"question"`).
    pub entry_kind: DeliberationEntryKind,
    /// 64-hex-character content fingerprint of the entry body.
    pub body_hash: String,
}

// ============================================================================
// Decision recording (#2280/#2281 — fourth ProcessTransitionReceipt class)
// ============================================================================

/// Request body for
/// `POST /gov/domains/{domain_id}/process-sessions/{session_id}/decisions/{decision_id}/record`.
///
/// `body_hash` is the 64-hex-character blake3 fingerprint of the decision
/// body — **the body itself is never sent to or stored by this surface**.
/// Per the #2281 Q4 decision this request deliberately carries **no other
/// field**: no decision kind, no outcome, no tally, no deciding-body
/// handle — the receipt records a generic recorded-decision fact, parallel
/// to (never converging with) the proposal/vote decision lineage. The
/// `domain_id`, `session_id`, and `decision_id` come from the path, and
/// the recorder comes from the authenticated token — not the body.
///
/// **Fails closed on unknown fields** (`deny_unknown_fields`): a client
/// that posts `body_hash` alongside `outcome`, `proposal_id`, `decider`,
/// or a raw body field is rejected with 400 rather than having the extra
/// fields silently discarded. The #2281 boundary — no decision
/// semantics/body cross this surface — is thereby enforced by rejection,
/// not by omission.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordDecisionRequest {
    /// 64-hex-character content fingerprint of the decision body.
    pub body_hash: String,
}

// ============================================================================
// Institutional domain policy adoption (#2142 — gated DomainPolicy adoption)
// ============================================================================

/// Request body for `POST /gov/domains/{domain_id}/domain-policy/adopt`.
///
/// Carries only the policy *content* to adopt. The server content-addresses it
/// (blake3) into a `DomainPolicyId` and binds it to the path `domain_id`, so the
/// adopted `DomainPolicyRef` is correct by construction: a client cannot assert
/// a mismatched id or author the policy for a foreign domain. The adopting
/// actor is the authenticated caller (token `sub`) — never a body field — and
/// the authority to adopt is resolved server-side through the
/// `DefaultMandateGate`, not asserted here.
///
/// The MVP `DomainPolicy` stores no CCL text (#1817); `policy_content` is the
/// opaque bytes the content-addressed id commits to. Only the adopted
/// `DomainPolicyRef` is persisted in the `InstitutionalDomain`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptDomainPolicyRequest {
    /// Opaque policy content the adopted `DomainPolicyId` is the blake3 hash of.
    pub policy_content: String,
}

/// Response body for a successful domain-policy adoption.
///
/// The legible projection of the adopted [`icn_governance::DomainPolicyRef`]:
/// the content-addressed policy id (hex) and the domain it was adopted for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptDomainPolicyResponse {
    /// Hex-encoded content-addressed id of the adopted policy version.
    pub policy_id: String,
    /// The governance domain the policy was adopted for.
    pub domain_id: String,
}

// ============================================================================
// Institutional domain declaration (#2142 — gated declare/create)
// ============================================================================

/// Request body for `POST /gov/domains/{domain_id}/institutional-domain/declare`.
///
/// Declares the `InstitutionalDomain` authority record for the existing
/// `GovernanceDomainId` carried in the path. The declaring actor is the
/// authenticated caller (token `sub`), never a body field, and the authority to
/// declare is resolved server-side through the `DefaultMandateGate` — not
/// asserted here. `entity_type` is the closed [`BootstrapEntityType`] taxonomy
/// (`"federation"` | `"cooperative"` | `"community"` | `"individual"`); an
/// out-of-taxonomy value is rejected by JSON deserialization (400). `charter_id`
/// is the optional hex (64-char) id of the domain's founding charter; omit it
/// to declare a charter-less domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclareInstitutionalDomainRequest {
    /// The governed entity taxonomy the domain is owned by.
    pub entity_type: BootstrapEntityType,
    /// Optional hex (64-char) id of the domain's founding `CharterId`.
    #[serde(default)]
    pub charter_id: Option<String>,
}

/// Response body for a successful `InstitutionalDomain` declaration.
///
/// A legible projection of the freshly-declared
/// [`icn_governance::InstitutionalDomain`]. A freshly-declared domain is
/// unbound, so `current_policy_id` is always `null` here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclareInstitutionalDomainResponse {
    /// The governance domain the institutional-domain record is keyed by.
    pub domain_id: String,
    /// The owning entity taxonomy.
    pub owning_entity_class: BootstrapEntityType,
    /// Hex id of the adopted founding charter, if any.
    pub charter_id: Option<String>,
    /// Hex id of the current adopted policy — `null` for a fresh declaration.
    pub current_policy_id: Option<String>,
}

// ============================================================================
// Charter activation
// ============================================================================

/// Activate a CCL charter so the running runtime starts enforcing it.
///
/// Hands a validated CCL document off to the gateway's charter ratification hook
/// (which deploys it into `CharterPolicyOracle`). This is the bootstrap-side
/// counterpart to the governance-ratification path: it does not create a
/// proposal, it does not record a decision — it registers an already-decided
/// charter so the kernel can enforce it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActivateCharterRequest {
    /// Stable identifier for the charter. Convention: equals the governance
    /// domain id this charter governs (e.g. cooperative DID or human handle).
    pub charter_id: String,
    /// Full CCL document, YAML-serialized. Parsed and validated at the
    /// boundary; malformed input returns 400.
    pub charter_yaml: String,
}

/// How a charter became active — the authority lineage of the activation.
///
/// Bootstrap activation is **not** ratified/mandate activation. The substrate
/// has to be bootstrappable (the first charter comes from outside the
/// governance loop, which is administrative by definition), but a charter that
/// bypasses governance must never be mistaken by clients or surfaces for one
/// ratified through a proposal. This discriminator marks the direct path
/// explicitly so it cannot be rendered as ordinary ratified authority.
///
/// Single variant for now; the ratified-path counterpart is intentionally not
/// modeled here (it travels a different response type). A future administrative
/// receipt may subsume this signal — see the bootstrap-activation receipt-class
/// question in the abuse-case hardening doctrine (§4.4, §15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActivationPath {
    /// Direct, bootstrap / direct-administrative activation: registered an
    /// already-decided charter without a proposal or recorded decision. The
    /// artifact-level marker for this path is the synthetic
    /// `direct-activation:<charter_id>` provenance string carried on the
    /// emitted effect, which this discriminator surfaces at the API boundary.
    Bootstrap,
}

/// Response returned after a successful charter activation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActivateCharterResponse {
    pub charter_id: String,
    /// `"active"` once deployed into the policy oracle.
    pub status: String,
    /// Unix epoch seconds when activation was recorded.
    pub activated_at: u64,
    /// Authority lineage of this activation. Direct activation is always
    /// [`ActivationPath::Bootstrap`]: it is a bootstrap / direct-administrative
    /// path, not a ratified/mandate activation.
    pub activation_path: ActivationPath,
}

// ============================================================================
// Member standing (read model for `GET /me/standing`)
// ============================================================================

/// Optional query parameters for `GET /me/standing`.
///
/// All filters are inclusive: applying a filter narrows the response, never
/// expands it. The caller is always derived from the authenticated DID — there
/// is no `did` query parameter, so a caller cannot ask for someone else's
/// standing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct StandingQuery {
    /// If set, restrict `domains` and `roles` to assignments under this
    /// governance domain. Unknown ids are NOT a 404 — they simply produce an
    /// empty standing for that filter, which is a valid answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_id: Option<String>,
}

/// Caller-facing membership and authority read model.
///
/// Returned by `GET /v1/gov/me/standing`. Composes existing governance state
/// (domain memberships + structure role assignments) into one digestible
/// response so member-facing UI does not need to query four endpoints and
/// stitch them together.
///
/// ## What this is NOT
///
/// - This is **not** an authorization token. Scopes here are descriptive,
///   not bearer-issued. Authorization decisions still flow through the
///   `PolicyOracle`.
/// - This is **not** an action-card feed. Action-card generation builds on
///   top of this read model in a separate endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StandingResponse {
    /// The authenticated caller's DID.
    pub did: String,
    /// Optional human-readable label. Reserved for a future identity-registry
    /// lookup; always `None` in this version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_label: Option<String>,
    /// Governance domains in which this caller has membership standing.
    /// May be empty.
    pub domains: Vec<StandingDomainMembership>,
    /// Role assignments held by this caller across all structures.
    /// May be empty.
    pub roles: Vec<StandingRoleAssignment>,
    /// Union of `authority_scope` strings across `roles`, deduplicated and
    /// sorted. Convenience field for UI; equivalent to flattening `roles`.
    pub authority_scopes: Vec<String>,
    /// Unix epoch seconds when this standing was computed. The response is a
    /// snapshot; nothing is cached server-side.
    pub generated_at: u64,
}

/// One governance-domain membership row in [`StandingResponse`].
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StandingDomainMembership {
    pub domain_id: String,
    pub domain_name: String,
    /// `"static_list"` or `"trust_threshold"`.
    pub membership_source: String,
    /// `"member"` if the caller is in the static member list,
    /// `"unverified"` if membership comes from a trust-threshold source that
    /// this read model does not evaluate (the caller may still be a member;
    /// the trust graph is the source of truth).
    pub status: String,
}

/// One role-assignment row in [`StandingResponse`]. Joins the underlying
/// [`icn_governance::RoleAssignment`] with cheap structure metadata
/// (name + parent entity) so UI does not have to fetch the structure.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StandingRoleAssignment {
    pub role_assignment_id: String,
    pub structure_id: String,
    /// `None` if the structure was deleted or is otherwise unreadable; the
    /// role row is still surfaced so the caller can see something is off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structure_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_entity_id: Option<String>,
    pub role: String,
    pub authority_scope: Vec<String>,
    pub start_date: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<u64>,
}

// ============================================================================
// Action cards (read model for `GET /me/action-cards`)
// ============================================================================

/// Where a card is derived from. Closed taxonomy. Variants `SignalRule` and
/// `ObligationLifecycle` are reserved by issue #1646 for future implementation;
/// the runtime does not emit them today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionCardSourceKind {
    Proposal,
    Meeting,
    ActionItem,
    SignalRule,
    ObligationLifecycle,
}

/// What the holder is being asked to do. Closed taxonomy. New variants land
/// when their source path implementation lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionCardActionKind {
    /// Cast a vote on a proposal.
    Vote,
    /// Attend a scheduled meeting.
    Attend,
    /// Complete an assigned action item.
    Complete,
}

/// What the card targets. Maps to ICN's `entity / structure / individual`
/// constitutional axes; institution packages bind their local taxonomy to
/// these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionCardScope {
    Entity,
    Structure,
    Individual,
}

/// Coarse risk indicator surfaced to the holder. UI may sort or annotate but
/// must not hide cards. Exact semantics are policy concerns; this enum carries
/// a conservative three-level vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionCardRiskLevel {
    Low,
    Normal,
    Elevated,
}

/// One pending action card for the authenticated caller.
///
/// Returned by `GET /v1/gov/me/action-cards`. Cards are **derived views** —
/// computed at request time from the caller's standing plus open governance
/// state — never stored entities. Two requests at the same moment from the
/// same DID return identical cards. See ADR-0027.
///
/// ## Boundary
///
/// All fields are generic. No institution-specific vocabulary. Institution
/// packages translate their local templates into these generic kinds; ICN
/// does not learn package-specific nouns from this surface.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionCard {
    /// Stable, deterministic id of the card. Runtime format:
    /// `card-<source_kind_snake>-<source_id>-<action_kind_snake>` (for example
    /// `card-proposal-<id>-vote`, `card-meeting-<id>-attend`,
    /// `card-action_item-<id>-complete`) so the same underlying object yields
    /// the same card id across requests.
    pub id: String,
    pub source_kind: ActionCardSourceKind,
    pub action_kind: ActionCardActionKind,
    pub scope: ActionCardScope,
    /// Short, plain-language title. Suitable for a list row.
    pub title: String,
    /// One-line plain-language summary of what this card is asking the
    /// holder to do.
    pub summary: String,
    /// Why the caller has the right to act here, in plain language. Examples:
    /// `"role_assignment_in_domain"`, `"assigned_action_item"`,
    /// `"meeting_attendee"`.
    pub authority_basis: String,
    /// Authority-scope strings the kernel would expect for this action.
    /// Generic; institution packages may use richer strings.
    pub required_authority_scope: Vec<String>,
    /// Optional Unix-seconds deadline. `None` means no time pressure encoded
    /// by this card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<u64>,
    pub risk_level: ActionCardRiskLevel,
    /// Accessibility note for the rendering shell. Generic across institutions;
    /// today the runtime emits the same baseline note for all cards.
    pub accessibility_hint: String,
    /// Whether successful completion of this action is expected to produce a
    /// receipt (governance receipt, attendance receipt, action-item completion
    /// receipt).
    pub receipt_expected: bool,
    /// Underlying object id (proposal id, meeting id, action item id).
    pub source_id: String,
    /// Governance domain this card lives under, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_id: Option<String>,
}

/// Wrapper response for `GET /me/action-cards`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionCardsResponse {
    /// The authenticated caller's DID.
    pub did: String,
    /// Pending cards for the caller, derived at request time. May be empty.
    pub cards: Vec<ActionCard>,
    /// Unix-seconds when this card set was computed. Snapshot only — nothing
    /// is cached server-side.
    pub generated_at: u64,
}

// ============================================================================
// Proposals
// ============================================================================

/// Scope for a proposal (local or federation-wide)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProposalScopeRequest {
    #[default]
    Local,
    Federation {
        federation_id: String,
    },
}

/// Create a new proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub payload: ProposalPayloadRequest,
    #[serde(default)]
    pub scope: Option<ProposalScopeRequest>,
    /// Action items to create when this proposal is accepted.
    /// Each spec becomes a linked ActionItem with provenance to this proposal.
    #[serde(default)]
    pub action_items_on_accept: Vec<ActionItemSpecRequest>,
}

/// Template for creating action items from accepted proposals (API model).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionItemSpecRequest {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Assignee DID (optional)
    #[serde(default)]
    pub assignee: Option<String>,
    /// Seconds after acceptance before this item is due
    #[serde(default)]
    pub due_offset_seconds: Option<u64>,
    /// Priority: low, medium (default), high, critical
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Proposal payload types
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProposalPayloadRequest {
    Text {
        body: String,
    },
    Budget {
        amount: i64,
        recipient: String,
        currency: String,
        purpose: String,
    },
    Membership {
        action: String,
        did: String,
    },
    ConfigChange {
        key: String,
        value: String,
    },
    /// Charter ratification — members vote to adopt a CCL charter document.
    Charter {
        /// Stable charter identifier (cooperative DID or human-readable name).
        charter_id: String,
        /// Complete YAML charter document (CCL schema_version: v0).
        charter_yaml: String,
    },
}

/// Request body for `POST /proposals/sdis/appoint-steward`.
///
/// Proposer must be an active steward (checked via `GovernanceContext::steward_checker`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AppointStewardProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    /// DID of the steward candidate.
    pub candidate: String,
    /// Geographic or operational region the steward will serve.
    pub region: String,
    /// Bond amount (in commons credits) the steward must post.
    pub bond_amount: i64,
    /// Proposed term length in seconds.
    pub term_length_seconds: u64,
    /// DIDs of stewards sponsoring this candidate. May be empty.
    #[serde(default)]
    pub sponsors: Vec<String>,
}

/// Request body for `POST /proposals/sdis/remove-steward`.
///
/// Proposer must be an active steward (checked via `GovernanceContext::steward_checker`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RemoveStewardProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    /// DID of the steward to remove.
    pub steward: String,
    /// Reason for removal.
    pub reason: String,
    /// Whether the steward's bond should be returned on removal.
    #[serde(default)]
    pub return_bond: bool,
}

/// Open a proposal for voting
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OpenProposalRequest {
    pub voting_period_seconds: Option<u64>,
}

/// Cast a vote on a proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CastVoteRequest {
    pub choice: String,
    pub comment: Option<String>,
}

/// Vote choice response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VoteChoiceResponse {
    For,
    Against,
    Abstain,
}

/// Gateway response DTO for governance proposals
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProposalResponse {
    pub id: String,
    pub domain_id: String,
    pub proposer: String,
    pub title: String,
    pub description: String,
    pub state: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_ref: Option<String>,
}

// ============================================================================
// Vote Delegation
// ============================================================================

/// Create a new vote delegation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateDelegationRequest {
    pub delegate: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

/// Delegation response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DelegationResponse {
    pub id: String,
    pub delegator: String,
    pub delegate: String,
    pub scope: String,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<u64>,
    pub is_active: bool,
}

/// List of delegations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DelegationListResponse {
    pub given: Vec<DelegationResponse>,
    pub received: Vec<DelegationResponse>,
}

// ============================================================================
// Federation Proposals
// ============================================================================

/// Common fields for federation proposal requests
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FederationProposalCommon {
    pub domain_id: String,
    pub title: String,
    pub description: String,
}

/// Federation terms for join proposals
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FederationTermsRequest {
    pub min_trust_threshold: f64,
    pub governance_binding: bool,
    pub data_sharing_level: String,
    pub dispute_resolution: String,
}

/// Request to create a "join federation" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JoinFederationProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub federation_id: String,
    pub terms: FederationTermsRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sponsor_coop_id: Option<String>,
}

/// Request to create a "leave federation" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LeaveFederationProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub federation_id: String,
    pub reason: String,
    pub grace_period_days: u32,
}

/// Request to create an "establish clearing" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EstablishClearingProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub partner_coop_id: String,
    pub partner_coop_did: String,
    pub max_imbalance: i64,
    pub settlement_interval: String,
    pub currency: String,
}

/// Request to create a "terminate clearing" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TerminateClearingProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub partner_coop_id: String,
    pub reason: String,
}

/// Request to create a "vouch for cooperative" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VouchProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub target_coop_id: String,
    pub target_coop_did: String,
    pub trust_score: f64,
    pub context: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// Request to create a "revoke vouch" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RevokeVouchProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub target_coop_id: String,
    pub reason: String,
}

/// Request to create an "update federation policy" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateFederationPolicyProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_accept_vouch_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_decay_factor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attestations_per_minute: Option<u32>,
}

// ============================================================================
// Action Items
// ============================================================================

/// Request to create a new action item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateActionItemRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<u64>,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_proposal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_context: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_priority() -> String {
    "medium".to_string()
}

/// Request to update an action item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateActionItemRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Request to add a note to an action item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddActionItemNoteRequest {
    pub content: String,
}

/// Action item response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionItemResponse {
    pub id: String,
    pub domain_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<u64>,
    pub status: String,
    pub priority: String,
    pub created_by: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_proposal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_context: Option<String>,
    pub tags: Vec<String>,
    pub notes: Vec<ActionItemNoteResponse>,
    pub is_overdue: bool,
}

/// Action item note response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionItemNoteResponse {
    pub id: String,
    pub author: String,
    pub content: String,
    pub created_at: u64,
}

/// Query parameters for listing action items
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionItemFilterParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overdue: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

/// Query parameters for `GET /gov/me/work`.
///
/// A subset of [`ActionItemFilterParams`] — the `assignee` field is omitted
/// because `me/work` implicitly filters by the authenticated caller's DID.
/// All fields default to `None` (no filtering applied).
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct MyWorkFilterParams {
    // Per-field `serde(default)` is required for serde_urlencoded (used by
    // actix-web `web::Query`) to fill absent query params with `None`. Struct-level
    // `#[serde(default)]` is not sufficient for key-value formats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overdue: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

// ============================================================================
// Discussion
// ============================================================================

/// Request to add a comment to a proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddCommentRequest {
    pub content: String,
    pub parent_id: Option<String>,
}

/// Request to edit an existing comment
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EditCommentRequest {
    pub content: String,
}

/// Request to add an emoji reaction to a comment
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddReactionRequest {
    pub emoji: String,
}

/// Request to remove an emoji reaction from a comment (DELETE with body)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RemoveReactionRequest {
    pub emoji: String,
}

/// Query parameters for listing comments
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListCommentsQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Single comment response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CommentResponse {
    pub id: String,
    pub proposal_id: String,
    pub author: String,
    pub content: String,
    pub parent_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub reactions: std::collections::HashMap<String, usize>,
    pub is_edited: bool,
    pub is_deleted: bool,
}

/// List of comments with pagination metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListCommentsResponse {
    pub comments: Vec<CommentResponse>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Full discussion for a proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscussionResponse {
    pub proposal_id: String,
    pub comments: Vec<CommentResponse>,
    pub participant_count: usize,
    pub last_activity_at: u64,
}

// ============================================================================
// Delegation helpers
// ============================================================================

/// Query parameters for listing delegations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListDelegationsQuery {
    #[serde(default)]
    pub include_revoked: bool,
}

// ============================================================================
// Action item helpers
// ============================================================================

/// Request to update only the status of an action item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StatusUpdateRequest {
    pub status: String,
}

/// Request to remove a domain member (DELETE with body)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RemoveDomainMemberRequest {
    pub did: String,
}

// ============================================================================
// Institutional Structure (committees, working groups, teams)
// ============================================================================

/// Request to create an internal structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateStructureRequest {
    /// Kind: "committee", "working_group", "team", "office"
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Structure response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StructureResponse {
    pub id: String,
    pub entity_id: String,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    pub created_at: u64,
}

/// Request to assign a role in a structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AssignRoleRequest {
    /// DID of the person receiving the role
    pub did: String,
    /// Role name: "coordinator", "member", "facilitator", "note_taker", etc.
    pub role: String,
    /// Delegated authority scopes attached to this assignment.
    ///
    /// Opaque to ICN — institutions define their own scope vocabulary
    /// (e.g. `"approve_budget_within_policy"`, `"curate_session_intake"`).
    /// Omitting the field defaults to an empty scope, preserving backward
    /// compatibility with callers that predate #1629.
    #[serde(default)]
    pub authority_scope: Vec<String>,
}

/// Role assignment response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RoleAssignmentResponse {
    pub id: String,
    pub structure_id: String,
    pub person_did: String,
    pub role: String,
    /// Delegated authority scopes for this assignment. May be empty.
    /// Serialized even when empty so clients can rely on the field's
    /// presence in the response shape.
    #[serde(default)]
    pub authority_scope: Vec<String>,
    pub start_date: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<u64>,
}

// ============================================================================
// Activity (events, programs, projects, initiatives)
// ============================================================================

/// Request to create an activity
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateActivityRequest {
    /// Kind: "event", "program", "project", "initiative"
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<u64>,
    /// Structures linked to this activity by structure ID.
    #[serde(default)]
    pub linked_structures: Vec<String>,
    /// Optional parent program ID (e.g., "annual-summit-cycle")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_program_id: Option<String>,
}

/// Activity response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActivityResponse {
    pub id: String,
    pub entity_id: String,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<u64>,
    pub linked_structures: Vec<String>,
    /// Parent program ID if this activity executes within a program cycle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_program_id: Option<String>,
    pub created_at: u64,
}

// ============================================================================
// Meeting (deliberation trace objects)
// ============================================================================

/// Request to create a meeting
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateMeetingRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Scheduled start time as Unix timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<u64>,
}

/// Request to add an attendee to a meeting
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddAttendeeRequest {
    pub did: String,
    /// Coordination role: "facilitator", "note_taker", "participant", "observer"
    #[serde(default = "default_meeting_role")]
    pub meeting_role: String,
}

fn default_meeting_role() -> String {
    "participant".to_string()
}

/// Request to mark attendance for a participant
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MarkAttendanceRequest {
    pub did: String,
    /// Status: "present", "absent", "remote"
    pub status: String,
}

/// Request to add an agenda item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddAgendaItemRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presenter: Option<String>,
    /// Proposal ID to discuss during this agenda item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_proposal: Option<String>,
}

/// Request to update an agenda item outcome
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateAgendaItemRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discussion_notes: Option<String>,
    /// Outcome: "resolved", "tabled", "referred", "no_action"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// Attendee in a meeting response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MeetingAttendeeResponse {
    pub did: String,
    pub status: String,
    pub meeting_role: String,
}

/// Agenda item in a meeting response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AgendaItemResponse {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presenter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_proposal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discussion_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    pub generated_action_items: Vec<String>,
}

/// Meeting response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MeetingResponse {
    pub id: String,
    pub domain_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<u64>,
    pub attendees: Vec<MeetingAttendeeResponse>,
    pub agenda: Vec<AgendaItemResponse>,
    pub linked_structures: Vec<String>,
    pub linked_activities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_doc_id: Option<String>,
    pub created_by: String,
    pub created_at: u64,
    pub present_count: usize,
}

// ============================================================================
// Program (multi-phase institutional endeavors)
// ============================================================================

/// Request to create a program
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateProgramRequest {
    /// Entity ID of the owning entity (cooperative, community, federation)
    pub parent_entity_id: String,
    /// Kind: "cycle", "campaign", "initiative", "series", or a custom string
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional start time as Unix timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<u64>,
    /// Optional end time as Unix timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_at: Option<u64>,
    /// Proposal ID that authorized this program (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_decision: Option<String>,
}

/// Program response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProgramResponse {
    pub id: String,
    pub domain_id: String,
    pub parent_entity_id: String,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_at: Option<u64>,
    pub milestones: Vec<String>,
    pub activities: Vec<String>,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_decision: Option<String>,
}

// ============================================================================
// Milestone (stage-gates within programs)
// ============================================================================

/// Request to create a milestone
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateMilestoneRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Ordinal position among the program's milestones (0-based)
    #[serde(default)]
    pub phase_index: u32,
    /// Optional target date as Unix timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<u64>,
    /// Free-form checklist of completion criteria
    #[serde(default)]
    pub completion_criteria: Vec<String>,
}

/// Request to update a milestone's status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateMilestoneStatusRequest {
    /// Status: "pending", "in_progress", "completed", "blocked", "skipped"
    pub status: String,
}

/// Request to update a program's lifecycle status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateProgramStatusRequest {
    /// Status: "draft", "active_planning", "public_launch", "in_execution", "completed", "archived"
    pub status: String,
}

/// Milestone response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilestoneResponse {
    pub id: String,
    pub program_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub phase_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<u64>,
    pub status: String,
    pub completion_criteria: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_by: Option<String>,
    pub created_at: u64,
}

// ============================================================================
// Milestone preview (read-only readiness view)
// ============================================================================

/// Summary row describing a milestone that blocks another milestone from
/// advancing.
///
/// Emitted inside [`MilestonePreviewResponse::blocking_milestones`]: any
/// earlier-phase milestone that has not reached a terminal status
/// (`Completed` or `Skipped`). Order matches `phase_index`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BlockingMilestoneSummary {
    pub id: String,
    pub name: String,
    pub phase_index: u32,
    pub status: String,
}

/// Composite read-only preview describing whether a milestone is currently
/// ready to be advanced, and what observable state drove that answer.
///
/// Returned by `GET /gov/milestones/{milestone_id}/preview`. Purely read-only:
/// no mutation, no status transition, no event emission. The preview reflects
/// the same ordering semantics a human operator would use when deciding
/// whether to mark a milestone complete — earlier-phase milestones must be
/// `Completed` or `Skipped`, and the target milestone itself must be open and
/// not `Blocked`.
///
/// `completion_criteria` is surfaced verbatim for caller inspection but is
/// **not evaluated**. The governance core deliberately treats criteria as
/// free-form declarative text; interpretation of what "satisfied" means is an
/// institution-package concern. This endpoint therefore reports what the
/// milestone *declares* it needs, not whether any individual criterion has
/// been met.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MilestonePreviewResponse {
    /// Milestone identifier.
    pub milestone_id: String,
    /// Program the milestone belongs to.
    pub program_id: String,
    /// Milestone display name.
    pub name: String,
    /// Ordinal position among the program's milestones (0-based).
    pub phase_index: u32,
    /// Current milestone status (`pending`, `in_progress`, `completed`,
    /// `blocked`, `skipped`).
    pub status: String,
    /// `true` while the milestone is not in a terminal (`completed` /
    /// `skipped`) state.
    pub is_open: bool,
    /// Enclosing program's current lifecycle status (`draft`,
    /// `active_planning`, `public_launch`, `in_execution`, `closed`,
    /// `archived`). Informational — the preview does not require any specific
    /// program status.
    pub program_status: String,
    /// `true` when every earlier-phase milestone (strictly lower
    /// `phase_index`) is `completed` or `skipped`.
    pub earlier_milestones_complete: bool,
    /// Earlier-phase milestones that are not yet `completed` or `skipped`,
    /// ordered by `phase_index`. Empty when `earlier_milestones_complete` is
    /// `true`.
    pub blocking_milestones: Vec<BlockingMilestoneSummary>,
    /// Free-form checklist declared on the milestone at creation. Surfaced
    /// verbatim for caller inspection; **not evaluated** by this endpoint.
    pub completion_criteria: Vec<String>,
    /// Count of declared completion criteria (equivalent to
    /// `completion_criteria.len()`). Provided for callers that only need the
    /// count without transferring the strings.
    pub criteria_count: usize,
    /// Observable readiness for advancement.
    ///
    /// `true` iff all of:
    /// - milestone is open (not `completed` or `skipped`),
    /// - milestone status is not `blocked`,
    /// - every earlier-phase milestone is `completed` or `skipped`.
    ///
    /// Does **not** evaluate `completion_criteria` — that is out of scope for
    /// ICN governance core.
    pub ready_to_advance: bool,
    /// Human-readable reason when `ready_to_advance` is `false`. `None` when
    /// the milestone is ready.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ============================================================================
// Program dashboard (composite read surface)
// ============================================================================

/// Compact milestone row inside the program dashboard.
///
/// Uses the full `MilestoneResponse` field set minus `completion_criteria` and
/// `created_at`, which are detail-level and inflate the composite response
/// without helping a dashboard consumer understand program progress.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardMilestoneSummary {
    pub id: String,
    pub name: String,
    pub phase_index: u32,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
}

/// Milestone status counts for the program dashboard.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct DashboardMilestoneCounts {
    pub total: usize,
    pub completed: usize,
    pub in_progress: usize,
    pub blocked: usize,
    pub pending: usize,
    pub skipped: usize,
}

/// Compact activity row inside the program dashboard.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardActivitySummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub status: String,
}

/// Action item status counts for the program dashboard.
///
/// Counts only items whose `parent` is an `Activity` listed in the program's
/// `activities` vec. Domain items with no activity parent or a parent that
/// does not belong to this program are excluded.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct DashboardActionItemCounts {
    pub pending: usize,
    pub in_progress: usize,
    pub completed: usize,
    pub deferred: usize,
    pub cancelled: usize,
    pub total: usize,
}

/// Compact meeting row inside the program dashboard.
///
/// Returned as part of [`ProgramDashboardResponse`]. Only includes the fields
/// a dashboard consumer needs to render meeting status; full meeting details
/// (agenda, attendees, notes) are available via `GET /gov/meetings/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardMeetingSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<u64>,
    /// Activity IDs from this program that the meeting is linked to.
    pub linked_activity_ids: Vec<String>,
}

/// Composite program dashboard response.
///
/// Returned by `GET /gov/programs/{program_id}/dashboard`. Combines the program
/// record, its ordered milestones (with status counts), its linked activities,
/// action item counts scoped to those activities, and meetings linked through
/// those activities.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProgramDashboardResponse {
    pub program_id: String,
    pub domain_id: String,
    pub parent_entity_id: String,
    pub name: String,
    pub kind: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_at: Option<u64>,
    pub milestones: Vec<DashboardMilestoneSummary>,
    pub milestone_counts: DashboardMilestoneCounts,
    pub activities: Vec<DashboardActivitySummary>,
    pub action_item_counts: DashboardActionItemCounts,
    /// Meetings linked to at least one activity that belongs to this program.
    /// Deduped (a meeting linked to multiple activities appears once), sorted
    /// earliest `scheduled_at` first; unscheduled meetings sort last.
    pub meetings: Vec<DashboardMeetingSummary>,
}

// ============================================================================
// Milestone history (read-only lifecycle bookmark view)
// ============================================================================

/// A single lifecycle bookmark extracted from the milestone record.
///
/// **Coverage limitation**: the governance store records only two temporal
/// facts about a milestone — when it was created (`created_at`) and when it
/// reached `Completed` (`completed_at` + `completed_by`). Intermediate
/// transitions (e.g. `pending → in_progress → blocked`) are not currently
/// persisted; they will not appear in this list. `source` names which field
/// on the record the entry was derived from, so callers know exactly what
/// persisted data backs each entry.
///
/// Fields that are genuinely unknown are `null` (`changed_by` on the
/// creation entry; `to_status` would always be the entry's status).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MilestoneHistoryEntry {
    /// Unix seconds when this lifecycle event occurred (always present).
    pub changed_at: u64,
    /// DID of the actor who caused the transition, if recorded.
    /// `null` for the creation entry (the creator is not currently stored on
    /// the milestone record itself).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_by: Option<String>,
    /// The status the milestone moved INTO at this event.
    pub to_status: String,
    /// Persisted-data source backing this entry.
    /// `"creation"` → derived from `created_at` (initial status is always
    /// `pending`). `"completion_record"` → derived from `completed_at` +
    /// `completed_by`.
    pub source: String,
}

/// Ordered collection of lifecycle bookmarks for a milestone.
///
/// Returned by `GET /gov/milestones/{milestone_id}/history`. Entries are
/// ordered oldest-to-newest (creation first).
///
/// `coverage` describes the fidelity of the history: `"lifecycle_bookmarks"`
/// means only creation and completion events are available because the
/// governance store does not persist intermediate status transitions.
/// Callers MUST treat this list as a partial view, not a complete audit log.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MilestoneHistoryResponse {
    /// Milestone identifier.
    pub milestone_id: String,
    /// Fidelity annotation.
    ///
    /// Always `"lifecycle_bookmarks"` in the current implementation: only
    /// `created_at` and `completed_at` are persisted on the milestone record.
    /// Future store migrations (e.g. an append-only transition log) would
    /// change this to `"full_transition_log"`.
    pub coverage: String,
    /// Lifecycle bookmarks, oldest first.
    pub entries: Vec<MilestoneHistoryEntry>,
}

// ============================================================================
// Program summary (read-only progression view)
// ============================================================================

/// Compact reference to the next milestone that has not yet reached a terminal
/// state (`completed` or `skipped`), ordered by `phase_index`.
///
/// `null` in [`ProgramSummaryResponse`] when all milestones are terminal or
/// when the program has no milestones.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NextUnfinishedMilestone {
    pub milestone_id: String,
    pub name: String,
    pub phase_index: u32,
    pub status: String,
}

/// Read-only progression summary for a program.
///
/// Returned by `GET /gov/programs/{program_id}/summary`. Complements the
/// richer `/dashboard` surface (which includes activities and action items)
/// by focusing solely on the program's milestone progression state.
///
/// The `progress_basis` field names the mechanism used to derive
/// `current_phase_index` and `next_unfinished_milestone`. Currently always
/// `"phase_index_ordering"` — milestones are ordered by their `phase_index`
/// field (0-based), which is the only programmatic ordering signal in the
/// current model.
///
/// Milestone semantics are deliberately not interpreted: this endpoint reports
/// observable state and ordering, not what any milestone "means" institutionally.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProgramSummaryResponse {
    pub program_id: String,
    pub name: String,
    /// Current program lifecycle status.
    pub program_status: String,
    /// Milestone status counts.
    pub milestone_counts: DashboardMilestoneCounts,
    /// All milestones for this program, ordered by `phase_index` ascending.
    pub milestones: Vec<DashboardMilestoneSummary>,
    /// The lowest-`phase_index` milestone that is not yet `completed` or
    /// `skipped`. `null` when all milestones are terminal or there are none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_unfinished_milestone: Option<NextUnfinishedMilestone>,
    /// `phase_index` of [`next_unfinished_milestone`] when one exists, else
    /// the highest `phase_index` among terminal milestones, else `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_phase_index: Option<u32>,
    /// Basis used to derive `current_phase_index` and
    /// `next_unfinished_milestone`. Currently always
    /// `"phase_index_ordering"`.
    pub progress_basis: String,
}

// ============================================================================
// Proposal deliberation trail (reverse read-model)
// ============================================================================

/// Decision receipt summary for a deliberation trail.
///
/// Flattens a `GovernanceDecisionReceipt` into the minimal set of fields
/// required to answer "what was decided, and when". Callers that need full
/// cryptographic proof should hit `GET /gov/proposals/{id}/proof`; callers
/// that need the economic-effect chain should hit `.../chain`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeliberationDecisionResponse {
    /// Lowercase outcome label: `"accepted"`, `"rejected"`, `"no_quorum"`.
    pub outcome: String,
    /// Unix seconds when the decision was recorded.
    pub decided_at: u64,
    /// Hex-encoded decision hash (stable identifier of the receipt).
    pub decision_hash: String,
}

/// A single meeting in a proposal's deliberation trail.
///
/// Each entry captures one agenda item that referenced the proposal. The same
/// meeting may appear multiple times if its agenda includes the proposal in
/// more than one item.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeliberationMeetingResponse {
    pub meeting_id: String,
    pub meeting_title: String,
    /// Lowercase meeting status: `"scheduled"`, `"in_progress"`, `"completed"`, `"cancelled"`.
    pub meeting_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<u64>,
    pub agenda_item_id: String,
    pub agenda_item_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presenter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discussion_notes: Option<String>,
    /// Free-form agenda outcome string recorded by the facilitator,
    /// e.g. `"resolved"`, `"tabled"`, `"referred"`, `"no_action"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// Action items this agenda item generated on meeting close.
    pub generated_action_items: Vec<String>,
}

/// A persisted institutional effect record, shaped for the HTTP wire.
///
/// Mirrors [`crate::institutional_effect::InstitutionalEffectRecord`] with
/// the binary `decision_hash` rendered as a lowercase hex string. See the
/// record module docs for semantics.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InstitutionalEffectResponse {
    pub record_id: String,
    pub proposal_id: String,
    pub domain_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_hash: Option<String>,
    /// `"freeze_member"`, `"unfreeze_member"`, `"deploy_charter"`,
    /// `"appoint_steward"`, or `"revoke_steward"`.
    pub effect_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_did: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub recorded_at: u64,
    /// Full translation detail as JSON. Callers that need specifics beyond
    /// the typed fields read from here.
    pub payload: serde_json::Value,
}

/// Downstream dispatch evidence entry as rendered on the wire.
///
/// Carries the subsystem name, optional receipt_ref, success flag, and
/// timestamp. `receipt_ref` is opaque — resolving it to a downstream
/// record is the subsystem's responsibility, not governance's.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DispatchEvidenceResponse {
    pub evidence_id: String,
    pub effect_record_id: String,
    pub proposal_id: String,
    pub subsystem: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_ref: Option<String>,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub recorded_at: u64,
}

/// An emitted effect record paired with its dispatch evidence and a
/// derived reconciliation status.
///
/// `reconciliation_status` is one of:
/// - `"emitted_only"` — no downstream evidence yet (either not dispatched,
///   fire-and-forget dispatch, or subsystem never reported back);
/// - `"execution_evidenced"` — at least one successful evidence entry and
///   no recorded failures;
/// - `"execution_failed"` — at least one evidence entry reported failure.
///   `reconciliation_error` surfaces the most recent failure message.
///
/// A later success does NOT erase an earlier failure — audit discipline.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReconciledEffectResponse {
    #[serde(flatten)]
    pub record: InstitutionalEffectResponse,
    pub reconciliation_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconciliation_error: Option<String>,
    pub dispatch_evidence: Vec<DispatchEvidenceResponse>,
}

/// Response body for `GET /gov/proposals/{proposal_id}/effects`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProposalEffectsResponse {
    pub proposal_id: String,
    pub effects: Vec<ReconciledEffectResponse>,
}

/// Reverse read-model: a proposal's deliberation trail.
///
/// Returned by `GET /gov/proposals/{proposal_id}/deliberation`. Links a
/// proposal backwards to every meeting where it appeared on the agenda,
/// together with the decision receipt (when closed) and the translated
/// institutional-effect shape.
///
/// `effect_kind` labels the `GovernanceEffect` variant the proposal would
/// translate into on acceptance (e.g. `"freeze_member"`, `"deploy_charter"`,
/// `"appoint_steward"`, `"unhandled"`). It is a shape claim only — the
/// presence of a label does NOT imply the effect was dispatched. Consult
/// `governance_decision` and, for economic effects, `GET .../chain` for
/// dispatch evidence.
///
/// This endpoint is NOT a progress or activity tracker: it answers
/// "what deliberation and decision produced this institutional action?"
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProposalDeliberationResponse {
    pub proposal_id: String,
    pub domain_id: String,
    /// Proposal payload type name (`"text"`, `"budget"`, `"freeze_member"`, …).
    pub payload_type: String,
    /// Lowercase proposal lifecycle state
    /// (`"draft"`, `"deliberation"`, `"open"`, `"accepted"`, `"rejected"`,
    /// `"no_quorum"`, `"cancelled"`, `"vetoed"`, `"force_closed"`).
    pub state: String,
    /// Translated [`crate::http::configure::GovernanceEffect`] shape for this
    /// proposal payload. `"unhandled"` indicates no structured institutional
    /// effect is wired for this payload type — the acceptance would still be
    /// recorded but no gateway dispatch would occur beyond action items.
    pub effect_kind: String,
    /// Meetings, in chronological order, where this proposal was on the agenda.
    pub deliberations: Vec<DeliberationMeetingResponse>,
    /// Decision receipt summary if the proposal has been closed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub governance_decision: Option<DeliberationDecisionResponse>,
    /// Durable institutional effect records emitted at acceptance, paired
    /// with dispatch evidence and derived reconciliation status. Oldest
    /// first. Empty when the proposal was not accepted or when the payload
    /// translated to `Unhandled`.
    #[serde(default)]
    pub emitted_effects: Vec<ReconciledEffectResponse>,
}
