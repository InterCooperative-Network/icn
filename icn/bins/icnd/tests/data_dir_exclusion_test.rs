//! The daemon's half of the data-directory exclusion protocol (#2744).
//!
//! This lives in the `icnd` package on purpose. An equivalent test in `icnctl`
//! had to locate `icnd` by guessing a sibling path and skip when it was absent —
//! so `cargo test -p icnctl`, which is not obliged to build another package's
//! binary, could report success precisely because the subject under test did not
//! exist. Here `CARGO_BIN_EXE_icnd` is provided by Cargo for a binary it must
//! build, so there is nothing to skip.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

fn icnd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnd"))
}

/// A configuration the daemon *cannot* load.
///
/// That is the discriminator, not an accident. Asserting only that a held lock
/// makes `icnd` exit would prove it takes the lock *somewhere*, and a mutation
/// removing the pre-config acquisition survives that — the later acquisition
/// still refuses. With an unloadable configuration the two orderings answer
/// differently:
///
/// * lock first   -> the exclusion refusal
/// * config first -> a config load failure
fn write_unloadable_config(data_dir: &Path) -> PathBuf {
    let path = data_dir.join("icn.toml");
    std::fs::write(
        &path,
        "# deliberately not loadable by Config::from_file\ndata_dir = \"/nonexistent\"\n\
         [network]\nmdns_enabled = true\n",
    )
    .unwrap();
    path
}

/// Everything a runtime-root ceremony owns while it runs: the configuration it
/// will publish at `<root>/icn.toml`, and the storage under `<root>` it will
/// rewrite. Both, because a daemon may hold only one of them — see
/// [`a_running_daemon_keeps_the_configuration_it_consumed_when_its_storage_is_elsewhere`].
fn ceremony_locks(root: &Path) -> (icn_core::DataDirLock, icn_core::DataDirLock) {
    let configuration =
        icn_core::DataDirLock::acquire_config(root, "a maintenance ceremony").unwrap();
    let storage = icn_core::DataDirLock::acquire(root, "a maintenance ceremony").unwrap();
    (configuration, storage)
}

fn run_icnd(config: &Path) -> (bool, String) {
    let out = std::process::Command::new(icnd_bin())
        .arg("--config")
        .arg(config)
        .output()
        .expect("the icnd binary must be runnable");
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// The daemon refuses while another ICN process owns the data root, and refuses
/// **before it consumes the configuration**.
#[test]
fn the_daemon_refuses_before_consuming_a_configuration_of_an_owned_root() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = write_unloadable_config(dir.path());

    let held = ceremony_locks(dir.path());
    let (ok, text) = run_icnd(&config);
    drop(held);

    assert!(
        !ok,
        "the daemon must not start while the root is owned:\n{text}"
    );
    assert!(
        text.contains("already holds the configuration"),
        "it must refuse at the CONFIGURATION exclusion specifically. The storage \
         lock is taken after the read, so a storage refusal here would mean the \
         daemon had already consumed bytes a ceremony was republishing:\n{text}"
    );
    assert!(
        !text.contains("Failed to load config"),
        "and it must not have reached the configuration at all:\n{text}"
    );
}

/// Once the root is released, the same daemon invocation gets as far as the
/// configuration — proving the refusal above was the exclusion and not some
/// unrelated permanent failure.
#[test]
fn the_daemon_proceeds_to_the_configuration_once_the_root_is_free() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = write_unloadable_config(dir.path());

    let (ok, text) = run_icnd(&config);

    assert!(!ok, "this fixture's config is deliberately unloadable");
    assert!(
        text.contains("Failed to load config") || text.contains("missing field"),
        "with the root free the daemon must reach the configuration:\n{text}"
    );
    assert!(
        !text.contains("already holds"),
        "and must not report an exclusion that nobody holds:\n{text}"
    );
}

/// A symlinked configuration path must not let the daemon derive a different
/// exclusion identity from the one the ceremony owns.
///
/// Canonicalizing the *directory* does not cover this: the alias is in the file
/// component. `icnd --config /elsewhere/link.toml` pointing at
/// `<root>/icn.toml` would otherwise lock `/elsewhere`, contend with nobody, and
/// then follow the link and consume the very bytes a ceremony owning `<root>` is
/// about to replace.
#[test]
fn a_symlinked_config_path_still_resolves_to_the_managed_root() {
    let root = tempfile::TempDir::new().unwrap();
    let elsewhere = tempfile::TempDir::new().unwrap();
    let real_config = write_unloadable_config(root.path());

    let link = elsewhere.path().join("link.toml");
    std::os::unix::fs::symlink(&real_config, &link).unwrap();

    // A ceremony owns the REAL root.
    let held = ceremony_locks(root.path());

    let (ok, text) = run_icnd(&link);
    drop(held);

    assert!(!ok, "the daemon must not start:\n{text}");
    assert!(
        text.contains("already holds the configuration"),
        "the daemon must contend with the lock on the root the config really \
         lives in, not with one derived from the alias path:\n{text}"
    );
    assert!(
        !text.contains("Failed to load config"),
        "and must refuse before following the link to read it:\n{text}"
    );
}

