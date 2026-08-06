#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The bootstrap composition path must actually *state* the operator's expectation.
//!
//! `icn-net` can weigh a configured DID against the authenticated one, but only for a dial
//! that declared an expectation. Whether the bootstrap path declares one is a property of
//! `dial_bootstrap_peers`, not of `icn-net` — and it is exactly the kind of property that
//! looks fine in review and is absent at runtime, because the two dial entry points differ
//! by one word at the call site (#2533).
//!
//! So this drives the real `dial_bootstrap_peers` against a real `NetworkActor`, from a
//! bootstrap **URL string**, and asserts on what the node observed. A version of this
//! function that called `dial` instead of `dial_expecting` would still connect, still log
//! "✓ Reached bootstrap peer", and record nothing — which is the regression to catch.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use icn_core::supervisor::init_bootstrap::{dial_bootstrap_peers, BootstrapConfig};
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

fn mismatch_total(snapshotter: &Snapshotter) -> u64 {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter_map(|(key, _, _, value)| {
            if key.key().name() != MISMATCH_METRIC {
                return None;
            }
            match value {
                DebugValue::Counter(v) => Some(v),
                _ => None,
            }
        })
        .sum()
}

/// A current-thread runtime plus a recorder only this thread can see.
///
/// Both are required together: `with_local_recorder` is thread-local, and the mismatch is
/// decided inside a spawned connection-handler task, which only a current-thread runtime
/// polls on the thread that holds the recorder.
fn run_with_local_metrics<F>(body: F) -> anyhow::Result<Snapshotter>
where
    F: std::future::Future<Output = anyhow::Result<()>>,
{
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    with_local_recorder(&recorder, || runtime.block_on(body))?;
    Ok(snapshotter)
}

async fn spawn_node(bundle: IdentityBundle) -> anyhow::Result<(NetworkHandle, SocketAddr, Guard)> {
    let (shutdown_tx, _) = broadcast::channel(1);
    let handler: IncomingMessageHandler = Arc::new(|_msg: NetworkMessage| {});

    let mut last_err = None;
    for _ in 0..8 {
        let port = portpicker::pick_unused_port().expect("no free port");
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
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
                return Ok((handle, addr, Guard(shutdown_tx)));
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("could not bind a node")))
}

struct Guard(broadcast::Sender<()>);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

async fn wait_until_authenticated(observer: &NetworkHandle, did: &Did, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if observer.get_peer_connection_info(did).await.is_some() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
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

/// A `icn://did:icn:X@HOST:PORT` entry whose named DID is not who answers is recorded.
///
/// The whole point of routing this through a URL string is that the URL is the operator's
/// interface. `parse_bootstrap_peer` turns it into `KnownDid`, and the `KnownDid` arm is the
/// only production caller that may claim an expectation.
#[test]
fn a_known_did_bootstrap_entry_records_a_divergent_peer() -> anyhow::Result<()> {
    init();
    let snapshotter = run_with_local_metrics(async {
        let dialer = IdentityBundle::generate()?;
        let listener = IdentityBundle::generate()?;
        let listener_did = listener.did().clone();
        let expected_did = IdentityBundle::generate()?.did().clone();

        let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
        let (_listener_handle, listener_addr, _listener_guard) = spawn_node(listener).await?;

        let config = bootstrap_config(vec![format!("icn://{expected_did}@{listener_addr}")]);
        let dialed = dial_bootstrap_peers(&config, &dialer_handle).await;
        assert_eq!(
            dialed,
            vec![listener_addr],
            "precondition: bootstrap must have reached the endpoint at all"
        );

        assert!(
            wait_until_authenticated(&dialer_handle, &listener_did, Duration::from_secs(20)).await,
            "precondition: the node that actually answered never authenticated"
        );
        assert_eq!(
            dialer_handle.connected_peers().await,
            vec![listener_did.clone()],
            "the authenticated peer stays canonical — a bootstrap entry names an endpoint \
             and an expectation, never a peer"
        );
        Ok(())
    })?;

    assert_eq!(
        mismatch_total(&snapshotter),
        1,
        "the bootstrap URL named one DID and another authenticated; `dial_bootstrap_peers` \
         must state that expectation so it can be weighed (#2533)"
    );
    Ok(())
}

/// Control: the same path, with the entry naming the node that really is there.
///
/// Without this, the test above would still pass if `dial_bootstrap_peers` reported a
/// mismatch unconditionally — which would make the signal worthless in exactly the
/// deployment where the configuration is right.
#[test]
fn a_correct_known_did_bootstrap_entry_records_nothing() -> anyhow::Result<()> {
    init();
    let snapshotter = run_with_local_metrics(async {
        let dialer = IdentityBundle::generate()?;
        let listener = IdentityBundle::generate()?;
        let listener_did = listener.did().clone();

        let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
        let (_listener_handle, listener_addr, _listener_guard) = spawn_node(listener).await?;

        let config = bootstrap_config(vec![format!("icn://{listener_did}@{listener_addr}")]);
        let dialed = dial_bootstrap_peers(&config, &dialer_handle).await;
        assert_eq!(
            dialed,
            vec![listener_addr],
            "precondition: bootstrap must have reached the endpoint at all"
        );
        assert!(
            wait_until_authenticated(&dialer_handle, &listener_did, Duration::from_secs(20)).await,
            "precondition: the configured peer never authenticated, so nothing was weighed"
        );
        Ok(())
    })?;

    assert_eq!(
        mismatch_total(&snapshotter),
        0,
        "the entry named the node that answered; a correct configuration must stay silent"
    );
    Ok(())
}

/// Control: an `icn://HOST:PORT` entry states no expectation, so it cannot fail one.
///
/// This is the addr-only arm of the same production function. It reaches `icn-net` through
/// `dial_addr`, which synthesises a placeholder DID — so a bootstrap path that derived an
/// expectation from "the DID we dialled with" would report every address-only entry as a
/// misconfiguration.
#[test]
fn an_addr_only_bootstrap_entry_records_nothing() -> anyhow::Result<()> {
    init();
    let snapshotter = run_with_local_metrics(async {
        let dialer = IdentityBundle::generate()?;
        let listener = IdentityBundle::generate()?;
        let listener_did = listener.did().clone();

        let (dialer_handle, _dialer_addr, _dialer_guard) = spawn_node(dialer).await?;
        let (_listener_handle, listener_addr, _listener_guard) = spawn_node(listener).await?;

        let config = bootstrap_config(vec![format!("icn://{listener_addr}")]);
        let dialed = dial_bootstrap_peers(&config, &dialer_handle).await;
        assert_eq!(
            dialed,
            vec![listener_addr],
            "precondition: bootstrap must have reached the endpoint at all"
        );
        assert!(
            wait_until_authenticated(&dialer_handle, &listener_did, Duration::from_secs(20)).await,
            "precondition: the peer never authenticated, so no comparison was reachable"
        );
        Ok(())
    })?;

    assert_eq!(
        mismatch_total(&snapshotter),
        0,
        "an address-only entry names nobody, so the peer that answers cannot be the wrong one"
    );
    Ok(())
}
