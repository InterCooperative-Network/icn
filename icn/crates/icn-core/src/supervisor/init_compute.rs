//! Compute actor initialization and callback setup
//!
//! This module extracts the compute actor setup from the supervisor,
//! providing a cleaner separation of concerns for distributed compute.

use icn_identity::Did;
use icn_kernel_api::services::TrustService;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Type aliases for common handle types
pub type LedgerHandle = super::actors::LedgerHandle;
pub type GossipHandle = Arc<RwLock<icn_gossip::GossipActor>>;
pub type ComputeHandleHolder = Arc<RwLock<Option<icn_compute::ComputeHandle>>>;
pub type DisputeHandleHolder = Arc<RwLock<Option<icn_ccl::DisputeActorHandle>>>;

/// Dependencies required for compute actor initialization
#[derive(Clone)]
pub struct ComputeDeps {
    /// Trust service for trust score lookups (kernel/app separated)
    pub trust_service: Arc<dyn TrustService>,
    /// Ledger for payment settlement
    pub ledger: LedgerHandle,
    /// Gossip handle for message routing
    pub gossip_handle: GossipHandle,
    /// This node's DID
    pub own_did: Did,
    /// Compute handle holder (filled after spawn)
    pub compute_handle_holder: ComputeHandleHolder,
    /// Dispute handle holder (filled after spawn)
    pub dispute_handle_holder: DisputeHandleHolder,
    /// Network handle for neighbor sets
    pub network_handle: icn_net::NetworkHandle,
    /// Misbehavior detector for Byzantine fault detection
    pub misbehavior_detector: Arc<RwLock<icn_security::MisbehaviorDetector>>,
    /// Identity bundle for signing
    pub identity_bundle: icn_identity::IdentityBundle,
    /// Store path for dispute storage
    pub store_path: std::path::PathBuf,
    /// Contract registry for CclRef resolution (optional)
    pub contract_registry: Option<icn_ccl::ContractRegistryHandle>,
    /// Governance policy thresholds for commons pool admission and cost estimation
    pub policy_config: crate::config::ComputePolicyConfig,
}

/// Services returned from compute initialization
pub struct ComputeServices {
    /// Handle to the compute actor
    pub compute_handle: icn_compute::ComputeHandle,
    /// Handle to the dispute actor
    pub dispute_handle: icn_ccl::DisputeActorHandle,
    /// Event broadcaster for WebSocket delivery
    pub broadcaster: Arc<icn_gateway::EventBroadcaster>,
    /// Policy manager for cooperative scheduling
    pub policy_manager: Arc<icn_compute::PolicyManager>,
}

/// Create the trust callback for compute actor
pub fn create_trust_callback(trust_service: Arc<dyn TrustService>) -> icn_compute::TrustCallback {
    Arc::new(move |did_str: &str| trust_service.trust_score(&did_str.to_string()))
}

/// Create the send callback for routing compute messages through gossip
pub fn create_send_callback(gossip_handle: GossipHandle) -> icn_compute::SendCallback {
    Arc::new(move |compute_msg| {
        let gossip = gossip_handle.clone();
        tokio::spawn(async move {
            let (topic, data) = get_message_topic_and_data(&compute_msg);

            match data {
                Ok(bytes) => {
                    let mut gossip = gossip.write().await;
                    if let Err(e) = gossip.publish(topic, bytes).await {
                        warn!("Failed to publish compute message to {}: {}", topic, e);
                    }
                }
                Err(e) => {
                    warn!("Failed to serialize compute message: {}", e);
                }
            }
        });
    })
}

