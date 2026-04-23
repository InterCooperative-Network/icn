# NYCN Bootstrap Runbook

Current bootstrap entrypoint:

```bash
cd /home/matt/projects/icn/icn
cargo run -p icnctl -- institution bootstrap validate --package ../institutions/nycn
cargo run -p icnctl -- institution bootstrap plan --package ../institutions/nycn
cargo run -p icnctl -- institution bootstrap apply --package ../institutions/nycn --coop-id <existing-auth-coop>
```

What works now:
1. The package charter is loaded from `../charter/nycn-federation.charter.yaml`.
2. The package bootstrap manifest is loaded from `../bootstrap.yaml`.
3. Seed ordering is validated for entities, structures, activity/program, milestones, and role assignments.
4. Governance domain declarations are validated against entity `governance_domain_id` references.
5. Cross-reference checks run for parent entities, linked structures, and target programs.
6. A generic bootstrap plan is emitted for entity, governance-domain, structure, activity, program, milestone, and role-assignment operations.
7. `apply` can perform real live writes for the subset of steps the current generic gateway APIs can faithfully represent.

What `apply` can really persist today:
1. The NYCN federation entity.
2. The organizer cooperative entity.
3. NYCN governance domains declared in `02-governance-domains.seed.yaml`.
4. Committee and working-group structures under the organizer cooperative.
5. Summit activity records including `linked_structures` when target structures are live-resolvable.

What `apply` still cannot complete for NYCN:
1. Charter storage and activation through a generic package bootstrap sink.
2. Role assignment application where no `person_did` has been supplied yet.

Current practical order:
1. Validate the package with `icnctl institution bootstrap validate`.
2. Review the generic operation sequence with `icnctl institution bootstrap plan`.
3. Run `icnctl institution bootstrap apply --package ../institutions/nycn --coop-id <existing-auth-coop>`.
4. Inspect the apply report for completed, deferred, unsupported, and failed steps.
5. Supply real DIDs for role assignments when organizer identities exist.
