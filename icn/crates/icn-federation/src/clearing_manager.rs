//! Clearing Manager (Phase F3)
//!
//! Manages bilateral clearing agreements and cross-cooperative transfers.

use crate::clearing::{
    BilateralClearingAgreement, ClearingPosition, CrossCoopTransfer, TransferStatus,
};
use crate::error::{FederationError, Result};
use crate::metrics;
use crate::types::current_timestamp;
use icn_store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{debug, info, warn};

/// Storage key prefixes
const AGREEMENT_PREFIX: &[u8] = b"federation/clearing/agreements/";
const POSITION_PREFIX: &[u8] = b"federation/clearing/positions/";
const TRANSFER_PREFIX: &[u8] = b"federation/clearing/transfers/";

/// Manager for clearing agreements and settlements
pub struct ClearingManager {
    store: Arc<dyn Store>,
    agreements: RwLock<HashMap<String, BilateralClearingAgreement>>,
    positions: RwLock<HashMap<String, ClearingPosition>>,
    own_coop_id: String,
}

impl ClearingManager {
    /// Create a new clearing manager
    pub fn new(store: Arc<dyn Store>, own_coop_id: String) -> Result<Self> {
        let manager = Self {
            store,
            agreements: RwLock::new(HashMap::new()),
            positions: RwLock::new(HashMap::new()),
            own_coop_id,
        };

        manager.load_from_store()?;
        Ok(manager)
    }

