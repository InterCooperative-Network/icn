#!/usr/bin/env python3
"""Independent reference implementation of GEN-A (icn#2695), for cross-implementation audit.

WHY THIS FILE EXISTS
--------------------
`gen_subject_context.rs` pins fixed expected bytes for every GEN-A derivation. A vector that the
Rust implementation generates and then compares against itself proves only that the code is
deterministic. It does not prove that the *specification* is sufficient for someone else to
reimplement -- which is the property a semantic-convergence primitive actually needs.

This file is that someone else. It is written from the prose in
`docs/architecture/GEN_SUBJECT_CONTEXT_GENESIS.md` alone and shares no code, no library and no
constant definition with `icn-identity`. It re-derives every value from first principles, then
reads the `EXPECT_*` literals out of the Rust test file and asserts byte equality.

The Rust literals are used ONLY as the comparison target. No value below is derived from them.

RUN IT
------
    python3 icn/crates/icn-identity/tests/reference/gen_subject_context_reference.py

Exits 0 when both implementations agree, 1 on any mismatch, 2 on a setup problem.
Requires `cryptography` (Ed25519 public-key derivation). Not wired into CI: it is an audit tool,
and adding a Python crypto dependency to the Rust test job would buy nothing the Rust tests do
not already assert.

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
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
except ImportError:  # pragma: no cover - setup problem, not a vector mismatch
    print("SETUP: this reference needs `cryptography` (pip install cryptography)", file=sys.stderr)
    sys.exit(2)

# --------------------------------------------------------------------------------------------
# Primitives, transcribed from the specification's "Primitives" section.
# --------------------------------------------------------------------------------------------
def lp(x: bytes) -> bytes:     return len(x).to_bytes(4, "big") + x   # LP(x) := u32be(len) || x
def u16(n: int) -> bytes:      return n.to_bytes(2, "big")
def u32(n: int) -> bytes:      return n.to_bytes(4, "big")
def u64(n: int) -> bytes:      return n.to_bytes(8, "big")
def h(b: bytes) -> bytes:      return hashlib.sha256(b).digest()
def principal(pk: bytes) -> bytes:  return b"\x01" + pk               # P(k)  := u8(0x01) || b32
def writer_set(pk: bytes) -> bytes: return u32(1) + principal(pk)     # PS(k) := u32be(1) || P(k)

def ed25519_pub(seed: bytes) -> bytes:
    return Ed25519PrivateKey.from_private_bytes(seed).public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw)

# --------------------------------------------------------------------------------------------
# Constants, transcribed from the specification's "Constants" table and N1's module docs.
# --------------------------------------------------------------------------------------------
N1_DOMAIN, N1_VERSION = b"icn.authority-log", 1
KDF_DOMAIN            = b"icn.authority-log.kdf"
COMMIT_DOMAIN         = b"icn.authority-log.commit"
KIND_INCEPTION, KIND_ROTATE, KIND_AUTHORIZE = 0x01, 0x02, 0x03
TERMINAL_COMMITMENT   = bytes(32)

GEN_CONTEXT_DOMAIN,  GEN_CONTEXT_VERSION  = b"icn.gen.subject-context", 1
SCR_DOMAIN,          SCR_VERSION          = b"icn.gen.subject-context-ref", 1
IDBR_DOMAIN,         IDBR_VERSION         = b"icn.gen.initial-device-binding-ref", 1
KIND_GOVERNANCE_DOMAIN_V1 = 0x01
CAP_SIGN, CAP_PRESENT     = 0x01, 0x03

# The spec states these ASCII lengths; check rather than trust the prose.
assert (len(GEN_CONTEXT_DOMAIN), len(SCR_DOMAIN), len(IDBR_DOMAIN)) == (23, 27, 34)

# --------------------------------------------------------------------------------------------
# Fixed vector inputs, from the specification's "Test vectors" table.
# --------------------------------------------------------------------------------------------
CONTEXT_ID   = "coop.example.governance"
CONTEXT_SALT = bytes(range(32))                          # salt[i]   = i
SECRET       = bytes((0x80 + i) & 0xFF for i in range(32))  # secret[i] = 0x80 + i
DEVICE_SEED  = bytes((0x40 + i) & 0xFF for i in range(32))  # seed[i]   = 0x40 + i
HORIZON      = 4                                          # plan = [Rotate; 4]

# --------------------------------------------------------------------------------------------
# Derivations
# --------------------------------------------------------------------------------------------
context_preimage = (lp(GEN_CONTEXT_DOMAIN) + u16(GEN_CONTEXT_VERSION)
                    + bytes([KIND_GOVERNANCE_DOMAIN_V1])
                    + lp(CONTEXT_ID.encode("utf-8")) + CONTEXT_SALT)
context_nonce = h(context_preimage)

def authority_pub(generation: int) -> bytes:
    """A_g = pubkey(SHA-256(LP(KDF_DOMAIN) || b32(nonce) || u64be(g) || b32(root)))."""
    return ed25519_pub(h(lp(KDF_DOMAIN) + context_nonce + u64(generation) + SECRET))

def commitment(generation: int) -> bytes:
    """C_g = H(LP(COMMIT) || u8(kind_g) || PS(A_g) || b32(C_{g+1})), folded from the horizon."""
    if generation == 0 or generation > HORIZON:
        return TERMINAL_COMMITMENT
    c = TERMINAL_COMMITMENT
    for g in range(HORIZON, generation - 1, -1):
        c = h(lp(COMMIT_DOMAIN) + bytes([KIND_ROTATE]) + writer_set(authority_pub(g)) + c)
    return c

a0, c1 = authority_pub(0), commitment(1)

inception_body = (lp(N1_DOMAIN) + u16(N1_VERSION) + bytes([KIND_INCEPTION])
                  + principal(a0) + context_nonce + writer_set(a0) + c1)
inception_event_id = h(inception_body)
subject_id = inception_event_id            # SubjectId = event_id(inception body)

device_pub = ed25519_pub(DEVICE_SEED)
header = subject_id + u64(1) + inception_event_id + principal(a0)
capabilities = u32(2) + bytes([CAP_SIGN]) + bytes([CAP_PRESENT])
validity_span = bytes([0x00])              # None
authorize_body = (lp(N1_DOMAIN) + u16(N1_VERSION) + bytes([KIND_AUTHORIZE])
                  + header + principal(device_pub) + capabilities + validity_span)
authorize_event_id = h(authorize_body)

subject_context_ref = h(lp(SCR_DOMAIN) + u16(SCR_VERSION)
                        + bytes([KIND_GOVERNANCE_DOMAIN_V1])
                        + lp(CONTEXT_ID.encode("utf-8")) + CONTEXT_SALT + inception_event_id)
initial_device_binding_ref = h(lp(IDBR_DOMAIN) + u16(IDBR_VERSION)
                               + subject_context_ref + authorize_event_id)

DERIVED = {
    "EXPECT_CONTEXT_PREIMAGE":            context_preimage,
    "EXPECT_CONTEXT_NONCE":               context_nonce,
    "EXPECT_INITIAL_AUTHORITY_PUB":       a0,
    "EXPECT_NEXT_COMMITMENT_C1":          c1,
    "EXPECT_DEVICE_PUB":                  device_pub,
    "EXPECT_INCEPTION_BODY":              inception_body,
    "EXPECT_SUBJECT_ID":                  subject_id,
    "EXPECT_AUTHORIZE_BODY":              authorize_body,
    "EXPECT_AUTHORIZE_EVENT_ID":          authorize_event_id,
    "EXPECT_SUBJECT_CONTEXT_REF":         subject_context_ref,
    "EXPECT_INITIAL_DEVICE_BINDING_REF":  initial_device_binding_ref,
}

# --------------------------------------------------------------------------------------------
# Compare against the Rust test's fixed literals. Comparison target only -- nothing above reads
# these, so agreement is evidence of two independent derivations meeting.
# --------------------------------------------------------------------------------------------
RUST_TEST = Path(__file__).resolve().parent.parent / "gen_subject_context.rs"

def rust_literals(path: Path) -> dict:
    if not path.is_file():
        print(f"SETUP: cannot find the Rust test at {path}", file=sys.stderr)
        sys.exit(2)
    source = path.read_text(encoding="utf-8")
    found = {}
    for name in DERIVED:
        m = re.search(rf'const {name}: &str =\s*"([0-9a-f]+)"', source)
        if m:
            found[name] = m.group(1)
    return found

def main() -> int:
    expected = rust_literals(RUST_TEST)
    width = max(len(n) for n in DERIVED)
    failures, missing = 0, 0

    print(f"GEN-A independent reference vs {RUST_TEST.name}\n")
    for name, value in DERIVED.items():
        mine = value.hex()
        theirs = expected.get(name)
        if theirs is None:
            print(f"  ?? {name:<{width}}  no literal found in the Rust test")
            missing += 1
        elif theirs == mine:
            print(f"  ok {name:<{width}}  {len(value):3d}B  {mine[:32]}…")
        else:
            failures += 1
            print(f"  XX {name:<{width}}  MISMATCH\n       reference: {mine}\n       rust:      {theirs}")

    print()
    if failures:
        print(f"FAIL: {failures} vector(s) disagree. Investigate the cause; do not edit either "
              f"side to make this pass.")
        return 1
    if missing:
        print(f"FAIL: {missing} literal(s) missing from the Rust test — the vectors are no "
              f"longer pinned there.")
        return 1
    print(f"OK: all {len(DERIVED)} vectors agree byte-for-byte between two independent "
          f"implementations.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
