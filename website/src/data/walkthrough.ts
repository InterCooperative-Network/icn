// walkthrough.ts — the fictional scenario behind /see-it-work (#2610).
//
// ─── What this is ────────────────────────────────────────────────────────────
//
// A deterministic, entirely fictional institutional scenario used to show a
// public visitor the shape of ICN without a terminal, an account, or a live
// institution. Every value here is a literal. Nothing is fetched, nothing is
// generated at build time, and the page renders identically on every build.
//
// ─── Where the shapes come from ──────────────────────────────────────────────
//
// The field names mirror the repository's real wire contracts so a reader who
// later opens the code finds the same vocabulary rather than a parallel one
// invented for marketing:
//
//   standing          icn/apps/governance/src/http/models.rs — StandingResponse,
//                     StandingDomainMembership, StandingRoleAssignment
//   action card       icn/apps/governance/src/http/models.rs — ActionCard
//                     (`authority_basis` is a plain string on the member-facing
//                     model; the kernel-level AuthorityBasis enum is anti-entropy
//                     machinery and is deliberately NOT used here)
//   mutation preview  docs/contracts/pending-publish-summary.schema.json
//   receipt           icn-governance/src/proof.rs — ActionItemCompletionReceipt
//   receipt chain     the ADR-0026 process receipt classes, each of which
//                     carries the previous receipt's record_hash under a named
//                     field. That chaining is what makes the last step of the
//                     walkthrough drawable from the data itself.
//
// ─── Safety rules this file must keep ────────────────────────────────────────
//
//  1. No real partner names. Brightworks Collective / Northeast Worker
//     Federation / Maple Street Mutual Aid are the fictional entities already
//     used on this site and in web/member-shell fixtures. Real partner names
//     live only in institutions/<name>/ packages and never here.
//  2. No personal names, emails, or phone numbers — not even fake ones. People
//     appear by role ("a shop steward"), which is also the accurate way to talk
//     about standing: it attaches to a role in a scope, not to a person's name.
//  3. No payment / wallet / balance / currency vocabulary. See
//     docs/design/CONTENT_STYLE_GUIDE.md § Regulatory-safe vocabulary.
//  4. Receipts record that something happened. They never authorize anything —
//     the Rust doc comments are explicit that a receipt "grants zero authority",
//     and the copy here must not blur that.
//  5. Hashes are displayed hex-encoded for readability. On the wire a
//     record_hash is a 32-byte array, and the page says so.

/** Truth label carried by the whole surface, per the visual explainer bible. */
export const WALKTHROUGH_TRUTH_LABEL = "illustrative direction";

/** The fictional institutions in the scenario. */
export const SCENARIO_SCOPES = {
  cooperative: {
    id: "demo.coop.brightworks",
    name: "Brightworks Collective",
    kind: "cooperative" as const,
    blurb:
      "A fictional forty-person worker cooperative that does metal finishing.",
  },
  federation: {
    id: "demo.federation.northeast-worker",
    name: "Northeast Worker Federation",
    kind: "federation" as const,
    blurb:
      "A fictional federation Brightworks belongs to, which does not own or govern it.",
  },
};

/** The governance domain the scenario takes place in. */
export const SCENARIO_DOMAIN = {
  domain_id: "demo.coop.brightworks/workplace-safety",
  domain_name: "Workplace Safety (demo)",
};

export interface WalkthroughStep {
  /** Stable anchor id, used for the step index and deep links. */
  id: string;
  /** "01".."06" */
  n: string;
  /** Plain-language step name. The primary reading path. */
  title: string;
  /** The ICN concept id this step is really about, for <Term> rendering. */
  conceptId: string;
  /** One sentence in plain language. No ICN vocabulary allowed here. */
  plain: string;
  /** Two or three sentences of narrative. Still plain language. */
  narrative: string[];
  /**
   * The technical layer, shown inside a <details>. This is where field names,
   * hashes and contract vocabulary are allowed to appear.
   */
  technical: {
    /** What the underlying record is called. */
    recordName: string;
    /** Where that shape is defined in the repository. */
    definedIn: string;
    /** Field/value pairs as they would appear on the wire. */
    fields: Array<{ key: string; value: string; note?: string }>;
    /** What a reader should take away — often a boundary, not a capability. */
    note?: string;
  };
}

