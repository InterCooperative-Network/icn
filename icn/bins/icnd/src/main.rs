//! ICNd - The ICN substrate daemon
//!
//! # Safety
//! This binary denies panicking on unwrap/expect to prevent runtime crashes.
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow cfg checks for optional HSM/TPM features defined in Cargo.toml
#![allow(unexpected_cfgs)]

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use icn_core::{Config, GenesisBundle, Runtime};
use icn_identity::{AgeKeyStore, KeyStore};
use icn_kernel_api::protocol_params::ProtocolParameterStore;
use icn_kernel_api::ServiceRegistry;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use zeroize::Zeroizing;

#[derive(Parser, Debug)]
#[command(name = "icnd")]
#[command(about = "ICN substrate daemon", long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Data directory
    #[arg(short, long)]
    data_dir: Option<PathBuf>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Enable the Gateway API server
    #[arg(long)]
    gateway_enable: bool,

    /// Gateway API bind address (format: "IP:PORT")
    #[arg(long)]
    gateway_bind: Option<String>,

    /// Gateway JWT secret
    #[arg(long)]
    gateway_jwt_secret: Option<String>,

    /// Allow running Gateway API without JWT authentication (INSECURE - development only)
    #[arg(long)]
    insecure_gateway_no_jwt: bool,

    /// Validate configuration and exit (useful for CI/CD)
    #[arg(long)]
    validate_config: bool,

    /// Enable distributed tracing with OpenTelemetry
    #[arg(long)]
    tracing_enable: bool,

    /// OTLP endpoint for trace export (e.g., "http://tempo:4317")
    #[arg(long)]
    tracing_otlp_endpoint: Option<String>,

    /// Tracing sampling rate (0.0 to 1.0, default: 0.1)
    #[arg(long)]
    tracing_sampling_rate: Option<f64>,

    /// Initialize a new node: generate identity, write default config, and exit.
    /// Uses ICN_KEYSTORE_PASSPHRASE env var or prompts interactively.
    #[arg(long)]
    init: bool,

    /// Node name for the generated config (used in --init)
    #[arg(long, default_value = "icn-node")]
    node_name: String,

    /// Gateway port for the generated config (used in --init)
    #[arg(long)]
    init_gateway_port: Option<u16>,

    /// Gossip port for the generated config (used in --init)
    #[arg(long)]
    init_gossip_port: Option<u16>,

    /// Bootstrap peer addresses (used in --init, can be repeated)
    #[arg(long)]
    bootstrap_peer: Vec<String>,
}

