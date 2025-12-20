use crate::{CoopError, CoopStatus, Cooperative, Result};
use chrono::Utc;
use icn_identity::Did;
use tracing::{info, warn};

pub struct LifecycleManager {
    // Handles will be added when actor system is wired up
}

#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    Created { coop_id: String, founder: Did },
    Activated { coop_id: String },
    Suspended { coop_id: String, reason: String },
    Resumed { coop_id: String },
    DissolutionStarted { coop_id: String, initiator: Did },
    Dissolved { coop_id: String },
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleManager {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn create_cooperative(
        &self,
        mut coop: Cooperative,
        founder: Did,
    ) -> Result<Cooperative> {
        info!("Creating cooperative: {} by {}", coop.name, founder);

        // Create governance domain (will be implemented when wired to governance)
        let domain_id = format!("coop.{}", coop.id);

        coop.domain_id = Some(domain_id);
        coop.status = CoopStatus::Forming;
        coop.updated_at = Utc::now();

        Ok(coop)
    }

    pub async fn activate(
        &self,
        mut coop: Cooperative,
        charter_hash: String,
    ) -> Result<Cooperative> {
        if !coop.can_transition_to(&CoopStatus::Active) {
            return Err(CoopError::InvalidStateTransition(format!(
                "Cannot activate from {:?}",
                coop.status
            )));
        }

        info!("Activating cooperative: {}", coop.id);

        coop.status = CoopStatus::Active;
        coop.charter_hash = Some(charter_hash);
        coop.updated_at = Utc::now();

        Ok(coop)
    }

    pub async fn suspend(&self, mut coop: Cooperative, reason: String) -> Result<Cooperative> {
        if !coop.can_transition_to(&CoopStatus::Suspended) {
            return Err(CoopError::InvalidStateTransition(format!(
                "Cannot suspend from {:?}",
                coop.status
            )));
        }

        warn!("Suspending cooperative {}: {}", coop.id, reason);

        coop.status = CoopStatus::Suspended;
        coop.metadata
            .insert("suspension_reason".to_string(), reason);
        coop.updated_at = Utc::now();

        Ok(coop)
    }

    pub async fn resume(&self, mut coop: Cooperative) -> Result<Cooperative> {
        if coop.status != CoopStatus::Suspended {
            return Err(CoopError::InvalidStateTransition(format!(
                "Cannot resume from {:?}",
                coop.status
            )));
        }

        info!("Resuming cooperative: {}", coop.id);

        coop.status = CoopStatus::Active;
        coop.metadata.remove("suspension_reason");
        coop.updated_at = Utc::now();

        Ok(coop)
    }

    pub async fn start_dissolution(
        &self,
        mut coop: Cooperative,
        initiator: Did,
    ) -> Result<Cooperative> {
        if !coop.can_transition_to(&CoopStatus::Dissolving) {
            return Err(CoopError::InvalidStateTransition(format!(
                "Cannot dissolve from {:?}",
                coop.status
            )));
        }

        info!(
            "Starting dissolution of cooperative {} by {}",
            coop.id, initiator
        );

        coop.status = CoopStatus::Dissolving;
        coop.metadata
            .insert("dissolution_initiator".to_string(), initiator.to_string());
        coop.updated_at = Utc::now();

        Ok(coop)
    }

    pub async fn complete_dissolution(&self, mut coop: Cooperative) -> Result<Cooperative> {
        if !coop.can_transition_to(&CoopStatus::Dissolved) {
            return Err(CoopError::InvalidStateTransition(format!(
                "Cannot complete dissolution from {:?}",
                coop.status
            )));
        }

        info!("Completing dissolution of cooperative: {}", coop.id);

        coop.status = CoopStatus::Dissolved;
        coop.updated_at = Utc::now();

        Ok(coop)
    }
}
