# Curriculum Gap Analysis and Iteration Plan

## Gap analysis checklist
- Core runtime: `icnd` -> `Runtime` -> `Supervisor`
- Identity and trust: DID, keystore, trust graph, policy usage
- Network and gossip: transport, routing, topic subscriptions
- Ledger and contracts: mutual credit and CCL execution
- Gateway and SDK: auth, scopes, client usage
- Web UI: pilot workflow and token handling
- Ops: configuration, observability, deployment
- Contributor workflow: tests, linting, CI expectations

## Known gaps to validate
- Specific ledger data model types and storage layout
- Error handling patterns across actors
- Testing conventions for each crate
- Security model details beyond the architecture doc

## Iteration plan
1. After each cohort, collect feedback by module.
2. Add a short troubleshooting section per module.
3. Add diagrams for the top 3 confusing flows:
   - Auth challenge/verify
   - Gossip propagation
   - Ledger transaction lifecycle
4. Review docs quarterly to align with code changes.
