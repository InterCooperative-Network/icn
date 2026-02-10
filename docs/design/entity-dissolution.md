# Entity Dissolution Workflow

This document describes the governance-approved dissolution workflow for entities in ICN, which addresses the circular dependency issue that previously made entity deletion impossible.

## Overview

The dissolution workflow provides a formal process for safely winding down cooperatives, federations, and other entities. It includes:

- **Governance approval requirements**: Dissolution must be approved via a governance proposal
- **Mandatory waiting period**: Default 30-day notice period (configurable per entity)
- **Asset distribution**: Time for members to handle assets, settle accounts, and prepare for closure
- **Comprehensive audit trail**: All dissolution steps are logged for compliance and transparency
- **Cancellation capability**: Dissolution can be cancelled before the waiting period expires

## The Problem: Circular Dependency

Previously, entity deletion was impossible due to two protection mechanisms:

1. **Last founder protection**: Can't remove the last founder from an entity (returns 400)
2. **Active members protection**: Can't delete an entity that has members (returns 400)

This created a circular dependency:
```
1. Entity has members [Alice (founder)]
2. Alice tries to delete entity → 400 "Cannot remove entity with active members"
3. Alice tries to remove herself → 400 "Cannot remove last founder"
```

## The Solution: Dissolution Workflow

The dissolution workflow breaks this circular dependency by introducing a multi-step process:

### Step 1: Initiate Dissolution

**Endpoint**: `POST /entities/:id/dissolution`

**Requirements**:
- Caller must be a Founder or BoardMember
- Entity must be in `Active` status (not `Suspended`, `Forming`, etc.)
- Must provide a governance `proposal_id` (from an approved proposal)
- Must provide a `reason` for dissolution

**Optional Parameters**:
- `waiting_period_seconds`: Override the default 30-day waiting period

**Effect**:
- Entity status changes to `Dissolving { started_at: timestamp }`
- Audit record created: `DissolutionInitiated`
- Members are notified (via normal entity status checks)
- Completion date calculated: `now + waiting_period`

**Response**:
```json
{
  "entity_id": "entity:icn:cooperative:food-coop",
  "status": "dissolving",
  "initiated_at": 1706131200,
  "completion_date": 1708723200,
  "proposal_id": "proposal-123"
}
```

### Step 2: Member Removal

During the waiting period:
- The last founder protection is **still enforced**
- Members must voluntarily leave or be removed by other founders
- All members except the last founder must be removed
- The last founder can only be removed after dissolution is completed

This ensures orderly wind-down:
1. Asset distribution occurs
2. Accounts are settled
3. Members leave voluntarily or by governance decision
4. Eventually only one founder remains to complete dissolution

### Step 3: Complete Dissolution

**Endpoint**: `POST /entities/:id/dissolution/complete`

**Requirements**:
- Caller must be a Founder or BoardMember
- Entity must be in `Dissolving` status
- Waiting period must have elapsed
- Entity must have **no members** remaining

**Effect**:
- Entity is permanently deleted from the registry
- Audit record created: `DissolutionCompleted`
- Entity status changes to `Deleted` (tombstone for sync)

**Response**: `204 No Content`

### Optional: Cancel Dissolution

**Endpoint**: `DELETE /entities/:id/dissolution`

**Requirements**:
- Caller must be a Founder or BoardMember
- Entity must be in `Dissolving` status
- Can be called at any time before completion

**Effect**:
- Entity status reverts to `Active`
- Audit record created: `DissolutionCancelled`
- Members remain in place
- Entity resumes normal operations

**Response**: `204 No Content`

## Entity Status Lifecycle

```
Active
  │
  │ POST /entities/:id/dissolution
  ▼
Dissolving { started_at }
  │
  ├─── DELETE /entities/:id/dissolution ──▶ Active (cancelled)
  │
  │ POST /entities/:id/dissolution/complete
  │ (after waiting period, no members)
  ▼
Deleted (tombstone)
```

## Audit Trail

All dissolution operations create audit records:

### DissolutionInitiated
```json
{
  "operation": {
    "DissolutionInitiated": {
      "proposal_id": "proposal-123",
      "completion_date": 1708723200,
      "reason": "Cooperative wind-down due to market conditions"
    }
  },
  "performed_by": "entity:icn:individual:z5TC...",
  "performed_at": 1706131200,
  "proposal_id": "proposal-123",
  "notes": "Cooperative wind-down due to market conditions"
}
```

