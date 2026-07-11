#!/usr/bin/env bash
# lib-seed-diag.sh — shared, unit-testable helper for the appliance demo smoke.
#
# Sourced by smoke/smoke-local.sh (the witness harness) and exercised directly by
# smoke/seed-redaction-check.sh (a VM-free regression test). Kept separate so the
# redaction behaviour can be tested without booting a VM.

# seed_incomplete_diag <seed_json> <missing_field_names>
#
# Emit a REDACTION-SAFE diagnostic for an incomplete or malformed
# `icn-demo-seed --json` payload. The seed payload carries the browser session
# JWT, so it is NEVER echoed. The message reports which required fields are
# missing/empty and whether the payload parsed as JSON at all — enough to
# diagnose the failure without reproducing the authentication JWT.
seed_incomplete_diag() {
    local seed_json="$1" missing="$2"
    if printf '%s' "$seed_json" | jq -e . >/dev/null 2>&1; then
        printf 'seed output incomplete — missing/empty field(s):%s (payload parsed as JSON; JWT withheld from logs)' "$missing"
    else
        printf 'seed output was not valid JSON (%s bytes; content withheld — it may contain a JWT)' \
            "$(printf '%s' "$seed_json" | wc -c | tr -d ' ')"
    fi
}
