use icn_governance_kernel::{
    CclInterpreter, 
    CclError
};
use icn_identity::IdentityScope;

// Test the interpreter with a valid cooperative bylaws
#[test]
fn test_cooperative_bylaws_interpretation() {
    let interpreter = CclInterpreter::new();
    
    // Based on the example template structure
    let ccl = r#"coop_bylaws {
        "name": "Test Cooperative",
        "description": "A cooperative for testing CCL interpretation",
        "founding_date": "2023-01-01",
        "mission_statement": "To build a better world through shared ownership",
        
        "governance": {
            "decision_making": "consent",
            "quorum": 0.75,
            "majority": 0.66,
            "term_length": 365,
            "roles": [
                {
                    "name": "member",
                    "permissions": [
                        "vote_on_proposals",
                        "create_proposals"
                    ]
                },
                {
                    "name": "steward",
                    "permissions": [
                        "vote_on_proposals",
                        "create_proposals",
                        "manage_working_groups"
                    ]
                }
            ]
        },
        
        "membership": {
            "onboarding": {
                "requires_sponsor": true,
                "trial_period_days": 90,
                "requirements": [
                    "Complete orientation",
                    "Attend one meeting"
                ]
            },
            "dues": {
                "amount": 10,
                "frequency": "monthly",
                "variable_options": [
                    {
                        "amount": 0,
                        "description": "Financial hardship"
                    },
                    {
                        "amount": 25,
                        "description": "Supporting membership"
                    }
                ]
            }
        },
        
        "dispute_resolution": {
            "process": [
                "Direct communication",
                "Facilitated dialogue",
                "Mediation"
            ],
            "committee_size": 3
        }
    }"#;
    
    // Parse and validate using the interpreter
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Cooperative);
    assert!(result.is_ok(), "Failed to interpret valid CCL: {:?}", result.err());
    
    let config = result.unwrap();
    
    // Verify template type and scope
    assert_eq!(config.template_type, "coop_bylaws");
    assert_eq!(config.template_version, "v1");
    assert_eq!(config.governing_scope, IdentityScope::Cooperative);
    
    // Verify identity info
    let identity = config.identity.unwrap();
    assert_eq!(identity.name, Some("Test Cooperative".to_string()));
    assert_eq!(identity.description, Some("A cooperative for testing CCL interpretation".to_string()));
    assert_eq!(identity.founding_date, Some("2023-01-01".to_string()));
    assert_eq!(identity.mission_statement, Some("To build a better world through shared ownership".to_string()));
    
    // Verify governance structure
    let governance = config.governance.unwrap();
    assert_eq!(governance.decision_making, Some("consent".to_string()));
    assert_eq!(governance.quorum, Some(0.75));
    assert_eq!(governance.majority, Some(0.66));
    assert_eq!(governance.term_length, Some(365));
    
    // Verify roles
    let roles = governance.roles.unwrap();
    assert_eq!(roles.len(), 2);
    assert_eq!(roles[0].name, "member");
    assert_eq!(roles[0].permissions.len(), 2);
    assert_eq!(roles[1].name, "steward");
    assert_eq!(roles[1].permissions.len(), 3);
    
    // Verify membership rules
    let membership = config.membership.unwrap();
    let onboarding = membership.onboarding.unwrap();
    assert_eq!(onboarding.requires_sponsor, Some(true));
    assert_eq!(onboarding.trial_period_days, Some(90));
    assert_eq!(onboarding.requirements.unwrap().len(), 2);
    
    let dues = membership.dues.unwrap();
    assert_eq!(dues.amount, Some(10));
    assert_eq!(dues.frequency, Some("monthly".to_string()));
    let variable_options = dues.variable_options.unwrap();
    assert_eq!(variable_options.len(), 2);
    assert_eq!(variable_options[0].amount, 0);
    assert_eq!(variable_options[0].description, "Financial hardship");
    
    // Verify dispute resolution
    let dispute = config.dispute_resolution.unwrap();
    assert_eq!(dispute.process.unwrap().len(), 3);
    assert_eq!(dispute.committee_size, Some(3));
}

