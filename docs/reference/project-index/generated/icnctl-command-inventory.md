---
Status: generated
Canonical: no
Generated: 2026-06-26T20:17:59+00:00
---

# `icnctl` Command Inventory (generated)

> Generated mechanically by [`docs/scripts/icnctl_command_inventory.py`](../../../scripts/icnctl_command_inventory.py). **Do not hand-edit** — rerun the script.
> Regenerate: `python3 docs/scripts/icnctl_command_inventory.py --write`  ·  Check drift: `python3 docs/scripts/icnctl_command_inventory.py --check`  ·  Issue [#2113](https://github.com/InterCooperative-Network/icn/issues/2113)

## What this proves / does not prove

- **Proves:** these `icnctl` command declarations exist in the clap command tree under `icn/bins/icnctl/src/**` at the snapshot commit (proof level **L1**).
- **Does NOT prove:** that a command works, is safe for organizers, is production-ready, is wired to a live gateway, has correct auth/permissions, is part of a supported pilot flow, or is appropriate for non-technical users.
- **`role` is a curated navigation heuristic** (top-level command group -> role), **not** mechanically derived from clap and **needs review**. It says "where might I look first", never "this user may safely run this".
- **`status` is a curated, evidence-pointer classification** (issue #2113): `live` / `partial` / `fixture-demo` / `planned`, defaulting to `unknown / needs local verification` for any command not explicitly classified. It is curated from source/test evidence (a static clap scan proves a command is *declared*, not how far its handler is implemented), so it is **never** inferred from a declaration. `live` is asserted only with concrete-handler + (integration test | local-only no-network) evidence and is **not** a production-readiness claim; demo/dev-gated commands are never presented as live. See the [Implementation status](#implementation-status-classification) section.
- Defer to canonical truth/precedence: [`source-of-truth-map.md`](../source-of-truth-map.md) and proof levels in [`proof-level-taxonomy-capability-matrix.md`](../proof-level-taxonomy-capability-matrix.md). Orientation artifact (`Canonical: no`); companion to [`generated/route-inventory.md`](route-inventory.md).

## Snapshot

- Source commit: `a3cdd5ac46917504962209af17f117909542df2e`
- Source scanned: `icn/bins/icnctl/src/**` (clap `#[derive(Subcommand)]` / `#[derive(Parser)]` tree)

## Summary

- **Total leaf commands (default build): 162**
- Top-level command groups: 33
- By role (curated, needs review): organizer 53 · operator 64 · developer 43 · maintainer 2
- By status (curated, see section below): live 38 · partial 103 · planned 13 · unknown / needs local verification 8
- Proof level: every command is `L1` (declaration exists in source).
- **Feature-gated commands (NOT in the default build, excluded from the counts above): 1** (see section below).
- Unparsed / unresolved candidates: 0 (see section below).

## Commands by role

Role is the **curated** top-level-group heuristic (needs review). `status` is a **curated, per-command** classification (see the [Implementation status](#implementation-status-classification) section); `proof` is uniform at `L1` (declaration scan).

### organizer (53)

| Command | Status | Proof | Source |
|---|---|---|---|
| `icnctl amendment add-change` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1867 |
| `icnctl amendment list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1766 |
| `icnctl amendment open-voting` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1809 |
| `icnctl amendment propose` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1731 |
| `icnctl amendment show` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1785 |
| `icnctl amendment submit` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1795 |
| `icnctl amendment vote` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1823 |
| `icnctl amendment withdraw` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1849 |
| `icnctl appeal add-evidence` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1972 |
| `icnctl appeal file` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1904 |
| `icnctl appeal list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1943 |
| `icnctl appeal respond` | partial | L1 | `icn/bins/icnctl/src/main.rs`:2002 |
| `icnctl appeal show` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1962 |
| `icnctl appeal withdraw` | partial | L1 | `icn/bins/icnctl/src/main.rs`:2024 |
| `icnctl charter create` | live | L1 | `icn/bins/icnctl/src/main.rs`:1614 |
| `icnctl charter deploy` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1718 |
| `icnctl charter inspect` | live | L1 | `icn/bins/icnctl/src/main.rs`:1704 |
| `icnctl charter list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1651 |
| `icnctl charter ratify` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1684 |
| `icnctl charter show` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1641 |
| `icnctl charter sign` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1666 |
| `icnctl charter validate` | live | L1 | `icn/bins/icnctl/src/main.rs`:1698 |
| `icnctl commons affiliations` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1582 |
| `icnctl commons anchor` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1575 |
| `icnctl commons enroll` | live | L1 | `icn/bins/icnctl/src/main.rs`:1564 |
| `icnctl commons join` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1589 |
| `icnctl commons leave` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1600 |
| `icnctl commons status` | live | L1 | `icn/bins/icnctl/src/main.rs`:1561 |
| `icnctl dispute add-evidence` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1394 |
| `icnctl dispute assign-mediator` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1405 |
| `icnctl dispute file` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1366 |
| `icnctl dispute get` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1388 |
| `icnctl dispute list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1377 |
| `icnctl dispute resolve` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1416 |
| `icnctl gov domain add-member` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1168 |
| `icnctl gov domain create` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1127 |
| `icnctl gov domain list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1165 |
| `icnctl gov domain show` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1158 |
| `icnctl gov proposal cancel` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1278 |
| `icnctl gov proposal close` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1271 |
| `icnctl gov proposal create` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1191 |
| `icnctl gov proposal list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1253 |
| `icnctl gov proposal open` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1242 |
| `icnctl gov proposal show` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1264 |
| `icnctl gov vote cast` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1288 |
| `icnctl gov vote delegate` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1310 |
| `icnctl gov vote delegations` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1325 |
| `icnctl gov vote revoke` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1328 |
| `icnctl gov vote show` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1303 |
| `icnctl init-coop` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:128 |
| `icnctl institution bootstrap apply` | partial | L1 | `icn/bins/icnctl/src/institution_bootstrap.rs`:49 |
| `icnctl institution bootstrap plan` | live | L1 | `icn/bins/icnctl/src/institution_bootstrap.rs`:38 |
| `icnctl institution bootstrap validate` | live | L1 | `icn/bins/icnctl/src/institution_bootstrap.rs`:27 |

### operator (64)

| Command | Status | Proof | Source |
|---|---|---|---|
| `icnctl audit verify` | partial | L1 | `icn/bins/icnctl/src/main.rs`:330 |
| `icnctl backup` | live | L1 | `icn/bins/icnctl/src/main.rs`:98 |
| `icnctl federation add` | live | L1 | `icn/bins/icnctl/src/main.rs`:862 |
| `icnctl federation attestation from` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1022 |
| `icnctl federation attestation issue` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1028 |
| `icnctl federation attestation list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1016 |
| `icnctl federation clearing create` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1059 |
| `icnctl federation clearing list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1050 |
| `icnctl federation clearing position` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1097 |
| `icnctl federation clearing rate` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1078 |
| `icnctl federation clearing settle` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1103 |
| `icnctl federation clearing show` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1053 |
| `icnctl federation config` | live | L1 | `icn/bins/icnctl/src/main.rs`:884 |
| `icnctl federation connect` | partial | L1 | `icn/bins/icnctl/src/main.rs`:878 |
| `icnctl federation coop list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:944 |
| `icnctl federation coop register` | partial | L1 | `icn/bins/icnctl/src/main.rs`:953 |
| `icnctl federation coop show` | partial | L1 | `icn/bins/icnctl/src/main.rs`:947 |
| `icnctl federation coop update` | partial | L1 | `icn/bins/icnctl/src/main.rs`:972 |
| `icnctl federation gateway-connect` | partial | L1 | `icn/bins/icnctl/src/main.rs`:899 |
| `icnctl federation invite` | partial | L1 | `icn/bins/icnctl/src/main.rs`:896 |
| `icnctl federation peers` | partial | L1 | `icn/bins/icnctl/src/main.rs`:859 |
| `icnctl federation remove` | live | L1 | `icn/bins/icnctl/src/main.rs`:872 |
| `icnctl federation set` | live | L1 | `icn/bins/icnctl/src/main.rs`:887 |
| `icnctl federation status` | partial | L1 | `icn/bins/icnctl/src/main.rs`:856 |
| `icnctl federation vouch issue` | partial | L1 | `icn/bins/icnctl/src/main.rs`:989 |
| `icnctl federation vouch list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1004 |
| `icnctl federation vouch revoke` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1007 |
| `icnctl network dial` | partial | L1 | `icn/bins/icnctl/src/main.rs`:837 |
| `icnctl network peers` | partial | L1 | `icn/bins/icnctl/src/main.rs`:834 |
| `icnctl network stats` | partial | L1 | `icn/bins/icnctl/src/main.rs`:847 |
| `icnctl network status` | partial | L1 | `icn/bins/icnctl/src/main.rs`:850 |
| `icnctl policy list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:531 |
| `icnctl policy remove` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:534 |
| `icnctl policy set` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:513 |
| `icnctl policy show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:524 |
| `icnctl preflight` | partial | L1 | `icn/bins/icnctl/src/main.rs`:203 |
| `icnctl quota list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:555 |
| `icnctl quota show` | partial | L1 | `icn/bins/icnctl/src/main.rs`:544 |
| `icnctl restore` | live | L1 | `icn/bins/icnctl/src/main.rs`:104 |
| `icnctl snapshot cleanup` | live | L1 | `icn/bins/icnctl/src/main.rs`:1356 |
| `icnctl snapshot create` | live | L1 | `icn/bins/icnctl/src/main.rs`:1338 |
| `icnctl snapshot delete` | live | L1 | `icn/bins/icnctl/src/main.rs`:1350 |
| `icnctl snapshot list` | live | L1 | `icn/bins/icnctl/src/main.rs`:1341 |
| `icnctl snapshot verify` | live | L1 | `icn/bins/icnctl/src/main.rs`:1344 |
| `icnctl status` | partial | L1 | `icn/bins/icnctl/src/main.rs`:51 |
| `icnctl steward attesters` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1458 |
| `icnctl steward check-vui` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1496 |
| `icnctl steward config` | live | L1 | `icn/bins/icnctl/src/main.rs`:1433 |
| `icnctl steward enrollment-status` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1513 |
| `icnctl steward info` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1436 |
| `icnctl steward issue-token` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1544 |
| `icnctl steward list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1445 |
| `icnctl steward recovery-status` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1538 |
| `icnctl steward register` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1465 |
| `icnctl steward retire` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1487 |
| `icnctl steward start-enrollment` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1502 |
| `icnctl steward start-recovery` | planned | L1 | `icn/bins/icnctl/src/main.rs`:1519 |
| `icnctl steward status` | partial | L1 | `icn/bins/icnctl/src/main.rs`:1430 |
| `icnctl steward topics` | live | L1 | `icn/bins/icnctl/src/main.rs`:1555 |
| `icnctl trust add` | partial | L1 | `icn/bins/icnctl/src/main.rs`:694 |
| `icnctl trust list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:707 |
| `icnctl trust remove` | partial | L1 | `icn/bins/icnctl/src/main.rs`:716 |
| `icnctl trust show` | partial | L1 | `icn/bins/icnctl/src/main.rs`:710 |
| `icnctl verify-backup` | live | L1 | `icn/bins/icnctl/src/main.rs`:114 |

### developer (43)

| Command | Status | Proof | Source |
|---|---|---|---|
| `icnctl api export-openapi` | live | L1 | `icn/bins/icnctl/src/main.rs`:267 |
| `icnctl auth token` | partial | L1 | `icn/bins/icnctl/src/main.rs`:410 |
| `icnctl completions` | live | L1 | `icn/bins/icnctl/src/main.rs`:214 |
| `icnctl compute cancel` | partial | L1 | `icn/bins/icnctl/src/main.rs`:500 |
| `icnctl compute status` | partial | L1 | `icn/bins/icnctl/src/main.rs`:494 |
| `icnctl compute submit` | partial | L1 | `icn/bins/icnctl/src/main.rs`:432 |
| `icnctl compute submit-wasm` | partial | L1 | `icn/bins/icnctl/src/main.rs`:463 |
| `icnctl contract call` | partial | L1 | `icn/bins/icnctl/src/main.rs`:812 |
| `icnctl contract deploy` | partial | L1 | `icn/bins/icnctl/src/main.rs`:779 |
| `icnctl contract deploy-signed` | partial | L1 | `icn/bins/icnctl/src/main.rs`:806 |
| `icnctl contract list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:828 |
| `icnctl contract prepare` | live | L1 | `icn/bins/icnctl/src/main.rs`:786 |
| `icnctl contract sign` | live | L1 | `icn/bins/icnctl/src/main.rs`:796 |
| `icnctl device add` | live | L1 | `icn/bins/icnctl/src/main.rs`:600 |
| `icnctl device approve` | live | L1 | `icn/bins/icnctl/src/main.rs`:614 |
| `icnctl device list` | live | L1 | `icn/bins/icnctl/src/main.rs`:597 |
| `icnctl device revoke` | live | L1 | `icn/bins/icnctl/src/main.rs`:620 |
| `icnctl id export` | live | L1 | `icn/bins/icnctl/src/main.rs`:582 |
| `icnctl id import` | live | L1 | `icn/bins/icnctl/src/main.rs`:588 |
| `icnctl id init` | live | L1 | `icn/bins/icnctl/src/main.rs`:565 |
| `icnctl id rotate` | live | L1 | `icn/bins/icnctl/src/main.rs`:571 |
| `icnctl id show` | live | L1 | `icn/bins/icnctl/src/main.rs`:568 |
| `icnctl ledger balance` | partial | L1 | `icn/bins/icnctl/src/main.rs`:728 |
| `icnctl ledger head` | partial | L1 | `icn/bins/icnctl/src/main.rs`:725 |
| `icnctl ledger history` | partial | L1 | `icn/bins/icnctl/src/main.rs`:738 |
| `icnctl ledger quarantine drop` | partial | L1 | `icn/bins/icnctl/src/main.rs`:767 |
| `icnctl ledger quarantine get` | partial | L1 | `icn/bins/icnctl/src/main.rs`:755 |
| `icnctl ledger quarantine list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:752 |
| `icnctl ledger quarantine purge` | partial | L1 | `icn/bins/icnctl/src/main.rs`:773 |
| `icnctl ledger quarantine release` | partial | L1 | `icn/bins/icnctl/src/main.rs`:761 |
| `icnctl receipts allocation` | partial | L1 | `icn/bins/icnctl/src/main.rs`:299 |
| `icnctl receipts chain` | partial | L1 | `icn/bins/icnctl/src/main.rs`:281 |
| `icnctl receipts intent` | partial | L1 | `icn/bins/icnctl/src/main.rs`:313 |
| `icnctl recovery attest` | partial | L1 | `icn/bins/icnctl/src/main.rs`:656 |
| `icnctl recovery cancel` | partial | L1 | `icn/bins/icnctl/src/main.rs`:681 |
| `icnctl recovery config` | live | L1 | `icn/bins/icnctl/src/main.rs`:647 |
| `icnctl recovery finalize` | partial | L1 | `icn/bins/icnctl/src/main.rs`:675 |
| `icnctl recovery initiate` | partial | L1 | `icn/bins/icnctl/src/main.rs`:650 |
| `icnctl recovery list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:666 |
| `icnctl recovery setup` | live | L1 | `icn/bins/icnctl/src/main.rs`:633 |
| `icnctl recovery status` | partial | L1 | `icn/bins/icnctl/src/main.rs`:669 |
| `icnctl registry list` | partial | L1 | `icn/bins/icnctl/src/main.rs`:366 |
| `icnctl registry trace` | partial | L1 | `icn/bins/icnctl/src/main.rs`:389 |

### maintainer (2)

| Command | Status | Proof | Source |
|---|---|---|---|
| `icnctl coop entity-backfill-surrogates` | live | L1 | `icn/bins/icnctl/src/main.rs`:249 |
| `icnctl coop entity-report` | live | L1 | `icn/bins/icnctl/src/main.rs`:230 |

## Feature-gated commands (not in the default build)

These commands are guarded by a Cargo `#[cfg(feature = …)]` and are **absent from the default `icnctl` build** (`icn/bins/icnctl/Cargo.toml` has `default = []`). They are **excluded** from the counts and role tables above and only exist when the named feature is enabled at build time.

| Command | Required cfg | Role (curated) | Proof | Source |
|---|---|---|---|---|
| `icnctl id upgrade-pq` | `cfg(feature = "post-quantum")` | developer | L1 | `icn/bins/icnctl/src/main.rs`:579 |

## Implementation status classification

Status is a **curated** classification (issue #2113), defaulting to `unknown / needs local verification`. It is curated from source/test evidence — a static clap scan proves a command is *declared*, not how far its handler is implemented — so it is never inferred from a declaration. This is a deliberately **narrow, defensible first pass**: only commands with clear evidence are classified; the rest stay `unknown` (honest, not a failure).

### Vocabulary

- **`live`** — compiled in the default build, calls a concrete client/runtime path, and has evidence it works: an integration test that spawns the binary and asserts success, OR a local-only operation with no network dependency. **Not** asserted because a clap subcommand exists; **not** a production-readiness claim.
- **`partial`** — the handler does real work but depends on incomplete/unproven runtime support (e.g. a live gateway) and is not integration-tested end-to-end.
- **`fixture-demo`** — a demo / fixture / rehearsal-only path, exercisable without live network/service; must not be implied as live operational use.
- **`planned`** — declared but the handler is a placeholder / TODO / prints "not yet implemented".
- **`unknown / needs local verification`** — status not established from source/tests/docs in this pass; the conservative default.

### Counts by status (default build)

| Status | Count |
|---|---|
| live | 38 |
| partial | 103 |
| fixture-demo | 0 |
| planned | 13 |
| unknown / needs local verification | 8 |

### Classified commands (154)

Every non-`unknown` command, with its evidence basis. (All other default-build commands carry `unknown / needs local verification`.)

| Command | Status | Basis (source/test evidence) | Source |
|---|---|---|---|
| `icnctl api export-openapi` | live | serializes the embedded `icn_gateway::openapi::ApiDoc` to file/stdout; local-only, no gateway (`icn/bins/icnctl/src/main.rs` ApiCommands::ExportOpenapi) | `icn/bins/icnctl/src/main.rs`:267 |
| `icnctl backup` | live | local data-dir backup; integration-tested (`icn/bins/icnctl/tests/backup_restore_test.rs` asserts success + tarball created) | `icn/bins/icnctl/src/main.rs`:98 |
| `icnctl charter create` | live | local charter-document construction; builds the charter and `std::fs::write`s it to a file; no client/network (`icn/bins/icnctl/src/main.rs` CharterCommands::Create) | `icn/bins/icnctl/src/main.rs`:1614 |
| `icnctl charter inspect` | live | local CCL parse+print `icn_ccl::schema::CclDocument::from_yaml`; no client/network (`icn/bins/icnctl/src/main.rs` CharterCommands::Inspect) | `icn/bins/icnctl/src/main.rs`:1704 |
| `icnctl charter validate` | live | local CCL validation `icn_ccl::schema::CclDocument::from_yaml` of a charter file; no client/network (`icn/bins/icnctl/src/main.rs` CharterCommands::Validate) | `icn/bins/icnctl/src/main.rs`:1698 |
| `icnctl commons enroll` | live | local: reads the keystore for the DID and prints enrollment guidance (the actual enrollment is the out-of-band steward/in-person flow); no network (`icn/bins/icnctl/src/main.rs` CommonsCommands::Enroll) | `icn/bins/icnctl/src/main.rs`:1564 |
| `icnctl commons status` | live | local Commons-holder status: reads the keystore via `AgeKeyStore` and prints identity/enrollment status; no network (`icn/bins/icnctl/src/main.rs` CommonsCommands::Status) | `icn/bins/icnctl/src/main.rs`:1561 |
| `icnctl completions` | live | shell-completion generation via `clap_complete::generate`; local-only, no network (`icn/bins/icnctl/src/main.rs` Commands::Completions) | `icn/bins/icnctl/src/main.rs`:214 |
| `icnctl contract prepare` | live | local contract-deployment preparation `handle_contract_prepare` (reads contract JSON, signs with keystore, `std::fs::write`s the deployment file); sync, no client/network (`icn/bins/icnctl/src/main.rs` ContractCommands::Prepare) | `icn/bins/icnctl/src/main.rs`:786 |
| `icnctl contract sign` | live | local deployment-file co-signing `handle_contract_sign` (reads deployment file, signs with keystore, `std::fs::write`s output); sync, no client/network (`icn/bins/icnctl/src/main.rs` ContractCommands::Sign) | `icn/bins/icnctl/src/main.rs`:796 |
| `icnctl coop entity-backfill-surrogates` | live | local coop-store surrogate backfill; integration-tested (`icn/bins/icnctl/tests/coop_entity_backfill_test.rs` asserts success) | `icn/bins/icnctl/src/main.rs`:249 |
| `icnctl coop entity-report` | live | read-only local coop-store report; integration-tested (`icn/bins/icnctl/tests/coop_entity_report_test.rs`, `icn/bins/icnctl/tests/coop_entity_backfill_test.rs` assert success + JSON) | `icn/bins/icnctl/src/main.rs`:230 |
| `icnctl device add` | live | local keystore device add; integration-tested (`icn/bins/icnctl/tests/qr_code_test.rs` asserts success) | `icn/bins/icnctl/src/main.rs`:600 |
| `icnctl device approve` | live | local keystore device-approval from a request file via `AgeKeyStore` in `handle_device_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` DeviceCommands::Approve) | `icn/bins/icnctl/src/main.rs`:614 |
| `icnctl device list` | live | local keystore device listing (`AgeKeyStore::get_did_document`/`get_device_id`) in `handle_device_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` DeviceCommands::List) | `icn/bins/icnctl/src/main.rs`:597 |
| `icnctl device revoke` | live | local keystore device revocation via `AgeKeyStore` in `handle_device_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` DeviceCommands::Revoke) | `icn/bins/icnctl/src/main.rs`:620 |
| `icnctl federation add` | live | local-only: validates the `icn://` URL then appends the peer to `network.bootstrap_peers` and `config.to_file(icn.toml)` — no client/network (`icn/bins/icnctl/src/main.rs` FederationCommands::Add) | `icn/bins/icnctl/src/main.rs`:862 |
| `icnctl federation config` | live | local-only: reads `icn.toml` and prints the `[federation]` config — no client/network (`icn/bins/icnctl/src/main.rs` FederationCommands::Config) | `icn/bins/icnctl/src/main.rs`:884 |
| `icnctl federation remove` | live | local-only: removes matching bootstrap peers from `icn.toml` and `config.to_file` — no client/network (`icn/bins/icnctl/src/main.rs` FederationCommands::Remove) | `icn/bins/icnctl/src/main.rs`:872 |
| `icnctl federation set` | live | local-only: sets a `federation.*` key in `icn.toml` and `config.to_file` — no client/network (`icn/bins/icnctl/src/main.rs` FederationCommands::Set) | `icn/bins/icnctl/src/main.rs`:887 |
| `icnctl id export` | live | local passphrase-gated keystore export via `AgeKeyStore` in `handle_id_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` IdCommands::Export) | `icn/bins/icnctl/src/main.rs`:582 |
| `icnctl id import` | live | local keystore import via `AgeKeyStore` in `handle_id_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` IdCommands::Import) | `icn/bins/icnctl/src/main.rs`:588 |
| `icnctl id init` | live | local keystore init; integration-tested (`icn/bins/icnctl/tests/backup_restore_test.rs`, `icn/bins/icnctl/tests/qr_code_test.rs` spawn the binary, assert success) | `icn/bins/icnctl/src/main.rs`:565 |
| `icnctl id rotate` | live | local keystore rotation `AgeKeyStore::rotate` in `handle_id_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` IdCommands::Rotate) | `icn/bins/icnctl/src/main.rs`:571 |
| `icnctl id show` | live | local keystore read; integration-tested (`icn/bins/icnctl/tests/backup_restore_test.rs`, `icn/bins/icnctl/tests/qr_code_test.rs`) | `icn/bins/icnctl/src/main.rs`:568 |
| `icnctl institution bootstrap plan` | live | local bootstrap plan report `build_plan_report` (sync `pub fn`, no network) (`icn/bins/icnctl/src/institution_bootstrap.rs` InstitutionCommands::Bootstrap -> InstitutionBootstrapCommands::Plan) | `icn/bins/icnctl/src/institution_bootstrap.rs`:38 |
| `icnctl institution bootstrap validate` | live | local package validation `validate_package` (sync `pub fn`, reads the package dir, no network) (`icn/bins/icnctl/src/institution_bootstrap.rs` InstitutionCommands::Bootstrap -> InstitutionBootstrapCommands::Validate) | `icn/bins/icnctl/src/institution_bootstrap.rs`:27 |
| `icnctl recovery config` | live | local keystore read: opens+unlocks `AgeKeyStore` and prints the DID document's recovery config; no client/network (`icn/bins/icnctl/src/main.rs` RecoveryCommands::Config) | `icn/bins/icnctl/src/main.rs`:647 |
| `icnctl recovery setup` | live | local keystore op: opens+unlocks `AgeKeyStore` and writes the social-recovery config into the DID document via `update_did_document`; no client/network (`icn/bins/icnctl/src/main.rs` RecoveryCommands::Setup) | `icn/bins/icnctl/src/main.rs`:633 |
| `icnctl restore` | live | local data-dir restore; integration-tested (`icn/bins/icnctl/tests/backup_restore_test.rs`) | `icn/bins/icnctl/src/main.rs`:104 |
| `icnctl snapshot cleanup` | live | local snapshot cleanup `icn_snapshot::cleanup_old_snapshots` in `handle_snapshot_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` SnapshotCommands::Cleanup) | `icn/bins/icnctl/src/main.rs`:1356 |
| `icnctl snapshot create` | live | local snapshot creation via `icn_snapshot` on the store dir in `handle_snapshot_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` SnapshotCommands::Create) | `icn/bins/icnctl/src/main.rs`:1338 |
| `icnctl snapshot delete` | live | local snapshot deletion (`std::fs::remove_file`) in `handle_snapshot_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` SnapshotCommands::Delete) | `icn/bins/icnctl/src/main.rs`:1350 |
| `icnctl snapshot list` | live | local snapshot listing via `icn_snapshot` in `handle_snapshot_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` SnapshotCommands::List) | `icn/bins/icnctl/src/main.rs`:1341 |
| `icnctl snapshot verify` | live | local snapshot integrity check `icn_snapshot::verify_snapshot`/`verify_timestamped_snapshot` in `handle_snapshot_command` (sync, no endpoint, no network) (`icn/bins/icnctl/src/main.rs` SnapshotCommands::Verify) | `icn/bins/icnctl/src/main.rs`:1344 |
| `icnctl steward config` | live | local read+print of the `[steward]` section of `data_dir/config.toml`; no client/network (`icn/bins/icnctl/src/main.rs` StewardCommands::Config) | `icn/bins/icnctl/src/main.rs`:1433 |
| `icnctl steward topics` | live | prints static steward gossip-topic constants (`icn_steward::topics::*`); local-only, no network (`icn/bins/icnctl/src/main.rs` StewardCommands::Topics) | `icn/bins/icnctl/src/main.rs`:1555 |
| `icnctl verify-backup` | live | local backup verification; integration-tested (`icn/bins/icnctl/tests/backup_restore_test.rs`) | `icn/bins/icnctl/src/main.rs`:114 |
| `icnctl amendment add-change` | partial | gateway client `reqwest POST {gateway}/v1/constitutional/amendments/{id}/changes` (route in route-inventory; auth via get_gateway_token) (`icn/bins/icnctl/src/main.rs` AmendmentCommands::AddChange); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1867 |
| `icnctl amendment list` | partial | gateway client `reqwest GET {gateway}/v1/constitutional/amendments` (route in route-inventory) (`icn/bins/icnctl/src/main.rs` AmendmentCommands::List); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1766 |
| `icnctl amendment open-voting` | partial | gateway client `reqwest POST {gateway}/v1/constitutional/amendments/{id}/vote` (route in route-inventory; auth via get_gateway_token) (`icn/bins/icnctl/src/main.rs` AmendmentCommands::OpenVoting); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1809 |
| `icnctl amendment propose` | partial | gateway client `reqwest POST {gateway}/v1/constitutional/amendments` (route in `docs/reference/project-index/generated/route-inventory.md`; auth via get_gateway_token) (`icn/bins/icnctl/src/main.rs` AmendmentCommands::Propose); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1731 |
| `icnctl amendment show` | partial | gateway client `reqwest GET {gateway}/v1/constitutional/amendments/{id}` (route in route-inventory) (`icn/bins/icnctl/src/main.rs` AmendmentCommands::Show); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1785 |
| `icnctl amendment submit` | partial | gateway client `reqwest POST {gateway}/v1/constitutional/amendments/{id}/submit` (route in route-inventory; auth via get_gateway_token) (`icn/bins/icnctl/src/main.rs` AmendmentCommands::Submit); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1795 |
| `icnctl amendment vote` | partial | gateway client `reqwest POST {gateway}/v1/constitutional/amendments/{id}/ratify` (route in route-inventory; auth via get_gateway_token) (`icn/bins/icnctl/src/main.rs` AmendmentCommands::Vote); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1823 |
| `icnctl amendment withdraw` | partial | gateway client `reqwest POST {gateway}/v1/constitutional/amendments/{id}/withdraw` (route in route-inventory; auth via get_gateway_token) (`icn/bins/icnctl/src/main.rs` AmendmentCommands::Withdraw); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1849 |
| `icnctl appeal add-evidence` | partial | gateway client `reqwest POST {gateway}/v1/constitutional/appeals/{id}/evidence` (route in route-inventory; auth via get_gateway_token) (`icn/bins/icnctl/src/main.rs` AppealCommands::AddEvidence); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1972 |
| `icnctl appeal file` | partial | gateway client `reqwest POST {gateway}/v1/constitutional/appeals` (route in route-inventory; auth via get_gateway_token) (`icn/bins/icnctl/src/main.rs` AppealCommands::File); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1904 |
| `icnctl appeal list` | partial | gateway client `reqwest GET {gateway}/v1/constitutional/appeals` (route in route-inventory) (`icn/bins/icnctl/src/main.rs` AppealCommands::List); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1943 |
| `icnctl appeal respond` | partial | gateway client `reqwest POST {gateway}/v1/constitutional/appeals/{id}/respond` (route in route-inventory; auth via get_gateway_token) (`icn/bins/icnctl/src/main.rs` AppealCommands::Respond); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:2002 |
| `icnctl appeal show` | partial | gateway client `reqwest GET {gateway}/v1/constitutional/appeals/{id}` (route in route-inventory) (`icn/bins/icnctl/src/main.rs` AppealCommands::Show); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1962 |
| `icnctl appeal withdraw` | partial | gateway client `reqwest POST {gateway}/v1/constitutional/appeals/{id}/withdraw` (route in route-inventory; auth via get_gateway_token) (`icn/bins/icnctl/src/main.rs` AppealCommands::Withdraw); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:2024 |
| `icnctl audit verify` | partial | concrete gateway client `GET /v1/receipts/chain/{hash}` (`icn/bins/icnctl/src/main.rs` AuditCommands::Verify); chain-verification algorithm covered by `icn/bins/icnctl/tests/audit_verify_test.rs` (inlined copy); end-to-end against a live gateway not integration-tested | `icn/bins/icnctl/src/main.rs`:330 |
| `icnctl auth token` | partial | gateway auth client: signs a keystore challenge then `reqwest POST {gateway}/v1/auth/challenge` + `/v1/auth/verify` (routes in `docs/reference/project-index/generated/route-inventory.md`) in `handle_auth_command` (`icn/bins/icnctl/src/main.rs` AuthCommands::Token); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:410 |
| `icnctl charter list` | partial | gateway client `reqwest GET {gateway}/v1/charter` (route in route-inventory) (`icn/bins/icnctl/src/main.rs` CharterCommands::List); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1651 |
| `icnctl charter ratify` | partial | gateway client `reqwest POST {gateway}/v1/charter/{id}/activate` (route in route-inventory; auth via get_gateway_token) (`icn/bins/icnctl/src/main.rs` CharterCommands::Ratify); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1684 |
| `icnctl charter show` | partial | gateway client `reqwest GET {gateway}/v1/charter/{id}` (route in route-inventory) (`icn/bins/icnctl/src/main.rs` CharterCommands::Show); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1641 |
| `icnctl charter sign` | partial | gateway client `reqwest POST {gateway}/v1/charter/{id}/sign` (route in route-inventory; auth via get_gateway_token) (`icn/bins/icnctl/src/main.rs` CharterCommands::Sign); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1666 |
| `icnctl commons anchor` | partial | gateway client `reqwest GET {gateway}/v1/commons/anchor/by-did/{did}` (gateway from `ICN_GATEWAY` env) (`icn/bins/icnctl/src/main.rs` CommonsCommands::Anchor); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1575 |
| `icnctl compute cancel` | partial | authenticated daemon RPC `client.call("compute.cancel", ...)` (`icn/bins/icnctl/src/main.rs` ComputeCommands::Cancel); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:500 |
| `icnctl compute status` | partial | authenticated daemon RPC `client.call("compute.status", ...)` (`icn/bins/icnctl/src/main.rs` ComputeCommands::Status); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:494 |
| `icnctl compute submit` | partial | authenticated daemon RPC `client.call("compute.submit", ...)` via `create_authenticated_rpc_client` in `handle_compute_command` (`icn/bins/icnctl/src/main.rs` ComputeCommands::Submit); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:432 |
| `icnctl compute submit-wasm` | partial | authenticated daemon RPC `client.call("compute.submit", ...)` (reads a local wasm file) via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` ComputeCommands::SubmitWasm); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:463 |
| `icnctl contract call` | partial | daemon RPC `client.call_contract()` (`icn/bins/icnctl/src/main.rs` ContractCommands::Call); "Is icnd running?"; requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:812 |
| `icnctl contract deploy` | partial | signs the deployment locally then daemon RPC `client.deploy_contract()` (`icn_rpc::RpcClient`) in `handle_contract_command` (`icn/bins/icnctl/src/main.rs` ContractCommands::Deploy); "Is icnd running?"; requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:779 |
| `icnctl contract deploy-signed` | partial | daemon RPC deploy of a pre-signed deployment file via `handle_contract_deploy_signed(.., &mut client)` (`icn/bins/icnctl/src/main.rs` ContractCommands::DeploySigned); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:806 |
| `icnctl contract list` | partial | daemon RPC `client.list_contracts()` (`icn/bins/icnctl/src/main.rs` ContractCommands::List); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:828 |
| `icnctl dispute add-evidence` | partial | daemon RPC `client.dispute_add_evidence()` (`icn/bins/icnctl/src/main.rs` DisputeCommands::AddEvidence); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1394 |
| `icnctl dispute assign-mediator` | partial | daemon RPC `client.dispute_assign_mediator()` (`icn/bins/icnctl/src/main.rs` DisputeCommands::AssignMediator); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1405 |
| `icnctl dispute file` | partial | daemon RPC `client.dispute_file()` (`icn_rpc::RpcClient`) (`icn/bins/icnctl/src/main.rs` DisputeCommands::File); "Is icnd running?"; requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1366 |
| `icnctl dispute get` | partial | daemon RPC `client.dispute_get()` (`icn/bins/icnctl/src/main.rs` DisputeCommands::Get); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1388 |
| `icnctl dispute list` | partial | daemon RPC `client.dispute_list()` (`icn/bins/icnctl/src/main.rs` DisputeCommands::List); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1377 |
| `icnctl dispute resolve` | partial | daemon RPC `client.dispute_resolve()` (`icn/bins/icnctl/src/main.rs` DisputeCommands::Resolve); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1416 |
| `icnctl federation attestation from` | partial | daemon RPC `client.federation_attestation_from()` -> `federation.attestation.from` (registered server.rs:884) (`icn/bins/icnctl/src/main.rs` AttestationCommands::From); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1022 |
| `icnctl federation attestation issue` | partial | daemon RPC `client.federation_attestation_issue()` -> `federation.attestation.issue` (registered server.rs:888) (`icn/bins/icnctl/src/main.rs` AttestationCommands::Issue); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1028 |
| `icnctl federation attestation list` | partial | daemon RPC `client.federation_attestation_list()` -> `federation.attestation.list` (registered server.rs:880) (`icn/bins/icnctl/src/main.rs` AttestationCommands::List); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1016 |
| `icnctl federation clearing create` | partial | daemon RPC `client.federation_clearing_create()` -> `federation.clearing.create` (registered server.rs:905) (`icn/bins/icnctl/src/main.rs` ClearingCommands::Create); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1059 |
| `icnctl federation clearing list` | partial | daemon RPC `client.federation_clearing_list()` -> `federation.clearing.list` (registered server.rs:899) (`icn/bins/icnctl/src/main.rs` ClearingCommands::List); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1050 |
| `icnctl federation clearing position` | partial | daemon RPC `client.federation_clearing_position()` -> `federation.clearing.position` (registered server.rs:914) (`icn/bins/icnctl/src/main.rs` ClearingCommands::Position); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1097 |
| `icnctl federation clearing settle` | partial | daemon RPC `client.federation_clearing_settle()` -> `federation.clearing.settle` (registered server.rs:918) (`icn/bins/icnctl/src/main.rs` ClearingCommands::Settle); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1103 |
| `icnctl federation clearing show` | partial | daemon RPC `client.federation_clearing_show()` -> `federation.clearing.show` (registered server.rs:902) (`icn/bins/icnctl/src/main.rs` ClearingCommands::Show); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1053 |
| `icnctl federation connect` | partial | daemon RPC `client.dial(did, addr)` -> `network.dial` (registered server.rs:640) (`icn/bins/icnctl/src/main.rs` FederationCommands::Connect); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:878 |
| `icnctl federation coop list` | partial | daemon RPC `client.federation_coop_list()` -> `federation.coop.list` (registered server.rs:852) (`icn/bins/icnctl/src/main.rs` CoopCommands::List); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:944 |
| `icnctl federation coop register` | partial | daemon RPC `client.federation_coop_register()` -> `federation.coop.register` (registered server.rs:858) (`icn/bins/icnctl/src/main.rs` CoopCommands::Register); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:953 |
| `icnctl federation coop show` | partial | daemon RPC `client.federation_coop_get()` -> `federation.coop.get` (registered server.rs:855) (`icn/bins/icnctl/src/main.rs` CoopCommands::Show); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:947 |
| `icnctl federation coop update` | partial | daemon RPC `client.federation_own_update()` -> `federation.own.update` (registered server.rs:865) (`icn/bins/icnctl/src/main.rs` CoopCommands::Update); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:972 |
| `icnctl federation gateway-connect` | partial | gateway client `reqwest POST {gateway}/v1/federation/connect` (bearer_auth; route `federation_connect` @ `icn/crates/icn-gateway/src/api/federation.rs`, in route-inventory) (`icn/bins/icnctl/src/main.rs` FederationCommands::GatewayConnect); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:899 |
| `icnctl federation invite` | partial | derives DID from the local keystore, then best-effort daemon RPC `client.get_status()` -> `network.status` (registered server.rs:642) for the listen address (`icn/bins/icnctl/src/main.rs` FederationCommands::Invite); requires a running daemon for the full invite URL, not integration-tested | `icn/bins/icnctl/src/main.rs`:896 |
| `icnctl federation peers` | partial | lists `icn.toml` bootstrap peers, then annotates connection state via daemon RPC `client.get_peers()` -> `network.peers` (server.rs:639, best-effort) (`icn/bins/icnctl/src/main.rs` FederationCommands::Peers); requires a running daemon for status, not integration-tested | `icn/bins/icnctl/src/main.rs`:859 |
| `icnctl federation status` | partial | daemon RPC `client.get_peers()` -> `network.peers` (registered `icn/crates/icn-rpc/src/server.rs`:639); prints config-only status if the daemon is down (`icn/bins/icnctl/src/main.rs` FederationCommands::Status); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:856 |
| `icnctl federation vouch issue` | partial | daemon RPC `client.federation_vouch_issue()` -> `federation.vouch.issue` (registered server.rs:871) (`icn/bins/icnctl/src/main.rs` VouchCommands::Issue); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:989 |
| `icnctl federation vouch list` | partial | daemon RPC `client.federation_own_get()` -> `federation.own.get` (server.rs:864) + `federation_coop_list()` -> `federation.coop.list` (852) + `federation_vouch_list()` -> `federation.vouch.list` (registered server.rs:868) (`icn/bins/icnctl/src/main.rs` VouchCommands::List); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1004 |
| `icnctl federation vouch revoke` | partial | daemon RPC `client.federation_vouch_remove()` -> `federation.vouch.remove` (registered server.rs:875) (`icn/bins/icnctl/src/main.rs` VouchCommands::Revoke); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1007 |
| `icnctl gov domain add-member` | partial | gateway client `reqwest POST {gateway}/v1/gov/domains/{domain_id}/members` (bearer_auth; route `add_domain_member` @ `icn/apps/governance/src/http/configure.rs`, in route-inventory) — does not use the RPC client (`icn/bins/icnctl/src/main.rs` DomainCommands::AddMember); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1168 |
| `icnctl gov domain create` | partial | daemon RPC `client.call("governance.domain.create")` via `create_authenticated_rpc_client` in `handle_gov_command` (`icn/bins/icnctl/src/main.rs` DomainCommands::Create); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1127 |
| `icnctl gov domain list` | partial | daemon RPC `client.call("governance.domain.list")` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` DomainCommands::List); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1165 |
| `icnctl gov domain show` | partial | daemon RPC `client.call("governance.domain.get")` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` DomainCommands::Show); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1158 |
| `icnctl gov proposal close` | partial | daemon RPC `client.call("governance.proposal.close")` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` ProposalCommands::Close); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1271 |
| `icnctl gov proposal create` | partial | daemon RPC `client.call("governance.proposal.create")` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` ProposalCommands::Create); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1191 |
| `icnctl gov proposal list` | partial | daemon RPC `client.call("governance.proposal.list")` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` ProposalCommands::List); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1253 |
| `icnctl gov proposal open` | partial | three daemon RPC calls via `create_authenticated_rpc_client`: `client.call("governance.proposal.get")`, then `client.call("governance.domain.get")`, then `client.call("governance.proposal.open")` (`icn/bins/icnctl/src/main.rs` ProposalCommands::Open); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1242 |
| `icnctl gov proposal show` | partial | daemon RPC `client.call("governance.proposal.get")` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` ProposalCommands::Show); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1264 |
| `icnctl gov vote cast` | partial | daemon RPC `client.call("governance.vote.cast")` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` VoteCommands::Cast); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1288 |
| `icnctl institution bootstrap apply` | partial | async `apply_package` posts the package to the gateway via `reqwest::Client` (`icn/bins/icnctl/src/institution_bootstrap.rs` InstitutionCommands::Bootstrap -> InstitutionBootstrapCommands::Apply); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/institution_bootstrap.rs`:49 |
| `icnctl ledger balance` | partial | daemon RPC client `client.get_ledger_balance()` via `create_rpc_client` (`icn/bins/icnctl/src/main.rs` LedgerCommands::Balance); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:728 |
| `icnctl ledger head` | partial | daemon RPC client `client.get_ledger_head()` via `create_rpc_client` in `handle_ledger_command` (`icn/bins/icnctl/src/main.rs` LedgerCommands::Head); "Is icnd running?"; requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:725 |
| `icnctl ledger history` | partial | daemon RPC client `client.get_ledger_history()` via `create_rpc_client` (`icn/bins/icnctl/src/main.rs` LedgerCommands::History); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:738 |
| `icnctl ledger quarantine drop` | partial | daemon RPC client `client.quarantine_drop()` via `create_rpc_client` (`icn/bins/icnctl/src/main.rs` LedgerCommands::Quarantine -> QuarantineCommands::Drop); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:767 |
| `icnctl ledger quarantine get` | partial | daemon RPC client `client.quarantine_get()` via `create_rpc_client` (`icn/bins/icnctl/src/main.rs` LedgerCommands::Quarantine -> QuarantineCommands::Get); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:755 |
| `icnctl ledger quarantine list` | partial | daemon RPC client `client.quarantine_list()` via `create_rpc_client` in `handle_quarantine_command` (`icn/bins/icnctl/src/main.rs` LedgerCommands::Quarantine -> QuarantineCommands::List); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:752 |
| `icnctl ledger quarantine purge` | partial | daemon RPC client `client.quarantine_purge()` via `create_rpc_client` (`icn/bins/icnctl/src/main.rs` LedgerCommands::Quarantine -> QuarantineCommands::Purge); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:773 |
| `icnctl ledger quarantine release` | partial | daemon RPC client `client.quarantine_release()` via `create_rpc_client` (`icn/bins/icnctl/src/main.rs` LedgerCommands::Quarantine -> QuarantineCommands::Release); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:761 |
| `icnctl network dial` | partial | daemon RPC client `client.dial(did, addr)` via `create_rpc_client` (`icn/bins/icnctl/src/main.rs` NetworkCommands::Dial); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:837 |
| `icnctl network peers` | partial | daemon RPC client `client.get_peers()` via `create_rpc_client` in `handle_network_command` (`icn/bins/icnctl/src/main.rs` NetworkCommands::Peers); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:834 |
| `icnctl network stats` | partial | daemon RPC client `client.get_stats()` via `create_rpc_client` (`icn/bins/icnctl/src/main.rs` NetworkCommands::Stats); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:847 |
| `icnctl network status` | partial | daemon RPC client `client.get_status()` then optionally `client.get_nat_status()` via `create_rpc_client` (`icn/bins/icnctl/src/main.rs` NetworkCommands::Status); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:850 |
| `icnctl preflight` | partial | runs real local health checks (data-dir, keystore open via `AgeKeyStore`) in `handle_preflight_command`; the gateway-connectivity check requires a running gateway; not integration-tested | `icn/bins/icnctl/src/main.rs`:203 |
| `icnctl quota list` | partial | authenticated daemon RPC `client.call("quota.list", ...)` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` QuotaCommands::List); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:555 |
| `icnctl quota show` | partial | authenticated daemon RPC `client.call("quota.usage", ...)` via `create_authenticated_rpc_client` in `handle_quota_command` (`icn/bins/icnctl/src/main.rs` QuotaCommands::Show); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:544 |
| `icnctl receipts allocation` | partial | concrete gateway client `reqwest GET {gateway}/v1/receipts/allocations/{hash}` (route in `docs/reference/project-index/generated/route-inventory.md`) (`icn/bins/icnctl/src/main.rs` ReceiptCommands::Allocation); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:299 |
| `icnctl receipts chain` | partial | concrete gateway client `reqwest GET {gateway}/v1/receipts/chain/{decision_hash}` (route in `docs/reference/project-index/generated/route-inventory.md`) (`icn/bins/icnctl/src/main.rs` ReceiptCommands::Chain); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:281 |
| `icnctl receipts intent` | partial | concrete gateway client `reqwest GET {gateway}/v1/receipts/intents/{hash}` (route in `docs/reference/project-index/generated/route-inventory.md`) (`icn/bins/icnctl/src/main.rs` ReceiptCommands::Intent); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:313 |
| `icnctl recovery attest` | partial | authenticated daemon RPC `client.attest_recovery()` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` RecoveryCommands::Attest); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:656 |
| `icnctl recovery cancel` | partial | authenticated daemon RPC `client.cancel_recovery()` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` RecoveryCommands::Cancel); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:681 |
| `icnctl recovery finalize` | partial | authenticated daemon RPC `client.finalize_recovery()` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` RecoveryCommands::Finalize); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:675 |
| `icnctl recovery initiate` | partial | authenticated daemon RPC `client.initiate_recovery()` via `create_authenticated_rpc_client` in `handle_recovery_command` (`icn/bins/icnctl/src/main.rs` RecoveryCommands::Initiate); "Is icnd running?"; requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:650 |
| `icnctl recovery list` | partial | authenticated daemon RPC `client.list_recoveries()` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` RecoveryCommands::List); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:666 |
| `icnctl recovery status` | partial | authenticated daemon RPC `client.get_recovery_status()` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` RecoveryCommands::Status); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:669 |
| `icnctl registry list` | partial | concrete gateway client `reqwest GET {gateway}/v1/registry/decisions` (route in `docs/reference/project-index/generated/route-inventory.md`) (`icn/bins/icnctl/src/main.rs` RegistryCommands::List); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:366 |
| `icnctl registry trace` | partial | concrete gateway client `reqwest GET {gateway}/v1/registry/decisions/{receipt_id}/trace` (route in `docs/reference/project-index/generated/route-inventory.md`) (`icn/bins/icnctl/src/main.rs` RegistryCommands::Trace); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:389 |
| `icnctl status` | partial | RPC client to the daemon (`create_rpc_client` + `client.get_status()`) in `handle_status_command` (`icn/bins/icnctl/src/main.rs` Commands::Status); prints local config and fails gracefully when no daemon; requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:51 |
| `icnctl steward attesters` | partial | gateway client `reqwest GET {gateway}/v1/steward/attesters` (route in route-inventory) (`icn/bins/icnctl/src/main.rs` StewardCommands::Attesters); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1458 |
| `icnctl steward info` | partial | gateway client `reqwest GET` whose route branches on the input: `{gateway}/v1/steward/by-did/{steward}` if it starts with `did:`, else `{gateway}/v1/steward/{steward}` (both routes in route-inventory) (`icn/bins/icnctl/src/main.rs` StewardCommands::Info); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1436 |
| `icnctl steward list` | partial | gateway client `reqwest GET {gateway}/v1/steward` (route in route-inventory) (`icn/bins/icnctl/src/main.rs` StewardCommands::List); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1445 |
| `icnctl steward register` | partial | gateway client `reqwest POST {gateway}/v1/steward` (route in route-inventory) (`icn/bins/icnctl/src/main.rs` StewardCommands::Register); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1465 |
| `icnctl steward retire` | partial | gateway client `reqwest POST {gateway}/v1/steward/{id}/retire` (route in route-inventory) (`icn/bins/icnctl/src/main.rs` StewardCommands::Retire); requires a running gateway, not integration-tested | `icn/bins/icnctl/src/main.rs`:1487 |
| `icnctl steward status` | partial | daemon RPC client `create_rpc_client` + `client.get_status()` (also reads local config.toml) in `handle_steward_command` (`icn/bins/icnctl/src/main.rs` StewardCommands::Status); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:1430 |
| `icnctl trust add` | partial | daemon RPC client `client.add_trust()` via `create_authenticated_rpc_client` in `handle_trust_command` (`icn/bins/icnctl/src/main.rs` TrustCommands::Add); "Is icnd running?"; requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:694 |
| `icnctl trust list` | partial | daemon RPC client `client.list_trust()` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` TrustCommands::List); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:707 |
| `icnctl trust remove` | partial | daemon RPC client `client.remove_trust()` via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` TrustCommands::Remove); requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:716 |
| `icnctl trust show` | partial | daemon RPC client (compute-trust-score call) via `create_authenticated_rpc_client` (`icn/bins/icnctl/src/main.rs` TrustCommands::Show); "Is icnd running?"; requires a running daemon, not integration-tested | `icn/bins/icnctl/src/main.rs`:710 |
| `icnctl charter deploy` | planned | handler validates the CCL doc locally, then prints "Not yet implemented — charter deployment requires gateway integration" (`icn/bins/icnctl/src/main.rs` CharterCommands::Deploy) | `icn/bins/icnctl/src/main.rs`:1718 |
| `icnctl commons affiliations` | planned | prints "Affiliation lookup requires gateway integration. This feature is pending gateway API implementation." — no client call (`icn/bins/icnctl/src/main.rs` CommonsCommands::Affiliations) | `icn/bins/icnctl/src/main.rs`:1582 |
| `icnctl commons join` | planned | prints "Join request requires gateway integration. This feature is pending gateway API implementation." — no client call (`icn/bins/icnctl/src/main.rs` CommonsCommands::Join) | `icn/bins/icnctl/src/main.rs`:1589 |
| `icnctl commons leave` | planned | prints "Leave request requires gateway integration. This feature is pending gateway API implementation." — no client call (`icn/bins/icnctl/src/main.rs` CommonsCommands::Leave) | `icn/bins/icnctl/src/main.rs`:1600 |
| `icnctl federation clearing rate` | planned | no client call — prints "Exchange rate noted..." + a note; in-source comment "Rate updates are not yet implemented via RPC" (`icn/bins/icnctl/src/main.rs` ClearingCommands::Rate) | `icn/bins/icnctl/src/main.rs`:1078 |
| `icnctl gov proposal cancel` | planned | `bail!("Cancel command not yet supported via RPC. Use 'proposal close' instead, or stop the daemon and modify the store directly.")` before any client call (`icn/bins/icnctl/src/main.rs` ProposalCommands::Cancel) | `icn/bins/icnctl/src/main.rs`:1278 |
| `icnctl gov vote show` | planned | `bail!("Vote show command not yet supported via RPC. Use 'proposal show {proposal_id}' to see the proposal.")` before any client call (`icn/bins/icnctl/src/main.rs` VoteCommands::Show) | `icn/bins/icnctl/src/main.rs`:1303 |
| `icnctl steward check-vui` | planned | placeholder; validates input then prints "VUI registry check requires running steward daemon" (`icn/bins/icnctl/src/main.rs` StewardCommands::CheckVui) | `icn/bins/icnctl/src/main.rs`:1496 |
| `icnctl steward enrollment-status` | planned | placeholder; ceremony status check requires a running steward daemon (`icn/bins/icnctl/src/main.rs` StewardCommands::EnrollmentStatus) | `icn/bins/icnctl/src/main.rs`:1513 |
| `icnctl steward issue-token` | planned | placeholder for the full SDIS token issuance flow; requires a running steward daemon (`icn/bins/icnctl/src/main.rs` StewardCommands::IssueToken) | `icn/bins/icnctl/src/main.rs`:1544 |
| `icnctl steward recovery-status` | planned | placeholder; ceremony status check requires a running steward daemon (`icn/bins/icnctl/src/main.rs` StewardCommands::RecoveryStatus) | `icn/bins/icnctl/src/main.rs`:1538 |
| `icnctl steward start-enrollment` | planned | placeholder for the full SDIS enrollment flow; requires a running steward daemon (`icn/bins/icnctl/src/main.rs` StewardCommands::StartEnrollment) | `icn/bins/icnctl/src/main.rs`:1502 |
| `icnctl steward start-recovery` | planned | placeholder for the full SDIS recovery flow; requires a running steward daemon (`icn/bins/icnctl/src/main.rs` StewardCommands::StartRecovery) | `icn/bins/icnctl/src/main.rs`:1519 |

### Status non-claims

- A command marked `live` is **not** a production-readiness claim — only that it runs and does real work per the cited local/test evidence.
- This generated static inventory is **not** runtime execution proof except where a basis cites a test; proof level stays `L1` (declaration) regardless of status.
- A default `unknown / needs local verification` is an honest "not yet verified", not a failure or a defect.
- Feature-gated commands remain **excluded** from the default-build counts and are left unclassified.

## Unparsed / unknown candidates

- None: every top-level variant resolved to either a leaf command or a parsed `#[derive(Subcommand)]` enum.

## Safe vs unsafe claims (examples)

- ✅ Safe: "`icnctl api export-openapi` is declared in the CLI at this commit (L1)."
- ✅ Safe: "`icnctl` declares 162 commands across these groups; role grouping is a curated navigation aid pending review."
- ❌ Unsafe: "`icnctl gov …` is organizer-ready" / "this command is live in production" / "this command is safe for non-technical organizers" — none of that is established by a declaration scan.

