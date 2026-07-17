#!/usr/bin/env bash
# Static validator for an assembled evaluator package (portable bootable vertical
# slice). Fails closed. Designed to run WITHOUT KVM and without the ~1GB qcow2, so
# CI can catch packaging / script / checksum / manifest / privacy / bind
# regressions on an ordinary runner.
#
# Usage:
#   validate-evaluator-package.sh <package-root-dir> [--no-image] [--expect-commit SHA]
#
#   --no-image        The qcow2 is absent (CI / template-only mode). Image
#                     presence + image-hash checks are skipped; every other check
#                     still runs. The manifest's declared image_sha256 is still
#                     cross-checked against SHA256SUMS when both exist.
#   --expect-commit   Assert manifest.git_commit == SHA (packaging provenance).
#
# Exit non-zero on ANY failure. Never prints a secret-shaped value; findings name
# the file + class only.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=/dev/null
. "$SCRIPT_DIR/package-spec.env"

PKG_ROOT="${1:-}"
NO_IMAGE=0
EXPECT_COMMIT=""
shift || true
while [ "$#" -gt 0 ]; do
  case "$1" in
    --no-image) NO_IMAGE=1 ;;
    --expect-commit) shift; EXPECT_COMMIT="${1:-}" ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
  shift
done

if [ -z "$PKG_ROOT" ] || [ ! -d "$PKG_ROOT" ]; then
  echo "usage: validate-evaluator-package.sh <package-root-dir> [--no-image] [--expect-commit SHA]" >&2
  exit 2
fi
PKG_ROOT="$(cd "$PKG_ROOT" && pwd)"

FAILS=0
pass() { printf '  ok    %s\n' "$*"; }
fail() { printf '  FAIL  %s\n' "$*"; FAILS=$((FAILS+1)); }
note() { printf '  --    %s\n' "$*"; }

echo "== validate evaluator package: $PKG_ROOT =="
[ "$NO_IMAGE" -eq 1 ] && note "image checks skipped (--no-image)"

# ---- 1. Required layout ----------------------------------------------------
for rel in "${REQUIRED_FILES[@]}"; do
  if [ "$NO_IMAGE" -eq 1 ] && { [ "$rel" = "$IMAGE_BASENAME" ]; }; then
    note "skip (no-image): $rel"; continue
  fi
  if [ -e "$PKG_ROOT/$rel" ]; then pass "present: $rel"; else fail "missing required file: $rel"; fi
done

# ---- 2. Executable bits ----------------------------------------------------
for rel in "${EXECUTABLE_FILES[@]}"; do
  if [ -x "$PKG_ROOT/$rel" ]; then pass "executable: $rel"; else fail "not executable: $rel"; fi
done

# ---- 3. Script syntax + shellcheck ----------------------------------------
SCRIPTS=()
while IFS= read -r -d '' s; do SCRIPTS+=("$s"); done < <(find "$PKG_ROOT" -type f -name '*.sh' -print0)
for s in "${SCRIPTS[@]}"; do
  if bash -n "$s" 2>/dev/null; then pass "bash -n: ${s#"$PKG_ROOT"/}"; else fail "bash -n FAILED: ${s#"$PKG_ROOT"/}"; fi
done
if command -v shellcheck >/dev/null 2>&1; then
  for s in "${SCRIPTS[@]}"; do
    if shellcheck -S warning "$s" >/dev/null 2>&1; then pass "shellcheck: ${s#"$PKG_ROOT"/}"; else fail "shellcheck warnings: ${s#"$PKG_ROOT"/}"; fi
  done
else
  note "shellcheck not installed — skipping static analysis (syntax still checked)"
fi

# ---- 4. Checksums ----------------------------------------------------------
if [ -f "$PKG_ROOT/SHA256SUMS" ]; then
  if [ "$NO_IMAGE" -eq 1 ]; then
    # Verify every listed file that is present (the image line is expected absent).
    miss=0
    while read -r _hash rel; do
      rel="${rel#\*}"
      [ -z "$rel" ] && continue
      if [ "$rel" = "$IMAGE_BASENAME" ]; then continue; fi
      [ -f "$PKG_ROOT/$rel" ] || { fail "SHA256SUMS lists missing file: $rel"; miss=1; }
    done < "$PKG_ROOT/SHA256SUMS"
    if [ "$miss" -eq 0 ]; then
      ( cd "$PKG_ROOT" && grep -vF "$IMAGE_BASENAME" SHA256SUMS | sha256sum -c --quiet - ) \
        && pass "SHA256SUMS verify (non-image entries)" || fail "SHA256SUMS verify failed (non-image entries)"
    fi
  else
    ( cd "$PKG_ROOT" && sha256sum -c --quiet SHA256SUMS ) \
      && pass "SHA256SUMS verify (all entries)" || fail "SHA256SUMS verify failed"
  fi