/// A **running** daemon still owns the configuration it consumed, even when its
/// storage is somewhere else entirely.
///
/// This is the case a storage-only protocol cannot cover. `icnd --config
/// <A>/icn.toml --data-dir <B>` mutates `<B>` and contends for nothing in
/// `<A>` — so a ceremony rooted at `<A>`, whose whole job is to republish
/// `<A>/icn.toml`, would be admitted and would rewrite the file underneath a
/// live process still acting on the old contents. An earlier revision released
/// the configuration lock at exactly this point.
///
/// Two probes, because the fix has two halves and each has its own failure
/// mode:
///
/// * a **publisher** must be refused — that is the exclusion;
/// * a second **reader** must be admitted — daemons only read configurations,
///   and `scripts/demo-two-node.sh` keeps two of them in one directory with
///   different data roots. An exclusive daemon-side lock would pass the first
///   probe and silently break that demo.
#[test]
fn a_running_daemon_keeps_the_configuration_it_consumed_when_its_storage_is_elsewhere() {
    let config_root = tempfile::TempDir::new().unwrap();
    let data_root = tempfile::TempDir::new().unwrap();

    // A node whose storage is `data_root`, and whose configuration this test
    // then moves into a directory of its own.
    let init = std::process::Command::new(icnd_bin())
        .args(["--init", "--node-name", "exclusion-witness"])
        .arg("--data-dir")
        .arg(data_root.path())
        .args(["--init-gateway-port", &free_tcp_port().to_string()])
        .args(["--init-gossip-port", &free_udp_port().to_string()])
        .env("ICN_KEYSTORE_PASSPHRASE", "test-only-not-a-real-credential")
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "fixture: --init must succeed:\n{}{}",
        String::from_utf8_lossy(&init.stdout),
        String::from_utf8_lossy(&init.stderr)
    );

    let config = config_root.path().join("icn.toml");
    let generated = std::fs::read_to_string(data_root.path().join("config.toml")).unwrap();
    // Bind loopback rather than every interface: this witness needs a daemon
    // that stays up, not one that fights the host for a wildcard address.
    let loopback = generated.replace("0.0.0.0:", "127.0.0.1:");
    assert_ne!(
        loopback, generated,
        "fixture: the generated config binds 0.0.0.0"
    );
    std::fs::write(&config, loopback).unwrap();

    let mut daemon = Daemon::spawn(&config, data_root.path());
    daemon.wait_until_running();

    // The publisher's side: a ceremony rooted at the configuration directory.
    let publisher =
        icn_core::DataDirLock::acquire_config(config_root.path(), "runtime-root provisioning");
    // The reader's side: a second daemon with a configuration of its own here.
    let second_reader =
        icn_core::DataDirLock::acquire_config_shared_if_manageable(config_root.path(), "a daemon");
    // And the storage the daemon does mutate.
    let storage = icn_core::DataDirLock::acquire(data_root.path(), "a daemon");

    assert!(
        daemon.is_running(),
        "fixture: the daemon must still be up when the probes are taken:\n{}",
        daemon.log()
    );

    assert!(
        publisher.is_err(),
        "a ceremony must not republish the configuration of a running daemon, even \
         when that daemon's storage is elsewhere:\n{}",
        daemon.log()
    );
    assert!(
        format!("{:#}", publisher.unwrap_err()).contains("already holds the configuration"),
        "the refusal must name the configuration"
    );
    assert!(
        second_reader.unwrap().is_some(),
        "the daemon's hold is the reader's share: another daemon reading a \
         configuration from this directory must still start"
    );
    assert!(
        storage.is_err(),
        "and the storage the daemon mutates must remain exclusively its own"
    );
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

