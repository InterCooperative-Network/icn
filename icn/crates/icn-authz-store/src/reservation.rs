//! Strict, versioned local resource-reservation records.
//!
//! These records carry only generic capacity constraints and opaque hashes. Institution-specific
//! allocation meaning stays in the governance rule and the custodian's policy artifact.

use std::any::Any;
use std::path::Path;
use std::sync::Arc;

use icn_authz::effect_v0::{
    preflight_checkpointed, AdmissionRefusal, CheckpointedEvidenceV0, CheckpointedPreflight,
    ConsumedAuthorization, ConsumedEntry, ContextRef, EffectV0, RelyingFrontierV0, RuleResolver,
};
use icn_store::{CompareAndPut, SledStore, Store, TransactionalStore};
use sha2::{Digest, Sha256};

use crate::admission::{
    AdmissionGradeV0, AdmissionPolicyRefV0, AdmissionRegistryV0, AdmissionRequirementV1,
    AdmittedUnder,
};

const VERSION: u16 = 0;
/// Version of the experimental V1 pool record. V0 records keep `VERSION`.
const POOL_V1_VERSION: u16 = 1;
/// Distinct from `POOL_RECORD_DOMAIN`: a V0-only decoder refuses a V1 record (domain mismatch)
/// instead of reading it as a generic pool.
const POOL_RECORD_V1_DOMAIN: &[u8] = b"icn.authz-store.v1.resource-pool";
const REQUIREMENT_CALLER_RESOLVED: u8 = 1;
const REQUIREMENT_POLICY: u8 = 2;
const MAX_UNIT_LEN: usize = 64;
const REQUEST_DOMAIN: &[u8] = b"icn.authz-store.v0.reservation-request";
const POOL_RECORD_DOMAIN: &[u8] = b"icn.authz-store.v0.resource-pool";
const RESERVATION_RECORD_DOMAIN: &[u8] = b"icn.authz-store.v0.reservation";

/// Action label an authorization must carry for a local resource reservation.
pub const RESERVATION_ACTION: &str = "resource.reserve.v0";
/// Pool key: prefix, context, custodian target, pool id.
pub const POOL_KEY_PREFIX: &[u8] = b"icn.authz-store.v0.pool/";
/// Reservation key: prefix, context, operation id.
pub const RESERVATION_KEY_PREFIX: &[u8] = b"icn.authz-store.v0.reservation/";

/// Typed failures at the reservation boundary.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReservationError {
    /// Independently verified authorization was refused.
    #[error("authorization refused: {0}")]
    Authorization(AdmissionRefusal),
    /// Caller-supplied request or pool state is invalid.
    #[error("invalid reservation input: {0}")]
    InvalidInput(String),
    /// Stored state is malformed or internally inconsistent.
    #[error("corrupt reservation state: {0}")]
    Corrupt(String),
    /// No local pool exists under the requested identity.
    #[error("resource pool not found")]
    PoolNotFound,
    /// Pool initialization tried to overwrite an existing pool. `existing` is the stored record,
    /// so a custodian can compare it with what it meant to create instead of treating the
    /// conflict as idempotent success.
    #[error("resource pool already exists")]
    PoolAlreadyExists { existing: Box<ResourcePoolRecord> },
    /// The presented [`PoolAdminV0`] was not issued by this store instance, or has been revoked
    /// by a later [`LocalResourceReservations::with_pool_admin`].
    #[error("pool administration capability is not valid for this store")]
    PoolAdminMismatch,
    /// The request is not permitted by the custodian's local pool policy.
    #[error("local reservation policy denied the request: {0}")]
    PolicyDenied(String),
    /// The authorization names an older or future pool revision.
    #[error("stale pool revision: expected {expected}, actual {actual}")]
    StalePool { expected: u64, actual: u64 },
    /// The finite pool cannot satisfy the request.
    #[error("insufficient capacity: requested {requested}, available {available}")]
    Insufficient { requested: u64, available: u64 },
    /// Checked capacity or revision arithmetic could not be represented.
    #[error("reservation arithmetic overflow: {0}")]
    ArithmeticOverflow(String),
    /// The atomic commit returned, but the durability barrier failed.
    #[error("reservation commit durability is uncertain: {0}")]
    CommitUncertain(String),
    /// Storage could not complete an operation before acknowledgement.
    #[error("reservation storage unavailable: {0}")]
    Unavailable(String),
    /// The admission does not satisfy the pool's authoritative admission requirement.
    #[error("pool admission requirement not satisfied: {0}")]
    AdmissionDenied(String),
    /// The pool requires an admission policy this custodian has no verifier for.
    #[error("required admission policy is unavailable: {0}")]
    AdmissionUnavailable(String),
    /// The custodian-installed verifier refused the evidence.
    #[error("custodian admission verifier refused: {0}")]
    AdmissionRefused(String),
}

/// The persistent key for a custodian-owned pool.
pub fn pool_key(context: ContextRef, custodian: [u8; 32], pool_id: [u8; 32]) -> Vec<u8> {
    let mut key = Vec::with_capacity(POOL_KEY_PREFIX.len() + 96);
    key.extend_from_slice(POOL_KEY_PREFIX);
    key.extend_from_slice(&context.0);
    key.extend_from_slice(&custodian);
    key.extend_from_slice(&pool_id);
    key
}

/// The persistent key for an operation's reservation record.
pub fn reservation_key(context: ContextRef, operation_id: [u8; 32]) -> Vec<u8> {
    let mut key = Vec::with_capacity(RESERVATION_KEY_PREFIX.len() + 64);
    key.extend_from_slice(RESERVATION_KEY_PREFIX);
    key.extend_from_slice(&context.0);
    key.extend_from_slice(&operation_id);
    key
}

/// Opaque custodian policy identity plus generic local hard limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalReservationPolicyV0 {
    /// A disabled pool refuses every new reservation.
    pub enabled: bool,
    /// Per-operation ceiling chosen by the custodian.
    pub max_quantity_per_reservation: u64,
    /// Hash of the custodian-controlled policy artifact the authorization relied on.
    pub policy_hash: [u8; 32],
}

/// Exact parameters authorized for one reservation attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationRequestV0 {
    pub context: ContextRef,
    pub operation_id: [u8; 32],
    pub pool_id: [u8; 32],
    pub expected_pool_revision: u64,
    pub quantity: u64,
    pub unit: String,
    /// Opaque hash of request-specific parameters interpreted outside the substrate.
    pub request_params_hash: [u8; 32],
    /// The custodian policy artifact this request expects.
    pub policy_hash: [u8; 32],
}

impl ReservationRequestV0 {
    /// Validate input bounds before hashing or storage access.
    pub fn validate(&self) -> Result<(), ReservationError> {
        validate_quantity_unit(self.quantity, &self.unit)
    }

    /// Hash every request field under a versioned domain.
    pub fn commitment(&self) -> Result<[u8; 32], ReservationError> {
        self.validate()?;
        let mut w = Writer::new(REQUEST_DOMAIN);
        w.b32(&self.context.0);
        w.b32(&self.operation_id);
        w.b32(&self.pool_id);
        w.u64(self.expected_pool_revision);
        w.u64(self.quantity);
        w.lp(self.unit.as_bytes())?;
        w.b32(&self.request_params_hash);
        w.b32(&self.policy_hash);
        Ok(Sha256::digest(w.finish()).into())
    }

    /// The exact `EffectV0` an institutional authorization must contain.
    pub fn effect(&self, target: [u8; 32]) -> Result<EffectV0, ReservationError> {
        Ok(EffectV0 {
            target,
            action: RESERVATION_ACTION.to_string(),
            params_hash: self.commitment()?,
        })
    }
}

/// Durable finite capacity owned by one local custodian.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePoolV0 {
    pub context: ContextRef,
    pub pool_id: [u8; 32],
    pub custodian: [u8; 32],
    pub unit: String,
    pub total_capacity: u64,
    pub available_capacity: u64,
    pub revision: u64,
    pub policy: LocalReservationPolicyV0,
}

