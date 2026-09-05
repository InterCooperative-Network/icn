//! Listings Manager for Cooperative Exchange
//!
//! Manages listings for internal exchange between cooperatives.
//! This enables coops to post offers and wants before going external,
//! keeping value circulating within the network.

use anyhow::Result;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::warn;
use uuid::Uuid;

/// Unique listing identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ListingId(pub Uuid);

impl ListingId {
    /// Generate a new random listing ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from an existing UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for ListingId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ListingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ListingId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Type of listing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListingType {
    /// Offering something
    Offer,
    /// Looking for something
    Want,
}

impl ListingType {
    /// Return the string representation of the listing type
    pub fn as_str(&self) -> &str {
        match self {
            Self::Offer => "offer",
            Self::Want => "want",
        }
    }
}

/// Category of listing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListingCategory {
    /// Equipment, machinery, tools
    Equipment,
    /// Services offered or needed
    Services,
    /// Raw materials, supplies
    Materials,
    /// Physical space, office, storage
    Space,
    /// Other
    Other(String),
}

impl ListingCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Equipment => "equipment",
            Self::Services => "services",
            Self::Materials => "materials",
            Self::Space => "space",
            Self::Other(s) => s,
        }
    }

    pub fn from_string(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "equipment" => Self::Equipment,
            "services" => Self::Services,
            "materials" => Self::Materials,
            "space" => Self::Space,
            _ => Self::Other(s.to_string()),
        }
    }
}

/// Visibility scope of a listing
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListingVisibility {
    /// Only visible within the coop
    Coop,
    /// Visible to federation members
    #[default]
    Federation,
    /// Visible to all network participants
    Network,
}

/// Status of a listing
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListingStatus {
    /// Active and available
    #[default]
    Active,
    /// Matched with someone but not completed
    Matched,
    /// Exchange completed
    Completed,
    /// No longer available
    Expired,
    /// Cancelled by owner
    Cancelled,
}

/// A listing for internal exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listing {
    /// Unique identifier
    pub id: ListingId,
    /// Type: offer or want
    pub listing_type: ListingType,
    /// Short title
    pub title: String,
    /// Full description
    pub description: String,
    /// Category
    pub category: ListingCategory,
    /// Photo URLs or IPFS hashes
    pub photos: Vec<String>,
    /// Who posted this listing
    pub offered_by: Did,
    /// Which coop this listing belongs to
    pub coop_id: String,
    /// What they're looking for in exchange
    pub seeking: String,
    /// Visibility scope
    pub visibility: ListingVisibility,
    /// Current status
    pub status: ListingStatus,
    /// When created (Unix timestamp)
    pub created_at: u64,
    /// When last updated
    pub updated_at: u64,
    /// When this expires (optional)
    pub expires_at: Option<u64>,
    /// Tags for searchability
    pub tags: Vec<String>,
}

impl Listing {
    /// Create a new listing
    pub fn new(
        listing_type: ListingType,
        title: String,
        description: String,
        category: ListingCategory,
        offered_by: Did,
        coop_id: String,
        seeking: String,
        now: u64,
    ) -> Self {
        Self {
            id: ListingId::new(),
            listing_type,
            title,
            description,
            category,
            photos: Vec::new(),
            offered_by,
            coop_id,
            seeking,
            visibility: ListingVisibility::default(),
            status: ListingStatus::default(),
            created_at: now,
            updated_at: now,
            expires_at: None,
            tags: Vec::new(),
        }
    }

    /// Check if listing is active
    pub fn is_active(&self) -> bool {
        self.status == ListingStatus::Active
    }

    /// Check if listing is expired based on current time
    pub fn is_expired(&self, now: u64) -> bool {
        if let Some(expires_at) = self.expires_at {
            now > expires_at
        } else {
            false
        }
    }

    /// Mark as matched
    pub fn mark_matched(&mut self, now: u64) {
        self.status = ListingStatus::Matched;
        self.updated_at = now;
    }

    /// Mark as completed
    pub fn mark_completed(&mut self, now: u64) {
        self.status = ListingStatus::Completed;
        self.updated_at = now;
    }

    /// Cancel the listing
    pub fn cancel(&mut self, now: u64) {
        self.status = ListingStatus::Cancelled;
        self.updated_at = now;
    }
}

/// Expression of interest in a listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListingInterest {
    /// Unique identifier
    pub id: Uuid,
    /// Which listing this is for
    pub listing_id: ListingId,
    /// Who expressed interest
    pub from_did: Did,
    /// Which coop they're from
    pub from_coop: String,
    /// Message to the listing owner
    pub message: String,
    /// What they're offering in exchange (if any)
    pub offer: Option<String>,
    /// When expressed
    pub created_at: u64,
}

impl ListingInterest {
    pub fn new(
        listing_id: ListingId,
        from_did: Did,
        from_coop: String,
        message: String,
        offer: Option<String>,
        now: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            listing_id,
            from_did,
            from_coop,
            message,
            offer,
            created_at: now,
        }
    }
}

/// Maximum items per page for pagination
pub const MAX_PAGE_SIZE: usize = 100;
/// Default page size
pub const DEFAULT_PAGE_SIZE: usize = 20;
/// Maximum offset for pagination (prevents DoS via huge skip values)
pub const MAX_OFFSET: usize = 100_000;

/// Sorting options for listings
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ListingSortBy {
    /// Sort by creation time (default, newest first)
    #[default]
    CreatedAt,
    /// Sort by status (Active > Matched > Completed > Cancelled > Expired)
    Status,
    /// Sort by category alphabetically
    Category,
    /// Sort by listing type (Offer before Want)
    ListingType,
}

/// Helper function to sort listings based on the sort option
fn sort_listings(listings: &mut [Listing], sort_by: ListingSortBy) {
    match sort_by {
        ListingSortBy::CreatedAt => {
            // Newest first
            listings.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        }
        ListingSortBy::Status => {
            // Active > Matched > Completed > Cancelled > Expired, then by created_at
            listings.sort_by(|a, b| {
                let status_order = |s: &ListingStatus| match s {
                    ListingStatus::Active => 0,
                    ListingStatus::Matched => 1,
                    ListingStatus::Completed => 2,
                    ListingStatus::Cancelled => 3,
                    ListingStatus::Expired => 4,
                };
                status_order(&a.status)
                    .cmp(&status_order(&b.status))
                    .then_with(|| b.created_at.cmp(&a.created_at))
            });
        }
        ListingSortBy::Category => {
            // Alphabetically by category, then by created_at
            listings.sort_by(|a, b| {
                a.category
                    .as_str()
                    .cmp(b.category.as_str())
                    .then_with(|| b.created_at.cmp(&a.created_at))
            });
        }
        ListingSortBy::ListingType => {
            // Offer before Want, then by created_at
            listings.sort_by(|a, b| {
                let type_order = |t: &ListingType| match t {
                    ListingType::Offer => 0,
                    ListingType::Want => 1,
                };
                type_order(&a.listing_type)
                    .cmp(&type_order(&b.listing_type))
                    .then_with(|| b.created_at.cmp(&a.created_at))
            });
        }
    }
}

/// Filter criteria for listings.
///
/// # Pagination
///
/// - `limit`: Maximum results per page. Defaults to [`DEFAULT_PAGE_SIZE`] (20) if `None`.
///   Capped at [`MAX_PAGE_SIZE`] (100) even if a higher value is requested.
/// - `offset`: Number of results to skip. Defaults to 0 if `None`.
///
/// # Example
///
/// ```ignore
/// let filter = ListingFilter {
///     status: Some(ListingStatus::Active),
///     limit: Some(50),  // Get 50 results
///     offset: Some(100), // Skip first 100 (page 3 of 50-per-page)
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct ListingFilter {
    /// Filter by type
    pub listing_type: Option<ListingType>,
    /// Filter by category
    pub category: Option<ListingCategory>,
    /// Filter by coop
    pub coop_id: Option<String>,
    /// Filter by status
    pub status: Option<ListingStatus>,
    /// Filter by tag
    pub tag: Option<String>,
    /// Filter by owner (for "my listings" queries)
    pub offered_by: Option<Did>,
    /// Filter by visibility level
    pub visibility: Option<ListingVisibility>,
    /// Only show active listings
    pub active_only: bool,
    /// Maximum number of results (for pagination).
    /// Defaults to [`DEFAULT_PAGE_SIZE`] (20), capped at [`MAX_PAGE_SIZE`] (100).
    pub limit: Option<usize>,
    /// Number of results to skip (for pagination). Defaults to 0.
    pub offset: Option<usize>,
    /// Sort order for results
    pub sort_by: ListingSortBy,
}

