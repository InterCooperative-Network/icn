//! P3 at the app layer: an anchor is derived from the **stored** binding only when the founding
//! evidence supports it (design spec §2.2 "EXTEND: anchor derivation"). EXPERIMENTAL, unratified.
//!
//! P2 pins that a raw-store holder can write, replace or delete a binding
//! (`institutional_authority_binding_store.rs`). These tests show that such a record, however it
//! got there, yields an anchor only if the context's own inception and founder-signed genesis
//! agree with it, and that missing or corrupt storage is a distinct refusal, never an anchor.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use ed25519_dalek::SigningKey;
use icn_governance::authz_threshold::{Ratio, ThresholdRuleV0};
use icn_governance::domain::served_domain_commitment;
use icn_governance::institutional_admission::RecognitionRequirement;
use icn_governance::institutional_domain::anchor_derivation::{
    AnchorDerivationError, ContextInception, ContextInceptionV1, LogEntryV0, SignedLogEntryV0,
    TransitionV0,
};
use icn_governance::institutional_domain::authority_binding::{
    authority_binding_key, BindingBasisV1, ContextRef, InstitutionalAuthorityBindingV1,
};
use icn_governance::GovernanceDomainId;
use icn_governance_actor::institutional_anchor_derivation::{
    derive_anchor_from_store, AnchorDerivationInput, StoredAnchorError,
};
use icn_governance_actor::institutional_authority_store::{
    AuthorityBindingError, CorruptBinding, InstitutionalAuthorityStore,
};
use icn_identity::authority_log::PrincipalKey;
use icn_store::{SledStore, Store};

const D: &str = "coop-a";
const OTHER: &str = "coop-b";

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn pk(k: &SigningKey) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(k.verifying_key()).unwrap()
}

fn rule() -> ThresholdRuleV0 {
    ThresholdRuleV0 {
        role: "member".into(),
        quorum: Ratio { num: 2, den: 3 },
        approval: Ratio { num: 2, den: 3 },
    }
}

/// A context founded (2-of-3) for `domain`, and its founder-signed genesis.
struct Founded {
    inception: ContextInception,
    ctx: ContextRef,
    genesis: SignedLogEntryV0,
}

fn found(domain: &str) -> Founded {
    let founders = [key(0xF1), key(0xF2), key(0xF3)];
    let v1 = ContextInceptionV1::new(
        founders.iter().map(pk).collect(),
        2,
        [0x1b; 32],
        served_domain_commitment(&GovernanceDomainId::new(domain)).unwrap(),
    )
    .unwrap();
    let ctx = v1.context_ref();
    let body = LogEntryV0 {
        context: ctx,
        seq: 1,
        prev: ctx.0,
        transition: TransitionV0::Genesis {
            writer: pk(&key(0xA1)),
            transition_rule: [0x5a; 32], // genesis replay never resolves rules
            effect_rule: [0x5a; 32],
            standing: vec![],
        },
    };
    let genesis = SignedLogEntryV0::genesis(body, &[&founders[0], &founders[1]]).unwrap();
    Founded {
        inception: ContextInception::V1(v1),
        ctx,
        genesis,
    }
}

fn binding_for(domain: &str, f: &Founded) -> InstitutionalAuthorityBindingV1 {
    InstitutionalAuthorityBindingV1::new(
        GovernanceDomainId::new(domain),
        f.ctx,
        f.genesis.body.head(),
        BindingBasisV1::DeclaredGenesis {
            declaration_decision_hash: [0x33; 32],
            mandate_id: [0x44; 32],
        },
    )
    .unwrap()
}

fn input(f: &Founded) -> AnchorDerivationInput<'_> {
    AnchorDerivationInput {
        inception: &f.inception,
        genesis: &f.genesis,
        min_seq: 1,
        pinned: None,
        rules: vec![rule()],
        recognition: RecognitionRequirement::RehearsalPermitted,
    }
}

