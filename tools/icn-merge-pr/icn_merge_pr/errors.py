"""Typed failures. No magic strings cross a stage boundary.

Every one of these carries a stable outcome code, so a caller maps a failure to a result without
matching on a message. The predecessor passed literal `"PENDING"` between shell stages; a typo
there was a silent fail-open, because an unrecognised string simply did not match "not ready".
"""

from __future__ import annotations

from . import codes


class MergeToolError(Exception):
    """Base class. `outcome` is the stable code a caller reports."""

    outcome = codes.REFUSED_UNAVAILABLE_EVIDENCE

    def __init__(self, detail: str, *, details: list[str] | None = None) -> None:
        super().__init__(detail)
        self.detail = detail
        self.details = list(details or [])


class EvidenceUnavailable(MergeToolError):
    """Evidence needed for the decision could not be read, or was not a value we recognise.

    Unavailable is NOT ready. This includes an enum member GitHub reports that this program's
    pinned vocabulary does not know: an unknown state is not a safe state.
    """

    outcome = codes.REFUSED_UNAVAILABLE_EVIDENCE


class NotDefaultBase(MergeToolError):
    """The target's base is not the externally resolved default branch.

    Raised by the snapshot loader BEFORE any policy is loaded: a non-default base must never get
    to supply the document that defines readiness.
    """

    outcome = codes.REFUSED_NOT_DEFAULT_BASE


class PolicyInvalid(MergeToolError):
    """The pinned policy did not satisfy its own schema."""

    outcome = codes.REFUSED_POLICY_INVALID


class StrategyInvalid(MergeToolError):
    """A merge strategy outside the closed set owned by this code."""

    outcome = codes.REFUSED_STRATEGY_INVALID


class GitHubRefused(MergeToolError):
    """GitHub refused the merge. This is FINAL — never retried with weaker flags or privileges."""

    outcome = codes.REFUSED_GITHUB


class UsageError(MergeToolError):
    outcome = codes.REFUSED_USAGE


class ForbiddenOption(MergeToolError):
    outcome = codes.REFUSED_FORBIDDEN_OPTION


class NotInstalled(MergeToolError):
    outcome = codes.REFUSED_NOT_INSTALLED
