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

/// Dial local and public address categories in parallel using Happy Eyeballs within each.
///
/// Each endpoint category (Local, Public) is raced via `dial_happy_eyeballs()`, which
/// handles IPv4/IPv6 racing per RFC 8305. The two category tasks are then raced against
/// each other: first success wins. Relay is intentionally excluded here — it is handled
/// by the caller (`dial_with_fallback`) as a last-resort fallback.
///
/// Composition:
/// - `dial_parallel` races **across** endpoint kinds (Local vs Public)
/// - `dial_happy_eyeballs` races **within** a kind (IPv6 vs IPv4)
async fn dial_parallel(
    network_handle: &NetworkHandle,
    candidate: &ConnectionCandidate,
    config: &NatDialConfig,
) -> Result<(AddrType, SocketAddr), Vec<DialError>> {
    use icn_net::candidate::EndpointKind;

    let did = &candidate.did;
    let local_timeout = Duration::from_millis(config.local_dial_timeout_ms);
    let public_timeout = Duration::from_millis(config.public_dial_timeout_ms);

    let local_addrs: Vec<SocketAddr> = candidate.endpoints_of_kind(EndpointKind::Local).collect();
    let public_addrs: Vec<SocketAddr> = candidate.endpoints_of_kind(EndpointKind::Public).collect();

    if local_addrs.is_empty() && public_addrs.is_empty() {
        return Err(vec![]);
    }

    // Spawn Happy Eyeballs tasks for each category
    let local_task = if !local_addrs.is_empty() {
        icn_obs::metrics::nat::dial_attempt_inc("local");
        let handle = network_handle.clone();
        let d = did.clone();
        let cfg = config.clone();
        Some(tokio::spawn(async move {
            dial_happy_eyeballs(local_addrs, &d, &handle, local_timeout, &cfg).await
        }))
    } else {
        None
    };

    let public_task = if !public_addrs.is_empty() {
        icn_obs::metrics::nat::dial_attempt_inc("public");
        let handle = network_handle.clone();
        let d = did.clone();
        let cfg = config.clone();
        Some(tokio::spawn(async move {
            dial_happy_eyeballs(public_addrs, &d, &handle, public_timeout, &cfg).await
        }))
    } else {
        None
    };

    // Race the two category tasks; first Some(_) wins
    let (local_result, public_result) = match (local_task, public_task) {
        (Some(l), Some(p)) => {
            tokio::select! {
                res = l => {
                    let addr = res.ok().flatten();
                    (addr, None)
                }
                res = p => {
                    let addr = res.ok().flatten();
                    (None, addr)
                }
            }
        }
        (Some(l), None) => (l.await.ok().flatten(), None),
        (None, Some(p)) => (None, p.await.ok().flatten()),
        (None, None) => (None, None),
    };

    if let Some(addr) = local_result {
        return Ok((AddrType::Local, addr));
    }
    if let Some(addr) = public_result {
        return Ok((AddrType::Public, addr));
    }

    // Both categories exhausted — return empty error list (caller collects relay fallback)
    Err(vec![])
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

    // Fast path: only one IP version present — try all addresses in order, no stagger needed
    let has_ipv6 = sorted.iter().any(|a| a.is_ipv6());
    let has_ipv4 = sorted.iter().any(|a| a.is_ipv4());
    if !has_ipv6 || !has_ipv4 {
        icn_obs::metrics::nat::dial_attempt_inc("happy_eyeballs");
        debug!(%did, candidates = sorted.len(), "happy_eyeballs: single IP version, dialing sequentially");
        for addr in &sorted {
            if let Ok(Ok(_)) =
                tokio::time::timeout(dial_timeout, network_handle.dial(*addr, did.clone())).await
            {
                icn_obs::metrics::nat::dial_success_inc(if addr.is_ipv6() {
                    "ipv6"
                } else {
                    "ipv4"
                });
                info!(%did, %addr, "happy_eyeballs: connected");
                return Some(*addr);
            }
        }
        icn_obs::metrics::nat::dial_failure_inc();
        return None;
    }

    // Dual-stack path: IPv6 first (immediate), IPv4 after stagger (RFC 8305 §5)
    let ipv6_addrs: Vec<SocketAddr> = sorted.iter().filter(|a| a.is_ipv6()).copied().collect();
    let ipv4_addrs: Vec<SocketAddr> = sorted.iter().filter(|a| a.is_ipv4()).copied().collect();

    debug!(
        %did,
        ipv6_candidates = ipv6_addrs.len(),
        ipv4_candidates = ipv4_addrs.len(),
        stagger_ms = config.happy_eyeballs_delay_ms,
        "happy_eyeballs: racing IPv6 (preferred) and IPv4 (staggered)"
    );
    icn_obs::metrics::nat::dial_attempt_inc("happy_eyeballs");

    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel::<SocketAddr>(1);

    // Spawn IPv6 task: try all IPv6 addresses in order, send first success
    let ipv6_handle = network_handle.clone();
    let ipv6_did = did.clone();
    let ipv6_tx = result_tx.clone();
    let ipv6_task = tokio::spawn(async move {
        for addr in ipv6_addrs {
            if let Ok(Ok(_)) =
                tokio::time::timeout(dial_timeout, ipv6_handle.dial(addr, ipv6_did.clone())).await
            {
                let _ = ipv6_tx.send(addr).await;
                break;
            }
        }
    });

    // Spawn IPv4 task: sleep stagger, then try all IPv4 addresses in order
    let ipv4_handle = network_handle.clone();
    let ipv4_did = did.clone();
    let ipv4_tx = result_tx.clone();
    let ipv4_task = tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        for addr in ipv4_addrs {
            if let Ok(Ok(_)) =
                tokio::time::timeout(dial_timeout, ipv4_handle.dial(addr, ipv4_did.clone())).await
            {
                let _ = ipv4_tx.send(addr).await;
                break;
            }
        }
    });

    // Drop the original sender so the channel closes when both tasks finish
    drop(result_tx);

    // Wait for first success or both tasks exhausted
    let winner = result_rx.recv().await;

    // Cancel the other task (best-effort)
    ipv6_task.abort();
    ipv4_task.abort();

    if let Some(addr) = winner {
        icn_obs::metrics::nat::dial_success_inc(if addr.is_ipv6() { "ipv6" } else { "ipv4" });
        info!(%did, %addr, "happy_eyeballs: connected");
    } else {
        icn_obs::metrics::nat::dial_failure_inc();
        debug!(%did, "happy_eyeballs: all candidates exhausted");
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
