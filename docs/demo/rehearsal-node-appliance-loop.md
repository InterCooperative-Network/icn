# Assembled Rehearsal Node — organizer → member loop (appliance)

**Status**: descriptive · non-canonical · **Last Reviewed**: 2026-07-12

This is the assembled-appliance wiring of the Rehearsal Node organizer
review→confirm loop (#2386). It connects, on a single isolated demo-profile VM,
the browser organizer surface (`web/member-shell/?surface=organizer`, PR #2407)
to the merged Rehearsal-mode runtime (PR #2406) so a person can run the whole
loop with **no terminal and no credential paste**:

```text
one command  →  browser opens the organizer surface
  →  Start organizer rehearsal  (a fresh least-privilege organizer session)
  →  review one fictional item · confirm the bound preview
  →  one real action item + ADR-0026 process receipts are created on THIS VM
  →  Continue as the assigned member  (a FRESH least-privilege member session)
  →  complete the action card  →  completion receipt
  →  steward verifies: sudo icn-demo-verify --rehearsal
```

Fictional data on an isolated node. **Not production, not a pilot, not live
federation.** Receipts record process facts and grant no authority.

## What changed to assemble it

- **Governance mode.** The demo-profile daemon runs
  `ICN_GOVERNANCE_BUILD_MODE=rehearsal` (`20-demo-profile.conf`). This is a
  labeled non-production stance, **strictly additive** over `test`: it mounts the
  `/v1/gov/domains/{d}/rehearsal/*` surface and adds rehearsal rows to the
  pending-publish summary. The dev gates key on "not production" (rehearsal
  passes); Production validation is unchanged; no runtime path keys on `== test`.
- **Three credential shapes**, all minted by trusted local issuance
  (`icnctl --local-mint`, this VM's own instance-local gateway secret — never the
  public self-asserted `/auth/verify` path, which stays fail-closed on the
  routable `0.0.0.0` bind, #2075):
  - **internal setup** — `governance:read` + `governance:rehearsal:setup`
    (initializes the workspace + binds the fictional label). **Never returned to
    the browser or logged.**
  - **organizer browser** — `governance:read` +
    `governance:pending-publish:review` + `governance:pending-publish:confirm`.
    No setup / write / meeting:write / action-item:complete / entity:write /
    coop:admin.
  - **member browser** — `governance:read` + `governance:action-item:complete`
    (completion-only, #2400). No organizer or setup authority.
- **Role sessions.** The loopback demo-session endpoint reads a **closed** role
  intent (`{"role":"organizer"|"member"}`) and maps it to one of two **fixed**
  `icn-demo-seed --session <role>` commands — no request bytes reach the command.
  It returns **one** role's least-privilege session per request. A role
  transition mints a **fresh** session; it never upgrades a token in the browser.
- **Deterministic setup.** The seed reuses the fictional NYCN package's
  `nycn-federation-gov` StaticList domain (whose sole member is the per-instance
  operator DID), initializes the rehearsal workspace once (idempotent — it never
  wipes an organizer-created item), and binds `Example member (fictional)` → the
  operator DID. In a single-operator appliance the organizer and the assigned
  member are the **same** operator DID wearing different least-privilege tokens.
  No pre-seeded action item — the organizer creates it by confirming.

## Run it

One-command (from your workstation, tunnels + opens the organizer surface):

```bash
ICN_DEMO_VM_IP=<node-ip> bash deploy/appliance/scripts/open-proxmox-demo.sh
```

Steward verification (in-VM, after the organizer session has run):

```bash
sudo icn-demo-seed --session organizer     # if not already seeded
sudo icn-demo-verify --rehearsal            # least-privilege matrix + loop + evidence
```

`icn-demo-verify --rehearsal` asserts, against THIS VM's live gateway: the
rehearsal routes are mounted; the organizer token can review+confirm but **cannot**
bind, initialize, complete a member item, or broad-write; the member token can
complete but **cannot** review/bind; one action item is created by confirmation;
the member completion receipt binds (32-byte `record_hash`); the process-receipt
ladder (gate/activation/plan/applied) and the value-withheld evidence packet
validate with **no DID and no credential**.

## Verification status

- **Software loop — PROVEN end-to-end** against a real rehearsal-mode `icnd`
  built from this branch (the entire loop above ran green: setup, least-privilege
  matrix, organizer confirm → item created, member complete, completion receipt +
  process receipts + value-withheld evidence). This validates the build-mode
  flip, the seed setup, the role scopes, and the verifier matrix against the
  actual runtime.
- **Full assembled-image (KVM) witness — NOT performed in the authoring session.**
  It requires building a demo-profile `qcow2` from a base image
  (`build-image.sh --real`) and then `smoke-local.sh --real --demo`; the base
  image build is a heavy separate step. Per the honesty rule, the appliance
  tranche is **not** claimed "assembled-image witnessed" until that runs. The KVM
  smoke path is otherwise ready (qemu + `/dev/kvm` available; the `--demo` flow
  drives a real browser request path and a no-outbound canary).

## Non-claims

Receipts do not create authority · rehearsal is not a pilot · fictional data is
not live-federation data · identity binding is a fictional-rehearsal convenience,
not production enrollment · one local node is not federation · a successful
software rehearsal is not organizer approval, nor the human assistive-technology
gate (#2041). The human gates #2041 / #1703 / #1746 remain open.
