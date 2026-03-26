<!-- Relocated from docs/plans/.scratch/ (tranche 3). -->

# Human Interface — Repo State & Gap Analysis

## 1. Current State

### 1.1 pilot-ui Inventory

Location: `web/pilot-ui/` (source) → synced to `icn-gateway/static/` (served at gateway root)

| Screen / Tab | File(s) | API Calls | Scope-Aware? | Shows Receipts? |
|-------------|---------|-----------|-------------|-----------------|
| Login | `index.html:32-100`, `app.js:520-630` | `GET /v1/health`, `GET /v1/ledger/{coop}/balance/{did}` (validation) | **No** — coop ID manually entered | No |
| QR Login | `app.js:5237-5400` | `POST /v1/sessions` (create), `GET /v1/sessions/{id}` (poll status) | No | No |
| Join via Invite | `index.html` (join-screen), `app.js:5100-5170` | `POST /v1/invites/join` | No | No |
| Dashboard | `index.html:371-476`, `app.js:632-700` | `GET /v1/ledger/{coop}/balance/{did}`, `GET /v1/coops/{coop}`, members, transactions, proposals | **No** — shows coop name only | No |
| Log Hours | `index.html:708-738` | `POST /v1/ledger/{coop}/payment` | No | No |
| Transaction History | `index.html:739-905` | `GET /v1/ledger/{coop}/history` | No | No |
| Members | `index.html:906-1308` | `GET /v1/coops/{coop}` (includes members), member CRUD | No | No |
| Member Profile | `index.html:477-595` | `GET /v1/members/{coop}/{did}` | No | No |
| Service Board | `index.html:596-707` (hidden tab) | Listings API | No | No |
| Governance | `index.html:1309-1453` | `GET/POST /v1/gov/proposals`, votes, discussion, proof | No | **Partial** — governance proof button exists |
| Proposal Detail | `app.js:987-1500` (modal) | `GET /v1/gov/proposals/{id}`, votes, discussion, comments, reactions | No | Proof link only |
| Exchange (Marketplace) | `index.html:1454-1660` (hidden tab) | `GET/POST /v1/listings`, interest, matching | No | No |
| Receipt Chain Viewer | `index.html:1661+`, `receipts.js` | `GET /v1/receipts/chain/{hash}` | No | **Yes** — full chain view |
| SDIS Enrollment | `sdis-enrollment.html/js/css` | SDIS endpoints | No | No |
| SDIS Identity | `sdis-identity.html/js/css` | Identity endpoints | No | No |
| SDIS Recovery | `sdis-recovery.html/js/css` | Recovery endpoints | No | No |
| SDIS Proofs | `sdis-proofs.html/js/css` | Proof endpoints | No | No |
| Steward Dashboard | `steward-dashboard.html/js/css` | Steward endpoints | No | No |
| Anchor Manager | `components/anchor-manager.js` | Anchor endpoints | No | No |
| Offline Fallback | `offline.html` | None | No | No |
| Test API | `test-api.html` | Various (debugging) | No | No |

**Total: 12 tabs/views in main SPA + 5 standalone SDIS pages + 2 utility pages**

### 1.2 Auth Flows

#### Current Login (Manual Token Paste)
1. User runs `icnctl auth token -c COOP_ID -s "ledger:read,gov:write"` in terminal
2. icnctl performs challenge-response with gateway (DID + Ed25519 signature)
3. icnctl outputs JWT string
4. User manually pastes JWT into pilot-ui login form
5. pilot-ui validates by calling `GET /v1/health` + `GET /v1/ledger/{coop}/balance/{did}`
6. Token stored in `state.token` (in-memory) with expiry tracking
7. Token expires after 24h; user must repeat

**Friction**: Requires CLI access and manual copy-paste. Non-technical users cannot log in.

#### QR Login (Implemented but incomplete)
1. pilot-ui calls `POST /v1/sessions` → creates session with unique ID
2. QR code displayed containing session ID + gateway URL
3. Mobile wallet (not yet built) scans QR
4. Wallet calls `POST /v1/sessions/{id}/approve` with JWT
5. pilot-ui polls `GET /v1/sessions/{id}` until status = "approved"
6. pilot-ui receives JWT from approved session

**Status**: Backend endpoints exist (`api/sessions.rs`). QR code generation works. BUT no mobile wallet exists to scan/approve. The flow is **dead-ended** — you can display a QR but nothing can scan it.

#### Magic Link
**Does not exist** anywhere in the codebase. No email integration, no link-based auth.

