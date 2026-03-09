# Terminal Presenter Mode — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `--present` (keypress-advance) and `--narrated` (auto-scroll) modes to all four ICN demo flow scripts, plus a tmux two-pane launcher and per-flow presenter notes.

**Architecture:** All changes are a rendering layer only — no flow logic changes. `lib-demo-ports.sh` gains a `PRESENTER_MODE` variable, a `_beat` function, and overridden output helpers that suppress technical noise in presenter modes. Flow scripts get `_beat` calls inserted after each `narrate` section. A new `present.sh` launcher sets up a tmux session with audience view (left) and full technical log (right).

**Tech Stack:** bash, tmux, sed, existing lib-demo-ports.sh helper pattern.

---

## Reference

Design doc: `demo/docs/plans/2026-03-09-terminal-presenter-mode-design.md`

Existing files to understand before starting:
- `demo/scripts/lib-demo-ports.sh` — the shared library; all output helpers live here
- `demo/scripts/lib-demo-ports.sh.test` — the smoke test pattern to extend
- `demo/scripts/flow-1-governance.sh` through `flow-4-reporting.sh` — the four flows
- `demo/docs/plans/2026-03-09-terminal-presenter-mode-design.md` — full design rationale

Beat positions (narrate calls per flow):
- flow-1: 11 beats (lines 120–367)
- flow-2: 13 beats (lines 137–485)
- flow-3: 11 beats (lines 125–495)
- flow-4: 12 beats (lines 120–514)

---

## Task 1: Add PRESENTER_MODE detection to lib-demo-ports.sh

**Files:**
- Modify: `demo/scripts/lib-demo-ports.sh` (after the exported variables block, ~line 78)

### Step 1: Add mode detection block

Find the line:
```bash
# HTTP status code from the last demo_curl call
export DEMO_LAST_HTTP_CODE=""
```

Insert this block immediately after:

```bash
# ---------------------------------------------------------------------------
# Presenter mode — set by flow scripts that parse their own args
# PRESENTER_MODE = "" (normal) | "present" (keypress) | "narrated" (auto)
# DEMO_BEAT_PAUSE = seconds between beats in narrated mode (default 4)
# PRESENTER_LOG = file path for technical detail output
# ---------------------------------------------------------------------------
export PRESENTER_MODE="${PRESENTER_MODE:-}"
export DEMO_BEAT_PAUSE="${DEMO_BEAT_PAUSE:-4}"
export PRESENTER_LOG="${PRESENTER_LOG:-/tmp/icn-demo-presenter-$$.log}"

# Initialize log file
if [ -n "$PRESENTER_MODE" ]; then
  : > "$PRESENTER_LOG"
fi
```

### Step 2: Verify the block is syntactically valid

```bash
bash -n demo/scripts/lib-demo-ports.sh
```
Expected: no output, exit 0.

### Step 3: Commit

```bash
git add demo/scripts/lib-demo-ports.sh
git commit -m "feat(demo): add PRESENTER_MODE variable to lib-demo-ports"
```

---

## Task 2: Override output helpers for presenter mode

**Files:**
- Modify: `demo/scripts/lib-demo-ports.sh` (replace the existing helper definitions, ~lines 91–103)

### Step 1: Replace the narration helpers block

Find the existing helpers block:
```bash
# narrate: print a section header / step announcement
narrate() { echo -e "\n${BLUE}▶${NC} $*\n"; }

# result: print a success line
result()  { echo -e "  ${GREEN}✓${NC} $*"; }

# aside: print an informational line
aside()   { echo -e "  ${YELLOW}→${NC} $*"; }

# warn: print a warning line (non-fatal)
warn()    { echo -e "  ${YELLOW}⚠${NC} $*"; }

# fail: print a failure line
fail()    { echo -e "  ${RED}✗${NC} $*"; }
```

Replace with:

