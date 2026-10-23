#!/usr/bin/env python3
"""Synthetic tests for the provider-pointer guard in scripts/check-truth-spine.py (icn#2809).

The guard exists because ops/state/config/repo-map.json#repos carried a concrete
private provider remote and a `../` sibling path for months, routing agents to a
superseded repository as current provider truth. Public icn may address exactly
one repository — itself — and the provider layer is a pointer-free ROLE under
#org_repos (docs/ATLAS.md §4).

The first version of the guard substring-matched the org name and accepted
missing/empty values; review (PR #2813) showed every one of those cases passed the
NON-strict run CI actually uses. Every MUST-FAIL case below is therefore a
reconstruction of a way the guard was shown to fail open, and every MUST-PASS case
is a control proving it is not simply failing on everything. All values are
synthetic; no private location appears here.

Run: python3 scripts/tests/test_repo_pointer_guard.py
"""
import contextlib
import importlib.util
import io
import json
import pathlib
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
SPINE = ROOT / "scripts" / "check-truth-spine.py"

spec = importlib.util.spec_from_file_location("cts_pointer_under_test", SPINE)
cts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cts)

failures = []


def check(desc, cond):
    if cond:
        print(f"  ok   {desc}")
    else:
        print(f"  FAIL {desc}")
        failures.append(desc)


GOOD_SSH = "git@github.com:InterCooperative-Network/icn.git"
GOOD_ICN = {"local": ".", "remote": GOOD_SSH}


def run_spine(repo_map, *, strict=False, raw_text=None, omit_file=False):
    """Run main() against a synthetic root whose ONLY interesting file is the
    repo-map; everything else is the minimum that keeps the other guards quiet.
    Returns (exit_code, stdout). NON-strict by default — the mode CI uses."""
    with tempfile.TemporaryDirectory() as td:
        root = pathlib.Path(td)
        (root / "ops/state/truth").mkdir(parents=True)
        (root / "ops/state/config").mkdir(parents=True)
        (root / "ops/state/truth/sources.json").write_text(json.dumps({"domains": {}}))
        (root / "ops/state/ecosystem.json").write_text(json.dumps({"repos": {}}))
        map_path = root / "ops/state/config/repo-map.json"
        if not omit_file:
            map_path.write_text(raw_text if raw_text is not None else json.dumps(repo_map))
        cts.warnings.clear()
        cts.errors.clear()
        argv = sys.argv[:]
        sys.argv = ["check-truth-spine.py", "--repo-root", str(root)] + (["--strict"] if strict else [])
        buf = io.StringIO()
        try:
            with contextlib.redirect_stdout(buf):
                code = cts.main()
        finally:
            sys.argv = argv
        return code, buf.getvalue()


# ---------------------------------------------------------------------------
# 1. is_canonical_public_remote — the parse the guard turns on.
# ---------------------------------------------------------------------------
print("is_canonical_public_remote")
for desc, remote in [
    ("scp-style ssh with .git", GOOD_SSH),
    ("scp-style ssh without .git", "git@github.com:InterCooperative-Network/icn"),
    ("ssh:// form", "ssh://git@github.com/InterCooperative-Network/icn.git"),
    ("https form", "https://github.com/InterCooperative-Network/icn"),
    ("https form with .git", "https://github.com/InterCooperative-Network/icn.git"),
]:
    check(f"accepts {desc}", cts.is_canonical_public_remote(remote))
for desc, remote in [
    ("icn-infra (same org, different repo; substring match would pass)", "git@github.com:InterCooperative-Network/icn-infra.git"),
    ("another same-org repository", "git@github.com:InterCooperative-Network/nycn.git"),
    ("another host with an ICN-looking path", "git@example.invalid:InterCooperative-Network/icn.git"),
    ("another https host with the exact path", "https://git.example.invalid/InterCooperative-Network/icn.git"),
    ("userinfo-spoofed host (host is not github.com)", "https://github.com@evil.invalid/InterCooperative-Network/icn"),
    ("subdomain of github.com", "https://api.github.com/InterCooperative-Network/icn"),
    ("another org on github.com", "git@github.com:someone/icn.git"),
    ("trailing path segment", "https://github.com/InterCooperative-Network/icn/extra"),
    ("empty string", ""),
    ("None", None),
    ("non-string", 42),
    ("case-mangled org", "git@github.com:intercooperative-network/icn.git"),
    ("leading whitespace", " git@github.com:InterCooperative-Network/icn.git"),
]:
    check(f"rejects {desc}", not cts.is_canonical_public_remote(remote))

