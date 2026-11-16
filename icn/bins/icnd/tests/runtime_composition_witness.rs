//! Runtime composition witness: what the shipped `icnd` actually assembles (#2544, #2500).
//!
//! # The defect class
//!
//! A capability can exist, pass its isolated tests, and never be installed by the daemon's real
//! composition root (#2500: a token-revocation list, the `net` policy oracle — #2488, which
//! silenced all networking — and the RPC node keypair). Tests that build their own wiring cannot
//! see that, because they prove the mechanism, not the installation.
//!
//! # What this suite observes, and how strongly
//!
//! | test | boundary | observable |
//! |---|---|---|
//! | `the_shipped_daemon_assembles_the_contracted_policy_domains_across_a_restart` | real `icnd` binary, `--init` node, loopback, SIGKILL + restart | the supervisor's own report of `OracleRegistry::domains()` at the Running transition, and its service-registry report |
//! | `without_an_identity_the_daemon_assembles_no_policy_registry` | real binary, no keystore | the identity-less branch is taken and the registry report is absent |
//! | `the_daemon_policy_components_report_the_contracted_domains` | the real app components `build_services` installs | each component's `PolicyOracle::domain()` |
//!
//! The binary-level domain set is read from a log line the daemon prints from the live registry
//! (`lifecycle.rs`, "OracleRegistry phase: Running (registered domains: …)"). That is the daemon's
//! own report, not an independent probe: no test here sends a policy query into a running
//! daemon. Decision routing is proven one layer down, through the real `register_core_oracles`
//! and the real gossip and network consumers, in `icn-core`'s
//! `supervisor::lifecycle::core_oracle_registration_tests`. Together the two say: the shipped
//! daemon installs these components (here), they report these domains (here), and the real
//! registration function routes the consumers' queries to them (there).
//!
//! # What it does not cover
//!
//! * No policy decision is requested from a running daemon (no public surface exposes one).
//! * Whether the `charter` domain *should* have a registry consumer — none exists today.
//! * Whether an identity-bearing node should refuse to run with a missing domain. Today it does
//!   not; that is a production-behaviour decision for #2500, not a test change.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::{Duration, Instant};

use icn_kernel_api::authz::{Domain, PolicyOracle};

const PASSPHRASE: &str = "test-only-not-a-real-credential";
const DEADLINE: Duration = Duration::from_secs(120);

/// Printed by the supervisor immediately after `OracleRegistry::set_phase(Running)`, from the
/// live registry's `domains()`.
const RUNNING_REPORT: &str = "OracleRegistry phase: Running (registered domains: ";
/// Printed only on the identity-less branch, which is exclusive with the one above.
const NO_IDENTITY_REPORT: &str = "No identity bundle available - actors not spawned";

// ── the contract ────────────────────────────────────────────────────────────────────────────────

/// The policy domains an identity-bearing `icnd` registers.
///
/// `trust` and `net` are queried by assembled consumers (gossip subscription/pull/protocol ask
/// `Domain::trust()`; the network rate limiter asks `icn_net::NETWORK_DOMAIN`). `charter` is
/// registered from the charter app; no registry consumer queries it yet.
///
/// Deliberately absent: `ledger` and `governance`. Their app oracles exist, but nothing in the
/// assembled daemon queries them through this registry, and the ledger oracle is a permissive
/// placeholder. Registering either would widen authorization without a caller that needs it.
fn contracted_domains() -> BTreeSet<String> {
    [
        Domain::trust().0,
        icn_net::NETWORK_DOMAIN.to_string(),
        "charter".to_string(),
    ]
    .into_iter()
    .collect()
}

fn queried_by_assembled_consumers() -> [String; 2] {
    [Domain::trust().0, icn_net::NETWORK_DOMAIN.to_string()]
}

/// Services `build_services` installs whenever the node has an identity.
const INSTALLED_SERVICES: [&str; 3] = [
    "  - TrustService: provided",
    "  - GovernanceService: provided",
    "  - LedgerService: provided",
];

/// A `ServiceRegistry` slot with no production installer (#2500). Pinned so a future installer
/// has to update this witness rather than go unnoticed.
const UNINSTALLED_SERVICE: &str = "  - SecurityService: provided";

// ── tests ───────────────────────────────────────────────────────────────────────────────────────

