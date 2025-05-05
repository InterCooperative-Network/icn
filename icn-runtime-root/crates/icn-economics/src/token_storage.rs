use crate::{EconomicsError, EconomicsResult, ScopedResourceToken, ResourceAuthorization};
use async_trait::async_trait;
use cid::Cid;
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use uuid::Uuid;

// StorageBackend trait from icn-storage
#[async_trait]
pub trait StorageBackend {
    async fn put_kv(&mut self, key: Cid, value: Vec<u8>) -> Result<(), String>;
    async fn get_kv(&self, key: &Cid) -> Result<Option<Vec<u8>>, String>;
    async fn list_keys(&self) -> Result<Vec<Cid>, String>;
}

/// Storage key prefix for token state
const TOKEN_KEY_PREFIX: &str = "token::";
/// Storage key prefix for authorization state
const AUTH_KEY_PREFIX: &str = "auth::";

/// Helper function to create a multihash using SHA-256
fn create_sha256_multihash(data: &[u8]) -> cid::multihash::Multihash {
    // Create a new SHA-256 multihash
    let mut buf = [0u8; 32];
    let digest = Sha256::digest(data);
    buf.copy_from_slice(digest.as_slice());
    
    // Create the multihash (code 0x12 is SHA256)
    cid::multihash::Multihash::wrap(0x12, &buf[..]).expect("valid multihash")
}

/// Simple token storage trait that leverages the StorageBackend from icn-storage
#[async_trait]
pub trait TokenStorage: Send + Sync {
    /// Store a token
    async fn store_token(&mut self, token: &ScopedResourceToken) -> EconomicsResult<()>;
    
    /// Get a token by ID
    async fn get_token(&self, token_id: &Uuid) -> EconomicsResult<Option<ScopedResourceToken>>;
    
    /// Delete a token by ID (for burning)
    async fn delete_token(&mut self, token_id: &Uuid) -> EconomicsResult<()>;
    
    /// List tokens owned by a particular DID
    async fn list_tokens_by_owner(&self, owner_did: &str) -> EconomicsResult<Vec<ScopedResourceToken>>;
    
    /// List all CIDs in storage (for implementations that need it)
    async fn list_all(&self) -> EconomicsResult<Vec<Cid>> {
        // Default implementation returns an empty list
        // Concrete implementations should override this
        Ok(Vec::new())
    }
}

