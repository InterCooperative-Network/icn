//! Standardized pagination, filtering, and response envelopes for API endpoints
//!
//! This module provides a consistent API experience across all list endpoints:
//!
//! ## Request Parameters
//!
//! ```text
//! GET /v1/entities?cursor=<opaque>&limit=50&filter[type]=cooperative&sort=-created_at
//! ```
//!
//! | Parameter | Description |
//! |-----------|-------------|
//! | `cursor` | Opaque cursor for next page |
//! | `limit` | Items per page (default 20, max 100) |
//! | `filter[field]` | Field-specific filters |
//! | `sort` | Sort field, prefix `-` for descending |
//!
//! ## Response Envelope
//!
//! ```json
//! {
//!   "data": [...],
//!   "pagination": {
//!     "cursor": "next_cursor_value",
//!     "has_more": true
//!   },
//!   "meta": {
//!     "request_id": "...",
//!     "timestamp": "..."
//!   }
//! }
//! ```
//!
//! ## Cursor Format
//!
//! Cursors are base64-encoded JSON objects containing:
//! - `ts`: timestamp (Unix milliseconds)
//! - `id`: unique identifier
//!
//! This allows for efficient seeking to a specific position in ordered data.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum page size to prevent OOM attacks
pub const MAX_PAGE_SIZE: usize = 100;

/// Default page size
pub const DEFAULT_PAGE_SIZE: usize = 20;

/// Maximum filter string length to prevent DoS via expensive string matching
pub const MAX_FILTER_LENGTH: usize = 256;

/// Direction of pagination
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Direction {
    /// Forward pagination (newer items first by default)
    #[default]
    Forward,
    /// Backward pagination (older items first)
    Backward,
}

/// Cursor containing position information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cursor {
    /// Timestamp in milliseconds for ordering
    pub ts: u64,
    /// Unique identifier for tie-breaking
    pub id: String,
}

impl Cursor {
    /// Create a new cursor from timestamp and id
    pub fn new(timestamp_ms: u64, id: impl Into<String>) -> Self {
        Self {
            ts: timestamp_ms,
            id: id.into(),
        }
    }

    /// Create cursor from Unix seconds timestamp
    pub fn from_seconds(timestamp_secs: u64, id: impl Into<String>) -> Self {
        Self::new(timestamp_secs * 1000, id)
    }

    /// Encode cursor to string for URL transmission
    pub fn encode(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        URL_SAFE_NO_PAD.encode(json.as_bytes())
    }

    /// Decode cursor from string
    pub fn decode(encoded: &str) -> Option<Self> {
        let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
        let json = String::from_utf8(bytes).ok()?;
        serde_json::from_str(&json).ok()
    }
}

/// Pagination request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationRequest {
    /// Cursor to start from (None = from beginning)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,

    /// Maximum number of items to return
    #[serde(default = "default_limit")]
    pub limit: usize,

    /// Direction of pagination
    #[serde(default)]
    pub direction: Direction,
}

fn default_limit() -> usize {
    DEFAULT_PAGE_SIZE
}

impl Default for PaginationRequest {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: DEFAULT_PAGE_SIZE,
            direction: Direction::default(),
        }
    }
}

impl PaginationRequest {
    /// Validate and normalize pagination parameters
    pub fn validate(self) -> Self {
        Self {
            cursor: self.cursor,
            limit: self.limit.clamp(1, MAX_PAGE_SIZE),
            direction: self.direction,
        }
    }

    /// Get the decoded cursor if present
    pub fn decoded_cursor(&self) -> Option<Cursor> {
        self.cursor.as_ref().and_then(|c| Cursor::decode(c))
    }
}

/// Pagination response metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationResponse {
    /// Cursor for the next page (None if no more items)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,

    /// Cursor for the previous page (None if at beginning)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_cursor: Option<String>,

    /// Number of items in current page
    pub count: usize,

    /// Whether there are more items
    pub has_more: bool,
}

