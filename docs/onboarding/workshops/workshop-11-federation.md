# Workshop 11: Federation Hands-On

## Goal
Explore federation code paths, understand agreement workflows, and trace
cross-cooperative message handling.

## Prerequisites
- Completed Module 11 reading
- Familiarity with gossip concepts from Module 5

## Estimated time
2-3 hours

## Part 1: Cooperative Registry Exploration

### Steps
1. Open `icn/crates/icn-federation/src/registry.rs`
2. Find the `CooperativeRegistry` struct
3. Identify the methods for:
   - Adding a cooperative
   - Looking up a cooperative
   - Listing known cooperatives

### Questions to answer
1. What information is stored about each cooperative?
2. How is the registry persisted?
3. What happens when a duplicate cooperative is registered?

### Code to find
```rust
pub struct CooperativeInfo {
    pub coop_id: String,
    pub coop_name: String,
    pub did: Did,
    pub policy: FederationPolicy,
    // ...
}
```

### Checkpoint
- [ ] You found the CooperativeRegistry struct
- [ ] You understand what metadata is stored per cooperative

## Part 2: Agreement Lifecycle Walkthrough

### Steps
1. Open `icn/crates/icn-federation/src/agreement/types.rs`
2. Find the `AgreementStatus` enum
3. Open `icn/crates/icn-federation/src/agreement/manager.rs`
4. Trace the state transitions:
   - Draft → Proposed
   - Proposed → Active
   - Active → Suspended
   - Active → Terminated

### Exercise
Draw the state machine for agreement lifecycle. Include:
- All states
- Transitions with triggering methods
- Final states

### Expected states
```
Draft → Proposed → Active → Suspended → Active
                     ↓          ↓
                Terminated ← ─ ─ ┘
```

### Questions to answer
1. What triggers the transition from Proposed to Active?
2. Can a terminated agreement be reactivated?
3. How is agreement expiration handled?

### Checkpoint
- [ ] You drew the agreement state machine
- [ ] You found the `all_parties_signed` check

## Part 3: Agreement Types Deep Dive

### Steps
1. In `agreement/types.rs`, find `AgreementType` enum
2. List all agreement types
3. For each type, explain its purpose

### Expected findings
| Type | Purpose |
|------|---------|
| ResourceSharing | Share compute/storage resources |
| ClearingArrangement | Settle cross-coop transactions |
| MutualRecognition | Recognize members/attestations |
| JointGovernance | Coordinate shared decisions |
| Custom | Domain-specific agreements |

### Questions to answer
1. What terms would a ResourceSharing agreement include?
2. How does MutualRecognition relate to attestations?
3. When would you use a Custom agreement?

### Checkpoint
- [ ] You can explain all agreement types
- [ ] You understand when to use each type

## Part 4: Clearing and Netting Exploration

### Steps
1. Open `icn/crates/icn-federation/src/clearing_manager.rs`
2. Find the `record_obligation` method
3. Find the `net_position` method
4. Read `icn/crates/icn-federation/src/netting_example.md`

### Netting calculation exercise
Given these obligations:
```
Coop A owes Coop B: 150
Coop B owes Coop A: 80
Coop A owes Coop C: 50
Coop C owes Coop A: 30
```

Calculate:
1. Net position between A and B: ___
2. Net position between A and C: ___
3. Total transfers with netting: ___
4. Total transfers without netting: ___

### Expected answers
1. A owes B: 70 (150 - 80)
2. A owes C: 20 (50 - 30)
3. With netting: 90 total
4. Without netting: 310 total

### Checkpoint
- [ ] You understand bilateral netting
- [ ] You calculated net positions correctly

## Part 5: Federation Gossip Handler

### Steps
1. Open `icn/crates/icn-federation/src/gossip.rs`
2. Find `FederationGossipHandler`
3. Identify the gossip topics handled:
   - `federation:announce`
   - `federation:agreements`
   - `federation:clearing`

### Questions to answer
1. What happens when an announcement is received?
2. How are agreement messages routed?
3. What is the send callback used for?

### Code to trace
```rust
pub fn handle_message(&self, topic: &str, data: &[u8]) -> Result<()>
```

### Checkpoint
- [ ] You found all federation gossip topics
- [ ] You understand the message routing logic

## Part 6: Federation Initialization

### Steps
1. Open `icn/crates/icn-core/src/supervisor/init_federation.rs`
2. Trace the initialization sequence:
   - Registry creation
   - Clearing manager setup
   - Attestation store initialization
   - Agreement manager creation
   - Gossip handler wiring

### Questions to answer
1. What services are created during federation init?
2. How is the gossip send callback configured?
3. What happens if federation is disabled in config?

### Checkpoint
- [ ] You understand federation initialization order
- [ ] You found the config check for enabling federation

## Part 7: Attestation Store

### Steps
1. Open `icn/crates/icn-federation/src/attestation_store.rs`
2. Find the attestation storage methods
3. Understand how attestations are verified

### Questions to answer
1. What data is stored in an attestation?
2. How is the attestation signature verified?
3. What's the difference between attestations and trust edges?

### Checkpoint
- [ ] You can explain attestation structure
- [ ] You understand attestation vs trust edge distinction

## Summary
After completing this workshop you should be able to:
- Navigate the federation crate structure
- Trace agreement lifecycle from draft to termination
- Calculate bilateral net positions
- Understand federation gossip message routing
- Explain attestation storage and verification

## Troubleshooting

### "No federation config found"
Federation must be enabled in the node config. Check `FederationConfig` in
the configuration file.

### "Unknown cooperative"
The cooperative must be in the registry before creating agreements. Verify
the announcement was received.

## Next steps
You've completed the advanced federation module! Consider:
- Implementing a custom agreement type
- Exploring multilateral netting scenarios
- Contributing to federation test coverage
