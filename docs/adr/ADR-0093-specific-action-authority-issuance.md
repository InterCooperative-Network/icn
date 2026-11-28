---
id: "0093"
title: "Specific-Action Authority Issuance for Institutional-Domain Acts"
status: "proposed"
date: "2026-11-27"
deciders: ["Matt Faherty"]
tags: ["governance", "authority", "mandate", "grant-minting", "institutional-domain", "domain-policy"]
supersedes: []
superseded_by: []
amends: ["ADR-0019 (minted-class set; executor authorization, point 3)", "ADR-0014 (MandateStatus: InProgress, Revoked causes)"]
implementation_status: "not-started"
references:
  - "ADR-0014 (Constitutional Object Model: AuthorityGrant, Mandate, MandateStatus, TypedScope)"
  - "ADR-0019 (Authority Grant Minting and Mandate Persistence Seam)"
  - "ADR-0083 (proposed) (InstitutionalDomain and DomainPolicy runtime root)"
  - "ADR-0092 (proposed, PR #2935) (Context Authority Log convergence)"
  - "docs/architecture/IDENTITY_SEMANTICS.md"
  - "icn/apps/governance/src/grant_minting.rs"
  - "icn/apps/governance/src/mandate_gate.rs"
  - "icn/apps/governance/src/domain_policy_adoption.rs"
  - "icn/apps/governance/src/close_journal.rs"
---

# ADR 0093: Specific-Action Authority Issuance for Institutional-Domain Acts

## Status

Proposed. This ADR is a design for maintainer review. Nothing here is implemented, and
nothing here changes production issuance until it is accepted.

Revisions 2–5 incorporate an independent architecture review and its delta reviews (the last delta review: no blockers; revision 5 applies its one-sentence follow-ups). The
BLOCKERs and FOLLOW_UPs are resolved below; the policy questions are listed under
*Open maintainer decisions*.

## Context

Two production acts are gated on ADR-0014 authority:

- `adopt_domain_policy_gated`, which sets an `InstitutionalDomain`'s current policy;
- `declare_institutional_domain_gated`, which creates an `InstitutionalDomain`.

Each requires, through `DefaultMandateGate`, an **Execution** `AuthorityGrant` to the actor
whose scope binds the domain and the act token (`domain_policy:adopt` /
`institutional_domain:declare`).

**No legitimate decision produces that authority today.**

- `derive_grants_for_accepted_proposal` mints grants only for SDIS steward proposals, which yield Attestation-class grants.
- ADR-0019 keeps the minting set small and requires a follow-up ADR to broaden it. That is this ADR.
- Authority for these acts can currently exist only if written outside governance. The gates must stay fail-closed until this ADR is decided and implemented.

**Gaps on main that this design must close, not inherit:**

1. **Content binding.** The gate never reads `Mandate.payload_hash`. Resolution returns the first matching mandate with no content filter. A consumer that compared fields after resolution could pick the wrong mandate, and would be comparing against data that is not bound to the decision.
2. **Discharge shadowing.** Domain-target resolution returns at the first matched mandate's lifecycle failure. A discharged mandate would therefore block a later, valid mandate for the same executor and domain.
3. **Mutable pointer.** `InstitutionalDomain.current_policy` is a mutable pointer written by a plain put, with no record of which authority set it. Replaying an old execution could silently revert a newer one.
4. **Grantor.** The gate compares `scope.domain` with the request but never the grantor, so authority minted by one domain could name another.
5. **Ungated declaration.** `declare_institutional_domain` is `pub` and ungated "for bootstrap … authority established out of band". That is operator sovereignty.

**Dependency.** This ADR assumes decision-commitment verification: a mandate authorizes only when its proposal has committed to exactly the mandate's decision. That verification is implemented on a pending branch, not on main. Implementing this ADR requires it to land first.

## Decision (proposed)

### 1. Specific-action authority first

The first issuance path is **specific-action authority**. One accepted proposal authorizes
exactly one institutional act with exactly specified content, performed by one named executor.

Scoped discretionary, office/role and conditional authority are acknowledged and **not**
introduced (§10).

