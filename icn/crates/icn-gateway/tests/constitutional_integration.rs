//! Integration tests for Constitutional Governance flows
//!
//! Tests end-to-end workflows for:
//! - Amendment lifecycle (create → submit → vote → ratify)
//! - Appeal lifecycle (file → review → resolve)
//! - Multi-stakeholder ratification
//! - Appeal outcomes and remedies

use icn_gateway::commons_mgr::CommonsManager;
use icn_governance::{
    amendment::{
        Amendment, AmendmentChange, AmendmentScope, AmendmentType, ChangeTarget, ChangeType,
        Ratification, RatifierType,
    },
    appeal::{Appeal, AppealGrounds, AppealOutcome, AppealRemedy, AppealScope, AppealType},
    Charter, CharterStatus, DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType,
};
use icn_identity::KeyPair;

/// Helper to create a test charter and activate it
async fn create_active_charter(commons_mgr: &CommonsManager) -> String {
    let mut charter = Charter::new(
        OrgType::Cooperative,
        "coop:constitutional-test".to_string(),
        "Constitutional Test Coop".to_string(),
        GovernanceConfig::cooperative_default(),
        MembershipPolicy::default(),
        DisputePolicy::default(),
    );

    let charter_id = charter.charter_id.to_hex();

    // Add founders
    for i in 0..3u8 {
        let keypair = KeyPair::generate().unwrap();
        let sig = icn_governance::FounderSignature {
            did: keypair.did().clone(),
            signature: vec![i; 64],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            role: Some("founder".to_string()),
        };
        charter.add_founder(sig);
    }

    commons_mgr.store_charter(charter).await.unwrap();
    commons_mgr
        .update_charter_status(&charter_id, CharterStatus::Active)
        .await
        .unwrap();

    charter_id
}

/// Test complete amendment lifecycle: create → submit → vote → ratify
#[actix_web::test]
async fn test_amendment_full_lifecycle() {
    let commons_mgr = CommonsManager::new();

    // Create and activate a charter
    let _charter_id = create_active_charter(&commons_mgr).await;

    // Create proposer
    let proposer = KeyPair::generate().unwrap();
    let proposer_did = proposer.did().clone();

    // Create an amendment
    let mut amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: "coop:constitutional-test".to_string(),
        },
        "Update Membership Policy".to_string(),
        "Change minimum membership period from 30 to 60 days".to_string(),
        proposer_did.clone(),
    );

    // Set zero review period for testing
    amendment.requirements.review_period_secs = 0;

    // Add change before submitting
    amendment.add_change(AmendmentChange {
        target: ChangeTarget::MembershipPolicy,
        change_type: ChangeType::Modify,
        description: "Increase probation period".to_string(),
        old_value: Some("30 days".to_string()),
        new_value: "60 days".to_string(),
    });

    let amendment_id = amendment.id.clone();

    // Store the amendment
    commons_mgr.store_amendment(amendment).await.unwrap();

    // Verify it's in Draft status
    let stored = commons_mgr.get_amendment(&amendment_id).await.unwrap().unwrap();
    assert!(matches!(
        stored.status,
        icn_governance::amendment::AmendmentStatus::Draft
    ));

    // Submit the amendment
    let submitted = commons_mgr
        .submit_amendment(&amendment_id, &proposer_did)
        .await
        .unwrap();
    assert!(matches!(
        submitted.status,
        icn_governance::amendment::AmendmentStatus::Submitted { .. }
    ));

    // Open voting
    let voting = commons_mgr
        .open_amendment_voting(&amendment_id, &proposer_did)
        .await
        .unwrap();
    assert!(matches!(
        voting.status,
        icn_governance::amendment::AmendmentStatus::Voting { .. }
    ));

    // Verify the store has the voting status
    let stored_voting = commons_mgr.get_amendment(&amendment_id).await.unwrap().unwrap();
    assert!(
        stored_voting.status.is_active(),
        "Expected active status in store, got {:?}",
        stored_voting.status
    );

    // Add a ratification - with a single approval at 100%, amendment is auto-ratified
    let ratifier = KeyPair::generate().unwrap();
    let ratification = Ratification {
        ratifier_id: "holder-0".to_string(),
        ratifier_type: RatifierType::CommonsHolder,
        authority_did: ratifier.did().clone(),
        approved: true,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        signature: vec![0u8; 64],
        comment: Some("Approved by holder 0".to_string()),
    };

    commons_mgr
        .add_amendment_ratification(&amendment_id, ratification)
        .await
        .unwrap();

    // Verify the amendment was auto-ratified (1/1 = 100% > 67% threshold)
    let ratified = commons_mgr.get_amendment(&amendment_id).await.unwrap().unwrap();
    assert_eq!(ratified.ratifications.len(), 1);
    assert!(matches!(
        ratified.status,
        icn_governance::amendment::AmendmentStatus::Ratified { .. }
    ));
}

