//! Storage Specification (E4)
//!
//! Defines storage tiers and data locality rules for the compute substrate.
//!
//! # Design
//!
//! Storage is classified along two dimensions:
//!
//! 1. **Storage Class** - What kind of data is this?
//!    - Canonical: Immutable, replicated, authoritative (ledger, governance, trust)
//!    - ServiceState: Mutable, coop-scoped (app state, calendars, schedules)
//!    - Blobs: Content-addressed, locality-flexible (documents, media, backups)
//!
//! 2. **Data Locality** - Where can this data live?
//!    - CellLocal: Must stay in originating cell
//!    - CoopReplicated: Replicated across coop nodes
//!    - FederationMirrored: Cross-entity interoperability
//!    - CommonsPublic: Public indices and shared libraries
//!
//! # Enforcement
//!
//! The kernel enforces locality constraints at placement time:
//! - Tasks with `CellLocal` data cannot execute on federation nodes
//! - `Canonical` outputs from `Canonical` tasks are required for state mutation
//! - Cross-locality access triggers capability checks

use serde::{Deserialize, Serialize};

// ============================================================================
// Storage Class (E4)
// ============================================================================

/// Classification of storage by mutability and replication requirements.
///
/// This determines how data is stored, replicated, and accessed.
///
/// # Examples
///
/// ```
/// use icn_kernel_api::storage::StorageClass;
///
/// // Ledger entries must be canonical
/// let ledger = StorageClass::Canonical;
/// assert!(ledger.is_immutable());
/// assert!(ledger.requires_replication());
///
/// // App state is mutable
/// let app_state = StorageClass::ServiceState;
/// assert!(!app_state.is_immutable());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageClass {
    /// Immutable, replicated, authoritative data.
    ///
    /// Examples: ledger entries, governance receipts, trust edges.
    /// - Cannot be modified after creation
    /// - Replicated across the cooperative
    /// - Used for data that affects future constraints
    Canonical,

    /// Mutable, coop-scoped application state.
    ///
    /// Examples: calendars, inventory, schedules, user preferences.
    /// - Can be updated via CAS (compare-and-swap)
    /// - Scoped to the cooperative
    /// - Default for general compute tasks
    #[default]
    ServiceState,

    /// Content-addressed, locality-flexible binary data.
    ///
    /// Examples: documents, media files, backups, WASM modules.
    /// - Addressed by content hash (immutable by reference)
    /// - Locality rules determine placement
    /// - Can be cached/replicated based on access patterns
    Blobs,
}

impl StorageClass {
    /// Returns true if data in this class is immutable after creation.
    #[inline]
    pub const fn is_immutable(&self) -> bool {
        matches!(self, StorageClass::Canonical | StorageClass::Blobs)
    }

    /// Returns true if data requires active replication.
    #[inline]
    pub const fn requires_replication(&self) -> bool {
        matches!(self, StorageClass::Canonical)
    }

    /// Returns true if this is the default storage class.
    #[inline]
    pub const fn is_default(&self) -> bool {
        matches!(self, StorageClass::ServiceState)
    }

    /// Returns true if this class is suitable for canonical task outputs.
    ///
    /// Only `Canonical` storage can hold outputs that mutate ledger/governance state.
    #[inline]
    pub const fn can_hold_canonical_output(&self) -> bool {
        matches!(self, StorageClass::Canonical)
    }
}

impl std::fmt::Display for StorageClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageClass::Canonical => write!(f, "canonical"),
            StorageClass::ServiceState => write!(f, "service_state"),
            StorageClass::Blobs => write!(f, "blobs"),
        }
    }
}

// ============================================================================
// Data Locality (E4)
// ============================================================================

/// Data locality constraints for placement and access control.
///
/// Determines where data can reside and which tasks can access it.
///
/// # Ordering
///
/// Locality levels form a hierarchy from narrowest to widest:
/// `CellLocal < CoopReplicated < FederationMirrored < CommonsPublic`
///
/// # Access Rules
///
/// - Narrower data cannot be accessed by tasks at wider locality
/// - e.g., `CellLocal` data cannot be read by `FederationMirrored` tasks
///
/// # Examples
///
/// ```
/// use icn_kernel_api::storage::DataLocality;
///
/// let cell = DataLocality::CellLocal;
/// let fed = DataLocality::FederationMirrored;
///
/// // Cell data is narrower than federation
/// assert!(cell < fed);
///
/// // Federation tasks cannot access cell-local data
/// assert!(!fed.can_access(cell));
/// ```
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum DataLocality {
    /// Data must stay in the originating cell.
    ///
    /// Used for: sensitive local state, cell-specific configuration,
    /// data subject to local regulations.
    #[default]
    CellLocal = 0,

    /// Data is replicated across cooperative nodes.
    ///
    /// Used for: shared coop state, member data, internal services.
    CoopReplicated = 1,

    /// Data is mirrored for cross-entity interoperability.
    ///
    /// Used for: federated identity, cross-coop coordination,
    /// inter-cooperative agreements.
    FederationMirrored = 2,

    /// Data is publicly available in the commons.
    ///
    /// Used for: public indices, shared libraries, open datasets,
    /// commons-contributed resources.
    CommonsPublic = 3,
}