else
  fail "SHA256SUMS missing"
fi

# ---- 5. Manifest metadata + honest posture --------------------------------
MANIFEST="$PKG_ROOT/$MANIFEST_BASENAME"
if [ -f "$MANIFEST" ]; then
  # Run inside `if` so a non-zero exit is captured (not aborted by `set -e`),
  # letting the remaining privacy/bind/non-claim checks still run + accumulate.
  if python3 - "$MANIFEST" "$PKG_ROOT" "$NO_IMAGE" "$EXPECT_COMMIT" \
    "$PKG_VERSION" "$PKG_ARCH" "$REQUIRE_NON_PRODUCTION" "$REQUIRE_SIGNED" \
    "$REQUIRE_DEMO_PROFILE" "$IMAGE_BASENAME" <<'PY'
import json, sys, re, os
mf, root, no_image, expect_commit, ver, arch, req_np, req_signed, req_demo, image_base = sys.argv[1:11]
no_image = no_image == "1"
out=[]
ok=True
def P(m): out.append("  ok    "+m)
def F(m):
    global ok; ok=False; out.append("  FAIL  "+m)
try:
    d=json.load(open(mf))
except Exception as e:
    print("  FAIL  manifest is not valid JSON"); sys.exit(1)
# honest posture
for key,want in (("non_production",req_np=="true"),("signed",req_signed=="true"),("demo_profile",req_demo=="true")):
    got=d.get(key)
    if isinstance(got,bool) and got==want: P(f"manifest {key}={got}")
    else: F(f"manifest {key}={got!r} (expected {want})")
# version / arch agreement (manifest.version is like '0.0.2-demo')
mv=str(d.get("version",""))
if mv.split("-")[0]==ver: P(f"manifest version starts with {ver}")
else: F(f"manifest version {mv!r} disagrees with spec {ver}")
if str(d.get("arch",""))==arch: P(f"manifest arch={arch}")
else: F(f"manifest arch {d.get('arch')!r} disagrees with spec {arch}")
# paths must be basenames (no leaked host dirs)
for key in ("image_path","base_image_path"):
    v=d.get(key)
    if v is None: P(f"manifest {key} absent (fine)"); continue
    if "/" in str(v) or "\\" in str(v): F(f"manifest {key} is not a basename (host path leak): contains a path separator")
    else: P(f"manifest {key} is a bare basename")
# image_sha256 present + shape
ish=str(d.get("image_sha256",""))
if re.fullmatch(r"[0-9a-f]{64}", ish): P("manifest image_sha256 present (64 hex)")
else: F("manifest image_sha256 missing/malformed")
# cross-check against SHA256SUMS
sums=os.path.join(root,"SHA256SUMS")
if os.path.isfile(sums):
    want=None
    for line in open(sums):
        parts=line.split()
        if len(parts)>=2 and parts[1].lstrip("*")==image_base:
            want=parts[0]
    if want:
        if want==ish: P("manifest image_sha256 == SHA256SUMS image entry")
        else: F("manifest image_sha256 disagrees with SHA256SUMS image entry")
    else:
        (P if no_image else F)("SHA256SUMS has no image entry" + (" (expected in no-image mode)" if no_image else ""))
# commit provenance
gc=str(d.get("git_commit",""))
if re.fullmatch(r"[0-9a-f]{40}", gc): P("manifest git_commit present (40 hex)")
else: F("manifest git_commit missing/malformed")
if expect_commit:
    if gc==expect_commit: P("manifest git_commit == expected source commit")
    else: F("manifest git_commit != expected source commit")
# real image hash when present
if not no_image:
    img=os.path.join(root,image_base)
    if os.path.isfile(img):
        import hashlib
        h=hashlib.sha256()
        with open(img,'rb') as fh:
            for chunk in iter(lambda: fh.read(1<<20), b''): h.update(chunk)
        if h.hexdigest()==ish: P("actual qcow2 sha256 == manifest image_sha256")
        else: F("actual qcow2 sha256 != manifest image_sha256")
    else:
        F("image declared but file missing")
print("\n".join(out))
sys.exit(0 if ok else 1)
PY
  then pass "manifest checks"; else fail "manifest checks (see above)"; fi
else
  fail "manifest missing: $MANIFEST_BASENAME"
fi

