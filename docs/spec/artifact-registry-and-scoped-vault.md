---
Status: normative
Authority: spec (defines the forward-direction `ArtifactRegistry` v0 and `ScopedVault` boundary that the merged storage-durability-policies spec and the integrated system model both track under #1798)
Canonical: no
Last Reviewed: 2026-05-14
---

# Artifact Registry and Scoped Vault — boundary, v0 design

> **Status: spec, work-in-progress.** Defines the design-level shape of `ArtifactRegistry` v0 (the institutional record of content-addressed artifacts and their metadata) and `ScopedVault` (the privacy-enforced container for restricted objects). Names how documents, compute outputs, evidence packets, private evidence, and mobile cache fit. Reuses existing canonical types verbatim (`Hash`, `Did`, `Signature`, `StorageClass`, `DataLocality`, `ArtifactReceipt` from `icn-kernel-api`; `StorageSpec` / `BackupPolicy` / `ReplicationPolicy` / `RecoveryPolicy` / `ArchivePolicy` / `IntegrityPolicy` from `#1816`). Cross-links the **forward-direction** `PrivacyClass` / `DisclosurePolicy` / `PrivateObjectRef` / `AccessReceipt` / `ExportReceipt` / `RedactionMap` vocabulary proposed under `#1792` (still forward-direction at the time of writing — and distinct from existing `PrivacyClass` enums in code; see §"`ScopedVault` (object outline)" for the naming-collision surface this spec keeps explicit). Explicitly defers wire-stable schema, encryption / key model, and storage-backend implementation. The PR introducing this doc advances `#1798` without closing it.

## Purpose

Storage in ICN should mean **sovereign institutional memory**, not files in an app. The merged spine doc names a seven-class custody taxonomy; `docs/spec/storage-durability-policies.md` (`#1816`, merged via PR #1823) names the durability policies that govern each class. Two custody classes remain forward-direction in those merged specs:

- **Artifact / blob store** — the content-addressed memory of artifacts the institution produces and reads. Tracked under #1798.
- **Scoped vault / private storage** — the privacy-enforced container for restricted objects. Tracked under #1798 (and #1767 for encryption, #1792 for disclosure).

This spec defines the design-level shape of both, and the boundary between them. It does not define schema, wire format, encryption, or storage-backend implementation. It does name how the existing receipt envelope (ADR-0026 Layer 2 `ArtifactReceipt`), the forward-direction privacy / disclosure vocabulary proposed under `#1792` (`PrivacyClass` / `DisclosurePolicy` / `PrivateObjectRef` / `AccessReceipt`), and the merged durability policy objects (`#1816`'s `StorageSpec` / `BackupPolicy` / `ReplicationPolicy` / `RecoveryPolicy` / `ArchivePolicy` / `IntegrityPolicy`) compose with the new `ArtifactRegistry` and `ScopedVault` records. Note: `#1816` does **not** define a separate `RetentionPolicy` object — retention is a field inside `BackupPolicy` (retention window) and `ArchivePolicy` (retention horizon). And `#1792`'s seven-variant `PrivacyClass` taxonomy is forward-direction; the codebase already exports a different `PrivacyClass` enum (`icn-kernel-api/src/compute.rs:217` with three variants — `Public` / `Member` / `NeedToKnow` — and a separate two-variant enum in `icn-boundary/src/types.rs:30`), which this spec keeps separate from `#1792`'s proposed taxonomy.

The four merged sibling architecture specs each reference #1798 as the home for this boundary:

- `docs/spec/storage-durability-policies.md` (#1823, merged): names "Artifact / blob store" and "Scoped vault / private storage" as custody classes whose registry / vault details live under `#1798`.
- `docs/spec/institutional-domain.md` (#1820, merged): names "scoped vault" in the storage taxonomy reference.
- `docs/spec/governed-service-binding.md` (#1822, merged): names the binding's storage object reference may point at "an artifact in the registry, a volume, a vault entry, a service state store. References to objects in `#1798` (`ArtifactRegistry`) and `#1767` (encrypted private-overlay store) when those land."
- `docs/spec/effect-dispatch-contract.md` (#1819, merged): names `ArtifactReceipt` (Layer 2) as one of the receipt classes emitted at Stage 5.

This spec gives those names design-level shape.

## What this spec is not

- Not runtime implementation. No code lands here.
- Not a schema migration. No Rust types, JSON schemas, or wire formats are introduced.
- Not an encryption implementation. The vault's "encryption / key model" field is a **placeholder**; specifics are deferred to `#1767`.
- Not a blob migration. Existing `blob_store` (in `icn/crates/icn-store/src/blob_store.rs`) continues to operate; the spec names what an artifact record points to, not how the bytes move.
- Not a redefinition of `ArtifactReceipt` (the existing Layer 2 receipt type in `icn-kernel-api/src/proofs.rs:30`). The receipt proves a blob transfer; the registry records that an artifact exists and what governs it. These are distinct concepts (see §"`ArtifactReceipt` vs `ArtifactRegistry`" below).
- Not a redefinition of `PrivacyClass`, `DisclosurePolicy`, `PrivateObjectRef`, `AccessReceipt`, `ExportReceipt`, or `RedactionMap`. Those are proposed (forward-direction) under `#1792`; this spec references the proposed names without redefining them. The existing `PrivacyClass` enums in code (`icn-kernel-api/src/compute.rs:217`, `icn-boundary/src/types.rs:30`) are distinct from `#1792`'s proposed seven-variant taxonomy; reconciliation is forward work and out of scope here.
- Not a redefinition of `StorageClass`, `DataLocality`, `StorageSpec`, `BackupPolicy`, `ReplicationPolicy`, `RecoveryPolicy`, `ArchivePolicy`, or `IntegrityPolicy`. Those are defined under `#1816` (`docs/spec/storage-durability-policies.md`, merged via PR #1823); this spec references them. `#1816` does **not** define a separate `RetentionPolicy` object — retention is a field inside `BackupPolicy` and `ArchivePolicy`.
- Not authority to mutate DNS, K3s, Forgejo, identity-bridge, gateway, blob storage backends, or any deployed infrastructure.
- Not a production-readiness or live-federation claim.
- Not closure of `#1798`. The PR introducing this doc uses `Refs:`; closure is left for separate review against the issue's six acceptance criteria.

## Boundary lines

Storage objects nearby in the architecture overlap with `ArtifactRegistry` and `ScopedVault` but are distinct. The boundary is load-bearing.

| Object | Distinct from `ArtifactRegistry` / `ScopedVault` in this way |
|---|---|
| **`ArtifactReceipt`** (existing Layer 2 receipt in `icn-kernel-api/src/proofs.rs:30`) | A receipt that proves a blob transfer happened, with fields `blob_hash`, `provider_did`, `requester_did`, `request_id`, `scope_id`, `verified_at`, `receipt_hash`, `signature`. The new `ArtifactRegistry` is the institutional record that an artifact **exists** and what governs it; the receipt proves an **action on** the artifact. The registry holds `receipt_refs`; some of those refs point at `ArtifactReceipt` instances. They share a name root and are intentionally separable. |
| **`ReceiptStore`** (existing `icn-gateway/src/receipt_store.rs`) | Persistent receipt backend keyed by `record_hash`. Stores governance receipts, allocation receipts, attendance receipts. The `ArtifactRegistry` is a sibling store for artifact metadata, not a replacement. The two stores collaborate: receipts produced about artifacts land in `ReceiptStore`; the artifact metadata that describes the underlying object lands in `ArtifactRegistry`. |
| **Generic key-value records** (existing `icn-store` abstractions for service state) | Large blobs SHOULD NOT be stored directly in generic key-value records. Artifacts go through the registry; the registry points at a content-addressed blob location (filesystem, object store, or distributed overlay). The KV store remains for small mutable records. |
| **`ScopedVault`** (this spec) | The vault is the privacy-enforced container for restricted objects. Vault-backed objects appear in the artifact registry as `PrivateObjectRef`s (per `#1792`); the vault enforces access policy and emits `AccessReceipt`s. A vault entry is itself an artifact in the registry sense; the vault adds the privacy enforcement layer. |
| **`PrivateObjectRef`** (per `#1792`) | The reference shape used by the registry when an artifact's content lives in a vault. Carries `object_id`, `content_hash`, `vault_id`, `privacy_class`, `policy_hash`, `receipt_ref`. The registry record's `blob_location` for a vault-backed artifact is a `PrivateObjectRef`, not a plaintext path. |
| **`InstitutionalDocument`** (proposed by `#1536`) | A typed artifact metadata kind, not a separate storage universe. Documents become artifacts in the registry with `artifact_class = Document` and document-specific typed fields layered on top. See §"Integration points" below. |
| **Mobile / member-shell cache** | Disposable derived state. Reconstructible from the source artifact. **MUST NOT** be treated as canonical (matches `#1816` §"Failure and safety rules"). Cache entries are not registered as artifacts. |

The shorthand:

> The registry records what artifacts exist and what governs them. The vault enforces privacy on restricted artifacts. The receipt store proves actions on both.

## `ArtifactRegistry` v0 (object outline)

The artifact registry is the institutional memory of the cooperative's content-addressed artifacts. Each entry is a metadata record describing one logical artifact, pointing at the blob's location, naming what governs it, and recording what receipts have been emitted about it.

Fields at design granularity (no Rust type, no JSON schema, no field order fixed):

**Identity**

- **`artifact_id`** — Stable identifier for this artifact as a logical object. Survives version churn.
- **`content_hash`** — `Hash` (from `icn-kernel-api`); content-addressed reference to the bytes. The hash is the integrity surface (also referenced by `#1816`'s `IntegrityPolicy`).

**Storage backend reference**

- **`blob_location`** — Where the bytes live. For public-by-policy artifacts: a content-addressed path in the cooperative's blob store. For vault-backed artifacts: a `PrivateObjectRef` (per `#1792`) pointing into a `ScopedVault`. The registry never holds plaintext for restricted material; it holds the reference and lets the vault enforce access.
- **`mime_type`** — Media type / format identifier.
- **`size`** — Byte size (informational; not the authoritative integrity surface).

**Classification**

- **`artifact_class`** — One of the closed taxonomy named in §"Artifact class taxonomy" below.
- **`scope`** — Owning scope (domain, structure, activity). The scope is the authority root for this artifact's lifecycle.

**Provenance**

- **`created_by`** — `Did` of the actor that introduced this artifact into the registry.
- **`created_at`** — Timestamp of registration.
- **`provenance_refs`** — Pointers back to the governance act (or other authority basis) that produced this artifact: `decision_hash`, `proposal_id`, `mandate_id`, `effect_record_id`, or an explicit `none` where the artifact predates a covering governance act and is grandfathered.

**Policy references**

- **`access_policy_ref`** — Pointer to the `DisclosurePolicy` (per `#1792`) that governs who may read this artifact under what authority.
- **`retention_policy_ref`** — Pointer to the durability policy set that governs this artifact's retention. `#1816` carries retention as fields inside `BackupPolicy` (retention window) and `ArchivePolicy` (retention horizon); the artifact's `retention_policy_ref` resolves to one or both of those policies adopted by the owning domain, not to a separate `RetentionPolicy` object.

**Receipts**

- **`receipt_refs`** — Pointers to receipts emitted about this artifact: creation receipt, access receipts (per `AccessReceipt`), export receipts, modification receipts (where applicable), expiration receipts. The list is append-only; each receipt is itself in `ReceiptStore` per `effect-dispatch-contract.md` §"Stage 5". For blob-transfer events specifically, the receipt is the existing `ArtifactReceipt` (Layer 2).

**Versioning**

- **`version_ref`** — Stable version identifier within this artifact's lineage. Reuses `SemanticVersion` shape from `icn/crates/icn-ccl/src/registry.rs` where applicable; otherwise a content-hash-derived version.
- **`parent_refs`** — Pointers to predecessor versions, supersession records, or composing artifacts. Reversal of a version is a fresh registration with a parent pointer, never an edit.

**Exportability**

- **`exportability`** — Class indicating whether and how this artifact may be exported: `Public-portable`, `Member-portable-with-receipt`, `Scope-restricted`, `Vault-only-with-redaction`, `Not-exportable`. Exportability inherits from the `DisclosurePolicy` and from `BackupPolicy` / `ArchivePolicy` retention horizons; the registry record names the resulting class for quick reference.

**Invariants:**

- An artifact MUST NOT be registered without a `content_hash` and `blob_location`. A record with neither is not an artifact; it is a placeholder, and the chain MUST refuse it.
- An artifact MUST carry an `access_policy_ref` and a `retention_policy_ref` (or an explicit acknowledgement of `none` where the policy intentionally allows it, e.g., for purely public artifacts whose retention is "indefinite, public-by-policy").
- An artifact's `scope` MUST match (or be **narrower** than) the authority basis named in `provenance_refs`. A workload cannot register an artifact into a scope wider than the authority it acts under. (Earlier draft said "wider than"; that was inverted — authority is the upper bound, scope is what the actor is permitted to write into.)
- Reversal / supersession is a new version, not a mutation. The registry is append-only.

## `ScopedVault` (object outline)

The scoped vault is the privacy-enforced container for restricted objects. A vault is the authority-bound space that holds `PrivateObjectRef`s (per `#1792`) and emits `AccessReceipt`s on every read.

Fields at design granularity:

**Identity and ownership**

- **`vault_id`** — Stable identifier for this vault as a logical container.
- **`owning_scope`** — Scope that owns this vault: domain, structure, activity, or holder DID. The owning scope is the authority root for vault policy decisions.

**Privacy and disclosure**

- **`privacy_class`** — A class from the **forward-direction** seven-variant taxonomy proposed under `#1792`: `Public` / `MembersOnly` / `ScopeRestricted` / `PrivateOverlay` / `SecretCredential` / `ExternalCustodian` / `SealedUntil`. The vault's privacy class is the upper bound for everything it holds; an artifact entering the vault inherits at least the vault's privacy class.

  **Naming collision note.** Two unrelated `PrivacyClass` enums already exist in code: `icn/crates/icn-kernel-api/src/compute.rs:217` (`Public` / `Member` / `NeedToKnow`, used by compute workload manifests) and `icn/crates/icn-boundary/src/types.rs:30` (`Public` / `EncryptedOverlay`, used at the boundary layer). The `#1792` taxonomy this `ScopedVault` field uses is a **third, broader naming** intended for the disclosure / vault domain — it is **not** the same enum as either of the in-code types, and `#1792` itself is still forward-direction (the seven variants are proposed, not implemented). The eventual implementation tranche for `ScopedVault` will need to decide whether (a) `#1792` ships a separate `DisclosureClass` enum, (b) the existing `kernel-api::PrivacyClass` is widened to seven variants, or (c) both coexist with a typed conversion. That decision belongs to the implementation tranche, not to this design spec. This field reads against the `#1792` proposal until then.
- **`encryption_key_model_placeholder`** — Where the vault's encryption model would be named. **Specifics are deferred to `#1767`** (encrypted distributed private-overlay storage). This spec names the placeholder slot; it does not specify the cipher, key custody, rotation, or recovery protocol.

**Access and retention**

- **`access_policy`** — Pointer to or inline `DisclosurePolicy` (per `#1792`) carrying `visibility`, `allowed_scopes`, `redaction_rules`, `access_request_path`.
- **`retention_policy`** — Pointer to the durability policy set that governs vault retention. `#1816` defines retention as fields inside `BackupPolicy` (retention window) and `ArchivePolicy` (retention horizon); the vault's `retention_policy` resolves to one or both of those policies adopted by the owning domain, not to a separate `RetentionPolicy` object. A vault entry's `BackupPolicy` inherits the vault's `privacy_class` and MUST NOT broaden disclosure (per `#1816` §"Locality and privacy inheritance").
- **`backup_export_policy`** — Specific subset of the retention policy that names when and how vault material may be backed up or exported, and what authority is required.

**Receipts**

- **`access_receipt_requirement`** — Declarative rule for what an access produces. Every read MUST emit an `AccessReceipt` (per `#1792`) naming `object_ref`, `actor`, `authority_basis`, `purpose`, `timestamp`. No silent reads.
- **`export_receipt_requirement`** — Every export of vault material MUST emit an `ExportReceipt` (per `#1792`) naming the redaction applied, the recipient, and the governance authority that authorized the export.

**Relationship to private overlays**

- **`private_overlay_binding`** — Where the vault delegates to a distributed private overlay (per `#1767`), this field names the binding. For local-only vaults, this is `none`. The binding is itself a forward-direction concept; `#1767` will specify its shape.

**Invariants:**

- A vault MUST carry an `access_policy` and a `retention_policy`. A vault without those is not safely adoptable.
- A vault MUST emit an `AccessReceipt` on every successful read and an `ExportReceipt` on every successful export. No silent vault access.
- A vault's `privacy_class` is the upper bound for entries it holds; entries cannot have a wider privacy class than the vault.
- Backup of vault material MUST inherit the vault's `privacy_class` and `DataLocality` (per `#1816` §"Locality and privacy inheritance"). Backup that broadens either is refused at the policy layer.
- Vault contents MUST NOT be exposed in member-shell or steward-cockpit dashboards. Surfaces show the existence of vault entries (the **fact** that they exist) and their access provenance, never the contents.

## `ArtifactReceipt` vs `ArtifactRegistry`

The two share a name root and are intentionally separable.

| Concept | Lives in | Purpose | Key fields |
|---|---|---|---|
| **`ArtifactReceipt`** (existing, implemented) | `icn-kernel-api/src/proofs.rs:30` | Proves a blob transfer occurred under specific authority. Layer 2 of ADR-0026. | `blob_hash`, `provider_did`, `requester_did`, `request_id`, `scope_id`, `verified_at`, `receipt_hash`, `signature`. |
| **`ArtifactRegistry`** (this spec, forward-direction) | TBD (forthcoming follow-up: `schema(storage): define ArtifactRegistry persisted records`) | Records the existence of an artifact and what governs it (policy, provenance, receipts). The institutional memory of artifacts. | `artifact_id`, `content_hash`, `blob_location`, `mime_type`, `size`, `artifact_class`, `scope`, `created_by`, `created_at`, `access_policy_ref`, `retention_policy_ref`, `provenance_refs`, `receipt_refs`, `version_ref`, `parent_refs`, `exportability`. |

The registry's `receipt_refs` field may point at `ArtifactReceipt` instances (when blob transfer events are recorded) alongside other receipt classes (creation receipt, `AccessReceipt`, `ExportReceipt`, expiration receipt). The names are deliberately similar because both concern artifacts; the responsibilities are distinct.

## Artifact class taxonomy

The `artifact_class` field carries a closed taxonomy at the design level. New classes require an ADR amendment (matching the pattern from `#1815` runtime classes and `effect-dispatch-contract.md` receipt classes).

Initial closed set (each may carry typed metadata beyond the registry's generic fields):

| Class | Typical content | Typed metadata source |
|---|---|---|
| `Document` | Typed institutional document (meeting minutes, charter, policy text, committee report, etc.) | `#1536` — typed institutional memory / documents. The registry record carries `doc_type` and document-specific fields. |
| `ComputeOutput` | Output of a workload execution (per `docs/spec/governed-service-binding.md`'s `WorkloadManifest.outputs`) | `#1815` — governed service binding. The registry record carries `manifest_ref`, `binding_id`, `compute_receipt_ref` (per ADR-0031). |
| `EvidencePacket` | Redacted / exportable bundle assembled from underlying artifacts and receipts | `#1748` — institutional process substrate. The packet's contents are themselves artifacts; the packet is an artifact whose `blob_location` is the assembled bundle. |
| `PrivateEvidence` | Vault-backed evidence (a `PrivateObjectRef` per `#1792`) | `#1792` — private data disclosure boundary. The registry record's `blob_location` is a `PrivateObjectRef`; reading requires going through the vault. |
| `Backup` | A backup of canonical or service state (per `#1816`'s `BackupPolicy`) | `#1816` — storage durability policies. The registry record carries `source_artifact_ref` or `source_storage_spec`, `backup_policy_ref`, integrity verification result. |
| `SettlementRecord` | Economic settlement artifact (per `#1634`) | `#1634` — obligation / allocation / settlement primitives. The registry record carries `settlement_target`, `obligation_ref`, `allocation_ref`. |
| `Other` | Forward-direction placeholder. New classes start here and migrate to a named class via ADR amendment when the pattern matures. | — |

The list is **closed at the kind level** (no silent expansion of `artifact_class` values) and **open at the typed-metadata level** (each class may layer additional fields from its source spec). The boundary holds the kernel/app meaning-firewall principle: the registry does not interpret institution-specific meaning of an artifact; it carries the class label and points at the source spec for typed semantics.

## Integration points

The six integration points named in `#1798`:

### 1. `InstitutionalDocument` as typed artifact metadata

Per `#1536`. Documents are not a separate storage universe; they are an `artifact_class` (`Document`) with typed metadata layered on top of the generic registry fields. `doc_type` (e.g., `MeetingAgenda`, `MeetingMinutes`, `CommitteeReport`, `Policy`, `Charter`) and document-specific parent / version pointers live in the document's typed metadata; the registry's generic fields carry identity, content hash, scope, policy refs, provenance, receipts.

### 2. Compute outputs as artifacts

Per `#1815` / `docs/spec/governed-service-binding.md`. A `WorkloadManifest` declares `outputs`; each completed workload execution registers its outputs as artifacts of class `ComputeOutput`, with `provenance_refs` pointing at the binding's `mandate_ref` and the dispatch chain's `decision_hash`. The `ComputeReceipt` (per ADR-0031) is one of the `receipt_refs`.

Review state for advisory outputs (per ADR-0030, utility computation outputs are advisory until governance ratifies them) lives in the artifact's typed metadata: a review status field, the ratifying governance act if any, the receipt of the ratification.

### 3. Evidence packets as redacted / exportable bundles

Per `#1748`. An evidence packet is itself an artifact (class `EvidencePacket`) whose `blob_location` is the assembled bundle. The packet's contents are individually artifacts (typically `Document`, `ComputeOutput`, `Backup` instances); the packet record carries pointers to its constituent artifact records. Export of an evidence packet emits a packet-level `ExportReceipt` plus access receipts for any vault-backed contents touched during assembly.

### 4. Private evidence as vault-backed `PrivateObjectRef`s

Per `#1792`. An artifact of class `PrivateEvidence` has `blob_location` set to a `PrivateObjectRef` pointing into a `ScopedVault`. The registry records that the artifact exists, who is authorized to access, and what receipts have been emitted; the vault holds the bytes (encrypted per `#1767`'s eventual key model) and enforces the access policy on every read.

### 5. Mobile / member-shell cache as disposable derived

Per the spine doc's seven-class custody taxonomy and `#1816`'s "cache treated as canonical is a bug" rule. Mobile cache is reconstructible from the source artifact; it is **not registered** in `ArtifactRegistry`. The mobile cache holds local copies for offline operation; canonical truth lives in the registry. When the mobile cache and the registry diverge, the registry wins.

### 6. Replication policy tied to privacy / disclosure class

Per `#1816`'s `ReplicationPolicy`. An artifact's replication policy MUST be bounded by its `privacy_class`: a `PrivateOverlay` artifact cannot replicate into `FederationMirrored` or `CommonsPublic`; a `Public` artifact may replicate freely under its `BackupPolicy`. The binding from `ReplicationPolicy` to `privacy_class` is the load-bearing rule (and matches `#1816`'s "Replica violates locality / privacy policy" failure mode).

## Receipt expectations

The registry and vault emit receipts at significant lifecycle transitions. No new receipt class is introduced by this spec; existing classes from `docs/spec/effect-dispatch-contract.md` §"Receipt class summary" and `#1792`'s taxonomy are reused.

| Transition | Receipt class | Source |
|---|---|---|
| Artifact created / registered | Creation effect record (per ADR-0025 forward direction); referenced by the registry's `receipt_refs` | `docs/spec/effect-dispatch-contract.md` §"Stage 5" |
| Artifact read (public-by-policy) | None required at the artifact level; the dispatching workload's evidence is enough | — |
| Vault entry accessed | `AccessReceipt` (per `#1792`) | `#1792` |
| Artifact exported | `ExportReceipt` (per `#1792`); for blob-transfer-specifically: `ArtifactReceipt` (Layer 2, ADR-0026, existing) | `#1792`, ADR-0026 |
| Vault entry exported | `ExportReceipt` with redaction reference (per `#1792`) | `#1792` |
| Artifact new version registered | Creation effect record + version-link receipt | `docs/spec/effect-dispatch-contract.md` |
| Artifact expired / retired | Expiration receipt (per `#1816` `ArchivePolicy.Expiration authority`) | `#1816` |
| Artifact backup created | Backup-creation receipt (per `#1816` `BackupPolicy.Receipt classes`) | `#1816` |
| Artifact restored | Restore-execution receipt (per `#1816` `RecoveryPolicy.Receipt classes`) | `#1816` |
| Vault entry deletion (governed) | Deletion receipt with explicit authority basis | `#1816`, `#1792` |

The registry never invents a new receipt class. It carries `receipt_refs` to existing classes.

## Boundary rules

Six load-bearing rules, drawn directly from `#1798`'s scope.

| Rule | What it means |
|---|---|
| **No large blobs in generic key-value records.** | The KV store is for small mutable service state. Large blobs go through `blob_store` and are pointed at by the registry's `blob_location`. The registry doesn't store bytes; it stores references. |
| **No private vault contents in dashboards.** | Dashboards (member shell `#1818`, steward cockpit `#1795`) MUST NOT render vault contents. They MAY show that a vault entry exists, its `privacy_class`, its access provenance, and access-receipt counts — never the bytes. |
| **No bypass of receipt / provenance for artifact lifecycle.** | Every artifact registration, every version, every vault read, every export emits a receipt. No silent operations on registered artifacts. |
| **The registry does not interpret institution-specific meaning.** | The registry carries `artifact_class` labels and generic fields. Institution-specific semantics (a federation's role names, a cooperative's intake form schema) live in the typed metadata attached per class. The boundary is the meaning firewall (per `docs/architecture/KERNEL_APP_SEPARATION.md`). |
| **The vault is not a generic private-state dumping ground.** | Every vault carries an `access_policy` and a `retention_policy`. An untyped private blob with no policy MUST NOT enter a vault. The vault is a governed surface, not a pile. |
| **No encryption / key / runtime implementation in this spec.** | The vault's `encryption_key_model_placeholder` is a slot. Specifics live in `#1767`. This spec names the slot's existence; it does not specify the cipher or key custody. |

## Failure and safety rules

| Failure | Response |
|---|---|
| Artifact registration without `content_hash` or `blob_location` | Adoption MUST fail. Not an artifact. |
| Artifact registration without `access_policy_ref` or `retention_policy_ref` | Adoption MUST fail (unless an explicit "policy intentionally `none`" record names why; this is a governance act in itself). |
| Artifact `scope` wider than the authority basis in `provenance_refs` | Refused. Cannot register into a scope you don't have authority over. |
| Vault read without an `AccessReceipt` | Bug. The read MUST be refused at the vault layer and recorded as evidence. |
| Vault export without `ExportReceipt` with redaction reference | Bug. Export MUST be refused. |
| Vault entry with `privacy_class` wider than the vault | Adoption MUST fail. An entry cannot widen the vault's privacy upper bound. |
| Backup of vault material that broadens locality or disclosure | Refused at the policy layer (per `#1816` §"Locality and privacy inheritance"). |
| Private overlay binding referenced but `#1767` types not yet implemented | Vault MAY enter a `pending-overlay` state where local-only operation is allowed; the binding is recorded but inert until `#1767`'s implementation lands. Surface the gap in the steward cockpit. |
| Mobile cache treated as canonical | Bug per `#1816`. Cache MUST NOT be registered as an artifact. |
| Registry surfaces vault contents in a dashboard | Bug. Refuse at the dashboard layer; record the attempt. |
| Artifact class outside the closed taxonomy | Refused. New classes require ADR amendment. |
| Replication of a `PrivateOverlay` artifact into `FederationMirrored` or `CommonsPublic` | Refused per `#1816` `ReplicationPolicy` constraint. |
| Stale policy reference (artifact points at deprecated `BackupPolicy` version) | Domain MUST amend its `DomainPolicy` via governance before continuing; same fail-closed pattern as `ccl-policy-registry.md` and `storage-durability-policies.md`. |

Cross-cutting rule (matches the prior six architecture specs): **fail closed, surface honestly, never silently fall back.**

## First safe implementation slice

Per #1798's fifth acceptance criterion ("Spec identifies first safe implementation slice without starting it"). The candidate is:

> **`ArtifactRegistry` v0 schema and `Document` artifact-class wire-stable record, with read-only registry surface for the steward cockpit.**

Why this slice first:

- The `Document` class has the most active need (per `#1536`'s typed institutional memory).
- A read-only registry surface exercises the metadata model without committing to write paths (which need `BackupPolicy` and `ArchivePolicy` from `#1816` to be implemented first; those policies carry the retention fields that the artifact's `retention_policy_ref` resolves to).
- It does not require `ScopedVault` implementation: documents in this slice are public-by-policy or members-only (not vault-backed). Vault-backed artifact classes (`PrivateEvidence`) are deferred to the slice after `#1767` lands.
- It does not require encryption (`#1767` not yet implemented).
- Steward cockpit (`#1795`) is the natural first reader; member shell (`#1818`) is the second reader.

This is a recommendation, not a commitment. The actual implementation tranche order is for the user.

## Relationship to sibling work

Cross-link, do not duplicate.

| Concern | Where it lives |
|---|---|
| Storage durability policies (`StorageSpec`, `BackupPolicy`, `ReplicationPolicy`, `RecoveryPolicy`, `ArchivePolicy`, `IntegrityPolicy` — retention is a field inside `BackupPolicy` / `ArchivePolicy`, not a separate `RetentionPolicy` object) | `docs/spec/storage-durability-policies.md` (`#1816`, merged via PR #1823). |
| Governed service binding (`StorageSpec` lives in the binding) | `docs/spec/governed-service-binding.md` (`#1815`, merged via PR #1822). |
| `InstitutionalDomain` + `DomainPolicy` (the domain adopts vault + artifact policies) | `docs/spec/institutional-domain.md` (`#1820`, merged via PR #1820). |
| Effect dispatch contract (Stage 5 evidence; receipt classes) | `docs/spec/effect-dispatch-contract.md` (`#1819`, merged via PR #1819). |
| CCL policy registry (artifact-related policy adoption) | `docs/spec/ccl-policy-registry.md` (`#1821`, merged via PR #1821). |
| Integrated cooperative operating model (storage as custody) | `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` (`#1793`, merged via PR #1814). |
| Foundational storage canon (`StorageClass`, `DataLocality`, `StorageValidationError`) | `docs/state/storage-governance-spec.md` + `icn/crates/icn-kernel-api/src/storage.rs`. |
| Private data disclosure boundary (`PrivacyClass`, `DisclosurePolicy`, `PrivateObjectRef`, `AccessReceipt`, `ExportReceipt`, `RedactionMap`) | `#1792`. |
| Encrypted distributed private-overlay storage (key custody for vault) | `#1767`. |
| Typed institutional memory / documents (`InstitutionalDocument` doc_type) | `#1536`. |
| Institutional process substrate (evidence packets) | `#1748`. |
| Obligation / allocation / settlement primitives (`SettlementRecord` class) | `#1634`. |
| Network anti-entropy proof loops (replication convergence surface) | `#1799`. |
| Compute placement (resource allocation for storage-providing workloads) | `#1801`. |
| Member shell v0 (member-facing artifact rendering) | `#1818`. |
| Steward cockpit v0 (operator-facing registry surface) | `#1795`. |
| Receipt and provenance proof envelope (`ArtifactReceipt` Layer 2) | ADR-0026. |
| Constitutional object model (`AuthorityClass`, `Mandate`) | ADR-0014. |
| Commons compute admission and settlement (`ComputeReceipt`) | ADR-0031. |

## Open questions and deferred decisions

| Question | Deferred to |
|---|---|
| Wire-stable schema for `ArtifactRegistry` and `ScopedVault` records | Follow-up `schema(storage): define ArtifactRegistry and ScopedVault persisted records`. |
| Closed taxonomy of `artifact_class` values past v0 | ADR amendment per new class. |
| Typed-metadata schemas for each artifact class (`Document`, `ComputeOutput`, `EvidencePacket`, `PrivateEvidence`, `Backup`, `SettlementRecord`) | Each class's source spec: `#1536`, `#1815` / `#1822`, `#1748`, `#1792`, `#1816`, `#1634`. |
| Encryption / key model for `ScopedVault` (the `encryption_key_model_placeholder` slot) | `#1767`. |
| Distributed private-overlay binding shape | `#1767`. |
| `AccessReceipt` / `ExportReceipt` envelope (Rust type, persistence, audit-query surface) | `#1792` and the receipt-envelope follow-ups for `#1816`. |
| Registry read-only surface for the steward cockpit (the first safe implementation slice) | `#1795`. |
| Member-shell rendering of artifacts | `#1818`. |
| Replication convergence verification for artifact metadata | `#1799`. |
| `ProvenanceQuery` (Layer 4 surface for cross-coop artifact discovery) | ADR-0026 §4; tracked under `#1438`. |
| Reconciliation of the 7-class custody taxonomy with the 3-variant kernel `StorageClass` enum | Carried from `#1816`; not in scope here. |

## Non-claims (repeated for clarity)

- This document does not introduce `ArtifactRegistry` or `ScopedVault` as Rust types. They remain forward-direction.
- It does not introduce schema, wire format, or contract changes.
- It does not redefine `ArtifactReceipt` (the existing Layer 2 receipt) or any of the other types it references — kernel-api types (`Hash`, `Did`, `Signature`, `StorageClass`, `DataLocality`); forward-direction `#1792` proposals (`PrivacyClass`, `DisclosurePolicy`, `PrivateObjectRef`, `AccessReceipt`, `ExportReceipt`, `RedactionMap`); merged `#1816` policy objects (`StorageSpec`, `BackupPolicy`, `ReplicationPolicy`, `RecoveryPolicy`, `ArchivePolicy`, `IntegrityPolicy`). The spec also does not invent a `RetentionPolicy` object; `#1816` does not define one (retention lives inside `BackupPolicy` and `ArchivePolicy`).
- It does not specify encryption, key custody, or the distributed private-overlay protocol. Those are tracked under `#1767`.
- It does not authorize mutation of any deployed infrastructure (DNS, K3s, Forgejo, identity-bridge, gateway, blob storage backends).
- It does not claim production readiness.
- It does not claim live inter-cooperative federation.
- It does not claim any partner federation is operating under these primitives today. NYCN appears nowhere in the spec; institution-package vocabulary stays in institution packages.
- It does not use payment, wallet, balance, currency, or blockchain as ICN-native framing. The discipline list in `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` §"Vocabulary discipline" applies in full.
- It does not by itself close `#1798`. The PR uses `Refs:`.

## Related documents and issues

Architecture and spine:

- `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` — §"Storage as custody" (the seven-class custody taxonomy this spec extends).
- `docs/architecture/KERNEL_APP_SEPARATION.md` — meaning firewall; the kernel never imports artifact / vault types.
- `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` — generic shapes vs. package vocabulary; the registry holds generic `artifact_class`, packages carry typed metadata.
- `docs/architecture/COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` — design-direction overview.

Specs (siblings and parent):

- `docs/spec/storage-durability-policies.md` — durability policies the registry references.
- `docs/spec/governed-service-binding.md` — `StorageSpec` references the registry.
- `docs/spec/institutional-domain.md` — `DomainPolicy` adopts vault and artifact policies.
- `docs/spec/effect-dispatch-contract.md` — Stage 5 receipts the registry's `receipt_refs` carry.
- `docs/spec/ccl-policy-registry.md` — policy adoption is a CCL-evaluated governance act.
- `docs/spec/KERNEL_CONTRACTS.md` — kernel primitive contracts.

State and design:

- `docs/state/storage-governance-spec.md` — foundational `StorageClass` / `DataLocality` canon.
- `docs/design/compute-classes.md` — runtime classes that produce `ComputeOutput` artifacts.

ADRs:

- ADR-0014 — Constitutional Object Model.
- ADR-0019 — Authority Grant Minting and Mandate Persistence Seam.
- ADR-0025 — Institutional Effect Record Canonical Schema (proposed).
- ADR-0026 — Receipt and Provenance Proof Envelope (`ArtifactReceipt` Layer 2).
- ADR-0027 — Action Card Contract.
- ADR-0030 — Compute Workload Manifest and Authority Boundary.
- ADR-0031 — Commons Compute Admission and Settlement Policy (`ComputeReceipt`).

Code surface (existing types this spec references):

- `icn/crates/icn-kernel-api/src/proofs.rs` — `ArtifactReceipt` (line 30; Layer 2 of ADR-0026 for blob transfers).
- `icn/crates/icn-kernel-api/src/storage.rs` — `StorageClass` (line 53), `DataLocality` (line 155), `StorageValidationError` (line 272).
- `icn/crates/icn-store/src/blob_store.rs` — hybrid blob store (sled metadata + filesystem blobs).
- `icn/crates/icn-store/src/lib.rs` — persistent KV abstraction; `StorageChallenge`, `MerkleProof`, `PeerCache`, `MaintenanceManager`.
- `icn/crates/icn-store/src/pos.rs` — proof-of-storage challenge system.
- `icn/crates/icn-gateway/src/receipt_store.rs` — receipt persistence backend (sibling store to the future `ArtifactRegistry`).
- `icn/crates/icn-governance/src/proof.rs` — `GovernanceProof`, `GovernanceDecisionReceipt`, `GovernanceDecisionAttestation`.

Issues:

- `#1798` — this spec.
- `#1793` — integrated operating model (merged via PR #1814).
- `#1794` — institutional domain (merged via PR #1820).
- `#1797` — effect dispatch contract (merged via PR #1819).
- `#1815` — governed service binding (merged via PR #1822).
- `#1816` — storage durability policies (merged via PR #1823).
- `#1817` — CCL policy registry (merged via PR #1821).
- `#1767` — encrypted distributed private-overlay storage.
- `#1792` — private data disclosure boundary.
- `#1536` — typed institutional memory / documents.
- `#1748` — institutional process substrate.
- `#1634` — obligation / allocation / settlement primitives.
- `#1799` — network anti-entropy proof loops.
- `#1801` — compute placement.
- `#1818` — member shell v0.
- `#1795` — steward cockpit v0.
- `#1438` — `ProvenanceQuery` Layer 4 surface (forward).
