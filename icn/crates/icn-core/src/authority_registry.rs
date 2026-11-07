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
//! decisions. It is deterministic (no clock, no process data) and holds no cryptographic secret,
//! but it names subjects, public authority keys and device grants, so it is created owner-only
//! and stays local. Its readers are tests and operators; nothing in the runtime reads it back, and
//! it is neither an authority certificate nor proof that a daemon is running (ADR-0088 §10).
//!
//! The daemon discards the previous receipt ([`AuthorityRegistry::discard_receipt`]) as soon as it
//! holds the data-directory lock, before the N2-A gate, and [`AuthorityRegistry::open`] does the
//! same once its own checks pass. So a start refused at the gate or here leaves none. A start
//! refused before the lock (another holder, an incomplete restore) cannot touch the directory,
//! and an earlier receipt may remain: a record of that start, not of this one.
//!
//! # Who can read the store
//!
//! sled creates the store's files with no explicit mode, so they follow the process umask. The
//! store directory does not: it is created `0700` whatever the umask, and an existing one is
//! checked before the registry opens it (the N2-A gate's audit has opened it already, with the
//! mode it was found with). A directory of this account with a wider mode, as a restore can
//! leave it, is set to `0700` with its contents untouched; a link, a non-directory or another
//! account's directory is refused. Nothing above `store/authority` is changed: the data directory
//! and every other store still follow the umask (ADR-0088 §11).

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

    /// The store directory cannot be kept owner-only: it is a link or not a directory, belongs to
    /// another account, or its mode could not be set. Nothing in it was opened.
    #[error(
        "refusing to start: the N1 authority store at {store} cannot be kept owner-only: \
         {reason}. It must be a directory, not a link, owned by the account icnd runs as"
    )]
    StoreUnprotected {
        /// The store directory.
        store: PathBuf,
        /// Why.
        reason: String,
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

    /// An offline authority import into this data directory did not finish (ADR-0089 §6).
    ///
    /// The store may hold only part of the named segment. Its history is not complete, so it is
    /// not opened as if it were.
    #[error(
        "refusing to start: an offline N1 authority import into this data directory did not \
         finish ({marker} names subject {subject}, segment {segment}). The authority store may \
         hold only part of that segment. `icnctl authority import --resume` is the command that \
         will complete it, and it is not implemented yet; until it is, copy the whole data \
         directory first, then restore it from a backup taken before the import (ADR-0088 §12). \
         Do not remove the marker by hand",
        segment = hex::encode(.segment_id)
    )]
    ImportIncomplete {
        /// The marker.
        marker: PathBuf,
        /// The Subject the unfinished segment belongs to.
        subject: SubjectId,
        /// `SHA-256` of the unfinished segment's canonical bytes.
        segment_id: [u8; 32],
    },

    /// Something is at the import marker's name, but it could not be read as a marker record.
    ///
    /// Still a refusal: an unreadable marker is not an absent one.
    #[error(
        "refusing to start: something is at {marker}, so an offline N1 authority import into \
         this data directory may not have finished, but it could not be read ({reason}). It is \
         not treated as absent. If the cause is temporary (permissions, I/O), fix it and start \
         again. Otherwise no import can be resumed from this marker: copy the whole data \
         directory first, then restore it from a backup taken before the import (ADR-0088 §12). \
         Do not remove the marker by hand"
    )]
    ImportMarkerUnreadable {
        /// The marker.
        marker: PathBuf,
        /// Why it could not be read.
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
        refuse_incomplete_restore(data_dir)?;
        // Before anything below writes: a refused open removes no receipt, writes no record and
        // opens no store (ADR-0089 §7).
        refuse_incomplete_import(data_dir)?;

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
                let store = open_store(data_dir, &store_dir, true)?;
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
                let store = open_store(data_dir, &store_dir, true)?;
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
                let instance = initialize(&OnDisk {
                    store: &store,
                    store_dir: &store_dir,
                    data_dir,
                    record_path: &record_path,
                    lock,
                })?;
                (store, instance, Startup::CompletedInterruptedInitialization)
            }
            (None, false) => {
                let store = open_store(data_dir, &store_dir, false)?;
                let instance = initialize(&OnDisk {
                    store: &store,
                    store_dir: &store_dir,
                    data_dir,
                    record_path: &record_path,
                    lock,
                })?;
                (store, instance, Startup::Initialized)
            }
        };

        let registry = Self::rehydrated(store, instance, &store_dir)?;
        let receipt_body = registry.receipt(startup);
        lock_still_held(data_dir, lock)?;
        write_receipt(data_dir, &receipt, &receipt_body)?;
        Ok((registry, receipt_body))
    }

    /// Remove the previous start's receipt without opening anything.
    ///
    /// The daemon calls this as soon as it holds the data-directory lock, so a start refused
    /// before [`Self::open`] runs — by the N2-A gate, say — does not leave the previous start's
    /// receipt in place to be read as this one's. Needs this directory's storage lock. Removing
    /// nothing is not an error.
    pub fn discard_receipt(data_dir: &Path, lock: &DataDirLock) -> Result<(), RegistryError> {
        check_lock(data_dir, lock)?;
        refuse_incomplete_restore(data_dir)?;
        remove_stale_receipt(data_dir, &receipt_path(data_dir))
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
        // This module writes the record as a regular file. A link or anything else at its name
        // was put there by something else, and is not followed.
        Ok(meta) if !meta.file_type().is_file() => {
            return Err(unreadable("not a regular file".into()));
        }
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

/// Every open of the store by the registry goes through [`protect_store_dir`] first. A new store
/// is owner-only before sled creates anything in it. An existing one has already been opened by
/// the N2-A gate's audit, with whatever mode it had (ADR-0088 §11).
fn open_store(
    data_dir: &Path,
    store_dir: &Path,
    exists: bool,
) -> Result<Arc<dyn DurableStore>, RegistryError> {
    protect_store_dir(data_dir, store_dir, exists)?;
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

// ── initialization ──────────────────────────────────────────────────────────────────────────────

/// The steps that establish a store's identity.
///
/// A trait only so their order can be observed and failed on demand in this module's tests. It is
/// private, and production has exactly one implementation, [`OnDisk`].
trait InitSteps {
    /// The instance row the store already holds, if any.
    fn existing_instance(&self) -> Result<Option<[u8; INSTANCE_LEN]>, RegistryError>;
    /// Write a new instance row, returning only after the store's durability barrier.
    fn write_instance(&self, instance: [u8; INSTANCE_LEN]) -> Result<(), RegistryError>;
    /// `fsync` the store's files and the directories that name them.
    fn fsync_store(&self) -> Result<(), RegistryError>;
    /// Confirm the exclusion still holds, then write the record atomically: the commit point.
    fn commit_record(&self, instance: [u8; INSTANCE_LEN]) -> Result<(), RegistryError>;
}

/// Establish the store's instance, then commit the record that names it.
///
/// The record is the commit point: once it exists, a missing or different instance is a refusal
/// ([`RegistryError::StoreReplaced`]). So everything it names must be on disk first, in this
/// order, and any failure stops before the record:
///
/// 1. reuse the instance row an interrupted start left, or write a new one behind the store's
///    barrier — which survives a crash of this process, not power loss (sled's flush waits with
///    `sync_file_range`, ADR-0087 §9);
/// 2. `fsync` the store's files and directories through descriptors of their own (`fsync` acts on
///    the inode, whoever wrote it) — also when adopting, because the process that wrote the row
///    may have died before this step;
/// 3. commit the record (synced temporary, rename, directory sync).
///
/// The order is pinned by this module's tests. Those prove the calls happen in this order; they
/// cannot prove what a disk keeps through a power cut.
///
/// Closing and reopening the database would also `fsync`, but sled can release its file lock a
/// moment after the handle drops, so an immediate reopen could refuse a first start, and its close
/// path unwraps a failed `fsync`.
fn initialize(steps: &dyn InitSteps) -> Result<[u8; INSTANCE_LEN], RegistryError> {
    let instance = match steps.existing_instance()? {
        Some(instance) => instance,
        None => {
            let instance: [u8; INSTANCE_LEN] = rand::random();
            steps.write_instance(instance)?;
            instance
        }
    };
    steps.fsync_store()?;
    steps.commit_record(instance)?;
    Ok(instance)
}

/// [`InitSteps`] over a real store and data directory.
struct OnDisk<'a> {
    store: &'a Arc<dyn DurableStore>,
    store_dir: &'a Path,
    data_dir: &'a Path,
    record_path: &'a Path,
    lock: &'a DataDirLock,
}

impl InitSteps for OnDisk<'_> {
    fn existing_instance(&self) -> Result<Option<[u8; INSTANCE_LEN]>, RegistryError> {
        read_instance(self.store, self.store_dir)
    }

    fn write_instance(&self, instance: [u8; INSTANCE_LEN]) -> Result<(), RegistryError> {
        self.store
            .put(INSTANCE_KEY, &instance)
            .and_then(|()| self.store.sync_durable())
            .map_err(|e| store_error(self.store_dir, e))
    }

    fn fsync_store(&self) -> Result<(), RegistryError> {
        fsync_store_files(self.store_dir).map_err(|e| store_error(self.store_dir, e))
    }

    fn commit_record(&self, instance: [u8; INSTANCE_LEN]) -> Result<(), RegistryError> {
        lock_still_held(self.data_dir, self.lock)?;
        write_record(self.data_dir, self.record_path, instance)
    }
}

