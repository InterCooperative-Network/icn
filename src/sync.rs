use crate::api::{ApiClient, ApiError};
use crate::federation::{FederationError, FederationRuntime, MonitoringStatus};
use crate::identity::{Identity, IdentityManager};
use crate::proposal::{Proposal, ProposalError};
use crate::storage::{StorageManager, StorageError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;
use thiserror::Error;

/// Errors that can occur in sync operations
#[derive(Debug, Error)]
pub enum SyncError {
    #[error("API error: {0}")]
    ApiError(#[from] ApiError),
    
    #[error("Federation error: {0}")]
    FederationError(#[from] FederationError),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
    
    #[error("Proposal error: {0}")]
    ProposalError(#[from] ProposalError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Sync error: {0}")]
    SyncError(String),
}

/// Types of notifications that can be sent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationType {
    ProposalPassed(String),
    ProposalRejected(String),
    GuardianVoteRequest(String),
    RecoveryRequest(String),
    NewProposal(String),
    DagSyncComplete,
}

/// A notification message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub notification_type: NotificationType,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub id: String,
    pub read: bool,
}

/// A message to be synchronized with the federation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    pub id: String,
    pub sender: Option<String>,
    pub recipient: Option<String>,
    pub message_type: String,
    pub content: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub signature: Option<String>,
    pub status: String,
}

/// Type of sync operations
#[derive(Debug, Clone, Copy)]
pub enum SyncOperation {
    InboxSync,
    OutboxSync,
    DagWatch,
}

/// Result of a sync operation
#[derive(Debug, Clone)]
pub struct SyncResult {
    pub operation: SyncOperation,
    pub success: bool,
    pub message: String,
    pub notifications: Vec<Notification>,
}

/// Configuration for the sync manager
#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub inbox_sync_interval: u64, // seconds
    pub outbox_sync_interval: u64, // seconds
    pub dag_watch_interval: u64, // seconds
    pub inbox_path: PathBuf,
    pub outbox_path: PathBuf,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            inbox_sync_interval: 60, // 1 minute
            outbox_sync_interval: 60, // 1 minute
            dag_watch_interval: 30, // 30 seconds
            inbox_path: PathBuf::from("proposals/inbox"),
            outbox_path: PathBuf::from("proposals/outbox"),
        }
    }
}

/// Manager for synchronizing with the federation
pub struct SyncManager {
    federation_runtime: Arc<Mutex<FederationRuntime>>,
    storage_manager: StorageManager,
    identity_manager: Arc<Mutex<IdentityManager>>,
    config: SyncConfig,
    running: Arc<Mutex<bool>>,
    notification_tx: mpsc::Sender<Notification>,
    notification_rx: mpsc::Receiver<Notification>,
}

impl Clone for SyncManager {
    fn clone(&self) -> Self {
        // Create a new channel pair for notifications
        let (tx, rx) = mpsc::channel();
        
        Self {
            federation_runtime: self.federation_runtime.clone(),
            storage_manager: self.storage_manager.clone(),
            identity_manager: self.identity_manager.clone(),
            config: self.config.clone(),
            running: self.running.clone(),
            notification_tx: tx,
            notification_rx: rx,
        }
    }
}

impl SyncManager {
    /// Create a new sync manager
    pub fn new(
        federation_runtime: FederationRuntime,
        storage_manager: StorageManager,
        identity_manager: IdentityManager,
        config: Option<SyncConfig>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        
        Self {
            federation_runtime: Arc::new(Mutex::new(federation_runtime)),
            storage_manager,
            identity_manager: Arc::new(Mutex::new(identity_manager)),
            config: config.unwrap_or_default(),
            running: Arc::new(Mutex::new(false)),
            notification_tx: tx,
            notification_rx: rx,
        }
    }
    
