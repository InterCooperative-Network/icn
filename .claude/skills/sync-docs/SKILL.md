---
name: sync-docs
description: Check for documentation drift — crate inventory vs workspace, GOLDEN_PROMPT freshness, broken doc links.
argument-hint: "[--fix-index]"
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
---

Audit documentation drift. Reports problems; does not auto-fix (unless --fix-index).

## Checks

### 1. Crate inventory drift

Read workspace members:
```bash
grep -A999 '^\[workspace\]' icn/Cargo.toml | grep '"' | grep -v '#' | sed 's/.*"\(.*\)".*/\1/'
```

Compare against crate names mentioned in:
- `docs/planning/icn-crate-reference.md`
- `docs/ARCHITECTURE.md` (search for `icn-` patterns)

Report:
- Crates in workspace but NOT documented
- Crates documented but NOT in workspace (may be removed/renamed)

### 2. GOLDEN_PROMPT.md freshness

```bash
# Last modification date
git log -1 --format="%ci" -- docs/GOLDEN_PROMPT.md

# Last git tag date
git log -1 --format="%ci" $(git describe --tags --abbrev=0 2>/dev/null) 2>/dev/null || echo "no tags"

# Days since last update
python3 -c "
from datetime import datetime, timezone
import subprocess
result = subprocess.run(['git','log','-1','--format=%ct','--','docs/GOLDEN_PROMPT.md'], capture_output=True, text=True)
if result.stdout.strip():
    ts = int(result.stdout.strip())
    days = (datetime.now(timezone.utc).timestamp() - ts) / 86400
    print(f'GOLDEN_PROMPT.md last updated {days:.0f} days ago')
else:
    print('GOLDEN_PROMPT.md: unknown modification date')
"
```

Warn if > 14 days since last update.

### 3. KNOWLEDGE_INDEX.yaml freshness

Check if `docs/dev-journal/KNOWLEDGE_INDEX.yaml` exists:
- If not: report as missing, suggest creating it
- If exists: check `last_updated` field vs today, warn if > 7 days stale

### 4. Broken internal doc links (sampled)

Search for markdown links in `docs/` that reference files starting with `../` or `./`:
```bash
grep -rn '\]\(\.\.' docs/ --include='*.md' | head -30
```

For a sample of 10 such links, verify the target file exists. Report missing targets.

### 5. docs/INDEX.md completeness

Count `.md` files in `docs/`:
```bash
find docs/ -name '*.md' | wc -l
```

Count entries in `docs/INDEX.md`:
```bash
grep -c '\.md' docs/INDEX.md 2>/dev/null || echo "INDEX.md missing"
```

Report gap (files not indexed).

## Output

```
=== Docs Sync Report ===

Crate drift:
  In workspace, not documented: <list or "none">
  Documented, not in workspace: <list or "none">

GOLDEN_PROMPT.md: <N> days old <WARN if > 14>
KNOWLEDGE_INDEX.yaml: <exists/missing> <staleness>

Broken links: <N> checked, <N> broken
  - <file>:<line> -> <target> (missing)

INDEX.md: <N> .md files in docs/, <N> in INDEX.md, <gap> unindexed
```

## --fix-index

If `$ARGUMENTS` includes `--fix-index`:
- Append missing doc paths to `docs/INDEX.md` under an `## Unindexed` section
- Print: "Added <N> entries to docs/INDEX.md — review and categorize"
- Do NOT auto-commit

## Important

- Report only. No auto-fixes except `--fix-index`.
- Do not modify any Rust files.
- If a crate is in workspace but marked as deprecated/example, note that instead of flagging as undocumented.
