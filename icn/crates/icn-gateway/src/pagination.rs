//! Cursor-based pagination for efficient large dataset navigation
//!
//! This module provides cursor-based pagination which is more efficient than
//! offset-based pagination for large datasets because:
//!
//! 1. No need to count/skip over items before the cursor
//! 2. Stable pagination even when data changes
//! 3. Consistent performance regardless of page number
//!
//! # Cursor Format
//!
//! Cursors are base64-encoded JSON objects containing:
//! - `ts`: timestamp (Unix milliseconds)
//! - `id`: unique identifier
//!
//! This allows for efficient seeking to a specific position in ordered data.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};

/// Maximum page size to prevent OOM attacks
pub const MAX_PAGE_SIZE: usize = 100;

/// Default page size
pub const DEFAULT_PAGE_SIZE: usize = 20;

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

/// Paginate a vector of items using cursor-based pagination
///
/// Items must be sorted in descending timestamp order (newest first).
pub fn paginate_items<T: Cursored + Clone>(
    items: Vec<T>,
    request: &PaginationRequest,
) -> PaginatedList<T> {
    let cursor = request.decoded_cursor();
    let limit = request.limit.min(MAX_PAGE_SIZE);

    // Find starting position if cursor provided
    // Cursor points to the LAST item on the previous page, so we want items STRICTLY after it
    let start_idx = if let Some(ref cursor) = cursor {
        items
            .iter()
            .position(|item| {
                // Items are sorted newest first (descending by timestamp)
                // We want to find items with timestamp strictly less than cursor
                // OR same timestamp but ID strictly less (for tie-breaking)
                item.cursor_timestamp() < cursor.ts
                    || (item.cursor_timestamp() == cursor.ts
                        && item.cursor_id() < cursor.id.as_str())
            })
            .unwrap_or(items.len())
    } else {
        0
    };

    // Collect items for this page
    let page_items: Vec<T> = items.iter().skip(start_idx).take(limit).cloned().collect();

    // Check if there are more items
    let has_more = start_idx + limit < items.len();

    // Build next cursor if there are more items
    let next_cursor = if has_more {
        page_items.last().map(|item| item.to_cursor())
    } else {
        None
    };

    // Build prev cursor if we're not at the start
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
mod tests {
    use super::*;

    #[derive(Clone, Debug)]
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
            limit: 1000, // Too large
            direction: Direction::Forward,
        };
        let validated = req.validate();
        assert_eq!(validated.limit, MAX_PAGE_SIZE);

        let req = PaginationRequest {
            cursor: None,
            limit: 0, // Too small
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

        // Items should be in descending order (newest first)
        assert_eq!(result.items[0].id, "item49");
        assert_eq!(result.items[9].id, "item40");
    }

    #[test]
    fn test_paginate_items_with_cursor() {
        let items: Vec<TestItem> = (0..50)
            .rev()
            .map(|i| TestItem {
                id: format!("item{i}"),
                timestamp: 1000000 + i as u64,
            })
            .collect();

        // Get first page
        let request1 = PaginationRequest {
            cursor: None,
            limit: 10,
            direction: Direction::Forward,
        };
        let page1 = paginate_items(items.clone(), &request1);

        // Use cursor to get second page
        let request2 = PaginationRequest {
            cursor: page1.pagination.next_cursor,
            limit: 10,
            direction: Direction::Forward,
        };
        let page2 = paginate_items(items, &request2);

        assert_eq!(page2.items.len(), 10);
        assert!(page2.pagination.has_more);

        // Second page should continue from where first left off
        assert_eq!(page2.items[0].id, "item39");
        assert_eq!(page2.items[9].id, "item30");
    }

    #[test]
    fn test_paginate_items_last_page() {
        let items: Vec<TestItem> = (0..25)
            .rev()
            .map(|i| TestItem {
                id: format!("item{i}"),
                timestamp: 1000000 + i as u64,
            })
            .collect();

        // Get last page (items 5-24 = 20 items, then items 0-4 = 5 items)
        let request = PaginationRequest {
            cursor: Some(Cursor::new(1000005, "item5").encode()),
            limit: 10,
            direction: Direction::Forward,
        };
        let result = paginate_items(items, &request);

        assert_eq!(result.items.len(), 5);
        assert!(!result.pagination.has_more);
        assert!(result.pagination.next_cursor.is_none());
    }

    #[test]
    fn test_paginate_items_empty() {
        let items: Vec<TestItem> = Vec::new();

        let request = PaginationRequest::default();
        let result = paginate_items(items, &request);

        assert_eq!(result.items.len(), 0);
        assert!(!result.pagination.has_more);
        assert!(result.pagination.next_cursor.is_none());
    }

    #[test]
    fn test_pagination_response_serialization() {
        let response = PaginationResponse {
            next_cursor: Some("abc123".to_string()),
            prev_cursor: None,
            count: 10,
            has_more: true,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("next_cursor"));
        assert!(!json.contains("prev_cursor")); // Skipped when None
        assert!(json.contains("count"));
        assert!(json.contains("has_more"));
    }
}