/// Test amendment withdrawal by proposer
#[actix_web::test]
async fn test_amendment_withdrawal() {
    let commons_mgr = CommonsManager::new();

    let proposer = KeyPair::generate().unwrap();
    let proposer_did = proposer.did().clone();

    let mut amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: "coop:test".to_string(),
        },
        "Test Amendment".to_string(),
        "This will be withdrawn".to_string(),
        proposer_did.clone(),
    );

    // Must add a change before submitting
    amendment.add_change(AmendmentChange {
        target: ChangeTarget::GovernanceRules,
        change_type: ChangeType::Modify,
        description: "Test change".to_string(),
        old_value: None,
        new_value: "new value".to_string(),
    });

    let amendment_id = amendment.id.clone();
    commons_mgr.store_amendment(amendment).await.unwrap();

    // Submit then withdraw
    commons_mgr
        .submit_amendment(&amendment_id, &proposer_did)
        .await
        .unwrap();

    let withdrawn = commons_mgr
        .withdraw_amendment(&amendment_id, &proposer_did, "Changed my mind".to_string())
        .await
        .unwrap();

    assert!(matches!(
        withdrawn.status,
        icn_governance::amendment::AmendmentStatus::Withdrawn { .. }
    ));
}

/// Test that only proposer can withdraw amendment
#[actix_web::test]
async fn test_amendment_withdrawal_requires_proposer() {
    let commons_mgr = CommonsManager::new();

    let proposer = KeyPair::generate().unwrap();
    let other_user = KeyPair::generate().unwrap();

    let mut amendment = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: "coop:test".to_string(),
        },
        "Test Amendment".to_string(),
        "Test".to_string(),
        proposer.did().clone(),
    );

    amendment.add_change(AmendmentChange {
        target: ChangeTarget::GovernanceRules,
        change_type: ChangeType::Modify,
        description: "Test".to_string(),
        old_value: None,
        new_value: "new".to_string(),
    });

    let amendment_id = amendment.id.clone();
    commons_mgr.store_amendment(amendment).await.unwrap();

    // Submit as proposer
    commons_mgr
        .submit_amendment(&amendment_id, proposer.did())
        .await
        .unwrap();

    // Try to withdraw as different user - should fail
    let result = commons_mgr
        .withdraw_amendment(&amendment_id, other_user.did(), "Unauthorized".to_string())
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("proposer"));
}

/// Test appeal lifecycle: file → review → resolve
#[actix_web::test]
async fn test_appeal_full_lifecycle() {
    let commons_mgr = CommonsManager::new();

    let appellant = KeyPair::generate().unwrap();
    let appellant_did = appellant.did().clone();

    // File an appeal
    let appeal = Appeal::new(
        AppealType::MembershipDenial {
            jurisdiction_id: "coop:test".to_string(),
            application_id: None,
        },
        AppealScope::Jurisdiction {
            domain_id: "coop:test".to_string(),
        },
        appellant_did.clone(),
        vec![AppealGrounds::ProceduralError {
            description: "No notice given before denial".to_string(),
        }],
        "My membership was denied without proper review".to_string(),
        AppealRemedy::Reinstate,
    );

    let appeal_id = appeal.id.clone();

    // Store the appeal
    commons_mgr.store_appeal(appeal).await.unwrap();

    // Verify it's in Filed status
    let stored = commons_mgr.get_appeal(&appeal_id).await.unwrap().unwrap();
    assert!(matches!(
        stored.status,
        icn_governance::appeal::AppealStatus::Filed { .. }
    ));

    // Begin review
    let under_review = commons_mgr.begin_appeal_review(&appeal_id).await.unwrap();
    assert!(matches!(
        under_review.status,
        icn_governance::appeal::AppealStatus::UnderReview { .. }
    ));

    // Resolve the appeal
    let resolved = commons_mgr
        .resolve_constitutional_appeal(
            &appeal_id,
            AppealOutcome::Upheld {
                reason: "Procedural error confirmed - no notice was given".to_string(),
                remedy: AppealRemedy::Reinstate,
            },
        )
        .await
        .unwrap();

    assert!(matches!(
        resolved.status,
        icn_governance::appeal::AppealStatus::Resolved { .. }
    ));
}

