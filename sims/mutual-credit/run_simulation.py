#!/usr/bin/env python3
"""
CLI for running mutual credit economic simulations.

Usage:
    python run_simulation.py --scenario scenarios/baseline.json
    python run_simulation.py --scenario scenarios/high_demurrage.json --output results/demurrage
    python run_simulation.py --all  # Run all scenarios
"""

import argparse
import json
import sys
from pathlib import Path

from model import MutualCreditModel
from economy import CreditPolicy, validate_credit_policy
from agents import AgentType


def load_scenario(scenario_path: str) -> dict:
    """Load scenario configuration from JSON file."""
    with open(scenario_path, 'r') as f:
        return json.load(f)


def create_model_from_scenario(scenario: dict) -> MutualCreditModel:
    """Create a MutualCreditModel from scenario configuration."""
    params = scenario['parameters']

    # Create credit policy
    policy_params = params.get('credit_policy', {})
    credit_policy = CreditPolicy(**policy_params)

    # Validate policy
    error = validate_credit_policy(credit_policy)
    if error:
        raise ValueError(f"Invalid credit policy: {error}")

    # Convert agent type distribution to enum keys
    agent_dist_raw = params.get('agent_type_distribution', {})
    agent_distribution = {}
    for type_name, proportion in agent_dist_raw.items():
        agent_type = AgentType(type_name)
        agent_distribution[agent_type] = proportion

    # Create model
    model = MutualCreditModel(
        n_agents=params.get('n_agents', 100),
        credit_policy=credit_policy,
        simulation_months=params.get('simulation_months', 12),
        agent_type_distribution=agent_distribution,
        trust_network_density=params.get('trust_network_density', 0.6),
        seed=params.get('seed')
    )

    return model


def run_scenario(scenario_path: str, output_dir: str = None, verbose: bool = True):
    """Run a single scenario and export results."""
    if verbose:
        print(f"Loading scenario: {scenario_path}")

    scenario = load_scenario(scenario_path)

    if verbose:
        print(f"Scenario: {scenario['name']}")
        print(f"Description: {scenario['description']}")
        print()

    # Create model
    model = create_model_from_scenario(scenario)

    if verbose:
        print(f"Running simulation: {model.n_agents} agents, {model.simulation_months} months")
        print()

    # Run simulation
    model.run_simulation(verbose=verbose)

    # Export results
    if output_dir is None:
        # Default output directory based on scenario name
        scenario_name = Path(scenario_path).stem
        output_dir = f"results/{scenario_name}"

    if verbose:
        print(f"\nExporting results to {output_dir}/")

    model.export_results(output_dir)

    # Print summary
    if verbose:
        summary = model.get_summary_statistics()
        print("\n=== Summary ===")
        print(f"Total transactions: {summary['total_transactions']}")
        print(f"Average velocity: {summary['average_velocity']:.1f} credits/month")
        print(f"Default rate: {summary['default_rate']:.2%}")
        print(f"Gini coefficient: {summary['gini_coefficient']:.2f}")
        print(f"Hoarding index: {summary['hoarding_index']:.2%}")
        print(f"Agents in default: {summary['agents_in_default']}")
        print(f"Average trust: {summary['avg_trust']:.2f}")

        # Check for failure modes
        print("\n=== Failure Mode Detection ===")
        if summary['average_velocity'] < 300:
            print("⚠️  VELOCITY COLLAPSE: Transaction volume critically low")
        if summary['gini_coefficient'] > 0.7:
            print("⚠️  EXTREME INEQUALITY: Gini coefficient > 0.7")
        if summary['default_rate'] > 0.15:
            print("⚠️  HIGH DEFAULT RATE: System stress detected")
        if summary['hoarding_index'] > 0.8:
            print("⚠️  HOARDING CONCENTRATION: Top 10% hold >80% of positive balances")

        if (summary['average_velocity'] >= 300 and
            summary['gini_coefficient'] <= 0.7 and
            summary['default_rate'] <= 0.15 and
            summary['hoarding_index'] <= 0.8):
            print("✅ No critical failure modes detected")

    return model


def run_all_scenarios():
    """Run all scenarios in the scenarios/ directory."""
    scenarios_dir = Path("scenarios")

    if not scenarios_dir.exists():
        print("Error: scenarios/ directory not found")
        sys.exit(1)

    scenario_files = list(scenarios_dir.glob("*.json"))

    if not scenario_files:
        print("No scenario files found in scenarios/")
        sys.exit(1)

    print(f"Found {len(scenario_files)} scenarios\n")
    print("=" * 80)

    results = []

    for scenario_file in sorted(scenario_files):
        print(f"\n{'=' * 80}")
        print(f"SCENARIO: {scenario_file.name}")
        print("=" * 80)

        try:
            model = run_scenario(str(scenario_file), verbose=True)
            results.append({
                'scenario': scenario_file.stem,
                'stats': model.get_summary_statistics()
            })
        except Exception as e:
            print(f"❌ Error running scenario: {e}")
            import traceback
            traceback.print_exc()

        print()

    # Comparative summary
    print("\n" + "=" * 80)
    print("COMPARATIVE SUMMARY")
    print("=" * 80)
    print()

    # Table header
    print(f"{'Scenario':<20} {'Velocity':<12} {'Default%':<10} {'Gini':<8} {'Hoarding%':<12}")
    print("-" * 80)

    for result in results:
        stats = result['stats']
        print(f"{result['scenario']:<20} "
              f"{stats['average_velocity']:>11.1f} "
              f"{stats['default_rate']:>9.1%} "
              f"{stats['gini_coefficient']:>7.2f} "
              f"{stats['hoarding_index']:>11.1%}")


def main():
    parser = argparse.ArgumentParser(
        description="Run mutual credit economic simulations",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python run_simulation.py --scenario scenarios/baseline.json
  python run_simulation.py --scenario scenarios/high_demurrage.json --output custom_output/
  python run_simulation.py --all
        """
    )

    parser.add_argument(
        '--scenario',
        type=str,
        help='Path to scenario JSON file'
    )

    parser.add_argument(
        '--output',
        type=str,
        help='Output directory for results (default: results/<scenario_name>)'
    )

    parser.add_argument(
        '--all',
        action='store_true',
        help='Run all scenarios in scenarios/ directory'
    )

    parser.add_argument(
        '--quiet',
        action='store_true',
        help='Suppress verbose output'
    )

    args = parser.parse_args()

    if args.all:
        run_all_scenarios()
    elif args.scenario:
        run_scenario(args.scenario, args.output, verbose=not args.quiet)
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == '__main__':
    main()
