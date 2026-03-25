---
description: Audit a code path, crate, or flow for trust boundary violations, regulatory terminology, and economic invariant risks
allowed-tools: Read, Grep, Glob, Bash(cargo:*, rg:*)
---

Perform a focused security, trust, and regulatory audit on the code path or crate named by the user.

**Input:** A crate name, file path, function name, or flow description (e.g. "icn-ledger settlement flow", "governance proposal handler", "CCL execution path").

**Audit Layer 1: Regulatory Terminology**

Scan for prohibited terms that create regulatory exposure:
```
rg -n "payment|currency|wallet|token|blockchain|transaction fee" <target>
```

For each match:
- Flag the location
- State which regulation it could implicate
- Provide the correct ICN terminology replacement
- Note if it's in a doc-comment (public API) vs internal code (lower risk)

**Audit Layer 2: Trust Boundary Violations**

Check every point where an external input (network message, user request, CCL contract) crosses into internal state:

- Is the message signature verified before acting on it?
- Is the sender's trust score checked before granting access?
- Is rate limiting applied at the boundary?
- Are there any paths that skip identity verification?
- Does any code trust a `node_id` or `did` that arrived over the network without verifying it?

**Audit Layer 3: Economic Invariants**

For any code touching ledger, CCL, or treasury:

- Can a settlement bypass credit limit checks?
- Can a JournalEntry be written without provenance?
- Can concurrent operations create a double-spend?
- Can a CCL contract leave partial state on failure?
- Are all arithmetic operations checked for overflow?

**Audit Layer 4: Privilege Escalation**

- Does any handler grant permissions based on self-reported claims?
- Can a cooperative grant itself resources without governance approval?
- Can a member bypass quorum requirements?
- Are there any admin-only operations accessible from the public API?

**Audit Layer 5: Denial of Service**

- Are there unbounded loops or allocations in message handlers?
- Can a malicious message cause the handler to time out?
- Is fuel metering applied to CCL execution?
- Are there any `unwrap()` or `expect()` calls in hot paths that could panic the daemon?

**Output format:**
```
## Audit Report: <target>

### Regulatory Terminology
| Location | Term | Risk | Fix |
|----------|------|------|-----|
...

### Trust Boundary Issues
- CRITICAL: ...
- WARNING: ...

### Economic Invariant Risks
- ...

### Privilege Issues
- ...

### DoS Risks
- ...

### Clean (no issues found)
- ...

### Recommended Actions (priority order)
1. ...
```

**Severity levels:**
- CRITICAL: immediate action required (invariant violation, auth bypass, live regulatory risk)
- WARNING: should fix before next release
- INFO: consider addressing, low urgency
