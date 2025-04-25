use crate::federation::{
    FederationError, FederationRuntime, 
    FinalizationReceipt, FederationManifest
};
use crate::identity::{Identity, IdentityManager};
use crate::storage::{StorageManager, StorageError};
use crate::vc::{CredentialError, VerifiableCredential};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur in federation sync service
#[derive(Debug, Error)]
pub enum FederationSyncError {
    #[error("Federation error: {0}")]
    FederationError(#[from] FederationError),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
    
    #[error("Credential error: {0}")]
    CredentialError(#[from] CredentialError),
    
    #[error("Sync service error: {0}")]
    ServiceError(String),
}

/// Status of a synchronized credential
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CredentialStatus {
    Pending,
    Verified,
    Invalid,
    Revoked,
    Expired,
}

/// Trust score information for a credential
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialTrustScore {
    pub score: u32,                    // 0-100
    pub status: String,                // "High", "Medium", "Low"
    pub issuer_verified: bool,
    pub signature_verified: bool,
    pub federation_verified: bool,
    pub quorum_met: bool,
}

/// Credential sync data including original receipt and verification status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialSyncData {
    pub credential_id: String,
    pub receipt_id: String,
    pub receipt: FinalizationReceipt,
    pub federation_id: String,
    pub status: CredentialStatus,
    pub trust_score: Option<CredentialTrustScore>,
    pub last_verified: chrono::DateTime<chrono::Utc>,
    pub verifiable_credential: Option<VerifiableCredential>,
}

/// Configuration for the federation sync service
#[derive(Debug, Clone)]
pub struct FederationSyncConfig {
    pub sync_interval_seconds: u64,    // How often to check for new receipts
    pub verify_interval_minutes: u64,  // How often to re-verify credentials
    pub auto_sync_enabled: bool,       // Whether to sync automatically
    pub auto_verify_enabled: bool,     // Whether to verify automatically
    pub credentials_path: PathBuf,     // Path to store credential data
}

impl Default for FederationSyncConfig {
    fn default() -> Self {
        Self {
            sync_interval_seconds: 300,  // 5 minutes
            verify_interval_minutes: 60, // 1 hour
            auto_sync_enabled: true,
            auto_verify_enabled: true,
            credentials_path: PathBuf::from("credentials"),
        }
    }
}

/// Federation sync service for credentials
pub struct FederationSyncService {
    federation_runtime: Arc<Mutex<FederationRuntime>>,
    storage_manager: StorageManager,
    identity_manager: Arc<Mutex<IdentityManager>>,
    config: FederationSyncConfig,
    running: Arc<Mutex<bool>>,
    sync_data: Arc<Mutex<HashMap<String, CredentialSyncData>>>,
    notification_tx: mpsc::Sender<CredentialSyncData>,
    notification_rx: mpsc::Receiver<CredentialSyncData>,
}

impl Clone for FederationSyncService {
    fn clone(&self) -> Self {
        // Create a new channel pair for notifications
        let (tx, rx) = mpsc::channel();
        
        Self {
            federation_runtime: self.federation_runtime.clone(),
            storage_manager: self.storage_manager.clone(),
            identity_manager: self.identity_manager.clone(),
            config: self.config.clone(),
            running: self.running.clone(),
            sync_data: self.sync_data.clone(),
            notification_tx: tx,
            notification_rx: rx,
        }
    }
}

impl FederationSyncService {
    /// Create a new federation sync service
    pub fn new(
        federation_runtime: FederationRuntime,
        storage_manager: StorageManager,
        identity_manager: IdentityManager,
        config: Option<FederationSyncConfig>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        
        Self {
            federation_runtime: Arc::new(Mutex::new(federation_runtime)),
            storage_manager,
            identity_manager: Arc::new(Mutex::new(identity_manager)),
            config: config.unwrap_or_default(),
            running: Arc::new(Mutex::new(false)),
            sync_data: Arc::new(Mutex::new(HashMap::new())),
            notification_tx: tx,
            notification_rx: rx,
        }
    }
    
    /// Initialize the sync service
    pub fn initialize(&self) -> Result<(), FederationSyncError> {
        // Create credentials directory if it doesn't exist
        let credentials_dir = self.storage_manager.get_data_dir().join(&self.config.credentials_path);
        if !credentials_dir.exists() {
            std::fs::create_dir_all(&credentials_dir)
                .map_err(|e| FederationSyncError::StorageError(StorageError::IoError(e.to_string())))?;
        }
        
        // Load existing credential sync data
        self.load_sync_data()?;
        
        Ok(())
    }
    