impl DataLocality {
    /// All locality levels in ascending order (narrowest to widest).
    pub const ALL: [DataLocality; 4] = [
        DataLocality::CellLocal,
        DataLocality::CoopReplicated,
        DataLocality::FederationMirrored,
        DataLocality::CommonsPublic,
    ];

    /// Returns true if a task with this locality can access data at the given locality.
    ///
    /// Tasks can only access data at their own locality or wider.
    /// e.g., `CoopReplicated` tasks can access `CoopReplicated` and `CommonsPublic` data,
    /// but not `CellLocal` data.
    #[inline]
    pub const fn can_access(&self, data_locality: DataLocality) -> bool {
        // Task locality must be <= data locality
        // (narrower or equal tasks can access, wider cannot)
        (*self as u8) <= (data_locality as u8)
    }

    /// Returns the next wider locality level, or `None` if already at `CommonsPublic`.
    pub fn widen(&self) -> Option<DataLocality> {
        match self {
            DataLocality::CellLocal => Some(DataLocality::CoopReplicated),
            DataLocality::CoopReplicated => Some(DataLocality::FederationMirrored),
            DataLocality::FederationMirrored => Some(DataLocality::CommonsPublic),
            DataLocality::CommonsPublic => None,
        }
    }

    /// Returns the next narrower locality level, or `None` if already at `CellLocal`.
    pub fn narrow(&self) -> Option<DataLocality> {
        match self {
            DataLocality::CellLocal => None,
            DataLocality::CoopReplicated => Some(DataLocality::CellLocal),
            DataLocality::FederationMirrored => Some(DataLocality::CoopReplicated),
            DataLocality::CommonsPublic => Some(DataLocality::FederationMirrored),
        }
    }

    /// Numeric value for serialization and constraint sets.
    #[inline]
    pub const fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// Parse from numeric value. Returns `None` for invalid values.
    pub fn from_u8(v: u8) -> Option<DataLocality> {
        match v {
            0 => Some(DataLocality::CellLocal),
            1 => Some(DataLocality::CoopReplicated),
            2 => Some(DataLocality::FederationMirrored),
            3 => Some(DataLocality::CommonsPublic),
            _ => None,
        }
    }

    /// Returns true if this is the narrowest locality (CellLocal).
    #[inline]
    pub const fn is_cell_local(&self) -> bool {
        matches!(self, DataLocality::CellLocal)
    }

    /// Returns true if this locality allows federation-wide access.
    #[inline]
    pub const fn allows_federation_access(&self) -> bool {
        matches!(
            self,
            DataLocality::FederationMirrored | DataLocality::CommonsPublic
        )
    }
}

impl std::fmt::Display for DataLocality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataLocality::CellLocal => write!(f, "cell_local"),
            DataLocality::CoopReplicated => write!(f, "coop_replicated"),
            DataLocality::FederationMirrored => write!(f, "federation_mirrored"),
            DataLocality::CommonsPublic => write!(f, "commons_public"),
        }
    }
}

// ============================================================================
// Validation Errors
// ============================================================================

