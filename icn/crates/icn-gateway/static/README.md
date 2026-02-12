# ICN Pilot Web UI

A simple web interface for ICN pilot communities to manage their timebank or mutual credit system.

## Features

- **Dashboard**: View balance, member count, and recent activity
- **Log Hours**: Record service hours provided to other members
- **History**: View all transactions
- **Members**: See community member list

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

## Troubleshooting

### "Connection failed"
- Check that the gateway is running
- Verify the gateway URL is correct
- Check browser console for CORS errors

### "Unauthorized"
- Token may have expired (24h default)
- Get a new token with `icnctl auth`

### No members showing
- Verify you're using the correct cooperative ID
- Check that members have been added via `icnctl`

### Transactions not appearing
- Refresh the page
- Check the History tab (recent activity only shows last 5)

## License

MIT OR Apache-2.0
