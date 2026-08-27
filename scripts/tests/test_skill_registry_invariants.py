#!/usr/bin/env python3
"""Mutation controls for scripts/check-skill-registry.py (Refs icn#2633).

A checker that only ever runs against a known-good tree proves nothing: it would pass
identically if every rule were a no-op. Each case here copies the real skill trees into a
temporary root, applies ONE mutation, and requires the checker to fail for a stated reason.
The positive controls run first, so a broken fixture surfaces as a fixture bug, not as a
spurious pass.

Usage: python3 scripts/tests/test_skill_registry_invariants.py [--verbose]
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CHECKER = REPO / "scripts" / "check-skill-registry.py"
REGISTRY = "ops/state/truth/skills.json"
REPO_MAP = "ops/state/config/repo-map.json"
TREES = ["ops/automation/skills", ".agents/skills", ".claude/skills"]

VERBOSE = False
results: list[tuple[bool, str, str]] = []


def make_fixture(tmp: Path) -> Path:
    """A minimal repo root: the registry plus the three skill trees, symlinks preserved."""
    root = tmp / "repo"
    (root / "ops" / "state" / "truth").mkdir(parents=True)
    (root / "ops" / "state" / "config").mkdir(parents=True)
    shutil.copy2(REPO / REGISTRY, root / REGISTRY)
    shutil.copy2(REPO / REPO_MAP, root / REPO_MAP)
    for tree in TREES:
        shutil.copytree(REPO / tree, root / tree, symlinks=True)
    return root


def run_checker(root: Path) -> tuple[int, str]:
    p = subprocess.run([sys.executable, str(CHECKER), "--repo-root", str(root)],
                       capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def load_registry(root: Path) -> dict:
    return json.loads((root / REGISTRY).read_text())


def save_registry(root: Path, reg: dict) -> None:
    (root / REGISTRY).write_text(json.dumps(reg, indent=2) + "\n")


def record(name: str, passed: bool, detail: str) -> None:
    results.append((passed, name, detail))
    print(f"  {'PASS' if passed else 'FAIL'}  {name}")
    if VERBOSE or not passed:
        print(f"        {detail}")


def expect_clean(name: str, mutate=None) -> None:
    with tempfile.TemporaryDirectory() as td:
        root = make_fixture(Path(td))
        if mutate:
            mutate(root)
        rc, out = run_checker(root)
        record(name, rc == 0, "checker accepted the tree" if rc == 0
               else f"expected exit 0, got {rc}: {out.strip()[:600]}")


def expect_fail(name: str, mutate, must_mention: str) -> None:
    with tempfile.TemporaryDirectory() as td:
        root = make_fixture(Path(td))
        # Control: the unmutated fixture must be clean, or the case proves nothing.
        rc, out = run_checker(root)
        if rc != 0:
            record(name, False, f"fixture was already failing before mutation: {out.strip()[:400]}")
            return
        mutate(root)
        rc, out = run_checker(root)
        if rc == 0:
            record(name, False, "checker still exited 0 AFTER the mutation — the rule is a no-op")
            return
        if must_mention not in out:
            record(name, False, f"failed, but not for the stated reason "
                                f"(no {must_mention!r} in output): {out.strip()[:400]}")
            return
        record(name, True, f"rejected with {must_mention!r}")


# ── mutations ─────────────────────────────────────────────────────────────────

def add_unregistered_skill(root: Path) -> None:
    d = root / ".claude" / "skills" / "smuggled-skill"
    d.mkdir(parents=True)
    (d / "SKILL.md").write_text("---\nname: smuggled-skill\ntruth_contract:\n  canonical_sources: []\n---\n\nbody\n")


def diverge_exact_mirror(root: Path) -> None:
    p = root / ".claude" / "skills" / "verify" / "SKILL.md"
    p.write_text(p.read_text() + "\nAn extra provider-side instruction nobody reviewed.\n")


def delete_exact_mirror(root: Path) -> None:
    """The mirror is deleted; its registry entry still declares it. Both directions must notice."""
    shutil.rmtree(root / ".claude" / "skills" / "verify")


def declare_valid_adapter(root: Path) -> None:
    """A legitimate adapter: identical body, differs only in a declared provider key."""
    p = root / ".claude" / "skills" / "bench" / "SKILL.md"
    s = p.read_text().replace('allowed-tools: "Bash"', 'allowed-tools: "Bash, Read"')
    assert s != p.read_text(), "bench fixture lacks the expected allowed-tools line"
    p.write_text(s)
    reg = load_registry(root)
    for e in reg["skills"]["icn_level"]:
        if e["name"] == "bench":
            e["provider_mirrors"][0]["policy"] = "adapter"
            e["provider_mirrors"][0]["adapter_fields"] = ["allowed-tools"]
    save_registry(root, reg)


def adapter_diverges_undeclared(root: Path) -> None:
    """Declared adapter for allowed-tools, but the body was changed too."""
    declare_valid_adapter(root)
    p = root / ".claude" / "skills" / "bench" / "SKILL.md"
    p.write_text(p.read_text() + "\nProvider-only workflow step that canonical never saw.\n")


def adapter_diverges_semantically(root: Path) -> None:
    """Declared adapter, but the mirror rewrote truth_contract — becoming a second owner."""
    declare_valid_adapter(root)
    p = root / ".claude" / "skills" / "bench" / "SKILL.md"
    p.write_text(p.read_text().replace("truth_contract:", "truth_contract:  # provider override\n  extra_owner: true\n", 1))


def canonical_retired_machine_path(root: Path) -> None:
    p = root / ".agents" / "skills" / "devnet" / "SKILL.md"
    p.write_text(p.read_text().replace('cd "${REPO_ROOT}/deploy/devnet" && make up',
                                       "cd /home/ubuntu/projects/icn/deploy/devnet && make up", 1))


def mirror_retired_machine_path(root: Path) -> None:
    p = root / ".claude" / "skills" / "devnet" / "SKILL.md"
    p.write_text(p.read_text().replace('cd "${REPO_ROOT}/deploy/devnet" && make up',
                                       "cd /home/ubuntu/projects/icn/deploy/devnet && make up", 1))


def merge_prs_hardcodes_check_count(root: Path) -> None:
    p = root / ".agents" / "skills" / "merge-prs" / "SKILL.md"
    p.write_text(p.read_text() + "\nMerge once all 11 required checks are green.\n")


def merge_prs_hardcodes_check_list(root: Path) -> None:
    p = root / ".agents" / "skills" / "merge-prs" / "SKILL.md"
    p.write_text(p.read_text() + '\nrequired = {"Build Release", "Test", "Clippy"}\n')


def merge_prs_drops_authorization(root: Path) -> None:
    p = root / ".agents" / "skills" / "merge-prs" / "SKILL.md"
    p.write_text(p.read_text().replace("explicit, per-PR maintainer authorization", "authorization"))


def merge_pr_regrows_a_raw_merge_path(root: Path) -> None:
    """icn#2651 stage B: the thin wrapper must not carry a merge it can execute by itself."""
    p = root / ".agents" / "skills" / "merge-pr" / "SKILL.md"
    p.write_text(p.read_text() + "\nIf the executable is unavailable, run `gh pr merge $PR`.\n")


