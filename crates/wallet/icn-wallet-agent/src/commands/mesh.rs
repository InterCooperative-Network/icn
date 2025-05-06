use anyhow::{anyhow, Result};
use chrono::Utc;
use cid::Cid;
use clap::{Args, Subcommand};
use serde_json::{json, Value};
use tracing::{debug, error, info};
use std::fs;
use std::path::PathBuf;

use icn_wallet_types::MeshPolicy;
use mesh_types::{
    MeshPolicyFragment,
    ReputationParamsFragment,
    RewardSettingsFragment,
    BondingRequirementsFragment,
    CapabilityScopeFragment,
    SchedulingParamsFragment,
    VerificationQuorumFragment,
};

/// Mesh Compute Commands
#[derive(Debug, Args)]
pub struct MeshCommands {
    #[clap(subcommand)]
    pub command: MeshSubcommand,
}

/// Subcommands for Mesh Compute
#[derive(Debug, Subcommand)]
pub enum MeshSubcommand {
    /// Policy subcommands
    #[clap(subcommand)]
    Policy(PolicySubcommand),
    
    /// Compute task subcommands (publish, list, etc.)
    #[clap(subcommand)]
    Task(TaskSubcommand),
}

/// Policy subcommands
#[derive(Debug, Subcommand)]
pub enum PolicySubcommand {
    /// View the current active policy for a federation
    View {
        /// Federation DID (defaults to the wallet's federation)
        #[clap(long)]
        federation_did: Option<String>,
    },
    
    /// Propose a policy update
    Propose {
        /// Federation DID (defaults to the wallet's federation)
        #[clap(long)]
        federation_did: Option<String>,
        
        /// JSON file containing the policy update fragment
        #[clap(long)]
        update_file: PathBuf,
        
        /// Description of the update
        #[clap(long)]
        description: String,
    },
    
    /// List policy proposals
    List {
        /// Federation DID (defaults to the wallet's federation)
        #[clap(long)]
        federation_did: Option<String>,
        
        /// Show all proposals, including inactive ones
        #[clap(long)]
        all: bool,
    },
    
    /// Vote on a policy proposal
    Vote {
        /// Policy CID to vote on
        #[clap(long)]
        policy_cid: String,
        
        /// Vote approval (yes/no)
        #[clap(long)]
        approve: bool,
    },
}

/// Task subcommands
#[derive(Debug, Subcommand)]
pub enum TaskSubcommand {
    /// Publish a compute task
    Publish {
        /// WASM module CID
        #[clap(long)]
        wasm_cid: String,
        
        /// Input data CID
        #[clap(long)]
        input_cid: String,
        
        /// Fee for execution (in tokens)
        #[clap(long)]
        fee: u64,
        
        /// Number of verifiers required
        #[clap(long, default_value = "3")]
        verifiers: u32,
        
        /// Memory required (MB)
        #[clap(long, default_value = "128")]
        mem_mb: u32,
        
        /// CPU cycles required
        #[clap(long, default_value = "1000000")]
        cpu_cycles: u32,
        
        /// GPU FLOPS required (0 for no GPU)
        #[clap(long, default_value = "0")]
        gpu_flops: u32,
        
        /// I/O bandwidth required (MB)
        #[clap(long, default_value = "50")]
        io_mb: u32,
    },
    
    /// List active compute tasks
    List {
        /// Show only tasks published by this wallet
        #[clap(long)]
        mine: bool,
        
        /// Show tasks in specific state (all, pending, running, completed, failed)
        #[clap(long, default_value = "all")]
        state: String,
    },
}

