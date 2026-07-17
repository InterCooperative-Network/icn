#!/usr/bin/env bash
# Reproducible generator for the portable bootable vertical-slice evaluator
# package. Assembles a distributable ZIP from DECLARED inputs — a built appliance
# image (a build-image.sh output, demo profile) + its typed manifest + the
# repo-owned templates in ./templates — and fails closed on any inconsistency.
#
# This is the "package assembly + validation + zip" half of the lane. Image
# BUILDING stays in build-image.sh; this script never builds an image, never
# reaches into a developer home directory, and bakes no host path into the
# output (the packaged manifest is rewritten to bare basenames).
#
# Usage:
#   build-evaluator-package.sh --image PATH.qcow2 --manifest PATH.manifest.json \
#       --out DIR [--source-commit SHA] [--allow-commit-mismatch] [--no-zip]
#
# Fails closed if: image missing; manifest not JSON; manifest.image_sha256 !=
# sha256(image); manifest.git_commit != source commit; honest-posture flags wrong;
# a required doc/script is absent; a script fails syntax; the assembled package
# fails the static validator (privacy/bind/checksum/layout).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=/dev/null
. "$SCRIPT_DIR/package-spec.env"
TEMPLATES="$SCRIPT_DIR/templates"

IMAGE="" ; MANIFEST_IN="" ; OUTDIR="" ; SOURCE_COMMIT="" ; ALLOW_MISMATCH=0 ; DO_ZIP=1
log() { printf '[evaluator-pkg] %s\n' "$*"; }
err() { printf '[evaluator-pkg] ERROR: %s\n' "$*" >&2; }
die() { err "$*"; exit 1; }

while [ "$#" -gt 0 ]; do
  case "$1" in
    --image) shift; IMAGE="${1:-}" ;;
    --manifest) shift; MANIFEST_IN="${1:-}" ;;
    --out) shift; OUTDIR="${1:-}" ;;
    --source-commit) shift; SOURCE_COMMIT="${1:-}" ;;
    --allow-commit-mismatch) ALLOW_MISMATCH=1 ;;
    --no-zip) DO_ZIP=0 ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown arg: $1" ;;
  esac
  shift
done

[ -n "$IMAGE" ] && [ -f "$IMAGE" ] || die "missing/unreadable --image"
[ -n "$MANIFEST_IN" ] && [ -f "$MANIFEST_IN" ] || die "missing/unreadable --manifest"
[ -n "$OUTDIR" ] || die "missing --out"
for t in sha256sum python3 find sed; do command -v "$t" >/dev/null 2>&1 || die "missing tool: $t"; done

PKG_NAME="${PKG_STEM}-${PKG_VERSION}-${PKG_ARCH}"
log "package: $PKG_NAME (profile=$PKG_PROFILE)"

# ---- read + validate the source manifest ----------------------------------
log "hashing image (streaming)…"
IMG_SHA="$(sha256sum "$IMAGE" | awk '{print $1}')"

read -r M_SHA M_COMMIT M_VER M_ARCH M_NP M_SIGNED M_DEMO M_FMT < <(python3 - "$MANIFEST_IN" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
def b(x): return "true" if x is True else "false"
print(d.get("image_sha256",""), d.get("git_commit",""), d.get("version",""),
      d.get("arch",""), b(d.get("non_production")), b(d.get("signed")), b(d.get("demo_profile")),
      d.get("image_format","") or "-")
PY
)

# The lane packages a qcow2 (the launcher + IMAGE_BASENAME assume it). Reject any
# other source format so a raw/other image is never shipped under a .qcow2 name.
[ "$M_FMT" = "qcow2" ] || die "manifest image_format=$M_FMT (this lane only packages qcow2)"
case "$IMAGE_BASENAME" in *.qcow2) ;; *) die "IMAGE_BASENAME $IMAGE_BASENAME is not a .qcow2 name" ;; esac