impl PaginationResponse {
    /// Create response with just next cursor
    pub fn with_next(next_cursor: Option<Cursor>, count: usize, has_more: bool) -> Self {
        Self {
            next_cursor: next_cursor.map(|c| c.encode()),
            prev_cursor: None,
            count,
            has_more,
        }
    }

    /// Create response with both cursors
    pub fn with_cursors(
        next_cursor: Option<Cursor>,
        prev_cursor: Option<Cursor>,
        count: usize,
        has_more: bool,
    ) -> Self {
        Self {
            next_cursor: next_cursor.map(|c| c.encode()),
            prev_cursor: prev_cursor.map(|c| c.encode()),
            count,
            has_more,
        }
    }

    /// Create empty response
    pub fn empty() -> Self {
        Self {
            next_cursor: None,
            prev_cursor: None,
            count: 0,
            has_more: false,
        }
    }
}

/// A paginated list of items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedList<T> {
    /// The items on this page
    pub items: Vec<T>,

    /// Pagination metadata
    pub pagination: PaginationResponse,
}

impl<T> PaginatedList<T> {
    /// Create a new paginated list
    pub fn new(items: Vec<T>, pagination: PaginationResponse) -> Self {
        Self { items, pagination }
    }

    /// Create an empty paginated list
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            pagination: PaginationResponse::empty(),
        }
    }
}

/// Helper trait for items that can be cursor-paginated
pub trait Cursored {
    /// Get the timestamp for this item (in milliseconds)
    fn cursor_timestamp(&self) -> u64;

    /// Get the unique ID for this item
    fn cursor_id(&self) -> &str;

    /// Create a cursor for this item
    fn to_cursor(&self) -> Cursor {
        Cursor::new(self.cursor_timestamp(), self.cursor_id())
    }
}

// ============================================================================
// Standard API Response Types
// ============================================================================

/// Standard query parameters for list endpoints
///
/// Supports cursor-based pagination, filtering, and sorting.
///
/// # Example Query String
/// ```text
/// ?cursor=abc123&limit=50&filter[type]=cooperative&filter[status]=active&sort=-created_at
/// ```
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    /// Opaque cursor for pagination (from previous response)
    #[serde(default)]
    pub cursor: Option<String>,

    /// Maximum items to return (default: 20, max: 100)
    #[serde(default = "default_limit")]
    pub limit: usize,

    /// Sort field(s), prefix with `-` for descending
    /// Examples: "created_at", "-created_at", "name,-created_at"
    #[serde(default)]
    pub sort: Option<String>,

    /// Raw query parameters for filter extraction
    /// Filters use the format: filter[field]=value
    #[serde(flatten)]
    pub extra: HashMap<String, String>,
}

impl ListQuery {
    /// Create a new ListQuery with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate and normalize query parameters
    pub fn validate(mut self) -> Self {
        self.limit = self.limit.clamp(1, MAX_PAGE_SIZE);
        self
    }

    /// Get the decoded cursor if present
    pub fn decoded_cursor(&self) -> Option<Cursor> {
        self.cursor.as_ref().and_then(|c| Cursor::decode(c))
    }

    /// Parse offset-based cursor for simple pagination
    ///
    /// Returns Ok(offset) if cursor is valid or None.
    /// Returns Err with message if cursor format is invalid.
    ///
    /// Valid formats:
    /// - None -> Ok(0)
    /// - "offset:123" -> Ok(123)
    /// - "123" -> Ok(123) (backwards compatibility)
    /// - "invalid" -> Err
    pub fn parse_offset_cursor(&self) -> Result<usize, String> {
        match &self.cursor {
            None => Ok(0),
            Some(cursor) => {
                // Try "offset:N" format first
                if let Some(num_str) = cursor.strip_prefix("offset:") {
                    num_str.parse::<usize>().map_err(|_| {
                        format!("Invalid cursor format: expected 'offset:<number>', got '{cursor}'")
                    })
                } else {
                    // Try plain number for backwards compatibility
                    cursor.parse::<usize>().map_err(|_| {
                        format!("Invalid cursor format: expected 'offset:<number>', got '{cursor}'")
                    })
                }
            }
        }
    }

