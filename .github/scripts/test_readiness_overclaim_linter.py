#!/usr/bin/env python3
"""
Self-test for readiness_overclaim_linter.py.

Proves the readiness firewall catches affirmative readiness overclaims while
allowing the legitimate bounded contexts the repo relies on (stale/archive
banners, negations, conditionals, "do not claim" rule lines). This is the
regression guard for the linter itself: if someone weakens the patterns to make
CI pass, these assertions fail.

Run:
    python3 .github/scripts/test_readiness_overclaim_linter.py
"""

import importlib.util
import json
import os
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
FIX = os.path.join(HERE, "fixtures", "readiness")
REPO_ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))

_spec = importlib.util.spec_from_file_location(
    "readiness_overclaim_linter", os.path.join(HERE, "readiness_overclaim_linter.py")
)
linter = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(linter)


class TestBannerExempt(unittest.TestCase):
    def test_banner_detected(self):
        self.assertTrue(linter.is_banner_exempt(["# Title", "> Historical snapshot from 2025-12-12."]))

    def test_no_banner(self):
        self.assertFalse(linter.is_banner_exempt(["# Title", "**Status:** PRODUCTION READY"]))


class TestScanLines(unittest.TestCase):
    def test_flags_affirmative_production_ready(self):
        v = linter.scan_lines("x.md", ["# T", "**Status:** PRODUCTION READY"])
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].rule, "production-ready")

    def test_flags_affirmative_live_federation(self):
        v = linter.scan_lines("x.md", ["ICN runs a live federation across cooperatives."])
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].rule, "live federation")

    def test_flags_approved_for_production(self):
        v = linter.scan_lines("x.md", ["Deployment readiness: APPROVED FOR PRODUCTION."])
        self.assertEqual(len(v), 1)

    def test_banner_exempts_whole_file(self):
        lines = ["# T", "> Historical snapshot.", "**Status:** PRODUCTION READY"]
        self.assertEqual(linter.scan_lines("x.md", lines), [])

    def test_negation_not_flagged(self):
        self.assertEqual(linter.scan_lines("x.md", ["ICN is not production-ready."]), [])

    def test_conditional_once_not_flagged(self):
        self.assertEqual(linter.scan_lines("x.md", ["Once hardened, ICN becomes production-ready."]), [])

    def test_hypothetical_deployment_not_flagged(self):
        line = "In a production deployment, each member would see their position here."
        self.assertEqual(linter.scan_lines("x.md", [line]), [])

    def test_do_not_claim_not_flagged(self):
        self.assertEqual(linter.scan_lines("x.md", ["Do not claim the federation is production-ready."]), [])

    def test_future_target_not_flagged(self):
        self.assertEqual(linter.scan_lines("x.md", ["Target: production-ready by a future milestone."]), [])

    def test_allowlist_suppresses(self):
        try:
            linter.ALLOWLIST["x.md:2"] = "test exception"
            v = linter.scan_lines("x.md", ["# T", "**Status:** PRODUCTION READY"])
            self.assertEqual(v, [])
        finally:
            linter.ALLOWLIST.pop("x.md:2", None)

    # --- governance-completion coverage (firewall contract claims this class) ---
    def test_flags_governance_completion(self):
        v = linter.scan_lines("x.md", ["Proposal, vote, and member-standing governance is complete."])
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].rule, "governance-completion")

    def test_governance_completion_negated_not_flagged(self):
        self.assertEqual(linter.scan_lines("x.md", ["Governance is not complete yet."]), [])

    # --- negation must be scoped to the overclaim's own clause ---
    def test_negation_in_separate_clause_does_not_mask(self):
        v = linter.scan_lines("x.md", ["ICN is not experimental; it is PRODUCTION READY."])
        self.assertEqual(len(v), 1)

    def test_leading_conditional_comma_preserved(self):
        # Comma is not a clause delimiter: a leading conditional still exempts.
        self.assertEqual(linter.scan_lines("x.md", ["Once hardened, ICN becomes production-ready."]), [])

    # --- bare word "snapshot" must not exempt a whole file ---
    def test_snapshot_word_alone_does_not_exempt(self):
        lines = ["# Deploy", "Snapshot frequency is configurable.", "**Status:** PRODUCTION READY"]
        self.assertEqual(len(linter.scan_lines("x.md", lines)), 1)

    def test_real_historical_banner_still_exempts(self):
        lines = ["# Deploy", "> Historical deployment snapshot from 2025-12-12.", "**Status:** PRODUCTION READY"]
        self.assertEqual(linter.scan_lines("x.md", lines), [])


class TestNonclaimContext(unittest.TestCase):
    """Precision: explicit nonclaim / red-line / question contexts are not claims,
    but direct affirmative assertions still warn."""

    # --- true positives still warn ---
    def test_direct_production_ready_still_warns(self):
        v = linter.scan_lines("x.md", ["ICN is production-ready for cooperative deployments."])
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].rule, "production-ready")

    def test_direct_live_federation_deployed_still_warns(self):
        v = linter.scan_lines("x.md", ["ICN live federation is deployed across cooperatives today."])
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].rule, "live federation")

    # --- false positives suppressed ---
    def test_must_not_claim_enumeration_suppressed(self):
        # The clause-local guard splits on ';'/':'; the line-level nonclaim rule
        # must still recognise the whole bullet as a non-claim.
        line = "Must not claim: that any of this is production-ready or live federation."
        self.assertEqual(linter.scan_lines("x.md", [line]), [])

    def test_disclaimer_does_not_mask_separate_overclaim(self):
        # A readiness disclaimer in one clause must NOT line-skip a separate
        # affirmative overclaim in another clause: only the disclaimer's own clause
        # is exempt (via clause-local NEGATION_RE), the other clause still warns.
        v = linter.scan_lines("x.md", ["This is not production-ready; ICN is generally available."])
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].rule, "general availability")

    def test_line_under_nonclaims_heading_suppressed(self):
        lines = ["## Nonclaims", "- ICN is production-ready and runs a live federation."]
        self.assertEqual(linter.scan_lines("x.md", lines), [])

    def test_question_line_suppressed(self):
        self.assertEqual(linter.scan_lines("x.md", ["### Is this ready for production?"]), [])
        self.assertEqual(linter.scan_lines("x.md", ['**"Is this ready for production?"**']), [])

    def test_not_a_formal_pilot_suppressed(self):
        self.assertEqual(linter.scan_lines("x.md", ["This is not a formal pilot."]), [])

    def test_nothing_disclaimer_suppressed(self):
        self.assertEqual(linter.scan_lines("x.md", ["Nothing about ICN is production-ready."]), [])

    # --- nonclaim section context resets at the next heading ---
    def test_nonclaim_section_resets_at_next_heading(self):
        lines = [
            "## Nonclaims",
            "- ICN is production-ready.",   # exempt (under Nonclaims)
            "## Status",
            "ICN is production-ready.",      # NOT exempt (new section)
        ]
        v = linter.scan_lines("x.md", lines)
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].line, 4)

    # --- red-line section heading variants are recognised ---
    def test_red_lines_section_suppressed(self):
        lines = ["## Red lines", "- No live federation is deployed; production-ready is never claimed."]
        self.assertEqual(linter.scan_lines("x.md", lines), [])

    def test_forbidden_collapses_section_suppressed(self):
        lines = ["## Forbidden collapses", "| federation command | live federation | production-ready |"]
        self.assertEqual(linter.scan_lines("x.md", lines), [])


