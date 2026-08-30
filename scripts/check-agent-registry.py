#!/usr/bin/env python3
"""Make ops/state/truth/agents.json mechanically true about every provider surface.

The v1 registry recorded 17 paths, all under `.claude/agents/`, and claimed to be "all
available agents". `.github/agents/` held 21 GitHub Copilot agent definitions that it did
not mention -- a live surface, routed by `.github/copilot-instructions.md`, whose default
router carries `infer: true`. Nothing failed, because the only checks that existed were
one-directional and single-tree:

  - ops/scripts/drift-check.sh check 6 walked `.claude/agents/` only;
  - scripts/check-preflight-consistency.sh iterated `agents[]` only, which is why the
    `orchestrator` record could name `../../../.claude/agents/orchestrator.md` -- a path
    that escapes the repository root and matches no file -- without ever failing.

This checker replaces both with a bidirectional check over every declared surface, and
verifies the registry's own declarations rather than trusting them (icn#2632 stage 1).

Scope: inventory and provider topology. Prompt CONTENT is stage 2 and is not checked here.

Run: python3 scripts/check-agent-registry.py [--verbose]
"""
import argparse
import json
import pathlib
import re
import sys

REGISTRY = "ops/state/truth/agents.json"
SKILLS = "ops/state/truth/skills.json"


def split_front_matter(text):
    m = re.match(r"^---\n(.*?)\n---\n(.*)$", text, re.S)
    return (m.group(1), m.group(2)) if m else (None, text)


def front_matter_value(block, key):
    if not block:
        return None
    m = re.search(r"^%s:\s*(.*)$" % re.escape(key), block, re.M)
    return m.group(1).strip() if m else None


def front_matter_infer(block):
    """Tri-state: True / False / None when the key is absent.

    None is a real answer, not a failure: `.github/agents/README.md` documents what
    `infer: true` and `infer: false` mean but never states Copilot's default for an absent
    key, so the registry records the absence rather than guessing a behaviour.
    """
    raw = front_matter_value(block, "infer")
    if raw is None:
        return None
    return raw.strip().lower() == "true"


# Values the ownership model assigns to the provider. The registry must not carry a second,
# unpinned copy of them -- that is the drift this checker exists to end, one level in.
PROVIDER_OWNED_KEYS = ("description", "color", "model", "tools")


