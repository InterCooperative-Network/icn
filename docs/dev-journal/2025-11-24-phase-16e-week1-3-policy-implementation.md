# Phase 16E Week 1: Cooperative Scheduling Policy Design

**Date**: 2025-11-24
**Status**: ✅ Complete
**Phase**: 16E - Cooperative Scheduling Policies (3-4 weeks)
**Dependencies**: Phase 16D (actor migration), Phase 16C (locality awareness), Phase 13 (governance primitives)

## Overview

Phase 16E adds **per-cooperative scheduling policies** to ICN's compute layer, enabling communities to enforce their own rules about resource usage, member priorities, and placement constraints.

### Key Insight

Different cooperatives have different needs:
- **Timebanks**: Equal access, usage quotas (10 CPU-hours/member/month)
- **Research Coops**: Data sovereignty (EU-only processing for GDPR)
- **Housing Coops**: Priority-based (building automation gets Critical priority)
- **Mutual Aid Networks**: Fair-share enforcement (no single member dominates)

The scheduler must support **pluggable policies** that cooperatives can configure via governance proposals.

## Motivation

### Current State (Phase 16D)

The scheduler places tasks based on:
- Trust scores (25%)
- Resource capacity (20%)
- Queue depth (15%)
- Network RTT (15%)
- Data locality (15%)
- Locality hints (10%)

**But**: No per-cooperative rules. Every task is treated equally regardless of:
- Who submitted it (member quotas)
- What cooperative it belongs to (coop-level policies)
- What time of day it is (time windows)
- Where data must stay (data sovereignty)

### Problems Without Policy Enforcement

1. **Quota Abuse**: Single member submits 1000 tasks, starves others
2. **Regulatory Violations**: GDPR data processed outside EU
3. **Priority Violations**: Low-priority tasks block critical infrastructure
4. **Economic Unfairness**: Heavy users don't pay proportional costs

### Solution: Policy Layer

Add a **policy evaluation step** before placement scoring:
```
Task Submission → Policy Check → (Pass) → Placement Scoring → Execution
                              → (Fail) → Reject with reason
```

Policies can:
- **Enforce quotas**: "Max 10 CPU-hours per member per month"
- **Control placement**: "All tasks must run in EU region"
- **Set priorities**: "Building automation = Critical priority"
- **Track usage**: "Member X has used 5.2/10 CPU-hours this month"

## Architecture

### Core Types

```rust
/// Cooperative scheduling policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoopSchedulingPolicy {
    /// Cooperative identifier
    pub coop_id: String,

    /// Governance domain controlling this policy (Phase 13 integration)
    pub governance_domain: Option<String>,

    /// Policy rules to enforce
    pub rules: Vec<SchedulingRule>,

    /// Member-specific quotas
    pub member_quotas: HashMap<String, MemberQuota>,

    /// Default quota for new members
    pub default_quota: MemberQuota,

    /// Policy enforcement mode
    pub enforcement_mode: EnforcementMode,
}

/// Individual scheduling rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SchedulingRule {
    /// Restrict task execution to specific region
    DataSovereignty {
        /// Required region (e.g., "eu-central", "us-west")
        region: String,
        /// Applies to tasks with these tags
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

    /// Member priority override
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
        min_version: Option<String>,
    },

    /// Executor blacklist/whitelist
    ExecutorFilter {
        /// Whitelist (if non-empty, only these executors allowed)
        whitelist: Vec<String>,
        /// Blacklist (these executors never used)
        blacklist: Vec<String>,
    },
}

/// Member resource quota
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberQuota {
    /// CPU-hours per month
    pub cpu_hours_per_month: f64,

    /// Maximum concurrent tasks
    pub max_concurrent_tasks: u32,

    /// Maximum task priority allowed
    pub max_priority: TaskPriority,

    /// Credits per month (economic limit)
    pub credits_per_month: Option<u64>,
}

/// Policy enforcement strictness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnforcementMode {
    /// Reject tasks that violate policy
    Strict,

    /// Log violations but allow execution
    Permissive,

    /// Only track violations, no enforcement
    Monitoring,
}
```

### Policy Manager