[ "$M_SHA" = "$IMG_SHA" ] || die "manifest image_sha256 ($M_SHA) != actual image sha ($IMG_SHA)"
log "image sha matches manifest: OK"
[ "$M_NP" = "$REQUIRE_NON_PRODUCTION" ] || die "manifest non_production=$M_NP (require $REQUIRE_NON_PRODUCTION)"
[ "$M_SIGNED" = "$REQUIRE_SIGNED" ] || die "manifest signed=$M_SIGNED (require $REQUIRE_SIGNED)"
[ "$M_DEMO" = "$REQUIRE_DEMO_PROFILE" ] || die "manifest demo_profile=$M_DEMO (require $REQUIRE_DEMO_PROFILE)"
[ "${M_VER%%-*}" = "$PKG_VERSION" ] || die "manifest version $M_VER disagrees with spec version $PKG_VERSION"
[ "$M_ARCH" = "$PKG_ARCH" ] || die "manifest arch $M_ARCH disagrees with spec arch $PKG_ARCH"
log "honest-posture flags + version/arch: OK"

if [ -z "$SOURCE_COMMIT" ]; then SOURCE_COMMIT="$M_COMMIT"; fi
if [ "$M_COMMIT" != "$SOURCE_COMMIT" ]; then
  if [ "$ALLOW_MISMATCH" -eq 1 ]; then
    log "WARNING: manifest git_commit ($M_COMMIT) != requested source commit ($SOURCE_COMMIT) — allowed by flag"
  else
    die "manifest git_commit ($M_COMMIT) != requested source commit ($SOURCE_COMMIT) (use --allow-commit-mismatch to override)"
  fi
fi
echo "$M_COMMIT" | grep -qE '^[0-9a-f]{40}$' || die "manifest git_commit is not a 40-hex sha"
log "source commit: $M_COMMIT"

# Injection guard: every value interpolated into the sed substitution below MUST
# be free of sed metacharacters, so a crafted manifest cannot inject a sed command
# (e.g. GNU sed's `e`, which executes a shell command) to run code on the builder
# host. The manifest version is only prefix-checked above, so validate the whole
# string here; git_commit/image_sha are already hex-validated; the spec-derived
# basenames are checked defensively too. Legit values are [0-9A-Za-z._-] only.
echo "$M_VER"  | grep -qE '^[0-9A-Za-z._-]+$' || die "manifest version contains unsafe characters — refusing"
echo "$IMG_SHA" | grep -qE '^[0-9a-f]{64}$'   || die "image sha256 is malformed"
for _v in "$IMAGE_BASENAME" "$MANIFEST_BASENAME" "$PKG_NAME"; do
  echo "$_v" | grep -qE '^[0-9A-Za-z._-]+$' || die "spec-derived value has unsafe characters: $_v"
done

# ---- assemble staging tree -------------------------------------------------
STAGE="$OUTDIR/$PKG_NAME"
rm -rf "$STAGE"
mkdir -p "$STAGE/scripts" "$STAGE/docs"

subst() { # substitute placeholders from stdin
  sed -e "s/@IMAGE_BASENAME@/$IMAGE_BASENAME/g" \
      -e "s/@MANIFEST_BASENAME@/$MANIFEST_BASENAME/g" \
      -e "s/@PKG_NAME@/$PKG_NAME/g" \
      -e "s/@IMAGE_VERSION@/$M_VER/g" \
      -e "s/@SOURCE_COMMIT@/$M_COMMIT/g" \
      -e "s/@IMAGE_SHA256@/$IMG_SHA/g"
}

copy_tmpl() { # $1 rel path under templates/ and stage/
  [ -f "$TEMPLATES/$1" ] || die "template missing: $1"
  subst < "$TEMPLATES/$1" > "$STAGE/$1"
}
copy_tmpl "README.md"
copy_tmpl "setup-and-run.sh"
copy_tmpl "start-icn-demo-vm.sh"
copy_tmpl "scripts/run-demo.sh"
copy_tmpl "scripts/verify.sh"
copy_tmpl "docs/START-HERE.md"
copy_tmpl "docs/RUNBOOK.md"
copy_tmpl "docs/WALKTHROUGH.md"
for ex in "${EXECUTABLE_FILES[@]}"; do chmod 0755 "$STAGE/$ex"; done

