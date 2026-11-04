# Proposal: an endpoint-liveness claim category

**Status:** proposal. Not implemented. No linter behaviour changes with this document.
**Companion to:** [GATE_RATCHET_PLAN.md](GATE_RATCHET_PLAN.md), [../dev/language-guide.md](../dev/language-guide.md)
**Raised by:** the `icn.zone` routing-table incident (below), during the work that
extended `just website-claims` to the `docs/` files that actually publish.

## The gap this exists to close

`docs/deployment/ICN_ZONE_ROUTING.md` published a routing table whose status
column marked `api.icn.zone`, `pilot.icn.zone` and `metrics.icn.zone` as
reachable. At the time, `pilot.` and `metrics.` had no DNS record at all
(NXDOMAIN) and `api.` returned HTTP 530.

The uncomfortable part is that **no scope change would have caught this**:

- `docs/deployment` is already in the readiness linter's hardcoded `SCAN_DIRS`,
  so the file was already being scanned on every CI run.
- The linter reported it clean, and was right to: the file carries no banner, is
  not a dated/status-named historical doc, and none of its lines match any
  existing category.

A status cell reading `Active`, or `Live (GitHub Pages + CNAME)`, is not an
affirmative readiness claim in the sense the current patterns describe. It is a
**reachability assertion about a named network endpoint** — a different kind of
claim, and one this repository does not yet have a category for.

Under ADR-0032 the public site is a truth boundary, and a routing table is one of
the most literally checkable things on it. It should not be the one claim class
with no gate.

## Why the existing categories do not reach it

| Category | Keys off | Why it misses a routing table |
|---|---|---|
| affirmative overclaim | a fixed phrase list (readiness / federation / availability wording) | a status cell is a single word that is not on the list |
| `unmarked-historical-liveness` | a **dated or status-named filename** | `ICN_ZONE_ROUTING.md` is an ordinary current doc |

The second is the near miss: it already understands that liveness language needs
an evidence trail, and it already runs regardless of any banner. It is scoped by
filename, so it never looks at a current operational doc.

## Proposed semantics

Match **structurally**, not by keyword. A finding is a table row that pairs a
cell containing a network identifier (a hostname, URL, or subdomain) with a
separate cell whose entire content is a bare liveness token, where the row
carries no evidence reference and no negating or forward-looking qualifier.

Structure matters because it is what distinguishes an assertion from a mention.
The same words appear constantly in prose and configuration where they assert
nothing:

**Lines the category must NOT flag** — each is a mention, not a claim:

- `ssl_certificate /etc/letsencrypt/live/api.your-coop.org/fullchain.pem;` — the
  word `live` is a filesystem path component inside a fenced config sample.
- `Verify bootstrap peers are reachable: telnet bootstrap.example.com 7777` — an
  instruction to the reader, not a statement of current fact.
- `Forgejo running at forge.intercooperative.network.` — prose; the proposal
  deliberately does not try to adjudicate prose in its first form.
- Any row already qualified `planned`, `pending`, `down`, `target`, or marked
  with a not-yet indicator.

Content inside fenced code blocks is out of scope entirely: a config sample is a
sample, and the existing linter already treats fenced content as literal.

## Proposed exemption marker

Mirror the `historical-proof` marker rather than inventing a second mechanism:

    <!-- claim-class: endpoint-liveness probe=<how it was checked> date=<YYYY-MM-DD> evidence=<link> -->

`probe` and `date` required for validity, `evidence` recommended, same as the
existing marker's `ref`/`date` contract. A stale/archive banner must **not**
exempt this category, for the same reason it does not exempt
`unmarked-historical-liveness`: a banner lets a document describe what was once
true, but a reachability table is read as current infrastructure regardless of
how the page is framed.

The intended outcome is that asserting an endpoint is reachable costs the author
one line recording how they checked — which is exactly the evidence that was
missing when the incident shipped.

## Measured blast radius

Measured over the 686 published documents (the corpus the widened
`just website-claims` now covers), before any exemption markers exist:

| Prototype | Findings | Files | Verdict |
|---|---|---|---|
| structural (table row: host cell + bare status cell, outside fences) | 5 | 3 | tractable; catches all three incident rows |
| naive (any line containing a hostname and a liveness word) | 29 | 17 | unusable; dominated by config samples and prose |

The structural prototype's five findings are the three `icn.zone` rows from the
incident plus two website-status rows in `docs/design/CLAUDE_DESIGN_CONTEXT.md`
and `docs/reference/project-index/source-tree-map.md`. That is a small enough
backlog to triage in the same change that introduces the category.

The gap between the two rows in that table is the whole argument for matching on
structure. A keyword category here would be noisy enough that people would
weaken it, and a weakened claim gate is worse than an absent one.

## Non-goals

- Probing DNS or HTTP from CI. The category asks for a recorded claim class, not
  a live network check; CI that resolves hostnames would be flaky and would make
  the gate depend on the very infrastructure it is describing.
- Adjudicating liveness language in prose. First form is tables only.
- Any change to the existing categories, their patterns, or their allowlist.

## Open questions

1. Should the category also cover definition lists and two-column bullet forms
   (`- api.icn.zone — Active`), or stay strictly tabular in its first form?
2. Should it run at default scope (all of `SCAN_DIRS`) or only over the published
   manifest? The incident file sits in both, so either closes it.
3. Ratchet entry: straight to blocking, or one observational phase first, per the
   phases in [GATE_RATCHET_PLAN.md](GATE_RATCHET_PLAN.md)?