```rust
/// Policy evaluation and enforcement
pub struct PolicyManager {
    /// Policies indexed by cooperative ID
    policies: Arc<RwLock<HashMap<String, CoopSchedulingPolicy>>>,

    /// Usage tracker for quota enforcement
    usage_tracker: Arc<UsageTracker>,

    /// Governance integration (Phase 13)
    governance: Option<Arc<dyn GovernanceProvider>>,
}

impl PolicyManager {
    /// Check if task submission is allowed
    pub async fn check_submission(
        &self,
        task: &ComputeTask,
        submitter: &Did,
    ) -> Result<PolicyDecision, ComputeError> {
        let policy = self.get_policy(&task.coop_id).await?;

        // Check member quota
        let quota = policy.get_quota(submitter);
        let usage = self.usage_tracker.get_usage(submitter, &task.coop_id).await?;

        if usage.cpu_hours_this_month >= quota.cpu_hours_per_month {
            return Ok(PolicyDecision::Reject {
                reason: format!(
                    "Monthly CPU quota exceeded: {:.1}/{:.1} hours",
                    usage.cpu_hours_this_month,
                    quota.cpu_hours_per_month
                ),
            });
        }

        if usage.concurrent_tasks >= quota.max_concurrent_tasks {
            return Ok(PolicyDecision::Reject {
                reason: format!(
                    "Concurrent task limit reached: {}/{}",
                    usage.concurrent_tasks,
                    quota.max_concurrent_tasks
                ),
            });
        }

        // Check task priority against quota
        if task.priority > quota.max_priority {
            return Ok(PolicyDecision::Reject {
                reason: format!(
                    "Task priority {:?} exceeds member limit {:?}",
                    task.priority,
                    quota.max_priority
                ),
            });
        }

        // Evaluate scheduling rules
        for rule in &policy.rules {
            match rule {
                SchedulingRule::TimeWindow { allowed_hours, allowed_days, priorities } => {
                    if priorities.contains(&task.priority) {
                        let now = chrono::Utc::now();
                        if !allowed_hours.contains(&(now.hour() as u8)) {
                            return Ok(PolicyDecision::Reject {
                                reason: format!(
                                    "Task not allowed at hour {} (allowed: {:?})",
                                    now.hour(),
                                    allowed_hours
                                ),
                            });
                        }
                        if !allowed_days.contains(&(now.weekday().num_days_from_sunday() as u8)) {
                            return Ok(PolicyDecision::Reject {
                                reason: format!(
                                    "Task not allowed on {} (allowed days: {:?})",
                                    now.weekday(),
                                    allowed_days
                                ),
                            });
                        }
                    }
                }
                // ... other rules
                _ => {}
            }
        }

        Ok(PolicyDecision::Allow {
            adjusted_priority: self.calculate_adjusted_priority(task, submitter, &policy),
            placement_constraints: self.extract_placement_constraints(&policy),
        })
    }

    /// Filter executors based on policy rules
    pub fn filter_executors(
        &self,
        policy: &CoopSchedulingPolicy,
        executors: Vec<ExecutorInfo>,
    ) -> Vec<ExecutorInfo> {
        let mut filtered = executors;

        for rule in &policy.rules {
            match rule {
                SchedulingRule::DataSovereignty { region, .. } => {
                    filtered.retain(|e| e.region.as_ref() == Some(region));
                }
                SchedulingRule::ExecutorFilter { whitelist, blacklist } => {
                    if !whitelist.is_empty() {
                        filtered.retain(|e| whitelist.contains(&e.did));
                    }
                    filtered.retain(|e| !blacklist.contains(&e.did));
                }
                SchedulingRule::RequireCapability { capability, .. } => {
                    filtered.retain(|e| e.capabilities.contains(capability));
                }
                _ => {}
            }
        }

        filtered
    }
}

/// Policy evaluation result
pub enum PolicyDecision {
    /// Allow task with possible adjustments
    Allow {
        /// Adjusted priority based on member rules
        adjusted_priority: TaskPriority,
        /// Placement constraints from policy
        placement_constraints: PlacementConstraints,
    },

    /// Reject task with reason
    Reject {
        reason: String,
    },
}

/// Placement constraints extracted from policy
#[derive(Debug, Clone, Default)]
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
```

### Usage Tracking

