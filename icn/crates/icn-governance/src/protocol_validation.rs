//! Protocol Parameter Validation Helpers
//!
//! This module provides shared validation logic for parameter store implementations.
//! Both `InMemoryParameterStore` and `SledParameterStore` use these helpers to ensure
//! consistent validation behavior.
//!
//! # Validation Order
//!
//! The validation sequence for parameter updates is:
//! 1. Value validation against stored constraints (prevents constraint bypass attacks)
//! 2. Scope override permission check
//! 3. Version check (optimistic locking)
//!
//! # New vs. Existing Parameters
//!
//! Validation differs based on whether the parameter already exists:
//!
//! - **Existing parameter (update)**: Validates against the STORED parameter's constraints.
//!   This prevents constraint bypass attacks where an attacker submits modified constraints.
//!
//! - **New parameter (create)**: Validates against the parameter's OWN constraints since
//!   there is no stored version. New parameters start at version 0.
//!
//! # Security Considerations
//!
//! - **Constraint Bypass Prevention**: For updates, we validate against the STORED
//!   parameter's constraints, not the incoming parameter's constraints. This prevents
//!   attackers from submitting parameters with modified constraints.
//!
//! - **Scope Override Check**: Parameters can optionally allow/disallow overrides at
//!   cooperative or federation scopes. This is checked before allowing non-global scope updates.
//!
//! - **Optimistic Locking**: Version checks prevent lost updates from concurrent modifications.

use crate::protocol::{ParameterScope, ParameterValue, ProtocolParameter};
use crate::protocol_store::ParameterStoreError;

/// Validate a parameter value against the appropriate constraints.
///
/// For updates (when `stored_param` is `Some`), validates against the stored parameter's
/// constraints to prevent constraint bypass attacks where an attacker submits a parameter
/// with modified constraints.
///
/// For new parameters (when `stored_param` is `None`), validates against the parameter's
/// own constraints.
///
/// # Arguments
///
/// * `new_value` - The value to validate
/// * `new_param` - The incoming parameter (used for new parameter validation)
/// * `stored_param` - The existing stored parameter, if updating
///
/// # Returns
///
/// * `Ok(())` if validation passes
/// * `Err(ParameterStoreError::Validation)` if validation fails
///
/// # Note
///
/// This function returns a generic validation error. Callers should wrap the error
/// with context including the parameter ID for better debugging, e.g.:
/// ```ignore
/// validate_parameter_value(&param.value, &param, stored.as_ref())
///     .map_err(|e| anyhow::anyhow!("Parameter validation failed for '{}': {e}", id))?;
/// ```
pub fn validate_parameter_value(
    new_value: &ParameterValue,
    new_param: &ProtocolParameter,
    stored_param: Option<&ProtocolParameter>,
) -> Result<(), ParameterStoreError> {
    if let Some(stored) = stored_param {
        // For updates, validate against the STORED parameter's constraints
        // This prevents constraint bypass attacks
        stored.validate(new_value)?;
    } else {
        // For new parameters (initial setup), validate against own constraints
        new_param.validate(new_value)?;
    }
    Ok(())
}

/// Check if a scope override is allowed for the parameter.
///
/// Parameters can be configured to allow or disallow overrides at non-global scopes
/// (cooperative or federation). This check enforces that restriction.
///
/// # Arguments
///
/// * `param_id` - The parameter ID (for error messages)
/// * `scope` - The scope being set (Global, Federation, or Cooperative)
/// * `stored_param` - The existing stored parameter (must exist for scope overrides)
///
/// # Returns
///
/// * `Ok(())` if the scope override is allowed (or scope is Global)
/// * `Err(ParameterStoreError::ScopeOverrideNotAllowed)` if overrides are disallowed
pub fn validate_scope_override(
    param_id: &str,
    scope: &ParameterScope,
    stored_param: Option<&ProtocolParameter>,
) -> Result<(), ParameterStoreError> {
    // Global scope is always allowed
    if matches!(scope, ParameterScope::Global) {
        return Ok(());
    }

    // For non-global scopes, we need a stored parameter to check allow_override
    if let Some(stored) = stored_param {
        if !stored.constraints.allow_override {
            return Err(ParameterStoreError::ScopeOverrideNotAllowed {
                parameter_id: param_id.to_string(),
            });
        }
    }
    // Note: If no stored param exists, this is a new scoped parameter
    // The caller should handle this case (typically by requiring the global
    // parameter to exist first)

    Ok(())
}

/// Validate version for optimistic locking.
///
/// Compares the provided version against the stored version to detect concurrent
/// modifications. If versions don't match, another process updated the parameter
/// between the read and write.
///
/// # Arguments
///
/// * `param_id` - The parameter ID (for error messages)
/// * `provided_version` - The version from the incoming update
/// * `stored_version` - The version currently in storage
///
/// # Returns
///
/// * `Ok(())` if versions match
/// * `Err(ParameterStoreError::ConcurrentModification)` if versions differ
pub fn validate_version(
    param_id: &str,
    provided_version: u64,
    stored_version: u64,
) -> Result<(), ParameterStoreError> {
    if provided_version != stored_version {
        return Err(ParameterStoreError::ConcurrentModification {
            parameter_id: param_id.to_string(),
            expected_version: provided_version,
            actual_version: stored_version,
        });
    }
    Ok(())
}

