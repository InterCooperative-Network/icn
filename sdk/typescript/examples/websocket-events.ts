/**
 * WebSocket Events Example
 *
 * Shows how to subscribe to real-time events from ICN:
 * - Connect to WebSocket
 * - Authenticate
 * - Handle different event types (ledger, membership, governance, compute)
 * - Reconnection logic
 *
 * Run: npx ts-node examples/websocket-events.ts
 */

import { ICNClient, WsMessage, CoopEventType } from '../src';

const COOP_ID = 'timebank-coop';

async function main() {
  // Create authenticated client
  const client = new ICNClient({
    baseUrl: 'http://localhost:8080',
    token: 'your-jwt-token', // Get this via authentication
  });

  console.log('Connecting to WebSocket...\n');

  let reconnectAttempts = 0;
  const maxReconnectAttempts = 5;

  function connect() {
    const ws = client.connectWebSocket(COOP_ID, {
      onOpen: () => {
        console.log('WebSocket connected, authenticating...');
        reconnectAttempts = 0;
      },

      onMessage: (message: WsMessage) => {
        handleMessage(message);
      },

      onError: (error) => {
        console.error('WebSocket error:', error.message);
      },

      onClose: () => {
        console.log('WebSocket closed');

        // Attempt reconnection
        if (reconnectAttempts < maxReconnectAttempts) {
          reconnectAttempts++;
          const delay = Math.min(1000 * Math.pow(2, reconnectAttempts), 30000);
          console.log(`Reconnecting in ${delay / 1000}s (attempt ${reconnectAttempts})...`);
          setTimeout(connect, delay);
        } else {
          console.log('Max reconnection attempts reached');
        }
      },
    });

    return ws;
  }

  connect();

  // Keep the process running
  console.log('Listening for events. Press Ctrl+C to exit.\n');
}

function handleMessage(message: WsMessage) {
  switch (message.type) {
    case 'AuthOk':
      console.log(`Authenticated as ${message.did}\n`);
      console.log('Waiting for events...\n');
      break;

    case 'Error':
      console.error(`Server error: ${message.message}`);
      break;

    case 'Event':
      handleEvent(message.event_type, message.payload);
      break;

    default:
      console.log('Unknown message:', message);
  }
}

