//! N2-A startup gate: a key-equality binary refuses to open stores it cannot
//! open safely (#2627).
//!
//! # Why this exists
//!
//! `Did` equality and hashing name the principal, not the spelling (I7,
//! #2686). That changed no persisted byte — but it changed what happens on the
//! next start of any binary carrying it: every `Did`-keyed rebuild now folds
//! alias-spelled rows of one principal into one entry, and the write-back that
//! follows orphans the losers. The N2-A0 inventory (§12.1 item 7) and the
//! migration-gate record (§3.5) therefore require the same thing: the
//! fail-closed check must live **inside the binary**, run **at every start**,
//! and refuse rather than trust a scan run earlier. Aliasing is attacker-chosen
//! and `from` is unsigned, so a clean scan yesterday says nothing about today.
//!
//! # What it does
//!
//! [`enforce`] runs once per process, before the first store is opened:
//!
//! 1. reads the prior receipt, if any, and refuses a receipt it cannot read or
//!    one written by a **newer** principal-identity generation than this
//!    binary understands;
//! 2. finds every sled database beneath the data directory — the ones the
//!    deployment actually keeps, not the ones a list expected;
//! 3. opens each in turn, audits it through the one shared
//!    [`audit_sled_store`] computation, and closes it so the daemon can open it
//!    for real;
//! 4. writes a payload-free receipt atomically once a verdict exists — that is,
//!    for `clear` and for `refused`, but **not** for the refusals that occur
//!    before any store is audited (an unreadable or newer-generation receipt, a
//!    missing data directory, an incomplete discovery sweep, an unverifiable
//!    store). Those return without recording anything, because there is no
//!    verdict yet to record and overwriting a prior receipt would destroy the
//!    last one that meant something;
//! 5. returns the receipt on `clear`, and a [`GateRefusal`] otherwise.
//!
//! # What it deliberately does not do
//!
//! * **It never writes to a domain store.** The only mutation that occurs while
//!   it runs is sled's own recovery on open, which the daemon would perform
//!   moments later anyway. Its one write is its own receipt, in the data
//!   directory, outside every database.
//! * **The receipt is a record, not a skip token.** The audit runs at every
//!   start regardless of what the receipt says. The receipt exists so an
//!   operator can see what was inspected, what was rejected and when, and so a
//!   future generation has a boundary this one can detect.
//! * **There is no bypass.** A refusal names the store, the keyspace, the rule
//!   status and the principal fingerprints. Disposition is manual and belongs
//!   to the domain that owns the keyspace; `did-collision-scan` gives the full
//!   report.
//! * **It migrates, merges and re-keys nothing.** Membership and vote rows stay
//!   behind IDENTITY_SEMANTICS §7.5; every other merge rule stays where the
//!   scanner registry says it is.
//!
//! # Generations
//!
//! [`PRINCIPAL_IDENTITY_GENERATION`] is the persisted-semantics boundary a
//! binary can detect. Generation 1 is I7: principal-byte `Did` equality,
//! spelling-preserving persistence, no row re-keyed. A receipt carrying a
//! higher number was written by a binary that may have rewritten rows in a
//! form this one does not understand, and this one refuses to open the data
//! directory rather than guess. A receipt carrying a lower number, or no
//! receipt at all, simply means the audit has not been recorded under this
//! generation yet — the audit runs either way.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::did_collision_scan::{
    audit_sled_store, find_sled_roots, DeferredCollisionPosture, SledStoreAudit,
};
use crate::SledStore;

/// The principal-identity generation this binary implements and records.
///
/// * **1** — I7 (#2686): `Did` equality and hashing over the decoded 32
///   identifier bytes; `Display`/`as_str`/`Serialize` unchanged; no persisted
///   row re-keyed; membership and vote keyspaces untouched (§7.5).
pub const PRINCIPAL_IDENTITY_GENERATION: u32 = 1;

/// Schema identifier written into every receipt.
pub const RECEIPT_SCHEMA: &str = "icn/n2a-startup-gate/v1";

