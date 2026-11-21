//! Executable contract for journal-entry identity versus operation identity (icn#2871 part 2).
//!
//! A journal entry id is `SHA-256(canonical JSON of {timestamp, author, contract_ref, accounts,
//! parents, nonce})` (`hash.rs`). It is a *content digest*. These tests pin what that digest can
//! and cannot do when two economic operations look identical, without changing any encoding:
//!
//! 1. Two distinct same-shape operations built in the same millisecond with no nonce get the SAME id.
//!    That is the collision behind the gateway settlement failure on icn#2873.
//! 2. A distinct per-operation nonce separates them in the same millisecond. `nonce` is already part
//!    of the hashed content (`"nonce":null` when absent), so supplying one changes no encoding.
//! 3. The SAME operation nonce in a later millisecond yields a DIFFERENT id. The content digest
//!    therefore cannot make a retry idempotent: retry safety needs an operation index keyed by the
//!    operation identity, not by the entry id.
//! 4. Golden vector: the id of an entry without a nonce is frozen, so historical ids stay valid.
//!
//! Every timestamp is fixed. Nothing here depends on wall-clock timing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::Did;
use icn_ledger::types::ProvenanceRef;
use icn_ledger::{AccountDelta, ContentHash, JournalEntry};

const PAYER: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
const PAYEE: &str = "did:icn:zH3C2AVvLMv6gmMNam3uVAjZpfkcJCwDwnZn6z3wXmqPV";
const T0: u64 = 1_762_819_200_000; // fixed millisecond

/// The shape the gateway builds for `LedgerManager::create_settlement` (`ledger_mgr.rs`):
/// author = payer, debit payer, credit payee, direct-member provenance, no nonce.
fn settlement_shape(timestamp: u64, nonce: Option<[u8; 32]>) -> JournalEntry {
    let payer = Did::from_str(PAYER).unwrap();
    let payee = Did::from_str(PAYEE).unwrap();
    let mut entry = JournalEntry {
        id: None,
        timestamp,
        author: payer.clone(),
        contract_ref: None,
        accounts: vec![
            AccountDelta::debit(payer.clone(), "hours".to_string(), 50),
            AccountDelta::credit(payee, "hours".to_string(), 50),
        ],
        parents: vec![],
        signature: None,
        nonce,
        provenance: ProvenanceRef::DirectMember {
            did: payer,
            auth_proof: "system-executed".to_string(),
        },
    };
    entry.compute_hash().unwrap();
    entry
}

fn id(entry: &JournalEntry) -> ContentHash {
    entry.id.clone().expect("compute_hash sets the id")
}

/// (1) The defect mechanism, deterministically: two operations, one id.
#[test]
fn distinct_operations_without_nonce_in_one_millisecond_share_an_id() {
    let first = settlement_shape(T0, None);
    let second = settlement_shape(T0, None);
    assert_eq!(
        id(&first),
        id(&second),
        "characterization: nothing but the clock distinguishes nonce-less same-shape operations"
    );
}

/// Provenance is not part of the id: two operations authorised by different proofs still collide.
#[test]
fn provenance_does_not_separate_same_millisecond_operations() {
    let first = settlement_shape(T0, None);
    let mut second = settlement_shape(T0, None);
    second.provenance = ProvenanceRef::DirectMember {
        did: Did::from_str(PAYER).unwrap(),
        auth_proof: "jwt-sig:a-different-request".to_string(),
    };
    second.compute_hash().unwrap();
    assert_eq!(
        id(&first),
        id(&second),
        "characterization: provenance is excluded from the content digest"
    );
}

/// (2) A distinct operation nonce separates same-millisecond operations, no format change.
#[test]
fn distinct_operation_nonces_separate_same_millisecond_operations() {
    let first = settlement_shape(T0, Some([0x01; 32]));
    let second = settlement_shape(T0, Some([0x02; 32]));
    assert_ne!(id(&first), id(&second));
}

/// (3) The digest is not an idempotency key: a retry of ONE operation built a millisecond later
/// is a different entry. Exactly-once therefore needs a create-only index keyed by the operation
/// identity (as the treasury receipt claim does), never id equality alone.
#[test]
fn same_operation_nonce_in_a_later_millisecond_is_a_different_entry() {
    let attempt = settlement_shape(T0, Some([0x07; 32]));
    let retry = settlement_shape(T0 + 1, Some([0x07; 32]));
    assert_ne!(
        id(&attempt),
        id(&retry),
        "characterization: id-level duplicate refusal cannot catch a retry built later"
    );
}

/// Same operation, same millisecond, same nonce: one id (the only case id equality catches).
#[test]
fn same_operation_nonce_in_the_same_millisecond_is_one_identity() {
    assert_eq!(
        id(&settlement_shape(T0, Some([0x07; 32]))),
        id(&settlement_shape(T0, Some([0x07; 32])))
    );
}

/// (4) Compatibility pin. The preimage and digest of a nonce-less entry are frozen: any proposal
/// that changes what feeds the id must leave this vector (historical entries) unchanged.
/// Cross-checked independently: `hashlib.sha256(PREIMAGE.encode()).hexdigest()` in Python.
#[test]
fn nonce_less_entry_id_golden_vector() {
    let entry = settlement_shape(T0, None);
    const PREIMAGE: &str = concat!(
        r#"{"timestamp":1762819200000,"#,
        r#""author":"did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9","#,
        r#""contract_ref":null,"#,
        r#""accounts":[{"account_id":"did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9","#,
        r#""currency":"hours","debit":50,"credit":null},"#,
        r#"{"account_id":"did:icn:zH3C2AVvLMv6gmMNam3uVAjZpfkcJCwDwnZn6z3wXmqPV","#,
        r#""currency":"hours","debit":null,"credit":50}],"#,
        r#""parents":[],"nonce":null}"#
    );
    const GOLDEN: &str = "91b607ce4c41404eb950ea691d02413a03936a0a122f7ee482a153d7660249fb";
    let recomputed = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(PREIMAGE.as_bytes()))
    };
    assert_eq!(
        hex::encode(id(&entry).0),
        recomputed,
        "the id must be SHA-256 of exactly this preimage"
    );
    assert_eq!(
        recomputed, GOLDEN,
        "historical nonce-less ids must not move"
    );
}
