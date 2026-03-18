//! Gossip send callback initialization
//!
//! Creates the send callback for gossip messages with E2E encryption support.
//! This module handles:
//! - Signed envelope creation for outgoing gossip messages
//! - E2E encryption for unicast messages to capable peers
//! - Encryption sequence tracking and cleanup
//! - Metrics for message types and encryption status

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

use icn_gossip::SendMessageCallback;
use icn_identity::{Did, KeyPair};
use icn_net::NetworkHandle;

/// Dependencies for initializing the send callback
pub struct SendCallbackDeps {
    /// Network handle for sending messages
    pub network_handle: NetworkHandle,
    /// Node's DID
    pub own_did: Did,
    /// Node's signing keypair
    pub keypair: KeyPair,
    /// Node's X25519 secret for encryption
    pub x25519_secret_bytes: [u8; 32],
    /// Gossip store for encryption sequence tracking
    pub gossip_store: Arc<dyn icn_store::Store>,
    /// Whether E2E encryption is enabled
    pub encryption_enabled: bool,
    /// Circuit breaker threshold for cleanup failures
    pub circuit_breaker_threshold: u32,
    /// Shutdown signal sender
    pub shutdown_tx: BroadcastSender<()>,
}

/// Services created during send callback initialization
pub struct SendCallbackServices {
    /// The send callback for gossip messages
    pub send_callback: SendMessageCallback,
    /// Encryption sequence tracker (for metrics/monitoring)
    pub encryption_sequence_tracker: Arc<icn_net::OutgoingSequenceTracker>,
}

/// Initialize the gossip send callback with E2E encryption support
///
/// This creates:
/// - A signed envelope wrapper for all outgoing gossip messages
/// - E2E encryption for unicast messages to peers that support it
/// - Background cleanup task for stale encryption sequences
///
/// The callback follows fail-closed semantics: if encryption fails for a peer
/// that supports it, the message is dropped rather than sent in plaintext.
pub async fn init_send_callback(
    deps: SendCallbackDeps,
    background_tasks: &mut JoinSet<()>,
) -> Result<SendCallbackServices> {
    let sequence_counter = Arc::new(AtomicU64::new(0));

    // Create encryption sequence tracker
    let encryption_sequence_tracker = Arc::new(
        icn_net::OutgoingSequenceTracker::new(deps.gossip_store)
            .context("Failed to create encryption sequence tracker")?,
    );

    // Initialize encryption if enabled
    if deps.encryption_enabled {
        info!("E2E encryption enabled - messages to capable peers will be encrypted");

        // Initialize sequence tracker synchronously on startup
        encryption_sequence_tracker
            .load_and_apply_safety_gap()
            .await
            .context("Failed to apply encryption sequence safety gap")?;

        // Run initial cleanup
        if let Err(e) = encryption_sequence_tracker
            .cleanup_stale_entries(86400)
            .await
        {
            warn!("Initial encryption sequence cleanup failed: {}", e);
        }

        // Spawn periodic cleanup task
        spawn_encryption_cleanup_task(
            encryption_sequence_tracker.clone(),
            deps.shutdown_tx.subscribe(),
            deps.circuit_breaker_threshold,
            background_tasks,
        );
    }

    // Create the send callback
    let send_callback = create_send_callback(
        deps.network_handle,
        deps.own_did,
        deps.keypair,
        deps.x25519_secret_bytes,
        sequence_counter,
        encryption_sequence_tracker.clone(),
        deps.encryption_enabled,
    );

    Ok(SendCallbackServices {
        send_callback,
        encryption_sequence_tracker,
    })
}

