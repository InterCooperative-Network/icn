//! Naming service initialization.
//!
//! Provides supervisor wiring for the app-level `icn-naming` implementation
//! of the kernel `NamingService` trait.

use anyhow::Result;
use icn_kernel_api::naming::NamingService;
use std::sync::Arc;
use tracing::info;

use crate::config::Config;

/// Initialize naming service with sled-backed persistence.
pub fn init_naming_service(config: &Config) -> Result<Arc<dyn NamingService>> {
    let store_path = config.store_path().join("naming");
    let store = Arc::new(icn_store::SledStore::open(&store_path)?);
    let naming = Arc::new(icn_naming::SledNamingService::new(store));

    info!("Naming service initialized at {:?}", store_path);

    Ok(naming)
}
