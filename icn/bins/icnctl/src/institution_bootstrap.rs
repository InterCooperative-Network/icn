use crate::{get_keystore_path, print_gateway_error, print_http_error, read_passphrase};
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use icn_governance::{
    ActivityKind, BootstrapEntityType, BootstrapOperation, BootstrapPlan, BootstrapSeedManifest,
    BootstrapSeedRef, InstitutionBootstrapManifest, ProgramKind, StructureKind,
};
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Subcommand, Debug)]
pub enum InstitutionCommands {
    /// Validate or plan a generic institution package bootstrap
    Bootstrap {
        #[command(subcommand)]
        command: InstitutionBootstrapCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum InstitutionBootstrapCommands {
    /// Validate package manifest ordering, charter parsing, and seed references
    Validate {
        /// Path to the institution package directory
        #[arg(long)]
        package: PathBuf,

        /// Output machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Build the ordered generic bootstrap plan for a package
    Plan {
        /// Path to the institution package directory
        #[arg(long)]
        package: PathBuf,

        /// Output machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Apply the generic bootstrap plan to a running node as far as supported
    Apply {
        /// Path to the institution package directory
        #[arg(long)]
        package: PathBuf,

        /// Gateway API URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,

        /// Cooperative ID used for gateway authentication
        #[arg(short, long)]
        coop_id: String,

        /// Output machine-readable JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapValidationReport {
    pub package_dir: String,
    pub manifest_path: String,
    pub charter_path: String,
    pub charter_valid: bool,
    pub seed_count: usize,
    pub validated_seed_files: Vec<String>,
    pub warnings: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapPlanReport {
    pub manifest_path: String,
    pub charter_path: String,
    pub charter_valid: bool,
    pub plan: BootstrapPlan,
    pub preview: BootstrapPreview,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BootstrapPreview {
    pub entity_count: usize,
    pub governance_domain_count: usize,
    pub structure_count: usize,
    pub activity_count: usize,
    pub program_count: usize,
    pub milestone_count: usize,
    pub role_assignment_count: usize,
    pub deferred_role_assignment_count: usize,
    pub live_apply_supported: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapApplyReport {
    pub manifest_path: String,
    pub charter_path: String,
    pub charter_valid: bool,
    pub completed: Vec<BootstrapApplyStepReport>,
    pub deferred: Vec<BootstrapApplyStepReport>,
    pub unsupported: Vec<BootstrapApplyStepReport>,
    pub failed: Option<BootstrapApplyFailure>,
    pub alias_bindings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapApplyStepReport {
    pub operation: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapApplyFailure {
    pub operation: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
struct ApplyState {
    alias_bindings: BTreeMap<String, String>,
}

impl ApplyState {
    fn bind(&mut self, alias: &str, live_id: &str) {
        self.alias_bindings
            .insert(alias.to_string(), live_id.to_string());
    }

    fn resolve(&self, alias_or_id: &str) -> String {
        self.alias_bindings
            .get(alias_or_id)
            .cloned()
            .unwrap_or_else(|| alias_or_id.to_string())
    }

    fn resolve_bound(&self, alias_or_id: &str) -> Option<String> {
        self.alias_bindings.get(alias_or_id).cloned()
    }
}

enum ApplyOutcome {
    Completed(String),
    Deferred(String),
    Unsupported(String),
    Failed(String),
}

const BOOTSTRAP_SUBJECT_DID_PLACEHOLDER: &str = "$bootstrap_subject_did";

#[derive(Debug, Clone)]
struct BootstrapAuthContext {
    token: String,
    subject_did: String,
}

pub async fn handle_institution_command(cmd: InstitutionCommands, data_dir: &Path) -> Result<()> {
    match cmd {
        InstitutionCommands::Bootstrap { command } => match command {
            InstitutionBootstrapCommands::Validate { package, json } => {
                let report = validate_package(&package)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print_validation_report(&report);
                }
            }
            InstitutionBootstrapCommands::Plan { package, json } => {
                let report = build_plan_report(&package)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print_plan_report(&report);
                }
            }
            InstitutionBootstrapCommands::Apply {
                package,
                gateway,
                coop_id,
                json,
            } => {
                let report = apply_package(&package, &gateway, &coop_id, data_dir).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print_apply_report(&report);
                }
            }
        },
    }

    Ok(())
}

fn print_validation_report(report: &BootstrapValidationReport) {
    println!("Institution Bootstrap Validation");
    println!("================================\n");
    println!("Package:         {}", report.package_dir);
    println!("Manifest:        {}", report.manifest_path);
    println!("Charter:         {}", report.charter_path);
    println!(
        "Charter status:  {}",
        if report.charter_valid {
            "valid"
        } else {
            "invalid"
        }
    );
    println!("Seed files:      {}", report.seed_count);

    if !report.validated_seed_files.is_empty() {
        println!("\nValidated seed files:");
        for path in &report.validated_seed_files {
            println!("  - {path}");
        }
    }

    if !report.warnings.is_empty() {
        println!("\nWarnings:");
        for warning in &report.warnings {
            println!("  - {warning}");
        }
    }

    if !report.blockers.is_empty() {
        println!("\nBlockers:");
        for blocker in &report.blockers {
            println!("  - {blocker}");
        }
    }
}

fn print_plan_report(report: &BootstrapPlanReport) {
    println!("Institution Bootstrap Plan");
    println!("==========================\n");
    println!("Manifest:        {}", report.manifest_path);
    println!("Charter:         {}", report.charter_path);
    println!(
        "Charter status:  {}",
        if report.charter_valid {
            "valid"
        } else {
            "invalid"
        }
    );
    println!("Operations:      {}", report.plan.operations.len());
    println!();

    for op in &report.plan.operations {
        println!("- {}", summarize_operation(op));
    }

    println!("\nPreview counts:");
    println!("  Entities:              {}", report.preview.entity_count);
    println!(
        "  Governance domains:    {}",
        report.preview.governance_domain_count
    );
    println!(
        "  Structures:            {}",
        report.preview.structure_count
    );
    println!("  Activities:            {}", report.preview.activity_count);
    println!("  Programs:              {}", report.preview.program_count);
    println!(
        "  Milestones:            {}",
        report.preview.milestone_count
    );
    println!(
        "  Role assignments:      {}",
        report.preview.role_assignment_count
    );
    println!(
        "  Deferred role records: {}",
        report.preview.deferred_role_assignment_count
    );
    println!(
        "  Live apply supported:  {}",
        if report.preview.live_apply_supported {
            "yes"
        } else {
            "no"
        }
    );

    if !report.plan.warnings.is_empty() {
        println!("\nWarnings:");
        for warning in &report.plan.warnings {
            println!("  - {warning}");
        }
    }

    if !report.plan.blockers.is_empty() {
        println!("\nBlockers:");
        for blocker in &report.plan.blockers {
            println!("  - {blocker}");
        }
    }
}

fn print_apply_report(report: &BootstrapApplyReport) {
    println!("Institution Bootstrap Apply");
    println!("===========================\n");
    println!("Manifest:        {}", report.manifest_path);
    println!("Charter:         {}", report.charter_path);
    println!(
        "Charter status:  {}",
        if report.charter_valid {
            "valid"
        } else {
            "invalid"
        }
    );
    println!("Completed:       {}", report.completed.len());
    println!("Deferred:        {}", report.deferred.len());
    println!("Unsupported:     {}", report.unsupported.len());
    println!(
        "Failed:          {}",
        if report.failed.is_some() { "yes" } else { "no" }
    );

    if !report.completed.is_empty() {
        println!("\nCompleted steps:");
        for step in &report.completed {
            println!("  - {}: {}", step.operation, step.detail);
        }
    }

    if !report.deferred.is_empty() {
        println!("\nDeferred steps:");
        for step in &report.deferred {
            println!("  - {}: {}", step.operation, step.detail);
        }
    }

    if !report.unsupported.is_empty() {
        println!("\nUnsupported steps:");
        for step in &report.unsupported {
            println!("  - {}: {}", step.operation, step.detail);
        }
    }

    if !report.alias_bindings.is_empty() {
        println!("\nAlias bindings:");
        for (alias, live_id) in &report.alias_bindings {
            println!("  - {alias} -> {live_id}");
        }
    }

    if let Some(failure) = &report.failed {
        println!("\nFailure:");
        println!("  - {}: {}", failure.operation, failure.detail);
    }
}

fn summarize_operation(op: &BootstrapOperation) -> String {
    match op {
        BootstrapOperation::ValidateCharter { path } => format!("validate charter {path}"),
        BootstrapOperation::CreateEntity {
            id, entity_type, ..
        } => {
            format!("create {:?} entity {id}", entity_type)
        }
        BootstrapOperation::ProvisionGovernanceDomain { id, .. } => {
            format!("provision governance domain {id}")
        }
        BootstrapOperation::CreateStructure { id, kind, .. } => {
            format!("create {:?} structure {id}", kind)
        }
        BootstrapOperation::CreateProgram { id, .. } => format!("create program {id}"),
        BootstrapOperation::CreateActivity { id, .. } => format!("create activity {id}"),
        BootstrapOperation::CreateMilestone { id, program_id, .. } => {
            format!("create milestone {id} in {program_id}")
        }
        BootstrapOperation::AssignRole {
            structure_id,
            person_did,
            role,
            ..
        } => format!("assign role {role} on {structure_id} to {person_did}"),
        BootstrapOperation::DeferRoleAssignment {
            structure_id, role, ..
        } => format!("defer role {role} on {structure_id}"),
    }
}

pub fn validate_package(package_dir: &Path) -> Result<BootstrapValidationReport> {
    let loaded = load_package(package_dir)?;
    Ok(BootstrapValidationReport {
        package_dir: package_dir.display().to_string(),
        manifest_path: loaded.manifest_path.display().to_string(),
        charter_path: loaded.charter_path.display().to_string(),
        charter_valid: true,
        seed_count: loaded.seed_paths.len(),
        validated_seed_files: loaded
            .seed_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        warnings: loaded.warnings,
        blockers: loaded.blockers,
    })
}

pub fn build_plan_report(package_dir: &Path) -> Result<BootstrapPlanReport> {
    let loaded = load_package(package_dir)?;
    let plan = make_plan(&loaded);
    let preview = preview_execute(&plan);
    Ok(BootstrapPlanReport {
        manifest_path: loaded.manifest_path.display().to_string(),
        charter_path: loaded.charter_path.display().to_string(),
        charter_valid: true,
        plan,
        preview,
    })
}

pub async fn apply_package(
    package_dir: &Path,
    gateway: &str,
    coop_id: &str,
    data_dir: &Path,
) -> Result<BootstrapApplyReport> {
    let loaded = load_package(package_dir)?;
    let plan = make_plan(&loaded);
    let auth = get_bootstrap_gateway_token(gateway, coop_id, data_dir).await?;
    run_apply_plan(&loaded, &plan, gateway, &auth).await
}

/// Apply a pre-loaded plan with a caller-supplied auth context.
///
/// Separates authentication from execution so tests can inject a fixture token
/// without touching the keystore.  The public CLI path is `apply_package`
/// which obtains auth via the normal challenge/verify flow.
#[cfg(test)]
async fn apply_with_auth_ctx(
    package_dir: &Path,
    gateway: &str,
    auth: BootstrapAuthContext,
) -> Result<BootstrapApplyReport> {
    let loaded = load_package(package_dir)?;
    let plan = make_plan(&loaded);
    run_apply_plan(&loaded, &plan, gateway, &auth).await
}

async fn run_apply_plan(
    loaded: &LoadedPackage,
    plan: &BootstrapPlan,
    gateway: &str,
    auth: &BootstrapAuthContext,
) -> Result<BootstrapApplyReport> {
    let client = reqwest::Client::new();
    let mut state = ApplyState::default();

    let mut report = BootstrapApplyReport {
        manifest_path: loaded.manifest_path.display().to_string(),
        charter_path: loaded.charter_path.display().to_string(),
        charter_valid: true,
        completed: Vec::new(),
        deferred: Vec::new(),
        unsupported: vec![BootstrapApplyStepReport {
            operation: "store charter".to_string(),
            detail: "package-owned CCL charters validate locally today, but there is no generic live CCL charter registration/activation sink yet".to_string(),
        }],
        failed: None,
        alias_bindings: BTreeMap::new(),
    };

    for op in &plan.operations {
        let operation = summarize_operation(op);
        match apply_operation(op, gateway, &client, auth, &mut state).await {
            ApplyOutcome::Completed(detail) => report
                .completed
                .push(BootstrapApplyStepReport { operation, detail }),
            ApplyOutcome::Deferred(detail) => report
                .deferred
                .push(BootstrapApplyStepReport { operation, detail }),
            ApplyOutcome::Unsupported(detail) => report
                .unsupported
                .push(BootstrapApplyStepReport { operation, detail }),
            ApplyOutcome::Failed(detail) => {
                report.failed = Some(BootstrapApplyFailure { operation, detail });
                report.alias_bindings = state.alias_bindings;
                return Ok(report);
            }
        }
    }

    report.alias_bindings = state.alias_bindings;
    Ok(report)
}

struct LoadedPackage {
    manifest_path: PathBuf,
    charter_path: PathBuf,
    seed_paths: Vec<PathBuf>,
    manifest: InstitutionBootstrapManifest,
    seeds: Vec<BootstrapSeedManifest>,
    warnings: Vec<String>,
    blockers: Vec<String>,
}

fn load_package(package_dir: &Path) -> Result<LoadedPackage> {
    let manifest_path = package_dir.join("bootstrap.yaml");
    let manifest_yaml = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let manifest: InstitutionBootstrapManifest = serde_yaml::from_str(&manifest_yaml)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;

    let charter_path = package_dir.join(&manifest.charter.path);
    let charter_yaml = fs::read_to_string(&charter_path)
        .with_context(|| format!("Failed to read {}", charter_path.display()))?;
    let charter_doc = icn_ccl::schema::CclDocument::from_yaml(&charter_yaml)
        .with_context(|| format!("Failed to parse {}", charter_path.display()))?;
    charter_doc
        .validate()
        .with_context(|| format!("Validation failed for {}", charter_path.display()))?;

    validate_seed_order(&manifest.seeds)?;

    let mut seeds = Vec::new();
    let mut seed_paths = Vec::new();
    for seed_ref in &manifest.seeds {
        let seed_path = package_dir.join(&seed_ref.path);
        let seed_yaml = fs::read_to_string(&seed_path)
            .with_context(|| format!("Failed to read {}", seed_path.display()))?;
        let seed: BootstrapSeedManifest = serde_yaml::from_str(&seed_yaml)
            .with_context(|| format!("Failed to parse {}", seed_path.display()))?;
        if seed.seed_kind() != seed_ref.kind {
            bail!(
                "Seed kind mismatch for {}: manifest says {:?}, file says {:?}",
                seed_path.display(),
                seed_ref.kind,
                seed.seed_kind()
            );
        }
        seeds.push(seed);
        seed_paths.push(seed_path);
    }

    let (warnings, blockers) = validate_seed_graph(&seeds);

    Ok(LoadedPackage {
        manifest_path,
        charter_path,
        seed_paths,
        manifest,
        seeds,
        warnings,
        blockers,
    })
}

fn validate_seed_order(seeds: &[BootstrapSeedRef]) -> Result<()> {
    let mut last = None;
    let mut seen_orders = BTreeSet::new();
    for seed in seeds {
        if !seen_orders.insert(seed.order) {
            bail!("Duplicate bootstrap seed order {}", seed.order);
        }
        if let Some(prev) = last {
            if seed.order <= prev {
                bail!("Bootstrap seed order must be strictly increasing");
            }
        }
        last = Some(seed.order);
    }
    Ok(())
}

fn validate_seed_graph(seeds: &[BootstrapSeedManifest]) -> (Vec<String>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut blockers = Vec::new();
    let mut entities = BTreeSet::new();
    let mut declared_domains = BTreeSet::new();
    let mut referenced_domains = BTreeSet::new();
    let mut structures = BTreeSet::new();
    let mut programs = BTreeSet::new();

    for seed in seeds {
        match seed {
            BootstrapSeedManifest::Entity(entity_seed) => {
                let entity = &entity_seed.entity;
                if let Some(parent) = &entity.parent_entity_id {
                    if !entities.contains(parent) {
                        blockers.push(format!(
                            "Entity {} references missing parent entity {}",
                            entity.id, parent
                        ));
                    }
                }
                referenced_domains.insert(entity.governance_domain_id.clone());
                entities.insert(entity.id.clone());
            }
            BootstrapSeedManifest::GovernanceDomain(domain_seed) => {
                for domain in &domain_seed.domains {
                    if !declared_domains.insert(domain.id.clone()) {
                        blockers.push(format!(
                            "Governance domain seed declares duplicate domain {}",
                            domain.id
                        ));
                    }
                    if domain.members.is_empty() {
                        warnings.push(format!(
                            "Governance domain {} has no declared members; live apply will be unsupported until at least one member DID (or {BOOTSTRAP_SUBJECT_DID_PLACEHOLDER}) is provided",
                            domain.id
                        ));
                    }
                }
            }
            BootstrapSeedManifest::Structure(structure_seed) => {
                if !entities.contains(&structure_seed.parent_entity_id) {
                    blockers.push(format!(
                        "Structure seed references missing parent entity {}",
                        structure_seed.parent_entity_id
                    ));
                }
                for structure in &structure_seed.structures {
                    structures.insert(structure.id.clone());
                }
            }
            BootstrapSeedManifest::ActivityProgram(activity_program_seed) => {
                if !entities.contains(&activity_program_seed.parent_entity_id) {
                    blockers.push(format!(
                        "Activity/program seed references missing parent entity {}",
                        activity_program_seed.parent_entity_id
                    ));
                }
                for linked_structure in &activity_program_seed.activity.linked_structures {
                    if !structures.contains(linked_structure) {
                        blockers.push(format!(
                            "Activity {} references missing structure {}",
                            activity_program_seed.activity.id, linked_structure
                        ));
                    }
                }
                if let Some(prior) = &activity_program_seed.program.prior_cycle_program_id {
                    warnings.push(format!(
                        "Program {} references prior cycle {} which is not created by this package seed set",
                        activity_program_seed.program.id, prior
                    ));
                }
                programs.insert(activity_program_seed.program.id.clone());
            }
            BootstrapSeedManifest::MilestoneTemplate(milestone_seed) => {
                if !programs.contains(&milestone_seed.program_id) {
                    blockers.push(format!(
                        "Milestone seed references missing program {}",
                        milestone_seed.program_id
                    ));
                }
            }
            BootstrapSeedManifest::RoleAssignment(role_seed) => {
                for assignment in &role_seed.assignments {
                    if !structures.contains(&assignment.structure_id) {
                        blockers.push(format!(
                            "Role assignment references missing structure {}",
                            assignment.structure_id
                        ));
                    }
                    if assignment.person_did.is_none() {
                        warnings.push(format!(
                            "Role assignment for structure {} and role {} has no DID yet; it will be deferred",
                            assignment.structure_id, assignment.role
                        ));
                    }
                }
            }
        }
    }

    for domain_id in &referenced_domains {
        if !declared_domains.contains(domain_id) {
            blockers.push(format!(
                "Entity references governance domain {} but no governance-domain-seed declares it",
                domain_id
            ));
        }
    }

    for domain_id in &declared_domains {
        if !referenced_domains.contains(domain_id) {
            warnings.push(format!(
                "Governance domain {} is declared but not referenced by any entity seed",
                domain_id
            ));
        }
    }

    (warnings, blockers)
}

fn make_plan(loaded: &LoadedPackage) -> BootstrapPlan {
    let mut operations = vec![BootstrapOperation::ValidateCharter {
        path: loaded.charter_path.display().to_string(),
    }];

    let mut entity_domains = BTreeMap::new();
    let mut declared_domains = BTreeMap::new();
    let mut provisioned_domains = BTreeSet::new();

    for seed in &loaded.seeds {
        match seed {
            BootstrapSeedManifest::Entity(entity_seed) => {
                let entity = &entity_seed.entity;
                entity_domains.insert(entity.id.clone(), entity.governance_domain_id.clone());
                operations.push(BootstrapOperation::CreateEntity {
                    id: entity.id.clone(),
                    entity_type: entity.entity_type,
                    parent_entity_id: entity.parent_entity_id.clone(),
                    display_name: entity.display_name.clone(),
                    governance_domain_id: entity.governance_domain_id.clone(),
                    charter_ref: entity.charter_ref.clone(),
                });
            }
            BootstrapSeedManifest::GovernanceDomain(domain_seed) => {
                for domain in &domain_seed.domains {
                    declared_domains.insert(domain.id.clone(), domain.clone());
                    operations.push(BootstrapOperation::ProvisionGovernanceDomain {
                        id: domain.id.clone(),
                        name: domain.name.clone(),
                        profile: domain.profile.clone(),
                        quorum_percent: domain.quorum_percent,
                        approval_percent: domain.approval_percent,
                        voting_period_days: domain.voting_period_days,
                        members: domain.members.clone(),
                        decision_mode: domain.decision_mode.clone(),
                        max_objections: domain.max_objections,
                    });
                    provisioned_domains.insert(domain.id.clone());
                }
            }
            BootstrapSeedManifest::Structure(structure_seed) => {
                for structure in &structure_seed.structures {
                    operations.push(BootstrapOperation::CreateStructure {
                        id: structure.id.clone(),
                        parent_entity_id: structure_seed.parent_entity_id.clone(),
                        kind: structure.kind,
                        name: structure.name.clone(),
                        mandate: structure.mandate.clone(),
                        scope: structure.scope.clone(),
                    });
                }
            }
            BootstrapSeedManifest::ActivityProgram(activity_program_seed) => {
                let program = &activity_program_seed.program;
                let domain_id = entity_domains
                    .get(&activity_program_seed.parent_entity_id)
                    .cloned()
                    .unwrap_or_else(|| activity_program_seed.parent_entity_id.clone());
                if !provisioned_domains.contains(&domain_id) {
                    if let Some(domain) = declared_domains.get(&domain_id) {
                        operations.push(BootstrapOperation::ProvisionGovernanceDomain {
                            id: domain.id.clone(),
                            name: domain.name.clone(),
                            profile: domain.profile.clone(),
                            quorum_percent: domain.quorum_percent,
                            approval_percent: domain.approval_percent,
                            voting_period_days: domain.voting_period_days,
                            members: domain.members.clone(),
                            decision_mode: domain.decision_mode.clone(),
                            max_objections: domain.max_objections,
                        });
                        provisioned_domains.insert(domain_id.clone());
                    }
                }
                operations.push(BootstrapOperation::CreateProgram {
                    id: program.id.clone(),
                    domain_id,
                    parent_entity_id: activity_program_seed.parent_entity_id.clone(),
                    kind: program.kind.to_program_kind(),
                    name: program.name.clone(),
                    description: program.description.clone(),
                    prior_cycle_program_id: program.prior_cycle_program_id.clone(),
                });
                operations.push(BootstrapOperation::CreateActivity {
                    id: activity_program_seed.activity.id.clone(),
                    parent_entity_id: activity_program_seed.parent_entity_id.clone(),
                    kind: activity_program_seed.activity.kind,
                    name: activity_program_seed.activity.name.clone(),
                    description: activity_program_seed.activity.description.clone(),
                    linked_structures: activity_program_seed.activity.linked_structures.clone(),
                    parent_program_id: Some(program.id.clone()),
                });
            }
            BootstrapSeedManifest::MilestoneTemplate(milestone_seed) => {
                for milestone in &milestone_seed.milestones {
                    operations.push(BootstrapOperation::CreateMilestone {
                        id: milestone.id.clone(),
                        program_id: milestone_seed.program_id.clone(),
                        title: milestone.title.clone(),
                        description: milestone.description.clone(),
                        phase_index: milestone.phase_index,
                        target_date: milestone.target_date,
                        completion_criteria: milestone.completion_criteria.clone(),
                    });
                }
            }
            BootstrapSeedManifest::RoleAssignment(role_seed) => {
                for assignment in &role_seed.assignments {
                    match &assignment.person_did {
                        Some(did) => operations.push(BootstrapOperation::AssignRole {
                            structure_id: assignment.structure_id.clone(),
                            person_did: did.clone(),
                            role: assignment.role.clone(),
                            authority_scope: assignment.authority_scope.clone(),
                        }),
                        None => operations.push(BootstrapOperation::DeferRoleAssignment {
                            structure_id: assignment.structure_id.clone(),
                            role: assignment.role.clone(),
                            holder_label: assignment.holder_label.clone(),
                            authority_scope: assignment.authority_scope.clone(),
                            reason: "no person_did supplied in seed".to_string(),
                        }),
                    }
                }
            }
        }
    }

    BootstrapPlan {
        package_id: loaded.manifest.package_id.clone(),
        charter_path: loaded.charter_path.display().to_string(),
        operations,
        warnings: loaded.warnings.clone(),
        blockers: loaded.blockers.clone(),
    }
}

fn preview_execute(plan: &BootstrapPlan) -> BootstrapPreview {
    let mut preview = BootstrapPreview {
        live_apply_supported: true,
        ..BootstrapPreview::default()
    };

    for op in &plan.operations {
        match op {
            BootstrapOperation::ValidateCharter { .. } => {}
            BootstrapOperation::CreateEntity { .. } => preview.entity_count += 1,
            BootstrapOperation::ProvisionGovernanceDomain { .. } => {
                preview.governance_domain_count += 1
            }
            BootstrapOperation::CreateStructure { .. } => preview.structure_count += 1,
            BootstrapOperation::CreateProgram { .. } => preview.program_count += 1,
            BootstrapOperation::CreateActivity { .. } => preview.activity_count += 1,
            BootstrapOperation::CreateMilestone { .. } => preview.milestone_count += 1,
            BootstrapOperation::AssignRole { .. } => preview.role_assignment_count += 1,
            BootstrapOperation::DeferRoleAssignment { .. } => {
                preview.deferred_role_assignment_count += 1
            }
        }
    }

    preview
}

fn entity_id_from_seed(slug: &str, entity_type: BootstrapEntityType) -> String {
    let type_slug = match entity_type {
        BootstrapEntityType::Federation => "federation",
        BootstrapEntityType::Cooperative => "cooperative",
        BootstrapEntityType::Community => "community",
        BootstrapEntityType::Individual => "individual",
    };
    format!("entity:icn:{type_slug}:{slug}")
}

async fn apply_operation(
    op: &BootstrapOperation,
    gateway: &str,
    client: &reqwest::Client,
    auth: &BootstrapAuthContext,
    state: &mut ApplyState,
) -> ApplyOutcome {
    match op {
        BootstrapOperation::ValidateCharter { path } => ApplyOutcome::Completed(format!(
            "validated charter locally at {path}; live charter registration remains unsupported"
        )),
        BootstrapOperation::CreateEntity {
            id,
            entity_type,
            parent_entity_id,
            display_name,
            governance_domain_id,
            charter_ref,
        } => {
            let parent_id = parent_entity_id
                .as_deref()
                .map(|parent| state.resolve(parent));
            let body = json!({
                "entity_type": entity_type_api_name(*entity_type),
                "identifier": id,
                "name": display_name,
                "description": charter_ref
                    .as_ref()
                    .map(|reference| format!("package charter ref: {reference}")),
                "parent_id": parent_id,
            });
            match post_json(client, gateway, &auth.token, "/v1/entities", &body).await {
                Ok(resp) => match required_string_field(&resp, "id") {
                    Ok(live_id) => {
                        state.bind(id, &live_id);
                        ApplyOutcome::Completed(format!(
                            "created entity as {live_id}; governance domain alias {governance_domain_id} recorded for subsequent provisioning"
                        ))
                    }
                    Err(err) => ApplyOutcome::Failed(err.to_string()),
                },
                Err(ref err) if err.contains("HTTP 409") => {
                    // Entity already exists from a prior apply run — bind alias deterministically.
                    let live_id = entity_id_from_seed(id, *entity_type);
                    state.bind(id, &live_id);
                    ApplyOutcome::Completed(format!("entity {live_id} already exists"))
                }
                Err(err) => ApplyOutcome::Failed(err),
            }
        }
        BootstrapOperation::ProvisionGovernanceDomain {
            id,
            name,
            profile,
            quorum_percent,
            approval_percent,
            voting_period_days,
            members,
            decision_mode,
            max_objections,
        } => {
            let resolved_members = resolve_bootstrap_member_dids(members, &auth.subject_did);
            if resolved_members.is_empty() {
                return ApplyOutcome::Unsupported(format!(
                    "governance domain {id} has no usable members after resolving placeholders; provide at least one DID or {BOOTSTRAP_SUBJECT_DID_PLACEHOLDER}"
                ));
            }
            match remote_exists(
                client,
                gateway,
                &auth.token,
                &format!("/v1/gov/domains/{id}"),
            )
            .await
            {
                Ok(true) => {
                    ApplyOutcome::Completed(format!("governance domain {id} already exists"))
                }
                Ok(false) => {
                    let body = json!({
                        "id": id,
                        "name": name,
                        "profile": profile,
                        "quorum_percent": quorum_percent,
                        "approval_percent": approval_percent,
                        "voting_period_days": voting_period_days,
                        "members": resolved_members,
                        "decision_mode": decision_mode,
                        "max_objections": max_objections,
                    });
                    match post_json(client, gateway, &auth.token, "/v1/gov/domains", &body).await {
                        Ok(resp) => match required_string_field(&resp, "id") {
                            Ok(live_id) => ApplyOutcome::Completed(format!(
                                "provisioned governance domain as {live_id}"
                            )),
                            Err(err) => ApplyOutcome::Failed(err.to_string()),
                        },
                        Err(err) => ApplyOutcome::Failed(err),
                    }
                }
                Err(err) => ApplyOutcome::Failed(err),
            }
        }
        BootstrapOperation::CreateStructure {
            id,
            parent_entity_id,
            kind,
            name,
            mandate,
            scope,
        } => {
            if !scope.is_empty() {
                return ApplyOutcome::Unsupported(format!(
                    "structure scope is not writable through the current generic create_structure API for alias {id}"
                ));
            }
            let parent = state.resolve(parent_entity_id);
            let path = format!("/v1/gov/entities/{parent}/structures");
            let body = json!({
                "kind": structure_kind_api_name(*kind),
                "name": name,
                "description": mandate,
            });
            match post_json(client, gateway, &auth.token, &path, &body).await {
                Ok(resp) => match required_string_field(&resp, "id") {
                    Ok(live_id) => {
                        state.bind(id, &live_id);
                        ApplyOutcome::Completed(format!("created structure as {live_id}"))
                    }
                    Err(err) => ApplyOutcome::Failed(err.to_string()),
                },
                Err(ref err) if err.contains("HTTP 409") => {
                    ApplyOutcome::Completed(format!("structure {id} already exists"))
                }
                Err(err) => ApplyOutcome::Failed(err),
            }
        }
        BootstrapOperation::CreateProgram {
            id,
            domain_id,
            parent_entity_id,
            kind,
            name,
            description,
            prior_cycle_program_id,
        } => {
            if let Some(prior) = prior_cycle_program_id {
                return ApplyOutcome::Unsupported(format!(
                    "prior cycle linkage {prior} is not writable through the current generic create_program API for alias {id}"
                ));
            }
            match remote_exists(
                client,
                gateway,
                &auth.token,
                &format!("/v1/gov/domains/{domain_id}"),
            )
            .await
            {
                Ok(true) => {}
                Ok(false) => {
                    return ApplyOutcome::Unsupported(format!(
                        "governance domain {domain_id} is still missing after domain-provisioning steps; verify governance-domain seeds and apply permissions"
                    ));
                }
                Err(err) => return ApplyOutcome::Failed(err),
            }
            let parent_entity = state.resolve(parent_entity_id);
            let path = format!("/v1/gov/domains/{domain_id}/programs");
            let body = json!({
                "parent_entity_id": parent_entity,
                "kind": program_kind_api_name(kind),
                "name": name,
                "description": description,
                "start_at": serde_json::Value::Null,
                "end_at": serde_json::Value::Null,
                "created_by_decision": serde_json::Value::Null,
            });
            match post_json(client, gateway, &auth.token, &path, &body).await {
                Ok(resp) => match required_string_field(&resp, "id") {
                    Ok(live_id) => {
                        state.bind(id, &live_id);
                        ApplyOutcome::Completed(format!("created program as {live_id}"))
                    }
                    Err(err) => ApplyOutcome::Failed(err.to_string()),
                },
                Err(ref err) if err.contains("HTTP 409") => {
                    ApplyOutcome::Completed(format!("program {id} already exists"))
                }
                Err(err) => ApplyOutcome::Failed(err),
            }
        }
        BootstrapOperation::CreateActivity {
            id,
            parent_entity_id,
            kind,
            name,
            description,
            linked_structures,
            parent_program_id,
        } => {
            let resolved_parent_program_id = match parent_program_id {
                Some(program_id) => match resolve_live_or_existing_id(
                    client,
                    gateway,
                    &auth.token,
                    state,
                    program_id,
                    "/v1/gov/programs",
                )
                .await
                {
                    Ok(Some(resolved)) => Some(resolved),
                    Ok(None) => {
                        return ApplyOutcome::Unsupported(format!(
                            "parent program {program_id} is not available as a live program ID for activity alias {id}"
                        ));
                    }
                    Err(err) => return ApplyOutcome::Failed(err),
                },
                None => None,
            };
            let parent_entity = state.resolve(parent_entity_id);
            let path = format!("/v1/gov/entities/{parent_entity}/activities");
            let resolved_linked_structures: Vec<String> = linked_structures
                .iter()
                .map(|structure_id| state.resolve(structure_id))
                .collect();
            let body = json!({
                "kind": activity_kind_api_name(*kind),
                "name": name,
                "description": description,
                "start_date": serde_json::Value::Null,
                "end_date": serde_json::Value::Null,
                "linked_structures": resolved_linked_structures,
                "parent_program_id": resolved_parent_program_id,
            });
            match post_json(client, gateway, &auth.token, &path, &body).await {
                Ok(resp) => match required_string_field(&resp, "id") {
                    Ok(live_id) => {
                        state.bind(id, &live_id);
                        ApplyOutcome::Completed(format!("created activity as {live_id}"))
                    }
                    Err(err) => ApplyOutcome::Failed(err.to_string()),
                },
                Err(ref err) if err.contains("HTTP 409") => {
                    ApplyOutcome::Completed(format!("activity {id} already exists"))
                }
                Err(err) => ApplyOutcome::Failed(err),
            }
        }
        BootstrapOperation::CreateMilestone {
            id,
            program_id,
            title,
            description,
            phase_index,
            target_date,
            completion_criteria,
        } => {
            let resolved_program_id = match resolve_live_or_existing_id(
                client,
                gateway,
                &auth.token,
                state,
                program_id,
                "/v1/gov/programs",
            )
            .await
            {
                Ok(Some(resolved)) => resolved,
                Ok(None) => {
                    return ApplyOutcome::Unsupported(format!(
                        "target program {program_id} is not available as a live program ID for milestone alias {id}"
                    ));
                }
                Err(err) => return ApplyOutcome::Failed(err),
            };
            let path = format!("/v1/gov/programs/{resolved_program_id}/milestones");
            let body = json!({
                "name": title,
                "description": description,
                "phase_index": phase_index,
                "target_date": target_date,
                "completion_criteria": completion_criteria,
            });
            match post_json(client, gateway, &auth.token, &path, &body).await {
                Ok(resp) => match required_string_field(&resp, "id") {
                    Ok(live_id) => {
                        state.bind(id, &live_id);
                        ApplyOutcome::Completed(format!("created milestone as {live_id}"))
                    }
                    Err(err) => ApplyOutcome::Failed(err.to_string()),
                },
                Err(ref err) if err.contains("HTTP 409") => {
                    ApplyOutcome::Completed(format!("milestone {id} already exists"))
                }
                Err(err) => ApplyOutcome::Failed(err),
            }
        }
        BootstrapOperation::AssignRole {
            structure_id,
            person_did,
            role,
            ..
        } => {
            let resolved_structure_id = match resolve_live_or_existing_id(
                client,
                gateway,
                &auth.token,
                state,
                structure_id,
                "/v1/gov/structures",
            )
            .await
            {
                Ok(Some(resolved)) => resolved,
                Ok(None) => {
                    return ApplyOutcome::Unsupported(format!(
                        "target structure {structure_id} is not available as a live structure ID"
                    ));
                }
                Err(err) => return ApplyOutcome::Failed(err),
            };
            let path = format!("/v1/gov/structures/{resolved_structure_id}/roles");
            let body = json!({
                "did": person_did.to_string(),
                "role": role,
            });
            match post_json(client, gateway, &auth.token, &path, &body).await {
                Ok(_) => ApplyOutcome::Completed(format!(
                    "assigned role {role} on live structure {resolved_structure_id}"
                )),
                Err(ref err) if err.contains("HTTP 409") => ApplyOutcome::Completed(format!(
                    "role {role} on {resolved_structure_id} already assigned"
                )),
                Err(err) => ApplyOutcome::Failed(err),
            }
        }
        BootstrapOperation::DeferRoleAssignment {
            reason,
            holder_label,
            ..
        } => ApplyOutcome::Deferred(match holder_label {
            Some(label) => format!("{reason}; awaiting DID for {label}"),
            None => reason.clone(),
        }),
    }
}

async fn get_bootstrap_gateway_token(
    gateway: &str,
    coop_id: &str,
    data_dir: &Path,
) -> Result<BootstrapAuthContext> {
    use icn_identity::{AgeKeyStore, KeyStore};

    let keystore_path = get_keystore_path(data_dir);
    if !keystore_path.exists() {
        bail!("No identity found. Run 'icnctl id init' first.");
    }

    let passphrase = read_passphrase("Keystore passphrase: ")?;
    let mut keystore = AgeKeyStore::open(&keystore_path)?;
    keystore.unlock(&passphrase)?;
    let keypair = keystore.get_keypair()?;
    let did = keypair.did().to_string();

    let client = reqwest::Client::new();
    let challenge_url = format!("{gateway}/v1/auth/challenge");
    let challenge_req = json!({ "did": did });
    let challenge_resp = client
        .post(&challenge_url)
        .json(&challenge_req)
        .send()
        .await
        .context("Failed to connect to gateway for auth challenge")?;

    if !challenge_resp.status().is_success() {
        bail!("Failed to get auth challenge: {}", challenge_resp.status());
    }

    let challenge_data: serde_json::Value = challenge_resp
        .json()
        .await
        .context("Failed to parse auth challenge response")?;
    let nonce = challenge_data["nonce"]
        .as_str()
        .context("Missing nonce in auth challenge response")?;
    let nonce_bytes = hex::decode(nonce).context("Invalid nonce format from gateway")?;
    let signature = keypair.sign(&nonce_bytes);

    let verify_url = format!("{gateway}/v1/auth/verify");
    let verify_req = json!({
        "did": did,
        "signature": hex::encode(signature.to_bytes()),
        "coop_id": coop_id,
        "scopes": ["entity:write", "governance:read", "governance:write"],
    });
    let verify_resp = client
        .post(&verify_url)
        .json(&verify_req)
        .send()
        .await
        .context("Failed to verify auth challenge")?;

    if !verify_resp.status().is_success() {
        bail!(
            "Failed to get bootstrap auth token: {}",
            verify_resp.status()
        );
    }

    let token_data: serde_json::Value = verify_resp
        .json()
        .await
        .context("Failed to parse auth token response")?;
    let token = token_data["token"]
        .as_str()
        .context("Missing token in auth response")?;

    Ok(BootstrapAuthContext {
        token: token.to_string(),
        subject_did: did,
    })
}

fn resolve_bootstrap_member_dids(members: &[String], bootstrap_subject_did: &str) -> Vec<String> {
    members
        .iter()
        .map(|member| {
            if member == BOOTSTRAP_SUBJECT_DID_PLACEHOLDER {
                bootstrap_subject_did.to_string()
            } else {
                member.clone()
            }
        })
        .filter(|member| !member.trim().is_empty())
        .collect()
}

async fn post_json(
    client: &reqwest::Client,
    gateway: &str,
    token: &str,
    path: &str,
    body: &serde_json::Value,
) -> std::result::Result<serde_json::Value, String> {
    let url = format!("{gateway}{path}");
    match client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .json(body)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                resp.json::<serde_json::Value>()
                    .await
                    .map_err(|err| format!("Failed to parse success response from {path}: {err}"))
            } else {
                let body_text = resp.text().await.unwrap_or_default();
                Err(format_http_error(path, status, &body_text))
            }
        }
        Err(err) => {
            print_gateway_error(gateway, &err);
            Err(format!("Failed to call {path}: {err}"))
        }
    }
}

async fn remote_exists(
    client: &reqwest::Client,
    gateway: &str,
    token: &str,
    path: &str,
) -> std::result::Result<bool, String> {
    let url = format!("{gateway}{path}");
    match client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            if status == reqwest::StatusCode::NOT_FOUND {
                Ok(false)
            } else if status.is_success() {
                Ok(true)
            } else {
                let body_text = resp.text().await.unwrap_or_default();
                Err(format_http_error(path, status, &body_text))
            }
        }
        Err(err) => {
            print_gateway_error(gateway, &err);
            Err(format!("Failed to call {path}: {err}"))
        }
    }
}

async fn resolve_live_or_existing_id(
    client: &reqwest::Client,
    gateway: &str,
    token: &str,
    state: &ApplyState,
    alias_or_id: &str,
    resource_prefix: &str,
) -> std::result::Result<Option<String>, String> {
    if let Some(bound) = state.resolve_bound(alias_or_id) {
        return Ok(Some(bound));
    }
    let candidate_path = format!("{resource_prefix}/{alias_or_id}");
    if remote_exists(client, gateway, token, &candidate_path).await? {
        Ok(Some(alias_or_id.to_string()))
    } else {
        Ok(None)
    }
}

fn required_string_field(value: &serde_json::Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .with_context(|| format!("Missing string field '{field}' in response: {value}"))
}

fn entity_type_api_name(entity_type: BootstrapEntityType) -> &'static str {
    match entity_type {
        BootstrapEntityType::Federation => "federation",
        BootstrapEntityType::Cooperative => "cooperative",
        BootstrapEntityType::Community => "community",
        BootstrapEntityType::Individual => "individual",
    }
}

fn structure_kind_api_name(kind: StructureKind) -> &'static str {
    match kind {
        StructureKind::Committee => "committee",
        StructureKind::WorkingGroup => "working_group",
        StructureKind::Team => "team",
        StructureKind::Office => "office",
    }
}

fn activity_kind_api_name(kind: ActivityKind) -> &'static str {
    match kind {
        ActivityKind::Event => "event",
        ActivityKind::Program => "program",
        ActivityKind::Project => "project",
        ActivityKind::Initiative => "initiative",
    }
}

