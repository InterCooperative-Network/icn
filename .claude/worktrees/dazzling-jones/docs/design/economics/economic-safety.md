# ICN Economic Safety Mechanisms

**Status:** Phase 12 Implementation (In Progress)
**Version:** 0.1.x
**Last Updated:** 2025-01-14

## Overview

Mutual credit systems fail predictably when communities lack economic safeguards. Without protection mechanisms, the first bad actor can destroy trust in the entire system. ICN implements multiple layers of economic safety to protect cooperative communities from common failure modes.

## Table of Contents

1. [Threat Model](#threat-model)
2. [Dynamic Credit Limits](#dynamic-credit-limits)
3. [New Member Protection](#new-member-protection)
4. [Dispute Resolution](#dispute-resolution)
5. [Configuration Guide](#configuration-guide)
6. [Monitoring & Alerts](#monitoring--alerts)
7. [Emergency Procedures](#emergency-procedures)

---

## Threat Model

### Common Mutual Credit Failures

**1. Free Riders**
- **Attack:** Extract value without contributing back
- **Impact:** System collapses as everyone goes negative
- **Likelihood:** High without credit limits

**2. "Grab and Run" (New Member Exploitation)**
- **Attack:** Join, max out credit immediately, disappear
- **Impact:** Community loses trust, restricts new members
- **Likelihood:** Very high without new member throttling

**3. Credit Limit Gaming**
- **Attack:** Build trust slowly, then max out and default
- **Impact:** Significant loss to cooperative
- **Likelihood:** Medium with static limits

**4. Frivolous Disputes**
- **Attack:** File disputes to block legitimate transactions
- **Impact:** System becomes unusable
- **Likelihood:** Low with mediator oversight

### ICN's Defense-in-Depth Approach

```
Layer 1: Dynamic Credit Limits (trust + history-based)
  ↓
Layer 2: New Member Throttling (time + contribution-based)
  ↓
Layer 3: Dispute Resolution (mediated oversight)
  ↓
Layer 4: Community Governance (Phase 13, future)
```

---

## Dynamic Credit Limits

### Overview

Credit limits determine how much debt a member can owe before being unable to transact. ICN calculates limits dynamically based on:
- **Trust score** (from trust graph)
- **Historical contributions** (cleared transaction volume)
- **Tenure** (time as a member)

### Formula

```rust
limit = baseline + trust_bonus + history_bonus

where:
  trust_bonus = baseline × trust_score × trust_multiplier
  history_bonus = cleared_volume × history_bonus_rate
```

### Example Calculation

**Scenario:** Conservative policy for "hours" currency

**Member Alice:**
- Trust score: 0.8 (highly trusted)
- Cleared volume: 1,000 hours (active contributor)
- Tenure: 6 months (established member)

**Calculation:**
```
baseline = 100 hours
trust_bonus = 100h × 0.8 × 0.3 = 24 hours
history_bonus = 1,000h × 0.05 = 50 hours
total limit = 100h + 24h + 50h = 174 hours
```

**Member Bob:**
- Trust score: 0.2 (new relationships)
- Cleared volume: 50 hours (limited activity)
- Tenure: 1 month (new member)

**Calculation:**
```
baseline = 100 hours
trust_bonus = 100h × 0.2 × 0.3 = 6 hours
history_bonus = 50h × 0.05 = 2.5 hours
total limit = 100h + 6h + 2.5h = 108.5 hours
```

**Result:** Alice can owe up to 174 hours, Bob only 108.5 hours. Trust and contribution are rewarded.

### Policy Presets

**Conservative** (recommended for new communities):
```rust
CreditPolicy::conservative("hours")
```
- Baseline: 100 hours
- Trust multiplier: 0.3 (30% bonus)
- History bonus rate: 0.05 (5% of cleared volume)

**Permissive** (for established communities):
```rust
CreditPolicy::permissive("hours")
```
- Baseline: 500 hours
- Trust multiplier: 0.5 (50% bonus)
- History bonus rate: 0.15 (15% of cleared volume)

### When Limits Are Checked

1. **Before transaction creation:** Validates sender won't exceed their limit
2. **During ledger sync:** Entries exceeding limits go to quarantine
3. **Periodic review:** Communities can audit limit violations

### Bypassing Limits

**Not supported in v0.1.** Future versions may support:
- Emergency overrides (requires multi-sig)
- Temporary limit increases (community vote)
- Special account types (infrastructure, shared resources)

---

## New Member Protection

### The Problem

Without throttling, new members can:
1. Join the cooperative
2. Immediately max out their credit limit
3. Disappear before contributing
4. Leave the community holding the debt

**Real-world example:** A timebank allows 100 hours of debt. New member Alice requests 100 hours of help in week 1, then ghosts. The community lost 100 hours of labor.

### ICN's Solution: Progressive Ramping

New members start with a **very low** credit limit that increases over time **only if** they contribute.

### Ramping Logic

```rust
if cleared_volume < contribution_threshold {
    return initial_limit;  // Stay at minimum
} else {
    // Linear ramp over ramp_period
    progress = (current_time - member_since) / ramp_period;
    return initial_limit + (full_limit - initial_limit) × progress;
}
```

### Example: Conservative Policy

**Parameters:**
- Initial limit: 10 hours
- Contribution threshold: 50 hours
- Ramp period: 90 days
- Full limit: 174 hours (from dynamic calculation)

**Timeline:**

**Week 1:**
- Alice joins, cleared = 0 hours
- Limit = 10 hours (initial)
- Can only request small favors

**Week 5:**
- Alice cleared = 60 hours (helps 6 members, 10h each)
- Tenure = 35 days
- Progress = 35 / 90 = 0.39
- Limit = 10h + (174h - 10h) × 0.39 = **74 hours**

**Day 90:**
- Alice cleared = 200 hours (active contributor)
- Progress = 90 / 90 = 1.0
- Limit = **174 hours** (full limit)

**Result:** Alice earned her limit by demonstrating value. A "grab and run" attacker would be stuck at 10 hours.

### Contribution Threshold

The threshold **prevents gaming** the time-based ramp:
- Can't just wait 90 days doing nothing
- Must clear `contribution_threshold` before ramping starts
- Demonstrates genuine participation

**Why 50 hours?** (Conservative default)
- Significant enough to prove commitment
- Achievable within first month for active members
- Small enough not to create unfair barriers

### Override Mechanisms (Future)

For legitimate edge cases:
- **Sponsor vouching:** Trusted member co-signs new member's limit
- **Emergency fund:** Community pool for unexpected needs
- **Hardship review:** Case-by-case evaluation

---

## Dispute Resolution

### Overview

Disputes provide a mechanism for members to contest ledger entries they believe are incorrect, fraudulent, or should be written off.

### Dispute Lifecycle

```
Normal Entry
  ↓
[Member files dispute] → Contested
  ↓
[Evidence added]
  ↓
[Mediator assigned]
  ↓
[Mediator investigates]
  ↓
[Resolution recorded] → Resolved
```

### Filing a Dispute

**Who can file:**
- Any participant in the transaction
- Affected account holders
- Community mediators (fraud detection)

**Required information:**
- Entry hash (which transaction)
- Reason (clear explanation)
- Optional evidence (emails, receipts, photos)

**Example:**
```rust
let reason = "Charged 100 hours for work that was agreed at 50 hours";
let evidence = "Email from 2025-01-10 confirms 50 hour agreement";

manager.file_dispute(entry_hash, my_did, reason, timestamp)?;
manager.add_evidence(&entry_hash, evidence)?;
```

### Mediation

**Mediator assignment:**
- Community designates trusted mediators
- Mediator cannot be involved in the transaction
- Mediator investigates and reviews evidence

**Mediator duties:**
1. Review the disputed entry
2. Contact both parties
3. Examine all evidence
4. Make a fair determination
5. Record resolution with reasoning

### Resolution Outcomes

**1. Upheld**
- Entry is valid
- Dispute was frivolous or mistaken
- Original entry stands
```rust
DisputeOutcome::Upheld
```

**2. Reversed**
- Entry is invalid
- Should be reversed/rolled back
- Original entry marked invalid
```rust
DisputeOutcome::Reversed
```

**3. Settlement**
- Partial agreement reached
- New entry replaces original
- Both parties accept terms
```rust
DisputeOutcome::Settlement {
    terms: "Split difference: 75 hours instead of 100".to_string(),
    replacement_entry: Some(new_entry_hash),
}
```

**4. Write-Off**
- Debt forgiven or written off
- Used for defaults, hardship, or community decisions
- Original entry stays, but obligation removed
```rust
DisputeOutcome::WriteOff {
    reason: "Member experienced emergency, community voted to forgive".to_string(),
}
```

### Dispute Query

**Active disputes:**
```rust
let active = manager.get_active_disputes();
for dispute in active {
    println!("Entry {} disputed by {}", dispute.entry_hash, dispute.filed_by);
}
```

**Disputes by filer:**
```rust
let my_disputes = manager.get_disputes_by_filer(&my_did);
println!("I've filed {} disputes", my_disputes.len());
```

**Check if entry is disputed:**
```rust
if manager.has_active_dispute(&entry_hash) {
    println!("⚠️ This entry is currently under dispute");
}
```

### Preventing Abuse

**Frivolous dispute protection:**
- Mediator oversight (bad faith disputes can be dismissed)
- Reputation impact (filers of frivolous disputes lose trust)
- Rate limiting (future: max N disputes per time period)

**Mediator accountability:**
- Mediator decisions are recorded and auditable
- Poor mediators can be removed by community governance
- Mediators have their own reputation scores

---

## Configuration Guide

### Choosing a Policy

**New communities (<10 members, <6 months old):**
```rust
let policy = CreditPolicyManager::conservative("hours".to_string());
```
- Protects against early bad actors
- Builds trust slowly
- Lower risk tolerance

**Established communities (>20 members, >1 year, proven track record):**
```rust
let policy = CreditPolicyManager::permissive("hours".to_string());
```
- Rewards active participation more
- Higher baseline for convenience
- Assumes mature trust relationships

**Custom policies:**
```rust
let credit_policy = CreditPolicy::new(
    200_00,  // 200 hours baseline (using 2 decimals)
    0.4,     // 40% trust bonus
    0.10,    // 10% history bonus
    "hours".to_string(),
);

let new_member_policy = NewMemberPolicy::new(
    20_00,                           // 20 hours initial
    Duration::from_secs(60 * 86400), // 60 days ramp
    75_00,                           // 75 hours threshold
    "hours".to_string(),
);

let manager = CreditPolicyManager::new(credit_policy, new_member_policy);
```

### Tuning Parameters

**Baseline:**
- **Higher:** More convenience, higher risk
- **Lower:** More restrictive, lower risk
- **Recommendation:** Start low, increase as trust builds

**Trust Multiplier:**
- **Higher:** Rewards trust more heavily
- **Lower:** Trust matters less
- **Recommendation:** 0.3 (conservative) to 0.5 (permissive)

**History Bonus Rate:**
- **Higher:** Rewards activity more
- **Lower:** Limits based more on trust
- **Recommendation:** 0.05 (conservative) to 0.15 (permissive)

**New Member Initial Limit:**
- **Higher:** Faster onboarding, higher risk
- **Lower:** Slower onboarding, lower risk
- **Recommendation:** 10-20 hours for timebanking

**Contribution Threshold:**
- **Higher:** Harder to reach full limit
- **Lower:** Easier to game the ramp
- **Recommendation:** 0.5× baseline (conservative)

**Ramp Period:**
- **Longer:** More vetting time
- **Shorter:** Faster integration
- **Recommendation:** 90 days (conservative), 60 days (moderate)

### Multi-Currency Configuration

Each currency should have its own policy:

```rust
// Hours (labor)
let hours_policy = CreditPolicyManager::conservative("hours".to_string());

// USD (monetary)
let usd_policy = CreditPolicyManager::new(
    CreditPolicy::new(100_00, 0.2, 0.05, "USD".to_string()),  // $100 baseline
    NewMemberPolicy::new(10_00, Duration::from_secs(90*86400), 50_00, "USD".to_string()),
);

// kWh (energy)
let kwh_policy = CreditPolicyManager::permissive("kWh".to_string());
```

---

## Monitoring & Alerts

### Key Metrics

**Credit limit violations:**
```prometheus
# Entries quarantined due to credit limit exceeded
icn_ledger_entries_quarantined_total{reason="credit_limit"}

# Alert if > 10 per day
```

**Dispute activity:**
```bash
# Check active dispute count
icnctl disputes list --active
```

**New member ramp progress:**
```bash
# Check a member's current limit
icnctl credit-limit show did:icn:alice
```

### Recommended Alerts

**High quarantine rate:**
```yaml
- alert: HighCreditLimitQuarantine
  expr: rate(icn_ledger_entries_quarantined_total{reason="credit_limit"}[1h]) > 5
  severity: warning
  message: "Many entries being quarantined for credit limit violations"
```

**Dispute spike:**
```yaml
- alert: DisputeSpike
  expr: icn_dispute_active_count > 10
  severity: warning
  message: "Unusual number of active disputes"
```

### Community Health Indicators

**Healthy community:**
- Low quarantine rate (<1% of transactions)
- Few disputes (<5% of transactions)
- Members staying within 80% of their limits
- New members successfully ramping up

**Warning signs:**
- Spike in quarantined entries
- Multiple disputes from same member
- New members hitting limits immediately
- High default rate

---

## Emergency Procedures

### Scenario 1: Member Defaults on Large Debt

**Situation:** Member owes 500 hours, disappears

**Immediate actions:**
1. File dispute against all outstanding entries
2. Freeze member's account (prevent new transactions)
3. Assign mediator to investigate
4. Contact member (email, phone, in-person)

**Resolution options:**
1. **Payment plan:** Member agrees to repay over time
2. **Partial write-off:** Community votes to forgive portion
3. **Full write-off:** Community absorbs the loss
4. **Expulsion:** Remove member, write off debt

**Prevention for next time:**
- Review credit policy settings
- Consider lowering limits
- Implement more frequent check-ins

### Scenario 2: Credit Policy Too Restrictive

**Situation:** Legitimate members can't transact enough

**Diagnosis:**
```bash
# Check how many members are near their limits
icnctl ledger balances --format json | jq '.[] | select(.balance < -.limit * 0.9)'
```

**Solutions:**
1. **Increase baseline:** Raise the floor for everyone
2. **Increase trust multiplier:** Reward trusted members more
3. **Decrease new member threshold:** Easier to ramp up
4. **Switch to permissive preset:** For established communities

**Implementation:**
```rust
// Increase baseline from 100 to 150 hours
let new_policy = CreditPolicy::new(
    150_00,  // Increased baseline
    0.3,
    0.05,
    "hours".to_string(),
);
```

### Scenario 3: Dispute System Abuse

**Situation:** Member filing frivolous disputes

**Diagnosis:**
```bash
# Check disputes filed by member
icnctl disputes list --filed-by did:icn:alice
```

**Actions:**
1. **Mediator review:** All disputes by this member
2. **Pattern detection:** Are disputes legitimate?
3. **Warning:** Notify member of abuse concerns
4. **Sanction:** Reduce trust score if abuse confirmed
5. **Rate limiting:** Implement max disputes per month

**Future prevention:**
- Reputation impact for bad-faith disputes
- Community review of repeated filers
- Mediator training on identifying abuse

### Scenario 4: Mass Defaults (System-Wide Crisis)

**Situation:** Many members defaulting simultaneously

**Possible causes:**
- Economic shock (real-world crisis)
- Policy misconfiguration
- Coordinated attack

**Emergency measures:**
1. **Freeze new transactions:** Prevent further damage
2. **Community meeting:** Discuss crisis response
3. **Debt forgiveness vote:** Collective write-off
4. **Policy reset:** Rebuild with lessons learned

**Recovery:**
```rust
// Lower all limits to prevent future crisis
let crisis_policy = CreditPolicy::new(
    50_00,   // Much lower baseline
    0.2,     // Less trust bonus
    0.03,    // Less history bonus
    "hours".to_string(),
);

// Restart with conservative settings
```

---

## Best Practices

### For Communities

**1. Start conservative, loosen gradually**
- Use conservative preset initially
- Monitor for 3-6 months
- Adjust based on actual behavior

**2. Regular policy reviews**
- Quarterly check-in on policy effectiveness
- Adjust parameters based on community growth
- Survey members about limit adequacy

**3. Transparent communication**
- Explain credit limits clearly to new members
- Document policy rationale in community handbook
- Publicize policy changes

**4. Fair mediation**
- Select unbiased mediators
- Rotate mediator duties
- Document mediation decisions

### For Members

**1. Stay within your limits**
- Monitor your balance regularly
- Plan ahead for large transactions
- Build trust and contribution history

**2. File disputes promptly**
- Don't wait months to contest an entry
- Provide clear evidence
- Be honest and specific

**3. Honor commitments**
- Fulfill obligations on time
- Communicate if you can't deliver
- Build reputation through reliability

---

## Future Enhancements

### Phase 13: Governance Integration

**Planned features:**
- Community voting on policy changes
- Proposal system for limit adjustments
- Elected mediator boards
- Automated dispute escalation

### Track B3: Economic Modeling

**Research areas:**
- Agent-based simulation of attacks
- Optimal parameter tuning via simulation
- Risk assessment for different policies
- Default prediction modeling

### Advanced Credit Features

**Potential additions:**
- Credit unions (shared limits across groups)
- Seasonal adjustments (summer/winter variations)
- Emergency credit pools
- Cross-currency credit swaps

---

## References

- [ROADMAP.md](../../development/sessions/undated/ROADMAP.md) - Phase 12 specification
- [docs/incident-response.md](../../operations/deployment/incident-response.md) - Default handling procedures
- [docs/operations-guide.md](../../guides/operations/operations-guide.md) - Daily operational workflows
- [icn-ledger/src/credit_policy.rs](../../../icn/crates/icn-ledger/src/credit_policy.rs) - Implementation
- [icn-ledger/src/dispute.rs](../../../icn/crates/icn-ledger/src/dispute.rs) - Dispute resolution

---

## Conclusion

ICN's economic safety mechanisms provide defense-in-depth protection against common mutual credit failures:

1. **Dynamic credit limits** prevent free riders and limit gaming
2. **New member throttling** stops "grab and run" attacks
3. **Dispute resolution** handles conflicts and enables debt forgiveness
4. **Community governance** (Phase 13) adds democratic oversight

These safeguards make ICN suitable for real-world cooperative economies, where trust must be earned and bad actors must be contained.

**Ready for pilot deployment:** All Phase 12 core features implemented and tested.

**Version:** 0.1.x
**Last Updated:** 2025-01-14
**Status:** Phase 12 - Economic Safety Rails (In Progress)
