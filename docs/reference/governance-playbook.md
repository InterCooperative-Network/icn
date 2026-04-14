# Governance Playbook

Common decision patterns for institutions on ICN. Each pattern maps to an ICN `ProposalPayload` variant and includes the typical governance flow, threshold guidance, and API example.

Use these as templates when your institution encounters these decision types.

---

## Budget Decisions

### Annual Budget Approval

**When**: Start of fiscal year or planning cycle.
**Payload**: `Allocation`
**Typical threshold**: Two-thirds supermajority of steering/board.
**Who votes**: Steering committee, board, or full membership (depends on charter).

```json
{
  "title": "2026 Annual Operating Budget",
  "payload": {
    "type": "Allocation",
    "pool_amount": 10000,
    "unit": "USD",
    "options": [
      { "name": "programs", "amount": 4000, "description": "Core program delivery" },
      { "name": "operations", "amount": 3000, "description": "Rent, utilities, admin" },
      { "name": "outreach", "amount": 1500, "description": "Marketing and recruitment" },
      { "name": "reserve", "amount": 1500, "description": "Emergency contingency" }
    ],
    "purpose": "Allocate operating funds for calendar year 2026"
  }
}
```

**Flow**: Finance committee drafts → deliberation period → board/steering votes → on acceptance, allocations become ledger entries.

### Single Expenditure Approval

**When**: A specific expense exceeds committee delegation authority.
**Payload**: `Budget`
**Typical threshold**: Simple majority of governing body.

```json
{
  "title": "Approve venue deposit — Albany Convention Center",
  "payload": {
    "type": "Budget",
    "amount": 1500,
    "currency": "USD",
    "recipient": "did:icn:z6Mk...",
    "purpose": "Venue deposit for 2026 summit. Non-refundable. 200-person capacity, fully accessible."
  }
}
```

**Flow**: Committee researches options → recommends one → proposal to governing body → vote → on acceptance, ledger settlement authorized.

### Budget Amendment

**When**: Committee needs more than originally allocated.
**Payload**: `Budget` (additional allocation to specific account)
**Typical threshold**: Same as budget approval.

```json
{
  "title": "Marketing budget amendment — additional $300 for print materials",
  "payload": {
    "type": "Budget",
    "amount": 300,
    "currency": "USD",
    "recipient": "did:icn:z6Mk...",
    "purpose": "Print run for summit brochures. Original $500 allocation exhausted on digital ads."
  }
}
```

---

## Membership Decisions

### Admit New Member

**When**: A person or organization wants to join.
**Payload**: `Membership`
**Typical threshold**: Simple majority (for individuals), two-thirds (for organizations joining a federation).

```json
{
  "title": "Admit GreenStar Food Co-op to federation",
  "description": "GreenStar (Ithaca, NY) is a consumer food cooperative with 12,000 members. They've participated in the last 3 summits and align with federation values.",
  "payload": {
    "type": "Membership",
    "action": "Add",
    "member": "did:icn:z6Mk..."
  }
}
```

**Flow**: Applicant submits request → membership committee reviews → proposal created → members vote → on acceptance, entity added to membership roster.

### Remove Member

**When**: Member violates charter or becomes inactive.
**Payload**: `Membership`
**Typical threshold**: Two-thirds supermajority with extended deliberation.

```json
{
  "title": "Remove inactive member organization",
  "description": "Organization has not participated in governance or sent delegates for 2 consecutive years. Per charter section 4.3, this triggers removal review.",
  "payload": {
    "type": "Membership",
    "action": "Remove",
    "member": "did:icn:z6Mk..."
  }
}
```

---

## Governance Configuration

### Change Governance Parameters

**When**: Adjusting thresholds, voting periods, quorum requirements.
**Payload**: `ConfigChange`
**Typical threshold**: Two-thirds supermajority with deliberation period.

```json
{
  "title": "Lower committee quorum from 50% to 40%",
  "description": "Committee attendance has averaged 45%. Current 50% quorum means decisions frequently fail. 40% still ensures representative participation.",
  "payload": {
    "type": "ConfigChange",
    "new_config": "{\"committee_quorum\": 0.4}"
  }
}
```

### Form New Committee

**When**: Creating a new working group or committee.
**Payload**: `Text` (followed by entity creation on acceptance)
**Typical threshold**: Simple majority of governing body.

```json
{
  "title": "Create Volunteer Coordination Committee",
  "description": "Scope: Recruit, train, and manage event-day volunteers. Authority: Autonomous volunteer recruitment and scheduling. Budget: $200 from operations allocation. Initial members: [names]. Coordinator: [name].",
  "payload": {
    "type": "Text",
    "body": "Establish a Volunteer Coordination Committee under the organizing cooperative. Scope, authority, budget, and initial membership as described."
  }
}
```

**Post-acceptance action**: Create `Community` entity with `parent_id` pointing to the parent co-op, assign members.

