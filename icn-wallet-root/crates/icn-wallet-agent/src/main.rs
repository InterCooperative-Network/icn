/*!
 * ICN Wallet Agent CLI
 *
 * Command-line interface for wallet operations including
 * receipt import, verification, and management.
 */

use icn_wallet_agent::run_cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_cli().await
} 