/// Implementation of TokenStorage that wraps a StorageBackend
#[async_trait]
impl<T: StorageBackend + Send + Sync> TokenStorage for T {
    async fn store_token(&mut self, token: &ScopedResourceToken) -> EconomicsResult<()> {
        // Generate a key CID from the token ID
        let key_str = format!("{}:{}", TOKEN_KEY_PREFIX, token.token_id);
        let hash = create_sha256_multihash(key_str.as_bytes());
        let key_cid = Cid::new_v1(0x71, hash);
        
        // Serialize the token
        let token_data = serde_json::to_vec(token)
            .map_err(|e| EconomicsError::InvalidToken(format!("Failed to serialize token: {}", e)))?;
        
        // Store the token using key-value operations
        self.put_kv(key_cid, token_data)
            .await
            .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))?;
        
        // Also store a reference in an index by owner DID
        let owner_key_str = format!("{}owner:{}:{}", TOKEN_KEY_PREFIX, token.owner_did, token.token_id);
        let owner_hash = create_sha256_multihash(owner_key_str.as_bytes());
        let owner_key_cid = Cid::new_v1(0x71, owner_hash);
        
        // Just store the token ID in the owner index
        let id_bytes = token.token_id.to_string().into_bytes();
        
        self.put_kv(owner_key_cid, id_bytes)
            .await
            .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))?;
        
        Ok(())
    }
    
    async fn get_token(&self, token_id: &Uuid) -> EconomicsResult<Option<ScopedResourceToken>> {
        // Generate the key CID from the token ID
        let key_str = format!("{}:{}", TOKEN_KEY_PREFIX, token_id);
        let hash = create_sha256_multihash(key_str.as_bytes());
        let key_cid = Cid::new_v1(0x71, hash);
        
        // Retrieve the token data
        let token_data_opt = self.get_kv(&key_cid)
            .await
            .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))?;
        
        // If token data exists, deserialize it
        match token_data_opt {
            Some(token_data) => {
                let token = serde_json::from_slice(&token_data)
                    .map_err(|e| EconomicsError::InvalidToken(format!("Failed to deserialize token: {}", e)))?;
                Ok(Some(token))
            },
            None => Ok(None),
        }
    }
    
    async fn delete_token(&mut self, token_id: &Uuid) -> EconomicsResult<()> {
        // First, get the token to find the owner info
        let token_opt = self.get_token(token_id).await?;
        
        if let Some(token) = token_opt {
            // Delete the main token entry
            let key_str = format!("{}:{}", TOKEN_KEY_PREFIX, token_id);
            let hash = create_sha256_multihash(key_str.as_bytes());
            let key_cid = Cid::new_v1(0x71, hash);
            
            // We'll just store an empty value to "delete" it
            // A real implementation might actually remove it
            self.put_kv(key_cid, vec![])
                .await
                .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))?;
            
            // Also delete from the owner index
            let owner_key_str = format!("{}owner:{}:{}", TOKEN_KEY_PREFIX, token.owner_did, token_id);
            let owner_hash = create_sha256_multihash(owner_key_str.as_bytes());
            let owner_key_cid = Cid::new_v1(0x71, owner_hash);
            
            self.put_kv(owner_key_cid, vec![])
                .await
                .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))?;
            
            Ok(())
        } else {
            // Token not found - consider it already deleted
            Ok(())
        }
    }
    
    async fn list_tokens_by_owner(&self, owner_did: &str) -> EconomicsResult<Vec<ScopedResourceToken>> {
        // In a real implementation, we would query an index or scan for tokens by owner
        // Here we'll use a simplified approach that relies on list_all() which is not efficient
        // A production implementation would use a proper indexing mechanism
        
        let all_cids = TokenStorage::list_all(self)
            .await
            .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))?;
        
        let mut tokens = Vec::new();
        
        for cid in all_cids {
            // Skip non-token entries by looking at the CID string representation
            if !cid.to_string().contains(TOKEN_KEY_PREFIX) {
                continue;
            }
            
            // Get the data for this CID
            let data_opt = self.get_kv(&cid)
                .await
                .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))?;
            
            // Skip deleted or empty entries
            if data_opt.is_none() || data_opt.as_ref().unwrap().is_empty() {
                continue;
            }
            
            // Try to deserialize as a token
            if let Some(data) = data_opt {
                if cid.to_string().contains(&format!("{}:", TOKEN_KEY_PREFIX)) {
                    if let Ok(token) = serde_json::from_slice::<ScopedResourceToken>(&data) {
                        // Check if this token is owned by the target DID
                        if token.owner_did == owner_did {
                            tokens.push(token);
                        }
                    }
                }
            }
        }
        
        Ok(tokens)
    }

    /// List all CIDs in storage
    async fn list_all(&self) -> EconomicsResult<Vec<Cid>> {
        self.list_keys()
            .await
            .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))
    }
}

/// Mock implementation of TokenStorage for testing
#[derive(Default, Debug, Clone)]
pub struct MockTokenStorage {
    /// In-memory storage for tokens
    pub tokens: HashMap<Uuid, ScopedResourceToken>,
    /// Mock cids for list_all
    pub cids: Vec<Cid>,
}

impl MockTokenStorage {
    /// Create a new empty mock storage
    pub fn new() -> Self {
        Self {
            tokens: HashMap::new(),
            cids: Vec::new(),
        }
    }
}

