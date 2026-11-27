//! Supervisor gateway startup and governance evidence readiness.
//!
//! The gateway is the component that installs the governance receipt store and
//! dispatch-evidence backend and runs close recovery. How its startup ends
//! decides what governance may do:
//!
//! * ready: recovery settled governance; scheduled closes write evidence;
//! * disabled by configuration: no evidence path is provisioned; scheduled
//!   closes proceed as before, with no receipt-backed guarantee;
//! * a configured gateway that fails before its evidence path is established:
//!   scheduled work is held (overdue proposals preserved) and terminal
//!   transitions are refused, while unrelated activity continues;
//! * a failure after evidence and recovery were established: governance stays
//!   ready.
//!
//! These drive the supervisor's real startup step (`start_gateway`) wherever a
//! configuration can produce the case, and its settlement seam
//! (`settle_governance_evidence`) for states between sink installation steps.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use icn_core::config::GatewayConfig;
use icn_core::supervisor::init_gateway::{
    settle_governance_evidence, start_gateway, GatewayHandles, GatewayStartup,
    GatewayStartupFailure,
};
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

fn handles_with(h: &GovernanceHandle) -> GatewayHandles {
    GatewayHandles {
        governance_actor_handle: Some(h.clone()),
        // As the daemon wires it: with a broadcaster the gateway is built over
        // its data directory (without one it would use temporary storage).
        event_broadcaster: Some(Arc::new(icn_gateway::EventBroadcaster::new())),
        ..GatewayHandles::default()
    }
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

#[tokio::test]
async fn a_disabled_gateway_declares_no_evidence_path_and_scheduled_closes_proceed() {
    let fx = fixture().await;
    let data = tempfile::tempdir().unwrap();
    let startup = start_gateway(
        &GatewayConfig::default(),
        data.path().to_path_buf(),
        handles_with(&fx.handle),
    )
    .await;
    assert_eq!(startup, GatewayStartup::Disabled);
    assert_eq!(
        fx.handle.evidence_readiness(),
        EvidenceReadiness::NotProvisioned
    );
    let s = wait_closed(&fx.handle, &fx.overdue).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    fx.handle.shutdown().await;
}

#[tokio::test]
async fn missing_credentials_are_a_failure_not_a_disabled_gateway() {
    let fx = fixture().await;
    let data = tempfile::tempdir().unwrap();
    let config = GatewayConfig {
        jwt_secret: String::new(),
        ..enabled_config()
    };
    let startup = start_gateway(&config, data.path().to_path_buf(), handles_with(&fx.handle)).await;
    assert_eq!(
        startup,
        GatewayStartup::Failed(GatewayStartupFailure::MissingCredentials)
    );
    assert_eq!(fx.handle.evidence_readiness(), EvidenceReadiness::Failed);
    assert_held_and_fail_closed(&fx).await;
    fx.handle.shutdown().await;
}

#[tokio::test]
async fn an_invalid_bind_address_is_a_configuration_failure() {
    let fx = fixture().await;
    let data = tempfile::tempdir().unwrap();
    let config = GatewayConfig {
        bind_addr: "not-an-address".to_string(),
        ..enabled_config()
    };
    let startup = start_gateway(&config, data.path().to_path_buf(), handles_with(&fx.handle)).await;
    assert!(
        matches!(
            startup,
            GatewayStartup::Failed(GatewayStartupFailure::InvalidConfiguration(_))
        ),
        "{startup:?}"
    );
    assert_eq!(fx.handle.evidence_readiness(), EvidenceReadiness::Failed);
    assert_held_and_fail_closed(&fx).await;
    fx.handle.shutdown().await;
}

#[tokio::test]
async fn a_storage_failure_before_the_receipt_store_holds_governance() {
    // The gateway thread starts, then cannot open its store: its data
    // directory is a regular file.
    let fx = fixture().await;
    let data = tempfile::tempdir().unwrap();
    let not_a_dir = data.path().join("file");
    std::fs::write(&not_a_dir, b"x").unwrap();
    let startup = start_gateway(&enabled_config(), not_a_dir, handles_with(&fx.handle)).await;
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

#[tokio::test]
async fn a_healthy_gateway_recovers_and_overdue_closes_then_complete() {
    let fx = fixture().await;
    let data = tempfile::tempdir().unwrap();
    let startup = start_gateway(
        &enabled_config(),
        data.path().to_path_buf(),
        handles_with(&fx.handle),
    )
    .await;
    assert_eq!(startup, GatewayStartup::Ready);
    assert_eq!(fx.handle.evidence_readiness(), EvidenceReadiness::Ready);
    let s = wait_closed(&fx.handle, &fx.overdue).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    fx.handle.shutdown().await;
}

fn real_receipt_store() -> (
    tempfile::TempDir,
    Arc<icn_gateway::receipt_store::ReceiptStore>,
) {
    let dir = tempfile::tempdir().unwrap();
    let db = sled::open(dir.path()).unwrap();
    (
        dir,
        Arc::new(icn_gateway::receipt_store::ReceiptStore::new(db)),
    )
}

#[tokio::test]
async fn a_failure_between_receipt_store_and_recovery_holds_governance() {
    // The receipt store is installed but recovery never ran (the dispatch
    // backend or recovery step did not complete).
    let fx = fixture().await;
    let (_dir, receipts) = real_receipt_store();
    fx.handle.install_receipt_store(receipts).await;
    let state = settle_governance_evidence(
        &GatewayStartup::Failed(GatewayStartupFailure::ExitedBeforeReadiness(None)),
        Some(&fx.handle),
    );
    assert_eq!(state, Some(EvidenceReadiness::Failed));
    assert_held_and_fail_closed(&fx).await;
    fx.handle.shutdown().await;
}

#[tokio::test]
async fn a_failure_after_evidence_and_recovery_keeps_governance_closing_with_evidence() {
    let fx = fixture().await;
    let (_dir, receipts) = real_receipt_store();
    fx.handle.install_receipt_store(receipts.clone()).await;
    fx.handle.recover_incomplete_closes().await;
    let state = settle_governance_evidence(
        &GatewayStartup::Failed(GatewayStartupFailure::ExitedBeforeActivation),
        Some(&fx.handle),
    );
    assert_eq!(state, Some(EvidenceReadiness::Ready));
    let s = wait_closed(&fx.handle, &fx.overdue).await;
    assert!(matches!(s, ProposalState::Accepted { .. }), "{s:?}");
    assert!(
        receipts
            .get_mandate_by_proposal(&fx.overdue.0)
            .unwrap()
            .is_some(),
        "the accepted close writes its mandate to the real receipt store"
    );
    fx.handle.shutdown().await;
}

#[tokio::test]
async fn without_a_governance_actor_startup_settles_nothing() {
    assert_eq!(
        settle_governance_evidence(
            &GatewayStartup::Failed(GatewayStartupFailure::MissingCredentials),
            None
        ),
        None
    );
    let data = tempfile::tempdir().unwrap();
    let startup = start_gateway(
        &GatewayConfig::default(),
        data.path().to_path_buf(),
        GatewayHandles::default(),
    )
    .await;
    assert_eq!(startup, GatewayStartup::Disabled);
}
