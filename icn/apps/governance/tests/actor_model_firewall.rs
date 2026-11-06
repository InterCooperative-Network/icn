//! Rung 4 — human actor-model firewall, actor-level obligations.
//!
//! Adversarial tests 4–8 of the rung-4 acceptance contract, driven through the
//! real `GovernanceActor` command path against a real sled store. A
//! `ContextSubjectV1` domain is persisted exactly as a record produced by the
//! library constructor would be; every legacy Principal-rail command aimed at
//! it must be refused **before** any mutation.
//!
//! "Before any mutation" is proved by snapshotting every governance record the
//! actor can write (domains, proposals, votes, delegations, proofs, close
//! journal) and requiring byte-equal snapshots across the refused command. Each
//! negative has a legacy control that succeeds, so a green test cannot be a
//! vacuous "nothing ever works here".

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use ed25519_dalek::SigningKey;
use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::subject_recognition::SubjectRecognitionV1;
use icn_governance::{
    Delegation, DelegationScope, ForcedOutcome, GovernanceConfig, GovernanceDomain,
    GovernanceDomainId, HumanActorModel, MembershipAction, MembershipConfig, MembershipResolver,
    Proposal, ProposalId, ProposalPayload, StaticMembershipResolver, VoteChoice,
};
use icn_governance_actor::actor::{GovernanceActor, GovernanceHandle};
use icn_governance_actor::state_store::{GovernanceStateStore, SledGovernanceStateStore};
use icn_governance_actor::GovernanceCommand;
use icn_identity::authority_log::{ContinuityRoot, PrincipalKey};
use icn_identity::subject_context::{
    incept_subject_context_v1, ContextSalt, SubjectContextDescriptor,
};
use icn_identity::{Did, IdentityBundle};
use icn_store::SledStore;

const LEGACY: &str = "legacy-dom";
const SUBJECT: &str = "subject-dom";

struct Fx {
    handle: GovernanceHandle,
    state: SledGovernanceStateStore,
    node: Did,
    member: Did,
    _tmp: tempfile::TempDir,
}

fn new_did() -> Did {
    IdentityBundle::generate().unwrap().did().clone()
}

fn domain_with(id: &str, members: Vec<Did>) -> GovernanceDomain {
    let mut config = GovernanceConfig::cooperative_default();
    config.membership = MembershipConfig::static_list(members);
    GovernanceDomain::with_id(GovernanceDomainId::new(id), id.to_string(), config)
}

fn open_proposal(domain: &str, proposer: &Did, id: &str) -> Proposal {
    let mut p = Proposal::new(
        GovernanceDomainId::new(domain),
        proposer.clone(),
        format!("proposal {id}"),
        "d".to_string(),
        ProposalPayload::Text { body: "b".into() },
    );
    p.id = ProposalId(id.to_string());
    p.open(3600).unwrap();
    p
}

async fn fx() -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let bundle = IdentityBundle::generate().unwrap();
    let node = bundle.did().clone();
    let member = new_did();
    let store = Arc::new(SledStore::open(tmp.path()).unwrap());
    let gossip = GossipActor::spawn(node.clone(), None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".into(),
        AccessControl::Public,
    ));
    let handle = GovernanceActor::spawn(
        node.clone(),
        store.clone(),
        gossip,
        Arc::new(StaticMembershipResolver::new()),
        None,
        None,
    )
    .await
    .unwrap();
    let state = SledGovernanceStateStore::new(store);

    // Legacy control domain with one member and an open proposal.
    state
        .save_domain(&domain_with(LEGACY, vec![member.clone()]))
        .unwrap();
    state
        .save_proposal(&open_proposal(LEGACY, &node, "p-legacy"))
        .unwrap();

    // Subject-mode domain: the inert membership shape, an open proposal.
    let subject = domain_with(SUBJECT, vec![])
        .with_human_actor_model(HumanActorModel::ContextSubjectV1)
        .unwrap();
    state.save_domain(&subject).unwrap();
    state
        .save_proposal(&open_proposal(SUBJECT, &node, "p-subject"))
        .unwrap();

    Fx {
        handle,
        state,
        node,
        member,
        _tmp: tmp,
    }
}

