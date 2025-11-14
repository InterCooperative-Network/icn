"""
Agent behavior definitions for mutual credit simulation.

Each agent type models different economic behaviors observed in
real-world mutual credit systems.
"""

from mesa import Agent
import random
from enum import Enum


class AgentType(Enum):
    """Agent behavioral archetypes."""
    RECIPROCATOR = "reciprocator"
    HOARDER = "hoarder"
    FREE_RIDER = "free_rider"
    OPPORTUNIST = "opportunist"
    SUPER_CONTRIBUTOR = "super_contributor"


class MutualCreditAgent(Agent):
    """
    Base agent in a mutual credit economy.

    Each agent can provide and request services, building trust
    and accumulating credits/debts over time.
    """

    def __init__(self, unique_id, model, agent_type: AgentType):
        super().__init__(model)
        self.unique_id = unique_id  # Set unique_id manually in Mesa 3.x
        self.agent_type = agent_type

        # Economic state
        self.balance = 0  # Current credit balance
        self.credit_limit = model.credit_policy.initial_limit
        self.max_credit_limit = model.credit_policy.max_limit
        self.total_cleared = 0  # Lifetime cleared transactions

        # Trust & reputation
        self.trust_score = 0.5  # Starts neutral
        self.successful_transactions = 0
        self.failed_transactions = 0
        self.months_active = 0

        # Transaction history
        self.transaction_count = 0
        self.last_transaction_tick = 0
        self.days_in_default = 0

        # Agent-specific parameters
        self.set_agent_parameters()

    def set_agent_parameters(self):
        """Set behavioral parameters based on agent type."""
        if self.agent_type == AgentType.RECIPROCATOR:
            self.provide_probability = 0.3  # Willing to work
            self.consume_probability = 0.3  # Willing to request
            self.default_probability = 0.02  # Rarely defaults
            self.spend_when_positive = 0.6  # Moderate saver

        elif self.agent_type == AgentType.HOARDER:
            self.provide_probability = 0.4  # Works often
            self.consume_probability = 0.1  # Rarely spends
            self.default_probability = 0.01  # Very reliable
            self.spend_when_positive = 0.1  # Almost never spends

        elif self.agent_type == AgentType.FREE_RIDER:
            self.provide_probability = 0.1  # Rarely works
            self.consume_probability = 0.5  # Consumes a lot
            self.default_probability = 0.15  # Often defaults
            self.spend_when_positive = 0.0  # Never has positive balance

        elif self.agent_type == AgentType.OPPORTUNIST:
            # Changes behavior based on trust level
            if self.trust_score < 0.5:
                # Building trust phase
                self.provide_probability = 0.4
                self.consume_probability = 0.2
                self.default_probability = 0.01
            else:
                # Exploitation phase
                self.provide_probability = 0.1
                self.consume_probability = 0.7
                self.default_probability = 0.3
            self.spend_when_positive = 0.3

        elif self.agent_type == AgentType.SUPER_CONTRIBUTOR:
            self.provide_probability = 0.6  # Works a lot
            self.consume_probability = 0.2  # Modest needs
            self.default_probability = 0.0  # Never defaults
            self.spend_when_positive = 0.4  # Eventually spends

    def step(self):
        """Execute one simulation step (1 day)."""
        # Check if in default
        if self.balance < 0 and abs(self.balance) > 100:
            self.days_in_default += 1
        else:
            self.days_in_default = 0

        # Decide whether to transact
        if random.random() < self.provide_probability:
            self.provide_service()

        if random.random() < self.consume_probability:
            self.request_service()

        # Opportunists update behavior based on trust
        if self.agent_type == AgentType.OPPORTUNIST:
            self.set_agent_parameters()

    def provide_service(self):
        """Provide a service to another agent (earn credits)."""
        # Find potential customers
        potential_customers = [a for a in self.model.agents
                              if a != self and a.can_afford_service()]

        if not potential_customers:
            return

        # Select customer (prefer high-trust partners)
        customer = self.select_partner(potential_customers, prefer_high_trust=True)

        # Service value (randomized 5-20 credits)
        value = random.randint(5, 20)

        # Execute transaction
        self.model.execute_transaction(self, customer, value)

    def request_service(self):
        """Request a service from another agent (spend credits)."""
        # Check credit limit
        if not self.can_afford_service():
            return

        # Find potential providers
        potential_providers = [a for a in self.model.agents if a != self]

        if not potential_providers:
            return

        # Select provider (prefer high-trust partners)
        provider = self.select_partner(potential_providers, prefer_high_trust=True)

        # Service value (randomized 5-20 credits)
        value = random.randint(5, 20)

        # Decide whether to default
        if random.random() < self.default_probability and self.balance < 0:
            # Skip payment (default)
            self.model.record_default(self, provider, value)
            return

        # Execute transaction
        self.model.execute_transaction(provider, self, value)

    def can_afford_service(self):
        """Check if agent has room in credit limit."""
        # Hoarders rarely spend even when they can
        if self.agent_type == AgentType.HOARDER and self.balance > 0:
            return random.random() < self.spend_when_positive

        # Others check credit limit
        return self.balance > self.credit_limit

    def select_partner(self, candidates, prefer_high_trust=True):
        """
        Select a transaction partner from candidates.

        Uses trust graph to prefer/avoid partners based on trust scores.
        """
        if not candidates:
            return None

        # Simple selection: weighted by trust score
        if prefer_high_trust and len(candidates) > 1:
            # Get trust scores for all candidates
            trust_weights = [self.model.trust_graph.get_trust(self, c)
                           for c in candidates]

            # Normalize to probabilities
            total = sum(trust_weights)
            if total > 0:
                probabilities = [w / total for w in trust_weights]
                return random.choices(candidates, weights=probabilities)[0]

        # Fallback: random selection
        return random.choice(candidates)

    def update_credit_limit(self):
        """Update credit limit based on cleared volume and trust."""
        if self.total_cleared == 0:
            return

        # Base growth: +10 credits per 50 cleared
        growth = (self.total_cleared // 50) * 10

        # Trust multiplier
        trust_multiplier = 1.0 + (self.trust_score * self.model.credit_policy.trust_multiplier)

        # New limit (use max() since limits are negative - less negative is better)
        new_limit = max(
            self.model.credit_policy.initial_limit - int(growth * trust_multiplier),
            self.max_credit_limit
        )

        self.credit_limit = new_limit

    def apply_demurrage(self):
        """Apply negative interest to positive balances (if enabled)."""
        demurrage_rate = self.model.credit_policy.demurrage_rate
        demurrage_threshold = self.model.credit_policy.demurrage_threshold

        if demurrage_rate < 0 and self.balance > demurrage_threshold:
            # Apply demurrage (negative interest) - REDUCE positive balances
            demurrage_amount = int(self.balance * abs(demurrage_rate))
            self.balance -= demurrage_amount  # Subtract to reduce hoarding incentive
            self.model.total_demurrage_applied += demurrage_amount

    def update_trust(self, delta):
        """Update trust score based on transaction outcome."""
        self.trust_score = max(0.0, min(1.0, self.trust_score + delta))

    def __str__(self):
        return f"{self.agent_type.value.capitalize()} #{self.unique_id} (bal={self.balance}, trust={self.trust_score:.2f})"
