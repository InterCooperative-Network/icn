# Fix "Session Expired" Error

> **Archived Document Notice (2026-02-12):** This file is retained for historical context and may not reflect current code, APIs, runtime defaults, CI status, or deployment posture.
> Use active documentation under `docs/` as authoritative.

## The Problem

The UI is loading an OLD token from localStorage (browser cache). That token has expired.

## The Solution

### Option 1: Clear Browser Storage (Recommended)

1. Open **Developer Tools** (Press F12)
2. Go to **Application** tab (Chrome) or **Storage** tab (Firefox)
3. On the left, find **Local Storage** → **http://localhost:3000**
4. Click **Clear All** or delete these keys:
   - `icn-token`
   - `icn-token-expiry`
   - `icn-gateway`
   - `icn-coop`
   - `icn-did`
5. **Refresh the page** (F5)
6. **Login again** with the new credentials

### Option 2: Use Console to Clear

1. Press **F12** to open Developer Tools
2. Go to **Console** tab
3. Paste this command and press Enter:
   ```javascript
   localStorage.clear();
   location.reload();
   ```
4. **Login again** with the new credentials

### Option 3: Private/Incognito Window

1. Open a **new private/incognito window**
2. Go to http://localhost:3000
3. **Login** with the new credentials

### Option 4: Logout First

If you can see the dashboard:
1. Click your profile/settings
2. Click **Sign Out** or **Logout**
3. **Login again** with the new credentials

---

## Fresh Credentials to Use

**Gateway URL:** `http://localhost:8080`  
**Cooperative ID:** `rochester-tool-library`  
**DID:** `did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh`

**Token:**
```
eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJkaWQ6aWNuOnpCRm5oSmhndlJqZ3VraFFta3E5ZGRCejV3aUV0MzJwdGtRa0JEald4NnVQaCIsImlhdCI6MTc2NjA5NTA3MiwiZXhwIjoxNzY2MDk4NjcyLCJjb29wX2lkIjoicm9jaGVzdGVyLXRvb2wtbGlicmFyeSIsInNjb3BlcyI6WyJsZWRnZXI6cmVhZCIsImxlZGdlcjp3cml0ZSIsImNvb3A6cmVhZCIsImdvdjpyZWFkIiwiZ292OndyaXRlIl19.DvDXir5YbuSa5hLcg6WsBt6dZBIr9K4Drcc5bZFA6TM
```

**Expires:** 22:57 UTC (58 minutes from now)

---

## Quick Console Command

Press F12, go to Console tab, paste this:

```javascript
// Clear old credentials
localStorage.clear();

// Set new credentials
localStorage.setItem('icn-gateway', 'http://localhost:8080');
localStorage.setItem('icn-coop', 'rochester-tool-library');
localStorage.setItem('icn-did', 'did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh');
localStorage.setItem('icn-token', 'eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJkaWQ6aWNuOnpCRm5oSmhndlJqZ3VraFFta3E5ZGRCejV3aUV0MzJwdGtRa0JEald4NnVQaCIsImlhdCI6MTc2NjA5NTA3MiwiZXhwIjoxNzY2MDk4NjcyLCJjb29wX2lkIjoicm9jaGVzdGVyLXRvb2wtbGlicmFyeSIsInNjb3BlcyI6WyJsZWRnZXI6cmVhZCIsImxlZGdlcjp3cml0ZSIsImNvb3A6cmVhZCIsImdvdjpyZWFkIiwiZ292OndyaXRlIl19.DvDXir5YbuSa5hLcg6WsBt6dZBIr9K4Drcc5bZFA6TM');
localStorage.setItem('icn-token-expiry', '1766098672000');

// Reload page
location.reload();
```

This will automatically set everything and reload the page. You should go straight to the dashboard!

---

## Verify It Worked

After clearing and logging in, check the Console (F12) for:
- ✅ No more "401 Unauthorized" errors
- ✅ No more "session expired" messages
- ✅ Balance loads (even if empty)
- ✅ Dashboard displays

---

**The token IS valid and works with the API - it's just a caching issue!**
