#!/usr/bin/env python3
"""
Firewall Denylist Checker - CI Script

Verifies the Meaning Firewall contract described in
docs/architecture/KERNEL_APP_SEPARATION.md, driven entirely by the
single-source taxonomy at scripts/firewall-taxonomy.toml:

  1. Kernel-class crates must not depend on domain/app crates (directly or
     transitively). Existing violations are pinned as [[exception]] entries.
  2. STALE-EXCEPTION DETECTION: an exception whose edge no longer exists FAILS
     the check until the entry is removed. `kind = "direct"` entries must remain
     direct production deps; `transitive-or-dev` entries must remain in the
     metadata closure.
  3. API-SHELL PINS: each shell's DIRECT domain/app dependencies must equal the
     pinned set in [shells.*] — new domain coupling on shells fails CI.

Usage:
    python3 .github/scripts/firewall_denylist.py

Exit codes:
    0: contract holds (all violations pinned, no stale entries, shells match pins)
    1: violations detected
    2: script error (cargo/JSON/taxonomy failure)

Tracking Issue: #1007 (Wave 1 Firewall Contract); taxonomy introduced 2026-07-22.
"""

import json
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Set, Tuple

sys.path.insert(0, str(Path(__file__).resolve().parent))
import firewall_taxonomy as taxonomy  # noqa: E402


def get_cargo_metadata(manifest_path: str = "icn/Cargo.toml") -> Dict:
    """Run `cargo metadata` and return the parsed JSON."""
    try:
        result = subprocess.run(
            ["cargo", "metadata", "--format-version=1", "--manifest-path", manifest_path],
            capture_output=True,
            text=True,
            check=True,
        )
        return json.loads(result.stdout)
    except subprocess.CalledProcessError as e:
        print(f"Error running cargo metadata: {e}", file=sys.stderr)
        print(f"stderr: {e.stderr}", file=sys.stderr)
        sys.exit(2)
    except json.JSONDecodeError as e:
        print(f"Error parsing cargo metadata JSON: {e}", file=sys.stderr)
        sys.exit(2)


def get_package_id(metadata: Dict, crate_name: str) -> str | None:
    for package in metadata["packages"]:
        if package["name"] == crate_name:
            return package["id"]
    return None


def get_transitive_deps(metadata: Dict, package_id: str) -> Set[str]:
    """Transitive dependency closure (all dep kinds) as package names."""
    dep_graph: Dict[str, Set[str]] = {}
    id_to_name: Dict[str, str] = {}

    for package in metadata["packages"]:
        id_to_name[package["id"]] = package["name"]
        dep_graph[package["id"]] = set()

    if "resolve" in metadata and metadata["resolve"]:
        for node in metadata["resolve"]["nodes"]:
            dep_graph[node["id"]] = set(node.get("dependencies", []))

    visited: Set[str] = set()
    queue = [package_id]
    while queue:
        current = queue.pop(0)
        if current in visited:
            continue
        visited.add(current)
        for dep in dep_graph.get(current, ()):
            if dep not in visited:
                queue.append(dep)

    return {id_to_name[p] for p in visited if p in id_to_name}


def get_direct_prod_deps(metadata: Dict, crate_name: str) -> Set[str]:
    """Direct [dependencies] (+target deps) names for a workspace crate."""
    for package in metadata["packages"]:
        if package["name"] == crate_name:
            return {
                d["name"]
                for d in package.get("dependencies", [])
                if d.get("kind") in (None, "normal")
            }
    return set()


def check_completeness(metadata: Dict, tax: Dict) -> List[str]:
    """Every workspace member must be classified in exactly one taxonomy class.

    Prevents a new crate from evading the firewall by staying unlisted, and
    forces removal of taxonomy rows when a crate leaves the workspace.
    """
    members = {p["name"] for p in metadata["packages"] if p["id"] in _workspace_ids(metadata)}
    classified = taxonomy.all_classified(tax)
    problems: List[str] = []
    for missing in sorted(members - classified):
        problems.append(
            f"workspace member '{missing}' is UNCLASSIFIED — add it to a class in "
            f"scripts/firewall-taxonomy.toml (new crates must be classified)"
        )
    for phantom in sorted(classified - members):
        problems.append(
            f"taxonomy lists '{phantom}' which is not a workspace member — remove it"
        )
    return problems


def _workspace_ids(metadata: Dict) -> Set[str]:
    return set(metadata.get("workspace_members", []))


def check_tool_isolation(metadata: Dict, tax: Dict) -> List[str]:
    """Tool-class crates must not be production deps of any restricted member."""
    tools = taxonomy.tool_crates(tax)
    unrestricted = taxonomy.unrestricted_crates(tax)
    problems: List[str] = []
    for package in metadata["packages"]:
        name = package["name"]
        if package["id"] not in _workspace_ids(metadata) or name in unrestricted:
            continue
        hits = get_direct_prod_deps(metadata, name) & tools
        for hit in sorted(hits):
            problems.append(
                f"{name} -> {hit} (tool-class crate used as a PRODUCTION dependency; "
                f"tools are test/dev support only — use a dev-dependency or reclassify)"
            )
    return problems


