#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Whether the assembled daemon *has* RPC authentication is a property of the
//! composition path, not of `icn-rpc`.
//!
//! `RpcAuthManager` works. It is implemented and tested. But `RpcServer` holds it
//! as `Option<Arc<RpcAuthManager>>`, and `handle_rpc` opens with
//! `if let Some(auth_manager) = &state.auth_manager` — so a server that was never
//! given one does not reject requests, it simply **never checks any**. The absent
//! case is silent by construction: no error, no warning at the call site, just a
//! server that authenticates nothing.
//!
//! This is the recurring shape #2500 names, and `init_rpc.rs` already records two
//! instances of it in its own comments:
//!
//!   * #2497 — `RpcServer::set_own_keypair` had no caller anywhere in the workspace,
//!     so it stayed `None` for the daemon's whole lifetime and `handler/federation.rs`
//!     emitted vouches with no signature *while still reporting success*.
//!   * #2437 — `new_with_auth` alone left `revocation_list: None`, so `auth.revoke`
//!     answered "Token revocation not available" even though the full
//!     `TokenRevocationList` machinery existed and passed its own tests.
//!
//! Both were fixed at the composition root. Nothing stops either from regressing,
//! because no test observes what the *canonical config path* actually installs.
//!
//! So this drives the real `RpcConfig::from_daemon_config` — the function the
//! daemon itself calls, at `lifecycle.rs:1150` — from an operator-facing `Config`,
//! builds the server on the same branch `spawn_rpc_server` takes, and asserts on the
//! capability the server ended up holding.
//!
//! **What is and is not pinned here.** Be precise about this, because the difference
//! is the whole point of #2500:
//!
//!   * **Pinned for real:** `RpcConfig::from_daemon_config`. It is invoked directly,
//!     not imitated, so a change that drops the operator's `jwt_secret` or moves the
//!     bind off loopback fails these tests.
//!   * **Mirrored, NOT pinned:** the server-construction branch. `server_for` below
//!     restates the `match` from `spawn_rpc_server` rather than calling it, because
//!     `spawn_rpc_server` needs a full `RpcDeps` and a `JoinSet`. Changing the
//!     authenticated arm of `spawn_rpc_server` to `RpcServer::new` would leave
//!     production RPC unauthenticated and **these tests would still pass**.
//!   * **Not covered at all:** the `set_auth_manager_with_store` upgrade that
//!     `spawn_rpc_server` applies after construction to add token revocation (#2437).
//!
//! Closing the second and third bullets means driving `spawn_rpc_server` itself, which
//! is a larger harness than this contract; it is deliberately left as follow-up rather
//! than claimed here.
//!
//! The load-bearing assertion is the last one. Today the unauthenticated branch is
//! reachable — `gateway.enabled` defaults to `false`, which yields
//! `jwt_secret: None` and therefore no auth manager. That is safe *only* because
//! `from_daemon_config` hardcodes the bind to IPv6 loopback. Those two facts live
//! in different concerns and nothing currently ties them together: making the RPC
//! address configurable would turn a documented-safe default into a remote
//! unauthenticated control channel, and every existing test would still pass.
//! This file is where that change is supposed to fail.

use icn_core::config::Config;
use icn_core::supervisor::init_rpc::RpcConfig;
use icn_identity::IdentityBundle;
use icn_rpc::RpcServer;

/// Build a server the way `init_rpc::spawn_rpc_server` does at its construction site:
/// branch on whether the composition path produced a JWT secret.
///
/// This branch is a **mirror** of `spawn_rpc_server`, not a call into it — that
/// function requires a full `RpcDeps` and a `JoinSet`. So this helper does not guard
/// `spawn_rpc_server`'s own arm; see the module header for exactly what that leaves
/// uncovered. What it does do is keep the branch in one place, so every test below
/// observes the same `RpcConfig` the production decision produced.
fn server_for(config: &Config) -> (RpcConfig, RpcServer) {
    let rpc_config = RpcConfig::from_daemon_config(config);
    let signer = IdentityBundle::generate()
        .expect("generate identity")
        .signing_capability()
        .expect("signing capability");
    let server = match rpc_config.jwt_secret.as_ref() {
        Some(secret) => RpcServer::new_with_auth(rpc_config.addr, secret.clone(), signer),
        None => RpcServer::new(rpc_config.addr, signer),
    };
    (rpc_config, server)
}

