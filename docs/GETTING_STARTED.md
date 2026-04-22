# Getting Started with ICN

This guide is the contributor and evaluator entrypoint for the ICN repo. It is written for people who want to understand the project, build the codebase, run the main checks, and choose an initial way to help without guessing their way through the repository.

If you are looking for institutional engagement, non-technical contribution, or financial support, use the public [Get Involved](https://intercooperative.network/get-involved) page first.

## Choose Your Starting Path

- **I want to understand the project before building anything**: start with [intercooperative.network/what-is-icn](https://intercooperative.network/what-is-icn), [intercooperative.network/for-developers](https://intercooperative.network/for-developers), and [intercooperative.network/whats-real-now](https://intercooperative.network/whats-real-now)
- **I want to contribute code**: keep reading this guide, then read [CONTRIBUTING.md](../CONTRIBUTING.md)
- **I want a deeper technical curriculum**: use [docs/onboarding/README.md](onboarding/README.md)
- **I want to run a node locally and inspect the daemon**: follow the local-node section below after you build the workspace

## Developer Quickstart

### Prerequisites

- Rust toolchain from `icn/rust-toolchain.toml`
- Git
- Enough disk for a full workspace build and incremental cache

### Clone and build

```bash
git clone https://github.com/InterCooperative-Network/icn.git
cd icn/icn
cargo build
cargo test --workspace --lib
```

Important: the Rust workspace is in the `icn/` subdirectory, not at the repo root.

### Read these next

1. [README.md](../README.md) — repo map and role-based routing
2. [AGENTS.md](../AGENTS.md) — verification rules, invariants, and crate layout
3. [CONTRIBUTING.md](../CONTRIBUTING.md) — PR expectations and architectural guardrails
4. [docs/onboarding/README.md](onboarding/README.md) — structured contributor path

### Repo orientation

- `icn/` — Rust workspace
- `icn/crates/` — core crates and shared subsystems
- `icn/apps/` — runtime-integrated application crates
- `icn/bins/` — binaries such as `icnd` and `icnctl`
- `website/` — public site for `intercooperative.network`
- `docs/` — architecture, guides, reference material, and onboarding

### Choose an initial area

- Start with [good first issues](https://github.com/InterCooperative-Network/icn/issues?q=is%3Aissue+is%3Aopen+label%3Agood-first-issue) if you want a scoped first PR
- Use [GitHub Discussions](https://github.com/InterCooperative-Network/icn/discussions) if the work is exploratory, architectural, institutional, or not yet issue-shaped
- If you want to read first and patch later, follow the reading order on [For Developers](https://intercooperative.network/for-developers)

### If you are helping without writing Rust

- Documentation work: use [docs/INDEX.md](INDEX.md) to find the right surface, then open a scoped docs issue or PR
- Design, research, testing, and ecosystem work: use [intercooperative.network/get-involved](https://intercooperative.network/get-involved) for the current public routing, then move concrete proposals into [GitHub Discussions](https://github.com/InterCooperative-Network/icn/discussions)
- Institutional questions: start with [intercooperative.network/for-cooperatives](https://intercooperative.network/for-cooperatives) and [intercooperative.network/whats-real-now](https://intercooperative.network/whats-real-now), then open a Discussion with the real use case
- Financial support: the live path today is [GitHub Sponsors](https://github.com/sponsors/InterCooperative-Network)

### Verify before you push

```bash
cd icn
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --lib
```

Run the checks that match the area you touched. `AGENTS.md` is the routing table for workspace crates, website work, SDK work, and docs.

## Running a Local Node

If you want to inspect the daemon and CLI directly after building:

### Create an identity

```bash
cd icn
./target/debug/icnctl --data-dir ~/.icn id init
./target/debug/icnctl --data-dir ~/.icn id show
```

### Start the daemon

```bash
cd icn
./target/debug/icnd --data-dir ~/.icn
```

### Check status from another terminal

```bash
cd icn
./target/debug/icnctl --data-dir ~/.icn status
./target/debug/icnctl --data-dir ~/.icn network peers
```

### Useful local endpoints

- Gateway health: `http://localhost:8080/v1/health`
- Metrics: `http://localhost:9100/metrics`

## Other Real Ways to Help

If you are not contributing Rust code, there are still real paths:

- docs, design, testing, research, governance, and ecosystem work are routed through [intercooperative.network/get-involved](https://intercooperative.network/get-involved)
- institutional use cases and partnership questions belong in [GitHub Discussions](https://github.com/InterCooperative-Network/icn/discussions)
- financial support goes through [GitHub Sponsors](https://github.com/sponsors/InterCooperative-Network)

## What This Guide Does Not Promise

- a polished non-technical onboarding flow for ordinary members
- a hosted sign-up path or managed pilot program
- a finished member-facing product surface

Those are still in progress. This guide is for building, reading, testing, and contributing against the repo as it exists today.

## Next Documents

- [README.md](../README.md) for the repo map and public-routing layer
- [CONTRIBUTING.md](../CONTRIBUTING.md) for PR expectations and architectural guardrails
- [onboarding/README.md](onboarding/README.md) for the longer contributor curriculum
- [ARCHITECTURE.md](ARCHITECTURE.md) and [STATE.md](STATE.md) if you need deeper technical and project-status context

## Where To Ask Or Contribute Next

- Use [GitHub Issues](https://github.com/InterCooperative-Network/icn/issues) for scoped implementation and documentation work
- Use [GitHub Discussions](https://github.com/InterCooperative-Network/icn/discussions) for design questions, institutional use cases, and broader project conversation
- Use [intercooperative.network/get-involved](https://intercooperative.network/get-involved) if you are deciding how to help across technical, non-technical, institutional, or financial paths
