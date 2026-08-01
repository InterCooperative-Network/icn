// Matches the convention used by the other integration tests in this crate.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The node identity is a construction requirement of `RpcServer` (#2497).
//!
//! # What broke
//!
//! `own_keypair` was an `Option` filled by `RpcServer::set_own_keypair`, and that
//! setter had **no caller anywhere in the workspace**. It stayed `None` for the
//! daemon's entire lifetime. Trust and recovery RPC answered
//! "Node keypair not available", and `handler/federation.rs` took the `else`
//! branch of `if let Some(keypair)` — emitting **unsigned** vouches while still
//! reporting success.
//!
//! # Why these tests look thin
//!
//! Most of the guarantee is now carried by the type system rather than by
//! assertions. `own_keypair` is a required constructor argument and the field is
//! no longer an `Option`, so:
//!
//! - there is no "unconfigured" state a handler can fall through,
//! - the unsigned branch in `federation.rs` no longer exists to be taken,
//! - and the setter that was never called is gone entirely.
//!
//! Deleting the production wiring is now a **compile error**, not a silent
//! regression — which is a stronger guard than any runtime test, and is
//! deliberately preferred over a test that mirrors the implementation.
//!
//! What remains worth asserting at runtime is that the identity a caller supplies
//! is the one handlers actually observe.

use std::net::SocketAddr;
use std::sync::Arc;

use ed25519_dalek::Verifier;
use icn_identity::KeyPair;
use icn_rpc::RpcServer;

fn addr() -> SocketAddr {
    // Not bound - these tests only exercise server state, never the listener.
    "[::1]:0".parse().unwrap()
}

/// The identity handed to the constructor is the identity handlers read.
///
/// `handler/trust.rs` derives the node's own DID from exactly this value, and
/// `handler/federation.rs` signs with it.
#[test]
fn the_constructed_identity_is_what_handlers_observe() {
    let keypair = KeyPair::generate().unwrap();
    let expected_did = keypair.did().to_string();

    let server = RpcServer::new(addr(), Arc::new(keypair));

    assert_eq!(
        server.own_keypair().did().to_string(),
        expected_did,
        "the server must expose the identity it was constructed with"
    );
}

/// The authenticated constructor carries the identity too.
///
/// Both constructors previously defaulted `own_keypair` to `None`; a fix that
/// only covered one of them would leave the auth-enabled daemon — the production
/// configuration — still broken.
#[test]
fn the_authenticated_constructor_also_carries_the_identity() {
    let keypair = KeyPair::generate().unwrap();
    let expected_did = keypair.did().to_string();

    let server = RpcServer::new_with_auth(addr(), b"test-secret".to_vec(), Arc::new(keypair));

    assert_eq!(
        server.own_keypair().did().to_string(),
        expected_did,
        "new_with_auth must carry the identity as well - this is the constructor \
         the production daemon uses when the gateway is enabled"
    );
}

/// Distinct nodes keep distinct identities.
///
/// Guards against a regression that returns some shared or default keypair,
/// which would make every node's signatures indistinguishable.
#[test]
fn distinct_nodes_expose_distinct_identities() {
    let a = RpcServer::new(addr(), Arc::new(KeyPair::generate().unwrap()));
    let b = RpcServer::new(addr(), Arc::new(KeyPair::generate().unwrap()));

    assert_ne!(
        a.own_keypair().did().to_string(),
        b.own_keypair().did().to_string(),
        "two independently constructed servers must not share an identity"
    );
}

/// The signing key is usable, not merely present.
///
/// `own_keypair()` returning something is not enough — `federation.rs` calls
/// `vouch.sign(...)` with it, so the key has to actually produce a verifiable
/// signature.
#[test]
fn the_identity_can_produce_a_verifiable_signature() {
    let keypair = KeyPair::generate().unwrap();
    let server = RpcServer::new(addr(), Arc::new(keypair));

    let message = b"federation vouch payload";
    let signature = server.own_keypair().sign(message);

    // Verify against the DID's public key, not the keypair object: this is what
    // a remote peer would actually do with a received vouch.
    let verifying_key = server
        .own_keypair()
        .did()
        .to_verifying_key()
        .expect("the node DID must yield a verifying key");

    assert!(
        verifying_key.verify(message, &signature).is_ok(),
        "the installed identity must produce a signature that verifies against \
         the public key its DID advertises - this is what a peer receiving a \
         federation vouch checks"
    );
}
