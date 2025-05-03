use std::path::{Path, PathBuf};
use std::fs;
use std::io;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use tokio::fs::{create_dir_all, read_to_string, write};
use crate::error::{WalletResult, WalletError};
use crate::identity::IdentityWallet;
use crate::vc::VerifiableCredential;
use crate::dag::{DagNode, DagThread};
use crate::crypto::KeyPair;
use super::LocalWalletStore;

/// File-based implementation of the LocalWalletStore trait
#[derive(Clone)]
pub struct FileStore {
    base_path: PathBuf,
}

impl FileStore {
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }
    
    async fn ensure_dir(&self, dir: &str) -> WalletResult<PathBuf> {
        let path = self.base_path.join(dir);
        create_dir_all(&path).await
            .map_err(|e| WalletError::StoreError(format!("Failed to create directory: {}", e)))?;
        Ok(path)
    }
    
    async fn save_json<T: Serialize>(&self, dir: &str, id: &str, data: &T) -> WalletResult<()> {
        let dir_path = self.ensure_dir(dir).await?;
        let file_path = dir_path.join(format!("{}.json", id));
        
        let json = serde_json::to_string_pretty(data)
            .map_err(|e| WalletError::SerializationError(format!("Failed to serialize data: {}", e)))?;
            
        write(&file_path, json).await
            .map_err(|e| WalletError::StoreError(format!("Failed to write file: {}", e)))?;
            
        Ok(())
    }
    
    async fn load_json<T: for<'de> Deserialize<'de>>(&self, dir: &str, id: &str) -> WalletResult<T> {
        let dir_path = self.base_path.join(dir);
        let file_path = dir_path.join(format!("{}.json", id));
        
        let json = read_to_string(&file_path).await
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => WalletError::NotFound(format!("Item not found: {}/{}", dir, id)),
                _ => WalletError::StoreError(format!("Failed to read file: {}", e)),
            })?;
            
        serde_json::from_str(&json)
            .map_err(|e| WalletError::SerializationError(format!("Failed to deserialize data: {}", e)))
    }
    
    async fn list_items(&self, dir: &str) -> WalletResult<Vec<String>> {
        let dir_path = self.base_path.join(dir);
        
        if !dir_path.exists() {
            return Ok(Vec::new());
        }
        
        let mut entries = tokio::fs::read_dir(dir_path).await
            .map_err(|e| WalletError::StoreError(format!("Failed to read directory: {}", e)))?;
            
        let mut items = Vec::new();
        
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_file() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if let Some(id) = file_name.strip_suffix(".json") {
                            items.push(id.to_string());
                        }
                    }
                }
            }
        }
        
        Ok(items)
    }
}

#[async_trait]
impl LocalWalletStore for FileStore {
    async fn init(&self) -> WalletResult<()> {
        for dir in &["identities", "credentials", "dag/nodes", "dag/threads", "keys"] {
            self.ensure_dir(dir).await?;
        }
        Ok(())
    }
    
    async fn save_identity(&self, identity: &IdentityWallet) -> WalletResult<()> {
        let did = identity.did.to_string();
        self.save_json("identities", &did, identity).await
    }
    
    async fn load_identity(&self, did: &str) -> WalletResult<IdentityWallet> {
        self.load_json("identities", did).await
    }
    
    async fn list_identities(&self) -> WalletResult<Vec<String>> {
        self.list_items("identities").await
    }
    
    async fn save_credential(&self, credential: &VerifiableCredential, id: &str) -> WalletResult<()> {
        self.save_json("credentials", id, credential).await
    }
    
    async fn load_credential(&self, id: &str) -> WalletResult<VerifiableCredential> {
        self.load_json("credentials", id).await
    }
    
    async fn list_credentials(&self) -> WalletResult<Vec<String>> {
        self.list_items("credentials").await
    }
    
    async fn save_dag_node(&self, cid: &str, node: &DagNode) -> WalletResult<()> {
        self.save_json("dag/nodes", cid, node).await
    }
    
    async fn load_dag_node(&self, cid: &str) -> WalletResult<DagNode> {
        self.load_json("dag/nodes", cid).await
    }
    
    async fn save_dag_thread(&self, thread_id: &str, thread: &DagThread) -> WalletResult<()> {
        self.save_json("dag/threads", thread_id, thread).await
    }
    
    async fn load_dag_thread(&self, thread_id: &str) -> WalletResult<DagThread> {
        self.load_json("dag/threads", thread_id).await
    }
    
    async fn list_dag_threads(&self) -> WalletResult<Vec<String>> {
        self.list_items("dag/threads").await
    }
    
    async fn store_keypair(&self, id: &str, keypair: &KeyPair) -> WalletResult<()> {
        self.save_json("keys", id, keypair).await
    }
    
    async fn load_keypair(&self, id: &str) -> WalletResult<KeyPair> {
        self.load_json("keys", id).await
    }
    
    async fn delete_keypair(&self, id: &str) -> WalletResult<()> {
        let dir_path = self.base_path.join("keys");
        let file_path = dir_path.join(format!("{}.json", id));
        
        if !file_path.exists() {
            return Err(WalletError::NotFound(format!("Keypair not found: {}", id)));
        }
        
        tokio::fs::remove_file(file_path).await
            .map_err(|e| WalletError::StoreError(format!("Failed to delete keypair: {}", e)))?;
            
        Ok(())
    }
    
    async fn has_keypair(&self, id: &str) -> WalletResult<bool> {
        let dir_path = self.base_path.join("keys");
        let file_path = dir_path.join(format!("{}.json", id));
        
        Ok(file_path.exists())
    }
    
    async fn list_keypairs(&self) -> WalletResult<Vec<String>> {
        self.list_items("keys").await
    }
} 