# ICN Node Dashboard

A modern web-based administration dashboard for monitoring and managing your ICN node.

## Features

### 📊 Overview
- Real-time statistics (peers, ledger entries, proposals, compute tasks)
- Network activity charts
- Ledger volume visualization
- Recent activity feed

### 🔗 Network Management
- View connected peers
- Trust scores
- Connection timestamps
- Peer addresses

### 💰 Ledger Monitoring
- Browse all ledger entries
- Filter by time period
- Export to CSV
- Transaction details

### 🗳️ Governance
- View active proposals
- Vote tallies (For/Against/Abstain)
- Proposal status tracking
- Historical proposals

### ⚙️ Compute Tasks
- Monitor distributed compute tasks
- Filter by status (queued, running, completed, failed)
- Task execution details
- Performance metrics

### 🤝 Federation
- List federated cooperatives
- Gateway endpoints
- Last seen timestamps
- Federation status

### 📈 System Metrics
- Gossip performance (messages/sec)
- Trust graph size
- Storage usage
- Network bandwidth

### 📋 System Logs
- Real-time log streaming
- Filter by log level (error, warn, info, debug)
- Terminal-style display
- Auto-scroll

### ⚙️ Settings
- Configure API endpoint
- Configure WebSocket endpoint
- Set refresh interval
- Enable/disable auto-refresh

## Quick Start

### Option 1: Python HTTP Server (Simplest)

```bash
cd web/dashboard
python3 -m http.server 8080
```

Then visit: http://localhost:8080

### Option 2: Node.js HTTP Server

```bash
cd web/dashboard
npx http-server -p 8080
```

### Option 3: Docker

```bash
cd web/dashboard
docker build -t icn-dashboard .
docker run -p 8080:80 icn-dashboard
```

## Configuration

On first launch, configure the dashboard in **Settings**:

1. **API Endpoint**: Default is `http://localhost:8000` (where ICN gateway runs)
2. **WebSocket Endpoint**: Default is `ws://localhost:8000/ws`
3. **Refresh Interval**: How often to update data (in seconds)
4. **Auto-refresh**: Enable/disable automatic data updates

Settings are saved in localStorage and persist across sessions.

## Requirements

- **ICN Node**: Must be running with Gateway API enabled
- **Modern Browser**: Chrome, Firefox, Safari, or Edge (recent versions)
- **WebSocket Support**: For real-time updates

## API Endpoints Used

The dashboard queries these ICN Gateway API endpoints:

- `GET /v1/node/info` - Node information and DID
- `GET /v1/network/peers` - Connected peers list
- `GET /v1/ledger/entries` - Ledger transaction history
- `GET /v1/governance/proposals` - Governance proposals
- `GET /v1/compute/tasks` - Compute task queue
- `GET /v1/federation/cooperatives` - Federated coops
- `GET /v1/metrics` - System performance metrics
- `GET /v1/logs` - System logs
- `WS /ws` - WebSocket for real-time updates

## Real-Time Updates

The dashboard uses WebSocket to receive real-time notifications for:

- New ledger entries
- Proposal vote updates
- Compute task status changes
- Network peer connections/disconnections

## Browser Support

- ✅ Chrome 90+
- ✅ Firefox 88+
- ✅ Safari 14+
- ✅ Edge 90+

## Security Notes

**⚠️ Important**: This dashboard connects directly to your ICN node's Gateway API. For production use:

1. **Use HTTPS**: Deploy behind a reverse proxy with TLS
2. **Authentication**: Enable Gateway API authentication
3. **Firewall**: Restrict access to authorized IP addresses
4. **CORS**: Configure CORS properly for your domain

Example nginx configuration:

```nginx
server {
    listen 443 ssl;
    server_name dashboard.mycoop.org;
    
    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;
    
    location / {
        root /var/www/icn-dashboard;
        try_files $uri $uri/ /index.html;
    }
    
    location /v1/ {
        proxy_pass http://localhost:8000;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

## Development

The dashboard is a single-page application (SPA) with:

- **index.html**: Main HTML structure
- **style.css**: Styling (dark theme, responsive)
- **app.js**: Application logic (vanilla JavaScript)

No build process required - just edit and refresh!

### Adding a New View

1. Add navigation item in `index.html` sidebar
2. Create view container with `class="view"` and unique ID
3. Add case in `loadViewData()` method in `app.js`
4. Implement data loading function
5. Add title mapping in `switchView()` method

## Troubleshooting

### Dashboard shows "Offline"
- Check that ICN node is running: `systemctl status icnd`
- Verify Gateway API is enabled in `icn.toml`
- Check API endpoint in Settings matches your node's address

### No data appears
- Open browser console (F12) to check for errors
- Verify API endpoints return data: `curl http://localhost:8000/v1/node/info`
- Check CORS settings if dashboard is on different domain

### WebSocket won't connect
- Ensure WebSocket endpoint is correct (typically same as API but with `ws://` or `wss://`)
- Check firewall allows WebSocket connections
- Verify no reverse proxy is blocking WebSocket upgrade

### Charts not loading
- Charts are placeholder elements - integrate with charting library like Chart.js or D3.js for visualization
- Current implementation shows "Loading..." text

## Future Enhancements

- [ ] Add Chart.js for interactive graphs
- [ ] Implement CSV export functionality
- [ ] Add proposal creation UI
- [ ] Enable log downloading
- [ ] Add dark/light theme toggle
- [ ] Mobile-responsive improvements
- [ ] Multi-node monitoring
- [ ] Alert/notification system
- [ ] Performance profiling tools

## License

MIT - See LICENSE file

## Support

- Documentation: https://github.com/InterCooperative-Network/icn/tree/main/docs
- Issues: https://github.com/InterCooperative-Network/icn/issues
- Discussions: https://github.com/InterCooperative-Network/icn/discussions