/// Build the service registry and bootstrap handles with domain app services
///
/// This creates the app-level services (trust, governance, ledger) and
/// packages them for injection into the kernel:
/// - `ServiceRegistry` carries trait-based service abstractions
/// - `BootstrapHandles` carries typed concrete domain handles
///
/// This is the key point where domain logic is separated from kernel logic.
///
/// All mapping from icn-core config structs to primitive adapter args lives here
/// (in the daemon binary), keeping apps/ledger free of icn-core dependencies.
async fn build_services(
    config: &Config,
    identity_bundle: Option<&icn_identity::IdentityBundle>,
) -> Result<(
    ServiceRegistry,
    Option<icn_core::supervisor::BootstrapHandles>,
)> {
    let mut registry = ServiceRegistry::new();

    // Only create services if we have an identity
    let Some(bundle) = identity_bundle else {
        return Ok((registry, None));
    };

    let own_did = bundle.did().clone();

    // Open trust store
    let trust_store_path = config.store_path().join("trust");
    std::fs::create_dir_all(&trust_store_path)?;
    let trust_store: Arc<dyn icn_store::Store> =
        Arc::new(icn_store::SledStore::open(&trust_store_path)?);

    // Create TrustGraph with tokio lock (for icn-core compatibility)
    let trust_store_for_sequences = trust_store.clone();
    let trust_graph = icn_trust::TrustGraph::new(trust_store, own_did.clone());
    let trust_graph_handle = Arc::new(RwLock::new(trust_graph));

    // Create TrustService from apps/trust.
    // TrustGraph is owned by the service — no separate handle in BootstrapHandles.
    // All kernel components use the TrustService trait (not direct TrustGraph access).
    //
    // NOTE: `bundle.keypair()` will fail for hardware-backed keys (PKCS#11, TPM),
    // because those backends cannot expose a raw keypair. This is currently safe
    // because hardware key backends are not yet implemented and keystore unlock
    // fails early for such configurations.
    // TODO(hardware-keys): When hardware-backed key support is implemented, refactor
    // TrustService construction to use `bundle.did_key()` and `bundle.sign()` so that
    // both software and hardware-backed keys are supported here.
    let keypair = bundle
        .keypair()
        .context("Cannot create TrustService: failed to extract keypair from identity bundle")?;
    let trust_service =
        icn_trust_app::create_service_tokio(trust_graph_handle, keypair, trust_store_for_sequences);
    registry = registry.with_trust(trust_service);
    tracing::info!("Trust service initialized from apps/trust");

    // Create GovernanceService from apps/governance
    let param_store_path = config.protocol_params_path();
    std::fs::create_dir_all(&param_store_path).with_context(|| {
        format!(
            "Failed to create protocol params store directory: {}",
            param_store_path.display()
        )
    })?;
    let param_db = sled::open(&param_store_path).with_context(|| {
        format!(
            "Failed to open protocol params sled database: {}",
            param_store_path.display()
        )
    })?;
    let parameter_store = Arc::new(
        icn_governance::SledParameterStore::new(Arc::new(param_db))
            .context("Failed to initialize SledParameterStore")?,
    );
    // Load default parameters on first run (if store is empty)
    {
        let existing = parameter_store.list()?;
        if existing.is_empty() {
            let defaults = icn_governance::default_parameters();
            let count = defaults.len();
            for param in defaults {
                parameter_store.set(param, None, None)?;
            }
            tracing::info!("✓ {} default protocol parameters initialized", count);
        } else {
            tracing::info!(
                "✓ Protocol parameter store loaded ({} parameters)",
                existing.len()
            );
        }
    }
    let parameter_store_trait: Arc<dyn icn_governance::ProtocolParameterStore> =
        parameter_store.clone();
    let governance_service = icn_governance_app::create_service(parameter_store_trait);
    registry = registry.with_governance(governance_service);
    tracing::info!("Governance service initialized from apps/governance");

    // Create LedgerService from apps/ledger
    let ledger_store_path = config.ledger_store_path();
    std::fs::create_dir_all(&ledger_store_path).with_context(|| {
        format!(
            "Failed to create ledger store directory: {}",
            ledger_store_path.display()
        )
    })?;
    let ledger_store = Arc::new(icn_store::SledStore::open(&ledger_store_path).with_context(
        || {
            format!(
                "Failed to open ledger store: {}",
                ledger_store_path.display()
            )
        },
    )?);
    let ledger_store_trait: Arc<dyn icn_store::Store> = ledger_store.clone();
    let ledger =
        icn_ledger::Ledger::new(ledger_store_trait).context("Failed to initialize Ledger")?;
    let ledger_handle = Arc::new(RwLock::new(ledger));
    let ledger_service = icn_ledger_app::create_service(ledger_handle.clone());
    registry = registry.with_ledger(ledger_service);

    // Initialize ledger services (oracle, witness, membership, credit, dispute,
    // treasury, contracts). Config→primitive mapping stays in the daemon binary.
    let oracle_config = icn_ledger_app::config::build_oracle_config(
        config.ledger.oracle.default_ttl_secs,
        config.ledger.oracle.min_sources_for_consensus,
        config.ledger.oracle.outlier_threshold,
        config.ledger.oracle.staleness_threshold_secs,
        config.ledger.oracle.default_suspicious_rate_threshold,
        config.ledger.oracle.suspicious_rate_thresholds.clone(),
    );
    let witness_config = icn_ledger_app::config::build_witness_config(
        &config.ledger.witness.default_policy,
        config.ledger.witness.threshold,
        config.ledger.witness.quorum_required,
        config.ledger.witness.quorum_witnesses.as_deref(),
        config.ledger.witness.collection_timeout_secs,
        config.ledger.witness.min_witness_trust,
    )
    .context("Invalid witness configuration")?;
    let ledger_services = icn_ledger_app::init::init_ledger_services(
        ledger_handle.clone(),
        ledger_store.clone(),
        own_did.clone(),
        oracle_config,
        witness_config,
    )
    .await
    .context("Failed to initialize ledger services")?;
    tracing::info!("Ledger service initialized from apps/ledger");

    // Create CharterPolicyOracle from apps/charter (Phase 1 charter engine).
    // Starts empty; charters are deployed at runtime via governance ratification.
    // We hold the concrete Arc<CharterPolicyOracle> so the gateway can wire its
    // on_charter_accepted hook to it; the kernel sees only the dyn PolicyOracle view.
    let charter_oracle = Arc::new(icn_charter_app::CharterPolicyOracle::new());
    registry =
        registry.with_charter_oracle(
            charter_oracle.clone() as Arc<dyn icn_kernel_api::authz::PolicyOracle>
        );
    tracing::info!("Charter oracle initialized from apps/charter");

    // Build charter ratification hook: when a Charter proposal is accepted the governance
    // handler calls this closure with (charter_id, charter_yaml).  We parse the YAML here
    // and deploy into the oracle so subsequent CCL evaluations use the new charter.
    let oracle_for_hook = charter_oracle.clone();
    let charter_accepted_hook: Arc<dyn Fn(String, String) + Send + Sync> = Arc::new(
        move |charter_id, yaml| match icn_ccl::schema::CclDocument::from_yaml(&yaml) {
            Ok(doc) => {
                let ctx = icn_ccl::schema::bridge::CharterContext::new().with_members(100);
                oracle_for_hook.deploy_charter(charter_id.clone(), doc, ctx);
                tracing::info!("Charter '{}' deployed via ratification", charter_id);
            }
            Err(e) => {
                tracing::warn!(
                    "Charter ratification: failed to parse YAML for '{}': {}",
                    charter_id,
                    e
                );
            }
        },
    );

    let bootstrap_handles = icn_core::supervisor::BootstrapHandles {
        ledger: ledger_handle,
        ledger_store,
        dispute_manager: ledger_services.dispute_manager,
        treasury_manager: ledger_services.treasury_manager,
        contract_runtime: ledger_services.contract_runtime,
        contract_actor: ledger_services.contract_actor,
        protocol_parameter_store: parameter_store.clone()
            as Arc<dyn icn_kernel_api::protocol_params::ProtocolParameterStore>,
        effect_subscription_factory: Some(Box::new(|effect_callback| {
            icn_governance_actor::create_effect_subscription(move |effects, receipt_id| {
                effect_callback(effects, receipt_id);
            })
        })),
        charter_accepted_hook: Some(charter_accepted_hook),
    };

    Ok((registry, Some(bootstrap_handles)))
}