impl ListingFilter {
    pub fn matches(&self, listing: &Listing) -> bool {
        self.matches_at(listing, icn_time::current_timestamp_secs())
    }

    /// Check if a listing matches at a specific timestamp.
    /// Useful for testing with controlled time.
    pub fn matches_at(&self, listing: &Listing, now: u64) -> bool {
        if let Some(ref lt) = self.listing_type {
            if listing.listing_type != *lt {
                return false;
            }
        }
        if let Some(ref cat) = self.category {
            if listing.category != *cat {
                return false;
            }
        }
        if let Some(ref coop) = self.coop_id {
            if &listing.coop_id != coop {
                return false;
            }
        }
        if let Some(status) = self.status {
            if listing.status != status {
                return false;
            }
        }
        if let Some(ref tag) = self.tag {
            if !listing.tags.contains(tag) {
                return false;
            }
        }
        if let Some(ref owner) = self.offered_by {
            if &listing.offered_by != owner {
                return false;
            }
        }
        if let Some(ref vis) = self.visibility {
            if &listing.visibility != vis {
                return false;
            }
        }
        if self.active_only && !listing.is_active() {
            return false;
        }
        // Filter out expired active listings - they appear stale to users
        if listing.status == ListingStatus::Active && listing.is_expired(now) {
            return false;
        }
        true
    }
}

/// In-memory listings store for testing purposes only.
///
/// # Warning
/// This store is NOT suitable for production use. It lacks:
/// - Persistence (data is lost on restart)
/// - True atomic compare-and-swap operations (uses RwLock instead)
///
/// For production, use [`SledListingsStore`] which provides:
/// - Persistent storage with versioned keys for migrations
/// - True atomic CAS operations for duplicate prevention
///
/// # Concurrency Model
/// The current implementation uses RwLock for exclusive write access, which prevents
/// lost updates within a single process. For horizontal scaling (multiple gateway
/// instances), version-based optimistic locking would be needed. This is deferred
/// as the pilot phase uses a single gateway instance.
#[derive(Default)]
pub struct InMemoryListingsStore {
    listings: RwLock<HashMap<ListingId, Listing>>,
    interests: RwLock<HashMap<ListingId, Vec<ListingInterest>>>,
}

impl InMemoryListingsStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Save a listing
    pub fn save(&self, listing: &Listing) -> Result<()> {
        let mut listings = self
            .listings
            .write()
            .map_err(|_| anyhow::anyhow!("Listings storage lock poisoned"))?;
        listings.insert(listing.id, listing.clone());
        Ok(())
    }

    /// Get a listing by ID
    pub fn get(&self, id: &ListingId) -> Result<Option<Listing>> {
        let listings = self
            .listings
            .read()
            .map_err(|_| anyhow::anyhow!("Listings storage lock poisoned"))?;
        Ok(listings.get(id).cloned())
    }

    /// List all listings matching a filter with pagination
    pub fn list(&self, filter: &ListingFilter) -> Result<Vec<Listing>> {
        let listings = self
            .listings
            .read()
            .map_err(|_| anyhow::anyhow!("Listings storage lock poisoned"))?;

        let mut result: Vec<_> = listings
            .values()
            .filter(|l| filter.matches(l))
            .cloned()
            .collect();

        // Apply sorting based on filter
        sort_listings(&mut result, filter.sort_by);

        // Apply pagination with validation logging
        let requested_offset = filter.offset.unwrap_or(0);
        let offset = requested_offset.min(MAX_OFFSET);
        let requested_limit = filter.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        let limit = requested_limit.min(MAX_PAGE_SIZE);

        // Log if offset was capped (potential DoS attempt)
        if requested_offset > MAX_OFFSET {
            tracing::warn!(
                requested = requested_offset,
                actual = offset,
                "Pagination offset capped to MAX_OFFSET (potential DoS attempt)"
            );
        }

        // Log if limit was capped
        if requested_limit > MAX_PAGE_SIZE {
            tracing::debug!(
                requested = requested_limit,
                actual = limit,
                "Pagination limit capped to MAX_PAGE_SIZE"
            );
        }

        // Log if offset is larger than result set (empty response)
        let total_matching = result.len();
        if offset >= total_matching && !result.is_empty() {
            tracing::debug!(
                offset = offset,
                total_matching = total_matching,
                "Pagination offset exceeds result count - returning empty results"
            );
        }

        Ok(result.into_iter().skip(offset).take(limit).collect())
    }

    /// Delete a listing and its associated interests
    pub fn delete(&self, id: &ListingId) -> Result<bool> {
        let mut listings = self
            .listings
            .write()
            .map_err(|_| anyhow::anyhow!("Listings storage lock poisoned"))?;

        // Also clean up any associated interests to prevent orphaned data
        let mut interests = self
            .interests
            .write()
            .map_err(|_| anyhow::anyhow!("Interests storage lock poisoned"))?;
        interests.remove(id);

        Ok(listings.remove(id).is_some())
    }

    /// Add interest in a listing
    pub fn add_interest(&self, interest: &ListingInterest) -> Result<()> {
        let mut interests = self
            .interests
            .write()
            .map_err(|_| anyhow::anyhow!("Interests storage lock poisoned"))?;
        interests
            .entry(interest.listing_id)
            .or_default()
            .push(interest.clone());
        Ok(())
    }

    /// Get interests for a listing
    pub fn get_interests(&self, listing_id: &ListingId) -> Result<Vec<ListingInterest>> {
        let interests = self
            .interests
            .read()
            .map_err(|_| anyhow::anyhow!("Interests storage lock poisoned"))?;
        Ok(interests.get(listing_id).cloned().unwrap_or_default())
    }

    /// Atomically add interest if user hasn't already expressed interest.
    /// Returns Ok(true) if added, Ok(false) if duplicate.
    pub fn add_interest_if_not_duplicate(
        &self,
        interest: &ListingInterest,
        from_did: &Did,
    ) -> Result<bool> {
        let mut interests = self
            .interests
            .write()
            .map_err(|_| anyhow::anyhow!("Interests storage lock poisoned"))?;

        let existing = interests.entry(interest.listing_id).or_default();

        // Check for duplicate under the same write lock
        if existing.iter().any(|i| &i.from_did == from_did) {
            return Ok(false);
        }

        existing.push(interest.clone());
        Ok(true)
    }

    /// List all listings with a specific status (ignores pagination and expiry filtering).
    ///
    /// Used for background tasks like expiring stale listings.
    ///
    /// # Performance
    ///
    /// This method performs a full table scan (O(n) where n = total listings).
    /// For deployments with >10K listings, schedule execution during off-peak hours
    /// or add a secondary index on (status, expires_at) for efficient queries.
    ///
    /// Current pilot phase supports up to 10K listings efficiently.
    ///
    /// # Difference from `list()`
    ///
    /// Unlike `list()`, this method does NOT filter out expired Active listings.
    /// This is intentional: when expiring stale listings, we need to find Active
    /// listings that have expired, which `list()` would hide.
    pub fn list_all_matching_status(&self, status: ListingStatus) -> Result<Vec<Listing>> {
        let listings = self
            .listings
            .read()
            .map_err(|_| anyhow::anyhow!("Listings storage lock poisoned"))?;

        Ok(listings
            .values()
            .filter(|l| l.status == status)
            .cloned()
            .collect())
    }

    /// Delete all interests for a listing.
    ///
    /// Used when expiring or cancelling listings to prevent stale data accumulation
    /// and privacy leaks from retained buyer/interest information.
    ///
    /// Returns the number of interests deleted.
    pub fn delete_interests(&self, listing_id: &ListingId) -> Result<usize> {
        let mut interests = self
            .interests
            .write()
            .map_err(|_| anyhow::anyhow!("Interests storage lock poisoned"))?;

        if let Some(removed) = interests.remove(listing_id) {
            Ok(removed.len())
        } else {
            Ok(0)
        }
    }
}

/// Sled key version for schema migration support.
/// When the schema changes, increment this version and add migration logic.
const SLED_KEY_VERSION: &str = "v1";

/// The value every `interest_idx` row the writer produces carries.
///
/// The row is a uniqueness marker, not a pointer: it holds this sentinel and
/// never the interest id. A row under the namespace carrying anything else was
/// not written by [`SledListingsStore::add_interest_if_not_duplicate`], so it is
/// evidence this code cannot interpret rather than evidence it may skip
/// (#2627 M4b).
const INTEREST_INDEX_PRESENT: &[u8] = b"1";