# image + sanitized manifest (rewrite host paths to bare basenames)
cp "$IMAGE" "$STAGE/$IMAGE_BASENAME"
python3 - "$MANIFEST_IN" "$STAGE/$MANIFEST_BASENAME" "$IMAGE_BASENAME" <<'PY'
import json,sys,os
src,dst,image_base=sys.argv[1:4]
d=json.load(open(src))
# strip host-specific absolute paths → bare, portable basenames
if "image_path" in d: d["image_path"]=image_base
if "base_image_path" in d and d["base_image_path"]:
    d["base_image_path"]=os.path.basename(str(d["base_image_path"]))
json.dump(d, open(dst,"w"), indent=2, sort_keys=False)
open(dst,"a").write("\n")
PY
log "assembled tree + sanitized manifest (paths → basenames)"

# ---- optional PDFs (deterministic doc set = md always; pdf best-effort) -----
PDF_TOOL=""
if command -v pandoc >/dev/null 2>&1; then PDF_TOOL="pandoc"; fi
if [ -n "$PDF_TOOL" ]; then
  for md in "${DOC_MARKDOWN[@]}"; do
    pdf="${md%.md}.pdf"
    if pandoc "$STAGE/$md" -o "$STAGE/$pdf" >/dev/null 2>&1; then log "pdf: $pdf"; else log "pdf skipped (convert failed): $pdf"; fi
  done
else
  log "no PDF converter (pandoc) — shipping Markdown only (matching PDFs omitted this build)"
fi

# ---- syntax gate before checksums -----------------------------------------
while IFS= read -r -d '' s; do
  bash -n "$s" || die "script failed syntax: ${s#"$STAGE"/}"
done < <(find "$STAGE" -type f -name '*.sh' -print0)

# ---- SHA256SUMS over the package (sorted, relative) ------------------------
( cd "$STAGE" && find . -type f ! -name SHA256SUMS -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS )
log "wrote SHA256SUMS ($(wc -l < "$STAGE/SHA256SUMS") entries)"

# ---- static validation (fail closed) --------------------------------------
log "running static validator…"
"$SCRIPT_DIR/validate-evaluator-package.sh" "$STAGE" --expect-commit "$M_COMMIT" \
  || die "assembled package failed static validation"

# ---- zip + outer checksum --------------------------------------------------
if [ "$DO_ZIP" -eq 1 ]; then
  command -v zip >/dev/null 2>&1 || die "missing tool: zip"
  ZIP="$OUTDIR/${PKG_NAME}.zip"
  rm -f "$ZIP"
  # Deterministic archive: identical declared inputs → identical ZIP bytes.
  # (1) Canonical modes independent of the caller umask (zip -X still records Unix
  #     mode bits, so umask 022 vs 077 would otherwise diverge): dirs + executables
  #     0755, every other file 0644.
  find "$STAGE" -type d -exec chmod 0755 {} +
  find "$STAGE" -type f -exec chmod 0644 {} +
  for ex in "${EXECUTABLE_FILES[@]}"; do chmod 0755 "$STAGE/$ex"; done
  # (2) Pin every entry's mtime to the manifest build timestamp (fixed epoch
  #     fallback), feed a sorted entry list, use -X (drop uid/gid/extra fields)
  #     under TZ=UTC. (mtime/mode changes are after SHA256SUMS — those hashes are
  #     content-only, unaffected.)
  REF_EPOCH="$(python3 - "$STAGE/$MANIFEST_BASENAME" <<'PY'
import json,sys,datetime
try:
    ts=json.load(open(sys.argv[1])).get("build_timestamp_utc","")
    print(int(datetime.datetime.fromisoformat(ts.replace("Z","+00:00")).timestamp()))
except Exception:
    print(1577836800)  # 2020-01-01T00:00:00Z fixed fallback
PY
)"
  find "$STAGE" -exec touch -h -d "@$REF_EPOCH" {} +
  ( cd "$OUTDIR" && find "$PKG_NAME" -print | LC_ALL=C sort | TZ=UTC zip -q -X -@ "${PKG_NAME}.zip" >/dev/null )
  ( cd "$OUTDIR" && sha256sum "${PKG_NAME}.zip" > "${PKG_NAME}.zip.sha256" )
  log "wrote $ZIP"
  log "outer sha256: $(awk '{print $1}' "$OUTDIR/${PKG_NAME}.zip.sha256")"
else
  log "staging complete (--no-zip): $STAGE"
fi
log "DONE. Non-production, unsigned evaluator package."
