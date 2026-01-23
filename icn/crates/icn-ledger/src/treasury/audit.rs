//! Treasury audit trail operations
//!
//! This module provides functionality for recording and retrieving audit trails
//! of all treasury operations, providing complete transparency and accountability.

use crate::labor_shares::{
    BondId, BondPaymentType, CooperativeBond, ScheduledPayout, ShareId, SurplusAllocation,
};
use crate::types::ContentHash;
use anyhow::{bail, Result};
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use super::approvals::ApprovalType;
use super::{uuid_simple, TREASURY_AUDIT_PREFIX};

/// Treasury operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TreasuryOperation {
    /// Deposit funds into treasury
    Deposit {
        from: Did,
        amount: i64,
        currency: String,
        memo: Option<String>,
    },
    /// Withdraw funds from treasury
    Withdraw {
        to: Did,
        amount: i64,
        currency: String,
        purpose: String,
        budget_id: Option<String>,
    },
    /// Allocate funds to a budget
    AllocateBudget {
        budget_id: String,
        amount: i64,
        currency: String,
        purpose: String,
    },
    /// Transfer between budgets
    TransferBetweenBudgets {
        from_budget: String,
        to_budget: String,
        amount: i64,
        currency: String,
        reason: String,
    },
    /// Reallocate unspent budget back to treasury
    ReclaimBudget {
        budget_id: String,
        amount: i64,
        currency: String,
        reason: String,
    },
    /// Create a new budget
    CreateBudget {
        budget_id: String,
        purpose: String,
        amount: i64,
        currency: String,
    },
    /// Cancel a budget
    CancelBudget {
        budget_id: String,
        reason: String,
        return_to_treasury: bool,
    },
    /// Modify spending rule
    ModifySpendingRule {
        rule_id: String,
        new_threshold: Option<i64>,
        new_approval_type: Option<ApprovalType>,
        is_active: Option<bool>,
    },

    // === Labor Share Operations (Razeto Integration) ===
    /// Allocate surplus to labor shareholders
    ///
    /// Distributes surplus proportionally based on labor days.
    /// Requires governance approval via SurplusAllocation proposal.
    AllocateSurplus {
        /// The surplus allocation details
        allocation: SurplusAllocation,
    },

    /// Redeem labor share (payout to departing member)
    ///
    /// Initiates share redemption with a payout schedule.
    /// Requires governance approval via ShareRedemption proposal.
    RedeemShare {
        /// Share being redeemed
        share_id: ShareId,
        /// Total payout amount
        payout_amount: i64,
        /// Recipient member
        recipient: Did,
        /// Payout schedule (immediate or installments)
        payout_schedule: Vec<ScheduledPayout>,
    },

    /// Issue cooperative bond
    ///
    /// Creates a new bond for inter-coop financing.
    /// Requires governance approval via BondIssuance proposal.
    IssueBond {
        /// The bond being issued
        bond: CooperativeBond,
    },

    /// Make bond payment (interest or principal)
    ///
    /// Executes a scheduled bond payment from treasury.
    BondPayment {
        /// Bond ID
        bond_id: BondId,
        /// Type of payment
        payment_type: BondPaymentType,
        /// Payment amount
        amount: i64,
    },

    /// Record labor contribution to a share
    ///
    /// Updates the labor_days on a member's share.
    RecordLaborContribution {
        /// Share to update
        share_id: ShareId,
        /// Labor days to add
        labor_days: u64,
        /// Description of work performed
        description: Option<String>,
    },
}

/// Audit record for treasury operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryAuditRecord {
    /// Unique audit ID
    pub id: String,

    /// Treasury DID
    pub treasury_did: Did,

    /// Operation performed
    pub operation: TreasuryOperation,

    /// Ledger entry hash (if applicable)
    pub ledger_entry_hash: Option<ContentHash>,

    /// Governance proposal ID (if required approval)
    pub proposal_id: Option<String>,

    /// Who performed the operation
    pub performed_by: Did,

    /// When performed
    pub performed_at: u64,

    /// Treasury balance after operation
    pub balance_after: i64,

    /// Additional notes
    pub notes: Option<String>,
}

