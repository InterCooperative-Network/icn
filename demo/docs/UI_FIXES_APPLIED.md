# UI API Endpoint Fixes - 2025-12-18

## Issue
UI was using old API endpoint paths that didn't match the gateway implementation.

## Fixes Applied

### 1. Balance Endpoint (2 occurrences)
**Before:**
```javascript
`/ledger/${state.coopId}/balance/${encodeURIComponent(state.did)}`
```

**After:**
```javascript
`/ledger/coops/${state.coopId}/balances/${encodeURIComponent(state.did)}`
```

**Locations:**
- Line 436 (login verification)
- Line 511 (balance refresh)

### 2. History Endpoint (1 occurrence)
**Before:**
```javascript
`/ledger/${state.coopId}/history?limit=50`
```

**After:**
```javascript
`/ledger/coops/${state.coopId}/history?limit=50`
```

**Location:**
- Line 629 (loadTransactions function)

### 3. Payment Endpoint (1 occurrence)
**Before:**
```javascript
`/ledger/${state.coopId}/payment`
```

**After:**
```javascript
`/ledger/coops/${state.coopId}/payments`
```

**Location:**
- Line 1369 (transaction creation)

## Summary

**Total Fixes:** 4 endpoint paths  
**File Modified:** `web/pilot-ui/app.js`  
**Status:** ✅ All ledger endpoints now match gateway routes

## Testing

To verify these fixes work:

1. Start gateway (if not running)
2. Get JWT token via icnctl
3. Open UI at http://localhost:3000
4. Login with:
   - Gateway: http://localhost:8080
   - Coop: rochester-tool-library
   - DID: did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh
   - Token: (from icnctl)
5. Check that:
   - Login succeeds ✅
   - Balance loads ✅
   - Transaction history loads ✅
   - Can create transaction ✅

## Next Steps

- [ ] Test login flow
- [ ] Verify balance display
- [ ] Test transaction creation
- [ ] Load sample members data
- [ ] Create historical transactions

---

*Fixes Applied: 2025-12-18 21:40 UTC*  
*Status: Ready for testing*
