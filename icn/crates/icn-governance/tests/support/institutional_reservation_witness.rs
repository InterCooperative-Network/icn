//! Test-only institution -> custodian -> auditor witness, using the exact-state v0 contract.
//!
//! The archive contains public evidence only. Separate processes exercise reload boundaries;
//! deterministic fixture keys and a shared test binary do not establish secret isolation.
//! The local store is an allocation observation, not a portable custody receipt or physical
//! inventory/fulfillment proof. All paths are private temporary fixtures, never service stores.
use super::*;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// Catches loss of archived authority, skipped verification, non-atomic allocation, replay,
// audit writes, or treating a historical decision as fresh permission after expiry.
#[test]
fn witness_survives_producer_and_custodian_exit() {
    let (dir, trust) = fixture();
    // A stuck child must fail promptly, retain both diagnostic streams, and be reaped.
    let timeout = child_with_timeout(dir.path(), &trust, "timeout-probe", Duration::from_secs(1))
        .expect_err("a stuck child must time out");
    assert!(timeout.contains("timed out after"), "{timeout}");
    assert!(timeout.contains("terminated and reaped"), "{timeout}");
    assert!(timeout.contains("timeout probe stdout"), "{timeout}");
    assert!(timeout.contains("timeout probe stderr"), "{timeout}");
    let pid = fs::read_to_string(dir.path().join("timeout.pid")).unwrap();
    assert!(
        !Path::new("/proc").join(pid).exists(),
        "timed-out child remains live or unreaped"
    );
    child_ok(dir.path(), &trust, "institution", "packet-synced");
    let packet = load_packet(&dir.path().join("packet")).unwrap();
    assert_eq!(packet.evidence.vote_bundles.len(), 2);
    assert_eq!(packet.evidence.authorization.body.inputs.len(), 2);
    let state =
        replay_context_log(&packet.evidence.inception, &packet.evidence.log, &rules()).unwrap();
    assert_eq!(state.tip().standing.len(), 3);
    let subjects: BTreeSet<_> = packet
        .evidence
        .vote_bundles
        .iter()
        .map(|b| b.subject())
        .collect();
    assert_eq!(subjects.len(), 2);
    assert_eq!(
        (
            packet.request.quantity,
            packet.request.expected_pool_revision
        ),
        (4, 0)
    );
    let before = snapshot(dir.path(), &trust);
    assert_eq!((before.0.available_capacity, before.0.revision), (10, 0));
    assert!(before.1.is_none() && before.2.is_none());
    child_ok(dir.path(), &trust, "custodian", "durable-confirmed");
    let committed = snapshot(dir.path(), &trust);
    assert_eq!(
        (committed.0.available_capacity, committed.0.revision),
        (6, 1)
    );
    assert!(committed.1.is_some() && committed.2.is_some());
    child_ok(dir.path(), &trust, "audit", "authorization-verified; reservation-observed; reserved=4; available=6; revision=1; fulfillment=unproven");
    child_ok(dir.path(), &trust, "custodian", "replayed");
    child_ok(
        dir.path(),
        &trust,
        "audit-late",
        "audit-time=1501; historical-only",
    );
    child_ok(dir.path(), &trust, "custodian-late", "expired");
    child_ok(dir.path(), &trust, "audit", "fulfillment=unproven");
    assert_eq!(snapshot(dir.path(), &trust), committed);

    // Replace the archive with genuine new votes and a writer signature for the same
    // context/operation, but three units instead of the four already reserved. This is
    // valid authority under the original pin, not a malformed or untrusted packet.
    fs::rename(
        dir.path().join("packet"),
        dir.path().join("original-packet"),
    )
    .unwrap();
    child_ok(dir.path(), &trust, "institution-conflict", "packet-synced");
    let alternate = load_packet(&dir.path().join("packet")).unwrap();
    let verified = preflight_checkpointed(
        &alternate.evidence,
        &trust.frontier(),
        &rules(),
        trust.target,
        NOW,
    )
    .unwrap();
    assert_eq!(verified.verified().authorization().verdict, Verdict::Pass);
    assert_eq!(alternate.request.context, packet.request.context);
    assert_eq!(alternate.request.operation_id, packet.request.operation_id);
    assert_eq!(alternate.request.quantity, 3);
    assert_ne!(
        alternate.request.commitment().unwrap(),
        packet.request.commitment().unwrap()
    );
    assert_ne!(
        alternate.evidence.authorization.encode(),
        packet.evidence.authorization.encode()
    );
    child_refused(
        dir.path(),
        &trust,
        "audit",
        "archive differs from consumed authorization/writer",
    );
    assert_eq!(snapshot(dir.path(), &trust), committed);
}