/// Handle mesh policy view command
pub async fn handle_policy_view(
    federation_did: Option<String>,
) -> Result<()> {
    // Get the federation DID
    let federation_did = match federation_did {
        Some(did) => did,
        None => get_current_federation_did().await?,
    };
    
    // Get the active policy
    let active_policy = get_active_policy(&federation_did).await?;
    
    // Format and print the policy
    println!("Active Mesh Policy for federation {}", federation_did);
    println!("Version: {}", active_policy.policy_version);
    println!("Activation: {}", active_policy.activation_timestamp);
    println!();
    println!("== Resource Parameters ==");
    println!("Min Fee: {}", active_policy.min_fee);
    println!("Base Capability Scope:");
    println!("  Memory: {} MB", active_policy.base_capability_scope.mem_mb);
    println!("  CPU Cycles: {}", active_policy.base_capability_scope.cpu_cycles);
    println!("  GPU FLOPS: {}", active_policy.base_capability_scope.gpu_flops);
    println!("  I/O Bandwidth: {} MB", active_policy.base_capability_scope.io_mb);
    println!();
    println!("== Reputation Parameters ==");
    println!("Alpha (execution weight): {}", active_policy.alpha);
    println!("Beta (verification weight): {}", active_policy.beta);
    println!("Gamma (penalty factor): {}", active_policy.gamma);
    println!("Lambda (decay rate): {}", active_policy.lambda);
    println!("Stake Weight: {}", active_policy.stake_weight);
    println!();
    println!("== Reward Settings ==");
    println!("Worker: {}%", active_policy.reward_settings.worker_percentage);
    println!("Verifiers: {}%", active_policy.reward_settings.verifier_percentage);
    println!("Platform: {}%", active_policy.reward_settings.platform_fee_percentage);
    println!("Use Reputation Weighting: {}", active_policy.reward_settings.use_reputation_weighting);
    println!();
    println!("== Bonding Requirements ==");
    println!("Min Stake: {}", active_policy.bonding_requirements.min_stake_amount);
    println!("Min Lock Period: {} days", active_policy.bonding_requirements.min_lock_period_days);
    println!("Allowed Token Types: {:?}", active_policy.bonding_requirements.allowed_token_types);
    println!();
    println!("== Verification Quorum ==");
    println!("Required Percentage: {}%", active_policy.verification_quorum.required_percentage);
    println!("Min Verifiers: {}", active_policy.verification_quorum.minimum_verifiers);
    println!("Max Verifiers: {}", active_policy.verification_quorum.maximum_verifiers);
    println!("Timeout: {} minutes", active_policy.verification_quorum.verification_timeout_minutes);
    
    Ok(())
}

/// Handle mesh policy propose command
pub async fn handle_policy_propose(
    federation_did: Option<String>,
    update_file: PathBuf,
    description: String,
) -> Result<()> {
    // Get the federation DID
    let federation_did = match federation_did {
        Some(did) => did,
        None => get_current_federation_did().await?,
    };
    
    // Read the update file
    let update_json = fs::read_to_string(update_file)
        .map_err(|e| anyhow!("Failed to read update file: {}", e))?;
    
    // Parse the JSON
    let update_value: Value = serde_json::from_str(&update_json)
        .map_err(|e| anyhow!("Failed to parse update JSON: {}", e))?;
    
    // Convert to MeshPolicyFragment
    let mut fragment = parse_policy_fragment(update_value, description)?;
    
    // Get the proposer's DID
    let proposer_did = get_wallet_did()?;
    fragment.proposer_did = proposer_did;
    
    // Get the active policy CID
    let active_policy_cid = get_active_policy_cid(&federation_did).await?;
    
    // Construct the mesh-policy-update CCL transaction
    println!("Preparing to submit policy update proposal:");
    println!("Federation: {}", federation_did);
    println!("Previous Policy CID: {}", active_policy_cid);
    println!("Description: {}", fragment.description);
    println!();
    println!("Changes:");
    
    // Print the changes
    if let Some(rep) = &fragment.reputation_params {
        println!("Reputation Parameters:");
        if let Some(alpha) = rep.alpha {
            println!("  Alpha: {}", alpha);
        }
        if let Some(beta) = rep.beta {
            println!("  Beta: {}", beta);
        }
        if let Some(gamma) = rep.gamma {
            println!("  Gamma: {}", gamma);
        }
        if let Some(lambda) = rep.lambda {
            println!("  Lambda: {}", lambda);
        }
    }
    
    if let Some(reward) = &fragment.reward_settings {
        println!("Reward Settings:");
        if let Some(worker) = reward.worker_percentage {
            println!("  Worker: {}%", worker);
        }
        if let Some(verifier) = reward.verifier_percentage {
            println!("  Verifiers: {}%", verifier);
        }
        if let Some(platform) = reward.platform_fee_percentage {
            println!("  Platform: {}%", platform);
        }
    }
    
    if let Some(min_fee) = fragment.min_fee {
        println!("Min Fee: {}", min_fee);
    }
    
    // Confirm submission
    println!();
    println!("Submit this proposal? [y/N]");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    
    if input.trim().to_lowercase() != "y" {
        return Err(anyhow!("Proposal submission cancelled"));
    }
    
    // Submit the proposal using the governance system
    submit_policy_proposal(&active_policy_cid, &fragment, &federation_did).await?;
    
    println!("Policy update proposal submitted successfully!");
    
    Ok(())
}

