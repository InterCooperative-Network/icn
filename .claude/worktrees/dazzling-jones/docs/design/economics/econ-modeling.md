# Economic Modeling for Mutual Credit Systems

**Status**: ✅ **COMPLETE** (Track B3)
**Last Updated**: 2025-01-14
**Simulation Results**: [sims/mutual-credit/RESULTS_SUMMARY.md](../../../sims/mutual-credit/RESULTS_SUMMARY.md)

## Purpose

Mutual credit systems have well-documented failure modes. Before deploying ICN's ledger in production, we need to:

1. **Catalog known failure modes** from historical mutual credit systems
2. **Model agent behaviors** that lead to system collapse
3. **Test economic parameters** (credit limits, demurrage, default handling)
4. **Recommend defaults** for Phase 12 (Economic Safety Rails)

This document guides the economic modeling parallel research track.

---

## Simulation Results Summary (January 2025)

**Framework Implemented**: Agent-based model using Mesa 3.3.1 with 100 agents over 12 months across 5 scenarios.

### Key Findings

| Intervention | Effect | Recommendation |
|--------------|--------|----------------|
| **Dynamic Credit Limits** (2x trust multiplier) | -16% velocity, -33% defaults | ✅ Use for stability, accept lower volume |
| **Demurrage** (-2% monthly on >50) | -22% inequality (Gini), no velocity harm | ✅ Highly effective redistribution |
| **High Free-Riders** (20% vs 8%) | +51% defaults, system stressed but stable | ⚠️ System tolerates up to ~20% |
| **Low Trust Network** (30% density) | +100% hoarding, conservative behavior | ⚠️ Network density critical |

### Validated Hypotheses

1. ✅ **Demurrage works**: Reduced Gini from 0.36 to 0.28 without harming velocity
2. ✅ **Trust-gated limits reduce defaults**: 1.8% vs 2.7% baseline
3. ✅ **System can tolerate free-riders**: Up to 20% before serious stress
4. ❌ **Sparse networks reduce hoarding**: WRONG - they increase it (counterintuitive but economically sound)

### Recommended Defaults (Updated with Simulation Data)

Based on 5 scenarios with ~13,000 transactions each:

**Credit Limits**:
- Initial: -20 (validated - allows 2-4 small transactions)
- Maximum: -500 (validated - caps extreme exposure)
- Growth: +10 per 50 cleared (validated - agents reached -500 in 3-4 months)
- Trust multiplier: 2.0 (tested - provides meaningful differentiation)

**Demurrage**:
- Rate: -2% monthly on balances >50 (validated - reduces inequality 22%)
- Threshold: 50 credits (tested - exempts small savers)
- Application: Monthly (timing critical - must be synchronized)

**New Member Protection**:
- Initial limit: -20 (conservative, validated)
- Ramp period: 3 months (implemented in Phase 12)
- Contribution requirement: 10 credits cleared (Phase 12 feature)

### Implementation Notes

All economic safety features from simulation are now implemented in ICN:
- Phase 12: Dynamic credit limits, new member throttling, dispute resolution
- Ledger: Double-entry with quarantine for conflicts
- Trust graph: Transitive trust computation with decay

See [sims/mutual-credit/](../../../sims/mutual-credit/) for complete framework, scenarios, and analysis notebooks.

---

## Known Failure Modes

### 1. Tragedy of the Credits (Hoarding / Deflation)

**Symptom**: Users accumulate positive balances and refuse to spend, leading to systemic deflation.

**Mechanism**:
- Alice provides 100 hours of service, earns +100 credits
- Alice never spends those credits (treats them as savings)
- System has net negative balance elsewhere (someone is -100)
- As more people hoard, fewer credits circulate
- New members can't earn credits because no one is spending

**Historical Examples**:
- LETS (Local Exchange Trading Systems) in 1990s UK: 20-30% of members held 80% of positive balances
- Ithaca Hours: Chronic deflation as businesses accumulated hours but couldn't spend them locally

**Potential Mitigations**:
- **Demurrage**: Negative interest on positive balances (e.g., -2% per month)
- **Expiring credits**: Credits lose value after 12 months
- **Balance caps**: Maximum positive balance enforced by protocol
- **Contribution requirements**: Must spend X% of earnings to remain in good standing

