use icn_wallet_sync::compat::{self, WalletDagNode, WalletDagNodeMetadata};
use std::time::{SystemTime, UNIX_EPOCH};
use icn_dag::{DagNode, DagNodeMetadata};
use libipld::Ipld;
use cid::Cid;
use icn_identity::IdentityId;
use std::collections::BTreeMap;

fn main() -> anyhow::Result<()> {
    println!("=== ICN Wallet-Runtime DAG Node Conversion Example ===\n");
    
    // Create a wallet DAG node
    let wallet_node = create_wallet_node();
    println!("Created wallet DAG node with CID: {}", wallet_node.cid);
    
    // Convert wallet node to runtime node
    let runtime_node = compat::wallet_to_runtime(&wallet_node)?;
    println!("Converted to runtime DAG node with CID: {}", runtime_node.cid);
    
    // Convert back to wallet node
    let converted_wallet_node = compat::runtime_to_wallet(&runtime_node)?;
    println!("Converted back to wallet DAG node with CID: {}", converted_wallet_node.cid);
    
    // Verify the conversion preserved data
    assert_eq!(wallet_node.cid, converted_wallet_node.cid);
    assert_eq!(wallet_node.parents, converted_wallet_node.parents);
    assert_eq!(wallet_node.issuer, converted_wallet_node.issuer);
    assert_eq!(wallet_node.signature, converted_wallet_node.signature);
    
    // Create a legacy wallet node
    let legacy_node = compat::wallet_to_legacy(&wallet_node)?;
    println!("Converted to legacy wallet format with ID: {}", legacy_node.id);
    
    // Convert back from legacy format
    let from_legacy = compat::legacy_to_wallet(&legacy_node)?;
    println!("Converted from legacy format back to wallet node with CID: {}", from_legacy.cid);
    
    println!("\nAll conversions completed successfully!");
    Ok(())
}

// Helper function to create a test wallet DAG node
fn create_wallet_node() -> WalletDagNode {
    WalletDagNode {
        cid: "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string(),
        parents: vec!["bafkreiaxnnnb7qz6drrbababuirxx54hlzkrl2yxekizxr6gpceiqdu4i".to_string()],
        issuer: "did:icn:test".to_string(),
        timestamp: UNIX_EPOCH + std::time::Duration::from_secs(1683123456),
        signature: vec![1, 2, 3, 4],
        payload: r#"{"key":"value"}"#.as_bytes().to_vec(),
        metadata: WalletDagNodeMetadata {
            sequence: Some(42),
            scope: Some("test-scope".to_string()),
        },
    }
}

// Helper function to create a test runtime DAG node
fn create_runtime_node() -> DagNode {
    let cid = Cid::try_from("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap();
    let parent_cid = Cid::try_from("bafkreiaxnnnb7qz6drrbababuirxx54hlzkrl2yxekizxr6gpceiqdu4i").unwrap();
    
    let metadata = DagNodeMetadata {
        timestamp: 1683123456,
        sequence: Some(42),
        scope: Some("test-scope".to_string()),
    };
    
    let mut map = BTreeMap::new();
    map.insert("key".to_string(), Ipld::String("value".to_string()));
    
    DagNode {
        cid,
        parents: vec![parent_cid],
        issuer: IdentityId::new("did:icn:test".to_string()),
        signature: vec![1, 2, 3, 4],
        payload: Ipld::Map(map),
        metadata,
    }
} 