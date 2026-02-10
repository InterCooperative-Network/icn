# Social Recovery Guide

**Status**: MVC Complete (Phase 12)
**Version**: 1.0
**Last Updated**: 2025-01-15

## Table of Contents

1. [Overview](#overview)
2. [User Guide](#user-guide)
   - [Setting Up Social Recovery](#setting-up-social-recovery)
   - [Initiating Recovery](#initiating-recovery)
   - [Attesting to a Recovery (Trustees)](#attesting-to-a-recovery-trustees)
   - [Finalizing Recovery](#finalizing-recovery)
   - [Cancelling a Recovery](#cancelling-a-recovery)
3. [Technical Overview](#technical-overview)
   - [Architecture](#architecture)
   - [Security Model](#security-model)
   - [Gossip Protocol](#gossip-protocol)
   - [State Transitions](#state-transitions)
4. [Operator Guide](#operator-guide)
   - [Monitoring Recoveries](#monitoring-recoveries)
   - [Troubleshooting](#troubleshooting)
   - [Incident Response](#incident-response)
5. [Security Considerations](#security-considerations)
6. [FAQ](#faq)

---

## Overview

Social recovery allows users to regain access to their ICN identity if they lose all their devices. Instead of relying on seed phrases or backup files (which can be lost or stolen), social recovery uses a network of trusted contacts (trustees) who can collectively vouch for your identity through out-of-band verification.

### Key Features

- **M-of-N Trust Model**: Configure how many trustees (M) need to attest out of total trustees (N)
- **Delay Period**: Fraud protection window where the original owner can cancel
- **Identity Migration**: Trust relationships and ledger balances automatically transfer to new DID
- **Gossip-Based**: Distributed coordination with no central authority
- **Audit Trail**: Complete history of attestations and recovery events

### When to Use

Social recovery is appropriate when:
- ✅ You've lost all devices with your ICN identity
- ✅ You cannot access your encrypted keystore
- ✅ You have pre-configured trustees who can verify your identity

Social recovery is **NOT** for:
- ❌ Compromised keys (use key rotation instead)
- ❌ Single device loss when you have others (use device management)
- ❌ Password reset (use passphrase recovery)

---

## User Guide

### Setting Up Social Recovery

**Prerequisites:**
- Active ICN identity
- 2+ trusted contacts with ICN identities
- Trust relationships established with those contacts

**Step 1: Choose Your Trustees**

Select people who:
- Know you well personally
- Are technically competent enough to use ICN
- Are reachable through multiple channels (phone, video call, in-person)
- Are geographically distributed (reduces attack surface)

**Recommended**: 3-5 trustees with 2-3 threshold (67% majority)

**Step 2: Configure Recovery**

```bash
# Example: 2-of-3 recovery with 24-hour delay
icnctl recovery setup \
  did:icn:alice,did:icn:bob,did:icn:carol \
  --threshold 2 \
  --delay 86400
```

**Parameters:**
- **Trustees**: Comma-separated list of DIDs
- **Threshold**: How many attestations needed (--threshold M)
- **Delay**: Fraud protection period in seconds (default: 24 hours)

**Step 3: Verify Configuration**

```bash
icnctl recovery config
```

Output:
```
Recovery Configuration:
  Identity: did:icn:yourdid

Trustees (3):
  1. did:icn:alice
  2. did:icn:bob
  3. did:icn:carol

Threshold: 2 of 3
Delay period: 86400 seconds (24 hours)
Method: Social recovery (2-of-3)
```

**Step 4: Inform Your Trustees**

Contact each trustee and:
1. Let them know they're your recovery trustee
2. Explain how to verify your identity (video call, in-person, etc.)
3. Share their recovery responsibilities

---

### Initiating Recovery

**When you've lost all devices:**

**Step 1: Create New Identity**

On a new device:
```bash
# Initialize new ICN identity
icnctl id init
```

This creates a new DID that will become your recovered identity.

**Step 2: Get Your New DID**

```bash
icnctl id show
```

Note your new DID (e.g., `did:icn:new123...`)

**Step 3: Contact Trustees**

Reach out to your trustees through **out-of-band channels** (phone, video call, in-person) and provide:
- Your old DID (the one you lost)
- Your new DID (from step 2)
- Proof of your identity (video call, shared secrets, etc.)

**Step 4: Initiate Recovery**

```bash
icnctl recovery initiate did:icn:olduser
```

This broadcasts a recovery request to the network.

**Step 5: Wait for Attestations**

Your trustees will receive the recovery notification through the ICN gossip network. They need to:
1. Verify your identity (video call, etc.)
2. Attest to your recovery using `icnctl recovery attest`

**Step 6: Monitor Progress**

```bash
# Check recovery status
icnctl recovery status <recovery-id>
```

Output shows:
- Threshold progress (e.g., "2 of 3 attestations received")
- Current state (Pending, Delayed, ReadyToFinalize)
- Attestation details

---

### Attesting to a Recovery (Trustees)

**When someone asks you to attest:**

**Step 1: Verify Identity Out-of-Band**

Before attesting, **you must verify the person's identity** through:
- ✅ Video call (see their face, hear their voice)
- ✅ In-person meeting
- ✅ Multi-factor verification (multiple channels)

**DO NOT attest based on:**
- ❌ Text messages alone
- ❌ Emails (easily spoofed)
- ❌ Social media DMs

**Step 2: Attest to Recovery**

```bash
icnctl recovery attest \
  <recovery-id> \
  did:icn:olduser \
  did:icn:newuser \
  --verification "Video call on 2025-01-15, confirmed identity via shared history"
```

**Parameters:**
- `recovery-id`: The unique recovery identifier
- `old_did`: The DID being recovered
- `new_did`: The new DID that will inherit the identity
- `--verification`: Description of how you verified their identity

**Step 3: Confirm**

The system will prompt:
```
⚠️  IMPORTANT: You are attesting that you have verified the identity of the person
requesting recovery through out-of-band means (video call, in-person, etc.).

Attesting to a fraudulent recovery could harm the legitimate user.

Old DID: did:icn:olduser
New DID: did:icn:newuser

Proceed with attestation? [y/N]:
```

Type `y` and press Enter to confirm.

---

### Finalizing Recovery

Once the threshold is reached and the delay period has expired:

```bash
icnctl recovery finalize <recovery-id>
```

This triggers:
1. **Trust Graph Migration**: All edges from old_did → new_did
2. **Ledger Transfer**: All balances from old_did → new_did
3. **Finalization Broadcast**: Network updated via gossip

**Verification:**

```bash
# Check new identity's trust relationships
icnctl trust list

# Check new identity's ledger balances
icnctl ledger balance
```

---

### Cancelling a Recovery

**If you detect fraud** (someone trying to steal your identity):

```bash
icnctl recovery cancel <recovery-id> "Fraudulent recovery attempt detected"
```

This immediately stops the recovery and broadcasts the cancellation.

**Fraud Protection:**

The delay period (default: 24 hours) gives you time to:
- Monitor for unexpected recovery attempts
- Cancel fraudulent recoveries before finalization
- Contact your trustees if something seems wrong

---

## Technical Overview

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Social Recovery System                    │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────┐      ┌──────────────┐      ┌────────────┐ │
│  │   Recovery   │      │    Gossip    │      │   Trust    │ │
│  │  Primitives  │─────→│   Protocol   │◄────→│   Graph    │ │
│  │  (RecoveryEvent)│      │(identity:recovery)│      │  (mapping) │ │
│  └──────────────┘      └──────────────┘      └────────────┘ │
│         │                      │                      │       │
│         │                      │                      │       │
│         ▼                      ▼                      ▼       │
│  ┌──────────────┐      ┌──────────────┐      ┌────────────┐ │
│  │  CLI Tools   │      │   Daemon     │      │   Ledger   │ │
│  │  (icnctl)    │      │ (supervisor) │      │  (transfer)│ │
│  └──────────────┘      └──────────────┘      └────────────┘ │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

**Components:**

1. **Recovery Primitives** (`icn-identity/src/recovery.rs`):
   - `RecoveryEvent`: State machine (Pending → Delayed → ReadyToFinalize → Finalized)
   - `RecoveryAttestation`: Ed25519-signed trustee attestation
   - `RecoveryMessage`: Gossip message types (Initiated, Attestation, Finalized, Cancelled)

2. **Gossip Protocol** (`identity:recovery` topic):
   - Initiated: Broadcast recovery request
   - Attestation: Trustees publish attestations
   - Finalized: Trigger trust/ledger migration
   - Cancelled: Fraud protection

3. **Trust Graph Integration** (`icn-trust/src/lib.rs`):
   - `map_did_recovery()`: Migrates edges old_did → new_did
   - Clears cache to force recomputation

4. **Ledger Integration** (`icn-ledger/src/ledger.rs`):
   - `transfer_balances_for_recovery()`: Double-entry journal entries
   - Transfers both credits (positive balances) and debts (negative balances)

5. **Daemon Integration** (`icn-core/src/supervisor.rs`):
   - Subscribes to `identity:recovery` topic
   - Handles incoming messages
   - Triggers trust/ledger updates on finalization

### Security Model

**Trust Assumptions:**

1. **M-of-N Trustees**: At least M trustees are honest and will verify identity properly
2. **Delay Period**: Original owner monitors their identity and can cancel fraud
3. **Ed25519 Signatures**: Attestations are cryptographically signed
4. **Gossip Integrity**: Network propagates messages reliably

**Attack Vectors & Mitigations:**

| Attack | Mitigation |
|--------|-----------|
| **Collusion** (M trustees collude to steal identity) | Choose diverse, independent trustees; higher threshold (e.g., 3-of-5) |
| **Social Engineering** (attacker tricks trustees) | Require out-of-band verification; trustee education |
| **Delay Period Bypass** | Enforced by state machine; cannot finalize before delay expires |
| **Replay Attacks** | Each attestation is unique (signed with timestamp + recovery_id) |
| **Sybil Trustees** | User manually selects trustees; trust relationships required |

**Cryptographic Guarantees:**

- Attestations use Ed25519 signatures (128-bit security)
- Recovery IDs are unique (UUID v4)
- State transitions are validated before application

### Gossip Protocol

**Topic**: `identity:recovery`

**Message Types:**

```rust
pub enum RecoveryMessage {
    Initiated {
        id: String,              // Recovery UUID
        old_did: Did,            // DID being recovered
        new_did: Did,            // New DID inheriting identity
        threshold: usize,        // M-of-N threshold
        delay_period: u64,       // Fraud protection delay (seconds)
        timestamp: u64,          // When initiated
    },
    Attestation {
        recovery_id: String,     // Recovery this applies to
        attestation: RecoveryAttestation,
        timestamp: u64,
    },
    Finalized {
        id: String,
        old_did: Did,
        new_did: Did,
        timestamp: u64,
    },
    Cancelled {
        id: String,
        cancelled_by: Did,
        reason: String,
        timestamp: u64,
    },
}
```

**Message Flow:**

```
User A (lost device)          Trustees (B, C, D)              Network
────────────────              ──────────────────              ───────

1. Initiate
   ├─ Initiated ────────────────────────────────────────────→ Broadcast

2. Attestations
   ◄──────────────────────── Attestation (B) ────────────────
   ◄──────────────────────── Attestation (C) ────────────────
   (Threshold reached: 2/3)

3. Delay Period
   [24 hours fraud protection window]

4. Finalize
   ├─ Finalized ──────────────────────────────────────────────→ Broadcast
   ├─ Trust graph migration
   └─ Ledger balance transfer
```

**Persistence:**

Each node maintains a recovery store (`{data_dir}/recovery/`):
- Key format: `recovery:{id}`
- Value: JSON-serialized `RecoveryEvent`
- State persists across daemon restarts

### State Transitions

```
┌──────────┐
│  Pending │  ← Initial state when recovery is initiated
└────┬─────┘
     │ add_attestation() x M times
     ▼
┌──────────┐
│ Delayed  │  ← Threshold reached, delay period active
└────┬─────┘
     │ check_delay_expired() after delay_period
     ▼
┌────────────────┐
│ReadyToFinalize │  ← Ready to finalize (delay expired)
└────┬───────────┘
     │ finalize()
     ▼
┌──────────┐
│Finalized │  ← Recovery complete (trust/ledger migrated)
└──────────┘

     OR

┌──────────┐
│Cancelled │  ← Fraud detected, recovery cancelled
└──────────┘
```

**Invariants:**

1. Cannot finalize without reaching threshold
2. Cannot finalize during delay period
3. Cannot add attestations after finalization/cancellation
4. Cannot have duplicate trustees (same DID attesting twice)

---

## Operator Guide

### Monitoring Recoveries

**View Active Recoveries:**

```bash
# List all recoveries in store
ls ~/.icn/recovery/
```

**Check Recovery Status:**

```bash
# View recovery details
icnctl recovery status <recovery-id>
```

**Prometheus Metrics:**

```
# Total recoveries initiated
icn_recovery_initiated_total

# Attestations received
icn_recovery_attestations_total

# Finalizations completed
icn_recovery_finalized_total

# Cancellations
icn_recovery_cancelled_total
```

**Gossip Topic Monitoring:**

```
# Subscribe to recovery topic
icnctl gossip subscribe identity:recovery

# Monitor message flow
tail -f ~/.icn/logs/icnd.log | grep "recovery"
```

### Troubleshooting

**Problem**: Recovery not progressing

**Possible Causes:**
1. Trustees haven't attested yet
2. Network partition (gossip not propagating)
3. Threshold not yet reached
4. Delay period still active

**Debugging Steps:**

```bash
# Check recovery status
icnctl recovery status <recovery-id>

# Check gossip connectivity
icnctl status

# Check logs for errors
journalctl -u icnd -f | grep recovery

# Verify trustees received the message
# (Contact trustees out-of-band)
```

**Problem**: Trust graph or ledger migration failed

**Possible Causes:**
1. Finalization received for unknown recovery (Bug 2 - now fixed)
2. Network issue during finalization
3. Store corruption

**Debugging Steps:**

```bash
# Check trust graph state
icnctl trust list

# Check ledger balances
icnctl ledger balance

# Check daemon logs
grep "Trust graph: migrated" ~/.icn/logs/icnd.log
grep "Ledger: transferred" ~/.icn/logs/icnd.log
```

**Problem**: Duplicate attestations rejected

**Expected Behavior**: Same trustee cannot attest twice (security feature)

**Solution**: No action needed - this is correct behavior

### Incident Response

**Scenario 1: Fraudulent Recovery Detected**

1. **Cancel immediately**:
   ```bash
   icnctl recovery cancel <recovery-id> "Fraud detected"
   ```

2. **Investigate**:
   - Who initiated the recovery?
   - Which trustees attested (if any)?
   - How did attacker obtain old/new DID?

3. **Contact trustees** (out-of-band):
   - Alert them about fraud attempt
   - Review verification procedures

4. **Consider key rotation** if original identity compromised

**Scenario 2: Legitimate User Locked Out**

1. **Verify it's actually the legitimate user** (out-of-band)

2. **Check recovery status**:
   ```bash
   icnctl recovery status <recovery-id>
   ```

3. **If stuck in Delayed state**:
   - Wait for delay period to expire
   - Then finalize:
   ```bash
   icnctl recovery finalize <recovery-id>
   ```

4. **If insufficient attestations**:
   - Contact remaining trustees
   - Provide verification instructions

---

## Security Considerations

### Choosing Trustees

**Best Practices:**

- ✅ Choose people you know personally (not online-only contacts)
- ✅ Geographic diversity (different cities/countries)
- ✅ Relationship diversity (family, friends, colleagues)
- ✅ Technical competence (can use ICN CLI)
- ✅ Stability (won't disappear or become unreachable)

**Anti-Patterns:**

- ❌ All trustees from same organization (correlated risk)
- ❌ Strangers or weak social ties
- ❌ Too few trustees (2-of-2 has no redundancy)
- ❌ Too many trustees (coordination overhead)

### Delay Period Configuration

**Recommendations:**

- **High-value identities**: 7 days (604800 seconds)
- **Normal use**: 24 hours (86400 seconds - default)
- **Low-risk testing**: 1 hour (3600 seconds)

**Trade-offs:**

- Longer delay = more fraud protection, longer recovery time
- Shorter delay = faster recovery, less fraud protection

### Attestation Verification

**Trustees MUST verify identity through:**

1. **Visual verification** (video call, in-person)
2. **Voice verification** (phone call)
3. **Shared knowledge** (personal questions only you could answer)
4. **Multi-channel confirmation** (verify through 2+ independent channels)

**Never attest based on:**

- Single channel communication (email/SMS)
- Unverified digital signatures
- Hearsay or third-party confirmation

---

## FAQ

**Q: Can I change my trustees after setup?**

A: Yes. Run `icnctl recovery setup` again with new trustees. The configuration is stored in your DID Document.

**Q: What happens if I lose access before setting up recovery?**

A: Unfortunately, without pre-configured social recovery, your identity cannot be recovered. This is why setting up recovery early is critical.

**Q: Can trustees see my ledger balances or trust relationships?**

A: No. Trustees only attest to your identity. They don't have access to your data.

**Q: What if a trustee loses their device?**

A: If you have M-of-N with redundancy (e.g., 2-of-3), losing one trustee is fine. Otherwise, update your recovery configuration to replace them.

**Q: Can I be my own trustee?**

A: Technically yes, but this defeats the purpose. Social recovery assumes you've lost all devices.

**Q: How is this different from Ethereum social recovery?**

A: Similar concept, but ICN uses gossip (no blockchain), Ed25519 signatures (vs smart contracts), and integrates with trust graph + mutual credit ledger.

**Q: Can I use multiple recovery methods simultaneously?**

A: Currently no. You configure one recovery method (Social, BackupSeed, or None). Future versions may support fallback methods.

**Q: What's the maximum number of trustees?**

A: No hard limit, but 3-5 is recommended for practical coordination.

**Q: How long does recovery take?**

A: Minimum: delay_period (default 24 hours). Maximum: depends on trustee response time.

**Q: Is the recovery process private?**

A: Partially. The gossip messages are visible to the network, but attestation verification details are off-chain (video calls, etc.).

---

## Additional Resources

- [Multi-Device Identity Design](multi-device-identity-design.md)
- [ICN Identity Crate Documentation](../crates/icn-identity/src/recovery.rs)
- [End-to-End Recovery Test](../crates/icn-core/tests/recovery_integration.rs)

## Changelog

**v1.0 (2025-01-15)**:
- Initial documentation for Phase 12 MVC
- Complete user guide + operator guide
- Security considerations and FAQ
