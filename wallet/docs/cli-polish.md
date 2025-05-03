# CLI Polish Guidelines

This document outlines the improvements needed for the ICN Wallet CLI to ensure consistent argument handling, better help messages, and improved output formatting.

## Command Structure Improvements

### Current Structure
```
icn-wallet-cli [global options] <command> [command options]
```

### Enhanced Structure
```
icn-wallet-cli [global options] <command> <subcommand> [options]
```

- Ensure all commands follow a consistent pattern
- Group related functionality under parent commands
- Use consistent terminology across commands

## Help Message Improvements

### Enhanced Main Help

Improve the main help message:

```
ICN Wallet CLI v1.0.0

A command-line interface for the ICN Wallet, providing identity management,
credential handling, proposal signing, and governance participation.

USAGE:
  icn-wallet-cli [OPTIONS] <COMMAND>

GLOBAL OPTIONS:
  -d, --data-dir <PATH>    Data directory [default: ./wallet-data]
  -v, --verbose            Enable verbose output
  -h, --help               Print help information
  -V, --version            Print version information

COMMANDS:
  identity    Manage DID identities
  credential  Handle verifiable credentials
  proposal    Create and sign proposals
  sync        Synchronize with federation
  agoranet    Interact with AgoraNet
  bundle      Manage trust bundles
  serve       Start the API server

Use "icn-wallet-cli <command> --help" for more information about a command.
```

### Command Help

Improve command-specific help messages:

```
COMMAND: identity
  Manage DID identities within the wallet

USAGE:
  icn-wallet-cli identity <SUBCOMMAND>

SUBCOMMANDS:
  create     Create a new identity
  list       List all identities
  show       Show details of a specific identity
  activate   Set the active identity for operations
  export     Export identity to file
  import     Import identity from file

Use "icn-wallet-cli identity <subcommand> --help" for more information about a subcommand.
```

### Subcommand Help

Improve subcommand-specific help messages:

```
SUBCOMMAND: create
  Create a new identity with the specified scope

USAGE:
  icn-wallet-cli identity create [OPTIONS]

OPTIONS:
  -s, --scope <SCOPE>     Identity scope (personal, organization, device, service)
                          [default: personal]
  -m, --metadata <JSON>   Optional JSON metadata
  -o, --output <FORMAT>   Output format (text, json) [default: text]

EXAMPLES:
  # Create a personal identity
  icn-wallet-cli identity create

  # Create an organizational identity with metadata
  icn-wallet-cli identity create --scope organization --metadata '{"name":"Acme Corp"}'

  # Create an identity and output as JSON
  icn-wallet-cli identity create --output json
```

## Output Formatting

### 1. Consistent Text Output

Ensure text output is consistent across commands:

```
IDENTITY CREATED
ID:       f47ac10b-58cc-4372-a567-0e02b2c3d479
DID:      did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK
Scope:    personal
Created:  2023-05-01 12:00:00 UTC
```

- Use uppercase headers for sections
- Align values consistently
- Use consistent date formats
- Ensure readability with proper spacing

### 2. Tables for Collections

Use tables for displaying collections:

```
IDENTITIES
+--------------------------------------+--------------------------------------+-------------+------------------------+
| ID                                   | DID                                  | Scope       | Created                |
+--------------------------------------+--------------------------------------+-------------+------------------------+
| f47ac10b-58cc-4372-a567-0e02b2c3d479 | did:icn:z6MkhaXgBZDvotDkL5257faiz... | personal    | 2023-05-01 12:00:00   |
| a1b2c3d4-e5f6-4a5b-8c7d-9e0f1a2b3c4d | did:icn:z6MkhZTjRYkJEYMSnbPT9PAff... | organization| 2023-05-02 15:30:00   |
+--------------------------------------+--------------------------------------+-------------+------------------------+
```

