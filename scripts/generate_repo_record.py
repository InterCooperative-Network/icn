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
import datetime as dt
import hashlib
import json
import os
import subprocess
from collections import Counter, defaultdict
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable


TEXT_EXTENSIONS = {
    ".adoc",
    ".astro",
    ".css",
    ".csv",
    ".env",
    ".example",
    ".html",
    ".js",
    ".json",
    ".jsx",
    ".md",
    ".mjs",
    ".py",
    ".rs",
    ".sh",
    ".sql",
    ".toml",
    ".ts",
    ".tsx",
    ".txt",
    ".yaml",
    ".yml",
}

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


def make_file_record(repo: Path, rel: str, tracked: bool) -> FileRecord | None:
    full = repo / rel
    if not full.is_file():
        return None
    suffix = full.suffix.lower()
    return FileRecord(
        path=rel,
        directory=str(Path(rel).parent) if str(Path(rel).parent) != "." else "",
        name=full.name,
        extension=suffix or "(none)",
        language=language_for(rel),
        role_guess=role_guess(rel),
        size_bytes=full.stat().st_size,
        sha256=sha256_file(full),
        tracked=tracked,
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
    repo_path: Path,
    head: str,
    branch: str,
    files: list[FileRecord],
    directories: list[DirectoryRecord],
) -> None:
    extension_counts = Counter(file.extension for file in files)
    role_counts = Counter(file.role_guess for file in files)
    total_size = sum(file.size_bytes for file in files)

    lines: list[str] = []
    lines.append("---")
    lines.append("Status: generated")
    lines.append("Canonical: no")
    lines.append(f"Generated: {dt.datetime.now(dt.UTC).isoformat()}")
    lines.append("---")
    lines.append("")
    lines.append(f"# Full Repository Record — `{repo_name}`")
    lines.append("")
    lines.append("> Generated mechanically from `git ls-files` and filesystem metadata. Do not hand-edit generated sections; rerun `scripts/generate_repo_record.py`.")
    lines.append("")
    lines.append("## Snapshot")
    lines.append("")
    lines.append(f"- Repo path: `{repo_path}`")
    lines.append(f"- Branch: `{branch}`")
    lines.append(f"- HEAD: `{head}`")
    lines.append(f"- Tracked files recorded: `{len(files)}`")
    lines.append(f"- Directories recorded: `{len(directories)}`")
    lines.append(f"- Total tracked bytes: `{total_size}`")
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


def generate_for_repo(repo_name: str, repo_path: Path, out_dir: Path, include_untracked: bool) -> None:
    repo_path = repo_path.resolve()
    if not (repo_path / ".git").exists():
        raise SystemExit(f"{repo_path} does not look like a git repository")

    tracked_paths = git_ls_files(repo_path)
    all_paths: list[tuple[str, bool]] = [(path, True) for path in tracked_paths]
    if include_untracked:
        all_paths.extend((path, False) for path in git_untracked(repo_path))

    files = [record for path, tracked in all_paths if (record := make_file_record(repo_path, path, tracked))]
    directories = build_directories(files)
    head = git_head(repo_path)
    branch = git_branch(repo_path)

    out_dir.mkdir(parents=True, exist_ok=True)
    payload = {
        "schema": "icn.repo_record.v1",
        "generated_at": dt.datetime.now(dt.UTC).isoformat(),
        "repo": repo_name,
        "repo_path": str(repo_path),
        "branch": branch,
        "head": head,
        "include_untracked": include_untracked,
        "summary": {
            "file_count": len(files),
            "directory_count": len(directories),
            "total_size_bytes": sum(file.size_bytes for file in files),
            "extensions": dict(sorted(Counter(file.extension for file in files).items())),
            "roles": dict(sorted(Counter(file.role_guess for file in files).items())),
        },
        "directories": [asdict(record) for record in directories],
        "files": [asdict(record) for record in sorted(files, key=lambda item: item.path)],
    }

    json_path = out_dir / f"{repo_name}-file-record.json"
    md_path = out_dir / f"{repo_name}-file-record.md"
    json_path.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
    write_markdown(md_path, repo_name, repo_path, head, branch, files, directories)

    print(f"wrote {json_path}")
    print(f"wrote {md_path}")


def parse_repo_arg(value: str) -> tuple[str, Path]:
    if "=" not in value:
        raise argparse.ArgumentTypeError("repo must be NAME=PATH")
    name, path = value.split("=", 1)
    if not name:
        raise argparse.ArgumentTypeError("repo name cannot be empty")
    return name, Path(path)


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
        "--include-untracked",
        action="store_true",
        help="Also record untracked, non-ignored local files. Review carefully before committing outputs.",
    )
    args = parser.parse_args()

    for repo_name, repo_path in args.repo:
        generate_for_repo(repo_name, repo_path, args.out, args.include_untracked)


if __name__ == "__main__":
    main()