    /// Extract filters from query parameters
    ///
    /// Parses `filter[field]=value` parameters into a HashMap.
    pub fn filters(&self) -> HashMap<String, String> {
        let mut filters = HashMap::new();
        for (key, value) in &self.extra {
            if let Some(field) = key
                .strip_prefix("filter[")
                .and_then(|s| s.strip_suffix(']'))
            {
                filters.insert(field.to_string(), value.clone());
            }
        }
        filters
    }

    /// Get a specific filter value
    ///
    /// Checks both `filter[field]` format and direct `field` format for backwards compatibility.
    /// Returns None if filter exceeds MAX_FILTER_LENGTH.
    pub fn filter(&self, field: &str) -> Option<&str> {
        // Check filter[field] format first (new standard)
        let filter_key = format!("filter[{field}]");
        self.extra
            .get(&filter_key)
            // Fall back to direct field name (backwards compatibility)
            .or_else(|| self.extra.get(field))
            .map(|s| s.as_str())
            // Prevent DoS via expensive string matching
            .filter(|s| s.len() <= MAX_FILTER_LENGTH)
    }

    /// Validate all filter values don't exceed MAX_FILTER_LENGTH
    ///
    /// Returns Err with a message if any filter is too long.
    pub fn validate_filters(&self) -> Result<(), String> {
        for (key, value) in &self.extra {
            if value.len() > MAX_FILTER_LENGTH {
                return Err(format!(
                    "Filter '{key}' exceeds maximum length of {MAX_FILTER_LENGTH} characters"
                ));
            }
        }
        Ok(())
    }

    /// Parse sort fields into a list of (field, ascending) tuples
    pub fn sort_fields(&self) -> Vec<SortField> {
        self.sort
            .as_ref()
            .map(|s| SortField::parse_list(s))
            .unwrap_or_default()
    }

    /// Convert to a PaginationRequest for backward compatibility
    pub fn to_pagination_request(&self) -> PaginationRequest {
        PaginationRequest {
            cursor: self.cursor.clone(),
            limit: self.limit.clamp(1, MAX_PAGE_SIZE),
            direction: Direction::Forward,
        }
    }
}

/// A sort field with direction
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortField {
    /// Field name to sort by
    pub field: String,
    /// Sort direction (true = ascending, false = descending)
    pub ascending: bool,
}

impl SortField {
    /// Create a new ascending sort field
    pub fn asc(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            ascending: true,
        }
    }

    /// Create a new descending sort field
    pub fn desc(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            ascending: false,
        }
    }

    /// Parse a single sort field (e.g., "-created_at" or "name")
    pub fn parse(s: &str) -> Self {
        if let Some(field) = s.strip_prefix('-') {
            Self::desc(field)
        } else {
            Self::asc(s)
        }
    }

    /// Parse a comma-separated list of sort fields
    pub fn parse_list(s: &str) -> Vec<Self> {
        s.split(',')
            .map(|field| field.trim())
            .filter(|field| !field.is_empty())
            .map(Self::parse)
            .collect()
    }
}

/// Response metadata for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMeta {
    /// Unique request identifier for tracing
    pub request_id: String,

    /// ISO 8601 timestamp when response was generated
    pub timestamp: String,
}

impl ResponseMeta {
    /// Create metadata with a generated request ID and current timestamp
    pub fn now() -> Self {
        Self {
            request_id: generate_request_id(),
            timestamp: chrono_timestamp(),
        }
    }

    /// Create metadata with a specific request ID
    pub fn with_request_id(request_id: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            timestamp: chrono_timestamp(),
        }
    }
}

/// Standard response envelope for list endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse<T> {
    /// The items on this page
    pub data: Vec<T>,

    /// Pagination metadata
    pub pagination: ListPagination,

    /// Response metadata
    pub meta: ResponseMeta,
}