    /// Start background sync threads
    pub fn start(&self) -> Result<(), SyncError> {
        // Set running flag
        let mut running = self.running.lock().unwrap();
        if *running {
            return Ok(());
        }
        *running = true;
        drop(running);
        
        // Create the inbox and outbox directories if they don't exist
        let inbox_dir = self.storage_manager.get_data_dir().join(&self.config.inbox_path);
        let outbox_dir = self.storage_manager.get_data_dir().join(&self.config.outbox_path);
        
        if !inbox_dir.exists() {
            fs::create_dir_all(&inbox_dir)?;
        }
        
        if !outbox_dir.exists() {
            fs::create_dir_all(&outbox_dir)?;
        }
        
        // Start inbox sync thread
        let federation_runtime = self.federation_runtime.clone();
        let storage_manager = self.storage_manager.clone();
        let identity_manager = self.identity_manager.clone();
        let config = self.config.clone();
        let running = self.running.clone();
        let tx = self.notification_tx.clone();
        
        thread::spawn(move || {
            while *running.lock().unwrap() {
                // Sync inbox
                if let Err(e) = Self::sync_inbox(
                    &federation_runtime,
                    &storage_manager,
                    &identity_manager,
                    &config,
                    &tx,
                ) {
                    eprintln!("Inbox sync error: {}", e);
                }
                
                // Sleep for configured interval
                thread::sleep(Duration::from_secs(config.inbox_sync_interval));
            }
        });
        
        // Start outbox sync thread
        let federation_runtime = self.federation_runtime.clone();
        let storage_manager = self.storage_manager.clone();
        let identity_manager = self.identity_manager.clone();
        let config = self.config.clone();
        let running = self.running.clone();
        let tx = self.notification_tx.clone();
        
        thread::spawn(move || {
            while *running.lock().unwrap() {
                // Sync outbox
                if let Err(e) = Self::sync_outbox(
                    &federation_runtime,
                    &storage_manager,
                    &identity_manager,
                    &config,
                    &tx,
                ) {
                    eprintln!("Outbox sync error: {}", e);
                }
                
                // Sleep for configured interval
                thread::sleep(Duration::from_secs(config.outbox_sync_interval));
            }
        });
        
        // Start DAG watch thread
        let federation_runtime = self.federation_runtime.clone();
        let storage_manager = self.storage_manager.clone();
        let identity_manager = self.identity_manager.clone();
        let config = self.config.clone();
        let running = self.running.clone();
        let tx = self.notification_tx.clone();
        
        thread::spawn(move || {
            while *running.lock().unwrap() {
                // Watch DAG
                if let Err(e) = Self::watch_dag(
                    &federation_runtime,
                    &storage_manager,
                    &identity_manager,
                    &config,
                    &tx,
                ) {
                    eprintln!("DAG watch error: {}", e);
                }
                
                // Sleep for configured interval
                thread::sleep(Duration::from_secs(config.dag_watch_interval));
            }
        });
        
        Ok(())
    }
    
    /// Stop background sync threads
    pub fn stop(&self) {
        let mut running = self.running.lock().unwrap();
        *running = false;
    }
    
    /// Get the next notification
    pub fn next_notification(&self) -> Option<Notification> {
        self.notification_rx.try_recv().ok()
    }
    
    /// Wait for the next notification with timeout
    pub fn wait_for_notification(&self, timeout_ms: u64) -> Option<Notification> {
        match self.notification_rx.recv_timeout(Duration::from_millis(timeout_ms)) {
            Ok(notification) => Some(notification),
            Err(_) => None,
        }
    }
    
    /// Sync inbox with federation
    fn sync_inbox(
        federation_runtime: &Arc<Mutex<FederationRuntime>>,
        storage_manager: &StorageManager,
        identity_manager: &Arc<Mutex<IdentityManager>>,
        config: &SyncConfig,
        notification_tx: &mpsc::Sender<Notification>,
    ) -> Result<SyncResult, SyncError> {
        let inbox_dir = storage_manager.get_data_dir().join(&config.inbox_path);
        let federation = federation_runtime.lock().unwrap();
        
        // Get active proposals from the federation
        let proposals = federation.list_active_proposals()?;
        
        // Get the active identity
        let identity_manager = identity_manager.lock().unwrap();
        let active_identity = identity_manager.get_active_identity();
        
        let mut notifications = Vec::new();
        
        if let Some(identity) = active_identity {
            // Check if there are new proposals that require a vote
            for proposal in &proposals {
                // Check if this proposal is relevant to the current identity
                if proposal.scope == identity.scope() {
                    // Check if we already have it in the inbox
                    let proposal_path = inbox_dir.join(format!("{}.json", proposal.hash));
                    
                    if !proposal_path.exists() {
                        // This is a new proposal, add it to the inbox
                        let proposal_json = serde_json::to_string_pretty(proposal)
                            .map_err(|e| SyncError::SyncError(format!("Failed to serialize proposal: {}", e)))?;
                        
                        fs::write(&proposal_path, proposal_json)?;
                        
                        // Create notification
                        let notification = Notification {
                            notification_type: NotificationType::NewProposal(proposal.hash.clone()),
                            message: format!("New proposal: {}", proposal.title),
                            timestamp: chrono::Utc::now(),
                            id: uuid::Uuid::new_v4().to_string(),
                            read: false,
                        };
                        
                        notifications.push(notification.clone());
                        
                        // Send notification
                        let _ = notification_tx.send(notification);
                    }
                }
            }
        }
        
        // Return the result
        Ok(SyncResult {
            operation: SyncOperation::InboxSync,
            success: true,
            message: format!("Synced inbox with {} notifications", notifications.len()),
            notifications,
        })
    }
    
