//! NAT traversal dial strategy with multi-candidate fallback
//!
//! This module provides intelligent connection establishment for peers behind NAT.
//! When dialing a peer, it attempts multiple addresses in order of preference:
//!
//! 1. **Local address** (same LAN) - fastest, 2s timeout
//! 2. **Public address** (NAT hole punch) - medium, 10s timeout
//! 3. **Relay address** (TURN server) - fallback, 30s timeout
//!
//! The strategy supports parallel attempts for local + public addresses to minimize
//! connection latency while still falling back to relay when direct connection fails.

use crate::config::NatDialConfig;
use icn_net::{candidate::ConnectionCandidate, NetworkHandle};
use std::net::SocketAddr;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Result of a dial attempt
#[derive(Debug)]
pub enum DialResult {
    /// Successfully connected via the given address type
    Connected {
        addr_type: AddrType,
        addr: SocketAddr,
    },
    /// All attempts failed
    Failed { errors: Vec<DialError> },
}

/// Type of address used for connection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddrType {
    Local,
    Public,
    Relay,
}

impl std::fmt::Display for AddrType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AddrType::Local => write!(f, "local"),
            AddrType::Public => write!(f, "public"),
            AddrType::Relay => write!(f, "relay"),
        }
    }
}

/// Error from a dial attempt
#[derive(Debug)]
pub struct DialError {
    pub addr_type: AddrType,
    pub addr: SocketAddr,
    pub error: String,
}

/// Attempt to connect to a peer using multiple candidate addresses with fallback
///
/// This function implements a smart dial strategy:
/// - If `parallel_dial` is enabled, local and public addresses are raced in parallel
/// - If both direct attempts fail, falls back to relay address (if available)
/// - Respects configurable timeouts per address type
///
/// # Arguments
/// * `network_handle` - Handle to send dial commands
/// * `candidate` - Connection candidate with addresses to try
/// * `config` - Dial strategy configuration (timeouts, parallel mode)
///
/// # Returns
/// * `DialResult::Connected` - Successfully connected with address type and address
/// * `DialResult::Failed` - All attempts failed with error details
pub async fn dial_with_fallback(
    network_handle: &NetworkHandle,
    candidate: &ConnectionCandidate,
    config: &NatDialConfig,
) -> DialResult {
    let did = candidate.did.clone();
    let mut errors = Vec::new();

    // Increment attempt metric
    icn_obs::metrics::nat::dial_attempt_inc("total");

    if config.parallel_dial {
        // Parallel mode: race local and public addresses
        match dial_parallel(network_handle, candidate, config).await {
            Ok((addr_type, addr)) => {
                icn_obs::metrics::nat::dial_success_inc(addr_type.to_string().as_str());
                info!(
                    "Connected to {} via {} address {} (parallel mode)",
                    did, addr_type, addr
                );
                return DialResult::Connected { addr_type, addr };
            }
            Err(parallel_errors) => {
                errors.extend(parallel_errors);
            }
        }
    } else {
        // Sequential mode: try local first, then public
        if let Some(result) = dial_sequential(network_handle, candidate, config, &mut errors).await
        {
            icn_obs::metrics::nat::dial_success_inc(result.0.to_string().as_str());
            info!(
                "Connected to {} via {} address {} (sequential mode)",
                did, result.0, result.1
            );
            return DialResult::Connected {
                addr_type: result.0,
                addr: result.1,
            };
        }
    }

    // Fallback to relay if available and direct connection failed
    if let Some(relay_addr) = candidate.relay_addr() {
        debug!(
            "Direct connection failed, attempting relay via {}",
            relay_addr
        );
        icn_obs::metrics::nat::dial_attempt_inc("relay");

        let timeout = Duration::from_millis(config.relay_dial_timeout_ms);
        match tokio::time::timeout(timeout, network_handle.dial(relay_addr, did.clone())).await {
            Ok(Ok(_)) => {
                icn_obs::metrics::nat::dial_success_inc("relay");
                info!(
                    "Connected to {} via relay address {} (fallback)",
                    did, relay_addr
                );
                return DialResult::Connected {
                    addr_type: AddrType::Relay,
                    addr: relay_addr,
                };
            }
            Ok(Err(e)) => {
                errors.push(DialError {
                    addr_type: AddrType::Relay,
                    addr: relay_addr,
                    error: e.to_string(),
                });
            }
            Err(_) => {
                errors.push(DialError {
                    addr_type: AddrType::Relay,
                    addr: relay_addr,
                    error: format!("Timeout after {}ms", config.relay_dial_timeout_ms),
                });
            }
        }
    }

    // All attempts failed
    icn_obs::metrics::nat::dial_failure_inc();
    warn!(
        "Failed to connect to {} after {} attempts",
        did,
        errors.len()
    );
    DialResult::Failed { errors }
}