#[async_trait]
impl TokenStorage for MockTokenStorage {
    async fn store_token(&mut self, token: &ScopedResourceToken) -> EconomicsResult<()> {
        self.tokens.insert(token.token_id, token.clone());
        Ok(())
    }
    
    async fn get_token(&self, token_id: &Uuid) -> EconomicsResult<Option<ScopedResourceToken>> {
        Ok(self.tokens.get(token_id).cloned())
    }
    
    async fn delete_token(&mut self, token_id: &Uuid) -> EconomicsResult<()> {
        self.tokens.remove(token_id);
        Ok(())
    }
    
    async fn list_tokens_by_owner(&self, owner_did: &str) -> EconomicsResult<Vec<ScopedResourceToken>> {
        let tokens = self.tokens.values()
            .filter(|token| token.owner_did == owner_did)
            .cloned()
            .collect();
        Ok(tokens)
    }

    /// List all CIDs in storage
    async fn list_all(&self) -> EconomicsResult<Vec<Cid>> {
        // Return the mock CIDs
        Ok(self.cids.clone())
    }
}

/// Simple authorization storage trait
#[async_trait]
pub trait AuthorizationStorage: Send + Sync {
    /// Store an authorization
    async fn store_authorization(&mut self, auth: &ResourceAuthorization) -> EconomicsResult<()>;
    
    /// Get an authorization by ID
    async fn get_authorization(&self, auth_id: &Uuid) -> EconomicsResult<Option<ResourceAuthorization>>;
    
    /// Update an authorization (e.g., to consume resources)
    async fn update_authorization(&mut self, auth: &ResourceAuthorization) -> EconomicsResult<()>;
    
    /// List authorizations by grantee DID
    async fn list_authorizations_by_grantee(&self, grantee_did: &str) -> EconomicsResult<Vec<ResourceAuthorization>>;
    
    /// List all CIDs in storage (for implementations that need it)
    async fn list_all(&self) -> EconomicsResult<Vec<Cid>> {
        // Default implementation returns an empty list
        // Concrete implementations should override this
        Ok(Vec::new())
    }
}

