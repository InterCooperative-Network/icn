/* ICN Member Shell — i18n seam (icn#2042).
 *
 * Dependency-free. No framework, no build step, no storage, no network.
 * A single IIFE exposing `window.ICNI18n`. Loaded BEFORE shell.js.
 *
 * This is the INFRASTRUCTURE for language, not a set of translations. Adding a
 * language is a catalog entry (`MESSAGES[locale]` + a `LOCALES` row), never a
 * code change. The English catalog is the source of truth and the per-key
 * fallback target.
 *
 * Contract: docs/spec/member-shell-i18n-v0.md. Consistent with the closed
 * member-facing vocabulary in docs/spec/member-shell-v0.md — the SYNC and
 * LIFECYCLE values below are byte-identical to that spec's closed set.
 *
 * Hard rules honored here:
 *  - Glyphs (✓ ⚠ ● ○ ◐ ▲) are NOT strings in this catalog; they stay in code.
 *  - Class/record names (ActionItemCompletionReceipt), raw enum values, DIDs,
 *    ids, and hashes are NOT translated.
 *  - No payment/wallet/balance/currency/token/blockchain/timebank vocabulary.
 */
(function () {
  "use strict";

  // -------------------------------------------------------------------------
  // The catalog. EVERY member-facing string in shell.js + index.html lives
  // here, keyed by short semantic dotted keys. Parametrized strings use
  // {name} placeholders interpolated by t().
  // -------------------------------------------------------------------------
  var MESSAGES = {
    en: {
      // --- Document chrome / index.html static text ---------------------
      "doc.title": "ICN Member Shell — v0 reference client",
      "skip.toMain": "Skip to main content",
      "header.title": "ICN Member Shell",
      "header.subtitle": "v0 reference client",
      "header.lede": "Your view of your own participation: who you are here, what you are authorized to do, what needs your attention, and the receipts that prove what happened.",

      // Mode nav
      "nav.mode.label": "Mode",
      "nav.mode.demo": "Demo mode (fixture data)",
      "nav.mode.live": "Live mode (local node)",

      // Language selector
      "nav.language.label": "Language",
      "nav.language.choose": "Choose language",

      // Demo launcher section
      "demoLaunch.heading": "Start the local demo",
      "demoLaunch.body": "This page was opened by the local DEV/DEMO launcher. Select Start to begin a fresh local demo — your standing and a sample action card load automatically. There is no address to type and no credential to paste. The session is local to this dev node and nothing here is signed.",
      "demoLaunch.start": "Start local demo",
      "demoLaunch.manual": "Connect manually instead",

      // Connect section
      "connect.heading": "Connect to a local node",
      "connect.body": "Enter the address of a locally running ICN gateway and paste an access credential (the signed authorization string issued by the gateway's sign-in flow). Your credential is kept only on this page while it is open. It is never saved, never written to storage, and never sent anywhere except the gateway address you enter.",
      "connect.gatewayLabel": "Gateway address",
      "connect.credentialLabel": "Access credential",
      "connect.credentialPlaceholder": "Paste credential",
      "connect.credentialHelp.before": "Needs the ",
      "connect.credentialHelp.middle": " scope to read your standing and action cards. Marking a task complete additionally needs ",
      "connect.credentialHelp.or": " or ",
      "connect.credentialHelp.after": ". If connecting fails immediately with \"Failed to fetch\", the node was not started with this page's address in ",
      "connect.credentialHelp.end": " — see this directory's README.",
      "connect.submit": "Connect and load my standing",

      // Sync / records section
      "sync.heading": "Records status",

      // Identity section
      "identity.heading": "Who am I here",
      "identity.showTechnical": "Show technical identifier",
      "identity.didIntro": "Your network identity (a ",
      "identity.didDfn": "DID",
      "identity.didIntroAfter": " — decentralized identifier, the address your participation records are tied to):",

      // Standing section
      "standing.heading": "My standing",
      "standing.explain": "Standing is what this institution currently recognizes about you: where you are a member, which roles you hold, and what those roles authorize you to do.",
      "standing.memberships": "Memberships",
      "standing.rolesAndAuthority": "Roles and authority",

      // Cards section
      "cards.heading": "Things you can act on",
      "cards.explain": "Each card below is something open to you right now, with what it is, why you are allowed to act, and what will happen if you do.",

      // Receipts section
      "receipts.heading": "Receipts",
      "receipts.explain": "A receipt is the record that proves something happened: who acted, what obligation it satisfied, and when. The plain-language summary comes first; the formal evidence is available under “Show evidence detail.”",

      // Help section
      "help.heading": "Need help?",
      "help.body": "If something here looks wrong — a missing membership, a card you cannot act on, a record you want reviewed — contact your steward. In this reference client the steward contact path is not wired; a real deployment fills this in from its institution package.",

      // Footer
      "footer.intro": "This is a ",
      "footer.referenceClient": "reference client",
      "footer.middle": " proving the ",
      "footer.linkText": "member-shell-v0 rendering contract",
      "footer.after": " is implementable. It is not the production member shell, makes no platform decision, has no offline support, and claims no live-federation or production posture. See the README in this directory for the honest maturity and accessibility checklist.",

      // --- Honesty banner ----------------------------------------------
      "banner.loading": "Loading…",
      "banner.demo": "Fixture-backed demo — no live node, nothing signed.",
      "banner.live": "Live-local node — dev rehearsal, not production.",

      // --- Sync state (closed vocabulary, member-shell-v0 §"Sync state") -
      // VERBATIM. Byte-identical to the spec's closed set.
      "sync.synced": "Synced",
      "sync.delayed": "Sync delayed",
      "sync.verifying": "Some records are being verified",
      "sync.paused": "Action paused until records sync",
      "sync.receipt": "Receipt available",
      "sync.review": "Review required",
      "sync.degraded": "Sync delayed / degraded",

      // --- Action lifecycle (closed vocabulary, §"Action lifecycle") ----
      // VERBATIM. Byte-identical to the spec's closed set.
      "lifecycle.open": "Open",
      "lifecycle.openPaused": "Open but paused",
      "lifecycle.sent": "Sent — waiting for receipt",
      "lifecycle.confirmed": "Confirmed",
      "lifecycle.declined": "Declined",
      "lifecycle.closedDeadline": "Closed — deadline passed",
      "lifecycle.closedSuperseded": "Closed — superseded",
      "lifecycle.closedAuthority": "Closed — insufficient authority",

      // --- ActionCard plain-language enum maps (ADR-0027) ---------------
      "action.vote": "Cast a vote",
      "action.attend": "Confirm attendance",
      "action.complete": "Mark a task complete",
      "source.proposal": "A proposal in your domain",
      "source.meeting": "A scheduled meeting",
      "source.actionItem": "A task assigned to you",
      "source.availableSoon": "Available soon (not yet enabled)",
      "scope.entity": "Affects your whole organization",
      "scope.structure": "Affects one body within your organization (a committee or working group)",
      "scope.individual": "Affects only you",
      "risk.low": "Low impact",
      "risk.normal": "Normal impact",
      "risk.elevated": "Take extra care",

      // --- Formatting / time -------------------------------------------
      "time.noTimestamp": "(no timestamp)",
      "time.day": "day",
      "time.days": "days",
      "time.hour": "hour",
      "time.hours": "hours",
      "time.minute": "minute",
      "time.minutes": "minutes",
      "time.closesIn": "closes in about {n} {unit}",
      "time.deadlinePassed": "deadline passed about {n} {unit} ago",
      "hash.unavailable": "(unavailable)",

      // --- Identity / standing render ----------------------------------
      "identity.noDisplayName": "(no display name configured)",
      "identity.unknownDid": "(unknown)",
      "membership.member": "You are a member of this domain (you are on its member list).",
      "membership.unverified": "Your membership here comes from a participation-trust source and is shown as unverified in this view. That does not mean you are not a member — it means this view does not evaluate it.",
      "membership.other": "Membership status: {status}",
      "standing.noMemberships": "No memberships are recorded for you yet. If you expect to see one, please contact your steward.",
      "standing.noRoles": "You hold no recorded roles right now.",
      "standing.showTechnical": "Show technical detail",
      "standing.kv.domainId": "Domain id",
      "standing.kv.membershipSource": "Membership source",
      "standing.kv.status": "Status",
      "standing.structureNameUnavailable": "structure {id} (its name is currently unavailable — something may be off; you can ask your steward)",
      "standing.roleHeading": "{role} — {structure}",
      "standing.roleAuthorizes": "This role authorizes you to act in these areas: {scopes}.",
      "standing.roleNoScopes": "(none recorded)",
      "standing.heldSince": "Held since {start}",
      "standing.heldSinceEnds": "Held since {start}; ends {end}",
      "standing.kv.roleAssignmentId": "Role assignment id",
      "standing.kv.structureId": "Structure id",
      "standing.kv.parentEntity": "Parent entity",
      "standing.kv.parentEntityNone": "(none)",
      "standing.kv.authorityScopeStrings": "Authority scope strings",

      // --- Action cards render -----------------------------------------
      "cards.none": "Nothing needs your attention right now.",
      "cards.checkAgain": "Check again",
      "card.untitled": "(untitled card)",
      "card.beforeYouDecide": "Before you decide: {hint}",
      "card.kv.askedToDo": "What you are being asked to do",
      "card.action.fallback": "Act ({action})",
      "card.kv.whereFrom": "Where this comes from",
      "card.kv.whereApplies": "Where this applies",
      "card.kv.scope": "Scope",
      "card.kv.whyYouCanAct": "Why you can act here",
      "card.noAuthorityBasis": "(no authority basis given)",
      "card.kv.authorization": "Authorization",
      "card.auth.authorized": "You are authorized for this.",
      "card.auth.assignedCompletion": "The node checks your authorization when you confirm. Assigned tasks are authorized by your assignment, which this view cannot evaluate from your standing alone.",
      "card.auth.insufficient": "This requires authority you do not currently hold: {missing}.",
      "card.kv.timePressure": "Time pressure",
      "card.timePressure.none": "No time pressure.",
      "card.exactTime": "Exact time",
      "card.localTime": "{time} (your local time)",
      "card.kv.careLevel": "Care level",
      "card.kv.whatHappens": "What happens if you act",
      "card.whatHappens.receipt": "Acting will produce a receipt — a permanent record you can review in your Receipts.",
      "card.whatHappens.noReceipt": "Acting is not expected to produce a receipt.",
      "card.showTechnical": "Show technical detail",
      "card.kv.cardId": "Card id",
      "card.kv.underlyingRecordId": "Underlying record id",
      "card.kv.domainId": "Domain id",
      "card.domainIdNotCarried": "(not carried on this card)",
      "card.kv.rawKind": "Raw kind / action / scope / risk",

      // --- Complete flow (the one mutation) -----------------------------
      "complete.noDomainRef": "This card does not carry a domain reference, so this client cannot address the completion endpoint for it.",
      "complete.openButton": "Mark complete…",
      "complete.beforeConfirm": "Before you confirm",
      "complete.kv.whatChanges": "What will change",
      "complete.whatChanges.body": "This task will be recorded as completed, in your name.",
      "complete.kv.whyYouMay": "Why you may do this",
      "complete.kv.scope": "Scope",
      "complete.kv.receipt": "Receipt",
      "complete.receipt.expected": "A completion receipt will be produced and shown in your Receipts.",
      "complete.receipt.notExpected": "No receipt is expected for this action.",
      "complete.kv.canUndo": "Can this be undone?",
      "complete.canUndo.body": "The completion record is permanent once accepted. This client offers no undo; if you complete it in error, contact your steward.",
      "complete.confirmButton": "Confirm — mark complete",
      "complete.cancelButton": "Cancel",
      "complete.closeButton": "Close",
      "complete.sentFetching": "{sent} — completion accepted, fetching the receipt.",
      "complete.confirmedSeeReceipts": "{confirmed}. {receipt} — see your Receipts below.",
      "complete.syncDetail": "Your latest action produced a receipt. Records were last refreshed at {time}.",
      "complete.receiptReadFailed": "Your completion was accepted and recorded. The receipt could not be retrieved right now ({error}). Do not redo the task — refresh your Receipts later or ask your steward.",
      "complete.rejected": "Rejected with reason: {error} Nothing was recorded. You can try again or contact your steward.",
      "complete.receiptContext": "You marked a task complete.",

      // --- Receipts render ---------------------------------------------
      "receipts.none": "No receipts to show yet. When you complete an action that produces a receipt, it appears here.",
      "receipt.who.self": "You",
      "receipt.who.other": "Another member (identifier under evidence detail)",
      "receipt.actionCompleted": "Action completed",
      "receipt.markedComplete": "{who} marked a task as complete on {when}.",
      "receipt.whatThisProves": "What this proves: who acted (the actor recorded below), which obligation it satisfied (the task identified below), and when. {context}",
      "receipt.showEvidence": "Show evidence detail",
      "receipt.kv.recordClass": "Record class",
      "receipt.kv.taskId": "Task (action item) id",
      "receipt.kv.domainId": "Domain id",
      "receipt.kv.actor": "Actor",
      "receipt.kv.transition": "Transition",
      "receipt.kv.completedAt": "Completed at (unix seconds)",
      "receipt.kv.recordHashDemo": "Record hash (illustrative fixture value — not a real blake3 binding)",
      "receipt.kv.recordHashLive": "Record hash (blake3, canonical binding of the fields above)",

      // --- Demo mode flow ----------------------------------------------
      "demo.loadingFixture": "Loading the committed fixture snapshot…",
      "demo.receiptContext": "This is a fictional demonstration receipt for a previously completed task; it is not tied to any card above.",
      "demo.syncedDetail": "Fixture snapshot generated at {when}. Deadlines above are read relative to that snapshot moment, because fixture data does not move with the clock.",
      "demo.loadFailed": "The fixture files could not be loaded ({error}). Make sure the page is served from the web/ directory — see the README.",

      // --- Live mode flow ----------------------------------------------
      "live.nodeAnswered": "The node answered {status}: {reason}",
      "live.askingStanding": "Asking the node for your standing…",
      "live.syncedDetail": "Records last refreshed at {time}. This view is a snapshot; refresh to re-check.",
      "live.connected": "Connected. Your standing and open actions are shown below.",
      "live.standingUnavailable": "Your standing is currently unavailable. We'll keep trying. If this persists, please contact your steward.",
      "live.couldNotLoad": "Could not load your records. Technical detail: {error}",
      "live.invalidUrl": "That gateway address is not a valid URL.",
      "live.urlScheme": "The gateway address must start with http:// or https://.",
      "live.pasteCredential": "Paste an access credential first.",

      // --- Demo launcher flow ------------------------------------------
      "launcher.ready": "Ready. Select Start local demo to load your standing.",
      "launcher.notConnected": "Not connected yet. Enter your local gateway address above.",
      "launcher.starting": "Starting the local demo…",
      "launcher.startFailed": "Could not start the local demo ({error}). Use Connect manually instead, or check the launcher tunnel.",

      // --- #2289 organizer-steward evidence surface (fixture-only) -------
      // Renders the seven already-landed ADR-0026 Layer 2 process-transition
      // receipts as a plain-language evidence story, plus a repo-safe evidence
      // summary. Fixture/dev only; nothing signed; read-only. No readiness is
      // claimed and no new receipt class is introduced.
      "evidence.process_session_opened.heading": "Process session opened",
      "evidence.deliberation_entry_recorded.heading": "Deliberation input recorded",
      "evidence.decision_recorded.heading": "Decision recorded",
      "evidence.process_gate_result.heading": "Gate result recorded",
      "evidence.activation_crossed.heading": "Activation crossed",
      "evidence.mutation_plan_recorded.heading": "Mutation plan recorded",
      "evidence.mutation_applied.heading": "Mutation applied",

      // Deliberation redaction demo (steward-visible vs member/export-redacted).
      "evidence.redaction.stewardHeading": "What the steward body sees",
      "evidence.redaction.memberHeading": "What members and the export see",
      "evidence.redaction.notice": "Redacted here — the input text is not shown. {reason}",

      // Activation-boundary explainer (icn#2297): names the three states plainly
      // so the boundary is legible. Activation is not the change itself.
      "evidence.activation.boundaryHeading": "What crossing the activation boundary means here",
      "evidence.activation.boundary.decision": "A decision was recorded (the decision receipt above) — who recorded it, when, and a content fingerprint.",
      "evidence.activation.boundary.crossing": "That recorded decision then crossed from decision toward later action planning, with the required gate observed as passed. This receipt records only that the crossing happened.",
      "evidence.activation.boundary.deferred": "The later action planning itself — planning or applying any change — is not done here and remains deferred. Activation is the boundary, not the change; the receipt records a process fact and grants zero authority.",

      // Mutation-plan-recorded explainer (icn#2304): names the states plainly so
      // recording a plan is never read as applying it. Two later steps stay deferred.
      "evidence.mutationPlan.boundaryHeading": "What recording a mutation plan means here",
      "evidence.mutationPlan.boundary.decision": "A decision was recorded (the decision receipt above) — who recorded it, when, and a content fingerprint.",
      "evidence.mutationPlan.boundary.activation": "That recorded decision then crossed the activation boundary (the activation receipt above), with the required gate observed as passed.",
      "evidence.mutationPlan.boundary.planRecorded": "A mutation plan was then recorded — after activation, before any mutation was applied. This receipt references the activation it follows and fingerprints the plan body as a content hash. It records only that the plan record exists; it does not apply the plan, expose the private plan body, or grant authority.",
      "evidence.mutationPlan.boundary.applicationDeferred": "Applying the plan — making any change — is not done here and remains deferred. Recording the plan is the boundary, not the change.",
      "evidence.mutationPlan.boundary.evidencePacketDeferred": "Producing an evidence packet from the plan is also not done here and remains deferred.",

      // Mutation-applied explainer (icn#2311): names the states plainly so an
      // application-recorded fact is never read as executing/authorizing the
      // mutation. Evidence-packet production stays deferred.
      "evidence.mutationApplied.boundaryHeading": "What recording a mutation applied means here",
      "evidence.mutationApplied.boundary.decision": "A decision was recorded (the decision receipt above) — who recorded it, when, and a content fingerprint.",
      "evidence.mutationApplied.boundary.activation": "That recorded decision then crossed the activation boundary (the activation receipt above), with the required gate observed as passed.",
      "evidence.mutationApplied.boundary.planRecorded": "A mutation plan was then recorded (the mutation-plan receipt above) — after activation, before any mutation was applied.",
      "evidence.mutationApplied.boundary.applied": "The plan's application was then recorded here — an application fact. This receipt references the plan it applies and fingerprints the applied result as a content hash. It does not execute, authorize, validate, enforce, roll back, or prove the mutation correct; it does not expose the applied-result body; and it grants zero authority.",
      "evidence.mutationApplied.boundary.evidencePacketDeferred": "Producing an evidence packet from the application is not done here and remains deferred.",

      // Evidence-detail key/value labels for the seven receipt classes.
      "evidence.kv.domainId": "Domain id",
      "evidence.kv.sessionId": "Session id",
      "evidence.kv.openedBy": "Opened by (who recorded this fact)",
      "evidence.kv.recordedAt": "Recorded at (unix seconds)",
      "evidence.kv.entryId": "Entry id",
      "evidence.kv.author": "Author (who recorded this input)",
      "evidence.kv.entryKind": "Entry kind",
      "evidence.kv.bodyHash": "Content fingerprint (body hash — proof of content; the input itself is never stored)",
      "evidence.kv.decisionId": "Decision id",
      "evidence.kv.recordedBy": "Recorded by (who recorded this fact, not who decided)",
      "evidence.kv.gateKind": "Gate kind",
      "evidence.kv.gateResult": "Gate result",
      "evidence.kv.activationId": "Activation id",
      "evidence.kv.decisionRef": "Activated decision id (the decision this crossing names)",
      "evidence.kv.decisionRecordHash": "Activated decision record hash (proof pointer to the recorded decision — not its text)",
      "evidence.kv.gateBasis": "Gate basis (fingerprint over the passed gate-result record hashes)",
      "evidence.kv.declaredGate": "Passed gate in basis — {kind} ({result}) — record hash",
      "evidence.kv.crossedBy": "Crossed by (who recorded the crossing, not an authority to act)",
      "evidence.kv.planId": "Plan id",
      "evidence.kv.activationRef": "Referenced activation id (the activation this plan was recorded against)",
      "evidence.kv.activationRecordHash": "Referenced activation record hash (proof pointer to the activation crossing — not its text)",
      "evidence.kv.planBodyHash": "Plan content fingerprint (body hash — proof of content; the plan body is never stored)",
      "evidence.kv.planRecordedBy": "Recorded by (who recorded the plan, not the planner, approver, applier, or authority holder)",
      "evidence.kv.applicationId": "Application id",
      "evidence.kv.planRef": "Referenced plan id (the plan this application applies)",
      "evidence.kv.planRecordHash": "Referenced plan record hash (proof pointer to the mutation-plan-recorded receipt — not its body)",
      "evidence.kv.resultHash": "Applied-result fingerprint (result hash — proof of content; the applied-result body is never stored)",
      "evidence.kv.appliedBy": "Applied by (who recorded the application — recorder / apply-witness, not the applier, approver, or authority holder)",
      "evidence.kv.appliedAt": "Applied at (unix seconds)",

      // Evidence summary / export (read-only render of the committed fixture).
      "evidence.export.heading": "Evidence summary",
      "evidence.export.explain": "This is a repo-safe evidence summary of the process story above, rendered read-only from a committed fixture. It is not generated, downloaded, copied, or sent from this page.",
      "evidence.export.summary": "This evidence summary describes a {mode} rehearsal and is classified {safety}.",
      "evidence.export.readonly": "Read-only view of a committed fixture — the summary is not generated, downloaded, copied, or sent from this page.",
      "evidence.export.outcomesHeading": "What the rehearsal recorded (plain language)",
      "evidence.export.privacy": "Privacy review: {status}. {notes}",
      "evidence.export.showDetail": "Show export detail",
      "evidence.export.kv.contract": "Contract",
      "evidence.export.kv.mode": "Rehearsal mode",
      "evidence.export.kv.steps": "Workflow steps completed",
      "evidence.export.kv.audiences": "Audience categories",
      "evidence.export.kv.mutation": "Mutation",
      "evidence.export.mutationNone": "None (read-only; nothing was changed)",
      "evidence.export.kv.accessibility": "Accessibility review status",
      "evidence.export.kv.safety": "Export safety classification",
      "evidence.export.nonClaimsHeading": "What this is not"
    }
    // Additional languages are added here as a single entry — no code change.
    // Example: MESSAGES.fr = { "sync.synced": "Synchronisé", ... }
    // A partial catalog is fine: missing keys fall back to `en` per t().
  };

  // -------------------------------------------------------------------------
  // Locale metadata. `ar` ships with NO translations on purpose: it
  // demonstrates dir=rtl mirroring plus the per-key translation-pending
  // fallback to English. `qps-ploc` is the pseudo-locale coverage test.
  // -------------------------------------------------------------------------
  var LOCALES = {
    en: { name: "English", dir: "ltr" },
    // `qps-ploc` is the conventional pseudo-locale KEY (used in ?lang= and the
    // selector). It is not a valid BCP-47 language tag, so the document `lang`
    // attribute emits the valid private-use form `en-x-ploc` (via htmlLang) to
    // keep the HTML lang valid for assistive tech and axe `html-lang-valid`.
    "qps-ploc": { name: "Pseudo-locale (i18n coverage test)", dir: "ltr", pseudo: true, htmlLang: "en-x-ploc" },
    // `ar` demonstrates the dir=rtl switch + the translation-pending fallback.
    // Its catalog is intentionally empty, so member text falls back to English.
    // The document `lang` therefore stays `en` (htmlLang override) so screen
    // readers and browser translation handle the actually-displayed English as
    // English, not as Arabic — RTL layout is shown via `dir` without
    // mislabeling the fallback prose. When real `ar` strings are added to
    // MESSAGES.ar, drop the htmlLang override so `lang` becomes `ar`.
    ar: { name: "العربية (translations pending — RTL demo)", dir: "rtl", htmlLang: "en" }
  };

  // Empty (no translations) catalogs for locales declared in LOCALES but not
  // present in MESSAGES, so every active locale resolves cleanly through the
  // en fallback. qps-ploc derives from en at render time (see pseudo()).
  if (!MESSAGES["qps-ploc"]) { MESSAGES["qps-ploc"] = {}; }
  if (!MESSAGES.ar) { MESSAGES.ar = {}; }

  // -------------------------------------------------------------------------
  // Active locale resolution:
  //   1. ?lang= query param (exact locale key)
  //   2. navigator.language, matched by prefix against available locales
  //   3. "en"
  // -------------------------------------------------------------------------
  function availableLocales() {
    return Object.keys(LOCALES);
  }

  function resolveLocale() {
    var avail = availableLocales();
    var params = new URLSearchParams(window.location.search);
    var q = params.get("lang");
    if (q && avail.indexOf(q) !== -1) { return q; }

    var nav = (navigator.language || "").toLowerCase();
    if (nav) {
      // Exact match first, then language-prefix match (e.g. "ar-EG" → "ar").
      for (var i = 0; i < avail.length; i++) {
        if (avail[i].toLowerCase() === nav) { return avail[i]; }
      }
      var prefix = nav.split("-")[0];
      for (var j = 0; j < avail.length; j++) {
        if (avail[j].toLowerCase().split("-")[0] === prefix) { return avail[j]; }
      }
    }
    return "en";
  }

  var ACTIVE = resolveLocale();

  // -------------------------------------------------------------------------
  // Pseudo-locale transform. Wraps the resolved English string in ⟦…⟧ and
  // accents vowels so any externalized string is visibly flipped. Plain
  // English left on screen under ?lang=qps-ploc is a missed extraction.
  // Placeholders ({name}) are preserved untouched so interpolation still works.
  // -------------------------------------------------------------------------
  var ACCENT = { a: "á", e: "é", i: "í", o: "ó", u: "ú", A: "Á", E: "É", I: "Í", O: "Ó", U: "Ú" };
  function pseudo(str) {
    var out = "";
    var inPlaceholder = false;
    for (var i = 0; i < str.length; i++) {
      var ch = str[i];
      if (ch === "{") { inPlaceholder = true; out += ch; continue; }
      if (ch === "}") { inPlaceholder = false; out += ch; continue; }
      out += (!inPlaceholder && ACCENT[ch]) ? ACCENT[ch] : ch;
    }
    return "⟦" + out + "⟧";
  }

  // -------------------------------------------------------------------------
  // Interpolate {param} placeholders from params. Missing params are left as
  // the literal placeholder so the gap is visible rather than silently blank.
  // -------------------------------------------------------------------------
  function interpolate(str, params) {
    if (!params) { return str; }
    return str.replace(/\{(\w+)\}/g, function (match, key) {
      return Object.prototype.hasOwnProperty.call(params, key)
        ? String(params[key]) : match;
    });
  }

  // -------------------------------------------------------------------------
  // t(key, params): active locale → en fallback → key itself. Never throws,
  // never returns blank. For qps-ploc, transform the resolved English string.
  // -------------------------------------------------------------------------
  function lookup(locale, key) {
    var cat = MESSAGES[locale];
    if (cat && Object.prototype.hasOwnProperty.call(cat, key)) { return cat[key]; }
    return null;
  }

  function t(key, params) {
    var locale = ACTIVE;

    if (LOCALES[locale] && LOCALES[locale].pseudo) {
      // Pseudo-locale derives from the English catalog so coverage is exact:
      // any key present in `en` is flipped; a missing key falls through to the
      // key itself (still visibly bracketed so the gap is obvious).
      var enVal = lookup("en", key);
      if (enVal !== null) { return interpolate(pseudo(enVal), params); }
      return pseudo(key);
    }

    var val = lookup(locale, key);
    if (val === null && locale !== "en") { val = lookup("en", key); }  // translation-pending fallback
    if (val === null) { return key; }  // never throw, never blank
    return interpolate(val, params);
  }

  // -------------------------------------------------------------------------
  // applyDocumentLocale: set <html lang> and <html dir> from LOCALES metadata.
  // -------------------------------------------------------------------------
  function applyDocumentLocale(locale) {
    var meta = LOCALES[locale] || LOCALES.en;
    // Emit a valid BCP-47 tag for the document lang. Most locale keys are
    // already valid tags; the pseudo-locale carries an explicit htmlLang
    // override (its key `qps-ploc` is not a valid BCP-47 tag).
    document.documentElement.lang = meta.htmlLang || locale;
    document.documentElement.dir = meta.dir || "ltr";
  }

  window.ICNI18n = {
    t: t,
    locale: ACTIVE,
    resolveLocale: resolveLocale,
    applyDocumentLocale: applyDocumentLocale,
    availableLocales: availableLocales,
    localeMeta: function (locale) { return LOCALES[locale] || null; },
    MESSAGES: MESSAGES,
    LOCALES: LOCALES
  };
})();
