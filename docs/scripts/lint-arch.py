#!/usr/bin/env python3
"""
Lint docs/ARCHITECTURE.md for regulatory compliance and technical correctness

Checks:
1. Forbidden terms: Mana, CoVM, icn-covm, blockchain, token (except "capability token"),
   payment, currency, balance (except in code blocks), wallet
   - HARD FORBIDDEN: Mana, CoVM, icn-covm (never allowed, period)
   - SOFT FORBIDDEN: blockchain, token, payment, currency, balance, wallet
     (allowed in negation/comparison context: "Not a...", "Use X not Y", anti-claims)
2. Every ## section has a truth-class comment (normative/descriptive/operational)
3. Crate names mentioned exist in Cargo.toml (if --cargo provided)
4. Internal markdown links resolve (if --check-links)
5. No Mana or CoVM references anywhere

Usage:
    python3 lint-arch.py docs/ARCHITECTURE.md
    python3 lint-arch.py --cargo path/to/Cargo.toml docs/ARCHITECTURE.md
    python3 lint-arch.py --check-links docs/ARCHITECTURE.md

Exit codes:
    0: Clean
    1: Violations found
"""

import sys
import re
import argparse
from pathlib import Path
from typing import NamedTuple

class Violation(NamedTuple):
    line: int
    severity: str  # "error" or "warning"
    message: str

# --- Hard forbidden: never allowed anywhere ---
HARD_FORBIDDEN = {
    "Mana": re.compile(r"\bMana\b"),
    "CoVM": re.compile(r"\bCoVM\b"),
    "icn-covm": re.compile(r"\bicn-covm\b"),
}

# --- Soft forbidden: allowed in negation/comparison context ---
SOFT_FORBIDDEN = {
    "blockchain": re.compile(r"\bblockchain\b", re.IGNORECASE),
    "payment": re.compile(r"\bpayment\b", re.IGNORECASE),
    "currency": re.compile(r"\bcurrency\b", re.IGNORECASE),
    "balance": re.compile(r"\bbalance\b", re.IGNORECASE),
    "wallet": re.compile(r"\bwallet\b", re.IGNORECASE),
    "token": re.compile(r"\btoken\b", re.IGNORECASE),
}

# Patterns that indicate negation/comparison context (soft terms are OK here)
NEGATION_PATTERNS = [
    re.compile(r"\bnot\s+a\b", re.IGNORECASE),
    re.compile(r"\bnot\s+a\s", re.IGNORECASE),
    re.compile(r"\buse\s+\".+\"\s+not\b", re.IGNORECASE),
    re.compile(r"\buse\s+\S+\s+not\b", re.IGNORECASE),
    re.compile(r"\binstead\s+of\b", re.IGNORECASE),
    re.compile(r"\bnever\b", re.IGNORECASE),
    re.compile(r"\bno\s+\w+\s+(payment|currency|balance|wallet|token|blockchain)", re.IGNORECASE),
    re.compile(r"\brefuse[sd]?\b", re.IGNORECASE),
    re.compile(r"\bavoid\b", re.IGNORECASE),
    re.compile(r"\bforbidden\b", re.IGNORECASE),
    re.compile(r"\banti-feature\b", re.IGNORECASE),
    re.compile(r"\bnot\b.*\b(payment|currency|balance|wallet|blockchain)\b", re.IGNORECASE),
    re.compile(r"\"[^\"]*\".*not.*\"[^\"]*\"", re.IGNORECASE),  # "X" not "Y" patterns
]

# Sections where soft forbidden terms are expected (anti-claims, regulatory)
NEGATION_SECTIONS = {
    "Boundary Conditions",
    "Anti-Claims",
    "Regulatory Architecture",
    "Terminology discipline",
}

TRUTH_CLASS_COMMENT = re.compile(r"<!--\s*truth:\s*(normative|descriptive|operational)\s*-->")
GENERATED_COMMENT = re.compile(r"<!--\s*generated:\s*true\s*-->")
SECTION_H2 = re.compile(r"^##\s+\d+\.\s+(.+)")
SECTION_ANY = re.compile(r"^#{1,3}\s+(.+)")
CODE_BLOCK_FENCE = re.compile(r"^```")
INTERNAL_LINK = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")
CRATE_REF = re.compile(r"`(icn-[a-z][-a-z]*)`")


def is_negation_context(line: str) -> bool:
    """Check if the line uses forbidden terms in negation/comparison."""
    return any(p.search(line) for p in NEGATION_PATTERNS)


