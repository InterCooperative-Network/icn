//! ADR-0014 Person-grantee enumeration must follow Principal identity, not
//! the DID spelling that happened to be persisted (#2627, N2-A M2).
//!
//! These fixtures run the **production** routes — the accepted-proposal
//! revocation seam and the live `MandateGate` — over a real `ReceiptStore`
//! on real sled. A hand-rolled lookup would only restate the reader under
//! test; the point is what an accepted governance decision actually does.

// Test-only: assertions and fixture setup panic on failure by design.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gateway::receipt_store::ReceiptStore;
use icn_governance::authority::{
    AuthorityClass, AuthorityGrant, AuthorityGrantId, DecisionProvenance, Grantee, GrantorEntityId,
    TypedScope,
};
use icn_governance::{GovernanceDomainId, Mandate, ProposalPayload, SdisProposal};
use icn_governance_actor::grant_minting::mint_and_persist_for_accepted;
use icn_governance_actor::mandate_gate::{
    DefaultMandateGate, MandateAct, MandateGate, MandateGateError, MandateRejection,
    MandateRequest, MandateTarget,
};
use icn_identity::Did;

fn temp_store() -> Arc<ReceiptStore> {
    Arc::new(ReceiptStore::new(
        sled::Config::new().temporary(true).open().unwrap(),
    ))
}

/// Two accepted spellings of one principal: the canonical base58btc form a
/// keypair emits, and the base16 form of the same 32 identifier bytes.
fn alias_pair() -> (Did, Did) {
    let canonical = icn_identity::KeyPair::generate().unwrap().did().clone();
    let bytes = canonical.identifier_bytes().unwrap();
    let alias = Did::from_str(&format!("did:icn:f{}", hex::encode(bytes))).unwrap();
    assert_ne!(canonical.as_str(), alias.as_str(), "spellings must differ");
    assert_eq!(canonical, alias, "one principal, two spellings");
    (canonical, alias)
}

const DOMAIN: &str = "coop:tech";

fn domain() -> GovernanceDomainId {
    GovernanceDomainId(DOMAIN.into())
}

fn grant_for(person: &Did, decision_hash: [u8; 32], valid_from: u64) -> AuthorityGrant {
    AuthorityGrant {
        id: AuthorityGrantId::new(),
        class: AuthorityClass::Execution,
        grantor: GrantorEntityId(DOMAIN.into()),
        grantee: Grantee::Person(person.clone()),
        scope: TypedScope {
            domain: Some(domain()),
            action_kind: vec!["domain_policy:adopt".into()],
            ..TypedScope::default()
        },
        granted_by: Some(DecisionProvenance {
            proposal_id: "p-origin".into(),
            decision_hash,
        }),
        valid_from,
        valid_until: None,
        revoked_at: None,
    }
}

/// **Defect fixture B — revocation.** A grant issued to a Person under one
/// accepted spelling must be reached by an accepted `RevokeAuthority`
/// decision naming that same Principal under another spelling. The SDIS
/// payload carries a DID, never a grant id, so by-grantee enumeration is
/// the only way the seam can find its target.
#[test]
fn alias_spelled_revocation_reaches_the_grant() {
    let store = temp_store();
    let (a, b) = alias_pair();

    let g = grant_for(&a, [0xe1u8; 32], 1_000);
    store.put_authority_grant(&g).unwrap();

    let outcome = mint_and_persist_for_accepted(
        store.as_ref(),
        "p-revoke",
        &domain(),
        [0xe2u8; 32],
        &ProposalPayload::Sdis {
            proposal: SdisProposal::RevokeAuthority {
                authority_did: b.clone(),
                reason: "test".into(),
                effective_at: None,
            },
        },
        2_000,
    )
    .unwrap();
    let _ = outcome;

    let after = store.get_authority_grant(&g.id).unwrap().unwrap();
    assert!(
        after.revoked_at.is_some(),
        "an accepted revocation naming this Principal must reach the grant \
         whatever spelling indexed it"
    );
}

