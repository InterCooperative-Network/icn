//! The daemon's owner of N1 authority state (N1-R1, on ADR-0087).
//!
//! One value holds both halves of the durable layer so they cannot drift: the in-memory
//! [`AuthorityStore`] that [`derive`] reads, and the [`SledAuthorityFactStore`] that makes it
//! survive a restart. It adds three things the library boundary deliberately left to its caller:
//!
//! - **Where the store lives, and when it must exist.** `{data_dir}/store/authority/`
//!   ([`crate::config::authority_fact_store_path`]), plus an initialization record beside the
//!   stores ([`INIT_RECORD_FILE_NAME`]). Once a node has initialized its authority store, a
//!   missing or replaced store is a refusal, never "a fresh node".
//! - **Rehydration that fails closed.** [`AuthorityRegistry::open`] either returns the state
//!   [`rehydrate`] rebuilt from every record, or an error. There is no path that substitutes an
//!   empty [`AuthorityStore`].
//! - **The write order.** [`AuthorityRegistry::ingest`] admits, then persists through the
//!   acknowledged barrier, and only then publishes into the in-memory state, so the state a
//!   caller can read never holds a fact the medium did not acknowledge.
//!
//! # What the initialization record can and cannot detect
//!
//! The record and the store each carry the same random instance identifier: the record in its
//! JSON, the store in a row outside the fact namespace ([`INSTANCE_KEY`]). That detects:
//!
//! - the store directory removed while the record remains;
//! - the store replaced by a different or freshly-created database (its instance row is absent
//!   or different);
//! - the record removed while the store still holds facts.
//!
//! It cannot detect the record **and** the store disappearing together — for example the whole
//! data directory being replaced by an empty one. Nothing inside the data directory can tell that
//! from a node that never initialized; detecting it needs an anchor retained outside the
//! directory, which this slice does not have. Nor can it detect individual fact rows vanishing
//! from an intact store (ADR-0087 §9), or the store being rolled back to an earlier copy of
//! itself, which keeps its instance and has fewer facts.
//!
//! The refusals tell an operator to restore from a backup, and `icnctl restore` restores a whole
//! data directory. On a node whose store has never held a fact — every node today, since nothing
//! produces facts yet — `RecordMissing` cannot occur and a `StoreReplaced` or `StoreMissing`
//! refusal loses nothing if the record is removed by hand, after which the next start
//! re-initializes.
//!
//! # The startup receipt
//!
//! Each successful [`AuthorityRegistry::open`] writes [`RECEIPT_FILE_NAME`]: counts, and per
//! subject a summary of what [`derive`] returns over the rehydrated state, with a digest of those
//! decisions. It is deterministic (no clock, no process data), holds no secret, and is read by
//! tests and operators; nothing in the runtime reads it back. Once its lock and restore-marker
//! checks pass, [`AuthorityRegistry::open`] removes a stale receipt before it opens anything, so a
//! refusal after that point leaves none. A start refused earlier — by those checks, before the
//! daemon holds the data-directory lock, or by the N2-A gate — writes nothing, and the previous
//! start's receipt remains: a record of that start, not of this one.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use icn_identity::authority_log::{
    admissible, derive, rehydrate, AdmissionError, AuthorityFactStore, AuthorityState,
    AuthorityStore, AuthorityView, DeviceCapability, FactStoreError, PersistedFact,
    SignedAuthorityEvent, SubjectId,
};
use icn_store::SledStore;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::authority_facts::{DurableStore, SledAuthorityFactStore, AUTHORITY_FACT_PREFIX};
use crate::DataDirLock;

/// Name of the initialization record, relative to the data directory.
pub const INIT_RECORD_FILE_NAME: &str = "n1-authority-initialized.json";

/// Name of the startup receipt, relative to the data directory.
pub const RECEIPT_FILE_NAME: &str = "n1-authority-receipt.json";

/// The store row carrying the instance identifier. Outside [`AUTHORITY_FACT_PREFIX`], so
/// [`rehydrate`] never sees it as a fact.
pub const INSTANCE_KEY: &[u8] = b"authority/meta/instance";

const INIT_RECORD_FORMAT: &str = "icn.n1.authority-store-initialized";
const RECEIPT_FORMAT: &str = "icn.n1.authority-receipt";
const FORMAT_VERSION: u32 = 1;
const STORE_RELATIVE: &str = "store/authority";
const DECISIONS_DIGEST_DOMAIN: &[u8] = b"icn.n1.authority-receipt.decisions.v1\0";
const INSTANCE_LEN: usize = 16;

