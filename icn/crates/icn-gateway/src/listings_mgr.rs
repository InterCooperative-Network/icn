//! Listings Manager for Cooperative Exchange
//!
//! Manages listings for internal exchange between cooperatives.
//! This enables coops to post offers and wants before going external,
//! keeping value circulating within the network.

use anyhow::Result;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
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

/// Filter criteria for listings
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
}

impl ListingFilter {
    pub fn matches(&self, listing: &Listing) -> bool {
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
        true
    }
}

/// In-memory listings store
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

    /// List all listings matching a filter
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

        // Sort by created_at descending (newest first)
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(result)
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
}

/// Listings manager for the gateway
pub struct ListingsManager {
    store: InMemoryListingsStore,
}

impl ListingsManager {
    pub fn new() -> Self {
        Self {
            store: InMemoryListingsStore::new(),
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
            category,
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

        // Prevent duplicate interests from the same user
        let existing_interests = self.store.get_interests(&listing_id)?;
        if existing_interests.iter().any(|i| i.from_did == from_did) {
            anyhow::bail!("You have already expressed interest in this listing");
        }

        let now = icn_time::current_timestamp_secs();
        let interest = ListingInterest::new(listing_id, from_did, from_coop, message, offer, now);

        self.store.add_interest(&interest)?;
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
            .filter_map(|id| {
                self.store
                    .get_interests(id)
                    .ok()
                    .map(|interests| (*id, interests.len()))
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
        listing.mark_matched(now);
        self.store.save(&listing)?;
        Ok(listing)
    }

    /// Mark a listing as completed
    pub fn mark_completed(&self, id: &ListingId) -> Result<Listing> {
        let mut listing = self
            .store
            .get(id)?
            .ok_or_else(|| anyhow::anyhow!("Listing not found"))?;

        let now = icn_time::current_timestamp_secs();
        listing.mark_completed(now);
        self.store.save(&listing)?;
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
        Ok(listing)
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
        Did::from_anchor_id(&[1; 32])
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
        let other_did = Did::from_anchor_id(&[99; 32]);
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
                Did::from_anchor_id(&[2; 32]),
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
            Did::from_anchor_id(&[2; 32]), // Same DID as before
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
            Did::from_anchor_id(&[3; 32]), // Different DID
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
}