// Catches packet-supplied trust, evidence stripping, and unbound request/policy artifacts.
#[test]
fn witness_packet_cannot_supply_trust() {
    for (attack, expected) in [
        ("stripped-bundle", "authority-refused"),
        ("institution-other", "UntrustedContext"),
        ("institution-fork", "PinnedCheckpointMismatch"),
        ("institution-wrong-target", "WrongTarget"),
        ("institution-policy", "local-policy-refused"),
        ("parameters", "artifact hash mismatch"),
        ("policy", "artifact hash mismatch"),
        ("quantity", "signed request mismatch"),
        ("packet-trust", "unknown field"),
    ] {
        let (dir, trust) = fixture();
        let before = snapshot(dir.path(), &trust);
        if attack.starts_with("institution-") {
            child_ok(dir.path(), &trust, attack, "packet-synced");
        } else {
            child_ok(dir.path(), &trust, "institution", "packet-synced");
            let packet_dir = dir.path().join("packet");
            match attack {
                "stripped-bundle" => {
                    let mut m = read_manifest(&packet_dir);
                    let name = m.bundles.pop().unwrap();
                    fs::remove_file(packet_dir.join(name)).unwrap();
                    write_manifest(&packet_dir, &m);
                }
                "parameters" | "policy" => {
                    fs::write(packet_dir.join(format!("{attack}.bin")), b"substituted").unwrap();
                }
                "quantity" => {
                    let mut m = read_manifest(&packet_dir);
                    m.request.quantity = 3;
                    write_manifest(&packet_dir, &m);
                }
                "packet-trust" => edit_json(&packet_dir, |m| {
                    m["trust"] = serde_json::json!({"context": [0], "pin": [0], "min_seq": 0});
                }),
                _ => unreachable!(),
            }
        }
        child_refused(dir.path(), &trust, "custodian", expected);
        assert_eq!(snapshot(dir.path(), &trust), before, "{attack}");
        let other = trust_for(&world_in([0x2a; 32]));
        let store =
            LocalResourceReservations::open(dir.path().join("store"), trust.target).unwrap();
        assert!(store
            .inspect_consumed(ContextRef(other.context), OP)
            .unwrap()
            .is_none());
        assert!(store
            .inspect_reservation(ContextRef(other.context), OP)
            .unwrap()
            .is_none());
        assert!(store
            .inspect_pool(ContextRef(other.context), POOL)
            .unwrap()
            .is_none());
    }
    // Same target, pool id, and operation id, but independently trusted institutional contexts.
    let (dir, trust) = fixture();
    let original = snapshot(dir.path(), &trust);
    let other = trust_for(&world_in([0x2a; 32]));
    initialize(dir.path(), &other);
    child_ok(dir.path(), &other, "institution-other", "packet-synced");
    child_ok(dir.path(), &other, "custodian", "durable-confirmed");
    child_ok(dir.path(), &other, "audit", "reservation-observed");
    assert_eq!(snapshot(dir.path(), &trust), original);
    let other_committed = snapshot(dir.path(), &other);
    fs::remove_dir_all(dir.path().join("packet")).unwrap();
    child_ok(dir.path(), &trust, "institution", "packet-synced");
    child_ok(dir.path(), &trust, "custodian", "durable-confirmed");
    assert_eq!(snapshot(dir.path(), &other), other_committed);
}

// Catches permissive transport decoding before any allocation, including bounded reads and
// manifest path selection. Each malformed packet is submitted to the actual child loader.
#[test]
fn witness_malformed_packets_never_allocate_or_trigger_recovery_resubmit() {
    for attack in [
        "missing",
        "manifest-large",
        "binary-large",
        "packet-large",
        "version",
        "unknown",
        "request-unknown",
        "missing-field",
        "duplicate-field",
        "manifest-trailing",
        "logs-large",
        "bundles-large",
        "path",
        "absolute-path",
        "duplicate-name",
        "order",
        "symlink",
        "directory-symlink",
        "special-file",
        "extra-file",
        "trailing-inception",
        "trailing-authorization",
        "trailing-log",
        "trailing-bundle",
    ] {
        let (dir, trust) = fixture();
        child_ok(dir.path(), &trust, "institution", "packet-synced");
        let before = snapshot(dir.path(), &trust);
        corrupt_packet(dir.path(), attack);
        let reason = match attack {
            "manifest-large" | "binary-large" => "packet component size limit",
            "packet-large" => "whole packet size limit",
            "logs-large" | "bundles-large" => "packet list limit",
            "version" => "unsupported packet version",
            "unknown" | "request-unknown" => "unknown field",
            "duplicate-field" => "duplicate field",
            "path" | "absolute-path" => "fixed filename",
            "duplicate-name" | "order" => "ordered fixed filenames",
            "symlink" | "special-file" => "regular file",
            "directory-symlink" => "never a symlink",
            _ => "packet-refused",
        };
        child_refused(dir.path(), &trust, "custodian", reason);
        assert_eq!(snapshot(dir.path(), &trust), before, "{attack}");
    }
    let (dir, trust) = fixture();
    child_ok(dir.path(), &trust, "institution", "packet-synced");
    child_ok(dir.path(), &trust, "custodian", "durable-confirmed");
    let committed = snapshot(dir.path(), &trust);
    fs::remove_file(dir.path().join("packet/authorization.bin")).unwrap();
    child_refused(dir.path(), &trust, "audit", "packet-refused");
    assert_eq!(snapshot(dir.path(), &trust), committed);
}

