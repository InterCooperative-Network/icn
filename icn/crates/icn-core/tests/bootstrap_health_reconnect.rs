#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The bootstrap health task must bring a DID-named bootstrap peer back after an outage —
//! through a DID-verified redial — and must stop cleanly on shutdown.
//!
//! This drives the production pieces the daemon wires together: `known_did_bootstrap_peers`
//! turns the operator's bootstrap URLs into health-task targets, and
//! `spawn_bootstrap_health_task` keeps them connected against real `NetworkActor`s. Before it
//! was wired in, two nodes that lost their link stayed apart until one of them restarted.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use icn_core::supervisor::background_tasks::{spawn_bootstrap_health_task, BootstrapHealthConfig};
use icn_core::supervisor::init_bootstrap::{known_did_bootstrap_peers, BootstrapConfig};
use icn_identity::{Did, IdentityBundle};
use icn_net::{IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMessage};
use metrics::with_local_recorder;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use tokio::sync::broadcast;

const MISMATCH_METRIC: &str = "icn_network_bootstrap_did_expectation_mismatch_total";

fn init() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}

fn fast_health() -> BootstrapHealthConfig {
    BootstrapHealthConfig {
        check_interval: Duration::from_millis(300),
        reconnect_timeout: Duration::from_secs(3),
        max_failures: 3,
    }
}

fn bootstrap_config(urls: Vec<String>) -> BootstrapConfig {
    BootstrapConfig {
        bootstrap_peers: urls,
        federation_enabled: false,
        network_name: "test".to_string(),
        peer_exchange_delay_ms: 0,
        peer_exchange_max_peers: 0,
    }
}

