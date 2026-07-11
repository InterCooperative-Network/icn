#!/usr/bin/env python3
"""Synthetic tests for the public-docs address boundary guard added for icn#2393.

Runs the scan_public_docs_boundary / docs_address_violations logic in
scripts/check-truth-spine.py against SYNTHETIC values only. No concrete provider
address ever appears here (that is the whole point of the guard). Exit 0 = pass.

Run: python3 scripts/tests/test_public_docs_boundary.py
"""
import contextlib
import importlib.util
import io
import pathlib
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
SPINE = ROOT / "scripts" / "check-truth-spine.py"

spec = importlib.util.spec_from_file_location("cts_under_test", SPINE)
cts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cts)

failures = []


def check(desc, cond):
    if cond:
        print(f"  ok   {desc}")
    else:
        print(f"  FAIL {desc}")
        failures.append(desc)


def flagged(text):
    """True if docs_address_violations reports at least one violation."""
    return bool(cts.docs_address_violations(text))


# ---- MUST FLAG (disallowed concrete host literals) ---------------------------
FLAG = [
    ("planted disallowed IPv4 (private)", "peer at 10.20.30.40 here"),
    ("planted disallowed IPv4 (public)", "resolver 8.8.8.8"),
    ("planted disallowed IPv4 (rfc1918 172.16)", "node 172.16.9.9"),
    ("URL authority with disallowed IPv4", "curl http://10.0.0.5:8080/v1/health"),
    ("compressed IPv6 at start (>=3 groups)", "addr ::a:b:c:d listening"),
    ("compressed IPv6 in middle (>=3 groups)", "route 1:2::3:4:5 via mesh"),
    ("compressed IPv6 at end (>=3 groups)", "fd12:3456:789a::1 reachable"),
    ("full 8-group IPv6", "fd00:1:2:3:4:5:6:7 up"),
    ("bracketed IPv6 URL (2-group, bracket forces flag)", "http://[fdff::1]:8080/x"),
    ("bracketed IPv6 URL (>=3 group)", "ws://[fd12:3456::1]:30080/ws"),
]

# ---- MUST NOT FLAG (allowed documentation / bind / protocol-safe forms) ------
ALLOW = [
    ("IPv4 loopback", "bind 127.0.0.1:8080"),
    ("IPv4 wildcard", "listen 0.0.0.0:8080"),
    ("QEMU slirp host alias", "host is 10.0.2.2 inside qemu"),
    ("RFC5737 192.0.2.x", "example gateway 192.0.2.10:30080"),
    ("RFC5737 198.51.100.x", "peer 198.51.100.9"),
    ("RFC5737 203.0.113.x", "peer 203.0.113.7"),
    ("RFC5737 network form", "subnet 192.0.2.0/24"),
    ("IPv6 loopback", "bind ::1 only"),
    ("IPv6 unspecified", "bind [::]:8080"),
    ("RFC3849 2001:db8::1", "example v6 2001:db8::1"),
    ("RFC3849 multi-group", "example 2001:db8:1:2::3"),
    ("RFC3849 bracketed URL", "http://[2001:db8::5]:8080/x"),
    (".example hostname (no host detection)", "gateway.example.coop:30080"),
    ("env-var placeholder", "curl http://${ICN_GATEWAY_HOST}:30080/v1/health"),
    # False-positive guards: hex-looking Rust / capability syntax, versions
    ("rust crypto import aead::", "use chacha20poly1305::aead::{Aead, KeyInit, OsRng};"),
    ("short 2-group hex :: (dead:beef::)", "marker dead:beef:: token"),
    ("capability action a::b (2 groups)", 'Action::new("read::write")'),
    ("four-plus-part version string", "version 1.2.3.4.5 released"),
    ("semver three-part", "release 1.2.3 shipped"),
    ("bare 2-group IPv6 (intentional FP-avoidance gap)", "note fd00::1 somewhere"),
]

print("== MUST FLAG ==")
for desc, text in FLAG:
    check(desc, flagged(text))

print("== MUST NOT FLAG ==")
for desc, text in ALLOW:
    check(desc, not flagged(text))

# ---- Diagnostics must never contain the matched value ------------------------
print("== diagnostics withhold value ==")
cts.errors.clear()
cts.warnings.clear()
with tempfile.TemporaryDirectory() as td:
    root = pathlib.Path(td)
    (root / "docs").mkdir()
    (root / "docs" / "planted.md").write_text("gateway at 10.99.88.77 here\n", encoding="utf-8")
    with contextlib.redirect_stdout(io.StringIO()):
        cts.scan_public_docs_boundary(root)
    msgs = "\n".join(cts.errors)
    check("scan flagged the planted file", "docs/planted.md" in msgs)
    check("diagnostic does NOT contain the planted value", "10.99.88.77" not in msgs)
    check("diagnostic reports a line number", "docs/planted.md:1" in msgs)

# ---- Fail closed on undecodable input ---------------------------------------
print("== fail closed on unreadable/undecodable ==")
cts.errors.clear()
cts.warnings.clear()
with tempfile.TemporaryDirectory() as td:
    root = pathlib.Path(td)
    (root / "docs").mkdir()
    # invalid UTF-8 bytes in a .md surface -> must fail closed, not skip silently
    (root / "docs" / "bad.md").write_bytes(b"\xff\xfe gateway \x80\x81\n")
    with contextlib.redirect_stdout(io.StringIO()):
        cts.scan_public_docs_boundary(root)
    check("undecodable surface fails closed (error raised)", len(cts.errors) >= 1)

# ---- Excluded files are not scanned -----------------------------------------
print("== deferred-file exclusion honored ==")
cts.errors.clear()
cts.warnings.clear()
with tempfile.TemporaryDirectory() as td:
    root = pathlib.Path(td)
    (root / "docs" / "adr").mkdir(parents=True)
    excluded = "docs/adr/ADR-0003-ipv6-dual-stack-transport-with-endpoint-sets.md"
    (root / excluded).write_text("ULA fd12:3456:789a::1 illustration\n", encoding="utf-8")
    with contextlib.redirect_stdout(io.StringIO()):
        cts.scan_public_docs_boundary(root)
    check("excluded illustration file is not flagged", len(cts.errors) == 0)

print()
if failures:
    print(f"FAILED: {len(failures)} case(s)")
    sys.exit(1)
print("ALL PASS")
sys.exit(0)
