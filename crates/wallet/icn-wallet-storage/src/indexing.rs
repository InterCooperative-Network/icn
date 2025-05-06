use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::error::{StorageError, StorageResult};
use crate::traits::{ensure_directory, SecureStorage};
use crate::secure::SimpleSecureStorage;
use serde::{Serialize, Deserialize};
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, warn};
use rand::{rngs::OsRng, RngCore};
use sha2::{Sha256, Digest};

/// Secure encrypted index for sensitive data
pub struct SecureIndex {
    /// Base directory for storing indexes
    base_dir: PathBuf,
    
    /// Index data structure (prefix/term -> list of encrypted references)
    indices: Arc<RwLock<HashMap<String, Vec<EncryptedReference>>>>,
    
    /// Encrypted storage backend
    secure_storage: Arc<SimpleSecureStorage>,
    
    /// Encryption key for index entries
    index_key: [u8; 32],
}

/// Reference to an encrypted value
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncryptedReference {
    /// Encrypted key name (to prevent key enumeration)
    encrypted_key: String,
    
    /// Optional metadata (encrypted)
    metadata: Option<String>,
    
    /// Term score/priority for ranking results
    score: f32,
    
    /// When the reference was created/updated
    timestamp: i64,
}

/// Search result with relevance information
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Key where this result was found
    pub key: String,
    
    /// Decrypted optional metadata (if originally provided)
    pub metadata: Option<String>,
    
    /// Relevance score
    pub score: f32,
}

/// Terms extraction methods
#[derive(Debug, Clone, Copy)]
pub enum TermsExtraction {
    /// Extract terms from keys only
    KeysOnly,
    
    /// Extract terms from values only
    ValuesOnly,
    
    /// Extract terms from both keys and values
    Both,
}

impl SecureIndex {
    /// Create a new secure index
    pub async fn new(
        base_dir: impl AsRef<Path>, 
        secure_storage: Arc<SimpleSecureStorage>
    ) -> StorageResult<Self> {
        let index_dir = base_dir.as_ref().join("indices");
        ensure_directory(&index_dir).await?;
        
        // Generate a random key for index encryption
        let mut index_key = [0u8; 32];
        OsRng.fill_bytes(&mut index_key);
        
        let mut instance = Self {
            base_dir: index_dir,
            indices: Arc::new(RwLock::new(HashMap::new())),
            secure_storage,
            index_key,
        };
        
        // Load existing indices
        instance.load_indices().await?;
        
        Ok(instance)
    }
    
    /// Create a new secure index with a provided key
    pub async fn with_key(
        base_dir: impl AsRef<Path>, 
        secure_storage: Arc<SimpleSecureStorage>,
        index_key: [u8; 32]
    ) -> StorageResult<Self> {
        let index_dir = base_dir.as_ref().join("indices");
        ensure_directory(&index_dir).await?;
        
        let mut instance = Self {
            base_dir: index_dir,
            indices: Arc::new(RwLock::new(HashMap::new())),
            secure_storage,
            index_key,
        };
        
        // Load existing indices
        instance.load_indices().await?;
        
        Ok(instance)
    }
    
    /// Index path for a specific index name
    fn index_path(&self, index_name: &str) -> PathBuf {
        self.base_dir.join(format!("{}.idx", index_name))
    }
    
