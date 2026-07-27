#!/usr/bin/env bash
# Unit tests for validate-evaluator-package.sh. Builds synthetic packages in a
# temp dir (NO qcow2 — runs in --no-image CI mode) and asserts the validator
# PASSES a clean package and FAILS each injected defect class. No KVM, no big
# image, so this runs on an ordinary CI runner.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EV="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=/dev/null
. "$EV/package-spec.env"
PKG_NAME="${PKG_STEM}-${PKG_VERSION}-${PKG_ARCH}"
VALIDATE="$EV/validate-evaluator-package.sh"
TEMPLATES="$EV/templates"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
PASS=0 FAILN=0
ok(){ printf '  PASS  %s\n' "$1"; PASS=$((PASS+1)); }
no(){ printf '  FAIL  %s\n' "$1"; FAILN=$((FAILN+1)); }

# ---- build a clean synthetic package (no image) ---------------------------
mkpkg() {
  local root="$1/$PKG_NAME"; rm -rf "$root"; mkdir -p "$root/scripts" "$root/docs"
  local commit="0123456789abcdef0123456789abcdef01234567"
  local ish; ish="$(printf 'synthetic-image' | sha256sum | awk '{print $1}')"
  subst() { sed -e "s/@IMAGE_BASENAME@/$IMAGE_BASENAME/g" -e "s/@MANIFEST_BASENAME@/$MANIFEST_BASENAME/g" \
        -e "s/@PKG_NAME@/$PKG_NAME/g" -e "s/@IMAGE_VERSION@/${PKG_VERSION}-demo/g" \
        -e "s/@SOURCE_COMMIT@/$commit/g" -e "s/@IMAGE_SHA256@/$ish/g"; }
  for f in README.md setup-and-run.sh start-icn-demo-vm.sh scripts/run-demo.sh scripts/verify.sh \
           docs/START-HERE.md docs/RUNBOOK.md docs/WALKTHROUGH.md; do
    subst < "$TEMPLATES/$f" > "$root/$f"
  done
  chmod 0755 "$root"/setup-and-run.sh "$root"/start-icn-demo-vm.sh "$root"/scripts/*.sh
  cat > "$root/$MANIFEST_BASENAME" <<EOF
{ "manifest_version":1, "version":"${PKG_VERSION}-demo", "arch":"$PKG_ARCH",
  "image_sha256":"$ish", "git_commit":"$commit",
  "image_path":"$IMAGE_BASENAME", "base_image_path":"base.qcow2",
  "non_production":true, "signed":false, "demo_profile":true }
EOF
  ( cd "$root" && find . -type f ! -name SHA256SUMS -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS )
  echo "$root"
}
run() { "$VALIDATE" "$1" --no-image >"$TMP/v.log" 2>&1; }

# 1. clean package PASSES
R="$(mkpkg "$TMP/clean")"; if run "$R"; then ok "clean package validates"; else no "clean package should validate"; cat "$TMP/v.log"; fi

# 2. absolute /home path leak FAILS
R="$(mkpkg "$TMP/homeleak")"; echo '# see /home/ubuntu/secret/path' >> "$R/docs/RUNBOOK.md"
( cd "$R" && find . -type f ! -name SHA256SUMS -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS )
if run "$R"; then no "should reject /home path leak"; else ok "rejects /home path leak"; fi

# 3. RFC1918 IP FAILS  (generic RFC1918 fixture, not a real site address)
R="$(mkpkg "$TMP/ip")"; echo 'connect to 10.0.0.5 now' >> "$R/README.md"  # sanitize-ok: synthetic test fixture
( cd "$R" && find . -type f ! -name SHA256SUMS -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS )
if run "$R"; then no "should reject RFC1918 IP"; else ok "rejects RFC1918 IP"; fi

# 4. internal hostname FAILS  (synthetic internal host; triggers rehearsal.icn + .internal)
R="$(mkpkg "$TMP/host")"; echo 'https://rehearsal.icn.example.internal/' >> "$R/README.md"
( cd "$R" && find . -type f ! -name SHA256SUMS -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS )
if run "$R"; then no "should reject internal hostname"; else ok "rejects internal hostname"; fi

# 5. non-loopback hostfwd FAILS
R="$(mkpkg "$TMP/bind")"; sed -i 's/hostfwd=tcp:127\.0\.0\.1:/hostfwd=tcp:0.0.0.0:/' "$R/start-icn-demo-vm.sh"
( cd "$R" && find . -type f ! -name SHA256SUMS -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS )
if run "$R"; then no "should reject non-loopback hostfwd"; else ok "rejects non-loopback hostfwd"; fi

# 6. broken checksum FAILS
R="$(mkpkg "$TMP/sum")"; echo 'tampered' >> "$R/README.md"
if run "$R"; then no "should reject broken checksum"; else ok "rejects broken checksum (README changed after SHA256SUMS)"; fi

# 7. missing required doc FAILS
R="$(mkpkg "$TMP/missing")"; rm -f "$R/docs/WALKTHROUGH.md"
( cd "$R" && find . -type f ! -name SHA256SUMS -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS )
if run "$R"; then no "should reject missing doc"; else ok "rejects missing required doc"; fi

# 8. dishonest manifest (signed:true) FAILS
R="$(mkpkg "$TMP/signed")"; sed -i 's/"signed":false/"signed":true/' "$R/$MANIFEST_BASENAME"
( cd "$R" && find . -type f ! -name SHA256SUMS -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS )
if run "$R"; then no "should reject signed:true"; else ok "rejects dishonest signed:true"; fi

# 9. host-path manifest (image_path with dir) FAILS
R="$(mkpkg "$TMP/mpath")"; sed -i "s#\"image_path\":\"$IMAGE_BASENAME\"#\"image_path\":\"/home/ubuntu/out/$IMAGE_BASENAME\"#" "$R/$MANIFEST_BASENAME"
( cd "$R" && find . -type f ! -name SHA256SUMS -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS )
if run "$R"; then no "should reject non-basename image_path"; else ok "rejects non-basename image_path (host path leak)"; fi

# 10. JWT-shaped credential FAILS  (synthetic non-credential fixture)
R="$(mkpkg "$TMP/jwt")"; printf 'credential eyJTWVNURVNU.ZmFrZVRFU1Rml4dHVyZQ\n' >> "$R/docs/RUNBOOK.md"  # sanitize-ok: synthetic test fixture
( cd "$R" && find . -type f ! -name SHA256SUMS -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS )
if run "$R"; then no "should reject JWT-shaped value"; else ok "rejects JWT-shaped credential"; fi

# 11. GENERATOR command-injection defense: a crafted manifest version that would
#     inject a GNU sed `e` command must be refused, and the injected command must
#     NOT run on the builder host. Uses a tiny fake image (no KVM, CI-runnable).
GEN="$EV/build-evaluator-package.sh"
FAKEIMG="$TMP/fake.qcow2"; printf 'not-a-real-image' > "$FAKEIMG"
FSHA="$(sha256sum "$FAKEIMG" | awk '{print $1}')"
MARKER="$TMP/EVIL_MARKER"; rm -f "$MARKER"
python3 - "$TMP/craft.json" "$PKG_VERSION" "$PKG_ARCH" "$FSHA" "$IMAGE_BASENAME" "$MARKER" <<'PY'
import json,sys
craft,ver,arch,sha,imgbase,marker=sys.argv[1:7]
# version passes the prefix check (ver-...) but injects a sed `e` command
d={"manifest_version":1,"version":f"{ver}-x/;e${{IFS}}touch${{IFS}}{marker};#","arch":arch,
   "image_sha256":sha,"git_commit":"0123456789abcdef0123456789abcdef01234567",
   "image_path":imgbase,"base_image_path":"b.qcow2","image_format":"qcow2",
   "non_production":True,"signed":False,"demo_profile":True}
json.dump(d,open(craft,"w"))
PY
"$GEN" --image "$FAKEIMG" --manifest "$TMP/craft.json" --out "$TMP/geninj" --no-zip >/dev/null 2>&1 || true
if [ -e "$MARKER" ]; then no "generator executed an injected manifest-version command (RCE)"; else ok "generator refuses injected manifest version (no code execution)"; fi

echo "== $PASS passed, $FAILN failed =="
[ "$FAILN" -eq 0 ]
