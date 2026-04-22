# ICN - InterCooperative Network

[![CI](https://github.com/InterCooperative-Network/icn/actions/workflows/ci.yml/badge.svg)](https://github.com/InterCooperative-Network/icn/actions/workflows/ci.yml)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](LICENSE)

A shared infrastructure effort for cooperatives, communities, and federations.

**[intercooperative.network](https://intercooperative.network)**

ICN is institutional infrastructure for democratic organizations. It is being built so cooperatives, communities, and federations can prove decisions, operate on infrastructure they control, and coordinate across organizational boundaries without handing those functions to a platform landlord.

The Rust workspace lives in [`icn/`](icn/). The public site lives in [`website/`](website/). The project is large and uneven in maturity, so the fastest way to avoid getting lost is to start from the right entrypoint for your role.

## Start Here by Role

- **Understand the project first**: start with [intercooperative.network](https://intercooperative.network), especially [What is ICN](https://intercooperative.network/what-is-icn), [What's Real Now](https://intercooperative.network/whats-real-now), and [Get Involved](https://intercooperative.network/get-involved).
- **Contribute technically**: read [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md), then [CONTRIBUTING.md](CONTRIBUTING.md), then [docs/onboarding/README.md](docs/onboarding/README.md). For a first scoped contribution, start with [good first issues](https://github.com/InterCooperative-Network/icn/issues?q=is%3Aissue+is%3Aopen+label%3Agood-first-issue).
- **Contribute non-technically**: use the public [Get Involved](https://intercooperative.network/get-involved) page for docs, design, testing, research, policy, and ecosystem paths, then open a [GitHub Discussion](https://github.com/InterCooperative-Network/icn/discussions) or issue once the work is concrete.
- **Bring an institutional use case**: read [For Cooperatives](https://intercooperative.network/for-cooperatives) and [What's Real Now](https://intercooperative.network/whats-real-now), then open a [GitHub Discussion](https://github.com/InterCooperative-Network/icn/discussions).
- **Support the work financially**: the live rail today is [GitHub Sponsors](https://github.com/sponsors/InterCooperative-Network).

---

## Developer Quickstart

If you want to build the codebase and orient quickly:

```bash
git clone https://github.com/InterCooperative-Network/icn.git
cd icn/icn
cargo build
cargo test --workspace --lib
```

Before opening a PR, read:

- [AGENTS.md](AGENTS.md) for repo operating rules, verification routing, and invariants
- [CONTRIBUTING.md](CONTRIBUTING.md) for architectural guardrails
- [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md) for a cleaner contributor-first startup path

## What ICN Provides

- **Decentralized Identity** — DIDs with Ed25519 cryptography and Age-encrypted keystores
- **Trust Graph** — Web-of-participation trust computation with signed attestations
- **Mutual Credit Ledger** — Double-entry accounting with Merkle-DAG integrity
- **Cooperative Contracts** — CCL (Cooperative Contract Language) for expressing bylaws, agreements, and governance
- **P2P Networking** — QUIC/TLS sessions with mDNS discovery and gossip replication
- **Democratic Governance** — Proposals, voting, and parameter management
- **Distributed Compute** — Trust-gated task execution with receipt settlement

## Architecture

ICN implements a **constraint enforcement architecture**:

```
CCL Document (constitution / bylaws / treaty)
         |
App / Policy Oracle (governance, trust, ledger)
         |
ConstraintSet (rate limits, credit ceilings, voting weights)
         |
Kernel enforces constraints mechanically
```

The kernel never sees "trust scores" or "governance rules" — only generic constraints. This is the **Meaning Firewall**: domain semantics stay in apps, the kernel stays predictable.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full architecture documentation.

## Repo Map

The repo is split across several surfaces:

- [`icn/`](icn/) — Rust workspace: crates in `icn/crates/`, runtime apps in `icn/apps/`, binaries in `icn/bins/`
- [`website/`](website/) — public site for `intercooperative.network`
- [`docs/`](docs/) — architecture, reference, contributor, and operator documentation
- [`sdk/`](sdk/) — TypeScript and React Native SDK work
- [`deploy/`](deploy/) — deployment manifests and cluster configuration

### Common Ports

| Service | Port | Protocol | Purpose |
|---------|------|----------|---------|
| Peer Transport | 7777 | QUIC/UDP | P2P communication |
| RPC API | 5601 | HTTP | CLI control (icnctl) |
| Metrics | 9100 | HTTP | Prometheus exporter |
| Health | 8080 | HTTP | Health checks |

## Development

```bash
cd icn

cargo build
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --lib
```

Run the checks that match what you touched. [AGENTS.md](AGENTS.md) is the routing table for Rust crates, website work, SDK work, and docs changes.

## Security

ICN implements three-layer security:

1. **Transport** — QUIC/TLS with DID-TLS certificate binding
2. **Message** — Ed25519 signed envelopes with replay protection
3. **Application** — E2E encryption with X25519-ChaCha20-Poly1305

Trust-gated rate limiting enforces per-actor throughput bounds based on trust class. See [docs/production-hardening.md](docs/production-hardening.md).

## Documentation

- **[docs/GETTING_STARTED.md](docs/GETTING_STARTED.md)** — developer and evaluator startup path
- **[CONTRIBUTING.md](CONTRIBUTING.md)** — contribution guardrails and PR expectations
- **[docs/onboarding/README.md](docs/onboarding/README.md)** — deeper developer onboarding curriculum
- **[docs/INDEX.md](docs/INDEX.md)** — master doc navigation
- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — architecture deep dive
- **[docs/STATE.md](docs/STATE.md)** — current project snapshot
- **[Production Hardening](docs/production-hardening.md)** — security and deployment

## Status

ICN is real, active, and uneven in maturity. The strongest parts today are provenance, cryptographic identity, and the decision-to-record path. Member-facing polish, broader execution coverage, and cleaner onboarding surfaces are still being built. For the current truth plane, read [docs/STATE.md](docs/STATE.md) and the public [What's Real Now](https://intercooperative.network/whats-real-now) page.

## License

[AGPL-3.0](LICENSE)