```rust
/// Track resource usage per member per cooperative
pub struct UsageTracker {
    /// Usage records indexed by (coop_id, member_did)
    records: Arc<RwLock<HashMap<(String, String), UsageRecord>>>,

    /// Persistent storage backend
    store: Arc<dyn UsageStore>,
}

/// Member usage record
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl UsageTracker {
    /// Record task execution
    pub async fn record_execution(
        &self,
        coop_id: &str,
        member: &Did,
        fuel_used: u64,
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
            last_reset_at: now_millis(),
            updated_at: now_millis(),
        });

        // Check if monthly reset needed
        let now = now_millis();
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

        // Persist to storage
        self.store.save_record(record).await?;

        Ok(())
    }

    /// Increment concurrent task count
    pub async fn task_claimed(
        &self,
        coop_id: &str,
        member: &Did,
    ) -> Result<(), ComputeError> {
        let key = (coop_id.to_string(), member.to_string());
        let mut records = self.records.write().await;

        if let Some(record) = records.get_mut(&key) {
            record.concurrent_tasks += 1;
            record.updated_at = now_millis();
        }

        Ok(())
    }

    /// Decrement concurrent task count
    pub async fn task_completed(
        &self,
        coop_id: &str,
        member: &Did,
    ) -> Result<(), ComputeError> {
        let key = (coop_id.to_string(), member.to_string());
        let mut records = self.records.write().await;

        if let Some(record) = records.get_mut(&key) {
            record.concurrent_tasks = record.concurrent_tasks.saturating_sub(1);
            record.updated_at = now_millis();
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
            last_reset_at: now_millis(),
            updated_at: now_millis(),
        }))
    }

    fn should_reset_monthly(last_reset: u64, now: u64) -> bool {
        // Reset if calendar month changed
        let last = chrono::DateTime::from_timestamp((last_reset / 1000) as i64, 0)
            .unwrap_or_default();
        let current = chrono::DateTime::from_timestamp((now / 1000) as i64, 0)
            .unwrap_or_default();

        last.month() != current.month() || last.year() != current.year()
    }
}
```

## Integration Points

### 1. ComputeActor Integration

```rust
// In ComputeActor::handle_submit
pub async fn handle_submit(&mut self, task: ComputeTask, submitter: Did) -> Result<(), ComputeError> {
    // NEW: Policy check before accepting task
    if let Some(ref policy_manager) = self.policy_manager {
        let decision = policy_manager.check_submission(&task, &submitter).await?;

        match decision {
            PolicyDecision::Reject { reason } => {
                tracing::warn!(
                    task_id = %task.id,
                    submitter = %submitter,
                    reason = %reason,
                    "Task rejected by policy"
                );

                // Emit rejection event
                if let Some(ref event_callback) = self.event_callback {
                    event_callback(ComputeEvent::TaskRejected {
                        task_id: task.id.clone(),
                        submitter: submitter.clone(),
                        reason: reason.clone(),
                    });
                }

                return Err(ComputeError::PolicyViolation(reason));
            }
            PolicyDecision::Allow { adjusted_priority, placement_constraints } => {
                // Apply policy adjustments
                let mut adjusted_task = task;
                adjusted_task.priority = adjusted_priority;
                adjusted_task.placement_constraints = Some(placement_constraints);

                // Continue with normal flow
                self.task_manager.add_task(adjusted_task)?;
            }
        }
    } else {
        // No policy manager, proceed normally
        self.task_manager.add_task(task)?;
    }

    Ok(())
}
```

### 2. Placement Scoring Integration

```rust
// In DefaultPlacementPolicy::score
pub fn score(&self, request: &PlacementRequest, offer: &PlacementOffer) -> f64 {
    let mut score = 0.0;

    // ... existing scoring (trust, capacity, queue, RTT, data, hints) ...

    // NEW: Apply policy constraints
    if let Some(ref constraints) = request.task.placement_constraints {
        // Required region check
        if let Some(ref required_region) = constraints.required_region {
            if offer.executor.region.as_ref() != Some(required_region) {
                return 0.0; // Hard reject
            }
        }

        // Whitelist check
        if !constraints.allowed_executors.is_empty() {
            if !constraints.allowed_executors.contains(&offer.executor.did) {
                return 0.0; // Hard reject
            }
        }

        // Blacklist check
        if constraints.forbidden_executors.contains(&offer.executor.did) {
            return 0.0; // Hard reject
        }

        // Capability check
        for required_cap in &constraints.required_capabilities {
            if !offer.executor.capabilities.contains(required_cap) {
                return 0.0; // Hard reject
            }
        }
    }

    score
}
```