/// Path of the initialization record for a data directory.
pub fn init_record_path(data_dir: &Path) -> PathBuf {
    data_dir.join(INIT_RECORD_FILE_NAME)
}

/// Path of the startup receipt for a data directory.
pub fn receipt_path(data_dir: &Path) -> PathBuf {
    data_dir.join(RECEIPT_FILE_NAME)
}

/// Why the registry refused to open, or a fact was not ingested.
///
/// Every opening variant is a refusal to start. None of them means "start empty".
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// The lock offered is not this data directory's storage lock, or no longer covers it.
    #[error("refusing to open the N1 authority registry for {data_dir}: {reason}")]
    Lock {
        /// The data directory.
        data_dir: PathBuf,
        /// What was wrong with the lock.
        reason: String,
    },

    /// The initialization record exists but cannot be trusted.
    #[error(
        "refusing to start: the N1 authority initialization record {path} is unreadable ({reason}). \
         It is not treated as absent, because that would let a damaged node re-initialize an \
         empty authority store"
    )]
    RecordUnreadable {
        /// The record.
        path: PathBuf,
        /// Why it could not be used.
        reason: String,
    },

    /// The record says this node initialized its authority store, and the store is gone.
    #[error(
        "refusing to start: {record} records an initialized N1 authority store, but {store} does \
         not exist. Starting with an empty authority state would silently drop every persisted \
         authority fact; restore the store from a backup"
    )]
    StoreMissing {
        /// The missing store directory.
        store: PathBuf,
        /// The record that requires it.
        record: PathBuf,
    },

    /// The store opened but is not the store the record names.
    #[error(
        "refusing to start: the N1 authority store at {store} is not the one {record} records \
         (expected instance {expected}, found {found}). It was replaced or recreated; restore the \
         original store from a backup"
    )]
    StoreReplaced {
        /// The store directory.
        store: PathBuf,
        /// The record.
        record: PathBuf,
        /// The instance the record names.
        expected: String,
        /// The instance the store holds, or `none`.
        found: String,
    },

    /// The store holds facts but the record that should accompany them is gone.
    #[error(
        "refusing to start: the N1 authority store at {store} holds {facts} fact(s) but its \
         initialization record {record} is missing. Restore the record from a backup"
    )]
    RecordMissing {
        /// The store directory.
        store: PathBuf,
        /// The missing record.
        record: PathBuf,
        /// How many fact rows the store holds.
        facts: usize,
    },

    /// The backing database failed to open, read or write.
    #[error("refusing to start: the N1 authority store at {store} failed: {reason}")]
    Store {
        /// The store directory.
        store: PathBuf,
        /// The backend error.
        reason: String,
    },

    /// A persisted record could not be re-admitted. The store is refused whole.
    #[error("refusing to start: the N1 authority store at {store} did not rehydrate: {source}")]
    Rehydrate {
        /// The store directory.
        store: PathBuf,
        /// The first record that failed.
        #[source]
        source: FactStoreError,
    },

    /// The initialization record could not be made durable.
    #[error("refusing to start: could not write the N1 authority initialization record {path}: {reason}")]
    RecordUnwritable {
        /// The record.
        path: PathBuf,
        /// What failed.
        reason: String,
    },

    /// The startup receipt could not be written, or a stale one removed.
    #[error("refusing to start: could not write the N1 authority receipt {path}: {reason}")]
    ReceiptUnwritable {
        /// The receipt.
        path: PathBuf,
        /// What failed.
        reason: String,
    },

    /// A fact offered to [`AuthorityRegistry::ingest`] is not admissible.
    #[error("authority fact is not admissible: {0}")]
    Inadmissible(#[source] AdmissionError),

    /// A fact could not be persisted; it was not published either.
    #[error("authority fact was not persisted: {0}")]
    Persist(#[source] FactStoreError),
}

/// How [`AuthorityRegistry::open`] found the data directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Startup {
    /// Neither record nor store existed: a fresh node, initialized now.
    Initialized,
    /// A store with no facts and no record: an initialization interrupted before its record was
    /// written, completed now.
    CompletedInterruptedInitialization,
    /// Record and matching store: rehydrated.
    Rehydrated,
}

