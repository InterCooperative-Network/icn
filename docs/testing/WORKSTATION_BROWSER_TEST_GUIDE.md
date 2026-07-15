# Workstation browser test guide — LAN Rehearsal Node

**Status: descriptive · non-production.** The complete organizer→member
rehearsal, from an ordinary browser on a LAN workstation. No terminal, no
credential paste, no configuration. Replace `rehearsal.example.internal` with
your deployment's internal hostname.

## 1. Open the landing page

Open **`https://rehearsal.example.internal/`** in your browser.

You should see the **ICN Rehearsal Node** landing page: a non-production
banner, a status strip (services available; build commit), and three actions.
If the status strip says services are unavailable, wait a minute (the VM may
be booting) and choose **Check again**.

## 2. Start as the organizer

Choose **Start a new rehearsal** (first run or fresh run) or **Continue as
organizer** (resume the current run). The member shell opens on the organizer
surface with a one-click start button — pressing it obtains a short-lived,
organizer-scoped session from the appliance. No account, no password, no
pasted token; the credential lives only in the page's memory.

Then walk the loop:

1. **Review** — the pending item ("proposed work") is listed; open it and
   approve it for editing/assignment.
2. **Edit** — change the item's content; your edit is what will be executed.
3. **Assign** — pick the member-readable holder (the fictional example
   member).
4. **Preview** — the surface shows the exact resulting action and a preview
   digest. What you confirm is bound to this digest.
5. **Confirm** — confirming executes exactly the previewed plan and creates
   **one** real action item, with process receipts recorded.

**Fail-closed checks worth doing on purpose:**
- After opening a preview, edit the item again, then try to confirm the old
  preview → the confirm is refused (stale digest, HTTP 409). Re-preview and
  confirm the current digest.
- Try the member-only completion from the organizer session → refused (403).

The organizer surface also lists the process receipts (plan recorded, action
applied) and an evidence summary.

## 3. Switch to the member

Use the surface's member link, or return to the landing page and choose
**Continue as member**. This mints a **fresh, least-privilege member session**
— the organizer session is never reused or upgraded.

1. The assigned action card is visible.
2. Complete it.
3. The completion receipt appears; the card reaches its completed state.
4. Complete it again (reload and retry) → the state does not double-apply
   (completion is idempotent).
5. Try an organizer-only act (review/confirm) from the member session →
   refused (403).

## 4. Evidence

The organizer surface's evidence summary shows the value-withheld evidence
export: it proves the process happened without carrying identities or
credentials. The operator-side verification (`sudo icn-demo-verify
--rehearsal` on the VM) re-checks the receipt ladder and the
no-identity/no-credential invariant; tampering with an exported packet makes
verification fail closed.

## 5. Reset and run again

Back on the landing page, **Start a new rehearsal** retires the previous
run's unfinished items and opens a fresh organizer session. Receipts from
earlier runs remain — they are permanent process facts, and the new run is a
new workspace generation. Repeat §2–§3; the previous run's state must not
leak into the new one.

## When something fails

The landing page's **Diagnostics** section explains every expected failure
state (service unavailable, expired session, wrong role, stale preview,
already completed) and the operator commands behind them. The full matrix of
steps and expected outcomes is
[WORKSTATION_BROWSER_TEST_MATRIX.md](WORKSTATION_BROWSER_TEST_MATRIX.md);
recovery behavior is
[REHEARSAL_RESET_AND_RECOVERY.md](REHEARSAL_RESET_AND_RECOVERY.md).
