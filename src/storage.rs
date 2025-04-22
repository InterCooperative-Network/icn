use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use directories::ProjectDirs;
use flate2::write::GzEncoder;
use flate2::read::GzDecoder;
use flate2::Compression;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};
use base64;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Failed to create directory: {0}")]
    DirectoryCreation(String),
    
    #[error("Failed to read file: {0}")]
    FileRead(String),
    
    #[error("Failed to write file: {0}")]
    FileWrite(String),
    
    #[error("Failed to serialize data: {0}")]
    Serialization(String),
    
    #[error("Failed to deserialize data: {0}")]
    Deserialization(String),
    
    #[error("Failed to get data directory")]
    DataDirNotAvailable,
    
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    #[error("I/O error: {0}")]
    IoError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Deserialization error: {0}")]
    DeserializationError(String),
    
    #[error("Storage directory not found")]
    DirectoryNotFound,
    
    #[error("Data not found for scope: {0}, key: {1}")]
    DataNotFound(String, String),
    
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    
    #[error("Decryption error: {0}")]
    DecryptionError(String),
}

// StorageType defines how data is stored
#[derive(Debug, Clone, Copy)]
pub enum StorageType {
    // Standard file-based storage
    File,
    // TODO: Implement encrypted storage in future versions
    // Encrypted,
}

// Storage manager handles saving and loading data
#[derive(Clone)]
pub struct StorageManager {
    base_dir: PathBuf,
    storage_type: StorageType,
}

impl StorageManager {
    pub fn new(storage_type: StorageType) -> Result<Self, StorageError> {
        let proj_dirs = ProjectDirs::from("org", "icn", "icn-wallet")
            .ok_or(StorageError::DataDirNotAvailable)?;
        
        let base_dir = proj_dirs.data_dir().to_path_buf();
        
        // Create base directory if it doesn't exist
        fs::create_dir_all(&base_dir).map_err(|e| {
            StorageError::DirectoryCreation(format!("Could not create data directory: {}", e))
        })?;
        
        Ok(Self {
            base_dir,
            storage_type,
        })
    }
    
    // Create a custom storage manager with a specific base directory (useful for testing)
    pub fn with_base_dir(base_dir: PathBuf, storage_type: StorageType) -> Result<Self, StorageError> {
        fs::create_dir_all(&base_dir).map_err(|e| {
            StorageError::DirectoryCreation(format!("Could not create data directory: {}", e))
        })?;
        
        Ok(Self {
            base_dir,
            storage_type,
        })
    }
    
    // Get the scope directory path
    pub fn scope_dir(&self, scope: &str) -> PathBuf {
        let mut dir = self.base_dir.clone();
        dir.push(scope);
        dir
    }
    
    // Ensure a scope directory exists
    pub fn ensure_scope_dir(&self, scope: &str) -> Result<PathBuf, StorageError> {
        let dir = self.scope_dir(scope);
        fs::create_dir_all(&dir).map_err(|e| {
            StorageError::DirectoryCreation(format!("Could not create scope directory: {}", e))
        })?;
        Ok(dir)
    }
    
    // Save data to storage
    pub fn save<T: Serialize>(&self, scope: &str, name: &str, data: &T) -> Result<(), StorageError> {
        let scope_dir = self.ensure_scope_dir(scope)?;
        let file_path = scope_dir.join(format!("{}.json", name));
        
        match self.storage_type {
            StorageType::File => self.save_to_file(&file_path, data),
        }
    }
    
    // Load data from storage
    pub fn load<T: DeserializeOwned>(&self, scope: &str, name: &str) -> Result<T, StorageError> {
        let scope_dir = self.scope_dir(scope);
        let file_path = scope_dir.join(format!("{}.json", name));
        
        if !file_path.exists() {
            return Err(StorageError::NotFound(format!("File {} not found", file_path.display())));
        }
        
        match self.storage_type {
            StorageType::File => self.load_from_file(&file_path),
        }
    }
    