fn enabled_gateway_config() -> Config {
    let mut config = Config::default();
    config.gateway.enabled = true;
    config.gateway.jwt_secret = "composition-contract-secret-at-least-32-bytes".to_string();
    config
}

#[test]
fn operator_enabling_the_gateway_installs_rpc_auth() {
    let (rpc_config, server) = server_for(&enabled_gateway_config());

    assert!(
        rpc_config.jwt_secret.is_some(),
        "composition path dropped the operator's jwt_secret before the server was built"
    );
    assert!(
        server.auth_manager().is_some(),
        "gateway is enabled with a secret, but the assembled RPC server holds no auth \
         manager — every request would skip the auth check entirely"
    );
}

#[test]
fn an_empty_secret_does_not_count_as_configured_auth() {
    // Defensive, not an operator scenario: `Config::validate()` (config/mod.rs:219)
    // already rejects `gateway.enabled = true` with an empty `jwt_secret` as a fatal
    // error, so a validated config cannot reach this state. This pins the behaviour
    // for the paths that skip validation — programmatic `Config` construction and
    // tests — so that if validation is ever relaxed or bypassed, the fallback is a
    // server with no auth manager rather than one keyed on an empty secret.
    let mut config = Config::default();
    config.gateway.enabled = true;
    config.gateway.jwt_secret = String::new();

    let (rpc_config, server) = server_for(&config);

    assert!(rpc_config.jwt_secret.is_none());
    assert!(
        server.auth_manager().is_none(),
        "an empty jwt_secret produced an auth manager keyed on nothing"
    );
}

#[test]
fn the_default_config_yields_an_rpc_server_that_checks_no_credentials() {
    // Not an aspiration — the current, intended behaviour, pinned so that a change
    // to it is deliberate. `gateway.enabled` defaults to false, so the default
    // daemon serves RPC with no auth manager at all.
    let (rpc_config, server) = server_for(&Config::default());

    assert!(
        rpc_config.jwt_secret.is_none(),
        "default config unexpectedly produced a jwt_secret"
    );
    assert!(
        server.auth_manager().is_none(),
        "default config unexpectedly installed an auth manager"
    );
}

#[test]
fn rpc_never_binds_off_loopback_whether_or_not_auth_is_installed() {
    // THE CONTRACT. An RPC server with no auth manager checks no credentials, so
    // the only thing standing between the default daemon and an unauthenticated
    // control channel is the bind address. Assert it for both branches — the
    // unauthenticated one is where it actually matters.
    for (label, config) in [
        ("default (no auth installed)", Config::default()),
        ("gateway enabled (auth installed)", enabled_gateway_config()),
    ] {
        let (rpc_config, server) = server_for(&config);
        let authed = server.auth_manager().is_some();

        assert!(
            rpc_config.addr.ip().is_loopback(),
            "RPC bound to {} in the {label} case (auth installed: {authed}). An RPC server \
             reachable off-loopback without an auth manager is an unauthenticated control \
             channel: handle_rpc skips its auth block entirely when auth_manager is None.",
            rpc_config.addr.ip()
        );
    }
}

#[test]
fn the_unauthenticated_branch_is_reachable_from_operator_config() {
    // Control for the assertion above: prove the no-auth branch is not dead code
    // that the loopback check trivially passes over. If a future change makes auth
    // mandatory, this test fails and the loopback contract can be revisited
    // deliberately rather than quietly relied upon.
    let (_, server) = server_for(&Config::default());
    assert!(
        server.auth_manager().is_none(),
        "the unauthenticated branch is no longer reachable — if RPC auth is now \
         mandatory, rpc_never_binds_off_loopback_whether_or_not_auth_is_installed is \
         no longer load-bearing and this pair should be revisited together"
    );
}
