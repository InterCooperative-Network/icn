use icn_core_vm::{IdentityContext, VMContext, ResourceAuthorization};
use icn_governance_kernel::{GovernanceKernel, Proposal, Vote, VoteChoice, ProposalStatus};
use icn_federation::{FederationManager, FederationManagerConfig, TrustBundle};
use icn_identity::{IdentityId, IdentityScope, KeyPair};
use icn_storage::AsyncInMemoryStorage;
use icn_execution_tools::derive_authorizations;
use icn_agoranet_integration::AgoraNetIntegration;
use icn_dag::DagManager;

use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::{Duration, Instant};
use cid::Cid;
use futures::future::join_all;
use std::collections::HashMap;
use tokio::time::sleep;
use rand::{Rng, thread_rng};

// Helper function to create test identities
fn create_test_identities(count: usize) -> Vec<(KeyPair, IdentityId)> {
    let mut identities = Vec::with_capacity(count);
    
    for i in 0..count {
        // Generate test keypair
        let mut rng = thread_rng();
        let private_key = (0..32).map(|_| rng.gen::<u8>()).collect::<Vec<_>>();
        let public_key = (0..32).map(|_| rng.gen::<u8>()).collect::<Vec<_>>();
        let keypair = KeyPair::new(private_key, public_key);
        
        let identity_id = IdentityId::new(&format!("did:icn:user{}", i));
        
        identities.push((keypair, identity_id));
    }
    
    identities
}

