# ICN Mobile App

A complete mobile application for the Intercooperative Network (ICN), built with React Native and Expo.

## Features

### 🏠 Home Dashboard
- Real-time balance display
- Quick stats (cooperatives, notifications)
- Quick action buttons
- Pull-to-refresh

### 💰 Ledger Management
- Transaction history
- Create payments
- Balance tracking
- Credit limit monitoring

### 🗳️ Governance
- View proposals
- Cast votes (For/Against/Abstain)
- Create new proposals
- Track voting results

### 🤝 Cooperatives
- Browse cooperatives
- Join/leave cooperatives
- View member lists
- Create new cooperatives

### 👤 Profile & Settings
- User profile management
- Budget management
- Recurring payment setup
- Push notification preferences

## Quick Start

### Prerequisites

- Node.js 16+ and npm
- Expo CLI: `npm install -g expo-cli`
- iOS Simulator (Mac) or Android Studio
- Or Expo Go app on your phone

### Installation

```bash
cd examples/mobile-app
npm install
```

### Running

```bash
# Start development server
npm start

# Run on iOS (Mac only)
npm run ios

# Run on Android
npm run android

# Run on web
npm run web
```

### On Physical Device

1. Install Expo Go from App Store/Google Play
2. Run `npm start`
3. Scan QR code with camera (iOS) or Expo Go (Android)

## Configuration

### API Endpoint

Configure in login screen:
- Local: `http://localhost:8000`
- Network: `http://192.168.1.x:8000`
- Production: `https://api.your-coop.org`

### Authentication

1. Enter DID and API URL
2. Request challenge
3. Sign challenge with private key
4. Paste signature to authenticate
5. JWT token stored in AsyncStorage

## Project Structure

```
icn-mobile/
├── App.tsx                    # Main entry
├── src/
│   ├── screens/              # Main screens
│   ├── services/             # API client
│   ├── contexts/             # React contexts
│   └── components/           # Reusable components
├── BudgetManager.tsx         # Budget management
├── CooperativeManager.tsx    # Cooperative CRUD
├── NotificationCenter.tsx    # Notifications
├── RecurringPaymentSetup.tsx # Recurring payments
└── VotingScreen.tsx          # Governance voting
```

## Building for Production

### iOS (Mac only)

```bash
expo build:ios
```

### Android

```bash
expo build:android
```

## Troubleshooting

### "Unable to connect to server"
- Check API URL
- Ensure Gateway is running
- Use IP address (not localhost) for physical devices

### Cache Issues
```bash
expo start -c
```

## License

MIT - See [LICENSE](../../LICENSE)

## Support

- Docs: https://github.com/InterCooperative-Network/icn/tree/main/docs
- Issues: https://github.com/InterCooperative-Network/icn/issues

---

Built with ❤️ by the ICN community
