#!/usr/bin/env python3
"""check-skill-registry.py — make the skill-ownership claim in ops/state/truth/skills.json true.

The registry declares, for every skill in a declared scope, a canonical owner and the exact
relationship each provider-facing copy may have with it. This checker is the mechanism that
makes those declarations enforceable rather than aspirational.

Everything it enforces is DATA in the registry: scan scope, mirror policies, forbidden patterns
and per-skill assertions. Adding a skill, a tree or a rule must never require editing this file.

Usage:
    python3 scripts/check-skill-registry.py [--repo-root PATH] [--verbose]

Exit 0 = the registry is mechanically true. Exit 1 = it is not.
Refs icn#2633.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REGISTRY = "ops/state/truth/skills.json"


# ── front matter ──────────────────────────────────────────────────────────────

def split_front_matter(text: str) -> tuple[str | None, str]:
    """Return (front_matter_block, body). front_matter is None when the file has none.

    Deliberately not a YAML parse: we compare blocks, and a dependency-free reader cannot
    silently normalise away a difference that matters.
    """
    if not text.startswith("---\n"):
        return None, text
    end = text.find("\n---\n", 3)
    if end == -1:
        return None, text
    return text[4:end + 1], text[end + 5:]


def front_matter_keys(block: str) -> dict[str, str]:
    """Split a front-matter block into {top-level key: full value block}."""
    keys: dict[str, list[str]] = {}
    current: str | None = None
    for line in block.splitlines(keepends=True):
        m = re.match(r"([A-Za-z_][\w.-]*):", line)
        if m and not line[0].isspace():
            current = m.group(1)
            keys.setdefault(current, [])
        if current is not None:
            keys[current].append(line)
    return {k: "".join(v) for k, v in keys.items()}


# ── registry model ────────────────────────────────────────────────────────────

class Checker:
    def __init__(self, root: Path, verbose: bool = False) -> None:
        self.root = root
        self.verbose = verbose
        self.failures: list[str] = []
        self.checked = 0

    def fail(self, msg: str) -> None:
        self.failures.append(msg)

    def ok(self, msg: str) -> None:
        self.checked += 1
        if self.verbose:
            print(f"  ok    {msg}")

    # -- population ------------------------------------------------------------

    def skill_dirs(self, tree: str) -> set[str]:
        d = self.root / tree
        if not d.is_dir():
            return set()
        return {p.name for p in d.iterdir() if (p.is_dir() or p.is_symlink())}

    def run(self) -> int:
        reg_path = self.root / REGISTRY
        if not reg_path.is_file():
            self.fail(f"registry missing: {REGISTRY}")
            return self.report()
        try:
            reg = json.loads(reg_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            self.fail(f"registry is not valid JSON: {exc}")
            return self.report()

        skills = reg.get("skills", {})
        enforcement = reg.get("enforcement", {})
        scope = enforcement.get("scan_scope", {})
        canonical_trees = scope.get("canonical_trees", [])
        provider_trees = scope.get("provider_trees", [])

        entries = self.collect_entries(skills)
        self.check_population(entries, skills, canonical_trees, provider_trees)
        self.check_relationships(entries, reg)
        self.check_content(reg, canonical_trees + provider_trees)
        self.check_cargo_workspace_root(canonical_trees + provider_trees)
        return self.report()

    def collect_entries(self, skills: dict) -> list[dict]:
        out = []
        for e in skills.get("ops_automation_canonical", []):
            out.append({
                "name": e["name"],
                "canonical_path": e["canonical_path"],
                "mirrors": e.get("provider_mirrors", []),
                "assertions": e.get("canonical_assertions"),
                "mirror_policy": e.get("mirror_policy", "none"),
                "group": "ops_automation_canonical",
            })
        for e in skills.get("icn_level", []):
            out.append({
                "name": e["name"],
                "canonical_path": e["canonical_path"],
                "mirrors": e.get("provider_mirrors", []),
                "assertions": e.get("canonical_assertions"),
                "mirror_policy": e.get("mirror_policy", "none"),
                "group": "icn_level",
            })
        for e in skills.get("project_level_only", []):
            out.append({
                "name": e["name"],
                "canonical_path": None,
                "provider_only_path": e["path"],
                "rationale": e.get("rationale"),
                "mirrors": [],
                "assertions": None,
                "group": "project_level_only",
            })
        return out

    def check_population(self, entries, skills, canonical_trees, provider_trees) -> None:
        """Both directions: nothing registered that is absent, nothing present that is unregistered."""
        # Registered -> present
        for e in entries:
            if e["canonical_path"]:
                p = self.root / e["canonical_path"]
                if not p.is_file():
                    self.fail(f"registered canonical path does not exist: "
                              f"{e['name']} -> {e['canonical_path']}")
                else:
                    self.ok(f"canonical present: {e['canonical_path']}")
            if e["group"] == "project_level_only":
                p = self.root / e["provider_only_path"]
                if not p.is_file():
                    self.fail(f"registered provider-only path does not exist: "
                              f"{e['name']} -> {e['provider_only_path']}")
                if not e.get("rationale"):
                    self.fail(f"provider-only skill has no rationale: {e['name']} "
                              f"(a provider-only skill must say why it has no canonical owner)")
            for m in e["mirrors"]:
                p = self.root / m["path"]
                if not (p.is_file() or p.is_symlink()):
                    self.fail(f"registered provider mirror does not exist: "
                              f"{e['name']} -> {m['path']}")

        # Present -> registered
        registered_by_tree: dict[str, set[str]] = {}
        for e in entries:
            for path in filter(None, [e["canonical_path"], e.get("provider_only_path")]
                               + [m["path"] for m in e["mirrors"]]):
                parts = Path(path).parts
                tree = str(Path(*parts[:-2]))
                registered_by_tree.setdefault(tree, set()).add(parts[-2])

        for tree in canonical_trees + provider_trees:
            present = self.skill_dirs(tree)
            known = registered_by_tree.get(tree, set())
            for orphan in sorted(present - known):
                self.fail(f"unregistered skill directory: {tree}/{orphan} — add it to "
                          f"{REGISTRY} (icn_level, project_level_only, or a provider mirror). "
                          f"An unregistered skill makes the registry's completeness claim false.")
            for ghost in sorted(known - present):
                self.fail(f"registered skill has no directory on disk: {tree}/{ghost}")
            if present and not (present - known) and not (known - present):
                self.ok(f"population matches registry: {tree} ({len(present)} skills)")

    # -- relationships ---------------------------------------------------------

    def check_relationships(self, entries, reg) -> None:
        policy = reg.get("adapter_policy", {})
        permitted = set(policy.get("permitted_front_matter_keys", []))
        body_must_match = policy.get("body_must_be_identical", True)

        for e in entries:
            if not e["canonical_path"]:
                continue
            cpath = self.root / e["canonical_path"]
            if not cpath.is_file():
                continue
            canonical = cpath.read_text(encoding="utf-8")

            for m in e["mirrors"]:
                mpath = self.root / m["path"]
                pol = m.get("policy", "exact_mirror")

                if mpath.is_symlink():
                    target = mpath.resolve()
                    if target == cpath.resolve():
                        self.ok(f"{e['name']}: mirror is a symlink to canonical (drift-proof)")
                    else:
                        self.fail(f"{e['name']}: mirror symlink {m['path']} resolves to {target}, "
                                  f"not to canonical {e['canonical_path']}")
                    continue
                # A symlinked skill DIRECTORY is equally drift-proof.
                if mpath.parent.is_symlink():
                    if mpath.parent.resolve() == cpath.parent.resolve():
                        self.ok(f"{e['name']}: mirror directory is a symlink to canonical (drift-proof)")
                        continue
                    self.fail(f"{e['name']}: mirror directory {m['path']} is a symlink to "
                              f"{mpath.parent.resolve()}, not to the canonical directory")
                    continue

                if not mpath.is_file():
                    self.fail(f"{e['name']}: declared {pol} mirror is missing: {m['path']}")
                    continue
                mirror = mpath.read_text(encoding="utf-8")

                if pol == "exact_mirror":
                    if mirror == canonical:
                        self.ok(f"{e['name']}: exact mirror in sync")
                    else:
                        self.fail(
                            f"{e['name']}: exact-mirror divergence between {e['canonical_path']} and "
                            f"{m['path']}. The registry declares this mirror carries no provider-specific "
                            f"behaviour, so it must be byte-identical. Either re-sync it from canonical, or "
                            f"— if the difference is genuine provider mechanics — declare it "
                            f'policy: "adapter" with an explicit adapter_fields list.')
                elif pol == "adapter":
                    declared = set(m.get("adapter_fields", []))
                    if not declared:
                        self.fail(f"{e['name']}: declared adapter with no adapter_fields — an adapter "
                                  f"must name the provider mechanics it differs in")
                        continue
                    undeclared = declared - permitted
                    if undeclared:
                        self.fail(f"{e['name']}: adapter_fields {sorted(undeclared)} are not in "
                                  f"adapter_policy.permitted_front_matter_keys — those keys are "
                                  f"semantics, not provider mechanics")
                        continue
                    self.compare_adapter(e["name"], canonical, mirror, declared, permitted,
                                         body_must_match, e["canonical_path"], m["path"])
                elif pol == "none":
                    self.fail(f"{e['name']}: mirror declared policy \"none\" but a mirror path is listed")
                else:
                    self.fail(f"{e['name']}: unknown mirror policy {pol!r}")

            if not e["mirrors"] and e["group"] != "project_level_only":
                mp = e.get("mirror_policy", "none")
                self.ok(f"{e['name']}: canonical-only ({mp})")

    def compare_adapter(self, name, canonical, mirror, declared, permitted,
                        body_must_match, cpath, mpath) -> None:
        c_fm, c_body = split_front_matter(canonical)
        m_fm, m_body = split_front_matter(mirror)
        if c_fm is None or m_fm is None:
            self.fail(f"{name}: adapter comparison needs front matter in both copies")
            return
        if body_must_match and c_body != m_body:
            self.fail(f"{name}: adapter body differs from canonical ({cpath} vs {mpath}). "
                      f"An adapter may differ only in declared provider mechanics; workflow "
                      f"semantics come from the canonical skill.")
        c_keys, m_keys = front_matter_keys(c_fm), front_matter_keys(m_fm)
        differing = {k for k in set(c_keys) | set(m_keys) if c_keys.get(k) != m_keys.get(k)}
        undeclared = differing - declared
        if undeclared:
            semantic = sorted(undeclared - permitted)
            mech = sorted(undeclared & permitted)
            if semantic:
                self.fail(f"{name}: adapter diverges in semantic front-matter key(s) {semantic}. "
                          f"Those are owned by the canonical skill — a provider copy that changes "
                          f"them has become an independent truth owner.")
            if mech:
                self.fail(f"{name}: adapter diverges in undeclared provider key(s) {mech} — "
                          f"add them to adapter_fields or re-sync them from canonical.")
        if not undeclared and (not body_must_match or c_body == m_body):
            self.ok(f"{name}: adapter differs only in declared fields {sorted(declared)}")

    # -- cargo workspace root ---------------------------------------------------

    def cargo_workspace_subdir(self) -> str | None:
        """The Cargo workspace subdirectory, from repo-map.json — never hardcoded here.

        icn#2642: three skills invoked `cargo` after resolving REPO_ROOT to the monorepo
        root, which has no Cargo.toml. The fix pattern recurs, so it is now a structural
        check instead of three one-off fixes: this repository's workspace subdir name is a
        fact repo-map.json already owns (repos.icn.cargo_workspace), so the checker reads it
        rather than assuming "icn" — a repo-map change updates the check automatically.
        """
        p = self.root / "ops" / "state" / "config" / "repo-map.json"
        if not p.is_file():
            return None
        try:
            d = json.loads(p.read_text(encoding="utf-8"))
            return d.get("repos", {}).get("icn", {}).get("cargo_workspace")
        except (json.JSONDecodeError, AttributeError):
            return None

    def check_cargo_workspace_root(self, trees: list[str]) -> None:
        """Every fenced bash block invoking `cargo` must resolve the workspace root itself.

        A block is self-contained evidence, not the surrounding prose: an agent (or a human)
        may copy a single fenced block in isolation, so a `cd` stated only in a preceding
        paragraph does not protect the command inside the block. This is deliberately
        block-scoped, not file-scoped or line-numbered, so it holds for any future skill
        without editing this checker.
        """
        subdir = self.cargo_workspace_subdir()
        if not subdir:
            self.fail("could not resolve repos.icn.cargo_workspace from repo-map.json — "
                      "cargo-workspace-root check cannot run")
            return

        cargo_line = re.compile(r"^\s*cargo\s")
        # Both alternations anchor on the destination, never on the syntax of the `cd`.
        # An earlier form carried a bare `|\(cd\s` alternation to catch the inline
        # `(cd ... && cargo ...)` subshell, but that accepted a subshell cd to *anywhere*
        # — `(cd docs && ls)` beside a bare `cargo test` passed. The subdir-anchored
        # alternation already covers the subshell form, so the bare one was pure
        # accept-set widening: a guard that fails open on the very class it screens for.
        establishes_root = re.compile(
            r'cd\s+"?\$\{?CARGO_ROOT\}?"?'
            r'|cd\s+[^\n&|;]*/' + re.escape(subdir) + r'\b'
        )

        for tree in trees:
            d = self.root / tree
            if not d.is_dir():
                continue
            for f in sorted(d.glob("*/SKILL.md")):
                rel = f.relative_to(self.root)
                text = f.read_text(encoding="utf-8")
                blocks = re.findall(r"```bash\n(.*?)```", text, re.S)
                for i, block in enumerate(blocks):
                    lines = block.splitlines()
                    has_cargo = any(cargo_line.match(ln) for ln in lines)
                    if not has_cargo:
                        continue
                    if establishes_root.search(block):
                        self.ok(f"{rel} block {i}: cargo invocation resolves workspace root")
                    else:
                        self.fail(
                            f"{rel}: a fenced bash block invokes `cargo` without resolving the "
                            f"Cargo workspace root ({subdir}/) within that same block — the "
                            f"monorepo root has no Cargo.toml, so this fails if copied verbatim. "
                            f"Add `cd \"${{CARGO_ROOT}}\"` (or an inline `(cd ... && cargo ...)`) "
                            f"inside the block itself:\n            " +
                            "\n            ".join(ln for ln in lines if cargo_line.match(ln))[:300]
                        )

    # -- content ---------------------------------------------------------------

    def check_content(self, reg, trees) -> None:
        enforcement = reg.get("enforcement", {})
        patterns = enforcement.get("forbidden_patterns", [])
        require_tc = enforcement.get("require_truth_contract", False)

        compiled = []
        for p in patterns:
            allow = p.get("allow_if_line_matches")
            compiled.append((p["id"], re.compile(p["pattern"]),
                             re.compile(allow) if allow else None, p.get("why", "")))

        for tree in trees:
            d = self.root / tree
            if not d.is_dir():
                continue
            for f in sorted(d.glob("*/SKILL.md")):
                rel = f.relative_to(self.root)
                text = f.read_text(encoding="utf-8")
                if require_tc and not re.search(r"^truth_contract:", text, re.M):
                    self.fail(f"missing truth_contract: {rel} — without it the skill can silently "
                              f"encode volatile state")
                for pid, rx, allow, why in compiled:
                    for i, line in enumerate(text.splitlines(), 1):
                        if rx.search(line) and not (allow and allow.search(line)):
                            self.fail(f"{rel}:{i}: [{pid}] {why}\n            {line.strip()}")

        # Per-skill canonical assertions (registry data, not code).
        for group in ("ops_automation_canonical", "icn_level"):
            for e in reg.get("skills", {}).get(group, []):
                a = e.get("canonical_assertions")
                if not a:
                    continue
                p = self.root / e["canonical_path"]
                if not p.is_file():
                    continue
                text = p.read_text(encoding="utf-8")
                for ref in a.get("must_reference", []):
                    if ref not in text:
                        self.fail(f"{e['canonical_path']}: canonical assertion failed — must reference "
                                  f"{ref!r}. {a.get('rationale','')}")
                    else:
                        self.ok(f"{e['name']}: references {ref!r}")
                for rule in a.get("must_not_match", []):
                    rx = re.compile(rule["pattern"] if isinstance(rule, dict) else rule)
                    for i, line in enumerate(text.splitlines(), 1):
                        if rx.search(line):
                            why = rule.get("why", "") if isinstance(rule, dict) else ""
                            self.fail(f"{e['canonical_path']}:{i}: canonical assertion failed — "
                                      f"matched forbidden pattern {rx.pattern!r}. {why}\n"
                                      f"            {line.strip()}")

    def report(self) -> int:
        if self.failures:
            print("check-skill-registry: FAIL")
            for f in self.failures:
                print(f"  FAIL  {f}")
            print(f"\n{len(self.failures)} failure(s); {self.checked} assertion(s) passed")
            return 1
        print(f"check-skill-registry: clean ({self.checked} assertions passed)")
        return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo-root", default=None,
                    help="repository root to check (default: the root containing this script)")
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()
    root = Path(args.repo_root).resolve() if args.repo_root else Path(__file__).resolve().parents[1]
    return Checker(root, args.verbose).run()


if __name__ == "__main__":
    sys.exit(main())
