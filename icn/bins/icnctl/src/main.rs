//! icnctl - CLI for managing ICNd
#![deny(clippy::unwrap_used, clippy::expect_used)]

use anyhow::{bail, Context, Result};
use rust_i18n::t;

// Initialize i18n with locale files from the locales directory
rust_i18n::i18n!("locales", fallback = "en");
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::generate;
// Governance types no longer needed - using RPC instead
use icn_identity::{
    AgeKeyStore, Capability, Did, KeyPair, KeyStore, KeyType,
    RecoveryConfig as IdentityRecoveryConfig, RecoveryMethod,
};
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph, TrustScore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tar::{Archive, Builder};
use zeroize::Zeroizing;

/// Maximum QR code data size in bytes for reliable scanning
const MAX_RELIABLE_QR_SIZE: usize = 2000;

#[derive(Parser, Debug)]
#[command(name = "icnctl")]
#[command(about = "ICN control CLI", long_about = None)]
struct Args {
    /// Data directory (defaults to ~/.icn)
    #[arg(short, long)]
    data_dir: Option<PathBuf>,

    /// RPC endpoint (defaults to 127.0.0.1:5601)
    #[arg(short, long, default_value = "127.0.0.1:5601")]
    endpoint: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show daemon status
    Status,

    /// Identity management
    #[command(subcommand)]
    Id(IdCommands),

    /// Device management (multi-device identity)
    #[command(subcommand)]
    Device(DeviceCommands),

    /// Social recovery for lost devices
    #[command(subcommand)]
    Recovery(RecoveryCommands),

    /// Trust graph management
    #[command(subcommand)]
    Trust(TrustCommands),

    /// Ledger operations
    #[command(subcommand)]
    Ledger(LedgerCommands),

    /// Dispute management (ledger entry disputes)
    #[command(subcommand)]
    Dispute(DisputeCommands),

    /// Contract operations
    #[command(subcommand)]
    Contract(ContractCommands),

    /// Network operations (mDNS discovery, QUIC sessions)
    #[command(subcommand)]
    Network(NetworkCommands),

    /// Federation management (cross-network connectivity)
    #[command(subcommand)]
    Federation(FederationCommands),

    /// Governance operations (domains, proposals, votes)
    #[command(subcommand)]
    Gov(GovCommands),

    /// Backup data directory
    Backup {
        /// Output file path for backup archive
        output: PathBuf,
    },

    /// Restore data directory from backup
    Restore {
        /// Input backup archive path
        input: PathBuf,

        /// Force restore even if data directory exists
        #[arg(short, long)]
        force: bool,
    },

    /// Verify backup integrity without permanent restore
    VerifyBackup {
        /// Backup archive path to verify
        input: PathBuf,

        /// Also verify ledger integrity (requires more time)
        #[arg(long)]
        verify_ledger: bool,
    },

    /// Snapshot management
    #[command(subcommand)]
    Snapshot(SnapshotCommands),

    /// Initialize a new cooperative (guided setup wizard)
    InitCoop {
        /// Cooperative name
        #[arg(short, long)]
        name: Option<String>,

        /// Initial member DIDs (comma-separated)
        #[arg(short, long)]
        members: Option<String>,

        /// Skip confirmation prompts
        #[arg(short, long)]
        yes: bool,

        /// Don't start the daemon after setup
        #[arg(long)]
        no_start: bool,
    },

    /// Gateway authentication (get JWT tokens)
    #[command(subcommand)]
    Auth(AuthCommands),

    /// Distributed compute operations
    #[command(subcommand)]
    Compute(ComputeCommands),

    /// Cooperative scheduling policy management (Phase 16E)
    #[command(subcommand)]
    Policy(PolicyCommands),

    /// Resource quota management (Phase 16E)
    #[command(subcommand)]
    Quota(QuotaCommands),

    /// Steward network operations (SDIS Phase S3)
    #[command(subcommand)]
    Steward(StewardCommands),

    /// Commons identity operations (Commons Evolution)
    #[command(subcommand)]
    Commons(CommonsCommands),

    /// Charter operations (organizational founding documents)
    #[command(subcommand)]
    Charter(CharterCommands),

    /// Amendment operations (constitutional governance)
    #[command(subcommand)]
    Amendment(AmendmentCommands),

    /// Appeal operations (due process for governance decisions)
    #[command(subcommand)]
    Appeal(AppealCommands),

    /// API schema management
    #[command(subcommand)]
    Api(ApiCommands),

    /// Generate shell completions
    Completions {
        /// Shell type
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
enum ApiCommands {
    /// Export OpenAPI specification to stdout or file
    ExportOpenapi {
        /// Output file path (defaults to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output format: yaml or json
        #[arg(short, long, default_value = "yaml")]
        format: String,
    },
}

#[derive(Subcommand, Debug)]
enum AuthCommands {
    /// Get a JWT token for the Gateway API
    Token {
        /// Gateway API URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,

        /// Cooperative ID
        #[arg(short, long)]
        coop_id: String,

        /// Scopes to request (comma-separated)
        #[arg(
            short,
            long,
            default_value = "ledger:read,ledger:write,coop:read,gov:read,gov:write"
        )]
        scopes: String,
    },
}

#[derive(Subcommand, Debug)]
enum ComputeCommands {
    /// Submit a CCL contract for distributed execution
    Submit {
        /// Path to CCL contract JSON file
        #[arg(short, long)]
        contract: PathBuf,

        /// Task ID (auto-generated if not provided)
        #[arg(short, long)]
        id: Option<String>,

        /// Fuel limit (default 10000)
        #[arg(short, long, default_value = "10000")]
        fuel: u64,

        /// Task priority: low, normal, high, or critical (default: normal)
        #[arg(short = 'P', long, default_value = "normal")]
        priority: String,

        /// Path to inputs JSON file
        #[arg(long)]
        inputs: Option<PathBuf>,

        /// Payment rate per 1000 fuel (optional)
        #[arg(short, long)]
        payment_rate: Option<u64>,

        /// Payment currency (default: credits)
        #[arg(long)]
        payment_currency: Option<String>,
    },

    /// Submit a WebAssembly module for distributed execution
    SubmitWasm {
        /// Path to WASM binary file (.wasm)
        #[arg(short, long)]
        wasm: PathBuf,

        /// Task ID (auto-generated if not provided)
        #[arg(short, long)]
        id: Option<String>,

        /// Fuel limit (default 10000)
        #[arg(short, long, default_value = "10000")]
        fuel: u64,

        /// Task priority: low, normal, high, or critical (default: normal)
        #[arg(short = 'P', long, default_value = "normal")]
        priority: String,

        /// Path to inputs JSON file
        #[arg(long)]
        inputs: Option<PathBuf>,

        /// Payment rate per 1000 fuel (optional)
        #[arg(short, long)]
        payment_rate: Option<u64>,

        /// Payment currency (default: credits)
        #[arg(long)]
        payment_currency: Option<String>,
    },

    /// Check task status
    Status {
        /// Task hash (hex)
        task_hash: String,
    },

    /// Cancel a task
    Cancel {
        /// Task hash (hex)
        task_hash: String,

        /// Cancellation reason
        #[arg(short, long, default_value = "Cancelled by user")]
        reason: String,
    },
}

#[derive(Subcommand, Debug)]
enum PolicyCommands {
    /// Set or update policy for a cooperative
    Set {
        /// Cooperative ID
        #[arg(short, long)]
        coop_id: String,

        /// Path to policy JSON file
        #[arg(short, long)]
        policy: PathBuf,
    },

    /// Show policy for a cooperative
    Show {
        /// Cooperative ID
        #[arg(short, long)]
        coop_id: String,
    },

    /// List all policies
    List,