/// The daemon-owned N1 authority state. See the module documentation.
pub struct AuthorityRegistry {
    live: AuthorityStore,
    durable: SledAuthorityFactStore,
    instance: [u8; INSTANCE_LEN],
}

impl std::fmt::Debug for AuthorityRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorityRegistry")
            .field("instance", &hex::encode(self.instance))
            .field("bodies", &self.live.body_count())
            .field("witnesses", &self.live.witness_count())
            .finish()
    }
}

impl AuthorityRegistry {
    /// Open, initialize or refuse the authority store of `data_dir`, rehydrate it, and write the
    /// startup receipt.
    ///
    /// `lock` must be this directory's storage lock, still anchored: asking for it keeps the
    /// record, the store and the receipt inside the exclusion domain. The restore-marker refusal
    /// is asked again here rather than assumed from the lock, because not every storage lock is
    /// taken through [`DataDirLock::acquire`] (`acquire_for_restore_recovery` skips it by design).
    /// The daemon also runs the N2-A startup gate before calling this, so the store is audited
    /// before it is read.
    pub fn open(
        data_dir: &Path,
        lock: &DataDirLock,
    ) -> Result<(Self, AuthorityReceipt), RegistryError> {
        check_lock(data_dir, lock)?;
        crate::restore_marker::refuse_if_restore_incomplete(data_dir, "the N1 authority registry")
            .map_err(|e| RegistryError::Lock {
                data_dir: data_dir.to_path_buf(),
                reason: format!("{e:#}"),
            })?;

        let store_dir = crate::config::authority_fact_store_path(data_dir);
        let record_path = init_record_path(data_dir);
        let receipt = receipt_path(data_dir);

        remove_stale_receipt(data_dir, &receipt)?;
        let record = read_record(&record_path)?;
        // Only `NotFound` is absence. Failing to look is not evidence that nothing is there, and
        // reading it as absence would initialize over a store this process merely cannot see.
        let store_exists = match std::fs::symlink_metadata(&store_dir) {
            Ok(_) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => {
                return Err(RegistryError::Store {
                    store: store_dir,
                    reason: format!("cannot inspect: {e}"),
                });
            }
        };

        let (store, instance, startup) = match (record, store_exists) {
            (Some(_), false) => {
                return Err(RegistryError::StoreMissing {
                    store: store_dir,
                    record: record_path,
                });
            }
            (Some(record), true) => {
                let store = open_store(&store_dir)?;
                match read_instance(&store, &store_dir)? {
                    Some(found) if found == record => {}
                    found => {
                        return Err(RegistryError::StoreReplaced {
                            store: store_dir,
                            record: record_path,
                            expected: hex::encode(record),
                            found: found.map(hex::encode).unwrap_or_else(|| "none".into()),
                        });
                    }
                }
                (store, record, Startup::Rehydrated)
            }
            (None, true) => {
                let store = open_store(&store_dir)?;
                let facts = store
                    .scan(AUTHORITY_FACT_PREFIX)
                    .map_err(|e| store_error(&store_dir, e))?
                    .len();
                if facts > 0 {
                    return Err(RegistryError::RecordMissing {
                        store: store_dir,
                        record: record_path,
                        facts,
                    });
                }
                let instance = match read_instance(&store, &store_dir)? {
                    Some(instance) => instance,
                    None => write_new_instance(&store, &store_dir)?,
                };
                lock_still_held(data_dir, lock)?;
                write_record(data_dir, &record_path, instance)?;
                (store, instance, Startup::CompletedInterruptedInitialization)
            }
            (None, false) => {
                let store = open_store(&store_dir)?;
                let instance = write_new_instance(&store, &store_dir)?;
                lock_still_held(data_dir, lock)?;
                write_record(data_dir, &record_path, instance)?;
                (store, instance, Startup::Initialized)
            }
        };

        let registry = Self::rehydrated(store, instance, &store_dir)?;
        let receipt_body = registry.receipt(startup);
        lock_still_held(data_dir, lock)?;
        write_receipt(data_dir, &receipt, &receipt_body)?;
        Ok((registry, receipt_body))
    }

    fn rehydrated(
        store: Arc<dyn DurableStore>,
        instance: [u8; INSTANCE_LEN],
        store_dir: &Path,
    ) -> Result<Self, RegistryError> {
        let durable = SledAuthorityFactStore::new(store);
        let live = rehydrate(&durable).map_err(|source| RegistryError::Rehydrate {
            store: store_dir.to_path_buf(),
            source,
        })?;
        Ok(AuthorityRegistry {
            live,
            durable,
            instance,
        })
    }

