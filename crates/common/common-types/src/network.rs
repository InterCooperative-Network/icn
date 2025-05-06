//! Common networking types for the ICN project
//!
//! This module provides shared networking definitions used across components.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::{IpAddr, SocketAddr};

/// Network address representing a node in the ICN network
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkAddress {
    /// The underlying socket address
    pub socket_addr: SocketAddr,
    /// Optional protocol identifier
    pub protocol: Option<Protocol>,
}

/// Network protocols supported in the ICN system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Protocol {
    /// HTTP protocol
    Http,
    /// HTTPS protocol
    Https,
    /// WebSocket protocol
    WebSocket,
    /// Secure WebSocket protocol
    WebSocketSecure,
    /// libp2p protocol
    Libp2p,
}

impl fmt::Display for NetworkAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(protocol) = self.protocol {
            write!(f, "{}://{}", protocol, self.socket_addr)
        } else {
            write!(f, "{}", self.socket_addr)
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::Http => write!(f, "http"),
            Protocol::Https => write!(f, "https"),
            Protocol::WebSocket => write!(f, "ws"),
            Protocol::WebSocketSecure => write!(f, "wss"),
            Protocol::Libp2p => write!(f, "p2p"),
        }
    }
}

impl NetworkAddress {
    /// Create a new network address with the given socket address and protocol
    pub fn new(socket_addr: SocketAddr, protocol: Option<Protocol>) -> Self {
        Self {
            socket_addr,
            protocol,
        }
    }

    /// Create a new HTTP network address
    pub fn http(ip: IpAddr, port: u16) -> Self {
        Self::new(
            SocketAddr::new(ip, port),
            Some(Protocol::Http),
        )
    }

    /// Create a new HTTPS network address
    pub fn https(ip: IpAddr, port: u16) -> Self {
        Self::new(
            SocketAddr::new(ip, port),
            Some(Protocol::Https),
        )
    }

    /// Create a new WebSocket network address
    pub fn ws(ip: IpAddr, port: u16) -> Self {
        Self::new(
            SocketAddr::new(ip, port),
            Some(Protocol::WebSocket),
        )
    }

    /// Create a new Secure WebSocket network address
    pub fn wss(ip: IpAddr, port: u16) -> Self {
        Self::new(
            SocketAddr::new(ip, port),
            Some(Protocol::WebSocketSecure),
        )
    }

    /// Create a new libp2p network address
    pub fn p2p(ip: IpAddr, port: u16) -> Self {
        Self::new(
            SocketAddr::new(ip, port),
            Some(Protocol::Libp2p),
        )
    }
} 