/// Stress test for governance proposal creation and voting
#[tokio::test]
async fn test_governance_stress() {
    const NUM_IDENTITIES: usize = 100;
    const NUM_PROPOSALS: usize = 50;
    const VOTES_PER_PROPOSAL: usize = 80;
    
    println!("=== GOVERNANCE STRESS TEST ===");
    println!("Creating {} identities, {} proposals with {} votes each", 
             NUM_IDENTITIES, NUM_PROPOSALS, VOTES_PER_PROPOSAL);
    
    // 1. Set up common storage backend
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // 2. Create test identities
    let identities = create_test_identities(NUM_IDENTITIES);
    let federation_id = IdentityId::new("did:icn:federation:stress-test");
    
    // 3. Create identity context for first user (proposal creator)
    let identity_context = Arc::new(IdentityContext::new(
        identities[0].0.clone(),
        identities[0].1.to_string()
    ));
    
    // 4. Initialize governance kernel
    let governance_kernel = GovernanceKernel::new(
        storage.clone(),
        identity_context.clone()
    );
    
    // 5. Create AgoraNet integration
    let agoranet = AgoraNetIntegration::new(storage.clone());
    
    // 6. Performance metrics
    let mut proposal_times = Vec::with_capacity(NUM_PROPOSALS);
    let mut vote_times = Vec::with_capacity(NUM_PROPOSALS * VOTES_PER_PROPOSAL);
    let mut finalize_times = Vec::with_capacity(NUM_PROPOSALS);
    let mut execute_times = Vec::with_capacity(NUM_PROPOSALS);
    
    // 7. Create proposals
    let mut proposal_cids = Vec::with_capacity(NUM_PROPOSALS);
    
    println!("Creating {} proposals...", NUM_PROPOSALS);
    let start_all = Instant::now();
    
    for i in 0..NUM_PROPOSALS {
        let proposal = Proposal::new(
            format!("Stress Test Proposal {}", i),
            format!("This is a stress test proposal #{}", i),
            identities[i % NUM_IDENTITIES].1.clone(),
            IdentityScope::Federation,
            Some(federation_id.clone()),
            3600, // 1-hour voting period
            Some(format!("// Sample CCL code for proposal {}\nrule stress_test_{} {{\n  always allow\n}}", i, i)),
        );
        
        let start = Instant::now();
        let proposal_cid = governance_kernel.process_proposal(proposal.clone()).await.unwrap();
        let duration = start.elapsed();
        
        proposal_times.push(duration);
        proposal_cids.push(proposal_cid);
        
        // Register event with AgoraNet
        let events = governance_kernel.get_proposal_events(proposal_cid).await;
        for event in events {
            agoranet.register_governance_event(&event).await.unwrap();
        }
        
        // Report progress
        if (i + 1) % 10 == 0 || i == NUM_PROPOSALS - 1 {
            println!("Created {}/{} proposals", i + 1, NUM_PROPOSALS);
        }
    }
    
    println!("Casting {} votes across all proposals...", NUM_PROPOSALS * VOTES_PER_PROPOSAL);
    
    // 8. Cast votes in parallel
    let mut vote_futures = Vec::with_capacity(NUM_PROPOSALS * VOTES_PER_PROPOSAL);
    
    for (i, proposal_cid) in proposal_cids.iter().enumerate() {
        for j in 0..VOTES_PER_PROPOSAL {
            let identity_idx = (i + j) % NUM_IDENTITIES;
            let vote_choice = if j % 3 == 0 { VoteChoice::Against } else { VoteChoice::For };
            
            let vote = Vote::new(
                identities[identity_idx].1.clone(),
                *proposal_cid,
                vote_choice,
                IdentityScope::Federation,
                Some(federation_id.clone()),
                Some(format!("Vote from user {} on proposal {}", identity_idx, i)),
            );
            
            let governance_kernel_clone = governance_kernel.clone();
            let agoranet_clone = agoranet.clone();
            
            vote_futures.push(tokio::spawn(async move {
                let start = Instant::now();
                governance_kernel_clone.record_vote(vote).await.unwrap();
                let duration = start.elapsed();
                
                // Register vote event
                let events = governance_kernel_clone.get_proposal_events(*proposal_cid).await;
                let latest_event = events.last().unwrap();
                agoranet_clone.register_governance_event(latest_event).await.unwrap();
                
                duration
            }));
        }
        
        // Report progress
        if (i + 1) % 10 == 0 || i == NUM_PROPOSALS - 1 {
            println!("Submitted votes for {}/{} proposals", i + 1, NUM_PROPOSALS);
        }
    }
    
    // Wait for all votes to complete
    let vote_results = join_all(vote_futures).await;
    for result in vote_results {
        vote_times.push(result.unwrap());
    }
    
    println!("Finalizing and executing all proposals...");
    
    // 9. Finalize and execute proposals
    for proposal_cid in &proposal_cids {
        // Finalize
        let start = Instant::now();
        governance_kernel.finalize_proposal(*proposal_cid).await.unwrap();
        finalize_times.push(start.elapsed());
        
        // Execute with appropriate authorizations
        let proposal = governance_kernel.get_proposal(*proposal_cid).await.unwrap();
        let template = proposal.get_template();
        let authorizations = derive_authorizations(&template);
        
        let vm_context = VMContext::new(
            identity_context.clone(),
            authorizations
        );
        
        let start = Instant::now();
        governance_kernel.execute_proposal_with_context(*proposal_cid, vm_context).await.unwrap();
        execute_times.push(start.elapsed());
        
        // Register events with AgoraNet
        let events = governance_kernel.get_proposal_events(*proposal_cid).await;
        let finalize_event = &events[events.len() - 2];
        let execute_event = &events[events.len() - 1];
        
        agoranet.register_governance_event(finalize_event).await.unwrap();
        agoranet.register_governance_event(execute_event).await.unwrap();
    }
    
    let total_duration = start_all.elapsed();
    
    // 10. Report performance metrics
    print_performance_metrics(
        "Proposal Creation", &proposal_times, NUM_PROPOSALS);
    print_performance_metrics(
        "Vote Recording", &vote_times, NUM_PROPOSALS * VOTES_PER_PROPOSAL);
    print_performance_metrics(
        "Proposal Finalization", &finalize_times, NUM_PROPOSALS);
    print_performance_metrics(
        "Proposal Execution", &execute_times, NUM_PROPOSALS);
    
    println!("Total test duration: {:?}", total_duration);
    println!("Operations per second: {:.2}", 
             (NUM_PROPOSALS + NUM_PROPOSALS * VOTES_PER_PROPOSAL + NUM_PROPOSALS * 2) as f64 
             / total_duration.as_secs_f64());
}

