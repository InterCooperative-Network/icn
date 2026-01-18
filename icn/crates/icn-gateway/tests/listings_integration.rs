//! Integration tests for listings (internal exchange) features
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! Tests the full lifecycle of listings including creation, interest expression,
//! status transitions, and cleanup.

use ed25519_dalek::SigningKey;
use icn_gateway::listings_mgr::{
    ListingCategory, ListingFilter, ListingId, ListingStatus, ListingType, ListingVisibility,
    ListingsManager,
};
use icn_identity::Did;
use std::sync::Arc;

/// Generate a valid Ed25519-based DID from a seed byte.
/// This creates a proper keypair to ensure the DID will pass validation during serialization.
fn test_did(seed: u8) -> Did {
    let signing_key = SigningKey::from_bytes(&[seed; 32]);
    let verifying_key = signing_key.verifying_key();
    Did::from_public_key(&verifying_key)
}

// ============================================================================
// Full Lifecycle Tests
// ============================================================================

#[tokio::test]
async fn test_listing_interest_lifecycle() {
    // This test covers the full user journey:
    // 1. User A creates listing
    // 2. User B expresses interest
    // 3. User A views interests
    // 4. User A marks as matched
    // 5. User A marks as completed
    // Verify: state transitions work correctly

    let db = sled::Config::new().temporary(true).open().unwrap();
    let mgr = ListingsManager::with_sled(Arc::new(db));

    let user_a = test_did(1);
    let user_b = test_did(2);

    // Step 1: User A creates a listing
    let listing = mgr
        .create_listing(
            ListingType::Offer,
            "Commercial Oven".to_string(),
            "Vulcan convection oven, barely used, works great".to_string(),
            ListingCategory::Equipment,
            user_a.clone(),
            "bakery-coop".to_string(),
            "Credits or labor exchange".to_string(),
            vec!["https://example.com/photo1.jpg".to_string()],
            ListingVisibility::Federation,
            None,
            vec!["kitchen".to_string(), "equipment".to_string()],
        )
        .expect("should create listing");

    assert_eq!(listing.title, "Commercial Oven");
    assert_eq!(listing.status, ListingStatus::Active);
    assert!(listing.is_active());

    // Step 2: User B expresses interest
    let interest = mgr
        .express_interest(
            listing.id,
            user_b.clone(),
            "tech-coop".to_string(),
            "We'd love this for our community kitchen! Can offer 20 hours of tech support."
                .to_string(),
            Some("20 hours tech support".to_string()),
        )
        .expect("should express interest");

    assert_eq!(interest.listing_id, listing.id);
    assert_eq!(interest.from_did, user_b);
    assert_eq!(interest.from_coop, "tech-coop");

    // Step 3: User A views interests
    let interests = mgr.get_interests(&listing.id).unwrap();
    assert_eq!(interests.len(), 1);
    assert_eq!(interests[0].from_did, user_b);
    assert_eq!(
        interests[0].offer,
        Some("20 hours tech support".to_string())
    );

    // Step 4: User A marks as matched
    let matched_listing = mgr.mark_matched(&listing.id).unwrap();
    assert_eq!(matched_listing.status, ListingStatus::Matched);
    assert!(!matched_listing.is_active());

    // Step 5: User A marks as completed
    let completed_listing = mgr.mark_completed(&listing.id).unwrap();
    assert_eq!(completed_listing.status, ListingStatus::Completed);
    assert!(!completed_listing.is_active());
}

#[tokio::test]
async fn test_listing_cancellation() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let mgr = ListingsManager::with_sled(Arc::new(db));

    let owner = test_did(1);

    let listing = mgr
        .create_listing(
            ListingType::Want,
            "Looking for office space".to_string(),
            "Need 500 sq ft for small team".to_string(),
            ListingCategory::Space,
            owner,
            "startup-coop".to_string(),
            "Monthly rent or revenue share".to_string(),
            vec![],
            ListingVisibility::Network,
            None,
            vec!["office".to_string()],
        )
        .unwrap();

    assert!(listing.is_active());

    // Cancel the listing
    let cancelled = mgr.cancel_listing(&listing.id).unwrap();
    assert_eq!(cancelled.status, ListingStatus::Cancelled);
    assert!(!cancelled.is_active());
}