class TestProjectIndexFP(unittest.TestCase):
    """Precision for the docs/reference/project-index FP classes (so that root can be
    scanned cleanly), while direct affirmative claims still warn."""

    # --- true positives still warn ---
    def test_tp_production_ready(self):
        v = linter.scan_lines("x.md", ["ICN is production-ready."])
        self.assertEqual(len(v), 1)

    def test_tp_has_live_federation(self):
        v = linter.scan_lines("x.md", ["ICN has live federation between cooperatives."])
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].rule, "live federation")

    def test_tp_generally_available(self):
        v = linter.scan_lines("x.md", ["ICN is generally available."])
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].rule, "general availability")

    # --- false positives suppressed ---
    def test_caveat_prefix_unsafe_suppressed(self):
        line = "- Unsafe without more evidence: ICN is production-ready across arbitrary networks."
        self.assertEqual(linter.scan_lines("x.md", [line]), [])

    def test_quoted_avoid_list_suppressed(self):
        # A quoted red-line phrase is exempt only INSIDE an explicit avoid-list
        # block — the "**Avoid** ...:" lead-in (separated by a blank line, as in
        # the real proof-level-taxonomy doc) puts the bullet into avoid context.
        lines = [
            "**Avoid** (these claim past the evidence):",
            "",
            '- "production-ready", "fully federated", "live pilot", "secure by default"',
        ]
        self.assertEqual(linter.scan_lines("x.md", lines), [])

    def test_single_scare_quoted_claim_still_warns(self):
        # A lone scare-quoted phrase in an assertion (no avoid-list context) is
        # still a claim — quoting alone never exempts.
        for line in ['ICN is "production-ready".', "The status is `production-ready`."]:
            v = linter.scan_lines("x.md", [line])
            self.assertEqual(len(v), 1, msg=line)
            self.assertEqual(v[0].rule, "production-ready", msg=line)

    def test_quoted_pair_without_avoid_framing_still_warns(self):
        # Regression for Codex review (#2230): two quoted phrases on a line is NOT
        # an avoid-list. Without explicit avoid framing, a quoted affirmative
        # assertion must warn — the old quote-count heuristic wrongly suppressed it.
        line = 'Status: "live federation" and "production-ready" are now true.'
        v = linter.scan_lines("x.md", [line])
        self.assertEqual(len(v), 1)

    def test_avoid_block_ends_at_heading_then_quoted_claim_warns(self):
        # The avoid-list block is bounded: a heading closes it, so a quoted claim
        # after the block still warns (the exemption cannot leak past its list).
        lines = [
            "**Avoid** (these claim past the evidence):",
            '- "production-ready", "live pilot"',
            "",
            "## Status",
            'ICN is "production-ready" now.',
        ]
        v = linter.scan_lines("x.md", lines)
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].line, 5)

    def test_quoted_overclaim_without_leadin_warns(self):
        # A bullet of quoted red-line phrases with NO preceding avoid lead-in is
        # not in avoid context, so the quoted overclaim still warns.
        line = '- "production-ready" and shipping today'
        v = linter.scan_lines("x.md", [line])
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].rule, "production-ready")

    def test_risk_cell_overclaim_label_suppressed(self):
        line = "| Entity / federation / trust | implemented but partial | high: live federation overclaim |"
        self.assertEqual(linter.scan_lines("x.md", [line]), [])

    def test_checklist_nonclaims_item_suppressed(self):
        # Both the plain and markdown-bold ("**nonclaims** for") checklist phrasings
        # are exempt — the bold variant is the real project-index line.
        for line in [
            "- [ ] Includes nonclaims for production readiness, formal pilot readiness, live federation.",
            "- [ ] Includes **nonclaims** for production readiness, live federation, Phase 2 completion.",
        ]:
            self.assertEqual(linter.scan_lines("x.md", [line]), [], msg=line)

    def test_bare_nonclaims_mention_does_not_exempt_separate_claim(self):
        # Regression for Codex review (#2230): the nonclaims exemption is scoped to
        # the "nonclaims for ..." checklist phrasing. A bare mention must NOT suppress
        # a separate overclaim in the same colon/comma-joined segment (_framing_segment
        # does not split on ":" or ",").
        cases = [
            ("Nonclaims no longer apply: ICN is production-ready.", "production-ready"),
            ("This page lists nonclaims, but ICN is generally available.", "general availability"),
        ]
        for line, rule in cases:
            v = linter.scan_lines("x.md", [line])
            self.assertEqual(len(v), 1, msg=line)
            self.assertEqual(v[0].rule, rule, msg=line)

    def test_claim_requires_meta_statement_suppressed(self):
        line = "A live federation claim requires a governed inter-institutional relationship and evidence."
        self.assertEqual(linter.scan_lines("x.md", [line]), [])

    def test_claim_requires_does_not_mask_separate_overclaim(self):
        # The "claim requires" exemption is per-match (scoped to the described
        # phrase), so a separate affirmative overclaim in another clause still warns.
        v = linter.scan_lines("x.md", ["A live federation claim requires evidence; ICN is production-ready."])
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].rule, "production-ready")

    def test_without_requiring_live_federation_suppressed(self):
        line = "The operator ladder walks drive content into proofs without requiring a live federation step."
        self.assertEqual(linter.scan_lines("x.md", [line]), [])

    # --- a blanket "without" must NOT suppress a real claim ---
    def test_without_blanket_does_not_mask_real_claim(self):
        v = linter.scan_lines("x.md", ["ICN is production-ready without caveats."])
        self.assertEqual(len(v), 1)

    # --- nonclaim FRAMING is segment-scoped: it must NOT mask a separate
    #     affirmative overclaim in a LATER clause of the same line ---
    def test_nonclaim_framing_does_not_mask_later_overclaim(self):
        cases = [
            # caveat prefix exempts clause 1, but clause 2 is a separate claim
            ("Unsafe without more evidence: ICN is production-ready; ICN is generally available.",
             "general availability"),
            # "nonclaims for ..." exempts clause 1, clause 2 still warns
            ("Includes nonclaims for live federation; ICN is production-ready.",
             "production-ready"),
            # narrow "without ... live federation" exempts clause 1, clause 2 warns
            ("Walks proofs without requiring a live federation step; ICN is generally available.",
             "general availability"),
            # period (not just ';') also splits segments: framed example then a claim
            ("Unsafe without more evidence: live federation. ICN is generally available.",
             "general availability"),
        ]
        for line, expected_rule in cases:
            v = linter.scan_lines("x.md", [line])
            self.assertEqual(len(v), 1, msg=line)
            self.assertEqual(v[0].rule, expected_rule, msg=line)

    def test_must_not_claim_prefix_still_exempts_its_list(self):
        # The ":"-inclusive segment keeps the framing attached to the list it
        # introduces, so "Must not claim: ... live federation." stays exempt.
        line = "Must not claim: that any of this is production-ready or live federation."
        self.assertEqual(linter.scan_lines("x.md", [line]), [])

    # --- a suppressed first occurrence must NOT hide a later real assertion of
    #     the SAME pattern (scan_lines iterates every match per pattern) ---
    def test_suppressed_match_does_not_hide_later_same_pattern(self):
        cases = [
            ('"production-ready" is an avoid-list example; ICN is production-ready.', "production-ready"),
            ("high: live federation overclaim; ICN has live federation deployed.", "live federation"),
        ]
        for line, expected_rule in cases:
            v = linter.scan_lines("x.md", [line])
            self.assertEqual(len(v), 1, msg=line)
            self.assertEqual(v[0].rule, expected_rule, msg=line)


