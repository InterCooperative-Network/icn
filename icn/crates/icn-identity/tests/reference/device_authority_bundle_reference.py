#!/usr/bin/env python3
"""Independent reference for the N4-B device-authority bundle framing, for cross-implementation audit.

WHY THIS FILE EXISTS
--------------------
`device_authority_bundle.rs` pins the bytes of one bundle. A vector the Rust code generates and
compares against itself proves only determinism; the property an externally carried container
needs is that someone else, holding only the written contract, produces the same bytes. This file
is that someone else: it is written from the prose of `docs/architecture/N4B_DEVICE_AUTHORITY_BUNDLE.md`
alone, shares no code with `icn-identity`, and compares its result against the `EXPECT_*` literals
read out of the Rust test file.

What it covers: the bundle framing (domain, version, fact count, act, act signature) over the N4-A
act vector, with ZERO facts. What it does not cover: a fact record, because producing an admissible
N1 fact requires the N1 inception/authorize encodings, which `gen_subject_context_reference.py`
covers separately. The fact-record layout is pinned by the Rust tests' hand-framed records instead.

RUN IT
------
    python3 icn/crates/icn-identity/tests/reference/device_authority_bundle_reference.py

Exits 0 when both agree, 1 on any mismatch, 2 on a setup problem. Requires `cryptography`.
Not wired into CI: an audit tool.
"""
import hashlib
import re
import sys
from pathlib import Path

try:
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
except ImportError:  # pragma: no cover
    print("setup: the `cryptography` package is required", file=sys.stderr)
    sys.exit(2)


# --- framing, from the N1/N4 contracts -------------------------------------------------------
def lp(x: bytes) -> bytes:   return len(x).to_bytes(4, "big") + x
def u8(n: int) -> bytes:     return n.to_bytes(1, "big")
def u16(n: int) -> bytes:    return n.to_bytes(2, "big")
def u32(n: int) -> bytes:    return n.to_bytes(4, "big")
def u64(n: int) -> bytes:    return n.to_bytes(8, "big")
def h(b: bytes) -> bytes:    return hashlib.sha256(b).digest()
def principal(pk: bytes) -> bytes:  return b"\x01" + pk


# --- constants, from the documents -----------------------------------------------------------
DEVICE_ACT_DOMAIN = b"icn.n4.device-act"
DEVICE_ACT_VERSION = 1
BUNDLE_DOMAIN = b"icn.n4.device-authority-bundle"
BUNDLE_VERSION = 1
CAPABILITY_SIGN = 0x01

# --- the N4-A act vector, restated -----------------------------------------------------------
SUBJECT = bytes(0x20 + i for i in range(32))
DEVICE_SEED = bytes(0x40 + i for i in range(32))
EVALUATION_POSITION = 3
PAYLOAD = b"katie:open-workspace"


def main() -> int:
    here = Path(__file__).resolve()
    rust_test = here.parent.parent / "device_authority_bundle.rs"
    if not rust_test.is_file():
        print(f"setup: cannot find {rust_test}", file=sys.stderr)
        return 2
    expect = dict(re.findall(r'const (EXPECT_[A-Z0-9_]+): &str\s*=\s*"([0-9a-f]+)";', rust_test.read_text()))

    key = Ed25519PrivateKey.from_private_bytes(DEVICE_SEED)
    pub = key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
    act = (
        lp(DEVICE_ACT_DOMAIN) + u16(DEVICE_ACT_VERSION) + SUBJECT + principal(pub)
        + u64(EVALUATION_POSITION) + u8(CAPABILITY_SIGN) + lp(PAYLOAD)
    )
    signature = key.sign(act)  # RFC 8032: deterministic, so the bytes are reproducible

    # bundle_v1 := LP(domain) || u16be(version) || u32be(fact_count) || facts || LP(act) || b64(sig)
    bundle = lp(BUNDLE_DOMAIN) + u16(BUNDLE_VERSION) + u32(0) + lp(act) + signature

    derived = {
        "EXPECT_BUNDLE_BYTES_HEX": bundle.hex(),
        "EXPECT_BUNDLE_ID_HEX": h(bundle).hex(),
    }
    ok = True
    for name, value in derived.items():
        pinned = expect.get(name)
        if pinned is None:
            print(f"MISSING  {name} (reference derives {value})")
            ok = False
        elif pinned != value:
            print(f"MISMATCH {name}\n  rust      = {pinned}\n  reference = {value}")
            ok = False
        else:
            print(f"ok       {name}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
