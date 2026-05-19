---
Status: normative
Authority: spec (defines the design-level rendering contract for the ICN member shell at v0; consumes ADR-0020 `/me/standing`, ADR-0027 ActionCard schema, ADR-0028 accessibility baseline, and the closed status-string sets from `docs/spec/compute-placement-policy.md` and `docs/spec/network-anti-entropy-proof-loops.md`; some clauses are explicitly forward-direction)
Canonical: no
Last Reviewed: 2026-05-15
---

# Member Shell v0

> **Status: spec, work-in-progress.** Defines the ICN member shell as the primary participation surface at v0: mobile-first, offline-tolerant, accessibility-first, plain-language-first. The shell is the **app-side rendering surface** that turns ADR-0020 standing, ADR-0027 ActionCards, ADR-0026 receipts, and the merged sibling specs' status vocabularies into participation. It does **not** redefine any of those contracts. It does **not** specify a native app, a PWA, an iOS/Android decision, a new backend endpoint, or any partner-specific skin. The PR introducing this doc advances `#1818` without closing it.

<!-- truth-class: normative -->

## Purpose

A member shows up to participate in their cooperative, community, or federation. The member shell is what they see. It answers, in plain language:

1. **Who am I in this institution?**
2. **What is my standing?** What am I currently authorized to do?
3. **What scope am I acting in?**
4. **What actions are open to me?** And what is closed to me, and why?
5. **What authority allows this action?**
6. **What will happen if I sign / confirm?** What is the consequence, what is reversible, what is not?
7. **What receipt proves what happened?**
8. **What is still syncing, delayed, challenged, private, or under review?**
9. **How do I ask for help, challenge, review, revoke, or exit where allowed?**

This spec defines the v0 contract for that surface as a design-level rendering specification. It is not an implementation; no code lands here. It is the contract a future native app, PWA, web shell, or institution-package skin must satisfy to be called "the ICN member shell."

## Scope and non-goals

**In scope:**

- The boundary between the member shell, the steward cockpit, the node operator civic-role surface, the public website, the institution-package skin layer, and the runtime / gateway.
- Mobile-first, offline-tolerant, accessibility-first, plain-language-first design principles.
- The member-shell information architecture at v0 (the surfaces the shell must offer; not the visual design).
- The rendering contract for ADR-0027 ActionCards.
- The rendering contract for ADR-0020 standing.
- The signing / confirmation flow contract.
- The receipt / provenance rendering contract.
- Offline / low-bandwidth behavior, including the sync-degradation language from merged `docs/spec/network-anti-entropy-proof-loops.md`.
- Privacy and ScopedVault member affordances (per merged `docs/spec/artifact-registry-and-scoped-vault.md`).
- The closed accessibility baseline (per ADR-0028 and `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`).
- A closed v0 member-facing status vocabulary.
- A failure / safety table.
- A fixture-first, design-review-first dogfood slice.

**Not in scope (preserved out of this PR):**

