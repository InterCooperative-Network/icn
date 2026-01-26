---
name: Kernel/App Separation Work
about: Track extraction of domain logic from kernel to apps
title: ''
labels: ['core', 'architecture']
assignees: ''
---

## Summary
<!-- What domain logic is being extracted? -->

## Current Location
<!-- Which kernel crate(s) contain this logic? -->

## Target Location
<!-- Which app will own this logic? -->

## Infection Points
<!-- How many direct imports/usages exist? Use: grep -r 'pattern' crates/ | wc -l -->

## Migration Pattern
<!-- How will the kernel access this via PolicyOracle? -->

## Acceptance Criteria
- [ ] No domain imports in kernel crates
- [ ] App implements PolicyOracle (if applicable)
- [ ] All tests pass
- [ ] Meaning firewall verification passes

## Phase
<!-- Which phase of kernel/app separation? -->
