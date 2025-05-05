//! Federation Lifecycle Management for the Intercooperative Network
//! 
//! This crate provides functionality for federation merges and splits,
//! including trust mapping, economic ledger adjustments, and DAG anchoring.

pub mod types;
pub mod error;
pub mod bundle;
pub mod executor;
pub mod economics;

pub use types::{
    LineageAttestation, LineageAttestationType, MergeProposal, SplitProposal,
    PreMergeBundle, SplitBundle, QuorumConfig, PartitionMap, ResourceAllocation,
    MergeProcess, SplitProcess, MergeStatus, SplitStatus, TrustMapping, CredentialValidation,
};
pub use error::{LifecycleError, LifecycleResult};
pub use bundle::{
    create_trust_mapping, create_merged_governance_policy,
    create_merged_trust_bundle, create_split_trust_bundle,
};
pub use executor::{execute_merge, execute_split};
pub use economics::{union_ledgers_impl, shard_ledger_impl, create_transfer_plan};

/// Creates a federation merge process from two source federations
pub async fn initiate_federation_merge(
    federation_a: &icn_federation::Federation,
    federation_b: &icn_federation::Federation,
    merge_proposal: &MergeProposal,
) -> LifecycleResult<MergeProcess> {
    // Re-export of the core implementation from bundle.rs
    use crate::bundle::create_trust_mapping;
    use crate::bundle::create_merged_governance_policy;
    use crate::bundle::create_merged_trust_bundle;
    use uuid::Uuid;
    use chrono::Utc;

    // Verify federation approvals
    verify_federation_approval(federation_a, merge_proposal)?;
    verify_federation_approval(federation_b, merge_proposal)?;
    
    // Create trust mapping
    let federation_a_members = federation_a.members().iter().map(|m| m.did().clone()).collect::<Vec<_>>();
    let federation_b_members = federation_b.members().iter().map(|m| m.did().clone()).collect::<Vec<_>>();
    
    let trust_mapping = create_trust_mapping(
        &federation_a_members,
        &federation_b_members,
        &merge_proposal.src_fed_a,
    )?;
    
    // Create merged governance policy
    let merged_policy = create_merged_governance_policy(
        &federation_a.policies(),
        &federation_b.policies(),
    )?;
    
    // Get active trust bundles
    let bundle_a = PreMergeBundle {
        dag_roots: vec![federation_a.genesis_cid()],
        metadata: federation_a.metadata().clone(),
        lineage: LineageAttestation {
            parents: vec![federation_a.id().clone()],
            children: vec![merge_proposal.src_fed_a.clone()],
            typ: LineageAttestationType::Merge,
            proof: merge_proposal.approval_a.clone().unwrap_or_default(),
            timestamp: Utc::now(),
            metadata: std::collections::HashMap::new(),
        },
        proofs: vec![merge_proposal.approval_a.clone().unwrap_or_default()],
    };
    
    let bundle_b = PreMergeBundle {
        dag_roots: vec![federation_b.genesis_cid()],
        metadata: federation_b.metadata().clone(),
        lineage: LineageAttestation {
            parents: vec![federation_b.id().clone()],
            children: vec![merge_proposal.src_fed_b.clone()],
            typ: LineageAttestationType::Merge,
            proof: merge_proposal.approval_b.clone().unwrap_or_default(),
            timestamp: Utc::now(),
            metadata: std::collections::HashMap::new(),
        },
        proofs: vec![merge_proposal.approval_b.clone().unwrap_or_default()],
    };
    
    // Create merged trust bundle
    let merged_bundle = create_merged_trust_bundle(
        &bundle_a,
        &bundle_b,
        &merge_proposal.src_fed_a,
    )?;
    
    // Create merge process
    let merge_process = MergeProcess {
        id: Uuid::new_v4().to_string(),
        federation_a_id: federation_a.id().clone(),
        federation_b_id: federation_b.id().clone(),
        new_federation_id: merge_proposal.src_fed_a.clone(),
        merge_proposal: merge_proposal.clone(),
        trust_mapping,
        merged_policy,
        merged_bundle,
        status: MergeStatus::Initiated,
        start_time: Utc::now(),
        completion_time: None,
    };
    
    Ok(merge_process)
}