/// Handle `icnd --init`: generate a fresh identity and default config, then exit.
///
/// Creates:
/// 1. Data directory structure
/// 2. Age-encrypted keystore with a new Ed25519 identity
/// 3. `config.toml` with sane defaults for the node
///
/// Uses `ICN_KEYSTORE_PASSPHRASE` env var or prompts interactively.
fn handle_init(args: &Args) -> Result<()> {
    let data_dir = args
        .data_dir
        .clone()
        .unwrap_or_else(|| Config::default().data_dir);

    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("Failed to create data directory: {}", data_dir.display()))?;

    let keystore_path = data_dir.join("identity.age");
    let config_path = data_dir.join("config.toml");

    // Check if already initialized
    if keystore_path.exists() {
        anyhow::bail!(
            "Node already initialized (keystore exists at {}). \
             Remove the data directory to re-initialize.",
            keystore_path.display()
        );
    }

    println!("Initializing ICN node in {}...\n", data_dir.display());

    // Get passphrase
    let passphrase = read_passphrase("Enter passphrase for new keystore: ")
        .context("Failed to read passphrase")?;

    // Initialize keystore (generates Ed25519 keypair + TLS binding)
    let keystore =
        AgeKeyStore::init(&keystore_path, &passphrase).context("Failed to initialize keystore")?;
    let did = keystore
        .get_keypair()
        .context("Failed to read keypair from new keystore")?
        .did()
        .clone();

    println!("  Identity: {did}");
    println!("  Keystore: {}", keystore_path.display());

    // Build config with CLI overrides
    let gateway_port = args.init_gateway_port.unwrap_or(8000);
    let gossip_port = args.init_gossip_port.unwrap_or(9000);
    let did_str = did.to_string();

    let mut config = Config {
        data_dir: data_dir.clone(),
        ..Config::default()
    };
    config.gateway.enabled = true;
    config.gateway.bind_addr = format!("0.0.0.0:{gateway_port}");
    // Derive a development JWT secret from the node DID (not for production)
    config.gateway.jwt_secret =
        format!("icn-dev-{}", &did_str[8..std::cmp::min(40, did_str.len())]);
    config.network.listen_addr = format!("0.0.0.0:{gossip_port}");
    config.network.bootstrap_peers = args.bootstrap_peer.clone();

    // Write config
    config
        .to_file(&config_path)
        .with_context(|| format!("Failed to write config to {}", config_path.display()))?;

    println!("  Config:   {}", config_path.display());

    // Generate genesis bundle — a sealed, versioned document of initial network state.
    // Uses the node name as the network_id so bundles from different init invocations
    // are distinguishable. The bundle records the node's DID and seed peers so it can
    // be shared with other nodes joining the same devnet.
    let genesis_path = data_dir.join("genesis.json");
    let mut bundle = GenesisBundle::new_single_node(&args.node_name, did.to_string());
    bundle.seed_peers = args.bootstrap_peer.clone();
    let bundle = bundle.seal().context("Failed to seal genesis bundle")?;
    bundle
        .to_file(&genesis_path)
        .context("Failed to write genesis bundle")?;

    println!("  Genesis:  {}\n", genesis_path.display());
    println!("Node initialized successfully.");
    println!("  Start with: icnd --config {}", config_path.display());

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Handle --init: generate identity + config and exit
    if args.init {
        return handle_init(&args);
    }

    // Initialize rustls crypto provider (required for QUIC/TLS)
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install default crypto provider"))?;

    // Load or create config (before tracing init so we can use tracing config)
    let mut config = if let Some(config_path) = &args.config {
        Config::from_file(config_path).context("Failed to load config file")?
    } else {
        Config::default()
    };

    // Apply tracing CLI args before initialization
    if args.tracing_enable {
        config.observability.tracing.enabled = true;
    }
    if let Some(endpoint) = args.tracing_otlp_endpoint {
        config.observability.tracing.otlp_endpoint = endpoint;
    }
    if let Some(rate) = args.tracing_sampling_rate {
        config.observability.tracing.sampling_rate = rate;
    }

    // Check for OTEL environment variables (standard OpenTelemetry config)
    if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        config.observability.tracing.otlp_endpoint = endpoint;
        config.observability.tracing.enabled = true;
    }
    if let Ok(service_name) = std::env::var("OTEL_SERVICE_NAME") {
        config.observability.tracing.service_name = service_name;
    }

    // Initialize observability with distributed tracing support
    let tracing_config = config.observability.tracing.to_obs_config(None);
    icn_obs::init_tracing(&tracing_config)?;
    tracing::info!("ICNd starting");

    // Override with CLI args
    if let Some(data_dir) = args.data_dir {
        config.data_dir = data_dir;
    }

    config.observability.log_level = args.log_level;

    // Apply gateway CLI arguments
    if args.gateway_enable {
        config.gateway.enabled = true;
    }

    if let Some(bind_addr) = args.gateway_bind {
        config.gateway.bind_addr = bind_addr;
    }

    // Read JWT secret from CLI arg, then env var ICN_GATEWAY_JWT_SECRET, then config
    if let Some(jwt_secret) = args.gateway_jwt_secret {
        config.gateway.jwt_secret = jwt_secret;
    } else if let Ok(jwt_secret) = std::env::var("ICN_GATEWAY_JWT_SECRET") {
        config.gateway.jwt_secret = jwt_secret;
        tracing::debug!(
            "Gateway JWT secret loaded from ICN_GATEWAY_JWT_SECRET environment variable"
        );
    }

    // If --insecure-gateway-no-jwt is set, use a placeholder to pass validation
    // (the actual insecure mode warning is logged later)
    let insecure_no_jwt = args.insecure_gateway_no_jwt
        && config.gateway.enabled
        && config.gateway.jwt_secret.is_empty();
    if insecure_no_jwt {
        config.gateway.jwt_secret = "__INSECURE_NO_JWT__".to_string();
    }

    // Handle --validate-config flag
    if args.validate_config {
        println!("Validating configuration...\n");
        match config.validate() {
            Ok(warnings) => {
                if warnings.is_empty() {
                    println!("\x1b[32m✓ Configuration is valid (no warnings)\x1b[0m");
                } else {
                    println!("\x1b[32m✓ Configuration is valid\x1b[0m\n");
                    println!("Warnings:");
                    for warning in &warnings {
                        println!("  \x1b[33m⚠\x1b[0m {warning}");
                    }
                }
                std::process::exit(0);
            }
            Err(errors) => {
                println!("\x1b[31m✗ Configuration has errors:\x1b[0m\n");
                for error in &errors {
                    println!("  \x1b[31m✗\x1b[0m {error}");
                }
                std::process::exit(1);
            }
        }
    }

    // Validate config on normal startup (log warnings, exit on errors)
    match config.validate() {
        Ok(warnings) => {
            for warning in warnings {
                tracing::warn!("Config: {}", warning);
            }
        }
        Err(errors) => {
            for error in &errors {
                tracing::error!("Config error: {}", error);
            }
            tracing::error!(
                "Configuration has {} error(s). Run with --validate-config for details.",
                errors.len()
            );
            std::process::exit(1);
        }
    }

    // Ensure data directory exists
    std::fs::create_dir_all(&config.data_dir)?;

    tracing::info!("Data directory: {:?}", config.data_dir);
    tracing::info!("Log level: {}", config.observability.log_level);

    if config.gateway.enabled {
        tracing::info!("Gateway API enabled on {}", config.gateway.bind_addr);
        if insecure_no_jwt {
            tracing::warn!(
                "⚠️  Gateway running WITHOUT JWT authentication (--insecure-gateway-no-jwt)"
            );
            tracing::warn!("⚠️  This is insecure and should only be used for development!");
            // Clear the placeholder so gateway knows to skip JWT validation
            config.gateway.jwt_secret = String::new();
        } else {
            tracing::info!(
                "Gateway JWT secret configured (length: {})",
                config.gateway.jwt_secret.len()
            );
        }
    } else {
        tracing::debug!("Gateway API disabled");
    }

    // Check for identity keystore
    let keystore_path = config.keystore_path();
    let identity_bundle = if keystore_path.exists() {
        tracing::info!("Identity keystore found at: {:?}", keystore_path);
        tracing::info!("Using identity backend: {}", config.identity.backend);

        // Get passphrase: tries ICN_KEYSTORE_PASSPHRASE env var first, then ICN_PASSPHRASE,
        // then falls back to interactive prompt. For automated deployments (systemd, Docker, K8s),
        // set one of the environment variables.
        // Security: Passphrase uses Zeroizing<Vec<u8>> to automatically clear from memory.
        let passphrase =
            read_passphrase("Enter keystore passphrase: ").context("Failed to read passphrase")?;

        // Load and unlock keystore
        // Check backend type and use appropriate loading strategy
        let mut keystore = match config.identity.backend.as_str() {
            "age" => {
                // Age backend: use direct AgeKeyStore
                AgeKeyStore::open(&keystore_path).context("Failed to open Age keystore")?
            }
            "pkcs11" | "tpm" => {
                // Hardware backends: not yet implemented
                // Return explicit error instead of calling unimplemented factory
                anyhow::bail!(
                    "Hardware identity backend '{}' is not yet implemented. \
                     See docs/reference/config/identity-backend-configuration.md for status.",
                    config.identity.backend
                );
            }
            other => {
                anyhow::bail!("Unknown identity backend '{}'", other);
            }
        };

        keystore
            .unlock(&passphrase)
            .context("Failed to unlock keystore - incorrect passphrase?")?;

        let bundle = keystore
            .get_identity_bundle()
            .context("Failed to get identity bundle from keystore")?;

        tracing::info!("Identity loaded: {} (with DID-TLS binding)", bundle.did());
        Some(bundle.clone())
    } else {
        tracing::warn!("No identity keystore found at: {:?}", keystore_path);
        tracing::warn!("Run 'icnctl id init' to create an identity");
        tracing::warn!("Daemon will run without Identity and Network actors");
        None
    };

    // Build service registry and bootstrap handles with domain app services.
    // ServiceRegistry carries trait abstractions; BootstrapHandles carries typed domain objects.
    let (service_registry, bootstrap_handles) =
        build_services(&config, identity_bundle.as_ref()).await?;

    // Create runtime with injected services and bootstrap handles
    let mut runtime = Runtime::new(config.clone(), identity_bundle).with_services(service_registry);
    if let Some(handles) = bootstrap_handles {
        runtime = runtime.with_bootstrap_handles(handles);
    }

    // Get shutdown signal before moving runtime
    let shutdown_tx = runtime.shutdown_tx();

    // Spawn runtime task
    let mut runtime_handle = tokio::spawn(async move { runtime.run().await });

    // Wait for shutdown signal or runtime completion
    let shutdown_result = tokio::select! {
        result = &mut runtime_handle => {
            // Runtime exited on its own
            tracing::info!("Runtime exited");
            Some(result??)
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received Ctrl+C, shutting down gracefully...");
            let _ = shutdown_tx.send(());
            None
        }
        _ = wait_for_sigterm() => {
            tracing::info!("Received SIGTERM, shutting down gracefully...");
            let _ = shutdown_tx.send(());
            None
        }
    };

    // If we signaled shutdown, wait for runtime to finish
    if shutdown_result.is_none() {
        runtime_handle.await??;
    }

    tracing::info!("ICNd stopped");

    // Flush any pending traces before exit
    icn_obs::shutdown_tracing();

    Ok(())
}

