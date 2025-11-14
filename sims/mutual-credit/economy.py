"""
Credit policy and transaction logic for mutual credit economy.

Implements the economic rules that govern credit limits, demurrage,
and default handling.
"""

from dataclasses import dataclass
from typing import Optional


@dataclass
class CreditPolicy:
    """
    Economic parameters governing the mutual credit system.

    These parameters can be tuned to test different scenarios and
    prevent failure modes.
    """
    # Credit limits
    initial_limit: int = -20  # Starting borrowing limit
    max_limit: int = -500  # Maximum borrowing limit
    growth_rate: float = 0.1  # How fast limits grow with cleared volume
    trust_multiplier: float = 1.0  # How much trust affects limits (2.0 = doubles at max trust)

    # Demurrage (negative interest on positive balances)
    demurrage_rate: float = 0.0  # Monthly rate (negative value, e.g., -0.02 = -2%)
    demurrage_threshold: int = 50  # Only apply demurrage above this balance

    # Default handling
    default_threshold: int = 100  # Balance below which account is considered at risk
    default_period_days: int = 90  # Days before flagging as default
    write_off_threshold: int = 200  # Threshold for community write-off vote

    # New member protection
    ramp_period_months: int = 3  # Months before reaching baseline limit
    contribution_requirement: int = 10  # Min credits cleared in first month

    def calculate_limit(self, balance: int, cleared_volume: int, trust_score: float, months_active: int) -> int:
        """
        Calculate credit limit for an agent based on history and trust.

        Args:
            balance: Current balance
            cleared_volume: Total lifetime cleared transactions
            trust_score: Trust score (0.0-1.0)
            months_active: Months since joining

        Returns:
            Credit limit (negative value)
        """
        # Base limit (grows with cleared volume)
        base_growth = (cleared_volume // 50) * int(self.growth_rate * 100)

        # Trust bonus
        trust_bonus = int(base_growth * trust_score * self.trust_multiplier)

        # New member protection (gradual ramp)
        if months_active < self.ramp_period_months:
            ramp_factor = months_active / self.ramp_period_months
            base_growth = int(base_growth * ramp_factor)
            trust_bonus = int(trust_bonus * ramp_factor)

        # Calculate final limit
        limit = self.initial_limit - base_growth - trust_bonus

        # Cap at maximum
        return max(limit, self.max_limit)

    def is_in_default(self, balance: int, days_in_default: int) -> bool:
        """Check if an account is in default."""
        return balance < -self.default_threshold and days_in_default >= self.default_period_days

    def requires_write_off(self, balance: int) -> bool:
        """Check if debt should be considered for write-off."""
        return balance < -self.write_off_threshold


class Transaction:
    """Record of a credit transaction between two agents."""

    def __init__(self, provider_id: int, consumer_id: int, value: int, tick: int, success: bool = True):
        self.provider_id = provider_id
        self.consumer_id = consumer_id
        self.value = value
        self.tick = tick
        self.success = success  # False if defaulted

    def __repr__(self):
        status = "✓" if self.success else "✗"
        return f"{status} T{self.tick}: #{self.provider_id} → #{self.consumer_id} ({self.value}cr)"


class EconomyMetrics:
    """
    Tracks aggregate economic metrics for the simulation.

    Used to detect failure modes and measure system health.
    """

    def __init__(self):
        # Transaction metrics
        self.total_transactions = 0
        self.total_volume = 0
        self.successful_transactions = 0
        self.defaulted_transactions = 0

        # Balance metrics
        self.total_positive_balances = 0
        self.total_negative_balances = 0
        self.balance_sum = 0  # Should always be 0 (double-entry)

        # Defaults and write-offs
        self.agents_in_default = 0
        self.total_write_offs = 0
        self.write_off_volume = 0

        # Demurrage
        self.total_demurrage_applied = 0

        # Velocity (transactions per month)
        self.monthly_volume = []
        self.current_month_volume = 0

    def record_transaction(self, value: int, success: bool = True):
        """Record a transaction."""
        self.total_transactions += 1
        if success:
            self.total_volume += value
            self.current_month_volume += value
            self.successful_transactions += 1
        else:
            self.defaulted_transactions += 1

    def end_month(self):
        """Called at the end of each month to update metrics."""
        self.monthly_volume.append(self.current_month_volume)
        self.current_month_volume = 0

    def velocity(self) -> float:
        """Calculate average monthly transaction velocity."""
        if not self.monthly_volume:
            return 0.0
        return sum(self.monthly_volume) / len(self.monthly_volume)

    def default_rate(self) -> float:
        """Calculate percentage of transactions that defaulted."""
        if self.total_transactions == 0:
            return 0.0
        return self.defaulted_transactions / self.total_transactions

    def gini_coefficient(self, balances: list[int]) -> float:
        """
        Calculate Gini coefficient of balance inequality.

        0.0 = perfect equality, 1.0 = perfect inequality
        """
        if not balances or len(balances) < 2:
            return 0.0

        # Sort balances
        sorted_balances = sorted([abs(b) for b in balances])
        n = len(sorted_balances)

        # Calculate Gini
        cumulative = 0
        for i, b in enumerate(sorted_balances):
            cumulative += (2 * (i + 1) - n - 1) * b

        if sum(sorted_balances) == 0:
            return 0.0

        return cumulative / (n * sum(sorted_balances))

    def hoarding_index(self, balances: list[int]) -> float:
        """
        Calculate hoarding concentration.

        Returns the % of total positive balances held by top 10% of agents.
        """
        positive_balances = [b for b in balances if b > 0]
        if not positive_balances:
            return 0.0

        sorted_balances = sorted(positive_balances, reverse=True)
        top_10_percent = max(1, len(sorted_balances) // 10)
        top_10_total = sum(sorted_balances[:top_10_percent])

        total_positive = sum(positive_balances)
        if total_positive == 0:
            return 0.0

        return top_10_total / total_positive

    def __repr__(self):
        return (f"EconomyMetrics(txns={self.total_transactions}, "
                f"velocity={self.velocity():.1f}, "
                f"default_rate={self.default_rate():.2%})")


def validate_credit_policy(policy: CreditPolicy) -> Optional[str]:
    """
    Validate that a credit policy makes economic sense.

    Returns error message if invalid, None if valid.
    """
    if policy.initial_limit >= 0:
        return "initial_limit must be negative (it's a debt limit)"

    if policy.max_limit >= policy.initial_limit:
        return "max_limit must be more negative than initial_limit"

    if policy.demurrage_rate > 0:
        return "demurrage_rate must be negative or zero"

    if policy.demurrage_threshold < 0:
        return "demurrage_threshold must be positive"

    if policy.trust_multiplier < 0:
        return "trust_multiplier must be non-negative"

    if policy.default_threshold < 0:
        return "default_threshold must be positive"

    if policy.write_off_threshold <= policy.default_threshold:
        return "write_off_threshold should be greater than default_threshold"

    return None
