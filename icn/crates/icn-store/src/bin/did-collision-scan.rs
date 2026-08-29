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
//! # What a CLEAR verdict is conditional on
//!
//! A recursive file copy of a **live** store is not a point-in-time snapshot.
//! Writes can land between one file being copied and the next, so sled may
//! recover the copy successfully while omitting a row that existed in the
//! source — including an aliasing row. A CLEAR verdict therefore describes a
//! state the source may never have held at any single instant.
//!
//! That is a limit of the evidence, not a bug to code around: quiescing a store
//! means stopping a workload, which this tool must not do. It is stated here,
//! printed with every report, and it is why the migration design puts the
//! fail-closed check *inside* the key-equality binary rather than trusting a
//! scan run earlier. For a verdict that is binding rather than indicative, scan
//! a quiesced store or a coherent volume snapshot, with writes held until the
//! flip.
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
    // `args_os`, not `args`: a store path on unix may hold bytes that are not
    // UTF-8, and `std::env::args()` panics while decoding one — exiting 101
    // past this function's `Result` handling, with no gate report at all.
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let paths: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.as_encoded_bytes().starts_with(b"--"))
        .map(PathBuf::from)
        .collect();

    if paths.is_empty() {
        anyhow::bail!("usage: did-collision-scan <store-path> [<store-path> ...] [--json]");
    }

    let descriptors = n2a_keyspaces();
    let mut all_clear = true;
    let mut documents = Vec::with_capacity(paths.len());

    for path in &paths {
        if !path.exists() {
            anyhow::bail!("store path does not exist: {}", path.display());
        }
        ensure_sled_root(path)?;

        let outcome = scan_copy_of(path, &descriptors)
            .with_context(|| format!("scanning {}", path.display()))?;

        all_clear &= outcome.is_clear();

        if json {
            documents.push(json_document(path, &outcome));
        } else {
            print_human(path, &outcome);
        }
    }

    // One document for the whole run, not one per store. Printing a top-level
    // object per path leaves stdout that is not valid JSON as a whole, so `jq`
    // and `serde_json::from_str` fail on the documented multi-store form even
    // though each line parses on its own.
    if json {
        let doc = serde_json::json!({
            "clear": all_clear,
            "quiescence": "A copy of a live store is not a point-in-time snapshot; \
                           a clear verdict is conditional on the source being quiesced.",
            "stores": documents,
        });
        println!("{doc}");
    }

    Ok(all_clear)
}

/// Refuse a path that is not itself a sled database root.
///
/// `sled::open` on a directory that is not a database *creates* one, so a path
/// one level too high — `/data` rather than `/data/store/ledger`, which is
/// exactly what the documented `kubectl cp` produces — yields a freshly
/// initialised empty database in the scratch copy. The scan would then report
/// zero rows and exit CLEAR while never having looked at the real stores
/// underneath. An operator's wrong path must not become a passing gate.
///
/// Nested databases are discovered and named rather than merely rejected, so
/// the error tells the caller what to run instead.
fn ensure_sled_root(path: &Path) -> Result<()> {
    if !path.is_dir() {
        anyhow::bail!("not a directory: {}", path.display());
    }
    // `conf` is written by sled when a database is created and is present for
    // every database, empty or not.
    if path.join("conf").is_file() {
        return Ok(());
    }

    let mut nested = Vec::new();
    find_sled_roots(path, 0, &mut nested);

    if nested.is_empty() {
        anyhow::bail!(
            "{} is not a sled database (no `conf`), and no database was found beneath it. \
             Point the scan at a store directory.",
            path.display()
        );
    }

    let listed: Vec<String> = nested.iter().map(|p| p.display().to_string()).collect();
    anyhow::bail!(
        "{} is not a sled database itself, but contains {}. Opening it would create an \
         empty database and report a false CLEAR. Scan these instead:\n  {}",
        path.display(),
        if nested.len() == 1 {
            "one".to_string()
        } else {
            format!("{} databases", nested.len())
        },
        listed.join("\n  ")
    );
}

