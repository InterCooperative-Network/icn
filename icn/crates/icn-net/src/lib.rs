//! ICN Net - Network transport, discovery, and session management

pub mod actor;
pub mod discovery;
pub mod encryption;
pub mod envelope;
pub mod global_rate_limit;
pub mod protocol;
pub mod rate_limit;
pub mod replay_guard;
pub mod session;
pub mod tls;
pub mod topology;
pub mod version;

pub use actor::{IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMsg, NetworkStats};
pub use discovery::{Discovery, PeerInfo};
pub use encryption::EncryptedEnvelope;
pub use envelope::{PayloadType, SignedEnvelope};
pub use global_rate_limit::GlobalRateLimiter;
pub use protocol::{MessagePayload, NetworkMessage, read_message, write_message};
pub use rate_limit::{RateLimitConfig, RateLimiter, TrustGatedRateLimitConfig};
pub use replay_guard::ReplayGuard;
pub use session::SessionManager;
pub use topology::{
    FanoutConfig, NeighborLimitsConfig, NeighborMetrics, NeighborSets, NodeRole, PeerId,
    TopologyConfig, TopologyInfo,
};
pub use version::{common_capabilities, negotiate_version, CapabilityFlags, VersionInfo};
