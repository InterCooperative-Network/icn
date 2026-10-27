#!/usr/bin/env python3
"""Independent, fixture-only ADR-0089 §3 history-segment v1 reference vectors.

The bytes of N1 bodies and signatures are derived from the documented N1
canonical framing and fixed 32-byte test seeds; NO Rust implementation is used.
No disk/network storage or operational keys. This is a vector producer and
self-test, not an import, authority verdict, or completeness proof.

Usage: python3 history_segment_reference.py [--write PATH]
Dependencies: Python >=3.9 and cryptography (for deterministic Ed25519).
"""
import argparse
import hashlib
import json
import struct
from pathlib import Path
from cryptography.hazmat.primitives import serialization
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

# Protocol constants from N1 canonical framing and ADR-0089 §3.
N1_DOMAIN = b"icn.authority-log"
N1_SIG = b"icn.authority-log.sig"
N1_COMMIT = b"icn.authority-log.commit"
N1_KDF = b"icn.authority-log.kdf"
SEGMENT_DOMAIN = b"icn.n1.history-segment"
VERSION = 1
MAX_POSITION = 1_048_576
MAX_BYTES, MAX_FACTS, MAX_WITNESSES = 1 << 20, 4096, 16
INCEPTION, ROTATE, AUTHORIZE, REVOKE = 1, 2, 3, 4
SIGN = 1


def u16(x): return struct.pack(">H", x)
def u32(x): return struct.pack(">I", x)
def u64(x): return struct.pack(">Q", x)
def lp(x): return u32(len(x)) + x
def digest(x): return hashlib.sha256(x).digest()
def principal(pk): return b"\x01" + pk
def singleton(pk): return u32(1) + principal(pk)
def pub(key):
    return key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)


# Pure test inputs. Subject = SHA256(canonical inception body).
ROOT_SECRET = bytes([0x31] * 32)
NONCE = bytes([0x31 ^ 0xa5] * 32)
DEVICE_A = Ed25519PrivateKey.from_private_bytes(bytes([0x41] * 32))
DEVICE_B = Ed25519PrivateKey.from_private_bytes(bytes([0x42] * 32))
HORIZON = 4


def authority_key(g):
    return Ed25519PrivateKey.from_private_bytes(digest(lp(N1_KDF) + NONCE + u64(g) + ROOT_SECRET))


def next_commitment():
    c = bytes(32)  # terminal commitment after the fourth generation
    for g in range(HORIZON, 0, -1):
        c = digest(lp(N1_COMMIT) + bytes([ROTATE]) + singleton(pub(authority_key(g))) + c)
    return c


def prefix(kind): return lp(N1_DOMAIN) + u16(1) + bytes([kind])
def inception():
    p = pub(authority_key(0))
    return prefix(INCEPTION) + principal(p) + NONCE + singleton(p) + next_commitment()


def header(subject, position, prev):
    return subject + u64(position) + prev + principal(pub(authority_key(0)))


def authorize(subject, position, prev, device):
    return prefix(AUTHORIZE) + header(subject, position, prev) + principal(pub(device)) + u32(1) + bytes([SIGN, 0])


def revoke(subject, position, prev, device):
    return prefix(REVOKE) + header(subject, position, prev) + principal(pub(device))


def witness(body): return authority_key(0).sign(lp(N1_SIG) + body)


def fixtures():
    b0 = inception()
    subject = digest(b0)
    b1 = authorize(subject, 1, digest(b0), DEVICE_A)
    b2 = authorize(subject, 2, digest(b1), DEVICE_B)
    b3 = revoke(subject, 3, digest(b2), DEVICE_A)
    # Entry: event_id, body, [(signature)] -- one witness for each fact.
    return subject, [(digest(b), b, (witness(b),)) for b in (b0, b1, b2, b3)]


def encode(subject, start, end, entries):
    raw = lp(SEGMENT_DOMAIN) + u16(VERSION) + subject + u64(start) + u64(end) + u32(len(entries))
    for eid, body, sigs in sorted(entries, key=lambda e: e[0]):
        raw += eid + lp(body) + u32(len(sigs)) + b"".join(sorted(sigs))
    assert len(raw) <= MAX_BYTES
    return raw


class Refused(ValueError): pass