### 2. Dedicated typed payloads

Two dedicated proposal payload variants. Names and fields may be refined in review:

```
AdoptDomainPolicy {
    domain: GovernanceDomainId,
    policy_id: DomainPolicyId,                 // content address of the exact policy text
    expected_prior: Option<DomainPolicyId>,    // the current_policy this decision replaces
    executor: <principal>,                     // see §6
    valid_for: Seconds,
}
DeclareInstitutionalDomain {
    domain: GovernanceDomainId,
    owning_entity_class: BootstrapEntityType,
    charter_ref: Option<CharterId>,
    executor: <principal>,
    valid_for: Seconds,
}
```

There is no generic "grant authority" payload. Each new act class needs its own variant and
its own ADR-level justification.

The gate's class-level `proposal_class` tokens for these acts ("DomainPolicy",
"InstitutionalDomain") are discretionary scope and are removed for these acts. Specific-action
grants are minted with an **empty** `proposal_class`, so no gate, old or new, can match them
by class.

### 3. Action commitment (exact content binding)

**What it is.** Each specific-action mandate carries an **action commitment**: a fixed-layout,
domain-separated blake3 hash over:

- a version and domain-separation tag (e.g. `icn/specific-action/v1`);
- the act token;
- the target domain;
- the act content:
  - adoption: `policy_id` and `expected_prior`;
  - declaration: `owning_entity_class` and `charter_ref`.

**Encoding.** The layout is canonical and length-prefixed. It does **not** reuse the
serde_json payload hash, because that hash depends on field order and changes when a field
or serde attribute is added. Golden-vector tests pin the encoding (the canonical-encodings
invariant).

**How it is enforced.**
- The commitment is stamped on the mandate at mint time.
- The requesting act computes the same commitment from what it is about to do and passes it in the `MandateRequest`.
- The resolver considers **only** mandates whose stamped commitment equals the requested one.
- A substituted action therefore finds no authority. Holding several live mandates for one domain cannot select the wrong one.

The commitment is shape-agnostic. A future discretionary scope can be expressed as a set of
admissible commitments (§10).

**Versioned act tokens (downgrade safety).** Specific-action grants are minted under new act
tokens, `domain_policy:adopt/v1` and `institutional_domain:declare/v1`. A binary that
predates this ADR recognizes only the unversioned tokens, so it finds no matching grant and
fails closed. It never treats a specific-action grant as content-free authority. The
unversioned tokens are never minted.

### 4. Grantor and scope binding

- **At mint:** refuse when `payload.domain` differs from the deciding governance domain. A domain's decision cannot mint authority over another domain.
- **At the gate:** for these acts, require `grant.grantor == scope.domain == request.domain`.
- This rule assumes D6's draft answer (the domain decides for itself). If D6 admits another sovereign forum, this rule is replaced by "grantor == the forum D6 names", and §4 is conditional on D6.
- **Platform identities:** a node operator, gateway or daemon identity is never a grantor or an implicit executor.

### 5. Durable, recoverable, single execution

The act writes to two stores:

- the mandate and its effect record, in the receipt store (Db-A);
- the domain record, in the governance state store (Db-B).

Execution is a three-step protocol. It reuses existing objects and only the close journal's
commit-boundary rule, not its replay-by-overwrite mechanics.

1. **Claim (Db-A, one transaction).** Move the mandate `Pending → InProgress`, stamp an execution id, and reserve the act's `InstitutionalEffectRecord` identity (already transactionally unique per proposal and effect kind). A second concurrent execution fails the claim.
   - The claim also writes an `InProgress` index entry, so recovery can enumerate claims.
   - It writes a commitment-keyed index entry, so a replay can find the execution record from the act content.