def merge_pr_regrows_a_privileged_path(root: Path) -> None:
    p = root / ".agents" / "skills" / "merge-pr" / "SKILL.md"
    p.write_text(p.read_text() + "\nIf protection blocks the merge, retry with --admin.\n")


def merge_pr_regrows_a_deferred_merge(root: Path) -> None:
    p = root / ".agents" / "skills" / "merge-pr" / "SKILL.md"
    p.write_text(p.read_text() + "\nIf a required check is still running, arm it with --auto.\n")


def merge_pr_bakes_a_protection_path(root: Path) -> None:
    p = root / ".agents" / "skills" / "merge-pr" / "SKILL.md"
    p.write_text(p.read_text()
                 + "\nRead contexts from repos/o/n/branches/main/protection before merging.\n")


def merge_pr_drops_the_executable(root: Path) -> None:
    """A wrapper that no longer names the program it wraps has become an owner again."""
    p = root / ".agents" / "skills" / "merge-pr" / "SKILL.md"
    text = p.read_text().replace("icn-merge-pr", "the merge helper")
    p.write_text(text)
    mirror = root / ".claude" / "skills" / "merge-pr" / "SKILL.md"
    mirror.write_text(text)


def canonical_declares_state_canonical(root: Path) -> None:
    p = root / ".agents" / "skills" / "icn-dev" / "SKILL.md"
    p.write_text(p.read_text() + "\n- Current state: `docs/STATE.md` (canonical)\n")