    // Check if a data file exists
    pub fn exists(&self, scope: &str, name: &str) -> bool {
        let scope_dir = self.scope_dir(scope);
        let file_path = scope_dir.join(format!("{}.json", name));
        file_path.exists()
    }
    
    // Delete a data file
    pub fn delete(&self, scope: &str, name: &str) -> Result<(), StorageError> {
        let scope_dir = self.scope_dir(scope);
        let file_path = scope_dir.join(format!("{}.json", name));
        
        if file_path.exists() {
            fs::remove_file(&file_path).map_err(|e| {
                StorageError::FileWrite(format!("Could not delete file: {}", e))
            })?;
        }
        
        Ok(())
    }
    
    // List all files in a scope
    pub fn list_files(&self, scope: &str) -> Result<Vec<String>, StorageError> {
        let scope_dir = self.scope_dir(scope);
        
        if !scope_dir.exists() {
            return Ok(Vec::new());
        }
        
        let entries = fs::read_dir(&scope_dir).map_err(|e| {
            StorageError::FileRead(format!("Could not read directory: {}", e))
        })?;
        
        let mut files = Vec::new();
        
        for entry in entries {
            let entry = entry.map_err(|e| {
                StorageError::FileRead(format!("Could not read directory entry: {}", e))
            })?;
            
            let path = entry.path();
            
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                if let Some(filename) = path.file_stem() {
                    if let Some(filename_str) = filename.to_str() {
                        files.push(filename_str.to_string());
                    }
                }
            }
        }
        
