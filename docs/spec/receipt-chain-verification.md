---
Status: normative (design contract; implemented + locally tested on branch task/gap-atlas-20260717, NOT merged)
Canonical: no
Current-state source: docs/STATE.md + docs/PHASE_PROGRESS.md
Last Reviewed: 2026-07-17
---

# Receipt-chain verification contract (ADR-0026 re-verifiability, made enforceable)

> **Implementation status (2026-07-17).** The core of this contract is
> **implemented and unit/integration-tested** on branch `task/gap-atlas-20260717`
> (uncommitted, not merged): `icn-governance::verify` (integrity/authenticity
> recompute over `GovernanceDecisionReceipt`/`GovernanceProof`, fail-closed
> `Pass`/`Fail`/`Unresolved`/`NotApplicable` folding, chain-link and collision
> checks), consumed by a rewritten `icnctl audit verify` that recomputes the
> decision hash from content instead of comparing hash strings. Per-ladder-class
> field recompute (activation → mutation-plan → mutation-applied) and read-path
> re-verification inside the gateway store remain follow-on. This proves
> **integrity**, not authorization or legitimacy.

> **What this document is.** A design contract for a *mechanical, offline
> re-verifier* of ICN receipt chains. It writes down what "verify a receipt
> chain" must actually check, so that the **re-verifiability invariant already
> asserted by [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md)
> becomes falsifiable** rather than merely stated.
>
> **What this document is NOT.** It is not an implementation, not a claim that
> re-verification is implemented today, and it changes no runtime behavior. It
> lands no code. It does not modify the receipt schema, the receipt store, or
> any auth/write path. Current implementation status lives only in
> [`../STATE.md`](../STATE.md).

## Problem (evidence, not assertion)

ADR-0026 records four invariants for the receipt-and-provenance envelope; two of
them are the entire value proposition:

- **Immutable** — "Once a layer's record is signed and persisted, it is not
  edited. Reversal is a counter-record, not a mutation."
- **Re-verifiable** — "Anyone with the public keys can verify the signature
  chain. Vote sets are merkle-rooted; proof hashes bind the set."

The *primitives* to enforce re-verifiability exist and are unit-tested.
`icn-governance::proof` exposes `compute_vote_hash`, `verify_signature`, ~14
per-class `compute_record_hash(fields…)` associated functions, and one
`verify_binding(&self)` (on the legacy `GovernanceProof` type); a separate
`verify_binding` family lives in `icn-kernel-api::proofs`. The mutation rung is
genuinely hash-chained (`ActivationCrossedReceipt.decision_record_hash` →
`MutationPlanRecordedReceipt.activation_record_hash` →
`MutationAppliedReceipt.plan_record_hash` → `EvidencePacket…`). (Note: the
Layer-2 ladder classes expose `compute_record_hash(fields…)` as an *associated*
function, not a `&self` `verify_binding`/`verify` — see "Where it belongs".)

But **the store/read/audit path never re-verifies.** As of `b44a1821`
(2026-07-17), there is exactly one runtime re-verification anywhere, and it is
not on the path that matters here: the gossip *ingest* handler
(`apps/governance/src/actor.rs::handle_incoming`) calls `verify_binding()` +
`verify_signature()` on an incoming **legacy Layer-1 `GovernanceProof`** to
reject bad remote proofs. Nothing re-verifies a **stored Layer-2 receipt** on
read, audit, or export:

1. No component recomputes a stored receipt's hash or checks its signature when
   reading it back. The only re-verification (above) is at gossip ingest of the
   legacy Layer-1 proof, not on the store/audit/export path this contract
   covers.
2. The gateway receipt store hydrates receipts with
   `serde_json::from_slice(&bytes)` and **no read-side hash recomputation**
   (`icn-gateway/src/receipt_store.rs`). Persistence is trust-on-write: the
   store guarantees write-once uniqueness and a `(class, record_hash)` bind
   collision sentinel, but it never re-derives the hash from the payload on read.
3. `icnctl audit verify` (`bins/icnctl/src/main.rs`, `verify_receipt_chain`)
   performs **string-equality checks only** — e.g. `chain.decision_hash ==
   decision_hash`, "every allocation references the correct decision hash",
   "every intent hash is claimed by an allocation". It never recomputes a
   record hash from the receipt's canonical fields and never verifies a
   signature.

**Consequence:** a receipt whose stored `record_hash` field is internally
consistent with its sibling references, but whose *content has been altered*,
passes every check the system currently runs. The invariant ADR-0026 promises
("re-verifiable", "immutable") is not mechanically defended anywhere. This is a
credibility hole at the exact center of the value proposition — the appliance
evidence-export witness, a steward's audit, and any future federated peer all
inherit it.