```bash
# _strip_technical: remove HTTP codes and DID noise from a string
# Used by result/warn/narrate in presenter mode to keep output clean.
_strip_technical() {
  sed -e 's/ (HTTP [0-9]\{3\})//g' \
      -e 's/HTTP:[0-9]\{3\}//g'
}

# narrate: print a section header / step announcement
narrate() {
  if [ -n "${PRESENTER_MODE:-}" ]; then
    echo -e "\n${BLUE}▶${NC} $*\n" | _strip_technical
  else
    echo -e "\n${BLUE}▶${NC} $*\n"
  fi
}

# result: print a success line
result() {
  if [ -n "${PRESENTER_MODE:-}" ]; then
    echo -e "  ${GREEN}✓${NC} $*" | _strip_technical
  else
    echo -e "  ${GREEN}✓${NC} $*"
  fi
}

# aside: in normal mode print inline; in presenter mode redirect to log only
aside() {
  if [ -n "${PRESENTER_MODE:-}" ]; then
    echo "  → $*" >> "${PRESENTER_LOG:-/dev/null}"
  else
    echo -e "  ${YELLOW}→${NC} $*"
  fi
}

# warn: print a warning line (non-fatal)
warn() {
  if [ -n "${PRESENTER_MODE:-}" ]; then
    echo -e "  ${YELLOW}⚠${NC} $*" | _strip_technical
  else
    echo -e "  ${YELLOW}⚠${NC} $*"
  fi
}

# fail: print a failure line
fail() { echo -e "  ${RED}✗${NC} $*"; }
```

### Step 2: Override `_pretty` (JSON pretty-printer used by flow scripts)

Find the `_pretty` function (or its usage — it may be defined in the flow scripts). Check:

```bash
grep -n "_pretty" demo/scripts/lib-demo-ports.sh demo/scripts/flow-1-governance.sh | head -20
```

If `_pretty` is defined in `lib-demo-ports.sh`, add a presenter-mode guard to it.
If it is only used inline in flow scripts, add this function to `lib-demo-ports.sh`
at the end of the helpers block:

```bash
# _pretty: pretty-print the last curl response body
# In presenter mode, redirects to the log instead of showing on screen.
_pretty() {
  local src="${_RESP_FILE:-/dev/null}"
  if [ -n "${PRESENTER_MODE:-}" ]; then
    echo "--- response body ---" >> "${PRESENTER_LOG:-/dev/null}"
    python3 -m json.tool < "$src" >> "${PRESENTER_LOG:-/dev/null}" 2>/dev/null \
      || cat "$src" >> "${PRESENTER_LOG:-/dev/null}"
  else
    python3 -m json.tool < "$src" 2>/dev/null || cat "$src"
  fi
}
```

### Step 3: Add _beat function

Add this after the helpers block:

```bash
# ---------------------------------------------------------------------------
# _beat [presenter-note]
# Pause point between story beats.
#   --present mode: wait for keypress; ? shows presenter note
#   --narrated mode: sleep DEMO_BEAT_PAUSE seconds
#   normal mode: no-op
# ---------------------------------------------------------------------------
_beat() {
  local note="${1:-}"
  if [ "${PRESENTER_MODE:-}" = "present" ]; then
    echo ""
    if [ -n "$note" ]; then
      printf "  \033[2m[ any key to continue — ? for notes ]\033[0m"
    else
      printf "  \033[2m[ any key to continue ]\033[0m"
    fi
    while IFS= read -r -s -n1 key 2>/dev/null; do
      if [ "$key" = "?" ] && [ -n "$note" ]; then
        echo ""
        echo ""
        echo -e "  ${YELLOW}📋 PRESENTER:${NC} $note"
        echo ""
        printf "  \033[2m[ any key to continue ]\033[0m"
      else
        break
      fi
    done
    echo ""
  elif [ "${PRESENTER_MODE:-}" = "narrated" ]; then
    sleep "${DEMO_BEAT_PAUSE:-4}"
  fi
  # normal mode: no-op
}
```

### Step 4: Syntax check

```bash
bash -n demo/scripts/lib-demo-ports.sh
```
Expected: no output, exit 0.

### Step 5: Update the smoke test

Add these tests to `demo/scripts/lib-demo-ports.sh.test` before the final summary block:

```bash
# ---------------------------------------------------------------------------
# Test: presenter mode infrastructure
# ---------------------------------------------------------------------------
echo "--- Test: presenter mode ---"

# Default: no presenter mode
[ -z "${PRESENTER_MODE:-}" ] && _pass "PRESENTER_MODE unset by default" || _fail "PRESENTER_MODE should be unset"

# aside goes to log in presenter mode
PRESENTER_MODE="present"
PRESENTER_LOG="$(mktemp)"
aside "test-aside-message"
grep -q "test-aside-message" "$PRESENTER_LOG" \
  && _pass "aside redirected to log in presenter mode" \
  || _fail "aside did not redirect to log"
rm -f "$PRESENTER_LOG"
unset PRESENTER_MODE PRESENTER_LOG

# aside prints inline in normal mode
OUTPUT=$(aside "test-inline" 2>&1)
echo "$OUTPUT" | grep -q "test-inline" \
  && _pass "aside prints inline in normal mode" \
  || _fail "aside did not print inline in normal mode"

# result strips HTTP codes in presenter mode
PRESENTER_MODE="present"
OUTPUT=$(result "Posted (HTTP 201)" 2>&1)
echo "$OUTPUT" | grep -q "HTTP" \
  && _fail "result should strip HTTP codes in presenter mode" \
  || _pass "result strips HTTP codes in presenter mode"
unset PRESENTER_MODE

# _beat is a no-op in normal mode
START=$(date +%s%N)
_beat "some note"
END=$(date +%s%N)
ELAPSED=$(( (END - START) / 1000000 ))
[ "$ELAPSED" -lt 500 ] \
  && _pass "_beat is no-op in normal mode (${ELAPSED}ms)" \
  || _fail "_beat took too long in normal mode (${ELAPSED}ms)"

# _beat sleeps in narrated mode
PRESENTER_MODE="narrated"
DEMO_BEAT_PAUSE=1
START=$(date +%s)
_beat
END=$(date +%s)
ELAPSED=$(( END - START ))
[ "$ELAPSED" -ge 1 ] \
  && _pass "_beat sleeps in narrated mode" \
  || _fail "_beat did not sleep in narrated mode"
unset PRESENTER_MODE DEMO_BEAT_PAUSE
```

### Step 6: Run smoke test

```bash
bash demo/scripts/lib-demo-ports.sh.test
```
Expected: all tests PASS, exits 0. New presenter mode tests should pass.

### Step 7: Commit

```bash
git add demo/scripts/lib-demo-ports.sh demo/scripts/lib-demo-ports.sh.test
git commit -m "feat(demo): add _beat, presenter mode output helpers to lib-demo-ports"
```

---

## Task 3: Add argument parsing and _beat calls to flow-1-governance.sh

**Files:**
- Modify: `demo/scripts/flow-1-governance.sh`

Flow-1 beats (narrate lines): 120, 127, 147, 168, 188, 218, 232, 260, 298, 319, 350, 367

### Step 1: Add argument parsing at the top of the script

Find the block near the top of flow-1 that looks like:
```bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-demo-ports.sh"
```

Add argument parsing immediately after the `source` line:

```bash
# Parse presenter mode flag
for _arg in "$@"; do
  case "$_arg" in
    --present)  export PRESENTER_MODE="present"  ;;
    --narrated) export PRESENTER_MODE="narrated" ;;
  esac
done
unset _arg
```

### Step 2: Insert _beat after every narrate call

For each `narrate "..."` line, add `_beat` on the very next line (after any `echo ""` that follows). The beat pauses AFTER the section header is displayed, before the content of the section runs.

Add presenter notes as `_beat` arguments for key story moments. Use empty `_beat` for setup/utility beats.

Here is the complete list of insertions (line numbers are approximate — adjust to actual file):

