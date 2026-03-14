#!/usr/bin/env python3
"""
ICN Governance Demo Script
===========================
Demonstrates the full cooperative governance flow:
  1. Authenticate three identities (admin, Alice, Bob)
  2. Create a cooperative and governance domain
  3. Add members to both
  4. Alice proposes a budget item
  5. Alice and Bob vote
  6. Admin closes the proposal
  7. Display results, tally, and cryptographic proof

Usage:
    python3 demo-governance.py [GATEWAY_URL]

Default GATEWAY_URL: http://localhost:8080
"""

import json
import os
import sys
import time
import hashlib
import urllib.request
import urllib.error

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import (
    Encoding,
    PublicFormat,
    PrivateFormat,
    NoEncryption,
)

# ── Configuration ────────────────────────────────────────────────────────

GATEWAY = os.environ.get("ICN_GATEWAY", sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8080")
COOP_ID = "brightworks"
COOP_NAME = "Brightworks Cooperative"
DOMAIN_ID = f"coop:{COOP_ID}"
DOMAIN_NAME = f"{COOP_NAME} Governance"

# ── Base58 Bitcoin encoding (multibase compatible) ───────────────────────

ALPHABET = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

def base58_encode(data: bytes) -> str:
    """Encode bytes to Base58 (Bitcoin alphabet)."""
    n = int.from_bytes(data, "big")
    result = []
    while n > 0:
        n, remainder = divmod(n, 58)
        result.append(ALPHABET[remainder:remainder + 1])
    # Handle leading zero bytes
    for byte in data:
        if byte == 0:
            result.append(ALPHABET[0:1])
        else:
            break
    return b"".join(reversed(result)).decode("ascii")


def make_did(public_key_bytes: bytes) -> str:
    """Create a did:icn: DID from a 32-byte Ed25519 public key.

    Uses multibase Base58Btc encoding (prefix 'z').
    """
    encoded = "z" + base58_encode(public_key_bytes)
    return f"did:icn:{encoded}"


# ── Identity generation ──────────────────────────────────────────────────

class Identity:
    """An Ed25519 identity with DID."""

    def __init__(self, name: str):
        self.name = name
        self.private_key = Ed25519PrivateKey.generate()
        self.public_key = self.private_key.public_key()
        pub_bytes = self.public_key.public_bytes(Encoding.Raw, PublicFormat.Raw)
        self.did = make_did(pub_bytes)
        self.token = None

    def sign(self, data: bytes) -> bytes:
        return self.private_key.sign(data)


# ── HTTP helpers ─────────────────────────────────────────────────────────

def api(method: str, path: str, body=None, token=None):
    """Make an API call and return (status_code, response_json)."""
    url = f"{GATEWAY}/v1{path}"
    data = json.dumps(body).encode() if body else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(req) as resp:
            body_text = resp.read().decode()
            return resp.status, json.loads(body_text) if body_text else {}
    except urllib.error.HTTPError as e:
        body_text = e.read().decode()
        try:
            error_json = json.loads(body_text)
        except json.JSONDecodeError:
            error_json = {"error": body_text}
        return e.code, error_json


def step(label: str):
    """Print a step header."""
    print(f"\n{'─' * 60}")
    print(f"  {label}")
    print(f"{'─' * 60}")


def ok(msg: str, data=None):
    """Print success."""
    print(f"  ✓ {msg}")
    if data:
        print(f"    {json.dumps(data, indent=2)[:500]}")


def fail(msg: str, status=None, data=None):
    """Print failure and exit."""
    print(f"  ✗ {msg}")
    if status:
        print(f"    HTTP {status}")
    if data:
        print(f"    {json.dumps(data, indent=2)[:500]}")
    sys.exit(1)


# ── Authentication ───────────────────────────────────────────────────────

def authenticate(identity: Identity, coop_id: str, scopes: list[str]):
    """Perform challenge-response auth and get a JWT token."""
    # Step 1: Request challenge
    status, resp = api("POST", "/auth/challenge", {"did": identity.did})
    if status != 200:
        fail(f"Challenge failed for {identity.name}", status, resp)

    nonce_hex = resp["nonce"]
    nonce_bytes = bytes.fromhex(nonce_hex)

    # Step 2: Sign the nonce
    signature = identity.sign(nonce_bytes)
    signature_hex = signature.hex()

    # Step 3: Verify and get token
    status, resp = api("POST", "/auth/verify", {
        "did": identity.did,
        "signature": signature_hex,
        "coop_id": coop_id,
        "scopes": scopes,
    })
    if status != 200:
        fail(f"Verify failed for {identity.name}", status, resp)

    identity.token = resp["token"]
    ok(f"Authenticated {identity.name} ({identity.did[:30]}...)")
    return identity.token


# ── Main demo flow ───────────────────────────────────────────────────────

def main():
    print("=" * 60)
    print("  ICN Cooperative Governance Demo")
    print("=" * 60)
    print(f"  Gateway: {GATEWAY}")
    print()

    # Check health
    status, health = api("GET", "/health")
    if status != 200:
        fail("Gateway not reachable", status, health)
    ok(f"Gateway healthy (version {health.get('version', '?')})")

    # ── Generate identities ──────────────────────────────────────────
    step("1. Generate identities")
    admin = Identity("Admin")
    alice = Identity("Alice")
    bob = Identity("Bob")
    ok(f"Admin: {admin.did}")
    ok(f"Alice: {alice.did}")
    ok(f"Bob:   {bob.did}")

    # ── Authenticate admin ───────────────────────────────────────────
    all_scopes = [
        "coop:read", "coop:write", "coop:admin",
        "governance:read", "governance:write",
        "treasury:read", "treasury:write",
    ]

    step("2. Authenticate Admin")
    authenticate(admin, COOP_ID, all_scopes)

    # ── Create cooperative ───────────────────────────────────────────
    step("3. Create cooperative")
    status, resp = api("POST", "/coops", {
        "id": COOP_ID,
        "name": COOP_NAME,
    }, admin.token)
    if status == 200 or status == 201:
        ok(f"Created cooperative: {COOP_NAME}")
    else:
        fail("Failed to create cooperative", status, resp)

    # ── Create governance domain ─────────────────────────────────────
    step("4. Create governance domain")
    status, resp = api("POST", "/gov/domains", {
        "id": DOMAIN_ID,
        "name": DOMAIN_NAME,
        "profile": "cooperative_default",
        "quorum_percent": 50,
        "approval_percent": 51,
        "voting_period_days": 7,
        "members": [admin.did],
    }, admin.token)
    if status == 200 or status == 201:
        ok(f"Created domain: {DOMAIN_ID}")
    else:
        fail("Failed to create governance domain", status, resp)

    # ── Add Alice to cooperative ─────────────────────────────────────
    step("5. Add Alice to cooperative")
    status, resp = api("POST", f"/coops/{COOP_ID}/members", {
        "did": alice.did,
        "role": "participant",
        "display_name": "Alice",
    }, admin.token)
    if status == 200 or status == 201:
        ok("Alice added to cooperative")
    else:
        fail("Failed to add Alice to cooperative", status, resp)

    # ── Add Alice to governance domain ───────────────────────────────
    step("6. Add Alice to governance domain")
    status, resp = api("POST", f"/gov/domains/{DOMAIN_ID}/members", {
        "did": alice.did,
        "weight": 1.0,
    }, admin.token)
    if status == 200 or status == 201:
        ok("Alice added to governance domain")
    else:
        fail("Failed to add Alice to governance domain", status, resp)

    # ── Add Bob to cooperative ───────────────────────────────────────
    step("7. Add Bob to cooperative")
    status, resp = api("POST", f"/coops/{COOP_ID}/members", {
        "did": bob.did,
        "role": "participant",
        "display_name": "Bob",
    }, admin.token)
    if status == 200 or status == 201:
        ok("Bob added to cooperative")
    else:
        fail("Failed to add Bob to cooperative", status, resp)

    # ── Add Bob to governance domain ─────────────────────────────────
    step("8. Add Bob to governance domain")
    status, resp = api("POST", f"/gov/domains/{DOMAIN_ID}/members", {
        "did": bob.did,
        "weight": 1.0,
    }, admin.token)
    if status == 200 or status == 201:
        ok("Bob added to governance domain")
    else:
        fail("Failed to add Bob to governance domain", status, resp)

    # ── Authenticate Alice ───────────────────────────────────────────
    step("9. Authenticate Alice")
    authenticate(alice, COOP_ID, ["governance:read", "governance:write"])

    # ── Alice creates a proposal ─────────────────────────────────────
    step("10. Alice creates a proposal")
    status, resp = api("POST", "/gov/proposals", {
        "domain_id": DOMAIN_ID,
        "title": "Approve Q2 Budget of $12,000",
        "description": (
            "Proposal to allocate $12,000 for Q2 2026 operations. "
            "This covers equipment maintenance ($4,000), member training ($3,000), "
            "community outreach ($2,500), and administrative costs ($2,500)."
        ),
        "payload": {
            "type": "budget",
            "amount": 12000,
            "recipient": admin.did,
            "currency": "USD",
            "purpose": "Q2 2026 Operating Budget",
        },
    }, alice.token)
    if status == 200 or status == 201:
        proposal_id = resp.get("id", resp.get("proposal_id", "unknown"))
        ok(f"Proposal created: {proposal_id}")
        ok(f"Title: {resp.get('title', 'N/A')}")
    else:
        fail("Failed to create proposal", status, resp)

    # ── Alice opens the proposal for voting ──────────────────────────
    step("11. Open proposal for voting")
    status, resp = api("POST", f"/gov/proposals/{proposal_id}/open", {
        "voting_period_seconds": 3600,
    }, alice.token)
    if status == 200 or status == 201:
        ok("Proposal opened for voting")
    else:
        fail("Failed to open proposal", status, resp)

    # ── Alice votes "for" ────────────────────────────────────────────
    step("12. Alice votes FOR the proposal")
    status, resp = api("POST", f"/gov/proposals/{proposal_id}/vote", {
        "choice": "for",
        "comment": "Essential for our Q2 operations. Fully support.",
    }, alice.token)
    if status == 200 or status == 201:
        ok("Alice voted: FOR")
    else:
        fail("Alice failed to vote", status, resp)

    # ── Authenticate Bob ─────────────────────────────────────────────
    step("13. Authenticate Bob")
    authenticate(bob, COOP_ID, ["governance:read", "governance:write"])

    # ── Bob votes "for" ──────────────────────────────────────────────
    step("14. Bob votes FOR the proposal")
    status, resp = api("POST", f"/gov/proposals/{proposal_id}/vote", {
        "choice": "for",
        "comment": "Budget looks reasonable. Approved.",
    }, bob.token)
    if status == 200 or status == 201:
        ok("Bob voted: FOR")
    else:
        fail("Bob failed to vote", status, resp)

    # ── Admin closes the proposal ────────────────────────────────────
    step("15. Close proposal and tally votes")
    # Re-authenticate admin with governance scopes
    authenticate(admin, COOP_ID, all_scopes)

    status, resp = api("POST", f"/gov/proposals/{proposal_id}/close", {},
                       admin.token)
    if status == 200 or status == 201:
        ok("Proposal closed")
    else:
        fail("Failed to close proposal", status, resp)

    # ── Get final proposal state ─────────────────────────────────────
    step("16. Final proposal state")
    status, resp = api("GET", f"/gov/proposals/{proposal_id}", token=admin.token)
    if status == 200:
        ok(f"Proposal state: {resp.get('state', 'unknown')}")
        print(f"    {json.dumps(resp, indent=2)}")
    else:
        fail("Failed to get proposal", status, resp)

    # ── Get vote tally ───────────────────────────────────────────────
    step("17. Vote tally")
    status, resp = api("GET", f"/gov/proposals/{proposal_id}/tally",
                       token=admin.token)
    if status == 200:
        ok("Vote tally retrieved")
        print(f"    {json.dumps(resp, indent=2)}")
    else:
        print(f"  ⚠ Tally not available (HTTP {status})")
        print(f"    {json.dumps(resp, indent=2)[:300]}")

    # ── Get cryptographic proof ──────────────────────────────────────
    step("18. Cryptographic proof")
    status, resp = api("GET", f"/gov/proposals/{proposal_id}/proof",
                       token=admin.token)
    if status == 200:
        ok("Governance proof retrieved")
        print(f"    {json.dumps(resp, indent=2)[:800]}")
    else:
        print(f"  ⚠ Proof not available (HTTP {status})")
        print(f"    {json.dumps(resp, indent=2)[:300]}")

    # ── Summary ──────────────────────────────────────────────────────
    print()
    print("=" * 60)
    print("  Demo Complete")
    print("=" * 60)
    print(f"""
  Cooperative:  {COOP_NAME}
  Members:      Admin, Alice, Bob
  Proposal:     "Approve Q2 Budget of $12,000"
  Result:       2 votes FOR, 0 against (100% approval)
  Status:       Closed with cryptographic audit trail

  This demonstrates:
  • DID-based challenge-response authentication
  • Cooperative creation and member management
  • Democratic governance with proposals and voting
  • Cryptographic proofs for every decision
  • Full audit trail — no votes can be altered or deleted
""")


if __name__ == "__main__":
    main()