/// Stress test for federation TrustBundle synchronization
#[tokio::test]
async fn test_federation_stress() {
    const NUM_NODES: usize = 20;
    const NUM_EPOCHS: usize = 50;
    
    println!("=== FEDERATION STRESS TEST ===");
    println!("Creating {} federation nodes, simulating {} trust bundle epochs", 
             NUM_NODES, NUM_EPOCHS);
    
    // 1. Create storage backends for each node
    let mut storages = Vec::with_capacity(NUM_NODES);
    for _ in 0..NUM_NODES {
        storages.push(Arc::new(Mutex::new(AsyncInMemoryStorage::new())));
    }
    
    // 2. Create keypairs for each node
    let identities = create_test_identities(NUM_NODES);
    
    // 3. Create federation managers
    let mut federation_managers = Vec::with_capacity(NUM_NODES);
    let mut blob_senders = Vec::with_capacity(NUM_NODES);
    let mut fed_cmd_senders = Vec::with_capacity(NUM_NODES);
    
    println!("Initializing {} federation nodes...", NUM_NODES);
    
    for i in 0..NUM_NODES {
        let config = FederationManagerConfig {
            bootstrap_period: Duration::from_millis(50),
            peer_sync_interval: Duration::from_millis(100),
            trust_bundle_sync_interval: Duration::from_millis(200),
            max_peers: NUM_NODES,
            ..Default::default()
        };
        
        let (manager, blob_sender, fed_cmd_sender) = FederationManager::start_node(
            config,
            storages[i].clone()
        ).await.unwrap();
        
        federation_managers.push(manager);
        blob_senders.push(blob_sender);
        fed_cmd_senders.push(fed_cmd_sender);
        
        // Report progress
        if (i + 1) % 5 == 0 || i == NUM_NODES - 1 {
            println!("Initialized {}/{} federation nodes", i + 1, NUM_NODES);
        }
    }
    
    // Allow nodes to discover each other
    println!("Waiting for nodes to discover each other...");
    sleep(Duration::from_secs(2)).await;
    
    // 4. Create and publish trust bundles
    println!("Publishing {} trust bundle epochs...", NUM_EPOCHS);
    
    let mut push_times = Vec::with_capacity(NUM_EPOCHS);
    let mut sync_times = Vec::with_capacity(NUM_EPOCHS * (NUM_NODES - 1));
    
    for epoch in 1..=NUM_EPOCHS {
        // Create a new trust bundle
        let mut trust_bundle = TrustBundle::new(epoch as u64);
        
        // Add all nodes as validators
        for (_, identity) in &identities {
            trust_bundle.add_node(identity.clone(), icn_federation::roles::NodeRole::Validator);
        }
        
        // Sign the bundle (use dummy proof for testing)
        trust_bundle.set_proof(vec![1, 2, 3, 4]);
        
        // Publisher is the first node
        let start = Instant::now();
        federation_managers[0].publish_trust_bundle(trust_bundle.clone()).await.unwrap();
        push_times.push(start.elapsed());
        
        // Allow propagation time
        sleep(Duration::from_millis(100)).await;
        
        // Verify all other nodes can retrieve the bundle
        let sync_futures = federation_managers[1..].iter().map(|manager| {
            let manager = manager.clone();
            tokio::spawn(async move {
                let start = Instant::now();
                let result = manager.request_trust_bundle(epoch as u64).await;
                let duration = start.elapsed();
                
                assert!(result.is_ok(), "Failed to retrieve trust bundle");
                let bundle_opt = result.unwrap();
                assert!(bundle_opt.is_some(), "Bundle should exist");
                let bundle = bundle_opt.unwrap();
                assert_eq!(bundle.epoch_id, epoch as u64, "Bundle epoch should match");
                
                duration
            })
        });
        
        // Wait for all nodes to sync
        let sync_results = join_all(sync_futures).await;
        for result in sync_results {
            sync_times.push(result.unwrap());
        }
        
        // Report progress
        if epoch % 10 == 0 || epoch == NUM_EPOCHS {
            println!("Published and verified {}/{} trust bundle epochs", epoch, NUM_EPOCHS);
        }
    }
    
    // 5. Report performance metrics
    print_performance_metrics(
        "Trust Bundle Publication", &push_times, NUM_EPOCHS);
    print_performance_metrics(
        "Trust Bundle Synchronization", &sync_times, NUM_EPOCHS * (NUM_NODES - 1));
    
    // 6. Shutdown federation managers
    println!("Shutting down federation nodes...");
    for manager in federation_managers {
        manager.shutdown().await.unwrap();
    }
}

