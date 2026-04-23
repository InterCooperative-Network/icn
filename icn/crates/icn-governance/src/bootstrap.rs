//! Generic institution-package bootstrap manifest types.
//!
//! These types define the package-owned manifest contract for institution
//! bootstrapping without embedding any institution-specific vocabulary into ICN
//! core. They are transport- and filesystem-agnostic: CLI, gateway, tests, or
//! future runtime executors can all consume the same shapes.

use crate::{ActivityKind, ProgramKind, StructureKind};
use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Top-level manifest for an institution package bootstrap entrypoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstitutionBootstrapManifest {
    /// Contract version for the package bootstrap loader.
    pub manifest_version: String,
    /// Stable package identifier, e.g. `nycn`.
    pub package_id: String,
    /// Package kind, currently expected to be `institution`.
    pub package_kind: String,
    /// Package-owned working charter location.
    pub charter: BootstrapCharterRef,
    /// Ordered seed manifests to ingest.
    pub seeds: Vec<BootstrapSeedRef>,
}

/// Pointer to the package-owned charter file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BootstrapCharterRef {
    pub path: String,
}

/// Pointer to a specific ordered seed manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BootstrapSeedRef {
    pub order: u32,
    pub kind: BootstrapSeedKind,
    pub path: String,
}

/// Supported generic seed kinds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum BootstrapSeedKind {
    EntitySeed,
    GovernanceDomainSeed,
    StructureSeed,
    ActivityProgramSeed,
    MilestoneTemplateSeed,
    RoleAssignmentSeed,
}

/// Parsed seed manifest, tagged by the `kind` field in the YAML file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum BootstrapSeedManifest {
    #[serde(rename = "entity-seed")]
    Entity(EntitySeedManifest),
    #[serde(rename = "governance-domain-seed")]
    GovernanceDomain(GovernanceDomainSeedManifest),
    #[serde(rename = "structure-seed")]
    Structure(StructureSeedManifest),
    #[serde(rename = "activity-program-seed")]
    ActivityProgram(ActivityProgramSeedManifest),
    #[serde(rename = "milestone-template-seed")]
    MilestoneTemplate(MilestoneTemplateSeedManifest),
    #[serde(rename = "role-assignment-seed")]
    RoleAssignment(RoleAssignmentSeedManifest),
}

