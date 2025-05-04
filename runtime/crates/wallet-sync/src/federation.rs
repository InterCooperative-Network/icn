/*!
 * ICN Wallet Federation Sync Client
 *
 * Provides functionality for wallet-side synchronization of credentials from federations,
 * allowing verification, storage, and notification of credential updates.
 */

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use reqwest::Client;
use thiserror::Error;
use tracing::{debug, info, warn, error};

// Define our own types for verifiable credentials to avoid dependency on core-vm
/// A verifiable credential
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiableCredential {
    /// Context for JSON-LD
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    
    /// Credential ID
    pub id: String,
    
    /// Credential types
    #[serde(rename = "type")]
    pub types: Vec<String>,
    
    /// Issuer of the credential
    pub issuer: String,
    
    /// Issuance date of the credential
    #[serde(rename = "issuanceDate")]
    pub issuance_date: String,
    
    /// Credential subject
    pub credential_subject: serde_json::Value,
    
    /// Proof of the credential
    pub proof: Option<serde_json::Value>,
}

/// Subject of an execution receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceiptSubject {
    /// ID of the receipt subject
    pub id: String,
    
    /// Transaction ID
    pub transaction_id: String,
    
    /// Status of the execution
    pub status: String,
    
    /// Result of the execution
    pub result: serde_json::Value,
}

/// Error types for federation sync
#[derive(Error, Debug)]
pub enum FederationSyncError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
    
    #[error("Failed to parse credential: {0}")]
    ParseError(String),
    
    #[error("Credential verification failed: {0}")]
    VerificationError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    
    #[error("Unknown error: {0}")]
    UnknownError(String),
}

/// Types of credentials that can be synchronized
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncCredentialType {
    /// Execution Receipts from proposal executions
    ExecutionReceipt,
    /// Proposal Outcomes from voting procedures
    ProposalOutcome,
    /// Resource transfers between entities
    ResourceTransfer,
    /// Membership credentials
    MembershipCredential,
}

/// Parameters for credential synchronization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncParameters {
    /// Federation ID to synchronize with
    pub federation_id: String,
    /// Types of credentials to synchronize
    pub credential_types: Vec<SyncCredentialType>,
    /// Start timestamp (inclusive)
    pub from_timestamp: DateTime<Utc>,
    /// End timestamp (inclusive, None means current time)
    pub to_timestamp: Option<DateTime<Utc>>,
    /// Maximum number of credentials to fetch
    pub limit: Option<usize>,
}

/// A federation endpoint for synchronization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederationEndpoint {
    /// Federation ID
    pub federation_id: String,
    /// Base URL for the federation API
    pub base_url: String,
    /// Last successful sync timestamp
    pub last_sync: Option<DateTime<Utc>>,
    /// Authentication token, if required
    pub auth_token: Option<String>,
}

/// Interface for credential storage
#[async_trait]
pub trait CredentialStore: Send + Sync {
    /// Store a credential
    async fn store_credential(&self, credential_type: SyncCredentialType, credential: &str) -> Result<String, FederationSyncError>;
    
    /// Get a credential by ID
    async fn get_credential(&self, credential_id: &str) -> Result<Option<String>, FederationSyncError>;
    
    /// List credentials by type
    async fn list_credentials(&self, credential_type: SyncCredentialType) -> Result<Vec<String>, FederationSyncError>;
}

/// A simple in-memory credential store
pub struct MemoryCredentialStore {
    credentials: std::sync::RwLock<HashMap<String, String>>,
    by_type: std::sync::RwLock<HashMap<SyncCredentialType, Vec<String>>>,
}

