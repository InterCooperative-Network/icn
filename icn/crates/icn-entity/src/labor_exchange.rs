//! Labor exchange types for worker mobility between cooperatives.
//!
//! These types model cooperative labor assignments, labor pools, and credit routing
//! for inter-cooperative work arrangements.

use crate::error::{EntityError, Result};
use serde::{Deserialize, Serialize};

// ============================================================================
// Labor Assignments
// ============================================================================

/// Unique identifier for a labor assignment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssignmentId(pub String);

/// Type of labor assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentType {
    /// Fixed-term project work.
    Project { project_id: String },
    /// Seasonal or cyclical work.
    Seasonal { season: String },
    /// Trial period before potential membership.
    Trial { duration_days: u32 },
    /// Dual membership arrangement.
    DualMembership,
}

/// Assignment status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    /// Proposed, awaiting governance approval.
    Proposed,
    /// Active assignment.
    Active,
    /// Completed normally.
    Completed { completed_at: u64 },
    /// Terminated early.
    Terminated { at: u64, reason: String },
}

/// How credits earned at host flow to worker/home cooperative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingMode {
    /// All credits go through home cooperative, which pays the worker.
    ThroughHomeCoop,
    /// Credits go directly to the worker, home cooperative gets a fee.
    DirectToWorker,
    /// Split between worker (direct) and home cooperative (0-100).
    Split { worker_pct: u8 },
}

/// Credit routing configuration for an assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditRouting {
    /// How credits flow.
    pub routing_mode: RoutingMode,
    /// Home cooperative's coordination fee in basis points (0-10000).
    pub admin_fee_bps: u16,
    /// Whether host contributes to worker's home cooperative shares.
    pub share_contribution: bool,
}

/// Required approvals for a labor assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentApprovals {
    /// Governance proposal ID for the home cooperative.
    pub home_coop_proposal: Option<String>,
    /// Governance proposal ID for the host cooperative.
    pub host_coop_proposal: Option<String>,
}

/// Worker assignment to a host cooperative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaborAssignment {
    /// Unique identifier.
    pub id: AssignmentId,
    /// Worker being assigned (individual entity ID string).
    pub worker: String,
    /// Cooperative holding worker's primary membership.
    pub home_coop: String,
    /// Cooperative where work happens.
    pub host_coop: String,
    /// Type of assignment.
    pub assignment_type: AssignmentType,
    /// Start date (unix timestamp).
    pub start_date: u64,
    /// End date (None for open-ended, must be greater than start date).
    pub end_date: Option<u64>,
    /// Credit routing configuration.
    pub credit_routing: CreditRouting,
    /// Current status.
    pub status: AssignmentStatus,
    /// Governance proposals (from both cooperatives).
    pub approvals: AssignmentApprovals,
}

impl RoutingMode {
    /// Validate routing configuration invariants.
    pub fn validate(&self) -> Result<()> {
        if let RoutingMode::Split { worker_pct } = self {
            if *worker_pct > 100 {
                return Err(EntityError::InvalidFormat(format!(
                    "worker_pct must be between 0 and 100, got {worker_pct}"
                )));
            }
        }
        Ok(())
    }
}

impl CreditRouting {
    /// Validate routing configuration invariants.
    pub fn validate(&self) -> Result<()> {
        self.routing_mode.validate()?;
        if self.admin_fee_bps > 10_000 {
            return Err(EntityError::InvalidFormat(format!(
                "admin_fee_bps must be between 0 and 10000, got {}",
                self.admin_fee_bps
            )));
        }
        Ok(())
    }
}

impl LaborAssignment {
    /// Validate assignment invariants.
    pub fn validate(&self) -> Result<()> {
        self.credit_routing.validate()?;
        if let Some(end_date) = self.end_date {
            if end_date <= self.start_date {
                return Err(EntityError::InvalidFormat(format!(
                    "end_date must be greater than start_date (start_date={}, end_date={})",
                    self.start_date, end_date
                )));
            }
        }
        Ok(())
    }
}

// ============================================================================
// Labor Pool
// ============================================================================

/// Worker registration in a labor pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolRegistration {
    /// Worker entity ID string.
    pub worker: String,
    /// Home cooperative entity ID string.
    pub home_coop: String,
    /// Worker skills.
    pub skills: Vec<String>,
    /// Availability specification.
    pub availability: Availability,
    /// Registration timestamp (unix seconds).
    pub registered_at: u64,
}

/// Availability specification for labor pool matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    /// Ready to start immediately.
    Immediate,
    /// Available from a specific date.
    FromDate(u64),
    /// Seasonal availability.
    Seasonal { seasons: Vec<String> },
    /// Part-time availability by hours per week.
    PartTime { hours_per_week: u32 },
}

/// Open position in a labor pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaborNeed {
    /// Unique identifier.
    pub id: String,
    /// Cooperative requesting the worker.
    pub requesting_coop: String,
    /// Role description.
    pub role: String,
    /// Required skills for the role.
    pub skills_required: Vec<String>,
    /// Optional duration in days.
    pub duration: Option<u32>,
    /// Timestamp of posting.
    pub posted_at: u64,
}

