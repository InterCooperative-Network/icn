//! Shared HTTP scaffolding for ICN API crates.
//!
//! Provides generic building blocks — auth, error, pagination, validation —
//! that any API-facing crate (apps/governance, apps/ledger, etc.) can reuse
//! without pulling in gateway internals.
//!
//! **No domain knowledge lives here.** This crate is a peer of the kernel
//! transport layer, not a domain crate.

pub mod auth;
pub mod error;
pub mod pagination;
pub mod validation;

pub use auth::{BasicClaims, ClaimsLike};
pub use error::ApiError;
pub use pagination::{
    Cursor, Cursored, Direction, ListPagination, ListQuery, ListResponse, PaginatedList,
    PaginationRequest, PaginationResponse, ResponseMeta, SortField,
};
