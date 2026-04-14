# CCL Charter Templates

Starter charters for common institutional types on ICN. Each template defines governance parameters, committee delegation, trust thresholds, and budget authority. Customize to fit your institution.

These are written in CCL (Cooperative Contract Language) YAML format. When ratified via a `Charter` governance proposal, they're deployed to the `CharterPolicyOracle` and enforced by the kernel.

---

## Template 1: Federation of Cooperatives

Use when: A network of independent cooperatives coordinating through a shared governance layer. Each member co-op retains sovereignty. The federation governs shared concerns.

**Reference implementation: NYCN**

```yaml
# federation-charter.yaml
charter:
  name: "Regional Cooperative Federation"
  version: 1
  entity_type: federation
  
  # ─── Federation Governance ───────────────────────────
  governance:
    # Who can vote on federation-level decisions
    eligible_voters: federated_members
    
    decisions:
      # Admitting new member organizations
      membership_admission:
        threshold: simple_majority
        quorum: 0.5
        deliberation_period_hours: 72
        voting_period_hours: 168  # 7 days
      
      # Changing the charter itself
      charter_amendment:
        threshold: supermajority_two_thirds
        quorum: 0.5
        deliberation_period_hours: 168  # 7 days
        voting_period_hours: 336  # 14 days
      
      # Removing a member organization
      member_removal:
        threshold: supermajority_two_thirds
        quorum: 0.6
        deliberation_period_hours: 168
        voting_period_hours: 336
      
      # Federation-wide initiatives and resolutions
      initiative:
        threshold: simple_majority
        quorum: 0.4
        deliberation_period_hours: 72
        voting_period_hours: 168

  # ─── Operational Body ────────────────────────────────
  # The entity that executes federation mandates
  operations:
    entity_type: cooperative
    governance:
      eligible_voters: steering_members
      
      decisions:
        budget_approval:
          threshold: supermajority_two_thirds
          quorum: 0.5
          deliberation_period_hours: 72
          voting_period_hours: 168
        
        venue_selection:
          threshold: simple_majority
          quorum: 0.5
          voting_period_hours: 120  # 5 days
        
        program_approval:
          threshold: simple_majority
          quorum: 0.5
          voting_period_hours: 120
        
        spending_above_threshold:
          threshold: simple_majority
          quorum: 0.5
          voting_period_hours: 72  # 3 days
        
        new_committee:
          threshold: simple_majority
          quorum: 0.5
          deliberation_period_hours: 48
          voting_period_hours: 120
        
        emergency_freeze:
          threshold: supermajority_two_thirds
          quorum: 0.3  # Lower quorum for emergencies
          voting_period_hours: 24  # Fast action

  # ─── Committee Delegation ────────────────────────────
  committees:
    defaults:
      threshold: simple_majority
      quorum: 0.5
      voting_period_hours: 72
    
    delegation_rules:
      # What committees can decide without escalating
      - committee: content
        autonomous_scope:
          - "Approve individual sessions"
          - "Assign speakers to sessions"
          - "Set session schedule within approved structure"
        escalation_required:
          - "Overall program structure (tracks, keynote, session count)"
          - "Speaker fees above $200"
      
      - committee: finance
        autonomous_scope:
          - "Accept sponsorships up to $500"
          - "Process routine expenses within allocation"
          - "Generate financial reports"
        escalation_required:
          - "Sponsorships above $500"
          - "Budget amendments"
          - "New expense categories"
      
      - committee: logistics
        autonomous_scope:
          - "Vendor selection within budget"
          - "Day-of operational decisions"
        escalation_required:
          - "Venue selection"
          - "Contracts above $1,000"
      
      - committee: marketing
        autonomous_scope:
          - "Campaign execution within budget"
          - "Social media and email content"
          - "Design and branding decisions"
        escalation_required:
          - "Spending above allocation"
          - "Partnership agreements"

  # ─── Trust Thresholds ────────────────────────────────
  trust:
    thresholds:
      propose_discussion: 0.0       # Anyone can start a discussion
      vote_committee: 0.1           # Minimal participation required
      propose_budget_change: 0.5    # Established contributor
      committee_coordinator: 0.5    # Proven track record
      steering_membership: 0.7     # Core leadership
      charter_amendment: 0.7       # High trust for constitutional changes

  # ─── Budget Authority ────────────────────────────────
  budget:
    spending_threshold: 500  # USD — above this requires steering approval
    currency: USD
    
    # Committees receive allocations; spending within allocation is autonomous
    committee_allocations:
      approval_required: steering
      approval_threshold: supermajority_two_thirds
    
    # Individual expenses within allocation
    expense_approval:
      within_allocation: autonomous  # Committee coordinator approves
      above_allocation: steering     # Requires budget amendment proposal

  # ─── Membership Tiers ───────────────────────────────
  membership:
    tiers:
      - name: full_member
        entity_types: [cooperative]
        role: federated_member
        rights: [vote, propose, treasury_access, invite]
        obligations:
          - "Maintain active participation"
          - "Adhere to charter"
          - "Send delegate to annual meeting"
      
      - name: affiliate
        entity_types: [cooperative, community]
        role: associate_member
        rights: [vote_limited, propose_limited]
        obligations:
          - "Aligned mission"
      
      - name: emerging
        entity_types: [community]
        role: provisional_member
        rights: [propose_community]
        obligations:
          - "Working toward cooperative formation"
      
      - name: individual
        entity_types: [individual]
        role: member
        rights: [vote_in_committees, propose]
      
      - name: observer
        entity_types: [individual, cooperative]
        role: observer_member
        rights: [read_only]
```

