#!/usr/bin/env bash
# test-check-pr-stack.sh — fixture-based tests for scripts/check-pr-stack.sh.
#
# Everything runs --offline against throwaway fixture repo-roots, so the tests
# need no network, no gh, and never touch real manifests except the final
# read-only pass over this repo's own stacks/ directory.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECKER="${SCRIPT_DIR}/check-pr-stack.sh"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

TMP="$(mktemp -d /tmp/icn-pr-stack-test-XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

PASS=0
FAIL=0

# expect <name> <expected-exit> <fixture-root> [extra checker args...]
expect() {
  local name="$1" want="$2" root="$3"
  shift 3
  local got=0
  bash "$CHECKER" --repo-root "$root" --offline "$@" >"$TMP/out.log" 2>&1 || got=$?
  if [ "$got" -eq "$want" ]; then
    echo "  ok  $name (exit $got)"
    PASS=$((PASS + 1))
  else
    echo "  FAIL  $name: expected exit $want, got $got"
    sed 's/^/        /' "$TMP/out.log"
    FAIL=$((FAIL + 1))
  fi
}

mkfixture() { # mkfixture <name> -> prints root; manifest body on stdin
  local root="$TMP/$1"
  mkdir -p "$root/ops/coordination/stacks"
  cat > "$root/ops/coordination/stacks/$2"
  echo "$root"
}

echo "test-check-pr-stack"

# 1. Good manifest: upstream merged, downstream planned.
root="$(mkfixture good good-stack.stack.yaml <<'EOF'
schema: icn-pr-stack/v1
stack_id: good-stack
anchor_issue: example-org/example#1
stages:
  - stage: 1
    repo: example-org/example
    status: merged
    merged_prs: [10]
  - stage: 2
    repo: example-org/downstream
    status: planned
    merged_prs: []
EOF
)"
expect "good manifest passes" 0 "$root"
expect "good manifest passes strict" 0 "$root" --strict

# 2. Malformed: missing anchor_issue.
root="$(mkfixture no-anchor no-anchor.stack.yaml <<'EOF'
schema: icn-pr-stack/v1
stack_id: no-anchor
stages:
  - stage: 1
    repo: example-org/example
    status: planned
EOF
)"
expect "missing anchor_issue is malformed" 2 "$root"

# 3. Malformed: bad schema line.
root="$(mkfixture bad-schema bad-schema.stack.yaml <<'EOF'
schema: something-else/v9
stack_id: bad-schema
anchor_issue: example-org/example#1
stages:
  - stage: 1
    repo: example-org/example
    status: planned
EOF
)"
expect "wrong schema is malformed" 2 "$root"

# 4. Malformed: status outside the vocabulary.
root="$(mkfixture bad-status bad-status.stack.yaml <<'EOF'
schema: icn-pr-stack/v1
stack_id: bad-status
anchor_issue: example-org/example#1
stages:
  - stage: 1
    repo: example-org/example
    status: shipped
EOF
)"
expect "unknown status is malformed" 2 "$root"

# 5. Malformed: filename does not match stack_id.
root="$(mkfixture wrong-name mismatch.stack.yaml <<'EOF'
schema: icn-pr-stack/v1
stack_id: something-else
anchor_issue: example-org/example#1
stages:
  - stage: 1
    repo: example-org/example
    status: planned
EOF
)"
expect "filename/stack_id mismatch is malformed" 2 "$root"

# 6. Downstream merged before upstream: warning-mode passes, strict fails.
root="$(mkfixture out-of-order out-of-order.stack.yaml <<'EOF'
schema: icn-pr-stack/v1
stack_id: out-of-order
anchor_issue: example-org/example#1
stages:
  - stage: 1
    repo: example-org/example
    status: planned
  - stage: 2
    repo: example-org/downstream
    status: merged
EOF
)"
expect "downstream-before-upstream warns (non-strict exit 0)" 0 "$root"
expect "downstream-before-upstream fails under --strict" 1 "$root" --strict

# 7. Explicit depends_on_stages overrides the default ordering.
root="$(mkfixture explicit-deps explicit-deps.stack.yaml <<'EOF'
schema: icn-pr-stack/v1
stack_id: explicit-deps
anchor_issue: example-org/example#1
stages:
  - stage: 1
    repo: example-org/example
    status: merged
  - stage: 2
    repo: example-org/dashboard
    status: planned
    depends_on_stages: [1, 3]
  - stage: 3
    repo: example-org/downstream
    status: merged
    depends_on_stages: [1]
EOF
)"
expect "stage 3 merged before stage 2 is fine with explicit deps" 0 "$root" --strict

# 8. depends_on_stages referencing a missing stage is malformed.
root="$(mkfixture bad-dep bad-dep.stack.yaml <<'EOF'
schema: icn-pr-stack/v1
stack_id: bad-dep
anchor_issue: example-org/example#1
stages:
  - stage: 1
    repo: example-org/example
    status: planned
    depends_on_stages: [7]
EOF
)"
expect "dep on missing stage is malformed" 2 "$root"

# 9. Close-keyword next to an issue number in manifest text.
root="$(mkfixture close-kw close-kw.stack.yaml <<'EOF'
schema: icn-pr-stack/v1
stack_id: close-kw
anchor_issue: example-org/example#1
stages:
  - stage: 1
    repo: example-org/example
    status: planned
    note: "Closes #1 when done"
EOF
)"
expect "close-keyword in manifest warns (non-strict exit 0)" 0 "$root"
expect "close-keyword in manifest fails under --strict" 1 "$root" --strict

# 10. --stack filter: unknown id is an error, matching id checks one manifest.
root="$(mkfixture filter filter.stack.yaml <<'EOF'
schema: icn-pr-stack/v1
stack_id: filter
anchor_issue: example-org/example#1
stages:
  - stage: 1
    repo: example-org/example
    status: planned
EOF
)"
expect "--stack with unknown id errors" 2 "$root" --stack nope
expect "--stack with matching id passes" 0 "$root" --stack filter

# 11. The real repo manifests validate offline.
expect "real repo manifests pass offline" 0 "$REPO_ROOT"

echo
echo "test-check-pr-stack: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
