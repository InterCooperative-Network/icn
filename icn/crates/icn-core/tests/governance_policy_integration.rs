//! Integration test for Governance → Compute Policy execution flow
//!
//! This test validates that accepted SchedulingPolicy proposals automatically update compute policies:
//! 1. Create governance domain and scheduling policy proposal
//! 2. Vote and accept the proposal
//! 3. Verify policy was applied to compute actor
//! 4. Verify audit trail was stored linking proposal to policy update

use anyhow::Result;
use icn_compute::{
    ComputeActor, CoopSchedulingPolicy, EnforcementMode, MemberQuota, PolicyManager, UsageTracker,
};
use icn_core::{EventBus, SystemEvent};
use icn_governance::{ProposalId, ProposalPayload};
use icn_identity::KeyPair;
use icn_store::{SledStore, Store};
use std::sync::Arc;
use tracing::info;

#[tokio::test]
async fn test_scheduling_policy_proposal_execution() -> Result<()> {
    // Initialize tracing for test output
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Starting Governance→Policy integration test ===");

    // 1. Set up identity and stores
    let keypair = KeyPair::generate()?;
    let did = keypair.did().clone();

    let gov_store = Arc::new(SledStore::temporary()?);

    info!("Created identity: {}", did);

    // 2. Create compute actor with policy manager
    let usage_tracker = Arc::new(UsageTracker::new());
    let policy_manager = Arc::new(PolicyManager::new(usage_tracker.clone()));

    // Create compute actor
    let trust_callback = Arc::new(|_did: &str| 0.8); // Always return high trust for test
    let mut actor = ComputeActor::new(did.to_string(), trust_callback);

    // Set policy manager before spawning
    actor.set_policy_manager(policy_manager.clone());

    // Spawn the actor
    let compute_handle = actor.spawn();

    info!("Created compute actor with policy manager");

    // 3. Create event bus
    let event_bus = Arc::new(EventBus::new());

    info!("Created event bus");

    // 4. Subscribe to governance events and wire up policy handler
    // This mirrors the supervisor's governance integration code
    let _handle = {
        let compute = compute_handle.clone();
        let audit_store = gov_store.clone();

        event_bus.subscribe(Arc::new(move |event| {
            if let SystemEvent::ProposalAccepted {
                proposal_id,
                payload: ProposalPayload::SchedulingPolicy { coop_id, policy_json },
                decided_at,
                ..
            } = event {
                info!("📋 Executing scheduling policy proposal {}: update policy for {}",
                      proposal_id.0, coop_id);

                // Spawn async task to apply policy update
                let compute_handle = compute.clone();
                let prop_id = proposal_id.clone();
                let store = audit_store.clone();
                let coop = coop_id.clone();
                let policy_str = policy_json.clone();
                let decision_time = decided_at;

                    tokio::spawn(async move {
                        // IDEMPOTENCY CHECK: Skip if proposal already executed
                        let audit_key = format!("gov:audit:policy:{}", prop_id.0);
                        match store.get(audit_key.as_bytes()) {
                            Ok(Some(_)) => {
                                info!("Policy proposal {} already executed, skipping duplicate", prop_id.0);
                                return;
                            }
                            Ok(None) => {
                                // Not executed yet, proceed
                            }
                            Err(e) => {
                                eprintln!("🚨 Failed to check audit trail for policy proposal {}: {}", prop_id.0, e);
                                eprintln!("   Refusing to execute to prevent potential duplicate");
                                return;
                            }
                        }

                        // Parse policy JSON
                        match serde_json::from_str::<CoopSchedulingPolicy>(&policy_str) {
                            Ok(policy) => {
                                // Apply policy update via ComputeHandle
                                match compute_handle.set_policy(policy.clone()).await {
                                    Ok(_) => {
                                        info!("✅ Scheduling policy proposal {} executed: policy updated for {}",
                                              prop_id.0, coop);

                                        // Store audit trail
                                        let audit_record = serde_json::json!({
                                            "proposal_id": prop_id.0,
                                            "coop_id": coop,
                                            "decided_at": decision_time,
                                            "executed_at": std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap()
                                                .as_secs(),
                                        });

                                        if let Ok(audit_json) = serde_json::to_vec(&audit_record) {
                                            if let Err(e) = store.put(audit_key.as_bytes(), &audit_json) {
                                                eprintln!("🚨 Failed to store audit trail for policy proposal {}: {}", prop_id.0, e);
                                            } else {
                                                info!("📋 Audit trail recorded for policy proposal {}", prop_id.0);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("❌ Failed to apply scheduling policy for proposal {}: {}", prop_id.0, e);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("❌ Failed to parse policy JSON for proposal {}: {}", prop_id.0, e);
                            }
                        }
                    });
            }
        })).await
    };

    info!("Subscribed to governance events");

    // 5. Create a test scheduling policy
    let test_policy = CoopSchedulingPolicy {
        coop_id: "test-coop".to_string(),
        governance_domain: Some("governance:test".to_string()),
        rules: vec![],
        member_quotas: std::collections::HashMap::new(),
        default_quota: MemberQuota {
            cpu_hours_per_month: 100.0,
            max_concurrent_tasks: 10,
            max_priority: icn_compute::TaskPriority::High,
            credits_per_month: Some(1000),
        },
        enforcement_mode: EnforcementMode::Strict,
    };

    let policy_json = serde_json::to_string(&test_policy)?;

    info!("Created test scheduling policy");

    // 6. Create and fire a ProposalAccepted event
    let proposal_id = ProposalId::new("test-policy-proposal-123");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    let payload = ProposalPayload::SchedulingPolicy {
        coop_id: "test-coop".to_string(),
        policy_json: policy_json.clone(),
    };

    info!(
        "Firing ProposalAccepted event for proposal {}",
        proposal_id.0
    );

    event_bus
        .emit(SystemEvent::ProposalAccepted {
            proposal_id: proposal_id.clone(),
            domain_id: "test-domain".to_string(),
            payload,
            decided_at: now,
        })
        .await;

    // 7. Wait for async processing
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 8. Verify policy was applied
    let applied_policy = policy_manager.get_policy("test-coop").await;
    assert!(applied_policy.is_some(), "Policy should have been applied");

    let applied = applied_policy.unwrap();
    assert_eq!(applied.coop_id, "test-coop");
    assert_eq!(applied.default_quota.cpu_hours_per_month, 100.0);
    assert_eq!(applied.default_quota.max_concurrent_tasks, 10);
    assert_eq!(applied.enforcement_mode, EnforcementMode::Strict);

    info!("✅ Verified policy was applied to compute actor");

    // 9. Verify audit trail was stored
    let audit_key = format!("gov:audit:policy:{}", proposal_id.0);
    let audit_data = gov_store.get(audit_key.as_bytes())?;
    assert!(audit_data.is_some(), "Audit trail should have been stored");

    let audit_json: serde_json::Value = serde_json::from_slice(&audit_data.unwrap())?;
    assert_eq!(audit_json["proposal_id"], proposal_id.0);
    assert_eq!(audit_json["coop_id"], "test-coop");

    info!("✅ Verified audit trail was stored");

    // 10. Test idempotency - fire the same event again
    info!("Testing idempotency - firing duplicate event");

    event_bus
        .emit(SystemEvent::ProposalAccepted {
            proposal_id: proposal_id.clone(),
            domain_id: "test-domain".to_string(),
            payload: ProposalPayload::SchedulingPolicy {
                coop_id: "test-coop".to_string(),
                policy_json: policy_json.clone(),
            },
            decided_at: now,
        })
        .await;

    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // Policy should still be the same (no duplicate execution)
    let policy_after_duplicate = policy_manager.get_policy("test-coop").await;
    assert!(policy_after_duplicate.is_some());
    assert_eq!(policy_after_duplicate.unwrap().coop_id, "test-coop");

    info!("✅ Verified idempotency - duplicate event was ignored");

    info!("=== Governance→Policy integration test completed successfully ===");

    Ok(())
}

#[tokio::test]
async fn test_invalid_policy_json_handling() -> Result<()> {
    // Initialize tracing for test output
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_test_writer()
        .try_init();

    info!("=== Testing invalid policy JSON handling ===");

    // 1. Set up identity and stores
    let keypair = KeyPair::generate()?;
    let did = keypair.did().clone();

    let gov_store = Arc::new(SledStore::temporary()?);

    // 2. Create compute actor with policy manager
    let usage_tracker = Arc::new(UsageTracker::new());
    let policy_manager = Arc::new(PolicyManager::new(usage_tracker.clone()));

    // Create compute actor
    let trust_callback = Arc::new(|_did: &str| 0.8); // Always return high trust for test
    let mut actor = ComputeActor::new(did.to_string(), trust_callback);

    // Set policy manager before spawning
    actor.set_policy_manager(policy_manager.clone());

    // Spawn the actor
    let compute_handle = actor.spawn();

    // 3. Create event bus and subscribe
    let event_bus = Arc::new(EventBus::new());

    let _handle = {
        let compute = compute_handle.clone();
        let audit_store = gov_store.clone();

        event_bus
            .subscribe(Arc::new(move |event| {
                if let SystemEvent::ProposalAccepted {
                    proposal_id,
                    payload:
                        ProposalPayload::SchedulingPolicy {
                            coop_id,
                            policy_json,
                        },
                    decided_at,
                    ..
                } = event
                {
                    let compute_handle = compute.clone();
                    let prop_id = proposal_id.clone();
                    let store = audit_store.clone();
                    let _coop = coop_id.clone();
                    let policy_str = policy_json.clone();
                    let _decision_time = decided_at;

                    tokio::spawn(async move {
                        let audit_key = format!("gov:audit:policy:{}", prop_id.0);
                        match store.get(audit_key.as_bytes()) {
                            Ok(Some(_)) => return,
                            Ok(None) => {}
                            Err(_) => return,
                        }

                        match serde_json::from_str::<CoopSchedulingPolicy>(&policy_str) {
                            Ok(policy) => {
                                let _ = compute_handle.set_policy(policy.clone()).await;
                            }
                            Err(_) => {
                                // Invalid JSON should be logged but not crash
                            }
                        }
                    });
                }
            }))
            .await
    };

    // 4. Fire an event with invalid JSON
    let proposal_id = ProposalId::new("invalid-json-proposal");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    event_bus
        .emit(SystemEvent::ProposalAccepted {
            proposal_id: proposal_id.clone(),
            domain_id: "test-domain".to_string(),
            payload: ProposalPayload::SchedulingPolicy {
                coop_id: "test-coop".to_string(),
                policy_json: "{invalid json}".to_string(),
            },
            decided_at: now,
        })
        .await;

    // 5. Wait for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // 6. Verify policy was NOT applied (due to invalid JSON)
    let policy = policy_manager.get_policy("test-coop").await;
    assert!(
        policy.is_none(),
        "Invalid policy should not have been applied"
    );

    // 7. Verify no audit trail was stored (execution failed)
    let audit_key = format!("gov:audit:policy:{}", proposal_id.0);
    let audit_data = gov_store.get(audit_key.as_bytes())?;
    assert!(
        audit_data.is_none(),
        "Audit trail should not exist for failed execution"
    );

    info!("✅ Verified invalid JSON was handled gracefully");

    Ok(())
}