/// Dial local and public addresses in parallel, return first success
///
/// Note: Uses unwrap() in select! branches that are guarded by is_some() checks.
/// These are safe because the guards ensure the Option is Some before accessing.
#[allow(clippy::unwrap_used)]
async fn dial_parallel(
    network_handle: &NetworkHandle,
    candidate: &ConnectionCandidate,
    config: &NatDialConfig,
) -> Result<(AddrType, SocketAddr), Vec<DialError>> {
    let did = candidate.did.clone();
    let Some(local_addr) = candidate.local_addr() else {
        // No local address in candidate — skip parallel dial, try public only
        let public_addr = candidate.public_addr();
        if let Some(pub_addr) = public_addr {
            icn_obs::metrics::nat::dial_attempt_inc("public");
            let public_timeout = Duration::from_millis(config.public_dial_timeout_ms);
            return match tokio::time::timeout(
                public_timeout,
                network_handle.dial(pub_addr, did.clone()),
            )
            .await
            {
                Ok(Ok(_)) => Ok((AddrType::Public, pub_addr)),
                Ok(Err(e)) => Err(vec![DialError {
                    addr_type: AddrType::Public,
                    addr: pub_addr,
                    error: e.to_string(),
                }]),
                Err(_) => Err(vec![DialError {
                    addr_type: AddrType::Public,
                    addr: pub_addr,
                    error: format!("Timeout after {}ms", config.public_dial_timeout_ms),
                }]),
            };
        }
        return Err(vec![]);
    };
    let public_addr = candidate.public_addr();

    let local_timeout = Duration::from_millis(config.local_dial_timeout_ms);
    let public_timeout = Duration::from_millis(config.public_dial_timeout_ms);

    // Track metrics
    icn_obs::metrics::nat::dial_attempt_inc("local");
    if public_addr.is_some() {
        icn_obs::metrics::nat::dial_attempt_inc("public");
    }

    // Spawn local dial task
    let local_handle = network_handle.clone();
    let local_did = did.clone();
    let mut local_task = tokio::spawn(async move {
        tokio::time::timeout(local_timeout, local_handle.dial(local_addr, local_did)).await
    });

    // Spawn public dial task (if public address available)
    let mut public_task = if let Some(pub_addr) = public_addr {
        let public_handle = network_handle.clone();
        let public_did = did.clone();
        Some(tokio::spawn(async move {
            tokio::time::timeout(public_timeout, public_handle.dial(pub_addr, public_did)).await
        }))
    } else {
        None
    };

    let mut errors = Vec::new();
    let mut local_done = false;
    let mut public_done = public_addr.is_none(); // Already done if no public address

    // Race the tasks using a loop
    while !local_done || !public_done {
        tokio::select! {
            result = &mut local_task, if !local_done => {
                local_done = true;
                match result {
                    Ok(Ok(Ok(_))) => return Ok((AddrType::Local, local_addr)),
                    Ok(Ok(Err(e))) => {
                        errors.push(DialError {
                            addr_type: AddrType::Local,
                            addr: local_addr,
                            error: e.to_string(),
                        });
                    }
                    Ok(Err(_)) => {
                        errors.push(DialError {
                            addr_type: AddrType::Local,
                            addr: local_addr,
                            error: format!("Timeout after {}ms", config.local_dial_timeout_ms),
                        });
                    }
                    Err(e) => {
                        errors.push(DialError {
                            addr_type: AddrType::Local,
                            addr: local_addr,
                            error: format!("Task failed: {e}"),
                        });
                    }
                }
            }
            result = async { public_task.as_mut().unwrap().await }, if !public_done && public_task.is_some() => {
                public_done = true;
                match result {
                    Ok(Ok(Ok(_))) => return Ok((AddrType::Public, public_addr.unwrap())),
                    Ok(Ok(Err(e))) => {
                        errors.push(DialError {
                            addr_type: AddrType::Public,
                            addr: public_addr.unwrap(),
                            error: e.to_string(),
                        });
                    }
                    Ok(Err(_)) => {
                        errors.push(DialError {
                            addr_type: AddrType::Public,
                            addr: public_addr.unwrap(),
                            error: format!("Timeout after {}ms", config.public_dial_timeout_ms),
                        });
                    }
                    Err(e) => {
                        errors.push(DialError {
                            addr_type: AddrType::Public,
                            addr: public_addr.unwrap(),
                            error: format!("Task failed: {e}"),
                        });
                    }
                }
            }
        }
    }

    Err(errors)
}

