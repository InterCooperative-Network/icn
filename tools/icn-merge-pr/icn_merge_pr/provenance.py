"""Read the install-time provenance record that sits beside an installed package.

The record answers one question: which commit of which repository's default branch produced the
code that is about to decide a merge. It is written by the installer only after that commit has
been proved to be the live default-branch tip, so its presence is the marker of a trusted runtime.
"""

from __future__ import annotations

import json
import pathlib

from .errors import NotInstalled

FILENAME = "provenance.json"


def record_path() -> pathlib.Path:
    return pathlib.Path(__file__).resolve().parent.parent / FILENAME


def is_installed() -> bool:
    return record_path().is_file()


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
    if not isinstance(record, dict) or not record.get("source_commit"):
        raise NotInstalled(f"provenance record at {path} names no source commit")
    return record


def default_repository() -> tuple[str, str] | None:
    """(owner, name) recorded at install time, or None when running from a source checkout."""
    try:
        record = read()
    except NotInstalled:
        return None
    slug = record.get("repository")
    if not isinstance(slug, str) or slug.count("/") != 1:
        return None
    owner, name = slug.split("/", 1)
    return (owner, name) if owner and name else None
