//! Governance Service Implementation
//!
//! Implements `GovernanceService` trait from `icn-kernel-api`.
//! This provides the full governance interface for kernel integration.

use icn_governance::ProtocolParameterStore;
use icn_kernel_api::authz::PolicyOracle;
use icn_kernel_api::services::{GovernanceEvent, GovernanceService};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::GovernancePolicyOracle;

/// Implementation of `GovernanceService` for the ICN governance subsystem
///
/// This service wraps the protocol parameter store and exposes governance
/// functionality through the kernel-api interface.
pub struct GovernanceServiceImpl {
    parameter_store: Arc<dyn ProtocolParameterStore>,
    oracle: Arc<GovernancePolicyOracle>,
}

impl GovernanceServiceImpl {
    /// Create a new governance service
    pub fn new(parameter_store: Arc<dyn ProtocolParameterStore>) -> Self {
        let oracle = Arc::new(GovernancePolicyOracle::new(parameter_store.clone()));
        Self {
            parameter_store,
            oracle,
        }
    }

    /// Get access to the underlying parameter store
    ///
    /// This is for internal use only - the kernel should use the trait methods.
    pub fn parameter_store(&self) -> &Arc<dyn ProtocolParameterStore> {
        &self.parameter_store
    }
}

impl GovernanceService for GovernanceServiceImpl {
    fn oracle(&self) -> Arc<dyn PolicyOracle> {
        self.oracle.clone()
    }

    fn get_parameter(&self, key: &str) -> Option<String> {
        match self.parameter_store.get(key) {
            Ok(Some(param)) => {
                debug!(key = %key, "Governance parameter accessed");
                Some(param.value.to_string())
            }
            Ok(None) => {
                debug!(key = %key, "Governance parameter not found");
                None
            }
            Err(e) => {
                warn!(key = %key, error = %e, "Failed to get governance parameter");
                None
            }
        }
    }

    fn list_parameters(&self) -> Vec<String> {
        match self.parameter_store.list() {
            Ok(params) => params.into_iter().map(|p| p.id).collect(),
            Err(e) => {
                warn!(error = %e, "Failed to list governance parameters");
                Vec::new()
            }
        }
    }

    fn record_event(&self, event: GovernanceEvent) {
        // Log events for observability
        match &event {
            GovernanceEvent::ParameterAccessed { key, accessor } => {
                debug!(key = %key, accessor = %accessor, "Governance parameter accessed");
            }
            GovernanceEvent::ParameterChangeRequested {
                key,
                new_value,
                requestor,
            } => {
                debug!(
                    key = %key,
                    new_value = %new_value,
                    requestor = %requestor,
                    "Governance parameter change requested"
                );
                // TODO: This should trigger the governance proposal flow
                // For now, we just log it
            }
            GovernanceEvent::Action {
                action_type,
                actor,
                metadata,
            } => {
                debug!(
                    action_type = %action_type,
                    actor = %actor,
                    metadata = ?metadata,
                    "Governance action recorded"
                );
            }
        }

        // TODO: Emit metrics
        // icn_obs::metrics::governance::event_recorded_inc(&event_type);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_governance::{
        ParameterConstraints, ParameterScope, ParameterValue, ProtocolParameter, SledParameterStore,
    };
    use icn_kernel_api::types::Did;

    fn create_test_store() -> Arc<dyn ProtocolParameterStore> {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = sled::open(temp_dir.path()).unwrap();
        Arc::new(SledParameterStore::new(Arc::new(db)).unwrap())
    }

    fn create_test_param(id: &str, value: ParameterValue) -> ProtocolParameter {
        ProtocolParameter {
            id: id.to_string(),
            name: id.to_string(),
            description: "Test parameter".to_string(),
            value,
            constraints: ParameterConstraints::default(),
            scope: ParameterScope::Global,
            updated_at: 0,
            updated_by: None,
            version: 0,
        }
    }

    #[test]
    fn test_service_oracle() {
        let store = create_test_store();
        let service = GovernanceServiceImpl::new(store);

        let oracle = service.oracle();
        assert_eq!(oracle.domain().as_str(), "governance");
    }

    #[test]
    fn test_service_get_parameter() {
        let store = create_test_store();

        // Add a test parameter
        let param = create_test_param(
            "test.param",
            ParameterValue::String("test_value".to_string()),
        );
        store.set(param, None, None).unwrap();

        let service = GovernanceServiceImpl::new(store);

        // Note: ParameterValue::String wraps in quotes when displayed
        assert_eq!(
            service.get_parameter("test.param"),
            Some("\"test_value\"".to_string())
        );
        assert_eq!(service.get_parameter("nonexistent"), None);
    }

    #[test]
    fn test_service_list_parameters() {
        let store = create_test_store();

        // Add some test parameters
        for i in 0..3 {
            let param = create_test_param(
                &format!("test.param.{}", i),
                ParameterValue::Integer(i as i64),
            );
            store.set(param, None, None).unwrap();
        }

        let service = GovernanceServiceImpl::new(store);
        let params = service.list_parameters();

        assert_eq!(params.len(), 3);
        assert!(params.contains(&"test.param.0".to_string()));
        assert!(params.contains(&"test.param.1".to_string()));
        assert!(params.contains(&"test.param.2".to_string()));
    }

    #[test]
    fn test_service_record_event() {
        let store = create_test_store();
        let service = GovernanceServiceImpl::new(store);

        // This should not panic
        service.record_event(GovernanceEvent::ParameterAccessed {
            key: "test.key".to_string(),
            accessor: Did::from("did:icn:test123".to_string()),
        });

        service.record_event(GovernanceEvent::ParameterChangeRequested {
            key: "test.key".to_string(),
            new_value: "new_value".to_string(),
            requestor: Did::from("did:icn:test123".to_string()),
        });

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("foo".to_string(), "bar".to_string());
        service.record_event(GovernanceEvent::Action {
            action_type: "test_action".to_string(),
            actor: Did::from("did:icn:test123".to_string()),
            metadata,
        });
    }
}