// Test the community charter template
#[test]
fn test_community_charter_interpretation() {
    let interpreter = CclInterpreter::new();
    
    let ccl = r#"community_charter {
        "name": "Test Community",
        "description": "A community for testing CCL interpretation",
        "founding_date": "2023-02-15",
        "mission_statement": "To create a thriving community ecosystem",
        
        "governance": {
            "decision_making": "consensus",
            "quorum": 0.6,
            "majority": 0.75,
            "roles": [
                {
                    "name": "community_member",
                    "permissions": [
                        "participate_in_discussions",
                        "vote_on_proposals"
                    ]
                },
                {
                    "name": "facilitator",
                    "permissions": [
                        "participate_in_discussions",
                        "vote_on_proposals",
                        "facilitate_meetings",
                        "moderate_content"
                    ]
                }
            ]
        },
        
        "working_groups": {
            "formation_threshold": 3,
            "dissolution_threshold": 2,
            "resource_allocation": {
                "default_budget": 500,
                "requires_approval": true
            }
        }
    }"#;
    
    // Parse and validate using the interpreter
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Community);
    assert!(result.is_ok(), "Failed to interpret valid community charter: {:?}", result.err());
    
    let config = result.unwrap();
    
    // Verify template type and scope
    assert_eq!(config.template_type, "community_charter");
    assert_eq!(config.template_version, "v1");
    assert_eq!(config.governing_scope, IdentityScope::Community);
    
    // Verify identity info
    let identity = config.identity.unwrap();
    assert_eq!(identity.name, Some("Test Community".to_string()));
    assert_eq!(identity.description, Some("A community for testing CCL interpretation".to_string()));
    assert_eq!(identity.founding_date, Some("2023-02-15".to_string()));
    
    // Verify governance structure
    let governance = config.governance.unwrap();
    assert_eq!(governance.decision_making, Some("consensus".to_string()));
    assert_eq!(governance.quorum, Some(0.6));
    assert_eq!(governance.majority, Some(0.75));
    
    // Verify roles
    let roles = governance.roles.unwrap();
    assert_eq!(roles.len(), 2);
    assert_eq!(roles[0].name, "community_member");
    assert_eq!(roles[0].permissions.len(), 2);
    assert_eq!(roles[1].name, "facilitator");
    assert_eq!(roles[1].permissions.len(), 4);
    
    // Verify working groups
    let working_groups = config.working_groups.unwrap();
    assert_eq!(working_groups.formation_threshold, Some(3));
    assert_eq!(working_groups.dissolution_threshold, Some(2));
    
    let resource_allocation = working_groups.resource_allocation.unwrap();
    assert_eq!(resource_allocation.default_budget, Some(500));
    assert_eq!(resource_allocation.requires_approval, Some(true));
}

// Test budget proposal template
#[test]
fn test_budget_proposal() {
    let interpreter = CclInterpreter::new();
    
    let ccl = r#"budget_proposal {
        "name": "Q2 Budget",
        "economic_model": {
            "surplus_distribution": "equal",
            "compensation_policy": {
                "hourly_rates": {
                    "standard": 15
                }
            }
        }
    }"#;
    
    // Budget proposals can be used in any scope
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Individual);
    assert!(result.is_ok(), "Failed to interpret valid budget proposal: {:?}", result.err());
    
    let config = result.unwrap();
    assert_eq!(config.template_type, "budget_proposal");
    
    let economic = config.economic_model.unwrap();
    assert_eq!(economic.surplus_distribution, Some("equal".to_string()));
    
    let compensation = economic.compensation_policy.unwrap();
    let rates = compensation.hourly_rates.unwrap();
    assert_eq!(rates.get("standard"), Some(&15));
}

// Test resolution template
#[test]
fn test_resolution_template() {
    let interpreter = CclInterpreter::new();
    
    let ccl = r#"resolution {
        "name": "Strategic Direction Resolution",
        "description": "A resolution to establish our strategic direction for the year",
        
        "proposals": {
            "types": [
                {
                    "name": "organizational_direction",
                    "quorum_modifier": 1.2,
                    "majority_modifier": 1.1,
                    "discussion_period_days": 14
                }
            ]
        }
    }"#;
    
    // Resolutions can be used in any scope
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Cooperative);
    assert!(result.is_ok(), "Failed to interpret valid resolution: {:?}", result.err());
    
    let config = result.unwrap();
    assert_eq!(config.template_type, "resolution");
    
    // Verify identity info
    let identity = config.identity.unwrap();
    assert_eq!(identity.name, Some("Strategic Direction Resolution".to_string()));
    
    // Verify proposals
    let proposals = config.proposals.unwrap();
    let types = proposals.types.unwrap();
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].name, "organizational_direction");
    assert_eq!(types[0].quorum_modifier, Some(1.2));
    assert_eq!(types[0].majority_modifier, Some(1.1));
    assert_eq!(types[0].discussion_period_days, Some(14));
}