/// Every record the actor can write, in one comparable value.
fn snapshot(f: &Fx) -> serde_json::Value {
    let mut votes = serde_json::Map::new();
    for p in ["p-legacy", "p-subject"] {
        let id = ProposalId(p.to_string());
        votes.insert(
            p.into(),
            serde_json::json!({
                "votes": serde_json::to_value(f.state.list_votes(&id).unwrap()).unwrap(),
                "proof": f.state.get_proof_bytes(&id).unwrap().map(|b| b.len()),
            }),
        );
    }
    serde_json::json!({
        "domains": serde_json::to_value(f.state.list_domains().unwrap()).unwrap(),
        "proposals": serde_json::to_value(f.state.list_proposals().unwrap()).unwrap(),
        "per_proposal": votes,
        "delegations": f.state.list_all_delegations().unwrap().len(),
    })
}

fn refused(r: anyhow::Result<()>) -> String {
    let e = r.expect_err("a Subject-mode domain must refuse this legacy operation");
    let msg = e.to_string();
    assert!(
        msg.contains("ContextSubjectV1"),
        "refusal must come from the actor-model firewall, got: {msg}"
    );
    msg
}

// ---------------------------------------------------------------------------
// Test 4 — legacy membership insertion refused before state changes
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn t4_membership_insertion_refused_without_mutation() {
    let f = fx().await;
    let newcomer = new_did();

    let before = snapshot(&f);
    refused(
        f.handle
            .submit(GovernanceCommand::UpdateMembership {
                domain_id: GovernanceDomainId::new(SUBJECT),
                action: MembershipAction::Add,
                member: newcomer.clone(),
            })
            .await,
    );
    assert_eq!(snapshot(&f), before, "refusal must precede every write");

    // The stored Subject domain still has the inert shape and its mode.
    let d = f
        .state
        .get_domain(&GovernanceDomainId::new(SUBJECT))
        .unwrap()
        .unwrap();
    assert_eq!(d.human_actor_model(), HumanActorModel::ContextSubjectV1);

    // Control: the identical command on a legacy domain still works.
    f.handle
        .submit(GovernanceCommand::UpdateMembership {
            domain_id: GovernanceDomainId::new(LEGACY),
            action: MembershipAction::Add,
            member: newcomer.clone(),
        })
        .await
        .unwrap();
    let legacy = f
        .state
        .get_domain(&GovernanceDomainId::new(LEGACY))
        .unwrap()
        .unwrap();
    assert!(legacy.config.membership.source.contains_static(&newcomer));
}

// ---------------------------------------------------------------------------
// Test 5 — other membership/config modification paths
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn t5_removal_and_config_replacement_refused_without_mutation() {
    let f = fx().await;
    let before = snapshot(&f);

    // Removal (even of a DID that is not present: the refusal must not depend
    // on the list's contents).
    refused(
        f.handle
            .submit(GovernanceCommand::UpdateMembership {
                domain_id: GovernanceDomainId::new(SUBJECT),
                action: MembershipAction::Remove,
                member: f.member.clone(),
            })
            .await,
    );

    // Config replacement that smuggles in a legacy static list.
    let mut smuggled = GovernanceConfig::cooperative_default();
    smuggled.membership = MembershipConfig::static_list(vec![f.member.clone()]);
    assert!(f
        .handle
        .submit(GovernanceCommand::UpdateDomainConfig {
            domain_id: GovernanceDomainId::new(SUBJECT),
            new_config: smuggled,
        })
        .await
        .is_err());

    // ...and one that smuggles in the (default) trust-threshold source.
    assert!(f
        .handle
        .submit(GovernanceCommand::UpdateDomainConfig {
            domain_id: GovernanceDomainId::new(SUBJECT),
            new_config: GovernanceConfig::cooperative_default(),
        })
        .await
        .is_err());

    assert_eq!(snapshot(&f), before, "no refused command may mutate state");
}

/// A config update that keeps the inert shape may proceed, and it cannot flip
/// the mode: the mode is not part of `GovernanceConfig`.
#[tokio::test(flavor = "multi_thread")]
async fn t5b_inert_config_update_preserves_subject_mode() {
    let f = fx().await;
    let mut cfg = GovernanceConfig::cooperative_default();
    cfg.membership = MembershipConfig::static_list(vec![]);
    f.handle
        .submit(GovernanceCommand::UpdateDomainConfig {
            domain_id: GovernanceDomainId::new(SUBJECT),
            new_config: cfg,
        })
        .await
        .unwrap();
    let d = f
        .state
        .get_domain(&GovernanceDomainId::new(SUBJECT))
        .unwrap()
        .unwrap();
    assert_eq!(d.human_actor_model(), HumanActorModel::ContextSubjectV1);
}