    /// Remove policy for a cooperative
    Remove {
        /// Cooperative ID
        #[arg(short, long)]
        coop_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum QuotaCommands {
    /// Show resource usage for a member
    Show {
        /// Cooperative ID
        #[arg(short, long)]
        coop_id: String,

        /// Member DID
        #[arg(short, long)]
        member: String,
    },

    /// List usage for all members in a cooperative
    List {
        /// Cooperative ID
        #[arg(short, long)]
        coop_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum IdCommands {
    /// Initialize identity with new keystore
    Init,

    /// Show current identity
    Show,

    /// Rotate to a new key
    Rotate {
        /// Reason for rotation
        #[arg(short, long)]
        reason: Option<String>,
    },

    /// Upgrade identity to post-quantum security
    #[cfg(feature = "post-quantum")]
    UpgradePq,

    /// Export identity (backup)
    Export {
        /// Output file path
        output: PathBuf,
    },

    /// Import identity (restore)
    Import {
        /// Input file path
        input: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum DeviceCommands {
    /// List all devices for this identity
    List,

    /// Create a device-add request for a new device
    Add {
        /// Device name/label
        name: String,

        /// Display QR code in terminal
        #[arg(long)]
        qr: bool,

        /// Save QR code as image file (implies --qr)
        #[arg(long)]
        qr_image: Option<PathBuf>,
    },

    /// Approve a device-add request (from an existing authorized device)
    Approve {
        /// Path to device-add request file
        request_file: PathBuf,
    },

    /// Revoke a device
    Revoke {
        /// Device ID to revoke
        device_id: String,

        /// Reason for revocation
        #[arg(short, long)]
        reason: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum RecoveryCommands {
    /// Configure social recovery trustees
    Setup {
        /// Trustee DIDs (comma-separated)
        trustees: String,

        /// Number of trustees required (M-of-N threshold)
        #[arg(short, long)]
        threshold: usize,

        /// Delay period in seconds before finalization (default: 86400 = 24 hours)
        #[arg(short, long, default_value = "86400")]
        delay: u64,
    },

    /// Show current recovery configuration
    Config,

    /// Initiate recovery for a lost identity
    Initiate {
        /// Old DID to recover
        old_did: String,
    },

    /// Sign a recovery attestation as a trustee
    Attest {
        /// Recovery ID to attest
        recovery_id: String,

        /// How you verified the user's identity
        #[arg(short, long)]
        verification: String,
    },

    /// List all active recovery requests
    List,

    /// Show status of a specific recovery
    Status {
        /// Recovery ID
        recovery_id: String,
    },

    /// Finalize a recovery (after M signatures + delay)
    Finalize {
        /// Recovery ID to finalize
        recovery_id: String,
    },

    /// Cancel a recovery (if fraudulent or device found)
    Cancel {
        /// Recovery ID to cancel
        recovery_id: String,

        /// Reason for cancellation
        #[arg(short, long)]
        reason: String,
    },
}

#[derive(Subcommand, Debug)]
enum TrustCommands {
    /// Add trust edge
    Add {
        /// Target DID
        did: String,

        /// Trust score (0.0-1.0)
        score: f64,

        /// Optional label
        #[arg(short, long)]
        label: Option<String>,
    },

    /// List all trust edges
    List,

    /// Show computed trust score for a DID
    Show {
        /// Target DID
        did: String,
    },

    /// Remove trust edge
    Remove {
        /// Target DID
        did: String,
    },
}

#[derive(Subcommand, Debug)]
enum LedgerCommands {
    /// Show most recent ledger entry
    Head,

    /// Show balance for an account
    Balance {
        /// Account DID
        account_id: String,

        /// Optional currency filter
        #[arg(short, long)]
        currency: Option<String>,
    },

    /// Show recent ledger history
    History {
        /// Number of entries to show (default: 10)
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Quarantine management
    #[command(subcommand)]
    Quarantine(QuarantineCommands),
}

#[derive(Subcommand, Debug)]
enum QuarantineCommands {
    /// List all quarantined entries
    List,

    /// Get detailed info about a quarantined entry
    Get {
        /// Entry ID (content hash)
        entry_id: String,
    },

    /// Release an entry from quarantine (retry)
    Release {
        /// Entry ID (content hash)
        entry_id: String,
    },

    /// Permanently drop an entry from quarantine
    Drop {
        /// Entry ID (content hash)
        entry_id: String,
    },

    /// Purge all expired entries
    Purge,
}

#[derive(Subcommand, Debug)]
enum ContractCommands {
    /// Deploy a contract from JSON file (single-party deployment)
    Deploy {
        /// Path to contract JSON file
        contract_file: PathBuf,
    },

    /// Prepare a contract for multi-party deployment (Phase 10C)
    /// Creates a deployment message with your signature
    Prepare {
        /// Path to contract JSON file
        contract_file: PathBuf,

        /// Output file for partial deployment message
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Add your signature to a partial deployment (Phase 10C)
    Sign {
        /// Path to partial deployment JSON file
        deployment_file: PathBuf,

        /// Output file for updated deployment message
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Deploy a fully-signed contract (Phase 10C)
    DeploySigned {
        /// Path to fully-signed deployment JSON file
        deployment_file: PathBuf,
    },

    /// Call a contract rule
    Call {
        /// Contract code hash (hex)
        code_hash: String,

        /// Rule name to execute
        rule_name: String,

        /// Caller DID
        caller: String,

        /// Arguments as JSON
        #[arg(short, long)]
        args: Option<String>,
    },

    /// List deployed contracts
    List,
}

#[derive(Subcommand, Debug)]
enum NetworkCommands {
    /// List discovered peers (via mDNS)
    Peers,

    /// Connect to a peer via QUIC
    Dial {
        /// Target DID
        did: String,

        /// Optional socket address (IP:port)
        #[arg(short, long)]
        addr: Option<String>,
    },

    /// Show network statistics
    Stats,

    /// Show network actor status
    Status,
}

#[derive(Subcommand, Debug)]
enum FederationCommands {
    /// Show current federation status
    Status,

    /// List configured bootstrap peers
    Peers,

    /// Add a bootstrap peer
    Add {
        /// Peer URL in format: icn://did:icn:PUBKEY@IP:PORT
        peer_url: String,

        /// Initial trust score for this peer (0.0-1.0)
        #[arg(short, long, default_value = "0.3")]
        trust: f64,
    },

    /// Remove a bootstrap peer
    Remove {
        /// DID of the peer to remove
        did: String,
    },

    /// Connect to a bootstrap peer immediately
    Connect {
        /// Peer URL in format: icn://did:icn:PUBKEY@IP:PORT
        peer_url: String,
    },

    /// Show federation configuration
    Config,

    /// Set federation configuration options
    Set {
        /// Configuration key (e.g., enabled, network_name, bootstrap_peer_trust)
        key: String,

        /// Configuration value
        value: String,
    },

    /// Generate a federation invite URL for this node
    Invite,

    /// Connect to a peer via the gateway API (registers peer cooperative)
    GatewayConnect {
        /// Peer address in host:port format (e.g., "node-b.local:9000")
        address: String,

        /// DID of the remote cooperative (required)
        #[arg(long)]
        peer_did: String,

        /// Optional cooperative ID for the peer
        #[arg(long)]
        coop_id: Option<String>,

        /// Optional human-readable name for the peer
        #[arg(long)]
        name: Option<String>,

        /// Gateway URL (defaults to ICN_GATEWAY or http://localhost:8080)
        #[arg(long)]
        gateway: Option<String>,

        /// Bearer token (defaults to ICN_TOKEN env var)
        #[arg(long)]
        token: Option<String>,
    },

    /// Cooperative registry management (inter-coop federation)
    #[command(subcommand)]
    Coop(CoopCommands),

    /// Vouch for cooperatives (trust bridging)
    #[command(subcommand)]
    Vouch(VouchCommands),

    /// Trust attestation management
    #[command(subcommand)]
    Attestation(AttestationCommands),

    /// Bilateral clearing agreements (credit settlement)
    #[command(subcommand)]
    Clearing(ClearingCommands),
}

#[derive(Subcommand, Debug)]
enum CoopCommands {
    /// List known cooperatives
    List,

    /// Show details of a cooperative
    Show {
        /// Cooperative ID (e.g., "food-coop")
        coop_id: String,
    },

    /// Register this node's cooperative with the federation
    Register {
        /// Cooperative ID (unique identifier)
        #[arg(long)]
        coop_id: String,

        /// Human-readable name
        #[arg(long)]
        name: String,

        /// Federation gateway endpoint (e.g., "https://food-coop.example.com:8080")
        #[arg(long)]
        gateway: String,

        /// Description (optional)
        #[arg(long)]
        description: Option<String>,
    },

    /// Update cooperative information
    Update {
        /// Cooperative ID
        coop_id: String,

        /// New gateway endpoint
        #[arg(long)]
        gateway: Option<String>,

        /// New description
        #[arg(long)]
        description: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum VouchCommands {
    /// Vouch for another cooperative
    Issue {
        /// Target cooperative ID to vouch for
        #[arg(long)]
        target_coop: String,

        /// Trust score to assign (0.0-1.0)
        #[arg(long)]
        trust: f64,

        /// Validity duration in days (default: 365)
        #[arg(long, default_value = "365")]
        days: u64,
    },

    /// List vouches we have issued
    List,

    /// Revoke a vouch
    Revoke {
        /// Cooperative ID whose vouch to revoke
        target_coop: String,
    },
}

#[derive(Subcommand, Debug)]
enum AttestationCommands {
    /// List attestations for a member
    List {
        /// Member DID to look up attestations for
        member_did: String,
    },

    /// Show details of attestations from a cooperative
    From {
        /// Source cooperative ID
        coop_id: String,
    },

    /// Issue a federated trust attestation
    Issue {
        /// Member DID to attest
        #[arg(long)]
        member_did: String,

        /// Trust score (0.0-1.0)
        #[arg(long)]
        trust: f64,

        /// Trust context (economic, governance, technical, social)
        #[arg(long, default_value = "economic")]
        context: String,

        /// Validity duration in days (default: 30)
        #[arg(long, default_value = "30")]
        days: u64,
    },
}

#[derive(Subcommand, Debug)]
enum ClearingCommands {
    /// List clearing agreements
    List,

    /// Show details of a clearing agreement
    Show {
        /// Agreement ID
        agreement_id: String,
    },

    /// Create a new bilateral clearing agreement
    Create {
        /// Agreement ID (unique identifier)
        #[arg(long)]
        agreement_id: String,

        /// Partner cooperative ID
        #[arg(long)]
        partner_coop: String,

        /// Maximum imbalance allowed (credit units)
        #[arg(long, default_value = "10000")]
        max_imbalance: i64,

        /// Settlement interval (daily, weekly, monthly, manual)
        #[arg(long, default_value = "monthly")]
        settlement: String,
    },

    /// Add an exchange rate to an agreement
    Rate {
        /// Agreement ID
        #[arg(long)]
        agreement_id: String,

        /// Source currency (e.g., "hours")
        #[arg(long)]
        from: String,

        /// Destination currency (e.g., "USD")
        #[arg(long)]
        to: String,

        /// Exchange rate
        #[arg(long)]
        rate: f64,
    },

    /// Show clearing position for an agreement
    Position {
        /// Agreement ID
        agreement_id: String,
    },

    /// Trigger settlement for an agreement
    Settle {
        /// Agreement ID
        agreement_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum GovCommands {
    /// Domain management
    #[command(subcommand)]
    Domain(DomainCommands),

    /// Proposal management
    #[command(subcommand)]
    Proposal(ProposalCommands),

    /// Vote on proposals
    #[command(subcommand)]
    Vote(VoteCommands),
}

#[derive(Subcommand, Debug)]
enum DomainCommands {
    /// Create a new governance domain
    Create {
        /// Domain ID (e.g., "coop:tiny-food")
        #[arg(long)]
        domain_id: String,

        /// Human-readable name
        #[arg(long)]
        name: String,

        /// Comma-separated member DIDs
        #[arg(long)]
        members: String,

        /// Governance profile (default: cooperative_default)
        #[arg(long, default_value = "cooperative_default")]
        profile: String,

        /// Quorum percentage (0-100)
        #[arg(long, default_value = "50")]
        quorum: u8,

        /// Approval threshold percentage (0-100)
        #[arg(long, default_value = "50")]
        approval: u8,

        /// Voting period in seconds (default: 7 days)
        #[arg(long, default_value = "604800")]
        voting_period: u64,
    },

    /// Show domain details
    Show {
        /// Domain ID
        #[arg(long)]
        domain_id: String,
    },

    /// List all domains
    List,

    /// Add a member to a governance domain
    AddMember {
        /// Domain ID
        #[arg(long)]
        domain_id: String,

        /// DID of the member to add
        #[arg(long)]
        did: String,

        /// Gateway URL (defaults to ICN_GATEWAY or http://localhost:8080)
        #[arg(long)]
        gateway: Option<String>,

        /// Bearer token (defaults to ICN_TOKEN env var)
        #[arg(long)]
        token: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
enum ProposalCommands {
    /// Create a new proposal
    Create {
        /// Domain ID
        #[arg(long)]
        domain_id: String,

        /// Proposal title
        #[arg(long)]
        title: String,

        /// Proposal description
        #[arg(long)]
        description: String,

        /// Proposal type (text, budget, membership, config-change)
        #[arg(long)]
        kind: String,

        /// For text proposals: body content
        #[arg(long)]
        body: Option<String>,

        /// For budget proposals: amount
        #[arg(long)]
        amount: Option<i64>,

        /// For budget proposals: currency
        #[arg(long)]
        currency: Option<String>,

        /// For budget proposals: recipient DID
        #[arg(long)]
        recipient: Option<String>,

        /// For budget proposals: purpose
        #[arg(long)]
        purpose: Option<String>,

        /// For membership proposals: member DID
        #[arg(long)]
        member: Option<String>,

        /// For membership proposals: action (add/remove)
        #[arg(long)]
        action: Option<String>,

        /// For config-change proposals: new config JSON
        #[arg(long)]
        new_config: Option<String>,
    },

    /// Open a proposal for voting
    Open {
        /// Proposal ID
        #[arg(long)]
        proposal_id: String,

        /// Optional voting duration override (seconds)
        #[arg(long)]
        duration: Option<u64>,
    },

    /// List proposals in a domain
    List {
        /// Domain ID
        #[arg(long)]
        domain_id: String,

        /// Optional state filter (draft, open, accepted, rejected, noquorum, cancelled)
        #[arg(long)]
        state: Option<String>,
    },

    /// Show proposal details with current tally
    Show {
        /// Proposal ID
        #[arg(long)]
        proposal_id: String,
    },

    /// Close a proposal and compute outcome
    Close {
        /// Proposal ID
        #[arg(long)]
        proposal_id: String,
    },

    /// Cancel a proposal
    Cancel {
        /// Proposal ID
        #[arg(long)]
        proposal_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum VoteCommands {
    /// Cast or update a vote
    Cast {
        /// Proposal ID
        #[arg(long)]
        proposal_id: String,

        /// Vote choice (for, against, abstain)
        #[arg(long)]
        choice: String,

        /// Optional comment
        #[arg(long)]
        comment: Option<String>,
    },

    /// Show your vote on a proposal
    Show {
        /// Proposal ID
        #[arg(long)]
        proposal_id: String,
    },

    /// Delegate your voting power to another member
    Delegate {
        /// DID of the delegate (who will vote on your behalf)
        #[arg(long)]
        delegate: String,

        /// Scope of delegation: "blanket", "domain:<id>", or "proposal:<id>"
        #[arg(long, default_value = "blanket")]
        scope: String,

        /// Expiry duration (e.g., "7d", "30d", "1y")
        #[arg(long)]
        expires: Option<String>,
    },

    /// List your active delegations
    Delegations,

    /// Revoke a delegation
    Revoke {
        /// Delegation ID to revoke
        #[arg(long)]
        delegation_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum SnapshotCommands {
    /// Create a manual snapshot
    Create,

    /// List all available snapshots
    List,

    /// Verify snapshot integrity
    Verify {
        /// Snapshot filename (optional, defaults to state.snapshot)
        snapshot: Option<String>,
    },

    /// Delete a snapshot
    Delete {
        /// Snapshot filename to delete
        snapshot: String,
    },

    /// Delete all old snapshots
    Cleanup {
        /// Number of snapshots to keep (default: 3)
        #[arg(short, long, default_value = "3")]
        keep: usize,
    },
}

#[derive(Subcommand, Debug)]
enum DisputeCommands {
    /// File a dispute against a ledger entry
    File {
        /// Entry hash (hex) to dispute
        #[arg(long)]
        entry_hash: String,

        /// Reason for the dispute
        #[arg(short, long)]
        reason: String,
    },

    /// List disputes
    List {
        /// Filter by status: pending, resolved, all (default: pending)
        #[arg(short, long, default_value = "pending")]
        status: String,

        /// Filter by filer DID
        #[arg(short, long)]
        filer: Option<String>,
    },

    /// Get details of a specific dispute
    Get {
        /// Entry hash (hex) of the disputed entry
        entry_hash: String,
    },

    /// Add evidence to a dispute
    AddEvidence {
        /// Entry hash (hex) of the disputed entry
        #[arg(long)]
        entry_hash: String,

        /// Evidence text to add
        #[arg(short, long)]
        evidence: String,
    },

    /// Assign a mediator to a dispute
    AssignMediator {
        /// Entry hash (hex) of the disputed entry
        #[arg(long)]
        entry_hash: String,

        /// Mediator DID
        #[arg(short, long)]
        mediator: String,
    },

    /// Resolve a dispute (requires mediator role)
    Resolve {
        /// Entry hash (hex) of the disputed entry
        #[arg(long)]
        entry_hash: String,

        /// Resolution outcome: upheld, reversed, settlement, writeoff
        #[arg(short, long)]
        outcome: String,
    },
}

#[derive(Subcommand, Debug)]
enum StewardCommands {
    /// Show steward status and statistics
    Status,

    /// Show steward configuration
    Config,

    /// Get steward info by ID or DID
    Info {
        /// Steward ID or DID
        steward: String,
        /// Gateway URL (defaults to ICN_GATEWAY env or http://localhost:8080)
        #[arg(long, short)]
        gateway: Option<String>,
    },

    /// List registered stewards
    List {
        /// Show only active stewards
        #[arg(long)]
        active: bool,
        /// Filter by jurisdiction
        #[arg(long)]
        jurisdiction: Option<String>,
        /// Gateway URL
        #[arg(long, short)]
        gateway: Option<String>,
    },

    /// List stewards who can issue attestations
    Attesters {
        /// Gateway URL
        #[arg(long, short)]
        gateway: Option<String>,
    },

    /// Register as a steward (requires Strong POP level)
    Register {
        /// Term duration in days (30-730)
        #[arg(long, default_value = "365")]
        term_days: u64,
        /// Bond amount in credits
        #[arg(long, default_value = "1000")]
        bond: u64,
        /// Governance proposal ID that approved this registration
        #[arg(long)]
        governance_approval: String,
        /// Optional jurisdiction scope
        #[arg(long)]
        jurisdiction: Option<String>,
        /// Specializations (comma-separated, e.g., "identity,mediation")
        #[arg(long)]
        specializations: Option<String>,
        /// Gateway URL
        #[arg(long, short)]
        gateway: Option<String>,
    },

    /// Retire from stewardship (self-service)
    Retire {
        /// Steward ID (defaults to current user's steward record)
        steward_id: Option<String>,
        /// Gateway URL
        #[arg(long, short)]
        gateway: Option<String>,
    },

    /// Check if a VUI is already registered
    CheckVui {
        /// VUI hash (hex, 32 bytes)
        vui_hash: String,
    },

    /// Start an enrollment ceremony
    StartEnrollment {
        /// VUI commitment (hex, 32 bytes)
        #[arg(long)]
        vui_commitment: String,

        /// Biometric pathway hash (hex, 8 bytes)
        #[arg(long)]
        pathway_hash: String,
    },

    /// Get enrollment ceremony status
    EnrollmentStatus {
        /// Ceremony ID (hex, 32 bytes)
        ceremony_id: String,
    },

    /// Start a recovery ceremony
    StartRecovery {
        /// Old DID being recovered
        #[arg(long)]
        old_did: String,

        /// New DID to replace the old one
        #[arg(long)]
        new_did: String,

        /// Evidence hash (hex, 32 bytes)
        #[arg(long)]
        evidence_hash: String,

        /// Anchor commitment (hex, 32 bytes)
        #[arg(long)]
        anchor_commitment: String,
    },

    /// Get recovery ceremony status
    RecoveryStatus {
        /// Ceremony ID (hex, 32 bytes)
        ceremony_id: String,
    },

    /// Issue an enrollment token (steward only)
    IssueToken {
        /// VUI commitment (hex, 32 bytes)
        #[arg(long)]
        vui_commitment: String,

        /// Blinded message (hex)
        #[arg(long)]
        blinded_message: String,
    },

    /// List steward gossip topics
    Topics,
}

#[derive(Subcommand, Debug)]
enum CommonsCommands {
    /// Show commons holder status for current identity
    Status,

    /// Begin enrollment as a Commons Holder
    Enroll {
        /// Gateway URL for enrollment
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,

        /// Cooperative/jurisdiction to enroll with
        #[arg(short, long)]
        coop_id: String,
    },

    /// Show PersonhoodAnchor details
    Anchor {
        /// DID to look up (defaults to current identity)
        #[arg(short, long)]
        did: Option<String>,
    },

    /// List affiliations for a commons holder
    Affiliations {
        /// DID to look up (defaults to current identity)
        #[arg(short, long)]
        did: Option<String>,
    },

    /// Request to join a jurisdiction
    Join {
        /// Jurisdiction ID (e.g., coop:mycoop, federation:pacific-nw)
        #[arg(short, long)]
        jurisdiction: String,

        /// Gateway URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// Leave a jurisdiction
    Leave {
        /// Jurisdiction ID to leave
        #[arg(short, long)]
        jurisdiction: String,

        /// Gateway URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,
    },
}

#[derive(Subcommand, Debug)]
enum CharterCommands {
    /// Create a new organizational charter
    Create {
        /// Organization name
        #[arg(short, long)]
        name: String,

        /// Organization type: cooperative, community, federation, or network
        #[arg(short = 't', long, default_value = "cooperative")]
        org_type: String,

        /// Governance domain ID
        #[arg(short, long)]
        domain: String,

        /// Mission statement
        #[arg(short, long)]
        mission: Option<String>,

        /// Initial member DIDs (comma-separated)
        #[arg(long)]
        founders: Option<String>,

        /// Output charter to file (JSON)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Show charter details
    Show {
        /// Charter ID (hex)
        charter_id: String,

        /// Gateway URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// List charters
    List {
        /// Filter by organization type
        #[arg(short = 't', long)]
        org_type: Option<String>,

        /// Filter by status (draft, ratified, suspended, dissolved)
        #[arg(short, long)]
        status: Option<String>,

        /// Gateway URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// Sign a charter as a founder
    Sign {
        /// Charter ID (hex)
        charter_id: String,

        /// Cooperative/domain ID for authentication context
        #[arg(short, long)]
        coop_id: String,

        /// Gateway URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,

        /// Role of the founder (e.g., "founder", "officer", "advisor")
        #[arg(short, long, default_value = "founder")]
        role: String,
    },

    /// Ratify (activate) a charter (requires sufficient founder signatures)
    Ratify {
        /// Charter ID (hex)
        charter_id: String,

        /// Cooperative/domain ID for authentication context
        #[arg(short, long)]
        coop_id: String,

        /// Gateway URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,
    },
}

#[derive(Subcommand, Debug)]
enum AmendmentCommands {
    /// Propose a new amendment
    Propose {
        /// Amendment title
        #[arg(long)]
        title: String,

        /// Amendment description
        #[arg(long)]
        description: String,

        /// Amendment type: charter, constitutional, policy, economic, governance
        #[arg(short = 't', long, default_value = "policy")]
        amendment_type: String,

        /// Scope type: jurisdiction, federation, network
        #[arg(short = 's', long, default_value = "jurisdiction")]
        scope: String,

        /// Scope ID (jurisdiction or federation ID)
        #[arg(long)]
        scope_id: Option<String>,

        /// Charter ID (required for charter amendments)
        #[arg(long)]
        charter_id: Option<String>,

        /// Cooperative ID for authentication
        #[arg(short, long, env = "ICN_COOP_ID")]
        coop_id: String,

        /// Gateway URL
        #[arg(long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// List amendments
    List {
        /// Filter by status: draft, submitted, under_review, voting, ratified, rejected
        #[arg(short, long)]
        status: Option<String>,

        /// Filter by scope: jurisdiction, federation, network
        #[arg(long)]
        scope: Option<String>,

        /// Filter by amendment type
        #[arg(short = 't', long)]
        amendment_type: Option<String>,

        /// Gateway URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// Show amendment details
    Show {
        /// Amendment ID
        amendment_id: String,

        /// Gateway URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// Submit an amendment for review
    Submit {
        /// Amendment ID
        amendment_id: String,

        /// Cooperative ID for authentication
        #[arg(short, long, env = "ICN_COOP_ID")]
        coop_id: String,

        /// Gateway URL
        #[arg(long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// Open voting on an amendment (after review period)
    OpenVoting {
        /// Amendment ID
        amendment_id: String,

        /// Cooperative ID for authentication
        #[arg(short, long, env = "ICN_COOP_ID")]
        coop_id: String,

        /// Gateway URL
        #[arg(long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// Vote on an amendment (as a ratifier)
    Vote {
        /// Amendment ID
        amendment_id: String,

        /// Approve the amendment
        #[arg(long, group = "vote_choice")]
        approve: bool,

        /// Reject the amendment
        #[arg(long, group = "vote_choice")]
        reject: bool,

        /// Optional comment with your vote
        #[arg(long)]
        comment: Option<String>,

        /// Cooperative ID for authentication
        #[arg(short, long, env = "ICN_COOP_ID")]
        coop_id: String,

        /// Gateway URL
        #[arg(long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// Withdraw an amendment (proposer only)
    Withdraw {
        /// Amendment ID
        amendment_id: String,

        /// Reason for withdrawal
        #[arg(short, long)]
        reason: Option<String>,

        /// Cooperative ID for authentication
        #[arg(long, env = "ICN_COOP_ID")]
        coop_id: String,

        /// Gateway URL
        #[arg(long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// Add a change to a draft amendment
    AddChange {
        /// Amendment ID
        amendment_id: String,

        /// Target of the change: governance_rules, membership_policy, economic_policy, rights, charter_article, or custom name
        #[arg(short, long)]
        target: String,

        /// Change type: add, modify, remove, replace
        #[arg(short = 'k', long, default_value = "modify")]
        change_type: String,

        /// Description of the change
        #[arg(short, long)]
        description: String,

        /// New value (JSON or text)
        #[arg(short, long)]
        new_value: String,

        /// Old value (for modify/replace)
        #[arg(short, long)]
        old_value: Option<String>,

        /// Cooperative/domain ID for authentication context
        #[arg(short, long)]
        coop_id: String,

        /// Gateway URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,
    },
}

#[derive(Subcommand, Debug)]
enum AppealCommands {
    /// File a new appeal
    File {
        /// Appeal type: revocation, suspension, governance, dispute, membership, steward
        #[arg(short = 't', long)]
        appeal_type: String,

        /// Target ID (revocation ID, proposal ID, etc.)
        #[arg(long)]
        target_id: String,

        /// Scope: jurisdiction, federation, network
        #[arg(short = 's', long, default_value = "jurisdiction")]
        scope: String,

        /// Scope ID (jurisdiction or federation ID)
        #[arg(long)]
        scope_id: Option<String>,

        /// Statement explaining the appeal
        #[arg(long)]
        statement: String,

        /// Grounds for appeal (comma-separated): procedural, factual, proportionality, new_evidence, rights_violation
        #[arg(long)]
        grounds: String,

        /// Requested remedy: reverse, reinstate, modify, compensation
        #[arg(long, default_value = "reverse")]
        remedy: String,

        /// Cooperative ID for authentication
        #[arg(short, long, env = "ICN_COOP_ID")]
        coop_id: String,

        /// Gateway URL
        #[arg(long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// List appeals
    List {
        /// Filter by status: filed, under_review, hearing, resolved, dismissed, withdrawn
        #[arg(short = 's', long)]
        status: Option<String>,

        /// Filter by appeal type
        #[arg(short = 't', long)]
        appeal_type: Option<String>,

        /// Filter by appellant DID
        #[arg(long)]
        appellant: Option<String>,

        /// Gateway URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// Show appeal details
    Show {
        /// Appeal ID
        appeal_id: String,

        /// Gateway URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// Add evidence to an appeal
    AddEvidence {
        /// Appeal ID
        appeal_id: String,

        /// Evidence type: document, testimony, record, communication, other
        #[arg(short = 't', long, default_value = "document")]
        evidence_type: String,

        /// Description of the evidence
        #[arg(long)]
        description: String,

        /// Content hash (for off-chain evidence)
        #[arg(long)]
        content_hash: Option<String>,

        /// URI to evidence location
        #[arg(long)]
        uri: Option<String>,

        /// Cooperative ID for authentication
        #[arg(short, long, env = "ICN_COOP_ID")]
        coop_id: String,

        /// Gateway URL
        #[arg(long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// Respond to an appeal (as respondent)
    Respond {
        /// Appeal ID
        appeal_id: String,

        /// Response type: answer, objection, motion, evidence
        #[arg(short = 't', long, default_value = "answer")]
        response_type: String,

        /// Response content
        #[arg(long)]
        content: String,

        /// Cooperative ID for authentication
        #[arg(short, long, env = "ICN_COOP_ID")]
        coop_id: String,

        /// Gateway URL
        #[arg(long, default_value = "http://localhost:8080")]
        gateway: String,
    },

    /// Withdraw an appeal
    Withdraw {
        /// Appeal ID
        appeal_id: String,

        /// Reason for withdrawal
        #[arg(short, long)]
        reason: Option<String>,

        /// Cooperative ID for authentication
        #[arg(long, env = "ICN_COOP_ID")]
        coop_id: String,

        /// Gateway URL
        #[arg(long, default_value = "http://localhost:8080")]
        gateway: String,
    },
}

fn get_data_dir(data_dir: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = data_dir {
        Ok(dir)
    } else {
        // Default to ~/.icn
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(".icn"))
    }
}

fn get_keystore_path(data_dir: &Path) -> PathBuf {
    data_dir.join("identity.age")
}

fn get_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store")
}

/// Create an RPC client, optionally with authentication
///
/// Tries to authenticate if:
/// 1. The keystore exists
/// 2. ICN_PASSPHRASE env var is set (non-interactive)
///
/// For interactive use, the client starts unauthenticated and will prompt
/// if needed on auth failure.
fn create_rpc_client(
    endpoint: &str,
    data_dir: &Path,
    require_auth: bool,
) -> Result<icn_rpc::RpcClient> {
    let rpc_addr: std::net::SocketAddr = endpoint
        .parse()
        .with_context(|| format!("Invalid RPC endpoint: {endpoint}"))?;

    let keystore_path = get_keystore_path(data_dir);

    // Check if we can/should authenticate
    let passphrase_available = std::env::var("ICN_PASSPHRASE").is_ok();
    let keystore_exists = keystore_path.exists();

    if require_auth && !keystore_exists {
        bail!("Authentication required but no identity found. Run 'icnctl id init' first.");
    }

    // Use authenticated client if passphrase is available and keystore exists
    if passphrase_available && keystore_exists {
        // SAFETY: passphrase_available is true, so ICN_PASSPHRASE env var exists
        #[allow(clippy::unwrap_used)]
        let passphrase = std::env::var("ICN_PASSPHRASE").unwrap();
        let mut keystore = AgeKeyStore::open(&keystore_path)?;
        keystore.unlock(passphrase.as_bytes())?;
        let keypair = keystore.get_keypair()?;
        Ok(icn_rpc::RpcClient::with_credentials(
            rpc_addr,
            std::sync::Arc::new(keypair.clone()),
        ))
    } else {
        // Unauthenticated client - works in dev mode
        Ok(icn_rpc::RpcClient::new(rpc_addr))
    }
}

/// Create an RPC client with authentication (prompts for passphrase)
fn create_authenticated_rpc_client(endpoint: &str, data_dir: &Path) -> Result<icn_rpc::RpcClient> {
    let rpc_addr: std::net::SocketAddr = endpoint
        .parse()
        .with_context(|| format!("Invalid RPC endpoint: {endpoint}"))?;

    let keystore_path = get_keystore_path(data_dir);
    if !keystore_path.exists() {
        bail!("No identity found. Run 'icnctl id init' first.");
    }

    let passphrase = read_passphrase("Enter passphrase for RPC authentication: ")?;
    let mut keystore = AgeKeyStore::open(&keystore_path)?;
    keystore.unlock(&passphrase)?;
    let keypair = keystore.get_keypair()?;

    Ok(icn_rpc::RpcClient::with_credentials(
        rpc_addr,
        std::sync::Arc::new(keypair.clone()),
    ))
}

fn read_passphrase(prompt: &str) -> Result<Vec<u8>> {
    // Check for ICN_PASSPHRASE environment variable first
    if let Ok(passphrase) = std::env::var("ICN_PASSPHRASE") {
        return Ok(passphrase.into_bytes());
    }

    print!("{prompt}");
    io::stdout().flush()?;
    let passphrase = rpassword::read_password()?;
    Ok(passphrase.into_bytes())
}

fn confirm_passphrase() -> Result<Vec<u8>> {
    // If ICN_PASSPHRASE is set, use it without confirmation
    if let Ok(passphrase) = std::env::var("ICN_PASSPHRASE") {
        return Ok(passphrase.into_bytes());
    }

    let pass1 = read_passphrase("Enter passphrase: ")?;
    let pass2 = read_passphrase("Confirm passphrase: ")?;

    if pass1 != pass2 {
        bail!("Passphrases do not match");
    }

    Ok(pass1)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize locale from environment or system
    let locale = std::env::var("ICN_LOCALE")
        .ok()
        .or_else(sys_locale::get_locale)
        .unwrap_or_else(|| "en".to_string());
    rust_i18n::set_locale(&locale);

    // Initialize simple logging
    icn_obs::init()?;

    let data_dir = get_data_dir(args.data_dir)?;

    match args.command {
        Commands::Status => {
            handle_status_command(&data_dir, &args.endpoint).await?;
        }

        Commands::Id(id_cmd) => handle_id_command(id_cmd, &data_dir)?,

        Commands::Device(device_cmd) => handle_device_command(device_cmd, &data_dir)?,

        Commands::Recovery(recovery_cmd) => {
            handle_recovery_command(recovery_cmd, &data_dir, &args.endpoint).await?
        }

        Commands::Trust(trust_cmd) => {
            handle_trust_command(trust_cmd, &data_dir, &args.endpoint).await?
        }

        Commands::Ledger(ledger_cmd) => handle_ledger_command(ledger_cmd, &args.endpoint)?,

        Commands::Contract(contract_cmd) => {
            handle_contract_command(contract_cmd, &args.endpoint, &data_dir).await?
        }

        Commands::Network(net_cmd) => handle_network_command(net_cmd, &args.endpoint)?,

        Commands::Federation(fed_cmd) => {
            handle_federation_command(fed_cmd, &data_dir, &args.endpoint).await?
        }

        Commands::Gov(gov_cmd) => handle_gov_command(gov_cmd, &data_dir, &args.endpoint).await?,

        Commands::Snapshot(snapshot_cmd) => handle_snapshot_command(snapshot_cmd, &data_dir)?,

        Commands::Backup { output } => handle_backup_command(&data_dir, &output)?,

        Commands::Restore { input, force } => handle_restore_command(&data_dir, &input, force)?,

        Commands::VerifyBackup {
            input,
            verify_ledger,
        } => handle_verify_backup_command(&input, verify_ledger)?,

        Commands::InitCoop {
            name,
            members,
            yes,
            no_start,
        } => handle_init_coop_command(&data_dir, name, members, yes, no_start).await?,

        Commands::Auth(auth_cmd) => handle_auth_command(auth_cmd, &data_dir).await?,

        Commands::Compute(compute_cmd) => {
            handle_compute_command(compute_cmd, &data_dir, &args.endpoint).await?
        }

        Commands::Policy(policy_cmd) => {
            handle_policy_command(policy_cmd, &data_dir, &args.endpoint).await?
        }

        Commands::Quota(quota_cmd) => {
            handle_quota_command(quota_cmd, &data_dir, &args.endpoint).await?
        }

        Commands::Dispute(dispute_cmd) => {
            handle_dispute_command(dispute_cmd, &args.endpoint).await?
        }

        Commands::Steward(steward_cmd) => {
            handle_steward_command(steward_cmd, &data_dir, &args.endpoint).await?
        }

        Commands::Commons(commons_cmd) => {
            handle_commons_command(commons_cmd, &data_dir, &args.endpoint).await?
        }

        Commands::Charter(charter_cmd) => {
            handle_charter_command(charter_cmd, &data_dir, &args.endpoint).await?
        }

        Commands::Amendment(amendment_cmd) => {
            handle_amendment_command(amendment_cmd, &data_dir, &args.endpoint).await?
        }

        Commands::Appeal(appeal_cmd) => {
            handle_appeal_command(appeal_cmd, &data_dir, &args.endpoint).await?
        }

        Commands::Api(api_cmd) => handle_api_command(api_cmd)?,

        Commands::Completions { shell } => {
            let mut cmd = Args::command();
            let bin_name = cmd.get_name().to_string();
            generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
        }
    }

    Ok(())
}

fn handle_id_command(cmd: IdCommands, data_dir: &Path) -> Result<()> {
    let keystore_path = get_keystore_path(data_dir);

    match cmd {
        IdCommands::Init => {
            // Check if keystore already exists
            if keystore_path.exists() {
                bail!(
                    "{} {}",
                    t!(
                        "cli.id.init.already_exists",
                        path = keystore_path.display().to_string()
                    ),
                    "Use 'id show' to view it."
                );
            }

            println!("{}\n", t!("cli.id.init.starting"));

            // Get passphrase
            let passphrase = confirm_passphrase()?;

            // Create data directory if needed
            std::fs::create_dir_all(data_dir).context("Failed to create data directory")?;

            // Initialize keystore (generates keypair internally)
            println!("\n{}...", t!("cli.id.init.generating"));
            let keystore = AgeKeyStore::init(&keystore_path, &passphrase)?;

            println!("\n✓ {}", t!("cli.id.init.success"));
            println!(
                "  {}: {}",
                t!("cli.id.init.did_label"),
                keystore.get_keypair()?.did()
            );
            println!(
                "  {}: {}",
                t!("cli.id.init.keystore_label"),
                keystore_path.display()
            );
            println!("\n{}", t!("cli.id.init.important"));
        }

        IdCommands::Show => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("{}", t!("cli.common.no_identity"));
            }

            // Get passphrase
            let passphrase = read_passphrase(&t!("cli.prompts.enter_passphrase"))?;

            // Open and unlock keystore
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            let keypair = keystore.get_keypair()?;
            println!("{}", t!("cli.id.show.title"));
            println!("{} {}", t!("cli.id.show.did_label"), keypair.did());
            println!(
                "{} {}",
                t!("cli.id.show.keystore_label"),
                keystore_path.display()
            );
        }

        IdCommands::Rotate { reason } => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("{}", t!("cli.common.no_identity"));
            }

            println!("{}\n", t!("cli.id.rotate.starting"));

            // Get passphrase
            let passphrase = read_passphrase(&t!("cli.prompts.enter_passphrase"))?;

            // Open and unlock keystore
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            let old_did = keystore.get_keypair()?.did().clone();

            // Generate new keypair
            println!("{}...", t!("cli.id.rotate.generating"));
            let new_keypair = KeyPair::generate()?;
            let new_did = new_keypair.did().clone();

            // Perform rotation
            let rotation = keystore.rotate(new_keypair)?;

            println!("\n✓ {}", t!("cli.id.rotate.success"));
            println!("  {}: {old_did}", t!("cli.id.rotate.old_did"));
            println!("  {}: {new_did}", t!("cli.id.rotate.new_did"));
            if let Some(r) = reason {
                println!("  {}: {r}", t!("cli.id.rotate.reason"));
            } else {
                println!("  {}: {:?}", t!("cli.id.rotate.reason"), rotation.reason);
            }
            println!("\n{}", t!("cli.id.rotate.important"));
            println!(
                "  {}: {}",
                t!("cli.id.rotate.timestamp"),
                rotation.timestamp
            );
        }

        #[cfg(feature = "post-quantum")]
        IdCommands::UpgradePq => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' to create one.");
            }

            println!("Upgrading identity to post-quantum security...\n");
            println!("This will add ML-DSA (Dilithium3) signing keys and ML-KEM (Kyber768) encryption keys.");
            println!("Your DID will remain the same, but cryptography will be hybrid:\n");
            println!("  Signatures: Ed25519 + ML-DSA (both required)");
            println!("  Encryption: X25519 + ML-KEM (combined via HKDF)\n");

            // Get passphrase
            let passphrase = read_passphrase("Enter passphrase: ")?;

            // Open and unlock keystore
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            println!("Generating post-quantum keys (this may take a moment)...");

            // Upgrade to PQ (generates ML-DSA + ML-KEM keys and saves to disk)
            let did = keystore.upgrade_to_pq(&passphrase)?;

            println!("\n✓ Post-quantum upgrade successful!");
            println!("  DID: {did} (unchanged)");
            println!("  Signatures: Ed25519 (64B) + ML-DSA-65 (~3.3KB)");
            println!("  Encryption: X25519 (32B) + ML-KEM-768 (~1.1KB ciphertext)");
            println!("  Security: Hybrid (both algorithms required)");
            println!("\nAll future operations will use hybrid cryptography.");
            println!("IMPORTANT: Backup your upgraded keystore!");
        }

        IdCommands::Export { output } => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!(
                    "No identity found at {}. Run 'id init' first.",
                    keystore_path.display()
                );
            }

            // Check if output file already exists
            if output.exists() {
                bail!(
                    "Output file already exists: {}. Remove it first or choose a different path.",
                    output.display()
                );
            }

            // Verify passphrase before allowing export
            let passphrase = read_passphrase("Enter passphrase to authorize export: ")?;

            // Open and unlock keystore to verify ownership
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore
                .unlock(&passphrase)
                .context("Failed to unlock keystore. Incorrect passphrase.")?;

            // Get DID for export confirmation
            let did = keystore.get_keypair()?.did().clone();

            // Create output directory if needed
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent).context("Failed to create output directory")?;
            }

            // Copy the encrypted keystore to output
            std::fs::copy(&keystore_path, &output)
                .with_context(|| format!("Failed to export keystore to {}", output.display()))?;

            println!("✓ Identity exported successfully!");
            println!("  DID:  {did}");
            println!("  From: {}", keystore_path.display());
            println!("  To:   {}", output.display());
            println!("\nIMPORTANT:");
            println!("  • Store this file securely");
            println!("  • The file is encrypted with your passphrase");
            println!("  • You'll need the same passphrase to import it");
        }

        IdCommands::Import { input } => {
            // Check if input file exists
            if !input.exists() {
                bail!("Input file not found: {}", input.display());
            }

            // Check if target keystore already exists
            if keystore_path.exists() {
                // Prompt for confirmation
                print!(
                    "Identity already exists at {}. Overwrite? (y/N): ",
                    keystore_path.display()
                );
                io::stdout().flush()?;

                let mut response = String::new();
                io::stdin().read_line(&mut response)?;
                if !response.trim().eq_ignore_ascii_case("y") {
                    println!("Import cancelled.");
                    return Ok(());
                }

                println!("Overwriting existing identity...");
            }

            // Create data directory if needed
            std::fs::create_dir_all(data_dir).context("Failed to create data directory")?;

            // Verify the input file by attempting to load it
            print!("Enter passphrase for imported identity: ");
            io::stdout().flush()?;
            let passphrase = rpassword::read_password()?;
            let passphrase = Zeroizing::new(passphrase.into_bytes());

            // Test unlock on the input file
            let mut test_keystore = AgeKeyStore::new(&input);
            test_keystore
                .unlock(&passphrase)
                .context("Failed to unlock imported keystore. Check your passphrase.")?;

            let imported_did = test_keystore.get_keypair()?.did().clone();

            // Copy the validated keystore to target location
            std::fs::copy(&input, &keystore_path).with_context(|| {
                format!("Failed to import keystore to {}", keystore_path.display())
            })?;

            println!("\n✓ Identity imported successfully!");
            println!("  From: {}", input.display());
            println!("  To:   {}", keystore_path.display());
            println!("  DID:  {imported_did}");
        }
    }

    Ok(())
}

async fn handle_recovery_command(
    cmd: RecoveryCommands,
    data_dir: &Path,
    endpoint: &str,
) -> Result<()> {
    let keystore_path = get_keystore_path(data_dir);

    match cmd {
        // Setup and Config are local-only operations (modify keystore's DID document)
        RecoveryCommands::Setup {
            trustees,
            threshold,
            delay,
        } => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' to create one.");
            }

            // Parse trustee DIDs
            let trustee_dids: Result<Vec<Did>> = trustees
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(Did::from_str)
                .collect();
            let trustee_dids = trustee_dids.context("Failed to parse trustee DIDs")?;

            // Validate threshold
            if threshold > trustee_dids.len() {
                bail!(
                    "Threshold ({}) cannot be greater than number of trustees ({})",
                    threshold,
                    trustee_dids.len()
                );
            }
            if threshold == 0 {
                bail!("Threshold must be at least 1");
            }

            // Get passphrase and unlock keystore
            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            // Create recovery configuration
            let recovery_config = IdentityRecoveryConfig {
                method: RecoveryMethod::Social {
                    m: threshold as u8,
                    n: trustee_dids.len() as u8,
                },
                threshold: threshold as u8,
                trustees: trustee_dids.clone(),
                delay_period: delay,
            };

            // Update DID document with recovery config
            keystore.update_did_document(
                |did_doc| {
                    did_doc.recovery = Some(recovery_config.clone());
                    Ok(())
                },
                None, // No rotation event
                &passphrase,
            )?;

            println!("✓ Recovery configuration saved!");
            println!("\nRecovery trustees ({}):", trustee_dids.len());
            for (i, trustee) in trustee_dids.iter().enumerate() {
                println!("  {}. {}", i + 1, trustee);
            }
            println!("\nThreshold: {} of {}", threshold, trustee_dids.len());
            println!("Delay period: {} seconds ({} hours)", delay, delay / 3600);
            println!("\n⚠️  IMPORTANT:");
            println!("  • Choose trustees you trust and who know you well");
            println!(
                "  • Trustees must verify your identity out-of-band (phone, video, in-person)"
            );
            println!(
                "  • If you lose all devices, contact {threshold} trustees to initiate recovery"
            );
        }

        RecoveryCommands::Config => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' to create one.");
            }

            // Get passphrase and unlock keystore
            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            // Get DID document
            let did_doc = keystore.get_did_document()?;

            if let Some(recovery_config) = &did_doc.recovery {
                println!("Recovery Configuration:");
                println!("  Identity: {}", did_doc.id);
                println!("\nTrustees ({}):", recovery_config.trustees.len());
                for (i, trustee) in recovery_config.trustees.iter().enumerate() {
                    println!("  {}. {}", i + 1, trustee);
                }
                println!("\nThreshold: {}", recovery_config.threshold);
                println!(
                    "Delay period: {} seconds ({} hours)",
                    recovery_config.delay_period,
                    recovery_config.delay_period / 3600
                );

                match &recovery_config.method {
                    RecoveryMethod::Social { m, n } => {
                        println!("Method: Social recovery ({m}-of-{n})");
                    }
                    RecoveryMethod::BackupSeed => {
                        println!("Method: Backup seed");
                    }
                    RecoveryMethod::None => {
                        println!("Method: None (no recovery)");
                    }
                }
            } else {
                println!("No recovery configuration found.");
                println!("\nTo configure social recovery, run:");
                println!("  icnctl recovery setup <trustees> --threshold <M>");
            }
        }

        // Storage-based commands use RPC
        RecoveryCommands::Initiate { old_did } => {
            // Create authenticated RPC client
            let mut client = create_authenticated_rpc_client(endpoint, data_dir)?;

            println!("Initiating recovery:");
            println!("  Old DID: {old_did}");
            println!();

            // Prompt user for threshold and delay
            print!("Enter threshold (M-of-N): ");
            io::stdout().flush()?;
            let mut threshold_input = String::new();
            io::stdin().read_line(&mut threshold_input)?;
            let threshold: usize = threshold_input
                .trim()
                .parse()
                .context("Invalid threshold")?;

            print!("Enter delay period in seconds (default 86400 = 24 hours): ");
            io::stdout().flush()?;
            let mut delay_input = String::new();
            io::stdin().read_line(&mut delay_input)?;
            let delay: u64 = if delay_input.trim().is_empty() {
                86400
            } else {
                delay_input.trim().parse().context("Invalid delay")?
            };

            // Initiate recovery via RPC
            let recovery_id = client
                .initiate_recovery(&old_did, threshold, Some(delay))
                .await
                .context("Failed to initiate recovery. Is icnd running?")?;

            println!("\n✓ Recovery initiated!");
            println!("  Recovery ID: {recovery_id}");
            println!("\nNext steps:");
            println!(
                "  1. Contact your {threshold} trustees out-of-band (phone, video, in-person)"
            );
            println!("  2. Ask them to run: icnctl recovery attest {recovery_id}");
            println!("  3. After {threshold} attestations, wait {delay} seconds delay period");
            println!("  4. Finalize recovery: icnctl recovery finalize {recovery_id}");
        }

        RecoveryCommands::Attest {
            recovery_id,
            verification,
        } => {
            // Create authenticated RPC client
            let mut client = create_authenticated_rpc_client(endpoint, data_dir)?;

            println!("Adding attestation to recovery {recovery_id}...");
            println!("  Verification method: {verification}");
            println!();

            // Attest via RPC (daemon uses its own keypair)
            let threshold_reached = client
                .attest_recovery(&recovery_id, &verification)
                .await
                .context("Failed to add attestation. Is icnd running?")?;

            println!("✓ Attestation signed and added!");
            println!("  Verification: {verification}");

            if threshold_reached {
                println!("\n🎉 Threshold reached! Recovery entering delay period.");
            }
        }

        RecoveryCommands::List => {
            // Create authenticated RPC client
            let mut client = create_authenticated_rpc_client(endpoint, data_dir)?;

            let recoveries = client
                .list_recoveries()
                .await
                .context("Failed to list recoveries. Is icnd running?")?;

            let empty_vec = Vec::new();
            let recoveries = recoveries.as_array().unwrap_or(&empty_vec);

            if recoveries.is_empty() {
                println!("No recovery requests found.");
            } else {
                println!("Recovery requests:\n");
                for recovery in recoveries {
                    let id = recovery["id"].as_str().unwrap_or("?");
                    let old_did = recovery["old_did"].as_str().unwrap_or("?");
                    let new_did = recovery["new_did"].as_str().unwrap_or("?");
                    let status = recovery["status"].as_str().unwrap_or("?");
                    let attestations = recovery["attestations_count"].as_u64().unwrap_or(0);
                    let threshold = recovery["threshold"].as_u64().unwrap_or(0);
                    let progress = recovery["progress_summary"].as_str().unwrap_or("?");

                    println!("Recovery ID: {id}");
                    println!("  Old DID: {old_did}");
                    println!("  New DID: {new_did}");
                    println!("  Status: {status}");
                    println!("  Attestations: {attestations}/{threshold}");
                    println!("  Progress: {progress}");
                    println!();
                }
            }
        }

        RecoveryCommands::Status { recovery_id } => {
            // Create authenticated RPC client
            let mut client = create_authenticated_rpc_client(endpoint, data_dir)?;

            let recovery = client
                .get_recovery_status(&recovery_id)
                .await
                .context("Failed to get recovery status. Is icnd running?")?;

            let id = recovery["id"].as_str().unwrap_or("?");
            let old_did = recovery["old_did"].as_str().unwrap_or("?");
            let new_did = recovery["new_did"].as_str().unwrap_or("?");
            let initiated_at = recovery["initiated_at"].as_u64().unwrap_or(0);
            let threshold = recovery["threshold"].as_u64().unwrap_or(0);
            let delay_period = recovery["delay_period"].as_u64().unwrap_or(0);
            let status = recovery["status"].as_str().unwrap_or("?");
            let progress = recovery["progress_summary"].as_str().unwrap_or("?");

            println!("Recovery Status");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("ID: {id}");
            println!("Old DID: {old_did}");
            println!("New DID: {new_did}");
            println!("Initiated: {initiated_at}");
            println!();
            println!("Configuration:");
            println!("  Threshold: {threshold}");
            println!("  Delay: {delay_period} seconds");
            println!();
            println!("Status: {status}");
            println!("Progress: {progress}");
            println!();

            // Show attestations
            if let Some(attestations) = recovery["attestations"].as_array() {
                println!("Attestations ({}/{}):", attestations.len(), threshold);
                for (i, att) in attestations.iter().enumerate() {
                    let trustee = att["trustee"].as_str().unwrap_or("?");
                    let verification = att["verification_method"].as_str().unwrap_or("?");
                    let timestamp = att["timestamp"].as_u64().unwrap_or(0);
                    println!("  {}. Trustee: {}", i + 1, trustee);
                    println!("     Verification: {verification}");
                    println!("     Timestamp: {timestamp}");
                }
            }

            if status == "finalized" {
                if let Some(finalized_at) = recovery["finalized_at"].as_u64() {
                    println!("\n✓ Recovery finalized at: {finalized_at}");
                }
            }
        }

        RecoveryCommands::Finalize { recovery_id } => {
            // Create authenticated RPC client
            let mut client = create_authenticated_rpc_client(endpoint, data_dir)?;

            println!("Finalizing recovery {recovery_id}...");
            println!();

            // Finalize via RPC
            client
                .finalize_recovery(&recovery_id)
                .await
                .context("Failed to finalize recovery. Is icnd running?")?;

            println!("✓ Recovery finalized successfully!");
            println!("\nNext steps:");
            println!("  • Trust graph and ledger will recognize the new DID");
            println!("  • All relationships and balances are preserved");
            println!("  • Old DID is marked as recovered");
        }

        RecoveryCommands::Cancel {
            recovery_id,
            reason,
        } => {
            // Create authenticated RPC client
            let mut client = create_authenticated_rpc_client(endpoint, data_dir)?;

            println!("Cancelling recovery {recovery_id}...");
            println!("  Reason: {reason}");
            println!();

            // Cancel via RPC
            client
                .cancel_recovery(&recovery_id, &reason)
                .await
                .context("Failed to cancel recovery. Is icnd running?")?;

            println!("✓ Recovery cancelled!");
            println!("\n⚠️  This recovery attempt has been marked as cancelled.");
            println!("   Reason: {reason}");
        }
    }

    Ok(())
}

async fn handle_trust_command(cmd: TrustCommands, data_dir: &Path, endpoint: &str) -> Result<()> {
    // Create authenticated RPC client
    let mut client = create_authenticated_rpc_client(endpoint, data_dir)?;

    match cmd {
        TrustCommands::Add { did, score, label } => {
            // Validate score
            if !(0.0..=1.0).contains(&score) {
                bail!("Trust score must be between 0.0 and 1.0");
            }

            // Add trust edge via RPC
            client
                .add_trust(&did, score, label.as_deref())
                .await
                .context("Failed to add trust edge. Is icnd running?")?;

            println!("✓ {}", t!("cli.trust.add.success"));
            println!("  {}: {did}", t!("cli.trust.add.did_label"));
            println!("  {}: {score:.2}", t!("cli.trust.add.score_label"));
            if let Some(l) = label {
                println!("  {}: {l}", t!("cli.trust.add.label_label"));
            }
        }

        TrustCommands::List => {
            let edges = client
                .list_trust()
                .await
                .context("Failed to list trust edges. Is icnd running?")?;

            if edges.is_empty() {
                println!("{}", t!("cli.trust.list.no_edges"));
            } else {
                println!("{}:\n", t!("cli.trust.list.title"));
                for edge in edges {
                    println!("  → {}", edge.target_did);
                    println!("    {}: {:.2}", t!("cli.trust.add.score_label"), edge.score);
                    if !edge.labels.is_empty() {
                        println!(
                            "    {}: {}",
                            t!("cli.trust.add.label_label"),
                            edge.labels.join(", ")
                        );
                    }
                    println!();
                }
            }
        }

        TrustCommands::Show { did } => {
            // Compute trust score via RPC
            let score = client
                .compute_trust(&did)
                .await
                .context("Failed to compute trust score. Is icnd running?")?;

            // Determine trust class based on score
            let class = if score < 0.1 {
                t!("cli.trust.show.class_isolated")
            } else if score < 0.4 {
                t!("cli.trust.show.class_known")
            } else if score < 0.7 {
                t!("cli.trust.show.class_partner")
            } else {
                t!("cli.trust.show.class_federated")
            };

            println!("{} {did}:", t!("cli.trust.show.title"));
            println!("  {}: {score:.4}", t!("cli.trust.add.score_label"));
            println!("  {}: {class}", t!("cli.trust.show.class_label"));
        }

        TrustCommands::Remove { did } => {
            client
                .remove_trust(&did)
                .await
                .context("Failed to remove trust edge. Is icnd running?")?;

            println!("✓ {} {did}", t!("cli.trust.remove.success"));
        }
    }

    Ok(())
}

/// Handle icnctl status command - show daemon status
async fn handle_status_command(data_dir: &std::path::Path, endpoint: &str) -> Result<()> {
    println!("ICN Node Status");
    println!("{}", "=".repeat(60));
    println!();

    // Show local configuration
    println!("Local Configuration:");
    println!("  Data directory: {}", data_dir.display());
    println!("  RPC endpoint:   {endpoint}");
    println!();

    // Try to connect to daemon (auto-authenticate if ICN_PASSPHRASE is set)
    let mut client = match create_rpc_client(endpoint, data_dir, false) {
        Ok(c) => c,
        Err(e) => {
            println!("Daemon Connection: FAILED");
            println!("  Error: {e}");
            return Ok(());
        }
    };

    // Get daemon status - may require auth retry
    let status = match client.get_status().await {
        Ok(s) => s,
        Err(e) => {
            let err_str = e.to_string();
            // Check if this is an auth error
            if err_str.contains("Authentication required") {
                println!("Note: Daemon requires authentication.");
                // Try with credentials
                let keystore_path = get_keystore_path(data_dir);
                if keystore_path.exists() {
                    println!();
                    client = match create_authenticated_rpc_client(endpoint, data_dir) {
                        Ok(c) => c,
                        Err(e) => {
                            println!("Daemon Status: AUTH FAILED");
                            println!("  Error: {e}");
                            return Ok(());
                        }
                    };
                    // Retry with auth
                    match client.get_status().await {
                        Ok(s) => s,
                        Err(e) => {
                            println!("Daemon Status: NOT CONNECTED");
                            println!("  Error: {e}");
                            return Ok(());
                        }
                    }
                } else {
                    println!("Daemon Status: AUTH REQUIRED");
                    println!("  No identity found. Run 'icnctl id init' first.");
                    println!("  Or set ICN_PASSPHRASE env var for non-interactive auth.");
                    return Ok(());
                }
            } else {
                println!("Daemon Status: NOT CONNECTED");
                println!("  Error: {e}");
                println!();
                println!("Is the ICN daemon running? Start it with:");
                println!("  icnd");
                return Ok(());
            }
        }
    };

    println!("Daemon Status:");
    println!(
        "  Running:      {}",
        if status.running { "YES" } else { "NO" }
    );
    println!("  Listen addr:  {}", status.listen_addr);
    println!();

    // Get network stats
    match client.get_stats().await {
        Ok(stats) => {
            println!("Network Statistics:");
            println!("  Peers discovered:    {}", stats.peers_discovered);
            println!("  Active connections:  {}", stats.connections_active);
            println!("  Total connections:   {}", stats.connections_total);
        }
        Err(e) => {
            println!("Network Statistics: unavailable ({e})");
        }
    }
    println!();

    // Get peers
    match client.get_peers().await {
        Ok(peers) => {
            if peers.is_empty() {
                println!("Connected Peers: none");
                println!("  Tip: Other nodes will be discovered via mDNS automatically.");
            } else {
                println!("Connected Peers ({}):", peers.len());
                for peer in peers.iter().take(5) {
                    let did_short = if peer.did.len() > 20 {
                        format!("{}...", &peer.did[..20])
                    } else {
                        peer.did.clone()
                    };
                    println!("  - {} @ {}", did_short, peer.addr);
                }
                if peers.len() > 5 {
                    println!(
                        "  ... and {} more (use 'icnctl network peers' for full list)",
                        peers.len() - 5
                    );
                }
            }
        }
        Err(e) => {
            println!("Connected Peers: unavailable ({e})");
        }
    }
    println!();

    // Check for identity
    let keystore_path = data_dir.join("identity.age");
    if keystore_path.exists() {
        println!("Identity: configured");
        println!("  Keystore: {}", keystore_path.display());
    } else {
        println!("Identity: NOT CONFIGURED");
        println!("  Run 'icnctl id init' to create an identity.");
    }

    Ok(())
}

#[tokio::main]
async fn handle_network_command(cmd: NetworkCommands, endpoint: &str) -> Result<()> {
    // Network commands communicate with daemon via RPC
    let rpc_addr = endpoint.parse()?;
    let mut client = icn_rpc::RpcClient::new(rpc_addr);

    match cmd {
        NetworkCommands::Peers => {
            let peers = client
                .get_peers()
                .await
                .context("Failed to get peers from daemon. Is icnd running?")?;

            if peers.is_empty() {
                println!("{}", t!("cli.network.peers.no_peers"));
                println!("\n{}", t!("cli.network.peers.tip"));
            } else {
                println!("{}:\n", t!("cli.network.peers.title"));
                println!("{}", t!("cli.network.peers.header"));
                println!("{}", "-".repeat(80));
                for peer in peers {
                    println!("{:<50} {:<22} {}", peer.did, peer.addr, peer.version);
                }
            }
        }

        NetworkCommands::Dial { did, addr } => {
            let addr_str = addr.unwrap_or_else(|| "auto-discover".to_string());
            println!("{}", t!("cli.network.dial.connecting"));
            println!("  Target DID: {did}");
            println!("  Address: {addr_str}\n");

            client
                .dial(did.clone(), addr_str)
                .await
                .context("Failed to dial peer. Is icnd running?")?;

            println!("✓ {} {did}", t!("cli.network.dial.success"));
        }

        NetworkCommands::Stats => {
            let stats = client
                .get_stats()
                .await
                .context("Failed to get network stats from daemon. Is icnd running?")?;

            println!("{}:\n", t!("cli.network.stats.title"));
            println!(
                "  {}:      {}",
                t!("cli.network.stats.active_connections"),
                stats.peers_discovered
            );
            println!(
                "  {}:    {}",
                t!("cli.network.stats.active_connections"),
                stats.connections_active
            );
            println!("  Total connections:     {}", stats.connections_total);
        }

        NetworkCommands::Status => {
            let status = client
                .get_status()
                .await
                .context("Failed to get network status from daemon. Is icnd running?")?;

            println!("{}:\n", t!("cli.network.status.title"));
            println!("  Running:               {}", status.running);
            println!(
                "  {}:      {}",
                t!("cli.network.status.listening"),
                status.listen_addr
            );
        }
    }

    Ok(())
}

async fn handle_federation_command(
    cmd: FederationCommands,
    data_dir: &Path,
    endpoint: &str,
) -> Result<()> {
    use icn_core::config::{Config, FederationConfig};

    let config_path = data_dir.join("icn.toml");

    match cmd {
        FederationCommands::Status => {
            // Try to get status from daemon first
            let rpc_addr: std::net::SocketAddr = endpoint.parse()?;
            let mut client = icn_rpc::RpcClient::new(rpc_addr);

            match client.get_peers().await {
                Ok(peers) => {
                    // Load config for federation settings
                    let fed_config = if config_path.exists() {
                        let config = Config::from_file(&config_path)?;
                        config.federation
                    } else {
                        FederationConfig::default()
                    };

                    println!("Federation Status:\n");
                    println!("  Enabled:           {}", fed_config.enabled);
                    println!("  Network name:      {}", fed_config.network_name);
                    println!("  Connected peers:   {}", peers.len());
                    println!("  Max federations:   {}", fed_config.max_federations);
                    println!("  Auto-accept:       {}", fed_config.auto_accept_invites);
                    if fed_config.auto_accept_invites {
                        println!("  Min invite trust:  {:.2}", fed_config.min_invite_trust);
                    }
                }
                Err(_) => {
                    // Daemon not running, show config-only status
                    let fed_config = if config_path.exists() {
                        let config = Config::from_file(&config_path)?;
                        config.federation
                    } else {
                        FederationConfig::default()
                    };

                    println!("Federation Status (daemon not running):\n");
                    println!("  Enabled:           {}", fed_config.enabled);
                    println!("  Network name:      {}", fed_config.network_name);
                    println!("  Max federations:   {}", fed_config.max_federations);
                    println!("  Auto-accept:       {}", fed_config.auto_accept_invites);
                    println!("\nNote: Start icnd to see connected peers.");
                }
            }
        }

        FederationCommands::Peers => {
            // Load config to get bootstrap peers
            let config = if config_path.exists() {
                Config::from_file(&config_path)?
            } else {
                Config::default()
            };

            if config.network.bootstrap_peers.is_empty() {
                println!("No bootstrap peers configured.\n");
                println!("Add peers with: icnctl federation add <peer_url>");
            } else {
                println!("Configured Bootstrap Peers:\n");
                println!("{:<60} Status", "URL");
                println!("{}", "-".repeat(70));

                // Try to check status from daemon
                let rpc_addr: std::net::SocketAddr = endpoint.parse()?;
                let mut client = icn_rpc::RpcClient::new(rpc_addr);
                let connected_peers = client.get_peers().await.unwrap_or_default();
                let connected_dids: std::collections::HashSet<String> =
                    connected_peers.iter().map(|p| p.did.clone()).collect();

                for peer_url in &config.network.bootstrap_peers {
                    // Extract DID from URL
                    let status = if let Some(did) = extract_did_from_peer_url(peer_url) {
                        if connected_dids.contains(&did) {
                            "Connected"
                        } else {
                            "Disconnected"
                        }
                    } else {
                        "Invalid URL"
                    };
                    println!("{peer_url:<60} {status}");
                }
            }
        }

        FederationCommands::Add { peer_url, trust } => {
            // Validate URL format
            if !peer_url.starts_with("icn://") {
                bail!("Invalid peer URL format. Expected: icn://did:icn:PUBKEY@IP:PORT");
            }

            // Validate trust range
            if !(0.0..=1.0).contains(&trust) {
                bail!("Trust score must be between 0.0 and 1.0");
            }

            // Load existing config
            let mut config = if config_path.exists() {
                Config::from_file(&config_path)?
            } else {
                Config::default()
            };

            // Check if already exists
            if config.network.bootstrap_peers.contains(&peer_url) {
                println!("Peer already configured: {peer_url}");
                return Ok(());
            }

            // Add peer
            config.network.bootstrap_peers.push(peer_url.clone());
            config.to_file(&config_path)?;

            println!("✓ Added bootstrap peer: {peer_url}");
            println!("  Initial trust: {trust:.2}");
            println!(
                "\nNote: Restart icnd or use 'icnctl federation connect' to connect immediately."
            );
        }

        FederationCommands::Remove { did } => {
            // Load existing config
            let mut config = if config_path.exists() {
                Config::from_file(&config_path)?
            } else {
                bail!("No configuration file found at {}", config_path.display());
            };

            // Find and remove peer by DID
            let original_len = config.network.bootstrap_peers.len();
            config
                .network
                .bootstrap_peers
                .retain(|url| !url.contains(&did));

            if config.network.bootstrap_peers.len() == original_len {
                println!("No peer found with DID: {did}");
            } else {
                config.to_file(&config_path)?;
                println!("✓ Removed bootstrap peer: {did}");
            }
        }

        FederationCommands::Connect { peer_url } => {
            // Validate URL format
            if !peer_url.starts_with("icn://") {
                bail!("Invalid peer URL format. Expected: icn://did:icn:PUBKEY@IP:PORT");
            }

            // Extract DID and address from URL
            let (did, addr) = parse_peer_url(&peer_url)?;

            // Connect via RPC
            let rpc_addr: std::net::SocketAddr = endpoint.parse()?;
            let mut client = icn_rpc::RpcClient::new(rpc_addr);

            println!("Connecting to peer...");
            println!("  DID:     {did}");
            println!("  Address: {addr}\n");

            client
                .dial(did.clone(), addr)
                .await
                .context("Failed to connect to peer. Is icnd running?")?;

            println!("✓ Successfully connected to {did}");
        }

        FederationCommands::Config => {
            // Show federation configuration
            let config = if config_path.exists() {
                Config::from_file(&config_path)?
            } else {
                Config::default()
            };

            let fed = &config.federation;

            println!("Federation Configuration:\n");
            println!("  enabled:              {}", fed.enabled);
            println!("  network_name:         {}", fed.network_name);
            println!("  bootstrap_peer_trust: {:.2}", fed.bootstrap_peer_trust);
            println!("  auto_accept_invites:  {}", fed.auto_accept_invites);
            println!("  min_invite_trust:     {:.2}", fed.min_invite_trust);
            println!("  max_federations:      {}", fed.max_federations);
            println!("  announce_public_addr: {}", fed.announce_public_addr);
            println!("\nRetry Configuration:");
            println!("  max_retries:            {}", fed.retry.max_retries);
            println!("  initial_delay_secs:     {}", fed.retry.initial_delay_secs);
            println!("  max_delay_secs:         {}", fed.retry.max_delay_secs);
            println!(
                "  reconnect_interval_secs:{}",
                fed.retry.reconnect_interval_secs
            );

            println!(
                "\nBootstrap Peers: {}",
                config.network.bootstrap_peers.len()
            );
            for peer in &config.network.bootstrap_peers {
                println!("  • {peer}");
            }
        }

        FederationCommands::Set { key, value } => {
            // Load existing config
            let mut config = if config_path.exists() {
                Config::from_file(&config_path)?
            } else {
                Config::default()
            };

            // Clone value for display after potential move
            let display_value = value.clone();

            match key.as_str() {
                "enabled" => {
                    config.federation.enabled = value
                        .parse()
                        .context("Invalid value for 'enabled'. Use 'true' or 'false'.")?;
                }
                "network_name" => {
                    config.federation.network_name = value;
                }
                "bootstrap_peer_trust" => {
                    let trust: f64 = value.parse()
                        .context("Invalid value for 'bootstrap_peer_trust'. Use a number between 0.0 and 1.0.")?;
                    if !(0.0..=1.0).contains(&trust) {
                        bail!("Trust score must be between 0.0 and 1.0");
                    }
                    config.federation.bootstrap_peer_trust = trust;
                }
                "auto_accept_invites" => {
                    config.federation.auto_accept_invites = value.parse().context(
                        "Invalid value for 'auto_accept_invites'. Use 'true' or 'false'.",
                    )?;
                }
                "min_invite_trust" => {
                    let trust: f64 = value.parse().context(
                        "Invalid value for 'min_invite_trust'. Use a number between 0.0 and 1.0.",
                    )?;
                    if !(0.0..=1.0).contains(&trust) {
                        bail!("Trust score must be between 0.0 and 1.0");
                    }
                    config.federation.min_invite_trust = trust;
                }
                "max_federations" => {
                    config.federation.max_federations = value
                        .parse()
                        .context("Invalid value for 'max_federations'. Use a positive integer.")?;
                }
                "announce_public_addr" => {
                    config.federation.announce_public_addr = value.parse().context(
                        "Invalid value for 'announce_public_addr'. Use 'true' or 'false'.",
                    )?;
                }
                _ => {
                    bail!("Unknown configuration key: {key}\n\nValid keys:\n  enabled, network_name, bootstrap_peer_trust, auto_accept_invites,\n  min_invite_trust, max_federations, announce_public_addr");
                }
            }

            config.to_file(&config_path)?;
            println!("✓ Set federation.{key} = {display_value}");
        }

        FederationCommands::Invite => {
            // Generate invite URL for this node
            let keystore_path = data_dir.join("identity.age");
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            // Get DID from keystore
            let passphrase = read_passphrase("Enter keystore passphrase: ")?;
            let mut keystore = AgeKeyStore::new(&keystore_path);
            keystore.unlock(&passphrase)?;
            let did = keystore.get_keypair()?.did().to_string();

            // Try to get our listen address from daemon
            let rpc_addr: std::net::SocketAddr = endpoint.parse()?;
            let mut client = icn_rpc::RpcClient::new(rpc_addr);

            match client.get_status().await {
                Ok(status) => {
                    let invite_url = format!("icn://{}@{}", did, status.listen_addr);
                    println!("Federation Invite URL:\n");
                    println!("  {invite_url}");
                    println!("\nShare this URL with peers who want to connect to your node.");
                    println!("They can add it with: icnctl federation add '{invite_url}'");
                }
                Err(_) => {
                    println!("Your DID: {did}\n");
                    println!(
                        "Start icnd to generate a complete invite URL with your network address."
                    );
                }
            }
        }

        FederationCommands::GatewayConnect {
            address,
            peer_did,
            coop_id,
            name,
            gateway,
            token,
        } => {
            let gateway = gateway
                .or_else(|| std::env::var("ICN_GATEWAY").ok())
                .unwrap_or_else(|| "http://localhost:8080".to_string());
            let token = token
                .or_else(|| std::env::var("ICN_TOKEN").ok())
                .context("No token provided. Use --token or set ICN_TOKEN env var.")?;

            let http_client = reqwest::Client::new();
            let url = format!("{gateway}/v1/federation/connect");

            let resp = http_client
                .post(&url)
                .bearer_auth(&token)
                .json(&serde_json::json!({
                    "address": address,
                    "peer_did": peer_did,
                    "coop_id": coop_id,
                    "name": name
                }))
                .send()
                .await
                .context("Failed to connect to gateway")?;

            if resp.status().is_success() {
                let data: serde_json::Value = resp.json().await?;
                println!("Federation peer connected:");
                println!(
                    "  Peer: {}",
                    data["peer_coop_id"].as_str().unwrap_or("unknown")
                );
                println!("  Address: {address}");
            } else {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                bail!("Failed to connect federation peer: {status} - {body}");
            }
        }

        FederationCommands::Coop(cmd) => {
            handle_coop_command(cmd, endpoint).await?;
        }

        FederationCommands::Vouch(cmd) => {
            handle_vouch_command(cmd, endpoint).await?;
        }

        FederationCommands::Attestation(cmd) => {
            handle_attestation_command(cmd, endpoint).await?;
        }

        FederationCommands::Clearing(cmd) => {
            handle_clearing_command(cmd, endpoint).await?;
        }
    }

    Ok(())
}

/// Handle cooperative registry commands (via RPC to daemon)
async fn handle_coop_command(cmd: CoopCommands, endpoint: &str) -> Result<()> {
    let rpc_addr = endpoint.parse()?;
    let mut client = icn_rpc::RpcClient::new(rpc_addr);

    match cmd {
        CoopCommands::List => {
            let result = client
                .federation_coop_list()
                .await
                .context("Failed to list cooperatives from daemon. Is icnd running?")?;

            let empty_vec = vec![];
            let coops = result["cooperatives"].as_array().unwrap_or(&empty_vec);

            if coops.is_empty() {
                println!("No cooperatives registered yet.\n");
                println!("Register your cooperative with: icnctl federation coop register --coop-id <id> --name <name> --gateway <url>");
            } else {
                println!("Known Cooperatives:\n");
                println!("{:<20} {:<30} {:<10}", "ID", "Name", "Federated");
                println!("{}", "-".repeat(62));
                for coop in coops {
                    let coop_id = coop["coop_id"].as_str().unwrap_or("?");
                    let name = coop["name"].as_str().unwrap_or("?");
                    let federated = if coop["federated"].as_bool().unwrap_or(false) {
                        "Yes"
                    } else {
                        "No"
                    };
                    println!("{coop_id:<20} {name:<30} {federated:<10}");
                }
            }
        }

        CoopCommands::Show { coop_id } => {
            let result = client
                .federation_coop_get(&coop_id)
                .await
                .context("Failed to get cooperative from daemon. Is icnd running?")?;

            if result.get("error").is_some() {
                println!("Cooperative '{coop_id}' not found.");
            } else {
                println!("Cooperative Details:\n");
                println!(
                    "  ID:          {}",
                    result["coop_id"].as_str().unwrap_or("?")
                );
                println!("  Name:        {}", result["name"].as_str().unwrap_or("?"));
                println!(
                    "  DID:         {}",
                    result["public_did"].as_str().unwrap_or("?")
                );

                if let Some(gateways) = result["gateway_endpoints"].as_array() {
                    if !gateways.is_empty() {
                        let gw_strs: Vec<&str> =
                            gateways.iter().filter_map(|g| g.as_str()).collect();
                        println!("  Gateways:    {}", gw_strs.join(", "));
                    }
                }

                println!(
                    "  Federated:   {}",
                    result["federated"].as_bool().unwrap_or(false)
                );
                println!(
                    "  Last seen:   {}",
                    result["last_seen"].as_u64().unwrap_or(0)
                );

                // Show policy
                if let Some(policy) = result.get("federation_policy") {
                    println!("\n  Federation Policy: {policy}");
                }

                // Show capabilities
                if let Some(caps) = result["capabilities"].as_array() {
                    if !caps.is_empty() {
                        let cap_strs: Vec<&str> = caps.iter().filter_map(|c| c.as_str()).collect();
                        println!("  Capabilities: {}", cap_strs.join(", "));
                    }
                }

                // Show currencies
                if let Some(currencies) = result["currencies"].as_array() {
                    if !currencies.is_empty() {
                        println!("\n  Supported Currencies:");
                        for currency in currencies {
                            let symbol = currency["symbol"].as_str().unwrap_or("?");
                            let name = currency["name"].as_str().unwrap_or("?");
                            println!("    - {symbol} ({name})");
                        }
                    }
                }
            }
        }

        CoopCommands::Register {
            coop_id,
            name,
            gateway,
            description: _description,
        } => {
            let result = client
                .federation_coop_register(
                    &coop_id,
                    &name,
                    "", // public_did will be filled by daemon using its own identity
                    vec![gateway.clone()],
                    vec![],
                )
                .await
                .context("Failed to register cooperative. Is icnd running?")?;

            if result.get("error").is_some() {
                println!(
                    "Error: {}",
                    result["error"].as_str().unwrap_or("Unknown error")
                );
            } else {
                println!("Cooperative registered successfully!\n");
                println!("  ID:      {coop_id}");
                println!("  Name:    {name}");
                println!("  Gateway: {gateway}");
                println!(
                    "  DID:     {}",
                    result["public_did"].as_str().unwrap_or("?")
                );
            }
        }

        CoopCommands::Update {
            coop_id: _coop_id,
            gateway,
            description: _description,
        } => {
            let gateway_endpoints = gateway.map(|g| vec![g]);

            let result = client
                .federation_own_update(
                    None, // name unchanged
                    gateway_endpoints,
                    None, // capabilities unchanged
                )
                .await
                .context("Failed to update cooperative. Is icnd running?")?;

            if result.get("error").is_some() {
                println!(
                    "Error: {}",
                    result["error"].as_str().unwrap_or("Unknown error")
                );
            } else {
                println!("Cooperative updated successfully.");
            }
        }
    }

    Ok(())
}

/// Handle vouch commands (via RPC to daemon)
async fn handle_vouch_command(cmd: VouchCommands, endpoint: &str) -> Result<()> {
    let rpc_addr = endpoint.parse()?;
    let mut client = icn_rpc::RpcClient::new(rpc_addr);

    match cmd {
        VouchCommands::Issue {
            target_coop,
            trust,
            days: _days, // Expiry is handled by daemon
        } => {
            // Validate trust score
            if !(0.0..=1.0).contains(&trust) {
                anyhow::bail!("Trust score must be between 0.0 and 1.0");
            }

            let result = client
                .federation_vouch_issue(&target_coop, trust)
                .await
                .context("Failed to issue vouch. Is icnd running?")?;

            if result.get("error").is_some() {
                println!(
                    "Error: {}",
                    result["error"].as_str().unwrap_or("Unknown error")
                );
            } else {
                println!("Vouch issued successfully!\n");
                println!(
                    "  From:     {}",
                    result["voucher_coop_id"].as_str().unwrap_or("?")
                );
                println!("  Target:   {target_coop}");
                println!("  Trust:    {trust:.2}");
            }
        }

        VouchCommands::List => {
            // Get own cooperative info first
            let own_result = client
                .federation_own_get()
                .await
                .context("Failed to get own cooperative info. Is icnd running?")?;

            let own_coop_id = own_result["coop_id"].as_str().unwrap_or("local");

            // List all coops and check which ones we've vouched for
            let coops_result = client
                .federation_coop_list()
                .await
                .context("Failed to list cooperatives. Is icnd running?")?;

            let empty_coops = vec![];
            let coops = coops_result["cooperatives"]
                .as_array()
                .unwrap_or(&empty_coops);

            println!("Cooperatives We've Vouched For:\n");
            println!("{:<20} {:<10} {:<10}", "Cooperative", "Trust", "Expired");
            println!("{}", "-".repeat(42));

            let mut found_vouches = false;
            for coop in coops {
                let coop_id = coop["coop_id"].as_str().unwrap_or("?");

                // Get vouches for this coop
                let vouches_result = client.federation_vouch_list(coop_id).await;

                if let Ok(vouches) = vouches_result {
                    if let Some(vouchers) = vouches["vouchers"].as_array() {
                        for vouch in vouchers {
                            if vouch["voucher_coop_id"].as_str() == Some(own_coop_id) {
                                found_vouches = true;
                                let trust = vouch["trust_score"].as_f64().unwrap_or(0.0);
                                let expired = vouch["is_expired"].as_bool().unwrap_or(false);
                                println!(
                                    "{:<20} {:<10.2} {:<10}",
                                    coop_id,
                                    trust,
                                    if expired { "Yes" } else { "No" }
                                );
                            }
                        }
                    }
                }
            }

            if !found_vouches {
                println!("(none)");
                println!("\nVouch for a cooperative with: icnctl federation vouch issue --target-coop <id>");
            }
        }

        VouchCommands::Revoke { target_coop } => {
            let result = client
                .federation_vouch_remove(&target_coop)
                .await
                .context("Failed to revoke vouch. Is icnd running?")?;

            if result.get("error").is_some() {
                println!(
                    "Error: {}",
                    result["error"].as_str().unwrap_or("Unknown error")
                );
            } else {
                println!("Vouch for '{target_coop}' revoked successfully.");
            }
        }
    }

    Ok(())
}

/// Handle attestation commands (via RPC to daemon)
async fn handle_attestation_command(cmd: AttestationCommands, endpoint: &str) -> Result<()> {
    let rpc_addr = endpoint.parse()?;
    let mut client = icn_rpc::RpcClient::new(rpc_addr);

    match cmd {
        AttestationCommands::List { member_did } => {
            let result = client
                .federation_attestation_list(&member_did)
                .await
                .context("Failed to list attestations. Is icnd running?")?;

            let empty_atts = vec![];
            let attestations = result["attestations"].as_array().unwrap_or(&empty_atts);

            if attestations.is_empty() {
                println!("No valid attestations found for {member_did}.");
            } else {
                println!("Attestations for {member_did}:\n");
                println!("{:<20} {:<12} {:<10}", "Source Coop", "Context", "Trust");
                println!("{}", "-".repeat(44));
                for att in attestations {
                    let source = att["source_coop_id"].as_str().unwrap_or("?");
                    let context = att["trust_context"].as_str().unwrap_or("?");
                    let trust = att["trust_score"].as_f64().unwrap_or(0.0);
                    println!("{source:<20} {context:<12} {trust:<10.2}");
                }
            }
        }

        AttestationCommands::From { coop_id } => {
            let result = client
                .federation_attestation_from(&coop_id)
                .await
                .context("Failed to list attestations. Is icnd running?")?;

            let empty_atts_from = vec![];
            let attestations = result["attestations"]
                .as_array()
                .unwrap_or(&empty_atts_from);

            if attestations.is_empty() {
                println!("No attestations from cooperative '{coop_id}'.");
            } else {
                println!("Attestations from '{coop_id}':\n");
                println!("{:<50} {:<12} {:<10}", "Member DID", "Context", "Trust");
                println!("{}", "-".repeat(74));
                for att in attestations {
                    let did_str = att["member_did"].as_str().unwrap_or("?");
                    let short_did = if did_str.len() > 48 {
                        format!("{}...", &did_str[..45])
                    } else {
                        did_str.to_string()
                    };
                    let context = att["trust_context"].as_str().unwrap_or("?");
                    let trust = att["trust_score"].as_f64().unwrap_or(0.0);
                    println!("{short_did:<50} {context:<12} {trust:<10.2}");
                }
            }
        }

        AttestationCommands::Issue {
            member_did,
            trust,
            context,
            days,
        } => {
            if !(0.0..=1.0).contains(&trust) {
                bail!("Trust score must be between 0.0 and 1.0");
            }

            let result = client
                .federation_attestation_issue(&member_did, trust, &context, days)
                .await
                .context("Failed to issue attestation. Is icnd running?")?;

            if result.get("error").is_some() {
                println!(
                    "Error: {}",
                    result["error"].as_str().unwrap_or("Unknown error")
                );
            } else {
                println!("Attestation issued successfully!\n");
                println!("  Member:   {member_did}");
                println!("  Trust:    {trust:.2}");
                println!("  Context:  {context}");
                println!("  Validity: {days} days");
            }
        }
    }

    Ok(())
}

/// Handle clearing agreement commands (via RPC to daemon)
async fn handle_clearing_command(cmd: ClearingCommands, endpoint: &str) -> Result<()> {
    let rpc_addr = endpoint.parse()?;
    let mut client = icn_rpc::RpcClient::new(rpc_addr);

    match cmd {
        ClearingCommands::List => {
            let result = client
                .federation_clearing_list()
                .await
                .context("Failed to list clearing agreements. Is icnd running?")?;

            let empty_agreements = vec![];
            let agreements = result["agreements"].as_array().unwrap_or(&empty_agreements);

            if agreements.is_empty() {
                println!("No clearing agreements yet.\n");
                println!(
                    "Create an agreement with: icnctl federation clearing create --agreement-id <id> --partner-coop <coop>"
                );
            } else {
                println!("Clearing Agreements:\n");
                println!(
                    "{:<20} {:<20} {:<20} {:<15}",
                    "ID", "Coop A", "Coop B", "Max Imbalance"
                );
                println!("{}", "-".repeat(77));
                for agreement in agreements {
                    let id = agreement["agreement_id"].as_str().unwrap_or("?");
                    let coop_a = agreement["coop_a"].as_str().unwrap_or("?");
                    let coop_b = agreement["coop_b"].as_str().unwrap_or("?");
                    let max_imb = agreement["max_imbalance"].as_i64().unwrap_or(0);
                    println!("{id:<20} {coop_a:<20} {coop_b:<20} {max_imb:<15}");
                }
            }
        }

        ClearingCommands::Show { agreement_id } => {
            let result = client
                .federation_clearing_show(&agreement_id)
                .await
                .context("Failed to get clearing agreement. Is icnd running?")?;

            if result.get("error").is_some() {
                println!("Agreement '{agreement_id}' not found.");
            } else {
                println!("Clearing Agreement Details:\n");
                println!(
                    "  ID:            {}",
                    result["agreement_id"].as_str().unwrap_or("?")
                );
                println!(
                    "  Coop A:        {}",
                    result["coop_a"].as_str().unwrap_or("?")
                );
                println!(
                    "  Coop B:        {}",
                    result["coop_b"].as_str().unwrap_or("?")
                );
                println!(
                    "  Max Imbalance: {}",
                    result["max_imbalance"].as_i64().unwrap_or(0)
                );
                println!(
                    "  Settlement:    {}",
                    result["settlement_interval"].as_str().unwrap_or("?")
                );
                println!(
                    "  Signatures:    {}",
                    result["signatures"].as_u64().unwrap_or(0)
                );

                if let Some(rates) = result["exchange_rates"].as_object() {
                    if !rates.is_empty() {
                        println!("\n  Exchange Rates:");
                        for (pair, rate) in rates {
                            let rate_val = rate.as_f64().unwrap_or(0.0);
                            println!("    {pair}: {rate_val:.4}");
                        }
                    }
                }
            }
        }

        ClearingCommands::Create {
            agreement_id,
            partner_coop,
            max_imbalance,
            settlement,
        } => {
            let result = client
                .federation_clearing_create(
                    &agreement_id,
                    &partner_coop,
                    max_imbalance,
                    &settlement,
                )
                .await
                .context("Failed to create clearing agreement. Is icnd running?")?;

            if result.get("error").is_some() {
                println!(
                    "Error: {}",
                    result["error"].as_str().unwrap_or("Unknown error")
                );
            } else {
                let our_coop = result["our_coop"].as_str().unwrap_or("?");
                println!("Clearing agreement created successfully!\n");
                println!("  ID:            {agreement_id}");
                println!("  Our Coop:      {our_coop}");
                println!("  Partner:       {partner_coop}");
                println!("  Max Imbalance: {max_imbalance}");
                println!("  Settlement:    {settlement}");
                println!("\nNote: Partner must accept the agreement with their signature.");
            }
        }

        ClearingCommands::Rate {
            agreement_id,
            from,
            to,
            rate,
        } => {
            // Rate updates are not yet implemented via RPC
            println!("Exchange rate noted for agreement '{agreement_id}':");
            println!("  {from} → {to} = {rate:.4}");
            println!(
                "\nNote: Exchange rate updates are typically done during agreement negotiation."
            );
        }

        ClearingCommands::Position { agreement_id } => {
            let result = client
                .federation_clearing_position(&agreement_id)
                .await
                .context("Failed to get clearing position. Is icnd running?")?;

            if result.get("error").is_some() {
                println!(
                    "Error: {}",
                    result["error"].as_str().unwrap_or("Unknown error")
                );
            } else {
                println!("Clearing Position for '{agreement_id}':\n");
                println!(
                    "  Coop A owes B: {}",
                    result["coop_a_owes_b"].as_i64().unwrap_or(0)
                );
                println!(
                    "  Coop B owes A: {}",
                    result["coop_b_owes_a"].as_i64().unwrap_or(0)
                );
                println!(
                    "  Net position:  {}",
                    result["net_position"].as_i64().unwrap_or(0)
                );
                println!(
                    "  Pending transfers: {}",
                    result["pending_transfers"].as_u64().unwrap_or(0)
                );
            }
        }

        ClearingCommands::Settle { agreement_id } => {
            let result = client
                .federation_clearing_settle(&agreement_id)
                .await
                .context("Failed to settle. Is icnd running?")?;

            if result.get("error").is_some() {
                println!(
                    "Error: {}",
                    result["error"].as_str().unwrap_or("Unknown error")
                );
            } else {
                println!("Settlement Report:\n");
                println!(
                    "  Agreement:         {}",
                    result["agreement_id"].as_str().unwrap_or("?")
                );
                println!(
                    "  Coop A owed:       {}",
                    result["coop_a_owed"].as_i64().unwrap_or(0)
                );
                println!(
                    "  Coop B owed:       {}",
                    result["coop_b_owed"].as_i64().unwrap_or(0)
                );
                println!(
                    "  Net settlement:    {}",
                    result["net_settlement"].as_i64().unwrap_or(0)
                );
                println!(
                    "  Transfers settled: {}",
                    result["transfers_settled"].as_u64().unwrap_or(0)
                );
                println!("\nSettlement completed successfully.");
            }
        }
    }

    Ok(())
}

/// Extract DID from a peer URL (icn://DID@IP:PORT)
fn extract_did_from_peer_url(url: &str) -> Option<String> {
    let url = url.strip_prefix("icn://")?;
    let parts: Vec<&str> = url.split('@').collect();
    if parts.len() == 2 {
        Some(parts[0].to_string())
    } else {
        None
    }
}

/// Parse peer URL into (DID, address)
fn parse_peer_url(url: &str) -> Result<(String, String)> {
    let url = url
        .strip_prefix("icn://")
        .context("URL must start with 'icn://'")?;

    let parts: Vec<&str> = url.split('@').collect();
    if parts.len() != 2 {
        bail!("Invalid peer URL format. Expected: icn://did:icn:PUBKEY@IP:PORT");
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}

#[tokio::main]
async fn handle_ledger_command(cmd: LedgerCommands, endpoint: &str) -> Result<()> {
    // Ledger commands communicate with daemon via RPC
    let rpc_addr = endpoint.parse()?;
    let mut client = icn_rpc::RpcClient::new(rpc_addr);

    match cmd {
        LedgerCommands::Head => {
            match client
                .get_ledger_head()
                .await
                .context("Failed to get ledger head from daemon. Is icnd running?")?
            {
                Some(entry) => {
                    println!("{}:\n", t!("cli.ledger.head.title"));
                    println!(
                        "  {}:      {}",
                        t!("cli.ledger.head.entry_hash"),
                        entry.hash
                    );
                    println!(
                        "  {}:      {}",
                        t!("cli.ledger.head.timestamp"),
                        entry.timestamp
                    );
                    println!("  Author:    {}", entry.author);
                    println!("\n  Account deltas:");
                    for delta in entry.accounts {
                        println!("    • {}", delta.account_id);
                        println!(
                            "      {}: {}",
                            t!("cli.ledger.balance.currency_label"),
                            delta.currency
                        );
                        if let Some(debit) = delta.debit {
                            println!("      Debit:    {debit}");
                        }
                        if let Some(credit) = delta.credit {
                            println!("      Credit:   {credit}");
                        }
                    }
                }
                None => {
                    println!("{}", t!("cli.ledger.head.no_entries"));
                }
            }
        }

        LedgerCommands::Balance {
            account_id,
            currency,
        } => {
            let balances = client
                .get_ledger_balance(account_id.clone(), currency.clone())
                .await
                .context("Failed to get balance from daemon. Is icnd running?")?;

            if balances.is_empty() {
                println!("{}", t!("cli.ledger.balance.no_balance"));
            } else if balances.len() == 1 && currency.is_some() {
                let balance = &balances[0];
                println!("{} {account_id}:\n", t!("cli.ledger.balance.title"));
                println!(
                    "  {}: {}",
                    t!("cli.ledger.balance.currency_label"),
                    balance.currency
                );
                println!(
                    "  {}: {}",
                    t!("cli.ledger.balance.balance_label"),
                    balance.amount
                );
            } else {
                println!("{} {account_id}:\n", t!("cli.ledger.balance.title"));
                for balance in balances {
                    println!("  {:<10} {}", balance.currency, balance.amount);
                }
            }
        }

        LedgerCommands::History { limit } => {
            let entries = client
                .get_ledger_history(Some(limit))
                .await
                .context("Failed to get ledger history from daemon. Is icnd running?")?;

            if entries.is_empty() {
                println!("{}", t!("cli.ledger.history.no_entries"));
            } else {
                println!(
                    "{} ({}):\n",
                    t!("cli.ledger.history.title"),
                    t!("cli.ledger.history.showing", count = entries.len())
                );

                for entry in entries {
                    println!("{}: {}", t!("cli.ledger.history.hash"), entry.hash);
                    println!("{}: {}", t!("cli.ledger.head.timestamp"), entry.timestamp);
                    println!("{}: {}", t!("cli.ledger.history.author"), entry.author);
                    println!("{}:", t!("cli.ledger.history.accounts_label"));
                    for delta in entry.accounts {
                        print!("  • {} ({}): ", delta.account_id, delta.currency);
                        if let Some(debit) = delta.debit {
                            print!("{} {debit} ", t!("cli.ledger.history.debit_label"));
                        }
                        if let Some(credit) = delta.credit {
                            print!("{} {credit} ", t!("cli.ledger.history.credit_label"));
                        }
                        println!();
                    }
                    println!();
                }
            }
        }

        LedgerCommands::Quarantine(q_cmd) => {
            handle_quarantine_command(q_cmd, &mut client).await?;
        }
    }

    Ok(())
}

async fn handle_quarantine_command(
    cmd: QuarantineCommands,
    client: &mut icn_rpc::RpcClient,
) -> Result<()> {
    match cmd {
        QuarantineCommands::List => {
            let result = client
                .quarantine_list()
                .await
                .context("Failed to list quarantine from daemon. Is icnd running?")?;

            // RPC returns PageResponse with "items" field
            let quarantined: Vec<serde_json::Value> = result
                .get("items")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            if quarantined.is_empty() {
                println!("No entries in quarantine.");
            } else {
                println!("Quarantined entries (showing {}):\n", quarantined.len());

                for item in quarantined {
                    println!(
                        "Entry ID:    {}",
                        item["entry_id"].as_str().unwrap_or("N/A")
                    );
                    println!("Reason:      {}", item["reason"].as_str().unwrap_or("N/A"));
                    println!("Author:      {}", item["author"].as_str().unwrap_or("N/A"));
                    println!("Observed at: {}", item["observed_at"].as_u64().unwrap_or(0));
                    if let Some(metadata) = item["metadata"].as_str() {
                        println!("Metadata:    {metadata}");
                    }
                    println!();
                }
            }
        }

        QuarantineCommands::Get { entry_id } => {
            let result = client
                .quarantine_get(entry_id.clone())
                .await
                .context("Failed to get quarantine entry from daemon. Is icnd running?")?;

            println!("Quarantined Entry: {entry_id}\n");

            if let Some(entry) = result.get("entry") {
                println!("Entry Details:");
                println!("  ID:          {}", entry["id"].as_str().unwrap_or("None"));
                println!(
                    "  Author:      {}",
                    entry["author"].as_str().unwrap_or("N/A")
                );
                println!(
                    "  Timestamp:   {}",
                    entry["timestamp"].as_u64().unwrap_or(0)
                );
                println!(
                    "  Parents:     {:?}",
                    entry["parents"].as_array().map(|v| v.len()).unwrap_or(0)
                );
                println!(
                    "  Accounts:    {}",
                    entry["num_accounts"].as_u64().unwrap_or(0)
                );
                println!();
            }

            if let Some(info) = result.get("quarantine_info") {
                println!("Quarantine Info:");
                println!(
                    "  Reason:      {}",
                    info["reason"].as_str().unwrap_or("N/A")
                );
                println!(
                    "  Author:      {}",
                    info["author"].as_str().unwrap_or("N/A")
                );
                println!(
                    "  Observed:    {}",
                    info["observed_at"].as_u64().unwrap_or(0)
                );
                if let Some(metadata) = info["metadata"].as_str() {
                    println!("  Metadata:    {metadata}");
                }
            }
        }

        QuarantineCommands::Release { entry_id } => {
            // Note: This returns an error if reappend fails, as the intent of "release"
            // is to retry the entry. Standard error handling will display the error message.
            client
                .quarantine_release(entry_id.clone())
                .await
                .context("Failed to release entry from daemon. Is icnd running?")?;

            println!("✓ Released entry: {entry_id}");
            println!("✓ Successfully reappended to ledger");
        }

        QuarantineCommands::Drop { entry_id } => {
            let result = client
                .quarantine_drop(entry_id.clone())
                .await
                .context("Failed to drop entry from daemon. Is icnd running?")?;

            if result
                .get("dropped")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                println!("✓ Permanently dropped entry: {entry_id}");
            } else {
                println!("✗ Failed to drop entry (not found in quarantine)");
            }
        }

        QuarantineCommands::Purge => {
            let result = client
                .quarantine_purge()
                .await
                .context("Failed to purge expired entries from daemon. Is icnd running?")?;

            let purged = result.get("purged").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("✓ Purged {purged} expired entries");
        }
    }

    Ok(())
}

async fn handle_contract_command(
    cmd: ContractCommands,
    endpoint: &str,
    data_dir: &Path,
) -> Result<()> {
    // Contract commands communicate with daemon via RPC
    let rpc_addr = endpoint.parse()?;
    let mut client = icn_rpc::RpcClient::new(rpc_addr);

    match cmd {
        ContractCommands::Deploy { contract_file } => {
            // Read contract JSON from file
            let contract_json = std::fs::read_to_string(&contract_file).with_context(|| {
                format!("Failed to read contract file: {}", contract_file.display())
            })?;

            println!(
                "{} {}...\n",
                t!("cli.contract.deploy.deploying"),
                contract_file.display()
            );

            // Parse contract to validate
            let contract: icn_ccl::Contract =
                serde_json::from_str(&contract_json).context("Failed to parse contract JSON")?;

            // Validate contract
            contract.validate().context("Contract validation failed")?;

            println!("✓ Contract validated");
            println!("  Name: {}", contract.name);
            println!("  Participants: {}", contract.participants.len());
            println!("  Rules: {}", contract.rules.len());
            println!();

            // Load keystore to sign deployment
            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("{}", t!("cli.common.no_identity"));
            }

            let passphrase = read_passphrase("Enter passphrase to sign deployment: ")?;

            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let deployer_did = keypair.did().clone();

            println!("Signing deployment as {deployer_did}");

            // Compute code hash (must match ContractActor::compute_code_hash)
            let code_hash = {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(contract.name.as_bytes());
                for participant in &contract.participants {
                    hasher.update(format!("{participant:?}").as_bytes());
                }
                icn_ledger::ContentHash::from_bytes(hasher.finalize().into())
            };

            // Create installation record
            let installed_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();

            // For now, only deployer signs (multi-party signing is future work)
            let signing_bytes =
                icn_ccl::ContractDeploymentMessage::compute_signing_bytes(&code_hash, installed_at);
            let deployer_signature = keypair.sign(&signing_bytes);

            let installation = icn_ccl::ContractInstallation {
                code_hash: code_hash.clone(),
                installed_by: deployer_did.clone(),
                capabilities: vec![], // No special capabilities for now
                participants: contract.participants.clone(),
                signatures: vec![(deployer_did.clone(), deployer_signature.to_bytes().to_vec())],
                installed_at,
                min_caller_trust: None, // Only participants can invoke
            };

            let deployment_msg = icn_ccl::ContractDeploymentMessage {
                code_hash: code_hash.clone(),
                contract: contract.clone(),
                installation,
                deployer_signature: deployer_signature.to_bytes().to_vec(),
            };

            // Serialize deployment message
            let deployment_json = serde_json::to_string(&deployment_msg)
                .context("Failed to serialize deployment message")?;

            println!("✓ Deployment message signed");
            println!();

            // Send to daemon
            let code_hash = client
                .deploy_contract(deployment_json)
                .await
                .context("Failed to deploy contract to daemon. Is icnd running?")?;
            println!("✓ {}", t!("cli.contract.deploy.success"));
            println!("  {}: {code_hash}", t!("cli.contract.deploy.code_hash"));
            println!("\nYou can now call contract rules using:");
            println!("  icnctl contract call {code_hash} <rule_name> <caller_did> --args '{{}}'")
        }

        ContractCommands::Call {
            code_hash,
            rule_name,
            caller,
            args,
        } => {
            // Parse args JSON (default to empty object)
            let args_value: serde_json::Value = if let Some(args_str) = args {
                serde_json::from_str(&args_str).context("Failed to parse args JSON")?
            } else {
                serde_json::json!({})
            };

            println!("{} {code_hash}...", t!("cli.contract.call.calling"));
            println!("  Rule: {rule_name}");
            println!("  Caller: {caller}");
            println!("  Args: {args_value}\n");

            let response = client
                .call_contract(
                    code_hash.clone(),
                    rule_name.clone(),
                    caller.clone(),
                    args_value,
                )
                .await
                .context("Failed to call contract. Is icnd running?")?;
            if response.success {
                println!("✓ {}", t!("cli.contract.call.success"));
                println!(
                    "  {}: {}",
                    t!("cli.contract.call.fuel_used"),
                    response.fuel_consumed
                );
                println!(
                    "  {}: {}",
                    t!("cli.contract.call.result"),
                    response.return_value
                );
            } else {
                println!("✗ Contract execution failed!");
            }
        }

        ContractCommands::Prepare {
            contract_file,
            output,
        } => {
            handle_contract_prepare(&contract_file, &output, data_dir)?;
        }

        ContractCommands::Sign {
            deployment_file,
            output,
        } => {
            handle_contract_sign(&deployment_file, &output, data_dir)?;
        }

        ContractCommands::DeploySigned { deployment_file } => {
            handle_contract_deploy_signed(&deployment_file, &mut client).await?;
        }

        ContractCommands::List => match client.list_contracts().await {
            Ok(contracts) => {
                if contracts.is_empty() {
                    println!("{}", t!("cli.contract.list.no_contracts"));
                } else {
                    println!("{}:\n", t!("cli.contract.list.title"));
                    for contract in contracts {
                        println!(
                            "{}: {}",
                            t!("cli.contract.deploy.code_hash"),
                            contract.code_hash
                        );
                        println!("  Name: {}", contract.name);
                        println!("  Participants: {}", contract.participants.join(", "));
                        if let Some(currency) = contract.currency {
                            println!("  Currency: {currency}");
                        }
                        println!("  Rules: {}", contract.rules.join(", "));
                        println!();
                    }
                }
            }
            Err(e) => {
                println!("Note: Contract listing not yet fully implemented.");
                println!("Error: {e}");
                println!("\nYou can still deploy and call contracts by code hash.");
            }
        },
    }

    Ok(())
}

/// Handle contract prepare command - create initial deployment message with first signature
fn handle_contract_prepare(contract_file: &Path, output: &Path, data_dir: &Path) -> Result<()> {
    // Read and validate contract
    let contract_json = std::fs::read_to_string(contract_file)
        .with_context(|| format!("Failed to read contract file: {}", contract_file.display()))?;

    println!("Preparing contract from {}...\n", contract_file.display());

    let contract: icn_ccl::Contract =
        serde_json::from_str(&contract_json).context("Failed to parse contract JSON")?;

    contract.validate().context("Contract validation failed")?;

    println!("✓ Contract validated");
    println!("  Name: {}", contract.name);
    println!("  Participants: {}", contract.participants.len());
    for (i, did) in contract.participants.iter().enumerate() {
        println!("    {}. {}", i + 1, did);
    }
    println!("  Rules: {}", contract.rules.len());
    println!();

    // Load keystore to sign
    let keystore_path = get_keystore_path(data_dir);
    if !keystore_path.exists() {
        bail!("No identity found. Run 'icnctl id init' to create one first.");
    }

    let passphrase = read_passphrase("Enter passphrase to sign deployment: ")?;

    let mut keystore = AgeKeyStore::open(&keystore_path)?;
    keystore.unlock(&passphrase)?;
    let keypair = keystore.get_keypair()?;
    let signer_did = keypair.did().clone();

    // Check if signer is a participant
    if !contract.participants.contains(&signer_did) {
        bail!("You ({signer_did}) are not a participant in this contract");
    }

    println!(
        "Signing as {} ({} of {} participants)",
        signer_did,
        1,
        contract.participants.len()
    );

    // Compute code hash
    let code_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(contract.name.as_bytes());
        for participant in &contract.participants {
            hasher.update(format!("{participant:?}").as_bytes());
        }
        icn_ledger::ContentHash::from_bytes(hasher.finalize().into())
    };

    // Create installation record
    let installed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    // Generate signature
    let signing_bytes =
        icn_ccl::ContractDeploymentMessage::compute_signing_bytes(&code_hash, installed_at);
    let signature = keypair.sign(&signing_bytes);

    let installation = icn_ccl::ContractInstallation {
        code_hash: code_hash.clone(),
        installed_by: signer_did.clone(),
        capabilities: vec![],
        participants: contract.participants.clone(),
        signatures: vec![(signer_did.clone(), signature.to_bytes().to_vec())],
        installed_at,
        min_caller_trust: None,
    };

    let deployment_msg = icn_ccl::ContractDeploymentMessage {
        code_hash: code_hash.clone(),
        contract: contract.clone(),
        installation,
        deployer_signature: signature.to_bytes().to_vec(),
    };

    // Write to output file
    let deployment_json = serde_json::to_string_pretty(&deployment_msg)
        .context("Failed to serialize deployment message")?;

    std::fs::write(output, deployment_json)
        .with_context(|| format!("Failed to write to {}", output.display()))?;

    println!("✓ Deployment message created: {}", output.display());
    println!();
    println!("Signatures collected: 1/{}", contract.participants.len());
    println!("Signed by:");
    println!("  ✓ {signer_did}");
    println!();
    println!("Next steps:");
    if contract.participants.len() > 1 {
        println!("  1. Send {} to other participants", output.display());
        println!(
            "  2. They run: icnctl contract sign {} -o <output>",
            output.display()
        );
        println!(
            "  3. Once all {} signatures collected, deploy with:",
            contract.participants.len()
        );
        println!("     icnctl contract deploy-signed <fully-signed.json>");
    } else {
        println!("  This is a single-participant contract. Deploy with:");
        println!("     icnctl contract deploy-signed {}", output.display());
    }

    Ok(())
}

/// Handle contract sign command - add your signature to a partial deployment
fn handle_contract_sign(deployment_file: &Path, output: &Path, data_dir: &Path) -> Result<()> {
    // Read partial deployment
    let deployment_json = std::fs::read_to_string(deployment_file).with_context(|| {
        format!(
            "Failed to read deployment file: {}",
            deployment_file.display()
        )
    })?;

    println!("Adding signature to {}...\n", deployment_file.display());

    let mut deployment_msg: icn_ccl::ContractDeploymentMessage =
        serde_json::from_str(&deployment_json).context("Failed to parse deployment JSON")?;

    println!("Contract: {}", deployment_msg.contract.name);
    println!(
        "Participants: {}",
        deployment_msg.contract.participants.len()
    );
    println!();

    // Load keystore
    let keystore_path = get_keystore_path(data_dir);
    if !keystore_path.exists() {
        bail!("No identity found. Run 'icnctl id init' to create one first.");
    }

    let passphrase = read_passphrase("Enter passphrase to sign: ")?;

    let mut keystore = AgeKeyStore::open(&keystore_path)?;
    keystore.unlock(&passphrase)?;
    let keypair = keystore.get_keypair()?;
    let signer_did = keypair.did().clone();

    // Check if signer is a participant
    if !deployment_msg.contract.participants.contains(&signer_did) {
        bail!("You ({signer_did}) are not a participant in this contract");
    }

    // Check if already signed
    if deployment_msg
        .installation
        .signatures
        .iter()
        .any(|(did, _)| did == &signer_did)
    {
        bail!("You ({signer_did}) have already signed this deployment");
    }

    // Generate signature
    let signing_bytes = deployment_msg.signing_bytes();
    let signature = keypair.sign(&signing_bytes);

    // Add signature
    deployment_msg
        .installation
        .signatures
        .push((signer_did.clone(), signature.to_bytes().to_vec()));

    // Write to output
    let updated_json = serde_json::to_string_pretty(&deployment_msg)
        .context("Failed to serialize updated deployment")?;

    std::fs::write(output, updated_json)
        .with_context(|| format!("Failed to write to {}", output.display()))?;

    let signatures_count = deployment_msg.installation.signatures.len();
    let total_participants = deployment_msg.contract.participants.len();

    println!("✓ Signature added: {}", output.display());
    println!();
    println!("Signatures collected: {signatures_count}/{total_participants}");
    println!("Signed by:");
    for (did, _) in &deployment_msg.installation.signatures {
        println!("  ✓ {did}");
    }

    if signatures_count < total_participants {
        println!();
        println!("Still waiting for signatures from:");
        for participant in &deployment_msg.contract.participants {
            if !deployment_msg
                .installation
                .signatures
                .iter()
                .any(|(did, _)| did == participant)
            {
                println!("  ⏳ {participant}");
            }
        }
        println!();
        println!("Next steps:");
        println!("  Send {} to remaining participants", output.display());
    } else {
        println!();
        println!("✓ All signatures collected! Ready to deploy.");
        println!();
        println!("Deploy with:");
        println!("  icnctl contract deploy-signed {}", output.display());
    }

    Ok(())
}

/// Handle deploy-signed command - deploy a fully-signed contract
async fn handle_contract_deploy_signed(
    deployment_file: &Path,
    client: &mut icn_rpc::RpcClient,
) -> Result<()> {
    // Read deployment
    let deployment_json = std::fs::read_to_string(deployment_file).with_context(|| {
        format!(
            "Failed to read deployment file: {}",
            deployment_file.display()
        )
    })?;

    println!(
        "Deploying signed contract from {}...\n",
        deployment_file.display()
    );

    let deployment_msg: icn_ccl::ContractDeploymentMessage =
        serde_json::from_str(&deployment_json).context("Failed to parse deployment JSON")?;

    println!("Contract: {}", deployment_msg.contract.name);
    println!(
        "Participants: {}",
        deployment_msg.contract.participants.len()
    );
    println!(
        "Signatures: {}",
        deployment_msg.installation.signatures.len()
    );
    println!();

    // Validate all participants have signed
    let participant_set: std::collections::HashSet<_> =
        deployment_msg.contract.participants.iter().collect();
    let signature_set: std::collections::HashSet<_> = deployment_msg
        .installation
        .signatures
        .iter()
        .map(|(did, _)| did)
        .collect();

    if participant_set != signature_set {
        let missing: Vec<_> = participant_set.difference(&signature_set).collect();
        bail!("Missing signatures from: {missing:?}");
    }

    println!(
        "✓ All {} participants have signed",
        deployment_msg.contract.participants.len()
    );
    println!();

    // Send to daemon
    let code_hash = client
        .deploy_contract(deployment_json)
        .await
        .context("Failed to deploy contract to daemon. Is icnd running?")?;
    println!("✓ Contract deployed successfully!");
    println!("  Code Hash: {code_hash}");
    println!("\nYou can now call contract rules using:");
    println!("  icnctl contract call {code_hash} <rule_name> <caller_did> --args '{{}}'");

    Ok(())
}

/// Device add request - created on new device, approved on existing device
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceAddRequest {
    /// The DID this request is for
    did: String,

    /// Device label (e.g., "Matt's Laptop")
    label: String,

    /// Ed25519 public key (hex encoded)
    ed25519_public_key: String,

    /// X25519 public key for encryption (hex encoded)
    x25519_public_key: String,

    /// Requested capabilities
    capabilities: Vec<Capability>,

    /// Timestamp when request was created
    created_at: u64,
}

fn handle_device_command(cmd: DeviceCommands, data_dir: &Path) -> Result<()> {
    let keystore_path = get_keystore_path(data_dir);

    match cmd {
        DeviceCommands::List => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("{}", t!("cli.common.no_identity"));
            }

            // Get passphrase and unlock
            let passphrase = read_passphrase(&t!("cli.prompts.enter_passphrase"))?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            // Get DID document
            let did_doc = keystore.get_did_document()?;
            let device_id = keystore.get_device_id()?;

            println!("Identity: {}", did_doc.id);
            println!("Version: {}", did_doc.version);
            println!("Updated: {}", did_doc.updated_at);
            println!(
                "\n{} ({}):",
                t!("cli.device.list.title"),
                did_doc.verification_method.len() / 2
            ); // Ed25519 + X25519 per device
            println!();

            // Group verification methods by device (Ed25519 + X25519 pairs)
            let mut device_map: std::collections::HashMap<
                String,
                Vec<&icn_identity::VerificationMethod>,
            > = std::collections::HashMap::new();

            for vm in &did_doc.verification_method {
                // Extract device number from id (e.g., "device-1" or "enc-1")
                let base_id = if vm.id.starts_with("device-") {
                    vm.id.clone()
                } else if vm.id.starts_with("enc-") {
                    format!("device-{}", &vm.id[4..])
                } else {
                    vm.id.clone()
                };

                device_map.entry(base_id).or_default().push(vm);
            }

            // Display devices in order
            let mut device_ids: Vec<_> = device_map.keys().collect();
            device_ids.sort();

            for dev_id in device_ids {
                let vms = &device_map[dev_id];

                // Find the signing key for this device
                let signing_vm = vms.iter().find(|vm| vm.key_type == KeyType::Ed25519);

                if let Some(vm) = signing_vm {
                    let current_marker = if dev_id == device_id {
                        &format!(" {}", t!("cli.device.list.current"))
                    } else {
                        ""
                    };
                    let revoked_marker = if vm.revoked_at.is_some() {
                        " [REVOKED]"
                    } else {
                        ""
                    };

                    println!("Device: {}{}{}", vm.id, current_marker, revoked_marker);
                    println!("  Label: {}", vm.label);
                    println!("  Added: {}", vm.added_at);

                    if let Some(revoked) = vm.revoked_at {
                        println!("  Revoked: {revoked}");
                    }

                    println!("  Capabilities:");
                    for cap in &vm.capabilities {
                        println!("    - {cap:?}");
                    }

                    // Show associated encryption key
                    let enc_vm = vms.iter().find(|v| v.key_type == KeyType::X25519);
                    if enc_vm.is_some() {
                        println!("  Encryption: Yes");
                    }

                    println!();
                }
            }
        }

        DeviceCommands::Add { name, qr, qr_image } => {
            println!("{} '{name}'...\n", t!("cli.device.add.creating"));

            // Prompt for the target DID (the identity to add this device to)
            print!("Enter the DID to add this device to: ");
            io::stdout().flush()?;
            let mut did_input = String::new();
            io::stdin().read_line(&mut did_input)?;
            let target_did = did_input.trim();

            if !target_did.starts_with("did:icn:") {
                bail!("Invalid DID format. Expected: did:icn:<base58btc-key>");
            }

            println!("Target DID: {target_did}");
            println!();

            // Generate new Ed25519 keypair for this device
            println!("Generating Ed25519 keypair for this device...");
            let keypair = KeyPair::generate()?;

            // Generate X25519 encryption key
            use rand::rngs::OsRng;
            use x25519_dalek::{PublicKey, StaticSecret};

            let x25519_secret = StaticSecret::random_from_rng(OsRng);
            let x25519_public = PublicKey::from(&x25519_secret);

            println!("✓ Generated keys for new device");
            println!(
                "  Ed25519 public key: {}",
                hex::encode(keypair.verifying_key().as_bytes())
            );
            println!();

            // Create request
            // Note: created_at timestamp is recorded for auditing purposes but expiration/replay
            // validation is intentionally deferred to the approval workflow on the authorized device.
            // This allows flexibility in transfer timing while the approval step provides the
            // security boundary (user must explicitly approve on an already-authorized device).
            let request = DeviceAddRequest {
                did: target_did.to_string(),
                label: name.clone(),
                ed25519_public_key: hex::encode(keypair.verifying_key().as_bytes()),
                x25519_public_key: hex::encode(x25519_public.as_bytes()),
                capabilities: vec![Capability::Sign, Capability::Encrypt],
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
            };

            // Save request to file
            let request_file = data_dir.join(format!(
                "device-add-{}.json",
                name.replace(" ", "-").to_lowercase()
            ));
            let request_json = serde_json::to_string_pretty(&request)?;
            std::fs::create_dir_all(data_dir)?;
            std::fs::write(&request_file, request_json)?;

            println!("✓ Device-add request created: {}", request_file.display());
            println!();

            // Generate QR code if requested
            let qr_requested = qr || qr_image.is_some();
            if qr_requested {
                use qrcode::QrCode;

                // Use compact JSON for QR code (no pretty printing)
                let qr_data = serde_json::to_string(&request)?;
                let qr_data_len = qr_data.len();

                println!("QR code data size: {} bytes", qr_data_len);

                if qr_data_len > MAX_RELIABLE_QR_SIZE {
                    eprintln!(
                        "⚠️  Warning: QR code data is large ({} bytes). May not scan reliably.",
                        qr_data_len
                    );
                }

                let code = QrCode::new(qr_data.as_bytes()).with_context(|| {
                    format!("Failed to generate QR code ({} bytes)", qr_data_len)
                })?;

                // Display QR in terminal if --qr is set, or if no image output was requested
                if qr || qr_image.is_none() {
                    println!("\n{}", "═".repeat(60));
                    println!("QR CODE - Scan with approving device");
                    println!("{}", "═".repeat(60));

                    let qr_string = code
                        .render::<char>()
                        .quiet_zone(false)
                        .module_dimensions(2, 1)
                        .build();

                    println!("{}", qr_string);
                    println!("{}\n", "═".repeat(60));

                    println!("⚠️  SECURITY WARNING:");
                    println!("  • Do not let unauthorized parties scan this QR code");
                    println!("  • Ensure no cameras or recording devices can capture it");
                    println!(
                        "  • This request should be deleted after use (no automatic expiration)"
                    );
                    println!();
                }

                // Save QR as image if --qr-image is set
                if let Some(image_path) = qr_image {
                    use image::Luma;

                    let image = code.render::<Luma<u8>>().build();
                    image.save(&image_path).with_context(|| {
                        format!("Failed to save QR code image to {}", image_path.display())
                    })?;

                    println!("✓ QR code image saved: {}", image_path.display());
                    println!();
                }
            }

            println!("Next steps:");
            if qr_requested {
                println!("  Option 1 (QR Code):");
                println!("    1. On authorized device, scan the QR code above");
                println!("    2. Approve the device-add request");
                println!();
                println!("  Option 2 (File Transfer):");
            }
            println!(
                "    1. Transfer {} to an authorized device for identity {}",
                request_file.display(),
                target_did
            );
            println!("    2. On authorized device, run:");
            println!("       icnctl device approve {}", request_file.display());
            println!();
            println!("⚠️  IMPORTANT:");
            println!("  • This device will be added to identity: {target_did}");
            println!("  • Do NOT create a new keystore on this device");
            println!("  • After approval, this device will share the same DID");
        }

        DeviceCommands::Approve { request_file } => {
            // Check if request file exists
            if !request_file.exists() {
                bail!("Request file not found: {}", request_file.display());
            }

            // Load request
            let request_json = std::fs::read_to_string(&request_file).with_context(|| {
                format!("Failed to read request file: {}", request_file.display())
            })?;

            let request: DeviceAddRequest = serde_json::from_str(&request_json)
                .context("Failed to parse device-add request")?;

            println!("{}\n", t!("cli.device.approve.approving"));
            println!("  Label: {}", request.label);
            println!("  DID: {}", request.did);
            println!("  Requested at: {}", request.created_at);
            println!();

            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("{}", t!("cli.common.no_identity"));
            }

            // Get passphrase and unlock
            let passphrase = read_passphrase("Enter passphrase to approve: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            // Verify DID matches
            let own_keypair = keystore.get_keypair()?;
            let own_did = own_keypair.did();
            if own_did.as_str() != request.did {
                bail!(
                    "DID mismatch: request is for {}, but your identity is {}",
                    request.did,
                    own_did
                );
            }

            // Get DID document and check capabilities
            let did_doc = keystore.get_did_document()?;
            let device_id = keystore.get_device_id()?;

            if !did_doc.has_capability(device_id, Capability::AddDevice) {
                bail!("This device does not have AddDevice capability");
            }

            // Determine next device ID
            let max_device_num = did_doc
                .verification_method
                .iter()
                .filter_map(|vm| {
                    if vm.id.starts_with("device-") {
                        vm.id[7..].parse::<u32>().ok()
                    } else {
                        None
                    }
                })
                .max()
                .unwrap_or(0);

            let new_device_id = format!("device-{}", max_device_num + 1);

            println!("Adding device as: {new_device_id}");

            // Decode public keys
            let ed25519_bytes = hex::decode(&request.ed25519_public_key)
                .context("Invalid Ed25519 public key encoding")?;
            let x25519_bytes = hex::decode(&request.x25519_public_key)
                .context("Invalid X25519 public key encoding")?;

            if ed25519_bytes.len() != 32 {
                bail!(
                    "Invalid Ed25519 key length: expected 32 bytes, got {}",
                    ed25519_bytes.len()
                );
            }
            if x25519_bytes.len() != 32 {
                bail!(
                    "Invalid X25519 key length: expected 32 bytes, got {}",
                    x25519_bytes.len()
                );
            }

            // Create rotation event for this device add (with both keys)
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            let new_version = did_doc.version + 1;

            // Prepare event data for signing
            let event_data = format!(
                "{}:add_device:{}:{}:{}",
                own_did.as_str(),
                new_device_id,
                timestamp,
                new_version
            );

            // Sign with current device key
            let signature = keystore.get_keypair()?.sign(event_data.as_bytes());

            let rotation_event = icn_identity::RotationEvent {
                did: own_did.clone(),
                event_type: icn_identity::RotationEventType::AddDeviceWithEncryption {
                    device_id: new_device_id.clone(),
                    label: request.label.clone(),
                    ed25519_public_key: ed25519_bytes.clone(),
                    x25519_public_key: x25519_bytes.clone(),
                    signing_capabilities: request.capabilities.clone(),
                },
                proof: signature.to_bytes().to_vec(),
                signed_by: device_id.to_string(),
                timestamp,
                new_version,
            };

            // Update DID document and save
            println!("Updating DID document...");
            let new_device_id_clone = new_device_id.clone();
            let request_label = request.label.clone();
            let request_caps = request.capabilities.clone();

            keystore.update_did_document(
                |did_doc| {
                    // Add both Ed25519 signing key and X25519 encryption key
                    // This increments the version only once, matching the rotation event
                    did_doc.add_device_with_encryption_key(
                        new_device_id_clone,
                        request_label,
                        ed25519_bytes,
                        x25519_bytes,
                        request_caps,
                    )?;

                    Ok(())
                },
                Some(rotation_event),
                &passphrase,
            )?;

            println!("✓ {}", t!("cli.device.approve.success"));
            println!("  {}: {new_device_id}", t!("cli.device.approve.device_id"));
            println!("  Label: {}", request.label);
            println!();
            println!("DID document updated:");
            let updated_doc = keystore.get_did_document()?;
            println!("  Version: {}", updated_doc.version);
            println!("  Devices: {}", updated_doc.verification_method.len() / 2);
        }

        DeviceCommands::Revoke { device_id, reason } => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("{}", t!("cli.common.no_identity"));
            }

            println!("{} {device_id}", t!("cli.device.revoke.revoking"));
            if let Some(r) = &reason {
                println!("{}: {r}", t!("cli.device.revoke.reason_label"));
            }
            println!();

            // Get passphrase and unlock
            let passphrase = read_passphrase("Enter passphrase to authorize revocation: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            // Get DID document and check capabilities
            let did_doc = keystore.get_did_document()?;
            let current_device_id = keystore.get_device_id()?;

            if !did_doc.has_capability(current_device_id, Capability::RevokeDevice) {
                bail!("This device does not have RevokeDevice capability");
            }

            // Check device exists
            if did_doc.get_verification_method(&device_id).is_none() {
                bail!("Device '{device_id}' not found in DID document");
            }

            // Determine revocation reason
            use icn_identity::RevocationReason;
            let revocation_reason = match reason.as_deref() {
                Some("compromised") => RevocationReason::Compromised,
                Some("lost") => RevocationReason::Lost,
                Some("rotated") => RevocationReason::Rotated,
                _ => RevocationReason::Removed,
            };

            // Create rotation event for this device revocation
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            let new_version = did_doc.version + 1;
            let did = keystore.get_keypair()?.did().clone();

            // Prepare event data for signing
            let event_data = format!(
                "{}:revoke_device:{}:{}:{}",
                did.as_str(),
                device_id,
                timestamp,
                new_version
            );

            // Sign with current device key
            let signature = keystore.get_keypair()?.sign(event_data.as_bytes());

            let rotation_event = icn_identity::RotationEvent {
                did,
                event_type: icn_identity::RotationEventType::RevokeDevice {
                    device_id: device_id.clone(),
                    reason: revocation_reason,
                },
                proof: signature.to_bytes().to_vec(),
                signed_by: current_device_id.to_string(),
                timestamp,
                new_version,
            };

            // Update DID document
            let device_id_clone = device_id.clone();
            keystore.update_did_document(
                |did_doc| {
                    did_doc.revoke_device(&device_id_clone)?;
                    Ok(())
                },
                Some(rotation_event),
                &passphrase,
            )?;

            println!("✓ {}", t!("cli.device.revoke.success"));
            println!("  Device: {device_id}");
            if let Some(r) = reason {
                println!("  {}: {r}", t!("cli.device.revoke.reason_label"));
            }
            println!();
            println!("DID document updated:");
            let updated_doc = keystore.get_did_document()?;
            println!("  Version: {}", updated_doc.version);
            println!(
                "  Active devices: {}",
                updated_doc
                    .verification_method
                    .iter()
                    .filter(|vm| vm.revoked_at.is_none() && vm.key_type == KeyType::Ed25519)
                    .count()
            );
        }
    }

    Ok(())
}

/// Backup metadata stored with the tarball
#[derive(Debug, Serialize, Deserialize)]
struct BackupMetadata {
    /// ICN version (from Cargo.toml)
    icn_version: String,
    /// Timestamp when backup was created
    created_at: u64,
    /// SHA256 checksum of the data directory content
    checksum: String,
}

fn handle_backup_command(data_dir: &Path, output: &Path) -> Result<()> {
    // Check if data directory exists
    if !data_dir.exists() {
        bail!("Data directory does not exist: {}", data_dir.display());
    }

    println!("{} {}...", t!("cli.backup.creating"), data_dir.display());

    // Create output file
    let output_file = File::create(output)
        .with_context(|| format!("Failed to create backup file: {}", output.display()))?;

    // Create tar archive builder
    let mut tar_builder = Builder::new(output_file);

    // Add data directory to tarball
    println!("Archiving data directory...");
    tar_builder
        .append_dir_all(".", data_dir)
        .context("Failed to archive data directory")?;

    // Calculate checksum of the data directory
    println!("Calculating checksum...");
    let checksum = calculate_dir_checksum(data_dir)?;

    // Create metadata
    let metadata = BackupMetadata {
        icn_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
        checksum: checksum.clone(),
    };

    // Add metadata to tarball
    let metadata_json = serde_json::to_string_pretty(&metadata)?;
    let metadata_bytes = metadata_json.as_bytes();
    let mut header = tar::Header::new_gnu();
    header.set_size(metadata_bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar_builder
        .append_data(&mut header, "backup_metadata.json", metadata_bytes)
        .context("Failed to add metadata to backup")?;

    // Finish the tarball
    tar_builder.finish().context("Failed to finalize backup")?;

    println!("✓ {}", t!("cli.backup.success"));
    println!("  {}: {}", t!("cli.backup.output_label"), output.display());
    println!("  ICN version: {}", metadata.icn_version);
    println!("  Checksum: {checksum}");
    println!();
    println!("IMPORTANT: {}", t!("cli.backup.includes"));

    Ok(())
}

fn handle_restore_command(data_dir: &Path, input: &Path, force: bool) -> Result<()> {
    // Check if input backup file exists
    if !input.exists() {
        bail!("Backup file not found: {}", input.display());
    }

    // Check if data directory already exists
    if data_dir.exists() && !force {
        bail!(
            "Data directory already exists: {}. Use --force to overwrite.",
            data_dir.display()
        );
    }

    println!("Restoring backup from {}...", input.display());

    // Open the backup archive
    let input_file = File::open(input)
        .with_context(|| format!("Failed to open backup file: {}", input.display()))?;
    let mut archive = Archive::new(input_file);

    // Extract metadata first
    println!("Reading backup metadata...");
    let metadata = extract_backup_metadata(&mut archive, input)?;

    println!("Backup information:");
    println!("  ICN version: {}", metadata.icn_version);
    println!(
        "  Created: {}",
        chrono::DateTime::from_timestamp(metadata.created_at as i64, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "Unknown".to_string())
    );
    println!("  Checksum: {}", metadata.checksum);
    println!();

    // If force, backup existing data directory
    if data_dir.exists() {
        println!("Backing up existing data directory...");
        let backup_dir = format!("{}.backup-{}", data_dir.display(), metadata.created_at);
        std::fs::rename(data_dir, &backup_dir)
            .with_context(|| "Failed to backup existing data directory".to_string())?;
        println!("  Existing data moved to: {backup_dir}");
    }

    // Create data directory if it doesn't exist
    std::fs::create_dir_all(data_dir).context("Failed to create data directory")?;

    // Extract the archive (excluding metadata file)
    println!("Extracting backup...");
    let input_file = File::open(input)
        .with_context(|| format!("Failed to reopen backup file: {}", input.display()))?;
    let mut archive = Archive::new(input_file);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        // Skip the metadata file - we've already read it
        if path.to_string_lossy() == "backup_metadata.json" {
            continue;
        }

        entry.unpack_in(data_dir)?;
    }

    // Verify checksum
    println!("Verifying checksum...");
    let restored_checksum = calculate_dir_checksum(data_dir)?;
    if restored_checksum != metadata.checksum {
        bail!(
            "Checksum mismatch! Expected: {}, Got: {}. Restore may be corrupted.",
            metadata.checksum,
            restored_checksum
        );
    }

    println!("✓ Backup restored successfully");
    println!("  Restored to: {}", data_dir.display());
    println!("  Checksum verified: {restored_checksum}");
    println!();
    println!("You can now use 'icnctl id show' to verify your restored identity.");

    Ok(())
}

/// Verify backup integrity without permanent restore (for CI validation)
fn handle_verify_backup_command(input: &Path, verify_ledger: bool) -> Result<()> {
    use tempfile::TempDir;

    // Check if input backup file exists
    if !input.exists() {
        bail!("Backup file not found: {}", input.display());
    }

    println!("Verifying backup: {}", input.display());
    println!();

    // Create temporary directory for validation
    let temp_dir = TempDir::new().context("Failed to create temporary directory")?;
    let restore_dir = temp_dir.path();

    // Open the backup archive
    let input_file = File::open(input)
        .with_context(|| format!("Failed to open backup file: {}", input.display()))?;
    let mut archive = Archive::new(input_file);

    // Extract metadata
    println!("[1/4] Reading backup metadata...");
    let metadata = extract_backup_metadata(&mut archive, input)?;

    println!("  ICN version: {}", metadata.icn_version);
    println!(
        "  Created: {}",
        chrono::DateTime::from_timestamp(metadata.created_at as i64, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "Unknown".to_string())
    );

    // Extract archive to temp directory
    println!("[2/4] Extracting backup to temporary location...");
    let input_file = File::open(input)
        .with_context(|| format!("Failed to reopen backup file: {}", input.display()))?;
    let mut archive = Archive::new(input_file);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        if path.to_string_lossy() == "backup_metadata.json" {
            continue;
        }

        // Security: Prevent path traversal attacks
        // Malicious tar files could contain paths like "../../../etc/passwd"
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            bail!(
                "FAILED: Backup contains path traversal attempt: {}",
                path.display()
            );
        }

        entry.unpack_in(restore_dir)?;
    }

    // Verify checksum
    println!("[3/4] Verifying checksum...");
    let restored_checksum = calculate_dir_checksum(restore_dir)?;
    if restored_checksum != metadata.checksum {
        bail!(
            "FAILED: Checksum mismatch! Expected: {}, Got: {}",
            metadata.checksum,
            restored_checksum
        );
    }
    println!("  ✓ Checksum verified: {restored_checksum}");

    // Verify required files exist
    println!("[4/4] Verifying backup contents...");
    let identity_path = restore_dir.join("identity.age");
    if !identity_path.exists() {
        bail!("FAILED: Backup missing identity.age (keystore)");
    }
    println!("  ✓ identity.age present");

    // Check for optional but important files
    let state_snapshot = restore_dir.join("state.snapshot");
    if state_snapshot.exists() {
        println!("  ✓ state.snapshot present");
    } else {
        println!("  ⚠ state.snapshot not present (may be first-run backup)");
    }

    // Optional: verify ledger integrity
    if verify_ledger {
        println!();
        println!("[Extra] Verifying ledger integrity...");
        verify_ledger_in_backup(restore_dir)?;
    }

    // Temp directory auto-cleaned on drop
    println!();
    println!("═══════════════════════════════════════");
    println!("✓ BACKUP VERIFICATION PASSED");
    println!("═══════════════════════════════════════");
    println!();
    println!("This backup can be safely restored.");

    Ok(())
}

/// Verify ledger integrity in a restored backup directory
fn verify_ledger_in_backup(restore_dir: &Path) -> Result<()> {
    use icn_store::{SledStore, Store};

    // Check if ledger store exists
    let ledger_db_path = restore_dir.join("ledger");
    if !ledger_db_path.exists() {
        println!("  ⚠ No ledger database found (may be new node)");
        return Ok(());
    }

    // Open the ledger store in read-only mode
    let store = SledStore::open(&ledger_db_path).context("Failed to open ledger store")?;

    // Count entries using the correct ledger journal prefix
    let entry_prefix = b"ledger:journal:";
    let entries = store.scan(entry_prefix)?;
    println!("  Found {} ledger entries", entries.len());

    // Verify double-entry invariant: Σ debits == Σ credits per currency
    // This means: sum of (debit - credit) per currency should be 0
    let mut currency_sums: std::collections::HashMap<String, i128> =
        std::collections::HashMap::new();
    let mut parse_errors = 0usize;

    for (_key, value) in entries {
        // Try to deserialize entry and extract account deltas
        match serde_json::from_slice::<serde_json::Value>(&value) {
            Ok(entry) => {
                if let Some(accounts) = entry.get("accounts").and_then(|a| a.as_array()) {
                    for account in accounts {
                        if let Some(currency) = account.get("currency").and_then(|c| c.as_str()) {
                            // AccountDelta uses separate debit and credit fields (both Option<i64>)
                            let debit = account.get("debit").and_then(|d| d.as_i64()).unwrap_or(0);
                            let credit =
                                account.get("credit").and_then(|c| c.as_i64()).unwrap_or(0);

                            // Use checked arithmetic to prevent overflow
                            let net =
                                (debit as i128).checked_sub(credit as i128).ok_or_else(|| {
                                    anyhow::anyhow!(
                                        "Arithmetic overflow computing net for currency {currency}"
                                    )
                                })?;

                            let sum = currency_sums.entry(currency.to_string()).or_insert(0);
                            *sum = sum.checked_add(net).ok_or_else(|| {
                                anyhow::anyhow!("Arithmetic overflow summing currency {currency}")
                            })?;
                        }
                    }
                }
            }
            Err(_) => {
                parse_errors += 1;
            }
        }
    }

    if parse_errors > 0 {
        println!("  ⚠ {parse_errors} entries could not be parsed");
    }

    // Check invariant
    let mut all_balanced = true;
    for (currency, sum) in &currency_sums {
        if *sum != 0 {
            println!("  ✗ Currency {currency} has imbalance: {sum}");
            all_balanced = false;
        }
    }

    if all_balanced {
        if currency_sums.is_empty() {
            println!("  ✓ Ledger empty (no currencies)");
        } else {
            println!(
                "  ✓ Double-entry invariant verified for {} currencies",
                currency_sums.len()
            );
        }
    } else {
        bail!("FAILED: Ledger double-entry invariant violated");
    }

    Ok(())
}

/// Calculate SHA256 checksum of all files in a directory
fn calculate_dir_checksum(dir: &Path) -> Result<String> {
    use std::collections::BTreeMap;

    let mut file_hashes: BTreeMap<String, String> = BTreeMap::new();

    // Walk directory and hash each file
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            let path = entry.path();
            let relative_path = path
                .strip_prefix(dir)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            // Read file and calculate hash
            let mut file = File::open(path)
                .with_context(|| format!("Failed to open file for hashing: {}", path.display()))?;
            let mut hasher = Sha256::new();
            std::io::copy(&mut file, &mut hasher)?;
            let hash = format!("{:x}", hasher.finalize());

            file_hashes.insert(relative_path, hash);
        }
    }

    // Create combined hash of all file hashes
    let mut combined_hasher = Sha256::new();
    for (path, hash) in file_hashes {
        combined_hasher.update(path.as_bytes());
        combined_hasher.update(hash.as_bytes());
    }

    Ok(format!("{:x}", combined_hasher.finalize()))
}

/// Extract backup metadata from the archive
fn extract_backup_metadata(_archive: &mut Archive<File>, input: &Path) -> Result<BackupMetadata> {
    // Re-open to read metadata
    let input_file = File::open(input)
        .with_context(|| format!("Failed to reopen backup file: {}", input.display()))?;
    let mut archive = Archive::new(input_file);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        if path.to_string_lossy() == "backup_metadata.json" {
            let mut metadata_json = String::new();
            entry.read_to_string(&mut metadata_json)?;
            let metadata: BackupMetadata =
                serde_json::from_str(&metadata_json).context("Failed to parse backup metadata")?;
            return Ok(metadata);
        }
    }

    bail!("Backup metadata not found in archive. This may not be a valid ICN backup.");
}

// RPC client helpers

async fn handle_gov_command(cmd: GovCommands, data_dir: &Path, endpoint: &str) -> Result<()> {
    // Create authenticated RPC client
    let mut client = create_authenticated_rpc_client(endpoint, data_dir)?;

    match cmd {
        GovCommands::Domain(domain_cmd) => match domain_cmd {
            DomainCommands::Create {
                domain_id,
                name,
                members,
                profile,
                quorum,
                approval,
                voting_period,
            } => {
                // Parse member DIDs as strings
                let member_list: Vec<String> = members
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();

                if member_list.is_empty() {
                    bail!("At least one member DID is required");
                }

                // Build RPC request
                let params = serde_json::json!({
                    "domain_id": domain_id,
                    "name": name,
                    "profile": profile,
                    "params": {
                        "quorum_percentage": quorum,
                        "approval_threshold_percentage": approval,
                        "voting_period_seconds": voting_period,
                    },
                    "membership": {
                        "type": "static_list",
                        "members": member_list,
                    },
                });

                client.call("governance.domain.create", params).await?;

                println!("✓ Governance domain created:");
                println!("  ID: {domain_id}");
                println!("  Name: {name}");
                println!("  Members: {}", member_list.len());
                println!("  Profile: {profile}");
                println!("  Quorum: {quorum}%");
                println!("  Approval: {approval}%");
                println!("  Voting period: {voting_period} seconds");
            }

            DomainCommands::Show { domain_id } => {
                let params = serde_json::json!({ "domain_id": domain_id });
                let result = client.call("governance.domain.get", params).await?;
                let domain = result.as_object().context("Invalid domain data")?;

                println!("Governance Domain:");
                println!(
                    "  ID: {}",
                    domain
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
                println!(
                    "  Name: {}",
                    domain
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unnamed")
                );
                println!(
                    "  Profile: {}",
                    domain
                        .get("profile")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );

                if let Some(params_obj) = domain.get("params").and_then(|v| v.as_object()) {
                    println!(
                        "  Quorum: {}%",
                        params_obj
                            .get("quorum_percentage")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                    );
                    println!(
                        "  Approval: {}%",
                        params_obj
                            .get("approval_threshold_percentage")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                    );
                    println!(
                        "  Voting period: {} seconds",
                        params_obj
                            .get("voting_period_seconds")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                    );
                }

                println!(
                    "  Membership: {}",
                    domain
                        .get("membership_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
            }

            DomainCommands::List => {
                let result = client
                    .call("governance.domain.list", serde_json::json!({}))
                    .await?;
                let domains: Vec<serde_json::Value> =
                    serde_json::from_value(result).context("Failed to parse domain list")?;

                println!("Governance Domains:");
                for domain in domains {
                    let id = domain["id"].as_str().unwrap_or("unknown");
                    let name = domain["name"].as_str().unwrap_or("unnamed");
                    println!("  - {id} ({name})");
                }
            }

            DomainCommands::AddMember {
                domain_id,
                did,
                gateway,
                token,
            } => {
                let gateway = gateway
                    .or_else(|| std::env::var("ICN_GATEWAY").ok())
                    .unwrap_or_else(|| "http://localhost:8080".to_string());
                let token = token
                    .or_else(|| std::env::var("ICN_TOKEN").ok())
                    .context("No token provided. Use --token or set ICN_TOKEN env var.")?;

                let http_client = reqwest::Client::new();
                let url = format!("{gateway}/v1/gov/domains/{domain_id}/members");

                let resp = http_client
                    .post(&url)
                    .bearer_auth(&token)
                    .json(&serde_json::json!({
                        "did": did,
                        "weight": 1.0
                    }))
                    .send()
                    .await
                    .context("Failed to connect to gateway")?;

                if resp.status().is_success() {
                    let data: serde_json::Value = resp.json().await?;
                    println!("Member added to governance domain:");
                    println!("  Domain: {domain_id}");
                    println!("  Member: {}", data["member_did"].as_str().unwrap_or(&did));
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    bail!("Failed to add member: {status} - {body}");
                }
            }
        },

        GovCommands::Proposal(proposal_cmd) => {
            match proposal_cmd {
                ProposalCommands::Create {
                    domain_id,
                    title,
                    description,
                    kind,
                    body,
                    amount,
                    currency,
                    recipient,
                    purpose,
                    member,
                    action,
                    new_config,
                } => {
                    // Build payload JSON for RPC (daemon handles proposer DID)
                    let payload_json = match kind.as_str() {
                    "text" => {
                        let body_text = body.context("--body required for text proposals")?;
                        serde_json::json!({
                            "type": "text",
                            "body": body_text
                        })
                    }
                    "budget" => {
                        let amt = amount.context("--amount required for budget proposals")?;
                        let curr = currency.context("--currency required for budget proposals")?;
                        let recip_str = recipient.context("--recipient required for budget proposals")?;
                        let purp = purpose.unwrap_or_default();

                        serde_json::json!({
                            "type": "budget",
                            "amount": amt,
                            "currency": curr,
                            "recipient": recip_str,
                            "purpose": purp
                        })
                    }
                    "membership" => {
                        let member_str = member.context("--member required for membership proposals")?;
                        let action_str = action.unwrap_or_else(|| "add".to_string());

                        if action_str != "add" && action_str != "remove" {
                            bail!("Invalid action: must be 'add' or 'remove'");
                        }

                        serde_json::json!({
                            "type": "membership",
                            "action": action_str,
                            "member": member_str
                        })
                    }
                    "config-change" => {
                        let cfg = new_config.context("--new-config required for config-change proposals")?;
                        serde_json::json!({
                            "type": "config_change",
                            "new_config": cfg
                        })
                    }
                    _ => bail!("Invalid proposal kind. Must be: text, budget, membership, config-change"),
                };

                    // Create proposal via RPC
                    let params = serde_json::json!({
                        "domain_id": domain_id,
                        "title": title,
                        "description": description,
                        "payload": payload_json
                    });

                    let result = client.call("governance.proposal.create", params).await?;
                    let proposal_id = result["proposal_id"]
                        .as_str()
                        .context("Missing proposal_id in response")?;

                    println!("✓ Proposal created:");
                    println!("  ID: {proposal_id}");
                    println!("  State: Draft");
                }

                ProposalCommands::Open {
                    proposal_id,
                    duration,
                } => {
                    // Get proposal to find its domain
                    let get_params = serde_json::json!({ "proposal_id": proposal_id });
                    let proposal_data = client.call("governance.proposal.get", get_params).await?;
                    let domain_id = proposal_data["domain_id"]
                        .as_str()
                        .context("Missing domain_id")?;

                    // Get domain to determine voting period
                    let domain_params = serde_json::json!({ "domain_id": domain_id });
                    let domain_data = client.call("governance.domain.get", domain_params).await?;
                    let default_period = domain_data["params"]["voting_period_seconds"]
                        .as_u64()
                        .unwrap_or(86400);

                    let voting_period = duration.unwrap_or(default_period);

                    // Open the proposal
                    let open_params = serde_json::json!({
                        "proposal_id": proposal_id,
                        "voting_period_seconds": voting_period
                    });
                    client.call("governance.proposal.open", open_params).await?;

                    println!("✓ Proposal opened for voting:");
                    println!("  ID: {proposal_id}");
                    println!("  Duration: {voting_period} seconds");
                }

                ProposalCommands::List { domain_id, state } => {
                    println!("Proposals in domain '{domain_id}':");

                    let result = client
                        .call("governance.proposal.list", serde_json::json!({}))
                        .await?;
                    let proposals: Vec<serde_json::Value> =
                        serde_json::from_value(result).context("Failed to parse proposal list")?;

                    for proposal in proposals {
                        let prop_domain_id = proposal["domain_id"].as_str().unwrap_or("");
                        if prop_domain_id != domain_id {
                            continue;
                        }

                        let prop_state = proposal["state"].as_str().unwrap_or("unknown");

                        if let Some(ref state_filter) = state {
                            if prop_state != *state_filter {
                                continue;
                            }
                        }

                        let state_upper = prop_state.to_uppercase();
                        let id = proposal["id"].as_str().unwrap_or("unknown");
                        let title = proposal["title"].as_str().unwrap_or("untitled");

                        println!("  [{state_upper}] {id} - {title}");
                    }
                }

                ProposalCommands::Show { proposal_id } => {
                    let params = serde_json::json!({ "proposal_id": proposal_id });
                    let proposal = client.call("governance.proposal.get", params).await?;

                    println!("Proposal:");
                    println!("  ID: {}", proposal["id"].as_str().unwrap_or("unknown"));
                    println!(
                        "  Title: {}",
                        proposal["title"].as_str().unwrap_or("untitled")
                    );
                    println!(
                        "  Description: {}",
                        proposal["description"].as_str().unwrap_or("")
                    );
                    println!(
                        "  State: {}",
                        proposal["state"].as_str().unwrap_or("unknown")
                    );
                    println!(
                        "  Proposer: {}",
                        proposal["proposer"].as_str().unwrap_or("unknown")
                    );
                    println!(
                        "  Domain: {}",
                        proposal["domain_id"].as_str().unwrap_or("unknown")
                    );

                    if let Some(opened_at) = proposal["opened_at"].as_u64() {
                        println!("  Opened at: {opened_at} (Unix timestamp)");
                    }
                    if let Some(closes_at) = proposal["closes_at"].as_u64() {
                        println!("  Closes at: {closes_at} (Unix timestamp)");
                    }
                    if let Some(closed_at) = proposal["closed_at"].as_u64() {
                        println!("  Closed at: {closed_at} (Unix timestamp)");
                    }
                }

                ProposalCommands::Close { proposal_id } => {
                    let params = serde_json::json!({ "proposal_id": proposal_id });
                    client.call("governance.proposal.close", params).await?;

                    println!("✓ Proposal closed:");
                    println!("  ID: {proposal_id}");
                    println!("  The daemon has evaluated votes and determined the outcome.");
                }

                ProposalCommands::Cancel { proposal_id: _ } => {
                    bail!("Cancel command not yet supported via RPC. Use 'proposal close' instead, or stop the daemon and modify the store directly.");
                }
            }
        }

        GovCommands::Vote(vote_cmd) => match vote_cmd {
            VoteCommands::Cast {
                proposal_id,
                choice,
                comment,
            } => {
                // Validate choice
                let choice_lower = choice.to_lowercase();
                if choice_lower != "for" && choice_lower != "against" && choice_lower != "abstain" {
                    bail!("Invalid choice. Must be: for, against, abstain");
                }

                // Cast vote via RPC
                let params = serde_json::json!({
                    "proposal_id": proposal_id,
                    "choice": choice_lower,
                    "comment": comment
                });

                client.call("governance.vote.cast", params).await?;

                println!("✓ Vote recorded:");
                println!("  Proposal: {proposal_id}");
                println!("  Choice: {choice_lower}");
            }

            VoteCommands::Show { proposal_id } => {
                bail!("Vote show command not yet supported via RPC. Use 'proposal show {proposal_id}' to see the proposal.");
            }

            VoteCommands::Delegate {
                delegate,
                scope,
                expires,
            } => {
                // Parse expiry duration to timestamp
                let expires_at = if let Some(expires_str) = &expires {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| anyhow::anyhow!("System time error: {e}"))?
                        .as_secs();
                    let duration_secs = parse_duration_to_seconds(expires_str)?;
                    Some(now + duration_secs)
                } else {
                    None
                };

                // Create delegation via RPC
                let params = serde_json::json!({
                    "delegate": delegate,
                    "scope": scope,
                    "expires_at": expires_at
                });

                let result = client.call("governance.delegation.create", params).await?;

                println!("✓ Delegation created:");
                println!("  ID: {}", result["id"].as_str().unwrap_or("unknown"));
                println!("  Delegate: {delegate}");
                println!("  Scope: {scope}");
                if let Some(exp) = expires_at {
                    println!("  Expires: {exp} (Unix timestamp)");
                } else {
                    println!("  Expires: never");
                }
            }

            VoteCommands::Delegations => {
                let result = client
                    .call("governance.delegation.list", serde_json::json!({}))
                    .await?;

                let given = result["given"].as_array();
                let received = result["received"].as_array();

                println!("Delegations Given:");
                if let Some(given_list) = given {
                    if given_list.is_empty() {
                        println!("  (none)");
                    } else {
                        for d in given_list {
                            let active = d["is_active"].as_bool().unwrap_or(false);
                            let status = if active { "active" } else { "inactive" };
                            println!(
                                "  {} → {} [{}] ({})",
                                d["delegator"].as_str().unwrap_or("?"),
                                d["delegate"].as_str().unwrap_or("?"),
                                d["scope"].as_str().unwrap_or("?"),
                                status
                            );
                            println!("    ID: {}", d["id"].as_str().unwrap_or("?"));
                        }
                    }
                } else {
                    println!("  (none)");
                }

                println!("\nDelegations Received:");
                if let Some(received_list) = received {
                    if received_list.is_empty() {
                        println!("  (none)");
                    } else {
                        for d in received_list {
                            let active = d["is_active"].as_bool().unwrap_or(false);
                            let status = if active { "active" } else { "inactive" };
                            println!(
                                "  {} → {} [{}] ({})",
                                d["delegator"].as_str().unwrap_or("?"),
                                d["delegate"].as_str().unwrap_or("?"),
                                d["scope"].as_str().unwrap_or("?"),
                                status
                            );
                            println!("    ID: {}", d["id"].as_str().unwrap_or("?"));
                        }
                    }
                } else {
                    println!("  (none)");
                }
            }

            VoteCommands::Revoke { delegation_id } => {
                let params = serde_json::json!({
                    "delegation_id": delegation_id
                });

                client.call("governance.delegation.revoke", params).await?;

                println!("✓ Delegation revoked:");
                println!("  ID: {delegation_id}");
            }
        },
    }

    Ok(())
}

/// Parse a duration string (e.g., "7d", "30d", "1y") to seconds
///
/// Supported formats:
/// - `Nd` - N days (e.g., "7d", "30d")
/// - `Nw` - N weeks (e.g., "2w", "4w")
/// - `Ny` - N years (e.g., "1y", "2y")
/// - `Nh` - N hours (e.g., "24h", "48h")
/// - Numeric value - seconds (e.g., "3600")
///
/// Note: Years use a fixed 365 days (leap years are not accounted for).
/// For precise long-term delegations, prefer using days or a specific timestamp.
fn parse_duration_to_seconds(s: &str) -> Result<u64> {
    let s = s.trim().to_lowercase();

    if let Some(days) = s.strip_suffix('d') {
        let n: u64 = days
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid duration: {s}"))?;
        return Ok(n * 86400);
    }

    if let Some(weeks) = s.strip_suffix('w') {
        let n: u64 = weeks
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid duration: {s}"))?;
        return Ok(n * 7 * 86400);
    }

    if let Some(years) = s.strip_suffix('y') {
        let n: u64 = years
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid duration: {s}"))?;
        return Ok(n * 365 * 86400);
    }

    if let Some(hours) = s.strip_suffix('h') {
        let n: u64 = hours
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid duration: {s}"))?;
        return Ok(n * 3600);
    }

    // Try parsing as seconds
    let secs: u64 = s
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid duration: {s}. Use format like 7d, 30d, 1y, 24h"))?;
    Ok(secs)
}

fn handle_snapshot_command(cmd: SnapshotCommands, data_dir: &Path) -> Result<()> {
    // Snapshots are stored in the store subdirectory
    let store_dir = data_dir.join("store");

    match cmd {
        SnapshotCommands::Create => {
            println!("{}", t!("cli.snapshot.create.creating"));

            // Check if store directory exists
            if !store_dir.exists() {
                bail!(
                    "Store directory does not exist: {}. Has the daemon been run?",
                    store_dir.display()
                );
            }

            // Load current snapshot (if it exists)
            match icn_snapshot::load_snapshot(&store_dir) {
                Ok(Some(snapshot)) => {
                    // Create timestamped backup
                    icn_snapshot::save_timestamped_snapshot(&snapshot, &store_dir)
                        .context("Failed to create timestamped snapshot")?;

                    // Also save as main snapshot with updated checksum
                    icn_snapshot::save_snapshot(&snapshot, &store_dir)
                        .context("Failed to save snapshot with checksum")?;

                    println!("✓ {}", t!("cli.snapshot.create.success"));
                    println!(
                        "  {}: {}/state.snapshot.{}",
                        t!("cli.snapshot.create.filename"),
                        store_dir.display(),
                        snapshot.created_at
                    );

                    let gossip_peers = snapshot
                        .gossip_state
                        .as_ref()
                        .map_or(0, |g| g.vector_clock.len());
                    let network_peers = snapshot
                        .network_state
                        .as_ref()
                        .map_or(0, |n| n.peer_x25519_keys.len());

                    println!("  Gossip peers: {gossip_peers}");
                    println!("  Network peers: {network_peers}");
                    println!("  SHA256 checksum: generated");
                }
                Ok(None) => {
                    println!(
                        "⚠ No snapshot exists yet. Start the daemon to generate initial state."
                    );
                }
                Err(e) => {
                    bail!("Failed to load snapshot: {e}");
                }
            }
        }

        SnapshotCommands::List => {
            println!("{}", t!("cli.snapshot.list.title"));
            println!();

            match icn_snapshot::list_snapshots(&store_dir) {
                Ok(snapshots) => {
                    if snapshots.is_empty() {
                        println!("{}", t!("cli.snapshot.list.no_snapshots"));
                    } else {
                        println!("{}", t!("cli.snapshot.list.header"));
                        println!("{}", "-".repeat(65));

                        for (filename, timestamp, size) in snapshots {
                            // Format timestamp as human-readable date
                            let datetime =
                                std::time::UNIX_EPOCH + std::time::Duration::from_secs(timestamp);
                            let formatted_date = format!("{datetime:?}");

                            // Format size in KB/MB
                            let formatted_size = if size < 1024 {
                                format!("{size} B")
                            } else if size < 1024 * 1024 {
                                format!("{:.1} KB", size as f64 / 1024.0)
                            } else {
                                format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
                            };

                            println!(
                                "{:<30} {:<20} {:<15}",
                                filename,
                                &formatted_date[..std::cmp::min(19, formatted_date.len())],
                                formatted_size
                            );
                        }

                        println!();
                        println!("Use 'icnctl snapshot verify <snapshot>' to check integrity");
                    }
                }
                Err(e) => {
                    bail!("Failed to list snapshots: {e}");
                }
            }
        }

        SnapshotCommands::Verify { snapshot } => {
            let snapshot_name = snapshot.unwrap_or_else(|| "state.snapshot".to_string());
            println!("{} {snapshot_name}", t!("cli.snapshot.verify.verifying"));

            // Verify the snapshot (main or timestamped)
            let verify_result = if snapshot_name == "state.snapshot" {
                icn_snapshot::verify_snapshot(&store_dir)
            } else {
                icn_snapshot::verify_timestamped_snapshot(&store_dir, &snapshot_name)
            };

            match verify_result {
                Ok(()) => {
                    println!("✓ {}", t!("cli.snapshot.verify.valid"));

                    // Load and display info
                    let load_result = if snapshot_name == "state.snapshot" {
                        icn_snapshot::load_snapshot(&store_dir)
                    } else {
                        icn_snapshot::load_timestamped_snapshot(&store_dir, &snapshot_name)
                            .map(Some)
                    };

                    if let Ok(Some(snapshot)) = load_result {
                        println!();
                        println!("Snapshot details:");
                        println!("  Created: {}", snapshot.created_at);

                        let gossip_peers = snapshot
                            .gossip_state
                            .as_ref()
                            .map_or(0, |g| g.vector_clock.len());
                        let gossip_topics = snapshot
                            .gossip_state
                            .as_ref()
                            .map_or(0, |g| g.subscriptions.len());
                        let network_peers = snapshot
                            .network_state
                            .as_ref()
                            .map_or(0, |n| n.peer_x25519_keys.len());

                        println!("  Gossip peers: {gossip_peers}");
                        println!("  Gossip topics: {gossip_topics}");
                        println!("  Network peers: {network_peers}");
                    }
                }
                Err(e) => {
                    println!("✗ {}", t!("cli.snapshot.verify.invalid"));
                    bail!("{e}");
                }
            }
        }

        SnapshotCommands::Delete { snapshot } => {
            println!("{} {snapshot}", t!("cli.snapshot.delete.deleting"));

            let snapshot_path = store_dir.join(&snapshot);
            let checksum_path = store_dir.join(format!("{snapshot}.sha256"));

            if !snapshot_path.exists() {
                bail!("Snapshot not found: {}", snapshot_path.display());
            }

            // Delete snapshot file
            std::fs::remove_file(&snapshot_path).with_context(|| {
                format!("Failed to delete snapshot: {}", snapshot_path.display())
            })?;

            // Delete checksum file if it exists
            if checksum_path.exists() {
                std::fs::remove_file(&checksum_path).with_context(|| {
                    format!("Failed to delete checksum: {}", checksum_path.display())
                })?;
            }

            println!("✓ {}", t!("cli.snapshot.delete.success"));
        }

        SnapshotCommands::Cleanup { keep } => {
            println!("{}", t!("cli.snapshot.cleanup.cleaning"));

            match icn_snapshot::cleanup_old_snapshots(&store_dir, keep) {
                Ok(deleted) => {
                    if deleted > 0 {
                        println!(
                            "✓ {}",
                            t!("cli.snapshot.cleanup.deleted_count", count = deleted)
                        );
                    } else {
                        println!("{}", t!("cli.snapshot.cleanup.kept_count", count = keep));
                    }
                }
                Err(e) => {
                    bail!("Failed to cleanup snapshots: {e}");
                }
            }
        }
    }

    Ok(())
}

/// Handle authentication commands
async fn handle_auth_command(cmd: AuthCommands, data_dir: &Path) -> Result<()> {
    match cmd {
        AuthCommands::Token {
            gateway,
            coop_id,
            scopes,
        } => {
            // Get keystore path and unlock
            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No keystore found. Run 'icnctl id init' first.");
            }

            // Get passphrase (from env var or prompt)
            let passphrase = read_passphrase("Keystore passphrase: ")?;

            // Unlock keystore
            let mut keystore = icn_identity::keystore::AgeKeyStore::open(&keystore_path)
                .context("Failed to open keystore")?;
            keystore
                .unlock(&passphrase)
                .context("Failed to unlock keystore")?;

            let bundle = keystore
                .get_identity_bundle()
                .context("Keystore is locked")?;
            let did = bundle.did().to_string();
            let keypair = bundle.keypair()?;

            // Parse scopes
            let scope_list: Vec<String> = scopes.split(',').map(|s| s.trim().to_string()).collect();

            println!("Getting token for DID: {did}");
            println!("Gateway: {gateway}");
            println!("Cooperative: {coop_id}");
            println!("Scopes: {}", scope_list.join(", "));
            println!();

            // Create HTTP client
            let client = reqwest::Client::new();

            // Step 1: Get challenge
            let challenge_url = format!("{gateway}/v1/auth/challenge");
            let challenge_req = serde_json::json!({
                "did": did
            });

            let challenge_resp = client
                .post(&challenge_url)
                .json(&challenge_req)
                .send()
                .await
                .context("Failed to connect to gateway")?;

            if !challenge_resp.status().is_success() {
                let status = challenge_resp.status();
                let body = challenge_resp.text().await.unwrap_or_default();
                bail!("Failed to get challenge: {status} - {body}");
            }

            let challenge_data: serde_json::Value = challenge_resp
                .json()
                .await
                .context("Failed to parse challenge response")?;

            let challenge = challenge_data["nonce"]
                .as_str()
                .context("Missing nonce in response")?;

            // Step 2: Sign the challenge (nonce is hex-encoded, decode before signing)
            let nonce_bytes = hex::decode(challenge).context("Invalid nonce format")?;
            let signature = keypair.sign(&nonce_bytes);
            let signature_hex = hex::encode(signature.to_bytes());

            // Step 3: Verify signature and get token
            let verify_url = format!("{gateway}/v1/auth/verify");
            let verify_req = serde_json::json!({
                "did": did,
                "signature": signature_hex,
                "coop_id": coop_id,
                "scopes": scope_list
            });

            let verify_resp = client
                .post(&verify_url)
                .json(&verify_req)
                .send()
                .await
                .context("Failed to verify signature")?;

            if !verify_resp.status().is_success() {
                let status = verify_resp.status();
                let body = verify_resp.text().await.unwrap_or_default();
                bail!("Failed to verify: {status} - {body}");
            }

            let token_data: serde_json::Value = verify_resp
                .json()
                .await
                .context("Failed to parse token response")?;

            let token = token_data["token"]
                .as_str()
                .context("Missing token in response")?;

            // Gateway returns expires_in (relative seconds), calculate absolute expiry
            let expires_in = token_data["expires_in"].as_u64().unwrap_or(3600);
            let expiry_time = chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64);
            let expiry_str = expiry_time.format("%Y-%m-%d %H:%M:%S UTC").to_string();

            println!("✓ Token obtained successfully!");
            println!();
            println!("Token (copy this to use with web UI):");
            println!("────────────────────────────────────────");
            println!("{token}");
            println!("────────────────────────────────────────");
            println!();
            println!("Expires: {expiry_str}");
        }
    }

    Ok(())
}

/// Interactive wizard for setting up a new cooperative
async fn handle_init_coop_command(
    data_dir: &Path,
    name: Option<String>,
    members: Option<String>,
    yes: bool,
    no_start: bool,
) -> Result<()> {
    println!();
    println!("╔════════════════════════════════════════╗");
    println!("║   ICN Cooperative Setup Wizard         ║");
    println!("╚════════════════════════════════════════╝");
    println!();

    // Step 1: Get cooperative name
    let coop_name = if let Some(n) = name {
        n
    } else {
        print!("Cooperative name: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        input.trim().to_string()
    };

    if coop_name.is_empty() {
        bail!("Cooperative name cannot be empty");
    }

    // Generate a domain ID from the name
    let domain_id = coop_name
        .to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>();

    println!("  Name: {coop_name}");
    println!("  Domain ID: {domain_id}");
    println!();

    // Step 2: Check/create identity
    let keystore_path = get_keystore_path(data_dir);
    let my_did = if keystore_path.exists() {
        // Use existing identity
        println!("Step 1: Using existing identity");
        let passphrase = read_passphrase("Enter passphrase: ")?;
        let mut keystore = AgeKeyStore::open(&keystore_path)?;
        keystore.unlock(&passphrase)?;
        let did = keystore.get_keypair()?.did().clone();
        println!("  DID: {did}");
        println!();
        did
    } else {
        // Create new identity
        println!("Step 1: Creating new identity");
        std::fs::create_dir_all(data_dir).context("Failed to create data directory")?;

        print!("Choose a passphrase: ");
        io::stdout().flush()?;
        let passphrase1 = rpassword::read_password()?;

        print!("Confirm passphrase: ");
        io::stdout().flush()?;
        let passphrase2 = rpassword::read_password()?;

        if passphrase1 != passphrase2 {
            bail!("Passphrases do not match");
        }
        if passphrase1.len() < 8 {
            bail!("Passphrase must be at least 8 characters");
        }

        let passphrase = Zeroizing::new(passphrase1.into_bytes());

        // Initialize keystore (generates keypair internally)
        let keystore = AgeKeyStore::init(&keystore_path, &passphrase)?;
        let did = keystore.get_keypair()?.did().clone();

        println!("  DID: {did}");
        println!("  Keystore: {}", keystore_path.display());
        println!();
        did
    };

    // Step 3: Parse initial members
    let mut member_dids = vec![my_did.clone()];

    if let Some(members_str) = members {
        println!("Step 2: Adding initial members");
        for member_str in members_str.split(',') {
            let member_str = member_str.trim();
            if member_str.is_empty() {
                continue;
            }
            let member_did =
                Did::from_str(member_str).with_context(|| format!("Invalid DID: {member_str}"))?;
            if !member_dids.contains(&member_did) {
                member_dids.push(member_did);
            }
        }
    } else if !yes {
        println!("Step 2: Add initial members (or press Enter to skip)");
        println!("  Enter member DIDs, one per line. Empty line to finish:");

        loop {
            print!("  > ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim();

            if input.is_empty() {
                break;
            }

            match Did::from_str(input) {
                Ok(member_did) => {
                    if !member_dids.contains(&member_did) {
                        member_dids.push(member_did);
                        println!("    Added: {input}");
                    } else {
                        println!("    Already added: {input}");
                    }
                }
                Err(e) => {
                    println!("    Invalid DID: {e}");
                }
            }
        }
    }

    println!();
    println!("Initial members ({}):", member_dids.len());
    for (i, did) in member_dids.iter().enumerate() {
        let marker = if did == &my_did { " (you)" } else { "" };
        println!("  {}. {}{}", i + 1, did, marker);
    }
    println!();

    // Step 4: Create configuration file
    let config_path = data_dir.join("icn.toml");
    if !config_path.exists() {
        println!("Step 3: Creating configuration");
        let config_content = format!(
            r#"# ICN Configuration for {coop_name}
# Generated by icnctl init-coop

data_dir = "{}"

[network]
mdns_enabled = true
listen_addr = "0.0.0.0:7777"
rpc_port = 5601

[observability]
metrics_port = 9090
health_port = 8080
log_level = "info"

[rate_limiting]
enabled = true
refill_interval_ms = 100

[rate_limiting.isolated]
max_messages_per_second = 10
burst_capacity = 2

[rate_limiting.known]
max_messages_per_second = 50
burst_capacity = 10

[rate_limiting.partner]
max_messages_per_second = 100
burst_capacity = 20

[rate_limiting.federated]
max_messages_per_second = 200
burst_capacity = 50

[gateway]
enabled = true
bind_addr = "127.0.0.1:8080"
# jwt_secret = "CHANGE_ME"  # Set this before starting!
token_expiry_hours = 24
"#,
            data_dir.display()
        );

        std::fs::write(&config_path, config_content).context("Failed to write configuration")?;
        println!("  Config: {}", config_path.display());
    } else {
        println!("Step 3: Using existing configuration");
        println!("  Config: {}", config_path.display());
    }
    println!();

    // Step 5: Show summary and confirm
    println!("════════════════════════════════════════");
    println!("Summary:");
    println!("  Cooperative: {coop_name}");
    println!("  Domain ID: {domain_id}");
    println!("  Your DID: {my_did}");
    println!("  Members: {}", member_dids.len());
    println!("  Data dir: {}", data_dir.display());
    println!("════════════════════════════════════════");
    println!();

    if !yes {
        print!("Proceed with setup? (Y/n): ");
        io::stdout().flush()?;
        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        if response.trim().eq_ignore_ascii_case("n") {
            println!("Setup cancelled.");
            return Ok(());
        }
    }

    // Step 6: Create initial governance domain
    // Save bootstrap file for offline use, then try live gateway call.
    let governance_setup_path = data_dir.join("governance_setup.json");
    let governance_setup = serde_json::json!({
        "domain": {
            "id": domain_id,
            "name": coop_name,
            "members": member_dids.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
            "profile": "cooperative_default"
        }
    });

    std::fs::write(
        &governance_setup_path,
        serde_json::to_string_pretty(&governance_setup)?,
    )
    .context("Failed to write governance setup")?;

    // Try to create the governance domain live if daemon is running
    let gateway_url =
        std::env::var("ICN_GATEWAY").unwrap_or_else(|_| "http://localhost:8080".to_string());
    let mut domain_created_live = false;

    // Check if daemon is running by hitting health endpoint
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    match http_client
        .get(format!("{gateway_url}/v1/health"))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            println!("Step 4: Daemon detected, creating governance domain live...");

            // Try to get a token if ICN_TOKEN is set
            if let Ok(token) = std::env::var("ICN_TOKEN") {
                let create_resp = http_client
                    .post(format!("{gateway_url}/v1/gov/domains"))
                    .bearer_auth(&token)
                    .json(&serde_json::json!({
                        "id": domain_id,
                        "name": coop_name,
                        "profile": "cooperative_default",
                        "quorum_percent": 50,
                        "approval_percent": 50,
                        "voting_period_days": 7,
                        "members": member_dids.iter().map(|d| d.to_string()).collect::<Vec<_>>()
                    }))
                    .send()
                    .await;

                match create_resp {
                    Ok(r) if r.status().is_success() => {
                        domain_created_live = true;
                        println!("  Domain created via gateway API");
                    }
                    Ok(r) => {
                        let status = r.status();
                        let body = r.text().await.unwrap_or_default();
                        println!("  Gateway domain creation returned {status}: {body}");
                        println!("  Falling back to bootstrap file.");
                    }
                    Err(e) => {
                        println!("  Gateway request failed: {e}");
                        println!("  Falling back to bootstrap file.");
                    }
                }
            } else {
                println!("  No ICN_TOKEN set, skipping live domain creation.");
                println!("  Set ICN_TOKEN to enable auto-setup.");
            }
        }
        _ => {
            println!("Step 4: Daemon not running, saving bootstrap file.");
        }
    }

    if domain_created_live {
        println!("  Domain ID: {domain_id}");
        println!("  Profile: cooperative_default (1-member-1-vote, 50% quorum)");
    } else {
        println!("  Bootstrap file: {}", governance_setup_path.display());
        println!("  Domain ID: {domain_id}");
        println!("  Profile: cooperative_default (1-member-1-vote, 50% quorum)");
    }
    println!();

    // Step 7: Create trust edges for initial members
    let store_path = get_store_path(data_dir);
    std::fs::create_dir_all(&store_path)?;
    let store = SledStore::open(&store_path).context("Failed to open store")?;
    let store = Arc::new(store);
    let mut trust_graph = TrustGraph::new(store, my_did.clone());

    for member_did in &member_dids {
        if member_did != &my_did {
            // Add bidirectional trust at "partner" level (0.5)
            let edge = TrustEdge::new(
                my_did.clone(),
                member_did.clone(),
                TrustScore::unchecked(0.5),
            );
            trust_graph.add_edge(edge)?;
        }
    }

    println!(
        "✓ Trust edges created for {} member(s)",
        member_dids.len() - 1
    );
    println!();

    // Step 8: Final instructions
    println!("════════════════════════════════════════");
    println!("  Setup Complete!");
    println!("════════════════════════════════════════");
    println!();

    if domain_created_live {
        println!("Governance domain created successfully via daemon.");
        println!();
        println!("To add more members later:");
        println!("  icnctl gov domain add-member --domain-id {domain_id} --did <MEMBER_DID> --token <TOKEN>");
        println!();
        println!("To connect to federation peers:");
        println!("  icnctl federation gateway-connect <HOST:PORT> --token <TOKEN>");
        println!();
        println!("Share the invite info with other members:");
        println!("  Your DID: {my_did}");
        println!("  Domain ID: {domain_id}");
    } else if no_start {
        println!("Next steps:");
        println!();
        println!("  1. Edit configuration:");
        println!("     $EDITOR {}", config_path.display());
        println!();
        println!("  2. Set JWT secret for gateway:");
        println!("     export ICN_GATEWAY_JWT_SECRET=\"your-secret-here\"");
        println!();
        println!("  3. Start the daemon:");
        println!("     icnd --config {}", config_path.display());
        println!();
        println!("  4. Create the governance domain:");
        println!("     icnctl gov domain create --domain-id {domain_id} --name \"{coop_name}\" \\");
        let members_str = member_dids
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(",");
        println!("       --members \"{members_str}\"");
        println!();
        println!("  5. Share the invite info with other members:");
        println!("     Your DID: {my_did}");
        println!("     Domain ID: {domain_id}");
    } else {
        println!("To complete setup:");
        println!();
        println!("  1. Set JWT secret:");
        println!("     export ICN_GATEWAY_JWT_SECRET=\"your-secret-here\"");
        println!();
        println!("  2. Start the daemon:");
        println!("     icnd --config {}", config_path.display());
        println!();
        println!("  3. Create governance domain (after daemon starts):");
        println!("     icnctl gov domain create --domain-id {domain_id} --name \"{coop_name}\" \\");
        let members_str = member_dids
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(",");
        println!("       --members \"{members_str}\"");
    }

    println!();
    println!("Documentation: https://icn.coop/docs");
    println!();

    Ok(())
}

async fn handle_compute_command(
    cmd: ComputeCommands,
    data_dir: &Path,
    endpoint: &str,
) -> Result<()> {
    // Create authenticated RPC client
    let mut client = create_authenticated_rpc_client(endpoint, data_dir)?;

    match cmd {
        ComputeCommands::Submit {
            contract,
            id,
            fuel,
            priority,
            inputs,
            payment_rate,
            payment_currency,
        } => {
            // Read contract JSON
            let contract_json = std::fs::read_to_string(&contract)
                .with_context(|| format!("Failed to read contract file: {contract:?}"))?;

            // Read inputs if provided
            let inputs_value: serde_json::Value = if let Some(inputs_path) = inputs {
                let inputs_json = std::fs::read_to_string(&inputs_path)
                    .with_context(|| format!("Failed to read inputs file: {inputs_path:?}"))?;
                serde_json::from_str(&inputs_json)?
            } else {
                serde_json::Value::Null
            };

            let task_id = id.unwrap_or_else(|| format!("task-{}", uuid::Uuid::new_v4()));

            let params = serde_json::json!({
                "task_id": task_id,
                "code": contract_json,
                "inputs": inputs_value,
                "fuel_limit": fuel,
                "priority": priority,
                "payment_rate": payment_rate,
                "payment_currency": payment_currency,
            });

            let result = client.call("compute.submit", params).await?;

            let task_hash = result
                .get("task_hash")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            println!("✓ {}", t!("cli.compute.submit.success"));
            println!("{}: {task_id}", t!("cli.compute.submit.task_id"));
            println!("{}: {task_hash}", t!("cli.compute.submit.task_hash"));
            println!();
            println!("Check status with:");
            println!("  icnctl compute status {task_hash}");
        }

        ComputeCommands::SubmitWasm {
            wasm,
            id,
            fuel,
            priority,
            inputs,
            payment_rate,
            payment_currency,
        } => {
            use base64::Engine;

            // Read WASM binary
            let wasm_bytes = std::fs::read(&wasm)
                .with_context(|| format!("Failed to read WASM file: {wasm:?}"))?;

            // Encode as base64
            let wasm_b64 = base64::engine::general_purpose::STANDARD.encode(&wasm_bytes);

            // Read inputs if provided
            let inputs_value: serde_json::Value = if let Some(inputs_path) = inputs {
                let inputs_json = std::fs::read_to_string(&inputs_path)
                    .with_context(|| format!("Failed to read inputs file: {inputs_path:?}"))?;
                serde_json::from_str(&inputs_json)?
            } else {
                serde_json::Value::Null
            };

            let task_id = id.unwrap_or_else(|| format!("task-{}", uuid::Uuid::new_v4()));

            let params = serde_json::json!({
                "task_id": task_id,
                "code_type": "wasm",
                "wasm_bytes": wasm_b64,
                "inputs": inputs_value,
                "fuel_limit": fuel,
                "priority": priority,
                "payment_rate": payment_rate,
                "payment_currency": payment_currency,
            });

            let result = client.call("compute.submit", params).await?;

            let task_hash = result
                .get("task_hash")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            println!("✓ WASM {}", t!("cli.compute.submit.success"));
            println!("{}: {task_id}", t!("cli.compute.submit.task_id"));
            println!("{}: {task_hash}", t!("cli.compute.submit.task_hash"));
            println!("WASM size: {} bytes", wasm_bytes.len());
            println!();
            println!("Check status with:");
            println!("  icnctl compute status {task_hash}");
        }

        ComputeCommands::Status { task_hash } => {
            let params = serde_json::json!({ "task_hash": task_hash });
            let result = client.call("compute.status", params).await?;

            let status = result
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            println!("Task:   {task_hash}");
            println!("{}: {status}", t!("cli.compute.status.state_label"));

            if let Some(executor) = result.get("executor").and_then(|v| v.as_str()) {
                println!("Executor: {executor}");
            }

            if let Some(task_result) = result.get("result") {
                let outcome = task_result
                    .get("outcome")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let fuel_used = task_result
                    .get("fuel_used")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let duration_ms = task_result
                    .get("duration_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                println!();
                println!("{}:", t!("cli.compute.status.result_label"));
                println!("  Outcome:     {outcome}");
                println!("  {}:   {fuel_used}", t!("cli.compute.status.fuel_used"));
                println!("  Duration:    {duration_ms}ms");

                if let Some(output) = task_result.get("output") {
                    if !output.is_null() {
                        println!("  Output:      {}", serde_json::to_string_pretty(output)?);
                    }
                }

                if let Some(error) = task_result.get("error").and_then(|v| v.as_str()) {
                    println!("  Error:       {error}");
                }
            }
        }

        ComputeCommands::Cancel { task_hash, reason } => {
            let params = serde_json::json!({
                "task_hash": task_hash,
                "reason": reason,
            });
            let result = client.call("compute.cancel", params).await?;

            let status = result
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            println!("✓ {}", t!("cli.compute.cancel.success"));
            println!("Task hash: {task_hash}");
            println!("{}: {status}", t!("cli.compute.status.state_label"));
            println!("Reason:    {reason}");
        }
    }

    Ok(())
}

async fn handle_policy_command(cmd: PolicyCommands, data_dir: &Path, endpoint: &str) -> Result<()> {
    // Create authenticated RPC client
    let mut client = create_authenticated_rpc_client(endpoint, data_dir)?;

    match cmd {
        PolicyCommands::Set { coop_id, policy } => {
            // Read policy JSON
            let policy_json = std::fs::read_to_string(&policy)
                .with_context(|| format!("Failed to read policy file: {policy:?}"))?;

            let policy_value: serde_json::Value = serde_json::from_str(&policy_json)?;

            let params = serde_json::json!({
                "coop_id": coop_id,
                "policy": policy_value,
            });

            client.call("policy.set", params).await?;

            println!("✓ Policy set for cooperative: {coop_id}");
            println!();
            println!("View policy with:");
            println!("  icnctl policy show --coop-id {coop_id}");
        }

        PolicyCommands::Show { coop_id } => {
            let params = serde_json::json!({ "coop_id": coop_id });
            let result = client.call("policy.get", params).await?;

            if result.is_null() {
                println!("No policy set for cooperative: {coop_id}");
                return Ok(());
            }

            println!("Policy for cooperative: {coop_id}");
            println!();
            println!("{}", serde_json::to_string_pretty(&result)?);
        }

        PolicyCommands::List => {
            let params = serde_json::json!({});
            let result = client.call("policy.list", params).await?;

            let policies = result.as_array().context("Expected array of policies")?;

            if policies.is_empty() {
                println!("No policies configured");
                return Ok(());
            }

            println!("Configured Policies:");
            for policy in policies {
                let coop_id = policy
                    .get("coop_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let enforcement_mode = policy
                    .get("enforcement_mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let rules_count = policy
                    .get("rules")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                println!("  - {coop_id}: {rules_count} rules ({enforcement_mode})");
            }
        }

        PolicyCommands::Remove { coop_id } => {
            let params = serde_json::json!({ "coop_id": coop_id });
            client.call("policy.remove", params).await?;

            println!("✓ Policy removed for cooperative: {coop_id}");
        }
    }

    Ok(())
}

async fn handle_quota_command(cmd: QuotaCommands, data_dir: &Path, endpoint: &str) -> Result<()> {
    // Create authenticated RPC client
    let mut client = create_authenticated_rpc_client(endpoint, data_dir)?;

    match cmd {
        QuotaCommands::Show { coop_id, member } => {
            let params = serde_json::json!({
                "coop_id": coop_id,
                "member_did": member,
            });
            let result = client.call("quota.usage", params).await?;

            println!("Usage for {member} in {coop_id}:");
            println!();

            let cpu_hours_month = result
                .get("cpu_hours_this_month")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let cpu_hours_total = result
                .get("cpu_hours_total")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let concurrent = result
                .get("concurrent_tasks")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let completed = result
                .get("tasks_completed_this_month")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let credits_spent = result
                .get("credits_spent_this_month")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            println!("  CPU Hours (this month): {cpu_hours_month:.2}");
            println!("  CPU Hours (total):      {cpu_hours_total:.2}");
            println!("  Concurrent tasks:       {concurrent}");
            println!("  Tasks completed:        {completed}");
            println!("  Credits spent:          {credits_spent}");
        }

        QuotaCommands::List { coop_id } => {
            let params = serde_json::json!({ "coop_id": coop_id });
            let result = client.call("quota.list", params).await?;

            let usage_records = result
                .as_array()
                .context("Expected array of usage records")?;

            if usage_records.is_empty() {
                println!("No usage records for cooperative: {coop_id}");
                return Ok(());
            }

            println!("Resource Usage for {coop_id}:");
            println!();
            println!(
                "{:<60} {:>12} {:>10} {:>12}",
                "Member", "CPU Hours", "Tasks", "Credits"
            );
            println!("{:-<96}", "");

            for record in usage_records {
                let member = record
                    .get("member_did")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let cpu_hours = record
                    .get("cpu_hours_this_month")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let tasks = record
                    .get("tasks_completed_this_month")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let credits = record
                    .get("credits_spent_this_month")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                let short_member = if member.len() > 55 {
                    format!("{}...", &member[0..52])
                } else {
                    member.to_string()
                };

                println!("{short_member:<60} {cpu_hours:>12.2} {tasks:>10} {credits:>12}");
            }
        }
    }

    Ok(())
}

async fn handle_dispute_command(cmd: DisputeCommands, endpoint: &str) -> Result<()> {
    let rpc_addr = endpoint.parse()?;
    let mut client = icn_rpc::RpcClient::new(rpc_addr);

    match cmd {
        DisputeCommands::File { entry_hash, reason } => {
            let result = client
                .dispute_file(&entry_hash, &reason)
                .await
                .context("Failed to file dispute. Is icnd running?")?;

            println!("Dispute filed successfully!");
            println!();
            println!("  Entry Hash: {entry_hash}");
            println!("  Reason:     {reason}");

            if let Some(filed_at) = result.get("filed_at") {
                println!("  Filed At:   {filed_at}");
            }
        }

        DisputeCommands::List { status, filer } => {
            let status_filter = if status == "all" {
                None
            } else {
                Some(status.as_str())
            };

            let result = client
                .dispute_list(status_filter, filer.as_deref())
                .await
                .context("Failed to list disputes. Is icnd running?")?;

            let disputes = result
                .get("disputes")
                .and_then(|d| d.as_array())
                .cloned()
                .unwrap_or_default();

            if disputes.is_empty() {
                println!("No disputes found.");
            } else {
                println!(
                    "{:<20} {:<12} {:<45} Filed At",
                    "Entry Hash", "Status", "Filer"
                );
                println!("{:-<100}", "");

                for dispute in &disputes {
                    let hash = dispute
                        .get("entry_hash")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let status = dispute
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let filer = dispute
                        .get("filer")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let filed_at = dispute
                        .get("filed_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    let short_hash = if hash.len() > 18 {
                        format!("{}...", &hash[0..15])
                    } else {
                        hash.to_string()
                    };

                    let short_filer = if filer.len() > 43 {
                        format!("{}...", &filer[0..40])
                    } else {
                        filer.to_string()
                    };

                    println!("{short_hash:<20} {status:<12} {short_filer:<45} {filed_at}");
                }

                println!();
                println!("Total: {} dispute(s)", disputes.len());
            }
        }

        DisputeCommands::Get { entry_hash } => {
            let result = client
                .dispute_get(&entry_hash)
                .await
                .context("Failed to get dispute. Is icnd running?")?;

            if let Some(dispute) = result.get("dispute") {
                println!("Dispute Details:");
                println!();
                println!(
                    "  Entry Hash: {}",
                    dispute
                        .get("entry_hash")
                        .and_then(|v| v.as_str())
                        .unwrap_or("N/A")
                );
                println!(
                    "  Status:     {}",
                    dispute
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("N/A")
                );
                println!(
                    "  Filer:      {}",
                    dispute
                        .get("filer")
                        .and_then(|v| v.as_str())
                        .unwrap_or("N/A")
                );
                println!(
                    "  Reason:     {}",
                    dispute
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("N/A")
                );
                println!(
                    "  Filed At:   {}",
                    dispute
                        .get("filed_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("N/A")
                );

                if let Some(mediator) = dispute.get("mediator").and_then(|v| v.as_str()) {
                    println!("  Mediator:   {mediator}");
                }

                if let Some(outcome) = dispute.get("outcome").and_then(|v| v.as_str()) {
                    println!("  Outcome:    {outcome}");
                }

                if let Some(resolved_at) = dispute.get("resolved_at").and_then(|v| v.as_str()) {
                    println!("  Resolved:   {resolved_at}");
                }

                // Display evidence
                if let Some(evidence) = dispute.get("evidence").and_then(|v| v.as_array()) {
                    if !evidence.is_empty() {
                        println!();
                        println!("  Evidence ({} item(s)):", evidence.len());
                        for (i, e) in evidence.iter().enumerate() {
                            let text = e.get("text").and_then(|v| v.as_str()).unwrap_or("N/A");
                            let submitted_by = e
                                .get("submitted_by")
                                .and_then(|v| v.as_str())
                                .unwrap_or("N/A");
                            let submitted_at = e
                                .get("submitted_at")
                                .and_then(|v| v.as_str())
                                .unwrap_or("N/A");

                            println!("    {}. \"{}\"", i + 1, text);
                            println!("       By: {submitted_by}");
                            println!("       At: {submitted_at}");
                        }
                    }
                }
            } else if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
                println!("Error: {error}");
            } else {
                println!("Dispute not found for entry hash: {entry_hash}");
            }
        }

        DisputeCommands::AddEvidence {
            entry_hash,
            evidence,
        } => {
            let result = client
                .dispute_add_evidence(&entry_hash, &evidence)
                .await
                .context("Failed to add evidence. Is icnd running?")?;

            if result
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                println!("Evidence added successfully!");
                println!();
                println!("  Entry Hash: {entry_hash}");
                println!("  Evidence:   {evidence}");
            } else if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
                println!("Failed to add evidence: {error}");
            }
        }

        DisputeCommands::AssignMediator {
            entry_hash,
            mediator,
        } => {
            let result = client
                .dispute_assign_mediator(&entry_hash, &mediator)
                .await
                .context("Failed to assign mediator. Is icnd running?")?;

            if result
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                println!("Mediator assigned successfully!");
                println!();
                println!("  Entry Hash: {entry_hash}");
                println!("  Mediator:   {mediator}");
            } else if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
                println!("Failed to assign mediator: {error}");
            }
        }

        DisputeCommands::Resolve {
            entry_hash,
            outcome,
        } => {
            // Validate outcome
            let valid_outcomes = ["upheld", "reversed", "settlement", "writeoff"];
            if !valid_outcomes.contains(&outcome.as_str()) {
                anyhow::bail!(
                    "Invalid outcome '{}'. Valid outcomes: {}",
                    outcome,
                    valid_outcomes.join(", ")
                );
            }

            let result = client
                .dispute_resolve(&entry_hash, &outcome)
                .await
                .context("Failed to resolve dispute. Is icnd running?")?;

            if result
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                println!("Dispute resolved successfully!");
                println!();
                println!("  Entry Hash: {entry_hash}");
                println!("  Outcome:    {outcome}");

                if let Some(resolved_at) = result.get("resolved_at").and_then(|v| v.as_str()) {
                    println!("  Resolved:   {resolved_at}");
                }
            } else if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
                println!("Failed to resolve dispute: {error}");
            }
        }
    }

    Ok(())
}

async fn handle_steward_command(
    cmd: StewardCommands,
    data_dir: &Path,
    endpoint: &str,
) -> Result<()> {
    match cmd {
        StewardCommands::Status => {
            println!("Steward Network Status");
            println!("======================\n");

            // Read config to check if steward is enabled
            let config_path = data_dir.join("config.toml");
            if config_path.exists() {
                let config_content = std::fs::read_to_string(&config_path)?;
                if config_content.contains("steward") && config_content.contains("enabled = true") {
                    println!("Status:     ENABLED");
                } else {
                    println!("Status:     DISABLED");
                    println!("\nTo enable steward mode, add to config.toml:");
                    println!("  [steward]");
                    println!("  enabled = true");
                    println!("  vui_threshold = 3");
                    println!("  vui_total_shares = 5");
                    return Ok(());
                }
            } else {
                println!("Status:     DISABLED (no config.toml found)");
                return Ok(());
            }

            // Try to get steward stats via RPC
            let mut client = create_rpc_client(endpoint, data_dir, false)?;
            match client.get_status().await {
                Ok(status) => {
                    if status.running {
                        println!("Daemon:     RUNNING");
                    } else {
                        println!("Daemon:     NOT RUNNING");
                    }
                    if !status.listen_addr.is_empty() {
                        println!("Listen:     {}", status.listen_addr);
                    }
                }
                Err(e) => {
                    println!("Warning: Could not reach daemon: {e}");
                }
            }

            println!("\nGossip Topics:");
            println!("  steward:announce  - Steward announcements");
            println!("  steward:vui-sync  - VUI registry synchronization");
            println!("  steward:enrollment - Enrollment ceremony coordination");
            println!("  steward:recovery  - Recovery ceremony coordination");
        }

        StewardCommands::Config => {
            println!("Steward Configuration");
            println!("=====================\n");

            // Read config
            let config_path = data_dir.join("config.toml");
            if !config_path.exists() {
                println!("No config.toml found at {}", config_path.display());
                println!("\nDefault steward configuration:");
                print_default_steward_config();
                return Ok(());
            }

            let config_content = std::fs::read_to_string(&config_path)?;
            if config_content.contains("[steward]") {
                // Parse and display steward section
                let config: toml::Value = toml::from_str(&config_content)?;
                if let Some(steward) = config.get("steward") {
                    let enabled = steward
                        .get("enabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let threshold = steward
                        .get("vui_threshold")
                        .and_then(|v| v.as_integer())
                        .unwrap_or(3);
                    let total = steward
                        .get("vui_total_shares")
                        .and_then(|v| v.as_integer())
                        .unwrap_or(5);
                    let max_enroll = steward
                        .get("max_concurrent_enrollments")
                        .and_then(|v| v.as_integer())
                        .unwrap_or(100);
                    let max_recover = steward
                        .get("max_concurrent_recoveries")
                        .and_then(|v| v.as_integer())
                        .unwrap_or(50);
                    let token_validity = steward
                        .get("token_validity_secs")
                        .and_then(|v| v.as_integer())
                        .unwrap_or(604800);

                    println!("enabled:                   {enabled}");
                    println!("vui_threshold:             {threshold}");
                    println!("vui_total_shares:          {total}");
                    println!("max_concurrent_enrollments: {max_enroll}");
                    println!("max_concurrent_recoveries:  {max_recover}");
                    println!(
                        "token_validity_secs:       {token_validity} ({} days)",
                        token_validity / 86400
                    );
                } else {
                    println!("No [steward] section in config.toml");
                    println!("\nDefault configuration:");
                    print_default_steward_config();
                }
            } else {
                println!("No [steward] section in config.toml");
                println!("\nDefault configuration:");
                print_default_steward_config();
            }
        }

        StewardCommands::Info { steward, gateway } => {
            println!("Steward Info");
            println!("============\n");

            let gateway = gateway
                .or_else(|| std::env::var("ICN_GATEWAY").ok())
                .unwrap_or_else(|| "http://localhost:8080".to_string());
            let client = reqwest::Client::new();

            // Try as DID first, then as steward_id
            let url = if steward.starts_with("did:") {
                format!("{gateway}/v1/steward/by-did/{steward}")
            } else {
                format!("{gateway}/v1/steward/{steward}")
            };

            match client.get(&url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let data: serde_json::Value = resp.json().await?;
                        println!(
                            "Steward ID:     {}",
                            data["steward_id"].as_str().unwrap_or("unknown")
                        );
                        println!(
                            "Steward DID:    {}",
                            data["steward_did"].as_str().unwrap_or("unknown")
                        );
                        println!(
                            "Holder DID:     {}",
                            data["holder_did"].as_str().unwrap_or("unknown")
                        );
                        println!(
                            "Status:         {}",
                            data["status"].as_str().unwrap_or("unknown")
                        );
                        println!(
                            "Can Attest:     {}",
                            if data["can_attest"].as_bool().unwrap_or(false) {
                                "Yes"
                            } else {
                                "No"
                            }
                        );
                        println!(
                            "Reputation:     {:.2}",
                            data["reputation_score"].as_f64().unwrap_or(0.0)
                        );
                        println!(
                            "Effectiveness:  {:.2}",
                            data["effectiveness_score"].as_f64().unwrap_or(0.0)
                        );
                        println!(
                            "Attestations:   {} issued, {} disputed",
                            data["attestations_issued"].as_u64().unwrap_or(0),
                            data["attestations_disputed"].as_u64().unwrap_or(0)
                        );
                        println!(
                            "Disputes:       {} against, {} won",
                            data["disputes_against"].as_u64().unwrap_or(0),
                            data["disputes_won"].as_u64().unwrap_or(0)
                        );
                        println!(
                            "Bond:           {} credits",
                            data["bond_amount"].as_u64().unwrap_or(0)
                        );
                        if let Some(jurisdiction) = data["jurisdiction"].as_str() {
                            println!("Jurisdiction:   {jurisdiction}");
                        }
                        if let Some(specs) = data["specializations"].as_array() {
                            if !specs.is_empty() {
                                let spec_strs: Vec<&str> =
                                    specs.iter().filter_map(|s| s.as_str()).collect();
                                println!("Specializations: {}", spec_strs.join(", "));
                            }
                        }
                        println!(
                            "Term Expired:   {}",
                            if data["is_term_expired"].as_bool().unwrap_or(false) {
                                "Yes"
                            } else {
                                "No"
                            }
                        );
                    } else if status == reqwest::StatusCode::NOT_FOUND {
                        println!("Steward not found: {steward}");
                    } else {
                        let text = resp.text().await.unwrap_or_default();
                        println!("Error: {status} - {text}");
                    }
                }
                Err(e) => {
                    println!("Could not reach gateway at {gateway}: {e}");
                }
            }
        }

        StewardCommands::List {
            active,
            jurisdiction,
            gateway,
        } => {
            println!("Registered Stewards");
            println!("===================\n");

            let gateway = gateway
                .or_else(|| std::env::var("ICN_GATEWAY").ok())
                .unwrap_or_else(|| "http://localhost:8080".to_string());
            let client = reqwest::Client::new();

            let mut url = format!("{gateway}/v1/steward");
            let mut params = vec![];
            if active {
                params.push("active=true".to_string());
            }
            if let Some(j) = jurisdiction {
                params.push(format!("jurisdiction={j}"));
            }
            if !params.is_empty() {
                url = format!("{}?{}", url, params.join("&"));
            }

            match client.get(&url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let stewards: Vec<serde_json::Value> = resp.json().await?;
                        if stewards.is_empty() {
                            println!("No stewards found.");
                        } else {
                            println!(
                                "{:<64} {:<10} {:<8} {:<12}",
                                "STEWARD_ID", "STATUS", "CAN_ATT", "REPUTATION"
                            );
                            println!("{}", "-".repeat(100));
                            for s in stewards {
                                println!(
                                    "{:<64} {:<10} {:<8} {:<12.2}",
                                    s["steward_id"].as_str().unwrap_or("-"),
                                    s["status"].as_str().unwrap_or("-"),
                                    if s["can_attest"].as_bool().unwrap_or(false) {
                                        "Yes"
                                    } else {
                                        "No"
                                    },
                                    s["reputation_score"].as_f64().unwrap_or(0.0)
                                );
                            }
                        }
                    } else {
                        let text = resp.text().await.unwrap_or_default();
                        println!("Error: {status} - {text}");
                    }
                }
                Err(e) => {
                    println!("Could not reach gateway at {gateway}: {e}");
                }
            }
        }

        StewardCommands::Attesters { gateway } => {
            println!("Active Attesters");
            println!("================\n");

            let gateway = gateway
                .or_else(|| std::env::var("ICN_GATEWAY").ok())
                .unwrap_or_else(|| "http://localhost:8080".to_string());
            let client = reqwest::Client::new();

            let url = format!("{gateway}/v1/steward/attesters");

            match client.get(&url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let attesters: Vec<serde_json::Value> = resp.json().await?;
                        if attesters.is_empty() {
                            println!("No attesters available.");
                        } else {
                            println!(
                                "{:<64} {:<8} {:<12} {:<12}",
                                "STEWARD_ID", "ATTESTED", "REPUTATION", "JURISDICTION"
                            );
                            println!("{}", "-".repeat(100));
                            for s in attesters {
                                println!(
                                    "{:<64} {:<8} {:<12.2} {:<12}",
                                    s["steward_id"].as_str().unwrap_or("-"),
                                    s["attestations_issued"].as_u64().unwrap_or(0),
                                    s["reputation_score"].as_f64().unwrap_or(0.0),
                                    s["jurisdiction"].as_str().unwrap_or("global")
                                );
                            }
                        }
                    } else {
                        let text = resp.text().await.unwrap_or_default();
                        println!("Error: {status} - {text}");
                    }
                }
                Err(e) => {
                    println!("Could not reach gateway at {gateway}: {e}");
                }
            }
        }

        StewardCommands::Register {
            term_days,
            bond,
            governance_approval,
            jurisdiction,
            specializations,
            gateway,
        } => {
            println!("Register as Steward");
            println!("===================\n");

            let gateway = gateway
                .or_else(|| std::env::var("ICN_GATEWAY").ok())
                .unwrap_or_else(|| "http://localhost:8080".to_string());
            let client = reqwest::Client::new();

            // Get auth token from keystore
            let keystore_path = data_dir.join("keystore.age");
            if !keystore_path.exists() {
                bail!("No keystore found. Run 'icnctl id init' first.");
            }

            // Build request body
            let specs: Vec<String> = specializations
                .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
                .unwrap_or_default();

            let body = serde_json::json!({
                "term_duration_days": term_days,
                "bond_amount": bond,
                "governance_approval": governance_approval,
                "jurisdiction": jurisdiction,
                "specializations": specs,
            });

            let url = format!("{gateway}/v1/steward");

            // Note: In production, this would need auth token
            match client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let data: serde_json::Value = resp.json().await?;
                        println!("Successfully registered as steward!");
                        println!(
                            "  Steward ID: {}",
                            data["steward_id"].as_str().unwrap_or("unknown")
                        );
                        println!("  Term: {term_days} days");
                        println!("  Bond: {bond} credits");
                    } else {
                        let text = resp.text().await.unwrap_or_default();
                        println!("Registration failed: {status} - {text}");
                    }
                }
                Err(e) => {
                    println!("Could not reach gateway at {gateway}: {e}");
                }
            }
        }

