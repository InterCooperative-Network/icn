# @icn/react-native

React Native SDK for the InterCooperative Network - mobile-first mutual credit and cooperative coordination.

## Features

- **Wallet Management** - Secure key storage with iOS Keychain / Android Keystore
- **Authentication** - Persistent login with automatic token refresh
- **Real-time Events** - WebSocket with auto-reconnect
- **QR Payments** - Generate and scan payment QR codes
- **React Hooks** - Easy integration with React Native apps
- **SDIS Steward** - Review and vouch for identity enrollments
- **Trust Graph** - View and create trust attestations

## Installation

```bash
npm install @icn/react-native @icn/client

# Required: crypto polyfill for React Native
npm install react-native-get-random-values

# For secure storage (choose one):
npm install react-native-keychain
# or
expo install expo-secure-store
```

**Important:** Import the crypto polyfill at the very top of your app entry point (before any other imports):

```typescript
// App.tsx - MUST be the first import
import 'react-native-get-random-values';

// Then other imports...
import React from 'react';
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

### Offline Support

The SDK automatically queues operations when offline and syncs when back online.

#### Network State

```typescript
// Subscribe to network changes
client.onNetworkStateChange((state) => {
  console.log('Network:', state); // 'online' | 'offline' | 'slow'
});

// Check current state
console.log(client.networkState);
```

#### Operation Queue

```typescript
// Operations are automatically queued when offline
await client.pay(coopId, paymentRequest); // Queued if offline

// Monitor queue
client.onQueueChange((queue) => {
  console.log(`${queue.length} operations pending`);
});

// Process queue manually
await client.processQueue();

// Clear failed operations
await client.clearFailedOperations();
```

### Trust Graph

#### Get Trust Score

```typescript
const trust = await client.getTrustScore('did:icn:alice');
console.log(trust.trust_score);  // 0.0 - 1.0
console.log(trust.trust_class);  // 'Isolated' | 'Known' | 'Partner' | 'Federated'
```

#### Create Trust Attestation

```typescript
await client.createTrustAttestation(
  'did:icn:alice',  // Target DID
  0.8,              // Score (0.0 - 1.0)
  'Great collaborator'  // Optional memo
);
```

#### Visualize Trust Network

```typescript
const network = await client.getTrustNetwork('did:icn:alice', 2);
// network.nodes - Array of DIDs with trust scores
// network.edges - Array of trust relationships
```

### SDIS Steward Hooks

Hooks for stewards to review and vouch for SDIS enrollments.

#### `usePendingEnrollments(client, options?)`

Fetch pending enrollments for steward review.

```typescript
import { usePendingEnrollments } from '@icn/react-native';

const {
  enrollments,
  pendingCount,
  isLoading,
  error,
  refresh,
} = usePendingEnrollments(client, { autoRefresh: true, refreshInterval: 30000 });
```

#### `useVouch(client)`

Submit a steward vouch for an enrollment.

```typescript
import { useVouch } from '@icn/react-native';

const { vouch, isSubmitting, error, success, reset } = useVouch(client);

await vouch(enrollmentId, 'I verified this person in a video call');
```

#### `useReject(client)`

Reject an enrollment.

```typescript
import { useReject } from '@icn/react-native';

const { reject, isSubmitting, error, success, reset } = useReject(client);

await reject(enrollmentId, 'Could not verify identity - suspicious behavior');
```

#### `useStewardStats(client)`

Fetch steward statistics.

```typescript
import { useStewardStats } from '@icn/react-native';

const { stats, isLoading, error, refresh } = useStewardStats(client);

// stats.total_vouches - Total vouches submitted
// stats.monthly_vouches - Vouches this month
// stats.total_rejections - Rejection count
// stats.reputation_score - Steward reputation (0-100)
// stats.avg_response_hours - Average response time
```

#### `useVouchHistory(client, limit?)`

Fetch steward vouch history.

```typescript
import { useVouchHistory } from '@icn/react-native';

const { history, total, isLoading, error, refresh } = useVouchHistory(client, 50);
```

#### `useEnrollmentDetail(client, enrollmentId)`

Fetch details for a specific enrollment.

```typescript
import { useEnrollmentDetail } from '@icn/react-native';

const { enrollment, isLoading, error, refresh } = useEnrollmentDetail(client, enrollmentId);
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
- Expo SDK 54+
- React Native 0.76+ (New Architecture supported)

## License

MIT OR Apache-2.0

## Pilot Features (v0.9.0+)

### Real-Time Notifications

React hooks for notification management:

```tsx
import { useNotifications, useNotificationCount } from '@icn/react-native';

function NotificationsScreen() {
  const { notifications, loading, markAsRead, deleteNotification } = useNotifications({
    read: false, // unread only
    limit: 20
  });

  const { total, unread } = useNotificationCount();

  return (
    <View>
      <Text>Unread: {unread} / {total}</Text>
      {notifications.map(notif => (
        <NotificationCard
          key={notif.id}
          notification={notif}
          onRead={() => markAsRead(notif.id)}
          onDelete={() => deleteNotification(notif.id)}
        />
      ))}
    </View>
  );
}
```

### Recurring Payments

