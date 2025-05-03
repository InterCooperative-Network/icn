use serde::{Serialize, Deserialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::{Duration, interval};
use thiserror::Error;
use sqlx::PgPool;

use crate::storage::{ThreadRepository, CredentialLinkRepository};
use crate::federation::Federation;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Failed to connect to Runtime: {0}")]
    ConnectionFailed(String),
    
    #[error("Storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
    
    #[error("Federation error: {0}")]
    Federation(#[from] crate::federation::FederationError),
    
    #[error("Unexpected error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, RuntimeError>;

/// Events from the ICN Runtime that we care about
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEvent {
    /// A new proposal has been created
    ProposalCreated {
        proposal_cid: String,
        title: String,
        created_by: String,
        timestamp: i64,
    },
    
    /// A credential has been issued
    CredentialIssued {
        credential_cid: String,
        issuer_did: String,
        subject_did: String,
        credential_type: String,
        timestamp: i64,
    },
    
    /// A proposal has been finalized
    ProposalFinalized {
        proposal_cid: String,
        approved: bool,
        timestamp: i64,
    },
}

/// Runtime client for interacting with ICN Runtime
pub struct RuntimeClient {
    /// Database connection pool
    db_pool: PgPool,
    
    /// Federation service (if available)
    federation: Option<Arc<Federation>>,
    
    /// Background task for listening to Runtime events
    listener_task: Option<JoinHandle<()>>,
    
    /// Whether the client is running
    running: bool,
    
    /// Runtime API endpoint
    runtime_endpoint: String,
}

impl RuntimeClient {
    /// Create a new Runtime client
    pub fn new(db_pool: PgPool, federation: Option<Arc<Federation>>) -> Self {
        let runtime_endpoint = std::env::var("RUNTIME_API_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
            
        Self {
            db_pool,
            federation,
            listener_task: None,
            running: false,
            runtime_endpoint,
        }
    }
    
    /// Start the Runtime client
    pub async fn start(&mut self) -> Result<()> {
        if self.running {
            return Ok(());
        }
        
        self.running = true;
        
        // Clone components for use in the task
        let db_pool = self.db_pool.clone();
        let federation = self.federation.clone();
        let runtime_endpoint = self.runtime_endpoint.clone();
        
        // Spawn the background task for listening to Runtime events
        let task = tokio::spawn(async move {
            let thread_repo = ThreadRepository::new(db_pool.clone());
            let link_repo = CredentialLinkRepository::new(db_pool.clone());
            
            let mut interval = interval(Duration::from_secs(10));
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Poll for Runtime events
                        match poll_runtime_events(&runtime_endpoint).await {
                            Ok(events) => {
                                for event in events {
                                    let _ = handle_runtime_event(
                                        &event, 
                                        &thread_repo, 
                                        &link_repo, 
                                        federation.as_ref()
                                    ).await;
                                }
                            },
                            Err(e) => {
                                tracing::error!("Failed to poll Runtime events: {}", e);
                            }
                        }
                    }
                }
            }
        });
        
        self.listener_task = Some(task);
        
        Ok(())
    }
    
    /// Stop the Runtime client
    pub async fn stop(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        
        if let Some(task) = self.listener_task.take() {
            task.abort();
            let _ = task.await;
        }
        
        self.running = false;
        
        Ok(())
    }
}

/// Poll for Runtime events
async fn poll_runtime_events(endpoint: &str) -> Result<Vec<RuntimeEvent>> {
    // For actual implementation, we would use reqwest to call the Runtime API
    // For example:
    // let client = reqwest::Client::new();
    // let response = client.get(&format!("{}/api/events", endpoint))
    //     .send()
    //     .await
    //     .map_err(|e| RuntimeError::ConnectionFailed(e.to_string()))?;
    // 
    // let events: Vec<RuntimeEvent> = response.json()
    //     .await
    //     .map_err(|e| RuntimeError::ConnectionFailed(e.to_string()))?;
    //
    // Ok(events)
    
    // For now, return an empty vector
    Ok(Vec::new())
}

/// Handle a Runtime event
async fn handle_runtime_event(
    event: &RuntimeEvent,
    thread_repo: &ThreadRepository,
    link_repo: &CredentialLinkRepository,
    federation: Option<&Arc<Federation>>,
) -> Result<()> {
    match event {
        RuntimeEvent::ProposalCreated { proposal_cid, title, created_by, timestamp } => {
            // Create a new thread for the proposal
            let thread = thread_repo.create_thread(title, Some(proposal_cid)).await?;
            
            // Announce the thread to the federation
            if let Some(fed) = federation {
                if let Err(e) = fed.announce_thread(&thread.id.to_string()).await {
                    tracing::warn!("Failed to announce thread for proposal: {}", e);
                }
            }
            
            Ok(())
        },
        RuntimeEvent::CredentialIssued { credential_cid, issuer_did, subject_did, credential_type, timestamp } => {
            // For now, we don't automatically link credentials
            // This requires knowing which thread to link the credential to
            Ok(())
        },
        RuntimeEvent::ProposalFinalized { proposal_cid, approved, timestamp } => {
            // Find the thread for this proposal
            match thread_repo.find_thread_by_proposal_cid(proposal_cid).await {
                Ok(thread) => {
                    // Update the thread title to indicate finalization
                    let status = if *approved { "APPROVED" } else { "REJECTED" };
                    let new_title = format!("[{}] {}", status, thread.title);
                    
                    // Update the thread
                    thread_repo.update_thread(thread.id, &new_title).await?;
                    
                    // Log the event
                    tracing::info!("Updated thread for finalized proposal: {}", proposal_cid);
                    
                    // Announce update to federation
                    if let Some(fed) = federation {
                        if let Err(e) = fed.announce_thread(&thread.id.to_string()).await {
                            tracing::warn!("Failed to announce thread update for finalized proposal: {}", e);
                        }
                    }
                },
                Err(e) => {
                    tracing::warn!("Could not find thread for finalized proposal {}: {}", proposal_cid, e);
                }
            }
            
            Ok(())
        }
    }
} 