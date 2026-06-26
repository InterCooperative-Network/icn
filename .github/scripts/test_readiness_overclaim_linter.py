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
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
FIX = os.path.join(HERE, "fixtures", "readiness")

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
        line = '- "production-ready", "fully federated", "live pilot", "secure by default"'
        self.assertEqual(linter.scan_lines("x.md", [line]), [])

    def test_risk_cell_overclaim_label_suppressed(self):
        line = "| Entity / federation / trust | implemented but partial | high: live federation overclaim |"
        self.assertEqual(linter.scan_lines("x.md", [line]), [])

    def test_checklist_nonclaims_item_suppressed(self):
        line = "- [ ] Includes nonclaims for production readiness, formal pilot readiness, live federation."
        self.assertEqual(linter.scan_lines("x.md", [line]), [])

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


class TestFixturesEndToEnd(unittest.TestCase):
    def test_bad_fixture_flagged(self):
        v = linter.scan_file("bad_unbannered.md", os.path.join(FIX, "bad_unbannered.md"))
        self.assertGreaterEqual(len(v), 1)

    def test_good_bannered_clean(self):
        self.assertEqual(linter.scan_file("good_bannered.md", os.path.join(FIX, "good_bannered.md")), [])

    def test_good_negated_clean(self):
        self.assertEqual(linter.scan_file("good_negated.md", os.path.join(FIX, "good_negated.md")), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
