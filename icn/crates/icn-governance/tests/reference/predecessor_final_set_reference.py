#!/usr/bin/env python3
"""Independent reference for the P5a final-set commitment F (S19 layout; EXPERIMENTAL, unratified).

F = SHA-256( lp("icn.governance.predecessor-final-set.v1") || u16be(1) || lp(L) || u64be(attempt)
             || K[32] || u32be(n) || lp(id)* ),  ids strictly ascending by UTF-8 bytes, 1 <= n.
Standard library only; shares no code with the Rust implementation.
"""
import hashlib
import struct

TAG = b"icn.governance.predecessor-final-set.v1"


def lp(data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + data


def final_set(l: str, attempt: int, k: bytes, ids) -> str:
    enc = [i.encode("utf-8") for i in ids]
    assert l and attempt >= 1 and len(k) == 32 and enc
    assert all(0 < len(i) <= 256 for i in enc)
    assert all(a < b for a, b in zip(enc, enc[1:])), "strictly ascending"
    pre = lp(TAG) + struct.pack(">H", 1) + lp(l.encode()) + struct.pack(">Q", attempt) + k
    pre += struct.pack(">I", len(enc)) + b"".join(lp(i) for i in enc)
    return hashlib.sha256(pre).hexdigest()


if __name__ == "__main__":
    k = bytes([0x4B]) * 32
    print("GOLDEN_F_ONE", final_set("legacy.coop.example", 1, k, ["cutover-proposal"]))
    print("GOLDEN_F_THREE", final_set("legacy.coop.example", 2, k,
                                      ["0f8c-a", "7b1e-b", "ñ-proposal"]))
