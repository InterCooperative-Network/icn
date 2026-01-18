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
            listings.sort_by(|a, b| b.created_at.cmp(&a.created_at));
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
        let offset = filter.offset.unwrap_or(0);
        let requested_limit = filter.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        let limit = requested_limit.min(MAX_PAGE_SIZE);

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
    /// Used for background tasks like expiring stale listings.
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
}

/// Sled key version for schema migration support.
/// When the schema changes, increment this version and add migration logic.
const SLED_KEY_VERSION: &str = "v1";

/// Sled-backed persistent listings store
pub struct SledListingsStore {
    db: Arc<sled::Db>,
}

impl SledListingsStore {
    /// Create a new Sled-backed listings store
    pub fn new(db: Arc<sled::Db>) -> Self {
        Self { db }
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
        let offset = filter.offset.unwrap_or(0);
        let requested_limit = filter.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        let limit = requested_limit.min(MAX_PAGE_SIZE);

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
        interests.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Ok(interests)
    }

    /// Atomically add interest if user hasn't already expressed interest.
    /// Returns Ok(true) if added, Ok(false) if duplicate.
    ///
    /// Uses a separate versioned index key with compare-and-swap to ensure
    /// atomicity: if CAS succeeds, we know no other concurrent request got there first.
    pub fn add_interest_if_not_duplicate(
        &self,
        interest: &ListingInterest,
        from_did: &Did,
    ) -> Result<bool> {
        let listing_id = interest.listing_id;

        // Use an index key to track which DIDs have expressed interest
        // This allows atomic CAS without scanning all interests
        let index_key = Self::interest_index_key(&listing_id, from_did);

        // Atomically try to set the index key (only succeeds if not present)
        // compare_and_swap(key, old, new) - if old matches current value, set to new
        let cas_result =
            self.db
                .compare_and_swap(index_key.as_bytes(), None::<&[u8]>, Some(b"1"))?;

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
    /// Used for background tasks like expiring stale listings.
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
    pub fn express_interest(
        &self,
        listing_id: ListingId,
        from_did: Did,
        from_coop: String,
        message: String,
        offer: Option<String>,
    ) -> Result<ListingInterest> {
        // Verify listing exists and is active
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

    /// Expire all listings that have passed their expiry date.
    /// Returns the number of listings that were marked as expired.
    ///
    /// This method is designed to be called periodically by a background task.
    pub fn expire_stale_listings(&self) -> Result<usize> {
        let now = icn_time::current_timestamp_secs();

        // Get all active listings (without expiry filter so we can find the stale ones)
        let all_active = self.store.list_all_matching_status(ListingStatus::Active)?;

        let mut expired_count = 0;
        for mut listing in all_active {
            if listing.is_expired(now) {
                listing.status = ListingStatus::Expired;
                listing.updated_at = now;
                self.store.save(&listing)?;
                expired_count += 1;
                icn_obs::metrics::exchange::listings_expired_inc();
            }
        }

        if expired_count > 0 {
            tracing::info!(count = expired_count, "Expired stale listings");
        }

        Ok(expired_count)
    }
}

impl Default for ListingsManager {
    fn default() -> Self {
        Self::new()
    }
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
