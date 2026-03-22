---
name: changelog
description: Generate a CHANGELOG.md entry from commits since the last git tag. Groups by type (feat/fix/refactor/etc), formats as Keep-a-Changelog.
argument-hint: "[version] [--dry-run]"
user-invocable: true
allowed-tools: "Bash, Read, Edit"
---

Generate a Keep-a-Changelog entry from commits since the last git tag and prepend it to CHANGELOG.md.

## Steps

### 1. Gather context

Run in parallel:
- `git tag --sort=-version:refname | head -5` — find last release tag
- `git branch --show-current` — confirm branch
- `git log --oneline $(git describe --tags --abbrev=0 2>/dev/null || echo "")..HEAD` — commits since last tag (or all if no tags)

### 2. Determine version

- If `$ARGUMENTS` contains a version (e.g. `0.2.0`), use that.
- Otherwise, infer from last tag + increment (patch for fixes only, minor if any feat, major if any breaking change marked with `!`).
- Print the determined version before proceeding.

### 3. Parse commits into groups

Group commits by conventional-commit type:
- `feat` → `### Added`
- `fix` → `### Fixed`
- `refactor` → `### Changed`
- `docs` → `### Documentation`
- `test` → `### Tests`
- `chore`, `ci` → `### Internal`
- `perf` → `### Performance`

Skip merge commits (`Merge pull request`, `Merge branch`). Skip commits with scope `ops` or `chore(ops)`.

Extract PR number from commit message if present (e.g., `(#1234)`) and format as link: `([#1234](../../pull/1234))`.

### 4. Format the entry

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- feat(scope): description ([#N](../../pull/N))

### Fixed
- fix(scope): description

### Changed
- ...
```

Use today's date (ISO 8601). Omit sections that have no entries.

### 5. Prepend to CHANGELOG.md

- Read current CHANGELOG.md
- Insert the new entry after the `# Changelog` header line (or at top if no header)
- Write the updated file

Skip this step if `$ARGUMENTS` contains `--dry-run` (print entry to stdout only, don't write).

### 6. Confirm

Print: "Changelog updated for vX.Y.Z with N entries."
List the sections and entry counts.

## Important

- Do NOT create a git commit — leave that to the user.
- If CHANGELOG.md doesn't exist, create it with the `# Changelog` header followed by the entry.
- Keep entries concise: one line per commit.
- Scope is law: do not rewrite or reformat existing CHANGELOG entries.
