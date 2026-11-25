#!/usr/bin/env python3
"""Independent reference for the P4 cutover commitment (S17 layout; EXPERIMENTAL, unratified).

R = SHA-256(lp("icn.governance.cutover-roster.v1") || u16be(1) || C || u32be(n)
            || for each pair: subject[32] || lp(role_utf8))
canonical = lp("icn.governance.cutover.v1") || u16be(1) || lp(L) || lp(S) || C || h1 || R || M
commitment = SHA-256(canonical)
Standard library only; shares no code with the Rust implementation.
"""
import hashlib
import struct

ROSTER_TAG = b"icn.governance.cutover-roster.v1"
CUTOVER_TAG = b"icn.governance.cutover.v1"


def lp(data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + data


def roster(context: bytes, standing) -> bytes:
    assert len(context) == 32 and len(standing) <= 1024
    pairs = [(s, r.encode("utf-8")) for s, r in standing]
    assert all(len(s) == 32 and len(r) <= 64 for s, r in pairs)
    assert all(a < b for a, b in zip(pairs, pairs[1:])), "strictly ascending"
    pre = lp(ROSTER_TAG) + struct.pack(">H", 1) + context + struct.pack(">I", len(pairs))
    for subject, role in pairs:
        pre += subject + lp(role)
    return hashlib.sha256(pre).digest()


def canonical(l: str, s: str, context: bytes, h1: bytes, r: bytes, m: bytes) -> bytes:
    return (lp(CUTOVER_TAG) + struct.pack(">H", 1) + lp(l.encode()) + lp(s.encode())
            + context + h1 + r + m)


if __name__ == "__main__":
    c = bytes([0xC1]) * 32
    a, b = bytes([0x01]) * 32, bytes([0x02]) * 32
    r0 = roster(c, [])
    r1 = roster(c, [(a, "member")])
    r3 = roster(c, [(a, "elector"), (a, "member"), (b, "socia-ñ")])
    print("GOLDEN_R_EMPTY", r0.hex())
    print("GOLDEN_R_ONE", r1.hex())
    print("GOLDEN_R_THREE", r3.hex())
    enc = canonical("legacy.coop.example", "successor.coop.example", c, bytes([0xB1]) * 32, r3,
                    bytes([0x4D]) * 32)
    print("GOLDEN_ENCODING", enc.hex())
    print("GOLDEN_COMMITMENT", hashlib.sha256(enc).hexdigest())
