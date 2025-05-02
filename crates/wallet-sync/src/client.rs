use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs::{self, File};
use std::io::{Read, Write};
use tokio::sync::Mutex;
use wallet_core::identity::IdentityWallet;
use crate::error::{SyncResult, SyncError};
use crate::trust::TrustBundleValidator;
use crate::dag::{DagObject, DagVerifier};
use cid::Cid;
use wallet_agent::governance::TrustBundle;
use reqwest::Client as HttpClient;

const DEFAULT_SYNC_SERVERS: [&str; 2] = [
    "https://icn-federation.example.com/api",
    "https://backup-icn.example.org/api",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub servers: Vec<String>,
    pub local_storage: PathBuf,
    pub sync_frequency_seconds: u64,
    pub auto_sync: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            servers: DEFAULT_SYNC_SERVERS.iter().map(|s| s.to_string()).collect(),
            local_storage: PathBuf::from("./storage/sync"),
            sync_frequency_seconds: 3600, // 1 hour
            auto_sync: true,
        }
    }
}

pub struct SyncClient {
    config: SyncConfig,
    #[allow(dead_code)]
    identity: IdentityWallet,
    http_client: HttpClient,
    trust_validator: TrustBundleValidator,
    dag_verifier: DagVerifier,
    cached_bundles: Mutex<HashMap<String, TrustBundle>>,
}

impl SyncClient {
    pub fn new(identity: IdentityWallet, config: Option<SyncConfig>) -> SyncResult<Self> {
        let config = config.unwrap_or_default();
        
        // Ensure storage directory exists
        fs::create_dir_all(&config.local_storage)
            .map_err(|e| SyncError::IoError(format!("Failed to create storage directory: {}", e)))?;
            
        let trust_validator = TrustBundleValidator::new(identity.clone());
        let dag_verifier = DagVerifier::new();
        
        Ok(Self {
            config,
            identity,
            http_client: HttpClient::new(),
            trust_validator,
            dag_verifier,
            cached_bundles: Mutex::new(HashMap::new()),
        })
    }
    
    pub async fn sync_trust_bundles(&self) -> SyncResult<Vec<TrustBundle>> {
        let mut results = Vec::new();
        
        for server_url in &self.config.servers {
            match self.fetch_trust_bundles_from_server(server_url).await {
                Ok(bundles) => {
                    for bundle in bundles {
                        if self.trust_validator.validate_bundle(&bundle)? {
                            // Cache the validated bundle
                            let mut cached = self.cached_bundles.lock().await;
                            cached.insert(bundle.id.clone(), bundle.clone());
                            
                            // Save to disk
                            self.save_bundle_to_disk(&bundle)?;
                            
                            results.push(bundle);
                        }
                    }
                }
                Err(e) => {
                    // Log the error but continue with other servers
                    eprintln!("Failed to sync from {}: {}", server_url, e);
                    continue;
                }
            }
        }
        
        Ok(results)
    }
    