/// Control for fixture B: the same route under the *same* spelling already
/// works, so the fixture above cannot pass by revoking everything.
#[test]
fn control_same_spelled_revocation_reaches_the_grant() {
    let store = temp_store();
    let (a, _b) = alias_pair();

    let g = grant_for(&a, [0xe3u8; 32], 1_000);
    store.put_authority_grant(&g).unwrap();

    mint_and_persist_for_accepted(
        store.as_ref(),
        "p-revoke-same",
        &domain(),
        [0xe4u8; 32],
        &ProposalPayload::Sdis {
            proposal: SdisProposal::RevokeAuthority {
                authority_did: a.clone(),
                reason: "test".into(),
                effective_at: None,
            },
        },
        2_000,
    )
    .unwrap();

    assert!(store
        .get_authority_grant(&g.id)
        .unwrap()
        .unwrap()
        .revoked_at
        .is_some());
}

/// Control for fixture B: a different Principal's grant is untouched.
#[test]
fn control_revocation_does_not_reach_another_principal() {
    let store = temp_store();
    let (a, _b) = alias_pair();
    let c = icn_identity::KeyPair::generate().unwrap().did().clone();

    let g = grant_for(&a, [0xe5u8; 32], 1_000);
    store.put_authority_grant(&g).unwrap();

    mint_and_persist_for_accepted(
        store.as_ref(),
        "p-revoke-other",
        &domain(),
        [0xe6u8; 32],
        &ProposalPayload::Sdis {
            proposal: SdisProposal::RevokeAuthority {
                authority_did: c,
                reason: "test".into(),
                effective_at: None,
            },
        },
        2_000,
    )
    .unwrap();

    assert!(
        store
            .get_authority_grant(&g.id)
            .unwrap()
            .unwrap()
            .revoked_at
            .is_none(),
        "another Principal's decision must not terminate this grant"
    );
}

/// Seed a full mandate + grant chain the domain-target gate can resolve.
fn seed_gate_chain(store: &ReceiptStore, person: &Did) -> AuthorityGrantId {
    let decision_hash = [0xf1u8; 32];
    let g = grant_for(person, decision_hash, 1_000);
    let mandate = Mandate::new(
        DecisionProvenance {
            proposal_id: "p-origin".into(),
            decision_hash,
        },
        [0xf2u8; 32],
        vec![g.id.clone()],
        Some(person.clone()),
        None,
        1_000,
    )
    .unwrap();
    store
        .put_mandate_with_grants_atomic(&mandate, std::slice::from_ref(&g))
        .unwrap();
    g.id
}

fn gate_request(actor: &Did) -> MandateRequest {
    MandateRequest {
        actor: actor.clone(),
        domain: domain(),
        act: MandateAct::AdoptDomainPolicy,
        target: MandateTarget::Domain(domain()),
        at: 2_000,
    }
}

/// **Defect fixture C — authority lookup.** The live `MandateGate`
/// domain-target path resolves actor-first through by-grantee enumeration.
/// Its verdict must not depend on which spelling names the actor.
#[test]
fn mandate_gate_is_representation_invariant_for_person_actors() {
    let store = temp_store();
    let (a, b) = alias_pair();
    seed_gate_chain(store.as_ref(), &a);

    let gate = DefaultMandateGate::new(store.clone());

    let under_a = gate.require(&gate_request(&a));
    assert!(
        under_a.is_ok(),
        "control: the issuing spelling must resolve; got {under_a:?}"
    );

    let under_b = gate.require(&gate_request(&b));
    assert!(
        under_b.is_ok(),
        "the same Principal under another spelling must reach the same \
         authority; got {under_b:?}"
    );
}

/// Control: a genuinely different Principal still gets `NoMandate`.
#[test]
fn control_mandate_gate_refuses_a_different_principal() {
    let store = temp_store();
    let (a, _b) = alias_pair();
    seed_gate_chain(store.as_ref(), &a);
    let c = icn_identity::KeyPair::generate().unwrap().did().clone();

    let gate = DefaultMandateGate::new(store.clone());
    match gate.require(&gate_request(&c)) {
        Err(MandateGateError::Rejected(r)) => assert_eq!(r, MandateRejection::NoMandate),
        other => panic!("expected NoMandate for an unrelated principal; got {other:?}"),
    }
}