/// Handle mesh policy list command
pub async fn handle_policy_list(
    federation_did: Option<String>,
    all: bool,
) -> Result<()> {
    // Get the federation DID
    let federation_did = match federation_did {
        Some(did) => did,
        None => get_current_federation_did().await?,
    };
    
    // Get the list of policy proposals
    let proposals = list_policy_proposals(&federation_did, all).await?;
    
    // Print the proposals
    println!("Mesh Policy Proposals for federation {}:", federation_did);
    println!("{:<10} {:<10} {:<20} {:<40}", "Version", "Status", "Proposer", "Description");
    println!("{:-<80}", "");
    
    for proposal in proposals {
        println!(
            "{:<10} {:<10} {:<20} {:<40}",
            proposal.version,
            proposal.status,
            proposal.proposer_did,
            proposal.description
        );
    }
    
    Ok(())
}

/// Handle mesh policy vote command
pub async fn handle_policy_vote(
    policy_cid: String,
    approve: bool,
) -> Result<()> {
    // Parse the policy CID
    let cid = Cid::try_from(policy_cid)
        .map_err(|e| anyhow!("Invalid CID format: {}", e))?;
    
    // Get the voter's DID
    let voter_did = get_wallet_did()?;
    
    // Submit the vote
    record_policy_vote(&voter_did, &cid, approve).await?;
    
    println!("Vote successfully recorded as {}", if approve { "approved" } else { "rejected" });
    
    // Check if the proposal has reached quorum
    if has_approval_quorum(&cid).await? {
        println!("This proposal has reached approval quorum and can now be activated");
    }
    
    Ok(())
}

/// Handle mesh task publish command
pub async fn handle_task_publish(
    wasm_cid: String,
    input_cid: String,
    fee: u64,
    verifiers: u32,
    mem_mb: u32,
    cpu_cycles: u32,
    gpu_flops: u32,
    io_mb: u32,
) -> Result<()> {
    // Parse CIDs
    let wasm_cid = Cid::try_from(wasm_cid)
        .map_err(|e| anyhow!("Invalid WASM CID format: {}", e))?;
    let input_cid = Cid::try_from(input_cid)
        .map_err(|e| anyhow!("Invalid input CID format: {}", e))?;
    
    // Create capability scope
    let capability_scope = mesh_types::CapabilityScope {
        mem_mb,
        cpu_cycles,
        gpu_flops,
        io_mb,
    };
    
    // Call the mesh integration function to publish the task
    let response = mesh_types::publish_computation_task(
        &wasm_cid,
        &input_cid,
        fee,
        verifiers,
        capability_scope,
        24, // 24 hour expiry by default
    ).await?;
    
    println!("Computation task published successfully!");
    println!("Task CID: {}", response); // In a real implementation, we'd get the task CID
    
    Ok(())
}

/// Handle mesh task list command
pub async fn handle_task_list(
    mine: bool,
    state: String,
) -> Result<()> {
    // Get the wallet's DID if filtering by mine
    let filter_did = if mine {
        Some(get_wallet_did()?)
    } else {
        None
    };
    
    // List the tasks
    let tasks = list_compute_tasks(filter_did.as_deref(), &state).await?;
    
    // Print the tasks
    println!("Mesh Compute Tasks:");
    println!("{:<10} {:<20} {:<30} {:<15}", "Status", "Publisher", "CID", "Fee");
    println!("{:-<80}", "");
    
    for task in tasks {
        println!(
            "{:<10} {:<20} {:<30} {:<15}",
            task.status,
            task.publisher_did,
            task.cid,
            task.fee
        );
    }
    
    Ok(())
}