/// `fsync` each regular file directly in `store_dir`, then `store_dir` and its parent.
fn fsync_store_files(store_dir: &Path) -> anyhow::Result<()> {
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

// ── the store directory's mode ──────────────────────────────────────────────────────────────────

/// The mode of `store/authority`, whatever the process umask (ADR-0088 §11). sled creates its
/// files with no explicit mode, so they follow the umask; a directory no other account can
/// traverse keeps them unreachable anyway. Every other store in the data directory still follows
/// the umask.
const STORE_DIR_MODE: u32 = 0o700;

/// What an existing store directory needs before sled may open it.
#[derive(Debug, PartialEq, Eq)]
enum StoreDirVerdict {
    /// A directory of this account, already owner-only.
    Protected,
    /// A directory of this account with some other mode: set it to [`STORE_DIR_MODE`], keeping
    /// everything in it.
    Tighten {
        /// The mode it had.
        from: u32,
    },
}

/// The policy, as a pure function: an unprivileged test cannot create a directory owned by
/// another account, but it can drive this with one.
///
/// `account` is the owner this process gives a new file. A directory owned by anyone else is
/// refused, not taken over: changing its mode would not stop its owner from changing it back.
fn judge_store_dir(
    file_type: std::fs::FileType,
    owner: u32,
    mode: u32,
    account: u32,
) -> Result<StoreDirVerdict, String> {
    if file_type.is_symlink() {
        return Err("it is a link, not a directory; links are not followed here".into());
    }
    if !file_type.is_dir() {
        return Err("it is not a directory".into());
    }
    if owner != account {
        return Err(format!(
            "it is owned by uid {owner}, not by uid {account}, the account this process runs as"
        ));
    }
    if mode == STORE_DIR_MODE {
        Ok(StoreDirVerdict::Protected)
    } else {
        Ok(StoreDirVerdict::Tighten { from: mode })
    }
}

/// Create `store_dir` owner-only, or check an existing one and set it owner-only. Runs under the
/// data-directory lock, before sled touches the directory.
///
/// Its contents are never touched: a restored store with a wider mode keeps every fact, its
/// instance row and the record that names it.
#[cfg(unix)]
fn protect_store_dir(data_dir: &Path, store_dir: &Path, exists: bool) -> Result<(), RegistryError> {
    use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, PermissionsExt as _};

    let unprotected = |reason: String| RegistryError::StoreUnprotected {
        store: store_dir.to_path_buf(),
        reason,
    };
    if !exists {
        if let Some(parent) = store_dir.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| unprotected(format!("cannot create {}: {e}", parent.display())))?;
        }
        // Not `create_dir_all`: the directory was just observed absent under the lock, so finding
        // it now means something else made it.
        std::fs::DirBuilder::new()
            .mode(STORE_DIR_MODE)
            .create(store_dir)
            .map_err(|e| unprotected(format!("cannot create it: {e}")))?;
    }

    let before = std::fs::symlink_metadata(store_dir)
        .map_err(|e| unprotected(format!("cannot inspect it: {e}")))?;
    // Asked of a probe, not of the directory being judged: a directory swapped in after `mkdir`
    // must still be compared with this process's account.
    let account = crate::identity_new_files_receive(data_dir)
        .map_err(|e| unprotected(format!("{e:#}")))?
        .uid;
    let verdict = judge_store_dir(
        before.file_type(),
        before.uid(),
        before.mode() & 0o7777,
        account,
    )
    .map_err(unprotected)?;

    // Act through a descriptor, and only on the inode just judged: a directory swapped in between
    // is refused rather than chmodded.
    let dir =
        std::fs::File::open(store_dir).map_err(|e| unprotected(format!("cannot open it: {e}")))?;
    let opened = dir
        .metadata()
        .map_err(|e| unprotected(format!("cannot inspect it: {e}")))?;
    if (opened.dev(), opened.ino()) != (before.dev(), before.ino()) {
        return Err(unprotected("it was replaced while being checked".into()));
    }
    if let StoreDirVerdict::Tighten { from } = verdict {
        dir.set_permissions(std::fs::Permissions::from_mode(STORE_DIR_MODE))
            .and_then(|()| dir.sync_all())
            .map_err(|e| unprotected(format!("cannot set mode {STORE_DIR_MODE:o}: {e}")))?;
        if exists {
            tracing::warn!(
                store = %store_dir.display(),
                from = format_args!("{from:o}"),
                "N1 authority store: directory mode set to 700"
            );
        }
    }
    Ok(())
}