impl ResourcePoolV0 {
    pub fn validate(&self) -> Result<(), ReservationError> {
        validate_unit(&self.unit)?;
        if self.total_capacity == 0 {
            return Err(ReservationError::InvalidInput(
                "pool total capacity must be positive".into(),
            ));
        }
        if self.available_capacity > self.total_capacity {
            return Err(ReservationError::InvalidInput(
                "available capacity exceeds total capacity".into(),
            ));
        }
        if self.policy.max_quantity_per_reservation == 0
            || self.policy.max_quantity_per_reservation > self.total_capacity
        {
            return Err(ReservationError::InvalidInput(
                "policy maximum must be within pool capacity".into(),
            ));
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, ReservationError> {
        self.validate()?;
        let mut w = Writer::new(POOL_RECORD_DOMAIN);
        w.b32(&self.context.0);
        w.b32(&self.pool_id);
        w.b32(&self.custodian);
        w.lp(self.unit.as_bytes())?;
        w.u64(self.total_capacity);
        w.u64(self.available_capacity);
        w.u64(self.revision);
        w.u8(u8::from(self.policy.enabled));
        w.u64(self.policy.max_quantity_per_reservation);
        w.b32(&self.policy.policy_hash);
        Ok(w.finish())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ReservationError> {
        let mut r = Reader::new(bytes, POOL_RECORD_DOMAIN)?;
        let value = ResourcePoolV0 {
            context: ContextRef(r.b32("context")?),
            pool_id: r.b32("pool_id")?,
            custodian: r.b32("custodian")?,
            unit: r.text("unit")?,
            total_capacity: r.u64("total_capacity")?,
            available_capacity: r.u64("available_capacity")?,
            revision: r.u64("revision")?,
            policy: LocalReservationPolicyV0 {
                enabled: r.boolean("policy_enabled")?,
                max_quantity_per_reservation: r.u64("policy_maximum")?,
                policy_hash: r.b32("policy_hash")?,
            },
        };
        r.finish()?;
        value.validate().map_err(as_corrupt)?;
        Ok(value)
    }
}

/// EXPERIMENTAL V1 pool record: a V0 pool plus the custodian's durable admission requirement.
///
/// Stored under the same key as a V0 pool, so one pool has exactly one record. Not a ratified
/// format: it awaits review by the reservation lane owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePoolV1 {
    pub pool: ResourcePoolV0,
    pub admission: AdmissionRequirementV1,
}

impl ResourcePoolV1 {
    pub fn validate(&self) -> Result<(), ReservationError> {
        self.pool.validate()
    }

    pub fn encode(&self) -> Result<Vec<u8>, ReservationError> {
        self.validate()?;
        let p = &self.pool;
        let mut w = Writer::with_version(POOL_RECORD_V1_DOMAIN, POOL_V1_VERSION);
        w.b32(&p.context.0);
        w.b32(&p.pool_id);
        w.b32(&p.custodian);
        w.lp(p.unit.as_bytes())?;
        w.u64(p.total_capacity);
        w.u64(p.available_capacity);
        w.u64(p.revision);
        w.u8(u8::from(p.policy.enabled));
        w.u64(p.policy.max_quantity_per_reservation);
        w.b32(&p.policy.policy_hash);
        match self.admission {
            AdmissionRequirementV1::CallerResolved => w.u8(REQUIREMENT_CALLER_RESOLVED),
            AdmissionRequirementV1::Policy { policy, min_grade } => {
                w.u8(REQUIREMENT_POLICY);
                w.b32(&policy.id);
                w.u32(policy.version);
                w.u8(min_grade.tag());
            }
        }
        Ok(w.finish())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ReservationError> {
        let mut r = Reader::with_version(bytes, POOL_RECORD_V1_DOMAIN, POOL_V1_VERSION)?;
        let pool = ResourcePoolV0 {
            context: ContextRef(r.b32("context")?),
            pool_id: r.b32("pool_id")?,
            custodian: r.b32("custodian")?,
            unit: r.text("unit")?,
            total_capacity: r.u64("total_capacity")?,
            available_capacity: r.u64("available_capacity")?,
            revision: r.u64("revision")?,
            policy: LocalReservationPolicyV0 {
                enabled: r.boolean("policy_enabled")?,
                max_quantity_per_reservation: r.u64("policy_maximum")?,
                policy_hash: r.b32("policy_hash")?,
            },
        };
        // Unknown requirement tags and grades are corruption, never a default.
        let admission = match r.u8("admission_requirement")? {
            REQUIREMENT_CALLER_RESOLVED => AdmissionRequirementV1::CallerResolved,
            REQUIREMENT_POLICY => {
                let id = r.b32("admission_policy_id")?;
                let version = r.u32("admission_policy_version")?;
                let min_grade = AdmissionGradeV0::from_tag(r.u8("admission_min_grade")?)
                    .ok_or_else(|| ReservationError::Corrupt("unknown admission grade".into()))?;
                AdmissionRequirementV1::Policy {
                    policy: AdmissionPolicyRefV0 { id, version },
                    min_grade,
                }
            }
            _ => {
                return Err(ReservationError::Corrupt(
                    "unknown admission requirement".into(),
                ))
            }
        };
        r.finish()?;
        let value = ResourcePoolV1 { pool, admission };
        value.validate().map_err(as_corrupt)?;
        Ok(value)
    }
}

/// Whichever pool record version is stored under a pool key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourcePoolRecord {
    V0(ResourcePoolV0),
    V1(ResourcePoolV1),
}

impl ResourcePoolRecord {
    /// Dispatch on the record's own domain tag. Anything else is corruption.
    pub fn decode(bytes: &[u8]) -> Result<Self, ReservationError> {
        let mut peek = Reader { bytes, pos: 0 };
        let domain = peek.lp("domain")?;
        if domain == POOL_RECORD_DOMAIN {
            ResourcePoolV0::decode(bytes).map(ResourcePoolRecord::V0)
        } else if domain == POOL_RECORD_V1_DOMAIN {
            ResourcePoolV1::decode(bytes).map(ResourcePoolRecord::V1)
        } else {
            Err(ReservationError::Corrupt("record domain mismatch".into()))
        }
    }

    /// Re-encode in the record's own version, so a commit never changes a pool's version.
    pub fn encode(&self) -> Result<Vec<u8>, ReservationError> {
        match self {
            ResourcePoolRecord::V0(pool) => pool.encode(),
            ResourcePoolRecord::V1(pool) => pool.encode(),
        }
    }

    pub fn pool(&self) -> &ResourcePoolV0 {
        match self {
            ResourcePoolRecord::V0(pool) => pool,
            ResourcePoolRecord::V1(pool) => &pool.pool,
        }
    }

    fn pool_mut(&mut self) -> &mut ResourcePoolV0 {
        match self {
            ResourcePoolRecord::V0(pool) => pool,
            ResourcePoolRecord::V1(pool) => &mut pool.pool,
        }
    }

    /// A V0 record predates admission requirements; its existing contract is caller-resolved.
    pub fn admission(&self) -> AdmissionRequirementV1 {
        match self {
            ResourcePoolRecord::V0(_) => AdmissionRequirementV1::CallerResolved,
            ResourcePoolRecord::V1(pool) => pool.admission,
        }
    }
}

/// Complete immutable witness of one local capacity reservation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationRecordV0 {
    pub context: ContextRef,
    pub operation_id: [u8; 32],
    pub pool_id: [u8; 32],
    pub custodian: [u8; 32],
    pub quantity: u64,
    pub unit: String,
    pub request_params_hash: [u8; 32],
    pub policy_hash: [u8; 32],
    pub authorization_hash: [u8; 32],
    pub reserved_at: u64,
    pub pool_revision_before: u64,
    pub pool_revision_after: u64,
    pub available_before: u64,
    pub available_after: u64,
}

impl ReservationRecordV0 {
    pub fn validate(&self) -> Result<(), ReservationError> {
        validate_quantity_unit(self.quantity, &self.unit)?;
        if self.pool_revision_before.checked_add(1) != Some(self.pool_revision_after) {
            return Err(ReservationError::InvalidInput(
                "reservation revision transition is invalid".into(),
            ));
        }
        if self.available_before.checked_sub(self.quantity) != Some(self.available_after) {
            return Err(ReservationError::InvalidInput(
                "reservation capacity transition is invalid".into(),
            ));
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, ReservationError> {
        self.validate()?;
        let mut w = Writer::new(RESERVATION_RECORD_DOMAIN);
        w.b32(&self.context.0);
        w.b32(&self.operation_id);
        w.b32(&self.pool_id);
        w.b32(&self.custodian);
        w.u64(self.quantity);
        w.lp(self.unit.as_bytes())?;
        w.b32(&self.request_params_hash);
        w.b32(&self.policy_hash);
        w.b32(&self.authorization_hash);
        w.u64(self.reserved_at);
        w.u64(self.pool_revision_before);
        w.u64(self.pool_revision_after);
        w.u64(self.available_before);
        w.u64(self.available_after);
        Ok(w.finish())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ReservationError> {
        let mut r = Reader::new(bytes, RESERVATION_RECORD_DOMAIN)?;
        let value = ReservationRecordV0 {
            context: ContextRef(r.b32("context")?),
            operation_id: r.b32("operation_id")?,
            pool_id: r.b32("pool_id")?,
            custodian: r.b32("custodian")?,
            quantity: r.u64("quantity")?,
            unit: r.text("unit")?,
            request_params_hash: r.b32("request_params_hash")?,
            policy_hash: r.b32("policy_hash")?,
            authorization_hash: r.b32("authorization_hash")?,
            reserved_at: r.u64("reserved_at")?,
            pool_revision_before: r.u64("pool_revision_before")?,
            pool_revision_after: r.u64("pool_revision_after")?,
            available_before: r.u64("available_before")?,
            available_after: r.u64("available_after")?,
        };
        r.finish()?;
        value.validate().map_err(as_corrupt)?;
        Ok(value)
    }
}

/// A durability capability used after the atomic transaction returns.
pub trait DurabilityBarrier: Send + Sync {
    fn sync(&self, store: &SledStore) -> Result<(), String>;
}

/// The production barrier: sled flushes and fsyncs its log.
#[derive(Debug, Clone, Copy, Default)]
pub struct SledDurabilityBarrier;

impl DurabilityBarrier for SledDurabilityBarrier {
    fn sync(&self, store: &SledStore) -> Result<(), String> {
        store.flush().map(|_| ()).map_err(|error| error.to_string())
    }
}

/// Read-only result after a restart or `CommitUncertain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReservationReconciliation {
    Absent,
    /// A prior standalone/legacy admission consumed this operation without a reservation.
    ConsumedOnly(Box<ConsumedEntry>),
    Committed {
        consumed: Box<ConsumedEntry>,
        reservation: Box<ReservationRecordV0>,
    },
}

/// Authority to create pools in ONE opened [`LocalResourceReservations`] instance (design-spec
/// Part 8 F2, experimental).
///
/// Pool administration is separate from reservation authority. Every reservation caller holds
/// `&LocalResourceReservations`; only the holder of this value can create pools.
///
/// * **Issued only at the custodian construction boundary.**
///   [`LocalResourceReservations::with_pool_admin`] takes the store by value, so a holder of
///   `&LocalResourceReservations` cannot obtain one. The field is private, and the type is
///   neither `Clone` nor `Default`, so it cannot be built or duplicated outside this crate:
///
///   ```compile_fail
///   fn forge() -> icn_authz_store::PoolAdminV0 {
///       icn_authz_store::PoolAdminV0 { binding: unimplemented!() }
///   }
///   ```
///
///   ```compile_fail
///   fn duplicate(admin: icn_authz_store::PoolAdminV0) -> (icn_authz_store::PoolAdminV0, icn_authz_store::PoolAdminV0) {
///       (admin.clone(), admin)
///   }
///   ```
///
///   while obtaining one from the owned store compiles:
///
///   ```
///   fn obtain(
///       store: icn_authz_store::LocalResourceReservations,
///   ) -> (icn_authz_store::LocalResourceReservations, icn_authz_store::PoolAdminV0) {
///       store.with_pool_admin()
///   }
///   ```
/// * **Bound to the issuing instance.** Presented to any other store instance, or after a later
///   `with_pool_admin` call has revoked it, creation is refused with
///   [`ReservationError::PoolAdminMismatch`] before anything is read or written. A store is bound
///   to one custodian `target`, and pool keys are `(context, target, pool_id)`, so the capability
///   covers exactly that custodian's pool namespace in that open store.
/// * **Per opened instance, not per database.** It is not persisted and does not survive
///   reopening the store: a reopened instance has no valid capability until its owner issues one.
///   Two instances over the same database (possible only for code holding the storage itself, see
///   below) each issue their own capabilities.
///
/// # What this does not protect against
///
/// * **Process trust.** This is an in-process Rust capability, not a credential. Any code that
///   holds the store **by value** (before it is shared), or that can `open` its database path once
///   the custodian has released it, is the custodian and can issue a capability. Nothing here
///   authenticates across a process boundary.
/// * **Raw storage.** Code with direct access to the underlying database (an owned `SledStore`,
///   its `db()` handle, or the files on disk) can write or delete pool records without any
///   capability. The gate stops reservation callers holding `&LocalResourceReservations`; it does
///   not protect against an adversary who already owns the storage.
pub struct PoolAdminV0 {
    binding: Arc<PoolAdminBinding>,
}

/// The identity a [`PoolAdminV0`] shares with the store that issued it (compared by allocation).
struct PoolAdminBinding;

impl std::fmt::Debug for PoolAdminV0 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PoolAdminV0 { .. }")
    }
}

/// One local custodian's atomic finite-capacity reservation store.
pub struct LocalResourceReservations<B: DurabilityBarrier = SledDurabilityBarrier> {
    store: SledStore,
    target: [u8; 32],
    barrier: B,
    /// Custodian-installed verifiers. Set only while the custodian owns the store by value.
    admission: AdmissionRegistryV0,
    /// The binding of the one valid [`PoolAdminV0`], if one has been issued. Set only while the
    /// custodian owns the store by value.
    pool_admin: Option<Arc<PoolAdminBinding>>,
}

impl LocalResourceReservations<SledDurabilityBarrier> {
    pub fn open(path: impl AsRef<Path>, target: [u8; 32]) -> Result<Self, ReservationError> {
        let store = SledStore::open(path)
            .map_err(|error| ReservationError::Unavailable(error.to_string()))?;
        Ok(Self::with_barrier(store, target, SledDurabilityBarrier))
    }
}

impl<B: DurabilityBarrier> LocalResourceReservations<B> {
    pub fn with_barrier(store: SledStore, target: [u8; 32], barrier: B) -> Self {
        LocalResourceReservations {
            store,
            target,
            barrier,
            admission: AdmissionRegistryV0::new(),
            pool_admin: None,
        }
    }

