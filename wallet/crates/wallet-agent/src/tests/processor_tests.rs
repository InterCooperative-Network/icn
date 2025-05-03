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

#[cfg(test)]
mod conflict_resolution_tests {
    use super::*;
    use crate::processor::{ActionProcessor, ConflictResolutionStrategy, ThreadConflict};
    use wallet_core::store::LocalWalletStore;
    use wallet_core::dag::{DagNode, DagThread};
    use wallet_core::identity::IdentityWallet;
    use wallet_core::crypto::KeyPair;
    use std::collections::HashMap;
    use std::time::{SystemTime, Duration};
    use serde_json::json;
    use uuid::Uuid;

    // Helper to create a test DAG node
    fn create_test_dag_node(
        cid: &str,
        parents: Vec<String>,
        timestamp: SystemTime,
        creator: &str,
        is_local: bool,
    ) -> DagNode {
        let content = if is_local {
            json!({
                "local_created": true,
                "data": "some data"
            })
        } else {
            json!({
                "data": "some data"
            })
        };

        DagNode {
            cid: cid.to_string(),
            parents,
            epoch: 0,
            creator: creator.to_string(),
            timestamp,
            content_type: "test".to_string(),
            content,
            signatures: vec![],
        }
    }

    // Setup test data for conflict resolution
    async fn setup_conflict_test_data(
        store: &impl LocalWalletStore,
    ) -> (String, Vec<String>, HashMap<String, DagNode>) {
        let thread_id = format!("thread:{}", Uuid::new_v4());
        let parent_cid = format!("bafy{}", Uuid::new_v4().to_string().replace("-", ""));
        
        // Create two nodes that share the same parent (fork)
        let local_cid = format!("bafy{}", Uuid::new_v4().to_string().replace("-", ""));
        let remote_cid = format!("bafy{}", Uuid::new_v4().to_string().replace("-", ""));
        
        let now = SystemTime::now();
        let earlier = now - Duration::from_secs(60);
        
        // The local node has a later timestamp
        let local_node = create_test_dag_node(
            &local_cid,
            vec![parent_cid.clone()],
            now, 
            "did:icn:local",
            true
        );
        
        // The remote node has an earlier timestamp
        let remote_node = create_test_dag_node(
            &remote_cid,
            vec![parent_cid.clone()],
            earlier,
            "did:icn:remote",
            false
        );
        
        // The parent node
        let parent_node = create_test_dag_node(
            &parent_cid,
            vec![],
            earlier - Duration::from_secs(60),
            "did:icn:local",
            true
        );
        
        // Store the nodes
        store.save_dag_node(&parent_cid, &parent_node).await.unwrap();
        store.save_dag_node(&local_cid, &local_node).await.unwrap();
        store.save_dag_node(&remote_cid, &remote_node).await.unwrap();
        
        // Create a DAG thread pointing to the local node (our current state)
        let thread = DagThread {
            id: thread_id.clone(),
            thread_type: "test".to_string(),
            creator: "did:icn:local".to_string(),
            latest_cid: local_cid.clone(),
            title: Some("Test Thread".to_string()),
            description: None,
            created_at: earlier,
            updated_at: now,
            status: "active".to_string(),
            tags: vec![],
        };
        
        store.save_dag_thread(&thread_id, &thread).await.unwrap();
        
        // Return the test data
        let conflicting_cids = vec![local_cid.clone(), remote_cid.clone()];
        let mut nodes_by_cid = HashMap::new();
        nodes_by_cid.insert(parent_cid.clone(), parent_node);
        nodes_by_cid.insert(local_cid.clone(), local_node);
        nodes_by_cid.insert(remote_cid.clone(), remote_node);
        
        (thread_id, conflicting_cids, nodes_by_cid)
    }

