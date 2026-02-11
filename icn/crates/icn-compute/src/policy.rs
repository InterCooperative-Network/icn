//! Cooperative scheduling policies for resource governance.
//!
//! This module enables per-cooperative rules, quotas, and enforcement:
//! - Member quotas (CPU hours, concurrent tasks, credits)
//! - Data sovereignty (region restrictions)
//! - Time windows (when tasks can run)
//! - Executor filtering (whitelist/blacklist)
//! - Priority adjustments based on member rules
//!
//! Policies can be updated via governance proposals (Phase 13 integration).

use crate::error::ComputeError;
use crate::types::{ComputeTask, TaskPriority};
use chrono::{Datelike, Timelike};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Commons Compute Governance (E7 - #1134)
// ============================================================================

/// Charter-defined priority policies for commons compute.
///
/// Cooperatives can configure how tasks are prioritized when using shared
/// commons compute resources. This affects scheduling order and preemption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CharterPriority {
    /// Universal basic services get top priority.
    /// Tasks tagged as UBS are scheduled before all others.
    #[default]
    UbsFirst,
    /// Emergency tasks preempt all others.
    /// Critical priority tasks can interrupt running lower-priority tasks.
    EmergencyFirst,
    /// Fair share - round-robin by member standing.
    /// Each member gets equal scheduling opportunities based on their standing.
    FairShare,
    /// First-come-first-served.
    /// Tasks are scheduled strictly in submission order.
    Fifo,
}

impl CharterPriority {
    /// Whether this policy allows preemption of running tasks.
    ///
    /// Only `EmergencyFirst` and `UbsFirst` policies support preemption,
    /// as they prioritize certain task types over others.
    pub fn allows_preemption(&self) -> bool {
        matches!(self, Self::EmergencyFirst | Self::UbsFirst)
    }

    /// Human-readable description of this priority policy.
    pub fn description(&self) -> &'static str {
        match self {
            Self::UbsFirst => "Universal basic services scheduled first",
            Self::EmergencyFirst => "Emergency tasks preempt all others",
            Self::FairShare => "Round-robin by member standing",
            Self::Fifo => "First-come-first-served",
        }
    }
}

/// Governance policy for commons compute pool.
///
/// This policy is defined in the cooperative's charter and controls how
/// shared compute resources are allocated among members.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonsPoolPolicy {
    /// Minimum standing required to submit to commons (0.0–1.0).
    /// Members below this threshold cannot use commons compute.
    pub min_standing: f64,

    /// Priority policy from charter.
    /// Determines how tasks are ordered for execution.
    pub priority: CharterPriority,

    /// Maximum concurrent tasks per submitter.
    /// Prevents any single member from monopolizing resources.
    pub max_concurrent_per_submitter: u32,

    /// Whether preemption is enabled.
    /// If true, higher-priority tasks can interrupt lower-priority ones.
    pub preemption_enabled: bool,

    /// Credit ceiling for cost validation.
    /// Tasks whose estimated cost exceeds available credits are rejected.
    #[serde(default)]
    pub credit_ceiling: Option<i64>,
}

impl Default for CommonsPoolPolicy {
    fn default() -> Self {
        Self {
            min_standing: 0.3,
            priority: CharterPriority::default(),
            max_concurrent_per_submitter: 5,
            preemption_enabled: false,
            credit_ceiling: None,
        }
    }
}

impl CommonsPoolPolicy {
    /// Validate that a submitter has sufficient credits for a task.
    ///
    /// Returns `Ok(())` if the submitter has sufficient credit ceiling,
    /// or if no ceiling is configured. Returns an error if the task's
    /// estimated cost would exceed the submitter's available credits.
    pub fn validate_submitter_credit(
        &self,
        task: &ComputeTask,
        submitter_balance: i64,
    ) -> Result<(), ComputeError> {
        // If no ceiling configured, skip validation
        let ceiling = match self.credit_ceiling {
            Some(c) => c,
            None => return Ok(()),
        };

        // Estimate task cost from fuel limit and payment rate
        let estimated_cost = Self::estimate_task_cost(task);

        // Check if submitter has room under their ceiling
        let available = ceiling.saturating_sub(submitter_balance);
        if estimated_cost > available {
            return Err(ComputeError::InsufficientCommonsCredits {
                balance: submitter_balance,
                required: estimated_cost,
            });
        }

        Ok(())
    }

    /// Estimate the credit cost of a task.
    ///
    /// Uses payment_rate if specified, otherwise derives from fuel limit.
    fn estimate_task_cost(task: &ComputeTask) -> i64 {
        if let Some(rate) = task.payment_rate {
            // payment_rate is credits per 1000 fuel units
            let fuel = task.fuel_limit.0 as i64;
            (fuel * rate as i64) / 1000
        } else {
            // Default: 1 credit per 1000 fuel units
            (task.fuel_limit.0 as i64) / 1000
        }
    }

    /// Check if a new task can preempt a running task.
    ///
    /// Returns `true` if preemption is enabled and the new task has
    /// sufficiently higher priority to justify interrupting the running task.
    pub fn can_preempt(&self, new_task: &ComputeTask, running_task: &ComputeTask) -> bool {
        // Preemption must be enabled
        if !self.preemption_enabled {
            return false;
        }

        // Priority policy must allow preemption
        if !self.priority.allows_preemption() {
            return false;
        }

        // New task must have strictly higher priority
        // Critical can preempt High/Normal/Low
        // High can preempt Normal/Low
        // etc.
        new_task.priority > running_task.priority
    }

    /// Check if a submitter meets the minimum standing requirement.
    pub fn check_standing(&self, standing: f64) -> Result<(), ComputeError> {
        if standing < self.min_standing {
            Err(ComputeError::InsufficientTrust {
                required: self.min_standing,
                actual: standing,
            })
        } else {
            Ok(())
        }
    }
}

/// Cooperative scheduling policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoopSchedulingPolicy {
    /// Cooperative identifier
    pub coop_id: String,

    /// Governance domain controlling this policy (Phase 13 integration)
    #[serde(default)]
    pub governance_domain: Option<String>,

    /// Policy rules to enforce
    #[serde(default)]
    pub rules: Vec<SchedulingRule>,

    /// Member-specific quotas
    #[serde(default)]
    pub member_quotas: HashMap<String, MemberQuota>,

    /// Default quota for new members
    pub default_quota: MemberQuota,

    /// Policy enforcement mode
    #[serde(default)]
    pub enforcement_mode: EnforcementMode,
}