/// Name of the receipt file, relative to the data directory.
pub const RECEIPT_FILE_NAME: &str = "n2a-startup-gate.json";

/// Path of the receipt for a data directory.
pub fn receipt_path(data_dir: &Path) -> PathBuf {
    data_dir.join(RECEIPT_FILE_NAME)
}

/// The gate's verdict on a store, or on the whole data directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Every principal-bearing row is accounted for and nothing must fail
    /// closed. The binary may open the store.
    Clear,
    /// At least one condition forbids opening the store under key-equality
    /// `Did`. The blockers say which.
    Refused,
}

/// What the gate inspected and decided, for the whole data directory.
///
/// Payload-free by construction: every field is derived from a
/// [`crate::did_collision_scan::CoverageAudit`], which never carries a stored
/// value, and principals appear only as truncated fingerprints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateReceipt {
    /// Always [`RECEIPT_SCHEMA`].
    pub schema: String,
    /// The generation that produced this receipt — [`PRINCIPAL_IDENTITY_GENERATION`].
    pub generation: u32,
    /// Seconds since the Unix epoch when the audit ran.
    pub verified_at_unix: u64,
    /// The `icn-store` crate version that ran the audit.
    pub icn_store_version: String,
    /// The data directory swept, lossily rendered.
    pub data_dir: String,
    /// `clear` only when every store is clear.
    pub verdict: Verdict,
    /// One entry per sled database found, in path order.
    pub stores: Vec<StoreReceipt>,
}

impl GateReceipt {
    /// Stores that refused.
    pub fn refused_stores(&self) -> impl Iterator<Item = &StoreReceipt> {
        self.stores.iter().filter(|s| s.verdict == Verdict::Refused)
    }

    /// A multi-line, payload-free operator summary of every blocker.
    pub fn refusal_summary(&self) -> String {
        let mut out = String::new();
        for store in self.refused_stores() {
            for blocker in &store.blocking {
                out.push_str("  ");
                out.push_str(&store.path);
                out.push_str(": ");
                out.push_str(&blocker.describe());
                out.push('\n');
            }
        }
        out
    }
}

/// One sled database's inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreReceipt {
    /// The database root, lossily rendered.
    pub path: String,
    pub verdict: Verdict,
    /// Every row in the default tree, under any prefix.
    pub total_rows: usize,
    /// Rows in the default tree whose key embeds a `did:icn:` spelling.
    pub rows_with_embedded_did: usize,
    /// Row count per sled tree, and how many of each embed a spelling.
    pub trees: Vec<TreeReceipt>,
    /// Every registered keyspace's result, whether or not it blocked.
    pub keyspaces: Vec<KeyspaceReceipt>,
    /// Every deferred namespace's result, whether or not it blocked.
    pub deferred: Vec<DeferredReceipt>,
    /// Principal-bearing rows under no registered keyspace and no named gate,
    /// by masked key shape.
    pub uncovered_shapes: BTreeMap<String, usize>,
    /// Principal-bearing rows in named trees the scan cannot reach.
    pub unreachable_rows: usize,
    /// Everything that made this store refuse. Empty when clear.
    pub blocking: Vec<Blocker>,
}

/// One sled tree's row counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeReceipt {
    pub name: String,
    pub rows: usize,
    pub rows_with_embedded_did: usize,
}

/// One registered keyspace's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyspaceReceipt {
    pub keyspace: String,
    pub disposition: String,
    pub basis: String,
    pub rows_scanned: usize,
    pub distinct_principals: usize,
    pub collision_groups: usize,
    pub rows_in_collisions: usize,
    pub rows_unreadable: usize,
    pub must_fail_closed: bool,
}

/// One deferred namespace's result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeferredReceipt {
    pub namespace: String,
    pub gate: String,
    pub posture: String,
    pub did_bearing_rows: usize,
    pub collision_groups: usize,
    pub rows_in_collisions: usize,
    pub rows_unreadable: usize,
    pub blocks: bool,
}

