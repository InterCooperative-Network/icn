#!/usr/bin/env node
/**
 * WebSocket Event Monitor for ICN Runtime
 * 
 * This script connects to the ICN Runtime WebSocket endpoint and monitors events
 * for testing and debugging purposes. It can:
 * 
 * 1. Log all events to console and/or file
 * 2. Filter events by type
 * 3. Wait for specific events to occur
 * 4. Execute callback functions when matching events are received
 * 
 * Usage:
 *   node websocket_monitor.js [options]
 * 
 * Options:
 *   --url <url>         WebSocket URL (default: ws://localhost:8090/events)
 *   --log-file <path>   Log file path (default: websocket_events.log)
 *   --filter <type>     Filter events by type (can be used multiple times)
 *   --wait-for <type>   Wait for specific event type before exiting
 *   --timeout <ms>      Timeout in milliseconds (default: 60000)
 *   --json              Output events as JSON
 *   --quiet             Suppress console output
 */

const WebSocket = require('ws');
const fs = require('fs');
const path = require('path');

// Parse command line arguments
const args = process.argv.slice(2);
const options = {
  url: 'ws://localhost:8090/events',
  logFile: 'websocket_events.log',
  filters: [],
  waitFor: null,
  timeout: 60000,
  json: false,
  quiet: false
};

for (let i = 0; i < args.length; i++) {
  switch (args[i]) {
    case '--url':
      options.url = args[++i];
      break;
    case '--log-file':
      options.logFile = args[++i];
      break;
    case '--filter':
      options.filters.push(args[++i]);
      break;
    case '--wait-for':
      options.waitFor = args[++i];
      break;
    case '--timeout':
      options.timeout = parseInt(args[++i], 10);
      break;
    case '--json':
      options.json = true;
      break;
    case '--quiet':
      options.quiet = true;
      break;
    case '--help':
      showHelp();
      process.exit(0);
      break;
  }
}

// Setup logging
const logStream = options.logFile ? fs.createWriteStream(options.logFile, { flags: 'a' }) : null;

function log(message, isEvent = false) {
  const timestamp = new Date().toISOString();
  const logMessage = `[${timestamp}] ${message}`;
  
  if (!options.quiet || isEvent) {
    console.log(logMessage);
  }
  
  if (logStream) {
    logStream.write(logMessage + '\n');
  }
}

// Show startup information
log(`WebSocket Event Monitor started`);
log(`Connecting to: ${options.url}`);
if (options.filters.length > 0) {
  log(`Filtering events: ${options.filters.join(', ')}`);
}
if (options.waitFor) {
  log(`Waiting for event: ${options.waitFor}`);
  log(`Timeout: ${options.timeout}ms`);
}

// Connect to WebSocket
const ws = new WebSocket(options.url);

// Track events and set timeout if needed
const receivedEvents = [];
let waitForTimeout = null;
if (options.waitFor) {
  waitForTimeout = setTimeout(() => {
    log(`Timeout waiting for event: ${options.waitFor}`);
    cleanupAndExit(1);
  }, options.timeout);
}

// WebSocket event handlers
ws.on('open', () => {
  log('Connected to ICN Runtime WebSocket');
});

ws.on('message', (data) => {
  try {
    const event = JSON.parse(data);
    
    // Check if event passes filters
    if (options.filters.length > 0 && !options.filters.includes(event.type)) {
      return;
    }
    
    // Format the event for logging
    const eventOutput = options.json ? JSON.stringify(event, null, 2) : 
      `Event: ${event.type}, ID: ${event.id || 'N/A'}, Timestamp: ${event.timestamp || 'N/A'}`;
    
    // Log the event
    log(eventOutput, true);
    
    // Add to received events
    receivedEvents.push(event);
    
    // Check if this is the event we're waiting for
    if (options.waitFor && event.type === options.waitFor) {
      log(`Received waited-for event: ${options.waitFor}`);
      if (waitForTimeout) {
        clearTimeout(waitForTimeout);
      }
      // Exit after a short delay to allow for log flushing
      setTimeout(() => cleanupAndExit(0), 500);
    }
  } catch (error) {
    log(`Error parsing message: ${error.message}`);
    log(`Raw message: ${data}`);
  }
});

ws.on('error', (error) => {
  log(`WebSocket error: ${error.message}`);
  cleanupAndExit(1);
});

ws.on('close', () => {
  log('WebSocket connection closed');
  cleanupAndExit(0);
});

// Handle process termination
process.on('SIGINT', () => {
  log('Received SIGINT, shutting down');
  cleanupAndExit(0);
});

process.on('SIGTERM', () => {
  log('Received SIGTERM, shutting down');
  cleanupAndExit(0);
});

// Clean up resources and exit
function cleanupAndExit(code) {
  if (ws.readyState === WebSocket.OPEN) {
    ws.close();
  }
  
  if (logStream) {
    logStream.end();
  }
  
  // Wait a moment to ensure logs are flushed
  setTimeout(() => {
    process.exit(code);
  }, 500);
}

// Display help information
function showHelp() {
  console.log(`
WebSocket Event Monitor for ICN Runtime

Usage:
  node websocket_monitor.js [options]

Options:
  --url <url>         WebSocket URL (default: ws://localhost:8090/events)
  --log-file <path>   Log file path (default: websocket_events.log)
  --filter <type>     Filter events by type (can be used multiple times)
  --wait-for <type>   Wait for specific event type before exiting
  --timeout <ms>      Timeout in milliseconds (default: 60000)
  --json              Output events as JSON
  --quiet             Suppress console output
  --help              Show this help message
  `);
} 