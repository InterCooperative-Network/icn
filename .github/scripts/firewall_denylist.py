#!/usr/bin/env python3
"""
Firewall Denylist Checker - CI Script

Verifies that kernel crates do not depend on domain/app crates (directly or transitively).
This enforces the Meaning Firewall contract described in docs/architecture/KERNEL_APP_SEPARATION.md.

Usage:
    python3 .github/scripts/firewall_denylist.py

Exit codes:
    0: No violations detected (firewall is intact)
    1: Violations detected (kernel imports forbidden domain crates)
    2: Script error (e.g., cargo or JSON parsing failure)

Tracking Issue: #1007 (Wave 1 Firewall Contract)
"""

import json
import subprocess
import sys
from typing import Set, Dict, List

# Kernel crates that MUST NOT depend on domain/app crates
KERNEL_CRATES = [
    "icn-core",
    "icn-kernel-api",
    "icn-net",
    "icn-gossip",
    "icn-store",
]

# Domain/app crates that kernel MUST NOT depend on
DENYLISTED_CRATES = [
    # Domain crates (internals)
    "icn-trust",
    "icn-governance",
    "icn-ledger",
    "icn-ccl",
    "icn-compute",
    "icn-entity",
    "icn-community",
    "icn-federation",
    "icn-steward",
    "icn-coop",
    # App crates (anything under apps/)
    # Note: These would be listed if they exist as separate crates
]

# Known violations that are being tracked for migration
# These are allowed temporarily until the kernel/app separation is complete
# Tracked in: docs/architecture/KERNEL_APP_SEPARATION.md
KNOWN_VIOLATIONS = {
    # icn-core currently depends on domain crates for app bootstrapping
    # Migration tracked in Wave 2-6 of the kernel separation roadmap
    ("icn-core", "icn-ccl"),
    ("icn-core", "icn-community"),
    ("icn-core", "icn-compute"),
    ("icn-core", "icn-coop"),
    ("icn-core", "icn-entity"),
    ("icn-core", "icn-federation"),
    ("icn-core", "icn-governance"),
    ("icn-core", "icn-ledger"),
    ("icn-core", "icn-steward"),
    ("icn-core", "icn-trust"),
}


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
    """Find the package ID for a crate by name."""
    for package in metadata["packages"]:
        if package["name"] == crate_name:
            return package["id"]
    return None


def get_dependencies(metadata: Dict, package_id: str) -> Set[str]:
    """
    Get the transitive dependency closure for a package.
    
    Returns a set of package names (not IDs).
    """
    # Build dependency graph
    dep_graph = {}
    id_to_name = {}
    
    for package in metadata["packages"]:
        pkg_id = package["id"]
        pkg_name = package["name"]
        id_to_name[pkg_id] = pkg_name
        
        # Extract dependencies from resolve graph
        dep_graph[pkg_id] = set()
    
    # Parse the resolve graph
    if "resolve" in metadata and metadata["resolve"]:
        for node in metadata["resolve"]["nodes"]:
            node_id = node["id"]
            # Dependencies are listed as package IDs
            deps = set(node.get("dependencies", []))
            dep_graph[node_id] = deps
    
    # BFS to find transitive closure
    visited = set()
    queue = [package_id]
    
    while queue:
        current = queue.pop(0)
        if current in visited:
            continue
        visited.add(current)
        
        if current in dep_graph:
            for dep in dep_graph[current]:
                if dep not in visited:
                    queue.append(dep)
    
    # Convert IDs to names
    dep_names = set()
    for pkg_id in visited:
        if pkg_id in id_to_name:
            dep_names.add(id_to_name[pkg_id])
    
    return dep_names


def check_firewall(metadata: Dict) -> tuple[List[str], List[str]]:
    """
    Check kernel crates for forbidden dependencies.
    
    Returns a tuple of (new_violations, known_violations).
    - new_violations: violations that are NOT in KNOWN_VIOLATIONS (fails CI)
    - known_violations: violations that ARE in KNOWN_VIOLATIONS (tracked, passes CI)
    """
    new_violations = []
    known_violations = []
    
    for kernel_crate in KERNEL_CRATES:
        pkg_id = get_package_id(metadata, kernel_crate)
        if not pkg_id:
            print(f"Warning: Kernel crate '{kernel_crate}' not found in workspace", file=sys.stderr)
            continue
        
        # Get transitive dependencies
        deps = get_dependencies(metadata, pkg_id)
        
        # Check for denylisted dependencies
        forbidden_deps = deps.intersection(DENYLISTED_CRATES)
        
        if forbidden_deps:
            for forbidden in sorted(forbidden_deps):
                violation = (kernel_crate, forbidden)
                if violation in KNOWN_VIOLATIONS:
                    known_violations.append(f"{kernel_crate} -> {forbidden}")
                else:
                    new_violations.append(f"{kernel_crate} -> {forbidden}")
    
    return new_violations, known_violations


def main():
    print("=" * 70)
    print("Firewall Denylist Check - Enforcing Kernel/App Separation")
    print("=" * 70)
    print()
    
    # Get cargo metadata
    print("Running cargo metadata...")
    metadata = get_cargo_metadata()
    print(f"Found {len(metadata['packages'])} packages in workspace")
    print()
    
    # Check for violations
    print("Checking kernel crates for forbidden dependencies...")
    print(f"Kernel crates: {', '.join(KERNEL_CRATES)}")
    print(f"Denylisted crates: {', '.join(DENYLISTED_CRATES)}")
    print()
    
    new_violations, known_violations = check_firewall(metadata)
    
    # Report known violations (tracked, not blocking)
    if known_violations:
        print(f"📋 KNOWN VIOLATIONS ({len(known_violations)} tracked for migration):")
        print()
        for violation in known_violations:
            print(f"  • {violation}")
        print()
        print("These violations are tracked in KNOWN_VIOLATIONS and will be fixed")
        print("as part of the kernel/app separation migration (Waves 2-6).")
        print("See docs/architecture/KERNEL_APP_SEPARATION.md for roadmap.")
        print()
    
    # Report new violations (blocking)
    if new_violations:
        print("❌ NEW FIREWALL VIOLATIONS DETECTED:")
        print()
        for violation in new_violations:
            print(f"  • {violation}")
        print()
        print("These are NEW dependencies that violate the Meaning Firewall contract.")
        print("See docs/architecture/KERNEL_APP_SEPARATION.md for details.")
        print()
        print("To fix:")
        print("  1. Remove direct imports of domain crates from kernel code")
        print("  2. Use PolicyOracle pattern to receive constraints from apps")
        print("  3. See migration examples in the docs")
        print()
        return 1
    
    if known_violations:
        print("✅ No NEW firewall violations detected!")
        print()
        print(f"Migration progress: {len(KNOWN_VIOLATIONS) - len(known_violations)}/{len(KNOWN_VIOLATIONS)} violations resolved")
        return 0
    else:
        print("✅ No firewall violations detected!")
        print()
        print("All kernel crates maintain proper separation from domain/app crates.")
        return 0


if __name__ == "__main__":
    sys.exit(main())