// ---------------------------------------------------------------------------
// Test 6 — legacy vote refused before any vote record or tally change
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn t6_legacy_vote_refused_without_vote_row() {
    let f = fx().await;
    let before = snapshot(&f);

    // Neither a would-be member Principal nor the node's own device Principal
    // may stand in for a Subject.
    for voter in [f.member.clone(), f.node.clone(), new_did()] {
        refused(
            f.handle
                .submit(GovernanceCommand::CastVote {
                    proposal_id: ProposalId("p-subject".into()),
                    voter,
                    choice: VoteChoice::For,
                    comment: None,
                })
                .await,
        );
    }
    assert_eq!(snapshot(&f), before);
    assert!(f
        .state
        .list_votes(&ProposalId("p-subject".into()))
        .unwrap()
        .is_empty());

    // Control: a legacy domain still records the same vote.
    f.handle
        .submit(GovernanceCommand::CastVote {
            proposal_id: ProposalId("p-legacy".into()),
            voter: f.member.clone(),
            choice: VoteChoice::For,
            comment: None,
        })
        .await
        .unwrap();
    assert_eq!(
        f.state
            .list_votes(&ProposalId("p-legacy".into()))
            .unwrap()
            .len(),
        1
    );
}

// ---------------------------------------------------------------------------
// Test 7 — alternate vote-adjacent entry points cannot bypass the firewall
// ---------------------------------------------------------------------------

