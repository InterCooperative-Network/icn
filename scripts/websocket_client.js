// Simple WebSocket client for testing ICN Wallet real-time notifications
// Run with: node websocket_client.js

const WebSocket = require('ws');

// Configuration
const WS_URL = 'ws://127.0.0.1:9876';

// Create WebSocket connection
const ws = new WebSocket(WS_URL);

// Connection opened
ws.on('open', () => {
    console.log(`Connected to ICN Wallet WebSocket server at ${WS_URL}`);
    console.log('Waiting for real-time notifications...');
    console.log('Press Ctrl+C to exit');
});

// Listen for messages
ws.on('message', (data) => {
    try {
        const notification = JSON.parse(data);
        console.log('\nReceived notification:');
        console.log('--------------------');
        console.log(`Type: ${getNotificationType(notification.notification_type)}`);
        console.log(`Message: ${notification.message}`);
        console.log(`Timestamp: ${new Date(notification.timestamp).toLocaleString()}`);
        console.log(`ID: ${notification.id}`);
        console.log('--------------------');
    } catch (e) {
        console.error('Error parsing notification:', e);
        console.log('Raw data:', data);
    }
});

// Connection closed
ws.on('close', () => {
    console.log('Connection closed');
});

// Error handling
ws.on('error', (error) => {
    console.error('WebSocket error:', error);
});

// Handle SIGINT (Ctrl+C)
process.on('SIGINT', () => {
    console.log('\nClosing connection');
    ws.close();
    process.exit(0);
});

// Helper function to convert notification type enum to readable string
function getNotificationType(typeObj) {
    if (!typeObj) return 'Unknown';
    
    const typeKeys = Object.keys(typeObj);
    if (typeKeys.length === 0) return 'Unknown';
    
    const type = typeKeys[0];
    const value = typeObj[type];
    
    if (type === 'DagSyncComplete') {
        return 'DAG Sync Complete';
    } else {
        return `${type}: ${value}`;
    }
} 