#!/usr/bin/env python3
"""check-truth-spine.py — validate the truth spine's own integrity (warning-mode).

Companion to ops/scripts/drift-check.sh and scripts/check-preflight-consistency.sh.
Checks that ops/state/truth/sources.json — the arbiter of truth ownership — points at
things that exist, does not double-assign owners, and that the org-repo coordination
registry (ops/state/config/repo-map.json#org_repos) stays consistent with the
ecosystem index (ops/state/ecosystem.json).

Warning-mode by default: exits 0 with warnings printed unless --strict is passed
(future ratchet, mirroring the readiness-overclaim linter's warning->blocking path).
Unconditional (HARD, independent of --strict) failures: an unreadable/unparseable
sources.json; the public-map boundary guard — the public icn machine-readable
maps (repo-map.json, ecosystem.json) must carry NO concrete host addresses or
operational values (docs/ATLAS.md §5; icn-infra ADR-0005), which live only in the
private network-ops repo (this map lists infrastructure ROLES only); the volatile-
currency invariant — a `volatile` domain must not serve a terminal (closed/expired)
record as its current answer without explicitly declaring dormancy; and the
SessionStart source guard — an unconditional startup hook must not name a
repo-relative file that does not exist (both icn#2634).

Deliberately NOT checked (v1): content freshness of downstream lock files (needs
private-repo access — VM-session concern, not CI).
"""

import argparse
import datetime
import ipaddress
import json
import re
import sys
from pathlib import Path

# Owners that legitimately are not a path inside this repo: live queries and
# external/private sources. A domain registered here is exempt from the on-disk
# existence check, never from needing an owner at all.
NONFILE_OWNERS = {"git", "github-api", "downstream-repos", "private-network-ops"}

# Staleness warning thresholds by declared stability class (days).
STALENESS_DAYS = {"volatile": 14, "slow-changing": 120}

# Date-ish keys we look for in JSON owner files, in preference order.
DATE_KEYS = ("last_reviewed", "reviewed_at", "start_date")

# --- Volatile-owner currency semantics (icn#2634) ------------------------------
# A `volatile` domain claims to answer "what is true RIGHT NOW". Two failure modes
# are possible and only one of them is a defect:
#
#   (a) the owner presents a TERMINAL record (a closed sprint, an expired window)
#       as if it were the domain's current answer  -> DEFECT, hard fail;
#   (b) the owner truthfully reports that nothing is active right now
#       -> CORRECT, and it must not be nagged for being "old".
#
# The rule therefore encodes semantics, not the calendar: a terminal record is
# allowed only when the owner ALSO declares dormancy explicitly and machine-
# readably. "There is no active sprint" is a valid current answer; "here is a
# five-month-old closed sprint" is not. Correspondingly, the staleness threshold
# below is applied only to owners that claim something IS active — dating a
# dormant record would be exactly the calendar superstition this replaces.
TERMINAL_STATES = {
    "closed", "done", "archived", "expired", "superseded",
    "complete", "completed", "cancelled", "canceled",
}
DORMANT_VALUES = {"dormant", "inactive", "none", "paused"}
DORMANCY_KEYS = ("cadence", "lifecycle")


def declares_dormancy(data: dict) -> bool:
    """True if a domain owner explicitly, machine-readably says 'nothing is active
    right now'. Requires a positive declaration — a missing key is not dormancy,
    or every malformed owner would pass by omission."""
    for key in DORMANCY_KEYS:
        if str(data.get(key, "")).strip().lower() in DORMANT_VALUES:
            return True
    # `active_<thing>: null` is the other accepted spelling, but the key must be
    # PRESENT: `.get()` returning None for an absent key would fail open.
    return any(k.startswith("active_") and data[k] is None for k in data)