/// Individual scheduling rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SchedulingRule {
    /// Restrict task execution to specific region (GDPR, data sovereignty)
    DataSovereignty {
        /// Required region (e.g., "eu-central", "us-west")
        region: String,
        /// Applies to tasks with these tags
        #[serde(default)]
        tags: Vec<String>,
    },

    /// Time-based execution windows
    TimeWindow {
        /// Allowed hours (UTC, 0-23)
        allowed_hours: Vec<u8>,
        /// Days of week (0=Sunday, 6=Saturday)
        allowed_days: Vec<u8>,
        /// Applies to priority levels
        priorities: Vec<TaskPriority>,
    },

    /// Member priority override (e.g., building automation gets 3x priority)
    MemberPriority {
        /// Member DID
        member: String,
        /// Priority multiplier (0.5 = half priority, 2.0 = double)
        multiplier: f64,
    },

    /// Required executor capabilities
    RequireCapability {
        /// Capability name
        capability: String,
        /// Minimum version
        #[serde(default)]
        min_version: Option<String>,
    },

    /// Executor blacklist/whitelist
    ExecutorFilter {
        /// Whitelist (if non-empty, only these executors allowed)
        #[serde(default)]
        whitelist: Vec<String>,
        /// Blacklist (these executors never used)
        #[serde(default)]
        blacklist: Vec<String>,
    },
}

/// Member resource quota
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemberQuota {
    /// CPU-hours per month
    pub cpu_hours_per_month: f64,

    /// Maximum concurrent tasks
    pub max_concurrent_tasks: u32,

    /// Maximum task priority allowed
    pub max_priority: TaskPriority,

    /// Credits per month (economic limit)
    #[serde(default)]
    pub credits_per_month: Option<u64>,
}

/// Policy enforcement strictness
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum EnforcementMode {
    /// Reject tasks that violate policy
    #[default]
    Strict,

    /// Log violations but allow execution
    Permissive,

    /// Only track violations, no enforcement
    Monitoring,
}

/// Policy evaluation result
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDecision {
    /// Allow task with possible adjustments
    Allow {
        /// Adjusted priority based on member rules
        adjusted_priority: TaskPriority,
        /// Placement constraints from policy
        placement_constraints: PlacementConstraints,
    },

    /// Reject task with reason
    Reject { reason: String },
}

/// Placement constraints extracted from policy
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlacementConstraints {
    /// Required region (data sovereignty)
    pub required_region: Option<String>,

    /// Executor whitelist
    pub allowed_executors: Vec<String>,

    /// Executor blacklist
    pub forbidden_executors: Vec<String>,

    /// Required capabilities
    pub required_capabilities: Vec<String>,
}

/// Policy evaluation and enforcement manager
pub struct PolicyManager {
    /// Policies indexed by cooperative ID
    policies: Arc<RwLock<HashMap<String, CoopSchedulingPolicy>>>,

    /// Usage tracker for quota enforcement
    usage_tracker: Arc<UsageTracker>,
}

impl PolicyManager {
    /// Create new policy manager with usage tracker
    pub fn new(usage_tracker: Arc<UsageTracker>) -> Self {
        Self {
            policies: Arc::new(RwLock::new(HashMap::new())),
            usage_tracker,
        }
    }

    /// Add or update a policy
    pub async fn set_policy(&self, policy: CoopSchedulingPolicy) {
        let mut policies = self.policies.write().await;
        policies.insert(policy.coop_id.clone(), policy);
    }

    /// Get usage tracker for direct access
    pub fn usage_tracker(&self) -> Arc<UsageTracker> {
        Arc::clone(&self.usage_tracker)
    }

    /// Get policy for cooperative
    pub async fn get_policy(&self, coop_id: &str) -> Option<CoopSchedulingPolicy> {
        let policies = self.policies.read().await;
        policies.get(coop_id).cloned()
    }

    /// Remove policy
    pub async fn remove_policy(&self, coop_id: &str) -> Option<CoopSchedulingPolicy> {
        let mut policies = self.policies.write().await;
        policies.remove(coop_id)
    }

    /// List all policies
    pub async fn list_policies(&self) -> Vec<CoopSchedulingPolicy> {
        let policies = self.policies.read().await;
        policies.values().cloned().collect()
    }

    /// Check if task submission is allowed
    pub async fn check_submission(
        &self,
        task: &ComputeTask,
        submitter: &Did,
        coop_id: &str,
    ) -> Result<PolicyDecision, ComputeError> {
        // Get policy for this cooperative
        let policy = match self.get_policy(coop_id).await {
            Some(p) => p,
            None => {
                // No policy = allow everything
                return Ok(PolicyDecision::Allow {
                    adjusted_priority: task.priority,
                    placement_constraints: PlacementConstraints::default(),
                });
            }
        };

        // Get member quota (use specific quota or default)
        let quota = policy
            .member_quotas
            .get(&submitter.to_string())
            .cloned()
            .unwrap_or_else(|| policy.default_quota.clone());

        // Get current usage
        let usage = self.usage_tracker.get_usage(submitter, coop_id).await?;

        // Check quotas
        if let Some(rejection) = self.check_quotas(&quota, &usage, task)? {
            // Emit metrics
            icn_obs::metrics::compute::policy_violations_inc(coop_id, &rejection);

            // Determine quota type for detailed metric (matches actual rejection messages)
            let quota_type = if rejection.contains("CPU quota") {
                "cpu_hours"
            } else if rejection.contains("Concurrent") {
                "concurrent_tasks"
            } else if rejection.contains("priority") {
                "priority"
            } else if rejection.contains("credit quota") {
                "credits"
            } else {
                "unknown"
            };
            icn_obs::metrics::compute::quota_exceeded_inc(
                coop_id,
                &submitter.to_string(),
                quota_type,
            );

            if policy.enforcement_mode == EnforcementMode::Strict {
                return Ok(PolicyDecision::Reject { reason: rejection });
            } else {
                tracing::warn!(
                    coop_id = %coop_id,
                    submitter = %submitter,
                    reason = %rejection,
                    "Policy violation (permissive mode)"
                );
            }
        }

        // Evaluate scheduling rules
        if let Some(rejection) = self.evaluate_rules(&policy, task)? {
            // Emit violation metric
            icn_obs::metrics::compute::policy_violations_inc(coop_id, &rejection);

            if policy.enforcement_mode == EnforcementMode::Strict {
                return Ok(PolicyDecision::Reject { reason: rejection });
            } else {
                tracing::warn!(
                    coop_id = %coop_id,
                    task_id = %task.id,
                    reason = %rejection,
                    "Rule violation (permissive mode)"
                );
            }
        }

        // Calculate adjusted priority and constraints
        let original_priority = task.priority;
        let adjusted_priority = self.calculate_adjusted_priority(task, submitter, &policy);
        let placement_constraints = self.extract_placement_constraints(&policy, task);

        // Emit priority adjustment metric if changed
        if adjusted_priority != original_priority {
            icn_obs::metrics::compute::priority_adjustments_inc(
                coop_id,
                &format!("{original_priority:?}"),
                &format!("{adjusted_priority:?}"),
            );
        }

        Ok(PolicyDecision::Allow {
            adjusted_priority,
            placement_constraints,
        })
    }

