#!/usr/bin/env python3
"""Rehearsal Node — pending-publish evidence-export generator.

Maps the committed pending-publish review-preview rows (the shape served by
GET /v1/gov/me/pending-publish-summary in a non-production build mode,
origin=committed_fixture) into a repo-safe evidence packet conforming to
urn:icn:contract:rehearsal-evidence-export:v1, then validates it.

Honest operator/steward layer — NOT the browser. The packet:
  * carries rehearsal_mode="fixture-only" (or "local-dry-run" only if a real
    loopback GET was performed by the caller; this script never touches the
    network);
  * NEVER copies the caller `did`, any credential, or any private path;
  * maps every review row to a `deferred` decision_outcome — a preview row is
    evidence for review, never an authorization; `approved_for_publish` is a
    review disposition, not an authority;
  * marks every expected receipt as proof_loop_references[].status="not-attempted"
    (an expectation, never issued);
  * declares mutation_boundary.executed=false / target="none".

Deterministic apart from `recorded_at`.

Usage:
    # write a packet to the gitignored output dir (recorded_at = now, UTC):
    python3 scripts/rehearsal_pending_publish_evidence.py --write
    # verify the committed canonical packet matches a fresh generation
    # (byte-identical after masking recorded_at) and both validate:
    python3 scripts/rehearsal_pending_publish_evidence.py --check
Exit 0 on success; non-zero (fail closed) on any validation/consistency failure.
"""
from __future__ import annotations

import argparse
import datetime
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ROWS_FIXTURE = ROOT / "web/member-shell/fixtures/pending-publish-summary.json"
COMMITTED_PACKET = ROOT / "web/member-shell/fixtures/pending-publish-evidence-export.json"
VALIDATOR = ROOT / "docs/scripts/validate-rehearsal-evidence.py"
OUTDIR = ROOT / "demo/output/pending-publish-evidence"

# Review status -> repo-safe decision_outcome category. A preview row is NEVER
# an authorization: approved_for_publish is a review disposition -> deferred.
STATUS_TO_OUTCOME = {
    "pending_review": "deferred",
    "needs_more_info": "deferred",
    "approved_for_publish": "deferred",
    "needs_edit": "edit-and-resubmit",
    "rejected": "rejected",
}
# receipt category -> proof_loop_references kind. settlement_receipt / none have
# no receipt kind in the contract -> represented via provenance only (skipped here).
RECEIPT_TO_PLR_KIND = {
    "action_item_completion_receipt": "action-item-completion-receipt",
    "governance_receipt": "governance-decision-receipt",
    "attendance_receipt": "meeting-attendance-receipt",
}
FIXED_RECORDED_AT = "2026-07-10T00:00:00Z"  # committed-packet anchor (documented)


def build_packet(recorded_at: str) -> dict:
    rows = json.loads(ROWS_FIXTURE.read_text()).get("rows", [])
    decision_outcomes = []
    receipt_kinds: list[str] = []
    for row in rows:
        cat = STATUS_TO_OUTCOME.get(row.get("status"), "deferred")
        decision_outcomes.append(
            {
                "category": cat,
                "plain_summary": row.get("plain_summary", "")
                + " Preview only — no decision was recorded.",
            }
        )
        rc = (row.get("receipt_expected") or {})
        if rc.get("expected"):
            k = RECEIPT_TO_PLR_KIND.get(rc.get("category"))
            if k and k not in receipt_kinds:
                receipt_kinds.append(k)
    proof_loop_references = [
        {
            "kind": "provenance-summary",
            "public_id_or_category": "pending-publish-committed-fixture",
            "status": "closed",
        }
    ] + [{"kind": k, "status": "not-attempted"} for k in receipt_kinds]
    return {
        "schema_version": "0.1.0",
        "recorded_at": recorded_at,
        "rehearsal_label": "Pending-publish review preview — committed-fixture rehearsal evidence",
        "rehearsal_mode": "fixture-only",
        "audience_categories": ["organizer", "steward", "reviewer"],
        "source_material": [
            {
                "kind": "committed-fixture",
                "basename": "pending-publish-summary.json",
                "summary": "Committed pending-publish review-preview rows, as served by GET /v1/gov/me/pending-publish-summary in a non-production build mode (origin = committed_fixture). Fictional rehearsal data; no real people or organization.",
            }
        ],
        "workflow_steps_completed": [
            "start",
            "select-material",
            "preview",
            "surface-result",
            "export-evidence",
        ],
        "decision_outcomes": decision_outcomes,
        "preview_review_boundary": {
            "enforced": True,
            "notes": "Preview rows are evidence presented for review — not authorization, publication, custody writes, or completed actions.",
        },
        "mutation_boundary": {
            "executed": False,
            "target": "none",
            "notes": "Read-only projection (GET only). No write, no publish, no action-card creation, no custody write; no network egress in fixture mode.",
        },
        "proof_loop_references": proof_loop_references,
        "privacy_review": {
            "reviewer_role": "reviewer",
            "status": "reviewed-clean",
            "notes": "Rows carry only generic plain-language labels — no DIDs, no private paths, no partner data. The runtime response's caller DID is deliberately not carried into this packet.",
        },
        "export_safety_classification": "repo-safe",
        "non_claims": [
            "Not production readiness and not a formal pilot.",
            "No live federation, no cloud sync.",
            "No private participant data is handled; the rows are fictional committed-fixture rehearsal data, never live participant state.",
            "An expected receipt is an evidence expectation for the reviewer, not authority. No preview row was authorized, published, or turned into a custody write; no receipt was issued.",
        ],
    }


def validate(packet_path: Path) -> None:
    r = subprocess.run(
        [sys.executable, str(VALIDATOR), str(packet_path)],
        capture_output=True,
        text=True,
    )
    if r.returncode != 0:
        sys.stderr.write(r.stdout + r.stderr)
        raise SystemExit(f"FAIL: evidence packet failed :v1 schema validation: {packet_path}")


def dump(packet: dict) -> str:
    return json.dumps(packet, indent=2, ensure_ascii=False) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--write", action="store_true", help="generate + validate a packet in the gitignored output dir")
    g.add_argument("--check", action="store_true", help="verify the committed canonical packet matches a fresh generation (mask recorded_at) and both validate")
    args = ap.parse_args()

    if args.write:
        now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
        OUTDIR.mkdir(parents=True, exist_ok=True)
        out = OUTDIR / "rehearsal-evidence-export.json"
        out.write_text(dump(build_packet(now)))
        validate(out)
        print(f"OK: wrote + validated {out} (recorded_at={now})")
        return 0

    # --check: the committed packet must equal a fresh generation apart from recorded_at,
    # and both must validate. This is the determinism + drift guard.
    committed = json.loads(COMMITTED_PACKET.read_text())
    generated = build_packet(committed["recorded_at"])
    validate(COMMITTED_PACKET)
    if generated != committed:
        # show the differing top-level keys only (never dump private data — there is none)
        diff = sorted(k for k in set(generated) | set(committed) if generated.get(k) != committed.get(k))
        raise SystemExit(f"FAIL: committed packet drifted from generator output; differing keys: {diff}")
    fresh = build_packet("1970-01-01T00:00:00Z")
    committed_masked = {**committed, "recorded_at": "1970-01-01T00:00:00Z"}
    if fresh != committed_masked:
        raise SystemExit("FAIL: generator is not deterministic apart from recorded_at")
    print("OK: committed packet matches generator (deterministic apart from recorded_at) and validates against :v1")
    return 0


if __name__ == "__main__":
    sys.exit(main())
