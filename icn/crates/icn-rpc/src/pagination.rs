//! Pagination support for RPC list endpoints
//!
//! This module provides pagination types and helpers for all list operations
//! in the RPC API. Pagination prevents unbounded memory usage and improves
//! API performance by limiting result set sizes.
//!
//! # Architecture
//!
//! - **PageRequest**: Client specifies offset and limit
//! - **PageResponse**: Server returns items with pagination metadata
//! - **Server-side caps**: Maximum page size enforced (default 100)
//!
//! # Usage
//!
//! ```rust
//! use icn_rpc::pagination::{PageRequest, PageResponse, paginate};
//!
//! // Client requests page
//! let request = PageRequest::new(0, 20);
//!
//! // Server applies pagination
//! let items = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
//! let response: PageResponse<i32> = paginate(items, &request, 100);
//!
//! assert_eq!(response.items.len(), 10);
//! assert_eq!(response.total, 10);
//! assert!(!response.has_more);
//! ```

use serde::{Deserialize, Serialize};

/// Default maximum page size
pub const DEFAULT_MAX_PAGE_SIZE: usize = 100;

/// Maximum page size for any list operation
pub const ABSOLUTE_MAX_PAGE_SIZE: usize = 1000;

/// Pagination request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRequest {
    /// Offset into the result set (0-based)
    #[serde(default)]
    pub offset: usize,

    /// Maximum number of items to return
    #[serde(default = "default_limit")]
    pub limit: usize,
}

impl PageRequest {
    /// Create a new page request
    pub fn new(offset: usize, limit: usize) -> Self {
        PageRequest { offset, limit }
    }

    /// Create request for first page with default limit
    pub fn first_page() -> Self {
        PageRequest {
            offset: 0,
            limit: DEFAULT_MAX_PAGE_SIZE,
        }
    }

    /// Create request for next page
    pub fn next_page(&self) -> Self {
        PageRequest {
            offset: self.offset + self.limit,
            limit: self.limit,
        }
    }

    /// Cap the limit to a maximum value
    pub fn cap_limit(&mut self, max: usize) {
        if self.limit > max {
            self.limit = max;
        }
    }
}

impl Default for PageRequest {
    fn default() -> Self {
        Self::first_page()
    }
}

fn default_limit() -> usize {
    DEFAULT_MAX_PAGE_SIZE
}

/// Paginated response with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageResponse<T> {
    /// Items in this page
    pub items: Vec<T>,

    /// Total number of items in the full result set
    pub total: usize,

    /// Whether there are more items beyond this page
    pub has_more: bool,

    /// Current offset (for convenience)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,

    /// Items per page (for convenience)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

impl<T> PageResponse<T> {
    /// Create a new paginated response
    pub fn new(items: Vec<T>, total: usize, offset: usize, limit: usize) -> Self {
        let has_more = offset + items.len() < total;

        PageResponse {
            items,
            total,
            has_more,
            offset: Some(offset),
            limit: Some(limit),
        }
    }

    /// Create response with only items (minimal metadata)
    pub fn simple(items: Vec<T>, total: usize, has_more: bool) -> Self {
        PageResponse {
            items,
            total,
            has_more,
            offset: None,
            limit: None,
        }
    }

    /// Create empty response
    pub fn empty() -> Self {
        PageResponse {
            items: Vec::new(),
            total: 0,
            has_more: false,
            offset: None,
            limit: None,
        }
    }

    /// Map items to a different type
    pub fn map<U, F>(self, f: F) -> PageResponse<U>
    where
        F: FnMut(T) -> U,
    {
        PageResponse {
            items: self.items.into_iter().map(f).collect(),
            total: self.total,
            has_more: self.has_more,
            offset: self.offset,
            limit: self.limit,
        }
    }
}

/// Apply pagination to a collection
///
/// This is a helper function that takes a full collection and returns
/// a paginated slice with metadata.
///
/// # Arguments
///
/// * `items` - Full collection of items
/// * `request` - Pagination request
/// * `max_page_size` - Server-enforced maximum page size
///
/// # Example
///
/// ```
/// use icn_rpc::pagination::{PageRequest, paginate};
///
/// let items = vec![1, 2, 3, 4, 5];
/// let request = PageRequest::new(0, 2);
/// let response = paginate(items, &request, 100);
///
/// assert_eq!(response.items, vec![1, 2]);
/// assert_eq!(response.total, 5);
/// assert!(response.has_more);
/// ```
pub fn paginate<T: Clone>(
    items: Vec<T>,
    request: &PageRequest,
    max_page_size: usize,
) -> PageResponse<T> {
    let total = items.len();
    let limit = request.limit.min(max_page_size);
    let offset = request.offset;

    // Bounds check
    if offset >= total {
        return PageResponse::empty();
    }

    // Extract the page
    let end = (offset + limit).min(total);
    let page_items = items[offset..end].to_vec();
    let has_more = end < total;

    PageResponse {
        items: page_items,
        total,
        has_more,
        offset: Some(offset),
        limit: Some(limit),
    }
}