# ---- 6. Privacy / forbidden-value scan (text files only) -------------------
TEXT=()
while IFS= read -r -d '' f; do TEXT+=("$f"); done < <(find "$PKG_ROOT" -type f ! -name '*.qcow2' ! -name '*.pdf' -print0)
scan() { printf '%s\0' "${TEXT[@]}" | xargs -0 grep -lE "$1" 2>/dev/null || true; }
declare -A FORBID=(
  ["absolute home/user/root path"]='/home/[a-z]|/Users/|/root/'
  ["RFC1918 private IP"]='(^|[^0-9])(10\.[0-9]+\.[0-9]+\.[0-9]+|192\.168\.[0-9]+\.[0-9]+|172\.(1[6-9]|2[0-9]|3[01])\.[0-9]+\.[0-9]+)([^0-9]|$)'
  # Generic internal-deployment shapes: the ICN rehearsal service subdomain,
  # private TLDs, and a deployment VM id. Site-specific internal domains are the
  # job of the local sanitize deny-list (not hardcoded in this public tool).
  ["internal hostname / deployment id"]='rehearsal\.icn[.:]|icn-rehearsal|\.(internal|lan|home|corp|localdomain)\b|\b[Vv][Mm][Ii]?[Dd]? ?[0-9]{3}\b'
  ["temp tunnel URL"]='loca\.lt|ngrok|trycloudflare'
  ["JWT-shaped credential"]='eyJ[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}'
  ["PEM private key"]='BEGIN [A-Z ]*PRIVATE KEY|BEGIN OPENSSH PRIVATE'
  ["bearer/authorization literal"]='[Aa]uthorization: *[Bb]earer [A-Za-z0-9._-]{12,}'
  ["personal name/email"]='([Aa]aron)|@gmail\.com|@icloud\.com'
  ["overclaim"]='production-ready|pilot-ready|organizer-approved|live federation|accessibility[- ]complete'
)
for label in "${!FORBID[@]}"; do
  hits="$(scan "${FORBID[$label]}")"
  if [ -z "$hits" ]; then pass "no $label"; else
    fail "found $label in: $(echo "$hits" | sed "s#$PKG_ROOT/##g" | tr '\n' ' ')"
  fi
done
# DIDs are prohibited in SCRIPTS (a shipped script must never embed an identity);
# docs may name the did: scheme conceptually, so restrict this one to scripts.
did_hits="$(printf '%s\0' "${SCRIPTS[@]}" | xargs -0 grep -lE 'did:(icn|key|web):[A-Za-z0-9]' 2>/dev/null || true)"
if [ -z "$did_hits" ]; then pass "no DID literal in scripts"; else fail "DID literal in scripts: $(echo "$did_hits" | sed "s#$PKG_ROOT/##g" | tr '\n' ' ')"; fi

# ---- 7. Archive safety (tree) ---------------------------------------------
if find "$PKG_ROOT" -type l | grep -q .; then fail "symlink present in package tree"; else pass "no symlinks in tree"; fi

# ---- 8. Launcher default binds are loopback --------------------------------
LAUNCHER="$PKG_ROOT/start-icn-demo-vm.sh"
if [ -f "$LAUNCHER" ]; then
  # QEMU host forwards must pin 127.0.0.1 (never empty-host tcp::PORT, never 0.0.0.0)
  if grep -qE 'hostfwd=tcp:(:[0-9]|0\.0\.0\.0:)' "$LAUNCHER"; then
    fail "launcher hostfwd defaults to a non-loopback bind"
  else pass "launcher hostfwd bind is loopback-scoped"; fi
  if grep -qE 'hostfwd=tcp:127\.0\.0\.1:' "$LAUNCHER"; then pass "launcher hostfwd explicitly 127.0.0.1"; else fail "launcher has no explicit 127.0.0.1 hostfwd"; fi
  # ssh -L forwards must not bind all interfaces; GatewayPorts must not be enabled
  if grep -qE 'GatewayPorts +yes' "$LAUNCHER"; then fail "launcher enables GatewayPorts (exposes tunnels)"; else pass "launcher does not enable GatewayPorts"; fi
  if grep -qE -- '-L +(\*|0\.0\.0\.0):' "$LAUNCHER"; then fail "launcher ssh -L binds all interfaces"; else pass "launcher ssh -L is localhost-bound"; fi
else
  fail "launcher template missing from package"
fi

# ---- 9. Non-claim wording present -----------------------------------------
if grep -rqiE 'not a production|non-production|unsigned|does not claim' "$PKG_ROOT"/README.md "$PKG_ROOT"/docs/*.md 2>/dev/null; then
  pass "honest non-claim wording present in docs"
else
  fail "docs lack explicit non-claim wording"
fi

echo "== result: $FAILS failure(s) =="
[ "$FAILS" -eq 0 ]
