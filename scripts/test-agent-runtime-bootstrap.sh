#!/usr/bin/env bash
# Acceptance suite for the fresh-worktree runtime bootstrap.
#
# WHY THIS EXISTS
#   A worktree created by `wt-new`, by Claude Code's nested-worktree mechanism, or by hand has
#   no ops/mcp/dist — `dist/` is gitignored and no creation path builds it. Because
#   icn-agent-session required an in-tree build, every such lane was PERMANENTLY unregistered.
#   Measured on the dev VM when this was found: 44 of 50 worktrees could not register, the
#   registry reported zero sessions while four Claude activations were live, and the readiness
#   audit that found it was itself running in an unregistered lane.
#
#   The defect was invisible to every existing test because they all ran somewhere that
#   happened to have a build. The rule here is therefore: NEVER let a case start from a
#   worktree that already has dist. Each case below creates a lane with no dist and asserts
#   that the lane becomes a working, registered agent lane on its own.
#
# WHAT IS AND IS NOT SHARED WITH THE REAL MACHINE
#   The dependency tree (~157M, better-sqlite3 compiled from source, ~3m30s) is machine-level
#   state keyed by lockfile, exactly like having node installed at all. Cases that only need a
#   working runtime borrow it through ICN_RUNTIME_CACHE/deps rather than recompiling it per
#   run. What is never borrowed is the compiled ICN runtime itself: every case builds dist from
#   the fixture's own sources, because that is the thing under test. The cold-dependency path
#   is covered separately, and by assertion rather than by waiting (see "cold deps").
#
# Usage: scripts/test-agent-runtime-bootstrap.sh

set -uo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
# chmod first: published cache entries are deliberately read-only (the poisoning guard), and
# plain `rm -rf` cannot remove them. Same escape hatch a human clearing the real cache needs.
trap 'chmod -R u+w "$TMP" 2>/dev/null; rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { printf '  ok    %s\n' "$1"; PASS=$((PASS+1)); }
bad() { printf '  FAIL  %s\n     %s\n' "$1" "${2:-}"; FAIL=$((FAIL+1)); }

REAL_CACHE="${ICN_RUNTIME_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/icn/agent-runtime}"
CACHE="$TMP/cache"
mkdir -p "$CACHE"

# Borrow only the dependency tree (see header). If the machine has never bootstrapped, the
# build cases cannot run and say so rather than silently reporting green.
DEPS_AVAILABLE=0
if [ -d "$REAL_CACHE/deps" ] && [ -n "$(ls -A "$REAL_CACHE/deps" 2>/dev/null)" ]; then
  ln -s "$REAL_CACHE/deps" "$CACHE/deps"
  DEPS_AVAILABLE=1
fi

export ICN_RUNTIME_CACHE="$CACHE"

# A fixture is a real Git repository holding only what the bootstrap and the CLI read. It is a
# real repo because lane identity is Git-derived and a non-repo cwd is correctly refused.
ORIGIN="$TMP/origin"
mkdir -p "$ORIGIN"
# .gitignore is part of the fixture, not incidental: the bootstrap publishes its resolution as
# a symlink at ops/mcp/dist, and the "stays clean" case is only meaningful if the fixture
# carries the same ignore rules the real repository does.
for p in .gitignore ops/scripts ops/mcp/src ops/mcp/package.json ops/mcp/package-lock.json ops/mcp/tsconfig.json; do
  [ -e "$REPO_ROOT/$p" ] || continue
  mkdir -p "$ORIGIN/$(dirname "$p")"
  cp -a "$REPO_ROOT/$p" "$ORIGIN/$(dirname "$p")/"
done
git -C "$ORIGIN" init -q -b main .
git -C "$ORIGIN" add -A >/dev/null 2>&1
git -C "$ORIGIN" -c user.email=fixture@local -c user.name=fixture commit -q -m "runtime bootstrap fixture"

# Every lane gets its own registry file so cases cannot see each other's rows, and so a test
# run can never write into the live ICN registry.
session() { ICN_OPS_DB="$TMP/registry-$1.db" "$2/ops/scripts/icn-agent-session" "${@:3}"; }

echo "fresh-worktree runtime bootstrap"

# --- case 1: canonical fresh worktree (the wt-new shape) ------------------------------------
LANE1="$TMP/lanes/task-fresh"
mkdir -p "$TMP/lanes"
git -C "$ORIGIN" worktree add -q -b task/fresh "$LANE1" main 2>/dev/null