impl TreasuryAuditRecord {
    /// Create a new audit record
    pub fn new(
        treasury_did: Did,
        operation: TreasuryOperation,
        performed_by: Did,
        balance_after: i64,
    ) -> Self {
        let now = icn_time::current_timestamp_secs();
        Self {
            id: format!("audit-{}-{}", now, uuid_simple()),
            treasury_did,
            operation,
            ledger_entry_hash: None,
            proposal_id: None,
            performed_by,
            performed_at: now,
            balance_after,
            notes: None,
        }
    }

    /// Add ledger entry reference
    pub fn with_ledger_entry(mut self, hash: ContentHash) -> Self {
        self.ledger_entry_hash = Some(hash);
        self
    }

    /// Add proposal reference
    pub fn with_proposal(mut self, proposal_id: String) -> Self {
        self.proposal_id = Some(proposal_id);
        self
    }

    /// Add notes
    pub fn with_notes(mut self, notes: String) -> Self {
        self.notes = Some(notes);
        self
    }
}

/// Paginated audit trail response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedAuditTrail {
    /// Audit records for the current page
    pub records: Vec<TreasuryAuditRecord>,
    /// Total number of records (for pagination UI)
    pub total: usize,
    /// Current offset
    pub offset: usize,
    /// Page size limit
    pub limit: usize,
}

/// Audit operations for TreasuryManager
impl super::TreasuryManager {
    /// Execute a treasury operation with audit logging
    pub fn record_audit(
        &mut self,
        treasury_did: &Did,
        operation: TreasuryOperation,
        performed_by: Did,
        balance_after: i64,
        proposal_id: Option<String>,
        ledger_entry_hash: Option<ContentHash>,
    ) -> Result<TreasuryAuditRecord> {
        if !self.treasuries.contains_key(treasury_did) {
            bail!("Treasury not found: {treasury_did}");
        }

        let mut record =
            TreasuryAuditRecord::new(treasury_did.clone(), operation, performed_by, balance_after);

        if let Some(hash) = ledger_entry_hash {
            record = record.with_ledger_entry(hash);
        }
        if let Some(proposal) = proposal_id {
            record = record.with_proposal(proposal);
        }

        info!(
            audit_id = %record.id,
            treasury_did = %treasury_did,
            operation_type = ?std::mem::discriminant(&record.operation),
            "Recording treasury audit"
        );

        if let Some(ref store) = self.store {
            self.persist_audit_record(&record, store)?;
        }

        Ok(record)
    }

    /// Get audit trail for a treasury with pagination
    ///
    /// Returns a `PaginatedAuditTrail` containing records and total count for UI pagination.
    /// Uses reverse iteration for efficiency - only loads the requested records instead
    /// of loading all records into memory for sorting.
    ///
    /// Note: Records are returned most recent first, based on key ordering
    /// (keys include timestamps in ascending order, so reverse iteration
    /// yields most recent first).
    pub fn get_audit_trail(
        &self,
        treasury_did: &Did,
        limit: usize,
        offset: usize,
    ) -> Result<PaginatedAuditTrail> {
        let Some(ref store) = self.store else {
            return Ok(PaginatedAuditTrail {
                records: Vec::new(),
                total: 0,
                offset,
                limit,
            });
        };

        let prefix = format!("{TREASURY_AUDIT_PREFIX}{treasury_did}");

        // Use optimized reverse pagination - only loads requested records
        // Keys are ordered by timestamp, so reverse gives most recent first
        let (pairs, total) = store.scan_reverse_paginated(prefix.as_bytes(), offset, limit)?;

        let records: Vec<TreasuryAuditRecord> = pairs
            .into_iter()
            .filter_map(|(_, value)| serde_json::from_slice(&value).ok())
            .collect();

        Ok(PaginatedAuditTrail {
            records,
            total,
            offset,
            limit,
        })
    }

    /// Persist audit record to storage (internal helper)
    pub(super) fn persist_audit_record(
        &self,
        record: &TreasuryAuditRecord,
        store: &Arc<dyn Store>,
    ) -> Result<()> {
        // Key includes timestamp for time-ordered retrieval
        let key = format!(
            "{}{}:{}:{}",
            TREASURY_AUDIT_PREFIX, record.treasury_did, record.performed_at, record.id
        );
        let value = serde_json::to_vec(record)?;
        store.put(key.as_bytes(), &value)?;
        Ok(())
    }
}
