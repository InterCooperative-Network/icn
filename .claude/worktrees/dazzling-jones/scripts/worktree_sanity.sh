#!/usr/bin/env bash
# Validate manager and dedicated main worktree are aligned with remote main.

set -euo pipefail

REMOTE="${ICN_WT_REMOTE:-origin}"

if ! ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    echo "FAIL: not inside a git repository"
    exit 1
fi

WT_DIR="${ICN_WT_DIR:-$(cd "${ROOT}/.." && pwd)/icn-wt}"
WT_MAIN_PATH="${WT_DIR}/main"

failures=0

pass() {
    echo "PASS: $*"
}

fail() {
    echo "FAIL: $*"
    failures=1
}

echo "== Worktree Sanity Check =="
echo "repo_root: ${ROOT}"
echo "remote: ${REMOTE}"
echo "wt_dir: ${WT_DIR}"

if [ -f "${ROOT}/scripts/pilot_chain_demo.sh" ] && [ -f "${ROOT}/scripts/pilot_smoke.sh" ]; then
    echo "WARN: multiple pilot smoke scripts found; preferred entrypoint: scripts/pilot_chain_demo.sh (see docs/ops/runbooks/07-pilot-vertical-slice-smoke.md)"
fi

if ! git -C "${ROOT}" remote get-url "${REMOTE}" >/dev/null 2>&1; then
    fail "remote '${REMOTE}' is not configured"
    echo "Fix: git -C \"${ROOT}\" remote add ${REMOTE} <url>"
    exit 1
fi

# Keep checks accurate against latest remote refs.
git -C "${ROOT}" fetch "${REMOTE}" --prune >/dev/null

if ! git -C "${ROOT}" show-ref --verify --quiet "refs/remotes/${REMOTE}/main"; then
    fail "${REMOTE}/main does not exist"
    echo "Fix: git -C \"${ROOT}\" fetch ${REMOTE} --prune"
    exit 1
fi

if ! git -C "${ROOT}" show-ref --verify --quiet "refs/heads/main"; then
    fail "local branch 'main' is missing"
    echo "Fix: git -C \"${ROOT}\" switch -c main --track ${REMOTE}/main"
    exit 1
fi

root_behind=$(git -C "${ROOT}" rev-list --count main.."${REMOTE}/main")
if [ "${root_behind}" -gt 0 ]; then
    fail "root main is behind ${REMOTE}/main by ${root_behind} commit(s)"
    echo "Fix:"
    echo "  git -C \"${ROOT}\" fetch ${REMOTE} --prune"
    echo "  git -C \"${ROOT}\" switch main"
    echo "  git -C \"${ROOT}\" pull --ff-only"
else
    pass "root main is up to date with ${REMOTE}/main"
fi

if [ ! -d "${WT_MAIN_PATH}" ] || ! git -C "${WT_MAIN_PATH}" rev-parse --show-toplevel >/dev/null 2>&1; then
    fail "dedicated main worktree missing at ${WT_MAIN_PATH}"
    echo "Fix:"
    echo "  git -C \"${ROOT}\" worktree add \"${WT_MAIN_PATH}\" -b wt-main ${REMOTE}/main"
    exit 1
fi

wt_branch=$(git -C "${WT_MAIN_PATH}" rev-parse --abbrev-ref HEAD)
if ! git -C "${WT_MAIN_PATH}" show-ref --verify --quiet "refs/heads/${wt_branch}"; then
    fail "worktree branch '${wt_branch}' is not a local branch"
    echo "Fix:"
    echo "  cd \"${WT_MAIN_PATH}\" && git checkout -b wt-main ${REMOTE}/main"
    exit 1
fi

wt_behind=$(git -C "${WT_MAIN_PATH}" rev-list --count "${wt_branch}".."${REMOTE}/main")
if [ "${wt_behind}" -gt 0 ]; then
    fail "${WT_MAIN_PATH} (${wt_branch}) is behind ${REMOTE}/main by ${wt_behind} commit(s)"
    echo "Fix:"
    echo "  cd \"${WT_MAIN_PATH}\" && git fetch ${REMOTE} --prune"
    echo "  cd \"${WT_MAIN_PATH}\" && git pull --ff-only"
else
    pass "${WT_MAIN_PATH} (${wt_branch}) is up to date with ${REMOTE}/main"
fi

if [ "${failures}" -ne 0 ]; then
    echo "RESULT: FAIL"
    exit 1
fi

echo "RESULT: PASS"