**Questions for Simulation**:
- What demurrage rate prevents hoarding without punishing savers?
- Does demurrage increase velocity or just annoy users?
- Do expiring credits create panic spending (bad UX) or healthy circulation?

---

### 2. Free-Rider Problem (Extraction Without Contribution)

**Symptom**: Users extract value from the community without contributing, exploiting trust or weak enforcement.

**Mechanism**:
- Bob joins, earns high trust quickly (is friendly, shows up to meetings)
- Bob racks up -50 credits (receives services)
- Bob ghosts the community, never clears debt
- Community eats the loss; trust in system degrades

**Historical Examples**:
- WIR Bank (Switzerland): Required mandatory membership fees + strict credit limits to prevent free-riding
- Sardex (Italy): Uses credit scoring + mandatory clearing periods

**Potential Mitigations**:
- **New member throttling**: Very low initial credit limits
- **Trust-gated limits**: Credit limit scales with demonstrated trustworthiness
- **Mandatory clearing**: Must clear debt within 90 days or lose borrowing privileges
- **Default visibility**: Failed obligations are publicly visible in reputation layer

**Questions for Simulation**:
- How low should initial credit limits be? (Too low = useless, too high = risky)
- Does trust-gated scaling work if trust graph is manipulated?
- What percentage of free-riders can a system tolerate before collapse?

---

### 3. Credit Limit Gaming (Borrow-and-Bail)

**Symptom**: Users maximize their credit limit, extract maximum value, then abandon the community.

**Mechanism**:
- Carol builds trust over 6 months (clears small debts reliably)
- Carol's credit limit increases to -200
- Carol immediately borrows -200 (receives services)
- Carol disappears, never repays

**Historical Examples**:
- Mutual credit systems without dynamic limits often see "graduation fraud"
- Similar to credit card bust-out schemes in traditional finance

**Potential Mitigations**:
- **Gradual limit increases**: Ramp credit slowly based on *cleared* volume (not just time)
- **Velocity-based limits**: Can only borrow X per month (rate limiting)
- **Collateral requirements**: High-trust members can vouch for new members (skin in the game)
- **Quarantine on large debts**: Debts above threshold require community approval to continue

**Questions for Simulation**:
- What's the optimal credit limit curve? (Linear, logarithmic, stepwise?)
- How fast should limits ramp? (Too slow = frustration, too fast = exploitation)
- Can social collateral (vouching) prevent gaming?

---

### 4. Velocity Collapse (Trust Breakdown → No Spending)

**Symptom**: After a few defaults or conflicts, members stop transacting out of fear.

**Mechanism**:
- Dan defaults on -30 credits
- Community becomes risk-averse
- Members only transact with known-safe people (high trust scores)
- New members can't break into the trade network
- Velocity drops to near-zero; system is technically alive but functionally dead

**Historical Examples**:
- Many LETS systems collapsed not from technical failure but from "loss of nerve"
- TimeBanks suffer from this when a few bad experiences go unresolved

**Potential Mitigations**:
- **Trust graph resilience**: Default by one member shouldn't tank trust in the system
- **Dispute resolution**: Fast, fair handling of conflicts prevents fear spiral
- **System insurance**: Community reserve pool to absorb small losses
- **Velocity targets**: Alert community if transaction volume drops below threshold

**Questions for Simulation**:
- What trust graph structure is resilient to local failures?
- Can insurance pools prevent velocity collapse without creating moral hazard?
- What's the minimum viable transaction volume for a healthy system?

---

### 5. Pricing Instability (No Shared Unit of Account)

**Symptom**: Different members value the credit unit differently, leading to price chaos.

**Mechanism**:
- Alice charges 10 credits/hour for massage
- Bob charges 50 credits/hour for plumbing
- Carol charges 5 credits/hour for childcare
- No one can agree on "fair" exchange rates
- System fragments into micro-economies with incompatible pricing

**Historical Examples**:
- Ithaca Hours struggled with this: Was 1 Hour = 1 hour of labor, or 1 Hour = $10?
- LETS systems often saw "professional service" vs. "manual labor" pricing conflicts

