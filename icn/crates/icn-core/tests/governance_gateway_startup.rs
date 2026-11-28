//! Supervisor gateway startup and governance evidence readiness (M-C1 S1).
//!
//! On a node with an identity the supervisor — not the gateway — installs the
//! governance receipt store and dispatch-evidence backend and runs close
//! recovery (`establish_governance_evidence`), over the store the daemon opened
//! once, before the gateway starts. Readiness is keyed to that outcome:
//!
//! * established: governance is Ready and scheduled closes write evidence,
//!   whatever then happens to the gateway (disabled, misconfigured, failed);
//! * the store cannot be opened, or none was injected: scheduled work is held
//!   (overdue proposals preserved) and terminal transitions are refused,
//!   while unrelated activity continues;
//! * the gateway never settles governance, and a gateway given the actor
//!   without the shared store refuses to start.
//!
//! Before S1 these cases were keyed to the gateway's own startup outcome
//! (disabled gateway = NotProvisioned; failed gateway = held). Each test below
//! states the new expectation it replaces.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use icn_core::config::GatewayConfig;
use icn_core::supervisor::init_gateway::{
    spawn_gateway, GatewayHandles, GatewayStartup, GatewayStartupFailure,
};
use icn_core::supervisor::init_governance_evidence::{
    establish_governance_evidence, EvidenceInitFailure, GovernanceEvidenceInit,
    GovernanceEvidenceInputs, GovernanceEvidenceSettled, GovernanceEvidenceStore,
};
use icn_gateway::shared_store::SharedGatewayStore;
use icn_gossip::{AccessControl, GossipActor, Topic};
use icn_governance::{
    GovernanceDomainId, GovernanceError, GovernanceOps, GovernanceParams, MembershipConfig,
    Proposal, ProposalId, ProposalPayload, ProposalState, StaticMembershipResolver, Vote,
    VoteChoice,
};
use icn_governance_actor::state_store::{GovernanceStateStore, SledGovernanceStateStore};
use icn_governance_actor::{
    EvidenceReadiness, GovernanceActor, GovernanceCommand, GovernanceHandle,
};
use icn_identity::{Did, IdentityBundle};
use icn_store::SledStore;

const DOMAIN: &str = "gateway-startup-dom";
/// The scheduler ticks every 10 s; allow two ticks plus slack.
const CLOSE_WAIT: Duration = Duration::from_secs(25);

fn new_did() -> Did {
    IdentityBundle::generate().unwrap().did().clone()
}

fn now() -> u64 {
    icn_time::current_timestamp_secs()
}

async fn spawn_actor(store: Arc<SledStore>) -> GovernanceHandle {
    let node = new_did();
    let gossip = GossipActor::spawn(node.clone(), None);
    gossip.write().await.create_topic(Topic::new(
        "governance:proposal".into(),
        AccessControl::Public,
    ));
    GovernanceActor::spawn(
        node,
        store,
        gossip,
        Arc::new(StaticMembershipResolver::new()),
        None,
        None,
    )
    .await
    .unwrap()
}

/// A governance store holding one static-list domain, an overdue Open
/// proposal with an accepting vote, and a live Open proposal.
struct Fixture {
    _tmp: tempfile::TempDir,
    handle: GovernanceHandle,
    voter: Did,
    overdue: ProposalId,
    live: ProposalId,
}

async fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let voter = new_did();
    {
        let store = Arc::new(SledStore::open(tmp.path()).unwrap());
        let h = spawn_actor(store).await;
        GovernanceOps::create_domain(
            &h,
            GovernanceDomainId(DOMAIN.to_string()),
            "Gateway startup".to_string(),
            "cooperative_default".to_string(),
            GovernanceParams::new(50, 50, 3600),
            MembershipConfig::static_list(vec![voter.clone()]),
        )
        .await
        .unwrap();
        h.shutdown().await;
    }
    let t = now();
    let overdue = plant_open(tmp.path(), &voter, t - 7200, t - 60);
    plant_vote(tmp.path(), &overdue, &voter);
    let live = plant_open(tmp.path(), &voter, t - 60, t + 3600);
    let handle = spawn_actor(Arc::new(SledStore::open(tmp.path()).unwrap())).await;
    Fixture {
        _tmp: tmp,
        handle,
        voter,
        overdue,
        live,
    }
}

