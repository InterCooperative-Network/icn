#!/usr/bin/env bash
# ============================================================================
# icn-demo-verify — evidence/audit step of the appliance demo loop.
# ----------------------------------------------------------------------------
# DEMO PROFILE ONLY. Installed by build-image.sh when
# ICN_APPLIANCE_DEMO_PROFILE=1. Run as root inside the appliance VM.
#
# Two evidence paths, both honest about what they prove:
#
#   sudo icn-demo-verify <item-id> [domain]
#       Re-fetches the completion receipt for that action item from THIS
#       VM's gateway and runs a CONSISTENCY CHECK: record_hash has the
#       32-byte shape and the receipt fields (item_id, domain_id,
#       transition, completed_at) match what was asked for. It does NOT
#       re-derive the BLAKE3 record_hash — the canonical hasher lives in
#       icn-governance proof.rs, and cryptographic verification of the
#       chain is what `--chain` (icnctl audit verify) does.
#       Does not prove production, federation, or pilot adoption.
#
#   sudo icn-demo-verify --chain
#       Runs the bundled 13/13 governed receipt-chain rehearsal
#       (scripts/local_receipt_chain_13of13_rehearsal.sh) against a FRESH
#       ephemeral node on loopback :18080 inside this VM. Asserts
#       `icnctl audit verify` reports a complete 13/13 receipt chain and
#       emits a schema-validated, repo-safe evidence packet.
#       Output: /var/lib/icn-demo/receipt-chain-13of13/
#
#   sudo icn-demo-verify --pending-publish
#       Steward-verifies the pending-publish review-preview evidence packet:
#       maps the committed pending-publish rows into a
#       urn:icn:contract:rehearsal-evidence-export:v1 packet and validates it
#       (fixture-only, offline, no gateway, no writes; fails closed on drift).
#       Output: /var/lib/icn-demo/pending-publish-evidence/
#
#   sudo icn-demo-verify --rehearsal [domain]     (#2386)
#       Steward-verifies the Rehearsal Node organizer->member loop against THIS
#       VM's live rehearsal-mode gateway (run after
#       `sudo icn-demo-seed --session organizer`): the least-privilege capability
#       matrix (organizer read+review+confirm, canNOT bind/broad-write/complete;
#       member read+complete, canNOT review/bind), one action item created by
#       confirmation, the member completion receipt, the process-receipt ladder,
#       and the value-withheld evidence packet (no DID / no credential). Drives
#       the loop itself when the browser has not (idempotent on an executed row).
#       Fictional data on an isolated node; not production, not a pilot.
# ============================================================================
set -uo pipefail

GW="${ICN_DEMO_GW:-http://127.0.0.1:8080}"
DATA_DIR=/var/lib/icn
ENV_FILE=/etc/icn/icnd.env
REPO=/usr/share/icn/demo/repo
COOP_ID="${ICN_DEMO_COOP_ID:-nycn}"

log(){ echo "[demo-verify] $*"; }
fatal(){ echo "[demo-verify] FATAL: $*" >&2; exit 1; }
[ "$(id -u)" -eq 0 ] || fatal "run as root: sudo icn-demo-verify"

if [ "${1:-}" = "--chain" ]; then
  RUNNER="$REPO/scripts/local_receipt_chain_13of13_rehearsal.sh"
  [ -f "$RUNNER" ] || fatal "bundled 13/13 rehearsal missing at $RUNNER"
  OUT=/var/lib/icn-demo
  mkdir -p "$OUT"
  # The rehearsal wrapper's binary gate only looks in $ROOT/icn/target/
  # (its ICND/ICNCTL env overrides apply to the inner script, not the
  # gate), so expose the installed binaries at the expected location.
  mkdir -p "$REPO/icn/target/release"
  ln -sf /usr/local/bin/icnd "$REPO/icn/target/release/icnd"
  ln -sf /usr/local/bin/icnctl "$REPO/icn/target/release/icnctl"
  log "running the 13/13 governed receipt-chain rehearsal (fresh ephemeral node, loopback :18080)..."
  log "this is the real proof path; it takes a few minutes on small VMs."
  ICND=/usr/local/bin/icnd ICNCTL=/usr/local/bin/icnctl ICN_GW_PORT=18080 \
    bash "$RUNNER" --fresh --no-build
  rc=$?
  if [ -d "$REPO/demo/output/receipt-chain-13of13" ]; then
    rm -rf "$OUT/receipt-chain-13of13"
    cp -r "$REPO/demo/output/receipt-chain-13of13" "$OUT/" 2>/dev/null || true
    log "evidence copied to $OUT/receipt-chain-13of13/"
  fi
  exit "$rc"
