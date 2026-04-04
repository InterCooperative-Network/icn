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
    /// Handle to the receipt clearing queue for cross-entity clearing (optional).
    /// When set, a `FederationClearingCallback` is wired so that federated task
    /// completions are queued for periodic batch settlement.
    pub federation_clearing_handle:
        Option<std::sync::Arc<tokio::sync::RwLock<Option<icn_federation::ReceiptClearingManager>>>>,
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
    /// Settlement engine — shared handle for audit queries (task_id → settled status).
    pub settlement_engine: Arc<dyn icn_kernel_api::services::SettlementQueryService>,
}

/// Create the trust callback for compute actor
pub fn create_trust_callback(trust_service: Arc<dyn TrustService>) -> icn_compute::TrustCallback {
    Arc::new(move |did_str: &str| trust_service.trust_score(&did_str.to_string()))
}

/// Create the balance callback for commons credit gate checks.
///
/// Returns the caller's commons credit balance (`"commons-capacity"` currency).
/// Used for the advisory credit floor check and the `CommonsPoolPolicy` credit
/// ceiling check inside `ComputeActor::handle_submit`.
///
/// Uses `try_read()` (non-blocking). On lock contention the callback returns
/// `i64::MAX` so the advisory check passes — the authoritative gate is the
/// `CommonsReserveCallback`, which must be wired separately for race-free
/// credit enforcement.
pub fn create_balance_callback(ledger: LedgerHandle) -> icn_compute::BalanceCallback {
    Arc::new(move |did_str: &str| {
        let did: Did = match did_str.parse() {
            Ok(d) => d,
            Err(_) => {
                warn!(
                    "balance_callback: unparseable DID '{}', returning 0",
                    did_str
                );
                return 0;
            }
        };
        match ledger.try_read() {
            Ok(guard) => {
                // The ICN ledger uses net_change = debit - credit, so `credit` entries
                // (which record earnings) produce a NEGATIVE raw balance. Negate to convert
                // "earned credit balance" → "spendable capacity" (positive = can spend).
                // Example: earn 1000 → raw balance = -1000 → capacity = +1000.
                -guard.get_balance(&did, icn_ledger::commons_credits::COMMONS_CREDIT_CURRENCY)
            }
            Err(_) => {
                // Lock contended; skip advisory check — authoritative gate handles races.
                tracing::debug!(
                    did = did_str,
                    "balance_callback: ledger read-contended, skipping advisory check"
                );
                i64::MAX
            }
        }
    })
}

