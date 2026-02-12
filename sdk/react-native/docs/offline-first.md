# Offline-First Architecture Guide

This guide explains the offline-first patterns in the ICN React Native SDK, focusing on the `QueueManager` for reliable operation handling.

## Overview

The ICN React Native SDK is designed for offline-first operation, ensuring that users can interact with their cooperative even when network connectivity is unreliable. Operations are queued locally and synchronized when connectivity is restored.

### Key Components

| Component | Purpose |
|-----------|---------|
| `QueueManager` | Queues and retries operations |
| `SecureStorage` | Persists queue across app restarts |
| `NetworkState` | Tracks connectivity status |

---

## QueueManager

The `QueueManager` handles queuing, persistence, and retry logic for operations that may fail due to network issues.

### Initialization

```typescript
import { QueueManager, SecureStorage } from '@icn/react-native';

// Create secure storage adapter (platform-specific)
const storage: SecureStorage = {
  setItem: (key, value) => AsyncStorage.setItem(key, value),
  getItem: (key) => AsyncStorage.getItem(key),
  removeItem: (key) => AsyncStorage.removeItem(key),
  hasItem: async (key) => (await AsyncStorage.getItem(key)) !== null,
};

// Create and initialize queue manager
const queueManager = new QueueManager(storage);
await queueManager.initialize();
```

### Enqueuing Operations

Add operations to the queue when network calls fail:

```typescript
import { QueuedOperation } from '@icn/react-native';

// Enqueue a payment that failed due to network error
const operationId = await queueManager.enqueue({
  type: 'payment',
  data: {
    coopId: 'food-coop',
    from: userDid,
    to: recipientDid,
    amount: 10,
    currency: 'hours',
    memo: 'Coffee',
  },
});

console.log(`Queued payment with ID: ${operationId}`);
```

### Supported Operation Types

| Type | Data Structure | Description |
|------|----------------|-------------|
| `payment` | `{ coopId, from, to, amount, currency, memo? }` | Mutual credit transfer |
| `vote` | `{ proposalId, choice }` | Governance vote |
| `proposal` | `{ domain_id, title, description?, kind }` | Create proposal |

> **Note**: Trust attestations are not yet available through the SDK API and cannot be queued.

### Operation Data Types

Define type-safe interfaces for operation data:

```typescript
// Type definitions for operation data
// These mirror the SDK's request types but include queue-specific fields

interface PaymentData {
  coopId: string;  // Passed separately to client.pay()
  from: string;
  to: string;
  amount: number;
  currency: string;  // Required (e.g., 'hours', 'credits')
  memo?: string;
}

interface VoteData {
  proposalId: string;  // Passed separately to client.vote()
  choice: 'yes' | 'no' | 'abstain';
}

interface ProposalData {
  domain_id: string;  // e.g., 'coop:food-coop'
  title: string;
  description?: string;
  kind: 'standard' | 'constitutional' | 'emergency';
}
```

### Processing the Queue

Process queued operations when network becomes available:

```typescript
import { ICNMobileClient } from '@icn/react-native';

// Assumes client is initialized ICNMobileClient instance
declare const client: ICNMobileClient;

// Define executor for each operation type
async function executeOperation(op: QueuedOperation): Promise<void> {
  switch (op.type) {
    case 'payment': {
      const data = op.data as PaymentData;
      await client.pay(data.coopId, {
        from: data.from,
        to: data.to,
        amount: data.amount,
        currency: data.currency,
        memo: data.memo,
      });
      break;
    }

    case 'vote': {
      const data = op.data as VoteData;
      await client.vote(data.proposalId, {
        choice: data.choice,
      });
      break;
    }

    case 'proposal': {
      const data = op.data as ProposalData;
      await client.createProposal({
        domain_id: data.domain_id,
        title: data.title,
        description: data.description,
        kind: data.kind,
      });
      break;
    }

    default:
      throw new Error(`Unknown operation type: ${op.type}`);
  }
}

// Process when online
await queueManager.processQueue(executeOperation);
```

### Monitoring Queue Status

```typescript
// Get pending count for UI badge
const pendingCount = queueManager.getPendingCount();

// Get full queue for debugging
const queue = queueManager.getQueue();

// Listen for changes
const unsubscribe = queueManager.onChange((queue) => {
  console.log(`Queue updated: ${queue.length} operations`);
  updateUI(queue);
});

// Later: stop listening
unsubscribe();
```

### Handling Failed Operations

Operations fail permanently after 3 retries (with exponential backoff):