#### JWT Lifecycle
- Created via `POST /v1/auth/verify` after challenge-response
- Claims: `sub` (DID), `coop_id`, `exp`, `iat`, `scopes`
- Default expiry: 24 hours
- No refresh token mechanism in gateway
- pilot-ui tracks expiry and warns at 15/10/5 minute marks
- Auto-logout on 401 response

### 1.3 CoopWallet Inventory

Location: `sdk/react-native/examples/CoopWallet/` (Expo/React Native)
React Native SDK: `sdk/react-native/src/`
Total: ~10K LOC (app + SDK), 21 screens

| Screen | Connected to Backend? | Offline? | Notes |
|--------|----------------------|----------|-------|
| LoginScreen | **Yes** — challenge-response auth | No | DID + coopId entry, QR scan for enrollment |
| HomeScreen | **Yes** — balance, recent tx | Queue | Dashboard with quick actions (Send/Receive/Scan/Vote/Trust) |
| PaymentScreen | **Yes** — `POST /v1/ledger/{coop}/payment` | Queue | Send hours via DID or QR, contacts + recent recipients |
| ReceiveScreen | **Local** — QR generation | N/A | Generates payment QR for receiving |
| TransactionsScreen | **Yes** — `GET /v1/ledger/{coop}/transactions` | Cache | Full history with filtering, pull-to-refresh |
| TransactionDetailScreen | **Yes** — tx metadata | Cache | Detail + memo display |
| GovernanceScreen | **Yes** — `GET /v1/governance/{coop}/proposals` | Cache | Active proposals with voting state |
| ProposalScreen | **Yes** — proposal details + vote | Queue | Voting interface, tally display |
| IdentityScreen | **Local** — ephemeral proof generation | Yes | Age 18+/21+/65+, citizenship, non-revocation; 6h max validity |
| VerifyScreen | **Yes** — `POST /v1/sdis/verify/level1` | No | Scan QR proofs and verify |
| VerificationHistoryScreen | **Local** — AsyncStorage | Yes | 30-day retention, max 100 entries |
| StewardDashboardScreen | **Yes** — pending enrollments | No | Steward-only: manage enrollments |
| VouchConfirmationScreen | **Yes** — vouch API | No | Steward confirmation of vouches |
| VouchHistoryScreen | **Local** — AsyncStorage | Yes | Steward audit trail |
| EnrollmentDetailScreen | **Yes** — enrollment API | No | Steward review of enrollment details |
| TrustAttestationScreen | **Yes** — `POST /v1/trust/attestations` | Queue | Create trust edges for other members |
| ContactsScreen | **Local** — AsyncStorage/SecureStore | Yes | Saved contacts with DID + name |
| MemberProfileScreen | **Yes** — `GET /v1/members/{coop}/{did}` | Cache | Role, tx count, trust score/class |
| SettingsScreen | **Local** | Yes | Account info, theme toggle, logout |

**Navigation**: Bottom tabs (Home | Transactions | Governance | Identity | Settings) + modal stacks for detail views.

**Crypto**: Ed25519 via `@noble/ed25519` for all signing. Post-quantum hybrid (Ed25519 + ML-DSA-65 Dilithium) available via `hybrid-crypto.ts`. Keys stored in SecureStore (iOS Keychain / Android Keystore).

**Offline Queue**: `QueueManager` in `sdk/react-native/src/queue-manager.ts`. Queues payments, votes, proposals, trust attestations. Persistent (AsyncStorage). 3 retries with exponential backoff (1s, 2s, 4s). `useQueue()` hook exposes pending/failed counts. `useNetworkState()` triggers processing on reconnect.

**i18n**: EN + ES via `i18next` + `react-i18next`.

**Gateway target**: `http://10.8.10.40:30080` (homelab staging), configurable in `src/config.ts`.

### 1.4 icnctl Command Coverage

**32 command groups, 100+ subcommands, 10,191 lines in single main.rs**

