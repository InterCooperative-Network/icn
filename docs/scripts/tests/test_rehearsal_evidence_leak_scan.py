#!/usr/bin/env python3
"""
Adversarial tests for the rehearsal evidence packet privacy leak scan
(docs/scripts/validate-rehearsal-evidence.py, issue #2422).

These are standalone assertions — no pytest dependency — so CI can run them
with a bare `python3 docs/scripts/tests/test_rehearsal_evidence_leak_scan.py`
alongside the other docs-freshness validators. Exit 0 = all pass, 1 = a
failure.

They exercise the distinction the fix rests on:
  * STRUCTURALLY VALID BUT PRIVACY-INVALID packets must be rejected by the
    privacy scan even though the JSON Schema accepts them — this is the whole
    point of #2422.
  * a valid, redacted packet must still pass.
  * legitimate public identifiers (contract URNs, opaque public row ids,
    public issue URLs) must NOT be flagged.
"""

import copy
import importlib.util
import json
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
VALIDATOR = REPO_ROOT / "docs" / "scripts" / "validate-rehearsal-evidence.py"
SCHEMA = REPO_ROOT / "docs" / "contracts" / "rehearsal-evidence-export.schema.json"
EXAMPLE = REPO_ROOT / "docs" / "contracts" / "rehearsal-evidence-export.example.json"

# Import the validator module (hyphenated filename → importlib).
_spec = importlib.util.spec_from_file_location("rev", VALIDATOR)
rev = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rev)

_failures = []


def check(name, cond):
    status = "ok  " if cond else "FAIL"
    print(f"[{status}] {name}")
    if not cond:
        _failures.append(name)


def leaks(packet):
    """(count, categories) from the importable scan."""
    f = rev.privacy_leak_findings(packet)
    return len(f), sorted({c for _, c in f})


def run_cli(packet_dict, extra=None):
    """Run the validator end-to-end on a temp packet; return exit code."""
    import tempfile
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
        json.dump(packet_dict, fh)
        p = fh.name
    cmd = [sys.executable, str(VALIDATOR), p, "--schema", str(SCHEMA)]
    if extra:
        cmd += extra
    rc = subprocess.run(cmd, capture_output=True, text=True)
    Path(p).unlink(missing_ok=True)
    return rc.returncode, (rc.stdout + rc.stderr)


def main():
    base = json.loads(EXAMPLE.read_text(encoding="utf-8"))

    # -- Baseline: the bundled redacted example is clean end-to-end. --------
    n, _ = leaks(base)
    check("valid redacted example: privacy scan finds nothing", n == 0)
    rc, _ = run_cli(base)
    check("valid redacted example: CLI exit 0", rc == 0)

    # -- Injected private DID (the reported #2422 case) --------------------
    p = copy.deepcopy(base)
    p["privacy_review"]["notes"] = "leaked did:icn:zTAMPERzzzzzzzzzzzzzzzzzzzzzzzzzz here"
    n, cats = leaks(p)
    check("injected DID in privacy_review.notes: flagged", n >= 1 and "did:icn identifier" in cats)
    rc, out = run_cli(p)
    check("injected DID: CLI exit 1 with [privacy] tag", rc == 1 and "[privacy]" in out)
    check("injected DID: value not echoed in diagnostics", "zTAMPER" not in out)

    # -- Injected JWT / bearer credential ----------------------------------
    p = copy.deepcopy(base)
    p["decision_outcomes"][0]["plain_summary"] = (
        "token eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhIn0.sig embedded"  # sanitize-ok: fabricated JWT-shape test fixture (decodes to {"alg":"HS256"}.{"sub":"a"})
    )
    n, cats = leaks(p)
    check("injected JWT in plain_summary: flagged", "JWT / bearer-shaped credential" in cats)

    # -- Injected private / loopback IP (topology) -------------------------
    # Use a generic RFC1918 address (10.0.0.0/8), NOT any real homelab address,
    # so the fixture exercises the scanner without recording live topology.
    p = copy.deepcopy(base)
    p["source_material"][0]["summary"] = "served from 10.0.0.5 internal host"
    n, cats = leaks(p)
    check("injected private IP: flagged", "private / loopback IP (topology)" in cats)
    # public / version-like dotted numbers must NOT trip the IP rule
    p2 = copy.deepcopy(base)
    p2["source_material"][0]["summary"] = "schema draft 2020.12.1.0 revision"
    _, cats2 = leaks(p2)
    check("version-like dotted number: NOT flagged as IP",
          "private / loopback IP (topology)" not in cats2)

    # -- Injected private filesystem path ----------------------------------
    p = copy.deepcopy(base)
    p["warnings"] = ["/home/ubuntu/icn-dev/private/secret.json referenced"]
    n, cats = leaks(p)
    check("injected private path in warnings: flagged", "private filesystem path" in cats)

    # -- PEM private key ---------------------------------------------------
    p = copy.deepcopy(base)
    p["rehearsal_label"] = "-----BEGIN EC PRIVATE KEY----- ..."  # sanitize-ok: bare PEM header string, no key material — leak-scan test fixture
    n, cats = leaks(p)
    check("injected PEM private key: flagged", "PEM private key" in cats)

    # -- Nested / array-contained leakage ----------------------------------
    p = copy.deepcopy(base)
    p["follow_ups"] = [{
        "title": "ok",
        "url": "https://github.com/InterCooperative-Network/icn/issues/2422",
        "notes": "assignee did:icn:zNESTEDzzzzzzzzzzzzzzzzzzzzzzzzz",
    }]
    n, cats = leaks(p)
    check("DID nested in follow_ups[].notes: flagged", "did:icn identifier" in cats)
    # the legitimate public github URL in the same object must NOT be flagged
    check("public github URL alongside: only one finding (the DID)", n == 1)

    # -- privacy_review flags themselves declare a leak --------------------
    p = copy.deepcopy(base)
    p["privacy_review"]["dids_exported"] = True
    _, cats = leaks(p)
    check("privacy_review.dids_exported=true: flagged",
          any("dids_exported" in c for c in cats))

    # -- Legitimate public identifiers are NOT rejected --------------------
    p = copy.deepcopy(base)
    p["proof_loop_references"][0]["public_id_or_category"] = "pending-row-action-item-001"
    p["source_material"][0]["summary"] = "contract urn:icn:contract:rehearsal-evidence-export:v1"
    n, _ = leaks(p)
    check("public row id + contract URN: NOT flagged", n == 0)

    # -- Layer separation: --skip-privacy-scan still passes structurally,
    #    but a DID-injected packet passes structurally and fails on privacy. -
    p = copy.deepcopy(base)
    p["privacy_review"]["notes"] = "did:icn:zLAYERzzzzzzzzzzzzzzzzzzzzzzzzzz"
    rc_struct, out_struct = run_cli(p, extra=["--skip-privacy-scan"])
    check("DID packet is STRUCTURALLY valid (schema can't see it)", rc_struct == 0)
    rc_priv, _ = run_cli(p)
    check("same DID packet FAILS with privacy scan on", rc_priv == 1)

    print()
    if _failures:
        print(f"FAILED: {len(_failures)} case(s): {_failures}", file=sys.stderr)
        return 1
    print("ALL PRIVACY-SCAN TESTS PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
