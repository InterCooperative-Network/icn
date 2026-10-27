#!/usr/bin/env python3
"""Independent reference reader for the proposed v1 import-incomplete marker record.

WHY THIS FILE EXISTS
--------------------
`.icn-import-incomplete` is a top-level file in an ICN data directory (ADR-0089 §6, §7). Backup,
restore and verify-backup handle data-directory entries by name, so once shipped its name and record
format are permanent (ADR-0089 "Irreversible commitments" #5). This file is a second reader of that
record, written from the grammar in `src/import_marker.rs` alone. It shares no code with
`icn-core`. It classifies every vector in `import_marker_vectors.txt` and compares the result with
the expected classification recorded there, which the Rust test `tests/import_marker.rs` checks
against the same file.

The format is PROPOSED, not frozen: the retention snapshot that follows `fact-count` is not yet
specified (ADR-0089 §8's retention file has no format yet). This reader therefore applies the same
rule as the Rust refusal path: the record is checked lexically in full, the identity prefix is
interpreted, and the lines after it are not.

RUN IT
------
    python3 -I icn/crates/icn-core/tests/reference/import_marker_reference.py

Exits 0 when every vector classifies as expected, 1 on any mismatch, 2 on a setup problem.
Standard library only.
"""
import re
import sys
from pathlib import Path

HEADER = b"icn-import-incomplete v1"
MAX_RECORD_BYTES = 64 * 1024
MAX_FACT_COUNT = 4096                      # MAX_SEGMENT_FACTS, ADR-0089 §8 (provisional)
IDENTITY_KEYS = ("segment-id", "subject", "fact-count")
KEY = re.compile(r"[a-z0-9-]+")
VALUE = re.compile(r"[\x21-\x7e]+")
HEX32 = re.compile(r"[0-9a-f]{64}")
COUNT = re.compile(r"[1-9][0-9]{0,3}")


class Unreadable(Exception):
    pass


def read(record: bytes):
    """Return (subject_hex, segment_hex, fact_count) or raise Unreadable."""
    if not record:
        raise Unreadable("empty")
    if len(record) > MAX_RECORD_BYTES:
        raise Unreadable("oversized")
    if any(not (b == 0x0A or 0x20 <= b <= 0x7E) for b in record):
        raise Unreadable("byte outside printable ASCII and LF")
    if record[-1] != 0x0A:
        raise Unreadable("last line unterminated")
    lines = record[:-1].decode("ascii").split("\n")
    if lines[0] != HEADER.decode():
        raise Unreadable("unsupported version" if lines[0].startswith("icn-import-incomplete ")
                         else "no header")
    fields, seen = [], set()
    for line in lines[1:]:
        if line == "":
            raise Unreadable("empty line")
        if line.startswith("#"):
            continue
        if " " not in line:
            raise Unreadable("no separator")
        key, value = line.split(" ", 1)
        if not KEY.fullmatch(key) or not VALUE.fullmatch(value):
            raise Unreadable("bad key or value")
        if key in seen:
            raise Unreadable("duplicate key")
        seen.add(key)
        fields.append((key, value))
    if len(fields) < 3 or tuple(k for k, _ in fields[:3]) != IDENTITY_KEYS:
        raise Unreadable("identity prefix missing or out of order")
    segment, subject, count = (v for _, v in fields[:3])
    if not HEX32.fullmatch(segment) or not HEX32.fullmatch(subject):
        raise Unreadable("bad hex")
    if not COUNT.fullmatch(count) or int(count) > MAX_FACT_COUNT:
        raise Unreadable("bad fact-count")
    return subject, segment, int(count)


def classify(record: bytes) -> str:
    try:
        subject, segment, count = read(record)
    except Unreadable:
        return "unreadable"
    return f"incomplete:{subject}:{segment}:{count}"


def main() -> int:
    vectors = Path(__file__).resolve().parent / "import_marker_vectors.txt"
    if not vectors.is_file():
        print(f"setup: cannot find {vectors}", file=sys.stderr)
        return 2
    ok, count = True, 0
    for line in vectors.read_text(encoding="ascii").splitlines():
        if not line or line.startswith("#"):
            continue
        name, expected, hexed = line.split("\t")
        got = classify(bytes.fromhex(hexed))
        count += 1
        if got == expected:
            print(f"ok       {name}")
        else:
            print(f"MISMATCH {name}: expected {expected}, reference reads {got}")
            ok = False

    # Size boundary, built here: a valid identity prefix padded with comment lines.
    prefix = (HEADER + b"\nsegment-id " + b"ab" * 32 + b"\nsubject " + b"cd" * 32
              + b"\nfact-count 1\n")
    def padded(total: int) -> bytes:
        pad = total - len(prefix)
        return prefix + b"#" + b"x" * (pad - 2) + b"\n"
    at, over = padded(MAX_RECORD_BYTES), padded(MAX_RECORD_BYTES + 1)
    assert len(at) == MAX_RECORD_BYTES and len(over) == MAX_RECORD_BYTES + 1
    for name, record, expected in (("exactly-64KiB", at, "incomplete:" + "cd" * 32 + ":" + "ab" * 32 + ":1"),
                                   ("64KiB-plus-one", over, "unreadable")):
        got = classify(record)
        count += 1
        if got == expected:
            print(f"ok       {name}")
        else:
            print(f"MISMATCH {name}: expected {expected}, reference reads {got}")
            ok = False
    print(f"{count} cases")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