This document is the missing **Layer 4 re-verification contract**. ADR-0026 §
Layer 4 (`ProvenanceQuery`, candidate ADR-0072) is about *discovery/query*
("holder-queryable"); this contract is the orthogonal *re-verification* half
("re-verifiable"), which ADR-0026 asserts as an invariant but leaves to no
component.

## Scope

In scope: a pure, side-effect-free verifier that takes a set of already-serialized
receipts (from the store, from an evidence export, or from a federated peer) and
returns a structured verdict. It reuses the existing `proof.rs` hashing and
signature functions — it does **not** re-implement the BLAKE3 domain-tagged
encoding (re-implementation would risk divergence from the canonical algorithm
and is explicitly forbidden here).

Out of scope: the query/index surface (ADR-0026 Layer 4 / candidate ADR-0072);
any new receipt class; any change to how receipts are written; cross-node
transport; and challenge/reversal semantics (those are a separate gap — see
Related work).

## The verification contract

A verifier over a receipt set MUST perform, and report per-receipt and
per-chain, all of the following. Every check fails closed: an unknown class, a
missing field, a decode error, or an unresolvable reference is a FAIL, never a
skip-and-pass.

### V1 — Record-hash re-derivation (per receipt)

For each receipt, recompute its `record_hash` (or class-specific proof hash)
from its canonical fields and compare to the stored value. For the Layer-2
ladder classes, this means reading the receipt's public fields and passing them,
in canonical order, into that class's `compute_record_hash(fields…)` associated
function (these classes expose no `&self` `verify_binding`); for the legacy
`GovernanceProof`, `verify_binding(&self)` may be called directly.

- PASS iff recomputed == stored.
- A byte-altered field that does not also (impossibly, without the key) update a
  signed hash MUST produce a FAIL here. **This is the falsification test for the
  whole contract** (see Acceptance).

### V2 — Signature verification (per receipt, where a signature is present)

Where a receipt carries a signature and the signer's verifying key is resolvable
(via the DID document / identity manager), verify it with `verify_signature`.

- PASS iff the signature verifies against the recomputed hash and the resolved
  key.
- Key not resolvable → report `UNRESOLVED_KEY` (a distinct, non-PASS verdict —
  the verifier states what it could not check rather than silently passing).

### V3 — Chain-link continuity (per chain)

For the hash-chained ladder, verify each rung's back-reference equals the prior
rung's recomputed `record_hash` (activation→decision, plan→activation,
applied→plan, evidence→applied). For string-id-linked receipts (the non-ladder
classes), verify referential integrity (every referenced id resolves within the
set or is explicitly marked external).

- PASS iff every link resolves to a V1-verified record.
- A rung pointing at a hash no receipt in the set produces → FAIL
  (`BROKEN_LINK`), not a silent gap.

### V4 — Merkle vote-root (where applicable)

For receipts binding a vote set, recompute the merkle/`compute_vote_hash` root
and compare to the bound value.

- PASS iff recomputed root == bound root.

### V5 — Immutability cross-check (per store or export)

Confirm the ADR-0026 immutability invariant structurally: no two distinct
payloads share a `(class, record_hash)` bind (this is what the store's collision
sentinel enforces at write time; the verifier confirms it at read time over the
whole set), and any "reversal" is a *new* receipt referencing the original, never
an edit of it.

### Verdict shape

The verifier returns a structured result, not a boolean: per-receipt
`{class, record_hash, v1, v2, v3, v4}` each ∈ `{PASS, FAIL, UNRESOLVED, N/A}`,
plus a chain-level roll-up and a top-level `verified: bool` that is true iff
every applicable check is PASS and none is FAIL. `UNRESOLVED` (e.g. a key the
verifier does not hold) makes `verified` false but is reported distinctly from
`FAIL` — an honest verifier says "I could not check X", never "X is fine."

## Where it belongs

- **Verifier core:** `icn-governance` (a `verify` module beside `proof.rs`) or a
  thin new module that depends only on `icn-governance::proof` +
  `icn-identity` for key resolution. It is pure and takes no `Store` — it
  operates on already-hydrated receipts, so it is equally usable over a live
  store, an evidence export, or a peer's bytes. **No kernel expansion**: this is
  app-layer verification of app-layer receipts; the kernel already stores them
  opaquely and is not involved.
- **CLI consumer:** replace the string-equality body of `icnctl audit verify`
  (`verify_receipt_chain`) with a call into the verifier core, so the existing
  command becomes a real re-verifier. This is the primary, already-existing
  consumer named by ADR-0008 ("Receipt Chain Vertical Slice and Audit Verify").
