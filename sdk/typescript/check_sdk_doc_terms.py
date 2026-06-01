#!/usr/bin/env python3
"""
Guard: the hand-written TypeScript SDK developer surface (README, examples, and
the in-source docstrings in src/index.ts) must describe the client's *actual*
exported methods and the canonical ICN ledger vocabulary — settlements and
positions denominated in a `unit`, not payments/balances in a `currency`.

ICN is mutual-credit infrastructure, not a payment/banking product (see
docs/adr/ADR-0006 and docs/dev/language-guide.md). Earlier SDK docs had drifted
to fintech vocabulary (`client.pay()`, `client.getBalance()`, `createEscrow`,
recurring payments, fiat budgets) and even documented helper methods that were
never shipped on `ICNClient`. This guard ties the docs back to code truth so the
drift cannot silently return.

Code-truth anchor: the guard reads the exported `async` methods from
src/index.ts and asserts that the canonical ledger methods exist
(settle, getPosition, getHistory, batchSettle) and the deprecated ones do NOT
(pay, getBalance, batchPay, createEscrow, createRecurringPayment, createBudget,
...). If those assumptions ever change, it exits 2 (misconfigured) rather than
giving false confidence — the same fail-loud contract used by
docs/api/check_api_doc_terms.py.

Exemption: Markdown blockquote lines (`>`-prefixed) are skipped, so a section may
explicitly name a deprecated / never-shipped helper in order to warn against it
(the "not implemented" note in README.md). The blockquote is the disclaimer,
mirroring the banner-as-exemption pattern used elsewhere in the repo.

Exit codes: 0 = clean; 1 = violations found; 2 = guard misconfigured.
"""
import argparse
import re
import sys
from pathlib import Path

# Canonical client methods that MUST exist (code-truth sanity check).
CANONICAL_METHODS = ("settle", "getPosition", "getHistory", "batchSettle")

# Deprecated / never-shipped methods that must NOT exist on the client. If any of
# these reappears as a real method, the migration assumptions changed and the
# guard should be revisited rather than trusted.
FORBIDDEN_METHODS = (
    "pay", "getBalance", "batchPay",
    "createEscrow", "listEscrows", "releaseEscrow", "refundEscrow",
    "createRecurringPayment", "listRecurringPayments",
    "updateRecurringPayment", "cancelRecurringPayment",
    "createBudget", "listBudgets", "updateBudget", "deleteBudget",
)