class TestFixturesEndToEnd(unittest.TestCase):
    def test_bad_fixture_flagged(self):
        v = linter.scan_file("bad_unbannered.md", os.path.join(FIX, "bad_unbannered.md"))
        self.assertGreaterEqual(len(v), 1)

    def test_good_bannered_clean(self):
        self.assertEqual(linter.scan_file("good_bannered.md", os.path.join(FIX, "good_bannered.md")), [])

    def test_good_negated_clean(self):
        self.assertEqual(linter.scan_file("good_negated.md", os.path.join(FIX, "good_negated.md")), [])


class TestScanConfig(unittest.TestCase):
    """--config loading: no-config default, override semantics, malformed input."""

    def test_no_config_returns_hardcoded_defaults(self):
        scan_dirs, exclude_dirs, _manifest = linter.load_scan_config(None)
        self.assertEqual(scan_dirs, linter.SCAN_DIRS)
        self.assertEqual(exclude_dirs, linter.EXCLUDE_DIRS)

    def _write_config(self, data):
        f = tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False, dir=self._tmp)
        json.dump(data, f)
        f.close()
        return f.name

    def setUp(self):
        self._tmpdir = tempfile.TemporaryDirectory()
        self._tmp = self._tmpdir.name

    def tearDown(self):
        self._tmpdir.cleanup()

    def test_config_replaces_scan_dirs_wholesale(self):
        path = self._write_config({"scan_dirs": ["docs/deployment", "website"]})
        scan_dirs, exclude_dirs, _manifest = linter.load_scan_config(path)
        self.assertEqual(scan_dirs, ["docs/deployment", "website"])
        # exclude_dirs key omitted -> stays at the hardcoded default.
        self.assertEqual(exclude_dirs, linter.EXCLUDE_DIRS)

    def test_config_replaces_exclude_dirs_wholesale(self):
        path = self._write_config({"exclude_dirs": ["only-this"]})
        scan_dirs, exclude_dirs, _manifest = linter.load_scan_config(path)
        self.assertEqual(scan_dirs, linter.SCAN_DIRS)
        self.assertEqual(exclude_dirs, {"only-this"})

    def test_missing_config_file_raises(self):
        with self.assertRaises(ValueError):
            linter.load_scan_config(os.path.join(self._tmp, "does-not-exist.json"))

    def test_malformed_json_raises(self):
        path = os.path.join(self._tmp, "bad.json")
        with open(path, "w") as f:
            f.write("{not valid json")
        with self.assertRaises(ValueError):
            linter.load_scan_config(path)

    def test_scan_dirs_wrong_type_raises(self):
        path = self._write_config({"scan_dirs": "not-a-list"})
        with self.assertRaises(ValueError):
            linter.load_scan_config(path)

    def test_scan_dirs_non_string_element_raises(self):
        path = self._write_config({"scan_dirs": ["ok", 42]})
        with self.assertRaises(ValueError):
            linter.load_scan_config(path)

    def test_config_not_an_object_raises(self):
        path = self._write_config(["not", "an", "object"])
        with self.assertRaises(ValueError):
            linter.load_scan_config(path)


class TestRunLintScope(unittest.TestCase):
    """run_lint honours configured scan_dirs/exclude_dirs (the --config include/path behavior)."""

    def setUp(self):
        self._tmpdir = tempfile.TemporaryDirectory()
        self._tmp = self._tmpdir.name

    def tearDown(self):
        self._tmpdir.cleanup()

    def test_run_lint_only_scans_configured_dirs(self):
        os.makedirs(os.path.join(self._tmp, "included"))
        os.makedirs(os.path.join(self._tmp, "not_configured"))
        with open(os.path.join(self._tmp, "included", "a.md"), "w") as f:
            f.write("ICN is production-ready.\n")
        with open(os.path.join(self._tmp, "not_configured", "b.md"), "w") as f:
            f.write("ICN is production-ready.\n")

        result = linter.run_lint(self._tmp, scan_dirs=["included"], exclude_dirs=set())
        self.assertEqual(result.files_scanned, 1)
        self.assertEqual(len(result.violations), 1)
        self.assertEqual(result.violations[0].file, "included/a.md")

    def test_run_lint_scans_astro_files_under_configured_dir(self):
        # The website/ use case: pure .astro pages, no .md files at all.
        os.makedirs(os.path.join(self._tmp, "website"))
        with open(os.path.join(self._tmp, "website", "index.astro"), "w") as f:
            f.write("ICN is production-ready.\n")

        result = linter.run_lint(self._tmp, scan_dirs=["website"], exclude_dirs=set())
        self.assertEqual(result.files_scanned, 1)
        self.assertEqual(len(result.violations), 1)
        self.assertEqual(result.violations[0].rule, "production-ready")

    def test_default_run_lint_matches_hardcoded_defaults(self):
        # No scan_dirs/exclude_dirs passed -> identical to the hardcoded globals
        # (the "no-config/default behavior" contract the CLI relies on).
        explicit = linter.run_lint(REPO_ROOT, scan_dirs=linter.SCAN_DIRS,
                                    exclude_dirs=linter.EXCLUDE_DIRS)
        implicit = linter.run_lint(REPO_ROOT)
        self.assertEqual(implicit.files_scanned, explicit.files_scanned)
        self.assertEqual(len(implicit.violations), len(explicit.violations))

    def test_unreadable_file_warns_and_does_not_abort_the_run(self):
        # Regression for Copilot review (#2358): run_lint() must tolerate a
        # per-file OSError the same way scan_file() already does, not let one
        # bad file turn the whole run into a script error.
        os.makedirs(os.path.join(self._tmp, "included"))
        unreadable = os.path.join(self._tmp, "included", "locked.md")
        with open(unreadable, "w") as f:
            f.write("ICN is production-ready.\n")
        os.chmod(unreadable, 0o000)
        readable = os.path.join(self._tmp, "included", "ok.md")
        with open(readable, "w") as f:
            f.write("ICN is production-ready.\n")
        try:
            if os.access(unreadable, os.R_OK):
                self.skipTest("running as a user that ignores file permissions (e.g. root)")
            result = linter.run_lint(self._tmp, scan_dirs=["included"], exclude_dirs=set())
            self.assertEqual(len(result.violations), 1)
            self.assertEqual(result.violations[0].file, "included/ok.md")
        finally:
            os.chmod(unreadable, 0o644)

    def test_banner_exempt_file_with_historical_violations_not_counted_exempt(self):
        # Regression for Copilot review (#2358): a banner-exempt file that
        # ALSO has unmarked-historical-liveness violations must not be
        # reported as "exempt" — that would misrepresent a file with findings
        # as clean in the summary count.
        os.makedirs(os.path.join(self._tmp, "docs_status"))
        path = os.path.join(self._tmp, "docs_status", "STATUS_2025-12-12.md")
        with open(path, "w") as f:
            f.write("# T\n> Historical snapshot.\n**Status**: Running (Healthy)\n")

        result = linter.run_lint(self._tmp, scan_dirs=["docs_status"], exclude_dirs=set())
        self.assertEqual(len(result.violations), 1)
        self.assertEqual(result.violations[0].rule, "unmarked-historical-liveness")
        self.assertNotIn("docs_status/STATUS_2025-12-12.md", result.files_exempt)

    def test_banner_exempt_file_without_historical_violations_still_counted_exempt(self):
        # Regression guard the OTHER direction: an ordinary banner-exempt file
        # (no historical-liveness findings) must still land in files_exempt —
        # the fix above must not turn every banner-exempt file into "not
        # exempt".
        os.makedirs(os.path.join(self._tmp, "included"))
        path = os.path.join(self._tmp, "included", "a.md")
        with open(path, "w") as f:
            f.write("# T\n> Historical snapshot.\n**Status:** PRODUCTION READY\n")

        result = linter.run_lint(self._tmp, scan_dirs=["included"], exclude_dirs=set())
        self.assertEqual(result.violations, [])
        self.assertIn("included/a.md", result.files_exempt)