/// Create the commons settlement callback for posting completed-task journal entries.
///
/// When a commons-scope task completes successfully, the compute actor fires this callback
/// with a `CommonsPaymentRequest`. This function:
///
/// 1. Fetches the consumer's current commons credit balance (for the balance-floor invariant).
/// 2. Converts the compute-layer `CommonsPaymentRequest` into a ledger-layer
///    `CommonsSettlementRequest` and runs it through `SettlementEngine::settle_commons_receipt()`.
/// 3. Appends the resulting `(earn_entry, spend_entry)` to the ledger.
///
/// `engine` must be pre-created by the caller. Use `SettlementEngine::with_store()` for
/// restart-safe idempotence; `SettlementEngine::new()` for process-local-only dedup.
///
/// Duplicate receipts (`LedgerError::DuplicateEntry`) are silently ignored. All other
/// errors are logged as warnings; task completion is not rolled back.
pub fn create_commons_settlement_callback(
    ledger: LedgerHandle,
    engine: Arc<icn_ledger::SettlementEngine>,
) -> icn_compute::CommonsSettlementCallback {
    Arc::new(move |req: icn_compute::CommonsPaymentRequest| {
        let ledger = ledger.clone();
        let engine = engine.clone();

        tokio::spawn(async move {
            // 1. Parse contributor DID (executor — earns credits)
            let contributor: icn_identity::Did = match req.contributor.parse() {
                Ok(d) => d,
                Err(e) => {
                    warn!(
                        "commons_settlement: unparseable contributor DID '{}': {}",
                        req.contributor, e
                    );
                    return;
                }
            };

            // 2. Parse consumer DID (submitter — spends credits)
            let consumer: icn_identity::Did = match req.consumer.parse() {
                Ok(d) => d,
                Err(e) => {
                    warn!(
                        "commons_settlement: unparseable consumer DID '{}': {}",
                        req.consumer, e
                    );
                    return;
                }
            };

            // 3. Fetch consumer balance for the balance-floor invariant.
            //    Negate: ledger credit entries give negative raw balance; negate to get
            //    spendable capacity (positive = can spend). Matches BalanceCallback convention.
            let consumer_balance = {
                let guard = ledger.read().await;
                -guard.get_balance(
                    &consumer,
                    icn_ledger::commons_credits::COMMONS_CREDIT_CURRENCY,
                )
            };

            // 4. Convert amount: u64 → i64 (SettlementEngine uses i64)
            let amount_i64 = match i64::try_from(req.amount) {
                Ok(v) => v,
                Err(_) => {
                    warn!(
                        "commons_settlement: amount {} overflows i64 for task {}",
                        req.amount, req.task_id
                    );
                    return;
                }
            };

            // 5. Build settlement request DTO
            let settlement_req = icn_ledger::CommonsSettlementRequest {
                receipt_hash: req.receipt_hash,
                contributor,
                consumer,
                scope: icn_kernel_api::ScopeLevel::Commons,
                amount: amount_i64,
                consumer_balance,
                // We are the executor — task completed on this node, no external sig needed.
                executor_verified: true,
                task_id: Some(req.task_id.clone()),
            };

            // 6. Run through settlement engine (dedup + invariant checks)
            let (earn_entry, spend_entry) = match engine.settle_commons_receipt(&settlement_req) {
                Ok(entries) => entries,
                Err(icn_ledger::LedgerError::DuplicateEntry(_)) => {
                    // Idempotent — already settled, not an error
                    return;
                }
                Err(e) => {
                    warn!(
                        "commons_settlement: engine rejected receipt {}: {}",
                        hex::encode(req.receipt_hash),
                        e
                    );
                    return;
                }
            };

            // 7. Append both entries to the ledger (double-entry: earn + spend)
            let mut ledger = ledger.write().await;
            if let Err(e) = ledger.append_entry(earn_entry).await {
                warn!(
                    "commons_settlement: failed to append earn entry for task {}: {}",
                    req.task_id, e
                );
                return;
            }
            match ledger.append_entry(spend_entry).await {
                Ok(_) => {
                    info!(
                        "commons_settlement: settled {} credits, consumer='{}', contributor='{}', task={}",
                        req.amount, req.consumer, req.contributor, req.task_id
                    );
                }
                Err(e) => {
                    warn!(
                        "commons_settlement: failed to append spend entry for task {}: {}",
                        req.task_id, e
                    );
                }
            }
        });
    })
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
    // Topics must be explicitly created before subscribe/publish will work.
    // Access controls mirror the compute actor's trust thresholds so the gossip
    // ACL and the executor trust gate are consistent.
    let compute_topics = [
        (
            icn_compute::TOPIC_SUBMIT,
            icn_gossip::AccessControl::MinTrustScore(icn_compute::MIN_TRUST_SUBMIT),
        ),
        (
            icn_compute::TOPIC_CLAIM,
            icn_gossip::AccessControl::MinTrustScore(icn_compute::MIN_TRUST_EXECUTE),
        ),
        (
            icn_compute::TOPIC_RESULT,
            icn_gossip::AccessControl::MinTrustScore(icn_compute::MIN_TRUST_SUBMIT),
        ),
        (
            icn_compute::TOPIC_CANCEL,
            icn_gossip::AccessControl::MinTrustScore(icn_compute::MIN_TRUST_SUBMIT),
        ),
    ];

    for (name, acl) in &compute_topics {
        gossip.create_topic(icn_gossip::Topic::new((*name).to_string(), acl.clone()));
        info!("Created compute gossip topic: {}", name);
    }

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

    // Set up balance callback for commons credit gate checks (advisory floor + ceiling).
    // Must be registered before the actor spawns so handle_submit sees it immediately.
    let balance_callback = create_balance_callback(deps.ledger.clone());
    compute_actor.set_balance_callback(balance_callback);

    // Set up commons settlement engine with sled-backed dedup store.
    // Keyed by receipt hash — survives daemon restart so the same receipt cannot
    // produce duplicate ledger entries across process boundaries.
    let settlement_dedup_path = deps.store_path.join("settlement_dedup");
    let settlement_engine: Arc<icn_ledger::SettlementEngine> = {
        let store = Arc::new(icn_store::SledStore::open(&settlement_dedup_path)?);
        match icn_ledger::SettlementEngine::with_store(store) {
            Ok(e) => Arc::new(e),
            Err(e) => {
                warn!(
                    "Failed to load commons settlement dedup store ({e}); \
                     falling back to in-memory-only dedup"
                );
                Arc::new(icn_ledger::SettlementEngine::new())
            }
        }
    };

    // Set up commons settlement callback — posts earn+spend journal entries to the ledger
    // when a commons-scope task completes successfully (closes the durability gap).
    let commons_settlement_callback =
        create_commons_settlement_callback(deps.ledger.clone(), settlement_engine.clone());
    compute_actor.set_commons_settlement_callback(commons_settlement_callback);

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

    // Wire cross-entity clearing callback (if federation clearing handle is provided)
    if let Some(rcm_handle) = deps.federation_clearing_handle {
        let cb = std::sync::Arc::new(
            move |notification: icn_compute::FederationClearingNotification| {
                let rcm_handle = rcm_handle.clone();
                let receipt = icn_federation::ClearingReceipt {
                    receipt_hash: notification.attestation_hash,
                    executor_did: notification.to_did.clone(),
                    executor_coop: notification.to_entity_id.clone(),
                    submitter_did: notification.from_did.clone(),
                    submitter_coop: notification.from_entity_id.clone(),
                    cost: notification.amount,
                    currency: notification.currency.clone(),
                    scope: notification.scope,
                    created_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    disputed: false,
                    dispute_reason: None,
                };
                // Fire-and-forget: best-effort enqueue. If the lock is poisoned or the
                // manager is uninit'd, we log a warning and continue — clearing is non-critical
                // for task completion.
                tokio::spawn(async move {
                    let guard = rcm_handle.read().await;
                    if let Some(ref rcm) = *guard {
                        if let Err(e) = rcm.submit_receipt(receipt) {
                            warn!("Failed to queue clearing receipt: {e}");
                        }
                    }
                });
            },
        );
        compute_actor.set_federation_clearing_callback(cb);
        info!("Federation clearing callback wired for cross-entity settlement");
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
        settlement_engine: settlement_engine.clone()
            as Arc<dyn icn_kernel_api::services::SettlementQueryService>,
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

    /// Proof test: completed commons compute task produces durable ledger entries.
    ///
    /// This test closes the durability gap identified in the settlement path trace:
    /// `CommonsSettlementCallback` was wired to `None` in the daemon, so completed commons
    /// tasks produced no ledger consequence. After this fix, the callback fires
    /// `SettlementEngine::settle_commons_receipt()` and appends both earn/spend journal
    /// entries to the ledger — making the economic consequence auditable.
    ///
    /// Flow exercised:
    ///   CommonsPaymentRequest → create_commons_settlement_callback → SettlementEngine
    ///   → (earn_entry, spend_entry) → Ledger::append_entry x2
    ///   → get_balance returns updated values
    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn test_commons_settlement_callback_posts_ledger_entries() {
        use icn_identity::KeyPair;
        use std::time::Duration;
        use tokio::sync::RwLock;

        let tmpdir = tempfile::tempdir().unwrap();
        let store = Arc::new(icn_store::SledStore::open(tmpdir.path()).unwrap());
        let ledger = Arc::new(RwLock::new(icn_ledger::Ledger::new(store).unwrap()));

        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();

        let executor_did = executor_kp.did().clone();
        let submitter_did = submitter_kp.did().clone();

        // Give the consumer (submitter) an initial commons credit balance.
        // build_earn_entry credits the consumer (net_change = -amount → raw balance = -10_000).
        // The BalanceCallback negates this to produce spendable capacity = +10_000.
        let initial_credit =
            icn_ledger::commons_credits::build_earn_entry(&submitter_did, 10_000).unwrap();
        ledger
            .write()
            .await
            .append_entry(initial_credit)
            .await
            .unwrap();

        // Verify initial raw ledger balance (negative = credits received)
        let initial_raw = ledger.read().await.get_balance(
            &submitter_did,
            icn_ledger::commons_credits::COMMONS_CREDIT_CURRENCY,
        );
        assert_eq!(
            initial_raw, -10_000,
            "raw balance after earn should be -10_000"
        );

        // Wire the callback (the unit under test) — in-memory engine for this unit test;
        // restart-safe persistence is proven separately in test_settlement_dedup_survives_restart.
        let engine = Arc::new(icn_ledger::SettlementEngine::new());
        let cb = create_commons_settlement_callback(ledger.clone(), engine);

        // Fire the callback as the compute actor does on commons task completion
        let receipt_hash = [0xAB_u8; 32];
        let settlement_amount = 500_u64;
        cb(icn_compute::CommonsPaymentRequest {
            contributor: executor_did.to_string(),
            consumer: submitter_did.to_string(),
            amount: settlement_amount,
            receipt_hash,
            task_id: "test-task-001".to_string(),
        });

        // Yield to let the spawned async task run and append to the ledger.
        // Two yields ensure the task is polled and the write lock is acquired/released.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // --- Assertions: the economic consequence is now a durable ledger fact ---
        //
        // Ledger raw balance convention: net_change = debit - credit.
        // earn_entry_with_receipt: credit(executor, 500) → executor raw = -500
        // spend_entry_with_receipt: debit(consumer, 500) → consumer raw = -10_000 + 500 = -9_500
        //
        // Spendable capacity = -raw_balance (as returned by create_balance_callback).

        let executor_raw = ledger.read().await.get_balance(
            &executor_did,
            icn_ledger::commons_credits::COMMONS_CREDIT_CURRENCY,
        );
        assert_eq!(
            executor_raw,
            -(settlement_amount as i64),
            "executor raw balance after earning: expected -{} (credit entry)",
            settlement_amount
        );

        let submitter_raw = ledger.read().await.get_balance(
            &submitter_did,
            icn_ledger::commons_credits::COMMONS_CREDIT_CURRENCY,
        );
        // submitter started at -10_000 (earn entry), then spend_entry debits +500 → -9_500
        assert_eq!(
            submitter_raw,
            -(10_000 - settlement_amount as i64),
            "submitter raw balance after spending: expected {} (earn -10_000 + debit +500)",
            -(10_000 - settlement_amount as i64)
        );

        // Idempotency: firing the same receipt again must not double-settle
        cb(icn_compute::CommonsPaymentRequest {
            contributor: executor_did.to_string(),
            consumer: submitter_did.to_string(),
            amount: settlement_amount,
            receipt_hash,
            task_id: "test-task-001".to_string(),
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let executor_raw_after_dupe = ledger.read().await.get_balance(
            &executor_did,
            icn_ledger::commons_credits::COMMONS_CREDIT_CURRENCY,
        );
        assert_eq!(
            executor_raw_after_dupe, executor_raw,
            "duplicate receipt must not double-settle the executor balance"
        );
    }
}
