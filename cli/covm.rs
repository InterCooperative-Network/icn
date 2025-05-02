/*!
# ICN Runtime CLI

This is the main entry point for the ICN Runtime command-line interface.
It uses clap to define subcommands for interacting with the runtime.
*/

use clap::{Parser, Subcommand};
use tracing_subscriber;
use uuid;

#[derive(Parser)]
#[clap(
    name = "covm",
    about = "ICN Runtime (CoVM V3) command-line interface",
    version,
    author = "ICN Cooperative"
)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
    
    #[clap(short, long, help = "Verbose output")]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Propose a new action using a CCL template
    #[clap(name = "propose")]
    Propose {
        /// Path to the CCL template
        #[clap(long, short = 't')]
        ccl_template: String,
        
        /// Path to the DSL input parameters
        #[clap(long, short = 'i')]
        dsl_input: String,
        
        /// Identity to use for signing the proposal
        #[clap(long, short = 'k')]
        identity: String,
    },
    
    /// Vote on a proposal
    #[clap(name = "vote")]
    Vote {
        /// Proposal ID
        #[clap(long, short = 'p')]
        proposal_id: String,
        
        /// Vote (approve/reject)
        #[clap(long, short = 'v')]
        vote: String,
        
        /// Reason for the vote
        #[clap(long, short = 'r')]
        reason: String,
        
        /// Identity to use for signing the vote
        #[clap(long, short = 'k')]
        identity: String,
    },
    
    /// Register a new identity
    #[clap(name = "identity")]
    Identity {
        /// Scope of the identity (coop, community, individual)
        #[clap(long, short = 's')]
        scope: String,
        
        /// Name of the identity
        #[clap(long, short = 'n')]
        name: String,
    },
    
    /// Execute a proposal with a given constitution
    #[clap(name = "execute")]
    Execute {
        /// Path to the WASM proposal payload
        #[clap(long, short = 'p')]
        proposal_payload: String,
        
        /// Path to the governing CCL constitution
        #[clap(long, short = 'c')]
        constitution: String,
        
        /// Identity DID to use as caller
        #[clap(long, short = 'i')]
        identity: String,
        
        /// Identity scope (Cooperative, Community, Individual)
        #[clap(long, short = 's')]
        scope: String,
        
        /// Optional proposal ID (CID)
        #[clap(long)]
        proposal_id: Option<String>,
    },
}

// Add this to handle the execute command
async fn handle_execute_command(
    proposal_payload: String,
    constitution: String,
    identity: String,
    scope: String, 
    proposal_id: Option<String>,
    verbose: bool,
) -> anyhow::Result<()> {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    use cid::Cid;
    use icn_governance_kernel::CclInterpreter;
    use icn_core_vm::{execute_wasm, VmContext};
    use icn_identity::IdentityScope;
    
    // Read the WASM proposal payload
    let wasm_bytes = fs::read(&proposal_payload)
        .map_err(|e| anyhow::anyhow!("Failed to read proposal payload: {}", e))?;
    
    // Read the CCL constitution
    let ccl_content = fs::read_to_string(&constitution)
        .map_err(|e| anyhow::anyhow!("Failed to read constitution: {}", e))?;
    
    // Parse the identity scope
    let identity_scope = match scope.to_lowercase().as_str() {
        "cooperative" => IdentityScope::Cooperative,
        "community" => IdentityScope::Community,
        "individual" => IdentityScope::Individual,
        _ => return Err(anyhow::anyhow!("Invalid scope: {}", scope)),
    };
    
    // Create CCL interpreter
    let interpreter = CclInterpreter::new();
    
    // Interpret the CCL content
    let governance_config = interpreter.interpret_ccl(&ccl_content, identity_scope)
        .map_err(|e| anyhow::anyhow!("CCL interpretation failed: {}", e))?;
    
    if verbose {
        println!("Successfully interpreted constitution. Template: {}:{}", 
            governance_config.template_type, governance_config.template_version);
    }
    
    // Create a timestamp for execution
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    
    // Create an execution ID
    let execution_id = format!("exec-{}", timestamp);
    
    // Parse the proposal ID if provided
    let proposal_cid = if let Some(id_str) = proposal_id {
        Some(Cid::try_from(id_str)
            .map_err(|e| anyhow::anyhow!("Invalid proposal CID: {}", e))?)
    } else {
        None
    };
    
    // Generate resource authorizations based on the governance config
    let (authorized_resources, active_authorizations) = derive_authorizations(
        &governance_config,
        &identity,
        identity_scope,
        timestamp,
        verbose
    );
    
    if verbose {
        println!("Generated {} resource authorizations from governance config", 
                 active_authorizations.len());
    }
    
    // Create the VM context
    let vm_context = VmContext::with_authorizations(
        identity.clone(),
        identity_scope,
        authorized_resources,
        active_authorizations,
        execution_id,
        timestamp,
        proposal_cid,
    );
    
    // Create a simple identity context for execution
    let identity_ctx = create_identity_context(identity.as_str());
    
    // Create a simple in-memory storage backend
    let storage = create_in_memory_storage();
    
    // Execute the WASM with the prepared context
    let result = execute_wasm(&wasm_bytes, vm_context, storage, identity_ctx).await
        .map_err(|e| anyhow::anyhow!("WASM execution failed: {}", e))?;
    
    // Print the execution result
    println!("Execution result:");
    println!("  Success: {}", result.success);
    println!("  Logs: {}", result.logs.join("\n    "));
    
    // Print consumed resources
    println!("  Resources consumed:");
    for (resource_type, amount) in &result.resources_consumed {
        println!("    {:?}: {}", resource_type, amount);
    }
    
    // If there's output data, print it
    if let Some(output) = result.output_data {
        if let Ok(output_str) = String::from_utf8(output.clone()) {
            println!("  Output: {}", output_str);
        } else {
            println!("  Output: {:?}", output);
        }
    }
    
    Ok(())
}

