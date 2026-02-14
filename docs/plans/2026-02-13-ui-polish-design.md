# UI Polish: Conference-Ready Demo — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the Pilot UI demo conference-ready with human-readable names instead of raw DIDs across login, transactions, and governance flows.

**Architecture:** Client-side name cache in app.js resolves DIDs to display names via the existing `GET /v1/members/{coop_id}/{did}` endpoint. A new `PUT /v1/members/{coop_id}/{did}/profile` backend endpoint enables name editing. Demo seed script creates 4 named members with transactions.

**Tech Stack:** Vanilla JS (app.js ~6,300 lines), Rust/actix-web (icn-gateway), bash (demo scripts)

---

## Task 1: Backend — Add PUT profile endpoint

**Files:**
- Modify: `icn/crates/icn-gateway/src/api/members.rs` (add `update_member_profile` handler)
- Modify: `icn/crates/icn-gateway/src/server.rs:995` (register route)
- Modify: `icn/crates/icn-gateway/src/commons_mgr.rs` (add `update_display_name` method)

### Step 1: Add `update_display_name` to CommonsManager

In `icn/crates/icn-gateway/src/commons_mgr.rs`, add after `get_holder_by_did` (line ~364):

```rust
/// Update the display name for a holder identified by DID
pub async fn update_display_name(&self, did: &Did, display_name: String) -> Result<()> {
    let did_str = did.to_string();
    let mut holder = self.store.get_holder_by_did(&did_str)?
        .ok_or_else(|| anyhow::anyhow!("No holder found for DID: {did_str}"))?;
    holder.set_display_name(display_name);
    self.store.put_holder(&holder)?;
    Ok(())
}
```

### Step 2: Add PUT handler in members.rs

In `icn/crates/icn-gateway/src/api/members.rs`, add after the existing `get_member_profile` handler:

```rust
use actix_web::put;

/// Request body for updating member profile
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateProfileRequest {
    /// Display name to set
    pub display_name: String,
}

/// PUT /v1/members/{coop_id}/{did}/profile - Update member profile
///
/// Allows a member to update their own display name.
/// JWT `sub` must match the DID being updated (self-service only).
#[put("/members/{coop_id}/{did}/profile")]
pub async fn update_member_profile(
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<UpdateProfileRequest>,
    coop_manager: web::Data<Arc<CoopManager>>,
    commons_manager: web::Data<Arc<CommonsManager>>,
) -> Result<HttpResponse> {
    let (coop_id, did_str) = path.into_inner();

    // Parse DID
    let did = did_str
        .parse::<Did>()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    // Auth: JWT sub must match the DID being edited
    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::Unauthorized("Authentication required".to_string()))?;
    let caller_did: Did = claims.sub.parse()
        .map_err(|e| GatewayError::Unauthorized(format!("Invalid caller DID: {e}")))?;
    if caller_did != did {
        return Err(GatewayError::Forbidden("Can only update your own profile".to_string()));
    }

    // Verify member exists in coop
    let coop = coop_manager.get_coop(&coop_id).await?;
    if !coop.members.iter().any(|m| m.did == did) {
        return Err(GatewayError::NotFound("Member not found in cooperative".to_string()));
    }

    // Validate display name
    let name = body.display_name.trim().to_string();
    if name.is_empty() || name.len() > 100 {
        return Err(GatewayError::BadRequest(
            "Display name must be 1-100 characters".to_string(),
        ));
    }

    // Update (creates holder record if needed via get_or_create pattern)
    commons_manager.update_display_name(&did, name.clone()).await
        .map_err(|e| GatewayError::Internal(format!("Failed to update display name: {e}")))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "did": did_str,
        "display_name": name
    })))
}
```

### Step 3: Register the route in server.rs

In `icn/crates/icn-gateway/src/server.rs`, find line 995:
```rust
.service(api::members::get_member_profile)
```

Add the PUT route right after:
```rust
.service(api::members::get_member_profile)
.service(api::members::update_member_profile)
```

### Step 4: Verify it compiles

Run: `cd icn && cargo check -p icn-gateway`
Expected: success (exit 0)

### Step 5: Write test

Add to the `#[cfg(test)] mod tests` in `members.rs`:

