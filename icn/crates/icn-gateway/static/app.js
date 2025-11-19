/**
 * ICN Pilot UI - Application Logic
 */

// State
const state = {
    gatewayUrl: '',
    coopId: '',
    did: '',
    token: '',
    members: [],
    transactions: [],
    proposals: [],
    ws: null,
    wsConnected: false,
};

// DOM Elements
const elements = {
    // Screens
    loginScreen: document.getElementById('login-screen'),
    mainScreen: document.getElementById('main-screen'),

    // Login form
    gatewayUrl: document.getElementById('gateway-url'),
    coopId: document.getElementById('coop-id'),
    did: document.getElementById('did'),
    token: document.getElementById('token'),
    loginBtn: document.getElementById('login-btn'),
    loginError: document.getElementById('login-error'),
    logoutBtn: document.getElementById('logout-btn'),

    // Header
    coopName: document.getElementById('coop-name'),
    userDid: document.getElementById('user-did'),

    // Navigation
    navBtns: document.querySelectorAll('.nav-btn'),
    tabContents: document.querySelectorAll('.tab-content'),

    // Dashboard
    myBalance: document.getElementById('my-balance'),
    totalMembers: document.getElementById('total-members'),
    monthlyHours: document.getElementById('monthly-hours'),
    recentActivity: document.getElementById('recent-activity'),

    // Log Hours
    logHoursForm: document.getElementById('log-hours-form'),
    recipient: document.getElementById('recipient'),
    hours: document.getElementById('hours'),
    memo: document.getElementById('memo'),
    logResult: document.getElementById('log-result'),

    // History
    transactionList: document.getElementById('transaction-list'),

    // Members
    memberList: document.getElementById('member-list'),

    // Governance
    proposalList: document.getElementById('proposal-list'),
    closedProposals: document.getElementById('closed-proposals'),

    // Footer
    connectionStatus: document.getElementById('connection-status'),
    lastUpdate: document.getElementById('last-update'),
};

// API Client
async function apiRequest(method, path, body = null) {
    const url = `${state.gatewayUrl}/v1${path}`;
    const headers = {
        'Content-Type': 'application/json',
    };

    if (state.token) {
        headers['Authorization'] = `Bearer ${state.token}`;
    }

    const options = {
        method,
        headers,
    };

    if (body) {
        options.body = JSON.stringify(body);
    }

    const response = await fetch(url, options);

    if (!response.ok) {
        const error = await response.json().catch(() => ({}));
        throw new Error(error.error || response.statusText);
    }

    if (response.status === 204) {
        return null;
    }

    return response.json();
}

// Helper Functions
function truncateDid(did) {
    if (!did || did.length <= 20) return did;
    return `${did.slice(0, 12)}...${did.slice(-6)}`;
}

function formatDate(timestamp) {
    return new Date(timestamp * 1000).toLocaleDateString();
}

function formatDateTime(timestamp) {
    return new Date(timestamp * 1000).toLocaleString();
}

function showError(element, message) {
    element.textContent = message;
    element.style.display = 'block';
}

function clearError(element) {
    element.textContent = '';
    element.style.display = 'none';
}

function showResult(element, message, isSuccess) {
    element.textContent = message;
    element.className = `result-message ${isSuccess ? 'success' : 'error'}`;
    element.style.display = 'block';
}

function updateConnectionStatus(connected) {
    const dot = elements.connectionStatus.querySelector('.status-dot');
    if (connected) {
        dot.classList.remove('disconnected');
        elements.connectionStatus.innerHTML = '<span class="status-dot"></span> Connected';
    } else {
        dot.classList.add('disconnected');
        elements.connectionStatus.innerHTML = '<span class="status-dot disconnected"></span> Disconnected';
    }
}

