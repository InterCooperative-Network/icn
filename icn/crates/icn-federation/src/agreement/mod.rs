//! Inter-Cooperative Agreement Framework
//!
//! This module provides a framework for formal agreements between cooperatives,
//! enabling structured economic relationships with lifecycle management,
//! multi-party signatures, and gossip synchronization.
//!
//! # Agreement Lifecycle
//!
//! ```text
//! Draft ──propose()──> Proposed ──all_signed()──> Active ──expire()──> Terminated
//!                          │                        │
//!                     reject()                  suspend()
//!                          │                        │
//!                          v                        v
//!                        Draft                  Suspended
//!                                                   │
//!                                             terminate()
//!                                                   │
//!                                                   v
//!                                              Terminated
//! ```
//!
//! # Agreement Types
//!
//! - **Trade**: Exchange of goods/services with specified items and currency
//! - **Credit**: Credit line with limit, interest rate, and currency
//! - **ResourceSharing**: Shared use of resources with compensation model
//! - **FederationMembership**: Terms for joining a federation network
//!
//! # Example
//!
//! ```ignore
//! use icn_federation::agreement::{Agreement, AgreementType, PartyRole};
//!
//! // Create a trade agreement draft
//! let agreement = Agreement::new(
//!     "Trade agreement between Food Coop and Tech Coop",
//!     "Monthly produce exchange",
//!     AgreementType::Trade {
//!         items: vec![TradeItem::new("organic vegetables", 100, "USD")],
//!         currency: "USD".to_string(),
//!     },
//! )
//! .with_party(food_coop_did, "food-coop", PartyRole::Proposer)
//! .with_party(tech_coop_did, "tech-coop", PartyRole::Counterparty);
//! ```

mod gossip;
mod manager;
mod store;
mod types;

pub use gossip::{AgreementGossipCallback, AgreementGossipHandler, AgreementMessage};
pub use manager::{AgreementEvent, AgreementEventCallback, AgreementManager};
pub use store::{AgreementStore, AgreementStoreOps, InMemoryAgreementStore};
pub use types::*;