```rust
#[actix_web::test]
async fn test_update_member_profile() {
    let coop_manager = Arc::new(CoopManager::new());
    let commons_manager = create_test_commons_manager();

    let coop_id = "test-coop";
    let keypair = icn_identity::KeyPair::generate().unwrap();
    let did = keypair.did().clone();
    let did_str = did.to_string();
    let timestamp = icn_time::current_timestamp_secs();

    coop_manager
        .create_coop(coop_id.to_string(), "Test".to_string(), did.clone(), timestamp)
        .await
        .unwrap();

    // Create a holder record so update_display_name can find it
    commons_manager
        .create_holder_from_anchor(&hex::encode([0u8; 32]), &did)
        .await
        .unwrap();

    // Create JWT claims for auth
    let jwt_secret = b"test-secret-32-characters-long!!";
    let token = crate::middleware::create_test_token(&did_str, jwt_secret);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(coop_manager))
            .app_data(web::Data::new(commons_manager))
            .service(update_member_profile)
            .wrap(crate::middleware::JwtAuth::new(jwt_secret.to_vec())),
    )
    .await;

    let req = test::TestRequest::put()
        .uri(&format!("/members/{coop_id}/{did_str}/profile"))
        .set_json(serde_json::json!({"display_name": "Test User"}))
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 200);
}
```

> **Note**: The test helper `create_test_token` may need to be added or adjusted to match actual JWT creation patterns in the gateway test suite. Check `icn-gateway/src/middleware.rs` for existing test helpers. If none exist, use the same JWT encoding the demo scripts use. The test can be simplified to just `test::TestRequest::put()` without auth if the route is registered outside the auth middleware scope for testing.

### Step 6: Run tests

Run: `cd icn && cargo test -p icn-gateway -- members`
Expected: all member tests pass

### Step 7: Commit

```bash
git add icn/crates/icn-gateway/src/api/members.rs icn/crates/icn-gateway/src/server.rs icn/crates/icn-gateway/src/commons_mgr.rs
git commit -m "feat(gateway): add PUT /v1/members/{coop_id}/{did}/profile endpoint"
```

---

## Task 2: Frontend — Name resolution layer

**Files:**
- Modify: `web/pilot-ui/app.js` (add name cache + resolveName helper near top)

### Step 1: Add name cache to state initialization

Find the `state` object initialization in `app.js` (near the top, around line 10-30). Add:

```javascript
nameCache: new Map(), // Map<did, {name: string, fetchedAt: number}>
```

### Step 2: Add resolveName and resolveNames helpers

Add after the `truncateDid` function (line ~334):

```javascript
// Name resolution with caching (5-minute TTL)
const NAME_CACHE_TTL = 5 * 60 * 1000;

async function resolveName(did) {
    if (!did) return 'Unknown';

    // Check cache first
    const cached = state.nameCache.get(did);
    if (cached && (Date.now() - cached.fetchedAt) < NAME_CACHE_TTL) {
        return cached.name;
    }

    // Current user shortcut
    if (did === state.did && state.userName) {
        return state.userName;
    }

    // Fetch from API
    try {
        const profile = await apiRequest('GET', `/members/${encodeURIComponent(state.coopId)}/${encodeURIComponent(did)}`);
        const name = profile.name || truncateDid(did);
        state.nameCache.set(did, { name, fetchedAt: Date.now() });
        return name;
    } catch (e) {
        // Fallback to truncated DID
        const fallback = truncateDid(did);
        state.nameCache.set(did, { name: fallback, fetchedAt: Date.now() });
        return fallback;
    }
}

async function resolveNames(dids) {
    const unique = [...new Set(dids.filter(Boolean))];
    await Promise.all(unique.map(did => resolveName(did)));
    // After this call, all names are cached; callers can use getNameSync
}

// Synchronous name access (returns cached or truncated)
function getNameSync(did) {
    if (!did) return 'Unknown';
    const cached = state.nameCache.get(did);
    if (cached) return cached.name;
    return truncateDid(did);
}
```

### Step 3: Verify no syntax errors

Open `http://<host>:3000` in browser (or check via console). The app should still load and function exactly as before since no call sites have changed yet.

### Step 4: Commit

```bash
git add web/pilot-ui/app.js
git commit -m "feat(ui): add DID-to-name resolution layer with caching"
```

---

