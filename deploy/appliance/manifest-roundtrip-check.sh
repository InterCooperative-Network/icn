#!/usr/bin/env bash
# manifest-roundtrip-check.sh — regression coverage for the typed appliance
# manifest path (#2259 emit / #2260 verify / #2261 build-script wiring).
#
# Proves `icnctl appliance emit-manifest` + `icnctl appliance verify-manifest`
# honor the typed manifest contract WITHOUT building a real QCOW2 image: it uses
# throwaway stand-in files for the output image, base image, and the icnd/icnctl
# binaries, emits a manifest, asserts its fields, and exercises the fail-closed
# negatives the build path depends on (tampered binary, wrong --root for a
# repo-relative binary source, missing binary at emit).
#
# This invokes no appliance runtime behavior, builds no image, and makes no
# production-readiness, signing, immutability, reproducibility, A-B update,
# measured-boot, or distribution claim. It is a check/test harness only.
#
# Usage: manifest-roundtrip-check.sh [path-to-icnctl]
#   With no argument, a prebuilt icnctl is located under
#   icn/target/{release,debug}/icnctl.
#
# Exit codes: 0 — every assertion passed; 1 — a check failed or icnctl is absent.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

ICNCTL="${1:-}"
if [ -z "$ICNCTL" ]; then
    for cand in "$REPO_ROOT/icn/target/release/icnctl" "$REPO_ROOT/icn/target/debug/icnctl"; do
        [ -x "$cand" ] && ICNCTL="$cand" && break
    done
fi
if [ -z "$ICNCTL" ] || [ ! -x "$ICNCTL" ]; then
    printf 'manifest-roundtrip: no icnctl binary found (build: cd icn && cargo build -p icnctl)\n' >&2
    exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
    printf 'manifest-roundtrip: python3 required for JSON assertions\n' >&2
    exit 1
fi

fails=0
pass() { printf '    ok   %s\n' "$*"; }
fail() { fails=$((fails + 1)); printf '    FAIL %s\n' "$*" >&2; }

# Fail closed if the temp dir cannot be created: without this, WORK stays empty
# and later setup paths would expand to /bin, /image.qcow2, /wrongroot, etc. — a
# root-run invocation could then write outside the intended throwaway dir.
WORK="$(mktemp -d)" || {
    printf 'manifest-roundtrip: mktemp -d failed (is TMPDIR writable?)\n' >&2
    exit 1
}
if [ -z "${WORK:-}" ] || [ ! -d "$WORK" ]; then
    printf 'manifest-roundtrip: temp dir was not created\n' >&2
    exit 1
fi
cleanup() {
    # Only remove the throwaway dir we actually created; never an empty path or /.
    if [ -n "${WORK:-}" ] && [ "$WORK" != "/" ] && [ -d "$WORK" ]; then
        rm -rf "$WORK"
    fi
}
trap cleanup EXIT

# ---- throwaway stand-in artifacts (NOT a real image or real binaries) -------
mkdir -p "$WORK/bin" "$WORK/wrongroot"
printf 'stand-in appliance output image\n' > "$WORK/image.qcow2"
printf 'stand-in base os image\n'          > "$WORK/base.img"
printf 'stand-in icnd\n'                   > "$WORK/bin/icnd"
printf 'stand-in icnctl\n'                 > "$WORK/bin/icnctl"

EXP_BASE_SHA="$(sha256sum "$WORK/base.img" | awk '{print $1}')"
GIT_COMMIT="0000000000000000000000000000000000000000"
TS="2026-01-01T00:00:00Z"

# Emit a manifest mirroring build-image.sh: absolute --image / --base-image, and
# repo-relative binary SOURCE paths resolved by verify under --root.
emit() {
    local out="$1"; shift
    "$ICNCTL" appliance emit-manifest \
        --image-version "roundtrip-test" \
        --image-format "qcow2" \
        --image "$WORK/image.qcow2" \
        --base-image "$WORK/base.img" \
        --git-commit "$GIT_COMMIT" \
        --build-timestamp "$TS" \
        --binary "/usr/local/bin/icnd=bin/icnd=$WORK/bin/icnd" \
        --binary "/usr/local/bin/icnctl=bin/icnctl=$WORK/bin/icnctl" \
        --output "$out" "$@" >/dev/null 2>&1
}

