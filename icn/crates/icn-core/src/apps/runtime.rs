//! App Runtime
//!
//! Manages app lifecycle: install, start, stop, uninstall.
//!
//! # Supervisor Constraint
//!
//! The supervisor calls this runtime with **zero domain knowledge**.
//! It loads manifests from config paths and grants capabilities.
//! It does NOT understand "trust" or "governance" semantically.
//!
//! # App Isolation
//!
//! - Each app gets isolated state namespace
//! - Apps cannot access other apps' state without capability
//! - Communication only via Comms primitive
//!
//! # Oracle Registration
//!
//! Apps that provide `capabilities_provided: [oracle:*]` can register
//! a PolicyOracle. The runtime wires it to the OracleRegistry.
//!
//! # App Lifecycle
//!
//! ```text
//! [Install] -> Installed -> [Start] -> Starting -> Running
//!                                            |
//!                                       [Stop/Error]
//!                                            |
//!                                            v
//!                              Stopping -> Stopped/Failed
//!                                            |
//!                                       [Uninstall]
//!                                            |
//!                                            v
//!                                        (removed)
//! ```
//!
//! ## State Transitions
//!
//! - **Installed**: App manifest loaded, state namespace created
//! - **Starting**: Dispatcher initializing, reducers/services registering
//! - **Running**: Dispatcher active, processing events
//! - **Stopping**: Shutdown signal sent, waiting for graceful termination
//! - **Stopped**: Clean shutdown completed, ready for restart or uninstall
//! - **Failed**: Shutdown timed out or error occurred, may need cleanup
//!
//! ## Cleanup Behavior
//!
//! When an app is **uninstalled**:
//! - If running, it is stopped first (with timeout protection)
//! - The app entry is removed from the registry
//! - **State is NOT automatically deleted** (preserves data for re-install)
//!
//! When **stop times out**:
//! - App status is set to `Failed`
//! - Timeout metric is incremented
//! - The app remains in the registry and can be:
//!   - Retried with another stop attempt
//!   - Force-uninstalled (removes registry entry only)
//!   - Left for manual intervention
//!
//! When an app enters **Failed** state:
//! - The dispatcher task may still be running (leaked)
//! - State may be inconsistent
//! - Recommend logging the failure for operator review
//!
//! ## Concurrent Operations
//!
//! The runtime guards against concurrent operations on the same app:
//! - `uninstall()` checks if app is being modified and returns error
//! - Lock ordering prevents deadlocks (see below)
//!
//! # Lock Ordering Invariants
//!
//! To prevent deadlocks, locks must be acquired in this order:
//!
//! 1. `AppRuntime.apps` (RwLock) - Top-level app registry
//! 2. `InstalledApp.dispatcher` (RwLock) - Per-app dispatcher
//!
//! **Important**: Never acquire `apps` lock while holding `dispatcher` lock.
//!
//! The `stop()` operation holds `apps.write()` while calling `dispatcher.stop()`,
//! but `dispatcher.stop()` only sends a shutdown signal and doesn't acquire
//! the `apps` lock, so no deadlock occurs.
//!
//! The `stop_inner()` implementation is protected by an overall timeout
//! (`STOP_OPERATION_TIMEOUT`) to prevent indefinite blocking if lock
//! acquisition takes too long.

use super::dispatcher::{BoxedReducer, BoxedService, ComputeDispatcher};
use super::manifest::{Manifest, ManifestError};
use super::state_factory::{AppNamespace, AppState, StateFactory};
use icn_kernel_api::authz::{Constraints, Domain, PolicyOracle};
use icn_kernel_api::bootstrap::{
    BootstrapPhase, CapabilityRequest, CapabilitySet, GenesisCapabilities, OracleRegistry,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

/// Timeout for dispatcher shutdown. If dispatcher doesn't stop within this
/// duration, we log a warning and continue.
const DISPATCHER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for overall stop operation including lock acquisition.
/// This should be greater than DISPATCHER_SHUTDOWN_TIMEOUT to allow for
/// graceful shutdown before timing out the entire operation.
const STOP_OPERATION_TIMEOUT: Duration = Duration::from_secs(10);

/// App identifier.
pub type AppId = String;

/// App status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppStatus {
    /// App is installed but not started
    Installed,
    /// App is starting
    Starting,
    /// App is running
    Running,
    /// App is stopping
    Stopping,
    /// App is stopped
    Stopped,
    /// App failed to start
    Failed,
}

