# Module 8: Web UI Integration

## Learning Objectives

By the end of this module, you will:
- Understand the thin-client architecture pattern
- Trace user actions through the UI to gateway API calls
- Implement session management with JWT tokens
- Handle real-time updates via WebSocket
- Design responsive, accessible interfaces for cooperatives
- Debug common UI integration issues

## Prerequisites

- Module 7 (Gateway API and SDK)
- Basic JavaScript and DOM manipulation
- Understanding of REST APIs and WebSockets

## Key Reading

- `web/pilot-ui/README.md`
- `web/pilot-ui/app.js`
- `web/pilot-ui/index.html`
- `web/pilot-ui/style.css`

---

## Concepts (Textbook Style)

### The Thin-Client Architecture

The Pilot UI follows a **thin-client** architecture where the UI contains no ICN business logic.
Instead, it serves purely as a presentation layer that renders data from the gateway.

```
┌──────────────────────────────────────────────────────────────────┐
│                           Thin Client                            │
│                                                                  │
│  ┌─────────────┐    ┌─────────────┐    ┌──────────────────────┐ │
│  │  index.html │    │  style.css  │    │       app.js         │ │
│  │             │    │             │    │                      │ │
│  │  Structure  │    │  Styling    │    │  - API calls         │ │
│  │  Layout     │    │  Responsive │    │  - State management  │ │
│  │  Semantics  │    │  Themes     │    │  - Event handlers    │ │
│  └─────────────┘    └─────────────┘    │  - DOM updates       │ │
│                                        │  - WebSocket client  │ │
│                                        └──────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
                            │
                            │ HTTP/JSON + WebSocket
                            ▼
┌──────────────────────────────────────────────────────────────────┐
│                         Gateway API                              │
│                                                                  │
│    Auth │ Ledger │ Governance │ Members │ Real-time Events      │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

**Why thin-client?**

1. **Single Source of Truth**: The ICN daemon is authoritative. The UI never computes
   balances, validates transactions, or enforces rules - it displays what the gateway tells it.

2. **Simplicity**: No build pipeline, no framework. Just HTML, CSS, and vanilla JavaScript
   that can be served by any static file server.

3. **Flexibility**: Different cooperatives can customize the UI without understanding ICN internals.
   They only need to know the API contract.

4. **Security**: Business logic in the UI can be bypassed. Keeping it server-side ensures
   consistent enforcement.

### Application State

The UI maintains minimal client-side state:

```javascript
const state = {
    // Connection state
    gatewayUrl: '',      // Gateway API base URL
    coopId: '',          // Current cooperative
    did: '',             // Authenticated user's DID
    token: '',           // JWT token
    tokenExpiry: null,   // When token expires (for warnings)

    // User context
    userRole: null,      // 'owner', 'admin', or 'member'

    // Data caches (refreshed from API)
    members: [],         // Member list
    transactions: [],    // Transaction history
    proposals: [],       // Governance proposals

    // WebSocket state
    ws: null,            // WebSocket connection
    wsConnected: false,  // Connection status
};
```

**State management principles**:

1. **Server is authoritative**: Cached data is refreshed from the API; never modified locally
   except through API calls

2. **Minimal state**: Only store what's needed for rendering; don't duplicate gateway state

3. **Persistence**: Session data persists to `localStorage` for reload recovery

### UI Data Flow

Every user action follows the same pattern:

```
┌──────────┐     ┌──────────┐     ┌─────────┐     ┌──────────┐
│  User    │────▶│  Event   │────▶│  API    │────▶│  Update  │
│  Action  │     │ Handler  │     │  Call   │     │   DOM    │
└──────────┘     └──────────┘     └─────────┘     └──────────┘
     │                                                  │
     │                                                  │
     ▼                                                  ▼