impl MemoryCredentialStore {
    /// Create a new memory credential store
    pub fn new() -> Self {
        Self {
            credentials: std::sync::RwLock::new(HashMap::new()),
            by_type: std::sync::RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl CredentialStore for MemoryCredentialStore {
    async fn store_credential(&self, credential_type: SyncCredentialType, credential: &str) -> Result<String, FederationSyncError> {
        // Parse the credential to get its ID
        let cred_value: serde_json::Value = serde_json::from_str(credential)
            .map_err(|e| FederationSyncError::ParseError(format!("Failed to parse credential: {}", e)))?;
        
        let cred_id = cred_value["id"]
            .as_str()
            .ok_or_else(|| FederationSyncError::ParseError("Credential missing ID".to_string()))?
            .to_string();
        
        // Store the credential
        {
            let mut creds = self.credentials.write().unwrap();
            creds.insert(cred_id.clone(), credential.to_string());
        }
        
        // Add to the type index
        {
            let mut by_type = self.by_type.write().unwrap();
            let type_list = by_type.entry(credential_type).or_insert_with(Vec::new);
            type_list.push(cred_id.clone());
        }
        
        Ok(cred_id)
    }
    
    async fn get_credential(&self, credential_id: &str) -> Result<Option<String>, FederationSyncError> {
        let creds = self.credentials.read().unwrap();
        Ok(creds.get(credential_id).cloned())
    }
    
    async fn list_credentials(&self, credential_type: SyncCredentialType) -> Result<Vec<String>, FederationSyncError> {
        let by_type = self.by_type.read().unwrap();
        let ids = by_type.get(&credential_type).cloned().unwrap_or_default();
        
        let creds = self.credentials.read().unwrap();
        let mut result = Vec::new();
        
        for id in ids {
            if let Some(cred) = creds.get(&id) {
                result.push(cred.clone());
            }
        }
        
        Ok(result)
    }
}

/// Interface for credential notification
#[async_trait]
pub trait CredentialNotifier: Send + Sync {
    /// Notify about a new credential
    async fn notify_credential(&self, credential_type: SyncCredentialType, credential_id: &str, credential: &str) -> Result<(), FederationSyncError>;
}

/// Federation sync client configuration
#[derive(Debug, Clone)]
pub struct FederationSyncClientConfig {
    /// Federation endpoints
    pub endpoints: Vec<FederationEndpoint>,
    /// Automatic sync interval
    pub sync_interval: Option<Duration>,
    /// Verify credentials
    pub verify_credentials: bool,
    /// Notification enabled
    pub notify_on_sync: bool,
}

impl Default for FederationSyncClientConfig {
    fn default() -> Self {
        Self {
            endpoints: Vec::new(),
            sync_interval: Some(Duration::from_secs(300)), // 5 minutes
            verify_credentials: true,
            notify_on_sync: true,
        }
    }
}

/// Client for federation credential synchronization
pub struct FederationSyncClient<S, N>
where
    S: CredentialStore,
    N: CredentialNotifier,
{
    /// HTTP client
    http_client: Client,
    /// Credential store
    store: Arc<S>,
    /// Credential notifier
    notifier: Option<Arc<N>>,
    /// Configuration
    config: FederationSyncClientConfig,
    /// Last sync time by federation
    last_sync_times: std::sync::RwLock<HashMap<String, DateTime<Utc>>>,
}

impl<S, N> FederationSyncClient<S, N>
where
    S: CredentialStore + 'static,
    N: CredentialNotifier + 'static,
{
    /// Create a new federation sync client
    pub fn new(store: Arc<S>, config: FederationSyncClientConfig) -> Self {
        Self {
            http_client: Client::new(),
            store,
            notifier: None,
            config,
            last_sync_times: std::sync::RwLock::new(HashMap::new()),
        }
    }
    
    /// Set the credential notifier
    pub fn with_notifier(mut self, notifier: Arc<N>) -> Self {
        self.notifier = Some(notifier);
        self
    }
    
    /// Start the background sync task
    pub fn start_background_sync(&self) -> tokio::task::JoinHandle<()> {
        if self.config.sync_interval.is_none() {
            return tokio::spawn(async {
                info!("Background sync disabled");
            });
        }
        
        let http_client = self.http_client.clone();
        let store = self.store.clone();
        let notifier = self.notifier.clone();
        let config = self.config.clone();
        let sync_times = self.last_sync_times.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.sync_interval.unwrap());
            
            loop {
                interval.tick().await;
                
                info!("Starting wallet-side federation credential sync");
                
                for endpoint in &config.endpoints {
                    let from_timestamp = {
                        let sync_times = sync_times.read().unwrap();
                        sync_times.get(&endpoint.federation_id).cloned().unwrap_or_else(|| 
                            DateTime::<Utc>::from_utc(
                                chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
                                Utc,
                            )
                        )
                    };
                    
                    let params = SyncParameters {
                        federation_id: endpoint.federation_id.clone(),
                        credential_types: vec![
                            SyncCredentialType::ExecutionReceipt,
                            SyncCredentialType::ProposalOutcome,
                        ],
                        from_timestamp,
                        to_timestamp: None,
                        limit: Some(100),
                    };
                    
                    match sync_credentials_from_endpoint(
                        &http_client,
                        endpoint,
                        &params,
                        &store,
                        notifier.as_deref(),
                        config.verify_credentials,
                    ).await {
                        Ok(count) => {
                            info!(
                                federation = %endpoint.federation_id,
                                credentials_synced = %count,
                                "Successfully synced credentials from federation"
                            );
                            
                            // Update last sync time
                            let mut sync_times = sync_times.write().unwrap();
                            sync_times.insert(endpoint.federation_id.clone(), Utc::now());
                        }
                        Err(e) => {
                            error!(
                                federation = %endpoint.federation_id,
                                error = %e,
                                "Failed to sync credentials from federation"
                            );
                        }
                    }
                }
            }
        })
    }
    