```bash
# After: narrate "Step 0: Starting Harbor Homes gateway connection"
_beat ""

# After: narrate "Authenticating as Harbor Homes board"
_beat ""

# After: narrate "Step 1: The situation — inspection report received"
_beat "Explain: this is a real cooperative making a real decision. The inspection report triggered a governance process — just like any coop board would run, but recorded on the network."

# After: narrate "Step 2: Establish the governance domain for this vote"
_beat "Point to the domain ID. Every coop has its own governance domain — Harbor Homes controls this one."

# After: narrate "Step 3: Delphine Moreau raises the roof repair proposal"
_beat "Delphine is the board president. She's raising this to the full membership. Anyone can see this proposal — there's no back room."

# After: narrate "Step 4: Board opens the proposal for member voting"
_beat "The proposal is now open. Every member gets a vote. The voting period is enforced by the network, not by trust in an administrator."

# After: narrate "Step 5: Members vote — transparent, named, recorded on-chain"
_beat "Three votes: for, for, for. The DID next to each vote is that member's permanent cooperative identity. This vote is public and permanent."

# After: narrate "Step 6: Tally — the vote count is public to all members"
_beat "100% in favor. Any member — or any outside observer the coop grants access to — can verify this tally independently."

# After: narrate "Step 7: Closing the proposal — result is final"
_beat "The proposal is closed. The outcome is accepted. Nobody can change this."

# After: narrate "Step 8: Governance proof — what is verifiable now"
_beat "This is the key idea: the decision is a verifiable artifact. The roof contractor, a lender, a funder — anyone Harbor Homes chooses to share this with can confirm the vote happened."

# After: narrate "Step 9: Provenance — what any Harbor Homes member can verify right now"
_beat ""

# After: narrate "Step 10: The authorized action — governance to execution"
_beat "Governance authorizes action. The cooperative decided — now the network records that authorization so the execution can prove it was legitimate."
```

### Step 3: Verify the script still parses cleanly

```bash
bash -n demo/scripts/flow-1-governance.sh
```
Expected: no output, exit 0.

### Step 4: Quick dry-run in narrated mode with short pause

```bash
DEMO_BEAT_PAUSE=0 bash demo/scripts/flow-1-governance.sh --narrated 2>&1 | head -30
```
Expected: output starts, no aside lines visible, beats advance automatically.

### Step 5: Commit

```bash
git add demo/scripts/flow-1-governance.sh
git commit -m "feat(demo): add _beat calls and --present/--narrated to flow-1"
```

---

## Task 4: Add argument parsing and _beat calls to flow-2-patronage.sh

**Files:**
- Modify: `demo/scripts/flow-2-patronage.sh`

Flow-2 beats (narrate lines): 137, 145, 164, 194, 231, 257, 283, 317, 347, 375, 422, 452, 485

### Step 1: Add argument parsing (same pattern as Task 3 Step 1)

Find the `source` line and add after it:

```bash
for _arg in "$@"; do
  case "$_arg" in
    --present)  export PRESENTER_MODE="present"  ;;
    --narrated) export PRESENTER_MODE="narrated" ;;
  esac
done
unset _arg
```

### Step 2: Insert _beat after every narrate call

```bash
# After: narrate "Step 0: Starting BrightWorks gateway connection"
_beat ""

# After: narrate "Authenticating as BrightWorks"
_beat ""

# After: narrate "Step 1: The situation — Q1 has closed"
_beat "BrightWorks is a worker coop. Q1 ended with a surplus. Cooperative law requires them to distribute that surplus back to members — proportional to contribution. This is called patronage."

# After: narrate "Step 2: Locating the Q1 patronage proposal"
_beat "The allocation formula was already submitted as a governance proposal. Members are about to vote on whether it's correct."

# After: narrate "Step 3: The allocation formula — transparent to every member"
_beat "Every member can see the formula. 120 hours gets 880 credits. 115 hours gets 843. The math is public and verifiable — not decided behind closed doors."

# After: narrate "Step 4: Opening the proposal for member vote"
_beat ""

# After: narrate "Step 5: Members vote to ratify the Q1 patronage allocation"
_beat "The members are voting on the allocation formula — not just rubber-stamping it. This is the democratic moment."

# After: narrate "Step 6: Vote tally — ratification confirmed"
_beat "Accepted. The allocation is now official. The ledger settlement is authorized."

# After: narrate "Step 7: Closing the proposal — allocation is official"
_beat ""

# After: narrate "Step 8: Ledger settlement — credits distributed to members"
_beat "The governance decision just authorized this ledger entry. 880 patronage credits to Yusuf Okafor. This is what the coop approved happening."

# After: narrate "Step 9: Ledger balance — cooperative account shows allocation"
_beat "The balance is live. Any member can query their own balance at any time — the coop doesn't have to send a statement."

# After: narrate "Step 10: Ledger history — the full allocation trail"
_beat "Every transaction is in the history. Governance decision → formula → vote → settlement → balance. The whole chain is traceable."

# After: narrate "Step 11: Receipt chain — allocation provenance"
_beat "This is the foundation for grant reporting, audit, external accountability — anyone the coop authorizes can follow this chain."
```

