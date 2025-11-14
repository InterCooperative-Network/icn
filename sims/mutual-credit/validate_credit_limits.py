#!/usr/bin/env python3
"""
Validation script for credit limit enforcement.

Tests that agents cannot exceed their credit limits during transactions.
This prevents the bug where agents could borrow beyond their safety cap.
"""

from model import MutualCreditModel
from economy import CreditPolicy


def validate_credit_limit_enforcement():
    """Run simulation and verify no agents exceed their credit limits."""
    print("=" * 80)
    print("CREDIT LIMIT ENFORCEMENT VALIDATION")
    print("=" * 80)
    print()

    # Test with aggressive parameters to stress-test the limits
    policy = CreditPolicy(
        initial_limit=-20,
        max_limit=-500,
        trust_multiplier=2.0
    )

    model = MutualCreditModel(
        n_agents=100,
        credit_policy=policy,
        simulation_months=12,
        seed=42
    )

    print(f"Running 12-month simulation with {model.n_agents} agents...")
    model.run_simulation(verbose=False)

    # Check for violations
    violations = []
    for agent in model.agents:
        if agent.balance < agent.credit_limit:
            violations.append({
                'id': agent.unique_id,
                'balance': agent.balance,
                'limit': agent.credit_limit,
                'excess': agent.balance - agent.credit_limit
            })

    print(f"\nResults:")
    print(f"  Total transactions: {model.metrics.total_transactions:,}")
    print(f"  Total agents: {len(model.agents)}")
    print(f"  Credit limit violations: {len(violations)}")
    print()

    if violations:
        print("❌ VALIDATION FAILED - Credit limit violations detected!")
        print("\nTop 10 violations (worst first):")
        violations_sorted = sorted(violations, key=lambda x: x['excess'])
        for v in violations_sorted[:10]:
            print(f"  Agent {v['id']}: balance={v['balance']}, "
                  f"limit={v['limit']}, exceeded by {v['excess']}")
        return False
    else:
        print("✅ VALIDATION PASSED - No credit limit violations")

        # Show how close agents get to their limits
        margins = [agent.balance - agent.credit_limit for agent in model.agents]
        print(f"\nMargin statistics (balance - limit):")
        print(f"  Minimum: {min(margins)} credits (closest to limit)")
        print(f"  Maximum: {max(margins)} credits (furthest from limit)")
        print(f"  Average: {sum(margins) / len(margins):.1f} credits")

        # Show limit distribution
        at_max = sum(1 for a in model.agents if a.credit_limit == policy.max_limit)
        print(f"\nCredit limit progression:")
        print(f"  Agents at max limit ({policy.max_limit}): {at_max}/{len(model.agents)}")
        print(f"  Percentage: {100 * at_max / len(model.agents):.1f}%")

        return True


if __name__ == '__main__':
    success = validate_credit_limit_enforcement()
    exit(0 if success else 1)