| Command Group | User-Facing? | Description | Connection |
|--------------|-------------|-------------|------------|
| `id` (init/show/rotate/export/import/upgrade-pq) | **Yes** | Identity management | Local keystore |
| `device` (list/add/approve/revoke) | **Yes** | Multi-device management | Local + gateway |
| `recovery` (setup/config/initiate/attest/finalize) | **Yes** | Social recovery (M-of-N) | Local + gateway |
| `trust` (add/list/show/remove) | **Yes** | Personal trust edges | RPC to daemon |
| `gov domain` (create/show/list/add-member) | Officer | Governance domains | RPC |
| `gov proposal` (create/open/list/show/close) | **Yes** | Democratic participation | RPC |
| `gov vote` (cast/show/delegate/revoke) | **Yes** | Voting | RPC |
| `ledger` (head/balance/history) | **Yes** | Account transparency | RPC |
| `ledger quarantine` (list/get/release/drop/purge) | Admin | Quarantined entries | RPC |
| `dispute` (file/list/get/add-evidence/resolve) | **Yes** | Dispute filing | RPC |
| `compute` (submit/submit-wasm/status/cancel) | Power | Task execution | RPC |
| `contract` (deploy/prepare/sign/call/list) | Power | CCL contracts | RPC |
| `network` (peers/dial/stats/status) | Operator | P2P debugging | RPC |
| `federation` (coop/vouch/attestation/clearing) | Officer | Cross-coop coordination | RPC + gateway |
| `steward` (register/retire/check-vui/enroll) | Steward | Identity stewardship | Gateway |
| `commons` (status/enroll/anchor/affiliations/join) | **Yes** | Jurisdiction membership | Gateway |
| `charter` (create/show/list/sign/ratify) | **Yes** | Organizational founding | Gateway |
| `amendment` (propose/list/show/vote/withdraw) | **Yes** | Constitutional governance | Gateway |
| `appeal` (file/list/show/add-evidence/respond) | **Yes** | Due process | Gateway |
| `auth` (token) | **Yes** | Get JWT token | Gateway |
| `receipts` (chain/allocation/intent) | **Yes** | Economic proof queries | Gateway |
| `policy` (set/show/list/remove) | Admin | Scheduling policies | RPC |
| `quota` (show/list) | Admin | Resource quotas | RPC |
| `status` | **Yes** | Daemon health | RPC |
| `preflight` | Operator | Pre-deployment checks | Local + gateway |
| `backup`/`restore`/`verify-backup` | Operator | Data management | Local |
| `snapshot` (create/list/verify/delete/cleanup) | Operator | State snapshots | Local |
| `init-coop` | Operator | Guided setup wizard | Local + RPC |
| `api` (export-openapi) | Developer | Schema export | Local |
| `completions` | **Yes** | Shell completion gen | Local |

**i18n**: 2 locales — `en.yaml` (8KB), `es.yaml` (9KB). Uses `rust_i18n` crate.

**Output**: Human-readable by default. `--json` flag on receipt commands only. No universal JSON output mode.

### 1.5 TypeScript SDK Coverage

Location: `sdk/typescript/` | Package: `@icn/client` v0.1.0 | Zero runtime deps

| Gateway Domain | SDK Methods | Coverage |
|---------------|------------|---------|
| Auth (challenge/verify) | `getChallenge`, `verify`, `authenticate` | Full |
| Token management | `setToken`, `clearToken`, `isTokenExpired` | Full |
| Cooperatives | `listCoops`, `createCoop`, `getCoop`, `updateCoop`, `deleteCoop`, `getCoopStats` | Full |
| Members | `listMembers`, `addMember`, `updateMember`, `removeMember`, `getMemberProfile`, batch ops | Full |
| Ledger/Payments | `getBalance`, `pay`, `crossPay`, `getHistory`, `batchPay`, query builder | Full |
| Governance | `createDomain`, `createProposal`, `vote`, `openProposal`, `closeProposal` | Full |
| Delegation | `createDelegation`, `listDelegations`, `revokeDelegation` | Full |
| Compute | `submitTask`, `submitWasmTask`, `getTaskStatus`, `waitForTask`, `cancelTask` | Full |
| WASM Registry | `uploadWasm`, `listWasm`, `getWasm`, `submitWasmByHash` | Full |
| Treasury | `getTreasuryStatus`, `getTreasuryBalance`, `proposeTreasurySpend` | Full |
| Services | `announceService`, `withdrawService`, `discoverServices` | Full |
| Charter | `createCharter`, `getCharter`, `signCharter`, `activateCharter` | Full |
| Membership (Commons) | `applyForMembership`, `approveMembership`, capabilities, roles | Full |
| Amendments | `createAmendment`, `submitAmendment`, `castAmendmentVote`, `ratifyAmendment` | Full |
| Appeals | `fileAppeal`, `addAppealEvidence`, `resolveAppeal` | Full |
| Identity | `resolveDid`, `identityHealth` | Full |
| Devices | `registerDevice`, `listDevices`, `revokeDevice` | Full |
| Exchange Rates | `getExchangeRate`, `convertAmount`, `setManualRate` | Full |
| WebSocket | `connectWebSocket` with auto-reconnect, backfill, event filtering | Full |
| Health | `health()` | Full |

**Total: 90+ methods across 19 domains. Comprehensive coverage.**

