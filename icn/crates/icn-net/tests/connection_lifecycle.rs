#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A connection that ends must stop counting as a session, and the same peer must be able to
//! come back through an authenticated redial.
//!
//! Before this was pinned, a connection whose handler had exited — the peer went away, or the
//! QUIC idle timeout fired — stayed in the session map, and `is_peer_connected` answered from
//! `peer_connections`, which outlives connections by design. So a departed peer looked
//! connected forever and nothing redialled it. These tests run the real actor end to end: the
//! connection handler's exit is what must evict, and the Hello's DID-TLS binding is what must
//! re-authenticate the returning peer.

use anyhow::Result;
use icn_identity::{Did, IdentityBundle};
use icn_net::{IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMessage};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

static ISSUED_PORTS: std::sync::Mutex<Option<std::collections::HashSet<u16>>> =
    std::sync::Mutex::new(None);

fn pick_port() -> u16 {
    let mut guard = ISSUED_PORTS.lock().expect("issued-port lock poisoned");
    let issued = guard.get_or_insert_with(std::collections::HashSet::new);
    for _ in 0..200 {
        let port = portpicker::pick_unused_port().expect("no free port");
        if issued.insert(port) {
            return port;
        }
    }
    panic!("could not find an unissued free port");
}

fn init() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}

/// Stops its node when dropped.
struct Node {
    handle: NetworkHandle,
    addr: SocketAddr,
    shutdown: broadcast::Sender<()>,
}

impl Node {
    async fn spawn(bundle: IdentityBundle) -> Result<Self> {
        let (shutdown, _) = broadcast::channel(1);
        let handler: IncomingMessageHandler = Arc::new(|_msg: NetworkMessage| {});
        let mut last_err = None;
        for _ in 0..8 {
            let addr: SocketAddr = format!("127.0.0.1:{}", pick_port()).parse()?;
            match NetworkActor::spawn(
                bundle.clone(),
                addr,
                shutdown.clone(),
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
                    return Ok(Self {
                        handle,
                        addr,
                        shutdown,
                    });
                }
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("could not bind a node")))
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
    }
}

async fn wait_for_connected(observer: &NetworkHandle, did: &Did, want: bool) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        if observer.is_peer_connected(did).await.unwrap() == want {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_for_active_connections(observer: &NetworkHandle, want: usize) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        if observer.get_stats().await.unwrap().connections_active == want {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_ended_connection_stops_counting_and_an_authenticated_redial_restores_it() -> Result<()>
{
    init();
    let survivor_bundle = IdentityBundle::generate()?;
    let departing_bundle = IdentityBundle::generate()?;
    let departing_did = departing_bundle.did().clone();

    let survivor = Node::spawn(survivor_bundle).await?;
    let departing = Node::spawn(departing_bundle.clone()).await?;

    survivor
        .handle
        .dial_expecting(departing.addr, departing_did.clone())
        .await?;
    assert!(
        wait_for_connected(&survivor.handle, &departing_did, true).await,
        "precondition: the peer must authenticate first"
    );

    // The peer goes away. Its connection ends; the survivor's handler for it exits.
    drop(departing);
    assert!(
        wait_for_connected(&survivor.handle, &departing_did, false).await,
        "an ended connection must stop counting as connected"
    );
    assert!(
        !survivor
            .handle
            .connected_peers()
            .await
            .contains(&departing_did),
        "and must not be listed as a connected peer"
    );
    // `connected_peers` and `is_peer_connected` both skip closed entries, so neither can tell an
    // evicted connection from one left behind. `connections_active` counts the session map
    // unfiltered — the view peer exchange discloses from, and the one periodic traffic iterates.
    assert!(
        wait_for_active_connections(&survivor.handle, 0).await,
        "the ended connection must be evicted from the session map, not merely be closed in it"
    );
    assert!(
        survivor
            .handle
            .get_peer_connection_info(&departing_did)
            .await
            .is_some(),
        "peer_connections rows outlive connections by design; liveness must not depend on them"
    );

    // The same identity returns at a new address; a DID-verified redial reconnects it.
    let returning = Node::spawn(departing_bundle).await?;
    survivor
        .handle
        .dial_expecting(returning.addr, departing_did.clone())
        .await?;
    assert!(
        wait_for_connected(&survivor.handle, &departing_did, true).await,
        "an authenticated redial must restore the session"
    );
    assert_eq!(survivor.handle.connected_peers().await, vec![departing_did]);
    Ok(())
}