        StewardCommands::Retire {
            steward_id,
            gateway,
        } => {
            println!("Retire from Stewardship");
            println!("=======================\n");

            let gateway = gateway
                .or_else(|| std::env::var("ICN_GATEWAY").ok())
                .unwrap_or_else(|| "http://localhost:8080".to_string());
            let client = reqwest::Client::new();

            let id = steward_id.unwrap_or_else(|| "me".to_string());
            if id == "me" {
                println!("Note: Retiring your own stewardship (use --steward-id for specific ID)");
            }

            let url = format!("{gateway}/v1/steward/{id}/retire");

            // Note: In production, this would need auth token
            match client.post(&url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        println!("Successfully retired from stewardship.");
                    } else if status == reqwest::StatusCode::NOT_FOUND {
                        println!("Steward not found: {id}");
                    } else {
                        let text = resp.text().await.unwrap_or_default();
                        println!("Retirement failed: {status} - {text}");
                    }
                }
                Err(e) => {
                    println!("Could not reach gateway at {gateway}: {e}");
                }
            }
        }

        StewardCommands::CheckVui { vui_hash } => {
            // Parse VUI hash
            let hash_bytes = hex::decode(&vui_hash)
                .with_context(|| format!("Invalid VUI hash hex: {vui_hash}"))?;
            if hash_bytes.len() != 32 {
                bail!(
                    "VUI hash must be 32 bytes (64 hex chars), got {} bytes",
                    hash_bytes.len()
                );
            }

            println!("Checking VUI registry for hash: {}...", &vui_hash[..16]);

            // Note: In a full implementation, this would query the steward network
            // For now, we just validate the input and print a placeholder
            println!("\n⚠️  VUI registry check requires running steward daemon.");
            println!("   Start daemon with steward enabled in config.toml");
        }

        StewardCommands::StartEnrollment {
            vui_commitment,
            pathway_hash,
        } => {
            // Validate inputs
            let commitment_bytes = hex::decode(&vui_commitment)
                .with_context(|| format!("Invalid VUI commitment hex: {vui_commitment}"))?;
            if commitment_bytes.len() != 32 {
                bail!("VUI commitment must be 32 bytes");
            }

            let pathway_bytes = hex::decode(&pathway_hash)
                .with_context(|| format!("Invalid pathway hash hex: {pathway_hash}"))?;
            if pathway_bytes.len() != 8 {
                bail!("Pathway hash must be 8 bytes");
            }

            println!("Starting enrollment ceremony...");
            println!("  VUI commitment: {}...", &vui_commitment[..16]);
            println!("  Pathway hash:   {pathway_hash}");

            // Note: In a full implementation, this would interact with the steward network
            println!("\n⚠️  Enrollment ceremony requires running steward daemon.");
            println!("   This is a placeholder for the full SDIS enrollment flow.");
        }

        StewardCommands::EnrollmentStatus { ceremony_id } => {
            let id_bytes = hex::decode(&ceremony_id)
                .with_context(|| format!("Invalid ceremony ID hex: {ceremony_id}"))?;
            if id_bytes.len() != 32 {
                bail!("Ceremony ID must be 32 bytes");
            }

            println!(
                "Checking enrollment ceremony status: {}...",
                &ceremony_id[..16]
            );
            println!("\n⚠️  Ceremony status check requires running steward daemon.");
        }

        StewardCommands::StartRecovery {
            old_did,
            new_did,
            evidence_hash,
            anchor_commitment,
        } => {
            // Validate DIDs
            if !old_did.starts_with("did:icn:") {
                bail!("Invalid old DID format: {old_did}");
            }
            if !new_did.starts_with("did:icn:") {
                bail!("Invalid new DID format: {new_did}");
            }

            // Validate evidence hash
            let evidence_bytes = hex::decode(&evidence_hash)
                .with_context(|| format!("Invalid evidence hash hex: {evidence_hash}"))?;
            if evidence_bytes.len() != 32 {
                bail!("Evidence hash must be 32 bytes");
            }

            // Validate anchor commitment
            let anchor_bytes = hex::decode(&anchor_commitment)
                .with_context(|| format!("Invalid anchor commitment hex: {anchor_commitment}"))?;
            if anchor_bytes.len() != 32 {
                bail!("Anchor commitment must be 32 bytes");
            }

            println!("Starting recovery ceremony...");
            println!("  Old DID:           {old_did}");
            println!("  New DID:           {new_did}");
            println!("  Evidence hash:     {}...", &evidence_hash[..16]);
            println!("  Anchor commitment: {}...", &anchor_commitment[..16]);

            println!("\n⚠️  Recovery ceremony requires running steward daemon.");
            println!("   This is a placeholder for the full SDIS recovery flow.");
        }

        StewardCommands::RecoveryStatus { ceremony_id } => {
            let id_bytes = hex::decode(&ceremony_id)
                .with_context(|| format!("Invalid ceremony ID hex: {ceremony_id}"))?;
            if id_bytes.len() != 32 {
                bail!("Ceremony ID must be 32 bytes");
            }

            println!(
                "Checking recovery ceremony status: {}...",
                &ceremony_id[..16]
            );
            println!("\n⚠️  Ceremony status check requires running steward daemon.");
        }

        StewardCommands::IssueToken {
            vui_commitment,
            blinded_message,
        } => {
            // Validate commitment
            let commitment_bytes = hex::decode(&vui_commitment)
                .with_context(|| format!("Invalid VUI commitment hex: {vui_commitment}"))?;
            if commitment_bytes.len() != 32 {
                bail!("VUI commitment must be 32 bytes");
            }

            // Validate blinded message (variable length)
            let blinded_bytes = hex::decode(&blinded_message)
                .with_context(|| format!("Invalid blinded message hex: {blinded_message}"))?;
            if blinded_bytes.is_empty() {
                bail!("Blinded message cannot be empty");
            }

            println!("Token issuance request...");
            println!("  VUI commitment:   {}...", &vui_commitment[..16]);
            println!("  Blinded message:  {} bytes", blinded_bytes.len());

            println!(
                "\n⚠️  Token issuance requires running steward daemon with steward privileges."
            );
            println!("   This is a placeholder for the full SDIS token issuance flow.");
        }

        StewardCommands::Topics => {
            println!("Steward Gossip Topics");
            println!("=====================\n");

            println!("Topic: {}", icn_steward::topics::STEWARD_ANNOUNCE);
            println!("  Purpose: Steward announcements and status updates");
            println!("  Access:  Public (any node can subscribe)\n");

            println!("Topic: {}", icn_steward::topics::VUI_SYNC);
            println!("  Purpose: VUI registry synchronization");
            println!("  Access:  Steward nodes only\n");

            println!("Topic: {}", icn_steward::topics::ENROLLMENT);
            println!("  Purpose: Enrollment ceremony coordination");
            println!("  Access:  Steward nodes and enrollees\n");

            println!("Topic: {}", icn_steward::topics::RECOVERY);
            println!("  Purpose: Recovery ceremony coordination");
            println!("  Access:  Steward nodes and recovery requesters");
        }
    }

    Ok(())
}

