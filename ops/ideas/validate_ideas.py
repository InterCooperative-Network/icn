#!/usr/bin/env python3
"""Validate ops/ideas/ideas.yaml against the refinery schema.

Stdlib-only. Hand-rolled minimal YAML reader (line-oriented,
2-space indent, ":" separators, "- " list items) tuned to the
small, stable shape used by ideas.yaml. If ideas.yaml grows beyond
this shape, replace this reader with a proper YAML library.

Usage:
    python3 ops/ideas/validate_ideas.py [--path PATH]

Exit code 0 on pass, 1 on validation failure, 2 on parse failure.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

ALLOWED_STATUS = {
    "raw",
    "captured",
    "framed",
    "classified",
    "decomposed",
    "needs_source_review",
    "needs_dogfood",
    "promotion_review",
    "promoted_rfc_candidate",
    "promoted_adr_candidate",
    "promoted_issue",
    "promoted_package_task",
    "promoted_learning_task",
    "promoted_website_claim",
    "parked",
    "rejected",
    "superseded",
}

ALLOWED_DESTINATIONS = {
    "icn",
    "nycn",
    "icn-learn",
    "website",
    "private_overlay",
    "google_drive",
    "external",
    "unknown",
}

ALLOWED_KINDS = {
    "product_frame",
    "institutional_need",
    "architecture_concept",
    "runtime_gap",
    "package_pattern",
    "learning_material",
    "public_claim",
    "source_review",
    "dogfood_slice",
    "process_rule",
    "privacy_boundary",
    "evidence_gap",
    "repo_hygiene",
    "future_research",
}

PROMOTED_STATUSES = {s for s in ALLOWED_STATUS if s.startswith("promoted_")}

REQUIRED_FIELDS = (
    "id",
    "title",
    "status",
    "kind",
    "source",
    "one_sentence",
    "problem",
    "belongs_to",
    "layer",
    "current_artifact",
    "proposed_next_artifact",
    "promoted_target",
    "proposed_objects",
    "requires_rfc",
    "likely_adr",
    "implementation_ready",
    "public_claim_ready",
    "evidence_required",
    "privacy_risk",
    "boundary_risk",
    "risks",
    "next_transform",
)


# --- minimal YAML reader -----------------------------------------------------


def _strip_comment(line: str) -> str:
    """Strip a trailing '# comment' that is not inside quotes."""
    in_single = False
    in_double = False
    for i, ch in enumerate(line):
        if ch == "'" and not in_double:
            in_single = not in_single
        elif ch == '"' and not in_single:
            in_double = not in_double
        elif ch == "#" and not in_single and not in_double:
            return line[:i].rstrip()
    return line.rstrip()


def _coerce_scalar(raw: str) -> Any:
    s = raw.strip()
    if s == "" or s == "null" or s == "~":
        return None
    if s in ("true", "True"):
        return True
    if s in ("false", "False"):
        return False
    if s.startswith("[") and s.endswith("]"):
        body = s[1:-1].strip()
        if body == "":
            return []
        # naive split: ideas.yaml inline lists are short and never
        # contain commas inside values
        return [_coerce_scalar(p) for p in _split_top_level(body, ",")]
    if (s.startswith('"') and s.endswith('"')) or (s.startswith("'") and s.endswith("'")):
        return s[1:-1]
    if s.startswith("|") or s.startswith(">"):
        return ""  # block scalars handled separately
    try:
        return int(s)
    except ValueError:
        pass
    try:
        return float(s)
    except ValueError:
        pass
    return s


def _split_top_level(body: str, sep: str) -> list[str]:
    out: list[str] = []
    cur = ""
    in_single = False
    in_double = False
    depth = 0
    for ch in body:
        if ch == "'" and not in_double:
            in_single = not in_single
        elif ch == '"' and not in_single:
            in_double = not in_double
        elif ch in "[{(" and not in_single and not in_double:
            depth += 1
        elif ch in "]})" and not in_single and not in_double:
            depth -= 1
        if ch == sep and depth == 0 and not in_single and not in_double:
            out.append(cur)
            cur = ""
        else:
            cur += ch
    if cur != "":
        out.append(cur)
    return out


def _indent_of(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def parse_ideas_yaml(text: str) -> dict[str, Any]:
    """Parse the small subset of YAML used by ideas.yaml.

    Supports:
      - top-level scalar `key: value`
      - top-level list `ideas:` followed by `- key: value` blocks
      - per-item nested scalars, block scalars (`|` and `>`), and
        inline `[]` lists
    """
    lines = text.splitlines()
    # strip pure-comment lines and trailing comments
    clean = []
    for ln in lines:
        if ln.strip().startswith("#"):
            continue
        clean.append(_strip_comment(ln))
    # drop trailing blank lines
    while clean and clean[-1].strip() == "":
        clean.pop()

    out: dict[str, Any] = {}
    i = 0
    n = len(clean)

    while i < n:
        ln = clean[i]
        if ln.strip() == "":
            i += 1
            continue
        if _indent_of(ln) != 0:
            raise ValueError(f"line {i + 1}: expected top-level key, got indented line")
        key, _, rest = ln.partition(":")
        key = key.strip()
        rest = rest.strip()
        if rest != "":
            out[key] = _coerce_scalar(rest)
            i += 1
            continue
        # block under this key
        i += 1
        # skip blank lines before block detection
        while i < n and clean[i].strip() == "":
            i += 1
        # detect list of mappings (ideas:)
        if i < n and clean[i].lstrip().startswith("- "):
            items: list[dict[str, Any]] = []
            list_indent = _indent_of(clean[i])
            while i < n and (
                clean[i].strip() == ""
                or (clean[i].lstrip().startswith("- ") and _indent_of(clean[i]) == list_indent)
                or _indent_of(clean[i]) > list_indent
            ):
                if clean[i].strip() == "":
                    i += 1
                    continue
                if clean[i].lstrip().startswith("- ") and _indent_of(clean[i]) == list_indent:
                    base_indent = _indent_of(clean[i])
                    item: dict[str, Any] = {}
                    first_line = clean[i][base_indent + 2 :]
                    if first_line.strip() != "":
                        # `- key: value` form
                        ik, _, iv = first_line.partition(":")
                        ik = ik.strip()
                        iv = iv.strip()
                        if iv != "":
                            item[ik] = _coerce_scalar(iv)
                        else:
                            item[ik] = None  # placeholder, may be re-set
                    i += 1
                    while i < n and clean[i].strip() != "" and _indent_of(clean[i]) > base_indent and not clean[i].lstrip().startswith("- "):
                        sub_line = clean[i]
                        sub_indent = _indent_of(sub_line)
                        sub_key, _, sub_rest = sub_line.partition(":")
                        sub_key = sub_key.strip()
                        sub_rest = sub_rest.strip()
                        if sub_rest == "":
                            # nested list or block scalar
                            i += 1
                            if i < n and clean[i].lstrip().startswith("- ") and _indent_of(clean[i]) > sub_indent:
                                lst: list[Any] = []
                                while i < n and clean[i].lstrip().startswith("- ") and _indent_of(clean[i]) > sub_indent:
                                    val_line = clean[i].lstrip()[2:].strip()
                                    lst.append(_coerce_scalar(val_line))
                                    i += 1
                                item[sub_key] = lst
                            else:
                                # empty list (e.g. proposed_objects: with nothing under)
                                item[sub_key] = []
                            continue
                        if sub_rest in ("|", ">", "|-", ">-"):
                            # block scalar — gather indented lines
                            i += 1
                            block_lines: list[str] = []
                            block_indent = None
                            while i < n and (clean[i].strip() == "" or _indent_of(clean[i]) > sub_indent):
                                if clean[i].strip() == "":
                                    block_lines.append("")
                                    i += 1
                                    continue
                                if block_indent is None:
                                    block_indent = _indent_of(clean[i])
                                block_lines.append(clean[i][block_indent:])
                                i += 1
                            if sub_rest.startswith(">"):
                                joined = " ".join(b.strip() for b in block_lines if b.strip() != "")
                            else:
                                joined = "\n".join(block_lines).rstrip()
                            item[sub_key] = joined
                            continue
                        item[sub_key] = _coerce_scalar(sub_rest)
                        i += 1
                    items.append(item)
                else:
                    i += 1
            out[key] = items
            continue
        # otherwise: nested mapping at this key (not used by ideas.yaml top level)
        i += 1

    return out


# --- validation --------------------------------------------------------------


def validate(doc: dict[str, Any]) -> list[str]:
    errs: list[str] = []
    ideas = doc.get("ideas")
    if not isinstance(ideas, list):
        errs.append("ideas: missing or not a list")
        return errs

    seen_ids: set[str] = set()
    for n, idea in enumerate(ideas, start=1):
        if not isinstance(idea, dict):
            errs.append(f"idea #{n}: not a mapping")
            continue
        ctx = idea.get("id") or f"#{n}"

        for fld in REQUIRED_FIELDS:
            if fld not in idea:
                errs.append(f"{ctx}: missing required field '{fld}'")

        idea_id = idea.get("id")
        if idea_id:
            if idea_id in seen_ids:
                errs.append(f"{ctx}: duplicate id '{idea_id}'")
            seen_ids.add(idea_id)

        status = idea.get("status")
        if status is not None and status not in ALLOWED_STATUS:
            errs.append(f"{ctx}: invalid status '{status}'")

        belongs_to = idea.get("belongs_to")
        if belongs_to is not None and belongs_to not in ALLOWED_DESTINATIONS:
            errs.append(f"{ctx}: invalid belongs_to '{belongs_to}'")

        kind = idea.get("kind")
        if kind is not None and kind not in ALLOWED_KINDS:
            errs.append(f"{ctx}: invalid kind '{kind}'")

        if status in PROMOTED_STATUSES:
            target = idea.get("promoted_target")
            if not target:
                errs.append(f"{ctx}: status '{status}' requires promoted_target")

        if idea.get("public_claim_ready") is True:
            ev = idea.get("evidence_required")
            if not ev or (isinstance(ev, list) and len(ev) == 0):
                errs.append(f"{ctx}: public_claim_ready=true requires non-empty evidence_required")

        if idea.get("implementation_ready") is True:
            ev = idea.get("evidence_required")
            if not ev or (isinstance(ev, list) and len(ev) == 0):
                errs.append(f"{ctx}: implementation_ready=true requires a proof path in evidence_required")

        for risk_field in ("privacy_risk", "boundary_risk"):
            v = idea.get(risk_field)
            if v is not None and v not in ("low", "medium", "high"):
                errs.append(f"{ctx}: invalid {risk_field} '{v}' (expected low/medium/high)")

    return errs


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--path", default="ops/ideas/ideas.yaml")
    args = ap.parse_args()

    path = Path(args.path)
    if not path.exists():
        print(f"validate_ideas: file not found: {path}", file=sys.stderr)
        return 2

    try:
        doc = parse_ideas_yaml(path.read_text(encoding="utf-8"))
    except Exception as e:  # noqa: BLE001
        print(f"validate_ideas: parse error: {e}", file=sys.stderr)
        return 2

    errs = validate(doc)
    if errs:
        for e in errs:
            print(f"FAIL: {e}", file=sys.stderr)
        print(f"validate_ideas: {len(errs)} error(s)", file=sys.stderr)
        return 1

    n = len(doc.get("ideas", []))
    print(f"validate_ideas: ok ({n} idea(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