class TestHistoricalProofArtifact(unittest.TestCase):
    """The historical-proof-artifact category: marker exempts, missing marker
    flags, ref/date are required for marker validity."""

    HISTORICAL_NAME = "DEPLOYMENT_STATUS_2025-12-12.md"
    NORMAL_NAME = "CURRENT_STATUS.md"
    VALID_MARKER = (
        "<!-- claim-class: historical-proof ref=91a63eec date=2026-04-29 "
        "evidence=https://example.invalid/issues/1 -->"
    )

    def test_is_historical_doc_matches_dated_status_filename(self):
        self.assertTrue(linter.is_historical_doc(self.HISTORICAL_NAME))
        self.assertTrue(linter.is_historical_doc("docs/deployment/" + self.HISTORICAL_NAME))

    def test_is_historical_doc_false_for_normal_filename(self):
        self.assertFalse(linter.is_historical_doc(self.NORMAL_NAME))

    def test_missing_marker_flags_liveness(self):
        lines = ["# Deploy", "**Status**: Running (Healthy)"]
        v = linter.scan_historical_liveness(self.HISTORICAL_NAME, lines)
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].rule, "unmarked-historical-liveness")

    def test_valid_marker_exempts_liveness(self):
        lines = ["# Deploy", self.VALID_MARKER, "**Status**: Running (Healthy)"]
        v = linter.scan_historical_liveness(self.HISTORICAL_NAME, lines)
        self.assertEqual(v, [])

    def test_marker_missing_ref_does_not_exempt(self):
        lines = [
            "# Deploy",
            "<!-- claim-class: historical-proof date=2026-04-29 -->",
            "**Status**: Running (Healthy)",
        ]
        v = linter.scan_historical_liveness(self.HISTORICAL_NAME, lines)
        self.assertEqual(len(v), 1)

    def test_marker_missing_date_does_not_exempt(self):
        lines = [
            "# Deploy",
            "<!-- claim-class: historical-proof ref=91a63eec -->",
            "**Status**: Running (Healthy)",
        ]
        v = linter.scan_historical_liveness(self.HISTORICAL_NAME, lines)
        self.assertEqual(len(v), 1)

    def test_marker_with_unparseable_date_does_not_exempt(self):
        lines = [
            "# Deploy",
            "<!-- claim-class: historical-proof ref=91a63eec date=not-a-date -->",
            "**Status**: Running (Healthy)",
        ]
        v = linter.scan_historical_liveness(self.HISTORICAL_NAME, lines)
        self.assertEqual(len(v), 1)

    def test_non_historical_doc_not_scanned_regardless_of_liveness_language(self):
        lines = ["**Status**: Running (Healthy)"]
        v = linter.scan_historical_liveness(self.NORMAL_NAME, lines)
        self.assertEqual(v, [])

    def test_negated_liveness_not_flagged(self):
        lines = ["The daemon is not running."]
        v = linter.scan_historical_liveness(self.HISTORICAL_NAME, lines)
        self.assertEqual(v, [])

    def test_parse_historical_marker_returns_attrs(self):
        attrs = linter.parse_historical_marker(["x", self.VALID_MARKER])
        self.assertEqual(attrs["ref"], "91a63eec")
        self.assertEqual(attrs["date"], "2026-04-29")

    def test_parse_historical_marker_none_when_absent(self):
        self.assertIsNone(linter.parse_historical_marker(["# Deploy", "no marker here"]))

    def test_real_deployment_status_doc_is_marked_and_clean(self):
        # Regression for the actual file this category was built for: after
        # applying the marker (PR3), the real doc must scan clean.
        path = os.path.join(REPO_ROOT, "docs", "deployment", self.HISTORICAL_NAME)
        with open(path, "r", encoding="utf-8") as f:
            lines = f.read().splitlines()
        attrs = linter.parse_historical_marker(lines)
        self.assertIsNotNone(
            attrs, "expected a valid claim-class: historical-proof marker in " + path
        )
        self.assertEqual(linter.scan_historical_liveness(self.HISTORICAL_NAME, lines), [])
        # Regression for Copilot review (#2358): evidence must be an
        # externally-citable pointer (a permalink/issue), not a self-reference
        # to the very file the marker is embedded in.
        self.assertNotEqual(attrs.get("evidence"), "docs/deployment/" + self.HISTORICAL_NAME)
        self.assertTrue(attrs.get("evidence", "").startswith("http"))


# ── Rendered-text normalisation: HTML comments are not public claims ─────────


