//! icn#2755 — the configuration provisioned for the Alpha institution must be
//! the configuration the native `icnd` service demonstrably consumes.
//!
//! The defect was not a parse difference. `deploy/icnd.service` invoked `icnd
//! --data-dir /var/lib/icn` with no `--config`, and `icnd` has no
//! auto-discovery — `bins/icnd/src/main.rs` falls back to `Config::default()`,
//! whose `cooperative.treasury_did` is `None`. The ledger then resolved the
//! treasury to the NODE DID, so a successfully provisioned institutional
//! runtime root had no effect on the daemon an operator actually runs, and node
//! identity silently stood in for institutional identity.
//!
//! The witness therefore takes `ExecStart` from the SHIPPED UNIT rather than
//! composing an `icnd --config …` invocation of its own. A test that spelled the
//! arguments itself would pass while the unit that operators run stayed broken,
//! which is precisely the gap that let this survive.
//!
//! Two substitutions are made, both environmental and neither touching the
//! property under test: `/var/lib/icn` becomes a temp data directory, and the
//! gateway binds an ephemeral port so concurrent tests do not collide. The
//! `--config` argument is used exactly as the artifact spells it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::TempDir;

const PASSPHRASE: &str = "icn-2755-native-config-fixture";

fn repo_root() -> PathBuf {
    // <repo>/icn/bins/icnctl -> <repo>
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("fixture: could not resolve repository root")
        .to_path_buf()
}

fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

/// `icnd` lives beside `icnctl` in the same target directory. `cargo test
/// --workspace --test '*'` (what CI runs) compiles every binary before running
/// any test, so this exists. Fail loudly rather than skipping if it does not —
/// a skipped witness is not evidence.
fn icnd_bin() -> PathBuf {
    let p = icnctl_bin().parent().unwrap().join("icnd");
    assert!(
        p.is_file(),
        "fixture: {} is missing. This witness drives the real daemon; build it \
         with `cargo build -p icnd` (CI's `cargo test --workspace` already does).",
        p.display()
    );
    p
}

/// Parse `ExecStart=` out of a systemd unit, honouring line continuations.
fn exec_start_argv(unit: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(unit)
        .unwrap_or_else(|e| panic!("fixture: could not read {}: {e}", unit.display()));
    let mut joined = String::new();
    let mut in_exec = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        if !in_exec {
            if let Some(rest) = t.strip_prefix("ExecStart=") {
                in_exec = true;
                joined.push_str(rest.trim_end_matches('\\'));
                if !t.ends_with('\\') {
                    break;
                }
                joined.push(' ');
            }
        } else {
            joined.push_str(t.trim_end_matches('\\'));
            if !t.ends_with('\\') {
                break;
            }
            joined.push(' ');
        }
    }
    assert!(in_exec, "fixture: no ExecStart= in {}", unit.display());
    joined.split_whitespace().map(str::to_string).collect()
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// THE witness for icn#2755.
#[test]
fn the_shipped_unit_starts_a_daemon_that_uses_the_provisioned_treasury() {
    let unit = repo_root().join("deploy").join("icnd.service");
    let argv = exec_start_argv(&unit);

    // 1. The artifact itself must carry the configuration. Asserted against the
    //    shipped file, so deleting the flag fails here rather than silently
    //    reinstating the node-DID fallback.
    let cfg_idx = argv.iter().position(|a| a == "--config").unwrap_or_else(|| {
        panic!("icn#2755: {} passes no --config, so a provisioned runtime root cannot reach the daemon:\n{argv:?}", unit.display())
    });
    assert_eq!(
        argv[cfg_idx + 1],
        "/var/lib/icn/icn.toml",
        "the unit must name the canonical native configuration path"
    );

    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("icn");

    // 2. Fresh native install shape: identity + configuration, nothing else.
    let init = Command::new(icnd_bin())
        .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        .args(["--init", "--data-dir"])
        .arg(&data_dir)
        .output()
        .expect("fixture: could not run `icnd --init`");
    assert!(
        init.status.success(),
        "fixture: `icnd --init` failed:\n{}{}",
        String::from_utf8_lossy(&init.stdout),
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(
        data_dir.join("icn.toml").is_file(),
        "fixture: `icnd --init` must write the canonical config the unit reads"
    );

    // 3. Provision the institutional runtime root.
    let prov = Command::new(icnctl_bin())
        .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        .arg("--data-dir")
        .arg(&data_dir)
        .args([
            "institution",
            "runtime-root",
            "create",
            "--name",
            "icn-2755-witness",
            "--yes",
        ])
        .output()
        .expect("fixture: could not run runtime-root create");
    assert!(
        prov.status.success(),
        "fixture: runtime-root provisioning failed:\n{}{}",
        String::from_utf8_lossy(&prov.stdout),
        String::from_utf8_lossy(&prov.stderr)
    );

    // The treasury the ceremony published, read back from the configuration the
    // unit is about to consume.
    let cfg = std::fs::read_to_string(data_dir.join("icn.toml")).unwrap();
    let treasury = cfg
        .lines()
        .find_map(|l| {
            let t = l.trim();
            t.strip_prefix("treasury_did")
                .and_then(|r| r.split('"').nth(1))
        })
        .unwrap_or_else(|| {
            panic!("fixture: provisioning wrote no treasury_did into icn.toml:\n{cfg}")
        })
        .to_string();

    // 4. Start the daemon through the UNIT's own argv, with only the data root
    //    and the port remapped.
    let port = free_port();
    let mut cmd = Command::new(icnd_bin());
    let mut it = argv.iter().skip(1); // argv[0] is the installed binary path
    while let Some(a) = it.next() {
        match a.as_str() {
            "--data-dir" => {
                it.next();
                cmd.arg("--data-dir").arg(&data_dir);
            }
            "--config" => {
                let unit_path = it.next().unwrap();
                let remapped = unit_path.replace("/var/lib/icn", data_dir.to_str().unwrap());
                cmd.arg("--config").arg(remapped);
            }
            "--gateway-bind" => {
                it.next();
                cmd.arg("--gateway-bind").arg(format!("127.0.0.1:{port}"));
            }
            other => {
                cmd.arg(other);
            }
        }
    }
    let mut child = cmd
        .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        .env("RUST_LOG", "icn_core=debug,info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("fixture: could not start icnd through the unit's argv");

    // 5. Read until the treasury decision is logged, or give up.
    //
    // `icnd` writes its tracing output to STDOUT. Watching only stderr returned
    // an empty log, and an empty log satisfies every "must not contain" check
    // vacuously — the first version of this test passed its negative assertion
    // while proving nothing. Drain stderr on its own thread too, so a full pipe
    // cannot wedge the child.
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    std::thread::spawn(
        move || {
            for _ in BufReader::new(stderr).lines().map_while(Result::ok) {}
        },
    );
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut seen = String::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            seen.push_str(&line);
            seen.push('\n');
            if line.contains("treasury principal") || line.contains("No treasury_did configured") {
                let _ = tx.send(seen.clone());
                return;
            }
        }
        let _ = tx.send(seen);
    });
    let log = rx
        .recv_timeout(std::time::Duration::from_secs(120))
        .unwrap_or_default();
    let _ = child.kill();
    let _ = child.wait();

    // Guard against the vacuous pass: the daemon must actually have reached the
    // point where it decides a treasury.
    assert!(
        log.contains("ICNd starting"),
        "the daemon did not start through the unit's argv, so nothing below is \
         evidence:\n{log}"
    );
    assert!(
        log.contains("treasury principal") || log.contains("No treasury_did configured"),
        "the daemon never reached its treasury decision within the timeout:\n{log}"
    );

    assert!(
        !log.contains("No treasury_did configured"),
        "icn#2755: the daemon started through the shipped unit fell back to the \
         node DID, so the provisioned treasury never reached it:\n{log}"
    );
    assert!(
        log.contains(&treasury),
        "the effective treasury must be the provisioned institutional principal \
         ({treasury}):\n{log}"
    );
}

