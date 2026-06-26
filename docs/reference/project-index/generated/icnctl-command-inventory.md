---
Status: generated
Canonical: no
Generated: 2026-06-26T02:17:04+00:00
---

# `icnctl` Command Inventory (generated)

> Generated mechanically by [`docs/scripts/icnctl_command_inventory.py`](../../../scripts/icnctl_command_inventory.py). **Do not hand-edit** — rerun the script.
> Regenerate: `python3 docs/scripts/icnctl_command_inventory.py --write`  ·  Check drift: `python3 docs/scripts/icnctl_command_inventory.py --check`  ·  Issue [#2113](https://github.com/InterCooperative-Network/icn/issues/2113)

## What this proves / does not prove

- **Proves:** these `icnctl` command declarations exist in the clap command tree under `icn/bins/icnctl/src/**` at the snapshot commit (proof level **L1**).
- **Does NOT prove:** that a command works, is safe for organizers, is production-ready, is wired to a live gateway, has correct auth/permissions, is part of a supported pilot flow, or is appropriate for non-technical users.
- **`role` is a curated navigation heuristic** (top-level command group -> role), **not** mechanically derived from clap and **needs review**. It says "where might I look first", never "this user may safely run this".
- **`status` is uniformly `unknown / needs local verification`** by construction: a static clap scan proves a command is *declared*, not whether it is `implemented` / `implemented but partial` / `fixture-backed` / `gateway-backed` / `docs-only / design-direction` / `planned`. Assigning those per command is a human/runtime follow-up — so nothing here is presented as live.
- Defer to canonical truth/precedence: [`source-of-truth-map.md`](../source-of-truth-map.md) and proof levels in [`proof-level-taxonomy-capability-matrix.md`](../proof-level-taxonomy-capability-matrix.md). Orientation artifact (`Canonical: no`); companion to [`generated/route-inventory.md`](route-inventory.md).

## Snapshot

- Source commit: `1b9576e06a74658a45242ac19c7a934149bf5c08`
- Source scanned: `icn/bins/icnctl/src/**` (clap `#[derive(Subcommand)]` / `#[derive(Parser)]` tree)

## Summary

- **Total leaf commands (default build): 162**
- Top-level command groups: 33
- By role (curated, needs review): organizer 53 · operator 64 · developer 43 · maintainer 2
- By status: every default-build command is `unknown / needs local verification` (162) — see note above.
- Proof level: every command is `L1` (declaration exists in source).
- **Feature-gated commands (NOT in the default build, excluded from the counts above): 1** (see section below).
- Unparsed / unresolved candidates: 0 (see section below).

## Commands by role

Role is the **curated** top-level-group heuristic (needs review). `status` and `proof` are uniform by construction (see the note above).

### organizer (53)

| Command | Status | Proof | Source |
|---|---|---|---|
| `icnctl amendment add-change` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1867 |
| `icnctl amendment list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1766 |
| `icnctl amendment open-voting` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1809 |
| `icnctl amendment propose` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1731 |
| `icnctl amendment show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1785 |
| `icnctl amendment submit` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1795 |
| `icnctl amendment vote` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1823 |
| `icnctl amendment withdraw` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1849 |
| `icnctl appeal add-evidence` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1972 |
| `icnctl appeal file` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1904 |
| `icnctl appeal list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1943 |
| `icnctl appeal respond` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:2002 |
| `icnctl appeal show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1962 |
| `icnctl appeal withdraw` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:2024 |
| `icnctl charter create` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1614 |
| `icnctl charter deploy` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1718 |
| `icnctl charter inspect` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1704 |
| `icnctl charter list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1651 |
| `icnctl charter ratify` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1684 |
| `icnctl charter show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1641 |
| `icnctl charter sign` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1666 |
| `icnctl charter validate` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1698 |
| `icnctl commons affiliations` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1582 |
| `icnctl commons anchor` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1575 |
| `icnctl commons enroll` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1564 |
| `icnctl commons join` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1589 |
| `icnctl commons leave` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1600 |
| `icnctl commons status` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1561 |
| `icnctl dispute add-evidence` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1394 |
| `icnctl dispute assign-mediator` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1405 |
| `icnctl dispute file` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1366 |
| `icnctl dispute get` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1388 |
| `icnctl dispute list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1377 |
| `icnctl dispute resolve` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1416 |
| `icnctl gov domain add-member` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1168 |
| `icnctl gov domain create` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1127 |
| `icnctl gov domain list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1165 |
| `icnctl gov domain show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1158 |
| `icnctl gov proposal cancel` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1278 |
| `icnctl gov proposal close` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1271 |
| `icnctl gov proposal create` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1191 |
| `icnctl gov proposal list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1253 |
| `icnctl gov proposal open` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1242 |
| `icnctl gov proposal show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1264 |
| `icnctl gov vote cast` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1288 |
| `icnctl gov vote delegate` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1310 |
| `icnctl gov vote delegations` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1325 |
| `icnctl gov vote revoke` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1328 |
| `icnctl gov vote show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1303 |
| `icnctl init-coop` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:128 |
| `icnctl institution bootstrap apply` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/institution_bootstrap.rs`:49 |
| `icnctl institution bootstrap plan` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/institution_bootstrap.rs`:38 |
| `icnctl institution bootstrap validate` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/institution_bootstrap.rs`:27 |

### operator (64)

| Command | Status | Proof | Source |
|---|---|---|---|
| `icnctl audit verify` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:330 |
| `icnctl backup` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:98 |
| `icnctl federation add` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:862 |
| `icnctl federation attestation from` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1022 |
| `icnctl federation attestation issue` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1028 |
| `icnctl federation attestation list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1016 |
| `icnctl federation clearing create` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1059 |
| `icnctl federation clearing list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1050 |
| `icnctl federation clearing position` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1097 |
| `icnctl federation clearing rate` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1078 |
| `icnctl federation clearing settle` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1103 |
| `icnctl federation clearing show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1053 |
| `icnctl federation config` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:884 |
| `icnctl federation connect` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:878 |
| `icnctl federation coop list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:944 |
| `icnctl federation coop register` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:953 |
| `icnctl federation coop show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:947 |
| `icnctl federation coop update` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:972 |
| `icnctl federation gateway-connect` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:899 |
| `icnctl federation invite` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:896 |
| `icnctl federation peers` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:859 |
| `icnctl federation remove` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:872 |
| `icnctl federation set` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:887 |
| `icnctl federation status` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:856 |
| `icnctl federation vouch issue` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:989 |
| `icnctl federation vouch list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1004 |
| `icnctl federation vouch revoke` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1007 |
| `icnctl network dial` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:837 |
| `icnctl network peers` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:834 |
| `icnctl network stats` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:847 |
| `icnctl network status` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:850 |
| `icnctl policy list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:531 |
| `icnctl policy remove` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:534 |
| `icnctl policy set` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:513 |
| `icnctl policy show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:524 |
| `icnctl preflight` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:203 |
| `icnctl quota list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:555 |
| `icnctl quota show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:544 |
| `icnctl restore` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:104 |
| `icnctl snapshot cleanup` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1356 |
| `icnctl snapshot create` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1338 |
| `icnctl snapshot delete` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1350 |
| `icnctl snapshot list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1341 |
| `icnctl snapshot verify` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1344 |
| `icnctl status` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:51 |
| `icnctl steward attesters` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1458 |
| `icnctl steward check-vui` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1496 |
| `icnctl steward config` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1433 |
| `icnctl steward enrollment-status` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1513 |
| `icnctl steward info` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1436 |
| `icnctl steward issue-token` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1544 |
| `icnctl steward list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1445 |
| `icnctl steward recovery-status` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1538 |
| `icnctl steward register` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1465 |
| `icnctl steward retire` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1487 |
| `icnctl steward start-enrollment` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1502 |
| `icnctl steward start-recovery` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1519 |
| `icnctl steward status` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1430 |
| `icnctl steward topics` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:1555 |
| `icnctl trust add` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:694 |
| `icnctl trust list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:707 |
| `icnctl trust remove` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:716 |
| `icnctl trust show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:710 |
| `icnctl verify-backup` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:114 |

### developer (43)

| Command | Status | Proof | Source |
|---|---|---|---|
| `icnctl api export-openapi` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:267 |
| `icnctl auth token` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:410 |
| `icnctl completions` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:214 |
| `icnctl compute cancel` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:500 |
| `icnctl compute status` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:494 |
| `icnctl compute submit` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:432 |
| `icnctl compute submit-wasm` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:463 |
| `icnctl contract call` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:812 |
| `icnctl contract deploy` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:779 |
| `icnctl contract deploy-signed` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:806 |
| `icnctl contract list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:828 |
| `icnctl contract prepare` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:786 |
| `icnctl contract sign` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:796 |
| `icnctl device add` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:600 |
| `icnctl device approve` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:614 |
| `icnctl device list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:597 |
| `icnctl device revoke` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:620 |
| `icnctl id export` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:582 |
| `icnctl id import` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:588 |
| `icnctl id init` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:565 |
| `icnctl id rotate` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:571 |
| `icnctl id show` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:568 |
| `icnctl ledger balance` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:728 |
| `icnctl ledger head` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:725 |
| `icnctl ledger history` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:738 |
| `icnctl ledger quarantine drop` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:767 |
| `icnctl ledger quarantine get` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:755 |
| `icnctl ledger quarantine list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:752 |
| `icnctl ledger quarantine purge` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:773 |
| `icnctl ledger quarantine release` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:761 |
| `icnctl receipts allocation` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:299 |
| `icnctl receipts chain` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:281 |
| `icnctl receipts intent` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:313 |
| `icnctl recovery attest` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:656 |
| `icnctl recovery cancel` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:681 |
| `icnctl recovery config` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:647 |
| `icnctl recovery finalize` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:675 |
| `icnctl recovery initiate` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:650 |
| `icnctl recovery list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:666 |
| `icnctl recovery setup` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:633 |
| `icnctl recovery status` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:669 |
| `icnctl registry list` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:366 |
| `icnctl registry trace` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:389 |

### maintainer (2)

| Command | Status | Proof | Source |
|---|---|---|---|
| `icnctl coop entity-backfill-surrogates` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:249 |
| `icnctl coop entity-report` | unknown / needs local verification | L1 | `icn/bins/icnctl/src/main.rs`:230 |

## Feature-gated commands (not in the default build)

These commands are guarded by a Cargo `#[cfg(feature = …)]` and are **absent from the default `icnctl` build** (`icn/bins/icnctl/Cargo.toml` has `default = []`). They are **excluded** from the counts and role tables above and only exist when the named feature is enabled at build time.

| Command | Required cfg | Role (curated) | Proof | Source |
|---|---|---|---|---|
| `icnctl id upgrade-pq` | `cfg(feature = "post-quantum")` | developer | L1 | `icn/bins/icnctl/src/main.rs`:579 |

## Commands by status

All 162 default-build commands carry the conservative status `unknown / needs local verification`. The static clap scan cannot mechanically distinguish `implemented` / `implemented but partial` / `fixture-backed` / `gateway-backed` / `docs-only / design-direction` / `planned`; that per-command classification is a human/runtime verification follow-up (so demo/dev-gated commands are never presented here as live).

## Unparsed / unknown candidates

- None: every top-level variant resolved to either a leaf command or a parsed `#[derive(Subcommand)]` enum.

## Safe vs unsafe claims (examples)

- ✅ Safe: "`icnctl api export-openapi` is declared in the CLI at this commit (L1)."
- ✅ Safe: "`icnctl` declares 162 commands across these groups; role grouping is a curated navigation aid pending review."
- ❌ Unsafe: "`icnctl gov …` is organizer-ready" / "this command is live in production" / "this command is safe for non-technical organizers" — none of that is established by a declaration scan.

