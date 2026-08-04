//! #2521 — a shutdown-polling deadline must not become a maximum handshake lifetime.
//!
//! `NetworkActor::handle_incoming_connections` wrapped the whole of
//! `SessionManager::accept()` in a 100ms timeout so it could periodically re-check for
//! shutdown. That one call does two unrelated things:
//!
//! 1. `Endpoint::accept()` — wait for a *new* inbound connection. Cancel-safe: the
//!    pending `Incoming` stays queued on the endpoint, so dropping the future loses
//!    nothing.
//! 2. `incoming.await` — drive that connection's full QUIC/TLS handshake. **Not**
//!    cancel-safe: `Incoming::into_future` consumes the `Incoming` into a `Connecting`,
//!    and dropping a `Connecting` drops the last `ConnectionRef`, which quinn turns into
//!    an implicit close. The connection attempt is destroyed with no application-level
//!    trace.
//!
//! Because the 100ms budget covers both phases and is re-armed each loop iteration, the
//! time actually left for a handshake is only whatever remains when the `Incoming`
//! arrives — a remainder varying from nearly zero up to 100ms, not a fixed 100ms. Under
//! arrivals that are not synchronised to the polling cycle this yields probabilistic
//! rather than threshold failure, which is why the defect presented as an intermittent
//! flake. (The arrival-phase distribution was not measured; no exact rate is claimed.)
//!
//! These tests pin the boundary deterministically, by construction rather than by
//! shrinking a duration until failure becomes likely. `tokio::time::timeout` polls its
//! inner future *before* it checks the deadline, so a `Duration::ZERO` budget grants
//! exactly one poll. That single poll is enough to tell the two phases apart:
//!
//! - `Endpoint::accept()` pops an already-queued `Incoming` on its first poll, so the
//!   cancel-safe phase always survives a zero budget.
//! - `Connecting::poll` reads a oneshot that the freshly-spawned connection driver cannot
//!   possibly have signalled yet, so the handshake phase never survives one.
//!
//! A zero budget is therefore not a strawman duration — it is the sharpest available
//! probe for *where the cancellation boundary sits*. The production 100ms value has the
//! same defect with a merely probabilistic trigger.
//!
//! Two earlier attempts at this test are worth recording, because both produce
//! false confidence:
//!
//! - A "1ms is shorter than any handshake" budget is not: a warm loopback handshake can
//!   complete in well under a millisecond, and the test then passes for the wrong reason.
//! - The *client's* view is not a valid oracle. The server can complete the handshake,
//!   let the client's `connect()` resolve `Ok`, and only then discard the connection — so
//!   asserting `connect()` failed is flaky. This is not merely a test artifact: it is
//!   precisely the production symptom, where the dialing peer logs a successful
//!   certificate exchange and the receiver then shows no trace of the connection at all.
//!   Every assertion below is made on the *server* side.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::Result;
use icn_identity::IdentityBundle;
use icn_net::session::SessionManager;
use icn_net::{NetworkActor, NetworkMessage, VersionInfo};
use quinn::{ClientConfig, Endpoint};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// A one-poll budget. See the module docs: this is what makes the tests deterministic
/// rather than a race against how fast the host completes a loopback handshake.
const ONE_POLL: Duration = Duration::ZERO;

/// Spacing between accept attempts, so a zero-budget loop does not spin hot while it
/// waits for the client's first packet to arrive. This sits *outside* the accept, so it
/// has no bearing on the property under test.
const POLL_GAP: Duration = Duration::from_millis(5);

/// How long an accept loop is given to produce a connection before the test gives up.
/// This is a hang guard, not the assertion — the assertions below are about *whether* a
/// connection was ever handed over, never about how fast.
const ACCEPT_WINDOW: Duration = Duration::from_secs(5);

fn init() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// A started `SessionManager` listening on loopback, plus the address to dial it on.
async fn start_server() -> Result<(Arc<SessionManager>, SocketAddr)> {
    let bundle = IdentityBundle::generate()?;
    let port = portpicker::pick_unused_port().expect("no free port");
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let mut manager = SessionManager::new();
    manager.start(&bundle, addr, None, None).await?;
    Ok((Arc::new(manager), addr))
}

/// A raw QUIC client presenting a real certificate, i.e. an entirely legitimate peer.
/// Nothing about the client is adversarial or slow — the only variable under test is how
/// the server's accept loop treats the handshake.
fn client_endpoint() -> Result<Endpoint> {
    let bundle = IdentityBundle::generate()?;
    let rustls_client =
        icn_net::tls::create_tofu_client_config(vec![bundle.tls_cert().clone()], bundle.tls_key())?;
    let mut endpoint = Endpoint::client("127.0.0.1:0".parse()?)?;
    endpoint.set_default_client_config(ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(rustls_client)?,
    )));
    Ok(endpoint)
}