/// Why a physical `interest_idx` row could not be proven to name — or not to
/// name — a given principal.
///
/// Every arm is a state the writer cannot produce. They are named individually
/// so a refusal says which one occurred without quoting the row: the reason
/// class reaches the HTTP body, and a DID spelling or an interest message must
/// not (#2627 M4b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UniquenessEvidenceDefect {
    /// The key is not UTF-8, so it holds no readable spelling.
    KeyNotUtf8,
    /// The key does not begin with the namespace it was scanned under.
    KeyOutsideNamespace,
    /// Nothing frames a listing id and a spelling apart.
    ListingFramingMissing,
    /// The framed listing component is not the writer's UUID rendering.
    ListingFramingUnparsable,
    /// The trailing component names no ICN principal.
    SpellingNamesNoPrincipal,
    /// The row carries a value the writer never writes.
    UnrecognisedValue,
    /// A canonical interest row under this listing did not decode.
    CanonicalInterestUnreadable,
    /// A canonical interest row filed under this listing names another one.
    CanonicalInterestNamesAnotherListing,
}

impl UniquenessEvidenceDefect {
    /// A stable, payload-free label. Safe to return to a caller.
    fn reason_class(self) -> &'static str {
        match self {
            Self::KeyNotUtf8 => "uniqueness-key-not-utf8",
            Self::KeyOutsideNamespace => "uniqueness-key-outside-namespace",
            Self::ListingFramingMissing => "uniqueness-key-listing-framing-missing",
            Self::ListingFramingUnparsable => "uniqueness-key-listing-framing-unparsable",
            Self::SpellingNamesNoPrincipal => "uniqueness-key-names-no-principal",
            Self::UnrecognisedValue => "uniqueness-row-value-unrecognised",
            Self::CanonicalInterestUnreadable => "canonical-interest-unreadable",
            Self::CanonicalInterestNamesAnotherListing => "canonical-interest-listing-mismatch",
        }
    }
}

/// Whether a principal already holds an interest in one listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrincipalStanding {
    /// No canonical interest and no uniqueness row names this principal.
    ProvenAbsent,
    /// Something already names this principal: it may not express interest again.
    AlreadyPresent,
}

/// Sled-backed persistent listings store
pub struct SledListingsStore {
    db: Arc<sled::Db>,
    /// Serialises the whole express-interest transaction: classify the
    /// principal, claim the spelling key, write the canonical interest.
    ///
    /// The `compare_and_swap` below is atomic on its own, but only for the
    /// *exact spelling* it is handed. Proving a principal absent requires
    /// reading rows that a concurrent alias-spelled request could add between
    /// the read and the CAS, and no single sled operation covers both. This
    /// mutex closes that window; it does not replace the CAS, which still
    /// decides the same-spelling race unconditionally (#2627 M4b).
    ///
    /// Scope: one lock per store instance, which is one serialisation domain
    /// per database because `sled::Config::open` takes an exclusive `flock` on
    /// the database file, so no second process — and no second `sled::open` in
    /// this one — can hold the same database open. `icn_gateway::server` builds
    /// exactly one `SledListingsStore`, owned by the one `ListingsManager` as a
    /// `Box`, so that instance is the only writer.
    ///
    /// The bound of that, stated exactly: constructing a *second* store over a
    /// clone of the same `Arc<sled::Db>` would give it a second lock and leave
    /// only the CAS between the two, so the alias guard is instance-scoped
    /// while the same-spelling guard is not. Nothing in production does — the
    /// single construction site is `icn_gateway::server` — and the two facts
    /// this rests on are pinned rather than asserted:
    /// `sled_refuses_a_second_open_of_one_database` and
    /// `two_store_instances_over_one_database_keep_the_cas_guarantee`.
    interest_uniqueness: std::sync::Mutex<()>,
}

impl SledListingsStore {
    /// Create a new Sled-backed listings store
    pub fn new(db: Arc<sled::Db>) -> Self {
        Self {
            db,
            interest_uniqueness: std::sync::Mutex::new(()),
        }
    }

    /// Generate a versioned key for listings
    #[inline]
    fn listing_key(id: &ListingId) -> String {
        format!("{SLED_KEY_VERSION}:listing:{}", id.0)
    }

    /// Generate a versioned key prefix for listing scans
    #[inline]
    fn listing_prefix() -> String {
        format!("{SLED_KEY_VERSION}:listing:")
    }

    /// Generate a versioned key for interests
    #[inline]
    fn interest_key(listing_id: &ListingId, interest_id: &Uuid) -> String {
        format!(
            "{SLED_KEY_VERSION}:interest:{}:{}",
            listing_id.0, interest_id
        )
    }

    /// Generate a versioned key prefix for interest scans
    #[inline]
    fn interest_prefix(listing_id: &ListingId) -> String {
        format!("{SLED_KEY_VERSION}:interest:{}:", listing_id.0)
    }

    /// Generate a versioned index key for duplicate detection
    #[inline]
    fn interest_index_key(listing_id: &ListingId, from_did: &Did) -> String {
        format!(
            "{SLED_KEY_VERSION}:interest_idx:{}:{}",
            listing_id.0, from_did
        )
    }

    /// Generate a versioned key prefix for interest index scans
    #[inline]
    fn interest_index_prefix(listing_id: &ListingId) -> String {
        format!("{SLED_KEY_VERSION}:interest_idx:{}:", listing_id.0)
    }

    /// The namespace every interest index key begins with, across all listings.
    #[inline]
    fn interest_index_namespace() -> String {
        format!("{SLED_KEY_VERSION}:interest_idx:")
    }

    /// Split an interest index key into the listing it belongs to and the DID
    /// spelling it files, following the writer's layout rather than counting
    /// delimiters.
    ///
    /// [`Self::interest_index_key`] builds `namespace ‖ listing ‖ ':' ‖ spelling`
    /// where the listing is a `Uuid` in canonical form — 36 characters of hex
    /// and hyphens, containing no `':'` — and the spelling is a `Did` rendered
    /// by `Display`, which begins `did:icn:` and therefore does. So the first
    /// `':'` after the namespace is the framing, and *everything* after it is
    /// the spelling, taken whole to the end of the key.
    ///
    /// Splitting the whole key on `':'` and requiring four components — what
    /// this replaced — counted the DID's own colons as key structure and so
    /// rejected every row the writer produces (#2627 M4b).
    fn parse_interest_index_key(
        key: &[u8],
    ) -> std::result::Result<(Uuid, &str), UniquenessEvidenceDefect> {
        let text = std::str::from_utf8(key).map_err(|_| UniquenessEvidenceDefect::KeyNotUtf8)?;
        let rest = text
            .strip_prefix(Self::interest_index_namespace().as_str())
            .ok_or(UniquenessEvidenceDefect::KeyOutsideNamespace)?;
        let (listing, spelling) = rest
            .split_once(':')
            .ok_or(UniquenessEvidenceDefect::ListingFramingMissing)?;
        let listing = Uuid::parse_str(listing)
            .map_err(|_| UniquenessEvidenceDefect::ListingFramingUnparsable)?;
        Ok((listing, spelling))
    }

    /// Decide whether `from_did`'s principal already holds an interest in
    /// `listing_id`, from the canonical interest rows and the uniqueness
    /// projection that must agree with them.
    ///
    /// The canonical fact is the `v1:interest:` row: it carries a real [`Did`],
    /// so comparing it with `==` compares *principals* — the same rule
    /// [`InMemoryListingsStore::add_interest_if_not_duplicate`] already applies.
    /// The `interest_idx` row is keyed by one spelling, so it cannot answer the
    /// principal question by lookup; it is read here so that a row naming this
    /// principal still suppresses a second interest even when its canonical row
    /// is missing. A live write must never repair that state by treating the
    /// missing row as absence (#2627 M4b).
    ///
    /// Evidence that cannot be decoded is refused, never skipped: a row this
    /// code cannot read is a row it cannot prove *does not* name the principal.
    fn classify_principal_for_listing(
        &self,
        listing_id: &ListingId,
        from_did: &Did,
    ) -> Result<PrincipalStanding> {
        let refuse = |defect: UniquenessEvidenceDefect| {
            anyhow::anyhow!(
                "cannot prove listing-interest uniqueness for this listing (reason: {})",
                defect.reason_class()
            )
        };

        // The canonical interest facts. A row that does not decode, or that is
        // filed here while naming another listing, makes the question
        // unanswerable rather than answered "absent".
        for item in self
            .db
            .scan_prefix(Self::interest_prefix(listing_id).as_bytes())
        {
            let (_, value) = item?;
            let interest: ListingInterest = icn_encoding::decode_versioned(&value)
                .map_err(|_| refuse(UniquenessEvidenceDefect::CanonicalInterestUnreadable))?;
            if interest.listing_id != *listing_id {
                return Err(refuse(
                    UniquenessEvidenceDefect::CanonicalInterestNamesAnotherListing,
                ));
            }
            // Principal equality, not spelling equality (#2627 I7).
            if interest.from_did == *from_did {
                return Ok(PrincipalStanding::AlreadyPresent);
            }
        }

        // The uniqueness projection. Each row must decode to a principal; one
        // that names this principal suppresses the write whether or not its
        // canonical row survived.
        for item in self
            .db
            .scan_prefix(Self::interest_index_prefix(listing_id).as_bytes())
        {
            let (key, value) = item?;
            if value.as_ref() != INTEREST_INDEX_PRESENT {
                return Err(refuse(UniquenessEvidenceDefect::UnrecognisedValue));
            }
            let (_, spelling) = Self::parse_interest_index_key(&key).map_err(refuse)?;
            let filed: Did = spelling
                .parse()
                .map_err(|_| refuse(UniquenessEvidenceDefect::SpellingNamesNoPrincipal))?;
            if filed == *from_did {
                return Ok(PrincipalStanding::AlreadyPresent);
            }
        }

        Ok(PrincipalStanding::ProvenAbsent)
    }

