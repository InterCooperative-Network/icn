# CoopWallet Mobile App - Testing Instructions

## Prerequisites ✅

- ✅ Node.js v20.19.4 (installed)
- ✅ npm v11.7.0 (installed)
- ✅ Expo v54.0.19 (installed)
- ✅ ICN Gateway running at http://10.8.10.40:30080

## Option 1: Test in Web Browser (Easiest) 🌐

### Quick Start
```bash
/home/matt/projects/icn/scripts/start-mobile-app.sh
```

Then press `w` to open in your web browser.

### Manual Start
```bash
cd /home/matt/projects/icn/sdk/react-native/examples/CoopWallet
npm start
# Press 'w' when prompted
```

The app will open in your default browser at `http://localhost:8081`

---

## Option 2: Test on Your Phone 📱

### Requirements
- Install **Expo Go** app on your phone:
  - iOS: https://apps.apple.com/app/expo-go/id982107779
  - Android: https://play.google.com/store/apps/details?id=host.exp.exponent

### Steps
1. **Start the dev server**:
   ```bash
   /home/matt/projects/icn/scripts/start-mobile-app.sh
   ```

2. **Connect your phone to the same network** as your computer

3. **Scan the QR code**:
   - iOS: Open Camera app and scan the QR code
   - Android: Open Expo Go app and scan the QR code

4. The app will open in Expo Go

### Troubleshooting Network Issues
If the app can't connect to the gateway:

**Option A: Use your computer's local IP**
1. Find your computer's IP: `ip addr show | grep "inet " | grep -v 127.0.0.1`
2. Edit `src/config.ts` and change `GATEWAY_URL` to `http://<your-ip>:30080`
3. Restart the dev server

**Option B: Use a tunnel**
```bash
cd /home/matt/projects/icn/sdk/react-native/examples/CoopWallet
npx expo start --tunnel
```
This creates a public URL that your phone can access.

---

## Option 3: Test in Android Emulator 🤖

### Requirements
- Android Studio installed
- Android emulator configured

### Steps
```bash
cd /home/matt/projects/icn/sdk/react-native/examples/CoopWallet
npm run android
```

---

## Option 4: Test in iOS Simulator 🍎

### Requirements
- macOS with Xcode installed
- iOS Simulator configured

### Steps
```bash
cd /home/matt/projects/icn/sdk/react-native/examples/CoopWallet
npm run ios
```

---

## Testing the App

### First Time Setup

1. **App launches** - You'll see the Login screen

2. **Enter Gateway URL**: `http://10.8.10.40:30080`
   - If testing from phone and can't reach 10.8.10.40, use tunnel mode or your computer's IP

3. **Choose Login Method**:

   **Option A: New Identity (SDIS Enrollment)**
   - Tap "Join Cooperative"
   - Enter your name
   - Enter Cooperative ID: `test-coop`
   - Complete device verification
   - (Optional) Get steward to vouch for Level 2

   **Option B: Existing Identity**
   - If you have an existing DID, enter it
   - App will handle challenge-response auth

### Test Scenarios

#### 1. Check Balance
- After login, view your balance (will be 0.0 for new identity)
- Balance is shown in hours

#### 2. Send Payment to Yourself
- Tap "Send" or payment icon
- Enter recipient: Use your own DID (copy from profile)
- Enter amount: `0.1`
- Add memo: "Test payment"
- Tap "Send"
- Check that balance updates

#### 3. Generate Receive QR Code
- Tap "Receive"
- QR code displays with your DID
- Optionally set a suggested amount
- Share QR with another user to receive payment

#### 4. View Transaction History
- Tap "History" or transactions tab
- See list of all your transactions
- Tap transaction for details

#### 5. Browse Governance
- Tap "Governance" tab
- View list of proposals
- Tap a proposal to view details
- Cast a vote (For/Against/Abstain)

---

## Configuration

The app is pre-configured for your homelab deployment:

**File**: `src/config.ts`
```typescript
export const GATEWAY_URL = 'http://10.8.10.40:30080';
```

To change gateway URL:
1. Edit `src/config.ts`
2. Save the file
3. App will hot-reload automatically

---

## Verifying Backend Connectivity

### Test Gateway from Your Computer
```bash
curl http://10.8.10.40:30080/v1/health
# Should return: {"status":"ok","version":"0.1.0"}
```

### Test Gateway from Your Phone's Network
If you can access your homelab from your phone:
```bash
curl http://10.8.10.40:30080/v1/health
```

If you can't reach 10.8.10.40 from your phone:
- Use tunnel mode: `npx expo start --tunnel`
- Or update gateway URL to your computer's IP

---

## Common Issues

### "Cannot connect to Metro bundler"
- Make sure dev server is running: `npm start`
- Check firewall isn't blocking port 8081
- Try tunnel mode: `npx expo start --tunnel`

### "Network request failed" in app
- Gateway URL might be incorrect
- Phone can't reach 10.8.10.40 (use tunnel or your computer's IP)
- Test connectivity: `curl http://10.8.10.40:30080/v1/health`

### "Authentication failed"
- Make sure you completed the enrollment or have valid credentials
- Check gateway URL has no trailing slash
- Try creating a new identity

### Hot reload not working
- Press `r` in the terminal to reload manually
- Or shake your device and tap "Reload"

---

## Development Commands

### Start dev server
```bash
npm start
```

### Start with tunnel (for phone testing)
```bash
npx expo start --tunnel
```

### Clear cache
```bash
npx expo start --clear
```

### View logs
```bash
# In the Expo dev server, press:
# - 'j' to open debugger
# - 'shift+m' to toggle menu
```

---

## Backend Monitoring

### Check if gateway is healthy
```bash
curl http://10.8.10.40:30080/v1/health
curl http://10.8.10.40:30080/v1/sdis/health
```

### View gateway logs
```bash
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn logs -f deployment/icn-daemon"
```

### Check pod status
```bash
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn get pods"
```

---

## What to Test

- [ ] App launches successfully
- [ ] Can enter gateway URL and coop ID
- [ ] SDIS enrollment creates new identity
- [ ] Login with existing identity works
- [ ] Balance displays correctly
- [ ] Can send payment to self
- [ ] Transaction appears in history
- [ ] Can generate receive QR code
- [ ] Can view governance proposals
- [ ] Can cast votes
- [ ] Real-time updates work (send payment, see balance change)

---

## Getting Help

- **Gateway API Issues**: Check `MOBILE_APP_DEPLOYMENT_STATUS.md`
- **App Issues**: Check `sdk/react-native/examples/CoopWallet/README.md`
- **SDIS Issues**: Check `docs/sdis/SDIS_API_GUIDE.md`

---

## Success Criteria

✅ App loads and shows login screen
✅ Can create new identity via SDIS
✅ Can authenticate with gateway
✅ Balance displays (even if 0)
✅ Can send test payment
✅ Transaction appears in history
✅ Governance proposals load
✅ Can cast votes

**Status**: Ready to test! 🚀

Start with: `/home/matt/projects/icn/scripts/start-mobile-app.sh`
