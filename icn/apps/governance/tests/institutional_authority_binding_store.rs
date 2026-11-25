//! KNOWN LIMITS of the set-once institutional authority binding (P2, design spec §2.2 and Part 6).
//! EXPERIMENTAL, unratified.
//!
//! `InstitutionalAuthorityStore::establish_authority_binding` never overwrites, and it is the only
//! typed writer. That is a rule of the TYPED API. Anything holding the raw `Arc<dyn Store>` for
//! the same database can write, replace or delete the key, exactly as
//! `raw_store_authority_limits.rs` pins for `save_domain`. These probes record that, so nobody
//! mistakes a stored binding for proof of legitimacy: a reader gets a *record*, and the consumer
//! that turns it into authority (P3) must re-check it against the context's inception.
//!
//! This is an integration test, so it sees exactly a dependent crate's surface. It cannot make a
//! `VerifiedAuthorityBindingV1` (no public constructor exists in this slice), so it never calls the
//! typed writer; the typed writer's behavior is pinned by the module's own unit tests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use icn_governance::institutional_domain::authority_binding::{
    authority_binding_key, BindingBasisV1, ContextRef, InstitutionalAuthorityBindingV1,
};
use icn_governance::GovernanceDomainId;
use icn_governance_actor::institutional_authority_store::{
    AuthorityBindingError, CorruptBinding, InstitutionalAuthorityStore,
};
use icn_store::{CompareAndPut, SledStore, Store};

fn binding(domain: &str, ctx: u8) -> InstitutionalAuthorityBindingV1 {
    InstitutionalAuthorityBindingV1::new(
        GovernanceDomainId::new(domain),
        ContextRef([ctx; 32]),
        [0x22; 32],
        BindingBasisV1::DeclaredGenesis {
            declaration_decision_hash: [0x33; 32],
            mandate_id: [0x44; 32],
        },
    )
    .unwrap()
}

fn fixture() -> (Arc<dyn Store>, InstitutionalAuthorityStore) {
    let raw: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
    (raw.clone(), InstitutionalAuthorityStore::new(raw))
}

/// LIMIT: a raw `put` of well-formed bytes is read back as a binding. The typed reader cannot tell
/// who wrote it, which is why a stored binding is not legitimacy evidence.
#[test]
fn raw_put_of_a_well_formed_record_reads_back_as_a_binding() {
    let (raw, typed) = fixture();
    let forged = binding("coop-a", 0x66);
    let key = authority_binding_key(forged.domain()).unwrap();
    raw.put(&key, &forged.encode()).unwrap();
    assert_eq!(
        typed.get_authority_binding(forged.domain()).unwrap(),
        Some(forged)
    );
}

/// LIMIT: set-once is a typed rule. A raw `put` replaces, and a raw `delete` unbinds.
#[test]
fn raw_put_replaces_and_raw_delete_unbinds() {
    let (raw, typed) = fixture();
    let first = binding("coop-a", 1);
    let second = binding("coop-a", 2);
    let key = authority_binding_key(first.domain()).unwrap();
    raw.put(&key, &first.encode()).unwrap();
    raw.put(&key, &second.encode()).unwrap();
    assert_eq!(
        typed.get_authority_binding(first.domain()).unwrap(),
        Some(second.clone())
    );
    // A raw CAS with the right expectation also replaces.
    assert!(matches!(
        raw.compare_and_put(&key, Some(&second.encode()), &first.encode())
            .unwrap(),
        CompareAndPut::Applied
    ));
    assert_eq!(
        typed.get_authority_binding(first.domain()).unwrap(),
        Some(first.clone())
    );
    raw.delete(&key).unwrap();
    assert_eq!(typed.get_authority_binding(first.domain()).unwrap(), None);
}

/// The CONTROL: whatever a raw writer puts that is not a valid binding for exactly this domain is
/// refused on read as Corrupt, never reported as unbound.
#[test]
fn raw_garbage_and_transplants_read_as_corrupt_not_unbound() {
    let (raw, typed) = fixture();
    let d = GovernanceDomainId::new("coop-a");
    let key = authority_binding_key(&d).unwrap();
    raw.put(&key, b"not a binding").unwrap();
    assert!(matches!(
        typed.get_authority_binding(&d),
        Err(AuthorityBindingError::Corrupt {
            reason: CorruptBinding::Undecodable(_),
            ..
        })
    ));
    raw.put(&key, &binding("coop-b", 1).encode()).unwrap();
    assert!(matches!(
        typed.get_authority_binding(&d),
        Err(AuthorityBindingError::Corrupt {
            reason: CorruptBinding::WrongDomain { .. },
            ..
        })
    ));
}

/// The binding key never appears under the prefixes the governance state store scans, so a typed
/// scan of domains cannot pick a binding up (or trip over one).
#[test]
fn bindings_are_invisible_to_governance_domain_scans() {
    let (raw, _typed) = fixture();
    let b = binding("coop-a", 1);
    raw.put(&authority_binding_key(b.domain()).unwrap(), &b.encode())
        .unwrap();
    // Every key prefix `state_store.rs` writes or scans (`gov:vote:<p>:` is under `gov:vote:`).
    for prefix in [
        b"gov:domain:".as_slice(),
        b"gov:institutional-domain:",
        b"gov:domain-policy-body:",
        b"gov:proposal:",
        b"gov:vote:",
        b"gov:delegation:",
        b"gov:close-intent:",
    ] {
        assert!(raw.scan(prefix).unwrap().is_empty(), "{prefix:?}");
    }
}
