use std::time::{SystemTime, Duration};
use serde_json::json;
use uuid::Uuid;
use wallet_core::identity::IdentityWallet;
use wallet_core::dag::{DagNode, DagThread};
use wallet_core::store::MemoryStore;
use wallet_sync::SyncManager;
use crate::processor::{ActionProcessor, ConflictResolutionStrategy, ThreadConflict};
use crate::queue::{ActionQueue, PendingAction, ActionStatus, ActionType};

// Helper to create a test processor with memory store
async fn create_test_processor() -> (ActionProcessor<MemoryStore>, MemoryStore) {
    let store = MemoryStore::new();
    let processor = ActionProcessor::new(store.clone());
    
    (processor, store)
}

// Helper to create a test processor with sync manager
async fn create_test_processor_with_sync() -> (ActionProcessor<MemoryStore>, MemoryStore) {
    let store = MemoryStore::new();
    let identity = IdentityWallet::generate().unwrap();
    
    // Create sync manager with mock config
    let sync_manager = SyncManager::new(identity, store.clone(), None);
    
    let processor = ActionProcessor::with_sync_manager(store.clone(), sync_manager);
    
    (processor, store)
}

// Helper to create a test thread with a fork
async fn create_test_thread_with_fork(store: &MemoryStore) -> (String, Vec<String>) {
    // Create a test thread ID
    let thread_id = format!("thread:{}", Uuid::new_v4());
    
    // Create the root node
    let root_node = DagNode {
        cid: "root".to_string(),
        parents: vec![],
        epoch: 1,
        creator: "did:icn:test".to_string(),
        timestamp: SystemTime::now() - Duration::from_secs(100),
        content_type: "test".to_string(),
        content: json!({"action": "root"}),
        signatures: vec![],
    };
    
    // Create two child nodes with the same parent (creating a fork)
    let child1 = DagNode {
        cid: "child1".to_string(),
        parents: vec!["root".to_string()],
        epoch: 1,
        creator: "did:icn:test".to_string(),
        timestamp: SystemTime::now() - Duration::from_secs(50),
        content_type: "test".to_string(),
        content: json!({"action": "child1"}),
        signatures: vec![],
    };
    
    let child2 = DagNode {
        cid: "child2".to_string(),
        parents: vec!["root".to_string()],
        epoch: 1,
        creator: "did:icn:test".to_string(),
        timestamp: SystemTime::now() - Duration::from_secs(20),
        content_type: "test".to_string(),
        content: json!({"action": "child2"}),
        signatures: vec![],
    };
    
    // Create the thread
    let thread = DagThread {
        id: thread_id.clone(),
        thread_type: "test".to_string(),
        creator: "did:icn:test".to_string(),
        latest_cid: "child2".to_string(),  // Latest is child2
        title: Some("Test Thread".to_string()),
        description: None,
        created_at: SystemTime::now() - Duration::from_secs(100),
        updated_at: SystemTime::now(),
        status: "active".to_string(),
        tags: vec![],
    };
    
    // Store the nodes and thread
    store.save_dag_node("root", &root_node).await.unwrap();
    store.save_dag_node("child1", &child1).await.unwrap();
    store.save_dag_node("child2", &child2).await.unwrap();
    store.save_dag_thread(&thread_id, &thread).await.unwrap();
    
    (thread_id, vec!["child1".to_string(), "child2".to_string()])
}

// Helper to create test actions for batch processing
async fn create_test_actions(store: &MemoryStore) -> Vec<String> {
    // Create identity
    let identity = IdentityWallet::generate().unwrap();
    store.save_identity(&identity).await.unwrap();
    
    // Create action queue
    let queue = ActionQueue::new(store.clone());
    
    // Create test actions
    let action_ids = vec![];
    
    // Action 1: Create a proposal
    let action1 = PendingAction {
        id: Uuid::new_v4().to_string(),
        creator_did: identity.did.to_string(),
        action_type: ActionType::Proposal,
        created_at: chrono::Utc::now().to_string(),
        expires_at: None,
        status: ActionStatus::Pending,
        payload: json!({
            "title": "Test Proposal",
            "description": "This is a test proposal"
        }),
        error: None,
    };
    
    // Action 2: Create a vote on the proposal
    let action2 = PendingAction {
        id: Uuid::new_v4().to_string(),
        creator_did: identity.did.to_string(),
        action_type: ActionType::Vote,
        created_at: chrono::Utc::now().to_string(),
        expires_at: None,
        status: ActionStatus::Pending,
        payload: json!({
            "proposal_id": action1.id,
            "vote": "approve"
        }),
        error: None,
    };
    
    // Add actions to queue
    queue.add_action(&action1).await.unwrap();
    queue.add_action(&action2).await.unwrap();
    
    vec![action1.id, action2.id]
}

#[tokio::test]
async fn test_resolve_thread_conflicts() {
    // Create test processor and store
    let (processor, store) = create_test_processor().await;
    
    // Create a test thread with a fork
    let (thread_id, conflicting_cids) = create_test_thread_with_fork(&store).await;
    
    // Resolve conflicts
    let conflict = processor.resolve_thread_conflicts(&thread_id).await.unwrap();
    
    // Verify conflict was detected and resolved
    assert!(conflict.is_some());
    let conflict = conflict.unwrap();
    
    assert_eq!(conflict.thread_id, thread_id);
    assert_eq!(conflict.conflicting_cids.len(), 2);
    assert_eq!(conflict.resolution_strategy, ConflictResolutionStrategy::EarliestTimestamp);
    assert!(conflict.resolved_cid.is_some());
    
    // The resolved CID should be child1 as it has the earliest timestamp
    assert_eq!(conflict.resolved_cid.unwrap(), "child1");
    
    // Verify thread was updated
    let thread = store.load_dag_thread(&thread_id).await.unwrap();
    assert_eq!(thread.latest_cid, "child1");
}

#[tokio::test]
async fn test_process_action_group() {
    // Create test processor and store
    let (processor, store) = create_test_processor().await;
    
    // Create test actions
    let action_ids = create_test_actions(&store).await;
    
    // Process action group
    let nodes = processor.process_action_group(&action_ids).await.unwrap();
    
    // Verify nodes were created
    assert_eq!(nodes.len(), 2);
    
    // Verify both actions are completed
    let queue = ActionQueue::new(store.clone());
    for action_id in action_ids {
        let action = queue.get_action(&action_id).await.unwrap();
        assert_eq!(action.status, ActionStatus::Completed);
    }
    
    // Verify the DAG nodes have the correct parent-child relationship
    // The second node should have the first node as its parent
    assert_eq!(nodes[1].parents.len(), 1);
    assert_eq!(nodes[1].parents[0], nodes[0].cid);
} 