---
Status: descriptive
Canonical: no
Last Reviewed: 2026-07-27
---

# Two-node appliance proof v0.2 plan

## Purpose

This is the executable acceptance design for the next ICN appliance proof. It
uses one reviewed appliance image to create two independently initialized VMs,
establishes an explicit authenticated peer relationship, moves one meaningful
receipt artifact from the producing node to an independently controlled witness
node, and then exercises restart, reboot, backup, destruction, and restoration.

It is the acceptance design for the appliance profile of the deployment-profiles
proposal in PR #2458 (which would land as ADR-0086; that file does **not** exist
under `docs/adr/` on `main` today, and this plan does not depend on it merging).
It does not adopt that proposal or promote its deployment decision.

This document is a test design, not a completed witness. It deliberately stops
at two current product gaps:

1. `icnctl audit verify` fetches the receipt chain from a gateway; ICN does not
   yet have a reviewed offline receipt-bundle verifier that lets Node B verify
   bytes independently of Node A.
2. The appliance data backup does not include `/etc/icn/icnd.env`, which holds
   the secret needed to open the restored keystore. Copying `/var/lib/icn`
   alone is not independent recovery.

The proof MUST remain blocked until both gaps have explicit, reviewed
implementations. A remote `200`, a connected peer count, a copied JSON file, or
a retained QCOW2 overlay does not waive either gate.

## Claim boundary

| Layer | Required evidence | What it does not establish |
|---|---|---|
| Transport connectivity | Node A and Node B exchange authenticated network traffic over the isolated test LAN | enrollment, authority, sync, or federation |
| Peer identity | Each node records the other node's DID and the authenticated connection binds to that DID | institutional membership or delegated authority |
| Explicit relationship | Both configurations name the other DID/address and a retained relationship record identifies the intended witness role | governance ratification or federation agreement |
| Institutional enrollment | A separately signed/ratified enrollment artifact, if exercised | inferred from transport or bootstrap configuration |
| Data synchronization | Exact named state objects converge and their hashes match | inferred from peer connectivity or generic gossip counters |
| Receipt transfer | Node B receives a byte-identical receipt bundle produced by Node A | independent verification by itself |
| Independent verification | Node B verifies the exported bundle locally using pinned Node A identity/key material and no request to Node A | production, legal, or institutional adoption |
| Backup | A recovery bundle contains Node B data plus the secret/config material required to reopen it | restoration |
| Restoration | A new overlay created from the original image assumes Node B's prior identity and state from the recovery bundle | availability, disaster-recovery SLA, or federation |
| Federation | Not exercised by v0.2 | never inferred from any row above |

## Exact roles

### Node A — genesis institution/domain host

Node A first-boots independently, owns its identity and state, initializes the
test domain, and produces the receipt bundle. It is the artifact source.

### Node B — independent witness/verifier

Node B first-boots independently, owns a different identity and state, has no
write authority on Node A, receives the exported receipt bundle, and verifies
it locally. Node B is a technical witness in v0.2, not a second institution
with governance authority.

Two-institution operation requires a later ratified enrollment/authority
ceremony. This plan preserves that distinction instead of calling an
authenticated peer a federation.

## Inputs pinned before execution

The runner MUST record these before creating either VM:

```text
origin_main_commit
proof_runner_commit
image_path
image_sha256
manifest_path
manifest_sha256
base_image_sha256
icnd_sha256
icnctl_sha256
qemu_version
host_kernel
libguestfs_version
```

```bash
icnctl appliance verify-manifest <manifest.json> --root <reviewed-tree>
```

`<manifest.json>` is a required positional argument, not an option; omitting it
exits with a clap usage error (`required arguments were not provided:
<MANIFEST>`) rather than a fail-closed verification, so a runner must not treat
a bare `--root` invocation as a passing gate. `--root` is optional and defaults
to the current directory.

This command MUST pass before the image is copied or booted. The manifest MUST
identify the same Git commit and image hash recorded by the runner. The run
stops on any mismatch.

## Isolated topology

The proof changes no host bridge, route, firewall, DNS, K3s, or private
infrastructure. Each VM has:

- one QEMU user-mode interface with `restrict=on` and a distinct loopback SSH
  forward for the test runner;
- one QEMU socket interface used only for the two-node LAN;
- a fixed test-only MAC matched by cloud-init;
- a static documentation-range address with no default gateway.