/// Errors from storage specification validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StorageValidationError {
    /// Canonical task should use Canonical storage for outputs.
    #[error(
        "Canonical task should use StorageClass::Canonical for outputs (got {actual})"
    )]
    CanonicalTaskNonCanonicalStorage {
        /// The actual storage class specified
        actual: StorageClass,
    },

    /// Task locality cannot access required data locality.
    #[error("Task with locality {task_locality} cannot access {data_locality} data")]
    LocalityAccessViolation {
        /// The task's data locality
        task_locality: DataLocality,
        /// The data locality being accessed
        data_locality: DataLocality,
    },

    /// CellLocal data accessed by FederationMirrored task.
    #[error("CellLocal data cannot be accessed by tasks with FederationMirrored locality")]
    CellLocalFederationViolation,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- StorageClass tests ---

    #[test]
    fn test_storage_class_default() {
        let class: StorageClass = Default::default();
        assert_eq!(class, StorageClass::ServiceState);
        assert!(class.is_default());
    }

    #[test]
    fn test_storage_class_canonical() {
        let class = StorageClass::Canonical;
        assert!(class.is_immutable());
        assert!(class.requires_replication());
        assert!(class.can_hold_canonical_output());
        assert!(!class.is_default());
    }

    #[test]
    fn test_storage_class_service_state() {
        let class = StorageClass::ServiceState;
        assert!(!class.is_immutable());
        assert!(!class.requires_replication());
        assert!(!class.can_hold_canonical_output());
        assert!(class.is_default());
    }

    #[test]
    fn test_storage_class_blobs() {
        let class = StorageClass::Blobs;
        assert!(class.is_immutable()); // content-addressed = immutable by reference
        assert!(!class.requires_replication());
        assert!(!class.can_hold_canonical_output());
        assert!(!class.is_default());
    }

    #[test]
    fn test_storage_class_display() {
        assert_eq!(StorageClass::Canonical.to_string(), "canonical");
        assert_eq!(StorageClass::ServiceState.to_string(), "service_state");
        assert_eq!(StorageClass::Blobs.to_string(), "blobs");
    }

    #[test]
    fn test_storage_class_serde_roundtrip() {
        for class in [
            StorageClass::Canonical,
            StorageClass::ServiceState,
            StorageClass::Blobs,
        ] {
            let json = serde_json::to_string(&class).unwrap();
            let parsed: StorageClass = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, class);
        }
    }

    #[test]
    fn test_storage_class_serde_values() {
        assert_eq!(
            serde_json::to_string(&StorageClass::Canonical).unwrap(),
            r#""canonical""#
        );
        assert_eq!(
            serde_json::to_string(&StorageClass::ServiceState).unwrap(),
            r#""service_state""#
        );
        assert_eq!(
            serde_json::to_string(&StorageClass::Blobs).unwrap(),
            r#""blobs""#
        );
    }

    // --- DataLocality tests ---

    #[test]
    fn test_data_locality_default() {
        let locality: DataLocality = Default::default();
        assert_eq!(locality, DataLocality::CellLocal);
        assert!(locality.is_cell_local());
    }

    #[test]
    fn test_data_locality_ordering() {
        assert!(DataLocality::CellLocal < DataLocality::CoopReplicated);
        assert!(DataLocality::CoopReplicated < DataLocality::FederationMirrored);
        assert!(DataLocality::FederationMirrored < DataLocality::CommonsPublic);
    }

    #[test]
    fn test_data_locality_ordering_all_pairs() {
        for (i, a) in DataLocality::ALL.iter().enumerate() {
            for (j, b) in DataLocality::ALL.iter().enumerate() {
                assert_eq!(a < b, i < j, "{a} vs {b}");
                assert_eq!(a == b, i == j, "{a} vs {b}");
            }
        }
    }

    #[test]
    fn test_data_locality_can_access() {
        // CellLocal tasks can access all levels
        assert!(DataLocality::CellLocal.can_access(DataLocality::CellLocal));
        assert!(DataLocality::CellLocal.can_access(DataLocality::CoopReplicated));
        assert!(DataLocality::CellLocal.can_access(DataLocality::FederationMirrored));
        assert!(DataLocality::CellLocal.can_access(DataLocality::CommonsPublic));

        // CoopReplicated can access coop+ but not cell
        assert!(!DataLocality::CoopReplicated.can_access(DataLocality::CellLocal));
        assert!(DataLocality::CoopReplicated.can_access(DataLocality::CoopReplicated));
        assert!(DataLocality::CoopReplicated.can_access(DataLocality::FederationMirrored));
        assert!(DataLocality::CoopReplicated.can_access(DataLocality::CommonsPublic));

        // FederationMirrored can access federation+ but not cell/coop
        assert!(!DataLocality::FederationMirrored.can_access(DataLocality::CellLocal));
        assert!(!DataLocality::FederationMirrored.can_access(DataLocality::CoopReplicated));
        assert!(DataLocality::FederationMirrored.can_access(DataLocality::FederationMirrored));
        assert!(DataLocality::FederationMirrored.can_access(DataLocality::CommonsPublic));

        // CommonsPublic can only access commons
        assert!(!DataLocality::CommonsPublic.can_access(DataLocality::CellLocal));
        assert!(!DataLocality::CommonsPublic.can_access(DataLocality::CoopReplicated));
        assert!(!DataLocality::CommonsPublic.can_access(DataLocality::FederationMirrored));
        assert!(DataLocality::CommonsPublic.can_access(DataLocality::CommonsPublic));
    }

    #[test]
    fn test_data_locality_widen() {
        assert_eq!(
            DataLocality::CellLocal.widen(),
            Some(DataLocality::CoopReplicated)
        );
        assert_eq!(
            DataLocality::CoopReplicated.widen(),
            Some(DataLocality::FederationMirrored)
        );
        assert_eq!(
            DataLocality::FederationMirrored.widen(),
            Some(DataLocality::CommonsPublic)
        );
        assert_eq!(DataLocality::CommonsPublic.widen(), None);
    }

    #[test]
    fn test_data_locality_narrow() {
        assert_eq!(DataLocality::CellLocal.narrow(), None);
        assert_eq!(
            DataLocality::CoopReplicated.narrow(),
            Some(DataLocality::CellLocal)
        );
        assert_eq!(
            DataLocality::FederationMirrored.narrow(),
            Some(DataLocality::CoopReplicated)
        );
        assert_eq!(
            DataLocality::CommonsPublic.narrow(),
            Some(DataLocality::FederationMirrored)
        );
    }

    #[test]
    fn test_data_locality_widen_narrow_roundtrip() {
        for locality in &DataLocality::ALL {
            if let Some(wider) = locality.widen() {
                assert_eq!(wider.narrow(), Some(*locality));
            }
            if let Some(narrower) = locality.narrow() {
                assert_eq!(narrower.widen(), Some(*locality));
            }
        }
    }

    #[test]
    fn test_data_locality_as_u8_roundtrip() {
        for locality in &DataLocality::ALL {
            assert_eq!(DataLocality::from_u8(locality.as_u8()), Some(*locality));
        }
    }

    #[test]
    fn test_data_locality_from_u8_invalid() {
        assert_eq!(DataLocality::from_u8(4), None);
        assert_eq!(DataLocality::from_u8(255), None);
    }

    #[test]
    fn test_data_locality_as_u8_values() {
        assert_eq!(DataLocality::CellLocal.as_u8(), 0);
        assert_eq!(DataLocality::CoopReplicated.as_u8(), 1);
        assert_eq!(DataLocality::FederationMirrored.as_u8(), 2);
        assert_eq!(DataLocality::CommonsPublic.as_u8(), 3);
    }

    #[test]
    fn test_data_locality_allows_federation_access() {
        assert!(!DataLocality::CellLocal.allows_federation_access());
        assert!(!DataLocality::CoopReplicated.allows_federation_access());
        assert!(DataLocality::FederationMirrored.allows_federation_access());
        assert!(DataLocality::CommonsPublic.allows_federation_access());
    }

    #[test]
    fn test_data_locality_display() {
        assert_eq!(DataLocality::CellLocal.to_string(), "cell_local");
        assert_eq!(DataLocality::CoopReplicated.to_string(), "coop_replicated");
        assert_eq!(
            DataLocality::FederationMirrored.to_string(),
            "federation_mirrored"
        );
        assert_eq!(DataLocality::CommonsPublic.to_string(), "commons_public");
    }

    #[test]
    fn test_data_locality_serde_roundtrip() {
        for locality in &DataLocality::ALL {
            let json = serde_json::to_string(locality).unwrap();
            let parsed: DataLocality = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, locality);
        }
    }

    #[test]
    fn test_data_locality_serde_values() {
        assert_eq!(
            serde_json::to_string(&DataLocality::CellLocal).unwrap(),
            r#""cell_local""#
        );
        assert_eq!(
            serde_json::to_string(&DataLocality::CoopReplicated).unwrap(),
            r#""coop_replicated""#
        );
        assert_eq!(
            serde_json::to_string(&DataLocality::FederationMirrored).unwrap(),
            r#""federation_mirrored""#
        );
        assert_eq!(
            serde_json::to_string(&DataLocality::CommonsPublic).unwrap(),
            r#""commons_public""#
        );
    }

    // --- Validation error tests ---

    #[test]
    fn test_validation_error_display() {
        let err = StorageValidationError::CanonicalTaskNonCanonicalStorage {
            actual: StorageClass::ServiceState,
        };
        assert!(err.to_string().contains("Canonical task"));
        assert!(err.to_string().contains("service_state"));

        let err = StorageValidationError::LocalityAccessViolation {
            task_locality: DataLocality::FederationMirrored,
            data_locality: DataLocality::CellLocal,
        };
        assert!(err.to_string().contains("federation_mirrored"));
        assert!(err.to_string().contains("cell_local"));

        let err = StorageValidationError::CellLocalFederationViolation;
        assert!(err.to_string().contains("CellLocal"));
        assert!(err.to_string().contains("FederationMirrored"));
    }
}