/// Stress test for DAG operations
#[tokio::test]
async fn test_dag_stress() {
    const NUM_NODES: usize = 1000;
    const BATCH_SIZE: usize = 100;
    
    println!("=== DAG STRESS TEST ===");
    println!("Creating a DAG with {} nodes, inserting in batches of {}", 
             NUM_NODES, BATCH_SIZE);
    
    // 1. Set up storage backend
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // 2. Initialize DAG manager
    let dag_manager = DagManager::new(storage.clone());
    
    // 3. Performance metrics
    let mut node_creation_times = Vec::with_capacity(NUM_NODES);
    let mut node_query_times = Vec::with_capacity(NUM_NODES);
    let mut path_query_times = Vec::with_capacity(NUM_NODES / 10);
    
    // 4. Create DAG nodes
    println!("Creating {} DAG nodes...", NUM_NODES);
    
    let mut node_cids = Vec::with_capacity(NUM_NODES);
    let mut rng = thread_rng();
    
    // Create root node
    let root_data = "Root DAG Node".as_bytes().to_vec();
    let root_cid = dag_manager.create_node(&root_data, vec![]).await.unwrap();
    node_cids.push(root_cid);
    
    let start_all = Instant::now();
    
    for batch in 0..(NUM_NODES / BATCH_SIZE) {
        let mut batch_futures = Vec::with_capacity(BATCH_SIZE);
        
        for i in 0..BATCH_SIZE {
            let node_index = batch * BATCH_SIZE + i + 1;
            let dag_manager = dag_manager.clone();
            let node_cids = node_cids.clone();
            
            batch_futures.push(tokio::spawn(async move {
                // Select 1-3 random parent nodes from existing nodes
                let num_parents = rng.gen_range(1..=3).min(node_index);
                let mut parents = Vec::with_capacity(num_parents);
                
                for _ in 0..num_parents {
                    let parent_idx = rng.gen_range(0..node_index);
                    parents.push(node_cids[parent_idx]);
                }
                
                // Create the node
                let node_data = format!("DAG Node {}", node_index).as_bytes().to_vec();
                let start = Instant::now();
                let node_cid = dag_manager.create_node(&node_data, parents).await.unwrap();
                let duration = start.elapsed();
                
                (node_cid, duration)
            }));
        }
        
        // Wait for batch to complete
        let results = join_all(batch_futures).await;
        for result in results {
            let (cid, duration) = result.unwrap();
            node_cids.push(cid);
            node_creation_times.push(duration);
        }
        
        // Report progress
        let completed = (batch + 1) * BATCH_SIZE;
        if completed % (BATCH_SIZE * 10) == 0 || completed >= NUM_NODES {
            println!("Created {}/{} DAG nodes", completed.min(NUM_NODES), NUM_NODES);
        }
    }
    
    println!("Performing {} random node queries...", NUM_NODES);
    
    // 5. Query nodes randomly
    for _ in 0..NUM_NODES {
        let node_idx = rng.gen_range(0..node_cids.len());
        let node_cid = node_cids[node_idx];
        
        let start = Instant::now();
        let _node = dag_manager.get_node(&node_cid).await.unwrap();
        node_query_times.push(start.elapsed());
    }
    
    println!("Performing {} random path queries...", NUM_NODES / 10);
    
    // 6. Query paths between random nodes
    for _ in 0..(NUM_NODES / 10) {
        let start_idx = rng.gen_range(0..node_cids.len() / 2);
        let end_idx = rng.gen_range(node_cids.len() / 2..node_cids.len());
        
        let start_cid = node_cids[start_idx];
        let end_cid = node_cids[end_idx];
        
        let start = Instant::now();
        let _paths = dag_manager.find_paths(&start_cid, &end_cid, 5).await.unwrap();
        path_query_times.push(start.elapsed());
    }
    
    let total_duration = start_all.elapsed();
    
    // 7. Report performance metrics
    print_performance_metrics(
        "DAG Node Creation", &node_creation_times, node_creation_times.len());
    print_performance_metrics(
        "DAG Node Queries", &node_query_times, node_query_times.len());
    print_performance_metrics(
        "DAG Path Queries", &path_query_times, path_query_times.len());
    
    println!("Total test duration: {:?}", total_duration);
    println!("Operations per second: {:.2}", 
             (node_creation_times.len() + node_query_times.len() + path_query_times.len()) as f64 
             / total_duration.as_secs_f64());
}