    /// Load all indices from disk
    async fn load_indices(&mut self) -> StorageResult<()> {
        let mut entries = fs::read_dir(&self.base_dir).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            
            // Skip non-index files
            if !path.is_file() || path.extension().map_or(true, |ext| ext != "idx") {
                continue;
            }
            
            // Extract index name
            if let Some(stem) = path.file_stem() {
                if let Some(index_name) = stem.to_str() {
                    // Load this index
                    match self.load_index(index_name).await {
                        Ok(_) => debug!("Loaded index: {}", index_name),
                        Err(e) => warn!("Failed to load index {}: {}", index_name, e),
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Load a specific index from disk
    async fn load_index(&self, index_name: &str) -> StorageResult<()> {
        let path = self.index_path(index_name);
        
        if !path.exists() {
            return Ok(());
        }
        
        let content = fs::read_to_string(&path).await?;
        let encrypted_refs: HashMap<String, Vec<EncryptedReference>> = serde_json::from_str(&content)
            .map_err(|e| StorageError::SerializationError(format!("Failed to parse index: {}", e)))?;
            
        // Add to the indices map
        let mut indices = self.indices.write().await;
        for (term, refs) in encrypted_refs {
            let entry = indices.entry(format!("{}:{}", index_name, term)).or_insert_with(Vec::new);
            entry.extend(refs);
        }
        
        Ok(())
    }
    
    /// Save an index to disk
    async fn save_index(&self, index_name: &str) -> StorageResult<()> {
        let path = self.index_path(index_name);
        
        // Extract all entries for this index
        let prefix = format!("{}:", index_name);
        let mut index_entries: HashMap<String, Vec<EncryptedReference>> = HashMap::new();
        
        let indices = self.indices.read().await;
        for (key, refs) in indices.iter() {
            if key.starts_with(&prefix) {
                if let Some(term) = key.strip_prefix(&prefix) {
                    index_entries.insert(term.to_string(), refs.clone());
                }
            }
        }
        
        // Serialize and save
        let serialized = serde_json::to_string_pretty(&index_entries)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize index: {}", e)))?;
            
        fs::write(&path, &serialized).await?;
        debug!("Saved index: {}", index_name);
        
        Ok(())
    }
    
    /// Compute deterministic HMAC from a value
    fn hmac_value(&self, value: &str) -> String {
        // A real implementation would use a proper HMAC
        // For simplicity, we use a SHA-256 hash with our key
        let mut hasher = Sha256::new();
        hasher.update(&self.index_key);
        hasher.update(value.as_bytes());
        format!("{:x}", hasher.finalize())
    }
    
    /// Extract searchable terms from a value
    fn extract_terms(&self, text: &str, min_length: usize) -> Vec<String> {
        // Split text into terms, normalize, and filter
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|term| term.len() >= min_length)
            .map(|term| term.to_lowercase())
            .collect()
    }
    
    /// Create obscured search terms that still allow similarity matching
    /// but don't reveal the original terms
    fn create_obscured_terms(&self, terms: &[String]) -> Vec<String> {
        terms.iter()
            .map(|term| {
                // For each term, create a deterministic but obscured version
                self.hmac_value(term)
            })
            .collect()
    }
    
    /// Index a key-value pair with searchable terms
    pub async fn index_item<V: Serialize>(
        &self,
        index_name: &str,
        key: &str,
        value: &V,
        metadata: Option<&str>,
        extraction: TermsExtraction
    ) -> StorageResult<()> {
        // 1. Extract searchable terms
        let mut all_terms = Vec::new();
        
        match extraction {
            TermsExtraction::KeysOnly => {
                all_terms.extend(self.extract_terms(key, 3));
            },
            TermsExtraction::ValuesOnly => {
                // Extract terms from serialized value
                let value_str = serde_json::to_string(value)
                    .map_err(|e| StorageError::SerializationError(format!("Failed to serialize value: {}", e)))?;
                all_terms.extend(self.extract_terms(&value_str, 3));
            },
            TermsExtraction::Both => {
                all_terms.extend(self.extract_terms(key, 3));
                let value_str = serde_json::to_string(value)
                    .map_err(|e| StorageError::SerializationError(format!("Failed to serialize value: {}", e)))?;
                all_terms.extend(self.extract_terms(&value_str, 3));
            }
        }
        
        // 2. Create obscured terms for privacy
        let obscured_terms = self.create_obscured_terms(&all_terms);
        
        // 3. Create encrypted reference
        let encrypted_key = self.hmac_value(key);
        let encrypted_metadata = metadata.map(|m| self.hmac_value(m));
        let timestamp = chrono::Utc::now().timestamp();
        
        let reference = EncryptedReference {
            encrypted_key,
            metadata: encrypted_metadata,
            score: 1.0, // Default score
            timestamp,
        };
        
        // 4. Add reference to index for each term
        let mut indices = self.indices.write().await;
        
        for term in &obscured_terms {
            let index_key = format!("{}:{}", index_name, term);
            let entry = indices.entry(index_key).or_insert_with(Vec::new);
            
            // Remove any existing reference to this key
            entry.retain(|r| r.encrypted_key != reference.encrypted_key);
            
            // Add new reference
            entry.push(reference.clone());
        }
        
        // 5. Save the modified index
        drop(indices);
        self.save_index(index_name).await?;
        
        Ok(())
    }
    
    /// Store a value with the secure storage and index it
    pub async fn store_and_index<V: Serialize + Send + Sync>(
        &self,
        index_name: &str,
        key: &str,
        value: &V,
        metadata: Option<&str>,
        extraction: TermsExtraction
    ) -> StorageResult<()> {
        // First store the value securely
        self.secure_storage.store_secret(key, value).await?;
        
        // Then index it
        self.index_item(index_name, key, value, metadata, extraction).await?;
        
        debug!("Stored and indexed item: {} in {}", key, index_name);
        
        Ok(())
    }
    
    /// Search for items in an index
    pub async fn search(
        &self,
        index_name: &str,
        query: &str,
        max_results: usize
    ) -> StorageResult<Vec<SearchResult>> {
        // 1. Extract query terms
        let query_terms = self.extract_terms(query, 2);
        
        // 2. Create obscured query terms
        let obscured_terms = self.create_obscured_terms(&query_terms);
        
        // 3. Find matching references
        let indices = self.indices.read().await;
        let mut scored_results: HashMap<String, f32> = HashMap::new();
        
        for term in &obscured_terms {
            let index_key = format!("{}:{}", index_name, term);
            
            if let Some(refs) = indices.get(&index_key) {
                for reference in refs {
                    let entry = scored_results.entry(reference.encrypted_key.clone()).or_insert(0.0);
                    *entry += reference.score;
                }
            }
        }
        
        // 4. Sort results by score
        let mut results: Vec<_> = scored_results.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        // 5. Convert encrypted keys back to original keys and limit results
        let mut search_results = Vec::new();
        
        // In a real implementation, we would need a bidirectional mapping
        // Here, we'll use the all_keys method to scan all keys
        // This is inefficient but demonstrates the concept
        for (original_key, score) in results.into_iter().take(max_results) {
            // Normally we would have a mapping from encrypted_key to actual key
            // For this example, we just return the encrypted form
            search_results.push(SearchResult {
                key: original_key.clone(),
                metadata: None,
                score,
            });
        }
        
        Ok(search_results)
    }
    
    /// Remove an item from indexes
    pub async fn remove_from_index(
        &self,
        index_name: &str,
        key: &str
    ) -> StorageResult<()> {
        let encrypted_key = self.hmac_value(key);
        
        // Remove references from all terms
        let mut indices = self.indices.write().await;
        let prefix = format!("{}:", index_name);
        
        let mut modified = false;
        for (index_key, refs) in indices.iter_mut() {
            if index_key.starts_with(&prefix) {
                let len_before = refs.len();
                refs.retain(|r| r.encrypted_key != encrypted_key);
                if len_before != refs.len() {
                    modified = true;
                }
            }
        }
        
        // Save if modified
        drop(indices);
        if modified {
            self.save_index(index_name).await?;
        }
        
        Ok(())
    }
    
    /// Delete an item from secure storage and remove from indexes
    pub async fn delete_and_remove(
        &self,
        index_name: &str,
        key: &str
    ) -> StorageResult<()> {
        // Remove from secure storage
        self.secure_storage.delete_secret(key).await?;
        
        // Remove from indexes
        self.remove_from_index(index_name, key).await?;
        
        debug!("Deleted and removed item: {} from {}", key, index_name);
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use serde::{Serialize, Deserialize};
    
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestUser {
        username: String,
        email: String,
        role: String,
    }
    
    #[tokio::test]
    async fn test_index_and_search() -> StorageResult<()> {
        // Create temporary directory
        let temp_dir = tempdir().unwrap();
        
        // Create secure storage
        let secure_storage = Arc::new(SimpleSecureStorage::new(temp_dir.path()).await?);
        
        // Create secure index
        let index = SecureIndex::new(temp_dir.path(), secure_storage.clone()).await?;
        
        // Create test data
        let user1 = TestUser {
            username: "johndoe".to_string(),
            email: "john.doe@example.com".to_string(),
            role: "admin".to_string(),
        };
        
        let user2 = TestUser {
            username: "janedoe".to_string(),
            email: "jane.doe@example.com".to_string(),
            role: "user".to_string(),
        };
        
        let user3 = TestUser {
            username: "bobsmith".to_string(),
            email: "bob.smith@example.com".to_string(),
            role: "admin".to_string(),
        };
        
        // Store and index users
        index.store_and_index("users", "user:1", &user1, None, TermsExtraction::Both).await?;
        index.store_and_index("users", "user:2", &user2, None, TermsExtraction::Both).await?;
        index.store_and_index("users", "user:3", &user3, None, TermsExtraction::Both).await?;
        
        // Get the encrypted keys
        let key1 = index.hmac_value("user:1");
        let key2 = index.hmac_value("user:2");
        let key3 = index.hmac_value("user:3");
        
        // Search for admin users (note: in a real implementation, search would resolve keys)
        let admin_results = index.search("users", "admin", 10).await?;
        
        // We should find user1 and user3
        assert_eq!(admin_results.len(), 2);
        
        // Extract keys from results
        let result_keys: Vec<String> = admin_results.iter().map(|r| r.key.clone()).collect();
        assert!(result_keys.contains(&key1));
        assert!(result_keys.contains(&key3));
        
        // Search for jane
        let jane_results = index.search("users", "jane", 10).await?;
        assert_eq!(jane_results.len(), 1);
        assert_eq!(jane_results[0].key, key2);
        
        // Delete a user and verify they no longer show up in results
        index.delete_and_remove("users", "user:1").await?;
        
        let admin_results_after = index.search("users", "admin", 10).await?;
        assert_eq!(admin_results_after.len(), 1);
        assert_eq!(admin_results_after[0].key, key3);
        
        Ok(())
    }
} 