#!/usr/bin/env python3
"""Generate a complete tracked-file record for one or more repositories.

This script is intentionally mechanical. It produces a reproducible inventory from
`git ls-files` and filesystem metadata, then writes JSON and Markdown artifacts
that can be reviewed, enriched, and cited by the living repo atlas.

Example:

    python3 scripts/generate_repo_record.py \
      --repo icn=. \
      --repo nycn=../nycn \
      --repo icn-learn=../icn-learn \
      --out docs/reference/project-index/generated

The script records tracked files only by default. Use --include-untracked for a
local archaeology pass, but do not commit private or generated local material
without review.
"""

from __future__ import annotations

import argparse
import contextlib
import datetime as dt
import hashlib
import json
import subprocess
import sys
import tempfile
from collections import Counter, defaultdict
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable

# datetime.timezone.utc works on every supported Python; dt.UTC is 3.11+.
UTC = dt.timezone.utc

LANGUAGE_BY_EXTENSION = {
    ".astro": "Astro",
    ".css": "CSS",
    ".csv": "CSV",
    ".html": "HTML",
    ".js": "JavaScript",
    ".json": "JSON",
    ".jsx": "JavaScript/React",
    ".md": "Markdown",
    ".mjs": "JavaScript",
    ".py": "Python",
    ".rs": "Rust",
    ".sh": "Shell",
    ".sql": "SQL",
    ".toml": "TOML",
    ".ts": "TypeScript",
    ".tsx": "TypeScript/React",
    ".txt": "Text",
    ".yaml": "YAML",
    ".yml": "YAML",
}

ROLE_RULES: list[tuple[str, str]] = [
    ("docs/adr/", "architecture decision record"),
    ("docs/rfcs/", "request for comments"),
    ("docs/reference/project-index/", "project index / atlas"),
    ("docs/", "documentation"),
    ("icn/crates/", "Rust library crate"),
    ("icn/apps/", "runtime app crate"),
    ("icn/bins/", "Rust binary"),
    ("apps/", "legacy/top-level app"),
    ("website/", "public website"),
    ("web/pilot-ui/", "pilot UI"),
    ("web/dashboard/", "dashboard UI"),
    ("sdk/typescript/", "TypeScript SDK"),
    ("sdk/react-native/", "React Native SDK"),
    ("deploy/", "deployment"),
    ("ops/", "operations / coordination"),
    ("monitoring/", "monitoring"),
    ("institutions/", "institution package"),
    ("contracts/", "contract template"),
    ("demo/", "demo"),
    ("scripts/", "script"),
    (".github/workflows/", "GitHub Actions workflow"),
    (".github/agents/", "agent definition"),
]


@dataclass(frozen=True)
class FileRecord:
    path: str
    directory: str
    name: str
    extension: str
    language: str
    role_guess: str
    size_bytes: int
    sha256: str
    tracked: bool
    kind: str  # "file", "symlink", or "broken-symlink"
    symlink_target: str | None = None


@dataclass(frozen=True)
class DirectoryRecord:
    path: str
    depth: int
    file_count: int
    total_size_bytes: int
    child_directory_count: int
    extensions: dict[str, int]
    role_guess: str


def run(repo: Path, args: list[str]) -> str:
    completed = subprocess.run(
        args,
        cwd=repo,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=False,
    )
    return completed.stdout.decode("utf-8", errors="replace")


def git_head(repo: Path) -> str:
    return run(repo, ["git", "rev-parse", "HEAD"]).strip()


def git_branch(repo: Path) -> str:
    return run(repo, ["git", "branch", "--show-current"]).strip() or "(detached)"


def git_ls_files(repo: Path) -> list[str]:
    raw = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=repo,
        check=True,
        stdout=subprocess.PIPE,
    ).stdout
    return [part.decode("utf-8", errors="replace") for part in raw.split(b"\0") if part]


def git_untracked(repo: Path) -> list[str]:
    raw = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard", "-z"],
        cwd=repo,
        check=True,
        stdout=subprocess.PIPE,
    ).stdout
    return [part.decode("utf-8", errors="replace") for part in raw.split(b"\0") if part]