**Potential Mitigations**:
- **Anchoring to external unit**: 1 credit = 1 hour of time (TimeBanks)
- **Pricing guidelines**: Community-approved rate sheets for common services
- **Market feedback**: Show "typical rates" for similar services
- **Trust-weighted pricing**: Trusted members' pricing signals influence norms

**Questions for Simulation**:
- Does anchoring (1 credit = 1 hour) prevent pricing chaos or cause other problems?
- Can pricing norms emerge organically, or do they need governance?
- What happens if a few members consistently over/underprice?

---

## Agent-Based Simulation Design

### Goals
- Model 50-200 agents with heterogeneous behaviors
- Simulate 1-2 years of transactions
- Test different parameter sets (credit limits, demurrage, trust dynamics)
- Identify failure modes and resilience thresholds

### Agent Types

**Reciprocators** (60-70% of population):
- Provide and consume services roughly equally
- Clear debts within reasonable time
- Build trust through consistent behavior
- Realistic model of "good community members"

**Hoarders** (10-15%):
- Accumulate positive balances
- Rarely spend earned credits
- Not malicious, just risk-averse or have fewer needs
- Test demurrage effectiveness

**Free-Riders** (5-10%):
- Consume more than they provide
- May ghost after hitting credit limit
- Test robustness of credit policies

**Opportunists** (5-10%):
- Game the system (build trust, then extract maximum)
- Test security of trust-gated limits

**Super-Contributors** (5-10%):
- Provide far more than they consume
- Anchor the economy with consistent supply
- Can become resentful if taken advantage of

### Simulation Parameters

**Economic**:
- `initial_credit_limit`: Starting borrowing limit (e.g., -10 to -50)
- `max_credit_limit`: Maximum borrowing limit (e.g., -500)
- `limit_growth_rate`: How fast limits increase (e.g., 10% of cleared volume)
- `demurrage_rate`: Negative interest on positive balances (e.g., -2% per month)
- `default_threshold`: When is someone considered in default? (e.g., -100 for >90 days)

**Trust**:
- `trust_growth_rate`: How fast trust builds with successful transactions
- `trust_decay_rate`: How fast trust decays with failed obligations
- `trust_limit_multiplier`: How much trust boosts credit limit (e.g., 2x at max trust)

**Network**:
- `network_density`: How connected is the trust graph? (affects transaction opportunities)
- `new_member_rate`: How many new members join per month?
- `churn_rate`: How many members leave per month?

### Metrics to Track

**System Health**:
- Total transaction volume per month (velocity)
- Gini coefficient of balances (inequality)
- Number of defaults / write-offs
- Percentage of members in good standing

**Individual Outcomes**:
- Average balance over time
- Credit limit distribution
- Trust score distribution
- Time to clear debts

**Failure Indicators**:
- Velocity collapse: Transactions drop below threshold
- Deflation spiral: Positive balance accumulation accelerates
- Default cascade: Multiple simultaneous defaults
- Network fragmentation: Isolated clusters stop trading

---

## Simulation Implementation

### Tools

**Option 1: Python + Mesa (Agent-Based Modeling)**
- Pros: Fast prototyping, good visualization, familiar to social scientists
- Cons: Slower execution for large simulations

**Option 2: Rust (Custom Simulation)**
- Pros: Fast, can reuse ICN types (DID, Ledger, TrustGraph)
- Cons: More code to write, less off-the-shelf visualization

**Recommendation**: Start with Python/Mesa for rapid iteration, rewrite in Rust if performance matters.

### Directory Structure
```
sims/mutual-credit/
  ├── agents.py          # Agent behavior definitions
  ├── economy.py         # Transaction logic, credit limits
  ├── trust.py           # Trust graph dynamics
  ├── model.py           # Mesa model orchestration
  ├── scenarios/         # Parameter sets for different tests
  │   ├── baseline.json
  │   ├── high_demurrage.json
  │   ├── low_trust.json
  ├── analysis/          # Jupyter notebooks for visualization
  │   ├── velocity.ipynb
  │   ├── inequality.ipynb
  ├── results/           # Simulation output (CSV, plots)
  └── README.md
```

