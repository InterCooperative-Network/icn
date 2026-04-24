# NYCN Bootstrap Runbook

Live-validated: 2026-04-24 against `icnd` debug binary on port 8085.

## Prerequisites

1. **Built binary** — structures and activities routes were added to `configure.rs` on
   2026-04-19. Build must be at least this date:
   ```bash
   cd icn
   cargo build -p icnd -p icnctl
   ```

2. **Fresh data directory with identity** — the gateway needs a keystore and a JWT secret.
   On first use (or to reset state):
   ```bash
   mkdir -p /tmp/icn-bootstrap-test2
   # Generate a JWT secret (min 32 bytes)
   openssl rand -base64 32 > /tmp/icn-bootstrap-test2/jwt-secret.txt
   # Create a new identity (or copy an existing identity.age)
   ICN_KEYSTORE_PASSPHRASE='' icnd --data-dir /tmp/icn-bootstrap-test2 id init
   ```
   Use `ICN_KEYSTORE_PASSPHRASE=''` for keystores with no passphrase.

3. **Ports** — port 8080 may be reserved by WSL/Hyper-V; prefer port 8085.

## Step 1 — Start the gateway

Write a launcher script (avoids command-substitution issues in `wsl -d` context):

```bash
cat > /tmp/icn-bootstrap-test2/run-gateway.sh <<'EOF'
#!/bin/bash
export ICN_KEYSTORE_PASSPHRASE=""
export ICN_GATEWAY_JWT_SECRET="$(cat /tmp/icn-bootstrap-test2/jwt-secret.txt)"
exec /path/to/icn/target/debug/icnd \
  --data-dir /tmp/icn-bootstrap-test2 \
  --gateway-enable \
  --gateway-bind 127.0.0.1:8085 \
  --log-level warn
EOF
chmod +x /tmp/icn-bootstrap-test2/run-gateway.sh
nohup /tmp/icn-bootstrap-test2/run-gateway.sh >> /tmp/icn-bootstrap-test2/icnd.log 2>&1 &
```

Confirm it is up:
```bash
curl -s http://127.0.0.1:8085/v1/health
# → {"status":"ok","version":"0.1.0"}
```

## Step 2 — Validate the package

```bash
cd icn
ICN_KEYSTORE_PASSPHRASE='' ./target/debug/icnctl \
  --data-dir /tmp/icn-bootstrap-test2 \
  institution bootstrap validate \
  --package ../institutions/nycn
```

Expected: `valid` with 4 warnings about role assignments that have no DID yet (expected — roles are deferred).

## Step 3 — Review the plan

```bash
ICN_KEYSTORE_PASSPHRASE='' ./target/debug/icnctl \
  --data-dir /tmp/icn-bootstrap-test2 \
  institution bootstrap plan \
  --package ../institutions/nycn
```

Expected: 22 operations — 2 entities, 2 governance domains, 7 structures, 1 program,
1 activity, 4 milestones, 4 deferred role assignments.

## Step 4 — Apply against a live gateway

```bash
ICN_KEYSTORE_PASSPHRASE='' ./target/debug/icnctl \
  --data-dir /tmp/icn-bootstrap-test2 \
  institution bootstrap apply \
  --package ../institutions/nycn \
  --gateway http://127.0.0.1:8085 \
  --coop-id nycn
```

`--coop-id` is format-validated only (alphanumeric, hyphens, underscores, ≤64 chars).
Any slug works — the value is stored in the JWT claim, not cross-checked against the
entity registry.

## Expected output (live-validated 2026-04-24)

```
Completed:   18
Deferred:    4
Unsupported: 1
Failed:      no
```

### Completed (18)

1. Validate charter (local only)
2. Create Federation entity `nycn` → `entity:icn:federation:nycn`
3. Create Cooperative entity `nycn-organizers` → `entity:icn:cooperative:nycn-organizers`
4. Provision governance domain `nycn-federation-gov`
5. Provision governance domain `nycn-organizers-gov`
6. Create 6 Committee structures (`nycn-backbone`, `nycn-steering`, `nycn-content`, `nycn-logistics`, `nycn-marketing`, `nycn-finance`) → `struct-*` UUIDs
7. Create 1 WorkingGroup structure (`nycn-accessibility-wg`) → `struct-*` UUID
8. Create program `summit-cycle-2026` → `prog-*` UUID
9. Create activity `summit-2026` → `act-*` UUID
10. Create 4 milestones (`summit-strategy-locked`, `summit-venue-ready`, `summit-public-launch-ready`, `summit-cycle-closeout`) → `mile-*` UUIDs

### Deferred (4)

Role assignments where `person_did` is not yet supplied in the seed:
- `coordinator` on `nycn-steering`
- `treasurer` on `nycn-finance`
- `program-coordinator` on `nycn-content`
- `operations-lead` on `nycn-logistics`

To apply these later:
```bash
# Once you have real DIDs, edit 05-role-assignment.seed.yaml and re-run apply.
```

**Idempotency status.** Re-running apply against a live gateway that already has
entities is an open issue. Today the gateway's duplicate-entity handler returns an
`InternalError` wrapping "AlreadyExists"; work to map duplicate registrations to
HTTP 409 and treat 409 as "already exists / skip" in `icnctl apply` is tracked in
the bootstrap roadmap. Until that lands end-to-end, run apply once against a fresh
data dir (or manually skip already-created resources).

### Unsupported (1)

- `store charter` — CCL charter registration/activation through a generic package
  bootstrap sink is not yet implemented.

## Remaining blockers

| Blocker | Impact | Status |
|---------|--------|--------|
| Governance domain storage is in-memory (not persisted to Sled) | Domains lost on gateway restart; `remote_exists` check always 404 on restart → re-creates domains on every apply run | Pre-existing; `create_domain` in standalone mode writes to `Arc<RwLock<HashMap>>` not to Sled |
| `apply` doesn't handle 409 as idempotent | Cannot safely re-run apply against a live gateway with existing entities | Needs a one-line fix in `post_json` to treat 409 as already-exists |
| Binary must be post-2026-04-19 | Structures/activities routes not in older binaries | Resolved by building from current source |
| Charter live registration | Charter not persisted to gateway | Known limitation; charter validates locally only |
| Role assignments need real DIDs | 4 roles deferred | Operational — supply DIDs when organizer identities exist |

## Governance domain persistence note

In standalone gateway mode (no daemon/actor), `create_domain` stores domains in an
in-memory map. Domains disappear on restart. This means:

- The `remote_exists` check (`GET /v1/gov/domains/{id}`) always returns 404 after a
  restart.
- Apply will re-provision domains on every run after restart (idempotent on the domain
  side — governance actor upserts are safe).
- Structures and entities ARE persisted to Sled and survive restarts.