def register_project_only_skill(root: Path) -> None:
    """A genuinely provider-only skill, correctly registered, must be accepted."""
    d = root / ".claude" / "skills" / "provider-only-demo"
    d.mkdir(parents=True)
    (d / "SKILL.md").write_text("---\nname: provider-only-demo\ntruth_contract:\n  canonical_sources: []\n---\n\nbody\n")
    reg = load_registry(root)
    reg["skills"]["project_level_only"].append({
        "name": "provider-only-demo",
        "path": ".claude/skills/provider-only-demo/SKILL.md",
        "rationale": "fixture: provider-only skill with no ICN-level counterpart by design",
    })
    save_registry(root, reg)


def project_only_without_rationale(root: Path) -> None:
    register_project_only_skill(root)
    reg = load_registry(root)
    for e in reg["skills"]["project_level_only"]:
        if e["name"] == "provider-only-demo":
            e.pop("rationale")
    save_registry(root, reg)


def symlink_the_mirror(root: Path) -> None:
    """The operational symlink representation: structurally drift-proof, must be accepted."""
    d = root / ".claude" / "skills" / "bench"
    shutil.rmtree(d)
    d.symlink_to(Path("..") / ".." / ".agents" / "skills" / "bench", target_is_directory=True)


def broken_symlink_mirror(root: Path) -> None:
    d = root / ".claude" / "skills" / "bench"
    shutil.rmtree(d)
    d.symlink_to(Path("..") / ".." / ".agents" / "skills" / "changelog", target_is_directory=True)


def registered_skill_missing_from_disk(root: Path) -> None:
    shutil.rmtree(root / ".agents" / "skills" / "bench")
    shutil.rmtree(root / ".claude" / "skills" / "bench")


def cargo_command_without_workspace_root(root: Path) -> None:
    """New skill: valid front matter, but a bash block invokes cargo with no cd to the workspace."""
    d = root / ".agents" / "skills" / "cargo-no-root-demo"
    d.mkdir(parents=True)
    (d / "SKILL.md").write_text(
        "---\nname: cargo-no-root-demo\ntruth_contract:\n  canonical_sources: []\n---\n\n"
        "## Steps\n\n```bash\nREPO_ROOT=\"$(git rev-parse --show-toplevel)\"\n"
        "cargo test --workspace\n```\n"
    )
    reg = load_registry(root)
    reg["skills"]["icn_level"].append({
        "name": "cargo-no-root-demo",
        "canonical_path": ".agents/skills/cargo-no-root-demo/SKILL.md",
        "provider_mirrors": [],
        "mirror_policy": "none",
    })
    save_registry(root, reg)


def cargo_command_with_workspace_root(root: Path) -> None:
    """The same shape, but the block resolves CARGO_ROOT and cds into it first — must PASS."""
    d = root / ".agents" / "skills" / "cargo-with-root-demo"
    d.mkdir(parents=True)
    (d / "SKILL.md").write_text(
        "---\nname: cargo-with-root-demo\ntruth_contract:\n  canonical_sources: []\n---\n\n"
        "## Steps\n\n```bash\nREPO_ROOT=\"$(git rev-parse --show-toplevel)\"\n"
        "CARGO_ROOT=\"${REPO_ROOT}/icn\"\ncd \"${CARGO_ROOT}\"\ncargo test --workspace\n```\n"
    )
    reg = load_registry(root)
    reg["skills"]["icn_level"].append({
        "name": "cargo-with-root-demo",
        "canonical_path": ".agents/skills/cargo-with-root-demo/SKILL.md",
        "provider_mirrors": [],
        "mirror_policy": "none",
    })
    save_registry(root, reg)


def cargo_command_with_unrelated_subshell_cd(root: Path) -> None:
    r"""A cargo block whose only `cd` is a subshell into an unrelated directory — must FAIL.

    Controls the guard against anchoring on `cd` *syntax* instead of the `cd` *destination*.
    An earlier form of the check carried a bare `|\(cd\s` alternation, so this exact shape
    — `(cd docs && ls)` next to a bare `cargo test` — passed while still being broken.
    """
    d = root / ".agents" / "skills" / "cargo-decoy-cd-demo"
    d.mkdir(parents=True)
    (d / "SKILL.md").write_text(
        "---\nname: cargo-decoy-cd-demo\ntruth_contract:\n  canonical_sources: []\n---\n\n"
        "## Steps\n\n```bash\nREPO_ROOT=\"$(git rev-parse --show-toplevel)\"\n"
        "(cd docs && ls)\ncargo test --workspace\n```\n"
    )
    reg = load_registry(root)
    reg["skills"]["icn_level"].append({
        "name": "cargo-decoy-cd-demo",
        "canonical_path": ".agents/skills/cargo-decoy-cd-demo/SKILL.md",
        "provider_mirrors": [],
        "mirror_policy": "none",
    })
    save_registry(root, reg)


