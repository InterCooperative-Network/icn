# Action Items and Internal Exchange Features

This document describes the architecture and implementation of the Action Item Tracker and Internal Exchange (Listings) features in ICN Gateway.

## Overview

These features address two key cooperative needs:
1. **Action Items**: Track tasks and commitments from governance meetings
2. **Internal Exchange**: Post offers/wants before going to external markets, keeping value circulating within the network

## Action Items

### Purpose

Action items help cooperatives track tasks that arise from board meetings, proposals, and day-to-day operations. They provide accountability by assigning items to members and tracking completion.

### Data Model

```rust
ActionItem {
    id: ActionItemId,           // Unique identifier (UUID)
    domain_id: GovernanceDomainId,  // Governance domain this belongs to
    title: String,              // Short title (max 200 chars)
    description: Option<String>, // Full description (max 5000 chars)
    assignee: Option<Did>,      // Who is responsible
    due_date: Option<u64>,      // Unix timestamp deadline
    priority: ActionItemPriority, // High, Medium, Low
    status: ActionItemStatus,   // Pending, InProgress, Completed, Deferred
    created_by: Did,            // Who created this
    created_at: u64,            // Creation timestamp
    updated_at: u64,            // Last update timestamp
    linked_proposal: Option<ProposalId>, // Associated proposal
    meeting_context: Option<String>,     // e.g., "2026-01-17 board meeting"
    tags: Vec<String>,          // Categorization tags
}
```

### Storage

**Sled Key Format**: `v1:action_item:{domain_id}:{item_id}`

The `v1:` prefix enables future schema migrations. When the data model changes:
1. Increment to `v2:`
2. Add migration logic to read `v1:` keys and convert to `v2:`
3. Eventually deprecate `v1:` support

### Authorization

| Operation | Who Can Perform |
|-----------|-----------------|
| Create | Any domain member with `governance:write` scope |
| Read | Any domain member with `governance:read` scope |
| Update | Creator only |
| Update Status | Creator OR Assignee |
| Delete | Creator only |

### Filtering & Sorting

Action items support filtering by:
- `status`: Pending, InProgress, Completed, Deferred
- `assignee`: Filter by assigned DID
- `priority`: High, Medium, Low
- `overdue`: Boolean flag for items past due date
- `tag`: Filter by specific tag
- `linked_proposal`: Filter by associated proposal

Default sort order: `due_date ASC`, then `priority DESC`, then `created_at DESC`

### Overdue Logic

An item is considered overdue when:
```rust
now > due_date  // Strictly greater than, not at the deadline
```

This means items are NOT overdue AT the deadline, only AFTER it passes.

## Internal Exchange (Listings)

### Purpose

The internal exchange enables cooperatives to post offers and wants before going to external markets. When a coop has an unused asset (equipment, surplus materials, space), they can check if another network coop needs it first, keeping value circulating within the network.

### Data Model

```rust
Listing {
    id: ListingId,              // Unique identifier (UUID)
    listing_type: ListingType,  // Offer or Want
    title: String,              // Short title (max 200 chars)
    description: String,        // Full description (max 5000 chars)
    category: ListingCategory,  // Equipment, Services, Materials, Space, Other
    photos: Vec<String>,        // URLs (https:// or ipfs://)
    offered_by: Did,            // Who posted this
    coop_id: String,            // Which coop this belongs to
    seeking: String,            // What they want in exchange (max 1000 chars)
    visibility: ListingVisibility, // Coop, Federation, Network
    status: ListingStatus,      // Active, Matched, Completed, Expired, Cancelled
    created_at: u64,            // Creation timestamp
    updated_at: u64,            // Last update timestamp
    expires_at: Option<u64>,    // Optional expiry (max 1 year in future)
    tags: Vec<String>,          // Searchability tags (max 15)
}

ListingInterest {
    id: Uuid,                   // Unique identifier
    listing_id: ListingId,      // Which listing
    from_did: Did,              // Who expressed interest
    from_coop: String,          // Their coop
    message: String,            // Message to owner (max 2000 chars)
    offer: Option<String>,      // What they're offering (max 1000 chars)
    created_at: u64,            // When expressed
}
```

### Storage

**Sled Key Formats**:
- Listings: `v1:listing:{id}`
- Interests: `v1:interest:{listing_id}:{interest_id}`
- Interest Index: `v1:interest_idx:{listing_id}:{from_did}` (for duplicate prevention)

The interest index enables atomic duplicate detection using Sled's compare-and-swap operation.

### Authorization