// Test participation rules template
#[test]
fn test_participation_rules() {
    let interpreter = CclInterpreter::new();
    
    let ccl = r#"participation_rules {
        "name": "Community Participation Guidelines",
        "description": "Guidelines for participation in our community",
        
        "governance": {
            "decision_making": "majority",
            "quorum": 0.5
        },
        
        "membership": {
            "onboarding": {
                "requires_sponsor": false,
                "trial_period_days": 30,
                "requirements": [
                    "Accept code of conduct",
                    "Complete introduction"
                ]
            },
            "offboarding": {
                "notice_period_days": 14,
                "max_inactive_days": 120
            }
        }
    }"#;
    
    // Participation rules can be used in any scope
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Community);
    assert!(result.is_ok(), "Failed to interpret valid participation rules: {:?}", result.err());
    
    let config = result.unwrap();
    assert_eq!(config.template_type, "participation_rules");
    
    // Verify governance structure
    let governance = config.governance.unwrap();
    assert_eq!(governance.decision_making, Some("majority".to_string()));
    assert_eq!(governance.quorum, Some(0.5));
    
    // Verify membership
    let membership = config.membership.unwrap();
    let onboarding = membership.onboarding.unwrap();
    assert_eq!(onboarding.requires_sponsor, Some(false));
    assert_eq!(onboarding.trial_period_days, Some(30));
    
    let offboarding = membership.offboarding.unwrap();
    assert_eq!(offboarding.notice_period_days, Some(14));
    assert_eq!(offboarding.max_inactive_days, Some(120));
}

// Test invalid template for scope
#[test]
fn test_invalid_template_for_scope() {
    let interpreter = CclInterpreter::new();
    
    // Community charter template with cooperative scope
    let ccl = r#"community_charter {
        "name": "Test Community",
        "description": "A test community",
        "governance": {
            "decision_making": "consensus"
        }
    }"#;
    
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Cooperative);
    assert!(result.is_err(), "Expected error for invalid template for scope");
    
    match result.unwrap_err() {
        CclError::InvalidTemplateForScope { template, scope } => {
            assert_eq!(template, "community_charter");
            assert_eq!(scope, IdentityScope::Cooperative);
        },
        err => panic!("Expected InvalidTemplateForScope, got: {:?}", err),
    }
}

// Test unsupported template version
#[test]
fn test_unsupported_template_version() {
    let interpreter = CclInterpreter::new();
    
    // Cooperative bylaws with unsupported version
    let ccl = r#"coop_bylaws:v99 {
        "name": "Test Cooperative",
        "description": "A test cooperative",
        "governance": {
            "decision_making": "consent"
        }
    }"#;
    
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Cooperative);
    assert!(result.is_err(), "Expected error for unsupported template version");
    
    match result.unwrap_err() {
        CclError::UnsupportedTemplateVersion { template, version } => {
            assert_eq!(template, "coop_bylaws:v99");
            assert_eq!(version, "v99");
        },
        err => panic!("Expected UnsupportedTemplateVersion, got: {:?}", err),
    }
}

// Test missing required fields
#[test]
fn test_missing_required_fields() {
    let interpreter = CclInterpreter::new();
    
    // Cooperative bylaws without governance
    let ccl = r#"coop_bylaws {
        "name": "Test Cooperative",
        "description": "A test cooperative"
    }"#;
    
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Cooperative);
    assert!(result.is_err(), "Expected error for missing required field");
    
    match result.unwrap_err() {
        CclError::MissingRequiredField(msg) => {
            assert!(msg.contains("governance"), "Error message should mention missing governance");
        },
        err => panic!("Expected MissingRequiredField, got: {:?}", err),
    }
    
    // Participation rules without membership
    let ccl = r#"participation_rules {
        "name": "Community Guidelines",
        "governance": {
            "decision_making": "majority"
        }
    }"#;
    
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Community);
    assert!(result.is_err(), "Expected error for missing required field");
    
    match result.unwrap_err() {
        CclError::MissingRequiredField(msg) => {
            assert!(msg.contains("membership"), "Error message should mention missing membership");
        },
        err => panic!("Expected MissingRequiredField, got: {:?}", err),
    }
}