    /// Save a listing
    pub fn save(&self, listing: &Listing) -> Result<()> {
        let key = Self::listing_key(&listing.id);
        let value = icn_encoding::encode_versioned(listing)?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }

    /// Get a listing by ID
    pub fn get(&self, id: &ListingId) -> Result<Option<Listing>> {
        let key = Self::listing_key(id);
        match self.db.get(key.as_bytes())? {
            Some(value) => Ok(Some(icn_encoding::decode_versioned(&value)?)),
            None => Ok(None),
        }
    }

    /// List all listings matching a filter with pagination
    pub fn list(&self, filter: &ListingFilter) -> Result<Vec<Listing>> {
        let prefix = Self::listing_prefix();
        let mut result = Vec::new();

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            let listing: Listing = icn_encoding::decode_versioned(&value)?;
            if filter.matches(&listing) {
                result.push(listing);
            }
        }

        // Apply sorting based on filter
        sort_listings(&mut result, filter.sort_by);

        // Apply pagination with validation logging
        let requested_offset = filter.offset.unwrap_or(0);
        let offset = requested_offset.min(MAX_OFFSET);
        let requested_limit = filter.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        let limit = requested_limit.min(MAX_PAGE_SIZE);

        // Log if offset was capped (potential DoS attempt)
        if requested_offset > MAX_OFFSET {
            tracing::warn!(
                requested = requested_offset,
                actual = offset,
                "Pagination offset capped to MAX_OFFSET (potential DoS attempt)"
            );
        }

        // Log if limit was capped
        if requested_limit > MAX_PAGE_SIZE {
            tracing::debug!(
                requested = requested_limit,
                actual = limit,
                "Pagination limit capped to MAX_PAGE_SIZE"
            );
        }

        // Log if offset is larger than result set (empty response)
        let total_matching = result.len();
        if offset >= total_matching && !result.is_empty() {
            tracing::debug!(
                offset = offset,
                total_matching = total_matching,
                "Pagination offset exceeds result count - returning empty results"
            );
        }

