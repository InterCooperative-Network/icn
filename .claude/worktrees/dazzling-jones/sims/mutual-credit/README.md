# Mutual Credit Economic Simulation

Agent-based simulation for modeling ICN's mutual credit ledger under various economic parameters and agent behaviors.

## Purpose

This simulation validates economic assumptions before deploying ICN in production. It tests:

- **Failure modes**: Hoarding, free-riding, credit limit gaming, velocity collapse
- **Parameter tuning**: Credit limits, demurrage rates, trust dynamics
- **Resilience**: System behavior under stress (defaults, trust breakdown, external shocks)

See [`/docs/econ-modeling.md`](../../docs/econ-modeling.md) for full research context.

## Requirements

```bash
pip install mesa pandas matplotlib seaborn jupyter networkx
```

## Quick Start

```bash
# Run baseline scenario (100 agents, 12 months)
python run_simulation.py --scenario scenarios/baseline.json --output results/baseline

# Run with visualization (slower, interactive)
python run_simulation.py --scenario scenarios/baseline.json --visualize

# Analyze results
jupyter notebook analysis/velocity.ipynb
```

## Architecture

### Agent Types

- **Reciprocators** (60-70%): Balanced give-and-take, reliable
- **Hoarders** (10-15%): Accumulate positive balances, rarely spend
- **Free-Riders** (5-10%): Consume more than provide, may ghost
- **Opportunists** (5-10%): Game trust system, borrow-and-bail
- **Super-Contributors** (5-10%): Provide far more than consume

### Key Files

| File | Purpose |
|------|---------|
| `agents.py` | Agent behavior definitions (5 types) |
| `economy.py` | Transaction logic, credit policies |
| `trust.py` | Trust graph dynamics |
| `model.py` | Mesa model orchestration, metrics |
| `run_simulation.py` | CLI entry point |
| `scenarios/*.json` | Parameter sets for experiments |
| `analysis/*.ipynb` | Jupyter notebooks for visualization |

## Scenarios

### Baseline
- No demurrage
- Fixed credit limits (-50)
- 100 agents, 12 months
- **Purpose**: Establish normal failure mode

### High Demurrage
- 2% monthly demurrage on balances > 50
- **Tests**: Can demurrage prevent hoarding?

### Dynamic Credit Limits
- Trust-gated limits (10x range: -20 to -200)
- **Tests**: Does dynamic scaling prevent gaming?

### Low Trust Environment
- Sparse trust graph (30% density vs 60% baseline)
- **Tests**: Can economy function with low social capital?

## Metrics

### System Health
- **Velocity**: Total transaction volume per month
- **Inequality**: Gini coefficient of balances
- **Default rate**: % of accounts in default
- **Active members**: % transacting in last 30 days

### Failure Indicators
- Velocity < 10 transactions/agent/month → Collapse
- Gini > 0.7 → Extreme inequality
- Default rate > 15% → System stress
- Hoarding: Top 10% hold > 80% of positive balances

## Usage

### Running Experiments

```python
from model import MutualCreditModel
from economy import CreditPolicy

# Create model with custom parameters
policy = CreditPolicy(
    initial_limit=-20,
    max_limit=-500,
    growth_rate=0.1,
    demurrage_rate=-0.02
)

model = MutualCreditModel(
    n_agents=100,
    credit_policy=policy,
    simulation_months=12
)

# Run simulation
for i in range(model.simulation_months * 30):  # 30 ticks per month
    model.step()

# Extract results
results = model.datacollector.get_model_vars_dataframe()
print(results[['Velocity', 'Gini', 'DefaultRate']].tail())
```

### Analyzing Results

```python
import pandas as pd
import matplotlib.pyplot as plt

# Load results
df = pd.read_csv('results/baseline/metrics.csv')

# Plot velocity over time
plt.figure(figsize=(12, 6))
plt.plot(df['Month'], df['Velocity'])
plt.xlabel('Month')
plt.ylabel('Transaction Volume')
plt.title('Economic Velocity Over Time')
plt.savefig('results/baseline/velocity.png')
```

## Validation

Once pilot community (Track C2) is live:

1. Export real transaction data from ICN pilot
2. Calibrate agent probabilities to match observed patterns
3. Test counterfactuals ("What if we had 2x demurrage?")
4. Predict scale-up outcomes

**Success Criteria**: Simulation predictions match pilot data within 20% margin.

## Next Steps

- [ ] **Week 1-2**: Implement basic agents and run baseline
- [ ] **Month 1**: Test demurrage and credit limit scenarios
- [ ] **Month 2-3**: Calibrate against pilot data
- [ ] **Month 4+**: Rewrite in Rust if performance is needed

## References

- Lietaer, B. (2001). *The Future of Money*
- Stodder, J. (2009). *Reciprocal Exchange Networks: WIR Bank Analysis*
- Mesa Documentation: https://mesa.readthedocs.io