/// Non-Unix makes no mode claim.
#[cfg(not(unix))]
fn protect_store_dir(_: &Path, _: &Path, _: bool) -> Result<(), RegistryError> {
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

/// Asked here rather than assumed from the lock: `acquire_for_restore_recovery` is a storage lock
/// that skips it by design.
fn refuse_incomplete_restore(data_dir: &Path) -> Result<(), RegistryError> {
    crate::restore_marker::refuse_if_restore_incomplete(data_dir, "the N1 authority registry")
        .map_err(|e| RegistryError::Lock {
            data_dir: data_dir.to_path_buf(),
            reason: format!("{e:#}"),
        })
}

/// Any entry at the import marker's name refuses; only a confirmed absence passes (ADR-0089 §6).
fn refuse_incomplete_import(data_dir: &Path) -> Result<(), RegistryError> {
    use crate::import_marker::{import_marker_path, inspect_import_marker, ImportMarkerState};
    match inspect_import_marker(data_dir) {
        ImportMarkerState::Absent => Ok(()),
        ImportMarkerState::Incomplete(identity) => Err(RegistryError::ImportIncomplete {
            marker: import_marker_path(data_dir),
            subject: identity.subject,
            segment_id: identity.segment_id,
        }),
        ImportMarkerState::Unreadable(reason) => Err(RegistryError::ImportMarkerUnreadable {
            marker: import_marker_path(data_dir),
            reason,
        }),
    }
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
/// temporary, renamed over the target, then the directory synced. The temporary is never read; a
/// leftover one is removed and the new one created exclusively, owner-only.
fn write_atomically(data_dir: &Path, path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write as _;

    let mut tmp_name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    // A leftover temporary is removed, never reused, and the new one is created exclusively:
    // `create_new` is `O_CREAT | O_EXCL`, which refuses any existing name — a link planted there
    // included — instead of following it out of the data directory. Owner-only from creation, so
    // no umask can widen it: these files name subjects, keys and device grants.
    match std::fs::remove_file(&tmp) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("remove stale {} failed: {e}", tmp.display()));
        }
        _ => {}
    }
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options
            .open(&tmp)
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

    // ── initialization order (M9) ───────────────────────────────────────────────────────────────
    //
    // These prove the order of calls and that a failure stops before the record. They cannot prove
    // what a disk keeps through a power cut; nothing in-process can.

    /// One recorded step: its name, and the instance it was given, if any.
    type Call = (&'static str, Option<[u8; INSTANCE_LEN]>);

    /// Records each step and can fail any one of them.
    struct RecordingSteps {
        existing: Option<[u8; INSTANCE_LEN]>,
        fail_at: Option<&'static str>,
        calls: Mutex<Vec<Call>>,
    }

    impl RecordingSteps {
        fn new(existing: Option<[u8; INSTANCE_LEN]>, fail_at: Option<&'static str>) -> Self {
            RecordingSteps {
                existing,
                fail_at,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn step(
            &self,
            name: &'static str,
            instance: Option<[u8; INSTANCE_LEN]>,
        ) -> Result<(), RegistryError> {
            self.calls.lock().unwrap().push((name, instance));
            if self.fail_at == Some(name) {
                return Err(RegistryError::Store {
                    store: PathBuf::from("/recording"),
                    reason: format!("injected failure at {name}"),
                });
            }
            Ok(())
        }

        fn names(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().iter().map(|(n, _)| *n).collect()
        }
    }

    impl InitSteps for RecordingSteps {
        fn existing_instance(&self) -> Result<Option<[u8; INSTANCE_LEN]>, RegistryError> {
            self.step("existing_instance", None)?;
            Ok(self.existing)
        }
        fn write_instance(&self, instance: [u8; INSTANCE_LEN]) -> Result<(), RegistryError> {
            self.step("write_instance", Some(instance))
        }
        fn fsync_store(&self) -> Result<(), RegistryError> {
            self.step("fsync_store", None)
        }
        fn commit_record(&self, instance: [u8; INSTANCE_LEN]) -> Result<(), RegistryError> {
            self.step("commit_record", Some(instance))
        }
    }

    #[test]
    fn a_fresh_instance_is_written_and_fsynced_before_the_record_names_it() {
        let steps = RecordingSteps::new(None, None);
        let instance = initialize(&steps).unwrap();

        assert_eq!(
            steps.names(),
            [
                "existing_instance",
                "write_instance",
                "fsync_store",
                "commit_record"
            ]
        );
        let calls = steps.calls.lock().unwrap();
        assert_eq!(
            calls[1].1,
            Some(instance),
            "the row written is the instance"
        );
        assert_eq!(
            calls[3].1,
            Some(instance),
            "the record names that same instance"
        );
    }

    #[test]
    fn an_adopted_instance_is_fsynced_again_before_the_record_names_it() {
        let steps = RecordingSteps::new(Some([9; INSTANCE_LEN]), None);
        assert_eq!(initialize(&steps).unwrap(), [9; INSTANCE_LEN]);
        assert_eq!(
            steps.names(),
            ["existing_instance", "fsync_store", "commit_record"],
            "no second row, and the row a dead process wrote is fsynced before the commit"
        );
    }

    #[test]
    fn any_failure_before_the_commit_point_leaves_no_record() {
        for (existing, fail_at) in [
            (None, "existing_instance"),
            (None, "write_instance"),
            (None, "fsync_store"),
            (Some([9; INSTANCE_LEN]), "existing_instance"),
            (Some([9; INSTANCE_LEN]), "fsync_store"),
        ] {
            let steps = RecordingSteps::new(existing, Some(fail_at));
            assert!(initialize(&steps).is_err(), "{fail_at} must fail the init");
            let names = steps.names();
            assert_eq!(names.last(), Some(&fail_at), "nothing runs after {fail_at}");
            assert!(!names.contains(&"commit_record"), "{fail_at}: {names:?}");
        }
    }

    #[test]
    fn a_failed_commit_is_reported_not_swallowed() {
        let steps = RecordingSteps::new(None, Some("commit_record"));
        assert!(initialize(&steps).is_err());
    }

    /// The real steps over a fault-injecting store, a real directory and a real lock.
    fn on_disk(
        store: &Arc<FlakyStore>,
        dir: &Path,
        f: impl FnOnce(&OnDisk<'_>) -> Result<[u8; INSTANCE_LEN], RegistryError>,
    ) -> Result<[u8; INSTANCE_LEN], RegistryError> {
        let lock = DataDirLock::acquire(dir, "the init-order test").unwrap();
        let store_dir = crate::config::authority_fact_store_path(dir);
        std::fs::create_dir_all(&store_dir).unwrap();
        let store: Arc<dyn DurableStore> = Arc::clone(store) as Arc<dyn DurableStore>;
        f(&OnDisk {
            store: &store,
            store_dir: &store_dir,
            data_dir: dir,
            record_path: &init_record_path(dir),
            lock: &lock,
        })
    }

    #[test]
    fn a_failed_instance_barrier_leaves_no_record_and_the_next_start_adopts_the_row() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FlakyStore::default());

        *store.fail_sync.lock().unwrap() = true;
        assert!(matches!(
            on_disk(&store, dir.path(), |steps| initialize(steps)),
            Err(RegistryError::Store { .. })
        ));
        assert!(
            !init_record_path(dir.path()).exists(),
            "no record may name an instance whose barrier failed"
        );
        // The write reached the backend before its barrier failed: the crash-equivalent state.
        let written = store.get(INSTANCE_KEY).unwrap().unwrap();

        *store.fail_sync.lock().unwrap() = false;
        let adopted = on_disk(&store, dir.path(), |steps| initialize(steps)).unwrap();
        assert_eq!(
            adopted.as_slice(),
            written.as_slice(),
            "adopted, not replaced"
        );
        assert_eq!(
            read_record(&init_record_path(dir.path())).unwrap(),
            Some(adopted)
        );
    }

    #[test]
    fn a_failed_instance_write_leaves_neither_row_nor_record() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FlakyStore::default());
        *store.fail_put.lock().unwrap() = true;

        assert!(on_disk(&store, dir.path(), |steps| initialize(steps)).is_err());
        assert_eq!(store.get(INSTANCE_KEY).unwrap(), None);
        assert!(!init_record_path(dir.path()).exists());
    }

    #[test]
    fn a_successful_init_records_the_instance_the_store_holds() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FlakyStore::default());

        let instance = on_disk(&store, dir.path(), |steps| initialize(steps)).unwrap();
        assert_eq!(store.get(INSTANCE_KEY).unwrap(), Some(instance.to_vec()));
        assert_eq!(
            read_record(&init_record_path(dir.path())).unwrap(),
            Some(instance)
        );
    }

    // ── the store directory's mode ──────────────────────────────────────────────────────────────

    /// The file types `judge_store_dir` is given, read from real inodes: a directory, a link to
    /// one, and a regular file.
    #[cfg(unix)]
    fn kinds(dir: &Path) -> [std::fs::FileType; 3] {
        let target = dir.join("target");
        std::fs::create_dir(&target).unwrap();
        std::os::unix::fs::symlink(&target, dir.join("link")).unwrap();
        std::fs::write(dir.join("file"), b"").unwrap();
        ["target", "link", "file"].map(|name| {
            std::fs::symlink_metadata(dir.join(name))
                .unwrap()
                .file_type()
        })
    }

    #[cfg(unix)]
    #[test]
    fn only_this_accounts_own_directory_is_opened_and_a_wider_mode_is_tightened() {
        let dir = tempfile::tempdir().unwrap();
        let [directory, link, file] = kinds(dir.path());
        let me = 1000;

        assert_eq!(
            judge_store_dir(directory, me, 0o700, me),
            Ok(StoreDirVerdict::Protected)
        );
        for wider in [0o755, 0o777, 0o750, 0o711, 0o2700, 0o500] {
            assert_eq!(
                judge_store_dir(directory, me, wider, me),
                Ok(StoreDirVerdict::Tighten { from: wider }),
                "{wider:o}"
            );
        }

        let foreign = judge_store_dir(directory, 0, 0o700, me).unwrap_err();
        assert!(foreign.contains("owned by uid 0"), "{foreign}");
        let foreign_and_wide = judge_store_dir(directory, me + 1, 0o777, me).unwrap_err();
        assert!(
            foreign_and_wide.contains("owned by uid"),
            "{foreign_and_wide}"
        );

        let linked = judge_store_dir(link, me, 0o777, me).unwrap_err();
        assert!(linked.contains("link"), "{linked}");
        let regular = judge_store_dir(file, me, 0o700, me).unwrap_err();
        assert!(regular.contains("not a directory"), "{regular}");
    }
}