// Login
async function login() {
    clearError(elements.loginError);

    state.gatewayUrl = elements.gatewayUrl.value.trim().replace(/\/$/, '');
    state.coopId = elements.coopId.value.trim();
    state.did = elements.did.value.trim();
    state.token = elements.token.value.trim();

    if (!state.gatewayUrl || !state.coopId || !state.did || !state.token) {
        showError(elements.loginError, 'Please fill in all fields');
        return;
    }

    try {
        elements.loginBtn.disabled = true;
        elements.loginBtn.textContent = 'Connecting...';

        // Test connection by fetching health
        await apiRequest('GET', '/health');

        // Fetch balance to verify auth
        await apiRequest('GET', `/ledger/${state.coopId}/balance/${encodeURIComponent(state.did)}`);

        // Save to localStorage
        localStorage.setItem('icn-gateway', state.gatewayUrl);
        localStorage.setItem('icn-coop', state.coopId);
        localStorage.setItem('icn-did', state.did);
        localStorage.setItem('icn-token', state.token);

        // Show main screen
        elements.loginScreen.classList.add('hidden');
        elements.mainScreen.classList.remove('hidden');

        // Update header
        elements.coopName.textContent = state.coopId;
        elements.userDid.textContent = truncateDid(state.did);

        // Load data
        await loadAllData();
        updateConnectionStatus(true);

        // Connect WebSocket for real-time updates
        connectWebSocket();

    } catch (error) {
        showError(elements.loginError, `Connection failed: ${error.message}`);
        updateConnectionStatus(false);
    } finally {
        elements.loginBtn.disabled = false;
        elements.loginBtn.textContent = 'Connect';
    }
}

function logout() {
    // Disconnect WebSocket
    disconnectWebSocket();

    localStorage.removeItem('icn-token');
    state.token = '';
    elements.mainScreen.classList.add('hidden');
    elements.loginScreen.classList.remove('hidden');
}

// Data Loading
async function loadAllData() {
    await Promise.all([
        loadBalance(),
        loadMembers(),
        loadTransactions(),
        loadProposals(),
    ]);

    elements.lastUpdate.textContent = `Updated: ${new Date().toLocaleTimeString()}`;
}

async function loadBalance() {
    try {
        const balance = await apiRequest(
            'GET',
            `/ledger/${state.coopId}/balance/${encodeURIComponent(state.did)}`
        );

        const value = balance.balance.toFixed(1);
        elements.myBalance.textContent = value;
        elements.myBalance.className = `stat-value ${balance.balance >= 0 ? 'positive' : 'negative'}`;

    } catch (error) {
        console.error('Failed to load balance:', error);
        elements.myBalance.textContent = '--';
    }
}

async function loadMembers() {
    try {
        const members = await apiRequest('GET', `/coops/${state.coopId}/members`);
        state.members = members;

        elements.totalMembers.textContent = members.length;

        // Update recipient dropdown
        elements.recipient.innerHTML = '<option value="">Select member...</option>';
        for (const member of members) {
            if (member.did !== state.did) {
                const option = document.createElement('option');
                option.value = member.did;
                option.textContent = truncateDid(member.did);
                elements.recipient.appendChild(option);
            }
        }

        // Update member list
        renderMemberList(members);

    } catch (error) {
        console.error('Failed to load members:', error);
        elements.totalMembers.textContent = '--';
    }
}

async function loadTransactions() {
    try {
        const history = await apiRequest(
            'GET',
            `/ledger/${state.coopId}/history?limit=50`
        );
        state.transactions = history.transactions;

        // Calculate monthly hours
        const oneMonthAgo = Date.now() / 1000 - 30 * 24 * 60 * 60;
        const monthlyTx = history.transactions.filter(tx => tx.timestamp > oneMonthAgo);
        const totalHours = monthlyTx.reduce((sum, tx) => sum + tx.amount, 0);
        elements.monthlyHours.textContent = totalHours.toFixed(1);

        // Render lists
        renderRecentActivity(history.transactions.slice(0, 5));
        renderTransactionList(history.transactions);

    } catch (error) {
        console.error('Failed to load transactions:', error);
        elements.monthlyHours.textContent = '--';
    }
}