def lint_document(
    doc_path: str,
    cargo_path: str | None = None,
    check_links: bool = False,
) -> list[Violation]:
    violations: list[Violation] = []

    with open(doc_path) as f:
        lines = f.readlines()

    # Load crate names if Cargo.toml provided
    known_crates: set[str] | None = None
    if cargo_path:
        try:
            with open(cargo_path) as f:
                content = f.read()
            members_match = re.findall(r'members\s*=\s*\[(.*?)\]', content, re.DOTALL)
            if members_match:
                known_crates = set()
                for m in members_match:
                    for name in re.findall(r'"([^"]+)"', m):
                        # Extract crate name from path like "crates/icn-core"
                        known_crates.add(name.split("/")[-1])
        except Exception as e:
            print(f"Warning: Could not load Cargo.toml: {e}", file=sys.stderr)

    # Track state
    in_code_block = False
    current_h2_section = ""
    current_section_name = ""
    h2_sections: list[tuple[int, str, bool]] = []  # (line, name, has_truth_class)
    current_has_truth = False

    for i, raw_line in enumerate(lines, 1):
        line = raw_line.rstrip()

        # Track code blocks
        if CODE_BLOCK_FENCE.match(line.strip()):
            in_code_block = not in_code_block
            continue

        if in_code_block:
            continue

        # Track sections
        h2_match = SECTION_H2.match(line)
        if h2_match:
            # Close previous section
            if current_h2_section:
                h2_sections.append((i, current_h2_section, current_has_truth))
            current_h2_section = h2_match.group(1).strip()
            current_section_name = current_h2_section
            current_has_truth = False

        any_match = SECTION_ANY.match(line)
        if any_match:
            current_section_name = any_match.group(1).strip()

        # Track truth class comments
        if TRUTH_CLASS_COMMENT.search(line):
            current_has_truth = True

        # --- Check hard forbidden terms (always error) ---
        for term, pattern in HARD_FORBIDDEN.items():
            if pattern.search(line):
                violations.append(Violation(i, "error", f"Hard-forbidden term: '{term}'"))

        # --- Check soft forbidden terms (context-sensitive) ---
        in_negation_section = any(ns.lower() in current_section_name.lower() for ns in NEGATION_SECTIONS)

        for term, pattern in SOFT_FORBIDDEN.items():
            if not pattern.search(line):
                continue

            # Allow "capability token"
            if term == "token" and re.search(r"\bcapability\s+token\b", line, re.IGNORECASE):
                continue

            # Allow in negation context
            if is_negation_context(line):
                continue

            # Allow in sections that are explicitly about what ICN is NOT
            if in_negation_section:
                continue

            # Allow in table headers/rows that show "Current → Target" mappings
            if "|" in line and ("→" in line or "->" in line or "not" in line.lower()):
                continue

            violations.append(Violation(i, "warning", f"Soft-forbidden term: '{term}' (not in negation context)"))

        # --- Check internal links ---
        if check_links:
            for link_match in INTERNAL_LINK.finditer(line):
                link_path = link_match.group(2)
                if link_path.startswith(("http", "#", "mailto:")):
                    continue
                base_dir = Path(doc_path).parent
                full_path = base_dir / link_path
                if not full_path.exists():
                    violations.append(Violation(i, "warning", f"Link not found: '{link_path}'"))

        # --- Check crate references ---
        if known_crates is not None:
            for crate_match in CRATE_REF.finditer(line):
                crate_name = crate_match.group(1)
                if crate_name not in known_crates:
                    violations.append(Violation(i, "warning", f"Unknown crate: '{crate_name}'"))

    # Close last section
    if current_h2_section:
        h2_sections.append((len(lines), current_h2_section, current_has_truth))

    # --- Check all H2 sections have truth class ---
    for line_num, section_name, has_truth in h2_sections:
        if not has_truth:
            violations.append(Violation(line_num, "error", f"Section '{section_name}' missing <!-- truth: ... --> comment"))

    return violations


def main():
    parser = argparse.ArgumentParser(description="Lint docs/ARCHITECTURE.md")
    parser.add_argument("document", help="Path to ARCHITECTURE.md")
    parser.add_argument("--cargo", default=None, help="Path to workspace Cargo.toml")
    parser.add_argument("--check-links", action="store_true", help="Verify internal links resolve")

    args = parser.parse_args()

    if not Path(args.document).exists():
        print(f"ERROR: {args.document} not found", file=sys.stderr)
        sys.exit(1)

    violations = lint_document(args.document, args.cargo, args.check_links)

    errors = [v for v in violations if v.severity == "error"]
    warnings = [v for v in violations if v.severity == "warning"]

    if violations:
        if errors:
            print(f"\nERRORS ({len(errors)}):")
            for v in errors:
                print(f"  Line {v.line}: {v.message}")
        if warnings:
            print(f"\nWARNINGS ({len(warnings)}):")
            for v in warnings:
                print(f"  Line {v.line}: {v.message}")
        print(f"\nTotal: {len(errors)} errors, {len(warnings)} warnings")
        sys.exit(1 if errors else 0)
    else:
        print("CLEAN: No violations found")
        sys.exit(0)


if __name__ == "__main__":
    main()
