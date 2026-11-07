//! The production entry point `init_governance_actor` (#2864 A2).
//!
//! The migrated integration tests drive `init_governance_actor_with_store`, the body
//! of this function. This test drives the path-opening wrapper itself, the function
//! the `icnd` supervisor actually calls, so that wrapper is not left uncovered:
//!
//! * it opens the real Sled store at `<store_path>/governance`,
//! * the actor it spawns works against that store, and
//! * no handle to that store is returned to the caller, so the only holder of the
//!   directory lock is the actor (a second open of the directory is refused).
//!
//! It does NOT claim that no other handle can exist anywhere in the process: code
//! that opens or is handed a different handle to a shared database is outside what
//! this function controls.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use icn_gossip::GossipActor;
use icn_governance::{GovernanceDomainId, GovernanceOps, GovernanceParams, MembershipConfig};
use icn_governance_actor::init::{init_governance_actor, GovernanceActorDeps};
use icn_governance_actor::manager::GovernanceManager;
use icn_identity::IdentityBundle;
use icn_kernel_api::events::{EventEmitter, SystemEvent};
use icn_store::SledStore;

struct NoopKernelEvents;

#[async_trait::async_trait]
impl EventEmitter for NoopKernelEvents {
    async fn emit(&self, _event: SystemEvent) {}
}

#[tokio::test]
async fn production_entry_point_opens_and_alone_holds_the_governance_store() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let did = IdentityBundle::generate().expect("identity").did().clone();
    let gossip = Arc::new(tokio::sync::RwLock::new(GossipActor::new(
        did.clone(),
        None,
    )));

    let services = init_governance_actor(
        tmp.path(),
        did.clone(),
        GovernanceActorDeps {
            gossip_handle: gossip,
            event_bus: Arc::new(NoopKernelEvents),
            signing_key: None,
            trust_service: None,
        },
    )
    .await
    .expect("init_governance_actor");

    // The actor works against the store the wrapper opened.
    let ops = GovernanceManager::with_handle(
        Arc::new(services.governance_handle) as Arc<dyn GovernanceOps + Send + Sync>
    );
    let id = GovernanceDomainId::new("owned");
    ops.create_domain(
        id.clone(),
        "Owned".to_string(),
        "cooperative_default".to_string(),
        GovernanceParams::new(50, 50, 3600),
        MembershipConfig::static_list(vec![did]),
    )
    .await
    .expect("create_domain");
    assert!(ops.get_domain(&id).await.expect("get_domain").is_some());

    // The wrapper opened `<store_path>/governance` ...
    let gov_dir = tmp.path().join("governance");
    assert!(
        gov_dir.is_dir(),
        "the wrapper must open <store_path>/governance"
    );
    // ... and nothing else holds it: sled's directory lock is taken, by the actor.
    assert!(
        SledStore::open(&gov_dir).is_err(),
        "the directory must be locked by the actor that owns the store"
    );
}
