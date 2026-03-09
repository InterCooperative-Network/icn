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

cat << 'EOF'

  ┌────────────────────────────────────────────────────────────┐
  │  ICN DEMO PRESENTER                                        │
EOF
printf "  │  Flow:    %-49s│\n" "${FLOW_NAME}"
printf "  │  Mode:    %-49s│\n" "${MODE}"
printf "  │  Session: %-49s│\n" "${SESSION_NAME}"
cat << 'EOF'
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
