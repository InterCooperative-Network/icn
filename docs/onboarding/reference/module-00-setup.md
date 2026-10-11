# Setup and troubleshooting reference

This is a **supporting reference**, not a second quickstart or a promise that every environment works identically.

**Start with [Getting Started](../../GETTING_STARTED.md#3-technical-path-get-a-development-environment-working).** It owns the newcomer workflow and a focused first test. This page helps you diagnose failures without copying stale toolchain versions or setup commands into another place.

## What you need for Rust work

- Git, a terminal, and [rustup](https://rustup.rs/).
- The toolchain specified in [`icn/rust-toolchain.toml`](../../../icn/rust-toolchain.toml). **Do not switch to a newer or older Rust version just to silence a failure.**
- System libraries and build tools needed by the crate you are working on.
- Disk space for downloaded Rust dependencies and incremental compilation. The full workspace is large.

Linux is the most direct environment; WSL2 is a practical Windows development route. Other environments may need different system packages. If a platform is unverified, document what happens instead of calling it supported by assumption.

### Check your starting environment

From the cloned repository:

```bash
git status --short
cd icn
rustup show active-toolchain
cargo --version
```

The second `icn/` is the Rust workspace inside the repository. Cargo commands run there.

For the smallest documented initial Rust exercise, follow the focused crate test in [Getting Started](../../GETTING_STARTED.md#3-technical-path-get-a-development-environment-working). You do not need a whole-workspace build to report an onboarding defect.

## Optional bootstrap helper

The repo contains [`scripts/bootstrap.sh`](../../../scripts/bootstrap.sh), which fetches Rust dependencies and can install system packages, Node.js dependencies, and optional development tools. **Read the script before running it**: it modifies the local development environment.

From the repository root, if you choose to use it:

```bash
# Skips extra Cargo developer-tool installation
./scripts/bootstrap.sh --ci

# Also skips automatic system-package installation
./scripts/bootstrap.sh --ci --no-sysdeps
```

System-package auto-installation is written for Debian/Ubuntu-like systems; other platforms need their own dependency setup. The `--no-sysdeps` flag does **not** mean the script is read-only: it still performs other setup actions.

Node.js and npm are relevant to SDK and website paths, not required merely to read docs or open a design Discussion.

## Diagnosing a failure

### Toolchain not found or wrong version

Install rustup, enter the `icn/` workspace, and check `rustup show active-toolchain`. Confirm it agrees with `icn/rust-toolchain.toml`. If automatic toolchain installation fails, preserve the error and investigate your rustup/network state; don't silently change the pin.

### Linker, compiler, or system library missing

Capture the **first** missing executable or library named by Cargo. On Linux, common dependencies include a C toolchain, `pkg-config`, OpenSSL headers, and Protocol Buffers tools. See `scripts/bootstrap.sh` for the repository's current dependency setup and install the equivalents appropriate to your OS.

### A test fails

Record whether the failure is compilation, test assertion, permissions, networking, timeout, or an external service. Re-run the smallest failing test you can identify, without modifying implementation or weakening checks merely to make it pass. Include the exact command and error in your report.

### Port already in use

Find which process owns the port and decide whether it is safe to stop **your own** test process or choose a different test environment. Do not run bulk kill commands copied from old tutorials; they may terminate unrelated services.

### Identity, passphrase, or node startup fails

CLI and daemon experiments are separate from a first contributor build. Use isolated development data, read the current CLI help and applicable operations docs, and do not paste credentials, private keys, signing material, or secrets into GitHub or Discord. A running process alone is not evidence of federation or production readiness.

### Build uses too many resources

Prefer a targeted package/test rather than `cargo build --release` or `cargo test --workspace` as a default first step. Record available memory, disk space, and the command that exhausted resources. Release builds can demand *more* time and space; they are not a general cache fix.

## Report useful evidence

For reproducible setup problems, share:

```text
OS and version:
Shell / environment (including WSL2 if applicable):
Git revision (git rev-parse --short HEAD):
Rust toolchain (rustup show active-toolchain):
Command:
Expected result:
Actual result / first relevant error:
What I already tried:
```

Use [GitHub Discussions](https://github.com/InterCooperative-Network/icn/discussions) or [Discord](https://discord.gg/sAEFCnEahn) when you are unsure where a problem belongs. For a scoped reproducible defect, check for an existing issue before creating one. For a correction PR, see [CONTRIBUTING.md](../../../CONTRIBUTING.md) and [AGENTS.md](../../../AGENTS.md).

**Next:** return to [Getting Started](../../GETTING_STARTED.md) and complete one small contribution. More extensive learning is available in [the onboarding curriculum](../README.md).