/// Pagination metadata in ListResponse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPagination {
    /// Cursor for the next page (None if no more items)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,

    /// Whether there are more items after this page
    pub has_more: bool,

    /// Number of items in current page
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,

    /// Total number of items (optional, can be expensive to compute)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
}

impl ListPagination {
    /// Create pagination metadata for a page with more items
    pub fn with_cursor(cursor: impl Into<String>, count: usize) -> Self {
        Self {
            cursor: Some(cursor.into()),
            has_more: true,
            count: Some(count),
            total: None,
        }
    }

    /// Create pagination metadata for the last page
    pub fn last_page(count: usize) -> Self {
        Self {
            cursor: None,
            has_more: false,
            count: Some(count),
            total: None,
        }
    }

    /// Create empty pagination
    pub fn empty() -> Self {
        Self {
            cursor: None,
            has_more: false,
            count: Some(0),
            total: None,
        }
    }

    /// Add total count to pagination
    pub fn with_total(mut self, total: usize) -> Self {
        self.total = Some(total);
        self
    }
}

impl<T: Serialize> ListResponse<T> {
    /// Create a new list response
    pub fn new(data: Vec<T>, pagination: ListPagination) -> Self {
        Self {
            data,
            pagination,
            meta: ResponseMeta::now(),
        }
    }

    /// Create a list response with specific metadata
    pub fn with_meta(data: Vec<T>, pagination: ListPagination, meta: ResponseMeta) -> Self {
        Self {
            data,
            pagination,
            meta,
        }
    }

    /// Create an empty list response
    pub fn empty() -> Self {
        Self::new(Vec::new(), ListPagination::empty())
    }

    /// Create a list response from a PaginatedList
    pub fn from_paginated_list<U: Into<T>>(list: PaginatedList<U>) -> Self
    where
        T: From<U>,
    {
        let pagination = ListPagination {
            cursor: list.pagination.next_cursor,
            has_more: list.pagination.has_more,
            count: Some(list.pagination.count),
            total: None,
        };
        Self::new(list.items.into_iter().map(Into::into).collect(), pagination)
    }
}

/// Helper to create a ListResponse from items with cursor-based pagination
pub fn create_list_response<T, F>(
    items: Vec<T>,
    query: &ListQuery,
    get_cursor: F,
) -> ListResponse<T>
where
    T: Serialize + Clone,
    F: Fn(&T) -> Cursor,
{
    let limit = query.limit.min(MAX_PAGE_SIZE);
    let cursor = query.decoded_cursor();

    // Find starting position if cursor provided
    let start_idx = if let Some(ref cursor) = cursor {
        items
            .iter()
            .position(|item| {
                let item_cursor = get_cursor(item);
                item_cursor.ts < cursor.ts
                    || (item_cursor.ts == cursor.ts && item_cursor.id < cursor.id)
            })
            .unwrap_or(items.len())
    } else {
        0
    };

    // Collect items for this page
    let page_items: Vec<T> = items.iter().skip(start_idx).take(limit).cloned().collect();
    let count = page_items.len();
    let has_more = start_idx + limit < items.len();

    // Build pagination
    let pagination = if has_more {
        if let Some(last_item) = page_items.last() {
            ListPagination::with_cursor(get_cursor(last_item).encode(), count)
        } else {
            ListPagination::empty()
        }
    } else {
        ListPagination::last_page(count)
    };

    ListResponse::new(page_items, pagination)
}

// Helper functions for metadata generation

fn generate_request_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0);
    format!("req_{timestamp:x}_{:04x}", rand_u16())
}

fn rand_u16() -> u16 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut hasher = DefaultHasher::new();

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    nanos.hash(&mut hasher);

    std::thread::current().id().hash(&mut hasher);

    hasher.finish() as u16
}

fn chrono_timestamp() -> String {
    use chrono::Utc;
    Utc::now().to_rfc3339()
}

// ============================================================================
// Legacy Pagination (for backward compatibility)
// ============================================================================

