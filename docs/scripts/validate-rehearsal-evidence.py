#!/usr/bin/env python3
"""
Validate a rehearsal evidence export packet against the substrate-level
JSON Schema at docs/contracts/rehearsal-evidence-export.schema.json.

The schema is identified by a non-DNS URN ($id =
urn:icn:contract:rehearsal-evidence-export:v1). The repo-relative path
is a distribution hint, not authority. See
docs/contracts/rehearsal-evidence-export.md for the rationale.

Usage:
    python3 docs/scripts/validate-rehearsal-evidence.py
        # Validates the bundled example packet by default.

    python3 docs/scripts/validate-rehearsal-evidence.py path/to/packet.json
        # Validates an arbitrary packet.

    python3 docs/scripts/validate-rehearsal-evidence.py --schema custom.schema.json packet.json
        # Validates against an alternate schema (rarely needed; intended
        # for partner repositories that pin a different version URN).

Exit codes:
    0: packet validates
    1: packet fails validation (or input file is missing/invalid)

Requires: Python 3.11+ and the `jsonschema` library.
"""

import argparse
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_SCHEMA = REPO_ROOT / "docs" / "contracts" / "rehearsal-evidence-export.schema.json"
DEFAULT_PACKET = REPO_ROOT / "docs" / "contracts" / "rehearsal-evidence-export.example.json"


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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("packet", nargs="?", type=Path, default=DEFAULT_PACKET, help="Packet JSON file to validate (defaults to the bundled example).")
    parser.add_argument("--schema", type=Path, default=DEFAULT_SCHEMA, help="Schema JSON file to validate against (defaults to the bundled schema).")
    args = parser.parse_args()

    try:
        from jsonschema import Draft202012Validator
    except ImportError:
        print("ERROR: the `jsonschema` Python package is required (pip install jsonschema).", file=sys.stderr)
        return 1

    schema = load_json(args.schema, "schema")
    Draft202012Validator.check_schema(schema)
    validator = Draft202012Validator(schema)

    packet = load_json(args.packet, "packet")
    errors = sorted(validator.iter_errors(packet), key=lambda e: list(e.path))

    if not errors:
        contract = schema.get("$id", "<unknown>")
        print(f"OK: {args.packet.relative_to(REPO_ROOT) if args.packet.is_relative_to(REPO_ROOT) else args.packet} validates against {contract}")
        return 0

    print(f"FAIL: {args.packet} did not validate against {schema.get('$id', '<unknown>')}", file=sys.stderr)
    for err in errors:
        loc = "/".join(str(p) for p in err.absolute_path) or "<root>"
        print(f"  at {loc}: {err.message}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
