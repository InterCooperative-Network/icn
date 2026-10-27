"""The explicit squash commit message: its shape, its preview and its proof.

WHY THIS EXISTS
Left to itself, GitHub composes a squash commit from repository settings — here the PR title and
EVERY commit message on the branch. Those commit messages were never reviewed as mainline history,
and one of them can carry something that must not land there (a personal name, a machine path).
The operator who authorizes a merge can therefore state the title and body explicitly, and this
module is the one place that says what such a statement may look like.

WHAT IT DOES NOT DO
It does not decide whether a merge may happen; every gate stays where it was. A message is carried
on the SAME head-pinned request as before — two more structured fields, never a second request,
never a retry without them. If GitHub refuses the request, that refusal is final like any other.

PROVED, NOT PREDICTED
What the operator previews is what this program sends, not what GitHub stores. After a confirmed
merge the resulting commit is read back and compared, and the receipt says MATCH, MISMATCH or
UNREADABLE. A mismatch cannot be undone by this program — the merge has happened — so it is
reported as evidence for a human, never retried and never hidden.
"""

from __future__ import annotations

import hashlib
import pathlib
import unicodedata
from dataclasses import dataclass

from .errors import UsageError

# Generous for a subject line, small enough that a pasted body is caught as a mistake.
MAX_TITLE_CHARS = 256
# A commit body, not a document. A file larger than this is almost certainly the wrong file.
MAX_BODY_BYTES = 64 * 1024

# Characters a body may contain beyond printable text. Everything else in Unicode category C* is
# refused: a carriage return, an escape sequence or a bidi override is not something a reviewer
# can see in a preview, and the point of an explicit message is that what was read is what lands.
_BODY_ALLOWED_CONTROLS = {"\n", "\t"}

# The receipt's comparison states.
MATCH = "MATCH"
MISMATCH = "MISMATCH"
UNREADABLE = "UNREADABLE"


def digest(text: str) -> str:
    # `surrogatepass`: this also hashes text GitHub returned, which may hold a lone surrogate.
    return hashlib.sha256(text.encode("utf-8", "surrogatepass")).hexdigest()


def _hidden(ch: str) -> bool:
    """Control, format, surrogate, private-use or unassigned (C*), or a Unicode line/paragraph
    separator (Zl/Zp), which breaks a line without looking like a newline in a preview."""
    category = unicodedata.category(ch)
    return category.startswith("C") or category in ("Zl", "Zp")


@dataclass(frozen=True)
class SquashMessage:
    title: str
    body: str

    @property
    def full_text(self) -> str:
        """The commit message this request asks GitHub to record: headline, blank line, body."""
        return f"{self.title}\n\n{self.body}"

    @property
    def sha256(self) -> str:
        return digest(self.full_text)

    def as_dict(self) -> dict:
        return {"title": self.title, "body": self.body, "sha256": self.sha256}


def validate_title(title: str) -> str:
    """Refuse rather than repair: the title sent must be the title the operator wrote."""
    if not title.strip():
        raise UsageError("--commit-title is empty")
    if title != title.strip():
        raise UsageError("--commit-title has leading or trailing whitespace; state it exactly")
    if len(title) > MAX_TITLE_CHARS:
        raise UsageError(f"--commit-title is {len(title)} characters; the limit is "
                         f"{MAX_TITLE_CHARS}")
    bad = sorted({f"U+{ord(c):04X}" for c in title if _hidden(c)})
    if bad:
        raise UsageError(f"--commit-title contains control or invisible characters {bad}; a "
                         "title is one visible line")
    return title


def validate_body(body: str) -> str:
    if not body.strip():
        raise UsageError("the --commit-body-file body is empty. An empty body is not a neutral "
                         "message: GitHub may substitute its default, which is the commit list "
                         "this option exists to keep out")
    bad = sorted({f"U+{ord(c):04X}" for c in body
                  if c not in _BODY_ALLOWED_CONTROLS and _hidden(c)})
    if bad:
        raise UsageError(f"the --commit-body-file body contains control or invisible characters "
                         f"{bad}; only newline and tab are accepted, so that the previewed text "
                         "is the text that lands")
    return body


def read_body_file(path: str) -> str:
    """Read the body exactly, except for trailing newlines (a file normally ends with one)."""
    if not path:
        raise UsageError("--commit-body-file requires a path")
    file = pathlib.Path(path)
    try:
        if not file.is_file():
            raise UsageError(f"--commit-body-file {path!r} is not a regular file")
        size = file.stat().st_size
        if size > MAX_BODY_BYTES:
            raise UsageError(f"--commit-body-file {path!r} is {size} bytes; the limit is "
                             f"{MAX_BODY_BYTES}")
        raw = file.read_bytes()
    except OSError as exc:
        raise UsageError(f"--commit-body-file {path!r} could not be read: {exc}") from exc
    if len(raw) > MAX_BODY_BYTES:                # the file grew between stat and read
        raise UsageError(f"--commit-body-file {path!r} exceeds {MAX_BODY_BYTES} bytes")
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise UsageError(f"--commit-body-file {path!r} is not valid UTF-8: {exc}") from exc
    if text.startswith("﻿"):
        raise UsageError(f"--commit-body-file {path!r} begins with a byte-order mark; save it "
                         "as plain UTF-8")
    return validate_body(text.rstrip("\n"))


def build(title: str, body: str) -> SquashMessage:
    return SquashMessage(validate_title(title), validate_body(body))


def compare(requested: SquashMessage, observed: object) -> str:
    """MATCH when the stored message is the requested one, allowing only trailing newlines."""
    if not isinstance(observed, str):
        return UNREADABLE
    return MATCH if observed.rstrip("\n") == requested.full_text else MISMATCH
