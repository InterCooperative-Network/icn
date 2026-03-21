//! Connection candidate exchange for NAT traversal
//!
//! This module provides types for advertising and discovering connection endpoints
//! through the gossip protocol. Nodes publish their connection information (local
//! IP, STUN-discovered public IP, relay addresses) to help peers establish
//! connections even when both parties are behind NAT.
//!
//! # Dual-Stack Design
//!
//! `EndpointCandidate` and `EndpointKind` are the foundational types for IPv6
//! dual-stack support (#1297). `ConnectionCandidate` stores a `Vec<EndpointCandidate>`
//! instead of flat SocketAddr fields, enabling multiple addresses per kind (required
//! for mDNS multi-address #1298 and Happy Eyeballs #1299).

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Classification of a connection endpoint address
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EndpointKind {
    /// Same LAN (mDNS-discovered or manually configured local address)
    Local,
    /// STUN-discovered external address for NAT hole punching
    Public,
    /// TURN relay address (fallback when direct connection fails)
    Relay,
}

/// A single candidate address with its endpoint classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EndpointCandidate {
    pub addr: SocketAddr,
    pub kind: EndpointKind,
}

impl EndpointCandidate {
    pub fn new(addr: SocketAddr, kind: EndpointKind) -> Self {
        Self { addr, kind }
    }

    pub fn local(addr: SocketAddr) -> Self {
        Self::new(addr, EndpointKind::Local)
    }

    pub fn public(addr: SocketAddr) -> Self {
        Self::new(addr, EndpointKind::Public)
    }

    pub fn relay(addr: SocketAddr) -> Self {
        Self::new(addr, EndpointKind::Relay)
    }
}

/// Connection candidate announced by a node
///
/// This message is published to the `network:candidates` gossip topic
/// to advertise how other nodes can reach this peer.
///
/// # Wire Format Change (intentional, #1297)
///
/// Previously stored `local_addr: SocketAddr`, `public_addr: Option<SocketAddr>`,
/// `relay_addr: Option<SocketAddr>` as flat fields. Now stores
/// `endpoints: Vec<EndpointCandidate>` to support multiple addresses per kind
/// (required for IPv6 dual-stack, mDNS multi-address, and Happy Eyeballs).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionCandidate {
    /// DID of the node advertising this candidate
    pub did: Did,

    /// All candidate endpoints, classified by kind.
    ///
    /// Use `local_addr()`, `public_addr()`, `relay_addr()` for single-address
    /// access, or `endpoints_of_kind()` for multi-address iteration.
    pub endpoints: Vec<EndpointCandidate>,

    /// Unix timestamp (seconds since epoch) when this candidate was created
    ///
    /// Helps peers determine freshness of connection information.
    pub timestamp: u64,

    /// Protocol version for future compatibility
    ///
    /// Set to 1 for initial implementation. Future versions may add
    /// additional fields or change semantics.
    pub version: u8,
}

impl ConnectionCandidate {
    /// Create a new connection candidate from flat address arguments.
    ///
    /// Backward-compatible constructor — converts the three SocketAddr
    /// arguments into a `Vec<EndpointCandidate>` internally.
    ///
    /// # Arguments
    /// * `did` - DID of the node
    /// * `local_addr` - Local address the node is listening on
    /// * `public_addr` - Public address discovered via STUN (if available)
    /// * `relay_addr` - Relay address (if available)
    pub fn new(
        did: Did,
        local_addr: SocketAddr,
        public_addr: Option<SocketAddr>,
        relay_addr: Option<SocketAddr>,
    ) -> Self {
        let mut endpoints = vec![EndpointCandidate::local(local_addr)];
        if let Some(a) = public_addr {
            endpoints.push(EndpointCandidate::public(a));
        }
        if let Some(a) = relay_addr {
            endpoints.push(EndpointCandidate::relay(a));
        }
        Self {
            did,
            endpoints,
            timestamp: icn_time::current_timestamp_secs(),
            version: 1,
        }
    }

