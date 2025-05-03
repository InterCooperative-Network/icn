use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::federation::Federation;

/// Shared application state across all routes and services
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool
    pub db_pool: PgPool,
    
    /// Federation service (if enabled)
    pub federation: Option<Arc<Federation>>,
}

impl AppState {
    /// Create a new instance of AppState
    pub fn new(db_pool: PgPool, federation: Option<Arc<Federation>>) -> Self {
        Self { db_pool, federation }
    }
    
    /// Get a reference to the database pool
    pub fn db(&self) -> &PgPool {
        &self.db_pool
    }
    
    /// Get a reference to the federation service (if available)
    pub fn federation(&self) -> Option<&Arc<Federation>> {
        self.federation.as_ref()
    }
} 