---

## Template 2: Worker Cooperative

Use when: A single worker-owned cooperative where members share ownership and governance equally. All workers are member-owners with equal voting rights.

```yaml
# worker-coop-charter.yaml
charter:
  name: "Worker Cooperative"
  version: 1
  entity_type: cooperative
  
  governance:
    eligible_voters: all_members
    
    decisions:
      # Day-to-day operational decisions
      operational:
        threshold: simple_majority
        quorum: 0.5
        voting_period_hours: 48
      
      # Financial decisions above threshold
      financial:
        threshold: simple_majority
        quorum: 0.6
        voting_period_hours: 72
      
      # New member admission
      membership:
        threshold: supermajority_two_thirds
        quorum: 0.6
        deliberation_period_hours: 168  # 7 days
        voting_period_hours: 168
      
      # Charter changes
      constitutional:
        threshold: supermajority_two_thirds
        quorum: 0.75
        deliberation_period_hours: 336  # 14 days
        voting_period_hours: 336
      
      # Emergency actions
      emergency:
        threshold: supermajority_two_thirds
        quorum: 0.3
        voting_period_hours: 24

  # Worker co-ops are typically flat — no committees by default
  # Add committees section if needed as the co-op grows
  
  trust:
    thresholds:
      propose_operational: 0.0     # Any worker-member
      propose_financial: 0.2       # Some track record
      propose_membership: 0.3      # Established member
      propose_constitutional: 0.5  # Senior member
      officer_eligibility: 0.5     # Board/officer candidates

  budget:
    spending_threshold: 200  # Above this needs a vote
    currency: USD
    
    expense_approval:
      below_threshold: any_member     # Anyone can approve small expenses
      above_threshold: membership_vote # Requires collective decision

  membership:
    probationary_period_days: 90  # New workers serve probation
    buy_in_required: true         # Member equity contribution
    
    roles:
      - name: worker_owner
        role: worker
        rights: [vote, propose, treasury_access]
      
      - name: probationary
        role: provisional_member
        rights: [propose]
        # Cannot vote until probation ends
      
      - name: board_member
        role: board_member
        rights: [vote, propose, treasury_access, configure]
        # Elected by worker-owners
      
      - name: officer
        role: officer
        rights: [vote, propose, treasury_access, sign, configure]
        # Elected for specific titles (president, treasurer, secretary)

  # ─── Surplus Distribution ────────────────────────────
  surplus:
    # How profits are distributed
    allocation:
      reserves: 0.20          # 20% to reserves
      member_dividends: 0.60  # 60% to workers based on patronage
      community_fund: 0.10    # 10% to community
      education: 0.10         # 10% to education/training
    
    patronage_basis: hours_worked  # Dividends proportional to labor
```

---

## Template 3: Community Organization

Use when: A community group, mutual aid network, or civic organization focused on collective action and shared governance. May or may not handle money.

```yaml
# community-charter.yaml
charter:
  name: "Community Organization"
  version: 1
  entity_type: community
  
  governance:
    eligible_voters: all_active_members
    
    decisions:
      # General community decisions
      general:
        threshold: simple_majority
        quorum: 0.3  # Lower quorum — community orgs have variable attendance
        voting_period_hours: 120  # 5 days
      
      # Spending community funds (if any)
      financial:
        threshold: simple_majority
        quorum: 0.4
        voting_period_hours: 72
      
      # New member admission
      membership:
        threshold: simple_majority
        quorum: 0.3
        voting_period_hours: 72
        # Many community orgs have open membership — set threshold low
      
      # Leadership elections
      elections:
        threshold: simple_majority
        quorum: 0.5
        voting_period_hours: 168
      
      # Charter changes
      constitutional:
        threshold: supermajority_two_thirds
        quorum: 0.4
        deliberation_period_hours: 168
        voting_period_hours: 336

  # ─── Working Groups ─────────────────────────────────
  working_groups:
    # Community orgs often use working groups instead of formal committees
    formation: proposal  # New groups require a governance proposal
    
    defaults:
      threshold: simple_majority
      quorum: 0.5
      voting_period_hours: 48
    
    # Working groups are autonomous within their scope
    # but cannot commit the community to financial obligations
    delegation:
      autonomous:
        - "Planning within scope"
        - "Recruiting volunteers"
        - "Scheduling activities"
      requires_community_approval:
        - "Any financial commitment"
        - "Public statements on behalf of community"
        - "Partnerships or agreements"

  trust:
    thresholds:
      join_community: 0.0         # Open membership
      propose_activity: 0.0       # Anyone can propose
      vote: 0.05                  # Minimal participation
      lead_working_group: 0.3     # Some track record
      propose_financial: 0.3      # Trusted with money decisions
      leadership_eligible: 0.4    # Can serve as coordinator

  membership:
    # Open membership is common for community orgs
    admission: open  # or "proposal" for gated membership
    
    roles:
      - name: community_member
        role: member
        rights: [vote, propose]
      
      - name: coordinator
        role: officer
        title: "Coordinator"
        rights: [vote, propose, invite, configure]
      
      - name: working_group_lead
        role: custom
        name: "Working Group Lead"
        rights: [vote, propose, invite]
```

