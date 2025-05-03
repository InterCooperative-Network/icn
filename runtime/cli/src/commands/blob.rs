use clap::{Args, Subcommand};
use crate::error::CliResult;
use std::path::PathBuf;
use icn_storage::{AsyncInMemoryStorage, AsyncFsStorage, AsyncStorageBackend};
use cid::Cid;
use reqwest::StatusCode;
use serde::{Serialize, Deserialize};
use comfy_table::{Table, Row, Cell, ContentArrangement, Width};
use comfy_table::presets::UTF8_FULL;
use std::time::{Duration, SystemTime};
use chrono::{DateTime, Utc};

/// Blob commands for working with content-addressed storage
#[derive(Args, Debug)]
pub struct BlobCommand {
    #[command(subcommand)]
    command: BlobCommands,
}

/// Blob subcommands
#[derive(Subcommand, Debug)]
pub enum BlobCommands {
    /// Upload a file to the blob store
    Upload {
        /// Path to file to upload
        file: PathBuf,
        
        /// Pin the file after upload (store permanently)
        #[arg(long)]
        pin: bool,
    },
    
    /// Download a file from the blob store
    Download {
        /// CID of the file to download
        cid: String,
        
        /// Path to save the downloaded file
        #[arg(long)]
        output: Option<PathBuf>,
    },
    
    /// Pin a blob in the store
    Pin {
        /// CID of the blob to pin
        cid: String,
    },
    
    /// Unpin a blob from the store
    Unpin {
        /// CID of the blob to unpin
        cid: String,
    },
    
    /// Show blob status and replication information
    Status {
        /// CID of the blob to check
        cid: String,
        
        /// Display verbose information
        #[arg(short, long)]
        verbose: bool,
        
        /// Use JSON output format
        #[arg(long)]
        json: bool,
    },
}

/// Response format for blob status
#[derive(Debug, Serialize, Deserialize)]
struct BlobStatusResponse {
    /// Blob CID
    cid: String,
    
    /// Whether the blob exists on this node
    exists: bool,
    
    /// Size of the blob in bytes
    size: Option<usize>,
    
    /// Whether the blob is pinned
    pinned: bool,
    
    /// Replication information
    replication: ReplicationInfo,
    
    /// Health issues (if any)
    health_issues: Vec<BlobHealthIssue>,
    
    /// Creation time (if known)
    created_at: Option<DateTime<Utc>>,
    
    /// Last access time (if known)
    last_accessed: Option<DateTime<Utc>>,
}

/// Replication information
#[derive(Debug, Serialize, Deserialize)]
struct ReplicationInfo {
    /// Target replication factor
    target_factor: u32,
    
    /// Current replication factor
    current_factor: u32,
    
    /// Replication completion percentage
    completion_percentage: u8,
    
    /// Nodes hosting this blob
    hosting_nodes: Vec<HostingNode>,
    
    /// Replication policy applied
    policy: String,
}

/// Node hosting the blob
#[derive(Debug, Serialize, Deserialize)]
struct HostingNode {
    /// Node ID
    id: String,
    
    /// Node address
    address: String,
    
    /// Node status
    status: String,
    
    /// Health status
    healthy: bool,
}

/// Health issue with a specific blob
#[derive(Debug, Serialize, Deserialize)]
struct BlobHealthIssue {
    /// Issue type
    issue_type: String,
    
    /// Detailed description
    description: String,
    
    /// Timestamp when the issue was detected
    detected_at: DateTime<Utc>,
}

impl BlobCommand {
    pub async fn run(&self, runtime_url: &str) -> CliResult<()> {
        match &self.command {
            BlobCommands::Upload { file, pin } => {
                self.upload_blob(runtime_url, file, *pin).await
            },
            BlobCommands::Download { cid, output } => {
                self.download_blob(runtime_url, cid, output.as_deref()).await
            },
            BlobCommands::Pin { cid } => {
                self.pin_blob(runtime_url, cid).await
            },
            BlobCommands::Unpin { cid } => {
                self.unpin_blob(runtime_url, cid).await
            },
            BlobCommands::Status { cid, verbose, json } => {
                self.blob_status(runtime_url, cid, *verbose, *json).await
            },
        }
    }
    
    // Existing methods...
    
    /// Query and display blob status information
    pub async fn blob_status(&self, runtime_url: &str, cid: &str, verbose: bool, json_output: bool) -> CliResult<()> {
        let url = format!("{}/api/v1/blob/{}/status", runtime_url, cid);
        
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await?;
        
        if response.status() != StatusCode::OK {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Failed to get blob status: {}", error_text).into());
        }
        
        let blob_status: BlobStatusResponse = response.json().await?;
        
        if json_output {
            // Output in JSON format
            println!("{}", serde_json::to_string_pretty(&blob_status)?);
            return Ok(());
        }
        
        // Output in table format
        let mut table = Table::new();
        table.set_header(vec!["Property", "Value"]);
        table.load_preset(UTF8_FULL);
        table.set_content_arrangement(ContentArrangement::Dynamic);
        
        // Basic information
        table.add_row(vec!["CID", &blob_status.cid]);
        table.add_row(vec!["Exists", &blob_status.exists.to_string()]);
        
        if let Some(size) = blob_status.size {
            table.add_row(vec!["Size", &format_size(size)]);
        }
        
        table.add_row(vec!["Pinned", &blob_status.pinned.to_string()]);
        
        if let Some(created_at) = blob_status.created_at {
            table.add_row(vec!["Created", &created_at.to_rfc3339()]);
        }
        
        if let Some(last_accessed) = blob_status.last_accessed {
            table.add_row(vec!["Last Accessed", &last_accessed.to_rfc3339()]);
        }
        
        // Replication information
        table.add_row(vec!["Replication Policy", &blob_status.replication.policy]);
        table.add_row(vec!["Target Factor", &blob_status.replication.target_factor.to_string()]);
        table.add_row(vec!["Current Factor", &blob_status.replication.current_factor.to_string()]);
        table.add_row(vec!["Completion", &format!("{}%", blob_status.replication.completion_percentage)]);
        
        println!("{table}");
        
        // Display health issues if any
        if !blob_status.health_issues.is_empty() {
            println!("\n🔴 Health Issues:");
            
            let mut issues_table = Table::new();
            issues_table.set_header(vec!["Issue Type", "Description", "Detected At"]);
            issues_table.load_preset(UTF8_FULL);
            
            for issue in blob_status.health_issues {
                issues_table.add_row(vec![
                    &issue.issue_type,
                    &issue.description,
                    &issue.detected_at.to_rfc3339(),
                ]);
            }
            
            println!("{issues_table}");
        }
        
        // Display hosting nodes if verbose
        if verbose {
            println!("\n📦 Hosting Nodes:");
            
            let mut nodes_table = Table::new();
            nodes_table.set_header(vec!["Node ID", "Address", "Status", "Health"]);
            nodes_table.load_preset(UTF8_FULL);
            
            for node in blob_status.replication.hosting_nodes {
                nodes_table.add_row(vec![
                    &node.id,
                    &node.address,
                    &node.status,
                    if node.healthy { "✅" } else { "❌" },
                ]);
            }
            
            println!("{nodes_table}");
        }
        
        Ok(())
    }
}

/// Format a byte size as a human-readable string
fn format_size(size: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;
    const GB: usize = MB * 1024;
    
    if size < KB {
        format!("{} B", size)
    } else if size < MB {
        format!("{:.2} KB", size as f64 / KB as f64)
    } else if size < GB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else {
        format!("{:.2} GB", size as f64 / GB as f64)
    }
} 