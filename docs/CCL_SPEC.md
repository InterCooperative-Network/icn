# Cooperative Consensus Language (CCL) Specification

## Introduction

This document specifies the Cooperative Consensus Language (CCL), a domain-specific language designed for the Intercooperative Network (ICN) to express federation governance rules, smart contracts, and cross-federation agreements. CCL provides a formal, auditable way to define the behavior of autonomous federation processes.

> **Related Documentation:**
> - [ARCHITECTURE.md](ARCHITECTURE.md) - Overall system architecture
> - [GOVERNANCE_SYSTEM.md](GOVERNANCE_SYSTEM.md) - Governance framework
> - [SECURITY.md](SECURITY.md) - Security model

## Language Design Principles

CCL adheres to the following design principles:

1. **Safety-First**: Language constructs prevent common vulnerabilities
2. **Auditability**: Code is human-readable and formally verifiable
3. **Federation-Centric**: First-class support for federation operations
4. **Deterministic Execution**: Same inputs always produce same outputs
5. **Resource-Bounded**: Explicit resource limits and cost model
6. **Composability**: Rules and contracts can be combined and extended

## Language Syntax & Semantics

### Lexical Structure

CCL uses a clean, minimal syntax influenced by Rust and Python:

```rust
// This is a single-line comment

/* This is a
   multi-line comment */

// Federation declaration
federation Coop1 {
    // Federation properties
    name: "Agricultural Cooperative",
    threshold: 2/3,
    guardian_set: ["guardian1", "guardian2", "guardian3"],

    // Rules
    rule transfer_rule {
        when Transfer {
            // Rule logic
            if amount > 1000 {
                require signatures >= 2;
            }
        }
    }
}
```

### Type System

CCL employs a strong, static type system with the following primitive types:

```rust
// Primitive types
let boolean_value: Bool = true;
let integer_value: Int = 42;
let decimal_value: Decimal = 3.14159;
let text_value: String = "Hello, Federation";
let timestamp: DateTime = @2023-11-15T14:30:00Z;
let duration_value: Duration = 30d; // 30 days
let address_value: Address = @coop1:user:alice;
let signature_value: Signature = sig"0x...";

// Container types
let array_value: Array<Int> = [1, 2, 3, 4, 5];
let map_value: Map<String, Int> = {"one": 1, "two": 2};
let set_value: Set<String> = {"apple", "banana", "cherry"};
let optional_value: Option<Int> = Some(42);
let result_value: Result<Int, String> = Ok(42);

// Domain-specific types
let asset: Asset = 100 ICN;
let vote: Vote = vote(proposal_id: "prop1", choice: "approve");
let threshold: Threshold = 2/3;
```

### Functions and Expressions

CCL supports pure functions and expressions:

```rust
// Function declaration
fn calculate_fee(amount: Asset, user_tier: String) -> Asset {
    match user_tier {
        "premium" => amount * 0.01,
        "standard" => amount * 0.02,
        _ => amount * 0.03
    }
}

// Expression examples
let discount = if total > 1000 { 0.1 } else { 0.05 };
let tax = match location {
    "US" => 0.07,
    "EU" => 0.20,
    _ => 0.00
};
```

### State and Mutations

CCL provides controlled state mutation:

```rust
// State variable declaration
state total_supply: Asset = 1000000 ICN;
state allowances: Map<Address, Map<Address, Asset>> = {};

// Mutation through actions
action transfer(to: Address, amount: Asset) {
    // Preconditions
    require(amount > 0, "Amount must be positive");
    require(balances[msg.sender] >= amount, "Insufficient balance");
    
    // State mutations
    balances[msg.sender] -= amount;
    balances[to] += amount;
    
    // Emit event
    emit Transfer(from: msg.sender, to: to, amount: amount);
}
```

## Execution Model

### Transaction Lifecycle

CCL transactions follow a defined lifecycle:

```
┌─────────────────────────────────────────────────────────┐
│                Transaction Lifecycle                    │
├─────────────────────────────────────────────────────────┤
│ 1. Transaction Submission                              │
│ 2. Validation                                          │
│    - Syntax checking                                   │
│    - Type checking                                     │
│    - Signature verification                            │
│ 3. Rule Evaluation                                     │
│    - Precondition checking                             │
│    - Resource allocation                               │
│ 4. Execution                                           │
│    - State transitions                                 │
│    - Event emission                                    │
│ 5. Commitment                                          │
│    - State finalization                                │
│    - Receipt generation                                │
└─────────────────────────────────────────────────────────┘
```

