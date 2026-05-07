---
Status: normative
Canonical: no
Last Reviewed: 2026-05-07
---

# Private-overlay DID activation flow (generic ICN spec)

This document specifies the **generic substrate boundary** for binding **organizer-readable holder labels** to **substrate DIDs** without leaking private overlay material into public ICN repositories.

It defines the **boundary and lifecycle**, not partner-specific overlay contents. Institution packages (cooperative, community, federation, organizing-network) own their local overlay storage, steward roles, sign-off procedures, and retention policy. ICN core docs describe how the boundary works, what may cross it, and what must never cross it.

**Tracking:** [ICN#1730](https://github.com/InterCooperative-Network/icn/issues/1730). Parent: [ICN#1724](https://github.com/InterCooperative-Network/icn/issues/1724). Milestone: [ICN#1746](https://github.com/InterCooperative-Network/icn/issues/1746). Companion partner-package follow-up: NYCN#57.

## 1. Status and non-claims

**Status.** Generic ICN spec. Documentation only. No runtime code is changed by this spec. No DID resolver is introduced or redesigned. No directory service is shipped. No partner overlay is implemented in this repository.

**This document is NOT:**

- a partner overlay implementation;
- a directory service;
- a DID resolver redesign;
- a production launch flow;
- formal NYCN pilot authorization;
- a live federation flow;
- a place where private partner, member, sponsor, attendee, accommodation, or contact data is stored.

**Vocabulary.** Substrate-facing copy uses **obligation**, **allocation**, **settlement**, **unit**, **position**, **receipt**, **provenance**, and **evidence** when referring to ICN primitives. ICN-native economic primitives should not be described using consumer-finance or fintech framing.

## 2. Definitions

### 2.1 Holder label

A **holder label** is the organizer-facing, human-readable placeholder used in repo-safe workflows.

- May be a **role** (for example `facilitator`, `treasurer-role`), a **placeholder** (for example `holder-A`, `committee-chair-2026`), or a **fixture-local reference** used for rehearsal.
- Lives in public/repo-safe organizer material: wireframes, fixtures, demo scripts, preview/review summaries, action-card examples.
- Is **not** a DID and is **not** proof of identity. It does not assert that any specific person controls any specific key.
- Is rebindable: a label can be reassigned, retired, or replaced without rewriting public history.

### 2.2 DID

A **DID** is the substrate identity reference used by the ICN runtime after a consented activation.

- Format: `did:icn:<base58-pubkey>` (Ed25519), per `icn-identity` (see [`../architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md`](../architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md)).
- Appears in public/repo-safe artifacts only when the DID is **explicitly fictional**, **fixture-safe**, or **already authorized** by the relevant package policy for public reference.
- Carries **no participant attributes** by itself: a DID is a substrate handle, not a profile, contact card, or attendance record.

### 2.3 Private overlay

A **private overlay** is the non-public mapping/context that binds holder labels to real DIDs and to real-person records.

- May include private organizer/member/sponsor/attendee data (legal names, contact details, accommodation notes, attendance rolls, sponsor/member private fields, internal identifiers).
- **Must not be committed to public ICN docs or repositories.** Overlay storage location, retention, and access are package-local concerns.
- May include the cryptographic material or recovery context that an institution package treats as private (for example, a rotation history, an internal sign-off log).
- Tokens, secrets, and credentials are private overlay material and never appear in public artifacts.

### 2.4 Activation

**Activation** is the consented transition from a label-only workflow to a DID-bound workflow.

- Performed by an authorized steward/operator, or by a governed process explicitly defined in the institution package.
- Must be **reviewable** (an authorized reviewer can confirm the activation occurred and was authorized), **reversible/rotatable where appropriate** (a binding can be retired, rotated, or replaced), and **auditable without leaking private overlay contents** (public evidence shows status and authority basis, not mapping contents).
- Activation does **not** publish a person's identity. It records, in the appropriate authority context, that a label has become a DID-bound participant for a specific scope.

## 3. Public/private split

The boundary between public ICN material and private overlay material is enforced at the doc/PR level today and at the evidence-export level by the substrate. Institution packages must respect both directions of this split.

### 3.1 Public/repo-safe (allowed in this repository and in repo-safe partner artifacts)

- **Holder labels** (roles, placeholders, fixture-local references).
- **Fixture-only DID examples** that are obviously non-live (clearly synthetic patterns; not the DID of any real participant).
- **Schema/contract references** (for example `action-card.schema.json`, `preview-review.schema.json`, `rehearsal-evidence-export.schema.json`).
- **Policy boundaries** (this document, the no-CLI workflow doc, the accessibility gate, package boundary briefs).
- **Redacted evidence summaries** (basenames, categories, mode flags, outcome categories).
- **Activation status proofs** that record _that_ activation happened (event status, authority basis, timestamp/hash/redacted pointer) without exposing the private overlay contents.

### 3.2 Private/overlay-only (must never enter public ICN material)

- Real **person ↔ DID mapping** (the binding contents themselves).
- **Contact details** (email, phone, address, social handles tied to a real participant).
- **Attendance rolls** that name real participants.
- **Access needs / accommodation notes** for real participants.
- **Sponsor/member private data** (private agreements, internal-only fields, private financial context).
- **Tokens, secrets, credentials** of any participant or steward.
- **Partner-specific overlay records** in their full form (raw mapping rows, private review notes, private sign-off logs).

A reviewer who finds any item from §3.2 in this repository or in a repo-safe artifact must treat the PR as not-ready and route the material into the relevant package-local overlay before merge. There is no version of "leak it once and rotate later" that is acceptable for §3.2 categories.

## 4. Activation lifecycle (generic flow)

The activation lifecycle is the canonical sequence institution packages must follow to take an organizer-facing label and bind it to a substrate DID. Implementations may batch or reorder UI screens where safe, but every step's intent must be preserved.

1. **Label appears in organizer-facing material.** The label is created in the package's normal organizer workflow (rehearsal fixture, governance roster, action-item assignment) and is treated as a placeholder. No DID is implied.
2. **Steward/facilitator identifies whether activation is needed.** Many labels (committee roles, fixture references, future placeholders) never need activation. Activation is requested only when a substrate-bound action is required for that label (signing a receipt, casting a substrate-recognized vote, completing an action item under a DID-bound flow).
3. **Person or authorized representative consents to DID binding.** The participant whose label is being activated, or an explicitly authorized representative under package policy, gives **informed consent** to the binding. Consent records what scope the binding covers and what the participant should expect to see in public artifacts after activation.
4. **Steward reviews the mapping in a private overlay.** The mapping (label → DID → person record) is recorded in the package-local private overlay only. The review confirms that the DID is the participant's intended substrate handle, that the package is authorized to perform the binding, and that nothing private has leaked into a public surface.
5. **Activation creates or confirms the DID binding.** The substrate-side effect (registering the binding for the relevant scope, updating the standing/membership view, issuing a capability if applicable) is performed by an authorized steward/operator or governed process.
6. **Public artifact records only redacted activation status.** A repo-safe receipt or evidence summary records that activation occurred and under what authority basis (steward role, committee decision, governed process), with timestamp and a redacted pointer (for example a hash of the private overlay row, never the row itself). The mapping contents stay in the private overlay.
7. **Later revocation/rotation can replace or remove the binding.** Subsequent rotations or revocations are recorded the same way: status + authority basis + redacted pointer in public material; mapping changes recorded only in the private overlay.
8. **Evidence export records boundary-safe status only.** Any rehearsal or pilot evidence export that references activations must use the redacted activation-status form, not the raw overlay contents. The repo-safe evidence export contract ([`../contracts/rehearsal-evidence-export.md`](../contracts/rehearsal-evidence-export.md)) governs this.

A package may add **package-local steps** (for example multi-steward sign-off, time-windowed consent, recorded review meetings) on top of this generic flow. A package may **not** remove a step from the generic flow.

## 5. Consent and authority boundary

Consent and authority rules apply to every activation, regardless of which package performs it.

- **No unconsented binding.** An activation never proceeds without informed consent from the participant or an explicitly authorized representative under package policy.
- **No public doxxing-by-activation.** The act of activating must not publish a participant's legal identity, contact info, or accommodation context. Public artifacts show status and authority, not mapping contents.
- **A public name does not authorize a DID binding.** Mentioning a person in a public agenda, roster, or news item is not consent and does not authorize an activation. Authority and consent are distinct from public visibility.
- **No binding from scraped documents alone.** Drive ingest, scraped agendas, mailing-list rosters, and similar inputs may surface a label and may suggest a future activation candidate. They are never sufficient by themselves to perform an activation.
- **Role/committee labels remain labels until activation is explicitly needed.** A committee chair role can stay a label across many cycles; it becomes DID-bound only when a substrate-bound action requires it.
- **Activation is performed by an authorized steward/operator or governed process.** Authority to activate is package-defined. ICN core does not enumerate steward identities; it enumerates the boundary they must respect.
- **Partner-specific procedures live in partner repositories/policies.** Steward roster, multi-sign-off rules, retention windows, and overlay storage location are package concerns. ICN core does not encode any one partner's procedure.

## 6. Revocation, rotation, and correction

Bindings are **mutable in the substrate sense** (rotatable/revocable) and the public-evidence layer must reflect that without rewriting history.

- **DID rotation path.** When a participant rotates their substrate key, the binding is updated in the private overlay; a redacted rotation status entry appears in the public evidence trail. The previous DID is retired in the binding without erasing prior public receipts that referenced it.
- **Mistaken-binding correction path.** When a mapping was recorded against the wrong DID, the package corrects the overlay row, records a redacted correction status in public evidence, and (where applicable) issues an explicit reversal of any substrate-side effect that depended on the wrong binding.
- **Withdrawal/revocation of consent.** A participant (or an authorized representative under package policy) may withdraw consent. The package retires the binding, records a redacted revocation status in public evidence, and rebinds (or relabels) future obligations according to package rules.
- **Stale label cleanup.** Labels that no longer correspond to any active role or participant are retired in the package's organizer surface. Retirement is a label-side action and does not by itself imply revocation of any substrate binding.
- **Evidence shows that a correction occurred without exposing private data.** Public evidence records the kind of change (rotation, correction, revocation, retirement), the authority basis, and a redacted pointer. It does not record the prior or new mapping contents.

## 7. Threat model notes

The following risks must be addressed by every package implementing the activation flow. This list is not exhaustive; packages should add risks specific to their environment (live cloud sync, multi-org overlay, etc.) in their package-local policy.

- **Accidental private data commit.** Private overlay rows committed to a public repo. Mitigation: PR-level review, redaction of evidence summaries, repo-safe export contract enforcement.
- **Unconsented DID binding.** A binding performed without informed participant consent. Mitigation: package-local consent record, steward review step, withdrawal procedure.
- **Public label treated as identity proof.** A label appearing in a public agenda or fixture being assumed to authorize substrate-bound action. Mitigation: explicit "label ≠ DID" statement in product UI and operator docs; activation as the only authorization path.
- **Role label confused for person identity.** A role label (for example `treasurer-role`) being treated as a specific person across cycles after that role changes hands. Mitigation: rebinding flow, public retirement of the label-to-person association, no implicit carryover.
- **Fixture DID accidentally reused as a real DID.** A demo/fixture DID being used as if it were a real participant. Mitigation: obviously synthetic fixture DID patterns; package validation that rejects fixture DIDs in non-rehearsal modes.
- **Evidence packet leaking overlay content.** A rehearsal or pilot evidence export including raw mapping rows. Mitigation: repo-safe evidence export contract; redacted-pointer form; CI check on evidence shape.
- **Live sync accidentally enabled during rehearsal.** A package-local connector pushing private overlay rows to a live cloud sync surface. Mitigation: rehearsal-mode default-off for all live connectors; package-local pre-flight check.
- **Stale overlay mapping after role/person changes.** An overlay row continuing to bind a label to a DID that no longer represents the participant's intent. Mitigation: rotation/revocation procedure; periodic package-local audit.
- **Unauthorized steward/operator activation.** A non-authorized actor performing an activation. Mitigation: package-local authority list; steward identity verification; logged authority basis on every activation event.

## 8. Evidence and receipts

Evidence and receipts must prove **boundary compliance**, not reveal **private mapping**. The substrate's evidence-export contract is the canonical surface for this; this section describes what activation events should and should not contribute.

- **Public evidence proves boundary compliance.** A repo-safe evidence summary may state that an activation, rotation, revocation, or correction occurred, with authority basis, timestamp, and a redacted pointer. It must not include legal names, contact info, accommodation notes, attendance rolls, or any other private overlay content.
- **Receipts/provenance reference activation event status only.** A receipt that records an activation event records the **status** of the event (activated / rotated / revoked / corrected), the **authority basis** under which it was performed, the **timestamp/hash**, and a **redacted pointer** to the overlay row where applicable. It does not record the mapping contents.
- **Private overlay content is never published in receipts.** A receipt is a public artifact. If a participant's private data appears in a receipt, the receipt is malformed and must be regenerated from the redacted form.
- **Fixture-only examples must use obviously non-live DID patterns.** Sample receipts and example evidence packets must use DIDs that cannot be mistaken for a real participant's substrate handle. Packages should provide synthetic patterns that are recognizable as fixtures.

## 9. Package responsibility

ICN core defines the generic boundary. Institution packages own the local concerns that make the boundary operable in their environment.

**ICN core defines:**

- the public/private split;
- the activation lifecycle;
- the consent and authority requirements;
- the revocation, rotation, and correction expectations;
- the threat-model surface;
- the evidence and receipt boundary.

**Institution packages define:**

- local overlay storage location and access controls;
- steward roles and sign-off procedure;
- retention policy;
- package-local consent UX;
- package-local authority list;
- live-sync defaults for any package connectors;
- package-local additions to the threat model in §7.

**ICN core docs do not name partner-specific overlay contents.** A partner's steward roster, mapping table, retention window, and live-sync provider are partner concerns. ICN docs reference the boundary, not the contents.

**Downstream package-specific follow-up:** NYCN#57 is the NYCN organizing-network package's holder-label → DID activation / private-overlay policy. It must reference this spec, define the seven package-local concerns named above, and keep all private partner data out of the public NYCN repository. NYCN#57 is not started in this PR.

## 10. Cross-links

- [`../pilots/no-cli-organizer-member-rehearsal-workflow.md`](../pilots/no-cli-organizer-member-rehearsal-workflow.md) — the no-CLI organizer/member workflow this spec serves; §3 step 5 and §4 "No private partner data in ICN" are the points where this boundary is invoked.
- [`../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) — accessibility gate for organizer/member shells; relevant when the activation flow surfaces in a UI that must support privacy-preserving accommodation paths.
- [`../architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md`](../architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md) — substrate identity primitives that the DID side of activation references.
- [`../contracts/rehearsal-evidence-export.md`](../contracts/rehearsal-evidence-export.md) — repo-safe evidence export contract; the canonical shape for redacted activation-status entries.
- NYCN#57 — downstream NYCN-specific holder-label → DID activation / private-overlay policy (companion package-side policy; not authored in ICN core).