### Step 3–5: Same verification and commit pattern as Task 3

```bash
bash -n demo/scripts/flow-2-patronage.sh
DEMO_BEAT_PAUSE=0 bash demo/scripts/flow-2-patronage.sh --narrated 2>&1 | head -30
git add demo/scripts/flow-2-patronage.sh
git commit -m "feat(demo): add _beat calls and --present/--narrated to flow-2"
```

---

## Task 5: Add argument parsing and _beat calls to flow-3-federation.sh

**Files:**
- Modify: `demo/scripts/flow-3-federation.sh`

Flow-3 beats (narrate lines): 125, 133, 153, 178, 218, 264, 306, 356, 392, 455, 495

### Step 1–2: Add argument parsing and _beat insertions

Same pattern. Presenter notes:

```bash
# After: narrate "Step 0: Connecting to all three federation nodes"
_beat ""

# After: narrate "Authenticating all three participants"
_beat ""

# After: narrate "Step 1: The situation — two coops with complementary needs"
_beat "River City has metalworking equipment. BrightWorks needs it occasionally. They're going to form an agreement — not through a central platform, but directly, with Finger Lakes CDN as a neutral facilitator."

# After: narrate "Step 2: Current federation status — three independent views"
_beat "Three coops, three independent nodes. None of them controls the others. The federation is voluntary."

# After: narrate "Step 3: River City Tool Library — internal governance"
_beat "River City governs itself. Their governance decisions don't require approval from BrightWorks or Finger Lakes."

# After: narrate "Step 4: BrightWorks Cooperative — independent internal governance"
_beat "Same for BrightWorks. Independent. Sovereign."

# After: narrate "Step 5: Finger Lakes CDN — facilitating without controlling"
_beat "Finger Lakes is a coordination layer — a facilitator. They don't own the agreement. They help enforce the terms both parties agreed to."

# After: narrate "Step 6: Federation vouching — trust attestation without ownership"
_beat "This is the key idea: trust is attestable without ownership. Finger Lakes says 'we vouch for this agreement' — but River City and BrightWorks remain independent."

# After: narrate "Step 7: Clearing agreement — tracking value across boundaries"
_beat "The agreement is now on the network. Every exchange of off-peak equipment access for maintenance hours gets recorded here — not in a spreadsheet, not in someone's email."

# After: narrate "Step 9: The key moment — three nodes, three views, one agreement"
_beat "Three independent nodes. Each sees the same agreement from their own perspective. No central server. No single point of failure or control."

# After: narrate "Step 10: What federation without centralization looks like"
_beat "This is the whole pitch. Cooperatives coordinating with each other — as equals — without surrendering sovereignty to a platform."
```

### Step 3–5: Verify and commit

```bash
bash -n demo/scripts/flow-3-federation.sh
DEMO_BEAT_PAUSE=0 bash demo/scripts/flow-3-federation.sh --narrated 2>&1 | head -30
git add demo/scripts/flow-3-federation.sh
git commit -m "feat(demo): add _beat calls and --present/--narrated to flow-3"
```

---

## Task 6: Add argument parsing and _beat calls to flow-4-reporting.sh

**Files:**
- Modify: `demo/scripts/flow-4-reporting.sh`

Flow-4 beats (narrate lines): 120, 128, 150, 170, 229, 273, 297, 325, 363, 390, 453, 474, 514

### Step 1–2: Add argument parsing and _beat insertions