/// Paginate a vector of items using cursor-based pagination
///
/// Items must be sorted in descending timestamp order (newest first).
pub fn paginate_items<T: Cursored + Clone>(
    items: Vec<T>,
    request: &PaginationRequest,
) -> PaginatedList<T> {
    let cursor = request.decoded_cursor();
    let limit = request.limit.min(MAX_PAGE_SIZE);

    let start_idx = if let Some(ref cursor) = cursor {
        items
            .iter()
            .position(|item| {
                item.cursor_timestamp() < cursor.ts
                    || (item.cursor_timestamp() == cursor.ts
                        && item.cursor_id() < cursor.id.as_str())
            })
            .unwrap_or(items.len())
    } else {
        0
    };

    let page_items: Vec<T> = items.iter().skip(start_idx).take(limit).cloned().collect();

    let has_more = start_idx + limit < items.len();

    let next_cursor = if has_more {
        page_items.last().map(|item| item.to_cursor())
    } else {
        None
    };

    let prev_cursor = if start_idx > 0 {
        items
            .get(start_idx.saturating_sub(1))
            .map(|item| item.to_cursor())
    } else {
        None
    };

    let count = page_items.len();
    PaginatedList::new(
        page_items,
        PaginationResponse::with_cursors(next_cursor, prev_cursor, count, has_more),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Serialize)]
    struct TestItem {
        id: String,
        timestamp: u64,
    }

    impl Cursored for TestItem {
        fn cursor_timestamp(&self) -> u64 {
            self.timestamp
        }

        fn cursor_id(&self) -> &str {
            &self.id
        }
    }

    #[test]
    fn test_cursor_encode_decode() {
        let cursor = Cursor::new(1234567890000, "item123");
        let encoded = cursor.encode();
        let decoded = Cursor::decode(&encoded).unwrap();

        assert_eq!(decoded.ts, 1234567890000);
        assert_eq!(decoded.id, "item123");
    }

    #[test]
    fn test_cursor_decode_invalid() {
        assert!(Cursor::decode("not-valid-base64!!!").is_none());
        assert!(Cursor::decode("").is_none());
    }

    #[test]
    fn test_pagination_request_validation() {
        let req = PaginationRequest {
            cursor: None,
            limit: 1000,
            direction: Direction::Forward,
        };
        let validated = req.validate();
        assert_eq!(validated.limit, MAX_PAGE_SIZE);

        let req = PaginationRequest {
            cursor: None,
            limit: 0,
            direction: Direction::Forward,
        };
        let validated = req.validate();
        assert_eq!(validated.limit, 1);
    }

    #[test]
    fn test_paginate_items_first_page() {
        let items: Vec<TestItem> = (0..50)
            .rev()
            .map(|i| TestItem {
                id: format!("item{i}"),
                timestamp: 1000000 + i as u64,
            })
            .collect();

        let request = PaginationRequest {
            cursor: None,
            limit: 10,
            direction: Direction::Forward,
        };

        let result = paginate_items(items, &request);

        assert_eq!(result.items.len(), 10);
        assert!(result.pagination.has_more);
        assert!(result.pagination.next_cursor.is_some());
        assert!(result.pagination.prev_cursor.is_none());

        assert_eq!(result.items[0].id, "item49");
        assert_eq!(result.items[9].id, "item40");
    }

    #[test]
    fn test_sort_field_parse() {
        let field = SortField::parse("created_at");
        assert_eq!(field.field, "created_at");
        assert!(field.ascending);

        let field = SortField::parse("-created_at");
        assert_eq!(field.field, "created_at");
        assert!(!field.ascending);
    }

    #[test]
    fn test_parse_offset_cursor() {
        let query = ListQuery {
            cursor: None,
            limit: 20,
            sort: None,
            extra: HashMap::new(),
        };
        assert_eq!(query.parse_offset_cursor(), Ok(0));

        let query = ListQuery {
            cursor: Some("offset:42".to_string()),
            limit: 20,
            sort: None,
            extra: HashMap::new(),
        };
        assert_eq!(query.parse_offset_cursor(), Ok(42));
    }
}