/// Dial addresses sequentially: local first, then public
async fn dial_sequential(
    network_handle: &NetworkHandle,
    candidate: &ConnectionCandidate,
    config: &NatDialConfig,
    errors: &mut Vec<DialError>,
) -> Option<(AddrType, SocketAddr)> {
    let did = candidate.did.clone();

    // Try local address first (if available)
    if let Some(local_addr) = candidate.local_addr() {
        icn_obs::metrics::nat::dial_attempt_inc("local");
        let local_timeout = Duration::from_millis(config.local_dial_timeout_ms);

        debug!(
            "Attempting connection to {} via local address {}",
            did, local_addr
        );

        match tokio::time::timeout(local_timeout, network_handle.dial(local_addr, did.clone()))
            .await
        {
            Ok(Ok(_)) => {
                return Some((AddrType::Local, local_addr));
            }
            Ok(Err(e)) => {
                debug!("Failed to connect via local address: {}", e);
                errors.push(DialError {
                    addr_type: AddrType::Local,
                    addr: local_addr,
                    error: e.to_string(),
                });
            }
            Err(_) => {
                debug!(
                    "Local address dial timeout after {}ms",
                    config.local_dial_timeout_ms
                );
                errors.push(DialError {
                    addr_type: AddrType::Local,
                    addr: local_addr,
                    error: format!("Timeout after {}ms", config.local_dial_timeout_ms),
                });
            }
        }
    }

    // Try public address if available
    if let Some(public_addr) = candidate.public_addr() {
        icn_obs::metrics::nat::dial_attempt_inc("public");
        let public_timeout = Duration::from_millis(config.public_dial_timeout_ms);

        debug!(
            "Attempting connection to {} via public address {}",
            did, public_addr
        );

        match tokio::time::timeout(
            public_timeout,
            network_handle.dial(public_addr, did.clone()),
        )
        .await
        {
            Ok(Ok(_)) => {
                return Some((AddrType::Public, public_addr));
            }
            Ok(Err(e)) => {
                debug!("Failed to connect via public address: {}", e);
                errors.push(DialError {
                    addr_type: AddrType::Public,
                    addr: public_addr,
                    error: e.to_string(),
                });
            }
            Err(_) => {
                debug!(
                    "Public address dial timeout after {}ms",
                    config.public_dial_timeout_ms
                );
                errors.push(DialError {
                    addr_type: AddrType::Public,
                    addr: public_addr,
                    error: format!("Timeout after {}ms", config.public_dial_timeout_ms),
                });
            }
        }
    }

    None
}