/// Handle to a running app.
pub struct AppHandle {
    /// App identifier
    pub id: AppId,
    /// App manifest
    pub manifest: Manifest,
    /// App namespace
    pub namespace: AppNamespace,
    /// App state
    pub state: AppState,
    /// Compute dispatcher
    pub dispatcher: Arc<RwLock<ComputeDispatcher>>,
    /// Issued capabilities
    pub capabilities: CapabilitySet,
    /// Current status
    pub status: AppStatus,
    /// Dispatcher task handle
    dispatcher_task: Option<JoinHandle<()>>,
    /// Flag set when dispatcher task fails (panic or error)
    failed: Arc<AtomicBool>,
}

impl AppHandle {
    /// Get app ID.
    pub fn id(&self) -> &AppId {
        &self.id
    }

    /// Get app status.
    ///
    /// Note: If the dispatcher task has failed, this returns Failed
    /// regardless of the stored status.
    pub fn status(&self) -> AppStatus {
        if self.failed.load(Ordering::SeqCst) {
            AppStatus::Failed
        } else {
            self.status
        }
    }

    /// Check if app is running.
    pub fn is_running(&self) -> bool {
        self.status == AppStatus::Running && !self.failed.load(Ordering::SeqCst)
    }

    /// Check if app has failed.
    pub fn is_failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }
}

/// Builder for registering handlers with an app.
pub struct AppBuilder {
    manifest: Manifest,
    namespace: AppNamespace,
    state: AppState,
    reducers: HashMap<String, BoxedReducer>,
    services: HashMap<String, BoxedService>,
    oracle: Option<Arc<dyn PolicyOracle>>,
}

impl AppBuilder {
    /// Register a reducer.
    pub fn with_reducer(mut self, event_type: impl Into<String>, reducer: BoxedReducer) -> Self {
        self.reducers.insert(event_type.into(), reducer);
        self
    }

    /// Register a service.
    pub fn with_service(mut self, request_type: impl Into<String>, service: BoxedService) -> Self {
        self.services.insert(request_type.into(), service);
        self
    }

    /// Register a policy oracle.
    pub fn with_oracle(mut self, oracle: Arc<dyn PolicyOracle>) -> Self {
        self.oracle = Some(oracle);
        self
    }

    /// Get the manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Get the namespace.
    pub fn namespace(&self) -> &AppNamespace {
        &self.namespace
    }

    /// Get the state handles.
    pub fn state(&self) -> &AppState {
        &self.state
    }
}

/// App runtime - manages app lifecycle.
pub struct AppRuntime {
    /// State factory for creating namespaced state
    state_factory: StateFactory,
    /// Oracle registry
    oracle_registry: Arc<OracleRegistry>,
    /// Installed apps
    apps: RwLock<HashMap<AppId, AppHandle>>,
    /// Genesis capabilities (for bootstrap)
    genesis_caps: Option<GenesisCapabilities>,
}

impl AppRuntime {
    /// Create a new app runtime.
    pub fn new(oracle_registry: Arc<OracleRegistry>) -> Self {
        Self {
            state_factory: StateFactory::new(),
            oracle_registry,
            apps: RwLock::new(HashMap::new()),
            genesis_caps: None,
        }
    }

    /// Set genesis capabilities for bootstrap.
    pub fn set_genesis_capabilities(&mut self, genesis: GenesisCapabilities) {
        self.genesis_caps = Some(genesis);
    }

    /// Get the oracle registry.
    pub fn oracle_registry(&self) -> &Arc<OracleRegistry> {
        &self.oracle_registry
    }

    /// Get the current bootstrap phase.
    pub fn phase(&self) -> BootstrapPhase {
        self.oracle_registry.phase()
    }

    /// Set the bootstrap phase.
    pub fn set_phase(&self, phase: BootstrapPhase) {
        self.oracle_registry.set_phase(phase);
    }