- Not a native iOS / Android / PWA implementation. No platform decision.
- Not a runtime, gateway, or scheduler change. The shell consumes existing surfaces; it does not define new ones.
- Not a redefinition of ADR-0020 `/me/standing`, ADR-0027 ActionCard, ADR-0025 EffectRecord, ADR-0026 receipt envelope, ADR-0028 accessibility baseline, or any merged spec (#1819, #1820, #1821, #1822, #1823, #1824, #1826, #1829).
- Not a new endpoint specification beyond cross-referencing existing `/me/standing` and `/me/action-cards` surfaces.
- Not a steward cockpit specification (`#1795`) or a node operator civic-role surface (`#1613`). Those are separate surfaces with their own contracts.
- Not the institution-package skin / theme layer. Packages localize; the generic shell does not pre-decide.
- Not a wallet, account-management dashboard, fintech product, balance/trading interface, or generic admin panel.
- Not the public website (`web/` non-`pilot-ui` surfaces).
- Not NYCN-specific or named-partner-specific framing. Institution packages bring their own labels; the generic shell stays generic.
- Not a production-readiness or live-federation claim.
- Not closure of `#1818`. The PR introducing this doc uses `Refs:`; closure is left for separate review against the issue's acceptance criteria.
- Not the `signal_rule` (#1631) or `obligation_lifecycle` (#1634) ActionCard source paths. Those remain RFC-gated; this spec lists them only as reserved enum values per ADR-0027.

## Boundary lines

The member shell lives in a clearly-bounded surface space. Each boundary below is load-bearing for keeping the shell honest at v0.

### Member shell vs. steward cockpit (`#1795`)

The member shell is **the member's view of their own participation**. The steward cockpit is **the operator's view of the institution's operability**. They render different things from the same underlying receipts and evidence.

- The member shell renders **plain participation status** (Synced, Action paused until records sync, Receipt available, Review required, etc.). It does not render `DivergenceEvidence` class names, peer DIDs, digest forms, or `policy_version_id` hashes.
- The cockpit renders **technical detail** (the eight cockpit fields from `docs/spec/network-anti-entropy-proof-loops.md` §"Steward cockpit surface"). The shell does not.

The same divergence event surfaces in both, with different vocabularies. The shell never shows the cockpit's technical detail; the cockpit never collapses to the shell's plain-language summary alone.

### Member shell vs. node operator civic-role surface (`#1613`)

The node operator civic-role surface is a separate operator-facing affordance for cooperative-operators who also run infrastructure. It is **not** the member shell. A member who happens to also operate a node sees the operator surface in addition to, not instead of, the member shell.

### Member shell vs. public website

The public website is the institution's outward face. The member shell is the institution's inward face. The website does not need a sign-in or a standing claim. The shell requires both.

### Member shell vs. institution-package skin / theme

Institution packages (cooperative-specific, federation-specific, partner-specific) localize the member shell with their own role nouns ("steward," "delegate," "co-chair," etc.), ceremony markers, and visual identity. The **generic** member-shell spec stays generic: it names the structural concepts (`LocalDomain`, `InstitutionalDomain`, `Domain`, `DomainPolicy`, `Mandate`, `AuthorityClass`, `ActionCard`, `Standing`), per `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3, and lets packages bring local nouns.

### Member shell vs. backend / runtime

The member shell is **app-side rendering** per `docs/architecture/KERNEL_APP_SEPARATION.md`. The kernel never imports member-shell strings, layouts, or status enums. The shell consumes existing app-side surfaces (`/me/standing`, `/me/action-cards`, ADR-0026 receipts, the merged sibling specs' status vocabularies) and renders them. It does not define new wire formats, new schemas, new endpoints, or new receipt classes.

## Design principles

Ten v0 principles, each load-bearing for what the shell is allowed to be:

1. **Mobile-first.** The shell is designed for a small touch screen on an older device on a slow network. Desktop layouts are the **secondary** target, not the primary. Large tap targets (≥ 44 × 44 px per ADR-0028), single-column-first information density.
2. **Accessibility-first.** Every surface must pass the twelve-category gate in `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` before it can be called member-facing. Failing any category gates the surface from the "member-facing / pilot-ready" track.
3. **Offline-tolerant.** Read paths cache locally; write paths queue with explicit "draft intent" framing until signed and acknowledged. Local cache is **derived state**, never source of truth.
4. **Plain language before formal records.** Every receipt, ActionCard, and standing entry shows a one-sentence plain-language summary first. The formal record (hash, version id, signature, proof envelope) is available second, under a clearly-labeled "details" affordance.
5. **Receipt / provenance visible for every state change.** Every action the member takes that mutates institutional state produces a receipt the shell can render and link to. Every visible state change has an explanation that points back to a receipt or an open evidence trail. No silent state changes.
6. **Scope-aware action.** The shell always shows the member which `LocalDomain` (or wider scope) they are acting in. An ActionCard whose required authority scope does not match the member's current standing is rendered as visibly closed, with an explanation.
7. **Safe signing confirmations.** No signing happens without a pre-confirm summary that names the authority basis, the scope, the consequence, the reversibility window (or its absence), the expected receipt class, the privacy / disclosure warning (when relevant), and the offline / sync warning (when relevant).
8. **Local cache is derived state, never source of truth.** The member sees "last refreshed at <time>" on every read view that may be cached. If the cache is stale beyond a policy window, the shell marks it stale; if a pending action depends on stale state, the shell renders "Action paused until records sync" rather than letting the member act on stale data.
9. **Multilingual / inclusive-access path.** The shell must integrate with the language / glossary model in `#1610` and the multilingual access plan in `#1740`. ActionCard `title` and `summary`, standing entries, and the closed status-string set are all translation-tagged at the rendering layer; translation fallbacks are explicit, not silent.
10. **No financial-product framing.** The shell never uses payment, currency, balance, wallet, token, crypto, or blockchain framing for ICN-native participation. The vocabulary discipline in `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` §"Vocabulary discipline" applies in full. Settlement, position, obligation, allocation, receipt, and provenance are the spec-facing terms.

## Member shell information architecture

The v0 shell offers the following surfaces. The list is the **minimum**; institution-package skins may add, but a shell missing any of these is not v0.

| Surface | Purpose | Sources consumed |
|---|---|---|
| **Home / Today** | One-screen-at-a-glance: who am I, what's open to me today, what's pending, what's syncing. | `/me/standing` + `/me/action-cards` + sync-status vocabulary. |
| **My Standing** | The member's identity, devices, memberships, roles, mandates, authority class set, current scope. The "who am I in this institution?" surface. | `/me/standing` (ADR-0020). |
| **Current Scope** | Which `LocalDomain` / `InstitutionalDomain` / wider scope the member is acting in. Switcher if the member has standing in multiple domains. | `/me/standing` (domains list). |
| **Action Cards** | The list of open ActionCards, with status, deadline, authority basis, expected receipt. | `/me/action-cards` (ADR-0027). |
| **Decisions / Governance** | Open proposals the member can vote on, recent decisions in scope, the proposal-to-decision-to-effect chain. | `/me/action-cards` proposal source paths + governance read surface (forward-direction). |
| **Receipts** | The member's own receipts — every state change they participated in, with plain-language summaries, the formal record under "details," and the challenge / review path. | ADR-0026 receipt envelope (Layer 1–3); future ProvenanceQuery Layer 4 (`#1438`). |
| **Records / Artifacts** | The member-visible artifact list scoped to what their standing exposes. Private artifacts surface as opaque references (existence + scope + access path only). | ArtifactRegistry (`docs/spec/artifact-registry-and-scoped-vault.md`). |
| **Privacy / Access** | What private data exists about the member, who can access it, why, and the access / export receipts trail. | ScopedVault + access receipts (per `docs/spec/artifact-registry-and-scoped-vault.md`). |
| **Sync / Offline status** | The current sync state surface (the seven anti-entropy strings), the queue of draft intents, the queue of awaiting-acknowledgement signed actions. | `docs/spec/network-anti-entropy-proof-loops.md` §"Member shell surface." |
| **Help / Challenge / Review / Exit** | The member's path to ask for help, challenge a record, request review, revoke a delegation, or exit a relationship where the domain's policy permits it. | Domain policy (per `docs/spec/institutional-domain.md`). |

The shell may offer more surfaces; a v0-conformant shell offers at least these.

## ActionCard rendering contract

ADR-0027 defines the canonical ActionCard schema with fourteen fields. The spec **does not redefine those fields**. It defines the **rendering contract** — how the shell must surface each field to the member.

### Mandatory rendering for every ActionCard

| ADR-0027 field | Rendering requirement |
|---|---|
| `title` | First line, plain language, visible in the list row. |
| `summary` | One-line plain-language summary; visible in the list row. |
| `action_kind` | Mapped to a closed v0 plain-language verb set (see §"Member-facing status vocabulary" below). Not the raw enum value. |
| `source_kind` | Mapped to a closed v0 plain-language source label (e.g., `proposal` → "Proposal in your domain"). Not the raw enum value. |
| `scope` | Rendered as the human-readable domain name (per institution-package localization) plus the current-scope context. The member always sees which scope this card belongs to before they confirm. |
| `authority_basis` | Rendered verbatim in plain language (ADR-0027 already requires the field to be plain-language). Visible in the pre-confirm summary. |
| `required_authority_scope` | Rendered as either "you are authorized for this" (matches member's current standing) or "this requires authority you do not currently hold" with the specific missing authority class named. |
| `deadline` | If present, rendered as a relative-time clock ("closes in 2 days") with the absolute timestamp under "details." If absent, rendered as "no time pressure" rather than "no deadline" (more honest framing). |
| `risk_level` | Rendered as a coarse visual indicator (low / medium / high), color **and** glyph **and** label (per ADR-0028 color-independence rule). Not numeric; not technical. |
| `accessibility_hint` | Rendered as plain-language preamble on the card, before any decision controls. Required by ADR-0028. |
| `receipt_expected` | Rendered as the expected outcome line: "If you confirm, this will produce a <receipt class summary>." Plain-language receipt-class name from the closed shell vocabulary. |
| `source_id` | Not rendered to the member by default. Available under "details" for the technically-inclined. |
| `domain_id` | Not rendered as an id. Translated to the institution-package domain name. |
| `id` | Not rendered to the member by default. Available under "details" for support / challenge contexts. |

### Card states the shell must distinguish

- **Open** — the member can act now; all conditions are met.
- **Open but paused** — the card is open in principle but `Action paused until records sync` (per `docs/spec/network-anti-entropy-proof-loops.md`). The confirm control is disabled; the explanation is plain.
- **Closed: insufficient authority** — `required_authority_scope` does not match the member's standing. The shell renders the missing authority class plainly and offers a "request authority" path if the domain's policy permits one.
- **Closed: deadline passed** — the deadline elapsed before the member acted.
- **Closed: superseded** — a later governance action made this card obsolete.
- **Closed: confirmed** — the member confirmed; the receipt is available or pending.
- **Closed: declined** — the member explicitly declined; the receipt is available if the action kind produces one.
- **Forward-direction** — `signal_rule` / `obligation_lifecycle` source-path cards are listed under `#1631` / `#1634` and rendered as "available soon" placeholders only when the domain's policy adopts them; they are not enabled by the generic v0 shell.

### Reserved source paths

Per ADR-0027 the closed `source_kind` taxonomy reserves `signal_rule` and `obligation_lifecycle` against `#1631` and `#1634`. The shell **must not** render either source path as live until the corresponding implementation lands. Until then, those values are inert at the rendering layer.

### v0.2 rendering refinements

> Promoted from the v0.2 Claude Design seed (`seed/MEMBER_SHELL_v0.2_PATCH.md`) per [`docs/design/claude-design-seed/CHANGELOG.md`](../design/claude-design-seed/CHANGELOG.md). Refinements only — the 14-field ADR-0027 schema stays canonical. This section tightens the contract on how the shell **renders** uncertain network state, gated cards, and the decision-threshold preamble.

#### Sync-state chip (every card)

Every ActionCard carries a **sync-state chip** that names the action's current state in the local-sync queue and the gateway. Six named states; no silent failure; no spinner without a label:

- **drafted** — the member started the action; nothing left the device
- **saved on device** — the action is durable locally but not yet sent
- **sent to network** — the action left the device; awaiting confirmation
- **confirmed** — the gateway accepted the action and emitted the expected receipt
- **offline queued** — the device is offline; the action will replay when network returns
- **rejected with reason** — the gateway rejected the action; the reason is named on the card

The chip is text + icon. Color reinforces; the named state is load-bearing. A grayscale render must still distinguish the six states.

#### Stale, sync-in-flight, degraded scope (network honesty)

When the gateway state is uncertain, every affected card surfaces it explicitly:

- **Stale** — last gateway confirmation > 5 minutes ago. The card renders a "last confirmed" timestamp below the sync-state chip.
- **Sync in flight** — the action has been sent but not confirmed. The sync-state chip reads "Sent · awaiting confirmation" with a sub-line counting elapsed time.
- **Degraded scope** — the gateway reports degraded mode for the scope (reduced consensus, partition-tolerant). A scope-level banner above the affected cards reads "This scope is operating in degraded mode. Decisions may delay." Dismissible per session; reappears on every relevant card until the gateway clears the flag.

Each state has a text companion; no color-only signaling, no motion-only signaling.

#### "Unavailable" — gated cards never hide, they explain

When an action is gated and the member cannot perform it, the card renders in an explicit unavailable state that **names the missing condition**:

- "Action unavailable. Mandate `Charter §4.2` requires `recognized-member` standing. Your standing in `Brightworks Collective` is `applicant`."
- "Action unavailable. Quorum not reached. 4 of 7 votes recorded."
- "Action unavailable. Offline. Action will queue until network reconnects."

The card never silently disables the action. It always names why. This refines the existing "Closed: insufficient authority" state by requiring the missing-condition text be plain language and member-actionable.

#### Threshold + tally above confirm (governance actions)

For governance ActionCards, the **decision threshold** required (quorum, supermajority, simple majority) and the **current tally** are rendered above the confirm step — not hidden under "details." Source: the governance domain's charter-derived rule (already shown in [`docs/mobile/icn-mobile-ux-spec-v1.md`](../mobile/icn-mobile-ux-spec-v1.md) §3.1 as the `Threshold info` row). The refinement is placement: the member sees the decision rule before casting a vote, not after.

#### Cross-reference

These v0.2 refinements elaborate [ADR-0027 (Action Card Contract)](../adr/ADR-0027-action-card-contract.md) at the rendering layer. ADR-0027 remains the canonical schema; this section is the rendering contract's v0.2 update. See also [`docs/design/CONTENT_STYLE_GUIDE.md`](../design/CONTENT_STYLE_GUIDE.md) "Dangerous-action copy" template, which has been updated to require sync state and threshold + tally before the confirm step.

## Standing surface

The Standing surface renders `/me/standing` (per ADR-0020) at v0. The shell must surface:

- **Identity** — the member's DID, device name, last-active timestamp. The DID itself is shown under "details"; the surface uses a plain display name when one is configured.
- **Memberships** — the list of `LocalDomain` / `InstitutionalDomain` entries the member belongs to, each with its owning entity class (Cooperative / Community / Federation / Individual / other governed class).
- **Roles** — the member's roles per domain. Generic structural names by default; the institution package may localize.
- **Mandates and authority** — the active mandates covering the member, with plain-language `AuthorityClass` labels (Representation / Execution / Attestation per ADR-0014). Each mandate names its scope, its grant origin, and its expiry / revocation conditions.
- **Current scope** — the domain the member is currently acting in, with a switcher when the member has standing in multiple.
- **What is open to me now** — a count and link to the current ActionCard list.
- **What is unavailable and why** — a count of cards closed for "insufficient authority" with a one-line explanation of each blocker. This is the member's first signal that something is gated.

The shell does **not** show raw `AuthorityGrant` content hashes, raw `policy_version_id` strings, or other technical identifiers as default surface. Those live under "details" for support and challenge contexts.

## Signing / confirmation flow

No member action that mutates institutional state happens without passing through a pre-confirm summary. The flow is:

1. **Review summary.** Title + summary + plain-language description of what will change.
2. **Authority basis.** The mandate / policy clause that authorizes the action. Rendered from ADR-0027 `authority_basis` field.
3. **Scope.** The `LocalDomain` (or wider) the action affects. Rendered from ADR-0027 `scope` field, translated to the institution package's domain name.
4. **Consequence.** What will be true after the action that was not true before. Rendered in plain language; institution-specific consequences localized.
5. **Reversibility / challenge window.** Plain-language statement of whether the action is reversible and within what window. If irreversible, the shell renders an explicit "**this cannot be undone**" line and requires a distinct confirmation step (not a single tap).
6. **Receipt expected.** Plain-language name of the receipt class the action will produce (per ADR-0026 / ADR-0025), with a note that the receipt will appear in the member's Receipts surface.
7. **Privacy / disclosure warning.** If the action involves private data — exposing, sharing, requesting access — a plain-language disclosure warning before confirm.
8. **Offline / sync warning.** If the shell is currently in `Sync delayed`, `Some records are being verified`, `Action paused until records sync`, or `Sync delayed / degraded` state, the confirm control is **either** disabled (when the action's authority depends on the affected state class) **or** the warning is shown and the action is queued as draft intent (when the authority is independent of the degraded class).
9. **Confirm / cancel.** Two clearly-distinct affordances. Confirm requires a deliberate gesture (per ADR-0028 cognitive-load principle). Cancel always available, returns the member to the card list.
10. **Post-action receipt status.** Immediately after confirm, the shell renders one of: `Receipt available` (the receipt landed before the shell returned), `Some records are being verified` (the receipt is pending and the loop is in progress), or `Review required` (the action requires further governance ratification, e.g., advisory output per ADR-0030).

Draft intent vs. signed-and-committed is always legible. A queued draft is plainly labeled "Draft — will be sent when records sync." A signed-but-not-acknowledged action is plainly labeled "Sent — waiting for receipt." A fully-acknowledged action shows the receipt summary.

## Receipt and provenance rendering

Receipts are the spine of the shell. Every visible state change connects to a receipt. The rendering contract:

1. **Plain summary first.** One line: what happened, when, in what scope, with what consequence. No technical identifiers.
2. **Explanation second.** Two-to-three sentences: what authority allowed it, what came before (linked), what comes next (if applicable).
3. **Formal record third.** Under a clearly-labeled "details" affordance: receipt class name (`GovernanceDecisionReceipt`, `ActionItemCompletionReceipt`, `MeetingAttendanceReceipt`, `EffectDispatchEvidence`, future `RatificationReceipt`, etc.), receipt id, signature, mandate reference, policy_version_id, Stage 5 evidence link per `docs/spec/effect-dispatch-contract.md`.
4. **Challenge / review path always visible where applicable.** If the domain's policy provides a challenge window or a review surface for this receipt class, the shell renders the "challenge / request review" affordance with the deadline.

The shell renders the **plain summary** even when the formal record is incomplete (e.g., during the Stage 5 evidence-still-arriving window). It marks the formal record as pending in that case.

The shell renders the **artifact-class** label from `docs/spec/artifact-registry-and-scoped-vault.md` (Document, ComputeOutput, EvidencePacket, PrivateEvidence, Backup, SettlementRecord, Other) when a receipt references an artifact. `PrivateEvidence` artifacts are rendered as opaque references — existence and scope and access-path only, never body content.

## Offline / low-bandwidth behavior

The shell must function on a slow or intermittent network on an older device. The contract:

- **Cache as derived.** All read views may be cached locally. The cache shows "last refreshed at <time>." Stale-beyond-policy-window cache is visibly stale.
- **Stale markers.** A stale list row, card, or receipt summary shows a "this is the last copy we have; refresh when online" affordance.
- **Blocked actions where sync evidence is required.** Per `docs/spec/network-anti-entropy-proof-loops.md` Boundary rule 6, federation- or commons-scope actions require fresh sync proof; the shell renders these as `Action paused until records sync` with a plain-language explanation rather than letting the member act on stale evidence.
- **Draft intent vs. signed / committed action.** A draft intent is queued locally with no signature. A signed action is committed (signature is final) but its receipt may be in transit. The shell distinguishes the two visibly.
- **Queued-but-not-accepted distinction.** A signed action that has not yet been acknowledged by the institution is "Sent — waiting for receipt," not "Confirmed." Confirmation requires the receipt.
- **Sync degradation language.** The shell uses the closed seven-string set from `docs/spec/network-anti-entropy-proof-loops.md` §"Member shell surface" and the closed seven-string set from `docs/spec/compute-placement-policy.md` §"Member shell" verbatim. No invention.

## Privacy and ScopedVault member affordances

Per merged `docs/spec/artifact-registry-and-scoped-vault.md` and forward-tracked `#1792` / `#1767`, private data lives in scoped vaults and never travels raw through gossip or shell-level caches. The shell's contract:

- **Existence metadata, never restricted contents.** A `PrivateEvidence` artifact (per the artifact-class taxonomy) surfaces as "a private record exists in this scope" — never a preview of the body.
- **Who can access.** The shell renders the list of access-holders in plain language (e.g., "your steward of record," "the privacy reviewer for this scope") when the policy permits the member to see that list.
- **Why they can access.** The mandate or policy clause that authorizes the access, rendered plainly.
- **Access / export receipts.** Every access to a member's private record produces a receipt. The shell renders these in the member's Receipts surface, plainly summarized.
- **Request / review / challenge path.** The shell offers the member a path to request additional access, request review of who has access, or challenge an access that they believe was inappropriate.
- **No dashboard preview of private vault content.** The shell does not paginate, preview, or surface body bytes of private vault objects. The opaque reference + existence + scope is the surface, full stop.

## Accessibility baseline

The shell inherits the twelve-category gate from `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` and ADR-0028. **Every** member-facing surface must pass all twelve, or it is gated from the member-facing track:

1. **Language access** — plain language, glossary hooks (per `#1610`), translation readiness (per `#1740`).
2. **Screen-reader and non-visual access** — semantic HTML or platform-native equivalents, meaningful headings, ARIA labels, structured summaries.
3. **Low-vision access** — 200% zoom support, WCAG AA contrast (4.5:1 body, 3:1 large), no sub-14px critical labels.
4. **Color-independent meaning** — grayscale-safe status; color reinforces, never the only carrier. Glyphs and labels in addition to color for risk level, sync status, and receipt state.
5. **Keyboard, switch, non-pointer access** — fully reachable via keyboard / switch, visible focus state, no traps, ≥ 44 × 44 px hit targets.
6. **Captions, transcripts, non-audio access** — paired captions / transcripts; audio never sole signal.
7. **Cognitive load and step complexity** — progressive disclosure, consequences explained pre-action, reversibility legible.
8. **Low-bandwidth and low-device access** — mobile-first, older devices, minimal animations, exportable for offline reading.
9. **Assistive-technology compatibility** — real semantic structure, standard inputs, switch-control and screen-reader tested.
10. **Privacy-preserving accommodation path** — accommodation data lives in private overlays per `#1767`, never in public git or in plain-text cache. The shell never persists accommodation profile locally without encryption.
11. **Receipts, provenance, evidence access** — plain-language summaries before raw data, clear fixture-vs-live boundaries.
12. **Governance and action access** — ActionCards expose authority basis, status, reversibility, decision, and consequence in plain language **before** any confirm control.

Failing any category gates the surface from the "member-facing / pilot-ready" track. No exceptions. The shell is not "for some members"; it is for all members, including older members, members on older devices, members with disabilities, members on slow networks, and members in non-English-default settings.

## Member-facing status vocabulary

The v0 shell uses a **closed** plain-language vocabulary. The closed set is the union of the merged sibling specs' member-shell strings plus a small set of action-status strings the shell needs for its own life-cycle. Inventing new strings outside this set is a v0 violation; the v0.x or v1 conversation can extend the set.

### Sync state (from `docs/spec/network-anti-entropy-proof-loops.md` §"Member shell surface")

- **Synced**
- **Sync delayed**
- **Some records are being verified**
- **Action paused until records sync**
- **Receipt available**
- **Review required**
- **Sync delayed / degraded**

### Execution scope (from `docs/spec/compute-placement-policy.md` §"Member shell")

- **local execution**
- **processed inside your institution**
- **sent to a federation executor under agreement**
- **commons compute pending**
- **review required** (overlaps with sync state; same string)
- **receipt available** (overlaps with sync state; same string)
- **sync delayed / degraded** (overlaps with sync state; same string)

### Action lifecycle (defined here as v0)

- **Open** — the action is available to confirm.
- **Open but paused** — the action is open but blocked pending sync or related state.
- **Draft — will be sent when records sync** — the member started but the shell is offline / degraded.
- **Sent — waiting for receipt** — the member signed; the receipt has not arrived.
- **Confirmed** — the receipt is available; the action is complete.
- **Declined** — the member declined; the action is closed.
- **Closed — deadline passed**.
- **Closed — superseded**.
- **Closed — insufficient authority**.

### Privacy / disclosure (defined here as v0)

- **Private record exists**
- **Access pending review**
- **Access granted**
- **Access denied**
- **Access export receipt available**

### Receipt-class plain-language labels (mapping to ADR-0026 / ADR-0025)

The shell maps technical receipt class names to a small set of plain-language labels:

- "Governance decision recorded" → `GovernanceDecisionReceipt`
- "Action completed" → `ActionItemCompletionReceipt`
- "Attendance recorded" → `MeetingAttendanceReceipt`
- "Effect applied" → `InstitutionalEffectRecord` + `EffectDispatchEvidence`
- "Output produced" → `ArtifactReceipt` (Layer 2)
- "Advisory output awaiting ratification" → forward `RatificationReceipt` (when implemented)
- "Repair recorded" → `RepairReceipt` (forward-direction, per `docs/spec/network-anti-entropy-proof-loops.md`)
- "Settlement recorded" → settlement receipt class (per `docs/spec/federation-settlement-finality.md`)

The full receipt class name is available under "details," never as the primary surface.

## Failure and safety table

The following failures are member-facing concerns. Each row names the failure, where it surfaces, and the shell's required disposition.

| Failure | Where it surfaces | Disposition |
|---|---|---|
| `/me/standing` returns empty or fails | Standing surface | Render "Your standing is currently unavailable. We'll keep trying. If this persists, please contact your steward." Plain language; no error code as primary surface. Help path always visible. |
| ActionCard generated without an `authority_basis` field | ActionCard rendering | Card rendered as "Closed: missing authority basis. This card is misconfigured; please contact your steward." Never silently shown as actionable. |
| Member signs while sync is degraded | Signing flow | If the action depends on the degraded state class: confirm control disabled with "Action paused until records sync." If the action is authority-independent of the degraded class: confirm allowed with explicit "Sync delayed — your action will be queued" warning. |
| Receipt missing after confirmed action | Receipts surface | "Sent — waiting for receipt" status persists; shell offers "this is taking longer than expected — request review" affordance after the policy's grace window expires. |
| Local cache is stale beyond policy window | All cached surfaces | Stale marker visible on every affected list row; pending actions that depend on the stale class render `Action paused until records sync`. |
| Private record existence hidden when policy says it should be visible | Privacy / Access surface | Disposition is a policy-side decision, not a shell-side default. The shell surfaces what the policy authorizes; if the policy is silent, the shell defaults to **showing existence** so the member is not blind to records about them. |
| Restricted content exposed in shell | Any surface | Shell-side hard rule: never. Bodies of private artifacts never reach the rendering layer. If the rendering layer receives body bytes for a `PrivateEvidence` artifact, the surface drops them and logs a safety failure. |
| Challenge window missing on receipt class that should have one | Receipts surface | Plain-language note: "The challenge path for this kind of record is being defined. Please contact your steward if you wish to dispute." Receipt remains visible. |
| Translation missing for current language | Any surface | Fallback to the institution's default language with a visible "translation pending" tag. Never silent fallback; the member always knows the surface is showing a non-native string. |
| Accessibility gate failed for a surface | Any surface | The surface is **not allowed** in member-facing v0. Gate failure is a v0 violation; the surface is removed from the build until it passes. |
| Scope ambiguous (member has standing in multiple domains and the shell does not know which one to render) | All surfaces | Scope switcher rendered prominently with "Acting in <domain>" label. Member must explicitly select before confirm controls become active for cross-domain actions. |
| Offline draft intent mistaken for committed action | Action lifecycle rendering | Draft intent always labeled "Draft — will be sent when records sync." Confirmed action labeled "Confirmed." Two visibly distinct labels and affordances. |
| Member presented with raw scheduler, runtime, or cockpit jargon | Any surface | v0 violation. The shell uses the closed plain-language vocabulary above. Raw jargon (DivergenceEvidence class names, policy_version_id strings, peer DIDs, BloomFilter, etc.) belongs to the cockpit, not the shell. |
| Financial vocabulary (payment, wallet, balance, currency, token, crypto, blockchain) appears in member shell | Any surface | v0 violation. Replace with settlement / position / obligation / allocation / receipt / provenance. Forbidden as positive ICN-native framing. |
| Action labeled reversible when it is not (or vice versa) | ActionCard rendering | The reversibility window comes from the underlying source. The shell must surface what the source declares. A reversible action lacking a challenge window prompts the safety table row above ("challenge window missing"). |
| Action irreversible but not labeled as such | Signing flow | Confirm gated by an explicit "this cannot be undone" line and a distinct second confirmation step. Single-tap confirm is never allowed for irreversible actions. |
| Member uses help / challenge / review / exit path and receives no response | Help surface | The institution's policy defines response cadence. The shell surfaces "Your request was received at <time>. Expected response window: <policy-defined>." If the window elapses with no response, the shell surfaces escalation per policy. |
| Institution-package skin overrides a generic structural label (e.g., renames "Mandate" to a partner-specific noun) | Any surface | Allowed by policy as long as the generic structural concept stays available under "details." The shell never **loses** the structural concept; the package only **adds** the local noun. |

## First safe proof-loop / dogfood slice

Per `#1818`'s acceptance criterion and the established session pattern, the spec names a docs- and fixture-first slice that exercises the contract without touching real members, real partner institutions, or a real backend.

### Slice A (preferred): Read-only standing + ActionCard + receipt + sync-delayed fixture

- **Fictional member.** A fixture DID (e.g., `did:icn:demo:member-1`) with a local signing key, a fixture display name, and a fixture device.
- **Fictional institution / domain.** A fixture `LocalDomain` (e.g., `did:icn:demo:exampledomain`) with the four merged-spec scope vocabularies available.
- **One vote / review ActionCard.** A fixture ActionCard of `source_kind = proposal`, `action_kind = vote`, with a fixture `authority_basis` ("you are a voting member of this domain") and a fixture deadline.
- **One completed action with receipt available.** A fixture `MeetingAttendanceReceipt` rendered as "Attendance recorded" with the plain summary, the two-sentence explanation, the formal record under details, and no challenge window (since attendance is not disputable here).
- **One action paused due to `Sync delayed / degraded`.** A fixture ActionCard rendered as "Open but paused — Action paused until records sync," with the confirm control disabled and the plain explanation visible.
- **One restricted / private record shown as existence metadata only.** A fixture `PrivateEvidence` artifact rendered as "Private record exists — access pending review," with the access path affordance present and no body content surfaced.
- **Accessibility checklist applied.** All twelve categories pass on the fixture rendering. Keyboard reachable, screen-reader navigable, color-independent, 200% zoom, plain language, captions where media is present, etc.
- **No real partner data.** No NYCN-specific labels, no named partner cooperatives, no real members.
- **No live network.** All data is fixture; no `/me/standing` call against a real gateway; no real receipts retrieved.
- **No runtime mutation.** The fixture is a rendering rehearsal. No state changes; no signed actions sent anywhere.

The slice produces evidence that the rendering contract is implementable, that the accessibility gate is passable at v0, and that the closed status-string vocabularies render in plain language. It does not implement the shell.

**Implementation status (`#1839`).** Slice A is implemented as a fixture-only Rust integration test at `icn/crates/icn-kernel-api/tests/member_shell_read_only_render_slice_a.rs`. The fixture builds the same Slice A `DivergenceEvidence` + `RepairPlan` proof rail the cockpit fixture (`#1840` / PR #1846) renders, then renders a `FixtureMemberShellView` — a standing surface, four ActionCards (proposal/vote Open; meeting/attend Confirmed with an `AttendanceRecorded` receipt summary; action_item/complete `Open but paused — Action paused until records sync`; proposal/vote `Closed — insufficient authority` with a plain-language explanation), and a `PrivateEvidence` reference rendered as existence + scope + access status + review-path only. The closed seven-string sync vocabulary (`Synced` / `Sync delayed` / `Some records are being verified` / `Action paused until records sync` / `Receipt available` / `Review required` / `Sync delayed / degraded`) is locked verbatim. The twelve-category accessibility checklist (`FixtureAccessibilityChecklist`) is evaluated on each rendered view and asserted to pass. The open-then-resolved transition takes the surface from `Sync delayed` to `Receipt available`. A targeted test asserts that no member-facing string contains protocol jargon (`Bloom`, `Merkle`, `vector clock`, `evidence_hash`, `plan_hash`, `DivergenceEvidence`, `RepairPlan`, `RepairReceipt`, `PeerSyncReport`, `AntiEntropyProbe`, `StateDigest`, `policy_version_id`, `protobuf`, `gossip`, `wallet`, `balance `, etc.), and structural tests bar body / content / payload / raw-bytes / secret fields on the private-reference and receipt-summary types. The public `RepairReceipt` and `PeerSyncReport` identifiers remain design-level — the resolved view carries the closed plain-language `Receipt available` label without claiming a wire-stable receipt has landed. The fixture is a proof-of-shape (the shell can describe the member's participation surface honestly), not a proof-of-runtime: it does not implement the shell, make a platform choice (iOS / Android / PWA / web / native), add a partner skin, ship a live network call, mutate runtime state, or advance the production-readiness or live-federation posture of the spec. The steward cockpit (`#1840`, landed via PR #1846) remains a separate surface.

### Slice B (after design review): Signing flow rehearsal

Same fixture stack. A fixture member walks the signing / confirmation flow for a fixture ActionCard. The shell renders the ten-step pre-confirm summary; the member confirms; a fixture receipt arrives. Asserts the flow's accessibility, plain-language consistency, and reversibility / privacy / sync warning surfaces.

### Slice C (after design review): Offline / degraded sync rehearsal

Same fixture stack. A fixture member starts an action while the shell is in `Sync delayed`. The action is queued as draft intent. The shell transitions through `Some records are being verified` and finally `Sync delayed / degraded`. The member sees plain-language warnings at each transition and is offered the help / review path.

All three slices are fixture-only. None implements the shell. None claims production readiness, partner-pilot operation, or live federation.

## Relationship to sibling work

| Concern | Where it lives |
|---|---|
| Member shell (this spec) | `#1818` |
| Steward cockpit operability surface | `#1795` |
| Node operator civic-role surface in Commons Shell | `#1613` |
| `/me/action-cards` endpoint | `#1608` (current vertical slice) + `#1646` (action-cards on top of `/me/standing`) |
| ActionCard `$id` compatibility review | `#1742` |
| `signal_rule` ActionCard source path | `#1631` (forward-direction, reserved enum) |
| `obligation_lifecycle` ActionCard source path | `#1634` (forward-direction, reserved enum) |
| Multilingual / inclusive-access path | `#1740` |
| Language / glossary endpoint | `#1610` |
| Network anti-entropy proof loops | `#1799` (merged #1829) |
| Compute placement policy | `#1801` (merged #1826) |
| ArtifactRegistry / ScopedVault | `#1798` (merged #1824) |
| Storage durability policies | `#1816` (merged #1823) |
| Governed service binding | `#1815` (merged #1822) |
| CCL policy registry | `#1817` (merged #1821) |
| Institutional domain and policy primitive | `#1794` (merged #1820) |
| Effect dispatch contract | `#1797` (merged #1819) |
| Integrated cooperative operating model | `#1793` (merged #1814) |
| Encrypted distributed private-overlay storage | `#1767` (forward-direction) |
| Private data disclosure boundary | `#1792` (forward-direction) |
| Bootstrap activation + standing read model | ADR-0020 |
| ActionCard schema | ADR-0027 + `docs/contracts/institution-package/action-card.schema.json` |
| Accessibility baseline | ADR-0028 + `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` |
| AuthorityClass / TypedScope / Mandate | ADR-0014 |
| Authority grant minting seam | ADR-0019 |
| Institutional Effect Record canonical schema | ADR-0025 |
| Receipt and provenance proof envelope | ADR-0026 |
| Compute workload manifest and authority boundary | ADR-0030 |
| Commons compute admission and settlement | ADR-0031 |
| Entity-scope vocabulary boundary | `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 |
| Kernel / app separation | `docs/architecture/KERNEL_APP_SEPARATION.md` |
| Integrated spine | `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` |

## Non-claims (repeat block for grep clarity)

- This spec does not implement a member shell. No code lands here.
- This spec does not specify a native app, an iOS / Android / PWA decision, or any platform choice.
- This spec does not define a new endpoint. It consumes existing `/me/standing` (per ADR-0020) and `/me/action-cards` (per `#1608` / `#1646`) surfaces; it does not extend, version, or otherwise mutate them.
- This spec does not redefine ADR-0020 standing, ADR-0027 ActionCard, ADR-0025 EffectRecord, ADR-0026 receipt envelope, ADR-0028 accessibility baseline, or any merged sibling spec.
- This spec does not introduce a new receipt class. It maps existing classes (per ADR-0026 / ADR-0025 / merged sibling specs) to plain-language labels at the rendering layer only.
- This spec does not implement the signal_rule or obligation_lifecycle ActionCard source paths. Those remain RFC-gated in `#1631` and `#1634`.
- This spec does not implement the steward cockpit, the node operator civic-role surface, or any operator-side surface.
- This spec does not specify partner-institution skinning. Generic shell stays generic; packages localize.
- This spec does not claim production readiness, a live partner federation, a formal NYCN pilot, or operation under this contract by any real institution today.
- This spec does not move, expose, preview, or cache private vault contents. Body bytes never reach the rendering layer.
- This spec does not use wallet, payment, balance, currency, token, crypto, or blockchain framing for ICN-native participation. All such terms in this doc appear in explicit negation context (design principle 10, failure-safety row, this non-claims block) or as references to existing legacy code identifiers preserved without endorsement (per the cross-sprint reconciliation plan).
- This spec does not authorize any change to the runtime, gateway, SDK, website, deploy scripts, K3s, DNS, Forgejo, or any deployed infrastructure.
- This spec does not close `#1818`. The PR uses `Refs: #1818`; closure is a user-driven decision against the issue's acceptance criteria.