/// Wait for SIGTERM signal on Unix systems
#[cfg(unix)]
async fn wait_for_sigterm() -> Result<()> {
    use tokio::signal::unix::{signal, SignalKind};
    let mut stream = signal(SignalKind::terminate())?;
    stream.recv().await;
    Ok(())
}

/// Wait for SIGTERM signal on non-Unix systems (no-op)
#[cfg(not(unix))]
async fn wait_for_sigterm() -> Result<()> {
    // On non-Unix systems, just wait forever
    std::future::pending().await
}

/// Read passphrase from environment or stdin
///
/// Checks in order:
/// 1. ICN_KEYSTORE_PASSPHRASE environment variable (preferred for production)
/// 2. ICN_PASSPHRASE environment variable (legacy, for backward compatibility)
/// 3. Interactive prompt (for development)
///
/// Returns a zeroizing container that automatically clears the passphrase
/// from memory when it goes out of scope, preventing sensitive data leakage.
///
/// Security: Both the String returned by rpassword and the final `Vec<u8>` are
/// wrapped in Zeroizing to ensure complete memory cleanup.
fn read_passphrase(prompt: &str) -> Result<Zeroizing<Vec<u8>>> {
    // Check for ICN_KEYSTORE_PASSPHRASE environment variable first (preferred)
    if let Ok(passphrase) = std::env::var("ICN_KEYSTORE_PASSPHRASE") {
        tracing::debug!("Passphrase loaded from ICN_KEYSTORE_PASSPHRASE environment variable");
        return Ok(Zeroizing::new(passphrase.into_bytes()));
    }

    // Check for ICN_PASSPHRASE environment variable (legacy, backward compatible)
    if let Ok(passphrase) = std::env::var("ICN_PASSPHRASE") {
        tracing::debug!("Passphrase loaded from ICN_PASSPHRASE environment variable");
        return Ok(Zeroizing::new(passphrase.into_bytes()));
    }

    // Fall back to interactive prompt
    print!("{prompt}");
    io::stdout().flush()?;
    // Wrap the String immediately in Zeroizing to prevent it from lingering in memory
    let passphrase_str =
        Zeroizing::new(rpassword::read_password().context("Failed to read password")?);
    // Convert to bytes (copies from zeroized String, which is then dropped and zeroed)
    Ok(Zeroizing::new(passphrase_str.as_bytes().to_vec()))
}