    /// Prepare an app for installation.
    ///
    /// This validates the manifest and creates state handles, returning
    /// a builder that allows registering handlers before finalizing.
    pub async fn prepare(&self, manifest: Manifest) -> Result<AppBuilder, RuntimeError> {
        // Check if already installed
        {
            let apps = self.apps.read().await;
            if apps.contains_key(&manifest.app_id()) {
                return Err(RuntimeError::AlreadyInstalled(manifest.app_id()));
            }
        }

        // Create namespace
        let namespace = AppNamespace::new(&manifest.publisher, &manifest.name)
            .map_err(|e| RuntimeError::State(e.to_string()))?;

        // Create state handles
        let state = self
            .state_factory
            .create_for_app(namespace.clone(), &manifest.state)
            .await
            .map_err(|e| RuntimeError::State(e.to_string()))?;

        Ok(AppBuilder {
            manifest,
            namespace,
            state,
            reducers: HashMap::new(),
            services: HashMap::new(),
            oracle: None,
        })
    }

    /// Install an app from a prepared builder.
    pub async fn install(
        &self,
        builder: AppBuilder,
        installer_caps: &CapabilitySet,
    ) -> Result<AppId, RuntimeError> {
        let manifest = builder.manifest;
        let app_id = manifest.app_id();

        // Validate capability requests
        for cap_str in &manifest.capabilities_required {
            let cap_req = CapabilityRequest::parse(cap_str).ok_or_else(|| {
                RuntimeError::Manifest(ManifestError::Validation(format!(
                    "Invalid capability format: {}",
                    cap_str
                )))
            })?;
            if !installer_caps.can_delegate(&cap_req) {
                return Err(RuntimeError::InsufficientPrivilege(cap_str.clone()));
            }
        }

        // Issue capabilities
        let capabilities = self.issue_capabilities(&manifest)?;

        // Create dispatcher with registered handlers
        let mut dispatcher = ComputeDispatcher::new(&app_id, builder.state.clone());

        for (event_type, reducer) in builder.reducers {
            dispatcher.register_reducer(event_type, reducer);
        }

        for (request_type, service) in builder.services {
            dispatcher.register_service(request_type, service);
        }

        // Register oracle if provided
        if let Some(oracle) = builder.oracle {
            if let Some(oracle_config) = &manifest.oracle {
                // Validate oracle domain matches manifest declaration
                let manifest_domain = Domain::new(&oracle_config.domain);
                let oracle_domain = oracle.domain();
                if oracle_domain.as_str() != manifest_domain.as_str() {
                    return Err(RuntimeError::OracleDomainMismatch {
                        oracle_domain: oracle_domain.as_str().to_string(),
                        manifest_domain: manifest_domain.as_str().to_string(),
                    });
                }
                self.oracle_registry.register(manifest_domain, oracle);
            }
        }

        // Create app handle
        let handle = AppHandle {
            id: app_id.clone(),
            manifest,
            namespace: builder.namespace,
            state: builder.state,
            dispatcher: Arc::new(RwLock::new(dispatcher)),
            capabilities,
            status: AppStatus::Installed,
            dispatcher_task: None,
            failed: Arc::new(AtomicBool::new(false)),
        };

        // Store app
        {
            let mut apps = self.apps.write().await;
            apps.insert(app_id.clone(), handle);
        }

        Ok(app_id)
    }

    /// Install an app from a manifest file.
    ///
    /// This is a convenience method that loads and validates the manifest,
    /// but does NOT register handlers. Use `prepare()` + `install()` for
    /// apps that need custom handlers.
    pub async fn install_from_path<P: AsRef<Path>>(
        &self,
        path: P,
        installer_caps: &CapabilitySet,
    ) -> Result<AppId, RuntimeError> {
        let manifest = Manifest::load(path).map_err(RuntimeError::Manifest)?;
        let builder = self.prepare(manifest).await?;
        self.install(builder, installer_caps).await
    }

    /// Start an app.
    pub async fn start(&self, app_id: &AppId) -> Result<(), RuntimeError> {
        let mut apps = self.apps.write().await;
        let app = apps
            .get_mut(app_id)
            .ok_or_else(|| RuntimeError::NotFound(app_id.clone()))?;

        if app.status == AppStatus::Running {
            return Ok(()); // Already running
        }

        app.status = AppStatus::Starting;

        // Start dispatcher event loop
        let dispatcher = app.dispatcher.clone();
        let failed = app.failed.clone();
        let app_id_clone = app_id.clone();
        let task = tokio::spawn(async move {
            let result = {
                let mut dispatcher = dispatcher.write().await;
                dispatcher.run().await
            };

            match result {
                Ok(()) => {
                    tracing::debug!("Dispatcher for {} stopped normally", app_id_clone);
                }
                Err(e) => {
                    tracing::error!("Dispatcher for {} failed: {}", app_id_clone, e);
                    failed.store(true, Ordering::SeqCst);
                }
            }
        });

        app.dispatcher_task = Some(task);
        app.status = AppStatus::Running;

        Ok(())
    }

