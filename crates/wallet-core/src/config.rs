use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use std::fs;
use std::io;
use crate::error::{WalletResult, WalletError};

/// Wallet configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    /// Sync configuration
    #[serde(default)]
    pub sync: SyncConfig,
    /// Storage configuration
    #[serde(default)]
    pub storage: StorageConfig,
    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// Sync configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Federation node URLs to connect to
    #[serde(default = "default_federation_urls")]
    pub federation_urls: Vec<String>,
    /// Sync interval in seconds
    #[serde(default = "default_sync_interval")]
    pub sync_interval_seconds: u64,
    /// HTTP request timeout in seconds
    #[serde(default = "default_request_timeout")]
    pub request_timeout_seconds: u64,
    /// Maximum number of retry attempts for HTTP requests
    #[serde(default = "default_max_retries")]
    pub max_retry_attempts: u32,
    /// Whether to auto-sync on startup
    #[serde(default = "default_true")]
    pub auto_sync_on_startup: bool,
    /// Whether to auto-sync periodically
    #[serde(default = "default_true")]
    pub auto_sync_periodic: bool,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Base path for wallet data
    #[serde(default = "default_base_path")]
    pub base_path: PathBuf,
    /// Path for sync data
    #[serde(default = "default_sync_path")]
    pub sync_state_path: PathBuf,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Whether to log to file
    #[serde(default = "default_false")]
    pub log_to_file: bool,
    /// Log file path
    #[serde(default = "default_log_file")]
    pub log_file: PathBuf,
}

// Default functions for SyncConfig
fn default_federation_urls() -> Vec<String> {
    vec!["http://localhost:8080".to_string()]
}

fn default_sync_interval() -> u64 {
    3600 // 1 hour
}

fn default_request_timeout() -> u64 {
    30 // 30 seconds
}

fn default_max_retries() -> u32 {
    3
}

// Default functions for StorageConfig
fn default_base_path() -> PathBuf {
    PathBuf::from("./storage")
}

fn default_sync_path() -> PathBuf {
    PathBuf::from("./storage/sync")
}

// Default functions for LoggingConfig
fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_file() -> PathBuf {
    PathBuf::from("./logs/wallet.log")
}

// Common default functions
fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            sync: SyncConfig::default(),
            storage: StorageConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            federation_urls: default_federation_urls(),
            sync_interval_seconds: default_sync_interval(),
            request_timeout_seconds: default_request_timeout(),
            max_retry_attempts: default_max_retries(),
            auto_sync_on_startup: default_true(),
            auto_sync_periodic: default_true(),
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            base_path: default_base_path(),
            sync_state_path: default_sync_path(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            log_to_file: default_false(),
            log_file: default_log_file(),
        }
    }
}

impl WalletConfig {
    /// Load configuration from a TOML file
    pub fn load<P: AsRef<Path>>(path: P) -> WalletResult<Self> {
        let content = fs::read_to_string(path)
            .map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    // If file is not found, return default config
                    return WalletError::StoreError("Config file not found, using defaults".to_string());
                }
                WalletError::StoreError(format!("Failed to read config file: {}", e))
            })?;
            
        let config: WalletConfig = toml::from_str(&content)
            .map_err(|e| WalletError::SerializationError(format!("Failed to parse config file: {}", e)))?;
            
        Ok(config)
    }
    
    /// Save configuration to a TOML file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> WalletResult<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| WalletError::SerializationError(format!("Failed to serialize config: {}", e)))?;
            
        // Ensure the directory exists
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)
                .map_err(|e| WalletError::StoreError(format!("Failed to create config directory: {}", e)))?;
        }
        
        fs::write(path, content)
            .map_err(|e| WalletError::StoreError(format!("Failed to write config file: {}", e)))?;
            
        Ok(())
    }
    
    /// Load or create default configuration
    pub fn load_or_create<P: AsRef<Path>>(path: P) -> WalletResult<Self> {
        match Self::load(&path) {
            Ok(config) => Ok(config),
            Err(WalletError::StoreError(ref msg)) if msg.contains("not found") => {
                // Create default config and save it
                let config = Self::default();
                config.save(&path)?;
                Ok(config)
            },
            Err(e) => Err(e),
        }
    }
} 