- **Evidence-export consumer (follow-on):** the rehearsal evidence-export
  validators (`docs/scripts/validate-rehearsal-evidence.py` and the `:v1` packet
  path) can call the same verifier so an exported packet is verifiable by its
  recipient with no live node.

## Authorization, receipts, surfaces, custody (feature-placement checklist)

Per [`ICN_OPERATING_MODEL.md`](../architecture/ICN_OPERATING_MODEL.md) §Feature-placement:

1. **What kind of thing is this?** Generic runtime logic (verification of a core
   noun), plus a tool consumer (`icnctl`). Not a new noun, not a package concept.
2. **Where does it belong?** An engine-adjacent pure verifier in `icn-governance`
   + the existing `icnctl` command. No package vocabulary; firewall-clean.
3. **Who authorizes it?** Nobody — verification reads and recomputes; it grants
   no authority and mutates no state. It is safe to run by anyone holding the
   receipts (this is the point of "re-verifiable").
4. **What receipt proves it?** None needed — verification is a read. (It may
   *emit* an operator log line; it must not write a receipt, to avoid a
   verify-writes-a-receipt-that-must-be-verified regress.)
5. **Which surface shows it?** CLI now; steward cockpit "chain verified ✓/✗"
   badge is a natural but out-of-scope follow-on.
6. **Custody/privacy/exit?** The verifier reads existing receipts and resolves
   public keys only; it introduces no new stored data and no new disclosure.
   It must not log receipt bodies or DIDs at INFO (see the logging-redaction gap
   noted in Related work).

## Acceptance criteria (falsifiable)

1. **Tamper is caught.** A test that takes a valid, stored ladder receipt,
   flips one byte of a canonical field, and re-serializes it MUST make the
   verifier return `verified: false` with a V1 `FAIL` on that receipt.
   *Today, the same tampered receipt passes `icnctl audit verify`.* This single
   test is the falsification of the current state and the definition of done for
   the core.
2. **Honest genuine PASS.** A complete, untampered rehearsal chain (organizer→
   member loop, the #2406–#2409 path) verifies `verified: true` with every
   applicable check PASS and no `UNRESOLVED` when the node's keys are available.
3. **Honest partial.** The same chain verified with a key withheld reports
   `UNRESOLVED_KEY` on the signature check and `verified: false` — never a
   silent PASS.
4. **Broken link is caught.** A chain missing one rung reports `BROKEN_LINK`,
   not a pass over the remaining rungs.
5. **No re-implementation drift.** The verifier calls the existing
   `proof.rs` `compute_record_hash(fields…)` / `compute_vote_hash` /
   `verify_signature` functions (and `verify_binding` where a type exposes it);
   a grep shows it does not define its own BLAKE3 domain-tag encoding. (Because
   the ladder classes expose only the associated `compute_record_hash`, the
   verifier is a per-class shim that reads fields and re-passes them — still no
   BLAKE3 re-implementation.)

## Non-claims

- This is a design contract. Nothing here is implemented.
- A green verifier proves *integrity and authenticity of recorded facts*, not
  their *legitimacy*: it does not check that a decision was authorized, that a
  quorum was real, or that a mandate stood behind an act. Those are separate
  gaps (see Related work). A verified receipt still "records a process fact and
  grants zero authority."
- This is not federation, not production, not a pilot, and closes no human gate.

## Related work

- [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) — the
  envelope this contract makes re-verifiable; ADR-0008 (receipt chain + audit
  verify origin); candidate ADR-0072 (Layer 4 *query*, distinct from this
  *re-verification*).
- [`community-proof-spine-0.1.md`](community-proof-spine-0.1.md) — the closest
  existing artifact: it already demonstrates the "blake3-bound receipt →
  recompute-to-verify" pattern for one runtime authority path (#2084). This
  contract generalizes that recompute discipline to the whole receipt set; it is
  not a duplicate (that spec verifies one path; this defines the generic
  re-verifier), but new work should reuse its recompute conventions.
- The **legitimacy** half — that a recorded decision was actually authorized —
  is out of scope here and tracked as separate gaps: adopted-policy evaluation
  (the CCL evaluator-selection seam, parked under #2141), act-time mandate
  enforcement (ADR-0019 "kernel dispatch gated by mandates — NOT IMPLEMENTED"),
  and the DomainPolicy adoption receipt (a spine transition that currently emits
  no receipt — see `docs/spec/domain-policy-adoption-receipt.md`, companion
  scaffold).
- Logging redaction: the verifier must not reintroduce DID/body leakage into
  operational logs (a known observability gap).
