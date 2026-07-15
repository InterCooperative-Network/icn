# Rehearsal reset and recovery — LAN Rehearsal Node

**Status: descriptive · non-production.** What survives what, and which reset
to use when.

## Three reset levels

| Level | How | What it does | What survives |
|---|---|---|---|
| **New rehearsal** (normal) | Landing page → *Start a new rehearsal* (or `sudo icn-demo-seed --session organizer --fresh` on the VM) | starts a new workspace generation; the prior run's un-completed fictional items are **retired (cancelled, not erased)** | all recorded receipts (permanent process facts), node identity, completed items |
| **Full appliance reset** (destructive) | `sudo icn-demo-reset` on the VM, or `icn-rehearsal-ctl reset` | stops services, **wipes `/var/lib/icn`** and per-instance secrets, re-runs firstboot | nothing node-local; the VM and image remain |
| **Redeploy** | re-import the image disk (see LAN_REHEARSAL_DEPLOYMENT.md) | brand-new node (new identity, empty state) | only the image + repo provenance |

## Durable vs intentionally ephemeral

- **Durable (sled, `/var/lib/icn`):** action items, completion receipts,
  process-transition receipts, node identity/keystore, config. These survive
  service restarts and VM reboots.
- **Rebuilt per process (intentional):** the rehearsal pending-publish
  **workspace view** (rows/generation) has an in-memory component backed by a
  durable item store — an `icnd` restart resets the view while prior confirmed
  items and receipts remain findable. Practically: after a restart, continue
  from the landing page; if the workspace looks empty, *Start a new rehearsal*
  (prior receipts are unaffected).
- **Per-session (by design):** browser credentials live only in page memory —
  closing the tab ends the session; start a fresh one from the landing page.

## Recovery expectations

- **Service restart** (`icn-rehearsal-ctl restart`): stack returns in seconds;
  reload the page.
- **VM reboot**: all units are enabled; the stack recovers unattended. First
  contact after reboot may take ~a minute while `icnd` unlocks and health
  turns green (the landing status strip shows it).
- **Sled lock error in `icnd` logs** ("could not acquire lock"): two icnd
  processes contended — `systemctl restart icnd` after confirming only the
  unit-managed process remains.
- **Session endpoint 403** ("dev gates off"): the VM is not in the rehearsal
  demo posture — verify the demo-profile drop-ins survived whatever changed.

## What reset never does

- It never prints or exposes a credential (reset output is
  credential-free by construction).
- It never erases recorded receipts within a node's lifetime — retiring a
  run cancels its open items; the receipts of what actually happened remain.