## Task 3: Frontend — Wire name resolution into login flow

**Files:**
- Modify: `web/pilot-ui/app.js` (login function + header display)

### Step 1: Resolve current user's name on login

In the `login()` function (line ~530), after `elements.userDid.textContent = truncateDid(state.did);`, add:

```javascript
// Resolve display name for current user
resolveName(state.did).then(name => {
    state.userName = name;
    elements.userDid.textContent = name;
});
```

### Step 2: Add `state.userName` initialization

In the state object, add: `userName: null,`

### Step 3: Commit

```bash
git add web/pilot-ui/app.js
git commit -m "feat(ui): resolve current user name on login"
```

---

## Task 4: Frontend — Replace truncateDid in member dropdown

**Files:**
- Modify: `web/pilot-ui/app.js` (lines 682-691, member dropdown population)

### Step 1: Update member dropdown to show names

Find the `loadMembers()` function around line 682. Replace:

```javascript
option.textContent = truncateDid(member.did);
```

With:

```javascript
option.textContent = getNameSync(member.did) !== truncateDid(member.did)
    ? `${getNameSync(member.did)} (${truncateDid(member.did)})`
    : truncateDid(member.did);
```

### Step 2: Pre-resolve all member names before populating dropdown

Before the dropdown population loop (before `elements.recipient.innerHTML = ...`), add:

```javascript
// Pre-resolve all member names
await resolveNames(members.map(m => m.did));
```

> **Note**: If `loadMembers()` is not already async, make it async.

### Step 3: Commit

```bash
git add web/pilot-ui/app.js
git commit -m "feat(ui): show member names in recipient dropdown"
```

---

## Task 5: Frontend — Replace truncateDid in transaction displays

**Files:**
- Modify: `web/pilot-ui/app.js` (lines 1049, 1264, 3384-3386, 3235)

### Step 1: Update activity feed (line ~1049)

Replace:
```javascript
${isReceived ? 'From' : 'To'} ${truncateDid(other)}
```
With:
```javascript
${isReceived ? 'From' : 'To'} ${getNameSync(other)}
```

### Step 2: Update recent transactions (line ~1264)

Replace:
```javascript
${truncateDid(tx.from)} &rarr; ${truncateDid(tx.to)}
```
With:
```javascript
${getNameSync(tx.from)} &rarr; ${getNameSync(tx.to)}
```

### Step 3: Update transaction list rows (lines ~3384-3386)

Replace:
```javascript
<div class="transaction-from">${truncateDid(tx.from)}</div>
<div class="transaction-arrow">→</div>
<div class="transaction-to">${truncateDid(tx.to)}</div>
```
With:
```javascript
<div class="transaction-from">${getNameSync(tx.from)}</div>
<div class="transaction-arrow">→</div>
<div class="transaction-to">${getNameSync(tx.to)}</div>
```

### Step 4: Update transaction party display (line ~3235)

Replace:
```javascript
<span class="did">${truncateDid(isIncoming ? tx.from : tx.to)}</span>
```
With:
```javascript
<span class="did">${getNameSync(isIncoming ? tx.from : tx.to)}</span>
```

### Step 5: Pre-resolve names before rendering transactions

In the function that loads/renders transactions, before the render loop, add:

```javascript
// Pre-resolve all transaction party names
const allDids = transactions.flatMap(tx => [tx.from, tx.to]);
await resolveNames(allDids);
```

### Step 6: Commit

```bash
git add web/pilot-ui/app.js
git commit -m "feat(ui): show member names in transaction displays"
```

---

## Task 6: Frontend — Replace truncateDid in member lists and contributors

**Files:**
- Modify: `web/pilot-ui/app.js` (lines 1242, 1298, 3280)

### Step 1: Update top contributors (line ~1242)

Replace:
```javascript
<span class="contributor-did">${truncateDid(did)}</span>
```
With:
```javascript
<span class="contributor-did">${getNameSync(did)}</span>
```

### Step 2: Update member list item (line ~1298)

Replace:
```javascript
<div class="member-did" title="${escapeHtml(member.did)}">${truncateDid(member.did)}</div>
```
With:
```javascript
<div class="member-did" title="${escapeHtml(member.did)}">${getNameSync(member.did)}</div>
```

### Step 3: Update member card (line ~3280)

