# Curriculum Update Process

This process keeps onboarding content aligned with code changes and flags
lessons that need review.

## When to update
- Any change to code paths referenced in `docs/onboarding/` should trigger a
  review of the related module.
- API surface changes (gateway, SDK) require module 7 review.
- Runtime or actor initialization changes require module 3 review.

## Automated flagging
Use the script `scripts/check-onboarding-maps.sh` in CI or locally. It:
- scans onboarding docs for backticked file paths
- fails if referenced files are missing
- flags if referenced files changed in the current git diff

Example:
```bash
./scripts/check-onboarding-maps.sh origin/main
```

## Manual review checklist
- Verify module objectives still match code behavior
- Confirm key reading links are valid
- Re-run exercises to ensure they still work
- Update any code references in lessons

## Ownership
- Each module should have an owner in sprint planning
- Owners review their modules when related crates change