/// Create the gossip send callback closure
fn create_send_callback(
    network_handle: NetworkHandle,
    own_did: Did,
    keypair: KeyPair,
    x25519_secret_bytes: [u8; 32],
    sequence_counter: Arc<AtomicU64>,
    encryption_sequence_tracker: Arc<icn_net::OutgoingSequenceTracker>,
    encryption_enabled: bool,
) -> SendMessageCallback {
    Arc::new(move |recipient, gossip_msg| {
        let net_handle = network_handle.clone();
        let from_did = own_did.clone();
        let keypair = keypair.clone();
        let sequence_ctr = sequence_counter.clone();
        let enc_seq_tracker = encryption_sequence_tracker.clone();
        let x25519_secret = x25519_dalek::StaticSecret::from(x25519_secret_bytes);

        // Track metrics based on message type
        use icn_gossip::GossipMessage;
        match &gossip_msg {
            GossipMessage::Announce { .. } => icn_obs::metrics::gossip::announces_sent_inc(),
            GossipMessage::Request { .. } => icn_obs::metrics::gossip::requests_sent_inc(),
            GossipMessage::Response { .. } => icn_obs::metrics::gossip::responses_sent_inc(),
            _ => {} // Other message types
        }

        // Spawn async task to send message
        tokio::spawn(async move {
            // Get next sequence number for signed envelope
            let sequence = sequence_ctr.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            // Create inner signed envelope with gossip payload
            let inner_envelope = match icn_net::SignedEnvelope::from_payload(
                &from_did,
                &keypair,
                sequence,
                icn_net::PayloadType::Gossip,
                &gossip_msg,
            ) {
                Ok(env) => env,
                Err(e) => {
                    warn!("Failed to create signed envelope: {}", e);
                    return;
                }
            };

            // Determine if we should encrypt (unicast + encryption enabled + peer supports it)
            let should_encrypt = if let Some(ref target_did) = recipient {
                encryption_enabled
                    && net_handle
                        .peer_has_capability(target_did, icn_net::CapabilityFlags::E2E_ENCRYPTION)
                        .await
            } else {
                false // Broadcast cannot be encrypted
            };

            let result = if let Some(target_did) = recipient {
                // Unicast
                if should_encrypt {
                    // Try to encrypt the message
                    match super::bridge::try_encrypt_envelope(
                        &net_handle,
                        &from_did,
                        &target_did,
                        &inner_envelope,
                        &keypair,
                        &x25519_secret,
                        &enc_seq_tracker,
                        sequence,
                    )
                    .await
                    {
                        Ok(outer_envelope) => {
                            debug!("Sending encrypted gossip to {}", target_did);
                            icn_obs::metrics::network::encrypted_messages_sent_inc();
                            let net_msg = icn_net::NetworkMessage::signed(
                                Some(target_did.clone()),
                                outer_envelope,
                            );
                            net_handle.send_message(target_did, net_msg).await
                        }
                        Err(e) => {
                            // Fail-closed: drop the message rather than transmit plaintext
                            error!(
                                "Encryption failed for {}, dropping message (fail-closed): {}",
                                target_did, e
                            );
                            icn_obs::metrics::network::encryption_failed_inc("encryption_error");
                            // Return Ok to avoid triggering retry logic - this is intentional drop
                            Ok(())
                        }
                    }
                } else {
                    // Send unencrypted (peer doesn't support E2E or encryption disabled)
                    let net_msg =
                        icn_net::NetworkMessage::signed(Some(target_did.clone()), inner_envelope);
                    net_handle.send_message(target_did, net_msg).await
                }
            } else {
                // Broadcast (cannot be encrypted)
                let net_msg = icn_net::NetworkMessage::signed(None, inner_envelope);
                net_handle.broadcast(net_msg).await
            };

            if let Err(e) = result {
                warn!("Failed to send gossip message: {}", e);
            }
        });
    })
}

/// Spawn the periodic encryption sequence cleanup task
///
/// This task runs hourly to clean up stale sequence entries, preventing
/// unbounded memory growth. It includes a circuit breaker that escalates
/// to ERROR level logging after consecutive failures.
fn spawn_encryption_cleanup_task(
    tracker: Arc<icn_net::OutgoingSequenceTracker>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    circuit_breaker_threshold: u32,
    background_tasks: &mut JoinSet<()>,
) {
    background_tasks.spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        let mut consecutive_failures: u32 = 0;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match tracker.cleanup_stale_entries(86400).await {
                        Ok(removed) => {
                            if removed > 0 {
                                debug!("Cleaned up {} stale encryption sequence entries", removed);
                            }
                            consecutive_failures = 0; // Reset on success

                            // Update gauge with current pair count
                            let pair_count = tracker.pair_count().await;
                            icn_obs::metrics::network::encryption_sequence_pairs_set(pair_count as u64);
                        }
                        Err(e) => {
                            consecutive_failures += 1;
                            icn_obs::metrics::network::encryption_sequence_cleanup_failed_inc();

                            if consecutive_failures >= circuit_breaker_threshold {
                                // Circuit breaker tripped
                                error!(
                                    consecutive_failures = consecutive_failures,
                                    "Encryption sequence cleanup has failed {} consecutive times! \
                                     Storage may be degraded. Sequence tracker memory may grow unbounded. \
                                     Error: {}",
                                    consecutive_failures, e
                                );
                                icn_obs::metrics::network::encryption_circuit_breaker_trips_inc();
                            } else {
                                warn!(
                                    consecutive_failures = consecutive_failures,
                                    threshold = circuit_breaker_threshold,
                                    "Encryption sequence cleanup failed ({}/{}): {}",
                                    consecutive_failures, circuit_breaker_threshold, e
                                );
                            }
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    debug!("Running final encryption sequence cleanup before shutdown");
                    if let Err(e) = tracker.cleanup_stale_entries(86400).await {
                        warn!("Final encryption sequence cleanup failed: {}", e);
                    }
                    debug!("Encryption sequence cleanup task shutting down");
                    break;
                }
            }
        }
    });
}