class TestHtmlCommentStripping(unittest.TestCase):
    def test_offsets_and_line_count_preserved(self):
        """Blanking must keep every column offset, or the column-based guards
        (_clause_around, _framing_segment, _phrase_is_quoted) silently misfire."""
        lines = ["abc <!-- hidden --> def", "plain"]
        out = linter.strip_html_comments(lines)
        self.assertEqual(len(out), len(lines))
        for a, b in zip(lines, out):
            self.assertEqual(len(a), len(b))
        self.assertTrue(out[0].startswith("abc "))
        self.assertTrue(out[0].endswith(" def"))
        self.assertNotIn("hidden", out[0])

    def test_claim_inside_comment_is_not_flagged(self):
        self.assertEqual(linter.scan_lines("docs/x.md", ["<!-- ICN is production-ready -->"]), [])

    def test_claim_inside_multiline_comment_is_not_flagged(self):
        lines = ["<!--", "**Status:** PRODUCTION READY", "-->"]
        self.assertEqual(linter.scan_lines("docs/x.md", lines), [])

    def test_unterminated_comment_swallows_rest_of_file(self):
        lines = ["<!-- oops", "ICN is production-ready."]
        self.assertEqual(linter.scan_lines("docs/x.md", lines), [])

    def test_claim_after_a_closed_comment_on_same_line_still_flags(self):
        v = linter.scan_lines("docs/x.md", ["<!-- note --> ICN is production-ready."])
        self.assertEqual(len(v), 1)

    def test_comments_are_not_stripped_inside_a_fence(self):
        """strip_html_comments leaves fenced content alone (there a comment is
        displayed literally). Whether it is SCANNED is decided separately, by the
        fenced-code rule — see TestFencedCode."""
        lines = ["```", "<!-- ICN is production-ready -->", "```"]
        self.assertEqual(linter.strip_html_comments(lines), lines)

    def test_comments_are_not_stripped_inside_a_tilde_fence(self):
        lines = ["~~~", "<!-- ICN is production-ready -->", "~~~"]
        self.assertEqual(linter.strip_html_comments(lines), lines)

    def test_real_state_md_sync_note_is_not_a_public_claim(self):
        """docs/STATE.md's comment-embedded sync notes were 23 of the 95 baseline
        findings when the linter read raw bytes."""
        path = os.path.join(REPO_ROOT, "docs", "STATE.md")
        if not os.path.isfile(path):
            self.skipTest("docs/STATE.md not present")
        with open(path, encoding="utf-8", errors="replace") as f:
            lines = f.read().splitlines()
        self.assertEqual(linter.scan_lines("docs/STATE.md", lines), [])


# ── Inline negating lead-in ──────────────────────────────────────────────────


class TestInlineNegationScope(unittest.TestCase):
    def test_inline_enumeration_after_does_not_claim_is_exempt(self):
        line = "This sync explicitly does NOT claim: a lifecycle; live federation; Phase 2 completion."
        self.assertEqual(linter.scan_lines("docs/x.md", [line]), [])

    def test_bold_leadin_is_exempt(self):
        line = "**Does not claim:** a lifecycle; live federation."
        self.assertEqual(linter.scan_lines("docs/x.md", [line]), [])

    def test_scope_ends_at_the_sentence_boundary(self):
        """A later sentence is a separate assertion and must still flag."""
        line = "This does not claim: a; b. ICN is production-ready."
        self.assertEqual(len(linter.scan_lines("docs/x.md", [line])), 1)

    def test_revoking_leadin_does_not_exempt(self):
        """Words between the framing and the ":" can invert the meaning."""
        line = "Nonclaims no longer apply: ICN is production-ready."
        self.assertEqual(len(linter.scan_lines("docs/x.md", [line])), 1)

    def test_leadin_does_not_reach_the_next_line(self):
        lines = ["We do not claim:", "ICN is production-ready."]
        self.assertEqual(len(linter.scan_lines("docs/x.md", lines)), 1)

    def test_bare_claim_unaffected(self):
        self.assertEqual(len(linter.scan_lines("docs/x.md", ["ICN is production-ready."])), 1)


# ── Manifest loading: fails closed ──────────────────────────────────────────


class TestLoadManifest(unittest.TestCase):
    def setUp(self):
        self._tmpdir = tempfile.TemporaryDirectory()
        self._tmp = self._tmpdir.name
        os.makedirs(os.path.join(self._tmp, "docs"))
        for name in ("a.md", "b.md"):
            with open(os.path.join(self._tmp, "docs", name), "w") as f:
                f.write("# ok\n")

    def tearDown(self):
        self._tmpdir.cleanup()

    def _manifest(self, payload, name="m.json"):
        path = os.path.join(self._tmp, name)
        with open(path, "w") as f:
            json.dump(payload, f)
        return name

    def test_valid_manifest_returns_sorted_pairs(self):
        rel = self._manifest({"files": [
            {"path": "docs/b.md", "bannered": True},
            {"path": "docs/a.md", "bannered": False},
        ]})
        self.assertEqual(
            linter.load_manifest(self._tmp, rel),
            [("docs/a.md", False), ("docs/b.md", True)],
        )

    def test_bannered_defaults_to_false(self):
        rel = self._manifest({"files": [{"path": "docs/a.md"}]})
        self.assertEqual(linter.load_manifest(self._tmp, rel), [("docs/a.md", False)])

    def test_absent_manifest_fails_closed(self):
        """The manifest is a build artifact: absent means generation did not run,
        and a gate that then scanned nothing would report a false success."""
        with self.assertRaises(ValueError):
            linter.load_manifest(self._tmp, "nope.json")

    def test_malformed_json_fails_closed(self):
        path = os.path.join(self._tmp, "bad.json")
        with open(path, "w") as f:
            f.write("{not json")
        with self.assertRaises(ValueError):
            linter.load_manifest(self._tmp, "bad.json")

    def test_missing_files_key_fails_closed(self):
        rel = self._manifest({"count": 0})
        with self.assertRaises(ValueError):
            linter.load_manifest(self._tmp, rel)

    def test_empty_files_list_fails_closed(self):
        rel = self._manifest({"files": []})
        with self.assertRaises(ValueError):
            linter.load_manifest(self._tmp, rel)

    def test_nonexistent_path_fails_closed(self):
        rel = self._manifest({"files": [{"path": "docs/gone.md"}]})
        with self.assertRaises(ValueError):
            linter.load_manifest(self._tmp, rel)

    def test_absolute_path_rejected(self):
        rel = self._manifest({"files": [{"path": "/etc/passwd"}]})
        with self.assertRaises(ValueError):
            linter.load_manifest(self._tmp, rel)

    def test_parent_escape_rejected(self):
        rel = self._manifest({"files": [{"path": "../outside.md"}]})
        with self.assertRaises(ValueError):
            linter.load_manifest(self._tmp, rel)

    def test_unnormalised_path_rejected(self):
        rel = self._manifest({"files": [{"path": "docs/./a.md"}]})
        with self.assertRaises(ValueError):
            linter.load_manifest(self._tmp, rel)

    def test_non_bool_bannered_rejected(self):
        rel = self._manifest({"files": [{"path": "docs/a.md", "bannered": "yes"}]})
        with self.assertRaises(ValueError):
            linter.load_manifest(self._tmp, rel)

    def test_duplicate_path_is_deduplicated(self):
        rel = self._manifest({"files": [
            {"path": "docs/a.md", "bannered": True},
            {"path": "docs/a.md", "bannered": True},
        ]})
        self.assertEqual(linter.load_manifest(self._tmp, rel), [("docs/a.md", True)])

    def test_conflicting_duplicate_fails_closed(self):
        rel = self._manifest({"files": [
            {"path": "docs/a.md", "bannered": True},
            {"path": "docs/a.md", "bannered": False},
        ]})
        with self.assertRaises(ValueError):
            linter.load_manifest(self._tmp, rel)

    def test_config_exposes_scan_manifest(self):
        path = os.path.join(self._tmp, "cfg.json")
        with open(path, "w") as f:
            json.dump({"scan_dirs": ["docs"], "scan_manifest": "m.json"}, f)
        _dirs, _excl, manifest = linter.load_scan_config(path)
        self.assertEqual(manifest, "m.json")

    def test_config_without_scan_manifest_is_none(self):
        path = os.path.join(self._tmp, "cfg2.json")
        with open(path, "w") as f:
            json.dump({"scan_dirs": ["docs"]}, f)
        _dirs, _excl, manifest = linter.load_scan_config(path)
        self.assertIsNone(manifest)

    def test_non_string_scan_manifest_rejected(self):
        path = os.path.join(self._tmp, "cfg3.json")
        with open(path, "w") as f:
            json.dump({"scan_manifest": ["a"]}, f)
        with self.assertRaises(ValueError):
            linter.load_scan_config(path)

    def test_default_config_reports_no_manifest(self):
        self.assertIsNone(linter.load_scan_config(None)[2])


