# CoopOS: ICN-Native Linux Distribution

**Version**: 0.1.0
**Status**: Vision/Design Phase
**Last Updated**: 2025-01-25

> **Note**: This document describes a long-term vision. CoopOS is not yet in development. The realistic timeline is 2-3 years after kernel stabilization.

---

## Executive Summary

CoopOS is a Linux distribution purpose-built for cooperative organizations, where ICN is the native identity and coordination layer. This isn't about replacing Windows - it's about building the primary operating system for cooperative enterprise from the ground up.

**Key insight**: Every workstation becomes a node in the cooperative cloud, contributing resources when idle while prioritizing local user experience.

---

## The Cooperative Cloud

ICN as a whole functions as a **cooperative cloud** - distributed infrastructure owned and operated by the cooperative movement, not rented from corporations.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         THE COOPERATIVE CLOUD                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Traditional Cloud:                                                          │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  Amazon/Google/Microsoft own the servers                             │   │
│  │  You rent compute, storage, bandwidth                                │   │
│  │  They extract profit, control your data                              │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  Cooperative Cloud:                                                          │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  Coops collectively own the infrastructure                           │   │
│  │  Each org contributes compute, storage, bandwidth                    │   │
│  │  Resources shared via mutual aid, not rent extraction               │   │
│  │  Democratic governance of shared infrastructure                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Org Nodes as Centralized Services

Each cooperative runs **org nodes** that serve the same function as traditional enterprise servers:

| Windows Server Provides | ICN Org Nodes Provide |
|------------------------|----------------------|
| Active Directory (identity) | DID-based identity + SSO |
| Group Policy (device mgmt) | Capability-based policies |
| File Server (shared storage) | Namespaced state + sync |
| SQL Server (databases) | Event logs + KV stores |
| Exchange (email/calendar) | Comms primitives + apps |
| WSUS (updates) | App deployment via manifests |

**Key difference:**
- Windows Server is proprietary, licensed per-seat
- ICN org nodes are cooperative-owned, no licensing
- Resources can be shared across the federation

---

## Workstations as Network Nodes

When a workstation runs CoopOS and joins an org, it becomes a **node in the network** - not just a client consuming services, but a contributor to collective infrastructure.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                 WORKSTATION = NODE (NOT JUST CLIENT)                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Traditional model:                                                          │
│  ┌─────────────┐         ┌─────────────┐                                   │
│  │ Workstation │────────►│   Server    │  Workstation consumes              │
│  │  (client)   │◄────────│  (provider) │  Server provides                   │
│  └─────────────┘         └─────────────┘  One-way dependency                │
│                                                                              │
│  ICN model:                                                                  │
│  ┌─────────────┐         ┌─────────────┐                                   │
│  │ Workstation │◄───────►│  Org Nodes  │  Workstation contributes AND       │
│  │   (node)    │◄───────►│   (nodes)   │  consumes. Mutual aid.            │
│  └─────────────┘         └─────────────┘                                    │
│        │                                                                     │
│        └──────────────────►┌─────────────┐                                  │
│                            │  Network    │  Workstation also contributes    │
│                            │   (cloud)   │  to broader cooperative cloud    │
│                            └─────────────┘                                  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Resource Priority Hierarchy

The core principle: **minimum latency for users, maximum contribution to the collective**.