/// A daemon that is killed however the test leaves — including by a panic, so a
/// failed assertion never strands a process holding a lock.
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
            .env("ICN_KEYSTORE_PASSPHRASE", "test-only-not-a-real-credential")
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

    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Block until the daemon is past both acquisitions and serving.
    ///
    /// "Network actor running" is deliberately later than the locks: by then
    /// the QUIC endpoint is bound, so the daemon will not die of a port
    /// collision a moment after the probes are taken.
    fn wait_until_running(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while std::time::Instant::now() < deadline {
            if self.log().contains("Network actor running") {
                return;
            }
            assert!(
                self.is_running(),
                "fixture: the daemon exited before it was serving:\n{}",
                self.log()
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        panic!("fixture: the daemon never came up:\n{}", self.log());
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `--validate-config` must work where the daemon itself could not start.
///
/// It parses, prints a verdict and exits: no daemon, no retained interpretation
/// of those bytes, so nothing for a ceremony to invalidate and nothing to
/// exclude. Taking the configuration lock made the documented validation-only
/// command unusable wherever the account can read a configuration directory it
/// may not write — a root-owned `/etc/icn` inspected by a service or CI
/// account, which is an ordinary layout.
#[cfg(unix)]
#[test]
fn validate_config_works_in_a_directory_the_lock_cannot_be_created_in() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::TempDir::new().unwrap();
    let config = write_unloadable_config(dir.path());

    // Readable, not writable — the shape that yields EACCES on lock creation.
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let creatable = std::fs::File::create(dir.path().join(".probe")).is_ok();
    if creatable {
        let _ = std::fs::remove_file(dir.path().join(".probe"));
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        eprintln!(
            "SKIPPED validate_config_works_in_a_directory_the_lock_cannot_be_created_in: \
             this process can write through a 0500 directory (running as root?). Not \
             evidence in this environment."
        );
        return;
    }

    let out = std::process::Command::new(icnd_bin())
        .arg("--config")
        .arg(&config)
        .arg("--validate-config")
        .output()
        .expect("the icnd binary must be runnable");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !text.contains("may not create it"),
        "validation-only must not be blocked by the configuration lock:\n{text}"
    );
    // This fixture's configuration is deliberately unloadable, so validation
    // reports that — which is the point: it got far enough to have an opinion.
    assert!(
        text.contains("Failed to load config") || text.contains("missing field"),
        "it must reach the configuration and report on it:\n{text}"
    );
}

/// `--validate-config` must predict what normal startup does with a configured
/// treasury.
///
/// `resolve_treasury_did` refuses an unparseable `[cooperative] treasury_did`
/// rather than silently substituting the node's own DID, so a daemon started on
/// such a file is certain to fail — but it failed late, after stores and
/// services had begun initializing, while `--validate-config` reported success.
/// A validator that cannot predict a certain refusal is not validating.
///
/// The check is structural only: it parses the string. Whether that treasury
/// exists, holds key material, or is authorized are runtime questions, and
/// `--validate-config` is not a deployment-readiness check.
#[test]
fn validate_config_rejects_a_treasury_did_the_daemon_would_refuse() {
    let dir = tempfile::TempDir::new().unwrap();

    // Generated by the daemon itself, so the only thing under test is the
    // treasury field — a hand-written skeleton would fail to load for unrelated
    // reasons (icn#2747) and prove nothing.
    let init = std::process::Command::new(icnd_bin())
        .args(["--init", "--node-name", "validate-fixture"])
        .arg("--data-dir")
        .arg(dir.path())
        .env("ICN_KEYSTORE_PASSPHRASE", "test-only-not-a-real-credential")
        .output()
        .expect("the icnd binary must be runnable");
    assert!(
        init.status.success(),
        "fixture: --init must succeed:\n{}{}",
        String::from_utf8_lossy(&init.stdout),
        String::from_utf8_lossy(&init.stderr)
    );
    let base = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(
        base.contains("[cooperative]"),
        "fixture: the generated config is expected to carry an empty [cooperative] section"
    );
    assert!(
        !base.contains("treasury_did"),
        "fixture: and no treasury, so the field under test is the only variable"
    );

    let run = |name: &str, treasury: &str| -> String {
        let path = dir.path().join(name);
        let text = base.replace(
            "[cooperative]\n",
            &format!("[cooperative]\ntreasury_did = \"{treasury}\"\n"),
        );
        assert_ne!(
            text, base,
            "fixture: the treasury must actually be inserted"
        );
        std::fs::write(&path, text).unwrap();
        let out = std::process::Command::new(icnd_bin())
            .arg("--config")
            .arg(&path)
            .arg("--validate-config")
            .output()
            .expect("the icnd binary must be runnable");
        format!(
            "exit={}\n{}{}",
            out.status.success(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    };

    // `did:icn:treasury:...` is a spelling this repository already produces and
    // `Did::from_str` has never accepted: not multibase.
    let bad = run("bad.toml", "did:icn:treasury:notmultibase");
    assert!(
        bad.starts_with("exit=false"),
        "validation must reject a treasury the daemon would refuse:\n{bad}"
    );
    assert!(
        bad.contains("treasury_did"),
        "and must name the field, so an operator knows what to fix:\n{bad}"
    );

    // One fact different: a usable DID is still accepted, so the check is about
    // usability rather than about the section existing at all.
    // Generated rather than hand-written: `Did::from_str` requires multibase
    // bytes that decode to exactly 32 and form a valid Ed25519 point, so a
    // plausible-looking literal is not a valid control.
    let usable = icn_identity::KeyPair::generate().unwrap().did().to_string();
    let good = run("good.toml", &usable);
    // The outcome, not merely the absence of a word: the command could exit
    // non-zero for some unrelated reason and never mention the field, and a
    // negative-only control would still pass. Same base config, one fact
    // different.
    assert!(
        good.starts_with("exit=true"),
        "a structurally usable treasury DID must be accepted:\n{good}"
    );
    assert!(
        !good.contains("treasury_did"),
        "and must not be flagged (diagnostic):\n{good}"
    );
}
