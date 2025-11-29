# @icn/react-native

React Native SDK for the InterCooperative Network - mobile-first mutual credit and cooperative coordination.

## Features

- **Wallet Management** - Secure key storage with iOS Keychain / Android Keystore
- **Authentication** - Persistent login with automatic token refresh
- **Real-time Events** - WebSocket with auto-reconnect
- **QR Payments** - Generate and scan payment QR codes
- **React Hooks** - Easy integration with React Native apps

## Installation

```bash
npm install @icn/react-native @icn/client

# For secure storage (choose one):
npm install react-native-keychain
# or
expo install expo-secure-store
```

## Quick Start

### 1. Set up secure storage

```typescript
import * as Keychain from 'react-native-keychain';
import { SecureStorage } from '@icn/react-native';

const secureStorage: SecureStorage = {
  async setItem(key, value) {
    await Keychain.setGenericPassword(key, value, { service: key });
  },
  async getItem(key) {
    const result = await Keychain.getGenericPassword({ service: key });
    return result ? result.password : null;
  },
  async removeItem(key) {
    await Keychain.resetGenericPassword({ service: key });
  },
  async hasItem(key) {
    const result = await Keychain.getGenericPassword({ service: key });
    return !!result;
  },
};
```

### 2. Create wallet and client

```typescript
import { createWallet, createMobileClient } from '@icn/react-native';

// Create wallet
const wallet = createWallet(secureStorage);

// Generate or load key pair
if (!(await wallet.hasKeyPair())) {
  await wallet.generateKeyPair();
}

// Create client
const client = createMobileClient({
  baseUrl: 'https://icn.mycoop.org',
  wallet,
  storage: secureStorage,
});

// Initialize (loads persisted auth)
await client.initialize();
```

### 3. Use in React components

```tsx
import React from 'react';
import { View, Text, Button, ActivityIndicator } from 'react-native';
import { useAuth, useBalance, usePayment } from '@icn/react-native';

function HomeScreen() {
  const { isAuthenticated, did, login, logout, isLoading } = useAuth(client);

  if (isLoading) {
    return <ActivityIndicator />;
  }

  if (!isAuthenticated) {
    return (
      <View>
        <Button title="Login" onPress={() => login('my-timebank')} />
      </View>
    );
  }

  return (
    <View>
      <Text>Welcome, {did}</Text>
      <BalanceCard coopId="my-timebank" did={did} />
      <Button title="Logout" onPress={logout} />
    </View>
  );
}

function BalanceCard({ coopId, did }: { coopId: string; did: string }) {
  const { balance, isLoading, refresh } = useBalance(client, coopId, did);

  if (isLoading) {
    return <ActivityIndicator />;
  }

  return (
    <View>
      <Text>Balance: {balance?.balance ?? 0} hours</Text>
      <Button title="Refresh" onPress={refresh} />
    </View>
  );
}
```

## API Reference

### Client

#### `createMobileClient(options)`

Create a new mobile ICN client.

```typescript
const client = createMobileClient({
  baseUrl: 'https://icn.mycoop.org',
  wallet: myWallet,        // Optional: for automatic signing
  storage: secureStorage,  // Optional: for persistent auth
  timeout: 30000,          // Optional: request timeout
});
```

#### `client.initialize()`

Load persisted authentication state. Call this on app startup.

#### `client.login(coopId?, scopes?)`

Authenticate using the configured wallet.

#### `client.logout()`

Clear authentication and disconnect WebSocket.

### Wallet

#### `createWallet(storage)`

Create a wallet with secure storage.

```typescript
const wallet = createWallet(secureStorage);
```

#### `wallet.generateKeyPair()`

Generate a new Ed25519 key pair.

#### `wallet.importKeyPair(privateKey)`

Import an existing private key (hex format).

#### `wallet.sign(message)`

Sign a message with the stored private key.

### QR Codes

#### `generatePaymentQR(data)`

Generate a QR code string for receiving payments.

```typescript
import { generatePaymentQR } from '@icn/react-native';

const qrData = generatePaymentQR({
  to: 'did:icn:alice',
  amount: 5,
  memo: 'Coffee',
  coopId: 'my-timebank',
});

// Use with react-native-qrcode-svg or similar
<QRCode value={qrData} />
```

#### `parsePaymentQR(qrData)`

Parse a scanned QR code into payment data.

```typescript
import { parsePaymentQR } from '@icn/react-native';

// From camera scanner
const payment = parsePaymentQR(scannedData);
if (payment) {
  await client.pay(payment.coopId, {
    to: payment.to,
    amount: payment.amount,
    memo: payment.memo,
  });
}
```

### React Hooks

#### `useAuth(client)`

Manage authentication state.

```typescript
const {
  isAuthenticated,
  did,
  coopId,
  login,
  logout,
  isLoading,
  error,
} = useAuth(client);
```

#### `useBalance(client, coopId, did)`

Fetch and auto-refresh balance.

```typescript
const { balance, isLoading, error, refresh } = useBalance(client, coopId, did);
```

#### `usePayment(client, coopId)`

Make payments.

```typescript
const { pay, isPaying, error } = usePayment(client, coopId);

await pay({ to: recipientDid, amount: 5, memo: 'Thanks!' });
```

#### `useRealtime(client, autoConnect?)`

Manage WebSocket connection.

```typescript
const { state, isConnected, connect, disconnect } = useRealtime(client);
```

#### `useEvent(client, eventType, handler)`

Subscribe to real-time events.

```typescript
useEvent(client, 'PaymentCreated', (event) => {
  showNotification(`Received payment from ${event.from}`);
});
```

#### `useProposals(client, domainId?)`

Fetch and manage governance proposals.

```typescript
const { proposals, isLoading, vote, refresh } = useProposals(client, 'coop:my-coop');
```

## Example App

See [examples/](./examples/) for a complete React Native example app.

## Security Notes

1. **Always use secure storage** - Never store private keys in AsyncStorage
2. **Use react-native-keychain** - It provides hardware-backed security on both platforms
3. **Pin certificates in production** - Consider SSL pinning for the gateway URL
4. **Validate QR codes** - Always validate scanned data before processing payments

## Supported Platforms

- iOS 13+
- Android 6.0+ (API 23+)
- Expo SDK 49+

## License

MIT OR Apache-2.0