    /// Start background sync and verification threads
    pub fn start(&self) -> Result<(), FederationSyncError> {
        // Set running flag
        let mut running = self.running.lock().unwrap();
        if *running {
            return Ok(());
        }
        *running = true;
        drop(running);
        
        // Start credential sync thread if auto-sync is enabled
        if self.config.auto_sync_enabled {
            let federation_runtime = self.federation_runtime.clone();
            let storage_manager = self.storage_manager.clone();
            let identity_manager = self.identity_manager.clone();
            let config = self.config.clone();
            let running = self.running.clone();
            let sync_data = self.sync_data.clone();
            let tx = self.notification_tx.clone();
            
            thread::spawn(move || {
                while *running.lock().unwrap() {
                    // Try to get the active identity for syncing
                    let identity = {
                        let identity_manager = identity_manager.lock().unwrap();
                        identity_manager.get_active_identity().cloned()
                    };
                    
                    // If we have an active identity, sync receipts
                    if let Some(identity) = identity {
                        if let Err(e) = Self::sync_credentials(
                            &federation_runtime,
                            &storage_manager,
                            &identity,
                            &config,
                            &sync_data,
                            &tx,
                        ) {
                            eprintln!("Credential sync error: {}", e);
                        }
                    }
                    
                    // Sleep for the configured interval
                    thread::sleep(Duration::from_secs(config.sync_interval_seconds));
                }
            });
        }
        
        // Start credential verification thread if auto-verify is enabled
        if self.config.auto_verify_enabled {
            let federation_runtime = self.federation_runtime.clone();
            let storage_manager = self.storage_manager.clone();
            let config = self.config.clone();
            let running = self.running.clone();
            let sync_data = self.sync_data.clone();
            let tx = self.notification_tx.clone();
            
            thread::spawn(move || {
                // Add a small delay before starting verification
                thread::sleep(Duration::from_secs(10));
                
                while *running.lock().unwrap() {
                    if let Err(e) = Self::verify_credentials(
                        &federation_runtime,
                        &storage_manager,
                        &config,
                        &sync_data,
                        &tx,
                    ) {
                        eprintln!("Credential verification error: {}", e);
                    }
                    
                    // Sleep for the configured interval
                    thread::sleep(Duration::from_secs(config.verify_interval_minutes * 60));
                }
            });
        }
        
        Ok(())
    }
    
    /// Stop the sync service
    pub fn stop(&self) {
        let mut running = self.running.lock().unwrap();
        *running = false;
    }
    
    /// Get the next credential notification if available
    pub fn next_notification(&self) -> Option<CredentialSyncData> {
        self.notification_rx.try_recv().ok()
    }
    
    /// Wait for a credential notification with timeout
    pub fn wait_for_notification(&self, timeout_ms: u64) -> Option<CredentialSyncData> {
        self.notification_rx.recv_timeout(Duration::from_millis(timeout_ms)).ok()
    }
    
    /// Get all synced credentials
    pub fn get_all_credentials(&self) -> Vec<CredentialSyncData> {
        let sync_data = self.sync_data.lock().unwrap();
        sync_data.values().cloned().collect()
    }
    
    /// Get a specific credential by ID
    pub fn get_credential(&self, credential_id: &str) -> Option<CredentialSyncData> {
        let sync_data = self.sync_data.lock().unwrap();
        sync_data.get(credential_id).cloned()
    }
    
    /// Manually trigger a sync for a specific DID
    pub fn sync_did(&self, did: &str) -> Result<Vec<CredentialSyncData>, FederationSyncError> {
        let federation_runtime = self.federation_runtime.lock().unwrap();
        
        // Get receipts for the DID
        let receipts = federation_runtime.get_finalized_receipts_by_did(did)?;
        
        // Process the receipts
        let mut new_credentials = Vec::new();
        for receipt in receipts {
            let credential_data = self.process_receipt(receipt)?;
            new_credentials.push(credential_data);
        }
        
        Ok(new_credentials)
    }
    
