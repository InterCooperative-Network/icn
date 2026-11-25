#!/bin/bash
# Regression tests for .claude/hooks/scope-guard.sh.
#
# Builds a throwaway git repository with several worktrees under a fresh temporary directory, then
# feeds synthetic PreToolUse payloads to the hook, launching it from one directory while the edit
# targets another. No repository outside the temporary directory is read or written.
#
# Contract checked: the hook always exits 0 and prints either nothing or exactly ONE valid JSON object
# {"continue":true,"systemMessage":...} naming the branch of the worktree that RECEIVES the edit.
#
# Usage: test_scope_guard.sh [path/to/scope-guard.sh]   (default: the sibling ../scope-guard.sh)
set -u

HOOK=${1:-"$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/scope-guard.sh"}
command -v jq >/dev/null 2>&1 || { echo "jq is required to run these tests" >&2; exit 2; }
command -v git >/dev/null 2>&1 || { echo "git is required to run these tests" >&2; exit 2; }
[[ -f "$HOOK" ]] || { echo "hook not found: $HOOK" >&2; exit 2; }
HOOK=$(cd "$(dirname "$HOOK")" && pwd -P)/$(basename "$HOOK")

T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
T=$(cd -P "$T" && pwd -P)
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
g() { git -c user.name=t -c user.email=t@example.invalid -c init.defaultBranch=main "$@" >/dev/null 2>&1; }

# --- fixture --------------------------------------------------------------------------------------
R="$T/repo"
mkdir -p "$R/icn/crates/icn-ledger/src" "$R/icn/crates/icn-trust/src" "$R/icn/crates/icn-http-kit/src"
for c in ledger trust http-kit; do echo "// $c" >"$R/icn/crates/icn-$c/src/lib.rs"; done
g init "$R" && g -C "$R" add -A && g -C "$R" commit -m init
LEDGER="$T/wt-ledger"     # feat/ledger-x
TRUST="$T/wt trust"       # fix/trust-y   (path contains a space)
DETACHED="$T/wt-detached" # detached HEAD
QUOTE="$T/wt-quote"       # feat/ledger"q (quote in the branch name)
HTTP="$T/wt-http"         # fix/http-kit-refactor
WIP="$T/wt-wip"           # wip-ledger    (no type prefix)
g -C "$R" worktree add -b feat/ledger-x "$LEDGER"
g -C "$R" worktree add -b fix/trust-y "$TRUST"
g -C "$R" worktree add --detach "$DETACHED"
g -C "$R" worktree add -b 'feat/ledger"q' "$QUOTE"
g -C "$R" worktree add -b fix/http-kit-refactor "$HTTP"
g -C "$R" worktree add -b wip-ledger "$WIP"
INNER="$TRUST/vendor/inner" # a nested repository on feat/gossip-z
mkdir -p "$INNER/crates/icn-ledger/src" && echo x >"$INNER/crates/icn-ledger/src/lib.rs"
g init "$INNER" && g -C "$INNER" checkout -b feat/gossip-z && g -C "$INNER" add -A && g -C "$INNER" commit -m inner
OUTSIDE="$T/outside" && mkdir -p "$OUTSIDE/icn/crates/icn-trust/src"
# symlinks inside the ledger worktree that point into the trust worktree
ln -s "$TRUST/icn/crates/icn-ledger/src/lib.rs" "$LEDGER/icn/link-file.rs"
ln -s "$TRUST/icn/crates" "$LEDGER/icn/trust-crates"
ln -s "$TRUST/icn/crates/icn-ledger/src/not-yet.rs" "$LEDGER/icn/dangling.rs"
ln -s "$LEDGER/icn/loop-b.rs" "$LEDGER/icn/loop-a.rs" && ln -s "$LEDGER/icn/loop-a.rs" "$LEDGER/icn/loop-b.rs"
[[ "$(git -C "$TRUST" symbolic-ref --short HEAD)" == fix/trust-y ]] || { echo "fixture setup failed" >&2; exit 2; }