### 3. Governance Integration (Phase 13)

```rust
/// Proposal to update cooperative scheduling policy
pub struct UpdateSchedulingPolicyProposal {
    pub coop_id: String,
    pub new_policy: CoopSchedulingPolicy,
    pub effective_date: Option<u64>,
}

// In governance handler
pub async fn handle_proposal_executed(&self, proposal: &Proposal) -> Result<(), ComputeError> {
    if let ProposalKind::ConfigChange(ConfigChange::SchedulingPolicy { policy }) = &proposal.kind {
        // Apply new policy
        self.policy_manager.update_policy(&proposal.domain_id, policy.clone()).await?;

        tracing::info!(
            coop_id = %proposal.domain_id,
            proposal_id = %proposal.id,
            "Scheduling policy updated via governance"
        );
    }

    Ok(())
}
```

## Example Policies

### Timebank Policy

```rust
CoopSchedulingPolicy {
    coop_id: "timebank:seattle".to_string(),
    governance_domain: Some("timebank:seattle:governance".to_string()),
    rules: vec![
        // Fair-share: max 10 CPU-hours per member per month
    ],
    member_quotas: HashMap::new(),
    default_quota: MemberQuota {
        cpu_hours_per_month: 10.0,
        max_concurrent_tasks: 3,
        max_priority: TaskPriority::Normal,
        credits_per_month: None, // No credit limits (timebank uses hours)
    },
    enforcement_mode: EnforcementMode::Strict,
}
```

### Research Coop Policy (GDPR)

```rust
CoopSchedulingPolicy {
    coop_id: "research:biolab".to_string(),
    governance_domain: Some("research:biolab:council".to_string()),
    rules: vec![
        SchedulingRule::DataSovereignty {
            region: "eu-central".to_string(),
            tags: vec!["patient-data".to_string(), "clinical-trial".to_string()],
        },
        SchedulingRule::RequireCapability {
            capability: "hipaa-compliant".to_string(),
            min_version: Some("1.0".to_string()),
        },
    ],
    member_quotas: HashMap::new(), // No quotas, research-focused
    default_quota: MemberQuota {
        cpu_hours_per_month: 100.0, // Generous limit
        max_concurrent_tasks: 10,
        max_priority: TaskPriority::High,
        credits_per_month: Some(1000), // 1000 credits/month
    },
    enforcement_mode: EnforcementMode::Strict,
}
```

### Housing Coop Policy (Critical Infrastructure)

```rust
CoopSchedulingPolicy {
    coop_id: "housing:greenfield".to_string(),
    governance_domain: Some("housing:greenfield:board".to_string()),
    rules: vec![
        // Building automation gets priority
        SchedulingRule::MemberPriority {
            member: "did:icn:building-automation-system".to_string(),
            multiplier: 3.0, // 3x priority
        },
        // Maintenance tasks run overnight only
        SchedulingRule::TimeWindow {
            allowed_hours: vec![0, 1, 2, 3, 4, 5, 22, 23], // 10pm - 6am
            allowed_days: vec![0, 1, 2, 3, 4, 5, 6], // Every day
            priorities: vec![TaskPriority::Low], // Only low-priority tasks affected
        },
    ],
    member_quotas: HashMap::new(),
    default_quota: MemberQuota {
        cpu_hours_per_month: 5.0, // Limited for residential use
        max_concurrent_tasks: 2,
        max_priority: TaskPriority::Normal,
        credits_per_month: Some(50),
    },
    enforcement_mode: EnforcementMode::Strict,
}
```

## Prometheus Metrics