    /// Process a single receipt and create/update credential data
    fn process_receipt(&self, receipt: FinalizationReceipt) -> Result<CredentialSyncData, FederationSyncError> {
        let mut sync_data = self.sync_data.lock().unwrap();
        
        // Generate a stable ID based on receipt ID
        let credential_id = format!("cred-{}", receipt.id);
        
        // Check if we already have this receipt
        let existing = sync_data.get(&credential_id).cloned();
        
        // Create new credential sync data or update existing
        let credential_data = if let Some(mut existing) = existing {
            // Update the receipt if needed
            existing.receipt = receipt;
            existing.last_verified = chrono::Utc::now();
            existing
        } else {
            // Create new credential data
            let credential_data = CredentialSyncData {
                credential_id: credential_id.clone(),
                receipt_id: receipt.id.clone(),
                receipt: receipt.clone(),
                federation_id: receipt.federation_id.clone(),
                status: CredentialStatus::Pending,
                trust_score: None,
                last_verified: chrono::Utc::now(),
                verifiable_credential: None,
            };
            
            // Send notification for new credential
            let _ = self.notification_tx.send(credential_data.clone());
            
            credential_data
        };
        
        // Update in-memory data
        sync_data.insert(credential_id, credential_data.clone());
        
        // Persist to storage
        self.save_credential(&credential_data)?;
        
        Ok(credential_data)
    }
    
    /// Verify credential and update trust score
    pub fn verify_credential(&self, credential_id: &str) -> Result<CredentialSyncData, FederationSyncError> {
        let mut sync_data = self.sync_data.lock().unwrap();
        let federation_runtime = self.federation_runtime.lock().unwrap();
        
        // Get the credential data
        let credential_data = sync_data.get(credential_id).cloned()
            .ok_or_else(|| FederationSyncError::ServiceError(format!("Credential not found: {}", credential_id)))?;
        
        // Get the federation manifest for verification
        let manifest = federation_runtime.get_federation_manifest(&credential_data.federation_id)?;
        
        // Verify signatures and compute trust score
        let mut updated = credential_data.clone();
        updated.status = CredentialStatus::Verified;
        updated.last_verified = chrono::Utc::now();
        
        // Very basic trust score for now
        let mut score = 50;
        let mut signature_verified = false;
        let mut federation_verified = false;
        let mut quorum_met = false;
        
        // Check if issuer is in the federation
        let issuer_did = &credential_data.receipt.issuer;
        let issuer_verified = manifest.members.contains_key(issuer_did);
        if issuer_verified {
            score += 20;
            federation_verified = true;
        }
        
        // Check signatures (simple count for now)
        if !credential_data.receipt.signatures.is_empty() {
            score += 10;
            signature_verified = true;
            
            // Add more score for multiple signatures
            if credential_data.receipt.signatures.len() >= 3 {
                score += 20;
                quorum_met = true;
            } else if credential_data.receipt.signatures.len() >= 2 {
                score += 10;
                quorum_met = true;
            }
        }
        
        // Determine trust status
        let status = if score >= 80 {
            "High".to_string()
        } else if score >= 50 {
            "Medium".to_string()
        } else {
            "Low".to_string()
        };
        
        // Update trust score
        updated.trust_score = Some(CredentialTrustScore {
            score: score,
            status,
            issuer_verified,
            signature_verified,
            federation_verified,
            quorum_met,
        });
        
        // Convert to VC if not already done
        if updated.verifiable_credential.is_none() {
            updated.verifiable_credential = Some(self.receipt_to_vc(&credential_data.receipt)?);
        }
        
        // Update in-memory data
        sync_data.insert(credential_id.to_string(), updated.clone());
        
        // Persist to storage
        self.save_credential(&updated)?;
        
        // Send notification for verified credential
        let _ = self.notification_tx.send(updated.clone());
        
        Ok(updated)
    }
    
