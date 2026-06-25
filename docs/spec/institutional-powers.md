---
Status: normative
Authority: spec (defines the legitimacy circuit and institutional power classes as design-level doctrine over primitives already named by the canon; forward-direction clauses are named as such)
Canonical: no
Last Reviewed: 2026-06-25
---

# Institutional Powers and Legitimacy Invariants

> **Status: spec, work-in-progress.** This document names how ICN encodes *institutional
> power* so that law, governance, economics, justice, contribution, and protective
> authority cannot become unaccountable state or capital power in new clothes. It is
> design-level doctrine over primitives the canon already names — `InstitutionalDomain`,
> `DomainPolicy`, `AuthorityGrant`, `Mandate`, `TypedScope`, `EffectManifest`,
> `KernelEffect`, `GovernanceDecisionReceipt`, `InstitutionalEffectRecord`,
> `EffectDispatchEvidence` — and the [Effect Dispatch Contract](effect-dispatch-contract.md).
> It introduces **no** Rust types, schemas, wire formats, CCL grammar, routes, or runtime
> behavior. Clauses that depend on objects or kernel behavior not yet landed are named as
> forward-direction. The PR introducing this doc uses `Refs:`; it closes nothing.

## Purpose

ICN must host real institutions — bodies that make binding collective decisions, collect
and allocate shared contributions, interrupt active harm, and resolve injury. The danger is
not that institutions exist; it is that institutional power tends to become **sovereign**:
self-justifying, unbounded, invisible, and irreversible. State power and capital power both
take that shape. ICN's wager is that the same legitimacy machinery the canon already uses
for governed effects — authority, adopted policy, bounded execution, receipts — is exactly
what keeps institutional power *accountable* rather than sovereign.

This spec states that machinery as a single rule and four power classes, and maps each onto
seams that already exist.

> The ICN does not reject institutions. It rejects **sovereign, self-justifying**
> institutions.
>
> The ICN does not make institutions harmless by trusting them. It makes them **accountable**
> by forcing every act of institutional power through visible authority, adopted policy,
> bounded execution, receipts, and repair.

## The legitimacy circuit

Every binding institutional action in ICN must pass through the same circuit:

```
authority basis  ->  adopted policy  ->  bounded effect  ->  receipt  ->  challenge / repair path
```

No stage is optional, and the stages are ordered: an effect with no adopted policy behind it,
or a policy with no authority basis, or an effect with no receipt, or a receipt with no
challenge path, is **not** a legitimate institutional action. The circuit is the same one the
[Effect Dispatch Contract](effect-dispatch-contract.md) already describes for turning an
accepted decision into bounded effects with receipts; this spec adds that **all** institutional
power — not only service/tool binding — must traverse it, and names the failure-closed
invariants that hold at each stage.

## `InstitutionalPowerEvent`

`InstitutionalPowerEvent` is the abstract envelope every binding institutional act is an
instance of. It is **not** a new kernel type; it is the doctrine-level shape that the existing
record types already carry in pieces. An `InstitutionalPowerEvent` names, at design level:

- the **power class** exercised (`GovernancePower` / `ContributionPower` / `ProtectivePower` / `RepairPower`);
- the **authority basis** that legitimizes it (an `AuthorityGrant` / `Mandate` / standing, tied to a governing decision);
- the **adopted policy** under which it acts (a `DomainPolicy` clause / CCL policy selected from the registry);
- the **bounded effect** it produces (an `EffectManifest` resolving to one or more primitive `KernelEffect`s);
- the **receipt(s)** it emits (`GovernanceDecisionReceipt` + `InstitutionalEffectRecord` + `EffectDispatchEvidence`);
- the **challenge / repair path** by which it can be contested or reversed.

Concretely, an `InstitutionalPowerEvent` is reconstructable from existing records: the
`GovernanceDecisionReceipt` (authority + adopted policy), the `EffectManifest` /
`InstitutionalEffectRecord` (bounded effect), and `EffectDispatchEvidence` (that the effect
actually dispatched). This spec asserts they describe one accountable event and must not be
separable in a way that hides any stage.