/// Perform all pre-update validations in sequence.
///
/// This is a convenience function that runs all validation checks in the correct order:
/// 1. Value validation
/// 2. Scope override permission check
/// 3. Version check (if stored parameter exists)
///
/// # Arguments
///
/// * `param` - The incoming parameter to validate
/// * `stored_param` - The existing stored parameter, if updating
///
/// # Returns
///
/// * `Ok(())` if all validations pass
/// * `Err(ParameterStoreError)` with the specific validation failure
pub fn validate_parameter_update(
    param: &ProtocolParameter,
    stored_param: Option<&ProtocolParameter>,
) -> Result<(), ParameterStoreError> {
    // 1. Validate value against constraints
    validate_parameter_value(&param.value, param, stored_param)?;

    // 2. Check scope override permissions
    validate_scope_override(&param.id, &param.scope, stored_param)?;

    // 3. Check version for optimistic locking (only for updates)
    if let Some(stored) = stored_param {
        validate_version(&param.id, param.version, stored.version)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{
        ParameterConstraints, ParameterScope, ParameterValue, ProtocolParameter,
    };

    fn make_param(id: &str, value: i64, version: u64, allow_override: bool) -> ProtocolParameter {
        ProtocolParameter {
            id: id.to_string(),
            name: format!("{id} Name"),
            description: format!("{id} Description"),
            value: ParameterValue::Integer(value),
            constraints: ParameterConstraints {
                min: Some(ParameterValue::Integer(0)),
                max: Some(ParameterValue::Integer(100)),
                allowed_values: None,
                allow_override,
                requires_restart: false,
            },
            version,
            scope: ParameterScope::Global,
            updated_at: 0,
            updated_by: None,
        }
    }

    fn coop_scope(name: &str) -> ParameterScope {
        ParameterScope::Cooperative {
            id: name.to_string(),
        }
    }

    #[test]
    fn test_validate_value_against_stored_constraints() {
        let stored = make_param("test.param", 50, 1, true);
        let new_param = make_param("test.param", 75, 1, true);

        // Valid value within constraints
        assert!(
            validate_parameter_value(&ParameterValue::Integer(75), &new_param, Some(&stored))
                .is_ok()
        );

        // Invalid value (exceeds max)
        let result =
            validate_parameter_value(&ParameterValue::Integer(150), &new_param, Some(&stored));
        assert!(matches!(result, Err(ParameterStoreError::Validation(_))));
    }

    #[test]
    fn test_validate_value_for_new_param() {
        let new_param = make_param("test.param", 50, 0, true);

        // Valid value for new param
        assert!(validate_parameter_value(&ParameterValue::Integer(50), &new_param, None).is_ok());

        // Invalid value for new param
        let result = validate_parameter_value(&ParameterValue::Integer(150), &new_param, None);
        assert!(matches!(result, Err(ParameterStoreError::Validation(_))));
    }

    #[test]
    fn test_validate_scope_override_allowed() {
        let stored = make_param("test.param", 50, 1, true);

        // Global scope always allowed
        assert!(
            validate_scope_override("test.param", &ParameterScope::Global, Some(&stored)).is_ok()
        );

        // Cooperative scope allowed when allow_override=true
        assert!(validate_scope_override("test.param", &coop_scope("test"), Some(&stored)).is_ok());
    }

    #[test]
    fn test_validate_scope_override_disallowed() {
        let stored = make_param("test.param", 50, 1, false);

        // Cooperative scope disallowed when allow_override=false
        let result = validate_scope_override("test.param", &coop_scope("test"), Some(&stored));
        assert!(matches!(
            result,
            Err(ParameterStoreError::ScopeOverrideNotAllowed { .. })
        ));
    }

    #[test]
    fn test_validate_version_match() {
        assert!(validate_version("test.param", 5, 5).is_ok());
    }

    #[test]
    fn test_validate_version_mismatch() {
        let result = validate_version("test.param", 5, 10);
        assert!(matches!(
            result,
            Err(ParameterStoreError::ConcurrentModification {
                expected_version: 5,
                actual_version: 10,
                ..
            })
        ));
    }

    #[test]
    fn test_validate_parameter_update_all_checks() {
        let stored = make_param("test.param", 50, 1, true);
        let mut new_param = make_param("test.param", 75, 1, true);
        new_param.scope = coop_scope("test");

        // All validations pass
        assert!(validate_parameter_update(&new_param, Some(&stored)).is_ok());

        // Fails on version mismatch
        new_param.version = 0;
        let result = validate_parameter_update(&new_param, Some(&stored));
        assert!(matches!(
            result,
            Err(ParameterStoreError::ConcurrentModification { .. })
        ));
    }
}