---

## Template 4: Multi-Stakeholder Cooperative

Use when: A cooperative with multiple member classes (workers, consumers, community members, investors) who have different governance weights and rights.

```yaml
# multistakeholder-charter.yaml
charter:
  name: "Multi-Stakeholder Cooperative"
  version: 1
  entity_type: cooperative
  
  governance:
    # Multi-stakeholder co-ops often use weighted voting by class
    voting_model: stakeholder_weighted
    
    stakeholder_classes:
      - name: workers
        weight: 0.40          # 40% of voting power
        eligible_role: worker
      
      - name: consumers
        weight: 0.30          # 30% of voting power
        eligible_role: consumer
      
      - name: community
        weight: 0.20          # 20% of voting power
        eligible_role: member
      
      - name: investors
        weight: 0.10          # 10% of voting power
        eligible_role: associate_member
    
    decisions:
      operational:
        threshold: simple_majority  # Within each class
        quorum: 0.5
        voting_period_hours: 72
      
      financial:
        threshold: simple_majority
        quorum: 0.6
        voting_period_hours: 120
      
      membership:
        threshold: supermajority_two_thirds
        quorum: 0.5
        voting_period_hours: 168
      
      constitutional:
        threshold: supermajority_two_thirds
        quorum: 0.6
        # Constitutional changes require majority in EACH class
        per_class_approval: true
        deliberation_period_hours: 336
        voting_period_hours: 336

  trust:
    thresholds:
      propose: 0.1
      vote: 0.05
      board_eligible: 0.5
      officer_eligible: 0.6

  membership:
    classes:
      - name: worker_member
        role: worker
        buy_in: true
        probation_days: 90
        rights: [vote, propose, treasury_access]
      
      - name: consumer_member
        role: consumer
        buy_in: true  # Usually smaller share
        rights: [vote, propose]
      
      - name: community_member
        role: member
        buy_in: false
        rights: [vote, propose]
      
      - name: investor_member
        role: associate_member
        buy_in: true  # Preferred shares
        rights: [vote_limited, propose_limited]
        # Investor class has limited governance to prevent capital capture

  surplus:
    allocation:
      reserves: 0.20
      worker_dividends: 0.35  # Based on patronage (hours)
      consumer_dividends: 0.15  # Based on patronage (purchases)
      community_fund: 0.15
      investor_return: 0.10  # Capped return on investment
      education: 0.05

  # ─── Anti-Capture Provisions ─────────────────────────
  safeguards:
    # Prevent any single class from dominating
    max_class_weight: 0.50  # No class can exceed 50% voting power
    investor_return_cap: 0.08  # 8% maximum annual return
    worker_class_floor: 0.30  # Workers always have at least 30%
```

---

## Using a Template

1. **Choose the template** closest to your institutional type
2. **Customize** governance thresholds, trust levels, committee structure, and budget rules
3. **Save** as a YAML file
4. **Submit** as a `Charter` proposal via the gateway API
5. **Vote** to ratify with your founding members
6. **Deploy** — on ratification, the charter engine begins enforcing the rules

### Customization Checklist

Before ratifying, review:

- [ ] Quorum thresholds — too high means decisions stall; too low means minority rule
- [ ] Voting periods — long enough for participation, short enough for action
- [ ] Deliberation periods — skip for routine decisions, require for constitutional changes
- [ ] Spending thresholds — what dollar amount triggers escalation
- [ ] Trust thresholds — balance accessibility with accountability
- [ ] Committee delegation — what can committees do without full-body approval
- [ ] Membership admission — open, proposal-based, or invitation-only
- [ ] Surplus distribution — only if the institution handles revenue

---

## See Also

- [Institution Setup Guide](../guides/institution-setup.md) — Full bootstrap walkthrough
- [Governance Playbook](governance-playbook.md) — Common decision patterns
- [Entity Topology Patterns](entity-topology-patterns.md) — Organizational structures
