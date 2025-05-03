use serde::{Serialize, Deserialize};
use serde_json::Value;
use reqwest::{Client as HttpClient, StatusCode};
use crate::error::{AgentResult, AgentError};
use wallet_core::identity::IdentityWallet;
use wallet_core::dag::DagNode;
use wallet_core::dag::ThreadType;
use wallet_core::vc::VerifiableCredential;
use wallet_types::network::NodeSubmissionResponse;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use std::time::{Duration, Instant};
use backoff::ExponentialBackoff;
use async_trait::async_trait;
use tracing::{info, warn, error, debug};
use uuid::Uuid;
use tokio::time::sleep;

const DEFAULT_AGORANET_URL: &str = "https://agoranet.icn.network/api";
const DEFAULT_CACHE_TTL_SECS: u64 = 300; // 5 minutes
const MAX_RETRIES: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub id: String,
    pub title: String,
    pub proposal_id: Option<String>,
    pub topic: String,
    pub author: String,
    pub created_at: String,
    pub post_count: usize,
    pub credential_links: Vec<CredentialLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadDetail {
    pub id: String,
    pub title: String,
    pub proposal_id: Option<String>,
    pub topic: String,
    pub author: String,
    pub created_at: String,
    pub posts: Vec<Post>,
    pub credential_links: Vec<CredentialLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub thread_id: String,
    pub content: String,
    pub author: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialLink {
    pub id: String,
    pub thread_id: String,
    pub credential_id: String,
    pub credential_type: String,
    pub issuer: String,
    pub subject: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCredentialLinkRequest {
    pub thread_id: String,
    pub credential: VerifiableCredential,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub status: u16,
    pub message: String,
}

// Cache entry with expiration
struct CacheEntry<T> {
    data: T,
    expires_at: Instant,
}

// Cache type for thread summaries - key is query string (filter params)
type ThreadCache = HashMap<String, CacheEntry<Vec<ThreadSummary>>>;

// Cache type for thread details - key is thread ID
type ThreadDetailCache = HashMap<String, CacheEntry<ThreadDetail>>;

// Cache type for credential links - key is thread ID
type CredentialLinkCache = HashMap<String, CacheEntry<Vec<CredentialLink>>>;

pub struct AgoraNetClient {
    base_url: String,
    http_client: HttpClient,
    identity: IdentityWallet,
    thread_cache: Arc<RwLock<ThreadCache>>,
    thread_detail_cache: Arc<RwLock<ThreadDetailCache>>,
    credential_link_cache: Arc<RwLock<CredentialLinkCache>>,
    cache_ttl: Duration,
    connectivity_lock: Arc<Mutex<()>>,
}

/// Trait for handling API responses with proper error handling
#[async_trait]
trait ApiResponseHandler {
    async fn handle_api_response<T: for<'de> serde::Deserialize<'de> + Send>(
        &self, 
        response: reqwest::Response,
        context: &str
    ) -> AgentResult<T>;
}

#[async_trait]
impl ApiResponseHandler for AgoraNetClient {
    async fn handle_api_response<T: for<'de> serde::Deserialize<'de> + Send>(
        &self, 
        response: reqwest::Response,
        context: &str
    ) -> AgentResult<T> {
        let status = response.status();
        if status.is_success() {
            match response.json::<T>().await {
                Ok(data) => Ok(data),
                Err(e) => {
                    error!("Failed to parse response from {}: {}", context, e);
                    Err(AgentError::SerializationError(format!(
                        "Failed to parse response from {}: {}", context, e
                    )))
                }
            }
        } else {
            let error_body = match response.text().await {
                Ok(body) => body,
                Err(_) => "[Could not read error body]".to_string(),
            };
            
            let error_message = match status {
                StatusCode::UNAUTHORIZED => format!("Authentication failed when {}", context),
                StatusCode::FORBIDDEN => format!("Permission denied when {}", context),
                StatusCode::NOT_FOUND => format!("Resource not found when {}", context),
                StatusCode::TOO_MANY_REQUESTS => format!("Rate limit exceeded when {}", context),
                StatusCode::INTERNAL_SERVER_ERROR => format!("Server error occurred when {}", context),
                _ => format!("Error {} when {}: {}", status.as_u16(), context, error_body),
            };
            
            error!("{}", error_message);
            
            match status {
                StatusCode::UNAUTHORIZED => Err(AgentError::AuthenticationError(error_message)),
                StatusCode::FORBIDDEN => Err(AgentError::PermissionError(error_message)),
                StatusCode::NOT_FOUND => Err(AgentError::ResourceNotFound(error_message)),
                StatusCode::TOO_MANY_REQUESTS => Err(AgentError::RateLimitExceeded(error_message)),
                StatusCode::INTERNAL_SERVER_ERROR => Err(AgentError::ServerError(error_message)),
                _ => Err(AgentError::GovernanceError(error_message)),
            }
        }
    }
}

impl AgoraNetClient {
    pub fn new(identity: IdentityWallet, base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| DEFAULT_AGORANET_URL.to_string()),
            http_client: HttpClient::builder()
                .timeout(Duration::from_secs(30))
                .connect_timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| HttpClient::new()),
            identity,
            thread_cache: Arc::new(RwLock::new(HashMap::new())),
            thread_detail_cache: Arc::new(RwLock::new(HashMap::new())),
            credential_link_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: Duration::from_secs(DEFAULT_CACHE_TTL_SECS),
            connectivity_lock: Arc::new(Mutex::new(())),
        }
    }
    
    /// Set custom cache TTL
    pub fn with_cache_ttl(mut self, ttl_seconds: u64) -> Self {
        self.cache_ttl = Duration::from_secs(ttl_seconds);
        self
    }
    
    /// Clear all caches
    pub async fn clear_caches(&self) {
        let mut thread_cache = self.thread_cache.write().await;
        thread_cache.clear();
        
        let mut thread_detail_cache = self.thread_detail_cache.write().await;
        thread_detail_cache.clear();
        
        let mut credential_link_cache = self.credential_link_cache.write().await;
        credential_link_cache.clear();
        
        debug!("Cleared all AgoraNet caches");
    }
    
    /// Fetch threads from AgoraNet with caching and retry logic
    pub async fn get_threads(&self, proposal_id: Option<&str>, topic: Option<&str>) -> AgentResult<Vec<ThreadSummary>> {
        // Create cache key from query parameters
        let cache_key = format!(
            "proposal_id={};topic={}", 
            proposal_id.unwrap_or(""), 
            topic.unwrap_or("")
        );
        
        // Check cache first
        {
            let cache = self.thread_cache.read().await;
            if let Some(entry) = cache.get(&cache_key) {
                if Instant::now() < entry.expires_at {
                    debug!("Thread cache hit for query: {}", cache_key);
                    return Ok(entry.data.clone());
                }
            }
        }
        
        // Prepare query parameters
        let mut query_params = HashMap::new();
        if let Some(pid) = proposal_id {
            query_params.insert("proposal_id", pid);
        }
        if let Some(t) = topic {
            query_params.insert("topic", t);
        }
        
        // Define the operation to fetch threads
        let url = format!("{}/threads", self.base_url);
        let self_clone = self.clone();
        let query_params_clone = query_params.clone();
        
        // Execute with retry logic
        let threads = self.retry_with_backoff("fetch threads", move || {
            let url = url.clone();
            let query_params = query_params_clone.clone();
            let client = self_clone.clone();
            
            async move {
                client.try_get_threads(&url, &query_params).await
            }
        }).await?;
        
        // Update the cache
        {
            let mut cache = self.thread_cache.write().await;
            cache.insert(cache_key, CacheEntry {
                data: threads.clone(),
                expires_at: Instant::now() + self.cache_ttl,
            });
        }
        
        Ok(threads)
    }
    
    /// Helper function to attempt fetching threads without retry logic
    async fn try_get_threads(&self, url: &str, query_params: &HashMap<&str, &str>) -> AgentResult<Vec<ThreadSummary>> {
        let response = self.http_client.get(url)
            .query(query_params)
            .header("Authorization", format!("DID {}", self.identity.did))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() || e.is_connect() {
                    AgentError::ConnectionError(format!("Connection error: {}", e))
                } else {
                    AgentError::NetworkError(format!("Network error: {}", e))
                }
            })?;
            
        // Check status code
        if !response.status().is_success() {
            // Handle different error status codes
            match response.status() {
                StatusCode::FORBIDDEN => {
                    return Err(AgentError::PermissionError(format!(
                        "Permission denied fetching threads: {}", response.status()
                    )));
                },
                StatusCode::UNAUTHORIZED => {
                    return Err(AgentError::AuthenticationError(format!(
                        "Authentication failed fetching threads: {}", response.status()
                    )));
                },
                StatusCode::NOT_FOUND => {
                    return Err(AgentError::ResourceNotFound(format!(
                        "Resource not found fetching threads: {}", response.status()
                    )));
                },
                StatusCode::TOO_MANY_REQUESTS => {
                    return Err(AgentError::RateLimitExceeded(format!(
                        "Rate limit exceeded fetching threads: {}", response.status()
                    )));
                },
                StatusCode::INTERNAL_SERVER_ERROR => {
                    return Err(AgentError::ServerError(format!(
                        "Server error fetching threads: {}", response.status()
                    )));
                },
                _ => {
                    return Err(AgentError::ConnectionError(format!(
                        "Network error fetching threads: {}", response.status()
                    )));
                }
            }
        }
        
        // Try to parse the response as a list of thread info
        response.json::<Vec<ThreadSummary>>().await
            .map_err(|e| AgentError::SerializationError(format!(
                "Failed to deserialize thread list: {}", e
            )))
    }
    
    /// Fetch a specific thread by ID with caching and retry logic
    pub async fn get_thread(&self, thread_id: &str) -> AgentResult<ThreadDetail> {
        // Check cache first
        {
            let cache = self.thread_detail_cache.read().await;
            if let Some(entry) = cache.get(thread_id) {
                if Instant::now() < entry.expires_at {
                    debug!("Thread detail cache hit for ID: {}", thread_id);
                    return Ok(entry.data.clone());
                }
            }
        }
        
        // Define the operation to fetch the thread
        let url = format!("{}/threads/{}", self.base_url, thread_id);
        let thread_id_clone = thread_id.to_string();
        let self_clone = self.clone();
        
        // Execute with retry logic
        let thread = self.retry_with_backoff("fetch thread details", move || {
            let url = url.clone();
            let thread_id = thread_id_clone.clone();
            let client = self_clone.clone();
            
            async move {
                client.try_get_thread(&url, &thread_id).await
            }
        }).await?;
        
        // Update the cache
        {
            // Update the thread detail cache
            let mut cache = self.thread_detail_cache.write().await;
            cache.insert(thread_id.to_string(), CacheEntry {
                data: thread.clone(),
                expires_at: Instant::now() + self.cache_ttl,
            });
            
            // Also update the credential links cache
            let mut link_cache = self.credential_link_cache.write().await;
            link_cache.insert(thread_id.to_string(), CacheEntry {
                data: thread.credential_links.clone(),
                expires_at: Instant::now() + self.cache_ttl,
            });
        }
        
        Ok(thread)
    }
    
    /// Helper method to fetch a thread without retry logic
    async fn try_get_thread(&self, url: &str, thread_id: &str) -> AgentResult<ThreadDetail> {
        let response = self.http_client.get(url)
            .header("Authorization", format!("DID {}", self.identity.did))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() || e.is_connect() {
                    AgentError::ConnectionError(format!("Connection error: {}", e))
                } else {
                    AgentError::NetworkError(format!("Network error: {}", e))
                }
            })?;
            
        // Check status code
        if !response.status().is_success() {
            // Handle different error status codes
            match response.status() {
                StatusCode::FORBIDDEN => {
                    return Err(AgentError::PermissionError(format!(
                        "Permission denied fetching thread {}: {}", thread_id, response.status()
                    )));
                },
                StatusCode::UNAUTHORIZED => {
                    return Err(AgentError::AuthenticationError(format!(
                        "Authentication failed fetching thread {}: {}", thread_id, response.status()
                    )));
                },
                StatusCode::NOT_FOUND => {
                    return Err(AgentError::ResourceNotFound(format!(
                        "Thread not found: {}", thread_id
                    )));
                },
                StatusCode::TOO_MANY_REQUESTS => {
                    return Err(AgentError::RateLimitExceeded(format!(
                        "Rate limit exceeded fetching thread {}: {}", thread_id, response.status()
                    )));
                },
                StatusCode::INTERNAL_SERVER_ERROR => {
                    return Err(AgentError::ServerError(format!(
                        "Server error fetching thread {}: {}", thread_id, response.status()
                    )));
                },
                _ => {
                    return Err(AgentError::ConnectionError(format!(
                        "Network error fetching thread {}: {}", thread_id, response.status()
                    )));
                }
            }
        }
        
        // Try to parse the response as thread details
        response.json::<ThreadDetail>().await
            .map_err(|e| AgentError::SerializationError(format!(
                "Failed to deserialize thread details: {}", e
            )))
    }
    
    /// Link a credential to a thread with retry logic
    pub async fn link_credential(&self, thread_id: &str, credential: &VerifiableCredential) -> AgentResult<CredentialLink> {
        // Prepare request data
        let request = CreateCredentialLinkRequest {
            thread_id: thread_id.to_string(),
            credential: credential.clone(),
        };
        
        // Sign the request with our identity
        let payload = serde_json::to_string(&request)
            .map_err(|e| AgentError::SerializationError(format!("Failed to serialize request: {}", e)))?;
        
        let signature = self.identity.sign_message(payload.as_bytes());
        let signature_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &signature);
        
        // Execute request with retry logic
        let url = format!("{}/threads/credential-link", self.base_url);
        
        // Configure backoff strategy
        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(Duration::from_secs(30)),
            ..ExponentialBackoff::default()
        };
        
        // Lock to prevent multiple retry-heavy operations
        let _lock = self.connectivity_lock.lock().await;
        
        // Retry with exponential backoff
        let result = async {
            let response = self.http_client.post(&url)
                .header("Authorization", format!("DID {}", self.identity.did))
                .header("X-Signature", signature_b64.clone())
                .json(&request)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() || e.is_connect() {
                        backoff::Error::transient(e)
                    } else {
                        backoff::Error::permanent(e)
                    }
                })?;
                
            // Check status code
            if !response.status().is_success() {
                // Handle different error status codes
                match response.status() {
                    StatusCode::FORBIDDEN | 
                    StatusCode::UNAUTHORIZED | 
                    StatusCode::NOT_FOUND |
                    StatusCode::TOO_MANY_REQUESTS | 
                    StatusCode::INTERNAL_SERVER_ERROR => {
                        return Err(backoff::Error::permanent(self.handle_api_response::<CredentialLink>(response, "linking credential").await?));
                    },
                    _ => {
                        return Err(backoff::Error::transient(AgentError::ConnectionError(format!(
                            "Network error linking credential: {}", response.status()
                        ))));
                    }
                }
            }
            
            // Try to parse the response as a credential link
            let link: CredentialLink = response.json().await
                .map_err(|e| backoff::Error::permanent(AgentError::SerializationError(format!(
                    "Failed to deserialize credential link: {}", e
                ))))?;
                
            Ok(link)
        }.await;
        
        match result {
            Ok(link) => {
                // Invalidate relevant caches
                self.invalidate_thread_caches(thread_id).await;
                
                Ok(link)
            },
            Err(e) => Err(e)
        }
    }
    
    /// Get credential links for a thread with caching and retry logic
    pub async fn get_credential_links(&self, thread_id: &str) -> AgentResult<Vec<CredentialLink>> {
        // Check cache first
        {
            let cache = self.credential_link_cache.read().await;
            if let Some(entry) = cache.get(thread_id) {
                if Instant::now() < entry.expires_at {
                    debug!("Credential link cache hit for thread ID: {}", thread_id);
                    return Ok(entry.data.clone());
                }
            }
        }
        
        // Execute request with retry logic
        let url = format!("{}/threads/{}/credential-links", self.base_url, thread_id);
        
        // Configure backoff strategy
        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(Duration::from_secs(30)),
            ..ExponentialBackoff::default()
        };
        
        // Lock to prevent multiple retry-heavy operations
        let _lock = self.connectivity_lock.lock().await;
        
        // Retry with exponential backoff
        let result = async {
            let response = self.http_client.get(&url)
                .header("Authorization", format!("DID {}", self.identity.did))
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() || e.is_connect() {
                        backoff::Error::transient(e)
                    } else {
                        backoff::Error::permanent(e)
                    }
                })?;
                
            // Check status code
            if !response.status().is_success() {
                // Handle different error status codes
                match response.status() {
                    StatusCode::FORBIDDEN | 
                    StatusCode::UNAUTHORIZED | 
                    StatusCode::NOT_FOUND |
                    StatusCode::TOO_MANY_REQUESTS | 
                    StatusCode::INTERNAL_SERVER_ERROR => {
                        return Err(backoff::Error::permanent(self.handle_api_response::<Vec<CredentialLink>>(response, "fetching credential links").await?));
                    },
                    _ => {
                        return Err(backoff::Error::transient(AgentError::ConnectionError(format!(
                            "Network error fetching credential links: {}", response.status()
                        ))));
                    }
                }
            }
            
            // Try to parse the response as a list of credential links
            let links: Vec<CredentialLink> = response.json().await
                .map_err(|e| backoff::Error::permanent(AgentError::SerializationError(format!(
                    "Failed to deserialize credential links: {}", e
                ))))?;
                
            Ok(links)
        }.await;
        
        match result {
            Ok(links) => {
                // Update the cache
                let mut cache = self.credential_link_cache.write().await;
                cache.insert(thread_id.to_string(), CacheEntry {
                    data: links.clone(),
                    expires_at: Instant::now() + self.cache_ttl,
                });
                
                Ok(links)
            },
            Err(e) => Err(e)
        }
    }
    
    /// Notify AgoraNet about a proposal event with retry logic
    pub async fn notify_proposal_event(&self, proposal_id: &str, event_type: &str, details: Value) -> AgentResult<()> {
        // Prepare request data
        let payload = serde_json::json!({
            "event_type": event_type,
            "details": details,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        
        // Execute request with retry logic
        let url = format!("{}/proposals/{}/events", self.base_url, proposal_id);
        
        // Configure backoff strategy
        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(Duration::from_secs(30)),
            ..ExponentialBackoff::default()
        };
        
        // Lock to prevent multiple retry-heavy operations
        let _lock = self.connectivity_lock.lock().await;
        
        // Retry with exponential backoff
        let result = async {
            let response = self.http_client.post(&url)
                .header("Authorization", format!("DID {}", self.identity.did))
                .json(&payload)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() || e.is_connect() {
                        backoff::Error::transient(e)
                    } else {
                        backoff::Error::permanent(e)
                    }
                })?;
                
            // Check status code
            if !response.status().is_success() {
                // Handle different error status codes
                match response.status() {
                    StatusCode::FORBIDDEN | 
                    StatusCode::UNAUTHORIZED | 
                    StatusCode::NOT_FOUND |
                    StatusCode::TOO_MANY_REQUESTS | 
                    StatusCode::INTERNAL_SERVER_ERROR => {
                        return Err(backoff::Error::permanent(self.handle_api_response::<()>(response, "notifying about event").await?));
                    },
                    _ => {
                        return Err(backoff::Error::transient(AgentError::ConnectionError(format!(
                            "Network error notifying AgoraNet: {}", response.status()
                        ))));
                    }
                }
            }
            
            Ok(())
        }.await;
        
        match result {
            Ok(()) => {
                // Invalidate relevant caches for this proposal
                self.invalidate_proposal_caches(proposal_id).await;
                Ok(())
            },
            Err(e) => Err(e)
        }
    }
    
    /// Invalidate caches related to a specific thread
    async fn invalidate_thread_caches(&self, thread_id: &str) {
        // Remove from thread detail cache
        {
            let mut cache = self.thread_detail_cache.write().await;
            cache.remove(thread_id);
        }
        
        // Remove from credential link cache
        {
            let mut cache = self.credential_link_cache.write().await;
            cache.remove(thread_id);
        }
        
        // Thread summary cache is trickier since we need to find all entries that might
        // contain this thread and invalidate them
        // For simplicity, we'll just clear the entire thread cache
        {
            let mut cache = self.thread_cache.write().await;
            cache.clear();
        }
        
        debug!("Invalidated caches for thread ID: {}", thread_id);
    }
    
    /// Invalidate caches related to a specific proposal
    async fn invalidate_proposal_caches(&self, proposal_id: &str) {
        // Thread summary cache might contain this proposal
        // For simplicity, we'll just clear the entire thread cache
        {
            let mut cache = self.thread_cache.write().await;
            cache.clear();
        }
        
        // For thread detail and credential link caches, we'd need to know which threads
        // are associated with this proposal. We don't have that mapping here, so
        // we'll just rely on cache TTL to eventually refresh.
        
        debug!("Invalidated caches for proposal ID: {}", proposal_id);
    }
    
    /// Check if AgoraNet service is available
    pub async fn check_connection(&self) -> AgentResult<bool> {
        let url = format!("{}/health", self.base_url);
        
        match self.http_client.get(&url)
            .timeout(Duration::from_secs(5))
            .send().await 
        {
            Ok(response) => Ok(response.status().is_success()),
            Err(e) => {
                warn!("AgoraNet connection check failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Handle error responses from the AgoraNet API
    async fn handle_error_response<T>(&self, response: reqwest::Response) -> AgentResult<T> {
        let status = response.status();
        let error_message = match response.text().await {
            Ok(text) => text,
            Err(_) => format!("Unknown error (status {})", status),
        };
        
        match status {
            StatusCode::UNAUTHORIZED => Err(AgentError::AuthenticationError(error_message)),
            StatusCode::FORBIDDEN => Err(AgentError::PermissionError(error_message)),
            StatusCode::NOT_FOUND => Err(AgentError::ResourceNotFound(error_message)),
            StatusCode::TOO_MANY_REQUESTS => Err(AgentError::RateLimitExceeded(error_message)),
            StatusCode::INTERNAL_SERVER_ERROR => Err(AgentError::ServerError(error_message)),
            _ => Err(AgentError::GovernanceError(error_message)),
        }
    }

    /// Generic retry function for network operations
    async fn retry_with_backoff<T, F, Fut>(&self, operation_name: &str, operation: F) -> AgentResult<T> 
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = AgentResult<T>>,
    {
        // Configure backoff strategy
        let backoff = ExponentialBackoff {
            max_elapsed_time: Some(Duration::from_secs(30)),
            ..ExponentialBackoff::default()
        };
        
        // Lock to prevent multiple retry-heavy operations from running concurrently
        let _lock = self.connectivity_lock.lock().await;
        
        // Use a direct approach without backoff::Error type conversions
        let mut retry_count = 0;
        let max_retries = 5;
        let mut last_error = None;
        
        loop {
            if retry_count >= max_retries {
                return Err(AgentError::RetryExhausted(format!(
                    "Failed to {} after {} retries: {:?}", 
                    operation_name,
                    max_retries, 
                    last_error.unwrap_or_else(|| "Unknown error".to_string())
                )));
            }
            
            match operation().await {
                Ok(result) => {
                    if retry_count > 0 {
                        debug!("Operation '{}' succeeded after {} retries", operation_name, retry_count);
                    }
                    return Ok(result);
                },
                Err(e) => {
                    // Check if this is a transient error that we should retry
                    if matches!(e, 
                        AgentError::ConnectionError(_) | 
                        AgentError::NetworkError(_) |
                        AgentError::ServerError(_) |
                        AgentError::RateLimitExceeded(_)
                    ) {
                        retry_count += 1;
                        last_error = Some(e.to_string());
                        
                        // Calculate backoff delay
                        let delay = backoff.next_backoff().unwrap_or(Duration::from_millis(500));
                        debug!("Retrying '{}' after delay: {:?} (attempt {} of {})", 
                            operation_name, delay, retry_count, max_retries);
                        
                        tokio::time::sleep(delay).await;
                        continue;
                    } else {
                        // Non-transient error, don't retry
                        return Err(e);
                    }
                }
            }
        }
    }
} 