    fn load_from_store(&self) -> Result<()> {
        // Load agreements
        let agreement_entries = self.store.scan(AGREEMENT_PREFIX)?;
        let mut agreements = self.agreements.write().unwrap_or_else(|poisoned| {
            warn!("Agreements lock poisoned, recovering");
            poisoned.into_inner()
        });
        for (_key, value) in agreement_entries {
            if let Ok(agreement) = serde_json::from_slice::<BilateralClearingAgreement>(&value) {
                agreements.insert(agreement.agreement_id.clone(), agreement);
            }
        }
        drop(agreements);

        // Load positions
        let position_entries = self.store.scan(POSITION_PREFIX)?;
        let mut positions = self.positions.write().unwrap_or_else(|poisoned| {
            warn!("Positions lock poisoned, recovering");
            poisoned.into_inner()
        });
        for (_key, value) in position_entries {
            if let Ok(position) = serde_json::from_slice::<ClearingPosition>(&value) {
                positions.insert(position.agreement_id.clone(), position);
            }
        }
        drop(positions);

        let count = self
            .agreements
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Agreements lock poisoned, recovering");
                poisoned.into_inner()
            })
            .len();
        metrics::clearing::agreements_active_set(count);
        info!("Loaded {} clearing agreements", count);
        Ok(())
    }

    fn agreement_key(agreement_id: &str) -> Vec<u8> {
        let mut key = AGREEMENT_PREFIX.to_vec();
        key.extend(agreement_id.as_bytes());
        key
    }

    fn position_key(agreement_id: &str) -> Vec<u8> {
        let mut key = POSITION_PREFIX.to_vec();
        key.extend(agreement_id.as_bytes());
        key
    }

    fn transfer_key(transfer_id: &str) -> Vec<u8> {
        let mut key = TRANSFER_PREFIX.to_vec();
        key.extend(transfer_id.as_bytes());
        key
    }

    /// Create a new clearing agreement (returns agreement ID)
    pub fn create_agreement(&self, agreement: BilateralClearingAgreement) -> Result<String> {
        let agreement_id = agreement.agreement_id.clone();

        // Check if agreement already exists
        if self
            .agreements
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Agreements lock poisoned, recovering");
                poisoned.into_inner()
            })
            .contains_key(&agreement_id)
        {
            return Err(FederationError::ClearingAgreementExists(
                agreement.coop_a.clone(),
                agreement.coop_b.clone(),
            ));
        }

        // Persist
        let key = Self::agreement_key(&agreement_id);
        let value = serde_json::to_vec(&agreement)?;
        self.store.put(&key, &value)?;

        // Create initial position
        let position = ClearingPosition::new(agreement_id.clone());
        let pos_key = Self::position_key(&agreement_id);
        let pos_value = serde_json::to_vec(&position)?;
        self.store.put(&pos_key, &pos_value)?;

        // Update caches
        self.agreements
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Agreements lock poisoned, recovering");
                poisoned.into_inner()
            })
            .insert(agreement_id.clone(), agreement.clone());
        self.positions
            .write()
            .unwrap_or_else(|poisoned| {
                warn!("Positions lock poisoned, recovering");
                poisoned.into_inner()
            })
            .insert(agreement_id.clone(), position);

        // Metrics
        metrics::clearing::agreements_created_inc(&agreement.coop_a, &agreement.coop_b);
        let count = self
            .agreements
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Agreements lock poisoned, recovering");
                poisoned.into_inner()
            })
            .len();
        metrics::clearing::agreements_active_set(count);

        info!("Created clearing agreement: {}", agreement_id);
        Ok(agreement_id)
    }

    /// Accept an agreement by adding signature
    pub fn accept_agreement(
        &self,
        agreement_id: &str,
        signer_did: icn_identity::Did,
        signature: Vec<u8>,
    ) -> Result<()> {
        let mut agreements = self.agreements.write().unwrap_or_else(|poisoned| {
            warn!("Agreements lock poisoned, recovering");
            poisoned.into_inner()
        });
        let agreement = agreements
            .get_mut(agreement_id)
            .ok_or_else(|| FederationError::ClearingAgreementNotFound(agreement_id.to_string()))?;

        // Add signature
        agreement.signatures.push((signer_did, signature));

        // Persist
        let key = Self::agreement_key(agreement_id);
        let value = serde_json::to_vec(agreement)?;
        self.store.put(&key, &value)?;

        info!("Agreement {} accepted", agreement_id);
        Ok(())
    }

    /// Get an agreement by ID
    pub fn get_agreement(&self, agreement_id: &str) -> Result<Option<BilateralClearingAgreement>> {
        Ok(self
            .agreements
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Agreements lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(agreement_id)
            .cloned())
    }

    /// List all agreements
    pub fn list_agreements(&self) -> Vec<BilateralClearingAgreement> {
        self.agreements
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Agreements lock poisoned, recovering");
                poisoned.into_inner()
            })
            .values()
            .cloned()
            .collect()
    }

    /// Propose a cross-cooperative transfer
    pub fn propose_transfer(&self, transfer: CrossCoopTransfer) -> Result<String> {
        let transfer_id = transfer.id.clone();

        // Find the relevant agreement
        let agreement_id = self.find_agreement(&transfer.from_coop, &transfer.to_coop)?;

        // Validate exchange rate exists
        let agreements = self.agreements.read().unwrap_or_else(|poisoned| {
            warn!("Agreements lock poisoned, recovering");
            poisoned.into_inner()
        });
        let agreement = agreements
            .get(&agreement_id)
            .ok_or_else(|| FederationError::ClearingAgreementNotFound(agreement_id.clone()))?;

        if agreement
            .get_rate(&transfer.source_currency, &transfer.dest_currency)
            .is_none()
        {
            return Err(FederationError::ExchangeRateNotFound(
                transfer.source_currency.clone(),
                transfer.dest_currency.clone(),
            ));
        }

        // Capture max_imbalance before dropping the lock
        let max_imbalance = agreement.max_imbalance;
        drop(agreements);

        // Check imbalance limit
        let positions = self.positions.read().unwrap_or_else(|poisoned| {
            warn!("Positions lock poisoned, recovering");
            poisoned.into_inner()
        });
        if let Some(position) = positions.get(&agreement_id) {
            if position.exceeds_limit(max_imbalance) {
                return Err(FederationError::ImbalanceLimitExceeded {
                    max: max_imbalance,
                    current: position.net_position(),
                });
            }
        }
        drop(positions);

        // Persist transfer
        let key = Self::transfer_key(&transfer_id);
        let value = serde_json::to_vec(&transfer)?;
        self.store.put(&key, &value)?;

        // Add to position
        let mut positions = self.positions.write().unwrap_or_else(|poisoned| {
            warn!("Positions lock poisoned, recovering");
            poisoned.into_inner()
        });
        if let Some(position) = positions.get_mut(&agreement_id) {
            position.add_transfer(transfer.clone());

            let pos_key = Self::position_key(&agreement_id);
            let pos_value = serde_json::to_vec(position)?;
            self.store.put(&pos_key, &pos_value)?;
        }

        // Metrics
        metrics::clearing::transfer_amount_record(
            transfer.source_amount,
            &transfer.source_currency,
        );

        debug!("Proposed transfer: {}", transfer_id);
        Ok(transfer_id)
    }

    /// Confirm a transfer, booking its amount into its agreement's position exactly once.
    ///
    /// The durable position record is the authority for whether a transfer has been booked. It
    /// holds the balances *and* each tracked transfer's status, so one `put` of that record is the
    /// whole economic effect: the amount and the "booked" marker cannot be persisted apart. The
    /// transfer row is written afterwards as a derived record.
    ///
    /// The decision is made under the manager's position lock, against the durable records read
    /// under that lock — never against the cache, and never against the transfer row read before
    /// the lock. So a repeated call, a call racing another on this manager, and a call after a
    /// restart all find the transfer already booked and change nothing.
    ///
    /// # Result
    ///
    /// * `Ok(())` when this call books the transfer, and also when it was already booked: a replay
    ///   is an idempotent no-op that leaves every balance unchanged.
    /// * [`FederationError::InvalidState`], with nothing written, when booking would contradict a
    ///   durable record: the transfer or its position entry is `Disputed`, or a `Pending` transfer
    ///   is not tracked by its agreement's position.
    /// * Any other error is a failed read or write. Retrying is safe. If the position write failed,
    ///   nothing was booked and the retry books it once. If only the later transfer-row write
    ///   failed, the amount is already booked and the retry repairs the row without booking again.
    ///
    /// # Ownership
    ///
    /// This serializes the callers of *one* `ClearingManager`. It does not coordinate several
    /// managers over the same store: the other position writers in this type (proposal,
    /// settlement, netting) write the manager's cached position with a plain `put`, so one manager
    /// per clearing keyspace is assumed.
    pub fn confirm_transfer(&self, transfer_id: &str) -> Result<()> {
        let key = Self::transfer_key(transfer_id);

        // Identify the agreement. This read happens before the lock and only names the parties;
        // nothing below trusts the status it saw.
        let observed = self.load_transfer(&key, transfer_id)?;
        let agreement_id = self.find_agreement(&observed.from_coop, &observed.to_coop)?;

        let mut positions = self.positions.write().unwrap_or_else(|poisoned| {
            warn!("Positions lock poisoned, recovering");
            poisoned.into_inner()
        });

        // Decide from durable state, read under the lock.
        let mut transfer = self.load_transfer(&key, transfer_id)?;
        if transfer.from_coop != observed.from_coop || transfer.to_coop != observed.to_coop {
            return Err(FederationError::InvalidState(format!(
                "transfer {transfer_id} changed parties while it was being confirmed"
            )));
        }
        let pos_key = Self::position_key(&agreement_id);
        let mut position: ClearingPosition = match self.store.get(&pos_key)? {
            Some(bytes) => serde_json::from_slice(&bytes)?,
            None => return Err(FederationError::ClearingAgreementNotFound(agreement_id)),
        };

        match decide_confirmation(&transfer, &position) {
            ConfirmDecision::Refuse(reason) => Err(FederationError::InvalidState(format!(
                "refusing to confirm transfer {transfer_id}: {reason}"
            ))),
            ConfirmDecision::AlreadyBooked => {
                if transfer.status == TransferStatus::Pending {
                    // Booked by an earlier call whose row write did not land: repair the row.
                    transfer.confirm();
                    self.store.put(&key, &serde_json::to_vec(&transfer)?)?;
                }
                debug!(
                    "Transfer {} already booked; confirmation is a no-op",
                    transfer_id
                );
                Ok(())
            }
            ConfirmDecision::Book => {
                let owes = if transfer.from_coop == self.own_coop_id {
                    // We sent, so we owe them
                    &mut position.coop_a_owes_b
                } else {
                    // They sent, so they owe us
                    &mut position.coop_b_owes_a
                };
                *owes = owes.checked_add(transfer.dest_amount).ok_or_else(|| {
                    FederationError::InvalidState(format!(
                        "booking transfer {transfer_id} would overflow the position"
                    ))
                })?;
                for entry in position
                    .pending_transfers
                    .iter_mut()
                    .filter(|entry| entry.id == transfer_id)
                {
                    entry.status = TransferStatus::Confirmed;
                }

                // The one write that books the transfer. On failure nothing is booked and the
                // cache is untouched, so the running manager still agrees with the store.
                self.store.put(&pos_key, &serde_json::to_vec(&position)?)?;
                positions.insert(agreement_id, position);

                if transfer.status != TransferStatus::Confirmed {
                    transfer.confirm();
                    self.store.put(&key, &serde_json::to_vec(&transfer)?)?;
                }

                metrics::clearing::transfers_confirmed_inc();
                info!("Confirmed transfer: {}", transfer_id);
                Ok(())
            }
        }
    }

    fn load_transfer(&self, key: &[u8], transfer_id: &str) -> Result<CrossCoopTransfer> {
        let value = self
            .store
            .get(key)?
            .ok_or_else(|| FederationError::TransferNotFound(transfer_id.to_string()))?;
        Ok(serde_json::from_slice(&value)?)
    }

    /// Calculate the current position for an agreement
    pub fn calculate_position(&self, agreement_id: &str) -> Result<ClearingPosition> {
        self.positions
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Positions lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(agreement_id)
            .cloned()
            .ok_or_else(|| FederationError::ClearingAgreementNotFound(agreement_id.to_string()))
    }

    /// Trigger settlement for an agreement
    pub fn trigger_settlement(&self, agreement_id: &str) -> Result<SettlementReport> {
        let mut positions = self.positions.write().unwrap_or_else(|poisoned| {
            warn!("Positions lock poisoned, recovering");
            poisoned.into_inner()
        });
        let position = positions
            .get_mut(agreement_id)
            .ok_or_else(|| FederationError::ClearingAgreementNotFound(agreement_id.to_string()))?;

        let net = position.net_position();

        // Mark all confirmed transfers as settled
        let mut settled_count = 0;
        for transfer in &mut position.pending_transfers {
            if transfer.status == TransferStatus::Confirmed {
                transfer.status = TransferStatus::Settled;
                settled_count += 1;
            }
        }

        // Remove settled transfers
        position
            .pending_transfers
            .retain(|t| t.status != TransferStatus::Settled);

        // Reset position
        let old_a_owes_b = position.coop_a_owes_b;
        let old_b_owes_a = position.coop_b_owes_a;
        position.coop_a_owes_b = 0;
        position.coop_b_owes_a = 0;
        position.last_settlement = current_timestamp();

        // Persist
        let pos_key = Self::position_key(agreement_id);
        let pos_value = serde_json::to_vec(position)?;
        self.store.put(&pos_key, &pos_value)?;

        // Metrics
        metrics::clearing::settlements_completed_inc(agreement_id);
        for _ in 0..settled_count {
            metrics::clearing::transfers_settled_inc();
        }

        info!(
            "Settlement completed for {}: net position = {}",
            agreement_id, net
        );

        Ok(SettlementReport {
            agreement_id: agreement_id.to_string(),
            coop_a_owed: old_a_owes_b,
            coop_b_owed: old_b_owes_a,
            net_settlement: net,
            transfers_settled: settled_count,
            timestamp: current_timestamp(),
        })
    }

    /// Perform multilateral netting across all agreements for a given currency
    /// This computes which cycles could be canceled and returns the results.
    /// NOTE: This is informational only - it does NOT modify positions.
    /// Use apply_multilateral_netting() to actually update positions.
    pub fn perform_multilateral_netting(
        &self,
        currency: &str,
    ) -> Result<crate::netting::NettingResult> {
        use crate::netting::NettingEngine;

        let mut engine = NettingEngine::new(currency.to_string());

        // Build the debt graph from all positions
        let positions = self.positions.read().unwrap_or_else(|poisoned| {
            warn!("Positions lock poisoned, recovering");
            poisoned.into_inner()
        });
        let agreements = self.agreements.read().unwrap_or_else(|poisoned| {
            warn!("Agreements lock poisoned, recovering");
            poisoned.into_inner()
        });

        for (agreement_id, position) in positions.iter() {
            if let Some(agreement) = agreements.get(agreement_id) {
                // Add edges to the graph based on net positions
                let net = position.net_position();
                if net > 0 {
                    // coop_a owes coop_b
                    engine.add_obligation(agreement.coop_a.clone(), agreement.coop_b.clone(), net);
                } else if net < 0 {
                    // coop_b owes coop_a
                    engine.add_obligation(agreement.coop_b.clone(), agreement.coop_a.clone(), -net);
                }
            }
        }

        drop(positions);
        drop(agreements);

        // Perform netting
        let result = engine.net();

        // Log the results
        info!(
            "Multilateral netting for {}: {} cycles canceled, {} total reduced",
            currency,
            result.cycles_canceled.len(),
            result.amount_reduced
        );

        for cycle in &result.cycles_canceled {
            debug!(
                "Canceled cycle {:?} with amount {}",
                cycle.participants, cycle.amount
            );
        }

        Ok(result)
    }

    /// Apply multilateral netting results to positions
    /// This actually modifies the stored positions based on netting results
    pub fn apply_multilateral_netting(&self, result: &crate::netting::NettingResult) -> Result<()> {
        let mut positions = self.positions.write().unwrap_or_else(|poisoned| {
            warn!("Positions lock poisoned, recovering");
            poisoned.into_inner()
        });
        let agreements = self.agreements.read().unwrap_or_else(|poisoned| {
            warn!("Agreements lock poisoned, recovering");
            poisoned.into_inner()
        });

        // First, zero out all positions that were netted
        for obligation in &result.original {
            // Find the agreement between these parties
            for (agreement_id, agreement) in agreements.iter() {
                let is_match = (agreement.coop_a == obligation.from
                    && agreement.coop_b == obligation.to)
                    || (agreement.coop_a == obligation.to && agreement.coop_b == obligation.from);

                if is_match {
                    if let Some(position) = positions.get_mut(agreement_id) {
                        // Reset to zero - we'll rebuild from netted obligations
                        position.coop_a_owes_b = 0;
                        position.coop_b_owes_a = 0;
                    }
                }
            }
        }

        // Now apply the netted obligations
        for obligation in &result.netted {
            for (agreement_id, agreement) in agreements.iter() {
                let forward_match =
                    agreement.coop_a == obligation.from && agreement.coop_b == obligation.to;
                let reverse_match =
                    agreement.coop_a == obligation.to && agreement.coop_b == obligation.from;

                if forward_match {
                    if let Some(position) = positions.get_mut(agreement_id) {
                        position.coop_a_owes_b = obligation.amount;
                        let pos_key = Self::position_key(agreement_id);
                        let pos_value = serde_json::to_vec(position)?;
                        self.store.put(&pos_key, &pos_value)?;
                    }
                } else if reverse_match {
                    if let Some(position) = positions.get_mut(agreement_id) {
                        position.coop_b_owes_a = obligation.amount;
                        let pos_key = Self::position_key(agreement_id);
                        let pos_value = serde_json::to_vec(position)?;
                        self.store.put(&pos_key, &pos_value)?;
                    }
                }
            }
        }

        drop(agreements);
        drop(positions);

        info!(
            "Applied netting: reduced {} obligations to {}",
            result.original.len(),
            result.netted.len()
        );

        Ok(())
    }

    /// Check for agreements that are due for settlement
    /// Returns list of agreement IDs that should be settled
    pub fn get_due_settlements(&self) -> Vec<String> {
        let now = current_timestamp();
        let mut due = Vec::new();

        let agreements = self.agreements.read().unwrap_or_else(|poisoned| {
            warn!("Agreements lock poisoned, recovering");
            poisoned.into_inner()
        });
        let positions = self.positions.read().unwrap_or_else(|poisoned| {
            warn!("Positions lock poisoned, recovering");
            poisoned.into_inner()
        });

        for (agreement_id, agreement) in agreements.iter() {
            // Skip manual settlement agreements
            if let Some(interval_secs) = agreement.settlement_interval.seconds() {
                if let Some(position) = positions.get(agreement_id) {
                    let time_since_last = now.saturating_sub(position.last_settlement);

                    // Check if settlement is due
                    if time_since_last >= interval_secs {
                        due.push(agreement_id.clone());
                    }
                }
            }
        }

        due
    }

    /// Trigger settlements for all agreements that are due
    /// Returns list of settlement reports
    pub fn process_scheduled_settlements(&self) -> Vec<SettlementReport> {
        let due_agreements = self.get_due_settlements();
        let mut reports = Vec::new();

        for agreement_id in due_agreements {
            match self.trigger_settlement(&agreement_id) {
                Ok(report) => {
                    info!("Scheduled settlement completed for {}", agreement_id);
                    reports.push(report);
                }
                Err(e) => {
                    warn!("Scheduled settlement failed for {}: {}", agreement_id, e);
                }
            }
        }

        reports
    }

    /// Terminate a bilateral clearing agreement between two cooperatives.
    ///
    /// Looks up the agreement by the partner cooperative ID (either side), removes
    /// it from the durable store, and drops the associated position tracking.
    /// Returns the terminated agreement ID.
    ///
    /// Fails with `ClearingAgreementNotFound` if no agreement exists between the
    /// own coop and the given partner.
    pub fn terminate_agreement(&self, own_coop: &str, partner_coop: &str) -> Result<String> {
        let agreement_id = self.find_agreement(own_coop, partner_coop)?;

        // Remove agreement
        {
            let mut agreements = self.agreements.write().unwrap_or_else(|poisoned| {
                warn!("Agreements lock poisoned, recovering");
                poisoned.into_inner()
            });
            agreements.remove(&agreement_id);
        }

        // Remove associated position tracking
        {
            let mut positions = self.positions.write().unwrap_or_else(|poisoned| {
                warn!("Positions lock poisoned, recovering");
                poisoned.into_inner()
            });
            positions.remove(&agreement_id);
        }

        info!(
            agreement_id = %agreement_id,
            own_coop = %own_coop,
            partner_coop = %partner_coop,
            "Bilateral clearing agreement terminated via governance"
        );

        Ok(agreement_id)
    }

    /// Find agreement between two cooperatives
    fn find_agreement(&self, coop_a: &str, coop_b: &str) -> Result<String> {
        let agreements = self.agreements.read().unwrap_or_else(|poisoned| {
            warn!("Agreements lock poisoned, recovering");
            poisoned.into_inner()
        });
        for (id, agreement) in agreements.iter() {
            if (agreement.coop_a == coop_a && agreement.coop_b == coop_b)
                || (agreement.coop_a == coop_b && agreement.coop_b == coop_a)
            {
                return Ok(id.clone());
            }
        }
        Err(FederationError::ClearingAgreementNotFound(format!(
            "{coop_a} <-> {coop_b}"
        )))
    }
}