# ── Manifest scope + archive banner equivalence in run_lint ─────────────────


class TestRunLintManifest(unittest.TestCase):
    OVERCLAIM = "**Status:** PRODUCTION READY"
    # Filename is dated/status-named so is_historical_doc() recognises it.
    HIST = "PROJECT_STATUS_2025-12-06.md"

    def setUp(self):
        self._tmpdir = tempfile.TemporaryDirectory()
        self._tmp = self._tmpdir.name
        os.makedirs(os.path.join(self._tmp, "docs"))
        os.makedirs(os.path.join(self._tmp, "website"))

    def tearDown(self):
        self._tmpdir.cleanup()

    def _write(self, rel, text):
        path = os.path.join(self._tmp, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(text)

    def test_manifest_file_outside_scan_dirs_is_scanned(self):
        """The whole point: docs/ is not a scan_dir, but a published doc is linted."""
        self._write("docs/pub.md", self.OVERCLAIM + "\n")
        result = linter.run_lint(
            self._tmp, scan_dirs=["website"], exclude_dirs=set(),
            manifest_entries=[("docs/pub.md", False)],
        )
        self.assertEqual([v.file for v in result.violations], ["docs/pub.md"])

    def test_unpublished_sibling_is_not_scanned(self):
        self._write("docs/pub.md", "# clean\n")
        self._write("docs/withheld.md", self.OVERCLAIM + "\n")
        result = linter.run_lint(
            self._tmp, scan_dirs=[], exclude_dirs=set(),
            manifest_entries=[("docs/pub.md", False)],
        )
        self.assertEqual(result.violations, [])
        self.assertEqual(result.files_scanned, 1)

    def test_bannered_true_exempts_affirmative_overclaim(self):
        """Equivalent to a source banner: the published rendering carries one."""
        self._write("docs/arch.md", self.OVERCLAIM + "\n")
        result = linter.run_lint(
            self._tmp, scan_dirs=[], exclude_dirs=set(),
            manifest_entries=[("docs/arch.md", True)],
        )
        self.assertEqual(result.violations, [])
        self.assertEqual(result.files_exempt, ["docs/arch.md"])

    def test_bannered_false_does_not_exempt(self):
        self._write("docs/arch.md", self.OVERCLAIM + "\n")
        result = linter.run_lint(
            self._tmp, scan_dirs=[], exclude_dirs=set(),
            manifest_entries=[("docs/arch.md", False)],
        )
        self.assertEqual(len(result.violations), 1)

    def test_bannered_does_NOT_suppress_historical_liveness(self):
        """The category exists so a banner cannot launder "exercised once" into
        "still live". A manifest banner must not buy what a source banner cannot."""
        self._write("docs/" + self.HIST, "# Status\n- 3 nodes operational\n")
        result = linter.run_lint(
            self._tmp, scan_dirs=[], exclude_dirs=set(),
            manifest_entries=[("docs/" + self.HIST, True)],
        )
        self.assertEqual([v.rule for v in result.violations], ["unmarked-historical-liveness"])
        # A file with findings is not "exempt", even though it is bannered.
        self.assertEqual(result.files_exempt, [])

    def test_valid_claim_class_marker_still_exempts_bannered_file(self):
        self._write(
            "docs/" + self.HIST,
            "# Status\n<!-- claim-class: historical-proof ref=abc123 date=2025-12-06 "
            "evidence=https://example.invalid/1 -->\n- 3 nodes operational\n",
        )
        result = linter.run_lint(
            self._tmp, scan_dirs=[], exclude_dirs=set(),
            manifest_entries=[("docs/" + self.HIST, True)],
        )
        self.assertEqual(result.violations, [])

    def test_path_in_both_scan_dir_and_manifest_is_scanned_once(self):
        self._write("website/page.md", self.OVERCLAIM + "\n")
        result = linter.run_lint(
            self._tmp, scan_dirs=["website"], exclude_dirs=set(),
            manifest_entries=[("website/page.md", False)],
        )
        self.assertEqual(result.files_scanned, 1)
        self.assertEqual(len(result.violations), 1)

    def test_manifest_bannered_wins_over_plain_walk(self):
        """A walked file carries no banner metadata; the manifest's does apply."""
        self._write("website/page.md", self.OVERCLAIM + "\n")
        result = linter.run_lint(
            self._tmp, scan_dirs=["website"], exclude_dirs=set(),
            manifest_entries=[("website/page.md", True)],
        )
        self.assertEqual(result.violations, [])
        self.assertEqual(result.files_scanned, 1)

    def test_no_manifest_keeps_directory_only_behaviour(self):
        self._write("website/page.md", self.OVERCLAIM + "\n")
        result = linter.run_lint(self._tmp, scan_dirs=["website"], exclude_dirs=set())
        self.assertEqual(len(result.violations), 1)


class TestWebsiteConfigWiring(unittest.TestCase):
    CONFIG = os.path.join(REPO_ROOT, ".github", "claim-lint-website.json")

    def test_website_config_declares_the_published_docs_manifest(self):
        """Guards the wiring itself: if the config loses scan_manifest, every
        republished docs page silently leaves the claim gate again."""
        if not os.path.isfile(self.CONFIG):
            self.skipTest("website claim-lint config not present")
        dirs, _excl, manifest = linter.load_scan_config(self.CONFIG)
        self.assertIn("website", dirs)
        self.assertEqual(manifest, "website/src/data/published-docs.generated.json")


# ── Family A: nonclaim heading vocabulary ────────────────────────────────────


class TestScopeBoundaryHeadings(unittest.TestCase):
    """A section naming itself as work NOT done here enumerates red lines."""

    def _scan(self, heading, body):
        return linter.scan_lines("docs/x.md", [heading, "", body])

    def test_deferred_work_heading(self):
        self.assertEqual(
            self._scan(
                "## 13. Deferred work (explicitly out of scope of this contract)",
                "- Production / pilot / NYCN activation / live federation / Phase-2 work.",
            ),
            [],
        )

    def test_out_of_scope_heading(self):
        self.assertEqual(self._scan("## Out of scope", "- live federation"), [])

    def test_not_in_scope_heading(self):
        self.assertEqual(self._scan("## Not in scope", "- live federation"), [])

    def test_claims_this_doctrine_does_not_make_heading(self):
        self.assertEqual(
            self._scan("## Claims this doctrine does not make", "live federation · pilot"),
            [],
        )

    def test_not_yet_done_heading(self):
        self.assertEqual(
            self._scan("## 6. What stays explicitly not-yet-done", "- Live federation;"),
            [],
        )

    # ── adversarial ──

    def test_section_exemption_ends_at_the_next_heading(self):
        """The laundering route to close: an out-of-scope section must not exempt
        the rest of the document."""
        v = linter.scan_lines("docs/x.md", [
            "## Out of scope",
            "- live federation",
            "## Status",
            "ICN is production-ready.",
        ])
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].line, 4)

    def test_future_work_heading_is_NOT_nonclaim_framing(self):
        """"Future work"/"Roadmap" sections DO make forward-looking assertions of
        their own, so they are deliberately absent from the vocabulary."""
        self.assertEqual(len(self._scan("## Future work", "ICN is production-ready.")), 1)

    def test_roadmap_heading_is_NOT_nonclaim_framing(self):
        self.assertEqual(len(self._scan("## Roadmap", "ICN is production-ready.")), 1)

    def test_scope_word_inside_ordinary_heading_does_not_exempt(self):
        self.assertEqual(
            len(self._scan("## Deployment scope", "ICN is production-ready.")), 1
        )


