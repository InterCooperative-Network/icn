#!/usr/bin/env bash
# PreToolUse hook: Warns when editing files outside the scope implied by the branch of the worktree
# that RECEIVES the edit. Non-blocking: advisory only (always exits 0).
#
# Branch naming convention: feat/<scope>-*, fix/<scope>-*, etc.
# Scope is inferred from the first path segment after the prefix.
# Example: fix/ledger-overflow -> scope=ledger -> warn if editing icn-trust/
#
# Target resolution (not the session's working directory):
#   1. A relative tool_input.file_path is resolved against the hook input's `cwd`; with no `cwd`
#      the target is unresolved and the hook stays silent.
#   2. A symlink at the final component is followed (a write goes where the link points).
#   3. The nearest existing ancestor directory is canonicalized physically (cd -P), so symlinked
#      directories resolve to the worktree that really holds the file. Components that do not exist
#      yet (a new file in new directories) are appended; '.' or '..' among them leaves the target
#      unresolved.
#   4. The worktree is `git rev-parse --show-toplevel` of that ancestor, and the branch is that
#      worktree's `git branch --show-current`. A detached HEAD has no branch and is never treated as main.
#      A path outside any git worktree is out of scope.
# Unresolved targets, malformed input and missing tools produce no output.
#
# Output: nothing, or exactly ONE JSON object serialized by jq: {"continue":true,"systemMessage":...}.

set -uo pipefail

command -v jq >/dev/null 2>&1 || exit 0
command -v git >/dev/null 2>&1 || exit 0
# Inherited repository overrides would make `git -C` ignore the target's own worktree.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_OBJECT_DIRECTORY GIT_NAMESPACE

INPUT=$(cat)
field() {
  printf '%s' "$INPUT" | jq -r "if type == \"object\" then ($1 | strings) else empty end" 2>/dev/null
}
FILE_PATH=$(field '(if (.tool_input | type) == "object" then .tool_input.file_path else null end)')
INPUT_CWD=$(field '.cwd')

if [[ -z "$FILE_PATH" ]]; then
  exit 0
fi

# Resolve FILE_PATH. Sets TARGET (canonical absolute path) and ANCHOR (its nearest existing,
# canonical ancestor directory). Returns 1 when the target cannot be resolved.
TARGET=""
ANCHOR=""
resolve_target() {
  local p=$1 hops=0 link dir rest
  if [[ "$p" != /* ]]; then
    [[ "$INPUT_CWD" == /* ]] || return 1
    p="$INPUT_CWD/$p"
  fi
  while [[ -L "$p" ]]; do
    hops=$((hops + 1))
    ((hops <= 40)) || return 1
    link=$(readlink -- "$p") || return 1
    if [[ "$link" == /* ]]; then p=$link; else p="${p%/*}/$link"; fi
  done
  dir=${p%/*}
  rest=${p##*/}
  [[ -n "$dir" ]] || dir=/
  while [[ ! -d "$dir" ]]; do
    rest="${dir##*/}/$rest"
    dir=${dir%/*}
    [[ -n "$dir" ]] || dir=/
  done
  case "/$rest/" in
    */./* | */../*) return 1 ;;
  esac
  dir=$(cd -P -- "$dir" 2>/dev/null && pwd -P) || return 1
  ANCHOR=$dir
  TARGET="${dir%/}/$rest"
}
resolve_target "$FILE_PATH" || exit 0

WORKTREE=$(git -C "$ANCHOR" rev-parse --show-toplevel 2>/dev/null) || exit 0
[[ -n "$WORKTREE" ]] || exit 0
WORKTREE=$(cd -P -- "$WORKTREE" 2>/dev/null && pwd -P) || exit 0
case "$TARGET" in
  "$WORKTREE"/*) REL_PATH=${TARGET#"$WORKTREE"/} ;;
  *) exit 0 ;;
esac

# The branch of the worktree that receives the edit. Detached HEAD: no branch, no scope.
# (`branch --show-current`, not `symbolic-ref --short`, which yields "heads/<name>" when a tag
# shares the branch's name.)
BRANCH=$(git -C "$WORKTREE" branch --show-current 2>/dev/null) || BRANCH=""

# ---------------------------------------------------------------------------------------------
# Scope policy (unchanged): the same prefixes, known scopes and matching rules as before.
# ---------------------------------------------------------------------------------------------

# Skip on branches where scope enforcement doesn't apply
if [[ -z "$BRANCH" ]] || [[ "$BRANCH" == "main" ]] || [[ "$BRANCH" == "develop" ]]; then
  exit 0
fi

# Only check branches with a type prefix (feat/, fix/, refactor/, etc.)
if ! printf '%s\n' "$BRANCH" | grep -qE '^(feat|fix|refactor|docs|chore|test|ci)/'; then
  exit 0
fi

# Extract full scope segment from branch name (e.g. fix/http-kit-refactor -> http-kit-refactor)
# Using the full segment (not just up to the first '-') allows matching hyphenated scopes.
SCOPE_SEGMENT=$(printf '%s\n' "$BRANCH" | sed -E 's|^[^/]+/([^/]+).*|\1|')

if [[ -z "$SCOPE_SEGMENT" ]] || [[ ${#SCOPE_SEGMENT} -lt 3 ]]; then
  exit 0
fi

# Build list of known ICN crate scopes
KNOWN_SCOPES="core identity trust net gossip ledger ccl store rpc obs gateway governance compute security time snapshot crypto steward zkp community coop entity encoding api naming authz federation privacy protocol services http-kit"

# Find the longest matching known scope (prefix match against full segment).
# Longest match handles hyphenated scopes correctly: "http-kit-refactor" matches "http-kit", not "http".
MATCHED_SCOPE=""
LONGEST_MATCH_LEN=0
for scope in $KNOWN_SCOPES; do
  if [[ "$SCOPE_SEGMENT" == "$scope"* ]]; then
    if ((${#scope} > LONGEST_MATCH_LEN)); then
      MATCHED_SCOPE="$scope"
      LONGEST_MATCH_LEN=${#scope}
    fi
  fi
done

if [[ -z "$MATCHED_SCOPE" ]]; then
  exit 0
fi

# Crate of the file being edited, from its path inside the worktree
# (e.g. icn/crates/icn-trust/src/... -> trust)
FILE_CRATE=""
if printf '%s\n' "$REL_PATH" | grep -qE 'crates/icn-([^/]+)/'; then
  FILE_CRATE=$(printf '%s\n' "$REL_PATH" | sed 's|.*crates/icn-\([^/]*\)/.*|\1|')
fi

if [[ -z "$FILE_CRATE" ]]; then
  exit 0
fi

# Warn if editing a crate that doesn't match scope
if [[ "$FILE_CRATE" != "$MATCHED_SCOPE"* ]] && [[ "$MATCHED_SCOPE" != "$FILE_CRATE"* ]]; then
  jq -nc --arg branch "$BRANCH" --arg scope "$MATCHED_SCOPE" --arg crate "$FILE_CRATE" --arg worktree "$WORKTREE" '{
    continue: true,
    systemMessage: ("Scope warning: branch '"'"'" + $branch + "'"'"' (worktree " + $worktree + ") suggests scope '"'"'" + $scope
      + "'"'"' but you are editing '"'"'icn-" + $crate
      + "'"'"'. If this is intentional (e.g. fixing a downstream consumer), ignore this warning. If not, confirm you are on the correct branch.")
  }'
fi

exit 0