# Public-map boundary guard (docs/ATLAS.md §5; icn-infra ADR-0005): the PUBLIC
# icn repo's machine-readable maps must never carry concrete host addresses or
# operational values — those live only in the private network-ops repo. This is
# a HARD failure (independent of --strict) if any reappear.
PUBLIC_MAP_FILES = ("ops/state/config/repo-map.json", "ops/state/ecosystem.json")
_IPV4_RE = re.compile(
    r"(?<![\d.])(?:(?:25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|1?\d?\d)(?![\d.])"
)
# loopback / bind-all / RFC5737 documentation ranges / QEMU slirp host alias are not sensitive
_IPV4_ALLOWED_RE = re.compile(
    r"^(?:127\.|0\.0\.0\.0$|10\.0\.2\.2$|192\.0\.2\.|198\.51\.100\.|203\.0\.113\.)"
)
# RFC5737 documentation ranges (each a /24) — used to reject a doc-range base
# carrying a CIDR mask that escapes its reserved /24 (e.g. 192.0.2.0/8).
_RFC5737_RE = re.compile(r"^(?:192\.0\.2\.|198\.51\.100\.|203\.0\.113\.)")
# IPv6 literals — comprehensive: full 8-group, or any "::" compression anywhere in
# the token (including mid-address). Requires 8 groups (7 colons) or a "::", so a
# HH:MM:SS time (2 colons, no "::") is NOT matched. A boundary guard fails closed.
_IPV6_RE = re.compile(
    r"(?<![:.\w])(?:"
    r"(?:[A-Fa-f0-9]{1,4}:){7}[A-Fa-f0-9]{1,4}"
    r"|(?:[A-Fa-f0-9]{1,4}:){1,7}:"
    r"|(?:[A-Fa-f0-9]{1,4}:){1,6}:[A-Fa-f0-9]{1,4}"
    r"|(?:[A-Fa-f0-9]{1,4}:){1,5}(?::[A-Fa-f0-9]{1,4}){1,2}"
    r"|(?:[A-Fa-f0-9]{1,4}:){1,4}(?::[A-Fa-f0-9]{1,4}){1,3}"
    r"|(?:[A-Fa-f0-9]{1,4}:){1,3}(?::[A-Fa-f0-9]{1,4}){1,4}"
    r"|(?:[A-Fa-f0-9]{1,4}:){1,2}(?::[A-Fa-f0-9]{1,4}){1,5}"
    r"|[A-Fa-f0-9]{1,4}:(?::[A-Fa-f0-9]{1,4}){1,6}"
    r"|:(?::[A-Fa-f0-9]{1,4}){1,7}"
    r")(?![:.\w])"
)
# ::1 loopback / :: unspecified / RFC3849 documentation range 2001:db8::/32
_IPV6_ALLOWED_RE = re.compile(r"^(?:::1$|2001:0?db8:)", re.IGNORECASE)
_PRIVATE_HOST_RE = re.compile(
    r"[A-Za-z0-9_-]+\.(?:lan|local|internal|home|homelab|lab)\b", re.IGNORECASE
)

warnings: list[str] = []
errors: list[str] = []


def warn(msg: str) -> None:
    warnings.append(msg)
    print(f"  !!  {msg}")


def err(msg: str) -> None:
    errors.append(msg)
    print(f"  FAIL  {msg}")


def ok(msg: str) -> None:
    print(f"  ok  {msg}")