| Operation | Who Can Perform |
|-----------|-----------------|
| Create Listing | Any member with `coop:write` scope |
| Read Listings | Any member with `coop:read` scope |
| Update Listing | Owner only |
| Delete Listing | Owner only |
| Express Interest | Any member except owner |
| View Interests | Listing owner only |

### Privacy

**Interest Count Privacy**: Interest counts are only visible to listing owners. This prevents competitors from seeing market demand signals.

```rust
// In list_listings: only fetch counts for owned listings
let owned_listing_ids: Vec<ListingId> = listings
    .iter()
    .filter(|l| caller_did.as_ref().is_some_and(|did| did == &l.offered_by))
    .map(|l| l.id)
    .collect();

let interest_counts = mgr.get_interest_counts(&owned_listing_ids);
```

### Race Condition Prevention

Duplicate interest prevention uses atomic operations:

**In-Memory Store**: Check and insert under single write lock
```rust
let mut interests = self.interests.write()?;
let existing = interests.entry(listing_id).or_default();
if existing.iter().any(|i| &i.from_did == from_did) {
    return Ok(false); // Duplicate
}
existing.push(interest.clone());
Ok(true)
```

**Sled Store**: Compare-and-swap on index key
```rust
let index_key = format!("v1:interest_idx:{}:{}", listing_id, from_did);
let cas_result = db.compare_and_swap(index_key, None::<&[u8]>, Some(b"1"))?;
if cas_result.is_err() {
    return Ok(false); // Key existed, duplicate
}
// CAS succeeded, insert actual interest data
```

### Photo URL Security

Photo URLs undergo comprehensive validation:

1. **Scheme Validation**: Only `https://` and `ipfs://` allowed
2. **SSRF Protection**: Blocks private/internal IP addresses:
   - RFC1918 private ranges (10.x, 172.16-31.x, 192.168.x)
   - Loopback (127.x.x.x, ::1)
   - Link-local (169.254.x.x, fe80::/10)
   - Carrier-grade NAT (100.64.0.0/10)
   - Documentation ranges
3. **Hostname Blocking**: Rejects localhost, .local, .internal, .localhost
4. **Format Validation**: Proper URL parsing, no CRLF injection

### Pagination

Both features support pagination:
- `limit`: Max items per page (default 20, max 100)
- `offset`: Number of items to skip

```rust
pub const MAX_PAGE_SIZE: usize = 100;
pub const DEFAULT_PAGE_SIZE: usize = 20;
```

## API Endpoints

### Action Items

```
POST   /gov/domains/{domain_id}/action-items     - Create action item
GET    /gov/domains/{domain_id}/action-items     - List with filters
GET    /gov/domains/{domain_id}/action-items/{id} - Get by ID
PUT    /gov/domains/{domain_id}/action-items/{id} - Update
DELETE /gov/domains/{domain_id}/action-items/{id} - Delete
```

### Listings

```
POST   /listings                    - Create listing
GET    /listings                    - Browse (filters: type, category, status, coop, tag)
GET    /listings/my                 - Get caller's listings
GET    /listings/{id}               - Get details
PUT    /listings/{id}               - Update
DELETE /listings/{id}               - Delete
PUT    /listings/{id}/status        - Update status (matched/completed/cancelled)
POST   /listings/{id}/interest      - Express interest
GET    /listings/{id}/interests     - Get interests (owner only)
```

## Storage Abstraction

Both features use trait-based storage abstraction:

```rust
pub trait ActionItemStoreBackend: Send + Sync {
    fn save(&self, item: &ActionItem) -> Result<()>;
    fn get(&self, domain_id: &GovernanceDomainId, id: &ActionItemId) -> Result<Option<ActionItem>>;
    fn list(&self, domain_id: &GovernanceDomainId, filter: &ActionItemFilter) -> Result<Vec<ActionItem>>;
    fn delete(&self, domain_id: &GovernanceDomainId, id: &ActionItemId) -> Result<bool>;
}

pub trait ListingsStoreBackend: Send + Sync {
    fn save(&self, listing: &Listing) -> Result<()>;
    fn get(&self, id: &ListingId) -> Result<Option<Listing>>;
    fn list(&self, filter: &ListingFilter) -> Result<Vec<Listing>>;
    fn delete(&self, id: &ListingId) -> Result<bool>;
    fn add_interest(&self, interest: &ListingInterest) -> Result<()>;
    fn get_interests(&self, listing_id: &ListingId) -> Result<Vec<ListingInterest>>;
    fn add_interest_if_not_duplicate(&self, interest: &ListingInterest, from_did: &Did) -> Result<bool>;
}
```

