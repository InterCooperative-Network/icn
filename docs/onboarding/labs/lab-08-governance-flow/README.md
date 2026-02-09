# Lab 08: Governance Flow — Proposal to Constraint Update

## Goal
Implement the complete governance flow: proposal → vote → activation → oracle constraint update.

## Requirements

### 1. Proposal Lifecycle
```rust
pub enum ProposalState {
    Draft,
    Proposed { voting_ends: u64 },
    Active,
    Terminated { reason: String },
}

pub struct Proposal {
    pub id: String,
    pub title: String,
    pub parameter_change: ParameterChange,
    pub state: ProposalState,
    pub votes: Vec<Vote>,
}

pub struct ParameterChange {
    pub name: String,
    pub old_value: f64,
    pub new_value: f64,
}
```

### 2. Voting
```rust
pub struct Vote {
    pub voter: String,
    pub choice: VoteChoice,
    pub weight: f64,  // Could be based on stake, reputation, etc.
}

pub enum VoteChoice {
    Approve,
    Reject,
    Abstain,
}

impl Proposal {
    pub fn tally_votes(&self) -> VoteTally {
        let mut approve = 0.0;
        let mut reject = 0.0;
        
        for vote in &self.votes {
            match vote.choice {
                VoteChoice::Approve => approve += vote.weight,
                VoteChoice::Reject => reject += vote.weight,
                VoteChoice::Abstain => {}
            }
        }
        
        VoteTally { approve, reject }
    }
    
    pub fn check_outcome(&self, threshold: f64) -> Option<Outcome> {
        let tally = self.tally_votes();
        let total = tally.approve + tally.reject;
        
        if total == 0.0 {
            return None;
        }
        
        if tally.approve / total >= threshold {
            Some(Outcome::Passed)
        } else {
            Some(Outcome::Failed)
        }
    }
}
```

### 3. Parameter Store
```rust
pub struct ParameterStore {
    params: HashMap<String, f64>,
}

impl ParameterStore {
    pub fn apply_proposal(&mut self, proposal: &Proposal) {
        if matches!(proposal.state, ProposalState::Active) {
            self.params.insert(
                proposal.parameter_change.name.clone(),
                proposal.parameter_change.new_value,
            );
        }
    }
}
```

### 4. Oracle Integration
```rust
pub struct GovernanceOracle {
    params: Arc<RwLock<ParameterStore>>,
}

impl PolicyOracle for GovernanceOracle {
    fn evaluate(&self, _user: &str) -> ConstraintSet {
        let params = self.params.read().unwrap();
        let max_size = params.get("max_proposal_size").unwrap_or(&10000.0);
        
        ConstraintSet {
            quota: Some(*max_size as u64),
            ..Default::default()
        }
    }
}
```

## Tests to Write

### Test 1: Proposal Lifecycle
```rust
#[test]
fn test_proposal_lifecycle() {
    let mut proposal = Proposal::new("prop-1", "Increase max size");
    
    assert!(matches!(proposal.state, ProposalState::Draft));
    
    proposal.submit();
    assert!(matches!(proposal.state, ProposalState::Proposed { .. }));
    
    proposal.activate();
    assert!(matches!(proposal.state, ProposalState::Active));
}
```

### Test 2: Vote Tallying
```rust
#[test]
fn test_vote_tallying() {
    let mut proposal = Proposal::new("prop-1", "Test");
    
    proposal.cast_vote(Vote {
        voter: "alice".into(),
        choice: VoteChoice::Approve,
        weight: 1.0,
    });
    
    proposal.cast_vote(Vote {
        voter: "bob".into(),
        choice: VoteChoice::Reject,
        weight: 1.0,
    });
    
    proposal.cast_vote(Vote {
        voter: "carol".into(),
        choice: VoteChoice::Approve,
        weight: 1.0,
    });
    
    let tally = proposal.tally_votes();
    assert_eq!(tally.approve, 2.0);
    assert_eq!(tally.reject, 1.0);
    
    let outcome = proposal.check_outcome(0.5);
    assert_eq!(outcome, Some(Outcome::Passed));
}
```

### Test 3: Parameter Update
```rust
#[test]
fn test_parameter_update() {
    let mut store = ParameterStore::new();
    store.set("max_size", 10000.0);
    
    let mut proposal = Proposal::new("prop-1", "Increase max size");
    proposal.parameter_change = ParameterChange {
        name: "max_size".into(),
        old_value: 10000.0,
        new_value: 20000.0,
    };
    
    proposal.submit();
    proposal.cast_vote(Vote {
        voter: "alice".into(),
        choice: VoteChoice::Approve,
        weight: 1.0,
    });
    
    let outcome = proposal.check_outcome(0.5).unwrap();
    assert_eq!(outcome, Outcome::Passed);
    
    proposal.activate();
    store.apply_proposal(&proposal);
    
    assert_eq!(store.get("max_size"), Some(20000.0));
}
```

### Test 4: Oracle Constraint Update
```rust
#[test]
fn test_oracle_constraint_update() {
    let params = Arc::new(RwLock::new(ParameterStore::new()));
    params.write().unwrap().set("max_proposal_size", 10000.0);
    
    let oracle = GovernanceOracle::new(params.clone());
    
    let constraints_before = oracle.evaluate("user1");
    assert_eq!(constraints_before.quota, Some(10000));
    
    // Governance proposal passes, changes parameter
    params.write().unwrap().set("max_proposal_size", 20000.0);
    
    let constraints_after = oracle.evaluate("user1");
    assert_eq!(constraints_after.quota, Some(20000));
    
    // Kernel enforcement changes WITHOUT kernel code changes
}
```

## Done When
- Proposal lifecycle transitions correctly
- Votes are tallied with weights
- Parameters update when proposal activates
- Oracle reads new parameters
- Kernel enforcement changes without kernel modification
- All tests pass

## Why This Matters
This lab proves cooperative self-governance: communities change rules via democratic votes, and the system enforces new rules automatically via the Meaning Firewall.

## Resources
- `icn-governance/src/proposal.rs`
- `icn-governance/src/params.rs`
- `apps/governance/src/oracle.rs`