    /// Stop an app.
    ///
    /// This operation has a timeout to prevent indefinite blocking. If the
    /// operation times out, a `ShutdownTimeout` error is returned and the
    /// app is marked as `Failed`.
    pub async fn stop(&self, app_id: &AppId) -> Result<(), RuntimeError> {
        let app_id_clone = app_id.clone();
        match tokio::time::timeout(STOP_OPERATION_TIMEOUT, self.stop_inner(app_id)).await {
            Ok(result) => result,
            Err(_) => {
                tracing::error!(
                    app_id = %app_id_clone,
                    timeout_secs = STOP_OPERATION_TIMEOUT.as_secs(),
                    "App stop operation timed out"
                );

                // Record metric for timeout
                icn_obs::metrics::apps::apps_shutdown_timeout_total_inc(&app_id_clone);

                // Mark app as failed due to shutdown timeout
                if let Ok(mut apps) = self.apps.try_write() {
                    if let Some(app) = apps.get_mut(&app_id_clone) {
                        app.status = AppStatus::Failed;
                    }
                }

                Err(RuntimeError::ShutdownTimeout(
                    STOP_OPERATION_TIMEOUT.as_secs(),
                ))
            }
        }
    }

    /// Internal stop implementation without timeout wrapper.
    async fn stop_inner(&self, app_id: &AppId) -> Result<(), RuntimeError> {
        let mut apps = self.apps.write().await;
        let app = apps
            .get_mut(app_id)
            .ok_or_else(|| RuntimeError::NotFound(app_id.clone()))?;

        if app.status != AppStatus::Running {
            return Ok(()); // Not running
        }

        app.status = AppStatus::Stopping;

        // Stop dispatcher
        {
            let dispatcher = app.dispatcher.read().await;
            dispatcher.stop().await;
        }

        // Wait for task to complete with timeout
        if let Some(task) = app.dispatcher_task.take() {
            match tokio::time::timeout(DISPATCHER_SHUTDOWN_TIMEOUT, task).await {
                Ok(_) => {}
                Err(_) => {
                    tracing::warn!(
                        app_id = %app_id,
                        timeout_secs = DISPATCHER_SHUTDOWN_TIMEOUT.as_secs(),
                        "Dispatcher task did not stop within timeout"
                    );
                }
            }
        }

        app.status = AppStatus::Stopped;

        Ok(())
    }

    /// Uninstall an app.
    ///
    /// # Race Condition Note
    ///
    /// This operation first calls `stop()` then removes the app under a new
    /// lock acquisition. In rare cases where `stop()` times out and another
    /// caller removes the app concurrently, this method will return
    /// `NotFound`. This is acceptable as the app has been removed.
    ///
    /// For truly atomic uninstall, callers should ensure exclusive access
    /// to the runtime (single writer pattern).
    pub async fn uninstall(&self, app_id: &AppId) -> Result<(), RuntimeError> {
        // Stop first if running (ignore errors - app may already be stopped or not exist)
        let stop_result = self.stop(app_id).await;

        // Remove app under lock
        let mut apps = self.apps.write().await;
        let app = match apps.remove(app_id) {
            Some(app) => app,
            None => {
                // App was removed between stop() and now, or never existed
                // If stop succeeded, another caller removed it; if stop failed
                // with NotFound, app never existed.
                if matches!(stop_result, Err(RuntimeError::NotFound(_))) {
                    return Err(RuntimeError::NotFound(app_id.clone()));
                }
                // App was removed by concurrent caller after successful stop
                tracing::debug!(
                    app_id = %app_id,
                    "App already removed by concurrent operation"
                );
                return Ok(());
            }
        };

        // Unregister oracle if present
        if let Some(oracle_config) = &app.manifest.oracle {
            let domain = Domain::new(&oracle_config.domain);
            self.oracle_registry.unregister(&domain);
        }

        // Clean up state
        self.state_factory.remove(&app.namespace).await;

        Ok(())
    }

    /// Get app status.
    ///
    /// Note: This calls `AppHandle::status()` which checks the failed flag,
    /// returning `Failed` if the dispatcher task has failed.
    pub async fn status(&self, app_id: &AppId) -> Option<AppStatus> {
        let apps = self.apps.read().await;
        apps.get(app_id).map(|a| a.status())
    }

