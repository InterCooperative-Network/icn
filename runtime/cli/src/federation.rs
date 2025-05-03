/*!
# Federation CLI Commands

This module provides commands for working with ICN federations:
1. `federation init` - Initialize a new federation
2. `federation status` - Check federation status
3. `federation verify` - Verify federation integrity
*/

use clap::{Args, Subcommand};
use icn_federation::{FederationManager, FederationManagerConfig, TrustBundle, roles::NodeRole};
use icn_identity::{IdentityId, KeyPair, IdentityScope};
use icn_dag::{DagNodeBuilder, DagNode, DagManager};
use icn_dag::audit::{DAGAuditVerifier, VerificationReport, format_report_for_cli};
use icn_storage::{Storage, StorageConfig, FileSystemStorage};
use serde::{Serialize, Deserialize};

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::fs;
use std::io::{self, Write};
use tracing::{info, debug, warn, error};
use anyhow::{Result, anyhow, Context};

/// Command line arguments for federation commands
#[derive(Args, Debug)]
pub struct FederationArgs {
    /// Subcommand for federation operations
    #[clap(subcommand)]
    pub command: FederationCommand,
}

/// Subcommands for federation operations
#[derive(Subcommand, Debug)]
pub enum FederationCommand {
    /// Initialize a new federation
    #[clap(name = "init")]
    Init(FederationInitOptions),
    
    /// Check federation status
    #[clap(name = "status")]
    Status(FederationStatusOptions),
    
    /// Verify federation integrity
    #[clap(name = "verify")]
    Verify(FederationVerifyOptions),
}

/// Options for federation initialization
#[derive(Args, Debug)]
pub struct FederationInitOptions {
    /// Federation name
    #[clap(long)]
    pub name: String,
    
    /// Federation DID (if not provided, will be auto-generated)
    #[clap(long)]
    pub did: Option<String>,
    
    /// Initial nodes (comma-separated list of DIDs)
    #[clap(long)]
    pub nodes: String,
    
    /// Genesis node (DID of the node creating the federation)
    #[clap(long)]
    pub genesis_node: String,
    
    /// Federation config file (TOML format)
    #[clap(long)]
    pub config_file: Option<PathBuf>,
    
    /// Output directory for federation artifacts
    #[clap(long)]
    pub output_dir: PathBuf,
    
    /// Storage directory
    #[clap(long)]
    pub storage_dir: Option<PathBuf>,
}

/// Options for federation status check
#[derive(Args, Debug)]
pub struct FederationStatusOptions {
    /// Federation DID
    #[clap(long)]
    pub federation: String,
    
    /// Storage directory
    #[clap(long)]
    pub storage_dir: Option<PathBuf>,
    
    /// Output format (text or json)
    #[clap(long, default_value = "text")]
    pub output: String,
    
    /// Output file (if not specified, print to stdout)
    #[clap(long)]
    pub output_file: Option<PathBuf>,
}

/// Options for federation verification
#[derive(Args, Debug)]
pub struct FederationVerifyOptions {
    /// Federation DID
    #[clap(long)]
    pub federation: String,
    
    /// Storage directory
    #[clap(long)]
    pub storage_dir: Option<PathBuf>,
    
    /// Whether to verify from genesis (otherwise from latest checkpoint)
    #[clap(long)]
    pub from_genesis: bool,
    
    /// Output format (text or json)
    #[clap(long, default_value = "text")]
    pub output: String,
    
    /// Output file (if not specified, print to stdout)
    #[clap(long)]
    pub output_file: Option<PathBuf>,
}

/// Federation configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    /// Federation name
    pub name: String,
    
    /// Federation DID
    pub did: String,
    
    /// Genesis node DID
    pub genesis_node: String,
    
    /// Initial node list
    pub nodes: Vec<NodeConfig>,
    
    /// Federation type (Cooperative, Community, etc.)
    pub federation_type: String,
    
    /// Federation description
    pub description: Option<String>,
    
    /// Custom parameters
    #[serde(flatten)]
    pub parameters: serde_json::Value,
}

/// Node configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node DID
    pub did: String,
    
    /// Node role
    pub role: String,
    
    /// Node endpoint (optional)
    pub endpoint: Option<String>,
}