/// Helper function to get the current federation DID from the wallet
async fn get_current_federation_did() -> Result<String> {
    // In a real implementation, this would query the wallet's current federation
    // For now, just return a placeholder
    Ok("did:icn:federation:default".to_string())
}

/// Helper function to get the wallet's DID
fn get_wallet_did() -> Result<String> {
    // In a real implementation, this would get the wallet's DID
    // For now, just return a placeholder
    Ok("did:icn:wallet:default".to_string())
}

/// Helper function to get the active mesh policy
async fn get_active_policy(federation_did: &str) -> Result<MeshPolicy> {
    // In a real implementation, this would query the active policy
    // For now, just return a placeholder default policy
    Ok(mesh_types::MeshPolicy::new_default(federation_did))
}

/// Helper function to get the active policy CID
async fn get_active_policy_cid(federation_did: &str) -> Result<Cid> {
    // In a real implementation, this would query the active policy CID
    // For now, just return a placeholder
    Ok(Cid::default())
}

/// Helper function to parse a policy fragment from JSON
fn parse_policy_fragment(json: Value, description: String) -> Result<MeshPolicyFragment> {
    // Start with an empty fragment
    let mut fragment = MeshPolicyFragment {
        reputation_params: None,
        stake_weight: None,
        min_fee: None,
        base_capability_scope: None,
        reward_settings: None,
        bonding_requirements: None,
        scheduling_params: None,
        verification_quorum: None,
        description,
        proposer_did: "".to_string(), // Will be filled in later
    };
    
    // Parse reputation parameters
    if let Some(reputation) = json.get("reputation_params") {
        if reputation.is_object() {
            let mut params = ReputationParamsFragment {
                alpha: None,
                beta: None,
                gamma: None,
                lambda: None,
            };
            
            if let Some(alpha) = reputation.get("alpha").and_then(|v| v.as_f64()) {
                params.alpha = Some(alpha);
            }
            if let Some(beta) = reputation.get("beta").and_then(|v| v.as_f64()) {
                params.beta = Some(beta);
            }
            if let Some(gamma) = reputation.get("gamma").and_then(|v| v.as_f64()) {
                params.gamma = Some(gamma);
            }
            if let Some(lambda) = reputation.get("lambda").and_then(|v| v.as_f64()) {
                params.lambda = Some(lambda);
            }
            
            fragment.reputation_params = Some(params);
        }
    }
    
    // Parse reward settings
    if let Some(reward) = json.get("reward_settings") {
        if reward.is_object() {
            let mut settings = RewardSettingsFragment {
                worker_percentage: None,
                verifier_percentage: None,
                platform_fee_percentage: None,
                use_reputation_weighting: None,
                platform_fee_address: None,
            };
            
            if let Some(worker) = reward.get("worker_percentage").and_then(|v| v.as_u64()) {
                settings.worker_percentage = Some(worker as u8);
            }
            if let Some(verifier) = reward.get("verifier_percentage").and_then(|v| v.as_u64()) {
                settings.verifier_percentage = Some(verifier as u8);
            }
            if let Some(platform) = reward.get("platform_fee_percentage").and_then(|v| v.as_u64()) {
                settings.platform_fee_percentage = Some(platform as u8);
            }
            if let Some(weighting) = reward.get("use_reputation_weighting").and_then(|v| v.as_bool()) {
                settings.use_reputation_weighting = Some(weighting);
            }
            if let Some(address) = reward.get("platform_fee_address").and_then(|v| v.as_str()) {
                settings.platform_fee_address = Some(address.to_string());
            }
            
            fragment.reward_settings = Some(settings);
        }
    }
    
    // Parse simple parameters
    if let Some(stake_weight) = json.get("stake_weight").and_then(|v| v.as_f64()) {
        fragment.stake_weight = Some(stake_weight);
    }
    if let Some(min_fee) = json.get("min_fee").and_then(|v| v.as_u64()) {
        fragment.min_fee = Some(min_fee);
    }
    
    // Parse capability scope
    if let Some(scope) = json.get("base_capability_scope") {
        if scope.is_object() {
            let mut cap_scope = CapabilityScopeFragment {
                mem_mb: None,
                cpu_cycles: None,
                gpu_flops: None,
                io_mb: None,
            };
            
            if let Some(mem) = scope.get("mem_mb").and_then(|v| v.as_u64()) {
                cap_scope.mem_mb = Some(mem as u32);
            }
            if let Some(cpu) = scope.get("cpu_cycles").and_then(|v| v.as_u64()) {
                cap_scope.cpu_cycles = Some(cpu as u32);
            }
            if let Some(gpu) = scope.get("gpu_flops").and_then(|v| v.as_u64()) {
                cap_scope.gpu_flops = Some(gpu as u32);
            }
            if let Some(io) = scope.get("io_mb").and_then(|v| v.as_u64()) {
                cap_scope.io_mb = Some(io as u32);
            }
            
            fragment.base_capability_scope = Some(cap_scope);
        }
    }
    
    // Parse bonding requirements
    if let Some(bonding) = json.get("bonding_requirements") {
        if bonding.is_object() {
            let mut requirements = BondingRequirementsFragment {
                min_stake_amount: None,
                min_lock_period_days: None,
                allowed_token_types: None,
            };
            
            if let Some(min_stake) = bonding.get("min_stake_amount").and_then(|v| v.as_u64()) {
                requirements.min_stake_amount = Some(min_stake);
            }
            if let Some(lock_period) = bonding.get("min_lock_period_days").and_then(|v| v.as_u64()) {
                requirements.min_lock_period_days = Some(lock_period as u32);
            }
            if let Some(tokens) = bonding.get("allowed_token_types").and_then(|v| v.as_array()) {
                let token_types = tokens.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>();
                if !token_types.is_empty() {
                    requirements.allowed_token_types = Some(token_types);
                }
            }
            
            fragment.bonding_requirements = Some(requirements);
        }
    }
    
    // Parse scheduling parameters
    if let Some(scheduling) = json.get("scheduling_params") {
        if scheduling.is_object() {
            let mut params = SchedulingParamsFragment {
                use_fair_queuing: None,
                max_queue_length: None,
                reputation_priority_boost: None,
                task_timeout_minutes: None,
                default_capability_scope: None,
            };
            
            if let Some(fair_queue) = scheduling.get("use_fair_queuing").and_then(|v| v.as_bool()) {
                params.use_fair_queuing = Some(fair_queue);
            }
            if let Some(queue_limit) = scheduling.get("max_queue_length").and_then(|v| v.as_u64()) {
                params.max_queue_length = Some(queue_limit as u32);
            }
            if let Some(priority) = scheduling.get("reputation_priority_boost").and_then(|v| v.as_f64()) {
                params.reputation_priority_boost = Some(priority);
            }
            if let Some(timeout) = scheduling.get("task_timeout_minutes").and_then(|v| v.as_u64()) {
                params.task_timeout_minutes = Some(timeout as u32);
            }
            
            fragment.scheduling_params = Some(params);
        }
    }
    
    // Parse verification quorum
    if let Some(quorum) = json.get("verification_quorum") {
        if quorum.is_object() {
            let mut q = VerificationQuorumFragment {
                required_percentage: None,
                minimum_verifiers: None,
                maximum_verifiers: None,
                verification_timeout_minutes: None,
            };
            
            if let Some(req) = quorum.get("required_percentage").and_then(|v| v.as_u64()) {
                q.required_percentage = Some(req as u8);
            }
            if let Some(min) = quorum.get("minimum_verifiers").and_then(|v| v.as_u64()) {
                q.minimum_verifiers = Some(min as u32);
            }
            if let Some(max) = quorum.get("maximum_verifiers").and_then(|v| v.as_u64()) {
                q.maximum_verifiers = Some(max as u32);
            }
            if let Some(timeout) = quorum.get("verification_timeout_minutes").and_then(|v| v.as_u64()) {
                q.verification_timeout_minutes = Some(timeout as u32);
            }
            
            fragment.verification_quorum = Some(q);
        }
    }
    
    Ok(fragment)
}