/// Implementation of AuthorizationStorage that wraps a StorageBackend
#[async_trait]
impl<T: StorageBackend + Send + Sync> AuthorizationStorage for T {
    async fn store_authorization(&mut self, auth: &ResourceAuthorization) -> EconomicsResult<()> {
        // Generate a key CID from the authorization ID
        let key_str = format!("{}:{}", AUTH_KEY_PREFIX, auth.auth_id);
        let hash = create_sha256_multihash(key_str.as_bytes());
        let key_cid = Cid::new_v1(0x71, hash);
        
        // Serialize the authorization
        let auth_data = serde_json::to_vec(auth)
            .map_err(|e| EconomicsError::InvalidToken(format!("Failed to serialize authorization: {}", e)))?;
        
        // Store the authorization using key-value operations
        self.put_kv(key_cid, auth_data)
            .await
            .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))?;
        
        // Also store a reference in an index by grantee DID
        let grantee_key_str = format!("{}grantee:{}:{}", AUTH_KEY_PREFIX, auth.grantee_did, auth.auth_id);
        let grantee_hash = create_sha256_multihash(grantee_key_str.as_bytes());
        let grantee_key_cid = Cid::new_v1(0x71, grantee_hash);
        
        // Just store the auth ID in the grantee index
        let id_bytes = auth.auth_id.to_string().into_bytes();
        
        self.put_kv(grantee_key_cid, id_bytes)
            .await
            .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))?;
        
        Ok(())
    }
    
    async fn get_authorization(&self, auth_id: &Uuid) -> EconomicsResult<Option<ResourceAuthorization>> {
        // Generate the key CID from the auth ID
        let key_str = format!("{}:{}", AUTH_KEY_PREFIX, auth_id);
        let hash = create_sha256_multihash(key_str.as_bytes());
        let key_cid = Cid::new_v1(0x71, hash);
        
        // Retrieve the auth data
        let auth_data_opt = self.get_kv(&key_cid)
            .await
            .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))?;
        
        // If auth data exists, deserialize it
        match auth_data_opt {
            Some(auth_data) => {
                let auth = serde_json::from_slice(&auth_data)
                    .map_err(|e| EconomicsError::InvalidToken(format!("Failed to deserialize authorization: {}", e)))?;
                Ok(Some(auth))
            },
            None => Ok(None),
        }
    }
    
    async fn update_authorization(&mut self, auth: &ResourceAuthorization) -> EconomicsResult<()> {
        // Just call store_authorization as the logic is the same
        self.store_authorization(auth).await
    }
    
    async fn list_authorizations_by_grantee(&self, grantee_did: &str) -> EconomicsResult<Vec<ResourceAuthorization>> {
        // In a real implementation, we would query an index or scan for authorizations by grantee
        // Here we'll use a simplified approach that relies on list_all()
        
        let all_cids = AuthorizationStorage::list_all(self)
            .await
            .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))?;
        
        let mut authorizations = Vec::new();
        
        for cid in all_cids {
            // Skip non-auth entries
            if !cid.to_string().contains(AUTH_KEY_PREFIX) {
                continue;
            }
            
            // Get the data for this CID
            let data_opt = self.get_kv(&cid)
                .await
                .map_err(|e| EconomicsError::InvalidToken(format!("Storage error: {}", e)))?;
            
            // Skip empty entries
            if data_opt.is_none() || data_opt.as_ref().unwrap().is_empty() {
                continue;
            }
            
            // Try to deserialize as an authorization
            if let Some(data) = data_opt {
                if cid.to_string().contains(&format!("{}:", AUTH_KEY_PREFIX)) {
                    if let Ok(auth) = serde_json::from_slice::<ResourceAuthorization>(&data) {
                        // Check if this auth is for the target grantee
                        if auth.grantee_did == grantee_did {
                            authorizations.push(auth);
                        }
                    }
                }
            }
        }
        
        Ok(authorizations)
    }

    /// List all CIDs in storage
    async fn list_all(&self) -> EconomicsResult<Vec<Cid>> {
        Ok(Vec::new())
    }
}

/// MockAuthorizationStorage for testing
#[derive(Default, Debug, Clone)]
pub struct MockAuthorizationStorage {
    /// In-memory storage for authorizations
    pub authorizations: HashMap<Uuid, ResourceAuthorization>,
    /// Mock cids for list_all
    pub cids: Vec<Cid>,
}

impl MockAuthorizationStorage {
    /// Create a new empty mock storage
    pub fn new() -> Self {
        Self {
            authorizations: HashMap::new(),
            cids: Vec::new(),
        }
    }
}

#[async_trait]
impl AuthorizationStorage for MockAuthorizationStorage {
    async fn store_authorization(&mut self, auth: &ResourceAuthorization) -> EconomicsResult<()> {
        self.authorizations.insert(auth.auth_id, auth.clone());
        Ok(())
    }
    
    async fn get_authorization(&self, auth_id: &Uuid) -> EconomicsResult<Option<ResourceAuthorization>> {
        Ok(self.authorizations.get(auth_id).cloned())
    }
    
    async fn update_authorization(&mut self, auth: &ResourceAuthorization) -> EconomicsResult<()> {
        self.authorizations.insert(auth.auth_id, auth.clone());
        Ok(())
    }
    
    async fn list_authorizations_by_grantee(&self, grantee_did: &str) -> EconomicsResult<Vec<ResourceAuthorization>> {
        let auths = self.authorizations.values()
            .filter(|auth| auth.grantee_did == grantee_did)
            .cloned()
            .collect();
        Ok(auths)
    }

    /// List all CIDs in storage
    async fn list_all(&self) -> EconomicsResult<Vec<Cid>> {
        // Return the mock CIDs
        Ok(self.cids.clone())
    }
}

