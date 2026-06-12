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
  // Closed member-facing vocabulary (docs/spec/member-shell-v0.md
  // §"Member-facing status vocabulary"). Strings are verbatim; inventing
  // new ones is a v0 violation.
  // ---------------------------------------------------------------------
  var SYNC = {
    SYNCED: "Synced",
    DELAYED: "Sync delayed",
    VERIFYING: "Some records are being verified",
    PAUSED: "Action paused until records sync",
    RECEIPT: "Receipt available",
    REVIEW: "Review required",
    DEGRADED: "Sync delayed / degraded"
  };

  var LIFECYCLE = {
    OPEN: "Open",
    OPEN_PAUSED: "Open but paused",
    SENT: "Sent — waiting for receipt",
    CONFIRMED: "Confirmed",
    DECLINED: "Declined",
    CLOSED_DEADLINE: "Closed — deadline passed",
    CLOSED_SUPERSEDED: "Closed — superseded",
    CLOSED_AUTHORITY: "Closed — insufficient authority"
  };

  // Plain-language maps for the closed ADR-0027 enums. The raw enum value
  // is never the primary surface (spec §"ActionCard rendering contract").
  var ACTION_KIND_LABEL = {
    vote: "Cast a vote",
    attend: "Confirm attendance",
    complete: "Mark a task complete"
  };
  var SOURCE_KIND_LABEL = {
    proposal: "A proposal in your domain",
    meeting: "A scheduled meeting",
    action_item: "A task assigned to you",
    // Reserved source paths (icn#1631 / icn#1634): render as inert
    // "available soon" placeholders, never as live cards.
    signal_rule: "Available soon (not yet enabled)",
    obligation_lifecycle: "Available soon (not yet enabled)"
  };
  var SCOPE_LABEL = {
    entity: "Affects your whole organization",
    structure: "Affects one body within your organization (a committee or working group)",
    individual: "Affects only you"
  };
  var RISK_LABEL = {
    low: { glyph: "○", text: "Low impact" },        // ○
    normal: { glyph: "◐", text: "Normal impact" },  // ◐
    elevated: { glyph: "▲", text: "Take extra care" } // ▲
  };

  // ---------------------------------------------------------------------
  // Mode + page state
  // ---------------------------------------------------------------------
  var params = new URLSearchParams(window.location.search);
  var MODE = params.get("mode") === "demo" ? "demo" : "live";

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
    if (typeof unixSeconds !== "number") { return "(no timestamp)"; }
    return new Date(unixSeconds * 1000).toLocaleString();
  }

  // Relative time, anchored to an explicit reference point so the demo
  // fixture (a frozen snapshot) renders honestly against its own
  // generated_at rather than pretending the snapshot is current.
  function fmtRel(targetSec, anchorSec) {
    var delta = targetSec - anchorSec;
    var abs = Math.abs(delta);
    var unit, n;
    if (abs >= 86400) { n = Math.round(abs / 86400); unit = n === 1 ? "day" : "days"; }
    else if (abs >= 3600) { n = Math.round(abs / 3600); unit = n === 1 ? "hour" : "hours"; }
    else { n = Math.max(1, Math.round(abs / 60)); unit = n === 1 ? "minute" : "minutes"; }
    return delta >= 0
      ? "closes in about " + n + " " + unit
      : "deadline passed about " + n + " " + unit + " ago";
  }

  function hashToHex(recordHash) {
    // Wire form is a 32-byte array (icn-governance proof.rs `Hash = [u8; 32]`).
    if (Array.isArray(recordHash)) {
      return recordHash.map(function (b) {
        return ("0" + (b & 0xff).toString(16)).slice(-2);
      }).join("");
    }
    if (typeof recordHash === "string") { return recordHash; }
    return "(unavailable)";
  }

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
    var label = standing.display_label || "(no display name configured)";
    byId("identity-label").textContent = label;
    byId("identity-did").textContent = standing.did || "(unknown)";
    show("identity-section");
  }

  function membershipPlain(domain) {
    if (domain.status === "member") {
      return "You are a member of this domain (you are on its member list).";
    }
    if (domain.status === "unverified") {
      return "Your membership here comes from a participation-trust source " +
        "and is shown as unverified in this view. That does not mean you " +
        "are not a member — it means this view does not evaluate it.";
    }
    return "Membership status: " + String(domain.status);
  }

  function renderStanding(standing) {
    var domainsList = byId("domains-list");
    clear(domainsList);
    if (!standing.domains || standing.domains.length === 0) {
      domainsList.appendChild(el("li", {
        text: "No memberships are recorded for you yet. If you expect to " +
          "see one, please contact your steward."
      }));
    } else {
      standing.domains.forEach(function (d) {
        var li = el("li");
        li.appendChild(el("h3", { text: d.domain_name || d.domain_id }));
        li.appendChild(el("p", { text: membershipPlain(d) }));
        var details = el("details");
        details.appendChild(el("summary", { text: "Show technical detail" }));
        var dl = el("dl", { class: "kv" });
        kvRow(dl, "Domain id", d.domain_id || "");
        kvRow(dl, "Membership source", d.membership_source || "");
        kvRow(dl, "Status", d.status || "");
        details.appendChild(dl);
        li.appendChild(details);
        domainsList.appendChild(li);
      });
    }

    var rolesList = byId("roles-list");
    clear(rolesList);
    if (!standing.roles || standing.roles.length === 0) {
      rolesList.appendChild(el("li", { text: "You hold no recorded roles right now." }));
    } else {
      standing.roles.forEach(function (r) {
        var li = el("li");
        var structureName = r.structure_name || ("structure " + r.structure_id +
          " (its name is currently unavailable — something may be off; you can ask your steward)");
        li.appendChild(el("h3", { text: capitalize(r.role) + " — " + structureName }));
        li.appendChild(el("p", {
          text: "This role authorizes you to act in these areas: " +
            (r.authority_scope && r.authority_scope.length
              ? r.authority_scope.map(scopePlain).join("; ")
              : "(none recorded)") + "."
        }));
        li.appendChild(el("p", {
          class: "muted",
          text: "Held since " + fmtAbs(r.start_date) +
            (r.end_date ? ("; ends " + fmtAbs(r.end_date)) : "")
        }));
        var details = el("details");
        details.appendChild(el("summary", { text: "Show technical detail" }));
        var dl = el("dl", { class: "kv" });
        kvRow(dl, "Role assignment id", r.role_assignment_id || "");
        kvRow(dl, "Structure id", r.structure_id || "");
        kvRow(dl, "Parent entity", r.parent_entity_id || "(none)");
        kvRow(dl, "Authority scope strings", (r.authority_scope || []).join(", "));
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
      li.appendChild(el("p", { text: "Nothing needs your attention right now." }));
      if (MODE === "live") {
        var again = el("button", { class: "secondary", text: "Check again" });
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

    li.appendChild(el("h3", { text: card.title || "(untitled card)" }));

    // accessibility_hint renders as plain-language preamble BEFORE any
    // decision controls (ADR-0028 / spec rendering table).
    if (card.accessibility_hint) {
      li.appendChild(el("p", { class: "muted", text: "Before you decide: " + card.accessibility_hint }));
    }

    li.appendChild(el("p", { text: card.summary || "" }));

    // Lifecycle status chip (closed vocabulary). This client derives only
    // states it can honestly know: reserved → inert; missing authority →
    // Closed — insufficient authority; otherwise Open. Mutation flow moves
    // a card to Sent/Confirmed below.
    var status = reserved ? SOURCE_KIND_LABEL[card.source_kind]
      : auth.authorized ? LIFECYCLE.OPEN
      : LIFECYCLE.CLOSED_AUTHORITY;
    var chipTone = reserved ? "neutral" : auth.authorized ? "ok" : "warn";
    var chipGlyph = reserved ? "● " : auth.authorized ? "✓ " : "⚠ ";
    var statusChip = el("span", { class: "chip " + chipTone, role: "status", text: chipGlyph + status });
    li.appendChild(el("p", {}, [statusChip]));

    var dl = el("dl", { class: "kv" });
    kvRow(dl, "What you are being asked to do",
      ACTION_KIND_LABEL[card.action_kind] || ("Act (" + String(card.action_kind) + ")"));
    kvRow(dl, "Where this comes from",
      SOURCE_KIND_LABEL[card.source_kind] || String(card.source_kind));
    kvRow(dl, "Scope", SCOPE_LABEL[card.scope] || String(card.scope));
    kvRow(dl, "Why you can act here", card.authority_basis || "(no authority basis given)");

    if (auth.authorized) {
      kvRow(dl, "Authorization", "You are authorized for this.");
    } else {
      kvRow(dl, "Authorization",
        "This requires authority you do not currently hold: " +
        auth.missing.map(scopePlain).join("; ") + ".");
    }

    if (typeof card.deadline === "number") {
      var rel = fmtRel(card.deadline, anchorSec);
      var dd = el("span", { text: rel + " " });
      var det = el("details");
      det.appendChild(el("summary", { text: "Exact time" }));
      det.appendChild(el("p", { text: fmtAbs(card.deadline) + " (your local time)" }));
      var wrap = el("span");
      wrap.appendChild(dd);
      wrap.appendChild(det);
      kvRow(dl, "Time pressure", wrap);
    } else {
      // Spec: absent deadline renders as "no time pressure", not "no deadline".
      kvRow(dl, "Time pressure", "No time pressure.");
    }

    var risk = RISK_LABEL[card.risk_level] || { glyph: "●", text: String(card.risk_level) };
    kvRow(dl, "Care level", risk.glyph + " " + risk.text);

    kvRow(dl, "What happens if you act",
      card.receipt_expected
        ? "Acting will produce a receipt — a permanent record you can review in your Receipts."
        : "Acting is not expected to produce a receipt.");
    li.appendChild(dl);

    // Technical identifiers live under "details", never as primary surface.
    var details = el("details");
    details.appendChild(el("summary", { text: "Show technical detail" }));
    var tdl = el("dl", { class: "kv" });
    kvRow(tdl, "Card id", card.id || "");
    kvRow(tdl, "Underlying record id", card.source_id || "");
    kvRow(tdl, "Domain id", card.domain_id || "(not carried on this card)");
    kvRow(tdl, "Raw kind / action / scope / risk",
      [card.source_kind, card.action_kind, card.scope, card.risk_level].join(" / "));
    details.appendChild(tdl);
    li.appendChild(details);

    // The one mutating action this v0 client implements (live mode only):
    // mark an assigned task complete, then fetch and render its completion
    // receipt. Everything else is read-only.
    if (MODE === "live" && !reserved &&
        card.source_kind === "action_item" && card.action_kind === "complete") {
      li.appendChild(buildCompleteFlow(card, auth, statusChip));
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
  function buildCompleteFlow(card, auth, statusChip) {
    var holder = el("div");

    if (!card.domain_id) {
      holder.appendChild(el("p", {
        class: "muted",
        text: "This card does not carry a domain reference, so this client " +
          "cannot address the completion endpoint for it."
      }));
      return holder;
    }

    var openBtn = el("button", { text: "Mark complete…" });
    var panel = el("div", { class: "confirm-panel", hidden: "hidden" });

    // Pre-confirm summary (spec §"Signing / confirmation flow"): authority
    // basis, scope, consequence, receipt expected, reversibility honesty.
    panel.appendChild(el("h4", { text: "Before you confirm" }));
    var pdl = el("dl", { class: "kv" });
    kvRow(pdl, "What will change",
      "This task will be recorded as completed, in your name.");
    kvRow(pdl, "Why you may do this", card.authority_basis || "(no authority basis given)");
    kvRow(pdl, "Scope", SCOPE_LABEL[card.scope] || String(card.scope));
    kvRow(pdl, "Receipt", card.receipt_expected
      ? "A completion receipt will be produced and shown in your Receipts."
      : "No receipt is expected for this action.");
    kvRow(pdl, "Can this be undone?",
      "The completion record is permanent once accepted. This client " +
      "offers no undo; if you complete it in error, contact your steward.");
    panel.appendChild(pdl);

    var statusLine = el("p", { role: "status", "aria-live": "polite" });

    var confirmBtn = el("button", { text: "Confirm — mark complete" });
    var cancelBtn = el("button", { class: "secondary", text: "Cancel" });
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
      confirmBtn.disabled = true;
      cancelBtn.disabled = true;
      statusLine.textContent = LIFECYCLE.SENT;
      statusChip.textContent = "● " + LIFECYCLE.SENT;
      statusChip.className = "chip neutral";

      liveFetch("/v1/gov/domains/" + encodeURIComponent(card.domain_id) +
                "/action-items/" + encodeURIComponent(card.source_id) + "/status", {
        method: "PUT",
        body: JSON.stringify({ status: "completed" })
      }).then(function () {
        statusLine.textContent = LIFECYCLE.SENT + " — completion accepted, fetching the receipt.";
        return liveFetch("/v1/gov/domains/" + encodeURIComponent(card.domain_id) +
                         "/action-items/" + encodeURIComponent(card.source_id) +
                         "/completion-receipt", { method: "GET" });
      }).then(function (receipt) {
        statusChip.textContent = "✓ " + LIFECYCLE.CONFIRMED;
        statusChip.className = "chip ok";
        statusLine.textContent = LIFECYCLE.CONFIRMED + ". " + SYNC.RECEIPT +
          " — see your Receipts below.";
        addReceipt(receipt, "You marked a task complete.");
        setSyncChip(SYNC.RECEIPT, "ok",
          "Your latest action produced a receipt. Records were last refreshed at " +
          new Date().toLocaleString() + ".");
      }).catch(function (err) {
        // "rejected with reason" — the gateway's answer, rendered plainly,
        // never a silent failure.
        statusLine.textContent = "Rejected with reason: " + err.message +
          " Nothing was recorded. You can try again or contact your steward.";
        statusChip.textContent = "⚠ " + LIFECYCLE.OPEN;
        statusChip.className = "chip warn";
        confirmBtn.disabled = false;
        cancelBtn.disabled = false;
      });
    });

    if (!auth.authorized) {
      openBtn.disabled = true;
      holder.appendChild(el("p", {
        class: "muted",
        text: "Action unavailable here: your standing does not list the " +
          "required authority (" + auth.missing.map(scopePlain).join("; ") +
          "). The node makes the final decision; this client will not send " +
          "an action it can already see is not authorized."
      }));
    }

    holder.appendChild(openBtn);
    holder.appendChild(panel);
    return holder;
  }

  // ---------------------------------------------------------------------
  // Receipts (plain summary first; formal record under a disclosure)
  // ---------------------------------------------------------------------
  function addReceipt(receipt, plainContext) {
    state.receipts.push({ receipt: receipt, plainContext: plainContext });
    renderReceipts();
  }

  function renderReceipts() {
    var list = byId("receipts-list");
    clear(list);
    if (state.receipts.length === 0) {
      list.appendChild(el("li", {
        text: "No receipts to show yet. When you complete an action that " +
          "produces a receipt, it appears here."
      }));
    } else {
      state.receipts.forEach(function (entry) {
        list.appendChild(renderCompletionReceipt(entry.receipt, entry.plainContext));
      });
    }
    show("receipts-section");
  }

  // Renders an ActionItemCompletionReceipt (icn-governance proof.rs):
  // { item_id, domain_id, actor_did, transition, completed_at, record_hash }.
  function renderCompletionReceipt(receipt, plainContext) {
    var li = el("li");
    var isSelf = state.standing && receipt.actor_did === state.standing.did;
    var who = isSelf ? "You" : "Another member (identifier under evidence detail)";

    li.appendChild(el("h3", { text: "Action completed" }));
    li.appendChild(el("p", {}, [
      el("span", { class: "chip ok", text: "✓ " + SYNC.RECEIPT })
    ]));
    li.appendChild(el("p", {
      text: who + " marked a task as complete on " + fmtAbs(receipt.completed_at) + "."
    }));
    li.appendChild(el("p", {
      class: "muted",
      text: "What this proves: who acted (the actor recorded below), which " +
        "obligation it satisfied (the task identified below), and when. " +
        (plainContext || "")
    }));

    var details = el("details");
    details.appendChild(el("summary", { text: "Show evidence detail" }));
    var dl = el("dl", { class: "kv" });
    kvRow(dl, "Record class", "ActionItemCompletionReceipt");
    kvRow(dl, "Task (action item) id", String(receipt.item_id || ""));
    kvRow(dl, "Domain id", String(receipt.domain_id || ""));
    kvRow(dl, "Actor", el("code", { text: String(receipt.actor_did || "") }));
    kvRow(dl, "Transition", String(receipt.transition || ""));
    kvRow(dl, "Completed at (unix seconds)", String(receipt.completed_at || ""));
    kvRow(dl, "Record hash (blake3, canonical binding of the fields above)",
      el("code", { text: hashToHex(receipt.record_hash) }));
    details.appendChild(dl);
    li.appendChild(details);
    return li;
  }

  // ---------------------------------------------------------------------
  // Demo mode: fixture-backed, nothing signed, no live node.
  // ---------------------------------------------------------------------
  function loadDemo() {
    setSyncChip(SYNC.VERIFYING, "neutral", "Loading the committed fixture snapshot…");
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
      addReceipt(results[2],
        "This is a fictional demonstration receipt for a previously " +
        "completed task; it is not tied to any card above.");

      setSyncChip(SYNC.SYNCED, "ok",
        "Fixture snapshot generated at " + fmtAbs(anchor) +
        ". Deadlines above are read relative to that snapshot moment, " +
        "because fixture data does not move with the clock.");
    }).catch(function (err) {
      setSyncChip(SYNC.DEGRADED, "warn",
        "The fixture files could not be loaded (" + err.message + "). " +
        "Make sure the page is served from the web/ directory — see the README.");
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
          throw new Error("The node answered " + resp.status + ": " + String(reason).slice(0, 300));
        });
      }
      return resp.json();
    });
  }

  function loadLive() {
    setSyncChip(SYNC.VERIFYING, "neutral", "Asking the node for your standing…");
    byId("connect-status").textContent = "";

    liveFetch("/v1/gov/me/standing").then(function (standing) {
      state.standing = standing;
      renderIdentity(standing);
      renderStanding(standing);
      return liveFetch("/v1/gov/me/action-cards");
    }).then(function (cardsResponse) {
      state.cards = cardsResponse;
      var anchor = Math.floor(Date.now() / 1000);
      renderCards(cardsResponse, state.standing, anchor);
      renderReceipts();
      setSyncChip(SYNC.SYNCED, "ok",
        "Records last refreshed at " + new Date().toLocaleString() +
        ". This view is a snapshot; refresh to re-check.");
      byId("connect-status").textContent = "Connected. Your standing and open actions are shown below.";
    }).catch(function (err) {
      // Failure-table disposition (spec §"Failure and safety table"):
      // plain language first, no error code as primary surface, help path
      // always visible.
      setSyncChip(SYNC.DEGRADED, "warn",
        "Your standing is currently unavailable. We'll keep trying. " +
        "If this persists, please contact your steward.");
      byId("connect-status").textContent =
        "Could not load your records. Technical detail: " + err.message;
    });
  }

  function wireConnectForm() {
    byId("connect-form").addEventListener("submit", function (ev) {
      ev.preventDefault();
      var url;
      try {
        url = new URL(byId("gateway-url").value);
      } catch (e) {
        byId("connect-status").textContent = "That gateway address is not a valid URL.";
        return;
      }
      if (url.protocol !== "http:" && url.protocol !== "https:") {
        byId("connect-status").textContent = "The gateway address must start with http:// or https://.";
        return;
      }
      state.gateway = url.origin;
      state.credential = byId("credential").value.trim();
      if (!state.credential) {
        byId("connect-status").textContent = "Paste an access credential first.";
        return;
      }
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
    var banner = byId("honesty-banner");
    if (MODE === "demo") {
      banner.textContent = "Fixture-backed demo — no live node, nothing signed.";
      banner.className = "banner demo";
      byId("nav-demo").setAttribute("aria-current", "true");
      loadDemo();
    } else {
      banner.textContent = "Live-local node — dev rehearsal, not production.";
      banner.className = "banner live";
      byId("nav-live").setAttribute("aria-current", "true");
      show("connect-section");
      setSyncChip(SYNC.DELAYED, "neutral",
        "Not connected yet. Enter your local gateway address above.");
      wireConnectForm();
    }
  }

  init();
})();
