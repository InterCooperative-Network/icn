---
name: sync-and-build
description: Sync ICN documentation from icn/docs/ to the website, then build and verify. Cross-repo content pipeline.
disable-model-invocation: true
---

Run the ICN → website content pipeline. Execute each step and report results.

## Step 1: Check what changed in icn/docs/

```bash
git -C /home/ubuntu/projects/icn log --oneline -5 -- docs/
```

Report: last 5 commits that touched docs/ (or "no recent doc changes" if empty).

## Step 2: Run the sync script

```bash
cd /home/ubuntu/projects/icn-website && bash scripts/sync-from-icn.sh 2>&1
```

Report:
- How many files were synced (count lines of output mentioning "syncing" or check `src/content/docs/` file count)
- Any errors from the script

## Step 3: Build the website

```bash
cd /home/ubuntu/projects/icn-website && npm run build 2>&1
```

Report:
- ✅ Build succeeded (show build time if visible in output)
- ❌ Build failed — show the first error message

If build fails, stop here and report the error. Do not proceed to step 4.

## Step 4: Check for broken internal links (informational)

```bash
grep -r 'href="/' /home/ubuntu/projects/icn-website/src --include="*.astro" -l 2>/dev/null | head -10
```

Report: list of pages with absolute hrefs (these might need attention — informational only, not blocking).

## Step 5: Summary

Report as:
```
Sync complete
  Docs commits: N
  Files synced: N
  Build: ✅ / ❌
  Pages with absolute hrefs: N (informational)
```