/// Derive resource authorizations from the governance config
/// 
/// This function analyzes the governance config to determine what resource authorizations
/// should be granted for the execution.
/// 
/// # Arguments
/// * `config` - The governance config
/// * `caller_did` - The DID of the caller
/// * `scope` - The identity scope
/// * `timestamp` - The current timestamp
/// * `verbose` - Whether to print verbose output
/// 
/// # Returns
/// A tuple of (resource_types, authorizations)
pub fn derive_authorizations(
    config: &icn_governance_kernel::config::GovernanceConfig,
    caller_did: &str,
    scope: icn_identity::IdentityScope,
    timestamp: i64,
    verbose: bool
) -> (Vec<icn_economics::ResourceType>, Vec<icn_economics::ResourceAuthorization>) {
    use icn_economics::{ResourceType, ResourceAuthorization};
    use uuid::Uuid;
    
    let mut resource_types = Vec::new();
    let mut authorizations = Vec::new();
    
    // Default expiry time (1 hour from now)
    let expiry = Some(timestamp + 3600);
    
    // System DID for authorizations
    let system_did = "did:icn:system:governance".to_string();
    
    // Default resource amounts
    let default_compute = 1_000_000;
    let default_storage = 500_000;
    let default_network = 200_000;
    let default_memory = 100_000;
    
    // Base resource types granted to all executions
    resource_types.push(ResourceType::Compute);
    
    // Grant compute authorization
    authorizations.push(ResourceAuthorization {
        auth_id: Uuid::new_v4(),
        grantor_did: system_did.clone(),
        grantee_did: caller_did.to_string(),
        resource_type: ResourceType::Compute,
        authorized_amount: default_compute,
        consumed_amount: 0,
        scope,
        expiry_timestamp: expiry,
        metadata: None,
    });
    
    // Analyze config sections to determine additional authorizations
    
    // If governance section exists, grant additional compute
    if let Some(governance) = &config.governance {
        // Add additional compute authorization for governance operations
        let governance_compute = match config.template_type.as_str() {
            "coop_bylaws" | "community_charter" => 500_000, // More compute for full governance templates
            "resolution" => 300_000,                         // Medium for resolutions
            _ => 200_000,                                   // Base level for other templates
        };
        
        if verbose {
            println!("  Granting additional compute ({}) for governance section", governance_compute);
        }
        
        // Add extra amounts to existing authorizations
        for auth in &mut authorizations {
            if matches!(auth.resource_type, ResourceType::Compute) {
                auth.authorized_amount += governance_compute;
            }
        }
        
        // If roles are defined, grant permissions based on roles
        if let Some(roles) = &governance.roles {
            // Look for specific permissions in roles that would grant additional resource types
            for role in roles {
                for permission in &role.permissions {
                    match permission.as_str() {
                        "manage_working_groups" | "create_proposals" | "administrate" => {
                            // Administrative roles get more resources
                            if !resource_types.contains(&ResourceType::Storage) {
                                resource_types.push(ResourceType::Storage.clone());
                                authorizations.push(ResourceAuthorization {
                                    auth_id: Uuid::new_v4(),
                                    grantor_did: system_did.clone(),
                                    grantee_did: caller_did.to_string(),
                                    resource_type: ResourceType::Storage,
                                    authorized_amount: default_storage,
                                    consumed_amount: 0,
                                    scope,
                                    expiry_timestamp: expiry,
                                    metadata: None,
                                });
                                
                                if verbose {
                                    println!("  Granting storage authorization for administrative role: {}", role.name);
                                }
                            }
                        },
                        "moderate_content" | "facilitate_meetings" => {
                            // Moderation roles get network bandwidth
                            if !resource_types.contains(&ResourceType::NetworkBandwidth) {
                                resource_types.push(ResourceType::NetworkBandwidth.clone());
                                authorizations.push(ResourceAuthorization {
                                    auth_id: Uuid::new_v4(),
                                    grantor_did: system_did.clone(),
                                    grantee_did: caller_did.to_string(),
                                    resource_type: ResourceType::NetworkBandwidth,
                                    authorized_amount: default_network,
                                    consumed_amount: 0,
                                    scope,
                                    expiry_timestamp: expiry,
                                    metadata: None,
                                });
                                
                                if verbose {
                                    println!("  Granting network bandwidth for moderation role: {}", role.name);
                                }
                            }
                        },
                        _ => {
                            // Default roles get basic permissions already granted
                        }
                    }
                }
            }
        }
    }
    
    // If economic model exists, grant storage permission
    if let Some(economic_model) = &config.economic_model {
        if !resource_types.contains(&ResourceType::Storage) {
            resource_types.push(ResourceType::Storage.clone());
            authorizations.push(ResourceAuthorization {
                auth_id: Uuid::new_v4(),
                grantor_did: system_did.clone(),
                grantee_did: caller_did.to_string(),
                resource_type: ResourceType::Storage,
                authorized_amount: default_storage,
                consumed_amount: 0,
                scope,
                expiry_timestamp: expiry,
                metadata: None,
            });
            
            if verbose {
                println!("  Granting storage authorization for economic model");
            }
        }
        
        // If compensation policy exists, add labor hours resource
        if let Some(compensation) = &economic_model.compensation_policy {
            if let Some(hourly_rates) = &compensation.hourly_rates {
                for (skill, _rate) in hourly_rates {
                    let labor_resource = ResourceType::LaborHours { 
                        skill: skill.clone() 
                    };
                    resource_types.push(labor_resource.clone());
                    authorizations.push(ResourceAuthorization {
                        auth_id: Uuid::new_v4(),
                        grantor_did: system_did.clone(),
                        grantee_did: caller_did.to_string(),
                        resource_type: labor_resource,
                        authorized_amount: 40,  // 40 hours default
                        consumed_amount: 0,
                        scope,
                        expiry_timestamp: expiry,
                        metadata: None,
                    });
                    
                    if verbose {
                        println!("  Granting labor hours authorization for skill: {}", skill);
                    }
                }
            }
        }
    }
    
    // If working groups exist, grant additional resources
    if let Some(working_groups) = &config.working_groups {
        if let Some(resource_allocation) = &working_groups.resource_allocation {
            // Grant memory resources for working groups
            let custom_memory = ResourceType::Custom { 
                identifier: "Memory".to_string() 
            };
            resource_types.push(custom_memory.clone());
            authorizations.push(ResourceAuthorization {
                auth_id: Uuid::new_v4(),
                grantor_did: system_did.clone(),
                grantee_did: caller_did.to_string(),
                resource_type: custom_memory,
                authorized_amount: default_memory,
                consumed_amount: 0,
                scope,
                expiry_timestamp: expiry,
                metadata: None,
            });
            
            // If there's a default budget, use it to inform authorization amounts
            if let Some(budget) = resource_allocation.default_budget {
                // Grant community credit resources based on budget
                if budget > 0 {
                    let community_credit = ResourceType::CommunityCredit { 
                        community_did: caller_did.to_string() 
                    };
                    resource_types.push(community_credit.clone());
                    authorizations.push(ResourceAuthorization {
                        auth_id: Uuid::new_v4(),
                        grantor_did: system_did.clone(),
                        grantee_did: caller_did.to_string(),
                        resource_type: community_credit,
                        authorized_amount: budget,
                        consumed_amount: 0,
                        scope,
                        expiry_timestamp: expiry,
                        metadata: None,
                    });
                    
                    if verbose {
                        println!("  Granting community credit authorization with budget: {}", budget);
                    }
                }
            }
        }
    }
    
    // If dispute resolution exists, grant network bandwidth
    if let Some(_dispute) = &config.dispute_resolution {
        if !resource_types.contains(&ResourceType::NetworkBandwidth) {
            resource_types.push(ResourceType::NetworkBandwidth.clone());
            authorizations.push(ResourceAuthorization {
                auth_id: Uuid::new_v4(),
                grantor_did: system_did.clone(),
                grantee_did: caller_did.to_string(),
                resource_type: ResourceType::NetworkBandwidth,
                authorized_amount: default_network,
                consumed_amount: 0,
                scope,
                expiry_timestamp: expiry,
                metadata: None,
            });
            
            if verbose {
                println!("  Granting network bandwidth for dispute resolution");
            }
        }
    }
    
    // Ensure any template gets minimal resources
    if resource_types.len() <= 1 {
        // Add minimal storage and network for any template
        resource_types.push(ResourceType::Storage.clone());
        authorizations.push(ResourceAuthorization {
            auth_id: Uuid::new_v4(),
            grantor_did: system_did.clone(),
            grantee_did: caller_did.to_string(),
            resource_type: ResourceType::Storage,
            authorized_amount: default_storage / 2,  // Half the default
            consumed_amount: 0,
            scope,
            expiry_timestamp: expiry,
            metadata: None,
        });
        
        if verbose {
            println!("  Granting minimal storage for basic template");
        }
    }
    
    // Always ensure Storage is included regardless of template
    // This is crucial for all operations
    if !resource_types.contains(&ResourceType::Storage) {
        resource_types.push(ResourceType::Storage.clone());
        authorizations.push(ResourceAuthorization {
            auth_id: Uuid::new_v4(),
            grantor_did: system_did.clone(),
            grantee_did: caller_did.to_string(),
            resource_type: ResourceType::Storage,
            authorized_amount: default_storage / 2,  // Half the default for minimal templates
            consumed_amount: 0,
            scope,
            expiry_timestamp: expiry,
            metadata: None,
        });
        
        if verbose {
            println!("  Granting required basic storage");
        }
    }
    
    // TODO(V3-MVP): Implement more sophisticated authorization derivation logic based on detailed config rules
    // (e.g., roles defined in membership), potentially requiring storage lookups for token balances
    // or existing credentials.
    
    (resource_types, authorizations)
}

