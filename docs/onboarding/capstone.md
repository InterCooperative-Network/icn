# Capstone: Local Two-Node ICN + Ledger Flow

## Goal
Bring up a two-node ICN network locally, perform a ledger transaction through
the gateway, and observe gossip propagation.

## Prerequisites
- Completed Modules 0-9
- Built ICN binaries

## Steps
1. Build ICN: `cd icn && cargo build --release`
2. Start node alpha with `config/icn-alpha.toml`
3. Start node beta with `config/icn-beta.toml`
4. Verify peers discover each other with `icnctl network peers`
5. Enable gateway on one node (config or CLI)
6. Get a JWT token with `icnctl auth` flow
7. Create a cooperative via gateway API
8. Post a ledger payment via gateway API
9. Verify ledger state and gossip on both nodes

## Expected outputs
- Both nodes show each other as peers
- Gateway health returns OK
- Ledger balance reflects the payment
- Gossip shows the ledger update being replicated

## Rubric
- **Setup complete**: nodes running, logs clean, config validated
- **Auth working**: successful challenge and JWT token
- **API operations**: cooperative created, payment posted
- **Replication**: ledger state confirmed on both nodes
- **Observability**: metrics endpoint reachable and shows activity

## Optional extensions
- Add a governance proposal and cast votes
- Simulate a node restart and verify persistence
- Introduce a third node and observe gossip convergence