# Deprecated tokens that must not appear as *current* SDK API. (rule, regex, hint)
# Note: `client.crossPay(` and the cross-currency FX surface are intentionally
# NOT matched — that is a live method with its own field names (from_currency/
# to_currency) and is out of scope for this guard.
DEPRECATED_PATTERNS = [
    ("client.pay", re.compile(r"\bclient\.pay\("),
     "client.pay() does not exist -> client.settle()"),
    ("client.getBalance", re.compile(r"\bclient\.getBalance\("),
     "client.getBalance() does not exist -> client.getPosition()"),
    ("client.batchPay", re.compile(r"\bclient\.batchPay\("),
     "client.batchPay() does not exist -> client.batchSettle()"),
    ("escrow-helpers",
     re.compile(r"\bclient\.(?:createEscrow|listEscrows|releaseEscrow|refundEscrow)\("),
     "escrow helpers were never shipped; ICN has no escrow facility"),
    ("recurring-payment-helpers",
     re.compile(r"\bclient\.(?:create|list|update|cancel)RecurringPayments?\("),
     "recurring-payment helpers were never shipped"),
    ("budget-helpers",
     re.compile(r"\bclient\.(?:createBudget|listBudgets|updateBudget|deleteBudget)\("),
     "fiat-budget helpers were never shipped"),
    ("notification-helpers",
     re.compile(r"\bclient\.(?:connectNotifications|listNotifications|markNotificationRead|getNotificationCount)\("),
     "notification helpers were never shipped; use connectWebSocket()/subscribe()"),
    ("PaymentCreated-event", re.compile(r"""['"]PaymentCreated['"]"""),
     "ledger event type is 'SettlementCreated', not 'PaymentCreated'"),
    # Deprecated *balance* surface. The canonical position query is the
    # `/ledger/{coop}/position/{did}` route and the `.position` property. There is
    # no live `/balance` route and no live `.balance` field anywhere in the SDK
    # types (TreasuryBalanceResponse exposes `positions`), so these two patterns
    # are unambiguous and catch copy-paste-invalid examples the method patterns
    # miss (examples/ are not type-checked: tsconfig rootDir is src).
    ("balance-route", re.compile(r"/ledger/[^\s'\"`]*?/balance\b"),
     "ledger route is `/ledger/{coop}/position/{did}`, not `/balance`"),
    ("balance-property", re.compile(r"\.balance\b"),
     "position/treasury responses expose `.position`, not `.balance`"),
    # NOTE: a bare `currency` field/property is deliberately NOT matched. Unlike
    # the deprecated *balance* surface, `currency` is live vocabulary on two
    # exported flows: crossPay() FX (`from_currency` / `to_currency`) and the
    # Flow C `governance.proposeSpend()` treasury API, whose ProposeSpendRequest
    # accepts `currency?` and ProposeSpendResponse returns `currency` (src/types.ts).
    # The bare token cannot distinguish deprecated-ledger use from those live uses,
    # so matching it as a blanket rule would reject valid documentation. Instead, a
    # *settlement-request* `currency:` is caught by the context-aware check in
    # scan_file() below (only when it appears inside a settle()/batchSettle() call),
    # which leaves the live crossPay()/Flow C uses untouched.
]

# Context-aware settlement check: `currency:` is only deprecated when it appears
# as a field inside a settle()/batchSettle() request. SettlementRequest uses `unit`.
_SETTLE_OPEN = re.compile(r"\b(?:settle|batchSettle)\s*\(")
_SETTLE_CURRENCY = re.compile(r"(?<![A-Za-z_])currency\s*:")

# Context-aware ledger-read check. A `.currency` / `.balance` *property* read is
# only deprecated on the canonical ledger surfaces (Position/Transaction/Treasury
# all expose `.unit` / `.position`). We bind variables that come from a canonical
# ledger getter or transaction iteration, then flag `.currency` / `.balance` reads
# on exactly those. This leaves the live crossPay() result and the Flow C
# proposeSpend response (whose `.currency` is valid) untouched.
_LEDGER_ASSIGN = re.compile(
    r"\b(?:const|let|var)\s+([A-Za-z0-9_]+)\s*=\s*(?:await\s+)?(?:[A-Za-z0-9_]+\.)?"
    r"(?:getPosition|getHistory|getTreasuryStatus|getTreasuryPosition|settle|batchSettle)\s*\(")
_LEDGER_ITER = re.compile(
    r"\.transactions\.(?:forEach|map|filter|reduce|find|some|every)\(\s*(?:async\s*)?\(?\s*([A-Za-z0-9_]+)")
_LEDGER_FOROF = re.compile(
    r"\bfor\s*\(\s*(?:const|let)\s+([A-Za-z0-9_]+)\s+of\b[^)]*\.transactions\b")

# Hand-written developer-facing surface. Generated artifacts (src/api-types.ts,
# src/generated/**) are deliberately excluded — they are regenerated from the
# OpenAPI spec by the api-types workflow and must not be hand-edited.
CHECKED = [
    "README.md",
    "examples/README.md",
    "examples",      # *.ts under here
    "src/index.ts",
]


def exported_methods(index_ts: Path) -> set:
    text = index_ts.read_text(encoding="utf-8")
    return set(re.findall(r"\basync\s+([A-Za-z0-9_]+)\s*\(", text))


