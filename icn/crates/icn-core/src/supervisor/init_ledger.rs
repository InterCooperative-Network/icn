//! Ledger and contract initialization
//!
//! Initializes the double-entry ledger, dispute management, and contract execution.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

use icn_gossip::GossipActor;
use icn_identity::Did;
use icn_ledger::{
    CreditPolicy, CreditPolicyManager, DisputeManager, Ledger, NewMemberPolicy, OracleManager,
    SledMembershipStore, TreasuryManager,
};
use icn_security::MisbehaviorDetector;
use icn_store::SledStore;
use icn_trust::TrustGraph;

use crate::config::Config;

/// Timeout for acquiring treasury validation lock (milliseconds)
/// Set to 1000ms (1 second) to handle high load and slow storage backends.
/// If validation times out, the transaction is rejected to prevent unauthorized withdrawals.
const TREASURY_VALIDATION_LOCK_TIMEOUT_MS: u64 = 1000;

/// Services initialized during ledger setup
pub struct LedgerServices {
    /// The ledger handle
    pub ledger_handle: Arc<RwLock<Ledger>>,
    /// Dispute manager for handling payment disputes
    pub dispute_manager: Arc<RwLock<DisputeManager>>,
    /// Treasury manager for cooperative treasury operations
    pub treasury_manager: Arc<RwLock<TreasuryManager>>,
    /// Contract runtime for CCL execution
    pub contract_runtime: Arc<RwLock<icn_ccl::ContractRuntime>>,
    /// Contract actor for contract lifecycle management
    pub contract_actor: Arc<RwLock<icn_ccl::ContractActor>>,
    /// Ledger store (shared with dispute manager)
    pub ledger_store: Arc<SledStore>,
}

/// Dependencies for ledger initialization
pub struct LedgerDeps {
    pub gossip_handle: Arc<RwLock<GossipActor>>,
    pub misbehavior_detector: Arc<RwLock<MisbehaviorDetector>>,
    /// Trust graph for fallback (deprecated, kept for backward compatibility)
    pub trust_graph: Arc<RwLock<TrustGraph>>,
    /// Trust service for proper kernel/app separation
    /// When provided, passed to Ledger.set_trust_service()
    pub trust_service: Option<Arc<dyn icn_kernel_api::services::TrustService>>,
    /// Pre-created ledger handle from daemon (daemon owns lifecycle)
    pub ledger_handle: Option<Arc<RwLock<Ledger>>>,
}

