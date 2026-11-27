//! The node's single handle on `<data_dir>/gateway_store`.
//!
//! On a node with an identity, the daemon opens this database exactly once
//! (in `icnd`'s service construction) and shares it two ways: the receipt
//! store's trait view goes to the governance actor and dispatch-evidence sink
//! through the supervisor's governance evidence initialization, and this
//! concrete bundle goes to the gateway through
//! [`crate::server::GatewayServer::with_shared_store`]. One open means the
//! receipt store's legacy index migration runs once and no second component
//! contends for the sled lock. Same path and keys as before; no data
//! migration.

use std::path::Path;
use std::sync::Arc;

use crate::receipt_store::ReceiptStore;

/// Directory name of the gateway database under the node data directory.
pub const GATEWAY_STORE_DIR: &str = "gateway_store";

/// The opened gateway database and the receipt store built over it.
#[derive(Clone)]
pub struct SharedGatewayStore {
    db: sled::Db,
    receipt_store: Arc<ReceiptStore>,
}

impl SharedGatewayStore {
    /// Open `<data_dir>/gateway_store` and build the receipt store over it
    /// (which runs the receipt store's legacy by-proposal index migration).
    ///
    /// Fails if the database cannot be opened, for example because another
    /// holder has it locked.
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        let path = data_dir.join(GATEWAY_STORE_DIR);
        let db = sled::open(&path)
            .map_err(|e| format!("open gateway store at {}: {e}", path.display()))?;
        Ok(Self::from_db(db))
    }

    /// Build the bundle over an already-open database.
    pub fn from_db(db: sled::Db) -> Self {
        let receipt_store = Arc::new(ReceiptStore::new(db.clone()));
        Self { db, receipt_store }
    }

    /// The database, for the gateway's other sled-backed stores.
    pub fn db(&self) -> &sled::Db {
        &self.db
    }

    /// The receipt store over [`Self::db`].
    pub fn receipt_store(&self) -> &Arc<ReceiptStore> {
        &self.receipt_store
    }
}