def strip_truth_contract(root: Path) -> None:
    p = root / ".agents" / "skills" / "bench" / "SKILL.md"
    p.write_text(p.read_text().replace("truth_contract:", "informal_notes:", 1))


# ── suite ─────────────────────────────────────────────────────────────────────

def main() -> int:
    global VERBOSE
    ap = argparse.ArgumentParser()
    ap.add_argument("--verbose", action="store_true")
    VERBOSE = ap.parse_args().verbose

    print("test_skill_registry_invariants — positive controls")
    expect_clean("clean tree passes")
    expect_clean("declared adapter with only a declared field difference passes", declare_valid_adapter)
    expect_clean("registered provider-only skill passes", register_project_only_skill)
    expect_clean("symlinked mirror passes (drift-proof representation)", symlink_the_mirror)

    print("\ntest_skill_registry_invariants — negative controls (mutation)")
    expect_fail("unregistered skill directory is rejected",
                add_unregistered_skill, "unregistered skill directory")
    expect_fail("exact-mirror content divergence is rejected",
                diverge_exact_mirror, "exact-mirror divergence")
    expect_fail("missing exact mirror is rejected",
                delete_exact_mirror, "registered provider mirror does not exist")
    expect_fail("adapter diverging in an undeclared body is rejected",
                adapter_diverges_undeclared, "adapter body differs")
    expect_fail("adapter diverging in truth_contract is rejected",
                adapter_diverges_semantically, "semantic front-matter key")
    expect_fail("retired machine path in a CANONICAL skill is rejected",
                canonical_retired_machine_path, "retired-machine-root")
    expect_fail("retired machine path in a PROVIDER mirror is rejected",
                mirror_retired_machine_path, "retired-machine-root")
    expect_fail("merge-prs hardcoding a required-check COUNT is rejected",
                merge_prs_hardcodes_check_count, "required-check count")
    expect_fail("merge-prs hardcoding a required-check NAME is rejected",
                merge_prs_hardcodes_check_list, "literal required-check name")
    expect_fail("merge-prs dropping the merge-authorization boundary is rejected",
                merge_prs_drops_authorization, "explicit, per-PR maintainer authorization")
    expect_fail("merge-pr regrowing a raw merge command is rejected",
                merge_pr_regrows_a_raw_merge_path, "gh")
    expect_fail("merge-pr regrowing a privileged merge path is rejected",
                merge_pr_regrows_a_privileged_path, "privileged path")
    expect_fail("merge-pr regrowing a deferred/armed merge is rejected",
                merge_pr_regrows_a_deferred_merge, "never arms")
    expect_fail("merge-pr baking a branch-protection path is rejected",
                merge_pr_bakes_a_protection_path, "baked path")
    expect_fail("merge-pr no longer naming the trusted executable is rejected",
                merge_pr_drops_the_executable, "icn-merge-pr")
    expect_fail("a skill declaring STATE.md canonical is rejected",
                canonical_declares_state_canonical, "state-phase-canonicality")
    expect_fail("provider-only skill without a rationale is rejected",
                project_only_without_rationale, "no rationale")
    expect_fail("mirror symlinked to the WRONG canonical skill is rejected",
                broken_symlink_mirror, "not to the canonical directory")
    expect_fail("registered skill absent from disk is rejected",
                registered_skill_missing_from_disk, "no directory on disk")
    expect_fail("skill without truth_contract is rejected",
                strip_truth_contract, "missing truth_contract")
    expect_fail("cargo invocation with no workspace-root cd in its own block is rejected",
                cargo_command_without_workspace_root, "without resolving the Cargo workspace root")
    expect_clean("cargo invocation that resolves the workspace root in its own block passes",
                 cargo_command_with_workspace_root)
    expect_fail("cargo block whose only cd is a subshell elsewhere is rejected",
                cargo_command_with_unrelated_subshell_cd, "without resolving the Cargo workspace root")

    failed = [r for r in results if not r[0]]
    print(f"\n{len(results) - len(failed)}/{len(results)} cases passed")
    if failed:
        print("FAIL: the checker did not behave as specified for:")
        for _, name, detail in failed:
            print(f"  - {name}: {detail}")
        return 1
    print("check-skill-registry enforcement verified: every rule rejects its own violation.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