/// Control: the same server, client and handshake, with the accept never cancelled.
///
/// Without this the negative test below would be vacuous — "no connection arrived" is
/// also what a broken harness looks like.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_uncancelled_accept_completes_the_inbound_handshake() -> Result<()> {
    init();
    let (server, addr) = start_server().await?;
    let client = client_endpoint()?;

    let dial = tokio::spawn(async move {
        let result = client.connect(addr, "localhost")?.await;
        // Hold the endpoint open until the server has finished with it.
        Ok::<_, anyhow::Error>((result, client))
    });

    let accepted = tokio::time::timeout(ACCEPT_WINDOW, server.accept()).await;

    let accepted = accepted
        .expect("the server should accept well inside the window when nothing cancels it")?;
    assert!(
        accepted.is_some(),
        "control: an uncancelled accept must hand over the inbound connection"
    );

    let (client_result, _client) = dial.await??;
    assert!(
        client_result.is_ok(),
        "control: the client's handshake must succeed when the server does not cancel it: {:?}",
        client_result.err()
    );
    Ok(())
}

/// The defect, pinned as executable documentation: `SessionManager::accept()` fuses the
/// cancel-safe wait with the cancel-*unsafe* handshake, so wrapping it in a polling
/// timeout destroys legitimate inbound connections.
///
/// This asserts a *negative* property of the fused API, so it holds before and after the
/// production fix. It is the standing reason the accept loop may no longer call
/// `accept()` inside a cancellation scope.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fused_accept_cancelled_after_one_poll_destroys_the_handshake() -> Result<()> {
    init();
    let (server, addr) = start_server().await?;
    let client = client_endpoint()?;

    // The client is kept alive for the whole window so the server has a genuine, live
    // connection attempt to accept on every iteration below.
    let dial = tokio::spawn(async move {
        let result = client.connect(addr, "localhost")?.await;
        tokio::time::sleep(ACCEPT_WINDOW).await;
        Ok::<_, anyhow::Error>((result, client))
    });

    // Exactly the shape the production accept loop used: re-arm a poll timeout around the
    // fused accept, discarding whatever was in flight when it fires.
    let deadline = tokio::time::Instant::now() + ACCEPT_WINDOW;
    let mut handed_over = None;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(ONE_POLL, server.accept()).await {
            // Elapsed: the production loop dropped this future and looped. The `Incoming`
            // was already consumed off the endpoint's queue by that single poll, so the
            // connection attempt is now gone — a later iteration will not rediscover it.
            Err(_elapsed) => {
                tokio::time::sleep(POLL_GAP).await;
            }
            Ok(Ok(Some(connection))) => {
                handed_over = Some(connection);
                break;
            }
            Ok(Ok(None)) => break,
            Ok(Err(_)) => {
                tokio::time::sleep(POLL_GAP).await;
            }
        }
    }

    assert!(
        handed_over.is_none(),
        "a one-poll budget cannot span a QUIC/TLS handshake: the connection driver has \
         only just been spawned and cannot have signalled completion. Handing over a \
         connection here would mean this test has stopped probing the boundary."
    );

    dial.abort();
    Ok(())
}

/// The regression guard. With the wait and the handshake separated, the wait may be
/// cancelled as aggressively as a caller likes — here on the same one-poll budget that
/// destroys the fused API above, which is far more hostile than the production shutdown
/// check — and a legitimate inbound handshake still completes.
///
/// This is the property that makes the accept loop's shutdown responsiveness free: it can
/// abandon the wait whenever it wants without ever putting a connection at risk.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_the_accept_wait_does_not_destroy_an_inbound_handshake() -> Result<()> {
    init();
    let (server, addr) = start_server().await?;
    let client = client_endpoint()?;

    let dial = tokio::spawn(async move {
        let result = client.connect(addr, "localhost")?.await;
        Ok::<_, anyhow::Error>((result, client))
    });

    let endpoint = server
        .endpoint_handle()
        .await
        .expect("the server endpoint is started");

    // Phase 1, cancelled repeatedly on a one-poll budget. Cancel-safe: a queued `Incoming`
    // survives every one of these abandonments.
    let deadline = tokio::time::Instant::now() + ACCEPT_WINDOW;
    let mut arrived = None;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(ONE_POLL, endpoint.accept()).await {
            Err(_elapsed) => {
                tokio::time::sleep(POLL_GAP).await;
            }
            Ok(Some(incoming)) => {
                arrived = Some(incoming);
                break;
            }
            Ok(None) => break,
        }
    }
    let incoming = arrived.expect(
        "the cancel-safe wait must eventually hand over an Incoming no matter how often \
         it is abandoned",
    );

    // Phase 2, driven outside any cancellation scope. The timeout here is a test hang
    // guard on a deadline three orders of magnitude larger than a loopback handshake, not
    // a budget the handshake is expected to strain against.
    let connection = tokio::time::timeout(ACCEPT_WINDOW, incoming)
        .await
        .expect("an uncancelled handshake must not hang")?;

    assert!(
        connection.remote_address().ip().is_loopback(),
        "the completed handshake must be the loopback client we dialled with"
    );

    let (client_result, _client) = dial.await??;
    assert!(
        client_result.is_ok(),
        "the client must observe the same successful handshake the server completed: {:?}",
        client_result.err()
    );
    Ok(())
}

