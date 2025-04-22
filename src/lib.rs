pub mod api;
pub mod guardians;
pub mod identity;
pub mod proposal;
pub mod storage;
pub mod token;
pub mod federation;
pub mod sync;
pub mod vc;
pub mod tui;
pub mod websocket;

// Re-export key types for convenience
pub use api::{ApiClient, ApiConfig, ApiError};
pub use guardians::{GuardianManager, GuardianSet, GuardianStatus, GuardianError, RecoveryBundle, GuardianSignature};
pub use identity::{Identity, IdentityManager, KeyType, IdentityError, DeviceLink, DeviceLinkChallenge};
pub use proposal::{ProposalManager, Proposal, Vote, VoteOption, ProposalStatus, ProposalError};
pub use storage::{StorageManager, StorageType, StorageError};
pub use token::{TokenStore, TokenType, TokenBalance, TokenError};
pub use federation::{FederationRuntime, FederationError, MonitoringStatus, MonitoringResult, MonitoringOptions, DagStatus, ProposalAudit};
pub use sync::{SyncManager, SyncConfig, SyncError, Notification, NotificationType};
pub use vc::{CredentialGenerator, VerifiableCredential, CredentialError};
pub use tui::run_tui;
pub use websocket::{WebSocketServer, WebSocketConfig, WebSocketError}; 