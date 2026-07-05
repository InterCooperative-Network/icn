# Made-Available, Federation Sync, Access, and Repair Boundary Map

**Status:** draft - design/control map
**Truth class:** descriptive
**Canonical:** no - implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last reviewed:** 2026-07-05
**Source basis:** read against `origin/main` at `8a64eec26ab83398e10d1f9ceba89e99fa766e2c`
**Related:** #2336, #2335, #2334, #2333, #2330, #1792, #1748, #2141, #2041, #1868, #2061, #2080, #2081

> This document maps the control boundaries after `EvidencePacketMadeAvailableReceipt`. It is a design map for ICN as Digital Public Infrastructure, a Coordination OS, and democratic institutional infrastructure. It adds no runtime behavior, receipt class, route, authorization rule, vault, federation path, user interface, or fixture. It does not claim that a packet was accessed, delivered, received, accepted, audited, certified, or legally sufficient. Receipts record bounded institutional facts. Receipts grant zero authority.

## 1. Why this map exists
<!-- truth: descriptive -->

`EvidencePacketMadeAvailableReceipt` is now a landed runtime class and has a fixture-only member-shell render. That closes one narrow sender/custodian-side evidence boundary. It does not establish the mechanisms that come after availability.

The next implementation could overreach in several ways:

- treating a made-available fact as proof that a recipient accessed an artifact;
- treating receipt or artifact propagation between peers as agreement;
- treating a routing acknowledgement as delivery to a recipient institution or person;
- treating an artifact registry reference as disclosure of private contents;
- treating a repair action as authority to read or copy restricted material;
- treating trust evidence as an authority grant.

This map prevents those substitutions. It connects the landed receipt to the existing federation proof-loop and storage designs, identifies what remains design-only, and names the authority work that must precede private-access runtime.

## 2. Current landed facts
<!-- truth: descriptive -->

The landed process/evidence sequence is:

```text
ProcessGateResultReceipt
ProcessSessionOpenedReceipt
DeliberationEntryRecordedReceipt
DecisionRecordedReceipt
ActivationCrossedReceipt
MutationPlanRecordedReceipt
MutationAppliedReceipt
EvidencePacketProducedReceipt
EvidencePacketExportPreparedReceipt
EvidencePacketMadeAvailableReceipt
```

The last three facts are deliberately narrow:

1. `EvidencePacketProducedReceipt` records that a public/redacted packet artifact was produced and fingerprinted from referenced process evidence.
2. `EvidencePacketExportPreparedReceipt` records that an export of that packet was prepared for an opaque recipient scope under a fingerprinted export policy.
3. `EvidencePacketMadeAvailableReceipt` records that the prepared export was made available to that scope under a fingerprinted disclosure policy and availability method.

The tenth class landed in #2333 with tag `icn:gov:evidence_packet_made_available:v1`. Its fixture-only member-shell render landed in #2335. The render states the negative boundary in plain language and introduces no new receipt class.

The following are not landed as runtime behavior in this lane:

- no `AccessReceipt`, `DisclosureDecisionReceipt`, or `RedactionAppliedReceipt`;
- no artifact-registry or scoped-vault persistence and enforcement path;
- no private-object retrieval path;
- no live anti-entropy emission, classification, or repair path for these receipts;
- no recipient-side delivery, receipt, or acceptance contract;
- no authority decision that follows from any receipt.

The anti-entropy identifiers named in [`network-anti-entropy-proof-loops.md`](../spec/network-anti-entropy-proof-loops.md) are wire-stable, but the spec explicitly says that live construction, gossip, classification, and repair are not wired end to end. The artifact registry and scoped vault in [`artifact-registry-and-scoped-vault.md`](../spec/artifact-registry-and-scoped-vault.md) remain design-level objects.

## 3. Boundary vocabulary
<!-- truth: descriptive -->

These terms are related but not interchangeable.