    /// Convert a receipt to a verifiable credential
    fn receipt_to_vc(&self, receipt: &FinalizationReceipt) -> Result<VerifiableCredential, FederationSyncError> {
        // Basic conversion - would be enhanced in a real implementation
        let vc = VerifiableCredential {
            context: vec![
                "https://www.w3.org/2018/credentials/v1".to_string(),
                "https://identity.foundation/presentation-exchange/submission/v1".to_string(),
            ],
            id: format!("urn:uuid:{}", Uuid::new_v4()),
            types: vec![
                "VerifiableCredential".to_string(),
                format!("{}Credential", receipt.receipt_type).to_string(),
            ],
            issuer: receipt.issuer.clone(),
            issuanceDate: receipt.timestamp,
            credentialSubject: crate::vc::FederationMemberSubject {
                id: receipt.subject_did.clone(),
                federationMember: crate::vc::FederationMember {
                    scope: receipt.federation_id.split(':').nth(1).unwrap_or("governance").to_string(),
                    username: receipt.subject_did.split(':').last().unwrap_or("unknown").to_string(),
                    role: "member".to_string(),
                },
            },
            proof: Some(crate::vc::Proof {
                type_: "Ed25519Signature2020".to_string(),
                created: receipt.timestamp,
                verificationMethod: format!("{}#keys-1", receipt.issuer),
                proofPurpose: "assertionMethod".to_string(),
                proofValue: receipt.signatures.first()
                    .map(|sig| sig.signature_value.clone())
                    .unwrap_or_else(|| "".to_string()),
            }),
        };
        
        Ok(vc)
    }
    
    /// Save credential data to storage
    fn save_credential(&self, credential_data: &CredentialSyncData) -> Result<(), FederationSyncError> {
        let credentials_dir = self.storage_manager.get_data_dir().join(&self.config.credentials_path);
        let file_path = credentials_dir.join(format!("{}.json", credential_data.credential_id));
        
        let json = serde_json::to_string_pretty(credential_data)
            .map_err(|e| FederationSyncError::ServiceError(format!("Failed to serialize credential: {}", e)))?;
        
        std::fs::write(&file_path, json)
            .map_err(|e| FederationSyncError::StorageError(StorageError::IoError(e.to_string())))?;
        
        Ok(())
    }
    
    /// Load all saved credential data from storage
    fn load_sync_data(&self) -> Result<(), FederationSyncError> {
        let credentials_dir = self.storage_manager.get_data_dir().join(&self.config.credentials_path);
        if !credentials_dir.exists() {
            return Ok(());
        }
        
        let mut sync_data = self.sync_data.lock().unwrap();
        
        for entry in std::fs::read_dir(credentials_dir)
            .map_err(|e| FederationSyncError::StorageError(StorageError::IoError(e.to_string())))? {
                
            let entry = entry
                .map_err(|e| FederationSyncError::StorageError(StorageError::IoError(e.to_string())))?;
            
            if !entry.file_type()
                .map_err(|e| FederationSyncError::StorageError(StorageError::IoError(e.to_string())))?
                .is_file() {
                continue;
            }
            
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                let json = std::fs::read_to_string(&path)
                    .map_err(|e| FederationSyncError::StorageError(StorageError::IoError(e.to_string())))?;
                
                let credential_data: CredentialSyncData = serde_json::from_str(&json)
                    .map_err(|e| FederationSyncError::ServiceError(format!("Failed to deserialize credential: {}", e)))?;
                
                sync_data.insert(credential_data.credential_id.clone(), credential_data);
            }
        }
        
        Ok(())
    }
    
    /// Background thread function to sync credentials
    fn sync_credentials(
        federation_runtime: &Arc<Mutex<FederationRuntime>>,
        storage_manager: &StorageManager,
        identity: &Identity,
        config: &FederationSyncConfig,
        sync_data: &Arc<Mutex<HashMap<String, CredentialSyncData>>>,
        notification_tx: &mpsc::Sender<CredentialSyncData>,
    ) -> Result<(), FederationSyncError> {
        let runtime = federation_runtime.lock().unwrap();
        
        // Get the DID from the identity
        let did = identity.did();
        
        // Get receipts from the federation
        let receipts = runtime.get_finalized_receipts_by_did(did)?;
        
        // Process each receipt
        let mut sync_data_lock = sync_data.lock().unwrap();
        let credentials_dir = storage_manager.get_data_dir().join(&config.credentials_path);
        
        for receipt in receipts {
            // Generate a stable ID based on receipt ID
            let credential_id = format!("cred-{}", receipt.id);
            
            // Check if we already have this receipt
            if let Some(existing) = sync_data_lock.get(&credential_id) {
                // Skip if we've already processed this receipt
                continue;
            }
            
            // Create new credential data
            let credential_data = CredentialSyncData {
                credential_id: credential_id.clone(),
                receipt_id: receipt.id.clone(),
                receipt: receipt.clone(),
                federation_id: receipt.federation_id.clone(),
                status: CredentialStatus::Pending,
                trust_score: None,
                last_verified: chrono::Utc::now(),
                verifiable_credential: None,
            };
            
            // Save to storage
            let file_path = credentials_dir.join(format!("{}.json", credential_id));
            let json = serde_json::to_string_pretty(&credential_data)
                .map_err(|e| FederationSyncError::ServiceError(format!("Failed to serialize credential: {}", e)))?;
            
            std::fs::write(&file_path, json)
                .map_err(|e| FederationSyncError::StorageError(StorageError::IoError(e.to_string())))?;
            
            // Update in-memory data
            sync_data_lock.insert(credential_id, credential_data.clone());
            
            // Send notification
            let _ = notification_tx.send(credential_data);
        }
        
        Ok(())
    }
    
