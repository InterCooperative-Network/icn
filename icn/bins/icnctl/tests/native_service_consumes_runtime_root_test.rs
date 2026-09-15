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

// ── A: the EFFECTIVE profile, not just the base unit ───────────────────────
//
// The appliance installs `20-demo-profile.conf` into
// `/etc/systemd/system/icnd.service.d/`, and that drop-in REPLACES `ExecStart`
// wholesale. Proving the base unit alone would leave the composed profile —
// the thing that actually runs on the image — unproven, and a drop-in that
// forgot `--config` would silently reinstate icn#2755 on that profile only.

/// Apply systemd's documented drop-in semantics to a base unit and its
/// drop-ins, in lexical order.
///
/// Rules modelled, from systemd.unit(5): drop-ins are applied after the base
/// unit; a directive assigned again overrides the earlier value; and assigning
/// an **empty** value to a list-valued directive such as `ExecStart` resets the
/// list, which is why `20-demo-profile.conf` writes `ExecStart=` before its own.
fn compose_unit(base: &Path, dropins: &[PathBuf]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for file in std::iter::once(base).chain(dropins.iter().map(|p| p.as_path())) {
        let text = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("fixture: could not read {}: {e}", file.display()));
        let mut joined: Vec<String> = Vec::new();
        let mut cont = String::new();
        for raw in text.lines() {
            let t = raw.trim();
            if t.starts_with('#') || t.starts_with(';') {
                continue;
            }
            if let Some(stripped) = t.strip_suffix('\\') {
                cont.push_str(stripped.trim_end());
                cont.push(' ');
                continue;
            }
            if !cont.is_empty() {
                cont.push_str(t);
                joined.push(std::mem::take(&mut cont));
            } else {
                joined.push(t.to_string());
            }
        }
        for line in joined {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim().to_string(), v.trim().to_string());
            if k.is_empty() {
                continue;
            }
            if v.is_empty() {
                // Reset: drop everything previously assigned to this key.
                out.retain(|(ek, _)| *ek != k);
                continue;
            }
            // Non-list directives override; ExecStart accumulates after a reset.
            if k != "ExecStart" {
                out.retain(|(ek, _)| *ek != k);
            }
            out.push((k, v));
        }
    }
    out
}

fn effective(directives: &[(String, String)], key: &str) -> Vec<String> {
    directives
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .collect()
}

/// Every shipped `icnd` drop-in, discovered rather than named.
///
/// The first version of this witness composed the base unit with the demo
/// drop-in only — and MISSED `deploy/appliance/lan/icnd-30-lan-origin.conf.in`,
/// which also resets `ExecStart`, sorts after the demo override, and shipped
/// without `--config`. A hardcoded list of drop-ins tests the drop-ins someone
/// remembered; enumerating them tests the profile.
fn shipped_icnd_dropins(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.join("deploy")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            // `.conf` and `.conf.in` (templated at image-build time) both ship.
            let is_conf = name.ends_with(".conf") || name.ends_with(".conf.in");
            // A drop-in for *icnd* specifically: either it lives in an
            // `icnd.service.d/` directory, or its own name says `icnd`.
            let for_icnd = name.contains("icnd")
                || dir
                    .file_name()
                    .map(|d| d.to_string_lossy().contains("icnd.service.d"))
                    .unwrap_or(false);
            if is_conf && for_icnd {
                found.push(p);
            }
        }
    }
    // Sort by the name each file is INSTALLED as, not by its path in the
    // repository. systemd orders drop-ins lexically within
    // `icnd.service.d/`, and `build-image.sh` renders
    // `lan/icnd-30-lan-origin.conf.in` to `30-lan-origin.conf` — so sorting by
    // repo path would put `appliance/lan/...` before `appliance/systemd/...`
    // and get the override order backwards, which is precisely the ordering the
    // LAN defect depended on.
    found.sort_by_key(|p| installed_dropin_name(p));
    found
}

/// The filename a shipped drop-in is installed as under `icnd.service.d/`.
fn installed_dropin_name(p: &Path) -> String {
    let n = p
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let n = n.strip_suffix(".in").unwrap_or(&n).to_string();
    n.strip_prefix("icnd-").unwrap_or(&n).to_string()
}

/// EVERY shipped drop-in that replaces `ExecStart` must carry `--config`.
///
/// systemd applies drop-ins in lexical order and the last `ExecStart` wins, so
/// one override that forgets the flag reinstates icn#2755 for whichever profile
/// installs it — regardless of how correct the base unit and the other drop-ins
/// are.
#[test]
fn every_shipped_dropin_that_replaces_execstart_keeps_config() {
    let root = repo_root();
    let dropins = shipped_icnd_dropins(&root);
    assert!(
        dropins.len() >= 3,
        "fixture: expected to discover the firstboot, demo and LAN drop-ins, found {dropins:?}"
    );

    let mut checked = 0;
    for d in &dropins {
        let text = std::fs::read_to_string(d).unwrap();
        // Only drop-ins that actually redefine ExecStart can lose the flag.
        let redefines = text
            .lines()
            .any(|l| l.trim().starts_with("ExecStart=") && l.trim() != "ExecStart=");
        if !redefines {
            continue;
        }
        checked += 1;
        assert!(
            text.contains("--config /var/lib/icn/icn.toml"),
            "icn#2755: {} replaces ExecStart but does not pass --config, so the \
             profile installing it runs with Config::default() and the node-DID \
             treasury fallback",
            d.display()
        );
    }
    assert!(
        checked >= 2,
        "fixture: expected at least the demo and LAN overrides to redefine \
         ExecStart, saw {checked}"
    );
}

