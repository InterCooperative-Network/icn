/* ICN Member Shell — v0 reference client.
 *
 * Dependency-free vanilla JS. No framework, no build step, no storage.
 *
 * Contract sources (do not invent beyond these):
 *  - docs/spec/member-shell-v0.md          (rendering contract, closed vocabulary)
 *  - docs/adr/ADR-0027-action-card-contract.md (ActionCard schema)
 *  - docs/adr/ADR-0020-*                   (/me/standing read model)
 *  - docs/adr/ADR-0028-*                   (accessibility baseline)
 *  - icn/apps/governance/src/http/models.rs (StandingResponse, ActionCardsResponse)
 *
 * Two modes:
 *  - ?mode=demo  → renders the committed pilot-ui fixture pack. No network
 *                  beyond static file fetches. Nothing signed, nothing sent.
 *  - live (default) → connect panel against a locally running gateway.
 *                  Dev rehearsal only; not production.
 *
 * The access credential is held in a function-scoped variable for the life
 * of the page only. It is never written to localStorage, sessionStorage,
 * cookies, or the URL.
 */
(function () {
  "use strict";

  // ---------------------------------------------------------------------
  // i18n seam (icn#2042). Every member-facing string is externalized into
  // the ICNI18n catalog (i18n.js, loaded before this file) and resolved via
  // t(). The catalog holds the text; this file holds only keys and logic.
  // t() never throws and never returns blank — at worst it returns the key.
  // ---------------------------------------------------------------------
  // Degrade gracefully if i18n.js failed to load (wrong serve root, cache, or
  // partial deploy): a no-op shim so the shell still renders (showing keys)
  // instead of throwing before any content appears.
  var I18N = window.ICNI18n || {
    t: function (key) { return key; },
    locale: "en",
    resolveLocale: function () { return "en"; },
    applyDocumentLocale: function () {},
    availableLocales: function () { return ["en"]; },
    localeMeta: function () { return { name: "English", dir: "ltr" }; }
  };
  function t(key, params) { return I18N.t(key, params); }

  // ---------------------------------------------------------------------
  // Closed member-facing vocabulary (docs/spec/member-shell-v0.md
  // §"Member-facing status vocabulary"). The English values are verbatim in
  // the `en` catalog; inventing new ones is a v0 violation. These maps are
  // key maps — their displayed text comes from the catalog via t().
  // ---------------------------------------------------------------------
  var SYNC = {
    SYNCED: "sync.synced",
    DELAYED: "sync.delayed",
    VERIFYING: "sync.verifying",
    PAUSED: "sync.paused",
    RECEIPT: "sync.receipt",
    REVIEW: "sync.review",
    DEGRADED: "sync.degraded"
  };

  var LIFECYCLE = {
    OPEN: "lifecycle.open",
    OPEN_PAUSED: "lifecycle.openPaused",
    SENT: "lifecycle.sent",
    CONFIRMED: "lifecycle.confirmed",
    DECLINED: "lifecycle.declined",
    CLOSED_DEADLINE: "lifecycle.closedDeadline",
    CLOSED_SUPERSEDED: "lifecycle.closedSuperseded",
    CLOSED_AUTHORITY: "lifecycle.closedAuthority"
  };

  // Plain-language maps for the closed ADR-0027 enums. The raw enum value
  // is never the primary surface (spec §"ActionCard rendering contract").
  // Values are catalog keys; displayed text comes from t().
  var ACTION_KIND_LABEL = {
    vote: "action.vote",
    attend: "action.attend",
    complete: "action.complete"
  };
  var SOURCE_KIND_LABEL = {
    proposal: "source.proposal",
    meeting: "source.meeting",
    action_item: "source.actionItem",
    // Reserved source paths (icn#1631 / icn#1634): render as inert
    // "available soon" placeholders, never as live cards.
    signal_rule: "source.availableSoon",
    obligation_lifecycle: "source.availableSoon"
  };
  var SCOPE_LABEL = {
    entity: "scope.entity",
    structure: "scope.structure",
    individual: "scope.individual"
  };
  // Glyphs (○ ◐ ▲) stay in code, never translated. Text comes from t().
  var RISK_LABEL = {
    low: { glyph: "○", text: "risk.low" },        // ○
    normal: { glyph: "◐", text: "risk.normal" },  // ◐
    elevated: { glyph: "▲", text: "risk.elevated" } // ▲
  };

  // ---------------------------------------------------------------------
  // #1726 pending-publish review-preview panel. Closed, fail-closed enums from
  // urn:icn:contract:pending-publish-summary:v1, served by
  // GET /v1/gov/me/pending-publish-summary (icn#1728). Values are catalog keys;
  // displayed text comes from t(). An unknown enum value renders its raw string
  // (never coerced to a guess), matching the server's fail-closed posture.
  // ---------------------------------------------------------------------
  var PP_ORIGIN_LABEL = {
    committed_fixture: { glyph: "●", tone: "neutral", text: "pp.origin.committedFixture" },
    live_runtime: { glyph: "◆", tone: "ok", text: "pp.origin.liveRuntime" },
    // #2386: the /me/pending-publish-summary read model can now report a
    // membership-scoped rehearsal workspace on a Rehearsal-mode node.
    rehearsal_runtime: { glyph: "◆", tone: "ok", text: "pp.origin.rehearsalRuntime" }
  };
  var PP_KIND_LABEL = {
    action_item: "pp.kind.actionItem",
    decision: "pp.kind.decision",
    attendance: "pp.kind.attendance",
    obligation: "pp.kind.obligation",
    allocation: "pp.kind.allocation",
    settlement: "pp.kind.settlement",
    evidence_note: "pp.kind.evidenceNote",
    risk_note: "pp.kind.riskNote"
  };
  // Glyphs stay in code, never translated. Tone + text carry the meaning so
  // status is never conveyed by color alone. Each status is rendered distinctly
  // (human review states vs a policy/authority rejection are not collapsed).
  var PP_STATUS_LABEL = {
    pending_review: { glyph: "◔", tone: "neutral", text: "pp.status.pendingReview" },
    approved_for_publish: { glyph: "✓", tone: "ok", text: "pp.status.approvedForPublish" },
    rejected: { glyph: "✗", tone: "warn", text: "pp.status.rejected" },
    needs_edit: { glyph: "✎", tone: "warn", text: "pp.status.needsEdit" },
    needs_more_info: { glyph: "?", tone: "warn", text: "pp.status.needsMoreInfo" }
  };
  var PP_PROVENANCE_LABEL = {
    committed_fixture: "pp.prov.committedFixture",
    meeting_record: "pp.prov.meetingRecord",
    governance_record: "pp.prov.governanceRecord",
    example_snippet: "pp.prov.exampleSnippet",
    repo_safe_paste: "pp.prov.repoSafePaste",
    prior_evidence_packet: "pp.prov.priorEvidencePacket"
  };
  var PP_RECEIPT_LABEL = {
    governance_receipt: "pp.receipt.governance",
    attendance_receipt: "pp.receipt.attendance",
    action_item_completion_receipt: "pp.receipt.actionItemCompletion",
    settlement_receipt: "pp.receipt.settlement",
    none: "pp.receipt.none"
  };

  // ---------------------------------------------------------------------
  // #2386 organizer workflow enums. Closed values from the Rehearsal-mode
  // runtime (docs/contracts/rehearsal-review-workflow.md). Values are catalog
  // keys; an unknown value renders its raw string (fail-closed). The raw enum
  // value is placed in a data-* attribute for tests, never in organizer copy.
  // ---------------------------------------------------------------------
  var ORG_REVIEW_DECISIONS = [
    { value: "approve", key: "organizer.review.approve", primary: true },
    { value: "reject", key: "organizer.review.reject", primary: false },
    { value: "needs_edit", key: "organizer.review.needsEdit", primary: false },
    { value: "needs_more_info", key: "organizer.review.needsMoreInfo", primary: false }
  ];
  var ORG_OUTCOME_LABEL = {
    "executed": "organizer.outcome.executed",
    "interrupted-execution": "organizer.outcome.interruptedExecution",
    "rejected": "organizer.outcome.rejected",
    "edit-and-resubmit": "organizer.outcome.editAndResubmit",
    "approved-not-executed": "organizer.outcome.approvedNotExecuted",
    "deferred": "organizer.outcome.deferred"
  };
  var ORG_RECEIPT_CLASS_LABEL = {
    process_session_opened: "organizer.receipts.class.processSessionOpened",
    decision_recorded: "organizer.receipts.class.decisionRecorded",
    process_gate_result: "organizer.receipts.class.processGateResult",
    activation_crossed: "organizer.receipts.class.activationCrossed",
    mutation_plan_recorded: "organizer.receipts.class.mutationPlanRecorded",
    mutation_applied: "organizer.receipts.class.mutationApplied"
  };

  // ---------------------------------------------------------------------
  // Mode + page state
  // ---------------------------------------------------------------------
  var params = new URLSearchParams(window.location.search);
  var MODE = params.get("mode") === "demo" ? "demo" : "live";

  // #2289 organizer-steward evidence surface. `?mode=demo&set=process-evidence`
  // swaps the demo pack for a fixture-only, read-only evidence story over the
  // eight already-landed ADR-0026 Layer 2 process-transition receipts plus the
  // ninth, export-boundary EvidencePacketExportPreparedReceipt (session
  // opened -> deliberation entry recorded -> decision recorded -> gate result
  // -> activation crossed -> mutation plan recorded -> mutation applied ->
  // evidence packet produced -> evidence packet export prepared) plus a
  // repo-safe evidence-summary export.
  // Fixture/dev only: nothing is live, every hash is illustrative (see the
  // demo hash label), and the surface renders read views only — no download,
  // no mutation. `set` is demo-only.
  var SET = MODE === "demo" ? params.get("set") : null;

  // #2386 organizer rehearsal review→confirm surface. `?surface=organizer` is
  // honoured ONLY in live mode: the workflow requires a running Rehearsal-mode
  // node and a review/confirm credential. There is deliberately no fixture
  // "organizer demo" — a surface that appeared to confirm work would be fake
  // success — so demo mode keeps its non-mutating guarantee and SURFACE is null
  // there. See docs/design/ORGANIZER_REHEARSAL_WORKFLOW_WIREFRAME.md.
  var SURFACE = MODE === "live" && params.get("surface") === "organizer" ? "organizer" : null;

  // DEV/DEMO one-click launcher context. The local launcher
  // (deploy/appliance/scripts/open-proxmox-demo.sh) opens this page with
  // ?demo=launcher and forwards two loopback ports: the gateway to :18080 and
  // a DEV/DEMO session endpoint to :18091. When present, the shell shows a
  // "Start local demo" button that obtains a fresh session in one click — no
  // gateway typing, no credential paste, no credential in any URL. The
  // credential is held in page memory exactly like the manual flow (never
  // persisted). The flag is a UI hint only and carries no secret.
  var DEMO_LAUNCHER = MODE === "live" && params.get("demo") === "launcher";
  // The shell is served from one of two very different origins, and the
  // launcher targets must follow the origin — a loopback literal is only
  // correct when the PAGE itself is on loopback:
  //   * loopback (assembled-appliance tunnels / smoke-local.sh): the gateway
  //     and session endpoint are SSH/hostfwd-forwarded to host ports
  //     (defaults 18080/18091, overridable via ?gw / ?session), and
  //   * a LAN single-origin deployment (appliance LAN profile): one reverse
  //     proxy serves this page AND forwards /v1/* to the gateway and
  //     /v1/dev/demo/session to the VM-loopback session endpoint. There the
  //     only correct target is the page's own origin — "localhost" would
  //     point the browser at the VIEWER'S machine, not the appliance.
  var PAGE_ON_LOOPBACK = window.location.hostname === "127.0.0.1" ||
    window.location.hostname === "localhost";
  var DEMO_LOOPBACK = window.location.hostname === "127.0.0.1" ? "127.0.0.1" : "localhost";
  // The launcher forwards the gateway and session endpoint to host ports and
  // passes them as ?gw / ?session so operator port overrides
  // (ICN_DEMO_GW_PORT / ICN_DEMO_SESSION_PORT) are honored. Digits only — a
  // non-numeric value falls back to the default and can never inject into the
  // URL we build. Port overrides are loopback-tunnel concepts; on a LAN
  // origin both targets are same-origin paths behind the reverse proxy.
  function demoPort(name, fallback) {
    var v = params.get(name);
    return v && /^[0-9]{1,5}$/.test(v) ? v : fallback;
  }
  var DEMO_GATEWAY = PAGE_ON_LOOPBACK
    ? "http://" + DEMO_LOOPBACK + ":" + demoPort("gw", "18080")
    : window.location.origin;
  var DEMO_SESSION_URL = (PAGE_ON_LOOPBACK
    ? "http://" + DEMO_LOOPBACK + ":" + demoPort("session", "18091")
    : window.location.origin) + "/v1/dev/demo/session";
  // ?fresh=1 (organizer launcher only): ask the session endpoint to start a
  // NEW rehearsal generation before minting the organizer session. Reset is
  // an organizer act with retire-not-erase semantics (prior fictional items
  // are cancelled unless already completed; recorded receipts remain
  // permanent process facts). The flag is a UI intent only — the endpoint
  // maps it to a fixed command; nothing here carries a secret.
  var DEMO_FRESH = DEMO_LAUNCHER && SURFACE === "organizer" && params.get("fresh") === "1";

  var state = {
    gateway: null,     // live mode only; validated http(s) origin string
    credential: null,  // live mode only; never persisted
    standing: null,    // StandingResponse
    cards: null,       // ActionCardsResponse
    receipts: []       // rendered receipt objects {receipt, plainContext}
  };

  // #2386 organizer workflow state — one object (not scattered booleans),
  // guarded by a monotonic orgSeq so an abandoned response never renders into a
  // newer connection/domain. Row-level preview staleness is additionally caught
  // by the (rowId, version) checks in the preview/confirm handlers.
  var org = {
    standing: null,     // organizer's own StandingResponse
    eligible: [],       // domains the caller is a member of
    domain: null,       // selected { domain_id, domain_name }
    generation: null,   // workspace generation
    rows: [],           // listing rows for the selected domain
    bindings: [],       // [{ label, bound }] — never a DID
    selectedRowId: null,
    detail: null,       // GET .../{row}          → { row, assignee_bound }
    preview: null,      // GET .../{row}/preview   → carries preview_digest
    result: null,       // POST .../{row}/confirm  → ladder ids + hashes
    receipts: null,     // GET .../receipts
    evidence: null,     // GET .../evidence-export
    confirming: false,  // the explicit confirm screen is showing
    busy: false,        // a review/edit/assign/preview/confirm is in flight
    pendingStatus: null // one-shot status message to show after a re-render
  };
  var orgSeq = 0;

  // Fixture paths are relative to this page. They resolve only when the
  // serve root is web/ (see README): /member-shell/.. → /pilot-ui/...
  var FIXTURES = {
    standing: "../pilot-ui/fixtures/icn-organizer-demo/standing.json",
    cards: "../pilot-ui/fixtures/icn-organizer-demo/action-cards.json",
    // member-shell-local receipt fixture (the pilot-ui pack carries no
    // receipt packet); wire-shaped per icn-governance proof.rs
    // ActionItemCompletionReceipt.
    completionReceipt: "fixtures/demo-completion-receipt.json",
    // member-shell-local pending-publish fixture in the RUNTIME response shape
    // (PendingPublishSummaryResponse), so demo mode renders the same shape the
    // live GET /v1/gov/me/pending-publish-summary endpoint serves. Fictional
    // rehearsal data (origin=committed_fixture); never live participant state.
    pendingPublish: "fixtures/pending-publish-summary.json"
  };

  // #2084 Community Proof Spine 0.1 — fixture/dev mirror of the civic loop.
  // `?mode=demo&set=community` swaps the demo pack for a community-shaped
  // walkthrough: belonging -> standing -> action card -> authorized transition
  // -> receipt -> plain-language explanation. Fixture/dev only — nothing is live,
  // the receipt hash is illustrative (see the receipt's demo hash label), and this
  // models a community as an icn-entity Community entity, not a live community
  // domain. See docs/spec/community-proof-spine-0.1.md.
  if (MODE === "demo" && params.get("set") === "community") {
    FIXTURES.standing = "fixtures/community-standing.json";
    FIXTURES.cards = "fixtures/community-action-cards.json";
    FIXTURES.completionReceipt = "fixtures/community-completion-receipt.json";
  }

  // #2289 organizer-steward evidence surface (fixture-only). Keeps the demo
  // standing + cards pack; adds the nine-receipt process-evidence sequence and
  // its repo-safe evidence-summary export (both member-shell-local fixtures).
  if (SET === "process-evidence") {
    FIXTURES.processEvidence = "fixtures/process-evidence-receipts.json";
    FIXTURES.processEvidenceExport = "fixtures/process-evidence-export.json";
  }

  // ---------------------------------------------------------------------
  // Small DOM helpers — element construction only, no innerHTML with
  // dynamic data, so fixture/gateway strings can never inject markup.
  // ---------------------------------------------------------------------
  function el(tag, attrs, children) {
    var node = document.createElement(tag);
    if (attrs) {
      Object.keys(attrs).forEach(function (k) {
        if (k === "text") { node.textContent = attrs[k]; }
        else if (k === "class") { node.className = attrs[k]; }
        else { node.setAttribute(k, attrs[k]); }
      });
    }
    (children || []).forEach(function (c) { node.appendChild(c); });
    return node;
  }
  function byId(id) { return document.getElementById(id); }
  function show(id) { byId(id).hidden = false; }
  function hide(id) { byId(id).hidden = true; }
  function clear(node) { while (node.firstChild) { node.removeChild(node.firstChild); } }

  function kvRow(dl, term, value) {
    dl.appendChild(el("dt", { text: term }));
    if (typeof value === "string") {
      dl.appendChild(el("dd", { text: value }));
    } else {
      var dd = el("dd");
      dd.appendChild(value);
      dl.appendChild(dd);
    }
  }

  // ---------------------------------------------------------------------
  // Formatting helpers
  // ---------------------------------------------------------------------
  function fmtAbs(unixSeconds) {
    if (typeof unixSeconds !== "number") { return t("time.noTimestamp"); }
    return new Date(unixSeconds * 1000).toLocaleString();
  }

  // Relative time, anchored to an explicit reference point so the demo
  // fixture (a frozen snapshot) renders honestly against its own
  // generated_at rather than pretending the snapshot is current.
  function fmtRel(targetSec, anchorSec) {
    var delta = targetSec - anchorSec;
    var abs = Math.abs(delta);
    var unit, n;
    if (abs >= 86400) { n = Math.round(abs / 86400); unit = t(n === 1 ? "time.day" : "time.days"); }
    else if (abs >= 3600) { n = Math.round(abs / 3600); unit = t(n === 1 ? "time.hour" : "time.hours"); }
    else { n = Math.max(1, Math.round(abs / 60)); unit = t(n === 1 ? "time.minute" : "time.minutes"); }
    return delta >= 0
      ? t("time.closesIn", { n: n, unit: unit })
      : t("time.deadlinePassed", { n: n, unit: unit });
  }

  function hashToHex(recordHash) {
    // Wire form is a 32-byte array (icn-governance proof.rs `Hash = [u8; 32]`).
    if (Array.isArray(recordHash)) {
      return recordHash.map(function (b) {
        return ("0" + (b & 0xff).toString(16)).slice(-2);
      }).join("");
    }
    if (typeof recordHash === "string") { return recordHash; }
    return t("hash.unavailable");
  }

  // text is already-resolved display text (callers pass t(...) results or
  // composed strings). The glyph stays in code; tone drives the chip class.
  function setSyncChip(text, tone, detail) {
    var chip = byId("sync-chip");
    var glyph = tone === "ok" ? "✓ " : tone === "warn" ? "⚠ " : "● ";
    chip.textContent = glyph + text;
    chip.className = "chip " + (tone || "neutral");
    byId("sync-detail").textContent = detail || "";
  }

  // ---------------------------------------------------------------------
  // Identity + standing rendering (ADR-0020 read model)
  // ---------------------------------------------------------------------
  function renderIdentity(standing) {
    var label = standing.display_label || t("identity.noDisplayName");
    byId("identity-label").textContent = label;
    byId("identity-did").textContent = standing.did || t("identity.unknownDid");
    show("identity-section");
  }

  function membershipPlain(domain) {
    if (domain.status === "member") {
      return t("membership.member");
    }
    if (domain.status === "unverified") {
      return t("membership.unverified");
    }
    return t("membership.other", { status: String(domain.status) });
  }

  function renderStanding(standing) {
    var domainsList = byId("domains-list");
    clear(domainsList);
    if (!standing.domains || standing.domains.length === 0) {
      domainsList.appendChild(el("li", { text: t("standing.noMemberships") }));
    } else {
      standing.domains.forEach(function (d) {
        var li = el("li");
        li.appendChild(el("h3", { text: d.domain_name || d.domain_id }));
        li.appendChild(el("p", { text: membershipPlain(d) }));
        var details = el("details");
        details.appendChild(el("summary", { text: t("standing.showTechnical") }));
        var dl = el("dl", { class: "kv" });
        kvRow(dl, t("standing.kv.domainId"), d.domain_id || "");
        kvRow(dl, t("standing.kv.membershipSource"), d.membership_source || "");
        kvRow(dl, t("standing.kv.status"), d.status || "");
        details.appendChild(dl);
        li.appendChild(details);
        domainsList.appendChild(li);
      });
    }

    var rolesList = byId("roles-list");
    clear(rolesList);
    if (!standing.roles || standing.roles.length === 0) {
      rolesList.appendChild(el("li", { text: t("standing.noRoles") }));
    } else {
      standing.roles.forEach(function (r) {
        var li = el("li");
        var structureName = r.structure_name ||
          t("standing.structureNameUnavailable", { id: r.structure_id });
        li.appendChild(el("h3", {
          text: t("standing.roleHeading", { role: capitalize(r.role), structure: structureName })
        }));
        li.appendChild(el("p", {
          text: t("standing.roleAuthorizes", {
            scopes: (r.authority_scope && r.authority_scope.length
              ? r.authority_scope.map(scopePlain).join("; ")
              : t("standing.roleNoScopes"))
          })
        }));
        li.appendChild(el("p", {
          class: "muted",
          text: r.end_date
            ? t("standing.heldSinceEnds", { start: fmtAbs(r.start_date), end: fmtAbs(r.end_date) })
            : t("standing.heldSince", { start: fmtAbs(r.start_date) })
        }));
        var details = el("details");
        details.appendChild(el("summary", { text: t("standing.showTechnical") }));
        var dl = el("dl", { class: "kv" });
        kvRow(dl, t("standing.kv.roleAssignmentId"), r.role_assignment_id || "");
        kvRow(dl, t("standing.kv.structureId"), r.structure_id || "");
        kvRow(dl, t("standing.kv.parentEntity"), r.parent_entity_id || t("standing.kv.parentEntityNone"));
        kvRow(dl, t("standing.kv.authorityScopeStrings"), (r.authority_scope || []).join(", "));
        details.appendChild(dl);
        li.appendChild(details);
        rolesList.appendChild(li);
      });
    }
    show("standing-section");
  }

  // Turn an authority-scope string like "program_review" into readable
  // words. This is a presentation aid only; the raw string stays available
  // under "Show technical detail".
  function scopePlain(scope) {
    return String(scope).replace(/[_:]/g, " ");
  }
  function capitalize(s) {
    s = String(s || "");
    return s.charAt(0).toUpperCase() + s.slice(1);
  }

  // ---------------------------------------------------------------------
  // Action cards (ADR-0027 rendering contract)
  // ---------------------------------------------------------------------
  function authorityCheck(card, standing) {
    var held = (standing && standing.authority_scopes) || [];
    var required = card.required_authority_scope || [];
    var missing = required.filter(function (s) { return held.indexOf(s) === -1; });
    return { authorized: missing.length === 0, missing: missing };
  }

  function renderCards(cardsResponse, standing, anchorSec) {
    var list = byId("cards-list");
    clear(list);
    var cards = (cardsResponse && cardsResponse.cards) || [];
    if (cards.length === 0) {
      var li = el("li");
      li.appendChild(el("p", { text: t("cards.none") }));
      if (MODE === "live") {
        var again = el("button", { class: "secondary", text: t("cards.checkAgain") });
        again.addEventListener("click", function () { loadLive(); });
        li.appendChild(again);
      }
      list.appendChild(li);
      show("cards-section");
      return;
    }

    cards.forEach(function (card) {
      list.appendChild(renderCard(card, standing, anchorSec));
    });
    show("cards-section");
  }

  function renderCard(card, standing, anchorSec) {
    var li = el("li");
    var auth = authorityCheck(card, standing);
    var reserved = card.source_kind === "signal_rule" ||
      card.source_kind === "obligation_lifecycle";
    // Assigned-task completion is authorized by the gateway at request time
    // (token scope + creator/assignee + domain membership), not by the
    // role-derived scopes flattened into /me/standing. Do not gate it on
    // the client; the node's accept/reject answer is rendered honestly.
    var assignedCompletion = card.source_kind === "action_item" &&
      card.action_kind === "complete";

    li.appendChild(el("h3", { text: card.title || t("card.untitled") }));

    // accessibility_hint renders as plain-language preamble BEFORE any
    // decision controls (ADR-0028 / spec rendering table).
    if (card.accessibility_hint) {
      li.appendChild(el("p", { class: "muted", text: t("card.beforeYouDecide", { hint: card.accessibility_hint }) }));
    }

    li.appendChild(el("p", { text: card.summary || "" }));

    // Lifecycle status chip (closed vocabulary). This client derives only
    // states it can honestly know: reserved → inert; missing authority →
    // Closed — insufficient authority; otherwise Open. Mutation flow moves
    // a card to Sent/Confirmed below.
    // A card whose deadline is already behind the anchor is closed no matter
    // what authority says (spec: "Closed — deadline passed").
    var expired = typeof card.deadline === "number" && card.deadline < anchorSec;
    var actionable = !expired && (auth.authorized || assignedCompletion);
    var statusKey = reserved ? SOURCE_KIND_LABEL[card.source_kind]
      : expired ? LIFECYCLE.CLOSED_DEADLINE
      : actionable ? LIFECYCLE.OPEN
      : LIFECYCLE.CLOSED_AUTHORITY;
    var status = t(statusKey);
    var chipTone = reserved ? "neutral" : actionable ? "ok" : "warn";
    var chipGlyph = reserved ? "● " : actionable ? "✓ " : "⚠ ";
    var statusChip = el("span", { class: "chip " + chipTone, role: "status", text: chipGlyph + status });
    li.appendChild(el("p", {}, [statusChip]));

    var dl = el("dl", { class: "kv" });
    kvRow(dl, t("card.kv.askedToDo"),
      ACTION_KIND_LABEL[card.action_kind]
        ? t(ACTION_KIND_LABEL[card.action_kind])
        : t("card.action.fallback", { action: String(card.action_kind) }));
    kvRow(dl, t("card.kv.whereFrom"),
      SOURCE_KIND_LABEL[card.source_kind]
        ? t(SOURCE_KIND_LABEL[card.source_kind])
        : String(card.source_kind));
    // The member must see WHICH domain this card acts in before confirming,
    // not just the constitutional axis (spec: "What scope am I acting in?").
    // standing.domains carries the human-readable name for the card's domain.
    var domainName = String(card.domain_id);
    var domains = (standing && standing.domains) || [];
    for (var di = 0; di < domains.length; di++) {
      if (domains[di].domain_id === card.domain_id) {
        domainName = (domains[di].domain_name || String(card.domain_id));
        break;
      }
    }
    kvRow(dl, t("card.kv.whereApplies"), domainName);
    kvRow(dl, t("card.kv.scope"),
      SCOPE_LABEL[card.scope] ? t(SCOPE_LABEL[card.scope]) : String(card.scope));
    kvRow(dl, t("card.kv.whyYouCanAct"), card.authority_basis || t("card.noAuthorityBasis"));

    if (auth.authorized) {
      kvRow(dl, t("card.kv.authorization"), t("card.auth.authorized"));
    } else if (assignedCompletion) {
      kvRow(dl, t("card.kv.authorization"), t("card.auth.assignedCompletion"));
    } else {
      kvRow(dl, t("card.kv.authorization"),
        t("card.auth.insufficient", { missing: auth.missing.map(scopePlain).join("; ") }));
    }

    if (typeof card.deadline === "number") {
      var rel = fmtRel(card.deadline, anchorSec);
      var dd = el("span", { text: rel + " " });
      var det = el("details");
      det.appendChild(el("summary", { text: t("card.exactTime") }));
      det.appendChild(el("p", { text: t("card.localTime", { time: fmtAbs(card.deadline) }) }));
      var wrap = el("span");
      wrap.appendChild(dd);
      wrap.appendChild(det);
      kvRow(dl, t("card.kv.timePressure"), wrap);
    } else {
      // Spec: absent deadline renders as "no time pressure", not "no deadline".
      kvRow(dl, t("card.kv.timePressure"), t("card.timePressure.none"));
    }

    var risk = RISK_LABEL[card.risk_level]
      ? { glyph: RISK_LABEL[card.risk_level].glyph, text: t(RISK_LABEL[card.risk_level].text) }
      : { glyph: "●", text: String(card.risk_level) };
    kvRow(dl, t("card.kv.careLevel"), risk.glyph + " " + risk.text);

    kvRow(dl, t("card.kv.whatHappens"),
      card.receipt_expected
        ? t("card.whatHappens.receipt")
        : t("card.whatHappens.noReceipt"));
    li.appendChild(dl);

    // Technical identifiers live under "details", never as primary surface.
    var details = el("details");
    details.appendChild(el("summary", { text: t("card.showTechnical") }));
    var tdl = el("dl", { class: "kv" });
    kvRow(tdl, t("card.kv.cardId"), card.id || "");
    kvRow(tdl, t("card.kv.underlyingRecordId"), card.source_id || "");
    kvRow(tdl, t("card.kv.domainId"), card.domain_id || t("card.domainIdNotCarried"));
    kvRow(tdl, t("card.kv.rawKind"),
      [card.source_kind, card.action_kind, card.scope, card.risk_level].join(" / "));
    details.appendChild(tdl);
    li.appendChild(details);

    // The one mutating action this v0 client implements (live mode only):
    // mark an assigned task complete, then fetch and render its completion
    // receipt. Everything else is read-only.
    if (MODE === "live" && !reserved && assignedCompletion && !expired) {
      li.appendChild(buildCompleteFlow(card, statusChip));
    }
    return li;
  }

  // ---------------------------------------------------------------------
  // #1726 pending-publish review-preview panel. Read-only projection of
  // GET /v1/gov/me/pending-publish-summary (icn#1728). It renders the rows a
  // reviewer would see BEFORE anything is recorded or published. This surface
  // has NO decision controls and issues NO write — it records nothing. The same
  // renderer serves both a demo committed-fixture response and a live gateway
  // response; a live_runtime response with no rows renders an honest empty
  // state.
  // ---------------------------------------------------------------------
  function renderPendingPublish(response) {
    var list = byId("pending-publish-list");
    clear(list);
    var origin = (response && response.origin) || "live_runtime";
    var rows = (response && response.rows) || [];

    // Origin chip: committed_fixture (fictional rehearsal data) vs live_runtime.
    // role="status" announces it; glyph + text, never color alone.
    var originMap = PP_ORIGIN_LABEL[origin];
    var originChip = byId("pending-publish-origin");
    originChip.className = "chip " + (originMap ? originMap.tone : "neutral");
    originChip.textContent = (originMap ? originMap.glyph : "●") + " " +
      (originMap ? t(originMap.text) : String(origin));

    if (rows.length === 0) {
      // Honest empty state. live_runtime serves no rows today; an empty
      // committed_fixture is an empty rehearsal set.
      var li = el("li");
      li.appendChild(el("p", {
        text: origin === "committed_fixture" ? t("pp.empty.fixture") : t("pp.empty.live")
      }));
      list.appendChild(li);
      show("pending-publish-section");
      return;
    }

    rows.forEach(function (row) { list.appendChild(renderPendingRow(row)); });
    show("pending-publish-section");
  }

  function renderPendingRow(row) {
    var li = el("li");
    // Kind + plain summary lead. Kind is a plain label, never the raw enum.
    var kind = PP_KIND_LABEL[row.kind] ? t(PP_KIND_LABEL[row.kind]) : String(row.kind);
    li.appendChild(el("h3", { text: kind }));

    // accessibility_hint as plain-language preamble BEFORE the detail (ADR-0028).
    if (row.accessibility_hint) {
      li.appendChild(el("p", { class: "muted", text: t("pp.beforeYouRead", { hint: row.accessibility_hint }) }));
    }

    li.appendChild(el("p", { text: row.plain_summary || "" }));

    // Review-status chip (closed vocabulary). Glyph + tone + text; never color
    // alone. Fail-closed: an unknown status renders its raw value, not a guess.
    var st = PP_STATUS_LABEL[row.status];
    var statusText = st ? t(st.text) : String(row.status);
    var statusChip = el("span", {
      class: "chip " + (st ? st.tone : "neutral"),
      role: "status",
      text: (st ? st.glyph : "●") + " " + statusText
    });
    li.appendChild(el("p", {}, [statusChip]));

    var dl = el("dl", { class: "kv" });
    kvRow(dl, t("pp.kv.whereApplies"), row.target_scope_label || "");
    kvRow(dl, t("pp.kv.governingBody"), row.governing_body_label || "");
    if (row.assignee_label) {
      // The assignee is an organizer-readable label, NOT a DID. Identity
      // binding is a separate private step and is never surfaced here.
      var aWrap = el("span");
      aWrap.appendChild(el("span", { text: row.assignee_label + " " }));
      var aDet = el("details");
      aDet.appendChild(el("summary", { text: t("pp.assignee.whatIsThis") }));
      aDet.appendChild(el("p", { text: t("pp.assignee.note") }));
      aWrap.appendChild(aDet);
      kvRow(dl, t("pp.kv.assignee"), aWrap);
    }
    kvRow(dl, t("pp.kv.whyProposed"), row.authority_basis || t("pp.noAuthorityBasis"));

    var risk = RISK_LABEL[row.risk_level]
      ? { glyph: RISK_LABEL[row.risk_level].glyph, text: t(RISK_LABEL[row.risk_level].text) }
      : { glyph: "●", text: String(row.risk_level) };
    kvRow(dl, t("pp.kv.careLevel"), risk.glyph + " " + risk.text);

    // Expected evidence (receipt) — an expectation for the reviewer, NOT
    // authority, and NOT yet issued.
    var rc = row.receipt_expected || {};
    if (rc.expected) {
      var cat = PP_RECEIPT_LABEL[rc.category] ? t(PP_RECEIPT_LABEL[rc.category]) : String(rc.category);
      kvRow(dl, t("pp.kv.expectedEvidence"), t("pp.receipt.expected", { category: cat }));
    } else {
      kvRow(dl, t("pp.kv.expectedEvidence"), t("pp.receipt.notExpected"));
    }
    li.appendChild(dl);

    // Technical identifiers under "details", never the primary surface.
    var details = el("details");
    details.appendChild(el("summary", { text: t("pp.showTechnical") }));
    var tdl = el("dl", { class: "kv" });
    kvRow(tdl, t("pp.kv.rowId"), row.id || "");
    kvRow(tdl, t("pp.kv.provenance"),
      PP_PROVENANCE_LABEL[row.source_provenance]
        ? t(PP_PROVENANCE_LABEL[row.source_provenance])
        : String(row.source_provenance));
    kvRow(tdl, t("pp.kv.rawKind"), [row.kind, row.status, row.risk_level].join(" / "));
    details.appendChild(tdl);
    li.appendChild(details);

    return li;
  }

  // ---------------------------------------------------------------------
  // The single mutation: complete an action item (live mode).
  // PUT /v1/gov/domains/{domain_id}/action-items/{item_id}/status
  //   body {"status": "completed"}
  //   scopes: governance:action-item:complete (completion-only) OR governance:meeting:write OR governance:write
  //   (caller must also be the item's creator or assignee and a domain
  //   member — the gateway enforces this; we render its answer honestly.)
  // Then GET .../completion-receipt (scope governance:read).
  // ---------------------------------------------------------------------
  // Note: completion is never scope-gated client-side. /me/standing only
  // flattens role-assignment scopes, so an ordinary assignee may not show
  // the card's required_authority_scope even though the gateway will accept
  // the request (token scope + creator/assignee + domain membership). The
  // gateway authorizes the actual PUT; a refusal is rendered with its
  // reason by the rejection path below.
  function buildCompleteFlow(card, statusChip) {
    var holder = el("div");

    if (!card.domain_id) {
      holder.appendChild(el("p", {
        class: "muted",
        text: t("complete.noDomainRef")
      }));
      return holder;
    }

    var openBtn = el("button", { text: t("complete.openButton") });
    var panel = el("div", { class: "confirm-panel", hidden: "hidden" });

    // Pre-confirm summary (spec §"Signing / confirmation flow"): authority
    // basis, scope, consequence, receipt expected, reversibility honesty.
    panel.appendChild(el("h4", { text: t("complete.beforeConfirm") }));
    var pdl = el("dl", { class: "kv" });
    kvRow(pdl, t("complete.kv.whatChanges"), t("complete.whatChanges.body"));
    kvRow(pdl, t("complete.kv.whyYouMay"), card.authority_basis || t("card.noAuthorityBasis"));
    kvRow(pdl, t("complete.kv.scope"),
      SCOPE_LABEL[card.scope] ? t(SCOPE_LABEL[card.scope]) : String(card.scope));
    kvRow(pdl, t("complete.kv.receipt"), card.receipt_expected
      ? t("complete.receipt.expected")
      : t("complete.receipt.notExpected"));
    kvRow(pdl, t("complete.kv.canUndo"), t("complete.canUndo.body"));
    panel.appendChild(pdl);

    var statusLine = el("p", { role: "status", "aria-live": "polite" });

    var confirmBtn = el("button", { text: t("complete.confirmButton") });
    var cancelBtn = el("button", { class: "secondary", text: t("complete.cancelButton") });
    var actions = el("div", { class: "actions" }, [confirmBtn, cancelBtn]);
    panel.appendChild(actions);
    panel.appendChild(statusLine);

    openBtn.addEventListener("click", function () {
      panel.hidden = false;
      openBtn.hidden = true;
      confirmBtn.focus();
    });
    cancelBtn.addEventListener("click", function () {
      panel.hidden = true;
      openBtn.hidden = false;
      openBtn.focus();
    });

    confirmBtn.addEventListener("click", function () {
      // Bind this flow to the connect attempt it started under. If a newer
      // attempt (possibly another member) takes the view while the PUT or
      // receipt GET is in flight, the callbacks must not render into it.
      var flowSeq = liveLoadSeq;
      confirmBtn.disabled = true;
      cancelBtn.disabled = true;
      statusLine.textContent = t(LIFECYCLE.SENT);
      statusChip.textContent = "● " + t(LIFECYCLE.SENT);
      statusChip.className = "chip neutral";

      liveFetch("/v1/gov/domains/" + encodeURIComponent(card.domain_id) +
                "/action-items/" + encodeURIComponent(card.source_id) + "/status", {
        method: "PUT",
        body: JSON.stringify({ status: "completed" })
      }).then(function () {
        if (flowSeq !== liveLoadSeq) { return null; }  // view changed hands; do not fetch under the new credential
        statusLine.textContent = t("complete.sentFetching", { sent: t(LIFECYCLE.SENT) });
        // Past this point the completion is COMMITTED on the node. A failure
        // to read the receipt back must never be reported as "nothing was
        // recorded" — that would be the shell lying about a recorded action.
        return liveFetch("/v1/gov/domains/" + encodeURIComponent(card.domain_id) +
                         "/action-items/" + encodeURIComponent(card.source_id) +
                         "/completion-receipt", { method: "GET" })
          .then(function (receipt) {
            if (flowSeq !== liveLoadSeq) { return; }  // never render one member's receipt in another's view
            statusChip.textContent = "✓ " + t(LIFECYCLE.CONFIRMED);
            statusChip.className = "chip ok";
            statusLine.textContent = t("complete.confirmedSeeReceipts", {
              confirmed: t(LIFECYCLE.CONFIRMED), receipt: t(SYNC.RECEIPT)
            });
            addReceipt(receipt, t("complete.receiptContext"));
            setSyncChip(t(SYNC.RECEIPT), "ok",
              t("complete.syncDetail", { time: new Date().toLocaleString() }));
          })
          .catch(function (err) {
            if (flowSeq !== liveLoadSeq) { return; }
            // Receipt read failed AFTER the completion was accepted: keep the
            // committed state, do not re-enable Confirm, say exactly what is
            // and is not known.
            statusChip.textContent = "● " + t(LIFECYCLE.SENT);
            statusChip.className = "chip neutral";
            statusLine.textContent = t("complete.receiptReadFailed", { error: err.message });
            cancelBtn.textContent = t("complete.closeButton");
            cancelBtn.disabled = false;
          });
      }).catch(function (err) {
        // The completion itself was refused — the gateway's answer, rendered
        // plainly, never a silent failure. Nothing was recorded.
        if (flowSeq !== liveLoadSeq) { return; }
        statusLine.textContent = t("complete.rejected", { error: err.message });
        statusChip.textContent = "⚠ " + t(LIFECYCLE.OPEN);
        statusChip.className = "chip warn";
        confirmBtn.disabled = false;
        cancelBtn.disabled = false;
      });
    });

    holder.appendChild(openBtn);
    holder.appendChild(panel);
    return holder;
  }

  // ---------------------------------------------------------------------
  // Receipts (plain summary first; formal record under a disclosure)
  // ---------------------------------------------------------------------
  // opts is optional. Existing callers (live completion + demo completion) pass
  // no opts, so the entry keeps its original {receipt, plainContext} shape and
  // renders through renderCompletionReceipt exactly as before. The #2289
  // process-evidence pack passes opts.kind (one of the ten process/evidence
  // classes) plus optional redaction metadata, routing to renderProcessReceipt.
  // No existing behavior changes.
  function addReceipt(receipt, plainContext, opts) {
    var entry = { receipt: receipt, plainContext: plainContext };
    if (opts) {
      entry.kind = opts.kind;
      entry.memberVisibility = opts.memberVisibility;
      entry.stewardSummary = opts.stewardSummary;
      entry.redactionReason = opts.redactionReason;
    }
    state.receipts.push(entry);
    renderReceipts();
  }

  function renderReceipts() {
    var list = byId("receipts-list");
    clear(list);
    if (state.receipts.length === 0) {
      list.appendChild(el("li", { text: t("receipts.none") }));
    } else {
      state.receipts.forEach(function (entry) {
        list.appendChild(entry.kind
          ? renderProcessReceipt(entry)
          : renderCompletionReceipt(entry.receipt, entry.plainContext));
      });
    }
    show("receipts-section");
  }

  // Renders an ActionItemCompletionReceipt (icn-governance proof.rs):
  // { item_id, domain_id, actor_did, transition, completed_at, record_hash }.
  function renderCompletionReceipt(receipt, plainContext) {
    var li = el("li");
    var isSelf = state.standing && receipt.actor_did === state.standing.did;
    var who = isSelf ? t("receipt.who.self") : t("receipt.who.other");

    li.appendChild(el("h3", { text: t("receipt.actionCompleted") }));
    li.appendChild(el("p", {}, [
      el("span", { class: "chip ok", text: "✓ " + t(SYNC.RECEIPT) })
    ]));
    li.appendChild(el("p", {
      text: t("receipt.markedComplete", { who: who, when: fmtAbs(receipt.completed_at) })
    }));
    li.appendChild(el("p", {
      class: "muted",
      text: t("receipt.whatThisProves", { context: (plainContext || "") })
    }));

    var details = el("details");
    details.appendChild(el("summary", { text: t("receipt.showEvidence") }));
    var dl = el("dl", { class: "kv" });
    // "ActionItemCompletionReceipt" is a record class name — not translated.
    kvRow(dl, t("receipt.kv.recordClass"), "ActionItemCompletionReceipt");
    kvRow(dl, t("receipt.kv.taskId"), String(receipt.item_id || ""));
    kvRow(dl, t("receipt.kv.domainId"), String(receipt.domain_id || ""));
    kvRow(dl, t("receipt.kv.actor"), el("code", { text: String(receipt.actor_did || "") }));
    kvRow(dl, t("receipt.kv.transition"), String(receipt.transition || ""));
    kvRow(dl, t("receipt.kv.completedAt"), String(receipt.completed_at || ""));
    // Maturity-tier honesty: the demo fixture's hash is illustrative only —
    // it is not a real blake3 binding and nothing is signed. Only live mode
    // may claim the canonical binding.
    kvRow(dl, MODE === "demo"
        ? t("receipt.kv.recordHashDemo")
        : t("receipt.kv.recordHashLive"),
      el("code", { text: hashToHex(receipt.record_hash) }));
    details.appendChild(dl);
    li.appendChild(details);
    return li;
  }

  // ---------------------------------------------------------------------
  // #2289 organizer-steward evidence surface (fixture-only, read-only).
  // Renders one of the ten process/evidence receipts — the eight ADR-0026
  // Layer 2 process-transition classes plus the export-boundary
  // EvidencePacketExportPreparedReceipt and the availability-boundary
  // EvidencePacketMadeAvailableReceipt — from proof.rs as a plain-language
  // summary first, with the record-level fields
  // under a progressive-disclosure "Show evidence detail" control. record_hash
  // is the proof pointer; body_hash is labeled proof-of-content (the body is
  // never stored). In demo mode the hashes are illustrative, mirroring the
  // completion receipt's honesty label. No readiness is claimed.
  // ---------------------------------------------------------------------
  var PROCESS_RECEIPT_CLASS = {
    process_session_opened: "ProcessSessionOpenedReceipt",
    deliberation_entry_recorded: "DeliberationEntryRecordedReceipt",
    decision_recorded: "DecisionRecordedReceipt",
    process_gate_result: "ProcessGateResultReceipt",
    activation_crossed: "ActivationCrossedReceipt",
    mutation_plan_recorded: "MutationPlanRecordedReceipt",
    mutation_applied: "MutationAppliedReceipt",
    evidence_packet_produced: "EvidencePacketProducedReceipt",
    evidence_packet_export_prepared: "EvidencePacketExportPreparedReceipt",
    evidence_packet_made_available: "EvidencePacketMadeAvailableReceipt"
  };

  // record_hash label mirrors renderCompletionReceipt's maturity-tier honesty:
  // demo hashes are illustrative, only live may claim the canonical binding.
  function recordHashRow(dl, hash) {
    kvRow(dl, MODE === "demo"
        ? t("receipt.kv.recordHashDemo")
        : t("receipt.kv.recordHashLive"),
      el("code", { text: hashToHex(hash) }));
  }

  function renderProcessReceipt(entry) {
    var r = entry.receipt || {};
    var kind = entry.kind;
    var li = el("li");

    li.appendChild(el("h3", { text: t("evidence." + kind + ".heading") }));
    li.appendChild(el("p", {}, [
      el("span", { class: "chip ok", text: "✓ " + t(SYNC.RECEIPT) })
    ]));
    // plainContext is a self-contained plain-language summary (the fixture
    // supplies it per receipt); shown before any raw field per gate §3.11.
    li.appendChild(el("p", { class: "muted", text: entry.plainContext || "" }));

    // Deliberation-entry redaction demo (gate §3.11): show the steward-body
    // view and the member/export view together so the privacy boundary is
    // legible without leaking any private text. The steward summary is
    // clearly-fictional fixture context; the receipt itself holds only a
    // body_hash, so the member/export view can honestly show the proof pointer
    // and the redaction reason and nothing else.
    if (kind === "deliberation_entry_recorded" && entry.memberVisibility === "redacted") {
      var red = el("div", { class: "redaction" });
      red.appendChild(el("h4", { text: t("evidence.redaction.stewardHeading") }));
      red.appendChild(el("p", { class: "muted", text: entry.stewardSummary || "" }));
      red.appendChild(el("h4", { text: t("evidence.redaction.memberHeading") }));
      red.appendChild(el("p", {
        text: t("evidence.redaction.notice", { reason: (entry.redactionReason || "") })
      }));
      li.appendChild(red);
    }

    // Activation-boundary explainer (icn#2297): make the boundary legible to a
    // non-technical steward/member by naming the three states plainly — the
    // recorded decision, the activation crossing itself, and the later
    // mutation-planning work that remains deferred. Activation is not the
    // mutation; the receipt records a process fact and grants zero authority.
    if (kind === "activation_crossed") {
      var boundary = el("div", { class: "boundary" });
      boundary.appendChild(el("h4", { text: t("evidence.activation.boundaryHeading") }));
      var bul = el("ul", { class: "card-list" });
      bul.appendChild(el("li", { text: t("evidence.activation.boundary.decision") }));
      bul.appendChild(el("li", { text: t("evidence.activation.boundary.crossing") }));
      bul.appendChild(el("li", { text: t("evidence.activation.boundary.deferred") }));
      boundary.appendChild(bul);
      li.appendChild(boundary);
    }

    // Mutation-plan-recorded explainer (icn#2304): make the plan-recording
    // boundary legible by naming the states plainly — the recorded decision,
    // the activation crossing, the mutation plan being recorded here, and the
    // two later steps that remain deferred (mutation application and
    // evidence-packet production). Recording the plan is not applying it; the
    // receipt carries only a body-hash fingerprint and grants zero authority.
    if (kind === "mutation_plan_recorded") {
      var planBoundary = el("div", { class: "boundary" });
      planBoundary.appendChild(el("h4", { text: t("evidence.mutationPlan.boundaryHeading") }));
      var pbul = el("ul", { class: "card-list" });
      pbul.appendChild(el("li", { text: t("evidence.mutationPlan.boundary.decision") }));
      pbul.appendChild(el("li", { text: t("evidence.mutationPlan.boundary.activation") }));
      pbul.appendChild(el("li", { text: t("evidence.mutationPlan.boundary.planRecorded") }));
      pbul.appendChild(el("li", { text: t("evidence.mutationPlan.boundary.applicationDeferred") }));
      pbul.appendChild(el("li", { text: t("evidence.mutationPlan.boundary.evidencePacketDeferred") }));
      planBoundary.appendChild(pbul);
      li.appendChild(planBoundary);
    }

    // Mutation-applied explainer (icn#2311): make the applied boundary legible
    // by naming the states plainly — the recorded decision, the activation
    // crossing, the mutation plan recorded, the plan's application recorded
    // here, and the later evidence-packet production that remains deferred.
    // Recording the application is not executing/authorizing/validating/
    // enforcing/rolling-back/proving the mutation; the receipt carries only a
    // result-hash fingerprint and grants zero authority.
    if (kind === "mutation_applied") {
      var appliedBoundary = el("div", { class: "boundary" });
      appliedBoundary.appendChild(el("h4", { text: t("evidence.mutationApplied.boundaryHeading") }));
      var abul = el("ul", { class: "card-list" });
      abul.appendChild(el("li", { text: t("evidence.mutationApplied.boundary.decision") }));
      abul.appendChild(el("li", { text: t("evidence.mutationApplied.boundary.activation") }));
      abul.appendChild(el("li", { text: t("evidence.mutationApplied.boundary.planRecorded") }));
      abul.appendChild(el("li", { text: t("evidence.mutationApplied.boundary.applied") }));
      abul.appendChild(el("li", { text: t("evidence.mutationApplied.boundary.evidencePacketDeferred") }));
      appliedBoundary.appendChild(abul);
      li.appendChild(appliedBoundary);
    }

    // Evidence-packet-produced explainer (icn#2319): make the terminal evidence
    // boundary legible by naming the states plainly — the recorded decision,
    // the activation crossing, the mutation plan recorded, the plan's
    // application recorded, and the evidence packet's production recorded here.
    // Producing a packet is not delivering, accepting, auditing, or certifying
    // it; the receipt carries fingerprints only (packet, source set, redaction
    // profile) and grants zero authority.
    if (kind === "evidence_packet_produced") {
      var packetBoundary = el("div", { class: "boundary" });
      packetBoundary.appendChild(el("h4", { text: t("evidence.evidencePacket.boundaryHeading") }));
      var ebul = el("ul", { class: "card-list" });
      ebul.appendChild(el("li", { text: t("evidence.evidencePacket.boundary.decision") }));
      ebul.appendChild(el("li", { text: t("evidence.evidencePacket.boundary.activation") }));
      ebul.appendChild(el("li", { text: t("evidence.evidencePacket.boundary.planRecorded") }));
      ebul.appendChild(el("li", { text: t("evidence.evidencePacket.boundary.applied") }));
      ebul.appendChild(el("li", { text: t("evidence.evidencePacket.boundary.producedHere") }));
      ebul.appendChild(el("li", { text: t("evidence.evidencePacket.boundary.notDelivered") }));
      packetBoundary.appendChild(ebul);
      li.appendChild(packetBoundary);
    }

    // Evidence-packet-export-prepared explainer (icn#2327): make the export
    // boundary legible by naming the states plainly — the produced packet, the
    // export preparation recorded here for a named recipient scope, and the
    // stacked negations (prepared is not made-available / delivered / received /
    // accepted / audited / certified). Preparing an export is not sending it;
    // this surface assembles no export artifact, carries no access / vault /
    // custody / retrieval meaning, and grants zero authority. This is the ninth
    // receipt class — the first beyond the eight process-transition classes —
    // and does not complete any #1748 acceptance gate.
    if (kind === "evidence_packet_export_prepared") {
      var exportBoundary = el("div", { class: "boundary" });
      exportBoundary.appendChild(el("h4", { text: t("evidence.exportPrepared.boundaryHeading") }));
      var xbul = el("ul", { class: "card-list" });
      xbul.appendChild(el("li", { text: t("evidence.exportPrepared.boundary.produced") }));
      xbul.appendChild(el("li", { text: t("evidence.exportPrepared.boundary.preparedHere") }));
      xbul.appendChild(el("li", { text: t("evidence.exportPrepared.boundary.notDelivered") }));
      xbul.appendChild(el("li", { text: t("evidence.exportPrepared.boundary.noAuthority") }));
      exportBoundary.appendChild(xbul);
      li.appendChild(exportBoundary);
    }

    // Evidence-packet-made-available explainer (icn#2334): make the availability
    // boundary legible by naming the states plainly — the prepared export, the
    // availability recorded here to the same recipient scope under a disclosure
    // policy, and the stacked negations (made available is not retrieved /
    // accessed / delivered / received / accepted / audited / certified). Making
    // an export available is not sending or opening it; this surface assembles,
    // fetches, and delivers nothing, carries no URL / endpoint / token / vault /
    // location meaning, and grants zero authority (who may access is a separate
    // authority decision, not asserted here). This is the tenth receipt class
    // and does not complete any #1748 acceptance gate.
    if (kind === "evidence_packet_made_available") {
      var availBoundary = el("div", { class: "boundary" });
      availBoundary.appendChild(el("h4", { text: t("evidence.madeAvailable.boundaryHeading") }));
      var abul = el("ul", { class: "card-list" });
      abul.appendChild(el("li", { text: t("evidence.madeAvailable.boundary.exportPrepared") }));
      abul.appendChild(el("li", { text: t("evidence.madeAvailable.boundary.availableHere") }));
      abul.appendChild(el("li", { text: t("evidence.madeAvailable.boundary.notAccessed") }));
      abul.appendChild(el("li", { text: t("evidence.madeAvailable.boundary.noAuthority") }));
      availBoundary.appendChild(abul);
      li.appendChild(availBoundary);
    }

    var details = el("details");
    details.appendChild(el("summary", { text: t("receipt.showEvidence") }));
    var dl = el("dl", { class: "kv" });
    // Record class names are not translated (they are wire identifiers).
    kvRow(dl, t("receipt.kv.recordClass"), PROCESS_RECEIPT_CLASS[kind] || String(kind));
    kvRow(dl, t("evidence.kv.domainId"), String(r.domain_id || ""));
    kvRow(dl, t("evidence.kv.sessionId"), String(r.session_id || ""));

    if (kind === "process_session_opened") {
      kvRow(dl, t("evidence.kv.openedBy"), el("code", { text: String(r.opened_by || "") }));
      kvRow(dl, t("evidence.kv.recordedAt"), String(r.opened_at || ""));
    } else if (kind === "deliberation_entry_recorded") {
      kvRow(dl, t("evidence.kv.entryId"), String(r.entry_id || ""));
      kvRow(dl, t("evidence.kv.author"), el("code", { text: String(r.author || "") }));
      kvRow(dl, t("evidence.kv.entryKind"), String(r.entry_kind || ""));
      kvRow(dl, t("evidence.kv.recordedAt"), String(r.recorded_at || ""));
      kvRow(dl, t("evidence.kv.bodyHash"), el("code", { text: hashToHex(r.body_hash) }));
    } else if (kind === "decision_recorded") {
      kvRow(dl, t("evidence.kv.decisionId"), String(r.decision_id || ""));
      kvRow(dl, t("evidence.kv.recordedBy"), el("code", { text: String(r.recorded_by || "") }));
      kvRow(dl, t("evidence.kv.recordedAt"), String(r.recorded_at || ""));
      kvRow(dl, t("evidence.kv.bodyHash"), el("code", { text: hashToHex(r.body_hash) }));
    } else if (kind === "process_gate_result") {
      kvRow(dl, t("evidence.kv.gateKind"), String(r.gate_kind || ""));
      kvRow(dl, t("evidence.kv.gateResult"), String(r.result || ""));
      kvRow(dl, t("evidence.kv.recordedBy"), el("code", { text: String(r.recorded_by || "") }));
      kvRow(dl, t("evidence.kv.recordedAt"), String(r.recorded_at || ""));
    } else if (kind === "activation_crossed") {
      kvRow(dl, t("evidence.kv.activationId"), String(r.activation_id || ""));
      // Decision reference (icn#2297 / #2295 B1): the crossing names the
      // activated decision by both its caller-opaque id and its
      // content-addressed record hash. Both are proof pointers, not body text.
      kvRow(dl, t("evidence.kv.decisionRef"), String(r.decision_id || ""));
      kvRow(dl, t("evidence.kv.decisionRecordHash"),
        el("code", { text: hashToHex(r.decision_record_hash) }));
      // Gate basis (#2295 B2): a fingerprint over the passed gate-result
      // record hashes. The receipt carries only the fingerprint; the declared
      // source hashes below are display-only fixture context.
      kvRow(dl, t("evidence.kv.gateBasis"), el("code", { text: hashToHex(r.gate_basis) }));
      (entry.declaredGateBasis || []).forEach(function (g) {
        kvRow(dl,
          t("evidence.kv.declaredGate", {
            kind: String(g.gate_kind || ""), result: String(g.result || "")
          }),
          el("code", { text: hashToHex(g.record_hash) }));
      });
      kvRow(dl, t("evidence.kv.crossedBy"), el("code", { text: String(r.crossed_by || "") }));
      kvRow(dl, t("evidence.kv.recordedAt"), String(r.recorded_at || ""));
    } else if (kind === "mutation_plan_recorded") {
      kvRow(dl, t("evidence.kv.planId"), String(r.plan_id || ""));
      // Activation reference (icn#2304 / M1): the plan names the activation it
      // follows by both its caller-opaque id and its content-addressed record
      // hash. Both are proof pointers to the activation crossing above — never
      // body text. Decision and gate basis are inherited transitively through
      // that activation, not restated here.
      kvRow(dl, t("evidence.kv.activationRef"), String(r.activation_id || ""));
      kvRow(dl, t("evidence.kv.activationRecordHash"),
        el("code", { text: hashToHex(r.activation_record_hash) }));
      // Plan proof pointer (M2): a body-hash fingerprint of the plan only —
      // never the plan body, operation list, target list, or effect payload;
      // only this hash is kept.
      kvRow(dl, t("evidence.kv.planBodyHash"), el("code", { text: hashToHex(r.body_hash) }));
      kvRow(dl, t("evidence.kv.planRecordedBy"), el("code", { text: String(r.recorded_by || "") }));
      kvRow(dl, t("evidence.kv.recordedAt"), String(r.recorded_at || ""));
    } else if (kind === "mutation_applied") {
      kvRow(dl, t("evidence.kv.applicationId"), String(r.application_id || ""));
      // Plan reference (icn#2311 / A1): the application names the plan it
      // applies by both its caller-opaque id and its content-addressed record
      // hash. Both are proof pointers to the mutation-plan-recorded receipt
      // above — never body text. Activation, decision, and gate basis are
      // inherited transitively through that plan, not restated here.
      kvRow(dl, t("evidence.kv.planRef"), String(r.plan_id || ""));
      kvRow(dl, t("evidence.kv.planRecordHash"),
        el("code", { text: hashToHex(r.plan_record_hash) }));
      // Result proof pointer (A2): a result-hash fingerprint of the applied
      // result only — never the applied-result body, operation list, target
      // list, or effect payload; only this hash is kept.
      kvRow(dl, t("evidence.kv.resultHash"), el("code", { text: hashToHex(r.result_hash) }));
      kvRow(dl, t("evidence.kv.appliedBy"), el("code", { text: String(r.applied_by || "") }));
      kvRow(dl, t("evidence.kv.appliedAt"), String(r.applied_at || ""));
    } else if (kind === "evidence_packet_produced") {
      kvRow(dl, t("evidence.kv.packetId"), String(r.packet_id || ""));
      // Applied reference (icn#2319 / EP1): the packet names the applied step
      // it draws from by both its caller-opaque id and its content-addressed
      // record hash. Both are proof pointers to the mutation-applied receipt
      // above — never body text. Plan, activation, decision, and gate basis are
      // inherited transitively through that applied step, not restated here.
      kvRow(dl, t("evidence.kv.applicationRef"), String(r.mutation_application_id || ""));
      kvRow(dl, t("evidence.kv.mutationAppliedRecordHash"),
        el("code", { text: hashToHex(r.mutation_applied_record_hash) }));
      // Source-set commitment (EP1/EP2): a fingerprint over the source
      // receipts' record hashes — references only; no source receipt body is
      // ever stored or shown.
      kvRow(dl, t("evidence.kv.receiptSetHash"),
        el("code", { text: hashToHex(r.receipt_set_hash) }));
      // Packet proof pointer (EP2): fingerprints the public/redacted packet
      // artifact only — the packet body is never stored.
      kvRow(dl, t("evidence.kv.packetHash"), el("code", { text: hashToHex(r.packet_hash) }));
      // Redaction boundary (EP3): fingerprints the redaction profile that
      // shaped the public packet — the profile body is never stored.
      kvRow(dl, t("evidence.kv.redactionProfileHash"),
        el("code", { text: hashToHex(r.redaction_profile_hash) }));
      kvRow(dl, t("evidence.kv.producedBy"), el("code", { text: String(r.produced_by || "") }));
      kvRow(dl, t("evidence.kv.producedAt"), String(r.produced_at || ""));
    } else if (kind === "evidence_packet_export_prepared") {
      kvRow(dl, t("evidence.kv.exportId"), String(r.export_id || ""));
      // Produced-packet reference (icn#2327 / EX5): the export names the
      // produced packet it prepares by both its caller-opaque id and its
      // content-addressed record hash. Both are proof pointers to the
      // evidence-packet-produced receipt above — never body text. Applied,
      // plan, activation, decision, and gate provenance are inherited
      // transitively through that produced packet, not restated here.
      kvRow(dl, t("evidence.kv.exportPacketRef"), String(r.packet_id || ""));
      kvRow(dl, t("evidence.kv.packetProducedRecordHash"),
        el("code", { text: hashToHex(r.packet_produced_record_hash) }));
      // Echoed packet fingerprint (EX5): the produced receipt's public/redacted
      // packet_hash, echoed here as a proof link — the packet body is never
      // stored and no delivery / made-available fact is asserted by echoing it.
      kvRow(dl, t("evidence.kv.exportPacketHashEcho"),
        el("code", { text: hashToHex(r.packet_hash) }));
      // Export policy fingerprint (EX5): which export policy shaped this
      // preparation — the policy body is never stored; not a claim the export
      // is authorized, complete, satisfied, or legally sufficient.
      kvRow(dl, t("evidence.kv.exportPolicyHash"),
        el("code", { text: hashToHex(r.export_policy_hash) }));
      // Recipient scope (EX3): a caller-opaque governance handle naming *which*
      // scope this export was prepared for — never a name, email, address, or
      // any personal contact data.
      kvRow(dl, t("evidence.kv.recipientScopeId"), String(r.recipient_scope_id || ""));
      kvRow(dl, t("evidence.kv.preparedBy"), el("code", { text: String(r.prepared_by || "") }));
      kvRow(dl, t("evidence.kv.preparedAt"), String(r.prepared_at || ""));
    } else if (kind === "evidence_packet_made_available") {
      kvRow(dl, t("evidence.kv.availabilityId"), String(r.availability_id || ""));
      // Export-prepared reference (icn#2334 / D4): the availability names the
      // export-prepared receipt it follows by both its caller-opaque id and its
      // content-addressed record hash. Both are proof pointers to the
      // evidence-packet-export-prepared receipt above — never body text. Produced,
      // applied, plan, activation, decision, and gate provenance are inherited
      // transitively through that export-prepared receipt, not restated here.
      kvRow(dl, t("evidence.kv.madeAvailableExportRef"), String(r.export_id || ""));
      kvRow(dl, t("evidence.kv.exportPreparedRecordHash"),
        el("code", { text: hashToHex(r.export_prepared_record_hash) }));
      kvRow(dl, t("evidence.kv.exportPacketRef"), String(r.packet_id || ""));
      // Echoed packet fingerprint (D4): the export-prepared receipt's
      // public/redacted packet_hash, echoed here as a verified proof link — the
      // packet body is never stored and no retrieval / access / delivery fact is
      // asserted by echoing it.
      kvRow(dl, t("evidence.kv.madeAvailablePacketHashEcho"),
        el("code", { text: hashToHex(r.packet_hash) }));
      // Recipient scope (R6): a caller-opaque governance handle naming *which*
      // scope this was made available to — echoed from and verified against the
      // export-prepared receipt; never a name, email, address, or contact data.
      kvRow(dl, t("evidence.kv.madeAvailableRecipientScope"), String(r.recipient_scope_id || ""));
      // Disclosure policy fingerprint (R6): which disclosure policy governs this
      // availability — the policy body is never stored; not a claim access was
      // granted, or that anything is complete, satisfied, or legally sufficient.
      kvRow(dl, t("evidence.kv.disclosurePolicyHash"),
        el("code", { text: hashToHex(r.disclosure_policy_hash) }));
      // Availability method fingerprint (R6): which method made this available —
      // a fingerprint only, never a URL, endpoint, retrieval token, vault path,
      // location, or contact detail; the method descriptor is never stored.
      kvRow(dl, t("evidence.kv.availabilityMethodHash"),
        el("code", { text: hashToHex(r.availability_method_hash) }));
      kvRow(dl, t("evidence.kv.madeAvailableBy"), el("code", { text: String(r.made_available_by || "") }));
      kvRow(dl, t("evidence.kv.madeAvailableAt"), String(r.made_available_at || ""));
    }
    recordHashRow(dl, r.record_hash);
    details.appendChild(dl);
    li.appendChild(details);
    return li;
  }

  // Render the repo-safe evidence-summary export (conforms to
  // urn:icn:contract:rehearsal-evidence-export:v1) as a READ-ONLY view of the
  // committed fixture. The surface never generates, downloads, mutates, or
  // copies an export — the committed JSON is the single source of truth.
  function renderEvidenceExport(exp) {
    exp = exp || {};
    var body = byId("evidence-export-body");
    clear(body);

    body.appendChild(el("p", {
      text: t("evidence.export.summary", {
        mode: String(exp.rehearsal_mode || ""),
        safety: String(exp.export_safety_classification || "")
      })
    }));
    body.appendChild(el("p", { class: "muted", text: t("evidence.export.readonly") }));

    var outcomes = exp.decision_outcomes || [];
    if (outcomes.length) {
      body.appendChild(el("h3", { text: t("evidence.export.outcomesHeading") }));
      var oul = el("ul", { class: "card-list" });
      outcomes.forEach(function (o) {
        oul.appendChild(el("li", { text: String(o.plain_summary || "") }));
      });
      body.appendChild(oul);
    }

    if (exp.privacy_review) {
      body.appendChild(el("p", {
        text: t("evidence.export.privacy", {
          status: String(exp.privacy_review.status || ""),
          notes: String(exp.privacy_review.notes || "")
        })
      }));
    }

    var details = el("details");
    details.appendChild(el("summary", { text: t("evidence.export.showDetail") }));
    var dl = el("dl", { class: "kv" });
    kvRow(dl, t("evidence.export.kv.contract"), "urn:icn:contract:rehearsal-evidence-export:v1");
    kvRow(dl, t("evidence.export.kv.mode"), String(exp.rehearsal_mode || ""));
    kvRow(dl, t("evidence.export.kv.steps"), (exp.workflow_steps_completed || []).join(", "));
    kvRow(dl, t("evidence.export.kv.audiences"), (exp.audience_categories || []).join(", "));
    kvRow(dl, t("evidence.export.kv.mutation"),
      exp.mutation_boundary && exp.mutation_boundary.executed === false
        ? t("evidence.export.mutationNone")
        : String((exp.mutation_boundary && exp.mutation_boundary.target) || ""));
    kvRow(dl, t("evidence.export.kv.accessibility"),
      exp.accessibility_review ? String(exp.accessibility_review.status || "") : "");
    kvRow(dl, t("evidence.export.kv.safety"), String(exp.export_safety_classification || ""));
    details.appendChild(dl);
    body.appendChild(details);

    var nc = exp.non_claims || [];
    if (nc.length) {
      body.appendChild(el("h3", { text: t("evidence.export.nonClaimsHeading") }));
      var ncul = el("ul", { class: "card-list" });
      nc.forEach(function (c) { ncul.appendChild(el("li", { text: String(c) })); });
      body.appendChild(ncul);
    }

    show("evidence-export-section");
  }

  // ---------------------------------------------------------------------
  // Demo mode: fixture-backed, nothing signed, no live node.
  // ---------------------------------------------------------------------
  function loadDemo() {
    setSyncChip(t(SYNC.VERIFYING), "neutral", t("demo.loadingFixture"));
    if (SET === "process-evidence") { loadProcessEvidenceDemo(); return; }
    Promise.all([
      fetchJson(FIXTURES.standing),
      fetchJson(FIXTURES.cards),
      fetchJson(FIXTURES.completionReceipt),
      // #1726 pending-publish review preview — DEFAULT demo view only. The
      // community (#2084) and process-evidence (#2289) sets are self-contained
      // walkthroughs whose regenerated evidence must not include these generic
      // review rows, so the panel is not loaded there. Resilient: a missing
      // fixture must not fail the core standing/cards render.
      (SET ? Promise.resolve(null)
           : fetchJson(FIXTURES.pendingPublish).catch(function () { return null; }))
    ]).then(function (results) {
      state.standing = results[0];
      state.cards = results[1];
      var anchor = state.cards.generated_at || state.standing.generated_at;

      renderIdentity(state.standing);
      renderStanding(state.standing);
      renderCards(state.cards, state.standing, anchor);
      addReceipt(results[2], t("demo.receiptContext"));
      if (results[3]) { renderPendingPublish(results[3]); }

      setSyncChip(t(SYNC.SYNCED), "ok",
        t("demo.syncedDetail", { when: fmtAbs(anchor) }));
    }).catch(function (err) {
      setSyncChip(t(SYNC.DEGRADED), "warn",
        t("demo.loadFailed", { error: err.message }));
    });
  }

  // #2289 organizer-steward evidence surface (fixture-only). Reuses the demo
  // standing + cards render path, then renders the nine-receipt process
  // sequence and the repo-safe evidence-summary export. No network beyond the
  // committed fixtures; nothing signed; read-only.
  function loadProcessEvidenceDemo() {
    Promise.all([
      fetchJson(FIXTURES.standing),
      fetchJson(FIXTURES.cards),
      fetchJson(FIXTURES.processEvidence),
      fetchJson(FIXTURES.processEvidenceExport)
    ]).then(function (results) {
      state.standing = results[0];
      state.cards = results[1];
      var pack = results[2] || {};
      var exp = results[3];
      var anchor = state.cards.generated_at || state.standing.generated_at;

      renderIdentity(state.standing);
      renderStanding(state.standing);
      renderCards(state.cards, state.standing, anchor);

      (pack.sequence || []).forEach(function (item) {
        addReceipt(item.receipt, item.plainContext, {
          kind: item.kind,
          memberVisibility: item.memberVisibility,
          stewardSummary: item.stewardSummary,
          redactionReason: item.redactionReason
        });
      });
      renderEvidenceExport(exp);

      setSyncChip(t(SYNC.SYNCED), "ok",
        t("demo.syncedDetail", { when: fmtAbs(anchor) }));
    }).catch(function (err) {
      setSyncChip(t(SYNC.DEGRADED), "warn",
        t("demo.loadFailed", { error: err.message }));
    });
  }

  // ---------------------------------------------------------------------
  // Live mode: local gateway, dev rehearsal.
  // ---------------------------------------------------------------------
  function liveFetch(path, opts) {
    opts = opts || {};
    var headers = {
      "Authorization": "Bearer " + state.credential,
      "Accept": "application/json"
    };
    if (opts.body) { headers["Content-Type"] = "application/json"; }
    return fetch(state.gateway + path, {
      method: opts.method || "GET",
      headers: headers,
      body: opts.body || undefined
    }).then(function (resp) {
      if (!resp.ok) {
        return resp.text().then(function (bodyText) {
          var reason;
          try { reason = JSON.parse(bodyText).error || bodyText; }
          catch (e) { reason = bodyText || ("HTTP " + resp.status); }
          // Attach the numeric status so status-aware callers (the organizer
          // workflow) can render distinct 401/403/404/409/422/500 copy. The
          // message is unchanged, so existing callers that read only .message
          // are unaffected.
          var err = new Error(t("live.nodeAnswered", { status: resp.status, reason: String(reason).slice(0, 300) }));
          err.status = resp.status;
          throw err;
        });
      }
      return resp.json();
    });
  }

  // Monotonic load sequence: responses from an abandoned connect attempt
  // must never render over a newer attempt's view (overlapping submits on
  // a shared browser could otherwise mix two members' records).
  var liveLoadSeq = 0;

  function loadLive() {
    var seq = ++liveLoadSeq;
    setSyncChip(t(SYNC.VERIFYING), "neutral", t("live.askingStanding"));
    byId("connect-status").textContent = "";

    // A new connect attempt invalidates whatever member view is on screen.
    // The previous member's identity, standing, cards, and receipts must not
    // remain visible during the load or after a FAILED load — on a shared
    // browser that would expose their records under someone else's attempt.
    // Same-member receipts are restored once the new standing confirms the
    // DID matches.
    var prior = {
      did: state.standing && state.standing.did,
      receipts: state.receipts
    };
    state.standing = null;
    state.cards = null;
    state.receipts = [];
    hide("identity-section");
    hide("standing-section");
    clear(byId("domains-list"));
    clear(byId("roles-list"));
    clear(byId("cards-list"));
    hide("pending-publish-section");
    clear(byId("pending-publish-list"));
    renderReceipts();

    liveFetch("/v1/gov/me/standing").then(function (standing) {
      if (seq !== liveLoadSeq) { return null; }  // a newer attempt owns the view
      if (prior.did && prior.did === standing.did) {
        state.receipts = prior.receipts;
      }
      state.standing = standing;
      renderIdentity(standing);
      renderStanding(standing);
      return liveFetch("/v1/gov/me/action-cards");
    }).then(function (cardsResponse) {
      if (cardsResponse === null || seq !== liveLoadSeq) { return; }
      state.cards = cardsResponse;
      var anchor = Math.floor(Date.now() / 1000);
      renderCards(cardsResponse, state.standing, anchor);
      renderReceipts();
      // #1726 pending-publish review preview. Independent + resilient: a node
      // build without this endpoint (older gateway) must not break the
      // standing/cards view. The seq guard stops a stale attempt rendering
      // over a newer one. GET only — this panel never mutates.
      liveFetch("/v1/gov/me/pending-publish-summary")
        .then(function (pp) { if (seq === liveLoadSeq) { renderPendingPublish(pp); } })
        .catch(function () { /* panel stays hidden; core live view already rendered */ });
      setSyncChip(t(SYNC.SYNCED), "ok",
        t("live.syncedDetail", { time: new Date().toLocaleString() }));
      byId("connect-status").textContent = t("live.connected");
    }).catch(function (err) {
      if (seq !== liveLoadSeq) { return; }  // stale failure must not clobber a newer attempt
      // Failure-table disposition (spec §"Failure and safety table"):
      // plain language first, no error code as primary surface, help path
      // always visible.
      setSyncChip(t(SYNC.DEGRADED), "warn", t("live.standingUnavailable"));
      byId("connect-status").textContent = t("live.couldNotLoad", { error: err.message });
    });
  }

  function wireConnectForm(loader) {
    byId("connect-form").addEventListener("submit", function (ev) {
      ev.preventDefault();
      var url;
      try {
        url = new URL(byId("gateway-url").value);
      } catch (e) {
        byId("connect-status").textContent = t("live.invalidUrl");
        return;
      }
      if (url.protocol !== "http:" && url.protocol !== "https:") {
        byId("connect-status").textContent = t("live.urlScheme");
        return;
      }
      state.gateway = url.origin;
      var credentialInput = byId("credential");
      var credential = credentialInput.value.trim();
      // Accept a pasted full header value ("Bearer <value>") and normalize
      // it to the bare credential.
      credential = credential.replace(/^Bearer\s+/i, "").trim();
      if (!credential) {
        byId("connect-status").textContent = t("live.pasteCredential");
        return;
      }
      state.credential = credential;
      // The credential now lives only in page memory; clear the DOM input
      // so it is not left readable in the field.
      credentialInput.value = "";
      (loader || loadLive)();
    });
  }

  function fetchJson(path) {
    return fetch(path).then(function (resp) {
      if (!resp.ok) { throw new Error("HTTP " + resp.status + " for " + path); }
      return resp.json();
    });
  }

  // =====================================================================
  // #2386 Organizer rehearsal review → confirm surface (live-only).
  // Drives the Rehearsal-mode runtime routes under
  //   /v1/gov/domains/{domain}/rehearsal/*
  // plus the shared /v1/gov/me/standing. The organizer credential holds only
  // governance:read + pending-publish:review + pending-publish:confirm; it never
  // binds a label, never initializes a workspace, and never handles a DID. See
  // docs/design/ORGANIZER_REHEARSAL_WORKFLOW_WIREFRAME.md and
  // docs/contracts/rehearsal-review-workflow.md.
  // =====================================================================
  var ORG_SECTIONS = [
    "organizer-domain-section", "organizer-workspace-section",
    "organizer-receipts-section", "organizer-evidence-section",
    "organizer-member-section"
  ];

  // Point the connect panel's copy at the organizer credential requirements
  // without changing the single, security-sensitive credential-capture path.
  function applyOrganizerConnectCopy() {
    byId("connect-h").textContent = t("organizer.connect.heading");
    byId("connect-body").textContent = t("organizer.connect.body");
    byId("connect-button").textContent = t("organizer.connect.submit");
    var help = byId("credential-help");
    clear(help);
    help.appendChild(document.createTextNode(t("organizer.connect.credentialHelp")));
  }

  // Map a liveFetch error (carrying .status) to plain organizer-facing copy.
  // The raw backend detail is never the primary surface.
  function orgErrorText(err) {
    var s = err && err.status;
    if (s === 401) { return t("organizer.error.401"); }
    if (s === 403) { return t("organizer.error.403"); }
    if (s === 404) { return t("organizer.error.404"); }
    if (s === 409) { return t("organizer.error.409"); }
    if (s === 422) { return t("organizer.error.422"); }
    if (s === 500) { return t("organizer.error.500"); }
    return t("organizer.error.generic");
  }

  function orgDomainPath(suffix) {
    return "/v1/gov/domains/" + encodeURIComponent(org.domain.domain_id) + "/rehearsal" + suffix;
  }

  function orgRowPath(rowId, suffix) {
    return orgDomainPath("/pending-publish/" + encodeURIComponent(rowId) + (suffix || ""));
  }

  // Entry after a successful organizer connect (mirrors loadLive()). A new or
  // failed connect attempt bumps orgSeq and clears any prior organizer's rows
  // BEFORE the fetch, so nothing from a previous connection lingers on screen.
  function loadOrganizer() {
    var seq = ++orgSeq;
    resetOrganizerSurface();
    setSyncChip(t(SYNC.VERIFYING), "neutral", t("live.askingStanding"));
    byId("connect-status").textContent = "";

    liveFetch("/v1/gov/me/standing").then(function (standing) {
      if (seq !== orgSeq) { return; }
      org.standing = standing;
      // Render the organizer's OWN standing (memberships/roles). renderIdentity
      // is deliberately NOT called: it is the only place a DID enters the DOM,
      // and the organizer surface stays DID-free by construction.
      renderStanding(standing);
      org.eligible = (standing.domains || []).filter(function (d) { return d.status === "member"; });
      setSyncChip(t(SYNC.SYNCED), "ok", t("live.syncedDetail", { time: new Date().toLocaleString() }));
      byId("connect-status").textContent = t("live.connected");
      if (org.eligible.length === 0) {
        show("organizer-domain-section");
        clear(byId("organizer-domain-choices"));
        byId("organizer-domain-status").textContent = t("organizer.domain.none");
        byId("organizer-domain-open").disabled = true;
        return;
      }
      if (org.eligible.length === 1) { openWorkspace(org.eligible[0]); return; }
      renderDomainChoices(org.eligible, seq);
    }).catch(function (err) {
      if (seq !== orgSeq) { return; }
      setSyncChip(t(SYNC.DEGRADED), "warn", t("live.standingUnavailable"));
      byId("connect-status").textContent = t("live.couldNotLoad", { error: orgErrorText(err) });
    });
  }

  function resetOrganizerSurface() {
    org.standing = null; org.eligible = []; org.domain = null; org.generation = null;
    org.rows = []; org.bindings = []; org.selectedRowId = null; org.detail = null;
    org.preview = null; org.result = null; org.receipts = null; org.evidence = null;
    org.confirming = false; org.busy = false; org.pendingStatus = null;
    ORG_SECTIONS.forEach(hide);
    clear(byId("organizer-domain-choices"));
    clear(byId("organizer-rows-list"));
    clear(byId("organizer-row-detail"));
    clear(byId("organizer-receipts-list"));
    clear(byId("organizer-evidence-body"));
    byId("organizer-domain-status").textContent = "";
    byId("organizer-workspace-status").textContent = "";
    byId("organizer-workspace-context").textContent = "";
    byId("organizer-action-status").textContent = "";
    byId("organizer-domain-open").disabled = false;
    // The organizer surface never renders the member action-card, read-only
    // pending-publish, DID identity, or member-receipts sections.
    hide("identity-section");
    hide("cards-section");
    hide("pending-publish-section");
    hide("receipts-section");
    // Clear the prior organizer's STANDING too: on a FAILED reconnect the
    // standing is not repopulated, so it must not linger on a shared browser
    // (mirrors loadLive()'s clear-before-fetch discipline for the member view).
    hide("standing-section");
    clear(byId("domains-list"));
    clear(byId("roles-list"));
  }

  function renderDomainChoices(domains, seq) {
    var wrap = byId("organizer-domain-choices");
    clear(wrap);
    domains.forEach(function (d, i) {
      var choice = el("div", { class: "organizer-choice" });
      var id = "org-domain-" + i;
      // Deliberately NOT pre-checked: with more than one eligible domain the
      // organizer must make an explicit choice (the chooseFirst guard below
      // refuses to open until one is selected), so they can never land in the
      // wrong rehearsal workspace by just pressing Open/Enter.
      var radio = el("input", { type: "radio", name: "org-domain", id: id, value: d.domain_id });
      var label = el("label", { for: id,
        text: (d.domain_name || d.domain_id) + " (" + t("organizer.domain.member") + ")" });
      choice.appendChild(radio);
      choice.appendChild(label);
      wrap.appendChild(choice);
    });
    show("organizer-domain-section");
    // onsubmit (not addEventListener) so a re-render never stacks handlers.
    byId("organizer-domain-form").onsubmit = function (ev) {
      ev.preventDefault();
      if (seq !== orgSeq) { return; }
      var checked = wrap.querySelector('input[name="org-domain"]:checked');
      if (!checked) { byId("organizer-domain-status").textContent = t("organizer.domain.chooseFirst"); return; }
      var chosen = domains.filter(function (d) { return d.domain_id === checked.value; })[0];
      if (chosen) { openWorkspace(chosen); }
    };
  }

  // Select a domain and load its workspace. Bumps orgSeq so any in-flight load
  // from a previous domain is dropped, and clears prior rows/preview/result.
  function openWorkspace(domain) {
    var seq = ++orgSeq;
    org.domain = { domain_id: domain.domain_id, domain_name: domain.domain_name || domain.domain_id };
    org.rows = []; org.bindings = []; org.selectedRowId = null; org.detail = null;
    org.preview = null; org.result = null; org.receipts = null; org.evidence = null;
    org.confirming = false;
    hide("organizer-domain-section");
    hide("organizer-receipts-section");
    hide("organizer-evidence-section");
    hide("organizer-member-section");
    clear(byId("organizer-row-detail"));
    clear(byId("organizer-rows-list"));
    byId("organizer-workspace-context").textContent = "";
    byId("organizer-action-status").textContent = "";
    byId("organizer-workspace-status").textContent = t("organizer.workspace.loading");
    show("organizer-workspace-section");
    loadWorkspace(seq);
  }

  function loadWorkspace(seq) {
    liveFetch(orgDomainPath("/pending-publish")).then(function (resp) {
      if (seq !== orgSeq) { return; }
      org.generation = resp.generation;
      org.rows = resp.rows || [];
      byId("organizer-workspace-status").textContent = "";
      byId("organizer-workspace-context").textContent = t("organizer.workspace.context", {
        domain: org.domain.domain_name, generation: String(resp.generation)
      });
      renderOrgRows();
      // Labels + any receipts/evidence for this workspace. Independent and
      // resilient: a failure of any does not break the list.
      refreshOrgBindings(seq);
      loadOrgReceipts(seq);
      loadOrgEvidence(seq);
    }).catch(function (err) {
      if (seq !== orgSeq) { return; }
      if (err && err.status === 404) {
        // Workspace not initialized. Explain; do NOT call reset; keep standing.
        org.rows = [];
        clear(byId("organizer-rows-list"));
        byId("organizer-workspace-context").textContent = "";
        byId("organizer-workspace-status").textContent = "";
        var detail = byId("organizer-row-detail");
        clear(detail);
        detail.appendChild(el("p", { class: "explain boundary", text: t("organizer.workspace.uninitialized") }));
        var again = el("button", { class: "secondary", text: t("organizer.workspace.checkAgain") });
        again.addEventListener("click", function () {
          byId("organizer-workspace-status").textContent = t("organizer.workspace.loading");
          clear(byId("organizer-row-detail"));
          loadWorkspace(seq);
        });
        detail.appendChild(again);
        return;
      }
      byId("organizer-workspace-status").textContent = orgErrorText(err);
    });
  }

  function refreshOrgBindings(seq) {
    liveFetch(orgDomainPath("/bindings")).then(function (resp) {
      if (seq !== orgSeq) { return; }
      org.bindings = (resp && resp.bindings) || [];
      // Re-render the detail ONLY if a row is open and the assign control is not
      // yet showing (labels arrived after the row was opened). Never rebuild once
      // the assign <select> exists — that would clobber an in-progress note/edit
      // and drop focus.
      if (org.selectedRowId && org.detail && !byId("organizer-assign-select")) { renderOrgDetail(); }
    }).catch(function () { /* labels unavailable; the assign control shows none */ });
  }

  function updateOrgListRow(rowId, patch) {
    org.rows.forEach(function (r) {
      if (r.id === rowId) { Object.keys(patch).forEach(function (k) { r[k] = patch[k]; }); }
    });
  }

  function renderOrgRows() {
    var list = byId("organizer-rows-list");
    clear(list);
    if (org.rows.length === 0) {
      list.appendChild(el("li", { text: t("organizer.workspace.empty") }));
      return;
    }
    org.rows.forEach(function (row) {
      var li = el("li");
      var selected = row.id === org.selectedRowId;
      if (selected) { li.className = "selected"; }
      var kind = PP_KIND_LABEL[row.kind] ? t(PP_KIND_LABEL[row.kind]) : String(row.kind);
      li.appendChild(el("h3", { text: kind }));
      li.appendChild(el("p", { text: row.plain_summary || "" }));
      var st = PP_STATUS_LABEL[row.status];
      li.appendChild(el("p", {}, [ el("span", {
        class: "chip " + (st ? st.tone : "neutral"),
        text: (st ? st.glyph : "●") + " " + (st ? t(st.text) : String(row.status))
      }) ]));
      if (row.assignee_label) {
        li.appendChild(el("p", { class: "muted", text: t("pp.kv.assignee") + ": " + row.assignee_label }));
      }
      if (row.executed) {
        li.appendChild(el("p", { class: "muted", text: t("organizer.detail.executed") }));
      }
      var btn = el("button", { text: selected ? t("organizer.row.selected") : t("organizer.row.reviewButton") });
      btn.setAttribute("data-row-id", row.id);
      btn.disabled = selected;
      btn.addEventListener("click", function () {
        if (org.busy) { return; }   // never switch rows mid-mutation
        selectOrgRow(row.id);
      });
      li.appendChild(btn);
      list.appendChild(li);
    });
  }

  function selectOrgRow(rowId) {
    var seq = orgSeq;
    org.selectedRowId = rowId;
    org.preview = null; org.result = null; org.confirming = false;
    setOrgStatus("");
    renderOrgRows();
    var detail = byId("organizer-row-detail");
    clear(detail);
    detail.appendChild(el("p", { class: "muted", text: t("organizer.workspace.loading") }));
    liveFetch(orgRowPath(rowId)).then(function (resp) {
      if (seq !== orgSeq || org.selectedRowId !== rowId) { return; }
      org.detail = resp;
      renderOrgDetail();
      focusById("organizer-detail-h");
    }).catch(function (err) {
      if (seq !== orgSeq || org.selectedRowId !== rowId) { return; }
      clear(detail);
      setOrgStatus(orgErrorText(err));
    });
  }

  function focusById(id) {
    var node = byId(id);
    if (node) { node.setAttribute("tabindex", "-1"); node.focus(); }
  }

  function setOrgControlsDisabled(disabled) {
    var controls = byId("organizer-row-detail").querySelectorAll("button, input, textarea, select");
    Array.prototype.forEach.call(controls, function (c) { c.disabled = disabled; });
  }

  function appendBackToList(host) {
    var back = el("button", { class: "secondary", text: t("organizer.detail.backToList") });
    back.addEventListener("click", function () {
      if (org.busy) { return; }
      org.selectedRowId = null; org.detail = null; org.preview = null;
      org.result = null; org.confirming = false;
      clear(byId("organizer-row-detail"));
      setOrgStatus("");
      renderOrgRows();
    });
    host.appendChild(back);
  }

  // The action-status live region is a PERSISTENT node (index.html), so it exists
  // in the a11y tree before its text changes and screen readers announce outcomes.
  function orgStatusNode() { return byId("organizer-action-status"); }
  function setOrgStatus(text) { orgStatusNode().textContent = text; }
  function flushOrgPendingStatus() {
    if (org.pendingStatus) { setOrgStatus(org.pendingStatus); org.pendingStatus = null; }
  }

  // Rebuild the selected-row detail pane from org.detail (+ preview/result). The
  // persistent action-status node is NOT rebuilt here.
  function renderOrgDetail() {
    var host = byId("organizer-row-detail");
    clear(host);
    if (!org.detail || !org.detail.row) { return; }
    var row = org.detail.row;
    var executed = !!row.executed;

    var kind = PP_KIND_LABEL[row.kind] ? t(PP_KIND_LABEL[row.kind]) : String(row.kind);
    host.appendChild(el("h3", { id: "organizer-detail-h", text: t("organizer.detail.heading", { kind: kind }) }));
    host.appendChild(el("p", { class: "muted", text: t("organizer.detail.version", { version: String(row.version) }) }));
    host.appendChild(el("p", { text: row.plain_summary || "" }));
    var st = PP_STATUS_LABEL[row.status];
    host.appendChild(el("p", {}, [ el("span", {
      class: "chip " + (st ? st.tone : "neutral"),
      text: (st ? st.glyph : "●") + " " + (st ? t(st.text) : String(row.status))
    }) ]));
    var dl = el("dl", { class: "kv" });
    kvRow(dl, t("pp.kv.whereApplies"), row.target_scope_label || "");
    kvRow(dl, t("pp.kv.governingBody"), row.governing_body_label || "");
    kvRow(dl, t("pp.kv.whyProposed"), row.authority_basis || t("pp.noAuthorityBasis"));
    var rk = RISK_LABEL[row.risk_level];
    kvRow(dl, t("pp.kv.careLevel"), rk ? (rk.glyph + " " + t(rk.text)) : ("● " + String(row.risk_level)));
    if (row.note) { kvRow(dl, t("organizer.review.noteLabel"), row.note); }
    host.appendChild(dl);

    if (executed) {
      host.appendChild(el("p", { class: "explain boundary", text: t("organizer.detail.executed") }));
      renderOrgResultInto(host, row);
      appendBackToList(host);
      flushOrgPendingStatus();
      return;
    }

    host.appendChild(renderOrgReviewControls());
    host.appendChild(renderOrgEditControl());
    host.appendChild(renderOrgAssignControl());

    if (org.preview && org.preview.row_id === row.id && org.preview.version === row.version) {
      host.appendChild(org.confirming ? renderOrgConfirmPanel() : renderOrgPreviewPanel());
    } else {
      host.appendChild(renderOrgPreviewTrigger());
    }

    appendBackToList(host);
    flushOrgPendingStatus();
  }

  function renderOrgReviewControls() {
    var panel = el("div", { class: "organizer-panel" });
    panel.appendChild(el("h4", { text: t("organizer.review.heading") }));
    var noteId = "organizer-review-note";
    panel.appendChild(el("label", { for: noteId, text: t("organizer.review.noteLabel") }));
    var note = el("textarea", { id: noteId, rows: "2", maxlength: "2000" });
    note.setAttribute("placeholder", t("organizer.review.notePlaceholder"));
    panel.appendChild(note);
    var actions = el("div", { class: "organizer-actions" });
    ORG_REVIEW_DECISIONS.forEach(function (dec) {
      var btn = el("button", dec.primary ? {} : { class: "secondary" });
      btn.textContent = t(dec.key);
      btn.setAttribute("data-review-action", dec.value);
      btn.disabled = org.busy;
      btn.addEventListener("click", function () { doReview(dec.value, note.value); });
      actions.appendChild(btn);
    });
    panel.appendChild(actions);
    panel.appendChild(el("p", { class: "muted", text: t("organizer.review.approveNote") }));
    return panel;
  }

  function renderOrgEditControl() {
    var panel = el("div", { class: "organizer-panel" });
    panel.appendChild(el("h4", { text: t("organizer.edit.heading") }));
    var id = "organizer-edit-summary";
    panel.appendChild(el("label", { for: id, text: t("organizer.edit.label") }));
    var input = el("textarea", { id: id, rows: "2", maxlength: "256" });
    input.value = org.detail.row.plain_summary || "";
    panel.appendChild(input);
    var save = el("button", { class: "secondary" });
    save.textContent = t("organizer.edit.save");
    save.setAttribute("data-org-action", "edit-save");
    save.disabled = org.busy;
    save.addEventListener("click", function () { doEdit(input.value); });
    panel.appendChild(el("div", { class: "organizer-actions" }, [save]));
    panel.appendChild(el("p", { class: "muted", text: t("organizer.edit.note") }));
    return panel;
  }

  function renderOrgAssignControl() {
    var panel = el("div", { class: "organizer-panel" });
    panel.appendChild(el("h4", { text: t("organizer.assign.heading") }));
    if (!org.bindings || org.bindings.length === 0) {
      panel.appendChild(el("p", { class: "muted", text: t("organizer.assign.noLabels") }));
      return panel;
    }
    var id = "organizer-assign-select";
    panel.appendChild(el("label", { for: id, text: t("organizer.assign.label") }));
    var select = el("select", { id: id, class: "assign-select" });
    var none = el("option", { value: "", text: t("organizer.assign.none") });
    if (!org.detail.row.assignee_label) { none.setAttribute("selected", "selected"); }
    select.appendChild(none);
    org.bindings.forEach(function (b) {
      var suffix = b.bound ? t("organizer.assign.bound") : t("organizer.assign.unbound");
      var opt = el("option", { value: b.label, text: b.label + " (" + suffix + ")" });
      if (org.detail.row.assignee_label === b.label) { opt.setAttribute("selected", "selected"); }
      select.appendChild(opt);
    });
    panel.appendChild(select);
    var save = el("button", { class: "secondary" });
    save.textContent = t("organizer.assign.save");
    save.setAttribute("data-org-action", "assign-save");
    save.disabled = org.busy;
    save.addEventListener("click", function () { doAssign(select.value); });
    var clearBtn = el("button", { class: "secondary" });
    clearBtn.textContent = t("organizer.assign.clear");
    clearBtn.setAttribute("data-org-action", "assign-clear");
    clearBtn.disabled = org.busy;
    clearBtn.addEventListener("click", function () { doAssign(""); });
    panel.appendChild(el("div", { class: "organizer-actions" }, [save, clearBtn]));
    panel.appendChild(el("p", { class: "muted", text: t("organizer.assign.note") }));
    return panel;
  }

  function renderOrgPreviewTrigger() {
    var panel = el("div", { class: "organizer-panel" });
    var row = org.detail.row;
    if (row.kind !== "action_item") {
      panel.appendChild(el("p", { class: "muted", text: t("organizer.preview.notExecutable") }));
      return panel;
    }
    if (row.status !== "approved_for_publish") {
      panel.appendChild(el("p", { class: "muted", text: t("organizer.preview.needsApproval") }));
      return panel;
    }
    var btn = el("button", {});
    btn.textContent = t("organizer.preview.button");
    btn.setAttribute("data-org-action", "preview");
    btn.disabled = org.busy;
    btn.addEventListener("click", function () { doPreview(); });
    panel.appendChild(btn);
    return panel;
  }

  function previewAssigneeText(p) {
    if (!p.assignee_label) { return t("organizer.preview.assigneeNone"); }
    return p.assignee_bound
      ? t("organizer.preview.assigneeBound", { label: p.assignee_label })
      : t("organizer.preview.assigneeUnbound", { label: p.assignee_label });
  }

  function previewReceiptsText(p) {
    var arr = p.expected_receipts || [];
    if (arr.length === 0) { return t("pp.receipt.none"); }
    return arr.map(function (c) {
      return ORG_RECEIPT_CLASS_LABEL[c] ? t(ORG_RECEIPT_CLASS_LABEL[c]) : String(c);
    }).join(" · ");
  }

  function renderOrgPreviewPanel() {
    var p = org.preview;
    var panel = el("div", { class: "confirm-panel" });
    panel.appendChild(el("h4", { id: "organizer-preview-h", text: t("organizer.preview.heading") }));
    var dl = el("dl", { class: "kv" });
    kvRow(dl, t("organizer.preview.kv.action"), t("organizer.preview.action"));
    kvRow(dl, t("organizer.preview.kv.title"), p.title || "");
    kvRow(dl, t("organizer.preview.kv.description"), p.description || "");
    kvRow(dl, t("organizer.preview.kv.domain"), org.domain.domain_name);
    kvRow(dl, t("organizer.preview.kv.assignee"), previewAssigneeText(p));
    kvRow(dl, t("organizer.preview.kv.authority"), p.authority_basis || t("pp.noAuthorityBasis"));
    var rk = RISK_LABEL[p.risk_level];
    kvRow(dl, t("organizer.preview.kv.risk"), rk ? (rk.glyph + " " + t(rk.text)) : ("● " + String(p.risk_level)));
    kvRow(dl, t("organizer.preview.kv.receipts"), previewReceiptsText(p));
    kvRow(dl, t("organizer.preview.kv.reversible"), t("organizer.preview.reversibleNo"));
    kvRow(dl, t("organizer.preview.kv.createsItem"), t("organizer.preview.createsYes"));
    panel.appendChild(dl);
    if (p.permanence_note) { panel.appendChild(el("p", { text: p.permanence_note })); }
    panel.appendChild(el("p", { class: "muted", text: p.privacy_note || t("organizer.preview.privacy") }));

    var det = el("details");
    det.appendChild(el("summary", { text: t("organizer.preview.showTechnical") }));
    var tdl = el("dl", { class: "kv" });
    kvRow(tdl, t("organizer.preview.kv.digest"), p.preview_digest || "");
    kvRow(tdl, t("organizer.preview.kv.planId"), p.plan_id || "");
    det.appendChild(tdl);
    panel.appendChild(det);

    var actions = el("div", { class: "actions" });
    if (p.confirmable !== false) {
      var cont = el("button", {});
      cont.textContent = t("organizer.preview.continue");
      cont.setAttribute("data-org-action", "continue");
      cont.disabled = org.busy;
      cont.addEventListener("click", function () { org.confirming = true; renderOrgDetail(); focusById("organizer-confirm-h"); });
      actions.appendChild(cont);
    } else {
      panel.appendChild(el("p", { class: "explain boundary", text: t("organizer.preview.notConfirmable") }));
    }
    var cancel = el("button", { class: "secondary" });
    cancel.textContent = t("organizer.preview.cancel");
    cancel.disabled = org.busy;
    cancel.addEventListener("click", function () { org.preview = null; org.confirming = false; renderOrgDetail(); });
    actions.appendChild(cancel);
    panel.appendChild(actions);
    return panel;
  }

  function renderOrgConfirmPanel() {
    var panel = el("div", { class: "confirm-panel" });
    panel.appendChild(el("h4", { id: "organizer-confirm-h", text: t("organizer.confirm.heading") }));
    panel.appendChild(el("p", { class: "explain boundary", text: t("organizer.confirm.warning") }));
    var actions = el("div", { class: "actions" });
    var confirm = el("button", {});
    confirm.textContent = t("organizer.confirm.button");
    confirm.setAttribute("data-org-action", "confirm");
    confirm.disabled = org.busy;
    confirm.addEventListener("click", function () { doConfirm(confirm); });
    var back = el("button", { class: "secondary" });
    back.textContent = t("organizer.confirm.back");
    back.disabled = org.busy;
    back.addEventListener("click", function () { org.confirming = false; renderOrgDetail(); focusById("organizer-preview-h"); });
    actions.appendChild(confirm);
    actions.appendChild(back);
    panel.appendChild(actions);
    return panel;
  }

  function renderOrgResultInto(host, executedRow) {
    var panel = el("div", { class: "confirm-panel" });
    panel.appendChild(el("h4", { id: "organizer-result-h", text: t("organizer.result.heading") }));
    var r = org.result;
    var label = (org.detail && org.detail.row && org.detail.row.assignee_label) ||
                (executedRow && executedRow.assignee_label);
    panel.appendChild(el("p", { text: label
      ? t("organizer.result.body", { label: label })
      : t("organizer.result.bodyUnassigned") }));
    if (r && r.idempotent) { panel.appendChild(el("p", { class: "muted", text: t("organizer.result.idempotent") })); }

    var det = el("details");
    det.appendChild(el("summary", { text: t("organizer.result.showTechnical") }));
    var tdl = el("dl", { class: "kv" });
    if (r) {
      kvRow(tdl, t("organizer.result.kv.actionItemId"), r.action_item_id || "");
      kvRow(tdl, t("organizer.result.kv.sessionId"), r.session_id || "");
      kvRow(tdl, t("organizer.result.kv.decisionId"), r.decision_id || "");
      kvRow(tdl, t("organizer.result.kv.activationId"), r.activation_id || "");
      kvRow(tdl, t("organizer.result.kv.planId"), r.plan_id || "");
      kvRow(tdl, t("organizer.result.kv.applicationId"), r.application_id || "");
      kvRow(tdl, t("organizer.result.kv.resultHash"), r.result_hash || "");
    } else if (executedRow && executedRow.execution) {
      kvRow(tdl, t("organizer.result.kv.actionItemId"), executedRow.execution.action_item_id || "");
    }
    det.appendChild(tdl);
    panel.appendChild(det);

    var actions = el("div", { class: "actions" });
    var vr = el("button", { class: "secondary" });
    vr.textContent = t("organizer.result.viewReceipts");
    vr.addEventListener("click", function () { show("organizer-receipts-section"); focusById("organizer-receipts-h"); });
    var ve = el("button", { class: "secondary" });
    ve.textContent = t("organizer.result.viewEvidence");
    ve.addEventListener("click", function () { show("organizer-evidence-section"); focusById("organizer-evidence-h"); });
    actions.appendChild(vr);
    actions.appendChild(ve);
    panel.appendChild(actions);
    host.appendChild(panel);
    show("organizer-member-section");
  }

  // ---- mutation handlers (each disables its controls, guards staleness) ----

  function doReview(decision, noteVal) {
    if (org.busy) { return; }
    var rowId = org.selectedRowId, seq = orgSeq;
    org.busy = true; setOrgControlsDisabled(true);
    setOrgStatus(t("organizer.review.submitting"));
    var body = { decision: decision };
    var note = (noteVal || "").trim();
    if (note) { body.note = note; }
    liveFetch(orgRowPath(rowId, "/review"), { method: "POST", body: JSON.stringify(body) }).then(function (resp) {
      org.busy = false;
      if (seq !== orgSeq || org.selectedRowId !== rowId) { return; }
      org.detail.row.status = resp.row.status;
      org.detail.row.version = resp.version;
      org.detail.row.note = note || null;
      org.preview = null; org.confirming = false;
      updateOrgListRow(rowId, { status: resp.row.status, version: resp.version });
      renderOrgRows();
      renderOrgDetail();
      var st = PP_STATUS_LABEL[resp.row.status];
      setOrgStatus(t("organizer.review.recorded", { status: st ? t(st.text) : String(resp.row.status) }));
      focusById("organizer-detail-h");   // keep keyboard/switch users on the item, not thrown to <body>
      loadOrgReceipts(seq);   // the review recorded a DecisionRecordedReceipt
    }).catch(function (err) {
      org.busy = false;
      if (seq !== orgSeq || org.selectedRowId !== rowId) { return; }
      renderOrgDetail();
      setOrgStatus(orgErrorText(err));
      focusById("organizer-detail-h");
    });
  }

  function doEdit(summaryVal) {
    if (org.busy) { return; }
    var rowId = org.selectedRowId, seq = orgSeq;
    var summary = (summaryVal || "").trim();
    if (!summary) { setOrgStatus(t("organizer.edit.empty")); return; }
    // The runtime bounds plain_summary to 256 BYTES; maxlength counts UTF-16 code
    // units, so validate the UTF-8 byte length here to fail friendly rather than
    // as a raw backend error on non-ASCII input.
    if (new TextEncoder().encode(summary).length > 256) { setOrgStatus(t("organizer.edit.tooLong")); return; }
    org.busy = true; setOrgControlsDisabled(true);
    setOrgStatus(t("organizer.edit.saving"));
    liveFetch(orgRowPath(rowId), { method: "PUT", body: JSON.stringify({ plain_summary: summary }) }).then(function (resp) {
      org.busy = false;
      if (seq !== orgSeq || org.selectedRowId !== rowId) { return; }
      org.detail.row.plain_summary = resp.row.plain_summary;
      org.detail.row.status = resp.row.status || "pending_review";
      org.detail.row.version = resp.version;
      org.preview = null; org.confirming = false;
      updateOrgListRow(rowId, { status: org.detail.row.status, version: resp.version, plain_summary: resp.row.plain_summary });
      renderOrgRows();
      renderOrgDetail();
      setOrgStatus(t("organizer.edit.saved"));
      focusById("organizer-detail-h");
    }).catch(function (err) {
      org.busy = false;
      if (seq !== orgSeq || org.selectedRowId !== rowId) { return; }
      renderOrgDetail();
      setOrgStatus(orgErrorText(err));
      focusById("organizer-detail-h");
    });
  }

  function doAssign(label) {
    if (org.busy) { return; }
    var rowId = org.selectedRowId, seq = orgSeq;
    org.busy = true; setOrgControlsDisabled(true);
    setOrgStatus(t("organizer.assign.saving"));
    var trimmed = (label || "").trim();
    var body = { assignee_label: trimmed ? trimmed : null };
    liveFetch(orgRowPath(rowId, "/assign"), { method: "POST", body: JSON.stringify(body) }).then(function (resp) {
      org.busy = false;
      if (seq !== orgSeq || org.selectedRowId !== rowId) { return; }
      org.detail.row.assignee_label = resp.row.assignee_label || null;
      org.detail.row.status = resp.row.status || "pending_review";
      org.detail.row.version = resp.version;
      org.detail.assignee_bound = resp.assignee_bound;
      org.preview = null; org.confirming = false;
      updateOrgListRow(rowId, {
        status: org.detail.row.status, version: resp.version,
        assignee_label: resp.row.assignee_label || null, assignee_bound: resp.assignee_bound
      });
      renderOrgRows();
      renderOrgDetail();
      setOrgStatus(t("organizer.assign.saved"));
      focusById("organizer-detail-h");
    }).catch(function (err) {
      org.busy = false;
      if (seq !== orgSeq || org.selectedRowId !== rowId) { return; }
      renderOrgDetail();
      setOrgStatus(orgErrorText(err));
      focusById("organizer-detail-h");
    });
  }

  function doPreview() {
    if (org.busy) { return; }
    var rowId = org.selectedRowId, version = org.detail.row.version, seq = orgSeq;
    org.busy = true; setOrgControlsDisabled(true);
    setOrgStatus(t("organizer.preview.loading"));
    liveFetch(orgRowPath(rowId, "/preview")).then(function (resp) {
      org.busy = false;
      if (seq !== orgSeq || org.selectedRowId !== rowId || org.detail.row.version !== version) { return; }
      org.preview = resp; org.confirming = false;
      setOrgStatus("");
      renderOrgDetail();
      focusById("organizer-preview-h");
    }).catch(function (err) {
      org.busy = false;
      if (seq !== orgSeq || org.selectedRowId !== rowId) { return; }
      renderOrgDetail();
      setOrgStatus(orgErrorText(err));
      focusById("organizer-detail-h");
    });
  }

  function doConfirm(confirmBtn) {
    if (org.busy) { return; }
    var rowId = org.selectedRowId, seq = orgSeq;
    var digest = org.preview && org.preview.preview_digest;
    if (!digest) { return; }
    org.busy = true;
    confirmBtn.disabled = true;   // one interaction → one request
    setOrgControlsDisabled(true);
    setOrgStatus(t("organizer.confirm.submitting"));
    liveFetch(orgRowPath(rowId, "/confirm"), {
      method: "POST", body: JSON.stringify({ preview_digest: digest })
    }).then(function (resp) {
      org.busy = false;
      if (seq !== orgSeq || org.selectedRowId !== rowId) { return; }
      org.result = resp; org.preview = null; org.confirming = false;
      org.detail.row.executed = true;
      updateOrgListRow(rowId, { executed: true });
      setOrgStatus("");
      renderOrgRows();
      renderOrgDetail();
      focusById("organizer-result-h");
      loadOrgReceipts(seq);
      loadOrgEvidence(seq);
      show("organizer-member-section");
    }).catch(function (err) {
      org.busy = false;
      if (seq !== orgSeq || org.selectedRowId !== rowId) { return; }
      if (err && err.status === 409) {
        // Stale preview / conflict: clear the local preview, re-fetch the row so
        // no enabled Confirm survives a stale state, and announce it plainly.
        org.preview = null; org.confirming = false;
        org.pendingStatus = t("organizer.confirm.stale");
        selectOrgRow(rowId);
        return;
      }
      renderOrgDetail();
      setOrgStatus(orgErrorText(err));
      focusById("organizer-detail-h");
    });
  }

  // ---- receipts + evidence (read-back) ----

  function loadOrgReceipts(seq) {
    liveFetch(orgDomainPath("/receipts")).then(function (resp) {
      if (seq !== orgSeq) { return; }
      org.receipts = resp;
      renderOrgReceipts();
    }).catch(function () { /* receipts unavailable; section left as-is */ });
  }

  function renderOrgReceipts() {
    var list = byId("organizer-receipts-list");
    clear(list);
    var receipts = (org.receipts && org.receipts.receipts) || [];
    if (receipts.length === 0) {
      list.appendChild(el("li", { text: t("organizer.receipts.none") }));
      return;   // nothing to reveal yet
    }
    receipts.forEach(function (rc) {
      var li = el("li");
      li.appendChild(el("h3", { text: ORG_RECEIPT_CLASS_LABEL[rc.class] ? t(ORG_RECEIPT_CLASS_LABEL[rc.class]) : String(rc.class) }));
      var dl = el("dl", { class: "kv" });
      if (rc.id) { kvRow(dl, t("organizer.receipts.kv.id"), rc.id); }
      if (rc.record_hash) { kvRow(dl, t("organizer.receipts.kv.recordHash"), rc.record_hash); }
      if (rc.row_id) { kvRow(dl, t("organizer.receipts.kv.rowId"), rc.row_id); }
      if (rc.decision) { kvRow(dl, t("organizer.receipts.kv.decision"), rc.decision); }
      if (rc.body_hash) { kvRow(dl, t("organizer.receipts.kv.bodyHash"), rc.body_hash); }
      if (rc.action_item_id) { kvRow(dl, t("organizer.receipts.kv.actionItemId"), rc.action_item_id); }
      var refs = rc.references_decision || rc.references_activation || rc.references_plan;
      if (refs) { kvRow(dl, t("organizer.receipts.kv.references"), refs); }
      li.appendChild(dl);
      list.appendChild(li);
    });
    show("organizer-receipts-section");
  }

  function loadOrgEvidence(seq) {
    liveFetch(orgDomainPath("/evidence-export")).then(function (resp) {
      if (seq !== orgSeq) { return; }
      org.evidence = resp;
      renderOrgEvidence();
    }).catch(function () { /* evidence unavailable; section left as-is */ });
  }

  function renderOrgEvidence() {
    var host = byId("organizer-evidence-body");
    clear(host);
    var e = org.evidence;
    if (!e) { host.appendChild(el("p", { text: t("organizer.evidence.none") })); return; }
    var dl = el("dl", { class: "kv" });
    kvRow(dl, t("organizer.evidence.kv.contract"), e.contract || "");
    kvRow(dl, t("organizer.evidence.kv.domain"), org.domain ? org.domain.domain_name : (e.domain_id || ""));
    kvRow(dl, t("organizer.evidence.kv.generation"), String(e.generation));
    kvRow(dl, t("organizer.evidence.kv.run"), e.run_id || "");
    host.appendChild(dl);
    host.appendChild(el("h3", { text: t("organizer.evidence.outcomesHeading") }));
    var ul = el("ul", { class: "card-list" });
    (e.rows || []).forEach(function (r) {
      var kind = PP_KIND_LABEL[r.kind] ? t(PP_KIND_LABEL[r.kind]) : String(r.kind);
      var outcome = ORG_OUTCOME_LABEL[r.outcome] ? t(ORG_OUTCOME_LABEL[r.outcome]) : String(r.outcome);
      ul.appendChild(el("li", { text: t("organizer.evidence.rowLine", { kind: kind, outcome: outcome }) }));
    });
    host.appendChild(ul);
    if (e.privacy_review_result) {
      host.appendChild(el("p", { class: "explain boundary", text: t("organizer.evidence.privacyReview", { result: e.privacy_review_result }) }));
    }
    if (e.non_claims && e.non_claims.length) {
      host.appendChild(el("h3", { text: t("organizer.evidence.nonClaimsHeading") }));
      var ncl = el("ul");
      e.non_claims.forEach(function (nc) { ncl.appendChild(el("li", { text: nc })); });
      host.appendChild(ncl);
    }
    var det = el("details");
    det.appendChild(el("summary", { text: t("organizer.evidence.showTechnical") }));
    var tdl = el("dl", { class: "kv" });
    kvRow(tdl, t("organizer.evidence.kv.packetHash"), e.packet_hash || "");
    kvRow(tdl, t("organizer.evidence.kv.packetHashSha256"), e.packet_hash_sha256 || "");
    det.appendChild(tdl);
    host.appendChild(det);
    // Reveal only once something has actually been confirmed (an executed row),
    // so the pre-confirm surface stays focused. The result panel's "View
    // evidence" button reveals it explicitly after a confirm.
    if (org.domain && (e.rows || []).some(function (r) { return r.mutation_executed; })) {
      show("organizer-evidence-section");
    }
  }

  // ---------------------------------------------------------------------
  // Boot
  // ---------------------------------------------------------------------
  function init() {
    // i18n boot: set <html lang>/<dir>, the document title, all static
    // data-i18n nodes and data-i18n-attr attributes, and the language
    // selector — before the mode-specific render path runs.
    var locale = I18N.locale;
    I18N.applyDocumentLocale(locale);
    document.title = t("doc.title");
    applyStaticI18n();
    wireLanguageSelector(locale);

    // Keep the chosen language sticky across Demo/Live mode switches: if the
    // user set ?lang= explicitly, carry it on the mode-nav links (which are
    // otherwise hardcoded to ?mode=...). Navigator-default locale leaves the
    // links clean.
    var langParam = params.get("lang");
    if (langParam) {
      var lp = encodeURIComponent(langParam);
      byId("nav-demo").setAttribute("href", "?mode=demo&lang=" + lp);
      byId("nav-live").setAttribute("href", "?mode=live&lang=" + lp);
      var memberLink = byId("organizer-member-link");
      if (memberLink) { memberLink.setAttribute("href", "?mode=live&lang=" + lp); }
    }

    var banner = byId("honesty-banner");
    if (MODE === "demo") {
      banner.textContent = t("banner.demo");
      banner.className = "banner demo";
      byId("nav-demo").setAttribute("aria-current", "true");
      loadDemo();
    } else if (SURFACE === "organizer") {
      // #2386 organizer rehearsal surface. Manual organizer credential, OR — on
      // the assembled appliance (?demo=launcher) — a no-paste organizer session
      // minted by the loopback demo-session endpoint.
      banner.textContent = t("banner.organizer");
      banner.className = "banner live";
      byId("nav-live").setAttribute("aria-current", "true");
      applyOrganizerConnectCopy();
      wireConnectForm(loadOrganizer);
      if (DEMO_LAUNCHER) {
        // Point the member-transition link at the MEMBER launcher URL so the
        // organizer continues as the member via a FRESH least-privilege session
        // (never a token upgrade), and offer the one-click organizer start.
        var ml = byId("organizer-member-link");
        if (ml) {
          var mp = new URLSearchParams();
          mp.set("mode", "live"); mp.set("demo", "launcher");
          if (params.get("gw")) { mp.set("gw", params.get("gw")); }
          if (params.get("session")) { mp.set("session", params.get("session")); }
          if (langParam) { mp.set("lang", langParam); }
          ml.setAttribute("href", "?" + mp.toString());
        }
        byId("gateway-url").value = DEMO_GATEWAY;
        setOrganizerLaunchCopy();
        show("demo-launch-section");
        setSyncChip(t(SYNC.DELAYED), "neutral", t("launcher.ready"));
        wireDemoLaunch("organizer", loadOrganizer, DEMO_FRESH);
      } else {
        // Manual connect on a non-loopback origin: the HTML default
        // (http://localhost:8080) would point at the viewer's machine, so
        // prefill the page's own origin (the LAN deployment proxies /v1/*).
        if (!PAGE_ON_LOOPBACK) { byId("gateway-url").value = window.location.origin; }
        show("connect-section");
        setSyncChip(t(SYNC.DELAYED), "neutral", t("launcher.notConnected"));
      }
    } else {
      banner.textContent = t("banner.live");
      banner.className = "banner live";
      byId("nav-live").setAttribute("aria-current", "true");
      wireConnectForm();
      if (DEMO_LAUNCHER) {
        // One-click DEV/DEMO start. Prefill the gateway for the launcher's
        // tunnel so even the "connect manually instead" fallback needs no
        // typing, then show the Start button.
        byId("gateway-url").value = DEMO_GATEWAY;
        show("demo-launch-section");
        setSyncChip(t(SYNC.DELAYED), "neutral", t("launcher.ready"));
        wireDemoLaunch("member", loadLive);
      } else {
        // Same non-loopback prefill as the organizer manual path above.
        if (!PAGE_ON_LOOPBACK) { byId("gateway-url").value = window.location.origin; }
        show("connect-section");
        setSyncChip(t(SYNC.DELAYED), "neutral", t("launcher.notConnected"));
      }
    }
  }

  // On the assembled appliance the organizer launch panel re-uses the demo-launch
  // section; give it organizer-rehearsal copy (the manual-connect fallback is the
  // organizer connect form, already wired above).
  function setOrganizerLaunchCopy() {
    byId("demo-launch-h").textContent = t("organizer.launch.heading");
    var body = document.querySelector("#demo-launch-section p[data-i18n=\"demoLaunch.body\"]");
    if (body) { body.textContent = t("organizer.launch.body"); }
    byId("demo-launch-button").textContent = t("organizer.launch.start");
  }

  // Apply the catalog to every static node carrying data-i18n (textContent)
  // and data-i18n-attr ("attr:key;attr:key" → setAttribute). Element
  // construction / textContent only — no innerHTML, so catalog strings can
  // never inject markup.
  function applyStaticI18n() {
    var textNodes = document.querySelectorAll("[data-i18n]");
    Array.prototype.forEach.call(textNodes, function (node) {
      node.textContent = t(node.getAttribute("data-i18n"));
    });
    var attrNodes = document.querySelectorAll("[data-i18n-attr]");
    Array.prototype.forEach.call(attrNodes, function (node) {
      node.getAttribute("data-i18n-attr").split(";").forEach(function (pair) {
        var parts = pair.split(":");
        if (parts.length === 2) {
          var attr = parts[0].trim();
          var key = parts[1].trim();
          if (attr && key) { node.setAttribute(attr, t(key)); }
        }
      });
    });
  }

  // Populate the language selector and, on change, set ?lang= and reload.
  // Reload is the simplest robust re-render path (no partial DOM rebuild).
  function wireLanguageSelector(activeLocale) {
    var select = byId("language-select");
    if (!select) { return; }
    I18N.availableLocales().forEach(function (loc) {
      var meta = I18N.localeMeta(loc) || { name: loc };
      var opt = el("option", { value: loc, text: meta.name });
      if (loc === activeLocale) { opt.setAttribute("selected", "selected"); }
      select.appendChild(opt);
    });
    select.addEventListener("change", function () {
      var params = new URLSearchParams(window.location.search);
      params.set("lang", select.value);
      window.location.search = params.toString();
    });
  }

  // DEV/DEMO one-click start: ask the launcher's loopback session endpoint for
  // a fresh demo session, hold it in page memory (never persisted, never in a
  // URL), then load standing + cards — the same render path as the manual
  // flow. Falls back to the manual connect form on any failure.
  // role is the CLOSED session intent ("member" | "organizer"); loader is the
  // surface loader to run once the session is minted. The manual-connect fallback
  // uses the connect form already wired for this surface in init().
  function wireDemoLaunch(role, loader, fresh) {
    loader = loader || loadLive;
    var btn = byId("demo-launch-button");
    var adv = byId("demo-advanced-button");
    var st = byId("demo-launch-status");
    function toManual(msg) {
      hide("demo-launch-section");
      show("connect-section");
      setSyncChip(t(SYNC.DELAYED), "neutral", t("launcher.notConnected"));
      if (msg) { byId("connect-status").textContent = msg; }
    }
    btn.addEventListener("click", function () {
      btn.disabled = true;
      st.textContent = t("launcher.starting");
      // JSON body carries the CLOSED role intent (plus the organizer-only
      // fresh flag); the loopback session endpoint maps it to a fixed command
      // and mints a least-privilege per-role session. On a cross-origin
      // (tunnel) posture this triggers a CORS preflight, which the endpoint
      // answers for the demo-shell origin; on the LAN single-origin posture
      // the request is same-origin. The credential lives only in page memory.
      fetch(DEMO_SESSION_URL, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(fresh ? { role: role, fresh: true } : { role: role })
      }).then(function (resp) {
        if (!resp.ok) { throw new Error("HTTP " + resp.status); }
        return resp.json();
      }).then(function (session) {
        var cred = session && session.jwt ? String(session.jwt) : "";
        if (!cred) { throw new Error("no session credential returned"); }
        // Use the launcher's tunnelled gateway, not the node-internal address
        // the endpoint reports. Credential lives only in page memory.
        state.gateway = DEMO_GATEWAY;
        state.credential = cred;
        // Drop ?fresh from the address bar once the new generation exists, so
        // a reload continues the rehearsal instead of silently resetting it
        // again (reset retires un-completed items).
        if (fresh) {
          var cleaned = new URLSearchParams(window.location.search);
          cleaned.delete("fresh");
          var qs = cleaned.toString();
          window.history.replaceState(null, "",
            window.location.pathname + (qs ? "?" + qs : ""));
        }
        hide("demo-launch-section");
        show("connect-section");
        loader();
      }).catch(function (err) {
        btn.disabled = false;
        st.textContent = t("launcher.startFailed", { error: err.message });
      });
    });
    adv.addEventListener("click", function () { toManual(""); });
  }

  init();
})();