    /// Synchronize credentials from a specific federation
    pub async fn sync_credentials(
        &self,
        federation_id: &str,
        credential_types: &[SyncCredentialType],
        from_timestamp: DateTime<Utc>,
    ) -> Result<usize, FederationSyncError> {
        // Find the endpoint for the federation
        let endpoint = self.config.endpoints.iter()
            .find(|e| e.federation_id == federation_id)
            .ok_or_else(|| FederationSyncError::ConfigurationError(
                format!("Federation endpoint not configured: {}", federation_id)
            ))?;
        
        let params = SyncParameters {
            federation_id: federation_id.to_string(),
            credential_types: credential_types.to_vec(),
            from_timestamp,
            to_timestamp: None,
            limit: None,
        };
        
        sync_credentials_from_endpoint(
            &self.http_client,
            endpoint,
            &params,
            &self.store,
            self.notifier.as_deref(),
            self.config.verify_credentials,
        ).await
    }
    
    /// Get credentials by type from local store
    pub async fn get_credentials_by_type(
        &self,
        credential_type: SyncCredentialType,
    ) -> Result<Vec<String>, FederationSyncError> {
        self.store.list_credentials(credential_type).await
    }
    
    /// Get a specific credential by ID
    pub async fn get_credential(
        &self,
        credential_id: &str,
    ) -> Result<Option<String>, FederationSyncError> {
        self.store.get_credential(credential_id).await
    }

    /// Retrieves execution receipts from a federation node
    pub async fn fetch_execution_receipts(
        &self,
        federation_id: &str,
        scope: &str,
        since: Option<i64>,
    ) -> Result<Vec<VerifiableCredential>, FederationSyncError> {
        // Find the endpoint for the given federation ID
        let endpoint = self.config.endpoints.iter()
            .find(|e| e.federation_id == federation_id)
            .ok_or_else(|| FederationSyncError::ConfigurationError(
                format!("No endpoint configured for federation: {}", federation_id)
            ))?;
        
        // Construct the request URL
        let mut url = format!("{}/dag/receipts?scope={}", endpoint.base_url, scope);
        
        // Add optional timestamp filter
        if let Some(timestamp) = since {
            url.push_str(&format!("&since={}", timestamp));
        }
        
        debug!("Fetching execution receipts from: {}", url);
        
        // Create request with optional authentication
        let mut request = self.http_client.get(&url);
        if let Some(token) = &endpoint.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        // Execute request
        let response = request.send().await?;
        
        // Handle errors
        if !response.status().is_success() {
            return Err(FederationSyncError::HttpError(
                reqwest::Error::from(response.error_for_status().unwrap_err())
            ));
        }
        
        // Parse response
        let receipts: Vec<VerifiableCredential> = response.json().await?;
        
        // Optionally verify receipts
        if self.config.verify_credentials {
            let mut verified_receipts = Vec::new();
            for receipt in receipts {
                if verify_execution_receipt(&receipt) {
                    verified_receipts.push(receipt);
                } else {
                    warn!("Skipping unverified receipt: {}", receipt.id);
                }
            }
            Ok(verified_receipts)
        } else {
            Ok(receipts)
        }
    }
}

