/// Resource token burn commands
#[derive(Debug, Subcommand)]
pub enum ResourceBurnCommand {
    /// Consume a resource token and record the burn
    Consume {
        /// Token ID to consume
        #[clap(long)]
        token_id: String,
        
        /// Amount to consume
        #[clap(long)]
        amount: f64,
        
        /// Owner DID
        #[clap(long)]
        owner: String,
        
        /// Optional job ID if token is being burned for a job
        #[clap(long)]
        job_id: Option<String>,
        
        /// Optional receipt ID linking to execution receipt
        #[clap(long)]
        receipt_id: Option<String>,
        
        /// Optional reason for the burn
        #[clap(long)]
        reason: Option<String>,
    },
    
    /// List token burn records
    List {
        /// Filter by owner DID
        #[clap(long)]
        owner: Option<String>,
        
        /// Filter by token type (e.g., icn:resource/compute)
        #[clap(long)]
        token_type: Option<String>,
        
        /// Filter by federation scope
        #[clap(long)]
        scope: Option<String>,
        
        /// Filter by job ID
        #[clap(long)]
        job_id: Option<String>,
        
        /// Filter by receipt ID
        #[clap(long)]
        receipt_id: Option<String>,
        
        /// Limit the number of results
        #[clap(long, default_value = "20")]
        limit: usize,
    },
}

/// Resource token commands
#[derive(Debug, Subcommand)]
pub enum ResourceCommand {
    /// Create a new resource token
    #[clap(alias = "create")]
    Mint {
        /// Token type (e.g., compute, storage, network)
        #[clap(long)]
        token_type: String,
        
        /// Token amount
        #[clap(long)]
        amount: f64,
        
        /// Owner DID
        #[clap(long)]
        owner: String,
        
        /// Federation scope
        #[clap(long)]
        scope: String,
        
        /// Optional expiration in seconds from now
        #[clap(long)]
        expiration_seconds: Option<u64>,
    },
    
    /// Check resource token balance
    Balance {
        /// Token ID
        #[clap(long)]
        token_id: String,
        
        /// Owner DID
        #[clap(long)]
        owner: String,
    },
    
    /// List resource tokens
    List {
        /// Filter by owner DID
        #[clap(long)]
        owner: Option<String>,
        
        /// Filter by token type
        #[clap(long)]
        token_type: Option<String>,
        
        /// Filter by federation scope
        #[clap(long)]
        scope: Option<String>,
        
        /// Include expired tokens
        #[clap(long)]
        include_expired: bool,
    },
    
    /// Burn resource tokens (consume tokens)
    #[clap(subcommand)]
    Burn(ResourceBurnCommand),
} 