/// Test appeal withdrawal by appellant
#[actix_web::test]
async fn test_appeal_withdrawal() {
    let commons_mgr = CommonsManager::new();

    let appellant = KeyPair::generate().unwrap();
    let appellant_did = appellant.did().clone();

    let appeal = Appeal::new(
        AppealType::Suspension {
            target_id: "holder:123".to_string(),
            suspension_type: "CommonsHolder".to_string(),
        },
        AppealScope::Jurisdiction {
            domain_id: "coop:test".to_string(),
        },
        appellant_did.clone(),
        vec![AppealGrounds::NewEvidence {
            description: "New documentation discovered".to_string(),
        }],
        "I have new evidence".to_string(),
        AppealRemedy::Reverse,
    );

    let appeal_id = appeal.id.clone();
    commons_mgr.store_appeal(appeal).await.unwrap();

    // Withdraw the appeal
    let withdrawn = commons_mgr
        .withdraw_appeal(&appeal_id, &appellant_did, Some("Resolved privately".to_string()))
        .await
        .unwrap();

    assert!(matches!(
        withdrawn.status,
        icn_governance::appeal::AppealStatus::Withdrawn { .. }
    ));
}

/// Test that only appellant can withdraw appeal
#[actix_web::test]
async fn test_appeal_withdrawal_requires_appellant() {
    let commons_mgr = CommonsManager::new();

    let appellant = KeyPair::generate().unwrap();
    let other_user = KeyPair::generate().unwrap();

    let appeal = Appeal::new(
        AppealType::Suspension {
            target_id: "holder:456".to_string(),
            suspension_type: "Member".to_string(),
        },
        AppealScope::Jurisdiction {
            domain_id: "coop:test".to_string(),
        },
        appellant.did().clone(),
        vec![AppealGrounds::ExceededAuthority {
            description: "Decision maker had no authority".to_string(),
        }],
        "Test appeal".to_string(),
        AppealRemedy::Reverse,
    );

    let appeal_id = appeal.id.clone();
    commons_mgr.store_appeal(appeal).await.unwrap();

    // Try to withdraw as different user - should fail
    let result = commons_mgr
        .withdraw_appeal(&appeal_id, other_user.did(), None)
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("appellant"));
}

/// Test listing amendments by status
#[actix_web::test]
async fn test_list_amendments_by_status() {
    let commons_mgr = CommonsManager::new();

    let proposer = KeyPair::generate().unwrap();

    // Create amendments in different states
    let mut draft = Amendment::new(
        AmendmentType::Policy,
        AmendmentScope::Jurisdiction {
            domain_id: "coop:test".to_string(),
        },
        "Draft Amendment".to_string(),
        "Still drafting".to_string(),
        proposer.did().clone(),
    );
    draft.add_change(AmendmentChange {
        target: ChangeTarget::GovernanceRules,
        change_type: ChangeType::Add,
        description: "Draft change".to_string(),
        old_value: None,
        new_value: "new".to_string(),
    });

    let mut submitted = Amendment::new(
        AmendmentType::Charter,
        AmendmentScope::Jurisdiction {
            domain_id: "coop:test".to_string(),
        },
        "Submitted Amendment".to_string(),
        "Ready for review".to_string(),
        proposer.did().clone(),
    );
    submitted.add_change(AmendmentChange {
        target: ChangeTarget::MembershipPolicy,
        change_type: ChangeType::Modify,
        description: "Submitted change".to_string(),
        old_value: Some("old".to_string()),
        new_value: "new".to_string(),
    });

    let submitted_id = submitted.id.clone();

    commons_mgr.store_amendment(draft).await.unwrap();
    commons_mgr.store_amendment(submitted).await.unwrap();

    // Submit the second one
    commons_mgr
        .submit_amendment(&submitted_id, proposer.did())
        .await
        .unwrap();

    // List all amendments
    let all = commons_mgr.list_amendments(None, None, None).await.unwrap();
    assert_eq!(all.len(), 2);

    // List by scope
    let by_scope = commons_mgr
        .list_amendments(None, Some("coop:test"), None)
        .await
        .unwrap();
    assert_eq!(by_scope.len(), 2);
}

