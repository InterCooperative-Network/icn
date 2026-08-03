# Mutation controls for the #2517 migration tests

**Status:** current · **Scope:** `icn-net` — `replay_guard`, `signing_sequence`

A test that passes on broken code is worse than no test, because it reports safety it has not
established. This file records which deliberate defect each #2517 test actually catches, so a later
change that quietly makes one of them vacuous is visible as a gap in this table rather than as a
still-green suite.

Companion to [protocol-state-migration-invariants.md](protocol-state-migration-invariants.md).

## Method

Apply one defect to `origin/main` + the #2517 change, run
`cargo test -p icn-net --lib -- migration_tests signing_sequence::tests`, record which tests fail,
revert. Every mutation must kill at least one **named** test; a mutation that kills nothing means
the property is unguarded.

The final row is the control: with every mutation reverted the suite is green, which is what makes
the failures above it meaningful rather than a broken checkout.

## Results

| # | defect introduced | tests killed |
|---|---|---|
| 1 | Legacy state treated as current — remove the version branch entirely | `legacy_high_water_does_not_reject_a_legitimate_lower_sequence_forever`, `migration_completes_at_the_envelope_validity_horizon`, `migration_runs_once_and_does_not_re_trigger_on_restart`, `crash_before_migration_completes_re_quarantines_rather_than_trusting_legacy`, `unknown_future_semantic_version_fails_closed` |
| 2 | Sender resets the watermark when stamping an unversioned store (recreates #2510) | `unversioned_durable_watermark_is_stamped_without_disturbing_the_sequence`, `corrupt_watermark_is_rejected_rather_than_silently_reset` |
| 3 | Legacy state dropped immediately, with no fail-closed hold | `captured_legacy_envelope_stays_rejected_across_the_whole_migration`, `legacy_high_water_does_not_reject_a_legitimate_lower_sequence_forever`, `migration_completes_at_the_envelope_validity_horizon`, `migration_runs_once_and_does_not_re_trigger_on_restart`, `crash_before_migration_completes_re_quarantines_rather_than_trusting_legacy`, `state_rewritten_by_an_older_binary_is_treated_as_legacy` |
| 4 | Current version never stamped on write, so migration re-runs forever | `migration_runs_once_and_does_not_re_trigger_on_restart`, `current_semantic_state_is_restored_exactly_and_not_migrated` |
| 5 | Sender accepts an unknown future regime instead of refusing to open | `unknown_future_semantic_version_refuses_to_start`, `corrupt_semantic_version_is_rejected` |
| 6 | **Receiver routes unknown-future through the bounded legacy migration** | `unknown_future_version_stays_fail_closed_past_the_legacy_migration_horizon`, `a_known_version_without_a_migration_does_not_borrow_the_legacy_path` |
| 7 | **Receiver treats an unknown future regime as a current-semantic floor** | `unknown_future_version_stays_fail_closed_past_the_legacy_migration_horizon`, `a_known_version_without_a_migration_does_not_borrow_the_legacy_path` |
| — | *(all reverted — control)* | none; 23 passed, 0 failed |

Mutation 3 is the security control: it is what proves the fail-closed hold is load-bearing rather
than decorative. Discarding a legacy high-water without holding the peer makes a captured legacy
envelope acceptable again, and `captured_legacy_envelope_stays_rejected_across_the_whole_migration`
is the test that notices.

Mutations 6 and 7 are the downgrade-safety controls, and they are the reason known-legacy and
unknown-future do not share a branch. Both express "an unknown regime becomes current" — one by
countdown, one immediately — and both must be caught. They kill exactly the two unknown-future
tests and nothing else, which is what shows those tests discriminate the unsupported path from the
bounded one rather than merely observing that something was rejected.

## Recorded gap

Mutation 5 originally carried a receiver-side half that edited a single
`entry.semantic_version != REPLAY_STATE_SEMANTIC_VERSION` branch. When that branch was replaced by
an enumerated match, the edit stopped matching any source text and the receiver half became a silent
no-op — the mutation still "passed" because its sender half killed tests. Mutation 7 exists because
of that.

The general hazard: a mutation applied by text substitution can stop applying when the code is
restructured, and reports success either way. A mutation that kills *fewer* tests than it did before
is the signal, which is why the counts above are recorded rather than just the pass/fail.