def scan_public_map_boundary(root: Path) -> None:
    """HARD-fail if any public machine-readable map carries a concrete host
    address or private hostname. The public icn repo must not (docs/ATLAS.md §5;
    icn-infra ADR-0005). Never echoes the offending value."""
    for rel in PUBLIC_MAP_FILES:
        try:
            text = (root / rel).read_text(encoding="utf-8")
        except FileNotFoundError:
            continue  # not every checkout carries every map
        except (OSError, UnicodeError) as e:
            err(
                f"{rel}: present but unreadable/undecodable ({type(e).__name__}) — "
                f"cannot boundary-scan a public map; failing closed."
            )
            continue
        for m in _IPV4_RE.finditer(text):
            if not _IPV4_ALLOWED_RE.match(m.group(0)):
                err(
                    f"{rel}: contains a concrete IPv4 host address — the public icn "
                    f"repo must not (docs/ATLAS.md §5; icn-infra ADR-0005). Use "
                    f"role-level/symbolic refs; concrete values live in the private "
                    f"network-ops repo. (value withheld)"
                )
                break
        for m in _IPV6_RE.finditer(text):
            if not _IPV6_ALLOWED_RE.match(m.group(0)):
                err(
                    f"{rel}: contains a concrete IPv6 host address — the public icn "
                    f"repo must not (docs/ATLAS.md §5; icn-infra ADR-0005). Use "
                    f"role-level/symbolic refs. (value withheld)"
                )
                break
        if _PRIVATE_HOST_RE.search(text):
            err(
                f"{rel}: contains a private host name — the public icn repo must not "
                f"(docs/ATLAS.md §5). Use role-level/symbolic refs. (value withheld)"
            )


# ---------------------------------------------------------------------------
# Public docs/agent/test address boundary guard (icn#2393). The non-runtime
# public surfaces cleaned in the #2393 docs slice must never carry a concrete
# provider IPv4/IPv6 host address. Reuses the #2392 IPv4 regex/allowlist; the
# IPv6 rule: bracketed [..], >=3 hextets, or short 2-hextet forms that parse
# into a ULA / link-local range (fc00::/7, fe80::/10) are flagged; hex-looking
# global-range identifiers (Rust path segments, short capability strings) are
# NOT false-flagged. HARD failure, value always withheld. Hostname detection is
# intentionally omitted here — on prose it false-positives on filenames like
# `.env.local` and illustrative `*.internal`/`*.example` config; the §5
# provider concern on these surfaces is IP literals.
_PUBLIC_DOCS_DIRS = (                                 # scanned recursively for *.md
    "docs",
    ".claude",
    ".agents",
    ".github/agents",  # public agent definitions (icn#2393 slice 2)
)
_PUBLIC_DOCS_EXTRA = (                                # specific non-.md cleaned surfaces
    "CHANGELOG.md",
    "web/pilot-ui/tests/steward-gateway-url.test.js",
    # Shipped SDIS gateway source cleaned in icn#2393 slice 2: its gateway-url
    # test fixture must stay a documentation-range host, never provider topology.
    "icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs",
)
# Deferred to a separate targeted review (NOT ATLAS §5 provider topology, so
# excluded to keep this guard free of false positives): IPv6 address-TYPE
# illustrations (ULA / global / link-local / CGN format examples in
# protocol-design docs) and Rust-syntax / capability-action-string matches.
_PUBLIC_DOCS_EXCLUDE = frozenset({
    "docs/adr/ADR-0003-ipv6-dual-stack-transport-with-endpoint-sets.md",
    "docs/architecture/ARCHITECTURE_MAP.md",
    "docs/design/ipv6-endpoint-sets-design.md",
    "docs/features/ACTION_ITEMS_EXCHANGE.md",
    "docs/plans/2026-02-23-authz-capability-graph-design.md",
    "docs/plans/2026-02-23-authz-capability-graph-impl-plan.md",
})


