use icn_governance_kernel::{
    CclInterpreter, 
    CclError,
    config::{
        GovernanceConfig, MembershipRules, GovernanceStructure, Role,
        DisputeResolution, EconomicModel, IdentityInfo
    }
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
    assert!(result.is_err());
    
    match result.unwrap_err() {
        CclError::InvalidTemplateForScope { template, scope } => {
            assert_eq!(template, "community_charter");
            assert_eq!(scope, IdentityScope::Cooperative);
        },
        err => panic!("Expected InvalidTemplateForScope, got: {:?}", err),
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
    assert!(result.is_err());
    
    match result.unwrap_err() {
        CclError::MissingRequiredField(_) => {}, // Expected error
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
    assert!(result.is_err());
    
    match result.unwrap_err() {
        CclError::TypeMismatch { field, expected, .. } => {
            assert_eq!(field, "quorum");
            assert_eq!(expected, "number");
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
    assert!(result.is_err());
    
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
    if let Err(ref e) = result {
        println!("Error: {:?}", e);
    }
    assert!(result.is_ok());
    
    let config = result.unwrap();
    assert_eq!(config.template_type, "coop_bylaws");
    assert_eq!(config.template_version, "v2");
}

// Test with budget proposal template
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
    assert!(result.is_ok());
    
    let config = result.unwrap();
    assert_eq!(config.template_type, "budget_proposal");
    
    let economic = config.economic_model.unwrap();
    assert_eq!(economic.surplus_distribution, Some("equal".to_string()));
} 