Replace:
```javascript
<div class="member-did">${truncateDid(member.did)}</div>
```
With:
```javascript
<div class="member-did">${getNameSync(member.did)}</div>
```

### Step 4: Commit

```bash
git add web/pilot-ui/app.js
git commit -m "feat(ui): show member names in member lists and contributors"
```

---

## Task 7: Frontend — Replace truncateDid in governance displays

**Files:**
- Modify: `web/pilot-ui/app.js` (lines 777-859, proposal rendering)

### Step 1: Add proposer name to proposal cards

In `renderProposalList` (line ~841), after the `proposal-header` div, add a proposer line:

```javascript
${proposal.proposer ? `<div class="proposal-proposer">Proposed by ${getNameSync(proposal.proposer)}</div>` : ''}
```

### Step 2: Pre-resolve proposer names

At the start of `renderProposalList`, before the map:

```javascript
// Pre-resolve proposer names
await resolveNames(proposals.map(p => p.proposer).filter(Boolean));
```

### Step 3: Commit

```bash
git add web/pilot-ui/app.js
git commit -m "feat(ui): show proposer names in governance proposals"
```

---

## Task 8: Frontend — Replace remaining truncateDid calls

**Files:**
- Modify: `web/pilot-ui/app.js` (lines 531, 2262, 2424, 2611, 4077-4078, 4289, 4304, 4319, 5143, 5146, 3705)

### Step 1: Header user DID (line ~531)

Already handled by Task 3 (login flow resolves name).

### Step 2: Profile page DID (line ~2262)

Replace:
```javascript
document.getElementById('profile-did').textContent = truncateDid(state.did || 'did:icn:...');
```
With:
```javascript
document.getElementById('profile-did').textContent = state.userName || truncateDid(state.did || 'did:icn:...');
```

### Step 3: Service history peer (line ~2424)

Replace `truncateDid(peer)` with `getNameSync(peer)`.

### Step 4: Service listing poster (line ~2611)

Replace `truncateDid(service.poster)` with `getNameSync(service.poster)`.

### Step 5: Batch import preview (lines ~4077-4078)

Replace both `truncateDid(row.from)` and `truncateDid(row.to)` with `getNameSync(row.from)` and `getNameSync(row.to)`.

### Step 6: Toast notifications (lines ~4289, 4304, 4319)

Replace `truncateDid(did)` with `getNameSync(did)` in all three toasts.

### Step 7: Filter labels (lines ~5143, 5146)

Replace `truncateDid(filterState.fromDid)` and `truncateDid(filterState.toDid)` with `getNameSync(filterState.fromDid)` and `getNameSync(filterState.toDid)`.

### Step 8: CSV import preview (line ~3705)

Replace `truncateDid(row.did)` with `getNameSync(row.did)`.

### Step 9: Commit

```bash
git add web/pilot-ui/app.js
git commit -m "feat(ui): replace all remaining truncateDid calls with name resolution"
```

---

## Task 9: Frontend — Profile edit button in header

**Files:**
- Modify: `web/pilot-ui/app.js` (header area near line 531)
- Modify: `web/pilot-ui/index.html` (add edit button element to header)

### Step 1: Add edit button to HTML header

In `index.html`, find the user DID display element (likely `<span id="user-did">` or similar). Add an edit button next to it:

```html
<button id="edit-name-btn" class="btn-icon" title="Edit display name" style="display:none;">✏️</button>
```

### Step 2: Add edit name handler in app.js

After the login name resolution code, add:

```javascript
// Show edit button after login
const editNameBtn = document.getElementById('edit-name-btn');
if (editNameBtn) {
    editNameBtn.style.display = 'inline-block';
    editNameBtn.onclick = () => {
        const currentName = state.userName || '';
        const newName = prompt('Enter your display name:', currentName);
        if (newName !== null && newName.trim() !== '') {
            apiRequest('PUT', `/members/${encodeURIComponent(state.coopId)}/${encodeURIComponent(state.did)}/profile`, {
                display_name: newName.trim()
            }).then(() => {
                state.userName = newName.trim();
                state.nameCache.set(state.did, { name: newName.trim(), fetchedAt: Date.now() });
                elements.userDid.textContent = newName.trim();
                showToast('Display name updated', 'success');
            }).catch(err => {
                showToast('Failed to update name: ' + err.message, 'error');
            });
        }
    };
}
```

