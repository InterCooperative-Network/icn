//! Graceful shutdown and state snapshot management
//!
//! Handles graceful shutdown of the supervisor including:
//! - Exporting gossip and network state
//! - Saving state snapshots to disk
//! - Cleanup of old snapshots

use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use icn_gossip::GossipActor;
use icn_net::NetworkHandle;
use icn_snapshot::StateSnapshot;

/// Save state snapshot during shutdown
///
/// Exports gossip and network state, saves to disk, and cleans up old snapshots.
/// Errors are logged but do not prevent shutdown.
pub async fn save_shutdown_snapshot(
    gossip_handle: Option<&Arc<RwLock<GossipActor>>>,
    network_handle: Option<&NetworkHandle>,
    data_dir: &Path,
) {
    if gossip_handle.is_none() && network_handle.is_none() {
        return;
    }

    info!("Saving state snapshot before shutdown");

    let snapshot_result = export_state(gossip_handle, network_handle).await;

    match snapshot_result {
        Ok(snapshot) => {
            record_snapshot_metrics(&snapshot);
            save_and_cleanup_snapshot(&snapshot, data_dir);
        }
        Err(e) => {
            warn!("Failed to export actor state during shutdown: {}", e);
            // Continue with shutdown even if state export failed
        }
    }
}

/// Export state from gossip and network actors
async fn export_state(
    gossip_handle: Option<&Arc<RwLock<GossipActor>>>,
    network_handle: Option<&NetworkHandle>,
) -> anyhow::Result<StateSnapshot> {
    let mut snapshot = StateSnapshot::new();

    // Export gossip state
    if let Some(gossip_handle) = gossip_handle {
        let gossip_state = gossip_handle.read().await.export_state();
        info!(
            "Exported gossip state: {} vector clock entries, {} subscriptions",
            gossip_state.vector_clock.len(),
            gossip_state.subscriptions.len()
        );
        snapshot.gossip_state = Some(gossip_state);
    }

    // Export network state
    if let Some(network_handle) = network_handle {
        // SAFETY: Use block_in_place to safely call async export_state from sync context.
        // This moves other tokio tasks off the current thread before blocking.
        // catch_unwind handles any panics from runtime state issues during shutdown.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async { network_handle.export_state().await })
            })
        })) {
            Ok(state) => {
                info!(
                    "Exported network state: {} peer X25519 keys",
                    state.peer_x25519_keys.len()
                );
                snapshot.network_state = Some(state);
            }
            Err(e) => {
                warn!("Failed to export network state (panic): {:?}", e);
            }
        }
    }

    Ok(snapshot)
}

/// Record snapshot metrics for Prometheus
fn record_snapshot_metrics(snapshot: &StateSnapshot) {
    if let Some(ref gossip_state) = snapshot.gossip_state {
        icn_obs::metrics::snapshot::gossip_vector_clock_entries_set(
            gossip_state.vector_clock.len(),
        );
        icn_obs::metrics::snapshot::gossip_subscriptions_set(gossip_state.subscriptions.len());
        icn_obs::metrics::snapshot::gossip_topics_set(gossip_state.topics.len());
    }
    if let Some(ref network_state) = snapshot.network_state {
        icn_obs::metrics::snapshot::network_x25519_keys_set(network_state.peer_x25519_keys.len());
    }
}

/// Save snapshot to disk and cleanup old snapshots
fn save_and_cleanup_snapshot(snapshot: &StateSnapshot, data_dir: &Path) {
    let save_start = std::time::Instant::now();
    let save_result = icn_snapshot::save_snapshot(snapshot, data_dir);
    let save_duration = save_start.elapsed();

    match save_result {
        Ok(()) => {
            icn_obs::metrics::snapshot::save_total_inc();
            icn_obs::metrics::snapshot::save_duration_record(save_duration.as_secs_f64());

            // Record snapshot file size
            let snapshot_path = data_dir.join("state.snapshot");
            if let Ok(metadata) = std::fs::metadata(&snapshot_path) {
                icn_obs::metrics::snapshot::size_bytes_set(metadata.len());
            }

            info!(
                "✅ State snapshot saved to {}/state.snapshot in {:.3}s",
                data_dir.display(),
                save_duration.as_secs_f64()
            );

            // Save timestamped backup for archival
            if let Err(e) = icn_snapshot::save_timestamped_snapshot(snapshot, data_dir) {
                warn!("Failed to save timestamped snapshot backup: {}", e);
            }

            // Cleanup old snapshots (keep last 3)
            match icn_snapshot::cleanup_old_snapshots(data_dir, 3) {
                Ok(deleted) if deleted > 0 => {
                    info!("Cleaned up {} old snapshot(s)", deleted);
                }
                Ok(_) => {}
                Err(e) => {
                    warn!("Failed to cleanup old snapshots: {}", e);
                }
            }
        }
        Err(e) => {
            icn_obs::metrics::snapshot::save_errors_inc();
            warn!("Failed to save state snapshot: {}", e);
        }
    }
}

/// Log shutdown status for actors
pub fn log_actor_shutdown_status(
    network_handle: Option<&NetworkHandle>,
    gossip_handle: Option<&Arc<RwLock<GossipActor>>>,
    ledger_handle: bool,
) {
    if network_handle.is_some() {
        info!("Network actor will shut down via shutdown signal");
    }
    if gossip_handle.is_some() {
        info!("Gossip actor will be dropped when all references are released");
    }
    if ledger_handle {
        info!("Ledger will be dropped when all references are released");
    }
}
