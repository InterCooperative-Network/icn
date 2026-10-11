# Start contributing to ICN

ICN is shared infrastructure for cooperatives, communities, and federations. It is active, experimental, and uneven in maturity. You do **not** need to understand the whole system, know Rust, or finish a course to make a useful contribution.

**The shortest path:** choose one task below, do it, and share the result. If you get stuck, that can be a useful result too.

For the project's purpose, read [What is ICN](https://intercooperative.network/what-is-icn). For current capabilities and limitations, check [What's Real Now](https://intercooperative.network/whats-real-now); a design document does not prove a feature is operational.

## 1. Pick a first contribution

| If you're interested in... | Try this first | Share it as... |
| --- | --- | --- |
| **Testing and onboarding** | Follow this guide from a clean checkout. Report the first unclear, missing, or broken step, with enough detail to reproduce it. | [Discussion](https://github.com/InterCooperative-Network/icn/discussions) or a scoped issue |
| **Writing and documentation** | Choose one sentence or instruction on the site or in these docs. Check it against the code or current project-status evidence, then propose a clearer and more accurate version. | Small documentation PR or Discussion |
| **UI, UX, and accessibility** | Review one public or member-facing flow. Record what a person cannot find, understand, or do, including device and accessibility details where relevant. | Annotated report or design Discussion |
| **Cooperative organizing and governance** | Describe one real coordination problem: who is involved, who has authority, what decision or commitment must be made, and what record must survive afterward. | [Discussion](https://github.com/InterCooperative-Network/icn/discussions) |
| **Code** | Build and test one crate, then choose a bounded issue or reproduce one small defect before changing anything. | Reproduction, test, or focused PR |

The [Get Involved page](https://intercooperative.network/get-involved) describes additional routes. You can also ask in the [ICN Discord](https://discord.gg/sAEFCnEahn) without a GitHub account. Tell us what you are interested in and what kind of task you would like; you do not need a proposal ready.

**Where to start if you have no task in mind:** use the onboarding test immediately below. It exists whether or not the [good-first-issue search](https://github.com/InterCooperative-Network/icn/issues?q=is%3Aissue+is%3Aopen+label%3Agood-first-issue) has something appropriate today. Check any issue's current status and scope before starting it.

## 2. First task: test the onboarding

**Goal:** find one specific improvement that would make the next newcomer's experience easier. This task does not require Rust or a full build.

1. Read this page and [the repository README](../README.md) as someone new to ICN.
2. Choose one entry point: documentation, a public-facing page, the technical setup below, or a real cooperative use case.
3. Try that step. Note what you expected, what happened, and where you got stuck or had to guess.
4. Share the result in [GitHub Discussions](https://github.com/InterCooperative-Network/icn/discussions), here on Discord, or as a focused issue/PR where appropriate.

A useful report can be as simple as:

```text
Starting point: [link or file path]
Goal: [what I was trying to do]
Environment: [OS, shell, relevant tool versions; omit if not applicable]
Steps I tried: [...]
Expected: [...]
Observed: [...]
Suggested fix or open question: [...]
```

**Done when:** another contributor could repeat what you tried or act on your suggestion. A successful setup confirmation is useful too; include your environment and which steps you actually verified. Don't post passphrases, tokens, private keys, or sensitive logs.

For a reproducible defect, open a [GitHub issue](https://github.com/InterCooperative-Network/icn/issues) after checking for duplicates. The repository's issue forms require project classification; if you are unsure which area owns the problem, start with a Discussion or Discord rather than guessing labels.

## 3. Technical path: get a development environment working

This section is for people who want to work on Rust, networking, identity, APIs, testing, or the CLI. **You do not need to complete it to contribute documentation, designs, research, or institutional examples.**

### Prerequisites

- Git and basic terminal familiarity.
- [rustup](https://rustup.rs/) to manage Rust. Use the version pinned in [`icn/rust-toolchain.toml`](../icn/rust-toolchain.toml), not an arbitrary latest version.
- A supported development shell appropriate to your OS. Linux is the most direct path; on Windows, WSL2 is a practical option. If setup fails on another environment, report the exact platform.
- Sufficient disk space and memory for Rust dependencies and build artifacts. This is a large workspace; start with a focused crate instead of assuming you must build everything.

For system dependencies and optional tooling, inspect [`scripts/bootstrap.sh`](../scripts/bootstrap.sh) before running it. It can install packages and download dependencies. Its `--ci` option skips optional Cargo development tools; `--no-sysdeps` skips system package installation. The script automatically installs system packages only on Debian/Ubuntu-like systems.

### Clone and run one focused test

```bash
git clone https://github.com/InterCooperative-Network/icn.git
cd icn

# Confirm your starting checkout is clean
git status --short

# Rust workspace is nested under icn/, not at repository root
cd icn
rustup show active-toolchain
cargo test -p icn-kernel-api --lib
```

This tests the `icn-kernel-api` crate, **not** the whole system. It may still download and compile transitive dependencies. If it fails, capture the command, OS, relevant tool versions, and the first useful error. Don't silently change the pinned toolchain or disable checks.

If you want to work with the CLI and daemon binaries, the next bounded step is:

```bash
cargo build -p icnctl -p icnd
./target/debug/icnctl --help
./target/debug/icnd --help
```

These commands build and inspect local tools; they do **not** demonstrate that a multi-node deployment, federation, or member-facing workflow is ready. Running and configuring a node is a separate exercise.

For deeper exploration, see the [optional developer curriculum](onboarding/README.md) and the [architecture overview](ARCHITECTURE.md).

## 4. Turn the result into a contribution

**For a small fix:**

1. Read [CONTRIBUTING.md](../CONTRIBUTING.md) for scope, security, and architectural expectations. For nontrivial code work, also follow [AGENTS.md](../AGENTS.md) and the relevant domain owner.
2. Confirm the issue still exists, and keep the change to one reviewable problem.
3. Work on a branch such as `docs/clarify-first-build`, `fix/your-bug`, or `test/your-case`. Fork the repo first if you do not have push access.
4. Run verification appropriate to **the files you changed**. For documentation, start at the repository root with:

   ```bash
   git diff --check
   python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml
   ```

   For Rust, choose the focused crate checks and relevant [AGENTS.md](../AGENTS.md) validation rather than treating a single passing test as evidence for the whole project.
5. Open a pull request against `main`. Explain **what** changed, **why**, **risk or limits**, and **what you verified**. Link the issue or Discussion if there is one.

**For observations or ideas:** a Discussion is a valid contribution. Describe the problem, the people affected, what you tried or observed, and what would need to change. Nobody needs to write code just to be heard.

If a task is larger than advertised or you cannot tell which part of ICN owns it, say so before widening the work. We can split it into something manageable.

## 5. Where to go next

- **Find another task:** [open issues](https://github.com/InterCooperative-Network/icn/issues) or [GitHub Discussions](https://github.com/InterCooperative-Network/icn/discussions). The good-first-issue list is not a promise that a suitable issue is always available.
- **Understand the structure:** [repository README](../README.md), [Ecosystem Atlas](ATLAS.md), and [Architecture](ARCHITECTURE.md).
- **Study the internals systematically:** [developer onboarding curriculum](onboarding/README.md). Its Reader/Fixer/Builder/Maintainer exercises are optional learning paths, **not requirements for contributing or permissions to work on ICN**.
- **Bring an institutional problem:** [For Cooperatives](https://intercooperative.network/for-cooperatives), then describe the use case in [Discussions](https://github.com/InterCooperative-Network/icn/discussions).
- **Ask for help informally:** [Discord](https://discord.gg/sAEFCnEahn).

The goal of this guide is not to make everyone learn the entire project first. It is to help someone take a useful step, make that work visible, and find another step if they want one.