**Auth Model**: `SignatureProvider` interface — SDK does NOT handle key generation, storage, or Ed25519 crypto. Users must implement `sign(challenge: string): Promise<string>`. This is intentional for security (HSM, WebAuthn support) but creates an onboarding barrier.

**Key Gap**: No key management utilities. A non-developer cannot use the SDK without importing a separate crypto library and writing signing code.

### 1.6 Gateway Route Module Inventory

Location: `icn/crates/icn-gateway/src/api/` — 43 route module files

**Total: ~200+ endpoints across 34+ domain groups**

| Module | Domain | Endpoints | Status | Auth |
|--------|--------|-----------|--------|------|
| `health.rs` | Diagnostics | 5 | **Live** | Public |
| `auth.rs` | Challenge/verify | 2 | **Live** | Public |
| `sessions.rs` | QR Login | 3 | **Live** | Mixed |
| `coops.rs` | Cooperative CRUD | 8 | **Live** | JWT |
| `members.rs` | Membership | 2+ | **Live** | JWT |
| `ledger.rs` | Payments/balance | 6 | **Live** | JWT |
| `governance.rs` | Domains/proposals | 25+ | **Live** | JWT |
| `trust.rs` | Trust graph | 5 | **Live** | JWT |
| `compute.rs` | Task execution | 6 | **Live** (if compute handle) | JWT |
| `federation.rs` | Cross-coop | 16 | **Live** | JWT |
| `treasury.rs` | Collective reserves | 5+ | **Partial** (needs handle) | JWT |
| `listings.rs` | Marketplace | 7 | **Live** | JWT |
| `entity.rs` | Entity CRUD+audit | 10 | **Live** | JWT |
| `escrow.rs` | Conditional tx | 5 | **Live** | JWT |
| `recurring_payments.rs` | Subscriptions | 4+ | **Live** (scheduler runs) | JWT |
| `budgets.rs` | Spending limits | 5 | **Live** | JWT |
| `services.rs` | Service discovery | 5+ | **Live** | JWT |
| `contracts.rs` | CCL contracts | 7 | **Partial** (needs registry) | JWT |
| `invites.rs` | Invite flow | 3 | **Live** | Mixed |
| `governance_dashboard.rs` | Summary stats | 1 | **Live** | JWT |
| `registry.rs` | Decision registry | 6 | **Live** | JWT |
| `receipts.rs` | Economic receipts | 2+ | **Live** | JWT |
| `oracle.rs` | Exchange rates | 4+ | **Live** | JWT |
| `steward/mod.rs` | SDIS stewards | 10 | **Live** | JWT |
| `membership/mod.rs` | Commons membership | 5+ | **Live** | JWT |
| `charter/mod.rs` | Org founding docs | 7 | **Live** | JWT |
| `sdis/mod.rs` | Identity verification | 5 | **Stub** (basic logic) | Public |
| `constitutional/mod.rs` | Amendments/appeals | 5+ | **Stub** (minimal logic) | JWT |
| `commons/mod.rs` | Resource mgmt | 3+ | **Stub** | JWT |
| `communities.rs` | Civic engine | 10 | **Stub** (basic CRUD) | JWT |
| `notifications.rs` | Push notifications | 5 | **Partial** (FCM optional) | JWT |
| `agreements.rs` | Inter-coop agreements | 14 | **Partial** (needs handle) | JWT |
| `identity.rs` | DID resolution | 3 | **Live** | Mixed |
| `devices.rs` | Multi-device | 4 | **Live** | JWT |

### 1.7 WebSocket Events

Endpoint: `/v1/websocket` or `/v1/ws/{coop_id}`

| Event Type | Category | Trigger |
|-----------|----------|---------|
| `PaymentCreated` | Ledger | New payment |
| `CrossPaymentCreated` | Ledger | Cross-currency payment |
| `TransactionCompleted` | Ledger | Transaction confirmed |
| `BalanceChanged` | Ledger | Balance update |
| `BatchBalanceChanged` | Ledger | Batch balance update |
| `LedgerMemberFrozen` | Ledger | Account frozen |
| `LedgerMemberUnfrozen` | Ledger | Account unfrozen |
| `LedgerForkDetected` | Ledger | Conflict detected |
| `LedgerForkResolved` | Ledger | Conflict resolved |
| `LedgerRollback` | Ledger | Rollback occurred |
| `ResourceAccessTransferred` | Ledger | Resource transferred |
| `MemberAdded` | Membership | New member |
| `MemberRemoved` | Membership | Member left |
| `RoleUpdated` | Membership | Role changed |
| `GovernanceDomainCreated` | Governance | New domain |
| `GovernanceProposalCreated` | Governance | Proposal submitted |
| `GovernanceProposalOpened` | Governance | Voting started |
| `GovernanceProposalClosed` | Governance | Voting ended |
| `GovernanceVoteCast` | Governance | Vote cast |
| `ComputeTaskSubmitted` | Compute | Task submitted |
| `ComputeTaskClaimed` | Compute | Task claimed |
| `ComputeTaskCompleted` | Compute | Task finished |
| `ComputeTaskCancelled` | Compute | Task cancelled |
| `TrustAttested` | Trust | Trust created |
| `TrustRevoked` | Trust | Trust revoked |
| `SettingsUpdated` | Config | Settings changed |
| `Shutdown` | Control | Server shutting down |

