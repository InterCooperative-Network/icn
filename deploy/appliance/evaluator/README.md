# Portable evaluator package lane

Repository-owned, reproducible generation of the **portable bootable vertical
slice** — the downloadable evaluator package an external reviewer runs locally on
their own Debian/Ubuntu machine to see the current browser-operable
standing → action card → completion → receipt path.

This lane turns the package from an ad-hoc pile of locally assembled files into a
declared-input, fail-closed, validated artifact.

## What this is (and is NOT)

- A **portable evaluator** artifact: localhost-only exposure, disposable VM
  overlay, one-command setup. Unsigned, `non_production`, `demo_profile`.
- **NOT** the LAN Rehearsal Node (that is the operator-controlled internal
  single-TLS-origin deployment — a different profile, threat model, and audience;
  see `deploy/appliance/lan/`). The two profiles are deliberately not collapsed.
- **NOT** production, pilot, organizer-approved, accessibility-complete, or
  federated. It demonstrates a bounded slice of what exists today.

## Naming and provenance (identity correction, 2026-07-19)

The package stem is **`icn-portable-evaluator`**. Earlier packages in this line
(the ad-hoc 0.0.2 pre-releases and the first lane-built 0.0.3 release) were
published under an externally introduced **"common sense"** name that arrived
with an externally assembled distribution and was never an ICN-ratified
identity; it belongs to an unrelated project and does not describe anything in
this repository. The *payloads* of those releases are genuine ICN appliance
images (manifest `git_commit` values are real commits of this repository, and
0.0.3 was runtime-witnessed on the exact published bytes) — only the name was
foreign. Release tags and asset filenames for ≤0.0.3 are retained unchanged so
published checksums keep verifying; their release pages are retitled and carry
this correction. From 0.0.4 onward the stem above is the only identity.

## Two-step separation of concerns

1. **Build the image** — `deploy/appliance/build-image.sh` with the demo profile
   (`ICN_APPLIANCE_DEMO_PROFILE=1`, and NOT `ICN_APPLIANCE_LAN_PROFILE`). It emits
   the qcow2 + a typed manifest (`icnctl appliance emit-manifest`).
2. **Assemble the package** — `build-evaluator-package.sh` takes that image +
   manifest as declared inputs and produces the distributable ZIP. It never builds
   an image and never bakes a host path into the output.

## Generate

```bash
deploy/appliance/evaluator/build-evaluator-package.sh \
  --image   /path/to/icn-appliance-<ver>-demo-amd64.qcow2 \
  --manifest /path/to/icn-appliance-<ver>-demo-amd64.manifest.json \
  --out     /path/to/output-dir
# → output-dir/icn-portable-evaluator-<ver>-amd64/           (staging tree)
# → output-dir/icn-portable-evaluator-<ver>-amd64.zip        (distributable)
# → output-dir/icn-portable-evaluator-<ver>-amd64.zip.sha256 (outer checksum)
```

Add `--no-zip` to stop at the validated staging tree, or `--source-commit SHA` to
assert provenance against a specific commit (defaults to the manifest's
`git_commit`).

## Reproducibility contract

- **Identity** is declared once in `package-spec.env` (name, version, arch,
  profile, image/manifest basenames, required layout, executable set).
- **Templates** in `templates/` are the repo-owned source for every script and
  doc. Placeholders (`@PKG_NAME@`, `@IMAGE_BASENAME@`, `@MANIFEST_BASENAME@`,
  `@IMAGE_VERSION@`, `@SOURCE_COMMIT@`, `@IMAGE_SHA256@`) are stamped at generation
  from the actual manifest, so the RUNBOOK's recorded commit/sha are always the
  real ones for the packaged image.
- **Determinism**: given identical declared inputs (templates, spec, image,
  manifest), the ZIP is byte-reproducible — entry order is sorted and every
  entry's mtime is pinned to the manifest `build_timestamp_utc` (fixed-epoch
  fallback) under `TZ=UTC` with `zip -X`. The only inputs outside that function
  are the qcow2 bytes (built upstream) and the optional PDFs (generated only when
  `pandoc` is present; the Markdown doc set always ships, PDFs are best-effort and
  therefore the one non-reproducible surface if pandoc availability differs).

## Fail-closed guarantees

The generator aborts (non-zero) if: the image is missing; the manifest is not
JSON; `image_sha256 != sha256(image)`; `git_commit != source commit`; the honest-
posture flags are wrong (`non_production!=true`, `signed!=false`,
`demo_profile!=true`); version/arch disagree with the spec; a required doc/script
is absent; a script fails `bash -n`; or the assembled package fails the static
validator below.

## Static validation (KVM-free)

`validate-evaluator-package.sh <pkg-root> [--no-image] [--expect-commit SHA]`
checks layout, exec bits, `bash -n` + ShellCheck, `SHA256SUMS`, manifest metadata
(honest posture, basename-only paths, sha cross-check, commit), a privacy/forbidden
scan (no host paths, RFC1918 IPs, internal hostnames, tunnel URLs, JWT/PEM/bearer,
DID literals in scripts, personal names, overclaims), archive safety (no symlinks),
launcher loopback-default binds, and non-claim wording.

`--no-image` mode runs every non-image check without the ~1GB qcow2, so CI catches
packaging/script/checksum/manifest/privacy/bind regressions on an ordinary runner.
`tests/test-evaluator-package.sh` exercises the validator against synthetic clean +
defective packages.

## What is NOT here

- The qcow2 and the release ZIP are never committed to Git (large, non-source).
- This lane does not publish a GitHub release. Publishing is a separate,
  explicitly-authorized manual step after the validator is green.