def git_working_tree_dirty(repo: Path) -> bool:
    """Return True if the working tree has unstaged or staged changes.

    Uses ``git status --porcelain --untracked-files=no``. Untracked files do
    not count as "dirty" here; their inventory is gated separately by
    ``--include-untracked``.

    The clean/dirty signal is necessary but not sufficient for blob-equality
    with HEAD: file SHAs in this generator come from working-tree bytes (see
    ``sha256_file``), and ``.gitattributes`` filters (CRLF normalization,
    smudge filters, ident expansion) can make working-tree bytes differ from
    HEAD blob bytes even when the porcelain status is clean. The dirty check
    catches the obvious audit hazard (uncommitted local edits silently mixed
    into the snapshot); a stricter "matches HEAD blobs" signal would require
    hashing blobs from Git objects, which is a separate follow-up.
    """

    raw = subprocess.run(
        ["git", "status", "--porcelain", "--untracked-files=no"],
        cwd=repo,
        check=True,
        stdout=subprocess.PIPE,
    ).stdout
    return bool(raw.strip())


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def role_guess(path: str) -> str:
    for prefix, role in ROLE_RULES:
        if path.startswith(prefix):
            return role
    if "/tests/" in path or path.endswith("_test.rs") or path.endswith(".test.ts"):
        return "test"
    if path in {"README.md", "AGENTS.md", "CONTRIBUTING.md", "CLAUDE.md"}:
        return "repo control document"
    if path.startswith("."):
        return "repo configuration"
    return "uncategorized"


def language_for(path: str) -> str:
    suffix = Path(path).suffix.lower()
    return LANGUAGE_BY_EXTENSION.get(suffix, "unknown")


def classify_extension(name: str) -> str:
    """Return a classification token for a file's extension.

    Uses ``Path.suffix`` for ordinary files; for dotfiles like ``.env`` or
    ``.gitignore`` (where ``Path('.env').suffix`` is empty), returns the full
    name (``.env``) so dotfiles retain a meaningful extension classification
    instead of being lumped into ``(none)``.
    """

    suffix = Path(name).suffix.lower()
    if suffix:
        return suffix
    if name.startswith(".") and "." not in name[1:]:
        return name.lower()
    return "(none)"


def make_file_record(repo: Path, rel: str, tracked: bool) -> FileRecord | None:
    full = repo / rel
    # Use lstat so we can inventory symlinks (including dangling ones) without
    # following them. Codex P1: ``git ls-files`` returns tracked symlinks; the
    # generator must not silently drop them.
    try:
        lst = full.lstat()
    except FileNotFoundError:
        return None

    is_symlink = full.is_symlink()
    if is_symlink:
        try:
            target = str(full.readlink())
        except OSError:
            target = None
        kind = "symlink" if full.exists() else "broken-symlink"
        # Don't traverse into the target; record the link itself with size 0
        # and a sha256 over the link target text so the record stays
        # deterministic without depending on whether the target resolves.
        link_text = (target or "").encode("utf-8", errors="replace")
        sha = hashlib.sha256(link_text).hexdigest()
        size = lst.st_size
    elif not full.is_file():
        # Tracked path that resolves to neither a regular file nor a symlink
        # (e.g. submodule gitlink). Skip; ``git ls-files`` returning these is
        # rare and the record schema is regular-file-shaped.
        return None
    else:
        target = None
        kind = "file"
        sha = sha256_file(full)
        size = lst.st_size

    return FileRecord(
        path=rel,
        directory=str(Path(rel).parent) if str(Path(rel).parent) != "." else "",
        name=full.name,
        extension=classify_extension(full.name),
        language=language_for(rel),
        role_guess=role_guess(rel),
        size_bytes=size,
        sha256=sha,
        tracked=tracked,
        kind=kind,
        symlink_target=target,
    )