Manage subscription-style payments:

```tsx
import { useRecurringPayments, useCreateRecurringPayment } from '@icn/react-native';

function RecurringPaymentsScreen() {
  const { payments, loading, updatePayment, cancelPayment } = useRecurringPayments({
    status: 'active'
  });

  const { create, creating } = useCreateRecurringPayment();

  const handleCreate = async () => {
    await create({
      from_account: 'alice-checking',
      to_account: 'netflix',
      amount: 1599, // $15.99
      currency: 'USD',
      frequency: 'monthly',
      description: 'Netflix subscription'
    });
  };

  return (
    <ScrollView>
      {payments.map(payment => (
        <PaymentCard
          key={payment.id}
          payment={payment}
          onPause={() => updatePayment(payment.id, { status: 'paused' })}
          onCancel={() => cancelPayment(payment.id)}
        />
      ))}
      <Button title="Add Payment" onPress={handleCreate} disabled={creating} />
    </ScrollView>
  );
}
```

### Payment Escrow

Conditional fund holding:

```tsx
import { useEscrows, useCreateEscrow } from '@icn/react-native';

function EscrowScreen() {
  const { escrows, loading, releaseEscrow, refundEscrow } = useEscrows();
  const { create } = useCreateEscrow();

  const handleCreateEscrow = async () => {
    await create({
      from_account: 'buyer-account',
      to_account: 'seller-account',
      amount: 50000,
      currency: 'USD',
      description: 'House deposit',
      conditions: [
        { requires_approval: { did: 'did:icn:escrow-agent' } }
      ],
      expires_at: Date.now() / 1000 + (30 * 24 * 60 * 60) // 30 days
    });
  };

  return (
    <View>
      {escrows.map(escrow => (
        <EscrowCard
          key={escrow.id}
          escrow={escrow}
          onRelease={() => releaseEscrow(escrow.id)}
          onRefund={() => refundEscrow(escrow.id)}
        />
      ))}
    </View>
  );
}
```

### Budget Management

Spending limits with visual indicators:

```tsx
import { useBudgets, useCreateBudget } from '@icn/react-native';

function BudgetScreen() {
  const { budgets, loading, updateBudget, deleteBudget } = useBudgets();
  const { create } = useCreateBudget();

  return (
    <View>
      {budgets.map(budget => (
        <BudgetCard
          key={budget.id}
          budget={budget}
          percentageUsed={budget.percentage_used}
          remaining={budget.remaining}
          isExceeded={budget.is_exceeded}
          onUpdate={(updates) => updateBudget(budget.id, updates)}
          onDelete={() => deleteBudget(budget.id)}
        />
      ))}
    </View>
  );
}
```

### Governance UI

Enhanced governance features:

```tsx
import {
  useCharterSummary,
  useAmendmentVoting,
  useGovernanceDashboard
} from '@icn/react-native';

function GovernanceScreen({ charterId }) {
  const { summary, founders, timeline } = useCharterSummary(charterId);
  const { dashboard } = useGovernanceDashboard(charterId);

  return (
    <ScrollView>
      <CharterSummary data={summary} founders={founders} />
      <GovernanceStats
        pendingAmendments={dashboard.pending_amendments}
        openAppeals={dashboard.open_appeals}
      />
      <Timeline events={timeline} />
    </ScrollView>
  );
}

function AmendmentVoteScreen({ amendmentId }) {
  const { vote, loading } = useAmendmentVoting(amendmentId);

  return (
    <View>
      <Button
        title="Approve"
        onPress={() => vote('approve', 'I support this change')}
        disabled={loading}
      />
      <Button
        title="Reject"
        onPress={() => vote('reject', 'I have concerns')}
        disabled={loading}
      />
      <Button
        title="Abstain"
        onPress={() => vote('abstain')}
        disabled={loading}
      />
    </View>
  );
}
```

## Push Notifications (FCM)

Configure push notifications for mobile apps:

```tsx
import messaging from '@react-native-firebase/messaging';
import { registerDeviceForNotifications } from '@icn/react-native';

// Request permission and register device
async function setupPushNotifications() {
  const authStatus = await messaging().requestPermission();
  const enabled =
    authStatus === messaging.AuthorizationStatus.AUTHORIZED ||
    authStatus === messaging.AuthorizationStatus.PROVISIONAL;

  if (enabled) {
    const fcmToken = await messaging().getToken();
    
    // Register with ICN gateway
    await registerDeviceForNotifications({
      fcm_token: fcmToken,
      device_id: DeviceInfo.getUniqueId(),
      platform: Platform.OS,
    });
  }
}

// Handle foreground notifications
messaging().onMessage(async remoteMessage => {
  console.log('Notification received:', remoteMessage);
  // Show in-app notification
});

// Handle background notifications
messaging().setBackgroundMessageHandler(async remoteMessage => {
  console.log('Background notification:', remoteMessage);
});
```

## TypeScript Support

All hooks and components are fully typed:

```tsx
import type {
  RecurringPayment,
  Escrow,
  Budget,
  Notification,
} from '@icn/react-native';

const payment: RecurringPayment = await createRecurringPayment(data);
```

## License

MIT