#[test]
fn the_shipped_daemon_assembles_the_contracted_policy_domains_across_a_restart() {
    let node = Node::init("composition-witness");

    let mut first = node.start();
    let first_domains = first.registered_domains();
    assert_contract(&first_domains, &first.log());
    assert_installed_services(&first.log());

    // SIGKILL: no destructor, no store close. The restart must reopen the same data directory
    // (its storage locks released by process death) and assemble the same registry.
    first.sigkill();

    let mut second = node.start();
    let second_domains = second.registered_domains();
    assert_eq!(
        second_domains,
        first_domains,
        "a restarted daemon must assemble the same policy domains:\n{}",
        second.log()
    );
    assert_installed_services(&second.log());
    assert_eq!(
        second.log().matches(RUNNING_REPORT).count(),
        1,
        "one start, one Running transition"
    );

    let status = second.sigterm();
    assert!(
        status.success() || std::os::unix::process::ExitStatusExt::signal(&status).is_some(),
        "the daemon must stop on SIGTERM; got {status:?}"
    );
}

/// Control: the registry report above is produced by the identity-bearing assembly, not printed
/// unconditionally. A node with no keystore takes the other branch, and none of it appears.
#[test]
fn without_an_identity_the_daemon_assembles_no_policy_registry() {
    let node = Node::init("composition-witness-no-identity");
    let keystore = node.dir().join("identity.age");
    assert!(
        keystore.is_file(),
        "fixture: --init must create {keystore:?}"
    );
    std::fs::remove_file(&keystore).unwrap();

    let mut daemon = Daemon::spawn(&node.config, node.dir());
    daemon.wait_for_log(NO_IDENTITY_REPORT);
    let log = daemon.log();

    assert!(
        !log.contains(RUNNING_REPORT),
        "without an identity no policy registry is assembled, so none may be reported:\n{log}"
    );
    for line in INSTALLED_SERVICES {
        assert!(
            !log.contains(line),
            "without an identity build_services installs nothing, yet {line:?} was reported:\n{log}"
        );
    }
}

/// The components `build_services` hands to the supervisor report the domains the contract and
/// the consumers name. `register_core_oracles` registers by `oracle.domain()`, so a component
/// that reported a different domain would leave its consumers unserved while every stand-in
/// test still passed.
#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_policy_components_report_the_contracted_domains() {
    // Trust: constructed exactly as `build_services` does — a sled-backed graph behind a tokio
    // lock, the node keypair, and the same store for sequence numbers.
    let dir = tempfile::TempDir::new().unwrap();
    let store: std::sync::Arc<dyn icn_store::Store> =
        std::sync::Arc::new(icn_store::SledStore::open(dir.path()).unwrap());
    let keypair = icn_identity::KeyPair::generate().unwrap();
    let graph = icn_trust::TrustGraph::new(store.clone(), keypair.did().clone());
    let trust = icn_trust_app::create_service_tokio(
        std::sync::Arc::new(tokio::sync::RwLock::new(graph)),
        keypair,
        store,
    );

    let trust_domain = trust.oracle().domain();
    assert_eq!(
        trust_domain,
        Domain::trust(),
        "the shipped trust oracle must report the domain the gossip consumers query"
    );

    let charter_domain = icn_charter_app::CharterPolicyOracle::new().domain();

    let reported: BTreeSet<String> = [
        trust_domain.0,
        icn_net::NETWORK_DOMAIN.to_string(),
        charter_domain.0,
    ]
    .into_iter()
    .collect();
    assert_eq!(
        reported,
        contracted_domains(),
        "the trust and charter components plus the composition root's `net` registration \
         must make up exactly the contract"
    );
}

// ── assertions ──────────────────────────────────────────────────────────────────────────────────

fn assert_contract(domains: &BTreeSet<String>, log: &str) {
    assert_eq!(
        *domains,
        contracted_domains(),
        "the shipped daemon must register exactly the contracted policy domains:\n{log}"
    );
    for queried in queried_by_assembled_consumers() {
        assert!(
            domains.contains(&queried),
            "'{queried}' is queried by an assembled consumer but not registered — every such \
             query is denied in Running (the #2488 shape):\n{log}"
        );
    }
}

fn assert_installed_services(log: &str) {
    for line in INSTALLED_SERVICES {
        assert!(
            log.contains(line),
            "the identity-bearing daemon must report {line:?}:\n{log}"
        );
    }
    assert!(
        !log.contains(UNINSTALLED_SERVICE),
        "{UNINSTALLED_SERVICE:?} is reported: the security slot now has an installer. Update \
         this witness and #2500 together:\n{log}"
    );
}

/// Parse `[Domain("a"), Domain("b")]` from the Running report. `None` until it is printed.
fn parse_registered_domains(log: &str) -> Option<BTreeSet<String>> {
    let start = log.find(RUNNING_REPORT)? + RUNNING_REPORT.len();
    let rest = &log[start..];
    // The list ends at `])`; a bare `)` would cut inside the first `Domain("…")`.
    let list = &rest[..rest.find("])")?];
    Some(
        list.split("Domain(\"")
            .skip(1)
            .filter_map(|item| item.split('"').next())
            .map(str::to_string)
            .collect(),
    )
}

// ── a disposable node (same pattern as authority_restart_test.rs) ───────────────────────────────

