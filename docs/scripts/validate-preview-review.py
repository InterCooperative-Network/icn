#!/usr/bin/env python3
"""
Validate a preview/review packet against the substrate-level
JSON Schema at docs/contracts/preview-review.schema.json.

The schema is identified by a non-DNS URN ($id =
urn:icn:contract:preview-review:v1). The repo-relative path
is a distribution hint, not authority. See
docs/contracts/preview-review.md for the rationale.

Format enforcement:
The validator runs with an explicit Draft 2020-12 format
checker so `format: date-time` and `format: uri` annotations
in the schema are actually enforced, not just advisory. The
optional jsonschema format-extras packages (`rfc3339-validator`,
`rfc3987`) are not installed system-wide; this script registers
small stdlib-only checkers for the two formats this contract
uses, so no new repo dependency is added.

Usage:
    python3 docs/scripts/validate-preview-review.py
        # Validates the bundled example packet by default.

    python3 docs/scripts/validate-preview-review.py path/to/packet.json
        # Validates an arbitrary packet.

    python3 docs/scripts/validate-preview-review.py --schema custom.schema.json packet.json
        # Validates against an alternate schema (rarely needed; intended
        # for partner repositories that pin a different version URN).

Exit codes:
    0: packet validates
    1: packet fails validation, or input file / schema is invalid

Requires: Python 3.11+ and the `jsonschema` library.
"""

import argparse
import json
import sys
from datetime import datetime
from pathlib import Path
from urllib.parse import urlparse

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_SCHEMA = REPO_ROOT / "docs" / "contracts" / "preview-review.schema.json"
DEFAULT_PACKET = REPO_ROOT / "docs" / "contracts" / "preview-review.example.json"


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
    using only stdlib. The default Draft 2020-12 FORMAT_CHECKER does not
    register either of those formats unless the optional jsonschema
    format-extras packages are installed; this keeps the script dependency-free
    while still actually enforcing the formats this contract uses.
    """
    checker = FormatChecker()

    @checker.checks("date-time", raises=ValueError)
    def _check_datetime(value: object) -> bool:
        if not isinstance(value, str):
            return False
        # Normalise an `Z` suffix to `+00:00` for fromisoformat (Python 3.11+
        # handles `Z` natively; earlier versions don't, so do it explicitly).
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
        # Accept either an authority-based URI (https://...) or a scheme + path
        # (urn:..., mailto:...). Reject empty values that parse to nothing.
        if not (parsed.netloc or parsed.path):
            raise ValueError("uri missing netloc or path")
        return True

    return checker


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "packet",
        nargs="?",
        type=Path,
        default=DEFAULT_PACKET,
        help="Packet JSON file to validate (defaults to the bundled example).",
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

    packet = load_json(args.packet, "packet")

    try:
        errors = sorted(validator.iter_errors(packet), key=lambda e: list(e.path))
    except Exception as exc:  # safety net; iter_errors normally returns, not raises
        print(
            f"ERROR: validator raised unexpectedly while validating {args.packet}: {exc}",
            file=sys.stderr,
        )
        return 1

    if not errors:
        contract = schema.get("$id", "<unknown>")
        rel = (
            args.packet.relative_to(REPO_ROOT)
            if args.packet.is_relative_to(REPO_ROOT)
            else args.packet
        )
        print(f"OK: {rel} validates against {contract}")
        return 0

    print(
        f"FAIL: {args.packet} did not validate against {schema.get('$id', '<unknown>')}",
        file=sys.stderr,
    )
    for err in errors:
        loc = "/".join(str(p) for p in err.absolute_path) or "<root>"
        print(f"  at {loc}: {err.message}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