def docs_address_violations(text: str) -> list[tuple[int, str]]:
    """Return [(line_no, category)] for disallowed concrete host literals in a
    cleaned public-docs surface. Never includes the value. Category is
    'ipv4-host' or 'ipv6-host'. Allowed: loopback / bind-all / QEMU slirp alias
    / RFC5737 (v4); ::1 / :: / RFC3849 2001:db8::/32 (v6). IPv6 is flagged when
    bracketed ([..]), written with >=3 hextets, OR (for short 2-hextet forms)
    when it parses into a ULA / link-local range (fc00::/7, fe80::/10) — so real
    provider IPv6 hosts like `fd00::1` are caught while hex-looking global-range
    identifiers (Rust path segments, short capability strings such as `dead:beef::`)
    are not false-flagged."""
    out: list[tuple[int, str]] = []
    for i, line in enumerate(text.splitlines(), 1):
        for m in _IPV4_RE.finditer(line):
            tok = m.group(0)
            if not _IPV4_ALLOWED_RE.match(tok):
                out.append((i, "ipv4-host"))
                break
            # A permitted RFC5737 base with a CIDR mask that escapes its reserved
            # /24 (e.g. 192.0.2.0/8) is NOT a valid documentation range — reject
            # it so an over-broad or policy-changing subnet cannot slip through.
            if _RFC5737_RE.match(tok):
                mm = re.match(r"/(\d{1,3})", line[m.end():])
                if mm and int(mm.group(1)) < 24:
                    out.append((i, "ipv4-doc-mask"))
                    break
        for m in _IPV6_RE.finditer(line):
            tok = m.group(0)
            if _IPV6_ALLOWED_RE.match(tok):
                # RFC3849 2001:db8::/32 with a mask escaping /32 is likewise invalid.
                if tok.lower().startswith("2001:"):
                    mm = re.match(r"/(\d{1,3})", line[m.end():])
                    if mm and int(mm.group(1)) < 32:
                        out.append((i, "ipv6-doc-mask"))
                        break
                continue
            s, e = m.start(), m.end()
            bracketed = s > 0 and line[s - 1] == "[" and e < len(line) and line[e] == "]"
            hextets = [g for g in tok.split(":") if g]
            flag = bracketed or len(hextets) >= 3
            if not flag:
                # short (<=2 hextet) compressed forms: flag only if the token
                # actually parses into a ULA / link-local address range so that
                # hex-looking global-range identifiers are not false-flagged.
                try:
                    a = ipaddress.IPv6Address(tok.strip("[]"))
                    flag = a.is_private or a.is_link_local
                except ValueError:
                    flag = False
            if flag:
                out.append((i, "ipv6-host"))
                break
    return out


def scan_public_docs_boundary(root: Path) -> None:
    """HARD-fail if a cleaned public docs/agent/test surface (icn#2393 slice)
    carries a concrete provider IPv4/IPv6 host address — public icn must not
    (docs/ATLAS.md §5; icn-infra ADR-0005). Never echoes the offending value;
    fails closed on unreadable/undecodable surfaces."""
    seen: set[str] = set()
    surfaces: list[tuple[str, Path]] = []
    for d in _PUBLIC_DOCS_DIRS:
        base = root / d
        if not base.is_dir():
            continue
        for p in sorted(base.rglob("*.md")):
            rel = p.relative_to(root).as_posix()
            if rel in _PUBLIC_DOCS_EXCLUDE or rel in seen:
                continue
            seen.add(rel)
            surfaces.append((rel, p))
    for extra in _PUBLIC_DOCS_EXTRA:
        p = root / extra
        if p.is_file() and extra not in _PUBLIC_DOCS_EXCLUDE and extra not in seen:
            seen.add(extra)
            surfaces.append((extra, p))
    for rel, p in surfaces:
        try:
            text = p.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as e:
            err(
                f"{rel}: present but unreadable/undecodable ({type(e).__name__}) — "
                f"cannot boundary-scan a public docs surface; failing closed."
            )
            continue
        for ln, cat in docs_address_violations(text):
            err(
                f"{rel}:{ln}: contains a concrete {cat} address — public icn docs "
                f"must not (docs/ATLAS.md §5; icn#2393). Use ${{ICN_*}} env vars / "
                f"symbolic roles / RFC5737 / RFC3849 examples; concrete values live "
                f"in the private network-ops repo. (value withheld)"
            )