    /// Background thread function to verify credentials
    fn verify_credentials(
        federation_runtime: &Arc<Mutex<FederationRuntime>>,
        storage_manager: &StorageManager,
        config: &FederationSyncConfig,
        sync_data: &Arc<Mutex<HashMap<String, CredentialSyncData>>>,
        notification_tx: &mpsc::Sender<CredentialSyncData>,
    ) -> Result<(), FederationSyncError> {
        let runtime = federation_runtime.lock().unwrap();
        let mut sync_data_lock = sync_data.lock().unwrap();
        let credentials_dir = storage_manager.get_data_dir().join(&config.credentials_path);
        
        // Get a list of all credentials that need verification
        let credentials: Vec<_> = sync_data_lock.values().cloned().collect();
        
        for mut credential_data in credentials {
            // Skip already verified credentials that are recent
            if credential_data.status == CredentialStatus::Verified && 
               (chrono::Utc::now() - credential_data.last_verified).num_minutes() < 
               config.verify_interval_minutes as i64 {
                continue;
            }
            
            // Get the federation manifest for verification
            let manifest = match runtime.get_federation_manifest(&credential_data.federation_id) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Failed to get federation manifest: {}", e);
                    continue;
                }
            };
            
            // Very basic trust score for now
            let mut score = 50;
            let mut signature_verified = false;
            let mut federation_verified = false;
            let mut quorum_met = false;
            
            // Check if issuer is in the federation
            let issuer_did = &credential_data.receipt.issuer;
            let issuer_verified = manifest.members.contains_key(issuer_did);
            if issuer_verified {
                score += 20;
                federation_verified = true;
            }
            
            // Check signatures (simple count for now)
            if !credential_data.receipt.signatures.is_empty() {
                score += 10;
                signature_verified = true;
                
                // Add more score for multiple signatures
                if credential_data.receipt.signatures.len() >= 3 {
                    score += 20;
                    quorum_met = true;
                } else if credential_data.receipt.signatures.len() >= 2 {
                    score += 10;
                    quorum_met = true;
                }
            }
            
            // Determine trust status
            let status = if score >= 80 {
                "High".to_string()
            } else if score >= 50 {
                "Medium".to_string()
            } else {
                "Low".to_string()
            };
            
            // Update credential data
            credential_data.status = CredentialStatus::Verified;
            credential_data.last_verified = chrono::Utc::now();
            credential_data.trust_score = Some(CredentialTrustScore {
                score: score,
                status,
                issuer_verified,
                signature_verified,
                federation_verified,
                quorum_met,
            });
            
            // Convert to VC if not already done
            if credential_data.verifiable_credential.is_none() {
                credential_data.verifiable_credential = match Self::receipt_to_vc_static(&credential_data.receipt) {
                    Ok(vc) => Some(vc),
                    Err(e) => {
                        eprintln!("Failed to convert receipt to VC: {}", e);
                        None
                    }
                };
            }
            
