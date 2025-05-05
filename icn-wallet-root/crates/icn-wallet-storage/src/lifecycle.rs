use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::error::StorageResult;
use crate::StorageManager;
use tracing::{debug, info, warn};

/// App lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Application is active and in the foreground
    Active,
    
    /// Application is in the background but still running
    Background,
    
    /// Application is suspended and may be terminated by the OS
    Suspended,
    
    /// Application is started or resumed from a terminated state
    Starting,
}

/// Lifecycle callback type for mobile platforms
pub type LifecycleCallback = Box<dyn Fn(AppState) -> StorageResult<()> + Send + Sync>;

/// Lifecycle-aware storage manager that responds to application state changes
pub struct LifecycleAwareStorageManager {
    /// The underlying storage manager
    storage: Arc<StorageManager>,
    
    /// Current application state
    current_state: RwLock<AppState>,
    
    /// Custom lifecycle callbacks
    lifecycle_callbacks: RwLock<Vec<LifecycleCallback>>,
    
    /// Configuration
    config: LifecycleConfig,
}

/// Configuration for lifecycle behaviors
pub struct LifecycleConfig {
    /// Whether to clear sensitive data from memory when app goes to background
    pub clear_sensitive_on_background: bool,
    
    /// Whether to store state snapshots when app goes to background
    pub snapshot_on_background: bool,
    
    /// Maximum time in seconds to keep sensitive data cached when app is in background
    pub sensitive_data_ttl_seconds: u64,
    
    /// Whether to verify the integrity of critical data when app becomes active
    pub verify_integrity_on_resume: bool,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            clear_sensitive_on_background: true,
            snapshot_on_background: true,
            sensitive_data_ttl_seconds: 300, // 5 minutes
            verify_integrity_on_resume: true,
        }
    }
}

impl LifecycleAwareStorageManager {
    /// Create a new lifecycle-aware storage manager
    pub async fn new(base_dir: impl AsRef<Path>) -> StorageResult<Self> {
        let storage = Arc::new(StorageManager::new(base_dir).await?);
        
        Ok(Self {
            storage,
            current_state: RwLock::new(AppState::Starting),
            lifecycle_callbacks: RwLock::new(Vec::new()),
            config: LifecycleConfig::default(),
        })
    }
    
    /// Create a new lifecycle-aware storage manager with custom configuration
    pub async fn with_config(base_dir: impl AsRef<Path>, config: LifecycleConfig) -> StorageResult<Self> {
        let storage = Arc::new(StorageManager::new(base_dir).await?);
        
        Ok(Self {
            storage,
            current_state: RwLock::new(AppState::Starting),
            lifecycle_callbacks: RwLock::new(Vec::new()),
            config,
        })
    }
    
    /// Get a reference to the underlying storage manager
    pub fn storage(&self) -> Arc<StorageManager> {
        self.storage.clone()
    }
    
    /// Get the current application state
    pub async fn current_state(&self) -> AppState {
        *self.current_state.read().await
    }
    
    /// Register a custom lifecycle callback
    pub async fn register_lifecycle_callback(&self, callback: LifecycleCallback) {
        let mut callbacks = self.lifecycle_callbacks.write().await;
        callbacks.push(callback);
    }
    
    /// Handle a lifecycle state change
    pub async fn handle_state_change(&self, new_state: AppState) -> StorageResult<()> {
        let old_state = {
            let mut state = self.current_state.write().await;
            let old = *state;
            *state = new_state;
            old
        };
        
        info!("App state changed: {:?} -> {:?}", old_state, new_state);
        
        // Skip if state hasn't actually changed
        if old_state == new_state {
            return Ok(());
        }
        
        // Handle the state transition
        match new_state {
            AppState::Active => {
                self.handle_becoming_active().await?;
            },
            AppState::Background => {
                self.handle_entering_background().await?;
            },
            AppState::Suspended => {
                self.handle_being_suspended().await?;
            },
            AppState::Starting => {
                self.handle_starting().await?;
            },
        }
        
        // Execute custom callbacks
        self.execute_callbacks(new_state).await?;
        
        Ok(())
    }
    
    /// Execute all registered lifecycle callbacks
    async fn execute_callbacks(&self, state: AppState) -> StorageResult<()> {
        let callbacks = self.lifecycle_callbacks.read().await;
        
        for callback in callbacks.iter() {
            if let Err(e) = callback(state) {
                warn!("Lifecycle callback error: {:?}", e);
                // Continue with other callbacks even if one fails
            }
        }
        
        Ok(())
    }
    
    /// Handle the application becoming active
    async fn handle_becoming_active(&self) -> StorageResult<()> {
        debug!("Handling app becoming active");
        
        // Verify integrity if configured
        if self.config.verify_integrity_on_resume {
            self.verify_data_integrity().await?;
        }
        
        Ok(())
    }
    