```typescript
// Check for failed operations
const failedOps = queueManager.getQueue().filter(op => op.status === 'failed');

if (failedOps.length > 0) {
  // Show user the failed operations
  Alert.alert(
    'Sync Failed',
    `${failedOps.length} operation(s) could not be completed. Would you like to retry?`,
    [
      { text: 'Retry', onPress: () => retryFailedOperations() },
      { text: 'Clear', onPress: () => queueManager.clearFailed() },
    ]
  );
}

async function retryFailedOperations() {
  // Reset failed operations to pending using the proper API
  for (const op of failedOps) {
    await queueManager.updateStatus(op.id, 'pending');
  }
  await queueManager.processQueue(executeOperation);
}
```

---

## Sync Strategy

### Automatic Sync on Network Change

```typescript
import NetInfo from '@react-native-community/netinfo';

// Listen for network changes
NetInfo.addEventListener(state => {
  if (state.isConnected && state.isInternetReachable) {
    // Network restored - sync queue
    queueManager.processQueue(executeOperation);
  }
});
```

### Manual Sync Trigger

```typescript
// Pull-to-refresh handler
async function onRefresh() {
  setRefreshing(true);
  try {
    await queueManager.processQueue(executeOperation);
    await refreshData();
  } finally {
    setRefreshing(false);
  }
}
```

### Background Sync

For operations that must sync even when app is backgrounded:

```typescript
import BackgroundFetch from 'react-native-background-fetch';

BackgroundFetch.configure({
  minimumFetchInterval: 15, // 15 minutes
}, async (taskId) => {
  // Sync queued operations
  await queueManager.processQueue(executeOperation);
  BackgroundFetch.finish(taskId);
});
```

---

## Conflict Resolution

When syncing queued operations, conflicts can occur if the server state has changed.

### Payment Conflicts

| Conflict | Resolution |
|----------|------------|
| Insufficient balance | Fail operation, notify user |
| Recipient not found | Fail operation, notify user |
| Duplicate transaction | Skip (idempotent) |
| Credit limit exceeded | Fail operation, notify user |

### Vote Conflicts

| Conflict | Resolution |
|----------|------------|
| Proposal expired | Fail operation, notify user |
| Already voted | Skip (idempotent) |
| Voting period not started | Retry later |

### Handling Conflicts

```typescript
// Check if an error represents a conflict (vs network/server issue)
function isConflictError(error: Error): boolean {
  return error.message.includes('insufficient balance') ||
         error.message.includes('already voted') ||
         error.message.includes('not found') ||
         error.message.includes('expired') ||
         error.message.includes('credit limit');
}

// Check if a conflict can be resolved by retrying
function isRetryableConflict(error: Error): boolean {
  // Retryable: network issues, temporary server errors
  return error.message.includes('network') ||
         error.message.includes('timeout') ||
         error.message.includes('503');
}

// Get user-friendly error message
function getConflictMessage(error: Error): string {
  if (error.message.includes('insufficient balance')) {
    return 'Not enough balance to complete this payment';
  }
  if (error.message.includes('already voted')) {
    return 'You have already voted on this proposal';
  }
  if (error.message.includes('expired')) {
    return 'The voting period has ended';
  }
  return 'This operation could not be completed';
}

// Enhanced executor with conflict handling
async function executeOperationWithConflictHandling(op: QueuedOperation): Promise<void> {
  try {
    await executeOperation(op);
  } catch (error) {
    if (isConflictError(error as Error)) {
      if (isRetryableConflict(error as Error)) {
        throw error; // Will retry with backoff
      } else {
        // Non-recoverable conflict - mark as failed
        // The error will be caught by processQueue and status updated to 'failed'
        throw new Error(getConflictMessage(error as Error));
      }
    }
    throw error;
  }
}
```

---

## Best Practices

### 1. Show Queue Status in UI

Always show users when operations are queued:

```tsx
function SyncIndicator() {
  const [pending, setPending] = useState(0);

  useEffect(() => {
    return queueManager.onChange(queue => {
      setPending(queue.filter(op => op.status === 'pending').length);
    });
  }, []);

  if (pending === 0) return null;

  return (
    <View style={styles.indicator}>
      <ActivityIndicator size="small" />
      <Text>{pending} pending</Text>
    </View>
  );
}
```

### 2. Optimistic UI Updates

Update UI immediately, then sync in background:

```typescript
async function sendPayment(to: string, amount: number, currency: string) {
  // Optimistically update local balance
  setBalance(prev => prev - amount);

  try {
    await client.pay(coopId, { from: userDid, to, amount, currency });
  } catch (error) {
    if (isNetworkError(error)) {
      // Queue for later - UI already updated
      await queueManager.enqueue({
        type: 'payment',
        data: { coopId, from: userDid, to, amount, currency },
      });
    } else {
      // Revert optimistic update
      setBalance(prev => prev + amount);
      throw error;
    }
  }
}
```

### 3. Validate Before Queuing

Check constraints locally before queuing:

```typescript
async function enqueuePayment(to: string, amount: number, currency: string) {
  // Local validation
  if (amount <= 0) {
    throw new Error('Amount must be positive');
  }
  if (balance - amount < -creditLimit) {
    throw new Error('Would exceed credit limit');
  }

  // Queue if validation passes
  await queueManager.enqueue({
    type: 'payment',
    data: { coopId, from: userDid, to, amount, currency },
  });
}
```

### 4. Handle App Restart

Queue persists across restarts - process on app launch:

```typescript
async function initializeApp() {
  // Initialize queue manager
  await queueManager.initialize();

  // Check network and sync if online
  const netInfo = await NetInfo.fetch();
  if (netInfo.isConnected && netInfo.isInternetReachable) {
    await queueManager.processQueue(executeOperation);
  }
}
```

### 5. Clear Queue on Logout

Remove queued operations when user logs out:

```typescript
async function logout() {
  // Clear queue to prevent leaking operations to new user
  await queueManager.clear();
  await client.logout();
}
```

---

## Example: Complete Offline Payment Flow

The following example demonstrates a complete offline-capable payment flow.

```typescript
import { useState, useEffect } from 'react';
import { QueueManager, ICNMobileClient, QueuedOperation, isNetworkError } from '@icn/react-native';
import NetInfo from '@react-native-community/netinfo';

// Helper functions (implement based on your UI framework)
// function showSuccess(msg: string): void { ... }
// function showInfo(msg: string): void { ... }
// function showError(msg: string): void { ... }

// Assumes these are initialized elsewhere in your app
declare const client: ICNMobileClient;
declare const queueManager: QueueManager;
declare const coopId: string;
declare const userDid: string;

function PaymentScreen() {
  const [isOnline, setIsOnline] = useState(true);
  const [pendingCount, setPendingCount] = useState(0);

  useEffect(() => {
    // Track network state
    const unsubNet = NetInfo.addEventListener(state => {
      setIsOnline(state.isConnected && state.isInternetReachable);
    });

    // Track queue state
    const unsubQueue = queueManager.onChange(queue => {
      setPendingCount(queue.filter(op => op.status === 'pending').length);
    });

    return () => {
      unsubNet();
      unsubQueue();
    };
  }, []);

  async function handlePayment(to: string, amount: number, currency: string, memo: string) {
    try {
      if (isOnline) {
        // Try direct payment
        await client.pay(coopId, { from: userDid, to, amount, currency, memo });
        showSuccess('Payment sent!');
      } else {
        // Queue for later
        await queueManager.enqueue({
          type: 'payment',
          data: { coopId, from: userDid, to, amount, currency, memo },
        });
        showInfo('Payment queued - will sync when online');
      }
    } catch (error) {
      if (isNetworkError(error)) {
        // Network failed during request - queue it
        await queueManager.enqueue({
          type: 'payment',
          data: { coopId, from: userDid, to, amount, currency, memo },
        });
        showInfo('Payment queued - will sync when online');
      } else {
        showError((error as Error).message);
      }
    }
  }

  return (
    <View>
      {!isOnline && <OfflineBanner />}
      {pendingCount > 0 && <SyncIndicator count={pendingCount} />}
      <PaymentForm onSubmit={handlePayment} />
    </View>
  );
}
```

---

## API Reference

### QueueManager

| Method | Returns | Description |
|--------|---------|-------------|
| `initialize()` | `Promise<void>` | Load persisted queue |
| `enqueue(op)` | `Promise<string>` | Add operation, returns ID |
| `getQueue()` | `QueuedOperation[]` | Get all operations |
| `getPendingCount()` | `number` | Count pending operations |
| `remove(id)` | `Promise<void>` | Remove specific operation |
| `updateStatus(id, status, error?)` | `Promise<void>` | Update operation status |
| `processQueue(executor)` | `Promise<void>` | Process pending operations |
| `clearFailed()` | `Promise<void>` | Remove failed operations |
| `clear()` | `Promise<void>` | Remove all operations |
| `onChange(listener)` | `() => void` | Subscribe to changes |

### QueuedOperation

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique identifier |
| `type` | `'payment' \| 'vote' \| 'proposal'` | Operation type |
| `data` | `unknown` | Type-specific data |
| `queuedAt` | `number` | Timestamp (ms) |
| `retries` | `number` | Retry count (max 3) |
| `status` | `'pending' \| 'processing' \| 'failed'` | Current status |
| `error?` | `string` | Error message if failed |

> **Note**: Successfully processed operations are removed from the queue rather than being marked as `'completed'`.

---

## Related Documentation

- [Getting Started](../README.md) - SDK setup guide
- [Client API](../src/client.ts) - Full client API reference
- [Types](../src/types.ts) - TypeScript type definitions