// Catches converting a failed durability acknowledgment into confirmed success or allocating
// again during recovery. Normal close/reopen is not a power-loss durability simulation.
#[test]
fn witness_uncertainty_is_not_success() {
    let (dir, trust) = fixture();
    child_ok(dir.path(), &trust, "institution", "packet-synced");
    let report = child_ok(
        dir.path(),
        &trust,
        "custodian-uncertain",
        "uncertain; visible-local-commit; durability-unacknowledged",
    );
    assert!(!report.contains("durable-confirmed"));
    let observed = snapshot(dir.path(), &trust);
    assert_eq!((observed.0.available_capacity, observed.0.revision), (6, 1));
    child_ok(dir.path(), &trust, "audit", "reservation-observed");
    child_ok(dir.path(), &trust, "custodian", "replayed");
    assert_eq!(snapshot(dir.path(), &trust), observed);
}

#[test]
#[ignore = "subprocess helper; invoked with an explicit role and temporary root"]
fn witness_child() {
    let root = std::env::var_os("ICN_WITNESS_ROOT").expect("explicit temporary root");
    let role = std::env::var("ICN_WITNESS_ROLE").expect("explicit child role");
    let trust: Trust = serde_json::from_str(&std::env::var("ICN_WITNESS_TRUST").unwrap()).unwrap();
    if role == "timeout-probe" {
        println!("timeout probe stdout");
        eprintln!("timeout probe stderr");
        std::io::stdout().flush().unwrap();
        std::io::stderr().flush().unwrap();
        fs::write(
            Path::new(&root).join("timeout.pid"),
            std::process::id().to_string(),
        )
        .unwrap();
        std::thread::sleep(Duration::from_secs(3));
        return;
    }
    match run_phase(Path::new(&root), &trust, &role) {
        Ok(report) => println!("WITNESS {report}"),
        Err(reason) => {
            println!("WITNESS refused: {reason}");
            std::process::exit(23);
        }
    }
}

const POOL: [u8; 32] = [0x71; 32];
const PARAMETERS: &[u8] = b"purpose=collective-reservation-witness\n";
const POLICY: &[u8] = b"enabled=true\nmax_quantity=4\nunit=units\n";
const MANIFEST_LIMIT: usize = 64 * 1024;
const BINARY_LIMIT: usize = 1024 * 1024;
const PACKET_LIMIT: usize = 8 * 1024 * 1024;
const LIST_LIMIT: usize = 16;
type WitnessResult<T> = Result<T, String>;

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn require(condition: bool, reason: &str) -> WitnessResult<()> {
    if condition {
        Ok(())
    } else {
        Err(reason.into())
    }
}

fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

// This configuration travels out of band in the child environment, from the test operator.
// Neither verifier constructs a World or obtains a signing/continuity handle. Public packet
// metadata cannot change the context, pin, frontier, rule implementation or local target.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Trust {
    context: [u8; 32],
    pin: [u8; 32],
    seq: u64,
    target: [u8; 32],
}

impl Trust {
    fn frontier(&self) -> RelyingFrontierV0 {
        RelyingFrontierV0 {
            context: ContextRef(self.context),
            min_seq: self.seq,
            pinned: Some(LogCheckpointV0 {
                seq: self.seq,
                head: self.pin,
            }),
        }
    }
}

fn trust_for(w: &World) -> Trust {
    let cp = checkpoint(&w.state(&[w.genesis()]));
    Trust {
        context: w.ctx.0,
        pin: cp.head,
        seq: cp.seq,
        target: w.target,
    }
}

fn fixture() -> (tempfile::TempDir, Trust) {
    let dir = tempfile::tempdir().unwrap();
    let trust = trust_for(&world());
    initialize(dir.path(), &trust);
    (dir, trust)
}

fn initialize(root: &Path, trust: &Trust) {
    LocalResourceReservations::open(root.join("store"), trust.target)
        .unwrap()
        .initialize_pool(ResourcePoolV0 {
            context: ContextRef(trust.context),
            pool_id: POOL,
            custodian: trust.target,
            unit: "units".into(),
            total_capacity: 10,
            available_capacity: 10,
            revision: 0,
            policy: LocalReservationPolicyV0 {
                enabled: true,
                max_quantity_per_reservation: 4,
                policy_hash: hash(POLICY),
            },
        })
        .unwrap();
}