    /// Admit `event`, persist it through the durability barrier, then publish it.
    ///
    /// On any error the in-memory state is unchanged. If persisting failed after the write
    /// reached the medium, the fact may be present on the next start: it was admitted, so that
    /// is a fact the store was allowed to hold, and writing it again is safe (ADR-0087 §3, §4).
    ///
    /// No production code calls this yet. Facts reach a running node only once a producer with
    /// its own design and security review exists.
    pub fn ingest(&mut self, event: &SignedAuthorityEvent) -> Result<(), RegistryError> {
        admissible(&event.body, &event.signature).map_err(RegistryError::Inadmissible)?;
        self.durable
            .put_fact(&PersistedFact::from_event(event))
            .map_err(RegistryError::Persist)?;
        self.live.ingest(event).map_err(RegistryError::Inadmissible)
    }

    /// What [`derive`] says about `subject` over the published state.
    pub fn view(&self, subject: SubjectId) -> AuthorityView {
        derive(subject, &self.live)
    }

    /// The published durable sets.
    pub fn state(&self) -> &AuthorityStore {
        &self.live
    }

    /// The instance identifier shared by the record and the store.
    pub fn instance(&self) -> [u8; INSTANCE_LEN] {
        self.instance
    }

    /// A receipt for the current published state.
    pub fn receipt(&self, startup: Startup) -> AuthorityReceipt {
        AuthorityReceipt::of(&self.live, self.instance, startup)
    }
}

// ── the receipt ─────────────────────────────────────────────────────────────────────────────────

/// What a start rehydrated, and what [`derive`] decides over it. See the module documentation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthorityReceipt {
    /// Always `icn.n1.authority-receipt`.
    pub format: String,
    /// The receipt layout version.
    pub version: u32,
    /// The store instance, hex.
    pub instance: String,
    /// How the start found the data directory.
    pub startup: Startup,
    /// Admitted bodies held.
    pub body_count: usize,
    /// Witnesses held.
    pub witness_count: usize,
    /// One entry per subject named by a held body, in subject order.
    pub subjects: Vec<SubjectReceipt>,
    /// SHA-256, hex, over a domain separator and the JSON of `subjects`.
    pub decisions_digest: String,
}

/// The derived decision for one subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubjectReceipt {
    /// The subject, hex.
    pub subject: String,
    /// What [`derive`] returned.
    pub view: ViewReceipt,
}

/// A summary of an [`AuthorityView`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", deny_unknown_fields)]
pub enum ViewReceipt {
    /// No inception body is held for the subject.
    Unknown,
    /// The chain advanced to `frontier`.
    #[serde(rename_all = "camelCase")]
    Live {
        /// The folded state.
        state: StateReceipt,
        /// The first position with no authorized candidate.
        frontier: u64,
    },
    /// Equally-authorized candidates at `disputed_at`.
    #[serde(rename_all = "camelCase")]
    Halted {
        /// The folded state before the dispute.
        state: StateReceipt,
        /// The disputed position.
        disputed_at: u64,
        /// The candidates' event ids, hex, sorted.
        candidates: Vec<String>,
    },
}

/// A summary of an [`AuthorityState`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateReceipt {
    /// The establishment generation.
    pub generation: u64,
    /// The authority principals, hex, sorted.
    pub authority: Vec<String>,
    /// The device grants, in device order.
    pub devices: Vec<DeviceReceipt>,
}

/// One device grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceReceipt {
    /// The device principal, hex.
    pub device: String,
    /// The granted capabilities, by name.
    pub capabilities: Vec<String>,
    /// The position the grant was made at.
    pub granted_at: u64,
    /// The validity span, inclusive, if any.
    pub validity: Option<[u64; 2]>,
}