// Helper function to create an identity context
fn create_identity_context(did: &str) -> std::sync::Arc<icn_core_vm::IdentityContext> {
    use std::sync::Arc;
    use icn_identity::{IdentityId, generate_did_keypair};
    
    // Generate a keypair for the DID
    // In a real-world scenario, we'd load an existing keypair
    // For now, we'll generate a new one even though the DIDs won't match
    let (_did_str, keypair) = generate_did_keypair().expect("Failed to generate keypair");
    
    // Create and return the identity context
    Arc::new(icn_core_vm::IdentityContext {
        keypair,
        did: IdentityId::new(did),
    })
}

// Helper function to create an in-memory storage backend
fn create_in_memory_storage() -> std::sync::Arc<futures::lock::Mutex<dyn icn_storage::StorageBackend + Send + Sync>> {
    use std::sync::Arc;
    use futures::lock::Mutex;
    use icn_storage::AsyncInMemoryStorage;
    
    // Create and return the storage backend
    Arc::new(Mutex::new(AsyncInMemoryStorage::new()))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .init();
    
    // Parse command-line arguments
    let cli = Cli::parse();
    
    // Set verbose mode if requested
    if cli.verbose {
        println!("Verbose mode enabled");
    }
    
    // Execute the requested command
    match cli.command {
        Commands::Propose { ccl_template, dsl_input, identity } => {
            println!("Proposing with template: {}, input: {}, identity: {}", 
                    ccl_template, dsl_input, identity);
            Ok(())
        },
        Commands::Vote { proposal_id, vote, reason, identity } => {
            println!("Voting {} on proposal: {} with reason: {}, identity: {}", 
                    vote, proposal_id, reason, identity);
            Ok(())
        },
        Commands::Identity { scope, name } => {
            println!("Registering identity with scope: {}, name: {}", scope, name);
            Ok(())
        },
        Commands::Execute { proposal_payload, constitution, identity, scope, proposal_id } => {
            handle_execute_command(
                proposal_payload, 
                constitution, 
                identity, 
                scope, 
                proposal_id,
                cli.verbose
            ).await
        },
    }
} 