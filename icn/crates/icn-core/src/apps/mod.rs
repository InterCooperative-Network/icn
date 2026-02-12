//! App Runtime Infrastructure
//!
//! This module provides the runtime for loading and managing ICN apps.
//!
//! # Architecture
//!
//! The runtime implements a clean kernel/app separation:
//!
//! - **Kernel**: Provides primitives (state, compute, comms, etc.)
//! - **Apps**: Implement domain logic using kernel APIs
//!
//! The supervisor loads apps from manifests with **zero domain knowledge**.
//! It does not understand "trust" or "governance" - it just loads what
//! config tells it to load.
//!
//! # Modules
//!
//! - [`manifest`]: YAML manifest schema for apps
//! - [`state_factory`]: Namespaced state handle creation
//! - [`dispatcher`]: Event/request routing with reducer/service split
//! - [`runtime`]: App lifecycle (install, start, stop, uninstall)
//!
//! # Example
//!
//! ```ignore
//! use icn_core::apps::{AppRuntime, Manifest};
//! use icn_kernel_api::bootstrap::OracleRegistry;
//!
//! // Create runtime
//! let registry = Arc::new(OracleRegistry::new());
//! let runtime = AppRuntime::new(registry);
//!
//! // Prepare app from manifest
//! let manifest = Manifest::parse(yaml)?;
//! let builder = runtime.prepare(manifest).await?;
//!
//! // Register handlers
//! let builder = builder
//!     .with_reducer("echo:message", Box::new(EchoReducer))
//!     .with_service("echo:query", Box::new(EchoService));
//!
//! // Install and start
//! let app_id = runtime.install(builder, &installer_caps).await?;
//! runtime.start(&app_id).await?;
//! ```
//!
//! # The Meaning Firewall
//!
//! Before adding code to this module, ask:
//!
//! - Does this interpret domain semantics? → Must be in an app
//! - Does this hardcode a schema? → Must be in an app
//! - Does this privilege a specific application? → Must be in an app
//!
//! The runtime provides mechanisms, not policies.

pub mod dispatcher;
pub mod manifest;
pub mod runtime;
pub mod state_factory;

// Re-exports
pub use dispatcher::{
    BoxedReducer, BoxedService, ComputeDispatcher, DispatchError, Event, KvOp, LogOp, Reducer,
    Request, Response, Service, StateDelta, StateSnapshot,
};
pub use manifest::{
    BlobConfig, ComputeConfig, KvConfig, LogConfig, LogOrdering, Manifest, ManifestError,
    OracleConfig, ReducerConfig, ServiceConfig, StateConfig,
};
pub use runtime::{AppBuilder, AppHandle, AppId, AppRuntime, AppStatus, RuntimeError};
pub use state_factory::{
    AppNamespace, AppState, BlobHandle, KvHandle, LogEntry, LogHandle, StateError, StateFactory,
};