        Ok(result.into_iter().skip(offset).take(limit).collect())
    }

    /// Delete a listing and its associated interests
    pub fn delete(&self, id: &ListingId) -> Result<bool> {
        let listing_key = Self::listing_key(id);
        let existed = self.db.remove(listing_key.as_bytes())?.is_some();

        // Clean up associated interests
        let interest_prefix = Self::interest_prefix(id);
        for item in self.db.scan_prefix(interest_prefix.as_bytes()) {
            let (key, _) = item?;
            self.db.remove(key)?;
        }

        // Clean up interest index keys
        self.delete_interest_indexes(id)?;

        Ok(existed)
    }

    /// Add interest in a listing
    pub fn add_interest(&self, interest: &ListingInterest) -> Result<()> {
        let key = Self::interest_key(&interest.listing_id, &interest.id);
        let value = icn_encoding::encode_versioned(interest)?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }

    /// Get interests for a listing
    pub fn get_interests(&self, listing_id: &ListingId) -> Result<Vec<ListingInterest>> {
        let prefix = Self::interest_prefix(listing_id);
        let mut interests = Vec::new();

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            let interest: ListingInterest = icn_encoding::decode_versioned(&value)?;
            interests.push(interest);
        }

        // Sort by created_at ascending
        interests.sort_by_key(|a| a.created_at);

        Ok(interests)
    }

    /// Add interest if this **principal** has not already expressed interest.
    /// Returns Ok(true) if added, Ok(false) if duplicate.
    ///
    /// # What decides a duplicate
    ///
    /// The stored index key is `v1:interest_idx:<listing>:<spelling>`, and a
    /// `did:icn:` identifier has many accepted spellings of the same 32 bytes.
    /// The `compare_and_swap` below therefore answers "has this *spelling*
    /// claimed the listing", which is not the one-interest-per-member rule: one
    /// principal arriving under two spellings hit two different keys and got two
    /// canonical interests (#2627 M4b).
    ///
    /// So the decision is made against the canonical `v1:interest:` rows, whose
    /// `from_did` is a real [`Did`] and therefore compares by principal, with the
    /// uniqueness rows read as well so a row naming this principal still
    /// suppresses a write when its canonical row is gone. The CAS is kept
    /// unchanged behind that check: it remains the unconditional, storage-level
    /// decider of the same-spelling race, and the persisted bytes are exactly
    /// what they were.
    ///
    /// The classification and the write are serialised by
    /// [`Self::interest_uniqueness`], because a concurrent alias-spelled request
    /// could otherwise pass the same check before either CAS lands. See that
    /// field for the scope that lock covers and why it is the whole database.
    pub fn add_interest_if_not_duplicate(
        &self,
        interest: &ListingInterest,
        from_did: &Did,
    ) -> Result<bool> {
        let listing_id = interest.listing_id;

        // Held across classify → claim → write. Poisoning fails closed: a panic
        // under this lock leaves state this code cannot reason about, and
        // guessing is worse than refusing.
        let _uniqueness = self
            .interest_uniqueness
            .lock()
            .map_err(|_| anyhow::anyhow!("Listing interest uniqueness lock poisoned"))?;

        if self.classify_principal_for_listing(&listing_id, from_did)?
            == PrincipalStanding::AlreadyPresent
        {
            return Ok(false);
        }

        // Use an index key to track which DIDs have expressed interest
        // This allows atomic CAS without scanning all interests
        let index_key = Self::interest_index_key(&listing_id, from_did);

        // Atomically try to set the index key (only succeeds if not present)
        // compare_and_swap(key, old, new) - if old matches current value, set to new
        let cas_result = self.db.compare_and_swap(
            index_key.as_bytes(),
            None::<&[u8]>,
            Some(INTEREST_INDEX_PRESENT),
        )?;

        if cas_result.is_err() {
            // Key already existed - duplicate interest
            return Ok(false);
        }

        // CAS succeeded - no duplicate, now insert the actual interest data
        let interest_key = Self::interest_key(&listing_id, &interest.id);
        let interest_value = icn_encoding::encode_versioned(interest)?;

        // If insert fails, clean up the index key to avoid permanently blocking this DID
        if let Err(e) = self.db.insert(interest_key.as_bytes(), interest_value) {
            tracing::warn!(
                listing_id = %listing_id,
                from_did = %from_did,
                error = %e,
                "Failed to insert interest data; attempting cleanup of index key"
            );
            if let Err(cleanup_err) = self.db.remove(index_key.as_bytes()) {
                tracing::error!(
                    listing_id = %listing_id,
                    from_did = %from_did,
                    error = %cleanup_err,
                    "Critical: Failed to clean up index key - user may be permanently blocked from expressing interest"
                );
            }
            return Err(e.into());
        }

        Ok(true)
    }

    /// Clean up orphaned index keys that don't have corresponding interest entries.
    ///
    /// This handles edge cases where a process crash occurs between CAS success
    /// and interest insert completion. Such orphaned index keys would permanently
    /// block the affected user from expressing interest.
    ///
    /// # Performance
    ///
    /// This is O(n) where n = total interest index keys. Schedule during off-peak hours.
    ///
    /// # Concurrency Warning
    ///
    /// This function has a TOCTOU (time-of-check-time-of-use) race condition:
    /// between checking if an interest exists and removing the orphaned index,
    /// another thread could insert a new interest. It does not take the
    /// uniqueness lock, so the race is real. It is acceptable because:
    /// 1. This is a maintenance task meant for off-peak/quiescent periods
    /// 2. Worst case: an index row is removed for a just-created interest.
    ///    Uniqueness is unaffected — the canonical `v1:interest:` row decides,
    ///    and it is untouched here (#2627 M4b).
    ///
    /// What that costs is *evidence*, not safety: the row is not recreated. A
    /// later `express_interest` for that principal is answered `AlreadyPresent`
    /// from the canonical row and returns before reaching the CAS, so the
    /// N2-A gate simply has one fewer uniqueness row to read for that pair.
    /// Before M4b this pass could recognise no production row at all, so the
    /// race was unreachable; it is reachable now, and recorded rather than
    /// closed (migration gate §11.9).
    ///
    /// For production at scale, consider running this in a transaction or
    /// during scheduled maintenance windows when write traffic is minimal.
    ///
    /// # Returns
    ///
    /// The number of orphaned index keys that were cleaned up.
    ///
    /// # What counts as orphaned
    ///
    /// The row's value is a sentinel, never an interest id, so the canonical row
    /// cannot be resolved by lookup — it is found by scanning the listing's
    /// interests for one whose `from_did` renders to the **byte-identical**
    /// spelling. That is the writer's own contract: the key is built from
    /// `ListingInterest.from_did`, so index `A` implies a primary spelling `A`.
    /// Matching by principal instead would retain a row whose own primary is
    /// gone because a *differently spelled* interest for the same principal
    /// survives, which is a state the writer never produced.
    ///
    /// A row whose key or value this code cannot read is **skipped, not
    /// deleted** — the existing contract, and the safe default for a maintenance
    /// pass, since deleting nothing loses nothing. Refusing on unreadable
    /// evidence is the *live write path's* job
    /// ([`Self::classify_principal_for_listing`]); the two must not be
    /// confused (#2627 M4b).
    pub fn cleanup_orphaned_interest_indexes(&self) -> Result<usize> {
        let index_prefix = Self::interest_index_namespace();
        let mut cleaned = 0;

        for item in self.db.scan_prefix(index_prefix.as_bytes()) {
            let (key, value) = item?;

            // Parse by the writer's framing, not by counting ':' — a DID
            // spelling contains its own colons, so splitting the whole key on
            // ':' rejected every row the writer produces and made this pass a
            // no-op in production (#2627 M4b).
            let (listing_id, from_did_str) = match Self::parse_interest_index_key(&key) {
                Ok(parsed) => parsed,
                Err(defect) => {
                    tracing::warn!(
                        reason = defect.reason_class(),
                        "Skipping unreadable interest index key; not treating it as orphaned"
                    );
                    continue;
                }
            };

            if value.as_ref() != INTEREST_INDEX_PRESENT {
                tracing::warn!(
                    reason = UniquenessEvidenceDefect::UnrecognisedValue.reason_class(),
                    "Skipping interest index row with an unrecognised value; not treating it as orphaned"
                );
                continue;
            }

            // Check if any interest exists for this listing_id from this exact
            // spelling.
            let interest_prefix = format!("{SLED_KEY_VERSION}:interest:{listing_id}:");
            let mut has_matching_interest = false;

            for interest_item in self.db.scan_prefix(interest_prefix.as_bytes()) {
                let (_, value) = interest_item?;
                if let Ok(interest) = icn_encoding::decode_versioned::<ListingInterest>(&value) {
                    if interest.from_did.as_str() == from_did_str {
                        has_matching_interest = true;
                        break;
                    }
                }
            }

            if !has_matching_interest {
                tracing::info!(
                    listing_id = %listing_id,
                    "Cleaning up orphaned interest index key"
                );
                self.db.remove(key)?;
                cleaned += 1;
            }
        }

        if cleaned > 0 {
            tracing::info!(count = cleaned, "Cleaned up orphaned interest index keys");
        }

        Ok(cleaned)
    }

    /// Clean up index keys when deleting a listing's interests
    fn delete_interest_indexes(&self, listing_id: &ListingId) -> Result<()> {
        let prefix = Self::interest_index_prefix(listing_id);
        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, _) = item?;
            self.db.remove(key)?;
        }
        Ok(())
    }

    /// List all listings with a specific status (ignores pagination and expiry filtering).
    ///
    /// Used for background tasks like expiring stale listings.
    ///
    /// # Performance
    ///
    /// This method performs a full table scan (O(n) where n = total listings).
    /// For deployments with >10K listings, schedule execution during off-peak hours
    /// or add a secondary index on (status, expires_at) for efficient queries.
    ///
    /// Current pilot phase supports up to 10K listings efficiently.
    ///
    /// # Difference from `list()`
    ///
    /// Unlike `list()`, this method does NOT filter out expired Active listings.
    /// This is intentional: when expiring stale listings, we need to find Active
    /// listings that have expired, which `list()` would hide.
    ///
    /// # Memory Usage
    ///
    /// Results are collected in memory before returning. For n active listings,
    /// memory usage is approximately O(n × avg_listing_size). Typical listing
    /// is ~500 bytes, so 10K listings ≈ 5MB memory.
    ///
    /// For deployments exceeding 50K listings, consider:
    /// 1. Adding a cursor-based iterator API to process results in batches
    /// 2. Adding a secondary `status:listing_id` index for O(1) status lookups
    /// 3. Partitioning by created_at date range for incremental processing
    pub fn list_all_matching_status(&self, status: ListingStatus) -> Result<Vec<Listing>> {
        let prefix = Self::listing_prefix();
        let mut result = Vec::new();

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            if let Ok(listing) = icn_encoding::decode_versioned::<Listing>(&value) {
                if listing.status == status {
                    result.push(listing);
                }
            }
        }

        Ok(result)
    }

    /// Delete all interests for a listing.
    ///
    /// Used when expiring or cancelling listings to prevent stale data accumulation
    /// and privacy leaks from retained buyer/interest information.
    ///
    /// Returns the number of interests deleted.
    pub fn delete_interests(&self, listing_id: &ListingId) -> Result<usize> {
        let prefix = Self::interest_prefix(listing_id);
        let mut deleted_count = 0;

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, _) = item?;
            self.db.remove(key)?;
            deleted_count += 1;
        }

        // Also clean up interest index keys to prevent orphaned indexes
        self.delete_interest_indexes(listing_id)?;

        Ok(deleted_count)
    }
}

/// Listings store backend trait
pub trait ListingsStoreBackend: Send + Sync {
    fn save(&self, listing: &Listing) -> Result<()>;
    fn get(&self, id: &ListingId) -> Result<Option<Listing>>;
    fn list(&self, filter: &ListingFilter) -> Result<Vec<Listing>>;
    fn delete(&self, id: &ListingId) -> Result<bool>;
    fn add_interest(&self, interest: &ListingInterest) -> Result<()>;
    fn get_interests(&self, listing_id: &ListingId) -> Result<Vec<ListingInterest>>;
    /// Atomically add interest if user hasn't already expressed interest.
    /// Returns Ok(true) if added, Ok(false) if duplicate, Err on failure.
    fn add_interest_if_not_duplicate(
        &self,
        interest: &ListingInterest,
        from_did: &Did,
    ) -> Result<bool>;
    /// List all listings with a specific status (ignores pagination and expiry filtering).
    /// Used for background tasks like expiring stale listings.
    fn list_all_matching_status(&self, status: ListingStatus) -> Result<Vec<Listing>>;

    /// Clean up orphaned interest index keys.
    /// Returns the number of keys cleaned up.
    /// Default implementation returns 0 (no-op for in-memory stores).
    fn cleanup_orphaned_interest_indexes(&self) -> Result<usize> {
        Ok(0)
    }

    /// Delete all interests for a listing.
    /// Used when expiring or cancelling listings to prevent stale data accumulation.
    /// Returns the number of interests deleted.
    fn delete_interests(&self, listing_id: &ListingId) -> Result<usize>;
}