/// Federation status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationStatus {
    /// Federation DID
    pub federation_id: String,
    
    /// Federation name
    pub name: Option<String>,
    
    /// Current TrustBundle epoch
    pub current_epoch: u64,
    
    /// Node count
    pub node_count: usize,
    
    /// DAG height
    pub dag_height: u64,
    
    /// Anchor count
    pub anchor_count: u64,
    
    /// Last verified credential
    pub last_credential: Option<String>,
    
    /// Quorum threshold
    pub quorum_threshold: u64,
    
    /// Current quorum
    pub current_quorum: u64,
    
    /// Node health status
    pub node_health: Vec<NodeHealth>,
}

/// Node health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealth {
    /// Node DID
    pub node_id: String,
    
    /// Node role
    pub role: String,
    
    /// Status (Online, Offline, Degraded)
    pub status: String,
    
    /// Last seen timestamp
    pub last_seen: Option<String>,
}

/// Implements logic for initializing a new federation
pub async fn run_init(options: FederationInitOptions) -> Result<()> {
    info!("Initializing federation: {}", options.name);
    
    // 1. Set up storage directory
    let storage_dir = options.storage_dir.unwrap_or_else(|| {
        let mut home = dirs::home_dir()
            .expect("Could not determine home directory");
        home.push(".icn");
        home.push("storage");
        home
    });
    
    fs::create_dir_all(&storage_dir)
        .context("Failed to create storage directory")?;
    
    let storage_config = StorageConfig::new(storage_dir);
    let storage = Arc::new(Mutex::new(
        FileSystemStorage::new(storage_config)?
    ));
    
    // 2. Parse and validate node list
    let nodes: Vec<&str> = options.nodes.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    
    if nodes.is_empty() {
        return Err(anyhow!("No nodes specified"));
    }
    
    if !nodes.contains(&options.genesis_node.as_str()) {
        return Err(anyhow!("Genesis node must be in the node list"));
    }
    
    // 3. Generate or use provided federation DID
    let federation_id = match options.did {
        Some(did) => IdentityId::new(&did),
        None => {
            let federation_did = format!("did:icn:federation:{}", uuid::Uuid::new_v4());
            IdentityId::new(&federation_did)
        }
    };
    
    // 4. Create or load federation config
    let mut federation_config = match options.config_file {
        Some(config_path) => {
            let config_content = fs::read_to_string(&config_path)
                .context("Failed to read config file")?;
            
            toml::from_str::<FederationConfig>(&config_content)
                .context("Failed to parse config file")?
        },
        None => {
            // Create default config
            let node_configs: Vec<NodeConfig> = nodes.iter()
                .map(|&node_id| {
                    let role = if node_id == options.genesis_node {
                        "Validator".to_string()
                    } else {
                        "Validator".to_string()
                    };
                    
                    NodeConfig {
                        did: node_id.to_string(),
                        role,
                        endpoint: None,
                    }
                })
                .collect();
            
            FederationConfig {
                name: options.name.clone(),
                did: federation_id.to_string(),
                genesis_node: options.genesis_node.clone(),
                nodes: node_configs,
                federation_type: "Default".to_string(),
                description: Some("Auto-generated federation".to_string()),
                parameters: serde_json::json!({}),
            }
        }
    };
    
    // Ensure federation DID in config matches
    federation_config.did = federation_id.to_string();
    
    // 5. Create output directory
    fs::create_dir_all(&options.output_dir)
        .context("Failed to create output directory")?;
    
    // 6. Generate trust bundle
    info!("Generating initial TrustBundle...");
    let mut trust_bundle = TrustBundle::new(1);
    trust_bundle.set_federation_id(federation_id.clone());
    
    for node_config in &federation_config.nodes {
        let role = match node_config.role.as_str() {
            "Validator" => NodeRole::Validator,
            "Observer" => NodeRole::Observer,
            "Archiver" => NodeRole::Archiver,
            _ => NodeRole::Observer,
        };
        
        trust_bundle.add_node(IdentityId::new(&node_config.did), role);
    }
    
    // Generate proof (in real implementation, this would be signed)
    trust_bundle.set_proof(vec![1, 2, 3, 4]); // Dummy proof for now
    
    // 7. Create federation genesis DAG payload
    let genesis_payload = serde_json::json!({
        "type": "FederationGenesis",
        "name": federation_config.name,
        "description": federation_config.description,
        "federationType": federation_config.federation_type,
        "genesisNode": federation_config.genesis_node,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "epoch": 1,
        "parameters": federation_config.parameters,
    });
    
    // 8. Save federation artifacts
    // Save config
    let config_path = options.output_dir.join("federation_config.toml");
    let config_toml = toml::to_string_pretty(&federation_config)
        .context("Failed to serialize federation config")?;
    
    fs::write(&config_path, config_toml)
        .context("Failed to write federation config")?;
    
    // Save trust bundle
    let trust_bundle_path = options.output_dir.join("trust_bundle.json");
    let trust_bundle_json = serde_json::to_string_pretty(&trust_bundle)
        .context("Failed to serialize trust bundle")?;
    
    fs::write(&trust_bundle_path, trust_bundle_json)
        .context("Failed to write trust bundle")?;
    
    // Save genesis payload
    let genesis_path = options.output_dir.join("genesis_payload.json");
    let genesis_json = serde_json::to_string_pretty(&genesis_payload)
        .context("Failed to serialize genesis payload")?;
    
    fs::write(&genesis_path, genesis_json)
        .context("Failed to write genesis payload")?;
    
    // 9. Print success message and instructions
    println!("Federation '{}' successfully initialized!", options.name);
    println!("Federation DID: {}", federation_id);
    println!("Artifacts saved to: {}", options.output_dir.display());
    println!("\nTo start the federation, run:");
    println!("  1. Initialize the genesis node using the generated artifacts");
    println!("  2. Start additional nodes and connect them to the genesis node");
    println!("  3. Verify the federation status with 'federation status --federation {}'", 
             federation_id);
    
    Ok(())
}