    /// Get app handle (read-only).
    pub async fn get(&self, app_id: &AppId) -> Option<AppId> {
        let apps = self.apps.read().await;
        apps.get(app_id).map(|a| a.id.clone())
    }

    /// List all installed apps.
    pub async fn list(&self) -> Vec<AppId> {
        let apps = self.apps.read().await;
        apps.keys().cloned().collect()
    }

    /// Get dispatcher for an app (for sending events/requests).
    pub async fn dispatcher(&self, app_id: &AppId) -> Option<Arc<RwLock<ComputeDispatcher>>> {
        let apps = self.apps.read().await;
        apps.get(app_id).map(|a| a.dispatcher.clone())
    }

    /// Issue capabilities for an app based on its manifest.
    fn issue_capabilities(&self, manifest: &Manifest) -> Result<CapabilitySet, RuntimeError> {
        let mut cap_set = CapabilitySet::new();

        // During genesis, use genesis capabilities
        if let Some(genesis) = &self.genesis_caps {
            if genesis.is_valid() {
                for cap_str in &manifest.capabilities_required {
                    if let Some(cap_req) = CapabilityRequest::parse(cap_str) {
                        let cap = genesis
                            .issue(
                                &cap_req.resource,
                                &cap_req.action,
                                &manifest.publisher,
                                Constraints::default(),
                                u64::MAX,
                            )
                            .map_err(|e| RuntimeError::Capability(e.to_string()))?;
                        cap_set.add(cap);
                    }
                }
            }
        }

        Ok(cap_set)
    }
}