| Term | Bounded meaning | Does not establish |
|---|---|---|
| **Made available** | A sender/custodian-side fact that a prepared export was placed under a recorded disclosure policy for an opaque recipient scope. | Retrieval, access, delivery, recipient receipt, acceptance, or authority. |
| **Federation sync** | Bounded state or digest exchange between peers under a named scope and freshness window. | Agreement with the state, institutional acceptance, delivery to a person, or broader disclosure. |
| **Routing proof** | Evidence that a routed message or receipt reached acknowledging network peers within its freshness contract. | Application-level delivery, recipient access, human notice, or acceptance. |
| **Receipt digest propagation** | Propagation or comparison of bounded receipt-index fingerprints. | Propagation of receipt bodies, packet bodies, policy bodies, or recipient acknowledgement. |
| **Peer observation** | A peer observed a digest, message, or state comparison and produced evidence of that observation. | Institutional endorsement, agreement, or authority to act. |
| **Recipient access** | An authenticated actor scope attempted or performed a restricted-object read under policy and a cited authority basis, with an outcome. | Delivery, recipient acceptance, or proof that the cited authority basis was valid merely because a receipt records it. |
| **Delivery** | A future sender or transport claim that an artifact was transmitted to a target under a defined delivery contract. | Recipient receipt, opening, acceptance, or agreement. |
| **Recipient receipt** | A future recipient-side fact that the recipient obtained the artifact. | Acceptance, audit, certification, or agreement with contents. |
| **Acceptance** | A recipient-side institutional decision under recipient criteria and authority. | Audit, certification, settlement finality, or legal sufficiency. |
| **Disclosure decision** | A future decision that a disclosure request was approved, denied, limited, deferred, withdrawn, or superseded. | That access or disclosure actually occurred. |
| **Redaction applied** | A future fact that a redaction transformation was recorded from a source reference to a redacted artifact fingerprint. | Correctness, completeness, accessibility, legal compliance, or acceptance. |
| **Repair** | A bounded attempt to reconcile missing or divergent state under a repair plan and authority basis. | Authority to access private contents, permission to broaden locality, or proof that all peers now agree. |
| **Settlement finality** | A domain-specific condition that all required finality gates for a settlement have been met. | General federation agreement or finality for unrelated receipt and artifact classes. |
| **Authority basis** | The mandate, policy clause, capability, membership, or delegation cited for an act and evaluated by the authority layer. | Authority merely because it is referenced, trusted, synchronized, or written into a receipt. |

Trust can inform a policy oracle. It does not grant authority. The Meaning Firewall requires apps to translate semantic trust and governance decisions into bounded constraints while the kernel enforces those constraints without interpreting their institutional meaning.

## 4. The boundary chain
<!-- truth: descriptive -->

The control chain is:

```text
prepared
  -> made available
  -> federation observed or synced
  -> access attempted
  -> access allowed, denied, expired, revoked,
     not_found_or_not_visible, or policy_mismatch
  -> received
  -> accepted
  -> audited or certified
```

No arrow implies the next step. Each arrow requires its own actor, scope, authority, mechanism, evidence, and negative boundaries.

The load-bearing inequality is:

```text
prepared != made available
made available != accessed
accessed != delivered
delivered != received
received != accepted
accepted != audited
audited != certified
certified != legally sufficient
```

The federation extension is equally strict:

```text
sync != agreement
routing proof != delivery
federation observation != recipient acceptance
trust != authority
```

An implementation must not collapse two rows because the same network exchange, actor, or stored fingerprint happens to participate in both.

## 5. Made-available vs federation sync
<!-- truth: descriptive -->

`EvidencePacketMadeAvailableReceipt` is a sender/custodian availability fact. It records an opaque recipient scope plus disclosure-policy and availability-method fingerprints. It does not say that any federation peer observed the receipt.

Federation sync is a separate proof loop:

1. A peer constructs an `AntiEntropyProbe` for a bounded state class and scope.
2. Peers compare `StateDigest`, `ReceiptDigest`, or `ArtifactDigest` values.
3. A `PeerSyncReport` records matching, missing, divergent, or unknown/out-of-scope comparison results.
4. Non-matching results may produce `DivergenceEvidence`, a governed `RepairPlan`, and a `RepairReceipt` evidence artifact.

A made-available receipt may be one record represented in a `ReceiptDigest`. Its packet or future registry reference may be represented by an `ArtifactDigest`. Only the digest or permitted envelope propagates. The disclosure rules on the underlying body still govern any fetch.