fn print_default_steward_config() {
    println!("enabled:                   false");
    println!("vui_threshold:             3");
    println!("vui_total_shares:          5");
    println!("max_concurrent_enrollments: 100");
    println!("max_concurrent_recoveries:  50");
    println!("token_validity_secs:       604800 (7 days)");
}

// ========== Commons Commands (Commons Evolution) ==========

async fn handle_commons_command(
    cmd: CommonsCommands,
    data_dir: &Path,
    _endpoint: &str,
) -> Result<()> {
    match cmd {
        CommonsCommands::Status => {
            println!("Commons Holder Status");
            println!("=====================\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                println!("Status: NOT ENROLLED");
                println!("\nYou are not yet a Commons Holder.");
                println!("Run 'icnctl commons enroll' to begin the enrollment process.");
                return Ok(());
            }

            // Get current identity
            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            println!("DID:        {did}");
            println!("Status:     IDENTITY EXISTS");
            println!("\nNote: Full Commons Holder status requires PersonhoodAnchor");
            println!("verification through the SDIS enrollment process.");
            println!("\nTo check your enrollment status:");
            println!("  icnctl steward enrollment-status <ceremony_id>");
        }

        CommonsCommands::Enroll { gateway, coop_id } => {
            println!("Commons Holder Enrollment");
            println!("=========================\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            // Get current identity
            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            println!("Your DID:   {did}");
            println!("Gateway:    {gateway}");
            println!("Coop ID:    {coop_id}");
            println!();
            println!("Enrollment Steps:");
            println!("  1. Visit {gateway}/enroll/{coop_id}");
            println!("  2. Present your DID QR code to a steward");
            println!("  3. Complete proof-of-personhood verification");
            println!("  4. Receive your PersonhoodAnchor attestation");
            println!();
            println!("Note: The enrollment process requires in-person or video");
            println!("verification with a network steward.");
            println!();
            println!("Alternative: Use 'icnctl steward start-enrollment' for direct API access.");
        }

        CommonsCommands::Anchor { did } => {
            println!("PersonhoodAnchor Details");
            println!("========================\n");

            let target_did = if let Some(d) = did {
                d
            } else {
                // Get current identity
                let keystore_path = get_keystore_path(data_dir);
                if !keystore_path.exists() {
                    bail!("No identity found. Run 'icnctl id init' first or provide --did.");
                }

                let passphrase = read_passphrase("Enter passphrase: ")?;
                let mut keystore = AgeKeyStore::open(&keystore_path)?;
                keystore.unlock(&passphrase)?;
                keystore.get_keypair()?.did().to_string()
            };

            println!("DID: {target_did}");
            println!();

            // Try to fetch from gateway (requires default gateway URL)
            let gateway = std::env::var("ICN_GATEWAY")
                .unwrap_or_else(|_| "http://localhost:8080".to_string());
            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/commons/anchor/by-did/{target_did}");

            match client.get(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let data: serde_json::Value = resp.json().await?;
                        println!(
                            "Anchor ID:    {}",
                            data["anchor_id"].as_str().unwrap_or("unknown")
                        );
                        println!(
                            "Status:       {}",
                            data["status"].as_str().unwrap_or("unknown")
                        );
                        println!(
                            "POP Level:    {}",
                            data["pop_level"].as_str().unwrap_or("none")
                        );
                        println!(
                            "Attestations: {}",
                            data["attestation_count"].as_u64().unwrap_or(0)
                        );
                        println!("Created:      {}", data["created_at"].as_u64().unwrap_or(0));
                        println!("Updated:      {}", data["updated_at"].as_u64().unwrap_or(0));
                    } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
                        println!("No PersonhoodAnchor found for this DID.");
                        println!("\nTo create an anchor, complete the enrollment process:");
                        println!("  icnctl steward start-enrollment <identity_name> <coop_id>");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        println!("Error: {status} - {body}");
                    }
                }
                Err(e) => {
                    println!("Could not connect to gateway: {e}");
                    println!("Set ICN_GATEWAY environment variable or ensure gateway is running.");
                }
            }
        }

        CommonsCommands::Affiliations { did } => {
            println!("Commons Holder Affiliations");
            println!("===========================\n");

            let target_did = if let Some(d) = did {
                d
            } else {
                // Get current identity
                let keystore_path = get_keystore_path(data_dir);
                if !keystore_path.exists() {
                    bail!("No identity found. Run 'icnctl id init' first or provide --did.");
                }

                let passphrase = read_passphrase("Enter passphrase: ")?;
                let mut keystore = AgeKeyStore::open(&keystore_path)?;
                keystore.unlock(&passphrase)?;
                keystore.get_keypair()?.did().to_string()
            };

            println!("DID: {target_did}");
            println!();
            println!("Note: Affiliation lookup requires gateway integration.");
            println!("This feature is pending gateway API implementation.");
            println!();
            println!("Expected fields when available:");
            println!("  - Jurisdiction ID");
            println!("  - Membership status");
            println!("  - Role/capabilities");
            println!("  - Join date");
        }

        CommonsCommands::Join {
            jurisdiction,
            gateway,
        } => {
            println!("Join Jurisdiction");
            println!("=================\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let did = keystore.get_keypair()?.did().to_string();

            println!("Your DID:     {did}");
            println!("Jurisdiction: {jurisdiction}");
            println!("Gateway:      {gateway}");
            println!();
            println!("Note: Join request requires gateway integration.");
            println!("This feature is pending gateway API implementation.");
            println!();
            println!("The join process typically requires:");
            println!("  1. Active PersonhoodAnchor");
            println!("  2. Charter signature (for some jurisdictions)");
            println!("  3. Membership fee payment (if applicable)");
        }

        CommonsCommands::Leave {
            jurisdiction,
            gateway,
        } => {
            println!("Leave Jurisdiction");
            println!("==================\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let did = keystore.get_keypair()?.did().to_string();

            println!("Your DID:     {did}");
            println!("Jurisdiction: {jurisdiction}");
            println!("Gateway:      {gateway}");
            println!();
            println!("Note: Leave request requires gateway integration.");
            println!("This feature is pending gateway API implementation.");
            println!();
            println!("Important: Leaving a jurisdiction:");
            println!("  - Does NOT revoke your PersonhoodAnchor");
            println!("  - May affect pending transactions");
            println!("  - May require notice period");
        }
    }

    Ok(())
}