## Power classes

The four classes partition *what kind* of binding institutional power is being exercised.
Each is defined as the ICN reconciliation of a real-world power with the sovereign form ICN
rejects.

### `GovernancePower`

The authority to make binding collective decisions: adopt or amend policy, amend charters,
bind tools / services / routes, join or leave federations, and change standing rules. It is
the power that *creates the other powers' authority bases* — it is how a domain decides what
is legitimate. GovernancePower is exercised only through an adopted governance procedure and
recorded as a `GovernanceDecisionReceipt`.

### `ContributionPower`

The democratically adopted power to assess, collect, waive, allocate, refund, or redistribute
**shared contributions** to the commons. This is the ICN reconciliation of *taxation without
sovereign extraction*: a contribution is legitimate only when an adopted policy makes its
basis visible and contestable, never because an institution can simply take. ICN-native
vocabulary is **contribution / obligation / allocation / settlement**, never sovereign levy.

### `ProtectivePower`

Temporary, reviewable authority to interrupt active or imminent harm. This is the ICN
reconciliation of *force without police power or a permanent coercive class*: protective
authority exists to stop a harm in progress, expires by construction, and is always
reviewable and challengeable. There is no standing enforcement office; there is only scoped,
time-bounded, receipted interruption that someone must answer for.

### `RepairPower`

The authority to resolve harm: assign repair obligations, compensate affected parties,
reverse invalid effects, restore standing, and produce accountable repair. This is the ICN
reconciliation of *justice without punishment-first carceral logic*: the first move is
repair and restoration of standing, surfaced to affected parties with a receipt and a
challenge/reversal path — not punishment, and never a silent or retroactive rewrite.

## Legitimacy invariants

These hold for every `InstitutionalPowerEvent`. Each is **fail-closed**: when the condition
cannot be established, the action does not happen.

1. **No institutional power without an authority basis.** Every binding act references an
   `AuthorityGrant` / `Mandate` / standing that legitimizes it.
2. **No authority basis without adopted policy.** An authority basis is valid only under a
   `DomainPolicy` / CCL policy the domain has adopted.
3. **Unadopted policy is inert.** A drafted-but-unadopted policy authorizes nothing and
   produces no effect.
4. **Ambiguous authority fails closed.** If the authority basis cannot be resolved
   unambiguously, the action is denied — never granted on a guess.
5. **Protective authority is temporary by construction.** Emergency / protective authority
   must be scoped, time-bounded, receipted, reviewable, and challengeable. **No protective
   action may be permanent at creation** — an expiry/review obligation is part of the act.
6. **Contributions are fully disclosed.** No contribution may be collected without a visible
   assessment formula, a liable entity, an amount, a due date, an allocation target, a waiver
   path, and an appeal / challenge path.
7. **Repair respects due process.** No repair / justice effect may bypass due process,
   affected-party visibility, a receipt, and a challenge / reversal path.
8. **No ledger / treasury bypass.** Contribution and settlement effects flow only through
   adopted policy and receipts — never a direct ledger or treasury write that skips the
   circuit.
9. **No silent mutation.** No resource, standing, route, treasury, federation, membership, or
   policy mutation may be silent: each emits a receipt / `InstitutionalEffectRecord`.
10. **Reversal is forward-only.** A reversal is a new transition / counter-effect with its own
    receipt — never a retroactive edit of a prior record.
11. **The right to exit remains constitutional** wherever the domain model already defines it;
    no institutional power may revoke it.
12. **The meaning firewall holds.** The kernel must not import domain semantics; meaning stays
    in the governance / domain / package / CCL layers. The kernel receives primitive
    `KernelEffect`s only and enforces them blindly.

## Mapping to existing seams

This spec is doctrine *over* the existing object model, not new machinery. Each stage of the
circuit binds to seams that already exist:

| Circuit stage | Existing seam(s) | Source |
|---|---|---|
| authority basis | `AuthorityGrant`, `TypedScope`, `Mandate`, standing | `icn-governance/src/authority.rs`, `icn-governance/src/mandate.rs`; ADR-0019 |
| adopted policy | `InstitutionalDomain`, `DomainPolicy`, CCL policy registry / evaluator selection | `icn-governance/src/institutional_domain.rs`; [institutional-domain.md](institutional-domain.md), [ccl-policy-registry.md](ccl-policy-registry.md) |
| bounded effect | `EffectManifest` → `KernelEffect` | `icn-governance/src/effect_manifest.rs`, `icn-kernel-api/src/effects.rs`; [effect-dispatch-contract.md](effect-dispatch-contract.md) |
| receipt | `GovernanceDecisionReceipt`, `InstitutionalEffectRecord`, `EffectDispatchEvidence` | `icn-governance/src/proof.rs`, `apps/governance/src/institutional_effect.rs`, `apps/governance/src/dispatch_evidence.rs`; ADR-0025, ADR-0026 |
| challenge / repair path | counter-effect / reversal / counter-receipt (RepairPower) | [effect-dispatch-contract.md](effect-dispatch-contract.md); ADR-0027 (action-card contract: mandate + reversibility + receipt before confirm) |

The constitutional object model (ADR-0014) and the authority-grant minting / mandate
persistence seam (ADR-0019) anchor the authority side; the institutional effect record
(ADR-0025) and receipt/provenance proof envelope (ADR-0026) anchor the receipt side. The
`coop_id → EntityId` resolver lane (RFC-0018; #2187–#2190) is the prerequisite that lets the
authority side name *which entity* an authority basis belongs to, fail-closed, with
provenance.

## Power class → effect family (design level only)

This mapping is descriptive: it says which existing effect families a power class *may*
produce, and under what gate. It introduces no new effect types and changes no dispatch code.

| Power class | May produce (effect families) | Only through |
|---|---|---|
| `GovernancePower` | policy / charter / binding / federation / config effects | an adopted governance procedure recorded as a `GovernanceDecisionReceipt` |
| `ContributionPower` | treasury / ledger / clearing effects | an adopted contribution `DomainPolicy` + receipts (never a direct ledger/treasury bypass) |
| `ProtectivePower` | temporary membership / resource / control / dispute effects | a scoped authority with an expiry and a review obligation at creation |
| `RepairPower` | dispute / reversal / compensation / standing effects | a due-process path with affected-party visibility, a receipt, and a challenge / reversal path |

Every row still traverses the full legitimacy circuit and obeys every invariant above. The
table narrows *which* primitive effects are reachable; it does not loosen *how* they are
authorized.

## What this spec is not

- Not runtime implementation. No code, no schemas, no wire formats, no CCL grammar changes.
- Not a route, auth-behavior, or token-issuance change.
- Not a redefinition of the meaning firewall, the policy-oracle pattern, the kernel/app
  boundary, or the effect dispatch chain; where this spec touches them, existing canon remains
  authoritative.
- Not a package-vocabulary surface: a specific institution's role names, ceremonies, and
  fixtures stay in their own repositories.
- Not a production, pilot, live-federation, NYCN, or complete-API readiness claim.
- Not closure of any implementation issue.

## Relationship

- Refs RFC-0018 (entity-aware request authorization) and the A2 resolver lane #2187, #2188, #2189, #2190.
- Refs ADR-0014 (constitutional object model), ADR-0019 (authority-grant minting + mandate persistence seam), ADR-0025 (institutional effect record canonical schema), ADR-0026 (receipt & provenance proof envelope), ADR-0027 (action-card contract).
- Refs sibling specs: [institutional-domain.md](institutional-domain.md), [effect-dispatch-contract.md](effect-dispatch-contract.md), [ccl-policy-registry.md](ccl-policy-registry.md), [governed-service-binding.md](governed-service-binding.md).
- Refs #2061, #2080, #1868, #2082 (entity-aware authorization / trusted issuance / scope decomposition / coop↔entity mapping — the authority-side work this doctrine governs).