/// Helper function to submit a policy proposal
async fn submit_policy_proposal(
    active_policy_cid: &Cid,
    fragment: &MeshPolicyFragment,
    federation_did: &str,
) -> Result<()> {
    // In a real implementation, this would submit the CCL policy update transaction
    // For now, just log that it would be submitted
    info!("Would submit policy update proposal: {} -> {:?} for {}", 
          active_policy_cid, fragment, federation_did);
    Ok(())
}

/// Helper function to list policy proposals
async fn list_policy_proposals(
    federation_did: &str,
    all: bool,
) -> Result<Vec<PolicyProposal>> {
    // In a real implementation, this would query the DAG for policy proposals
    // For now, just return a placeholder list
    let proposals = vec![
        PolicyProposal {
            cid: Cid::default(),
            version: 2,
            status: "Active".to_string(),
            proposer_did: "did:icn:member:proposer1".to_string(),
            description: "Initial policy update".to_string(),
            timestamp: Utc::now(),
        },
        PolicyProposal {
            cid: Cid::default(),
            version: 3,
            status: "Pending".to_string(),
            proposer_did: "did:icn:member:proposer2".to_string(),
            description: "Increase worker rewards".to_string(),
            timestamp: Utc::now(),
        },
    ];
    
    Ok(proposals)
}