// ========== Charter Commands (Organizational Founding Documents) ==========

async fn handle_charter_command(
    cmd: CharterCommands,
    data_dir: &Path,
    _endpoint: &str,
) -> Result<()> {
    match cmd {
        CharterCommands::Create {
            name,
            org_type,
            domain,
            mission,
            founders,
            output,
        } => {
            use icn_governance::{Charter, GovernanceConfig, OrgType};

            println!("Create Organizational Charter");
            println!("=============================\n");

            // Parse organization type
            let org = match org_type.to_lowercase().as_str() {
                "cooperative" | "coop" => OrgType::Cooperative,
                "community" => OrgType::Community,
                "federation" => OrgType::Federation,
                "network" => OrgType::Federation, // Networks use federation structure
                other => bail!("Unknown organization type: {other}. Use: cooperative, community, federation, or network"),
            };

            // Create governance config based on org type
            let config = GovernanceConfig::cooperative_default();

            // Create the charter based on org type
            let mut charter = match org {
                OrgType::Cooperative => Charter::cooperative(
                    domain.clone(),
                    name.clone(),
                    config,
                    "credits".to_string(), // Default currency
                ),
                OrgType::Community => Charter::community(domain.clone(), name.clone(), config),
                OrgType::Federation => Charter::federation(
                    domain.clone(),
                    name.clone(),
                    config,
                    Vec::new(), // Empty initial member jurisdictions
                ),
            };

            // Set description if provided
            if let Some(m) = mission {
                charter.description = Some(m);
            }

            // Add founders if provided
            if let Some(founder_list) = founders {
                let founder_dids: Vec<&str> = founder_list.split(',').map(|s| s.trim()).collect();
                println!("Founders: {}", founder_dids.len());
                for f in &founder_dids {
                    println!("  - {f}");
                }
            }

            println!();
            println!("Charter Created:");
            println!("  ID:       {}", charter.charter_id);
            println!("  Name:     {}", charter.name);
            println!("  Type:     {}", charter.org_type);
            println!("  Domain:   {}", charter.domain_id);
            println!("  Status:   {}", charter.status);
            println!("  Created:  {}", charter.created_at);

            // Output to file if requested
            if let Some(path) = output {
                let json = serde_json::to_string_pretty(&charter)?;
                std::fs::write(&path, &json)?;
                println!();
                println!("Charter saved to: {}", path.display());
            } else {
                println!();
                println!("Charter JSON:");
                println!("{}", serde_json::to_string_pretty(&charter)?);
            }

            println!();
            println!("Next steps:");
            println!("  1. Share the charter with founders");
            println!("  2. Collect founder signatures: icnctl charter sign <charter_id>");
            println!("  3. Ratify the charter: icnctl charter ratify <charter_id>");
        }

        CharterCommands::Show {
            charter_id,
            gateway,
        } => {
            println!("Charter Details");
            println!("===============\n");

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/charter/{charter_id}");

            match client.get(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let data: serde_json::Value = resp.json().await?;
                        println!(
                            "Charter ID:   {}",
                            data["charter_id"].as_str().unwrap_or("unknown")
                        );
                        println!(
                            "Name:         {}",
                            data["name"].as_str().unwrap_or("unknown")
                        );
                        println!(
                            "Domain:       {}",
                            data["domain_id"].as_str().unwrap_or("unknown")
                        );
                        println!(
                            "Type:         {}",
                            data["org_type"].as_str().unwrap_or("unknown")
                        );
                        println!(
                            "Status:       {}",
                            data["status"].as_str().unwrap_or("unknown")
                        );
                        if let Some(desc) = data["description"].as_str() {
                            println!("Description:  {desc}");
                        }
                        println!("Created:      {}", data["created_at"].as_u64().unwrap_or(0));

                        if let Some(founders) = data["founders"].as_array() {
                            println!("\nFounders ({}):", founders.len());
                            for f in founders {
                                let did = f["did"].as_str().unwrap_or("unknown");
                                let role = f["role"].as_str().unwrap_or("founder");
                                println!("  - {did} ({role})");
                            }
                        }
                    } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
                        println!("Charter not found: {charter_id}");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        println!("Error: {status} - {body}");
                    }
                }
                Err(e) => {
                    println!("Could not connect to gateway: {e}");
                }
            }
        }

        CharterCommands::List {
            org_type,
            status,
            gateway,
        } => {
            println!("List Charters");
            println!("=============\n");

            let client = reqwest::Client::new();
            let mut url = format!("{gateway}/v1/charter");
            let mut params = Vec::new();
            if let Some(ref t) = org_type {
                params.push(format!("org_type={t}"));
            }
            if let Some(ref s) = status {
                params.push(format!("status={s}"));
            }
            if !params.is_empty() {
                url = format!("{}?{}", url, params.join("&"));
            }

            match client.get(&url).send().await {
                Ok(resp) => {
                    let resp_status = resp.status();
                    if resp_status.is_success() {
                        let charters: Vec<serde_json::Value> = resp.json().await?;
                        if charters.is_empty() {
                            println!("No charters found.");
                        } else {
                            println!(
                                "{:<12} {:<20} {:<15} {:<10}",
                                "TYPE", "NAME", "DOMAIN", "STATUS"
                            );
                            println!("{}", "-".repeat(60));
                            for c in charters {
                                println!(
                                    "{:<12} {:<20} {:<15} {:<10}",
                                    c["org_type"].as_str().unwrap_or("-"),
                                    c["name"].as_str().unwrap_or("-"),
                                    c["domain_id"].as_str().unwrap_or("-"),
                                    c["status"].as_str().unwrap_or("-"),
                                );
                            }
                        }
                    } else {
                        let body = resp.text().await.unwrap_or_default();
                        println!("Error: {resp_status} - {body}");
                    }
                }
                Err(e) => {
                    println!("Could not connect to gateway: {e}");
                }
            }
        }

        CharterCommands::Sign {
            charter_id,
            coop_id,
            gateway,
            role,
        } => {
            println!("Sign Charter");
            println!("============\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            println!("Your DID:    {did}");
            println!("Charter ID:  {charter_id}");
            println!("Role:        {role}");
            println!("Gateway:     {gateway}");
            println!();

            // Create signature over charter ID
            let message = format!("charter-sign:{charter_id}:{did}");
            let signature = keypair.sign(message.as_bytes());
            let signature_hex = hex::encode(signature.to_bytes());

            // Get auth token
            let token = get_gateway_token(&gateway, &did.to_string(), &coop_id, &keypair).await?;

            // Build request
            let request = serde_json::json!({
                "signature": signature_hex,
                "role": role,
            });

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/charter/{charter_id}/sign");

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .json(&request)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        println!("Charter signed successfully!");
                        println!();
                        if let Some(total) = body.get("total_founders") {
                            println!("Total founders: {total}");
                        }
                        if let Some(ready) = body.get("ready_for_activation") {
                            if ready.as_bool().unwrap_or(false) {
                                println!("Charter is ready for activation!");
                                println!(
                                    "Run: icnctl charter ratify {charter_id} --coop-id {coop_id}"
                                );
                            } else if let Some(needed) = body.get("founders_needed") {
                                println!("Founders needed for activation: {needed}");
                            }
                        }
                    } else {
                        let body = resp.text().await.unwrap_or_default();
                        print_http_error("signing charter", status, &body);
                    }
                }
                Err(e) => {
                    print_gateway_error(&gateway, &e);
                }
            }
        }

        CharterCommands::Ratify {
            charter_id,
            coop_id,
            gateway,
        } => {
            println!("Activate Charter");
            println!("================\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            println!("Your DID:    {did}");
            println!("Charter ID:  {charter_id}");
            println!("Gateway:     {gateway}");
            println!();

            // Get auth token
            let token = get_gateway_token(&gateway, &did.to_string(), &coop_id, &keypair).await?;

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/charter/{charter_id}/activate");

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        println!("Charter activated successfully!");
                        println!();
                        if let Some(s) = body.get("status") {
                            println!("Status: {s}");
                        }
                    } else {
                        let body = resp.text().await.unwrap_or_default();
                        print_http_error("activating charter", status, &body);
                    }
                }
                Err(e) => {
                    print_gateway_error(&gateway, &e);
                }
            }
        }
    }

    Ok(())
}

