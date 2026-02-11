# Cooperative Scheduling Policy Examples

This directory contains example policy configurations for ICN's cooperative scheduling system (Phase 16E).

## Overview

Policies enable cooperatives to define:
- **Resource quotas** per member (CPU hours, concurrent tasks, credits)
- **Priority rules** for specific members or use cases
- **Data sovereignty** constraints (GDPR, regional requirements)
- **Time windows** for off-peak scheduling
- **Executor filtering** (whitelist/blacklist trusted nodes)

## Example Policies

### 1. Basic Cooperative (`basic-cooperative.json`)

A simple starter policy with default quotas for all members.

**Use case:** Small cooperative with equal resource allocation

**Features:**
- 50 CPU hours/month per member
- 5 concurrent tasks max
- High priority allowed
- 500 credits/month budget

```bash
icnctl policy set --coop-id food-coop-001 --policy docs/examples/policies/basic-cooperative.json
```

### 2. GDPR Compliant (`gdpr-compliant.json`)

Healthcare cooperative with strict data sovereignty and encryption requirements.

**Use case:** Healthcare data processing with EU residency requirements

**Features:**
- Data must remain in `eu-central` region
- Requires `gdpr-compliant` and `encryption` capabilities
- Strict enforcement mode
- Higher quotas (100 CPU hours/month)

```bash
icnctl policy set --coop-id healthcare-coop --policy docs/examples/policies/gdpr-compliant.json
```

### 3. Tiered Membership (`tiered-membership.json`)

Housing cooperative with premium tiers for critical infrastructure.

**Use case:** Multi-tier membership with building automation and emergency services

**Features:**
- **Building automation system:** 200 CPU hours, 2x priority multiplier
- **Emergency services:** 500 CPU hours, 3x priority multiplier, unlimited credits
- **Guest members:** 10 CPU hours, low priority only
- **Regular members:** 50 CPU hours (default)

```bash
icnctl policy set --coop-id housing-coop --policy docs/examples/policies/tiered-membership.json
```

### 4. Time-Restricted (`time-restricted.json`)

Research lab with off-peak scheduling to avoid business hours.

**Use case:** Batch processing during nights and weekends

**Features:**
- Low/Normal priority tasks: Only nights (8pm-7am) on weekdays
- High priority tasks: Anytime on weekends (Saturday/Sunday)
- 100 CPU hours/month per researcher

**Hours reference:**
- `0-6, 20-23` = 8pm-7am (off-peak weekday hours)
- Days: `0=Sunday, 6=Saturday`

```bash
icnctl policy set --coop-id research-lab --policy docs/examples/policies/time-restricted.json
```

### 5. Executor Filtering (`executor-filtering.json`)

Security-focused cooperative with trusted executor whitelist.

**Use case:** High-security workloads with vetted execution nodes

**Features:**
- Whitelist of 3 trusted executors
- Blacklist for compromised/unreliable nodes
- Requires `secure-execution` capability v2.0+
- Strict enforcement mode

```bash
icnctl policy set --coop-id security-focused-coop --policy docs/examples/policies/executor-filtering.json
```

### 6. Permissive Development (`permissive-development.json`)

Development sandbox with relaxed limits and permissive enforcement.

**Use case:** Testing and development environment

**Features:**
- High quotas (1000 CPU hours, 100 concurrent tasks)
- Permissive mode (violations logged but not rejected)
- No governance domain (manual policy updates)
- Critical priority allowed

```bash
icnctl policy set --coop-id dev-sandbox --policy docs/examples/policies/permissive-development.json
```

## Policy Structure

```json
{
  "coop_id": "unique-cooperative-id",
  "governance_domain": "governance:domain-id",  // Optional: Phase 13 integration
  "rules": [
    // Scheduling rules (see types below)
  ],
  "member_quotas": {
    "did:icn:member-did": {
      "cpu_hours_per_month": 100.0,
      "max_concurrent_tasks": 10,
      "max_priority": "High",        // Low, Normal, High, Critical
      "credits_per_month": 1000      // null for unlimited
    }
  },
  "default_quota": {
    // Default quota for members without specific quotas
  },
  "enforcement_mode": "Strict"  // Strict, Permissive, Monitoring
}
```

