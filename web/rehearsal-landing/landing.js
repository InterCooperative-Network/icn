/* ICN Rehearsal Node — landing page behavior.
 *
 * Read-only: this page NEVER handles a credential. It calls two unauthenticated,
 * non-secret endpoints on its own origin:
 *   GET /v1/dev/demo/status  — sanitized counts (gateway health, workspace
 *                              generation, rows/receipts totals)
 *   GET /build-info.json     — build provenance staged into the image
 * Session minting happens only inside the member shell's launcher flow.
 */
(function () {
  "use strict";

  function byId(id) { return document.getElementById(id); }

  function fetchJson(url, timeoutMs) {
    var ctl = new AbortController();
    var timer = setTimeout(function () { ctl.abort(); }, timeoutMs || 6000);
    return fetch(url, { signal: ctl.signal, cache: "no-store" })
      .then(function (resp) {
        if (!resp.ok) { throw new Error("HTTP " + resp.status); }
        return resp.json();
      })
      .finally(function () { clearTimeout(timer); });
  }

  function setChip(cls, text) {
    var chip = byId("health-chip");
    chip.className = "chip " + cls;
    chip.textContent = text;
  }

  function describeWorkspace(status) {
    var ws = status && status.workspace;
    if (!ws) {
      return "Rehearsal workspace: not reported (it may not be initialized yet — starting a rehearsal initializes it).";
    }
    if (ws.initialized === false) {
      return "Rehearsal workspace: not initialized yet. “Start a new rehearsal” will initialize it.";
    }
    var parts = [];
    if (ws.generation !== null && ws.generation !== undefined) {
      parts.push("generation " + ws.generation);
    }
    if (typeof ws.rows_total === "number") {
      parts.push(ws.rows_total + " item" + (ws.rows_total === 1 ? "" : "s"));
    }
    var by = ws.rows_by_status || {};
    var buckets = Object.keys(by).sort().map(function (k) { return by[k] + " " + k; });
    if (buckets.length) { parts.push("(" + buckets.join(", ") + ")"); }
    var receipts = status.receipts && typeof status.receipts.total === "number"
      ? status.receipts.total + " process receipt" + (status.receipts.total === 1 ? "" : "s")
      : null;
    var line = "Rehearsal workspace: " + (parts.length ? parts.join(", ") : "initialized");
    if (receipts) { line += " · " + receipts; }
    return line + ".";
  }

  function loadStatus() {
    setChip("neutral", "⏳ Checking services…");
    byId("status-error").hidden = true;
    byId("status-retry").hidden = true;
    fetchJson("/v1/dev/demo/status", 20000).then(function (status) {
      byId("diag-status").textContent = JSON.stringify(status, null, 2);
      if (status.gateway_health === "ok") {
        setChip("ok", "✓ Services available");
        byId("workspace-line").textContent = describeWorkspace(status);
      } else {
        setChip("warn", "✗ Gateway unavailable");
        byId("workspace-line").textContent = "";
        showError("The rehearsal gateway is not answering. If the appliance " +
          "just started, give it a minute and check again.");
      }
    }).catch(function (err) {
      setChip("warn", "✗ Services unavailable");
      byId("workspace-line").textContent = "";
      byId("diag-status").textContent = "status request failed: " + err.message;
      showError("This page could not reach the rehearsal services (" +
        err.message + "). If the appliance just started, give it a minute " +
        "and check again. Otherwise see Diagnostics below.");
    });
  }

  function showError(msg) {
    var e = byId("status-error");
    e.textContent = msg;
    e.hidden = false;
    byId("status-retry").hidden = false;
  }

  function loadBuildInfo() {
    fetchJson("/build-info.json", 6000).then(function (info) {
      byId("diag-build").textContent = JSON.stringify(info, null, 2);
      var commit = info.git_commit ? String(info.git_commit).slice(0, 8) : "unknown";
      var when = info.build_timestamp || "unknown time";
      byId("build-line").textContent =
        "Build: commit " + commit + " · built " + when +
        (info.demo_profile ? " · demo profile" : "");
    }).catch(function (err) {
      byId("diag-build").textContent = "build-info request failed: " + err.message;
      byId("build-line").textContent = "Build: provenance file not available on this origin.";
    });
  }

  byId("status-retry").addEventListener("click", loadStatus);
  loadStatus();
  loadBuildInfo();
})();