/// Verifies an execution receipt credential
pub fn verify_execution_receipt(receipt: &VerifiableCredential) -> bool {
    // Basic verification:
    
    // 1. Check if it's an ExecutionReceipt type
    if !receipt.types.iter().any(|t| t == "ExecutionReceipt") {
        return false;
    }
    
    // 2. Verify it has required fields
    if receipt.issuer.is_empty() || receipt.issuance_date.is_empty() {
        return false;
    }
    
    // 3. Verify the subject has required fields
    let subject = &receipt.credential_subject;
    if !subject.is_object() 
        || subject["id"].as_str().is_none() 
        || subject["proposal_id"].as_str().is_none() 
        || subject["outcome"].as_str().is_none() {
        return false;
    }
    
    // 4. Verify proof if available
    if let Some(proof) = &receipt.proof {
        // In a real implementation, you would verify the proof cryptographically
        // For now, just check if it has the required fields
        if !proof.is_object() 
            || proof["type"].as_str().is_none() 
            || proof["created"].as_str().is_none() 
            || proof["proofValue"].as_str().is_none() {
            return false;
        }
    }
    
    true
}

/// Exports receipts to various formats
pub fn export_receipts(
    receipts: &[VerifiableCredential], 
    format: ExportFormat
) -> Result<String, FederationSyncError> {
    match format {
        ExportFormat::Json => {
            // Export as JSON array
            serde_json::to_string_pretty(receipts)
                .map_err(|e| FederationSyncError::ParseError(
                    format!("Failed to serialize receipts to JSON: {}", e)
                ))
        },
        ExportFormat::Csv => {
            // Export as CSV
            let mut csv = String::new();
            
            // Write header
            csv.push_str("id,issuer,issuance_date,proposal_id,outcome,federation_scope\n");
            
            // Write rows
            for receipt in receipts {
                let subject = &receipt.credential_subject;
                let proposal_id = subject["proposal_id"].as_str().unwrap_or("");
                let outcome = subject["outcome"].as_str().unwrap_or("");
                let federation_scope = subject["federation_scope"].as_str().unwrap_or("");
                
                csv.push_str(&format!(
                    "{},{},{},{},{},{}\n",
                    receipt.id,
                    receipt.issuer,
                    receipt.issuance_date,
                    proposal_id,
                    outcome,
                    federation_scope
                ));
            }
            
            Ok(csv)
        },
        ExportFormat::SignedBundle => {
            // Create a signed bundle
            let bundle = SignedReceiptBundle {
                receipts: receipts.to_vec(),
                timestamp: Utc::now().to_rfc3339(),
                signature: None, // In a real implementation, sign the bundle
            };
            
            serde_json::to_string_pretty(&bundle)
                .map_err(|e| FederationSyncError::ParseError(
                    format!("Failed to serialize receipt bundle: {}", e)
                ))
        }
    }
}

/// Format for exporting receipts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// JSON format
    Json,
    /// CSV format
    Csv,
    /// Signed bundle format
    SignedBundle,
}

/// A signed bundle of receipts for export/import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedReceiptBundle {
    /// Collection of receipts
    pub receipts: Vec<VerifiableCredential>,
    /// Timestamp of bundle creation
    pub timestamp: String,
    /// Optional signature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Synchronize credentials from a federation endpoint