/// Implements logic for checking federation status
pub async fn run_status(options: FederationStatusOptions) -> Result<()> {
    info!("Checking status for federation: {}", options.federation);
    
    // 1. Set up storage directory
    let storage_dir = options.storage_dir.unwrap_or_else(|| {
        let mut home = dirs::home_dir()
            .expect("Could not determine home directory");
        home.push(".icn");
        home.push("storage");
        home
    });
    
    let storage_config = StorageConfig::new(storage_dir);
    let storage = Arc::new(Mutex::new(
        FileSystemStorage::new(storage_config)?
    ));
    
    // 2. Create federation manager
    let config = FederationManagerConfig::default();
    let keypair = KeyPair::new(vec![1, 2, 3], vec![4, 5, 6]); // Dummy key for CLI
    
    let federation_manager = FederationManager::new(
        config,
        storage.clone(),
        keypair,
    ).await?;
    
    // 3. Get federation status information
    let federation_id = IdentityId::new(&options.federation);
    
    // Get latest trust bundle
    let latest_epoch = federation_manager.get_latest_known_epoch().await
        .context("Failed to get latest epoch")?;
    
    let trust_bundle = federation_manager.get_trust_bundle(latest_epoch).await
        .context("Failed to get trust bundle")?;
    
    // Get node health
    let mut node_health = Vec::new();
    for node in &trust_bundle.nodes {
        let health_status = federation_manager.get_node_health(&node.did).await;
        
        let status = match health_status {
            Ok(true) => "Online".to_string(),
            Ok(false) => "Degraded".to_string(),
            Err(_) => "Offline".to_string(),
        };
        
        let role = match node.role {
            NodeRole::Validator => "Validator",
            NodeRole::Observer => "Observer",
            NodeRole::Archiver => "Archiver",
        };
        
        node_health.push(NodeHealth {
            node_id: node.did.to_string(),
            role: role.to_string(),
            status,
            last_seen: None, // In a real implementation, this would be from the health check
        });
    }
    
    // Get DAG info
    let dag_manager = DagManager::new(storage.clone());
    let dag_stats = dag_manager.get_dag_stats(&federation_id.to_string()).await
        .unwrap_or_default();
    
    // Create the federation status
    let status = FederationStatus {
        federation_id: federation_id.to_string(),
        name: None, // In real implementation, get from federation metadata
        current_epoch: latest_epoch,
        node_count: trust_bundle.nodes.len(),
        dag_height: dag_stats.height,
        anchor_count: dag_stats.anchor_count,
        last_credential: dag_stats.last_credential_cid.map(|c| c.to_string()),
        quorum_threshold: trust_bundle.quorum_threshold(),
        current_quorum: node_health.iter().filter(|n| n.status == "Online").count() as u64,
        node_health,
    };
    
    // 4. Output status
    match options.output.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&status)?;
            match &options.output_file {
                Some(path) => {
                    fs::write(path, json)?;
                    info!("Federation status written to: {}", path.display());
                },
                None => {
                    println!("{}", json);
                }
            }
        },
        _ => {
            // Format as text
            let mut output = String::new();
            output.push_str(&format!("=== FEDERATION STATUS REPORT ===\n"));
            output.push_str(&format!("Federation: {}\n", status.federation_id));
            output.push_str(&format!("Current epoch: {}\n", status.current_epoch));
            output.push_str(&format!("Node count: {}\n", status.node_count));
            output.push_str(&format!("DAG height: {}\n", status.dag_height));
            output.push_str(&format!("Anchor count: {}\n", status.anchor_count));
            output.push_str(&format!("Last credential: {}\n", 
                status.last_credential.as_deref().unwrap_or("None")));
            output.push_str(&format!("Quorum: {}/{}\n", 
                status.current_quorum, status.quorum_threshold));
            
            output.push_str("\n=== NODE HEALTH ===\n");
            for (i, node) in status.node_health.iter().enumerate() {
                output.push_str(&format!("{}. {} ({}) - {}\n", 
                    i + 1, node.node_id, node.role, node.status));
            }
            
            match &options.output_file {
                Some(path) => {
                    fs::write(path, output)?;
                    info!("Federation status written to: {}", path.display());
                },
                None => {
                    print!("{}", output);
                }
            }
        }
    }
    
    Ok(())
}