### DissolutionCompleted
```json
{
  "operation": {
    "DissolutionCompleted": {
      "proposal_id": "proposal-123"
    }
  },
  "performed_by": "entity:icn:individual:z5TC...",
  "performed_at": 1708723500,
  "proposal_id": "proposal-123"
}
```

### DissolutionCancelled
```json
{
  "operation": {
    "DissolutionCancelled": {
      "reason": "Members voted to continue operations"
    }
  },
  "performed_by": "entity:icn:individual:z5TC...",
  "performed_at": 1706735000,
  "notes": "Dissolution cancelled"
}
```

## Example: Complete Dissolution Flow

```bash
# 1. Create and approve a dissolution proposal (via Governance API)
curl -X POST https://icn.coop/governance/proposals \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "domain_id": "food-coop-domain",
    "title": "Dissolve Food Cooperative",
    "description": "Wind down operations due to market conditions",
    "payload": { "Membership": {} }
  }'

# ... vote on proposal until approved ...

# 2. Initiate dissolution with 30-day waiting period
curl -X POST https://icn.coop/entities/entity:icn:cooperative:food-coop/dissolution \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "proposal_id": "proposal-123",
    "reason": "Approved by governance for market reasons",
    "waiting_period_seconds": 2592000
  }'

# Response:
# {
#   "entity_id": "entity:icn:cooperative:food-coop",
#   "status": "dissolving",
#   "initiated_at": 1706131200,
#   "completion_date": 1708723200,
#   "proposal_id": "proposal-123"
# }

# 3. During waiting period: Remove members
# (Members voluntarily leave or are removed by founders)
curl -X DELETE https://icn.coop/entities/entity:icn:cooperative:food-coop/members/entity:icn:individual:z5TC... \
  -H "Authorization: Bearer $TOKEN"

# 4. After 30 days and all members removed: Complete dissolution
curl -X POST https://icn.coop/entities/entity:icn:cooperative:food-coop/dissolution/complete \
  -H "Authorization: Bearer $TOKEN"

# Response: 204 No Content
# Entity is now deleted
```

## Optional: Cancellation Example

```bash
# If circumstances change before completion:
curl -X DELETE https://icn.coop/entities/entity:icn:cooperative:food-coop/dissolution \
  -H "Authorization: Bearer $TOKEN"

# Response: 204 No Content
# Entity status reverted to Active
```

## Authorization

All dissolution operations require:
- **JWT scope**: `entity:write`
- **Entity role**: Founder or BoardMember

Members with other roles (Worker, Consumer, Patron) cannot initiate or complete dissolution.

## Error Handling

Common errors:

### 400 Bad Request
- Entity not in Active status (for initiation)
- Entity not in Dissolving status (for completion/cancellation)
- Waiting period has not elapsed (for completion)
- Entity still has members (for completion)

### 403 Forbidden
- Caller is not a Founder or BoardMember
- Caller's token lacks `entity:write` scope

### 404 Not Found
- Entity does not exist

### 500 Internal Server Error
- Dissolution audit trail is missing expected records (e.g., `DissolutionInitiated` event or `completion_date`) - this indicates a data consistency issue
- System time error (system clock before UNIX epoch)
- Internal storage or query failures

## Rollback and Compensation

All dissolution operations include compensation logic for audit failures:

1. **Operation succeeds** → Audit log written
2. **Audit write fails** → Operation is rolled back
3. **Rollback fails** → Error logged, metrics incremented, CRITICAL alert

This ensures:
- Operations are atomic (succeed or fail together)
- Audit trail is always consistent
- Data loss is prevented
- Critical failures are visible in monitoring

## Future Enhancements

Potential future additions:

1. **Automated asset distribution**: Integration with ledger for pro-rata payouts
2. **Notification system**: Email/webhook notifications to members
3. **Grace period extensions**: Allow extending the waiting period
4. **Partial dissolutions**: Dissolve branches or sub-units
5. **Merger workflows**: Special dissolution type for mergers
6. **Governance templates**: Pre-defined dissolution proposals for common scenarios

## Related Documentation

- **Entity Lifecycle**: See [../ARCHITECTURE.md](../ARCHITECTURE.md) for entity status transitions
- **Governance System**: See `docs/governance-primitives.md` for proposal workflows
- **Audit System**: See `icn/crates/icn-gateway/src/entity_audit.rs` for audit implementation
- **API Reference**: See `docs/api/openapi.generated.yaml` for full API specs
