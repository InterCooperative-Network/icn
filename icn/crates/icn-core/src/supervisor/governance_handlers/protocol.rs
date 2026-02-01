//! Protocol change governance proposal handlers
//!
//! Extracted from governance_handlers/mod.rs to reduce file size.
//! These handlers manage protocol upgrades and parameter changes,
//! including immediate and delayed (scheduled) execution.

use tracing::{info, warn};

use icn_governance::ProposalId;

impl super::GovernanceEventHandler {
    /// Emit a protocol parameter changed event for audit logging
    fn emit_parameter_changed(
        &self,
        parameter_id: &str,
        old_value: &str,
        new_value: &str,
        proposal_id: Option<String>,
        changed_by: Option<String>,
    ) {
        if let Some(ref bus) = self.event_bus {
            let bus = bus.clone();
            let event = crate::events::SystemEvent::ProtocolParameterChanged {
                parameter_id: parameter_id.to_string(),
                old_value: old_value.to_string(),
                new_value: new_value.to_string(),
                proposal_id,
                changed_by,
                changed_at: icn_time::current_timestamp_secs(),
            };
            // Spawn async emit in the background
            // Note: EventBus::emit() is infallible (broadcasts to all subscribers)
            tokio::spawn(async move {
                bus.emit(event).await;
            });
        }
    }

    pub(super) fn handle_protocol_upgrade(
        &self,
        proposal_id: ProposalId,
        version: String,
        breaking_changes: Vec<String>,
        migration_guide: Option<String>,
        deadline: u64,
        min_required_version: Option<String>,
    ) {
        info!(
            "🔄 Protocol upgrade proposal {} accepted: -> {}",
            proposal_id.0, version
        );

        info!("   Target version: {}", version);
        info!("   Deadline: {}", deadline);
        if !breaking_changes.is_empty() {
            info!("   Breaking changes: {} items", breaking_changes.len());
        }
        if let Some(guide) = migration_guide {
            info!("   Migration guide: {}", guide);
        }
        if let Some(min_ver) = min_required_version {
            info!("   Minimum required version: {}", min_ver);
        }

        icn_obs::metrics::governance::proposals_executed_inc("protocol_upgrade");
    }