async function loadProposals() {
    try {
        // Try to load proposals from governance API
        const proposals = await apiRequest('GET', '/gov/proposals');
        state.proposals = proposals;

        // Split into open and closed
        const openProposals = proposals.filter(p => p.state === 'Open');
        const closedProposals = proposals.filter(p => p.state === 'Closed');

        // Render lists
        await renderProposalList(openProposals, elements.proposalList, true);
        await renderProposalList(closedProposals, elements.closedProposals, false);

    } catch (error) {
        console.error('Failed to load proposals:', error);
        if (elements.proposalList) {
            elements.proposalList.innerHTML = '<p class="empty-state">No proposals available</p>';
        }
    }
}

async function renderProposalList(proposals, container, showVoteButtons) {
    if (!container) return;

    if (proposals.length === 0) {
        container.innerHTML = `<p class="empty-state">${showVoteButtons ? 'No active proposals' : 'No closed proposals yet'}</p>`;
        return;
    }

    const html = await Promise.all(proposals.map(async (proposal) => {
        // Get vote tally
        let votes = { for_votes: 0, against_votes: 0, abstain_votes: 0 };
        try {
            votes = await apiRequest('GET', `/gov/proposals/${proposal.id}/votes`);
        } catch (e) {
            // Votes might not be available
        }

        const stateClass = proposal.state.toLowerCase();

        let actionsHtml = '';
        if (showVoteButtons) {
            // Use data attributes to prevent XSS from proposal.id
            const escapedId = escapeHtml(proposal.id);
            actionsHtml = `
                <div class="proposal-actions">
                    <button class="btn-vote for" data-proposal-id="${escapedId}" data-vote="for">For</button>
                    <button class="btn-vote against" data-proposal-id="${escapedId}" data-vote="against">Against</button>
                    <button class="btn-vote abstain" data-proposal-id="${escapedId}" data-vote="abstain">Abstain</button>
                </div>
            `;
        } else if (proposal.outcome) {
            const outcomeClass = proposal.outcome === 'Accepted' ? 'accepted' : 'rejected';
            actionsHtml = `<div class="proposal-outcome ${outcomeClass}">${proposal.outcome}</div>`;
        }

        return `
            <div class="proposal-item">
                <div class="proposal-header">
                    <div class="proposal-title">${escapeHtml(proposal.title)}</div>
                    <div class="proposal-state ${stateClass}">${proposal.state}</div>
                </div>
                ${proposal.description ? `<div class="proposal-description">${escapeHtml(proposal.description)}</div>` : ''}
                <div class="proposal-votes">
                    <span class="vote-count for">For: ${votes.for_votes || 0}</span>
                    <span class="vote-count against">Against: ${votes.against_votes || 0}</span>
                    <span class="vote-count abstain">Abstain: ${votes.abstain_votes || 0}</span>
                </div>
                ${actionsHtml}
            </div>
        `;
    }));

    container.innerHTML = html.join('');
}

async function castVote(proposalId, choice) {
    try {
        await apiRequest('POST', `/gov/proposals/${proposalId}/vote`, {
            choice: choice,
        });

        // Reload proposals to show updated votes
        await loadProposals();

        // Show a brief success message (optional)
        console.log(`Vote cast: ${choice} on proposal ${proposalId}`);

    } catch (error) {
        alert(`Failed to cast vote: ${error.message}`);
    }
}

