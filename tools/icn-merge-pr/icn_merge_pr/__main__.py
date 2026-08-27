"""Entry point, and the trusted bootstrap that runs before anything is imported from the install.

THE DEFECT THIS EXISTS FOR
Putting the install root on `sys.path` and then importing the package hands control to that
directory before anything has checked it. An unrecorded top-level module — `json.py` is the easy
one, because `cli` imports `json` — executes during that import, ahead of `provenance.read()`,
and the digest check never notices because it only looks at paths the record names. Poisoning the
install directory therefore bypassed the mutation gate without altering a single recorded file.
That was reproduced in a throwaway fixture install before this was written.

WHAT THIS IS, EXACTLY
CLOSED-TREE INTEGRITY. Not authentication. The record is still a local file, and a local actor who
can rewrite the whole installation together with its record is still outside this program's threat
model — they already hold the same credentials it would use. What changes here is narrower and
worth having on its own: a tree that has merely GAINED an unexpected importable file no longer
gets to run it.

WHY THE CHECKS LIVE IN THIS FILE
The bootstrap cannot import the package it is verifying, so it cannot reuse
`icn_merge_pr.provenance`. The duplication with `provenance._verify_files` is deliberate: this is
the copy that runs when nothing has been trusted yet, and it is written against the standard
library alone.

HOW THE INTERPRETER IS STARTED
The generated launcher uses `python3 -I -B`. Verified on this repository's interpreter (3.12.3):
`-I` leaves the script's own directory OFF `sys.path` entirely and ignores `PYTHONPATH`, so
`import json` here resolves from the standard library even when the install tree holds a `json.py`;
`-B` stops bytecode being written, which keeps the tree closed across runs instead of manufacturing
`__pycache__` that a closure check would then have to forgive. Neither flag is assumed: the path is
pruned and re-asserted below, and bytecode writing is disabled from inside as well, so the
guarantee does not depend on how this file was invoked.
"""

import hashlib
import json
import os
import stat
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
_LIB = os.path.dirname(_HERE)
_PACKAGE = os.path.basename(_HERE)
_RECORD_NAME = "provenance.json"
# Duplicated from `codes.REFUSED_NOT_INSTALLED` because the bootstrap must be able to refuse before
# it may import anything. A test asserts the two spellings agree.
_REFUSED = "REFUSED_NOT_INSTALLED"

# Even if someone invokes this file without -B, nothing below should leave import artifacts behind.
sys.dont_write_bytecode = True


def _refuse(detail):
    """Refuse in the same machine-readable shape the CLI uses, then stop."""
    payload = {"tool": "icn-merge-pr", "outcome": _REFUSED,
               "reasons": [{"code": _REFUSED, "detail": detail}]}
    sys.stdout.write(json.dumps(payload, indent=2) + "\n")
    sys.stderr.write(detail + "\n")
    raise SystemExit(1)


def _seal_import_path():
    """The install tree must not be importable until it has been verified.

    `-I` already achieves this, and this repeats it rather than trusting it: the guarantee that
    matters is a property of the running process, not of the command line that started it.
    """
    for entry in ("", ".", os.getcwd(), _LIB, _HERE):
        while entry in sys.path:
            sys.path.remove(entry)
    for entry in (_LIB, _HERE):
        if entry in sys.path:                      # asserted, not assumed
            _refuse(f"{entry} is on the import path before it has been verified")


def _digest(path):
    hasher = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def _recorded_files(record):
    files = record.get("files")
    if not isinstance(files, dict) or not files:
        _refuse(f"{_RECORD_NAME} names no installed files, so nothing about this tree is checkable")
    expected = {}
    for relative, digest in files.items():
        if not isinstance(relative, str) or not isinstance(digest, str):
            _refuse(f"{_RECORD_NAME} has a malformed file entry")
        parts = relative.replace("\\", "/").split("/")
        if len(parts) != 2 or parts[0] != _PACKAGE or not parts[1]:
            _refuse(f"{_RECORD_NAME} names {relative!r}, which is not a file inside {_PACKAGE}/")
        expected[parts[1]] = digest
    return expected


def _listdir(path, label):
    try:
        return set(os.listdir(path))
    except OSError as exc:
        _refuse(f"the installed {label} at {path} cannot be read: {exc}")


def _verify_closed_tree():
    """Prove the install tree holds exactly what its record describes, and nothing else.

    Returns False when there is no record at all, which is a source-tree run: there is no manifest
    to be closed against, and mutation is refused separately for exactly that reason.
    """
    record_path = os.path.join(_LIB, _RECORD_NAME)
    if not os.path.exists(record_path):
        return False
    if os.path.islink(record_path):
        _refuse(f"{record_path} is a symlink; an installed provenance record is a regular file")
    try:
        with open(record_path, "rb") as handle:
            record = json.loads(handle.read().decode("utf-8"))
    except (OSError, ValueError, UnicodeDecodeError) as exc:
        _refuse(f"{record_path} is unreadable: {exc}")
    if not isinstance(record, dict):
        _refuse(f"{record_path} is not a provenance record")

    expected = _recorded_files(record)

    # The install root is CLOSED: the record, and the package directory. Nothing else, because
    # the root is what goes on the import path and anything else there can shadow the standard
    # library — which is precisely how an unrecorded `json.py` got itself executed.
    unexpected_root = sorted(_listdir(_LIB, "library root") - {_RECORD_NAME, _PACKAGE})
    if unexpected_root:
        _refuse(f"the install tree at {_LIB} holds files its provenance record does not describe: "
                f"{unexpected_root}. An installed tree that has gained an importable file is not "
                f"the tree that was installed; reinstall rather than running it.")

    if os.path.islink(_HERE) or not os.path.isdir(_HERE):
        _refuse(f"{_HERE} is not a real package directory")
    unexpected_pkg = sorted(_listdir(_HERE, "package directory") - set(expected))
    if unexpected_pkg:
        _refuse(f"the installed package at {_HERE} holds entries its provenance record does not "
                f"describe: {unexpected_pkg}. Reinstall rather than running it.")

    # Every recorded file is a regular file, not a link to one, and is byte-for-byte as recorded.
    for name in sorted(expected):
        full = os.path.join(_HERE, name)
        if os.path.islink(full):
            _refuse(f"installed file {_PACKAGE}/{name} is a symlink; installed files are regular "
                    "files, and a link points somewhere this check does not cover")
        try:
            info = os.lstat(full)
        except OSError as exc:
            _refuse(f"installed file {_PACKAGE}/{name} cannot be read: {exc}")
        if not stat.S_ISREG(info.st_mode):
            _refuse(f"installed file {_PACKAGE}/{name} is not a regular file")
        try:
            if _digest(full) != expected[name]:
                _refuse(f"installed file {_PACKAGE}/{name} is not what the provenance record "
                        "describes; reinstall rather than trusting an edited tree")
        except OSError as exc:
            _refuse(f"installed file {_PACKAGE}/{name} cannot be read: {exc}")
    return True


_seal_import_path()
_verify_closed_tree()
sys.path.insert(0, _LIB)

from icn_merge_pr.cli import main  # noqa: E402  (nothing may be imported before verification)

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