### Concurrency Model

CCL employs an optimistic concurrency model:

```rust
// Transaction dependencies
transaction {
    // Explicit read dependencies
    reads balances[sender], allowances[owner][spender];
    
    // Explicit write dependencies
    writes balances[sender], balances[receiver];
    
    // Transaction logic
    action transfer_from(owner: Address, spender: Address, amount: Asset) {
        // Implementation
    }
}
```

### Resource Limits

Explicit resource constraints prevent excessive resource usage:

```rust
// Resource limits
limits {
    // Computation limit
    compute_units: 1000,
    
    // Memory limit
    memory_kb: 128,
    
    // State access limit
    state_access_count: 50,
    
    // External call limit
    external_calls: 5
}
```

## Smart Contracts

### Contract Definition

CCL contracts encapsulate state and behavior:

```rust
// Contract definition
contract Token {
    // State variables
    state balances: Map<Address, Asset> = {};
    state total_supply: Asset = 0 ICN;
    
    // Initialization
    constructor(initial_supply: Asset) {
        total_supply = initial_supply;
        balances[msg.sender] = initial_supply;
    }
    
    // Actions (mutating functions)
    action transfer(to: Address, amount: Asset) {
        require(balances[msg.sender] >= amount, "Insufficient balance");
        
        balances[msg.sender] -= amount;
        balances[to] += amount;
        
        emit Transfer(from: msg.sender, to: to, amount: amount);
    }
    
    // Views (non-mutating functions)
    view balance_of(owner: Address) -> Asset {
        return balances[owner];
    }
    
    // Events
    event Transfer(from: Address, to: Address, amount: Asset);
}
```

### Contract Deployment and Interaction

```rust
// Deploy a contract
deploy Token(initial_supply: 1000000 ICN) as token_instance;

// Interact with a contract
let my_balance = token_instance.balance_of(my_address);
token_instance.transfer(recipient, 100 ICN);
```

## Governance Rules

### Rule Definition

Rules define governance policies:

```rust
// Rule definition
rule large_transfer_approval {
    // When this event occurs
    when Transfer(amount) {
        // Apply this condition
        if amount > 10000 ICN {
            // Require this action
            require approval_from(federation.guardians, threshold: 2/3);
        }
    }
}

// Policy set
policy transfer_policies {
    include large_transfer_approval;
    include anti_money_laundering;
    include rate_limiting;
}
```

### Voting and Consensus

CCL provides first-class voting constructs:

```rust
// Proposal creation
proposal update_fee_structure {
    title: "Update Fee Structure",
    description: "Adjust transaction fees based on network usage",
    
    // Changes to apply if approved
    changes {
        fee_percentage = 0.015;
        fee_cap = 100 ICN;
    }
    
    // Voting period
    voting_period: 7d,
    
    // Approval threshold
    threshold: 2/3,
    
    // Execution delay after approval
    execution_delay: 2d
}

// Casting votes
vote(proposal_id: "update_fee_structure", choice: "approve");
vote(proposal_id: "update_fee_structure", choice: "reject", reason: "Too expensive");
```

## Cross-Federation Interactions

### Federation Bridges

CCL specifies cross-federation interactions:

```rust
// Define a bridge between federations
bridge Coop1_to_Coop2 {
    source: Federation("Coop1"),
    target: Federation("Coop2"),
    
    // Asset mapping
    asset_mapping {
        "Coop1:ICN" => "Coop2:ICN" at rate(1:1),
        "Coop1:USD" => "Coop2:USD" at rate(1:1)
    },
    
    // Required attestations
    attestations: [
        source.guardians(threshold: 2/3),
        target.guardians(threshold: 2/3)
    ],
    
    // Validation rules
    validation_rules: [
        validate_source_chain_state,
        validate_transfer_limits
    ]
}

// Cross-federation transfer
action cross_federation_transfer(
    to: Address("Coop2:user:bob"),
    amount: 100 ICN,
    bridge: Coop1_to_Coop2
) {
    // Implementation
}
```

