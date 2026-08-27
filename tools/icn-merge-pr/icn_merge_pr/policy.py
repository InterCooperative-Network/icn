"""Load and validate merge policy from a PINNED commit, then hand back typed values.

WHERE POLICY COMES FROM
Never the candidate worktree, never the candidate head, never a mutable branch name. It is read
from the trusted default branch revision resolved externally in `snapshot.load_snapshot`, through
the GitHub API at that exact OID.

WHAT VALIDATES IT
`scripts/check-merge-policy-schema.py` — the validator that landed with the schema (icn#2658). It
is not re-implemented here; a second copy of a rule is a second owner, and the two disagree the
moment either moves. The installer VENDORS that file next to this package so the installed
program never reaches back into a repository checkout to find it.

The validator resolves `admin_bypass.authoritative_source` against a repository root. The
installed program has no checkout, so this module materialises a minimal root from the SAME
pinned commit: the policy document, plus the ADR path the validator pins in its own code. That
makes "the authoritative source exists at the revision being evaluated" a property we actually
prove, instead of one we skip because it was inconvenient.
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
import pathlib
import tempfile
from dataclasses import dataclass

from .errors import EvidenceUnavailable, PolicyInvalid

POLICY_PATH = "ops/state/truth/policy.json"

# The closed set of merge strategies, owned HERE, in code. It is never read from the document
# under evaluation. A policy that says `admin` cannot widen it, because widening it requires
# editing this line. Nothing in this program ever forms a flag by prefixing a policy value.
GH_MERGE_STRATEGIES = frozenset({"merge", "squash", "rebase"})

# Symbolic live sources this program knows how to read. Also code, also closed.
LIVE_SOURCES = frozenset({"github_branch_protection"})


def _validator_path() -> pathlib.Path:
    """Where the schema validator lives, preferring the vendored installed copy.

    When a provenance record sits beside the package, this is an INSTALLED runtime and only the
    vendored copy is acceptable — the repository fallback is not merely unused there, it is
    refused, so no candidate checkout can supply the code that decides whether policy is sound.
    """
    here = pathlib.Path(__file__).resolve().parent
    vendored = here / "_policy_schema.py"
    if (here.parent / "provenance.json").is_file():
        if not vendored.is_file():
            raise EvidenceUnavailable(
                "installed runtime is missing its vendored policy validator; reinstall")
        return vendored
    if vendored.is_file():
        return vendored
    in_repo = here.parents[2] / "scripts" / "check-merge-policy-schema.py"
    if in_repo.is_file():
        return in_repo
    raise EvidenceUnavailable("policy schema validator not found")


def _load_validator():
    path = _validator_path()
    spec = importlib.util.spec_from_file_location("icn_merge_pr_policy_schema", path)
    if spec is None or spec.loader is None:
        raise EvidenceUnavailable(f"policy schema validator at {path} is not importable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@dataclass(frozen=True)
class MergePolicy:
    """The typed, validated subset this program acts on."""

    oid: str
    text_sha256: str
    default_strategy: str
    exception_strategy: str | None
    exception_applies_to: str | None
    required_checks: frozenset[str]
    live_source: str
    ready_mergeable: str
    ready_is_draft: bool
    ready_merge_states: frozenset[str]
    review_decision_allowlist: tuple[object, ...]
    max_unresolved_threads: int
    check_conclusion_allowlist: frozenset[str]
    require_queue_absent: bool
    require_not_in_queue: bool
    require_auto_merge_absent: bool
    require_strict_status_checks: bool | None
    require_approvals: int | None


def _typed(node, key, kind, label):
    """Fetch one field of an exact JSON type. `bool` is not an integer here, whatever Python says.

    `isinstance(False, int)` is True, so a JSON `false` satisfies an integer check and compares
    equal to 0. The landed validator rejects that; this reader refuses to re-open it.
    """
    value = node.get(key)
    exact = isinstance(value, kind) and (kind is bool or not isinstance(value, bool))
    if not exact:
        raise PolicyInvalid(f"{label}: wrong JSON type after validation")
    return value


def load_policy(client, owner: str, name: str, oid: str) -> MergePolicy:
    """Read policy at `oid`, validate it, and return typed values. Raises on anything less."""
    text = client.blob_text(owner, name, oid, POLICY_PATH)
    if text is None:
        raise EvidenceUnavailable(f"{POLICY_PATH} does not exist at {oid}")
    try:
        document = json.loads(text)
    except ValueError as exc:
        raise PolicyInvalid(f"{POLICY_PATH} at {oid} is not decodable JSON: {exc}") from exc

    validator = _load_validator()
    with tempfile.TemporaryDirectory(prefix="icn-merge-pr-policy-") as tmp:
        root = pathlib.Path(tmp)
        (root / POLICY_PATH).parent.mkdir(parents=True, exist_ok=True)
        (root / POLICY_PATH).write_text(text, encoding="utf-8")
        # The ADR path comes from the VALIDATOR's pinned constant, never from the document being
        # validated: letting untrusted data name the path we go and fetch would hand it back the
        # authority the pinning was there to remove.
        adr = getattr(validator, "ADMIN_BYPASS_ADR", None)
        if isinstance(adr, str) and adr:
            adr_text = client.blob_text(owner, name, oid, adr)
            if adr_text is not None:
                target = root / adr
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(adr_text, encoding="utf-8")
        previous_root = validator.ROOT
        try:
            validator.ROOT = root
            failures = validator.validate(document)
        finally:
            validator.ROOT = previous_root
    if failures:
        raise PolicyInvalid(f"{POLICY_PATH} at {oid} failed schema validation",
                            details=list(failures))

    merge = document["merge"]
    ready = merge["ready_when"]
    exception = merge.get("exception") or {}

    default_strategy = _typed(merge, "default_strategy", str, "merge.default_strategy")
    if default_strategy not in GH_MERGE_STRATEGIES:
        # Belt and braces: the validator owns the same closed set, and this program refuses to
        # depend on a single gate for the one value that could otherwise become a flag.
        raise PolicyInvalid(f"merge.default_strategy {default_strategy!r} is outside the closed set")
    exception_strategy = exception.get("strategy")
    if exception_strategy is not None and exception_strategy not in GH_MERGE_STRATEGIES:
        raise PolicyInvalid(f"merge.exception.strategy {exception_strategy!r} is outside the closed set")

    live_source = _typed(merge, "required_checks_live_source", str,
                         "merge.required_checks_live_source")
    if live_source not in LIVE_SOURCES:
        raise PolicyInvalid(f"merge.required_checks_live_source {live_source!r} is not a source "
                            "this program knows how to read")

    required = _typed(merge, "required_checks", list, "merge.required_checks")

    # `branch.strict_up_to_date` is the pinned document's declaration about the SAME live object
    # `required_checks_live_source` points at: `required_status_checks`, whose `strict` flag says
    # whether a branch must be current before it may merge. Reading it is not the circularity the
    # trust sequence forbids — that prohibition is on treating a policy field as the TRUST ROOT,
    # and `branch.primary` is still never consulted. By the time this runs, the base has already
    # been proved to be the externally resolved default branch, so the document is trusted.
    # It sits outside `merge`, which the landed schema validator does not cover, so its type is
    # established here: a malformed declaration about a protection control is not a soft failure.
    branch = document.get("branch") if isinstance(document.get("branch"), dict) else {}
    strict_declared = branch.get("strict_up_to_date")
    if strict_declared is not None and not isinstance(strict_declared, bool):
        raise PolicyInvalid("branch.strict_up_to_date is present but is not a boolean; a "
                            "malformed declaration about a branch-protection control is not a "
                            "claim this program will act on")
    approvals_declared = branch.get("required_approvals")
    if approvals_declared is not None and type(approvals_declared) is not int:
        raise PolicyInvalid("branch.required_approvals is present but is not an integer; a "
                            "malformed declaration about a branch-protection control is not a "
                            "claim this program will act on")

    return MergePolicy(
        oid=oid,
        text_sha256=hashlib.sha256(text.encode("utf-8")).hexdigest(),
        default_strategy=default_strategy,
        exception_strategy=exception_strategy,
        exception_applies_to=exception.get("applies_to"),
        required_checks=frozenset(required),
        live_source=live_source,
        ready_mergeable=_typed(ready, "mergeable", str, "ready_when.mergeable"),
        ready_is_draft=_typed(ready, "is_draft", bool, "ready_when.is_draft"),
        ready_merge_states=frozenset(_typed(ready, "merge_state_status_in", list,
                                            "ready_when.merge_state_status_in")),
        review_decision_allowlist=tuple(_typed(ready, "review_decision_allowlist", list,
                                               "ready_when.review_decision_allowlist")),
        max_unresolved_threads=_typed(ready, "unresolved_review_threads", int,
                                      "ready_when.unresolved_review_threads"),
        check_conclusion_allowlist=frozenset(_typed(
            ready, "required_check_conclusion_allowlist", list,
            "ready_when.required_check_conclusion_allowlist")),
        require_queue_absent=bool(ready["not_deferred"]["merge_queue_absent"]),
        require_not_in_queue=not bool(ready["not_deferred"]["is_in_merge_queue"]),
        require_auto_merge_absent=bool(ready["not_deferred"]["auto_merge_request_absent"]),
        require_strict_status_checks=strict_declared,
        require_approvals=approvals_declared,
    )
