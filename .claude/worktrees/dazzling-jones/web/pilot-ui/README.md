# ICN Pilot Web UI

A user-friendly web interface for ICN pilot communities to manage their timebank or mutual credit system.

## 🚀 Quick Start

**New to ICN?** Choose your path:

- **🎯 I want to test locally (5 minutes)**: [Getting Started Guide](GETTING-STARTED.md)
- **🏢 I want to deploy for my cooperative**: [Production Deployment](PRODUCTION-DEPLOY.md)
- **📋 I'm deploying to production**: [Deployment Checklist](DEPLOYMENT-CHECKLIST.md)
- **📚 I want to learn everything**: [Complete Summary](SUMMARY.md)

**Deployment Scripts**:
- `./deploy-ui.sh` - Simple UI deployment (Python/Node/Docker)
- `./seed-demo-data.sh` - Populate with sample data for testing
- `../../deploy/quickstart.sh` - Complete ICN + UI setup with Docker

**User Documentation**:
- [Quick Start for Members](QUICK-START.md) - 5-minute onboarding
- [Treasurer's Guide](TREASURER-GUIDE.md) - Financial management
- [Admin Guide](ADMIN-GUIDE.md) - System administration
- [FAQ](FAQ.md) - Common questions answered

---

## Features

- **Dashboard**: View balance, member count, and recent activity
- **Log Hours**: Record service hours provided to other members
- **History**: View all transactions
- **Members**: See community member list
- **Governance**: Vote on proposals and view community decisions
- **Real-time Updates**: WebSocket notifications for instant updates
- **User-Friendly Errors**: Clear, helpful error messages instead of technical jargon
- **Token Management**: Visual token expiration warnings
- **Auth Help**: Step-by-step guide to getting authentication tokens
- **Toast Notifications**: Non-intrusive success/error messages

## Recent Improvements (Phase 1 Enhancements)

### 1. Better Authentication Experience
- **"How do I get a token?" button** - Opens modal with step-by-step instructions
- **Copy-to-clipboard** functionality for auth commands
- **Token expiration tracking** - Shows countdown in header
- **Auto-expiration warnings** - Alerts at 15, 10, and 5 minutes before expiry
- **Expired token detection** - Prevents login with expired tokens

### 2. User-Friendly Error Messages
All technical errors are now translated to helpful messages:
- ❌ `401 Unauthorized` → ✅ "Your session has expired. Please sign in again."
- ❌ `403 Forbidden` → ✅ "You don't have permission to do that. Check with your administrator."
- ❌ `429 Too Many Requests` → ✅ "Too many requests. Please wait a moment and try again."
- ❌ `NetworkError` → ✅ "Cannot connect to the server. Please check your internet connection."

### 3. Toast Notification System
Modern, non-blocking notifications for:
- ✅ Successful actions (green border)
- ❌ Errors (red border)
- ⚠️ Warnings (yellow border)
- ℹ️ Info messages (blue border)

Auto-dismiss after 5 seconds with manual close option.

### 4. Session Management
- **Automatic logout** on token expiration
- **Persistent sessions** with localStorage
- **Visual token countdown** in header (changes color based on time remaining)
- **Graceful expiration handling** - warns before kicking user out

### 5. Governance UI (Already Existed, Now Documented)
- View active proposals
- Cast votes (For/Against/Abstain)
- See vote tallies in real-time
- View closed proposals with outcomes
- WebSocket updates for live vote counts

## Phase 2 Enhancements (Polish & Mobile Support)

### 1. Comprehensive Responsive Design
- **Mobile-optimized** (≤768px): Vertical layouts, touch-friendly buttons
- **Small mobile** (≤375px): Optimized for iPhone SE and small Android devices
- **Tablet landscape** (769-1024px): Efficient use of screen space
- **iOS auto-zoom prevention**: 16px font size on inputs
- **Touch-friendly scrolling**: Hidden scrollbars, smooth navigation

### 2. Member Directory Search
- **Real-time search** by DID
- **Case-insensitive** filtering
- **Instant results** - no delays
- Useful for large cooperatives with many members

### 3. Transaction History Filtering
- **5 time periods**: Today, This Week, This Month, This Year, All Time
- **Default**: This Month (most common use case)
- **Instant filtering** - no page reload
- Helps treasurers focus on recent activity

### 4. CSV Export for Transactions
- **One-click export** to Excel/Google Sheets
- **Respects current filter** - export only what you see
- **Proper formatting**: Quoted fields, escaped special characters
- **All fields included**: Date, Time, From, To, Amount, Currency, Memo
- Perfect for treasurer reports and analysis

### 5. Card Header System
- Flexible header layout with actions
- Professional UI with filters/search integrated
- Wraps gracefully on mobile

### 6. Loading Skeleton Animation
- CSS-only shimmer effect
- Better perceived performance
- Ready for future loading states

## Phase 3 Enhancements (Advanced Features & Documentation)

### 1. Quick Wins (Productivity Features)
- **⌨️ Keyboard Shortcuts**: Navigate tabs with Ctrl+1-5 (Cmd on Mac)
- **📋 Copy DID Button**: One-click copy button next to member DIDs
- **🔄 Transaction Sorting**: Sort by newest/oldest/highest/lowest amount
- **📈 Balance Trend Indicator**: Visual arrow showing if balance is ↑ up, ↓ down, or → stable
- **⏰ Proposal Deadlines**: Color-coded countdown showing urgency (gray → yellow → red)

### 2. Dashboard Enhancements
- **📊 Balance Chart**: Canvas-based line chart showing balance over last 30 days
- **🗳️ Pending Proposals Widget**: Quick access to open proposals requiring votes
- **🏆 Top Contributors**: Leaderboard showing top 5 most active givers with emoji medals

### 3. Comprehensive Documentation
- **📖 [Quick Start Guide](QUICK-START.md)**: 5-minute onboarding for new users (467 lines)
- **💰 [Treasurer's Guide](TREASURER-GUIDE.md)**: Financial management and reporting (584 lines)
- **⚙️ [Admin Guide](ADMIN-GUIDE.md)**: Complete system administration reference (738 lines)
- **❓ [FAQ](FAQ.md)**: 60+ questions covering all user scenarios (560 lines)

**Total Documentation**: 2,349 lines of professional guides!

### 4. Keyboard Shortcuts Reference
- **Ctrl+1**: Dashboard
- **Ctrl+2**: Log Hours
- **Ctrl+3**: History
- **Ctrl+4**: Members
- **Ctrl+5**: Governance

## Quick Start

### 1. Serve the UI

The UI is static HTML/CSS/JS, so you can serve it with any web server:

```bash
# Python 3
cd web/pilot-ui
python -m http.server 3000

# Node.js
npx serve -s . -l 3000

# Or just open index.html directly in your browser
```

### 2. Start the ICN Gateway

Make sure the ICN daemon is running with the gateway enabled:

```bash
icnd --gateway-enable --gateway-bind 127.0.0.1:8080 --gateway-jwt-secret "your-secret"
```

### 3. Get a JWT Token

Use icnctl to get an authentication token:

```bash
# Get challenge
icnctl auth challenge --did did:icn:your-did

# Sign and verify (you'll need to implement signing)
# For testing, you can use the gateway's test token if available
```

### 4. Connect

1. Open the UI in your browser (http://localhost:3000)
2. Enter the gateway URL (default: http://localhost:8080)
3. Enter your cooperative ID
4. Enter your DID
5. Paste your JWT token
6. Click "Connect"

## Usage

### Logging Hours

1. Click "Log Hours" tab
2. Select the member you provided service to
3. Enter the number of hours
4. Add a description of the service
5. Click "Log Hours"

The other member will see their balance decrease (they owe you) and yours will increase.

### Viewing Balance

Your current balance is shown on the Dashboard.
- **Positive** (green): You have credit - others owe you hours
- **Negative** (red): You owe hours to others

### Understanding Transactions

In a timebank:
- When you help someone, they owe you (their balance decreases, yours increases)
- When someone helps you, you owe them (your balance decreases, theirs increases)

All transactions are visible in the History tab.

## Development

### Customization

The UI is intentionally simple vanilla HTML/CSS/JS for easy customization:

- **style.css**: All styling, easy to adjust colors and layout
- **app.js**: Application logic, API calls
- **index.html**: Page structure

### Adding Features

Common additions for pilots:

1. **Offers/Requests Board**: Add a new tab to display member offers
2. **Profile Page**: Show member details and contact info
3. **Export**: Add CSV export for treasurer reports
4. **Notifications**: Show alerts for new transactions

### API Integration

The UI uses the ICN Gateway REST API. See [docs/api/openapi.yaml](../../docs/api/openapi.yaml) for the full API spec.

Key endpoints used:
- `GET /v1/health` - Check gateway status
- `GET /v1/ledger/{coop}/balance/{did}` - Get member balance
- `GET /v1/ledger/{coop}/history` - Get transactions
- `POST /v1/ledger/{coop}/payment` - Create transaction
- `GET /v1/coops/{coop}/members` - List members

## Deployment

For production pilots:

1. **Serve over HTTPS**: Use a reverse proxy (nginx, caddy) with TLS
2. **Restrict CORS**: Configure gateway to only accept requests from your domain
3. **Token Management**: Implement proper token refresh (tokens expire after 24h)

### Example nginx config

```nginx
server {
    listen 443 ssl;
    server_name timebank.example.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    # Serve static UI
    location / {
        root /var/www/icn-pilot-ui;
        try_files $uri /index.html;
    }

    # Proxy API requests
    location /v1 {
        proxy_pass http://localhost:8080;
        proxy_set_header Host $host;
    }
}
```

## Browser Support

Tested on:
- Chrome 90+
- Firefox 88+
- Safari 14+
- Edge 90+

Requires modern JavaScript features (fetch, async/await, template literals).

## User Guide

### Getting Your First Token

1. Click **"How do I get a token?"** on the login screen
2. Follow the 3-step wizard:
   - Open terminal
   - Run the provided command (click Copy button)
   - Paste the token back into the login form
3. Click **Connect**

The token is valid for 24 hours. You'll see a countdown timer in the header showing when it expires.

### Token Expiration

Watch for these indicators:
- **Green badge** (>1 hour): Token is fine
- **Yellow badge** (<1 hour): Token expiring soon
- **Red badge** (<15 minutes): Get a new token soon!

You'll receive automatic warnings at 15, 10, and 5 minutes before expiration.

### Understanding Notifications

The app uses toast notifications (top-right corner):
- **Green checkmark (✓)**: Success - action completed
- **Red X (✕)**: Error - something went wrong
- **Yellow warning (⚠)**: Warning - attention needed
- **Blue info (ℹ)**: Info - general notification

Click the **×** to dismiss, or they auto-dismiss after 5 seconds.

## Troubleshooting

### Error Messages Explained

The app now shows helpful error messages. Here's what they mean:

**"Cannot connect to the server. Please check your internet connection."**
- Gateway is not running or unreachable
- Check gateway URL (default: `http://localhost:8080`)
- Verify gateway is enabled: `icnd --gateway-enable`

**"Your session has expired. Please sign in again."**
- Your 24-hour token has expired
- Get a new token by clicking "How do I get a token?"
- Run: `icnctl auth login --gateway http://localhost:8080 --coop your-coop-id`

**"You don't have permission to do that."**
- Your token doesn't have the required scope
- Contact your cooperative administrator
- You may need admin privileges for this action

**"Too many requests. Please wait a moment and try again."**
- You're being rate-limited (100 requests per burst)
- Wait 10-30 seconds and try again
- This is a security feature to prevent abuse

**"The requested resource was not found."**
- Check your cooperative ID spelling
- Verify the cooperative exists
- Ask your admin for the correct ID

### No members showing
- Verify you're using the correct cooperative ID
- Check that members have been added via `icnctl coops member add`
- Refresh the page

### Transactions not appearing
- Transactions appear in real-time via WebSocket
- If disconnected, refresh the page
- Check the History tab (dashboard shows last 5 only)

### WebSocket Disconnected
- Check the footer status indicator
- Red dot = disconnected (will auto-reconnect in 5 seconds)
- Green dot = connected
- Refresh page if reconnection fails

### Token Expired During Session
- You'll see a red "Token expired" badge in header
- A persistent warning toast will appear
- Sign out and get a new token

## License

MIT OR Apache-2.0
