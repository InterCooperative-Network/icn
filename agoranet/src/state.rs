use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::federation::Federation;
use crate::services::ServiceRegistry;

/// Shared application state across all routes and services
pub struct AppState {
    /// Database connection pool
    pub db_pool: Arc<Pool<Postgres>>,
    
    /// Federation service (if enabled)
    pub federation: Option<Arc<Federation>>,
    
    /// Services registry
    pub services: ServiceRegistry,
}

impl AppState {
    /// Create a new instance of AppState
    pub fn new(db_pool: Arc<Pool<Postgres>>, federation: Option<Arc<Federation>>, services: ServiceRegistry) -> Self {
        Self { db_pool, federation, services }
    }
    
    /// Get a reference to the database pool
    pub fn db_pool(&self) -> &Pool<Postgres> {
        self.db_pool.as_ref()
    }
    
    /// Get a reference to the federation service (if available)
    pub fn federation(&self) -> Option<&Arc<Federation>> {
        self.federation.as_ref()
    }
} 