```bash
# After: narrate "Step 0: Connecting to all four coop nodes"
_beat ""

# After: narrate "Authenticating all participants"
_beat ""

# After: narrate "Step 1: The reporting scenario"
_beat "Amara is a program officer at a foundation that funds cooperatives. She needs to verify that Harbor Homes and BrightWorks actually did what they said they did — governed democratically, distributed surplus fairly."

# After: narrate "Step 2: Harbor Homes — governance and capital expenditure evidence"
_beat "The foundation can see the vote. Not a summary, not a claim — the actual on-chain record of the democratic decision."

# After: narrate "Step 3: BrightWorks — patronage distribution evidence"
_beat "The patronage allocation is verifiable. The formula, the vote, the ledger entry — all of it."

# After: narrate "Step 4: BrightWorks — ledger history and allocation trail"
_beat "Every transaction, in order. The foundation can trace the surplus from Q1 close → formula → vote → settlement → member balances."

# After: narrate "Step 5: Receipt chain — allocation provenance across the federation"
_beat ""

# After: narrate "Step 6: River City Tool Library — federation coordination evidence"
_beat "The federation agreement is also auditable. River City didn't just claim they were cooperating with BrightWorks — the agreement is on the record."

# After: narrate "Step 7: Finger Lakes CDN — federation overview"
_beat ""

# After: narrate "Step 8: The authorization boundary — read without write"
_beat "This is a security boundary. Amara can read everything she needs to verify. She cannot create transactions, cast votes, or modify anything. Read access only."

# After: narrate "Step 9: Governance dashboard — cooperative health at a glance"
_beat "One view across multiple coops. Active proposals, recent decisions, ledger activity. This is what a network of cooperatives looks like from the outside."

# After: narrate "Step 10: The grant report — what Amara has to show the foundation"
_beat "This is the output. A verifiable report. Not PDFs and spreadsheets — cryptographic proof that the coops governed and distributed as promised."

# After: narrate "Step 11: What PR #1327 (ExecutionReceiptGate) adds to this picture"
_beat "Coming soon: the execution receipt gate closes the last gap. Governance authorizes action; the receipt proves the action happened."
```

### Step 3–5: Verify and commit

```bash
bash -n demo/scripts/flow-4-reporting.sh
DEMO_BEAT_PAUSE=0 bash demo/scripts/flow-4-reporting.sh --narrated 2>&1 | head -30
git add demo/scripts/flow-4-reporting.sh
git commit -m "feat(demo): add _beat calls and --present/--narrated to flow-4"
```

---

## Task 7: Build present.sh tmux launcher

**Files:**
- Create: `demo/scripts/present.sh`

### Step 1: Write present.sh

