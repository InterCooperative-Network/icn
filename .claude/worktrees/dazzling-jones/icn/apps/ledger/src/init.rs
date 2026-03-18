//! Initialization helper for ledger stores.
//!
//! Mirrors `apps/governance/src/init.rs` — the caller provides a store path
//! and receives ready-to-use `Arc<dyn EscrowStore>` and `Arc<dyn BudgetStore>`.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use icn_kernel_api::budget::BudgetStore;
use icn_kernel_api::escrow::EscrowStore;
use icn_store::SledStore;

use crate::budget::SledBudgetStore;
use crate::escrow::SledEscrowStore;

/// Stores returned from ledger initialization.
pub struct LedgerStores {
    /// Escrow store for domain-level idempotency on escrow releases.
    pub escrow_store: Arc<dyn EscrowStore>,
    /// Budget store for budget enforcement.
    pub budget_store: Arc<dyn BudgetStore>,
}

/// Create the ledger stores (escrow + budget) rooted at `store_path`.
///
/// Opens:
/// - `<store_path>/escrow/` — Sled KV store via `icn_store::SledStore`
/// - `<store_path>/budget/` — Sled DB with a named tree
pub fn create_stores(store_path: &Path) -> Result<LedgerStores> {
    let escrow_store_path = store_path.join("escrow");
    let escrow_sled_store: Arc<SledStore> = Arc::new(SledStore::open(&escrow_store_path)?);
    let escrow_store: Arc<dyn EscrowStore> = Arc::new(SledEscrowStore::new(escrow_sled_store));

    let budget_store_path = store_path.join("budget");
    let budget_sled_db = sled::open(&budget_store_path)?;
    let budget_store: Arc<dyn BudgetStore> = Arc::new(SledBudgetStore::new(&budget_sled_db)?);

    Ok(LedgerStores {
        escrow_store,
        budget_store,
    })
}