impl AuthorityReceipt {
    /// The receipt for `store`: every subject a held body names, each run through [`derive`].
    ///
    /// Public so a test can compute what a receipt must say from a store it built itself, without
    /// going through the registry under test.
    pub fn of(store: &AuthorityStore, instance: [u8; INSTANCE_LEN], startup: Startup) -> Self {
        let subjects: BTreeSet<SubjectId> = store.bodies().iter().map(|b| b.subject()).collect();
        let subjects: Vec<SubjectReceipt> = subjects
            .into_iter()
            .map(|subject| SubjectReceipt {
                subject: hex::encode(subject.as_bytes()),
                view: ViewReceipt::of(&derive(subject, store)),
            })
            .collect();
        let decisions_digest = decisions_digest(&subjects);
        AuthorityReceipt {
            format: RECEIPT_FORMAT.into(),
            version: FORMAT_VERSION,
            instance: hex::encode(instance),
            startup,
            body_count: store.body_count(),
            witness_count: store.witness_count(),
            subjects,
            decisions_digest,
        }
    }
}

impl ViewReceipt {
    fn of(view: &AuthorityView) -> Self {
        match view {
            AuthorityView::Unknown => ViewReceipt::Unknown,
            AuthorityView::Live { state, frontier } => ViewReceipt::Live {
                state: StateReceipt::of(state),
                frontier: *frontier,
            },
            AuthorityView::Halted {
                state,
                disputed_at,
                candidates,
            } => ViewReceipt::Halted {
                state: StateReceipt::of(state),
                disputed_at: *disputed_at,
                candidates: candidates
                    .iter()
                    .map(|body| hex::encode(body.event_id().as_bytes()))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            },
        }
    }
}

impl StateReceipt {
    fn of(state: &AuthorityState) -> Self {
        StateReceipt {
            generation: state.generation,
            authority: state
                .authority
                .iter()
                .map(|key| hex::encode(key.as_bytes()))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            devices: state
                .devices
                .iter()
                .map(|(device, grant)| DeviceReceipt {
                    device: hex::encode(device.as_bytes()),
                    capabilities: ALL_CAPABILITIES
                        .iter()
                        .filter(|cap| grant.capabilities.contains(**cap))
                        .map(|cap| capability_name(*cap).to_string())
                        .collect(),
                    granted_at: grant.granted_at,
                    validity: grant
                        .validity
                        .map(|span| [span.from_position, span.until_position]),
                })
                .collect(),
        }
    }
}

/// Every capability, for listing a set that exposes only `contains`. [`capability_name`]'s match
/// is exhaustive, so a new variant fails to compile there and points back here.
const ALL_CAPABILITIES: [DeviceCapability; 4] = [
    DeviceCapability::Sign,
    DeviceCapability::Encrypt,
    DeviceCapability::Present,
    DeviceCapability::Recover,
];

fn capability_name(cap: DeviceCapability) -> &'static str {
    match cap {
        DeviceCapability::Sign => "sign",
        DeviceCapability::Encrypt => "encrypt",
        DeviceCapability::Present => "present",
        DeviceCapability::Recover => "recover",
    }
}

fn decisions_digest(subjects: &[SubjectReceipt]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(DECISIONS_DIGEST_DOMAIN);
    // Serializing owned strings, integers and vectors cannot fail; an empty digest input would
    // still be a distinct, honest digest of nothing rather than a panic.
    hasher.update(serde_json::to_vec(subjects).unwrap_or_default());
    hex::encode(hasher.finalize())
}

// ── the initialization record ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InitRecord {
    format: String,
    version: u32,
    instance: String,
    store: String,
}

/// `None` when no record exists. A record that exists but cannot be read is an error, never
/// `None`: treating it as absent would re-initialize over a node that had one.
fn read_record(path: &Path) -> Result<Option<[u8; INSTANCE_LEN]>, RegistryError> {
    let unreadable = |reason: String| RegistryError::RecordUnreadable {
        path: path.to_path_buf(),
        reason,
    };
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(unreadable(format!("cannot inspect: {e}"))),
        Ok(_) => {}
    }
    let bytes = std::fs::read(path).map_err(|e| unreadable(format!("cannot read: {e}")))?;
    let record: InitRecord =
        serde_json::from_slice(&bytes).map_err(|e| unreadable(format!("not a record: {e}")))?;
    if record.format != INIT_RECORD_FORMAT || record.version != FORMAT_VERSION {
        return Err(unreadable(format!(
            "unsupported record {} v{}",
            record.format, record.version
        )));
    }
    if record.store != STORE_RELATIVE {
        return Err(unreadable(format!(
            "names store {:?}, expected {STORE_RELATIVE:?}",
            record.store
        )));
    }
    decode_instance(&record.instance)
        .ok_or_else(|| unreadable("malformed instance".into()))
        .map(Some)
}

