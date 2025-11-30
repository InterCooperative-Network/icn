# ICN Threat Model

**Version**: 1.0
**Date**: 2025-11-29
**Status**: Complete
**Authors**: Security Review Team

## Executive Summary

This document provides a formal threat model for the InterCooperative Network (ICN) using the STRIDE framework. It identifies adversary classes, maps attack surfaces by system layer, analyzes threats for each component, and documents mitigations.

**Security Posture Summary**:
- **929+ tests** covering security-critical paths
- **Three-layer security architecture** (Transport, Message, Application)
- **8 critical vulnerabilities fixed** during production hardening
- **Known gaps documented** with priority rankings

---

## Table of Contents

1. [Adversary Classes](#1-adversary-classes)
2. [System Architecture Overview](#2-system-architecture-overview)
3. [Attack Surface by Layer](#3-attack-surface-by-layer)
4. [STRIDE Analysis by Component](#4-stride-analysis-by-component)
5. [Mitigations Reference](#5-mitigations-reference)
6. [Gap Analysis](#6-gap-analysis)
7. [Security Assumptions](#7-security-assumptions)
8. [References](#8-references)

---

## 1. Adversary Classes

### 1.1 Outsider (Network Attacker)

**Capabilities**:
- No ICN identity (no DID)
- Can observe network traffic
- Can inject network packets
- Can perform DoS attacks at network level
- Cannot decrypt QUIC/TLS traffic

**Goals**:
- Disrupt service availability
- Intercept communications (fails due to TLS)
- Impersonate legitimate peers (fails due to DID-TLS binding)

**Example Attacks**:
- Connection flooding
- TCP/UDP amplification
- DNS hijacking of discovery endpoints

### 1.2 Malicious Peer (Byzantine Node)

**Capabilities**:
- Valid ICN identity (DID with keypair)
- Can establish authenticated connections
- Can participate in gossip protocol
- Can submit transactions and proposals
- Subject to trust-based rate limiting

**Goals**:
- Manipulate trust graph for elevated access
- Spam gossip network
- Create ledger inconsistencies
- Influence governance outcomes
- Exhaust resources of legitimate nodes

**Example Attacks**:
- Sybil attacks (create multiple identities)
- Gossip flooding
- Vote manipulation
- Trust inflation via collusion

### 1.3 Compromised Node (Stolen Keys)

**Capabilities**:
- Full access to legitimate user's identity
- All capabilities of Malicious Peer
- Historical transaction and trust relationships
- Possibly access to multiple devices (multi-device scenario)

**Goals**:
- Drain victim's credit balance
- Vote on victim's behalf
- Damage victim's reputation
- Access victim's private communications

**Example Attacks**:
- Balance theft
- Governance takeover
- Trust relationship manipulation
- Identity destruction

### 1.4 Economic Attacker

**Capabilities**:
- One or more valid ICN identities
- Understanding of mutual credit mechanics
- Potentially multiple colluding accounts

**Goals**:
- Extract value from the credit system
- Create unbacked credit
- Default on credit obligations
- Game dynamic credit limits

**Example Attacks**:
- "Grab and run" (exhaust credit limit, leave network)
- Credit cycling (inflate limits via circular transactions)
- Social recovery abuse (fake recoveries to duplicate balances)
- Demurrage avoidance schemes

### 1.5 Governance Attacker

**Capabilities**:
- Valid ICN identity with voting rights
- Understanding of governance mechanics
- Potentially colluding voters

**Goals**:
- Pass malicious proposals
- Block legitimate proposals
- Manipulate cooperative policies
- Gain unauthorized access/roles

**Example Attacks**:
- Vote buying/coordination
- Proposal spam (resource exhaustion)
- Quorum manipulation (prevent voting)
- Emergency mechanism abuse

### 1.6 Contract Attacker

**Capabilities**:
- Ability to deploy CCL contracts
- Understanding of interpreter mechanics
- Knowledge of capability system

**Goals**:
- Execute unauthorized operations
- Exhaust compute resources (fuel bombing)
- Extract data via side channels
- Bypass capability restrictions

**Example Attacks**:
- Fuel exhaustion attacks
- Deeply nested expression stack overflow
- Capability escalation attempts
- Non-deterministic execution exploitation

---

## 2. System Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                        APPLICATION LAYER                             │
│  ┌─────────┐  ┌───────────┐  ┌──────────┐  ┌─────────┐  ┌─────────┐ │
│  │ Gateway │  │Governance │  │ Compute  │  │ Ledger  │  │   CCL   │ │
│  │  REST   │  │  Domains  │  │  Tasks   │  │ Mutual  │  │Contracts│ │
│  │   API   │  │ Proposals │  │ Priority │  │ Credit  │  │  Rules  │ │
│  └────┬────┘  └─────┬─────┘  └────┬─────┘  └────┬────┘  └────┬────┘ │
│       │             │             │             │             │      │
├───────┴─────────────┴─────────────┴─────────────┴─────────────┴──────┤
│                         MESSAGE LAYER                                │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  SignedEnvelope (Ed25519) │ ReplayGuard │ EncryptedEnvelope  │   │
│  └──────────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────────┤
│                        TRANSPORT LAYER                               │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │   QUIC/TLS 1.3  │  DID-TLS Binding  │  Certificate Verify    │   │
│  └──────────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────────┤
│                          DATA LAYER                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────────┐  │
│  │  Trust      │  │   Gossip    │  │       Sled Store            │  │
│  │  Graph      │  │   Topics    │  │   (Encrypted Keystore)      │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

### Security Boundaries

1. **Network Boundary**: QUIC/TLS separates untrusted network from authenticated peers
2. **Trust Boundary**: Trust graph determines access levels within authenticated peers
3. **Cooperative Boundary**: Per-coop isolation of ledgers and governance
4. **Capability Boundary**: CCL capabilities constrain contract operations

---

## 3. Attack Surface by Layer

### 3.1 Identity Layer

| Component | Entry Points | Attack Vectors |
|-----------|--------------|----------------|
| Keystore | File system, CLI | Key theft, passphrase brute force |
| DID | Network protocol | Format injection, malformed DIDs |
| TLS Cert | TLS handshake | Cert forgery, binding bypass |
| Multi-device | Gossip sync | Unauthorized device addition |
| Key rotation | Gossip broadcast | Chain hijacking, replay |

**Files**: `icn-identity/`, `icn-net/src/tls.rs`

### 3.2 Network Layer

| Component | Entry Points | Attack Vectors |
|-----------|--------------|----------------|
| QUIC endpoint | UDP socket | Connection flooding |
| TLS handshake | Connection establishment | Resource exhaustion |
| mDNS discovery | Local network | Spoofed announcements |
| Message framing | Stream data | Length prefix attacks |

**Files**: `icn-net/src/actor.rs`, `icn-net/src/protocol.rs`

### 3.3 Trust Layer

| Component | Entry Points | Attack Vectors |
|-----------|--------------|----------------|
| Trust edges | Gossip messages | Inflation, collusion |
| Trust computation | Query path | Loops, amplification |
| Trust thresholds | Access control | Threshold bypass |
| Decay mechanism | Time-based | Clock manipulation |

**Files**: `icn-trust/src/lib.rs`, `icn-trust/src/compute.rs`

### 3.4 Gossip Layer

| Component | Entry Points | Attack Vectors |
|-----------|--------------|----------------|
| Topic subscriptions | Protocol messages | Unauthorized access |
| Announcements | Push messages | Spam, flooding |
| Anti-entropy | Bloom filters | Index overflow |
| Vector clocks | Causal ordering | Clock manipulation |

**Files**: `icn-gossip/src/gossip.rs`, `icn-gossip/src/bloom.rs`

### 3.5 Ledger Layer

| Component | Entry Points | Attack Vectors |
|-----------|--------------|----------------|
| Transactions | Gateway API, RPC | Double-spend, forgery |
| Balances | Query API | Information disclosure |
| Credit limits | Policy engine | Limit bypass |
| Disputes | Dispute manager | False claims |
| Recovery | Social recovery | Balance duplication |

**Files**: `icn-ledger/src/ledger.rs`, `icn-ledger/src/credit_policy.rs`

### 3.6 Governance Layer

| Component | Entry Points | Attack Vectors |
|-----------|--------------|----------------|
| Domains | Gateway API | Unauthorized creation |
| Proposals | Create/open API | Spam, manipulation |
| Votes | Cast API | Vote stuffing, replay |
| Membership | Resolution | Threshold bypass |

**Files**: `icn-governance/src/`, `icn-gateway/src/api/governance.rs`

### 3.7 Compute Layer

| Component | Entry Points | Attack Vectors |
|-----------|--------------|----------------|
| Task submission | Gateway API, RPC | Fuel bomb, code injection |
| Executor selection | Scheduler | Trust threshold bypass |
| Result verification | Gossip | Fake results |
| Payment settlement | Ledger callback | Payment fraud |

**Files**: `icn-compute/src/actor.rs`, `icn-compute/src/executor.rs`

### 3.8 Gateway Layer

| Component | Entry Points | Attack Vectors |
|-----------|--------------|----------------|
| Challenge-response | Auth endpoints | Replay, timing attacks |
| JWT tokens | Bearer header | Token theft, forgery |
| Rate limiting | All endpoints | Bypass, exhaustion |
| WebSocket | Event stream | Connection exhaustion |

**Files**: `icn-gateway/src/auth.rs`, `icn-gateway/src/rate_limit.rs`

---

## 4. STRIDE Analysis by Component

### 4.1 Identity System

#### Spoofing

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| ID-S1 | Claim another user's DID | DID-TLS binding verification | ✅ Mitigated |
| ID-S2 | Generate colliding DIDs | Ed25519 256-bit key space | ✅ Mitigated |
| ID-S3 | Forge DID document | Ed25519 signature verification | ✅ Mitigated |
| ID-S4 | Impersonate via compromised device | Device revocation mechanism | ✅ Mitigated |

#### Tampering

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| ID-T1 | Modify keystore file | Age encryption (ChaCha20-Poly1305) | ✅ Mitigated |
| ID-T2 | Alter rotation chain | Dual-signature verification | ✅ Mitigated |
| ID-T3 | Modify TLS certificate | Certificate hash in DID binding | ✅ Mitigated |

#### Repudiation

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| ID-R1 | Deny signing a message | SignedEnvelope with Ed25519 | ✅ Mitigated |
| ID-R2 | Deny key rotation | Rotation chain with timestamps | ✅ Mitigated |

#### Information Disclosure

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| ID-I1 | Extract private key from keystore | Age encryption + passphrase | ✅ Mitigated |
| ID-I2 | Memory dump key extraction | Zeroizing<Vec<u8>> for passphrase | ✅ Mitigated |
| ID-I3 | Derive key from DID | Ed25519 one-way derivation | ✅ Mitigated |

#### Denial of Service

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| ID-D1 | Lock user out of keystore | No remote lockout possible | ✅ Mitigated |
| ID-D2 | Flood DID resolution | Local keystore, no remote | ✅ Mitigated |

#### Elevation of Privilege

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| ID-E1 | Add unauthorized device | Owner approval required | ✅ Mitigated |
| ID-E2 | Elevate device capabilities | Capability system per device | ✅ Mitigated |

### 4.2 Network System

#### Spoofing

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| NET-S1 | Spoof source DID | TLS client cert verification | ✅ Mitigated |
| NET-S2 | mDNS spoofing | DID verification on connect | ✅ Mitigated |
| NET-S3 | Spoof peer in messages | SignedEnvelope verification | ✅ Mitigated |

#### Tampering

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| NET-T1 | Modify message in transit | TLS 1.3 encryption | ✅ Mitigated |
| NET-T2 | Inject messages | SignedEnvelope verification | ✅ Mitigated |
| NET-T3 | Replay messages | ReplayGuard + sequence numbers | ✅ Mitigated |

#### Repudiation

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| NET-R1 | Deny sending message | SignedEnvelope includes sender DID | ✅ Mitigated |

#### Information Disclosure

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| NET-I1 | Eavesdrop network traffic | QUIC/TLS 1.3 encryption | ✅ Mitigated |
| NET-I2 | Traffic analysis | ⚠️ Not addressed | 🔴 Gap |
| NET-I3 | Connection metadata leakage | ⚠️ mDNS exposes local presence | 🟡 Partial |

#### Denial of Service

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| NET-D1 | Connection flooding | Max 100 peers, eviction policy | ✅ Mitigated |
| NET-D2 | Message flooding | Trust-gated rate limiting | ✅ Mitigated |
| NET-D3 | Stream exhaustion | QUIC 10 stream limit | ✅ Mitigated |
| NET-D4 | Memory exhaustion via messages | 10MB max message, validate before alloc | ✅ Mitigated |

#### Elevation of Privilege

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| NET-E1 | Bypass trust-gated rate limits | Trust score from graph | ✅ Mitigated |
| NET-E2 | Access without DID-TLS binding | Reject invalid bindings | ✅ Mitigated |

### 4.3 Trust System

#### Spoofing

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| TR-S1 | Create fake trust edges | SignedEnvelope verification | ✅ Mitigated |
| TR-S2 | Sybil attack (many identities) | ⚠️ Requires external identity | 🔴 Gap |

#### Tampering

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| TR-T1 | Modify trust edge weight | Signature on edge data | ✅ Mitigated |
| TR-T2 | Manipulate decay timestamps | Timestamp in signed data | ✅ Mitigated |

#### Repudiation

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| TR-R1 | Deny trust attestation | Signed edges with timestamp | ✅ Mitigated |

#### Information Disclosure

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| TR-I1 | Enumerate trust relationships | ⚠️ Trust graph queryable | 🟡 By Design |

#### Denial of Service

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| TR-D1 | Trust computation exhaustion | Bounded depth (3 hops), cycle prevention | ✅ Mitigated |
| TR-D2 | Spam trust edges | Rate limiting on edge creation | ✅ Mitigated |

#### Elevation of Privilege

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| TR-E1 | Trust inflation via collusion | ⚠️ Bounded computation limits impact | 🟡 Partial |
| TR-E2 | Bypass trust thresholds | Configurable minimum thresholds | ✅ Mitigated |

### 4.4 Gossip System

#### Spoofing

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GOS-S1 | Spoof gossip message origin | SignedEnvelope verification | ✅ Mitigated |
| GOS-S2 | Spoof anti-entropy responses | SignedEnvelope verification | ✅ Mitigated |

#### Tampering

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GOS-T1 | Modify gossip entries | Content-addressed hashing | ✅ Mitigated |
| GOS-T2 | Tamper with vector clocks | Signed vector clock updates | ✅ Mitigated |

#### Repudiation

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GOS-R1 | Deny publishing entry | Entry includes signed creator DID | ✅ Mitigated |

#### Information Disclosure

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GOS-I1 | Access private topic data | AccessControl::Private enforcement | ✅ Mitigated |
| GOS-I2 | Enumerate topic subscribers | ⚠️ Subscription metadata visible | 🟡 Partial |

#### Denial of Service

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GOS-D1 | Gossip flooding | Trust-gated rate limiting | ✅ Mitigated |
| GOS-D2 | Bloom filter overflow | Bounds validation | ✅ Mitigated |
| GOS-D3 | Topic entry exhaustion | Entry limit per topic (1000) | ✅ Mitigated |
| GOS-D4 | Large entry flooding | zstd compression, size limits | ✅ Mitigated |

#### Elevation of Privilege

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GOS-E1 | Subscribe to trust-gated topic | Trust threshold check | ✅ Mitigated |
| GOS-E2 | Publish to restricted topic | AccessControl enforcement | ✅ Mitigated |

### 4.5 Ledger System

#### Spoofing

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| LED-S1 | Create transaction as another user | SignedEnvelope verification | ✅ Mitigated |
| LED-S2 | Forge journal entry | Entry signature verification | ✅ Mitigated |

#### Tampering

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| LED-T1 | Modify transaction amount | Signed entry data | ✅ Mitigated |
| LED-T2 | Alter transaction history | Merkle-DAG chain | ✅ Mitigated |
| LED-T3 | Double-spend | Quarantine + sync protocol | ✅ Mitigated |

#### Repudiation

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| LED-R1 | Deny creating transaction | Signed journal entries | ✅ Mitigated |
| LED-R2 | Deny dispute filing | Dispute record with signature | ✅ Mitigated |

#### Information Disclosure

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| LED-I1 | View other users' balances | ⚠️ Balance queries permitted | 🟡 By Design |
| LED-I2 | Transaction history exposure | Per-coop isolation | ✅ Mitigated |

#### Denial of Service

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| LED-D1 | Transaction flooding | Dynamic credit limits | ✅ Mitigated |
| LED-D2 | Sync exhaustion | Gossip rate limiting | ✅ Mitigated |

#### Elevation of Privilege

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| LED-E1 | Exceed credit limit | Dynamic limit enforcement | ✅ Mitigated |
| LED-E2 | Bypass new member throttling | Progressive ramping (90 days) | ✅ Mitigated |
| LED-E3 | Create unbacked credit | Double-entry accounting | ✅ Mitigated |

### 4.6 Governance System

#### Spoofing

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GOV-S1 | Vote as another member | JWT auth + DID verification | ✅ Mitigated |
| GOV-S2 | Spoof governance messages | SignedEnvelope verification | ✅ Mitigated |

#### Tampering

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GOV-T1 | Modify proposal content | Signed proposal data | ✅ Mitigated |
| GOV-T2 | Alter vote tallies | Signed individual votes | ✅ Mitigated |
| GOV-T3 | Change proposal state illegally | State machine validation | ✅ Mitigated |

#### Repudiation

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GOV-R1 | Deny casting vote | Signed vote records | ✅ Mitigated |
| GOV-R2 | Deny creating proposal | Signed proposal | ✅ Mitigated |

#### Information Disclosure

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GOV-I1 | See votes before voting closes | ⚠️ Votes visible in real-time | 🟡 By Design |

#### Denial of Service

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GOV-D1 | Proposal spam | Rate limiting | ✅ Mitigated |
| GOV-D2 | Block quorum | ⚠️ No forced participation | 🟡 By Design |

#### Elevation of Privilege

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GOV-E1 | Vote without membership | Membership resolver check | ✅ Mitigated |
| GOV-E2 | Close proposal prematurely | State machine + timing | ✅ Mitigated |
| GOV-E3 | Bypass quorum requirements | Quorum validation | ✅ Mitigated |

### 4.7 Compute System

#### Spoofing

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| COM-S1 | Claim task execution falsely | Ed25519 signature on results | ✅ Mitigated |
| COM-S2 | Spoof submitter identity | JWT auth + DID verification | ✅ Mitigated |

#### Tampering

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| COM-T1 | Modify task code | Content-addressed task hash | ✅ Mitigated |
| COM-T2 | Alter execution results | Signed result payloads | ✅ Mitigated |

#### Repudiation

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| COM-R1 | Deny task submission | Signed submission record | ✅ Mitigated |
| COM-R2 | Deny execution | Executor signature on result | ✅ Mitigated |

#### Information Disclosure

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| COM-I1 | Side-channel data extraction | CCL deterministic execution | ✅ Mitigated |
| COM-I2 | Task content exposure | ⚠️ Task code visible to executors | 🟡 By Design |

#### Denial of Service

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| COM-D1 | Fuel bombing | Fuel metering with limits | ✅ Mitigated |
| COM-D2 | Task queue flooding | Rate limiting, trust-gated submission | ✅ Mitigated |
| COM-D3 | Executor exhaustion | Capacity limits (max_concurrent_tasks) | ✅ Mitigated |
| COM-D4 | Timeout abuse | Automatic timeout enforcement | ✅ Mitigated |

#### Elevation of Privilege

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| COM-E1 | Execute without sufficient trust | MIN_TRUST_EXECUTE (0.3) | ✅ Mitigated |
| COM-E2 | Bypass capability restrictions | CCL capability system | ✅ Mitigated |
| COM-E3 | Stack overflow via nested exprs | MAX_EXPRESSION_DEPTH validation | ✅ Mitigated |

### 4.8 Gateway API

#### Spoofing

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GW-S1 | Impersonate authenticated user | JWT token verification | ✅ Mitigated |
| GW-S2 | Forge challenge-response | Ed25519 signature verification | ✅ Mitigated |

#### Tampering

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GW-T1 | Modify JWT payload | JWT signature verification | ✅ Mitigated |
| GW-T2 | Tamper with request body | HTTPS in production | ✅ Mitigated |

#### Repudiation

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GW-R1 | Deny API action | JWT claims + audit logging | ✅ Mitigated |

#### Information Disclosure

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GW-I1 | Token theft | Token expiry (24h default) | ✅ Mitigated |
| GW-I2 | Scope information leakage | Scope-based authorization | ✅ Mitigated |
| GW-I3 | Error message data leakage | Sanitized error responses | ✅ Mitigated |

#### Denial of Service

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GW-D1 | API flooding | Per-DID rate limiting (100 burst) | ✅ Mitigated |
| GW-D2 | WebSocket exhaustion | Connection limits + heartbeat | ✅ Mitigated |
| GW-D3 | Challenge accumulation | 5-minute challenge TTL | ✅ Mitigated |

#### Elevation of Privilege

| Threat | Description | Mitigation | Status |
|--------|-------------|------------|--------|
| GW-E1 | Access without required scope | Scope enforcement per endpoint | ✅ Mitigated |
| GW-E2 | Bypass authentication | JWT middleware on protected routes | ✅ Mitigated |
| GW-E3 | TOCTOU race conditions | Atomic operations, duplicate ID prevention | ✅ Mitigated |

---

## 5. Mitigations Reference

### 5.1 Transport Layer Security

| Control | Implementation | Files |
|---------|----------------|-------|
| QUIC/TLS 1.3 | quinn crate with rustls | `icn-net/src/session.rs` |
| DID-TLS Binding | Certificate SAN + signature | `icn-net/src/tls.rs` |
| Connection Limits | PeerRegistry with eviction | `icn-net/src/actor.rs` |
| Stream Limits | QUIC transport config | `icn-net/src/session.rs` |

### 5.2 Message Layer Security

| Control | Implementation | Files |
|---------|----------------|-------|
| SignedEnvelope | Ed25519 signatures | `icn-net/src/envelope.rs` |
| ReplayGuard | Sequence + Bloom filter | `icn-net/src/replay_guard.rs` |
| EncryptedEnvelope | X25519-ChaCha20-Poly1305 | `icn-net/src/encrypted.rs` |
| Message Size Limits | 10MB max, validate before alloc | `icn-net/src/protocol.rs` |

### 5.3 Trust-Based Rate Limiting

| Trust Class | Rate (msg/sec) | Burst | Threshold |
|-------------|----------------|-------|-----------|
| Isolated | 10 | 2 | <0.1 |
| Known | 50 | 10 | 0.1-0.4 |
| Partner | 100 | 20 | 0.4-0.7 |
| Federated | 200 | 50 | >0.7 |

**Implementation**: `icn-net/src/rate_limit.rs`

### 5.4 Authentication & Authorization

| Control | Implementation | Files |
|---------|----------------|-------|
| Challenge-Response | 5-minute nonce TTL | `icn-gateway/src/auth.rs` |
| JWT Tokens | 24-hour expiry, HS256 | `icn-gateway/src/auth.rs` |
| Scope Enforcement | Per-endpoint checks | `icn-gateway/src/middleware.rs` |
| Rate Limiting | Token bucket per DID | `icn-gateway/src/rate_limit.rs` |

### 5.5 Economic Controls

| Control | Implementation | Files |
|---------|----------------|-------|
| Dynamic Credit Limits | Trust + history formula | `icn-ledger/src/credit_policy.rs` |
| New Member Throttling | 90-day progressive ramp | `icn-ledger/src/credit_policy.rs` |
| Dispute Resolution | Mediator workflow | `icn-ledger/src/dispute.rs` |
| Double-Entry Accounting | Debit/credit invariants | `icn-ledger/src/ledger.rs` |

### 5.6 Contract Security

| Control | Implementation | Files |
|---------|----------------|-------|
| Fuel Metering | Per-statement cost | `icn-ccl/src/interpreter.rs` |
| Capability System | Read/Write restrictions | `icn-ccl/src/capability.rs` |
| Expression Depth | MAX_EXPRESSION_DEPTH | `icn-ccl/src/ast.rs` |
| Input Validation | Name, var, rule limits | `icn-ccl/src/ast.rs` |

---

## 6. Gap Analysis

### 6.1 High Priority Gaps

| ID | Gap | Risk | Recommendation | Effort |
|----|-----|------|----------------|--------|
| GAP-H1 | No Sybil attack prevention | Attackers can create many identities | Require external identity proof or stake | High |
| GAP-H2 | Traffic analysis possible | Network metadata reveals activity patterns | Implement mix networking or padding | High |
| GAP-H3 | No Byzantine fault tolerance | Equivocating nodes not detected | Implement BFT consensus for critical data | High |

### 6.2 Medium Priority Gaps

| ID | Gap | Risk | Recommendation | Effort |
|----|-----|------|----------------|--------|
| GAP-M1 | Trust inflation via collusion | Colluding nodes can boost each other | Add Sybil resistance metrics | Medium |
| GAP-M2 | mDNS exposes local presence | Local network attackers see nodes | Add optional private discovery | Medium |
| GAP-M3 | Subscription metadata visible | Topic structure reveals interests | Add encrypted topic names | Medium |
| GAP-M4 | Votes visible during voting | Enables vote buying | Implement commit-reveal scheme | Medium |

### 6.3 Low Priority Gaps

| ID | Gap | Risk | Recommendation | Effort |
|----|-----|------|----------------|--------|
| GAP-L1 | Balance queries public | Privacy concern | Add optional balance privacy | Low |
| GAP-L2 | Task code visible to executors | IP exposure for compute tasks | Implement FHE or TEE execution | High |
| GAP-L3 | No perfect forward secrecy | Past message decryption if key stolen | Implement ratcheting protocol | Medium |

### 6.4 By-Design Limitations

These are intentional design decisions, not gaps:

| Item | Rationale |
|------|-----------|
| Public trust graph | Transparency is core cooperative value |
| Real-time vote visibility | Prevents hidden vote manipulation |
| Quorum not enforceable | Cannot force participation in democratic systems |

---

## 7. Security Assumptions

### 7.1 What This System Provides

- **Authenticated communication** between known identities
- **Integrity** of messages and stored data
- **Non-repudiation** of signed actions
- **Availability** under normal and moderate attack conditions
- **Economic safety rails** against common abuse patterns
- **Capability-constrained** contract execution

### 7.2 What This System Does NOT Provide

- **Anonymous communication** - All actions tied to DIDs
- **Byzantine fault tolerance** - Assumes honest majority
- **Perfect forward secrecy** - Key compromise reveals past messages
- **Traffic analysis resistance** - Network patterns observable
- **Sybil resistance** - No external identity verification
- **Legal enforceability** - Technical system only

### 7.3 Trust Assumptions

| Assumption | Impact if Violated |
|------------|-------------------|
| Ed25519 is secure | Complete system compromise |
| TLS 1.3 is secure | Network traffic exposure |
| ChaCha20-Poly1305 is secure | Encrypted data exposure |
| Age encryption is secure | Keystore compromise |
| System clocks are roughly synchronized | Replay attacks possible |
| Cooperative members are mostly honest | Governance manipulation |

---

## 8. References

### Internal Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture
- [production-hardening.md](production-hardening.md) - Security fixes
- [security-roadmap.md](security-roadmap.md) - Security implementation plan
- [phase-10c-security-analysis.md](phase-10c-security-analysis.md) - Contract security

### External References

- [STRIDE Threat Modeling](https://docs.microsoft.com/en-us/azure/security/develop/threat-modeling-tool-threats)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [QUIC RFC 9000](https://www.rfc-editor.org/rfc/rfc9000.html)
- [TLS 1.3 RFC 8446](https://www.rfc-editor.org/rfc/rfc8446.html)
- [Ed25519 RFC 8032](https://www.rfc-editor.org/rfc/rfc8032.html)

---

## Changelog

- **2025-11-29**: Initial comprehensive STRIDE analysis (v1.0)
