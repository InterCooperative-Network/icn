# ICN Wallet Agent

The `icn-wallet-agent` crate provides autonomous agent capabilities for the ICN Wallet. It enables background processing, scheduled tasks, and automated interactions with the ICN network.

## Features

- **Background Processing**: Perform wallet operations in the background
- **Scheduled Tasks**: Execute operations on schedules (one-time or recurring)
- **Event Handling**: React to wallet and network events with custom logic
- **Automation Rules**: Define rules for automated wallet behavior
- **Federation Monitoring**: Track federation activity and updates
- **Credential Management**: Automate credential renewal and verification
- **Proposal Tracking**: Monitor governance proposals and take actions

## Agent Types

- **SyncAgent**: Manages synchronization with the ICN network
- **GovernanceAgent**: Handles governance-related automated tasks
- **NotificationAgent**: Manages notifications and alerts
- **CredentialAgent**: Handles credential management tasks
- **SchedulingAgent**: Manages scheduled operations

## Usage

### Adding as a Dependency

```toml
[dependencies]
icn-wallet-agent = { version = "0.1.0", path = "../icn-wallet-agent" }
```

### Basic Example

```rust
use icn_wallet_agent::{AgentManager, AgentConfig, SyncAgentConfig};
use icn_wallet_core::WalletCore;

async fn agent_example(wallet: WalletCore) -> Result<(), Box<dyn std::error::Error>> {
    // Configure the agent system
    let agent_config = AgentConfig {
        enable_background_sync: true,
        sync_interval_seconds: 300, // 5 minutes
        enable_governance_monitoring: true,
        ..Default::default()
    };
    
    // Create the agent manager
    let mut agent_manager = AgentManager::new(wallet.clone(), agent_config);
    
    // Start the agent system
    agent_manager.start().await?;
    
    // Configure a specific agent
    let sync_config = SyncAgentConfig {
        federation_endpoints: vec!["https://node1.example.com".to_string()],
        retry_attempts: 3,
        ..Default::default()
    };
    
    agent_manager.configure_sync_agent(sync_config).await?;
    
    // Create a custom scheduled task
    agent_manager.schedule_task(
        "daily-backup",
        "0 0 * * *", // Cron expression for daily at midnight
        Box::new(|wallet| {
            Box::pin(async move {
                println!("Running daily backup");
                // Perform backup operations
                Ok(())
            })
        }),
    ).await?;
    
    println!("Agent system is running");
    
    // Later, when shutting down the application
    agent_manager.stop().await?;
    
    Ok(())
}
```

### Event Subscription Example

```rust
use icn_wallet_agent::{AgentManager, EventSubscription, WalletEvent};
use futures::StreamExt;

async fn event_subscription_example(agent_manager: &AgentManager) -> Result<(), Box<dyn std::error::Error>> {
    // Subscribe to wallet events
    let mut event_stream = agent_manager.subscribe_to_events(vec![
        WalletEvent::NewCredential,
        WalletEvent::ProposalCreated,
        WalletEvent::SyncCompleted,
    ]).await?;
    
    // Process events
    tokio::spawn(async move {
        while let Some(event) = event_stream.next().await {
            match event {
                WalletEvent::NewCredential(cred_id) => {
                    println!("New credential received: {}", cred_id);
                    // Process new credential
                },
                WalletEvent::ProposalCreated(prop_id) => {
                    println!("New proposal created: {}", prop_id);
                    // Process new proposal
                },
                WalletEvent::SyncCompleted => {
                    println!("Synchronization completed");
                    // Process sync completion
                },
                _ => {}
            }
        }
    });
    
    Ok(())
}
```

## Integration with ICN Wallet

This crate works with other ICN wallet components:

- `icn-wallet-core`: Provides the wallet functionality the agents operate on
- `icn-wallet-sync`: Used by the sync agent for network communication
- `icn-wallet-storage`: Used to persist agent state and task information
- `icn-wallet-identity`: Used for identity operations in automated tasks

## License

This crate is part of the ICN Wallet project and is licensed under the same terms as the parent project. 