/// A client-side verifier that stalls the handshake by a controlled amount.
///
/// The delay is deliberately placed on the *client's* verification of the **server's**
/// certificate. That step gates the client's Finished flight, which in turn gates the
/// server's handshake completion — so it lengthens the server-side handshake without any
/// production seam and without weakening the server's own TLS semantics.
#[derive(Debug)]
struct StallingServerCertVerifier {
    delay: Duration,
}

impl rustls::client::danger::ServerCertVerifier for StallingServerCertVerifier {
    fn verify_server_cert(
        &self,
        _e: &rustls::pki_types::CertificateDer<'_>,
        _i: &[rustls::pki_types::CertificateDer<'_>],
        _n: &rustls::pki_types::ServerName<'_>,
        _o: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // Synchronous by necessity: rustls verifiers are sync. This models a peer whose
        // handshake is slow for any ordinary reason — loaded CPU, WAN latency, expensive
        // certificate verification.
        std::thread::sleep(self.delay);
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _m: &[u8],
        _c: &rustls::pki_types::CertificateDer<'_>,
        _d: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _m: &[u8],
        _c: &rustls::pki_types::CertificateDer<'_>,
        _d: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![rustls::SignatureScheme::ED25519]
    }
}

/// Comfortably longer than the 100ms shutdown-poll interval the accept loop used, so the
/// server-side handshake is guaranteed to outlive that budget rather than merely likely to.
const HANDSHAKE_STALL: Duration = Duration::from_millis(300);

/// The regression property, exercised through the real production path: a `NetworkActor`,
/// its real accept loop, a real Hello, and real peer installation as the oracle.
///
/// A legitimate peer whose handshake takes 300ms must still be admitted. On pristine
/// `main` this fails deterministically, because the accept loop's 100ms shutdown poll
/// cancels the handshake before it can complete; after #2521 it passes unchanged.
///
/// This is the fail-before / pass-after test. The two `SessionManager`-layer tests above
/// are a deterministic *witness* of the defect — they assert the fused API is cancel-unsafe
/// and therefore pass on both trees. Only this one changes verdict across the fix.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_slow_but_legitimate_peer_is_still_admitted() -> Result<()> {
    init();

    let receiver_bundle = IdentityBundle::generate()?;
    let receiver_did = receiver_bundle.did().clone();
    let port = portpicker::pick_unused_port().expect("no free port");
    let receiver_addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;

    let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
    let handler: icn_net::IncomingMessageHandler = Arc::new(|_msg| {});
    let handle = NetworkActor::spawn(
        receiver_bundle,
        receiver_addr,
        shutdown_tx.clone(),
        Some(handler),
        None, // oracle
        None, // fallback_config
        None, // topology_config
        None, // stun_servers
        None, // turn_config
        None, // misbehavior_detector
        None, // store
        None, // personhood_store
        None, // anchor_rate_config
        None, // advertised_addr
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // A legitimate peer with a genuine certificate and a genuine binding. The only unusual
    // thing about it is that its handshake is slow.
    let sender = IdentityBundle::generate()?;
    let sender_did = sender.did().clone();
    let mut rustls_client = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(StallingServerCertVerifier {
            delay: HANDSHAKE_STALL,
        }))
        .with_client_auth_cert(vec![sender.tls_cert().clone()], sender.tls_key())?;
    rustls_client.alpn_protocols = vec![b"icn/1".to_vec()];

