---
Status: normative
Authority: spec (defines the forward-direction policy objects the spine doc names under §"Backup, redundancy, recovery, archive" and tracks under #1816; some clauses are explicitly forward-direction)
Canonical: no
Last Reviewed: 2026-05-14
---

# Storage Durability Policies — Backup, Replication, Recovery, Archive, Integrity

> **Status: spec, work-in-progress.** Defines the design-level policy objects that bind storage classes to durability, recoverability, locality, privacy, integrity, archival, and exit commitments. Names `StorageSpec`, `BackupPolicy`, `ReplicationPolicy`, `RecoveryPolicy`, `ArchivePolicy`, `IntegrityPolicy` as the six forward-direction objects the spine doc tracks under #1816 and the merged specs (#1819, #1820, #1821, #1822) defer to. Reuses existing kernel-level types (`StorageClass`, `DataLocality`, `StorageValidationError` from `icn/crates/icn-kernel-api/src/storage.rs`) verbatim where they apply. Clauses that depend on schema fields or kernel behavior that have not yet landed are marked as forward-direction. The PR introducing this doc advances `#1816` without closing it.

## Purpose

ICN's storage layer already enforces two kernel-level constraints: `StorageClass` (Canonical / ServiceState / Blobs, per `icn-kernel-api/src/storage.rs:53`) and `DataLocality` (CellLocal / CoopReplicated / FederationMirrored / CommonsPublic, per the same file at line 155). Those constraints establish what the kernel can refuse. They do not, by themselves, establish how storage stays durable, how a backup is taken or restored, what counts as a verified replica, what an archive owes the institution years from now, or what authority a restore requires.

The spine doc names six policy objects for the missing durability layer — `StorageSpec`, `BackupPolicy`, `ReplicationPolicy`, `RecoveryPolicy`, `ArchivePolicy`, `IntegrityPolicy` — and explicitly marks them as forward-direction tracked under #1816. The four merged sibling architecture specs each defer to this spec for the **design-level policy contract** for those objects:

- `docs/spec/governed-service-binding.md` (#1822, merged): "Backup policy reference. Pointer to the `BackupPolicy` the binding inherits (forward-direction, per `#1816`)."
- `docs/spec/institutional-domain.md` (#1820, merged): "Backup / recovery / archive defaults … (cross-link `#1816`)."
- `docs/spec/effect-dispatch-contract.md` (#1819, merged): "Backups inherit the disclosure policy of the source (see … the forthcoming backup spec under #1816)."
- `docs/spec/ccl-policy-registry.md` (#1821, merged): names `#1816` as the home for "backup / replication / recovery / archive policies" and for export-receipt provenance.

Foundational storage canon is **separate from the merged sibling specs above**. `docs/state/storage-governance-spec.md` is the existing foundational doc for `StorageClass`, `DataLocality`, and storage-access enforcement vocabulary; it independently names `StorageSpec` and `validate_storage_access()` as required follow-ups. This spec gives those names design-level shape without redefining the existing kernel-level types.

This spec defines those objects at **object granularity** — fields, lifecycle stages, authority bindings, receipt expectations, hard rules — and names the institutional rules that bind them to authority and receipts. **It does not introduce schema, wire format, Rust types, or storage-backend implementation.** It explicitly defers vendor choices, replication algorithms, SLA commitments, and storage-backend selection. It does not redefine `StorageClass` or `DataLocality`; both remain canonical per the kernel-level enums.

## What this spec is not

- Not a storage backend implementation. No code lands here.
- Not a backup-job implementation. The spec names what a backup is in institutional terms; the schedule, the worker, the wire format are forward work.
- Not a replication algorithm choice. The spec names what a replica owes; the algorithm is engineering choice deferred to implementation.
- Not a vendor-specific integration. Where the institution chooses an external custodian, that is a governed agreement (per `docs/spec/institutional-domain.md` §"Agreement registry"), not a spec-level coupling.
- Not a schema migration. No Rust types, JSON schemas, or wire formats are introduced.
- Not an SLA commitment. The spec names *objective classes* (e.g., "bounded restore time") rather than numerical SLAs; concrete numbers belong to per-domain `DomainPolicy` adoptions.
- Not a redefinition of `StorageClass` (3-variant kernel enum) or `DataLocality` (4-level kernel enum). Both remain canonical per `icn-kernel-api/src/storage.rs`.
- Not authority to mutate DNS, K3s, Forgejo, identity-bridge, gateway, backup jobs, replication targets, archive targets, or any deployed infrastructure.
- Not a production-readiness claim. No partner federation is operating under these primitives today.
- Not closure of `#1816`. The PR introducing this doc uses `Refs:`; closure is left for separate review against the issue's scope.

## Storage class model

ICN canon currently carries **two related but distinct** storage taxonomies. The spec uses both and names the relationship between them.

**Kernel-level `StorageClass` (3 variants, implemented):**

| Variant | Meaning |
|---|---|
| `Canonical` | Immutable, replicated, authoritative. Ledger entries, governance receipts, trust edges. Cannot be modified after creation. |
| `ServiceState` | Mutable, scope-default. Application state. Updated via CAS. Default for general compute tasks. |
| `Blobs` | Content-addressed, locality-flexible binary data. Documents, media files, WASM modules. Addressed by content hash; immutable by reference. |

These three variants are what the kernel can pattern-match on. Per the meaning firewall (`docs/architecture/KERNEL_APP_SEPARATION.md`), the kernel never enumerates a richer taxonomy.

**Design-level seven-class custody taxonomy** (per `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` §"Storage as custody"):

| Custody class | Kernel-level mapping | Per-class design expectations |
|---|---|---|
| **Canonical store** | `Canonical` | Custody: canonical. Locality: usually `CoopReplicated` or wider, per `DomainPolicy`. Privacy: typically `MembersOnly`+ except where intentionally public. Backup: mandatory. Replication: mandatory. Archive: mandatory. Restore authority: governed. Export: per domain exit policy. Receipts: every transition emits a receipt. |
| **Service state** | `ServiceState` | Custody: service state. Locality: per service. Privacy: per service binding. Backup: per-service `BackupPolicy`; some services accept lossy state. Replication: per-service `ReplicationPolicy`. Archive: usually not required. Restore authority: per service binding; routine for non-authoritative state. Export: per binding exit policy. Receipts: institutionally relevant transitions only. |
| **Artifact / blob store** | `Blobs` (tracked under #1798) | Custody: artifact. Locality: flexible; usually content-addressed. Privacy: per artifact registry classification. Backup: per artifact retention policy. Replication: per artifact class. Archive: per long-term retention class. Restore authority: per registry policy. Export: per content visibility. Receipts: significant artifact lifecycle transitions. |
| **Volume / block store** | (typically `ServiceState` underlying; access via runtime provider) | Custody: volume. Locality: per binding. Privacy: usually private-to-binding. Backup: per workload `BackupPolicy`. Replication: per `ReplicationPolicy` of the underlying provider. Archive: usually not. Restore authority: per binding. Export: per binding exit policy. Receipts: provisioning + destruction. |
| **Scoped vault / private storage** | (extends Blobs + access policy, tracked under #1767 / #1792) | Custody: vault. Locality: typically `CellLocal` or `CoopReplicated`. Privacy: `PrivateOverlay` / `ScopeRestricted` / `SecretCredential` per `#1792`. Backup: encrypted; must not broaden disclosure. Replication: bounded by privacy. Archive: per retention; honoring disclosure throughout. Restore authority: stricter than ordinary state; named explicitly. Export: per disclosure boundary; redaction may apply. Receipts: access produces an `AccessReceipt` (per `#1792`). |
| **Secret / key material** | (canonical / sealed, often outside ICN's direct storage) | Custody: secret. Locality: as narrow as the institution can tolerate. Privacy: `SecretCredential`. Backup: stricter than ordinary; rotation and recovery are governed. Replication: only via key-rotation / recovery protocols. Archive: only by explicit policy; usually rotated rather than archived. Restore authority: top-level governed events. Export: forbidden except by explicit policy. Receipts: rotation, recovery, revocation. |
| **Cache / derived state** | (`ServiceState` or `Blobs`; reconstructible) | Custody: derived. Locality: per the source. Privacy: inherits source. Backup: usually not required (regenerate from source). Replication: opportunistic. Archive: not applicable. Restore authority: trivial; rebuild from source. Export: not applicable. Receipts: not required. **Cache treated as canonical is a bug.** |

**Drift note (honest):** the seven-class custody taxonomy is broader than the three-variant kernel enum. They overlap but do not align 1:1 today. Reconciliation — whether to widen the kernel enum or to keep the kernel coarse and treat the seven classes as an app-layer projection — is forward work, tracked as a follow-up in this PR's handoff. This spec uses the **seven-class custody taxonomy** at design level and the **three-variant kernel enum** at kernel-enforcement level; the difference is explicit, not hand-waved.

## `StorageSpec`

`StorageSpec` is the binding between a workload, service, or domain object and the storage it touches. Today it does not exist as a Rust type; `storage-governance-spec.md` names it as a required follow-up. This spec defines its shape at design granularity.

**Identity and references**

- **Spec identifier.** A stable identifier for the storage binding inside the adopting domain.
- **Binding reference.** The `GovernedServiceBinding` (per `docs/spec/governed-service-binding.md`) that owns this storage spec, or the `InstitutionalDomain` that owns it directly for domain-level state.
- **Storage object reference.** What this spec governs: an artifact in the registry, a volume, a vault entry, a service state store. References to objects in `#1798` (`ArtifactRegistry`) and `#1767` (encrypted private-overlay store) when those land.

**Classification**

- **Custody class.** One of the seven design-level classes named above.
- **Kernel `StorageClass`.** One of `Canonical` / `ServiceState` / `Blobs` per `icn-kernel-api/src/storage.rs`.
- **`DataLocality`.** One of `CellLocal` / `CoopReplicated` / `FederationMirrored` / `CommonsPublic`. The spec's locality is the **upper bound** for replicas, backups, archives, and exports (see §"Locality and privacy inheritance").
- **Privacy / disclosure class.** Per the forthcoming `#1792` taxonomy (`Public` / `MembersOnly` / `ScopeRestricted` / `PrivateOverlay` / `SecretCredential` / `ExternalCustodian` / `SealedUntil`). The privacy class is the **upper bound** for all derived copies.

**Capacity and growth**

- **Capacity expectation.** Maximum size, growth rate envelope, and the threshold at which the domain expects to review.
- **Quota / allocation pointer.** The resource budget per the domain's compute placement policy (cross-link `#1801`).

**Policy references**

- **`BackupPolicy` reference.** Pointer to the adopted backup policy, or explicit acknowledgement that no backup applies (e.g., for `cache / derived state`).
- **`ReplicationPolicy` reference.** Pointer to the adopted replication policy, or explicit acknowledgement of `single-copy` for opportunistic state.
- **`RecoveryPolicy` reference.** Pointer to the adopted recovery policy.
- **`ArchivePolicy` reference.** Pointer to the adopted archive policy, where the class warrants long-term retention.
- **`IntegrityPolicy` reference.** Pointer to the adopted integrity policy.

**Lifecycle and exit**

- **Export / exit requirement.** What happens to this storage when the domain exits or the binding is removed (per `docs/spec/institutional-domain.md` §"Domain lifecycle" → Export).
- **Deletion / retention expectation.** Whether deletion is allowed, who authorizes it, what receipt class records it.

**Receipt expectations**

- **Per-policy receipt classes.** Each referenced policy names which receipt classes the chain emits at its lifecycle transitions (per `docs/spec/effect-dispatch-contract.md` §"Receipt class summary"). The spec carries those expectations forward; it does not invent new classes here.

**Invariants:**

- A workload's `WorkloadManifest` declared storage requirement; the `StorageSpec` is the actual grant. The spec never widens what the manifest declared.
- A storage binding without a custody class is rejected (per `storage-governance-spec.md`).
- A storage binding without a backup, replication, and exit policy is **incomplete**; the chain should refuse adoption until the gap is named (even explicitly to record that the class warrants none).
- `DataLocality` is upper-bound, not exact. A spec at `CoopReplicated` may host data in `CellLocal`; it may not host data in `FederationMirrored`.

## `BackupPolicy`

A backup is an institutional record of what was true at a moment, kept for the institution to recover from loss. Today `BackupPolicy` does not exist as a type. This spec defines its design shape.

**Frequency and schedule**

- **Cadence class.** Continuous (e.g., write-ahead log shadow), periodic (e.g., hourly / daily / weekly), governance-triggered (taken on demand under authority), or threshold-triggered (taken when state crosses a defined risk threshold).
- **Schedule reference.** Pointer to the per-domain operational schedule. The cadence is a class; the schedule is the concrete plan.

**Target**

- **Target locality.** One of `DataLocality` levels. **MUST be equal to or narrower than the source's `DataLocality`.** Backups never broaden locality (see §"Locality and privacy inheritance").
- **Target custody class.** Often the same as the source; sometimes narrower (e.g., a `Service state` backup may land in a `Scoped vault` if disclosure must tighten).
- **Storage location.** Reference to a physical or logical storage location; named in the per-domain operational plan.

**Privacy inheritance**

- **Privacy class.** **MUST be equal to or narrower than the source's privacy class.** Backup never broadens disclosure. Where the backup target reduces disclosure (e.g., scoped vault for what was members-only), the policy names the narrowing.
- **Encryption / key custody expectation.** For privacy classes that require encryption (`PrivateOverlay`, `SecretCredential`, `ExternalCustodian`), the policy names the key custody model. **Key custody specifics are deferred to `#1767`.** This spec names that backups inherit the source's encryption posture; it does not specify the cipher or key model.

**Retention**

- **Retention window.** How long the backup is kept before it expires or migrates to archive. Class options: short-horizon (operational rollback), institutional-horizon (months to a few years), archival (handed off to `ArchivePolicy`).
- **Cleanup / expiration authority.** Who authorizes deletion at expiry; what receipt class records the deletion.

**Integrity verification**

- **Integrity check.** How the backup proves it is what it claims to be (content hash, signed manifest). Cross-link `IntegrityPolicy` below.
- **Verification cadence.** How often the backup is re-verified.

**Restore-test cadence**

- **Drill frequency.** How often the institution proves it can restore (see §"Restore-test receipts").

**Authority**

- **Adoption authority.** Who can adopt or amend this policy (governance act per `docs/spec/ccl-policy-registry.md`).
- **Operational authority.** Who can execute the backup job under the policy. Operational backup is routine; deletion / restore is not.

**Receipt classes**

- Backup creation receipt.
- Backup verification receipt.
- Backup expiration / deletion receipt.
- Backup failure receipt (a failed backup is a governance-relevant event, not a silent log line).

**Hard rule:**

> A backup inherits the source's locality and disclosure rules unless a governance decision explicitly narrows access further. It MUST NOT broaden locality or disclosure.

## `ReplicationPolicy`

Replication keeps the service alive. It is *not* the same as backup; it shares only the verification surface with `IntegrityPolicy` and the anti-entropy proof loops tracked under `#1799`.

**Replica count and placement**

- **Minimum viable copies.** At design level, a class (e.g., `single-copy`, `dual-coop`, `quorum-of-3`, `federation-quorum`). Concrete numbers belong to the domain's operational adoption.
- **Placement constraints.** Diversity across cells, cooperatives, federations, or commons pools. The placement constraint inherits the spec's `DataLocality` upper bound.

**Convergence**

- **Anti-entropy expectation.** How replicas reconcile. Convergence semantics defer to `#1799` (network anti-entropy proof loops); this spec names that the policy declares an expectation, and the proof loops carry the verification surface.
- **Divergence detection.** How the policy detects that replicas have diverged. Cross-link `IntegrityPolicy` below.
- **Repair authority.** Who can authorize repair when replicas diverge in a way the proof loops cannot resolve automatically. Repair is a governed act for `Canonical` storage; routine for `ServiceState` per binding policy.

**Replica lifecycle authority**

- **Replica promotion / demotion.** When a replica becomes primary (e.g., for read load or failover) or steps down. For `Canonical` storage, promotion is institutionally significant and emits a receipt; for `ServiceState`, it may be routine.

**Receipt classes**

- Replication-completed receipt (per replica).
- Anti-entropy check receipt.
- Divergence-detected receipt.
- Repair receipt.

**Boundary:**

- **Replication is not backup.** A replica is a current copy; a backup is a record of past state. The two share a verification surface but answer different institutional questions.
- **Cross-reference:** `#1799` defines the substrate-side proof loops; this spec defines the institution-side policy that consumes them.

## `RecoveryPolicy`

Recovery is what the institution promises to itself when something goes wrong. The policy names what restoring authoritative state means and who can do it.

**Restore objective**

- **Restore objective class** (not SLA). Class options: best-effort (no commitment beyond "we try"), bounded (recovery within a named horizon), strict (recovery within a named horizon with named authority), disaster-only (recovery exercised only on declared incident). Concrete numerical objectives (RTO / RPO equivalents) belong to per-domain adoption.

**Authority**

- **Restore authority.** Which `AuthorityClass` (per ADR-0014) and which `Mandate` (per ADR-0019) covers a restore of this storage class. Restoring authoritative state is a governed event; the policy names which proposal class is required to authorize it.
- **Approval path.** How a restore is proposed, reviewed, decided, and dispatched. Maps onto the effect dispatch chain (`docs/spec/effect-dispatch-contract.md`) at each stage.
- **Witness authority.** Who validates that the restore matches the expected state. The witness need not be the actor that proposed or executed the restore; separation of duties is the institutional safeguard.

**Restore drills**

- **Drill cadence.** How often the institution proves it can restore without waiting for an incident.
- **Drill scope.** Test scope: full / partial / point-in-time. Drill targets a non-authoritative environment by default.

**Restore-target constraints**

- **Target class.** Where a restore is allowed to land — staging, sandbox, production. A drill that restores into the production environment is a `live restore`, not a `test`; the policy names which authority covers each.
- **Locality and disclosure.** The restore target inherits the source's locality and disclosure constraints (see §"Locality and privacy inheritance").

**Restore type**

- **Full restore.** Authoritative state replaced from backup. Always governed.
- **Partial restore.** A subset of state restored. Always governed; the policy names which subsets are allowed.
- **Point-in-time restore.** Restore to a named historical state. Always governed; the policy names which historical horizons are accessible.

**Challenge, reversal, audit**

- **Challenge path.** A restore can be challenged. Challenge is itself a governance act with its own receipt (per `docs/spec/effect-dispatch-contract.md` §"Challenge / reversal / counter-receipt").
- **Reversal.** Reversal of a restore is a separate governance act, not a retroactive edit. The audit log is append-only.

**Receipt classes**

- Restore-test receipt (drill; see §"Restore-test receipts" below).
- Restore-execution receipt (live restore).
- Restore-failure receipt.
- Restore-challenge receipt.

**Hard rule:**

> Restoring authoritative state is a governed event. It MUST produce receipts and preserve provenance.

## `ArchivePolicy`

Archives keep the institution accountable to itself and to its partners. An archive is not a backup; a backup may eventually become an archive when retention crosses an archival horizon.

**Retention**

- **Long-term retention class.** Decades-horizon, generations-horizon, indefinite. The policy names the class; the concrete horizon belongs to per-domain adoption.
- **Immutability expectation.** Archives are write-once at the policy level. The implementation may use append-only storage, content-addressed snapshots, or external custodians; the policy names the expectation, not the mechanism.

**Access**

- **Access path.** Who can read the archive, under what authority, with what receipt.
- **Redaction.** Per `#1792`, archived material that contains private data may require redaction on access. The policy names the redaction class.
- **Institutional hold.** Where the institution has a legal or governance obligation to preserve specific records past their default expiry, the policy names the hold class. **The detailed hold protocol is deferred to a follow-up issue** (drafted in this PR's handoff).

**Verification**

- **Archive verification cadence.** How often the archive proves its content has not silently rotted.
- **Integrity proof.** Cross-link `IntegrityPolicy` below.

**Export**

- **Archive export.** Conditions under which archive material may be exported to another institution or external custodian. Always governed.

**Deletion / expiration**

- **Expiration authority.** Who can authorize deletion at the archival horizon. The default is **never** for `Canonical` storage; archives that hold canonical state do not expire silently.
- **Receipt of expiration.** A deliberate, governed expiration produces a receipt naming what was retired, under whose authority, against which retention class.

**Receipt classes**

- Archive-creation receipt.
- Archive-verification receipt.
- Archive-access receipt (per `#1792` access-receipt pattern).
- Archive-export receipt.
- Archive-expiration receipt.

**Hard rule:**

> Archives keep the institution accountable. `ArchivePolicy` MUST NOT become a quiet deletion policy. Expiration of archival material is a governance act with its own receipt.

## `IntegrityPolicy`

Integrity is the institution's assertion that storage has not silently changed. The policy names how that assertion is produced and how often.

**Verification cadence**

- **Verification frequency.** Continuous (cryptographic structure), periodic (scheduled check), event-driven (on access or transition), drill (during restore tests).

**Proof shape**

- **Hash / proof vocabulary.** Where the canon already names a hash type (e.g., `blake3` content hashes per `icn-ccl/src/registry.rs`, Merkle proofs per `icn-store/src/lib.rs`), the policy reuses it. **The policy does not introduce new hash algorithms or proof formats.**
- **Content / proof reference.** What the integrity check records: content hash, signed manifest, Merkle root, or a combination per the underlying storage type.

**Repair path**

- **On failure.** What happens when integrity verification fails. For `Canonical` storage: a governed event with a named repair authority. For `ServiceState` or `Cache`: per-binding policy; often a re-replicate or rebuild from source.
- **Evidence class.** A failed integrity check is itself evidence (per `docs/spec/effect-dispatch-contract.md` §"Stage 5: Application + evidence"). It is institutional information, not a hidden log line.

**Relationships**

- **To `ReplicationPolicy`.** Replication divergence is one source of integrity verification failures; the policies share the substrate-side proof loops in `#1799`.
- **To `RecoveryPolicy`.** Restore tests are integrity events; restore-test receipts include the integrity verification outcome.
- **To anti-entropy** (`#1799`). The substrate provides the verification surface; `IntegrityPolicy` is the institutional consumer.

**Receipt classes**

- Integrity-check receipt (passed).
- Integrity-failure receipt.
- Integrity-repair receipt.

## Restore-test receipts

Restore-test receipts deserve their own section. A backup that has never been restored is hope, not durability.

**What a restore-test is**

A restore-test is a deliberate, scheduled or governance-triggered drill that exercises the recovery path without putting authoritative state at risk. The drill produces a receipt naming what was attempted, where it landed, who authorized it, and what was verified.

**Triggers**

- **Scheduled drill.** Per the cadence named in `RecoveryPolicy`. The default is "at least annually, more often for `Canonical` storage."
- **Governance-triggered drill.** A proposal accepted under the domain's authority for the storage class. Useful when policy changes, when a major incident has been resolved, or before adopting a new backup target.
- **Incident-aftermath drill.** Following a real incident, to confirm the recovery path is intact.

**Constraints**

- **Test scope.** Drills restore into a **non-authoritative** environment by default. A drill that restores into production is a `live restore`, not a `test`, and requires the strict authority class for that storage.
- **No silent mutation.** Test restores MUST NOT mutate authoritative state. Where a drill does require touching authoritative state (e.g., a destructive recovery rehearsal), the policy names the proposal class that authorizes it.

**What the receipt records**

The restore-test receipt MUST name:

- What was restored (source `StorageSpec`, version / point-in-time, content hash where applicable).
- Where it was restored to (target environment, locality, custody class, privacy class).
- Under whose authority (mandate reference, `GovernanceDecisionReceipt` reference for governance-triggered drills).
- What was verified (integrity check outcome, content equivalence, schema compatibility for typed state).
- What failed (per check; not a single boolean).
- Convergence time, where the drill measured it.

**Failure**

> A failed restore test is not a warning to hide. It is a governance-relevant event.

The failure produces a receipt with `success = false`. The steward cockpit (`#1795`) surfaces it. Where the failure indicates a real recoverability gap, the policy names which proposal class is required to address it.

**Member visibility**

For storage classes whose loss would affect members directly (canonical state, scoped vaults), the restore-test result is summarized for affected members at a member-shell-appropriate cadence (per `#1818`). The summary does not expose the underlying data; it asserts the institution's recovery posture.

## Locality and privacy inheritance

The single most load-bearing rule in this spec. Stated three ways for emphasis.

> **Note on scope vocabulary.** Generic scope language in this spec uses `LocalDomain` / `InstitutionalDomain` / `Domain` (per [`../architecture/INSTITUTION_PACKAGE_BOUNDARY.md`](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md) §C3 "Entity-scope vocabulary"). The kernel-level `DataLocality::CoopReplicated` enum variant is preserved as the existing serialized Rust identifier; new generic scope concepts that this spec introduces use the local-domain vocabulary rather than coining new `Coop-`-prefixed names. Rename of the kernel-level variant is forward work (compatibility-aware migration; not in this spec's scope).

**Inheritance rule:**

> Backups, replicas, archives, exports, and restore targets inherit the source's `DataLocality` and privacy / disclosure constraints.
>
> Policy MAY narrow disclosure (e.g., a `MembersOnly` source backed up into a `ScopeRestricted` target).
>
> Policy MAY NOT broaden locality or disclosure without an explicit governance act that names the broadening.

**Why:**

- A backup that lands in a wider locality than the source effectively re-publishes data the source's authority did not authorize.
- A replica in a wider locality is institutional disclosure by the back door.
- An archive in a wider disclosure class is silent declassification.
- An export to an external custodian is governed precisely because it crosses the boundary.

**Per-class application:**

- **Scoped vault / private overlay storage** (`#1767`). Backups and replicas remain encrypted; access requires the same authority as access to the source; export requires explicit governance.
- **Secret / key material.** Backups exist only via key rotation or recovery protocols; ordinary backup mechanisms MUST NOT copy secret material. (See §"Failure and safety rules": "Secret material copied into ordinary backup.")
- **Cache / derived state.** Backup is usually neither required nor appropriate; the source is reconstructible. Where a derived cache *is* backed up (e.g., to accelerate recovery), the backup inherits the privacy of the source it was derived from.

**Cross-link:**

- `#1767` — encrypted private-overlay storage, including key custody for backups of vault material.
- `#1792` — disclosure boundary and `AccessReceipt` class for backup access.

## Backup / export / restore authority

The authority rules tie this spec to ADR-0014 (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`) and to `docs/spec/effect-dispatch-contract.md`.

**Policy adoption and amendment**

- Adopting or amending `BackupPolicy`, `ReplicationPolicy`, `RecoveryPolicy`, `ArchivePolicy`, `IntegrityPolicy` is a governance act per `docs/spec/ccl-policy-registry.md` §"Adoption contract." The receipt chain is the dispatch chain.
- Unadopted policy is inert; the domain runs against its currently adopted versions.

**Operational backup execution**

- Routine backup execution may be operational, not governed per backup. The domain's operational backup operator runs the schedule the policy authorizes; the policy is the standing authority.
- Every backup execution emits evidence regardless. A backup with no evidence is institutional opacity.

**Restore of authoritative state**

- Restoring `Canonical` storage MUST be governed per restore. The proposal class is named in `RecoveryPolicy`; the dispatch chain runs end-to-end; the restore-execution receipt is the institutional record.
- Restoring `ServiceState` may be per-binding; not every routine rollback needs a full proposal.

**Export and exit**

- Export of canonical or vault material to another institution requires either a federation agreement (per `institutional-domain.md` §"Federation") or the domain's explicit exit policy.
- Exit is constitutional. A domain may always leave; the exit path produces export receipts and custody handoffs.

**Emergency authority**

- Where `RecoveryPolicy` permits emergency restore (e.g., to recover from a disaster faster than the ordinary proposal class allows), the emergency authority MUST be: bounded in time, named in the policy, reviewable after the fact, and receipt-backed. No emergency-restore authority is silent.

**Failure visibility**

- Failed backup integrity, failed restore tests, divergent replicas, missing archives — all surface in the steward cockpit (`#1795`). They are first-class institutional facts, not buried operational alerts.

## Relationship to sibling work

Cross-link, do not duplicate.

| Concern | Where it lives |
|---|---|
| Governed service binding, workload manifest, runtime provider | `docs/spec/governed-service-binding.md` (`#1815`, merged via PR #1822). |
| `InstitutionalDomain` and `DomainPolicy` (storage / backup / recovery / archive defaults) | `docs/spec/institutional-domain.md` (`#1794`, merged via PR #1820). |
| Effect dispatch contract (Stage 5 application + evidence; disclosure inheritance) | `docs/spec/effect-dispatch-contract.md` (`#1797`, merged via PR #1819). |
| CCL policy registry, adoption contract (policy adoption produces receipts) | `docs/spec/ccl-policy-registry.md` (`#1817`, merged via PR #1821). |
| Storage classes (`StorageClass`, `DataLocality`) and access enforcement | `docs/state/storage-governance-spec.md` + `icn/crates/icn-kernel-api/src/storage.rs`. |
| Integrated cooperative operating model (storage as custody + backup doctrine) | `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` (`#1793`, merged via PR #1814). |
| `ArtifactRegistry` v0 and `ScopedVault` boundary | `#1798`. |
| Encrypted distributed private-overlay storage (key custody for vault backups) | `#1767`. |
| Private data disclosure boundary (`PrivacyClass`, `DisclosurePolicy`, `AccessReceipt`) | `#1792`. |
| Network anti-entropy proof loops (verification surface for replication / integrity) | `#1799`. |
| Compute placement (resource allocation for storage-providing workloads) | `#1801`. |
| Member shell v0 (member-facing restore-test summary) | `#1818`. |
| Steward cockpit v0 (operator-facing failure surface) | `#1795`. |
| Receipt and provenance proof envelope (existing receipt layer model) | ADR-0026. |
| Constitutional object model (`AuthorityClass`, `AuthorityGrant`, `Mandate`) | ADR-0014. |
| Authority Grant Minting and Mandate Persistence Seam | ADR-0019. |

## Failure and safety rules

Each failure mode has a deterministic response. Cross-cutting rule: **fail closed, surface honestly, never silently fall back.**

| Failure | Response |
|---|---|
| **Missing `StorageSpec`** (a binding declares storage but no spec is bound). | Adoption MUST fail. The domain cannot govern what it has not classified. |
| **Storage binding without a custody class.** | Refused at adoption (per `storage-governance-spec.md`). |
| **`BackupPolicy` missing for non-discardable state.** | Adoption MUST fail unless the spec explicitly declares the storage class is discardable (e.g., cache). Silence is not "no backup needed"; absence is a contract gap. |
| **Backup crosses locality boundary** (target locality wider than source). | Backup refused. The runtime provider rejects; the failure is recorded as institutional evidence. |
| **Replica violates locality / privacy policy.** | Replica placement refused. The provider rejects; the steward cockpit surfaces the gap. |
| **Restore without authority.** | Restore refused. Where the attempt mutates state, the mutation MUST NOT proceed. The attempt is recorded with the would-be actor and the missing authority. |
| **Restore drill never run.** | The steward cockpit MUST surface the gap. After a policy-defined horizon, the binding is at-risk and the domain's emergency-review path applies. |
| **Archive quietly deleted.** | Forbidden. Archival material expires only via the governed-expiration path with a named authority and an expiration receipt. Any deletion without those records is a bug and a governance-relevant incident. |
| **Integrity check fails.** | The policy's repair path applies. The failure produces a receipt with `success = false`; the steward cockpit surfaces it; for `Canonical` storage, the repair authority is named and emergency review applies. |
| **Anti-entropy divergence found.** | Per `#1799`'s proof loops + `ReplicationPolicy.Repair authority`. For `Canonical` storage, the divergence is a governance-relevant event; for routine `ServiceState`, it may be auto-repairable. |
| **Cache treated as canonical.** | The class mismatch is a bug. The spec MUST refuse a `StorageSpec` that names cache custody but operates on `Canonical` storage class. |
| **Secret material copied into ordinary backup.** | The backup path MUST refuse. Secret material follows key-rotation / recovery protocols (`#1767`), not ordinary backups. |
| **Export path missing** for a binding that may exit. | Adoption MUST fail. A binding without an export path is not safely adoptable. |
| **Recovery objective overclaimed as SLA.** | The policy names *classes*, not numerical SLAs. A policy that asserts a numerical commitment without naming the operational mechanism that delivers it is incomplete; review MUST surface the gap. |
| **Provider unavailable** (storage backend or backup target down). | Spec stays valid; the workload may suspend per the governed-service-binding lifecycle. Operator must restore or governance must amend the policy. |
| **Stale policy reference** (spec points at a deprecated `BackupPolicy` version). | The domain MUST amend its `DomainPolicy` via a governance act before evaluation continues. References pointing at deprecated versions cause selection to fail closed (mirrors `ccl-policy-registry.md` §"Failure / safety rules"). |

## Open questions and deferred decisions

Each is named so future readers know where this spec stops.

| Question | Deferred to |
|---|---|
| Wire-stable schema for `StorageSpec`, `BackupPolicy`, `ReplicationPolicy`, `RecoveryPolicy`, `ArchivePolicy`, `IntegrityPolicy` records | Follow-up `schema(storage): …` issue drafted in this PR's handoff. |
| Restore-test receipt envelope (Rust type, persistence, audit-query surface) | Follow-up `spec(storage): define restore-test receipt envelope` drafted in this PR's handoff. |
| Locality / privacy inheritance check (the `validate_storage_access()`-style enforcement function named as TBD in `storage-governance-spec.md`) | Follow-up `spec(storage): define locality/privacy inheritance checks`. |
| Archive verification and export contract (the per-archive proof shape, hand-off protocol, custodian agreement) | Follow-up `spec(storage): define archive verification and export contract`. |
| Backup provider / runtime provider interface (how a `BackupPolicy` is executed by a `RuntimeProvider` per `#1815`) | Follow-up `spec(storage): define backup provider / runtime provider interface`. |
| Connection between `IntegrityPolicy` receipts and `#1799`'s anti-entropy proof loops | Follow-up `spec(anti-entropy): connect IntegrityPolicy receipts to network proof loops`. |
| Institutional hold protocol (legal / institutional holds beyond default expiry) | Forward; named as a hold class in `ArchivePolicy` above but the protocol is deferred. |
| Reconciliation of the 7-class custody taxonomy with the 3-variant kernel `StorageClass` enum | Forward; both stand today, named explicitly in §"Storage class model." |
| Concrete numerical objectives for per-domain `RecoveryPolicy` adoption | Domain-side, not spec-side. Each domain's `DomainPolicy` adoption names the concrete RTO/RPO-equivalents. |
| Federation-side recognition of one domain's backup / archive material | Federation agreement (per `institutional-domain.md` §"Agreement registry"); not in this spec. |
| Kernel-side enforcement of inheritance rules beyond what `StorageValidationError` already covers | Future ADR / extension to `icn-kernel-api/src/storage.rs`. |

## Non-claims (repeated for clarity)

- This document does not introduce any of the forward-direction policy objects it names. They remain forward-direction.
- It does not introduce schema, wire format, or contract changes.
- It does not redefine `StorageClass` (3-variant kernel enum) or `DataLocality` (4-level kernel enum). Both remain canonical per `icn/crates/icn-kernel-api/src/storage.rs`.
- It does not commit to a storage backend, a replication algorithm, a backup vendor, or an SLA numerical commitment.
- It does not authorize mutation of any deployed infrastructure (DNS, K3s, Forgejo, identity-bridge, gateway, backup jobs).
- It does not claim production readiness.
- It does not claim live inter-cooperative federation.
- It does not claim any partner federation is operating under these primitives today. NYCN appears nowhere in the spec; institution-package vocabulary stays in institution packages.
- It does not use payment, wallet, balance, currency, or blockchain as ICN-native framing. The discipline list in `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` §"Vocabulary discipline" applies in full.
- It does not by itself close `#1816`. The PR uses `Refs:`.

## Related documents and issues

Architecture and spine:

- `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` — §"Storage as custody" and §"Backup, redundancy, recovery, archive."
- `docs/architecture/KERNEL_APP_SEPARATION.md` — meaning firewall; the kernel never imports policy objects.
- `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` — generic shapes vs. package vocabulary.
- `docs/architecture/SERVICE_HOSTING_MODEL.md` — hosted → governed → native; storage durability is a binding-state requirement.
- `docs/architecture/COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` — design-direction overview.

Specs (siblings and parent):

- `docs/spec/institutional-domain.md` — `DomainPolicy` adopts the policy objects this spec defines.
- `docs/spec/effect-dispatch-contract.md` — Stage 5 receipts; disclosure inheritance.
- `docs/spec/ccl-policy-registry.md` — policy adoption is a CCL-evaluated governance act.
- `docs/spec/governed-service-binding.md` — `StorageSpec` and `BackupPolicy` reference live inside the binding.
- `docs/spec/KERNEL_CONTRACTS.md` — kernel primitive contracts.

State and design:

- `docs/state/storage-governance-spec.md` — `StorageClass`, `DataLocality`, `StorageSpec` (named as TBD), `validate_storage_access()` (named as TBD).
- `docs/design/compute-classes.md` — runtime class taxonomy that workloads consume storage under.
- `docs/design/deterministic-core.md` — determinism for legitimacy-compute workloads that read or write `Canonical` storage.

ADRs:

- ADR-0014 — Constitutional Object Model (authority primitives referenced by `RecoveryPolicy`).
- ADR-0019 — Authority Grant Minting and Mandate Persistence Seam.
- ADR-0026 — Receipt and Provenance Proof Envelope (existing receipt-layer model).
- ADR-0008 — Receipt-chain vertical slice and `audit verify` CLI.

Code surface (existing types this spec references):

- `icn/crates/icn-kernel-api/src/storage.rs` — `StorageClass` (line 53), `DataLocality` (line 155), `StorageValidationError` (line 272).
- `icn/crates/icn-store/src/blob_store.rs` — hybrid blob store (sled metadata + filesystem blobs).
- `icn/crates/icn-store/src/lib.rs` — persistent key-value abstraction; re-exports `StorageChallenge`, `MerkleProof`, `PeerCache`, `MaintenanceManager`.
- `icn/crates/icn-store/src/pos.rs` — proof-of-storage challenge system.
- `icn/crates/icn-snapshot/src/lib.rs` — state snapshot (Chandy-Lamport) for graceful restarts.
- `icn/crates/icn-governance/src/proof.rs` — `GovernanceProof`, `GovernanceDecisionReceipt`, `GovernanceDecisionAttestation`.

Issues:

- `#1816` — this spec.
- `#1793` — integrated operating model (merged via PR #1814).
- `#1794` — institutional domain (merged via PR #1820).
- `#1797` — effect dispatch contract (merged via PR #1819).
- `#1815` — governed service binding (merged via PR #1822).
- `#1817` — CCL policy registry (merged via PR #1821).
- `#1798` — `ArtifactRegistry` v0 and `ScopedVault` boundary.
- `#1767` — encrypted distributed private-overlay storage.
- `#1792` — private data disclosure boundary.
- `#1799` — network routing, redundancy, and anti-entropy proof loops.
- `#1801` — compute placement.
- `#1818` — member shell v0.
- `#1795` — steward cockpit.
- `#1748` — institutional process substrate.