    /// Handle the application entering background
    async fn handle_entering_background(&self) -> StorageResult<()> {
        debug!("Handling app entering background");
        
        // Create state snapshot if configured
        if self.config.snapshot_on_background {
            self.create_state_snapshot().await?;
        }
        
        Ok(())
    }
    
    /// Handle the application being suspended
    async fn handle_being_suspended(&self) -> StorageResult<()> {
        debug!("Handling app being suspended");
        
        // Clear sensitive data if configured
        if self.config.clear_sensitive_on_background {
            self.clear_sensitive_data().await?;
        }
        
        Ok(())
    }
    
    /// Handle the application starting
    async fn handle_starting(&self) -> StorageResult<()> {
        debug!("Handling app starting");
        
        // Initialize any required state
        self.initialize_state().await?;
        
        Ok(())
    }
    
    /// Create a snapshot of current state
    async fn create_state_snapshot(&self) -> StorageResult<()> {
        debug!("Creating state snapshot");
        
        // Store settings snapshot
        // This would store critical app state that needs to be preserved
        let timestamp = chrono::Utc::now().timestamp();
        self.storage.store_setting("last_snapshot_time", &timestamp).await?;
        
        // Add any additional snapshot logic here
        
        Ok(())
    }
    
    /// Clear sensitive data from memory
    async fn clear_sensitive_data(&self) -> StorageResult<()> {
        debug!("Clearing sensitive data from memory");
        
        // Access the SimpleSecureStorage and clear its in-memory cache
        let secure_storage = self.storage.secure_storage();
        
        // Here we'd typically clear in-memory caches
        // This is a simulated example since we don't directly expose
        // the cache clearing functionality
        
        Ok(())
    }
    
    /// Verify the integrity of critical data
    async fn verify_data_integrity(&self) -> StorageResult<()> {
        debug!("Verifying data integrity");
        
        // Here we would check that critical DAG nodes and other
        // important data structures are intact
        
        Ok(())
    }
    
    /// Initialize the storage state
    async fn initialize_state(&self) -> StorageResult<()> {
        debug!("Initializing storage state");
        
        // Set default settings if they don't exist
        if !self.storage.has_setting("initialized").await? {
            self.storage.store_setting("initialized", &true).await?;
            self.storage.store_setting("initialization_time", &chrono::Utc::now().timestamp()).await?;
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_lifecycle_transitions() -> StorageResult<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create lifecycle-aware storage manager
        let manager = LifecycleAwareStorageManager::new(temp_dir.path()).await?;
        
        // Test initial state
        assert_eq!(manager.current_state().await, AppState::Starting);
        
        // Test state transition to active
        manager.handle_state_change(AppState::Active).await?;
        assert_eq!(manager.current_state().await, AppState::Active);
        
        // Test state transition to background
        manager.handle_state_change(AppState::Background).await?;
        assert_eq!(manager.current_state().await, AppState::Background);
        
        // Test state transition to suspended
        manager.handle_state_change(AppState::Suspended).await?;
        assert_eq!(manager.current_state().await, AppState::Suspended);
        
        // Test state transition back to active
        manager.handle_state_change(AppState::Active).await?;
        assert_eq!(manager.current_state().await, AppState::Active);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_lifecycle_callbacks() -> StorageResult<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create lifecycle-aware storage manager
        let manager = LifecycleAwareStorageManager::new(temp_dir.path()).await?;
        
        // Create a flag to check if callback was called
        let callback_called = Arc::new(tokio::sync::RwLock::new(false));
        
        // Register a callback
        let callback_flag = callback_called.clone();
        manager.register_lifecycle_callback(Box::new(move |state| {
            if state == AppState::Background {
                let mut flag = callback_flag.blocking_write();
                *flag = true;
            }
            Ok(())
        })).await;
        
        // Trigger the callback
        manager.handle_state_change(AppState::Background).await?;
        
        // Check if callback was called
        assert!(*callback_called.read().await);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_state_persistence() -> StorageResult<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create lifecycle-aware storage manager
        let manager = LifecycleAwareStorageManager::new(temp_dir.path()).await?;
        
        // Initialize
        manager.handle_state_change(AppState::Starting).await?;
        
        // Verify initialization
        let initialized: bool = manager.storage().get_setting("initialized").await?;
        assert!(initialized);
        
        // Go to background and create snapshot
        manager.handle_state_change(AppState::Background).await?;
        
        // Verify snapshot was created
        let _snapshot_time: i64 = manager.storage().get_setting("last_snapshot_time").await?;
        
        Ok(())
    }
} 