type Snapshot = (
    ResourcePoolV0,
    Option<ConsumedEntry>,
    Option<icn_authz_store::ReservationRecordV0>,
);

fn snapshot(root: &Path, trust: &Trust) -> Snapshot {
    let store = LocalResourceReservations::open(root.join("store"), trust.target).unwrap();
    let ctx = ContextRef(trust.context);
    (
        store.inspect_pool(ctx, POOL).unwrap().unwrap(),
        store.inspect_consumed(ctx, OP).unwrap(),
        store.inspect_reservation(ctx, OP).unwrap(),
    )
}

fn child(root: &Path, trust: &Trust, role: &str) -> std::process::Output {
    child_with_timeout(root, trust, role, Duration::from_secs(30))
        .unwrap_or_else(|reason| panic!("{reason}"))
}

fn child_with_timeout(
    root: &Path,
    trust: &Trust,
    role: &str,
    timeout: Duration,
) -> WitnessResult<std::process::Output> {
    // Files preserve both streams without a full pipe blocking the child, or reader
    // threads waiting indefinitely for EOF. They are anonymous temporary fixtures.
    let mut stdout = tempfile::tempfile().map_err(message)?;
    let mut stderr = tempfile::tempfile().map_err(message)?;
    let mut child = Command::new(std::env::current_exe().map_err(message)?)
        .args([
            "--ignored",
            "--exact",
            "institutional_reservation_witness::witness_child",
            "--nocapture",
        ])
        .env("ICN_WITNESS_ROLE", role)
        .env("ICN_WITNESS_ROOT", root)
        .env(
            "ICN_WITNESS_TRUST",
            serde_json::to_string(trust).map_err(message)?,
        )
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout.try_clone().map_err(message)?))
        .stderr(Stdio::from(stderr.try_clone().map_err(message)?))
        .spawn()
        .map_err(message)?;
    let started = Instant::now();
    let observed = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => (),
            Err(error) => break Err(format!("could not poll child: {error}")),
        }
        if started.elapsed() >= timeout {
            break Err(format!("timed out after {timeout:?}"));
        }
        std::thread::sleep(
            Duration::from_millis(10).min(timeout.saturating_sub(started.elapsed())),
        );
    };
    let (status, failure) = match observed {
        Ok(status) => (Ok(status), None),
        Err(reason) => {
            // Always wait, including when kill races a natural exit or reports an error.
            let killed = child.kill();
            let reaped = child.wait();
            let cleanup = if killed.is_ok() && reaped.is_ok() {
                "terminated and reaped".to_owned()
            } else {
                format!("cleanup: kill={killed:?}, wait={reaped:?}")
            };
            (
                reaped,
                Some(format!("{role} child {} {reason}; {cleanup}", child.id())),
            )
        }
    };
    stdout.rewind().map_err(message)?;
    stderr.rewind().map_err(message)?;
    let mut out = Vec::new();
    let mut err = Vec::new();
    stdout.read_to_end(&mut out).map_err(message)?;
    stderr.read_to_end(&mut err).map_err(message)?;
    if let Some(reason) = failure {
        return Err(format!(
            "{reason}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(&err)
        ));
    }
    Ok(std::process::Output {
        status: status.map_err(message)?,
        stdout: out,
        stderr: err,
    })
}

fn child_ok(root: &Path, trust: &Trust, role: &str, expected: &str) -> String {
    let output = child(root, trust, role);
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{role}: {report}");
    assert!(
        report.contains("WITNESS ") && report.contains(expected),
        "{role}: {report}"
    );
    println!("{role}: {expected}");
    report
}