        Ok(files)
    }
    
    // List all scopes
    pub fn list_scopes(&self) -> Result<Vec<String>, StorageError> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }
        
        let entries = fs::read_dir(&self.base_dir).map_err(|e| {
            StorageError::FileRead(format!("Could not read directory: {}", e))
        })?;
        
        let mut scopes = Vec::new();
        
        for entry in entries {
            let entry = entry.map_err(|e| {
                StorageError::FileRead(format!("Could not read directory entry: {}", e))
            })?;
            
            let path = entry.path();
            
            if path.is_dir() {
                if let Some(dirname) = path.file_name() {
                    if let Some(dirname_str) = dirname.to_str() {
                        scopes.push(dirname_str.to_string());
                    }
                }
            }
        }
        
        Ok(scopes)
    }
    
    // Private helper methods
    fn save_to_file<T: Serialize>(&self, path: &Path, data: &T) -> Result<(), StorageError> {
        let json = serde_json::to_string_pretty(data).map_err(|e| {
            StorageError::Serialization(format!("Could not serialize data: {}", e))
        })?;
        
        let mut file = File::create(path).map_err(|e| {
            StorageError::FileWrite(format!("Could not create file: {}", e))
        })?;
        
        file.write_all(json.as_bytes()).map_err(|e| {
            StorageError::FileWrite(format!("Could not write to file: {}", e))
        })?;
        
        Ok(())
    }
    
    fn load_from_file<T: DeserializeOwned>(&self, path: &Path) -> Result<T, StorageError> {
        let mut file = File::open(path).map_err(|e| {
            StorageError::FileRead(format!("Could not open file: {}", e))
        })?;
        
        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(|e| {
            StorageError::FileRead(format!("Could not read file: {}", e))
        })?;
        
        serde_json::from_str(&contents).map_err(|e| {
            StorageError::Deserialization(format!("Could not deserialize data: {}", e))
        })
    }
    
    /// Create a backup of all wallet data
    pub fn create_backup(&self, output: &Path, password: Option<&str>) -> Result<(), StorageError> {
        // Get the base directory
        let base_dir = &self.base_dir;
        
        // Create a temporary tar archive
        let tar_path = tempfile::NamedTempFile::new()
            .map_err(|e| StorageError::IoError(e.to_string()))?;
        
        // Create a tar builder
        let file = File::create(tar_path.path())
            .map_err(|e| StorageError::IoError(e.to_string()))?;
        
        // Create a compressed writer
        let enc = GzEncoder::new(file, Compression::default());
        let mut tar = tar::Builder::new(enc);
        
        // Add all files in the directory to the archive
        self.add_dir_to_tar(&mut tar, base_dir, base_dir)
            .map_err(|e| StorageError::IoError(e.to_string()))?;
        
        // Finalize the tar archive
        tar.finish().map_err(|e| StorageError::IoError(e.to_string()))?;
        
        // If password is provided, encrypt the backup
        if let Some(pass) = password {
            // Read the tar file
            let mut tar_contents = Vec::new();
            let mut reader = File::open(tar_path.path())
                .map_err(|e| StorageError::IoError(e.to_string()))?;
            reader.read_to_end(&mut tar_contents)
                .map_err(|e| StorageError::IoError(e.to_string()))?;
            
            // Encrypt the contents
            let encrypted = self.encrypt_data(&tar_contents, pass)
                .map_err(|e| StorageError::EncryptionError(e.to_string()))?;
            
            // Write the encrypted data to the output file
            fs::write(output, encrypted)
                .map_err(|e| StorageError::IoError(e.to_string()))?;
        } else {
            // Just copy the tar file to the output
            fs::copy(tar_path.path(), output)
                .map_err(|e| StorageError::IoError(e.to_string()))?;
        }
        
        Ok(())
    }
    
    /// Restore wallet from a backup
    pub fn restore_backup(&self, input: &Path, password: Option<&str>) -> Result<(), StorageError> {
        // Get the base directory
        let base_dir = &self.base_dir;
        
        // Create a temporary file for the tar archive
        let tar_path = tempfile::NamedTempFile::new()
            .map_err(|e| StorageError::IoError(e.to_string()))?;
        
        // Read the input file
        let input_data = fs::read(input)
            .map_err(|e| StorageError::IoError(e.to_string()))?;
        
        // If password is provided, decrypt the backup
        if let Some(pass) = password {
            // Decrypt the contents
            let decrypted = self.decrypt_data(&input_data, pass)
                .map_err(|e| StorageError::DecryptionError(e.to_string()))?;
            
            // Write the decrypted data to the temporary file
            fs::write(tar_path.path(), decrypted)
                .map_err(|e| StorageError::IoError(e.to_string()))?;
        } else {
            // Just copy the input file to the temporary file
            fs::copy(input, tar_path.path())
                .map_err(|e| StorageError::IoError(e.to_string()))?;
        }
        
        // Open the tar archive
        let file = File::open(tar_path.path())
            .map_err(|e| StorageError::IoError(e.to_string()))?;
        
        // Create a compressed reader
        let dec = GzDecoder::new(file);
        let mut archive = tar::Archive::new(dec);
        
        // Extract all files to the base directory
        archive.unpack(base_dir)
            .map_err(|e| StorageError::IoError(e.to_string()))?;
        
        Ok(())
    }
    
    // Helper function to add a directory to a tar archive
    fn add_dir_to_tar<W: Write>(
        &self,
        tar: &mut tar::Builder<W>,
        base_path: &Path,
        dir_path: &Path,
    ) -> io::Result<()> {
        for entry in fs::read_dir(dir_path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                // Recursively add subdirectories
                self.add_dir_to_tar(tar, base_path, &path)?;
            } else {
                // Add file to the archive with relative path
                let rel_path = path.strip_prefix(base_path).unwrap_or(&path);
                let mut file = File::open(&path)?;
                tar.append_file(rel_path, &mut file)?;
            }
        }
        
        Ok(())
    }
    
    // Encrypt data with a password
    fn encrypt_data(&self, data: &[u8], password: &str) -> Result<Vec<u8>, String> {
        // Derive a key from the password using PBKDF2
        let salt = self.generate_random_bytes(16)?;
        let key = self.derive_key(password, &salt)?;
        
        // Create a nonce
        let nonce_bytes = self.generate_random_bytes(12)?;
        let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
            .map_err(|_| "Failed to create nonce".to_string())?;
        
        // Create an encryption key
        let unbound_key = UnboundKey::new(&AES_256_GCM, &key)
            .map_err(|_| "Failed to create encryption key".to_string())?;
        let less_safe_key = LessSafeKey::new(unbound_key);
        
        // Encrypt the data
        let mut buffer = data.to_vec();
        less_safe_key.seal_in_place_append_tag(nonce, Aad::empty(), &mut buffer)
            .map_err(|_| "Encryption failed".to_string())?;
        
        // Format: salt (16 bytes) + nonce (12 bytes) + encrypted data
        let mut result = Vec::with_capacity(salt.len() + nonce_bytes.len() + buffer.len());
        result.extend_from_slice(&salt);
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&buffer);
        
        Ok(result)
    }
    
    // Decrypt data with a password
    fn decrypt_data(&self, data: &[u8], password: &str) -> Result<Vec<u8>, String> {
        if data.len() < 28 {  // 16 (salt) + 12 (nonce)
            return Err("Encrypted data is too short".to_string());
        }
        
        // Extract salt, nonce, and encrypted data
        let salt = &data[0..16];
        let nonce_bytes = &data[16..28];
        let encrypted_data = &data[28..];
        
        // Derive the key from the password
        let key = self.derive_key(password, salt)?;
        
        // Create nonce
        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| "Invalid nonce".to_string())?;
        
        // Create decryption key
        let unbound_key = UnboundKey::new(&AES_256_GCM, &key)
            .map_err(|_| "Failed to create decryption key".to_string())?;
        let less_safe_key = LessSafeKey::new(unbound_key);
        
        // Decrypt the data
        let mut buffer = encrypted_data.to_vec();
        let result = less_safe_key.open_in_place(nonce, Aad::empty(), &mut buffer)
            .map_err(|_| "Decryption failed - incorrect password?".to_string())?;
        
        Ok(result.to_vec())
    }
    
    // Generate random bytes
    fn generate_random_bytes(&self, size: usize) -> Result<Vec<u8>, String> {
        let mut bytes = vec![0u8; size];
        let rng = SystemRandom::new();
        rng.fill(&mut bytes)
            .map_err(|_| "Failed to generate random bytes".to_string())?;
        Ok(bytes)
    }
    
    // Derive a key from a password using PBKDF2
    fn derive_key(&self, password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
        use ring::pbkdf2;
        
        let mut key = [0u8; 32];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            std::num::NonZeroU32::new(100_000).unwrap(),
            salt,
            password.as_bytes(),
            &mut key,
        );
        
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::tempdir;
    
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestData {
        name: String,
        value: i32,
    }
    
    #[test]
    fn test_save_and_load() {
        let temp_dir = tempdir().unwrap();
        let storage = StorageManager::with_base_dir(temp_dir.path().to_path_buf(), StorageType::File).unwrap();
        
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        
        storage.save("test_scope", "test_file", &data).unwrap();
        
        let loaded: TestData = storage.load("test_scope", "test_file").unwrap();
        
        assert_eq!(data, loaded);
    }
    
    #[test]
    fn test_file_operations() {
        let temp_dir = tempdir().unwrap();
        let storage = StorageManager::with_base_dir(temp_dir.path().to_path_buf(), StorageType::File).unwrap();
        
        // Test file does not exist initially
        assert!(!storage.exists("test_scope", "test_file"));
        
        // Save a file
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        storage.save("test_scope", "test_file", &data).unwrap();
        
        // Now it should exist
        assert!(storage.exists("test_scope", "test_file"));
        
        // List files in scope
        let files = storage.list_files("test_scope").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "test_file");
        
        // List scopes
        let scopes = storage.list_scopes().unwrap();
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0], "test_scope");
        
        // Delete the file
        storage.delete("test_scope", "test_file").unwrap();
        
        // File should no longer exist
        assert!(!storage.exists("test_scope", "test_file"));
    }
} 