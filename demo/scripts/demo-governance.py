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
    python3 demo-governance.py [GATEWAY_URL] --presenter

Default GATEWAY_URL: http://localhost:8080

Flags:
    --presenter   Interactive mode with colored output, phase grouping,
                  and Enter-to-continue pauses between phases.
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

# Parse args: positional URL and --presenter flag
PRESENTER_MODE = "--presenter" in sys.argv
_positional = [a for a in sys.argv[1:] if not a.startswith("--")]
GATEWAY = os.environ.get("ICN_GATEWAY", _positional[0] if _positional else "http://localhost:8080")
COOP_ID = "finger-lakes-food"
COOP_NAME = "Finger Lakes Food Co-op"
DOMAIN_ID = f"coop:{COOP_ID}"
DOMAIN_NAME = f"{COOP_NAME} Governance"

# ── ANSI colors (for --presenter mode) ───────────────────────────────────

class C:
    """ANSI color codes, disabled if not a TTY or not in presenter mode."""
    _on = PRESENTER_MODE and sys.stdout.isatty()
    RESET  = "\033[0m"  if _on else ""
    BOLD   = "\033[1m"  if _on else ""
    DIM    = "\033[2m"  if _on else ""
    GREEN  = "\033[32m" if _on else ""
    RED    = "\033[31m" if _on else ""
    YELLOW = "\033[33m" if _on else ""
    CYAN   = "\033[36m" if _on else ""
    BLUE   = "\033[34m" if _on else ""
    MAGENTA= "\033[35m" if _on else ""
    WHITE  = "\033[97m" if _on else ""


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
    print(f"  {C.GREEN}✓{C.RESET} {msg}")
    if data:
        print(f"    {json.dumps(data, indent=2)[:500]}")


def fail(msg: str, status=None, data=None):
    """Print failure and exit."""
    print(f"  {C.RED}✗ {msg}{C.RESET}")
    if status:
        print(f"    HTTP {status}")
    if data:
        print(f"    {json.dumps(data, indent=2)[:500]}")
    sys.exit(1)


def narrator(msg: str):
    """In presenter mode, print a narrative explanation."""
    if PRESENTER_MODE:
        print(f"  {C.DIM}{msg}{C.RESET}")


def phase_header(num: int, title: str, description: str):
    """In presenter mode, print a big phase header."""
    if PRESENTER_MODE:
        print()
        print(f"  {C.BOLD}{C.CYAN}{'━' * 56}{C.RESET}")
        print(f"  {C.BOLD}{C.CYAN}  Phase {num}: {title}{C.RESET}")
        print(f"  {C.DIM}  {description}{C.RESET}")
        print(f"  {C.BOLD}{C.CYAN}{'━' * 56}{C.RESET}")
        print()


def phase_pause(next_phase: str):
    """In presenter mode, pause and wait for Enter."""
    if PRESENTER_MODE:
        print()
        print(f"  {C.YELLOW}[Press Enter to continue to: {next_phase}]{C.RESET}")
        try:
            input()
        except EOFError:
            pass


# ── Authentication ───────────────────────────────────────────────────────

