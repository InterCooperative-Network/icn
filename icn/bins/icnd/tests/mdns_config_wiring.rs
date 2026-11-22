//! The daemon must hand `network.mdns_enabled` to the network layer.
//!
//! `icn-net`'s own test shows that `NetworkActor::spawn_with_mdns(.., false)` never starts
//! discovery. That says nothing about whether the daemon calls it with the operator's setting —
//! a supervisor that went back to `NetworkActor::spawn` would keep every `icn-net` test green
//! while advertising the node's DID on every interface again. So this drives the real `icnd`
//! binary from a real configuration file and reads what its network layer did: with the setting
//! off, discovery must never start; with it on (the positive control), it must.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

fn icnd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnd"))
}

const PASSPHRASE: &str = "test-only-not-a-real-credential";

/// `icnd --init` into a fresh directory, rebound to loopback, with `mdns_enabled` set.
fn node_config(dir: &Path, mdns_enabled: bool) -> PathBuf {
    let init = std::process::Command::new(icnd_bin())
        .args(["--init", "--node-name", "mdns-wiring-witness"])
        .arg("--data-dir")
        .arg(dir)
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
    let path = icn_core::config::config_file_path(dir);
    let generated = std::fs::read_to_string(&path).unwrap();
    assert!(
        generated.contains("mdns_enabled = true"),
        "fixture: the generated config states mdns_enabled"
    );
    let edited = generated.replace("0.0.0.0:", "127.0.0.1:").replace(
        "mdns_enabled = true",
        &format!("mdns_enabled = {mdns_enabled}"),
    );
    std::fs::write(&path, edited).unwrap();
    path
}

/// Run the daemon until its network layer is up, then stop it and return its log.
fn network_startup_log(config: &Path, data_dir: &Path) -> String {
    let log_dir = tempfile::TempDir::new().unwrap();
    let log_path = log_dir.path().join("icnd.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let mut child = std::process::Command::new(icnd_bin())
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

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let mut text = String::new();
    while std::time::Instant::now() < deadline {
        text = std::fs::read_to_string(&log_path).unwrap_or_default();
        if text.contains("Network actor running") {
            break;
        }
        if let Ok(Some(status)) = child.try_wait() {
            panic!("fixture: icnd exited ({status}) before its network came up:\n{text}");
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        text.contains("Network actor running"),
        "fixture: the network layer never came up:\n{text}"
    );
    text
}

#[test]
fn the_daemon_hands_mdns_enabled_to_the_network_layer() {
    // Positive control first: through this same binary and log, discovery visibly starts.
    let on = tempfile::TempDir::new().unwrap();
    let log = network_startup_log(&node_config(on.path(), true), on.path());
    assert!(
        log.contains("Discovery service starting"),
        "positive control: with mdns_enabled = true the daemon must start discovery:\n{log}"
    );

    let off = tempfile::TempDir::new().unwrap();
    let log = network_startup_log(&node_config(off.path(), false), off.path());
    assert!(
        !log.contains("Discovery service starting"),
        "mdns_enabled = false must reach the network layer: the daemon started discovery \
         (it advertises its DID and browses):\n{log}"
    );
    assert!(
        log.contains("mDNS discovery disabled by configuration"),
        "the daemon must report discovery disabled:\n{log}"
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