/// Apply pagination with ownership (avoids cloning)
///
/// Takes ownership of items and returns paginated slice.
pub fn paginate_owned<T>(
    items: Vec<T>,
    request: &PageRequest,
    max_page_size: usize,
) -> PageResponse<T> {
    let total = items.len();
    let limit = request.limit.min(max_page_size);
    let offset = request.offset;

    // Bounds check
    if offset >= total {
        return PageResponse::empty();
    }

    // Extract the page
    let end = (offset + limit).min(total);
    let page_items: Vec<T> = items.into_iter().skip(offset).take(limit).collect();
    let has_more = end < total;

    PageResponse {
        items: page_items,
        total,
        has_more,
        offset: Some(offset),
        limit: Some(limit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_request_creation() {
        let req = PageRequest::new(10, 20);
        assert_eq!(req.offset, 10);
        assert_eq!(req.limit, 20);
    }

    #[test]
    fn test_page_request_first_page() {
        let req = PageRequest::first_page();
        assert_eq!(req.offset, 0);
        assert_eq!(req.limit, DEFAULT_MAX_PAGE_SIZE);
    }

    #[test]
    fn test_page_request_next_page() {
        let req = PageRequest::new(0, 20);
        let next = req.next_page();
        assert_eq!(next.offset, 20);
        assert_eq!(next.limit, 20);
    }

    #[test]
    fn test_page_request_cap_limit() {
        let mut req = PageRequest::new(0, 200);
        req.cap_limit(100);
        assert_eq!(req.limit, 100);

        let mut req2 = PageRequest::new(0, 50);
        req2.cap_limit(100);
        assert_eq!(req2.limit, 50); // Already under cap
    }

    #[test]
    fn test_paginate_first_page() {
        let items = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let request = PageRequest::new(0, 5);
        let response = paginate(items, &request, 100);

        assert_eq!(response.items, vec![1, 2, 3, 4, 5]);
        assert_eq!(response.total, 10);
        assert!(response.has_more);
        assert_eq!(response.offset, Some(0));
        assert_eq!(response.limit, Some(5));
    }

    #[test]
    fn test_paginate_middle_page() {
        let items = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let request = PageRequest::new(5, 3);
        let response = paginate(items, &request, 100);

        assert_eq!(response.items, vec![6, 7, 8]);
        assert_eq!(response.total, 10);
        assert!(response.has_more);
    }

    #[test]
    fn test_paginate_last_page() {
        let items = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let request = PageRequest::new(8, 5);
        let response = paginate(items, &request, 100);

        assert_eq!(response.items, vec![9, 10]);
        assert_eq!(response.total, 10);
        assert!(!response.has_more);
    }

    #[test]
    fn test_paginate_offset_beyond_total() {
        let items = vec![1, 2, 3];
        let request = PageRequest::new(10, 5);
        let response = paginate(items, &request, 100);

        assert!(response.items.is_empty());
        assert_eq!(response.total, 0);
        assert!(!response.has_more);
    }

    #[test]
    fn test_paginate_enforces_max_page_size() {
        let items: Vec<i32> = (1..=100).collect();
        let request = PageRequest::new(0, 200); // Request 200
        let response = paginate(items, &request, 50); // Server caps at 50

        assert_eq!(response.items.len(), 50);
        assert!(response.has_more);
    }

    #[test]
    fn test_paginate_empty_collection() {
        let items: Vec<i32> = vec![];
        let request = PageRequest::new(0, 10);
        let response = paginate(items, &request, 100);

        assert!(response.items.is_empty());
        assert_eq!(response.total, 0);
        assert!(!response.has_more);
    }

    #[test]
    fn test_paginate_owned() {
        let items = vec![1, 2, 3, 4, 5];
        let request = PageRequest::new(1, 2);
        let response = paginate_owned(items, &request, 100);

        assert_eq!(response.items, vec![2, 3]);
        assert_eq!(response.total, 5);
        assert!(response.has_more);
    }

    #[test]
    fn test_page_response_map() {
        let response = PageResponse {
            items: vec![1, 2, 3],
            total: 10,
            has_more: true,
            offset: Some(0),
            limit: Some(3),
        };

        let mapped = response.map(|x| x * 2);

        assert_eq!(mapped.items, vec![2, 4, 6]);
        assert_eq!(mapped.total, 10);
        assert!(mapped.has_more);
    }

    #[test]
    fn test_page_response_empty() {
        let response: PageResponse<i32> = PageResponse::empty();

        assert!(response.items.is_empty());
        assert_eq!(response.total, 0);
        assert!(!response.has_more);
    }

    #[test]
    fn test_serialization() {
        let response = PageResponse {
            items: vec![1, 2, 3],
            total: 10,
            has_more: true,
            offset: Some(0),
            limit: Some(3),
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: PageResponse<i32> = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.items, vec![1, 2, 3]);
        assert_eq!(deserialized.total, 10);
        assert!(deserialized.has_more);
    }

    #[test]
    fn test_default_page_request() {
        let req = PageRequest::default();
        assert_eq!(req.offset, 0);
        assert_eq!(req.limit, DEFAULT_MAX_PAGE_SIZE);
    }

    #[test]
    fn test_page_response_simple() {
        let response = PageResponse::simple(vec![1, 2, 3], 10, true);

        assert_eq!(response.items, vec![1, 2, 3]);
        assert_eq!(response.total, 10);
        assert!(response.has_more);
        assert!(response.offset.is_none());
        assert!(response.limit.is_none());
    }
}