fi

if [ "${1:-}" = "--pending-publish" ]; then
  # Steward verification of the pending-publish review-preview evidence packet.
  # Fixture-only + offline: maps the committed pending-publish rows into a
  # urn:icn:contract:rehearsal-evidence-export:v1 packet and validates it.
  # No gateway, no network egress, no custody/gateway writes; the only writes are
  # local repo-safe evidence artifacts under /var/lib/icn-demo/. Fails closed on drift.
  GEN="$REPO/scripts/rehearsal_pending_publish_evidence.py"
  [ -f "$GEN" ] || fatal "bundled pending-publish evidence generator missing at $GEN"
  command -v python3 >/dev/null || fatal "python3 missing"
  OUT=/var/lib/icn-demo
  mkdir -p "$OUT"
  log "verifying the committed pending-publish evidence packet (determinism + :v1 schema, fixture-only, no network)..."
  python3 "$GEN" --check || fatal "committed pending-publish evidence packet failed the determinism/drift + :v1 schema guard"
  log "generating a fresh timestamped packet + re-validating..."
  python3 "$GEN" --write || fatal "pending-publish evidence packet failed generation/validation"
  if [ -d "$REPO/demo/output/pending-publish-evidence" ]; then
    rm -rf "$OUT/pending-publish-evidence"
    cp -r "$REPO/demo/output/pending-publish-evidence" "$OUT/" 2>/dev/null || true
    log "evidence copied to $OUT/pending-publish-evidence/"
  fi
  log "OK: pending-publish evidence packet validates against urn:icn:contract:rehearsal-evidence-export:v1"
  exit 0
fi

if [ "${1:-}" = "--rehearsal" ]; then
  # Steward verification of the Rehearsal Node organizer->member loop (#2386):
  # the least-privilege capability matrix + the end-to-end loop + value-withheld
  # evidence, against THIS VM's live rehearsal-mode gateway. Run after
  # `sudo icn-demo-seed --session organizer`. Fail-closed (first mismatch aborts).
  DOMAIN="${2:-nycn-federation-gov}"
  [ -f "$ENV_FILE" ] || fatal "$ENV_FILE missing — has firstboot run?"
  command -v python3 >/dev/null || fatal "python3 missing"
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  [ -n "${ICN_KEYSTORE_PASSPHRASE:-}" ] || fatal "ICN_KEYSTORE_PASSPHRASE not present in $ENV_FILE"
  [ -n "${ICN_GATEWAY_JWT_SECRET:-}" ] || fatal "ICN_GATEWAY_JWT_SECRET not present in $ENV_FILE — needed for trusted local JWT issuance (icnctl --local-mint)"
  ICN_PASSPHRASE="$ICN_KEYSTORE_PASSPHRASE"
  ICN_CHILD_ENV_KEEP="ICN_KEYSTORE_PASSPHRASE ICN_PASSPHRASE ICN_GATEWAY_JWT_SECRET LANG LC_ALL LC_CTYPE TERM"

  # Trusted-local mint (same env-scrub + runuser discipline as the read-JWT below).
  mint_jwt(){  # $1=scopes -> bare JWT
    (
      for _name in $(compgen -e); do
        case " $ICN_CHILD_ENV_KEEP " in *" $_name "*) : ;; *) unset "$_name" 2>/dev/null || true ;; esac
      done
      export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
      export ICN_KEYSTORE_PASSPHRASE ICN_PASSPHRASE ICN_GATEWAY_JWT_SECRET
      runuser -u icn -- /usr/local/bin/icnctl --data-dir "$DATA_DIR" auth token --gateway "$GW" --coop-id "$COOP_ID" -s "$1" --local-mint 2>/dev/null # vocab-ok: icnctl CLI subcommand name
    ) | grep -oE 'eyJ[A-Za-z0-9_.-]+' | head -1
  }
  rstatus(){  # $1=METHOD $2=PATH $3=JWT [$4=BODY] -> HTTP code
    if [ -n "${4:-}" ]; then
      curl -s -o /dev/null -m 10 -w '%{http_code}' -X "$1" -H "Authorization: Bearer $3" -H 'Content-Type: application/json' -d "$4" "$GW$2"
    else
      curl -s -o /dev/null -m 10 -w '%{http_code}' -X "$1" -H "Authorization: Bearer $3" "$GW$2"
    fi
  }
  rbody(){    # $1=METHOD $2=PATH $3=JWT [$4=BODY] -> body
    if [ -n "${4:-}" ]; then
      curl -s -m 10 -X "$1" -H "Authorization: Bearer $3" -H 'Content-Type: application/json' -d "$4" "$GW$2"
    else
      curl -s -m 10 -X "$1" -H "Authorization: Bearer $3" "$GW$2"
    fi
  }
  expect(){   # $1=desc $2=want $3=got
    [ "$3" = "$2" ] && log "  OK   $1 (HTTP $3)" || fatal "$1 — expected HTTP $2, got $3"
  }

  ORG_JWT="$(mint_jwt "governance:read,governance:pending-publish:review,governance:pending-publish:confirm")"
  MEM_JWT="$(mint_jwt "governance:read,governance:action-item:complete")"
  { [ -n "$ORG_JWT" ] && [ -n "$MEM_JWT" ]; } || fatal "trusted local token mint failed (icnctl --local-mint). Check ICN_GATEWAY_JWT_SECRET + keystore."
  log "minted organizer / member tokens (trusted-local; never printed). Setup/init is the seed's job."

  s="$(rstatus GET "/v1/gov/domains/$DOMAIN/rehearsal/pending-publish" "$ORG_JWT")"
  [ "$s" = "200" ] || fatal "rehearsal workspace not reachable (HTTP $s) — is icnd in ICN_GOVERNANCE_BUILD_MODE=rehearsal and did 'icn-demo-seed --session organizer' run?"
  log "rehearsal routes mounted; organizer reads the workspace (HTTP 200)."

  # ---- least-privilege NEGATIVE matrix ----
  probe_bind="$(python3 -c 'import json;print(json.dumps({"label":"probe-x","did":"did:icn:probe"}))')"
  expect "organizer canNOT bind labels (setup-only)"          403 "$(rstatus POST "/v1/gov/domains/$DOMAIN/rehearsal/bindings" "$ORG_JWT" "$probe_bind")"
  expect "organizer canNOT create action items (broad write)" 403 "$(rstatus POST "/v1/gov/domains/$DOMAIN/action-items" "$ORG_JWT" '{"title":"x","description":"x","priority":"low"}')"
  expect "member canNOT bind labels"                          403 "$(rstatus POST "/v1/gov/domains/$DOMAIN/rehearsal/bindings" "$MEM_JWT" "$probe_bind")"

  # ---- locate the fictional action_item row ----
  list="$(rbody GET "/v1/gov/domains/$DOMAIN/rehearsal/pending-publish" "$ORG_JWT")"
  ROW="$(python3 -c 'import json,sys