async fn handle_amendment_command(
    cmd: AmendmentCommands,
    data_dir: &Path,
    _endpoint: &str,
) -> Result<()> {
    match cmd {
        AmendmentCommands::Propose {
            title,
            description,
            amendment_type,
            scope,
            scope_id,
            charter_id,
            coop_id,
            gateway,
        } => {
            println!("Propose Amendment");
            println!("=================\n");

            // Get identity for signing
            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            // Build request
            let request = serde_json::json!({
                "amendment_type": amendment_type,
                "scope_type": scope,
                "scope_id": scope_id,
                "title": title,
                "description": description,
                "charter_id": charter_id,
                "changes": []  // Changes added separately via add-change
            });

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/constitutional/amendments");

            // Get auth token
            let token = get_gateway_token(&gateway, &did.to_string(), &coop_id, &keypair).await?;

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .json(&request)
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let data: serde_json::Value = resp.json().await?;
                        let id = data["id"].as_str().unwrap_or("unknown");
                        println!("Amendment created successfully!");
                        println!();
                        println!("  ID:          {id}");
                        println!("  Title:       {title}");
                        println!("  Type:        {amendment_type}");
                        println!("  Scope:       {scope}");
                        println!("  Status:      Draft");
                        println!();
                        println!("Next steps:");
                        println!("  1. Add changes:  icnctl amendment add-change {id} ...");
                        println!("  2. Submit:       icnctl amendment submit {id}");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        print_http_error("creating amendment", status, &body);
                    }
                }
                Err(e) => {
                    print_gateway_error(&gateway, &e);
                }
            }
        }

        AmendmentCommands::List {
            status,
            scope,
            amendment_type,
            gateway,
        } => {
            println!("Amendments");
            println!("==========\n");

            let client = reqwest::Client::new();
            let mut url = format!("{gateway}/v1/constitutional/amendments");
            let mut params = Vec::new();
            if let Some(ref s) = status {
                params.push(format!("status={s}"));
            }
            if let Some(ref s) = scope {
                params.push(format!("scope={s}"));
            }
            if let Some(ref t) = amendment_type {
                params.push(format!("amendment_type={t}"));
            }
            if !params.is_empty() {
                url = format!("{}?{}", url, params.join("&"));
            }

            match client.get(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let amendments: Vec<serde_json::Value> = resp.json().await?;
                        if amendments.is_empty() {
                            println!("No amendments found.");
                        } else {
                            println!(
                                "{:<12} {:<30} {:<12} {:<10}",
                                "ID", "TITLE", "TYPE", "STATUS"
                            );
                            println!("{}", "-".repeat(70));
                            for a in amendments {
                                let id = a["id"].as_str().unwrap_or("-");
                                let short_id = if id.len() > 10 { &id[..10] } else { id };
                                println!(
                                    "{:<12} {:<30} {:<12} {:<10}",
                                    short_id,
                                    truncate_str(a["title"].as_str().unwrap_or("-"), 28),
                                    a["amendment_type"].as_str().unwrap_or("-"),
                                    a["status"].as_str().unwrap_or("-"),
                                );
                            }
                        }
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        print_http_error("listing amendments", status, &body);
                    }
                }
                Err(e) => {
                    print_gateway_error(&gateway, &e);
                }
            }
        }

        AmendmentCommands::Show {
            amendment_id,
            gateway,
        } => {
            println!("Amendment Details");
            println!("=================\n");

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/constitutional/amendments/{amendment_id}");

            match client.get(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let a: serde_json::Value = resp.json().await?;
                        println!("ID:           {}", a["id"].as_str().unwrap_or("-"));
                        println!("Title:        {}", a["title"].as_str().unwrap_or("-"));
                        println!(
                            "Type:         {}",
                            a["amendment_type"].as_str().unwrap_or("-")
                        );
                        println!("Scope:        {}", a["scope"].as_str().unwrap_or("-"));
                        println!("Status:       {}", a["status"].as_str().unwrap_or("-"));
                        println!("Proposer:     {}", a["proposer"].as_str().unwrap_or("-"));
                        println!("Description:  {}", a["description"].as_str().unwrap_or("-"));

                        if let Some(changes) = a["changes"].as_array() {
                            println!("\nChanges ({}):", changes.len());
                            for (i, c) in changes.iter().enumerate() {
                                println!(
                                    "  {}. [{}] {} - {}",
                                    i + 1,
                                    c["change_type"].as_str().unwrap_or("-"),
                                    c["target"].as_str().unwrap_or("-"),
                                    c["description"].as_str().unwrap_or("-")
                                );
                            }
                        }

                        let ratifications = a["ratifications_count"].as_u64().unwrap_or(0);
                        let approvals = a["approvals_count"].as_u64().unwrap_or(0);
                        println!("\nRatifications: {ratifications} ({approvals} approved)");
                    } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
                        println!("Amendment not found: {amendment_id}");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        print_http_error("fetching amendment", status, &body);
                    }
                }
                Err(e) => {
                    print_gateway_error(&gateway, &e);
                }
            }
        }

        AmendmentCommands::Submit {
            amendment_id,
            coop_id,
            gateway,
        } => {
            println!("Submit Amendment for Review");
            println!("===========================\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/constitutional/amendments/{amendment_id}/submit");
            let token = get_gateway_token(&gateway, &did.to_string(), &coop_id, &keypair).await?;

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        println!("Amendment submitted for review!");
                        println!("Amendment ID: {amendment_id}");
                        println!();
                        println!("The amendment is now under review.");
                        println!("After the review period, use 'icnctl amendment open-voting {amendment_id}'");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        print_http_error("submitting amendment", status, &body);
                    }
                }
                Err(e) => {
                    print_gateway_error(&gateway, &e);
                }
            }
        }

        AmendmentCommands::OpenVoting {
            amendment_id,
            coop_id,
            gateway,
        } => {
            println!("Open Voting on Amendment");
            println!("========================\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/constitutional/amendments/{amendment_id}/vote");
            let token = get_gateway_token(&gateway, &did.to_string(), &coop_id, &keypair).await?;

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        println!("Voting is now open!");
                        println!("Amendment ID: {amendment_id}");
                        println!();
                        println!("Members can now vote using:");
                        println!("  icnctl amendment vote {amendment_id} --approve");
                        println!("  icnctl amendment vote {amendment_id} --reject");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        print_http_error("opening voting", status, &body);
                    }
                }
                Err(e) => {
                    print_gateway_error(&gateway, &e);
                }
            }
        }

        AmendmentCommands::Vote {
            amendment_id,
            approve,
            reject,
            comment,
            coop_id,
            gateway,
        } => {
            println!("Vote on Amendment");
            println!("=================\n");

            let approved = if approve {
                true
            } else if reject {
                false
            } else {
                bail!("Must specify --approve or --reject");
            };

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            // Sign the vote
            let vote_data = format!("{amendment_id}:{approved}:{did}");
            let signature = keypair.sign(vote_data.as_bytes());

            let request = serde_json::json!({
                "ratifier_id": did.to_string(),
                "ratifier_type": "member",
                "approved": approved,
                "comment": comment,
                "signature": hex::encode(signature.to_bytes())
            });

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/constitutional/amendments/{amendment_id}/ratify");
            let token = get_gateway_token(&gateway, &did.to_string(), &coop_id, &keypair).await?;

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .json(&request)
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        println!("Vote recorded!");
                        println!();
                        println!("  Amendment:  {amendment_id}");
                        println!(
                            "  Your vote:  {}",
                            if approved { "APPROVE" } else { "REJECT" }
                        );
                        if let Some(c) = comment {
                            println!("  Comment:    {c}");
                        }
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        print_http_error("recording vote", status, &body);
                    }
                }
                Err(e) => {
                    print_gateway_error(&gateway, &e);
                }
            }
        }

        AmendmentCommands::Withdraw {
            amendment_id,
            reason,
            coop_id,
            gateway,
        } => {
            println!("Withdraw Amendment");
            println!("==================\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            let request = serde_json::json!({
                "reason": reason.unwrap_or_else(|| "Withdrawn by proposer".to_string())
            });

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/constitutional/amendments/{amendment_id}/withdraw");
            let token = get_gateway_token(&gateway, &did.to_string(), &coop_id, &keypair).await?;

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .json(&request)
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        println!("Amendment withdrawn.");
                        println!("Amendment ID: {amendment_id}");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        print_http_error("withdrawing amendment", status, &body);
                    }
                }
                Err(e) => {
                    print_gateway_error(&gateway, &e);
                }
            }
        }

        AmendmentCommands::AddChange {
            amendment_id,
            target,
            change_type,
            description,
            new_value,
            old_value,
            coop_id,
            gateway,
        } => {
            println!("Add Change to Amendment");
            println!("=======================\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            println!("Amendment ID:  {amendment_id}");
            println!("Target:        {target}");
            println!("Change Type:   {change_type}");
            println!("Description:   {description}");
            println!("New Value:     {new_value}");
            if let Some(ref ov) = old_value {
                println!("Old Value:     {ov}");
            }
            println!();

            let request = serde_json::json!({
                "target": target,
                "change_type": change_type,
                "description": description,
                "new_value": new_value,
                "old_value": old_value,
            });

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/constitutional/amendments/{amendment_id}/changes");
            let token = get_gateway_token(&gateway, &did.to_string(), &coop_id, &keypair).await?;

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .json(&request)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        println!("Change added successfully!");
                        println!();
                        if let Some(changes) = body.get("changes") {
                            if let Some(arr) = changes.as_array() {
                                println!("Total changes: {}", arr.len());
                            }
                        }
                    } else {
                        let body = resp.text().await.unwrap_or_default();
                        print_http_error("adding change", status, &body);
                    }
                }
                Err(e) => {
                    print_gateway_error(&gateway, &e);
                }
            }
        }
    }

    Ok(())
}

