//! Gossip-network bridging and bootstrap peer utilities
//!
//! This module handles:
//! - Parsing bootstrap peer URLs
//! - DNS resolution for peer addresses
//! - End-to-end envelope encryption for gossip messages

use anyhow::{Context, Result};
use icn_identity::Did;
use std::net::SocketAddr;
use tracing::debug;

/// Parsed bootstrap peer — either with a known DID or address-only
/// (DID learned from QUIC/DID-TLS handshake).
#[derive(Debug, Clone)]
pub enum BootstrapPeer {
    /// Full peer with known DID: `icn://did:icn:PUBKEY@HOST:PORT`
    KnownDid { did: Did, addr: SocketAddr },
    /// Address-only hint — DID learned from handshake: `icn://HOST:PORT`
    AddrOnly { addr: SocketAddr },
}

/// Parse bootstrap peer URL.
///
/// Supported formats:
/// - `icn://did:icn:PUBKEY@HOST:PORT` — verified peer (DID known)
/// - `icn://HOST:PORT` — bootstrap hint (DID learned from handshake)
///
/// Supports both IP addresses and DNS hostnames.
pub async fn parse_bootstrap_peer(url: &str) -> Result<BootstrapPeer> {
    // Check for icn:// prefix
    let body = url
        .strip_prefix("icn://")
        .context("Bootstrap peer URL must start with 'icn://'")?;

    // Check if it contains @ (DID@ADDR format)
    if let Some(at_pos) = body.find('@') {
        let did_str = &body[..at_pos];
        let addr_str = &body[at_pos + 1..];

        // Parse DID
        let did: Did = serde_json::from_value(serde_json::Value::String(did_str.to_string()))
            .context("Failed to parse DID")?;

        // Resolve address
        let addr = if let Ok(sock_addr) = addr_str.parse::<SocketAddr>() {
            sock_addr
        } else {
            resolve_address(addr_str).await?
        };

        Ok(BootstrapPeer::KnownDid { did, addr })
    } else {
        // No @ — address-only bootstrap hint
        let addr = if let Ok(sock_addr) = body.parse::<SocketAddr>() {
            sock_addr
        } else {
            resolve_address(body).await?
        };

        Ok(BootstrapPeer::AddrOnly { addr })
    }
}

/// Resolve a hostname:port string to a SocketAddr using DNS lookup.
///
/// # Arguments
/// * `addr_str` - Address string in format "hostname:port" or "ip:port"
///
/// # Returns
/// * `Ok(SocketAddr)` - The resolved socket address (first result if multiple)
/// * `Err` - If DNS resolution fails or no addresses are returned
pub async fn resolve_address(addr_str: &str) -> Result<SocketAddr> {
    use tokio::net::lookup_host;

    debug!("Resolving DNS for: {addr_str}");

    // Use tokio's async DNS resolution
    let mut addrs = lookup_host(addr_str)
        .await
        .with_context(|| format!("DNS resolution failed for '{addr_str}'"))?;

    // Take the first resolved address
    let addr = addrs
        .next()
        .with_context(|| format!("DNS resolution returned no addresses for '{addr_str}'"))?;

    debug!("Resolved {addr_str} -> {addr}");
    Ok(addr)
}

