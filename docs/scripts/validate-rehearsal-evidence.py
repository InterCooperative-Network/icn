#!/usr/bin/env python3
"""
Validate a rehearsal evidence export packet against the substrate-level
JSON Schema at docs/contracts/rehearsal-evidence-export.schema.json, and
then scan its string values for identifiers the value-withheld contract
forbids (DIDs, credentials, private paths, private-network addresses).

The schema is identified by a non-DNS URN ($id =
urn:icn:contract:rehearsal-evidence-export:v1). The repo-relative path
is a distribution hint, not authority. See
docs/contracts/rehearsal-evidence-export.md for the rationale.

TWO DISTINCT VALIDATION LAYERS (this tool covers a and b; NOT c or d):
  (a) STRUCTURAL  — JSON Schema: shape, required fields, enums,
      additionalProperties:false. Structure alone cannot catch a leaked
      identifier hidden inside an otherwise-valid free-text field, which is
      exactly why layer (b) exists (issue #2422).
  (b) PRIVACY / REDACTION — a deterministic value scan for the identifier
      classes the schema's `x-icn-must-not-include` forbids but that
      structure cannot see: `did:icn:` DIDs, JWT/bearer-shaped credentials,
      PEM private keys, private filesystem paths, and private-network /
      loopback IPs (topology). This asserts repo-safety of the packet's
      *content*; it does not prove completeness of redaction.
  (c) CRYPTOGRAPHIC verification (digest / signature / `packet_hash`) is a
      SEPARATE concern and is NOT performed here.
  (d) INSTITUTIONAL TRUTH (approval, pilot status, authority) is NOT
      asserted by validation — a valid, repo-safe packet still grants zero
      authority.

A passing result means: structurally valid AND free of the scanned
identifier classes. It does NOT mean cryptographically verified, redaction-
complete, approved, or authoritative.

Usage:
    python3 docs/scripts/validate-rehearsal-evidence.py
        # Validates the bundled example packet by default.

    python3 docs/scripts/validate-rehearsal-evidence.py path/to/packet.json
        # Validates an arbitrary packet.

    python3 docs/scripts/validate-rehearsal-evidence.py --schema custom.schema.json packet.json
        # Validates against an alternate schema (rarely needed; intended
        # for partner repositories that pin a different version URN).

    python3 docs/scripts/validate-rehearsal-evidence.py --skip-privacy-scan packet.json
        # Structural (schema) validation only. Use only to isolate a schema
        # failure; the privacy scan is on by default and CI must keep it on.

Exit codes:
    0: packet validates (structural + privacy)
    1: packet fails validation, or input file is missing/invalid

Requires: Python 3.11+ and the `jsonschema` library.
"""

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_SCHEMA = REPO_ROOT / "docs" / "contracts" / "rehearsal-evidence-export.schema.json"
DEFAULT_PACKET = REPO_ROOT / "docs" / "contracts" / "rehearsal-evidence-export.example.json"