async fn handle_appeal_command(
    cmd: AppealCommands,
    data_dir: &Path,
    _endpoint: &str,
) -> Result<()> {
    match cmd {
        AppealCommands::File {
            appeal_type,
            target_id,
            scope,
            scope_id,
            statement,
            grounds,
            remedy,
            coop_id,
            gateway,
        } => {
            println!("File Appeal");
            println!("===========\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            // Parse grounds
            let grounds_list: Vec<serde_json::Value> = grounds
                .split(',')
                .map(|g| {
                    serde_json::json!({
                        "ground_type": g.trim(),
                        "description": format!("Appeal on {} grounds", g.trim())
                    })
                })
                .collect();

            // Build appeal type request based on type
            let appeal_type_req = match appeal_type.to_lowercase().as_str() {
                "revocation" => serde_json::json!({
                    "category": "revocation",
                    "revocation_id": target_id
                }),
                "suspension" => serde_json::json!({
                    "category": "suspension",
                    "target_id": target_id
                }),
                "governance" => serde_json::json!({
                    "category": "governance_decision",
                    "proposal_id": target_id
                }),
                "dispute" => serde_json::json!({
                    "category": "dispute_resolution",
                    "dispute_id": target_id
                }),
                "membership" => serde_json::json!({
                    "category": "membership_denial",
                    "details": target_id
                }),
                "steward" => serde_json::json!({
                    "category": "steward_action",
                    "steward_did": target_id
                }),
                _ => bail!("Unknown appeal type: {appeal_type}. Use: revocation, suspension, governance, dispute, membership, steward"),
            };

            let request = serde_json::json!({
                "appeal_type": appeal_type_req,
                "scope_type": scope,
                "scope_id": scope_id,
                "grounds": grounds_list,
                "statement": statement,
                "requested_remedy": remedy
            });

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/constitutional/appeals");
            let token = get_gateway_token(&gateway, &did.to_string(), &coop_id, &keypair).await?;

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .json(&request)
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let data: serde_json::Value = resp.json().await?;
                        let id = data["id"].as_str().unwrap_or("unknown");
                        println!("Appeal filed successfully!");
                        println!();
                        println!("  Appeal ID:  {id}");
                        println!("  Type:       {appeal_type}");
                        println!("  Target:     {target_id}");
                        println!("  Status:     Filed");
                        println!();
                        println!("Next steps:");
                        println!("  - Add evidence: icnctl appeal add-evidence {id} ...");
                        println!("  - Check status: icnctl appeal show {id}");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        print_http_error("filing appeal", status, &body);
                    }
                }
                Err(e) => {
                    print_gateway_error(&gateway, &e);
                }
            }
        }

        AppealCommands::List {
            status,
            appeal_type,
            appellant,
            gateway,
        } => {
            println!("Appeals");
            println!("=======\n");

            let client = reqwest::Client::new();
            let mut url = format!("{gateway}/v1/constitutional/appeals");
            let mut params = Vec::new();
            if let Some(ref s) = status {
                params.push(format!("status={s}"));
            }
            if let Some(ref t) = appeal_type {
                params.push(format!("appeal_type={t}"));
            }
            if let Some(ref a) = appellant {
                params.push(format!("appellant={a}"));
            }
            if !params.is_empty() {
                url = format!("{}?{}", url, params.join("&"));
            }

            match client.get(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let appeals: Vec<serde_json::Value> = resp.json().await?;
                        if appeals.is_empty() {
                            println!("No appeals found.");
                        } else {
                            println!(
                                "{:<12} {:<15} {:<12} {:<10}",
                                "ID", "TYPE", "STATUS", "APPELLANT"
                            );
                            println!("{}", "-".repeat(55));
                            for a in appeals {
                                let id = a["id"].as_str().unwrap_or("-");
                                let short_id = if id.len() > 10 { &id[..10] } else { id };
                                let appellant_did = a["appellant"].as_str().unwrap_or("-");
                                let short_appellant = if appellant_did.len() > 10 {
                                    &appellant_did[..10]
                                } else {
                                    appellant_did
                                };
                                println!(
                                    "{:<12} {:<15} {:<12} {:<10}",
                                    short_id,
                                    a["appeal_type"].as_str().unwrap_or("-"),
                                    a["status"].as_str().unwrap_or("-"),
                                    short_appellant,
                                );
                            }
                        }
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        print_http_error("listing appeals", status, &body);
                    }
                }
                Err(e) => {
                    print_gateway_error(&gateway, &e);
                }
            }
        }

        AppealCommands::Show { appeal_id, gateway } => {
            println!("Appeal Details");
            println!("==============\n");

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/constitutional/appeals/{appeal_id}");

            match client.get(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let a: serde_json::Value = resp.json().await?;
                        println!("ID:          {}", a["id"].as_str().unwrap_or("-"));
                        println!("Type:        {}", a["appeal_type"].as_str().unwrap_or("-"));
                        println!("Scope:       {}", a["scope"].as_str().unwrap_or("-"));
                        println!("Status:      {}", a["status"].as_str().unwrap_or("-"));
                        println!("Appellant:   {}", a["appellant"].as_str().unwrap_or("-"));
                        if let Some(respondent) = a["respondent"].as_str() {
                            println!("Respondent:  {respondent}");
                        }
                        println!("Statement:   {}", a["statement"].as_str().unwrap_or("-"));
                        println!(
                            "Remedy:      {}",
                            a["requested_remedy"].as_str().unwrap_or("-")
                        );

                        if let Some(grounds) = a["grounds"].as_array() {
                            println!("\nGrounds ({}):", grounds.len());
                            for (i, g) in grounds.iter().enumerate() {
                                println!(
                                    "  {}. {} - {}",
                                    i + 1,
                                    g["ground_type"].as_str().unwrap_or("-"),
                                    g["description"].as_str().unwrap_or("-")
                                );
                            }
                        }

                        if let Some(evidence) = a["evidence"].as_array() {
                            if !evidence.is_empty() {
                                println!("\nEvidence ({}):", evidence.len());
                                for (i, e) in evidence.iter().enumerate() {
                                    println!(
                                        "  {}. [{}] {}",
                                        i + 1,
                                        e["evidence_type"].as_str().unwrap_or("-"),
                                        e["description"].as_str().unwrap_or("-")
                                    );
                                }
                            }
                        }

                        if let Some(responses) = a["responses"].as_array() {
                            if !responses.is_empty() {
                                println!("\nResponses ({}):", responses.len());
                                for (i, r) in responses.iter().enumerate() {
                                    println!(
                                        "  {}. [{}] {}",
                                        i + 1,
                                        r["response_type"].as_str().unwrap_or("-"),
                                        truncate_str(r["content"].as_str().unwrap_or("-"), 50)
                                    );
                                }
                            }
                        }

                        if let Some(outcome) = a["outcome"].as_object() {
                            println!("\nOutcome:");
                            println!(
                                "  Result: {}",
                                outcome
                                    .get("result")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                            );
                            if let Some(reason) = outcome.get("reason").and_then(|v| v.as_str()) {
                                println!("  Reason: {reason}");
                            }
                        }
                    } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
                        println!("Appeal not found: {appeal_id}");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        println!("Error: {status} - {body}");
                    }
                }
                Err(e) => {
                    println!("Could not connect to gateway: {e}");
                }
            }
        }

        AppealCommands::AddEvidence {
            appeal_id,
            evidence_type,
            description,
            content_hash,
            uri,
            coop_id,
            gateway,
        } => {
            println!("Add Evidence to Appeal");
            println!("======================\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            let request = serde_json::json!({
                "evidence_type": evidence_type,
                "description": description,
                "content_hash": content_hash,
                "uri": uri
            });

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/constitutional/appeals/{appeal_id}/evidence");
            let token = get_gateway_token(&gateway, &did.to_string(), &coop_id, &keypair).await?;

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .json(&request)
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        println!("Evidence added successfully!");
                        println!();
                        println!("  Appeal ID:  {appeal_id}");
                        println!("  Type:       {evidence_type}");
                        println!("  Description: {description}");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        println!("Error adding evidence: {status}");
                        println!("{body}");
                    }
                }
                Err(e) => {
                    println!("Could not connect to gateway: {e}");
                }
            }
        }

        AppealCommands::Respond {
            appeal_id,
            response_type,
            content,
            coop_id,
            gateway,
        } => {
            println!("Respond to Appeal");
            println!("=================\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            let request = serde_json::json!({
                "response_type": response_type,
                "content": content
            });

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/constitutional/appeals/{appeal_id}/respond");
            let token = get_gateway_token(&gateway, &did.to_string(), &coop_id, &keypair).await?;

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .json(&request)
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        println!("Response submitted!");
                        println!();
                        println!("  Appeal ID:     {appeal_id}");
                        println!("  Response type: {response_type}");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        println!("Error submitting response: {status}");
                        println!("{body}");
                    }
                }
                Err(e) => {
                    println!("Could not connect to gateway: {e}");
                }
            }
        }

        AppealCommands::Withdraw {
            appeal_id,
            reason,
            coop_id,
            gateway,
        } => {
            println!("Withdraw Appeal");
            println!("===============\n");

            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let did = keypair.did();

            let request = serde_json::json!({
                "reason": reason.unwrap_or_else(|| "Withdrawn by appellant".to_string())
            });

            let client = reqwest::Client::new();
            let url = format!("{gateway}/v1/constitutional/appeals/{appeal_id}/withdraw");
            let token = get_gateway_token(&gateway, &did.to_string(), &coop_id, &keypair).await?;

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .json(&request)
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        println!("Appeal withdrawn.");
                        println!("Appeal ID: {appeal_id}");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        println!("Error withdrawing appeal: {status}");
                        println!("{body}");
                    }
                }
                Err(e) => {
                    println!("Could not connect to gateway: {e}");
                }
            }
        }
    }

    Ok(())
}

