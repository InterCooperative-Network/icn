# Security Policy

ICN is institutional infrastructure for democratic organizations. Security reports are taken seriously and handled in confidence.

## Reporting a Vulnerability

**Do not open a public issue for a suspected vulnerability.**

Use GitHub's **private vulnerability reporting** for this repository:

[Report a vulnerability](https://github.com/InterCooperative-Network/icn/security/advisories/new)

This routes directly to maintainers and is not visible to the public.

If GitHub's private reporting is unavailable for some reason, you can email the maintainer instead — see `Maintainer` in [README.md](README.md). Treat the email channel as a fallback, not a default.

## What to Include

A good report contains:

- A description of the issue and what it lets an attacker do.
- The affected component or surface (repository, crate, endpoint, configuration).
- The version, commit, or environment where you observed it.
- Steps to reproduce, ideally minimal.
- Any proposed mitigation, if you have one.

You do not need a polished writeup to file a report. A short note describing the issue is enough to start the conversation.

## What to Expect

- **Acknowledgement**: within roughly one business week.
- **Triage**: severity assessed against the [threat model](docs/security/threat-model.md) and the live deployment posture.
- **Fix and disclosure**: coordinated with the reporter. Fixes ship through the normal PR process. Disclosure timing is negotiated, but the project favors prompt, honest disclosure once a fix is available.
- **Credit**: reporters are credited unless they prefer to remain anonymous.

## Scope

In scope:

- The Rust workspace under [`icn/`](icn/) — kernel, apps, daemons, CLI, gateway.
- The TypeScript SDK under [`sdk/typescript/`](sdk/typescript/).
- The public website under [`website/`](website/).
- Deployment manifests under [`deploy/`](deploy/) when used as documented.

Out of scope (or handled separately):

- Findings against forks, third-party applications, or unmaintained branches.
- Social engineering, denial-of-service that requires resource exhaustion of a public deployment, or attacks requiring physical access to a maintainer's machine.
- Reports that depend on a non-default, undocumented, or insecure configuration that contradicts the documented setup.

## Related Documentation

- [`docs/security/`](docs/security/) — threat models, audit notes, hardening guides.
- [`docs/security/threat-model.md`](docs/security/threat-model.md) — primary threat model.
- [`docs/security/production-hardening.md`](docs/security/production-hardening.md) — production posture.
- [`docs/security/SECRET_MANAGEMENT.md`](docs/security/SECRET_MANAGEMENT.md) — secret handling.

## A Note on Maturity

ICN is still pre-pilot. Some surfaces are mature and hardened (transport, signed envelopes, replay protection); others are explicitly research-grade. The [`docs/security/`](docs/security/) directory is candid about which is which. A report against an early surface is still useful — it informs prioritization even when the answer is "we know, that surface is not yet supposed to be production."