export const WALKTHROUGH_STEPS: WalkthroughStep[] = [
  {
    id: "standing",
    n: "01",
    title: "Someone is recognized by their cooperative",
    conceptId: "standing",
    plain:
      "Before anyone can act, the cooperative has to be able to say who counts as a member here — and be able to show why.",
    narrative: [
      "A shop steward at Brightworks Collective is recognized in the workplace-safety area of the cooperative. That recognition is not a setting an administrator toggled. It exists because the members adopted a rule that says who holds this role, and because a decision under that rule assigned it.",
      "The cooperative can check this for itself at any time. It does not have to ask a software vendor whether the person is really a member.",
    ],
    technical: {
      recordName: "StandingResponse",
      definedIn: "icn/apps/governance/src/http/models.rs",
      fields: [
        {
          key: "did",
          value: "did:icn:example-brightworks-steward-not-live",
          note: "A fictional identifier. Real identifiers are self-held keypairs, not accounts issued by a platform.",
        },
        {
          key: "domains[0].domain_id",
          value: "demo.coop.brightworks/workplace-safety",
        },
        {
          key: "domains[0].membership_source",
          value: "static_list",
          note: "How the membership was established. The other value the model allows is trust_threshold.",
        },
        { key: "domains[0].status", value: "member" },
        { key: "roles[0].role", value: "shop_steward" },
        {
          key: "roles[0].authority_scope",
          value: '["safety.action_item.complete", "safety.proposal.submit"]',
          note: "What this role may do. Authority is a list of specific things, never a general permission level.",
        },
        {
          key: "generated_at",
          value: "1755043200",
          note: "Standing is a snapshot taken when asked, not a cached status.",
        },
      ],
      note: "Standing attaches to a role in a scope, not to a person. The same human can hold different standing in a different cooperative, and neither cooperative learns about the other.",
    },
  },
  {
    id: "decision",
    n: "02",
    title: "Something comes up that needs a decision",
    conceptId: "governance",
    plain:
      "A piece of work appears that somebody has to decide on — with a deadline, and with a note about how consequential it is.",
    narrative: [
      "The extraction fan in the finishing room is failing. Replacing it is not a small purchase and it affects everyone who works that room, so it is not something one person quietly handles.",
      "It arrives as a specific item of work with a stated deadline, not as a message in a thread that somebody may or may not read.",
    ],
    technical: {
      recordName: "ActionCard",
      definedIn: "icn/apps/governance/src/http/models.rs",
      fields: [
        { key: "id", value: "demo-card-0417" },
        { key: "source_kind", value: "proposal" },
        { key: "action_kind", value: "review_and_decide" },
        {
          key: "scope",
          value: "entity",
          note: "One of entity | structure | individual — which level of the institution is acting.",
        },
        {
          key: "title",
          value: "Replace the failing extraction fan in the finishing room",
        },
        {
          key: "risk_level",
          value: "elevated",
          note: "One of low | normal | elevated. Surfaced to the member before they act, not after.",
        },
        { key: "deadline", value: "1755302400" },
        { key: "receipt_expected", value: "true" },
      ],
    },
  },
  {
    id: "authority",
    n: "03",
    title: "The rules say who may decide it, and why",
    conceptId: "authority",
    plain:
      "Instead of a permission an administrator granted, there is a rule the members adopted — and the system can point at it.",
    narrative: [
      "The steward may act on this because the cooperative adopted a rule placing workplace-safety decisions with the safety stewards, and because a decision under that rule assigned them the role. Both of those are records, and both can be read back.",
      'This is the part most software gets structurally wrong. Elsewhere, "who is allowed to do this" is an access-control setting held by whoever administers the tool. Here it is a consequence of decisions the members actually made.',
    ],
    technical: {
      recordName: "ActionCard.authority_basis",
      definedIn: "icn/apps/governance/src/http/models.rs",
      fields: [
        {
          key: "authority_basis",
          value: "role_assignment_in_domain",
          note: "The member-facing basis is a named string. Other values in use include assigned_action_item, meeting_attendee, domain_membership, governing_body_agenda.",
        },
        {
          key: "required_authority_scope",
          value: '["safety.action_item.complete"]',
          note: "Compared against the standing record from step 01. If it does not match, the action is not offered.",
        },
        { key: "domain_id", value: "demo.coop.brightworks/workplace-safety" },
      ],
      note: "Authority narrows as it travels. A role in one scope does not carry into another, and a federation cannot grant a role inside a cooperative that belongs to it.",
    },
  },
  {
    id: "preview",
    n: "04",
    title: "Before anything happens, you can see what would happen",
    conceptId: "execution",
    plain:
      "The system shows what record would be created, whether it can be undone, and what proof it would leave — before anyone confirms.",
    narrative: [
      "Reading this screen changes nothing. That is the point: an institution should be able to look at a consequential action in full detail without the act of looking being the act of doing.",
      "The three things shown before any confirmation are always the same three: what authorizes this, whether it can be reversed, and what record it will leave behind.",
    ],
    technical: {
      recordName: "pending-publish-summary → mutation_preview",
      definedIn: "docs/contracts/pending-publish-summary.schema.json",
      fields: [
        { key: "mutation_preview.would_create", value: "action_item_record" },
        {
          key: "mutation_preview.summary",
          value:
            "Would record completion of the extraction fan replacement in Workplace Safety (demo). No record is created by rendering this row.",
          note: "That closing sentence is part of the contract, not decoration — the preview must state its own inertness.",
        },
        {
          key: "receipt_expected.category",
          value: "action_item_completion_receipt",
        },
        {
          key: "review_actions",
          value: '["approve", "reject", "edit", "request_info", "defer"]',
          note: "Deferring and asking for more information are first-class outcomes, not failures to decide.",
        },
      ],
      note: "This walkthrough is read-only. There is no confirm step here, and no record is created anywhere by visiting this page.",
    },
  },
  {
    id: "receipt",
    n: "05",
    title: "What happened leaves a record that can be checked",
    conceptId: "provenance",
    plain:
      "Once the work is done, there is a durable record of what happened, who recorded it, and when — one that can be verified later without trusting whoever is telling you about it.",
    narrative: [
      "The fan gets replaced and the completion is recorded. The record is not a line in an activity feed that an administrator can quietly edit; it is content-addressed, so any later change to it produces a different hash and stops matching.",
      "A receipt says an act occurred. It does not make the act legitimate — the authority for that came from step 03, before anything happened.",
    ],
    technical: {
      recordName: "ActionItemCompletionReceipt",
      definedIn: "icn/crates/icn-governance/src/proof.rs",
      fields: [
        { key: "item_id", value: "demo-item-0417" },
        { key: "domain_id", value: "demo.coop.brightworks/workplace-safety" },
        {
          key: "actor_did",
          value: "did:icn:example-brightworks-steward-not-live",
        },
        { key: "transition", value: "assigned → completed" },
        { key: "completed_at", value: "1755216000" },
        {
          key: "record_hash",
          value: "9f2a7c41d8e0b3…",
          note: "Shown hex-encoded and truncated for reading. On the wire this is a 32-byte array.",
        },
      ],
      note: "The field is named recorded_by / actor_did rather than approver, deliberately. Recording an act is not the same as having had the authority to perform it, and the data model refuses to conflate them.",
    },
  },
  {
    id: "memory",
    n: "06",
    title: "The record joins the institution’s memory",
    conceptId: "accounting",
    plain:
      "Each record points back at the one that led to it, so a year later the cooperative can reconstruct not just what it did but why it was allowed to.",
    narrative: [
      "Every record in the chain carries the previous record’s hash. Follow them backwards and you arrive at the decision that authorized the work, and then at the rule that made the decision binding.",
      'This is what an institution loses when its history lives across a chat archive, a spreadsheet, and a shared drive: not the individual facts, but the links between them. Reconstructing "why were we allowed to do this" becomes an archaeology project instead of a query.',
    ],
    technical: {
      recordName: "ADR-0026 process receipt chain",
      definedIn: "icn/crates/icn-governance/src/proof.rs",
      fields: [
        {
          key: "DecisionRecordedReceipt.record_hash",
          value: "4b81e0…",
          note: "The decision that authorized the work.",
        },
        {
          key: "ActivationCrossedReceipt.decision_record_hash",
          value: "4b81e0…",
          note: "Points back at the decision — same hash, now carried forward.",
        },
        {
          key: "MutationPlanRecordedReceipt.activation_record_hash",
          value: "c70d95…",
        },
        {
          key: "MutationAppliedReceipt.plan_record_hash",
          value: "a1f3b8…",
        },
        {
          key: "EvidencePacketProducedReceipt.mutation_applied_record_hash",
          value: "2e6470…",
        },
      ],
      note: "Not every step of this chain is implemented to the same depth today. The maturity page states which parts carry real evidence and which do not.",
    },
  },
];