struct Node {
    root: tempfile::TempDir,
    config: PathBuf,
}

impl Node {
    /// `icnd --init` into a fresh directory, rebound to loopback.
    fn init(name: &str) -> Self {
        let root = tempfile::TempDir::new().unwrap();
        let init = std::process::Command::new(icnd_bin())
            .args(["--init", "--node-name", name])
            .arg("--data-dir")
            .arg(root.path())
            .args(["--init-gateway-port", &free_tcp_port().to_string()])
            .args(["--init-gossip-port", &free_udp_port().to_string()])
            .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
            .output()
            .unwrap();
        assert!(
            init.status.success(),
            "fixture: --init must succeed:\n{}{}",
            String::from_utf8_lossy(&init.stdout),
            String::from_utf8_lossy(&init.stderr)
        );
        let config = icn_core::config::config_file_path(root.path());
        let generated = std::fs::read_to_string(&config).unwrap();
        let loopback = generated.replace("0.0.0.0:", "127.0.0.1:");
        assert_ne!(
            loopback, generated,
            "fixture: the generated config binds 0.0.0.0"
        );
        std::fs::write(&config, loopback).unwrap();
        Node { root, config }
    }

    fn dir(&self) -> &Path {
        self.root.path()
    }

    /// Start a daemon and wait until it has reported its assembled registry.
    fn start(&self) -> Daemon {
        let mut daemon = Daemon::spawn(&self.config, self.dir());
        daemon.wait_for_log(RUNNING_REPORT);
        daemon
    }
}

fn icnd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnd"))
}

fn free_tcp_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn free_udp_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A daemon that is killed however the test leaves, so a failed assertion never strands a process
/// holding a storage lock.
struct Daemon {
    child: std::process::Child,
    log_path: PathBuf,
    _log_dir: tempfile::TempDir,
}

impl Daemon {
    fn spawn(config: &Path, data_dir: &Path) -> Self {
        let log_dir = tempfile::TempDir::new().unwrap();
        let log_path = log_dir.path().join("icnd.log");
        let log = std::fs::File::create(&log_path).unwrap();
        let child = std::process::Command::new(icnd_bin())
            .arg("--config")
            .arg(config)
            .arg("--data-dir")
            .arg(data_dir)
            .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
            .stdin(std::process::Stdio::null())
            .stderr(log.try_clone().unwrap())
            .stdout(log)
            .spawn()
            .expect("the icnd binary must be runnable");
        Self {
            child,
            log_path,
            _log_dir: log_dir,
        }
    }

    fn log(&self) -> String {
        std::fs::read_to_string(&self.log_path).unwrap_or_default()
    }

    /// Block until `needle` is logged. Fails — with the log — if the daemon exits first or the
    /// deadline passes. No retries: a daemon that never reports is a failure, not a flake.
    fn wait_for_log(&mut self, needle: &str) {
        let deadline = Instant::now() + DEADLINE;
        while Instant::now() < deadline {
            if self.log().contains(needle) {
                return;
            }
            assert!(
                matches!(self.child.try_wait(), Ok(None)),
                "the daemon exited before logging {needle:?}:\n{}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("the daemon never logged {needle:?}:\n{}", self.log());
    }

    fn registered_domains(&self) -> BTreeSet<String> {
        let log = self.log();
        parse_registered_domains(&log)
            .unwrap_or_else(|| panic!("the Running report is incomplete:\n{log}"))
    }

    fn wait_for_exit(&mut self) -> Option<ExitStatus> {
        let deadline = Instant::now() + DEADLINE;
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().unwrap() {
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }

    /// SIGKILL and reap: no destructor, no flush, no chance to close a store.
    fn sigkill(&mut self) {
        self.child.kill().unwrap();
        let status = self.child.wait().unwrap();
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(libc::SIGKILL),
            "fixture: the daemon must have died of SIGKILL"
        );
    }

    fn sigterm(&mut self) -> ExitStatus {
        let pid = libc::pid_t::try_from(self.child.id()).unwrap();
        // SAFETY: `kill(2)` with a pid this test spawned and has not yet reaped.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);
        self.wait_for_exit()
            .unwrap_or_else(|| panic!("the daemon ignored SIGTERM:\n{}", self.log()))
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn the_running_report_parser_reads_every_domain_and_nothing_before_it() {
    let log = "x registered domains: [Domain(\"bogus\")]\n\
               INFO OracleRegistry phase: Running (registered domains: \
               [Domain(\"trust\"), Domain(\"net\"), Domain(\"charter\")])\n";
    assert_eq!(
        parse_registered_domains(log),
        Some(
            ["charter", "net", "trust"]
                .into_iter()
                .map(str::to_string)
                .collect()
        )
    );
    assert_eq!(parse_registered_domains("no report yet"), None);
}