### Step 3: Commit

```bash
git add web/pilot-ui/app.js web/pilot-ui/index.html
git commit -m "feat(ui): add inline profile name editing"
```

---

## Task 10: Demo seed — Create named members with transactions

**Files:**
- Modify: `demo/scripts/run-tool-library-demo.sh` (Step 3b seed section, lines ~307-373)

### Step 1: Expand seed section to create 4 named members

Replace the current single-recipient seed block (lines ~311-372) with a loop that creates 4 members, sets their display names, adds them to the coop, and creates transactions between them.

**Pattern for each member:**

```bash
# Create identity
MEMBER_DIR=$(mktemp -d /tmp/icn-demo-member-N.XXXXXX)
MEMBER_DID=$(cd "$ICN_DIR" && ICN_PASSPHRASE=demo123 $TARGET_DIR/release/icnctl \
    -d "$MEMBER_DIR" id init 2>/dev/null \
    | grep -oE 'did:icn:[A-Za-z0-9]+' | head -1 || true)

# Add to cooperative
curl -s -X POST "$GATEWAY/v1/coops/$COOP_ID/members" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"did\":\"$MEMBER_DID\",\"role\":\"participant\"}" >/dev/null 2>&1

# Set display name (uses new PUT endpoint from Task 1)
# Note: The PUT endpoint requires JWT sub == DID. For demo seeding,
# we need a different approach. Option A: generate a token for each member.
# Option B: Add an admin override to the PUT endpoint for seeding.
# Simplest: Use the CommonsManager directly via a seed-specific endpoint
# or use the GET endpoint which already reads from CommonsHolderRecord.
#
# PRAGMATIC APPROACH: Since the demo script has admin tokens, add a
# query parameter ?admin=true or use a separate POST endpoint for seeding.
# OR: Seed names via icnctl (if it supports set-display-name).
#
# SIMPLEST: Add display_name to the AddMemberRequest so names are set
# when members are added to the coop. This is the least-effort path.
```

> **IMPORTANT DECISION**: The PUT profile endpoint requires self-auth (JWT sub == DID). For demo seeding, the simplest approach is to **add an optional `display_name` field to `AddMemberRequest`** so names are set when the admin adds members. This avoids generating per-member JWTs during seeding.

### Step 2: Add `display_name` to AddMemberRequest

In `icn/crates/icn-gateway/src/models.rs:103`:

```rust
pub struct AddMemberRequest {
    pub did: String,
    pub role: String,
    #[serde(default)]
    pub display_name: Option<String>,
}
```

### Step 3: Wire display_name into add_member handler

In `icn/crates/icn-gateway/src/api/coops.rs:175`, after `add_member_atomic` succeeds, set display name if provided:

```rust
// Set display name if provided
if let Some(name) = &req.display_name {
    if let Ok(commons) = commons_manager.as_ref() {
        let _ = commons.update_display_name(&did, name.clone()).await;
    }
}
```

> **Note**: The `add_member` handler needs `commons_manager` injected. Add it as a parameter.

### Step 4: Update seed script with 4 named members