    #[tokio::test]
    async fn test_take_local_strategy() {
        // Create a mock store
        let store = create_mock_store();
        
        // Create an action processor
        let processor = ActionProcessor::new(store.clone());
        
        // Set up conflict test data
        let (thread_id, conflicting_cids, _) = setup_conflict_test_data(&store).await;
        
        // Apply TakeLocal strategy
        let result = processor.resolve_thread_conflicts_with_strategy(
            &thread_id, 
            ConflictResolutionStrategy::TakeLocal
        ).await.unwrap();
        
        // Verify result
        assert!(result.is_some(), "Should have resolved a conflict");
        let conflict = result.unwrap();
        assert_eq!(conflict.thread_id, thread_id, "Thread ID should match");
        assert_eq!(conflict.resolution_strategy, ConflictResolutionStrategy::TakeLocal, "Strategy should match");
        
        // Verify that the resolved CID is the local one (first in the conflicting CIDs)
        let resolved_cid = conflict.resolved_cid.unwrap();
        assert_eq!(resolved_cid, conflicting_cids[0], "Resolved CID should be the local one");
        
        // Verify the thread was updated
        let thread = store.load_dag_thread(&thread_id).await.unwrap();
        assert_eq!(thread.latest_cid, resolved_cid, "Thread should be updated to the resolved CID");
    }

    #[tokio::test]
    async fn test_take_remote_strategy() {
        // Create a mock store
        let store = create_mock_store();
        
        // Create an action processor
        let processor = ActionProcessor::new(store.clone());
        
        // Set up conflict test data
        let (thread_id, conflicting_cids, _) = setup_conflict_test_data(&store).await;
        
        // Apply TakeRemote strategy
        let result = processor.resolve_thread_conflicts_with_strategy(
            &thread_id, 
            ConflictResolutionStrategy::TakeRemote
        ).await.unwrap();
        
        // Verify result
        assert!(result.is_some(), "Should have resolved a conflict");
        let conflict = result.unwrap();
        assert_eq!(conflict.thread_id, thread_id, "Thread ID should match");
        assert_eq!(conflict.resolution_strategy, ConflictResolutionStrategy::TakeRemote, "Strategy should match");
        
        // Verify that the resolved CID is the remote one (second in the conflicting CIDs)
        let resolved_cid = conflict.resolved_cid.unwrap();
        assert_eq!(resolved_cid, conflicting_cids[1], "Resolved CID should be the remote one");
        
        // Verify the thread was updated
        let thread = store.load_dag_thread(&thread_id).await.unwrap();
        assert_eq!(thread.latest_cid, resolved_cid, "Thread should be updated to the resolved CID");
    }

    #[tokio::test]
    async fn test_earliest_timestamp_strategy() {
        // Create a mock store
        let store = create_mock_store();
        
        // Create an action processor
        let processor = ActionProcessor::new(store.clone());
        
        // Set up conflict test data
        let (thread_id, conflicting_cids, _) = setup_conflict_test_data(&store).await;
        
        // Apply EarliestTimestamp strategy
        let result = processor.resolve_thread_conflicts_with_strategy(
            &thread_id, 
            ConflictResolutionStrategy::EarliestTimestamp
        ).await.unwrap();
        
        // Verify result
        assert!(result.is_some(), "Should have resolved a conflict");
        let conflict = result.unwrap();
        assert_eq!(conflict.thread_id, thread_id, "Thread ID should match");
        assert_eq!(conflict.resolution_strategy, ConflictResolutionStrategy::EarliestTimestamp, "Strategy should match");
        
        // Verify that the resolved CID is the one with the earliest timestamp (remote one, which is second)
        let resolved_cid = conflict.resolved_cid.unwrap();
        assert_eq!(resolved_cid, conflicting_cids[1], "Resolved CID should be the one with earliest timestamp");
        
        // Verify the thread was updated
        let thread = store.load_dag_thread(&thread_id).await.unwrap();
        assert_eq!(thread.latest_cid, resolved_cid, "Thread should be updated to the resolved CID");
    }

    #[tokio::test]
    async fn test_ask_user_strategy() {
        // Create a mock store
        let store = create_mock_store();
        
        // Create an action processor
        let processor = ActionProcessor::new(store.clone());
        
        // Set up conflict test data
        let (thread_id, _, _) = setup_conflict_test_data(&store).await;
        
        // Apply AskUser strategy
        let result = processor.resolve_thread_conflicts_with_strategy(
            &thread_id, 
            ConflictResolutionStrategy::AskUser
        ).await;
        
        // Should return an error for AskUser strategy since it requires user intervention
        assert!(result.is_err(), "AskUser strategy should return an error without user input");
        
        // Verify the error type
        match result {
            Err(AgentError::UserInterventionRequired(_)) => {
                // This is expected
            },
            _ => {
                panic!("Expected UserInterventionRequired error");
            }
        }
    }
} 