#!/usr/bin/env python3
"""Independent reference for the P5b slice 1a lifecycle record, witness, keys and basis
(EXPERIMENTAL, unratified). Standard library only; shares no code with the Rust implementation.
The cutover commitment C is built with this directory's own P4 reference
(cutover_commitment_reference.py), which is itself independent of Rust.

lp(x) = u32be(len x) || x
record  = lp(TAG) || u16be(1) || lp(L) || u64be(revision) || u8(state) || body
  0x01 Legacy : u64be(last_attempt) || u8(1) || aborted_K            (stored Legacy always aborted)
  0x02 Pending: u64be(attempt) || lp(C) || K || basis || E
  0x03 Sealed : u64be(attempt) || lp(C) || K || basis || E || F
witness = lp(WTAG) || u16be(1) || lp(L) || u64be(first_attempt) || K_first
pin     = SHA-256(lp(PIN_TAG) || lp(stored_domain_bytes))
basis   = SHA-256(lp(BASIS_TAG) || u16be(1) || lp(L) || u64be(r_begin) || u32be(2)
                  || 0x01 || E || 0x02 || pin)
"""
import hashlib
import importlib.util
import pathlib
import struct

_here = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("p4ref", _here / "cutover_commitment_reference.py")
p4 = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(p4)

TAG = b"icn.governance.predecessor-lifecycle.v1"
WTAG = b"icn.governance.predecessor-lifecycle-witness.v1"
BASIS_TAG = b"icn.governance.predecessor-lifecycle.basis.v1"
PIN_TAG = b"icn.governance.predecessor-domain-record-pin.v1"
KEY = b"gov:predecessor-lifecycle:v1:"
WKEY = b"gov:predecessor-lifecycle-witness:v1:"


def lp(b: bytes) -> bytes:
    return struct.pack(">I", len(b)) + b


def dom(l: str) -> bytes:
    b = l.encode("utf-8")
    assert 0 < len(b) <= 1024
    return b


def u64(n: int) -> bytes:
    return struct.pack(">Q", n)


def commitment(l: str, s: str) -> bytes:
    c = bytes([0xC1]) * 32
    a, b = bytes([0x01]) * 32, bytes([0x02]) * 32
    r3 = p4.roster(c, [(a, "elector"), (a, "member"), (b, "socia-ñ")])
    return p4.canonical(l, s, c, bytes([0xB1]) * 32, r3, bytes([0x4D]) * 32)


def k_of(c: bytes) -> bytes:
    return hashlib.sha256(c).digest()


def pin(stored: bytes) -> bytes:
    return hashlib.sha256(lp(PIN_TAG) + lp(stored)).digest()


def basis(l: str, r_begin: int, e: bytes, p: bytes) -> bytes:
    assert len(e) == 32 and len(p) == 32
    return hashlib.sha256(lp(BASIS_TAG) + struct.pack(">H", 1) + lp(dom(l)) + u64(r_begin)
                          + struct.pack(">I", 2) + b"\x01" + e + b"\x02" + p).digest()


def head(l: str, revision: int) -> bytes:
    assert revision >= 1
    return lp(TAG) + struct.pack(">H", 1) + lp(dom(l)) + u64(revision)


def legacy(l: str, revision: int, last_attempt: int, aborted_k: bytes) -> bytes:
    assert last_attempt >= 1 and len(aborted_k) == 32
    return head(l, revision) + b"\x01" + u64(last_attempt) + b"\x01" + aborted_k


def attempt_body(attempt: int, c: bytes, b: bytes, e: bytes) -> bytes:
    assert attempt >= 1
    return u64(attempt) + lp(c) + k_of(c) + b + e


def pending(l, revision, attempt, c, b, e):
    return head(l, revision) + b"\x02" + attempt_body(attempt, c, b, e)


def sealed(l, revision, attempt, c, b, e, f):
    return head(l, revision) + b"\x03" + attempt_body(attempt, c, b, e) + f


def witness(l: str, first_attempt: int, k: bytes) -> bytes:
    assert first_attempt >= 1
    return lp(WTAG) + struct.pack(">H", 1) + lp(dom(l)) + u64(first_attempt) + k


if __name__ == "__main__":
    L, S = "legacy.coop.example", "successor.coop.example"
    L2 = "co:op/ñ-legacy"  # colon and multi-byte UTF-8
    E = bytes([0xE1]) * 32
    F = bytes([0xF5]) * 32
    stored_domain = b'{"id":"legacy.coop.example","name":"Legacy"}'
    P = pin(stored_domain)
    B = basis(L, 1, E, P)
    C = commitment(L, S)
    C2 = commitment(L2, S)
    print("PIN", P.hex())
    print("BASIS", B.hex())
    print("KEY", (KEY + lp(dom(L))).hex())
    print("WITNESS_KEY", (WKEY + lp(dom(L2))).hex())
    print("RECORD_LEGACY_ABORTED", legacy(L, 3, 1, k_of(C)).hex())
    print("RECORD_PENDING", pending(L, 1, 1, C, B, E).hex())
    print("RECORD_PENDING_AFTER_ABORT", pending(L, 4, 2, C, B, E).hex())
    print("RECORD_SEALED_MAX_REVISION", sealed(L2, 2**64 - 1, 1, C2, B, E, F).hex())
    print("WITNESS", witness(L, 1, k_of(C)).hex())
