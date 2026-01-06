use crate::{AssetDistributionPlan, CoopError, CoopStatus, Cooperative, FormationRequest, Result};
use chrono::Utc;
use icn_governance::charter::FounderSignature;
use icn_identity::Did;
use tracing::{debug, info, warn};

pub struct LifecycleManager {
    // Handles will be added when actor system is wired up
}

#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    /// Cooperative created (in Forming state)
    Created { coop_id: String, founder: Did },
    /// Founder signed the charter
    CharterSigned {
        coop_id: String,
        signer: Did,
        signatures_collected: usize,
        signatures_required: usize,
    },
    /// Charter ratified (all founders signed)
    CharterRatified { coop_id: String },
    /// Cooperative activated (charter ratified + min members met)
    Activated { coop_id: String },
    /// Cooperative suspended
    Suspended { coop_id: String, reason: String },
    /// Cooperative resumed from suspension
    Resumed { coop_id: String },
    /// Dissolution process started
    DissolutionStarted {
        coop_id: String,
        initiator: Did,
        proposal_id: Option<String>,
    },
    /// Asset distribution completed during dissolution
    AssetsDistributed {
        coop_id: String,
        plan: AssetDistributionPlan,
    },
    /// Cooperative fully dissolved
    Dissolved {
        coop_id: String,
        proposal_id: Option<String>,
    },
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

    /// Add a founder signature to the charter
    ///
    /// Returns the updated cooperative and a LifecycleEvent indicating progress.
    /// If this signature completes the required number, also returns CharterRatified event.
    pub async fn sign_charter(
        &self,
        mut coop: Cooperative,
        signature: FounderSignature,
    ) -> Result<(Cooperative, Vec<LifecycleEvent>)> {
        if coop.status != CoopStatus::Forming {
            return Err(CoopError::InvalidStateTransition(format!(
                "Cannot sign charter for cooperative in {:?} state",
                coop.status
            )));
        }

        let signer = signature.did.clone();
        let was_ratified = coop.charter_ratified;

        if !coop.add_founder_signature(&signature) {
            return Err(CoopError::DuplicateMember(format!(
                "Founder {signer} has already signed the charter"
            )));
        }

        let mut events = vec![LifecycleEvent::CharterSigned {
            coop_id: coop.id.clone(),
            signer: signer.clone(),
            signatures_collected: coop.founding_signatures.len(),
            signatures_required: coop.min_founders,
        }];

        debug!(
            "Charter signature added for {}: {}/{} signatures",
            coop.id,
            coop.founding_signatures.len(),
            coop.min_founders
        );

        // Check if this signature completed the ratification
        if !was_ratified && coop.charter_ratified {
            info!("Charter ratified for cooperative: {}", coop.id);
            events.push(LifecycleEvent::CharterRatified {
                coop_id: coop.id.clone(),
            });
        }

        Ok((coop, events))
    }

    /// Create a cooperative from a formation request
    ///
    /// This creates a cooperative in the Forming state. The cooperative
    /// will not be activated until:
    /// 1. All required founders have signed the charter
    /// 2. The minimum member count is met
    pub async fn create_from_request(
        &self,
        request: FormationRequest,
        id: String,
        first_founder: Did,
    ) -> Result<(Cooperative, LifecycleEvent)> {
        info!(
            "Creating cooperative '{}' with {} founding members",
            request.name,
            request.founding_members.len()
        );

        let coop = Cooperative::from_formation_request(&request, id);

        let event = LifecycleEvent::Created {
            coop_id: coop.id.clone(),
            founder: first_founder,
        };

        Ok((coop, event))
    }

    /// Start dissolution with an asset distribution plan
    ///
    /// This transitions the cooperative to Dissolving state and records
    /// the asset distribution plan and governance proposal (if any).
    pub async fn start_dissolution_with_plan(
        &self,
        mut coop: Cooperative,
        initiator: Did,
        plan: AssetDistributionPlan,
        proposal_id: Option<String>,
    ) -> Result<(Cooperative, LifecycleEvent)> {
        if !coop.can_transition_to(&CoopStatus::Dissolving) {
            return Err(CoopError::InvalidStateTransition(format!(
                "Cannot dissolve cooperative in {:?} state",
                coop.status
            )));
        }

        info!(
            "Starting dissolution of cooperative {} by {} (proposal: {:?})",
            coop.id, initiator, proposal_id
        );

        coop.status = CoopStatus::Dissolving;
        coop.set_dissolution_plan(plan, proposal_id.clone());
        coop.metadata
            .insert("dissolution_initiator".to_string(), initiator.to_string());

        let event = LifecycleEvent::DissolutionStarted {
            coop_id: coop.id.clone(),
            initiator,
            proposal_id,
        };

        Ok((coop, event))
    }

    /// Complete dissolution after assets have been distributed
    ///
    /// This should be called after the asset distribution plan has been executed.
    pub async fn complete_dissolution_with_plan(
        &self,
        mut coop: Cooperative,
    ) -> Result<(Cooperative, Vec<LifecycleEvent>)> {
        if !coop.can_transition_to(&CoopStatus::Dissolved) {
            return Err(CoopError::InvalidStateTransition(format!(
                "Cannot complete dissolution from {:?}",
                coop.status
            )));
        }

        let mut events = Vec::new();

        // Record asset distribution event if there was a plan
        if let Some(plan) = &coop.dissolution_plan {
            events.push(LifecycleEvent::AssetsDistributed {
                coop_id: coop.id.clone(),
                plan: plan.clone(),
            });
        }

        info!("Completing dissolution of cooperative: {}", coop.id);

        let proposal_id = coop.dissolution_proposal_id.clone();
        coop.status = CoopStatus::Dissolved;
        coop.updated_at = Utc::now();

        events.push(LifecycleEvent::Dissolved {
            coop_id: coop.id.clone(),
            proposal_id,
        });

        Ok((coop, events))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CoopType, FormationRequest};
    use icn_governance::charter::FounderSignature;

    fn create_test_did() -> Did {
        icn_identity::KeyPair::generate().unwrap().did().clone()
    }

    fn create_test_formation_request(founders: Vec<Did>) -> FormationRequest {
        FormationRequest {
            name: "Test Coop".to_string(),
            coop_type: CoopType::Worker,
            min_members: 3,
            founding_members: founders,
            charter_document_hash: None,
            description: Some("A test cooperative".to_string()),
            currency: Some("hours".to_string()),
        }
    }

    #[tokio::test]
    async fn test_create_from_request() {
        let manager = LifecycleManager::new();
        let founder1 = create_test_did();
        let founder2 = create_test_did();
        let founder3 = create_test_did();

        let request =
            create_test_formation_request(vec![founder1.clone(), founder2.clone(), founder3]);
        let (coop, event) = manager
            .create_from_request(request, "test-coop-1".to_string(), founder1.clone())
            .await
            .unwrap();

        assert_eq!(coop.id, "test-coop-1");
        assert_eq!(coop.name, "Test Coop");
        assert_eq!(coop.status, CoopStatus::Forming);
        assert_eq!(coop.min_founders, 3);
        assert!(!coop.charter_ratified);

        if let LifecycleEvent::Created { coop_id, founder } = event {
            assert_eq!(coop_id, "test-coop-1");
            assert_eq!(founder, founder1);
        } else {
            panic!("Expected Created event");
        }
    }

    #[tokio::test]
    async fn test_sign_charter_workflow() {
        let manager = LifecycleManager::new();
        let founder1 = create_test_did();
        let founder2 = create_test_did();
        let founder3 = create_test_did();

        let request = create_test_formation_request(vec![
            founder1.clone(),
            founder2.clone(),
            founder3.clone(),
        ]);
        let (mut coop, _) = manager
            .create_from_request(request, "test-coop-2".to_string(), founder1.clone())
            .await
            .unwrap();

        // First signature
        let sig1 = FounderSignature {
            did: founder1.clone(),
            signature: vec![1, 2, 3],
            timestamp: 1000,
            role: Some("initiator".to_string()),
        };
        let (coop_updated, events) = manager.sign_charter(coop, sig1).await.unwrap();
        coop = coop_updated;

        assert_eq!(events.len(), 1);
        if let LifecycleEvent::CharterSigned {
            signatures_collected,
            signatures_required,
            ..
        } = &events[0]
        {
            assert_eq!(*signatures_collected, 1);
            assert_eq!(*signatures_required, 3);
        } else {
            panic!("Expected CharterSigned event");
        }
        assert!(!coop.charter_ratified);

        // Second signature
        let sig2 = FounderSignature {
            did: founder2.clone(),
            signature: vec![4, 5, 6],
            timestamp: 1001,
            role: None,
        };
        let (coop_updated, events) = manager.sign_charter(coop, sig2).await.unwrap();
        coop = coop_updated;

        assert_eq!(events.len(), 1);
        assert!(!coop.charter_ratified);

        // Third signature (should ratify)
        let sig3 = FounderSignature {
            did: founder3.clone(),
            signature: vec![7, 8, 9],
            timestamp: 1002,
            role: None,
        };
        let (coop_updated, events) = manager.sign_charter(coop, sig3).await.unwrap();
        coop = coop_updated;

        assert_eq!(events.len(), 2); // CharterSigned + CharterRatified
        assert!(coop.charter_ratified);
        assert!(matches!(events[1], LifecycleEvent::CharterRatified { .. }));
    }

    #[tokio::test]
    async fn test_duplicate_signature_rejected() {
        let manager = LifecycleManager::new();
        let founder1 = create_test_did();
        let founder2 = create_test_did();
        let founder3 = create_test_did();

        let request = create_test_formation_request(vec![founder1.clone(), founder2, founder3]);
        let (coop, _) = manager
            .create_from_request(request, "test-coop-3".to_string(), founder1.clone())
            .await
            .unwrap();

        let sig1 = FounderSignature {
            did: founder1.clone(),
            signature: vec![1, 2, 3],
            timestamp: 1000,
            role: None,
        };
        let (coop, _) = manager.sign_charter(coop, sig1.clone()).await.unwrap();

        // Try to sign again with same DID
        let result = manager.sign_charter(coop, sig1).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CoopError::DuplicateMember(_)));
    }

    #[tokio::test]
    async fn test_dissolution_workflow() {
        let manager = LifecycleManager::new();
        let founder = create_test_did();

        let mut coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        coop.status = CoopStatus::Active;

        let plan = AssetDistributionPlan::default();
        let (coop, event) = manager
            .start_dissolution_with_plan(
                coop,
                founder.clone(),
                plan.clone(),
                Some("prop-123".to_string()),
            )
            .await
            .unwrap();

        assert_eq!(coop.status, CoopStatus::Dissolving);
        assert!(coop.dissolution_plan.is_some());
        assert_eq!(coop.dissolution_proposal_id, Some("prop-123".to_string()));

        if let LifecycleEvent::DissolutionStarted {
            initiator,
            proposal_id,
            ..
        } = event
        {
            assert_eq!(initiator, founder);
            assert_eq!(proposal_id, Some("prop-123".to_string()));
        } else {
            panic!("Expected DissolutionStarted event");
        }

        // Complete dissolution
        let (coop, events) = manager.complete_dissolution_with_plan(coop).await.unwrap();

        assert_eq!(coop.status, CoopStatus::Dissolved);
        assert_eq!(events.len(), 2); // AssetsDistributed + Dissolved
        assert!(matches!(
            events[0],
            LifecycleEvent::AssetsDistributed { .. }
        ));
        assert!(matches!(events[1], LifecycleEvent::Dissolved { .. }));
    }

    #[tokio::test]
    async fn test_invalid_state_transitions() {
        let manager = LifecycleManager::new();

        // Cannot dissolve from Forming state
        let coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        assert_eq!(coop.status, CoopStatus::Forming);

        let founder = create_test_did();
        let plan = AssetDistributionPlan::default();
        let result = manager
            .start_dissolution_with_plan(coop, founder, plan, None)
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CoopError::InvalidStateTransition(_)
        ));
    }

    #[tokio::test]
    async fn test_cannot_sign_charter_after_activation() {
        let manager = LifecycleManager::new();
        let founder = create_test_did();

        let mut coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        coop.status = CoopStatus::Active; // Simulate already activated

        let sig = FounderSignature {
            did: founder,
            signature: vec![1, 2, 3],
            timestamp: 1000,
            role: None,
        };
        let result = manager.sign_charter(coop, sig).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CoopError::InvalidStateTransition(_)
        ));
    }
}