/// Concurrent DAG and Governance stress test
#[tokio::test]
async fn test_concurrent_stress() {
    const NUM_PROPOSALS: usize = 20;
    const NUM_DAG_NODES: usize = 200;
    const NUM_IDENTITIES: usize = 50;
    
    println!("=== CONCURRENT OPERATION STRESS TEST ===");
    println!("Running concurrent governance ({} proposals) and DAG ({} nodes) operations",
             NUM_PROPOSALS, NUM_DAG_NODES);
    
    // 1. Set up common storage backend
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // 2. Create test identities
    let identities = create_test_identities(NUM_IDENTITIES);
    let federation_id = IdentityId::new("did:icn:federation:concurrent-test");
    
    // 3. Create identity context
    let identity_context = Arc::new(IdentityContext::new(
        identities[0].0.clone(),
        identities[0].1.to_string()
    ));
    
    // 4. Initialize governance kernel
    let governance_kernel = GovernanceKernel::new(
        storage.clone(),
        identity_context.clone()
    );
    
    // 5. Initialize DAG manager
    let dag_manager = DagManager::new(storage.clone());
    
    let start_all = Instant::now();
    
    // 6. Run governance and DAG operations concurrently
    let governance_task = tokio::spawn(async move {
        let mut proposal_cids = Vec::with_capacity(NUM_PROPOSALS);
        
        // Create proposals
        for i in 0..NUM_PROPOSALS {
            let proposal = Proposal::new(
                format!("Concurrent Test Proposal {}", i),
                format!("This is a concurrent test proposal #{}", i),
                identities[i % NUM_IDENTITIES].1.clone(),
                IdentityScope::Federation,
                Some(federation_id.clone()),
                3600, // 1-hour voting period
                Some(format!("// Sample CCL code for concurrent test proposal {}\nrule concurrent_test_{} {{\n  always allow\n}}", i, i)),
            );
            
            let proposal_cid = governance_kernel.process_proposal(proposal.clone()).await.unwrap();
            proposal_cids.push(proposal_cid);
            
            // Cast votes from multiple identities
            let mut vote_futures = Vec::with_capacity(10);
            for j in 0..10 {
                let identity_idx = (i + j) % NUM_IDENTITIES;
                let vote_choice = if j % 3 == 0 { VoteChoice::Against } else { VoteChoice::For };
                
                let vote = Vote::new(
                    identities[identity_idx].1.clone(),
                    proposal_cid,
                    vote_choice,
                    IdentityScope::Federation,
                    Some(federation_id.clone()),
                    Some(format!("Concurrent vote from user {} on proposal {}", identity_idx, i)),
                );
                
                let governance_kernel_clone = governance_kernel.clone();
                vote_futures.push(tokio::spawn(async move {
                    governance_kernel_clone.record_vote(vote).await.unwrap();
                }));
            }
            
            // Wait for votes to complete
            join_all(vote_futures).await;
            
            // Finalize and execute proposal
            governance_kernel.finalize_proposal(proposal_cid).await.unwrap();
            
            let proposal = governance_kernel.get_proposal(proposal_cid).await.unwrap();
            let template = proposal.get_template();
            let authorizations = derive_authorizations(&template);
            
            let vm_context = VMContext::new(
                identity_context.clone(),
                authorizations
            );
            
            governance_kernel.execute_proposal_with_context(proposal_cid, vm_context).await.unwrap();
        }
        
        proposal_cids
    });
    
    let dag_task = tokio::spawn(async move {
        let mut node_cids = Vec::with_capacity(NUM_DAG_NODES);
        let mut rng = thread_rng();
        
        // Create root node
        let root_data = "Concurrent Root DAG Node".as_bytes().to_vec();
        let root_cid = dag_manager.create_node(&root_data, vec![]).await.unwrap();
        node_cids.push(root_cid);
        
        // Create DAG nodes with concurrent operations
        for i in 1..NUM_DAG_NODES {
            // Select 1-3 random parent nodes from existing nodes
            let num_parents = rng.gen_range(1..=3).min(i);
            let mut parents = Vec::with_capacity(num_parents);
            
            for _ in 0..num_parents {
                let parent_idx = rng.gen_range(0..i);
                parents.push(node_cids[parent_idx]);
            }
            
            // Create the node
            let node_data = format!("Concurrent DAG Node {}", i).as_bytes().to_vec();
            let node_cid = dag_manager.create_node(&node_data, parents).await.unwrap();
            node_cids.push(node_cid);
            
            // Periodically perform queries to increase contention
            if i % 10 == 0 {
                let query_futures = (0..5).map(|_| {
                    let dag_manager = dag_manager.clone();
                    let node_cids = node_cids.clone();
                    let query_idx = rng.gen_range(0..node_cids.len());
                    
                    tokio::spawn(async move {
                        let _node = dag_manager.get_node(&node_cids[query_idx]).await.unwrap();
                    })
                });
                
                // Run queries concurrently with node creation
                join_all(query_futures).await;
            }
        }
        
        node_cids
    });
    
    // Wait for both tasks to complete
    let (proposal_results, dag_results) = tokio::join!(governance_task, dag_task);
    
    let proposal_cids = proposal_results.unwrap();
    let node_cids = dag_results.unwrap();
    
    let total_duration = start_all.elapsed();
    
    println!("Concurrent test completed:");
    println!("  - Created {} governance proposals", proposal_cids.len());
    println!("  - Created {} DAG nodes", node_cids.len());
    println!("  - Total operations: {}", proposal_cids.len() * 12 + node_cids.len());
    println!("  - Total duration: {:?}", total_duration);
    println!("  - Operations per second: {:.2}", 
             (proposal_cids.len() * 12 + node_cids.len()) as f64 
             / total_duration.as_secs_f64());
}