## Rule Types

### DataSovereignty
Restrict execution to specific regions (GDPR, data residency laws).

```json
{
  "DataSovereignty": {
    "region": "eu-central",
    "tags": ["sensitive-data"]
  }
}
```

### TimeWindow
Define allowed execution hours (UTC) and days of week.

```json
{
  "TimeWindow": {
    "allowed_hours": [0, 1, 2, 3, 4, 5, 6, 20, 21, 22, 23],
    "allowed_days": [0, 1, 2, 3, 4, 5, 6],  // 0=Sunday, 6=Saturday
    "priorities": ["Low", "Normal"]
  }
}
```

### MemberPriority
Adjust priority for specific members (e.g., critical infrastructure).

```json
{
  "MemberPriority": {
    "member": "did:icn:building-automation",
    "multiplier": 2.0  // 0.5 = half priority, 2.0 = double priority
  }
}
```

### RequireCapability
Enforce executor capabilities (security, hardware features).

```json
{
  "RequireCapability": {
    "capability": "gdpr-compliant",
    "min_version": "1.0"  // Optional version requirement
  }
}
```

### ExecutorFilter
Whitelist/blacklist specific executor nodes.

```json
{
  "ExecutorFilter": {
    "whitelist": ["did:icn:trusted-1", "did:icn:trusted-2"],
    "blacklist": ["did:icn:unreliable-node"]
  }
}
```

## Enforcement Modes

### Strict
Reject tasks that violate policy rules or quotas.

**Use when:** Production deployments, compliance requirements

### Permissive
Log policy violations but allow execution.

**Use when:** Gradual policy rollout, testing new rules

### Monitoring
Track violations without any enforcement.

**Use when:** Analyzing usage patterns before setting limits

## CLI Commands

```bash
# Set a policy
icnctl policy set --coop-id <id> --policy <path/to/policy.json>

# View current policy
icnctl policy show --coop-id <id>

# List all policies
icnctl policy list

# Remove a policy
icnctl policy remove --coop-id <id>

# Check member usage
icnctl quota show --coop-id <id> --member <did>

# List all member usage
icnctl quota list --coop-id <id>
```

## Governance Integration (Phase 13)

Policies can be linked to governance domains for democratic updates:

```json
{
  "coop_id": "food-coop",
  "governance_domain": "governance:food-coop"
}
```

This enables:
- Proposal-based policy changes
- Member voting on resource limits
- Transparent policy evolution
- Audit trail of policy modifications

## Best Practices

1. **Start permissive:** Use `Permissive` mode during policy development
2. **Monitor first:** Use `Monitoring` mode to understand usage patterns
3. **Iterate gradually:** Adjust quotas based on actual usage data
4. **Document rationale:** Include governance domain for transparency
5. **Test thoroughly:** Use `dev-sandbox` for testing policy changes
6. **Security-first:** Use `ExecutorFilter` for sensitive workloads
7. **Compliance:** Apply `DataSovereignty` rules for regulated data

## Metrics

Policy enforcement emits Prometheus metrics:
- `icn_compute_policy_violations_total{coop_id, reason}`
- `icn_compute_quota_exceeded_total{coop_id, member_did, quota_type}`
- `icn_compute_priority_adjustments_total{coop_id, from, to}`
- `icn_compute_member_cpu_hours{coop_id, member_did}`
- `icn_compute_member_concurrent_tasks{coop_id, member_did}`
- `icn_compute_member_credits_spent{coop_id, member_did}`

Access metrics at `http://localhost:9090/metrics`

## Further Reading

- [Cooperative Scheduling Design](../../ARCHITECTURE.md#phase-16e-cooperative-scheduling)
- [Governance Primitives](../../design/governance/governance-primitives.md)
- [Economic Safety Rails](../../ARCHITECTURE.md#phase-12-economic-safety)