/// Initialize ledger and contract services
///
/// Creates:
/// - Ledger with gossip and misbehavior integration
/// - DisputeManager for payment dispute resolution
/// - ContractRuntime for CCL execution
/// - ContractActor for contract lifecycle
pub async fn init_ledger_services(
    config: &Config,
    did: Did,
    deps: LedgerDeps,
) -> anyhow::Result<LedgerServices> {
    // Open ledger store for DisputeManager, TreasuryManager, and (if no daemon handle) the Ledger.
    // Safe to call even when the daemon already opened this path: sled internally caches
    // DB instances by path and returns the same reference-counted sled::Db.
    let store_path = config.ledger_store_path();
    let store = Arc::new(SledStore::open(&store_path)?);

    // Use daemon-provided ledger handle or create a new one
    let ledger_handle = if let Some(handle) = deps.ledger_handle {
        info!("Using daemon-provided Ledger handle");
        handle
    } else {
        Arc::new(RwLock::new(Ledger::new(store.clone())?))
    };

    // Configure ledger with gossip, misbehavior detection, trust, and policies
    {
        let mut ledger = ledger_handle.write().await;

        ledger.set_gossip(deps.gossip_handle.clone());
        ledger.set_misbehavior_detector(deps.misbehavior_detector.clone());

        // Use TrustService if available (kernel/app separation), else fall back to TrustGraph
        if let Some(trust_service) = deps.trust_service.clone() {
            ledger.set_trust_service(trust_service);
            info!("Ledger initialized with TrustService (kernel/app separated)");
        } else {
            ledger.set_trust_graph(deps.trust_graph.clone());
            info!("Ledger initialized with TrustGraph (deprecated fallback)");
        }

        // Initialize oracle manager with per-pair rate thresholds from config (Issue #474)
        let oracle_config = config.ledger.oracle.to_oracle_config();
        let threshold_count = oracle_config.suspicious_rate_thresholds.len();
        let oracle_manager = Arc::new(OracleManager::with_config(store.clone(), oracle_config));
        ledger.set_oracle_manager(oracle_manager);

        if threshold_count > 0 {
            info!(
                "Oracle manager initialized with {} per-pair rate threshold(s)",
                threshold_count
            );
        } else {
            info!(
                "Oracle manager initialized with default threshold {}",
                config.ledger.oracle.default_suspicious_rate_threshold
            );
        }

        // Initialize witness config for material transaction signatures (Issue #676)
        let witness_config = config.ledger.witness.to_witness_config()?;
        let witness_policy = format!("{:?}", witness_config.default_policy);
        ledger.set_witness_config(witness_config);

        match &config.ledger.witness.threshold {
            Some(threshold) => {
                info!(
                    "Witness signatures configured: policy={}, threshold={} (timeout={}s)",
                    witness_policy, threshold, config.ledger.witness.collection_timeout_secs
                );
            }
            None => {
                info!(
                    "Witness signatures configured: policy={} for all transactions (timeout={}s)",
                    witness_policy, config.ledger.witness.collection_timeout_secs
                );
            }
        }

        // Initialize membership store for tracking when members joined
        let membership_store = Arc::new(SledMembershipStore::new(store.clone()));
        ledger.set_membership_store(membership_store);
        info!("Membership store initialized for new member tracking");

        // Initialize credit policy for server-side credit limit enforcement
        let credit_policy = CreditPolicy::conservative("hours".to_string());
        let new_member_policy = NewMemberPolicy::conservative("hours".to_string());
        let credit_manager = CreditPolicyManager::new(credit_policy, new_member_policy);
        ledger.set_credit_policy_manager(credit_manager);

        info!(
            "Credit policy manager initialized with conservative policy for 'hours' currency \
             (new members: 10hr initial, 90-day ramp, 50hr contribution threshold)"
        );
    }

    info!("Ledger initialized at {}", store_path.display());

    // Initialize DisputeManager (shares store with Ledger)
    let dispute_manager = DisputeManager::new(store.clone())?;
    let dispute_manager_handle = Arc::new(RwLock::new(dispute_manager));

    info!("Dispute manager initialized");

    // Initialize TreasuryManager (shares store with Ledger)
    let treasury_manager = TreasuryManager::with_store(store.clone())?;
    let treasury_manager_handle = Arc::new(RwLock::new(treasury_manager));

    info!("Treasury manager initialized");

    // Initialize Contract Runtime
    let contract_runtime = icn_ccl::ContractRuntime::new(ledger_handle.clone());
    let contract_runtime_handle = Arc::new(RwLock::new(contract_runtime));

    info!("Contract runtime initialized");

    // Initialize Charter Validator for cooperative policy enforcement (Gap #2)
    // Uses default cooperative rules with min trust 500 basis points (0.5)
    let domain_id = format!("coop:{did}"); // Use DID as default domain ID
    let charter_validator = Arc::new(icn_ccl::CharterValidator::cooperative_default(
        domain_id, 500, // Min trust = 0.5 (500 basis points)
    ));

    // Set up combined validation hook on ledger (charter rules + treasury rules)
    {
        let mut ledger = ledger_handle.write().await;
        let validator_clone = charter_validator.clone();
        let treasury_clone = treasury_manager_handle.clone();
        ledger.set_validation_hook(move |entry| {
            // First validate charter rules
            validator_clone.validate_entry(entry)?;

            // Then validate treasury spending rules
            // SECURITY: Treasury validation is critical - unauthorized withdrawals must be blocked.
            // Use timeout-based lock acquisition to balance availability with security.
            let timeout_duration = Duration::from_millis(TREASURY_VALIDATION_LOCK_TIMEOUT_MS);

            // SAFETY: Use block_in_place to acquire async lock in sync validation context.
            // The validation callback is sync (required by ledger API), but treasury access
            // is async. block_in_place moves other tokio tasks off this thread before blocking.
            let validation_result = tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    match tokio::time::timeout(timeout_duration, treasury_clone.read()).await {
                        Ok(treasury_mgr) => {
                            let result = treasury_mgr.validate_entry(entry);
                            if result.is_ok() {
                                icn_obs::metrics::treasury::validation_success_inc();
                            } else {
                                icn_obs::metrics::treasury::validation_failed_inc();
                            }
                            result
                        }
                        Err(_timeout) => {
                            // Lock acquisition timeout - track metric and reject
                            icn_obs::metrics::treasury::validation_lock_contention_inc();
                            let timeout_ms = TREASURY_VALIDATION_LOCK_TIMEOUT_MS;
                            Err(anyhow::anyhow!(
                                "Treasury validation timeout after {timeout_ms}ms - please retry. \
                                 This prevents unauthorized withdrawals during high contention."
                            ))
                        }
                    }
                })
            });

            validation_result
        });
    }

    info!("Charter and treasury validators initialized with validation hook");

    // Create ContractActor
    let contract_actor = icn_ccl::ContractActor::new(
        did,
        contract_runtime_handle.clone(),
        Some(deps.trust_graph.clone()),
    );
    let contract_actor_handle = Arc::new(RwLock::new(contract_actor));

    info!("Contract actor created");

    Ok(LedgerServices {
        ledger_handle,
        dispute_manager: dispute_manager_handle,
        treasury_manager: treasury_manager_handle,
        contract_runtime: contract_runtime_handle,
        contract_actor: contract_actor_handle,
        ledger_store: store,
    })
}