/// One reason a store refused. Each carries enough to act on and nothing that
/// reconstructs a principal or a payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Blocker {
    /// A registered keyspace holds rows that cannot be merged automatically:
    /// a collision under a rule nobody has authorized, or a row whose
    /// principal cannot be read.
    Keyspace {
        keyspace: String,
        disposition: String,
        basis: String,
        collision_groups: usize,
        rows_in_collisions: usize,
        rows_unreadable: usize,
        /// Truncated fingerprints of the colliding principals, one entry per
        /// group, positions joined with `+` for tuple keys.
        principals: Vec<String>,
    },
    /// A deferred namespace with a `BlockStartup` posture holds a collision or
    /// an unreadable row.
    Deferred {
        namespace: String,
        gate: String,
        collision_groups: usize,
        rows_in_collisions: usize,
        rows_unreadable: usize,
        principals: Vec<String>,
    },
    /// Principal-bearing rows under a prefix no registry names. The shape is
    /// masked: spellings are `<did>`, non-printables are `.`.
    Uncovered { shape: String, rows: usize },
    /// Principal-bearing rows in a named tree the scan cannot examine.
    Unreachable { rows: usize },
}

impl Blocker {
    /// One line an operator can act on.
    pub fn describe(&self) -> String {
        match self {
            Blocker::Keyspace {
                keyspace,
                disposition,
                basis,
                collision_groups,
                rows_in_collisions,
                rows_unreadable,
                principals,
            } => format!(
                "keyspace {keyspace} ({disposition}, {basis}): {collision_groups} collision \
                 group(s) over {rows_in_collisions} row(s), {rows_unreadable} unreadable; \
                 principals [{}]",
                principals.join(", ")
            ),
            Blocker::Deferred {
                namespace,
                gate,
                collision_groups,
                rows_in_collisions,
                rows_unreadable,
                principals,
            } => format!(
                "deferred namespace {namespace} (owned by: {gate}): {collision_groups} \
                 collision group(s) over {rows_in_collisions} row(s), {rows_unreadable} \
                 unreadable; the loader would merge these silently; principals [{}]",
                principals.join(", ")
            ),
            Blocker::Uncovered { shape, rows } => format!(
                "UNCOVERED: {rows} principal-bearing row(s) under no registered keyspace and \
                 no named gate, shape `{shape}` — register the keyspace or defer it explicitly"
            ),
            Blocker::Unreachable { rows } => format!(
                "UNREACHABLE: {rows} principal-bearing row(s) live in a named tree the scan \
                 cannot examine"
            ),
        }
    }
}

/// Why the gate refused to let the binary start.
#[derive(Debug, thiserror::Error)]
pub enum GateRefusal {
    /// The data directory does not exist or is not a directory. The gate does
    /// not create it: the caller owns the layout.
    #[error("N2-A startup gate: data directory {0} is not a directory")]
    DataDirMissing(PathBuf),

