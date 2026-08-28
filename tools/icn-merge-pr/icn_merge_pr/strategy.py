"""The CLOSED merge-strategy enum, and the ONLY mapping from a policy value to an API value.

There is no `admin` strategy, and there is no code path that builds an option by prefixing a
policy value with `--`. That is not a convention here, it is the shape of the module: a policy
value only ever leaves this file through a dictionary lookup whose keys are written out in
source. A base that could be contributor-controlled cannot widen a literal dict.
"""

from __future__ import annotations

from .errors import StrategyInvalid
from .policy import GH_MERGE_STRATEGIES, MergePolicy

# strategy -> the `merge_method` field of the GitHub merge API. A structured request field, not a
# command-line flag: there is no string being assembled, so there is nothing to inject into.
MERGE_METHOD = {
    "merge": "merge",
    "squash": "squash",
    "rebase": "rebase",
}


def api_merge_method(strategy: object) -> str:
    """Map a strategy through the closed set, or refuse. Never falls through to a default."""
    if not isinstance(strategy, str) or strategy not in MERGE_METHOD:
        raise StrategyInvalid(
            f"merge strategy {strategy!r} is not one of {sorted(MERGE_METHOD)} — the set is owned "
            "by this program's code and cannot be widened by policy, configuration or arguments")
    return MERGE_METHOD[strategy]


def select_strategy(policy: MergePolicy, requested: str | None,
                    reason: str | None) -> tuple[str, str | None]:
    """Choose the strategy for this merge: the trusted default, or an OPERATOR-STATED exception.

    Whether a PR is a subtree import is not mechanically derivable, so the documented exception is
    never inferred from the PR. An operator selects it explicitly and must say why — the reason is
    recorded in the result. Nothing else may be selected: an argument naming an arbitrary strategy
    is refused even when it is a member of the closed set.
    """
    if requested is None:
        return policy.default_strategy, None
    if requested not in GH_MERGE_STRATEGIES:
        raise StrategyInvalid(
            f"merge strategy {requested!r} is not one of {sorted(GH_MERGE_STRATEGIES)}")
    if requested == policy.default_strategy:
        return requested, reason
    if policy.exception_strategy is None or requested != policy.exception_strategy:
        raise StrategyInvalid(
            f"merge strategy {requested!r} is neither the policy default "
            f"({policy.default_strategy!r}) nor the one documented exception "
            f"({policy.exception_strategy!r})")
    if not reason or not reason.strip():
        raise StrategyInvalid(
            f"selecting the documented exception ({requested!r}, for "
            f"{policy.exception_applies_to!r}) is an operator statement and requires a stated "
            "reason")
    return requested, reason.strip()
