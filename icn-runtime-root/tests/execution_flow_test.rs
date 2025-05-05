//! Integration test for the full execution flow
//! 
//! This test demonstrates the complete execution path:
//! CCL → WASM → DAG anchor → ExecutionReceipt

use std::sync::Arc;
use std::path::PathBuf;

use icn_ccl_compiler::{compile, CompilationOptions, OptimizationLevel};
use icn_core_vm::{
    IdentityContext, VMContext, ResourceAuthorization,
    ExecutionReceipt, ExecutionReceiptSubject,
    VerifiableCredential
};
use icn_dag::{DagManager, DagNode, DagNodeType};
use icn_governance_kernel::{GovernanceKernel, Proposal, Vote, VoteChoice, ProposalStatus};
use icn_identity::{IdentityId, IdentityScope, KeyPair};
use icn_storage::AsyncInMemoryStorage;
use icn_execution_tools::derive_authorizations;
use icn_verifier_runtime::VerifierRuntime;

#[tokio::test]
async fn test_full_execution_flow() -> anyhow::Result<()> {
    // Step 1: Initialize the basic components
    let storage = AsyncInMemoryStorage::new();
    let dag_manager = DagManager::new(storage.clone());
    let verifier = VerifierRuntime::new(storage.clone());

    // Step 2: Set up identities
    let admin_keypair = KeyPair::generate_ed25519();
    let admin_id = IdentityId::from_keypair(&admin_keypair, IdentityScope::Individual)?;
    
    let user_keypair = KeyPair::generate_ed25519();
    let user_id = IdentityId::from_keypair(&user_keypair, IdentityScope::Individual)?;
    
    let federation_keypair = KeyPair::generate_ed25519();
    let federation_id = IdentityId::from_keypair(&federation_keypair, IdentityScope::Community)?;

    // Step 3: Compile CCL to WASM
    // Simple CCL for testing
    let ccl_code = r#"
    schema ProposalSchema {
        title: String,
        description: String,
        requested_action: String
    }
    
    rule AddMemberProposal {
        description: "Add a new member to the federation",
        when:
            proposal oftype ProposalSchema
            with proposal.requested_action == "add_member"
        then:
            authorize(invoker, "federation:add_member")
    }
    "#;
    
    // Compile CCL to WASM
    let compilation_options = CompilationOptions {
        optimization_level: OptimizationLevel::Speed,
        ..Default::default()
    };
    
    let wasm_binary = compile(ccl_code, &compilation_options)?;
    println!("Successfully compiled CCL to WASM, size: {} bytes", wasm_binary.len());
    
    // Step 4: Save WASM binary to storage
    let wasm_cid = dag_manager.store_content(&wasm_binary).await?;
    println!("Stored WASM with CID: {}", wasm_cid);
    
    // Step 5: Create a governance kernel with the WASM module
    let governance_config = serde_json::json!({
        "wasm_module_id": wasm_cid,
        "federation_id": federation_id.to_string(),
        "threshold": 0.5,
        "min_voters": 1,
        "voting_period_hours": 24
    });
    
    let governance_kernel = GovernanceKernel::new(
        governance_config.to_string(),
        storage.clone(),
        Some(Arc::new(move |id, _ctx| {
            // Simple identity resolver for testing
            if id == federation_id.to_string() {
                Some(federation_id.to_string())
            } else if id == admin_id.to_string() {
                Some(admin_id.to_string())
            } else if id == user_id.to_string() {
                Some(user_id.to_string())
            } else {
                None
            }
        }))
    )?;
    
    // Step 6: Create a proposal
    let proposal_data = serde_json::json!({
        "title": "Add New Member",
        "description": "Add user1 to the federation",
        "requested_action": "add_member",
        "member_id": user_id.to_string()
    });
    
    let proposal_id = governance_kernel.create_proposal(
        &admin_id.to_string(),
        proposal_data.to_string()
    ).await?;
    println!("Created proposal with ID: {}", proposal_id);
    
    // Step 7: Submit vote on proposal
    let vote = Vote {
        proposal_id: proposal_id.clone(),
        voter: federation_id.to_string(),
        choice: VoteChoice::Approve,
        reason: Some("Approved by federation admin".to_string()),
        timestamp: chrono::Utc::now(),
    };
    
    governance_kernel.submit_vote(&vote).await?;
    println!("Submitted vote from federation admin");
    
    // Step 8: Process the proposal (should now be approved)
    governance_kernel.process_proposals().await?;
    
    // Verify proposal status
    let proposal = governance_kernel.get_proposal(&proposal_id).await?;
    assert_eq!(proposal.status, ProposalStatus::Approved);
    println!("Proposal status: {:?}", proposal.status);
    
    // Step 9: Execute the proposal using VM
    // Create VM context
    let identity_context = IdentityContext {
        invoker: federation_id.to_string(),
        subject: Some(user_id.to_string()),
        federation: Some(federation_id.to_string()),
    };
    
    let vm_context = VMContext::new(
        storage.clone(),
        identity_context,
    );
    
    // Load WASM and execute
    let wasm_module = storage.get_binary(&wasm_cid).await?;
    let execution_result = vm_context.execute_wasm(&wasm_module, &proposal_data.to_string()).await?;
    
    // Step 10: Derive authorizations from execution result
    let authorizations = derive_authorizations(&execution_result)?;
    
    // Verify authorizations
    assert!(authorizations.iter().any(|auth| {
        matches!(auth, ResourceAuthorization { 
            identity_id, 
            resource, 
            .. 
        } if identity_id == &federation_id.to_string() && resource == "federation:add_member")
    }));
    println!("Derived authorizations: {:?}", authorizations);
    
    // Step 11: Create an ExecutionReceipt
    let receipt_subject = ExecutionReceiptSubject::Proposal(proposal_id.clone());
    
    let execution_receipt = ExecutionReceipt::new(
        receipt_subject,
        wasm_cid.clone(),
        proposal_data.to_string(),
        execution_result.clone(),
        authorizations.clone(),
        federation_id.to_string(),
    );
    
    // Step 12: Create a verifiable credential from the receipt
    let credential = VerifiableCredential::from_execution_receipt(
        execution_receipt,
        &federation_keypair,
        None,
    )?;
    
    println!("Created verifiable credential for execution receipt");
    
    // Step 13: Anchor the credential in the DAG
    let credential_json = serde_json::to_string(&credential)?;
    let dag_node = DagNode {
        node_type: DagNodeType::ExecutionReceipt,
        content: credential_json.into_bytes(),
        parents: vec![],
        signer: federation_id.to_string(),
        timestamp: chrono::Utc::now(),
        signature: "".to_string(), // Would be signed in a real scenario
    };
    
    let node_id = dag_manager.add_node(dag_node).await?;
    println!("Anchored execution receipt in DAG with ID: {}", node_id);
    
    // Step 14: Verify the credential using the verifier runtime
    let verification_result = verifier.verify_credential(&credential).await?;
    assert!(verification_result.is_valid, "Credential verification failed");
    println!("Verified credential successfully");
    
    Ok(())
} 