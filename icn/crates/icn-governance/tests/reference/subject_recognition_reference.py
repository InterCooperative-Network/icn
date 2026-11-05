#!/usr/bin/env python3
"""Independent reference for GEN-B `SubjectRecognitionV1`.

Written from docs/architecture/GEN_SUBJECT_RECOGNITION.md alone. It imports nothing from ICN and
reads no Rust output to *derive* a value. Inputs are the published GEN-A §8 vector values
(SubjectId, subject_context_ref) plus the domain string, all stated in the GEN-B document.

The `EXPECT_*` literals in tests/subject_recognition.rs are read purely as a comparison target;
the script exits non-zero on any disagreement.

BLAKE3 is implemented here for inputs of at most one 1024-byte chunk (every GEN-B preimage is far
smaller), because the stdlib has no BLAKE3. Before use it is self-checked against the published
empty-input hash and against 10 pattern-input digests (lengths 1, 63, 64, 65, 127, 128, 129, 143,
1023, 1024) that cross block boundaries, cross-checked against the Rust `blake3` crate.

Run:  python3 icn/crates/icn-governance/tests/reference/subject_recognition_reference.py
"""
import os
import re
import struct
import sys

M32 = 0xFFFFFFFF
IV = [0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
      0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19]
PERM = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8]
CHUNK_START, CHUNK_END, ROOT = 1, 2, 8


def rotr(x, n):
    return ((x >> n) | (x << (32 - n))) & M32


def g(v, a, b, c, d, mx, my):
    v[a] = (v[a] + v[b] + mx) & M32
    v[d] = rotr(v[d] ^ v[a], 16)
    v[c] = (v[c] + v[d]) & M32
    v[b] = rotr(v[b] ^ v[c], 12)
    v[a] = (v[a] + v[b] + my) & M32
    v[d] = rotr(v[d] ^ v[a], 8)
    v[c] = (v[c] + v[d]) & M32
    v[b] = rotr(v[b] ^ v[c], 7)


def compress(cv, m, counter, block_len, flags):
    v = list(cv) + IV[:4] + [counter & M32, (counter >> 32) & M32, block_len, flags]
    m = list(m)
    for r in range(7):
        g(v, 0, 4, 8, 12, m[0], m[1])
        g(v, 1, 5, 9, 13, m[2], m[3])
        g(v, 2, 6, 10, 14, m[4], m[5])
        g(v, 3, 7, 11, 15, m[6], m[7])
        g(v, 0, 5, 10, 15, m[8], m[9])
        g(v, 1, 6, 11, 12, m[10], m[11])
        g(v, 2, 7, 8, 13, m[12], m[13])
        g(v, 3, 4, 9, 14, m[14], m[15])
        if r < 6:
            m = [m[p] for p in PERM]
    return [v[i] ^ v[i + 8] for i in range(8)]


def blake3(data: bytes) -> bytes:
    assert len(data) <= 1024, "single-chunk reference only"
    blocks = [data[i:i + 64] for i in range(0, len(data), 64)] or [b""]
    cv = list(IV)
    for i, blk in enumerate(blocks):
        flags = 0
        if i == 0:
            flags |= CHUNK_START
        if i == len(blocks) - 1:
            flags |= CHUNK_END | ROOT
        padded = blk + b"\x00" * (64 - len(blk))
        words = struct.unpack("<16I", padded)
        cv = compress(cv, words, 0, len(blk), flags)
    return struct.pack("<8I", *cv)


# ---- GEN-B specification (docs/architecture/GEN_SUBJECT_RECOGNITION.md §4-§6) -------------------

RECOGNITION_TAG = b"icn:gov:subject-recognition:v1"
BASIS_TAG = b"icn:gov:recognition-basis:bootstrap-rehearsal:v1"
STANDING_MEMBER = 0x01
BASIS_BOOTSTRAP_REHEARSAL_V1 = 0x01


def u64le(n):
    return struct.pack("<Q", n)


def field_bytes(b):
    return u64le(len(b)) + b


def basis_preimage(domain: bytes, subject: bytes, context_ref: bytes) -> bytes:
    assert len(subject) == 32 and len(context_ref) == 32
    return BASIS_TAG + field_bytes(domain) + subject + context_ref