    /// Check if quotas are exceeded
    fn check_quotas(
        &self,
        quota: &MemberQuota,
        usage: &UsageRecord,
        task: &ComputeTask,
    ) -> Result<Option<String>, ComputeError> {
        // CPU hours check
        if usage.cpu_hours_this_month >= quota.cpu_hours_per_month {
            return Ok(Some(format!(
                "Monthly CPU quota exceeded: {:.1}/{:.1} hours",
                usage.cpu_hours_this_month, quota.cpu_hours_per_month
            )));
        }

        // Concurrent tasks check
        if usage.concurrent_tasks >= quota.max_concurrent_tasks {
            return Ok(Some(format!(
                "Concurrent task limit reached: {}/{}",
                usage.concurrent_tasks, quota.max_concurrent_tasks
            )));
        }

        // Priority check
        if task.priority > quota.max_priority {
            return Ok(Some(format!(
                "Task priority {:?} exceeds member limit {:?}",
                task.priority, quota.max_priority
            )));
        }

        // Credits check (if applicable)
        if let (Some(credits_limit), Some(credits_spent)) = (
            quota.credits_per_month,
            Some(usage.credits_spent_this_month),
        ) {
            if credits_spent >= credits_limit {
                return Ok(Some(format!(
                    "Monthly credit quota exceeded: {credits_spent}/{credits_limit}"
                )));
            }
        }

        Ok(None)
    }

    /// Evaluate scheduling rules
    fn evaluate_rules(
        &self,
        policy: &CoopSchedulingPolicy,
        task: &ComputeTask,
    ) -> Result<Option<String>, ComputeError> {
        use chrono::Datelike;

        for rule in &policy.rules {
            if let SchedulingRule::TimeWindow {
                allowed_hours,
                allowed_days,
                priorities,
            } = rule
            {
                if priorities.contains(&task.priority) {
                    let now = chrono::Utc::now();

                    if !allowed_hours.contains(&(now.hour() as u8)) {
                        return Ok(Some(format!(
                            "Task not allowed at hour {} (allowed: {:?})",
                            now.hour(),
                            allowed_hours
                        )));
                    }

                    let weekday = now.weekday().num_days_from_sunday() as u8;
                    if !allowed_days.contains(&weekday) {
                        return Ok(Some(format!(
                            "Task not allowed on {} (allowed days: {:?})",
                            now.weekday(),
                            allowed_days
                        )));
                    }
                }
            }
            // Other rules are placement constraints, not submission rejections
        }

        Ok(None)
    }

    /// Calculate adjusted priority based on member rules
    fn calculate_adjusted_priority(
        &self,
        task: &ComputeTask,
        submitter: &Did,
        policy: &CoopSchedulingPolicy,
    ) -> TaskPriority {
        let mut multiplier = 1.0;

        for rule in &policy.rules {
            if let SchedulingRule::MemberPriority {
                member,
                multiplier: rule_multiplier,
            } = rule
            {
                if member == &submitter.to_string() {
                    multiplier = *rule_multiplier;
                    break;
                }
            }
        }

        // Apply multiplier to priority
        let base_value = task.priority as i32;
        let adjusted_value = ((base_value as f64) * multiplier).round() as i32;

        // Clamp to valid TaskPriority range (0-3)
        match adjusted_value.clamp(0, 3) {
            0 => TaskPriority::Low,
            1 => TaskPriority::Normal,
            2 => TaskPriority::High,
            3 => TaskPriority::Critical,
            _ => task.priority, // Fallback
        }
    }

    /// Extract placement constraints from policy rules
    fn extract_placement_constraints(
        &self,
        policy: &CoopSchedulingPolicy,
        _task: &ComputeTask,
    ) -> PlacementConstraints {
        let mut constraints = PlacementConstraints::default();

        for rule in &policy.rules {
            match rule {
                SchedulingRule::DataSovereignty { region, .. } => {
                    constraints.required_region = Some(region.clone());
                }
                SchedulingRule::ExecutorFilter {
                    whitelist,
                    blacklist,
                } => {
                    constraints.allowed_executors = whitelist.clone();
                    constraints.forbidden_executors = blacklist.clone();
                }
                SchedulingRule::RequireCapability { capability, .. } => {
                    constraints.required_capabilities.push(capability.clone());
                }
                _ => {}
            }
        }

        constraints
    }
}

/// Track resource usage per member per cooperative
pub struct UsageTracker {
    /// Usage records indexed by (coop_id, member_did)
    records: Arc<RwLock<HashMap<(String, String), UsageRecord>>>,
}

impl UsageTracker {
    /// Create new usage tracker
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record task execution
    pub async fn record_execution(
        &self,
        coop_id: &str,
        member: &Did,
        duration_ms: u64,
        credits_spent: u64,
    ) -> Result<(), ComputeError> {
        let key = (coop_id.to_string(), member.to_string());
        let mut records = self.records.write().await;

        let record = records.entry(key.clone()).or_insert_with(|| UsageRecord {
            coop_id: coop_id.to_string(),
            member_did: member.to_string(),
            cpu_hours_this_month: 0.0,
            cpu_hours_total: 0.0,
            concurrent_tasks: 0,
            tasks_completed_this_month: 0,
            credits_spent_this_month: 0,
            last_reset_at: Self::now_millis(),
            updated_at: Self::now_millis(),
        });

        // Check if monthly reset needed
        let now = Self::now_millis();
        if Self::should_reset_monthly(record.last_reset_at, now) {
            record.cpu_hours_this_month = 0.0;
            record.tasks_completed_this_month = 0;
            record.credits_spent_this_month = 0;
            record.last_reset_at = now;
        }

        // CPU-hours = (duration_ms / 1000 / 3600)
        let cpu_hours = duration_ms as f64 / 1000.0 / 3600.0;

        record.cpu_hours_this_month += cpu_hours;
        record.cpu_hours_total += cpu_hours;
        record.tasks_completed_this_month += 1;
        record.credits_spent_this_month += credits_spent;
        record.updated_at = now;

        // Update metrics
        icn_obs::metrics::compute::member_cpu_hours_set(
            coop_id,
            &member.to_string(),
            record.cpu_hours_this_month,
        );
        icn_obs::metrics::compute::member_credits_spent_set(
            coop_id,
            &member.to_string(),
            record.credits_spent_this_month,
        );

        Ok(())
    }