impl ListingsStoreBackend for InMemoryListingsStore {
    fn save(&self, listing: &Listing) -> Result<()> {
        InMemoryListingsStore::save(self, listing)
    }
    fn get(&self, id: &ListingId) -> Result<Option<Listing>> {
        InMemoryListingsStore::get(self, id)
    }
    fn list(&self, filter: &ListingFilter) -> Result<Vec<Listing>> {
        InMemoryListingsStore::list(self, filter)
    }
    fn delete(&self, id: &ListingId) -> Result<bool> {
        InMemoryListingsStore::delete(self, id)
    }
    fn add_interest(&self, interest: &ListingInterest) -> Result<()> {
        InMemoryListingsStore::add_interest(self, interest)
    }
    fn get_interests(&self, listing_id: &ListingId) -> Result<Vec<ListingInterest>> {
        InMemoryListingsStore::get_interests(self, listing_id)
    }
    fn add_interest_if_not_duplicate(
        &self,
        interest: &ListingInterest,
        from_did: &Did,
    ) -> Result<bool> {
        InMemoryListingsStore::add_interest_if_not_duplicate(self, interest, from_did)
    }
    fn list_all_matching_status(&self, status: ListingStatus) -> Result<Vec<Listing>> {
        InMemoryListingsStore::list_all_matching_status(self, status)
    }
    fn cleanup_orphaned_interest_indexes(&self) -> Result<usize> {
        // In-memory store doesn't persist, so no orphans possible
        Ok(0)
    }
    fn delete_interests(&self, listing_id: &ListingId) -> Result<usize> {
        InMemoryListingsStore::delete_interests(self, listing_id)
    }
}

impl ListingsStoreBackend for SledListingsStore {
    fn save(&self, listing: &Listing) -> Result<()> {
        SledListingsStore::save(self, listing)
    }
    fn get(&self, id: &ListingId) -> Result<Option<Listing>> {
        SledListingsStore::get(self, id)
    }
    fn list(&self, filter: &ListingFilter) -> Result<Vec<Listing>> {
        SledListingsStore::list(self, filter)
    }
    fn delete(&self, id: &ListingId) -> Result<bool> {
        SledListingsStore::delete(self, id)
    }
    fn add_interest(&self, interest: &ListingInterest) -> Result<()> {
        SledListingsStore::add_interest(self, interest)
    }
    fn get_interests(&self, listing_id: &ListingId) -> Result<Vec<ListingInterest>> {
        SledListingsStore::get_interests(self, listing_id)
    }
    fn add_interest_if_not_duplicate(
        &self,
        interest: &ListingInterest,
        from_did: &Did,
    ) -> Result<bool> {
        SledListingsStore::add_interest_if_not_duplicate(self, interest, from_did)
    }
    fn list_all_matching_status(&self, status: ListingStatus) -> Result<Vec<Listing>> {
        SledListingsStore::list_all_matching_status(self, status)
    }
    fn cleanup_orphaned_interest_indexes(&self) -> Result<usize> {
        SledListingsStore::cleanup_orphaned_interest_indexes(self)
    }
    fn delete_interests(&self, listing_id: &ListingId) -> Result<usize> {
        SledListingsStore::delete_interests(self, listing_id)
    }
}

/// Listings manager for the gateway
pub struct ListingsManager {
    store: Box<dyn ListingsStoreBackend>,
}

impl ListingsManager {
    /// Create a new listings manager with in-memory storage (for testing)
    pub fn new() -> Self {
        Self {
            store: Box::new(InMemoryListingsStore::new()),
        }
    }

    /// Create a new listings manager with Sled-backed persistent storage
    pub fn with_sled(db: Arc<sled::Db>) -> Self {
        Self {
            store: Box::new(SledListingsStore::new(db)),
        }
    }

    /// Create a new listing
    pub fn create_listing(
        &self,
        listing_type: ListingType,
        title: String,
        description: String,
        category: ListingCategory,
        offered_by: Did,
        coop_id: String,
        seeking: String,
        photos: Vec<String>,
        visibility: ListingVisibility,
        expires_at: Option<u64>,
        tags: Vec<String>,
    ) -> Result<Listing> {
        let now = icn_time::current_timestamp_secs();
        let mut listing = Listing::new(
            listing_type,
            title,
            description,
            category.clone(),
            offered_by,
            coop_id,
            seeking,
            now,
        );
        listing.photos = photos;
        listing.visibility = visibility;
        listing.expires_at = expires_at;
        listing.tags = tags;

        self.store.save(&listing)?;

        // Emit metrics
        icn_obs::metrics::exchange::listings_created_inc(listing_type.as_str(), category.as_str());

        Ok(listing)
    }

    /// Get a listing by ID
    pub fn get_listing(&self, id: &ListingId) -> Result<Option<Listing>> {
        self.store.get(id)
    }

    /// List listings with filter
    pub fn list_listings(&self, filter: &ListingFilter) -> Result<Vec<Listing>> {
        self.store.list(filter)
    }

    /// Update a listing
    pub fn update_listing(&self, listing: &Listing) -> Result<()> {
        self.store.save(listing)
    }

    /// Delete a listing
    pub fn delete_listing(&self, id: &ListingId) -> Result<bool> {
        self.store.delete(id)
    }

    /// Express interest in a listing
    ///
    /// # Race Condition Note (TOCTOU)
    ///
    /// There is a potential race condition between checking if the listing is active
    /// and inserting the interest. Another thread could cancel/complete the listing
    /// between these operations. This is considered benign because:
    ///
    /// 1. The interest insertion itself is atomic (duplicate prevention works)
    /// 2. If the listing is cancelled/completed concurrently, the interest simply
    ///    becomes orphaned and will be cleaned up by `cleanup_orphaned_interest_indexes`
    /// 3. The listing owner will see the interest but won't be able to act on
    ///    a non-active listing anyway
    ///
    /// A fully transactional approach would require locking the listing during
    /// interest insertion, which adds complexity and lock contention. Given that
    /// listing state transitions are rare (once per listing lifecycle), the current
    /// eventual consistency approach is acceptable for cooperative exchange use cases.
    pub fn express_interest(
        &self,
        listing_id: ListingId,
        from_did: Did,
        from_coop: String,
        message: String,
        offer: Option<String>,
    ) -> Result<ListingInterest> {
        // Verify listing exists and is active
        // Note: This check prevents most invalid interest submissions, but there's
        // a small race window where the listing could change state between this
        // check and the interest insertion. See doc comment above for details.
        let listing = self
            .store
            .get(&listing_id)?
            .ok_or_else(|| anyhow::anyhow!("Listing not found"))?;

        if !listing.is_active() {
            anyhow::bail!("Listing is not active");
        }

        let now = icn_time::current_timestamp_secs();
        let interest =
            ListingInterest::new(listing_id, from_did.clone(), from_coop, message, offer, now);

        // Use atomic check-and-insert to prevent race conditions
        let added = self
            .store
            .add_interest_if_not_duplicate(&interest, &from_did)?;

        if !added {
            // Emit duplicate interest blocked metric
            icn_obs::metrics::exchange::duplicate_interests_blocked_inc();
            anyhow::bail!("You have already expressed interest in this listing");
        }

        // Emit interest expressed metric
        icn_obs::metrics::exchange::interests_expressed_inc(listing.listing_type.as_str());

        Ok(interest)
    }

    /// Get interests for a listing
    pub fn get_interests(&self, listing_id: &ListingId) -> Result<Vec<ListingInterest>> {
        self.store.get_interests(listing_id)
    }

    /// Get interest counts for multiple listings in a single operation.
    ///
    /// This is more efficient than calling `get_interests` in a loop
    /// as it acquires the lock only once.
    pub fn get_interest_counts(&self, listing_ids: &[ListingId]) -> HashMap<ListingId, usize> {
        listing_ids
            .iter()
            .filter_map(|id| match self.store.get_interests(id) {
                Ok(interests) => Some((*id, interests.len())),
                Err(e) => {
                    warn!(listing_id = %id, error = %e, "Failed to get interests for listing");
                    None
                }
            })
            .collect()
    }

    /// Mark a listing as matched
    pub fn mark_matched(&self, id: &ListingId) -> Result<Listing> {
        let mut listing = self
            .store
            .get(id)?
            .ok_or_else(|| anyhow::anyhow!("Listing not found"))?;

        let now = icn_time::current_timestamp_secs();

        // Calculate time to match (use saturating_sub to handle potential clock skew)
        let time_to_match = now.saturating_sub(listing.created_at) as f64;

        listing.mark_matched(now);
        self.store.save(&listing)?;

        // Emit metrics
        icn_obs::metrics::exchange::listing_time_to_match_observe(time_to_match);

        Ok(listing)
    }