    /// Sync outbox with federation
    fn sync_outbox(
        federation_runtime: &Arc<Mutex<FederationRuntime>>,
        storage_manager: &StorageManager,
        identity_manager: &Arc<Mutex<IdentityManager>>,
        config: &SyncConfig,
        notification_tx: &mpsc::Sender<Notification>,
    ) -> Result<SyncResult, SyncError> {
        let outbox_dir = storage_manager.get_data_dir().join(&config.outbox_path);
        let federation = federation_runtime.lock().unwrap();
        
        // List all files in the outbox
        let entries = fs::read_dir(&outbox_dir)?;
        
        let mut notifications = Vec::new();
        
        // Check each outbox item
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            // Only process JSON files
            if path.extension().map_or(false, |ext| ext == "json") {
                // Read the outbox item
                let content = fs::read_to_string(&path)?;
                
                // Try to parse as a proposal
                if let Ok(proposal) = serde_json::from_str::<Proposal>(&content) {
                    // Check the proposal status with the federation
                    let audit = federation.audit_proposal(&proposal.hash)?;
                    
                    // If the proposal has been executed or rejected, move it from outbox
                    if audit.execution_status == "executed" || audit.status == "rejected" {
                        // Move to archive directory
                        let archive_dir = storage_manager.get_data_dir().join("proposals").join("archive");
                        
                        if !archive_dir.exists() {
                            fs::create_dir_all(&archive_dir)?;
                        }
                        
                        let dest_path = archive_dir.join(path.file_name().unwrap());
                        fs::rename(&path, &dest_path)?;
                        
                        // Create notification
                        let notification_type = if audit.execution_status == "executed" {
                            NotificationType::ProposalPassed(proposal.hash.clone())
                        } else {
                            NotificationType::ProposalRejected(proposal.hash.clone())
                        };
                        
                        let notification = Notification {
                            notification_type,
                            message: format!("Proposal status update: {}", audit.status),
                            timestamp: chrono::Utc::now(),
                            id: uuid::Uuid::new_v4().to_string(),
                            read: false,
                        };
                        
                        notifications.push(notification.clone());
                        
                        // Send notification
                        let _ = notification_tx.send(notification);
                    }
                }
            }
        }
        
        // Return the result
        Ok(SyncResult {
            operation: SyncOperation::OutboxSync,
            success: true,
            message: format!("Synced outbox with {} updates", notifications.len()),
            notifications,
        })
    }
    
    /// Watch DAG for updates
    fn watch_dag(
        federation_runtime: &Arc<Mutex<FederationRuntime>>,
        storage_manager: &StorageManager,
        identity_manager: &Arc<Mutex<IdentityManager>>,
        config: &SyncConfig,
        notification_tx: &mpsc::Sender<Notification>,
    ) -> Result<SyncResult, SyncError> {
        let federation = federation_runtime.lock().unwrap();
        
        // Get DAG status
        let status = federation.get_dag_status(None)?;
        
        // Store the last known status to detect changes
        let status_path = storage_manager.get_data_dir().join("dag_status.json");
        
        let mut notifications = Vec::new();
        
        // Check if DAG status has changed
        if status_path.exists() {
            let last_status_json = fs::read_to_string(&status_path)?;
            let last_status = serde_json::from_str(&last_status_json)
                .map_err(|e| SyncError::SyncError(format!("Failed to parse DAG status: {}", e)))?;
            
            // Compare with current status
            if status.latest_vertex != last_status.latest_vertex {
                // DAG has been updated
                // Create notification
                let notification = Notification {
                    notification_type: NotificationType::DagSyncComplete,
                    message: format!("DAG updated to vertex {}", status.latest_vertex),
                    timestamp: chrono::Utc::now(),
                    id: uuid::Uuid::new_v4().to_string(),
                    read: false,
                };
                
                notifications.push(notification.clone());
                
                // Send notification
                let _ = notification_tx.send(notification);
            }
        }
        
        // Save current status
        let status_json = serde_json::to_string_pretty(&status)
            .map_err(|e| SyncError::SyncError(format!("Failed to serialize DAG status: {}", e)))?;
        
        fs::write(&status_path, status_json)?;
        
        // Return the result
        Ok(SyncResult {
            operation: SyncOperation::DagWatch,
            success: true,
            message: format!("Watched DAG with {} notifications", notifications.len()),
            notifications,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // TODO: Add tests for sync functionality
} 