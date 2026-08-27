"""Strict command line. Unknown is a refusal, never a default.

TWO FORMS, ONE OF WHICH MUTATES
    icn-merge-pr check <PR>
    icn-merge-pr merge <PR> --authorize

Evaluation is always available without mutating anything. Mutation requires `--authorize` to be
written out; there is no configuration, environment variable or implicit mode that supplies it.

WHAT IS REFUSED BY NAME
`--admin`, `--auto` and their privileged or deferred relatives. They are refused explicitly rather
than merely being unimplemented, so that an operator who reaches for the habit gets told the
primitive has no such mode instead of a generic parse error. Every other unknown option is refused
too — the point of a closed grammar is that nothing arrives by accident.
"""

from __future__ import annotations

import json
import sys

from . import codes, provenance
from .errors import ForbiddenOption, MergeToolError, UsageError
from .ghclient import GhCli
from .run import Result, run

USAGE = """icn-merge-pr — the trusted ordinary-merge primitive for ICN (icn#2651 stage B)

  icn-merge-pr check <PR>  [--repo OWNER/NAME]
  icn-merge-pr merge <PR>  --authorize [--repo OWNER/NAME]
                           [--strategy <policy exception> --reason "<why>"]
  icn-merge-pr provenance

`check` evaluates and mutates nothing. `merge` re-reads every piece of evidence immediately
before acting and performs at most one head-pinned ordinary merge. It has no privileged mode and
cannot arm, enqueue or defer a merge. Results are JSON on stdout; a one-line summary goes to
stderr. Exit 0 = READY or MERGED, 1 = refused, 2 = bad invocation."""

# Options that name a privileged, deferred or otherwise non-ordinary merge. Refused by name.
FORBIDDEN_OPTIONS = {
    "--admin": "there is no privileged merge path in this program; the human exception is owned "
               "by the ADR named in policy and is not automatable",
    "--admin-merge": "there is no privileged merge path in this program",
    "--auto": "an ordinary merge completes or refuses; it never arms a merge to happen later",
    "--disable-auto": "this program never disarms another actor's request",
    "--enqueue": "an ordinary merge never enqueues",
    "--queue": "an ordinary merge never enqueues",
    "--force": "there is no forced merge",
    "--squash": "the strategy is resolved from trusted policy; use --strategy for the one "
                "documented exception",
    "--merge": "the strategy is resolved from trusted policy; use --strategy for the one "
               "documented exception",
    "--rebase": "the strategy is resolved from trusted policy; use --strategy for the one "
                "documented exception",
}

COMMANDS = ("check", "merge", "provenance")

_VALUE_OPTIONS = {"--repo", "--strategy", "--reason"}
_FLAG_OPTIONS = {"--authorize"}


class Invocation:
    def __init__(self) -> None:
        self.command = ""
        self.number = 0
        self.owner = ""
        self.name = ""
        self.authorize = False
        self.strategy: str | None = None
        self.reason: str | None = None