/// Creates a federation split process from a parent federation
pub async fn initiate_federation_split(
    original_federation: &icn_federation::Federation,
    split_proposal: &SplitProposal,
) -> LifecycleResult<SplitProcess> {
    // Re-export of the core implementation from bundle.rs
    use crate::bundle::create_trust_mapping;
    use crate::bundle::create_split_trust_bundle;
    use uuid::Uuid;
    use chrono::Utc;
    
    // Verify federation approval
    verify_federation_approval(original_federation, split_proposal)?;
    
    // Get partition map
    let partition_map = get_partition_map(split_proposal)?;
    
    // Create trust mappings for the two federations
    let federation_members = original_federation.members().iter().map(|m| m.did().clone()).collect::<Vec<_>>();
    
    let trust_mapping_a = create_trust_mapping(
        &partition_map.members_a,
        &federation_members,
        &split_proposal.federation_a_id.clone().unwrap_or_default(),
    )?;
    
    let trust_mapping_b = create_trust_mapping(
        &partition_map.members_b,
        &federation_members,
        &split_proposal.federation_b_id.clone().unwrap_or_default(),
    )?;
    
    // Create governance policies for both federations
    let policy_a = original_federation.policies().clone();
    let policy_b = original_federation.policies().clone();
    
    // Create split bundles
    let base_bundle = SplitBundle {
        parent_root: original_federation.genesis_cid(),
        partition_map: partition_map.clone(),
        lineage: LineageAttestation {
            parents: vec![original_federation.id().clone()],
            children: vec![
                split_proposal.federation_a_id.clone().unwrap_or_default(),
                split_proposal.federation_b_id.clone().unwrap_or_default(),
            ],
            typ: LineageAttestationType::Split,
            proof: split_proposal.approval.clone().unwrap_or_default(),
            timestamp: Utc::now(),
            metadata: std::collections::HashMap::new(),
        },
        proofs: vec![split_proposal.approval.clone().unwrap_or_default()],
    };
    
    // Create individual bundles for each federation
    let bundle_a = create_split_trust_bundle(
        &base_bundle,
        &split_proposal.federation_a_id.clone().unwrap_or_default(),
        &partition_map.members_a,
    )?;
    
    let bundle_b = create_split_trust_bundle(
        &base_bundle,
        &split_proposal.federation_b_id.clone().unwrap_or_default(),
        &partition_map.members_b,
    )?;
    
    // Create split process
    let split_process = SplitProcess {
        id: Uuid::new_v4().to_string(),
        original_federation_id: original_federation.id().clone(),
        federation_a_id: split_proposal.federation_a_id.clone().unwrap_or_default(),
        federation_b_id: split_proposal.federation_b_id.clone().unwrap_or_default(),
        split_proposal: split_proposal.clone(),
        trust_mapping_a,
        trust_mapping_b,
        policy_a,
        policy_b,
        bundle_a,
        bundle_b,
        status: SplitStatus::Initiated,
        start_time: Utc::now(),
        completion_time: None,
    };
    
    Ok(split_process)
}

/// Verify federation approvals for proposals
fn verify_federation_approval(
    federation: &icn_federation::Federation,
    proposal: &impl std::fmt::Debug,
) -> LifecycleResult<()> {
    // This is a placeholder for actual verification logic
    // In a real implementation, this would verify quorum signatures
    // against the federation's authorized signers
    
    Ok(())
}

/// Extract and validate partition map from a split proposal
fn get_partition_map(
    split_proposal: &SplitProposal,
) -> LifecycleResult<PartitionMap> {
    // In a real implementation, this would retrieve the partition map
    // from the CID referenced in the proposal and validate it
    
    // For now, return a placeholder partition map
    Ok(PartitionMap {
        members_a: vec![],
        members_b: vec![],
        resources_a: std::collections::HashMap::new(),
        resources_b: std::collections::HashMap::new(),
        ledger_a: std::collections::HashMap::new(),
        ledger_b: std::collections::HashMap::new(),
    })
} 