#!/usr/bin/env python3
"""
Validate an ActionCard object against the institution-package
JSON Schema at docs/contracts/institution-package/action-card.schema.json.

The schema describes one element of the `cards` array on
`GET /v1/gov/me/action-cards`. The HTTP wrapper
(`{ did, cards, generated_at }`) is OpenAPI-documented in the
gateway and is not described by this hand-maintained schema; this
validator works against single card objects, which is what
institution packages produce when planning or rendering templates.

The schema's `$id` is currently DNS-backed (HTTPS). Per
docs/contracts/schema-id-audit.md, that identifier is retained
temporarily and reviewed by 2026-06-30; a future single-schema PR
may migrate it to a non-DNS URN under the §5 migration rules.
This validator works regardless of the `$id` form, since it loads
the schema by file path.

Usage:
    python3 docs/scripts/validate-action-card.py
        # Validates the bundled example card by default.

    python3 docs/scripts/validate-action-card.py path/to/card.json
        # Validates an arbitrary single-card object.

    python3 docs/scripts/validate-action-card.py --schema custom.schema.json card.json
        # Validates against an alternate schema (rarely needed; intended
        # for partner repositories that pin a different version).

Exit codes:
    0: card validates
    1: card fails validation, or input file / schema is invalid

Requires: Python 3.11+ and the `jsonschema` library.
"""

import argparse
import json
import sys
from datetime import datetime
from pathlib import Path
from urllib.parse import urlparse

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_SCHEMA = (
    REPO_ROOT
    / "docs"
    / "contracts"
    / "institution-package"
    / "action-card.schema.json"
)
DEFAULT_CARD = (
    REPO_ROOT
    / "docs"
    / "contracts"
    / "institution-package"
    / "action-card.example.json"
)


def load_json(path: Path, label: str) -> dict:
    if not path.exists():
        print(f"ERROR: {label} not found at {path}", file=sys.stderr)
        sys.exit(1)
    try:
        with path.open("r", encoding="utf-8") as fh:
            return json.load(fh)
    except json.JSONDecodeError as exc:
        print(f"ERROR: {label} at {path} is not valid JSON: {exc}", file=sys.stderr)
        sys.exit(1)


def build_format_checker(FormatChecker):
    """
    Return a FormatChecker that enforces RFC 3339 `date-time` and basic `uri`
    using only stdlib. The action-card schema does not currently use either
    `format` annotation; the registrations are kept symmetric with the other
    contract validators (`validate-preview-review.py`,
    `validate-rehearsal-evidence.py`) so a future field that adds either
    format is enforced without touching this file.
    """
    checker = FormatChecker()

    @checker.checks("date-time", raises=ValueError)
    def _check_datetime(value: object) -> bool:
        if not isinstance(value, str):
            return False
        normalised = value.replace("Z", "+00:00") if value.endswith("Z") else value
        datetime.fromisoformat(normalised)
        return True

    @checker.checks("uri", raises=ValueError)
    def _check_uri(value: object) -> bool:
        if not isinstance(value, str):
            return False
        parsed = urlparse(value)
        if not parsed.scheme:
            raise ValueError("uri missing scheme")
        if not (parsed.netloc or parsed.path):
            raise ValueError("uri missing netloc or path")
        return True

    return checker


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "card",
        nargs="?",
        type=Path,
        default=DEFAULT_CARD,
        help="Card JSON file to validate (defaults to the bundled example).",
    )
    parser.add_argument(
        "--schema",
        type=Path,
        default=DEFAULT_SCHEMA,
        help="Schema JSON file to validate against (defaults to the bundled schema).",
    )
    args = parser.parse_args()

    try:
        from jsonschema import Draft202012Validator, FormatChecker, SchemaError
    except ImportError:
        print(
            "ERROR: the `jsonschema` Python package is required (pip install jsonschema).",
            file=sys.stderr,
        )
        return 1

    schema = load_json(args.schema, "schema")

    try:
        Draft202012Validator.check_schema(schema)
    except SchemaError as exc:
        print(f"ERROR: schema at {args.schema} is invalid: {exc.message}", file=sys.stderr)
        return 1

    format_checker = build_format_checker(FormatChecker)
    validator = Draft202012Validator(schema, format_checker=format_checker)

    card = load_json(args.card, "card")

    try:
        errors = sorted(validator.iter_errors(card), key=lambda e: list(e.path))
    except Exception as exc:  # safety net; iter_errors normally returns, not raises
        print(
            f"ERROR: validator raised unexpectedly while validating {args.card}: {exc}",
            file=sys.stderr,
        )
        return 1

    if not errors:
        contract = schema.get("$id", "<unknown>")
        rel = (
            args.card.relative_to(REPO_ROOT)
            if args.card.is_relative_to(REPO_ROOT)
            else args.card
        )
        print(f"OK: {rel} validates against {contract}")
        return 0

    print(
        f"FAIL: {args.card} did not validate against {schema.get('$id', '<unknown>')}",
        file=sys.stderr,
    )
    for err in errors:
        loc = "/".join(str(p) for p in err.absolute_path) or "<root>"
        print(f"  at {loc}: {err.message}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