if [ -e "$LANE1/ops/mcp/dist" ]; then
  bad "canonical lane fixture already has a dist — the case would prove nothing" "$LANE1"
elif [ "$DEPS_AVAILABLE" = 0 ]; then
  bad "no dependency cache at $REAL_CACHE/deps; run ops/scripts/icn-runtime-build first" \
      "cannot prove fresh-worktree registration without it"
else
  out="$(session c1 "$LANE1" register --harness-key fresh-canonical-1 --cwd "$LANE1" \
         --provider test 2>"$TMP/c1.err")"
  if printf '%s' "$out" | grep -q '"session_id"'; then
    ok "a canonical fresh worktree with no dist registers automatically"
  else
    bad "canonical fresh worktree did not register" "$(tail -3 "$TMP/c1.err")"
  fi

  # Identity must be the LANE, not the origin the worktree was cut from. A bootstrap that
  # resolved the wrong repo or worktree would still print a session_id.
  st="$(session c1 "$LANE1" status --harness-key fresh-canonical-1 2>/dev/null)"
  if printf '%s' "$st" | grep -q '"registered":true'; then
    ok "the registered session is readable back as registered"
  else
    bad "registered session did not read back" "$st"
  fi
  if printf '%s' "$st" | grep -q 'task-fresh'; then
    ok "lane identity records the worktree, not the origin checkout"
  else
    bad "lane identity does not name the worktree" "$st"
  fi
fi

# --- case 2: Claude-style nested worktree ---------------------------------------------------
# Claude Code creates its per-session lane UNDER the selected project, at
# <project>/.claude/worktrees/<name>. That path is a real Git worktree of the same store, but
# it is nested inside another worktree — the shape the canonical tooling never produces.
LANE2="$ORIGIN/.claude/worktrees/nested-lane"
mkdir -p "$ORIGIN/.claude/worktrees"
git -C "$ORIGIN" worktree add -q -b claude/nested "$LANE2" main 2>/dev/null

if [ "$DEPS_AVAILABLE" = 1 ]; then
  if [ -e "$LANE2/ops/mcp/dist" ]; then
    bad "nested lane fixture already has a dist" "$LANE2"
  else
    out="$(session c2 "$LANE2" register --harness-key fresh-nested-1 --cwd "$LANE2" \
           --provider test 2>"$TMP/c2.err")"
    if printf '%s' "$out" | grep -q '"session_id"'; then
      ok "a Claude-style nested worktree registers automatically"
    else
      bad "nested worktree did not register" "$(tail -3 "$TMP/c2.err")"
    fi
    st="$(session c2 "$LANE2" status --harness-key fresh-nested-1 2>/dev/null)"
    if printf '%s' "$st" | grep -q 'nested-lane'; then
      ok "nested lane identity is the nested worktree, not its parent"
    else
      bad "nested lane resolved to the wrong worktree" "$st"
    fi
  fi
fi

# --- case 3: concurrent cold starts ---------------------------------------------------------
# Several agents may start at once against an empty build cache. They must all succeed, agree
# on one answer, and leave exactly one cache entry — not a half-written tree.
if [ "$DEPS_AVAILABLE" = 1 ]; then
  CONC="$TMP/cache-conc"; mkdir -p "$CONC"; ln -s "$REAL_CACHE/deps" "$CONC/deps"
  for i in 1 2 3 4; do
    ( ICN_RUNTIME_CACHE="$CONC" "$LANE1/ops/scripts/icn-runtime-build" >"$TMP/conc.$i" 2>"$TMP/conc.$i.err" ) &
  done
  wait
  paths="$(cat "$TMP"/conc.1 "$TMP"/conc.2 "$TMP"/conc.3 "$TMP"/conc.4 2>/dev/null | sort -u)"
  count="$(printf '%s\n' "$paths" | grep -c . )"
  if [ "$count" -eq 1 ] && [ -f "$(printf '%s' "$paths")/cli/session.js" ]; then
    ok "four concurrent cold starts agree on one usable build"
  else
    bad "concurrent starts disagreed or failed" "$(printf '%s' "$paths" | head -4)"
  fi
  entries="$(ls -1 "$CONC/build" 2>/dev/null | wc -l)"
  if [ "$entries" -eq 1 ]; then
    ok "concurrent starts leave exactly one cache entry"
  else
    bad "expected one build cache entry, found $entries" "$(ls -1 "$CONC/build" 2>/dev/null)"
  fi
  # A staging directory left behind would mean a partially built tree could be observed.
  leftovers="$(ls -1 "$CONC/tmp" 2>/dev/null | wc -l)"
  if [ "$leftovers" -eq 0 ]; then
    ok "no staging directories survive a concurrent build"
  else
    bad "staging directories leaked" "$(ls -1 "$CONC/tmp")"
  fi