- Use clear column headers
- Truncate long values appropriately (with ellipsis)
- Align columns for readability
- Add counts or summaries when appropriate

### 3. JSON Output Option

Add JSON output option for all commands:

```
icn-wallet-cli identity list --output json
```

Response:
```json
{
  "identities": [
    {
      "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
      "did": "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
      "scope": "personal",
      "created_at": "2023-05-01T12:00:00Z",
      "is_active": true
    },
    {
      "id": "a1b2c3d4-e5f6-4a5b-8c7d-9e0f1a2b3c4d",
      "did": "did:icn:z6MkhZTjRYkJEYMSnbPT9PAffxvNaEmhoLTcUgAvZfcvWUcN",
      "scope": "organization",
      "created_at": "2023-05-02T15:30:00Z",
      "is_active": false
    }
  ],
  "count": 2,
  "active_identity": "f47ac10b-58cc-4372-a567-0e02b2c3d479"
}
```

- Ensure JSON output is properly formatted
- Include metadata in JSON output
- Make JSON structure consistent across commands

### 4. Progress Indicators

Add progress indicators for long-running operations:

```
Syncing trust bundles... ⣾⣽⣻⢿⡿⣟⣯⣷ 45%
Fetching from peer 3/5: example.icn.network
```

- Use spinners for indeterminate operations
- Show percentage for operations with known progress
- Display step information for multi-step operations

### 5. Error Formatting

Improve error message formatting:

```
ERROR: Failed to create identity

DETAILS:
  - Unable to generate keypair: Insufficient entropy
  - System entropy pool may be depleted

SUGGESTED ACTIONS:
  - Wait a few moments and try again
  - Install rng-tools for improved entropy generation
  - Add the --force flag to use a less secure random source
```

- Clearly indicate errors with "ERROR:" prefix
- Provide detailed error information
- Include suggested actions when possible
- Format errors consistently

## Argument Handling Improvements

### 1. Global Options

Make global options consistent across all commands:

```
--data-dir <PATH>       Set data directory
--identity <ID|PATH>    Select identity (by ID or file path)
--output <FORMAT>       Output format (text, json)
--verbose               Enable verbose output
--quiet                 Suppress all output except errors
--no-color              Disable colored output
```

### 2. Common Command Structure

Restructure commands for consistency:

```
icn-wallet-cli identity create   # Create an identity
icn-wallet-cli identity list     # List identities
icn-wallet-cli identity show     # Show identity details

icn-wallet-cli credential issue  # Issue a credential
icn-wallet-cli credential verify # Verify a credential
icn-wallet-cli credential list   # List credentials

icn-wallet-cli proposal create   # Create a proposal
icn-wallet-cli proposal sign     # Sign a proposal
icn-wallet-cli proposal list     # List proposals
```

### 3. Input Validation

Improve input validation with clear error messages:

- Validate file paths before attempting to read files
- Check for valid UUIDs, DIDs, and other formatted strings
- Validate JSON inputs when appropriate
- Provide specific error messages for validation failures

### 4. Default Values

Make default values explicit in help messages:

```
--port <PORT>           Port to listen on [default: 3000]
--scope <SCOPE>         Identity scope [default: personal]
--format <FORMAT>       Output format [default: text]
```

## Implementation Tasks

1. **Reorganize Command Structure**
   - Refactor `main.rs` to use a more consistent command structure
   - Group related commands under parent commands
   - Ensure consistent naming across commands

2. **Improve Help Messages**
   - Enhance main help message
   - Add detailed help for each command
   - Include examples in help messages

3. **Enhance Output Formatting**
   - Implement consistent text output
   - Add table formatting for collections
   - Support JSON output for all commands
   - Add progress indicators

4. **Standardize Error Handling**
   - Create structured error messages
   - Include detailed error information
   - Add suggested actions for common errors

5. **Consistent Argument Handling**
   - Standardize global options
   - Use consistent argument patterns
   - Improve input validation