# ── Family B: label/value semantics across ":" ───────────────────────────────


class TestLabelValuePairs(unittest.TestCase):
    def test_qualifier_label_before_colon(self):
        self.assertEqual(
            linter.scan_lines("docs/x.md", ["**Target:** Production-ready Q1 2026"]), []
        )

    def test_qualifier_label_variants(self):
        for label in ("Goal", "Milestone", "Objective", "Planned", "ETA"):
            with self.subTest(label=label):
                self.assertEqual(
                    linter.scan_lines("docs/x.md", [label + ": production-ready"]), []
                )

    def test_not_yet_status_marker_qualifies_its_row(self):
        """Covered by the ❌ status marker in NEGATION_RE (same precedent as 🟡),
        not by the label/value machinery — the "general availability" pattern
        matches only a prefix, so span comparison was the wrong mechanism."""
        self.assertEqual(
            linter.scan_lines(
                "docs/x.md",
                ["- \u274c **General Availability**: not yet in this snapshot"],
            ),
            [],
        )

    def test_pending_status_marker_qualifies_its_clause(self):
        self.assertEqual(
            linter.scan_lines("docs/x.md", ["\u23f3 live federation is pending"]), []
        )

    def test_status_marker_in_another_table_cell_does_NOT_exempt(self):
        """"|" is a clause delimiter on purpose: one cell must not excuse another."""
        self.assertEqual(
            len(linter.scan_lines(
                "docs/x.md", ["| Phase 2 | \u23f3 | ICN is production-ready |"])), 1
        )

    def test_status_marker_does_not_reach_a_later_clause(self):
        """A marker in an earlier clause must not excuse a separate assertion."""
        self.assertEqual(
            len(linter.scan_lines(
                "docs/x.md", ["\u274c not shipped; ICN is production-ready."])), 1
        )

    def test_neither_is_a_negator(self):
        self.assertEqual(
            linter.scan_lines(
                "docs/x.md",
                ["- Neither proof claims production reachability or live federation;"],
            ),
            [],
        )

    def test_may_not_claim_leadin(self):
        self.assertEqual(
            linter.scan_lines(
                "docs/x.md", ["**It may not claim:** production-ready, pilot-ready,"]
            ),
            [],
        )

    def test_cannot_claim_leadin(self):
        self.assertEqual(
            linter.scan_lines("docs/x.md", ["It cannot claim: live federation, pilot."]), []
        )

    # ── adversarial ──

    def test_revoking_label_does_not_launder(self):
        """The case that broke the first draft of this rule: the label carries a
        negation WORD but asserts something."""
        v = linter.scan_lines("docs/x.md", ["Nonclaims no longer apply: ICN is production-ready."])
        self.assertEqual(len(v), 1)

    def test_plain_status_label_does_not_exempt(self):
        self.assertEqual(
            len(linter.scan_lines("docs/x.md", ["Status: ICN is production-ready."])), 1
        )

    def test_clause_label_cannot_be_excused_by_its_value(self):
        """A full assertion before the colon is not a bare label, so a negation in
        the value must not excuse it."""
        self.assertEqual(
            len(linter.scan_lines("docs/x.md", ["ICN is production-ready: not a drill."])), 1
        )

    def test_negation_does_not_cross_a_sentence_boundary(self):
        self.assertEqual(
            len(linter.scan_lines("docs/x.md", ["Target: later. ICN is production-ready."])), 1
        )

    def test_negation_does_not_cross_a_semicolon(self):
        self.assertEqual(
            len(linter.scan_lines(
                "docs/x.md", ["Target: Q1 2026; ICN is production-ready."])), 1
        )

    def test_only_one_colon_boundary_is_crossed(self):
        """A qualifier two colons away must not reach the claim."""
        self.assertEqual(
            len(linter.scan_lines("docs/x.md", ["Target: scope: ICN is production-ready."])), 1
        )


# ── Family C: fenced code samples ────────────────────────────────────────────


class TestFencedCode(unittest.TestCase):
    def test_config_sample_is_not_a_claim(self):
        lines = ["```toml", 'backend = "age"  # Software keystore (production-ready)', "```"]
        self.assertEqual(linter.scan_lines("docs/x.md", lines), [])

    def test_tilde_fence_also_exempt(self):
        lines = ["~~~", "STATUS: PRODUCTION READY", "~~~"]
        self.assertEqual(linter.scan_lines("docs/x.md", lines), [])

    def test_fenced_liveness_path_is_not_a_liveness_claim(self):
        """"/etc/letsencrypt/live/..." is a path, not a live endpoint."""
        lines = ["```nginx", "ssl_certificate /etc/letsencrypt/live/api.example.org/f.pem;", "```"]
        self.assertEqual(
            linter.scan_historical_liveness("docs/PROJECT_STATUS_2025-12-06.md", lines), []
        )

    # ── adversarial ──

    def test_claim_after_the_fence_closes_still_flags(self):
        lines = ["```", "sample = 1", "```", "ICN is production-ready."]
        v = linter.scan_lines("docs/x.md", lines)
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].line, 4)

    def test_claim_before_the_fence_opens_still_flags(self):
        v = linter.scan_lines("docs/x.md", ["ICN is production-ready.", "```", "x", "```"])
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].line, 1)

    def test_unclosed_fence_does_not_silently_exempt_a_later_claim(self):
        """An unclosed fence swallows the rest of the file. Documented, and the
        reason this is acceptable: the site renders it as code too, so the claim
        is not presented as prose either. Asserted so the behaviour is explicit
        rather than accidental."""
        lines = ["```", "ICN is production-ready."]
        self.assertEqual(linter.scan_lines("docs/x.md", lines), [])

    def test_heading_inside_a_fence_does_not_reset_nonclaim_state(self):
        """A "#" comment in a shell fence must not parse as a markdown heading and
        end an out-of-scope section."""
        lines = [
            "## Out of scope",
            "```bash",
            "# Status",
            "```",
            "- live federation",
        ]
        self.assertEqual(linter.scan_lines("docs/x.md", lines), [])


