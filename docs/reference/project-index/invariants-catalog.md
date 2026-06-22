---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-22
---

# ICN Invariants Catalog

> This catalog indexes invariants. It does not define them. The canonical source listed for each entry remains authoritative.

ICN's invariants are real but scattered across four canonical sources in different formats, so the
families get conflated or miscounted. This page is a single searchable enumeration that links each
invariant back to where it is actually defined. The machine-readable form is
[`invariants-catalog.toml`](invariants-catalog.toml).

**This is an orientation/index layer (`Canonical: no`), not a truth root.** It restates nothing; it
only points. The numbers below are the live source counts (verified 2026-06-22) and they match the
counts in [#2114](https://github.com/InterCooperative-Network/icn/issues/2114): 5 + 6 + 10 + 7 = **28**.

## Non-goals

- Not new invariants or doctrine — this page invents nothing.
- Not a redefinition — definitions live in the canonical sources; only one-line labels appear here.
- Not a competing source of truth — when this page and a canonical source disagree, the canonical
  source wins. The full precedence ladder is in [`source-of-truth-map.md`](source-of-truth-map.md).

## How to read an entry

Each entry carries: family, a stable id, a one-line label (mirroring the source's own wording), the
canonical source path, a stable **anchor** (a section heading, Rust enum variant name, or bold
label — not a line number, which drifts), source type (doc/code), and whether the source states it
normatively.

---

## 1. Operational invariants (tier 1)

Canonical source: [`docs/DESIGN_PRINCIPLES.md`](../../DESIGN_PRINCIPLES.md) §1 · 5 invariants · doc

| ID | Invariant | Anchor | Normative |
|----|-----------|--------|-----------|
| `adversarial_by_default` | Every peer is untrusted until verifiable participation establishes trust. | §1.1 Adversarial-by-default | yes |
| `determinism` | Same inputs produce same outputs; no floating point in consensus paths. | §1.2 Determinism | yes |
| `canonical_encodings` | One valid byte representation per value (definite-length CBOR). | §1.3 Canonical encodings | yes |
| `no_panics_in_protocol_paths` | Protocol/network/actor paths return `Result`, never panic on bad input. | §1.4 No panics in protocol paths | yes |
| `kernel_app_boundary_inviolable` | The dependency graph forbids domain crates inside kernel crates. | §1.5 Kernel/app boundary is inviolable | yes |

> `kernel_app_boundary_inviolable` is the tier-1 statement of the tier-2 firewall contract below —
> the same rule, decomposed into six mechanically checkable sub-rules.

## 2. Firewall contract (tier 2)

Canonical source: [`docs/architecture/KERNEL_APP_SEPARATION.md`](../../architecture/KERNEL_APP_SEPARATION.md) · 6 invariants · doc · mechanically enforced via CI

| ID | Invariant | Anchor | Normative |
|----|-----------|--------|-----------|
| `kernel_no_domain_imports` | Kernel must not import domain crates. | Invariant 1 | yes |
| `kernel_pure_data_types` | Kernel only accepts pure data types at its entrypoints. | Invariant 2 | yes |
| `semantic_lookup_in_policy_oracle` | All semantic lookup is confined to `PolicyOracle`. | Invariant 3 | yes |
| `no_pattern_match_constraintset_custom` | Kernel must not pattern-match on `ConstraintSet.custom` keys. | Invariant 4 | yes |
| `mechanism_not_policy` | Kernel contains enforcement mechanisms, not policy decisions. | Invariant 5 | yes |
| `opaque_receipt_storage` | Kernel stores app-generated receipts opaquely (never parses the body). | Invariant 6 | yes |

## 3. Frozen-core invariants (tier 3)

Canonical source: [`icn/crates/icn-kernel-api/src/invariants.rs`](../../../icn/crates/icn-kernel-api/src/invariants.rs) (`enum InvariantId`) · 10 invariants · code

The count is asserted in-crate by `test_all_invariants_returns_10`. Anchors are the enum variant names.

| ID | Invariant | Variant | Domain | Normative |
|----|-----------|---------|--------|-----------|
| `right_of_exit` | No rule can prevent an identity from leaving an entity. | `RightOfExit` | SovereigntyCore | yes |
| `no_retroactive_obligations` | Rules only apply to actions after activation block. | `NoRetroactiveObligations` | SovereigntyCore | yes |
| `explicit_cryptographic_entry` | Adding an identity requires their cryptographic signature. | `ExplicitCryptographicEntry` | SovereigntyCore | yes |
| `no_silent_power_changes` | Authz changes must include a computable capability delta. | `NoSilentPowerChanges` | AntiCaptureCore | yes |
| `bounded_authority` | No wildcard capabilities across all resources. | `BoundedAuthority` | AntiCaptureCore | yes |
| `mandatory_execution_delay` | Economic/capability changes require timelock delay. | `MandatoryExecutionDelay` | AntiCaptureCore | yes |
| `capability_conservation` | Cannot delegate capabilities you don't possess. | `CapabilityConservation` | AntiCaptureCore | yes |
| `mutual_credit_conservation` | Mutual credit loops must net to zero. | `MutualCreditConservation` | EconomicCore | yes |
| `sovereign_signature_requirement` | Funds move only via owner signature or pre-consented dispute hook. | `SovereignSignatureRequirement` | EconomicCore | yes |
| `universal_receipt_generation` | Every state transition emits an immutable receipt. | `UniversalReceiptGeneration` | EconomicCore | yes |

## 4. Regulatory invariants (tier 4)

Canonical source: [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md) §5 ("Seven regulatory invariants") · 7 invariants · doc

| ID | Invariant | Anchor | Normative |
|----|-----------|--------|-----------|
| `user_signed_transitions_only` | No transition without the user's cryptographic signature. | User-signed transitions only | yes |
| `no_hosted_positions` | ICN holds no balances on behalf of users. | No hosted positions | yes |
| `no_operator_routing_of_value` | Units move directly debtor → creditor, not through an operator account. | No operator routing of value | yes |
| `derived_views_not_primitives` | A balance is a computed view, not enforced protocol state. | Derived views are not protocol primitives | no (descriptive) |
| `no_embedded_convertibility` | Cross-coop settlement requires explicit trust agreements + governance approval. | No embedded convertibility | yes |
| `matching_market_opt_in` | Marketplace features are opt-in, scoped, governance-authorized (a CCL contract, not a platform feature). | Matching/market features are opt-in, scoped, governance-authorized | yes |
| `execution_receipts_close_loop` | Every state transition produces a cryptographically signed receipt. | Execution receipts close the loop | yes |

> Cross-reference: `execution_receipts_close_loop` (regulatory) and `universal_receipt_generation`
> (frozen-core) are the same receipt-universality property seen from two tiers. The catalog records
> the link via `related_to`; it does not merge them.

---

## Related enumerations (NOT indexed here)

The four families above are exactly the ones #2114 names. The repo contains other invariant-shaped
lists that are deliberately **out of scope** for this catalog, to avoid double-counting and taxonomy
sprawl:

- [`AGENTS.md`](../../../AGENTS.md) restates the five operational invariants as a PR-review gate for
  the `icn-invariants-guardian`. That is the same family 1, not a fifth family — `docs/DESIGN_PRINCIPLES.md`
  remains its canonical source. `AGENTS.md` points here for the broader source-linked index.
- [`docs/genesis.md`](../../genesis.md) describes "hard mechanisms" and "meta-invariants" under a
  different framing. Indexing those would require separate scoping; they are not part of #2114.

## Orientation discipline

This page lives in the project-index orientation layer. Like every map here it is `Canonical: no`:
defer to code/tests first, then [`docs/STATE.md`](../../STATE.md) and
[`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), then accepted ADRs/RFCs. Generated project-index
artifacts (under `generated/`) and these hand-maintained maps are navigation, not state.