# JSON helpers (python3, no jq dependency).
jget()  { python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2" 2>/dev/null; }
jbool() { python3 -c 'import json,sys; print(str(json.load(open(sys.argv[1]))[sys.argv[2]]).lower())' "$1" "$2" 2>/dev/null; }
jlen()  { python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))[sys.argv[2]]))' "$1" "$2" 2>/dev/null; }
jhas()  { python3 -c 'import json,sys; sys.exit(0 if sys.argv[2] in json.load(open(sys.argv[1])) else 1)' "$1" "$2"; }

assert_eq() { # assert_eq <label> <actual> <expected>
    if [ "$2" = "$3" ]; then pass "$1"; else fail "$1 (got '$2', want '$3')"; fi
}
expect_fail() { # expect_fail <label> <cmd...>
    local label="$1"; shift
    if "$@" >/dev/null 2>&1; then fail "$label (expected non-zero exit)"; else pass "$label"; fi
}

# ---- 1. default emit + field assertions -------------------------------------
DEF="$WORK/manifest.json"
if emit "$DEF"; then pass "emit default manifest"; else fail "emit default manifest"; fi

assert_eq "manifest_version == 1"          "$(jget  "$DEF" manifest_version)" "1"
if jhas "$DEF" notes; then fail "no notes field (present)"; else pass "no notes field"; fi
assert_eq "non_production == true"         "$(jbool "$DEF" non_production)"   "true"
assert_eq "signed == false"                "$(jbool "$DEF" signed)"          "false"
assert_eq "immutable == false"             "$(jbool "$DEF" immutable)"       "false"
assert_eq "demo_profile == false (default)" "$(jbool "$DEF" demo_profile)"   "false"
assert_eq "base_image_sha256 == actual"    "$(jget  "$DEF" base_image_sha256)" "$EXP_BASE_SHA"
assert_eq "built_binaries records == 2"    "$(jlen  "$DEF" built_binaries)"  "2"

# ---- 2. default manifest verifies clean -------------------------------------
if "$ICNCTL" appliance verify-manifest "$DEF" --root "$WORK" >/dev/null 2>&1; then
    pass "verify default manifest"
else
    fail "verify default manifest"
fi

# ---- 3. demo-profile case ---------------------------------------------------
DEMO="$WORK/manifest-demo.json"
if emit "$DEMO" --demo-profile; then pass "emit demo-profile manifest"; else fail "emit demo-profile manifest"; fi
assert_eq "demo_profile == true (--demo-profile)" "$(jbool "$DEMO" demo_profile)" "true"
if "$ICNCTL" appliance verify-manifest "$DEMO" --root "$WORK" >/dev/null 2>&1; then
    pass "verify demo-profile manifest"
else
    fail "verify demo-profile manifest"
fi

# ---- 4. fail-closed negatives -----------------------------------------------
# Wrong --root: absolute image/base still resolve, but the repo-relative binary
# source is not found under the wrong root, so verification must fail closed.
expect_fail "wrong --root rejected (repo-relative binary source)" \
    "$ICNCTL" appliance verify-manifest "$DEF" --root "$WORK/wrongroot"

# Missing binary at emit: a --binary FILE that does not exist must fail closed.
MISS="$WORK/manifest-missing.json"
expect_fail "missing binary rejected at emit" \
    "$ICNCTL" appliance emit-manifest \
        --image-version "roundtrip-test" --image-format "qcow2" \
        --image "$WORK/image.qcow2" --base-image "$WORK/base.img" \
        --git-commit "$GIT_COMMIT" --build-timestamp "$TS" \
        --binary "/usr/local/bin/icnctl=bin/icnctl=$WORK/bin/does-not-exist" \
        --output "$MISS"

# Tampered binary: mutate the recorded source after emit; re-hash must mismatch.
# Done last because it mutates a shared stand-in.
printf 'tampered\n' >> "$WORK/bin/icnctl"
expect_fail "tampered binary rejected" \
    "$ICNCTL" appliance verify-manifest "$DEF" --root "$WORK"

# ---- summary ----------------------------------------------------------------
if [ "$fails" -eq 0 ]; then
    printf '  manifest round-trip: all assertions passed\n'
    exit 0
else
    printf '  manifest round-trip: %d assertion(s) failed\n' "$fails" >&2
    exit 1
fi