/// What confirming a transfer may do to its agreement's position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmDecision {
    /// Nothing for this transfer is booked yet: book it.
    Book,
    /// The position already records the transfer as booked, or settlement has discharged it.
    AlreadyBooked,
    /// Booking would contradict a durable record. Refuse without writing.
    Refuse(&'static str),
}

/// Decide what confirming `transfer` may do, given its agreement's durable `position`.
///
/// The position is the authority because its balances and its per-transfer statuses change in
/// the same write. The transfer row is consulted only where the position has no entry for the
/// transfer: settlement removes discharged entries, and the row is written `Confirmed` only after
/// the position has booked it, so an untracked `Confirmed` row means booked and discharged.
fn decide_confirmation(
    transfer: &CrossCoopTransfer,
    position: &ClearingPosition,
) -> ConfirmDecision {
    match transfer.status {
        TransferStatus::Disputed => return ConfirmDecision::Refuse("the transfer is disputed"),
        TransferStatus::Settled => return ConfirmDecision::AlreadyBooked,
        TransferStatus::Pending | TransferStatus::Confirmed => {}
    }

    let mut tracked = false;
    let mut booked = false;
    for entry in position
        .pending_transfers
        .iter()
        .filter(|entry| entry.id == transfer.id)
    {
        tracked = true;
        match entry.status {
            TransferStatus::Disputed => {
                return ConfirmDecision::Refuse("the position records the transfer as disputed")
            }
            TransferStatus::Confirmed | TransferStatus::Settled => booked = true,
            TransferStatus::Pending => {}
        }
    }

    if booked {
        ConfirmDecision::AlreadyBooked
    } else if tracked {
        ConfirmDecision::Book
    } else if transfer.status == TransferStatus::Confirmed {
        ConfirmDecision::AlreadyBooked
    } else {
        ConfirmDecision::Refuse("the agreement's position does not track the transfer")
    }
}

