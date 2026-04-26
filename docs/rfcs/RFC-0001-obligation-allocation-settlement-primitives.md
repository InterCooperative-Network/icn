---
id: "0001"
title: "Obligation / Allocation / Settlement Primitives and External Settlement Bridges"
status: "draft"
created: "2026-04-26"
updated: "2026-04-26"
authors: ["Matt Faherty"]
reviewers: []
related_adr_candidates: ["0044", "0045", "0046", "0047", "0048", "0041", "0026"]
related_issues: ["#1634", "#1635"]
supersedes: []
superseded_by: []
---

# RFC 0001: Obligation / Allocation / Settlement Primitives and External Settlement Bridges

## Status

`draft` — being written, not yet ready for review. Per [RFC-0000](RFC-0000-rfc-process.md), accepted RFC does not mean implemented; the follow-up ADRs and tests carry that work.

## Summary

ICN needs a written model for cooperative economic coordination that is honest about two simultaneous realities:

1. **ICN-native economic coordination.** Obligations, allocations, settlements, units, positions, and receipts as governed institutional records — separate from payment-rail primitives, separate from currency.
2. **External settlement bridges.** Real institutions still need rent paid, venues deposited, payroll-equivalents disbursed, conversion loans funded, accountants engaged, and legal filings processed in the day-currency / regulated-financial systems they operate within today.

This RFC explores the design space for both layers and the boundary between them. The recommended direction is **Option B: External Bridge Receipts** — ICN records governance authority, settlement intent, obligations, allocations, positions, and receipts; external regulated/cooperative partners execute day-currency or legal-financial actions under their own authority and return a bridge receipt. ICN never becomes a bank, payment processor, currency issuer, or convertibility engine.

## Problem statement

Cooperatives, communities, and federations adopting ICN have to operate inside the present economy. They need to:

- Pay rent to a landlord that does not accept cooperative units.
- Cover venue deposits, travel support, printing, childcare, accessibility services, and legal filings using currency the wider world recognizes.
- Fund conversion loans, bridge loans, and payroll-equivalent disbursements through credit unions, cooperative lenders, CDFIs, fiscal sponsors, and (where applicable) public/community banks.
- Engage accountants, bookkeepers, legal/co-op developers, technical assistance firms, and organizer bodies whose work straddles the institutional boundary.

If ICN ignores this and only models internal coordination, real institutions cannot use it for the work that funds their existence. If ICN absorbs day-currency mechanics into its core, it becomes a payment system — with the regulatory burden, capture risk, and conceptual contamination that implies. ICN-native vocabulary (position, obligation, allocation) collapses into wallet/balance/currency, and the cooperative model is undermined.

The bridge problem is the gap. This RFC proposes that ICN occupies the **internal coordination + governance authorization + receipts** layer, and that external bridge institutions own the **regulated execution** layer, with a clean envelope between them.

## Goals

- Preserve regulatory-safe vocabulary in ICN-native primitives. ICN uses `obligation`, `allocation`, `settlement`, `unit`, `position`, `settlement asset`, `external settlement instruction`, `bridge receipt`. ICN does not use `payment`, `currency`, `balance`, or `wallet` for native primitives. (`currency` may appear only when explicitly discussing external day-currency systems ICN bridges to.)
- Define internal economic primitives at the schema level: obligation, allocation, settlement, unit, position, settlement asset, receipt, challenge.
- Define the external settlement bridge pattern: how ICN authorizes external execution, how the execution is carried out, how the result returns as a bridge receipt, how disputes are handled.
- Define the bridge receipt envelope as a first-class type, parallel to governance proofs and effect records.
- Define the challenge / reversal / appeal path for obligations, allocations, settlements, and bridge executions.
- Support the bridge institution roles credit unions, cooperative lenders, CDFIs, fiscal sponsors, public/community banks (where applicable), accountants, bookkeepers, legal/co-op developers, technical assistance firms.
- Support organizer bodies and technical-assistance bodies as **operating institutions** — not as informal helpers — with their own governance, authority, deliverables, and accountability.
- Keep ICN from implying convertibility between cooperative units and external currency. Bridge receipts attest to external execution; they are not exchange rates.

## Non-goals