fn write_record(
    data_dir: &Path,
    path: &Path,
    instance: [u8; INSTANCE_LEN],
) -> Result<(), RegistryError> {
    let record = InitRecord {
        format: INIT_RECORD_FORMAT.into(),
        version: FORMAT_VERSION,
        instance: hex::encode(instance),
        store: STORE_RELATIVE.into(),
    };
    let unwritable = |reason: String| RegistryError::RecordUnwritable {
        path: path.to_path_buf(),
        reason,
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|e| unwritable(e.to_string()))?;
    write_atomically(data_dir, path, &bytes).map_err(unwritable)
}

// ── the store ───────────────────────────────────────────────────────────────────────────────────

fn open_store(store_dir: &Path) -> Result<Arc<dyn DurableStore>, RegistryError> {
    let store = SledStore::open(store_dir).map_err(|e| store_error(store_dir, e))?;
    Ok(Arc::new(store))
}

fn read_instance(
    store: &Arc<dyn DurableStore>,
    store_dir: &Path,
) -> Result<Option<[u8; INSTANCE_LEN]>, RegistryError> {
    match store
        .get(INSTANCE_KEY)
        .map_err(|e| store_error(store_dir, e))?
    {
        None => Ok(None),
        Some(bytes) => <[u8; INSTANCE_LEN]>::try_from(bytes.as_slice())
            .map(Some)
            .map_err(|_| RegistryError::Store {
                store: store_dir.to_path_buf(),
                reason: format!(
                    "instance row is {} bytes, expected {INSTANCE_LEN}",
                    bytes.len()
                ),
            }),
    }
}

/// Write a fresh instance row, and get it onto disk before the record that names it exists.
///
/// The barrier alone survives a crash of this process but not power loss: sled's flush waits with
/// `sync_file_range` (ADR-0087 §9), while the record gets a full `fsync`. Power lost between the
/// two would leave a record naming an instance the store never kept, and the next start would
/// refuse a node that has nothing to restore.
///
/// So after the barrier, every file of the store and the directories that name it are fsynced
/// through descriptors of their own: `fsync` acts on the inode, whoever wrote it. Closing and
/// reopening the database would also fsync, but sled can release its file lock a moment after
/// the handle drops, so an immediate reopen could refuse a first start, and its close path
/// unwraps a failed fsync. Once per node lifetime.
fn write_new_instance(
    store: &Arc<dyn DurableStore>,
    store_dir: &Path,
) -> Result<[u8; INSTANCE_LEN], RegistryError> {
    let instance: [u8; INSTANCE_LEN] = rand::random();
    store
        .put(INSTANCE_KEY, &instance)
        .and_then(|()| store.sync_durable())
        .map_err(|e| store_error(store_dir, e))?;
    fsync_store(store_dir).map_err(|e| store_error(store_dir, e))?;
    Ok(instance)
}

/// `fsync` each regular file directly in `store_dir`, then `store_dir` and its parent.
fn fsync_store(store_dir: &Path) -> anyhow::Result<()> {
    use anyhow::Context as _;

    for entry in std::fs::read_dir(store_dir).context("listing the store")? {
        let entry = entry.context("listing the store")?;
        if entry
            .file_type()
            .context("inspecting a store file")?
            .is_file()
        {
            std::fs::File::open(entry.path())
                .and_then(|file| file.sync_all())
                .with_context(|| format!("fsync {}", entry.path().display()))?;
        }
    }
    crate::restore_marker::sync_directory(store_dir)?;
    if let Some(parent) = store_dir.parent() {
        crate::restore_marker::sync_directory(parent)?;
    }
    Ok(())
}

fn store_error(store_dir: &Path, error: anyhow::Error) -> RegistryError {
    RegistryError::Store {
        store: store_dir.to_path_buf(),
        reason: format!("{error:#}"),
    }
}

fn decode_instance(hex_instance: &str) -> Option<[u8; INSTANCE_LEN]> {
    hex::decode(hex_instance)
        .ok()
        .and_then(|bytes| <[u8; INSTANCE_LEN]>::try_from(bytes.as_slice()).ok())
}

// ── files and the lock ──────────────────────────────────────────────────────────────────────────