    /// The sweep for sled databases could not be completed, so the set of
    /// stores to audit is unknown. Refusing is the only safe reading: an
    /// omitted database is indistinguishable from a clean one, and a CLEAR
    /// receipt written over a partial sweep would authorize exactly the lossy
    /// merge this gate exists to prevent.
    #[error(
        "N2-A startup gate: could not enumerate the sled databases beneath {data_dir} \
         ({reason}). Refusing to start rather than audit an unknown subset of the stores \
         this daemon will open."
    )]
    DiscoveryIncomplete { data_dir: PathBuf, reason: String },

    /// A receipt is present but cannot be read as one. Refusing is the only
    /// safe reading: a receipt this binary cannot parse may be one a newer
    /// generation wrote, and guessing would defeat the generation boundary.
    #[error(
        "N2-A startup gate: receipt at {receipt_path} is unreadable ({reason}). Refusing to \
         start rather than guess which generation wrote it. Inspect the file; if it is \
         corrupt and the data directory is known to be at generation {supported} or earlier, \
         remove it and restart to re-audit.",
        supported = PRINCIPAL_IDENTITY_GENERATION
    )]
    UnreadableReceipt {
        receipt_path: PathBuf,
        reason: String,
    },

    /// The receipt was written by a newer principal-identity generation.
    #[error(
        "N2-A startup gate: receipt at {receipt_path} was written by principal-identity \
         generation {found}, but this binary implements generation {supported}. The data \
         directory may hold rows re-keyed in a form this binary does not understand. \
         Refusing to start; use a binary at generation {found} or later."
    )]
    NewerGeneration {
        receipt_path: PathBuf,
        found: u32,
        supported: u32,
    },

    /// A store could not be opened or read completely, so nothing can be said
    /// about it — and a store nothing can be said about is not one to start
    /// over.
    #[error(
        "N2-A startup gate: store {store} could not be verified ({reason}). A store that \
         cannot be read completely is not one a key-equality binary may open; refusing to \
         start."
    )]
    StoreUnverifiable { store: PathBuf, reason: String },

    /// The receipt could not be written. The audit result is not lost — it is
    /// in this error — but a data directory the gate cannot record into is one
    /// the daemon could not operate either.
    #[error(
        "N2-A startup gate: could not write receipt to {receipt_path} ({reason}). Refusing \
         to start without a record of what was inspected."
    )]
    ReceiptUnwritable {
        receipt_path: PathBuf,
        reason: String,
    },

    /// At least one store must not be opened. The receipt names every reason.
    #[error(
        "N2-A startup gate REFUSED: {refused} of {total} store(s) under {data_dir} hold \
         principal-bearing rows a key-equality binary cannot open safely.\n{summary}\
         Receipt written to {receipt_path}. Run `did-collision-scan <store>` for the full \
         report. Disposition is manual and belongs to the domain that owns each keyspace; \
         there is no bypass flag (IDENTITY_SEMANTICS §11 I7; n2-a0-stored-key-inventory \
         §12.1 item 7).",
        refused = receipt.refused_stores().count(),
        total = receipt.stores.len(),
        data_dir = receipt.data_dir,
        summary = receipt.refusal_summary(),
        receipt_path = receipt_path.display()
    )]
    Blocked {
        /// Boxed: the receipt is the bulk of every refusal, and a `Result`
        /// whose `Err` carries it inline would make every clear path pay for
        /// the size of the refused one.
        receipt: Box<GateReceipt>,
        receipt_path: PathBuf,
    },
}

/// Run the gate over every sled database beneath `data_dir`.
///
/// Blocking I/O; call it from a blocking context. `now` is taken as a
/// parameter so the receipt's timestamp is the caller's to supply — nothing
/// in the verdict depends on it.
///
/// Returns the receipt when every store is clear. Every other outcome is a
/// [`GateRefusal`], and the caller must not open any store under `data_dir`
/// after one.
pub fn enforce(data_dir: &Path, now: SystemTime) -> Result<GateReceipt, GateRefusal> {
    if !data_dir.is_dir() {
        return Err(GateRefusal::DataDirMissing(data_dir.to_path_buf()));
    }

    let receipt_path = receipt_path(data_dir);
    check_prior_receipt(&receipt_path)?;

    // Discovery is fail-closed: a partial sweep must not be mistaken for a
    // complete one, because every store below is audited and a store that was
    // never found is indistinguishable from a store that was clean.
    let roots = find_sled_roots(data_dir).map_err(|e| GateRefusal::DiscoveryIncomplete {
        data_dir: data_dir.to_path_buf(),
        reason: format!("{e:#}"),
    })?;

    let mut stores = Vec::new();
    for root in roots {
        let audit = audit_one(&root)?;
        stores.push(store_receipt(&root, &audit));
    }

    let verdict = if stores.iter().all(|s| s.verdict == Verdict::Clear) {
        Verdict::Clear
    } else {
        Verdict::Refused
    };

    let receipt = GateReceipt {
        schema: RECEIPT_SCHEMA.to_string(),
        generation: PRINCIPAL_IDENTITY_GENERATION,
        verified_at_unix: now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        icn_store_version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: data_dir.display().to_string(),
        verdict,
        stores,
    };

    write_receipt_atomically(&receipt_path, &receipt)?;

    match verdict {
        Verdict::Clear => Ok(receipt),
        Verdict::Refused => Err(GateRefusal::Blocked {
            receipt: Box::new(receipt),
            receipt_path,
        }),
    }
}