# ---------------------------------------------------------------------------
# Privacy / redaction scan (layer b).
#
# These patterns target ONLY identifier classes that are never legitimate in a
# value-withheld evidence packet (schema `x-icn-must-not-include`). They do NOT
# match — and must not reject — public identifiers that ARE legitimate:
#   * urn:icn:contract:... contract URNs (the packet's own identity),
#   * opaque public row ids like "pending-row-action-item-001",
#   * public issue/PR URLs, closed-taxonomy enum values.
# The `did:icn:` pattern is the load-bearing one for #2422; it mirrors the
# in-VM icn-demo-verify.sh leak scan so both layers agree.
# ---------------------------------------------------------------------------
_DID_RE = re.compile(r"did:icn:[A-Za-z0-9]+")
# JWT / bearer-shaped: base64url header.payload (two dot-separated segments).
_JWT_RE = re.compile(r"eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}")
_PEM_RE = re.compile(r"-----BEGIN (?:[A-Z0-9 ]+ )?PRIVATE KEY-----")
# Private filesystem paths (POSIX home/root, Windows user profile).
_PRIV_PATH_RE = re.compile(r"(?:/home/|/root/|/Users/|[A-Za-z]:\\Users\\)", re.IGNORECASE)
# Dotted-quad candidate; range-checked in _is_private_ip so we only flag
# private / loopback / link-local addresses (topology), never version strings.
_IPV4_RE = re.compile(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b")


def _is_private_ip(a: int, b: int, c: int, d: int) -> bool:
    if not all(0 <= x <= 255 for x in (a, b, c, d)):
        return False  # not a valid dotted-quad — treat as ordinary text
    if a == 10:
        return True
    if a == 127:
        return True
    if a == 192 and b == 168:
        return True
    if a == 172 and 16 <= b <= 31:
        return True
    if a == 169 and b == 254:  # link-local
        return True
    return False


def _string_findings(value: str, path: str) -> list:
    """Return (path, category) findings for a single string. Diagnostics never
    echo the offending value — only its JSON path and category — so the tool's
    own output can never become a leak vector."""
    out = []
    if _DID_RE.search(value):
        out.append((path, "did:icn identifier"))
    if _JWT_RE.search(value):
        out.append((path, "JWT / bearer-shaped credential"))
    if _PEM_RE.search(value):
        out.append((path, "PEM private key"))
    if _PRIV_PATH_RE.search(value):
        out.append((path, "private filesystem path"))
    for m in _IPV4_RE.finditer(value):
        if _is_private_ip(*(int(g) for g in m.groups())):
            out.append((path, "private / loopback IP (topology)"))
            break
    return out


def privacy_leak_findings(packet) -> list:
    """Walk every string value in the packet and return a list of
    (json_path, category) findings for forbidden identifier classes. Empty
    list == privacy-clean. Also honors the packet's own privacy_review flags:
    a truthy dids_exported / credentials_exported is itself a declared leak."""
    findings = []

    def walk(node, path):
        if isinstance(node, dict):
            for k, v in node.items():
                walk(v, f"{path}.{k}" if path else str(k))
        elif isinstance(node, list):
            for i, v in enumerate(node):
                walk(v, f"{path}[{i}]")
        elif isinstance(node, str):
            findings.extend(_string_findings(node, path or "<root>"))

    walk(packet, "")

    if isinstance(packet, dict):
        pr = packet.get("privacy_review")
        if isinstance(pr, dict):
            if pr.get("dids_exported"):
                findings.append(("privacy_review.dids_exported", "packet declares dids_exported=true"))
            if pr.get("credentials_exported"):
                findings.append(("privacy_review.credentials_exported", "packet declares credentials_exported=true"))

    return findings


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
    parser.add_argument("--skip-privacy-scan", action="store_true", help="Run structural (schema) validation only. CI must NOT set this.")
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

    rel = args.packet.relative_to(REPO_ROOT) if args.packet.is_relative_to(REPO_ROOT) else args.packet
    contract = schema.get("$id", "<unknown>")

    # Layer (a): structural / schema.
    errors = sorted(validator.iter_errors(packet), key=lambda e: list(e.path))
    if errors:
        print(f"FAIL [structural]: {rel} did not validate against {contract}", file=sys.stderr)
        for err in errors:
            loc = "/".join(str(p) for p in err.absolute_path) or "<root>"
            print(f"  at {loc}: {err.message}", file=sys.stderr)
        return 1

    # Layer (b): privacy / redaction value scan (unless explicitly skipped).
    if not args.skip_privacy_scan:
        leaks = privacy_leak_findings(packet)
        if leaks:
            print(f"FAIL [privacy]: {rel} is structurally valid but leaks forbidden identifiers "
                  f"(value-withheld contract {contract}):", file=sys.stderr)
            for loc, category in leaks:
                print(f"  at {loc}: {category}", file=sys.stderr)
            print("  (diagnostics name the field and class only; the offending value is never echoed)", file=sys.stderr)
            return 1
        print(f"OK: {rel} validates against {contract} — structural + privacy scan pass "
              f"(NOT cryptographic verification, redaction-completeness, approval, or authority).")
    else:
        print(f"OK [structural only]: {rel} validates against {contract} "
              f"(privacy scan SKIPPED — do not use for a repo-safety decision).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