def decode(raw):
    """Strict container/structure decoder, NOT N1 signature admission."""
    if len(raw) > MAX_BYTES:
        raise Refused("oversize")
    i = 0
    def take(n):
        nonlocal i
        if n > len(raw) - i: raise Refused("truncated")
        b = raw[i:i+n]
        i += n
        return b
    def num(n): return int.from_bytes(take(n), "big")
    def read_lp(): return take(num(4))

    if read_lp() != SEGMENT_DOMAIN: raise Refused("domain")
    if num(2) != VERSION: raise Refused("version")
    subject, start, end = take(32), num(8), num(8)
    if not 0 <= start <= end <= MAX_POSITION: raise Refused("range")
    count = num(4)
    if count > MAX_FACTS or count * (32 + 4 + 4 + 64) > len(raw) - i:
        raise Refused("fact count")
    positions = set()
    bodies = {}
    result = []
    previous = None
    for _ in range(count):
        eid, body, n = take(32), read_lp(), num(4)
        if previous is not None and eid <= previous: raise Refused("fact ordering")
        previous = eid
        if eid != digest(body): raise Refused("event digest")
        if n == 0 or n > MAX_WITNESSES or n * 64 > len(raw) - i:
            raise Refused("witness count")
        sigs = tuple(take(64) for _ in range(n))
        if tuple(sorted(set(sigs))) != sigs: raise Refused("witness ordering")
        # For the reference fixtures the N1 prefix, kind, and body locations
        # are reconstructed independently from N1's documented wire layout.
        base = lp(N1_DOMAIN) + u16(1)
        if not body.startswith(base) or len(body) < len(base) + 1:
            raise Refused("N1 framing")
        kind = body[len(base)]
        if kind == INCEPTION:
            pos, prev, bsubject = 0, None, eid
        elif kind in (ROTATE, AUTHORIZE, REVOKE):
            offset = len(base) + 1
            if len(body) < offset + 32 + 8 + 32:
                raise Refused("N1 header")
            bsubject = body[offset:offset+32]
            pos = int.from_bytes(body[offset+32:offset+40], "big")
            prev = body[offset+40:offset+72]
        else:
            raise Refused("N1 kind")
        if bsubject != subject: raise Refused("subject")
        if not start <= pos <= end: raise Refused("position")
        positions.add(pos)
        bodies.setdefault(pos, set()).add(eid)
        result.append((eid, body, sigs, pos, prev))
    if i != len(raw): raise Refused("trailing bytes")
    if positions != set(range(start, end + 1)):
        raise Refused("position gap")
    if start == 0 and not any(p == 0 and eid == subject for eid, _, _, p, _ in result):
        raise Refused("inception missing")
    for eid, _, _, pos, prev in result:
        if pos > start and prev not in bodies[pos-1]:
            raise Refused("predecessor not carried")
    return {"subject": subject, "from": start, "to": end, "facts": result}


def admit(decoded):
    """Cryptographic N1 admission, distinct from decode, no authority claim."""
    # This standalone reference recognizes the fixture writer's inline signing
    # key for its four generated bodies; the Rust implementation must instead
    # call canonical N1 admission for *every* body/witness pair.
    for _, body, sigs, _, _ in decoded["facts"]:
        for sig in sigs:
            authority_key(0).public_key().verify(sig, lp(N1_SIG) + body)
    return True


def generate():
    subject, events = fixtures()
    full = encode(subject, 0, 3, events)
    suffix = encode(subject, 1, 3, events[1:])
    for b in (full, suffix):
        admit(decode(b))
    # Fixed reference outputs, shared with the review issue. Keep these pinned.
    assert subject.hex() == "a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d"
    assert digest(full).hex() == "a3b8192c152d3e710a8a3b15c41fb10240340c5f98840506d15cb5ca40456608"
    assert digest(suffix).hex() == "f320220a862f9e626da7edb0837e18198539ea27f04919d94441cc61897468ba"
    # Reference-level negative controls: a mutation must be refused, either by
    # structural decode or cryptographic admission, never silently accepted.
    assert_raises(lambda: decode(full + b"\x00"))
    assert_raises(lambda: decode(full[:-1]))
    assert_raises(lambda: decode(full[:4] + bytes([full[4] ^ 1]) + full[5:]))
    assert_raises(lambda: decode(encode(subject, 0, 3, events[:2] + events[3:])))
    ev = list(events)
    altered = ev[1][2][0][:-1] + bytes([ev[1][2][0][-1] ^ 1])
    ev[1] = (ev[1][0], ev[1][1], (altered,))
    assert_raises(lambda: admit(decode(encode(subject, 0, 3, ev))))
    return {
        "description": "ADR-0089 facts-only, fixture data; decoded != admitted != authorized/complete/current",
        "subject_hex": subject.hex(),
        "full_0_3": {"bytes_hex": full.hex(), "segment_id_hex": digest(full).hex(), "facts": 4},
        "suffix_1_3": {"bytes_hex": suffix.hex(), "segment_id_hex": digest(suffix).hex(), "facts": 3,
                        "note": "requires an external position-0 anchor at import; no import is performed"},
        "event_ids_hex_by_position": [e[0].hex() for e in events],
    }


def assert_raises(operation):
    try:
        operation()
    except (Refused, InvalidSignature):
        pass
    else:
        raise AssertionError("invalid fixture was accepted")


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--write", type=Path, help="write verified JSON vectors")
    args = p.parse_args()
    output = generate()
    rendered = json.dumps(output, sort_keys=True, indent=2) + "\n"
    if args.write:
        args.write.write_text(rendered)
    print(f"PASS ADR-0089 fixture vectors: subject={output['subject_hex'][:16]}…; "
          f"full={output['full_0_3']['segment_id_hex']}; suffix={output['suffix_1_3']['segment_id_hex']}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