def parent_dirs(path: str) -> Iterable[str]:
    parent = Path(path).parent
    parts = parent.parts
    if not parts or parts == (".",):
        yield ""
        return
    yield ""
    acc: list[str] = []
    for part in parts:
        acc.append(part)
        yield "/".join(acc)


def build_directories(files: list[FileRecord]) -> list[DirectoryRecord]:
    dir_files: dict[str, list[FileRecord]] = defaultdict(list)
    child_dirs: dict[str, set[str]] = defaultdict(set)

    for file in files:
        for directory in parent_dirs(file.path):
            dir_files[directory].append(file)
        parent = file.directory
        while True:
            parent_path = Path(parent)
            if str(parent_path) in {".", ""}:
                break
            child = str(parent_path)
            grandparent = str(parent_path.parent)
            if grandparent == ".":
                grandparent = ""
            child_dirs[grandparent].add(child)
            if grandparent == "":
                break
            parent = grandparent

    records: list[DirectoryRecord] = []
    for directory, members in sorted(dir_files.items(), key=lambda item: (item[0].count("/"), item[0])):
        ext_counts = Counter(file.extension for file in members)
        records.append(
            DirectoryRecord(
                path=directory or ".",
                depth=0 if directory == "" else directory.count("/") + 1,
                file_count=len(members),
                total_size_bytes=sum(file.size_bytes for file in members),
                child_directory_count=len(child_dirs.get(directory, set())),
                extensions=dict(sorted(ext_counts.items())),
                role_guess=role_guess(f"{directory}/") if directory else "repo root",
            )
        )
    return records


