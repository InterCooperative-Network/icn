//! Protocol change governance proposal handlers.
//!
//! Handles protocol-related proposals including:
//! - Protocol upgrades
//! - Parameter changes (immediate and delayed)
//! - Protocol versioning
//!
//! # Design
//!
//! The `ProtocolHandler` implements the [`ExecutionCallback`] trait, receiving
//! protocol change proposals and applying them to the protocol parameter store.
//!
//! # Kernel Trait Integration
//!
//! When a [`ProtocolExecutor`] is configured via [`ProtocolHandler::with_executor`],
//! the handler delegates execution to the kernel-provided executor. This enables
//! clean separation between governance domain logic and kernel execution services.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::execution::{
    DecisionReceiptId, ExecutionCallback, ExecutionOutcome, ProposalExecutionContext, ProposalType,
    ProtocolChange, ProtocolExecutor,
};

/// Handler for protocol change governance proposals.
///
/// This handler processes protocol-related proposals such as parameter changes
/// and protocol upgrades. It validates the changes against parameter constraints
/// before applying them.
///
/// # Kernel Executor Integration
///
/// When configured with a [`ProtocolExecutor`] via [`with_executor`], the handler
/// delegates execution to the kernel-provided executor. Without an executor,
/// the handler returns an error indicating execution must be handled externally.
pub struct ProtocolHandler {
    /// Handler name for logging
    name: String,
    /// Optional kernel protocol executor
    executor: Option<Arc<dyn ProtocolExecutor>>,
}

impl ProtocolHandler {
    /// Create a new protocol handler without an executor.
    ///
    /// Without an executor configured, execution will fail with an error
    /// indicating the caller must use the executor callback pattern.
    pub fn new() -> Self {
        Self {
            name: "ProtocolHandler".to_string(),
            executor: None,
        }
    }

    /// Configure a protocol executor for delegated execution.
    ///
    /// When an executor is set, the handler delegates all protocol changes
    /// to the kernel-provided executor.
    pub fn with_executor(mut self, executor: Arc<dyn ProtocolExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }
}

impl Default for ProtocolHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert proposal payload to kernel ProtocolChange.
fn to_protocol_change(ctx: &ProposalExecutionContext) -> Result<ProtocolChange> {
    let parameter_name = ctx
        .payload
        .get("parameter_id")
        .or_else(|| ctx.payload.get("parameter_name"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing parameter_id/parameter_name in protocol change payload"))?
        .to_string();

    let new_value = ctx
        .payload
        .get("new_value")
        .map(|v| v.to_string())
        .unwrap_or_default();

    let old_value = ctx
        .payload
        .get("old_value")
        .map(|v| v.to_string())
        .unwrap_or_default();

    let effective_at = ctx
        .payload
        .get("effective_at")
        .and_then(|v| v.as_u64())
        .unwrap_or(ctx.decided_at);

    Ok(ProtocolChange {
        parameter_name,
        old_value,
        new_value,
        effective_at,
    })
}

#[async_trait]
impl ExecutionCallback for ProtocolHandler {
    async fn execute(&self, ctx: &ProposalExecutionContext) -> Result<()> {
        info!(
            proposal_id = %ctx.proposal_id,
            domain_id = %ctx.domain_id,
            "Executing protocol change proposal"
        );

        // Parse the protocol change type from the payload
        let change_type = ctx
            .payload
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        debug!(
            proposal_id = %ctx.proposal_id,
            change_type = %change_type,
            "Protocol change type"
        );

        // If we have an executor, delegate to it
        if let Some(executor) = &self.executor {
            let receipt_id = DecisionReceiptId::new(&ctx.proposal_id);
            let change = to_protocol_change(ctx)?;

            match executor.apply_protocol_change(&receipt_id, change).await? {
                ExecutionOutcome::Success { effects, .. } => {
                    info!(
                        proposal_id = %ctx.proposal_id,
                        effects = ?effects,
                        "Protocol change succeeded"
                    );
                    Ok(())
                }
                ExecutionOutcome::Failed { reason, .. } => {
                    Err(anyhow!("Protocol change failed: {reason}"))
                }
                ExecutionOutcome::Deferred { reason, .. } => {
                    warn!(
                        proposal_id = %ctx.proposal_id,
                        reason = %reason,
                        "Protocol change deferred"
                    );
                    Ok(())
                }
            }
        } else {
            // No executor configured - indicate execution must be delegated
            warn!(
                proposal_id = %ctx.proposal_id,
                "Protocol handler has no executor - execution delegated to icn-core"
            );

            Err(anyhow!(
                "Protocol handler not yet implemented - use executor callback"
            ))
        }
    }

    fn handles(&self, proposal_type: &ProposalType) -> bool {
        matches!(proposal_type, ProposalType::Protocol)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Protocol change types.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Variants used for routing logic in future migration
pub enum ProtocolChangeType {
    /// Upgrade protocol version
    Upgrade,
    /// Change a protocol parameter (immediate)
    ParameterChange,
    /// Change a protocol parameter (delayed execution)
    DelayedParameterChange,
}

impl ProtocolChangeType {
    /// Parse change type from JSON payload.
    #[allow(dead_code)] // Used in tests and future handler migration
    pub fn from_payload(payload: &serde_json::Value) -> Option<Self> {
        // Try explicit type field first
        if let Some(type_str) = payload.get("type").and_then(|v| v.as_str()) {
            match type_str {
                "Upgrade" | "upgrade" => return Some(Self::Upgrade),
                "ParameterChange" | "parameter_change" => return Some(Self::ParameterChange),
                "DelayedParameterChange" | "delayed_parameter_change" => {
                    return Some(Self::DelayedParameterChange)
                }
                _ => {}
            }
        }

        // Infer from other fields
        if payload.get("effective_at").is_some() {
            Some(Self::DelayedParameterChange)
        } else if payload.get("parameter_id").is_some() {
            Some(Self::ParameterChange)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_handler_handles() {
        let handler = ProtocolHandler::new();
        assert!(handler.handles(&ProposalType::Protocol));
        assert!(!handler.handles(&ProposalType::Treasury));
        assert!(!handler.handles(&ProposalType::Federation));
    }

    #[test]
    fn test_protocol_handler_name() {
        let handler = ProtocolHandler::new();
        assert_eq!(handler.name(), "ProtocolHandler");
    }

    #[test]
    fn test_protocol_change_type_from_payload() {
        let payload = serde_json::json!({"type": "Upgrade"});
        assert_eq!(
            ProtocolChangeType::from_payload(&payload),
            Some(ProtocolChangeType::Upgrade)
        );

        let payload = serde_json::json!({"type": "ParameterChange", "parameter_id": "foo"});
        assert_eq!(
            ProtocolChangeType::from_payload(&payload),
            Some(ProtocolChangeType::ParameterChange)
        );

        // Infer from effective_at field
        let payload = serde_json::json!({"parameter_id": "foo", "effective_at": 12345});
        assert_eq!(
            ProtocolChangeType::from_payload(&payload),
            Some(ProtocolChangeType::DelayedParameterChange)
        );

        // Infer from parameter_id field
        let payload = serde_json::json!({"parameter_id": "foo"});
        assert_eq!(
            ProtocolChangeType::from_payload(&payload),
            Some(ProtocolChangeType::ParameterChange)
        );
    }
}