fn program_kind_api_name(kind: &ProgramKind) -> String {
    match kind {
        ProgramKind::Cycle => "cycle".to_string(),
        ProgramKind::Campaign => "campaign".to_string(),
        ProgramKind::Initiative => "initiative".to_string(),
        ProgramKind::Series => "series".to_string(),
        ProgramKind::Custom(name) => name.clone(),
    }
}

fn format_http_error(path: &str, status: reqwest::StatusCode, body: &str) -> String {
    print_http_error(path, status, body);
    if let Ok(json_body) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(message) = json_body.get("message").or_else(|| json_body.get("error")) {
            if let Some(text) = message.as_str() {
                return format!("HTTP {status} on {path}: {text}");
            }
        }
    }
    if body.is_empty() {
        format!("HTTP {status} on {path}")
    } else {
        format!("HTTP {status} on {path}: {body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nycn_package_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join("institutions/nycn")
    }

    #[test]
    fn validates_nycn_package() {
        let report = match validate_package(&nycn_package_dir()) {
            Ok(report) => report,
            Err(err) => panic!("package should validate: {err}"),
        };
        assert!(report.charter_valid);
        assert_eq!(report.seed_count, 7);
    }

    #[test]
    fn builds_nycn_plan_with_expected_core_ops() {
        let report = match build_plan_report(&nycn_package_dir()) {
            Ok(report) => report,
            Err(err) => panic!("plan should build: {err}"),
        };
        assert!(report.plan.operations.len() >= 10);
        assert_eq!(report.preview.entity_count, 2);
        assert_eq!(report.preview.governance_domain_count, 2);
        assert_eq!(report.preview.structure_count, 7);
        assert_eq!(report.preview.program_count, 1);
        assert_eq!(report.preview.activity_count, 1);
        assert_eq!(report.preview.milestone_count, 4);
        assert_eq!(report.preview.deferred_role_assignment_count, 4);
        assert!(report.preview.live_apply_supported);
    }

    #[test]
    fn validate_seed_graph_blocks_missing_governance_domain_declaration() {
        let seeds = vec![BootstrapSeedManifest::Entity(
            icn_governance::EntitySeedManifest {
                seed_version: "draft-0".to_string(),
                entity: icn_governance::BootstrapEntityRecord {
                    id: "entity-a".to_string(),
                    entity_type: BootstrapEntityType::Cooperative,
                    parent_entity_id: None,
                    display_name: "Entity A".to_string(),
                    governance_domain_id: "domain-a".to_string(),
                    charter_ref: None,
                    operating_purpose: None,
                },
                notes: vec![],
            },
        )];
        let (_, blockers) = validate_seed_graph(&seeds);
        assert!(blockers
            .iter()
            .any(|b| b.contains("no governance-domain-seed declares it")));
    }

    #[test]
    fn plan_orders_domain_provisioning_before_program_creation() {
        let report = match build_plan_report(&nycn_package_dir()) {
            Ok(report) => report,
            Err(err) => panic!("plan should build: {err}"),
        };
        let domain_pos = report
            .plan
            .operations
            .iter()
            .position(|op| matches!(op, BootstrapOperation::ProvisionGovernanceDomain { id, .. } if id == "nycn-organizers-gov"))
            .unwrap_or_else(|| panic!("domain provision step should exist"));
        let program_pos = report
            .plan
            .operations
            .iter()
            .position(|op| matches!(op, BootstrapOperation::CreateProgram { id, .. } if id == "summit-cycle-2026"))
            .unwrap_or_else(|| panic!("program create step should exist"));
        assert!(domain_pos < program_pos);
    }

    #[test]
    fn program_kind_custom_maps_to_custom_string() {
        let kind = ProgramKind::Custom("annual-summit".to_string());
        assert_eq!(program_kind_api_name(&kind), "annual-summit");
    }

    #[tokio::test]
    async fn activity_with_linked_structures_attempts_live_write() {
        let op = BootstrapOperation::CreateActivity {
            id: "act-a".to_string(),
            parent_entity_id: "entity-a".to_string(),
            kind: ActivityKind::Event,
            name: "Summit".to_string(),
            description: None,
            linked_structures: vec!["committee-a".to_string()],
            parent_program_id: Some("program-a".to_string()),
        };
        let mut state = ApplyState::default();
        state.bind("program-a", "program-live");
        state.bind("committee-a", "structure-live");
        let outcome = apply_operation(
            &op,
            "http://localhost:8080",
            &reqwest::Client::new(),
            &BootstrapAuthContext {
                token: "token".to_string(),
                subject_did: "did:icn:testsubject".to_string(),
            },
            &mut state,
        )
        .await;
        match outcome {
            ApplyOutcome::Failed(reason) => {
                assert!(reason.contains("/v1/gov/entities/entity-a/activities"));
            }
            _ => panic!("activity should attempt a live write when prerequisites resolve"),
        }
    }

    #[tokio::test]
    async fn governance_domain_without_members_is_unsupported() {
        let op = BootstrapOperation::ProvisionGovernanceDomain {
            id: "domain-a".to_string(),
            name: "Domain A".to_string(),
            profile: "cooperative_default".to_string(),
            quorum_percent: 50,
            approval_percent: 50,
            voting_period_days: 7,
            members: vec![],
            decision_mode: None,
            max_objections: None,
        };
        let outcome = apply_operation(
            &op,
            "http://localhost:8080",
            &reqwest::Client::new(),
            &BootstrapAuthContext {
                token: "token".to_string(),
                subject_did: "did:icn:testsubject".to_string(),
            },
            &mut ApplyState::default(),
        )
        .await;
        match outcome {
            ApplyOutcome::Unsupported(reason) => {
                assert!(reason.contains("no usable members"));
            }
            _ => panic!("domain provisioning should be unsupported without members"),
        }
    }

    #[test]
    fn apply_state_resolves_alias_bindings() {
        let mut state = ApplyState::default();
        state.bind("program-a", "prog-live-1");
        assert_eq!(state.resolve("program-a"), "prog-live-1");
        assert_eq!(state.resolve("already-live"), "already-live");
    }

    #[test]
    fn resolves_bootstrap_subject_member_placeholder() {
        let members = vec![
            BOOTSTRAP_SUBJECT_DID_PLACEHOLDER.to_string(),
            "did:icn:static-member".to_string(),
        ];
        let resolved = resolve_bootstrap_member_dids(&members, "did:icn:bootstrap-member");
        assert_eq!(
            resolved,
            vec![
                "did:icn:bootstrap-member".to_string(),
                "did:icn:static-member".to_string()
            ]
        );
    }

    // ── Integration tests: full NYCN bootstrap apply round-trip ─────────────
    //
    // These tests start a real TCP server (actix-web on 127.0.0.1:0) wired with
    // the entity and governance HTTP handlers, obtain a fixture JWT token via the
    // challenge/verify auth flow, then drive apply_with_auth_ctx against that URL.
    //
    // They prove that the complete NYCN seed sequence applies successfully and
    // that the expected completed/deferred/unsupported counts are stable.

    #[cfg(test)]
    #[allow(clippy::expect_used)]
    mod apply_integration {
        use super::*;

        use actix_web::{web, App, HttpServer};
        use actix_web_httpauth::middleware::HttpAuthentication;
        use icn_gateway::{
            api, auth::AuthManager, entity_audit::EntityAuditManager, entity_mgr::EntityManager,
            middleware::jwt_auth, rate_limit::IpRateLimiter,
        };
        use icn_governance_actor::{
            events::NoopEventEmitter,
            http::configure::{configure as configure_governance, GovernanceContext},
            manager::GovernanceManager,
        };
        use icn_identity::IdentityBundle;
        use icn_store::SledStore;
        use std::{net::TcpListener, sync::Arc};

        /// Stand up a real HTTP server and return the port + a ready-to-use auth context.
        async fn start_test_server() -> (tokio::task::JoinHandle<()>, String, BootstrapAuthContext)
        {
            let jwt_secret = b"bootstrap-apply-integration-test!".to_vec();
            let auth_mgr = Arc::new(AuthManager::new(jwt_secret));
            let ip_limiter = Arc::new(IpRateLimiter::new_for_auth());
            let entity_mgr = Arc::new(EntityManager::new());
            let store = Arc::new(SledStore::temporary().expect("temp sled store"));
            let audit_mgr = Arc::new(EntityAuditManager::new(store));
            let gov_mgr = Arc::new(GovernanceManager::new());

            let gov_ctx = GovernanceContext {
                manager: gov_mgr.clone(),
                emitter: NoopEventEmitter,
                on_charter_accepted: None,
                on_proposal_accepted: None,
                on_proposal_accepted_with_evidence: None,
                member_checker: None,
                steward_checker: None,
                suspension_checker: None,
                membership_resolver: None,
                sdis_service: None,
            };

            let auth_mgr2 = auth_mgr.clone();
            let ip2 = ip_limiter.clone();
            let entity2 = entity_mgr.clone();
            let audit2 = audit_mgr.clone();
            let gov_ctx2 = gov_ctx.clone();

            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
            let port = listener.local_addr().expect("local addr").port();

            let server = HttpServer::new(move || {
                let auth_mw = HttpAuthentication::bearer(jwt_auth);
                App::new()
                    .app_data(web::Data::new(auth_mgr2.clone()))
                    .app_data(web::Data::new(ip2.clone()))
                    .app_data(web::Data::new(entity2.clone()))
                    .app_data(web::Data::new(audit2.clone()))
                    .service(
                        web::scope("/v1")
                            .service(api::auth::challenge)
                            .service(api::auth::verify)
                            .service(
                                web::scope("/entities")
                                    .configure(api::entity::configure)
                                    .wrap(auth_mw.clone()),
                            )
                            .service(
                                web::scope("/gov")
                                    .configure({
                                        let ctx = gov_ctx2.clone();
                                        move |cfg| configure_governance(cfg, ctx.clone())
                                    })
                                    .wrap(auth_mw),
                            ),
                    )
            })
            .listen(listener)
            .expect("server listen")
            .run();

            let handle = tokio::spawn(async move {
                let _ = server.await;
            });

            let base_url = format!("http://127.0.0.1:{port}");
            let bundle = IdentityBundle::generate().expect("IdentityBundle");
            let did = bundle.did().to_string();

            let client = reqwest::Client::new();

            let challenge: serde_json::Value = client
                .post(format!("{base_url}/v1/auth/challenge"))
                .json(&serde_json::json!({ "did": &did }))
                .send()
                .await
                .expect("challenge request")
                .json()
                .await
                .expect("challenge json");
            let nonce_bytes =
                hex::decode(challenge["nonce"].as_str().expect("nonce")).expect("hex nonce");
            let sig = bundle.sign(&nonce_bytes).expect("sign nonce");

            let token_resp: serde_json::Value = client
                .post(format!("{base_url}/v1/auth/verify"))
                .json(&serde_json::json!({
                    "did": &did,
                    "signature": hex::encode(sig.to_bytes()),
                    "coop_id": "test-coop",
                    "scopes": ["entity:write", "governance:read", "governance:write"],
                }))
                .send()
                .await
                .expect("verify request")
                .json()
                .await
                .expect("verify json");

            let token = token_resp["token"]
                .as_str()
                .expect("token field")
                .to_string();

            let auth = BootstrapAuthContext {
                token,
                subject_did: did,
            };

            (handle, base_url, auth)
        }

        /// Full NYCN bootstrap apply: proves the complete seed sequence lands with
        /// the expected completed / deferred / unsupported counts.
        ///
        /// Expected completed (18):
        ///   1  validate-charter
        ///   2  entities (nycn federation + nycn-organizers cooperative)
        ///   2  governance domains (nycn-federation-gov + nycn-organizers-gov)
        ///   7  structures (backbone, steering, content, logistics, marketing,
        ///                  finance, accessibility-wg)
        ///   1  program  (summit-cycle-2026)
        ///   1  activity (summit-2026)
        ///   4  milestones (strategy-locked, venue-ready, launch-ready, closeout)
        ///
        /// Expected deferred (4): role assignments without a real person_did.
        /// Expected unsupported (1): charter store (no live CCL sink).
        #[tokio::test]
        async fn nycn_bootstrap_apply_round_trip() {
            let (handle, base_url, auth) = start_test_server().await;

            let report = apply_with_auth_ctx(&nycn_package_dir(), &base_url, auth)
                .await
                .expect("apply_with_auth_ctx should succeed");

            assert!(
                report.failed.is_none(),
                "no operations should fail; first failure: {:?}",
                report.failed
            );
            assert_eq!(
                report.completed.len(),
                18,
                "expected 18 completed ops; got {:?}",
                report
                    .completed
                    .iter()
                    .map(|s| &s.operation)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                report.deferred.len(),
                4,
                "expected 4 deferred role assignments; got {:?}",
                report
                    .deferred
                    .iter()
                    .map(|s| &s.operation)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                report.unsupported.len(),
                1,
                "expected 1 unsupported op (charter store); got {:?}",
                report
                    .unsupported
                    .iter()
                    .map(|s| &s.operation)
                    .collect::<Vec<_>>()
            );

            // Alias bindings: entities and objects that get live IDs back
            assert!(
                report.alias_bindings.contains_key("nycn"),
                "alias 'nycn' must be bound after CreateEntity"
            );
            assert!(
                report.alias_bindings.contains_key("nycn-organizers"),
                "alias 'nycn-organizers' must be bound after CreateEntity"
            );
            assert!(
                report.alias_bindings.contains_key("summit-cycle-2026"),
                "alias 'summit-cycle-2026' must be bound after CreateProgram"
            );
            assert!(
                report.alias_bindings.contains_key("summit-2026"),
                "alias 'summit-2026' must be bound after CreateActivity"
            );

            handle.abort();
        }

        /// Idempotence: running apply twice against the same server does not fail.
        /// Domains and entities that already exist should not cause a hard failure.
        #[tokio::test]
        async fn nycn_bootstrap_apply_idempotent() {
            let (handle, base_url, auth_first) = start_test_server().await;

            // Re-auth for the second run (token is single-use for request signing,
            // but the JWT itself is valid for multiple calls — we just need a fresh
            // BootstrapAuthContext to hold the same subject DID).
            let auth_second = BootstrapAuthContext {
                token: auth_first.token.clone(),
                subject_did: auth_first.subject_did.clone(),
            };

            let first = apply_with_auth_ctx(&nycn_package_dir(), &base_url, auth_first)
                .await
                .expect("first apply");

            assert!(
                first.failed.is_none(),
                "first apply: no failures expected; got {:?}",
                first.failed
            );

            let second = apply_with_auth_ctx(&nycn_package_dir(), &base_url, auth_second)
                .await
                .expect("second apply");

            assert!(
                second.failed.is_none(),
                "second apply: no failures expected; got {:?}",
                second.failed
            );

            // Role assignments are always deferred (no person_did); count must be stable.
            assert_eq!(
                first.deferred.len(),
                second.deferred.len(),
                "deferred count must be stable across runs"
            );

            handle.abort();
        }

        // ── 409 idempotency unit tests (per operation type) ──────────────────
        //
        // Each test uses a minimal actix-web stub that returns 200 for GET
        // (prerequisite checks) and 409 Conflict for POST (create attempts).
        // State is pre-populated with necessary alias bindings so that
        // resolve_live_or_existing_id short-circuits without extra GET calls.
        //
        // These prove the `err.contains("HTTP 409")` guard added to each
        // create/assign operation turns a conflict into ApplyOutcome::Completed.

        /// Spin up a stub server: GET → 200 `{}`, POST → 409 Conflict.
        async fn start_conflict_stub() -> (tokio::task::JoinHandle<()>, String) {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind stub");
            let port = listener.local_addr().expect("addr").port();
            let server = HttpServer::new(|| {
                App::new().default_service(web::route().to(
                    |req: actix_web::HttpRequest| async move {
                        if req.method() == actix_web::http::Method::POST {
                            actix_web::HttpResponse::Conflict().finish()
                        } else {
                            actix_web::HttpResponse::Ok().json(serde_json::json!({}))
                        }
                    },
                ))
            })
            .listen(listener)
            .expect("listen stub")
            .run();
            let handle = tokio::spawn(async move {
                let _ = server.await;
            });
            (handle, format!("http://127.0.0.1:{port}"))
        }

        /// Stub server: every request returns 500 Internal Server Error.
        async fn start_error_stub() -> (tokio::task::JoinHandle<()>, String) {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind error stub");
            let port = listener.local_addr().expect("addr").port();
            let server = HttpServer::new(|| {
                App::new().default_service(
                    web::route()
                        .to(|| async { actix_web::HttpResponse::InternalServerError().finish() }),
                )
            })
            .listen(listener)
            .expect("listen error stub")
            .run();
            let handle = tokio::spawn(async move {
                let _ = server.await;
            });
            (handle, format!("http://127.0.0.1:{port}"))
        }

        fn test_auth() -> BootstrapAuthContext {
            BootstrapAuthContext {
                token: "test-token".to_string(),
                subject_did: "did:icn:testsubject".to_string(),
            }
        }

        #[tokio::test]
        async fn create_structure_409_is_idempotent() {
            let (handle, base_url) = start_conflict_stub().await;
            let op = BootstrapOperation::CreateStructure {
                id: "test-structure".to_string(),
                parent_entity_id: "parent-entity".to_string(),
                kind: StructureKind::Committee,
                name: "Test Committee".to_string(),
                mandate: None,
                scope: vec![],
            };
            let mut state = ApplyState::default();
            state.bind("parent-entity", "entity-live-id");
            let outcome = apply_operation(
                &op,
                &base_url,
                &reqwest::Client::new(),
                &test_auth(),
                &mut state,
            )
            .await;
            match outcome {
                ApplyOutcome::Completed(_) => {}
                _ => panic!("create_structure 409 must be Completed, not Failed/Unsupported"),
            }
            handle.abort();
        }

        #[tokio::test]
        async fn create_program_409_is_idempotent() {
            let (handle, base_url) = start_conflict_stub().await;
            let op = BootstrapOperation::CreateProgram {
                id: "test-program".to_string(),
                domain_id: "test-domain".to_string(),
                parent_entity_id: "parent-entity".to_string(),
                kind: ProgramKind::Custom("annual".to_string()),
                name: "Test Program".to_string(),
                description: None,
                prior_cycle_program_id: None,
            };
            let mut state = ApplyState::default();
            state.bind("parent-entity", "entity-live-id");
            // The domain check (remote_exists GET) is served by the stub returning 200 {}.
            let outcome = apply_operation(
                &op,
                &base_url,
                &reqwest::Client::new(),
                &test_auth(),
                &mut state,
            )
            .await;
            match outcome {
                ApplyOutcome::Completed(_) => {}
                _ => panic!("create_program 409 must be Completed, not Failed/Unsupported"),
            }
            handle.abort();
        }

        #[tokio::test]
        async fn create_activity_409_is_idempotent() {
            let (handle, base_url) = start_conflict_stub().await;
            let op = BootstrapOperation::CreateActivity {
                id: "test-activity".to_string(),
                parent_entity_id: "parent-entity".to_string(),
                kind: ActivityKind::Event,
                name: "Test Activity".to_string(),
                description: None,
                linked_structures: vec![],
                parent_program_id: Some("parent-program".to_string()),
            };
            let mut state = ApplyState::default();
            state.bind("parent-entity", "entity-live-id");
            state.bind("parent-program", "program-live-id");
            let outcome = apply_operation(
                &op,
                &base_url,
                &reqwest::Client::new(),
                &test_auth(),
                &mut state,
            )
            .await;
            match outcome {
                ApplyOutcome::Completed(_) => {}
                _ => panic!("create_activity 409 must be Completed, not Failed/Unsupported"),
            }
            handle.abort();
        }

        #[tokio::test]
        async fn create_milestone_409_is_idempotent() {
            let (handle, base_url) = start_conflict_stub().await;
            let op = BootstrapOperation::CreateMilestone {
                id: "test-milestone".to_string(),
                program_id: "parent-program".to_string(),
                title: "Test Milestone".to_string(),
                description: None,
                phase_index: 1,
                target_date: None,
                completion_criteria: vec![],
            };
            let mut state = ApplyState::default();
            state.bind("parent-program", "program-live-id");
            let outcome = apply_operation(
                &op,
                &base_url,
                &reqwest::Client::new(),
                &test_auth(),
                &mut state,
            )
            .await;
            match outcome {
                ApplyOutcome::Completed(_) => {}
                _ => panic!("create_milestone 409 must be Completed, not Failed/Unsupported"),
            }
            handle.abort();
        }

        #[tokio::test]
        async fn assign_role_409_is_idempotent() {
            let (handle, base_url) = start_conflict_stub().await;
            let bundle = IdentityBundle::generate().expect("IdentityBundle");
            let op = BootstrapOperation::AssignRole {
                structure_id: "test-structure".to_string(),
                person_did: bundle.did().clone(),
                role: "coordinator".to_string(),
                authority_scope: vec![],
            };
            let mut state = ApplyState::default();
            state.bind("test-structure", "structure-live-id");
            let outcome = apply_operation(
                &op,
                &base_url,
                &reqwest::Client::new(),
                &test_auth(),
                &mut state,
            )
            .await;
            match outcome {
                ApplyOutcome::Completed(_) => {}
                _ => panic!("assign_role 409 must be Completed, not Failed"),
            }
            handle.abort();
        }

        #[tokio::test]
        async fn non_conflict_error_is_not_swallowed() {
            let (handle, base_url) = start_error_stub().await;
            let op = BootstrapOperation::CreateStructure {
                id: "test-structure".to_string(),
                parent_entity_id: "parent-entity".to_string(),
                kind: StructureKind::Committee,
                name: "Test Committee".to_string(),
                mandate: None,
                scope: vec![],
            };
            let mut state = ApplyState::default();
            state.bind("parent-entity", "entity-live-id");
            let outcome = apply_operation(
                &op,
                &base_url,
                &reqwest::Client::new(),
                &test_auth(),
                &mut state,
            )
            .await;
            match outcome {
                ApplyOutcome::Failed(_) => {}
                _ => panic!("HTTP 500 must remain Failed, not be swallowed as Completed"),
            }
            handle.abort();
        }

        /// Plan/apply count parity: the plan preview's counts match the apply report.
        #[test]
        fn nycn_plan_counts_match_apply_expectations() {
            let report = build_plan_report(&nycn_package_dir()).expect("plan report");
            // 18 completed = 1 validate + 2 entities + 2 domains + 7 structures +
            //                1 program + 1 activity + 4 milestones
            let expected_completed = 1  // validate-charter
                + report.preview.entity_count
                + report.preview.governance_domain_count
                + report.preview.structure_count
                + report.preview.program_count
                + report.preview.activity_count
                + report.preview.milestone_count;
            assert_eq!(
                expected_completed, 18,
                "plan preview counts must sum to 18 completed ops"
            );
            assert_eq!(
                report.preview.deferred_role_assignment_count, 4,
                "plan preview must show 4 deferred role assignments"
            );
        }
    }
}
