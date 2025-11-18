# ICN Gateway Web UI

Modern web interface for the ICN Gateway API.

## Features

- **Dashboard** - Real-time overview with metrics and activity feed
- **Cooperatives** - Create and manage cooperative organizations
- **Governance** - Democratic decision-making with domains, proposals, and voting
- **Ledger** - View balances and transaction history, create payments
- **Real-time Updates** - WebSocket connection for live notifications
- **Responsive Design** - Works on desktop, tablet, and mobile devices

## Technology Stack

- **Frontend**: Vanilla JavaScript (no build tools required)
- **Styling**: Modern CSS with custom design system
- **Real-time**: WebSocket API integration
- **Authentication**: DID-based JWT authentication

## Development

The UI is served automatically by the ICN Gateway server when enabled.

### Running the Gateway with UI

```bash
# Start the daemon with gateway enabled
export ICN_GATEWAY_JWT_SECRET="your-secret-key"
./target/debug/icnd --gateway-enable --gateway-bind 127.0.0.1:8080

# Open your browser
open http://localhost:8080
```

### File Structure

```
static/
├── index.html          # Main application shell
├── css/
│   └── style.css      # Design system and component styles
├── js/
│   ├── api.js         # API client and WebSocket handler
│   └── app.js         # Application logic and UI management
└── README.md          # This file
```

## Usage

1. **Login**: Enter your DID and cooperative ID
2. **Navigate**: Use the top navigation to switch between sections
3. **Real-time**: Connection status shown in header (green = connected)
4. **Notifications**: Toast messages appear for important events
5. **Activity**: Recent events appear in the dashboard feed

## Customization

To customize the static files directory, set the `ICN_STATIC_DIR` environment variable:

```bash
export ICN_STATIC_DIR=/path/to/custom/static
```

## API Integration

The UI integrates with all Gateway API v1 endpoints:

- `/v1/auth/*` - Authentication (challenge/verify)
- `/v1/coops/*` - Cooperative management
- `/v1/gov/*` - Governance (domains/proposals/votes)
- `/v1/ledger/*` - Ledger operations
- `/v1/ws/:coop_id` - WebSocket events
- `/v1/health` - Health checks

## Security Notes

- Authentication required for all protected endpoints
- JWT tokens stored in localStorage
- WebSocket auto-reconnects with exponential backoff
- Per-DID rate limiting protects against abuse
- All API calls include proper error handling

## Future Enhancements

- [ ] Multi-cooperative switching
- [ ] Advanced governance voting (weighted, quadratic)
- [ ] Transaction graphs and analytics
- [ ] Member directory and profiles
- [ ] Settings page for user preferences
- [ ] Dark mode theme