def write_markdown(
    out_path: Path,
    repo_name: str,
    head: str,
    branch: str,
    files: list[FileRecord],
    directories: list[DirectoryRecord],
    working_tree_dirty: bool,
) -> None:
    extension_counts = Counter(file.extension for file in files)
    role_counts = Counter(file.role_guess for file in files)
    total_size = sum(file.size_bytes for file in files)

    lines: list[str] = []
    lines.append("---")
    lines.append("Status: generated")
    lines.append("Canonical: no")
    lines.append(f"Generated: {dt.datetime.now(UTC).isoformat()}")
    lines.append("---")
    lines.append("")
    lines.append(f"# Full Repository Record — `{repo_name}`")
    lines.append("")
    lines.append("> Generated mechanically from `git ls-files` and filesystem metadata. Do not hand-edit generated sections; rerun `scripts/generate_repo_record.py`.")
    lines.append("")
    lines.append("## Snapshot")
    lines.append("")
    # Intentionally no absolute repo path: the repo identity is the
    # user-supplied repo name plus branch + HEAD. Persisting an absolute
    # local checkout path would leak machine-specific filesystem details
    # (usernames, home paths) into committed artifacts.
    tracked_count = sum(1 for file in files if file.tracked)
    tracked_size = sum(file.size_bytes for file in files if file.tracked)
    untracked_count = len(files) - tracked_count
    untracked_size = total_size - tracked_size

    lines.append(f"- Repo: `{repo_name}`")
    lines.append(f"- Branch: `{branch}`")
    lines.append(f"- HEAD: `{head}`")
    # File SHAs are computed from working-tree bytes (see
    # `sha256_file`). The clean/dirty signal here reflects
    # `git status --porcelain --untracked-files=no` only — it does
    # NOT guarantee SHAs match the corresponding HEAD blob bytes,
    # because `.gitattributes` filters (CRLF normalization, smudge
    # filters, ident expansion) can make working-tree bytes diverge
    # from blob bytes even on a fresh clean checkout. Hashing from
    # Git objects is a separate follow-up if a stricter audit
    # signal is needed.
    if working_tree_dirty:
        lines.append(
            "- Working tree: `dirty (uncommitted changes against HEAD)`"
        )
    else:
        lines.append(
            "- Working tree: `clean (no uncommitted changes against HEAD)`"
        )
    lines.append(
        "- SHA source: `working tree bytes (may differ from HEAD blob "
        "bytes when .gitattributes filters apply, e.g. CRLF normalization)`"
    )
    # "Recorded" covers both tracked and untracked entries when
    # --include-untracked is supplied. Tracked-only counters are emitted
    # separately so audit consumers can filter without re-deriving from
    # the per-file array.
    lines.append(f"- Files recorded: `{len(files)}`")
    lines.append(f"- Tracked files recorded: `{tracked_count}`")
    if untracked_count:
        lines.append(f"- Untracked files recorded: `{untracked_count}`")
    lines.append(f"- Directories recorded: `{len(directories)}`")
    lines.append(f"- Total recorded bytes: `{total_size}`")
    lines.append(f"- Total tracked bytes: `{tracked_size}`")
    if untracked_size:
        lines.append(f"- Total untracked bytes: `{untracked_size}`")
    kind_counts = Counter(file.kind for file in files)
    if kind_counts:
        kind_summary = ", ".join(
            f"{kind}: {count}" for kind, count in sorted(kind_counts.items())
        )
        lines.append(f"- Entry kinds: `{kind_summary}`")
    lines.append("")
    lines.append("## Role summary")
    lines.append("")
    lines.append("| Role guess | Files |")
    lines.append("|---|---:|")
    for role, count in sorted(role_counts.items(), key=lambda item: (-item[1], item[0])):
        lines.append(f"| {role} | {count} |")
    lines.append("")
    lines.append("## Extension summary")
    lines.append("")
    lines.append("| Extension | Files |")
    lines.append("|---|---:|")
    for ext, count in sorted(extension_counts.items(), key=lambda item: (-item[1], item[0])):
        lines.append(f"| `{ext}` | {count} |")
    lines.append("")
    lines.append("## Directory record")
    lines.append("")
    lines.append("| Directory | Depth | Files under directory | Child dirs | Size bytes | Role guess |")
    lines.append("|---|---:|---:|---:|---:|---|")
    for directory in directories:
        lines.append(
            f"| `{directory.path}` | {directory.depth} | {directory.file_count} | "
            f"{directory.child_directory_count} | {directory.total_size_bytes} | {directory.role_guess} |"
        )
    lines.append("")
    lines.append("## File record")
    lines.append("")
    lines.append("| Path | Size | SHA-256 | Language | Role guess |")
    lines.append("|---|---:|---|---|---|")
    for file in sorted(files, key=lambda item: item.path):
        lines.append(
            f"| `{file.path}` | {file.size_bytes} | `{file.sha256}` | "
            f"{file.language} | {file.role_guess} |"
        )
    lines.append("")
    out_path.write_text("\n".join(lines), encoding="utf-8")