### Priority Levels

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    RESOURCE PRIORITY HIERARCHY                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  PRIORITY 1: LOCAL (User's workstation)                                     │
│  ════════════════════════════════════════                                   │
│  • Compute: User's active tasks get full local CPU/GPU                      │
│  • Storage: Hot data cached locally for instant access                      │
│  • Bandwidth: User's network requests prioritized                           │
│  • Goal: Minimum latency, best UX for the person at the keyboard           │
│                                                                              │
│  PRIORITY 2: ORG (Cooperative's collective resources)                       │
│  ═══════════════════════════════════════════════════                        │
│  • Compute: Idle cycles contribute to org workloads                         │
│  • Storage: Org data replicated across org workstations                     │
│  • Bandwidth: Org sync traffic before external traffic                      │
│  • Goal: Org self-sufficiency, reduce external dependencies                │
│                                                                              │
│  PRIORITY 3: NETWORK (Cooperative cloud)                                    │
│  ═══════════════════════════════════════                                    │
│  • Compute: Remaining idle cycles to federation/network                     │
│  • Storage: Contribute to distributed storage pool                          │
│  • Bandwidth: Assist network routing, content distribution                  │
│  • Goal: Collective infrastructure for the movement                         │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Example: Alice's Workstation Throughout the Day

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    EXAMPLE: ALICE'S WORKSTATION                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  9:00 AM - Alice actively working                                           │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  CPU: 80% Alice's apps │ 15% Org background │ 5% Network            │   │
│  │  RAM: 12GB Alice       │ 3GB Org cache      │ 1GB Network           │   │
│  │  Net: Alice traffic prioritized, sync in background                  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  12:00 PM - Alice at lunch (workstation idle)                               │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  CPU: 5% System        │ 60% Org batch jobs │ 35% Network compute   │   │
│  │  RAM: 2GB System       │ 8GB Org tasks      │ 6GB Network cache     │   │
│  │  Net: Org sync primary, network contribution secondary               │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  6:00 PM - Alice logged out (workstation on but unused)                     │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  CPU: 5% System        │ 40% Org overnight  │ 55% Network compute   │   │
│  │  RAM: 2GB System       │ 6GB Org tasks      │ 8GB Network cache     │   │
│  │  Net: Heavy contribution to cooperative cloud                        │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  Alice returns - resources INSTANTLY reprioritized to local                 │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## CoopOS Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           COOPOS WORKSTATION                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    PRODUCTIVITY APPLICATIONS                         │   │
│  │  LibreOffice, GIMP, Inkscape, Firefox, Thunderbird, etc.            │   │
│  │  + ICN-native apps: Governance, Ledger, Scheduling, Inventory...    │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    ICN DESKTOP INTEGRATION                           │   │
│  │  • DID-based login (SSO everywhere)                                  │   │
│  │  • Capability-based file/resource access                             │   │
│  │  • Encrypted sync across devices                                     │   │
│  │  • Org-managed app deployment                                        │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    ICN NODE (EMBEDDED)                               │   │
│  │  • Full node capabilities (not just a client)                        │   │
│  │  • Resource scheduler (Local → Org → Network priority)              │   │
│  │  • State sync with org nodes                                         │   │
│  │  • Compute contribution manager                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                      LINUX BASE (Debian/Fedora)                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Key Features

### 1. DID-Based Login

```
┌─────────────────────────────────────────────────────────────────┐
│                      LOGIN EXPERIENCE                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  [Tap badge / Insert key]                                       │
│  [Biometric / PIN confirm]                                      │
│  → Logged in as did:icn:z6MkAlice...                           │
│  → ICN verifies DID against org membership                     │
│  → Capability tokens loaded for this session                   │
│  → Org policies applied                                         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘

Login methods:
• Hardware security key (YubiKey, etc.)
• NFC badge tap
• Biometric + PIN
• Phone wallet approval (push notification)
• Traditional password (fallback, discouraged)
```

### 2. Capability-Based Access Control

```rust
// Alice holds capability token granting read access to /finance/ until 2025-12-31

// Benefits:
// • Explicit, auditable grants
// • Time-limited by default
// • Delegable without admin intervention
// • Revocable instantly
// • Works offline (token is self-contained)
```

### 3. Single Sign-On Across All Applications

```
┌─────────────────────────────────────────────────────────────────┐
│                    SINGLE SIGN-ON FLOW                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. User logs into CoopOS (DID authentication)                  │
│                                                                  │
│  2. Session established with capability tokens                  │
│                                                                  │
│  3. User opens any application:                                 │
│     • LibreOffice → "Save to org shared drive" works           │
│     • Firefox → Org intranet recognizes user automatically     │
│     • Thunderbird → Email creds derived from DID               │
│     • ICN Governance → Already logged in                        │
│     • Third-party web apps → ICN OAuth bridge                  │
│                                                                  │
│  4. All access uses same capability tokens                      │
│     • No separate passwords per application                     │
│     • No password managers needed                               │
│     • Unified audit log                                         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 4. Distributed Compute Contribution

```
┌─────────────────────────────────────────────────────────────────┐
│                    COMPUTE CONTRIBUTION                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  IDLE WORKSTATION                                                │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ While Alice is at lunch, her workstation:                │   │
│  │ • Runs org batch jobs (report generation, backups)      │   │
│  │ • Processes governance app reducers                      │   │
│  │ • Contributes to federation compute pool                 │   │
│  │ • Earns compute credits for the org                      │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
│  CONTRIBUTION TIERS                                              │
│  ─────────────────                                               │
│  Personal:     Contribute to own tasks only                     │
│  Org pool:     Contribute to org's collective compute           │
│  Federation:   Contribute to federated coop compute pool        │
│  Network:      Contribute to global ICN network compute         │
│                                                                  │
│  RESOURCE LIMITS (user-configurable)                            │
│  ─────────────────────────────────────                          │
│  cpu_contribution: 50%  # When idle                             │
│  memory_reserved: 4GB   # Always keep for local use             │
│  storage_shared: 100GB  # For distributed storage               │
│  network_hours: 9am-6pm # Only contribute during work hours     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 5. Org-Managed App Deployment

```yaml
# Org defines standard workstation configuration
# Deployed via ICN, not proprietary tools

# org-workstation-policy.yaml
workstation_policy:
  org: did:icn:food-coop
  version: 2025.01

  required_apps:
    - name: libreoffice
      version: ">=7.5"
    - name: icn-governance
      version: ">=1.0"
    - name: icn-ledger-ui
      version: ">=1.0"
    - name: org-inventory-app
      source: /food-coop/apps/inventory

  security:
    screen_lock_timeout: 5m
    require_hardware_key: true
    allow_usb_storage: false

  network:
    dns_servers: [10.0.0.1, 10.0.0.2]  # Org DNS
    proxy: http://proxy.foodcoop.internal:8080

  sync:
    org_shared_drive: /food-coop/shared/
    personal_backup: /food-coop/members/{did}/backup/
```

### 6. Encrypted Sync Across Devices

```
┌─────────────────────────────────────────────────────────────────┐
│                    MULTI-DEVICE SYNC                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Alice has:                                                      │
│  • Desktop at work (CoopOS)                                     │
│  • Laptop for remote work (CoopOS)                              │
│  • Phone (ICN wallet app)                                       │
│                                                                  │
│  All devices sync via ICN:                                      │
│  • Documents in personal namespace                               │
│  • Application settings                                          │
│  • Credentials and capabilities                                  │
│  • Encryption keys (via secure device-to-device transfer)       │
│                                                                  │
│  End-to-end encrypted:                                           │
│  • Org nodes route sync traffic                                 │
│  • But cannot read contents (encrypted to Alice's keys)         │
│  • Even org admins cannot access Alice's personal files         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Workstation Onboarding

### Adding Workstations to an Org

```
┌─────────────────────────────────────────────────────────────────┐
│                 WORKSTATION ONBOARDING FLOW                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. INSTALL COOPOS                                              │
│     • Fresh install or existing Linux + ICN packages            │
│     • User creates personal DID during setup                    │
│                                                                  │
│  2. JOIN ORG                                                     │
│     • User authenticates to org with existing membership        │
│     • OR requests membership (org approval flow)                │
│     • Workstation receives org configuration                    │
│                                                                  │
│  3. CONFIGURE CONTRIBUTION                                       │
│     • User sets resource limits (or accepts org defaults)       │
│     • Local node syncs org state                                │
│     • Workstation begins contributing to org pool               │
│                                                                  │
│  4. OPERATIONAL                                                  │
│     • DID-based SSO works for all org resources                │
│     • Org apps available locally                                │
│     • Resources flow: Local → Org → Network                    │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Contribution Configuration

```yaml
# /etc/icn/contribution.yaml

# User-level limits (what this workstation contributes)
contribution:
  # When user is ACTIVE (typing, clicking, etc.)
  active:
    cpu_available: 20%      # Keep 80% for user
    memory_available: 2GB   # Keep rest for user
    network_priority: user_first

  # When user is IDLE (no input for 5+ minutes)
  idle:
    cpu_available: 70%      # Most CPU available
    memory_available: 8GB   # More memory available
    network_priority: balanced

  # When user is AWAY (screen locked, logged out)
  away:
    cpu_available: 90%      # Almost all CPU available
    memory_available: 12GB  # Almost all memory
    network_priority: contribution_first

# Priority order for contributed resources
priority:
  1: local           # Always highest - user's own tasks
  2: org             # Second - org's collective workloads
  3: federation      # Third - federated coops
  4: network         # Fourth - broader cooperative cloud

# Org-level policies (can override/constrain user settings)
org_policy:
  minimum_contribution:
    idle_cpu: 50%           # Org requires at least 50% when idle
  restricted_hours:
    contribution_only: false # Don't force contribution during work hours
  data_sovereignty:
    keep_org_data_in_org: true  # Org data doesn't leave org nodes
```

---

## Technical Components

### Required Development

| Component | Description | Status |
|-----------|-------------|--------|
| PAM Module | `pam_icn.so` for Linux login via DID | Not started |
| FUSE Filesystem | Mount ICN namespaces as regular filesystems | Not started |
| Desktop Integration | GNOME/KDE widgets, file manager, notifications | Not started |
| Resource Scheduler | Priority-based resource allocation | Not started |
| Compute Contribution | WASM job execution for network | Not started |

### PAM Module for ICN Authentication

```c
// pam_icn.so - Pluggable Authentication Module
// Allows Linux login via ICN DID

// Login flow:
// 1. User presents hardware key / biometric
// 2. PAM module contacts local ICN node
// 3. ICN node verifies DID and org membership
// 4. Capability tokens cached in kernel keyring
// 5. Session established with ICN identity
```

### FUSE Filesystem for ICN Storage

```bash
# Mount ICN namespaces as regular filesystems
$ mount -t icnfs /org/food-coop/shared /mnt/shared
$ mount -t icnfs /personal/alice /home/alice/icn

# Files are automatically:
# • Encrypted at rest (user's keys)
# • Synced across devices
# • Access-controlled via capabilities
# • Versioned (ICN event log)
```

### Desktop Integration

- GNOME/KDE integration for ICN wallet
- System tray showing connection status
- File manager integration (ICN namespaces appear as drives)
- Notification system for governance proposals, transfers
- Screen lock tied to ICN session

---

## Why This Matters

### For Small Coops (5-20 people)

- No IT department needed - identity and SSO work out of the box
- Computers "just work" with org identity
- Shared drives without complex setup
- Every workstation strengthens the org's infrastructure

### For Larger Coops (50+ people)

- Democratic control over IT infrastructure
- Compute resources shared efficiently across the org
- Easier onboarding (DID badge, instant access)
- Unified audit trail for compliance
- Org's workstations collectively provide server-class infrastructure

### For the Movement

- Cooperative cloud built from member contributions
- Not dependent on any corporation for infrastructure
- Federated across coops (shared compute, shared apps)
- Training materials and IT expertise shared across movement
- IT mutual aid between coops - help each other, not help desks

### The Math Works

```
Small coop: 10 workstations × 8 idle hours/day = 80 compute-hours/day for org
Medium coop: 50 workstations × 8 idle hours = 400 compute-hours/day for org
Federation: 10 coops × 50 workstations = 4,000 compute-hours/day shared

This is serious infrastructure, owned by the movement.
```

---

## Development Roadmap

### Current Status

**Phase**: Vision/design phase. Not yet in development.

### Dependencies

Requires stable kernel primitives first. CoopOS builds on:

- Identity primitive (for DID login)
- Authorization primitive (for capability-based access)
- State primitive (for synced filesystems)
- Compute primitive (for distributed workloads)
- Naming primitive (for resource discovery)

### Realistic Timeline

**2-3 years after kernel stabilization**

### Incremental Path

```
1. First: PAM module for ICN login on any Linux
   └── Minimal integration, proves DID login works

2. Then: FUSE filesystem for ICN storage
   └── ICN namespaces accessible as files

3. Then: Desktop integration packages
   └── GNOME/KDE widgets, file manager integration

4. Finally: Full distribution with everything integrated
   └── Complete CoopOS distribution
```

---

## Related Documents

- [KERNEL_CONTRACTS.md](../spec/KERNEL_CONTRACTS.md) - Kernel primitive specifications
- [CLIENT_MODEL.md](../architecture/CLIENT_MODEL.md) - Client and wallet architecture

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2025-01-25 | Initial vision document |