/// Get the topic and serialized data for a compute message
fn get_message_topic_and_data(
    compute_msg: &icn_compute::ComputeMessage,
) -> (&'static str, Result<Vec<u8>, icn_encoding::Error>) {
    use icn_compute::ComputeMessage;

    match compute_msg {
        ComputeMessage::TaskSubmitted(_) => {
            (icn_compute::TOPIC_SUBMIT, icn_encoding::encode(compute_msg))
        }
        ComputeMessage::TaskClaimed { .. } => {
            (icn_compute::TOPIC_CLAIM, icn_encoding::encode(compute_msg))
        }
        ComputeMessage::TaskResult(_) => {
            (icn_compute::TOPIC_RESULT, icn_encoding::encode(compute_msg))
        }
        ComputeMessage::TaskCancelled { .. } => {
            (icn_compute::TOPIC_CANCEL, icn_encoding::encode(compute_msg))
        }
        ComputeMessage::ExecutorAnnounce { .. } => {
            (icn_compute::TOPIC_SUBMIT, icn_encoding::encode(compute_msg))
        }
        ComputeMessage::PlacementRequest { .. } => {
            (icn_compute::TOPIC_SUBMIT, icn_encoding::encode(compute_msg))
        }
        ComputeMessage::PlacementOffer { .. } => {
            (icn_compute::TOPIC_CLAIM, icn_encoding::encode(compute_msg))
        }
        ComputeMessage::NodeCapacityAnnounce { .. } => {
            (icn_compute::TOPIC_SUBMIT, icn_encoding::encode(compute_msg))
        }
        ComputeMessage::CheckpointQuery { .. } => (
            icn_compute::TOPIC_CHECKPOINT,
            icn_encoding::encode(compute_msg),
        ),
        ComputeMessage::CheckpointResponse { .. } => (
            icn_compute::TOPIC_CHECKPOINT,
            icn_encoding::encode(compute_msg),
        ),
        ComputeMessage::MigrationRequest { .. } => (
            icn_compute::TOPIC_MIGRATION,
            icn_encoding::encode(compute_msg),
        ),
        ComputeMessage::MigrationAccept { .. } => (
            icn_compute::TOPIC_MIGRATION,
            icn_encoding::encode(compute_msg),
        ),
        ComputeMessage::CheckpointAnnounce { .. } => (
            icn_compute::TOPIC_CHECKPOINT,
            icn_encoding::encode(compute_msg),
        ),
        ComputeMessage::MigrationReject { .. } => (
            icn_compute::TOPIC_MIGRATION,
            icn_encoding::encode(compute_msg),
        ),
        ComputeMessage::MigrationComplete { .. } => (
            icn_compute::TOPIC_MIGRATION,
            icn_encoding::encode(compute_msg),
        ),
        // Phase 21: Cross-cooperative federation messages
        ComputeMessage::FederatedExecutorAnnounce { .. } => (
            icn_compute::TOPIC_FEDERATION,
            icn_encoding::encode(compute_msg),
        ),
        ComputeMessage::FederatedTaskRequest { .. } => (
            icn_compute::TOPIC_FEDERATION,
            icn_encoding::encode(compute_msg),
        ),
        ComputeMessage::FederatedTaskResult { .. } => (
            icn_compute::TOPIC_FEDERATION,
            icn_encoding::encode(compute_msg),
        ),
    }
}

/// Create the payment callback for settling compute payments via ledger
pub fn create_payment_callback(ledger: LedgerHandle) -> icn_compute::PaymentCallback {
    Arc::new(move |req| {
        let ledger = ledger.clone();
        tokio::spawn(async move {
            // Parse DIDs
            let from_did: Did =
                match serde_json::from_value(serde_json::Value::String(req.from.clone())) {
                    Ok(d) => d,
                    Err(e) => {
                        warn!("Failed to parse payer DID: {}", e);
                        return;
                    }
                };
            let to_did: Did =
                match serde_json::from_value(serde_json::Value::String(req.to.clone())) {
                    Ok(d) => d,
                    Err(e) => {
                        warn!("Failed to parse payee DID: {}", e);
                        return;
                    }
                };

            // Security (#1342): `req.from` originates from `claimed_task.submitter`, which is
            // set by the task submitter at submission time. When tasks arrive via the gateway
            // REST API the submitter DID is extracted from the JWT (authenticated). However,
            // when a TaskSubmitted message is received via gossip, the submitter field comes
            // from the gossip payload and could be spoofed by a malicious peer to name a
            // victim DID as payer.
            //
            // Full fix requires the gossip layer to verify `entry.author == task.submitter`
            // before calling `on_task_submitted` (tracked in #1342). Until that is in place,
            // we record the payer DID in the provenance reason so audit logs can detect
            // mismatches. We intentionally do NOT use system provenance without the payer DID
            // to avoid hiding who was charged.
            let amount_i64 = match i64::try_from(req.amount) {
                Ok(v) => v,
                Err(_) => {
                    warn!(
                        "Payment amount overflow: cannot convert {} to i64 for compute payment \
                         (task {}, from {}, to {})",
                        req.amount, req.task_id, req.from, req.to
                    );
                    return;
                }
            };
            let provenance_reason = format!("compute-payment:payer={}", req.from);
            let entry = match icn_ledger::entry::JournalEntryBuilder::new(from_did.clone())
                .debit(from_did, req.currency.clone(), amount_i64)
                .credit(to_did, req.currency.clone(), amount_i64)
                .with_system_provenance(provenance_reason)
                .build()
            {
                Ok(e) => e,
                Err(e) => {
                    warn!("Failed to build payment entry: {}", e);
                    return;
                }
            };

            // Append to ledger
            let mut ledger = ledger.write().await;
            match ledger.append_entry(entry).await {
                Ok(_) => {
                    info!(
                        "Compute payment settled: {} {} from {} to {} for task {}",
                        req.amount, req.currency, req.from, req.to, req.task_id
                    );
                }
                Err(e) => {
                    warn!("Failed to settle compute payment: {}", e);
                }
            }
        });
    })
}