```bash
#!/usr/bin/env bash
# present.sh — tmux two-pane presenter launcher for ICN demo flows
#
# Usage:
#   bash present.sh <flow-name> [--present|--narrated] [DEMO_BEAT_PAUSE=N]
#
# Examples:
#   bash present.sh flow-2-patronage --present
#   bash present.sh flow-2-patronage --narrated
#   DEMO_BEAT_PAUSE=6 bash present.sh flow-3-federation --narrated
#
# Layout:
#   Left pane  (70%): audience view — clean story output, projected/streamed
#   Right pane (30%): presenter log — full technical detail, your eyes only
#
# Requirements: tmux 2.0+
# Fallback: if tmux not available, runs flow directly with log to /tmp/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------
FLOW_NAME="${1:-}"
if [ -z "$FLOW_NAME" ]; then
  echo "Usage: bash present.sh <flow-name> [--present|--narrated]"
  echo "       flow-name: flow-1-governance | flow-2-patronage | flow-3-federation | flow-4-reporting"
  exit 1
fi

FLOW_SCRIPT="${SCRIPT_DIR}/${FLOW_NAME}.sh"
if [ ! -f "$FLOW_SCRIPT" ]; then
  echo "Error: flow script not found: $FLOW_SCRIPT"
  exit 1
fi

MODE="${2:---present}"
SESSION_NAME="icn-demo-$$"
PRESENTER_LOG="/tmp/icn-demo-presenter-$$.log"
: > "$PRESENTER_LOG"

# ---------------------------------------------------------------------------
# Check tmux availability
# ---------------------------------------------------------------------------
if ! command -v tmux >/dev/null 2>&1; then
  echo "tmux not found — running flow directly (log to $PRESENTER_LOG)"
  export PRESENTER_LOG
  bash "$FLOW_SCRIPT" "$MODE"
  exit 0
fi

# ---------------------------------------------------------------------------
# Build the tmux command string
# The flow script is run with PRESENTER_LOG and MODE exported.
# Left pane runs the flow; right pane tails the log.
# ---------------------------------------------------------------------------
FLOW_CMD="export PRESENTER_LOG='${PRESENTER_LOG}'; export DEMO_BEAT_PAUSE='${DEMO_BEAT_PAUSE:-4}'; bash '${FLOW_SCRIPT}' '${MODE}'; echo ''; echo '  [flow complete — press any key to exit]'; read -r -s -n1"

cat << EOF

  ┌────────────────────────────────────────────────────────────┐
  │  ICN DEMO PRESENTER                                        │
  │  Flow:    ${FLOW_NAME}
  │  Mode:    ${MODE}
  │  Session: ${SESSION_NAME}
  │                                                            │
  │  LEFT PANE  = audience view (project/stream this)         │
  │  RIGHT PANE = presenter log (your eyes only)              │
  │                                                            │
  │  To exit: Ctrl-B then D to detach, or close the window    │
  └────────────────────────────────────────────────────────────┘

EOF

sleep 1

# Create tmux session with two panes
tmux new-session -d -s "$SESSION_NAME" -x 220 -y 50

# Right pane (30%): presenter log tail
tmux split-window -h -p 30 -t "$SESSION_NAME"
tmux send-keys -t "$SESSION_NAME:0.1" \
  "echo '=== PRESENTER LOG ===' && tail -f '${PRESENTER_LOG}'" Enter

# Left pane (70%): run the flow
tmux select-pane -t "$SESSION_NAME:0.0"
tmux send-keys -t "$SESSION_NAME:0.0" "$FLOW_CMD" Enter

# Attach
tmux attach-session -t "$SESSION_NAME"
```

### Step 2: Make executable

```bash
chmod +x demo/scripts/present.sh
```

### Step 3: Verify script parses cleanly

```bash
bash -n demo/scripts/present.sh
```
Expected: no output, exit 0.

### Step 4: Test tmux-less fallback

```bash
# Temporarily rename tmux to test the fallback path
PATH_WITHOUT_TMUX=$(echo "$PATH" | tr ':' '\n' | grep -v "^$" | head -5 | tr '\n' ':')
# Just verify the script's "tmux not found" branch by checking the help message:
bash demo/scripts/present.sh 2>&1 | head -5
```
Expected: usage message printed.

### Step 5: Commit

```bash
git add demo/scripts/present.sh
git commit -m "feat(demo): add present.sh tmux two-pane launcher"
```

---

## Task 8: Write presenter notes files

**Files:**
- Create: `demo/notes/flow-1-notes.md`
- Create: `demo/notes/flow-2-notes.md`
- Create: `demo/notes/flow-3-notes.md`
- Create: `demo/notes/flow-4-notes.md`

These are printed reference cards — one section per beat — to have open on a second screen or printed out. The inline `_beat "note"` arguments in the scripts are the short version; these files are the expanded version.

### Step 1: Create notes directory

```bash
mkdir -p demo/notes
```

### Step 2: Write each notes file

Each file follows this template:

```markdown
# Flow N — Presenter Notes
**Flow**: [name]
**Audience**: Cooperative members, organizers, community leaders (non-technical)
**Duration**: ~15 minutes with pauses
**Key message**: [one sentence]

---

## Beat: [narrate text]
**Say**: [1-3 sentences to speak aloud]
**Point to**: [what on screen to gesture at]
**If asked**: [anticipated question and short answer]

---
```

Write the notes files using the beat content from the `_beat` arguments in Tasks 3–6 as the source, expanded into full speaking notes with "If asked" entries for likely audience questions.