┌──────────────────────────────────────────────────────────────┐
│                    Gateway API                               │
└──────────────────────────────────────────────────────────────┘
```

**Example: Logging Hours**

```javascript
// 1. User clicks "Log Hours" button
elements.logHoursForm.addEventListener('submit', async (e) => {
    e.preventDefault();

    // 2. Event handler collects form data
    const paymentData = {
        from: state.did,
        to: elements.recipient.value,
        amount: parseFloat(elements.hours.value),
        currency: 'hours',
        memo: elements.memo.value,
    };

    // 3. API call to gateway
    try {
        const payment = await apiRequest(
            'POST',
            `/ledger/${state.coopId}/payment`,
            paymentData
        );

        // 4. Update DOM on success
        showToast('Hours logged successfully!', 'success');
        elements.logHoursForm.reset();

        // Data refresh happens via WebSocket event
    } catch (error) {
        showToast(getUserFriendlyError(error), 'error');
    }
});
```

---

## Authentication and Session Management

### Login Flow

The UI authenticates by validating a pre-obtained JWT token:

```
┌────────────────────────────────────────────────────────────────┐
│                        Login Flow                              │
│                                                                │
│  1. User enters:         2. Validate:          3. Persist:    │
│     - Gateway URL           - /health             - localStorage │
│     - Coop ID               - /balance            - state      │
│     - DID                                                       │
│     - JWT Token                                                 │
└────────────────────────────────────────────────────────────────┘
```

```javascript
async function login() {
    // 1. Collect and validate inputs
    state.gatewayUrl = elements.gatewayUrl.value.trim().replace(/\/$/, '');
    state.coopId = elements.coopId.value.trim();
    state.did = elements.did.value.trim();
    state.token = elements.token.value.trim();

    if (!state.gatewayUrl || !state.coopId || !state.did || !state.token) {
        showError(elements.loginError, 'Please fill in all fields');
        return;
    }

    try {
        // 2. Validate connectivity and authentication
        await apiRequest('GET', '/health');  // Gateway reachable?
        await apiRequest('GET', `/ledger/${state.coopId}/balance/${encodeURIComponent(state.did)}`);  // Token valid?

        // 3. Persist session
        state.tokenExpiry = Date.now() + (24 * 60 * 60 * 1000);  // Assume 24h
        localStorage.setItem('icn-gateway', state.gatewayUrl);
        localStorage.setItem('icn-coop', state.coopId);
        localStorage.setItem('icn-did', state.did);
        localStorage.setItem('icn-token', state.token);
        localStorage.setItem('icn-token-expiry', state.tokenExpiry.toString());

        // 4. Load data and connect WebSocket
        showScreen('main');
        await loadAllData();
        connectWebSocket();

    } catch (error) {
        showError(elements.loginError, getUserFriendlyError(error));
    }
}
```

### Token Expiration Handling

The UI tracks token expiration and warns users:

```javascript
// Check token expiry every minute
setInterval(checkTokenExpiry, 60000);

function checkTokenExpiry() {
    if (!state.tokenExpiry) return;

    const remaining = state.tokenExpiry - Date.now();
    const minutes = Math.floor(remaining / 60000);

    // Update visual indicator
    updateTokenBadge(minutes);

    // Warn at key thresholds
    if (minutes === 15) {
        showToast('Token expires in 15 minutes', 'warning');
    } else if (minutes === 5) {
        showToast('Token expires in 5 minutes! Get a new token soon.', 'warning');
    } else if (minutes <= 0) {
        showToast('Token expired! Please sign in again.', 'error');
        logout();
    }
}