fn check_lock(data_dir: &Path, lock: &DataDirLock) -> Result<(), RegistryError> {
    if lock.path() != DataDirLock::lock_path(data_dir) {
        return Err(RegistryError::Lock {
            data_dir: data_dir.to_path_buf(),
            reason: format!(
                "the lock offered is {}, not this directory's storage lock",
                lock.path().display()
            ),
        });
    }
    lock_still_held(data_dir, lock)
}

fn lock_still_held(data_dir: &Path, lock: &DataDirLock) -> Result<(), RegistryError> {
    lock.assert_still_anchored("opening the N1 authority registry")
        .map_err(|e| RegistryError::Lock {
            data_dir: data_dir.to_path_buf(),
            reason: format!("{e:#}"),
        })
}

fn remove_stale_receipt(data_dir: &Path, path: &Path) -> Result<(), RegistryError> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(RegistryError::ReceiptUnwritable {
            path: path.to_path_buf(),
            reason: format!("could not remove the previous receipt: {e}"),
        }),
        Ok(()) => crate::restore_marker::sync_directory(data_dir).map_err(|e| {
            RegistryError::ReceiptUnwritable {
                path: path.to_path_buf(),
                reason: format!("{e:#}"),
            }
        }),
    }
}

fn write_receipt(
    data_dir: &Path,
    path: &Path,
    receipt: &AuthorityReceipt,
) -> Result<(), RegistryError> {
    let unwritable = |reason: String| RegistryError::ReceiptUnwritable {
        path: path.to_path_buf(),
        reason,
    };
    let bytes = serde_json::to_vec_pretty(receipt).map_err(|e| unwritable(e.to_string()))?;
    write_atomically(data_dir, path, &bytes).map_err(unwritable)
}