    /// Issue this store instance's pool-administration capability ([`PoolAdminV0`]).
    ///
    /// Takes the store by value, like [`Self::with_admission`]: code that only holds
    /// `&LocalResourceReservations` (every reservation caller) cannot call it.
    ///
    /// ```compile_fail
    /// fn escalate(store: &icn_authz_store::LocalResourceReservations) {
    ///     let (_store, _admin) = store.with_pool_admin();
    /// }
    /// ```
    ///
    /// ```
    /// fn issue(store: icn_authz_store::LocalResourceReservations) {
    ///     let (_store, _admin) = store.with_pool_admin();
    /// }
    /// ```
    ///
    /// At most one capability is valid at a time. Issuing again revokes the previous one: from
    /// then on it is refused with [`ReservationError::PoolAdminMismatch`].
    pub fn with_pool_admin(mut self) -> (Self, PoolAdminV0) {
        let binding = Arc::new(PoolAdminBinding);
        self.pool_admin = Some(Arc::clone(&binding));
        (self, PoolAdminV0 { binding })
    }

    /// Refuse unless `admin` is this instance's current capability. Checked before any read,
    /// validation or write.
    fn authorize_pool_admin(&self, admin: &PoolAdminV0) -> Result<(), ReservationError> {
        match &self.pool_admin {
            Some(current) if Arc::ptr_eq(current, &admin.binding) => Ok(()),
            _ => Err(ReservationError::PoolAdminMismatch),
        }
    }

    /// Install the custodian's admission verifiers and clock
    /// ([`AdmissionRegistryV0::install_clock`]). Takes the store by value: code that only holds
    /// `&LocalResourceReservations` (every reservation caller) cannot add or swap either.
    /// Without a verifier for a pool's declared policy, that pool refuses every reservation;
    /// without a clock, [`Self::reserve_admitted`] refuses every reservation.
    ///
    /// Call this once, with the complete registry: a second call replaces the whole registry,
    /// verifiers and clock alike, rather than merging into or refusing over the first.
    pub fn with_admission(mut self, registry: AdmissionRegistryV0) -> Self {
        self.admission = registry;
        self
    }

    pub fn target(&self) -> [u8; 32] {
        self.target
    }

    /// Create a new, full V0 pool (caller-resolved admission) without overwriting any record.
    /// Requires this instance's [`PoolAdminV0`]; a reservation caller holding only `&self` has
    /// none, and there is no other public way to create a pool:
    ///
    /// ```compile_fail
    /// fn provision(
    ///     store: &icn_authz_store::LocalResourceReservations,
    ///     pool: icn_authz_store::ResourcePoolV0,
    /// ) {
    ///     let _ = store.initialize_pool(pool);
    /// }
    /// ```
    ///
    /// ```
    /// fn provision(
    ///     store: &icn_authz_store::LocalResourceReservations,
    ///     admin: &icn_authz_store::PoolAdminV0,
    ///     pool: icn_authz_store::ResourcePoolV0,
    /// ) {
    ///     let _ = store.initialize_pool(admin, pool);
    /// }
    /// ```
    ///
    /// An existing pool is never overwritten: the conflict returns
    /// [`ReservationError::PoolAlreadyExists`] carrying the stored record.
    pub fn initialize_pool(
        &self,
        admin: &PoolAdminV0,
        pool: ResourcePoolV0,
    ) -> Result<(), ReservationError> {
        self.authorize_pool_admin(admin)?;
        self.initialize_record(ResourcePoolRecord::V0(pool))
    }

    /// Create a new, full EXPERIMENTAL V1 pool that durably declares its admission requirement.
    /// Requires this instance's [`PoolAdminV0`], exactly as [`Self::initialize_pool`] does.
    pub fn initialize_pool_v1(
        &self,
        admin: &PoolAdminV0,
        pool: ResourcePoolV1,
    ) -> Result<(), ReservationError> {
        self.authorize_pool_admin(admin)?;
        self.initialize_record(ResourcePoolRecord::V1(pool))
    }

    fn initialize_record(&self, record: ResourcePoolRecord) -> Result<(), ReservationError> {
        let pool = record.pool();
        pool.validate()?;
        if pool.custodian != self.target {
            return Err(ReservationError::InvalidInput(
                "pool custodian is not this local target".into(),
            ));
        }
        if pool.revision != 0 || pool.available_capacity != pool.total_capacity {
            return Err(ReservationError::InvalidInput(
                "new pool must start at revision zero with full capacity".into(),
            ));
        }
        let key = pool_key(pool.context, self.target, pool.pool_id);
        let encoded = record.encode()?;
        match Store::compare_and_put(&self.store, &key, None, &encoded)
            .map_err(|error| ReservationError::Unavailable(error.to_string()))?
        {
            CompareAndPut::Applied => self
                .barrier
                .sync(&self.store)
                .map_err(ReservationError::CommitUncertain),
            CompareAndPut::Conflict {
                current: Some(existing),
            } => {
                let existing =
                    decode_pool_for_key(&existing, pool.context, self.target, pool.pool_id)?;
                Err(ReservationError::PoolAlreadyExists {
                    existing: Box::new(existing),
                })
            }
            CompareAndPut::Conflict { current: None } => Err(ReservationError::Corrupt(
                "backend reported a pool conflict on an absent key".into(),
            )),
        }
    }

    /// The capacity fields of a pool of either record version.
    pub fn inspect_pool(
        &self,
        context: ContextRef,
        pool_id: [u8; 32],
    ) -> Result<Option<ResourcePoolV0>, ReservationError> {
        Ok(self
            .inspect_pool_record(context, pool_id)?
            .map(|record| record.pool().clone()))
    }

    /// The stored pool record, including its admission requirement.
    pub fn inspect_pool_record(
        &self,
        context: ContextRef,
        pool_id: [u8; 32],
    ) -> Result<Option<ResourcePoolRecord>, ReservationError> {
        Store::get(&self.store, &pool_key(context, self.target, pool_id))
            .map_err(|error| ReservationError::Unavailable(error.to_string()))?
            .map(|bytes| decode_pool_for_key(&bytes, context, self.target, pool_id))
            .transpose()
    }

    pub fn inspect_reservation(
        &self,
        context: ContextRef,
        operation_id: [u8; 32],
    ) -> Result<Option<ReservationRecordV0>, ReservationError> {
        Store::get(&self.store, &reservation_key(context, operation_id))
            .map_err(|error| ReservationError::Unavailable(error.to_string()))?
            .map(|bytes| decode_reservation_for_key(&bytes, context, operation_id, self.target))
            .transpose()
    }

    pub fn inspect_consumed(
        &self,
        context: ContextRef,
        operation_id: [u8; 32],
    ) -> Result<Option<ConsumedEntry>, ReservationError> {
        Store::get(&self.store, &super::consumed_key(context, operation_id))
            .map_err(|error| ReservationError::Unavailable(error.to_string()))?
            .map(|bytes| {
                super::decode_record(context, operation_id, &bytes)
                    .map_err(|error| ReservationError::Corrupt(error.to_string()))
            })
            .transpose()
    }

    /// Read-only reconciliation. It never allocates, deletes, or repairs.
    pub fn reconcile(
        &self,
        context: ContextRef,
        operation_id: [u8; 32],
    ) -> Result<ReservationReconciliation, ReservationError> {
        let result = reconcile_records(
            self.inspect_consumed(context, operation_id)?,
            self.inspect_reservation(context, operation_id)?,
        )?;
        // v0 pools are initialized once, and the only supported mutation reserves capacity.
        // Later reservations may advance the revision and lower availability, but never reverse
        // those changes. Do not claim a coherent reservation when its pool is missing or has
        // regressed. This is a local consistency check, not a full aggregate capacity audit.
        if let ReservationReconciliation::Committed { reservation, .. } = &result {
            let pool = self
                .inspect_pool(context, reservation.pool_id)?
                .ok_or_else(|| {
                    ReservationError::Corrupt("committed reservation has no resource pool".into())
                })?;
            if pool.revision < reservation.pool_revision_after
                || pool.available_capacity > reservation.available_after
                || pool.unit != reservation.unit
                || pool.policy.policy_hash != reservation.policy_hash
            {
                return Err(ReservationError::Corrupt(
                    "resource pool state contradicts the committed reservation".into(),
                ));
            }
        }
        Ok(result)
    }

    /// Caller-resolved admission: the caller supplies the frontier and rule resolver.
    ///
    /// Valid only for pools whose authoritative requirement is `CallerResolved` (every V0 pool,
    /// and V1 pools that declare it explicitly). A pool that declares a custodian admission
    /// policy refuses this call inside the commit transaction, whatever the caller supplies.
    ///
    /// `now` is the caller's clock (the V0 contract, unchanged): on this path the caller already
    /// chooses its own trust inputs, so the custodian clock does not apply.
    pub fn reserve_checkpointed(
        &self,
        evidence: &CheckpointedEvidenceV0,
        frontier: &RelyingFrontierV0,
        rules: &dyn RuleResolver,
        now: u64,
        request: &ReservationRequestV0,
    ) -> Result<ReservationRecordV0, ReservationError> {
        let preflight = preflight_checkpointed(evidence, frontier, rules, self.target, now)
            .map_err(ReservationError::Authorization)?;
        self.commit_preflight(preflight, AdmittedUnder::CallerResolved, now, request)
    }

