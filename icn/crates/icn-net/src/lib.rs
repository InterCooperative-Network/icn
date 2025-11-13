//! ICN Net - Network transport, discovery, and session management

pub mod actor;
pub mod discovery;
pub mod global_rate_limit;
pub mod protocol;
pub mod rate_limit;
pub mod session;
pub mod tls;
pub mod topology;

pub use actor::{IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMsg, NetworkStats};
pub use discovery::{Discovery, PeerInfo};
pub use global_rate_limit::GlobalRateLimiter;
pub use protocol::{MessagePayload, NetworkMessage, read_message, write_message};
pub use rate_limit::{RateLimitConfig, RateLimiter, TrustGatedRateLimitConfig};
pub use session::SessionManager;
pub use topology::{NeighborLimitsConfig, NeighborMetrics, NeighborSets, NodeRole, PeerId, TopologyInfo};