# ---------------------------------------------------------------------------
# SessionStart source-existence guard (icn#2634). An unconditional startup hook
# runs before any agent reasoning, so a path it names is an orientation claim the
# agent cannot check. Before this guard, `.claude/settings.json` grepped two
# planning files that were absent from the repo (one had NEVER existed in its
# history) and the session banner emitted an empty current-work claim for months.
#
# Rule: every repo-relative file a SessionStart command names must exist. The
# same rule is applied one level deep, to path literals inside the hook scripts
# those commands invoke — otherwise the guard is defeated by moving the dead path
# from the JSON into the script it calls.
SETTINGS_FILE = ".claude/settings.json"
_PATH_TOKEN_RE = re.compile(r"[\w./-]*\.(?:md|json|sh|py|ya?ml|toml|txt|conf)\b")
_PROJECT_DIR_FORMS = (
    '"$CLAUDE_PROJECT_DIR"', "'$CLAUDE_PROJECT_DIR'",
    "${CLAUDE_PROJECT_DIR}", "$CLAUDE_PROJECT_DIR",
)


def repo_relative_paths(text: str, root: Path) -> list[str]:
    """Extract repo-relative file paths named in a shell command or script.

    Scoped deliberately: a token counts only when its first segment is an
    existing top-level entry of the repo. That catches the real recurrence case
    (`docs/strategy/ICN-Active-Sprint.md`) while never mistaking a system path
    (`/tmp/...`, `/usr/bin/...`) for a missing repo file."""
    for form in _PROJECT_DIR_FORMS:
        text = text.replace(form, "")
    out: list[str] = []
    for m in _PATH_TOKEN_RE.finditer(text):
        tok = m.group(0).lstrip("/").strip()
        if not tok or "/" not in tok or "$" in tok or "*" in tok or "?" in tok:
            continue
        first = tok.split("/", 1)[0]
        if not first or not (root / first).exists():
            continue
        if tok not in out:
            out.append(tok)
    return out