def check_kernel(metadata: Dict, tax: Dict) -> Tuple[List[str], List[str], List[str]]:
    """Returns (new_violations, known_violations, stale_exceptions).

    Scans the ENFORCED classes (kernel + substrate) — the taxonomy's substrate
    prohibition is real, not prose: all substrate crates were verified clean at
    introduction (2026-07-22), so no substrate exceptions exist or are accepted
    by the loader.
    """
    kernel = taxonomy.enforced_crates(tax)
    denylist = set(taxonomy.denylisted_crates(tax))
    known: Set[Tuple[str, str]] = taxonomy.exceptions(tax)
    exception_kind = {
        (e["from"], e["to"]): e["kind"] for e in taxonomy.exception_entries(tax)
    }

    new_violations: List[str] = []
    known_violations: List[str] = []
    observed_any: Set[Tuple[str, str]] = set()
    observed_direct: Set[Tuple[str, str]] = set()

    for kernel_crate in kernel:
        pkg_id = get_package_id(metadata, kernel_crate)
        if not pkg_id:
            print(f"Warning: kernel crate '{kernel_crate}' not found in workspace", file=sys.stderr)
            continue
        for forbidden in sorted(get_direct_prod_deps(metadata, kernel_crate) & denylist):
            observed_direct.add((kernel_crate, forbidden))
        deps = get_transitive_deps(metadata, pkg_id)
        for forbidden in sorted(deps.intersection(denylist)):
            edge = (kernel_crate, forbidden)
            observed_any.add(edge)
            if edge in known:
                known_violations.append(f"{kernel_crate} -> {forbidden}")
            else:
                new_violations.append(f"{kernel_crate} -> {forbidden}")

    stale: List[str] = []
    for edge in sorted(known):
        kind = exception_kind[edge]
        if kind == "direct":
            if edge not in observed_direct:
                stale.append(f"{edge[0]} -> {edge[1]} [direct edge absent]")
        elif edge not in observed_any:
            stale.append(f"{edge[0]} -> {edge[1]} [{kind} edge absent]")
    return new_violations, known_violations, stale


def check_shells(metadata: Dict, tax: Dict) -> List[str]:
    """Each api-shell's DIRECT domain/app deps must equal its pinned set."""
    denylist = set(taxonomy.denylisted_crates(tax))
    problems: List[str] = []
    for shell, pins in taxonomy.shell_pins(tax).items():
        actual = get_direct_prod_deps(metadata, shell) & denylist
        pinned = pins["domain"] | pins["app"]
        for extra in sorted(actual - pinned):
            problems.append(
                f"{shell} -> {extra} (NEW direct domain/app dep on an api-shell; "
                f"add via review to [shells.{shell}] or remove the dep)"
            )
        for gone in sorted(pinned - actual):
            problems.append(
                f"{shell} -> {gone} (pinned but no longer a direct dep; "
                f"update [shells.{shell}] — pins must track reality)"
            )
    return problems


def main() -> int:
    print("=" * 70)
    print("Firewall Denylist Check - Enforcing Kernel/App Separation")
    print("=" * 70)
    print()

    try:
        tax = taxonomy.load()
    except taxonomy.TaxonomyError as e:
        print(f"TAXONOMY ERROR: {e}", file=sys.stderr)
        return 2

    print(f"Taxonomy: scripts/firewall-taxonomy.toml v{tax['meta']['version']}")
    print(f"Enforced crates (kernel+substrate): {', '.join(taxonomy.enforced_crates(tax))}")
    print(f"Denylisted classes: domain + app ({len(taxonomy.denylisted_crates(tax))} crates)")
    print()

    print("Running cargo metadata...")
    metadata = get_cargo_metadata()
    print(f"Found {len(metadata['packages'])} packages in workspace")
    print()

    new_violations, known_violations, stale = check_kernel(metadata, tax)
    shell_problems = check_shells(metadata, tax)
    completeness_problems = check_completeness(metadata, tax)
    tool_problems = check_tool_isolation(metadata, tax)

    failed = False

    if known_violations:
        print(f"📋 KNOWN VIOLATIONS ({len(known_violations)} pinned for migration):")
        for v in known_violations:
            print(f"  • {v}")
        print()
        print("Pinned in scripts/firewall-taxonomy.toml [[exception]]; removed as the")
        print("kernel/app separation migration proceeds (Phases B–H).")
        print()

    if new_violations:
        failed = True
        print("❌ NEW FIREWALL VIOLATIONS DETECTED:")
        for v in new_violations:
            print(f"  • {v}")
        print()
        print("These edges are NOT in the taxonomy exceptions. To fix:")
        print("  1. Remove the dependency (preferred), or")
        print("  2. If temporarily unavoidable, add a reviewed [[exception]] with")
        print("     tracking + expiry to scripts/firewall-taxonomy.toml")
        print()

    if stale:
        failed = True
        print("❌ STALE EXCEPTIONS (edge no longer exists — remove the entry):")
        for v in stale:
            print(f"  • {v}")
        print()
        print("An exception may only describe a live edge. Delete these entries from")
        print("scripts/firewall-taxonomy.toml so the contract only shrinks honestly.")
        print()

    if shell_problems:
        failed = True
        print("❌ API-SHELL PIN MISMATCHES:")
        for v in shell_problems:
            print(f"  • {v}")
        print()

    if completeness_problems:
        failed = True
        print("❌ TAXONOMY COMPLETENESS FAILURES:")
        for v in completeness_problems:
            print(f"  • {v}")
        print()

    if tool_problems:
        failed = True
        print("❌ TOOL-CRATE ISOLATION FAILURES:")
        for v in tool_problems:
            print(f"  • {v}")
        print()

    if failed:
        return 1

    print("✅ Firewall dependency contract holds:")
    print(f"  • no NEW unpinned kernel/substrate violations")
    print(f"  • pinned firewall debt remains: {len(known_violations)} kernel exception edges (shrinking)")
    print("  • no stale exceptions (direct entries verified against direct deps)")
    print("  • api-shell direct domain/app deps match pins exactly")
    print("  • every workspace member classified; tool crates isolated")
    print()
    print("NOTE: this proves the DEPENDENCY contract only — kernel-api semantic")
    print("purity and reference-level coupling are owned by the Rust ratchets")
    print("(icn-core meaning_firewall.rs); see the taxonomy header ownership map.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
