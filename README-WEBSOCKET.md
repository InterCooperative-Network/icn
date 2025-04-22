# ICN Wallet Real-Time Updates

The ICN Wallet now supports real-time updates via WebSockets. This enables applications to receive immediate notifications about changes in the federation, such as new proposals, votes, and DAG updates.

## Features

- Real-time notifications for all wallet events
- WebSocket-based API for easy integration
- Supports multiple simultaneous clients
- Low latency updates
- Secure communication

## Notification Types

The following notification types are supported:

- `ProposalPassed`: A proposal has been approved and executed
- `ProposalRejected`: A proposal has been rejected
- `GuardianVoteRequest`: A request for a guardian to cast a vote
- `RecoveryRequest`: A request to recover an identity
- `NewProposal`: A new proposal has been created
- `DagSyncComplete`: The DAG has been synchronized with the federation

## Running the WebSocket Server

You can start the WebSocket server in two ways:

### 1. As a standalone server

```bash
icn-wallet websocket --host 127.0.0.1 --port 9876
```

Options:
- `--host`: Host to bind to (default: 127.0.0.1)
- `--port`: Port to listen on (default: 9876)

### 2. As part of the TUI

The WebSocket server is automatically started when you launch the TUI:

```bash
icn-wallet tui
```

## Connecting to the WebSocket Server

You can connect to the WebSocket server using any WebSocket client. A simple JavaScript client is provided in the `scripts` directory:

```bash
cd scripts
npm install
node websocket_client.js
```

## WebSocket API

### Connecting

Connect to the WebSocket server at `ws://host:port`, where `host` and `port` are the values specified when starting the server.

### Receiving Notifications

Notifications are sent as JSON objects with the following structure:

```json
{
  "notification_type": {
    "NotificationType": "value"
  },
  "message": "Human-readable message",
  "timestamp": "ISO 8601 timestamp",
  "id": "Unique notification ID",
  "read": false
}
```

The `notification_type` field contains an object with a single key-value pair, where the key is the type of notification and the value is additional data (if any).

Example:

```json
{
  "notification_type": {
    "NewProposal": "abc123"
  },
  "message": "New proposal: Update treasury allocation",
  "timestamp": "2025-04-22T15:30:00Z",
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "read": false
}
```

## Using with Other Applications

The WebSocket API makes it easy to integrate ICN Wallet notifications into other applications:

- Web applications can use the standard WebSocket API
- Mobile applications can use WebSocket libraries available for most platforms
- Desktop applications can use WebSocket libraries or create HTTP bridges

## Security Considerations

- The WebSocket server only accepts connections from localhost by default
- For production use, consider adding authentication and TLS
- Enable access from other hosts only if necessary and with proper security measures 