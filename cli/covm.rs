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
    use icn_economics::{ResourceType, ResourceAuthorization};
    use icn_identity::IdentityScope;
    use uuid::Uuid;
    
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
    
    // Create resource authorizations based on the governance config
    // This is where you would implement the logic to derive authorizations from the config
    // For now, we'll create some basic authorizations for common resource types
    let authorized_resources = vec![
        ResourceType::Compute,
        ResourceType::Storage,
        ResourceType::NetworkBandwidth,
        ResourceType::Custom { identifier: "Memory".to_string() },
    ];
    
    // Create active authorizations with high limits for now
    let active_authorizations = authorized_resources.iter().map(|rt| {
        ResourceAuthorization {
            auth_id: uuid::Uuid::new_v4(),
            grantor_did: "system".to_string(),
            grantee_did: identity.clone(),
            resource_type: rt.clone(),
            authorized_amount: 1_000_000,  // High limit for testing
            consumed_amount: 0,
            scope: identity_scope,
            expiry_timestamp: Some(timestamp + 3600),  // 1 hour from now
            metadata: None,
        }
    }).collect::<Vec<_>>();
    
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