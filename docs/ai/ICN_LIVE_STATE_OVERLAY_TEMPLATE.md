---
Status: template
Authority: agent process
Canonical: no
Last verified: 2026-08-19
---

# ICN Live State Overlay

The live-state overlay is an **on-demand orientation report**, generated from registered truth owners plus live Git/GitHub state.

It is not a project-state document, not a committed snapshot, and not a new truth root.

## Why it exists

A fresh agent often needs a quick answer to:

- Which checkout am I in?
- Where are important fact domains owned?
- What is live in Git/GitHub right now?
- What merge policy and agent/skill registries apply?
- Which generated navigation aids are available?
- Is there a recent handoff that may contain useful history?

The overlay answers those questions without copying a permanent "current state" narrative into agent prompts.

## Generate it

From the repository root:

```bash
python3 scripts/generate-live-state-overlay.py
python3 scripts/generate-live-state-overlay.py --format json
python3 scripts/generate-live-state-overlay.py --no-gh
python3 scripts/generate-live-state-overlay.py --check --no-gh
```

The default output is Markdown to stdout. Do not commit generated overlay output.

## Sources

The generator reads or queries:

- `ops/state/truth/sources.json` for fact ownership;
- `ops/state/truth/policy.json` for merge-policy data;
- `ops/state/truth/agents.json` for agent routing metadata;
- `ops/state/truth/skills.json` for skill ownership;
- `ops/state/config/repo-map.json` for repository/worktree topology metadata;
- `git` for branch, HEAD, working-tree status, and `origin/main` when present;
- `gh` for open PR/issue state when available and not disabled;
- the generated Agent Context Spine metadata when present;
- the newest handoff **path only** as an optional memory pointer.

It does **not** treat the body of a handoff as current state.

It does **not** contain a hardcoded list of active issue numbers, PRs, phases, identity assumptions, subsystem maturity claims, or deployment facts.

## Overlay sections

A valid overlay contains:

1. **provenance**: generation time and repository root;
2. **checkout**: branch, HEAD, dirty state, observed `origin/main`;
3. **truth_owners**: the domains currently registered in `sources.json`;
4. **merge_policy**: source path and policy summary read from `policy.json`;
5. **live_pull_requests**: queried from GitHub or marked unavailable;
6. **live_issues**: queried from GitHub or marked unavailable;
7. **agent_registry**: registry path and agent count/names;
8. **skill_registry**: registry path and canonical-source map;
9. **generated_context**: Agent Context Spine presence/metadata;
10. **memory_pointer**: newest handoff path, clearly labeled non-authoritative;
11. **agent_start_rules**: the bootstrap sequence from the modern workflow.

## Interpretation rules

### Owner map, not owner content

The overlay can tell an agent that `identity_semantics` is owned by some path. The agent must still read that owner before making an identity claim.

### Live GitHub is a snapshot

A PR list is current only at the overlay's generation time. Query the specific PR again before merge/review decisions.

### Memory is historical

The newest handoff may be useful when resuming prior work. Its branch/PR/CI/issue claims must be reverified before use.

### Generated context is navigation

The Agent Context Spine can point to likely crates, docs, and checks. Its source files win on disagreement.

## Manual fallback

If the generator is unavailable:

1. run `git rev-parse --show-toplevel`, `git branch --show-current`, `git status --short`, and `git rev-parse HEAD`;
2. read `ops/state/truth/sources.json`;
3. resolve and read only the domain owner(s) needed for the task;
4. query relevant GitHub state live;
5. read `ops/state/truth/policy.json` when merge readiness matters;
6. generate a path brief with `scripts/generate-agent-context-spine.py` when useful;
7. consult a handoff only as historical/resume context.

## Self-check contract

`--check` verifies structural properties of the overlay generator, including:

- all required sections are present;
- every registered truth domain has an owner;
- no hardcoded active issue/PR list exists in generated output;
- memory is labeled non-authoritative;
- offline mode never invents GitHub state;
- JSON output round-trips.

The self-check does not prove that every domain owner is substantively correct. That is what domain-specific review and drift tooling are for.