/// B3 — the Alpha profile must pin the service umask, and the pin must have the
/// effect it is pinned for.
///
/// `.mode(0o600)` is a REQUEST: the kernel applies `mode & ~umask`, so an
/// inherited umask with owner bits set turns a 0600 request into mode 000 — an
/// unusable lock anchor. `icn-core`'s data-directory lock and the runtime-root
/// ceremony both create files that way, and the containment ledger admits that
/// hazard only "provided the pinned appliance profile fixes the service umask".
///
/// Grepping the unit for `UMask=` would prove only that a string is present, so
/// this also runs the daemon under the mask the unit declares and inspects what
/// it actually created.
#[test]
fn the_pinned_umask_makes_daemon_created_files_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let unit = repo_root().join("deploy").join("icnd.service");
    let text = std::fs::read_to_string(&unit).unwrap();
    let mask = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("UMask="))
        .unwrap_or_else(|| {
            panic!(
                "the Alpha profile must pin a service umask; {} declares none",
                unit.display()
            )
        })
        .trim()
        .to_string();

    // A mask that clears an owner bit is the hazard itself, not a fix.
    let numeric = u32::from_str_radix(mask.trim_start_matches("0o"), 8)
        .unwrap_or_else(|_| panic!("UMask={mask} is not octal"));
    assert_eq!(
        numeric & 0o600,
        0,
        "UMask={mask} clears owner bits, which is the failure it exists to prevent"
    );

    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("icn");

    // Run under exactly that mask, the way systemd applies it: set, then exec.
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "umask {mask}; exec \"$0\" --init --data-dir \"$1\"",
        ))
        .arg(icnd_bin())
        .arg(&data_dir)
        .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        .output()
        .expect("fixture: could not run icnd under the pinned umask");
    assert!(
        out.status.success(),
        "fixture: `icnd --init` failed under umask {mask}:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Representative sensitive and non-sensitive artifacts of the same run.
    let mut checked = 0;
    for name in ["identity.age", "icn.toml", "genesis.json"] {
        let p = data_dir.join(name);
        if !p.is_file() {
            continue;
        }
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode & 0o077,
            0,
            "{name} is readable beyond its owner under the pinned umask (mode {mode:o})"
        );
        assert!(
            mode & 0o400 != 0,
            "{name} lost its owner read bit under the pinned umask (mode {mode:o}) — \
             this is the mode-000 hazard the pin exists to prevent"
        );
        checked += 1;
    }
    assert!(
        checked >= 2,
        "fixture: expected at least two daemon-created files to inspect, saw {checked}"
    );
}
