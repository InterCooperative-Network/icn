use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cid::Cid;
use multihash::{Code, MultihashDigest};
use icn_storage::{StorageError, StorageResult, DistributedStorage};
use async_trait::async_trait;

/// Simple in-memory blob store implementation for test and development use.
pub struct InMemoryBlobStore {
    /// Maximum size in bytes
    max_size: Option<usize>,
    /// Current size in bytes
    current_size: Mutex<usize>,
    /// Storage mapping from CID to blob data
    blobs: Mutex<HashMap<Cid, Vec<u8>>>,
}

impl InMemoryBlobStore {
    /// Create a new empty blob store with no size limit
    pub fn new() -> Self {
        Self {
            max_size: None,
            current_size: Mutex::new(0),
            blobs: Mutex::new(HashMap::new()),
        }
    }

    /// Create a new empty blob store with a size limit
    pub fn with_max_size(max_bytes: usize) -> Self {
        Self {
            max_size: Some(max_bytes),
            current_size: Mutex::new(0),
            blobs: Mutex::new(HashMap::new()),
        }
    }

    /// Store a blob and return its CID
    pub fn put(&self, data: &[u8]) -> Result<Cid, String> {
        // Calculate CID for the blob
        let hash = Code::Sha2_256.digest(data);
        let cid = Cid::new_v0(hash).map_err(|e| format!("Failed to create CID: {}", e))?;
        
        // Check if we already have this blob
        if self.blobs.lock().unwrap().contains_key(&cid) {
            return Ok(cid);
        }
        
        // Check size limit if applicable
        if let Some(max_size) = self.max_size {
            let mut current_size = self.current_size.lock().unwrap();
            let new_size = *current_size + data.len();
            
            if new_size > max_size {
                return Err(format!("Blob store size limit exceeded: {} > {}", new_size, max_size));
            }
            
            // Update size
            *current_size = new_size;
        }
        
        // Store the blob
        self.blobs.lock().unwrap().insert(cid, data.to_vec());
        
        Ok(cid)
    }

    /// Retrieve a blob by its CID
    pub fn get(&self, cid: &Cid) -> Option<Vec<u8>> {
        self.blobs.lock().unwrap().get(cid).cloned()
    }

    /// Check if a blob exists
    pub fn contains(&self, cid: &Cid) -> bool {
        self.blobs.lock().unwrap().contains_key(cid)
    }

    /// Get the current size of all blobs in bytes
    pub fn size(&self) -> usize {
        *self.current_size.lock().unwrap()
    }

    /// Remove a blob by its CID and return its size
    pub fn remove(&self, cid: &Cid) -> Option<usize> {
        let mut blobs = self.blobs.lock().unwrap();
        
        if let Some(data) = blobs.remove(cid) {
            let size = data.len();
            
            // Update current size
            let mut current_size = self.current_size.lock().unwrap();
            *current_size = current_size.saturating_sub(size);
            
            Some(size)
        } else {
            None
        }
    }

    /// Clear all blobs
    pub fn clear(&self) {
        self.blobs.lock().unwrap().clear();
        *self.current_size.lock().unwrap() = 0;
    }
}

#[async_trait]
impl DistributedStorage for InMemoryBlobStore {
    async fn put_blob(&self, content: &[u8]) -> StorageResult<Cid> {
        // Check blob size if there's a limit
        if let Some(max_size) = self.max_size {
            if content.len() > max_size {
                return Err(StorageError::BlobTooLarge(
                    content.len() as u64,
                    max_size as u64,
                ));
            }
        }
        
        // Hash the content with SHA-256
        let mh = Code::Sha2_256.digest(content);
        
        // Create CID v0 with the digest
        let cid = Cid::new_v0(mh)
            .map_err(|e| StorageError::InvalidCid(e.to_string()))?;
        
        // Store the blob
        let mut blobs = self.blobs.lock().unwrap();
        let mut current_size = self.current_size.lock().unwrap();
        
        if !blobs.contains_key(&cid) {
            *current_size += content.len();
            blobs.insert(cid, content.to_vec());
        }
        
        Ok(cid)
    }
    
    async fn get_blob(&self, cid: &Cid) -> StorageResult<Option<Vec<u8>>> {
        let blobs = self.blobs.lock().unwrap();
        Ok(blobs.get(cid).cloned())
    }
    
    async fn blob_exists(&self, cid: &Cid) -> StorageResult<bool> {
        let blobs = self.blobs.lock().unwrap();
        Ok(blobs.contains_key(cid))
    }
    
    async fn blob_size(&self, cid: &Cid) -> StorageResult<Option<u64>> {
        let blobs = self.blobs.lock().unwrap();
        Ok(blobs.get(cid).map(|blob| blob.len() as u64))
    }
    
    async fn is_pinned(&self, cid: &Cid) -> StorageResult<bool> {
        // In this simple implementation, all blobs are considered "pinned"
        let blobs = self.blobs.lock().unwrap();
        Ok(blobs.contains_key(cid))
    }
    
    async fn pin_blob(&self, cid: &Cid) -> StorageResult<()> {
        // Check if it exists
        let blobs = self.blobs.lock().unwrap();
        if !blobs.contains_key(cid) {
            return Err(StorageError::BlobNotFound(cid.to_string()));
        }
        // All blobs are already pinned in this implementation
        Ok(())
    }
    
    async fn unpin_blob(&self, _cid: &Cid) -> StorageResult<()> {
        // In this simple implementation, we can't unpin
        Ok(())
    }
} 