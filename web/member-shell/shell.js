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
  // Mode + page state
  // ---------------------------------------------------------------------
  var params = new URLSearchParams(window.location.search);
  var MODE = params.get("mode") === "demo" ? "demo" : "live";

  // #2289 organizer-steward evidence surface. `?mode=demo&set=process-evidence`
  // swaps the demo pack for a fixture-only, read-only evidence story over the
  // four already-landed ADR-0026 Layer 2 process-transition receipts (session
  // opened -> deliberation entry recorded -> decision recorded -> gate result)
  // plus a repo-safe evidence-summary export. Fixture/dev only: nothing is live,
  // every hash is illustrative (see the demo hash label), and the surface
  // renders read views only — no download, no mutation. `set` is demo-only.
  var SET = MODE === "demo" ? params.get("set") : null;

  // DEV/DEMO one-click launcher context. The local launcher
  // (deploy/appliance/scripts/open-proxmox-demo.sh) opens this page with
  // ?demo=launcher and forwards two loopback ports: the gateway to :18080 and
  // a DEV/DEMO session endpoint to :18091. When present, the shell shows a
  // "Start local demo" button that obtains a fresh session in one click — no
  // gateway typing, no credential paste, no credential in any URL. The
  // credential is held in page memory exactly like the manual flow (never
  // persisted). The flag is a UI hint only and carries no secret.
  var DEMO_LAUNCHER = MODE === "live" && params.get("demo") === "launcher";
  var DEMO_LOOPBACK = window.location.hostname === "127.0.0.1" ? "127.0.0.1" : "localhost";
  // The launcher forwards the gateway and session endpoint to host ports and
  // passes them as ?gw / ?session so operator port overrides
  // (ICN_DEMO_GW_PORT / ICN_DEMO_SESSION_PORT) are honored. Digits only — a
  // non-numeric value falls back to the default and can never inject into the
  // URL we build.
  function demoPort(name, fallback) {
    var v = params.get(name);
    return v && /^[0-9]{1,5}$/.test(v) ? v : fallback;
  }
  var DEMO_GATEWAY = "http://" + DEMO_LOOPBACK + ":" + demoPort("gw", "18080");
  var DEMO_SESSION_URL = "http://" + DEMO_LOOPBACK + ":" + demoPort("session", "18091") + "/v1/dev/demo/session";

  var state = {
    gateway: null,     // live mode only; validated http(s) origin string
    credential: null,  // live mode only; never persisted
    standing: null,    // StandingResponse
    cards: null,       // ActionCardsResponse
    receipts: []       // rendered receipt objects {receipt, plainContext}
  };

  // Fixture paths are relative to this page. They resolve only when the
  // serve root is web/ (see README): /member-shell/.. → /pilot-ui/...
  var FIXTURES = {
    standing: "../pilot-ui/fixtures/icn-organizer-demo/standing.json",
    cards: "../pilot-ui/fixtures/icn-organizer-demo/action-cards.json",
    // member-shell-local receipt fixture (the pilot-ui pack carries no
    // receipt packet); wire-shaped per icn-governance proof.rs
    // ActionItemCompletionReceipt.
    completionReceipt: "fixtures/demo-completion-receipt.json"
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
  // standing + cards pack; adds the four-receipt process-evidence sequence and
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
  // The single mutation: complete an action item (live mode).
  // PUT /v1/gov/domains/{domain_id}/action-items/{item_id}/status
  //   body {"status": "completed"}
  //   scopes: governance:meeting:write OR governance:write
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
  // process-evidence pack passes opts.kind (one of the four process-transition
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
  // Renders one of the four ADR-0026 Layer 2 process-transition receipts
  // (proof.rs) as a plain-language summary first, with the record-level fields
  // under a progressive-disclosure "Show evidence detail" control. record_hash
  // is the proof pointer; body_hash is labeled proof-of-content (the body is
  // never stored). In demo mode the hashes are illustrative, mirroring the
  // completion receipt's honesty label. No readiness is claimed.
  // ---------------------------------------------------------------------
  var PROCESS_RECEIPT_CLASS = {
    process_session_opened: "ProcessSessionOpenedReceipt",
    deliberation_entry_recorded: "DeliberationEntryRecordedReceipt",
    decision_recorded: "DecisionRecordedReceipt",
    process_gate_result: "ProcessGateResultReceipt"
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
      fetchJson(FIXTURES.completionReceipt)
    ]).then(function (results) {
      state.standing = results[0];
      state.cards = results[1];
      var anchor = state.cards.generated_at || state.standing.generated_at;

      renderIdentity(state.standing);
      renderStanding(state.standing);
      renderCards(state.cards, state.standing, anchor);
      addReceipt(results[2], t("demo.receiptContext"));

      setSyncChip(t(SYNC.SYNCED), "ok",
        t("demo.syncedDetail", { when: fmtAbs(anchor) }));
    }).catch(function (err) {
      setSyncChip(t(SYNC.DEGRADED), "warn",
        t("demo.loadFailed", { error: err.message }));
    });
  }

  // #2289 organizer-steward evidence surface (fixture-only). Reuses the demo
  // standing + cards render path, then renders the four-receipt process
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
          throw new Error(t("live.nodeAnswered", { status: resp.status, reason: String(reason).slice(0, 300) }));
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

  function wireConnectForm() {
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
      loadLive();
    });
  }

  function fetchJson(path) {
    return fetch(path).then(function (resp) {
      if (!resp.ok) { throw new Error("HTTP " + resp.status + " for " + path); }
      return resp.json();
    });
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
    }

    var banner = byId("honesty-banner");
    if (MODE === "demo") {
      banner.textContent = t("banner.demo");
      banner.className = "banner demo";
      byId("nav-demo").setAttribute("aria-current", "true");
      loadDemo();
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
        wireDemoLaunch();
      } else {
        show("connect-section");
        setSyncChip(t(SYNC.DELAYED), "neutral", t("launcher.notConnected"));
      }
    }
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
  function wireDemoLaunch() {
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
      // Simple cross-origin POST (no custom headers) — no preflight needed.
      fetch(DEMO_SESSION_URL, { method: "POST" }).then(function (resp) {
        if (!resp.ok) { throw new Error("HTTP " + resp.status); }
        return resp.json();
      }).then(function (session) {
        var cred = session && session.jwt ? String(session.jwt) : "";
        if (!cred) { throw new Error("no session credential returned"); }
        // Use the launcher's tunnelled gateway, not the node-internal address
        // the endpoint reports. Credential lives only in page memory.
        state.gateway = DEMO_GATEWAY;
        state.credential = cred;
        hide("demo-launch-section");
        show("connect-section");
        loadLive();
      }).catch(function (err) {
        btn.disabled = false;
        st.textContent = t("launcher.startFailed", { error: err.message });
      });
    });
    adv.addEventListener("click", function () { toManual(""); });
  }

  init();
})();
