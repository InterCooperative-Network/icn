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

    /// Confirm a transfer
    pub fn confirm_transfer(&self, transfer_id: &str) -> Result<()> {
        let key = Self::transfer_key(transfer_id);
        let value = self
            .store
            .get(&key)?
            .ok_or_else(|| FederationError::TransferNotFound(transfer_id.to_string()))?;

        let mut transfer: CrossCoopTransfer = serde_json::from_slice(&value)?;
        transfer.confirm();

        let updated_value = serde_json::to_vec(&transfer)?;
        self.store.put(&key, &updated_value)?;

        // Update position
        let agreement_id = self.find_agreement(&transfer.from_coop, &transfer.to_coop)?;
        let mut positions = self.positions.write().unwrap_or_else(|poisoned| {
            warn!("Positions lock poisoned, recovering");
            poisoned.into_inner()
        });
        if let Some(position) = positions.get_mut(&agreement_id) {
            // Update the owes amount
            if transfer.from_coop == self.own_coop_id {
                // We sent, so we owe them
                position.coop_a_owes_b += transfer.dest_amount;
            } else {
                // They sent, so they owe us
                position.coop_b_owes_a += transfer.dest_amount;
            }

            // Update pending transfers
            for t in &mut position.pending_transfers {
                if t.id == transfer_id {
                    t.status = TransferStatus::Confirmed;
                }
            }

            let pos_key = Self::position_key(&agreement_id);
            let pos_value = serde_json::to_vec(position)?;
            self.store.put(&pos_key, &pos_value)?;
        }

        metrics::clearing::transfers_confirmed_inc();
        info!("Confirmed transfer: {}", transfer_id);
        Ok(())
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
    /// This finds cycles in the debt graph and cancels them
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
                    engine.add_obligation(
                        agreement.coop_a.clone(),
                        agreement.coop_b.clone(),
                        net,
                    );
                } else if net < 0 {
                    // coop_b owes coop_a
                    engine.add_obligation(
                        agreement.coop_b.clone(),
                        agreement.coop_a.clone(),
                        -net,
                    );
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

/// Report generated after a settlement
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}
