"""icn-merge-pr — the trusted ordinary-merge executable for ICN (icn#2651 stage B).

Merge semantics live HERE, in code, not in Markdown. The `merge-pr` skill is a wrapper that
resolves a PR number, calls this program, shows what it said, and asks a human before mutation.

The program has exactly two mutation outcomes: MERGED or REFUSED. It never admin-merges, never
arms auto-merge, never enqueues, and never leaves a future merge armed.
"""

__all__ = ["__version__"]

__version__ = "1.0.0"