d=json.load(sys.stdin)
for r in d.get("rows",[]):
    if r.get("kind")=="action_item":
        print(r["id"]); break' <<<"$list")"
  [ -n "$ROW" ] || fatal "no action_item row in the rehearsal workspace: $list"
  expect "member canNOT review a row" 403 "$(rstatus POST "/v1/gov/domains/$DOMAIN/rehearsal/pending-publish/$ROW/review" "$MEM_JWT" '{"decision":"approve"}')"

  # ---- ensure the row is confirmed (drive it if the browser has not) ----
  detail="$(rbody GET "/v1/gov/domains/$DOMAIN/rehearsal/pending-publish/$ROW" "$ORG_JWT")"
  AI_ID="$(python3 -c 'import json,sys
d=json.load(sys.stdin).get("row",{})
print((d.get("execution") or {}).get("action_item_id","") if d.get("executed") else "")' <<<"$detail")"
  if [ -z "$AI_ID" ]; then
    expect "organizer approves the fictional item" 200 "$(rstatus POST "/v1/gov/domains/$DOMAIN/rehearsal/pending-publish/$ROW/review" "$ORG_JWT" '{"decision":"approve"}')"
    preview="$(rbody GET "/v1/gov/domains/$DOMAIN/rehearsal/pending-publish/$ROW/preview" "$ORG_JWT")"
    digest="$(python3 -c 'import json,sys;print(json.load(sys.stdin).get("preview_digest",""))' <<<"$preview")"
    [ -n "$digest" ] || fatal "preview returned no digest (is the assignee label bound? run 'icn-demo-seed --session organizer'): $preview"
    confirm="$(rbody POST "/v1/gov/domains/$DOMAIN/rehearsal/pending-publish/$ROW/confirm" "$ORG_JWT" "$(python3 -c 'import json,sys;print(json.dumps({"preview_digest":sys.argv[1]}))' "$digest")")"
    AI_ID="$(python3 -c 'import json,sys;print(json.load(sys.stdin).get("action_item_id",""))' <<<"$confirm")"
    [ -n "$AI_ID" ] || fatal "confirm did not create an action item: $confirm"
    log "organizer confirmed -> created one real action item: $AI_ID"
  else
    log "action item already confirmed (browser walkthrough): $AI_ID"
  fi

  # ---- completion: organizer canNOT; member CAN (if not already completed) ----
  crec="$(rbody GET "/v1/gov/domains/$DOMAIN/action-items/$AI_ID/completion-receipt" "$MEM_JWT")"
  already="$(python3 -c 'import json,sys
