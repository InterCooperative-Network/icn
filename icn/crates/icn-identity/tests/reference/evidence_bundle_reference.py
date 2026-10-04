#!/usr/bin/env python3
"""Independent reference implementation of the N4-A portable evidence bundle, for cross-implementation audit.

WHY THIS FILE EXISTS
--------------------
`tests/evidence_bundle.rs` pins fixed expected bytes for one bundle. A vector the Rust code generates
and then compares against itself proves only that the code is deterministic. It does not prove that
the *specification* is sufficient for someone else to reimplement -- which is the property an
externally transportable container actually needs: a stateless consumer in another language must be
able to produce, and more importantly parse, these exact bytes from the prose alone.

This file is that someone else. It is written from the prose in
`docs/architecture/N4A_PORTABLE_EVIDENCE_BUNDLE.md` (container framing, ordering, scope), the N1
canonical-encoding contract it cites (`icn/crates/icn-identity/src/authority_log/mod.rs` §1-§2 /
HUMAN_IDENTITY_ARCHITECTURE.md §9.2.1), and the N4-A act contract
(`docs/architecture/N4A_DEVICE_AUTHORITY_EVALUATION.md` §6). It shares no code, no library and no
constant definition with `icn-identity`. Every byte -- the N1 bodies, the witnesses, the act, the
container and its id -- is derived here from the stated seed inputs. It then reads the `EXPECT_*`
literals out of the Rust test file and asserts byte equality.

The Rust literals are used ONLY as the comparison target. No value below is derived from them.

RUN IT
------
    python3 icn/crates/icn-identity/tests/reference/evidence_bundle_reference.py

Exits 0 when both implementations agree, 1 on any mismatch, 2 on a setup problem.
Requires `cryptography` (Ed25519 key derivation and signing). Not wired into CI: it is an audit
tool, and adding a Python crypto dependency to the Rust test job would buy nothing the Rust tests
do not already assert.

Ed25519 signatures are deterministic (RFC 8032), so this reference reproduces the witnesses and the
act signature exactly rather than merely verifying them. It also independently applies the bundle's
decode rules to its own bytes (digest agreement, ordering, scope) so that the strictness the
contract prescribes is demonstrated from the prose, not inferred from the Rust decoder.

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


def key_from_seed(seed: bytes) -> Ed25519PrivateKey:
    return Ed25519PrivateKey.from_private_bytes(seed)


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

# --- N4-A act constants, N4A_DEVICE_AUTHORITY_EVALUATION.md §4 --------------------------------
DEVICE_ACT_DOMAIN = b"icn.n4.device-act"
DEVICE_ACT_VERSION = 1

# --- bundle constants, N4A_PORTABLE_EVIDENCE_BUNDLE.md §4 -------------------------------------
BUNDLE_DOMAIN = b"icn.n4.evidence-bundle"
BUNDLE_VERSION = 1


# --- vector inputs, stated in full in the Rust test and in the contract §8 --------------------
SUBJECT_SEED = 0x31
ROOT_SECRET = bytes([SUBJECT_SEED] * 32)
CONTEXT_NONCE = bytes([SUBJECT_SEED ^ 0xA5] * 32)   # the test fixture's nonce rule, stated as data
HORIZON = 4                                         # four Rotate generations armed
DEVICE_A_SEED = bytes([0x41] * 32)
DEVICE_B_SEED = bytes([0x42] * 32)
EVALUATION_POSITION = 3
PAYLOAD = b"fixture:open-workspace"


# --- N1 construction, from the prose ----------------------------------------------------------
def authority_key(generation: int) -> Ed25519PrivateKey:
    # seed = SHA-256(LP(KDF_DOMAIN) || b32(nonce) || u64be(g) || b32(root))
    return key_from_seed(h(lp(N1_KDF_DOMAIN) + CONTEXT_NONCE + u64(generation) + ROOT_SECRET))


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


# --- N4-A act, from §6 ------------------------------------------------------------------------
def device_act(subject: bytes, device: bytes, position: int, capability: int, payload: bytes) -> bytes:
    return lp(DEVICE_ACT_DOMAIN) + u16(DEVICE_ACT_VERSION) + subject + P(device) \
        + u64(position) + u8(capability) + lp(payload)


# --- the bundle, from the contract §4 ---------------------------------------------------------
def fact(body: bytes, witnesses: list) -> bytes:
    # fact := b32(event_id) || LP(canonical_body) || u32be(witness_count) || b64(signature)*
    # witnesses strictly ascending bytewise
    sigs = sorted(set(witnesses))
    assert len(sigs) == len(witnesses) >= 1
    out = h(body) + lp(body) + u32(len(sigs))
    for s in sigs:
        out += s
    return out


def bundle(subject: bytes, position: int, facts: dict, act: bytes, act_sig: bytes) -> bytes:
    # facts: {event_id: fact_bytes}, emitted strictly ascending by event_id
    out = lp(BUNDLE_DOMAIN) + u16(BUNDLE_VERSION) + subject + u64(position) + u32(len(facts))
    for event_id in sorted(facts):
        out += facts[event_id]
    return out + lp(act) + act_sig


# --- an independent strict decoder, so the contract's decode rules are exercised from the prose ---
class Refused(Exception):
    pass


def strict_decode(buf: bytes) -> dict:
    pos = 0

    def take(n: int) -> bytes:
        nonlocal pos
        if pos + n > len(buf):
            raise Refused("truncated")
        out = buf[pos:pos + n]
        pos += n
        return out

    def take_lp() -> bytes:
        n = int.from_bytes(take(4), "big")
        return take(n)

    def count(min_len: int) -> int:
        n = int.from_bytes(take(4), "big")
        if n * min_len > len(buf) - pos:
            raise Refused("count overflow")
        return n

    if take_lp() != BUNDLE_DOMAIN:
        raise Refused("bad domain")
    if int.from_bytes(take(2), "big") != BUNDLE_VERSION:
        raise Refused("bad version")
    subject = take(32)
    through = int.from_bytes(take(8), "big")
    if through == 0 or through > 1_048_576:
        raise Refused("position out of range")
    facts = {}
    prev_id = None
    for _ in range(count(32 + 4 + 4)):
        stated = take(32)
        if prev_id is not None and stated <= prev_id:
            raise Refused("fact order")
        prev_id = stated
        body = take_lp()
        if h(body) != stated:
            raise Refused("event_id mismatch")
        # Scope: the body must belong to `subject` and sit at position <= through. Inception is
        # position 0 and its subject is its own digest; other kinds carry subject||position in HDR.
        kind = body[4 + len(N1_DOMAIN) + 2]
        if kind == KIND_INCEPTION:
            body_subject, body_pos = h(body), 0
        else:
            hdr = 4 + len(N1_DOMAIN) + 2 + 1
            body_subject = body[hdr:hdr + 32]
            body_pos = int.from_bytes(body[hdr + 32:hdr + 40], "big")
        if body_subject != subject:
            raise Refused("fact subject mismatch")
        if body_pos > through:
            raise Refused("fact outside scope")
        n = count(64)
        if n == 0:
            raise Refused("no witness")
        prev_w = None
        sigs = []
        for _ in range(n):
            w = take(64)
            if prev_w is not None and w <= prev_w:
                raise Refused("witness order")
            prev_w = w
            sigs.append(w)
        facts[stated] = (body, sigs)
    act = take_lp()
    act_sig = take(64)
    if pos != len(buf):
        raise Refused("trailing bytes")
    act_subject = act[4 + len(DEVICE_ACT_DOMAIN) + 2:][:32]
    act_pos = int.from_bytes(act[4 + len(DEVICE_ACT_DOMAIN) + 2 + 32 + 33:][:8], "big")
    if act_subject != subject:
        raise Refused("act subject mismatch")
    if act_pos != through:
        raise Refused("act position mismatch")
    return {"subject": subject, "through": through, "facts": facts, "act": act, "act_sig": act_sig}


def rust_literals(path: Path) -> dict:
    text = path.read_text()
    found = {}
    for name, value in re.findall(r'const (EXPECT_[A-Z0-9_]+): &str\s*=\s*"([0-9a-f]+)";', text):
        found[name] = value
    m = re.search(r'const EXPECT_FACT_EVENT_IDS_HEX: \[&str; 4\] = \[(.*?)\];', text, re.S)
    if m:
        found["EXPECT_FACT_EVENT_IDS_HEX"] = re.findall(r'"([0-9a-f]+)"', m.group(1))
    return found


def main() -> int:
    here = Path(__file__).resolve()
    rust_test = here.parent.parent / "evidence_bundle.rs"
    if not rust_test.is_file():
        print(f"setup: cannot find {rust_test}", file=sys.stderr)
        return 2
    expect = rust_literals(rust_test)

    # Build the four N1 facts from the seeds.
    k0 = authority_key(0)
    a_key, b_key = key_from_seed(DEVICE_A_SEED), key_from_seed(DEVICE_B_SEED)
    a_pub, b_pub, k0_pub = pub(a_key), pub(b_key), pub(k0)

    inception = inception_body()
    subject = h(inception)                                  # SubjectId = event_id(inception)
    auth_a = authorize_body(subject, 1, h(inception), k0_pub, a_pub)
    auth_b = authorize_body(subject, 2, h(auth_a), k0_pub, b_pub)
    revoke_a = revoke_body(subject, 3, h(auth_b), k0_pub, a_pub)

    facts = {}
    for body in (inception, auth_a, auth_b, revoke_a):
        facts[h(body)] = fact(body, [witness(body, k0)])

    act = device_act(subject, b_pub, EVALUATION_POSITION, CAP_SIGN, PAYLOAD)
    act_sig = b_key.sign(act)
    bundle_bytes = bundle(subject, EVALUATION_POSITION, facts, act, act_sig)

    derived = {
        "EXPECT_SUBJECT_ID_HEX": subject.hex(),
        "EXPECT_ACT_BYTES_HEX": act.hex(),
        "EXPECT_BUNDLE_BYTES_HEX": bundle_bytes.hex(),
        "EXPECT_BUNDLE_ID_HEX": h(bundle_bytes).hex(),
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

    ids = [i.hex() for i in sorted(facts)]
    pinned_ids = expect.get("EXPECT_FACT_EVENT_IDS_HEX")
    if pinned_ids != ids:
        print(f"MISMATCH EXPECT_FACT_EVENT_IDS_HEX\n  rust      = {pinned_ids}\n  reference = {ids}")
        ok = False
    else:
        print("ok       EXPECT_FACT_EVENT_IDS_HEX (ascending by digest, not by position)")

    # The independently written strict decoder accepts the bytes and recovers the same facts, and
    # refuses the contract's named defect classes.
    try:
        decoded = strict_decode(bundle_bytes)
        assert decoded["subject"] == subject and decoded["through"] == EVALUATION_POSITION
        assert set(decoded["facts"]) == set(facts) and decoded["act"] == act
        print("ok       independent strict decode recovers the same facts and act")
    except (Refused, AssertionError) as exc:
        print(f"MISMATCH independent strict decode refused its own bytes: {exc}")
        ok = False

    def must_refuse(label: str, mutated: bytes) -> None:
        nonlocal ok
        try:
            strict_decode(mutated)
        except Refused as exc:
            print(f"ok       refuses {label} ({exc})")
            return
        print(f"MISMATCH {label} was accepted")
        ok = False

    must_refuse("trailing byte", bundle_bytes + b"\x00")
    must_refuse("truncation", bundle_bytes[:-1])
    swapped = bundle(subject, EVALUATION_POSITION, facts, act, act_sig)
    first, second = sorted(facts)[:2]
    body_a = facts[first]
    swapped = swapped.replace(facts[first] + facts[second], facts[second] + facts[first], 1)
    must_refuse("reordered facts", swapped)
    corrupt = bytearray(bundle_bytes)
    corrupt[4 + len(BUNDLE_DOMAIN) + 2 + 32 + 8 + 4 + 31] ^= 0x01        # first fact's stated digest
    must_refuse("stated event_id that is not the body digest", bytes(corrupt))
    out_of_scope = bundle(subject, 2, facts, device_act(subject, b_pub, 2, CAP_SIGN, PAYLOAD), act_sig)
    must_refuse("fact above the evaluation position", out_of_scope)
    wrong_pos = bundle(subject, 4, {}, act, act_sig)
    must_refuse("act position disagreeing with the bundle", wrong_pos)
    del body_a

    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