fn child_refused(root: &Path, trust: &Trust, role: &str, expected: &str) {
    let output = child(root, trust, role);
    let report = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(23),
        "{role}: {report}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(report.contains("WITNESS refused:"), "{role}: {report}");
    assert!(
        report.contains(expected),
        "{role}: expected {expected}: {report}"
    );
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestFields {
    context: [u8; 32],
    operation_id: [u8; 32],
    pool_id: [u8; 32],
    expected_pool_revision: u64,
    quantity: u64,
    unit: String,
    request_params_hash: [u8; 32],
    policy_hash: [u8; 32],
}

impl RequestFields {
    fn request(&self) -> ReservationRequestV0 {
        ReservationRequestV0 {
            context: ContextRef(self.context),
            operation_id: self.operation_id,
            pool_id: self.pool_id,
            expected_pool_revision: self.expected_pool_revision,
            quantity: self.quantity,
            unit: self.unit.clone(),
            request_params_hash: self.request_params_hash,
            policy_hash: self.policy_hash,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u16,
    request: RequestFields,
    inception: String,
    authorization: String,
    parameters: String,
    policy: String,
    log: Vec<String>,
    bundles: Vec<String>,
}

impl Manifest {
    fn filenames(&self) -> WitnessResult<Vec<String>> {
        require(self.version == 1, "unsupported packet version")?;
        require(
            self.log.len() <= LIST_LIMIT && self.bundles.len() <= LIST_LIMIT,
            "packet list limit",
        )?;
        require(
            self.inception == "inception.bin"
                && self.authorization == "authorization.bin"
                && self.parameters == "parameters.bin"
                && self.policy == "policy.bin",
            "fixed filename required",
        )?;
        require(
            self.log == names("log", self.log.len())
                && self.bundles == names("bundle", self.bundles.len()),
            "ordered fixed filenames required",
        )?;
        let mut files = vec![
            self.inception.clone(),
            self.authorization.clone(),
            self.parameters.clone(),
            self.policy.clone(),
        ];
        files.extend(self.log.clone());
        files.extend(self.bundles.clone());
        Ok(files)
    }
}

fn names(prefix: &str, count: usize) -> Vec<String> {
    (0..count).map(|i| format!("{prefix}-{i:02}.bin")).collect()
}

fn write_synced(path: &Path, bytes: &[u8]) -> WitnessResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(message)?;
    file.write_all(bytes).map_err(message)?;
    file.sync_all().map_err(message)
}

// The only role that uses signing handles. All serialized objects are public protocol data.
fn institution(root: &Path, role: &str) -> WitnessResult<String> {
    let w = if role == "institution-other" {
        world_in([0x2a; 32])
    } else {
        world()
    };
    let log = if role == "institution-fork" {
        vec![w.genesis_with(&[w.m(0), w.m(1)], &[&w.founders[0], &w.founders[1]])]
    } else {
        vec![w.genesis()]
    };
    let policy = if role == "institution-policy" {
        b"enabled=true\nmax_quantity=9\nunit=units\n".as_slice()
    } else {
        POLICY
    };
    let fields = RequestFields {
        context: w.ctx.0,
        operation_id: OP,
        pool_id: POOL,
        expected_pool_revision: 0,
        quantity: if role == "institution-conflict" { 3 } else { 4 },
        unit: "units".into(),
        request_params_hash: hash(PARAMETERS),
        policy_hash: hash(policy),
    };
    let target = if role == "institution-wrong-target" {
        [0xee; 32]
    } else {
        w.target
    };
    let evidence = w.authorize_effect(
        &log,
        checkpoint(&w.state(&log)),
        &w.writer,
        &[w.m(0), w.m(1)],
        VoteChoice::For,
        Verdict::Pass,
        fields.request().effect(target).map_err(message)?,
        OP,
    );
    // Even the fork is independently valid without a retained pin. Its rejection must come
    // from the verifier's trusted checkpoint, not malformed signatures or failing votes.
    verify_checkpointed_authorization(&evidence, &w.frontier(1, None), &rules())
        .map_err(message)?;
    let m = Manifest {
        version: 1,
        request: fields,
        inception: "inception.bin".into(),
        authorization: "authorization.bin".into(),
        parameters: "parameters.bin".into(),
        policy: "policy.bin".into(),
        log: names("log", evidence.log.len()),
        bundles: names("bundle", evidence.vote_bundles.len()),
    };
    let dir = root.join("packet");
    fs::create_dir(&dir).map_err(message)?;
    write_synced(
        &dir.join(&m.inception),
        &evidence.inception.canonical_bytes(),
    )?;
    write_synced(
        &dir.join(&m.authorization),
        &evidence.authorization.encode(),
    )?;
    write_synced(&dir.join(&m.parameters), PARAMETERS)?;
    write_synced(&dir.join(&m.policy), policy)?;
    for (name, entry) in m.log.iter().zip(&evidence.log) {
        write_synced(&dir.join(name), &entry.encode())?;
    }
    for (name, bundle) in m.bundles.iter().zip(&evidence.vote_bundles) {
        write_synced(&dir.join(name), &bundle.canonical_bytes())?;
    }
    write_synced(
        &dir.join("manifest.json"),
        &serde_json::to_vec(&m).map_err(message)?,
    )?;
    File::open(&dir)
        .map_err(message)?
        .sync_all()
        .map_err(message)?;
    File::open(root)
        .map_err(message)?
        .sync_all()
        .map_err(message)?;
    Ok("packet-synced".into())
}

struct Packet {
    evidence: CheckpointedEvidenceV0,
    request: ReservationRequestV0,
    policy: Vec<u8>,
}

fn bounded_file(path: &Path, limit: usize) -> WitnessResult<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(message)?;
    require(
        metadata.file_type().is_file(),
        "packet component must be a regular file, never a symlink",
    )?;
    require(
        metadata.len() <= limit as u64,
        "packet component size limit",
    )?;
    // The fixture directory has one writer, finished before either reader starts. This is
    // strict static symlink refusal, not a concurrent hostile-filesystem transport protocol.
    let file = File::open(path).map_err(message)?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(message)?;
    require(bytes.len() <= limit, "packet component size limit")?;
    Ok(bytes)
}

fn load_packet(dir: &Path) -> WitnessResult<Packet> {
    require(
        fs::symlink_metadata(dir)
            .map_err(message)?
            .file_type()
            .is_dir(),
        "packet must be a directory, never a symlink",
    )?;
    let manifest_bytes = bounded_file(&dir.join("manifest.json"), MANIFEST_LIMIT)?;
    let m: Manifest = serde_json::from_slice(&manifest_bytes).map_err(message)?;
    // Validate every name before any manifest-selected I/O.
    let filenames = m.filenames()?;
    let mut expected: BTreeSet<String> = filenames.iter().cloned().collect();
    expected.insert("manifest.json".into());
    let actual: BTreeSet<String> = fs::read_dir(dir)
        .map_err(message)?
        .map(|entry| {
            entry.map_err(message).and_then(|e| {
                e.file_name()
                    .into_string()
                    .map_err(|_| "non-UTF8 packet filename".into())
            })
        })
        .collect::<WitnessResult<_>>()?;
    require(actual == expected, "missing or unexpected packet files")?;
    let mut total = manifest_bytes.len();
    let mut components = std::collections::BTreeMap::new();
    for name in filenames {
        let bytes = bounded_file(&dir.join(&name), BINARY_LIMIT)?;
        total += bytes.len();
        require(total <= PACKET_LIMIT, "whole packet size limit")?;
        components.insert(name, bytes);
    }
    // No decode (and hence no nested allocation) before the entire transport budget passes.
    let get = |name: &str| -> WitnessResult<&[u8]> {
        components
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| "missing packet component".into())
    };
    let inception = ContextInceptionV0::decode(get(&m.inception)?).map_err(message)?;
    let authorization =
        Signed::<CheckpointedAuthorizationV0>::decode(get(&m.authorization)?).map_err(message)?;
    let log = m
        .log
        .iter()
        .map(|n| SignedLogEntryV0::decode(get(n)?).map_err(message))
        .collect::<WitnessResult<Vec<_>>>()?;
    let vote_bundles = m
        .bundles
        .iter()
        .map(|n| EvidenceBundle::decode(get(n)?).map_err(message))
        .collect::<WitnessResult<Vec<_>>>()?;
    let request = m.request.request();
    request.validate().map_err(message)?;
    require(
        hash(get(&m.parameters)?) == request.request_params_hash
            && hash(get(&m.policy)?) == request.policy_hash,
        "artifact hash mismatch",
    )?;
    let auth = &authorization.body;
    require(
        auth.context == request.context
            && auth.operation_id == request.operation_id
            && auth.effect == request.effect(auth.effect.target).map_err(message)?,
        "signed request mismatch",
    )?;
    Ok(Packet {
        evidence: CheckpointedEvidenceV0 {
            inception,
            log,
            authorization,
            vote_bundles,
        },
        request,
        policy: get(&m.policy)?.to_vec(),
    })
}

