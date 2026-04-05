//! Initialization helper for ledger stores.
//!
//! Mirrors `apps/governance/src/init.rs` — the caller provides a store path
//! and receives ready-to-use `Arc<dyn EscrowStore>`, `Arc<dyn BudgetStore>`,
//! and `Arc<dyn ResourceAccessStore>`.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use icn_kernel_api::budget::BudgetStore;
use icn_kernel_api::escrow::EscrowStore;
use icn_kernel_api::resource::ResourceAccessStore;
use icn_store::SledStore;

use crate::budget::SledBudgetStore;
use crate::escrow::SledEscrowStore;
use crate::resource_access::SledResourceAccessStore;

/// Stores returned from ledger initialization.
pub struct LedgerStores {
    /// Escrow store for domain-level idempotency on escrow releases.
    pub escrow_store: Arc<dyn EscrowStore>,
    /// Budget store for budget enforcement.
    pub budget_store: Arc<dyn BudgetStore>,
    /// Resource access store for governance-authorized grants.
    pub resource_access_store: Arc<dyn ResourceAccessStore>,
}

/// Create the ledger stores (escrow + budget + resource_access) rooted at `store_path`.
///
/// Opens:
/// - `<store_path>/escrow/` — Sled KV store via `icn_store::SledStore`
/// - `<store_path>/budget/` — Sled DB with a named tree
/// - `<store_path>/resource_access/` — Sled DB with a named tree
pub fn create_stores(store_path: &Path) -> Result<LedgerStores> {
    let escrow_store_path = store_path.join("escrow");
    let escrow_sled_store: Arc<SledStore> = Arc::new(SledStore::open(&escrow_store_path)?);
    let escrow_store: Arc<dyn EscrowStore> = Arc::new(SledEscrowStore::new(escrow_sled_store));

    let budget_store_path = store_path.join("budget");
    let budget_sled_db = sled::open(&budget_store_path)?;
    let budget_store: Arc<dyn BudgetStore> = Arc::new(SledBudgetStore::new(&budget_sled_db)?);

    let resource_access_path = store_path.join("resource_access");
    let resource_access_db = sled::open(&resource_access_path)?;
    let resource_access_store: Arc<dyn ResourceAccessStore> =
        Arc::new(SledResourceAccessStore::new(&resource_access_db)?);

    Ok(LedgerStores {
        escrow_store,
        budget_store,
        resource_access_store,
    })
}
