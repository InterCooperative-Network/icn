pub mod credential_linking;
pub mod thread;

use axum::Router;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::database::Database;

pub fn routes() -> Router<Arc<Mutex<Database>>> {
    Router::new()
        .merge(thread::routes())
        .merge(credential_linking::routes())
} 