function updateTokenBadge(minutes) {
    const badge = elements.tokenExpires;

    if (minutes > 60) {
        badge.className = 'token-badge green';
        badge.textContent = `${Math.floor(minutes / 60)}h remaining`;
    } else if (minutes > 15) {
        badge.className = 'token-badge yellow';
        badge.textContent = `${minutes}m remaining`;
    } else {
        badge.className = 'token-badge red';
        badge.textContent = `${minutes}m remaining`;
    }
}
```

### Session Restoration

On page load, attempt to restore the previous session:

```javascript
function restoreSession() {
    const gateway = localStorage.getItem('icn-gateway');
    const coopId = localStorage.getItem('icn-coop');
    const did = localStorage.getItem('icn-did');
    const token = localStorage.getItem('icn-token');
    const expiry = localStorage.getItem('icn-token-expiry');

    if (gateway && coopId && did && token && expiry) {
        const expiryTime = parseInt(expiry, 10);

        // Check if token is still valid
        if (expiryTime > Date.now()) {
            state.gatewayUrl = gateway;
            state.coopId = coopId;
            state.did = did;
            state.token = token;
            state.tokenExpiry = expiryTime;

            // Pre-fill form and auto-login
            elements.gatewayUrl.value = gateway;
            elements.coopId.value = coopId;
            elements.did.value = did;
            elements.token.value = token;

            login();  // Auto-reconnect
            return true;
        }
    }

    return false;  // Show login screen
}
```

---

## API Integration

### The apiRequest Function

All API calls go through a single function:

```javascript
async function apiRequest(method, path, body = null) {
    const url = `${state.gatewayUrl}/v1${path}`;

    const options = {
        method,
        headers: {
            'Content-Type': 'application/json',
        },
    };

    // Add auth token for protected endpoints
    if (state.token) {
        options.headers['Authorization'] = `Bearer ${state.token}`;
    }

    if (body) {
        options.body = JSON.stringify(body);
    }

    const response = await fetch(url, options);

    if (!response.ok) {
        // Parse error response
        const error = await response.json().catch(() => ({}));
        throw new Error(error.error || `HTTP ${response.status}`);
    }

    // Return parsed JSON (or null for 204 No Content)
    if (response.status === 204) {
        return null;
    }

    return response.json();
}
```

### User-Friendly Error Messages

Technical errors are translated to user-friendly messages:

```javascript
function getUserFriendlyError(error) {
    const message = error.message || String(error);

    // Network errors
    if (message.includes('Failed to fetch')) {
        return 'Cannot connect to the server. Please check your internet connection.';
    }

    // HTTP status errors
    const errorMap = {
        '401': 'Your session has expired. Please sign in again.',
        '403': "You don't have permission to do that.",
        '404': 'The requested resource was not found.',
        '429': 'Too many requests. Please wait a moment.',
        '500': 'Server error. Please try again later.',
    };

    for (const [code, friendly] of Object.entries(errorMap)) {
        if (message.includes(code)) {
            return friendly;
        }
    }

    // Specific error patterns
    if (message.includes('INSUFFICIENT_BALANCE')) {
        return 'Insufficient balance for this transaction.';
    }

    if (message.includes('MEMBER_NOT_FOUND')) {
        return 'Member not found. Please check the recipient.';
    }

    return message;  // Fall back to raw message
}
```

---

## WebSocket Integration

### Connection Lifecycle

```
┌─────────────────────────────────────────────────────────────────┐
│                   WebSocket Lifecycle                           │
│                                                                 │
│  Connect ──▶ Authenticate ──▶ Receive Events ──▶ Handle Events │
│      │                              │                    │      │
│      │                              ▼                    ▼      │
│      │                         ┌────────┐          ┌────────┐  │
│      │                         │ Event  │          │ Update │  │
│      │                         │ Queue  │──────────│  UI    │  │
│      │                         └────────┘          └────────┘  │
│      │                                                         │
│      │  On Disconnect: Auto-reconnect with exponential backoff │
└─────────────────────────────────────────────────────────────────┘
```

```javascript
function connectWebSocket() {
    if (state.ws) {
        state.ws.close();
    }

    // Convert HTTP URL to WebSocket URL
    const wsUrl = state.gatewayUrl
        .replace('http://', 'ws://')
        .replace('https://', 'wss://');

    const ws = new WebSocket(`${wsUrl}/v1/ws/${state.coopId}`);

    ws.onopen = () => {
        console.log('WebSocket connected');
        state.wsConnected = true;
        updateConnectionStatus(true);

        // Authenticate with token
        ws.send(JSON.stringify({
            type: 'Auth',
            token: state.token,
        }));
    };

    ws.onmessage = (event) => {
        try {
            const message = JSON.parse(event.data);
            handleWebSocketMessage(message);
        } catch (e) {
            console.error('Failed to parse WebSocket message:', e);
        }
    };

    ws.onerror = (error) => {
        console.error('WebSocket error:', error);
    };

    ws.onclose = () => {
        console.log('WebSocket closed');
        state.wsConnected = false;
        updateConnectionStatus(false);

        // Auto-reconnect after delay
        setTimeout(() => {
            if (state.token) {  // Only if still logged in
                connectWebSocket();
            }
        }, 5000);
    };

    state.ws = ws;
}
```

### Message Handling

```javascript
function handleWebSocketMessage(message) {
    switch (message.type) {
        case 'AuthOk':
            console.log('WebSocket authenticated:', message.did);
            // Can request backfill if needed
            break;

        case 'Error':
            console.error('WebSocket error:', message.message);
            showToast(message.message, 'error');
            break;

        case 'Event':
            handleWebSocketEvent(message);
            break;

        case 'Shutdown':
            showToast(`Server shutting down: ${message.reason}`, 'warning');
            break;
    }
}

