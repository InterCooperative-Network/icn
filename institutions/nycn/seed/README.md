# NYCN Bootstrap Seeds

These manifests are now consumed by the generic package loader entrypoint at `../bootstrap.yaml`.

Current reality:
- charter parsing and validation work today
- seed ordering and cross-reference validation work today
- a generic bootstrap plan can be emitted today
- a generic bootstrap apply command now exists
- governance domain provisioning from package seeds now works through the generic domain API
- activity `linked_structures` can now be persisted through the generic activity API path
- live entity and structure writes are available today through the generic gateway-backed executor
- charter registration and role assignments without DIDs are still partial or deferred

Bootstrap order:
1. `00-nycn-federation.seed.yaml`
2. `01-organizer-cooperative.seed.yaml`
3. `02-governance-domains.seed.yaml`
4. `02-structures-committees.seed.yaml`
5. `03-summit-activity-program.seed.yaml`
6. `04-milestone-template.seed.yaml`
7. `05-role-assignment.seed.yaml`