    /// Custodian-policy admission. The pool's stored requirement selects the verifier; the
    /// caller supplies only untrusted evidence and an opaque supplement for that verifier.
    ///
    /// **The caller supplies no time.** The admission time is read once from the custodian's
    /// installed [`crate::CustodianClockV0`] (unix seconds), and that one reading is used for the
    /// verifier preflight, the store's own expiry re-check and the record's `reserved_at`. Without
    /// an installed clock, or when the clock reports that it cannot determine the time (`None`),
    /// this refuses with [`ReservationError::AdmissionUnavailable`] before reading the pool,
    /// calling a verifier or opening a transaction, so nothing is consumed.
    ///
    /// The single reading is taken before verification, which may be slow (log replay). An
    /// authorization that expires while it is being verified is therefore still judged, and its
    /// `reserved_at` stamped, at the earlier reading. That is the accepted trade-off for one
    /// consistent time per call.
    ///
    /// The clock's guarantees are the clock's (see [`crate::CustodianClockV0`]): this removes the
    /// caller's control of admission time, not the host's.
    ///
    /// A caller holding `&LocalResourceReservations` reserves with evidence, a supplement and a
    /// request only (this compiles):
    ///
    /// ```
    /// use std::any::Any;
    /// use icn_authz::effect_v0::CheckpointedEvidenceV0;
    /// use icn_authz_store::{LocalResourceReservations, ReservationRequestV0};
    /// fn reserve(
    ///     store: &LocalResourceReservations,
    ///     evidence: &CheckpointedEvidenceV0,
    ///     supplement: &dyn Any,
    ///     request: &ReservationRequestV0,
    /// ) {
    ///     let _ = store.reserve_admitted(evidence, supplement, request);
    /// }
    /// ```
    ///
    /// It cannot pass a time (this does not compile):
    ///
    /// ```compile_fail
    /// use std::any::Any;
    /// use icn_authz::effect_v0::CheckpointedEvidenceV0;
    /// use icn_authz_store::{LocalResourceReservations, ReservationRequestV0};
    /// fn reserve(
    ///     store: &LocalResourceReservations,
    ///     evidence: &CheckpointedEvidenceV0,
    ///     supplement: &dyn Any,
    ///     request: &ReservationRequestV0,
    /// ) {
    ///     let _ = store.reserve_admitted(evidence, supplement, 1_000, request);
    /// }
    /// ```
    ///
    /// and cannot install or replace the clock, which only the by-value owner can do (the owner
    /// compiles):
    ///
    /// ```
    /// use std::sync::Arc;
    /// use icn_authz_store::{AdmissionRegistryV0, CustodianClockV0, LocalResourceReservations};
    /// fn build(store: LocalResourceReservations, clock: Arc<dyn CustodianClockV0>) {
    ///     let mut registry = AdmissionRegistryV0::new();
    ///     registry.install_clock(clock).unwrap();
    ///     let _custodian = store.with_admission(registry);
    /// }
    /// ```
    ///
    /// while a borrower does not:
    ///
    /// ```compile_fail
    /// use std::sync::Arc;
    /// use icn_authz_store::{AdmissionRegistryV0, CustodianClockV0, LocalResourceReservations};
    /// fn build(store: &LocalResourceReservations, clock: Arc<dyn CustodianClockV0>) {
    ///     let mut registry = AdmissionRegistryV0::new();
    ///     registry.install_clock(clock).unwrap();
    ///     let _custodian = store.with_admission(registry);
    /// }
    /// ```
    pub fn reserve_admitted(
        &self,
        evidence: &CheckpointedEvidenceV0,
        supplement: &dyn Any,
        request: &ReservationRequestV0,
    ) -> Result<ReservationRecordV0, ReservationError> {
        // The single reading of admission time for this call (Part 8 F1).
        let now = self
            .admission
            .clock()
            .ok_or_else(|| {
                ReservationError::AdmissionUnavailable("no custodian clock is installed".into())
            })?
            .now()
            .ok_or_else(|| {
                ReservationError::AdmissionUnavailable(
                    "the custodian clock cannot determine the time".into(),
                )
            })?;
        request.validate()?;
        let record = self
            .inspect_pool_record(request.context, request.pool_id)?
            .ok_or(ReservationError::PoolNotFound)?;
        let AdmissionRequirementV1::Policy { policy, .. } = record.admission() else {
            return Err(ReservationError::AdmissionDenied(
                "pool does not declare a custodian admission policy".into(),
            ));
        };
        let verifier = self.admission.get(&policy).ok_or_else(|| {
            ReservationError::AdmissionUnavailable(
                "no installed verifier for the pool's admission policy and version".into(),
            )
        })?;
        let verified = verifier
            .preflight(evidence, supplement, self.target, now)
            .map_err(|refusal| ReservationError::AdmissionRefused(refusal.0))?;
        // The requirement read above is advisory; the binding check runs again in the commit
        // transaction against the pool record read there.
        self.commit_preflight(
            verified.preflight,
            AdmittedUnder::Policy {
                policy,
                grade: verified.grade,
            },
            now,
            request,
        )
    }

    fn commit_preflight(
        &self,
        preflight: CheckpointedPreflight,
        admitted: AdmittedUnder,
        now: u64,
        request: &ReservationRequestV0,
    ) -> Result<ReservationRecordV0, ReservationError> {
        let auth = preflight.verified().authorization();
        request.validate()?;
        // Defence in depth: on the admitted path the preflight was produced by a verifier, so the
        // store itself re-asserts the target binding and expiry rather than trusting it to.
        if preflight.target() != self.target || auth.effect.target != self.target {
            return Err(ReservationError::InvalidInput(
                "authorization was not preflighted for this local target".into(),
            ));
        }
        if now > auth.not_after {
            return Err(ReservationError::InvalidInput(
                "authorization has expired".into(),
            ));
        }
        if auth.context != request.context || auth.operation_id != request.operation_id {
            return Err(ReservationError::InvalidInput(
                "authorization does not name the request context and operation".into(),
            ));
        }
        if auth.effect.action != RESERVATION_ACTION
            || auth.effect.params_hash != request.commitment()?
        {
            return Err(ReservationError::InvalidInput(
                "authorization effect does not bind the reservation request".into(),
            ));
        }

        let binding = CommitBinding::from_preflight(&preflight, admitted);
        match self.commit_atomic(&binding, request, now, |_| Ok(()))? {
            AtomicOutcome::Reserved(record) => {
                self.barrier
                    .sync(&self.store)
                    .map_err(ReservationError::CommitUncertain)?;
                Ok(record)
            }
            AtomicOutcome::Prior(prior) => Err(ReservationError::Authorization(
                preflight.refusal_for_prior(*prior),
            )),
            AtomicOutcome::Rejected(error) => Err(error),
        }
    }