### Federation Agreements

CCL can encode formal agreements between federations:

```rust
// Federation agreement
agreement trading_partnership {
    parties: [
        Federation("Coop1"),
        Federation("Coop2")
    ],
    
    // Agreement terms
    terms {
        trading_fee: 0.5%,
        dispute_resolution: "arbitration",
        termination_notice: 30d
    },
    
    // Required signatures
    signatures: [
        require parties[0].guardians(threshold: 3/4),
        require parties[1].guardians(threshold: 3/4)
    ],
    
    // Duration
    effective_period: 365d,
    
    // Renewal terms
    renewal: automatic if !opt_out(notice_period: 30d)
}
```

## Security Features

### Formal Verification

CCL supports formal verification:

```rust
// Property specification
property no_double_spend {
    forall tx1, tx2 in transactions:
        tx1 ≠ tx2 ∧ 
        tx1.transfers(from: addr, amount: a1) ∧ 
        tx2.transfers(from: addr, amount: a2) →
        balance(addr) ≥ a1 + a2
}

// Invariant specification
invariant total_supply_constant {
    sum(all_balances) == INITIAL_SUPPLY
}

// Verification directive
verify contract Token satisfies [
    no_double_spend,
    total_supply_constant
];
```

### Type-Level Protections

Type-level protections prevent common vulnerabilities:

```rust
// Protected asset type prevents accidental misuse
asset ICN {
    decimals: 8,
    total_supply: 10_000_000
}

// Protected time values
let lock_period: SecureTimelock = 30d;

// Protected address type
address owner: GuardedAddress = federation.treasury;
```

### Authorization Control

Fine-grained authorization rules:

```rust
// Role-based access control
role Administrator {
    permissions: [
        UpdateSystemParameters,
        EmergencyPause,
        AddGuardian
    ]
}

// Permission checking
action update_fee(new_fee: Decimal) {
    require_permission(msg.sender, UpdateSystemParameters);
    fee_percentage = new_fee;
}

// Multi-signature requirement
action withdraw_reserve(amount: Asset) {
    require_multi_sig(federation.guardians, threshold: 3/4);
    transfer(federation.treasury, amount);
}
```

## Integration Examples

### Token Contract Example

```rust
contract CooperativeToken {
    // State
    state balances: Map<Address, Asset> = {};
    state total_supply: Asset = 0 ICN;
    state federation_id: String;
    
    // Initialization
    constructor(federation: String, initial_supply: Asset) {
        federation_id = federation;
        total_supply = initial_supply;
        balances[federation_treasury(federation)] = initial_supply;
    }
    
    // Transfer tokens
    action transfer(to: Address, amount: Asset) {
        // Validation
        require(amount > 0 ICN, "Amount must be positive");
        require(balances[msg.sender] >= amount, "Insufficient balance");
        
        // Check federation rules
        check_federation_rules(federation_id, "transfer", msg.sender, to, amount);
        
        // Execute transfer
        balances[msg.sender] -= amount;
        balances[to] += amount;
        
        // Emit event
        emit Transfer(from: msg.sender, to: to, amount: amount);
    }
    
    // Check balance
    view balance_of(owner: Address) -> Asset {
        return balances[owner] ?? 0 ICN;
    }
    
    // Federation-specific mint
    action federation_mint(to: Address, amount: Asset) {
        // Only federation guardians can mint
        require_federation_guardians(federation_id, threshold: 2/3);
        
        // Update state
        total_supply += amount;
        balances[to] += amount;
        
        // Emit event
        emit Mint(to: to, amount: amount);
    }
    
    // Events
    event Transfer(from: Address, to: Address, amount: Asset);
    event Mint(to: Address, amount: Asset);
}
```

### Governance Proposal Example

