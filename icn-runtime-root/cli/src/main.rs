use anyhow::Result;
use clap::Command;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod commands;
mod dag_verify;
mod federation;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();
    
    // Build the CLI app
    let cli = Command::new("icn-runtime")
        .version(env!("CARGO_PKG_VERSION"))
        .about("ICN Runtime CLI")
        .subcommand(commands::wallet_test::cli())
        .subcommand(Command::new("dag")
            .about("DAG operations")
            .subcommand(Command::new("verify")
                .about("Verify DAG from genesis to tip")
                .arg(clap::Arg::new("federation")
                    .long("federation")
                    .help("Federation ID to verify")
                    .required(true))
                .arg(clap::Arg::new("from-genesis")
                    .long("from-genesis")
                    .help("Verify from genesis (otherwise from latest checkpoint)")
                    .action(clap::ArgAction::SetTrue))
                .arg(clap::Arg::new("entity")
                    .long("entity")
                    .help("Entity ID to verify (if not specified, verify all entities)")
                    .required(false))
                .arg(clap::Arg::new("storage-dir")
                    .long("storage-dir")
                    .help("Path to storage directory")
                    .required(false))
                .arg(clap::Arg::new("output")
                    .long("output")
                    .help("Output format (text or json)")
                    .default_value("text"))
                .arg(clap::Arg::new("output-file")
                    .long("output-file")
                    .help("Output file (if not specified, print to stdout)")
                    .required(false))
                .arg(clap::Arg::new("generate-proof")
                    .long("generate-proof")
                    .help("Generate proof of replay")
                    .action(clap::ArgAction::SetTrue))
                .arg(clap::Arg::new("anchor-proof")
                    .long("anchor-proof")
                    .help("Anchor proof to audit ledger")
                    .action(clap::ArgAction::SetTrue))
            )
        )
        .subcommand(Command::new("federation")
            .about("Federation operations")
            // Federation init command
            .subcommand(Command::new("init")
                .about("Initialize a new federation")
                .arg(clap::Arg::new("name")
                    .long("name")
                    .help("Federation name")
                    .required(true))
                .arg(clap::Arg::new("did")
                    .long("did")
                    .help("Federation DID (if not provided, will be auto-generated)")
                    .required(false))
                .arg(clap::Arg::new("nodes")
                    .long("nodes")
                    .help("Initial nodes (comma-separated list of DIDs)")
                    .required(true))
                .arg(clap::Arg::new("genesis-node")
                    .long("genesis-node")
                    .help("Genesis node (DID of the node creating the federation)")
                    .required(true))
                .arg(clap::Arg::new("config-file")
                    .long("config-file")
                    .help("Federation config file (TOML format)")
                    .required(false))
                .arg(clap::Arg::new("output-dir")
                    .long("output-dir")
                    .help("Output directory for federation artifacts")
                    .required(true))
                .arg(clap::Arg::new("storage-dir")
                    .long("storage-dir")
                    .help("Storage directory")
                    .required(false))
            )
            // Federation status command
            .subcommand(Command::new("status")
                .about("Check federation status")
                .arg(clap::Arg::new("federation")
                    .long("federation")
                    .help("Federation DID")
                    .required(true))
                .arg(clap::Arg::new("storage-dir")
                    .long("storage-dir")
                    .help("Storage directory")
                    .required(false))
                .arg(clap::Arg::new("output")
                    .long("output")
                    .help("Output format (text or json)")
                    .default_value("text"))
                .arg(clap::Arg::new("output-file")
                    .long("output-file")
                    .help("Output file (if not specified, print to stdout)")
                    .required(false))
            )
            // Federation verify command
            .subcommand(Command::new("verify")
                .about("Verify federation integrity")
                .arg(clap::Arg::new("federation")
                    .long("federation")
                    .help("Federation DID")
                    .required(true))
                .arg(clap::Arg::new("storage-dir")
                    .long("storage-dir")
                    .help("Storage directory")
                    .required(false))
                .arg(clap::Arg::new("from-genesis")
                    .long("from-genesis")
                    .help("Verify from genesis (otherwise from latest checkpoint)")
                    .action(clap::ArgAction::SetTrue))
                .arg(clap::Arg::new("output")
                    .long("output")
                    .help("Output format (text or json)")
                    .default_value("text"))
                .arg(clap::Arg::new("output-file")
                    .long("output-file")
                    .help("Output file (if not specified, print to stdout)")
                    .required(false))
            )
        )
        // Add other commands here
        ;
    
    // Parse arguments
    let matches = cli.get_matches();
    
    // Handle subcommands
    match matches.subcommand() {
        Some(("wallet-test", sub_matches)) => {
            let subcmd = sub_matches.subcommand().map_or("", |(s, _)| s);
            commands::wallet_test::execute(subcmd, sub_matches)
                .await?;
        }
        Some(("dag", sub_matches)) => {
            match sub_matches.subcommand() {
                Some(("verify", verify_matches)) => {
                    let options = dag_verify::DagVerifyOptions {
                        federation: verify_matches.get_one::<String>("federation").unwrap().clone(),
                        from_genesis: verify_matches.get_flag("from-genesis"),
                        entity: verify_matches.get_one::<String>("entity").cloned(),
                        storage_dir: verify_matches.get_one::<String>("storage-dir").map(|s| s.into()),
                        output: verify_matches.get_one::<String>("output").unwrap().clone(),
                        output_file: verify_matches.get_one::<String>("output-file").map(|s| s.into()),
                        generate_proof: verify_matches.get_flag("generate-proof"),
                        anchor_proof: verify_matches.get_flag("anchor-proof"),
                    };
                    dag_verify::run_verify(options).await?;
                }
                _ => {
                    println!("Unknown dag subcommand. Use --help for available commands.");
                }
            }
        }
        Some(("federation", sub_matches)) => {
            match sub_matches.subcommand() {
                Some(("init", init_matches)) => {
                    let options = federation::FederationInitOptions {
                        name: init_matches.get_one::<String>("name").unwrap().clone(),
                        did: init_matches.get_one::<String>("did").cloned(),
                        nodes: init_matches.get_one::<String>("nodes").unwrap().clone(),
                        genesis_node: init_matches.get_one::<String>("genesis-node").unwrap().clone(),
                        config_file: init_matches.get_one::<String>("config-file").map(|s| s.into()),
                        output_dir: init_matches.get_one::<String>("output-dir").unwrap().into(),
                        storage_dir: init_matches.get_one::<String>("storage-dir").map(|s| s.into()),
                    };
                    federation::run_init(options).await?;
                }
                Some(("status", status_matches)) => {
                    let options = federation::FederationStatusOptions {
                        federation: status_matches.get_one::<String>("federation").unwrap().clone(),
                        storage_dir: status_matches.get_one::<String>("storage-dir").map(|s| s.into()),
                        output: status_matches.get_one::<String>("output").unwrap().clone(),
                        output_file: status_matches.get_one::<String>("output-file").map(|s| s.into()),
                    };
                    federation::run_status(options).await?;
                }
                Some(("verify", verify_matches)) => {
                    let options = federation::FederationVerifyOptions {
                        federation: verify_matches.get_one::<String>("federation").unwrap().clone(),
                        storage_dir: verify_matches.get_one::<String>("storage-dir").map(|s| s.into()),
                        from_genesis: verify_matches.get_flag("from-genesis"),
                        output: verify_matches.get_one::<String>("output").unwrap().clone(),
                        output_file: verify_matches.get_one::<String>("output-file").map(|s| s.into()),
                    };
                    federation::run_verify(options).await?;
                }
                _ => {
                    println!("Unknown federation subcommand. Use --help for available commands.");
                }
            }
        }
        // Handle other commands here
        _ => {
            println!("No command specified. Use --help for available commands.");
        }
    }
    
    Ok(())
} 