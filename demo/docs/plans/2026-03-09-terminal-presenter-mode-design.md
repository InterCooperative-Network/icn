# Terminal Presenter Mode — Design Document
**Date**: 2026-03-09
**Status**: Approved, pending implementation
**Context**: New York Cooperative Summit (Oct 4, Albany NY) + The Sync live stream

---

## Problem

The existing demo flows (`flow-1` through `flow-4`) work correctly against the live K3s
cluster and tell a compelling cooperative story. But the raw terminal output is built for
operators, not audiences:

- Endpoint paths, HTTP status codes, and JSON response bodies are visible
- Port-forward startup noise appears at the top of every flow
- No pacing control — output scrolls without pause, hard to narrate over
- No separation between "what the audience sees" and "what the presenter needs"

Running this projected at a summit or streamed live would lose most people before the
first vote is cast.

---

## Goals

1. **Clean audience view** — cooperative story front and center, technical plumbing invisible
2. **Step-by-step presenter control** — pause at each beat, advance on keypress
3. **Narrated auto-scroll mode** — configurable timing for recordings and streams
4. **Presenter sidebar** — full technical output visible only to the presenter (tmux)
5. **Zero rewrite of existing flows** — rendering layer only, all logic unchanged

---

## Non-Goals

- Changing the flow logic or API calls
- Building a web UI or GUI
- Adding new demo scenarios
- Changing the underlying cluster setup

---

## Design

### Two Modes

Both modes are activated via a flag passed to any flow script:

```bash
bash flow-2-patronage.sh --present   # step-by-step, keypress to advance
bash flow-2-patronage.sh --narrated  # auto-scroll, configurable timing
```

No flag = current behavior (full output, no pauses). Existing operator workflow unchanged.

---

### Output Filtering

The `lib-demo-ports.sh` narration helpers (`narrate`, `result`, `aside`, `warn`, `fail`)
are the hook. In presenter/narrated mode, the helpers are redefined:

| Helper | Normal mode | Presenter/Narrated mode |
|--------|-------------|------------------------|
| `narrate` | Prints with blue arrow | **SHOWN** — story beats, step names |
| `result` | Prints with green check | **SHOWN** — successes, key outcomes |
| `warn` | Prints with yellow warning | **SHOWN** — honest gaps, limitations |
| `fail` | Prints with red X | **SHOWN** — errors |
| `aside` | Prints with yellow arrow | **HIDDEN from main pane** — endpoint paths, kubectl detail |
| Raw JSON / `_pretty` | Printed inline | **HIDDEN from main pane** |
| HTTP status codes | Printed inline | **HIDDEN from main pane** |
| Port-forward startup | Printed inline | **HIDDEN from main pane** |

"Hidden" means redirected to `$PRESENTER_LOG` (a temp file), not suppressed entirely.
The presenter sidebar tails this file.

Key data surfaces are **reformatted** for the audience rather than hidden:

- Vote results: `"for_votes": 3` → `  ✓ 3 votes in favor`
- Ledger settlement: raw JSON → `  ✓ 880 patronage_credits → Yusuf Okafor`
- Proposal state: `{"Accepted": {...}}` → `  ✓ Proposal accepted`

---

### Step-by-Step Mode (`--present`)

A `_beat` function is inserted at each narrative transition point in the flow. In
`--present` mode, `_beat` prints a subtle prompt and waits for keypress:

```
  [ SPACE to continue — or ? for presenter notes ]
```

`?` shows a one-line presenter note for that beat (what to say, what to point to).
Any other key advances.

Beats are placed at:
- Before each `narrate` section header
- After each significant result (vote cast, settlement posted, tally shown)
- Before and after any "gap" / limitation callout

In normal mode, `_beat` is a no-op.

---

### Narrated Mode (`--narrated`)

Same clean output as `--present`, but `_beat` sleeps instead of waiting for keypress.