/// Helper function to print performance metrics
fn print_performance_metrics(operation: &str, timings: &[Duration], count: usize) {
    if timings.is_empty() {
        println!("{}: No data", operation);
        return;
    }
    
    let total: Duration = timings.iter().sum();
    let avg = total / timings.len() as u32;
    
    let min = timings.iter().min().unwrap();
    let max = timings.iter().max().unwrap();
    
    // Calculate percentiles
    let mut sorted_timings = timings.to_vec();
    sorted_timings.sort();
    
    let p50_idx = (timings.len() as f64 * 0.5) as usize;
    let p95_idx = (timings.len() as f64 * 0.95) as usize;
    let p99_idx = (timings.len() as f64 * 0.99) as usize;
    
    let p50 = sorted_timings[p50_idx];
    let p95 = sorted_timings[p95_idx];
    let p99 = sorted_timings[p99_idx];
    
    println!("{} ({} operations):", operation, count);
    println!("  - Average: {:?}", avg);
    println!("  - Min: {:?}", min);
    println!("  - Max: {:?}", max);
    println!("  - p50: {:?}", p50);
    println!("  - p95: {:?}", p95);
    println!("  - p99: {:?}", p99);
    println!("  - Throughput: {:.2} ops/sec", 
             count as f64 / total.as_secs_f64());
}

