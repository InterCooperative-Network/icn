# Why ICN?

**Infrastructure for the Cooperative Economy**

---

## The Problem

Cooperatives run on spreadsheets, emails, and trust held in people's heads. When those people leave, institutional memory walks out the door. When cooperatives want to work together, there's no shared infrastructure.

**Common pain points:**
- Member records scattered across tools
- Time banking tracked in error-prone spreadsheets
- No way to verify reputation across organizations
- Governance decisions lost in email threads
- No interoperability between cooperative systems

---

## What ICN Is

ICN is a **peer-to-peer coordination substrate** for cooperatives. Think of it as shared infrastructure that cooperatives own together.

| Layer | What It Does |
|-------|--------------|
| **Identity** | Self-sovereign IDs that work everywhere |
| **Trust** | Reputation that travels with you |
| **Ledger** | Mutual credit without banks |
| **Governance** | Democratic decisions with an audit trail |
| **Contracts** | Programmable agreements between members |

> *"The goal isn't finance. The goal is cooperation you can audit."*

---

## How It's Different

| Aspect | Spreadsheets | Blockchain | ICN |
|--------|--------------|------------|-----|
| **Control** | Whoever has the file | Token holders | The cooperative |
| **Trust model** | Implicit | Assumes hostility | Assumes relationship |
| **Interoperability** | Copy/paste | Token bridges | Built-in federation |
| **Cost** | Free but fragile | Gas fees | Contribution-based |
| **Governance** | Informal | Token-weighted | Democratic |

ICN is **not** a blockchain. It's infrastructure for people who already trust each other to coordinate better.

---

## What You Get

### For Members
- **One identity** across all cooperatives you belong to
- **Portable reputation** that follows you
- **Clear records** of contributions and balances
- **Real participation** in governance decisions

### For Cooperatives
- **Member management** with cryptographic identity
- **Mutual credit ledger** for internal economy
- **Governance tools** with configurable decision rules
- **Federation** with other cooperatives

### For Federations
- **Trust bridging** across organizations
- **Credit exchange** between currencies
- **Shared governance** for inter-coop decisions
- **Network effects** without platform lock-in

---

## Who It's For

**Ideal first adopters:**
- Timebanks (hour-based mutual credit)
- Worker cooperatives (member coordination)
- Food cooperatives (buying clubs, CSAs)
- Housing cooperatives (resident governance)
- Mutual aid networks (resource sharing)
- Credit unions (member services)

**Not for:**
- Speculative trading
- Anonymous transactions
- Replacing government currency
- Organizations that don't want transparency

---

## Getting Started

**For individuals:**
```bash
icnctl id init          # Create your identity
icnctl id show          # See your DID
```

**For cooperatives:**
```bash
icnctl gov domain create --domain-id coop:my-coop --name "My Cooperative"
icnctl ledger currency create --currency hours
```

**For developers:**
```typescript
import { ICNClient } from '@icn/client';
const client = new ICNClient({ baseUrl: 'http://localhost:8080' });
```

---

## The Vision

A world where:
- **Cooperative identity** is portable and self-sovereign
- **Reputation** is earned through participation, not purchased
- **Economic coordination** happens without intermediaries
- **Governance** is transparent and auditable
- **Cooperation** scales beyond personal networks

ICN is the infrastructure layer that makes this possible.

---

## Learn More

| Resource | Link |
|----------|------|
| **Documentation** | `docs/` directory |
| **Quick Start** | `docs/GETTING_STARTED.md` |
| **Full Manual** | `docs/manual/ICN_USER_MANUAL.md` |
| **Architecture** | [ARCHITECTURE.md](../../ARCHITECTURE.md) |
| **GitHub** | github.com/InterCooperative-Network/icn |

---

**ICN: Cooperation you can audit.**

*Open source (AGPL-3.0) | Pilot ready | December 2025*