`QuorumSyncCheck` can prove that named federation peers exchanged matching digests for a state class within a `FederationSyncWindow`. It cannot prove that those peers agree with the institutional act, that a recipient accepted it, or that a private artifact was disclosed.

`RoutingProof` proves bounded network acknowledgement for a message or receipt. It is not an application-level delivery receipt. A future delivery contract would need to identify what was delivered, by which transport, to what recipient-side authority, and what acknowledgement counts. Until that contract exists, routing proof must be rendered as routing evidence only.

Peer observation is evidence, not assent. Federation coordination occurs by agreement among domains; gossip cannot create that agreement.

## 6. Made-available vs artifact registry and scoped vault
<!-- truth: descriptive -->

The landed made-available receipt carries fingerprints only:

- `disclosure_policy_hash` identifies the policy body without storing it;
- `availability_method_hash` identifies the method descriptor without exposing it;
- `packet_hash` is an echoed, verified fingerprint of the public/redacted packet;
- `recipient_scope_id` is an opaque scope handle.

It carries no artifact-registry entry, `PrivateObjectRef`, vault identifier, vault path, network address, retrieval credential, contact detail, policy body, method body, or packet body.

The future storage relationship is:

```text
ArtifactRegistry entry
  -> records that an artifact exists and what governs it
  -> uses a content-addressed public blob reference, or
  -> uses PrivateObjectRef for restricted contents

PrivateObjectRef
  -> bridges safe public/scoped metadata to a private object
  -> carries opaque identity, content fingerprint, privacy class,
     policy fingerprint, vault reference, and receipt reference
  -> never carries private contents or retrieval mechanics

ScopedVault
  -> stores and enforces access to restricted objects
  -> applies disclosure and retention policy
  -> emits access evidence for reads
```

The registry records what exists and what governs it. The vault enforces privacy. The receipt store records actions on both. These responsibilities stay separate.

`PrivateObjectRef` is the bridge, not the body. A reference may be digestible and synchronized without synchronizing the private contents. Anti-entropy repair must preserve that separation and must never widen privacy class, data locality, recipient scope, or custody.

## 7. Made-available vs AccessReceipt
<!-- truth: descriptive -->

Made available means that a prepared export was placed under a disclosure policy for a named recipient scope. The policy may describe which recipient scope can retrieve it, but the receipt does not adjudicate whether a particular actor currently holds valid authority.

A future `AccessReceipt` would record a different fact:

> An authenticated actor scope attempted or performed access to a referenced restricted object under a named disclosure policy, cited an authority basis, and received a bounded outcome.

The design-ready minimum includes:

- a `PrivateObjectRef` identifier and fingerprint, not private contents;
- an opaque actor scope;
- a disclosure-policy fingerprint;
- purpose and authority-basis fingerprints;
- an outcome from `allowed`, `denied`, `expired`, `revoked`, `not_found_or_not_visible`, or `policy_mismatch`;
- append-only evidence for successful and refused attempts where safe.

`not_found_or_not_visible` is mandatory to avoid turning the access path or receipt into an existence oracle. A caller must not be able to distinguish a missing hidden object from an object they are not allowed to know exists.

### Runtime readiness decision

**`AccessReceipt` is design-ready only. Its runtime is blocked.**

The following are partially unblocked at the design level:

- the bounded fact and negative claims;
- the privacy-preserving object-reference pattern;
- the outcome vocabulary, including enumeration-safe failure;
- the rule that every access must cite authority and emit evidence;
- the rule that a receipt records a cited basis but grants no authority.

Runtime remains blocked because:

- #1868 has not finished decomposing broad mutation capability into per-action authority;
- #2061 has not completed the entity-aware subject, target, membership, hierarchy, and delegation model across request authorization;
- #2080 has not landed the trusted positive issuance path for entity-scoped authority;
- #2081 has not completed enforcement cutover even for the treasury reference path;
- the artifact registry, `PrivateObjectRef`, and scoped-vault enforcement path are not runtime-real;
- there is no approved contract for visibility and retention of denied access evidence.

An `AccessReceipt` implementation before those seams are clear could faithfully record an access attempt while still relying on the wrong authority gate. That would create strong-looking evidence around weak authority. The receipt must follow the authority decision, not substitute for it.