def scan_sessionstart_sources(root: Path) -> None:
    """HARD-fail if a SessionStart hook names a repo-relative file that does not
    exist. Fails closed on an unreadable/unparseable settings file: a startup
    surface that cannot be inspected must not be assumed healthy."""
    settings_path = root / SETTINGS_FILE
    if not settings_path.is_file():
        return  # not every checkout carries provider settings
    try:
        settings = json.loads(settings_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as e:
        err(
            f"{SETTINGS_FILE}: present but unreadable/unparseable "
            f"({type(e).__name__}) — cannot verify SessionStart sources; failing closed."
        )
        return

    groups = ((settings.get("hooks") or {}).get("SessionStart")) or []
    commands: list[str] = []
    for group in groups:
        for hook in (group or {}).get("hooks") or []:
            cmd = hook.get("command")
            if isinstance(cmd, str):
                commands.append(cmd)
    if not commands:
        ok("SessionStart: no hook commands to verify")
        return

    checked = 0
    for cmd in commands:
        for rel in repo_relative_paths(cmd, root):
            checked += 1
            if not (root / rel).exists():
                err(
                    f"{SETTINGS_FILE}: SessionStart hook names {rel!r}, which does "
                    f"not exist. An unconditional startup surface must not point at "
                    f"a missing file or synthesise current state from one (icn#2634)."
                )
                continue
            # One level deep: path literals inside an invoked hook script.
            script = root / rel
            if script.suffix not in (".sh", ".py") or not script.is_file():
                continue
            try:
                body = script.read_text(encoding="utf-8")
            except (OSError, UnicodeError) as e:
                err(
                    f"{rel}: SessionStart hook script is unreadable "
                    f"({type(e).__name__}) — failing closed."
                )
                continue
            for inner in repo_relative_paths(body, root):
                checked += 1
                if not (root / inner).exists():
                    err(
                        f"{rel}: SessionStart hook script names {inner!r}, which "
                        f"does not exist (icn#2634)."
                    )
    ok(f"SessionStart: {checked} repo-relative source path(s) verified present")


def parse_date(value: str) -> datetime.date | None:
    try:
        return datetime.date.fromisoformat(str(value)[:10])
    except ValueError:
        return None


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--repo-root", default=".", help="repo root to validate (default: .)")
    ap.add_argument("--strict", action="store_true", help="exit 1 if any warnings (ratchet mode)")
    args = ap.parse_args()
    root = Path(args.repo_root)

    print("check-truth-spine")

    sources_path = root / "ops/state/truth/sources.json"
    try:
        sources = json.loads(sources_path.read_text())
    except (OSError, json.JSONDecodeError) as e:
        print(f"  FAIL  cannot read/parse {sources_path}: {e}")
        return 1

    domains = sources.get("domains", {})
    today = datetime.date.today()

    # 1. Every file-owner must exist; machine_view files must exist and parse.
    seen_owners: dict[str, list[tuple[str, frozenset[str]]]] = {}
    for name, dom in domains.items():
        owner = dom.get("owner", "")
        # Classify the owner once; only genuine path owners get existence,
        # duplicate, and staleness treatment. Blank/malformed owners must not
        # resolve to the repo root and silently "exist".
        is_path_owner = False
        if not owner.strip():
            warn(f"{name}: blank owner — every domain must name its source")
        elif owner in NONFILE_OWNERS:
            ok(f"{name}: non-file owner ({owner!r}) — skipped existence check")
        elif " " in owner:
            ok(f"{name}: descriptive (non-path) owner — skipped existence check")
        else:
            is_path_owner = True
            owner_path = root / owner.split("#", 1)[0]
            if not owner_path.exists():
                warn(f"{name}: owner path missing on disk: {owner}")
            else:
                ok(f"{name}: owner exists ({owner})")

        # Duplicate-owner rule applies to path owners only: live-query and
        # descriptive owners legitimately serve multiple domains, and blank
        # owners were already warned above.
        #
        # The invariant is one owner per CLAIM, not one domain per file. Two
        # domains may share a file when both scope themselves to `sections` and
        # those scopes are disjoint. A registration with no `sections` claims the
        # whole file and therefore collides with any sibling on that path, and
        # overlapping section scopes collide too — neither case was expressible
        # while the rule keyed on the path alone.
        if is_path_owner:
            # Key on the base path, never the raw owner string: `f` and
            # `f#infrastructure` are the same file, and keying on the raw string
            # would hide that collision behind the fragment. A `#fragment` is
            # itself a scope, so fold it into the declared sections.
            base, _, fragment = owner.partition("#")
            sections = frozenset(dom.get("sections") or ()) | (
                {fragment} if fragment else set()
            )
            for other_name, other_sections in seen_owners.get(base, []):
                if not sections or not other_sections:
                    warn(
                        f"duplicate owner: {name} and {other_name} both claim {base!r} "
                        "and at least one claims the whole file "
                        "(one source per claim — sources.json's own rule)"
                    )
                elif sections & other_sections:
                    warn(
                        f"overlapping owner: {name} and {other_name} both claim "
                        f"{base!r} sections {sorted(sections & other_sections)} "
                        "(one source per claim — sources.json's own rule)"
                    )
            seen_owners.setdefault(base, []).append((name, sections))

        mv = dom.get("machine_view")
        if mv:
            mv_path = root / mv
            if not mv_path.exists():
                warn(f"{name}: machine_view missing: {mv}")
            elif mv_path.suffix == ".json":
                try:
                    json.loads(mv_path.read_text())
                except json.JSONDecodeError as e:
                    warn(f"{name}: machine_view unparseable: {mv} ({e})")

        # 2. Currency semantics + staleness, for path owners that are JSON.
        stability = dom.get("stability", "")
        threshold = STALENESS_DAYS.get(stability)
        dormant = False
        if is_path_owner:
            owner_file = root / owner.split("#", 1)[0]
            if owner_file.is_file() and owner_file.suffix == ".json":
                try:
                    data = json.loads(owner_file.read_text())
                except json.JSONDecodeError:
                    data = None

                # 2a. A volatile domain must not present a TERMINAL record as its
                # current answer (icn#2634). Declaring dormancy is the honest way
                # to say "nothing is active"; silently serving a closed object is
                # not. HARD failure — this is what let a closed March sprint stand
                # as the registered answer to "what is being worked on" for months.
                if stability == "volatile" and isinstance(data, dict):
                    state = str(data.get("status", "")).strip().lower()
                    dormant = declares_dormancy(data)
                    if state in TERMINAL_STATES and not dormant:
                        err(
                            f"{name}: volatile domain owner {owner} has "
                            f"status={state!r} (terminal) but does not declare "
                            f"dormancy — a closed/expired record must not stand as "
                            f"a volatile domain's current answer. Either register "
                            f"the live object, or declare dormancy explicitly "
                            f"(e.g. cadence: \"dormant\" / active_*: null). "
                            f"(icn#2634)"
                        )
                    elif dormant:
                        ok(f"{name}: volatile owner declares dormancy explicitly")

                # 2b. Staleness applies ONLY to an owner claiming something is
                # active. Dating a record that truthfully reports "nothing is
                # running" is calendar superstition, and nagging it invites the
                # exact bad fix (opening a sprint that does not exist).
                if threshold and not dormant and isinstance(data, dict):
                    for key in DATE_KEYS:
                        if key not in data:
                            continue
                        d = parse_date(data[key])
                        if d is None:
                            warn(
                                f"{name}: {owner} {key}={data[key]!r} is not an "
                                "ISO date — trying the next date key"
                            )
                            continue
                        if (today - d).days > threshold:
                            warn(
                                f"{name}: {owner} {key}={data[key]} is "
                                f"{(today - d).days}d old (> {threshold}d for "
                                f"{dom.get('stability')})"
                            )
                        break

    # 3. Ecosystem index vs org-repo registry consistency.
    eco_path = root / "ops/state/ecosystem.json"
    map_path = root / "ops/state/config/repo-map.json"
    try:
        eco = json.loads(eco_path.read_text())
        repo_map = json.loads(map_path.read_text())
    except (OSError, json.JSONDecodeError) as e:
        warn(f"cannot cross-check ecosystem vs registry: {e}")
        eco, repo_map = {}, {}

    eco_repos = eco.get("repos", {})
    org_section = repo_map.get("org_repos") or {}
    org_repos = (org_section.get("repos") or {}) if isinstance(org_section, dict) else {}
    if eco_repos:
        # icn is "this repo" in ecosystem.json; homelab-inventory lives in #repos.
        expected = set(eco_repos) - {"icn", "homelab-inventory"}
        if not org_repos:
            # The registry this validator exists to protect has disappeared —
            # that must be a warning, never a silent skip.
            if expected:
                warn(
                    f"repo-map.json#org_repos is missing/empty while ecosystem.json "
                    f"names {len(expected)} downstream repos — coverage check cannot run"
                )
        else:
            missing = expected - set(org_repos)
            for r in sorted(missing):
                warn(f"ecosystem.json names repo {r!r} but repo-map.json#org_repos does not register it")
            if not missing:
                ok(f"registry covers all {len(expected)} ecosystem repos (extras allowed)")

            for r in sorted(expected & set(org_repos)):
                ev = eco_repos[r].get("visibility")
                rv = org_repos[r].get("visibility")
                if ev and rv and ev != rv:
                    warn(f"visibility disagrees for {r}: ecosystem.json={ev} registry={rv}")

    # 4. Public-map boundary guard — HARD fail regardless of --strict.
    scan_public_map_boundary(root)

    # 5. Public docs/agent/test address boundary guard (icn#2393) — HARD fail.
    scan_public_docs_boundary(root)

    # 6. SessionStart source-existence guard (icn#2634) — HARD fail.
    scan_sessionstart_sources(root)

    # Result
    if errors:
        print(
            f"check-truth-spine: {len(errors)} hard error(s) — FAIL "
            f"(public maps/docs must not carry concrete host addresses or "
            f"operational values; a volatile domain must not serve a terminal "
            f"record as current; SessionStart must not name a missing file)"
        )
        return 1
    if warnings:
        print(f"check-truth-spine: {len(warnings)} warning(s)")
        return 1 if args.strict else 0
    print("check-truth-spine: clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
