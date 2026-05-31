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