```text
Node A witness LAN: 192.0.2.10/24, MAC 52:54:00:00:02:10
Node B witness LAN: 192.0.2.11/24, MAC 52:54:00:00:02:11
QEMU socket link:    listen/connect on 127.0.0.1:19020
Node A SSH forward:  127.0.0.1:2221 -> guest:22
Node B SSH forward:  127.0.0.1:2222 -> guest:22
```

Node A creates the socket link with
`-netdev socket,id=witness,listen=127.0.0.1:19020`; Node B joins with
`-netdev socket,id=witness,connect=127.0.0.1:19020`. Both use
`-device virtio-net-pci,netdev=witness,mac=<fixed-mac>`.

The runner MUST prove that neither guest has an external default route on the
witness interface and that the isolated pair cannot reach a host canary that
the host itself can reach.

## Evidence directory

Every command writes exit status and sanitized output below:

```text
artifacts/icn/two-node-appliance-<git-short>-<UTC-date>/
  inputs/
    source.json
    manifest.json
    manifest.sha256
  node-a/
    firstboot/
    identity/
    systemd/
    relationship/
    receipt-source/
    restart/
    reboot/
  node-b/
    firstboot/
    identity/
    systemd/
    relationship/
    receipt-received/
    verification/
    restart/
    reboot/
    backup/
    restore/
  network/
    qemu-command-lines.redacted.txt
    peer-connection.json
    isolation-canary.json
  result.json
  SHA256SUMS
```

Evidence MUST contain no bearer credential, keystore passphrase, private key,
private infrastructure address, or unredacted secret environment file. The
final `SHA256SUMS` covers every retained non-secret artifact.

## Procedure and gates

### Gate 0 — source and artifact

1. Fetch and record current `origin/main`.
2. Verify the Git worktree is clean and the runner commit is reviewed.
3. Re-hash the base image, appliance image, manifest, `icnd`, and `icnctl`.
4. Run the typed manifest verifier.
5. Refuse unsigned/immutable claims when the manifest says `signed: false` or
   `immutable: false`.

Success is exact hash agreement and verifier exit zero. Any mismatch is a hard
failure.

### Gate 1 — two independent first boots

1. Create two new QCOW2 overlays from the same immutable input image.
2. Create two cloud-init seed ISOs with distinct instance IDs and SSH host
   keys.
3. Start Node A, then Node B, on the isolated topology.
4. Wait for SSH, first-boot marker, active `icnd`, and authenticated health.
5. Record on each node:
   - machine ID;
   - public DID;
   - SHA-256 of `identity.age`, `config.toml`, and `genesis.json`;
   - filesystem identity of the data volume;
   - systemd status and bounded boot timing.
6. Assert the DIDs, machine IDs, secrets, overlay paths, and stable-state hashes
   are different between A and B.

Equal identity material, a missing first-boot marker, or a service that becomes
healthy before first boot completes is a hard failure.

### Gate 2 — explicit peer relationship

After both DIDs are known:

1. Stop `icnd` on both nodes.
2. Add reciprocal `network.bootstrap_peers` entries:

   ```text
   Node A: icn://<node-b-did>@192.0.2.11:7777
   Node B: icn://<node-a-did>@192.0.2.10:7777
   ```

3. Write a non-secret relationship record on both nodes containing:
   `relationship_id`, both DIDs, intended roles, configuration hashes, creation
   time, and `authority_claim: none`.
4. Restart `icnd`.
5. Require load-bearing logs or an authenticated peer-status API to bind the
   connected address to the expected DID in both directions.
6. Interrupt the QEMU socket link and require both nodes to report the peer
   unavailable; restore the link and require bounded reconnection.

This gate proves an explicit authenticated peer relationship only. If the
runtime cannot bind peer status to the expected DID, the gate fails even when a
TCP connection exists.

### Gate 3 — optional institutional enrollment

This gate is required before anyone describes Node B as a second institution.
It requires a signed authority source, explicit accepting body, scope,
activation time, expiry/revocation rules, and durable receipt. Bootstrap peer
configuration and a trusted-local credential are not enrollment.

v0.2 MAY omit this gate and retain the role name “technical witness.” It MUST
not substitute an implicit or self-asserted authority path.

### Gate 4 — meaningful cross-node artifact

1. Node A creates a named governance action and completes the reviewed
   receipt-producing path.
