# Module 9: Operations and Deployment

## Objectives
- Understand configuration and deployment options
- Understand observability and hardening basics

## Prerequisites
- Module 8

## Key reading
- `deploy/README.md`
- `config/README.md`
- `docs/production-hardening.md`
- `docs/ops/`

## Walkthrough
ICN supports native, Docker, and Kubernetes deployment. Configuration is layered
and can be validated before startup. Observability includes metrics and health
endpoints.

## Concepts (textbook style)

### Configuration layering
ICN loads defaults, then file config, then environment overrides, then CLI flags.
This allows a safe baseline with flexible overrides for deployment environments.

### Configuration precedence (diagram)
```mermaid
flowchart TD
  defaults[Defaults] --> fileConfig[ConfigFile]
  fileConfig --> env[EnvOverrides]
  env --> cli[CliFlags]
```

### Observability
Metrics and health endpoints are required for production readiness. They provide
insight into node liveness and subsystem behavior.

### Deployment models
Native, Docker, and Kubernetes deployments serve different operational needs.
The core runtime remains the same; only the surrounding infrastructure changes.

### Deployment options (diagram)
```mermaid
flowchart TD
  icnd[icnd] --> native[NativeSystemd]
  icnd --> compose[DockerCompose]
  icnd --> k8s[Kubernetes]
```

## Detailed walkthrough (config and validation)

### 1) Choose a config file
Start from `config/icn.toml.example` or the minimal template and customize
network ports, data directory, and gateway settings.

### 2) Validate before running
Use `icnd --validate-config` to catch missing or invalid configuration early.

### 3) Start the daemon
Run `icnd` with `--config` and check logs for initialization warnings.

### 4) Verify health and metrics
Check `/health` and `/metrics` endpoints to confirm liveness and observability.

## Annotated code excerpts

### Config precedence is explicit
Source: `config/README.md`
```md
ICN reads configuration in this order (later sources override earlier):
1. Built-in defaults
2. Config file (if specified with `--config`)
3. `~/.icn/icn.toml` (if exists and no `--config`)
4. Environment variables
5. CLI flags
```
This ordering guarantees predictable overrides across environments.

### Runtime validation blocks unsafe configs
Source: `icn/bins/icnd/src/main.rs`
```rust
if args.validate_config {
    println!("Validating configuration...\n");
    match config.validate() {
        Ok(warnings) => { /* print warnings, exit 0 */ }
        Err(errors) => { /* print errors, exit 1 */ }
    }
}
```
Operators can verify configuration before startup to avoid unsafe runtime states.

## Code map
- `deploy/README.md`: deployment options and quickstart.
- `deploy/docker-compose.yml`: local stack composition.
- `config/icn.toml.example`: full config reference.
- `docs/production-hardening.md`: security and hardening guidance.

## Reference files (follow-up)
- `deploy/README.md`
- `deploy/docker-compose.yml`
- `deploy/kubernetes/`
- `deploy/helm/`
- `config/README.md`
- `config/icn.toml.example`
- `docs/production-hardening.md`

## Exercises
- Validate a config file with `--validate-config`
- Identify the ports used for transport, RPC, metrics, and health

## Checkpoints
- You can explain config precedence
- You can describe how to deploy locally with Docker
