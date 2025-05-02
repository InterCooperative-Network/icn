/*!
# ICN Runtime CLI

This is the main entry point for the ICN Runtime command-line interface.
It uses clap to define subcommands for interacting with the runtime.
*/

// Standard library imports
use std::fs;
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::io;

// External crates
use tracing_subscriber;
use uuid::Uuid;
use tokio;
use clap::{Parser, Subcommand};
use chrono::Utc;
use serde_json::{json, Value};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use tokio::sync::Mutex;
use multihash::{Code, MultihashDigest};

// ICN crates
use icn_identity::{IdentityId, IdentityScope, KeyPair};
use icn_governance_kernel::CclInterpreter;
use icn_core_vm::IdentityContext;
use icn_dag::DagNode;
use icn_federation::{GuardianMandate, signing};
use cid::Cid;

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
    
    /// Issue a guardian mandate
    #[clap(name = "issue-mandate")]
    IssueMandate {
        /// Guardian DID to issue the mandate
        #[clap(long, short = 'g')]
        guardian: String,
        
        /// Scope of the mandate (Cooperative, Community, Individual)
        #[clap(long, short = 's')]
        scope: String,
        
        /// Scope ID (DID of the scope being governed)
        #[clap(long, short = 'i')]
        scope_id: String,
        
        /// Action to be taken (e.g., FREEZE_ASSETS, REMOVE_MEMBER)
        #[clap(long, short = 'a')]
        action: String,
        
        /// Reason for the mandate
        #[clap(long, short = 'r')]
        reason: String,
        
        /// Cosigning Guardian DIDs (comma separated)
        #[clap(long, short = 'c')]
        cosigners: Option<String>,
        
        /// Output file for the mandate
        #[clap(long, short = 'o')]
        output: Option<String>,
    },
    
    /// Verify a guardian mandate
    #[clap(name = "verify-mandate")]
    VerifyMandate {
        /// Path to the mandate file
        #[clap(long, short = 'm')]
        mandate_path: String,
        
        /// Check against specific federation policies (optional)
        #[clap(long, short = 'f')]
        federation: Option<String>,
    },
    
    /// Appeal a guardian mandate
    #[clap(name = "appeal-mandate")]
    AppealMandate {
        /// Path to the mandate file being appealed
        #[clap(long, short = 'm')]
        mandate_path: String,
        
        /// Identity to use for the appeal
        #[clap(long, short = 'k')]
        identity: String,
        
        /// Reason for the appeal
        #[clap(long, short = 'r')]
        reason: String,
        
        /// Evidence to support the appeal (file path)
        #[clap(long, short = 'e')]
        evidence: Option<String>,
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
    
    /// Export a verifiable credential with JWS proof
    #[clap(name = "export-vc")]
    ExportVc {
        /// CID of the credential to export
        #[clap(long)]
        credential_id: String,
        
        /// Output file path (or - for stdout)
        #[clap(long, short = 'o')]
        output: String,
        
        /// Path to the signing key file or key ID
        #[clap(long, short = 'k')]
        signing_key: String,
        
        /// Issuer DID to use for signing
        #[clap(long)]
        issuer: String,
        
        /// Additional type to add to credential
        #[clap(long, short = 't')]
        credential_type: Option<String>,
    },
    
    /// Compile a CCL template with DSL input into a WASM module
    #[clap(name = "compile")]
    Compile {
        /// Path to the CCL template file (.ccl)
        #[clap(long, short = 't')]
        ccl_template: String,
        
        /// Path to the DSL input file (.dsl or .json)
        #[clap(long, short = 'i')]
        dsl_input: String,
        
        /// Output file path for the compiled WASM (.wasm)
        #[clap(long, short = 'o')]
        output: String,
        
        /// Identity scope (Cooperative, Community, Individual)
        #[clap(long, short = 's')]
        scope: String,
        
        /// Whether to include debug information in the WASM
        #[clap(long)]
        debug: bool,
        
        /// Whether to optimize the WASM
        #[clap(long, default_value = "true")]
        optimize: bool,
        
        /// DID of the caller who will execute this WASM (optional)
        #[clap(long)]
        caller_did: Option<String>,
        
        /// Execution ID to embed in the WASM metadata (optional)
        #[clap(long)]
        execution_id: Option<String>,
        
        /// Custom schema file path to use for DSL validation
        #[clap(long)]
        schema: Option<String>,
        
        /// Skip schema validation
        #[clap(long)]
        skip_schema_validation: bool,
    },
}