/// Create the event callback for task status changes
pub fn create_event_callback(
    broadcaster: Arc<icn_gateway::EventBroadcaster>,
) -> icn_compute::EventCallback {
    Arc::new(move |event| {
        // Log and update metrics
        match &event {
            icn_compute::ComputeEvent::TaskClaimed {
                task_hash,
                executor,
            } => {
                info!("Task claimed: {} by {}", task_hash, executor);
                icn_obs::metrics::compute::tasks_claimed_inc();
            }
            icn_compute::ComputeEvent::TaskCompleted {
                task_hash,
                executor,
                outcome,
                fuel_used,
                duration_ms,
            } => {
                info!(
                    "Task completed: {} by {} - outcome: {}, fuel: {}, duration: {}ms",
                    task_hash, executor, outcome, fuel_used, duration_ms
                );
                icn_obs::metrics::compute::tasks_completed_inc(outcome);
            }
            // Resource changes are internal monitoring events, already logged
            icn_compute::ComputeEvent::ResourcesChanged { .. } => {}
        }

        // Forward to EventBroadcaster for WebSocket delivery
        let broadcaster = broadcaster.clone();
        tokio::spawn(async move {
            icn_gateway::forward_compute_event(&broadcaster, event).await;
        });
    })
}

/// Create the locality callback for network topology data
pub fn create_locality_callback(
    neighbor_sets: Arc<RwLock<icn_net::NeighborSets>>,
) -> icn_compute::LocalityCallback {
    Arc::new(move |submitter_did: &str| {
        // Parse DID and get RTT if available
        let submitter_rtt_ms: Option<u64> =
            serde_json::from_value::<Did>(serde_json::Value::String(submitter_did.to_string()))
                .ok()
                .and_then(|did| {
                    let peer = icn_net::PeerId(did);
                    let sets = neighbor_sets.blocking_read();
                    sets.get_rtt(&peer)
                });

        icn_compute::LocalityContext {
            submitter_rtt_ms,
            local_blob_count: 0,
            total_blob_count: 0,
            own_region: None,
            submitter_region: None,
        }
    })
}

/// Subscribe to compute-related gossip topics
pub async fn subscribe_compute_topics(gossip: &mut icn_gossip::GossipActor, did: &Did) {
    for topic in &[
        icn_compute::TOPIC_SUBMIT,
        icn_compute::TOPIC_CLAIM,
        icn_compute::TOPIC_RESULT,
        icn_compute::TOPIC_CANCEL,
    ] {
        if let Err(e) = gossip.subscribe(topic, did.clone()).await {
            warn!("Failed to subscribe to compute topic {}: {}", topic, e);
        } else {
            info!("Subscribed to compute topic: {}", topic);
        }
    }

    // Subscribe to disputes topic
    if let Err(e) = gossip
        .subscribe(icn_ccl::TOPIC_DISPUTES_FILE, did.clone())
        .await
    {
        warn!("Failed to subscribe to disputes topic: {}", e);
    } else {
        info!("Subscribed to disputes topic");
    }
}