/// Resource utilization test to monitor CPU, memory and other resources
#[tokio::test]
async fn test_resource_utilization() {
    use std::process::Command;
    use tokio::time::Instant;
    
    const DURATION_SECS: u64 = 30;
    const SAMPLE_INTERVAL_MS: u64 = 500;
    
    println!("=== RESOURCE UTILIZATION TEST ===");
    println!("Running high-load operations and monitoring resource usage for {} seconds", DURATION_SECS);
    
    // 1. Set up storage backend
    let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // 2. Create test identities
    let identities = create_test_identities(20);
    let federation_id = IdentityId::new("did:icn:federation:resource-test");
    
    // 3. Create identity context
    let identity_context = Arc::new(IdentityContext::new(
        identities[0].0.clone(),
        identities[0].1.to_string()
    ));
    
    // 4. Initialize components
    let governance_kernel = GovernanceKernel::new(
        storage.clone(),
        identity_context.clone()
    );
    
    let dag_manager = DagManager::new(storage.clone());
    
    // 5. Get current process ID for resource monitoring
    let pid = std::process::id();
    println!("Monitoring process ID: {}", pid);
    
    // 6. Start the resource monitoring task
    let monitoring_task = tokio::spawn(async move {
        let mut cpu_samples = Vec::new();
        let mut memory_samples = Vec::new();
        let start_time = Instant::now();
        
        while start_time.elapsed().as_secs() < DURATION_SECS {
            // Sample resource usage
            #[cfg(target_os = "linux")]
            {
                // CPU usage (Linux)
                let output = Command::new("ps")
                    .args(&["-p", &pid.to_string(), "-o", "%cpu"])
                    .output()
                    .expect("Failed to execute ps command");
                
                let cpu_output = String::from_utf8_lossy(&output.stdout);
                let cpu_lines: Vec<&str> = cpu_output.split('\n').collect();
                if cpu_lines.len() >= 2 {
                    if let Ok(cpu) = cpu_lines[1].trim().parse::<f64>() {
                        cpu_samples.push(cpu);
                    }
                }
                
                // Memory usage (Linux)
                let output = Command::new("ps")
                    .args(&["-p", &pid.to_string(), "-o", "rss"])
                    .output()
                    .expect("Failed to execute ps command");
                
                let mem_output = String::from_utf8_lossy(&output.stdout);
                let mem_lines: Vec<&str> = mem_output.split('\n').collect();
                if mem_lines.len() >= 2 {
                    if let Ok(mem) = mem_lines[1].trim().parse::<u64>() {
                        // Convert from KB to MB
                        memory_samples.push(mem as f64 / 1024.0);
                    }
                }
            }
            
            #[cfg(target_os = "windows")]
            {
                // Windows monitoring requires different approach, simplified for testing
                println!("Resource monitoring on Windows is simplified");
            }
            
            #[cfg(target_os = "macos")]
            {
                // CPU usage (macOS)
                let output = Command::new("ps")
                    .args(&["-p", &pid.to_string(), "-o", "%cpu"])
                    .output()
                    .expect("Failed to execute ps command");
                
                let cpu_output = String::from_utf8_lossy(&output.stdout);
                let cpu_lines: Vec<&str> = cpu_output.split('\n').collect();
                if cpu_lines.len() >= 2 {
                    if let Ok(cpu) = cpu_lines[1].trim().parse::<f64>() {
                        cpu_samples.push(cpu);
                    }
                }
                
                // Memory usage (macOS)
                let output = Command::new("ps")
                    .args(&["-p", &pid.to_string(), "-o", "rss"])
                    .output()
                    .expect("Failed to execute ps command");
                
                let mem_output = String::from_utf8_lossy(&output.stdout);
                let mem_lines: Vec<&str> = mem_output.split('\n').collect();
                if mem_lines.len() >= 2 {
                    if let Ok(mem) = mem_lines[1].trim().parse::<u64>() {
                        // Convert from KB to MB
                        memory_samples.push(mem as f64 / 1024.0);
                    }
                }
            }
            
            sleep(Duration::from_millis(SAMPLE_INTERVAL_MS)).await;
        }
        
        (cpu_samples, memory_samples)
    });
    
    // 7. Run intensive operations
    println!("Starting high-load operations...");
    
    // Create a stream of proposals and votes
    for i in 0..100 {
        // Create proposal
        let proposal = Proposal::new(
            format!("Resource Test Proposal {}", i),
            format!("This is a resource test proposal #{}", i),
            identities[i % 20].1.clone(),
            IdentityScope::Federation,
            Some(federation_id.clone()),
            3600, // 1-hour voting period
            Some(format!("// Resource test CCL code\nrule resource_test_{} {{\n  always allow\n}}", i)),
        );
        
        let proposal_cid = governance_kernel.process_proposal(proposal.clone()).await.unwrap();
        
        // Cast votes
        for j in 0..5 {
            let identity_idx = (i + j) % 20;
            let vote = Vote::new(
                identities[identity_idx].1.clone(),
                proposal_cid,
                VoteChoice::For,
                IdentityScope::Federation,
                Some(federation_id.clone()),
                Some(format!("Resource test vote from user {} on proposal {}", identity_idx, i)),
            );
            
            governance_kernel.record_vote(vote).await.unwrap();
        }
        
        // Create DAG nodes
        for j in 0..10 {
            let node_data = format!("Resource Test DAG Node {}_{}", i, j).as_bytes().to_vec();
            let _node_cid = dag_manager.create_node(&node_data, vec![]).await.unwrap();
        }
        
        // Report progress
        if (i + 1) % 20 == 0 {
            println!("Completed {}/100 iterations of high-load operations", i + 1);
        }
        
        // Throttle operations to ensure monitoring captures a representative sample
        sleep(Duration::from_millis(50)).await;
    }
    
    // 8. Wait for monitoring to complete
    println!("Waiting for resource monitoring to complete...");
    let (cpu_samples, memory_samples) = monitoring_task.await.unwrap();
    
    // 9. Report resource metrics
    if !cpu_samples.is_empty() {
        let avg_cpu = cpu_samples.iter().sum::<f64>() / cpu_samples.len() as f64;
        let max_cpu = cpu_samples.iter().fold(0.0, |max, &val| max.max(val));
        
        println!("CPU Utilization:");
        println!("  - Average: {:.2}%", avg_cpu);
        println!("  - Maximum: {:.2}%", max_cpu);
        println!("  - Samples: {}", cpu_samples.len());
    } else {
        println!("No CPU samples collected");
    }
    
    if !memory_samples.is_empty() {
        let avg_mem = memory_samples.iter().sum::<f64>() / memory_samples.len() as f64;
        let max_mem = memory_samples.iter().fold(0.0, |max, &val| max.max(val));
        
        println!("Memory Utilization:");
        println!("  - Average: {:.2} MB", avg_mem);
        println!("  - Maximum: {:.2} MB", max_mem);
        println!("  - Samples: {}", memory_samples.len());
    } else {
        println!("No memory samples collected");
    }
    
    println!("Resource utilization test completed");
} 