---

## Validation Against Pilot Data

Once the pilot community (Phase C2) is running:

1. **Export real transaction data**:
   - Transaction volume per week
   - Balance distribution
   - Default rate
   - Trust dynamics

2. **Calibrate simulation**:
   - Adjust agent behavior probabilities to match observed patterns
   - Tune parameters to reproduce actual velocity, inequality, defaults

3. **Test counterfactuals**:
   - "What if we had 10% higher demurrage?"
   - "What if credit limits were 2x higher?"
   - "What if we had 5% more free-riders?"

4. **Predict outcomes**:
   - "If we grow to 200 members, will velocity hold?"
   - "If defaults spike, what interventions work?"

**Success Criteria**: Simulation predictions match pilot outcomes within 20% margin.

---

## Recommended Defaults (Preliminary)

**IMPORTANT**: These are hypotheses to be tested, not final values.

### Credit Limits
- **Initial**: -20 credits (enough for 2-3 small transactions)
- **Maximum**: -500 credits (caps extreme exposure)
- **Growth**: +10 credits per 50 credits cleared (rewards reliability)
- **Trust multiplier**: 2x at max trust (score > 0.8)

### Demurrage
- **Rate**: -1% per month on positive balances > 50
- **Rationale**: Gentle nudge to spend, not punitive
- **Exempt**: Balances under 50 (don't punish small savers)

### Default Handling
- **Threshold**: Negative balance > 100 for > 90 days
- **Action**: Freeze further borrowing, flag for dispute resolution
- **Write-off**: Community vote can socialize loss (mark as bad debt)

### New Member Protection
- **Ramp period**: 3 months before reaching baseline credit limit
- **Contribution requirement**: Must clear at least 10 credits in first month
- **Monitoring**: Flag accounts with unusual borrowing patterns

---

## Open Questions

1. **Demurrage psychology**: Does it create resentment or acceptance? (Can't simulate this, need real users)
2. **Trust graph manipulation**: Can agents collude to artificially boost each other's trust?
3. **External shocks**: How does the system handle sudden loss of high-trust members?
4. **Interoperability**: If ICN connects to traditional banking, how does pricing stability change?

---

## Next Steps

**Immediate (Week 1-2)**:
- [ ] Set up Python/Mesa simulation environment
- [ ] Implement basic agent types (reciprocators, hoarders, free-riders)
- [ ] Run baseline scenario (no demurrage, fixed credit limits)

**Short-term (Month 1)**:
- [ ] Test demurrage scenarios (0%, 1%, 2%, 5%)
- [ ] Test credit limit curves (linear, logarithmic, stepwise)
- [ ] Visualize velocity, inequality, default rates

**Medium-term (Month 2-3)**:
- [ ] Calibrate against pilot data (once available)
- [ ] Run sensitivity analysis (which parameters matter most?)
- [ ] Document recommended defaults for Phase 12

**Long-term (Month 4+)**:
- [ ] Rewrite in Rust for performance (if needed)
- [ ] Add governance modeling (how do decisions affect economics?)
- [ ] Model federation scenarios (multi-community trade)

---

## References

**Historical Mutual Credit Systems**:
- Lietaer, Bernard. "The Future of Money" (2001) - Comprehensive survey
- Williams, Colin. "A Commodified World? Mapping the Limits of Capitalism" (2005) - LETS case studies
- Stodder, James. "Reciprocal Exchange Networks" (2009) - WIR Bank analysis

**Agent-Based Modeling**:
- Wilensky, Uri. "NetLogo" - Classic ABM platform
- Mesa: Python framework for ABM (mesa.readthedocs.io)
- Epstein, Joshua. "Generative Social Science" (2006) - ABM methodology

**Economic Theory**:
- Graeber, David. "Debt: The First 5,000 Years" (2011) - Historical context
- North, Peter. "Money and Liberation" (2007) - Mutual credit philosophy
- Gesell, Silvio. "The Natural Economic Order" (1916) - Original demurrage theory

---

**Status**: This is a living research document. Add findings, update hypotheses, link to simulation results as they emerge.
