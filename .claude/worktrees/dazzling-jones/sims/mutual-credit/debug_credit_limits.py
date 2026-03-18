#!/usr/bin/env python3
"""
Debug script to verify dynamic credit limits are working.
"""

from model import MutualCreditModel
from economy import CreditPolicy

# Create model with dynamic limits (2x trust multiplier)
policy = CreditPolicy(
    initial_limit=-20,
    max_limit=-500,
    trust_multiplier=2.0  # High multiplier to see effect
)

model = MutualCreditModel(
    n_agents=10,
    credit_policy=policy,
    simulation_months=2,
    seed=42
)

print("=" * 80)
print("CREDIT LIMIT DEBUG TEST")
print("=" * 80)
print(f"Initial limit: {policy.initial_limit}")
print(f"Max limit: {policy.max_limit}")
print(f"Trust multiplier: {policy.trust_multiplier}")
print()

# Run simulation with step-by-step monitoring
total_steps = model.simulation_months * 30
for i in range(total_steps):
    model.step()

    # Print monthly checkpoints
    if model.steps % 30 == 0:
        month = model.steps // 30
        print(f"Month {month} (step {model.steps}):")
        print(f"  Sample agent #0: cleared={model.agents[0].total_cleared}, trust={model.agents[0].trust_score:.2f}, limit={model.agents[0].credit_limit}")
        print()

# Check credit limits
print("Agent Credit Limits After 2 Months:")
print("-" * 80)
print(f"{'Agent':<15} {'Type':<20} {'Balance':<10} {'Trust':<8} {'Cleared':<10} {'Limit':<10}")
print("-" * 80)

for agent in sorted(model.agents, key=lambda a: a.credit_limit, reverse=True):
    print(f"Agent #{agent.unique_id:<8} {agent.agent_type.value:<20} "
          f"{agent.balance:<10} {agent.trust_score:<8.2f} "
          f"{agent.total_cleared:<10} {agent.credit_limit:<10}")

print()

# Statistics
limits = [a.credit_limit for a in model.agents]
print(f"Min limit: {min(limits)} (most restrictive)")
print(f"Max limit: {max(limits)} (most permissive)")
print(f"Avg limit: {sum(limits) / len(limits):.1f}")
print()

if min(limits) == max(limits):
    print("❌ CREDIT LIMITS ARE NOT DYNAMIC - all agents have same limit")
else:
    print(f"✅ CREDIT LIMITS ARE DYNAMIC - range from {min(limits)} to {max(limits)}")