/// Report generated after a settlement
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SettlementReport {
    pub agreement_id: String,
    pub coop_a_owed: i64,
    pub coop_b_owed: i64,
    pub net_settlement: i64,
    pub transfers_settled: usize,
    pub timestamp: u64,
}

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::{SledStore, Store};

    fn test_did() -> icn_identity::Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_create_agreement() {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let manager = ClearingManager::new(store, "food-coop".to_string()).unwrap();

        let agreement = BilateralClearingAgreement::new(
            "agreement-1".to_string(),
            "food-coop".to_string(),
            test_did(),
            "tech-coop".to_string(),
            test_did(),
        )
        .with_rate("hours", "USD", 25.0);

        let id = manager.create_agreement(agreement).unwrap();
        assert_eq!(id, "agreement-1");

        let retrieved = manager.get_agreement(&id).unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_calculate_position() {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let manager = ClearingManager::new(store, "food-coop".to_string()).unwrap();

        let agreement = BilateralClearingAgreement::new(
            "agreement-1".to_string(),
            "food-coop".to_string(),
            test_did(),
            "tech-coop".to_string(),
            test_did(),
        )
        .with_rate("hours", "USD", 25.0);

        manager.create_agreement(agreement).unwrap();

        let position = manager.calculate_position("agreement-1").unwrap();
        assert_eq!(position.net_position(), 0);
    }

    #[test]
    fn test_multilateral_netting() {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let manager = ClearingManager::new(store, "food-coop".to_string()).unwrap();

        // Create a cycle: food -> tech -> arts -> food
        let agreement1 = BilateralClearingAgreement::new(
            "food-tech".to_string(),
            "food-coop".to_string(),
            test_did(),
            "tech-coop".to_string(),
            test_did(),
        )
        .with_rate("USD", "USD", 1.0);

        let agreement2 = BilateralClearingAgreement::new(
            "tech-arts".to_string(),
            "tech-coop".to_string(),
            test_did(),
            "arts-coop".to_string(),
            test_did(),
        )
        .with_rate("USD", "USD", 1.0);

        let agreement3 = BilateralClearingAgreement::new(
            "arts-food".to_string(),
            "arts-coop".to_string(),
            test_did(),
            "food-coop".to_string(),
            test_did(),
        )
        .with_rate("USD", "USD", 1.0);

        manager.create_agreement(agreement1).unwrap();
        manager.create_agreement(agreement2).unwrap();
        manager.create_agreement(agreement3).unwrap();

        // Set up positions to create a cycle
        // food owes tech 100, tech owes arts 80, arts owes food 60
        {
            let mut positions = manager.positions.write().unwrap();
            positions.get_mut("food-tech").unwrap().coop_a_owes_b = 100;
            positions.get_mut("tech-arts").unwrap().coop_a_owes_b = 80;
            positions.get_mut("arts-food").unwrap().coop_a_owes_b = 60;
        }

        // Perform multilateral netting
        let result = manager.perform_multilateral_netting("USD").unwrap();

        // Should find and cancel the cycle
        assert_eq!(result.cycles_canceled.len(), 1);
        assert_eq!(result.cycles_canceled[0].amount, 60);

        // After netting: food owes tech 40, tech owes arts 20, arts owes food 0
        assert_eq!(result.netted.len(), 2);
    }

    #[test]
    fn test_scheduled_settlements() {
        use crate::clearing::SettlementInterval;

        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let manager = ClearingManager::new(store, "food-coop".to_string()).unwrap();

        // Create agreement with daily settlement
        let agreement = BilateralClearingAgreement::new(
            "agreement-1".to_string(),
            "food-coop".to_string(),
            test_did(),
            "tech-coop".to_string(),
            test_did(),
        )
        .with_rate("USD", "USD", 1.0)
        .with_interval(SettlementInterval::Daily);

        manager.create_agreement(agreement).unwrap();

        // Modify last_settlement to be 2 days ago
        {
            let mut positions = manager.positions.write().unwrap();
            let position = positions.get_mut("agreement-1").unwrap();
            position.last_settlement = current_timestamp() - (2 * 24 * 60 * 60);
            // 2 days ago
        }

        // Check that settlement is due
        let due = manager.get_due_settlements();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0], "agreement-1");

        // Process scheduled settlements
        let reports = manager.process_scheduled_settlements();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].agreement_id, "agreement-1");

        // After settlement, should not be due anymore
        let due_after = manager.get_due_settlements();
        assert_eq!(due_after.len(), 0);
    }

    fn transfer_with(status: TransferStatus) -> CrossCoopTransfer {
        let mut transfer = CrossCoopTransfer::new(
            "t-1".to_string(),
            "coop-a".to_string(),
            test_did(),
            "coop-b".to_string(),
            test_did(),
            4,
            "CREDIT".to_string(),
            4,
            "CREDIT".to_string(),
        );
        transfer.status = status;
        transfer
    }

    fn position_with(entries: &[TransferStatus]) -> ClearingPosition {
        let mut position = ClearingPosition::new("agr".to_string());
        for status in entries {
            position.add_transfer(transfer_with(*status));
        }
        position
    }

    /// The whole confirmation policy, one row per (transfer row, position entries) case.
    #[test]
    fn decide_confirmation_books_only_an_unbooked_tracked_transfer() {
        use ConfirmDecision::{AlreadyBooked, Book, Refuse};
        use TransferStatus::{Confirmed, Disputed, Pending, Settled};

        let untracked = "the agreement's position does not track the transfer";
        let cases: &[(TransferStatus, &[TransferStatus], ConfirmDecision)] = &[
            // Tracked and not yet booked: the only case that changes a balance.
            (Pending, &[Pending], Book),
            // An earlier booking whose row write was lost (or legacy row-first writes): the
            // position still says Pending, so it is booked now, once.
            (Confirmed, &[Pending], Book),
            // Replays: the position already records the booking.
            (Pending, &[Confirmed], AlreadyBooked),
            (Confirmed, &[Confirmed], AlreadyBooked),
            (Confirmed, &[Settled], AlreadyBooked),
            // A duplicate entry for one id (NB-043) is booked once, never per entry.
            (Pending, &[Pending, Confirmed], AlreadyBooked),
            (Pending, &[Pending, Pending], Book),
            // Discharged by settlement: no entry, row Confirmed.
            (Confirmed, &[], AlreadyBooked),
            (Settled, &[], AlreadyBooked),
            (Settled, &[Pending], AlreadyBooked),
            // Never overwritten, never booked.
            (Disputed, &[Pending], Refuse("the transfer is disputed")),
            (Disputed, &[], Refuse("the transfer is disputed")),
            (
                Pending,
                &[Disputed],
                Refuse("the position records the transfer as disputed"),
            ),
            (
                Confirmed,
                &[Confirmed, Disputed],
                Refuse("the position records the transfer as disputed"),
            ),
            // A Pending row the position never recorded (e.g. a proposal whose position write
            // failed, NB-046): refusing is the only answer that cannot book a phantom.
            (Pending, &[], Refuse(untracked)),
        ];

        for (row, entries, expected) in cases {
            assert_eq!(
                decide_confirmation(&transfer_with(*row), &position_with(entries)),
                *expected,
                "row {row:?}, position entries {entries:?}"
            );
        }
    }

    /// Entries for other transfers never make this one look booked.
    #[test]
    fn decide_confirmation_ignores_other_transfers() {
        let mut position = ClearingPosition::new("agr".to_string());
        let mut other = transfer_with(TransferStatus::Confirmed);
        other.id = "t-other".to_string();
        position.add_transfer(other);
        position.add_transfer(transfer_with(TransferStatus::Pending));

        assert_eq!(
            decide_confirmation(&transfer_with(TransferStatus::Pending), &position),
            ConfirmDecision::Book
        );
    }
}