function handleEvent(eventType: CoopEventType, payload: unknown) {
  const timestamp = new Date().toLocaleTimeString();

  switch (eventType) {
    // -------------------------------------------------------------------------
    // Ledger Events
    // -------------------------------------------------------------------------
    case 'PaymentCreated': {
      const p = payload as {
        id: string;
        from: string;
        to: string;
        amount: number;
        currency: string;
        memo?: string;
      };

      console.log(`[${timestamp}] PAYMENT`);
      console.log(`  ${truncate(p.from)} -> ${truncate(p.to)}`);
      console.log(`  Amount: ${p.amount} ${p.currency}`);
      if (p.memo) console.log(`  Memo: ${p.memo}`);
      console.log();

      // Example: Send notification
      // await sendNotification(p.to, `You received ${p.amount} ${p.currency}`);
      break;
    }

    // -------------------------------------------------------------------------
    // Membership Events
    // -------------------------------------------------------------------------
    case 'MemberAdded': {
      const m = payload as { coop_id: string; did: string; role: string };

      console.log(`[${timestamp}] MEMBER ADDED`);
      console.log(`  DID: ${truncate(m.did)}`);
      console.log(`  Role: ${m.role}`);
      console.log();
      break;
    }

    case 'MemberRemoved': {
      const m = payload as { coop_id: string; did: string };

      console.log(`[${timestamp}] MEMBER REMOVED`);
      console.log(`  DID: ${truncate(m.did)}`);
      console.log();
      break;
    }

    case 'RoleUpdated': {
      const r = payload as { coop_id: string; did: string; role: string };

      console.log(`[${timestamp}] ROLE UPDATED`);
      console.log(`  DID: ${truncate(r.did)}`);
      console.log(`  New Role: ${r.role}`);
      console.log();
      break;
    }

    // -------------------------------------------------------------------------
    // Settings Events
    // -------------------------------------------------------------------------
    case 'SettingsUpdated': {
      const s = payload as { coop_id: string; settings: Record<string, unknown> };

      console.log(`[${timestamp}] SETTINGS UPDATED`);
      console.log(`  Changes: ${JSON.stringify(s.settings)}`);
      console.log();
      break;
    }

    // -------------------------------------------------------------------------
    // Governance Events
    // -------------------------------------------------------------------------
    case 'GovernanceDomainCreated': {
      const d = payload as { id: string; name: string; members: string[] };

      console.log(`[${timestamp}] GOVERNANCE DOMAIN CREATED`);
      console.log(`  ID: ${d.id}`);
      console.log(`  Name: ${d.name}`);
      console.log(`  Members: ${d.members.length}`);
      console.log();
      break;
    }

    case 'GovernanceProposalCreated': {
      const p = payload as { id: string; domain_id: string; title: string; kind: string };

      console.log(`[${timestamp}] PROPOSAL CREATED`);
      console.log(`  ID: ${p.id}`);
      console.log(`  Title: ${p.title}`);
      console.log(`  Kind: ${p.kind}`);
      console.log();
      break;
    }

    case 'GovernanceProposalOpened': {
      const p = payload as { proposal_id: string; opened_at: number };

      console.log(`[${timestamp}] PROPOSAL OPENED FOR VOTING`);
      console.log(`  ID: ${p.proposal_id}`);
      console.log();
      break;
    }

    case 'GovernanceProposalClosed': {
      const p = payload as {
        proposal_id: string;
        accepted: boolean;
        votes_for: number;
        votes_against: number;
      };

      console.log(`[${timestamp}] PROPOSAL CLOSED`);
      console.log(`  ID: ${p.proposal_id}`);
      console.log(`  Result: ${p.accepted ? 'ACCEPTED' : 'REJECTED'}`);
      console.log(`  Votes: ${p.votes_for} for, ${p.votes_against} against`);
      console.log();
      break;
    }

    case 'GovernanceVoteCast': {
      const v = payload as { proposal_id: string; voter: string; choice: string };

      console.log(`[${timestamp}] VOTE CAST`);
      console.log(`  Proposal: ${v.proposal_id}`);
      console.log(`  Voter: ${truncate(v.voter)}`);
      console.log(`  Choice: ${v.choice}`);
      console.log();
      break;
    }

    // -------------------------------------------------------------------------
    // Compute Events
    // -------------------------------------------------------------------------
    case 'ComputeTaskSubmitted': {
      const t = payload as {
        task_id: string;
        task_hash: string;
        submitter: string;
        fuel_limit: number;
      };

      console.log(`[${timestamp}] COMPUTE TASK SUBMITTED`);
      console.log(`  Task ID: ${t.task_id}`);
      console.log(`  Hash: ${t.task_hash}`);
      console.log(`  Submitter: ${truncate(t.submitter)}`);
      console.log(`  Fuel Limit: ${t.fuel_limit}`);
      console.log();
      break;
    }

    case 'ComputeTaskClaimed': {
      const t = payload as { task_hash: string; executor: string };

      console.log(`[${timestamp}] COMPUTE TASK CLAIMED`);
      console.log(`  Hash: ${t.task_hash}`);
      console.log(`  Executor: ${truncate(t.executor)}`);
      console.log();
      break;
    }

    case 'ComputeTaskCompleted': {
      const t = payload as {
        task_hash: string;
        executor: string;
        outcome: string;
        fuel_used: number;
        duration_ms: number;
      };

      console.log(`[${timestamp}] COMPUTE TASK COMPLETED`);
      console.log(`  Hash: ${t.task_hash}`);
      console.log(`  Executor: ${truncate(t.executor)}`);
      console.log(`  Outcome: ${t.outcome}`);
      console.log(`  Fuel Used: ${t.fuel_used}`);
      console.log(`  Duration: ${t.duration_ms}ms`);
      console.log();
      break;
    }

    case 'ComputeTaskCancelled': {
      const t = payload as { task_hash: string; reason?: string };

      console.log(`[${timestamp}] COMPUTE TASK CANCELLED`);
      console.log(`  Hash: ${t.task_hash}`);
      if (t.reason) console.log(`  Reason: ${t.reason}`);
      console.log();
      break;
    }

    default:
      console.log(`[${timestamp}] ${eventType}`);
      console.log(`  Payload: ${JSON.stringify(payload)}`);
      console.log();
  }
}

