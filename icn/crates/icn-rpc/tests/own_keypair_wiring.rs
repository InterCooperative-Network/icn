// Matches the convention used by the other integration tests in this crate.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Behavioural coverage for the node keypair on `RpcServer` (#2497).
//!
//! # What broke
//!
//! `RpcServer::set_own_keypair` had consumers in the trust, recovery and
//! federation handlers but **no caller anywhere in the workspace**, so
//! `own_keypair` was `None` for the daemon's entire lifetime.
//!
//! Two handlers failed loudly (`-32000: Node keypair not available`). The third
//! failed silently: `handler/federation.rs` signs a vouch with
//! `if let Some(keypair) = state.own_keypair()` and falls through to the
//! unsigned value otherwise, so every RPC-issued vouch went out **unsigned**
//! while the response still reported success.
//!
//! These tests pin the observable difference between a server that has the
//! keypair and one that does not. The wiring itself is guaranteed structurally:
//! `RpcDeps::identity_bundle` in `icn-core` is a required (non-`Option`) field,
//! so no caller can construct the deps without supplying an identity, and
//! `spawn_rpc_server` installs it unconditionally.

use std::net::SocketAddr;

use icn_rpc::RpcServer;

fn addr() -> SocketAddr {
    // Not bound - these tests only exercise server state, never the listener.
    "[::1]:0".parse().unwrap()
}

/// A freshly constructed server has no identity. This is the state the daemon
/// ran in for its whole lifetime before #2497.
#[test]
fn a_new_server_has_no_own_keypair() {
    let server = RpcServer::new(addr());

    assert!(
        server.own_keypair().is_none(),
        "a bare RpcServer must start without an identity - if this ever defaults \
         to Some, the fail-closed wiring in spawn_rpc_server stops being meaningful"
    );
}

/// Installing the keypair makes it observable to handlers.
///
/// Fails if `set_own_keypair` stops taking effect, which is what every
/// identity-dependent handler reads.
#[test]
fn installing_the_keypair_makes_it_available_to_handlers() {
    let keypair = icn_identity::KeyPair::generate().unwrap();
    let expected_did = keypair.did().to_string();

    let mut server = RpcServer::new(addr());
    server.set_own_keypair(std::sync::Arc::new(keypair));

    let installed = server
        .own_keypair()
        .expect("keypair must be present after set_own_keypair");

    assert_eq!(
        installed.did().to_string(),
        expected_did,
        "the installed keypair must be the one the node actually holds - \
         handler/trust.rs derives the node's own DID from exactly this value"
    );
}

/// The signing path federation vouches depend on.
///
/// `handler/federation.rs` signs only when `own_keypair()` is `Some`, and
/// silently emits an unsigned vouch otherwise. This asserts the two branches are
/// actually distinguishable, so an unwired server cannot be mistaken for a
/// signing one.
#[test]
fn signing_capability_is_present_only_when_the_keypair_is_installed() {
    let unwired = RpcServer::new(addr());
    assert!(
        unwired.own_keypair().is_none(),
        "without wiring, federation vouches take the unsigned branch"
    );

    let mut wired = RpcServer::new(addr());
    wired.set_own_keypair(std::sync::Arc::new(
        icn_identity::KeyPair::generate().unwrap(),
    ));
    assert!(
        wired.own_keypair().is_some(),
        "with wiring, federation vouches take the signing branch"
    );
}