/// Race IPv4 and IPv6 addresses within a single endpoint category (RFC 8305 Happy Eyeballs).
///
/// IPv6 is preferred: it is dialed immediately. If the IPv6 dial does not succeed within
/// `config.happy_eyeballs_delay_ms` (default 250ms), an IPv4 dial is spawned in parallel.
/// The first successful connection wins; all other tasks are cancelled.
///
/// If `addrs` contains only one IP version, that address is dialed directly with no stagger.
/// If `addrs` is empty, returns `None`.
///
/// # Design (ADR-0009)
///
/// This function races addresses **within** a single `EndpointKind`. The existing
/// `dial_parallel()` continues to race **across** kinds (Local vs Public). These two
/// levels of racing compose: for each category of addresses that has both IPv4 and IPv6,
/// Happy Eyeballs finds the faster path automatically.
pub async fn dial_happy_eyeballs(
    addrs: Vec<SocketAddr>,
    did: &icn_identity::Did,
    network_handle: &NetworkHandle,
    dial_timeout: Duration,
    config: &NatDialConfig,
) -> Option<SocketAddr> {
    if addrs.is_empty() {
        return None;
    }

    // Sort: IPv6 first, IPv4 second (RFC 8305 §5 preference order)
    let mut sorted = addrs;
    sorted.sort_by_key(|a| !a.is_ipv6());

    let delay = Duration::from_millis(config.happy_eyeballs_delay_ms);

    // Fast path: only one IP version present — dial it directly, no stagger needed
    let has_ipv6 = sorted.iter().any(|a| a.is_ipv6());
    let has_ipv4 = sorted.iter().any(|a| a.is_ipv4());
    if !has_ipv6 || !has_ipv4 {
        let addr = sorted[0];
        debug!(%did, %addr, "happy_eyeballs: single IP version, dialing directly");
        return tokio::time::timeout(dial_timeout, network_handle.dial(addr, did.clone()))
            .await
            .ok()
            .and_then(|r| r.ok())
            .map(|_| addr);
    }

    // Dual-stack path: IPv6 first, IPv4 after stagger
    let ipv6_addrs: Vec<SocketAddr> = sorted.iter().filter(|a| a.is_ipv6()).copied().collect();
    let ipv4_addrs: Vec<SocketAddr> = sorted.iter().filter(|a| a.is_ipv4()).copied().collect();

    let ipv6_addr = ipv6_addrs[0];
    let ipv4_addr = ipv4_addrs[0];

    debug!(
        %did,
        ipv6 = %ipv6_addr,
        ipv4 = %ipv4_addr,
        stagger_ms = config.happy_eyeballs_delay_ms,
        "happy_eyeballs: racing IPv6 (preferred) and IPv4 (staggered)"
    );

    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel::<SocketAddr>(1);

    // Spawn IPv6 task (immediate)
    let ipv6_handle_clone = network_handle.clone();
    let ipv6_did = did.clone();
    let ipv6_tx = result_tx.clone();
    let ipv6_task = tokio::spawn(async move {
        if let Ok(Ok(_)) =
            tokio::time::timeout(dial_timeout, ipv6_handle_clone.dial(ipv6_addr, ipv6_did)).await
        {
            let _ = ipv6_tx.send(ipv6_addr).await;
        }
    });

    // Spawn IPv4 task (staggered)
    let ipv4_handle_clone = network_handle.clone();
    let ipv4_did = did.clone();
    let ipv4_tx = result_tx.clone();
    let ipv4_task = tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        if let Ok(Ok(_)) =
            tokio::time::timeout(dial_timeout, ipv4_handle_clone.dial(ipv4_addr, ipv4_did)).await
        {
            let _ = ipv4_tx.send(ipv4_addr).await;
        }
    });

    // Drop the original sender so the channel closes when both tasks finish
    drop(result_tx);

    // Wait for first success or both tasks finishing
    let winner = result_rx.recv().await;

    // Cancel the other task (best-effort; the task is already detached but the result is ignored)
    ipv6_task.abort();
    ipv4_task.abort();

    if let Some(addr) = winner {
        info!(%did, %addr, "happy_eyeballs: connected");
    } else {
        debug!(%did, "happy_eyeballs: both IPv4 and IPv6 failed");
    }

    winner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addr_type_display() {
        assert_eq!(AddrType::Local.to_string(), "local");
        assert_eq!(AddrType::Public.to_string(), "public");
        assert_eq!(AddrType::Relay.to_string(), "relay");
    }

    #[test]
    fn test_default_config() {
        let config = NatDialConfig::default();
        assert!(config.parallel_dial);
        assert_eq!(config.local_dial_timeout_ms, 2000);
        assert_eq!(config.public_dial_timeout_ms, 10000);
        assert_eq!(config.relay_dial_timeout_ms, 30000);
        assert_eq!(config.candidate_announce_interval_secs, 150);
        assert_eq!(config.happy_eyeballs_delay_ms, 250);
    }

    #[test]
    fn test_happy_eyeballs_delay_default() {
        let config = NatDialConfig::default();
        assert_eq!(
            config.happy_eyeballs_delay_ms, 250,
            "RFC 8305 §5 mandates 250ms stagger"
        );
    }

    #[test]
    fn test_ipv6_sort_order() {
        // Verify that our sort puts IPv6 first
        let mut addrs: Vec<SocketAddr> = vec![
            "192.168.1.1:7777".parse().unwrap(), // IPv4
            "[::1]:7777".parse().unwrap(),       // IPv6
            "10.0.0.1:7777".parse().unwrap(),    // IPv4
            "[fe80::1]:7777".parse().unwrap(),   // IPv6
        ];
        addrs.sort_by_key(|a| !a.is_ipv6());

        assert!(addrs[0].is_ipv6(), "first address must be IPv6");
        assert!(addrs[1].is_ipv6(), "second address must be IPv6");
        assert!(addrs[2].is_ipv4(), "third address must be IPv4");
        assert!(addrs[3].is_ipv4(), "fourth address must be IPv4");
    }
}