/// Helper function to record a policy vote
async fn record_policy_vote(
    voter_did: &str,
    policy_cid: &Cid,
    approve: bool,
) -> Result<()> {
    // In a real implementation, this would submit a vote transaction
    // For now, just log that it would be submitted
    info!("Would record policy vote: {} -> {} for {}", 
          voter_did, approve, policy_cid);
    Ok(())
}

/// Helper function to check if a policy proposal has approval quorum
async fn has_approval_quorum(policy_cid: &Cid) -> Result<bool> {
    // In a real implementation, this would check votes in the DAG
    // For now, just return a placeholder
    Ok(true)
}

/// Helper function to list compute tasks
async fn list_compute_tasks(
    publisher_did: Option<&str>,
    state: &str,
) -> Result<Vec<ComputeTask>> {
    // In a real implementation, this would query the mesh network for tasks
    // For now, just return a placeholder list
    let tasks = vec![
        ComputeTask {
            cid: Cid::default(),
            publisher_did: "did:icn:wallet:default".to_string(),
            status: "Running".to_string(),
            fee: 100,
        },
        ComputeTask {
            cid: Cid::default(),
            publisher_did: "did:icn:federation:member".to_string(),
            status: "Completed".to_string(),
            fee: 50,
        },
    ];
    
    // Filter by publisher if specified
    let tasks = if let Some(did) = publisher_did {
        tasks.into_iter()
            .filter(|t| t.publisher_did == did)
            .collect()
    } else {
        tasks
    };
    
    // Filter by state if not "all"
    let tasks = if state != "all" {
        tasks.into_iter()
            .filter(|t| t.status.to_lowercase() == state.to_lowercase())
            .collect()
    } else {
        tasks
    };
    
    Ok(tasks)
}

/// Policy proposal information
#[derive(Debug, Clone)]
struct PolicyProposal {
    /// Content ID of the proposal
    cid: Cid,
    
    /// Policy version
    version: u32,
    
    /// Current status (Active, Pending, Rejected)
    status: String,
    
    /// Proposer DID
    proposer_did: String,
    
    /// Description of the update
    description: String,
    
    /// Timestamp
    timestamp: chrono::DateTime<Utc>,
}

/// Compute task information
#[derive(Debug, Clone)]
struct ComputeTask {
    /// Content ID of the task
    cid: Cid,
    
    /// Publisher DID
    publisher_did: String,
    
    /// Current status
    status: String,
    
    /// Fee offered
    fee: u64,
} 