/// Collect sled database roots beneath `dir`, bounded in depth.
fn find_sled_roots(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    const MAX_DEPTH: usize = 4;
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        if !child.is_dir() {
            continue;
        }
        if child.join("conf").is_file() {
            out.push(child);
        } else {
            find_sled_roots(&child, depth + 1, out);
        }
    }
}

/// Copy the store to scratch, scan the copy, remove it. The source is never
/// opened.
fn scan_copy_of(
    source: &Path,
    descriptors: &[icn_store::did_collision_scan::KeyspaceDescriptor],
) -> Result<ScanOutcome> {
    let scratch = tempdir()?;
    let working = scratch.join("store");

    // The copy is inside the guarded closure too. Returning `?` straight out of
    // it used to skip the cleanup below, so a copy that failed partway — on a
    // symlink, a permission error, a full disk — left stored payloads in `/tmp`
    // indefinitely, and each retry added another partial copy.
    let result = (|| -> Result<ScanOutcome> {
        copy_dir(source, &working)
            .with_context(|| format!("copying {} to scratch", source.display()))?;

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

    // Created 0700 rather than at the caller's umask. The scratch copy holds
    // complete stored payloads — for a deployment volume that includes keystore
    // material — and cleanup is best-effort, so a `022` umask on a shared host
    // would leave that readable to every local user.
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .context("creating scratch directory")?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(&dir).context("creating scratch directory")?;

    Ok(dir)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        // `file_type` on a `DirEntry` does not follow links, so this sees the
        // link itself rather than what it points at.
        let kind = entry.file_type()?;

        if kind.is_symlink() {
            // Skipping was wrong. A source represented as a symlink farm — a
            // backup, a restored snapshot — passes the `conf` check because
            // that check follows links, and then copies to an empty directory.
            // `SledStore::open` would initialise a fresh database there and the
            // gate would report CLEAR having scanned nothing. Refusing is the
            // only safe reading: following the link could also read outside the
            // store we were asked to scan.
            anyhow::bail!(
                "refusing to scan {}: it contains a symlink ({}). Resolve the store to real \
                 files first — copying past a link would scan an empty database and report a \
                 false CLEAR.",
                from.display(),
                entry.file_name().to_string_lossy()
            );
        }

        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
        } else {
            anyhow::bail!(
                "refusing to scan {}: unexpected non-regular entry ({})",
                from.display(),
                entry.file_name().to_string_lossy()
            );
        }
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
        "  note: a copy of a live store is not a point-in-time snapshot; a CLEAR \
         verdict is conditional on the source being quiesced"
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

fn json_document(path: &Path, outcome: &ScanOutcome) -> serde_json::Value {
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

    // `display()` is lossy, so two distinct non-UTF-8 paths — both legal on
    // unix, and both scannable since argv is parsed as `OsString` — would
    // serialize to the same string and a multi-store report could not say which
    // verdict belonged to which source. The lossy rendering stays because it is
    // what a human reads; when it is not faithful, the raw bytes travel beside
    // it so a consumer can still tell the two apart.
    let lossy = path.display().to_string();
    let exact_bytes: Option<Vec<u8>> = {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;
            let raw = path.as_os_str().as_bytes();
            (path.to_str().is_none()).then(|| raw.to_vec())
        }
        #[cfg(not(unix))]
        {
            None
        }
    };

    serde_json::json!({
        "store": lossy,
        "store_path_bytes": exact_bytes,
        "clear": outcome.is_clear(),
        "store_total_rows": outcome.audit.overview.total_rows,
        "store_rows_with_did": outcome.audit.overview.rows_with_embedded_did,
        "unreachable_did_rows": outcome.audit.unreachable_did_rows,
        "uncovered_did_rows": outcome.audit.uncovered_did_rows(),
        "deferred_namespaces": deferred,
        "uncovered_shapes": uncovered,
        "keyspaces": keyspaces,
    })
}
