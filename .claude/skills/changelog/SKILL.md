---
name: changelog
description: Generate a Keep-a-Changelog entry from commits since the last git tag. Prepends to CHANGELOG.md.
argument-hint: "[--dry-run] [--since <tag>]"
user-invocable: true
allowed-tools: "Bash, Read, Edit"
---

Generate a changelog entry from commits since the last tag. Follows Keep-a-Changelog format.

## Steps

### 1. Find range

```bash
# Last tag (or use $ARGUMENTS --since <tag> if specified)
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
if [ -z "$LAST_TAG" ]; then
  echo "No tags found. Using all commits."
  RANGE="HEAD"
else
  echo "Last tag: $LAST_TAG"
  RANGE="${LAST_TAG}..HEAD"
fi
```

### 2. Collect commits

```bash
git log $RANGE --oneline --no-merges --pretty=format:"%s"
```

### 3. Group by conventional commit prefix

Map commits to changelog categories:
| Prefix | Category |
|--------|----------|
| `feat` | **Added** |
| `fix` | **Fixed** |
| `refactor` | **Changed** |
| `docs` | **Documentation** |
| `test` | **Testing** |
| `chore`, `ci`, `build` | **Infrastructure** |
| `perf` | **Performance** |
| `security` | **Security** |

Commits without a prefix go under **Changed**.

Strip the `type(scope): ` prefix, capitalize first letter of the summary.

### 4. Format the entry

```markdown
## [Unreleased] - YYYY-MM-DD

### Added
- ...

### Fixed
- ...

### Changed
- ...
```

Date = today (`date +%Y-%m-%d`).

Only include sections that have entries. Omit empty sections.

### 5. Output / write

- If `$ARGUMENTS` includes `--dry-run`: print the entry to stdout only. Do NOT write to file.
- Otherwise: prepend the entry to `CHANGELOG.md` (after the `# Changelog` header line if present).
  - Read current `CHANGELOG.md`
  - Insert new entry after the header (or at top if no header)
  - Write back

### 6. Report

Print:
```
Changelog entry generated for <N> commits since <tag>.
Categories: Added(<n>), Fixed(<n>), Changed(<n>), ...
Written to CHANGELOG.md (or --dry-run: printed only)
Review and commit: git add CHANGELOG.md && git commit -m "chore(release): update changelog"
```

## Important

- Do NOT auto-commit the changelog. User reviews first.
- If a commit message is unclear or too long (>80 chars), truncate at word boundary + "..."
- Skip commits with `[skip changelog]` in the message.
- If `CHANGELOG.md` doesn't exist, create it with a standard header first.