Key anticipated questions to address across the flows:
- "What is a DID?" → "It's a cryptographic identity — like a membership card where only you hold the key. No username/password."
- "What does 'on-chain' mean?" → "It means the record is held across the network, not in one company's database. Nobody can delete or alter it."
- "Who runs this?" → "The cooperative runs it. ICN is software they run themselves — there's no ICN Inc. in the middle."
- "Is this a blockchain?" → "It uses similar ideas — distributed, tamper-evident records — but it's purpose-built for cooperatives, not for trading tokens."

### Step 3: Commit

```bash
git add demo/notes/
git commit -m "docs(demo): add presenter notes for all four flows"
```

---

## Task 9: End-to-end smoke test in both modes

**Goal**: Confirm all four flows run cleanly in `--present` and `--narrated` mode before the summit.

### Step 1: Run the lib smoke test

```bash
bash demo/scripts/lib-demo-ports.sh.test
```
Expected: all PASS, including new presenter mode tests.

### Step 2: Dry-run all four flows in narrated mode with 0-second pauses

This tests all four flows without needing the live cluster:

```bash
# We expect these to fail at the port-forward/ready step since we're not
# spinning up the cluster here — but they should get past argument parsing
# and the first beat before failing. Verify no syntax errors.
for f in flow-1-governance flow-2-patronage flow-3-federation flow-4-reporting; do
  echo "=== $f syntax ==="
  bash -n "demo/scripts/${f}.sh" && echo "OK"
done
```
Expected: all four print `OK`.

### Step 3: Live test against cluster (flow-2 only — the most complete)

With the K3s cluster running and reseed done:

```bash
cd demo && bash scripts/reseed-federation-demo.sh 2>&1 | tail -5
DEMO_BEAT_PAUSE=0 bash scripts/flow-2-patronage.sh --narrated 2>&1 | grep -E "^  [✓⚠✗▶]"
```

Expected: clean story output with no HTTP codes, no JSON, no `→ aside` lines.
Verify the PRESENTER_LOG was populated:

```bash
ls -la /tmp/icn-demo-presenter-*.log 2>/dev/null | tail -3
```
Expected: log file exists and has content.

### Step 4: Test present.sh launcher (requires tmux)

```bash
# Just verify it launches and the session exists
bash demo/scripts/present.sh flow-2-patronage --narrated &
sleep 3
tmux list-sessions 2>/dev/null | grep icn-demo && echo "SESSION OK"
tmux kill-server 2>/dev/null || true
```
Expected: `SESSION OK` printed.

### Step 5: Final commit

```bash
git add -A
git commit -m "test(demo): end-to-end smoke test for presenter mode"
```

---

## Summary

After all tasks complete:

```
demo/
  scripts/
    lib-demo-ports.sh          ✓ _beat, PRESENTER_MODE, overridden helpers
    lib-demo-ports.sh.test     ✓ presenter mode tests added
    flow-1-governance.sh       ✓ --present/--narrated + _beat calls
    flow-2-patronage.sh        ✓ same
    flow-3-federation.sh       ✓ same
    flow-4-reporting.sh        ✓ same
    present.sh                 ✓ NEW — tmux two-pane launcher
  notes/
    flow-1-notes.md            ✓ NEW — presenter talking points
    flow-2-notes.md            ✓ same
    flow-3-notes.md            ✓ same
    flow-4-notes.md            ✓ same
  docs/plans/
    2026-03-09-terminal-presenter-mode-design.md  ✓ design doc
    2026-03-09-terminal-presenter-mode-plan.md    ✓ this file
```

Success criteria from design doc:
- [ ] `bash flow-2-patronage.sh --present` — clean output, keypress control
- [ ] `bash flow-2-patronage.sh --narrated` — clean output, 4s auto-advance
- [ ] `bash present.sh flow-2-patronage --present` — tmux two-pane layout
- [ ] Presenter sidebar shows full technical log in real time
- [ ] All four flows work in both modes
- [ ] No flag = original behavior, no regression
- [ ] Presenter notes exist for all four flows
