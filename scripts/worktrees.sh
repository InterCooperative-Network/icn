#!/usr/bin/env bash
# Multi-agent Git worktree helper for ICN.
# Creates, lists, and removes agent worktrees in a sibling directory.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
WT_DIR="$(cd "${REPO_ROOT}/.." && pwd)/icn-wt"

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

Worktrees are created at: ${WT_DIR}/<name>
Branches are named:       feat/<name>
Base branch:              origin/main
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

    if [ -d "$wt_path" ]; then
        echo "Error: worktree already exists at ${wt_path}" >&2
        exit 1
    fi

    if git -C "$REPO_ROOT" show-ref --verify --quiet "refs/heads/${branch}" 2>/dev/null; then
        echo "Error: local branch '${branch}' already exists" >&2
        echo "  To use the existing branch: git worktree add ${wt_path} ${branch}" >&2
        exit 1
    fi

    echo "Fetching origin..."
    git -C "$REPO_ROOT" fetch origin

    mkdir -p "$WT_DIR"
    git -C "$REPO_ROOT" worktree add "$wt_path" -b "$branch" origin/main

    echo
    echo "Created worktree:"
    echo "  Path:   ${wt_path}"
    echo "  Branch: ${branch}"
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

    if [ ! -d "$wt_path" ]; then
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