function handleWebSocketEvent(message) {
    const event = message.event;

    switch (event.type) {
        case 'PaymentCreated':
            // Refresh balance and transactions
            loadBalance();
            loadTransactions();

            // Show notification
            if (event.to === state.did) {
                showToast(`Received ${event.amount} hours from ${shortenDid(event.from)}`, 'success');
            }
            break;

        case 'MemberAdded':
            loadMembers();
            showToast(`New member joined: ${shortenDid(event.did)}`, 'info');
            break;

        case 'MemberRemoved':
            loadMembers();
            break;

        case 'GovernanceProposalCreated':
            loadProposals();
            showToast(`New proposal: ${event.title}`, 'info');
            break;

        case 'GovernanceVoteCast':
            // Refresh vote counts
            loadProposals();
            break;
    }
}
```

### Connection Status Indicator

```javascript
function updateConnectionStatus(connected) {
    const indicator = elements.connectionStatus;

    if (connected) {
        indicator.className = 'status-indicator connected';
        indicator.title = 'Connected to real-time updates';
    } else {
        indicator.className = 'status-indicator disconnected';
        indicator.title = 'Disconnected - reconnecting...';
    }
}
```

---

## UI Patterns

### Toast Notifications

Non-blocking notifications for user feedback:

```javascript
function showToast(message, type = 'info', duration = 5000) {
    const toast = document.createElement('div');
    toast.className = `toast ${type}`;

    const icons = {
        success: '✓',
        error: '✕',
        warning: '⚠',
        info: 'ℹ'
    };

    // Build toast content using DOM methods for safety
    const icon = document.createElement('span');
    icon.className = 'toast-icon';
    icon.textContent = icons[type];

    const msg = document.createElement('span');
    msg.className = 'toast-message';
    msg.textContent = message;

    const closeBtn = document.createElement('button');
    closeBtn.className = 'toast-close';
    closeBtn.textContent = '×';
    closeBtn.onclick = () => toast.remove();

    toast.appendChild(icon);
    toast.appendChild(msg);
    toast.appendChild(closeBtn);

    elements.toastContainer.appendChild(toast);

    // Auto-remove
    if (duration > 0) {
        setTimeout(() => toast.remove(), duration);
    }
}
```

**CSS for toasts**:

```css
.toast-container {
    position: fixed;
    top: 20px;
    right: 20px;
    z-index: 1000;
}

.toast {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    margin-bottom: 10px;
    background: white;
    border-radius: 8px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    animation: slideIn 0.3s ease;
}