def parse(argv: list[str]) -> Invocation:
    """Parse or refuse. Nothing is inferred, and no option is accepted more than once."""
    if not argv:
        raise UsageError(USAGE)

    inv = Invocation()
    inv.command = argv[0]
    if inv.command not in COMMANDS:
        raise UsageError(f"unknown command {inv.command!r}; expected one of {list(COMMANDS)}")

    rest = argv[1:]
    if inv.command == "provenance":
        if rest:
            raise UsageError("provenance takes no arguments")
        return inv

    if not rest or rest[0].startswith("-"):
        raise UsageError(f"{inv.command} requires a PR number")
    raw_number, rest = rest[0], rest[1:]
    if not raw_number.isdigit() or int(raw_number) <= 0:
        raise UsageError(f"PR number {raw_number!r} is not a positive integer")
    inv.number = int(raw_number)

    seen: set[str] = set()
    index = 0
    while index < len(rest):
        token = rest[index]
        # `--opt=value` is normalised first so a forbidden option cannot slip past by carrying one.
        option, _, inline = token.partition("=")
        if option in FORBIDDEN_OPTIONS:
            raise ForbiddenOption(
                f"{option} is refused: {FORBIDDEN_OPTIONS[option]}. This program has exactly two "
                "mutation outcomes, MERGED or REFUSED.")
        if not option.startswith("--"):
            raise UsageError(f"unexpected argument {token!r}")
        if option not in _VALUE_OPTIONS and option not in _FLAG_OPTIONS:
            raise UsageError(f"unknown option {option!r}")
        if option in seen:
            raise UsageError(f"{option} was given more than once")
        seen.add(option)
        if option in _FLAG_OPTIONS:
            if inline:
                raise UsageError(f"{option} takes no value")
            setattr(inv, option[2:], True)
            index += 1
            continue
        if inline:
            value = inline
            index += 1
        else:
            if index + 1 >= len(rest):
                raise UsageError(f"{option} requires a value")
            value = rest[index + 1]
            index += 2
        if option == "--repo":
            if value.count("/") != 1 or not all(value.split("/")):
                raise UsageError(f"--repo must be OWNER/NAME, got {value!r}")
            inv.owner, inv.name = value.split("/", 1)
        elif option == "--strategy":
            inv.strategy = value
        elif option == "--reason":
            inv.reason = value

    if inv.command == "check" and inv.authorize:
        raise UsageError("--authorize belongs to `merge`; `check` never mutates")
    if inv.command == "merge" and not inv.authorize:
        raise UsageError(
            "`merge` mutates and requires --authorize to be stated explicitly. Run "
            f"`icn-merge-pr check {inv.number}` first.")
    if inv.reason is not None and inv.strategy is None:
        raise UsageError("--reason is only meaningful together with --strategy")
    return inv


def _resolve_repository(inv: Invocation) -> tuple[str, str]:
    if inv.owner and inv.name:
        return inv.owner, inv.name
    recorded = provenance.default_repository()
    if recorded is not None:
        return recorded
    raise UsageError(
        "no repository: pass --repo OWNER/NAME. (An installed icn-merge-pr defaults to the "
        "repository recorded at install time; this one has no provenance record.)")


def _summary(result: Result) -> str:
    head = f"{result.outcome}  {result.owner}/{result.name}#{result.number}"
    if not result.reasons:
        return head
    return head + "".join(f"\n  - [{r.code}] {r.detail}" for r in result.reasons)


HELP_TOKENS = ("-h", "--help", "help")


def main(argv: list[str]) -> int:
    if argv and argv[0] in HELP_TOKENS:
        print(USAGE)
        return codes.EXIT_OK
    try:
        inv = parse(argv)
    except MergeToolError as exc:
        print(json.dumps({"tool": "icn-merge-pr", "outcome": exc.outcome,
                          "reasons": [{"code": exc.outcome, "detail": exc.detail}]}, indent=2))
        print(exc.detail, file=sys.stderr)
        return codes.exit_code(exc.outcome)

    if inv.command == "provenance":
        try:
            print(json.dumps(provenance.read(), indent=2, sort_keys=True))
        except MergeToolError as exc:
            print(json.dumps({"tool": "icn-merge-pr", "outcome": exc.outcome,
                              "reasons": [{"code": exc.outcome, "detail": exc.detail}]}, indent=2))
            print(exc.detail, file=sys.stderr)
            return codes.exit_code(exc.outcome)
        return codes.EXIT_OK

    try:
        owner, name = _resolve_repository(inv)
    except MergeToolError as exc:
        print(json.dumps({"tool": "icn-merge-pr", "outcome": exc.outcome,
                          "reasons": [{"code": exc.outcome, "detail": exc.detail}]}, indent=2))
        print(exc.detail, file=sys.stderr)
        return codes.exit_code(exc.outcome)

    result = run(GhCli(), owner, name, inv.number, authorize=inv.authorize,
                 requested_strategy=inv.strategy, exception_reason=inv.reason)
    print(json.dumps(result.as_dict(), indent=2, sort_keys=True))
    print(_summary(result), file=sys.stderr)
    return codes.exit_code(result.outcome)