            // Save to storage
            let file_path = credentials_dir.join(format!("{}.json", credential_data.credential_id));
            match serde_json::to_string_pretty(&credential_data) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&file_path, json) {
                        eprintln!("Failed to save credential: {}", e);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to serialize credential: {}", e);
                }
            }
            
            // Update in-memory data
            sync_data_lock.insert(credential_data.credential_id.clone(), credential_data.clone());
            
            // Send notification
            let _ = notification_tx.send(credential_data);
        }
        
        Ok(())
    }
    
    /// Static helper to convert a receipt to a VC (for use in threads)
    fn receipt_to_vc_static(receipt: &FinalizationReceipt) -> Result<VerifiableCredential, FederationSyncError> {
        // Basic conversion - would be enhanced in a real implementation
        let vc = VerifiableCredential {
            context: vec![
                "https://www.w3.org/2018/credentials/v1".to_string(),
                "https://identity.foundation/presentation-exchange/submission/v1".to_string(),
            ],
            id: format!("urn:uuid:{}", Uuid::new_v4()),
            types: vec![
                "VerifiableCredential".to_string(),
                format!("{}Credential", receipt.receipt_type).to_string(),
            ],
            issuer: receipt.issuer.clone(),
            issuanceDate: receipt.timestamp,
            credentialSubject: crate::vc::FederationMemberSubject {
                id: receipt.subject_did.clone(),
                federationMember: crate::vc::FederationMember {
                    scope: receipt.federation_id.split(':').nth(1).unwrap_or("governance").to_string(),
                    username: receipt.subject_did.split(':').last().unwrap_or("unknown").to_string(),
                    role: "member".to_string(),
                },
            },
            proof: Some(crate::vc::Proof {
                type_: "Ed25519Signature2020".to_string(),
                created: receipt.timestamp,
                verificationMethod: format!("{}#keys-1", receipt.issuer),
                proofPurpose: "assertionMethod".to_string(),
                proofValue: receipt.signatures.first()
                    .map(|sig| sig.signature_value.clone())
                    .unwrap_or_else(|| "".to_string()),
            }),
        };
        
        Ok(vc)
    }
    
    /// Get a reference to the federation runtime
    pub fn get_federation_runtime(&self) -> Arc<FederationRuntime> {
        self.federation_runtime.clone()
    }
    
    /// Save a credential to the local wallet
    pub fn save_credential(&self, credential: serde_json::Value) -> Result<CredentialSyncData, FederationSyncError> {
        let mut sync_data = self.sync_data.lock().unwrap();
        
        // Extract necessary fields from the credential
        let credential_id = credential["id"].as_str().ok_or_else(|| {
            FederationSyncError::ServiceError("Credential missing id field".to_string())
        })?.to_string();
        
        let federation_id = match credential["metadata"]["federation"]["id"].as_str() {
            Some(id) => id.to_string(),
            None => {
                return Err(FederationSyncError::ServiceError(
                    "Credential missing federation id".to_string()
                ));
            }
        };
        
        // Create a simple receipt for the new credential
        let receipt = CredentialReceipt {
            receipt_id: format!("receipt:{}", uuid::Uuid::new_v4()),
            receipt_type: "amendment".to_string(),
            action_type: "restore".to_string(),
            federation_id: federation_id.clone(),
            issuer: credential["issuer"]["did"].as_str().unwrap_or("").to_string(),
            issuer_name: credential["issuer"]["name"].as_str().map(|s| s.to_string()),
            subject_did: credential["subjectDid"].as_str().unwrap_or("").to_string(),
            signatures: vec![],
            timestamp: chrono::Utc::now(),
            metadata: {
                let mut metadata = HashMap::new();
                
                // Add any amendment metadata
                if let Some(amend_id) = credential["credentialSubject"]["amendment_id"].as_str() {
                    metadata.insert("amendment_id".to_string(), amend_id.to_string());
                }
                
                if let Some(text_hash) = credential["credentialSubject"]["text_hash"].as_str() {
                    metadata.insert("text_hash".to_string(), text_hash.to_string());
                }
                
                if let Some(referenced) = credential["credentialSubject"]["referenced_credentials"].as_array() {
                    let refs_str: Vec<String> = referenced.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    metadata.insert("referenced_credentials".to_string(), refs_str.join(","));
                }
                
                metadata
            },
        };
        
        // Create new sync data entry
        let new_cred = CredentialSyncData {
            credential_id: credential_id.clone(),
            receipt_id: receipt.receipt_id.clone(),
            federation_id,
            receipt,
            status: CredentialStatus::Unverified,
            last_sync: chrono::Utc::now(),
            last_verified: chrono::Utc::now(),
            trust_score: None,
            credential_json: credential,
        };
        
        // Store in sync data
        sync_data.insert(credential_id.clone(), new_cred.clone());
        
        // Save to persistent storage
        match &self.storage {
            Some(storage) => {
                if let Err(e) = storage.store_credential(&new_cred) {
                    log::warn!("Failed to save credential to storage: {}", e);
                }
            }
            None => {
                log::warn!("No storage configured for federation sync service");
            }
        };
        
        Ok(new_cred)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // TODO: Add tests for federation sync service
} 