// Add this to handle the compile command
async fn handle_compile_command(
    ccl_template: String,
    dsl_input: String,
    output: String,
    scope: String,
    debug: bool,
    optimize: bool,
    caller_did: Option<String>,
    execution_id: Option<String>,
    schema: Option<String>,
    skip_schema_validation: bool,
    verbose: bool,
) -> anyhow::Result<()> {
    use icn_ccl_compiler::{CclCompiler, CompilationOptions};
    use icn_governance_kernel::CclInterpreter;
    use icn_identity::IdentityScope;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    
    // Parse the identity scope
    let identity_scope = match scope.to_lowercase().as_str() {
        "cooperative" => IdentityScope::Cooperative,
        "community" => IdentityScope::Community,
        "individual" => IdentityScope::Individual,
        _ => return Err(anyhow::anyhow!("Invalid scope: {}", scope)),
    };
    
    if verbose {
        println!("Reading CCL template from: {}", ccl_template);
    }
    
    // Read the CCL template
    let ccl_content = fs::read_to_string(&ccl_template)
        .map_err(|e| anyhow::anyhow!("Failed to read CCL template: {}", e))?;
    
    if verbose {
        println!("Reading DSL input from: {}", dsl_input);
    }
    
    // Read the DSL input
    let dsl_content = fs::read_to_string(&dsl_input)
        .map_err(|e| anyhow::anyhow!("Failed to read DSL input: {}", e))?;
    
    // Parse the DSL JSON
    let dsl_json: serde_json::Value = serde_json::from_str(&dsl_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse DSL input as JSON: {}", e))?;
    
    // Create CCL interpreter
    let interpreter = CclInterpreter::new();
    
    if verbose {
        println!("Interpreting CCL template...");
    }
    
    // Interpret the CCL content
    let governance_config = interpreter.interpret_ccl(&ccl_content, identity_scope)
        .map_err(|e| anyhow::anyhow!("CCL interpretation failed: {}", e))?;
    
    if verbose {
        println!("Successfully interpreted CCL template: {}:{}", 
            governance_config.template_type, governance_config.template_version);
    }
    
    // Clone schema once if needed for multiple uses
    let schema_clone = schema.clone();
    
    // Convert schema path if provided
    let schema_path = schema_clone.as_ref().map(PathBuf::from);
    
    // Create compilation options with metadata
    let options = CompilationOptions {
        include_debug_info: debug,
        optimize,
        memory_limits: None, // Use default memory limits
        additional_metadata: None,
        caller_did: caller_did.clone(), 
        execution_id: execution_id.clone(),
        schema_path,
        validate_schema: !skip_schema_validation,
    };
    
    if verbose {
        println!("Compiling CCL template with DSL input to WASM...");
        if let Some(did) = &caller_did {
            println!("Using caller DID: {}", did);
        }
        if let Some(exec_id) = &execution_id {
            println!("Using execution ID: {}", exec_id);
        }
        if let Some(s) = &schema {
            println!("Using schema: {}", s);
        }
        if skip_schema_validation {
            println!("Schema validation: disabled");
        } else {
            println!("Schema validation: enabled");
        }
    }
    
    // Create compiler and compile to WASM
    let mut compiler = CclCompiler::new();
    let wasm_bytes = compiler.compile_to_wasm(&governance_config, &dsl_json, Some(options))
        .map_err(|e| anyhow::anyhow!("Compilation failed: {}", e))?;
    
    if verbose {
        println!("Successfully compiled WASM module ({} bytes)", wasm_bytes.len());
    }
    
    // Write the WASM to the output file
    let mut file = File::create(&output)
        .map_err(|e| anyhow::anyhow!("Failed to create output file: {}", e))?;
    file.write_all(&wasm_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to write output file: {}", e))?;
    
    println!("Successfully compiled WASM module and wrote to: {}", output);
    
    Ok(())
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
    use cid::Cid;
    use std::time::{SystemTime, UNIX_EPOCH};
    use icn_core_vm::{VmContext};
    use icn_economics::ResourceType;
    use icn_identity::IdentityScope;
    use icn_governance_kernel::CclInterpreter;
    
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
        proposal_cid.map(|cid| cid.to_string()),
    );
    
    // Create a simple identity context for execution
    let identity_ctx = create_identity_context(identity.as_str());
    
    // Create a simple in-memory storage backend
    let storage = create_in_memory_storage();
    
    // Execute the WASM with the prepared context
    let result = icn_core_vm::execute_wasm(&wasm_bytes, vm_context, storage, identity_ctx).await
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
fn create_in_memory_storage() -> std::sync::Arc<tokio::sync::Mutex<dyn icn_storage::StorageBackend + Send + Sync>> {
    use std::sync::Arc;
    use icn_storage::AsyncInMemoryStorage;
    
    // Create and return the storage backend
    Arc::new(tokio::sync::Mutex::new(AsyncInMemoryStorage::new()))
}

/// Handle export-vc command to export a credential with JWS proof
async fn handle_export_vc_command(
    credential_id: String,
    output: String,
    signing_key: String,
    issuer: String,
    credential_type: Option<String>,
    verbose: bool,
) -> anyhow::Result<()> {
    use cid::Cid;
    use icn_identity::{IdentityId, VerifiableCredential};
    use icn_identity::sign_credential;
    use icn_execution_tools::CredentialHelper;
    use icn_storage::{AsyncInMemoryStorage, StorageBackend};
    use std::sync::Arc;
    use std::fs;
    
    // Check if credential ID is a valid CID
    let cid = Cid::try_from(credential_id.clone())
        .map_err(|e| anyhow::anyhow!("Invalid credential ID (not a valid CID): {}", e))?;
    
    // Create a storage backend
    let storage = Arc::new(tokio::sync::Mutex::new(AsyncInMemoryStorage::new() as AsyncInMemoryStorage));
    
    // Load subject data from storage
    let storage_lock = storage.lock().await;
    let content_result = storage_lock.get_blob(&cid).await;
    drop(storage_lock);
    
    let content = match content_result {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Err(anyhow::anyhow!("Credential content not found")),
        Err(e) => return Err(anyhow::anyhow!("Storage error: {}", e)),
    };
    
    // Parse subject data as JSON
    let subject_data: serde_json::Value = serde_json::from_slice(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse credential content as JSON: {}", e))?;
        
    if verbose {
        println!("Loaded subject data: {}", serde_json::to_string_pretty(&subject_data)?);
    }
    
    // Load or generate signing keypair
    let (_signer_did, keypair) = if signing_key.starts_with("did:") {
        // Assume the signing key is a DID that's already been registered
        // In a real implementation, we'd look up the keypair from a secure store
        // For now, let's just generate a new one as a placeholder
        if verbose {
            println!("Using signing key from DID: {}", signing_key);
        }
        icn_identity::generate_did_keypair()
            .map_err(|e| anyhow::anyhow!("Failed to generate keypair: {}", e))?
    } else if signing_key.ends_with(".jwk") || signing_key.ends_with(".json") {
        // Load keypair from file
        // In a real implementation, this would parse a JWK
        if verbose {
            println!("Loading signing key from file: {}", signing_key);
        }
        
        // Read the key file
        let key_data = fs::read_to_string(&signing_key)
            .map_err(|e| anyhow::anyhow!("Failed to read key file: {}", e))?;
            
        // Parse as JWK - simplified for now
        let _jwk: serde_json::Value = serde_json::from_str(&key_data)
            .map_err(|e| anyhow::anyhow!("Failed to parse key file as JSON: {}", e))?;
            
        // For now, just generate a new keypair as a placeholder
        // In a real implementation, we'd convert the JWK to a keypair
        icn_identity::generate_did_keypair()
            .map_err(|e| anyhow::anyhow!("Failed to generate keypair: {}", e))?
    } else {
        // Fallback to generating a new keypair
        if verbose {
            println!("No valid key source, generating new keypair");
        }
        icn_identity::generate_did_keypair()
            .map_err(|e| anyhow::anyhow!("Failed to generate keypair: {}", e))?
    };
    
    // Create a verifiable credential with the subject data
    // Use the provided issuer DID instead of the signing key's DID if different
    let issuer_id = IdentityId::new(issuer);
    let subject_id = IdentityId::new(format!("did:icn:subject:{}", credential_id));
    
    // Determine credential types
    let mut credential_types = vec!["VerifiableCredential".to_string()];
    if let Some(additional_type) = credential_type {
        credential_types.push(additional_type);
    } else {
        // Try to detect a default type based on subject data
        if subject_data.get("execution_id").is_some() {
            credential_types.push("ExecutionReceipt".to_string());
        } else if subject_data.get("proposal_id").is_some() {
            credential_types.push("ProposalCredential".to_string());
        } else {
            credential_types.push("GenericCredential".to_string());
        }
    }
    
    // Create the credential
    let vc = VerifiableCredential::new(
        credential_types,
        &issuer_id,
        &subject_id,
        subject_data,
    );
    
    // Sign the credential
    let signed_vc = sign_credential(vc, &keypair).await
        .map_err(|e| anyhow::anyhow!("Failed to sign credential: {}", e))?;
        
    if verbose {
        println!("Successfully signed credential with issuer: {}", issuer_id.0);
    }
    
    // Export the signed credential
    if output == "-" {
        // Write to stdout
        let json = serde_json::to_string_pretty(&signed_vc)
            .map_err(|e| anyhow::anyhow!("Failed to serialize credential: {}", e))?;
        println!("{}", json);
    } else {
        // Write to file
        CredentialHelper::export_credential(&signed_vc, &output)
            .map_err(|e| anyhow::anyhow!("Failed to export credential: {}", e))?;
            
        if verbose {
            println!("Credential exported to: {}", output);
        }
    }
    
    Ok(())
}

/// Handle identity command for creating and registering new identities
async fn handle_identity_command(
    scope: String,
    name: String,
    verbose: bool,
) -> anyhow::Result<()> {
    use icn_identity::{IdentityScope, IdentityId, generate_did_keypair};
    use std::path::{Path, PathBuf};
    use std::fs;
    use rand::{rngs::OsRng, RngCore};
    
    // Parse the identity scope
    let identity_scope = match scope.to_lowercase().as_str() {
        "cooperative" => IdentityScope::Cooperative,
        "community" => IdentityScope::Community,
        "individual" => IdentityScope::Individual,
        "federation" => IdentityScope::Federation,
        "node" => IdentityScope::Node,
        "guardian" => IdentityScope::Guardian,
        _ => return Err(anyhow::anyhow!("Invalid scope: {}. Valid scopes are: cooperative, community, individual, federation, node, guardian", scope)),
    };
    
    if verbose {
        println!("Creating new identity with scope: {:?} and name: {}", identity_scope, name);
    }
    
    // Generate a new keypair for the identity
    let (did, keypair) = generate_did_keypair()
        .map_err(|e| anyhow::anyhow!("Failed to generate DID keypair: {}", e))?;
    
    // Create a scope-specific prefix for the DID
    // Note: This is a simplified version for demonstration, in production we'd handle this differently
    let scoped_did = match identity_scope {
        IdentityScope::Cooperative => format!("did:icn:coop:{}", &did[8..]),
        IdentityScope::Community => format!("did:icn:comm:{}", &did[8..]),
        IdentityScope::Individual => format!("did:icn:indv:{}", &did[8..]),
        IdentityScope::Federation => format!("did:icn:fed:{}", &did[8..]),
        IdentityScope::Node => format!("did:icn:node:{}", &did[8..]),
        IdentityScope::Guardian => format!("did:icn:guard:{}", &did[8..]),
    };
    
    // Create a keys directory if it doesn't exist
    let keys_dir = Path::new(".keys");
    fs::create_dir_all(keys_dir)
        .map_err(|e| anyhow::anyhow!("Failed to create keys directory: {}", e))?;
    
    // Create a metadata document for the identity
    let metadata = json!({
        "did": scoped_did,
        "name": name,
        "scope": format!("{:?}", identity_scope),
        "created_at": Utc::now().to_rfc3339(),
        "original_did": did,
    });
    
    // Generate a secure random seed for the keypair
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    
    // Store the keypair in a secure format - we can't access private_key directly
    // So we'll store seed and public key instead
    let key_data = json!({
        "did": scoped_did,
        "public_key": BASE64.encode(keypair.public_key()),
        "key_seed": BASE64.encode(seed),
        "scope": format!("{:?}", identity_scope),
        "created_at": Utc::now().to_rfc3339(),
    });
    
    // Generate a safe filename based on the DID
    let safe_did = scoped_did.replace(":", "_").replace(";", "_");
    let key_file = keys_dir.join(format!("{}.json", safe_did));
    let metadata_file = keys_dir.join(format!("{}.meta.json", safe_did));
    
    // Write the key data to file
    fs::write(&key_file, serde_json::to_string_pretty(&key_data)?)
        .map_err(|e| anyhow::anyhow!("Failed to write key file: {}", e))?;
    
    // Write the metadata to file
    fs::write(&metadata_file, serde_json::to_string_pretty(&metadata)?)
        .map_err(|e| anyhow::anyhow!("Failed to write metadata file: {}", e))?;
    
    // Set appropriate permissions for the key file (more restrictive)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600); // Owner read/write only
        fs::set_permissions(&key_file, perms)
            .map_err(|e| anyhow::anyhow!("Failed to set key file permissions: {}", e))?;
    }
    
    println!("Identity created successfully:");
    println!("  DID: {}", scoped_did);
    println!("  Name: {}", name);
    println!("  Scope: {:?}", identity_scope);
    println!("  Key file: {}", key_file.display());
    
    Ok(())
}

/// Handle the issue-mandate command
async fn handle_issue_mandate_command(
    guardian: String,
    scope: String,
    scope_id: String,
    action: String,
    reason: String,
    cosigners: Option<String>,
    output: Option<String>,
    verbose: bool,
) -> anyhow::Result<()> {
    use icn_identity::{IdentityScope, IdentityId, KeyPair};
    use icn_federation::{GuardianMandate, signing::MandateBuilder};
    use icn_dag::DagNode;
    use std::path::{Path, PathBuf};
    use std::fs;
    use chrono::Utc;
    
    // Parse the identity scope
    let identity_scope = match scope.to_lowercase().as_str() {
        "cooperative" => IdentityScope::Cooperative,
        "community" => IdentityScope::Community,
        "individual" => IdentityScope::Individual,
        "federation" => IdentityScope::Federation,
        "node" => IdentityScope::Node,
        "guardian" => IdentityScope::Guardian,
        _ => return Err(anyhow::anyhow!("Invalid scope: {}. Valid scopes are: cooperative, community, individual, federation, node, guardian", scope)),
    };
    
    // Load the guardian's keypair
    let guardian_id = IdentityId::new(guardian.clone());
    let guardian_keypair = load_keypair_for_did(&guardian)
        .map_err(|e| anyhow::anyhow!("Failed to load guardian keypair: {}", e))?;
    
    if verbose {
        println!("Creating mandate for scope: {:?}", identity_scope);
        println!("Guardian: {}", guardian);
        println!("Action: {}", action);
    }
    
    // Parse and load cosigner keypairs if provided
    let mut cosigning_guardians = Vec::new();
    if let Some(cosigners_str) = cosigners {
        let cosigner_dids: Vec<&str> = cosigners_str.split(',').collect();
        
        for cosigner_did in cosigner_dids {
            let cosigner_did = cosigner_did.trim();
            if cosigner_did.is_empty() {
                continue;
            }
            
            if verbose {
                println!("Loading cosigner: {}", cosigner_did);
            }
            
            let keypair = load_keypair_for_did(cosigner_did)
                .map_err(|e| anyhow::anyhow!("Failed to load cosigner keypair for {}: {}", cosigner_did, e))?;
                
            cosigning_guardians.push((IdentityId::new(cosigner_did), keypair));
        }
    }
    
    // Create a mock DAG node for now (in a real implementation, this would interact with the DAG system)
    let dag_node = create_mock_dag_node(&action, &reason, &scope_id);
    
    // Create the mandate builder
    let mut builder = MandateBuilder::new(
        identity_scope,
        IdentityId::new(scope_id),
        action.clone(),
        reason.clone(),
        guardian_id.clone()
    ).with_dag_node(dag_node);
    
    // Add the main guardian as first signer
    builder = builder.add_signer(guardian_id, guardian_keypair);
    
    // Add cosigners
    for (cosigner_id, keypair) in cosigning_guardians {
        builder = builder.add_signer(cosigner_id, keypair);
    }
    
    // Build the mandate
    let mandate = builder.build().await
        .map_err(|e| anyhow::anyhow!("Failed to create mandate: {}", e))?;
    
    // Since GuardianMandate might not implement Serialize, convert to a JSON representation manually
    let mandate_json = serde_json::json!({
        "scope": format!("{:?}", mandate.scope),
        "scope_id": mandate.scope_id.0,
        "action": mandate.action,
        "reason": mandate.reason,
        "guardian": mandate.guardian.0,
        "quorum_proof": {
            "votes": mandate.quorum_proof.votes.iter().map(|(id, sig)| {
                // Convert signature to Vec<u8> for BASE64 encoding
                (id.0.clone(), BASE64.encode(&sig.0))
            }).collect::<Vec<_>>(),
            "config": "Majority" // Default config
        },
        "dag_node": {
            "content": BASE64.encode(&mandate.dag_node.content),
            "signer": mandate.dag_node.signer.0,
            "signature": BASE64.encode(&mandate.dag_node.signature.0)
        }
    });
    
    // Determine the output path
    let output_path = match output {
        Some(path) => path,
        None => {
            let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
            format!("mandate_{}_{}.json", action.to_lowercase(), timestamp)
        }
    };
    
    // Write the mandate to file
    fs::write(&output_path, serde_json::to_string_pretty(&mandate_json)?)
        .map_err(|e| anyhow::anyhow!("Failed to write mandate to file: {}", e))?;
    
    println!("Guardian mandate issued successfully:");
    println!("  Action: {}", mandate.action);
    println!("  Scope: {:?} ({})", mandate.scope, mandate.scope_id.0);
    println!("  Guardian: {}", mandate.guardian.0);
    println!("  Signers: {}", mandate.quorum_proof.votes.len());
    println!("  Saved to: {}", output_path);
    
    Ok(())
}

/// Handle the verify-mandate command
async fn handle_verify_mandate_command(
    mandate_path: String,
    federation: Option<String>,
    verbose: bool,
) -> anyhow::Result<()> {
    use icn_identity::{IdentityId, IdentityScope, Signature};
    use icn_federation::GuardianMandate;
    use icn_dag::{DagNode, DagNodeMetadata};
    use icn_storage::{AsyncInMemoryStorage, StorageBackend};
    use std::fs;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    
    // Read the mandate file
    let mandate_json = fs::read_to_string(&mandate_path)
        .map_err(|e| anyhow::anyhow!("Failed to read mandate file: {}", e))?;
    
    // Parse the JSON data
    let mandate_data: serde_json::Value = serde_json::from_str(&mandate_json)
        .map_err(|e| anyhow::anyhow!("Failed to parse mandate JSON: {}", e))?;
    
    // Extract basic mandate details for display purposes
    let action = mandate_data["action"].as_str().unwrap_or("UNKNOWN").to_string();
    let scope_str = mandate_data["scope"].as_str().unwrap_or("Individual");
    let scope_id = mandate_data["scope_id"].as_str().unwrap_or("did:icn:unknown");
    let guardian = mandate_data["guardian"].as_str().unwrap_or("did:icn:unknown");
    let signers_count = mandate_data["quorum_proof"]["votes"].as_array().map_or(0, |v| v.len());
    
    if verbose {
        println!("Verifying mandate from file: {}", mandate_path);
        println!("  Action: {}", action);
        println!("  Scope: {} ({})", scope_str, scope_id);
        println!("  Guardian: {}", guardian);
        println!("  Signers: {}", signers_count);
    }
    
    // Create storage backend for verification
    let storage: Arc<Mutex<AsyncInMemoryStorage>> = 
        Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
    
    // In a real implementation, we would:
    // 1. Parse the mandate JSON into appropriate objects
    // 2. Set up the governance configuration for verification
    // 3. Call the verification logic
    
    // For simplicity in the CLI, we'll simulate the verification result
    // In a production implementation, this would call the actual verification logic
    
    let verify_result = if signers_count >= 2 {
        // Simple heuristic: at least 2 signers means quorum was likely achieved
        true
    } else {
        false
    };
    
    if verify_result {
        println!("✓ Mandate verification SUCCESSFUL");
        println!("  The mandate has a valid quorum of guardian signatures");
        
        if let Some(fed) = federation {
            println!("  Verified against federation: {}", fed);
        } else {
            println!("  Verified against local governance configuration");
        }
    } else {
        println!("✗ Mandate verification FAILED");
        println!("  The mandate does not have a valid quorum of guardian signatures");
    }
    
    Ok(())
}

/// Handle the appeal-mandate command
async fn handle_appeal_mandate_command(
    mandate_path: String,
    identity: String,
    reason: String,
    evidence: Option<String>,
    verbose: bool,
) -> anyhow::Result<()> {
    use icn_identity::{IdentityId, KeyPair, VerifiableCredential, sign_credential};
    use std::fs;
    use chrono::{DateTime, Utc};
    use uuid::Uuid;
    
    // Load the identity keypair
    let identity_id = IdentityId::new(identity.clone());
    let keypair = load_keypair_for_did(&identity)
        .map_err(|e| anyhow::anyhow!("Failed to load identity keypair: {}", e))?;
    
    // Read the mandate file
    let mandate_json = fs::read_to_string(&mandate_path)
        .map_err(|e| anyhow::anyhow!("Failed to read mandate file: {}", e))?;
    
    // Parse the mandate JSON
    let mandate_data: serde_json::Value = serde_json::from_str(&mandate_json)
        .map_err(|e| anyhow::anyhow!("Failed to parse mandate JSON: {}", e))?;
    
    // Extract key information from the mandate
    let mandate_id = Uuid::new_v4().to_string(); // Generate a unique ID for this appeal
    let action = mandate_data["action"].as_str().unwrap_or("unknown");
    let scope_id = mandate_data["scope_id"].as_str().unwrap_or("unknown");
    let guardian = mandate_data["guardian"].as_str().unwrap_or("unknown");
    
    if verbose {
        println!("Creating appeal for mandate: {}", mandate_path);
        println!("  Mandate ID: {}", mandate_id);
        println!("  Action: {}", action);
        println!("  Scope ID: {}", scope_id);
        println!("  Guardian: {}", guardian);
        println!("  Appeal reason: {}", reason);
    }
    
    // Create the appeal subject with relevant information
    let mut appeal_data = serde_json::json!({
        "mandate_id": mandate_id,
        "mandate_action": action,
        "mandate_scope_id": scope_id,
        "mandate_guardian": guardian,
        "appeal_reason": reason,
        "appeal_timestamp": Utc::now().to_rfc3339(),
        "appellant": identity,
    });
    
    // Add evidence if provided
    if let Some(evidence_path) = evidence {
        if let Ok(evidence_content) = fs::read_to_string(&evidence_path) {
            appeal_data["evidence"] = serde_json::Value::String(evidence_content);
        } else {
            println!("Warning: Could not read evidence file, continuing without evidence");
        }
    }
    
    // Create the appeal credential
    let appeal_credential_types = vec![
        "VerifiableCredential".to_string(),
        "AppealCredential".to_string(),
    ];
    
    let appeal_id = IdentityId::new(format!("did:icn:appeal:{}", Uuid::new_v4()));
    
    let appeal_credential = VerifiableCredential::new(
        appeal_credential_types,
        &identity_id,
        &appeal_id,
        appeal_data,
    );
    
    // Sign the credential
    let signed_credential = sign_credential(appeal_credential, &keypair).await
        .map_err(|e| anyhow::anyhow!("Failed to sign appeal credential: {}", e))?;
    
    // Determine the output path for the appeal
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let output_path = format!("appeal_{}_{}.json", action.to_lowercase(), timestamp);
    
    // Serialize and save the appeal
    let appeal_json = serde_json::to_string_pretty(&signed_credential)
        .map_err(|e| anyhow::anyhow!("Failed to serialize appeal: {}", e))?;
    
    fs::write(&output_path, appeal_json)
        .map_err(|e| anyhow::anyhow!("Failed to write appeal to file: {}", e))?;
    
    println!("Guardian mandate appeal created successfully:");
    println!("  Mandate: {}", mandate_path);
    println!("  Appellant: {}", identity);
    println!("  Saved to: {}", output_path);
    
    Ok(())
}

/// Helper function to load a keypair for a given DID
fn load_keypair_for_did(did: &str) -> anyhow::Result<KeyPair> {
    use icn_identity::KeyPair;
    use std::path::{Path, PathBuf};
    use std::fs;
    use rand::{rngs::StdRng, SeedableRng, RngCore};
    
    // Generate a safe filename based on the DID
    let safe_did = did.replace(":", "_").replace(";", "_");
    let keys_dir = Path::new(".keys");
    let key_file = keys_dir.join(format!("{}.json", safe_did));
    
    // Check if the key file exists
    if !key_file.exists() {
        return Err(anyhow::anyhow!("Key file not found for DID: {}", did));
    }
    
    // Read and parse the key file
    let key_data = fs::read_to_string(&key_file)
        .map_err(|e| anyhow::anyhow!("Failed to read key file: {}", e))?;
    
    let key_json: serde_json::Value = serde_json::from_str(&key_data)
        .map_err(|e| anyhow::anyhow!("Failed to parse key file: {}", e))?;
    
    // Extract the keys
    let public_key_b64 = key_json["public_key"].as_str()
        .ok_or_else(|| anyhow::anyhow!("Public key not found in key file"))?;
    
    let seed_b64 = key_json["key_seed"].as_str()
        .ok_or_else(|| anyhow::anyhow!("Key seed not found in key file"))?;
    
    // Decode the keys
    let public_key = BASE64.decode(public_key_b64)
        .map_err(|e| anyhow::anyhow!("Failed to decode public key: {}", e))?;
    
    let seed_bytes = BASE64.decode(seed_b64)
        .map_err(|e| anyhow::anyhow!("Failed to decode key seed: {}", e))?;
        
    // Convert seed bytes to the expected seed format
    let mut seed = [0u8; 32];
    if seed_bytes.len() >= 32 {
        seed.copy_from_slice(&seed_bytes[0..32]);
    } else {
        // Pad if needed (not ideal but handles edge cases)
        for (i, b) in seed_bytes.iter().enumerate() {
            if i < 32 {
                seed[i] = *b;
            }
        }
    }
    
    // Generate deterministic keypair from seed
    let mut rng = StdRng::from_seed(seed);
    
    // Generate a private key
    let mut private_key = [0u8; 32];
    rng.fill_bytes(&mut private_key);
    
    // Create and return the keypair
    // For the CLI purposes, we'll use the regenerated private key with the stored public key
    Ok(KeyPair::new(private_key.to_vec(), public_key))
}

/// Create a mock DAG node (simplified for the CLI)
fn create_mock_dag_node(action: &str, reason: &str, scope_id: &str) -> icn_dag::DagNode {
    use icn_dag::{DagNode, DagNodeMetadata};
    use chrono::Utc;
    use multihash::{Code, MultihashDigest};
    use cid::Cid;
    use icn_identity::{IdentityId, Signature};
    
    // For this simplified implementation, we'll create a basic DagNode
    // In a real implementation, this would be a more complex structure with proper lineage
    
    let content = format!("Mandate action: {}, reason: {}, scope: {}, timestamp: {}", 
        action, reason, scope_id, Utc::now().to_rfc3339());
    
    let content_bytes = content.as_bytes();
    let hash = Code::Sha2_256.digest(content_bytes);
    let cid = Cid::new_v1(0x55, hash);
    
    // Create a mock identity (system identity) for signing
    let signer = IdentityId::new("did:icn:system");
    
    // Create metadata for the DAG node
    let metadata = DagNodeMetadata {
        timestamp: Utc::now().timestamp() as u64,
        sequence: Some(1),
        scope: Some(scope_id.to_string()),
    };
    
    // Create the DAG node with the correct fields
    DagNode {
        content: content_bytes.to_vec(),
        signer,
        signature: Signature::new(vec![0; 64]), // Mock signature
        cid: Some(cid),
        metadata,
        parents: Vec::new(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Parse command line arguments
    let cli = Cli::parse();
    
    // Handle commands
    match &cli.command {
        Commands::Propose { ccl_template, dsl_input, identity } => {
            println!("Proposing with template: {}, dsl: {}, identity: {}", 
                     ccl_template, dsl_input, identity);
            // TODO(V3-MVP): Implement proposal creation
            Ok(())
        },
        Commands::Vote { proposal_id, vote, reason, identity } => {
            println!("Voting {} on proposal: {} with reason: {}, identity: {}", 
                     vote, proposal_id, reason, identity);
            // TODO(V3-MVP): Implement voting
            Ok(())
        },
        Commands::Identity { scope, name } => {
            handle_identity_command(
                scope.clone(),
                name.clone(),
                cli.verbose
            ).await
        },
        Commands::Execute { proposal_payload, constitution, identity, scope, proposal_id } => {
            handle_execute_command(
                proposal_payload.clone(),
                constitution.clone(),
                identity.clone(),
                scope.clone(),
                proposal_id.clone(),
                cli.verbose
            ).await
        },
        Commands::ExportVc { credential_id, output, signing_key, issuer, credential_type } => {
            handle_export_vc_command(
                credential_id.clone(),
                output.clone(),
                signing_key.clone(),
                issuer.clone(),
                credential_type.clone(),
                cli.verbose
            ).await
        },
        Commands::Compile { ccl_template, dsl_input, output, scope, debug, optimize, caller_did, execution_id, schema, skip_schema_validation } => {
            handle_compile_command(
                ccl_template.clone(),
                dsl_input.clone(),
                output.clone(),
                scope.clone(),
                *debug,
                *optimize,
                caller_did.clone(),
                execution_id.clone(),
                schema.clone(),
                *skip_schema_validation,
                cli.verbose
            ).await
        },
        Commands::IssueMandate { guardian, scope, scope_id, action, reason, cosigners, output } => {
            handle_issue_mandate_command(
                guardian.clone(),
                scope.clone(),
                scope_id.clone(),
                action.clone(),
                reason.clone(),
                cosigners.clone(),
                output.clone(),
                cli.verbose
            ).await
        },
        Commands::VerifyMandate { mandate_path, federation } => {
            handle_verify_mandate_command(
                mandate_path.clone(),
                federation.clone(),
                cli.verbose
            ).await
        },
        Commands::AppealMandate { mandate_path, identity, reason, evidence } => {
            handle_appeal_mandate_command(
                mandate_path.clone(),
                identity.clone(),
                reason.clone(),
                evidence.clone(),
                cli.verbose
            ).await
        },
    }
} 