def iter_files(sdk_root: Path):
    seen = set()
    for entry in CHECKED:
        p = sdk_root / entry
        if p.is_dir():
            for f in sorted(p.glob("**/*.ts")):
                if f not in seen:
                    seen.add(f)
                    yield f
        elif p.is_file():
            if p not in seen:
                seen.add(p)
                yield p


def scan_file(path: Path, sdk_root: Path):
    violations = []
    rel = str(path.relative_to(sdk_root))
    in_settle = False        # inside a settle()/batchSettle() call's argument list
    depth = 0                # paren depth, counted from the opening settle(
    ledger_vars = set()      # identifiers bound to a canonical ledger result
    for i, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if line.lstrip().startswith(">"):  # markdown blockquote = disclaimer, exempt
            in_settle = False
            continue
        for rule, rx, hint in DEPRECATED_PATTERNS:
            if rx.search(line):
                violations.append((rel, i, rule, hint, line.strip()))

        # Context-aware: flag `currency:` only within a settle()/batchSettle() call.
        if not in_settle:
            m = _SETTLE_OPEN.search(line)
            if m:
                seg = line[m.end() - 1:]          # from the opening '(' onward
                if _SETTLE_CURRENCY.search(seg):
                    violations.append((rel, i, "settlement-currency-field",
                                       "settlement request field is `unit`, not `currency`",
                                       line.strip()))
                depth = seg.count("(") - seg.count(")")
                in_settle = depth > 0
        else:
            if _SETTLE_CURRENCY.search(line):
                violations.append((rel, i, "settlement-currency-field",
                                   "settlement request field is `unit`, not `currency`",
                                   line.strip()))
            depth += line.count("(") - line.count(")")
            in_settle = depth > 0

        # Bind ledger-typed variables (assignment must precede/equal the read line).
        for rx in (_LEDGER_ASSIGN, _LEDGER_ITER, _LEDGER_FOROF):
            for var in rx.findall(line):
                ledger_vars.add(var)
        # Flag deprecated `.currency` reads on canonical ledger variables only.
        # (`.balance` is already caught globally by the unambiguous balance rule.)
        for var in ledger_vars:
            if re.search(r"\b" + re.escape(var) + r"\.currency\b", line):
                violations.append((rel, i, "ledger-read-currency",
                                   "ledger reads expose `.unit`, not `.currency`",
                                   line.strip()))
                break
    return violations


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--sdk-root", default=".", help="path to sdk/typescript")
    args = ap.parse_args()
    sdk_root = Path(args.sdk_root).resolve()

    index_ts = sdk_root / "src" / "index.ts"
    if not index_ts.is_file():
        print(f"ERROR: cannot find {index_ts}", file=sys.stderr)
        return 2

    methods = exported_methods(index_ts)
    missing = [m for m in CANONICAL_METHODS if m not in methods]
    resurrected = [m for m in FORBIDDEN_METHODS if m in methods]
    if missing:
        print(f"ERROR: canonical client methods missing from src/index.ts: {missing}.\n"
              "The guard's premise no longer holds; update check_sdk_doc_terms.py.",
              file=sys.stderr)
        return 2
    if resurrected:
        print(f"ERROR: deprecated fintech methods now exist on the client: {resurrected}.\n"
              "If these were intentionally (re)introduced, revisit ADR-0006 and update this guard.",
              file=sys.stderr)
        return 2

    all_v = []
    for f in iter_files(sdk_root):
        all_v.extend(scan_file(f, sdk_root))

    if all_v:
        print("Deprecated fintech vocabulary presented as current SDK API:\n")
        for rel, ln, rule, hint, src in all_v:
            print(f"  {rel}:{ln} [{rule}] {hint}")
            print(f"      {src}")
        print(f"\n{len(all_v)} violation(s). ICN exposes settlements/positions in a `unit`, "
              "not payments/balances in a currency (ADR-0006). Markdown blockquotes are "
              "exempt so deprecated names can be documented as such.")
        return 1

    print("OK: SDK docs/examples use canonical ICN ledger primitives "
          f"(checked against {len(methods)} exported methods in src/index.ts).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