    /// Increment concurrent task count
    pub async fn task_claimed(&self, coop_id: &str, member: &Did) -> Result<(), ComputeError> {
        let key = (coop_id.to_string(), member.to_string());
        let mut records = self.records.write().await;

        let record = records.entry(key).or_insert_with(|| UsageRecord {
            coop_id: coop_id.to_string(),
            member_did: member.to_string(),
            cpu_hours_this_month: 0.0,
            cpu_hours_total: 0.0,
            concurrent_tasks: 0,
            tasks_completed_this_month: 0,
            credits_spent_this_month: 0,
            last_reset_at: Self::now_millis(),
            updated_at: Self::now_millis(),
        });

        record.concurrent_tasks += 1;
        record.updated_at = Self::now_millis();

        // Update metric
        icn_obs::metrics::compute::member_concurrent_tasks_set(
            coop_id,
            &member.to_string(),
            record.concurrent_tasks,
        );

        Ok(())
    }

    /// Decrement concurrent task count
    pub async fn task_completed(&self, coop_id: &str, member: &Did) -> Result<(), ComputeError> {
        let key = (coop_id.to_string(), member.to_string());
        let mut records = self.records.write().await;

        if let Some(record) = records.get_mut(&key) {
            record.concurrent_tasks = record.concurrent_tasks.saturating_sub(1);
            record.updated_at = Self::now_millis();

            // Update metric
            icn_obs::metrics::compute::member_concurrent_tasks_set(
                coop_id,
                &member.to_string(),
                record.concurrent_tasks,
            );
        }

        Ok(())
    }

    /// Get current usage for member
    pub async fn get_usage(
        &self,
        member: &Did,
        coop_id: &str,
    ) -> Result<UsageRecord, ComputeError> {
        let key = (coop_id.to_string(), member.to_string());
        let records = self.records.read().await;

        Ok(records.get(&key).cloned().unwrap_or_else(|| UsageRecord {
            coop_id: coop_id.to_string(),
            member_did: member.to_string(),
            cpu_hours_this_month: 0.0,
            cpu_hours_total: 0.0,
            concurrent_tasks: 0,
            tasks_completed_this_month: 0,
            credits_spent_this_month: 0,
            last_reset_at: Self::now_millis(),
            updated_at: Self::now_millis(),
        }))
    }

    /// List all usage records for a cooperative
    pub async fn list_coop_usage(&self, coop_id: &str) -> Result<Vec<UsageRecord>, ComputeError> {
        let records = self.records.read().await;

        let coop_records: Vec<UsageRecord> = records
            .iter()
            .filter(|((c, _), _)| c == coop_id)
            .map(|(_, record)| record.clone())
            .collect();

        Ok(coop_records)
    }

    fn now_millis() -> u64 {
        icn_time::current_timestamp_millis()
    }

    fn should_reset_monthly(last_reset: u64, now: u64) -> bool {
        use chrono::DateTime;

        let last = DateTime::from_timestamp((last_reset / 1000) as i64, 0).unwrap_or_default();
        let current = DateTime::from_timestamp((now / 1000) as i64, 0).unwrap_or_default();

        last.month() != current.month() || last.year() != current.year()
    }
}

impl Default for UsageTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Member usage record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageRecord {
    /// Cooperative ID
    pub coop_id: String,

    /// Member DID
    pub member_did: String,

    /// CPU-hours used this month
    pub cpu_hours_this_month: f64,

    /// CPU-hours used all-time
    pub cpu_hours_total: f64,

    /// Currently running tasks
    pub concurrent_tasks: u32,

    /// Tasks completed this month
    pub tasks_completed_this_month: u64,

    /// Credits spent this month
    pub credits_spent_this_month: u64,

    /// Last reset timestamp (monthly)
    pub last_reset_at: u64,

    /// Last updated timestamp
    pub updated_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_did() -> Did {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        keypair.did().clone()
    }

    fn make_test_task(priority: TaskPriority) -> ComputeTask {
        use crate::types::{
            DeterminismClass, ExecutorCapability, FuelLimit, PrivacyClass, TaskCode,
        };

        ComputeTask {
            id: "test-task".into(),
            submitter: make_test_did().to_string(),
            coop_id: None,
            code: TaskCode::Ccl(String::new()),
            inputs: vec![],
            fuel_limit: FuelLimit(10000),
            required_capabilities: vec![ExecutorCapability::Ccl],
            priority,
            created_at: 0,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
            federation_constraints: None,
            estimated_value: None,
            verification: None,
            // E1: Workload manifest fields
            inputs_hash: None,
            policy_hash: None,
            determinism_class: DeterminismClass::default(),
            privacy_class: PrivacyClass::default(),
            // E4: Storage specification fields
            storage_class: None,
            data_locality: None,
        }
    }