// Test with type mismatches
#[test]
fn test_type_mismatches() {
    let interpreter = CclInterpreter::new();
    
    // Quorum as string instead of number
    let ccl = r#"coop_bylaws {
        "name": "Test Cooperative",
        "description": "A test cooperative",
        "governance": {
            "decision_making": "consent",
            "quorum": "not a number"
        }
    }"#;
    
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Cooperative);
    assert!(result.is_err(), "Expected error for type mismatch");
    
    match result.unwrap_err() {
        CclError::TypeMismatch { field, expected, .. } => {
            assert_eq!(field, "quorum");
            assert_eq!(expected, "number");
        },
        err => panic!("Expected TypeMismatch, got: {:?}", err),
    }
    
    // Trial period as boolean instead of number
    let ccl = r#"participation_rules {
        "name": "Community Guidelines",
        "governance": {
            "decision_making": "majority"
        },
        "membership": {
            "onboarding": {
                "trial_period_days": true
            }
        }
    }"#;
    
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Community);
    assert!(result.is_err(), "Expected error for type mismatch");
    
    match result.unwrap_err() {
        CclError::TypeMismatch { field, expected, .. } => {
            assert_eq!(field, "trial_period_days");
            assert_eq!(expected, "integer");
        },
        err => panic!("Expected TypeMismatch, got: {:?}", err),
    }
}

// Test with syntax errors
#[test]
fn test_syntax_errors() {
    let interpreter = CclInterpreter::new();
    
    // Missing comma between properties
    let ccl = r#"coop_bylaws {
        "name": "Test Cooperative"
        "description": "Missing comma"
    }"#;
    
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Cooperative);
    assert!(result.is_err(), "Expected error for syntax error");
    
    match result.unwrap_err() {
        CclError::SyntaxError(_) => {}, // Expected error
        err => panic!("Expected SyntaxError, got: {:?}", err),
    }
    
    // Unclosed object
    let ccl = r#"community_charter {
        "name": "Test Community",
        "governance": {
            "decision_making": "consensus"
    }"#;
    
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Community);
    assert!(result.is_err(), "Expected error for syntax error");
    
    match result.unwrap_err() {
        CclError::SyntaxError(_) => {}, // Expected error
        err => panic!("Expected SyntaxError, got: {:?}", err),
    }
}

// Test nested objects and arrays
#[test]
fn test_nested_structures() {
    let interpreter = CclInterpreter::new();
    
    let ccl = r#"coop_bylaws {
        "name": "Test Cooperative",
        "description": "Testing nested structures",
        "governance": {
            "decision_making": "consent",
            "quorum": 0.75
        },
        "economic_model": {
            "surplus_distribution": "equal",
            "compensation_policy": {
                "hourly_rates": {
                    "standard": 15,
                    "specialized": 20
                },
                "track_hours": true,
                "volunteer_options": false
            }
        }
    }"#;
    
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Cooperative);
    assert!(result.is_ok(), "Failed to interpret: {:?}", result.err());
    
    let config = result.unwrap();
    let economic = config.economic_model.unwrap();
    assert_eq!(economic.surplus_distribution, Some("equal".to_string()));
    
    let compensation = economic.compensation_policy.unwrap();
    assert_eq!(compensation.track_hours, Some(true));
    assert_eq!(compensation.volunteer_options, Some(false));
    
    let rates = compensation.hourly_rates.unwrap();
    assert_eq!(rates.get("standard"), Some(&15));
    assert_eq!(rates.get("specialized"), Some(&20));
}

// Test with template version specified
#[test]
fn test_template_with_version() {
    let interpreter = CclInterpreter::new();
    
    let ccl = r#"coop_bylaws:v2 {
        "name": "Test Cooperative",
        "description": "Testing template versions",
        "governance": {
            "decision_making": "consent",
            "quorum": 0.75
        }
    }"#;
    
    let result = interpreter.interpret_ccl(ccl, IdentityScope::Cooperative);
    assert!(result.is_ok(), "Failed to interpret with explicit version: {:?}", result.err());
    
    let config = result.unwrap();
    assert_eq!(config.template_type, "coop_bylaws");
    assert_eq!(config.template_version, "v2");
} 