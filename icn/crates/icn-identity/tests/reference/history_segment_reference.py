#!/usr/bin/env python3
"""Independent reference implementation of the N1 history segment, for cross-implementation audit.

WHY THIS FILE EXISTS
--------------------
`tests/history_segment.rs` pins fixed expected bytes for three segments. A vector the Rust code
generates and then compares against itself proves only that the code is deterministic. It does not
prove that the *specification* is sufficient for someone else to reimplement, which is what a
segment handed between operators needs: an implementation in another language must be able to
produce, and more importantly refuse, these exact bytes from the prose alone.

This file is that someone else. It is written from the prose in
`docs/adr/ADR-0089-n1-offline-authority-fact-import.md` §3 (layout, validity rules 1-7) and the N1
canonical-encoding contract it cites (`icn/crates/icn-identity/src/authority_log/mod.rs` §1-§2 /
HUMAN_IDENTITY_ARCHITECTURE.md §9.2.1). It shares no code, no library and no constant definition
with `icn-identity`, and none with the other reference scripts beside it. Every byte -- the N1
bodies, the witnesses, the segments and their ids -- is derived here from the stated seed inputs.
It then reads the `EXPECT_*` literals out of the Rust test file and asserts byte equality.

The Rust literals are used ONLY as the comparison target. No value below is derived from them.

RUN IT
------
    python3 -I icn/crates/icn-identity/tests/reference/history_segment_reference.py

Exits 0 when both implementations agree, 1 on any mismatch, 2 on a setup problem.
Requires `cryptography` (Ed25519 key derivation, signing and verification). Not wired into CI: it
is an audit tool, like the other references in this directory.

The strict decoder below applies rules 1-6 and verifies every witness under the body's inline
signer, which is the signature part of N1 admission (rule 7). It does not re-implement the rest of
N1 body validation (key canonicity, set ordering inside bodies); the Rust side delegates that to
N1's own decoder, and the vectors here only use bodies that both sides construct identically.

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


# --- framing primitives, N1 §1 ---------------------------------------------------------------
def lp(x: bytes) -> bytes:   return len(x).to_bytes(4, "big") + x      # LP(x) := u32be(len) || x
def u8(n: int) -> bytes:     return n.to_bytes(1, "big")
def u16(n: int) -> bytes:    return n.to_bytes(2, "big")
def u32(n: int) -> bytes:    return n.to_bytes(4, "big")
def u64(n: int) -> bytes:    return n.to_bytes(8, "big")
def h(b: bytes) -> bytes:    return hashlib.sha256(b).digest()
def P(pk: bytes) -> bytes:   return b"\x01" + pk                       # tagged Ed25519 principal
def PS(pk: bytes) -> bytes:  return u32(1) + P(pk)                     # exactly one logical writer


def pub(key: Ed25519PrivateKey) -> bytes:
    return key.public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw
    )


# --- N1 constants, mod.rs ---------------------------------------------------------------------
N1_DOMAIN = b"icn.authority-log"
N1_SIG_DOMAIN = b"icn.authority-log.sig"
N1_COMMIT_DOMAIN = b"icn.authority-log.commit"
N1_KDF_DOMAIN = b"icn.authority-log.kdf"
N1_VERSION = 1
KIND_INCEPTION, KIND_ROTATE, KIND_AUTHORIZE, KIND_REVOKE = 0x01, 0x02, 0x03, 0x04
CAP_SIGN = 0x01
TERMINAL_COMMITMENT = bytes(32)
MAX_POSITION = 1_048_576

# --- segment constants, ADR-0089 §3 and §8 ----------------------------------------------------
SEGMENT_DOMAIN = b"icn.n1.history-segment"
SEGMENT_VERSION = 1
MAX_SEGMENT_BYTES = 1 << 20
MAX_SEGMENT_FACTS = 4096
MAX_WITNESSES_PER_FACT = 16


# --- vector inputs, stated in full in the Rust test -------------------------------------------
SUBJECT_SEED = 0x31
ROOT_SECRET = bytes([SUBJECT_SEED] * 32)
CONTEXT_NONCE = bytes([SUBJECT_SEED ^ 0xA5] * 32)   # the test fixture's nonce rule, stated as data
HORIZON = 4                                         # four Rotate generations armed
DEVICE_A_SEED = bytes([0x41] * 32)
DEVICE_B_SEED = bytes([0x42] * 32)
DEVICE_C_SEED = bytes([0x43] * 32)


# --- N1 construction, from the prose ----------------------------------------------------------
def authority_key(generation: int) -> Ed25519PrivateKey:
    # seed = SHA-256(LP(KDF_DOMAIN) || b32(nonce) || u64be(g) || b32(root))
    return Ed25519PrivateKey.from_private_bytes(
        h(lp(N1_KDF_DOMAIN) + CONTEXT_NONCE + u64(generation) + ROOT_SECRET)
    )


def commitment(generation: int) -> bytes:
    # C_{horizon+1} = TERMINAL; C_g = H(LP(COMMIT_DOMAIN) || u8(kind_g) || PS(A_g) || C_{g+1})
    if generation == 0 or generation > HORIZON:
        return TERMINAL_COMMITMENT
    c = TERMINAL_COMMITMENT
    for g in range(HORIZON, generation - 1, -1):
        c = h(lp(N1_COMMIT_DOMAIN) + u8(KIND_ROTATE) + PS(pub(authority_key(g))) + c)
    return c


def body_prefix(kind: int) -> bytes:
    return lp(N1_DOMAIN) + u16(N1_VERSION) + u8(kind)


def inception_body() -> bytes:
    a0 = pub(authority_key(0))
    return body_prefix(KIND_INCEPTION) + P(a0) + CONTEXT_NONCE + PS(a0) + commitment(1)


def header(subject: bytes, position: int, prev: bytes, signer: bytes) -> bytes:
    return subject + u64(position) + prev + P(signer)


def authorize_body(subject: bytes, position: int, prev: bytes, signer: bytes, device: bytes) -> bytes:
    # CS({Sign}) = u32be(1) || u8(0x01); SPAN absent = u8(0x00)
    return body_prefix(KIND_AUTHORIZE) + header(subject, position, prev, signer) + P(device) \
        + u32(1) + u8(CAP_SIGN) + u8(0x00)


def revoke_body(subject: bytes, position: int, prev: bytes, signer: bytes, device: bytes) -> bytes:
    return body_prefix(KIND_REVOKE) + header(subject, position, prev, signer) + P(device)


def witness(body: bytes, key: Ed25519PrivateKey) -> bytes:
    # signature preimage = LP(SIGNATURE_DOMAIN) || canonical_body
    return key.sign(lp(N1_SIG_DOMAIN) + body)


# --- reading the few body fields the segment rules need, from the N1 layout --------------------
KIND_OFFSET = 4 + len(N1_DOMAIN) + 2
HDR_OFFSET = KIND_OFFSET + 1


def body_kind(body: bytes) -> int:
    return body[KIND_OFFSET]


def body_subject(body: bytes) -> bytes:
    # An inception carries no subject field: its subject is its own digest.
    return h(body) if body_kind(body) == KIND_INCEPTION else body[HDR_OFFSET:HDR_OFFSET + 32]


def body_position(body: bytes) -> int:
    if body_kind(body) == KIND_INCEPTION:
        return 0
    return int.from_bytes(body[HDR_OFFSET + 32:HDR_OFFSET + 40], "big")


def body_prev(body: bytes):
    if body_kind(body) == KIND_INCEPTION:
        return None
    return body[HDR_OFFSET + 40:HDR_OFFSET + 72]


def body_signer(body: bytes) -> bytes:
    # inception: P(signer) right after the kind byte; others: after subject||position||prev.
    at = HDR_OFFSET if body_kind(body) == KIND_INCEPTION else HDR_OFFSET + 72
    assert body[at] == 0x01, "Ed25519 principal tag"
    return body[at + 1:at + 33]


# --- the segment, from ADR-0089 §3 ------------------------------------------------------------
def segment(subject: bytes, frm: int, to: int, facts: dict) -> bytes:
    """facts: {body_bytes: [signature, ...]}. Emitted ascending by event_id, witnesses ascending."""
    out = lp(SEGMENT_DOMAIN) + u16(SEGMENT_VERSION) + subject + u64(frm) + u64(to) + u32(len(facts))
    for body in sorted(facts, key=h):
        sigs = sorted(set(facts[body]))
        out += h(body) + lp(body) + u32(len(sigs)) + b"".join(sigs)
    return out


class Refused(Exception):
    pass


def strict_decode(buf: bytes) -> dict:
    """Rules 1-6, plus rule 7's signature verification. Returns {event_id: (body, [sig])}."""
    if len(buf) > MAX_SEGMENT_BYTES:
        raise Refused("too large")
    pos = 0

    def take(n: int) -> bytes:
        nonlocal pos
        if pos + n > len(buf):
            raise Refused("truncated")
        out = buf[pos:pos + n]
        pos += n
        return out

    def count(min_len: int) -> int:
        n = int.from_bytes(take(4), "big")
        if n * min_len > len(buf) - pos:
            raise Refused("count overflow")
        return n

    if take(int.from_bytes(take(4), "big")) != SEGMENT_DOMAIN:
        raise Refused("bad domain")
    if int.from_bytes(take(2), "big") != SEGMENT_VERSION:
        raise Refused("bad version")
    subject = take(32)
    frm = int.from_bytes(take(8), "big")
    to = int.from_bytes(take(8), "big")
    if not (frm <= to <= MAX_POSITION):
        raise Refused("range")                                         # rule 1
    n = count(32 + 4 + 4)
    if n > MAX_SEGMENT_FACTS:
        raise Refused("too many facts")
    facts = {}
    prev_id = None
    for _ in range(n):
        stated = take(32)
        if prev_id is not None and stated <= prev_id:
            raise Refused("fact order")                                # rule 6
        prev_id = stated
        body = take(int.from_bytes(take(4), "big"))
        if h(body) != stated:
            raise Refused("event_id mismatch")                         # rule 2
        if body_subject(body) != subject:
            raise Refused("fact subject mismatch")                     # rule 2
        if not (frm <= body_position(body) <= to):
            raise Refused("fact outside range")                        # rule 3
        w = count(64)
        if w == 0:
            raise Refused("no witness")                                # rule 6
        if w > MAX_WITNESSES_PER_FACT:
            raise Refused("too many witnesses")
        sigs = [take(64) for _ in range(w)]
        if any(b <= a for a, b in zip(sigs, sigs[1:])):
            raise Refused("witness order")                             # rule 6
        facts[stated] = (body, sigs)
    if pos != len(buf):
        raise Refused("trailing bytes")

    if frm == 0 and subject not in facts:
        raise Refused("missing inception")                             # rule 5
    covered = {body_position(b) for b, _ in facts.values()}
    for p in range(frm, to + 1):
        if p not in covered:
            raise Refused("continuity gap")                            # rule 4
    for b, _ in facts.values():
        p = body_position(b)
        if p == frm:
            continue
        parent = facts.get(body_prev(b))
        if parent is None or body_position(parent[0]) != p - 1:
            raise Refused("broken linkage")                            # rule 5
    for b, sigs in facts.values():                                     # rule 7, signature part
        key = Ed25519PublicKey.from_public_bytes(body_signer(b))
        for s in sigs:
            try:
                key.verify(s, lp(N1_SIG_DOMAIN) + b)
            except InvalidSignature:
                raise Refused("witness does not verify")
    return facts


