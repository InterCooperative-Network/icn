#!/usr/bin/env python3
"""Validate the ICN Agent Context Spine artifact.

Checks structural integrity and truth-discipline of
docs/reference/project-index/generated/agent-context-spine.json:

  - the artifact exists and is valid JSON
  - required top-level fields are present
  - every node has id / type / name / source_of_truth / non-empty evidence
  - node ids are unique
  - every edge has from / to / type / evidence.source
  - every edge endpoint references an existing node id
  - every node `path`, when present, exists on disk
  - every evidence `source` path exists on disk
  - no claim_surface node asserts readiness
  - no forbidden overclaim language appears in any generated field

Finally it runs `generate-agent-context-spine.py --check` to confirm the
committed artifact matches a fresh generation (no drift).

Exit 0 on success, 1 on validation failure. Python standard library only.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


def repo_root() -> Path:
    try:
        top = subprocess.check_output(
            ["git", "rev-parse", "--show-toplevel"],
            cwd=Path(__file__).resolve().parent,
            text=True,
        ).strip()
        return Path(top)
    except Exception:
        return Path(__file__).resolve().parents[1]


ROOT = repo_root()
ARTIFACT_REL = "docs/reference/project-index/generated/agent-context-spine.json"
ARTIFACT = ROOT / ARTIFACT_REL
GENERATOR_REL = "scripts/generate-agent-context-spine.py"

REQUIRED_TOP = [
    "schema", "status", "canonical", "generated", "source_commit",
    "generator", "regenerate", "check", "nodes", "edges",
]
REQUIRED_NODE = ["id", "type", "name", "source_of_truth", "evidence"]
REQUIRED_EDGE = ["from", "to", "type", "evidence"]

# Whole-phrase overclaims that must never appear in generated fields.
FORBIDDEN = [
    "production ready",
    "production-ready",
    "live federation",
    "formal pilot",
    "partner deployment",
    "pilot ready",
]


def iter_strings(value):
    """Yield every string contained in a JSON-ish value, recursively."""
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for v in value.values():
            yield from iter_strings(v)
    elif isinstance(value, list):
        for v in value:
            yield from iter_strings(v)


def main() -> int:
    errors: list[str] = []

    if not ARTIFACT.is_file():
        print(f"FAIL: {ARTIFACT_REL} does not exist - run "
              f"`python3 {GENERATOR_REL} --write`", file=sys.stderr)
        return 1

    try:
        data = json.loads(ARTIFACT.read_text(encoding="utf-8"))
    except Exception as e:  # noqa: BLE001
        print(f"FAIL: {ARTIFACT_REL} is not valid JSON: {e}", file=sys.stderr)
        return 1

    # top-level fields
    for f in REQUIRED_TOP:
        if f not in data:
            errors.append(f"top-level field missing: {f}")
    if data.get("canonical") is not False:
        errors.append("top-level 'canonical' must be false (orientation artifact, not a truth source)")
    if data.get("status") != "generated":
        errors.append("top-level 'status' must be 'generated'")

    nodes = data.get("nodes", [])
    edges = data.get("edges", [])

    # nodes
    node_ids: set[str] = set()
    for n in nodes:
        nid = n.get("id", "<no-id>")
        for f in REQUIRED_NODE:
            if f not in n or n[f] in (None, "", []):
                errors.append(f"node {nid}: missing/empty required field '{f}'")
        if nid in node_ids:
            errors.append(f"duplicate node id: {nid}")
        node_ids.add(nid)

        ev = n.get("evidence", [])
        if not isinstance(ev, list) or not ev:
            errors.append(f"node {nid}: evidence must be a non-empty list")
        else:
            for item in ev:
                src = item.get("source") if isinstance(item, dict) else None
                if not src:
                    errors.append(f"node {nid}: evidence item missing 'source'")
                elif not (ROOT / src).exists():
                    errors.append(f"node {nid}: evidence source does not exist: {src}")

        path = n.get("path")
        if path and not (ROOT / path).exists():
            errors.append(f"node {nid}: path does not exist: {path}")

    # claim_surface discipline
    for n in nodes:
        if n.get("type") == "claim_surface":
            nid = n.get("id", "<no-id>")
            if n.get("asserts_readiness") is not False:
                errors.append(f"claim_surface {nid}: must carry asserts_readiness=false")

    # path_guidance referential integrity (the code-quality brief layer)
    types_by_id = {n.get("id"): n.get("type") for n in nodes}
    # (id-prefix -> allowed node types it may reference)
    ref_specs = [
        ("risk_surfaces", {"claim_surface"}),
        ("invariants", {"invariant"}),
        ("docs", {"doc", "generated_artifact"}),
        ("recommended_skills", {"skill"}),
        ("recommended_agents", {"agent"}),
    ]
    for n in nodes:
        if n.get("type") != "path_guidance":
            continue
        nid = n.get("id", "<no-id>")
        match = n.get("match")
        if not match or not isinstance(match, str):
            errors.append(f"path_guidance {nid}: missing 'match' prefix")
        elif not (ROOT / match).exists():
            errors.append(f"path_guidance {nid}: match prefix does not exist: {match}")
        if not n.get("review_focus"):
            errors.append(f"path_guidance {nid}: 'review_focus' must be non-empty")
        for field, allowed in ref_specs:
            for ref in n.get(field, []):
                if ref not in node_ids:
                    errors.append(f"path_guidance {nid}: {field} references unknown node id: {ref}")
                elif types_by_id.get(ref) not in allowed:
                    errors.append(
                        f"path_guidance {nid}: {field} ref {ref} is type "
                        f"'{types_by_id.get(ref)}', expected one of {sorted(allowed)}"
                    )

    # edges
    for e in edges:
        label = f"{e.get('from','?')}->{e.get('to','?')} ({e.get('type','?')})"
        for f in REQUIRED_EDGE:
            if f not in e or e[f] in (None, "", []):
                errors.append(f"edge {label}: missing required field '{f}'")
        ev = e.get("evidence", {})
        src = ev.get("source") if isinstance(ev, dict) else None
        if not src:
            errors.append(f"edge {label}: evidence.source missing")
        elif not (ROOT / src).exists():
            errors.append(f"edge {label}: evidence source does not exist: {src}")
        if e.get("from") not in node_ids:
            errors.append(f"edge {label}: 'from' references unknown node id")
        if e.get("to") not in node_ids:
            errors.append(f"edge {label}: 'to' references unknown node id")

    # forbidden overclaim language across all generated string fields
    for text in iter_strings(data):
        low = text.lower()
        for phrase in FORBIDDEN:
            if phrase in low:
                errors.append(f"forbidden overclaim language present: '{phrase}'")

    if errors:
        print(f"FAIL: {len(errors)} validation error(s) in {ARTIFACT_REL}:", file=sys.stderr)
        for err in errors:
            print(f"  - {err}", file=sys.stderr)
        return 1

    # drift check via the generator
    rc = subprocess.call([sys.executable, str(ROOT / GENERATOR_REL), "--check"])
    if rc != 0:
        print("FAIL: generator --check reported drift (artifact differs from a fresh "
              "generation). Run `python3 scripts/generate-agent-context-spine.py --write` "
              "and commit.", file=sys.stderr)
        return 1

    print(f"OK: agent context spine valid ({len(nodes)} nodes, {len(edges)} edges, "
          f"{len(node_ids)} unique ids) and not stale.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
