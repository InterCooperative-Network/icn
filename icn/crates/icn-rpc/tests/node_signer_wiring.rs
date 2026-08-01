// Matches the convention used by the other integration tests in this crate.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The node's signing capability is a construction requirement of `RpcServer`
//! (#2497), and it is a *capability* rather than extractable key material
//! (#2501).
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
//! The first fix made the identity a constructor argument but demanded a raw
//! [`icn_identity::KeyPair`]. The composition root could only get one from
//! `IdentityBundle::keypair()`, which fails by design on a hardware-backed
//! bundle. Combined with fail-closed startup, that turned PKCS#11 and TPM
//! identities into a daemon that refuses to serve RPC (#2501). Requiring a key
//! to be *exportable* is strictly stronger than requiring it to be *usable*, and
//! no handler ever needed the private bytes.
//!
//! # What is asserted here, and what is not
//!
//! Part of the guarantee is carried by the type system rather than by
//! assertions. `own_signer` is a required constructor argument and the field is
//! not an `Option`, so there is no "unconfigured" state for a handler to fall
//! through, and the setter that was never called is gone. Deleting the
//! production wiring is a **compile error**, not a silent regression.
//!
//! These tests cover what types cannot: that the identity a caller supplies is
//! the one handlers observe, that it produces signatures a *peer* can verify,
//! and — in [`a_vouch_signed_by_a_non_extractable_signer_verifies`] — that a
//! real `Vouch` signed through this path passes `verify_signature()`. That last
//! one exercises the artifact; the others deliberately do not claim to.

use std::net::SocketAddr;
use std::sync::Arc;

use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier, VerifyingKey};
use icn_identity::{Did, DidSigner, KeyPair};
use icn_rpc::RpcServer;

/// A signer with no key-extraction API at all.
///
/// Stands in for PKCS#11/TPM: `sign()` is the *only* way to use the key. There
/// is deliberately no accessor returning a `KeyPair`, a `SigningKey`, or raw
/// bytes — if `RpcServer` ever needs one again, this file stops compiling, which
/// is the point. CI has no HSM, and the property worth proving is the shape of
/// the dependency, not a vendor driver.
struct NonExtractableSigner {
    /// Private. Never leaves this struct; a real backend would not have it in
    /// process memory at all.
    key: SigningKey,
    did: Did,
    verifying_key: VerifyingKey,
}

impl NonExtractableSigner {
    /// Seeded rather than random: these tests assert on identity plumbing, and a
    /// fixed seed keeps failures reproducible. Distinct seeds give distinct DIDs.
    fn with_seed(seed: u8) -> Self {
        let key = SigningKey::from_bytes(&[seed; 32]);
        let verifying_key = key.verifying_key();
        Self {
            did: Did::from_public_key(&verifying_key),
            key,
            verifying_key,
        }
    }
}

impl DidSigner for NonExtractableSigner {
    fn did(&self) -> &Did {
        &self.did
    }

    fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    fn sign(&self, message: &[u8]) -> anyhow::Result<Signature> {
        Ok(self.key.sign(message))
    }

    fn is_hardware_backed(&self) -> bool {
        true
    }

    fn backend_type(&self) -> &str {
        "test-non-extractable"
    }
}

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
        server.own_signer().did().to_string(),
        expected_did,
        "the server must expose the identity it was constructed with"
    );
}

/// The authenticated constructor carries the identity too.
///
/// Both constructors previously defaulted the field to `None`; a fix that only
/// covered one of them would leave the auth-enabled daemon — the production
/// configuration — still broken.
#[test]
fn the_authenticated_constructor_also_carries_the_identity() {
    let keypair = KeyPair::generate().unwrap();
    let expected_did = keypair.did().to_string();

    let server = RpcServer::new_with_auth(addr(), b"test-secret".to_vec(), Arc::new(keypair));

    assert_eq!(
        server.own_signer().did().to_string(),
        expected_did,
        "new_with_auth must carry the identity as well - this is the constructor \
         the production daemon uses when the gateway is enabled"
    );
}

