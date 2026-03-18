# ICN Governance Contract Templates

This directory contains CCL (Cooperative Contract Language) templates for common governance patterns used by cooperatives. Each template can be customized for your community's specific needs.

## Available Templates

### 1. Consensus with Fallback (`consensus-with-fallback-v1.ccl.json`)

**Best for**: Communities that value consensus but need a practical fallback when full agreement isn't possible.

**How it works**:
1. Proposals start in consensus period (default: 7 days)
2. If no objections and quorum met → **approved by consensus**
3. If objections exist after consensus period → falls back to supermajority vote (default: 67%)

**Key Rules**:
- `check_consensus` - Test if consensus is achieved
- `check_fallback_majority` - Test if supermajority threshold met
- `evaluate_proposal` - Full evaluation logic with automatic phase detection

**Configurable Parameters**:
| Parameter | Default | Range | Description |
|-----------|---------|-------|-------------|
| `consensus_period_hours` | 168 (7 days) | 24-720 | Time to seek consensus |
| `fallback_threshold_pct` | 67% | 51-100 | Supermajority threshold |
| `quorum_pct` | 50% | 10-100 | Minimum participation |
| `min_discussion_hours` | 24 | - | Minimum before evaluation |

---

### 2. Sociocracy Consent (`sociocracy-consent-v1.ccl.json`)

**Best for**: Communities using sociocratic principles where proposals pass unless someone has a principled objection.

**How it works**:
1. Proposals are shared with all members
2. Members respond: consent, objection (with reason), or abstain
3. If no objections → **approved by consent**
4. If objections exist → deliberation round to address concerns
5. Maximum objection rounds (default: 3) before blocking

**Key Rules**:
- `check_consent` - Test if consent achieved (no objections)
- `evaluate_proposal` - Full evaluation with objection round tracking

**Configurable Parameters**:
| Parameter | Default | Range | Description |
|-----------|---------|-------|-------------|
| `proposal_period_hours` | 168 (7 days) | 24-720 | Response collection period |
| `objection_resolution_hours` | 72 | - | Time per deliberation round |
| `min_acknowledgment_pct` | 50% | 10-100 | Minimum response rate |
| `max_objection_rounds` | 3 | - | Max deliberation rounds |

---

### 3. Council Delegation (`council-delegation-v1.ccl.json`)

**Best for**: Larger cooperatives where day-to-day decisions are delegated to an elected council, with member recall rights.

**How it works**:
1. Members elect a council (default: 5 seats)
2. Council makes decisions by internal vote (default: 60% approval)
3. Members can initiate recall votes (67% threshold)
4. Members can veto council decisions (75% threshold)

**Key Rules**:
- `check_election_result` - Validate election quorum
- `check_council_decision` - Council internal vote evaluation
- `check_recall_vote` - Member-initiated recall of council member
- `check_member_veto` - Full membership override of council decision

**Configurable Parameters**:
| Parameter | Default | Range | Description |
|-----------|---------|-------|-------------|
| `council_size` | 5 | 3-21 | Number of council seats |
| `council_term_days` | 365 | 90-730 | Term length |
| `election_quorum_pct` | 50% | - | Election participation threshold |
| `council_approval_threshold` | 60% | - | Council internal approval |
| `recall_threshold_pct` | 67% | - | Votes to recall member |
| `member_veto_threshold_pct` | 75% | - | Votes to veto council |

---

### 4. Emergency Lock (`emergency-lock-v1.ccl.json`)

**Best for**: Situations requiring immediate action with post-hoc ratification (security incidents, urgent operational decisions).

**How it works**:
1. Designated initiators (default: 2 members) can trigger emergency action
2. Emergency action takes effect immediately
3. Ratification vote runs in parallel (default: 48 hours)
4. If ratified → action continues
5. If not ratified or overridden (75%) → action reversed

**Key Rules**:
- `check_can_initiate_emergency` - Validate initiation requirements
- `check_ratification` - Test if emergency is ratified
- `check_override` - Test if members override emergency
- `evaluate_emergency` - Full lifecycle evaluation

**Configurable Parameters**:
| Parameter | Default | Range | Description |
|-----------|---------|-------|-------------|
| `emergency_initiator_count` | 2 | 1-10 | Members needed to trigger |
| `ratification_hours` | 48 | 12-168 | Time for ratification vote |
| `ratification_threshold_pct` | 60% | - | Approval to ratify |
| `override_threshold_pct` | 75% | - | Votes to cancel emergency |
| `cooldown_hours` | 168 (7 days) | - | Time between emergencies |
| `max_active_emergencies` | 3 | - | Concurrent emergency limit |

---

## Using Templates

### Loading a Template

```rust
use icn_ccl::Contract;
use std::fs;

// Load the JSON template
let template_json = fs::read_to_string("contracts/governance/consensus-with-fallback-v1.ccl.json")?;
let contract: Contract = serde_json::from_str(&template_json)?;

// Validate the contract
contract.validate()?;
```

### Customizing Parameters

Parameters are stored as state variables. Update them via the contract's update rules:

```rust
// Example: Change consensus period to 5 days (120 hours)
runtime.execute_rule(
    &contract_hash,
    "update_consensus_period",
    context,
    args! { "new_hours" => 120 },
).await?;
```

### Adding Participants

Each contract must have participants set before use:

```rust
let contract = contract
    .add_participant(alice.clone())
    .add_participant(bob.clone())
    .add_participant(charlie.clone());
```

---

## Template Selection Guide

| Community Type | Recommended Template | Why |
|----------------|---------------------|-----|
| Small coop (3-10 members) | Consensus with Fallback | Full consensus achievable, fallback for edge cases |
| Medium coop (10-50 members) | Sociocracy Consent | Efficient consent process, handles objections gracefully |
| Large coop (50+ members) | Council Delegation | Scales with delegation, maintains member control |
| Any coop (emergencies) | Emergency Lock | Add alongside primary template for crisis response |

---

## Creating Custom Templates

1. Start from an existing template
2. Modify state variables for your defaults
3. Add or modify rules as needed
4. Test with `Contract::validate()`
5. Deploy via ContractRuntime

See `icn-ccl/examples/timebank.rs` for a complete example.

---

## Version History

- **v1** (2025-12-06): Initial release with 4 governance templates
