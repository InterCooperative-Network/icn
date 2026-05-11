---
Status: draft
Canonical: no
Last Reviewed: 2026-05-11
---

# Foundational Manual: Deterministic Boundary Types

This note captures the current drafting doctrine for the deterministic boundary crate used by the first executable baseline-lock loop.

It is not a production-readiness claim.

It is not a Phase 2 completion claim.

It is not formal partner authorization.

It is not an implementation issue.

It complements:

- [`FOUNDATIONAL_MANUAL_EXECUTABLE_BASELINE_LOOP.md`](FOUNDATIONAL_MANUAL_EXECUTABLE_BASELINE_LOOP.md)
- [`FOUNDATIONAL_MANUAL_BASELINE_LOCK.md`](FOUNDATIONAL_MANUAL_BASELINE_LOCK.md)
- [`FOUNDATIONAL_MANUAL_STATE_PROJECTION_AND_COMPACTION.md`](FOUNDATIONAL_MANUAL_STATE_PROJECTION_AND_COMPACTION.md)
- [`FOUNDATIONAL_MANUAL_ECONOMIC_STATE_RESOLUTION.md`](FOUNDATIONAL_MANUAL_ECONOMIC_STATE_RESOLUTION.md)

The boundary crate is the first code seam for the proposed PR:

```text
test(runtime): add executable baseline-lock loop fixture
```

## Core doctrine

The boundary crate defines only the data that is allowed to cross the Meaning Firewall between the Rust authority-bearing host and the WASM constrained-execution guest.

It must be shared by host and guest.

It must be deterministic.

It must be boring.

It must not contain rich institutional meaning.

It must not import runtime state.

It must not expose database handles.

It must not expose host callbacks.

It must not contain floats.

It must not contain unordered maps.

It must not know cooperatives, democracy, money, membership, roles, or personhood.

The boundary crate is where the Meaning Firewall becomes type checking.

## Candidate crate

Candidate name:

```text
icn-boundary
```

Candidate responsibility:

```text
Hash
ExecutionInputEnvelopeV1
CanonicalFacts
ExecutionOutputEnvelopeV1
DeterminismClass
PrivacyClass
FinalityClass
ResultKind
```

## Boundary structs

Initial shape:

```rust
use serde::{Deserialize, Serialize};

/// A strict 32-byte array to ensure content-addressable hashes
/// are passed deterministically across the ABI boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hash(pub [u8; 32]);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionInputEnvelopeV1 {
    pub abi_version: u32,
    pub schema_id: String,
    pub workload_id: String,
    pub module_hash: Hash,
    pub process_id: String,
    pub target_ref: String,

    // Hashes proving the host sealed the causal frontier.
    pub state_resolution_capsule_hash: Hash,
    pub canonical_fact_snapshot_hash: Hash,
    pub authority_context_hash: Hash,
    pub standing_context_hash: Hash,
    pub agreement_context_hash: Hash,

    pub mandate_ref: Option<String>,
    pub rule_ref: String,
    pub determinism_class: DeterminismClass,
    pub privacy_class: PrivacyClass,
    pub finality_class: FinalityClass,
    pub fuel_limit: u64,
    pub input_artifact_refs: Vec<Hash>,

    // The only part of reality the WASM guest is allowed to see.
    pub canonical_facts: CanonicalFacts,
    pub expected_output_schema: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalFacts {
    pub eligible_voters: u32,
    pub votes_cast: u32,
    pub approvals: u32,
    pub rejections: u32,
    pub abstentions: u32,
    pub required_approvals: u32,
    pub notice_delivered: bool,
    pub allocation_requested: u64,
    pub allocation_limit: u64,
    pub reservation_not_consumed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionOutputEnvelopeV1 {
    pub abi_version: u32,
    pub schema_id: String,
    pub workload_id: String,
    pub process_id: String,
    pub result_kind: ResultKind,
    pub passed: bool,
    pub output_artifact_refs: Vec<Hash>,
    pub diagnostics: Vec<String>,

    // A proposed geometry, not an executed state transition.
    pub proposed_transition_ref: Option<Hash>,
    pub consumed_fuel: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeterminismClass {
    Strict,
    Advisory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyClass {
    Public,
    EncryptedOverlay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinalityClass {
    InstitutionLocal,
    FederationPending,
    BridgeExecuted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultKind {
    GateValidation,
    MutationProposal,
}
```

## Serialization rule

Governance-grade envelope serialization must be deterministic.

Do not use ad hoc JSON.

Do not rely on default serialization settings that may drift across crate versions.

Use a pinned canonical binary configuration.

The rule:

> Same envelope fields plus same serialization version plus same module hash must produce the same bytes and the same hash.

Required constraints:

```text
pinned crate version
explicit encoding configuration
no unordered maps
no f32/f64
stable field ordering
versioned schema ids
fixture tests that assert stable hashes
```

The host serializes:

```text
ExecutionInputEnvelopeV1 -> input_bytes
```

The guest deserializes:

```text
input_bytes -> ExecutionInputEnvelopeV1
```

The guest serializes:

```text
ExecutionOutputEnvelopeV1 -> output_bytes
```

The host deserializes and validates:

```text
output_bytes -> ExecutionOutputEnvelopeV1
```

This physically enforces:

```text
evaluate(input_bytes) -> output_bytes
```

## Fail-closed rule

If the WASM guest needs information that is not inside `CanonicalFacts`, it cannot reach out and get it.

It cannot query the network.

It cannot query the receipt DAG.

It cannot ask the host whether someone has authority.

It cannot write a receipt.

It fails closed.

## Build direction

After the boundary crate exists, build the WASM validation guest next.

The guest is smaller than the host orchestrator and proves the negative shape of the Meaning Firewall first.

The first guest should do only this:

```text
deserialize ExecutionInputEnvelopeV1
check approvals >= required_approvals
check notice_delivered == true
check allocation_requested <= allocation_limit
check reservation_not_consumed == true
serialize ExecutionOutputEnvelopeV1
```

No WASI.

No host imports.

No filesystem.

No network.

No clock.

No randomness.

No authority.

Then build the host orchestrator around that inert guest.

## Closing doctrine

The boundary crate is not plumbing.

It is constitutional machinery.

The guest only sees facts.

The host owns authority.

The institution supplies meaning.

If it is not in the envelope, the guest cannot know it.

That is the firewall.
