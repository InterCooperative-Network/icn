# Mobile Integration TODO

## Completed ✅

### Phase 1: Core Infrastructure
- [x] ICN Context Provider with global state management
- [x] Trust Graph Visualization (D3.js force-directed)
- [x] Trust Attestation Form UI component
- [x] Offline storage with IndexedDB
- [x] Service worker with caching strategies
- [x] Sync manager for background operations

### Phase 2: Push Notifications
- [x] Notification listener methods in ICNClient
- [x] useNotifications hook for React
- [x] WebSocket event definitions (TrustAttested, TrustRevoked, TransactionCompleted)
- [x] Trust API endpoints with event emission
- [x] Trust routes registered in gateway server

### Phase 3: Trust Graph Integration  
- [x] Trust API backend (attest, revoke, score, edges, network)
- [x] TrustManager with edge management
- [x] Trust events broadcast to personal DID channels
- [x] TrustGraph component wired to WebSocket updates
- [x] useTrustGraph and useTrustScore hooks

## Remaining Work 🔄

### 1. Transaction Event Emission
**Priority: HIGH**
**Effort: 2 hours**

The ledger operations need to emit `TransactionCompleted` events:

**Files to modify:**
- `icn/crates/icn-gateway/src/api/ledger.rs`
- `icn/crates/icn-gateway/src/ledger_mgr.rs`

**Changes needed:**
```rust
// In create_payment endpoint
let event = GatewayEvent::TransactionCompleted {
    coop_id: coop_id.clone(),
    hash: payment_hash,
    from: from_did.to_string(),
    to: to_did.to_string(),
    amount,
    currency: currency.clone(),
};
event_broadcaster.broadcast(&coop_id, event).await;
```

### 2. Conflict Resolution UI
**Priority: MEDIUM**
**Effort: 4 hours**

When offline operations sync, conflicts may arise. Need UI to handle:

**Files to create:**
- `web/app/components/ConflictResolutionDialog.tsx`
- `web/app/utils/conflictResolver.ts`

**Features:**
- Show conflicting operations side-by-side
- Allow user to choose which to keep
- Automatic resolution strategies (last-write-wins, merge)
- Undo/redo support

### 3. Error Boundaries
**Priority: MEDIUM**
**Effort: 2 hours**

Prevent component crashes from breaking the entire app:

**Files to create:**
- `web/app/components/ErrorBoundary.tsx`

**Features:**
- Catch React component errors
- Display friendly error message
- Log errors for debugging
- Provide reset button

### 4. Native Push Notifications
**Priority: MEDIUM**
**Effort: 8 hours**

For iOS/Android apps, integrate native push:

**Files to modify:**
- `web/app/utils/pushNotifications.ts` (create)
- FCM/APNs configuration

**Features:**
- Request push permission
- Register device token with gateway
- Handle background notifications
- Deep linking to relevant screens

### 5. Trust Recommendations
**Priority: LOW**
**Effort: 6 hours**

Suggest trust attestations based on graph analysis:

**Files to create:**
- `web/app/components/TrustRecommendations.tsx`
- `icn/crates/icn-gateway/src/api/trust_recommendations.rs`

**Algorithm:**
- Find mutual connections
- Calculate trust transitivity
- Rank by interaction frequency
- Filter by trust threshold

### 6. Biometric Authentication
**Priority: LOW**
**Effort: 4 hours**

Add fingerprint/Face ID for sensitive operations:

**Files to create:**
- `web/app/utils/biometric.ts`

**Features:**
- Detect biometric availability
- Prompt for biometric auth before signing
- Fallback to PIN/password
- Secure keychain storage

### 7. Performance Monitoring
**Priority: LOW**
**Effort: 3 hours**

Add Sentry or similar for production monitoring:

**Files to modify:**
- `web/app/App.tsx` (add Sentry init)
- `web/app/utils/analytics.ts` (create)

**Metrics:**
- Error rates
- API latency
- WebSocket disconnects
- User journey tracking

### 8. A/B Testing Framework
**Priority: LOW**
**Effort: 4 hours**

Test different UI variations:

**Files to create:**
- `web/app/utils/experiments.ts`
- `web/app/hooks/useExperiment.ts`

**Features:**
- Feature flags
- Variant assignment
- Metrics collection
- Admin dashboard

## Quick Wins (< 1 hour each)

1. **Add loading states** to all async operations
2. **Add success animations** for trust attestations
3. **Implement pull-to-refresh** on mobile screens
4. **Add haptic feedback** for button presses
5. **Add skeleton loaders** for data fetching
6. **Improve accessibility** (aria labels, screen reader support)
7. **Add keyboard shortcuts** for power users
8. **Implement dark mode** toggle
9. **Add connection quality indicator** (WebSocket status)
10. **Create onboarding tutorial** for first-time users

## Testing Checklist

- [ ] Test offline → online transition
- [ ] Test WebSocket reconnection
- [ ] Test trust attestation flow end-to-end
- [ ] Test with 100+ trust graph nodes
- [ ] Test on slow 3G connection
- [ ] Test with poor network (packet loss)
- [ ] Test memory usage over 24 hours
- [ ] Test battery impact
- [ ] Load test with 1000 notifications/sec
- [ ] Security audit of API endpoints

## Documentation Needs

- [ ] API reference for trust endpoints
- [ ] WebSocket event catalog
- [ ] Mobile app user guide
- [ ] Offline mode behavior documentation
- [ ] Trust graph visualization guide
- [ ] Deployment guide updates
- [ ] Troubleshooting FAQ
- [ ] Privacy policy for notifications

## Deployment Preparation

- [ ] Set up FCM project for push notifications
- [ ] Configure APNs certificates
- [ ] Set up CDN for static assets
- [ ] Configure production WebSocket server
- [ ] Set up monitoring (Sentry, Datadog)
- [ ] Create CI/CD pipeline
- [ ] Set up staging environment
- [ ] Create rollback procedure
- [ ] Load test with expected traffic
- [ ] Security penetration testing

## Current Status

**Lines of Code:** ~5000 (mobile + backend integration)
**Test Coverage:** 148 gateway tests passing
**Features Complete:** ~70%
**Production Ready:** ~60%

**Estimated Time to MVP:** 2-3 weeks
**Estimated Time to Production:** 4-6 weeks
