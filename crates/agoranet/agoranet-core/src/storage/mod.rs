// Storage module for AgoraNet
// Will handle database connections and persistence

mod db;
mod threads;
mod credentials;
mod messages;
mod reactions;

pub use db::create_db_pool;
pub use threads::ThreadRepository;
pub use credentials::CredentialLinkRepository;
pub use messages::MessageRepository;
pub use reactions::ReactionRepository;

use sqlx::postgres::PgPool;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Item not found")]
    NotFound,
    
    #[error("Unexpected error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;

pub struct Storage {
    pool: PgPool,
}

impl Storage {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url)
            .await
            .map_err(|e| StorageError::Database(e))?;
            
        Ok(Self { pool })
    }
    
    // Thread storage operations would go here
    
    // Credential link storage operations would go here
} 