try:
    print("1" if json.load(sys.stdin).get("transition")=="completed" else "")
except Exception:
    print("")' <<<"$crec")"
  if [ -z "$already" ]; then
    expect "organizer canNOT complete the member item" 403 "$(rstatus PUT "/v1/gov/domains/$DOMAIN/action-items/$AI_ID/status" "$ORG_JWT" '{"status":"completed"}')"
    expect "member completes the assigned item"        200 "$(rstatus PUT "/v1/gov/domains/$DOMAIN/action-items/$AI_ID/status" "$MEM_JWT" '{"status":"completed"}')"
    crec="$(rbody GET "/v1/gov/domains/$DOMAIN/action-items/$AI_ID/completion-receipt" "$MEM_JWT")"
  else
    log "item already completed (browser walkthrough)."
  fi

  # ---- completion receipt binding check (32-byte record_hash + fields) ----
  if ! RECEIPT_JSON="$crec" python3 - "$AI_ID" "$DOMAIN" <<'PY'
import json,os,sys
r=json.loads(os.environ["RECEIPT_JSON"])
item,dom=sys.argv[1],sys.argv[2]
h=r.get("record_hash")
ok=(isinstance(h,list) and len(h)==32 and all(isinstance(b,int) and 0<=b<=255 for b in h)
    and r.get("item_id")==item and r.get("domain_id")==dom
    and r.get("transition")=="completed"
    and isinstance(r.get("completed_at"),(int,float)) and r.get("completed_at")>0)
sys.exit(0 if ok else 1)
PY
  then fatal "completion receipt failed the 32-byte binding check: $crec"; fi
  log "member completion receipt validates (32-byte record_hash + field binding)."

  # ---- process receipts + value-withheld evidence (no DID / no credential) ----
  receipts="$(rbody GET "/v1/gov/domains/$DOMAIN/rehearsal/receipts" "$ORG_JWT")"
  evidence="$(rbody GET "/v1/gov/domains/$DOMAIN/rehearsal/evidence-export" "$ORG_JWT")"
  if ! RC="$receipts" EV="$evidence" python3 - <<'PY'
import json,os,re,sys
rc=json.loads(os.environ["RC"]); ev=json.loads(os.environ["EV"])
classes={x.get("class") for x in rc.get("receipts",[])}
for need in ("process_gate_result","activation_crossed","mutation_plan_recorded","mutation_applied"):
    if need not in classes: sys.exit("process receipts missing "+need)
for k in ("contract","run_id","generation","rows","packet_hash","packet_hash_sha256","non_claims","privacy_review_result"):
    if k not in ev: sys.exit("evidence missing "+k)
if ev["contract"]!="urn:icn:contract:rehearsal-workflow-evidence:v1": sys.exit("evidence wrong contract")
blob=json.dumps(rc)+json.dumps(ev)
if re.search(r'did:icn:',blob): sys.exit("receipts/evidence leaked a did:icn:")
if re.search(r'eyJ[A-Za-z0-9_-]{20,}\.',blob): sys.exit("receipts/evidence leaked a JWT")
pr=ev.get("privacy_review",{})
if pr.get("dids_exported") or pr.get("credentials_exported"): sys.exit("privacy_review flags a leak")
PY
  then fatal "process receipts / evidence failed validation or the value-withheld (no-DID / no-credential) check"; fi
  log "process receipts (gate/activation/plan/applied) + value-withheld evidence validate (no DID, no credential)."

  log "PASS — Rehearsal Node organizer->member loop verified end-to-end on THIS VM:"
  log "  least-privilege matrix (organizer read+review+confirm, canNOT bind/broad-write/complete;"
  log "  member read+complete, canNOT review/bind); one action item created by confirm; member"
  log "  completion receipt + process receipts + value-withheld evidence validate."
  log "  Fictional data on an isolated node. NOT production, NOT a pilot, NOT federation;"
  log "  receipts record process facts and grant no authority."
  exit 0
