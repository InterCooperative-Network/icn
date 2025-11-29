# Coop Wallet

Example React Native app demonstrating the ICN mobile SDK.

## Features

- **Secure Authentication** - Login with your cooperative using Ed25519 keys stored in device secure storage
- **Balance Display** - View your hour balance with real-time updates
- **Send Payments** - Transfer hours to other members
- **QR Payments** - Scan-to-pay and receive via QR codes
- **Governance** - View and vote on cooperative proposals

## Screenshots

```
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│    Login        │  │    Home         │  │    Payment      │
│                 │  │                 │  │                 │
│  ┌───────────┐  │  │  Balance: 42h   │  │  To: did:icn:.. │
│  │  Coop ID  │  │  │                 │  │  Amount: 5      │
│  └───────────┘  │  │  [Send][Receive]│  │  Memo: Thanks!  │
│                 │  │  [Scan][Vote]   │  │                 │
│  [  Login  ]    │  │                 │  │  [Send Payment] │
└─────────────────┘  └─────────────────┘  └─────────────────┘
```

## Getting Started

### Prerequisites

- Node.js 18+
- Expo CLI (`npm install -g expo-cli`)
- iOS Simulator or Android Emulator (or Expo Go app)

### Installation

```bash
# Navigate to example app
cd sdk/react-native/examples/CoopWallet

# Install dependencies
npm install

# Start development server
npm start
```

### Configuration

Edit `src/client.ts` to point to your ICN gateway:

```typescript
const GATEWAY_URL = 'https://icn.mycoop.org';
```

### Running on Device

```bash
# iOS Simulator
npm run ios

# Android Emulator
npm run android

# Expo Go (scan QR code)
npm start
```

## App Structure

```
CoopWallet/
├── App.tsx                 # Navigation setup
├── src/
│   ├── client.ts           # ICN client configuration
│   └── screens/
│       ├── LoginScreen.tsx     # Cooperative login
│       ├── HomeScreen.tsx      # Balance & quick actions
│       ├── PaymentScreen.tsx   # Send hours
│       ├── ScanScreen.tsx      # QR code scanner
│       ├── ReceiveScreen.tsx   # QR code generator
│       ├── GovernanceScreen.tsx# Proposal list
│       └── ProposalScreen.tsx  # Vote on proposals
└── package.json
```

## Core Flows

### Authentication

1. App loads and checks for existing auth
2. If no auth, shows login screen
3. User enters cooperative ID
4. SDK generates/uses Ed25519 keypair from secure storage
5. Challenge-response authentication with gateway
6. JWT token stored securely

### Payments

1. **Send**: Enter recipient DID, amount, memo
2. **Scan**: Camera scans `icn://pay?...` QR code, pre-fills payment
3. **Receive**: Generate QR code with your DID and optional amount

### Governance

1. Fetches open proposals from `coop:{coopId}` domain
2. Displays proposal list with current tally
3. Tap to view details and cast vote (for/against/abstain)
4. Real-time updates via WebSocket

## SDK Usage Examples

### Setup Client

```typescript
import { createWallet, createMobileClient, SecureStorage } from '@icn/react-native';
import * as SecureStore from 'expo-secure-store';

const storage: SecureStorage = {
  setItem: (key, value) => SecureStore.setItemAsync(key, value),
  getItem: (key) => SecureStore.getItemAsync(key),
  removeItem: (key) => SecureStore.deleteItemAsync(key),
  hasItem: async (key) => (await SecureStore.getItemAsync(key)) !== null,
};

const wallet = createWallet(storage);
const client = createMobileClient({
  baseUrl: 'https://icn.mycoop.org',
  wallet,
  storage,
});
```

### Use Hooks

```tsx
import { useAuth, useBalance, usePayment } from '@icn/react-native';

function MyComponent() {
  const { isAuthenticated, did, login, logout } = useAuth(client);
  const { balance, refresh } = useBalance(client, coopId, did);
  const { pay, isPaying } = usePayment(client, coopId);

  // ...
}
```

### QR Payments

```tsx
import { generateReceiveQR, parsePaymentQR } from '@icn/react-native';

// Generate QR for receiving
const qrData = generateReceiveQR(myDid, coopId, {
  suggestedAmount: 10,
  memo: 'For tutoring',
});

// Parse scanned QR
const payment = parsePaymentQR(scannedData);
if (payment) {
  await client.pay(payment.coopId, {
    to: payment.to,
    amount: payment.amount,
  });
}
```

## Customization

### Theming

The app uses a consistent color scheme:
- Primary: `#4A90A4` (teal)
- Success: `#4caf50` (green)
- Error: `#e53935` (red)

Modify `StyleSheet` objects in each screen to customize.

### Adding Screens

1. Create new screen in `src/screens/`
2. Export from `src/screens/index.ts`
3. Add to navigation stack in `App.tsx`
4. Update `RootStackParamList` type

## Troubleshooting

### Camera not working

Ensure camera permissions are granted:
```bash
expo install expo-camera
```

### Secure storage errors

On Android emulator, secure storage may not work. Use a physical device or set up the emulator with Google Play services.

### WebSocket connection issues

Check that your gateway URL is correct and accessible from your device/emulator.

## License

MIT OR Apache-2.0