    pub async fn fetch_dag_object(&self, cid: &str) -> SyncResult<DagObject> {
        // We don't need to keep the Cid object, just validate it's correct
        Cid::try_from(cid)
            .map_err(|e| SyncError::CidError(format!("Invalid CID format: {}", e)))?;
            
        // Check if we have it cached locally
        let local_path = self.get_dag_object_path(cid);
        if local_path.exists() {
            return self.load_dag_object_from_disk(cid);
        }
        
        // Try fetching from remote servers
        for server_url in &self.config.servers {
            let url = format!("{}/dag/{}", server_url, cid);
            
            match self.http_client.get(&url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let bytes = response.bytes().await
                            .map_err(|e| SyncError::HttpError(format!("Failed to read response: {}", e)))?;
                            
                        let obj: DagObject = serde_json::from_slice(&bytes)
                            .map_err(|e| SyncError::SerializationError(format!("Invalid DAG object: {}", e)))?;
                            
                        // Verify the object
                        if self.dag_verifier.verify_object(&obj, cid)? {
                            // Save to disk
                            self.save_dag_object_to_disk(&obj, cid)?;
                            
                            return Ok(obj);
                        } else {
                            return Err(SyncError::VerificationError(format!("DAG object verification failed: {}", cid)));
                        }
                    }
                }
                Err(_) => {
                    // Try next server
                    continue;
                }
            }
        }
        
        Err(SyncError::ConnectionError(format!("Failed to fetch DAG object: {}", cid)))
    }
    
    pub async fn verify_guardian_mandate(&self, did: &str) -> SyncResult<bool> {
        // In a real implementation, this would fetch the Guardian Mandate proof
        // from the DAG and verify it cryptographically
        
        // For this example, we'll just verify the DID is in an active trust bundle
        let cached = self.cached_bundles.lock().await;
        
        for bundle in cached.values() {
            if bundle.active && bundle.guardians.contains(&did.to_string()) {
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    // Helper methods
    async fn fetch_trust_bundles_from_server(&self, server_url: &str) -> SyncResult<Vec<TrustBundle>> {
        let url = format!("{}/trust-bundles", server_url);
        
        let response = self.http_client.get(&url).send().await
            .map_err(|e| SyncError::ConnectionError(format!("Failed to connect to server: {}", e)))?;
            
        if !response.status().is_success() {
            return Err(SyncError::ProtocolError(format!("Server returned error: {}", response.status())));
        }
        
        let bundles: Vec<TrustBundle> = response.json().await
            .map_err(|e| SyncError::SerializationError(format!("Failed to parse trust bundles: {}", e)))?;
            
        Ok(bundles)
    }
    
    fn get_bundle_path(&self, bundle_id: &str) -> PathBuf {
        self.config.local_storage.join("bundles").join(format!("{}.json", bundle_id))
    }
    
    fn get_dag_object_path(&self, cid: &str) -> PathBuf {
        self.config.local_storage.join("dag").join(format!("{}.json", cid))
    }
    
    fn save_bundle_to_disk(&self, bundle: &TrustBundle) -> SyncResult<()> {
        let bundle_dir = self.config.local_storage.join("bundles");
        fs::create_dir_all(&bundle_dir)
            .map_err(|e| SyncError::IoError(format!("Failed to create bundles directory: {}", e)))?;
            
        let path = self.get_bundle_path(&bundle.id);
        
        let content = serde_json::to_string_pretty(&bundle)
            .map_err(|e| SyncError::SerializationError(format!("Failed to serialize bundle: {}", e)))?;
            
        let mut file = File::create(path)
            .map_err(|e| SyncError::IoError(format!("Failed to create bundle file: {}", e)))?;
            
        file.write_all(content.as_bytes())
            .map_err(|e| SyncError::IoError(format!("Failed to write bundle file: {}", e)))?;
            
        Ok(())
    }
    
    fn save_dag_object_to_disk(&self, obj: &DagObject, cid: &str) -> SyncResult<()> {
        let dag_dir = self.config.local_storage.join("dag");
        fs::create_dir_all(&dag_dir)
            .map_err(|e| SyncError::IoError(format!("Failed to create DAG directory: {}", e)))?;
            
        let path = self.get_dag_object_path(cid);
        
        let content = serde_json::to_string_pretty(&obj)
            .map_err(|e| SyncError::SerializationError(format!("Failed to serialize DAG object: {}", e)))?;
            
        let mut file = File::create(path)
            .map_err(|e| SyncError::IoError(format!("Failed to create DAG file: {}", e)))?;
            
        file.write_all(content.as_bytes())
            .map_err(|e| SyncError::IoError(format!("Failed to write DAG file: {}", e)))?;
            
        Ok(())
    }
    
    fn load_dag_object_from_disk(&self, cid: &str) -> SyncResult<DagObject> {
        let path = self.get_dag_object_path(cid);
        
        let mut file = File::open(path)
            .map_err(|e| SyncError::IoError(format!("Failed to open DAG file: {}", e)))?;
            
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| SyncError::IoError(format!("Failed to read DAG file: {}", e)))?;
            
        let obj: DagObject = serde_json::from_str(&content)
            .map_err(|e| SyncError::SerializationError(format!("Failed to deserialize DAG object: {}", e)))?;
            
        Ok(obj)
    }
} 