def rust_literals(path: Path) -> dict:
    text = path.read_text()
    found = {}
    for name, value in re.findall(r'const (EXPECT_[A-Z0-9_]+): &str\s*=\s*"([0-9a-f]+)";', text):
        found[name] = value
    return found


def main() -> int:
    here = Path(__file__).resolve()
    rust_test = here.parent.parent / "history_segment.rs"
    if not rust_test.is_file():
        print(f"setup: cannot find {rust_test}", file=sys.stderr)
        return 2
    expect = rust_literals(rust_test)

    k0 = authority_key(0)
    k0_pub = pub(k0)
    a_pub, b_pub, c_pub = (pub(Ed25519PrivateKey.from_private_bytes(s))
                           for s in (DEVICE_A_SEED, DEVICE_B_SEED, DEVICE_C_SEED))

    inception = inception_body()
    subject = h(inception)                                  # SubjectId = event_id(inception)
    auth_a = authorize_body(subject, 1, h(inception), k0_pub, a_pub)
    auth_b = authorize_body(subject, 2, h(auth_a), k0_pub, b_pub)
    revoke_a = revoke_body(subject, 3, h(auth_b), k0_pub, a_pub)
    auth_c = authorize_body(subject, 2, h(auth_a), k0_pub, c_pub)   # a second candidate at 2

    def signed(*bodies):
        return {b: [witness(b, k0)] for b in bodies}

    vectors = {
        # 1. inception only: from == to == 0.
        "INCEPTION": segment(subject, 0, 0, signed(inception)),
        # 2. a linear excerpt that does not start at inception: 1..=3.
        "LINEAR": segment(subject, 1, 3, signed(auth_a, auth_b, revoke_a)),
        # 3. a fork at position 2, both candidates carried: 0..=2.
        "FORK": segment(subject, 0, 2, signed(inception, auth_a, auth_b, auth_c)),
    }

    ok = True

    def compare(name: str, value: str) -> None:
        nonlocal ok
        pinned = expect.get(name)
        if pinned is None:
            print(f"MISSING  {name} (reference derives {value})")
            ok = False
        elif pinned != value:
            print(f"MISMATCH {name}\n  rust      = {pinned}\n  reference = {value}")
            ok = False
        else:
            print(f"ok       {name}")

    compare("EXPECT_SUBJECT_ID_HEX", subject.hex())
    for label, data in vectors.items():
        compare(f"EXPECT_{label}_SEGMENT_HEX", data.hex())
        compare(f"EXPECT_{label}_SEGMENT_ID_HEX", h(data).hex())
        try:
            facts = strict_decode(data)
            print(f"ok       independent strict decode accepts {label} ({len(facts)} facts)")
        except Refused as exc:
            print(f"MISMATCH independent strict decode refused {label}: {exc}")
            ok = False

    fork = strict_decode(vectors["FORK"])
    at_two = sorted(b for b, _ in fork.values() if body_position(b) == 2)
    if at_two == sorted([auth_b, auth_c]):
        print("ok       both fork candidates at position 2 survive decode")
    else:
        print("MISMATCH fork candidates were not both carried")
        ok = False

    def must_refuse(label: str, mutated: bytes, reason: str) -> None:
        nonlocal ok
        try:
            strict_decode(mutated)
        except Refused as exc:
            if str(exc) == reason:
                print(f"ok       refuses {label} ({exc})")
            else:
                print(f"MISMATCH {label}: refused for '{exc}', expected '{reason}'")
                ok = False
            return
        print(f"MISMATCH {label} was accepted")
        ok = False

    linear = vectors["LINEAR"]
    must_refuse("trailing byte", linear + b"\x00", "trailing bytes")
    # One byte short, the last witness count can no longer be held: refused before reading.
    must_refuse("truncation", linear[:-1], "count overflow")
    must_refuse("truncation inside the header", linear[:20], "truncated")
    must_refuse("wrong domain", lp(b"icn.n4.evidence-bundle") + linear[4 + len(SEGMENT_DOMAIN):],
                "bad domain")
    must_refuse("wrong version", linear.replace(lp(SEGMENT_DOMAIN) + u16(1), lp(SEGMENT_DOMAIN) + u16(2), 1),
                "bad version")
    must_refuse("from > to", segment(subject, 3, 1, signed(auth_a)), "range")
    must_refuse("to > MAX_POSITION", segment(subject, 1, MAX_POSITION + 1, signed(auth_a)), "range")
    must_refuse("fact outside range", segment(subject, 1, 2, signed(auth_a, auth_b, revoke_a)),
                "fact outside range")
    must_refuse("gap in continuity", segment(subject, 1, 3, signed(auth_a, revoke_a)), "continuity gap")
    unlinked = authorize_body(subject, 2, h(b"not a carried fact"), k0_pub, b_pub)
    must_refuse("broken linkage", segment(subject, 0, 2, signed(inception, auth_a, unlinked)),
                "broken linkage")
    must_refuse("from 0 without inception", segment(subject, 0, 1, signed(auth_a)), "missing inception")
    must_refuse("oversized", b"\x00" * (MAX_SEGMENT_BYTES + 1), "too large")
    bad = {auth_a: [witness(auth_a, k0)], auth_b: [bytes([0xFF]) * 64], revoke_a: [witness(revoke_a, k0)]}
    must_refuse("witness that does not verify", segment(subject, 1, 3, bad), "witness does not verify")

    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
