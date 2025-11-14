"""
Trust graph dynamics for mutual credit simulation.

Tracks trust relationships between agents and how they evolve
based on transaction outcomes.
"""

import networkx as nx
from typing import Dict, Tuple, Optional


class TrustGraph:
    """
    Network of trust relationships between agents.

    Trust scores are directional (A trusts B != B trusts A) and
    evolve based on transaction success/failure.
    """

    def __init__(self, n_agents: int, initial_density: float = 0.6):
        """
        Initialize trust graph.

        Args:
            n_agents: Number of agents in the system
            initial_density: Fraction of possible edges to create initially
        """
        self.graph = nx.DiGraph()
        self.n_agents = n_agents

        # Add all agents as nodes
        for i in range(n_agents):
            self.graph.add_node(i, trust_scores={})

        # Create initial random trust relationships
        self._initialize_edges(initial_density)

    def _initialize_edges(self, density: float):
        """Create initial trust edges with random scores."""
        import random

        for i in range(self.n_agents):
            for j in range(self.n_agents):
                if i == j:
                    continue

                # Create edge with probability = density
                if random.random() < density:
                    # Initial trust is normally distributed around 0.5
                    initial_trust = max(0.1, min(0.9, random.gauss(0.5, 0.15)))
                    self.graph.add_edge(i, j, trust=initial_trust)

    def get_trust(self, source_agent, target_agent) -> float:
        """
        Get trust score from source to target.

        Returns 0.5 (neutral) if no edge exists.
        """
        source_id = source_agent.unique_id if hasattr(source_agent, 'unique_id') else source_agent
        target_id = target_agent.unique_id if hasattr(target_agent, 'unique_id') else target_agent

        if self.graph.has_edge(source_id, target_id):
            return self.graph[source_id][target_id]['trust']
        else:
            # No relationship = neutral trust
            return 0.5

    def update_trust(self, source_agent, target_agent, delta: float):
        """
        Update trust based on transaction outcome.

        Positive delta = trust increased (successful transaction)
        Negative delta = trust decreased (failed transaction)
        """
        source_id = source_agent.unique_id if hasattr(source_agent, 'unique_id') else source_agent
        target_id = target_agent.unique_id if hasattr(target_agent, 'unique_id') else target_agent

        # Create edge if it doesn't exist
        if not self.graph.has_edge(source_id, target_id):
            self.graph.add_edge(source_id, target_id, trust=0.5)

        # Update trust (clamped to [0, 1])
        current_trust = self.graph[source_id][target_id]['trust']
        new_trust = max(0.0, min(1.0, current_trust + delta))
        self.graph[source_id][target_id]['trust'] = new_trust

    def record_successful_transaction(self, provider, consumer, value: int):
        """
        Update trust based on successful transaction.

        Both parties gain trust in each other (reciprocal).
        """
        # Trust grows with larger transactions (but diminishing returns)
        delta = min(0.05, value * 0.001)

        # Provider trusts consumer (they paid)
        self.update_trust(provider, consumer, delta)

        # Consumer trusts provider (they delivered)
        self.update_trust(consumer, provider, delta)

    def record_default(self, provider, consumer, value: int):
        """
        Update trust based on failed transaction (default).

        Provider loses significant trust in consumer.
        """
        # Trust loss is proportional to debt size
        delta = -min(0.2, value * 0.005)

        # Provider loses trust in consumer
        self.update_trust(provider, consumer, delta)

        # Consumer's general trustworthiness degrades (reputation hit)
        # All agents who know about the consumer lose some trust
        for agent_id in self.graph.predecessors(consumer.unique_id if hasattr(consumer, 'unique_id') else consumer):
            self.update_trust(agent_id, consumer, delta * 0.5)  # Smaller indirect effect

    def get_transitive_trust(self, source_agent, target_agent, max_hops: int = 2) -> float:
        """
        Calculate transitive trust via shortest path.

        If A trusts B (0.8) and B trusts C (0.7), then A transitively
        trusts C at ~0.56 (0.8 * 0.7).
        """
        source_id = source_agent.unique_id if hasattr(source_agent, 'unique_id') else source_agent
        target_id = target_agent.unique_id if hasattr(target_agent, 'unique_id') else target_agent

        try:
            # Find shortest path
            path = nx.shortest_path(self.graph, source_id, target_id)

            if len(path) > max_hops + 1:
                # Too many hops, no transitive trust
                return 0.0

            # Calculate product of trust along path
            trust_product = 1.0
            for i in range(len(path) - 1):
                edge_trust = self.graph[path[i]][path[i+1]]['trust']
                trust_product *= edge_trust

            return trust_product

        except nx.NetworkXNoPath:
            # No path exists
            return 0.0

    def get_avg_trust_in_agent(self, agent) -> float:
        """Calculate average trust score that others have in this agent."""
        agent_id = agent.unique_id if hasattr(agent, 'unique_id') else agent

        incoming_edges = self.graph.in_edges(agent_id, data=True)
        if not incoming_edges:
            return 0.5  # Neutral

        trust_scores = [data['trust'] for _, _, data in incoming_edges]
        return sum(trust_scores) / len(trust_scores)

    def get_avg_trust_by_agent(self, agent) -> float:
        """Calculate average trust score this agent has in others."""
        agent_id = agent.unique_id if hasattr(agent, 'unique_id') else agent

        outgoing_edges = self.graph.out_edges(agent_id, data=True)
        if not outgoing_edges:
            return 0.5  # Neutral

        trust_scores = [data['trust'] for _, _, data in outgoing_edges]
        return sum(trust_scores) / len(trust_scores)

    def get_trust_statistics(self) -> Dict[str, float]:
        """Get aggregate trust graph statistics."""
        all_trust_scores = [data['trust'] for _, _, data in self.graph.edges(data=True)]

        if not all_trust_scores:
            return {
                'mean_trust': 0.5,
                'median_trust': 0.5,
                'min_trust': 0.0,
                'max_trust': 1.0,
                'edge_count': 0,
                'density': 0.0
            }

        import statistics

        return {
            'mean_trust': statistics.mean(all_trust_scores),
            'median_trust': statistics.median(all_trust_scores),
            'min_trust': min(all_trust_scores),
            'max_trust': max(all_trust_scores),
            'edge_count': len(all_trust_scores),
            'density': self.graph.number_of_edges() / (self.n_agents * (self.n_agents - 1))
        }

    def detect_isolated_agents(self, threshold: float = 0.3) -> list:
        """
        Find agents with low average trust (potential outcasts).

        Returns list of agent IDs with avg incoming trust below threshold.
        """
        isolated = []
        for agent_id in self.graph.nodes():
            incoming_edges = self.graph.in_edges(agent_id, data=True)
            if not incoming_edges:
                isolated.append(agent_id)
                continue

            trust_scores = [data['trust'] for _, _, data in incoming_edges]
            avg_trust = sum(trust_scores) / len(trust_scores)

            if avg_trust < threshold:
                isolated.append(agent_id)

        return isolated

    def detect_trust_clusters(self) -> list:
        """
        Find strongly connected components (cliques of mutual trust).

        Returns list of agent ID sets that form trust clusters.
        """
        # Use weakly connected components (ignore edge direction)
        undirected = self.graph.to_undirected()
        return list(nx.connected_components(undirected))

    def export_to_gexf(self, filename: str):
        """Export trust graph to GEXF format for visualization (Gephi)."""
        nx.write_gexf(self.graph, filename)

    def __repr__(self):
        stats = self.get_trust_statistics()
        return (f"TrustGraph(nodes={self.n_agents}, edges={stats['edge_count']}, "
                f"mean_trust={stats['mean_trust']:.2f}, density={stats['density']:.2f})")