2. Node A exports a self-contained receipt bundle that includes:
   - canonical receipt bytes;
   - receipt class/version;
   - producing Node A DID;
   - domain and artifact identifiers;
   - parent/provenance references required by the verifier;
   - digest manifest;
   - signature/key reference sufficient for offline checking.
3. Transfer the bundle over the witness LAN using a read-only, narrowly scoped
   session. The runner records source and destination hashes.
4. Assert the byte hash on Node B equals the exported hash on Node A.
5. Disconnect Node A.
6. On Node B, run the reviewed offline verifier against only:
   - the retained bundle;
   - pinned Node A public identity/key material;
   - the declared verification policy.
7. Tamper one byte and require verification failure; restore the original and
   require success.

Current stop condition: the repository has gateway-backed
`icnctl audit verify`, not this offline bundle contract. Until the exporter and
offline verifier exist, Gate 4 is `BLOCKED`, not passed. A Node B `curl` back to
Node A is transfer evidence only.

`COMMUNITY_TOPIC` is not used by this plan. Receipt transfer is explicit and
bounded, so the dormant topic-ownership decision remains independent.

### Gate 5 — service restart and VM reboot

For both nodes:

1. Record identity, config, genesis, relationship, and receipt/bundle hashes.
2. Restart `icnd`; wait for authenticated health and peer reconnection.
3. Re-run the accepted receipt verification on Node B while Node A remains
   disconnected.
4. Reboot both VMs; wait for first-boot marker, active service, and peer
   reconnection.
5. Assert all recorded hashes and DIDs are unchanged.
6. Re-run the tamper-negative and valid-positive bundle checks.

Loss of relationship state, identity drift, receipt drift, verifier dependence
on Node A, or first-boot regeneration is a hard failure.

### Gate 6 — backup and independent restoration of Node B

1. Stop Node B cleanly.
2. Create and verify an encrypted recovery bundle that contains:
   - the supported `icnctl backup` of Node B data;
   - the secret/config material needed to reopen that data, including the
     effective `/etc/icn/icnd.env`;
   - ownership/mode metadata;
   - the relationship record and received receipt bundle;
   - a manifest and hashes.
3. Move the recovery bundle outside Node B's overlay.
4. Power off Node B and retain its overlay only as failure evidence.
5. Create a brand-new overlay from the original appliance image and a new
   cloud-init instance ID.
6. Boot the replacement in an isolated recovery posture with `icnd` stopped.
7. Restore the verified bundle, ownership, and mode metadata.
8. Start `icnd` and assert:
   - the restored public DID equals the original Node B DID;
   - identity/config/genesis/relationship/bundle hashes equal pre-backup values;
   - the offline verifier still passes with Node A disconnected;
   - Node B reconnects to the expected Node A DID after Node A returns.
9. Assert the temporary identity generated by the fresh overlay is not active
   after restoration.

Current stop condition: the appliance lacks a reviewed recovery bundle that
includes the secret-bearing environment safely. A disk snapshot, copied
overlay, or data-only tarball does not pass Gate 6.

### Gate 7 — cleanup

1. Request clean poweroff of both VMs.
2. Require both QEMU processes to exit within the bound.
3. Run `qemu-img check` on retained failure/recovery evidence.
4. Remove temporary seed ISOs, sockets, credentials, and non-evidence overlays.
5. Scan retained logs for credentials, passphrases, private keys, and private
   infrastructure values.
6. Emit `result.json` with one status per gate: `pass`, `fail`, or `blocked`.

The runner exits zero only when every mandatory gate is `pass`. A `blocked`
prerequisite produces a non-zero “not yet executable to completion” result, not
a partial success.

## Acceptance summary

The completed proof must establish all of these simultaneously:

```text
one reviewed image
two independently initialized VMs
two distinct identities and secrets
two independent durable stores
explicit DID-bound peer relationship
one named receipt bundle transferred A -> B
offline verification on B with A disconnected
service restart continuity on both nodes
VM reboot continuity on both nodes
encrypted backup of B including recovery secrets
restore into a new B overlay
restored identity, state, relationship, and verification continuity
clean teardown and secret-clean evidence
```

It still does not establish production readiness, legal/institutional adoption,
or federation. The next authority-grade proof must add two independently
governed institutions and a ratified enrollment/relationship ceremony without
weakening the custody and recovery gates above.