/// The composed appliance profile must retain BOTH properties.
///
/// What this proves: systemd parses the base unit and the drop-in together as
/// one unit (asserted through `systemd-analyze verify`, which reports errors
/// against the drop-in's own path, so it demonstrably reads it); and under
/// systemd's documented override semantics the composed `ExecStart` still
/// carries `--config <data_dir>/icn.toml` while the composed `UMask` is still
/// the base unit's `0077`.
///
/// What this does NOT prove: the in-memory property values of a running systemd
/// manager. That needs a live manager and an installed binary, neither of which
/// exists in CI. The composition semantics are modelled here from
/// systemd.unit(5), and the negative control below is what keeps that model
/// honest — it fails if a drop-in drops `--config`.
#[test]
fn the_composed_appliance_profile_keeps_config_and_umask() {
    let root = repo_root();
    let base = root.join("deploy").join("icnd.service");
    let dropin = root
        .join("deploy/appliance/systemd/icnd.service.d")
        .join("20-demo-profile.conf");
    assert!(dropin.is_file(), "fixture: missing {}", dropin.display());

    // 1. systemd itself must accept the pair as a composed unit.
    let staged = TempDir::new().unwrap();
    let unit = staged.path().join("icnd.service");
    let dd = staged.path().join("icnd.service.d");
    std::fs::create_dir_all(&dd).unwrap();
    std::fs::copy(&base, &unit).unwrap();
    std::fs::copy(&dropin, dd.join("20-demo-profile.conf")).unwrap();
    if let Ok(out) = Command::new("systemd-analyze")
        .arg("verify")
        .arg(&unit)
        .output()
    {
        let text = String::from_utf8_lossy(&out.stderr).to_string()
            + &String::from_utf8_lossy(&out.stdout);
        // The only tolerated complaint is the binary being absent on a build host.
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            assert!(
                line.contains("is not executable") || line.contains("No such file or directory"),
                "systemd rejected the composed profile: {line}"
            );
        }
    }

    // 2. The composed directives must carry both properties.
    // Compose the base with EVERY shipped drop-in, in the order systemd would
    // apply them, so the last override to win is the one actually asserted on.
    let all: Vec<PathBuf> = shipped_icnd_dropins(&root)
        .into_iter()
        .filter(|p| p != &base)
        .collect();
    let composed = compose_unit(&base, &all);
    let exec = effective(&composed, "ExecStart");
    assert_eq!(
        exec.len(),
        1,
        "the last drop-in to reset ExecStart leaves exactly one: {exec:?}"
    );
    assert!(
        exec[0].contains("--config /var/lib/icn/icn.toml"),
        "icn#2755: the COMPOSED profile must still pass the provisioned \
         configuration; the drop-in replaces ExecStart wholesale, so dropping it \
         here reinstates the node-DID fallback on the appliance profile:\n{}",
        exec[0]
    );
    let umask = effective(&composed, "UMask");
    assert_eq!(
        umask,
        vec!["0077".to_string()],
        "the composed profile must retain the pinned umask"
    );
    // The demo drop-in must not have quietly loosened the bind either.
    assert!(
        exec[0].contains("--gateway-bind"),
        "sanity: the composed ExecStart is the drop-in's: {}",
        exec[0]
    );
}

/// Negative control for the composition model above.
///
/// Without this, `compose_unit` could be wrong in a way that makes the real
/// assertion pass for the wrong reason. A drop-in that resets `ExecStart` and
/// omits `--config` must be detected.
#[test]
fn the_composition_check_fails_when_a_dropin_drops_config() {
    let root = repo_root();
    let base = root.join("deploy").join("icnd.service");

    let tmp = TempDir::new().unwrap();
    let bad = tmp.path().join("99-bad.conf");
    std::fs::write(
        &bad,
        "[Service]\nExecStart=\nExecStart=/usr/local/bin/icnd \\\n    --data-dir /var/lib/icn \\\n    --gateway-enable\n",
    )
    .unwrap();

    let composed = compose_unit(&base, &[bad]);
    let exec = effective(&composed, "ExecStart");
    assert_eq!(
        exec.len(),
        1,
        "the bad drop-in resets and sets one: {exec:?}"
    );
    assert!(
        !exec[0].contains("--config"),
        "the negative control must actually lose --config, or it controls nothing: {}",
        exec[0]
    );
    // And the umask must survive a drop-in that does not mention it.
    assert_eq!(effective(&composed, "UMask"), vec!["0077".to_string()]);
}
