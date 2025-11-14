#!/usr/bin/env python3
"""Debug script to test demurrage directly."""

import sys
from model import MutualCreditModel
from economy import CreditPolicy

# Create model with HIGH demurrage
policy = CreditPolicy(
    initial_limit=-20,
    max_limit=-500,
    growth_rate=0.1,
    trust_multiplier=1.0,
    demurrage_rate=-0.02,  # 2% monthly
    demurrage_threshold=50,  # Apply above 50 credits
)

model = MutualCreditModel(
    n_agents=10,  # Small number for debugging
    credit_policy=policy,
    simulation_months=2,  # Just 2 months
    seed=42
)

print("=" * 80)
print("DEMURRAGE DEBUG TEST")
print("=" * 80)
print(f"Demurrage rate: {policy.demurrage_rate}")
print(f"Demurrage threshold: {policy.demurrage_threshold}")
print()

# Run 1 month
for i in range(30):
    model.step()

print(f"After Month 1:")
print(f"  Total demurrage applied: {model.total_demurrage_applied}")
print(f"  Agents with balance > {policy.demurrage_threshold}:")
for agent in model.agents:
    if agent.balance > policy.demurrage_threshold:
        print(f"    Agent #{agent.unique_id}: balance={agent.balance}, trust={agent.trust_score:.2f}")

# Run month 2
for i in range(30):
    model.step()

print(f"\nAfter Month 2:")
print(f"  Total demurrage applied: {model.total_demurrage_applied}")
print(f"  Agents with balance > {policy.demurrage_threshold}:")
for agent in model.agents:
    if agent.balance > policy.demurrage_threshold:
        print(f"    Agent #{agent.unique_id}: balance={agent.balance}, trust={agent.trust_score:.2f}")

print("\n" + "=" * 80)
if model.total_demurrage_applied > 0:
    print("✅ DEMURRAGE IS WORKING")
else:
    print("❌ DEMURRAGE NOT APPLIED")
print("=" * 80)