# --- harness --------------------------------------------------------------------------------------
PASS=0
FAIL=0
# run <launch dir> <stdin> [env assignments...]
run() {
  local launch=$1 payload=$2
  shift 2
  (cd "$launch" && printf '%s' "$payload" | env "$@" bash "$HOOK" >"$T/out" 2>"$T/err")
  RC=$?
  OUT=$(cat "$T/out")
}
pay() { jq -nc --arg p "$1" '{tool_name:"Edit",tool_input:{file_path:$p,old_string:"a",new_string:"b"}}'; }
pay_cwd() { jq -nc --arg p "$1" --arg c "$2" '{cwd:$c,tool_name:"Write",tool_input:{file_path:$p,content:"x"}}'; }
ok() { PASS=$((PASS + 1)); printf 'ok    %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); printf 'FAIL  %s: %s (rc=%s stdout=%q)\n' "$1" "$2" "$RC" "${OUT:0:200}"; }

expect_silent() { # name launch payload [env...]
  local name=$1
  shift
  run "$@"
  if [[ $RC -ne 0 ]]; then bad "$name" "expected exit 0"
  elif [[ -n "$OUT" ]]; then bad "$name" "expected no output"
  else ok "$name"; fi
}

expect_warn() { # name branch crate launch payload [env...]
  local name=$1 branch=$2 crate=$3
  shift 3
  run "$@"
  if [[ $RC -ne 0 ]]; then bad "$name" "expected exit 0"; return; fi
  local n msg
  n=$(printf '%s' "$OUT" | jq -s length 2>/dev/null) || { bad "$name" "stdout is not valid JSON"; return; }
  [[ "$n" == 1 ]] || { bad "$name" "expected exactly one JSON object, got $n"; return; }
  printf '%s' "$OUT" | jq -e '.continue == true and (.systemMessage | type) == "string"' >/dev/null ||
    { bad "$name" "object lacks continue:true / string systemMessage"; return; }
  msg=$(printf '%s' "$OUT" | jq -r .systemMessage)
  [[ "$msg" == "Scope warning: branch '$branch' "* ]] || { bad "$name" "message does not name branch '$branch'"; return; }
  [[ "$msg" == *"editing 'icn-$crate'"* ]] || { bad "$name" "message does not name crate icn-$crate"; return; }
  ok "$name"
}

L_LEDGER="icn/crates/icn-ledger/src/lib.rs"
L_TRUST="icn/crates/icn-trust/src/lib.rs"

echo "# launch from one worktree, target another"
expect_warn   "ledger wt launches, trust wt receives a ledger edit" fix/trust-y ledger "$LEDGER" "$(pay "$TRUST/$L_LEDGER")"
expect_silent "ledger wt launches, trust wt receives a trust edit"             "$LEDGER" "$(pay "$TRUST/$L_TRUST")"
expect_warn   "trust wt launches, ledger wt receives a trust edit" feat/ledger-x trust "$TRUST" "$(pay "$LEDGER/$L_TRUST")"
expect_silent "trust wt launches, ledger wt receives a ledger edit"            "$TRUST" "$(pay "$LEDGER/$L_LEDGER")"
expect_warn   "launched outside any repository"                    fix/trust-y ledger "$OUTSIDE" "$(pay "$TRUST/$L_LEDGER")"
expect_warn   "inherited GIT_DIR of another worktree is ignored"   fix/trust-y ledger "$LEDGER" "$(pay "$TRUST/$L_LEDGER")" "GIT_DIR=$LEDGER/.git" "GIT_WORK_TREE=$LEDGER"

echo "# relative paths"
expect_warn   "relative path resolved against input cwd"           fix/trust-y ledger "$LEDGER" "$(pay_cwd "$L_LEDGER" "$TRUST")"
expect_warn   "relative path with .. through existing dirs"        fix/trust-y ledger "$LEDGER" "$(pay_cwd "../wt trust/$L_LEDGER" "$LEDGER")"
expect_silent "relative path without input cwd is unresolved"                  "$TRUST" "$(pay "$L_LEDGER")"
expect_silent "relative cwd is unresolved"                                     "$TRUST" "$(pay_cwd "$L_LEDGER" "wt trust")"

echo "# nonexistent targets and nested directories"
expect_warn   "new file in new nested directories"                 fix/trust-y ledger "$LEDGER" "$(pay "$TRUST/icn/crates/icn-ledger/src/new/deep/x.rs")"
expect_warn   "new crate directory"                                fix/trust-y gossip "$LEDGER" "$(pay "$TRUST/icn/crates/icn-gossip/src/a.rs")"
expect_silent ".. inside the not-yet-existing part is unresolved"              "$LEDGER" "$(pay "$TRUST/icn/crates/nope/../icn-ledger/src/lib.rs")"
expect_warn   "nested repository: innermost worktree wins"         feat/gossip-z ledger "$LEDGER" "$(pay "$INNER/crates/icn-ledger/src/lib.rs")"

echo "# symlinks (attributed to where the write lands)"
expect_warn   "symlinked file into another worktree"               fix/trust-y ledger "$LEDGER" "$(pay "$LEDGER/icn/link-file.rs")"
expect_warn   "symlinked directory into another worktree"          fix/trust-y ledger "$LEDGER" "$(pay "$LEDGER/icn/trust-crates/icn-ledger/src/lib.rs")"
expect_warn   "dangling symlink to a new file"                     fix/trust-y ledger "$LEDGER" "$(pay "$LEDGER/icn/dangling.rs")"
expect_silent "symlink loop is unresolved"                                     "$LEDGER" "$(pay "$LEDGER/icn/loop-a.rs")"

echo "# branches without a scope"
expect_silent "detached HEAD is not main and has no scope"                     "$LEDGER" "$(pay "$DETACHED/$L_TRUST")"
expect_silent "main branch"                                                    "$LEDGER" "$(pay "$R/$L_TRUST")"
expect_silent "branch without a type prefix"                                   "$LEDGER" "$(pay "$WIP/$L_TRUST")"
expect_silent "path outside any repository"                                    "$LEDGER" "$(pay "$OUTSIDE/$L_TRUST")"
expect_silent "file outside a crates/icn-* path"                               "$LEDGER" "$(pay "$TRUST/README.md")"

echo "# existing scope policy preserved"
expect_silent "hyphenated scope http-kit on icn-http-kit"                      "$LEDGER" "$(pay "$HTTP/icn/crates/icn-http-kit/src/lib.rs")"
expect_warn   "hyphenated scope http-kit on icn-ledger"            fix/http-kit-refactor ledger "$LEDGER" "$(pay "$HTTP/$L_LEDGER")"
g -C "$R" tag fix/trust-y
expect_warn   "tag with the branch's name does not hide it"        fix/trust-y ledger "$LEDGER" "$(pay "$TRUST/$L_LEDGER")"
expect_warn   "quote in branch name is escaped"                   'feat/ledger"q' trust "$LEDGER" "$(pay "$QUOTE/$L_TRUST")"

echo "# malformed input (silent, exit 0)"
expect_silent "empty stdin"                 "$LEDGER" ''
expect_silent "not JSON"                    "$LEDGER" 'nope {'
expect_silent "JSON array"                  "$LEDGER" '[1]'
expect_silent "tool_input is a string"      "$LEDGER" '{"tool_input":"x"}'
expect_silent "file_path is a number"       "$LEDGER" '{"tool_input":{"file_path":5}}'
expect_silent "cwd is an object"            "$LEDGER" "$(jq -nc --arg p "$L_LEDGER" '{cwd:{},tool_input:{file_path:$p}}')"

echo
echo "passed=$PASS failed=$FAIL"
[[ $FAIL -eq 0 ]]