## Example Implementation

### Example: Enhanced Command Structure

```rust
/// ICN Wallet CLI
#[derive(Parser)]
#[command(author, version, about = "Command-line interface for the ICN Wallet")]
#[command(long_about = "A command-line interface for managing ICN Wallet identities, credentials, and governance participation.")]
struct Cli {
    /// Data directory for wallet data
    #[arg(short, long, global = true, default_value = "./wallet-data", env = "ICN_WALLET_DATA_DIR")]
    data_dir: PathBuf,
    
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
    
    /// Output format (text, json)
    #[arg(short, long, global = true, default_value = "text")]
    output: String,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage DID identities
    Identity {
        #[command(subcommand)]
        command: IdentityCommands,
    },
    
    /// Handle verifiable credentials
    Credential {
        #[command(subcommand)]
        command: CredentialCommands,
    },
    
    /// Create and sign proposals
    Proposal {
        #[command(subcommand)]
        command: ProposalCommands,
    },
    
    /// Synchronize with the federation
    Sync {
        /// Identity to use for synchronization
        #[arg(short, long)]
        identity: Option<String>,
        
        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// Start the wallet API server
    Serve {
        /// Host to bind to
        #[arg(short, long, default_value = "127.0.0.1")]
        host: String,
        
        /// Port to listen on
        #[arg(short, long, default_value_t = 3000)]
        port: u16,
        
        /// AgoraNet API URL
        #[arg(long)]
        agoranet_url: Option<String>,
    },
}

#[derive(Subcommand)]
enum IdentityCommands {
    /// Create a new identity
    Create {
        /// Type of identity (personal, organization, device, service)
        #[arg(short, long, default_value = "personal")]
        scope: String,
        
        /// Optional JSON metadata
        #[arg(short, long)]
        metadata: Option<String>,
    },
    
    /// List all identities
    List,
    
    /// Show details of a specific identity
    Show {
        /// Identity ID
        #[arg(required = true)]
        id: String,
    },
    
    /// Set the active identity
    Activate {
        /// Identity ID
        #[arg(required = true)]
        id: String,
    },
}
```

### Example: Improved Output Formatting

```rust
fn format_identity_output(wallet: &IdentityWallet, id: &str, output_format: &str) -> Result<()> {
    if output_format == "json" {
        let json = serde_json::json!({
            "id": id,
            "did": wallet.did.to_string(),
            "scope": format!("{:?}", wallet.scope),
            "document": wallet.to_document(),
            "created_at": chrono::Utc::now().to_rfc3339(),
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("IDENTITY");
        println!("ID:       {}", id);
        println!("DID:      {}", wallet.did.to_string());
        println!("Scope:    {:?}", wallet.scope);
        println!("Created:  {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
        println!();
        println!("DID Document:");
        let doc_json = serde_json::to_string_pretty(&wallet.to_document())?;
        println!("{}", doc_json);
    }
    
    Ok(())
}
```

### Example: Progress Indicator

```rust
fn sync_trust_bundles(client: &SyncClient, verbose: bool) -> Result<()> {
    println!("Syncing trust bundles...");
    
    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_message("Connecting to federation peers...");
    spinner.enable_steady_tick(100);
    
    // Perform initial connection
    spinner.set_message("Discovering peers...");
    let peers = client.discover_peers().await?;
    
    let progress = indicatif::ProgressBar::new(peers.len() as u64);
    progress.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=>-")
    );
    
    // Sync from each peer
    for (i, peer) in peers.iter().enumerate() {
        progress.set_message(format!("Syncing from {}", peer));
        
        // Perform sync
        let _result = client.sync_from_peer(peer).await;
        
        progress.inc(1);
    }
    
    progress.finish_with_message("Sync completed");
    
    let bundles = client.list_trust_bundles().await?;
    println!("Synced {} trust bundles", bundles.len());
    
    Ok(())
}
``` 