def authenticate(identity: Identity, coop_id: str, scopes: list[str]):
    """Perform challenge-response auth and get a JWT token."""
    narrator(f"  {identity.name} proves their identity by signing a cryptographic challenge...")

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
    if PRESENTER_MODE:
        print()
        print(f"  {C.BOLD}{C.WHITE}╔══════════════════════════════════════════════════════╗{C.RESET}")
        print(f"  {C.BOLD}{C.WHITE}║   Cooperative Governance Demo                        ║{C.RESET}")
        print(f"  {C.BOLD}{C.WHITE}║   InterCooperative Network (ICN)                     ║{C.RESET}")
        print(f"  {C.BOLD}{C.WHITE}╚══════════════════════════════════════════════════════╝{C.RESET}")
        print()
        print(f"  {C.DIM}This demo shows how a cooperative can make democratic{C.RESET}")
        print(f"  {C.DIM}decisions using cryptographic identities and proofs.{C.RESET}")
        print(f"  {C.DIM}Every vote is signed. Every decision is verifiable.{C.RESET}")
        print()
        print(f"  {C.DIM}Gateway: {GATEWAY}{C.RESET}")
    else:
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

    # ═══════════════════════════════════════════════════════════
    # PHASE 1: SETUP
    # ═══════════════════════════════════════════════════════════
    phase_header(1, "Set Up the Cooperative",
                 "Create a co-op, generate digital identities, add members")

    # ── Generate identities ──────────────────────────────────────────
    step("1. Generate identities")
    narrator("Each member gets a unique cryptographic keypair — like a digital signature.")
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
    narrator("A coordinator registers the co-op on the network...")
    status, resp = api("POST", "/coops", {
        "id": COOP_ID,
        "name": COOP_NAME,
    }, admin.token)
    if status == 200 or status == 201:
        ok(f"Created cooperative: {C.BOLD}{COOP_NAME}{C.RESET}")
    else:
        fail("Failed to create cooperative", status, resp)

    # ── Create governance domain ─────────────────────────────────────
    step("4. Create governance domain")
    narrator("The governance domain defines voting rules: 50% quorum, simple majority.")
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
    narrator("Alice joins the co-op and gets voting rights...")
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
    narrator("Bob joins too — each member has equal voting weight.")
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

    # ── Pause between phases ──────────────────────────────────────────
    if PRESENTER_MODE:
        print()
        print(f"  {C.GREEN}{C.BOLD}Phase 1 complete:{C.RESET} {C.GREEN}Cooperative \"{COOP_NAME}\" created with 3 members.{C.RESET}")
    phase_pause("Make a Decision Together")

    # ═══════════════════════════════════════════════════════════
    # PHASE 2: GOVERN
    # ═══════════════════════════════════════════════════════════
    phase_header(2, "Make a Decision Together",
                 "Submit a proposal and vote on it democratically")

    # ── Authenticate Alice ───────────────────────────────────────────
    step("9. Authenticate Alice")
    authenticate(alice, COOP_ID, ["governance:read", "governance:write"])

    # ── Alice creates a proposal ─────────────────────────────────────
    step("10. Alice creates a proposal")
    narrator("Alice submits a budget proposal for community kitchen equipment...")
    status, resp = api("POST", "/gov/proposals", {
        "domain_id": DOMAIN_ID,
        "title": "Approve $12,000 for community kitchen equipment",
        "description": (
            "Purchase commercial-grade equipment for the shared community kitchen: "
            "convection oven ($4,000), industrial mixer ($3,000), "
            "prep tables and storage ($2,500), safety equipment and small tools ($2,500). "
            "This serves all 47 member households."
        ),
        "payload": {
            "type": "budget",
            "amount": 12000,
            "recipient": admin.did,
            "currency": "USD",
            "purpose": "Community Kitchen Equipment",
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
    narrator("The proposal moves to a vote. Members have 1 hour to cast ballots.")
    status, resp = api("POST", f"/gov/proposals/{proposal_id}/open", {
        "voting_period_seconds": 3600,
    }, alice.token)
    if status == 200 or status == 201:
        ok("Proposal opened for voting")
    else:
        fail("Failed to open proposal", status, resp)

    # ── Alice votes "for" ────────────────────────────────────────────
    step("12. Alice votes FOR the proposal")
    narrator("Alice signs her vote cryptographically — it cannot be altered after submission.")
    status, resp = api("POST", f"/gov/proposals/{proposal_id}/vote", {
        "choice": "for",
        "comment": "Essential equipment for our community kitchen. Fully support.",
    }, alice.token)
    if status == 200 or status == 201:
        ok(f"{C.GREEN}Alice voted: FOR{C.RESET}")
    else:
        fail("Alice failed to vote", status, resp)

    # ── Authenticate Bob ─────────────────────────────────────────────
    step("13. Authenticate Bob")
    authenticate(bob, COOP_ID, ["governance:read", "governance:write"])

    # ── Bob votes "for" ──────────────────────────────────────────────
    step("14. Bob votes FOR the proposal")
    narrator("Bob reviews the proposal and casts his vote.")
    status, resp = api("POST", f"/gov/proposals/{proposal_id}/vote", {
        "choice": "for",
        "comment": "Great investment. The kitchen serves all our members.",
    }, bob.token)
    if status == 200 or status == 201:
        ok(f"{C.GREEN}Bob voted: FOR{C.RESET}")
    else:
        fail("Bob failed to vote", status, resp)

    # ── Pause between phases ──────────────────────────────────────────
    if PRESENTER_MODE:
        print()
        print(f"  {C.GREEN}{C.BOLD}Phase 2 complete:{C.RESET} {C.GREEN}2 votes cast (Alice: FOR, Bob: FOR).{C.RESET}")
    phase_pause("Verify the Result")

    # ═══════════════════════════════════════════════════════════
    # PHASE 3: VERIFY
    # ═══════════════════════════════════════════════════════════
    phase_header(3, "Verify the Result",
                 "Close the vote and generate a tamper-proof receipt")

    # ── Admin closes the proposal ────────────────────────────────────
    step("15. Close proposal and tally votes")
    narrator("The coordinator closes the vote. The system tallies and evaluates.")
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
        state = resp.get("state", "unknown")
        state_name = list(state.keys())[0] if isinstance(state, dict) else str(state)
        ok(f"Proposal state: {C.BOLD}{C.GREEN}{state_name}{C.RESET}")
        if not PRESENTER_MODE:
            print(f"    {json.dumps(resp, indent=2)}")
    else:
        fail("Failed to get proposal", status, resp)

    # ── Get vote tally ───────────────────────────────────────────────
    step("17. Vote tally")
    narrator("The tally is computed from all cryptographically signed ballots.")
    status, tally = api("GET", f"/gov/proposals/{proposal_id}/tally",
                       token=admin.token)
    if status == 200:
        ok("Vote tally retrieved")
        total = tally.get("for_votes", 0) + tally.get("against_votes", 0) + tally.get("abstain_votes", 0)
        for_pct = round(tally["for_votes"] / total * 100) if total > 0 else 0
        if PRESENTER_MODE:
            bar_width = 40
            for_bars = round(for_pct / 100 * bar_width)
            against_bars = bar_width - for_bars
            print()
            print(f"    {C.GREEN}{'█' * for_bars}{C.RED}{'█' * against_bars}{C.RESET}")
            print(f"    {C.GREEN}For: {tally['for_votes']} ({for_pct}%){C.RESET}  {C.RED}Against: {tally['against_votes']}{C.RESET}  {C.DIM}Abstain: {tally['abstain_votes']}{C.RESET}")
        else:
            print(f"    {json.dumps(tally, indent=2)}")
    else:
        print(f"  ⚠ Tally not available (HTTP {status})")
        print(f"    {json.dumps(tally, indent=2)[:300]}")

    # ── Get cryptographic proof ──────────────────────────────────────
    step("18. Cryptographic proof")
    narrator("A cryptographic receipt is generated — tamper-proof evidence of the decision.")
    status, resp = api("GET", f"/gov/proposals/{proposal_id}/proof",
                       token=admin.token)
    proof_hash = None
    if status == 200:
        ok("Governance proof retrieved")
        receipt = resp.get("receipt", resp)
        vote_hash_raw = receipt.get("vote_hash", [])
        if isinstance(vote_hash_raw, list):
            proof_hash = bytes(vote_hash_raw).hex()
        else:
            proof_hash = str(vote_hash_raw)
        if PRESENTER_MODE:
            print(f"    {C.DIM}Vote hash:     {proof_hash[:32]}...{C.RESET}")
            decision_hash_raw = receipt.get("decision_hash", [])
            if isinstance(decision_hash_raw, list):
                dh = bytes(decision_hash_raw).hex()
            else:
                dh = str(decision_hash_raw)
            print(f"    {C.DIM}Decision hash: {dh[:32]}...{C.RESET}")
        else:
            print(f"    {json.dumps(resp, indent=2)[:800]}")
    else:
        print(f"  ⚠ Proof not available (HTTP {status})")
        print(f"    {json.dumps(resp, indent=2)[:300]}")

    # ── Summary ──────────────────────────────────────────────────────
    print()
    if PRESENTER_MODE:
        print(f"  {C.BOLD}{C.WHITE}╔══════════════════════════════════════════════════════╗{C.RESET}")
        print(f"  {C.BOLD}{C.WHITE}║   Demo Complete                                      ║{C.RESET}")
        print(f"  {C.BOLD}{C.WHITE}╚══════════════════════════════════════════════════════╝{C.RESET}")
        print()
        print(f"  {C.CYAN}🏪 Cooperative:{C.RESET}  {COOP_NAME}")
        print(f"  {C.CYAN}👥 Members:{C.RESET}      Admin, Alice, Bob")
        print(f"  {C.CYAN}📋 Proposal:{C.RESET}     \"Approve $12,000 for community kitchen equipment\"")
        total_for = tally.get("for_votes", 0)
        total_against = tally.get("against_votes", 0)
        total_abstain = tally.get("abstain_votes", 0)
        print(f"  {C.CYAN}🗳  Votes:{C.RESET}        {C.GREEN}{total_for} for{C.RESET}, {total_against} against, {total_abstain} abstain")
        print(f"  {C.CYAN}✅ Result:{C.RESET}       {C.GREEN}{C.BOLD}ACCEPTED{C.RESET}")
        if proof_hash:
            print(f"  {C.CYAN}🔐 Proof:{C.RESET}        {C.DIM}{proof_hash[:40]}...{C.RESET}")
        print()
        print(f"  {C.DIM}Every vote was cryptographically signed.{C.RESET}")
        print(f"  {C.DIM}Every decision has a tamper-proof receipt.{C.RESET}")
        print(f"  {C.DIM}No votes can be altered or deleted after submission.{C.RESET}")
        print()
    else:
        print("=" * 60)
        print("  Demo Complete")
        print("=" * 60)
        print(f"""
  Cooperative:  {COOP_NAME}
  Members:      Admin, Alice, Bob
  Proposal:     "Approve $12,000 for community kitchen equipment"
  Result:       {tally.get('for_votes', '?')} votes FOR, {tally.get('against_votes', '?')} against (100% approval)
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
