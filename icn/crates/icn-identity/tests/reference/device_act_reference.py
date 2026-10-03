#!/usr/bin/env python3
"""Independent reference implementation of the N4-A canonical device act, for cross-implementation audit.

WHY THIS FILE EXISTS
--------------------
`device_authority.rs` pins fixed expected bytes for one `DeviceActV1`. A vector that the Rust
implementation generates and then compares against itself proves only that the code is
deterministic. It does not prove that the *specification* is sufficient for someone else to
reimplement -- which is the property an externally consumable act format actually needs, because
the whole point of N4-A's external boundary is that a downstream client can produce these bytes
and a verifier elsewhere can check them.

This file is that someone else. It is written from the prose in
`docs/architecture/N4A_DEVICE_AUTHORITY_EVALUATION.md` alone and shares no code, no library and
no constant definition with `icn-identity`. It re-derives the canonical bytes and the act id from
first principles, then reads the `EXPECT_*` literals out of the Rust test file and asserts byte
equality.

The Rust literals are used ONLY as the comparison target. No value below is derived from them.

RUN IT
------
    python3 icn/crates/icn-identity/tests/reference/device_act_reference.py

Exits 0 when both implementations agree, 1 on any mismatch, 2 on a setup problem.
Requires `cryptography` (Ed25519 public-key derivation and signature verification). Not wired into
CI: it is an audit tool, and adding a Python crypto dependency to the Rust test job would buy
nothing the Rust tests do not already assert.

The signature is verified rather than pinned: RFC 8032 signatures are deterministic, but the
document deliberately keeps signature bytes out of semantic identity (`act_id` is over the
canonical bytes alone), so this reference checks that the Rust-produced signature verifies under
the derived device key over the independently derived bytes, and nothing more.

IF A BYTE DIFFERS
-----------------
Investigate the cause. Do not "fix" whichever side is inconvenient: a disagreement here means the
spec, the Rust implementation, or this reference is wrong, and which one is the whole question.
"""
import hashlib
import re
import sys
from pathlib import Path

try:
    from cryptography.exceptions import InvalidSignature
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric.ed25519 import (
        Ed25519PrivateKey,
        Ed25519PublicKey,
    )
except ImportError:  # pragma: no cover - setup problem, not a vector mismatch
    print("setup: the `cryptography` package is required", file=sys.stderr)
    sys.exit(2)


# --- framing, from the document §3 -----------------------------------------------------------
def lp(x: bytes) -> bytes:     return len(x).to_bytes(4, "big") + x   # LP(x) := u32be(len) || x
def u8(n: int) -> bytes:       return n.to_bytes(1, "big")
def u16(n: int) -> bytes:      return n.to_bytes(2, "big")
def u64(n: int) -> bytes:      return n.to_bytes(8, "big")
def h(b: bytes) -> bytes:      return hashlib.sha256(b).digest()
def principal(pk: bytes) -> bytes:  return b"\x01" + pk               # u8(0x01) || b32(key)


def ed25519_pub(seed: bytes) -> bytes:
    key = Ed25519PrivateKey.from_private_bytes(seed)
    return key.public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw
    )


# --- constants, from the document §4 ---------------------------------------------------------
DEVICE_ACT_DOMAIN = b"icn.n4.device-act"
DEVICE_ACT_VERSION = 1
CAPABILITY_TAG = {"Sign": 0x01, "Encrypt": 0x02, "Present": 0x03, "Recover": 0x04}


# --- vector inputs, stated in full in the Rust test and in the document §8 -------------------
SUBJECT = bytes(0x20 + i for i in range(32))         # an opaque 32-byte SubjectId
DEVICE_SEED = bytes(0x40 + i for i in range(32))     # the device signing seed
EVALUATION_POSITION = 3
CAPABILITY = "Sign"
PAYLOAD = b"katie:open-workspace"


def canonical_act() -> bytes:
    return (
        lp(DEVICE_ACT_DOMAIN)
        + u16(DEVICE_ACT_VERSION)
        + SUBJECT
        + principal(ed25519_pub(DEVICE_SEED))
        + u64(EVALUATION_POSITION)
        + u8(CAPABILITY_TAG[CAPABILITY])
        + lp(PAYLOAD)
    )


def rust_literals(path: Path) -> dict:
    text = path.read_text()
    found = {}
    for name, value in re.findall(r'const (EXPECT_[A-Z0-9_]+): &str\s*=\s*"([0-9a-f]+)";', text):
        found[name] = value
    return found


def main() -> int:
    here = Path(__file__).resolve()
    rust_test = here.parent.parent / "device_authority.rs"
    if not rust_test.is_file():
        print(f"setup: cannot find {rust_test}", file=sys.stderr)
        return 2
    expect = rust_literals(rust_test)

    act = canonical_act()
    derived = {
        "EXPECT_DEVICE_PUBKEY_HEX": ed25519_pub(DEVICE_SEED).hex(),
        "EXPECT_ACT_BYTES_HEX": act.hex(),
        "EXPECT_ACT_ID_HEX": h(act).hex(),
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

    # The Rust test also pins the signature it produced. Verify it against the independently
    # derived bytes and key; it is evidence of agreement, not part of the act's identity.
    sig_hex = expect.get("EXPECT_ACT_SIGNATURE_HEX")
    if sig_hex is None:
        print("MISSING  EXPECT_ACT_SIGNATURE_HEX")
        ok = False
    else:
        pub = Ed25519PublicKey.from_public_bytes(ed25519_pub(DEVICE_SEED))
        try:
            pub.verify(bytes.fromhex(sig_hex), act)
            print("ok       EXPECT_ACT_SIGNATURE_HEX verifies over the reference bytes")
        except InvalidSignature:
            print("MISMATCH EXPECT_ACT_SIGNATURE_HEX does not verify over the reference bytes")
            ok = False

    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