```rust
// Define proposal types
enum ProposalType {
    ParameterChange,
    FederationJoin,
    FederationLeave,
    ContractUpgrade,
    EmergencyAction
}

// Define a governance contract
contract FederationGovernance {
    // State
    state proposals: Map<ProposalId, Proposal> = {};
    state votes: Map<ProposalId, Map<Address, Vote>> = {};
    state parameters: GovernanceParameters;
    state federation_id: String;
    
    // Initialization
    constructor(federation: String, initial_parameters: GovernanceParameters) {
        federation_id = federation;
        parameters = initial_parameters;
    }
    
    // Create a proposal
    action create_proposal(
        title: String,
        description: String,
        proposal_type: ProposalType,
        changes: ProposalChanges,
        voting_period: Duration
    ) -> ProposalId {
        // Ensure creator has sufficient stake
        require(
            token.balance_of(msg.sender) >= parameters.proposal_threshold,
            "Insufficient stake to create proposal"
        );
        
        // Generate proposal ID
        let proposal_id = generate_proposal_id(msg.sender, title, block.timestamp);
        
        // Create proposal object
        let new_proposal = Proposal {
            id: proposal_id,
            creator: msg.sender,
            title: title,
            description: description,
            proposal_type: proposal_type,
            changes: changes,
            status: ProposalStatus.Active,
            created_at: block.timestamp,
            voting_ends_at: block.timestamp + voting_period,
            votes_for: 0,
            votes_against: 0,
            votes_abstain: 0
        };
        
        // Store proposal
        proposals[proposal_id] = new_proposal;
        
        // Emit event
        emit ProposalCreated(
            proposal_id: proposal_id,
            creator: msg.sender,
            proposal_type: proposal_type
        );
        
        return proposal_id;
    }
    
    // Cast a vote
    action vote(proposal_id: ProposalId, choice: VoteChoice, reason: Option<String>) {
        // Get proposal
        let proposal = proposals[proposal_id] ?? fail("Proposal not found");
        
        // Check if proposal is active
        require(proposal.status == ProposalStatus.Active, "Proposal not active");
        require(block.timestamp < proposal.voting_ends_at, "Voting period ended");
        
        // Check if already voted
        require(votes[proposal_id][msg.sender] == null, "Already voted");
        
        // Get voting power
        let voting_power = token.balance_of(msg.sender);
        require(voting_power > 0, "No voting power");
        
        // Record vote
        votes[proposal_id][msg.sender] = Vote {
            voter: msg.sender,
            choice: choice,
            voting_power: voting_power,
            timestamp: block.timestamp,
            reason: reason
        };
        
        // Update vote tallies
        match choice {
            VoteChoice.For => proposal.votes_for += voting_power,
            VoteChoice.Against => proposal.votes_against += voting_power,
            VoteChoice.Abstain => proposal.votes_abstain += voting_power
        }
        
        // Emit event
        emit VoteCast(
            proposal_id: proposal_id,
            voter: msg.sender,
            choice: choice,
            voting_power: voting_power
        );
    }
    
    // Execute a proposal
    action execute_proposal(proposal_id: ProposalId) {
        // Get proposal
        let proposal = proposals[proposal_id] ?? fail("Proposal not found");
        
        // Check if proposal is active and voting period ended
        require(proposal.status == ProposalStatus.Active, "Proposal not active");
        require(block.timestamp >= proposal.voting_ends_at, "Voting period not ended");
        
        // Calculate total votes
        let total_votes = proposal.votes_for + proposal.votes_against + proposal.votes_abstain;
        
        // Check quorum
        require(
            total_votes >= parameters.quorum_threshold,
            "Quorum not reached"
        );
        
        // Check if proposal passed
        let approval_ratio = proposal.votes_for / (proposal.votes_for + proposal.votes_against);
        let passed = approval_ratio >= parameters.approval_threshold;
        
        if (passed) {
            // Execute proposal changes
            execute_changes(proposal.changes);
            proposal.status = ProposalStatus.Executed;
        } else {
            proposal.status = ProposalStatus.Rejected;
        }
        
        // Emit event
        emit ProposalExecuted(
            proposal_id: proposal_id,
            passed: passed,
            approval_ratio: approval_ratio
        );
    }
    
    // Events
    event ProposalCreated(proposal_id: ProposalId, creator: Address, proposal_type: ProposalType);
    event VoteCast(proposal_id: ProposalId, voter: Address, choice: VoteChoice, voting_power: Asset);
    event ProposalExecuted(proposal_id: ProposalId, passed: Bool, approval_ratio: Decimal);
}
```

## Error Handling

CCL provides rich error handling:

```rust
// Error definition
error InsufficientBalance(required: Asset, available: Asset);
error Unauthorized(address: Address, required_role: String);
error InvalidState(expected: String, actual: String);

// Fallible operations
action withdraw(amount: Asset) -> Result<TxReceipt, InsufficientBalance> {
    if balances[msg.sender] < amount {
        return Err(InsufficientBalance(
            required: amount,
            available: balances[msg.sender]
        ));
    }
    
    // Proceed with withdrawal
    balances[msg.sender] -= amount;
    return Ok(receipt());
}

// Error handling
let result = account.withdraw(100 ICN);
match result {
    Ok(receipt) => {
        // Handle success
        log("Withdrawal successful", receipt);
    },
    Err(InsufficientBalance{required, available}) => {
        // Handle error
        log("Insufficient balance", {required, available});
    }
}
```

## Interoperability

### External System Integration

CCL can interface with external systems:

```rust
// Oracle data feed
oracle price_feed {
    // Data source
    source: "https://api.pricing.example/v1/icn-usd",
    
    // Update frequency
    update_interval: 1h,
    
    // Required attestations
    attestations: federation.oracles(threshold: 3/5),
    
    // Data structure
    schema {
        price: Decimal,
        timestamp: DateTime,
        volume: Decimal
    }
}

// Use oracle data
action set_exchange_rate() {
    // Get price feed data
    let feed = price_feed.latest();
    
    // Validate freshness
    require(
        block.timestamp - feed.timestamp < 2h,
        "Price feed data too old"
    );
    
    // Update exchange rate
    exchange_rate = feed.price;
}
```

### Cross-Chain Communication

CCL supports cross-chain interactions:

```rust
// Cross-chain message
action send_cross_chain(
    target_chain: ChainId,
    recipient: Address,
    payload: Bytes,
    fee: Asset
) {
    // Verify fee
    require(msg.value >= fee, "Insufficient fee");
    
    // Create cross-chain message
    let message = CrossChainMessage {
        source_chain: this_chain_id,
        source_address: msg.sender,
        target_chain: target_chain,
        target_address: recipient,
        payload: payload,
        nonce: get_next_nonce(msg.sender, target_chain)
    };
    
    // Sign message with federation signatures
    let signatures = collect_federation_signatures(
        message,
        federation.validators,
        threshold: 2/3
    );
    
    // Emit cross-chain event
    emit CrossChainMessageSent(
        message_id: hash(message),
        message: message,
        signatures: signatures
    );
}
```

## Code Organization and Modularity

### Modules and Imports

CCL code can be organized into modules:

```rust
// File: token.ccl
module token {
    // Contract implementation
    contract Token {
        // Implementation details
    }
    
    // Exported functions
    pub fn calculate_fees(amount: Asset) -> Asset {
        // Implementation
    }
}

// File: main.ccl
import token from "token.ccl";
import governance from "governance.ccl";

// Use imported modules
deploy token.Token(initial_supply: 1000000 ICN);
```

### Interface Definitions

CCL supports interface definitions:

```rust
// Define an interface
interface IToken {
    // State properties
    readonly total_supply: Asset;
    
    // Actions
    action transfer(to: Address, amount: Asset);
    action approve(spender: Address, amount: Asset);
    action transfer_from(from: Address, to: Address, amount: Asset);
    
    // Views
    view balance_of(owner: Address) -> Asset;
    view allowance(owner: Address, spender: Address) -> Asset;
    
    // Events
    event Transfer(from: Address, to: Address, amount: Asset);
    event Approval(owner: Address, spender: Address, amount: Asset);
}

// Implement an interface
contract CoopToken implements IToken {
    // Implementation details
}
```

## Development and Testing

### Testing Framework

CCL includes a testing framework:

```rust
// Test suite
test_suite token_tests {
    // Setup for tests
    setup {
        deploy Token(initial_supply: 1000000 ICN) as token;
        create_account(alice, balance: 1000 ICN);
        create_account(bob, balance: 500 ICN);
    }
    
    // Test case
    test "transfer reduces sender balance" {
        // Initial state
        let initial_alice = token.balance_of(alice);
        let initial_bob = token.balance_of(bob);
        
        // Action
        token.transfer(from: alice, to: bob, amount: 100 ICN);
        
        // Assertions
        assert token.balance_of(alice) == initial_alice - 100 ICN;
        assert token.balance_of(bob) == initial_bob + 100 ICN;
    }
    
    // Test error conditions
    test "transfer fails with insufficient balance" {
        // Attempt transfer with insufficient funds
        let result = try_call token.transfer(from: alice, to: bob, amount: 2000 ICN);
        
        // Assertion
        assert result.is_error();
        assert result.error == InsufficientBalance;
    }
}
```

