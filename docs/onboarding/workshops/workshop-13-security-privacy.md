# Workshop 13: Security and Privacy Layers

## Learning Objectives

By the end of this workshop, you will be able to:

1. **Trace message security** - Follow a signed envelope through verification
2. **Explain replay protection** - Understand how sequence tracking works
3. **Identify trust-gated controls** - Locate rate limiting and access checks

## Goal
Understand how ICN enforces authenticity, integrity, and confidentiality through
layered security and privacy mechanisms.

## Prerequisites
- Completed [Module 13: Security and Privacy](../modules/module-13-security-privacy.md)
- Familiarity with Module 4 (Identity) and Module 5 (Network)

## Estimated time
1-2 hours

## Related Materials
- `icn/crates/icn-net/src/envelope.rs`
- `icn/crates/icn-security/`
- `icn/crates/icn-privacy/`

---

## Part 1: Signed Envelopes

### Steps
1. Open `icn/crates/icn-net/src/envelope.rs`
2. Locate the signed envelope structure
3. Identify fields responsible for authenticity and replay protection

### Questions
1. How is the sender identified?
2. Where is the signature verified?

### Checkpoint
- [ ] You can explain what makes an envelope authentic

---

## Part 2: Replay Guard

### Steps
1. Search for replay guard logic in `icn/crates/icn-net/`
2. Identify how sequence numbers are stored and checked

### Questions
1. What happens when a sequence is repeated?
2. How is state persisted across restarts?

### Checkpoint
- [ ] You can explain how replay attacks are prevented

---

## Part 3: Trust-Gated Controls

### Steps
1. Open `icn/crates/icn-security/` and find rate limiting
2. Identify where trust classes are defined and used

### Questions
1. How does trust class map to rate limits?
2. Where are access controls applied for gossip topics?

### Checkpoint
- [ ] You can connect trust scores to enforcement behavior

---

## Summary

After completing this workshop you should be able to:
- Trace signed envelope verification
- Explain replay protection
- Locate trust-gated access controls

## Next steps
Proceed to Workshop 14: Governance and CCL Deep Dive
