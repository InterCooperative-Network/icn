# Lab 01: Workspace Setup

## Goal
Build a 2-crate workspace mimicking `icn-core` + `icnctl` to learn workspace organization and basic build flow.

## Structure
```
lab-01-workspace/
├── Cargo.toml          # Workspace manifest
├── core/
│   ├── Cargo.toml
│   └── src/lib.rs      # Library crate with actor-like struct
└── cli/
    ├── Cargo.toml
    └── src/main.rs     # Binary that depends on core
```

## Requirements

### 1. Workspace Setup
- Create `Cargo.toml` with workspace members
- Add `core` as library crate
- Add `cli` as binary crate depending on `core`

### 2. Dependencies
Add to `core/Cargo.toml`:
- `tracing = "0.1"`
- `thiserror = "1.0"`
- `serde = { version = "1.0", features = ["derive"] }`

### 3. Core Library
In `core/src/lib.rs`, create:
- `CoreActor` struct with internal state
- `CoreHandle` wrapping the actor
- Basic request/response via synchronous API

### 4. CLI Binary
In `cli/src/main.rs`:
- Initialize tracing subscriber
- Create `CoreActor`
- Send a request via handle
- Print response

## Done When
- `cargo build` succeeds
- `cargo test` runs (add at least one test)
- Running `cli` binary produces tracing output
- You understand workspace dependency management

## Resources
- [Cargo Workspaces](https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html)
- ICN workspace: `icn/Cargo.toml`
