#!/usr/bin/env python3
"""Independent reference for G1-A `SemanticProposalCommitmentV1` (treasury-spend, v1).

Written from the layout documented in `icn-governance/src/proposal_commitment.rs` (module docs,
"Canonical encoding") alone. It imports nothing from ICN and reads no Rust output to *derive* a
value. The only shared code is the pure-Python BLAKE3 in the sibling GEN-B reference, which is
self-checked against published BLAKE3 vectors there.

Layout (all integers u64 little-endian; STR(x) = u64le(len(utf8(x))) || utf8(x)):

    TAG = b"icn:gov:proposal-commitment:treasury-spend:v1"
    preimage = TAG || STR(domain_id) || STR(proposal_id) || STR(treasury_did) || STR(recipient)
                   || STR(currency) || STR(memo) || u64le(amount) || u64le(nonce)
    commitment_hash = BLAKE3(preimage)

Run (compare against the Rust test's EXPECT_* literals; non-zero exit on any disagreement):
    python3 icn/crates/icn-governance/tests/reference/proposal_commitment_reference.py
Print the vectors instead:
    python3 .../proposal_commitment_reference.py --print
"""
import importlib.util
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location(
    "gen_b_ref", os.path.join(HERE, "subject_recognition_reference.py"))
_gen_b = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_gen_b)  # __main__-guarded: defines functions only
blake3 = _gen_b.blake3

TAG = b"icn:gov:proposal-commitment:treasury-spend:v1"

# Fixture DIDs: did:icn:z + base58btc(32 x byte).
DID_A = "did:icn:z29d2S7vB453rNYFdR5Ycwt7y9haRT5fwVwL9zTmBhfV2"  # 0x11 * 32 (treasury)
DID_B = "did:icn:z3JF3sEqM796hk5WFqA6EtmEwJQ9quALszsfJyvXNQKy3"  # 0x22 * 32 (recipient)
DID_C = "did:icn:z4Ss5JMkXAD9Z7cktFEdrqeMuT6jGMF1pVozTyPHZ6zT4"  # 0x33 * 32 (other recipient)


def u64le(n):
    assert 0 <= n < 2 ** 64
    return struct.pack("<Q", n)


def s(x):
    b = x.encode("utf-8")
    return u64le(len(b)) + b


def preimage(domain, proposal, treasury, recipient, currency, memo, amount, nonce):
    assert amount > 0
    return (TAG + s(domain) + s(proposal) + s(treasury) + s(recipient)
            + s(currency) + s(memo) + u64le(amount) + u64le(nonce))


BASE = dict(domain="test-coop", proposal="spend-prop-001", treasury=DID_A, recipient=DID_B,
            currency="HOURS", memo="sprint-26 provenance proof", amount=100, nonce=0)


def variant(**kw):
    d = dict(BASE)
    d.update(kw)
    return d


VECTORS = {
    "EXPECT_BASE_BYTES_HEX": lambda: preimage(**BASE).hex(),
    "EXPECT_BASE_HASH": lambda: blake3(preimage(**BASE)).hex(),
    "EXPECT_OTHER_RECIPIENT_HASH": lambda: blake3(preimage(**variant(recipient=DID_C))).hex(),
    "EXPECT_AMOUNT_101_HASH": lambda: blake3(preimage(**variant(amount=101))).hex(),
    "EXPECT_NONCE_1_HASH": lambda: blake3(preimage(**variant(nonce=1))).hex(),
    # Boundary-shift pair: moving a byte between adjacent string fields must change the hash.
    "EXPECT_SHIFT_A_HASH": lambda: blake3(preimage(**variant(domain="ab", proposal="c"))).hex(),
    "EXPECT_SHIFT_B_HASH": lambda: blake3(preimage(**variant(domain="a", proposal="bc"))).hex(),
    # Non-ASCII UTF-8 memo is committed as its exact bytes.
    "EXPECT_UTF8_MEMO_HASH": lambda: blake3(preimage(**variant(memo="café ✓"))).hex(),
}


def main():
    got = {k: f() for k, f in VECTORS.items()}
    if "--print" in sys.argv:
        for k, v in got.items():
            print(f'const {k}: &str = "{v}";')
        return 0
    src = open(os.path.join(HERE, "..", "proposal_commitment.rs"), encoding="utf-8").read()
    bad = 0
    for k, v in got.items():
        m = re.search(r'const %s: &str =\s*"([0-9a-f]+)";' % k, src)
        if not m:
            print(f"MISSING {k} in tests/proposal_commitment.rs")
            bad += 1
        elif m.group(1) != v:
            print(f"MISMATCH {k}: rust-test={m.group(1)} reference={v}")
            bad += 1
        else:
            print(f"ok {k}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
