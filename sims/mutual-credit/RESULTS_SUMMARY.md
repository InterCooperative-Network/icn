# Mutual Credit Economic Simulation - Results Summary

## Overview

This document summarizes the results from our agent-based economic simulations testing different policy configurations for mutual credit systems.

## Simulation Parameters

- **Agents**: 100 per scenario
- **Duration**: 12 months (360 days)
- **Agent Types**: Reciprocators (65%), Hoarders (15%), Free Riders (8%), Opportunists (7%), Super Contributors (5%)
- **Random Seed**: 42 (for reproducibility)

## Scenario Comparison

| Scenario | Velocity | Default Rate | Gini | Hoarding Index |
|----------|----------|--------------|------|----------------|
| **Baseline** | 9,505 | 2.7% | 0.36 | 7.1% |
| **Dynamic Limits** | 7,952 | 1.8% | 0.36 | 8.8% |
| **High Demurrage** | 9,532 | 2.7% | 0.28 | 6.9% |
| **High Free Riders** | 9,458 | 4.1% | 0.42 | 9.5% |
| **Low Trust Network** | 9,588 | 2.5% | 0.35 | 14.2% |

## Key Findings

### 1. Dynamic Credit Limits (2x Trust Multiplier)

**Effect**: 16% velocity reduction, 33% fewer defaults

**Interpretation**:
- Trust-gated credit limits effectively constrain free-riders and opportunists
- Lower transaction volume indicates limits are binding on some agents
- Significantly reduces default rate (2.7% → 1.8%)
- **Trade-off**: Economic activity slows down in exchange for system stability

### 2. High Demurrage (-2% monthly on balances >50)

**Effect**: 22% reduction in inequality (Gini: 0.36 → 0.28)

**Interpretation**:
- Demurrage successfully discourages hoarding
- Creates more equal distribution of wealth
- Velocity remains similar to baseline (9,532 vs 9,505)
- Hoarding index slightly lower (6.9% vs 7.1%)
- **Outcome**: Achieves redistribution goal without harming economic activity

### 3. High Free-Rider Ratio (20% vs 8% baseline)

**Effect**: 51% increase in default rate (2.7% → 4.1%)

**Interpretation**:
- System shows stress but does not collapse
- Higher inequality (Gini: 0.42 vs 0.36)
- Velocity remains relatively stable
- **Finding**: System can tolerate up to 20% free-riders but at cost of higher defaults

### 4. Low Trust Network (30% density vs 60%)

**Effect**: 2x hoarding rate (14.2% vs 7.1%)

**Interpretation**:
- Sparse trust networks lead to MORE hoarding, not less
- Agents hoard when they have fewer trusted trading partners
- Slightly lower default rate (2.5% vs 2.7%) - conservative behavior
- **Insight**: Trust network density critically affects economic behavior

## Economic Safety Mechanisms

### What Worked

1. **Dynamic Credit Limits**: Successfully reduced default rate by 33%
2. **Demurrage**: Reduced inequality by 22% without harming velocity
3. **Agent Reputation System**: Trust scores range from 0.48 to 1.00 after 12 months
4. **Credit Limit Growth**: Agents with high activity and trust reach -500 limit (25x initial -20)

### Failure Modes Tested

All scenarios remained stable with:
- ✅ Velocity > 300 credits/month (range: 7,952 - 9,588)
- ✅ Gini < 0.7 (range: 0.28 - 0.42)
- ✅ Default rate < 15% (range: 1.8% - 4.1%)
- ✅ Hoarding index < 80% (range: 6.9% - 14.2%)

**No critical failures observed** across any scenario.

## Implementation Notes

### Bug Fixes Applied

1. **Agent Trust Scores**: Fixed trust_score updates in transaction handlers
2. **Demurrage Timing**: Moved from agent.step() to model's monthly bookkeeping
3. **Credit Limit Updates**: Moved to monthly bookkeeping to fix timing issue
4. **Credit Limit Comparison**: Changed `min()` to `max()` for negative limit values
5. **Credit Limit Enforcement** (Critical): Added 20-credit buffer to `can_afford_service()` to prevent agents from exceeding limits during transactions. Before fix: 48 agents exceeded -500 limit. After fix: 0 violations across 18,683 transactions.

### Key Learnings

1. **Timing Matters**: Monthly updates must occur AFTER step counter increments
2. **Negative Numbers**: Credit limits are negative, so `max()` gives less restrictive value
3. **Mesa 3.x Changes**: Manual step tracking and agent shuffling required
4. **Trust Dynamics**: Pairwise trust (in trust graph) differs from individual reputation (trust_score)

## Next Steps

Based on these results, recommended next actions:

1. **Test intermediate demurrage rates** (-0.5%, -1%, -1.5%) to find optimal balance
2. **Explore trust multipliers** (1.5x, 3x, 5x) to understand sensitivity
3. **Model network formation** dynamics (how trust density evolves over time)
4. **Add intervention scenarios** (e.g., dispute resolution, community enforcement)
5. **Calibrate to real-world data** from existing mutual credit systems

## Validation

All simulations produced:
- ~12,000-13,500 transactions over 12 months
- Stable agent populations (no crashes or infinite loops)
- Consistent random seed behavior (reproducible results)
- Economically plausible outcomes

## Files Generated

Each scenario produces:
- `results/{scenario}/metrics.csv` - Model-level time series
- `results/{scenario}/agents.csv` - Agent-level data
- `results/{scenario}/transactions.csv` - Full transaction history
- `results/{scenario}/summary.txt` - Human-readable summary
- `results/{scenario}/trust_graph.gexf` - Network visualization data

---

**Simulation Framework**: Mesa 3.3.1
**Analysis Date**: 2025-01-14
**Version**: 1.0
