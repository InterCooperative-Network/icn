---
Status: operational
Canonical: no
Last Reviewed: 2026-06-29
---

# Organizer facilitator walkthrough human AT test plan and evidence template

## Status and purpose

**Blank test plan and evidence template only. No human assistive-technology testing is recorded by this document.**

Use this template for a real human session against `web/pilot-ui/?mode=demo`. It defines the minimum credible first assistive-technology (AT) smoke session for the four-step organizer facilitator walkthrough:

1. Standing
2. Action Cards
3. Review Preview
4. receipt/evidence explanation

The target remains a committed-fixture, read-only, demo-mode, non-mutating surface with L2 evidence only. Tracking issue [#2041](https://github.com/InterCooperative-Network/icn/issues/2041) remains open for actual human AT execution.

## Evidence boundary

The [partial accessibility evidence record](./organizer-facilitator-walkthrough-accessibility-pass.md) contains automated and browser-assisted observations such as Playwright keyboard events, axe results, accessibility-tree inspection, forced-color emulation, and reflow approximations. Those checks can establish useful structural evidence, but they cannot establish what a person hears, understands, or can operate with real AT.

| Evidence type | What it can support | What it cannot establish |
|---|---|---|
| Automated tests (`axe`, linters) | Detect covered rule violations and regressions | Human comprehension, announcement quality, or task usability |
| Browser-assisted tests (Playwright, accessibility tree, emulation) | Inspect structure, focus targets, and simulated conditions | Real screen-reader speech, human keyboard use, or actual graphical-browser zoom review |
| Human AT session | Record one person's observations with named tools and conditions | Full accessibility, legal conformance, or readiness for every user and environment |

This template becomes one human AT evidence record only after a real tester fills it in during a named device/browser/AT session. An untouched or example-filled copy is not evidence.

## Privacy and recording rules

- Use a tester role or anonymized identifier unless the tester explicitly consents to attribution.
- Do not record disability, medical, accommodation, contact, or other private details in public git or public issue comments.
- Record observations, barriers, tool versions, and repo-safe artifact names; keep recordings or private session notes outside the public repository unless every participant has approved publication.
- Do not convert an unrun check into a pass. Use **Not run** or **Blocked** and state why.

## Session record

Fill every applicable field before starting.

| Environment detail | Recorded value |
|---|---|
| Tester role/name or anonymized identifier | |
| Session date and time zone | |
| Commit SHA | |
| PR or branch, if applicable | |
| URL tested | |
| OS and version | |
| Browser and version | |
| Assistive technology and version | |
| Screen-reader/browser pair selected | |
| Input mode (keyboard, screen-reader commands, switch, voice, touch) | |
| Device type (desktop, laptop, phone, tablet) | |
| Device/model details needed to reproduce the result | |
| Browser zoom level | |
| Theme (light/dark) and contrast mode | |
| Language/locale, if relevant | |
| Display resolution or viewport, if known | |
| Audio output used for screen-reader review | |
| Repo-safe evidence artifact names/links | |

## Minimum credible human AT smoke pass

The minimum credible smoke pass is one real human session that performs all of the following. Its outcome applies only to the named tester, tools, device, and conditions in the session record; it is not a full accessibility outcome. Record **Pass**, **Pass with follow-up**, **Fail**, **Blocked**, or **Not run** for each item. Use **Fail** when the activity ran and the requirement was not met, and **Blocked** when a barrier prevented the activity from finishing.

| Required activity | Session result | Observation or evidence pointer |
|---|---|---|
| One named screen-reader/browser pair. Prefer NVDA with Firefox or Chrome on Windows, or VoiceOver with Safari on macOS, when available. | | |
| A human keyboard walkthrough using physical input, not Playwright-dispatched events. | | |
| Actual 200% browser zoom in a graphical browser, not CSS scaling or viewport emulation. | | |
| Light-theme and dark-theme review; record when the surface or platform does not expose one of these modes. | | |
| The tester can identify the demo, fixture, read-only, and non-mutating boundaries. | | |
| The tester can complete all four walkthrough steps without sighted mouse use. | | |
| The tester can explain that no participant data, assignment, review decision, or gateway state changed. | | |
| The tester can explain that receipts/evidence were described, not created or exported. | | |

Record the overall session as **Pass** only when every required activity ran and met its requirement. Use **Pass with follow-up** only when every required activity ran and the remaining observations are non-blocking and linked to follow-up work. Use **Fail**, **Blocked**, or **Not run** when any required activity has that outcome. If a required activity is unavailable, do not substitute automation; the partial session can still be useful evidence, but it is not a record of the minimum smoke pass defined here.

## Setup

Serve the committed pilot UI locally from `web/pilot-ui/`, then open the demo URL in the recorded graphical browser. One example is:

```bash
cd web/pilot-ui
python3 -m http.server 8000 --bind 127.0.0.1
```

Open `http://127.0.0.1:8000/?mode=demo`. Confirm the tested URL and commit SHA in the session record. Do not add credentials or connect the fixture walkthrough to a live gateway for this test.

Before beginning:

- enable the named screen reader and confirm its speech output with ordinary browser content;
- set browser zoom to 100% for the walkthrough unless a step below says otherwise;
- close unrelated tabs and notifications that could confuse announcements;
- agree on whether the facilitator may provide task prompts, while avoiding hints about the expected answer;
- confirm that the tester can stop the session at any time.

## Tester task script

Read the task prompts without describing where controls are located. Record the tester's words or a concise paraphrase in the observation fields.

| Step | Tester task | Result and observation |
|---|---|---|
| 1 | Open `web/pilot-ui/?mode=demo`. | |
| 2 | Identify whether the page is demo/fixture/read-only. | |
| 3 | Start the facilitator walkthrough. | |
| 4 | Complete Standing. | |
| 5 | Complete Action Cards. | |
| 6 | Complete Review Preview. | |
| 7 | Complete the receipt/evidence explanation. | |
| 8 | Explain what, if anything, was actually changed. | |
| 9 | Explain whether receipts/evidence were actually created or only explained. | |

Repeat the task at actual 200% browser zoom in the graphical browser. Review both light and dark platform/browser themes. If the surface does not adapt to a theme setting, record that fact and assess the rendered state that is actually available.

## Detailed evidence template

Use the result vocabulary from the minimum-session table. Record direct observations rather than inferred conformance.

| Check | Pass/fail status | Human observation | Repo-safe evidence pointer | Follow-up issue |
|---|---|---|---|---|
| Screen-reader announcement clarity | | | | |
| Heading and landmark navigation | | | | |
| Button names, disabled state, and current-step state | | | | |
| Live-region/status update timing and clarity | | | | |
| Focus order and visible focus | | | | |
| No keyboard trap or required sighted mouse use | | | | |
| Actual 200% zoom and reflow | | | | |
| Light/dark theme and contrast/high-contrast mode | | | | |
| Cognitive and plain-language clarity | | | | |
| Fixture/read-only/non-mutating boundary clarity | | | | |
| Receipt/evidence explanation versus creation boundary | | | | |
| Blockers | | | | |
| Follow-up issues | | | | |

### Step-by-step notes

#### Initial page and walkthrough start

- What was announced on load?
- Could the tester identify the main navigation, walkthrough region, and current demo boundary?
- How did the tester find and activate “Start rehearsal: Standing”?
- Notes:

#### Standing

- Was the destination heading announced?
- Were fixture identity and standing understandable without technical detail?
- Could the tester continue without sighted mouse help?
- Notes:

#### Action Cards

- Were card names, authority, required action, risk, date boundary, and expected-receipt language understandable?
- Were technical values secondary to plain-language content?
- Could the tester continue without sighted mouse help?
- Notes:

#### Review Preview

- Were row selection, selected state, disabled review choices, and wrapper/row statuses announced clearly?
- Was it clear that inspecting choices records nothing?
- Could the tester continue without sighted mouse help?
- Notes:

#### Receipt/evidence explanation

- Was the expanded explanation and focus change announced?
- Could the tester distinguish expected evidence categories from issued receipts or exported evidence?
- Notes:

### Tester summary in their own words

- What did this walkthrough let you do?
- What changed as a result of the walkthrough?
- Were any receipts or evidence packets created?
- What was confusing or difficult?
- What would you change first?

## Recommended later sessions

These are recommended after the minimum first session; they do not become implied results of that first record.

- VoiceOver with Safari on macOS
- TalkBack with Chrome on Android
- mobile touch exploration and coarse-pointer use on physical hardware
- switch-control scanning and activation
- voice-control naming and activation
- low-vision review, including magnification and high-contrast preferences
- cognitive-comprehension review with minimal facilitator prompting
- translation/localization review with expanded or translated copy
- additional screen-reader/browser combinations, including NVDA with both Firefox and Chrome

## Session disposition

| Field | Recorded value |
|---|---|
| Minimum-session outcome (Pass / Pass with follow-up / Fail / Blocked / Not run) | |
| Critical blockers | |
| Follow-up issue URLs | |
| Tester confirmation or anonymized sign-off | |
| Facilitator/reviewer | |
| Date recorded | |

The session disposition summarizes this one record. It does not promote the surface to a readiness state or replace the twelve-category [organizer/member accessibility gate](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md).

## Nonclaims

- A completed template is one human AT evidence record, not full accessibility completion.
- The template itself is not human AT evidence.
- Browser-assisted, Playwright, and axe tests do not replace human AT testing.
- This does not make the surface organizer-ready, member-ready, pilot-ready, production-ready, or live-federated.
- This does not complete #2041.
- This does not complete Phase 2.
- This does not complete #1727 or #1746.
- This does not claim #2082 or #2113 completion.
- This does not add mutation, persistence, assignment persistence, DID binding, receipt creation, evidence export, or live sync.
- The target surface remains fixture-backed, read-only, demo-mode, non-mutating, and L2 evidence only.
