/*!
 * ICN Federation Receipt Verification Service
 *
 * Demonstration application for the verifier.
 */

use std::sync::Arc;
use anyhow::Result;
use icn_verifier::{ReceiptVerifier, VerifierConfig, server::{start_server, ServerConfig}};
use icn_wallet_core::dag::LocalDagStore;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Get configuration from environment variables
    let verifier_config = VerifierConfig {
        private_key: std::env::var("ICN_VERIFIER_PRIVATE_KEY")
            .unwrap_or_else(|_| "VGhpcyBpcyBhIGR1bW15IHByaXZhdGUga2V5IGZvciB0ZXN0aW5nIG9ubHkh".to_string()), // Dummy key for testing
        federation_id: std::env::var("ICN_VERIFIER_FEDERATION_ID")
            .unwrap_or_else(|_| "did:icn:federation-test".to_string()),
        authorized_federations: std::env::var("ICN_VERIFIER_AUTHORIZED_FEDERATIONS")
            .unwrap_or_else(|_| "did:icn:federation1,did:icn:federation2".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect(),
    };
    
    let server_config = ServerConfig {
        host: std::env::var("ICN_VERIFIER_HOST")
            .unwrap_or_else(|_| "127.0.0.1".to_string()),
        port: std::env::var("ICN_VERIFIER_PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .unwrap_or(3000),
    };
    
    // Create a local DAG store for verification
    let dag_store = LocalDagStore::create().await?;
    
    // Create the verifier
    let verifier = Arc::new(ReceiptVerifier::new(verifier_config, dag_store));
    
    // Start the server
    start_server(server_config, verifier).await?;
    
    Ok(())
} 