    /// Mark a listing as completed
    pub fn mark_completed(&self, id: &ListingId) -> Result<Listing> {
        let mut listing = self
            .store
            .get(id)?
            .ok_or_else(|| anyhow::anyhow!("Listing not found"))?;

        let now = icn_time::current_timestamp_secs();

        // Calculate time to complete (use saturating_sub to handle potential clock skew)
        let time_to_complete = now.saturating_sub(listing.created_at) as f64;

        listing.mark_completed(now);
        self.store.save(&listing)?;

        // Emit metrics
        icn_obs::metrics::exchange::listings_completed_inc();
        icn_obs::metrics::exchange::listing_time_to_complete_observe(time_to_complete);

        // Also record the number of interests this listing received
        if let Ok(interests) = self.store.get_interests(id) {
            icn_obs::metrics::exchange::listing_interests_count_observe(interests.len());
        }

        Ok(listing)
    }

    /// Cancel a listing
    pub fn cancel_listing(&self, id: &ListingId) -> Result<Listing> {
        let mut listing = self
            .store
            .get(id)?
            .ok_or_else(|| anyhow::anyhow!("Listing not found"))?;

        let now = icn_time::current_timestamp_secs();
        listing.cancel(now);
        self.store.save(&listing)?;

        // Emit cancelled metric
        icn_obs::metrics::exchange::listings_cancelled_inc();

        Ok(listing)
    }

    /// Expire all listings that have passed their expiration date
    ///
    /// This function scans all active listings and marks any that have passed
    /// their `expires_at` timestamp as `Expired`. It should be called periodically
    /// to ensure timely listing expiration.
    ///
    /// # Scheduling Recommendations
    ///
    /// - **Frequency**: Call every 1-5 minutes depending on listing volume
    /// - **When**: During low-traffic periods (e.g., via background task)
    /// - **Batching**: For large deployments, consider processing in batches
    ///
    /// # Performance
    ///
    /// This is O(n) where n is the number of active listings. For typical
    /// cooperative deployments (< 1000 active listings), this completes in < 100ms.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // In a background task:
    /// loop {
    ///     tokio::time::sleep(Duration::from_secs(60)).await;
    ///     if let Err(e) = listings_mgr.expire_stale_listings() {
    ///         tracing::warn!("Failed to expire listings: {}", e);
    ///     }
    /// }
    /// ```
    pub fn expire_stale_listings(&self) -> Result<usize> {
        let now = icn_time::current_timestamp_secs();

        // Get all active listings (without expiry filter so we can find the stale ones)
        let all_active = self.store.list_all_matching_status(ListingStatus::Active)?;

        let mut expired_count = 0;
        let mut interests_deleted = 0;
        for mut listing in all_active {
            if listing.is_expired(now) {
                // Delete associated interests first to prevent stale data accumulation
                // and privacy leaks from retained buyer information
                match self.store.delete_interests(&listing.id) {
                    Ok(count) => interests_deleted += count,
                    Err(e) => {
                        warn!(
                            listing_id = %listing.id,
                            error = %e,
                            "Failed to delete interests for expired listing"
                        );
                        // Continue expiring the listing even if interest cleanup fails
                    }
                }

                listing.status = ListingStatus::Expired;
                listing.updated_at = now;
                self.store.save(&listing)?;
                expired_count += 1;
                icn_obs::metrics::exchange::listings_expired_inc();
            }
        }

        if expired_count > 0 {
            tracing::info!(
                count = expired_count,
                interests_deleted = interests_deleted,
                "Expired stale listings"
            );
        }

        Ok(expired_count)
    }

    /// Clean up orphaned interest index keys (Sled backend only).
    ///
    /// Orphaned index keys can occur if a process crashes between a successful
    /// CAS (compare-and-swap) operation and the subsequent interest insert.
    /// These orphaned keys would permanently block the affected user from
    /// expressing interest.
    ///
    /// This is a maintenance operation that should be scheduled to run periodically
    /// (e.g., hourly or daily) during off-peak hours.
    ///
    /// # Returns
    ///
    /// The number of orphaned index keys that were cleaned up.
    /// Returns 0 for in-memory backends (no persistence = no orphans).
    pub fn cleanup_orphaned_interest_indexes(&self) -> Result<usize> {
        self.store.cleanup_orphaned_interest_indexes()
    }
}

impl Default for ListingsManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Background Scheduler
// ============================================================================

/// Default interval for expiry checks (1 hour in seconds)
pub const DEFAULT_EXPIRY_CHECK_INTERVAL_SECS: u64 = 3600;

