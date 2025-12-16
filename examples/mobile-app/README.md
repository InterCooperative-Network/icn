# ICN Mobile App Examples

Example React Native components demonstrating ICN pilot features.

## Examples

- `NotificationCenter.tsx` - In-app notification center with real-time updates
- `RecurringPaymentSetup.tsx` - Create and manage recurring payments
- `VotingScreen.tsx` - Amendment voting interface
- `BudgetManager.tsx` - Budget creation and monitoring

## Usage

These are reference implementations. Copy and adapt for your app:

```tsx
import { NotificationCenter } from './examples/mobile-app/NotificationCenter';

function App() {
  return <NotificationCenter />;
}
```

## Prerequisites

```bash
npm install @icn/react-native
npm install @react-native-firebase/app
npm install @react-native-firebase/messaging
```

## Features Demonstrated

### Notifications
- Real-time WebSocket connection
- In-app notification list
- Push notification registration
- Read/unread state management

### Recurring Payments
- Payment frequency selection
- Start/end date pickers
- Payment preview
- Edit and cancel flows

### Governance
- Amendment voting UI
- Vote results visualization
- User's vote status
- Quorum indicators

### Budgets
- Spending visualization
- Threshold alerts
- Period selection
- Multi-account support