impl BootstrapSeedManifest {
    pub fn seed_kind(&self) -> BootstrapSeedKind {
        match self {
            Self::Entity(_) => BootstrapSeedKind::EntitySeed,
            Self::GovernanceDomain(_) => BootstrapSeedKind::GovernanceDomainSeed,
            Self::Structure(_) => BootstrapSeedKind::StructureSeed,
            Self::ActivityProgram(_) => BootstrapSeedKind::ActivityProgramSeed,
            Self::MilestoneTemplate(_) => BootstrapSeedKind::MilestoneTemplateSeed,
            Self::RoleAssignment(_) => BootstrapSeedKind::RoleAssignmentSeed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntitySeedManifest {
    pub seed_version: String,
    pub entity: BootstrapEntityRecord,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootstrapEntityRecord {
    pub id: String,
    pub entity_type: BootstrapEntityType,
    #[serde(default)]
    pub parent_entity_id: Option<String>,
    pub display_name: String,
    pub governance_domain_id: String,
    #[serde(default)]
    pub charter_ref: Option<String>,
    #[serde(default)]
    pub operating_purpose: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GovernanceDomainSeedManifest {
    pub seed_version: String,
    pub domains: Vec<BootstrapGovernanceDomainRecord>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootstrapGovernanceDomainRecord {
    pub id: String,
    pub name: String,
    pub profile: String,
    pub quorum_percent: u8,
    pub approval_percent: u8,
    pub voting_period_days: u64,
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub decision_mode: Option<String>,
    #[serde(default)]
    pub max_objections: Option<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapEntityType {
    Federation,
    Cooperative,
    Community,
    Individual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructureSeedManifest {
    pub seed_version: String,
    pub parent_entity_id: String,
    pub structures: Vec<BootstrapStructureRecord>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootstrapStructureRecord {
    pub id: String,
    pub kind: StructureKind,
    pub name: String,
    #[serde(default)]
    pub mandate: Option<String>,
    #[serde(default)]
    pub scope: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityProgramSeedManifest {
    pub seed_version: String,
    pub parent_entity_id: String,
    pub activity: BootstrapActivityRecord,
    pub program: BootstrapProgramRecord,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootstrapActivityRecord {
    pub id: String,
    pub kind: ActivityKind,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub linked_structures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootstrapProgramRecord {
    pub id: String,
    pub kind: BootstrapProgramKindSpec,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parent_activity_id: Option<String>,
    #[serde(default)]
    pub prior_cycle_program_id: Option<String>,
    #[serde(default)]
    pub views: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootstrapProgramKindSpec {
    pub kind: BootstrapProgramKindName,
    #[serde(default)]
    pub kind_tag: Option<String>,
}

impl BootstrapProgramKindSpec {
    pub fn to_program_kind(&self) -> ProgramKind {
        match self.kind {
            BootstrapProgramKindName::Cycle => ProgramKind::Cycle,
            BootstrapProgramKindName::Campaign => ProgramKind::Campaign,
            BootstrapProgramKindName::Initiative => ProgramKind::Initiative,
            BootstrapProgramKindName::Series => ProgramKind::Series,
            BootstrapProgramKindName::Custom => ProgramKind::Custom(
                self.kind_tag
                    .clone()
                    .unwrap_or_else(|| "custom".to_string()),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapProgramKindName {
    Cycle,
    Campaign,
    Initiative,
    Series,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MilestoneTemplateSeedManifest {
    pub seed_version: String,
    pub program_id: String,
    pub milestones: Vec<BootstrapMilestoneRecord>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootstrapMilestoneRecord {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub phase_index: u32,
    #[serde(default)]
    pub target_date: Option<u64>,
    #[serde(default)]
    pub completion_criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoleAssignmentSeedManifest {
    pub seed_version: String,
    pub assignments: Vec<BootstrapRoleAssignmentRecord>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootstrapRoleAssignmentRecord {
    pub structure_id: String,
    pub role: String,
    #[serde(default)]
    pub person_did: Option<Did>,
    #[serde(default)]
    pub holder_label: Option<String>,
    #[serde(default)]
    pub authority_scope: Vec<String>,
}

/// Generic operation plan emitted by a package bootstrap loader.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BootstrapPlan {
    pub package_id: String,
    pub charter_path: String,
    pub operations: Vec<BootstrapOperation>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub blockers: Vec<String>,
}

/// Generic bootstrap operations. These are platform-shape operations, not
/// institution-specific behaviors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BootstrapOperation {
    ValidateCharter {
        path: String,
    },
    CreateEntity {
        id: String,
        entity_type: BootstrapEntityType,
        parent_entity_id: Option<String>,
        display_name: String,
        governance_domain_id: String,
        charter_ref: Option<String>,
    },
    ProvisionGovernanceDomain {
        id: String,
        name: String,
        profile: String,
        quorum_percent: u8,
        approval_percent: u8,
        voting_period_days: u64,
        members: Vec<String>,
        decision_mode: Option<String>,
        max_objections: Option<u8>,
    },
    CreateStructure {
        id: String,
        parent_entity_id: String,
        kind: StructureKind,
        name: String,
        mandate: Option<String>,
        scope: Vec<String>,
    },
    CreateProgram {
        id: String,
        domain_id: String,
        parent_entity_id: String,
        kind: ProgramKind,
        name: String,
        description: Option<String>,
        prior_cycle_program_id: Option<String>,
    },
    CreateActivity {
        id: String,
        parent_entity_id: String,
        kind: ActivityKind,
        name: String,
        description: Option<String>,
        linked_structures: Vec<String>,
        parent_program_id: Option<String>,
    },
    CreateMilestone {
        id: String,
        program_id: String,
        title: String,
        description: Option<String>,
        phase_index: u32,
        target_date: Option<u64>,
        completion_criteria: Vec<String>,
    },
    AssignRole {
        structure_id: String,
        person_did: Did,
        role: String,
        authority_scope: Vec<String>,
    },
    DeferRoleAssignment {
        structure_id: String,
        role: String,
        holder_label: Option<String>,
        authority_scope: Vec<String>,
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_program_kind_maps_to_custom_variant() {
        let spec = BootstrapProgramKindSpec {
            kind: BootstrapProgramKindName::Custom,
            kind_tag: Some("annual-summit".to_string()),
        };
        assert_eq!(
            spec.to_program_kind(),
            ProgramKind::Custom("annual-summit".to_string())
        );
    }

    #[test]
    fn seed_enum_reports_kind() {
        let seed = BootstrapSeedManifest::MilestoneTemplate(MilestoneTemplateSeedManifest {
            seed_version: "draft-0".to_string(),
            program_id: "p".to_string(),
            milestones: vec![],
            notes: vec![],
        });
        assert_eq!(seed.seed_kind(), BootstrapSeedKind::MilestoneTemplateSeed);
    }

    #[test]
    fn governance_domain_seed_reports_kind() {
        let seed = BootstrapSeedManifest::GovernanceDomain(GovernanceDomainSeedManifest {
            seed_version: "draft-0".to_string(),
            domains: vec![BootstrapGovernanceDomainRecord {
                id: "domain-a".to_string(),
                name: "Domain A".to_string(),
                profile: "cooperative_default".to_string(),
                quorum_percent: 50,
                approval_percent: 50,
                voting_period_days: 7,
                members: vec!["$bootstrap_subject_did".to_string()],
                decision_mode: None,
                max_objections: None,
            }],
            notes: vec![],
        });
        assert_eq!(seed.seed_kind(), BootstrapSeedKind::GovernanceDomainSeed);
    }
}