- No ICN-native money or currency.
- No 1:1 convertibility promise between cooperative units and any external instrument.
- No payment processor, gateway, or rails design. ICN does not move money.
- No banking compliance implementation (KYC, AML, SOC, BSA, etc.) inside ICN core. Compliance lives where regulation lives — in the bridge institutions.
- No full general-ledger accounting system. Bridge institutions and cooperative accountants own that.
- No runtime implementation in this RFC. ADRs and issues follow.
- No NYCN-specific public website claims. NYCN may dogfood the patterns this RFC explores; the public website continues to use generic "reference institution package" framing.

## Background / current state

ICN's existing decisions about economic coordination:

- [ADR-0004: ICN Base Layer Models Obligations and Attestations, Not Payment Rails](../adr/ADR-0004-icn-base-layer-models-obligations-and-attestations-not-payment-rails.md) — the foundational rejection of payment-rail framing.
- [ADR-0005: Commons Credit Settlement Architecture](../adr/ADR-0005-commons-credit-settlement-architecture.md) — direction on commons-credit settlement.
- [ADR-0007: Commons Resource Policy — CCL Formula Extraction](../adr/ADR-0007-commons-resource-policy-ccl-formula-extraction.md) — CCL bridge for resource-policy formulas.
- [ADR-0013: Federation Clearing Adoption Contract](../adr/ADR-0013-federation-clearing-adoption-contract.md) — dual-path clearing model with store isolation now proven by `crates/icn-federation/tests/store_isolation_clearing_adoption.rs` (PR #1650).
- [ADR-0026: Receipt and Provenance Proof Envelope](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) — the layered envelope this RFC's bridge receipts plug into.
- [ADR-0031: Commons Compute Admission and Settlement Policy](../adr/ADR-0031-commons-compute-admission-and-settlement-policy.md) — back-fill of the settlement engine producing balanced earn/spend journal entries.

Open issues this RFC informs:

- [#1634](https://github.com/InterCooperative-Network/icn/issues/1634) — RFC obligation/allocation/settlement primitives.
- [#1635](https://github.com/InterCooperative-Network/icn/issues/1635) — support-institution package pattern for federations.

What does *not* exist today:

- A canonical schema for `Obligation`, `Allocation`, `Settlement` as first-class types with an explicit lifecycle. The settlement engine produces journal entries; it does not name the upstream commitments.
- Any envelope for external settlement instruction or bridge receipt. ICN cannot today record "the gateway-side accountant paid the venue deposit; here is the proof."
- A written model for organizer bodies and support institutions as governed operating bodies with mandate, scope, and accountability.

## Bridge flow

The bridge model is a single flow with explicit boundaries:

```text
need / request
    ↓
governance authorization                 (ICN: proposal → mandate → grant)
    ↓
obligation or allocation                 (ICN-native record; no day-currency assumption)
    ↓
settlement instruction                   (ICN-native envelope; addresses an external bridge)
    ↓
external bridge partner execution        (Bridge institution: credit union, CDFI, fiscal
                                          sponsor, accountant, lender, etc., under its own
                                          regulatory authority)
    ↓
bridge receipt returned                  (Bridge institution attests to what it did,
                                          when, in what external system, with what reference)
    ↓
ICN records bridge receipt               (ICN-native record; links back to the obligation,
                                          allocation, settlement instruction, mandate)
    ↓
challenge / reversal / appeal path       (per ADR-0029 / RFC-0002 territory; produces
                                          counter-receipts, not deletions)
```

ICN owns every step on the left of the partner boundary. The bridge institution owns every step on its own side. The boundary is the settlement instruction (going out) and the bridge receipt (coming back).

## Design options

### Option A — Internal-only economic records

ICN records obligations, allocations, settlements, and positions internally. External execution is invisible to ICN: members and institutions handle day-currency operations entirely outside.

**Pros**: simplest core, smallest surface, lowest regulatory exposure.
**Cons**: not enough for real institutions. The cooperative still has to pay rent; the substrate provides no honest record of how that happened or under whose authority. Audit trail is broken at the boundary. Members who challenge an external action have no anchor on the ICN side.

### Option B — External bridge receipts (recommended direction)

ICN records internal authorization (proposal → mandate → obligation/allocation → settlement instruction) and external execution receipts (bridge receipt returned by the partner). The settlement instruction and bridge receipt are first-class envelopes. ICN does not execute; it records.

**Pros**: practical transition path. The cooperative can use ICN for governance and provenance while still operating in the day-currency economy through trusted partners. Clean boundary: ICN never holds funds, never moves currency. Bridge receipts are auditable and challengeable. Supports the realistic landscape of credit unions, cooperative lenders, CDFIs, fiscal sponsors, accountants, lenders, and technical-assistance bodies.
**Cons**: requires bridge partner trust and a receipt standard. Disputes about whether the bridge partner actually executed correctly route through partner relationships and external accounting. Onboarding a new bridge partner is a real institutional act, not a config flip.

### Option C — Deep regulated integration

ICN integrates directly with banking, payment, and lending APIs. The substrate orchestrates external execution.

**Pros**: more automation; less manual partner coordination.
**Cons**: ICN inherits the regulatory burden of every system it integrates with. Bank-grade compliance becomes a precondition for any deployment. Capture risk is high — once ICN depends on a payment processor's API, that processor can shape the substrate. The cooperative model is at risk of being absorbed into the regulated-finance model. **Should be deferred. This RFC does not recommend Option C.**

## Tradeoffs

| Concern | Option A (internal-only) | Option B (bridge receipts) | Option C (deep integration) |
|---|---|---|---|
| ICN regulatory exposure | none | none — execution is the partner's | high — ICN becomes a regulated party |
| Real-world utility today | limited; institutions still operate dual-track | strong — institutions can use ICN for governance + provenance while still surviving | strong if compliance is achievable |
| Audit completeness | broken at the external boundary | complete — bridge receipts close the loop | complete; but audit is tied to external system audits |
| Capture risk | low | low — partners are pluggable | high — integration partners can shape the substrate |
| Conceptual integrity | preserved | preserved — ICN never claims convertibility | at risk — substrate starts thinking in payment-rail terms |
| Onboarding cost | trivial | moderate — bridge partner relationships are institutional | high — compliance for every integrated system |
| Failure mode if a partner exits | no impact (no integration) | obligations remain; settlement instructions stop until a new partner | substrate-level outage |
| Support for credit unions / CDFIs / fiscal sponsors | none | first-class | possible but secondary to mainstream rails |

The bridge-receipt path keeps ICN small, keeps the cooperative model intact, and meets institutions where they actually operate.

## Core/package boundary

### ICN core owns

- Generic **obligation**, **allocation**, **settlement**, **position**, and **unit** schemas with explicit lifecycles.
- The **settlement instruction envelope**: an ICN-native record naming what is being asked of an external bridge institution, under whose authority, against which obligation or allocation, with what expected reference data.
- The **bridge receipt envelope**: a record of what a bridge institution attests to having done, with provenance fields plugging into [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md).
- The **challenge / reversal / appeal lifecycle** for obligations, allocations, settlements, and bridge executions.
- The **authority / mandate linkage**: every settlement instruction must be covered by a mandate per [ADR-0014](../adr/ADR-0014-constitutional-object-model.md) and [ADR-0019](../adr/ADR-0019-authority-grant-minting-and-mandate-persistence-seam.md).
- The **proof / receipt linkage** to governance proof envelopes ([ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md)).

### Institution packages own

- Local **categories** of obligation and allocation (e.g., "venue deposit", "childcare support", "translation honorarium") — opaque to ICN core, expressed as `operating_purpose` strings or schema extensions per ADR-0024 candidate.
- **Program budgets** and approval thresholds.
- **Support-institution definitions** — which credit union, which fiscal sponsor, which accountant the package recognizes as bridge partners.
- **Organizer body roles** and operating-body definitions.
- **Local approval thresholds** for various obligation/allocation classes.
- **Bridge partner listings** with public attribution; private overlays (per ADR-0020) hold the actual contact and authorization data.
- **Plain-language descriptions** of what each obligation, allocation, or settlement instruction means to a member.

### External bridge institutions own

- **Actual day-currency execution** through their existing rails.
- **Lending decisions** and underwriting under their own authority.
- **Regulated custody** of any funds in motion.
- **Legal / accounting duties** under their jurisdiction.
- **External compliance** — KYC, AML, BSA, SOC, or whatever applies to that institution.

The meaning firewall holds: ICN never knows how a credit union processes an ACH; the credit union never knows what a `Mandate.authority_class` is. The bridge receipt is the only thing they share.

## Organizer / operating body model

Organizer bodies are not informal helpers. They are governed operating bodies that can receive delegated authority for programs, events, projects, or other bounded efforts. Treating them as governed institutions — rather than as "the volunteer crew" — is what lets the cooperative substrate hold them accountable and protect them.

An organizer body needs:

- **Scope** — what it is authorized to do, in what entity, for what duration.
- **Mandate** — a covering authority grant (per [ADR-0014](../adr/ADR-0014-constitutional-object-model.md), [ADR-0019](../adr/ADR-0019-authority-grant-minting-and-mandate-persistence-seam.md)) tying its actions back to a governing decision.
- **Budget / allocation authority** — explicit ceilings on what obligations and allocations it may originate, and which classes it may issue settlement instructions for.
- **Roles** — internal structure (coordinator, treasurer, facilitator, accessibility steward, etc.) with their own bounded authority.
- **Deliverables / milestones** — what it agreed to produce, by when.
- **Receipts** — for every settlement instruction it originates, the matching bridge receipt or counter-receipt, attached to the mandate.
- **Support-institution relationships** — which fiscal sponsor, which credit union, which accountant it works through.
- **Conflict path** — per [ADR-0029](../adr/ADR-0029-conflict-resolution-object-model.md) and the future RFC-0002, with explicit treatment of organizer overreach as a Shape-1 effect challenge.
- **Post-work accountability report** — when scope ends, what was done, what was spent, what remains, what the next body inherits.

Generic terms used in ICN core: `organizer body`, `operating body`, `program team`, `event lifecycle`, `program lifecycle`. NYCN-specific nouns (e.g., specific committee names) stay in NYCN's package.

## Support institutions and technical assistance

Support institutions are bodies that provide cooperative-relevant services. They are not vendors in the SaaS / marketplace sense; they are institutional actors with service scopes, authority boundaries, receipts, and dispute paths.

Examples of support-institution service scopes:

- **Co-op conversion support** — guiding a worker-owned conversion through legal, governance, and financial steps.
- **Legal / accounting setup** — entity formation, charter drafting, tax registration, ongoing books.
- **Facilitation** — meeting design, deliberation support, decision-process stewardship.
- **Training** — governance training, accessibility training, financial literacy for member-owners.
- **Governance support** — committee design, charter amendment, conflict-resolution-process design.
- **Accessibility services** — interpretation, captioning, document accessibility, deadline-justice planning.
- **Mediation / conflict support** — per [ADR-0029](../adr/ADR-0029-conflict-resolution-object-model.md).
- **Grant writing** — proposal development, reporting, post-award compliance.
- **Design / communications** — brand, public materials, member-facing surfaces.
- **IT / security** — node operation, key custody, infrastructure stewardship.
- **Lending / loan packaging support** — preparing applications to credit unions, CDFIs, cooperative lenders.

Support institutions in ICN have:

- A declared **service scope** (what they do, what they don't).
- A bounded **authority** to act on behalf of the engaging institution (settlement instructions they may originate, signatures they may collect, decisions they may not unilaterally make).
- A **service agreement** (a charter-side document, not an ICN core type) recording the engagement.
- **Receipts** for the work produced.
- A **dispute path** when service quality, scope, or accountability is contested.

This RFC does not draft the service-agreement schema; that is RFC-0014 candidate territory.

## Conflict / dispute path

Disputes about economic coordination route through the conflict resolution object model ([ADR-0029](../adr/ADR-0029-conflict-resolution-object-model.md)) once that model is implemented. Until then, the lifecycle here records what kinds of disputes are possible and how each maps:

| Dispute | Shape (per ADR-0029) | Resolution direction |
|---|---|---|
| Obligation challenge — "I never agreed to this" | Effect challenge | Counter-receipt voiding the obligation |
| Allocation challenge — "this allocation exceeded authority" | Effect challenge | Counter-receipt reversing the allocation; possible authority-grant review |
| External execution mismatch — "the bridge institution did not do what we asked" | Effect challenge | Counter-receipt; if systemic, Shape-2 process appeal against the bridge relationship |
| Bridge partner failure (insolvency, refusal, regulatory action) | Process appeal | Governance review of the bridge relationship; contingency plan; replace partner |
| Unauthorized settlement instruction — "no covering mandate" | Process appeal | Reversal; mandate audit; possible authority-grant revocation |
| Organizer overreach — body acted beyond scope | Effect challenge + relational conflict | Counter-receipt for the unauthorized acts; relational conflict process for the body |
| Support-institution service dispute | Relational conflict | Mediation per ADR-0029 Shape 3; service-agreement adjustment or termination |

Reversal is always a counter-receipt — never a deletion. The institutional memory holds that a thing happened, was challenged, and was reversed. This is the audit-grade choice (per [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md)).

## Accessibility implications

The economic surface is the most consequential member-facing surface, and the most likely to be opaque if not designed for accessibility from the start. Per [ADR-0028](../adr/ADR-0028-accessibility-baseline-for-member-interfaces.md) and the future RFC-0003:

- **Plain-language explanations** of obligations and settlement instructions. A member must be able to read "you owe a venue deposit of N units, authorized by the Tuesday meeting decision, executing through Credit Union X by Friday" without parsing legal/financial vocabulary.
- **Member-readable receipts**. Bridge receipts are not just for auditors. They render to a member with the transaction's plain-language summary, the authority chain, and the next step.
- **Low-bandwidth views** for members on metered or limited connections.
- **Language / glossary support** — the glossary endpoint (#1610) resolves "obligation", "allocation", "settlement instruction", "bridge receipt" into the member's chosen language and reading level.
- **Deadline justice** — settlement-instruction deadlines accommodate caregiving, shift-work, and timezone constraints. A 24-hour deadline is a structural exclusion of part of the membership.
- **No command-line requirement**. Member-facing economic surfaces are non-CLI by default.
- **Assistance path** for members who cannot interpret financial or legal documents — a designated support-institution role helps members understand what they are agreeing to before they confirm a settlement instruction.

## Security / privacy implications

- **Authority before settlement.** Every settlement instruction must carry a mandate reference. Unauthorized settlement instructions are rejected at the schema boundary.
- **Bridge receipt verifiability.** The bridge receipt envelope must support cryptographic verification of the bridge institution's signature, per [RFC-0008 candidate](../../ops/coordination/rfc_candidates.yaml).
- **Selective disclosure.** A bridge receipt may reference an external transaction without disclosing the external counterparty's PII. The external system holds the sensitive data; the receipt holds the reference.
- **Real personal and financial data** stays out of the public package and out of ICN core. Real DIDs bind through private overlays per [ADR-0020](../adr/ADR-0020-institutional-bootstrap-activation-and-standing-read-model.md). External account numbers, routing data, and KYC artifacts stay in bridge institutions' systems.
- **Replay protection** for settlement instructions: each carries a unique id; bridge institutions reject duplicates by content-hash.
- **Bridge partner compromise** is a conflict-of-shape Process appeal (ADR-0029 Shape 2). The substrate has no authority to override an external partner; it has authority to terminate the relationship and route around.

## Compute / automation boundary

> **Compute may advise; governance authorizes; mandates execute; receipts prove.**

Compute, under [ADR-0030](../adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md):

- **May**: calculate proposed allocations under a covering policy oracle, simulate budget impacts, summarize obligations for member readability, forecast settlement timing, validate settlement-instruction schema, flag risk patterns (over-budget, missing mandate, deadline slip, unfamiliar bridge partner).
- **May not**: authorize an obligation, authorize an allocation, originate a settlement instruction, execute a bridge call, or forge a bridge receipt. Authority lives in mandates; execution lives in bridge institutions; both are governance acts, not compute acts.

Determinism class is binding: a settlement instruction's amount, recipient, and authority chain are governance-grade and must be deterministic. Compute-produced advisory outputs (e.g., "this is unusual relative to past patterns") are explicitly marked advisory.

## Website / public truth implications

Per [ADR-0032](../adr/ADR-0032-website-truth-boundary.md) and [ADR-0033](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md):

The public website may say:

- ICN is exploring resource-governance and external-settlement-bridge models.
- ICN models obligations and attestations, not payment rails.
- ICN does not claim to be a bank, currency, payment rail, lending platform, or convertibility engine.

The public website may not say:

- That ICN settles funds, issues currency, processes payments, or holds custody.
- That obligations and allocations are equivalent to currency balances.
- That bridge receipts certify convertibility.
- That a specific institution package is a "reference federation" or names a specific institution. Generic framing only.

Maturity-band claims for the economic subsystem must wait for accepted ADRs and implementation evidence (`partial`/`implemented`/`verified`). The current band is `advancing` for governed-social-accounting framing; the bridge layer is `not-yet`.

## Migration / compatibility

- **Existing CCL contracts** that rely on the schema bridge ([ADR-0022](../adr/ADR-0022-ccl-schema-bridge-and-constraint-compilation.md)) continue to work. Obligation/allocation primitives extend the schema; they do not replace existing rule schemas.
- **Existing commons-credit settlement** ([ADR-0031](../adr/ADR-0031-commons-compute-admission-and-settlement-policy.md)) becomes a special case of the broader settlement primitive. Migration is additive; the existing balanced-journal-entry path remains correct.
- **Existing federation clearing** ([ADR-0013](../adr/ADR-0013-federation-clearing-adoption-contract.md)) is a peer of the bridge model: cross-coop clearing is one kind of settlement, external bridge execution is another. Both produce receipts; both fit under [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md).
- **Institution packages** that already declare allocation templates extend them with bridge-partner references when those exist; absence of bridge partners is a valid configuration.
- **No breaking API changes** in this RFC. Future ADRs may add new endpoints; they will be additive.

## Open questions

1. **Settlement instruction envelope shape**: what minimum fields, what optional, how does the envelope reference the external bridge institution unambiguously?
2. **Bridge receipt envelope shape**: what does the bridge institution sign? How is non-repudiation handled if the bridge institution disputes its own attestation later?
3. **Multi-step bridge executions**: when an action requires two bridge institutions in sequence (e.g., fiscal sponsor → credit union), does the substrate model that as one settlement instruction with multiple receipts, or a chain of instructions?
4. **Conversion / loan-packaging support**: how do cooperative lenders' underwriting decisions show up — as a bridge receipt class, as a separate decision-receipt class, or both?
5. **Currency reference**: when a bridge receipt names "$5000 USD", is that an annotation (external denomination) or a typed field? The RFC recommends an annotation only; the typed field would slide ICN into currency-as-primitive territory.
6. **Bridge partner registry governance**: how is a bridge partner added to or removed from a package? Charter amendment? Standing operating-body authority?
7. **Settlement asset taxonomy**: are settlement assets first-class typed objects or string identifiers? RFC recommends typed.
8. **Position composition across bridges**: can a single position aggregate obligations executed via multiple bridges? RFC recommends yes, with origin labeling per receipt.
9. **Failure mode contracts**: what does a bridge-institution-side failure look like to the substrate? Time-out, explicit-rejection, partial-execution — each needs a receipt class.
10. **Conflict path interaction with RFC-0002**: how does the obligation-challenge surface integrate with the conflict resolution object model when that lands?

## Decision criteria

The project moves toward Option B when:

- The settlement-instruction and bridge-receipt envelope shapes are concrete enough to draft as ADRs.
- At least one realistic bridge institution (credit union, CDFI, or fiscal sponsor) has agreed in principle to receive settlement instructions and return receipts in the proposed shape.
- The conflict path (RFC-0002) has progressed to the point where bridge-execution disputes have a written resolution model.
- The accessibility baseline (RFC-0003) has been drafted enough that the bridge-receipt rendering contract can plug into it.

The project moves toward Option A only if Option B's bridge-partner trust model proves untenable in practice. The project moves toward Option C only after explicit governance review acknowledging the regulatory and capture trade-offs.

## Outcome

To be filled in when the RFC moves to `accepted` or `rejected`. Will name the chosen option and link to the follow-up ADRs.

## Follow-up ADRs

If accepted, the RFC produces these ADRs (most are already in [`ops/coordination/adr_candidates.yaml`](../../ops/coordination/adr_candidates.yaml)):

- ADR — **Obligation Lifecycle**: states, transitions, authority requirements.
- ADR — **Allocation Authorization Model**: who may allocate, against which budget, under which mandate.
- ADR — **Settlement and Position Semantics**: how positions accumulate, how settlements zero them, how mutual-credit ceilings interact.
- ADR — **External Settlement Bridge Boundary**: the explicit ICN/bridge-institution boundary contract.
- ADR — **Settlement Instruction Envelope**: the schema for outgoing instructions.
- ADR — **Bridge Receipt Envelope**: the schema for returning receipts.
- ADR — **Challenge and Reversal Path**: counter-receipt lifecycle for obligations, allocations, settlements, bridge executions.
- ADR — **Bridge Institution Role Model**: typology of bridge institutions (credit union, CDFI, fiscal sponsor, etc.) and their authority profiles.
- ADR — **Organizer / Operating Body Authority Model**: the governance shape for operating bodies.
- ADR — **Support Institution Service Agreement Model**: the package-side schema for support engagements.
- ADR — **Cooperative Lending and Conversion Support Model**: the specific case of loan-packaging and conversion bridges.
- ADR — **Regulatory-Safe Economic Vocabulary**: the closed lexicon (use / avoid lists, with rationale).

These map onto the candidate registry rows ADR-0044, 0045, 0046, 0047, 0048, 0041 in [`adr_candidates.yaml`](../../ops/coordination/adr_candidates.yaml), plus several new candidates this RFC introduces (Bridge Institution Role Model, Organizer/Operating Body Authority Model, Support Institution Service Agreement Model, Bridge Receipt Envelope, Settlement Instruction Envelope).

## Follow-up implementation issues

Likely to be opened after the RFC is accepted and ADRs land:

- `schema(economics): obligation/allocation/settlement envelopes`
- `schema(economics): external settlement instruction`
- `schema(economics): bridge receipt`
- `docs(architecture): support institution role model`
- `docs(architecture): organizer/operating body model`
- `test(economics): settlement instruction cannot imply convertibility`
- `test(economics): bridge receipt round-trips through receipt envelope`
- `test(economics): challenge produces counter-receipt without state mutation`
- `feat(governance): mandate-gated settlement instruction origination`
- `website(public): public economic claims lint per ADR-0033`

## Validation / proof plan

Implementation status moves only when these are demonstrated:

- **`partial`**: schemas defined; settlement instruction and bridge receipt envelopes parse; obligations and allocations have lifecycle handlers in `icn-ledger` or a new `icn-economics` crate; integration tests prove envelope round-trips.
- **`implemented`**: at least one realistic bridge-institution flow operates end-to-end (e.g., a fiscal-sponsor stub returns a bridge receipt that ICN records and queries); challenge path produces counter-receipts without mutating originals; vocabulary lint passes (no `payment`/`currency`/`balance`/`wallet` in public surfaces).
- **`verified`**: a deployed institution operates economic coordination through the substrate for a sustained period (≥ one quarter); audit replay reproduces every receipt; at least one challenge case has run to resolution; a cooperative lender has accepted a settlement instruction in production and returned a bridge receipt that closed the loop.

The website maturity band moves from `not-yet` to `advancing` only at `partial`; from `advancing` to `maturing` only at `implemented`; from `maturing` to `strong` only at `verified` with public evidence.

---

## Notes

- For amendments during `draft` or `active`, edit in place and bump `updated`. Once `accepted`, prefer a successor RFC.
- This RFC is upstream of multiple ADRs. The economics tranche in `adr_candidates.yaml` (rows 0041, 0044–0048) should not be drafted until this RFC reaches `accepted`.
- This RFC introduces design space that may produce additional RFC candidates (RFC-0011 ICN project self-governance, RFC-0012 organizer/operating body model, RFC-0013 external bridge institutions, RFC-0014 technical assistance and cooperative conversion support). Those rows are added to [`rfc_candidates.yaml`](../../ops/coordination/rfc_candidates.yaml).