This enables:
- **In-memory storage** for testing (fast, isolated)
- **Sled storage** for production (persistent, crash-safe)
- Future backends (PostgreSQL, etc.) without API changes

## Performance Considerations

### Batch Operations

Interest counts are fetched in batch to avoid N+1 queries:
```rust
pub fn get_interest_counts(&self, listing_ids: &[ListingId]) -> HashMap<ListingId, usize>
```

### Efficient Filtering

The `offered_by` filter enables server-side "my listings" queries instead of fetching all listings and filtering client-side.

### Sorting

Action items are sorted by due_date, priority, then created_at. Listings are sorted by created_at descending (newest first).

### Future Optimization

For datasets exceeding 10K items, consider adding secondary indexes for common filters (status, assignee, category). Current implementation uses `scan_prefix` which is O(n).

## Orphan Cleanup

When a listing is deleted, associated data is cleaned up:
```rust
pub fn delete(&self, id: &ListingId) -> Result<bool> {
    // Remove listing
    let existed = self.db.remove(listing_key)?;

    // Clean up interests
    for item in self.db.scan_prefix(interest_prefix) {
        self.db.remove(key)?;
    }

    // Clean up interest index keys
    for item in self.db.scan_prefix(interest_idx_prefix) {
        self.db.remove(key)?;
    }

    Ok(existed)
}
```

## Frontend Integration

The pilot-ui implements these features with:
- **Action Items Tab**: List view with filters, quick-add, status updates
- **Exchange Tab**: Card-based listings, photo galleries, interest forms

Security measures:
- All user content escaped with `escapeHtml()` before rendering
- Uses `textContent` instead of `innerHTML` for dynamic content
- Input validation mirrors backend constraints

## Operations

### Listing Expiry Management

Listings can have optional expiry dates. The system provides two mechanisms for handling expired listings:

1. **Automatic Filtering**: Expired Active listings are automatically filtered out of list queries. Users won't see stale listings in search results, but the data is preserved.

2. **Manual Cleanup**: The `expire_stale_listings()` method can be called to transition all expired Active listings to Expired status:

```rust
// In a maintenance task or admin endpoint
let count = listings_manager.expire_stale_listings()?;
tracing::info!("Expired {} stale listings", count);
```

For the pilot phase, this can be triggered manually via an admin endpoint or script. For production, consider adding a background task:

```rust
// Example: hourly expiry check
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(3600));
    loop {
        interval.tick().await;
        if let Err(e) = listings_manager.expire_stale_listings() {
            tracing::error!("Failed to expire listings: {}", e);
        }
    }
});
```

### Performance Limits

**Pilot Phase Limits** (tested and supported):
- Up to 10,000 listings per deployment
- Up to 1,000 interests per listing
- Up to 100 active users per cooperative

**Scaling Considerations**:
- List queries scan all listings matching status filter - O(n) complexity
- For >10K listings, add secondary Sled indexes for status/category/offered_by
- Interest cleanup on listing delete is O(m) where m = interest count
- Batch operations (get_interest_counts) prevent N+1 query patterns

### Rate Limiting

Interest submissions are protected by two layers of rate limiting:

1. **DID-based Rate Limiting**: Applied via middleware to all authenticated endpoints. Uses token bucket algorithm with 60 burst capacity and 6 req/sec sustained rate for write operations.

2. **IP-based Rate Limiting**: Applied specifically to the interest submission endpoint (`POST /listings/{id}/interest`) for additional DoS protection. This prevents attackers from rotating DIDs to spam interest submissions.

### Monitoring

**Exchange Metrics** (Prometheus):
- `icn_exchange_listings_created_total{listing_type, category}` - Listing creation by type/category
- `icn_exchange_listings_completed_total` - Successful exchanges
- `icn_exchange_listings_expired_total` - Expired listings (from cleanup task)
- `icn_exchange_interests_expressed_total{listing_type}` - Interest expression activity
- `icn_exchange_listings_active` (gauge) - Current active listing count
- `icn_exchange_listing_time_to_match_seconds` (histogram) - Time to first interest match

**Action Item Metrics** (Prometheus):
- `icn_action_items_created_total{priority}` - Action items created by priority
- `icn_action_items_completed_total` - Completed action items
- `icn_action_items_deferred_total` - Deferred action items
- `icn_action_items_status_changes_total{from_status, to_status}` - Status transitions
- `icn_action_items_pending` (gauge) - Current pending items
- `icn_action_items_in_progress` (gauge) - Current in-progress items
- `icn_action_items_overdue` (gauge) - Current overdue items
- `icn_action_items_time_to_complete_seconds` (histogram) - Time to completion