    /// The closure contains only deterministic validation, checked arithmetic and staged writes.
    /// Sled may retry it. `observe` is always a no-op in production and exists only so unit tests
    /// can abort between staged writes.
    fn commit_atomic<F>(
        &self,
        binding: &CommitBinding,
        request: &ReservationRequestV0,
        now: u64,
        observe: F,
    ) -> Result<AtomicOutcome, ReservationError>
    where
        F: Fn(CommitStage) -> anyhow::Result<()>,
    {
        let consumed_key = super::consumed_key(request.context, request.operation_id);
        let reservation_key = reservation_key(request.context, request.operation_id);
        let pool_key = pool_key(request.context, self.target, request.pool_id);
        self.store
            .transaction(|tx| {
                let consumed_bytes = tx.get(&consumed_key)?;
                let reservation_bytes = tx.get(&reservation_key)?;
                match (consumed_bytes, reservation_bytes) {
                    (Some(consumed_bytes), reservation_bytes) => {
                        let prior = match super::decode_record(
                            request.context,
                            request.operation_id,
                            &consumed_bytes,
                        ) {
                            Ok(value) => value,
                            Err(error) => {
                                return Ok(AtomicOutcome::Rejected(ReservationError::Corrupt(
                                    error.to_string(),
                                )))
                            }
                        };
                        if let Some(bytes) = reservation_bytes {
                            let record = match decode_reservation_for_key(
                                &bytes,
                                request.context,
                                request.operation_id,
                                self.target,
                            ) {
                                Ok(value) => value,
                                Err(error) => return Ok(AtomicOutcome::Rejected(error)),
                            };
                            if record.authorization_hash != hash_consumed(&prior) {
                                return Ok(AtomicOutcome::Rejected(ReservationError::Corrupt(
                                    "consumption and reservation authorization hashes differ"
                                        .into(),
                                )));
                            }
                        }
                        return Ok(AtomicOutcome::Prior(Box::new(prior)));
                    }
                    (None, Some(_)) => {
                        return Ok(AtomicOutcome::Rejected(ReservationError::Corrupt(
                            "reservation exists without operation consumption".into(),
                        )))
                    }
                    (None, None) => {}
                }

                let Some(pool_bytes) = tx.get(&pool_key)? else {
                    return Ok(AtomicOutcome::Rejected(ReservationError::PoolNotFound));
                };
                let pool_record = match decode_pool_for_key(
                    &pool_bytes,
                    request.context,
                    self.target,
                    request.pool_id,
                ) {
                    Ok(value) => value,
                    Err(error) => return Ok(AtomicOutcome::Rejected(error)),
                };
                // Authoritative admission check: the requirement in the record read by this
                // transaction, not the one observed before verification.
                if let Err(reason) = binding.admitted.satisfies(pool_record.admission()) {
                    return Ok(AtomicOutcome::Rejected(ReservationError::AdmissionDenied(
                        reason,
                    )));
                }
                let pool = pool_record.pool();
                if request.expected_pool_revision != pool.revision {
                    return Ok(AtomicOutcome::Rejected(ReservationError::StalePool {
                        expected: request.expected_pool_revision,
                        actual: pool.revision,
                    }));
                }
                if !pool.policy.enabled {
                    return Ok(AtomicOutcome::Rejected(ReservationError::PolicyDenied(
                        "pool is disabled".into(),
                    )));
                }
                if request.policy_hash != pool.policy.policy_hash {
                    return Ok(AtomicOutcome::Rejected(ReservationError::PolicyDenied(
                        "policy hash mismatch".into(),
                    )));
                }
                if request.unit != pool.unit {
                    return Ok(AtomicOutcome::Rejected(ReservationError::PolicyDenied(
                        "unit mismatch".into(),
                    )));
                }
                if request.quantity > pool.policy.max_quantity_per_reservation {
                    return Ok(AtomicOutcome::Rejected(ReservationError::PolicyDenied(
                        "quantity exceeds the per-reservation maximum".into(),
                    )));
                }
                if request.quantity > pool.available_capacity {
                    return Ok(AtomicOutcome::Rejected(ReservationError::Insufficient {
                        requested: request.quantity,
                        available: pool.available_capacity,
                    }));
                }
                let Some(available_after) = pool.available_capacity.checked_sub(request.quantity)
                else {
                    return Ok(AtomicOutcome::Rejected(
                        ReservationError::ArithmeticOverflow("capacity underflow".into()),
                    ));
                };
                let Some(revision_after) = pool.revision.checked_add(1) else {
                    return Ok(AtomicOutcome::Rejected(
                        ReservationError::ArithmeticOverflow("pool revision overflow".into()),
                    ));
                };

                let mut next_pool = pool_record.clone();
                next_pool.pool_mut().available_capacity = available_after;
                next_pool.pool_mut().revision = revision_after;
                let record = ReservationRecordV0 {
                    context: request.context,
                    operation_id: request.operation_id,
                    pool_id: request.pool_id,
                    custodian: self.target,
                    quantity: request.quantity,
                    unit: request.unit.clone(),
                    request_params_hash: request.request_params_hash,
                    policy_hash: request.policy_hash,
                    authorization_hash: binding.authorization_hash,
                    reserved_at: now,
                    pool_revision_before: pool.revision,
                    pool_revision_after: revision_after,
                    available_before: pool.available_capacity,
                    available_after,
                };
                let consumed = binding.consumed.encode();
                if ConsumedEntry::decode(&consumed).as_ref() != Ok(&binding.consumed)
                    || !binding.consumed.is_authentic()
                {
                    return Ok(AtomicOutcome::Rejected(ReservationError::InvalidInput(
                        "consumed authorization is not strictly readable and authentic".into(),
                    )));
                }
                let pool = next_pool.encode()?;
                let reservation = record.encode()?;
                tx.set(&consumed_key, &consumed)?;
                observe(CommitStage::Consumed)?;
                tx.set(&pool_key, &pool)?;
                observe(CommitStage::Capacity)?;
                tx.set(&reservation_key, &reservation)?;
                observe(CommitStage::Reservation)?;
                Ok(AtomicOutcome::Reserved(record))
            })
            .map_err(|error| ReservationError::Unavailable(error.to_string()))
    }
}

#[derive(Clone)]
struct CommitBinding {
    consumed: ConsumedEntry,
    authorization_hash: [u8; 32],
    admitted: AdmittedUnder,
}

impl CommitBinding {
    fn from_preflight(preflight: &CheckpointedPreflight, admitted: AdmittedUnder) -> Self {
        let consumed = preflight.consumed_entry();
        CommitBinding {
            authorization_hash: hash_consumed(&consumed),
            consumed,
            admitted,
        }
    }
}

enum AtomicOutcome {
    Reserved(ReservationRecordV0),
    Prior(Box<ConsumedEntry>),
    Rejected(ReservationError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitStage {
    Consumed,
    Capacity,
    Reservation,
}

fn hash_consumed(consumed: &ConsumedEntry) -> [u8; 32] {
    Sha256::digest(consumed.encode()).into()
}

fn decode_pool_for_key(
    bytes: &[u8],
    context: ContextRef,
    target: [u8; 32],
    pool_id: [u8; 32],
) -> Result<ResourcePoolRecord, ReservationError> {
    let record = ResourcePoolRecord::decode(bytes)?;
    let pool = record.pool();
    if pool.context != context || pool.custodian != target || pool.pool_id != pool_id {
        return Err(ReservationError::Corrupt(
            "pool record does not name its storage key".into(),
        ));
    }
    Ok(record)
}

fn decode_reservation_for_key(
    bytes: &[u8],
    context: ContextRef,
    operation_id: [u8; 32],
    target: [u8; 32],
) -> Result<ReservationRecordV0, ReservationError> {
    let record = ReservationRecordV0::decode(bytes)?;
    if record.context != context
        || record.operation_id != operation_id
        || record.custodian != target
    {
        return Err(ReservationError::Corrupt(
            "reservation record does not name its storage key and custodian".into(),
        ));
    }
    Ok(record)
}

fn reconcile_records(
    consumed: Option<ConsumedEntry>,
    reservation: Option<ReservationRecordV0>,
) -> Result<ReservationReconciliation, ReservationError> {
    match (consumed, reservation) {
        (None, None) => Ok(ReservationReconciliation::Absent),
        (Some(consumed), None) => Ok(ReservationReconciliation::ConsumedOnly(Box::new(consumed))),
        (None, Some(_)) => Err(ReservationError::Corrupt(
            "reservation exists without operation consumption".into(),
        )),
        (Some(consumed), Some(reservation)) => {
            if reservation.authorization_hash != hash_consumed(&consumed) {
                return Err(ReservationError::Corrupt(
                    "consumption and reservation authorization hashes differ".into(),
                ));
            }
            // The hash above binds the *consumed authorization*, but not the separate
            // reservation record. Reconstruct the exact request from the durable record and
            // require its effect commitment to match what the writer actually signed.
            let request = ReservationRequestV0 {
                context: reservation.context,
                operation_id: reservation.operation_id,
                pool_id: reservation.pool_id,
                expected_pool_revision: reservation.pool_revision_before,
                quantity: reservation.quantity,
                unit: reservation.unit.clone(),
                request_params_hash: reservation.request_params_hash,
                policy_hash: reservation.policy_hash,
            };
            let expected_effect = request.effect(reservation.custodian).map_err(as_corrupt)?;
            let signed_effect = match &consumed.authorization {
                ConsumedAuthorization::Legacy(signed) => &signed.body.effect,
                ConsumedAuthorization::Checkpointed(signed) => &signed.body.effect,
            };
            if signed_effect != &expected_effect {
                return Err(ReservationError::Corrupt(
                    "reservation parameters do not match the signed authorization effect".into(),
                ));
            }
            Ok(ReservationReconciliation::Committed {
                consumed: Box::new(consumed),
                reservation: Box::new(reservation),
            })
        }
    }
}

fn validate_quantity_unit(quantity: u64, unit: &str) -> Result<(), ReservationError> {
    if quantity == 0 {
        return Err(ReservationError::InvalidInput(
            "reservation quantity must be positive".into(),
        ));
    }
    validate_unit(unit)
}

fn validate_unit(unit: &str) -> Result<(), ReservationError> {
    if unit.is_empty() || unit.len() > MAX_UNIT_LEN {
        return Err(ReservationError::InvalidInput(format!(
            "unit must contain 1..={MAX_UNIT_LEN} bytes"
        )));
    }
    Ok(())
}

fn as_corrupt(error: ReservationError) -> ReservationError {
    ReservationError::Corrupt(error.to_string())
}

struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn new(domain: &[u8]) -> Self {
        Self::with_version(domain, VERSION)
    }

    fn with_version(domain: &[u8], version: u16) -> Self {
        let mut writer = Writer { bytes: Vec::new() };
        // Every caller passes one of this module's short constant domains.
        writer.u32(domain.len() as u32);
        writer.bytes.extend_from_slice(domain);
        writer.u16(version);
        writer
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn b32(&mut self, value: &[u8; 32]) {
        self.bytes.extend_from_slice(value);
    }

    fn lp(&mut self, value: &[u8]) -> Result<(), ReservationError> {
        let len = u32::try_from(value.len())
            .map_err(|_| ReservationError::InvalidInput("length exceeds u32".into()))?;
        self.u32(len);
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], domain: &[u8]) -> Result<Self, ReservationError> {
        Self::with_version(bytes, domain, VERSION)
    }

    fn with_version(
        bytes: &'a [u8],
        domain: &[u8],
        version: u16,
    ) -> Result<Self, ReservationError> {
        let mut reader = Reader { bytes, pos: 0 };
        if reader.lp("domain")? != domain {
            return Err(ReservationError::Corrupt("record domain mismatch".into()));
        }
        if reader.u16("version")? != version {
            return Err(ReservationError::Corrupt(
                "unsupported record version".into(),
            ));
        }
        Ok(reader)
    }

    fn take(&mut self, len: usize, field: &str) -> Result<&'a [u8], ReservationError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| ReservationError::Corrupt(format!("{field} length overflow")))?;
        let value = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| ReservationError::Corrupt(format!("truncated {field}")))?;
        self.pos = end;
        Ok(value)
    }

    fn u8(&mut self, field: &str) -> Result<u8, ReservationError> {
        Ok(self.take(1, field)?[0])
    }

    fn u16(&mut self, field: &str) -> Result<u16, ReservationError> {
        let bytes: [u8; 2] = self
            .take(2, field)?
            .try_into()
            .map_err(|_| ReservationError::Corrupt(format!("invalid {field}")))?;
        Ok(u16::from_be_bytes(bytes))
    }

    fn u32(&mut self, field: &str) -> Result<u32, ReservationError> {
        let bytes: [u8; 4] = self
            .take(4, field)?
            .try_into()
            .map_err(|_| ReservationError::Corrupt(format!("invalid {field}")))?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn u64(&mut self, field: &str) -> Result<u64, ReservationError> {
        let bytes: [u8; 8] = self
            .take(8, field)?
            .try_into()
            .map_err(|_| ReservationError::Corrupt(format!("invalid {field}")))?;
        Ok(u64::from_be_bytes(bytes))
    }

    fn b32(&mut self, field: &str) -> Result<[u8; 32], ReservationError> {
        self.take(32, field)?
            .try_into()
            .map_err(|_| ReservationError::Corrupt(format!("invalid {field}")))
    }

    fn lp(&mut self, field: &str) -> Result<&'a [u8], ReservationError> {
        let len = usize::try_from(self.u32(field)?)
            .map_err(|_| ReservationError::Corrupt(format!("invalid {field} length")))?;
        self.take(len, field)
    }

    fn text(&mut self, field: &str) -> Result<String, ReservationError> {
        String::from_utf8(self.lp(field)?.to_vec())
            .map_err(|_| ReservationError::Corrupt(format!("{field} is not UTF-8")))
    }