function truncate(did: string): string {
  if (!did || did.length <= 20) return did;
  return `${did.slice(0, 12)}...${did.slice(-6)}`;
}

main().catch(console.error);

// -------------------------------------------------------------------------
// Usage Patterns
// -------------------------------------------------------------------------

/**
 * Example: Building a notification system
 */
function notificationExample() {
  const client = new ICNClient({
    baseUrl: 'http://localhost:8080',
    token: 'your-jwt-token',
  });

  client.connectWebSocket('timebank-coop', {
    onMessage: async (message) => {
      if (message.type === 'Event' && message.event_type === 'PaymentCreated') {
        const payment = message.payload as { to: string; amount: number };

        // Send email notification
        // await sendEmail(payment.to, `You received ${payment.amount} hours`);

        // Or push notification
        // await sendPushNotification(payment.to, 'New payment received');

        // Or update UI
        // store.dispatch(addTransaction(payment));
      }
    },
  });
}

/**
 * Example: Building a real-time dashboard
 */
function dashboardExample() {
  const state = {
    recentTransactions: [] as unknown[],
    activeProposals: [] as unknown[],
    activeTasks: [] as unknown[],
    memberCount: 0,
  };

  const client = new ICNClient({
    baseUrl: 'http://localhost:8080',
    token: 'your-jwt-token',
  });

  client.connectWebSocket('timebank-coop', {
    onMessage: (message) => {
      if (message.type !== 'Event') return;

      switch (message.event_type) {
        case 'PaymentCreated':
          state.recentTransactions.unshift(message.payload);
          state.recentTransactions = state.recentTransactions.slice(0, 10);
          // updateUI();
          break;

        case 'MemberAdded':
          state.memberCount++;
          // updateUI();
          break;

        case 'MemberRemoved':
          state.memberCount--;
          // updateUI();
          break;

        case 'GovernanceProposalOpened':
          state.activeProposals.push(message.payload);
          // updateUI();
          break;

        case 'GovernanceProposalClosed':
          state.activeProposals = state.activeProposals.filter(
            (p: any) => p.proposal_id !== (message.payload as any).proposal_id
          );
          // updateUI();
          break;

        case 'ComputeTaskSubmitted':
          state.activeTasks.push(message.payload);
          // updateUI();
          break;

        case 'ComputeTaskCompleted':
        case 'ComputeTaskCancelled':
          state.activeTasks = state.activeTasks.filter(
            (t: any) => t.task_hash !== (message.payload as any).task_hash
          );
          // updateUI();
          break;
      }
    },
  });
}

/**
 * Example: Event logging for audit trail
 */
function auditLogExample() {
  const client = new ICNClient({
    baseUrl: 'http://localhost:8080',
    token: 'your-jwt-token',
  });

  client.connectWebSocket('timebank-coop', {
    onMessage: (message) => {
      if (message.type === 'Event') {
        const logEntry = {
          timestamp: new Date().toISOString(),
          event_type: message.event_type,
          payload: message.payload,
        };

        // Write to log file
        // fs.appendFileSync('audit.log', JSON.stringify(logEntry) + '\n');

        // Or send to logging service
        // await loggingService.log(logEntry);

        console.log('Audit:', JSON.stringify(logEntry));
      }
    },
  });
}
