# ICN Governance Demo — Presentation-Ready Sprint

## Session Context

You are working on the ICN (InterCooperative Network) project at `~/projects/icn`. The `main` branch has 4 unpushed commits from a demo sprint completed Mar 14 2026:

```
9253a65e docs(state): add governance demo sprint status entry
fbd559f3 fix(governance): pass authenticated voter DID through actor command channel
4bd3cb70 Merge feat/demo-sprint: governance route fix + demo pipeline
95d1d65f fix(gateway): resolve governance route 404 caused by actix-web scope shadowing
```

All 547 tests pass. The 18-step governance demo works from cold start. Vote tally is correct (2/2). Working tree is clean.

**Why this matters:** The NY Cooperative Summit planning call is March 19 at 2pm EDT. I need to pitch ICN as the tech/infrastructure track and propose a governance demo workshop for the May call for presenters. The audience is cooperative organizers (Joe Marraffino, Frank Cetera, Liska Wilson, etc.) — NOT developers. They need to see what cooperative digital governance looks like, not how Rust actors work.

---

## Tasks (in order)

### 1. Push main to origin

```bash
git branch --show-current  # confirm main
git log --oneline origin/main..main  # confirm 4 commits
git push origin main
```

### 2. Audit and polish demo.html

Read `crates/icn-gateway/static/demo.html`. Assess its current state:
- Does it render a usable UI?
- Can a non-technical person understand what's happening?
- Does it connect to the gateway API and show live results?

**If it's a skeleton or developer-facing:** Rewrite it as a single-page presentation demo with these requirements:
- Clean, modern CSS (no framework dependencies — inline everything)
- Title: "Cooperative Governance Demo — InterCooperative Network"
- Three visual phases: **Setup** (create co-op + members), **Govern** (propose + vote), **Verify** (tally + proof)
- Each phase has a "Run" button that executes the API calls and shows results inline
- Status indicators: green checkmarks for success, red X for failure, spinner for in-progress
- The proposal text should be relatable: "Approve $12,000 Q2 budget for community kitchen equipment"
- Show the vote tally as a simple bar chart or vote count display
- Show the cryptographic proof hash as a "Receipt" card (not raw JSON)
- Error states should say what went wrong in plain English
- Gateway URL configurable at top (default `http://localhost:8080`)
- Mobile-responsive (it will be projected, but someone might pull it up on their phone)
- No localStorage, no external CDN dependencies, fully self-contained

**If it's already decent:** Polish it to meet the above requirements. Don't rewrite from scratch.

### 3. Create presenter mode script

Create `demo/scripts/present.sh` (or update the existing one) that:
1. Builds `icnd` if needed (skip if binary is fresh)
2. Initializes a clean data directory at `/tmp/icn-demo`
3. Starts the gateway on port 8080
4. Waits for health check (`/health`)
5. Runs `demo-governance.py` in **slow mode** — add a `--presenter` flag that:
   - Pauses between each phase (Setup / Govern / Verify) and prints a human-readable explanation
   - Outputs colored terminal text showing what's happening
   - Waits for Enter key between phases (so presenter controls pacing)