fi

# --- case 4: anti-stale ---------------------------------------------------------------------
# The old rule was "this checkout's location wins". The replacement must be at least as strong:
# a change to the runtime sources must NOT resolve to the build made from the old ones.
if [ "$DEPS_AVAILABLE" = 1 ]; then
  before_fp="$("$LANE1/ops/scripts/icn-runtime-build" --fingerprint)"
  before_path="$("$LANE1/ops/scripts/icn-runtime-build")"
  printf '\n// bootstrap acceptance: source change must change the fingerprint\n' \
    >> "$LANE1/ops/mcp/src/state/db.ts"
  after_fp="$("$LANE1/ops/scripts/icn-runtime-build" --fingerprint)"
  if [ "$before_fp" != "$after_fp" ]; then
    ok "editing a runtime source changes the fingerprint"
  else
    bad "fingerprint did not change after a source edit" "$before_fp"
  fi
  after_path="$(ICN_RUNTIME_NO_BUILD=1 "$LANE1/ops/scripts/icn-runtime-build" 2>/dev/null)"
  if [ -z "$after_path" ]; then
    ok "changed sources do not resolve to the build made from the old ones"
  else
    bad "stale build was handed out for changed sources" "$before_path -> $after_path"
  fi
  git -C "$LANE1" checkout -- ops/mcp/src/state/db.ts 2>/dev/null

  # Path-independence: two lanes holding identical sources must agree, or the cache degenerates
  # into a per-worktree build and the whole fix is undone.
  fp1="$("$LANE1/ops/scripts/icn-runtime-build" --fingerprint)"
  fp2="$("$LANE2/ops/scripts/icn-runtime-build" --fingerprint)"
  if [ "$fp1" = "$fp2" ]; then
    ok "two lanes with identical sources share one fingerprint"
  else
    bad "identical sources produced different fingerprints" "$fp1 vs $fp2"
  fi
fi

# --- case 4a: the native ABI is part of the cache identity -----------------------------------
# The installed tree holds a NATIVE better-sqlite3 binding. Keyed on the lockfile alone, an
# entry stayed valid by name across a Node upgrade: the fast path returned the hit before
# deps_native_ok() could notice the binding no longer loads, so every lane failed at CLI load
# with no path to repair itself.
if [ "$DEPS_AVAILABLE" = 1 ]; then
  a="$(ICN_RUNTIME_ABI=abi-aaa "$LANE1/ops/scripts/icn-runtime-build" --fingerprint)"
  b="$(ICN_RUNTIME_ABI=abi-bbb "$LANE1/ops/scripts/icn-runtime-build" --fingerprint)"
  src_a="$(ICN_RUNTIME_ABI=abi-aaa "$LANE1/ops/scripts/icn-runtime-build" --fingerprint-src)"
  src_b="$(ICN_RUNTIME_ABI=abi-bbb "$LANE1/ops/scripts/icn-runtime-build" --fingerprint-src)"
  if [ "$a" != "$b" ]; then
    ok "a different native ABI yields a different build identity"
  else
    bad "build identity ignored the ABI" "$a"
  fi
  if [ "$src_a" = "$src_b" ]; then
    ok "the ABI does not disturb the SOURCE fingerprint"
  else
    bad "ABI leaked into the source fingerprint" "$src_a vs $src_b"
  fi
fi

# --- case 4c: an entry records the sources it was compiled from ------------------------------
# The build key is a hash of sources AND ABI, so it cannot be read back. Recording the source
# fingerprint makes an entry self-describing, and lets this assert the property the staging
# re-hash protects: what was published matches what was hashed.
if [ "$DEPS_AVAILABLE" = 1 ]; then
  entry="$("$LANE1/ops/scripts/icn-runtime-build")"
  recorded="$(cat "$(dirname "$entry")/.icn-runtime-src" 2>/dev/null)"
  current="$("$LANE1/ops/scripts/icn-runtime-build" --fingerprint-src)"
  if [ -n "$recorded" ] && [ "$recorded" = "$current" ]; then
    ok "a published entry records the source fingerprint it was compiled from"
  else
    bad "entry provenance missing or does not match its sources" "recorded=$recorded current=$current"
  fi
fi