    #[tokio::test]
    async fn test_policy_manager_no_policy() {
        let tracker = Arc::new(UsageTracker::new());
        let manager = PolicyManager::new(tracker);

        let task = make_test_task(TaskPriority::Normal);
        let submitter = make_test_did();

        let decision = manager
            .check_submission(&task, &submitter, "test-coop")
            .await
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Allow { .. }));
    }

    #[tokio::test]
    async fn test_quota_cpu_hours_exceeded() {
        let tracker = Arc::new(UsageTracker::new());
        let manager = PolicyManager::new(tracker.clone());

        let submitter = make_test_did();
        let coop_id = "test-coop";

        // Set up policy with 1.0 CPU-hour limit
        let policy = CoopSchedulingPolicy {
            coop_id: coop_id.to_string(),
            governance_domain: None,
            rules: vec![],
            member_quotas: HashMap::new(),
            default_quota: MemberQuota {
                cpu_hours_per_month: 1.0,
                max_concurrent_tasks: 10,
                max_priority: TaskPriority::Critical,
                credits_per_month: None,
            },
            enforcement_mode: EnforcementMode::Strict,
        };

        manager.set_policy(policy).await;

        // Record 1.5 hours of usage (exceeds limit)
        tracker
            .record_execution(coop_id, &submitter, 5400000, 0) // 1.5 hours in ms
            .await
            .unwrap();

        // Try to submit task
        let task = make_test_task(TaskPriority::Normal);
        let decision = manager
            .check_submission(&task, &submitter, coop_id)
            .await
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Reject { .. }));
        if let PolicyDecision::Reject { reason } = decision {
            assert!(reason.contains("CPU quota exceeded"));
        }
    }

    #[tokio::test]
    async fn test_quota_concurrent_tasks_exceeded() {
        let tracker = Arc::new(UsageTracker::new());
        let manager = PolicyManager::new(tracker.clone());

        let submitter = make_test_did();
        let coop_id = "test-coop";

        // Set up policy with max 2 concurrent tasks
        let policy = CoopSchedulingPolicy {
            coop_id: coop_id.to_string(),
            governance_domain: None,
            rules: vec![],
            member_quotas: HashMap::new(),
            default_quota: MemberQuota {
                cpu_hours_per_month: 100.0,
                max_concurrent_tasks: 2,
                max_priority: TaskPriority::Critical,
                credits_per_month: None,
            },
            enforcement_mode: EnforcementMode::Strict,
        };

        manager.set_policy(policy).await;

        // Claim 2 tasks
        tracker.task_claimed(coop_id, &submitter).await.unwrap();
        tracker.task_claimed(coop_id, &submitter).await.unwrap();

        // Try to submit third task
        let task = make_test_task(TaskPriority::Normal);
        let decision = manager
            .check_submission(&task, &submitter, coop_id)
            .await
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Reject { .. }));
        if let PolicyDecision::Reject { reason } = decision {
            assert!(reason.contains("Concurrent task limit"));
        }
    }

    #[tokio::test]
    async fn test_quota_priority_exceeded() {
        let tracker = Arc::new(UsageTracker::new());
        let manager = PolicyManager::new(tracker);

        let submitter = make_test_did();
        let coop_id = "test-coop";

        // Set up policy with max Normal priority
        let policy = CoopSchedulingPolicy {
            coop_id: coop_id.to_string(),
            governance_domain: None,
            rules: vec![],
            member_quotas: HashMap::new(),
            default_quota: MemberQuota {
                cpu_hours_per_month: 100.0,
                max_concurrent_tasks: 10,
                max_priority: TaskPriority::Normal,
                credits_per_month: None,
            },
            enforcement_mode: EnforcementMode::Strict,
        };

        manager.set_policy(policy).await;

        // Try to submit High priority task
        let task = make_test_task(TaskPriority::High);
        let decision = manager
            .check_submission(&task, &submitter, coop_id)
            .await
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Reject { .. }));
        if let PolicyDecision::Reject { reason } = decision {
            assert!(reason.contains("priority"));
            assert!(reason.contains("exceeds"));
        }
    }

    #[tokio::test]
    async fn test_member_priority_multiplier() {
        let tracker = Arc::new(UsageTracker::new());
        let manager = PolicyManager::new(tracker);

        let submitter = make_test_did();
        let coop_id = "test-coop";

        // Set up policy with 2x priority multiplier for this member
        let policy = CoopSchedulingPolicy {
            coop_id: coop_id.to_string(),
            governance_domain: None,
            rules: vec![SchedulingRule::MemberPriority {
                member: submitter.to_string(),
                multiplier: 2.0,
            }],
            member_quotas: HashMap::new(),
            default_quota: MemberQuota {
                cpu_hours_per_month: 100.0,
                max_concurrent_tasks: 10,
                max_priority: TaskPriority::Critical,
                credits_per_month: None,
            },
            enforcement_mode: EnforcementMode::Strict,
        };

        manager.set_policy(policy).await;

        // Submit Normal priority task
        let task = make_test_task(TaskPriority::Normal);
        let decision = manager
            .check_submission(&task, &submitter, coop_id)
            .await
            .unwrap();

        // Should be adjusted to High priority (Normal=1 * 2.0 = 2 = High)
        if let PolicyDecision::Allow {
            adjusted_priority, ..
        } = decision
        {
            assert_eq!(adjusted_priority, TaskPriority::High);
        } else {
            panic!("Expected Allow decision");
        }
    }

    #[tokio::test]
    async fn test_data_sovereignty_constraint() {
        let tracker = Arc::new(UsageTracker::new());
        let manager = PolicyManager::new(tracker);

        let submitter = make_test_did();
        let coop_id = "test-coop";

        // Set up policy with EU data sovereignty
        let policy = CoopSchedulingPolicy {
            coop_id: coop_id.to_string(),
            governance_domain: None,
            rules: vec![SchedulingRule::DataSovereignty {
                region: "eu-central".to_string(),
                tags: vec![],
            }],
            member_quotas: HashMap::new(),
            default_quota: MemberQuota {
                cpu_hours_per_month: 100.0,
                max_concurrent_tasks: 10,
                max_priority: TaskPriority::Critical,
                credits_per_month: None,
            },
            enforcement_mode: EnforcementMode::Strict,
        };

        manager.set_policy(policy).await;

        let task = make_test_task(TaskPriority::Normal);
        let decision = manager
            .check_submission(&task, &submitter, coop_id)
            .await
            .unwrap();

        // Should have required_region constraint
        if let PolicyDecision::Allow {
            placement_constraints,
            ..
        } = decision
        {
            assert_eq!(
                placement_constraints.required_region,
                Some("eu-central".to_string())
            );
        } else {
            panic!("Expected Allow decision");
        }
    }

    #[tokio::test]
    async fn test_executor_filter_constraint() {
        let tracker = Arc::new(UsageTracker::new());
        let manager = PolicyManager::new(tracker);

        let submitter = make_test_did();
        let coop_id = "test-coop";

        // Set up policy with executor filter
        let policy = CoopSchedulingPolicy {
            coop_id: coop_id.to_string(),
            governance_domain: None,
            rules: vec![SchedulingRule::ExecutorFilter {
                whitelist: vec!["did:icn:executor-a".to_string()],
                blacklist: vec!["did:icn:executor-b".to_string()],
            }],
            member_quotas: HashMap::new(),
            default_quota: MemberQuota {
                cpu_hours_per_month: 100.0,
                max_concurrent_tasks: 10,
                max_priority: TaskPriority::Critical,
                credits_per_month: None,
            },
            enforcement_mode: EnforcementMode::Strict,
        };

        manager.set_policy(policy).await;

        let task = make_test_task(TaskPriority::Normal);
        let decision = manager
            .check_submission(&task, &submitter, coop_id)
            .await
            .unwrap();

        // Should have executor filter constraints
        if let PolicyDecision::Allow {
            placement_constraints,
            ..
        } = decision
        {
            assert_eq!(
                placement_constraints.allowed_executors,
                vec!["did:icn:executor-a".to_string()]
            );
            assert_eq!(
                placement_constraints.forbidden_executors,
                vec!["did:icn:executor-b".to_string()]
            );
        } else {
            panic!("Expected Allow decision");
        }
    }

    #[tokio::test]
    async fn test_permissive_mode_allows_violations() {
        let tracker = Arc::new(UsageTracker::new());
        let manager = PolicyManager::new(tracker.clone());

        let submitter = make_test_did();
        let coop_id = "test-coop";

        // Set up policy in Permissive mode
        let policy = CoopSchedulingPolicy {
            coop_id: coop_id.to_string(),
            governance_domain: None,
            rules: vec![],
            member_quotas: HashMap::new(),
            default_quota: MemberQuota {
                cpu_hours_per_month: 1.0,
                max_concurrent_tasks: 10,
                max_priority: TaskPriority::Critical,
                credits_per_month: None,
            },
            enforcement_mode: EnforcementMode::Permissive,
        };

        manager.set_policy(policy).await;

        // Record 1.5 hours (exceeds limit)
        tracker
            .record_execution(coop_id, &submitter, 5400000, 0)
            .await
            .unwrap();

        // Should still allow in Permissive mode
        let task = make_test_task(TaskPriority::Normal);
        let decision = manager
            .check_submission(&task, &submitter, coop_id)
            .await
            .unwrap();

        assert!(matches!(decision, PolicyDecision::Allow { .. }));
    }

    #[tokio::test]
    async fn test_usage_tracker_concurrent_tasks() {
        let tracker = UsageTracker::new();
        let member = make_test_did();
        let coop_id = "test-coop";

        // Claim 3 tasks
        tracker.task_claimed(coop_id, &member).await.unwrap();
        tracker.task_claimed(coop_id, &member).await.unwrap();
        tracker.task_claimed(coop_id, &member).await.unwrap();

        let usage = tracker.get_usage(&member, coop_id).await.unwrap();
        assert_eq!(usage.concurrent_tasks, 3);

        // Complete 1 task
        tracker.task_completed(coop_id, &member).await.unwrap();

        let usage = tracker.get_usage(&member, coop_id).await.unwrap();
        assert_eq!(usage.concurrent_tasks, 2);
    }

    #[tokio::test]
    async fn test_usage_tracker_cpu_hours() {
        let tracker = UsageTracker::new();
        let member = make_test_did();
        let coop_id = "test-coop";

        // Record 0.5 hours (1800000 ms)
        tracker
            .record_execution(coop_id, &member, 1800000, 0)
            .await
            .unwrap();

        let usage = tracker.get_usage(&member, coop_id).await.unwrap();
        assert!((usage.cpu_hours_this_month - 0.5).abs() < 0.01);
        assert!((usage.cpu_hours_total - 0.5).abs() < 0.01);

        // Record another 0.25 hours (900000 ms)
        tracker
            .record_execution(coop_id, &member, 900000, 0)
            .await
            .unwrap();

        let usage = tracker.get_usage(&member, coop_id).await.unwrap();
        assert!((usage.cpu_hours_this_month - 0.75).abs() < 0.01);
        assert!((usage.cpu_hours_total - 0.75).abs() < 0.01);
        assert_eq!(usage.tasks_completed_this_month, 2);
    }

    #[tokio::test]
    async fn test_policy_lifecycle_integration() {
        use crate::types::{DeterminismClass, FuelLimit, PrivacyClass, TaskCode};

        // Setup: Create PolicyManager with UsageTracker
        let tracker = Arc::new(UsageTracker::new());
        let manager = PolicyManager::new(tracker.clone());

        let coop_id = "integration-coop";
        let regular_member = make_test_did();
        let premium_member = make_test_did();
        let over_quota_member = make_test_did();

        // Create comprehensive policy with quotas, rules, and constraints
        let mut member_quotas = HashMap::new();

        // Premium member gets higher quotas
        member_quotas.insert(
            premium_member.to_string(),
            MemberQuota {
                cpu_hours_per_month: 200.0,
                max_concurrent_tasks: 20,
                max_priority: TaskPriority::Critical,
                credits_per_month: Some(2000),
            },
        );

        // Over-quota member gets low quotas (for testing rejection)
        member_quotas.insert(
            over_quota_member.to_string(),
            MemberQuota {
                cpu_hours_per_month: 1.0,
                max_concurrent_tasks: 2,
                max_priority: TaskPriority::Normal,
                credits_per_month: Some(100),
            },
        );

        let policy = CoopSchedulingPolicy {
            coop_id: coop_id.to_string(),
            governance_domain: Some("governance:integration".to_string()),
            rules: vec![
                // Priority multiplier for premium member
                SchedulingRule::MemberPriority {
                    member: premium_member.to_string(),
                    multiplier: 1.5,
                },
                // Executor whitelist/blacklist
                SchedulingRule::ExecutorFilter {
                    whitelist: vec![
                        "did:icn:executor-a".to_string(),
                        "did:icn:executor-b".to_string(),
                    ],
                    blacklist: vec!["did:icn:executor-bad".to_string()],
                },
                // Data sovereignty constraint
                SchedulingRule::DataSovereignty {
                    region: "eu-west".to_string(),
                    tags: vec![],
                },
                // Required capability
                SchedulingRule::RequireCapability {
                    capability: "gdpr-compliant".to_string(),
                    min_version: None,
                },
            ],
            member_quotas,
            default_quota: MemberQuota {
                cpu_hours_per_month: 50.0,
                max_concurrent_tasks: 5,
                max_priority: TaskPriority::High,
                credits_per_month: Some(500),
            },
            enforcement_mode: EnforcementMode::Strict,
        };

        manager.set_policy(policy).await;

        // Test 1: Regular member with normal task - should be allowed
        let task1 = make_test_task(TaskPriority::Normal);
        let decision1 = manager
            .check_submission(&task1, &regular_member, coop_id)
            .await
            .unwrap();

        assert!(matches!(decision1, PolicyDecision::Allow { .. }));
        if let PolicyDecision::Allow {
            placement_constraints,
            adjusted_priority,
        } = decision1
        {
            // Priority unchanged
            assert_eq!(adjusted_priority, TaskPriority::Normal);

            // Executor filter constraints applied
            assert_eq!(
                placement_constraints.allowed_executors,
                vec![
                    "did:icn:executor-a".to_string(),
                    "did:icn:executor-b".to_string()
                ]
            );
            assert_eq!(
                placement_constraints.forbidden_executors,
                vec!["did:icn:executor-bad".to_string()]
            );

            // Data sovereignty constraint applied
            assert_eq!(
                placement_constraints.required_region,
                Some("eu-west".to_string())
            );
            assert_eq!(
                placement_constraints.required_capabilities,
                vec!["gdpr-compliant".to_string()]
            );
        }

        // Test 2: Premium member with high priority - should get priority boost
        let task2 = make_test_task(TaskPriority::High);
        let decision2 = manager
            .check_submission(&task2, &premium_member, coop_id)
            .await
            .unwrap();

        if let PolicyDecision::Allow {
            adjusted_priority, ..
        } = decision2
        {
            // Priority boosted by 1.5x multiplier (High → Critical)
            assert_eq!(adjusted_priority, TaskPriority::Critical);
        } else {
            panic!("Expected Allow decision for premium member");
        }

        // Test 3: CPU hours quota exceeded - should reject
        // Simulate heavy usage (2 hours of execution)
        tracker
            .record_execution(coop_id, &over_quota_member, 7200000, 0)
            .await
            .unwrap();

        let task3 = make_test_task(TaskPriority::Normal);
        let decision3 = manager
            .check_submission(&task3, &over_quota_member, coop_id)
            .await
            .unwrap();

        assert!(matches!(decision3, PolicyDecision::Reject { .. }));
        if let PolicyDecision::Reject { reason } = decision3 {
            assert!(reason.contains("CPU quota"));
        }

        // Reset over_quota_member usage for next test
        {
            let mut usage_map = tracker.records.write().await;
            usage_map.remove(&(coop_id.to_string(), over_quota_member.to_string()));
        }

        // Test 4: Concurrent tasks limit exceeded - should reject
        // Claim 3 tasks (exceeds limit of 2)
        tracker
            .task_claimed(coop_id, &over_quota_member)
            .await
            .unwrap();
        tracker
            .task_claimed(coop_id, &over_quota_member)
            .await
            .unwrap();

        let task4 = make_test_task(TaskPriority::Normal);
        let decision4 = manager
            .check_submission(&task4, &over_quota_member, coop_id)
            .await
            .unwrap();

        assert!(matches!(decision4, PolicyDecision::Reject { .. }));
        if let PolicyDecision::Reject { reason } = decision4 {
            assert!(reason.contains("Concurrent"));
        }

        // Complete tasks to reset concurrent count
        tracker
            .task_completed(coop_id, &over_quota_member)
            .await
            .unwrap();
        tracker
            .task_completed(coop_id, &over_quota_member)
            .await
            .unwrap();

        // Test 5: Priority limit exceeded - should reject
        let task5 = ComputeTask {
            id: "priority-test".to_string(),
            submitter: over_quota_member.to_string(),
            coop_id: Some(coop_id.to_string()),
            code: TaskCode::Ccl("contract {}".to_string()),
            inputs: vec![],
            fuel_limit: FuelLimit(10000),
            required_capabilities: vec![],
            priority: TaskPriority::Critical, // Exceeds Normal limit
            created_at: 0,
            deadline: None,
            payment_rate: None,
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
            federation_constraints: None,
            estimated_value: None,
            verification: None,
            inputs_hash: None,
            policy_hash: None,
            determinism_class: DeterminismClass::default(),
            privacy_class: PrivacyClass::default(),
            // E4: Storage specification fields
            storage_class: None,
            data_locality: None,
        };

        let decision5 = manager
            .check_submission(&task5, &over_quota_member, coop_id)
            .await
            .unwrap();

        assert!(matches!(decision5, PolicyDecision::Reject { .. }));
        if let PolicyDecision::Reject { reason } = decision5 {
            assert!(reason.contains("priority"));
        }

        // Test 6: Credits quota exceeded - should reject
        // Simulate spending 150 credits (exceeds limit of 100)
        tracker
            .record_execution(coop_id, &over_quota_member, 1000, 150)
            .await
            .unwrap();

        let task6 = make_test_task(TaskPriority::Normal);
        let decision6 = manager
            .check_submission(&task6, &over_quota_member, coop_id)
            .await
            .unwrap();

        assert!(matches!(decision6, PolicyDecision::Reject { .. }));
        if let PolicyDecision::Reject { reason } = decision6 {
            assert!(reason.contains("credit quota"));
        }

        // Test 7: Verify usage tracking state
        let usage_regular = tracker.get_usage(&regular_member, coop_id).await.unwrap();
        assert_eq!(usage_regular.concurrent_tasks, 0);
        // Regular member submitted 1 task (not completed/executed yet in this test)

        let _usage_premium = tracker.get_usage(&premium_member, coop_id).await.unwrap();
        // Premium member submitted 1 task (not completed/executed yet in this test)

        let usage_over = tracker
            .get_usage(&over_quota_member, coop_id)
            .await
            .unwrap();
        assert_eq!(usage_over.concurrent_tasks, 0); // All completed
                                                    // Only record_execution increments tasks_completed_this_month (test 6)
        assert_eq!(usage_over.tasks_completed_this_month, 1);
        assert_eq!(usage_over.credits_spent_this_month, 150);
    }

    // ========================================================================
    // CharterPriority tests (E7 - #1134)
    // ========================================================================

    #[test]
    fn test_charter_priority_serialization() {
        // Test JSON serialization uses snake_case
        let priority = CharterPriority::UbsFirst;
        let json = serde_json::to_string(&priority).unwrap();
        assert_eq!(json, r#""ubs_first""#);

        let priority = CharterPriority::EmergencyFirst;
        let json = serde_json::to_string(&priority).unwrap();
        assert_eq!(json, r#""emergency_first""#);

        let priority = CharterPriority::FairShare;
        let json = serde_json::to_string(&priority).unwrap();
        assert_eq!(json, r#""fair_share""#);

        let priority = CharterPriority::Fifo;
        let json = serde_json::to_string(&priority).unwrap();
        assert_eq!(json, r#""fifo""#);
    }

    #[test]
    fn test_charter_priority_deserialization() {
        let priority: CharterPriority = serde_json::from_str(r#""ubs_first""#).unwrap();
        assert_eq!(priority, CharterPriority::UbsFirst);

        let priority: CharterPriority = serde_json::from_str(r#""emergency_first""#).unwrap();
        assert_eq!(priority, CharterPriority::EmergencyFirst);

        let priority: CharterPriority = serde_json::from_str(r#""fair_share""#).unwrap();
        assert_eq!(priority, CharterPriority::FairShare);

        let priority: CharterPriority = serde_json::from_str(r#""fifo""#).unwrap();
        assert_eq!(priority, CharterPriority::Fifo);
    }

    #[test]
    fn test_charter_priority_allows_preemption() {
        assert!(CharterPriority::UbsFirst.allows_preemption());
        assert!(CharterPriority::EmergencyFirst.allows_preemption());
        assert!(!CharterPriority::FairShare.allows_preemption());
        assert!(!CharterPriority::Fifo.allows_preemption());
    }

    #[test]
    fn test_charter_priority_default() {
        let priority = CharterPriority::default();
        assert_eq!(priority, CharterPriority::UbsFirst);
    }

    #[test]
    fn test_charter_priority_description() {
        assert!(!CharterPriority::UbsFirst.description().is_empty());
        assert!(!CharterPriority::EmergencyFirst.description().is_empty());
        assert!(!CharterPriority::FairShare.description().is_empty());
        assert!(!CharterPriority::Fifo.description().is_empty());
    }

    // ========================================================================
    // CommonsPoolPolicy tests (E7 - #1134)
    // ========================================================================

    #[test]
    fn test_commons_pool_policy_default() {
        let policy = CommonsPoolPolicy::default();
        assert!((policy.min_standing - 0.3).abs() < f64::EPSILON);
        assert_eq!(policy.priority, CharterPriority::UbsFirst);
        assert_eq!(policy.max_concurrent_per_submitter, 5);
        assert!(!policy.preemption_enabled);
        assert!(policy.credit_ceiling.is_none());
    }

    #[test]
    fn test_commons_pool_policy_serialization() {
        let policy = CommonsPoolPolicy {
            min_standing: 0.5,
            priority: CharterPriority::EmergencyFirst,
            max_concurrent_per_submitter: 10,
            preemption_enabled: true,
            credit_ceiling: Some(1000),
        };

        let json = serde_json::to_string(&policy).unwrap();
        let parsed: CommonsPoolPolicy = serde_json::from_str(&json).unwrap();

        assert!((parsed.min_standing - 0.5).abs() < f64::EPSILON);
        assert_eq!(parsed.priority, CharterPriority::EmergencyFirst);
        assert_eq!(parsed.max_concurrent_per_submitter, 10);
        assert!(parsed.preemption_enabled);
        assert_eq!(parsed.credit_ceiling, Some(1000));
    }

    #[test]
    fn test_commons_pool_policy_cost_validation_no_ceiling() {
        let policy = CommonsPoolPolicy::default();
        let task = make_test_task(TaskPriority::Normal);

        // No ceiling configured, should always pass
        let result = policy.validate_submitter_credit(&task, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_commons_pool_policy_cost_validation_sufficient_credit() {
        let policy = CommonsPoolPolicy {
            credit_ceiling: Some(100),
            ..Default::default()
        };
        let task = make_test_task(TaskPriority::Normal);
        // Task has fuel_limit=10000, so cost = 10000/1000 = 10 credits
        // Ceiling is 100, balance is 0, so 100-0=100 available > 10 required

        let result = policy.validate_submitter_credit(&task, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_commons_pool_policy_cost_validation_insufficient_credit() {
        let policy = CommonsPoolPolicy {
            credit_ceiling: Some(100),
            ..Default::default()
        };
        let task = make_test_task(TaskPriority::Normal);
        // Task has fuel_limit=10000, so cost = 10000/1000 = 10 credits
        // Ceiling is 100, balance is 95, so 100-95=5 available < 10 required

        let result = policy.validate_submitter_credit(&task, 95);
        assert!(result.is_err());
        if let Err(ComputeError::InsufficientCommonsCredits { balance, required }) = result {
            assert_eq!(balance, 95);
            assert_eq!(required, 10);
        } else {
            panic!("Expected InsufficientCommonsCredits error");
        }
    }

    #[test]
    fn test_commons_pool_policy_cost_with_payment_rate() {
        use crate::types::{
            DeterminismClass, ExecutorCapability, FuelLimit, PrivacyClass, TaskCode,
        };

        let policy = CommonsPoolPolicy {
            credit_ceiling: Some(100),
            ..Default::default()
        };

        // Task with explicit payment rate: 5 credits per 1000 fuel
        let task = ComputeTask {
            id: "test-task".into(),
            submitter: make_test_did().to_string(),
            coop_id: None,
            code: TaskCode::Ccl(String::new()),
            inputs: vec![],
            fuel_limit: FuelLimit(10000), // 10000 fuel * 5/1000 = 50 credits
            required_capabilities: vec![ExecutorCapability::Ccl],
            priority: TaskPriority::Normal,
            created_at: 0,
            deadline: None,
            payment_rate: Some(5), // 5 credits per 1000 fuel
            payment_currency: None,
            resource_profile: None,
            actor_mode: None,
            placement_constraints: None,
            federation_constraints: None,
            estimated_value: None,
            verification: None,
            inputs_hash: None,
            policy_hash: None,
            determinism_class: DeterminismClass::default(),
            privacy_class: PrivacyClass::default(),
            storage_class: None,
            data_locality: None,
        };

        // With balance=60, available=40, required=50 -> should fail
        let result = policy.validate_submitter_credit(&task, 60);
        assert!(result.is_err());

        // With balance=40, available=60, required=50 -> should pass
        let result = policy.validate_submitter_credit(&task, 40);
        assert!(result.is_ok());
    }

    #[test]
    fn test_commons_pool_policy_preemption_disabled() {
        let policy = CommonsPoolPolicy {
            preemption_enabled: false,
            priority: CharterPriority::EmergencyFirst,
            ..Default::default()
        };

        let high_task = make_test_task(TaskPriority::Critical);
        let low_task = make_test_task(TaskPriority::Low);

        // Even with higher priority, preemption disabled
        assert!(!policy.can_preempt(&high_task, &low_task));
    }

    #[test]
    fn test_commons_pool_policy_preemption_enabled() {
        let policy = CommonsPoolPolicy {
            preemption_enabled: true,
            priority: CharterPriority::EmergencyFirst,
            ..Default::default()
        };

        let critical = make_test_task(TaskPriority::Critical);
        let high = make_test_task(TaskPriority::High);
        let normal = make_test_task(TaskPriority::Normal);
        let low = make_test_task(TaskPriority::Low);

        // Critical can preempt all lower priorities
        assert!(policy.can_preempt(&critical, &high));
        assert!(policy.can_preempt(&critical, &normal));
        assert!(policy.can_preempt(&critical, &low));

        // High can preempt normal and low
        assert!(policy.can_preempt(&high, &normal));
        assert!(policy.can_preempt(&high, &low));

        // Cannot preempt same or higher priority
        assert!(!policy.can_preempt(&normal, &normal));
        assert!(!policy.can_preempt(&low, &high));
    }

    #[test]
    fn test_commons_pool_policy_preemption_policy_disallows() {
        // FairShare policy doesn't allow preemption
        let policy = CommonsPoolPolicy {
            preemption_enabled: true,
            priority: CharterPriority::FairShare,
            ..Default::default()
        };

        let high_task = make_test_task(TaskPriority::Critical);
        let low_task = make_test_task(TaskPriority::Low);

        // Even with preemption_enabled, policy doesn't allow it
        assert!(!policy.can_preempt(&high_task, &low_task));
    }

    #[test]
    fn test_commons_pool_policy_check_standing() {
        let policy = CommonsPoolPolicy {
            min_standing: 0.3,
            ..Default::default()
        };

        // Sufficient standing
        assert!(policy.check_standing(0.5).is_ok());
        assert!(policy.check_standing(0.3).is_ok());

        // Insufficient standing
        let result = policy.check_standing(0.2);
        assert!(result.is_err());
        if let Err(ComputeError::InsufficientTrust { required, actual }) = result {
            assert!((required - 0.3).abs() < f64::EPSILON);
            assert!((actual - 0.2).abs() < f64::EPSILON);
        } else {
            panic!("Expected InsufficientTrust error");
        }
    }
}