/// Labor pool for federation-wide coordination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaborPool {
    /// Unique identifier.
    pub id: String,
    /// Federation managing this pool.
    pub manager: String,
    /// Available workers.
    pub available_workers: Vec<PoolRegistration>,
    /// Open positions.
    pub open_needs: Vec<LaborNeed>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample_assignment() -> LaborAssignment {
        LaborAssignment {
            id: AssignmentId("assign-001".to_string()),
            worker: "entity:icn:individual:worker-123".to_string(),
            home_coop: "entity:icn:cooperative:home-coop".to_string(),
            host_coop: "entity:icn:cooperative:host-coop".to_string(),
            assignment_type: AssignmentType::Project {
                project_id: "project-xyz".to_string(),
            },
            start_date: 1_700_000_000,
            end_date: Some(1_700_086_400),
            credit_routing: CreditRouting {
                routing_mode: RoutingMode::Split { worker_pct: 80 },
                admin_fee_bps: 150,
                share_contribution: true,
            },
            status: AssignmentStatus::Proposed,
            approvals: AssignmentApprovals {
                home_coop_proposal: Some("proposal-home".to_string()),
                host_coop_proposal: None,
            },
        }
    }

    #[test]
    fn test_assignment_roundtrip() {
        let assignment = sample_assignment();
        let json = serde_json::to_string(&assignment).unwrap();
        let parsed: LaborAssignment = serde_json::from_str(&json).unwrap();
        assert_eq!(assignment, parsed);
    }

    #[test]
    fn test_assignment_type_roundtrip_variants() {
        let variants = vec![
            AssignmentType::Project {
                project_id: "project-xyz".to_string(),
            },
            AssignmentType::Seasonal {
                season: "winter".to_string(),
            },
            AssignmentType::Trial { duration_days: 90 },
            AssignmentType::DualMembership,
        ];

        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: AssignmentType = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, parsed);
        }
    }

    #[test]
    fn test_assignment_status_roundtrip_variants() {
        let variants = vec![
            AssignmentStatus::Proposed,
            AssignmentStatus::Active,
            AssignmentStatus::Completed {
                completed_at: 1_700_000_000,
            },
            AssignmentStatus::Terminated {
                at: 1_700_000_500,
                reason: "mutual".to_string(),
            },
        ];

        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: AssignmentStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, parsed);
        }
    }

    #[test]
    fn test_routing_mode_roundtrip_variants() {
        let variants = vec![
            RoutingMode::ThroughHomeCoop,
            RoutingMode::DirectToWorker,
            RoutingMode::Split { worker_pct: 50 },
        ];

        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: RoutingMode = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, parsed);
        }
    }

    #[test]
    fn test_availability_roundtrip_variants() {
        let variants = vec![
            Availability::Immediate,
            Availability::FromDate(1_700_000_000),
            Availability::Seasonal {
                seasons: vec!["winter".to_string(), "summer".to_string()],
            },
            Availability::PartTime { hours_per_week: 20 },
        ];

        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let parsed: Availability = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, parsed);
        }
    }

    #[test]
    fn test_credit_routing_validation() {
        let routing = CreditRouting {
            routing_mode: RoutingMode::Split { worker_pct: 80 },
            admin_fee_bps: 150,
            share_contribution: true,
        };
        routing.validate().unwrap();

        let routing = CreditRouting {
            routing_mode: RoutingMode::Split { worker_pct: 120 },
            admin_fee_bps: 150,
            share_contribution: false,
        };
        assert!(routing.validate().is_err());

        let routing = CreditRouting {
            routing_mode: RoutingMode::ThroughHomeCoop,
            admin_fee_bps: 20_000,
            share_contribution: false,
        };
        assert!(routing.validate().is_err());
    }

    #[test]
    fn test_assignment_date_validation() {
        let mut assignment = sample_assignment();
        assignment.validate().unwrap();

        assignment.end_date = Some(assignment.start_date);
        assert!(assignment.validate().is_err());
    }

    #[test]
    fn test_labor_pool_roundtrip() {
        let registration = PoolRegistration {
            worker: "entity:icn:individual:worker-123".to_string(),
            home_coop: "entity:icn:cooperative:home-coop".to_string(),
            skills: vec!["welding".to_string(), "maintenance".to_string()],
            availability: Availability::Immediate,
            registered_at: 1_700_000_100,
        };

        let need = LaborNeed {
            id: "need-456".to_string(),
            requesting_coop: "entity:icn:cooperative:host-coop".to_string(),
            role: "Technician".to_string(),
            skills_required: vec!["maintenance".to_string()],
            duration: Some(30),
            posted_at: 1_700_000_200,
        };

        let pool = LaborPool {
            id: "pool-001".to_string(),
            manager: "entity:icn:federation:midwest".to_string(),
            available_workers: vec![registration],
            open_needs: vec![need],
        };

        let json = serde_json::to_string(&pool).unwrap();
        let parsed: LaborPool = serde_json::from_str(&json).unwrap();
        assert_eq!(pool, parsed);
    }
}
