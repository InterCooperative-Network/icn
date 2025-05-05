use crate::error::{LifecycleError, LifecycleResult};
use crate::types::{
    LineageAttestation, LineageAttestationType, MergeStatus, PreMergeBundle, SplitBundle, SplitStatus,
};
use cid::Cid;
use icn_core_vm::HostContext;
use icn_dag::{DagManager, DagNode, DagNodeType};
use icn_economics::Ledger;
use icn_federation::Federation;
use icn_identity::{Did, create_federation_did, ExecutionReceipt, VerifiableCredential};
use serde_json::json;
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Execute a federation merge operation
pub async fn execute_merge(
    ctx: &mut HostContext,
    bundle: PreMergeBundle,
) -> LifecycleResult<ExecutionReceipt> {
    info!("Executing federation merge operation");
    
    // Verify bundle signatures
    if !bundle.verify_proofs()? {
        return Err(LifecycleError::VerificationFailed(
            "Bundle proof verification failed".to_string(),
        ));
    }
    
    // Extract federation DIDs from bundle
    let parent_a = &bundle.lineage.parents[0];
    let parent_b = &bundle.lineage.parents[1];
    let child_fed = &bundle.lineage.children[0];
    
    debug!("Merging federations: {} and {} into {}", parent_a, parent_b, child_fed);
    
    // Create federation DID if needed
    let federation_did = if child_fed.is_empty() {
        create_federation_did().map_err(|e| {
            LifecycleError::IdentityError(format!("Failed to create federation DID: {}", e))
        })?
    } else {
        child_fed.clone()
    };
    
    // Create Genesis DAG node for new federation
    let genesis_node = create_genesis_node(
        &federation_did,
        &bundle.dag_roots,
        &bundle.metadata,
    ).map_err(|e| {
        LifecycleError::DagAnchoringFailed(format!("Failed to create genesis node: {}", e))
    })?;
    
    // Anchor genesis node to DAG
    let genesis_cid = ctx.dag_manager()
        .add_node(genesis_node)
        .await
        .map_err(|e| {
            LifecycleError::DagAnchoringFailed(format!("Failed to anchor genesis node: {}", e))
        })?;
    
    debug!("Created federation genesis node with CID: {}", genesis_cid);
    
    // Create lineage attestation node
    let lineage_node = create_lineage_node(
        &bundle.lineage,
        &genesis_cid,
    ).map_err(|e| {
        LifecycleError::LineageAttestationError(format!("Failed to create lineage node: {}", e))
    })?;
    
    // Anchor lineage attestation to DAG
    let lineage_cid = ctx.dag_manager()
        .add_node(lineage_node)
        .await
        .map_err(|e| {
            LifecycleError::DagAnchoringFailed(format!("Failed to anchor lineage node: {}", e))
        })?;
    
    debug!("Created lineage attestation node with CID: {}", lineage_cid);
    
    // Create MergeBridge node
    let bridge_node = create_merge_bridge_node(
        &federation_did,
        &bundle.dag_roots,
        &genesis_cid,
        &lineage_cid,
    ).map_err(|e| {
        LifecycleError::DagAnchoringFailed(format!("Failed to create merge bridge node: {}", e))
    })?;
    
    // Anchor bridge node to DAG
    let bridge_cid = ctx.dag_manager()
        .add_node(bridge_node)
        .await
        .map_err(|e| {
            LifecycleError::DagAnchoringFailed(format!("Failed to anchor bridge node: {}", e))
        })?;
    
    debug!("Created merge bridge node with CID: {}", bridge_cid);
    
    // Prepare receipt information
    let receipt_data = json!({
        "operation": "federation_merge",
        "federation_a": parent_a,
        "federation_b": parent_b,
        "new_federation": federation_did,
        "genesis_cid": genesis_cid.to_string(),
        "lineage_cid": lineage_cid.to_string(),
        "bridge_cid": bridge_cid.to_string(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "status": "completed"
    });
    
    // Create execution receipt
    let receipt = ExecutionReceipt {
        id: uuid::Uuid::new_v4().to_string(),
        operation: "federation_merge".to_string(),
        executor: ctx.identity().did().to_string(),
        timestamp: chrono::Utc::now(),
        status: "success".to_string(),
        data: receipt_data.to_string(),
        signature: vec![], // This would be populated in a real implementation
    };
    
    info!("Federation merge execution completed successfully");
    Ok(receipt)
}

/// Execute a federation split operation
pub async fn execute_split(
    ctx: &mut HostContext,
    bundle: SplitBundle,
) -> LifecycleResult<ExecutionReceipt> {
    info!("Executing federation split operation");
    
    // Verify bundle signatures
    if !bundle.verify_proofs()? {
        return Err(LifecycleError::VerificationFailed(
            "Bundle proof verification failed".to_string(),
        ));
    }
    
    // Verify economic consistency
    if !bundle.verify_economic_consistency()? {
        return Err(LifecycleError::EconomicInconsistency(
            "Economic consistency check failed".to_string(),
        ));
    }
    
    // Extract federation DIDs from bundle
    let parent_fed = &bundle.lineage.parents[0];
    let child_a = &bundle.lineage.children[0];
    let child_b = &bundle.lineage.children[1];
    
    debug!("Splitting federation: {} into {} and {}", parent_fed, child_a, child_b);
    
    // Create federation DIDs if needed
    let federation_a_did = if child_a.is_empty() {
        create_federation_did().map_err(|e| {
            LifecycleError::IdentityError(format!("Failed to create federation A DID: {}", e))
        })?
    } else {
        child_a.clone()
    };
    
    let federation_b_did = if child_b.is_empty() {
        create_federation_did().map_err(|e| {
            LifecycleError::IdentityError(format!("Failed to create federation B DID: {}", e))
        })?
    } else {
        child_b.clone()
    };
    
    // Create metadata for both federations
    let metadata_a = HashMap::from([
        ("name".to_string(), format!("{}-A", parent_fed)),
        ("parent".to_string(), parent_fed.to_string()),
        ("split_timestamp".to_string(), chrono::Utc::now().to_rfc3339()),
    ]);
    
    let metadata_b = HashMap::from([
        ("name".to_string(), format!("{}-B", parent_fed)),
        ("parent".to_string(), parent_fed.to_string()),
        ("split_timestamp".to_string(), chrono::Utc::now().to_rfc3339()),
    ]);
    
    // Create Genesis DAG nodes for new federations
    let genesis_node_a = create_genesis_node(
        &federation_a_did,
        &[bundle.parent_root.clone()],
        &metadata_a,
    ).map_err(|e| {
        LifecycleError::DagAnchoringFailed(format!("Failed to create genesis node A: {}", e))
    })?;
    
    let genesis_node_b = create_genesis_node(
        &federation_b_did,
        &[bundle.parent_root.clone()],
        &metadata_b,
    ).map_err(|e| {
        LifecycleError::DagAnchoringFailed(format!("Failed to create genesis node B: {}", e))
    })?;
    
    // Anchor genesis nodes to DAG
    let genesis_cid_a = ctx.dag_manager()
        .add_node(genesis_node_a)
        .await
        .map_err(|e| {
            LifecycleError::DagAnchoringFailed(format!("Failed to anchor genesis node A: {}", e))
        })?;
    
    let genesis_cid_b = ctx.dag_manager()
        .add_node(genesis_node_b)
        .await
        .map_err(|e| {
            LifecycleError::DagAnchoringFailed(format!("Failed to anchor genesis node B: {}", e))
        })?;
    
    debug!("Created federation genesis nodes with CIDs: {} and {}", 
           genesis_cid_a, genesis_cid_b);
    
    // Create lineage attestation node
    let lineage_node = create_lineage_node(
        &bundle.lineage,
        &bundle.parent_root,
    ).map_err(|e| {
        LifecycleError::LineageAttestationError(format!("Failed to create lineage node: {}", e))
    })?;
    
    // Anchor lineage attestation to DAG
    let lineage_cid = ctx.dag_manager()
        .add_node(lineage_node)
        .await
        .map_err(|e| {
            LifecycleError::DagAnchoringFailed(format!("Failed to anchor lineage node: {}", e))
        })?;
    
    debug!("Created lineage attestation node with CID: {}", lineage_cid);
    
    // Create SplitBridge nodes
    let bridge_node_a = create_split_bridge_node(
        &federation_a_did,
        &bundle.parent_root,
        &genesis_cid_a,
        &lineage_cid,
        &bundle.partition_map.members_a,
    ).map_err(|e| {
        LifecycleError::DagAnchoringFailed(format!("Failed to create split bridge node A: {}", e))
    })?;
    
    let bridge_node_b = create_split_bridge_node(
        &federation_b_did,
        &bundle.parent_root,
        &genesis_cid_b,
        &lineage_cid,
        &bundle.partition_map.members_b,
    ).map_err(|e| {
        LifecycleError::DagAnchoringFailed(format!("Failed to create split bridge node B: {}", e))
    })?;
    
    // Anchor bridge nodes to DAG
    let bridge_cid_a = ctx.dag_manager()
        .add_node(bridge_node_a)
        .await
        .map_err(|e| {
            LifecycleError::DagAnchoringFailed(format!("Failed to anchor bridge node A: {}", e))
        })?;
    
    let bridge_cid_b = ctx.dag_manager()
        .add_node(bridge_node_b)
        .await
        .map_err(|e| {
            LifecycleError::DagAnchoringFailed(format!("Failed to anchor bridge node B: {}", e))
        })?;
    
    debug!("Created split bridge nodes with CIDs: {} and {}", 
           bridge_cid_a, bridge_cid_b);
    
    // Prepare receipt information
    let receipt_data = json!({
        "operation": "federation_split",
        "parent_federation": parent_fed,
        "federation_a": federation_a_did,
        "federation_b": federation_b_did,
        "genesis_cid_a": genesis_cid_a.to_string(),
        "genesis_cid_b": genesis_cid_b.to_string(),
        "lineage_cid": lineage_cid.to_string(),
        "bridge_cid_a": bridge_cid_a.to_string(),
        "bridge_cid_b": bridge_cid_b.to_string(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "status": "completed"
    });
    
    // Create execution receipt
    let receipt = ExecutionReceipt {
        id: uuid::Uuid::new_v4().to_string(),
        operation: "federation_split".to_string(),
        executor: ctx.identity().did().to_string(),
        timestamp: chrono::Utc::now(),
        status: "success".to_string(),
        data: receipt_data.to_string(),
        signature: vec![], // This would be populated in a real implementation
    };
    
    info!("Federation split execution completed successfully");
    Ok(receipt)
}

/// Create a genesis DAG node for a new federation
fn create_genesis_node(
    federation_did: &Did,
    parent_roots: &[Cid],
    metadata: &HashMap<String, String>,
) -> Result<DagNode, String> {
    let payload = json!({
        "type": "FederationGenesis",
        "federation_did": federation_did,
        "parent_roots": parent_roots,
        "metadata": metadata,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|e| format!("Failed to serialize genesis payload: {}", e))?;
    
    Ok(DagNode {
        id: "".to_string(), // Will be set when added to DAG
        node_type: DagNodeType::FederationGenesis,
        parent_ids: parent_roots.iter().map(|c| c.to_string()).collect(),
        content: payload_bytes,
        signature: vec![], // Will be set when added to DAG
        issuer: federation_did.to_string(),
        timestamp: chrono::Utc::now(),
    })
}

/// Create a lineage attestation DAG node
fn create_lineage_node(
    lineage: &LineageAttestation,
    parent_cid: &Cid,
) -> Result<DagNode, String> {
    let payload = serde_json::to_vec(lineage)
        .map_err(|e| format!("Failed to serialize lineage attestation: {}", e))?;
    
    let issuer_did = match lineage.typ {
        LineageAttestationType::Merge => lineage.children[0].clone(),
        LineageAttestationType::Split => lineage.parents[0].clone(),
    };
    
    Ok(DagNode {
        id: "".to_string(), // Will be set when added to DAG
        node_type: DagNodeType::LineageAttestation,
        parent_ids: vec![parent_cid.to_string()],
        content: payload,
        signature: vec![], // Will be set when added to DAG
        issuer: issuer_did,
        timestamp: chrono::Utc::now(),
    })
}

/// Create a merge bridge DAG node
fn create_merge_bridge_node(
    federation_did: &Did,
    parent_roots: &[Cid],
    genesis_cid: &Cid,
    lineage_cid: &Cid,
) -> Result<DagNode, String> {
    let payload = json!({
        "type": "MergeBridge",
        "federation_did": federation_did,
        "parent_roots": parent_roots,
        "genesis_cid": genesis_cid.to_string(),
        "lineage_cid": lineage_cid.to_string(),
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|e| format!("Failed to serialize merge bridge payload: {}", e))?;
    
    let parent_ids = vec![
        genesis_cid.to_string(),
        lineage_cid.to_string(),
    ];
    
    Ok(DagNode {
        id: "".to_string(), // Will be set when added to DAG
        node_type: DagNodeType::FederationBridge,
        parent_ids,
        content: payload_bytes,
        signature: vec![], // Will be set when added to DAG
        issuer: federation_did.to_string(),
        timestamp: chrono::Utc::now(),
    })
}

/// Create a split bridge DAG node
fn create_split_bridge_node(
    federation_did: &Did,
    parent_root: &Cid,
    genesis_cid: &Cid,
    lineage_cid: &Cid,
    members: &[Did],
) -> Result<DagNode, String> {
    let payload = json!({
        "type": "SplitBridge",
        "federation_did": federation_did,
        "parent_root": parent_root.to_string(),
        "genesis_cid": genesis_cid.to_string(),
        "lineage_cid": lineage_cid.to_string(),
        "members": members,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|e| format!("Failed to serialize split bridge payload: {}", e))?;
    
    let parent_ids = vec![
        genesis_cid.to_string(),
        lineage_cid.to_string(),
    ];
    
    Ok(DagNode {
        id: "".to_string(), // Will be set when added to DAG
        node_type: DagNodeType::FederationBridge,
        parent_ids,
        content: payload_bytes,
        signature: vec![], // Will be set when added to DAG
        issuer: federation_did.to_string(),
        timestamp: chrono::Utc::now(),
    })
} 