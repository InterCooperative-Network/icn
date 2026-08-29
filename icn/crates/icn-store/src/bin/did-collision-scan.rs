//! Read-only pre-migration collision scan for `Did`-keyed persisted rows.
//!
//! Runs the [`icn_store::did_collision_scan`] engine against one or more
//! on-disk stores and prints an aggregate report. This is the reproducible
//! procedure the N2-A0 inventory (§12.1 item 3) makes mandatory before `Did`
//! equality becomes key equality (N2-A, #2627).
//!
//! # Why it copies before reading
//!
//! `sled::open` is not a read-only operation. It takes an exclusive file lock,
//! and on an unclean directory it runs recovery, which *writes*. Pointing it at
//! a live deployment store would therefore both fail (the daemon holds the lock)
//! and violate the guarantee this tool exists to make.
//!
//! So the scan never opens the store it was given. It copies the directory to a
//! scratch location, opens the **copy**, scans it, and removes it. The source is
//! only ever read byte-for-byte. That also means the tool can be pointed at a
//! backup or a `kubectl cp` of a running pod's volume with the same semantics.
//!
//! # Usage
//!
//! ```text
//! did-collision-scan <store-path> [<store-path> ...] [--json]
//! ```
//!
//! Exit status is `0` when every scanned keyspace can be migrated without a
//! human deciding an outcome, and `1` when at least one must fail closed. That
//! makes the tool usable as the migration gate itself, not merely as a report.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use icn_store::did_collision_scan::{
    audit_store, n2a_deferred_namespaces, n2a_keyspaces, CoverageAudit,
};
use icn_store::{SledStore, Store};

/// One store's audit plus the per-tree coverage facts that produced it.
///
/// The verdict is **not** recomputed here. `CoverageAudit::is_clear` is the
/// gate, and this binary renders it — a runner that decided separately what its
/// own report meant is how an exit status comes to disagree with the text above
/// it.
struct ScanOutcome {
    audit: CoverageAudit,
    /// Row count per sled tree, including the default tree.
    trees: Vec<(String, usize)>,
    /// Rows per tree whose key embeds a `did:icn:` spelling.
    did_rows: Vec<(String, usize)>,
}

