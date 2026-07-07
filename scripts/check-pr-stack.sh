#!/usr/bin/env bash
# check-pr-stack.sh — validate cross-repo PR stack manifests (Refs icn#2141).
#
# Companion to ops/coordination/PR_STACK_PROTOCOL.md. Checks every
# ops/coordination/stacks/*.stack.yaml (format icn-pr-stack/v1):
#
#   offline (always):
#     - required keys present, stack_id matches filename, statuses in vocabulary
#     - stage numbers unique; depends_on_stages reference existing earlier stages
#     - no stage marked merged/adopted before the stages it depends on are
#     - no GitHub close-keyword (close/fix/resolve...) next to an issue number
#       anywhere in the manifest text
#   online (needs authenticated gh; skipped with --offline or when gh is absent):
#     - anchor issue exists (its open/closed state is reported, not judged)
#     - every merged_prs entry exists and is actually MERGED
#     - each such PR body Refs the anchor issue and has no close-keyword near it
#     - inaccessible (private/access-gated) repos are reported as UNVERIFIED,
#       never guessed at
#
# Manifests are coordination metadata, not architectural or runtime truth —
# ops/state/truth/sources.json stays the arbiter. This is a planner's checker,
# not an enforcement gate; it is run manually / from VM sessions, not CI.
#
# Exit codes: 0 ok (warnings allowed) · 1 findings under --strict · 2 malformed
# manifest or usage error.
set -euo pipefail

usage() {
  echo "usage: check-pr-stack.sh [--repo-root DIR] [--stack ID] [--offline] [--strict]"
}

REPO_ROOT="."
STRICT=0
OFFLINE=0
ONLY_STACK=""
while [ $# -gt 0 ]; do
  case "$1" in
    --repo-root) REPO_ROOT="${2:?--repo-root needs a value}"; shift 2 ;;
    --stack)     ONLY_STACK="${2:?--stack needs a value}"; shift 2 ;;
    --offline)   OFFLINE=1; shift ;;
    --strict)    STRICT=1; shift ;;
    -h|--help)   usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

cd "$REPO_ROOT" || { echo "FAIL cannot cd to repo root: $REPO_ROOT" >&2; exit 2; }
STACKS_DIR="ops/coordination/stacks"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

FINDINGS=0
MALFORMED=0
pass()     { echo -e "${GREEN}  ok${NC}  $1"; }
note()     { echo -e "  --  $1"; }
finding()  { echo -e "${YELLOW}  !!${NC}  $1"; FINDINGS=$((FINDINGS + 1)); }
malformed(){ echo -e "${RED}  FAIL${NC}  $1"; MALFORMED=$((MALFORMED + 1)); }

# Close-keyword next to an issue number (GitHub auto-close syntax). Optional
# repo prefix (org/repo#N). Word-bounded so e.g. "prefix #12" does not match.
CLOSE_KW_RE='\b(close[sd]?|fix(e[sd])?|resolve[sd]?)\b[[:space:]:]+([A-Za-z0-9._/-]+)?#[0-9]+'
STATUS_ENUM='none|planned|in_progress|merged|adopted'

echo "check-pr-stack"

if [ ! -d "$STACKS_DIR" ]; then
  malformed "stacks directory missing: $STACKS_DIR"
  exit 2
fi

GH_OK=0
if [ "$OFFLINE" -eq 1 ]; then
  note "offline mode: gh-backed issue/PR checks skipped"
elif ! command -v gh >/dev/null 2>&1; then
  note "gh not installed — gh-backed checks skipped (shape validation only)"
elif ! gh auth status >/dev/null 2>&1; then
  note "gh not authenticated — gh-backed checks skipped (shape validation only)"
else
  GH_OK=1
fi

scalar() { # scalar <key> <file> — first top-level "key: value", quotes stripped
  awk -F': ' -v k="$1" '$1 == k { sub(/^[^:]+: */, ""); gsub(/^["'\'']|["'\'']$/, ""); print; exit }' "$2"
}