## 8. Federation proof-loop relationship
<!-- truth: descriptive -->

The anti-entropy proof rail includes:

| Artifact | Role in this boundary |
|---|---|
| `ReceiptDigest` | Bounded digest of receipt-index state. May include a made-available receipt hash, never its private or policy bodies. |
| `ArtifactDigest` | Bounded digest of registry metadata or a scoped-vault reference, never the artifact body. |
| `PeerSyncReport` | Signed comparison result for peer state. Observation only. |
| `DivergenceEvidence` | Classified evidence of missing or divergent scoped state. |
| `RepairPlan` | Governed plan naming action, scope, authority basis, and boundary rules. |
| `RepairReceipt` | Evidence artifact recording repair attempt and outcome with before/after digest linkage. |
| `SyncDegradedStatus` | Honest status for unresolved missing or divergent state within or beyond a grace window. |
| `QuorumSyncCheck` | Fresh proof that a quorum exchanged matching digests for a state class. |
| `RoutingProof` | Bounded acknowledgement that a message or receipt reached named peers. |
| `RedundancyProof` | Evidence that observed replica count meets or misses policy. |
| `FederationSyncWindow` | Per-state-class freshness policy for quorum sync evidence. |

These are proof/evidence artifacts or envelopes where the current spec says so. They are not new process/evidence receipt classes. `RepairReceipt`, despite its name, travels inside an existing evidence envelope and does not extend the ten-class process ladder.

Anti-entropy can prove that a made-available receipt or safe artifact reference is missing, stale, or divergent at a peer. It can plan and record a bounded repair. It cannot authorize access to the private object. A repair executor may copy only material that the plan, authority basis, privacy class, locality, and disclosure policy permit. If the body is out of scope, the correct repair may synchronize only the digest/reference, quarantine state, or escalate for governance review.

Repair outcome is not convergence forever. A later probe must verify the after-state within a fresh window. Matching state is not agreement with the underlying institutional act.

## 9. Authority blockers and why they matter
<!-- truth: descriptive -->

| Issue | Open boundary | What it blocks here |
|---|---|---|
| **#1868** | `governance:write` still covers many distinct actions; the design must choose per-action capability scopes, mandate-bundle gating, or a hybrid. | A precise capability for requesting, approving, performing, challenging, or repairing private access. Trust or a broad bearer scope cannot stand in for this decomposition. |
| **#2061** | Flat namespace equality and entity-aware membership/hierarchy authorization are still converging. | A reliable interpretation of actor scope, target object scope, federation/community delegation, and same-entity vs delegated access. |
| **#2080** | Trusted production issuance from verified membership, invitation, session, or enrollment state is not landed. | A trustworthy token claim that binds the authenticated actor to entity scope and authority. |
| **#2081** | Entity-aware treasury authorization remains in observe-first cutover work. | Evidence that the entity-aware gate can become authoritative without legitimate-access regressions. It is a reference migration, not an AccessReceipt implementation. |
| **#2041** | Human screen-reader, assistive-input, and zoom/contrast pass remains owed. | Any claim that a future privacy/access rendering is organizer-ready or member-ready. It does not by itself block a headless design document, but every new human surface extends the owed pass. |
| **#1748** | The Institutional Process Substrate control issue still owes real privacy/redaction and accessibility-gate evidence. | Any claim that this map completes the process substrate or its acceptance gates. |
| **#2141** | The vertical institutional spine remains a coordinating control issue, including custody enforcement and federation outcomes. | Any claim that one receipt or proof loop completes package-to-evidence operation. |

`#1907` remains a protected review/control item unrelated to this implementation sequence. This map does not change or close it.

Trust remains an input to policy, not an authority source. The kernel can enforce a typed constraint or capability blindly. It must not derive authority by interpreting a trust score, federation relationship, sync result, or receipt.

## 10. Cross-repo implications
<!-- truth: descriptive -->

The repository order remains part of the control boundary:

