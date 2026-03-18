# SDIS Complete Build - Detailed Plan

**Session Start:** 2025-12-12 20:30 UTC
**Target:** Complete SDIS from API to Mobile
**Status:** IN PROGRESS

---

## ✅ What Already Exists

### Crypto & Core (100% Complete)
- ✅ icn-crypto-pq (51 tests passing)
- ✅ icn-steward (66 tests passing)  
- ✅ icn-zkp (42 tests passing)
- ✅ icn-identity (anchor, keybundle, VUI)

### Gateway API (Partial - 60% Complete)
- ✅ Ephemeral proof generation
- ✅ QR code encoding
- ✅ 3-tier verification (QR, NFC, Network)
- ❌ Enrollment endpoints
- ❌ Recovery endpoints
- ❌ Anchor management
- ❌ Steward integration

### UI (0% Complete)
- ❌ Pilot UI enrollment wizard
- ❌ Pilot UI identity viewer
- ❌ Pilot UI recovery flow
- ❌ Mobile enrollment screen
- ❌ Mobile credential wallet

---

## 🎯 Build Sequence

### STEP 1: Gateway API Completion (2-3 hours)

#### 1.1 Enrollment Endpoints
Create `icn/crates/icn-gateway/src/api/sdis/enrollment.rs`:

```rust
POST /v1/sdis/enrollment/start
Request:
{
  "pathway": "government_id" | "org_sponsor" | "web_of_trust" | "biometric",
  "proof_data": { ... },  // Pathway-specific verification data
  "initial_keybundle": { ... }  // Client-generated keys
}
Response:
{
  "ceremony_id": "...",
  "status": "pending_steward_verification",
  "required_stewards": 3,
  "estimated_completion": "2025-12-13T12:00:00Z"
}

GET /v1/sdis/enrollment/:ceremony_id
Response:
{
  "ceremony_id": "...",
  "status": "pending" | "approved" | "rejected",
  "steward_approvals": 2,
  "required_stewards": 3,
  "anchor": { ... } // Only present when approved
}

POST /v1/sdis/enrollment/:ceremony_id/finalize
Request:
{
  "vui_commitment": "..."
}
Response:
{
  "anchor_id": "...",
  "did": "did:icn:...",
  "keybundle_version": 1
}
```

#### 1.2 Recovery Endpoints
Create `icn/crates/icn-gateway/src/api/sdis/recovery.rs`:

```rust
POST /v1/sdis/recovery/start
Request:
{
  "anchor_id": "..." | "vui_hint": "...",
  "verification_data": { ... },
  "new_keybundle": { ... }
}
Response:
{
  "recovery_id": "...",
  "status": "pending_steward_verification"
}

GET /v1/sdis/recovery/:recovery_id
Response:
{
  "recovery_id": "...",
  "status": "pending" | "approved" | "rejected",
  "steward_approvals": 2,
  "required_stewards": 3
}

POST /v1/sdis/recovery/:recovery_id/complete
Response:
{
  "new_did": "did:icn:...",
  "keybundle_version": 2,
  "anchor_id": "..."  // Same anchor, new keys
}
```

#### 1.3 Anchor Management
Create `icn/crates/icn-gateway/src/api/sdis/anchor.rs`:

```rust
GET /v1/sdis/anchor/:anchor_id
Response:
{
  "anchor_id": "...",
  "created_at": 1702425600,
  "pathway": { ... },
  "current_did": "did:icn:...",
  "keybundle_version": 1,
  "devices": [...]
}

POST /v1/sdis/anchor/rotate-keys
Request:
{
  "anchor_id": "...",
  "new_keybundle": { ... }
}
Response:
{
  "new_did": "did:icn:...",
  "keybundle_version": 2
}
```

#### 1.4 Steward Integration
Modify `icn/crates/icn-gateway/src/server.rs`:

```rust
// Add StewardHandle to server state
use icn_steward::StewardHandle;

struct AppState {
    // ... existing fields
    steward: Arc<StewardHandle>,
}

// Initialize steward actor
let steward_handle = icn_steward::StewardActor::spawn(...);
```

---

### STEP 2: Pilot UI (1-2 days)

#### 2.1 Enrollment Wizard Component
Create `web/pilot-ui/src/components/EnrollmentWizard.js`:

```javascript
Screens:
1. Choose Pathway
   - Government ID
   - Organization Sponsorship
   - Web of Trust
   - Biometric

2. Upload Documents
   - ID scan
   - Selfie
   - Supplementary docs

3. Generate Keys
   - Client-side Ed25519 + ML-DSA
   - Show security notice

4. Submit for Verification
   - POST to /v1/sdis/enrollment/start

5. Wait for Stewards
   - Poll /v1/sdis/enrollment/:id
   - Show progress (2/3 stewards approved)

6. Receive Anchor
   - Display anchor ID
   - Download backup codes
   - Setup recovery contacts
```

#### 2.2 Identity Viewer Component
Create `web/pilot-ui/src/components/IdentityViewer.js`:

```javascript
Display:
- Anchor ID (permanent)
- Current DID (rotatable)
- KeyBundle version
- Enrollment pathway
- Trusted devices
- Recovery contacts
- Issued credentials
- Attestations received

Actions:
- Rotate keys
- Add device
- Update recovery contacts
- Export credentials
```

#### 2.3 Proof Generator Component
Create `web/pilot-ui/src/components/ProofGenerator.js`:

```javascript
Proof Types:
- Age (prove > 18, > 21, etc.)
- Membership (prove member of coop)
- Location (prove residency)
- Custom attribute

Flow:
1. Select proof type
2. Enter parameters
3. Generate ZK proof
4. Show QR code or send via network
```

#### 2.4 Recovery Flow Component
Create `web/pilot-ui/src/components/RecoveryFlow.js`:

```javascript
Steps:
1. Enter anchor ID or VUI hint
2. Verify identity (submit docs)
3. Wait for steward approval
4. Generate new keybundle
5. Complete recovery
6. Update all devices
```

---

### STEP 3: Mobile (CoopWallet) (1-2 days)

#### 3.1 Enrollment Screen
Create `sdk/react-native/examples/CoopWallet/src/screens/EnrollmentScreen.tsx`:

```typescript
Features:
- Camera integration (react-native-camera)
- ID document scanning
- Selfie capture
- Biometric auth (Face ID / Touch ID)
- Document upload
- Progress tracking
- Success celebration
```

#### 3.2 Identity Screen Enhancement
Update `src/screens/IdentityScreen.tsx`:

```typescript
Add SDIS Support:
- Show anchor ID
- Show current DID
- Show keybundle version
- Device management
- Recovery setup
- Backup codes
```

#### 3.3 Credential Wallet Screen
Create `src/screens/CredentialWalletScreen.tsx`:

```typescript
Features:
- List issued credentials
- Select credential for presentation
- Generate QR code
- NFC presentation (Android/iOS)
- Network verification
- Credential expiration tracking
```

#### 3.4 Proof Presentation Screen
Create `src/screens/PresentationScreen.tsx`:

```typescript
Tiers:
1. QR Code (offline, low security)
   - Generate ephemeral proof
   - Display QR
   - Verifier scans

2. NFC (proximity, medium security)
   - Challenge-response
   - Tap to present
   - Binding verification

3. Network (online, high security)
   - Full STARK verification
   - Real-time revocation check
   - Transaction-grade security
```

---

## 📦 Dependencies to Add

### Gateway
```toml
[dependencies]
# Add to icn-gateway/Cargo.toml
icn-steward = { path = "../icn-steward" }
```

### Mobile
```json
// Add to CoopWallet/package.json
"dependencies": {
  "react-native-camera": "^4.2.1",
  "react-native-nfc-manager": "^3.14.0",
  "@react-native-biometrics/core": "^3.0.0",
  "qrcode": "^1.5.3",
  "react-native-qrcode-scanner": "^1.5.5"
}
```

---

## 🧪 Testing Strategy

### Unit Tests
```bash
# Gateway API
cargo test -p icn-gateway sdis::

# Expected new tests:
- test_enrollment_lifecycle
- test_recovery_flow
- test_anchor_rotation
- test_steward_coordination
```

### Integration Tests
```bash
# End-to-end enrollment
1. Start enrollment via API
2. Mock steward approvals
3. Receive anchor
4. Verify anchor stored
5. Verify DID mapping

# End-to-end recovery
1. Start recovery
2. Mock verification
3. Get new keybundle
4. Verify anchor unchanged
5. Verify new DID
```

### Manual Testing
```
1. Web enrollment flow
2. Mobile enrollment flow
3. QR proof generation
4. NFC presentation (mobile)
5. Recovery from lost device
```

---

## 📊 Progress Tracking

### Phase 1: Gateway API (Target: 3 hours)
- [ ] enrollment.rs created
- [ ] recovery.rs created
- [ ] anchor.rs created
- [ ] Steward integrated
- [ ] Routes registered
- [ ] Tests written
- [ ] Build passes

### Phase 2: Pilot UI (Target: 1-2 days)
- [ ] EnrollmentWizard component
- [ ] IdentityViewer component
- [ ] ProofGenerator component
- [ ] RecoveryFlow component
- [ ] Integration with Gateway API
- [ ] Styling complete

### Phase 3: Mobile (Target: 1-2 days)
- [ ] EnrollmentScreen
- [ ] CredentialWallet
- [ ] PresentationScreen
- [ ] Camera integration
- [ ] NFC integration
- [ ] Biometric auth

---

## 🚀 Let's Start Building!

Beginning with Gateway API enrollment endpoints...


---

## ✅ SESSION PROGRESS UPDATE - 2025-12-12 20:35 UTC

### Phase 1: Gateway API - IN PROGRESS ✨

#### Completed:
- [x] enrollment.rs created (456 lines)
  - POST /v1/sdis/enrollment/start
  - GET /v1/sdis/enrollment/:id
  - POST /v1/sdis/enrollment/:id/finalize  
  - POST /v1/sdis/enrollment/:id/approve (testing)
  - 3 unit tests passing

- [x] recovery.rs created (432 lines)
  - POST /v1/sdis/recovery/start
  - GET /v1/sdis/recovery/:id
  - POST /v1/sdis/recovery/:id/complete
  - POST /v1/sdis/recovery/:id/approve (testing)
  - 7 unit tests passing

- [x] Module structure updated
- [x] All compilation errors fixed
- [x] 10 new tests passing

#### Next Steps:
- [ ] Create anchor.rs (anchor management API)
- [ ] Register routes in server.rs
- [ ] Add EnrollmentStore & RecoveryStore to server state
- [ ] Integration test for full enrollment flow
- [ ] Integration test for full recovery flow

**Time Elapsed:** 35 minutes
**Estimated Remaining:** 1.5-2 hours for Phase 1

