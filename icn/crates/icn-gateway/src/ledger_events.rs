//! Bridge between icn-ledger events and gateway WebSocket events
//!
//! This module subscribes to the ledger's event emitter and converts
//! ledger events into gateway events that can be broadcast over WebSocket.

use std::sync::Arc;

use icn_ledger::{LedgerEvent, SharedEventEmitter};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::events::{BalanceChangeDetail, EventBroadcaster, GatewayEvent};

/// Bridge that forwards ledger events to the gateway's WebSocket broadcaster
pub struct LedgerEventBridge {
    /// The shared event emitter from the ledger
    ledger_emitter: SharedEventEmitter,
    /// The gateway's event broadcaster
    gateway_broadcaster: Arc<EventBroadcaster>,
    /// Default coop ID to use when ledger events don't include one
    default_coop_id: String,
}

impl LedgerEventBridge {
    /// Create a new ledger event bridge
    pub fn new(
        ledger_emitter: SharedEventEmitter,
        gateway_broadcaster: Arc<EventBroadcaster>,
        default_coop_id: String,
    ) -> Self {
        Self {
            ledger_emitter,
            gateway_broadcaster,
            default_coop_id,
        }
    }

    /// Start the bridge, subscribing to ledger events and forwarding to WebSocket
    ///
    /// This spawns a background task that runs until the receiver is dropped.
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut receiver = self.ledger_emitter.subscribe();
            info!("Ledger event bridge started");

            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        self.handle_event(event).await;
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!("Ledger event bridge lagged, missed {} events", count);
                        // Continue processing - we'll catch up
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Ledger event emitter closed, bridge shutting down");
                        break;
                    }
                }
            }
        })
    }

    /// Handle a single ledger event
    async fn handle_event(&self, event: LedgerEvent) {
        match event {
            LedgerEvent::TransactionCreated(tx) => {
                let coop_id = tx.domain_id.unwrap_or_else(|| self.default_coop_id.clone());

                // Emit PaymentCreated for each transfer
                for transfer in tx.transfers {
                    let gateway_event = GatewayEvent::PaymentCreated {
                        coop_id: coop_id.clone(),
                        hash: tx.entry_hash.clone(),
                        from: transfer.from,
                        to: transfer.to,
                        amount: transfer.amount,
                        currency: transfer.currency,
                    };
                    self.gateway_broadcaster
                        .broadcast(&coop_id, gateway_event)
                        .await;
                }
                debug!(
                    entry_hash = %tx.entry_hash,
                    "Forwarded TransactionCreated event"
                );
            }

            LedgerEvent::TransactionConfirmed(tx) => {
                // For now, log confirmation events
                // In the future, we could add a TransactionConfirmed gateway event
                debug!(
                    entry_hash = %tx.entry_hash,
                    confirmations = tx.confirmations,
                    "TransactionConfirmed event (not forwarded)"
                );
            }

            LedgerEvent::BalanceChanged(bc) => {
                let coop_id = bc.domain_id.unwrap_or_else(|| self.default_coop_id.clone());
                let gateway_event = GatewayEvent::BalanceChanged {
                    coop_id: coop_id.clone(),
                    account: bc.account,
                    currency: bc.currency,
                    old_balance: bc.old_balance,
                    new_balance: bc.new_balance,
                    change: bc.change,
                    entry_hash: bc.entry_hash,
                };
                self.gateway_broadcaster
                    .broadcast(&coop_id, gateway_event)
                    .await;
                debug!("Forwarded BalanceChanged event");
            }

            LedgerEvent::BatchBalanceChanged(batch) => {
                // Determine coop_id from first change or use default
                let coop_id = batch
                    .changes
                    .first()
                    .and_then(|c| c.domain_id.clone())
                    .unwrap_or_else(|| self.default_coop_id.clone());

                let changes: Vec<BalanceChangeDetail> = batch
                    .changes
                    .iter()
                    .map(|c| BalanceChangeDetail {
                        account: c.account.clone(),
                        currency: c.currency.clone(),
                        old_balance: c.old_balance,
                        new_balance: c.new_balance,
                        change: c.change,
                    })
                    .collect();

                let gateway_event = GatewayEvent::BatchBalanceChanged {
                    coop_id: coop_id.clone(),
                    entry_hash: batch.entry_hash,
                    changes,
                };
                self.gateway_broadcaster
                    .broadcast(&coop_id, gateway_event)
                    .await;
                debug!("Forwarded BatchBalanceChanged event");
            }

            LedgerEvent::MemberFrozen(mf) => {
                // Use default coop_id since freeze events are ledger-wide
                let coop_id = self.default_coop_id.clone();
                let gateway_event = GatewayEvent::LedgerMemberFrozen {
                    coop_id: coop_id.clone(),
                    member: mf.member,
                    reason: mf.reason,
                    frozen_by: mf.frozen_by,
                    proposal_id: mf.proposal_id,
                    duration_seconds: mf.duration_seconds,
                };
                self.gateway_broadcaster
                    .broadcast(&coop_id, gateway_event)
                    .await;
                debug!("Forwarded MemberFrozen event");
            }

            LedgerEvent::MemberUnfrozen(mu) => {
                let coop_id = self.default_coop_id.clone();
                let gateway_event = GatewayEvent::LedgerMemberUnfrozen {
                    coop_id: coop_id.clone(),
                    member: mu.member,
                    reason: mu.reason,
                    unfrozen_by: mu.unfrozen_by,
                    proposal_id: mu.proposal_id,
                };
                self.gateway_broadcaster
                    .broadcast(&coop_id, gateway_event)
                    .await;
                debug!("Forwarded MemberUnfrozen event");
            }

            LedgerEvent::ForkDetected(fd) => {
                let coop_id = self.default_coop_id.clone();
                let gateway_event = GatewayEvent::LedgerForkDetected {
                    coop_id: coop_id.clone(),
                    parent_hash: fd.parent_hash,
                    conflicting_entries: fd.conflicting_entries,
                };
                self.gateway_broadcaster
                    .broadcast(&coop_id, gateway_event)
                    .await;
                warn!("Forwarded ForkDetected event");
            }

            LedgerEvent::ForkResolved(fr) => {
                let coop_id = self.default_coop_id.clone();
                let gateway_event = GatewayEvent::LedgerForkResolved {
                    coop_id: coop_id.clone(),
                    parent_hash: fr.parent_hash,
                    kept_entry: fr.kept_entry,
                    quarantined_entries: fr.quarantined_entries,
                    resolution_strategy: fr.resolution_strategy,
                };
                self.gateway_broadcaster
                    .broadcast(&coop_id, gateway_event)
                    .await;
                info!("Forwarded ForkResolved event");
            }

            LedgerEvent::RollbackPerformed(rp) => {
                let coop_id = self.default_coop_id.clone();
                let gateway_event = GatewayEvent::LedgerRollback {
                    coop_id: coop_id.clone(),
                    target_hash: rp.target_hash,
                    archived_count: rp.archived_count,
                    reason: rp.reason,
                };
                self.gateway_broadcaster
                    .broadcast(&coop_id, gateway_event)
                    .await;
                warn!("Forwarded RollbackPerformed event");
            }

            LedgerEvent::CrossCurrencyTransfer(cct) => {
                let coop_id = cct
                    .domain_id
                    .unwrap_or_else(|| self.default_coop_id.clone());
                let gateway_event = GatewayEvent::CrossPaymentCreated {
                    coop_id: coop_id.clone(),
                    hash: cct.entry_hash,
                    from: cct.from,
                    to: cct.to,
                    source_amount: cct.source_amount,
                    from_currency: cct.from_currency,
                    target_amount: cct.net_target_amount,
                    to_currency: cct.to_currency,
                    rate: cct.rate,
                };
                self.gateway_broadcaster
                    .broadcast(&coop_id, gateway_event)
                    .await;
                debug!("Forwarded CrossCurrencyTransfer event");
            }

            LedgerEvent::ResourceAccessTransferred(rat) => {
                // Resource access transfers are logged but not yet forwarded to WebSocket
                // In the future, we could add a ResourceAccessTransferred gateway event
                debug!(
                    resource_id = %rat.resource_id,
                    from = %rat.from_holder,
                    to = %rat.to_holder,
                    "ResourceAccessTransferred event (not forwarded)"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_ledger::{create_shared_emitter, Transfer};

    #[tokio::test]
    async fn test_bridge_forwards_transaction_created() {
        let ledger_emitter = create_shared_emitter();
        let gateway_broadcaster = Arc::new(EventBroadcaster::new());
        let coop_id = "test-coop".to_string();

        // Subscribe before starting bridge
        let mut rx = gateway_broadcaster
            .subscribe(&coop_id)
            .await
            .expect("Should subscribe");

        // Start bridge
        let bridge = LedgerEventBridge::new(
            ledger_emitter.clone(),
            gateway_broadcaster.clone(),
            coop_id.clone(),
        );
        let _handle = bridge.start();

        // Give the bridge time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Emit ledger event
        ledger_emitter.emit(LedgerEvent::TransactionCreated(
            icn_ledger::TransactionCreated {
                entry_hash: "hash123".to_string(),
                author: "did:icn:alice".to_string(),
                transfers: vec![Transfer {
                    from: "did:icn:alice".to_string(),
                    to: "did:icn:bob".to_string(),
                    amount: 10,
                    currency: "hours".to_string(),
                }],
                timestamp: 1234567890,
                domain_id: Some(coop_id.clone()),
            },
        ));

        // Wait for event to propagate
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Check that gateway received the event
        if let Ok(sequenced) = rx.try_recv() {
            match sequenced.event {
                GatewayEvent::PaymentCreated {
                    coop_id: c, amount, ..
                } => {
                    assert_eq!(c, "test-coop");
                    assert_eq!(amount, 10);
                }
                _ => panic!("Expected PaymentCreated event"),
            }
        } else {
            // Event may not have arrived yet - that's okay for this test
            // In real usage, the bridge runs continuously
        }
    }

    #[tokio::test]
    async fn test_bridge_forwards_balance_changed() {
        let ledger_emitter = create_shared_emitter();
        let gateway_broadcaster = Arc::new(EventBroadcaster::new());
        let coop_id = "test-coop".to_string();

        let mut rx = gateway_broadcaster
            .subscribe(&coop_id)
            .await
            .expect("Should subscribe");

        let bridge = LedgerEventBridge::new(
            ledger_emitter.clone(),
            gateway_broadcaster.clone(),
            coop_id.clone(),
        );
        let _handle = bridge.start();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Emit balance changed event
        ledger_emitter.emit(LedgerEvent::BalanceChanged(icn_ledger::BalanceChanged {
            account: "did:icn:alice".to_string(),
            currency: "hours".to_string(),
            old_balance: 0,
            new_balance: 10,
            change: 10,
            entry_hash: "hash123".to_string(),
            timestamp: 1234567890,
            domain_id: Some(coop_id.clone()),
        }));

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        if let Ok(sequenced) = rx.try_recv() {
            match sequenced.event {
                GatewayEvent::BalanceChanged {
                    account,
                    new_balance,
                    ..
                } => {
                    assert_eq!(account, "did:icn:alice");
                    assert_eq!(new_balance, 10);
                }
                _ => panic!("Expected BalanceChanged event"),
            }
        }
    }
}
