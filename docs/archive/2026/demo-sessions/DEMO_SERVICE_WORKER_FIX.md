# Fix Service Worker Caching Issue

> **Archived Document Notice (2026-02-12):** This file is retained for historical context and may not reflect current code, APIs, runtime defaults, CI status, or deployment posture.
> Use active documentation under `docs/` as authoritative.

## The Problem

The Service Worker is caching failed API requests (401/503) and serving them even after you login. This causes "offline" errors.

## The Solution

### Option 1: Disable Service Worker in DevTools (Easiest)

1. **Press F12** to open Developer Tools
2. Go to **Application** tab (Chrome) or **Storage** tab (Firefox)
3. On the left, click **Service Workers**
4. Check the box: **"Bypass for network"** or **"Update on reload"**
5. Click **"Unregister"** to remove the service worker
6. **Refresh the page** (F5)
7. Now run the console command to set credentials:

```javascript
localStorage.clear();
localStorage.setItem('icn-gateway', 'http://localhost:8080');
localStorage.setItem('icn-coop', 'rochester-tool-library');
localStorage.setItem('icn-did', 'did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh');
localStorage.setItem('icn-token', 'eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJkaWQ6aWNuOnpCRm5oSmhndlJqZ3VraFFta3E5ZGRCejV3aUV0MzJwdGtRa0JEald4NnVQaCIsImlhdCI6MTc2NjA5NTA3MiwiZXhwIjoxNzY2MDk4NjcyLCJjb29wX2lkIjoicm9jaGVzdGVyLXRvb2wtbGlicmFyeSIsInNjb3BlcyI6WyJsZWRnZXI6cmVhZCIsImxlZGdlcjp3cml0ZSIsImNvb3A6cmVhZCIsImdvdjpyZWFkIiwiZ292OndyaXRlIl19.DvDXir5YbuSa5hLcg6WsBt6dZBIr9K4Drcc5bZFA6TM');
localStorage.setItem('icn-token-expiry', '1766098672000');
location.reload();
```

### Option 2: Clear All Site Data (Nuclear Option)

1. **Press F12**
2. Go to **Application** tab
3. On the left, click **Storage** → **Clear site data**
4. Click **"Clear site data"** button
5. **Close and reopen** the browser tab
6. Go back to http://localhost:3000
7. Run the console command above

### Option 3: Use Private/Incognito Window (Cleanest)

1. Open a **new private/incognito window** (Ctrl+Shift+N or Cmd+Shift+N)
2. Go to http://localhost:3000
3. **Press F12** → **Console**
4. Paste the console command above
5. The service worker won't interfere in private mode

---

## Verify API Works

The API is working fine! Test it yourself:

```bash
TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJkaWQ6aWNuOnpCRm5oSmhndlJqZ3VraFFta3E5ZGRCejV3aUV0MzJwdGtRa0JEald4NnVQaCIsImlhdCI6MTc2NjA5NTA3MiwiZXhwIjoxNzY2MDk4NjcyLCJjb29wX2lkIjoicm9jaGVzdGVyLXRvb2wtbGlicmFyeSIsInNjb3BlcyI6WyJsZWRnZXI6cmVhZCIsImxlZGdlcjp3cml0ZSIsImNvb3A6cmVhZCIsImdvdjpyZWFkIiwiZ292OndyaXRlIl19.DvDXir5YbuSa5hLcg6WsBt6dZBIr9K4Drcc5bZFA6TM"

curl "http://localhost:8080/v1/coops/rochester-tool-library" \
  -H "Authorization: Bearer $TOKEN" | jq .

curl "http://localhost:8080/v1/ledger/rochester-tool-library/balance/did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Both work perfectly! ✅

---

## Complete Fix Steps (Recommended)

1. **Open Private/Incognito Window**
   - Ctrl+Shift+N (Chrome) or Ctrl+Shift+P (Firefox)

2. **Go to:** http://localhost:3000

3. **Press F12** → **Console**

4. **Paste and run:**
   ```javascript
   localStorage.setItem('icn-gateway', 'http://localhost:8080');
   localStorage.setItem('icn-coop', 'rochester-tool-library');
   localStorage.setItem('icn-did', 'did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh');
   localStorage.setItem('icn-token', 'eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJkaWQ6aWNuOnpCRm5oSmhndlJqZ3VraFFta3E5ZGRCejV3aUV0MzJwdGtRa0JEald4NnVQaCIsImlhdCI6MTc2NjA5NTA3MiwiZXhwIjoxNzY2MDk4NjcyLCJjb29wX2lkIjoicm9jaGVzdGVyLXRvb2wtbGlicmFyeSIsInNjb3BlcyI6WyJsZWRnZXI6cmVhZCIsImxlZGdlcjp3cml0ZSIsImNvb3A6cmVhZCIsImdvdjpyZWFkIiwiZ292OndyaXRlIl19.DvDXir5YbuSa5hLcg6WsBt6dZBIr9K4Drcc5bZFA6TM');
   localStorage.setItem('icn-token-expiry', '1766098672000');
   location.reload();
   ```

5. **Dashboard should load!** ✅

---

## What You Should See

After fixing:
- ✅ Dashboard loads
- ✅ Balance shows (even if empty: `{}`)
- ✅ Member shows: You (Steward)
- ✅ No "offline" errors
- ✅ No 503 errors

---

**The service worker is meant to help with offline support, but it's caching failed auth requests. Disabling it or using incognito fixes the issue!**

**Token expires: 22:57 UTC (55 minutes remaining)**