- **icn** defines the public generic primitives, proof artifacts, receipts, and this control map.
- **nycn** may later consume landed primitives through institution-package mappings and local policy. It must not define ICN receipt, access, or federation semantics.
- **icn-infra** may later carry bounded operator contracts and promotion gates. It is not a vault, secret store, or proof that runtime behavior exists.
- **icn-community-bridge** may later need recipient-side mirroring or delivery boundaries. It is not governance, authority, or canonical record.
- **icn-learn** may teach only after canonical truth lands in icn and any institution-specific truth lands in its source package.
- **.github** may mirror public truth only after icn establishes it.
- **demo-repository** has no assigned role in this lane.

No downstream repository changes are part of this map. Cross-repo work follows the order `icn` first, private consumers second, teaching third, and public mirror last.

## 11. Recommended next sequence
<!-- truth: descriptive -->

1. Review and close the PR for this map without treating the map as implementation.
2. **Take authority design/hardening first:** resolve the action-level authority shape across #1868 and #2061, including the representation an access decision can cite without the receipt adjudicating it.
3. Advance the trusted positive issuance and entity-enforcement prerequisites in #2080 and #2081 using their own scoped work.
4. Open a docs-only `AccessReceipt` decision rung after the authority representation is clear. Pin actor-scope representation, target/object references, outcome visibility, retention, existence-oracle behavior, predecessor links, and exact nonclaims.
5. Run a small anti-entropy fixture/proof-loop slice that demonstrates made-available receipt-digest propagation, missing/divergent evidence, and bounded repair without accessing packet contents.
6. Open a scoped-vault/`PrivateObjectRef` design rung that reconciles privacy-class naming and pins the registry-to-vault reference contract.
7. Implement registry/vault authorization and receipted read enforcement only after those contracts are accepted.
8. Revisit `AccessReceipt` runtime last, with authority, object reference, policy enforcement, and evidence visibility all testable together.

**Recommended first next lane:** the #1868/#2061 authority-basis control and hardening work. A docs-only `AccessReceipt` rung can follow immediately after that representation is explicit. Do not start `AccessReceipt` runtime from this map.

## 12. Non-goals
<!-- truth: descriptive -->

- No runtime changes.
- No Rust receipt class.
- No route, OpenAPI, or SDK change.
- No gateway or authorization implementation.
- No vault implementation.
- No encryption implementation.
- No `AccessReceipt` runtime.
- No `DisclosureDecisionReceipt` runtime.
- No `RedactionAppliedReceipt` runtime.
- No operator dashboard.
- No member-shell implementation.
- No fixture changes.
- No NYCN package update.
- No icn-learn update.
- No icn-infra update.
- No private body, policy body, availability-method body, vault location, network address, retrieval credential, recipient DID, or contact data.
- No production, pilot, member-ready, organizer-ready, live-federation, NYCN, or Phase-2 claim.
- No #2041 completion claim.
- No closure of #1748, #2141, #2041, #1868, #2061, #2080, #2081, or #1907.

## References

- [`access-made-available-disclosure-receipt-decision-rung.md`](access-made-available-disclosure-receipt-decision-rung.md)
- [`PRIVATE_DATA_DISCLOSURE_BOUNDARY.md`](../architecture/PRIVATE_DATA_DISCLOSURE_BOUNDARY.md)
- [`network-anti-entropy-proof-loops.md`](../spec/network-anti-entropy-proof-loops.md)
- [`artifact-registry-and-scoped-vault.md`](../spec/artifact-registry-and-scoped-vault.md)
- [`institutional-domain.md`](../spec/institutional-domain.md)
- [`governed-service-binding.md`](../spec/governed-service-binding.md)
- [`effect-dispatch-contract.md`](../spec/effect-dispatch-contract.md)
- [`federation-settlement-finality.md`](../spec/federation-settlement-finality.md)
- [`KERNEL_APP_SEPARATION.md`](../architecture/KERNEL_APP_SEPARATION.md)
- [`INSTITUTION_PACKAGE_BOUNDARY.md`](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md)
- [`repository-map.md`](../reference/project-index/repository-map.md)

Refs #2336.
Refs #2335.
Refs #2334.
Refs #2333.
Refs #2330.
Refs #1792.
Refs #1748.
Refs #2141.
Refs #2041.
Refs #1868.
Refs #2061.
Refs #2080.
Refs #2081.
