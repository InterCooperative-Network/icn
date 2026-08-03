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

## Sender sequence regime (§§8–11 of the invariants)

The second axis. Each defect below is applied alone to the merged branch, the suite is run, and the
files are restored. **8 of 8 killed.**

| # | defect introduced | outcome | tests killed |
|---|---|---|---|
| M1 | a missing `DURABLE_SIGNING_SEQUENCE` capability resolves to `DurableV1` | KILLED | `missing_durable_capability_resolves_to_unproven_not_durable`, `test_replay_attack_rejected`, `test_multiple_senders_independent`, `test_out_of_order_messages_forwarded` |
| M2 | an accepted sequence is stamped `DurableV1` regardless of the window's established regime | KILLED | `legacy_sender_traffic_is_never_recorded_as_durable_v1`, `a_sender_that_stays_legacy_keeps_working_and_stays_tagged_legacy`, `cleanup_of_an_inactive_peer_does_not_prove_the_legacy_namespace_never_existed`, `crash_before_the_transition_marker_resumes_from_legacy_state` |
| M3 | `LegacyOrUnproven → DurableV1` establishes directly, skipping the retirement hold | KILLED | `receiver_first_upgrade_migrates_the_sender_regime_end_to_end`, `a_captured_legacy_envelope_must_not_poison_a_fresh_durable_namespace`, `established_regime_survives_replay_state_cleanup`, `re_entering_the_transition_is_idempotent_and_does_not_reset_the_hold` |
| M4 | promotion no longer requires current authenticated `DurableV1` evidence | KILLED | `transition_does_not_promote_when_the_peer_returns_without_the_capability` |
| M5 | a downgrade resets the peer to unproven and clears its high-water | KILLED | `a_stale_legacy_connection_cannot_downgrade_established_durable_state`, `receiver_first_upgrade_migrates_the_sender_regime_end_to_end` |
| M6 | the transition is not written to the durable provenance record | KILLED | `the_transition_is_recorded_in_the_durable_provenance_record` |
| M7 | an unrecognised provenance value is read as `LegacyOrUnproven` | KILLED | `unknown_provenance_value_fails_closed_and_never_expires` |
| M8 | the Hello current-certificate check is removed, so capabilities are not bound to the connection | KILLED | `forged_hello_does_not_corrupt_established_peer_state`, `hello_replayed_onto_a_different_current_cert_is_rejected`, `weak_binding_verifier_is_confined_to_authorised_sites` |
| — | control: all mutations reverted | — | none — 391 pass, 0 fail |

### Three of these survived the first pass, and that is the useful part

Recorded because the survivals located real gaps rather than proving the mutations wrong.

**M1 survived** because every `ReplayGuard` unit test supplies the sender regime as a parameter, so
none of them exercised the one place that *derives* it from `peer_capabilities`. The single point
where a missing capability could be read as durable had no coverage at all. Closed by
`missing_durable_capability_resolves_to_unproven_not_durable` and its positive twin — the twin
matters, because without it the negative test would pass on a build that hardcoded
`LegacyOrUnproven` and never read capabilities.

**M7 survived** because the unknown-regime test wrote its unrecognised value into the *high-water
entry*, which has its own catch-all. The provenance load path was untested. Closed by
`unknown_provenance_value_fails_closed_and_never_expires`, plus
`corrupt_provenance_quarantines_rather_than_reading_as_absent` for the adjacent case — an unreadable
record must not be read as an absent one, since absent is permissive enough to establish a fresh
durable namespace after a hold.

**M6 survived twice.** The transition is recorded in two places, so removing either left a restart
still entering a 600-second hold — and a *fresh* hold is behaviourally identical to a *resumed* one.
The first replacement test asserted "restarting holds" and so passed with the provenance write
deleted entirely. The property is only pinnable structurally: assert the record exists. The
redundancy is deliberate — the high-water entry is the one `cleanup()` deletes — but redundancy is
exactly what makes a behavioural mutation control vacuous, and that is worth remembering the next
time two records carry the same fact.