6. Opens the demo.html URL in a message (don't auto-open browser — just print the URL)
7. On Ctrl+C, kills the gateway cleanly

### 4. Add --presenter flag to demo-governance.py

Modify the demo script to support two modes:
- **Default (automated):** Runs all 18 steps without pausing (current behavior, used for CI/testing)
- **`--presenter` flag:** Interactive mode that:
  - Groups steps into 3 phases with narrative headers
  - Prints plain-English explanations before each API call ("Now Alice is casting her vote...")
  - Shows success/failure with color (green/red)
  - Pauses between phases with `[Press Enter to continue]`
  - At the end, prints a summary card:
    ```
    ✓ Cooperative created: Finger Lakes Food Co-op
    ✓ Proposal: "Approve $12,000 Q2 budget for community kitchen equipment"
    ✓ Votes: 2 for, 0 against, 0 abstain
    ✓ Result: ACCEPTED
    ✓ Cryptographic proof: blake3:a1b2c3...
    ```

### 5. Write operator runbook

Create `demo/RUNBOOK.md`:

```markdown
# ICN Governance Demo — Operator Runbook

## Prerequisites
- Rust toolchain (stable)
- Python 3.8+
- `requests` Python package (`pip install requests`)
- Port 8080 available

## Quick Start (automated)
bash demo/scripts/start-demo.sh /tmp/icn-demo
python3 demo/scripts/demo-governance.py http://localhost:8080

## Presenter Mode (interactive, for live demos)
bash demo/scripts/present.sh

## Browser Demo
After starting the gateway, open: http://localhost:8080/static/demo.html

## Troubleshooting
[Fill in based on actual failure modes you encounter]

## Cleanup
pkill -f icnd
rm -rf /tmp/icn-demo
```

Fill in the troubleshooting section with real failure modes — try breaking things intentionally (wrong port, missing data dir, killed mid-flow) and document the error messages and fixes.

### 6. Record a screen capture script

Create `demo/scripts/record-demo.sh` that:
1. Checks for `asciinema` (install hint if missing)
2. Runs the presenter-mode demo while recording
3. Saves to `demo/recordings/governance-demo.cast`
4. Prints: "Upload to asciinema.org or play locally with: asciinema play demo/recordings/governance-demo.cast"

This is the backup plan for March 19 — if live demo is too risky over Zoom, I play the recording.

### 7. Verify everything from cold start

Kill all running `icnd` processes. Delete all temp data. Then:

```bash
# Full cold start
bash demo/scripts/present.sh
# Verify browser UI
curl -s http://localhost:8080/static/demo.html | head -5  # should return HTML
# Verify automated mode still works
pkill -9 -f icnd
bash demo/scripts/start-demo.sh /tmp/icn-verify-2
python3 demo/scripts/demo-governance.py http://localhost:8080
# Run tests
cargo test -p icn-gateway -p icn-governance-actor -p icn-governance -p icn-rpc 2>&1 | tail -5
pkill -9 -f icnd
```

All must pass. If anything breaks, fix it before committing.

### 8. Commit and push

Commit in logical chunks:
1. `feat(demo): presentation-ready demo.html with phased UI` (if demo.html was rewritten/polished)
2. `feat(demo): add --presenter flag to demo-governance.py` (interactive mode)
3. `docs(demo): add operator runbook and recording script` (RUNBOOK.md + record-demo.sh + present.sh)

Push to origin/main.

---

## Do NOT

- Refactor the governance actor, gateway, or any core crates — they work, ship them
- Add new API endpoints or features
- Touch the vote tally logic (it's fixed)
- Start federation demo work — that's Phase 1, not this sprint
- Add npm/webpack/React/any build tooling to demo.html — it must be a single self-contained HTML file
- Spend more than 30 minutes on any single task — if it's not working, document why and move on
- Add any external service dependencies (no databases, no Redis, no Docker required for the demo)
- Change the default automated behavior of demo-governance.py — `--presenter` is additive only

## Architecture Reference

- **Gateway binary:** `icnd` (built from workspace root)
- **Gateway port:** 8080
- **Auth flow:** DID challenge (`POST /v1/auth/challenge`) → sign nonce → verify (`POST /v1/auth/verify`) → JWT
- **Governance routes:** All under `/v1/gov/` (domains, proposals, votes, tally, proof)
- **Coop routes:** `/v1/coops/` (create, members, settings)
- **Static files:** `/static/*` served from `crates/icn-gateway/static/`
- **Data dir:** Passed as CLI arg, stores all state (delete to reset)
- **Key crates:** `icn-gateway` (HTTP server), `icn-governance-actor` (actor + commands), `icn-governance` (types + store + handle trait), `icn-rpc` (RPC handler)

## Success Criteria

When this sprint is done, I should be able to:
1. Run `bash demo/scripts/present.sh` and walk a non-technical person through cooperative governance in 5 minutes
2. Open `http://localhost:8080/static/demo.html` in a browser and click through the demo without touching a terminal
3. Play `asciinema play demo/recordings/governance-demo.cast` as a backup if live demo fails
4. Hand someone `demo/RUNBOOK.md` and they can run the demo without asking me questions