class Checker:
    def __init__(self, root, verbose=False):
        self.root = pathlib.Path(root)
        self.verbose = verbose
        self.errors = []
        self.notes = []

    def fail(self, msg):
        self.errors.append(msg)

    def ok(self, msg):
        if self.verbose:
            self.notes.append(msg)

    # ---- surface discovery -------------------------------------------------

    def definitions_on_disk(self, tree):
        """Agent definition files in a surface tree. README.md is documentation."""
        d = self.root / tree
        if not d.is_dir():
            return None
        return {p.stem: p for p in sorted(d.glob("*.md")) if p.name != "README.md"}

    def run(self):
        reg_path = self.root / REGISTRY
        if not reg_path.exists():
            print("MISSING: %s" % REGISTRY)
            return 1
        try:
            reg = json.loads(reg_path.read_text(encoding="utf-8"))
        except ValueError as exc:
            print("UNPARSEABLE: %s (%s)" % (REGISTRY, exc))
            return 1

        schema = reg.get("schema")
        if schema != "icn-agents/v2":
            self.fail("schema: expected icn-agents/v2, found %r. This checker enforces the "
                      "provider-surface model; a v1 file cannot express it." % (schema,))
            return self.report()

        surfaces = reg.get("provider_surfaces") or {}
        if not surfaces:
            self.fail("provider_surfaces: missing or empty -- nothing declares which trees "
                      "hold agent definitions, so completeness cannot mean anything.")
            return self.report()

        rel_model = reg.get("relationship_model") or {}
        records = reg.get("agents") or []

        # ---- 1. every declared surface tree must exist ----------------------
        disk = {}
        for sid, sdef in sorted(surfaces.items()):
            tree = (sdef or {}).get("tree")
            if not tree:
                self.fail("provider_surfaces.%s: no tree declared" % sid)
                continue
            # The v1 orchestrator record escaped the repo root and nothing noticed. A surface
            # tree can do the same one level higher: the checker would scan an out-of-repo
            # directory and report clean over it.
            if tree.startswith("/") or ".." in pathlib.PurePosixPath(tree).parts:
                self.fail("provider_surfaces.%s: tree %s is absolute or escapes the repository "
                          "root. A surface must be inside the repo or the checker validates "
                          "something this repo does not own." % (sid, tree))
                continue
            found = self.definitions_on_disk(tree)
            if found is None:
                self.fail("provider_surfaces.%s: declared tree %s does not exist" % (sid, tree))
                continue
            disk[sid] = found
            self.ok("surface %s -> %s (%d definitions)" % (sid, tree, len(found)))

        # ---- 2. record integrity --------------------------------------------
        seen = set()
        registered = {sid: set() for sid in disk}
        for rec in records:
            name = rec.get("name")
            if not name:
                self.fail("a record has no name: %r" % (rec,))
                continue
            if name in seen:
                self.fail("%s: duplicate record. One logical agent, one record -- two records "
                          "sharing a name is exactly the masquerade this registry prevents."
                          % name)
                continue
            seen.add(name)

            rec_surfaces = rec.get("surfaces") or {}
            if not rec_surfaces:
                self.fail("%s: no surfaces. A record that names no surface describes nothing."
                          % name)
                continue

            for sid, entry in sorted(rec_surfaces.items()):
                if sid not in surfaces:
                    self.fail("%s: surface %r is not a declared provider_surface" % (name, sid))
                    continue
                path = (entry or {}).get("path")
                if not path:
                    self.fail("%s.%s: no path" % (name, sid))
                    continue

                tree = surfaces[sid]["tree"]
                if not path.startswith(tree + "/"):
                    self.fail("%s.%s: path %s is not inside the declared tree %s"
                              % (name, sid, path, tree))
                    continue
                if ".." in pathlib.PurePosixPath(path).parts:
                    self.fail("%s.%s: path %s escapes the repository root" % (name, sid, path))
                    continue

                fp = self.root / path
                if not fp.exists():
                    self.fail("%s.%s -> %s (missing)" % (name, sid, path))
                    continue

                if fp.stem != name:
                    self.fail("%s.%s: file is %s.md. A record must not point at a file with a "
                              "different name -- that is two agents wearing one name."
                              % (name, sid, fp.stem))
                    continue

                fm, _ = split_front_matter(fp.read_text(encoding="utf-8"))
                declared = front_matter_value(fm, "name")
                if declared is not None and declared != name:
                    self.fail("%s.%s: front matter declares name: %s. The provider loads the "
                              "front-matter name, so the registry would route to a name the "
                              "provider does not answer to." % (name, sid, declared))

                # No second copy of provider-owned values. Nothing read these from the
                # registry, so a copy here could only drift -- which is exactly how the
                # registry came to describe a surface set that had not been true for months.
                for k in PROVIDER_OWNED_KEYS:
                    if k in (entry or {}):
                        self.fail("%s.%s: %r is owned by the provider definition, not the "
                                  "registry (provider_metadata_ownership.owned_by_the_provider)."
                                  " Nothing consumes it from here, so the copy can only rot. "
                                  "Read it from %s." % (name, sid, k, path))

                # infer IS owned here, so it is pinned to the file rather than trusted.
                if sid == "copilot":
                    if "infer" not in (entry or {}):
                        self.fail("%s.%s: the registry owns auto-selection reachability and "
                                  "this record does not state `infer`. An agent Copilot may "
                                  "select unbidden must not be silently unclassified."
                                  % (name, sid))
                    else:
                        actual = front_matter_infer(fm)
                        if entry["infer"] != actual:
                            self.fail(
                                "%s.%s: registry says infer=%r, %s says infer=%r. Copilot "
                                "loads the file, so the registry would be asserting the wrong "
                                "auto-selection behaviour for a live agent."
                                % (name, sid, entry["infer"], path, actual))
                        else:
                            self.ok("%s.%s: infer=%r matches the provider file"
                                    % (name, sid, actual))
                elif "infer" in (entry or {}):
                    self.fail("%s.%s: `infer` is a Copilot front-matter key. Recording it on a "
                              "%s surface asserts a behaviour that provider does not have."
                              % (name, sid, sid))

                registered.setdefault(sid, set()).add(name)
                self.ok("%s.%s -> %s" % (name, sid, path))

            # ---- 3. the relationship must be declared and true --------------
            rel = rec.get("relationship")
            if rel not in rel_model:
                self.fail("%s: relationship %r is not in relationship_model" % (name, rel))
                continue

            n = len(rec_surfaces)
            if rel == "single_surface" and n != 1:
                self.fail("%s: relationship single_surface but %d surfaces are named" % (name, n))
            if rel != "single_surface" and n < 2:
                self.fail("%s: relationship %s claims a cross-surface relationship but only %d "
                          "surface is named" % (name, rel, n))

            if rel == "exact_mirror" and n >= 2:
                bodies = {}
                for sid, entry in rec_surfaces.items():
                    fp = self.root / entry["path"]
                    if fp.exists():
                        _, body = split_front_matter(fp.read_text(encoding="utf-8"))
                        bodies[sid] = body.strip()
                if len(set(bodies.values())) > 1:
                    self.fail("%s: declared exact_mirror but the bodies differ (%s). A mirror "
                              "that drifts must fail, not silently become a variant."
                              % (name, " vs ".join(sorted(bodies))))
                else:
                    self.ok("%s: exact_mirror verified byte-identical" % name)

            # A mirror pair is an enforced promise between two surfaces, independent of the
            # record-level relationship. copilot-instructions.md documents one; nothing
            # checked it until now.
            for pair in rec.get("mirror_pairs") or []:
                if not (isinstance(pair, list) and len(pair) == 2):
                    self.fail("%s: mirror_pairs entries must name exactly two surfaces, "
                              "found %r" % (name, pair))
                    continue
                a, b = pair
                missing = [x for x in pair if x not in rec_surfaces]
                if missing:
                    self.fail("%s: mirror_pairs names surface(s) %s that this record does not "
                              "expose" % (name, ", ".join(missing)))
                    continue
                bodies = {}
                for sid in pair:
                    fp = self.root / rec_surfaces[sid]["path"]
                    if fp.exists():
                        _, body = split_front_matter(fp.read_text(encoding="utf-8"))
                        bodies[sid] = body.strip()
                if len(bodies) == 2 and bodies[a] != bodies[b]:
                    self.fail("%s: %s and %s are declared a mirror pair but their bodies "
                              "differ. The claim is enforced, so drift fails here rather "
                              "than quietly becoming a variant." % (name, a, b))
                elif len(bodies) == 2:
                    self.ok("%s: mirror pair %s/%s verified byte-identical" % (name, a, b))

            div = rec.get("divergence") or {}
            if rel == "divergent_unreviewed":
                if not div.get("owning_issue"):
                    self.fail("%s: divergent_unreviewed requires divergence.owning_issue so "
                              "the debt has an owner." % name)
                # The claim must remain true. Once stage 2 makes the bodies identical, this
                # record is asserting a divergence that no longer exists, carrying stale
                # body_similarity numbers -- and would otherwise stay green forever.
                bodies = {}
                for sid, entry in rec_surfaces.items():
                    fp = self.root / entry.get("path", "")
                    if fp.exists():
                        _, body = split_front_matter(fp.read_text(encoding="utf-8"))
                        bodies[sid] = body.strip()
                if len(bodies) > 1 and len(set(bodies.values())) == 1:
                    self.fail("%s: declared divergent_unreviewed but every body is now "
                              "identical. Promote it to exact_mirror -- a resolved divergence "
                              "must not keep claiming to be one." % name)
            if rel == "provider_variant":
                if not div.get("adjudicated"):
                    self.fail("%s: provider_variant asserts the divergence is deliberate and "
                              "requires divergence.adjudicated = true." % name)
                if not div.get("why"):
                    self.fail("%s: provider_variant requires divergence.why." % name)

        # ---- 4. completeness, the other direction ---------------------------
        for sid, found in sorted(disk.items()):
            for stem in sorted(found):
                if stem not in registered.get(sid, set()):
                    self.fail("%s/%s.md exists but no record names it on surface %r. An "
                              "unregistered provider agent must not be able to appear silently."
                              % (surfaces[sid]["tree"], stem, sid))

        # ---- 5. skills.json must not restate a false claim about this file ---
        sk = self.root / SKILLS
        if not sk.exists():
            self.fail("%s is missing. The cross-registry boundary between the two registries "
                      "cannot be verified without it." % SKILLS)
        if sk.exists():
            try:
                sj = json.loads(sk.read_text(encoding="utf-8"))
            except ValueError as exc:
                sj = None
                self.fail("skills.json is unparseable (%s). It is a registered truth owner and "
                          "holds the structured cross-registry contract, so skipping the "
                          "boundary check on a parse error would be fail-open exactly where "
                          "this checker is supposed to fail closed." % exc)
            if sj:
                trees = {v["tree"] for v in surfaces.values() if v.get("tree")}
                cross = (sj.get("declared_scope", {}) or {}).get("cross_registry") or {}
                claimed = cross.get("agent_surfaces_tracked_by_agents_json")
                if claimed is None:
                    self.fail("skills.json declared_scope.cross_registry."
                              "agent_surfaces_tracked_by_agents_json is missing. The "
                              "cross-registry claim must be structured data: a checker cannot "
                              "tell 'X is tracked' from 'X is NOT tracked' by reading prose, "
                              "which is how the two registries contradicted each other "
                              "unnoticed.")
                else:
                    claimed = set(claimed)
                    for extra in sorted(claimed - trees):
                        self.fail("skills.json says agents.json tracks %s, but agents.json "
                                  "declares no such provider surface." % extra)
                    for missing in sorted(trees - claimed):
                        self.fail("agents.json declares provider surface %s, but skills.json's "
                                  "cross_registry list omits it. Both registries must agree on "
                                  "the boundary between them." % missing)
                    if claimed == trees:
                        self.ok("skills.json and agents.json agree on all %d agent surfaces"
                                % len(trees))
                    for un in cross.get("provider_surfaces_no_registry_covers") or []:
                        if un in trees:
                            self.fail("skills.json lists %s as covered by no registry, but "
                                      "agents.json declares it as a surface." % un)

        return self.report()

    def report(self):
        for n in self.notes:
            print("  ok   %s" % n)
        if self.errors:
            print("check-agent-registry: %d PROBLEM(S)" % len(self.errors))
            for e in self.errors:
                print("  - %s" % e)
            return 1
        print("check-agent-registry: clean")
        return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo-root", default=None,
                    help="repository root to check (default: the root containing this script)")
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()
    root = (pathlib.Path(args.repo_root).resolve() if args.repo_root
            else pathlib.Path(__file__).resolve().parents[1])
    return Checker(root, args.verbose).run()


if __name__ == "__main__":
    sys.exit(main())