def generate_for_repo(
    repo_name: str,
    repo_path: Path,
    out_dir: Path,
    include_untracked: bool,
    allow_dirty: bool,
) -> None:
    # Resolve internally so subprocess calls can run against the repo, but the
    # resolved absolute path is intentionally NOT persisted in any generated
    # artifact (Codex P2 / Copilot review): committed records would otherwise
    # carry machine-specific paths and usernames.
    resolved_path = repo_path.resolve()
    if not (resolved_path / ".git").exists():
        raise SystemExit(f"{repo_name}: {repo_path} does not look like a git repository")

    # SHAs are computed from the working tree; the snapshot also advertises
    # `head`. In a dirty checkout these two diverge silently and an audit
    # consumer could read the record as commit-scoped while it is actually
    # mixed commit + local state. Refuse by default unless --allow-dirty (or
    # --include-untracked, which already implies working-tree semantics) is
    # supplied.
    dirty = git_working_tree_dirty(resolved_path)
    if dirty and not (allow_dirty or include_untracked):
        raise SystemExit(
            f"{repo_name}: working tree is dirty (unstaged or staged changes against HEAD).\n"
            "  File SHAs are computed from the working tree; recording them alongside HEAD\n"
            "  would mix commit-scoped and local-scoped state. Refusing by default to avoid\n"
            "  misleading audit consumers.\n"
            "  Either commit/stash the changes, or rerun with --allow-dirty (or\n"
            "  --include-untracked, which already implies working-tree semantics)."
        )

    tracked_paths = git_ls_files(resolved_path)
    all_paths: list[tuple[str, bool]] = [(path, True) for path in tracked_paths]
    if include_untracked:
        all_paths.extend((path, False) for path in git_untracked(resolved_path))

    files: list[FileRecord] = []
    skipped: list[str] = []
    for path, tracked in all_paths:
        record = make_file_record(resolved_path, path, tracked)
        if record is None:
            skipped.append(path)
        else:
            files.append(record)
    directories = build_directories(files)
    head = git_head(resolved_path)
    branch = git_branch(resolved_path)

    out_dir.mkdir(parents=True, exist_ok=True)
    payload = {
        "schema": "icn.repo_record.v1",
        "generated_at": dt.datetime.now(UTC).isoformat(),
        "repo": repo_name,
        # No `repo_path`: see Codex P2 / Copilot review. Repo identity is
        # `repo` + `branch` + `head`. Add normalized fields here only if they
        # cannot leak local filesystem details.
        "branch": branch,
        "head": head,
        "include_untracked": include_untracked,
        # `working_tree_dirty` reports whether the porcelain status shows
        # unstaged or staged changes against HEAD. File SHAs are always
        # computed from working-tree bytes; a clean tree means the snapshot
        # has no uncommitted local edits, but does NOT guarantee SHAs equal
        # HEAD blob bytes (`.gitattributes` filters such as CRLF
        # normalization can introduce divergence even on a clean checkout).
        # See `git_working_tree_dirty` and the Markdown "SHA source" line.
        "working_tree_dirty": dirty,
        "summary": {
            # `file_count` and `total_size_bytes` count every record in the
            # `files` array (tracked + untracked when --include-untracked is
            # supplied). Tracked-only counters are emitted alongside so audit
            # consumers can filter without re-deriving from the per-file array.
            "file_count": len(files),
            "tracked_file_count": sum(1 for file in files if file.tracked),
            "untracked_file_count": sum(1 for file in files if not file.tracked),
            "directory_count": len(directories),
            "total_size_bytes": sum(file.size_bytes for file in files),
            "tracked_total_size_bytes": sum(file.size_bytes for file in files if file.tracked),
            "untracked_total_size_bytes": sum(file.size_bytes for file in files if not file.tracked),
            "extensions": dict(sorted(Counter(file.extension for file in files).items())),
            "roles": dict(sorted(Counter(file.role_guess for file in files).items())),
            "kinds": dict(sorted(Counter(file.kind for file in files).items())),
            "skipped_paths": sorted(skipped),
        },
        "directories": [asdict(record) for record in directories],
        "files": [asdict(record) for record in sorted(files, key=lambda item: item.path)],
    }

    json_path = out_dir / f"{repo_name}-file-record.json"
    md_path = out_dir / f"{repo_name}-file-record.md"
    json_path.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
    write_markdown(md_path, repo_name, head, branch, files, directories, dirty)

    print(f"wrote {json_path}")
    print(f"wrote {md_path}")


def parse_repo_arg(value: str) -> tuple[str, Path]:
    if "=" not in value:
        raise argparse.ArgumentTypeError("repo must be NAME=PATH")
    name, path = value.split("=", 1)
    if not name:
        raise argparse.ArgumentTypeError("repo name cannot be empty")
    return name, Path(path)


def _normalize_record(text: str, ext: str) -> str:
    """Strip incidental, non-content fields so --check compares the snapshot
    payload (file inventory, sizes, SHAs, head) rather than the generation
    timestamp or the branch the snapshot happened to be taken on.

    Normalized out:
      - JSON `generated_at` (changes every run) and `branch` (incidental: a
        refresh on any branch is equivalent).
      - Markdown `Generated:` front-matter line and the `- Branch:` line.
    `head` and the file inventory are intentionally KEPT — a head/inventory
    difference is real snapshot staleness, which is exactly what --check reports.
    """
    if ext == "json":
        data = json.loads(text)
        data.pop("generated_at", None)
        data["branch"] = "<normalized>"
        return json.dumps(data, indent=2, sort_keys=True)
    # markdown
    kept = [
        line
        for line in text.splitlines()
        if not line.startswith("Generated:") and not line.startswith("- Branch:")
    ]
    return "\n".join(kept)


