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
    CreditPolicy, CreditPolicyManager, DisputeManager, Ledger, NewMemberPolicy,
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
    pub trust_graph: Arc<RwLock<TrustGraph>>,
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
    // Create ledger store
    let store_path = config.store_path().join("ledger");
    let store = Arc::new(SledStore::open(&store_path)?);

    // Initialize ledger with gossip, misbehavior detection, and trust graph
    let mut ledger = Ledger::new(store.clone())?;
    ledger.set_gossip(deps.gossip_handle.clone());
    ledger.set_misbehavior_detector(deps.misbehavior_detector.clone());
    ledger.set_trust_graph(deps.trust_graph.clone());

    // Initialize membership store for tracking when members joined
    // Used for new member credit limit ramping
    let membership_store = Arc::new(SledMembershipStore::new(store.clone()));
    ledger.set_membership_store(membership_store);

    info!("Membership store initialized for new member tracking");

    // Initialize credit policy for server-side credit limit enforcement.
    // Dynamic limits: baseline + trust bonus + history bonus.
    // Note: Using "hours" as the default currency unit for cooperatives.
    let credit_policy = CreditPolicy::conservative("hours".to_string());

    // New member protection policy configuration.
    // New members start with 10 hour limit, ramping to full over 90 days.
    // Members who contribute 50+ hours get full limit regardless of tenure.
    let new_member_policy = NewMemberPolicy::conservative("hours".to_string());

    let credit_manager = CreditPolicyManager::new(credit_policy, new_member_policy);
    ledger.set_credit_policy_manager(credit_manager);

    info!(
        "Credit policy manager initialized with conservative policy for 'hours' currency \
         (new members: 10hr initial, 90-day ramp, 50hr contribution threshold)"
    );

    let ledger_handle = Arc::new(RwLock::new(ledger));

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