    /// First local-kind address (same LAN), if any.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.endpoints
            .iter()
            .find(|e| e.kind == EndpointKind::Local)
            .map(|e| e.addr)
    }

    /// First public-kind address (STUN-discovered), if any.
    pub fn public_addr(&self) -> Option<SocketAddr> {
        self.endpoints
            .iter()
            .find(|e| e.kind == EndpointKind::Public)
            .map(|e| e.addr)
    }

    /// First relay-kind address, if any.
    pub fn relay_addr(&self) -> Option<SocketAddr> {
        self.endpoints
            .iter()
            .find(|e| e.kind == EndpointKind::Relay)
            .map(|e| e.addr)
    }

    /// Iterate all addresses of a given kind.
    ///
    /// Returns multiple addresses when available — used by Happy Eyeballs (#1299)
    /// and mDNS multi-address (#1298) once those features are implemented.
    pub fn endpoints_of_kind(&self, kind: EndpointKind) -> impl Iterator<Item = SocketAddr> + '_ {
        self.endpoints
            .iter()
            .filter(move |e| e.kind == kind)
            .map(|e| e.addr)
    }

    /// Check if this candidate is still fresh
    ///
    /// Candidates older than the given max_age (in seconds) are considered stale.
    /// Default recommendation: 300 seconds (5 minutes).
    pub fn is_fresh(&self, max_age_secs: u64) -> bool {
        let now = icn_time::current_timestamp_secs();
        now.saturating_sub(self.timestamp) <= max_age_secs
    }

    /// Get age of this candidate in seconds
    pub fn age_secs(&self) -> u64 {
        let now = icn_time::current_timestamp_secs();
        now.saturating_sub(self.timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_candidate_creation() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let local = "192.168.1.100:5000".parse().unwrap();
        let public = Some("203.0.113.5:5000".parse().unwrap());

        let candidate = ConnectionCandidate::new(did.clone(), local, public, None);

        assert_eq!(candidate.did, did);
        assert_eq!(candidate.local_addr(), Some(local));
        assert_eq!(candidate.public_addr(), public);
        assert_eq!(candidate.relay_addr(), None);
        assert_eq!(candidate.version, 1);
        assert!(candidate.timestamp > 0);
    }

    #[test]
    fn test_candidate_freshness() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let local = "192.168.1.100:5000".parse().unwrap();

        let candidate = ConnectionCandidate::new(did, local, None, None);

        // Fresh candidate
        assert!(candidate.is_fresh(300));
        assert_eq!(candidate.age_secs(), 0);

        // Simulate old candidate (manually set timestamp to 10 minutes ago)
        let mut old_candidate = candidate.clone();
        old_candidate.timestamp -= 600; // 10 minutes ago

        assert!(!old_candidate.is_fresh(300)); // Max 5 minutes
        assert!(old_candidate.age_secs() >= 600);
    }

    #[test]
    fn test_candidate_serialization() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let local = "192.168.1.100:5000".parse().unwrap();
        let public = Some("203.0.113.5:5000".parse().unwrap());

        let candidate = ConnectionCandidate::new(did.clone(), local, public, None);

        // Serialize to JSON
        let json = serde_json::to_string(&candidate).unwrap();
        assert!(json.contains(&did.to_string()));

        // Deserialize back
        let deserialized: ConnectionCandidate = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.did, candidate.did);
        assert_eq!(deserialized.local_addr(), candidate.local_addr());
        assert_eq!(deserialized.public_addr(), candidate.public_addr());
        assert_eq!(deserialized.timestamp, candidate.timestamp);
    }

    #[test]
    fn test_candidate_with_all_addresses() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let local = "10.0.0.5:5000".parse().unwrap();
        let public = Some("198.51.100.42:5000".parse().unwrap());
        let relay = Some("192.0.2.100:3478".parse().unwrap()); // Use IP instead of hostname

        let candidate = ConnectionCandidate::new(did.clone(), local, public, relay);

        assert_eq!(candidate.local_addr(), Some(local));
        assert_eq!(candidate.public_addr(), public);
        assert_eq!(candidate.relay_addr(), relay);
    }

    #[test]
    fn test_endpoint_candidate_constructors() {
        let addr: SocketAddr = "192.168.1.1:5000".parse().unwrap();
        assert_eq!(EndpointCandidate::local(addr).kind, EndpointKind::Local);
        assert_eq!(EndpointCandidate::public(addr).kind, EndpointKind::Public);
        assert_eq!(EndpointCandidate::relay(addr).kind, EndpointKind::Relay);
    }

    #[test]
    fn test_endpoint_kind_serialization_roundtrip() {
        let ep = EndpointCandidate::local("10.0.0.1:9000".parse().unwrap());
        let json = serde_json::to_string(&ep).unwrap();
        assert!(
            json.contains("\"local\""),
            "EndpointKind must serialize as snake_case"
        );
        let decoded: EndpointCandidate = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, ep);
    }

    #[test]
    fn test_connection_candidate_endpoints_of_kind() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let local: SocketAddr = "192.168.1.1:5000".parse().unwrap();
        let public: SocketAddr = "1.2.3.4:5000".parse().unwrap();

        let candidate = ConnectionCandidate::new(did, local, Some(public), None);

        let locals: Vec<SocketAddr> = candidate.endpoints_of_kind(EndpointKind::Local).collect();
        assert_eq!(locals, vec![local]);

        let publics: Vec<SocketAddr> = candidate.endpoints_of_kind(EndpointKind::Public).collect();
        assert_eq!(publics, vec![public]);

        let relays: Vec<SocketAddr> = candidate.endpoints_of_kind(EndpointKind::Relay).collect();
        assert!(relays.is_empty());
    }

    #[test]
    fn test_connection_candidate_wire_format_has_endpoints_field() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let local: SocketAddr = "10.0.0.5:5000".parse().unwrap();

        let candidate = ConnectionCandidate::new(did, local, None, None);
        let json = serde_json::to_string(&candidate).unwrap();

        // Wire format must use "endpoints" array, not flat fields
        assert!(
            json.contains("\"endpoints\""),
            "wire format must have endpoints field"
        );
        assert!(
            !json.contains("\"local_addr\""),
            "old flat field must not appear"
        );
    }
}