/// Truncate a string to max length, adding "..." if truncated
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Get a gateway auth token using challenge-response
async fn get_gateway_token(
    gateway: &str,
    did: &str,
    coop_id: &str,
    keypair: &icn_identity::KeyPair,
) -> Result<String> {
    let client = reqwest::Client::new();

    // Request challenge
    let challenge_url = format!("{gateway}/v1/auth/challenge");
    let challenge_req = serde_json::json!({ "did": did });

    let resp = client
        .post(&challenge_url)
        .json(&challenge_req)
        .send()
        .await
        .context("Failed to request auth challenge")?;

    if !resp.status().is_success() {
        bail!("Failed to get auth challenge: {}", resp.status());
    }

    let challenge_data: serde_json::Value = resp.json().await?;
    let nonce = challenge_data["nonce"]
        .as_str()
        .context("Missing nonce in response")?;

    // Sign the nonce
    let signature = keypair.sign(nonce.as_bytes());

    // Submit signed challenge to verify endpoint
    let verify_url = format!("{gateway}/v1/auth/verify");
    let verify_req = serde_json::json!({
        "did": did,
        "signature": hex::encode(signature.to_bytes()),
        "coop_id": coop_id,
        "scopes": ["gov:read", "gov:write"]
    });

    let resp = client
        .post(&verify_url)
        .json(&verify_req)
        .send()
        .await
        .context("Failed to verify signature")?;

    if !resp.status().is_success() {
        bail!("Failed to get auth token: {}", resp.status());
    }

    let token_data: serde_json::Value = resp.json().await?;
    let token = token_data["token"]
        .as_str()
        .context("Missing token in response")?;

    Ok(token.to_string())
}

/// Print a gateway connection error with actionable hints
fn print_gateway_error(gateway: &str, error: &impl std::fmt::Display) {
    println!("Error: Could not connect to gateway at {gateway}");
    println!();
    println!("  Cause: {error}");
    println!();
    println!("Troubleshooting:");
    println!("  • Check that the gateway server is running");
    println!("  • Verify the gateway URL is correct: {gateway}");
    println!("  • If using a non-default port, specify with --gateway");
}

/// Print an HTTP error response with context
fn print_http_error(action: &str, status: reqwest::StatusCode, body: &str) {
    println!("Error {action}: HTTP {status}");
    if !body.is_empty() {
        // Try to extract a message from JSON response
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(msg) = json.get("message").or_else(|| json.get("error")) {
                if let Some(s) = msg.as_str() {
                    println!("  Message: {s}");
                    return;
                }
            }
        }
        // Truncate long error bodies
        if body.len() > 200 {
            println!("  Details: {}...", &body[..200]);
        } else {
            println!("  Details: {body}");
        }
    }
}

/// Generate a QR code representation (ASCII art placeholder)
#[allow(dead_code)]
fn print_qr_placeholder(data: &str) {
    println!("┌─────────────────────┐");
    println!("│  [QR CODE WOULD BE  │");
    println!("│   DISPLAYED HERE]   │");
    println!("│                     │");
    println!("│  Data: {data:.12}...│");
    println!("└─────────────────────┘");
}

/// Handle API commands
fn handle_api_command(cmd: ApiCommands) -> Result<()> {
    use utoipa::OpenApi;

    match cmd {
        ApiCommands::ExportOpenapi { output, format } => {
            // Get the OpenAPI document
            let doc = icn_gateway::openapi::ApiDoc::openapi();

            // Serialize to the requested format
            let content = match format.to_lowercase().as_str() {
                "json" => doc
                    .to_json()
                    .context("Failed to serialize OpenAPI spec to JSON")?,
                _ => doc
                    .to_yaml()
                    .context("Failed to serialize OpenAPI spec to YAML")?,
            };

            // Write to file or stdout
            if let Some(path) = output {
                std::fs::write(&path, &content)
                    .with_context(|| format!("Failed to write to {}", path.display()))?;
                println!("OpenAPI specification written to {}", path.display());
            } else {
                print!("{content}");
            }
        }
    }

    Ok(())
}