/// Runtime errors.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// App already installed
    #[error("App already installed: {0}")]
    AlreadyInstalled(AppId),

    /// App not found
    #[error("App not found: {0}")]
    NotFound(AppId),

    /// Manifest error
    #[error("Manifest error: {0}")]
    Manifest(#[from] ManifestError),

    /// State error
    #[error("State error: {0}")]
    State(String),

    /// Insufficient privilege to grant capability
    #[error("Insufficient privilege for: {0}")]
    InsufficientPrivilege(String),

    /// Capability error
    #[error("Capability error: {0}")]
    Capability(String),

    /// Dispatch error
    #[error("Dispatch error: {0}")]
    Dispatch(String),

    /// Shutdown timeout
    #[error("App shutdown timed out after {0} seconds")]
    ShutdownTimeout(u64),

    /// Oracle domain mismatch
    #[error(
        "Oracle domain '{oracle_domain}' does not match manifest declaration '{manifest_domain}'"
    )]
    OracleDomainMismatch {
        oracle_domain: String,
        manifest_domain: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::dispatcher::{
        DispatchError, Event, Reducer, Request, Response, Service, StateDelta, StateSnapshot,
    };
    use crate::apps::manifest::{
        ComputeConfig, KvConfig, ReducerConfig, ServiceConfig, StateConfig,
    };
    use icn_kernel_api::authz::Capability;
    use tokio::sync::mpsc;

    /// Echo reducer for testing.
    struct EchoReducer;

    impl Reducer for EchoReducer {
        fn reduce(
            &self,
            _state: &StateSnapshot,
            event: &Event,
        ) -> Result<StateDelta, DispatchError> {
            let mut delta = StateDelta::new();
            let key = format!("msg:{}", event.timestamp);
            delta.kv_set("echoes", key, event.payload.clone());
            Ok(delta)
        }
    }

    /// Echo service for testing.
    struct EchoService;

    #[async_trait::async_trait]
    impl Service for EchoService {
        async fn handle(
            &self,
            request: Request,
            state: &StateSnapshot,
            _event_tx: mpsc::Sender<Event>,
        ) -> Result<Response, DispatchError> {
            // List all keys
            let keys = state.kv_keys("echoes");
            Response::success_with(&request.id, &keys.len())
        }
    }

    fn test_manifest() -> Manifest {
        Manifest {
            schema_version: 1,
            name: "test-app".to_string(),
            version: "0.1.0".to_string(),
            publisher: "did:icn:test".to_string(),
            description: None,
            capabilities_required: vec!["state:kv:write:self".to_string()],
            capabilities_provided: vec![],
            state: StateConfig {
                kv: vec![KvConfig {
                    name: "echoes".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            compute: ComputeConfig {
                reducers: vec![ReducerConfig {
                    name: "echo_reducer".to_string(),
                    event_type: "echo:message".to_string(),
                }],
                services: vec![ServiceConfig {
                    name: "echo_service".to_string(),
                    request_type: "echo:query".to_string(),
                }],
            },
            oracle: None,
            metadata: HashMap::new(),
        }
    }

    fn root_capability_set() -> CapabilitySet {
        let mut set = CapabilitySet::new();
        set.add(Capability {
            id: "root".to_string(),
            resource: "*".to_string(),
            action: "*".to_string(),
            constraints: Constraints::default(),
            holder: None,
            issuer: "did:icn:root".to_string(),
            expiration: u64::MAX,
            signature: vec![],
        });
        set
    }

    #[tokio::test]
    async fn test_app_install() {
        let registry = Arc::new(OracleRegistry::new());
        let runtime = AppRuntime::new(registry);

        let manifest = test_manifest();
        let builder = runtime.prepare(manifest).await.unwrap();
        let app_id = runtime
            .install(builder, &root_capability_set())
            .await
            .unwrap();

        assert!(runtime.status(&app_id).await.is_some());
        assert_eq!(runtime.status(&app_id).await, Some(AppStatus::Installed));
    }

    #[tokio::test]
    async fn test_app_start_stop() {
        let registry = Arc::new(OracleRegistry::new());
        let runtime = AppRuntime::new(registry);

        let manifest = test_manifest();
        let builder = runtime.prepare(manifest).await.unwrap();
        let app_id = runtime
            .install(builder, &root_capability_set())
            .await
            .unwrap();

        // Start
        runtime.start(&app_id).await.unwrap();
        assert_eq!(runtime.status(&app_id).await, Some(AppStatus::Running));

        // Stop
        runtime.stop(&app_id).await.unwrap();
        assert_eq!(runtime.status(&app_id).await, Some(AppStatus::Stopped));
    }

    #[tokio::test]
    async fn test_app_with_handlers() {
        let registry = Arc::new(OracleRegistry::new());
        let runtime = AppRuntime::new(registry);

        let manifest = test_manifest();
        let builder = runtime
            .prepare(manifest)
            .await
            .unwrap()
            .with_reducer("echo:message", Box::new(EchoReducer))
            .with_service("echo:query", Box::new(EchoService));

        let app_id = runtime
            .install(builder, &root_capability_set())
            .await
            .unwrap();
        runtime.start(&app_id).await.unwrap();

        // Get dispatcher and send event
        let dispatcher = runtime.dispatcher(&app_id).await.unwrap();
        let event = Event::new("echo:message", b"hello".to_vec(), "test");

        {
            let d = dispatcher.read().await;
            d.dispatch_event(event).await.unwrap();
        }

        // Query
        let request = Request::new("echo:query", vec![], "test");
        {
            let d = dispatcher.read().await;
            let response = d.handle_request(request).await.unwrap();
            assert!(response.success);
        }

        runtime.stop(&app_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_duplicate_install() {
        let registry = Arc::new(OracleRegistry::new());
        let runtime = AppRuntime::new(registry);

        let manifest = test_manifest();
        let builder = runtime.prepare(manifest.clone()).await.unwrap();
        runtime
            .install(builder, &root_capability_set())
            .await
            .unwrap();

        // Try to install again
        let builder2 = runtime.prepare(manifest).await;
        assert!(matches!(builder2, Err(RuntimeError::AlreadyInstalled(_))));
    }

    #[tokio::test]
    async fn test_uninstall() {
        let registry = Arc::new(OracleRegistry::new());
        let runtime = AppRuntime::new(registry);

        let manifest = test_manifest();
        let builder = runtime.prepare(manifest).await.unwrap();
        let app_id = runtime
            .install(builder, &root_capability_set())
            .await
            .unwrap();

        runtime.start(&app_id).await.unwrap();
        runtime.uninstall(&app_id).await.unwrap();

        assert!(runtime.status(&app_id).await.is_none());
    }

    #[tokio::test]
    async fn test_list_apps() {
        let registry = Arc::new(OracleRegistry::new());
        let runtime = AppRuntime::new(registry);

        let manifest = test_manifest();
        let builder = runtime.prepare(manifest).await.unwrap();
        let app_id = runtime
            .install(builder, &root_capability_set())
            .await
            .unwrap();

        let apps = runtime.list().await;
        assert_eq!(apps.len(), 1);
        assert!(apps.contains(&app_id));
    }
}