2. **Apply (Db-B, compare-and-put).** Apply the effect only if the domain is in the expected prior state:
   - adoption: `current_policy == expected_prior`;
   - declaration: the domain is absent.
   The **apply marker** is carried **inside the domain record's own value**: an append-only list of `(mandate id, applied effect)` entries. A single-key compare-and-put therefore covers the effect and its marker together. Later acts append; they never rewrite an entry. No multi-key write is involved, so a partially replayed commit cannot leave a marker without its effect.
   - **Recovery evidence, not history.** Markers exist to recover `InProgress` claims. The durable execution history is the `InstitutionalEffectRecord` in Db-A. An entry may be compacted away once its mandate is terminal there, so the list stays bounded.
   - **Every writer uses compare-and-put.** Every writer of the institutional-domain key, adoption and declaration included, uses compare-and-put against the exact bytes it read. A plain put would erase markers.
   - **Older binaries.** An older binary ignores the list on read but erases it on write. §12 therefore requires every claim to be resolved before a downgrade.
3. **Discharge (Db-A, one transaction).** Move `InProgress → Discharged`, revoke the mandate's grant (`revoked_at`), complete the effect record, and clear the `InProgress` index entry.

**Recovery.** For each `InProgress` mandate, recovery decides by its **apply marker**, never by the domain's current state:

| Marker | Precondition | Recovery action |
|---|---|---|
| present | — | Discharge (the act happened, even if a later act has since changed the domain) |
| absent | still holds | Release the claim back to `Pending`, conditional on the stamped execution id (nothing happened; the executor may retry while the mandate is valid) |
| absent | no longer holds | Set the mandate to `Revoked`, revoke its grant, and record the outcome `stale` on the effect record |

`Discharged` keeps its ADR-0014 meaning: all authorized effects applied. A stale execution is
`Revoked`, never `Discharged`. Recovery never re-applies an effect.

**When recovery runs.** Recovery runs only while no live execution path for the domain is
admitted: at startup, or under the per-domain execution lock. That stops a released claim
racing an in-flight apply.

**Live failures.** A live execution whose compare-and-put fails (no crash) takes the same
outcome as recovery's third row: `Revoked`, with the effect-record outcome `stale`.

**ADR-0014 amendment.**
- `InProgress` is redefined as "claimed for execution; effects may or may not have been applied".
- `InProgress → Pending` is permitted only as recovery's claim release.
- "Stale refusal" is added as a cause of `Revoked`, alongside explicit revocation and supersession.

**Multi-writer.** Two processes writing the same stores are declared unsupported for this
path. The claim transaction is the fence within one store.

### 6. Executor

- **Executor and deadline fields.** The executor is set in `Mandate.executor`, and the mandate deadline is `now + valid_for`. The mandate deadline is the gate's authoritative expiry.
- **Principal typing.**
  - Legacy-mode domains: the executor is a legacy principal.
  - Subject-mode domains refuse legacy principals (ADR-0092). This path applies to legacy-mode domains until a principal-typed executor is specified (open decision D12).
- **Losing standing.** Live specific-action grants are revoked by a member-removal or freeze decision against the executor, or by an explicit revoke-mandate decision.
- **Replacement.** Replacing an executor is a new decision, and it also revokes the old mandate. Two live mandates with different content can never both execute.
- **The claim is the cut-off.** A revocation or replacement that reaches a mandate already `InProgress` does not interrupt it: the claimed execution completes, or recovery resolves it per §5. Only `Pending` mandates are revocable. Revoking a mandate atomically with its claim is refused.
- **Sealing waits for claims.** Sealing a domain (§11) requires **no** `InProgress` claims for that domain. A seal attempted while a claim exists is refused until the claim resolves.
  - The seal takes the per-domain execution lock and holds it across its claim check and its commit. Claim and apply hold the same lock. The seal row and the claim live in different stores, and the lock is what serializes them; multi-writer is unsupported (§5).
  - No claim made before a seal can change the domain after it.

### 7. Idempotent, non-shadowing discharge

- **Terminal state.** A discharged mandate's grant is revoked, so the gate no longer lists it.
- **No early return.** Resolution skips dead or uncommitted candidates and keeps looking, so a discharged mandate cannot shadow later valid authority.
- **Replay.** Re-requesting an act that already executed returns the prior result from its `InstitutionalEffectRecord`, the durable execution receipt. It is found through the commitment-keyed index (§5). The gate is not involved.

