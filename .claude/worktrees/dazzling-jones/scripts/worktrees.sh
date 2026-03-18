#!/usr/bin/env bash
# Multi-agent Git worktree helper for ICN.
# Creates, lists, and removes agent worktrees in a sibling directory.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Overridable via environment
WT_DIR="${ICN_WT_DIR:-$(cd "${REPO_ROOT}/.." && pwd)/icn-wt}"
WT_REMOTE="${ICN_WT_REMOTE:-origin}"
WT_BASE_REF="${ICN_WT_BASE_REF:-${WT_REMOTE}/main}"

usage() {
    cat <<EOF
Usage: $(basename "$0") <command> [args]

Commands:
  create <name>   Create a new agent worktree on branch feat/<name>
  list            List all worktrees
  remove <name>   Remove an agent worktree and its local branch
  prune           Clean up stale worktree references

Examples:
  $(basename "$0") create agent-d
  $(basename "$0") list
  $(basename "$0") remove agent-d

Environment:
  ICN_WT_DIR       Worktree base directory  (default: ${WT_DIR})
  ICN_WT_REMOTE    Git remote name          (default: ${WT_REMOTE})
  ICN_WT_BASE_REF  Base ref for new branches (default: ${WT_BASE_REF})
EOF
    exit 1
}

validate_name() {
    local name="$1"
    if ! [[ "$name" =~ ^[A-Za-z0-9._-]+$ ]]; then
        echo "Error: name must match [A-Za-z0-9._-]+ (got '${name}')" >&2
        exit 1
    fi
}

cmd_create() {
    local name="$1"
    validate_name "$name"
    local wt_path="${WT_DIR}/${name}"
    local branch="feat/${name}"

    if [ -e "$wt_path" ]; then
        echo "Error: path already exists at ${wt_path}" >&2
        exit 1
    fi

    if git -C "$REPO_ROOT" show-ref --verify --quiet "refs/heads/${branch}" 2>/dev/null; then
        echo "Error: local branch '${branch}' already exists" >&2
        echo "  To use the existing branch: git worktree add ${wt_path} ${branch}" >&2
        exit 1
    fi

    echo "Fetching ${WT_REMOTE}..."
    git -C "$REPO_ROOT" fetch "$WT_REMOTE"

    mkdir -p "$WT_DIR"
    git -C "$REPO_ROOT" worktree add "$wt_path" -b "$branch" "$WT_BASE_REF"

    echo
    echo "Created worktree:"
    echo "  Path:   ${wt_path}"
    echo "  Branch: ${branch}"
    echo "  Base:   ${WT_BASE_REF}"
    echo
    echo "Next steps:"
    echo "  cd ${wt_path}"
    echo "  cd icn && cargo build"
}

cmd_list() {
    git -C "$REPO_ROOT" worktree list
}

cmd_remove() {
    local name="$1"
    validate_name "$name"
    local wt_path="${WT_DIR}/${name}"
    local branch="feat/${name}"

    if [ ! -e "$wt_path" ]; then
        echo "Error: worktree not found at ${wt_path}" >&2
        exit 1
    fi

    echo "Removing worktree at ${wt_path}..."
    git -C "$REPO_ROOT" worktree remove "$wt_path"

    # Delete local branch if it exists and is fully merged
    if git -C "$REPO_ROOT" show-ref --verify --quiet "refs/heads/${branch}" 2>/dev/null; then
        echo "Deleting local branch ${branch}..."
        git -C "$REPO_ROOT" branch -d "$branch" 2>/dev/null || \
            echo "Warning: branch ${branch} not fully merged; use 'git branch -D ${branch}' to force delete"
    fi

    git -C "$REPO_ROOT" worktree prune
    echo "Done."
}

cmd_prune() {
    git -C "$REPO_ROOT" worktree prune -v
}

# --- Main ---

if [ $# -lt 1 ]; then
    usage
fi

command="$1"
shift

case "$command" in
    create)
        [ $# -lt 1 ] && { echo "Error: create requires a name" >&2; usage; }
        cmd_create "$1"
        ;;
    list)
        cmd_list
        ;;
    remove)
        [ $# -lt 1 ] && { echo "Error: remove requires a name" >&2; usage; }
        cmd_remove "$1"
        ;;
    prune)
        cmd_prune
        ;;
    *)
        echo "Unknown command: ${command}" >&2
        usage
        ;;
esac
