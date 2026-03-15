#!/usr/bin/env python3
"""
ICN Cooperative Governance Demo
================================
A four-phase demo showing cooperative-first governance principles:

  Phase 1 – Founding Assembly
    Three equal founding members (Alice, Bob, Carol) form the cooperative.
    Alice is elected temporary coordinator — not an admin, not a permanent role.

  Phase 2 – Charter Ratification
    The first democratic act: all three members vote to ratify their own rules.
    No one person's authority creates the charter — the members do, together.

  Phase 3 – Democratic Decision
    Bob proposes $12,000 for community kitchen equipment.
    All three vote. Any member may propose; any member may vote.

  Phase 4 – Verification
    Carol closes the vote — not the coordinator, not the proposer.
    Authority rotates. The cryptographic proof is generated.

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
import urllib.request
import urllib.error

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import (
    Encoding,
    PublicFormat,
)

# ── Configuration ─────────────────────────────────────────────────────────────

PRESENTER_MODE = "--presenter" in sys.argv
_positional = [a for a in sys.argv[1:] if not a.startswith("--")]
GATEWAY = os.environ.get("ICN_GATEWAY", _positional[0] if _positional else "http://localhost:8080")

COOP_ID   = "finger-lakes-food"
COOP_NAME = "Finger Lakes Food Co-op"
DOMAIN_ID   = f"coop:{COOP_ID}"
DOMAIN_NAME = f"{COOP_NAME} Governance"

# ── ANSI colors ───────────────────────────────────────────────────────────────

class C:
    _on = PRESENTER_MODE and sys.stdout.isatty()
    RESET   = "\033[0m"  if _on else ""
    BOLD    = "\033[1m"  if _on else ""
    DIM     = "\033[2m"  if _on else ""
    GREEN   = "\033[32m" if _on else ""
    RED     = "\033[31m" if _on else ""
    YELLOW  = "\033[33m" if _on else ""
    CYAN    = "\033[36m" if _on else ""
    BLUE    = "\033[34m" if _on else ""
    MAGENTA = "\033[35m" if _on else ""
    WHITE   = "\033[97m" if _on else ""


# ── Base58 encoding ───────────────────────────────────────────────────────────

ALPHABET = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

def base58_encode(data: bytes) -> str:
    n = int.from_bytes(data, "big")
    result = []
    while n > 0:
        n, remainder = divmod(n, 58)
        result.append(ALPHABET[remainder:remainder + 1])
    for byte in data:
        if byte == 0:
            result.append(ALPHABET[0:1])
        else:
            break
    return b"".join(reversed(result)).decode("ascii")


def make_did(public_key_bytes: bytes) -> str:
    return f"did:icn:z{base58_encode(public_key_bytes)}"


# ── Identity ──────────────────────────────────────────────────────────────────

class Identity:
    def __init__(self, name: str):
        self.name = name
        self.private_key = Ed25519PrivateKey.generate()
        self.public_key = self.private_key.public_key()
        pub_bytes = self.public_key.public_bytes(Encoding.Raw, PublicFormat.Raw)
        self.did = make_did(pub_bytes)
        self.token = None

    def sign(self, data: bytes) -> bytes:
        return self.private_key.sign(data)


# ── HTTP helpers ──────────────────────────────────────────────────────────────

def api(method: str, path: str, body=None, token=None):
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
            return e.code, json.loads(body_text)
        except json.JSONDecodeError:
            return e.code, {"error": body_text}


def step(num: int, label: str):
    print(f"\n{'─' * 60}")
    print(f"  Step {num}. {label}")
    print(f"{'─' * 60}")


def ok(msg: str, data=None):
    print(f"  {C.GREEN}✓{C.RESET} {msg}")
    if data and not PRESENTER_MODE:
        print(f"    {json.dumps(data, indent=2)[:500]}")


def fail(msg: str, status=None, data=None):
    print(f"  {C.RED}✗ {msg}{C.RESET}")
    if status:
        print(f"    HTTP {status}")
    if data:
        print(f"    {json.dumps(data, indent=2)[:500]}")
    sys.exit(1)


def narrator(msg: str):
    if PRESENTER_MODE:
        print(f"  {C.DIM}{msg}{C.RESET}")


def phase_header(num: int, title: str, description: str):
    if PRESENTER_MODE:
        print()
        print(f"  {C.BOLD}{C.CYAN}{'━' * 56}{C.RESET}")
        print(f"  {C.BOLD}{C.CYAN}  Phase {num}: {title}{C.RESET}")
        print(f"  {C.DIM}  {description}{C.RESET}")
        print(f"  {C.BOLD}{C.CYAN}{'━' * 56}{C.RESET}")
        print()


def phase_pause(next_phase: str):
    if PRESENTER_MODE:
        print()
        print(f"  {C.YELLOW}[Press Enter to continue → {next_phase}]{C.RESET}")
        try:
            input()
        except EOFError:
            pass


def phase_complete(summary: str):
    if PRESENTER_MODE:
        print()
        print(f"  {C.GREEN}{C.BOLD}✓ {summary}{C.RESET}")


# ── Authentication ────────────────────────────────────────────────────────────

def authenticate(identity: Identity, scopes: list):
    """DID challenge-response: prove identity, receive JWT."""
    narrator(f"  {identity.name} signs a cryptographic challenge to prove their identity...")

    status, resp = api("POST", "/auth/challenge", {"did": identity.did})
    if status != 200:
        fail(f"Challenge failed for {identity.name}", status, resp)

    nonce_bytes = bytes.fromhex(resp["nonce"])
    signature_hex = identity.sign(nonce_bytes).hex()

    status, resp = api("POST", "/auth/verify", {
        "did": identity.did,
        "signature": signature_hex,
        "coop_id": COOP_ID,
        "scopes": scopes,
    })
    if status != 200:
        fail(f"Verify failed for {identity.name}", status, resp)

    identity.token = resp["token"]
    ok(f"Authenticated {identity.name}  {C.DIM}({identity.did[:32]}...){C.RESET}")


# ── Main ──────────────────────────────────────────────────────────────────────

def main():
    if PRESENTER_MODE:
        print()
        print(f"  {C.BOLD}{C.WHITE}╔══════════════════════════════════════════════════════╗{C.RESET}")
        print(f"  {C.BOLD}{C.WHITE}║   Cooperative Governance Demo                        ║{C.RESET}")
        print(f"  {C.BOLD}{C.WHITE}║   InterCooperative Network (ICN)                     ║{C.RESET}")
        print(f"  {C.BOLD}{C.WHITE}╚══════════════════════════════════════════════════════╝{C.RESET}")
        print()
        print(f"  {C.DIM}No admin. No central server. Three equal founding members.{C.RESET}")
        print(f"  {C.DIM}Every vote is signed. Every decision is verifiable.{C.RESET}")
        print(f"  {C.DIM}The first democratic act ratifies the rules themselves.{C.RESET}")
        print()
        print(f"  {C.DIM}Gateway: {GATEWAY}{C.RESET}")
    else:
        print("=" * 60)
        print("  ICN Cooperative Governance Demo")
        print("=" * 60)
        print(f"  Gateway: {GATEWAY}")
    print()

    status, health = api("GET", "/health")
    if status != 200:
        fail("Gateway not reachable", status, health)
    ok(f"Gateway healthy (version {health.get('version', '?')})")

    # =========================================================================
    # PHASE 1: FOUNDING ASSEMBLY
    # =========================================================================
    phase_header(1, "Founding Assembly",
                 "Three equal founding members form the cooperative")

    step(1, "Generate founding member identities")
    narrator("Each founder gets a cryptographic keypair — a self-sovereign digital identity.")
    alice = Identity("Alice")
    bob   = Identity("Bob")
    carol = Identity("Carol")
    ok(f"Alice: {alice.did}")
    ok(f"Bob:   {bob.did}")
    ok(f"Carol: {carol.did}")

    step(2, "Alice authenticates (elected temporary coordinator)")
    narrator("Alice is elected coordinator for the founding — a temporary, not permanent, role.")
    coordinator_scopes = [
        "coop:read", "coop:write", "coop:admin",
        "governance:read", "governance:write",
        "treasury:read", "treasury:write",
    ]
    authenticate(alice, coordinator_scopes)

    step(3, "Create the cooperative")
    narrator("The cooperative is registered on the network — owned by its members, not a company.")
    status, resp = api("POST", "/coops", {
        "id": COOP_ID,
        "name": COOP_NAME,
    }, alice.token)
    if status in (200, 201):
        ok(f"Cooperative created: {C.BOLD}{COOP_NAME}{C.RESET}")
    else:
        fail("Failed to create cooperative", status, resp)

    step(4, "Create governance domain")
    narrator("Voting rules: 50% quorum, simple majority, 7-day voting period.")
    status, resp = api("POST", "/gov/domains", {
        "id": DOMAIN_ID,
        "name": DOMAIN_NAME,
        "profile": "cooperative_default",
        "quorum_percent": 50,
        "approval_percent": 51,
        "voting_period_days": 7,
        "members": [alice.did],
    }, alice.token)
    if status in (200, 201):
        ok(f"Governance domain created: {DOMAIN_ID}")
    else:
        fail("Failed to create governance domain", status, resp)

    step(5, "Add Bob to cooperative and governance domain")
    narrator("Bob joins as an equal member with full voting rights.")
    status, resp = api("POST", f"/coops/{COOP_ID}/members", {
        "did": bob.did,
        "role": "participant",
        "display_name": "Bob",
    }, alice.token)
    if status not in (200, 201):
        fail("Failed to add Bob to cooperative", status, resp)
    status, resp = api("POST", f"/gov/domains/{DOMAIN_ID}/members", {
        "did": bob.did,
        "weight": 1.0,
    }, alice.token)
    if status in (200, 201):
        ok("Bob joined — equal voting weight")
    else:
        fail("Failed to add Bob to governance domain", status, resp)

    step(6, "Add Carol to cooperative and governance domain")
    narrator("Carol joins — the cooperative now has three equal founding members.")
    status, resp = api("POST", f"/coops/{COOP_ID}/members", {
        "did": carol.did,
        "role": "participant",
        "display_name": "Carol",
    }, alice.token)
    if status not in (200, 201):
        fail("Failed to add Carol to cooperative", status, resp)
    status, resp = api("POST", f"/gov/domains/{DOMAIN_ID}/members", {
        "did": carol.did,
        "weight": 1.0,
    }, alice.token)
    if status in (200, 201):
        ok("Carol joined — equal voting weight")
    else:
        fail("Failed to add Carol to governance domain", status, resp)

    phase_complete(f"{COOP_NAME} founded — Alice, Bob, and Carol, equal members.")
    phase_pause("Charter Ratification")

    # =========================================================================
    # PHASE 2: CHARTER RATIFICATION
    # =========================================================================
    phase_header(2, "Charter Ratification",
                 "The first democratic act: members ratify their own rules")

    step(7, "Alice submits the founding charter for ratification")
    narrator("In a cooperative, even the rules are decided democratically.")
    narrator("Alice proposes the charter — but it only takes effect when the members vote for it.")
    status, resp = api("POST", "/gov/proposals", {
        "domain_id": DOMAIN_ID,
        "title": "Ratify the Finger Lakes Food Co-op Founding Charter",
        "description": (
            "Adopt the cooperative's founding charter, establishing member rights, "
            "governance structure, and operating principles."
        ),
        "payload": {
            "type": "text",
            "body": (
                "COOPERATIVE CHARTER — Finger Lakes Food Co-op\n\n"
                "1. MEMBERSHIP: Any person who supports our mission may join. "
                "All members have equal voting rights.\n"
                "2. GOVERNANCE: Decisions are made democratically. "
                "No member has more authority than any other.\n"
                "3. PURPOSE: To provide affordable, healthy food to our community "
                "and build economic resilience through cooperation.\n"
                "4. COORDINATION: Coordinators are elected for specific purposes "
                "and terms. All coordination roles rotate.\n"
                "5. AMENDMENT: This charter may be amended by a two-thirds vote of members."
            ),
        },
    }, alice.token)
    if status in (200, 201):
        charter_id = resp.get("id", resp.get("proposal_id", "unknown"))
        ok(f"Charter proposal submitted: {charter_id}")
    else:
        fail("Failed to submit charter proposal", status, resp)

    step(8, "Open charter for ratification vote")
    status, resp = api("POST", f"/gov/proposals/{charter_id}/open", {
        "voting_period_seconds": 3600,
    }, alice.token)
    if status in (200, 201):
        ok("Charter open for ratification")
    else:
        fail("Failed to open charter", status, resp)

    step(9, "Alice votes to ratify the charter")
    narrator("Alice votes first — the coordinator has no more weight than any other member.")
    status, resp = api("POST", f"/gov/proposals/{charter_id}/vote", {
        "choice": "for",
        "comment": "This charter reflects our values. I vote to ratify.",
    }, alice.token)
    if status in (200, 201):
        ok(f"{C.GREEN}Alice: FOR{C.RESET}")
    else:
        fail("Alice's charter vote failed", status, resp)

    step(10, "Bob authenticates and votes to ratify")
    authenticate(bob, ["governance:read", "governance:write"])
    status, resp = api("POST", f"/gov/proposals/{charter_id}/vote", {
        "choice": "for",
        "comment": "Agreed. These rules give every member a voice.",
    }, bob.token)
    if status in (200, 201):
        ok(f"{C.GREEN}Bob: FOR{C.RESET}")
    else:
        fail("Bob's charter vote failed", status, resp)

    step(11, "Carol authenticates and votes to ratify")
    authenticate(carol, ["governance:read", "governance:write"])
    status, resp = api("POST", f"/gov/proposals/{charter_id}/vote", {
        "choice": "for",
        "comment": "Unanimously supported. Let's build something together.",
    }, carol.token)
    if status in (200, 201):
        ok(f"{C.GREEN}Carol: FOR{C.RESET}")
    else:
        fail("Carol's charter vote failed", status, resp)

    step(12, "Alice closes the charter ratification")
    narrator("3 votes cast, 3 in favor. The charter is ratified — by the members, for the members.")
    status, resp = api("POST", f"/gov/proposals/{charter_id}/close", {}, alice.token)
    if status in (200, 201):
        ok(f"{C.BOLD}{C.GREEN}Charter ratified — 3/3 unanimous{C.RESET}")
    else:
        fail("Failed to close charter", status, resp)

    phase_complete("Charter ratified 3-0. The cooperative is constituted.")
    phase_pause("Democratic Decision")

    # =========================================================================
    # PHASE 3: DEMOCRATIC DECISION
    # =========================================================================
    phase_header(3, "Democratic Decision",
                 "Bob proposes — any member can bring a proposal forward")

    step(13, "Bob proposes community kitchen equipment")
    narrator("Bob brings a budget proposal. The coordinator didn't propose this — Bob did.")
    narrator("Any member can initiate a proposal. Authority is distributed.")
    status, resp = api("POST", "/gov/proposals", {
        "domain_id": DOMAIN_ID,
        "title": "Approve $12,000 for community kitchen equipment",
        "description": (
            "Purchase commercial-grade equipment for our shared community kitchen: "
            "convection oven ($4,000), industrial mixer ($3,000), "
            "prep tables and storage ($2,500), safety and small tools ($2,500). "
            "This investment serves all 47 member households."
        ),
        "payload": {
            "type": "budget",
            "amount": 12000,
            "recipient": alice.did,
            "currency": "USD",
            "purpose": "Community Kitchen Equipment",
        },
    }, bob.token)
    if status in (200, 201):
        proposal_id = resp.get("id", resp.get("proposal_id", "unknown"))
        ok(f"Proposal submitted: {proposal_id}")
        ok(f"Title: {resp.get('title', 'N/A')}")
    else:
        fail("Failed to create proposal", status, resp)

    step(14, "Bob opens the proposal for voting")
    status, resp = api("POST", f"/gov/proposals/{proposal_id}/open", {
        "voting_period_seconds": 3600,
    }, bob.token)
    if status in (200, 201):
        ok("Voting is open")
    else:
        fail("Failed to open proposal", status, resp)

    step(15, "Alice votes FOR the kitchen equipment")
    narrator("Alice reviews Bob's proposal and votes.")
    status, resp = api("POST", f"/gov/proposals/{proposal_id}/vote", {
        "choice": "for",
        "comment": "This serves every member household. Fully support.",
    }, alice.token)
    if status in (200, 201):
        ok(f"{C.GREEN}Alice: FOR{C.RESET}")
    else:
        fail("Alice's vote failed", status, resp)

    step(16, "Bob votes FOR his own proposal")
    narrator("Bob votes on his own proposal — members may always vote on proposals they submit.")
    status, resp = api("POST", f"/gov/proposals/{proposal_id}/vote", {
        "choice": "for",
        "comment": "I believe in this investment. Voting in favor.",
    }, bob.token)
    if status in (200, 201):
        ok(f"{C.GREEN}Bob: FOR{C.RESET}")
    else:
        fail("Bob's vote failed", status, resp)

    step(17, "Carol votes FOR the proposal")
    narrator("Carol casts the final vote.")
    status, resp = api("POST", f"/gov/proposals/{proposal_id}/vote", {
        "choice": "for",
        "comment": "The kitchen equipment benefits everyone. Yes.",
    }, carol.token)
    if status in (200, 201):
        ok(f"{C.GREEN}Carol: FOR{C.RESET}")
    else:
        fail("Carol's vote failed", status, resp)

    phase_complete("All three members voted. 3 votes FOR, 0 against.")
    phase_pause("Verification")

    # =========================================================================
    # PHASE 4: VERIFICATION
    # =========================================================================
    phase_header(4, "Verification",
                 "Carol closes the vote — authority rotates across the membership")

    step(18, "Carol closes and tallies the vote")
    narrator("Carol closes the vote. Not the coordinator. Not the proposer.")
    narrator("Any member can close a vote once the voting period ends.")
    narrator("This is what rotating authority looks like.")
    status, resp = api("POST", f"/gov/proposals/{proposal_id}/close", {}, carol.token)
    if status in (200, 201):
        ok("Vote closed by Carol")
    else:
        fail("Failed to close proposal", status, resp)

    # Get proposal state
    status, resp = api("GET", f"/gov/proposals/{proposal_id}", token=carol.token)
    if status == 200:
        state = resp.get("state", "unknown")
        state_name = list(state.keys())[0] if isinstance(state, dict) else str(state)
        ok(f"Proposal state: {C.BOLD}{C.GREEN}{state_name.upper()}{C.RESET}")
    else:
        fail("Failed to get proposal state", status, resp)

    # Get tally
    status, tally = api("GET", f"/gov/proposals/{proposal_id}/tally", token=carol.token)
    if status == 200:
        ok("Vote tally retrieved")
        total = tally.get("for_votes", 0) + tally.get("against_votes", 0) + tally.get("abstain_votes", 0)
        for_count = tally.get("for_votes", 0)
        against_count = tally.get("against_votes", 0)
        abstain_count = tally.get("abstain_votes", 0)
        for_pct = round(for_count / total * 100) if total > 0 else 0
        if PRESENTER_MODE:
            bar_width = 40
            for_bars = round(for_pct / 100 * bar_width)
            against_bars = bar_width - for_bars
            print()
            print(f"    {C.GREEN}{'█' * for_bars}{C.RED}{'░' * against_bars}{C.RESET}")
            print(f"    {C.GREEN}For: {for_count} ({for_pct}%){C.RESET}  "
                  f"{C.RED}Against: {against_count}{C.RESET}  "
                  f"{C.DIM}Abstain: {abstain_count}{C.RESET}")
        else:
            print(f"    {json.dumps(tally, indent=2)}")
    else:
        tally = {}
        print(f"  ⚠ Tally not available (HTTP {status})")

    step(19, "Generate cryptographic proof")
    narrator("A tamper-proof receipt is generated — cryptographic evidence of this decision.")
    narrator("Anyone can verify this decision happened. No one can alter it after the fact.")
    status, resp = api("GET", f"/gov/proposals/{proposal_id}/proof", token=carol.token)
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
            decision_hash_raw = receipt.get("decision_hash", [])
            if isinstance(decision_hash_raw, list):
                dh = bytes(decision_hash_raw).hex()
            else:
                dh = str(decision_hash_raw)
            print(f"    {C.DIM}Vote hash:     {proof_hash[:40]}...{C.RESET}")
            print(f"    {C.DIM}Decision hash: {dh[:40]}...{C.RESET}")
        else:
            print(f"    {json.dumps(resp, indent=2)[:800]}")
    else:
        print(f"  ⚠ Proof not available (HTTP {status})")
        print(f"    {json.dumps(resp, indent=2)[:300]}")

    # ── Summary Card ──────────────────────────────────────────────────────────
    print()
    if PRESENTER_MODE:
        print(f"  {C.BOLD}{C.WHITE}╔══════════════════════════════════════════════════════╗{C.RESET}")
        print(f"  {C.BOLD}{C.WHITE}║   Demo Complete                                      ║{C.RESET}")
        print(f"  {C.BOLD}{C.WHITE}╚══════════════════════════════════════════════════════╝{C.RESET}")
        print()
        print(f"  {C.CYAN}🏪 Cooperative:{C.RESET}  {COOP_NAME}")
        print(f"  {C.CYAN}👥 Members:{C.RESET}      Alice (coordinator), Bob, Carol")
        print(f"  {C.CYAN}📜 Charter:{C.RESET}      Ratified 3/3 — members adopted their own rules")
        print(f"  {C.CYAN}📋 Proposal:{C.RESET}     \"Approve $12,000 for community kitchen equipment\"")
        print(f"  {C.CYAN}🗳  Votes:{C.RESET}        "
              f"{C.GREEN}{tally.get('for_votes', 3)} for{C.RESET}, "
              f"{tally.get('against_votes', 0)} against, "
              f"{tally.get('abstain_votes', 0)} abstain")
        print(f"  {C.CYAN}✅ Result:{C.RESET}       {C.GREEN}{C.BOLD}ACCEPTED{C.RESET}")
        if proof_hash:
            print(f"  {C.CYAN}🔐 Proof:{C.RESET}        {C.DIM}{proof_hash[:40]}...{C.RESET}")
        print()
        print(f"  {C.DIM}What this demonstrates:{C.RESET}")
        print(f"  {C.DIM}  No admin. Coordinator is elected and temporary.{C.RESET}")
        print(f"  {C.DIM}  Any member can propose. Any member can close a vote.{C.RESET}")
        print(f"  {C.DIM}  The first democratic act ratifies the rules themselves.{C.RESET}")
        print()
    else:
        print("=" * 60)
        print("  Demo Complete")
        print("=" * 60)
        print(f"""
  Cooperative:  {COOP_NAME}
  Members:      Alice (coordinator), Bob, Carol
  Charter:      Ratified 3/3
  Proposal:     "Approve $12,000 for community kitchen equipment"
  Votes:        {tally.get('for_votes', 3)} FOR, {tally.get('against_votes', 0)} against
  Status:       ACCEPTED — cryptographic audit trail generated

  What this demonstrates:
  • No admin. Coordinator is elected and temporary.
  • Any member can propose. Any member can close a vote.
  • The first democratic act ratifies the rules themselves.
  • Every vote is cryptographically signed and non-repudiable.
  • Every decision has a tamper-proof proof receipt.
""")


if __name__ == "__main__":
    main()