/// Test listing appeals by scope
#[actix_web::test]
async fn test_list_appeals_by_scope() {
    let commons_mgr = CommonsManager::new();

    let user1 = KeyPair::generate().unwrap();
    let user2 = KeyPair::generate().unwrap();

    // Create appeals in different jurisdictions
    let appeal1 = Appeal::new(
        AppealType::Suspension {
            target_id: "holder:alpha-1".to_string(),
            suspension_type: "Member".to_string(),
        },
        AppealScope::Jurisdiction {
            domain_id: "coop:alpha".to_string(),
        },
        user1.did().clone(),
        vec![AppealGrounds::ProceduralError {
            description: "Alpha procedural error".to_string(),
        }],
        "Alpha appeal".to_string(),
        AppealRemedy::Reverse,
    );

    let appeal2 = Appeal::new(
        AppealType::MembershipDenial {
            jurisdiction_id: "coop:beta".to_string(),
            application_id: Some("app-beta-1".to_string()),
        },
        AppealScope::Jurisdiction {
            domain_id: "coop:beta".to_string(),
        },
        user2.did().clone(),
        vec![AppealGrounds::RightsViolation {
            right_violated: "Right to fair consideration".to_string(),
        }],
        "Beta appeal".to_string(),
        AppealRemedy::Reinstate,
    );

    commons_mgr.store_appeal(appeal1).await.unwrap();
    commons_mgr.store_appeal(appeal2).await.unwrap();

    // List all appeals
    let all = commons_mgr.list_appeals(None, None, None).await.unwrap();
    assert_eq!(all.len(), 2);

    // List by scope
    let alpha = commons_mgr
        .list_appeals(None, Some("coop:alpha"), None)
        .await
        .unwrap();
    assert_eq!(alpha.len(), 1);

    let beta = commons_mgr
        .list_appeals(None, Some("coop:beta"), None)
        .await
        .unwrap();
    assert_eq!(beta.len(), 1);
}

/// Test different appeal outcomes
#[actix_web::test]
async fn test_appeal_outcomes() {
    let commons_mgr = CommonsManager::new();

    let outcomes = vec![
        AppealOutcome::Upheld {
            reason: "Original decision was wrong".to_string(),
            remedy: AppealRemedy::Reverse,
        },
        AppealOutcome::Denied {
            reason: "Original decision was correct".to_string(),
        },
        AppealOutcome::PartiallyUpheld {
            reason: "Decision was partially wrong".to_string(),
            remedy: AppealRemedy::Modify {
                modification: "Reduce penalty by 50%".to_string(),
            },
        },
        AppealOutcome::Remanded {
            instructions: "Reconsider with new evidence".to_string(),
            remanded_to: "coop:test governance body".to_string(),
        },
    ];

    for (i, outcome) in outcomes.into_iter().enumerate() {
        let appellant = KeyPair::generate().unwrap();

        let appeal = Appeal::new(
            AppealType::GovernanceDecision {
                proposal_id: format!("prop-outcome-{i}"),
                decision: "Rejected".to_string(),
            },
            AppealScope::Jurisdiction {
                domain_id: format!("coop:outcome-test-{i}"),
            },
            appellant.did().clone(),
            vec![AppealGrounds::FactualError {
                description: format!("Factual error in outcome test {i}"),
            }],
            format!("Testing outcome {i}"),
            AppealRemedy::Modify {
                modification: "Test modification".to_string(),
            },
        );

        let appeal_id = appeal.id.clone();
        commons_mgr.store_appeal(appeal).await.unwrap();
        commons_mgr.begin_appeal_review(&appeal_id).await.unwrap();

        let resolved = commons_mgr
            .resolve_constitutional_appeal(&appeal_id, outcome.clone())
            .await
            .unwrap();

        if let icn_governance::appeal::AppealStatus::Resolved {
            outcome: resolved_outcome,
            ..
        } = resolved.status
        {
            // Compare discriminants since outcomes have data
            assert_eq!(
                std::mem::discriminant(&resolved_outcome),
                std::mem::discriminant(&outcome)
            );
        } else {
            panic!("Expected Resolved status");
        }
    }
}
