#![allow(clippy::unwrap_used, clippy::expect_used)]
//! `network.mdns_enabled = false` must mean this node neither advertises nor browses.
//!
//! Until it was wired, the setting was read by nothing: every node registered an mDNS service
//! carrying its DID and listen address on all interfaces, and browsed for others, regardless.
//!
//! The oracle is `Discovery::start`, the only path that registers a service or spawns the
//! browse task; it announces itself on entry. Proving an *absence* over real multicast would be
//! a timing guess, so instead each case runs on a current-thread runtime under a thread-local
//! log capture, and the enabled case is the positive control that the capture can see `start`.

use icn_identity::IdentityBundle;
use icn_net::{IncomingMessageHandler, NetworkActor, NetworkMessage};
use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn spawn_and_capture(mdns_enabled: bool) -> String {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let capture = Capture::default();
    let writer = capture.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .finish();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tracing::subscriber::with_default(subscriber, || {
        runtime.block_on(async {
            let (shutdown, _) = broadcast::channel(1);
            let handler: IncomingMessageHandler = Arc::new(|_msg: NetworkMessage| {});
            let port = portpicker::pick_unused_port().expect("no free port");
            let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
            let _handle = NetworkActor::spawn_with_mdns(
                IdentityBundle::generate().unwrap(),
                addr,
                shutdown.clone(),
                Some(handler),
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
                mdns_enabled,
            )
            .await
            .expect("the actor must start");
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = shutdown.send(());
            tokio::time::sleep(Duration::from_millis(200)).await;
        })
    });
    let text = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    text
}

#[test]
fn disabled_mdns_never_starts_discovery() {
    let enabled = spawn_and_capture(true);
    assert!(
        enabled.contains("Discovery service starting"),
        "positive control: the capture must see discovery start when it is enabled:\n{enabled}"
    );

    let disabled = spawn_and_capture(false);
    assert!(
        !disabled.contains("Discovery service starting"),
        "discovery must not start when disabled:\n{disabled}"
    );
    assert!(!disabled.contains("mDNS service registered"), "{disabled}");
    assert!(
        disabled.contains("mDNS discovery disabled by configuration"),
        "{disabled}"
    );
}