def recognition_preimage(domain, subject, standing, kind, basis_hash) -> bytes:
    assert len(subject) == 32 and len(basis_hash) == 32
    return (RECOGNITION_TAG + field_bytes(domain) + subject
            + bytes([standing]) + bytes([kind]) + basis_hash)


# ---- Inputs: GEN-A §8 published values + GEN-B domain strings -----------------------------------

DOMAIN_A = b"coop.example.governance"
DOMAIN_B = b"coop.example.other"
SUBJECT = bytes.fromhex("3b0e4c9ed8634dc5e3ed4a47ef09059fb99c89a0ac61a1ee6812894b8a190f5a")
CONTEXT_REF = bytes.fromhex("5e5f0deeb4cc8e0f1616a06509b7042828a85914597fad2b95d6a77f695faf86")


def derive(domain):
    bp = basis_preimage(domain, SUBJECT, CONTEXT_REF)
    bh = blake3(bp)
    rp = recognition_preimage(domain, SUBJECT, STANDING_MEMBER, BASIS_BOOTSTRAP_REHEARSAL_V1, bh)
    return bp, bh, rp, blake3(rp)


BLAKE3_PATTERN_VECTORS = [
    (1, "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213"),
    (63, "e9bc37a594daad83be9470df7f7b3798297c3d834ce80ba85d6e207627b7db7b"),
    (64, "4eed7141ea4a5cd4b788606bd23f46e212af9cacebacdc7d1f4c6dc7f2511b98"),
    (65, "de1e5fa0be70df6d2be8fffd0e99ceaa8eb6e8c93a63f2d8d1c30ecb6b263dee"),
    (127, "d81293fda863f008c09e92fc382a81f5a0b4a1251cba1634016a0f86a6bd640d"),
    (128, "f17e570564b26578c33bb7f44643f539624b05df1a76c81f30acd548c44b45ef"),
    (129, "683aaae9f3c5ba37eaaf072aed0f9e30bac0865137bae68b1fde4ca2aebdcb12"),
    (143, "c6f7010b6eeae3b1dbe83b6322c26d9de8603cec82a7b24e656d2b284406e91e"),
    (1023, "10108970eeda3eb932baac1428c7a2163b0e924c9a9e25b35bba72b28f70bd11"),
    (1024, "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7"),
]


def main():
    # Self-check of the BLAKE3 implementation against the published empty-input hash.
    assert blake3(b"").hex() == \
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262", "blake3 self-check"
    # Multi-block self-check. The GEN-B preimages are 143 B and 127 B, i.e. 3 and 2 blocks, so the
    # empty-input hash alone would not exercise block chaining or the CHUNK_START/CHUNK_END/ROOT
    # flag placement. Input is byte i = i % 251 (the BLAKE3 test-vector pattern); each digest was
    # cross-checked against the Rust `blake3` crate 1.8.7 (a separate implementation) for every
    # length 0..=1024 with zero mismatches.
    for n, want in BLAKE3_PATTERN_VECTORS:
        got = blake3(bytes(i % 251 for i in range(n))).hex()
        assert got == want, "blake3 self-check len=%d" % n

    bp_a, bh_a, rp_a, rh_a = derive(DOMAIN_A)
    _, bh_b, _, rh_b = derive(DOMAIN_B)
    values = {
        "EXPECT_BASIS_PREIMAGE": bp_a.hex(),
        "EXPECT_BASIS_HASH": bh_a.hex(),
        "EXPECT_RECOGNITION_PREIMAGE": rp_a.hex(),
        "EXPECT_RECOGNITION_HASH": rh_a.hex(),
        "EXPECT_OTHER_DOMAIN_BASIS_HASH": bh_b.hex(),
        "EXPECT_OTHER_DOMAIN_RECOGNITION_HASH": rh_b.hex(),
    }
    here = os.path.dirname(os.path.abspath(__file__))
    rust = open(os.path.join(here, "..", "subject_recognition.rs"), encoding="utf-8").read()
    ok = True
    for name, val in values.items():
        m = re.search(r"const %s: &str =\s*\"([0-9a-f]+)\"" % name, rust)
        if not m:
            print("MISSING in rust test:", name)
            ok = False
            continue
        status = "ok" if m.group(1) == val else "MISMATCH"
        ok &= status == "ok"
        print("%-40s %s" % (name, status))
        if status != "ok" or "--print" in sys.argv:
            print("   python:", val)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