/// Open one database, audit it, and close it before returning.
fn audit_one(root: &Path) -> Result<SledStoreAudit, GateRefusal> {
    let unverifiable = |reason: String| GateRefusal::StoreUnverifiable {
        store: root.to_path_buf(),
        reason,
    };

    // Scoped so the handle — and sled's exclusive lock — is released before the
    // caller opens the same database for real. A handle that outlived this
    // block would make the daemon's own open fail on a store the gate had just
    // cleared.
    let store = SledStore::open(root).map_err(|e| unverifiable(format!("open failed: {e:#}")))?;
    let audit = audit_sled_store(&store).map_err(|e| unverifiable(format!("audit failed: {e:#}")));
    drop(store);
    audit
}

/// The two receipt fields a prior generation check needs, and nothing else,
/// so a receipt written by a later schema with more fields still yields its
/// generation instead of failing to parse.
#[derive(Deserialize)]
struct PriorReceipt {
    schema: String,
    generation: u32,
}

/// Read the prior receipt, if any, and refuse what cannot be trusted.
fn check_prior_receipt(receipt_path: &Path) -> Result<(), GateRefusal> {
    let bytes = match std::fs::read(receipt_path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(GateRefusal::UnreadableReceipt {
                receipt_path: receipt_path.to_path_buf(),
                reason: format!("read failed: {e}"),
            })
        }
    };

    let prior: PriorReceipt =
        serde_json::from_slice(&bytes).map_err(|e| GateRefusal::UnreadableReceipt {
            receipt_path: receipt_path.to_path_buf(),
            reason: format!("not a receipt: {e}"),
        })?;

    if !prior.schema.starts_with("icn/n2a-startup-gate/") {
        return Err(GateRefusal::UnreadableReceipt {
            receipt_path: receipt_path.to_path_buf(),
            reason: format!("unexpected schema `{}`", prior.schema),
        });
    }

    if prior.generation > PRINCIPAL_IDENTITY_GENERATION {
        return Err(GateRefusal::NewerGeneration {
            receipt_path: receipt_path.to_path_buf(),
            found: prior.generation,
            supported: PRINCIPAL_IDENTITY_GENERATION,
        });
    }

    Ok(())
}

/// Write the receipt so that a reader sees either the previous receipt or the
/// new one, never a partial file: write to a sibling temporary, sync it, then
/// rename over the target.
fn write_receipt_atomically(receipt_path: &Path, receipt: &GateReceipt) -> Result<(), GateRefusal> {
    let unwritable = |reason: String| GateRefusal::ReceiptUnwritable {
        receipt_path: receipt_path.to_path_buf(),
        reason,
    };

    let bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|e| unwritable(format!("serialize failed: {e}")))?;

    let tmp_path = temporary_receipt_path(receipt_path);
    {
        use std::io::Write as _;
        let mut file = std::fs::File::create(&tmp_path)
            .map_err(|e| unwritable(format!("create {} failed: {e}", tmp_path.display())))?;
        file.write_all(&bytes)
            .map_err(|e| unwritable(format!("write failed: {e}")))?;
        file.sync_all()
            .map_err(|e| unwritable(format!("sync failed: {e}")))?;
    }
    std::fs::rename(&tmp_path, receipt_path)
        .map_err(|e| unwritable(format!("rename failed: {e}")))?;

    // Best effort: make the rename itself durable. A failure here leaves a
    // complete receipt on disk that a crash could roll back to the previous
    // one, which is a stale record rather than a wrong one.
    if let Some(dir) = receipt_path.parent() {
        if let Ok(dir) = std::fs::File::open(dir) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

/// The temporary path an in-progress receipt is written to. Never read as a
/// receipt: [`check_prior_receipt`] looks only at the final name, so an
/// interrupted write leaves nothing the next start could mistake for a record.
fn temporary_receipt_path(receipt_path: &Path) -> PathBuf {
    let mut name = receipt_path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".tmp");
    receipt_path.with_file_name(name)
}