```rust
/// Policy enforcement metrics
pub fn policy_violations_total_inc(coop_id: &str, rule_type: &str);
pub fn quota_exceeded_total_inc(coop_id: &str, quota_type: &str);
pub fn tasks_rejected_policy_total_inc(coop_id: &str);
pub fn policy_evaluation_duration_observe(duration_ms: f64);

/// Usage tracking metrics
pub fn member_cpu_hours_gauge(coop_id: &str, member_did: &str, hours: f64);
pub fn member_tasks_completed_total_inc(coop_id: &str, member_did: &str);
pub fn member_credits_spent_total_add(coop_id: &str, member_did: &str, credits: u64);
```

## Implementation Plan

### Week 1: Core Types & Policy Manager (Current)
- ✅ Design architecture (this document)
- [ ] Implement `CoopSchedulingPolicy`, `SchedulingRule`, `MemberQuota` types
- [ ] Implement `PolicyManager` with basic enforcement
- [ ] Unit tests for policy evaluation

### Week 2: Usage Tracking & Integration
- [ ] Implement `UsageTracker` with monthly reset logic
- [ ] Integrate with `ComputeActor` for task submission checks
- [ ] Integrate with placement scoring for constraint enforcement
- [ ] Add Prometheus metrics

### Week 3: Governance Integration & CLI
- [ ] Connect to Phase 13 governance for policy updates
- [ ] Add `icnctl policy` commands (create, update, show, list)
- [ ] Add `icnctl quota` commands (show usage, set limits)
- [ ] Integration tests with governance proposals

### Week 4: Testing & Documentation
- [ ] Comprehensive test suite (quota enforcement, time windows, data sovereignty)
- [ ] Multi-cooperative integration test
- [ ] Performance testing (policy evaluation <5ms)
- [ ] Complete documentation with examples
- [ ] Dev journal completion

## Success Criteria

- [ ] Policy evaluation adds <5ms overhead to task submission
- [ ] Quota violations are detected and logged
- [ ] Data sovereignty rules prevent out-of-region execution
- [ ] Time windows correctly restrict task scheduling
- [ ] Usage tracking accurately reflects resource consumption
- [ ] Governance proposals can update policies
- [ ] 90% test coverage for policy enforcement
- [ ] Documentation includes 3+ real-world policy examples

## Testing Strategy

### Unit Tests
- Policy evaluation with various rules
- Quota enforcement (CPU hours, concurrent tasks, credits)
- Time window calculations
- Executor filtering

### Integration Tests
- Task submission → policy check → placement
- Usage tracking across multiple tasks
- Monthly quota reset
- Governance proposal → policy update

### Performance Tests
- Policy evaluation latency (<5ms target)
- Usage tracker throughput (1000+ updates/sec)
- Memory overhead per policy (target <1KB)

## Security Considerations

### Quota Bypass Prevention
- **All enforcement server-side** - clients cannot self-report usage
- **Atomic concurrent task tracking** - prevent race conditions
- **Cryptographic verification** - usage records signed by executor DIDs

### Policy Tampering Prevention
- **Governance-controlled updates** - only proposals can change policies
- **Audit trail** - all policy changes logged
- **Rollback capability** - can revert to previous policy version

### Privacy
- **Usage data scoped per-cooperative** - no cross-coop visibility
- **Aggregated metrics only** - individual task details not exposed
- **GDPR-compliant retention** - usage records expire after 90 days

## Open Questions

1. **Monthly reset timing**: Should it be calendar month or rolling 30 days?
   - **Decision**: Calendar month (simpler, aligns with billing cycles)

2. **Credit tracking**: Should this be here or in ledger?
   - **Decision**: Track in both - policy uses it for quotas, ledger for accounting

3. **Enforcement granularity**: Should violations be immediate reject or queued?
   - **Decision**: Immediate reject with clear error message (fail-fast)

4. **Policy versioning**: How to handle policy updates mid-month?
   - **Decision**: New policy applies to new tasks, running tasks continue with old policy

## Next Steps

1. **Implement core types** in `icn-compute/src/policy.rs`
2. **Add PolicyManager** integration to ComputeActor
3. **Create unit tests** for policy evaluation
4. **Begin usage tracker** implementation

---

**Author**: Claude Code + Matt
**Created**: 2025-11-24
**Status**: ✅ Complete (Weeks 1-3)
**Next**: Week 4 - Governance integration and CLI commands