fn plant_open(
    path: &std::path::Path,
    proposer: &Did,
    opened_at: u64,
    closes_at: u64,
) -> ProposalId {
    let state = SledGovernanceStateStore::new(Arc::new(SledStore::open(path).unwrap()));
    let mut p = Proposal::new(
        GovernanceDomainId(DOMAIN.to_string()),
        proposer.clone(),
        "startup".into(),
        "d".into(),
        ProposalPayload::Text { body: "b".into() },
    );
    p.state = ProposalState::Open {
        opened_at,
        closes_at,
    };
    state.create_proposal(&p).unwrap();
    state.flush().unwrap();
    p.id
}

fn plant_vote(path: &std::path::Path, id: &ProposalId, voter: &Did) {
    let state = SledGovernanceStateStore::new(Arc::new(SledStore::open(path).unwrap()));
    state
        .save_vote(id, &Vote::new(id.clone(), voter.clone(), VoteChoice::For))
        .unwrap();
    state.flush().unwrap();
}

async fn state_of(h: &GovernanceHandle, id: &ProposalId) -> ProposalState {
    h.get_proposal(id).await.unwrap().unwrap().state
}

async fn wait_closed(h: &GovernanceHandle, id: &ProposalId) -> ProposalState {
    let deadline = tokio::time::Instant::now() + CLOSE_WAIT;
    loop {
        let s = state_of(h, id).await;
        if !s.is_open() || tokio::time::Instant::now() >= deadline {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn handles_with(
    h: &GovernanceHandle,
    shared: Option<SharedGatewayStore>,
    settled: GovernanceEvidenceSettled,
) -> GatewayHandles {
    GatewayHandles {
        governance_actor_handle: Some(h.clone()),
        event_broadcaster: Some(Arc::new(icn_gateway::EventBroadcaster::new())),
        shared_store: shared,
        governance_evidence: Some(settled),
        ..GatewayHandles::default()
    }
}

/// The daemon's single open of `<data>/gateway_store` and the supervisor's
/// evidence initialization over it, as on an identity node.
async fn establish(
    fx: &Fixture,
    data: &std::path::Path,
) -> (
    GovernanceEvidenceInit,
    GovernanceEvidenceSettled,
    Option<SharedGatewayStore>,
) {
    let (store, shared) = match SharedGatewayStore::open(data) {
        Ok(shared) => {
            let receipts = shared.receipt_store().clone();
            let backfill = receipts.clone();
            (
                Ok(GovernanceEvidenceStore {
                    receipt_backend: receipts,
                    backfill_by_grantee_index: Arc::new(move || {
                        backfill.backfill_grant_by_grantee_index()
                    }),
                }),
                Some(shared),
            )
        }
        Err(e) => (Err(e), None),
    };
    let (init, readiness) = establish_governance_evidence(GovernanceEvidenceInputs {
        governance: fx.handle.clone(),
        store: Some(store),
        dispatch_evidence_sink: None,
        execution_store: None,
        post_backfill_cleanup: None,
    })
    .await;
    (init, readiness, shared)
}

fn enabled_config() -> GatewayConfig {
    GatewayConfig {
        enabled: true,
        jwt_secret: "s".repeat(32),
        bind_addr: "127.0.0.1:0".to_string(),
        ..GatewayConfig::default()
    }
}

/// Held: past at least one scheduler tick the overdue proposal is still Open,
/// a manual close is refused with the typed evidence error, and a vote inside
/// the live window still lands.
async fn assert_held_and_fail_closed(fx: &Fixture) {
    tokio::time::sleep(Duration::from_secs(12)).await;
    assert!(
        state_of(&fx.handle, &fx.overdue).await.is_open(),
        "overdue proposal must be preserved"
    );
    let err = fx
        .handle
        .submit(GovernanceCommand::CloseProposal {
            proposal_id: fx.overdue.clone(),
            eligible_voters: None,
            excluded_delegators: None,
            capability_scope: None,
        })
        .await
        .expect_err("a close must be refused without its evidence path");
    assert!(
        matches!(
            err.downcast::<GovernanceError>(),
            Ok(GovernanceError::EvidenceUnavailable(_))
        ),
        "refusal must be the typed evidence error"
    );
    assert!(state_of(&fx.handle, &fx.overdue).await.is_open());
    let ops: &dyn GovernanceOps = &fx.handle;
    ops.cast_vote(fx.live.clone(), fx.voter.clone(), VoteChoice::For, None)
        .await
        .expect("unrelated governance activity continues");
}

/// Was: a disabled gateway declared NotProvisioned. Now: on an identity node
/// evidence is established by the supervisor, so with the gateway disabled
/// governance is Ready and the overdue close completes with evidence.
#[tokio::test]
async fn a_disabled_gateway_leaves_established_governance_ready_and_closes_proceed() {
    let fx = fixture().await;
    let data = tempfile::tempdir().unwrap();
    let (init, readiness, shared) = establish(&fx, data.path()).await;
    assert!(matches!(init, GovernanceEvidenceInit::Established(_)));
    assert_eq!(readiness, EvidenceReadiness::Ready);
    let startup = spawn_gateway(
        &GatewayConfig::default(),
        data.path().to_path_buf(),
        handles_with(&fx.handle, shared.clone(), readiness),
    )
    .await;
    assert_eq!(startup, GatewayStartup::Disabled);
    assert_eq!(fx.handle.evidence_readiness(), EvidenceReadiness::Ready);
    let s = wait_closed(&fx.handle, &fx.overdue).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    assert!(shared
        .unwrap()
        .receipt_store()
        .get_mandate_by_proposal(&fx.overdue.0)
        .unwrap()
        .is_some());
    fx.handle.shutdown().await;
}

/// Was: missing gateway credentials held governance. Now: the gateway failure
/// is reported as before but does not reach governance, which stays Ready.
#[tokio::test]
async fn missing_gateway_credentials_are_a_gateway_failure_only() {
    let fx = fixture().await;
    let data = tempfile::tempdir().unwrap();
    let (_init, settled, shared) = establish(&fx, data.path()).await;
    let config = GatewayConfig {
        jwt_secret: String::new(),
        ..enabled_config()
    };
    let startup = spawn_gateway(
        &config,
        data.path().to_path_buf(),
        handles_with(&fx.handle, shared, settled),
    )
    .await;
    assert_eq!(
        startup,
        GatewayStartup::Failed(GatewayStartupFailure::MissingCredentials)
    );
    assert_eq!(fx.handle.evidence_readiness(), EvidenceReadiness::Ready);
    let s = wait_closed(&fx.handle, &fx.overdue).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    fx.handle.shutdown().await;
}

/// Was: an invalid gateway bind address held governance. Now: a gateway
/// configuration failure only; governance stays Ready.
#[tokio::test]
async fn an_invalid_bind_address_is_a_gateway_configuration_failure_only() {
    let fx = fixture().await;
    let data = tempfile::tempdir().unwrap();
    let (_init, settled, shared) = establish(&fx, data.path()).await;
    let config = GatewayConfig {
        bind_addr: "not-an-address".to_string(),
        ..enabled_config()
    };
    let startup = spawn_gateway(
        &config,
        data.path().to_path_buf(),
        handles_with(&fx.handle, shared, settled),
    )
    .await;
    assert!(
        matches!(
            startup,
            GatewayStartup::Failed(GatewayStartupFailure::InvalidConfiguration(_))
        ),
        "{startup:?}"
    );
    assert_eq!(fx.handle.evidence_readiness(), EvidenceReadiness::Ready);
    let s = wait_closed(&fx.handle, &fx.overdue).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    fx.handle.shutdown().await;
}

/// Was: the gateway failing to open its store held governance. Now: the
/// daemon cannot open the governance store (the data directory is a regular
/// file), so evidence init fails with StoreOpen and governance is held and
/// fails closed; the gateway, given the actor but no store, refuses to start.
#[tokio::test]
async fn an_unopenable_governance_store_holds_governance() {
    let fx = fixture().await;
    let data = tempfile::tempdir().unwrap();
    let not_a_dir = data.path().join("file");
    std::fs::write(&not_a_dir, b"x").unwrap();
    let (init, readiness, shared) = establish(&fx, &not_a_dir).await;
    assert!(
        matches!(
            init,
            GovernanceEvidenceInit::Failed(EvidenceInitFailure::StoreOpen(_))
        ),
        "{init:?}"
    );
    assert!(shared.is_none());
    assert_eq!(readiness, EvidenceReadiness::Failed);
    let startup = spawn_gateway(
        &enabled_config(),
        not_a_dir,
        handles_with(&fx.handle, None, readiness),
    )
    .await;
    assert!(
        matches!(
            startup,
            GatewayStartup::Failed(GatewayStartupFailure::ExitedBeforeReadiness(_))
        ),
        "{startup:?}"
    );
    assert_eq!(fx.handle.evidence_readiness(), EvidenceReadiness::Failed);
    assert_held_and_fail_closed(&fx).await;
    fx.handle.shutdown().await;
}

/// Unchanged expectation, new owner: evidence init then a healthy gateway;
/// governance Ready and the overdue close completes.
#[tokio::test]
async fn a_healthy_gateway_after_evidence_init_and_overdue_closes_complete() {
    let fx = fixture().await;
    let data = tempfile::tempdir().unwrap();
    let (_init, settled, shared) = establish(&fx, data.path()).await;
    let startup = spawn_gateway(
        &enabled_config(),
        data.path().to_path_buf(),
        handles_with(&fx.handle, shared, settled),
    )
    .await;
    assert_eq!(startup, GatewayStartup::Ready);
    assert_eq!(fx.handle.evidence_readiness(), EvidenceReadiness::Ready);
    let s = wait_closed(&fx.handle, &fx.overdue).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    fx.handle.shutdown().await;
}

/// Was: a failure between the gateway's receipt-store install and its
/// recovery held governance. That intermediate state no longer exists in
/// production (installs and recovery are one supervisor step). The remaining
/// "actor present, evidence never established" case — no store injected —
/// holds governance.
#[tokio::test]
async fn no_injected_store_holds_governance() {
    let fx = fixture().await;
    let (init, readiness) = establish_governance_evidence(GovernanceEvidenceInputs {
        governance: fx.handle.clone(),
        store: None,
        dispatch_evidence_sink: None,
        execution_store: None,
        post_backfill_cleanup: None,
    })
    .await;
    assert!(
        matches!(
            init,
            GovernanceEvidenceInit::Failed(EvidenceInitFailure::NotInjected)
        ),
        "{init:?}"
    );
    assert_eq!(readiness, EvidenceReadiness::Failed);
    assert_held_and_fail_closed(&fx).await;
    fx.handle.shutdown().await;
}

/// Was: a gateway failure after its own evidence install and recovery kept
/// governance Ready. Now: any gateway failure after evidence init keeps
/// governance Ready, and the close writes its mandate to the shared store.
#[tokio::test]
async fn a_gateway_failure_after_evidence_init_keeps_governance_closing_with_evidence() {
    let fx = fixture().await;
    let data = tempfile::tempdir().unwrap();
    let (_init, settled, shared) = establish(&fx, data.path()).await;
    let shared = shared.unwrap();
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let config = GatewayConfig {
        bind_addr: occupied.local_addr().unwrap().to_string(),
        ..enabled_config()
    };
    let startup = spawn_gateway(
        &config,
        data.path().to_path_buf(),
        handles_with(&fx.handle, Some(shared.clone()), settled),
    )
    .await;
    assert!(matches!(startup, GatewayStartup::Failed(_)), "{startup:?}");
    assert_eq!(fx.handle.evidence_readiness(), EvidenceReadiness::Ready);
    let s = wait_closed(&fx.handle, &fx.overdue).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    assert!(
        shared
            .receipt_store()
            .get_mandate_by_proposal(&fx.overdue.0)
            .unwrap()
            .is_some(),
        "the accepted close writes its mandate to the shared receipt store"
    );
    fx.handle.shutdown().await;
}

/// Was: without an actor, gateway startup settled nothing. Settlement now
/// takes an actor by type, so only the gateway half remains: with no actor
/// and no injected store (the identity-less path, unchanged by S1) the
/// gateway needs no shared store.
#[tokio::test]
async fn without_a_governance_actor_the_gateway_needs_no_injected_store() {
    let data = tempfile::tempdir().unwrap();
    let startup = spawn_gateway(
        &GatewayConfig::default(),
        data.path().to_path_buf(),
        GatewayHandles::default(),
    )
    .await;
    assert_eq!(startup, GatewayStartup::Disabled);
    let startup = spawn_gateway(
        &enabled_config(),
        data.path().to_path_buf(),
        GatewayHandles::default(),
    )
    .await;
    assert_eq!(startup, GatewayStartup::Ready);
}