/// Combined storage trait for convenience
#[async_trait]
pub trait EconomicsStorage: TokenStorage + AuthorizationStorage + Send + Sync {}

/// Implement the combined trait for anything that implements both base traits
impl<T: TokenStorage + AuthorizationStorage + Send + Sync> EconomicsStorage for T {}

/// Combined mock implementation for testing
#[derive(Default, Debug, Clone)]
pub struct MockEconomicsStorage {
    /// Token storage implementation
    pub token_storage: MockTokenStorage,
    /// Authorization storage implementation
    pub auth_storage: MockAuthorizationStorage,
}

impl MockEconomicsStorage {
    /// Create a new mock storage
    pub fn new() -> Self {
        Self {
            token_storage: MockTokenStorage::new(),
            auth_storage: MockAuthorizationStorage::new(),
        }
    }
}

#[async_trait]
impl TokenStorage for MockEconomicsStorage {
    async fn store_token(&mut self, token: &ScopedResourceToken) -> EconomicsResult<()> {
        self.token_storage.store_token(token).await
    }
    
    async fn get_token(&self, token_id: &Uuid) -> EconomicsResult<Option<ScopedResourceToken>> {
        self.token_storage.get_token(token_id).await
    }
    
    async fn delete_token(&mut self, token_id: &Uuid) -> EconomicsResult<()> {
        self.token_storage.delete_token(token_id).await
    }
    
    async fn list_tokens_by_owner(&self, owner_did: &str) -> EconomicsResult<Vec<ScopedResourceToken>> {
        self.token_storage.list_tokens_by_owner(owner_did).await
    }

    /// List all CIDs in storage
    async fn list_all(&self) -> EconomicsResult<Vec<Cid>> {
        TokenStorage::list_all(&self.token_storage).await
    }
}

#[async_trait]
impl AuthorizationStorage for MockEconomicsStorage {
    async fn store_authorization(&mut self, auth: &ResourceAuthorization) -> EconomicsResult<()> {
        self.auth_storage.store_authorization(auth).await
    }
    
    async fn get_authorization(&self, auth_id: &Uuid) -> EconomicsResult<Option<ResourceAuthorization>> {
        self.auth_storage.get_authorization(auth_id).await
    }
    
    async fn update_authorization(&mut self, auth: &ResourceAuthorization) -> EconomicsResult<()> {
        self.auth_storage.update_authorization(auth).await
    }
    
    async fn list_authorizations_by_grantee(&self, grantee_did: &str) -> EconomicsResult<Vec<ResourceAuthorization>> {
        self.auth_storage.list_authorizations_by_grantee(grantee_did).await
    }

    /// List all CIDs in storage
    async fn list_all(&self) -> EconomicsResult<Vec<Cid>> {
        AuthorizationStorage::list_all(&self.auth_storage).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ResourceType, ScopedResourceToken};
    use icn_identity::IdentityScope;
    
    #[tokio::test]
    async fn test_mock_token_storage() {
        // Create mock storage
        let mut storage = MockTokenStorage::new();
        
        // Create a test token
        let token = ScopedResourceToken::new(
            "did:icn:alice".to_string(),
            ResourceType::Compute,
            100,
            IdentityScope::Individual,
            None,
            chrono::Utc::now().timestamp(),
        );
        
        // Store the token
        storage.store_token(&token).await.unwrap();
        
        // Retrieve the token
        let retrieved = storage.get_token(&token.token_id).await.unwrap().unwrap();
        assert_eq!(retrieved.token_id, token.token_id);
        assert_eq!(retrieved.owner_did, "did:icn:alice");
        
        // List tokens by owner
        let tokens = storage.list_tokens_by_owner("did:icn:alice").await.unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_id, token.token_id);
        
        // Delete the token
        storage.delete_token(&token.token_id).await.unwrap();
        
        // Verify it's deleted
        let deleted = storage.get_token(&token.token_id).await.unwrap();
        assert!(deleted.is_none());
    }
} 