```bash
# Member names for demo
declare -a MEMBER_NAMES=("Sarah Chen" "Marcus Rivera" "Priya Patel" "James Okafor")
declare -a MEMBER_DIDS=()

for i in "${!MEMBER_NAMES[@]}"; do
    NAME="${MEMBER_NAMES[$i]}"
    MEMBER_DIR=$(mktemp -d /tmp/icn-demo-member-$i.XXXXXX)
    MEMBER_DID=$(cd "$ICN_DIR" && ICN_PASSPHRASE=demo123 $TARGET_DIR/release/icnctl \
        -d "$MEMBER_DIR" id init 2>/dev/null \
        | grep -oE 'did:icn:[A-Za-z0-9]+' | head -1 || true)

    if [ -n "$MEMBER_DID" ]; then
        curl -s -X POST "$GATEWAY/v1/coops/$COOP_ID/members" \
            -H "Authorization: Bearer $TOKEN" \
            -H "Content-Type: application/json" \
            -d "{\"did\":\"$MEMBER_DID\",\"role\":\"participant\",\"display_name\":\"$NAME\"}" \
            >/dev/null 2>&1 && echo -e "  ${GREEN}✓${NC} Member: $NAME ($MEMBER_DID)" || true
        MEMBER_DIDS+=("$MEMBER_DID")
    fi
    rm -rf "$MEMBER_DIR"
done

# Set current user display name
curl -s -X PUT "$GATEWAY/v1/members/$COOP_ID/$CURRENT_DID/profile" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"display_name":"Demo User"}' \
    >/dev/null 2>&1 && echo -e "  ${GREEN}✓${NC} Set current user name: Demo User" || true

# Create transactions between members
if [ ${#MEMBER_DIDS[@]} -ge 2 ]; then
    curl -s -X POST "$GATEWAY/v1/ledger/$COOP_ID/payment" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d "{\"from\":\"$CURRENT_DID\",\"to\":\"${MEMBER_DIDS[0]}\",\"amount\":3,\"currency\":\"hours\",\"memo\":\"Garden bed construction — built 4x8 raised bed\"}" \
        >/dev/null 2>&1 && echo -e "  ${GREEN}✓${NC} Transaction: Demo User → Sarah Chen (3 hrs)" || true

    curl -s -X POST "$GATEWAY/v1/ledger/$COOP_ID/payment" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d "{\"from\":\"$CURRENT_DID\",\"to\":\"${MEMBER_DIDS[1]}\",\"amount\":2,\"currency\":\"hours\",\"memo\":\"Tool maintenance — sharpened mower blades and pruning shears\"}" \
        >/dev/null 2>&1 && echo -e "  ${GREEN}✓${NC} Transaction: Demo User → Marcus Rivera (2 hrs)" || true

    curl -s -X POST "$GATEWAY/v1/ledger/$COOP_ID/payment" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d "{\"from\":\"$CURRENT_DID\",\"to\":\"${MEMBER_DIDS[2]}\",\"amount\":1,\"currency\":\"hours\",\"memo\":\"Workshop instruction — intro to power tools safety\"}" \
        >/dev/null 2>&1 && echo -e "  ${GREEN}✓${NC} Transaction: Demo User → Priya Patel (1 hr)" || true
fi
```

### Step 5: Run the full demo and verify names appear

Run: `./demo/scripts/run-tool-library-demo.sh`

Then open UI, log in, verify:
- Header shows "Demo User" instead of truncated DID
- Member dropdown shows "Sarah Chen (did:icn:abc...)" etc.
- Transaction history shows "Demo User → Sarah Chen" etc.

### Step 6: Commit

```bash
git add demo/scripts/run-tool-library-demo.sh icn/crates/icn-gateway/src/models.rs icn/crates/icn-gateway/src/api/coops.rs
git commit -m "feat(demo): seed 4 named members with transactions"
```

---

## Task 11: Verification — Full demo e2e test

### Step 1: Reset and run demo

```bash
# Kill any existing demo processes
pkill icnd 2>/dev/null || true
pkill -f "http.server" 2>/dev/null || true
rm -rf .demo-data/

# Run fresh demo
./demo/scripts/run-tool-library-demo.sh
```

### Step 2: Verify three flows

**Flow 1: Login + Dashboard**
- Open UI at `http://<host>:3000`
- Log in with displayed credentials
- Verify: Header shows "Demo User" (not truncated DID)
- Verify: Member list shows named members
- Verify: Contributor list shows names

**Flow 2: Log Hours + Transaction**
- Select "Sarah Chen (did:icn:...)" from dropdown
- Log 2 hours with memo
- Verify: Transaction row shows "Demo User → Sarah Chen"
- Verify: Activity feed shows names

**Flow 3: Governance**
- View proposals
- Verify: "Proposed by Demo User" appears on proposals
- Cast a vote
- Verify: Vote confirmation works

### Step 3: Run automated tests

```bash
cd icn && cargo test -p icn-gateway
./demo/scripts/quick-test.sh
```

### Step 4: Run pre-push gates

Use `/push` skill to verify fmt + clippy + push.

---

## Out of Scope

- SDIS enrollment UI
- Exchange/listings features
- Push notifications
- Offline/edge cases
- Invite flow
- Mobile responsive redesign
- Marketplace assignee dropdown (line 6212 already uses `display_name`)