/// A node bound to `addr`; retries briefly because a just-released port can take a moment.
async fn spawn_node_at(
    bundle: IdentityBundle,
    addr: SocketAddr,
) -> anyhow::Result<(NetworkHandle, broadcast::Sender<()>)> {
    let (shutdown_tx, _) = broadcast::channel(1);
    let handler: IncomingMessageHandler = Arc::new(|_msg: NetworkMessage| {});
    let mut last_err = None;
    for _ in 0..20 {
        match NetworkActor::spawn(
            bundle.clone(),
            addr,
            shutdown_tx.clone(),
            Some(handler.clone()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        {
            Ok(handle) => {
                tokio::time::sleep(Duration::from_millis(300)).await;
                return Ok((handle, shutdown_tx));
            }
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("could not bind {addr}")))
}

fn free_addr() -> SocketAddr {
    let port = portpicker::pick_unused_port().expect("no free port");
    format!("127.0.0.1:{port}").parse().unwrap()
}

async fn wait_for(observer: &NetworkHandle, did: &Did, connected: bool, within: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        if observer.is_peer_connected(did).await.unwrap() == connected {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[test]
fn address_only_entries_and_our_own_did_are_not_health_targets() {
    let did = IdentityBundle::generate().unwrap().did().clone();
    let own = IdentityBundle::generate().unwrap().did().clone();
    let config = bootstrap_config(vec![
        format!("icn://{did}@127.0.0.1:7777"),
        "icn://127.0.0.1:7778".to_string(),
        format!("icn://{own}@127.0.0.1:7779"),
    ]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let peers = runtime.block_on(known_did_bootstrap_peers(&config, &own));
    assert_eq!(peers, vec![(did, "127.0.0.1:7777".parse().unwrap())]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_bootstrap_peer_is_redialled_and_the_task_stops_on_shutdown() -> anyhow::Result<()> {
    init();
    let peer_bundle = IdentityBundle::generate()?;
    let peer_did = peer_bundle.did().clone();
    let peer_addr = free_addr();
    let (node, _node_shutdown) = spawn_node_at(IdentityBundle::generate()?, free_addr()).await?;
    let (_peer, peer_shutdown) = spawn_node_at(peer_bundle.clone(), peer_addr).await?;

    let targets = known_did_bootstrap_peers(
        &bootstrap_config(vec![format!("icn://{peer_did}@{peer_addr}")]),
        &IdentityBundle::generate()?.did().clone(),
    )
    .await;
    let (task_shutdown, task_shutdown_rx) = broadcast::channel(1);
    let task =
        spawn_bootstrap_health_task(fast_health(), targets, node.clone(), None, task_shutdown_rx);

    // No startup dial here: the task alone must establish the session.
    assert!(
        wait_for(&node, &peer_did, true, Duration::from_secs(15)).await,
        "the health task must connect a configured peer it finds disconnected"
    );

    // Outage: the peer goes away and its connection ends.
    let _ = peer_shutdown.send(());
    assert!(
        wait_for(&node, &peer_did, false, Duration::from_secs(15)).await,
        "the ended connection must stop counting as connected"
    );

    // The peer returns at its configured address; the task redials without any manual step.
    let (_peer_again, _peer_again_shutdown) = spawn_node_at(peer_bundle, peer_addr).await?;
    assert!(
        wait_for(&node, &peer_did, true, Duration::from_secs(15)).await,
        "the health task must redial the configured peer after it returns"
    );
    assert!(node.get_peer_connection_info(&peer_did).await.is_some());

    // Clean teardown: the task honours shutdown promptly.
    let _ = task_shutdown.send(());
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("the health task must exit on shutdown")?;
    Ok(())
}

fn counter(snapshotter: &Snapshotter, name: &str) -> u64 {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter_map(|(key, _, _, value)| match value {
            DebugValue::Counter(v) if key.key().name() == name => Some(v),
            _ => None,
        })
        .sum()
}

fn gauge(snapshotter: &Snapshotter, name: &str) -> Option<f64> {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .find_map(|(key, _, _, value)| match value {
            DebugValue::Gauge(v) if key.key().name() == name => Some(v.into_inner()),
            _ => None,
        })
}

fn mismatch_total(snapshotter: &Snapshotter) -> u64 {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter_map(|(key, _, _, value)| match value {
            DebugValue::Counter(v) if key.key().name() == MISMATCH_METRIC => Some(v),
            _ => None,
        })
        .sum()
}

/// A different node answering the configured endpoint is an identity mismatch, not health.
///
/// Three things are pinned. The redial carries the configured expectation, so the divergence is
/// recorded (#2533). The peer is not counted as connected. And the endpoint is not redialled
/// while the other node holds it: every redial would open another connection that
/// authenticates the same wrong node, so the divergence count is exactly one.
#[test]
fn a_wrong_did_at_the_configured_endpoint_is_a_mismatch_not_health() -> anyhow::Result<()> {
    mismatch_case(free_addr())
}

/// The same, with the observing node on a dual-stack socket — the production default
/// (`listen_addr = "[::]:7777"`). Its sessions to the configured IPv4 endpoint report the peer as
/// `::ffff:a.b.c.d`, which must still be recognised as the configured endpoint.
#[test]
fn a_wrong_did_is_a_mismatch_on_a_dual_stack_node() -> anyhow::Result<()> {
    let port = portpicker::pick_unused_port().expect("no free port");
    mismatch_case(format!("[::]:{port}").parse().unwrap())
}

fn mismatch_case(node_bind: SocketAddr) -> anyhow::Result<()> {
    init();
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    with_local_recorder(&recorder, || {
        runtime.block_on(async {
            let configured = IdentityBundle::generate()?.did().clone();
            let answering = IdentityBundle::generate()?;
            let answering_did = answering.did().clone();
            let addr = free_addr();
            let (node, _node_shutdown) =
                spawn_node_at(IdentityBundle::generate()?, node_bind).await?;
            let (_peer, _peer_shutdown) = spawn_node_at(answering, addr).await?;

            let (task_shutdown, rx) = broadcast::channel(1);
            let task = spawn_bootstrap_health_task(
                fast_health(),
                vec![(configured, addr)],
                node.clone(),
                None,
                rx,
            );
            let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
            while node
                .get_peer_connection_info(&answering_did)
                .await
                .is_none()
            {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "precondition: the answering node never authenticated"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            // Ten more checks. The configured endpoint is reached — by a different DID — so the
            // task must not keep opening fresh connections to it: each one would authenticate
            // the same node again and record another divergence.
            tokio::time::sleep(Duration::from_secs(3)).await;
            let _ = task_shutdown.send(());
            let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
            anyhow::Ok(())
        })
    })?;
    let mismatches = mismatch_total(&snapshotter);
    assert_eq!(
        mismatches, 1,
        "exactly one connection may reach the wrong node: the endpoint must not be redialled \
         while a different DID holds it ({mismatches} connections re-authenticated it)"
    );
    assert_eq!(
        gauge(&snapshotter, "icn_bootstrap_peers_connected"),
        Some(0.0),
        "a wrong DID at the configured endpoint must not count as the peer being connected"
    );
    assert_eq!(
        gauge(&snapshotter, "icn_bootstrap_peers_disconnected"),
        Some(1.0)
    );
    Ok(())
}

/// An unreachable bootstrap peer is retried with backoff, and each attempt's hold on the
/// network actor is bounded by the dial timeout.
///
/// The network actor awaits dials inline, so while a reconnection attempt to a dead endpoint is
/// in flight, every other request to the actor waits. This measures that stall (printed) and pins
/// the two bounds that keep it from becoming a wedge: attempts back off, and the stall per
/// attempt does not exceed the dial timeout by more than scheduling slack.
#[test]
fn an_unreachable_bootstrap_peer_is_retried_with_backoff_and_bounded_actor_stalls(
) -> anyhow::Result<()> {
    init();
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let dial_timeout = Duration::from_millis(1000);
    let max_stall = with_local_recorder(&recorder, || {
        runtime.block_on(async {
            let (node, _node_shutdown) =
                spawn_node_at(IdentityBundle::generate()?, free_addr()).await?;
            node.set_dial_timeout(dial_timeout);
            let dead = (IdentityBundle::generate()?.did().clone(), free_addr());
            let (task_shutdown, rx) = broadcast::channel(1);
            let task = spawn_bootstrap_health_task(
                BootstrapHealthConfig {
                    check_interval: Duration::from_millis(200),
                    reconnect_timeout: Duration::from_millis(1500),
                    max_failures: 3,
                },
                vec![dead],
                node.clone(),
                None,
                rx,
            );

            let mut max_stall = Duration::ZERO;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
            while tokio::time::Instant::now() < deadline {
                let started = tokio::time::Instant::now();
                node.get_peers().await?;
                max_stall = max_stall.max(started.elapsed());
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let _ = task_shutdown.send(());
            tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .expect("the health task must exit on shutdown")?;
            anyhow::Ok(max_stall)
        })
    })?;
    let attempts = counter(&snapshotter, "icn_nat_dial_failure_total");
    eprintln!(
        "measured: {attempts} reconnection attempts in 12s; longest network-actor stall \
         {max_stall:?} (dial timeout {dial_timeout:?})"
    );
    assert!(
        attempts >= 2,
        "the dead peer must be retried at all ({attempts})"
    );
    assert!(
        attempts <= 6,
        "retries must back off: {attempts} attempts in 12s at a 200ms check interval"
    );
    assert!(
        max_stall < dial_timeout + Duration::from_millis(750),
        "an attempt held the network actor for {max_stall:?}, beyond its {dial_timeout:?} \
         dial timeout"
    );
    Ok(())
}

/// A QUIC listener that completes the transport handshake and then says nothing: no Hello, so
/// no identity is ever proven. Connections are held open until the returned guard drops.
async fn silent_peer(addr: SocketAddr) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let mut manager = icn_net::SessionManager::new();
    manager
        .start(&IdentityBundle::generate()?, addr, None, None)
        .await?;
    Ok(tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok(Some(connection)) = manager.accept().await {
            held.push(connection); // accepted, never spoken to
        }
    }))
}

/// Transport that comes up but never authenticates the configured peer must back off.
///
/// The dial *succeeds* every time — the listener accepts — so a backoff that only counted
/// failed dials would redial every other check forever, leaving a new unauthenticated
/// connection behind each time.
#[test]
fn a_reachable_endpoint_that_never_authenticates_is_retried_with_backoff() -> anyhow::Result<()> {
    init();
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    with_local_recorder(&recorder, || {
        runtime.block_on(async {
            let peer_addr = free_addr();
            let silent = silent_peer(peer_addr).await?;
            let (node, _node_shutdown) =
                spawn_node_at(IdentityBundle::generate()?, free_addr()).await?;
            let configured = IdentityBundle::generate()?.did().clone();
            let (task_shutdown, rx) = broadcast::channel(1);
            let task = spawn_bootstrap_health_task(
                BootstrapHealthConfig {
                    check_interval: Duration::from_millis(200),
                    reconnect_timeout: Duration::from_secs(2),
                    max_failures: 3,
                },
                vec![(configured.clone(), peer_addr)],
                node.clone(),
                None,
                rx,
            );
            tokio::time::sleep(Duration::from_secs(12)).await;
            assert!(
                !node.is_peer_connected(&configured).await?,
                "an endpoint that never proves the configured DID is not that peer"
            );
            let _ = task_shutdown.send(());
            let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
            silent.abort();
            anyhow::Ok(())
        })
    })?;
    let attempts = counter(&snapshotter, "icn_nat_dial_success_total");
    eprintln!("measured: {attempts} transport-successful redials in 12s to a silent endpoint");
    assert!(
        attempts >= 2,
        "the endpoint must be retried at all ({attempts})"
    );
    assert!(
        attempts <= 6,
        "successful-but-unauthenticated dials must back off: {attempts} redials in 12s at a \
         200ms check interval"
    );
    Ok(())
}

/// Shutdown is honoured while a reconnection attempt is in flight, not only between checks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_interrupts_an_in_flight_reconnection_attempt() -> anyhow::Result<()> {
    init();
    let (node, _node_shutdown) = spawn_node_at(IdentityBundle::generate()?, free_addr()).await?;
    node.set_dial_timeout(Duration::from_secs(30));
    let dead = (IdentityBundle::generate()?.did().clone(), free_addr());
    let (task_shutdown, rx) = broadcast::channel(1);
    let task = spawn_bootstrap_health_task(
        BootstrapHealthConfig {
            check_interval: Duration::from_millis(100),
            reconnect_timeout: Duration::from_secs(30),
            max_failures: 3,
        },
        vec![dead],
        node.clone(),
        None,
        rx,
    );
    // The first check (after one interval) starts a dial that would take up to 30s.
    tokio::time::sleep(Duration::from_millis(600)).await;
    let asked = tokio::time::Instant::now();
    let _ = task_shutdown.send(());
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("shutdown must not wait for an in-flight reconnection attempt")?;
    eprintln!(
        "measured: health task exited {:?} after shutdown",
        asked.elapsed()
    );
    Ok(())
}