    /// Handle a protocol parameter change proposal (Phase 20)
    pub(super) fn handle_protocol_change(
        &self,
        proposal_id: ProposalId,
        proposal: icn_governance::ProtocolChangeProposal,
    ) {
        let proposal_id_str = proposal_id.0.clone();

        // Check if this is a delayed execution
        if let Some(effective_at) = proposal.effective_at {
            self.handle_delayed_protocol_change(proposal_id, proposal, effective_at);
            return;
        }

        info!(
            "⚙️  Protocol change proposal {} accepted: {} -> {:?}",
            proposal_id.0, proposal.parameter_id, proposal.new_value
        );

        // Get the protocol parameter store through the governance handle
        let param_result = self
            .gov_handle
            .get_protocol_parameter(&proposal.parameter_id);
        match param_result {
            Ok(Some(mut param)) => {
                // Capture old value for audit event (serialize to string for logging)
                let old_value_str = format!("{:?}", param.value);

                // Validate the new value against parameter constraints
                if let Err(e) = param.validate(&proposal.new_value) {
                    let error_msg = format!(
                        "Validation failed for parameter '{}': {}",
                        proposal.parameter_id, e
                    );
                    warn!("{} (proposal {})", error_msg, proposal_id_str);
                    self.emit_execution_failure(&proposal_id, "protocol_change", &error_msg);
                    return;
                }

                // Update the parameter with the new value
                param.value = proposal.new_value.clone();
                param.updated_at = icn_time::current_timestamp_secs();
                param.updated_by = Some(proposal_id_str.clone());

                // Update the scope if specified in the proposal (with validation)
                if let Some(scope) = proposal.scope {
                    // Defense-in-depth: verify scope override is allowed
                    // (should have been validated at proposal creation)
                    if !param.constraints.allow_override
                        && !matches!(scope, icn_governance::ParameterScope::Global)
                    {
                        let error_msg = format!(
                            "Parameter '{}' does not allow scope overrides",
                            proposal.parameter_id
                        );
                        warn!("{} (proposal {})", error_msg, proposal_id_str);
                        self.emit_execution_failure(&proposal_id, "protocol_change", &error_msg);
                        return;
                    }

                    // Re-validate entity existence at execution time (CRITICAL #3)
                    // Entity may have been deleted between proposal creation and execution.
                    // This prevents orphaned scoped parameters.
                    //
                    // Note: A narrow TOCTOU window exists between this check and parameter
                    // persistence below. In practice, entity deletion is governed and
                    // rate-limited, making this race extremely rare. Orphaned scoped
                    // parameters can be cleaned up via periodic parameter audit if needed.
                    if let Some(entity_id_str) = scope.entity_id_str() {
                        match self.gov_handle.entity_exists(entity_id_str) {
                            Ok(true) => {
                                // Entity exists, proceed with scope change
                            }
                            Ok(false) => {
                                let error_msg = format!(
                                    "Entity '{entity_id_str}' no longer exists. Cannot create scoped parameter."
                                );
                                warn!("{} (proposal {})", error_msg, proposal_id_str);
                                self.emit_execution_failure(
                                    &proposal_id,
                                    "protocol_change",
                                    &error_msg,
                                );
                                return;
                            }
                            Err(e) => {
                                let error_msg = format!(
                                    "Failed to verify entity '{entity_id_str}' existence: {e}"
                                );
                                warn!("{} (proposal {})", error_msg, proposal_id_str);
                                self.emit_execution_failure(
                                    &proposal_id,
                                    "protocol_change",
                                    &error_msg,
                                );
                                return;
                            }
                        }
                    }

                    param.scope = scope;
                }

                // Serialize new value for audit event
                let new_value_str = format!("{:?}", proposal.new_value);

                // Persist the updated parameter
                if let Err(e) = self.gov_handle.set_protocol_parameter(
                    param,
                    Some(proposal_id_str.clone()),
                    None,
                ) {
                    let error_msg = format!(
                        "Failed to persist parameter '{}': {}",
                        proposal.parameter_id, e
                    );
                    warn!("{} (proposal {})", error_msg, proposal_id_str);
                    self.emit_execution_failure(&proposal_id, "protocol_change", &error_msg);
                } else {
                    info!(
                        "✓ Protocol parameter {} updated to {:?}",
                        proposal.parameter_id, proposal.new_value
                    );

                    // Emit audit event for parameter change
                    self.emit_parameter_changed(
                        &proposal.parameter_id,
                        &old_value_str,
                        &new_value_str,
                        Some(proposal_id_str.clone()),
                        None, // changed_by is the proposal, not a specific user
                    );
                }
            }
            Ok(None) => {
                let error_msg = format!(
                    "Parameter '{}' not found, cannot apply change",
                    proposal.parameter_id
                );
                warn!("{} (proposal {})", error_msg, proposal_id_str);
                self.emit_execution_failure(&proposal_id, "protocol_change", &error_msg);
            }
            Err(e) => {
                let error_msg =
                    format!("Failed to get parameter '{}': {}", proposal.parameter_id, e);
                warn!("{} (proposal {})", error_msg, proposal_id_str);
                self.emit_execution_failure(&proposal_id, "protocol_change", &error_msg);
            }
        }

        icn_obs::metrics::governance::proposals_executed_inc("protocol_change");
    }

    /// Handle a protocol parameter change with delayed execution
    fn handle_delayed_protocol_change(
        &self,
        proposal_id: ProposalId,
        proposal: icn_governance::ProtocolChangeProposal,
        effective_at: u64,
    ) {
        let proposal_id_str = proposal_id.0.clone();

        // Calculate delay for logging
        let now = icn_time::current_timestamp_secs();
        let delay_secs = effective_at.saturating_sub(now);
        let delay_human = if delay_secs < 3600 {
            format!("{} minutes", delay_secs / 60)
        } else if delay_secs < 86400 {
            format!("{} hours", delay_secs / 3600)
        } else {
            format!("{} days", delay_secs / 86400)
        };

        info!(
            "⏰ Protocol change proposal {} scheduled for delayed execution: {} -> {:?} (effective in {})",
            proposal_id.0, proposal.parameter_id, proposal.new_value, delay_human
        );

        // Determine the scope for the pending change
        let scope = proposal
            .scope
            .clone()
            .unwrap_or(icn_governance::ParameterScope::Global);

        // Create the pending change
        let pending_change = icn_governance::PendingParameterChange::new(
            icn_governance::PendingParameterChange::generate_id(&proposal.parameter_id),
            &proposal.parameter_id,
            proposal.new_value.clone(),
            effective_at,
            scope,
            &proposal_id_str,
            &proposal.rationale,
        );

        // Store the pending change
        if let Err(e) = self.gov_handle.schedule_pending_change(pending_change) {
            let error_msg = format!(
                "Failed to schedule delayed parameter change for '{}': {}",
                proposal.parameter_id, e
            );
            warn!("{} (proposal {})", error_msg, proposal_id_str);
            self.emit_execution_failure(&proposal_id, "protocol_change", &error_msg);
            return;
        }

        info!(
            "✓ Protocol parameter change {} scheduled (effective_at: {})",
            proposal.parameter_id, effective_at
        );

        icn_obs::metrics::governance::proposals_executed_inc("protocol_change_scheduled");
    }
}
