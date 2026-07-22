#!/usr/bin/env python3
"""Self-test + cross-mechanism sync check for the meaning-firewall taxonomy.

Run in CI (firewall-contract job) BEFORE firewall_denylist.py. Fails when any
enforcement mechanism's embedded crate lists drift from
scripts/firewall-taxonomy.toml (the single source of truth), or when the
taxonomy itself is malformed.

Checks:
  1. Taxonomy loads and validates (loader invariants incl. one-class-per-crate).
  2. Loader negative fixtures: duplicate classification, malformed exception
     shape, and non-kernel exception 'from' are rejected.
  3. .claude/hooks/kernel-crates.conf [kernel]/[domain] == taxonomy classes.
  4. icn-core meaning_firewall.rs KERNEL_CRATES/DOMAIN_CRATES consts == taxonomy.
  5. scripts/check-meaning-firewall.sh + .github/workflows/ci.yml kernel-deps
     step actually source their lists from the taxonomy (marker check — they
     load at runtime, so content cannot drift; absence of the marker means
     someone reintroduced a hardcoded list).
  6. scripts/forbidden-deps-allowlist.txt stays retired (no live edge entries;
     exceptions live only in the taxonomy).

Exit codes: 0 ok / 1 drift or malformed / 2 script error.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import firewall_taxonomy as taxonomy  # noqa: E402

REPO = taxonomy.REPO_ROOT
FAILURES: list[str] = []


def fail(msg: str) -> None:
    FAILURES.append(msg)


def parse_conf_section(text: str, section: str) -> list[str]:
    """Parse a [section] crate list from kernel-crates.conf."""
    lines = []
    in_section = False
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("["):
            in_section = s == f"[{section}]"
            continue
        if in_section and s and not s.startswith("#"):
            lines.append(s)
    return lines


def parse_rust_const(text: str, name: str) -> list[str]:
    """Parse `const NAME: &[&str] = &[ ... ];` from meaning_firewall.rs."""
    m = re.search(rf"const {name}: &\[&str\] = &\[(.*?)\];", text, re.S)
    if not m:
        return []
    return re.findall(r'"([^"]+)"', m.group(1))


def run_denylist(taxonomy_path: Path) -> tuple[int, str]:
    """Run the REAL firewall_denylist.py against a doctored taxonomy fixture."""
    import os
    import subprocess

    env = dict(os.environ)
    env["ICN_FIREWALL_TAXONOMY"] = str(taxonomy_path)
    p = subprocess.run(
        [sys.executable, str(Path(__file__).resolve().parent / "firewall_denylist.py")],
        capture_output=True,
        text=True,
        env=env,
        cwd=REPO,
    )
    return p.returncode, p.stdout + p.stderr


def failure_path_tests() -> None:
    """Prove the gates actually FAIL on real violations (not just pattern checks).

    Uses doctored copies of the live taxonomy against the real cargo metadata,
    plus an isolated fixture repo for the shell scanner. Leaves the repository
    untouched.
    """
    import re as _re
    import shutil
    import stat
    import subprocess
    import tempfile

    real = (REPO / "scripts" / "firewall-taxonomy.toml").read_text()

    with tempfile.TemporaryDirectory() as td:
        tdir = Path(td)

        # F1: removing a live exception must surface it as a NEW violation.
        doctored = _re.sub(
            r"\n\[\[exception\]\][^\[]*?to = \"icn-ledger\"\n[^\[]*?expiry = \"edge-absent\"\n",
            "\n",
            real,
            count=1,
        )
        assert doctored != real, "F1 fixture surgery failed to remove the entry"
        f1 = tdir / "f1.toml"
        f1.write_text(doctored)
        rc, out = run_denylist(f1)
        if rc != 1 or "NEW FIREWALL VIOLATIONS" not in out or "icn-core -> icn-ledger" not in out:
            fail(f"F1 failure-path: removing the icn-ledger exception did not fail as a NEW violation (rc={rc})")

        # F2: a transitive-only edge pinned as kind="direct" must be STALE
        # (transitive/dev residue must not satisfy a direct exception).
        doctored = real.replace(
            'to = "icn-zkp"\nkind = "transitive-or-dev"',
            'to = "icn-zkp"\nkind = "direct"',
            1,
        )
        assert doctored != real, "F2 fixture surgery failed"
        f2 = tdir / "f2.toml"
        f2.write_text(doctored)
        rc, out = run_denylist(f2)
        if rc != 1 or "direct edge absent" not in out:
            fail(f"F2 failure-path: direct-kind exception satisfied by transitive residue (rc={rc})")

        # F3: an unpinned api-shell domain dep must fail the shell pins.
        doctored = real.replace(', "icn-ledger"]\napp = []\n\n[shells.icn-api]', ']\napp = []\n\n[shells.icn-api]', 1)
        assert doctored != real, "F3 fixture surgery failed"
        f3 = tdir / "f3.toml"
        f3.write_text(doctored)
        rc, out = run_denylist(f3)
        if rc != 1 or "NEW direct domain/app dep on an api-shell" not in out:
            fail(f"F3 failure-path: unpinned shell domain dep did not fail (rc={rc})")

        # F6: an UNCLASSIFIED workspace member must fail completeness.
        doctored = real.replace('bin = ["icnd", "icnctl", "icn-console"]', 'bin = ["icnd", "icnctl"]', 1)
        assert doctored != real, "F6 fixture surgery failed"
        f6 = tdir / "f6.toml"
        f6.write_text(doctored)
        rc, out = run_denylist(f6)
        if rc != 1 or "UNCLASSIFIED" not in out:
            fail(f"F6 failure-path: unclassified member did not fail completeness (rc={rc})")

        # F5: the shell scanner must fail on a real new import in an
        # exception-free kernel crate — proven in an isolated fixture repo.
        fx = tdir / "fixture-repo"
        (fx / "scripts").mkdir(parents=True)
        (fx / ".github" / "scripts").mkdir(parents=True)
        (fx / "icn" / "crates" / "icn-net" / "src").mkdir(parents=True)
        shutil.copy(REPO / ".github" / "scripts" / "firewall_taxonomy.py", fx / ".github" / "scripts")
        script_dst = fx / "scripts" / "check-meaning-firewall.sh"
        shutil.copy(REPO / "scripts" / "check-meaning-firewall.sh", script_dst)
        script_dst.chmod(script_dst.stat().st_mode | stat.S_IEXEC)
        (fx / "scripts" / "firewall-taxonomy.toml").write_text(
            '[meta]\nversion="fixture"\nupdated="fixture"\nowner="t"\nadr="t"\n'
            '[classes]\nkernel=["icn-net"]\nsubstrate=[]\napi-shell=[]\n'
            'domain=["icn-trust"]\napp=[]\nfacade=[]\ntool=[]\nbin=[]\n'
        )
        clean_env = {k: v for k, v in __import__("os").environ.items() if k != "ICN_FIREWALL_TAXONOMY"}
        (fx / "icn" / "crates" / "icn-net" / "src" / "lib.rs").write_text("pub fn ok() {}\n")
        p = subprocess.run(["bash", str(script_dst)], capture_output=True, text=True, env=clean_env)
        if p.returncode != 0:
            fail(f"F5a failure-path: clean fixture repo should pass (rc={p.returncode}): {p.stdout}{p.stderr}")
        (fx / "icn" / "crates" / "icn-net" / "src" / "lib.rs").write_text(
            "use icn_trust::TrustGraph;\npub fn bad() {}\n"
        )
        p = subprocess.run(["bash", str(script_dst)], capture_output=True, text=True, env=clean_env)
        if p.returncode != 1 or "NEW violation" not in p.stdout:
            fail(f"F5b failure-path: violating fixture repo did not fail (rc={p.returncode}): {p.stdout}{p.stderr}")


def main() -> int:
    # 1. Load + validate
    try:
        tax = taxonomy.load()
    except taxonomy.TaxonomyError as e:
        print(f"FAIL: taxonomy invalid: {e}")
        return 1

    kernel = taxonomy.kernel_crates(tax)
    domain = list(tax["classes"]["domain"])

    # 2. Negative fixtures (loader must reject malformed taxonomies).
    # Every fixture declares ALL required classes so it fails for its INTENDED
    # reason, not for a missing class list.
    import tempfile

    BASE_CLASSES = 'api-shell=[]\napp=[]\nfacade=[]\ntool=[]\nbin=[]\n'
    bad_dup = (
        '[meta]\nversion="0"\nupdated="0"\nowner="t"\nadr="t"\n'
        '[classes]\nkernel=["icn-core"]\nsubstrate=["icn-core"]\n'
        'domain=[]\n' + BASE_CLASSES
    )
    bad_from = (
        '[meta]\nversion="0"\nupdated="0"\nowner="t"\nadr="t"\n'
        '[classes]\nkernel=["icn-core"]\nsubstrate=[]\n'
        'domain=["icn-trust"]\n' + BASE_CLASSES +
        '[[exception]]\nfrom="icn-trust"\nto="icn-core"\nkind="direct"\ntracking="t"\nexpiry="edge-absent"\n'
    )
    bad_kind = (
        '[meta]\nversion="0"\nupdated="0"\nowner="t"\nadr="t"\n'
        '[classes]\nkernel=["icn-core"]\nsubstrate=[]\n'
        'domain=["icn-trust"]\n' + BASE_CLASSES +
        '[[exception]]\nfrom="icn-core"\nto="icn-trust"\nkind="maybe"\ntracking="t"\nexpiry="edge-absent"\n'
    )
    bad_to = (
        '[meta]\nversion="0"\nupdated="0"\nowner="t"\nadr="t"\n'
        '[classes]\nkernel=["icn-core"]\nsubstrate=["icn-time"]\n'
        'domain=["icn-trust"]\n' + BASE_CLASSES +
        '[[exception]]\nfrom="icn-core"\nto="icn-time"\nkind="direct"\ntracking="t"\nexpiry="edge-absent"\n'
    )
    for label, content in (
        ("duplicate-class", bad_dup),
        ("non-kernel-from", bad_from),
        ("bad-exception-kind", bad_kind),
        ("non-denylisted-to", bad_to),
    ):
        with tempfile.NamedTemporaryFile("w", suffix=".toml", delete=False) as f:
            f.write(content)
            tmp = Path(f.name)
        try:
            taxonomy.load(tmp)
            fail(f"negative fixture '{label}' was ACCEPTED (loader too permissive)")
        except taxonomy.TaxonomyError:
            pass
        finally:
            tmp.unlink()

    # 3. Hook conf sync
    conf_path = REPO / ".claude" / "hooks" / "kernel-crates.conf"
    if conf_path.exists():
        conf = conf_path.read_text()
        conf_kernel = parse_conf_section(conf, "kernel")
        conf_domain = parse_conf_section(conf, "domain")
        if sorted(conf_kernel) != sorted(kernel):
            fail(
                f"kernel-crates.conf [kernel] drift: {sorted(conf_kernel)} != taxonomy {sorted(kernel)}"
            )
        if sorted(conf_domain) != sorted(domain):
            fail(
                f"kernel-crates.conf [domain] drift: {sorted(conf_domain)} != taxonomy {sorted(domain)}"
            )
    else:
        fail("missing .claude/hooks/kernel-crates.conf")

    # 4. Ratchet consts sync
    mf_path = REPO / "icn" / "crates" / "icn-core" / "src" / "meaning_firewall.rs"
    if mf_path.exists():
        mf = mf_path.read_text()
        rust_kernel = parse_rust_const(mf, "KERNEL_CRATES")
        rust_domain = parse_rust_const(mf, "DOMAIN_CRATES")
        if sorted(rust_kernel) != sorted(kernel):
            fail(
                f"meaning_firewall.rs KERNEL_CRATES drift: {sorted(rust_kernel)} != taxonomy {sorted(kernel)}"
            )
        if sorted(rust_domain) != sorted(domain):
            fail(
                f"meaning_firewall.rs DOMAIN_CRATES drift: {sorted(rust_domain)} != taxonomy {sorted(domain)}"
            )
        rust_shell = parse_rust_const(mf, "API_SHELL_CRATES")
        rust_app = parse_rust_const(mf, "APP_CRATES")
        if sorted(rust_shell) != sorted(tax["classes"]["api-shell"]):
            fail(
                f"meaning_firewall.rs API_SHELL_CRATES drift: {sorted(rust_shell)} != taxonomy {sorted(tax['classes']['api-shell'])}"
            )
        if sorted(rust_app) != sorted(tax["classes"]["app"]):
            fail(
                f"meaning_firewall.rs APP_CRATES drift: {sorted(rust_app)} != taxonomy {sorted(tax['classes']['app'])}"
            )
    else:
        fail("missing icn/crates/icn-core/src/meaning_firewall.rs")

    # 5. Runtime-loading markers
    script = REPO / "scripts" / "check-meaning-firewall.sh"
    if script.exists():
        if "firewall_taxonomy.py" not in script.read_text():
            fail("check-meaning-firewall.sh no longer loads the taxonomy (hardcoded list reintroduced?)")
    else:
        fail("missing scripts/check-meaning-firewall.sh")

    ci = REPO / ".github" / "workflows" / "ci.yml"
    if ci.exists():
        ci_text = ci.read_text()
        if "firewall_taxonomy.py --bash" not in ci_text:
            fail("ci.yml kernel-deps step no longer sources lists from the taxonomy")
    else:
        fail("missing .github/workflows/ci.yml")

    # 6. Retired allowlist stays retired
    allow = REPO / "scripts" / "forbidden-deps-allowlist.txt"
    if allow.exists():
        live = [
            l
            for l in allow.read_text().splitlines()
            if l.strip() and not l.strip().startswith("#")
        ]
        if live:
            fail(
                "forbidden-deps-allowlist.txt has live entries; it is RETIRED — "
                "exceptions belong in scripts/firewall-taxonomy.toml: " + ", ".join(live)
            )

    # 7. Failure-path battery: the real checkers must go RED on real violations.
    failure_path_tests()

    if FAILURES:
        print("❌ FIREWALL TAXONOMY SYNC FAILURES:")
        for f_ in FAILURES:
            print(f"  • {f_}")
        print()
        print("Fix by updating the drifted mechanism to match scripts/firewall-taxonomy.toml")
        print("(or update the taxonomy via review if the change is intentional).")
        return 1

    print("✅ taxonomy valid; all mechanisms in sync (hook conf, ratchet consts,")
    print("   script+CI runtime loading, allowlist retired); negative fixtures")
    print("   rejected; failure-path battery green (new-edge / stale-direct /")
    print("   shell-pin / shell-scan violations all provably fail).")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:  # noqa: BLE001
        print(f"SCRIPT ERROR: {e}", file=sys.stderr)
        sys.exit(2)