/// Initialize the compute actor and related services
///
/// This sets up the compute actor with all required callbacks:
/// - Trust callback for trust score lookups
/// - Send callback for gossip routing
/// - Payment callback for ledger settlement
/// - Event callback for status changes and WebSocket delivery
/// - Locality callback for network topology
///
/// Returns the compute handle, dispute handle, event broadcaster, and policy manager.
pub async fn init_compute_services(deps: ComputeDeps) -> anyhow::Result<ComputeServices> {
    // Create trust callback
    let trust_callback = create_trust_callback(deps.trust_service.clone());

    // Create compute actor
    let mut compute_actor =
        icn_compute::ComputeActor::new(deps.own_did.to_string(), trust_callback);

    // Set up send callback
    let send_callback = create_send_callback(deps.gossip_handle.clone());
    compute_actor.set_send_callback(send_callback);

    // Set up payment callback
    let payment_callback = create_payment_callback(deps.ledger.clone());
    compute_actor.set_payment_callback(payment_callback);

    // Create event broadcaster
    let broadcaster = Arc::new(icn_gateway::EventBroadcaster::new());

    // Set up event callback
    let event_callback = create_event_callback(broadcaster.clone());
    compute_actor.set_event_callback(event_callback);

    // Initialize policy manager
    let usage_tracker = Arc::new(icn_compute::UsageTracker::new());
    let policy_manager = Arc::new(icn_compute::PolicyManager::new(usage_tracker.clone()));
    compute_actor.set_policy_manager(policy_manager.clone());

    // Apply governance policy thresholds from config.
    // min_standing, fuel_cost_divisor, credit_ceiling, and preemptable_priorities
    // go into CommonsPoolPolicy (submission gate + preemption control).
    // min_trust_score goes into SybilPolicy (pool admission gate).
    let commons_pool_policy = icn_compute::CommonsPoolPolicy {
        min_standing: deps.policy_config.min_standing,
        fuel_cost_divisor: deps.policy_config.fuel_cost_divisor,
        credit_ceiling: deps.policy_config.credit_ceiling,
        preemptable_priorities: deps.policy_config.preemptable_priorities.clone(),
        ..icn_compute::CommonsPoolPolicy::default()
    };
    compute_actor.set_commons_pool_policy(commons_pool_policy);

    let sybil_policy = icn_compute::SybilPolicy {
        min_trust_score: deps.policy_config.min_trust_score,
        ..icn_compute::SybilPolicy::default()
    };
    compute_actor.set_commons_sybil_policy(sybil_policy);

    info!(
        "Policy manager initialized (min_standing={}, min_trust_score={}, fuel_cost_divisor={}, credit_ceiling={:?}, preemptable_priorities={:?})",
        deps.policy_config.min_standing,
        deps.policy_config.min_trust_score,
        deps.policy_config.fuel_cost_divisor,
        deps.policy_config.credit_ceiling,
        deps.policy_config.preemptable_priorities
    );

    // Spawn DisputeActor with shared system
    let dispute_store_path = deps.store_path.join("disputes");
    let dispute_store: Arc<dyn icn_store::Store> =
        Arc::new(icn_store::SledStore::open(&dispute_store_path)?);
    let dispute_config = icn_ccl::DisputeConfig::default();
    let (dispute_system, dispute_handle) =
        icn_ccl::DisputeActor::spawn_shared(dispute_config.clone(), dispute_store);

    // Fill dispute handle holder
    *deps.dispute_handle_holder.write().await = Some(dispute_handle.clone());

    info!(
        "DisputeActor spawned (re-execution timeout: {:?})",
        dispute_config.re_execution_timeout
    );

    // Set signing key
    let signing_key_bytes = deps.identity_bundle.keypair()?.to_signing_key_bytes();
    compute_actor.set_signing_key(signing_key_bytes.to_vec());

    // Connect dispute system
    compute_actor.set_dispute_resolution(dispute_system);

    // Set misbehavior detector
    compute_actor.set_misbehavior_detector(deps.misbehavior_detector.clone());

    // Set locality callback if neighbor sets available
    if let Some(ref neighbor_sets) = deps.network_handle.neighbor_sets() {
        let locality_callback = create_locality_callback(neighbor_sets.clone());
        compute_actor.set_locality_callback(locality_callback);
        info!("Locality callback set for compute placement");
    }

    // Set contract registry for CclRef resolution
    if let Some(ref registry) = deps.contract_registry {
        compute_actor.set_contract_registry(registry.clone());
        info!("Contract registry set for CclRef resolution");
    }

    // Wire WASM registry and executor
    #[cfg(feature = "wasm")]
    {
        let wasm_store_path = deps.store_path.join("wasm");
        match sled::open(&wasm_store_path) {
            Ok(db) => {
                let registry = std::sync::Arc::new(icn_compute::WasmRegistry::with_store(db));
                compute_actor.set_wasm_registry(registry.clone());
                compute_actor.wire_wasm_executor(registry).await;
                info!(
                    "WASM registry and executor wired (sled path: {:?})",
                    wasm_store_path
                );
            }
            Err(e) => {
                warn!(
                    "Failed to open WASM sled store at {:?}: {e}",
                    wasm_store_path
                );
                warn!("WASM execution will not be available");
            }
        }
    }

    // Spawn the compute actor
    let compute_handle = compute_actor.spawn();
    icn_obs::metrics::supervisor::actor_spawned_inc("compute");

    // Fill compute handle holder
    *deps.compute_handle_holder.write().await = Some(compute_handle.clone());

    info!("Compute actor spawned with payment settlement");

    // Subscribe to topics
    {
        let mut gossip = deps.gossip_handle.write().await;
        subscribe_compute_topics(&mut gossip, &deps.own_did).await;
    }

    Ok(ComputeServices {
        compute_handle,
        dispute_handle,
        broadcaster,
        policy_manager,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_deps_clone() {
        // Verify ComputeDeps implements Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<ComputeDeps>();
    }

    #[test]
    fn test_compute_services_struct() {
        // Verify ComputeServices fields are accessible
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Arc<icn_gateway::EventBroadcaster>>();
        assert_send_sync::<Arc<icn_compute::PolicyManager>>();
    }
}