**Protocol features**: Sequence numbering, backfill on reconnect, 5000-event buffer, heartbeat (30s/60s timeout), 10K max global connections.

## 2. User Journey Analysis

### Journey A: New person joins a cooperative

**Current path (what they'd have to do today)**:
1. Install Rust toolchain (to build icnctl from source — no prebuilt binary)
2. Run `icnctl id init` to create Ed25519 keypair with passphrase
3. Run `icnctl id show` to get their DID
4. Give DID to a cooperative admin (out-of-band — email, chat)
5. Admin runs `icnctl federation coop register` or uses gateway API to add member
6. OR: admin creates invite via `POST /v1/invites`, shares invite code
7. New member opens pilot-ui, uses "Join" form with invite code
8. After join: run `icnctl auth token -c COOP_ID` to get JWT
9. Paste JWT into pilot-ui login form

**What's broken/missing**:
- Requires CLI / Rust toolchain — impossible for non-developers
- No web-based identity creation
- No in-browser key generation
- No invite flow that works end-to-end for non-technical users
- Join form exists in pilot-ui but requires DID (which requires CLI)
- No "create account" button that generates identity in browser

### Journey B: Member proposes and votes

**Current path**:
1. Log in to pilot-ui (JWT paste)
2. Navigate to Governance tab
3. Proposals visible with voting buttons (for/against/abstain)
4. Can create proposals via governance form
5. Can view vote tallies, discussion, proof
6. Comments and reactions supported
7. Proposal detail view with timeline

**What's broken/missing**:
- No scope context (which domain/jurisdiction is this proposal in?)
- No delegation UI (delegation is CLI-only via `icnctl gov vote delegate`)
- No "what law applies" context — proposals float without constitutional grounding
- Proposal outcomes don't show effect chain (decision → allocation → settlement)
- No receipt trail visible from proposal to ledger entry

### Journey C: Steward verifies personhood

**Current path**:
1. Run `icnctl steward register` with bond, term, specializations
2. Use `steward-dashboard.html` (standalone page, separate from main SPA)
3. SDIS enrollment via `sdis-enrollment.html` (standalone)
4. VUI check via `icnctl steward check-vui`
5. Enrollment ceremony via `icnctl steward start-enrollment`

**What's broken/missing**:
- Steward dashboard is a separate HTML page, not integrated into main SPA
- SDIS pages are standalone, not linked from main navigation
- No unified "steward workflow" — scattered across CLI + 4 separate pages
- Spatial proof logic is basic/stub in gateway
- No mobile-friendly enrollment (needs camera for spatial proofs)

### Journey D: Operator runs a node for their org

**Current path**:
1. Build from source (`cargo build --release`)
2. Run `icnctl id init` to create node identity
3. Run `icnd --init` to initialize data directory
4. Run `icnd` to start daemon
5. Access pilot-ui at `http://localhost:8080`
6. Use `icnctl status`, `icnctl network peers`, `icnctl network stats`
7. Deploy scripts in `deploy/k8s/` for K3s cluster

**What's broken/missing**:
- No prebuilt binaries (must compile from source)
- No Docker images published
- No guided setup wizard in the web UI
- Monitoring via Prometheus metrics but no bundled dashboard
- `icnctl preflight` exists but not integrated into web UI
- Federation setup is complex (multiple CLI commands)

## 3. Scope Navigation Gaps

- **Does any screen show "where am I"?** **No.** The pilot-ui header shows coop name and user DID, but no jurisdiction, scope, or constitutional context. The user has no way to know which governance domain they're operating in, what laws apply, or what scope their actions affect.

- **Can a user switch scope/jurisdiction?** **No.** Login is locked to a single coop ID. To switch cooperatives, user must log out and log back in with a different coop ID. There's no multi-org view, no scope switcher, no federation-aware navigation.

- **Is the scope context propagated in API calls?** **Partially.** The JWT includes `coop_id` and `scopes` claims. The gateway has cross-coop access checks (`require_coop_access()`). But the UI never shows which scope is active or what the user's capabilities are within that scope.

- **Missing scope primitives**:
  - No "scope banner" showing current jurisdiction
  - No "what law applies here" indicator
  - No "my standing in this scope" display
  - No constitutional context (which charter governs this space?)
  - No capability display (what can I do here?)

## 4. Receipts Visibility Gaps

- **Does any screen show receipt chains?** **Yes — one screen.** The Receipt Chain Viewer (`receipts.js`, hidden tab) can query `GET /v1/receipts/chain/{hash}` and display: DecisionReceipt → AllocationReceipt → SettlementIntent → LedgerEntry. But it's a hidden tab, requires manual hash entry, and is not linked from any other view.

- **Can a user trace decision → effect → settlement?** **Barely.** The receipt viewer can show the chain if you know the decision hash. But:
  - Governance proposals don't link to their receipt chain
  - Transaction history doesn't show provenance
  - No "why was I charged/credited?" view
  - No automatic receipt display after actions

- **Is there a "what happened and why?" view?** **No.** No audit trail visible in the UI. The gateway has entity audit endpoints (`GET /v1/entities/{id}/audit`) and decision registry (`GET /v1/registry/decisions/{id}/trace`), but no UI consumes them.

- **Missing receipt primitives**:
  - Governance proposals should link to receipt chain on close
  - Transactions should show provenance (which decision authorized this?)
  - Treasury spends should show full receipt chain
  - Appeal outcomes should show effect chain
  - Amendment ratification should show constraint propagation

## 5. Gap Analysis

### Critical Gaps (User story blockers)

| # | Gap | User Story Blocked | Missing Primitive OR Surface | Where It Belongs | Phase |
|---|-----|-------------------|------------------------------|-----------------|-------|
| G1 | No browser-based identity creation | "I want to join a cooperative" | Identity generation in browser (Ed25519 + DID) | pilot-ui + SDK | **0** |
| G2 | Auth requires CLI token paste | "I want to log in" | QR login needs mobile wallet OR magic link OR in-browser signing | pilot-ui auth flow | **0** |
| G3 | No scope context banner | "Where am I? What law applies?" | Scope context component with jurisdiction, charter, capabilities | pilot-ui header | **0** |
| G4 | Receipts not linked from actions | "What happened and why?" | Receipt chain links from proposals, transactions, treasury | pilot-ui governance + ledger tabs | **0** |
| G5 | No multi-org navigation | "I'm in 3 cooperatives" | Org switcher, affiliations home screen | pilot-ui nav | **1** |
| G6 | No standing/capability display | "What can I do here?" | Standing panel showing trust class, roles, capabilities | pilot-ui profile | **1** |
| G7 | No delegation UI | "I want to delegate my vote" | Delegation management (create, list, revoke) | pilot-ui governance tab | **1** |
| G8 | No treasury visualization | "Where does our money go?" | Treasury spend flow with receipt chain | pilot-ui new tab | **1** |
| G9 | No appeal filing in UI | "I want to appeal a decision" | Appeal form + status tracking | pilot-ui governance tab | **1** |
| G10 | No mobile app | "I want to do this from my phone" | React Native or PWA with signing capability | New project | **4** |
| G11 | SDIS pages not integrated | "I want to verify my identity" | Integrate SDIS flows into main SPA | pilot-ui | **2** |
| G12 | No offline queue for writes | "My network is unreliable" | Offline write queue with sync | pilot-ui service worker | **2** |
| G13 | No notification UI | "Tell me when something happens" | Notification bell + push notifications | pilot-ui | **2** |
| G14 | OpenAPI paths not exported | SDK type generation incomplete | Fix utoipa path generation | icn-gateway | **1** |
| G15 | SDK has no key management | "How do I sign things?" | Ed25519 key generation helper in SDK | sdk/typescript | **1** |

### Gaps by Surface

**pilot-ui**:
- G1 (identity creation), G2 (auth), G3 (scope), G4 (receipts), G5 (multi-org), G6 (standing), G7 (delegation), G8 (treasury viz), G9 (appeals), G11 (SDIS integration), G12 (offline writes), G13 (notifications)

**SDK**:
- G15 (key management helper)

**Gateway**:
- G14 (OpenAPI paths)

**Mobile (CoopWallet)**:
- G10 (doesn't exist yet — entirely new project)

## 6. Phase 0 Tasks (8 tasks)

These are the minimum-viable UI changes needed for the "Point A" baseline to be usable by non-developers.

### P0-UI-1: In-Browser Identity Creation
**What**: Add "Create Identity" button on login screen that generates Ed25519 keypair in browser, derives DID, and allows download of encrypted keystore.
**Files to create/modify**:
- `web/pilot-ui/index.html` — Add "Create New Identity" section to login screen
- `web/pilot-ui/crypto.js` — New file: Ed25519 key generation using WebCrypto API or noble-ed25519
- `web/pilot-ui/app.js` — Wire creation flow, store keypair in IndexedDB (encrypted)
**API dependencies**: None (local operation). Later: `POST /v1/auth/challenge` + `POST /v1/auth/verify` for auto-login after creation.
**Validation**: User can create identity, see DID, and use it to log in — all in browser, no CLI needed.

### P0-UI-2: Challenge-Response Auth in Browser
**What**: Replace "paste JWT token" with in-browser challenge-response. User enters DID, browser fetches challenge, signs with stored key, gets JWT.
**Files to create/modify**:
- `web/pilot-ui/app.js` — Replace manual token paste with automatic challenge-response flow
- `web/pilot-ui/crypto.js` — `sign(challenge, privateKey)` function
- `web/pilot-ui/index.html` — Simplify login form (remove token field, add passphrase field for keystore unlock)
**API dependencies**: `POST /v1/auth/challenge`, `POST /v1/auth/verify` (both exist)
**Validation**: User enters DID + passphrase → automatic JWT acquisition, no CLI needed.

### P0-UI-3: Scope Context Banner
**What**: Add a persistent header banner showing: current cooperative name, jurisdiction (if any), active charter name, user's role, trust class.
**Files to create/modify**:
- `web/pilot-ui/index.html` — Add scope banner below header
- `web/pilot-ui/app.js` — Fetch scope context on login (`GET /v1/coops/{coop}`, `GET /v1/charter?coop_id={coop}`)
- `web/pilot-ui/style.css` — Banner styling
**API dependencies**: `GET /v1/coops/{coop}` (exists), `GET /v1/charter` (exists), `GET /v1/trust/score/{did}` (exists)
**Validation**: After login, banner shows "Timebank Coop | Cooperative Charter v1 | Member | Trust: Known (0.35)"

### P0-UI-4: Receipt Chain Links from Governance
**What**: When a proposal closes, display "View Receipt Chain" button that opens the receipt viewer pre-populated with the decision hash.
**Files to create/modify**:
- `web/pilot-ui/app.js` — Add receipt link to `renderProposalDetailHeader()` and `renderProposalList()`
- `web/pilot-ui/receipts.js` — Add `openForDecision(hash)` public function
- `web/pilot-ui/index.html` — Unhide receipts tab
**API dependencies**: `GET /v1/receipts/chain/{hash}` (exists), `GET /v1/gov/proposals/{id}/proof` (exists)
**Validation**: Close a proposal → "View Receipt Chain" button → full chain displayed.

### P0-UI-5: "Constraints with Receipts" Dashboard (#1012)
**What**: Show active constraints for current user (rate limits, credit multipliers, voting weights) with links to the governance decisions that created them.
**Files to create/modify**:
- `web/pilot-ui/index.html` — Add "My Constraints" card to dashboard tab
- `web/pilot-ui/app.js` — Fetch and render constraints
**API dependencies**: Need new endpoint `GET /v1/constraints/{did}` OR derive from `GET /v1/trust/score/{did}` + governance config. May need gateway endpoint addition.
**Validation**: Dashboard shows "Rate Limit: 20 msg/sec (from Trust Class: Known)" with link to trust score source.

### P0-UI-6: "What Law Applies?" Stub
**What**: Add "Governing Law" section to scope banner that links to the active charter/constitution document.
**Files to create/modify**:
- `web/pilot-ui/index.html` — Extend scope banner with charter link
- `web/pilot-ui/app.js` — Fetch charter details, render link
**API dependencies**: `GET /v1/charter/{id}` (exists), `GET /v1/charter?coop_id={coop}` (exists)
**Validation**: Banner shows clickable "Governed by: Cooperative Charter v1 (Active since 2025-12-03)" that opens charter detail.

### P0-UI-7: LAN Accessibility Fixes
**What**: pilot-ui currently hardcodes `http://localhost:8080`. Make gateway URL auto-detected or configurable for LAN access.
**Files to create/modify**:
- `web/pilot-ui/app.js` — Auto-detect gateway URL from `window.location.origin` (since UI is served from gateway)
- `web/pilot-ui/index.html` — Pre-fill gateway URL with current host
**API dependencies**: None
**Validation**: Access pilot-ui at `http://192.168.1.X:8080` and login works without manual URL change.

### P0-UI-8: Non-Developer Onboarding Flow
**What**: Guided wizard combining P0-UI-1 + P0-UI-2: "Create Identity → Join Cooperative (via invite code) → Auto-login → Dashboard"
**Files to create/modify**:
- `web/pilot-ui/index.html` — Add onboarding wizard overlay
- `web/pilot-ui/app.js` — Wizard state machine (3 steps)
- `web/pilot-ui/style.css` — Wizard styling
**API dependencies**: Identity creation (P0-UI-1), auth (P0-UI-2), `POST /v1/invites/join` (exists)
**Validation**: Non-developer opens URL → clicks "Get Started" → creates identity → enters invite code → sees dashboard. No CLI needed.

## 7. Phase 1 Tasks (5 tasks)

### P1-UI-1: Standing & Capability Display
**What**: Profile tab shows user's trust score, trust class, active roles, capabilities, and rate limits.
**Files**: `web/pilot-ui/app.js` (profile section), `web/pilot-ui/index.html` (profile tab)
**API deps**: `GET /v1/trust/score/{did}`, `GET /v1/members/{coop}/{did}`, `GET /v1/membership/status/{did}` (all exist)

### P1-UI-2: Multi-Org Affiliations Home Screen
**What**: After login, show all cooperatives the user belongs to. Allow switching between them without full logout.
**Files**: New affiliations component in `app.js`, modified login flow to query multiple coops
**API deps**: Need `GET /v1/coops?member={did}` or similar. May need new endpoint.

### P1-UI-3: Treasury Spend Visualization with Receipt Chain
**What**: New "Treasury" tab showing collective balance, pending proposals, and completed spend flows with full receipt chains.
**Files**: New tab in `index.html` + `app.js`
**API deps**: `GET /v1/treasury/{coop}/status`, `GET /v1/treasury/{coop}/balance`, `GET /v1/receipts/chain/{hash}` (all exist)

### P1-UI-4: Delegation Management UI
**What**: In Governance tab, add delegation management: create delegation (blanket/domain/proposal scoped), view active delegations, revoke.
**Files**: Extend governance section in `index.html` + `app.js`
**API deps**: `POST /v1/gov/delegations`, `GET /v1/gov/delegations`, `DELETE /v1/gov/delegations/{id}` (all exist)

### P1-UI-5: Appeal Filing Entry Point
**What**: "File Appeal" button on closed proposals and governance decisions. Appeal form with grounds, evidence upload, respondent selection.
**Files**: New modal in `index.html` + handler in `app.js`
**API deps**: `POST /v1/constitutional/appeals` (exists but stub logic)

## 8. Phase 4 Tasks (5 tasks)

### P4-UI-1: CoopWallet MVP (React Native)
**What**: Build mobile wallet that can: create identity, scan QR codes, sign challenges, view balance, vote.
**Files**: New `apps/coop-wallet/` directory, React Native project
**Dependencies**: P0-UI-1 (crypto), P0-UI-2 (auth), SDK for API calls

### P4-UI-2: Mobile-First Identity (Signer → Lightweight Node)
**What**: Mobile app starts as pure signer (scan QR, sign challenges), evolves to lightweight node that can receive gossip.
**Dependencies**: P4-UI-1, icn-net QUIC library compiled for mobile

### P4-UI-3: Offline Queue + Sync
**What**: Queue write operations when offline, replay on reconnect with conflict detection.
**Files**: Extend pilot-ui service worker, add to mobile app
**Dependencies**: `offline-storage.js` exists but only caches reads

### P4-UI-4: Mobile Affiliations
**What**: Multi-org membership view on mobile with push notifications per org.
**Dependencies**: P4-UI-1, P1-UI-2 (affiliations data), notification infrastructure

### P4-UI-5: Steward Mobile Enrollment
**What**: Mobile-optimized SDIS enrollment with camera access for spatial proofs.
**Dependencies**: P4-UI-1, SDIS verification endpoints (exist but stub)

## 9. Relevant Open Issues

- **#1012**: "Constraints with Receipts" dashboard — directly maps to P0-UI-5
- QR login flow backend exists but no mobile client to complete it
- SDIS verification endpoints exist but have basic/stub logic
- Constitutional endpoints (amendments, appeals) exist but have minimal backend logic
- OpenAPI spec has schemas but empty paths (utoipa generation issue)
- Service Worker caches reads but doesn't queue writes
- i18n: English + Spanish only (pilot-ui has `locales/en.json`; icnctl has `en.yaml` + `es.yaml`)
