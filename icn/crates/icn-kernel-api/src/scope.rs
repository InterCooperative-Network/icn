//! Scope and Cell primitives for multi-scale cooperative computing.
//!
//! This module defines `ScopeLevel` and `CellId` — kernel primitives that
//! enable spatial awareness without introducing domain semantics.
//!
//! # Design
//!
//! The kernel knows that `Federation` is "wider" than `Org`, but it does NOT
//! know what an organization *is*. That interpretation stays in apps,
//! consistent with the Meaning Firewall.
//!
//! See `docs/architecture/CELLS_AND_SCOPES.md` for the full specification.

use serde::{Deserialize, Serialize};

// ============================================================================
// ScopeLevel
// ============================================================================

/// Scope level for operations, data, and services.
///
/// Defines how far an operation should reach in the network hierarchy.
/// The kernel uses this for routing, replication, and capacity allocation
/// without interpreting the domain semantics of each level.
///
/// # Ordering
///
/// `ScopeLevel` implements `Ord`: `Local < Cell < Org < Federation < Commons`.
/// This ordering is used for hierarchical fallback in placement and discovery.
///
/// # Examples
///
/// ```
/// use icn_kernel_api::scope::ScopeLevel;
///
/// assert!(ScopeLevel::Cell < ScopeLevel::Org);
/// assert!(ScopeLevel::Org.includes(ScopeLevel::Cell));
/// assert_eq!(ScopeLevel::Cell.widen(), Some(ScopeLevel::Org));
/// assert_eq!(ScopeLevel::Commons.widen(), None);
/// ```
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum ScopeLevel {
    /// This node only — no network involvement.
    ///
    /// This is the default scope level. Defaulting to the narrowest scope
    /// prevents accidental data or request leakage to wider audiences.
    #[default]
    Local = 0,
    /// Nodes within the same cell (HA cluster).
    Cell = 1,
    /// Cells within the same organization.
    Org = 2,
    /// Organizations within the same federation.
    Federation = 3,
    /// All reachable nodes (public commons).
    Commons = 4,
}

impl ScopeLevel {
    /// All scope levels in ascending order.
    pub const ALL: [ScopeLevel; 5] = [
        ScopeLevel::Local,
        ScopeLevel::Cell,
        ScopeLevel::Org,
        ScopeLevel::Federation,
        ScopeLevel::Commons,
    ];

    /// Return the next wider scope, or `None` if already at `Commons`.
    pub fn widen(&self) -> Option<ScopeLevel> {
        match self {
            ScopeLevel::Local => Some(ScopeLevel::Cell),
            ScopeLevel::Cell => Some(ScopeLevel::Org),
            ScopeLevel::Org => Some(ScopeLevel::Federation),
            ScopeLevel::Federation => Some(ScopeLevel::Commons),
            ScopeLevel::Commons => None,
        }
    }

    /// Return the next narrower scope, or `None` if already at `Local`.
    pub fn narrow(&self) -> Option<ScopeLevel> {
        match self {
            ScopeLevel::Local => None,
            ScopeLevel::Cell => Some(ScopeLevel::Local),
            ScopeLevel::Org => Some(ScopeLevel::Cell),
            ScopeLevel::Federation => Some(ScopeLevel::Org),
            ScopeLevel::Commons => Some(ScopeLevel::Federation),
        }
    }

    /// Check if this scope includes another scope.
    ///
    /// A wider scope includes all narrower scopes.
    /// e.g., `Org.includes(Cell)` is `true`, `Cell.includes(Org)` is `false`.
    pub fn includes(&self, other: ScopeLevel) -> bool {
        *self >= other
    }

    /// Numeric value for serialization and constraint sets.
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// Parse from numeric value. Returns `None` for invalid values.
    pub fn from_u8(v: u8) -> Option<ScopeLevel> {
        match v {
            0 => Some(ScopeLevel::Local),
            1 => Some(ScopeLevel::Cell),
            2 => Some(ScopeLevel::Org),
            3 => Some(ScopeLevel::Federation),
            4 => Some(ScopeLevel::Commons),
            _ => None,
        }
    }
}

impl std::fmt::Display for ScopeLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScopeLevel::Local => write!(f, "local"),
            ScopeLevel::Cell => write!(f, "cell"),
            ScopeLevel::Org => write!(f, "org"),
            ScopeLevel::Federation => write!(f, "federation"),
            ScopeLevel::Commons => write!(f, "commons"),
        }
    }
}

// ============================================================================
// CellId
// ============================================================================