.toast.success { border-left: 4px solid #22c55e; }
.toast.error { border-left: 4px solid #ef4444; }
.toast.warning { border-left: 4px solid #f59e0b; }
.toast.info { border-left: 4px solid #3b82f6; }

@keyframes slideIn {
    from { transform: translateX(100%); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
}
```

### Tab Navigation

```javascript
function setupNavigation() {
    elements.navBtns.forEach(btn => {
        btn.addEventListener('click', () => {
            const tabId = btn.dataset.tab;
            switchTab(tabId);
        });
    });
}

function switchTab(tabId) {
    // Update button states
    elements.navBtns.forEach(btn => {
        btn.classList.toggle('active', btn.dataset.tab === tabId);
    });

    // Show selected tab content
    elements.tabContents.forEach(content => {
        content.classList.toggle('active', content.id === `${tabId}-tab`);
    });

    // Refresh data for tab if needed
    switch (tabId) {
        case 'dashboard':
            loadBalance();
            break;
        case 'history':
            loadTransactions();
            break;
        case 'members':
            loadMembers();
            break;
        case 'governance':
            loadProposals();
            break;
    }
}
```

### Keyboard Shortcuts

```javascript
function setupKeyboardShortcuts() {
    document.addEventListener('keydown', (e) => {
        // Ctrl/Cmd + number for tab switching
        if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey) {
            const tabMap = {
                '1': 'dashboard',
                '2': 'log-hours',
                '3': 'history',
                '4': 'members',
                '5': 'governance',
            };

            if (tabMap[e.key]) {
                e.preventDefault();
                switchTab(tabMap[e.key]);
            }
        }
    });
}
```

---

## Responsive Design

### Mobile-First Approach

The UI adapts to different screen sizes:

```css
/* Base styles (mobile) */
.card {
    padding: 16px;
    margin-bottom: 16px;
}

.nav {
    display: flex;
    flex-direction: column;
}

/* Tablet and up */
@media (min-width: 768px) {
    .nav {
        flex-direction: row;
    }

    .dashboard-grid {
        display: grid;
        grid-template-columns: repeat(2, 1fr);
        gap: 20px;
    }
}

/* Desktop */
@media (min-width: 1024px) {
    .dashboard-grid {
        grid-template-columns: repeat(3, 1fr);
    }
}
```

### Touch-Friendly Targets

```css
/* Minimum 44px touch target */
.btn, .nav-btn {
    min-height: 44px;
    min-width: 44px;
    padding: 12px 24px;
}

/* Prevent iOS zoom on input focus */
input, select, textarea {
    font-size: 16px;  /* iOS zooms if < 16px */
}
```

---

## Feature Implementation Examples

### Balance Display with Trend

```javascript
async function loadBalance() {
    try {
        const response = await apiRequest(
            'GET',
            `/ledger/${state.coopId}/balance/${encodeURIComponent(state.did)}`
        );

        const balance = response.balance;
        const previousBalance = state.previousBalance || balance;

        // Update display
        elements.myBalance.textContent = formatBalance(balance);
        elements.myBalance.className = balance >= 0 ? 'positive' : 'negative';

        // Show trend indicator
        const trend = balance > previousBalance ? '↑' :
                      balance < previousBalance ? '↓' : '→';
        elements.balanceTrend.textContent = trend;

        state.previousBalance = balance;

    } catch (error) {
        showToast(getUserFriendlyError(error), 'error');
    }
}

function formatBalance(balance) {
    const sign = balance >= 0 ? '+' : '';
    return `${sign}${balance.toFixed(1)} hours`;
}
```

### Transaction History with Filtering

```javascript
async function loadTransactions(filter = 'month') {
    try {
        const params = new URLSearchParams();

        // Apply time filter
        const now = new Date();
        switch (filter) {
            case 'today':
                params.set('since', startOfDay(now).toISOString());
                break;
            case 'week':
                params.set('since', startOfWeek(now).toISOString());
                break;
            case 'month':
                params.set('since', startOfMonth(now).toISOString());
                break;
            // 'all' - no filter
        }

        const history = await apiRequest(
            'GET',
            `/ledger/${state.coopId}/history?${params}`
        );

        state.transactions = history.transactions;
        renderTransactions();

    } catch (error) {
        showToast(getUserFriendlyError(error), 'error');
    }
}

function renderTransactions() {
    const container = elements.transactionList;

    // Clear existing content
    while (container.firstChild) {
        container.removeChild(container.firstChild);
    }

    if (state.transactions.length === 0) {
        const emptyState = document.createElement('p');
        emptyState.className = 'empty-state';
        emptyState.textContent = 'No transactions found';
        container.appendChild(emptyState);
        return;
    }

    for (const tx of state.transactions) {
        const row = createTransactionRow(tx);
        container.appendChild(row);
    }
}

function createTransactionRow(tx) {
    const row = document.createElement('div');
    row.className = 'transaction-row';

    const isIncoming = tx.to === state.did;

    // Icon
    const icon = document.createElement('div');
    icon.className = `tx-icon ${isIncoming ? 'incoming' : 'outgoing'}`;
    icon.textContent = isIncoming ? '↓' : '↑';

    // Details
    const details = document.createElement('div');
    details.className = 'tx-details';

    const party = document.createElement('div');
    party.className = 'tx-party';
    party.textContent = `${isIncoming ? 'From' : 'To'}: ${shortenDid(isIncoming ? tx.from : tx.to)}`;

    const memo = document.createElement('div');
    memo.className = 'tx-memo';
    memo.textContent = tx.memo || 'No memo';

    details.appendChild(party);
    details.appendChild(memo);

    // Amount
    const amount = document.createElement('div');
    amount.className = `tx-amount ${isIncoming ? 'positive' : 'negative'}`;
    amount.textContent = `${isIncoming ? '+' : '-'}${tx.amount} ${tx.currency}`;

    // Date
    const date = document.createElement('div');
    date.className = 'tx-date';
    date.textContent = formatDate(tx.timestamp);

    row.appendChild(icon);
    row.appendChild(details);
    row.appendChild(amount);
    row.appendChild(date);

    return row;
}
```

### CSV Export

```javascript
function exportToCsv() {
    const rows = [
        ['Date', 'Time', 'From', 'To', 'Amount', 'Currency', 'Memo']
    ];

    for (const tx of state.transactions) {
        const date = new Date(tx.timestamp * 1000);
        rows.push([
            date.toLocaleDateString(),
            date.toLocaleTimeString(),
            tx.from,
            tx.to,
            tx.amount.toString(),
            tx.currency,
            `"${(tx.memo || '').replace(/"/g, '""')}"`,  // Escape quotes
        ]);
    }

    const csv = rows.map(row => row.join(',')).join('\n');
    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);

    const a = document.createElement('a');
    a.href = url;
    a.download = `icn-transactions-${state.coopId}.csv`;
    a.click();

    URL.revokeObjectURL(url);
}
```

---

## Debugging and Troubleshooting

### Common Issues

| Symptom | Likely Cause | Solution |
|---------|--------------|----------|
| "Cannot connect to server" | Gateway not running | Start gateway with `icnd --gateway-enable` |
| "Your session has expired" | JWT token expired | Get new token with `icnctl auth login` |
| No real-time updates | WebSocket disconnected | Check connection indicator, refresh page |
| Balance not updating | Event not processed | Check browser console for errors |
| Members list empty | Wrong coop ID | Verify coop ID matches gateway |

### Browser Console Debugging

```javascript
// Add to app.js for debugging
window.DEBUG = {
    state: () => console.log(state),
    ws: () => console.log('WS state:', state.ws?.readyState),
    api: async (path) => {
        const result = await apiRequest('GET', path);
        console.log(result);
        return result;
    },
};

// Usage in console:
// DEBUG.state()
// DEBUG.ws()
// DEBUG.api('/health')
```

### Network Tab Inspection

When debugging API issues:

1. Open DevTools → Network tab
2. Filter by "Fetch/XHR"
3. Look for:
   - Red entries (failed requests)
   - 4xx/5xx status codes
   - Request/response headers
   - Response body for error details

---

## Code Map

| File | Purpose |
|------|---------|
| `web/pilot-ui/index.html` | Page structure, semantic HTML |
| `web/pilot-ui/style.css` | Styling, responsive design, themes |
| `web/pilot-ui/app.js` | Application logic, API calls, state |
| `web/pilot-ui/tests/e2e/` | Playwright end-to-end tests |
| `web/pilot-ui/README.md` | Setup and usage documentation |
| `web/pilot-ui/QUICK-START.md` | User onboarding guide |
| `web/pilot-ui/TREASURER-GUIDE.md` | Financial management guide |
| `web/pilot-ui/ADMIN-GUIDE.md` | Administration reference |

---

## Exercises

### Exercise 1: Add a Refresh Button

Add a manual refresh button that reloads all data:

```javascript
// Implement this function
async function refreshAllData() {
    // Show loading indicator
    // Reload balance, members, transactions, proposals
    // Show success toast
}
```

### Exercise 2: Implement Dark Mode

Add a theme toggle that persists to localStorage:

```css
/* Add dark mode styles */
[data-theme="dark"] {
    --bg-color: #1a1a2e;
    --text-color: #eaeaea;
    /* ... */
}
```

### Exercise 3: Add Proposal Creation

Implement the UI for creating new governance proposals using DOM methods:

```javascript
function createProposalForm() {
    const form = document.createElement('form');
    form.id = 'create-proposal-form';

    // Add title input, description textarea, type select
    // Handle form submission
    // Call apiRequest('POST', '/gov/proposals', data)
}
```

---

## Checkpoints

Before proceeding to Module 9, verify you can:

- [ ] Explain the thin-client architecture and its benefits
- [ ] Trace a user action from button click to API response
- [ ] Implement session management with JWT tokens
- [ ] Handle WebSocket events to update the UI
- [ ] Debug common connection and authentication issues
- [ ] Create responsive layouts that work on mobile
- [ ] Use toast notifications for user feedback
- [ ] Export data to CSV format

---

## Next Steps

In **Module 9: Operations and Deployment**, you'll learn how to deploy the ICN daemon
and web UI to production environments, configure monitoring, and manage the system
in a live cooperative setting.