    let mut endpoint = Endpoint::client("127.0.0.1:0".parse()?)?;
    endpoint.set_default_client_config(ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(rustls_client)?,
    )));

    let dialled = tokio::time::timeout(
        Duration::from_secs(10),
        endpoint.connect(receiver_addr, "localhost")?,
    )
    .await
    .map_err(|_| anyhow::anyhow!("the client's own connect timed out"))?;

    // A receiver that cancels the handshake aborts it mid-flight, which quinn surfaces
    // here as "closed during the handshake". Naming it explicitly keeps the failure
    // self-describing instead of an opaque `?` on a transport error.
    let connection = match dialled {
        Ok(connection) => connection,
        Err(e) => panic!(
            "a legitimate peer whose handshake takes {}ms was cut off mid-handshake by \
             the receiver ({e}) — the accept loop cancelled a connection it had already \
             accepted (#2521)",
            HANDSHAKE_STALL.as_millis()
        ),
    };

    let hello = NetworkMessage::hello(
        sender_did.clone(),
        receiver_did,
        sender.binding_info(),
        VersionInfo::new("icnd-slow-peer".to_string()),
        None, // topology_info
        [7u8; 32],
        None, // ml_dsa_public
        None, // ml_kem_public
    );
    // If the receiver has already discarded this connection, opening the stream is where
    // the client finds out — but that is deliberately *not* the verdict. A receiver that
    // cancels the handshake can still leave the client believing it connected, so a
    // client-side error here is treated as a symptom to be confirmed, not as the result.
    // The verdict is the server-side admission asserted below.
    if let Ok((mut send, _recv)) = connection.open_bi().await {
        let _ = icn_net::protocol::write_message(&mut send, &hello).await;
        let _ = send.finish();
    }

    // The oracle is server-side peer installation, not the client's view: a handshake the
    // server later discards can still look successful from the client.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut admitted = None;
    while tokio::time::Instant::now() < deadline {
        if let Some(info) = handle.get_peer_connection_info(&sender_did).await {
            admitted = Some(info);
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let _ = shutdown_tx.send(());
    assert!(
        admitted.is_some(),
        "a legitimate peer whose handshake takes {}ms must still be admitted; if it is \
         not, the accept loop cancelled a handshake it had already accepted (#2521)",
        HANDSHAKE_STALL.as_millis()
    );
    Ok(())
}

/// Spawn a real receiver on the production stack, returning its handle, address and
/// shutdown sender.
async fn spawn_receiver() -> Result<(
    icn_net::NetworkHandle,
    SocketAddr,
    tokio::sync::broadcast::Sender<()>,
)> {
    let bundle = IdentityBundle::generate()?;
    let port = portpicker::pick_unused_port().expect("no free port");
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
    let handler: icn_net::IncomingMessageHandler = Arc::new(|_msg| {});
    let handle = NetworkActor::spawn(
        bundle,
        addr,
        shutdown_tx.clone(),
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
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok((handle, addr, shutdown_tx))
}

/// Shutdown must actually reach the endpoint and close it.
///
/// The old loop noticed shutdown by polling every 100ms; the new one is woken by the
/// signal directly. Either way the observable contract is the same and is asserted here
/// rather than assumed from the fact that `stop()` is called somewhere: an established
/// peer must see the connection closed promptly after shutdown is signalled.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shutdown_closes_established_connections_promptly() -> Result<()> {
    init();
    let (_handle, addr, shutdown_tx) = spawn_receiver().await?;

    let client = client_endpoint()?;
    let connection = client.connect(addr, "localhost")?.await?;

    let _ = shutdown_tx.send(());

    // Generous relative to the ~100ms the old poll could add, tight enough that a shutdown
    // hang (the failure mode a naive "just delete the timeout" fix would introduce) fails
    // this rather than passing slowly.
    tokio::time::timeout(Duration::from_secs(5), connection.closed())
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "the receiver did not close an established connection within 5s of \
                 shutdown: the accept loop is not observing the shutdown signal"
            )
        })?;
    Ok(())
}

/// The handshake slot must be released on every path, or the accept loop wedges forever.
///
/// This is the guard on the bounded-concurrency change itself. The permit is acquired
/// *before* an `Incoming` is taken, so a permit that leaked on completion would stall the
/// loop at `acquire_owned()` and it would never accept another connection. Driving more
/// peers than the cap and requiring all of them through detects exactly that.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn more_peers_than_the_handshake_cap_can_all_connect() -> Result<()> {
    init();
    let (_handle, addr, shutdown_tx) = spawn_receiver().await?;

    // Comfortably above MAX_CONCURRENT_INBOUND_HANDSHAKES (64): if permits were released
    // only on some paths, the loop would stop accepting somewhere around the cap.
    const PEERS: usize = 80;

    let mut dials = Vec::with_capacity(PEERS);
    for _ in 0..PEERS {
        let endpoint = client_endpoint()?;
        dials.push(tokio::spawn(async move {
            let result = tokio::time::timeout(
                Duration::from_secs(20),
                endpoint.connect(addr, "localhost")?,
            )
            .await;
            // Keep the endpoint alive until the connection resolves.
            Ok::<_, anyhow::Error>((result.is_ok() && result.unwrap().is_ok(), endpoint))
        }));
    }

    let mut connected = 0usize;
    for dial in dials {
        if let Ok(Ok((true, _endpoint))) = dial.await {
            connected += 1;
        }
    }

    let _ = shutdown_tx.send(());
    assert_eq!(
        connected, PEERS,
        "all {PEERS} peers must complete their handshake; only {connected} did, which \
         means handshake slots are not being released on every path"
    );
    Ok(())
}