/// Implements logic for verifying federation integrity
pub async fn run_verify(options: FederationVerifyOptions) -> Result<()> {
    info!("Verifying federation: {} (from_genesis={})", 
         options.federation, options.from_genesis);
    
    // 1. Set up storage directory
    let storage_dir = options.storage_dir.unwrap_or_else(|| {
        let mut home = dirs::home_dir()
            .expect("Could not determine home directory");
        home.push(".icn");
        home.push("storage");
        home
    });
    
    let storage_config = StorageConfig::new(storage_dir);
    let storage = Arc::new(Mutex::new(
        FileSystemStorage::new(storage_config)?
    ));
    
    // 2. Create DAG audit verifier
    let mut verifier = DAGAuditVerifier::new(storage.clone());
    
    // 3. Run verification
    info!("Starting DAG verification...");
    let verification_result = verifier.verify_entity_dag(&options.federation)
        .await
        .context("DAG verification failed")?;
    
    // 4. Check TrustBundle and credentials
    let federation_manager = FederationManager::new(
        FederationManagerConfig::default(),
        storage.clone(),
        KeyPair::new(vec![1, 2, 3], vec![4, 5, 6]), // Dummy key for CLI
    ).await?;
    
    let latest_epoch = federation_manager.get_latest_known_epoch().await
        .context("Failed to get latest epoch")?;
    
    let trust_bundle = federation_manager.get_trust_bundle(latest_epoch).await
        .context("Failed to get trust bundle")?;
    
    let credential_verification = federation_manager.verify_credential_consistency().await
        .unwrap_or(false);
    
    // 5. Output verification result
    match options.output.as_str() {
        "json" => {
            // Convert to combined result with trust bundle info
            let combined_result = serde_json::json!({
                "dag_verification": verification_result,
                "trust_bundle": {
                    "epoch": latest_epoch,
                    "node_count": trust_bundle.nodes.len(),
                    "quorum_threshold": trust_bundle.quorum_threshold(),
                    "federation_id": trust_bundle.federation_id,
                },
                "credential_verification": credential_verification,
            });
            
            let json = serde_json::to_string_pretty(&combined_result)?;
            match &options.output_file {
                Some(path) => {
                    fs::write(path, json)?;
                    info!("Verification report written to: {}", path.display());
                },
                None => {
                    println!("{}", json);
                }
            }
        },
        _ => {
            // Format as text
            let mut output = format_report_for_cli(&verification_result);
            
            // Add trust bundle and credential info
            output.push_str("\n=== TRUST BUNDLE VERIFICATION ===\n");
            output.push_str(&format!("Current epoch: {}\n", latest_epoch));
            output.push_str(&format!("Node count: {}\n", trust_bundle.nodes.len()));
            output.push_str(&format!("Quorum threshold: {}\n", trust_bundle.quorum_threshold()));
            
            output.push_str("\n=== CREDENTIAL VERIFICATION ===\n");
            output.push_str(&format!("Credential consistency: {}\n", 
                if credential_verification { "VERIFIED" } else { "FAILED" }));
            
            match &options.output_file {
                Some(path) => {
                    fs::write(path, output)?;
                    info!("Verification report written to: {}", path.display());
                },
                None => {
                    print!("{}", output);
                }
            }
        }
    }
    
    Ok(())
} 