Default timing: **4 seconds** between beats.
Override: `DEMO_BEAT_PAUSE=6 bash flow-2-patronage.sh --narrated`

Designed so a presenter can talk over it at a natural pace without rushing. The 4s
default leaves room for a sentence or two between beats.

---

### Presenter Sidebar (tmux)

A launcher script (`present.sh`) sets up a tmux session with two panes:

```
┌─────────────────────────────────────┬──────────────────────┐
│                                     │  PRESENTER SIDEBAR   │
│   AUDIENCE VIEW (main pane)         │                      │
│                                     │  Full technical log: │
│   ▶ Step 3: Governance vote         │  aside lines,        │
│                                     │  HTTP codes,         │
│   ✓ Harbor Homes voted: for         │  JSON responses,     │
│   ✓ Proposal accepted               │  kubectl output      │
│                                     │                      │
│   [ SPACE to continue ]             │  tail -f $LOG        │
│                                     │                      │
└─────────────────────────────────────┴──────────────────────┘
```

The left pane is what gets screen-captured or projected. The right pane is for the
presenter only (on a laptop screen not visible to the audience).

`present.sh` accepts the same arguments as the flow scripts:

```bash
bash present.sh flow-2-patronage --present
bash present.sh flow-2-patronage --narrated
```

---

### Presenter Notes

Each flow script gets a companion `flow-N-notes.md` with beat-by-beat talking points:

```markdown
## Beat: Governance vote cast
**Say**: "Every member of Harbor Homes has an equal vote. No board, no proxy —
just the cooperative deciding together."
**Point to**: The vote result line showing the DID and choice
**If asked about the DID**: "That's their cryptographic identity — like a
cooperative membership card that only they hold the key to."
```

Notes are also accessible inline via the `?` keypress in `--present` mode.

---

## File Layout

```
demo/
  scripts/
    lib-demo-ports.sh          # Add _beat, PRESENTER_MODE, output redirection
    flow-1-governance.sh       # Add _beat calls at each narrative transition
    flow-2-patronage.sh        # Same
    flow-3-federation.sh       # Same
    flow-4-reporting.sh        # Same
    present.sh                 # NEW: tmux launcher
  notes/
    flow-1-notes.md            # NEW: presenter talking points
    flow-2-notes.md
    flow-3-notes.md
    flow-4-notes.md
```

---

## Implementation Approach

All changes are additive and backwards-compatible:

1. **`lib-demo-ports.sh`**: Add `PRESENTER_MODE` and `BEAT_PAUSE` env vars. Redefine
   `aside`, `_pretty`, and HTTP-display helpers to respect `PRESENTER_MODE`. Add `_beat`
   function.

2. **Flow scripts**: Insert `_beat` calls. No logic changes — purely presentation hooks.

3. **`present.sh`**: tmux launcher. Checks tmux is installed, creates session, splits
   panes, tails presenter log in right pane, runs flow in left pane.

4. **Notes files**: Markdown, one section per beat, written by hand.

---

## Success Criteria

- [ ] `bash flow-2-patronage.sh --present` runs with clean audience output and keypress control
- [ ] `bash flow-2-patronage.sh --narrated` runs with 4s pauses, no keypress needed
- [ ] `bash present.sh flow-2-patronage --present` opens tmux with two-pane layout
- [ ] Presenter sidebar shows full technical log in real time
- [ ] All four flows work in both modes
- [ ] No flag = original behavior, no regression
- [ ] Presenter notes exist for all four flows

---

## Open Questions

- **Font size**: Terminal font size is set by the terminal emulator, not the script.
  Recommend: presenter sets font to 18-20pt before the summit, test with projector resolution.
- **Color on stream**: Some streaming encoders wash out ANSI colors. May need a
  `--no-color` flag or test in advance.
- **tmux availability**: `present.sh` should check for tmux and fall back gracefully
  (single pane with log to file) if not installed.
