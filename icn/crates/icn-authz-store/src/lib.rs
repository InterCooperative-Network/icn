//! Durable replay consumption and atomic local resource reservation for
//! [`icn_authz::effect_v0`] admission. **Experimental.**
//!
//! [`DurableConsumedSet`] is a [`ConsumedSet`] whose consumptions survive a restart of the
//! target, so a restarted target cannot admit an operation it already admitted. It is an
//! adapter: `icn-authz` stays free of storage, and nothing here changes what an authorization
//! means — admission ([`icn_authz::effect_v0::admit`],
//! [`icn_authz::effect_v0::admit_checkpointed`]) still decides replay and conflict from the
//! entry this set returns.
//!
//! ## Stages
//!
//! VERIFIED → ADMITTED → **PERSISTED** → **RESERVED** (this crate) → EXECUTED → SETTLED.
//! [`DurableConsumedSet`] provides persistence alone, so a separately persisted consumption whose
//! effect later fails is **stranded**. [`LocalResourceReservations`] instead commits consumption,
//! capacity reduction, and a reservation record in one local transaction. Neither path executes
//! a physical resource action or performs distributed settlement.
//!
//! ## Durability model
//!
//! - `consume` acknowledges a new consumption (`Ok(None)`) only after an atomic insert-if-absent
//!   **applied** and the backend's durability barrier **returned `Ok`**.
//! - The backend is a [`DurableBackend`]. The only one provided is [`SledStore`]: sled's
//!   `compare_and_swap` for the insert and sled's `flush` (which fsyncs its log) for the barrier.
//! - A generic [`icn_store::Store`] is **not** accepted: `Store::flush` defaults to a no-op, so
//!   implementing it says nothing about durability.
//! - Evidence: clean close/reopen, and a child process aborted (`SIGABRT`) after the
//!   acknowledgement or between the insert and the barrier. Not evidence for: OS crash, power
//!   loss, or storage that acknowledges an fsync it did not perform.
//!
//! ## Failure windows
//!
//! | Window | `consume` returns | Persisted state | The same authorization again |
//! |---|---|---|---|
//! | read or insert fails before applying | `Unavailable` | unchanged (the `Store` contract) | admitted normally |
//! | insert applied, barrier fails | `CommitUncertain` | the record may or may not survive a restart | refused while the record is visible; never overwritten |
//! | barrier ok, the caller never uses the admission | `Ok(None)` | persisted | refused as a replay: the operation is stranded |
//! | an existing record is invalid | `Corrupt` | unchanged | refused until an operator repairs it |
//!
//! No window acknowledges twice: the only path to `Ok(None)` is an applied insert on an absent
//! key followed by a successful barrier, and a key is never written over. `CommitUncertain` is
//! not rolled back — a write that may have persisted must not be erased — so an uncertain
//! commit can strand an operation; [`DurableConsumedSet::inspect`] tells an operator what is
//! stored.

use std::path::Path;

use icn_authz::effect_v0::{ConsumedEntry, ConsumedSet, ConsumedSetError, ContextRef};
use icn_store::{CompareAndPut, SledStore, Store};

mod reservation;

pub use reservation::{
    pool_key, reservation_key, DurabilityBarrier, LocalReservationPolicyV0,
    LocalResourceReservations, ReservationError, ReservationReconciliation, ReservationRecordV0,
    ReservationRequestV0, ResourcePoolV0, SledDurabilityBarrier, POOL_KEY_PREFIX,
    RESERVATION_ACTION, RESERVATION_KEY_PREFIX,
};

/// Key prefix. A key is `KEY_PREFIX || context (32 bytes) || operation_id (32 bytes)`.
///
/// Every key has the same length, so two different `(context, operation_id)` pairs never share a
/// key and no key is a prefix of another.
pub const KEY_PREFIX: &[u8] = b"icn.authz-store.v0.consumed/";

/// The persistent key for `(context, operation_id)`.
pub fn consumed_key(context: ContextRef, operation_id: [u8; 32]) -> Vec<u8> {
    let mut key = Vec::with_capacity(KEY_PREFIX.len() + 64);
    key.extend_from_slice(KEY_PREFIX);
    key.extend_from_slice(&context.0);
    key.extend_from_slice(&operation_id);
    key
}

/// A storage backend that can make an insert atomic and durable.
///
/// Implement it only if both hold:
///
/// - [`insert_if_absent`](Self::insert_if_absent) compares and writes as one atomic step and
///   **never overwrites** an existing value; an `Err` means nothing was written;
/// - [`sync`](Self::sync) returning `Ok` means every write applied before the call survives a
///   crash of this process and a reopen.
///
/// The trait exists so that durability is a stated capability rather than inferred from
/// [`Store::flush`], whose default is a no-op.
pub trait DurableBackend: Send + Sync {
    /// The value at `key`, if any.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, String>;
    /// Write `value` at `key` only if `key` is absent, atomically.
    fn insert_if_absent(&self, key: &[u8], value: &[u8]) -> Result<CompareAndPut, String>;
    /// The durability barrier.
    fn sync(&self) -> Result<(), String>;
}