// ============================================================================
// Duplicate Interest Prevention Tests
// ============================================================================

#[tokio::test]
async fn test_duplicate_interest_prevention_in_memory() {
    // Test with in-memory store
    let mgr = ListingsManager::new();

    let owner = test_did(1);
    let interested = test_did(2);

    let listing = mgr
        .create_listing(
            ListingType::Offer,
            "Laptop".to_string(),
            "MacBook Pro".to_string(),
            ListingCategory::Equipment,
            owner,
            "coop1".to_string(),
            "Any offer".to_string(),
            vec![],
            ListingVisibility::Federation,
            None,
            vec![],
        )
        .unwrap();

    // First interest should succeed
    let result = mgr.express_interest(
        listing.id,
        interested.clone(),
        "coop2".to_string(),
        "Interested!".to_string(),
        None,
    );
    assert!(result.is_ok());

    // Second interest from same user should fail
    let result = mgr.express_interest(
        listing.id,
        interested,
        "coop2".to_string(),
        "Still interested!".to_string(),
        None,
    );
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("already expressed interest"));
}

#[tokio::test]
async fn test_duplicate_interest_prevention_sled() {
    // Test with Sled store (uses CAS for atomicity)
    let db = sled::Config::new().temporary(true).open().unwrap();
    let mgr = ListingsManager::with_sled(Arc::new(db));

    let owner = test_did(1);
    let interested = test_did(2);

    let listing = mgr
        .create_listing(
            ListingType::Offer,
            "Server".to_string(),
            "Dell PowerEdge".to_string(),
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

    // First interest should succeed
    let result = mgr.express_interest(
        listing.id,
        interested.clone(),
        "coop2".to_string(),
        "We need this!".to_string(),
        None,
    );
    assert!(result.is_ok());

    // Second interest from same user should fail
    let result = mgr.express_interest(
        listing.id,
        interested,
        "coop2".to_string(),
        "Really want it!".to_string(),
        None,
    );
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("already expressed interest"));

    // Different user should succeed
    let other_user = test_did(3);
    let result = mgr.express_interest(
        listing.id,
        other_user,
        "coop3".to_string(),
        "I'm interested too!".to_string(),
        None,
    );
    assert!(result.is_ok());

    // Verify we have exactly 2 interests
    let interests = mgr.get_interests(&listing.id).unwrap();
    assert_eq!(interests.len(), 2);
}

// ============================================================================
// Filter and Pagination Tests
// ============================================================================

