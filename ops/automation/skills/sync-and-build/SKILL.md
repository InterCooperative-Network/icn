---
name: sync-and-build
description: Build and verify the ICN website. The website reads docs directly from the monorepo's docs/ — no sync step needed.
disable-model-invocation: true
truth_contract:
  canonical_sources:
    - ops/state/config/repo-map.json    # workspace root, website path
  live_load_required:
    - "git diff --name-only HEAD~1 -- docs/"    # what changed in docs
    - "cd ${REPO_ROOT}/website && npm run build 2>&1 | tail -5"
  examples_only: []
  never_hardcode:
    - workspace paths (use git rev-parse --show-toplevel)
    - sync scripts (there are none — website reads docs/ via path.resolve directly)
---

Run the ICN website build pipeline. Execute each step and report results.

> **Architecture note**: `website/` reads `../docs/` directly via `path.resolve` at Astro build time.
> There is no sync script and no content to copy. Edits to `docs/` are live on the next build.

## Step 1: Check what changed in docs/

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
git -C "${REPO_ROOT}" log --oneline -5 -- docs/
```

Report: last 5 commits that touched docs/ (or "no recent doc changes" if empty).

## Step 2: Install dependencies (if needed)

```bash
cd "${REPO_ROOT}/website" && npm ci 2>&1 | tail -5
```

Skip if `node_modules/` already exists and no `package.json` changes.

## Step 3: Build the website

```bash
cd "${REPO_ROOT}/website" && npm run build 2>&1
```

Report:
- ✅ Build succeeded (show build time if visible in output)
- ❌ Build failed — show the first error message

If build fails, stop here and report the error. Do not proceed to step 4.

## Step 4: Check for broken internal links (informational)

```bash
grep -r 'href="/' "${REPO_ROOT}/website/src" --include="*.astro" -l 2>/dev/null | head -10
```

Report: list of pages with absolute hrefs (informational only, not blocking).

## Step 5: Summary

Report as:
```
Website build complete
  Recent doc changes: N commits
  Build: ✅ / ❌
  Pages with absolute hrefs: N (informational)
```
