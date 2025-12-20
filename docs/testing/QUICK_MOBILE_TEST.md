# Quick Mobile App Testing Guide

## ✅ Status: Ready for Testing

**Gateway**: http://10.8.10.40:30080  
**Last Updated**: 2025-12-16

---

## Quick Start (5 minutes)

### 1. Start the Mobile App
```bash
cd /home/matt/projects/icn/sdk/react-native/examples/CoopWallet
npm start
```

Press `i` for iOS, `a` for Android, or `w` for web.

### 2. First Time Setup
In the app:
1. Enter Gateway URL: `http://10.8.10.40:30080`
2. Enter Cooperative ID: `test-coop`
3. Choose "New Identity" or "Import Existing"

### 3. Test Core Features
- ✅ Check your balance
- ✅ Send a test payment (0.1 hours to yourself)
- ✅ View transaction history
- ✅ Browse governance proposals
- ✅ Cast a vote

---

## API Quick Tests

### Check Gateway Health
```bash
curl http://10.8.10.40:30080/v1/health
# Should return: {"status":"ok","version":"0.1.0"}
```

### Check SDIS Health
```bash
curl http://10.8.10.40:30080/v1/sdis/health
# Should return: {"service":"sdis","status":"ok","version":"1.0"}
```

### Test SDIS Enrollment
```bash
/home/matt/projects/icn/scripts/test-sdis-enrollment.sh
```

---

## Common Issues

### "Cannot connect to gateway"
- Check gateway is running: `ssh ubuntu@10.8.10.40 "sudo kubectl -n icn get pods"`
- Test endpoint: `curl http://10.8.10.40:30080/v1/health`

### "Authentication failed"
- Make sure you created an identity first
- Check gateway URL is exactly `http://10.8.10.40:30080` (no trailing slash)

### "Balance shows as 0"
- This is normal for new identities
- Send yourself a test payment to see transaction flow

---

## Advanced Testing

### Generate Test Identity (Command Line)
```bash
cd /home/matt/projects/icn/icn
cargo build --release --bin icnctl
ICN_PASSPHRASE="test" ./target/release/icnctl id init
./target/release/icnctl id show
```

### Get Auth Token
```bash
ICN_PASSPHRASE="test" ./target/release/icnctl auth token \
  --gateway http://10.8.10.40:30080 \
  --coop-id test-coop
```

### Test Authenticated API Call
```bash
TOKEN="<your-token-here>"
curl -H "Authorization: Bearer $TOKEN" \
  http://10.8.10.40:30080/v1/ledger/test-coop/balance/<your-did>
```

---

## Monitoring

### Check Pod Status
```bash
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn get pods"
```

### View Logs
```bash
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn logs -f deployment/icn-daemon"
```

### Grafana Dashboard
Open: http://10.8.10.40:30300

---

## Need Help?

- Full deployment status: `MOBILE_APP_DEPLOYMENT_STATUS.md`
- API documentation: `icn/crates/icn-gateway/README.md`
- SDIS guide: `docs/sdis/SDIS_API_GUIDE.md`

---

**All systems green! Happy testing! 🎉**