### 8. Stale execution

A decision records `expected_prior`. If the domain has moved on by execution time (another
policy adopted, or the domain declared meanwhile), the compare-and-put refuses. A later
decision does not silently override an earlier one. Whether an institution may instead want
"latest decision supersedes" is open decision D13.

### 9. Constitutive versus constituted authority; founding

- **Constituted authority** is exercised within an existing institution under its established rules. Adoption is constituted authority.
- **Constitutive authority** creates or re-founds the institution itself. Declaration is constitutive.

**Stated limit.** The existing genesis and charter mechanisms do not yet supply
non-operator founding authority:

- governance-domain creation and charter activation are gated by capability scope;
- charter ratification counts founders without verifying their signatures.

Until a founding mechanism is chosen (D11), a declaration decided by the future domain's own
electorate inherits whatever authority set up that electorate. That is operator-mediated
founding, and this ADR does not claim otherwise.

**Ungated declaration.** The ungated `declare_institutional_domain` is retired from
production surfaces. It becomes test-only, or it is gated by the chosen founding mechanism.

### 10. Future authority shapes (acknowledged, not introduced)

- **Scoped discretionary authority:** a set of admissible action commitments bounded by institutional constraints. Needs its own ADR.
- **Continuing office/role authority:** needs role-to-grant minting, revocation at tenure end, and `Grantee::Role` / `MandateTarget::Role` support in the gate (which currently refuses Role).
- **Conditional authority:** needs a charter authority-clause schema and an evaluator.

None of these may be emulated by widening the specific-action path.

### 11. Relationship to ADR-0092

Under the Context Authority Log direction, institutional authority transitions move to
CAL-bound domains. This ADR is a **legacy-mode bridge**.

When a domain is sealed for cutover, pending specific-action mandates are refused (stale
execution across a cutover), never carried over. Sealing requires that no claim is in progress
for the domain (§6). The governance meaning stays in the app
layer and does not enter kernel crates.

### 12. Rollback

- **Downgrade with live grants.** The versioned act tokens (§3) make a pre-ADR binary fail closed on specific-action grants. Rolling back therefore never re-opens content-free adoption.
- **Downgrade with live claims.** Rolling back while `InProgress` claims exist is unsupported. A pre-ADR binary cannot recover them. This is the same rule as for pending enveloped close intents. The runbook requires every claim to be resolved before a downgrade.
- **Downgrade with new payloads.** Stored proposals that carry the new payload variants cannot be deserialized by a pre-ADR binary, so proposal hydration and listing would fail. A downgrade is supported only with no such proposals stored, or after rolling forward again. This is the same resolve-or-roll-forward runbook rule.

## Consequences

- **Legitimate authority.** The two gated consumers gain a legitimate source of authority under a one-decision, one-act model. Content binding, grantor binding, single execution and crash recovery are all specified.
- **End-to-end testing.** A full decision-to-execution test becomes possible: decide, mint, adopt, discharge, restart, replay, refusal of substitution, refusal of stale execution.
- **Until then.** Until acceptance and implementation, the consumers stay fail-closed, and component tests that seed grants remain component evidence only.

## Open maintainer decisions

| # | Decision | This draft |
|---|---|---|
| D1–D5, D9 | Shape, vehicle, payload form, execution model, binding/reuse, class tokens | As proposed above |
| D6 | Who may decide a declaration | Open. Draft: the future domain's own electorate, subject to §9's stated limit. §4's grantor rule follows this answer |
| D7 | Charter-direct grants (`granted_by: None`) | Open. Draft keeps them skipped |
| D8 | Office/role authority | Deferred (§10) |
| D10 | ADR-0083 status | Open: accept with an amendment pointing here, or supersede its authority section |
| D11 | Founding mechanism | Open: verified charter founder quorum, CAL genesis, or state operator-mediated founding as a limit |
| D12 | Executor typing for Subject-mode domains | Open |
| D13 | Stale execution: refuse, or later decision supersedes | Draft: refuse (§8) |