# --- case 4b: a published cache entry cannot be written into ---------------------------------
# A bootstrapped lane's ops/mcp/dist is a symlink into the shared cache, so `npm run build` in
# that lane would compile its sources into a cache entry other lanes are using. If the lane's
# sources changed, the fingerprint changed too, and the write lands in the OLD fingerprint's
# entry — handing every lane at that fingerprint a build made from different sources.
if [ "$DEPS_AVAILABLE" = 1 ]; then
  entry="$("$LANE1/ops/scripts/icn-runtime-build")"
  if touch "$entry/poison.js" 2>/dev/null; then
    bad "a published cache entry is writable — npm run build in a lane could poison it" "$entry"
    rm -f "$entry/poison.js"
  else
    ok "a published cache entry rejects writes instead of being poisoned"
  fi
fi

# --- case 5: a broken bootstrap fails LOUDLY and registers nothing ---------------------------
# The failure that started all of this was silent-by-accident. A bootstrap that cannot succeed
# must say so and must never leave the caller believing lifecycle tracking is active.
COLD="$TMP/cache-cold"; mkdir -p "$COLD"
err="$(ICN_RUNTIME_CACHE="$COLD" ICN_RUNTIME_NO_BUILD=1 \
       "$LANE1/ops/scripts/icn-runtime-build" 2>&1 >/dev/null)"
rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$err" | grep -q 'no cached runtime'; then
  ok "an unresolvable runtime exits non-zero with a specific reason"
else
  bad "unresolvable runtime did not fail loudly (rc=$rc)" "$err"
fi

# A lane of its own, and a fresh one: by now LANE1 has been bootstrapped and carries a working
# dist symlink, which the in-tree fallback would (correctly) rescue. "Fails loudly" is only
# meaningful where there is genuinely no runtime to fall back to.
LANE3="$TMP/lanes/task-noruntime"
git -C "$ORIGIN" worktree add -q -b task/noruntime "$LANE3" main 2>/dev/null
out="$(ICN_RUNTIME_CACHE="$COLD" ICN_RUNTIME_NO_BUILD=1 \
       session c5 "$LANE3" register --harness-key broken-1 --cwd "$LANE3" --provider test 2>"$TMP/c5.err")"
if printf '%s' "$out" | grep -q '"session_id"'; then
  bad "a broken bootstrap still reported a registration" "$out"
else
  ok "a broken bootstrap registers nothing"
fi
if grep -q 'DEGRADED (unregistered)' "$TMP/c5.err"; then
  ok "a broken bootstrap prints the DEGRADED banner on stderr"
else
  bad "broken bootstrap failed silently" "$(cat "$TMP/c5.err")"
fi

# cold deps: asserted, not waited on. A cold dependency tree must hand off to a detached
# bootstrap rather than stalling a human at a prompt for minutes.
# Four at once, because that is how SessionStart hooks actually arrive on a cold machine. Each
# must defer, and between them they must start EXACTLY ONE installer: `flock -n <file> true`
# released the lock before the child could take it, so every arrival passed the probe and
# launched its own detached multi-minute native build.
NODEPS="$TMP/cache-nodeps"
for i in 1 2 3 4; do
  ( ICN_RUNTIME_CACHE="$NODEPS" "$LANE1/ops/scripts/icn-runtime-build" \
      --background-if-cold >/dev/null 2>"$TMP/cold.$i.err" ) &
done
wait
err="$(cat "$TMP"/cold.*.err 2>/dev/null)"
deferred="$(grep -lc 'background bootstrap' "$TMP"/cold.*.err 2>/dev/null | wc -l)"
if [ "$deferred" -eq 4 ]; then
  ok "every concurrent cold start defers instead of blocking"
else
  bad "not all cold starts deferred ($deferred/4)" "$(printf '%s' "$err" | head -2)"
fi
started="$(grep -h 'background bootstrap was started' "$TMP"/cold.*.err 2>/dev/null | wc -l)"
if [ "$started" -eq 1 ]; then
  ok "a burst of concurrent cold starts launches exactly one installer"
else
  bad "expected exactly one launched installer, got $started" "$(printf '%s' "$err" | head -3)"