/// Unique cell identifier — deterministic hash of scope + name + salt.
///
/// A `CellId` is derived from three components:
/// - `scope_id`: Parent scope identifier (org DID, federation ID, etc.)
/// - `cell_name`: Human-readable name within the scope
/// - `genesis_salt`: 32-byte random value from cell creation
///
/// Two cells with the same parent scope and name but different genesis
/// salts produce different `CellId`s (intentional for re-creation scenarios).
///
/// # Display Format
///
/// `CellId` displays as `cell:<64-hex-chars>` (full 32 bytes).
/// Using hex rather than base58 to avoid an additional dependency in the
/// kernel crate and to provide consistent, fixed-width output for logging.
///
/// # Examples
///
/// ```
/// use icn_kernel_api::scope::CellId;
///
/// let salt = [42u8; 32];
/// let id = CellId::derive(b"did:icn:org123", "workshop", &salt);
/// assert_eq!(id, CellId::derive(b"did:icn:org123", "workshop", &salt)); // deterministic
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellId(pub [u8; 32]);

impl CellId {
    /// Derive a `CellId` from its components using blake3.
    ///
    /// # Arguments
    ///
    /// - `scope_id`: Parent scope identifier (org DID bytes, federation ID, etc.)
    /// - `cell_name`: Human-readable name within the scope
    /// - `genesis_salt`: 32-byte random value set at cell creation time
    pub fn derive(scope_id: &[u8], cell_name: &str, genesis_salt: &[u8; 32]) -> Self {
        let mut hasher = blake3::Hasher::new();
        // Domain separation: prefix with lengths to prevent ambiguous concatenation
        hasher.update(&(scope_id.len() as u32).to_le_bytes());
        hasher.update(scope_id);
        hasher.update(&(cell_name.len() as u32).to_le_bytes());
        hasher.update(cell_name.as_bytes());
        hasher.update(genesis_salt);
        CellId(*hasher.finalize().as_bytes())
    }

    /// Return the raw 32-byte hash.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for CellId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CellId({})", self)
    }
}

impl std::fmt::Display for CellId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Full 32 bytes as hex — fixed-width, no truncation ambiguity
        write!(f, "cell:")?;
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

// ============================================================================
// Mock Implementation (for testing)
// ============================================================================

/// A simple in-memory [`CellService`](crate::services::CellService) implementation for use in tests.
///
/// # Examples
///
/// ```
/// use icn_kernel_api::scope::{CellId, MockCellService, ScopeLevel};
/// use icn_kernel_api::CellService;
///
/// let cell_id = CellId::derive(b"org", "test", &[0u8; 32]);
/// let svc = MockCellService::new(Some(cell_id));
/// assert_eq!(svc.local_cell(), Some(cell_id));
/// ```
#[derive(Debug, Clone)]
pub struct MockCellService {
    /// The cell this "node" belongs to, if any.
    pub local_cell_id: Option<CellId>,
    /// Members of the local cell.
    pub members: Vec<crate::types::Did>,
    /// DIDs considered to be in the same org (but possibly different cell).
    pub org_peers: Vec<crate::types::Did>,
}

impl MockCellService {
    /// Create a new mock with an optional local cell.
    pub fn new(local_cell_id: Option<CellId>) -> Self {
        Self {
            local_cell_id,
            members: Vec::new(),
            org_peers: Vec::new(),
        }
    }

    /// Add a cell member.
    pub fn with_member(mut self, did: crate::types::Did) -> Self {
        self.members.push(did);
        self
    }

    /// Add an org peer (same org, possibly different cell).
    pub fn with_org_peer(mut self, did: crate::types::Did) -> Self {
        self.org_peers.push(did);
        self
    }
}

impl crate::services::CellService for MockCellService {
    fn local_cell(&self) -> Option<CellId> {
        self.local_cell_id
    }

    fn cell_scope(&self, cell_id: &CellId) -> Option<ScopeLevel> {
        if self.local_cell_id.as_ref() == Some(cell_id) {
            Some(ScopeLevel::Cell)
        } else {
            None
        }
    }

    fn cell_members(&self, cell_id: &CellId) -> Vec<crate::types::Did> {
        if self.local_cell_id.as_ref() == Some(cell_id) {
            self.members.clone()
        } else {
            Vec::new()
        }
    }

    fn is_cell_peer(&self, did: &crate::types::Did) -> bool {
        self.members.iter().any(|m| m == did)
    }

    fn is_org_peer(&self, did: &crate::types::Did) -> bool {
        self.members.iter().any(|m| m == did) || self.org_peers.iter().any(|p| p == did)
    }

