#!/usr/bin/env python3
"""Generate (or --check) a role-based, claim-disciplined inventory of `icnctl` commands.

Mechanically parses the `clap` derive command tree under `icn/bins/icnctl/src/**`
(`#[derive(Subcommand)] enum …`, plus the top-level `#[derive(Parser)]` -> `Commands`
enum) and emits the full command-path list with source file:line. Issue #2113.

What is mechanical (drift-checked) vs curated:
- **Mechanical:** the set of commands, their subcommand paths, and source file:line —
  derived from the clap enums. Counts never hand-maintained.
- **Curated (needs review):** the `role` column is a small, explicit top-level-group ->
  role map (a navigation heuristic, NOT derived from clap and asserting no usability or
  safety). The `status` column is a curated, evidence-pointer classification
  (`STATUS_BY_COMMAND`, issue #2113): live / partial / fixture-demo / planned, defaulting
  to `unknown / needs local verification` for any command not explicitly classified. A
  static scan proves a command is **declared**, not how far its handler is implemented, so
  status is curated from source/test evidence — never mechanically inferred from clap, and
  `live` is **never** asserted merely because a subcommand exists.
- **Proof level** is capped at **L1** (a command declaration exists in source) per
  `proof-level-taxonomy-capability-matrix.md`. A static scan cannot assert higher; the
  `status` column carries the implementation classification separately from proof level
  (a `live` status is NOT a higher proof level and NOT a production-readiness claim).

Usage:
    python3 docs/scripts/icnctl_command_inventory.py --write    # regenerate artifact
    python3 docs/scripts/icnctl_command_inventory.py --check     # exit 1 if stale
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


def repo_root() -> Path:
    try:
        top = subprocess.check_output(
            ["git", "rev-parse", "--show-toplevel"],
            cwd=Path(__file__).resolve().parent,
            text=True,
        ).strip()
        return Path(top)
    except Exception:
        return Path(__file__).resolve().parents[2]


ROOT = repo_root()
ICNCTL_SRC = ROOT / "icn" / "bins" / "icnctl" / "src"
ARTIFACT = ROOT / "docs" / "reference" / "project-index" / "generated" / "icnctl-command-inventory.md"
SCRIPT_REL = "docs/scripts/icnctl_command_inventory.py"

# The top-level clap command enum (the `#[command(subcommand)] command: Commands` field
# of the `#[derive(Parser)]` struct).
TOP_ENUM = "Commands"

# Curated, top-level-group -> role navigation heuristic (issue #2113). This is NOT
# mechanically derived and asserts nothing about usability, safety, or readiness — it is
# a "where would I look first" aid, and every entry is needs-review. Uncurated groups
# fall back to `unknown`. Roles use the #2113 vocabulary: organizer / operator /
# developer / agent / maintainer / unknown.
ROLE_BY_GROUP: dict[str, str] = {
    "status": "operator",
    "id": "developer",
    "device": "developer",
    "recovery": "developer",
    "trust": "operator",
    "ledger": "developer",
    "dispute": "organizer",
    "contract": "developer",
    "network": "operator",
    "federation": "operator",
    "gov": "organizer",
    "institution": "organizer",
    "backup": "operator",
    "restore": "operator",
    "verify-backup": "operator",
    "snapshot": "operator",
    "init-coop": "organizer",
    "coop": "maintainer",
    "auth": "developer",
    "compute": "developer",
    "policy": "operator",
    "quota": "operator",
    "steward": "operator",
    "commons": "organizer",
    "charter": "organizer",
    "amendment": "organizer",
    "appeal": "organizer",
    "api": "developer",
    "receipts": "developer",
    "audit": "operator",
    "registry": "developer",
    "preflight": "operator",
    "completions": "developer",
}

UNIFORM_STATUS = "unknown / needs local verification"
PROOF_LEVEL = "L1"

# Implementation-status vocabulary (issue #2113 acceptance criterion).
STATUS_LIVE = "live"
STATUS_PARTIAL = "partial"
STATUS_FIXTURE_DEMO = "fixture-demo"
STATUS_PLANNED = "planned"
STATUS_ORDER = [STATUS_LIVE, STATUS_PARTIAL, STATUS_FIXTURE_DEMO, STATUS_PLANNED, UNIFORM_STATUS]

# Curated implementation-status classification (issue #2113). Like ROLE_BY_GROUP this is
# NOT mechanically derivable from the clap tree: a static scan proves a command is
# *declared*, not how far its handler is implemented. Each entry is keyed by the EXACT
# command path (as rendered, without the leading `icnctl `) and carries (status, basis),
# where basis is a concise source/test evidence pointer. Every key is validated against the
# default-build command set at generation time (a renamed/removed command fails generation
# loudly), so the classification cannot silently rot. Any command NOT listed here defaults
# to `unknown / needs local verification` — an honest "not yet verified", not a failure.
#
# Discipline (issue #2113 "do not present demo/dev-gated as live"):
# - `live` is asserted ONLY with concrete-handler evidence AND either an integration test
#   that spawns the binary and asserts success, or a local-only operation with no network
#   dependency. It is never inferred from a clap declaration and is NOT a production claim.
# - `partial` = the handler does real work but depends on incomplete/unproven runtime
#   (e.g. a live gateway) and is not integration-tested end-to-end.
# - `planned` = the handler is an explicit placeholder / prints "not yet implemented".
# - `fixture-demo` = a demo/fixture/rehearsal-only path. (None today: `icnctl` has no
#   demo-only commands; the Summit Ops demo fixtures live in `web/pilot-ui`, not the CLI.)
# This is a deliberately NARROW, defensible first pass; most commands remain `unknown`.
STATUS_BY_COMMAND: dict[str, tuple[str, str]] = {
    # live — concrete handler + integration test (binary spawned, success asserted) or a
    # local-only, no-network operation.
    "id init": (STATUS_LIVE, "local keystore init; integration-tested (`icn/bins/icnctl/tests/backup_restore_test.rs`, `icn/bins/icnctl/tests/qr_code_test.rs` spawn the binary, assert success)"),
    "id show": (STATUS_LIVE, "local keystore read; integration-tested (`icn/bins/icnctl/tests/backup_restore_test.rs`, `icn/bins/icnctl/tests/qr_code_test.rs`)"),
    "backup": (STATUS_LIVE, "local data-dir backup; integration-tested (`icn/bins/icnctl/tests/backup_restore_test.rs` asserts success + tarball created)"),
    "restore": (STATUS_LIVE, "local data-dir restore; integration-tested (`icn/bins/icnctl/tests/backup_restore_test.rs`)"),
    "verify-backup": (STATUS_LIVE, "local backup verification; integration-tested (`icn/bins/icnctl/tests/backup_restore_test.rs`)"),
    "coop entity-report": (STATUS_LIVE, "read-only local coop-store report; integration-tested (`icn/bins/icnctl/tests/coop_entity_report_test.rs`, `icn/bins/icnctl/tests/coop_entity_backfill_test.rs` assert success + JSON)"),
    "coop entity-backfill-surrogates": (STATUS_LIVE, "local coop-store surrogate backfill; integration-tested (`icn/bins/icnctl/tests/coop_entity_backfill_test.rs` asserts success)"),
    "device add": (STATUS_LIVE, "local keystore device add; integration-tested (`icn/bins/icnctl/tests/qr_code_test.rs` asserts success)"),
    "completions": (STATUS_LIVE, "shell-completion generation via `clap_complete::generate`; local-only, no network (`icn/bins/icnctl/src/main.rs` Commands::Completions)"),
    "api export-openapi": (STATUS_LIVE, "serializes the embedded `icn_gateway::openapi::ApiDoc` to file/stdout; local-only, no gateway (`icn/bins/icnctl/src/main.rs` ApiCommands::ExportOpenapi)"),
    # live (pass 4, issue #2113) — local-only `id` / `device` / `snapshot` ops. Each handler
    # (handle_id_command / handle_device_command / handle_snapshot_command) is a SYNCHRONOUS fn
    # that receives only `data_dir` (no endpoint) and contains zero network markers
    # (reqwest / create_rpc_client / RpcClient), so it is structurally local-only with no
    # network dependency — the published `live` criterion. Same local keystore/snapshot
    # machinery as the integration-tested `id init`/`id show`/`device add`.
    "id rotate": (STATUS_LIVE, "local keystore rotation `AgeKeyStore::rotate` in `handle_id_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` IdCommands::Rotate)"),
    "id export": (STATUS_LIVE, "local passphrase-gated keystore export via `AgeKeyStore` in `handle_id_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` IdCommands::Export)"),
    "id import": (STATUS_LIVE, "local keystore import via `AgeKeyStore` in `handle_id_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` IdCommands::Import)"),
    "device list": (STATUS_LIVE, "local keystore device listing (`AgeKeyStore::get_did_document`/`get_device_id`) in `handle_device_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` DeviceCommands::List)"),
    "device approve": (STATUS_LIVE, "local keystore device-approval from a request file via `AgeKeyStore` in `handle_device_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` DeviceCommands::Approve)"),
    "device revoke": (STATUS_LIVE, "local keystore device revocation via `AgeKeyStore` in `handle_device_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` DeviceCommands::Revoke)"),
    "snapshot create": (STATUS_LIVE, "local snapshot creation via `icn_snapshot` on the store dir in `handle_snapshot_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` SnapshotCommands::Create)"),
    "snapshot list": (STATUS_LIVE, "local snapshot listing via `icn_snapshot` in `handle_snapshot_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` SnapshotCommands::List)"),
    "snapshot verify": (STATUS_LIVE, "local snapshot integrity check `icn_snapshot::verify_snapshot`/`verify_timestamped_snapshot` in `handle_snapshot_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` SnapshotCommands::Verify)"),
    "snapshot delete": (STATUS_LIVE, "local snapshot deletion (`std::fs::remove_file`) in `handle_snapshot_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` SnapshotCommands::Delete)"),
    "snapshot cleanup": (STATUS_LIVE, "local snapshot cleanup `icn_snapshot::cleanup_old_snapshots` in `handle_snapshot_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` SnapshotCommands::Cleanup)"),
    # live (pass 5, issue #2113) — local-only `contract` build/sign helpers. `handle_contract_prepare`
    # and `handle_contract_sign` are SYNCHRONOUS fns taking only file paths + data_dir (no client);
    # they read a contract/deployment file, sign with the local keystore, and `std::fs::write` the
    # output — no network/await (the RpcClient constructed in handle_contract_command is never used
    # by these arms).
    "contract prepare": (STATUS_LIVE, "local contract-deployment preparation `handle_contract_prepare` (reads contract JSON, signs with keystore, `std::fs::write`s the deployment file); sync, no client/network (`icn/bins/icnctl/src/main.rs` ContractCommands::Prepare)"),
    "contract sign": (STATUS_LIVE, "local deployment-file co-signing `handle_contract_sign` (reads deployment file, signs with keystore, `std::fs::write`s output); sync, no client/network (`icn/bins/icnctl/src/main.rs` ContractCommands::Sign)"),
    # live (pass 6, issue #2113) — local-only `commons` identity readouts (read the keystore, print
    # status/guidance; no network) and the local `institution bootstrap` validate/plan helpers
    # (`validate_package` / `build_plan_report` are synchronous `pub fn`s — structurally no async/network).
    "commons status": (STATUS_LIVE, "local Commons-holder status: reads the keystore via `AgeKeyStore` and prints identity/enrollment status; no network (`icn/bins/icnctl/src/main.rs` CommonsCommands::Status)"),
    "commons enroll": (STATUS_LIVE, "local: reads the keystore for the DID and prints enrollment guidance (the actual enrollment is the out-of-band steward/in-person flow); no network (`icn/bins/icnctl/src/main.rs` CommonsCommands::Enroll)"),
    "institution bootstrap validate": (STATUS_LIVE, "local package validation `validate_package` (sync `pub fn`, reads the package dir, no network) (`icn/bins/icnctl/src/institution_bootstrap.rs` InstitutionCommands::Bootstrap -> InstitutionBootstrapCommands::Validate)"),
    "institution bootstrap plan": (STATUS_LIVE, "local bootstrap plan report `build_plan_report` (sync `pub fn`, no network) (`icn/bins/icnctl/src/institution_bootstrap.rs` InstitutionCommands::Bootstrap -> InstitutionBootstrapCommands::Plan)"),
    # partial — real work, but depends on incomplete/unproven runtime; not integration-tested end-to-end.
    "audit verify": (STATUS_PARTIAL, "concrete gateway client `GET /v1/receipts/chain/{hash}` (`icn/bins/icnctl/src/main.rs` AuditCommands::Verify); chain-verification algorithm covered by `icn/bins/icnctl/tests/audit_verify_test.rs` (inlined copy); end-to-end against a live gateway not integration-tested"),
    "preflight": (STATUS_PARTIAL, "runs real local health checks (data-dir, keystore open via `AgeKeyStore`) in `handle_preflight_command`; the gateway-connectivity check requires a running gateway; not integration-tested"),
    # partial (pass 2, issue #2113) — `status` + `receipts` gateway/daemon-client group: each
    # handler makes a real client call to a route present in generated/route-inventory.md, but
    # depends on a running daemon/gateway and has no binary-spawning e2e test.
    "status": (STATUS_PARTIAL, "RPC client to the daemon (`create_rpc_client` + `client.get_status()`) in `handle_status_command` (`icn/bins/icnctl/src/main.rs` Commands::Status); prints local config and fails gracefully when no daemon; requires a running daemon, not integration-tested"),
    "receipts chain": (STATUS_PARTIAL, "concrete gateway client `reqwest GET {gateway}/v1/receipts/chain/{decision_hash}` (route in `docs/reference/project-index/generated/route-inventory.md`) (`icn/bins/icnctl/src/main.rs` ReceiptCommands::Chain); requires a running gateway, not integration-tested"),
    "receipts allocation": (STATUS_PARTIAL, "concrete gateway client `reqwest GET {gateway}/v1/receipts/allocations/{hash}` (route in `docs/reference/project-index/generated/route-inventory.md`) (`icn/bins/icnctl/src/main.rs` ReceiptCommands::Allocation); requires a running gateway, not integration-tested"),
    "receipts intent": (STATUS_PARTIAL, "concrete gateway client `reqwest GET {gateway}/v1/receipts/intents/{hash}` (route in `docs/reference/project-index/generated/route-inventory.md`) (`icn/bins/icnctl/src/main.rs` ReceiptCommands::Intent); requires a running gateway, not integration-tested"),
    # partial (pass 3, issue #2113) — `network` + `quota` (daemon RPC clients via
    # create_rpc_client / create_authenticated_rpc_client) and `registry` (gateway HTTP
    # client) groups: each handler makes a real client call but depends on a running
    # daemon/gateway and has no binary-spawning e2e test.
    "network peers": (STATUS_PARTIAL, "daemon RPC client `client.get_peers()` via `create_rpc_client` in `handle_network_command` (`icn/bins/icnctl/src/main.rs` NetworkCommands::Peers); requires a running daemon, not integration-tested"),
    "network dial": (STATUS_PARTIAL, "daemon RPC client `client.dial(did, addr)` via `create_rpc_client` (`icn/bins/icnctl/src/main.rs` NetworkCommands::Dial); requires a running daemon, not integration-tested"),
    "network stats": (STATUS_PARTIAL, "daemon RPC client `client.get_stats()` via `create_rpc_client` (`icn/bins/icnctl/src/main.rs` NetworkCommands::Stats); requires a running daemon, not integration-tested"),
    "network status": (STATUS_PARTIAL, "daemon RPC client `client.get_status()` then optionally `client.get_nat_status()` via `create_rpc_client` (`icn/bins/icnctl/src/main.rs` NetworkCommands::Status); requires a running daemon, not integration-tested"),
    "quota show": (STATUS_PARTIAL, "authenticated daemon RPC `client.call(\"quota.usage\", ...)` via `create_authenticated_rpc_client` in `handle_quota_command` (`icn/bins/icnctl/src/main.rs` QuotaCommands::Show); requires a running daemon, not integration-tested"),
    "quota list": (STATUS_PARTIAL, "authenticated daemon RPC `client.call(\"quota.list\", ...)` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` QuotaCommands::List); requires a running daemon, not integration-tested"),
    "registry list": (STATUS_PARTIAL, "concrete gateway client `reqwest GET {gateway}/v1/registry/decisions` (route in `docs/reference/project-index/generated/route-inventory.md`) (`icn/bins/icnctl/src/main.rs` RegistryCommands::List); requires a running gateway, not integration-tested"),
    "registry trace": (STATUS_PARTIAL, "concrete gateway client `reqwest GET {gateway}/v1/registry/decisions/{receipt_id}/trace` (route in `docs/reference/project-index/generated/route-inventory.md`) (`icn/bins/icnctl/src/main.rs` RegistryCommands::Trace); requires a running gateway, not integration-tested"),
    # partial (pass 4, issue #2113) — `auth token` does local keystore work (open/unlock/sign)
    # then a gateway challenge-response, so it depends on a running gateway.
    "auth token": (STATUS_PARTIAL, "gateway auth client: signs a keystore challenge then `reqwest POST {gateway}/v1/auth/challenge` + `/v1/auth/verify` (routes in `docs/reference/project-index/generated/route-inventory.md`) in `handle_auth_command` (`icn/bins/icnctl/src/main.rs` AuthCommands::Token); requires a running gateway, not integration-tested"),
    # partial (pass 5, issue #2113) — `trust` + `compute` (authenticated daemon RPC clients) and
    # the daemon-client `contract` arms. Each handler makes a real RPC call but depends on a
    # running daemon and has no binary-spawning e2e test.
    "trust add": (STATUS_PARTIAL, "daemon RPC client `client.add_trust()` via `create_authenticated_rpc_client` in `handle_trust_command` (`icn/bins/icnctl/src/main.rs` TrustCommands::Add); \"Is icnd running?\"; requires a running daemon, not integration-tested"),
    "trust list": (STATUS_PARTIAL, "daemon RPC client `client.list_trust()` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` TrustCommands::List); requires a running daemon, not integration-tested"),
    "trust show": (STATUS_PARTIAL, "daemon RPC client (compute-trust-score call) via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` TrustCommands::Show); \"Is icnd running?\"; requires a running daemon, not integration-tested"),
    "trust remove": (STATUS_PARTIAL, "daemon RPC client `client.remove_trust()` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` TrustCommands::Remove); requires a running daemon, not integration-tested"),
    "compute submit": (STATUS_PARTIAL, "authenticated daemon RPC `client.call(\"compute.submit\", ...)` via `create_authenticated_rpc_client` in `handle_compute_command` (`icn/bins/icnctl/src/main.rs` ComputeCommands::Submit); requires a running daemon, not integration-tested"),
    "compute submit-wasm": (STATUS_PARTIAL, "authenticated daemon RPC `client.call(\"compute.submit\", ...)` (reads a local wasm file) via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` ComputeCommands::SubmitWasm); requires a running daemon, not integration-tested"),
    "compute status": (STATUS_PARTIAL, "authenticated daemon RPC `client.call(\"compute.status\", ...)` (`icn/bins/icnctl/src/main.rs` ComputeCommands::Status); requires a running daemon, not integration-tested"),
    "compute cancel": (STATUS_PARTIAL, "authenticated daemon RPC `client.call(\"compute.cancel\", ...)` (`icn/bins/icnctl/src/main.rs` ComputeCommands::Cancel); requires a running daemon, not integration-tested"),
    "contract deploy": (STATUS_PARTIAL, "signs the deployment locally then daemon RPC `client.deploy_contract()` (`icn_rpc::RpcClient`) in `handle_contract_command` (`icn/bins/icnctl/src/main.rs` ContractCommands::Deploy); \"Is icnd running?\"; requires a running daemon, not integration-tested"),
    "contract deploy-signed": (STATUS_PARTIAL, "daemon RPC deploy of a pre-signed deployment file via `handle_contract_deploy_signed(.., &mut client)` (`icn/bins/icnctl/src/main.rs` ContractCommands::DeploySigned); requires a running daemon, not integration-tested"),
    "contract call": (STATUS_PARTIAL, "daemon RPC `client.call_contract()` (`icn/bins/icnctl/src/main.rs` ContractCommands::Call); \"Is icnd running?\"; requires a running daemon, not integration-tested"),
    "contract list": (STATUS_PARTIAL, "daemon RPC `client.list_contracts()` (`icn/bins/icnctl/src/main.rs` ContractCommands::List); requires a running daemon, not integration-tested"),
    # partial (pass 6, issue #2113) — `dispute` daemon RPC clients, the `commons anchor` gateway
    # client, and `institution bootstrap apply` (async gateway client). Each makes a real client
    # call but depends on a running daemon/gateway and has no binary-spawning e2e test.
    "dispute file": (STATUS_PARTIAL, "daemon RPC `client.dispute_file()` (`icn_rpc::RpcClient`) (`icn/bins/icnctl/src/main.rs` DisputeCommands::File); \"Is icnd running?\"; requires a running daemon, not integration-tested"),
    "dispute list": (STATUS_PARTIAL, "daemon RPC `client.dispute_list()` (`icn/bins/icnctl/src/main.rs` DisputeCommands::List); requires a running daemon, not integration-tested"),
    "dispute get": (STATUS_PARTIAL, "daemon RPC `client.dispute_get()` (`icn/bins/icnctl/src/main.rs` DisputeCommands::Get); requires a running daemon, not integration-tested"),
    "dispute add-evidence": (STATUS_PARTIAL, "daemon RPC `client.dispute_add_evidence()` (`icn/bins/icnctl/src/main.rs` DisputeCommands::AddEvidence); requires a running daemon, not integration-tested"),
    "dispute assign-mediator": (STATUS_PARTIAL, "daemon RPC `client.dispute_assign_mediator()` (`icn/bins/icnctl/src/main.rs` DisputeCommands::AssignMediator); requires a running daemon, not integration-tested"),
    "dispute resolve": (STATUS_PARTIAL, "daemon RPC `client.dispute_resolve()` (`icn/bins/icnctl/src/main.rs` DisputeCommands::Resolve); requires a running daemon, not integration-tested"),
    "commons anchor": (STATUS_PARTIAL, "gateway client `reqwest GET {gateway}/v1/commons/anchor/by-did/{did}` (gateway from `ICN_GATEWAY` env) (`icn/bins/icnctl/src/main.rs` CommonsCommands::Anchor); requires a running gateway, not integration-tested"),
    "institution bootstrap apply": (STATUS_PARTIAL, "async `apply_package` posts the package to the gateway via `reqwest::Client` (`icn/bins/icnctl/src/institution_bootstrap.rs` InstitutionCommands::Bootstrap -> InstitutionBootstrapCommands::Apply); requires a running gateway, not integration-tested"),
    # planned — handler is an explicit placeholder / prints "not yet implemented".
    "charter deploy": (STATUS_PLANNED, "handler validates the CCL doc locally, then prints \"Not yet implemented — charter deployment requires gateway integration\" (`icn/bins/icnctl/src/main.rs` CharterCommands::Deploy)"),
    "steward check-vui": (STATUS_PLANNED, "placeholder; validates input then prints \"VUI registry check requires running steward daemon\" (`icn/bins/icnctl/src/main.rs` StewardCommands::CheckVui)"),
    "steward start-enrollment": (STATUS_PLANNED, "placeholder for the full SDIS enrollment flow; requires a running steward daemon (`icn/bins/icnctl/src/main.rs` StewardCommands::StartEnrollment)"),
    "steward enrollment-status": (STATUS_PLANNED, "placeholder; ceremony status check requires a running steward daemon (`icn/bins/icnctl/src/main.rs` StewardCommands::EnrollmentStatus)"),
    "steward start-recovery": (STATUS_PLANNED, "placeholder for the full SDIS recovery flow; requires a running steward daemon (`icn/bins/icnctl/src/main.rs` StewardCommands::StartRecovery)"),
    "steward recovery-status": (STATUS_PLANNED, "placeholder; ceremony status check requires a running steward daemon (`icn/bins/icnctl/src/main.rs` StewardCommands::RecoveryStatus)"),
    "steward issue-token": (STATUS_PLANNED, "placeholder for the full SDIS token issuance flow; requires a running steward daemon (`icn/bins/icnctl/src/main.rs` StewardCommands::IssueToken)"),
    # planned (pass 6, issue #2113) — `commons` membership ops that print
    # "this feature is pending gateway API implementation" (no client call yet).
    "commons affiliations": (STATUS_PLANNED, "prints \"Affiliation lookup requires gateway integration. This feature is pending gateway API implementation.\" — no client call (`icn/bins/icnctl/src/main.rs` CommonsCommands::Affiliations)"),
    "commons join": (STATUS_PLANNED, "prints \"Join request requires gateway integration. This feature is pending gateway API implementation.\" — no client call (`icn/bins/icnctl/src/main.rs` CommonsCommands::Join)"),
    "commons leave": (STATUS_PLANNED, "prints \"Leave request requires gateway integration. This feature is pending gateway API implementation.\" — no client call (`icn/bins/icnctl/src/main.rs` CommonsCommands::Leave)"),
}

CFG_RE = re.compile(r"^\s*#\[cfg\((.+)\)\]\s*$")
ENUM_RE = re.compile(r"#\[derive\([^)]*\bSubcommand\b[^)]*\)\]")
ENUM_DECL_RE = re.compile(r"^\s*(?:pub\s+)?enum\s+([A-Za-z_]\w*)\s*\{")
# A variant at enum top level: `Name`, `Name,`, `Name {`, or `Name(Type)`. Group 3 is the
# tuple-delegation type (if any); group 4 distinguishes a struct variant (`{`).
VARIANT_RE = re.compile(r"^    ([A-Z][A-Za-z0-9]*)\s*(\(([^)]*)\))?\s*(\{|,|$)")
SUBCMD_ATTR = "#[command(subcommand)]"
FIELD_TYPE_RE = re.compile(r"^\s*(?:pub\s+)?\w+\s*:\s*([A-Za-z_][\w:]*)")


def camel_to_kebab(name: str) -> str:
    s = re.sub(r"(?<!^)(?=[A-Z])", "-", name)
    return s.lower()


def struct_variant_delegation(lines: list[str], start: int) -> str | None:
    """A struct-style variant `Name { … }` delegates to a sub-enum when its body has a
    `#[command(subcommand)]` attribute on a field, e.g.
    `Bootstrap { #[command(subcommand)] command: InstitutionBootstrapCommands }`.
    Returns the field's type (final `::` segment), or None. `start` is the variant line
    (which carries the opening `{`)."""
    depth = 0
    want = False
    n = len(lines)
    k = start
    while k < n:
        line = lines[k]
        if SUBCMD_ATTR in line:
            want = True
        elif want and line.strip():
            fm = FIELD_TYPE_RE.match(line)
            if fm:
                return fm.group(1).split("::")[-1]
            if not line.strip().startswith("#"):
                want = False  # not a subcommand field after all
        depth += line.count("{") - line.count("}")
        if depth <= 0 and k > start:
            return None
        k += 1
    return None


def parse_enums(text: str, rel: str) -> dict[str, list[dict]]:
    """Parse every `#[derive(...Subcommand...)] enum X { … }`. Returns
    {enum_name: [{variant, delegates_to|None, line, file}]}. Brace-depth tracked so nested
    braces in variant bodies don't end the enum early; struct variants are inspected for a
    nested `#[command(subcommand)]` field delegation."""
    out: dict[str, list[dict]] = {}
    lines = text.splitlines()
    i = 0
    n = len(lines)
    while i < n:
        if ENUM_RE.search(lines[i]):
            # Find the `enum X {` within the next few lines (skip other attrs).
            j = i + 1
            enum_name = None
            while j < min(i + 6, n):
                dm = ENUM_DECL_RE.match(lines[j])
                if dm:
                    enum_name = dm.group(1)
                    break
                j += 1
            if enum_name is None:
                i += 1
                continue
            variants: list[dict] = []
            depth = 0
            started = False
            pending_cfg: str | None = None  # a `#[cfg(...)]` seen since the last variant
            k = j
            while k < n:
                line = lines[k]
                if not started:
                    depth += line.count("{")
                    started = depth > 0
                    depth -= line.count("}")
                    k += 1
                    continue
                # Only treat a line as a variant when at the enum's own brace depth (1).
                if depth == 1:
                    cm = CFG_RE.match(line)
                    if cm:
                        # `#[cfg(...)]` on a variant (e.g. `#[cfg(feature = "post-quantum")]`)
                        # means the command only exists when that cfg is active. Attach it to
                        # the next variant. (Field-level cfgs live at depth 2 and are ignored.)
                        pending_cfg = cm.group(1).strip()
                    else:
                        vm = VARIANT_RE.match(line)
                        if vm:
                            delegates = None
                            if vm.group(3):
                                # `Name(Type)` — strip module path, take the final segment.
                                delegates = vm.group(3).strip().split("::")[-1]
                            elif vm.group(4) == "{":
                                # Struct variant — may delegate via a nested subcommand field.
                                delegates = struct_variant_delegation(lines, k)
                            variants.append(
                                {"variant": vm.group(1), "delegates_to": delegates,
                                 "line": k + 1, "file": rel, "cfg": pending_cfg}
                            )
                            pending_cfg = None
                depth += line.count("{") - line.count("}")
                if depth <= 0:
                    break
                k += 1
            out[enum_name] = variants
            i = k + 1
            continue
        i += 1
    return out


def build_tree() -> tuple[list[dict], list[str]]:
    """Return (leaf_commands, unparsed_notes). Leaf = a variant that does not delegate
    to a parsed Subcommand enum. Path = kebab(ancestors) joined by space."""
    enums: dict[str, list[dict]] = {}
    if not ICNCTL_SRC.is_dir():
        return [], [f"icnctl src not found at {ICNCTL_SRC}"]
    for rs in sorted(ICNCTL_SRC.rglob("*.rs")):
        rel = rs.relative_to(ROOT).as_posix()
        parsed = parse_enums(rs.read_text(encoding="utf-8", errors="replace"), rel)
        for name, variants in parsed.items():
            if name == "__files__":
                continue
            enums[name] = variants

    leaves: list[dict] = []
    unparsed: list[str] = []
    if TOP_ENUM not in enums:
        return [], [f"top-level `{TOP_ENUM}` enum not found among parsed enums"]

    seen_enums: set[str] = set()

    def walk(enum_name: str, prefix: list[str], cfg: str | None) -> None:
        if enum_name in seen_enums:  # cycle guard (none expected)
            return
        seen_enums.add(enum_name)
        for v in enums[enum_name]:
            kebab = camel_to_kebab(v["variant"])
            path = prefix + [kebab]
            # A command is feature-gated if it OR any ancestor group is cfg-gated.
            vcfg = v.get("cfg")
            combined = " && ".join(c for c in (cfg, vcfg) if c) or None
            dele = v["delegates_to"]
            if dele and dele in enums:
                walk(dele, path, combined)
            else:
                if dele and dele not in enums:
                    # Tuple variant whose type is not a parsed Subcommand enum —
                    # treat as a leaf command but note the unresolved delegation.
                    unparsed.append(
                        f"`{' '.join(path)}` -> unresolved subcommand type `{dele}` "
                        f"(`{v['file']}`:{v['line']})"
                    )
                leaves.append(
                    {
                        "path": " ".join(path),
                        "group": path[0],
                        "file": v["file"],
                        "line": v["line"],
                        "cfg": combined,
                    }
                )
        seen_enums.discard(enum_name)

    walk(TOP_ENUM, [], None)
    leaves.sort(key=lambda x: x["path"])
    return leaves, unparsed


def md_escape(s: str) -> str:
    return s.replace("|", "\\|")


def render(leaves: list[dict], unparsed: list[str], commit: str) -> str:
    # A command guarded by `#[cfg(feature = …)]` is NOT present in the default build
    # (`icnctl` Cargo.toml has `default = []`). Count those separately so the default-build
    # surface and totals are not inflated (issue #2113 review).
    default_cmds = [c for c in leaves if not c.get("cfg")]
    gated_cmds = [c for c in leaves if c.get("cfg")]

    # Anti-drift: every curated status key must match a real default-build command path,
    # else a rename/removal would silently drop the classification. Fail generation loudly.
    default_paths = {c["path"] for c in default_cmds}
    stale_keys = sorted(k for k in STATUS_BY_COMMAND if k not in default_paths)
    if stale_keys:
        raise ValueError(
            "STATUS_BY_COMMAND keys not found among default-build commands "
            f"(renamed/removed?): {stale_keys}"
        )
    # Validate values too, so a typo'd status or empty basis fails explicitly here rather
    # than later during rendering/sorting. Curated entries must use one of the non-default
    # statuses and carry a non-empty evidence basis.
    valid_statuses = {STATUS_LIVE, STATUS_PARTIAL, STATUS_FIXTURE_DEMO, STATUS_PLANNED}
    bad_values = sorted(
        f"{k} (status={status!r}, basis={'empty' if not basis.strip() else 'ok'})"
        for k, (status, basis) in STATUS_BY_COMMAND.items()
        if status not in valid_statuses or not basis.strip()
    )
    if bad_values:
        raise ValueError(
            f"STATUS_BY_COMMAND values invalid (status must be one of {sorted(valid_statuses)} "
            f"and basis must be non-empty): {bad_values}"
        )

    by_role: dict[str, list[dict]] = {}
    role_counts: dict[str, int] = {}
    status_counts: dict[str, int] = {}
    classified: list[dict] = []
    group_set = set()
    for c in default_cmds:
        role = ROLE_BY_GROUP.get(c["group"], "unknown")
        c["role"] = role
        status, basis = STATUS_BY_COMMAND.get(c["path"], (UNIFORM_STATUS, ""))
        c["status"] = status
        c["status_basis"] = basis
        if status != UNIFORM_STATUS:
            classified.append(c)
        by_role.setdefault(role, []).append(c)
        role_counts[role] = role_counts.get(role, 0) + 1
        status_counts[status] = status_counts.get(status, 0) + 1
        group_set.add(c["group"])
    for c in gated_cmds:
        c["role"] = ROLE_BY_GROUP.get(c["group"], "unknown")
        # Feature-gated commands stay unclassified (and excluded from default-build counts).
        c["status"] = UNIFORM_STATUS
        c["status_basis"] = ""
        group_set.add(c["group"])

    now = datetime.now(timezone.utc).replace(microsecond=0).isoformat()
    total = len(default_cmds)
    role_order = ["organizer", "operator", "developer", "agent", "maintainer", "unknown"]

    o: list[str] = []
    o.append("---")
    o.append("Status: generated")
    o.append("Canonical: no")
    o.append(f"Generated: {now}")
    o.append("---")
    o.append("")
    o.append("# `icnctl` Command Inventory (generated)")
    o.append("")
    o.append(f"> Generated mechanically by [`{SCRIPT_REL}`](../../../scripts/icnctl_command_inventory.py). "
             "**Do not hand-edit** — rerun the script.")
    o.append(f"> Regenerate: `python3 {SCRIPT_REL} --write`  ·  Check drift: `python3 {SCRIPT_REL} --check`  ·  Issue [#2113](https://github.com/InterCooperative-Network/icn/issues/2113)")
    o.append("")
    o.append("## What this proves / does not prove")
    o.append("")
    o.append("- **Proves:** these `icnctl` command declarations exist in the clap command tree "
             "under `icn/bins/icnctl/src/**` at the snapshot commit (proof level **L1**).")
    o.append("- **Does NOT prove:** that a command works, is safe for organizers, is "
             "production-ready, is wired to a live gateway, has correct auth/permissions, is "
             "part of a supported pilot flow, or is appropriate for non-technical users.")
    o.append("- **`role` is a curated navigation heuristic** (top-level command group -> role), "
             "**not** mechanically derived from clap and **needs review**. It says \"where might I "
             "look first\", never \"this user may safely run this\".")
    o.append("- **`status` is a curated, evidence-pointer classification** (issue #2113): "
             "`live` / `partial` / `fixture-demo` / `planned`, defaulting to "
             f"`{UNIFORM_STATUS}` for any command not explicitly classified. It is curated "
             "from source/test evidence (a static clap scan proves a command is *declared*, "
             "not how far its handler is implemented), so it is **never** inferred from a "
             "declaration. `live` is asserted only with concrete-handler + (integration "
             "test | local-only no-network) evidence and is **not** a production-readiness "
             "claim; demo/dev-gated commands are never presented as live. See the "
             "[Implementation status](#implementation-status-classification) section.")
    o.append("- Defer to canonical truth/precedence: [`source-of-truth-map.md`](../source-of-truth-map.md) "
             "and proof levels in [`proof-level-taxonomy-capability-matrix.md`](../proof-level-taxonomy-capability-matrix.md). "
             "Orientation artifact (`Canonical: no`); companion to [`generated/route-inventory.md`](route-inventory.md).")
    o.append("")
    o.append("## Snapshot")
    o.append("")
    o.append(f"- Source commit: `{commit}`")
    o.append("- Source scanned: `icn/bins/icnctl/src/**` (clap `#[derive(Subcommand)]` / `#[derive(Parser)]` tree)")
    o.append("")
    o.append("## Summary")
    o.append("")
    o.append(f"- **Total leaf commands (default build): {total}**")
    o.append(f"- Top-level command groups: {len(group_set)}")
    o.append("- By role (curated, needs review): "
             + " · ".join(f"{r} {role_counts.get(r, 0)}" for r in role_order if role_counts.get(r, 0)))
    o.append("- By status (curated, see section below): "
             + " · ".join(f"{s} {status_counts.get(s, 0)}" for s in STATUS_ORDER if status_counts.get(s, 0)))
    o.append(f"- Proof level: every command is `{PROOF_LEVEL}` (declaration exists in source).")
    o.append(f"- **Feature-gated commands (NOT in the default build, excluded from the counts "
             f"above): {len(gated_cmds)}** (see section below).")
    o.append(f"- Unparsed / unresolved candidates: {len(unparsed)} (see section below).")
    o.append("")
    o.append("## Commands by role")
    o.append("")
    o.append("Role is the **curated** top-level-group heuristic (needs review). `status` is a "
             "**curated, per-command** classification (see the "
             "[Implementation status](#implementation-status-classification) section); `proof` "
             "is uniform at `L1` (declaration scan).")
    o.append("")
    for role in role_order:
        cmds = sorted(by_role.get(role, []), key=lambda x: x["path"])
        if not cmds:
            continue
        o.append(f"### {role} ({len(cmds)})")
        o.append("")
        o.append("| Command | Status | Proof | Source |")
        o.append("|---|---|---|---|")
        for c in cmds:
            o.append(f"| `icnctl {md_escape(c['path'])}` | {md_escape(c['status'])} | {PROOF_LEVEL} | "
                     f"`{c['file']}`:{c['line']} |")
        o.append("")

    o.append("## Feature-gated commands (not in the default build)")
    o.append("")
    o.append("These commands are guarded by a Cargo `#[cfg(feature = …)]` and are **absent from "
             "the default `icnctl` build** (`icn/bins/icnctl/Cargo.toml` has `default = []`). They "
             "are **excluded** from the counts and role tables above and only exist when the named "
             "feature is enabled at build time.")
    o.append("")
    if gated_cmds:
        o.append("| Command | Required cfg | Role (curated) | Proof | Source |")
        o.append("|---|---|---|---|---|")
        for c in sorted(gated_cmds, key=lambda x: x["path"]):
            o.append(f"| `icnctl {md_escape(c['path'])}` | `cfg({md_escape(c['cfg'])})` | "
                     f"{c['role']} | {PROOF_LEVEL} | `{c['file']}`:{c['line']} |")
        o.append("")
    else:
        o.append("- None: every discovered command is present in the default build.")
        o.append("")

    o.append("## Implementation status classification")
    o.append("")
    o.append("Status is a **curated** classification (issue #2113), defaulting to "
             f"`{UNIFORM_STATUS}`. It is curated from source/test evidence — a static clap "
             "scan proves a command is *declared*, not how far its handler is implemented — "
             "so it is never inferred from a declaration. This is a deliberately **narrow, "
             "defensible first pass**: only commands with clear evidence are classified; the "
             "rest stay `unknown` (honest, not a failure).")
    o.append("")
    o.append("### Vocabulary")
    o.append("")
    o.append("- **`live`** — compiled in the default build, calls a concrete client/runtime "
             "path, and has evidence it works: an integration test that spawns the binary and "
             "asserts success, OR a local-only operation with no network dependency. **Not** "
             "asserted because a clap subcommand exists; **not** a production-readiness claim.")
    o.append("- **`partial`** — the handler does real work but depends on incomplete/unproven "
             "runtime support (e.g. a live gateway) and is not integration-tested end-to-end.")
    o.append("- **`fixture-demo`** — a demo / fixture / rehearsal-only path, exercisable "
             "without live network/service; must not be implied as live operational use.")
    o.append("- **`planned`** — declared but the handler is a placeholder / TODO / prints "
             "\"not yet implemented\".")
    o.append(f"- **`{UNIFORM_STATUS}`** — status not established from source/tests/docs in "
             "this pass; the conservative default.")
    o.append("")
    o.append("### Counts by status (default build)")
    o.append("")
    o.append("| Status | Count |")
    o.append("|---|---|")
    for s in STATUS_ORDER:
        o.append(f"| {s} | {status_counts.get(s, 0)} |")
    o.append("")
    o.append(f"### Classified commands ({len(classified)})")
    o.append("")
    o.append("Every non-`unknown` command, with its evidence basis. (All other default-build "
             f"commands carry `{UNIFORM_STATUS}`.)")
    o.append("")
    if classified:
        o.append("| Command | Status | Basis (source/test evidence) | Source |")
        o.append("|---|---|---|---|")
        for c in sorted(classified, key=lambda x: (STATUS_ORDER.index(x["status"]), x["path"])):
            o.append(f"| `icnctl {md_escape(c['path'])}` | {md_escape(c['status'])} | "
                     f"{md_escape(c['status_basis'])} | `{c['file']}`:{c['line']} |")
        o.append("")
    else:
        o.append(f"- None classified yet — every command carries `{UNIFORM_STATUS}`.")
        o.append("")
    o.append("### Status non-claims")
    o.append("")
    o.append("- A command marked `live` is **not** a production-readiness claim — only that "
             "it runs and does real work per the cited local/test evidence.")
    o.append("- This generated static inventory is **not** runtime execution proof except "
             "where a basis cites a test; proof level stays `L1` (declaration) regardless of status.")
    o.append(f"- A default `{UNIFORM_STATUS}` is an honest \"not yet verified\", not a failure "
             "or a defect.")
    o.append("- Feature-gated commands remain **excluded** from the default-build counts and "
             "are left unclassified.")
    o.append("")

    o.append("## Unparsed / unknown candidates")
    o.append("")
    if unparsed:
        o.append("Variants whose subcommand type could not be resolved to a parsed clap enum "
                 "(listed as leads, recorded as leaf commands above with a caveat):")
        o.append("")
        for u in sorted(unparsed):
            o.append(f"- {u}")
        o.append("")
    else:
        o.append("- None: every top-level variant resolved to either a leaf command or a parsed "
                 "`#[derive(Subcommand)]` enum.")
        o.append("")

    o.append("## Safe vs unsafe claims (examples)")
    o.append("")
    o.append("- ✅ Safe: \"`icnctl api export-openapi` is declared in the CLI at this commit (L1).\"")
    o.append(f"- ✅ Safe: \"`icnctl` declares {total} commands across these groups; role grouping is a "
             "curated navigation aid pending review.\"")
    o.append("- ❌ Unsafe: \"`icnctl gov …` is organizer-ready\" / \"this command is live in "
             "production\" / \"this command is safe for non-technical organizers\" — none of "
             "that is established by a declaration scan.")
    o.append("")
    return "\n".join(o)


def strip_volatile(text: str) -> str:
    keep = [ln for ln in text.splitlines()
            if not ln.startswith("Generated:") and not ln.startswith("- Source commit:")]
    return "\n".join(keep)


def main() -> int:
    ap = argparse.ArgumentParser(description="Generate or check the icnctl command inventory.")
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--write", action="store_true", help="regenerate the inventory artifact")
    g.add_argument("--check", action="store_true", help="exit 1 if the committed artifact is stale")
    args = ap.parse_args()

    try:
        commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    except Exception:
        commit = "unknown"

    leaves, unparsed = build_tree()
    if not leaves:
        # Surface the diagnostic build_tree() produced (missing src dir / missing top-level
        # enum) instead of a generic guess, so failures point at the real cause.
        detail = "; ".join(unparsed) if unparsed else f"no clap commands found under {ICNCTL_SRC}"
        print(f"ERROR: icnctl command inventory could not be built: {detail}", file=sys.stderr)
        return 2
    content = render(leaves, unparsed, commit)
    gated = sum(1 for c in leaves if c.get("cfg"))
    counts = f"{len(leaves) - gated} default-build + {gated} feature-gated, {len(unparsed)} unparsed"

    if args.write:
        ARTIFACT.parent.mkdir(parents=True, exist_ok=True)
        ARTIFACT.write_text(content + "\n", encoding="utf-8")
        print(f"wrote {ARTIFACT.relative_to(ROOT)}: {counts}")
        return 0

    if not ARTIFACT.is_file():
        print(f"STALE: {ARTIFACT.relative_to(ROOT)} does not exist — run --write", file=sys.stderr)
        return 1
    committed = ARTIFACT.read_text(encoding="utf-8")
    if strip_volatile(committed).strip() == strip_volatile(content).strip():
        print(f"OK: icnctl command inventory up to date ({counts})")
        return 0
    print(f"STALE: {ARTIFACT.relative_to(ROOT)} differs from a fresh scan — run --write and commit",
          file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