# ── Negated sentences that wrap across lines ─────────────────────────────────


class TestNegatedContinuation(unittest.TestCase):
    def test_dangling_negator_covers_the_wrapped_sentence(self):
        lines = [
            "`proposed` — this record states the decision for review. It does not",
            "adopt itself, authorize a production deployment, or certify any profile as",
            "production-ready.",
        ]
        self.assertEqual(linter.scan_lines("docs/x.md", lines), [])

    def test_wrapped_red_line_list(self):
        lines = [
            "**It may not claim:** production-ready, pilot-ready, organizer-approved,",
            "accessibility-complete, live federation, real institutional deployment, formal",
            "NYCN pilot, production trusted issuance.",
        ]
        self.assertEqual(linter.scan_lines("docs/x.md", lines), [])

    def test_never_be_used_to_claim_leadin_wraps(self):
        lines = [
            "**What this document must never be used to claim:** production readiness, pilot",
            "readiness, organizer approval, accessibility completion (#2041 is open",
            "gate), live federation, or that any item exists. When this document and",
        ]
        self.assertEqual(linter.scan_lines("docs/x.md", lines), [])

    def test_not_built_leadin_wraps(self):
        lines = [
            "Future lanes with no issue gate yet, **not built**: multi-person / two-member",
            "action flow, QR node-claim ceremony, signed /",
            "immutable image, partner-distributable image, live federation.",
        ]
        self.assertEqual(linter.scan_lines("docs/x.md", lines), [])

    # ── adversarial ──

    def test_scope_stops_at_the_sentence_terminator_on_a_continuation_line(self):
        """The laundering route to close: a real claim AFTER the negated sentence
        ends, on the very line that ends it."""
        lines = [
            "**It may not claim:** production-ready, pilot-ready,",
            "organizer-approved. ICN is production-ready.",
        ]
        v = linter.scan_lines("docs/x.md", lines)
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].line, 2)

    def test_blank_line_ends_the_continuation(self):
        lines = ["This record does not", "", "ICN is production-ready."]
        v = linter.scan_lines("docs/x.md", lines)
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].line, 3)

    def test_heading_ends_the_continuation(self):
        lines = ["This record does not", "## Status", "ICN is production-ready."]
        v = linter.scan_lines("docs/x.md", lines)
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].line, 3)

    def test_fence_ends_the_continuation(self):
        lines = ["This record does not", "```", "x", "```", "ICN is production-ready."]
        v = linter.scan_lines("docs/x.md", lines)
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].line, 5)

    def test_continuation_does_not_run_past_a_finished_sentence(self):
        """An ordinary paragraph after a finished negated sentence is unprotected."""
        lines = [
            "It does not",
            "certify any profile.",
            "ICN is production-ready.",
        ]
        v = linter.scan_lines("docs/x.md", lines)
        self.assertEqual(len(v), 1)
        self.assertEqual(v[0].line, 3)

    def test_leadin_ending_at_its_colon_still_requires_bullets(self):
        """A lead-in that hands off to the next line does NOT open a continuation;
        the avoid-list machine governs, and it requires bullets."""
        v = linter.scan_lines("docs/x.md", ["We do not claim:", "ICN is production-ready."])
        self.assertEqual(len(v), 1)

    def test_plain_prose_is_unaffected(self):
        self.assertEqual(
            len(linter.scan_lines("docs/x.md", ["ICN is production-ready today."])), 1
        )


# ── Allowlist integrity ──────────────────────────────────────────────────────


class TestAllowlistIntegrity(unittest.TestCase):
    """ALLOWLIST is keyed "relpath:line", so an edit above an entry silently
    retargets it. A stale entry is worse than none: it suppresses nothing the
    author intended and quietly excuses whatever moved into that line."""

    def test_every_entry_points_at_a_real_red_line_phrase(self):
        for key, reason in linter.ALLOWLIST.items():
            with self.subTest(entry=key):
                rel, _, lineno = key.rpartition(":")
                path = os.path.join(REPO_ROOT, rel)
                if not os.path.isfile(path):
                    self.skipTest(rel + " not present")
                with open(path, encoding="utf-8", errors="replace") as f:
                    lines = f.read().splitlines()
                n = int(lineno)
                self.assertLessEqual(n, len(lines), msg=key + " is past end of file")
                line = lines[n - 1]
                self.assertTrue(
                    any(pat.search(line) for pat, _rule in linter.OVERCLAIM_PATTERNS),
                    msg=key + " no longer contains a red-line phrase: " + repr(line[:90]),
                )

    def test_every_entry_has_a_documented_reason(self):
        for key, reason in linter.ALLOWLIST.items():
            with self.subTest(entry=key):
                self.assertGreater(
                    len(reason.strip()), 40, msg=key + " needs a real justification"
                )


class TestContinuationBreadthLimits(unittest.TestCase):
    """Guards on how much a wrapped negated sentence is allowed to excuse."""

    def test_copula_negation_does_not_open_a_continuation(self):
        """"is not" at end of line is ordinary prose, not a red-line framing."""
        v = linter.scan_lines("docs/x.md", ["The gateway is not", "production-ready."])
        self.assertEqual(len(v), 1)

    def test_colon_on_a_continuation_line_ends_the_reach(self):
        v = linter.scan_lines(
            "docs/x.md", ["This does not", "a drill: ICN is production-ready."]
        )
        self.assertEqual(len(v), 1)

    def test_does_not_still_opens_a_continuation(self):
        self.assertEqual(
            linter.scan_lines(
                "docs/x.md",
                ["It does not", "certify any profile as", "production-ready."],
            ),
            [],
        )

    def test_red_line_modals_still_open_a_continuation(self):
        for modal in ("must not", "may not", "cannot", "never"):
            with self.subTest(modal=modal):
                self.assertEqual(
                    linter.scan_lines(
                        "docs/x.md", ["This document " + modal, "claim production-ready status."]
                    ),
                    [],
                )


if __name__ == "__main__":
    unittest.main(verbosity=2)