fi
# REAP IT. This case starts a REAL detached bootstrap, which would otherwise keep running npm ci
# and a native compile for minutes after the suite exits — racing the teardown that deletes the
# directory it is installing into, and leaving exactly the kind of abandoned build this
# repository keeps having to clean up by hand.
BOOT_PID="$(cat "$NODEPS/bootstrap.pid" 2>/dev/null)"
if [ -n "$BOOT_PID" ]; then
  ok "the background bootstrap records its pid so it can be found and reaped"
  kill -TERM -- "-$BOOT_PID" 2>/dev/null || kill -TERM "$BOOT_PID" 2>/dev/null
  sleep 1
  kill -KILL -- "-$BOOT_PID" 2>/dev/null || kill -KILL "$BOOT_PID" 2>/dev/null
else
  bad "the background bootstrap left no pid file; it cannot be reaped" "$NODEPS/bootstrap.pid"
fi

# --- case 6: a failed re-resolution must not take away a working runtime ---------------------
# Regression. The first version of this fix re-resolved unconditionally on `register` and
# degraded when the bootstrap could not run — turning environments that HAD a working in-tree
# dist (no npm, a partial checkout, a read-only cache) from working into unregistered. The
# adoption gate caught it: its fixture carries a dist but no ops/mcp/package.json.
if [ "$DEPS_AVAILABLE" = 1 ]; then
  PARTIAL="$TMP/partial"
  mkdir -p "$PARTIAL/ops/mcp"
  cp -a "$REPO_ROOT/.gitignore" "$PARTIAL/" 2>/dev/null
  cp -a "$REPO_ROOT/ops/scripts" "$PARTIAL/ops/"
  # A working runtime, and deliberately no ops/mcp/package.json for the bootstrap to read.
  ln -s "$("$LANE1/ops/scripts/icn-runtime-build")" "$PARTIAL/ops/mcp/dist"
  git -C "$PARTIAL" init -q -b main .
  git -C "$PARTIAL" -c user.email=f@l -c user.name=f commit -q --allow-empty -m partial

  out="$(session c6 "$PARTIAL" register --harness-key partial-1 --cwd "$PARTIAL" \
         --provider test 2>"$TMP/c6.err")"
  if printf '%s' "$out" | grep -q '"session_id"'; then
    ok "an unrunnable bootstrap falls back to the in-tree build instead of degrading"
  else
    bad "fallback to the in-tree build did not happen" "$(tail -3 "$TMP/c6.err")"
  fi
  if grep -q 'could not re-resolve' "$TMP/c6.err"; then
    ok "the fallback says what it could not re-check rather than claiming a verified build"
  else
    bad "fallback was silent about the failed re-resolution" "$(cat "$TMP/c6.err")"
  fi
fi

# --- case 7: a cached runtime still registers into the REPOSITORY's registry -----------------
# Regression, and the reason every other case here pins ICN_OPS_DB is also the reason this one
# must not. db.ts derives the shared registry from the CLI's own __dirname; a cached runtime
# lives outside every checkout, so the derivation found no repository and silently fell back to
# a per-installation database. Every bootstrapped lane would have registered somewhere the MCP
# server and every other worktree could not see — the one-registry-per-repository invariant,
# broken by the fix meant to make registration work at all.
if [ "$DEPS_AVAILABLE" = 1 ]; then
  COMMON="$(cd "$(git -C "$LANE1" rev-parse --git-common-dir)" && pwd -P)"
  ( cd "$LANE1" && env -u ICN_OPS_DB "$LANE1/ops/scripts/icn-agent-session" register \
      --harness-key registry-derivation-1 --cwd "$LANE1" --provider test ) \
      >"$TMP/c7.out" 2>"$TMP/c7.err"
  if grep -q 'could not derive the shared registry' "$TMP/c7.err"; then
    bad "a cached runtime fell back to a per-installation registry" "$(head -2 "$TMP/c7.err")"
  else
    ok "a cached runtime does not fall back to a per-installation registry"
  fi
  if [ -f "$COMMON/icn-ops.db" ] && grep -q '"session_id"' "$TMP/c7.out"; then
    ok "the session lands in the repository's shared registry ($COMMON/icn-ops.db)"
  else
    bad "session did not land in the repository registry" "expected $COMMON/icn-ops.db"
  fi
fi

# --- case 8: the bootstrap never dirties the worktree ----------------------------------------
# A bootstrap that showed up in `git status` would put build output into every review.
if [ "$DEPS_AVAILABLE" = 1 ]; then
  dirty="$(git -C "$LANE1" status --porcelain 2>/dev/null)"
  if [ -z "$dirty" ]; then
    ok "a bootstrapped lane stays clean in git status"
  else
    bad "bootstrap dirtied the worktree" "$dirty"
  fi
fi

printf '\n%s passed, %s failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