/// Try to encrypt an inner SignedEnvelope for E2E encryption (Issue #404)
///
/// Creates a sign-encrypt-sign structure:
/// 1. Serialize inner SignedEnvelope
/// 2. Encrypt with recipient's X25519 public key
/// 3. Wrap in outer SignedEnvelope with PayloadType::Encrypted
///
/// # Sequence Numbers
///
/// Three sequence spaces are involved, but only TWO are incremented per message:
///
/// 1. **Signing sequence** (`outer_sequence` parameter):
///    - Per-sender, shared across all recipients
///    - Used for BOTH inner and outer SignedEnvelope (same value)
///    - The inner envelope's sequence doesn't trigger replay check (handled by
///      `handle_signed_inner()`) because the outer envelope already provides protection
///
/// 2. **Encryption nonce** (from `enc_seq_tracker`):
///    - Per-(sender, recipient) pair, persistent across restarts
///    - Used only for ChaCha20-Poly1305 nonce derivation (not for replay protection)
///    - Must be unique per pair to prevent nonce reuse attacks
///
/// This design ensures encrypted messages consume ONE signing sequence (not two),
/// maintaining consistent sequence advancement between encrypted and unencrypted paths.
///
/// # Sequence Allocation Design
///
/// The encryption sequence is allocated AFTER all fallible I/O operations (peer key lookup,
/// serialization) but BEFORE the actual encryption. This is intentional:
///
/// 1. The sequence is required to derive the ChaCha20-Poly1305 nonce
/// 2. Encryption with valid inputs is essentially infallible (ChaCha20 is deterministic)
/// 3. The only remaining operations (bincode serialization, Ed25519 signing) are also
///    effectively infallible with valid inputs
///
/// This ordering minimizes sequence waste: sequences are only consumed when all I/O and
/// validation is complete, and the remaining crypto operations cannot practically fail.
pub async fn try_encrypt_envelope(
    net_handle: &icn_net::NetworkHandle,
    from_did: &Did,
    to_did: &Did,
    inner_envelope: &icn_net::SignedEnvelope,
    keypair: &icn_identity::KeyPair,
    x25519_secret: &x25519_dalek::StaticSecret,
    enc_seq_tracker: &icn_net::OutgoingSequenceTracker,
    outer_sequence: u64,
) -> Result<icn_net::SignedEnvelope> {
    // Phase 1: All fallible I/O operations BEFORE sequence allocation
    // This ensures sequence is only consumed when encryption is highly likely to succeed.

    // Get recipient's X25519 public key (can fail if peer disconnected)
    let recipient_x25519_bytes = net_handle
        .get_peer_x25519_key(to_did)
        .await
        .context("Peer X25519 key not available")?;
    let recipient_x25519_public = x25519_dalek::PublicKey::from(recipient_x25519_bytes);

    // Serialize inner envelope (validates it's serializable before committing sequence)
    let inner_bytes =
        icn_encoding::encode(inner_envelope).context("Failed to serialize inner envelope")?;

    // Phase 2: Sequence allocation - point of no return
    // All operations after this are essentially infallible with valid inputs.
    // Get encryption sequence number (persistent, unique per sender-recipient pair)
    // This is separate from signing sequence - used only for nonce derivation
    let enc_sequence = enc_seq_tracker.next_sequence(from_did, to_did).await?;

    // Encrypt the inner envelope
    let encrypted = icn_net::EncryptedEnvelope::encrypt(
        from_did,
        to_did,
        enc_sequence,
        x25519_secret,
        &recipient_x25519_public,
        &inner_bytes,
    )
    .context("Failed to encrypt envelope")?;

    // Serialize encrypted envelope
    let encrypted_bytes =
        icn_encoding::encode(&encrypted).context("Failed to serialize encrypted envelope")?;

    // Create outer signed envelope with PayloadType::Encrypted
    // Uses signing sequence (outer_sequence) for replay protection, NOT encryption sequence
    let outer_envelope = icn_net::SignedEnvelope::new(
        from_did,
        keypair,
        outer_sequence,
        icn_net::PayloadType::Encrypted,
        encrypted_bytes,
    )
    .context("Failed to create outer signed envelope")?;

    Ok(outer_envelope)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Valid test DID (proper multibase-encoded Ed25519 public key)
    /// Generated from deterministic seed [1u8; 32]
    const TEST_ALICE_DID: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
    /// Generated from deterministic seed [3u8; 32]
    const TEST_BOB_DID: &str = "did:icn:zGyGKxMyg1p9SsHfm15MkNUu1u9TN2JtTspcdmrtGUdse";

    #[tokio::test]
    async fn test_parse_bootstrap_peer_valid() {
        let url = format!("icn://{TEST_ALICE_DID}@203.0.113.50:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_ok());

        match result.unwrap() {
            BootstrapPeer::KnownDid { did, addr } => {
                assert_eq!(did.as_str(), TEST_ALICE_DID);
                assert_eq!(addr.to_string(), "203.0.113.50:7777");
            }
            other => panic!("Expected KnownDid, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_ipv4() {
        let url = format!("icn://{TEST_BOB_DID}@192.168.1.100:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_ok());

        match result.unwrap() {
            BootstrapPeer::KnownDid { did, addr } => {
                assert_eq!(did.as_str(), TEST_BOB_DID);
                assert_eq!(addr.to_string(), "192.168.1.100:7777");
            }
            other => panic!("Expected KnownDid, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_missing_prefix() {
        let url = format!("{TEST_ALICE_DID}@203.0.113.50:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must start with 'icn://'"));
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_addr_only() {
        // No @ sign means addr-only format
        let url = "icn://203.0.113.50:7777";
        let result = parse_bootstrap_peer(url).await;
        assert!(result.is_ok());

        match result.unwrap() {
            BootstrapPeer::AddrOnly { addr } => {
                assert_eq!(addr.to_string(), "203.0.113.50:7777");
            }
            other => panic!("Expected AddrOnly, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_addr_only_hostname() {
        let url = "icn://localhost:7777";
        let result = parse_bootstrap_peer(url).await;
        assert!(result.is_ok(), "Failed to resolve localhost: {result:?}");

        match result.unwrap() {
            BootstrapPeer::AddrOnly { addr } => {
                assert!(addr.ip().is_loopback());
                assert_eq!(addr.port(), 7777);
            }
            other => panic!("Expected AddrOnly, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_dns_resolution() {
        // Test DNS resolution with localhost (should resolve)
        let url = format!("icn://{TEST_ALICE_DID}@localhost:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_ok(), "Failed to resolve localhost: {result:?}");

        match result.unwrap() {
            BootstrapPeer::KnownDid { did, addr } => {
                assert_eq!(did.as_str(), TEST_ALICE_DID);
                assert!(
                    addr.ip().is_loopback(),
                    "Expected loopback address, got: {}",
                    addr.ip()
                );
                assert_eq!(addr.port(), 7777);
            }
            other => panic!("Expected KnownDid, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_dns_failure() {
        // Test DNS resolution failure with a guaranteed-invalid hostname (RFC 6761)
        let url = format!("icn://{TEST_ALICE_DID}@test.invalid:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("DNS resolution failed"));
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_ipv6() {
        // Test IPv6 address parsing (fast path, no DNS needed)
        let url = format!("icn://{TEST_ALICE_DID}@[::1]:7777");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_ok(), "Failed to parse IPv6 address: {result:?}");

        match result.unwrap() {
            BootstrapPeer::KnownDid { did, addr } => {
                assert_eq!(did.as_str(), TEST_ALICE_DID);
                assert!(addr.ip().is_loopback());
                assert_eq!(addr.port(), 7777);
            }
            other => panic!("Expected KnownDid, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_bootstrap_peer_invalid_port() {
        let url = format!("icn://{TEST_ALICE_DID}@203.0.113.50:invalid");
        let result = parse_bootstrap_peer(&url).await;
        assert!(result.is_err());
        // With invalid port, DNS resolution will fail
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("DNS resolution failed"));
    }
}