CHECKED=0
for manifest in "$STACKS_DIR"/*.stack.yaml; do
  [ -e "$manifest" ] || { malformed "no *.stack.yaml manifests in $STACKS_DIR"; break; }
  fname="$(basename "$manifest")"

  stack_id="$(scalar stack_id "$manifest")"
  if [ -n "$ONLY_STACK" ] && [ "$stack_id" != "$ONLY_STACK" ] && [ "$fname" != "$ONLY_STACK.stack.yaml" ]; then
    continue
  fi
  CHECKED=$((CHECKED + 1))
  echo "── $fname"

  schema="$(scalar schema "$manifest")"
  anchor="$(scalar anchor_issue "$manifest")"

  if [ "$schema" = "icn-pr-stack/v1" ]; then
    pass "schema: icn-pr-stack/v1"
  else
    malformed "schema must be icn-pr-stack/v1 (got: '${schema:-<missing>}')"
  fi

  if [ -z "$stack_id" ] || ! echo "$stack_id" | grep -qE '^[a-z0-9][a-z0-9-]*$'; then
    malformed "stack_id missing or not kebab-case (got: '${stack_id:-<missing>}')"
  elif [ "$fname" != "$stack_id.stack.yaml" ]; then
    malformed "filename $fname does not match stack_id '$stack_id'"
  else
    pass "stack_id matches filename"
  fi

  anchor_repo=""; anchor_num=""
  if echo "$anchor" | grep -qE '^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+#[0-9]+$'; then
    anchor_repo="${anchor%%#*}"
    anchor_num="${anchor##*#}"
    pass "anchor_issue: $anchor"
  else
    malformed "anchor_issue must be org/repo#N (got: '${anchor:-<missing>}')"
  fi

  # Close-keyword scan of the manifest text itself: a manifest must never
  # carry auto-close phrasing that could be pasted into a PR body.
  kw_hits="$(grep -inE "$CLOSE_KW_RE" "$manifest" || true)"
  if [ -n "$kw_hits" ]; then
    finding "close-keyword next to an issue number in manifest text:"
    echo "$kw_hits" | sed 's/^/        /'
  else
    pass "no close-keyword near issue numbers in manifest text"
  fi

  # Stage records: NUM|REPO|STATUS|DEPS|PRS ("-" = key absent).
  stage_rows="$(awk '
    function flush() { if (stage != "") printf "%s|%s|%s|%s|%s\n", stage, repo, status, deps, prs }
    function val(s) { sub(/^[^:]*:[[:space:]]*/, "", s); gsub(/^["'\''[:space:]]+|["'\''[:space:]]+$/, "", s); return s }
    /^stages:/ { in_s = 1; next }
    in_s && /^[^[:space:]#]/ { flush(); stage = ""; in_s = 0 }
    in_s && /^[[:space:]]*-[[:space:]]*stage:/ {
      flush(); stage = val($0); repo = "-"; status = "-"; deps = "-"; prs = "-"; next
    }
    in_s && stage != "" {
      if ($0 ~ /^[[:space:]]*repo:/)              repo = val($0)
      else if ($0 ~ /^[[:space:]]*status:/)       status = val($0)
      else if ($0 ~ /^[[:space:]]*depends_on_stages:/) deps = val($0)
      else if ($0 ~ /^[[:space:]]*merged_prs:/)   prs = val($0)
    }
    END { flush() }
  ' "$manifest")"

  if [ -z "$stage_rows" ]; then
    malformed "no stages found (need a 'stages:' block with '- stage: N' entries)"
    continue
  fi

  declare -A st_repo=() st_status=() st_deps=() st_prs=()
  stage_order=""
  stages_ok=1
  while IFS='|' read -r num repo status deps prs; do
    if ! echo "$num" | grep -qE '^[0-9]+$'; then
      malformed "stage number not numeric: '$num'"; stages_ok=0; continue
    fi
    if [ -n "${st_repo[$num]+x}" ]; then
      malformed "duplicate stage number: $num"; stages_ok=0; continue
    fi
    if ! echo "$repo" | grep -qE '^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$'; then
      malformed "stage $num: repo must be org/name (got: '$repo')"; stages_ok=0
    fi
    if ! echo "$status" | grep -qE "^(${STATUS_ENUM})$"; then
      malformed "stage $num: status '$status' not in vocabulary (${STATUS_ENUM})"; stages_ok=0
    fi
    st_repo[$num]="$repo"; st_status[$num]="$status"; st_deps[$num]="$deps"; st_prs[$num]="$prs"
    stage_order="$stage_order $num"
  done <<< "$stage_rows"

  if [ "$stages_ok" -eq 1 ]; then
    pass "$(echo "$stage_order" | wc -w) stages parsed, statuses in vocabulary"
  fi

  # Dependency + ordering rules. Declared deps must exist and point at earlier
  # stages; a merged/adopted stage requires all its deps merged/adopted. When
  # depends_on_stages is absent, the default is every earlier non-'none' stage.
  for num in $stage_order; do
    deps="${st_deps[$num]}"
    dep_list=""
    if [ "$deps" = "-" ]; then
      for other in $stage_order; do
        if [ "$other" -lt "$num" ] && [ "${st_status[$other]}" != "none" ]; then
          dep_list="$dep_list $other"
        fi
      done
    else
      dep_list="$(echo "$deps" | tr -d '[]' | tr ',' ' ')"
      for dep in $dep_list; do
        if [ -z "${st_repo[$dep]+x}" ]; then
          malformed "stage $num: depends_on_stages references missing stage $dep"
        elif [ "$dep" = "$num" ]; then
          malformed "stage $num: depends on itself"
        fi
      done
    fi
    case "${st_status[$num]}" in
      merged|adopted)
        for dep in $dep_list; do
          [ -n "${st_status[$dep]+x}" ] || continue
          case "${st_status[$dep]}" in
            merged|adopted) ;;
            *) finding "stage $num (${st_repo[$num]}) is '${st_status[$num]}' but upstream stage $dep (${st_repo[$dep]:-?}) is only '${st_status[$dep]}' — downstream must not land before upstream" ;;
          esac
        done
        ;;
    esac
  done

  # gh-backed checks.
  if [ "$GH_OK" -eq 1 ] && [ -n "$anchor_num" ]; then
    if anchor_state="$(gh issue view "$anchor_num" --repo "$anchor_repo" --json state --jq .state 2>/dev/null)"; then
      pass "anchor issue $anchor exists (state: $anchor_state)"
    else
      finding "anchor issue $anchor not found or not accessible"
    fi

    for num in $stage_order; do
      prs="${st_prs[$num]}"
      if [ "$prs" = "-" ] || [ -z "$(echo "$prs" | tr -d '[] ,')" ]; then
        continue
      fi
      repo="${st_repo[$num]}"
      for pr in $(echo "$prs" | tr -d '[]' | tr ',' ' '); do
        if ! pr_json="$(gh pr view "$pr" --repo "$repo" --json state,body 2>/dev/null)"; then
          note "stage $num: $repo#$pr UNVERIFIED (repo/PR not accessible from here — not inventing state)"
          continue
        fi
        pr_state="$(echo "$pr_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["state"])')"
        if [ "$pr_state" = "MERGED" ]; then
          pass "stage $num: $repo#$pr is MERGED as listed"
        else
          finding "stage $num: $repo#$pr listed in merged_prs but state is $pr_state"
        fi
        pr_body="$(echo "$pr_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["body"] or "")')"
        if echo "$pr_body" | grep -qE "Refs[[:space:]]+([A-Za-z0-9._/-]+)?#${anchor_num}\b"; then
          pass "stage $num: $repo#$pr body Refs the anchor (#${anchor_num})"
        else
          finding "stage $num: $repo#$pr body has no 'Refs #${anchor_num}' for the stack anchor"
        fi
        kw="$(echo "$pr_body" | grep -inE "\b(close[sd]?|fix(e[sd])?|resolve[sd]?)\b[[:space:]:]+([A-Za-z0-9._/-]+)?#${anchor_num}\b" || true)"
        if [ -n "$kw" ]; then
          finding "stage $num: $repo#$pr body uses a close-keyword next to the anchor issue:"
          echo "$kw" | sed 's/^/        /'
        fi
      done
    done
  fi
done

if [ "$CHECKED" -eq 0 ] && [ -n "$ONLY_STACK" ]; then
  malformed "no manifest matched --stack $ONLY_STACK"
fi

echo
if [ "$MALFORMED" -gt 0 ]; then
  echo "check-pr-stack: $MALFORMED malformed, $FINDINGS finding(s)"
  exit 2
fi
if [ "$FINDINGS" -gt 0 ]; then
  if [ "$STRICT" -eq 1 ]; then
    echo "check-pr-stack: $FINDINGS finding(s) (strict: failing)"
    exit 1
  fi
  echo "check-pr-stack: $FINDINGS finding(s) (warning-mode)"
  exit 0
fi
echo "check-pr-stack: clean ($CHECKED manifest(s))"