// This role accepts raw public evidence only, then derives authority using its own trust and
// rules. reserve_checkpointed preflights again and atomically consumes/reserves; never admit.
fn custodian(root: &Path, trust: &Trust, packet: &Packet, role: &str) -> WitnessResult<String> {
    let now = if role == "custodian-late" {
        NOT_AFTER + 1
    } else {
        NOW
    };
    // Report independent preflight refusals before opening the store.
    match preflight_checkpointed(
        &packet.evidence,
        &trust.frontier(),
        &rules(),
        trust.target,
        now,
    ) {
        Ok(_) => (),
        Err(AdmissionRefusal::Expired { .. }) if role == "custodian-late" => (),
        Err(e) => return Err(format!("authority-refused: {e:?}")),
    }
    require(packet.policy == POLICY, "local-policy-refused")?;
    let outcome = if role == "custodian-uncertain" {
        let store = LocalResourceReservations::with_barrier(
            SledStore::open(root.join("store")).map_err(message)?,
            trust.target,
            FailingBarrier,
        );
        let result = store.reserve_checkpointed(
            &packet.evidence,
            &trust.frontier(),
            &rules(),
            now,
            &packet.request,
        );
        require(
            matches!(result, Err(ReservationError::CommitUncertain(_))),
            "expected uncertain durability",
        )?;
        require(
            matches!(
                store
                    .reconcile(packet.request.context, packet.request.operation_id)
                    .map_err(message)?,
                ReservationReconciliation::Committed { .. }
            ),
            "expected visible local commit",
        )?;
        // Visibility while sync fails does not acknowledge durability; never retry allocation.
        return Ok("uncertain; visible-local-commit; durability-unacknowledged".into());
    } else {
        let store =
            LocalResourceReservations::open(root.join("store"), trust.target).map_err(message)?;
        store.reserve_checkpointed(
            &packet.evidence,
            &trust.frontier(),
            &rules(),
            now,
            &packet.request,
        )
    };
    match outcome {
        Ok(record) => {
            require(
                record.quantity == 4
                    && record.available_before == 10
                    && record.available_after == 6
                    && record.pool_revision_before == 0
                    && record.pool_revision_after == 1,
                "unexpected reservation",
            )?;
            Ok("durable-confirmed".into())
        }
        Err(ReservationError::Authorization(AdmissionRefusal::Replayed)) => Ok("replayed".into()),
        Err(ReservationError::Authorization(AdmissionRefusal::Expired { .. }))
            if role == "custodian-late" =>
        {
            Ok("expired".into())
        }
        Err(e) => Err(format!("reservation-refused: {e:?}")),
    }
}