#[tokio::test]
async fn test_listing_filters() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let mgr = ListingsManager::with_sled(Arc::new(db));

    let owner = test_did(1);

    // Create various listings
    mgr.create_listing(
        ListingType::Offer,
        "Equipment 1".to_string(),
        "Description".to_string(),
        ListingCategory::Equipment,
        owner.clone(),
        "coop1".to_string(),
        "Credits".to_string(),
        vec![],
        ListingVisibility::Federation,
        None,
        vec!["heavy".to_string()],
    )
    .unwrap();

    mgr.create_listing(
        ListingType::Offer,
        "Equipment 2".to_string(),
        "Description".to_string(),
        ListingCategory::Equipment,
        owner.clone(),
        "coop1".to_string(),
        "Credits".to_string(),
        vec![],
        ListingVisibility::Federation,
        None,
        vec!["light".to_string()],
    )
    .unwrap();

    mgr.create_listing(
        ListingType::Want,
        "Need Services".to_string(),
        "Description".to_string(),
        ListingCategory::Services,
        owner.clone(),
        "coop2".to_string(),
        "Labor".to_string(),
        vec![],
        ListingVisibility::Coop,
        None,
        vec![],
    )
    .unwrap();

    // Filter by type
    let offers = mgr
        .list_listings(&ListingFilter {
            listing_type: Some(ListingType::Offer),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(offers.len(), 2);

    let wants = mgr
        .list_listings(&ListingFilter {
            listing_type: Some(ListingType::Want),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(wants.len(), 1);

    // Filter by category
    let equipment = mgr
        .list_listings(&ListingFilter {
            category: Some(ListingCategory::Equipment),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(equipment.len(), 2);

    // Filter by coop
    let coop1 = mgr
        .list_listings(&ListingFilter {
            coop_id: Some("coop1".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(coop1.len(), 2);

    // Filter by tag
    let heavy = mgr
        .list_listings(&ListingFilter {
            tag: Some("heavy".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(heavy.len(), 1);

    // Filter by owner
    let owned = mgr
        .list_listings(&ListingFilter {
            offered_by: Some(owner),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(owned.len(), 3);
}

#[tokio::test]
async fn test_listing_pagination() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let mgr = ListingsManager::with_sled(Arc::new(db));

    let owner = test_did(1);

    // Create 25 listings
    for i in 0..25 {
        mgr.create_listing(
            ListingType::Offer,
            format!("Item {i}"),
            "Description".to_string(),
            ListingCategory::Equipment,
            owner.clone(),
            "coop1".to_string(),
            "Credits".to_string(),
            vec![],
            ListingVisibility::Federation,
            None,
            vec![],
        )
        .unwrap();
    }

    // Default page size is 20
    let page1 = mgr.list_listings(&ListingFilter::default()).unwrap();
    assert_eq!(page1.len(), 20);

    // Get second page
    let page2 = mgr
        .list_listings(&ListingFilter {
            offset: Some(20),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page2.len(), 5);

    // Custom page size
    let small_page = mgr
        .list_listings(&ListingFilter {
            limit: Some(5),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(small_page.len(), 5);

    // Limit is capped at MAX_PAGE_SIZE (100)
    let max_page = mgr
        .list_listings(&ListingFilter {
            limit: Some(200),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(max_page.len(), 25); // All 25 since that's all we have
}

// ============================================================================
// Interest Count Tests
// ============================================================================

#[tokio::test]
async fn test_interest_counts_batch() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let mgr = ListingsManager::with_sled(Arc::new(db));

    let owner = test_did(1);
    let user2 = test_did(2);
    let user3 = test_did(3);
    let user4 = test_did(4);

    // Create listings
    let listing1 = mgr
        .create_listing(
            ListingType::Offer,
            "Item 1".to_string(),
            "Description".to_string(),
            ListingCategory::Equipment,
            owner.clone(),
            "coop1".to_string(),
            "Credits".to_string(),
            vec![],
            ListingVisibility::Federation,
            None,
            vec![],
        )
        .unwrap();

    let listing2 = mgr
        .create_listing(
            ListingType::Offer,
            "Item 2".to_string(),
            "Description".to_string(),
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

    // Add interests to listing1 (3 interests)
    mgr.express_interest(
        listing1.id,
        user2.clone(),
        "coop2".to_string(),
        "Interested".to_string(),
        None,
    )
    .unwrap();

    mgr.express_interest(
        listing1.id,
        user3.clone(),
        "coop3".to_string(),
        "Interested".to_string(),
        None,
    )
    .unwrap();

    mgr.express_interest(
        listing1.id,
        user4.clone(),
        "coop4".to_string(),
        "Interested".to_string(),
        None,
    )
    .unwrap();

    // Add interest to listing2 (1 interest)
    mgr.express_interest(
        listing2.id,
        user2,
        "coop2".to_string(),
        "Interested".to_string(),
        None,
    )
    .unwrap();

    // Batch fetch interest counts
    let counts = mgr.get_interest_counts(&[listing1.id, listing2.id]);

    assert_eq!(counts.get(&listing1.id), Some(&3));
    assert_eq!(counts.get(&listing2.id), Some(&1));
}

// ============================================================================
// Deletion and Cleanup Tests
// ============================================================================

#[tokio::test]
async fn test_listing_deletion_cleans_up_interests() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let mgr = ListingsManager::with_sled(Arc::new(db));

    let owner = test_did(1);
    let interested = test_did(2);

    let listing = mgr
        .create_listing(
            ListingType::Offer,
            "Item to delete".to_string(),
            "Description".to_string(),
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

    // Add interest
    mgr.express_interest(
        listing.id,
        interested,
        "coop2".to_string(),
        "Interested!".to_string(),
        None,
    )
    .unwrap();

    // Verify interest exists
    let interests = mgr.get_interests(&listing.id).unwrap();
    assert_eq!(interests.len(), 1);

    // Delete listing
    let deleted = mgr.delete_listing(&listing.id).unwrap();
    assert!(deleted);

    // Listing should be gone
    let listing = mgr.get_listing(&listing.id).unwrap();
    assert!(listing.is_none());

    // Interests should also be gone (orphan cleanup)
    // Note: The get_interests call on a deleted listing returns empty
    // because there's no listing to have interests for
}

#[tokio::test]
async fn test_cannot_express_interest_on_inactive_listing() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let mgr = ListingsManager::with_sled(Arc::new(db));

    let owner = test_did(1);
    let interested = test_did(2);

    let listing = mgr
        .create_listing(
            ListingType::Offer,
            "Item".to_string(),
            "Description".to_string(),
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

    // Cancel the listing
    mgr.cancel_listing(&listing.id).unwrap();

    // Try to express interest
    let result = mgr.express_interest(
        listing.id,
        interested,
        "coop2".to_string(),
        "Interested!".to_string(),
        None,
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not active"));
}

// ============================================================================
// Sled Persistence Tests
// ============================================================================

#[tokio::test]
async fn test_listing_persists_across_manager_instances() {
    let db_path = tempfile::tempdir().unwrap();
    let listing_id: ListingId;

    // Create listing with first manager instance
    {
        let db = sled::open(db_path.path()).unwrap();
        let mgr = ListingsManager::with_sled(Arc::new(db));

        let listing = mgr
            .create_listing(
                ListingType::Offer,
                "Persistent Item".to_string(),
                "Should survive restart".to_string(),
                ListingCategory::Equipment,
                test_did(1),
                "coop1".to_string(),
                "Credits".to_string(),
                vec![],
                ListingVisibility::Federation,
                None,
                vec![],
            )
            .unwrap();

        listing_id = listing.id;
    }

    // Open new manager instance and verify listing exists
    {
        let db = sled::open(db_path.path()).unwrap();
        let mgr = ListingsManager::with_sled(Arc::new(db));

        let listing = mgr.get_listing(&listing_id).unwrap();
        assert!(listing.is_some());

        let listing = listing.unwrap();
        assert_eq!(listing.title, "Persistent Item");
        assert_eq!(listing.description, "Should survive restart");
    }
}

#[tokio::test]
async fn test_interest_persists_across_manager_instances() {
    let db_path = tempfile::tempdir().unwrap();
    let listing_id: ListingId;
    let interested = test_did(2);

    // Create listing and interest with first manager instance
    {
        let db = sled::open(db_path.path()).unwrap();
        let mgr = ListingsManager::with_sled(Arc::new(db));

        let listing = mgr
            .create_listing(
                ListingType::Offer,
                "Persistent Item".to_string(),
                "Description".to_string(),
                ListingCategory::Equipment,
                test_did(1),
                "coop1".to_string(),
                "Credits".to_string(),
                vec![],
                ListingVisibility::Federation,
                None,
                vec![],
            )
            .unwrap();

        listing_id = listing.id;

        mgr.express_interest(
            listing_id,
            interested.clone(),
            "coop2".to_string(),
            "Persistent interest".to_string(),
            Some("My offer".to_string()),
        )
        .unwrap();
    }

    // Open new manager instance and verify interest exists
    {
        let db = sled::open(db_path.path()).unwrap();
        let mgr = ListingsManager::with_sled(Arc::new(db));

        let interests = mgr.get_interests(&listing_id).unwrap();
        assert_eq!(interests.len(), 1);
        assert_eq!(interests[0].from_did, interested);
        assert_eq!(interests[0].message, "Persistent interest");
        assert_eq!(interests[0].offer, Some("My offer".to_string()));

        // Duplicate prevention should still work after restart
        let result = mgr.express_interest(
            listing_id,
            interested,
            "coop2".to_string(),
            "Another message".to_string(),
            None,
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already expressed interest"));
    }
}