/// Start the listings expiry scheduler
///
/// Spawns a background task that periodically:
/// 1. Expires stale listings (marks Active listings past their expiry date as Expired)
/// 2. Cleans up orphaned interest index keys (Sled backend only)
///
/// # Arguments
///
/// * `listings_mgr` - The listings manager (must be wrapped in Arc<tokio::sync::RwLock>)
/// * `check_interval_secs` - How often to check for expired listings (default: 3600 = 1 hour)
///
/// # Example
///
/// ```ignore
/// use std::sync::Arc;
/// use tokio::sync::RwLock;
///
/// let listings_mgr = Arc::new(RwLock::new(ListingsManager::new()));
/// let handle = start_expiry_scheduler(
///     listings_mgr.clone(),
///     DEFAULT_EXPIRY_CHECK_INTERVAL_SECS,
/// );
/// ```
///
/// # Recommended Configuration
///
/// | Deployment Size | Interval | Rationale |
/// |-----------------|----------|-----------|
/// | Pilot (<1K listings) | 1 hour | Low overhead, responsive cleanup |
/// | Small (<10K) | 1 hour | Acceptable scan overhead |
/// | Medium (<50K) | 6 hours | Reduce peak load, run during off-hours |
/// | Large (>50K) | Daily | Schedule during maintenance window |
pub fn start_expiry_scheduler(
    listings_mgr: Arc<tokio::sync::RwLock<ListingsManager>>,
    check_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!(
            interval_secs = check_interval_secs,
            "Starting listings expiry scheduler"
        );

        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(check_interval_secs));

        loop {
            interval.tick().await;

            let mgr = listings_mgr.write().await;

            // Expire stale listings
            match mgr.expire_stale_listings() {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!(
                            expired_count = count,
                            "Listings expiry scheduler: expired stale listings"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "Listings expiry scheduler: failed to expire stale listings"
                    );
                }
            }

            // Clean up orphaned interest indexes (Sled only, no-op for in-memory)
            match mgr.cleanup_orphaned_interest_indexes() {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!(
                            cleaned_count = count,
                            "Listings expiry scheduler: cleaned up orphaned interest indexes"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "Listings expiry scheduler: failed to clean up orphaned indexes"
                    );
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::Did;

    fn test_did() -> Did {
        // Use deterministic secret bytes to generate a valid Ed25519-based DID
        // This ensures the DID will pass validation during deserialization
        use ed25519_dalek::SigningKey;
        let signing_key = SigningKey::from_bytes(&[1; 32]);
        let verifying_key = signing_key.verifying_key();
        Did::from_public_key(&verifying_key)
    }

    fn test_did_2() -> Did {
        // Second valid DID for testing
        use ed25519_dalek::SigningKey;
        let signing_key = SigningKey::from_bytes(&[2; 32]);
        let verifying_key = signing_key.verifying_key();
        Did::from_public_key(&verifying_key)
    }

    fn test_did_3() -> Did {
        // Third valid DID for testing
        use ed25519_dalek::SigningKey;
        let signing_key = SigningKey::from_bytes(&[3; 32]);
        let verifying_key = signing_key.verifying_key();
        Did::from_public_key(&verifying_key)
    }

    fn test_did_99() -> Did {
        // Another valid DID for testing
        use ed25519_dalek::SigningKey;
        let signing_key = SigningKey::from_bytes(&[99; 32]);
        let verifying_key = signing_key.verifying_key();
        Did::from_public_key(&verifying_key)
    }

    #[test]
    fn test_create_listing() {
        let mgr = ListingsManager::new();
        let listing = mgr
            .create_listing(
                ListingType::Offer,
                "Commercial Oven".to_string(),
                "Vulcan convection oven, barely used".to_string(),
                ListingCategory::Equipment,
                test_did(),
                "tech-coop".to_string(),
                "Credits or labor exchange".to_string(),
                vec!["ipfs://photo1".to_string()],
                ListingVisibility::Federation,
                None,
                vec!["kitchen".to_string(), "equipment".to_string()],
            )
            .expect("should create listing");

        assert_eq!(listing.title, "Commercial Oven");
        assert_eq!(listing.listing_type, ListingType::Offer);
        assert!(listing.is_active());
    }

    #[test]
    fn test_listing_filter() {
        let mgr = ListingsManager::new();

        // Create an offer
        mgr.create_listing(
            ListingType::Offer,
            "Oven".to_string(),
            "Big oven".to_string(),
            ListingCategory::Equipment,
            test_did(),
            "coop1".to_string(),
            "Credits".to_string(),
            vec![],
            ListingVisibility::Federation,
            None,
            vec![],
        )
        .unwrap();

        // Create a want
        mgr.create_listing(
            ListingType::Want,
            "Carpenter needed".to_string(),
            "Looking for carpentry help".to_string(),
            ListingCategory::Services,
            test_did(),
            "coop2".to_string(),
            "Will pay in credits".to_string(),
            vec![],
            ListingVisibility::Federation,
            None,
            vec![],
        )
        .unwrap();

        // Filter by type
        let filter = ListingFilter {
            listing_type: Some(ListingType::Offer),
            ..Default::default()
        };
        let offers = mgr.list_listings(&filter).unwrap();
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].title, "Oven");

        // Filter by coop
        let filter = ListingFilter {
            coop_id: Some("coop2".to_string()),
            ..Default::default()
        };
        let coop2_listings = mgr.list_listings(&filter).unwrap();
        assert_eq!(coop2_listings.len(), 1);
        assert_eq!(coop2_listings[0].coop_id, "coop2");

        // Filter by owner (offered_by) - both listings have the same owner
        let filter = ListingFilter {
            offered_by: Some(test_did()),
            ..Default::default()
        };
        let my_listings = mgr.list_listings(&filter).unwrap();
        assert_eq!(my_listings.len(), 2);

        // Filter by owner AND type (combining filters)
        let filter = ListingFilter {
            offered_by: Some(test_did()),
            listing_type: Some(ListingType::Offer),
            ..Default::default()
        };
        let my_offers = mgr.list_listings(&filter).unwrap();
        assert_eq!(my_offers.len(), 1);
        assert_eq!(my_offers[0].title, "Oven");

        // Filter by owner with no matches
        let other_did = test_did_99();
        let filter = ListingFilter {
            offered_by: Some(other_did),
            ..Default::default()
        };
        let no_listings = mgr.list_listings(&filter).unwrap();
        assert_eq!(no_listings.len(), 0);
    }

    #[test]
    fn test_express_interest() {
        let mgr = ListingsManager::new();

        let listing = mgr
            .create_listing(
                ListingType::Offer,
                "Laptop".to_string(),
                "Old but working laptop".to_string(),
                ListingCategory::Equipment,
                test_did(),
                "coop1".to_string(),
                "Any reasonable offer".to_string(),
                vec![],
                ListingVisibility::Network,
                None,
                vec![],
            )
            .unwrap();

        let interest = mgr
            .express_interest(
                listing.id,
                test_did_2(),
                "coop2".to_string(),
                "I'm interested! Can offer 10 hours of tech support.".to_string(),
                Some("10 hours tech support".to_string()),
            )
            .unwrap();

        assert_eq!(interest.listing_id, listing.id);

        let interests = mgr.get_interests(&listing.id).unwrap();
        assert_eq!(interests.len(), 1);

        // Trying to express interest again from same user should fail
        let duplicate_result = mgr.express_interest(
            listing.id,
            test_did_2(), // Same DID as before
            "coop2".to_string(),
            "Another message".to_string(),
            None,
        );
        assert!(
            duplicate_result.is_err(),
            "Should prevent duplicate interest from same user"
        );
        assert!(duplicate_result
            .unwrap_err()
            .to_string()
            .contains("already expressed interest"));

        // Different user should be able to express interest
        let different_user_interest = mgr.express_interest(
            listing.id,
            test_did_3(), // Different DID
            "coop3".to_string(),
            "I'm also interested!".to_string(),
            None,
        );
        assert!(different_user_interest.is_ok());

        // Now should have 2 interests
        let interests = mgr.get_interests(&listing.id).unwrap();
        assert_eq!(interests.len(), 2);
    }

    #[test]
    fn test_listing_lifecycle() {
        let mgr = ListingsManager::new();

        let listing = mgr
            .create_listing(
                ListingType::Offer,
                "Desk".to_string(),
                "Standing desk".to_string(),
                ListingCategory::Equipment,
                test_did(),
                "coop1".to_string(),
                "Credits".to_string(),
                vec![],
                ListingVisibility::Federation,
                None,
                vec![],
            )
            .unwrap();

        assert!(listing.is_active());

        // Mark as matched
        let listing = mgr.mark_matched(&listing.id).unwrap();
        assert_eq!(listing.status, ListingStatus::Matched);

        // Mark as completed
        let listing = mgr.mark_completed(&listing.id).unwrap();
        assert_eq!(listing.status, ListingStatus::Completed);
    }

    #[test]
    fn test_sled_listing_roundtrip() {
        // Test that listings can be stored and retrieved with Sled
        let db = sled::Config::new().temporary(true).open().unwrap();
        let mgr = ListingsManager::with_sled(std::sync::Arc::new(db));

        let listing = mgr
            .create_listing(
                ListingType::Offer,
                "Sled Test Item".to_string(),
                "Testing Sled storage".to_string(),
                ListingCategory::Equipment,
                test_did(),
                "coop1".to_string(),
                "Credits".to_string(),
                vec![],
                ListingVisibility::Federation,
                None,
                vec![],
            )
            .expect("should create listing");

        // Retrieve it
        let retrieved = mgr.get_listing(&listing.id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.title, "Sled Test Item");
        assert_eq!(retrieved.offered_by, test_did());
    }

    #[test]
    fn test_listing_interest_serialization() {
        // Test that ListingInterest roundtrips through icn_encoding correctly
        let did = test_did();
        let interest = ListingInterest {
            id: uuid::Uuid::new_v4(),
            listing_id: ListingId(uuid::Uuid::new_v4()),
            from_did: did.clone(),
            from_coop: "test-coop".to_string(),
            message: "Hello".to_string(),
            offer: Some("My offer".to_string()),
            created_at: 12345,
        };

        // Test direct encoding (no version prefix)
        let bytes = icn_encoding::encode(&interest).unwrap();
        let decoded: ListingInterest = icn_encoding::decode(&bytes).unwrap();
        assert_eq!(decoded.from_did, did);
        assert_eq!(decoded.from_coop, "test-coop");
        assert_eq!(decoded.message, "Hello");

        // Test versioned encoding
        let versioned_bytes = icn_encoding::encode_versioned(&interest).unwrap();
        let versioned_decoded: ListingInterest =
            icn_encoding::decode_versioned(&versioned_bytes).unwrap();
        assert_eq!(versioned_decoded.from_did, did);

        // Test with a different DID to ensure encoding handles variations
        let did2 = test_did_2();
        let interest2 = ListingInterest {
            id: uuid::Uuid::new_v4(),
            listing_id: ListingId(uuid::Uuid::new_v4()),
            from_did: did2.clone(),
            from_coop: "coop2".to_string(),
            message: "I want this!".to_string(),
            offer: Some("My offer".to_string()),
            created_at: icn_time::current_timestamp_secs(),
        };

        let bytes2 = icn_encoding::encode_versioned(&interest2).unwrap();
        let decoded2: ListingInterest = icn_encoding::decode_versioned(&bytes2).unwrap();
        assert_eq!(decoded2.from_did, did2);
        assert_eq!(decoded2.from_coop, "coop2");
        assert_eq!(decoded2.message, "I want this!");
    }

    #[test]
    fn test_sled_interest_roundtrip() {
        // Test that interests can be stored and retrieved with Sled
        let db = sled::Config::new().temporary(true).open().unwrap();
        let mgr = ListingsManager::with_sled(std::sync::Arc::new(db));

        let owner = test_did();
        let interested = test_did_2();

        let listing = mgr
            .create_listing(
                ListingType::Offer,
                "Sled Interest Test".to_string(),
                "Testing Sled interest storage".to_string(),
                ListingCategory::Equipment,
                owner,
                "coop1".to_string(),
                "Credits".to_string(),
                vec![],
                ListingVisibility::Federation,
                None,
                vec![],
            )
            .unwrap();

        // Express interest
        let interest = mgr
            .express_interest(
                listing.id,
                interested.clone(),
                "coop2".to_string(),
                "I want this!".to_string(),
                Some("My offer".to_string()),
            )
            .expect("should express interest");

        assert_eq!(interest.from_did, interested);

        // Retrieve interests
        let interests = mgr.get_interests(&listing.id).unwrap();
        assert_eq!(interests.len(), 1);
        assert_eq!(interests[0].from_did, interested);
        assert_eq!(interests[0].message, "I want this!");
        assert_eq!(interests[0].offer, Some("My offer".to_string()));
    }
}
