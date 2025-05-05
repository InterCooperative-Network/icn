/// Federation commands
#[clap(subcommand)]
Federation(FederationArgs),

/// Governance commands
#[clap(subcommand)]
Governance(GovernanceArgs),

/// Mesh Compute commands
#[clap(subcommand)]
Mesh(MeshArgs),
} 

// Match first-level commands
match cmd {
    WalletCommand::Account(cmd) => handle_account_command(&agent, cmd).await?,
    WalletCommand::Config(cmd) => handle_config_command(&agent, cmd).await?,
    WalletCommand::Contacts(cmd) => handle_contacts_command(&agent, cmd).await?,
    WalletCommand::DAG(cmd) => handle_dag_command(&agent, cmd).await?,
    WalletCommand::Federation(cmd) => handle_federation_command(&agent, cmd).await?,
    WalletCommand::Governance(cmd) => handle_governance_command(&agent, cmd).await?,
    WalletCommand::Mesh(cmd) => handle_mesh_command(&agent, cmd).await?,
}; 