fi

ITEM_ID="${1:-}"
DOMAIN="${2:-nycn-federation-gov}"
[ -n "$ITEM_ID" ] || fatal "usage: icn-demo-verify <item-id> [domain]   (or --chain, or --pending-publish)"
[ -f "$ENV_FILE" ] || fatal "$ENV_FILE missing — has firstboot run?"
command -v python3 >/dev/null || fatal "python3 missing"

# shellcheck disable=SC1090
. "$ENV_FILE"
[ -n "${ICN_KEYSTORE_PASSPHRASE:-}" ] || fatal "ICN_KEYSTORE_PASSPHRASE not present in $ENV_FILE"
# The read-JWT is minted by TRUSTED LOCAL issuance (--local-mint), signing with
# THIS VM's own instance-local gateway secret (mode 0600 owned icn:icn). It does
# NOT use the public self-asserted /auth/verify flow, which stays fail-closed on
# the demo's routable 0.0.0.0 bind (#2075) — so the mint needs the secret present.
[ -n "${ICN_GATEWAY_JWT_SECRET:-}" ] || fatal "ICN_GATEWAY_JWT_SECRET not present in $ENV_FILE — needed for trusted local read-JWT issuance (icnctl --local-mint)"
ICN_PASSPHRASE="$ICN_KEYSTORE_PASSPHRASE"

# Allowlist for the icn child process (see icn-demo-seed.sh as_icn() for the full
# rationale): strip the inherited root environment down to just what icnctl needs,
# then drop privilege with runuser (NOT sudo — sudo journals its command+env,
# leaking the passphrase). Secrets travel via the ENVIRONMENT, never argv, so they
# never reach ps/journal. A governance:read JWT is the least privilege that reads
# a completion receipt.
ICN_CHILD_ENV_KEEP="ICN_KEYSTORE_PASSPHRASE ICN_PASSPHRASE ICN_GATEWAY_JWT_SECRET LANG LC_ALL LC_CTYPE TERM"
SESSION_JWT="$(
  (
    for _name in $(compgen -e); do
      case " $ICN_CHILD_ENV_KEEP " in
        *" $_name "*) : ;;
        *) unset "$_name" 2>/dev/null || true ;;
      esac
    done
    export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    export ICN_KEYSTORE_PASSPHRASE ICN_PASSPHRASE ICN_GATEWAY_JWT_SECRET
    runuser -u icn -- /usr/local/bin/icnctl --data-dir "$DATA_DIR" auth token --gateway "$GW" --coop-id "$COOP_ID" -s "governance:read" --local-mint 2>/dev/null # vocab-ok: icnctl CLI subcommand name
  ) | grep -oE 'eyJ[A-Za-z0-9_.-]+' | head -1
)"
[ -n "$SESSION_JWT" ] || fatal "could not mint a read JWT via trusted local issuance (icnctl --local-mint). Check ICN_GATEWAY_JWT_SECRET is present in $ENV_FILE and the keystore unlocks."

receipt="$(curl -s -m 10 -H "Authorization: Bearer $SESSION_JWT" \
  "$GW/v1/gov/domains/$DOMAIN/action-items/$ITEM_ID/completion-receipt")"
echo "$receipt" | python3 -m json.tool 2>/dev/null || fatal "receipt fetch failed: $receipt"

if ! RECEIPT_JSON="$receipt" python3 - "$ITEM_ID" "$DOMAIN" <<'PY'
import json
import os
import sys

r = json.loads(os.environ["RECEIPT_JSON"])
item, dom = sys.argv[1], sys.argv[2]
h = r.get("record_hash")
ok = (
    isinstance(h, list)
    and len(h) == 32
    and all(isinstance(b, int) and 0 <= b <= 255 for b in h)
    and r.get("item_id") == item
    and r.get("domain_id") == dom
    and r.get("transition") == "completed"
    and isinstance(r.get("completed_at"), (int, float))
    and r.get("completed_at") > 0
)
sys.exit(0 if ok else 1)
PY
then
  fatal "receipt failed the binding check (32-byte record_hash over item/domain/actor/transition/time)"
fi

log "PASS — receipt re-fetched; consistency check OK (32-byte record_hash shape +"
log "field binding to this item/domain/transition). NOT a BLAKE3 re-derivation —"
log "cryptographic verification is the --chain path (icnctl audit verify)."
log "Does NOT prove: production deployment, federation, or pilot adoption."
log "Deeper proof: sudo icn-demo-verify --chain   (13/13 governed receipt chain)"
