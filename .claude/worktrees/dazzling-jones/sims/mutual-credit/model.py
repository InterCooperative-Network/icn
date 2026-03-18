"""
Mesa model for mutual credit economic simulation.

Orchestrates agents, economy, and trust graph to simulate
a functioning mutual credit system over time.
"""

from mesa import Model
from mesa.datacollection import DataCollector
from mesa.agent import AgentSet
import random

from agents import MutualCreditAgent, AgentType
from economy import CreditPolicy, EconomyMetrics, Transaction
from trust import TrustGraph


class MutualCreditModel(Model):
    """
    Agent-based model of a mutual credit economy.

    Simulates N agents transacting over time, with configurable
    economic parameters and agent type distributions.
    """

    def __init__(
        self,
        n_agents: int = 100,
        credit_policy: CreditPolicy = None,
        simulation_months: int = 12,
        agent_type_distribution: dict = None,
        trust_network_density: float = 0.6,
        seed: int = None
    ):
        super().__init__()

        # Seed RNG for reproducibility
        if seed is not None:
            random.seed(seed)

        # Model parameters
        self.n_agents = n_agents
        self.simulation_months = simulation_months
        self.credit_policy = credit_policy or CreditPolicy()

        # Default agent distribution
        if agent_type_distribution is None:
            agent_type_distribution = {
                AgentType.RECIPROCATOR: 0.65,
                AgentType.HOARDER: 0.15,
                AgentType.FREE_RIDER: 0.08,
                AgentType.OPPORTUNIST: 0.07,
                AgentType.SUPER_CONTRIBUTOR: 0.05
            }

        self.agent_type_distribution = agent_type_distribution

        # Initialize trust graph
        self.trust_graph = TrustGraph(n_agents, initial_density=trust_network_density)

        # Track simulation steps manually (Mesa 3.x doesn't have schedule.steps)
        self.steps = 0

        # Initialize economy metrics
        self.metrics = EconomyMetrics()

        # Transaction ledger
        self.transaction_history = []

        # Demurrage tracking
        self.total_demurrage_applied = 0

        # Create agents
        self._create_agents()

        # Data collection
        self.datacollector = DataCollector(
            model_reporters={
                "Velocity": lambda m: m.metrics.velocity(),
                "TotalVolume": lambda m: m.metrics.total_volume,
                "Transactions": lambda m: m.metrics.total_transactions,
                "DefaultRate": lambda m: m.metrics.default_rate(),
                "Gini": lambda m: m.metrics.gini_coefficient([a.balance for a in m.agents]),
                "HoardingIndex": lambda m: m.metrics.hoarding_index([a.balance for a in m.agents]),
                "AgentsInDefault": lambda m: sum(1 for a in m.agents if a.days_in_default > 0),
                "AvgTrust": lambda m: m.trust_graph.get_trust_statistics()['mean_trust'],
                "TotalDemurrage": lambda m: m.total_demurrage_applied,
                "PositiveBalances": lambda m: sum(1 for a in m.agents if a.balance > 0),
                "NegativeBalances": lambda m: sum(1 for a in m.agents if a.balance < 0),
                "AvgBalance": lambda m: sum(a.balance for a in m.agents) / m.n_agents,
            },
            agent_reporters={
                "Balance": "balance",
                "CreditLimit": "credit_limit",
                "Trust": "trust_score",
                "Type": lambda a: a.agent_type.value,
                "TransactionCount": "transaction_count",
                "DaysInDefault": "days_in_default",
            }
        )

        self.running = True
        self.datacollector.collect(self)

    def _create_agents(self):
        """Create agents with specified type distribution."""
        # Calculate count for each type
        agent_counts = {}
        remaining = self.n_agents

        for agent_type, proportion in self.agent_type_distribution.items():
            count = int(self.n_agents * proportion)
            agent_counts[agent_type] = count
            remaining -= count

        # Assign remaining agents to reciprocators
        agent_counts[AgentType.RECIPROCATOR] += remaining

        # Create agents
        agent_id = 0
        for agent_type, count in agent_counts.items():
            for _ in range(count):
                agent = MutualCreditAgent(agent_id, self, agent_type)
                agent_id += 1

    def execute_transaction(self, provider, consumer, value: int):
        """
        Execute a credit transaction between two agents.

        Provider earns credits (balance increases)
        Consumer spends credits (balance decreases)
        """
        # Update balances
        provider.balance += value
        consumer.balance -= value

        # Record transaction
        transaction = Transaction(
            provider.unique_id,
            consumer.unique_id,
            value,
            self.steps,
            success=True
        )
        self.transaction_history.append(transaction)

        # Update metrics
        self.metrics.record_transaction(value, success=True)

        # Update trust (successful transaction)
        self.trust_graph.record_successful_transaction(provider, consumer, value)

        # Update agent reputation (trust_score) based on successful transaction
        # Both agents gain slight reputation boost for completing transaction
        provider.update_trust(+0.01)  # Small boost for providing service
        consumer.update_trust(+0.01)  # Small boost for paying

        # Update agent stats
        provider.transaction_count += 1
        provider.successful_transactions += 1
        provider.total_cleared += value
        provider.last_transaction_tick = self.steps

        consumer.transaction_count += 1
        consumer.successful_transactions += 1
        consumer.total_cleared += value
        consumer.last_transaction_tick = self.steps

    def record_default(self, consumer, provider, value: int):
        """
        Record a failed transaction (default).

        Consumer failed to pay for service received.
        """
        # Record failed transaction
        transaction = Transaction(
            provider.unique_id,
            consumer.unique_id,
            value,
            self.steps,
            success=False
        )
        self.transaction_history.append(transaction)

        # Update metrics
        self.metrics.record_transaction(value, success=False)

        # Update trust (default damages trust)
        self.trust_graph.record_default(provider, consumer, value)

        # Update agent reputation - consumer loses trust for defaulting
        # Penalty proportional to debt (larger defaults = bigger hit)
        penalty = min(0.1, value * 0.002)  # Max 10% penalty
        consumer.update_trust(-penalty)

        # Update agent stats
        consumer.failed_transactions += 1

    def step(self):
        """Advance the model by one time step (1 day)."""
        # Agents act (random order in Mesa 3.x)
        agents_shuffled = list(self.agents)
        random.shuffle(agents_shuffled)
        for agent in agents_shuffled:
            agent.step()

        # Increment step counter
        self.steps += 1

        # Monthly bookkeeping
        if self.steps % 30 == 0:
            self.metrics.end_month()

            # Update credit limits and activity tracking for all agents
            for agent in self.agents:
                agent.months_active += 1
                agent.update_credit_limit()

            # Apply demurrage to all agents
            for agent in self.agents:
                agent.apply_demurrage()

            # Check for agents in default
            self.metrics.agents_in_default = sum(
                1 for a in self.agents
                if self.credit_policy.is_in_default(a.balance, a.days_in_default)
            )

        # Collect data
        self.datacollector.collect(self)

        # Stop after simulation period
        if self.steps >= (self.simulation_months * 30):
            self.running = False

    def run_simulation(self, verbose: bool = False):
        """Run the full simulation."""
        total_steps = self.simulation_months * 30

        for i in range(total_steps):
            self.step()

            if verbose and i % 30 == 0:
                month = i // 30 + 1
                print(f"Month {month}/{self.simulation_months}: "
                      f"{self.metrics.total_transactions} txns, "
                      f"velocity={self.metrics.velocity():.1f}, "
                      f"defaults={self.metrics.agents_in_default}")

        if verbose:
            print(f"\nSimulation complete!")
            print(f"Total transactions: {self.metrics.total_transactions}")
            print(f"Default rate: {self.metrics.default_rate():.2%}")
            print(f"Final Gini: {self.metrics.gini_coefficient([a.balance for a in self.agents]):.2f}")

    def get_summary_statistics(self) -> dict:
        """Get summary statistics for the simulation."""
        balances = [a.balance for a in self.agents]

        return {
            'total_transactions': self.metrics.total_transactions,
            'total_volume': self.metrics.total_volume,
            'average_velocity': self.metrics.velocity(),
            'default_rate': self.metrics.default_rate(),
            'gini_coefficient': self.metrics.gini_coefficient(balances),
            'hoarding_index': self.metrics.hoarding_index(balances),
            'agents_in_default': self.metrics.agents_in_default,
            'avg_trust': self.trust_graph.get_trust_statistics()['mean_trust'],
            'total_demurrage_applied': self.total_demurrage_applied,
            'balance_stats': {
                'mean': sum(balances) / len(balances) if balances else 0,
                'min': min(balances) if balances else 0,
                'max': max(balances) if balances else 0,
                'positive_count': sum(1 for b in balances if b > 0),
                'negative_count': sum(1 for b in balances if b < 0),
            },
            'trust_stats': self.trust_graph.get_trust_statistics(),
        }

    def export_results(self, output_dir: str):
        """Export simulation results to CSV files."""
        import os
        import pandas as pd

        os.makedirs(output_dir, exist_ok=True)

        # Model-level metrics
        model_data = self.datacollector.get_model_vars_dataframe()
        model_data['Month'] = model_data.index // 30
        model_data.to_csv(f"{output_dir}/metrics.csv")

        # Agent-level data
        agent_data = self.datacollector.get_agent_vars_dataframe()
        agent_data.to_csv(f"{output_dir}/agents.csv")

        # Transaction history
        tx_data = pd.DataFrame([
            {
                'tick': tx.tick,
                'provider_id': tx.provider_id,
                'consumer_id': tx.consumer_id,
                'value': tx.value,
                'success': tx.success
            }
            for tx in self.transaction_history
        ])
        tx_data.to_csv(f"{output_dir}/transactions.csv", index=False)

        # Summary statistics
        summary = self.get_summary_statistics()
        with open(f"{output_dir}/summary.txt", 'w') as f:
            f.write("=== Mutual Credit Simulation Summary ===\n\n")
            f.write(f"Agents: {self.n_agents}\n")
            f.write(f"Simulation months: {self.simulation_months}\n\n")

            f.write("Economic Outcomes:\n")
            f.write(f"  Total transactions: {summary['total_transactions']}\n")
            f.write(f"  Total volume: {summary['total_volume']} credits\n")
            f.write(f"  Average velocity: {summary['average_velocity']:.1f} credits/month\n")
            f.write(f"  Default rate: {summary['default_rate']:.2%}\n\n")

            f.write("Inequality Metrics:\n")
            f.write(f"  Gini coefficient: {summary['gini_coefficient']:.2f}\n")
            f.write(f"  Hoarding index: {summary['hoarding_index']:.2%}\n\n")

            f.write("System Health:\n")
            f.write(f"  Agents in default: {summary['agents_in_default']}\n")
            f.write(f"  Average trust: {summary['avg_trust']:.2f}\n")
            f.write(f"  Total demurrage applied: {summary['total_demurrage_applied']} credits\n")

        # Export trust graph
        self.trust_graph.export_to_gexf(f"{output_dir}/trust_graph.gexf")

        print(f"Results exported to {output_dir}/")
