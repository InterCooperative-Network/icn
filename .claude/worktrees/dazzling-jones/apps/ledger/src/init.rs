//! Ledger service initialization
//!
//! Initializes the ledger configuration (oracle, witness, membership, credit policy),
//! dispute management, treasury management, contract runtime, and validation hooks.
//!
//! The caller is responsible for:
//! - Creating `Ledger` + `SledStore` beforehand
//! - Wiring runtime handles (gossip, misbehavior, trust) AFTER calling this
//!
//! This module takes only primitive/domain types — no `icn-core` config types.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

use icn_identity::Did;
use icn_ledger::{
    CreditPolicy, CreditPolicyManager, DisputeManager, Ledger, NewMemberPolicy, OracleManager,
    SledMembershipStore, TreasuryManager,
};
use icn_store::SledStore;

/// Timeout for acquiring treasury validation lock (milliseconds).
/// Set to 1000ms (1 second) to handle high load and slow storage backends.
/// If validation times out, the transaction is rejected to prevent unauthorized withdrawals.
const TREASURY_VALIDATION_LOCK_TIMEOUT_MS: u64 = 1000;

/// Services created during ledger initialization.
///
/// Does NOT include `ledger_handle` or `ledger_store` — the caller already owns those.
pub struct LedgerServices {
    /// Dispute manager for handling payment disputes
    pub dispute_manager: Arc<RwLock<DisputeManager>>,
    /// Treasury manager for cooperative treasury operations
    pub treasury_manager: Arc<RwLock<TreasuryManager>>,
    /// Contract runtime for CCL execution
    pub contract_runtime: Arc<RwLock<icn_ccl::ContractRuntime>>,
    /// Contract actor for contract lifecycle management
    pub contract_actor: Arc<RwLock<icn_ccl::ContractActor>>,
}

/// Initialize ledger services (oracle, witness, membership, credit, dispute, treasury, contracts).
///
/// This configures the ledger with oracle, witness, membership, and credit policies,
/// then creates the DisputeManager, TreasuryManager, ContractRuntime, charter validator,
/// combined validation hook, and ContractActor.
///
/// # Arguments
/// * `ledger_handle` - Pre-created Ledger handle (owned by daemon)
/// * `store` - Pre-opened SledStore (shared with Ledger, DisputeManager, TreasuryManager)
/// * `did` - Node's DID
/// * `oracle_config` - Oracle configuration (built from primitive config values by daemon)
/// * `witness_config` - Witness configuration (built from primitive config values by daemon)
pub async fn init_ledger_services(
    ledger_handle: Arc<RwLock<Ledger>>,
    store: Arc<SledStore>,
    did: Did,
    oracle_config: icn_ledger::oracle::OracleConfig,
    witness_config: icn_ledger::WitnessConfig,
) -> anyhow::Result<LedgerServices> {
    // Configure ledger with oracle, witness, membership, and credit policies.
    // Note: This write lock is held for the entire configuration block.
    // This is acceptable because it runs during supervisor startup before any
    // concurrent readers exist. No contention is possible at this point.
    {
        let mut ledger = ledger_handle.write().await;

        // Initialize oracle manager with per-pair rate thresholds from config (Issue #474)
        let threshold_count = oracle_config.suspicious_rate_thresholds.len();
        let default_threshold = oracle_config.default_suspicious_rate_threshold;
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
                default_threshold
            );
        }

        // Initialize witness config for material transaction signatures (Issue #676)
        let witness_policy = format!("{:?}", witness_config.default_policy);
        let witness_threshold = witness_config.threshold;
        let witness_timeout = witness_config.collection_timeout_secs;
        ledger.set_witness_config(witness_config);

        match witness_threshold {
            Some(threshold) => {
                info!(
                    "Witness signatures configured: policy={}, threshold={} (timeout={}s)",
                    witness_policy, threshold, witness_timeout
                );
            }
            None => {
                info!(
                    "Witness signatures configured: policy={} for all transactions (timeout={}s)",
                    witness_policy, witness_timeout
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

    info!("Ledger configured");

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
    // Note: TrustGraph is no longer passed via LedgerDeps; ContractActor will
    // receive it via ServiceRegistry in a future migration (CCL extraction).
    let contract_actor = icn_ccl::ContractActor::new(
        did,
        contract_runtime_handle.clone(),
        // TrustGraph moved to app layer; CCL extraction migration pending.
        // SECURITY NOTE: With None, ContractActor skips deployer trust
        // validation for non-self deployments (logs a warning). This is
        // acceptable during migration because the OracleRegistry will
        // provide trust checks once CCL is extracted to an app crate.
        None,
    );
    let contract_actor_handle = Arc::new(RwLock::new(contract_actor));

    info!("Contract actor created");

    Ok(LedgerServices {
        dispute_manager: dispute_manager_handle,
        treasury_manager: treasury_manager_handle,
        contract_runtime: contract_runtime_handle,
        contract_actor: contract_actor_handle,
    })
}