async fn sync_credentials_from_endpoint<S, N>(
    http_client: &Client,
    endpoint: &FederationEndpoint,
    params: &SyncParameters,
    store: &Arc<S>,
    notifier: Option<&N>,
    verify: bool,
) -> Result<usize, FederationSyncError>
where
    S: CredentialStore,
    N: CredentialNotifier,
{
    let mut url = reqwest::Url::parse(&format!("{}/federation/credentials/sync", endpoint.base_url))
        .map_err(|e| FederationSyncError::ConfigurationError(
            format!("Invalid federation endpoint URL: {}", e)
        ))?;
    
    // Add query parameters
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("federationId", &params.federation_id);
        
        // Add credential types
        for cred_type in &params.credential_types {
            query.append_pair("credentialType", &format!("{:?}", cred_type));
        }
        
        // Add timestamp range
        query.append_pair("fromTimestamp", &params.from_timestamp.to_rfc3339());
        
        if let Some(to) = params.to_timestamp {
            query.append_pair("toTimestamp", &to.to_rfc3339());
        }
        
        // Add limit
        if let Some(limit) = params.limit {
            query.append_pair("limit", &limit.to_string());
        }
    }
    
    // Build the request
    let mut request = http_client.get(url);
    
    // Add authentication if available
    if let Some(token) = &endpoint.auth_token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }
    
    // Execute the request
    let response = request.send().await?;
    
    // Check response status
    if !response.status().is_success() {
        return Err(FederationSyncError::HttpError(
            reqwest::Error::new_deprecated(
                format!("HTTP error: {}", response.status())
            )
        ));
    }
    
    // Parse the response
    let credentials: Vec<String> = response.json().await?;
    
    // Process the credentials
    let mut processed_count = 0;
    
    for (i, credential) in credentials.iter().enumerate() {
        // Determine credential type
        let cred_value: serde_json::Value = match serde_json::from_str(credential) {
            Ok(value) => value,
            Err(e) => {
                warn!(
                    index = %i,
                    error = %e,
                    "Failed to parse credential JSON"
                );
                continue;
            }
        };
        
        let cred_types = match cred_value["type"].as_array() {
            Some(types) => types,
            None => {
                warn!(
                    index = %i,
                    "Credential missing type array"
                );
                continue;
            }
        };
        
        let cred_type = if cred_types.len() > 1 {
            match cred_types[1].as_str() {
                Some("ExecutionReceipt") => SyncCredentialType::ExecutionReceipt,
                Some("ProposalOutcome") => SyncCredentialType::ProposalOutcome,
                Some("ResourceTransfer") => SyncCredentialType::ResourceTransfer,
                Some("MembershipCredential") => SyncCredentialType::MembershipCredential,
                _ => {
                    warn!(
                        index = %i,
                        type_value = %cred_types[1],
                        "Unknown credential type"
                    );
                    continue;
                }
            }
        } else {
            warn!(
                index = %i,
                "Credential missing type information"
            );
            continue;
        };
        
        // Check if this type was requested
        if !params.credential_types.contains(&cred_type) {
            continue;
        }
        
        // Verify the credential if required
        // In a real implementation, we would verify the signature
        
        // Store the credential
        match store.store_credential(cred_type, credential).await {
            Ok(cred_id) => {
                processed_count += 1;
                
                // Notify if a notifier is configured
                if let Some(n) = notifier {
                    if let Err(e) = n.notify_credential(cred_type, &cred_id, credential).await {
                        warn!(
                            credential_id = %cred_id,
                            error = %e,
                            "Failed to send credential notification"
                        );
                    }
                }
            }
            Err(e) => {
                warn!(
                    index = %i,
                    error = %e,
                    "Failed to store credential"
                );
            }
        }
    }
    
    Ok(processed_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[derive(Default)]
    struct MockNotifier {
        notified: std::sync::RwLock<Vec<String>>,
    }
    
    impl MockNotifier {
        fn new() -> Self {
            Self {
                notified: std::sync::RwLock::new(Vec::new()),
            }
        }
        
        fn get_notified(&self) -> Vec<String> {
            self.notified.read().unwrap().clone()
        }
    }
    
    #[async_trait]
    impl CredentialNotifier for MockNotifier {
        async fn notify_credential(
            &self,
            credential_type: SyncCredentialType,
            credential_id: &str,
            credential: &str,
        ) -> Result<(), FederationSyncError> {
            let mut notified = self.notified.write().unwrap();
            notified.push(credential_id.to_string());
            Ok(())
        }
    }
    
    #[tokio::test]
    async fn test_memory_store() {
        let store = MemoryCredentialStore::new();
        
        let cred = r#"{"@context":["https://www.w3.org/2018/credentials/v1"],"id":"test-id-1","type":["VerifiableCredential","ExecutionReceipt"]}"#;
        
        let id = store.store_credential(SyncCredentialType::ExecutionReceipt, cred).await.unwrap();
        assert_eq!(id, "test-id-1");
        
        let retrieved = store.get_credential("test-id-1").await.unwrap().unwrap();
        assert_eq!(retrieved, cred);
        
        let all = store.list_credentials(SyncCredentialType::ExecutionReceipt).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0], cred);
    }
} 