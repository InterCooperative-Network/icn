#![allow(clippy::unwrap_used, clippy::expect_used)]
//! #2082 PR2: verify cooperative activation is wired to the canonical
//! `CoopEntityMap`, that the map is constructed from the existing shared sled DB
//! (no new database), and that the actor uses the handle during activation.
//!
//! A binding written here is a non-authoritative name binding only.

use std::sync::Arc;

use icn_coop::CoopType;
use icn_core::config::Config;
use icn_core::supervisor::init_coop::init_coop_services_with_treasury;
use icn_entity::EntityId;
use icn_gossip::GossipActor;
use icn_identity::KeyPair;
use icn_ledger::TreasuryManager;
use tokio::sync::RwLock;

/// Activating a cooperative with a mappable id binds it through the wired map,
/// proving (a) a `SledCoopEntityMap` is constructed in the treasury-enabled
/// path, and (b) the actor received and used the same handle.
#[tokio::test]
async fn activation_binds_through_wired_coop_entity_map() {
    let temp = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: temp.path().to_path_buf(),
        ..Config::default()
    };

    let node_did = KeyPair::generate().unwrap().did().clone();
    let gossip = GossipActor::spawn(node_did.clone(), None);
    let treasury_mgr = Arc::new(RwLock::new(TreasuryManager::new()));

    let services = init_coop_services_with_treasury(&config, gossip, node_did, Some(treasury_mgr))
        .await
        .expect("init coop services should succeed");

    let founder = KeyPair::generate().unwrap().did().clone();
    let coop = services
        .coop_handle
        .create_cooperative(
            Some("wiring-coop".to_string()),
            "Wiring Coop".to_string(),
            CoopType::Worker,
            founder,
        )
        .await
        .unwrap();
    services
        .coop_handle
        .activate_cooperative(coop.id.clone(), "charter".to_string())
        .await
        .unwrap();

    // The actor received the same map handle and bound the mappable id.
    let entity = EntityId::cooperative("wiring-coop").unwrap();
    assert_eq!(
        services
            .coop_entity_map
            .entity_for_coop("wiring-coop")
            .unwrap(),
        Some(entity.clone()),
        "activation must bind through the wired CoopEntityMap"
    );
    assert_eq!(
        services.coop_entity_map.coop_for_entity(&entity).unwrap(),
        Some("wiring-coop".to_string())
    );
}

/// The map shares the cooperative sled store: no separate database directory is
/// created for it.
#[tokio::test]
async fn wiring_creates_no_separate_map_database() {
    let temp = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: temp.path().to_path_buf(),
        ..Config::default()
    };

    let node_did = KeyPair::generate().unwrap().did().clone();
    let gossip = GossipActor::spawn(node_did.clone(), None);
    let treasury_mgr = Arc::new(RwLock::new(TreasuryManager::new()));

    let _services = init_coop_services_with_treasury(&config, gossip, node_did, Some(treasury_mgr))
        .await
        .expect("init coop services should succeed");

    // The cooperative sled store is opened once; the map reuses that same DB.
    assert!(
        config.store_path().join("cooperative").exists(),
        "cooperative sled store must exist"
    );
    assert!(
        !config.store_path().join("coop_entity_map").exists(),
        "no separate coop_entity_map database should be created"
    );
}