/**
 * The chain rendered in step 06, as an explicit ordered list so the diagram
 * and its text equivalent are generated from one source and cannot disagree.
 */
export const RECEIPT_CHAIN: Array<{ label: string; carries: string | null }> = [
  { label: "Decision recorded", carries: null },
  { label: "Activation crossed", carries: "the decision’s hash" },
  { label: "Change planned", carries: "the activation’s hash" },
  { label: "Change applied", carries: "the plan’s hash" },
  { label: "Evidence packet produced", carries: "the applied change’s hash" },
];

/**
 * What this walkthrough does and does not demonstrate. Rendered verbatim on
 * the page. #2610 requires the boundary between implemented behaviour,
 * fixture data, and future capability to be explicit rather than inferred.
 */
export const WALKTHROUGH_BOUNDARIES = {
  real: [
    "The record shapes, field names, and the hash-chaining between receipts are the ones the implementation uses.",
    "The distinction between authority (why an act was permitted) and a receipt (that it occurred) is enforced in the data model, not just in this description.",
    "Standing, action cards, previews, and completion receipts all exist as implemented surfaces in the repository.",
  ],
  fixture: [
    "Brightworks Collective, the workplace-safety domain, the extraction fan, and every identifier and hash on this page are invented for this walkthrough.",
    "No live institution, partner, or deployment is involved, and no data is fetched when you load this page.",
    "The values are literals in the site source, so the page renders identically every time.",
  ],
  future: [
    "This guided sequence is not itself a shipped product surface — it is an explanation assembled from the record shapes.",
    "The member-facing interface that would present this to an actual member is the part of the system furthest behind the rest of it.",
    "Some steps of the receipt chain are implemented more deeply than others; the maturity page is where that is stated per subsystem.",
  ],
};
