# ICN Membership App

Unified membership management for all entity types in ICN.

## Overview

The membership app consolidates membership models from `icn-entity`, `icn-coop`, and `icn-community` into a single, CCL-driven implementation.

## Features

- **Unified Entity Model**: Supports individuals, cooperatives, communities, and federations
- **Generic Membership Trait**: Common interface for all membership types
- **CCL-Based Criteria**: Membership rules defined in Cooperative Contract Language
- **Cooperative Features**: Share management, labor assignments, multi-coop workers
- **Community Features**: Multi-type members, weighted voting

## Architecture

```
MembershipApp
  ├── entity (unified EntityId model)
  ├── membership (generic trait + UnifiedMembership)
  ├── coop (cooperative-specific logic)
  └── community (community-specific logic)
```

## CCL Integration

Second CCL consumer in ICN (after governance). Example membership criteria:

```yaml
entity:
  name: "Rochester Civic Assembly"
  type: community
  membership:
    classes:
      - name: resident
        criteria:
          all:
            - field: verified_address
              op: "=="
              value: true
```

## Usage

```rust
use icn_membership_app::{
    EntityConfig, MembershipManager, MembershipRole,
    CoopMembershipManager, CommunityMembershipManager
};

// Create a cooperative
let config = EntityConfig::cooperative("food-coop", "Food Coop".to_string());

// Add a member
let manager = CoopMembershipManager::new();
let membership = manager
    .add_coop_member(member_id, coop_id, MembershipRole::Worker, &config)
    .await?;
```

## Testing

```bash
cargo test -p icn-membership-app
```

All 19 tests pass, covering:
- Entity config creation
- Membership state transitions
- CCL criteria evaluation
- Cooperative and community features
- Backward compatibility

## Status

✅ Phase 5 complete - All acceptance criteria met:
- Single app handles all entity types
- Old crates can use forwarding wrappers
- CCL schema drives membership rules
- All entity types (individual, coop, community, federation) work
