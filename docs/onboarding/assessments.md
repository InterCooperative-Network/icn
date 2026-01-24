# Assessments

Each module has a short check for understanding. These are designed to be quick
and practical. Use them as self-checks or as review prompts.

## Module 0
- Explain the difference between `icnd` and `icnctl`.
- Identify the config precedence order for ICN.

## Module 1
- Explain ownership and borrowing with a small Rust example.
- Write a Rust function that returns a `Result` and uses `?`.
- Explain when to use `Arc<Mutex<T>>` vs `RwLock<T>`.

## Module 2
- Draw the ICN layer stack from memory.
- Explain how trust and identity relate to ledger authorship.

## Module 3
- Trace the `icnd` startup path through `Runtime` and `Supervisor`.
- Describe why the Supervisor owns long-lived handles.

## Module 4
- Explain the DID format used by ICN.
- Describe how key rotation is represented in ICN.

## Module 5
- Explain the difference between transport and gossip in ICN.
- Describe how topic subscriptions affect routing.

## Module 6
- Explain how a mutual credit entry is represented in the ledger.
- Describe how contracts interact with ledger operations.

## Module 7
- Describe the gateway auth flow end-to-end.
- List three common scopes and what they allow.

## Module 8
- Describe how the Pilot UI obtains data from the gateway.
- Identify where token handling occurs in the UI.

## Module 9
- List three production hardening measures from ICN docs.
- Explain how to validate config before startup.

## Module 10
- Describe the test strategy and when to run which tests.
- Outline the PR checklist for contributors.

## Module 11 (Advanced)
- Explain the agreement lifecycle states and transitions.
- Describe how bilateral clearing reduces settlement transactions.
- List three types of inter-cooperative agreements and their purposes.

## Module 12
- Explain the difference between metrics, tracing, and logging in ICN.
- Identify where metrics are registered and how they are named.
- Describe how observability helps diagnose gossip or ledger issues.

## Module 13
- Explain ICN's three-layer security model.
- Describe how signed envelopes and replay guards work together.
- Explain how trust classes affect rate limiting or access control.

## Module 14
- Describe the governance lifecycle from proposal to activation.
- Explain how CCL capabilities limit contract behavior.
- Identify where governance decisions are enforced in the system.
