#!/usr/bin/env python3
"""
Validate the fixture-backed rehearsal-shell demo-mode bundle.

This is the validation harness for the fixture-mode bridge slice of
ICN #1727 ("fixture-backed demo mode for the rehearsal shell, no
network by default"). It does NOT implement the full backend
`--demo-mode` flag on icnd that #1727 ultimately tracks; that remains
OPEN. It proves a smaller, honest thing: that a committed, repo-safe,
deterministic read-model bundle exists for the future no-CLI
organizer/member shell (#1726) to load, and that loading/validating it
requires no network.

What it checks, given a manifest:

  1. The manifest declares demo mode and disabled network
     (`mode: "demo"`, `network: "disabled"`).
  2. The manifest carries `non_claims` covering the standing safety
     themes (no production, no network, no mutation, no private data).
  3. Every referenced read_model file exists and is valid JSON.
  4. Each read_model with `validate: "schema"` passes its named
     contract validator (this script shells out to the existing
     docs/scripts/validate-*.py validators rather than re-implementing
     JSON Schema, so the substrate contracts stay the single source of
     truth). Each `validate: "shape_only"` read_model has its declared
     `structural_keys` present at the top level.
  5. A simple, deterministic forbidden-reference guard runs over every
     referenced packet and the manifest itself: no live cloud-sync
     hostnames, no insecure URLs, no off-allowlist https hosts, no
     JWT-looking strings, no absolute private paths, and no
     payment/wallet/balance/currency framing outside `non_claims`.

The guard is intentionally small and deterministic — it is NOT a
general-purpose privacy scanner. The contract schemas
(`additionalProperties: false`, `x-icn-must-not-include`) are the real
privacy boundary; this guard is a cheap smoke check on top.

Usage:
    python3 docs/scripts/validate-rehearsal-shell-fixtures.py
        # Validates the bundled rehearsal-shell manifest by default.

    python3 docs/scripts/validate-rehearsal-shell-fixtures.py path/to/manifest.json
        # Validates an arbitrary manifest (used by negative tests and
        # partner repositories that pin their own bundle).

Exit codes:
    0: the whole bundle validates and the guard is clean
    1: a check failed, or an input file / schema / validator is invalid

Requires: Python 3.11+ and the `jsonschema` library (only because the
delegated contract validators require it).
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = (
    REPO_ROOT
    / "web"
    / "pilot-ui"
    / "fixtures"
    / "icn-organizer-demo"
    / "rehearsal-shell.manifest.json"
)

# Standing safety themes the manifest's non_claims must mention. Each entry
# is a list of acceptable substrings (any one satisfies the theme). Mirrors
# the machine-enforced non_claims discipline in the contract validators.
REQUIRED_NON_CLAIM_THEMES = {
    "no production deployment": ["production"],
    "no network by default": ["network"],
    "no mutation": ["mutation"],
    "no private data": ["private", "fictional"],
}

# Forbidden raw substrings (case-insensitive). Live cloud-sync hostnames and
# insecure URLs must never appear, even inside non_claims (the contract
# examples disclaim "Google Drive / Groups / Sheets" as words, never as
# hostnames, so matching hostnames here is false-positive-safe).
FORBIDDEN_SUBSTRINGS = [
    "drive.google.com",
    "docs.google.com",
    "groups.google.com",
    "sheets.google.com",
    "spreadsheets/d/",
    "http://",
]

# Absolute private path prefixes that must never appear in a repo-safe packet.
FORBIDDEN_PATH_PREFIXES = ["/home/", "/users/", "c:\\"]

# A JWT-looking string: base64url header "." base64url payload.
JWT_RE = re.compile(r"eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}")

# Money-transmission framing forbidden OUTSIDE non_claims. Cooperative
# coordination vocabulary (allocation / settlement / unit / position) is
# deliberately NOT here — it is regulatory-safe and expected in the rows.
MONEY_RE = re.compile(
    r"\bwallet\b|\busd\b|\bpaypal\b|\bstripe\b|\bcredit card\b|\$\s?\d",
    re.IGNORECASE,
)

HTTPS_HOST_RE = re.compile(r"https://([A-Za-z0-9._-]+)")


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


def strip_non_claims(node: object) -> object:
    """Return a copy of `node` with every `non_claims` array removed, so the
    money-framing check does not flag the disclaimers that are *supposed* to
    name the forbidden vocabulary."""
    if isinstance(node, dict):
        return {
            k: strip_non_claims(v) for k, v in node.items() if k != "non_claims"
        }
    if isinstance(node, list):
        return [strip_non_claims(v) for v in node]
    return node


def forbidden_reference_findings(
    label: str, data: object, raw_text: str, allowlist_hosts: list
) -> list:
    """Return a list of human-readable findings (empty == clean)."""
    findings = []
    lowered = raw_text.lower()

    for needle in FORBIDDEN_SUBSTRINGS:
        if needle in lowered:
            findings.append(f"{label}: forbidden reference {needle!r}")

    for prefix in FORBIDDEN_PATH_PREFIXES:
        if prefix in lowered:
            findings.append(f"{label}: forbidden absolute private path {prefix!r}")

    if JWT_RE.search(raw_text):
        findings.append(f"{label}: JWT-looking string present")

    for host in set(HTTPS_HOST_RE.findall(raw_text)):
        if host not in allowlist_hosts:
            findings.append(
                f"{label}: off-allowlist https host {host!r} "
                f"(allowed: {', '.join(allowlist_hosts)})"
            )

    money_text = json.dumps(strip_non_claims(data))
    match = MONEY_RE.search(money_text)
    if match:
        findings.append(
            f"{label}: payment/wallet/currency framing outside non_claims "
            f"({match.group(0)!r})"
        )

    return findings


def run_contract_validator(validator_rel: str, schema_rel: str, packet: Path) -> tuple:
    """Shell out to an existing docs/scripts/validate-*.py. Returns (ok, output)."""
    validator = REPO_ROOT / validator_rel
    if not validator.exists():
        return False, f"validator not found: {validator_rel}"
    cmd = [sys.executable, str(validator)]
    if schema_rel:
        cmd += ["--schema", str(REPO_ROOT / schema_rel)]
    cmd.append(str(packet))
    proc = subprocess.run(cmd, capture_output=True, text=True)
    output = (proc.stdout + proc.stderr).strip()
    return proc.returncode == 0, output


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "manifest",
        nargs="?",
        type=Path,
        default=DEFAULT_MANIFEST,
        help="Fixture manifest JSON (defaults to the bundled rehearsal-shell manifest).",
    )
    args = parser.parse_args()

    manifest_path = args.manifest
    manifest = load_json(manifest_path, "manifest")
    failures = []

    print(f"Validating rehearsal-shell fixture bundle: {manifest_path}")

    # 1. demo mode + network disabled
    if manifest.get("mode") != "demo":
        failures.append(f"manifest.mode is {manifest.get('mode')!r}, expected 'demo'")
    if manifest.get("network") != "disabled":
        failures.append(
            f"manifest.network is {manifest.get('network')!r}, expected 'disabled'"
        )
    else:
        print("  [ok] demo mode declared, network disabled")

    # 2. non_claims cover the standing safety themes
    non_claims = manifest.get("non_claims")
    if not isinstance(non_claims, list) or not non_claims:
        failures.append("manifest.non_claims missing or empty")
    else:
        joined = " ".join(non_claims).lower()
        for theme, needles in REQUIRED_NON_CLAIM_THEMES.items():
            if not any(n in joined for n in needles):
                failures.append(f"manifest.non_claims missing theme: {theme}")
        print(f"  [ok] non_claims present ({len(non_claims)} entries)")

    allowlist_hosts = manifest.get("url_allowlist_hosts", ["github.com"])

    # Guard the manifest itself.
    manifest_raw = manifest_path.read_text(encoding="utf-8")
    for finding in forbidden_reference_findings(
        "manifest", manifest, manifest_raw, allowlist_hosts
    ):
        failures.append(finding)

    # 3 + 4 + 5. each read_model
    read_models = manifest.get("read_models", [])
    if not read_models:
        failures.append("manifest.read_models is empty")

    for rm in read_models:
        name = rm.get("name", "<unnamed>")
        rel = rm.get("path", "")
        packet_path = REPO_ROOT / rel
        if not packet_path.exists():
            failures.append(f"{name}: referenced packet missing: {rel}")
            continue
        data = load_json(packet_path, f"{name} packet")
        raw_text = packet_path.read_text(encoding="utf-8")

        mode = rm.get("validate")
        if mode == "schema":
            ok, output = run_contract_validator(
                rm.get("validator", ""), rm.get("schema", ""), packet_path
            )
            if ok:
                print(f"  [ok] {name}: schema-valid ({rm.get('urn', rel)})")
            else:
                failures.append(f"{name}: schema validation failed -> {output}")
        elif mode == "shape_only":
            missing = [
                k for k in rm.get("structural_keys", []) if k not in data
            ]
            if missing:
                failures.append(f"{name}: missing structural keys {missing}")
            else:
                print(f"  [ok] {name}: shape-only keys present")
        else:
            failures.append(f"{name}: unknown validate mode {mode!r}")

        for finding in forbidden_reference_findings(
            name, data, raw_text, allowlist_hosts
        ):
            failures.append(finding)

    print()
    if failures:
        print(f"FAIL: {len(failures)} problem(s) found:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(f"PASS: rehearsal-shell fixture bundle is demo-only, no-network, and clean "
          f"({len(read_models)} read models).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
