---
name: systemic-leverage
description: Use after ESTABLISHING a defect — reproduced, not suspected. Separates the immediate cause from the mechanism that made the defect possible to write, then classifies the structural opportunity NOW / FOLLOW_UP / ARCHITECTURAL / NONE without widening the active delivery contract. Triggers on: bug fixed, root cause found, "why was this possible", recurring failure, review finding, maintenance sweep, post-incident.
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
truth_contract:
  canonical_sources:
    - ops/state/truth/engineering-leverage.json   # the loop, dispositions, pattern catalogue
    - ops/state/truth/delivery.json               # lifecycle, blocker predicate, freeze, follow-up ledger
  live_load_required:
    - "gh issue list --state open --search '<pattern keywords>'"
  examples_only: []
---

Load the catalogue before answering. It is data, not prose to be recalled:

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
python3 -c "
import json,sys
d=json.load(open('${REPO_ROOT}/ops/state/truth/engineering-leverage.json'))
for s in d['loop']['stages']: print(f\"{s['id']:<20} {s['asks']}\")
print()
for p in d['patterns']: print(f\"{p['id']:<32} {p['signal'][:96]}\")
"
```

## When this applies

Only after a defect is **established** — you reproduced it. A suspicion is not a defect, and a
suspicion does not earn a systemic candidate.

Skip it entirely for non-engineering work. Writing, research and documentation tasks do not
perform a defect analysis.

## The two questions

The habit this skill exists to build is asking the second one:

- What test would catch this next time?
- **What would have made this bug difficult or impossible to write?**

Tests remain necessary. Prevention is usually higher leverage.

## The loop

Work the stages in `loop.stages`. Four of them are distinct claims and collapsing them is the
most common failure:

Render the distinctions rather than reading them from here — a copy in this file would go stale
while still reading plausibly:

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
python3 -c "
import json
d=json.load(open('${REPO_ROOT}/ops/state/truth/engineering-leverage.json'))
for s in d['loop']['stages']:
    if s.get('must_not_be'):
        print(f\"{s['id']:<20} asks: {s['asks']}\")
        print(f\"{'':<20} is NOT: {s['must_not_be']}\")
print()
print('claim separation:', d['loop']['claim_separation']['rule'])
"
```

The `claim_separation` rule is the one to guard. Proving the instance is fixed is not proving the
class is prevented; claim the second only when a mechanism makes the defect unrepresentable or a
gate rejects it, and say which.

## Matching a pattern

Read `patterns[].signal`. If one matches, use its `question` and `preferred_response` rather than
inventing a direction, and cite the existing `examples[].reference` — the class has been seen
before, and that is the point of the catalogue.

If none matches, that is informative. Either the class is new, or there is no class.

## Classify, then continue

Every candidate gets exactly one disposition, and **the structured predicates decide it**. The
`meaning` and `test` fields explain a disposition to a human; they are not the decision procedure.
Reading them and judging is how two agents reach two answers from the same facts.

Answer the facts in `classification.facts`, then let the predicates resolve them:

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
python3 - "$REPO_ROOT" <<'EOF'
import json, sys
d = json.load(open(f"{sys.argv[1]}/ops/state/truth/engineering-leverage.json"))

print("Answer these, then re-run with them filled in:")
for name, why in d["classification"]["facts"].items():
    print(f"  {name}: ?    # {why}")

# Replace with your answers, e.g. {"recurring_class_established": True, ...}
facts = {}
if facts:
    matched = [n for n, s in d["dispositions"].items()
               if all(facts.get(k) == v for k, v in s["predicate"]["all_of"].items())]
    if len(matched) != 1:
        raise SystemExit(f"AMBIGUOUS: {len(matched)} matched {matched} — "
                         f"{d['classification']['rule']}")
    print("disposition:", matched[0])
    print("  explains:", d["dispositions"][matched[0]]["meaning"])
EOF
```

Exactly one predicate must match. Zero or several is ambiguity, and the canonical `rule` is
explicit that it must fail closed — never resolve it by preferring the answer you expected. If you
cannot answer a fact honestly, that is the finding: you do not yet know enough to classify.

Where the chosen disposition carries a `recorded_in` or a `constraint`, honour it.

Prose can mislead where predicates do not. With no recurring class but a boundary-changing
correction, the ARCHITECTURAL and NONE descriptions can both *read* as affirmative — while the
predicates match only `NONE`, because `recurring_class_established` is false. Follow the predicate
result. The `anti_patterns` list names manufacturing architecture work as a failure mode of this
framework itself.

## The boundary that is not negotiable

`ops/state/truth/delivery.json` remains canonical for the lifecycle. This skill never overrides it.

- A systemic observation does **not** widen the active acceptance contract.
- A systemic observation does **not** satisfy the blocker predicate. A reviewer may raise a
  candidate without it being a blocker; severity labels are advisory.
- A FOLLOW_UP or ARCHITECTURAL candidate does **not** prevent freeze, and does not reopen a frozen
  pull request.
- The default sequence stays: fix the defect → prove the defect fixed → record the candidate →
  continue bounded delivery.

If this skill and `delivery.json` ever disagree, `delivery.json` wins.

## Before creating an issue

In this order:

1. Search for an existing owner — issue, PR, or follow-up ledger entry — and reuse it.
2. If none exists, weigh the evidence and the expected payoff.
3. Create an issue only when the class is established, there is a concrete mechanism to change,
   and the work is worth scheduling.

One defect must not produce four speculative architecture tickets. For a small observation the
current pull request's follow-up ledger is usually sufficient.

## Where this matters most

During **maintenance**, actively aggregate: several follow-ups citing the same `patterns[].id` are
the signal that one bounded systemic tranche is now justified. That is where a candidate graduates
into implementation — not at the moment it is first noticed.

## Applies to the agent system too

An agent-control-plane defect gets the same loop. A retired path copied into several agent
surfaces is `duplicated-semantic-owner`, and its preferred response is a canonical owner with a
generated or mirrored projection and a drift validator — not a careful re-edit of each copy.