def check_repos(
    repos: list[tuple[str, Path]],
    out_dir: Path,
    include_untracked: bool,
) -> int:
    """Regenerate each repo record into a temp dir and compare it against the
    committed artifact under `out_dir`, ignoring the volatile fields above.

    Returns 0 when every committed record matches the working tree, 1 on drift
    or a missing committed artifact. Never writes to `out_dir`.
    """
    drift = False
    with tempfile.TemporaryDirectory() as tmp:
        tmp_dir = Path(tmp)
        for repo_name, repo_path in repos:
            # allow_dirty=True: --check compares against the current working
            # tree (it must not refuse on a dirty checkout); it never persists
            # a misleading artifact because it only writes into the temp dir.
            with contextlib.redirect_stdout(sys.stderr):
                generate_for_repo(repo_name, repo_path, tmp_dir, include_untracked, True)
            for ext in ("json", "md"):
                committed = out_dir / f"{repo_name}-file-record.{ext}"
                fresh = tmp_dir / f"{repo_name}-file-record.{ext}"
                if not committed.exists():
                    print(f"DRIFT: committed artifact missing: {committed}")
                    drift = True
                    continue
                if _normalize_record(committed.read_text(encoding="utf-8"), ext) != _normalize_record(
                    fresh.read_text(encoding="utf-8"), ext
                ):
                    drift = True
                    print(f"DRIFT: {committed} differs from a fresh generation.")
                    if ext == "json":
                        c = json.loads(committed.read_text(encoding="utf-8"))
                        f = json.loads(fresh.read_text(encoding="utf-8"))
                        print(
                            f"  committed head={c.get('head', '?')[:12]} "
                            f"file_count={c.get('summary', {}).get('file_count')}"
                        )
                        print(
                            f"  working   head={f.get('head', '?')[:12]} "
                            f"file_count={f.get('summary', {}).get('file_count')}"
                        )
    if drift:
        out_disp = out_dir if str(out_dir) != "." else "docs/reference/project-index/generated"
        print(
            "\nRegenerate with:\n"
            f"  python3 scripts/generate_repo_record.py "
            f"--repo {repos[0][0]}={repos[0][1]} --out {out_disp}",
            file=sys.stderr,
        )
        return 1
    print("OK: repo file-record matches the working tree (timestamp/branch ignored).")
    return 0


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo",
        action="append",
        type=parse_repo_arg,
        required=True,
        help="Repository to record as NAME=PATH. May be supplied multiple times.",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("docs/reference/project-index/generated"),
        help="Output directory for generated JSON and Markdown records.",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help=(
            "Do not write. Regenerate each record into a temp dir and compare "
            "it against the committed artifact under --out, ignoring the "
            "generation timestamp and branch. Exit 0 if current, 1 on drift "
            "(stale snapshot) or a missing committed artifact."
        ),
    )
    parser.add_argument(
        "--include-untracked",
        action="store_true",
        help="Also record untracked, non-ignored local files. Review carefully before committing outputs.",
    )
    parser.add_argument(
        "--allow-dirty",
        action="store_true",
        help=(
            "Allow snapshot generation when the working tree has unstaged or "
            "staged changes against HEAD. The resulting record will set "
            "working_tree_dirty=true so audit consumers can see that the "
            "file SHAs reflect the working tree, not strictly the HEAD blobs."
        ),
    )
    args = parser.parse_args()

    if args.check:
        raise SystemExit(check_repos(args.repo, args.out, args.include_untracked))

    for repo_name, repo_path in args.repo:
        generate_for_repo(
            repo_name,
            repo_path,
            args.out,
            args.include_untracked,
            args.allow_dirty,
        )


if __name__ == "__main__":
    main()