impl DurableBackend for SledStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
        Store::get(self, key).map_err(|e| e.to_string())
    }

    /// sled `compare_and_swap(key, None, Some(value))`: matches only an absent key.
    fn insert_if_absent(&self, key: &[u8], value: &[u8]) -> Result<CompareAndPut, String> {
        Store::compare_and_put(self, key, None, value).map_err(|e| e.to_string())
    }

    /// sled `flush`: writes and fsyncs its log.
    fn sync(&self) -> Result<(), String> {
        SledStore::flush(self)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// A [`ConsumedSet`] that persists each consumption before acknowledging it.
///
/// Legacy and checkpointed consumptions share one `(context, operation_id)` key space, exactly as
/// in [`icn_authz::effect_v0::InMemoryConsumedSet`].
#[derive(Debug)]
pub struct DurableConsumedSet<B: DurableBackend = SledStore> {
    backend: B,
}

impl DurableConsumedSet<SledStore> {
    /// Open (or create) a sled database at `path`. sled holds an exclusive lock on the directory
    /// while it is open, so one target process owns one set.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ConsumedSetError> {
        SledStore::open(path)
            .map(Self::with_backend)
            .map_err(|e| ConsumedSetError::Unavailable(e.to_string()))
    }
}

impl<B: DurableBackend> DurableConsumedSet<B> {
    /// A set over `backend`. See [`DurableBackend`] for what the backend must guarantee.
    pub fn with_backend(backend: B) -> Self {
        DurableConsumedSet { backend }
    }

    /// The backend.
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// The validated record for `(context, operation_id)`, if one is stored. Read-only; an
    /// operator's view after a `CommitUncertain` or a restart.
    pub fn inspect(
        &self,
        context: ContextRef,
        operation_id: [u8; 32],
    ) -> Result<Option<ConsumedEntry>, ConsumedSetError> {
        self.backend
            .get(&consumed_key(context, operation_id))
            .map_err(ConsumedSetError::Unavailable)?
            .map(|bytes| decode_record(context, operation_id, &bytes))
            .transpose()
    }

    /// [`ConsumedSet::consume`] through a shared reference: the backend serializes inserts.
    pub fn consume_shared(
        &self,
        context: ContextRef,
        operation_id: [u8; 32],
        entry: ConsumedEntry,
    ) -> Result<Option<ConsumedEntry>, ConsumedSetError> {
        // Refuse, before any write, an entry that could never be read back as valid.
        if entry.authorization.context() != context
            || entry.authorization.operation_id() != operation_id
        {
            return Err(ConsumedSetError::InvalidEntry(
                "entry does not name the key it is consumed under".into(),
            ));
        }
        if !entry.is_authentic() {
            return Err(ConsumedSetError::InvalidEntry(
                "entry signature does not verify under its writer".into(),
            ));
        }
        // Exactly what will be read back: an entry whose encoding does not decode to itself (for
        // example a statement over the codec's bounds) would be persisted and then refused as
        // `Corrupt` forever, stranding the operation.
        let record = entry.encode();
        if ConsumedEntry::decode(&record).as_ref() != Ok(&entry) {
            return Err(ConsumedSetError::InvalidEntry(
                "entry does not survive its own encoding".into(),
            ));
        }

        let key = consumed_key(context, operation_id);
        match self
            .backend
            .insert_if_absent(&key, &record)
            .map_err(ConsumedSetError::Unavailable)?
        {
            CompareAndPut::Applied => {
                self.backend
                    .sync()
                    .map_err(ConsumedSetError::CommitUncertain)?;
                Ok(None)
            }
            CompareAndPut::Conflict {
                current: Some(existing),
            } => decode_record(context, operation_id, &existing).map(Some),
            CompareAndPut::Conflict { current: None } => Err(ConsumedSetError::Corrupt(
                "backend reported a conflict on an absent key".into(),
            )),
        }
    }
}

/// Strict read of a stored record: canonical decoding (every length is bounded by the codec, so
/// no record decodes past them), the key it is stored under, and the writer's signature over the
/// statement. Anything else is `Corrupt`.
fn decode_record(
    context: ContextRef,
    operation_id: [u8; 32],
    bytes: &[u8],
) -> Result<ConsumedEntry, ConsumedSetError> {
    let entry =
        ConsumedEntry::decode(bytes).map_err(|e| ConsumedSetError::Corrupt(e.to_string()))?;
    if entry.authorization.context() != context
        || entry.authorization.operation_id() != operation_id
    {
        return Err(ConsumedSetError::Corrupt(
            "record names a different (context, operation_id) than its key".into(),
        ));
    }
    if !entry.is_authentic() {
        return Err(ConsumedSetError::Corrupt(
            "record signature does not verify under its writer".into(),
        ));
    }
    Ok(entry)
}

impl<B: DurableBackend> ConsumedSet for DurableConsumedSet<B> {
    fn consume(
        &mut self,
        context: ContextRef,
        operation_id: [u8; 32],
        entry: ConsumedEntry,
    ) -> Result<Option<ConsumedEntry>, ConsumedSetError> {
        self.consume_shared(context, operation_id, entry)
    }
}

impl<B: DurableBackend> ConsumedSet for &DurableConsumedSet<B> {
    fn consume(
        &mut self,
        context: ContextRef,
        operation_id: [u8; 32],
        entry: ConsumedEntry,
    ) -> Result<Option<ConsumedEntry>, ConsumedSetError> {
        self.consume_shared(context, operation_id, entry)
    }
}