---

## Selection Decisions

### Venue / Vendor Selection

**When**: Choosing between options for a significant commitment.
**Payload**: `Text` (structured comparison in body)
**Typical threshold**: Simple majority.

```json
{
  "title": "Select 2026 summit venue",
  "payload": {
    "type": "Text",
    "body": "## Options\n\n### Option A: Albany Convention Center\n- Cost: $1,500 deposit, $3,000 total\n- Capacity: 200\n- Accessibility: Full ADA compliance\n- Transit: Amtrak station 0.5 mi\n\n### Option B: Buffalo Conference Hall\n- Cost: $1,200 deposit, $2,500 total\n- Capacity: 150\n- Accessibility: Limited (2nd floor, elevator available)\n- Transit: Metro Rail 1 mi\n\n### Recommendation: Option A (Albany)\nHigher cost justified by capacity, accessibility, and transit access."
  }
}
```

### Speaker / Program Approval

**When**: Approving the overall program structure (not individual sessions, which committees handle autonomously).
**Payload**: `Text`
**Typical threshold**: Simple majority.

```json
{
  "title": "Approve 2026 summit program structure",
  "payload": {
    "type": "Text",
    "body": "## Proposed Structure\n- 1 keynote (Saturday AM)\n- 4 tracks: Worker Co-ops, Food Systems, Housing, Cross-Sector\n- 16 breakout sessions (4 per track)\n- 2 emergent/unconference slots\n- Panel: State of NY Cooperatives\n\nContent committee has autonomous authority to fill sessions within this structure."
  }
}
```

---

## Emergency Decisions

### Freeze Member

**When**: Account compromise, malicious behavior, or safety concern.
**Payload**: `FreezeMember`
**Typical threshold**: Two-thirds supermajority with fast voting period (24 hours).

```json
{
  "title": "Emergency freeze — suspected compromised account",
  "payload": {
    "type": "FreezeMember",
    "member": "did:icn:z6Mk...",
    "reason": "Unauthorized transactions detected from this DID. Freezing pending investigation.",
    "duration_seconds": 604800
  }
}
```

**Duration**: `604800` = 7 days. Set `null` for indefinite (until explicitly unfrozen).

### Veto a Proposal

**When**: A passed or in-progress proposal needs to be overridden.
**Payload**: `VetoProposal`
**Typical threshold**: Supermajority.

```json
{
  "title": "Veto proposal #42 — venue contract",
  "payload": {
    "type": "VetoProposal",
    "target_proposal_id": "prop-42",
    "reason": "New information: venue has unresolved ADA violations. Cannot proceed until resolved."
  }
}
```

### Force Close a Proposal

**When**: Proposal must be halted immediately.
**Payload**: `ForceCloseProposal`
**Typical threshold**: Highest threshold in charter.

```json
{
  "title": "Force close budget proposal — superseded",
  "payload": {
    "type": "ForceCloseProposal",
    "target_proposal_id": "prop-38",
    "reason": "Grant award changed total available funds. New budget proposal (prop-45) supersedes.",
    "forced_outcome": "Rejected"
  }
}
```

---

## Resolutions and Records

### Text Resolution

**When**: Recording a decision, statement, or policy that doesn't require a specific system action.
**Payload**: `Text`
**Typical threshold**: Simple majority.

```json
{
  "title": "Resolution: Affirm commitment to cooperative principles",
  "payload": {
    "type": "Text",
    "body": "The federation affirms its commitment to the seven cooperative principles as defined by the ICA. All member organizations are expected to operate in accordance with these principles."
  }
}
```

### Decision Record

**When**: Recording a decision made in a meeting for institutional memory.
**Payload**: `Text`
**Typical threshold**: Simple majority or consensus.

```json
{
  "title": "Decision record: 2026 summit date confirmed October 18",
  "payload": {
    "type": "Text",
    "body": "Steering committee confirmed October 18, 2026 as the summit date. Factors: venue availability, academic calendar, weather, conflict check with other cooperative events."
  }
}
```

---

## Proposal Best Practices

1. **Title should be a complete sentence**: "Approve 2026 budget" not "Budget"
2. **Description explains why, not just what**: Include context, alternatives considered, and rationale
3. **Use deliberation periods for significant decisions**: Let people read, ask questions, and discuss before voting opens
4. **Link related proposals**: Reference prior decisions that inform this one
5. **Keep scope focused**: One decision per proposal. Don't bundle unrelated items.
6. **Record the outcome context**: Even for `Text` proposals, explain what changes as a result

---

## See Also

- [CCL Charter Templates](ccl-charter-templates.md) — Governance parameters for different institution types
- [Entity Topology Patterns](entity-topology-patterns.md) — How to structure your organization
- [Institution Setup Guide](../guides/institution-setup.md) — Full bootstrap walkthrough
