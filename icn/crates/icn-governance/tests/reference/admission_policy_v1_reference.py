#!/usr/bin/env python3
"""Independent reference for the V1 institutional admission-policy identity (design-spec Part 8 F3).

SHA-256(lp(tag) || lp(D) || C || u32be(n) || sorted rule_hash * n || u8 recognition)
lp(x) = u32be(len(x)) || x. Recognition tags (D-F3a): rehearsal 0x01, production 0x02.
Standard library only; shares no code with the Rust implementation.
"""
import hashlib
import struct

TAG = b"icn.governance.institutional-admission-policy.v1"
REHEARSAL, PRODUCTION = 0x01, 0x02


def lp(data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + data


def policy_id_v1(domain: str, context: bytes, rule_hashes, recognition: int) -> str:
    assert len(context) == 32 and all(len(r) == 32 for r in rule_hashes)
    assert recognition in (REHEARSAL, PRODUCTION)
    rules = sorted(rule_hashes)
    pre = lp(TAG) + lp(domain.encode()) + context + struct.pack(">I", len(rules))
    pre += b"".join(rules) + bytes([recognition])
    return hashlib.sha256(pre).hexdigest()


if __name__ == "__main__":
    d, c, r1, r2 = "icn.test.domain", bytes([0xC0]) * 32, bytes([0x11]) * 32, bytes([0x22]) * 32
    print("GOLDEN_REHEARSAL", policy_id_v1(d, c, [r2, r1], REHEARSAL))
    print("GOLDEN_PRODUCTION", policy_id_v1(d, c, [r1, r2], PRODUCTION))
    print("GOLDEN_SINGLE", policy_id_v1(d, c, [r1], REHEARSAL))