struct FailingBarrier;
impl DurabilityBarrier for FailingBarrier {
    fn sync(&self, _: &SledStore) -> Result<(), String> {
        Err("injected witness durability failure".into())
    }
}

// No signing API, pre-verified object, reservation retry, or application-state write here.
// Historical verification deliberately does not issue fresh permission at the audit clock.
fn audit(root: &Path, trust: &Trust, packet: &Packet, late: bool) -> WitnessResult<String> {
    let verified = verify_checkpointed_authorization(&packet.evidence, &trust.frontier(), &rules())
        .map_err(|e| format!("authority-refused: {e:?}"))?;
    let auth = verified.authorization();
    require(
        auth.verdict == Verdict::Pass && auth.effect.target == trust.target,
        "audit authority/target refused",
    )?;
    require(packet.policy == POLICY, "local-policy-refused")?;
    let expected = ConsumedEntry {
        authorization: ConsumedAuthorization::Checkpointed(verified.signed().clone()),
        writer: verified.writer(),
    };
    // At the original admission time the same public evidence produces the same consumed
    // entry; at the later audit time it is explicitly expired for any new admission.
    require(
        preflight_checkpointed(
            &packet.evidence,
            &trust.frontier(),
            &rules(),
            trust.target,
            NOW,
        )
        .map_err(message)?
        .consumed_entry()
            == expected,
        "historical consumption mismatch",
    )?;
    if late {
        require(
            matches!(
                preflight_checkpointed(
                    &packet.evidence,
                    &trust.frontier(),
                    &rules(),
                    trust.target,
                    NOT_AFTER + 1
                ),
                Err(AdmissionRefusal::Expired { .. })
            ),
            "late evidence must not grant new permission",
        )?;
    }
    let store =
        LocalResourceReservations::open(root.join("store"), trust.target).map_err(message)?;
    let ReservationReconciliation::Committed {
        consumed,
        reservation,
    } = store
        .reconcile(packet.request.context, packet.request.operation_id)
        .map_err(message)?
    else {
        return Err("audit incomplete: no committed reservation; do not resubmit".into());
    };
    require(
        *consumed == expected,
        "archive differs from consumed authorization/writer",
    )?;
    let reconstructed = ReservationRequestV0 {
        context: reservation.context,
        operation_id: reservation.operation_id,
        pool_id: reservation.pool_id,
        expected_pool_revision: reservation.pool_revision_before,
        quantity: reservation.quantity,
        unit: reservation.unit.clone(),
        request_params_hash: reservation.request_params_hash,
        policy_hash: reservation.policy_hash,
    };
    require(
        reconstructed == packet.request
            && reconstructed
                .effect(reservation.custodian)
                .map_err(message)?
                == auth.effect,
        "reservation differs from archived signed request/effect",
    )?;
    let pool = store
        .inspect_pool(packet.request.context, packet.request.pool_id)
        .map_err(message)?
        .ok_or("missing pool")?;
    require(
        reservation.quantity == 4
            && reservation.available_before == 10
            && reservation.available_after == 6
            && reservation.pool_revision_before == 0
            && reservation.pool_revision_after == 1
            && reservation.reserved_at == NOW
            && pool.total_capacity == 10
            && pool.available_capacity == 6
            && pool.revision == 1,
        "audit capacity/revision mismatch",
    )?;
    Ok(if late { "audit-time=1501; historical-only; " } else { "" }.to_owned()
        + "authorization-verified; reservation-observed; reserved=4; available=6; revision=1; fulfillment=unproven")
}

