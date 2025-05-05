use icn_federation::{debug_api::DebugApi, debug_api::implementation::BasicDebugApi, create_sha256_multihash};
use icn_federation::{FederationManager, FederationManagerConfig};
use icn_storage::{AsyncInMemoryStorage, StorageBackend};
use futures::lock::Mutex;
use std::sync::Arc;
use std::time::Duration;
use libp2p::gossipsub;
use cid::Cid;
use icn_governance_kernel::{Proposal, ProposalStatus};
use icn_identity::{IdentityId, IdentityScope};
use serde_json;
use icn_dag::{DagNode, DagNodeMetadata};
use icn_identity::Signature;

// Helper function to create a test storage backend
async fn create_test_storage() -> Arc<Mutex<dyn StorageBackend + Send + Sync>> {
    Arc::new(Mutex::new(AsyncInMemoryStorage::new()))
}

// Helper function to create a test federation manager
async fn create_test_federation_manager(
    storage: Arc<Mutex<dyn StorageBackend + Send + Sync>>,
) -> icn_federation::FederationResult<FederationManager> {
    // Create a simple configuration for testing
    let config = FederationManagerConfig {
        bootstrap_period: Duration::from_millis(10),
        peer_sync_interval: Duration::from_millis(10),
        trust_bundle_sync_interval: Duration::from_millis(10),
        max_peers: 2,
        bootstrap_peers: vec![],
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        gossipsub_heartbeat_interval: Duration::from_millis(10),
        gossipsub_validation_mode: gossipsub::ValidationMode::Permissive,
    };

    let (manager, _blob_sender, _fed_cmd_sender) = FederationManager::start_node(config, storage).await?;
    Ok(manager)
}

#[tokio::test]
async fn test_query_proposal_status_found() {
    let storage = create_test_storage().await;
    let manager = create_test_federation_manager(storage.clone()).await.unwrap();
    let api = BasicDebugApi::new(storage.clone(), Arc::new(manager));

    // Create a dummy proposal
    let proposal = Proposal {
        title: "Test Proposal".to_string(),
        description: "Testing".to_string(),
        proposer: IdentityId::new("did:icn:test-proposer"),
        scope: IdentityScope::Federation,
        scope_id: None,
        status: ProposalStatus::Executed,
        voting_end_time: 1234567890,
        votes_for: 10,
        votes_against: 2,
        votes_abstain: 1,
        ccl_code: None,
        wasm_bytes: None,
    };

    // Serialize proposal
    let proposal_bytes = serde_json::to_vec(&proposal).unwrap();

    // Create a dummy proposal CID
    let mh = create_sha256_multihash(b"test_proposal");
    let proposal_cid = Cid::new_v1(0x55, mh);

    // Compute storage key (same logic as BasicDebugApi)
    let key_str = format!("proposal::{}", proposal_cid);
    let key_hash = create_sha256_multihash(key_str.as_bytes());
    let key_cid = Cid::new_v1(0x71, key_hash);

    // Store in KV
    {
        let mut guard = storage.lock().await;
        guard.put_kv(key_cid, proposal_bytes).await.unwrap();
    }

    // Query via API
    let resp = api.query_proposal_status(&proposal_cid).await.unwrap();
    assert!(resp.exists);
    assert_eq!(resp.status, format!("{:?}", ProposalStatus::Executed));
    assert_eq!(resp.vote_count, 13);
    assert!(resp.executed);
}

#[tokio::test]
async fn test_query_proposal_status_not_found() {
    let storage = create_test_storage().await;
    let manager = create_test_federation_manager(storage.clone()).await.unwrap();
    let api = BasicDebugApi::new(storage.clone(), Arc::new(manager));

    let mh = create_sha256_multihash(b"unknown");
    let cid = Cid::new_v1(0x55, mh);

    let resp = api.query_proposal_status(&cid).await.unwrap();
    assert!(!resp.exists);
    assert_eq!(resp.status, "NotFound");
}

#[tokio::test]
async fn test_query_dag_node_found() {
    let storage = create_test_storage().await;
    let manager = create_test_federation_manager(storage.clone()).await.unwrap();
    let api = BasicDebugApi::new(storage.clone(), Arc::new(manager));

    // Create a dummy DAG node
    let signer = IdentityId::new("did:icn:signer");
    let signature = Signature::new(vec![1, 2, 3]);
    let node = DagNode::new(b"content".to_vec(), vec![], signer, signature, Some(DagNodeMetadata::default())).unwrap();
    let node_bytes = serde_json::to_vec(&node).unwrap();

    // Store blob
    let stored_cid;
    {
        let mut guard = storage.lock().await;
        stored_cid = guard.put_blob(&node_bytes).await.unwrap();
    }

    let resp_opt = api.query_dag_node(&stored_cid).await.unwrap();
    let resp = resp_opt.expect("Expected DAG node response");
    assert_eq!(resp.cid, stored_cid.to_string());
    assert_eq!(resp.size, node_bytes.len());
    assert_eq!(resp.links.len(), 0);
}

#[tokio::test]
async fn test_query_dag_node_not_found() {
    let storage = create_test_storage().await;
    let manager = create_test_federation_manager(storage.clone()).await.unwrap();
    let api = BasicDebugApi::new(storage.clone(), Arc::new(manager));

    let mh = create_sha256_multihash(b"missing");
    let cid = Cid::new_v1(0x55, mh);

    let resp = api.query_dag_node(&cid).await.unwrap();
    assert!(resp.is_none());
}

#[tokio::test]
async fn test_query_federation_status_no_bundle() {
    let storage = create_test_storage().await;
    let manager = create_test_federation_manager(storage.clone()).await.unwrap();
    let api = BasicDebugApi::new(storage.clone(), Arc::new(manager));

    let status = api.query_federation_status().await.unwrap();
    assert_eq!(status.current_epoch, 0);
    assert_eq!(status.node_count, 0);
    assert_eq!(status.connected_peers, 0);
} 