### Property-Based Testing

CCL supports property-based testing:

```rust
// Property-based test
property_test "balance sum remains constant after transfers" {
    // Set up arbitrary accounts and balances
    let accounts = generate_accounts(count: 5..10);
    let initial_balances = distribute_tokens(accounts, total: 10000 ICN);
    
    // Perform random transfers
    for 1..100 times {
        let from = choose(accounts);
        let to = choose(accounts.without(from));
        let amount = random(1 ICN..balance_of(from));
        
        token.transfer(from: from, to: to, amount: amount);
    }
    
    // Check invariant
    assert sum(balance_of(account) for account in accounts) == 10000 ICN;
}
```

## Deployment and Upgrade Model

### Deployment Configuration

```rust
// Deployment manifest
deployment token_deployment {
    // Contract to deploy
    contract: Token,
    
    // Constructor arguments
    constructor_args: {
        initial_supply: 1000000 ICN,
        federation: "Coop1"
    },
    
    // Access control
    access_control: {
        admin: federation.treasury,
        upgrade_controller: federation.governance
    },
    
    // Initial state configuration
    initial_state: {
        token_name: "Cooperative Token",
        token_symbol: "COOP",
        decimals: 8
    }
}
```

### Upgrade Mechanism

```rust
// Upgrade proposal
upgrade_proposal token_v2_upgrade {
    // Contract being upgraded
    target: token_deployment,
    
    // New implementation
    new_implementation: TokenV2,
    
    // State migration function
    migration: migrate_token_state,
    
    // Approval requirements
    approvals: [
        require federation.guardians(threshold: 3/4),
        require token.holders(threshold: 2/3, min_voting_period: 7d)
    ]
}

// State migration function
fn migrate_token_state(old_state: TokenState) -> TokenV2State {
    return TokenV2State {
        balances: old_state.balances,
        total_supply: old_state.total_supply,
        // New state fields
        token_metadata: {
            name: old_state.token_name,
            symbol: old_state.token_symbol,
            logo_url: "https://example.com/logo.png"
        },
        // Initialize new fields
        reward_distribution: new_reward_distribution()
    };
}
```

## Formal Verification Example

```rust
// Safety properties for a token contract
contract_verification Token {
    // No tokens created out of thin air
    property conservation_of_tokens {
        after transfer(from, to, amount) {
            old(balance_of(from)) + old(balance_of(to)) == 
            balance_of(from) + balance_of(to)
        }
    }
    
    // Balances are never negative
    property no_negative_balances {
        forall address: Address {
            balance_of(address) >= 0 ICN
        }
    }
    
    // Total supply is constant (except for mint/burn)
    property total_supply_invariant {
        after any action except mint, burn {
            total_supply() == old(total_supply())
        }
    }
    
    // Transfer authorization
    property proper_authorization {
        can_execute transfer(from, to, amount) only if {
            msg.sender == from ||
            allowance(from, msg.sender) >= amount
        }
    }
}
```

## Glossary

| Term | Definition |
|------|------------|
| **Action** | A function that can modify contract state. |
| **Address** | A unique identifier for an account or contract in the system. |
| **Asset** | A typed token with specific properties and behaviors. |
| **Bridge** | A mechanism for transferring assets and data between federations. |
| **Contract** | A collection of state variables and functions that encapsulate behavior. |
| **Federation** | A cooperative group operating as a trust domain within the ICN. |
| **Guardian** | A trusted entity with special permissions in a federation. |
| **Invariant** | A condition that must always hold true throughout execution. |
| **Oracle** | An external data source that provides information to on-chain contracts. |
| **Proposal** | A suggested change to be voted on by federation members. |
| **Rule** | A condition that must be satisfied for certain operations to succeed. |
| **State** | The data stored by a contract that persists between transactions. |
| **Transaction** | An atomic unit of execution that may modify state. |
| **View** | A function that reads but does not modify contract state. |
| **Vote** | An expression of preference on a governance proposal. |
</rewritten_file> 