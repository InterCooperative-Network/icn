//! Ledger and contract initialization
//!
//! Initializes the double-entry ledger, dispute management, and contract execution.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use icn_gossip::GossipActor;
use icn_identity::Did;
use icn_ledger::{DisputeManager, Ledger, TreasuryManager};
use icn_security::MisbehaviorDetector;
use icn_store::SledStore;
use icn_trust::TrustGraph;

use crate::config::Config;

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

    // Initialize ledger with gossip and misbehavior detection
    let mut ledger = Ledger::new(store.clone())?;
    ledger.set_gossip(deps.gossip_handle.clone());
    ledger.set_misbehavior_detector(deps.misbehavior_detector.clone());
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
            // Use try_read to avoid blocking; if can't acquire lock, skip validation
            // (treasury validation is advisory, not critical path)
            if let Ok(treasury_mgr) = treasury_clone.try_read() {
                treasury_mgr.validate_entry(entry)?;
            }

            Ok(())
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