    fn boolean(&mut self, field: &str) -> Result<bool, ReservationError> {
        match self.u8(field)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ReservationError::Corrupt(format!("invalid {field}"))),
        }
    }

    fn finish(self) -> Result<(), ReservationError> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(ReservationError::Corrupt("trailing record bytes".into()))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::process::Command;

    use ed25519_dalek::SigningKey;
    use icn_authz::effect_v0::{
        CheckpointedAuthorizationV0, ConsumedAuthorization, LogCheckpointV0, Signed, Verdict,
    };
    use icn_identity::authority_log::PrincipalKey;

    use super::*;

    const CTX: ContextRef = ContextRef([0x81; 32]);
    const OP: [u8; 32] = [0x82; 32];
    const POOL: [u8; 32] = [0x83; 32];
    const TARGET: [u8; 32] = [0x84; 32];
    const POLICY: [u8; 32] = [0x85; 32];

    fn request() -> ReservationRequestV0 {
        ReservationRequestV0 {
            context: CTX,
            operation_id: OP,
            pool_id: POOL,
            expected_pool_revision: 0,
            quantity: 2,
            unit: "hours".into(),
            request_params_hash: [0x86; 32],
            policy_hash: POLICY,
        }
    }

    fn pool() -> ResourcePoolV0 {
        ResourcePoolV0 {
            context: CTX,
            pool_id: POOL,
            custodian: TARGET,
            unit: "hours".into(),
            total_capacity: 5,
            available_capacity: 5,
            revision: 0,
            policy: LocalReservationPolicyV0 {
                enabled: true,
                max_quantity_per_reservation: 5,
                policy_hash: POLICY,
            },
        }
    }

    fn binding() -> CommitBinding {
        let key = SigningKey::from_bytes(&[0x87; 32]);
        let request = request();
        let effect = request.effect(TARGET).unwrap();
        let body = CheckpointedAuthorizationV0 {
            context: CTX,
            checkpoint: LogCheckpointV0 {
                seq: 1,
                head: [0x88; 32],
            },
            subject_matter: effect.commitment(CTX, OP),
            rule_hash: [0x89; 32],
            verdict: Verdict::Pass,
            inputs: Vec::new(),
            effect,
            operation_id: OP,
            not_after: 1_500,
        };
        let consumed = ConsumedEntry {
            authorization: ConsumedAuthorization::Checkpointed(Signed::sign(body, &key)),
            writer: PrincipalKey::try_from_verifying_key(key.verifying_key()).unwrap(),
        };
        CommitBinding {
            authorization_hash: hash_consumed(&consumed),
            consumed,
            admitted: AdmittedUnder::CallerResolved,
        }
    }

    fn assert_pristine(store: &LocalResourceReservations) {
        assert_eq!(store.inspect_pool(CTX, POOL).unwrap(), Some(pool()));
        assert!(store.inspect_consumed(CTX, OP).unwrap().is_none());
        assert!(store.inspect_reservation(CTX, OP).unwrap().is_none());
    }

    #[test]
    fn well_formed_tampered_reservation_is_not_reconciled_as_committed() {
        let dir = tempfile::tempdir().unwrap();
        let (store, admin) = LocalResourceReservations::open(dir.path(), TARGET)
            .unwrap()
            .with_pool_admin();
        store.initialize_pool(&admin, pool()).unwrap();
        let outcome = store
            .commit_atomic(&binding(), &request(), 900, |_| Ok(()))
            .unwrap();
        assert!(matches!(outcome, AtomicOutcome::Reserved(_)));
        store.barrier.sync(&store.store).unwrap();

        let mut tampered = store.inspect_reservation(CTX, OP).unwrap().unwrap();
        // This stays internally well-formed: only the signed request binding detects it.
        tampered.quantity = 1;
        tampered.available_after = tampered.available_before - 1;
        Store::put(
            &store.store,
            &reservation_key(CTX, OP),
            &tampered.encode().unwrap(),
        )
        .unwrap();
        Store::flush(&store.store).unwrap();
        assert!(matches!(
            store.reconcile(CTX, OP),
            Err(ReservationError::Corrupt(_))
        ));
    }

    #[test]
    fn missing_or_regressed_pool_never_reconciles_as_committed() {
        let dir = tempfile::tempdir().unwrap();
        let (store, admin) = LocalResourceReservations::open(dir.path(), TARGET)
            .unwrap()
            .with_pool_admin();
        store.initialize_pool(&admin, pool()).unwrap();
        let outcome = store
            .commit_atomic(&binding(), &request(), 900, |_| Ok(()))
            .unwrap();
        assert!(matches!(outcome, AtomicOutcome::Reserved(_)));
        store.barrier.sync(&store.store).unwrap();

        // A well-formed but regressed pool contradicts the recorded reservation.
        Store::put(
            &store.store,
            &pool_key(CTX, TARGET, POOL),
            &pool().encode().unwrap(),
        )
        .unwrap();
        Store::flush(&store.store).unwrap();
        assert!(matches!(
            store.reconcile(CTX, OP),
            Err(ReservationError::Corrupt(_))
        ));

        Store::delete(&store.store, &pool_key(CTX, TARGET, POOL)).unwrap();
        Store::flush(&store.store).unwrap();
        assert!(matches!(
            store.reconcile(CTX, OP),
            Err(ReservationError::Corrupt(_))
        ));
    }

    #[test]
    fn transaction_faults_after_staged_writes_commit_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (store, admin) = LocalResourceReservations::open(dir.path(), TARGET)
            .unwrap()
            .with_pool_admin();
        store.initialize_pool(&admin, pool()).unwrap();
        for fail_at in [CommitStage::Consumed, CommitStage::Capacity] {
            let result = store.commit_atomic(&binding(), &request(), 900, |stage| {
                if stage == fail_at {
                    anyhow::bail!("injected transaction fault");
                }
                Ok(())
            });
            assert!(matches!(result, Err(ReservationError::Unavailable(_))));
            assert_pristine(&store);
        }
    }

    #[test]
    fn real_child_abort_recovers_only_whole_transactions() {
        use std::os::unix::process::ExitStatusExt;

        for mode in ["mid-consumed", "mid-capacity", "after-barrier"] {
            let dir = tempfile::tempdir().unwrap();
            {
                let (store, admin) = LocalResourceReservations::open(dir.path(), TARGET)
                    .unwrap()
                    .with_pool_admin();
                store.initialize_pool(&admin, pool()).unwrap();
            }
            let status = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "reservation::tests::child_transaction_abort_helper",
                ])
                .env("ICN_RESERVATION_ABORT_DB", dir.path())
                .env("ICN_RESERVATION_ABORT_MODE", mode)
                .status()
                .unwrap();
            assert_eq!(status.signal(), Some(6), "mode {mode}: {status:?}");

            let reopened = LocalResourceReservations::open(dir.path(), TARGET).unwrap();
            if mode == "after-barrier" {
                assert_eq!(
                    reopened
                        .inspect_pool(CTX, POOL)
                        .unwrap()
                        .unwrap()
                        .available_capacity,
                    3
                );
                assert!(matches!(
                    reopened.reconcile(CTX, OP).unwrap(),
                    ReservationReconciliation::Committed { .. }
                ));
            } else {
                assert_pristine(&reopened);
            }
        }
    }

    #[test]
    #[ignore = "child-process helper; invoked by real_child_abort_recovers_only_whole_transactions"]
    fn child_transaction_abort_helper() {
        let Some(path) = std::env::var_os("ICN_RESERVATION_ABORT_DB") else {
            return;
        };
        let mode = std::env::var("ICN_RESERVATION_ABORT_MODE").unwrap();
        let store = LocalResourceReservations::open(path, TARGET).unwrap();
        let outcome = store
            .commit_atomic(&binding(), &request(), 900, |stage| {
                if (mode == "mid-consumed" && stage == CommitStage::Consumed)
                    || (mode == "mid-capacity" && stage == CommitStage::Capacity)
                {
                    std::process::abort();
                }
                Ok(())
            })
            .unwrap();
        assert!(matches!(outcome, AtomicOutcome::Reserved(_)));
        store.barrier.sync(&store.store).unwrap();
        std::process::abort();
    }

    // ---------------------------------------------------------------------------------------
    // Pool admission requirement (experimental V1 record)
    // ---------------------------------------------------------------------------------------

    const ADMISSION: AdmissionPolicyRefV0 = AdmissionPolicyRefV0 {
        id: [0x8a; 32],
        version: 0,
    };

    fn pool_v1(admission: AdmissionRequirementV1) -> ResourcePoolV1 {
        ResourcePoolV1 {
            pool: pool(),
            admission,
        }
    }

    fn requires(min_grade: AdmissionGradeV0) -> AdmissionRequirementV1 {
        AdmissionRequirementV1::Policy {
            policy: ADMISSION,
            min_grade,
        }
    }

    fn admitted(policy: AdmissionPolicyRefV0, grade: AdmissionGradeV0) -> CommitBinding {
        CommitBinding {
            admitted: AdmittedUnder::Policy { policy, grade },
            ..binding()
        }
    }

    fn rejected(outcome: AtomicOutcome) -> ReservationError {
        match outcome {
            AtomicOutcome::Rejected(error) => error,
            AtomicOutcome::Reserved(_) => panic!("expected a rejection, got a reservation"),
            AtomicOutcome::Prior(_) => panic!("expected a rejection, got a prior consumption"),
        }
    }

    fn v1_store(
        dir: &tempfile::TempDir,
        admission: AdmissionRequirementV1,
    ) -> LocalResourceReservations {
        let (store, admin) = LocalResourceReservations::open(dir.path(), TARGET)
            .unwrap()
            .with_pool_admin();
        store
            .initialize_pool_v1(&admin, pool_v1(admission))
            .unwrap();
        store
    }

    #[test]
    fn policy_pool_refuses_a_caller_resolved_commit_and_stays_pristine() {
        let dir = tempfile::tempdir().unwrap();
        let store = v1_store(&dir, requires(AdmissionGradeV0::Rehearsal));
        let error = rejected(
            store
                .commit_atomic(&binding(), &request(), 900, |_| Ok(()))
                .unwrap(),
        );
        assert!(
            matches!(error, ReservationError::AdmissionDenied(_)),
            "{error:?}"
        );
        // The request carries the pool's policy_hash; that alone is not admission.
        assert_eq!(request().policy_hash, pool().policy.policy_hash);
        assert_pristine(&store);
    }

    #[test]
    fn policy_pool_accepts_only_its_exact_policy_version_and_grade() {
        let other_id = AdmissionPolicyRefV0 {
            id: [0x8b; 32],
            ..ADMISSION
        };
        let other_version = AdmissionPolicyRefV0 {
            version: 1,
            ..ADMISSION
        };
        for (binding, why) in [
            (
                admitted(other_id, AdmissionGradeV0::Production),
                "other policy",
            ),
            (
                admitted(other_version, AdmissionGradeV0::Production),
                "other version",
            ),
            (
                admitted(ADMISSION, AdmissionGradeV0::Rehearsal),
                "grade too low",
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let store = v1_store(&dir, requires(AdmissionGradeV0::Production));
            let error = rejected(
                store
                    .commit_atomic(&binding, &request(), 900, |_| Ok(()))
                    .unwrap(),
            );
            assert!(
                matches!(error, ReservationError::AdmissionDenied(_)),
                "{why}: {error:?}"
            );
            assert_pristine(&store);
        }
        let dir = tempfile::tempdir().unwrap();
        let store = v1_store(&dir, requires(AdmissionGradeV0::Production));
        let outcome = store
            .commit_atomic(
                &admitted(ADMISSION, AdmissionGradeV0::Production),
                &request(),
                900,
                |_| Ok(()),
            )
            .unwrap();
        assert!(matches!(outcome, AtomicOutcome::Reserved(_)));
    }

    #[test]
    fn explicit_generic_v1_pool_keeps_the_caller_resolved_contract() {
        let dir = tempfile::tempdir().unwrap();
        let store = v1_store(&dir, AdmissionRequirementV1::CallerResolved);
        let error = rejected(
            store
                .commit_atomic(
                    &admitted(ADMISSION, AdmissionGradeV0::Production),
                    &request(),
                    900,
                    |_| Ok(()),
                )
                .unwrap(),
        );
        assert!(
            matches!(error, ReservationError::AdmissionDenied(_)),
            "{error:?}"
        );
        let outcome = store
            .commit_atomic(&binding(), &request(), 900, |_| Ok(()))
            .unwrap();
        assert!(matches!(outcome, AtomicOutcome::Reserved(_)));
    }

    #[test]
    fn commit_keeps_the_v1_record_and_requirement_across_restart() {
        let dir = tempfile::tempdir().unwrap();
        let required = requires(AdmissionGradeV0::Production);
        {
            let store = v1_store(&dir, required);
            let outcome = store
                .commit_atomic(
                    &admitted(ADMISSION, AdmissionGradeV0::Production),
                    &request(),
                    900,
                    |_| Ok(()),
                )
                .unwrap();
            assert!(matches!(outcome, AtomicOutcome::Reserved(_)));
            store.barrier.sync(&store.store).unwrap();
        }
        let store = LocalResourceReservations::open(dir.path(), TARGET).unwrap();
        let Some(ResourcePoolRecord::V1(stored)) = store.inspect_pool_record(CTX, POOL).unwrap()
        else {
            panic!("a commit must not rewrite a V1 pool as V0");
        };
        assert_eq!(stored.admission, required);
        assert_eq!(
            (stored.pool.revision, stored.pool.available_capacity),
            (1, 3)
        );
        assert!(matches!(
            store.reconcile(CTX, OP).unwrap(),
            ReservationReconciliation::Committed { .. }
        ));
    }

    #[test]
    fn requirement_changed_after_verification_is_enforced_at_commit() {
        // Stands in for a future policy-update path or a concurrent writer: verification ran
        // under ADMISSION v0, then the authoritative pool record moved to v1 before the commit.
        let dir = tempfile::tempdir().unwrap();
        let store = v1_store(&dir, requires(AdmissionGradeV0::Rehearsal));
        let verified = admitted(ADMISSION, AdmissionGradeV0::Production);
        let moved = pool_v1(AdmissionRequirementV1::Policy {
            policy: AdmissionPolicyRefV0 {
                version: 1,
                ..ADMISSION
            },
            min_grade: AdmissionGradeV0::Rehearsal,
        });
        Store::put(
            &store.store,
            &pool_key(CTX, TARGET, POOL),
            &moved.encode().unwrap(),
        )
        .unwrap();
        let error = rejected(
            store
                .commit_atomic(&verified, &request(), 900, |_| Ok(()))
                .unwrap(),
        );
        assert!(
            matches!(error, ReservationError::AdmissionDenied(_)),
            "{error:?}"
        );
        assert!(store.inspect_consumed(CTX, OP).unwrap().is_none());
        assert_eq!(store.inspect_pool(CTX, POOL).unwrap(), Some(pool()));
    }

    #[test]
    fn v1_records_are_strict_and_unreadable_by_a_v0_decoder() {
        let record = pool_v1(requires(AdmissionGradeV0::Production));
        let bytes = record.encode().unwrap();
        assert_eq!(ResourcePoolV1::decode(&bytes).unwrap(), record);
        assert_eq!(
            ResourcePoolRecord::decode(&bytes).unwrap(),
            ResourcePoolRecord::V1(record.clone())
        );
        // An old binary only knows V0: it refuses the record instead of reading a generic pool.
        assert!(matches!(
            ResourcePoolV0::decode(&bytes),
            Err(ReservationError::Corrupt(_))
        ));
        let corrupt = |mutate: &dyn Fn(&mut Vec<u8>)| {
            let mut b = bytes.clone();
            mutate(&mut b);
            ResourcePoolRecord::decode(&b)
        };
        let len = bytes.len();
        // Layout tail: requirement tag, policy id (32), version (4), grade (1).
        let tag_at = len - 38;
        for (name, result) in [
            ("unknown requirement tag", corrupt(&|b| b[tag_at] = 9)),
            ("unknown grade", corrupt(&|b| b[len - 1] = 0)),
            ("truncated", corrupt(&|b| b.truncate(len - 1))),
            ("trailing byte", corrupt(&|b| b.push(0))),
            ("record version 0 under the V1 domain", {
                let domain_end = 4 + POOL_RECORD_V1_DOMAIN.len();
                corrupt(&|b| b[domain_end + 1] = 0)
            }),
        ] {
            assert!(
                matches!(result, Err(ReservationError::Corrupt(_))),
                "{name}: {result:?}"
            );
        }
        assert_eq!(bytes[tag_at], REQUIREMENT_POLICY);
    }

    #[test]
    fn corrupt_policy_record_refuses_rather_than_reopening_generic_admission() {
        let dir = tempfile::tempdir().unwrap();
        let store = v1_store(&dir, requires(AdmissionGradeV0::Rehearsal));
        let mut bytes = pool_v1(requires(AdmissionGradeV0::Rehearsal))
            .encode()
            .unwrap();
        let tag_at = bytes.len() - 38;
        bytes[tag_at] = 0;
        Store::put(&store.store, &pool_key(CTX, TARGET, POOL), &bytes).unwrap();
        let error = rejected(
            store
                .commit_atomic(&binding(), &request(), 900, |_| Ok(()))
                .unwrap(),
        );
        assert!(matches!(error, ReservationError::Corrupt(_)), "{error:?}");
        assert!(store.inspect_consumed(CTX, OP).unwrap().is_none());
    }

    /// Isolates the unknown-tag and unknown-grade arms from the trailing-bytes check: each record
    /// is otherwise well formed, so only a fail-closed decode refuses it. A generic V1 record is
    /// the shortest form: its last byte is the requirement tag.
    #[test]
    fn unknown_requirement_tag_or_grade_is_corrupt_without_trailing_bytes() {
        let generic = pool_v1(AdmissionRequirementV1::CallerResolved)
            .encode()
            .unwrap();
        let policy = pool_v1(requires(AdmissionGradeV0::Rehearsal))
            .encode()
            .unwrap();
        assert!(ResourcePoolRecord::decode(&generic).is_ok());
        assert!(ResourcePoolRecord::decode(&policy).is_ok());
        for tag in [0u8, 3, 0xff] {
            let mut bytes = generic.clone();
            *bytes.last_mut().unwrap() = tag;
            assert!(
                matches!(
                    ResourcePoolRecord::decode(&bytes),
                    Err(ReservationError::Corrupt(_))
                ),
                "requirement tag {tag}"
            );
        }
        for grade in [0u8, 3, 0xff] {
            let mut bytes = policy.clone();
            *bytes.last_mut().unwrap() = grade;
            assert!(
                matches!(
                    ResourcePoolRecord::decode(&bytes),
                    Err(ReservationError::Corrupt(_))
                ),
                "grade {grade}"
            );
        }
    }

    /// KNOWN LIMIT, pinned: the gate is an API boundary, not storage integrity. An adversary
    /// with raw write access can replace a policy pool with a well-formed V0 record, which then
    /// admits caller-resolved reservations. No API path writes such a record.
    #[test]
    fn raw_storage_downgrade_is_outside_the_api_gate() {
        let dir = tempfile::tempdir().unwrap();
        let (store, admin) =
            v1_store(&dir, requires(AdmissionGradeV0::Production)).with_pool_admin();
        // The API itself refuses to overwrite the pool with a V0 record, and returns the stored one.
        assert_eq!(
            store.initialize_pool(&admin, pool()),
            Err(ReservationError::PoolAlreadyExists {
                existing: Box::new(ResourcePoolRecord::V1(pool_v1(requires(
                    AdmissionGradeV0::Production
                ))))
            })
        );
        Store::put(
            &store.store,
            &pool_key(CTX, TARGET, POOL),
            &pool().encode().unwrap(),
        )
        .unwrap();
        let outcome = store
            .commit_atomic(&binding(), &request(), 900, |_| Ok(()))
            .unwrap();
        assert!(matches!(outcome, AtomicOutcome::Reserved(_)));
    }

    struct NeverCalled(AdmissionPolicyRefV0);

    impl crate::CustodianAdmissionVerifierV0 for NeverCalled {
        fn policy(&self) -> AdmissionPolicyRefV0 {
            self.0
        }
        fn preflight(
            &self,
            _: &CheckpointedEvidenceV0,
            _: &dyn Any,
            _: [u8; 32],
            _: u64,
        ) -> Result<crate::VerifiedAdmissionV0, crate::AdmissionRefusalV0> {
            panic!("a verifier for another policy or version must never be consulted")
        }
    }

    #[test]
    fn registry_refuses_a_second_verifier_for_the_same_policy() {
        let mut registry = AdmissionRegistryV0::new();
        registry
            .install(std::sync::Arc::new(NeverCalled(ADMISSION)))
            .unwrap();
        assert_eq!(
            registry.install(std::sync::Arc::new(NeverCalled(ADMISSION))),
            Err(crate::AdmissionRegistryError::Duplicate)
        );
    }

    // ---- Part 8 F1: custodian clock --------------------------------------------------------

    /// A fixed clock (a reading, or unavailable time) that also counts how often it is read.
    struct FixedClock {
        at: Option<u64>,
        reads: std::sync::atomic::AtomicUsize,
    }

    impl FixedClock {
        fn at(at: u64) -> std::sync::Arc<Self> {
            Self::reading(Some(at))
        }
        fn unavailable() -> std::sync::Arc<Self> {
            Self::reading(None)
        }
        fn reading(at: Option<u64>) -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self {
                at,
                reads: std::sync::atomic::AtomicUsize::new(0),
            })
        }
        fn reads(&self) -> usize {
            self.reads.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl crate::CustodianClockV0 for FixedClock {
        fn now(&self) -> Option<u64> {
            self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.at
        }
    }

    /// Records the `now` it was handed, then refuses (so nothing is ever committed).
    struct RecordingVerifier {
        policy: AdmissionPolicyRefV0,
        seen: std::sync::Mutex<Vec<u64>>,
    }

    impl crate::CustodianAdmissionVerifierV0 for RecordingVerifier {
        fn policy(&self) -> AdmissionPolicyRefV0 {
            self.policy
        }
        fn preflight(
            &self,
            _: &CheckpointedEvidenceV0,
            _: &dyn Any,
            _: [u8; 32],
            now: u64,
        ) -> Result<crate::VerifiedAdmissionV0, crate::AdmissionRefusalV0> {
            self.seen.lock().unwrap().push(now);
            Err(crate::AdmissionRefusalV0("recording only".into()))
        }
    }

    /// Structurally valid evidence whose authorization claims `not_after`. The verifiers here
    /// never verify it; it only has to be a value a caller could pass.
    fn evidence(not_after: u64) -> CheckpointedEvidenceV0 {
        let key = SigningKey::from_bytes(&[0x87; 32]);
        let writer = PrincipalKey::try_from_verifying_key(key.verifying_key()).unwrap();
        let effect = request().effect(TARGET).unwrap();
        let body = CheckpointedAuthorizationV0 {
            context: CTX,
            checkpoint: LogCheckpointV0 {
                seq: 1,
                head: [0x88; 32],
            },
            subject_matter: effect.commitment(CTX, OP),
            rule_hash: [0x89; 32],
            verdict: Verdict::Pass,
            inputs: Vec::new(),
            effect,
            operation_id: OP,
            not_after,
        };
        CheckpointedEvidenceV0 {
            inception: icn_authz::effect_v0::ContextInceptionV0::new(vec![writer], 1, [0x8A; 32])
                .unwrap(),
            log: Vec::new(),
            authorization: Signed::sign(body, &key),
            vote_bundles: Vec::new(),
        }
    }

    fn policy_store(
        dir: &tempfile::TempDir,
        verifier: std::sync::Arc<dyn crate::CustodianAdmissionVerifierV0>,
        clock: Option<std::sync::Arc<dyn crate::CustodianClockV0>>,
    ) -> LocalResourceReservations {
        let mut registry = AdmissionRegistryV0::new();
        registry.install(verifier).unwrap();
        if let Some(clock) = clock {
            registry.install_clock(clock).unwrap();
        }
        let (store, admin) = LocalResourceReservations::open(dir.path(), TARGET)
            .unwrap()
            .with_admission(registry)
            .with_pool_admin();
        store
            .initialize_pool_v1(&admin, pool_v1(requires(AdmissionGradeV0::Rehearsal)))
            .unwrap();
        store
    }

    fn untouched(store: &LocalResourceReservations) {
        assert!(store.inspect_consumed(CTX, OP).unwrap().is_none());
        assert!(store.inspect_reservation(CTX, OP).unwrap().is_none());
        assert_eq!(
            store.inspect_pool_record(CTX, POOL).unwrap(),
            Some(ResourcePoolRecord::V1(pool_v1(requires(
                AdmissionGradeV0::Rehearsal
            ))))
        );
    }

    #[test]
    fn without_a_clock_reserve_admitted_fails_closed_before_any_verifier_or_write() {
        let dir = tempfile::tempdir().unwrap();
        // NeverCalled panics if consulted: the refusal must come before the verifier.
        let store = policy_store(&dir, std::sync::Arc::new(NeverCalled(ADMISSION)), None);
        for not_after in [0, 1_000, u64::MAX] {
            assert!(matches!(
                store.reserve_admitted(&evidence(not_after), &(), &request()),
                Err(ReservationError::AdmissionUnavailable(_))
            ));
        }
        untouched(&store);
        // A generic (caller-resolved) V1 pool is refused the same way: no clock, no admitted path.
        let dir = tempfile::tempdir().unwrap();
        let generic = v1_store(&dir, AdmissionRequirementV1::CallerResolved);
        assert!(matches!(
            generic.reserve_admitted(&evidence(1_000), &(), &request()),
            Err(ReservationError::AdmissionUnavailable(_))
        ));
    }

    #[test]
    fn an_unavailable_clock_fails_closed_before_any_verifier_or_write_whatever_not_after() {
        let dir = tempfile::tempdir().unwrap();
        let clock = FixedClock::unavailable();
        // NeverCalled panics if consulted: the refusal must come before the verifier. Including
        // `not_after = u64::MAX`, which no substitute reading (`0` or `u64::MAX`) would refuse.
        let store = policy_store(
            &dir,
            std::sync::Arc::new(NeverCalled(ADMISSION)),
            Some(clock.clone()),
        );
        for not_after in [0, 1_000, u64::MAX] {
            assert!(matches!(
                store.reserve_admitted(&evidence(not_after), &(), &request()),
                Err(ReservationError::AdmissionUnavailable(_))
            ));
        }
        assert_eq!(clock.reads(), 3, "exactly one clock reading per call");
        untouched(&store);
    }

    #[test]
    fn zero_is_a_reading_and_unavailable_time_is_not() {
        // `Some(0)` is the epoch: it reaches the verifier as time 0.
        let dir = tempfile::tempdir().unwrap();
        let verifier = std::sync::Arc::new(RecordingVerifier {
            policy: ADMISSION,
            seen: std::sync::Mutex::new(Vec::new()),
        });
        let store = policy_store(&dir, verifier.clone(), Some(FixedClock::at(0)));
        assert!(matches!(
            store.reserve_admitted(&evidence(1_000), &(), &request()),
            Err(ReservationError::AdmissionRefused(_))
        ));
        assert_eq!(*verifier.seen.lock().unwrap(), vec![0]);
        untouched(&store);

        // `None` never reaches it: unavailable, not "time 0".
        let dir = tempfile::tempdir().unwrap();
        let verifier = std::sync::Arc::new(RecordingVerifier {
            policy: ADMISSION,
            seen: std::sync::Mutex::new(Vec::new()),
        });
        let store = policy_store(&dir, verifier.clone(), Some(FixedClock::unavailable()));
        assert!(matches!(
            store.reserve_admitted(&evidence(1_000), &(), &request()),
            Err(ReservationError::AdmissionUnavailable(_))
        ));
        assert!(verifier.seen.lock().unwrap().is_empty());
        untouched(&store);
    }

    #[test]
    fn the_verifier_is_handed_exactly_the_custodian_clock_whatever_the_caller_supplies() {
        for at in [0u64, 1_000, 1_700_000_000, u64::MAX] {
            let dir = tempfile::tempdir().unwrap();
            let verifier = std::sync::Arc::new(RecordingVerifier {
                policy: ADMISSION,
                seen: std::sync::Mutex::new(Vec::new()),
            });
            let clock = FixedClock::at(at);
            let store = policy_store(&dir, verifier.clone(), Some(clock.clone()));
            // Every caller-controlled input varies; none of them moves the admission time.
            for (not_after, supplement) in [(0u64, 1u64), (at, at), (u64::MAX, 0)] {
                assert!(matches!(
                    store.reserve_admitted(&evidence(not_after), &supplement, &request()),
                    Err(ReservationError::AdmissionRefused(_))
                ));
            }
            assert_eq!(*verifier.seen.lock().unwrap(), vec![at, at, at]);
            assert_eq!(clock.reads(), 3, "exactly one clock reading per call");
            untouched(&store);
        }
    }

    #[test]
    fn registry_refuses_a_second_clock() {
        let mut registry = AdmissionRegistryV0::new();
        registry.install_clock(FixedClock::at(1)).unwrap();
        assert_eq!(
            registry.install_clock(FixedClock::at(2)),
            Err(crate::AdmissionRegistryError::DuplicateClock)
        );
        assert!(format!("{registry:?}").contains("clock_installed: true"));
    }

    // ---- Part 8 F2: pool-administration capability ------------------------------------------

    fn admin_store(dir: &tempfile::TempDir) -> (LocalResourceReservations, PoolAdminV0) {
        LocalResourceReservations::open(dir.path(), TARGET)
            .unwrap()
            .with_pool_admin()
    }

    fn other_pool_id() -> ResourcePoolV0 {
        let mut other = pool();
        other.pool_id = [0x8F; 32];
        other
    }

    fn already_exists(record: ResourcePoolRecord) -> Result<(), ReservationError> {
        Err(ReservationError::PoolAlreadyExists {
            existing: Box::new(record),
        })
    }

    #[test]
    fn the_custodian_admin_creates_v0_and_v1_pools() {
        let dir = tempfile::tempdir().unwrap();
        let (store, admin) = admin_store(&dir);
        store.initialize_pool(&admin, pool()).unwrap();
        assert_eq!(
            store.inspect_pool_record(CTX, POOL).unwrap(),
            Some(ResourcePoolRecord::V0(pool()))
        );

        let dir = tempfile::tempdir().unwrap();
        let (store, admin) = admin_store(&dir);
        let v1 = pool_v1(requires(AdmissionGradeV0::Production));
        store.initialize_pool_v1(&admin, v1.clone()).unwrap();
        assert_eq!(
            store.inspect_pool_record(CTX, POOL).unwrap(),
            Some(ResourcePoolRecord::V1(v1))
        );
    }

    #[test]
    fn a_duplicate_pool_returns_the_existing_record_and_overwrites_nothing() {
        // V0 first: every later attempt at the same key returns the original V0 record.
        let dir = tempfile::tempdir().unwrap();
        let (store, admin) = admin_store(&dir);
        store.initialize_pool(&admin, pool()).unwrap();
        let mut larger = pool();
        larger.total_capacity += 1;
        larger.available_capacity = larger.total_capacity;
        for attempt in [
            store.initialize_pool(&admin, pool()),
            store.initialize_pool(&admin, larger),
            store.initialize_pool_v1(&admin, pool_v1(requires(AdmissionGradeV0::Rehearsal))),
        ] {
            assert_eq!(attempt, already_exists(ResourcePoolRecord::V0(pool())));
        }
        assert_eq!(
            store.inspect_pool_record(CTX, POOL).unwrap(),
            Some(ResourcePoolRecord::V0(pool()))
        );

        // V1 first: a V0 (generic) attempt cannot downgrade it and returns the V1 record.
        let dir = tempfile::tempdir().unwrap();
        let (store, admin) = admin_store(&dir);
        let v1 = pool_v1(requires(AdmissionGradeV0::Production));
        store.initialize_pool_v1(&admin, v1.clone()).unwrap();
        assert_eq!(
            store.initialize_pool(&admin, pool()),
            already_exists(ResourcePoolRecord::V1(v1.clone()))
        );
        assert_eq!(
            store.inspect_pool_record(CTX, POOL).unwrap(),
            Some(ResourcePoolRecord::V1(v1))
        );
    }

    #[test]
    fn another_stores_admin_is_refused_and_nothing_is_written() {
        let dir_a = tempfile::tempdir().unwrap();
        let (_store_a, admin_a) = admin_store(&dir_a);
        // A store that issued its own admin, and one that never issued any.
        let dir_b = tempfile::tempdir().unwrap();
        let (store_b, _admin_b) = admin_store(&dir_b);
        let dir_c = tempfile::tempdir().unwrap();
        let store_c = LocalResourceReservations::open(dir_c.path(), TARGET).unwrap();
        for store in [&store_b, &store_c] {
            assert_eq!(
                store.initialize_pool(&admin_a, pool()),
                Err(ReservationError::PoolAdminMismatch)
            );
            assert_eq!(
                store.initialize_pool_v1(&admin_a, pool_v1(requires(AdmissionGradeV0::Rehearsal))),
                Err(ReservationError::PoolAdminMismatch)
            );
            assert_eq!(store.inspect_pool_record(CTX, POOL).unwrap(), None);
        }
    }

    #[test]
    fn issuing_again_revokes_the_previous_admin() {
        let dir = tempfile::tempdir().unwrap();
        let (store, first) = admin_store(&dir);
        let (store, second) = store.with_pool_admin();
        assert_eq!(
            store.initialize_pool(&first, pool()),
            Err(ReservationError::PoolAdminMismatch)
        );
        assert_eq!(store.inspect_pool_record(CTX, POOL).unwrap(), None);
        store.initialize_pool(&second, pool()).unwrap();
    }

    #[test]
    fn the_admin_check_precedes_validation_and_any_storage_read() {
        let dir = tempfile::tempdir().unwrap();
        let (store, admin) = admin_store(&dir);
        let other = tempfile::tempdir().unwrap();
        let (_other, foreign) = admin_store(&other);
        // An invalid pool is refused for the admin, not for its content.
        let mut invalid = pool();
        invalid.revision = 1;
        assert_eq!(
            store.initialize_pool(&foreign, invalid.clone()),
            Err(ReservationError::PoolAdminMismatch)
        );
        assert!(matches!(
            store.initialize_pool(&admin, invalid),
            Err(ReservationError::InvalidInput(_))
        ));
        // An existing pool is not revealed to a foreign admin (no PoolAlreadyExists).
        store.initialize_pool(&admin, pool()).unwrap();
        assert_eq!(
            store.initialize_pool(&foreign, pool()),
            Err(ReservationError::PoolAdminMismatch)
        );
    }

    #[test]
    fn an_admin_does_not_survive_reopening_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let (store, old_admin) = admin_store(&dir);
        store.initialize_pool(&old_admin, pool()).unwrap();
        drop(store);

        let reopened = LocalResourceReservations::open(dir.path(), TARGET).unwrap();
        assert_eq!(
            reopened.initialize_pool(&old_admin, other_pool_id()),
            Err(ReservationError::PoolAdminMismatch)
        );
        let (reopened, admin) = reopened.with_pool_admin();
        assert_eq!(
            reopened.initialize_pool(&admin, pool()),
            already_exists(ResourcePoolRecord::V0(pool()))
        );
        reopened.initialize_pool(&admin, other_pool_id()).unwrap();
        assert_eq!(
            reopened.inspect_pool(CTX, [0x8F; 32]).unwrap(),
            Some(other_pool_id())
        );
    }
}