# ---------------------------------------------------------------------------
# 2. End to end through main(), NON-strict (what CI runs).
# ---------------------------------------------------------------------------
print("provider-pointer guard, non-strict main()")

code, out = run_spine({"repos": {"icn": GOOD_ICN}})
check("CONTROL: exact valid ICN remote + local '.' passes", code == 0)
check("  ...and reports the ok line", "addresses only the public icn repo" in out)
for desc, remote in [
    ("https canonical remote", "https://github.com/InterCooperative-Network/icn.git"),
    ("ssh:// canonical remote", "ssh://git@github.com/InterCooperative-Network/icn"),
]:
    code, _ = run_spine({"repos": {"icn": {"local": ".", "remote": remote}}})
    check(f"CONTROL: {desc} passes", code == 0)

MUST_FAIL = [
    ("icn-infra remote", {"repos": {"icn": {"local": ".", "remote": "git@github.com:InterCooperative-Network/icn-infra.git"}}}),
    ("another repository in the same org", {"repos": {"icn": {"local": ".", "remote": "git@github.com:InterCooperative-Network/nycn.git"}}}),
    ("another host with an ICN-looking path", {"repos": {"icn": {"local": ".", "remote": "git@example.invalid:InterCooperative-Network/icn.git"}}}),
    ("missing remote", {"repos": {"icn": {"local": "."}}}),
    ("empty remote", {"repos": {"icn": {"local": ".", "remote": ""}}}),
    ("null remote", {"repos": {"icn": {"local": ".", "remote": None}}}),
    ("missing local", {"repos": {"icn": {"remote": GOOD_SSH}}}),
    ("empty local", {"repos": {"icn": {"local": "", "remote": GOOD_SSH}}}),
    ("sibling local path", {"repos": {"icn": {"local": "../some-provider", "remote": GOOD_SSH}}}),
    ("missing repos section", {"org_repos": {}}),
    ("empty repos section", {"repos": {}}),
    ("repos is not an object", {"repos": ["icn"]}),
    ("extra repo entry beside a valid icn", {"repos": {"icn": GOOD_ICN, "some-provider": {"local": "../some-provider", "remote": "git@github.com:someone/some-provider.git"}}}),
    ("extra entry with no pointers at all", {"repos": {"icn": GOOD_ICN, "provider": {"kind": "role"}}}),
    ("icn entry is not an object", {"repos": {"icn": "."}}),
    ("only a non-icn entry", {"repos": {"some-provider": GOOD_ICN}}),
    ("top-level map is empty", {}),
    ("top-level map is not an object", []),
]
for desc, rm in MUST_FAIL:
    code, out = run_spine(rm)
    # Bind the failure to THIS guard (every pointer-guard message carries icn#2809),
    # so a future unrelated hard failure on the synthetic root cannot make these
    # cases pass for the wrong reason.
    check(f"MUST FAIL (non-strict): {desc}", code == 1 and "FAIL" in out and "icn#2809" in out)
    check(f"  ...without echoing a value", "some-provider" not in out and "example.invalid" not in out and "icn-infra" not in out and "nycn" not in out)

code, out = run_spine(None, raw_text="{not json")
check("MUST FAIL (non-strict): unparseable repo-map", code == 1 and "unparseable" in out and "icn#2809" in out)
code, out = run_spine(None, omit_file=True)
check("MUST FAIL (non-strict): absent repo-map", code == 1 and "absent" in out and "icn#2809" in out)

# strict mode must agree (a hard error is a hard error in both modes)
code, _ = run_spine({"repos": {"icn": GOOD_ICN}}, strict=True)
check("CONTROL (strict): valid map passes", code == 0)
code, _ = run_spine({"repos": {"icn": {"local": ".", "remote": "git@github.com:InterCooperative-Network/icn-infra.git"}}}, strict=True)
check("MUST FAIL (strict): icn-infra remote", code == 1)

# ---------------------------------------------------------------------------
# 3. The committed map satisfies the guard it is checked by.
# ---------------------------------------------------------------------------
print("committed map")
committed = json.loads((ROOT / "ops/state/config/repo-map.json").read_text())
check("committed repo-map.json#repos passes repo_pointer_violations", cts.repo_pointer_violations(committed) == [])

print()
if failures:
    print(f"{len(failures)} failure(s):")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
print("all checks passed")
