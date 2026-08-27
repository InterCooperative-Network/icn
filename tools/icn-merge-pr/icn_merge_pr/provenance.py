"""Read the install-time provenance record that sits beside an installed package.

The record answers one question: which commit of which repository's default branch produced the
code that is about to decide a merge. It is written by the installer only after that commit has
been proved to be the live default-branch tip, so its presence is the marker of a trusted runtime.
"""

from __future__ import annotations

import hashlib
import json
import pathlib
import re

from .errors import NotInstalled

FILENAME = "provenance.json"
# The directory name the source layout uses. An installed copy never lands here.
SOURCE_DIR_NAME = "icn-merge-pr"
PACKAGE_DIR_NAME = "icn_merge_pr"
# A full Git object id, as `git rev-parse HEAD` produces and as this repository uses. Validated at
# the boundary where the record becomes trusted structured evidence, rather than wherever it is
# first used: a truthy non-string reached `installed_commit[:12]` in the staleness report and
# raised TypeError, so malformed provenance crashed instead of refusing (icn#2659 review).
_GIT_OID = re.compile(r"\A[0-9a-fA-F]{40}\Z")
_SHA256 = re.compile(r"\A[0-9a-f]{64}\Z")


def record_path() -> pathlib.Path:
    return pathlib.Path(__file__).resolve().parent.parent / FILENAME


def _containing_source_checkout(start: pathlib.Path) -> pathlib.Path | None:
    """The git checkout carrying THIS TOOL'S SOURCE that `start` sits inside, if any.

    Refusing every path under any git repository would break a legitimate install for anyone who
    versions their home directory, so the test is specific: a repository is disqualifying only
    when it also contains `tools/icn-merge-pr`. That is the repository whose pull requests could
    have written the record, and a candidate that renames its directory to dodge the name check
    is still sitting inside it.
    """
    try:
        here = start.resolve()
    except OSError:
        return None
    for directory in (here, *here.parents):
        if (directory / ".git").exists() and (directory / "tools" / SOURCE_DIR_NAME).is_dir():
            return directory
    return None


def is_installed() -> bool:
    """True only for a copy whose provenance record it could not have written about itself."""
    try:
        read()
    except NotInstalled:
        return False
    return True


def read() -> dict:
    path = record_path()
    if not path.is_file():
        raise NotInstalled(
            "this is not an installed icn-merge-pr: no provenance record beside the package. "
            "Install it from a clean, up-to-date default-branch checkout with "
            "`python3 tools/icn-merge-pr/install.py`.")
    try:
        record = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise NotInstalled(f"provenance record at {path} is unreadable: {exc}") from exc
    if not isinstance(record, dict):
        raise NotInstalled(f"provenance record at {path} is not a provenance record")
    _typed_fields(path, record)

    # A record is a claim, and a claim shipped by the change under evaluation is worth nothing.
    # Committing `tools/icn-merge-pr/provenance.json` in a pull request would otherwise make the
    # candidate copy look installed and unlock mutation — the exact boundary the file marks.
    root = path.parent
    if root.name == SOURCE_DIR_NAME:
        raise NotInstalled(
            f"{path} sits in the tool's own source layout ({SOURCE_DIR_NAME}/), so it is a "
            "provenance record a checkout can write about itself. Install the program instead.")
    checkout = _containing_source_checkout(root)
    if checkout is not None:
        raise NotInstalled(
            f"{path} is inside {checkout}, a checkout that carries this tool's source, so the "
            "record is a file that repository can write about itself — renaming the directory "
            "does not change that. An installed copy lives outside every worktree.")
    recorded = record.get("lib")
    if not isinstance(recorded, str) or pathlib.Path(recorded).resolve() != root.resolve():
        raise NotInstalled(
            f"provenance record at {path} was written for {recorded!r}, not for {root}; a record "
            "copied beside a different copy of the code proves nothing about that copy.")
    _verify_files(root, record)
    return record


def _typed_fields(path: pathlib.Path, record: dict) -> None:
    """Every field this program later uses with an assumed type, established here.

    One bounded pass over the record, not a schema: the only field with the reported defect was
    `source_commit`, and the rest are pinned to the same standard so the next consumer of any of
    them cannot inherit the same surprise.
    """
    commit = record.get("source_commit")
    if not isinstance(commit, str) or not _GIT_OID.match(commit):
        raise NotInstalled(
            f"provenance record at {path} names {commit!r} as its source commit, which is not a "
            "full Git object id. Malformed provenance is refused here rather than surprising "
            "something further down that expected a string.")
    slug = record.get("repository")
    if not isinstance(slug, str) or slug.count("/") != 1 or not all(slug.split("/")):
        raise NotInstalled(
            f"provenance record at {path} names {slug!r} as its repository, which is not OWNER/NAME")
    branch = record.get("default_branch")
    if not isinstance(branch, str) or not branch:
        raise NotInstalled(
            f"provenance record at {path} names {branch!r} as its default branch")


def _verify_files(root: pathlib.Path, record: dict) -> None:
    """Every file the record names must be present and byte-for-byte what it names.

    This is an INTEGRITY check, not an authentication one, and the difference is worth stating: it
    catches an installed tree that has been edited or partially replaced since it was installed —
    the realistic tampering, because an install directory outlives the install. It does not, and
    cannot, make a wholly fabricated record detectable; a local file is not evidence about itself
    without a signature or an out-of-band root, and pretending otherwise would be the more
    dangerous mistake.
    """
    files = record.get("files")
    if not isinstance(files, dict) or not files:
        raise NotInstalled("provenance record names no installed files")
    for relative, digest in sorted(files.items()):
        if not isinstance(relative, str) or not isinstance(digest, str):
            raise NotInstalled("provenance record has a malformed file entry")
        if not _SHA256.match(digest):
            raise NotInstalled(
                f"provenance record describes {relative} with {digest!r}, which is not a sha256 "
                "digest")
        # The same shape the bootstrap requires: a file inside the package directory, and nothing
        # that could climb out of it. `root / "../.."` would have been read and compared rather
        # than refused.
        parts = relative.replace("\\", "/").split("/")
        if len(parts) != 2 or parts[0] != PACKAGE_DIR_NAME or not parts[1] or ".." in parts:
            raise NotInstalled(
                f"provenance record names {relative!r}, which is not a file inside "
                f"{PACKAGE_DIR_NAME}/")
        target = root / relative
        try:
            if not target.is_file() or hashlib.sha256(target.read_bytes()).hexdigest() != digest:
                raise NotInstalled(
                    f"installed file {relative} is missing or is not what the provenance record "
                    "describes; reinstall rather than trusting an edited tree")
        except OSError as exc:
            raise NotInstalled(f"installed file {relative} is unreadable: {exc}") from exc


def default_repository() -> tuple[str, str] | None:
    """(owner, name) recorded at install time, or None when running from a source checkout."""
    try:
        record = read()
    except NotInstalled:
        return None
    owner, name = record["repository"].split("/", 1)   # shape established by read()
    return owner, name