function escapeHtml(text) {
    if (!text) return '';
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// WebSocket Connection
function connectWebSocket() {
    // Close old connection without triggering its onclose handler side effects
    if (state.ws) {
        const oldWs = state.ws;
        state.ws = null; // Clear reference first
        oldWs.onclose = null; // Remove handler to prevent race condition
        oldWs.close();
    }

    // Convert HTTP URL to WebSocket URL
    const wsUrl = state.gatewayUrl
        .replace('http://', 'ws://')
        .replace('https://', 'wss://');

    const ws = new WebSocket(`${wsUrl}/v1/ws/${state.coopId}`);
    state.ws = ws;

    ws.onopen = () => {
        console.log('WebSocket connected');
        // Authenticate with token
        ws.send(JSON.stringify({
            type: 'Auth',
            token: state.token
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
        state.wsConnected = false;
        updateConnectionStatus(false);
    };

    ws.onclose = () => {
        console.log('WebSocket closed');
        state.wsConnected = false;
        // Only clear if this is still the active connection
        if (state.ws === ws) {
            state.ws = null;
        }

        // Attempt to reconnect after 5 seconds
        if (state.token && state.ws === null) {
            setTimeout(() => {
                if (state.token && state.ws === null) {
                    connectWebSocket();
                }
            }, 5000);
        }
    };
}

function handleWebSocketMessage(message) {
    switch (message.type) {
        case 'AuthOk':
            console.log('WebSocket authenticated:', message.did);
            state.wsConnected = true;
            updateConnectionStatus(true);
            break;

        case 'Error':
            console.error('WebSocket error:', message.message);
            break;

        case 'Event':
            handleWebSocketEvent(message);
            break;

        default:
            console.log('Unknown WebSocket message:', message);
    }
}

function handleWebSocketEvent(message) {
    // Handle different event types
    if (message.PaymentCreated) {
        console.log('Payment created:', message.PaymentCreated);
        // Reload transactions and balance
        loadTransactions();
        loadBalance();
        showNotification('New payment recorded');
    }

    if (message.MemberAdded) {
        console.log('Member added:', message.MemberAdded);
        // Reload members
        loadMembers();
        showNotification('New member joined');
    }

    if (message.MemberRemoved) {
        console.log('Member removed:', message.MemberRemoved);
        loadMembers();
    }

    if (message.GovernanceVoteCast) {
        console.log('Vote cast:', message.GovernanceVoteCast);
        // Reload proposals to show updated vote counts
        loadProposals();
        showNotification('New vote cast');
    }

    if (message.GovernanceProposalCreated) {
        console.log('Proposal created:', message.GovernanceProposalCreated);
        loadProposals();
        showNotification('New proposal created');
    }

    if (message.GovernanceProposalOpened) {
        console.log('Proposal opened:', message.GovernanceProposalOpened);
        loadProposals();
        showNotification('Proposal opened for voting');
    }

    if (message.GovernanceProposalClosed) {
        console.log('Proposal closed:', message.GovernanceProposalClosed);
        loadProposals();
        showNotification('Proposal closed');
    }
}

function showNotification(message) {
    // Update status bar briefly
    const originalStatus = elements.lastUpdate.textContent;
    elements.lastUpdate.textContent = message;
    elements.lastUpdate.style.color = '#16a34a';

    setTimeout(() => {
        elements.lastUpdate.textContent = `Updated: ${new Date().toLocaleTimeString()}`;
        elements.lastUpdate.style.color = '';
    }, 3000);
}

function disconnectWebSocket() {
    if (state.ws) {
        state.ws.close();
        state.ws = null;
    }
    state.wsConnected = false;
}

// Rendering
function renderRecentActivity(transactions) {
    if (transactions.length === 0) {
        elements.recentActivity.innerHTML = '<p class="loading">No recent activity</p>';
        return;
    }

    const html = transactions.map(tx => {
        const isReceived = tx.to === state.did;
        const other = isReceived ? tx.from : tx.to;
        const amountClass = isReceived ? 'received' : 'sent';
        const sign = isReceived ? '+' : '-';

        return `
            <div class="activity-item">
                <div class="activity-details">
                    <div class="activity-parties">
                        ${isReceived ? 'From' : 'To'} ${truncateDid(other)}
                    </div>
                    ${tx.memo ? `<div class="activity-memo">${tx.memo}</div>` : ''}
                </div>
                <div class="activity-amount ${amountClass}">
                    ${sign}${tx.amount} hrs
                </div>
            </div>
        `;
    }).join('');

    elements.recentActivity.innerHTML = html;
}

function renderTransactionList(transactions) {
    if (transactions.length === 0) {
        elements.transactionList.innerHTML = '<p class="loading">No transactions yet</p>';
        return;
    }

    const html = transactions.map(tx => {
        const isReceived = tx.to === state.did;

        return `
            <div class="transaction-item">
                <div class="transaction-info">
                    <div class="transaction-parties">
                        ${truncateDid(tx.from)} &rarr; ${truncateDid(tx.to)}
                    </div>
                    <div class="transaction-meta">
                        ${formatDateTime(tx.timestamp)}
                        ${tx.memo ? ` &bull; ${tx.memo}` : ''}
                    </div>
                </div>
                <div class="transaction-amount">
                    <div class="transaction-value ${isReceived ? 'positive' : ''}">${tx.amount}</div>
                    <div class="transaction-currency">${tx.currency}</div>
                </div>
            </div>
        `;
    }).join('');

    elements.transactionList.innerHTML = html;
}

function renderMemberList(members) {
    if (members.length === 0) {
        elements.memberList.innerHTML = '<p class="loading">No members</p>';
        return;
    }

    const html = members.map(member => {
        const roleClass = member.role === 'owner' ? 'owner' : member.role === 'admin' ? 'admin' : '';

        return `
            <div class="member-item">
                <div class="member-did">${truncateDid(member.did)}</div>
                <div class="member-role ${roleClass}">${member.role}</div>
            </div>
        `;
    }).join('');

    elements.memberList.innerHTML = html;
}

// Actions
async function logHours(event) {
    event.preventDefault();

    const recipient = elements.recipient.value;
    const hours = parseFloat(elements.hours.value);
    const memo = elements.memo.value.trim();

    if (!recipient || !hours) {
        showResult(elements.logResult, 'Please select a recipient and enter hours', false);
        return;
    }

    try {
        const payment = await apiRequest('POST', `/ledger/${state.coopId}/payment`, {
            from: recipient,  // They owe you
            to: state.did,    // You receive credit
            amount: hours,
            currency: 'hours',
            memo: memo || undefined,
        });

        showResult(
            elements.logResult,
            `Logged ${hours} hours! Transaction ID: ${payment.id.slice(0, 8)}...`,
            true
        );

        // Reset form
        elements.logHoursForm.reset();

        // Reload data
        await loadAllData();

    } catch (error) {
        showResult(elements.logResult, `Failed to log hours: ${error.message}`, false);
    }
}

// Tab Navigation
function switchTab(tabId) {
    // Update nav buttons
    elements.navBtns.forEach(btn => {
        btn.classList.toggle('active', btn.dataset.tab === tabId);
    });

    // Update content
    elements.tabContents.forEach(content => {
        content.classList.toggle('active', content.id === tabId);
    });
}

// Event Listeners
elements.loginBtn.addEventListener('click', login);
elements.logoutBtn.addEventListener('click', logout);
elements.logHoursForm.addEventListener('submit', logHours);

elements.navBtns.forEach(btn => {
    btn.addEventListener('click', () => switchTab(btn.dataset.tab));
});

// Enter key on login form
elements.token.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') login();
});

// Event delegation for vote buttons (prevents XSS from inline onclick)
document.addEventListener('click', (e) => {
    if (e.target.classList.contains('btn-vote')) {
        const proposalId = e.target.dataset.proposalId;
        const vote = e.target.dataset.vote;
        if (proposalId && vote) {
            castVote(proposalId, vote);
        }
    }
});

// Auto-refresh every 30 seconds
setInterval(async () => {
    if (state.token) {
        try {
            await loadAllData();
            updateConnectionStatus(true);
        } catch (error) {
            updateConnectionStatus(false);
        }
    }
}, 30000);

// Load saved credentials
document.addEventListener('DOMContentLoaded', () => {
    const savedGateway = localStorage.getItem('icn-gateway');
    const savedCoop = localStorage.getItem('icn-coop');
    const savedDid = localStorage.getItem('icn-did');
    const savedToken = localStorage.getItem('icn-token');

    if (savedGateway) elements.gatewayUrl.value = savedGateway;
    if (savedCoop) elements.coopId.value = savedCoop;
    if (savedDid) elements.did.value = savedDid;
    if (savedToken) elements.token.value = savedToken;

    // Auto-login if all fields are filled
    if (savedGateway && savedCoop && savedDid && savedToken) {
        login();
    }
});
