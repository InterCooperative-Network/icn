# Mismatch Rubric

## Severity

- `blocker`
- Incorrect security/architecture boundary claims
- Incorrect CI gate claims that affect merge safety
- Broken index/navigation links for primary docs

- `high`
- Wrong commands or wrong working directories
- Wrong/missing file paths in core onboarding docs
- Contradictory "current status" statements without dates

- `medium`
- Stale quantitative claims (counts/percentages) without date context
- Terminology drift or outdated examples that do not alter safety posture

## Self-Scoring (0-5)

Score each changed doc on:

1. `accuracy` - matches canonical truth source
2. `completeness` - fixes full mismatch scope, not partial wording
3. `consistency` - aligns with neighboring docs and index
4. `verifiability` - includes evidence path/date or command to verify

Trigger correction if any score is below `4`.

## Acceptance format

Record each changed doc as:

`doc_path | mismatch | truth_source | fix_summary | verify_cmd | score(A,C,C,V)`