/// Write so a reader sees the previous file or the new one, never a partial one: a synced sibling
/// temporary, renamed over the target, then the directory synced. The temporary is never read,
/// and a leftover one is truncated by the next write.
fn write_atomically(data_dir: &Path, path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write as _;

    let mut tmp_name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    {
        let mut file = std::fs::File::create(&tmp)
            .map_err(|e| format!("create {} failed: {e}", tmp.display()))?;
        file.write_all(bytes)
            .map_err(|e| format!("write failed: {e}"))?;
        file.sync_all().map_err(|e| format!("sync failed: {e}"))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("rename failed: {e}"))?;
    crate::restore_marker::sync_directory(data_dir).map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    //! The write order, against a backend that can fail on demand. The daemon-level behaviour is
    //! in `tests/authority_registry.rs` and in `icnd`'s restart suite.

    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use icn_identity::authority_log::{
        authorize_event, CapabilitySet, ContextNonce, ContinuityRoot, PrincipalKey,
        WitnessSignature,
    };
    use icn_store::{ContentHash, ReplicaMetadata, Store};

    use super::*;

    #[derive(Default)]
    struct FlakyStore {
        rows: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
        fail_put: Mutex<bool>,
        fail_sync: Mutex<bool>,
    }

    impl Store for FlakyStore {
        fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
            Ok(self.rows.lock().unwrap().get(key).cloned())
        }
        fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
            if *self.fail_put.lock().unwrap() {
                anyhow::bail!("injected put failure");
            }
            self.rows
                .lock()
                .unwrap()
                .insert(key.to_vec(), value.to_vec());
            Ok(())
        }
        fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
            self.rows.lock().unwrap().remove(key);
            Ok(())
        }
        fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
            let mut rows = self.rows.lock().unwrap();
            for key in keys {
                rows.remove(key);
            }
            Ok(())
        }
        fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .filter(|(key, _)| key.starts_with(prefix))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect())
        }
        fn get_replica_metadata(&self, _: &ContentHash) -> anyhow::Result<Option<ReplicaMetadata>> {
            Ok(None)
        }
        fn put_replica_metadata(&self, _: &ReplicaMetadata) -> anyhow::Result<()> {
            Ok(())
        }
        fn list_replica_hashes(&self) -> anyhow::Result<Vec<ContentHash>> {
            Ok(Vec::new())
        }
    }

    impl DurableStore for FlakyStore {
        fn sync_durable(&self) -> anyhow::Result<()> {
            if *self.fail_sync.lock().unwrap() {
                anyhow::bail!("injected sync failure");
            }
            Ok(())
        }
    }

    fn registry_over(store: &Arc<FlakyStore>) -> AuthorityRegistry {
        AuthorityRegistry::rehydrated(
            Arc::clone(store) as Arc<dyn DurableStore>,
            [7; INSTANCE_LEN],
            Path::new("/flaky"),
        )
        .unwrap()
    }

    fn events() -> (SignedAuthorityEvent, SignedAuthorityEvent) {
        let root = ContinuityRoot::new([3; 32], ContextNonce::from_bytes([0xa6; 32]), 2);
        let inception = root.incept().unwrap();
        let device = PrincipalKey::try_from_verifying_key(
            ed25519_dalek::SigningKey::from_bytes(&[10; 32]).verifying_key(),
        )
        .unwrap();
        let grant = authorize_event(
            &root.authority_signing_key(0),
            inception.body.subject(),
            1,
            inception.body.event_id(),
            device,
            CapabilitySet::new([DeviceCapability::Sign]),
            None,
        );
        (inception, grant)
    }

    fn fact_rows(store: &FlakyStore) -> usize {
        store.scan(AUTHORITY_FACT_PREFIX).unwrap().len()
    }

    #[test]
    fn an_admitted_fact_is_persisted_and_then_published() {
        let store = Arc::new(FlakyStore::default());
        let mut registry = registry_over(&store);
        let (inception, grant) = events();

        registry.ingest(&inception).unwrap();
        registry.ingest(&grant).unwrap();

        assert_eq!(fact_rows(&store), 2);
        assert_eq!(registry.state().body_count(), 2);
        assert!(matches!(
            registry.view(inception.body.subject()),
            AuthorityView::Live { frontier: 2, .. }
        ));
    }

    #[test]
    fn a_failed_barrier_publishes_nothing_and_a_retry_publishes_once() {
        let store = Arc::new(FlakyStore::default());
        let mut registry = registry_over(&store);
        let (inception, _) = events();

        *store.fail_sync.lock().unwrap() = true;
        let error = registry.ingest(&inception).unwrap_err();
        assert!(matches!(error, RegistryError::Persist(_)), "{error}");
        assert_eq!(
            registry.state().body_count(),
            0,
            "a fact the medium did not acknowledge must not be published"
        );
        // The write reached the backend before the barrier failed. That row was admitted, so a
        // restart may legitimately rehydrate it; this is the "may or may not be held" of §4.
        assert_eq!(fact_rows(&store), 1);

        *store.fail_sync.lock().unwrap() = false;
        registry.ingest(&inception).unwrap();
        assert_eq!(fact_rows(&store), 1);
        assert_eq!(registry.state().body_count(), 1);
    }

    #[test]
    fn a_failed_write_publishes_nothing() {
        let store = Arc::new(FlakyStore::default());
        let mut registry = registry_over(&store);
        let (inception, _) = events();

        *store.fail_put.lock().unwrap() = true;
        assert!(matches!(
            registry.ingest(&inception),
            Err(RegistryError::Persist(_))
        ));
        assert_eq!(fact_rows(&store), 0);
        assert_eq!(registry.state().body_count(), 0);
    }

    #[test]
    fn an_inadmissible_fact_is_neither_persisted_nor_published() {
        let store = Arc::new(FlakyStore::default());
        let mut registry = registry_over(&store);
        let (inception, _) = events();

        let mut signature = *inception.signature.as_bytes();
        signature[0] ^= 0x01;
        let forged = SignedAuthorityEvent::new(
            inception.body.clone(),
            WitnessSignature::from_bytes(signature),
        );

        assert!(matches!(
            registry.ingest(&forged),
            Err(RegistryError::Inadmissible(_))
        ));
        assert_eq!(fact_rows(&store), 0, "admission precedes persistence");
        assert_eq!(registry.state().body_count(), 0);
    }

    #[test]
    fn a_restart_rehydrates_what_was_acknowledged_including_an_unpublished_write() {
        // The window between acknowledgment and publication: the fact is on the medium, the
        // process dies before publishing. A restart rebuilds from the medium alone.
        let store = Arc::new(FlakyStore::default());
        let (inception, grant) = events();
        {
            let mut registry = registry_over(&store);
            registry.ingest(&inception).unwrap();
            // Persist the second fact without publishing it, as a crash after the barrier would.
            registry
                .durable
                .put_fact(&PersistedFact::from_event(&grant))
                .unwrap();
            assert_eq!(registry.state().body_count(), 1);
        }
        let restarted = registry_over(&store);
        assert_eq!(restarted.state().body_count(), 2);
    }
}