fn run_phase(root: &Path, trust: &Trust, role: &str) -> WitnessResult<String> {
    if role.starts_with("institution") {
        return institution(root, role);
    }
    let packet = load_packet(&root.join("packet")).map_err(|e| format!("packet-refused: {e}"))?;
    match role {
        "custodian" | "custodian-uncertain" | "custodian-late" => {
            custodian(root, trust, &packet, role)
        }
        "audit" | "audit-late" => audit(root, trust, &packet, role == "audit-late"),
        _ => Err("unknown witness role".into()),
    }
}

fn read_manifest(dir: &Path) -> Manifest {
    serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap()
}
fn write_manifest(dir: &Path, manifest: &Manifest) {
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec(manifest).unwrap(),
    )
    .unwrap();
}
fn edit_json(dir: &Path, edit: impl FnOnce(&mut serde_json::Value)) {
    let mut value = serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    edit(&mut value);
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
}

fn corrupt_packet(root: &Path, attack: &str) {
    let dir = root.join("packet");
    match attack {
        "missing" => fs::remove_file(dir.join("parameters.bin")).unwrap(),
        "manifest-large" => {
            fs::write(dir.join("manifest.json"), vec![b' '; MANIFEST_LIMIT + 1]).unwrap()
        }
        "binary-large" => fs::write(dir.join("parameters.bin"), vec![0; BINARY_LIMIT + 1]).unwrap(),
        "packet-large" => {
            let mut m = read_manifest(&dir);
            fs::remove_file(dir.join(&m.log[0])).unwrap();
            m.log = names("log", LIST_LIMIT);
            for name in &m.log {
                fs::write(dir.join(name), vec![0; BINARY_LIMIT]).unwrap();
            }
            write_manifest(&dir, &m);
        }
        "version" => edit_json(&dir, |m| m["version"] = 2.into()),
        "unknown" => edit_json(&dir, |m| m["surprise"] = true.into()),
        "request-unknown" => edit_json(&dir, |m| m["request"]["surprise"] = true.into()),
        "missing-field" => edit_json(&dir, |m| {
            m.as_object_mut().unwrap().remove("request");
        }),
        "duplicate-field" => {
            let bytes = fs::read_to_string(dir.join("manifest.json")).unwrap();
            fs::write(
                dir.join("manifest.json"),
                bytes.replacen('{', "{\"version\":1,", 1),
            )
            .unwrap();
        }
        "manifest-trailing" => OpenOptions::new()
            .append(true)
            .open(dir.join("manifest.json"))
            .unwrap()
            .write_all(b"{}")
            .unwrap(),
        "logs-large" | "bundles-large" => {
            let mut m = read_manifest(&dir);
            if attack == "logs-large" {
                m.log = names("log", LIST_LIMIT + 1);
            } else {
                m.bundles = names("bundle", LIST_LIMIT + 1);
            }
            write_manifest(&dir, &m);
        }
        "path" | "absolute-path" | "duplicate-name" | "order" => {
            let mut m = read_manifest(&dir);
            match attack {
                "path" => m.parameters = "../parameters.bin".into(),
                "absolute-path" => {
                    m.parameters = root.join("parameters.bin").to_str().unwrap().into()
                }
                "duplicate-name" => m.bundles[1] = m.bundles[0].clone(),
                "order" => m.bundles.swap(0, 1),
                _ => unreachable!(),
            }
            write_manifest(&dir, &m);
        }
        "symlink" => {
            fs::rename(dir.join("parameters.bin"), root.join("parameters.bin")).unwrap();
            std::os::unix::fs::symlink(root.join("parameters.bin"), dir.join("parameters.bin"))
                .unwrap();
        }
        "directory-symlink" => {
            fs::rename(&dir, root.join("original-packet")).unwrap();
            std::os::unix::fs::symlink(root.join("original-packet"), &dir).unwrap();
        }
        "special-file" => {
            fs::remove_file(dir.join("parameters.bin")).unwrap();
            fs::create_dir(dir.join("parameters.bin")).unwrap();
        }
        "extra-file" => fs::write(dir.join("trust.json"), b"{}").unwrap(),
        "trailing-inception" | "trailing-authorization" | "trailing-log" | "trailing-bundle" => {
            let name = match attack {
                "trailing-inception" => "inception.bin",
                "trailing-authorization" => "authorization.bin",
                "trailing-log" => "log-00.bin",
                _ => "bundle-00.bin",
            };
            OpenOptions::new()
                .append(true)
                .open(dir.join(name))
                .unwrap()
                .write_all(&[0])
                .unwrap();
        }
        _ => unreachable!(),
    }
}
