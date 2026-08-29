//! The contract code hash — one authoritative implementation.
//!
//! # What this is
//!
//! [`compute_contract_code_hash`] is the single definition of the rule that
//! turns a [`Contract`] into the [`ContentHash`] that identifies a deployment.
//! Before this module existed the same ten lines were written out ten times —
//! three production sites and seven test/protocol twins — with no shared
//! function, so changing one silently desynchronised the rest.
//!
//! # Where the result travels
//!
//! The hash this produces is not a local convenience value. It is:
//!
//! * **signed** — participants sign
//!   [`ContractDeploymentMessage::compute_signing_bytes`], which is
//!   `code_hash ‖ installed_at`;
//! * **gossiped** — published on the `contracts:deploy` topic;
//! * **accepted verbatim from remote peers** — the remote install path in
//!   [`crate::actor`] takes `msg.code_hash` as given and does *not* recompute
//!   it from `msg.contract`;
//! * the **in-memory registry and invocation key** for
//!   [`crate::runtime::ContractRuntime`].
//!
//! It is **not** the durable `ContractRegistry` storage key. That is a separate
//! rule — [`crate::registry::compute_hash`], BLAKE3 over the JSON serialisation
//! — which owns the persisted `contract:*` / `metadata:*` keyspace and is
//! independently spelling- and order-sensitive. The two rules are distinct and
//! must not be merged; see `docs/architecture/n2-a-migration-gate.md`.
//!
//! # What it hashes, and what it does not
//!
//! Carried forward from the original implementation: this is a placeholder for
//! hashing the contract *bytecode*. It hashes the contract **name and
//! participant list** and nothing else — currency, state variables, rules and
//! triggers are all outside it, so two contracts with the same name and
//! participants but different bodies share one identity. Replacing this with a
//! real code hash is a protocol change that moves every deployed identifier,
//! not a cleanup.
//!
//! # Why the encoding is left as it is
//!
//! Consolidating this rule deliberately changes **ownership and nothing else**.
//! Every property below is load-bearing for values that are already signed,
//! already gossiped, and already accepted from remote peers, so "improving" any
//! of them would move live protocol identifiers:
//!
//! * **SHA-256**, not a newer digest.
//! * The contract **name is fed raw** (`as_bytes()`), not `Debug`-quoted.
//! * Participants are fed in **`Vec` order**. Permuting the participant list
//!   changes the contract's identity. They are deliberately not sorted.
//! * Each participant is fed as **`format!("{participant:?}")`** — the derived
//!   `Debug` of `Did(String)`, so the bytes include the `Did(`, the quotes and
//!   Rust's string escaping. This is *not* `Display` and *not* `as_str()`.
//! * Consequently the hash is **sensitive to the textual spelling** of a DID.
//!   `did:icn:` identifiers are multibase, so one principal has many accepted
//!   spellings, and today each one yields a different contract identity.
//! * There is **no domain-separation tag, no length prefix and no separator**
//!   between fields. `compute_contract_code_hash` of an empty-named contract
//!   with no participants is exactly `sha256("")`. The concatenation is
//!   therefore ambiguous between the name and the participant list.
//!
//! That ambiguity and that spelling sensitivity are real defects. They are also
//! **migration semantics**, owned by N2-A / I7 (#2627), not by this module.
//! Fixing them here would change every deployed contract's identifier without a
//! migration. Do not repair them opportunistically — change the rule only as
//! part of a migration that also moves the identifiers that already exist.

use crate::ast::Contract;
use icn_ledger::ContentHash;
use sha2::{Digest, Sha256};

/// Compute the deterministic code hash identifying a contract deployment.
///
/// The historical rule, preserved byte for byte: `SHA-256(name ‖ Debug(p₀) ‖ …
/// ‖ Debug(pₙ))` over the participants in their declared `Vec` order.
///
/// See the module documentation for what the result is used for and why the
/// encoding must not be tidied up.
pub fn compute_contract_code_hash(contract: &Contract) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update(contract.name.as_bytes());
    for participant in &contract.participants {
        hasher.update(format!("{participant:?}").as_bytes());
    }
    ContentHash::from_bytes(hasher.finalize().into())
}