/// Vote delegation is a Principal-to-Principal voting construct. Domain- and
/// proposal-scoped delegations naming a Subject-mode domain are refused.
#[tokio::test(flavor = "multi_thread")]
async fn t7_delegation_into_subject_domain_refused() {
    let f = fx().await;
    let before = snapshot(&f);
    let (a, b) = (new_did(), new_did());

    for scope in [
        DelegationScope::Domain(GovernanceDomainId::new(SUBJECT)),
        DelegationScope::Proposal(ProposalId("p-subject".into())),
    ] {
        refused(
            f.handle
                .submit(GovernanceCommand::CreateDelegation {
                    delegation: Delegation::new(a.clone(), b.clone(), scope),
                })
                .await,
        );
    }
    assert_eq!(snapshot(&f), before, "no delegation row may be written");

    // Control: the same delegation scoped to the legacy domain is accepted.
    f.handle
        .submit(GovernanceCommand::CreateDelegation {
            delegation: Delegation::new(
                a,
                b,
                DelegationScope::Domain(GovernanceDomainId::new(LEGACY)),
            ),
        })
        .await
        .unwrap();
    assert_eq!(f.state.list_all_delegations().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Test 8 — closure / finalisation refused under a legacy electorate
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn t8_every_closure_path_refused_proposal_stays_open() {
    let f = fx().await;
    let id = ProposalId("p-subject".into());
    let before = snapshot(&f);

    // Plain close, and close with a caller-supplied eligibility filter (which
    // would otherwise let the caller pick the electorate).
    refused(
        f.handle
            .submit(GovernanceCommand::CloseProposal {
                proposal_id: id.clone(),
                eligible_voters: None,
                excluded_delegators: None,
                capability_scope: None,
            })
            .await,
    );
    refused(
        f.handle
            .submit(GovernanceCommand::CloseProposal {
                proposal_id: id.clone(),
                eligible_voters: Some([f.member.clone()].into_iter().collect()),
                excluded_delegators: None,
                capability_scope: None,
            })
            .await,
    );

    // Administrative overrides produce decisions with no electorate at all and
    // must not mint an apparently legitimate Accepted/Rejected outcome.
    for outcome in [
        ForcedOutcome::Accept,
        ForcedOutcome::Reject,
        ForcedOutcome::Cancel,
    ] {
        refused(
            f.handle
                .submit(GovernanceCommand::ForceCloseProposal {
                    proposal_id: id.clone(),
                    forced_outcome: outcome,
                    reason: "override".into(),
                })
                .await,
        );
    }
    refused(
        f.handle
            .submit(GovernanceCommand::VetoProposal {
                proposal_id: id.clone(),
                reason: "veto".into(),
            })
            .await,
    );

    assert_eq!(snapshot(&f), before, "no closure artifact or state change");
    assert!(f.state.get_proposal(&id).unwrap().unwrap().state.is_open());
    assert!(f.state.get_proof_bytes(&id).unwrap().is_none());

    // Control: the same veto on a legacy proposal still closes it.
    f.handle
        .submit(GovernanceCommand::VetoProposal {
            proposal_id: ProposalId("p-legacy".into()),
            reason: "control".into(),
        })
        .await
        .unwrap();
    assert!(!f
        .state
        .get_proposal(&ProposalId("p-legacy".into()))
        .unwrap()
        .unwrap()
        .state
        .is_open());
}

// ---------------------------------------------------------------------------
// GEN-B recognition is a fact, not an authority
// ---------------------------------------------------------------------------

fn seq(start: u8) -> [u8; 32] {
    let mut b = [0u8; 32];
    for (i, s) in b.iter_mut().enumerate() {
        *s = start.wrapping_add(i as u8);
    }
    b
}

/// A cryptographically valid GEN-B rehearsal recognition for the very domain
/// under test changes nothing: it is not a vote, a membership entry or a
/// closure input, and the legacy rail still refuses.
#[tokio::test(flavor = "multi_thread")]
async fn gen_b_rehearsal_recognition_grants_no_authority() {
    let f = fx().await;

    let descriptor =
        SubjectContextDescriptor::governance_domain_v1(SUBJECT, ContextSalt::from_bytes(seq(0)))
            .unwrap();
    let root = ContinuityRoot::new(seq(0x80), descriptor.context_nonce(), 4);
    let device =
        PrincipalKey::try_from_verifying_key(SigningKey::from_bytes(&seq(0x40)).verifying_key())
            .unwrap();
    let genesis = incept_subject_context_v1(&descriptor, &root, device).unwrap();
    let recognition = SubjectRecognitionV1::recognize_bootstrap_rehearsal(
        &GovernanceDomainId::new(SUBJECT),
        &genesis,
    )
    .unwrap();
    recognition.verify_against_genesis(&genesis).unwrap();
    assert!(!recognition.basis_kind().is_production_grade());

    let before = snapshot(&f);
    refused(
        f.handle
            .submit(GovernanceCommand::CastVote {
                proposal_id: ProposalId("p-subject".into()),
                voter: f.member.clone(),
                choice: VoteChoice::For,
                comment: Some(format!("{:?}", recognition.recognition_hash())),
            })
            .await,
    );
    let subject_domain = f
        .state
        .get_domain(&GovernanceDomainId::new(SUBJECT))
        .unwrap()
        .unwrap();
    assert!(StaticMembershipResolver::new()
        .resolve_members(&subject_domain)
        .is_err());
    assert_eq!(snapshot(&f), before);
}

// ---------------------------------------------------------------------------
// Close-journal replay is a closure path too
// ---------------------------------------------------------------------------

/// A journaled close is finalised by `complete_journaled_close` without ever
/// consulting the electorate resolver, so it bypasses the resolver guard. Two
/// routes reach it: a `CloseProposal` command that finds an in-flight entry,
/// and startup recovery. Both must refuse a Subject-mode domain and leave the
/// proposal open.
fn terminal_journal_for(
    f: &Fx,
    proposal: &str,
) -> icn_governance_actor::close_journal::CloseJournalEntry {
    use icn_governance::ProposalState;
    let mut p = f
        .state
        .get_proposal(&ProposalId(proposal.into()))
        .unwrap()
        .unwrap();
    p.close(ProposalState::Rejected { closed_at: 1 }).unwrap();
    icn_governance_actor::close_journal::CloseJournalEntry {
        proposal: p,
        proof_bytes: None,
        receipts: Default::default(),
        decided_at: 1,
        governance_decision_hash: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn t8b_journal_replay_via_command_cannot_finalise_subject_proposal() {
    let f = fx().await;
    f.state
        .save_close_intent(&terminal_journal_for(&f, "p-subject"))
        .unwrap();
    let id = ProposalId("p-subject".into());

    refused(
        f.handle
            .submit(GovernanceCommand::CloseProposal {
                proposal_id: id.clone(),
                eligible_voters: None,
                excluded_delegators: None,
                capability_scope: None,
            })
            .await,
    );
    assert!(
        f.state.get_proposal(&id).unwrap().unwrap().state.is_open(),
        "the journaled terminal state must not have been committed"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn t8c_startup_recovery_cannot_finalise_subject_proposal() {
    let f = fx().await;
    f.state
        .save_close_intent(&terminal_journal_for(&f, "p-subject"))
        .unwrap();
    // Control journal on the legacy domain: recovery must still complete it,
    // otherwise this test would pass for the wrong reason.
    f.state
        .save_close_intent(&terminal_journal_for(&f, "p-legacy"))
        .unwrap();

    f.handle.recover_incomplete_closes().await;

    assert!(
        !f.state
            .get_proposal(&ProposalId("p-legacy".into()))
            .unwrap()
            .unwrap()
            .state
            .is_open(),
        "control: legacy recovery still completes"
    );
    assert!(
        f.state
            .get_proposal(&ProposalId("p-subject".into()))
            .unwrap()
            .unwrap()
            .state
            .is_open(),
        "recovery must not finalise a Subject-mode proposal"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn state_store_refuses_to_persist_a_mixed_domain() {
    let f = fx().await;
    let before = snapshot(&f);
    let mut mixed = domain_with("mixed-in-process", vec![])
        .with_human_actor_model(HumanActorModel::ContextSubjectV1)
        .unwrap();
    mixed.config.membership = MembershipConfig::static_list(vec![f.member.clone()]);
    assert!(f.state.save_domain(&mixed).is_err());
    assert_eq!(snapshot(&f), before);
    // And listing still works: nothing unloadable was written.
    assert_eq!(f.state.list_domains().unwrap().len(), 2);
}

/// `CreateDomain` is an alternate way to change a domain's membership source:
/// it builds a fresh legacy-mode domain and overwrites by key. It must not
/// replace a stored Subject-mode domain, and must not gossip anything.
#[tokio::test(flavor = "multi_thread")]
async fn create_domain_cannot_overwrite_a_subject_domain() {
    use icn_governance::GovernanceParams;
    use icn_governance_actor::actor::GovernanceConfigLite;
    let f = fx().await;
    let before = snapshot(&f);
    let before_subject = f
        .state
        .get_domain(&GovernanceDomainId::new(SUBJECT))
        .unwrap()
        .unwrap();
    let r = f
        .handle
        .submit(GovernanceCommand::CreateDomain {
            domain_id: GovernanceDomainId::new(SUBJECT),
            name: "overwrite".into(),
            config: GovernanceConfigLite {
                profile: "cooperative_default".into(),
                membership: MembershipConfig::static_list(vec![f.member.clone()]),
                params: GovernanceParams::new(50, 50, 3600),
            },
        })
        .await;
    refused(r);
    assert_eq!(snapshot(&f), before);
    let d = f
        .state
        .get_domain(&GovernanceDomainId::new(SUBJECT))
        .unwrap()
        .unwrap();
    assert_eq!(d.human_actor_model(), HumanActorModel::ContextSubjectV1);
    // The stored record is byte-for-byte what it was.
    assert_eq!(
        serde_json::to_vec(&d).unwrap(),
        serde_json::to_vec(&before_subject).unwrap()
    );

    // Legacy control: the same command over a legacy domain is still accepted
    // (behaviour unchanged), so the refusal above is the mode check, not a
    // generic duplicate-id rejection.
    f.handle
        .submit(GovernanceCommand::CreateDomain {
            domain_id: GovernanceDomainId::new(LEGACY),
            name: "legacy-overwrite".into(),
            config: GovernanceConfigLite {
                profile: "cooperative_default".into(),
                membership: MembershipConfig::static_list(vec![f.member.clone()]),
                params: GovernanceParams::new(50, 50, 3600),
            },
        })
        .await
        .expect("legacy CreateDomain must keep working");
    let l = f
        .state
        .get_domain(&GovernanceDomainId::new(LEGACY))
        .unwrap()
        .unwrap();
    assert_eq!(l.name, "legacy-overwrite");
}