/// Distinct nodes keep distinct identities.
///
/// Guards against a regression that returns some shared or default identity,
/// which would make every node's signatures indistinguishable.
#[test]
fn distinct_nodes_expose_distinct_identities() {
    let a = RpcServer::new(addr(), Arc::new(KeyPair::generate().unwrap()));
    let b = RpcServer::new(addr(), Arc::new(KeyPair::generate().unwrap()));

    assert_ne!(
        a.own_signer().did().to_string(),
        b.own_signer().did().to_string(),
        "two independently constructed servers must not share an identity"
    );
}

/// The RPC layer accepts a signer that cannot yield a private key.
///
/// This is the whole of #2501: if this compiles, hardware-backed configurations
/// are serviceable; if it does not, they are locked out of RPC.
#[test]
fn rpc_server_accepts_a_signer_with_no_extractable_key() {
    let signer = NonExtractableSigner::with_seed(11);
    let expected_did = signer.did().to_string();

    let server = RpcServer::new(addr(), Arc::new(signer));

    assert_eq!(
        server.own_signer().did().to_string(),
        expected_did,
        "the server must expose the identity of the signer it was built with"
    );
    assert!(
        server.own_signer().is_hardware_backed(),
        "the signer's own backend classification must survive installation - \
         erasing it would hide which nodes hold non-extractable keys"
    );
}

/// The authenticated constructor takes the same capability.
#[test]
fn the_authenticated_constructor_also_accepts_a_non_extractable_signer() {
    let signer = NonExtractableSigner::with_seed(22);
    let expected_did = signer.did().to_string();

    let server = RpcServer::new_with_auth(addr(), b"test-secret".to_vec(), Arc::new(signer));

    assert_eq!(server.own_signer().did().to_string(), expected_did);
}

/// A non-extractable signer produces signatures a peer can actually verify.
///
/// Installing the capability is not enough — a remote peer checks results
/// against the public key the issuer's DID advertises, which is all it has.
#[test]
fn a_non_extractable_signer_produces_peer_verifiable_signatures() {
    let server = RpcServer::new(addr(), Arc::new(NonExtractableSigner::with_seed(33)));

    let message = b"federation vouch payload";
    let signature = server
        .own_signer()
        .sign(message)
        .expect("a healthy signer must sign");

    let verifying_key = server
        .own_signer()
        .did()
        .to_verifying_key()
        .expect("the node DID must yield a verifying key");

    assert!(
        verifying_key.verify(message, &signature).is_ok(),
        "a signature from a non-extractable signer must verify against the \
         public key its DID advertises"
    );
}

/// The end-to-end artifact: a real `Vouch`, signed via the installed capability,
/// passes the verification a receiving peer runs.
///
/// The other tests here assert plumbing. This one asserts the thing that was
/// actually broken — `handler/federation.rs` used to emit a `Vouch` whose
/// `signature` field was empty while reporting success, and `verify_signature()`
/// rejects exactly that with "Missing signature".
///
/// The voucher DID matches the signer because production builds the vouch from
/// `registry.own_coop_info().public_did`, which `init_federation.rs` populates
/// with the node's own DID. A mismatch there would make every issued vouch
/// unverifiable, so binding them here is not incidental.
#[test]
fn a_vouch_signed_by_a_non_extractable_signer_verifies() {
    let server = RpcServer::new(addr(), Arc::new(NonExtractableSigner::with_seed(44)));
    let signer = server.own_signer();

    let vouch = icn_federation::Vouch::new(
        "vouching-coop".to_string(),
        signer.did().clone(),
        "target-coop".to_string(),
        0.8,
    );

    // Unsigned, this is precisely what the old `else` branch shipped.
    assert!(
        vouch.clone().verify_signature().is_err(),
        "an unsigned vouch must not verify - if it did, the original bug would \
         have been invisible to peers"
    );

    let signed = vouch
        .sign(signer.as_ref())
        .expect("the installed signer must sign the vouch");

    assert!(
        !signed.signature.is_empty(),
        "signing must populate the signature field"
    );
    assert!(
        signed.verify_signature().is_ok(),
        "a vouch signed through the RPC server's own signing capability must \
         pass the verification a receiving peer performs"
    );
}