impl ScanOutcome {
    fn is_clear(&self) -> bool {
        self.audit.is_clear()
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!(
                "\nGATE: at least one keyspace must fail closed. \
                 Do not start a key-equality binary against these stores."
            );
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("did-collision-scan: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let paths: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();

    if paths.is_empty() {
        anyhow::bail!("usage: did-collision-scan <store-path> [<store-path> ...] [--json]");
    }

    let descriptors = n2a_keyspaces();
    let mut all_clear = true;

    for path in &paths {
        if !path.exists() {
            anyhow::bail!("store path does not exist: {}", path.display());
        }

        let outcome = scan_copy_of(path, &descriptors)
            .with_context(|| format!("scanning {}", path.display()))?;

        all_clear &= outcome.is_clear();

        if json {
            print_json(path, &outcome);
        } else {
            print_human(path, &outcome);
        }
    }

    Ok(all_clear)
}

/// Copy the store to scratch, scan the copy, remove it. The source is never
/// opened.
fn scan_copy_of(
    source: &Path,
    descriptors: &[icn_store::did_collision_scan::KeyspaceDescriptor],
) -> Result<ScanOutcome> {
    let scratch = tempdir()?;
    let working = scratch.join("store");

    copy_dir(source, &working)
        .with_context(|| format!("copying {} to scratch", source.display()))?;

    let result = (|| -> Result<ScanOutcome> {
        let store = SledStore::open(&working)
            .with_context(|| format!("opening copy of {}", source.display()))?;
        // `Store::scan` reads only sled's default tree. Read every tree so a
        // zero result cannot be a named tree the scan never looked in — and so
        // an unreadable row becomes an error rather than an absence.
        let trees = store.tree_row_counts()?;
        let did_rows = store.did_bearing_rows_per_tree()?;
        let unreachable: usize = did_rows
            .iter()
            .filter(|(name, _)| name != "__sled__default")
            .map(|(_, n)| *n)
            .sum();

        let deferrals = n2a_deferred_namespaces();
        let audit = audit_store(&store as &dyn Store, descriptors, &deferrals, unreachable)?;

        Ok(ScanOutcome {
            audit,
            trees,
            did_rows,
        })
    })();

    // Best-effort cleanup; a failure to remove scratch must not mask a result.
    let _ = std::fs::remove_dir_all(&scratch);

    result
}

fn tempdir() -> Result<PathBuf> {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = base.join(format!("icn-did-scan-{}-{}", std::process::id(), ts));
    std::fs::create_dir_all(&dir).context("creating scratch directory")?;
    Ok(dir)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
        // Symlinks and devices are skipped: a sled directory holds neither, and
        // following one would read outside the store we were asked to scan.
    }
    Ok(())
}

fn print_human(path: &Path, outcome: &ScanOutcome) {
    let ScanOutcome {
        audit,
        trees,
        did_rows,
    } = outcome;
    let CoverageAudit {
        report,
        overview,
        deferred,
        uncovered,
        unreachable_did_rows,
    } = audit;
    println!("\n=== {} ===", path.display());
    // Printed first, and always: it is what makes a row of zeros below mean
    // "this store holds none of these rows" rather than "the scan read nothing".
    println!(
        "  store: {} rows total, {} with an embedded did:icn: spelling",
        overview.total_rows, overview.rows_with_embedded_did
    );
    if !overview.namespaces.is_empty() {
        let listed: Vec<String> = overview
            .namespaces
            .iter()
            .map(|(ns, n)| format!("{ns}={n}"))
            .collect();
        println!("  namespaces: {}", listed.join(" "));
    }
    let tree_line: Vec<String> = trees
        .iter()
        .map(|(name, n)| {
            let did = did_rows
                .iter()
                .find(|(t, _)| t == name)
                .map(|(_, d)| *d)
                .unwrap_or(0);
            format!("{name}={n}(did:{did})")
        })
        .collect();
    println!("  trees: {}", tree_line.join(" "));
    let unreachable = *unreachable_did_rows;
    if unreachable > 0 {
        println!(
            "  WARNING: {unreachable} principal-keyed row(s) live in a named tree that \
             Store::scan cannot reach - this store is NOT fully scanned"
        );
    }

    println!(
        "{:<32} {:>7} {:>7} {:>7} {:>7} {:>7}  disposition",
        "keyspace", "rows", "princ", "groups", "inColl", "unread"
    );

    for k in &report.keyspaces {
        println!(
            "{:<32} {:>7} {:>7} {:>7} {:>7} {:>7}  {}{}",
            k.keyspace,
            k.rows_scanned,
            k.distinct_principals,
            k.collision_groups.len(),
            k.rows_in_collisions(),
            k.rows_unreadable,
            k.disposition.label(),
            if k.must_fail_closed() {
                "  <-- MUST FAIL CLOSED"
            } else {
                ""
            }
        );

        for group in &k.collision_groups {
            let reps: Vec<String> = group
                .representation_counts
                .iter()
                .map(|c| c.to_string())
                .collect();
            println!(
                "    principal {} : {} rows, {} representation(s); survivor at scan ordinal {}",
                group.principal_fingerprints.join("+"),
                group.rows.len(),
                reps.join("+"),
                group
                    .last_writer_survivor()
                    .map(|r| r.scan_ordinal.to_string())
                    .unwrap_or_else(|| "-".into()),
            );
        }
    }

    if !deferred.is_empty() {
        let listed: Vec<String> = deferred
            .iter()
            .map(|(name, n)| format!("{name}={n}"))
            .collect();
        println!(
            "  deferred: {} (behind a named gate; not scanned, not cleared)",
            listed.join(" ")
        );
    }
    if !uncovered.is_empty() {
        println!(
            "  UNCOVERED: {} principal-bearing row(s) under no registered keyspace and no \
             named gate - this store is NOT cleared:",
            audit.uncovered_did_rows()
        );
        for (shape, n) in uncovered {
            println!("    {n:>5}  {shape}");
        }
    }

    println!(
        "\n  totals: {} rows, {} collision groups, {} rows in collisions, {} unreadable",
        report.total_rows_scanned(),
        report.total_collision_groups(),
        report.total_rows_in_collisions(),
        report.total_rows_unreadable(),
    );
    println!(
        "  verdict: {}",
        if outcome.is_clear() {
            "CLEAR - no keyspace requires manual disposition"
        } else {
            "BLOCKED - manual disposition required"
        }
    );
}

fn print_json(path: &Path, outcome: &ScanOutcome) {
    // Built field by field through a real encoder rather than formatted by
    // hand. Hand-formatting produced invalid JSON for any path containing a
    // quote, a backslash or a newline, and the fix is not more escaping: it is
    // not writing the escaping. Fields are still listed explicitly instead of
    // deriving `Serialize` on the report types, so a field added to a report
    // later cannot reach this output — and therefore cannot leak a stored
    // value — without someone editing this function.
    let keyspaces: Vec<serde_json::Value> = outcome
        .audit
        .report
        .keyspaces
        .iter()
        .map(|k| {
            serde_json::json!({
                "keyspace": k.keyspace,
                "inventory_rows": k.inventory_rows,
                "disposition": k.disposition.label(),
                "rows_scanned": k.rows_scanned,
                "distinct_principals": k.distinct_principals,
                "collision_groups": k.collision_groups.len(),
                "rows_in_collisions": k.rows_in_collisions(),
                "rows_unreadable": k.rows_unreadable,
                "rows_without_did": k.rows_without_did,
                "must_fail_closed": k.must_fail_closed(),
            })
        })
        .collect();

    let deferred: Vec<serde_json::Value> = outcome
        .audit
        .deferred
        .iter()
        .map(|(name, n)| serde_json::json!({ "namespace": name, "did_bearing_rows": n }))
        .collect();

    // Masked shapes only: `did:icn:…` is already `<did>` and non-printables are
    // `.`, so this carries key structure and no identifier or payload.
    let uncovered: Vec<serde_json::Value> = outcome
        .audit
        .uncovered
        .iter()
        .map(|(shape, n)| serde_json::json!({ "shape": shape, "rows": n }))
        .collect();

    let doc = serde_json::json!({
        "store": path.display().to_string(),
        "clear": outcome.is_clear(),
        "store_total_rows": outcome.audit.overview.total_rows,
        "store_rows_with_did": outcome.audit.overview.rows_with_embedded_did,
        "unreachable_did_rows": outcome.audit.unreachable_did_rows,
        "uncovered_did_rows": outcome.audit.uncovered_did_rows(),
        "deferred_namespaces": deferred,
        "uncovered_shapes": uncovered,
        "keyspaces": keyspaces,
    });

    println!("{doc}");
}