/// Render one store's audit as its receipt.
fn store_receipt(root: &Path, audit: &SledStoreAudit) -> StoreReceipt {
    let coverage = &audit.audit;

    let trees = audit
        .trees
        .iter()
        .map(|(name, rows)| TreeReceipt {
            name: name.clone(),
            rows: *rows,
            rows_with_embedded_did: audit
                .did_rows
                .iter()
                .find(|(t, _)| t == name)
                .map(|(_, n)| *n)
                .unwrap_or(0),
        })
        .collect();

    let keyspaces = coverage
        .report
        .keyspaces
        .iter()
        .map(|k| KeyspaceReceipt {
            keyspace: k.keyspace.clone(),
            disposition: k.disposition.label().to_string(),
            basis: k.basis.label().to_string(),
            rows_scanned: k.rows_scanned,
            distinct_principals: k.distinct_principals,
            collision_groups: k.collision_groups.len(),
            rows_in_collisions: k.rows_in_collisions(),
            rows_unreadable: k.rows_unreadable,
            must_fail_closed: k.must_fail_closed(),
        })
        .collect();

    let deferred = coverage
        .deferred_reports
        .iter()
        .map(|d| DeferredReceipt {
            namespace: d.name.clone(),
            gate: d.gate.clone(),
            posture: d.posture.label().to_string(),
            did_bearing_rows: d.did_bearing_rows(),
            collision_groups: d.report.collision_groups.len(),
            rows_in_collisions: d.report.rows_in_collisions(),
            rows_unreadable: d.report.rows_unreadable,
            blocks: d.blocks(),
        })
        .collect();

    let mut blocking = Vec::new();

    for k in coverage.report.blocking_keyspaces() {
        blocking.push(Blocker::Keyspace {
            keyspace: k.keyspace.clone(),
            disposition: k.disposition.label().to_string(),
            basis: k.basis.label().to_string(),
            collision_groups: k.collision_groups.len(),
            rows_in_collisions: k.rows_in_collisions(),
            rows_unreadable: k.rows_unreadable,
            principals: k
                .collision_groups
                .iter()
                .map(|g| g.principal_fingerprints.join("+"))
                .collect(),
        });
    }

    for d in coverage.blocking_deferred() {
        debug_assert_eq!(d.posture, DeferredCollisionPosture::BlockStartup);
        blocking.push(Blocker::Deferred {
            namespace: d.name.clone(),
            gate: d.gate.clone(),
            collision_groups: d.report.collision_groups.len(),
            rows_in_collisions: d.report.rows_in_collisions(),
            rows_unreadable: d.report.rows_unreadable,
            principals: d
                .report
                .collision_groups
                .iter()
                .map(|g| g.principal_fingerprints.join("+"))
                .collect(),
        });
    }

    for (shape, rows) in &coverage.uncovered {
        blocking.push(Blocker::Uncovered {
            shape: shape.clone(),
            rows: *rows,
        });
    }

    if coverage.unreachable_did_rows > 0 {
        blocking.push(Blocker::Unreachable {
            rows: coverage.unreachable_did_rows,
        });
    }

    // The verdict is the library's, and the blocker list must agree with it in
    // both directions: a clear store lists nothing, and a refused store lists
    // at least one reason. Anything else is a renderer that has drifted from
    // the gate it renders.
    let verdict = if audit.is_clear() {
        Verdict::Clear
    } else {
        Verdict::Refused
    };
    debug_assert_eq!(verdict == Verdict::Refused, !blocking.is_empty());

    StoreReceipt {
        path: root.display().to_string(),
        verdict,
        total_rows: coverage.overview.total_rows,
        rows_with_embedded_did: coverage.overview.rows_with_embedded_did,
        trees,
        keyspaces,
        deferred,
        uncovered_shapes: coverage.uncovered.clone(),
        unreachable_rows: coverage.unreachable_did_rows,
        blocking,
    }
}
