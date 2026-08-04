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
//! time actually left for a handshake is whatever remains when the `Incoming` arrives —
//! uniformly distributed over (0, 100ms], not a fixed 100ms. That is why the defect
//! presented as an intermittent ~45% failure rather than a clean threshold.
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