    fn peer_scope(&self, did: &crate::types::Did) -> ScopeLevel {
        if self.members.iter().any(|m| m == did) {
            ScopeLevel::Cell
        } else if self.org_peers.iter().any(|p| p == did) {
            ScopeLevel::Org
        } else {
            ScopeLevel::Commons
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- ScopeLevel ordering ---

    #[test]
    fn test_scope_level_ordering() {
        assert!(ScopeLevel::Local < ScopeLevel::Cell);
        assert!(ScopeLevel::Cell < ScopeLevel::Org);
        assert!(ScopeLevel::Org < ScopeLevel::Federation);
        assert!(ScopeLevel::Federation < ScopeLevel::Commons);
    }

    #[test]
    fn test_scope_level_ordering_all_pairs() {
        for (i, a) in ScopeLevel::ALL.iter().enumerate() {
            for (j, b) in ScopeLevel::ALL.iter().enumerate() {
                assert_eq!(a < b, i < j, "{a} vs {b}");
                assert_eq!(a == b, i == j, "{a} vs {b}");
            }
        }
    }

    // --- widen / narrow ---

    #[test]
    fn test_widen_chain() {
        assert_eq!(ScopeLevel::Local.widen(), Some(ScopeLevel::Cell));
        assert_eq!(ScopeLevel::Cell.widen(), Some(ScopeLevel::Org));
        assert_eq!(ScopeLevel::Org.widen(), Some(ScopeLevel::Federation));
        assert_eq!(ScopeLevel::Federation.widen(), Some(ScopeLevel::Commons));
        assert_eq!(ScopeLevel::Commons.widen(), None);
    }

    #[test]
    fn test_narrow_chain() {
        assert_eq!(ScopeLevel::Commons.narrow(), Some(ScopeLevel::Federation));
        assert_eq!(ScopeLevel::Federation.narrow(), Some(ScopeLevel::Org));
        assert_eq!(ScopeLevel::Org.narrow(), Some(ScopeLevel::Cell));
        assert_eq!(ScopeLevel::Cell.narrow(), Some(ScopeLevel::Local));
        assert_eq!(ScopeLevel::Local.narrow(), None);
    }

    #[test]
    fn test_widen_narrow_roundtrip() {
        for scope in &ScopeLevel::ALL {
            if let Some(wider) = scope.widen() {
                assert_eq!(wider.narrow(), Some(*scope));
            }
            if let Some(narrower) = scope.narrow() {
                assert_eq!(narrower.widen(), Some(*scope));
            }
        }
    }

    // --- includes ---

    #[test]
    fn test_includes() {
        assert!(ScopeLevel::Org.includes(ScopeLevel::Cell));
        assert!(ScopeLevel::Org.includes(ScopeLevel::Org));
        assert!(!ScopeLevel::Cell.includes(ScopeLevel::Org));
        assert!(ScopeLevel::Commons.includes(ScopeLevel::Local));
        assert!(!ScopeLevel::Local.includes(ScopeLevel::Commons));
    }

    // --- as_u8 / from_u8 ---

    #[test]
    fn test_as_u8_roundtrip() {
        for scope in &ScopeLevel::ALL {
            assert_eq!(ScopeLevel::from_u8(scope.as_u8()), Some(*scope));
        }
    }

    #[test]
    fn test_from_u8_invalid() {
        assert_eq!(ScopeLevel::from_u8(5), None);
        assert_eq!(ScopeLevel::from_u8(255), None);
    }

    #[test]
    fn test_as_u8_values() {
        assert_eq!(ScopeLevel::Local.as_u8(), 0);
        assert_eq!(ScopeLevel::Cell.as_u8(), 1);
        assert_eq!(ScopeLevel::Org.as_u8(), 2);
        assert_eq!(ScopeLevel::Federation.as_u8(), 3);
        assert_eq!(ScopeLevel::Commons.as_u8(), 4);
    }

    // --- Display ---

    #[test]
    fn test_display() {
        assert_eq!(ScopeLevel::Local.to_string(), "local");
        assert_eq!(ScopeLevel::Cell.to_string(), "cell");
        assert_eq!(ScopeLevel::Org.to_string(), "org");
        assert_eq!(ScopeLevel::Federation.to_string(), "federation");
        assert_eq!(ScopeLevel::Commons.to_string(), "commons");
    }

    // --- Default ---

    #[test]
    fn test_default() {
        assert_eq!(ScopeLevel::default(), ScopeLevel::Local);
    }

    // --- Serde ---

    #[test]
    fn test_scope_level_serde_roundtrip() {
        for scope in &ScopeLevel::ALL {
            let json = serde_json::to_string(scope).unwrap();
            let parsed: ScopeLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, scope);
        }
    }

    // --- CellId ---

    #[test]
    fn test_cell_id_derive_deterministic() {
        let salt = [42u8; 32];
        let id1 = CellId::derive(b"did:icn:org123", "workshop", &salt);
        let id2 = CellId::derive(b"did:icn:org123", "workshop", &salt);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_cell_id_different_scope() {
        let salt = [42u8; 32];
        let id1 = CellId::derive(b"did:icn:org123", "workshop", &salt);
        let id2 = CellId::derive(b"did:icn:org456", "workshop", &salt);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_cell_id_different_name() {
        let salt = [42u8; 32];
        let id1 = CellId::derive(b"did:icn:org123", "workshop", &salt);
        let id2 = CellId::derive(b"did:icn:org123", "garden", &salt);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_cell_id_different_salt() {
        let salt1 = [42u8; 32];
        let salt2 = [99u8; 32];
        let id1 = CellId::derive(b"did:icn:org123", "workshop", &salt1);
        let id2 = CellId::derive(b"did:icn:org123", "workshop", &salt2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_cell_id_no_concatenation_ambiguity() {
        // "ab" + "cd" should differ from "abc" + "d"
        let salt = [0u8; 32];
        let id1 = CellId::derive(b"ab", "cd", &salt);
        let id2 = CellId::derive(b"abc", "d", &salt);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_cell_id_display() {
        let salt = [0u8; 32];
        let id = CellId::derive(b"test", "test", &salt);
        let display = id.to_string();
        assert!(display.starts_with("cell:"));
        // "cell:" + 64 hex chars (32 bytes, full hash, no truncation)
        assert_eq!(display.len(), 5 + 64);
    }

    #[test]
    fn test_cell_id_debug() {
        let salt = [0u8; 32];
        let id = CellId::derive(b"test", "test", &salt);
        let debug = format!("{:?}", id);
        assert!(debug.starts_with("CellId(cell:"));
    }

    #[test]
    fn test_cell_id_serde_roundtrip() {
        let salt = [42u8; 32];
        let id = CellId::derive(b"did:icn:org123", "workshop", &salt);
        let json = serde_json::to_string(&id).unwrap();
        let parsed: CellId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_cell_id_as_bytes() {
        let salt = [42u8; 32];
        let id = CellId::derive(b"test", "test", &salt);
        assert_eq!(id.as_bytes().len(), 32);
        assert_eq!(id.as_bytes(), &id.0);
    }

    // --- MockCellService ---

    #[test]
    fn test_mock_cell_service_no_cell() {
        use crate::services::CellService;

        let svc = MockCellService::new(None);
        assert_eq!(svc.local_cell(), None);

        let did: crate::types::Did = "did:icn:alice".into();
        assert!(!svc.is_cell_peer(&did));
        assert!(!svc.is_org_peer(&did));
        assert_eq!(svc.peer_scope(&did), ScopeLevel::Commons);
    }

    #[test]
    fn test_mock_cell_service_with_members() {
        use crate::services::CellService;

        let cell_id = CellId::derive(b"org", "test", &[0u8; 32]);
        let alice: crate::types::Did = "did:icn:alice".into();
        let bob: crate::types::Did = "did:icn:bob".into();
        let carol: crate::types::Did = "did:icn:carol".into();

        let svc = MockCellService::new(Some(cell_id))
            .with_member(alice.clone())
            .with_org_peer(bob.clone());

        assert_eq!(svc.local_cell(), Some(cell_id));
        assert!(svc.is_cell_peer(&alice));
        assert!(!svc.is_cell_peer(&bob)); // org peer, not cell peer
        assert!(svc.is_org_peer(&alice)); // cell peers are also org peers
        assert!(svc.is_org_peer(&bob));
        assert!(!svc.is_org_peer(&carol));

        assert_eq!(svc.peer_scope(&alice), ScopeLevel::Cell);
        assert_eq!(svc.peer_scope(&bob), ScopeLevel::Org);
        assert_eq!(svc.peer_scope(&carol), ScopeLevel::Commons);

        assert_eq!(svc.cell_members(&cell_id), vec![alice]);

        // Unknown cell returns empty members
        let other_cell = CellId::derive(b"other", "cell", &[1u8; 32]);
        assert!(svc.cell_members(&other_cell).is_empty());
        assert_eq!(svc.cell_scope(&other_cell), None);
        assert_eq!(svc.cell_scope(&cell_id), Some(ScopeLevel::Cell));
    }
}