fn fixture() -> (Arc<dyn Store>, InstitutionalAuthorityStore) {
    let raw: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
    (raw.clone(), InstitutionalAuthorityStore::new(raw))
}

/// Put a record under `domain`'s key with the raw store (the P2 LIMIT path).
fn raw_put(raw: &Arc<dyn Store>, domain: &str, bytes: &[u8]) {
    let key = authority_binding_key(&GovernanceDomainId::new(domain)).unwrap();
    raw.put(&key, bytes).unwrap();
}

#[test]
fn a_supported_stored_binding_derives_the_anchor() {
    let (raw, typed) = fixture();
    let f = found(D);
    raw_put(&raw, D, &binding_for(D, &f).encode());
    let derived = derive_anchor_from_store(&typed, &GovernanceDomainId::new(D), input(&f)).unwrap();
    assert_eq!(derived.domain(), &GovernanceDomainId::new(D));
    assert_eq!(derived.context(), f.ctx);
    assert_eq!(derived.genesis_head(), f.genesis.body.head());
}

#[test]
fn a_missing_binding_is_refused_not_defaulted() {
    let (_raw, typed) = fixture();
    let f = found(D);
    assert!(matches!(
        derive_anchor_from_store(&typed, &GovernanceDomainId::new(D), input(&f)),
        Err(StoredAnchorError::MissingBinding(d)) if d == GovernanceDomainId::new(D)
    ));
}

#[test]
fn corrupted_storage_is_refused() {
    let (raw, typed) = fixture();
    let f = found(D);
    let mut bytes = binding_for(D, &f).encode();
    bytes.push(0); // trailing byte: strict decode refuses
    raw_put(&raw, D, &bytes);
    assert!(matches!(
        derive_anchor_from_store(&typed, &GovernanceDomainId::new(D), input(&f)),
        Err(StoredAnchorError::Binding(AuthorityBindingError::Corrupt {
            reason: CorruptBinding::Undecodable(_),
            ..
        }))
    ));
}

/// A valid binding for OTHER copied under D's key is a transplanted record.
#[test]
fn a_transplanted_record_is_refused() {
    let (raw, typed) = fixture();
    let f = found(OTHER);
    raw_put(&raw, D, &binding_for(OTHER, &f).encode());
    assert!(matches!(
        derive_anchor_from_store(&typed, &GovernanceDomainId::new(D), input(&f)),
        Err(StoredAnchorError::Binding(AuthorityBindingError::Corrupt {
            reason: CorruptBinding::WrongDomain { .. },
            ..
        }))
    ));
}

/// A well-formed record forged with the raw store binds D to a context founded for OTHER. It
/// reads back as a binding (P2's LIMIT), but no anchor derives from it.
#[test]
fn a_forged_raw_store_binding_does_not_derive_an_anchor() {
    let (raw, typed) = fixture();
    let real = found(D);
    let rogue = found(OTHER);
    raw_put(&raw, D, &binding_for(D, &rogue).encode());
    assert!(typed
        .get_authority_binding(&GovernanceDomainId::new(D))
        .unwrap()
        .is_some());
    // Against D's real founding evidence: wrong context.
    assert!(matches!(
        derive_anchor_from_store(&typed, &GovernanceDomainId::new(D), input(&real)),
        Err(StoredAnchorError::Derivation(
            AnchorDerivationError::ContextMismatch
        ))
    ));
    // Against the rogue context's own evidence: it commits to OTHER, not D.
    assert!(matches!(
        derive_anchor_from_store(&typed, &GovernanceDomainId::new(D), input(&rogue)),
        Err(StoredAnchorError::Derivation(
            AnchorDerivationError::ServedSubjectMismatch
        ))
    ));
}

#[test]
fn an_unbindable_domain_id_is_refused() {
    let (_raw, typed) = fixture();
    let f = found(D);
    assert!(matches!(
        derive_anchor_from_store(&typed, &GovernanceDomainId::new(""), input(&f)),
        Err(StoredAnchorError::Binding(
            AuthorityBindingError::InvalidDomain(_)
        ))
    ));
}
