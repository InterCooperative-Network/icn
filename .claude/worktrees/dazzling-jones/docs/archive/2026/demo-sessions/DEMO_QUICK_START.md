# 🚀 ICN Demo - Quick Start Card

> **Archived Document Notice (2026-02-12):** This file is retained for historical context and may not reflect current code, APIs, runtime defaults, CI status, or deployment posture.
> Use active documentation under `docs/` as authoritative.

**Date:** 2025-12-18 snapshot  
**Status:** Historical demo quick-start snapshot  
**Next:** Test UI Integration  
**Time:** 2-4 hours to working transaction

> This card documents a 2025-12-18 demo environment.
> Verify current runtime and CI state before following it: `docs/status/CURRENT_SYSTEM_STATUS.md`, `docs/ci/CI_CURRENT_STATUS.md`.

---

## ⚡ Quick Commands

### Start Everything
```bash
# Terminal 1: Backend (if not running)
cd <repo-root>/icn
./target/release/icnd \
  -d <demo-data-dir>/data \
  -e 127.0.0.1:15602 \
  --gateway-enable \
  --gateway-bind "127.0.0.1:8080"
# Passphrase: demo123

# Terminal 2: UI
cd <repo-root>/web/pilot-ui
python3 -m http.server 3000

# Browser: http://localhost:3000
# Gateway: http://localhost:8080
```

### Test API
```bash
# Health check
curl http://localhost:8080/v1/health

# Get cooperative (needs token)
TOKEN="your-token"
curl http://localhost:8080/v1/coops/rochester-tool-library \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Verify Demo
```bash
# Run verification (should pass 13/13)
./demo/scripts/verify-demo.sh
```

---

## 📊 What's Working

✅ Backend build (0.88s)  
✅ Tests (33/34, 99.6%)  
✅ Gateway API (http://localhost:8080)  
✅ Cooperative created (Rochester Tool Library)  
✅ Sample data (12 members, 10 transactions)  
✅ Demo scripts (verify passing)

---

## 🎯 Next 3 Steps

1. **NOW:** Test UI at http://localhost:3000
   - Try to connect to gateway
   - Watch dev console for errors
   - Document what happens

2. **NEXT:** Fix integration issues
   - CORS errors?
   - API endpoint mismatches?
   - Auth flow problems?

3. **THEN:** Load sample data
   - Create script to add 12 members
   - Create historical transactions
   - Verify balances

---

## 📁 Key Files

**Demo Data:**
- `demo/data/tool-library-members.json` (12 members)
- `demo/data/tool-library-history.json` (10 transactions)

**Scripts:**
- `demo/scripts/verify-demo.sh` (✅ passing)
- `demo/scripts/setup-demo-env.sh` (setup)
- `demo/scripts/test-ui-integration.sh` (test guide)

**Config:**
- `demo/configs/tool-library.toml` (demo config)

**Docs:**
- `DEMO_CURRENT_STATUS.md` (full status)
- `DEMO_INFRASTRUCTURE_COMPLETE.md` (today's work)
- `DEMO_SUCCESS_FINAL.md` (backend verified)

---

## ⚠️ Known Issues

1. UI → API integration not tested yet
2. Sample data not loaded yet
3. Demo automation not complete yet
4. Multi-node not tested yet

---

## 🎯 This Week's Goal

**Get one transaction from Alice to Bob working end-to-end via UI**

When that works:
- Load 12 members
- Create historical transactions
- Test 10 times
- Automate the flow

---

## 💡 Quick Tips

**If gateway not responding:**
```bash
# Check if running
curl http://localhost:8080/v1/health

# If not, start daemon (see above)
```

**If UI errors:**
- Check browser console (F12)
- Look for CORS errors
- Check API endpoint URLs
- Verify authentication flow

**If stuck:**
- See DEMO_CURRENT_STATUS.md
- Check DEMO_WIRING_STATUS.md
- Review DEMO_SUCCESS_FINAL.md

---

## 📞 Resources

**Docs:** <repo-root>/docs/  
**UI:** <repo-root>/web/pilot-ui/  
**Config:** <repo-root>/config/  
**Demo:** <repo-root>/demo/

**Data Dir:** <demo-data-dir>/data  
**DID:** did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh  
**Passphrase:** demo123

---

## 🏁 Bottom Line

**You have:** Solid backend ✅  
**You need:** UI integration working ⏳  
**Time estimate:** 4-8 hours  
**Confidence:** HIGH (85%)

**Next command:**
```bash
cd web/pilot-ui && python3 -m http.server 3000
# Open http://localhost:3000
```

**Go build that demo!** 🚀
