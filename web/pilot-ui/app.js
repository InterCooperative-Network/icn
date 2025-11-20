/**
 * ICN Pilot UI - Application Logic
 */

// State
const state = {
    gatewayUrl: '',
    coopId: '',
    did: '',
    token: '',
    tokenExpiry: null,  // Track when token expires
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
    tokenExpires: document.getElementById('token-expires'),

    // Modal
    authHelpModal: document.getElementById('auth-help-modal'),
    showAuthHelp: document.getElementById('show-auth-help'),
    closeAuthHelp: document.getElementById('close-auth-help'),
    helpGateway: document.getElementById('help-gateway'),
    helpCoop: document.getElementById('help-coop'),
    copyCommand: document.getElementById('copy-command'),

    // Toast
    toastContainer: document.getElementById('toast-container'),

    // Navigation
    navBtns: document.querySelectorAll('.nav-btn'),
    tabContents: document.querySelectorAll('.tab-content'),

    // Dashboard
    myBalance: document.getElementById('my-balance'),
    totalMembers: document.getElementById('total-members'),
    monthlyHours: document.getElementById('monthly-hours'),
    balanceChart: document.getElementById('balance-chart'),
    dashboardProposals: document.getElementById('dashboard-proposals'),
    topContributors: document.getElementById('top-contributors'),
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
    memberSearch: document.getElementById('member-search'),

    // History
    historyFilter: document.getElementById('history-filter'),
    transactionSort: document.getElementById('transaction-sort'),
    exportCsv: document.getElementById('export-csv'),

    // Governance
    proposalList: document.getElementById('proposal-list'),
    closedProposals: document.getElementById('closed-proposals'),

    // Footer
    connectionStatus: document.getElementById('connection-status'),
    lastUpdate: document.getElementById('last-update'),
};

// Toast Notification System
function showToast(message, type = 'info', duration = 5000) {
    const toast = document.createElement('div');
    toast.className = `toast ${type}`;

    const icons = {
        success: '✓',
        error: '✕',
        warning: '⚠',
        info: 'ℹ'
    };

    toast.innerHTML = `
        <span class="toast-icon">${icons[type] || icons.info}</span>
        <span class="toast-message">${message}</span>
        <button class="toast-close">&times;</button>
    `;

    elements.toastContainer.appendChild(toast);

    // Close button
    toast.querySelector('.toast-close').addEventListener('click', () => {
        toast.remove();
    });

    // Auto-remove after duration
    if (duration > 0) {
        setTimeout(() => {
            toast.remove();
        }, duration);
    }
}

// User-Friendly Error Messages
function getUserFriendlyError(error) {
    const message = error.message || String(error);

    // Network errors
    if (message.includes('Failed to fetch') || message.includes('NetworkError')) {
        return 'Cannot connect to the server. Please check your internet connection and gateway URL.';
    }

    // HTTP status errors
    if (message.includes('401') || message.includes('Unauthorized')) {
        return 'Your session has expired. Please sign in again.';
    }

    if (message.includes('403') || message.includes('Forbidden')) {
        return 'You don\'t have permission to do that. Check with your cooperative administrator.';
    }

    if (message.includes('404') || message.includes('Not Found')) {
        return 'The requested resource was not found. Please check your cooperative ID.';
    }

    if (message.includes('429') || message.includes('Too Many Requests')) {
        return 'Too many requests. Please wait a moment and try again.';
    }

    if (message.includes('500') || message.includes('Internal Server Error')) {
        return 'The server encountered an error. Please try again later or contact support.';
    }

    // Token expiration
    if (message.includes('token') && message.includes('expired')) {
        return 'Your authentication token has expired. Please get a new token and sign in again.';
    }

    // Return original message if we don't have a friendly version
    return message;
}

// API Client with Better Error Handling
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

    try {
        const response = await fetch(url, options);

        if (!response.ok) {
            // Handle auth errors specially
            if (response.status === 401) {
                // Token expired, force logout
                showToast('Your session has expired. Please sign in again.', 'warning');
                setTimeout(() => logout(), 2000);
                throw new Error('Session expired');
            }

            const error = await response.json().catch(() => ({}));
            throw new Error(error.error || `HTTP ${response.status}: ${response.statusText}`);
        }

        if (response.status === 204) {
            return null;
        }

        return response.json();
    } catch (error) {
        // Re-throw with user-friendly message
        error.userMessage = getUserFriendlyError(error);
        throw error;
    }
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

// Token Expiration Management
function updateTokenExpiry() {
    if (!state.tokenExpiry || !elements.tokenExpires) return;

    const now = Date.now();
    const timeLeft = state.tokenExpiry - now;

    if (timeLeft <= 0) {
        elements.tokenExpires.textContent = 'Token expired';
        elements.tokenExpires.className = 'token-info expired';
        showToast('Your authentication token has expired. Please sign in again.', 'error', 0);
        return;
    }

    const hours = Math.floor(timeLeft / (1000 * 60 * 60));
    const minutes = Math.floor((timeLeft % (1000 * 60 * 60)) / (1000 * 60));

    if (hours > 1) {
        elements.tokenExpires.textContent = `Token expires in ${hours}h`;
        elements.tokenExpires.className = 'token-info';
    } else if (hours === 1) {
        elements.tokenExpires.textContent = `Token expires in ${hours}h ${minutes}m`;
        elements.tokenExpires.className = 'token-info warning';
    } else if (minutes > 15) {
        elements.tokenExpires.textContent = `Token expires in ${minutes}m`;
        elements.tokenExpires.className = 'token-info warning';
    } else {
        elements.tokenExpires.textContent = `Token expires in ${minutes}m`;
        elements.tokenExpires.className = 'token-info expired';

        // Show warning toast if less than 15 minutes
        if (minutes === 15 || minutes === 10 || minutes === 5) {
            showToast(`Your token expires in ${minutes} minutes. Get a new token to avoid interruption.`, 'warning', 10000);
        }
    }
}

// Modal Functions
function showAuthHelpModal() {
    // Update the command with current values
    const gateway = elements.gatewayUrl.value.trim() || 'http://localhost:8080';
    const coop = elements.coopId.value.trim() || 'your-coop-id';

    elements.helpGateway.textContent = gateway;
    elements.helpCoop.textContent = coop;

    elements.authHelpModal.classList.remove('hidden');
}

function closeAuthHelpModal() {
    elements.authHelpModal.classList.add('hidden');
}

function copyAuthCommand() {
    const gateway = elements.helpGateway.textContent;
    const coop = elements.helpCoop.textContent;
    const command = `icnctl auth login --gateway ${gateway} --coop ${coop}`;

    navigator.clipboard.writeText(command).then(() => {
        const btn = elements.copyCommand;
        const originalText = btn.textContent;
        btn.textContent = 'Copied!';
        setTimeout(() => {
            btn.textContent = originalText;
        }, 2000);
        showToast('Command copied to clipboard', 'success', 3000);
    }).catch(() => {
        showToast('Failed to copy. Please select and copy manually.', 'error');
    });
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

        // Set token expiry (default 24 hours from now)
        state.tokenExpiry = Date.now() + (24 * 60 * 60 * 1000);

        // Save to localStorage
        localStorage.setItem('icn-gateway', state.gatewayUrl);
        localStorage.setItem('icn-coop', state.coopId);
        localStorage.setItem('icn-did', state.did);
        localStorage.setItem('icn-token', state.token);
        localStorage.setItem('icn-token-expiry', state.tokenExpiry.toString());

        // Show main screen
        elements.loginScreen.classList.add('hidden');
        elements.mainScreen.classList.remove('hidden');

        // Update header
        elements.coopName.textContent = state.coopId;
        elements.userDid.textContent = truncateDid(state.did);
        updateTokenExpiry();

        // Load data
        await loadAllData();
        updateConnectionStatus(true);

        // Connect WebSocket for real-time updates
        connectWebSocket();

        showToast('Connected successfully!', 'success', 3000);

    } catch (error) {
        const friendlyMessage = error.userMessage || error.message;
        showError(elements.loginError, friendlyMessage);
        showToast(friendlyMessage, 'error');
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
    localStorage.removeItem('icn-token-expiry');
    state.token = '';
    state.tokenExpiry = null;
    elements.mainScreen.classList.add('hidden');
    elements.loginScreen.classList.remove('hidden');
    showToast('Signed out successfully', 'info', 3000);
}

// Data Loading
async function loadAllData() {
    await Promise.all([
        loadBalance(),
        loadMembers(),
        loadTransactions(),
        loadProposals(),
    ]);

    // Render dashboard widgets after data is loaded
    renderBalanceChart();
    renderTopContributors();
    await renderDashboardProposals();

    elements.lastUpdate.textContent = `Updated: ${new Date().toLocaleTimeString()}`;
}

async function loadBalance() {
    try {
        const balance = await apiRequest(
            'GET',
            `/ledger/${state.coopId}/balance/${encodeURIComponent(state.did)}`
        );

        const value = balance.balance.toFixed(1);
        const trend = calculateBalanceTrend();
        const trendIcon = trend > 0 ? '↑' : trend < 0 ? '↓' : '→';
        const trendClass = trend > 0 ? 'trend-up' : trend < 0 ? 'trend-down' : 'trend-stable';

        elements.myBalance.innerHTML = `${value} <span class="balance-trend ${trendClass}">${trendIcon}</span>`;
        elements.myBalance.className = `stat-value ${balance.balance >= 0 ? 'positive' : 'negative'}`;

    } catch (error) {
        console.error('Failed to load balance:', error);
        elements.myBalance.textContent = '--';
    }
}

function calculateBalanceTrend() {
    if (state.transactions.length === 0) return 0;

    const now = Date.now() / 1000;
    const sevenDaysAgo = now - 7 * 24 * 60 * 60;
    const fourteenDaysAgo = now - 14 * 24 * 60 * 60;

    // Calculate net change for recent period (last 7 days)
    const recentChange = state.transactions
        .filter(tx => tx.timestamp >= sevenDaysAgo)
        .reduce((sum, tx) => {
            if (tx.to === state.did) return sum + tx.amount;
            if (tx.from === state.did) return sum - tx.amount;
            return sum;
        }, 0);

    // Calculate net change for previous period (7-14 days ago)
    const previousChange = state.transactions
        .filter(tx => tx.timestamp >= fourteenDaysAgo && tx.timestamp < sevenDaysAgo)
        .reduce((sum, tx) => {
            if (tx.to === state.did) return sum + tx.amount;
            if (tx.from === state.did) return sum - tx.amount;
            return sum;
        }, 0);

    // Compare periods
    const threshold = 1; // Minimum change to show trend
    if (Math.abs(recentChange - previousChange) < threshold) return 0; // Stable
    return recentChange > previousChange ? 1 : -1; // Up or down
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

        // Calculate deadline countdown if available
        let deadlineHtml = '';
        if (proposal.closes_at && showVoteButtons) {
            const closesAt = new Date(proposal.closes_at);
            const now = new Date();
            const timeLeft = closesAt - now;

            if (timeLeft > 0) {
                const days = Math.floor(timeLeft / (1000 * 60 * 60 * 24));
                const hours = Math.floor((timeLeft % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60));

                let countdownText;
                if (days > 0) {
                    countdownText = `Closes in ${days} day${days > 1 ? 's' : ''}`;
                } else if (hours > 0) {
                    countdownText = `Closes in ${hours} hour${hours > 1 ? 's' : ''}`;
                } else {
                    countdownText = 'Closes soon';
                }

                const urgencyClass = days === 0 ? 'urgent' : days <= 2 ? 'warning' : '';
                deadlineHtml = `<div class="proposal-deadline ${urgencyClass}">${countdownText}</div>`;
            }
        }

        return `
            <div class="proposal-item">
                <div class="proposal-header">
                    <div class="proposal-title">${escapeHtml(proposal.title)}</div>
                    <div class="proposal-state ${stateClass}">${proposal.state}</div>
                </div>
                ${proposal.description ? `<div class="proposal-description">${escapeHtml(proposal.description)}</div>` : ''}
                ${deadlineHtml}
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

        showToast(`Vote cast: ${choice}`, 'success', 3000);

    } catch (error) {
        const friendlyMessage = error.userMessage || error.message;
        showToast(`Failed to cast vote: ${friendlyMessage}`, 'error');
    }
}

function escapeHtml(text) {
    if (!text) return '';
    const div = document.createElement('div');
    div.textContent = text;
    // Also escape quotes for safe use in HTML attributes
    return div.innerHTML
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
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
    // Use toast notifications
    showToast(message, 'info', 3000);
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

// Balance Chart Visualization
function renderBalanceChart() {
    if (!elements.balanceChart || state.transactions.length === 0) {
        return;
    }

    const canvas = elements.balanceChart;
    const ctx = canvas.getContext('2d');
    const width = canvas.width;
    const height = canvas.height;
    const padding = 40;

    // Clear canvas
    ctx.clearRect(0, 0, width, height);

    // Calculate balance history (last 30 days)
    const now = Date.now() / 1000;
    const thirtyDaysAgo = now - 30 * 24 * 60 * 60;

    // Sort transactions by time
    const sortedTx = [...state.transactions]
        .filter(tx => tx.timestamp >= thirtyDaysAgo)
        .sort((a, b) => a.timestamp - b.timestamp);

    if (sortedTx.length === 0) {
        ctx.fillStyle = '#666';
        ctx.font = '14px sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText('No transactions in the last 30 days', width / 2, height / 2);
        return;
    }

    // Calculate cumulative balance over time
    let balance = 0;
    const points = sortedTx.map(tx => {
        if (tx.to === state.did) balance += tx.amount;
        if (tx.from === state.did) balance -= tx.amount;
        return { timestamp: tx.timestamp, balance };
    });

    // Add current point
    points.push({ timestamp: now, balance });

    // Find min/max for scaling
    const minBalance = Math.min(...points.map(p => p.balance), 0);
    const maxBalance = Math.max(...points.map(p => p.balance), 0);
    const minTime = points[0].timestamp;
    const maxTime = points[points.length - 1].timestamp;

    // Scale functions
    const scaleX = (timestamp) => padding + ((timestamp - minTime) / (maxTime - minTime)) * (width - 2 * padding);
    const scaleY = (balance) => {
        const range = maxBalance - minBalance || 1;
        return height - padding - ((balance - minBalance) / range) * (height - 2 * padding);
    };

    // Draw axes
    ctx.strokeStyle = '#ddd';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(padding, padding);
    ctx.lineTo(padding, height - padding);
    ctx.lineTo(width - padding, height - padding);
    ctx.stroke();

    // Draw zero line if applicable
    if (minBalance < 0 && maxBalance > 0) {
        ctx.strokeStyle = '#999';
        ctx.setLineDash([5, 5]);
        ctx.beginPath();
        ctx.moveTo(padding, scaleY(0));
        ctx.lineTo(width - padding, scaleY(0));
        ctx.stroke();
        ctx.setLineDash([]);
    }

    // Draw line chart
    ctx.strokeStyle = balance >= 0 ? '#10b981' : '#ef4444';
    ctx.lineWidth = 2;
    ctx.beginPath();
    points.forEach((point, i) => {
        const x = scaleX(point.timestamp);
        const y = scaleY(point.balance);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
    });
    ctx.stroke();

    // Draw data points
    ctx.fillStyle = balance >= 0 ? '#10b981' : '#ef4444';
    points.forEach(point => {
        const x = scaleX(point.timestamp);
        const y = scaleY(point.balance);
        ctx.beginPath();
        ctx.arc(x, y, 3, 0, 2 * Math.PI);
        ctx.fill();
    });

    // Draw labels
    ctx.fillStyle = '#666';
    ctx.font = '12px sans-serif';
    ctx.textAlign = 'center';

    // Y-axis labels
    ctx.textAlign = 'right';
    ctx.fillText(maxBalance.toFixed(1), padding - 5, padding + 5);
    ctx.fillText('0', padding - 5, scaleY(0) + 5);
    ctx.fillText(minBalance.toFixed(1), padding - 5, height - padding + 5);

    // X-axis labels
    ctx.textAlign = 'center';
    const startDate = new Date(minTime * 1000);
    const endDate = new Date(maxTime * 1000);
    ctx.fillText(startDate.toLocaleDateString(), padding, height - padding + 20);
    ctx.fillText(endDate.toLocaleDateString(), width - padding, height - padding + 20);

    // Current balance label
    ctx.textAlign = 'left';
    ctx.fillStyle = balance >= 0 ? '#10b981' : '#ef4444';
    ctx.font = 'bold 14px sans-serif';
    ctx.fillText(`Current: ${balance.toFixed(1)} hrs`, width - padding - 120, padding);
}

// Dashboard Proposals Widget
async function renderDashboardProposals() {
    if (!elements.dashboardProposals) return;

    try {
        const proposals = await apiRequest('GET', '/gov/proposals');
        const openProposals = proposals.filter(p => p.state === 'Open').slice(0, 3);

        if (openProposals.length === 0) {
            elements.dashboardProposals.innerHTML = '<p class="empty-state">No pending proposals</p>';
            return;
        }

        const html = openProposals.map(proposal => `
            <div class="proposal-summary-item">
                <div class="proposal-summary-title">${escapeHtml(proposal.title)}</div>
                <button class="btn btn-small" onclick="switchTab('governance')">Vote Now</button>
            </div>
        `).join('');

        elements.dashboardProposals.innerHTML = html;
    } catch (error) {
        elements.dashboardProposals.innerHTML = '<p class="empty-state">No proposals available</p>';
    }
}

// Top Contributors Display
function renderTopContributors() {
    if (!elements.topContributors || state.transactions.length === 0) {
        elements.topContributors.innerHTML = '<p class="empty-state">No activity yet</p>';
        return;
    }

    // Calculate contribution totals (hours given)
    const contributions = {};
    state.transactions.forEach(tx => {
        if (!contributions[tx.from]) contributions[tx.from] = 0;
        contributions[tx.from] += tx.amount;
    });

    // Sort and get top 5
    const topContributors = Object.entries(contributions)
        .sort((a, b) => b[1] - a[1])
        .slice(0, 5);

    if (topContributors.length === 0) {
        elements.topContributors.innerHTML = '<p class="empty-state">No activity yet</p>';
        return;
    }

    const html = topContributors.map(([did, hours], index) => {
        const rank = ['🥇', '🥈', '🥉', '4️⃣', '5️⃣'][index];
        return `
            <div class="contributor-item">
                <span class="contributor-rank">${rank}</span>
                <span class="contributor-did">${truncateDid(did)}</span>
                <span class="contributor-hours">${hours.toFixed(1)} hrs</span>
            </div>
        `;
    }).join('');

    elements.topContributors.innerHTML = html;
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
            <div class="member-item" data-did="${member.did}">
                <div class="member-info">
                    <div class="member-did" title="${member.did}">${truncateDid(member.did)}</div>
                    <button class="btn-copy-did" data-did="${member.did}" title="Copy full DID">📋</button>
                </div>
                <div class="member-role ${roleClass}">${member.role}</div>
            </div>
        `;
    }).join('');

    elements.memberList.innerHTML = html;
}

// Member Search
function filterMembers(searchTerm) {
    const memberItems = elements.memberList.querySelectorAll('.member-item');
    const term = searchTerm.toLowerCase();

    memberItems.forEach(item => {
        const did = item.dataset.did.toLowerCase();
        if (did.includes(term)) {
            item.style.display = '';
        } else {
            item.style.display = 'none';
        }
    });
}

// Transaction History Filtering
function filterTransactionsByDate(period) {
    updateTransactionDisplay();
}

function sortTransactions(sortBy) {
    updateTransactionDisplay();
}

function updateTransactionDisplay() {
    const filtered = getFilteredAndSortedTransactions();
    renderTransactionList(filtered);
}

// Helper to get filtered and sorted transactions
function getFilteredAndSortedTransactions() {
    const period = elements.historyFilter.value;
    const now = Date.now() / 1000;
    let startTime;

    switch (period) {
        case 'today':
            startTime = now - 24 * 60 * 60;
            break;
        case 'week':
            startTime = now - 7 * 24 * 60 * 60;
            break;
        case 'month':
            startTime = now - 30 * 24 * 60 * 60;
            break;
        case 'year':
            startTime = now - 365 * 24 * 60 * 60;
            break;
        default:
            startTime = 0; // all time
    }

    // Filter transactions
    let filtered = state.transactions.filter(tx => tx.timestamp >= startTime);

    // Sort transactions
    const sortBy = elements.transactionSort.value;
    filtered = [...filtered].sort((a, b) => {
        switch (sortBy) {
            case 'date-desc':
                return b.timestamp - a.timestamp;
            case 'date-asc':
                return a.timestamp - b.timestamp;
            case 'amount-desc':
                return b.amount - a.amount;
            case 'amount-asc':
                return a.amount - b.amount;
            default:
                return b.timestamp - a.timestamp;
        }
    });

    return filtered;
}

// CSV Export
function exportTransactionsToCSV() {
    if (state.transactions.length === 0) {
        showToast('No transactions to export', 'warning');
        return;
    }

    // Get currently filtered and sorted transactions
    const filtered = getFilteredAndSortedTransactions();

    // Create CSV content
    const headers = ['Date', 'Time', 'From', 'To', 'Amount', 'Currency', 'Memo'];
    const rows = filtered.map(tx => {
        const date = new Date(tx.timestamp * 1000);
        return [
            date.toLocaleDateString(),
            date.toLocaleTimeString(),
            tx.from,
            tx.to,
            tx.amount,
            tx.currency,
            (tx.memo || '').replace(/"/g, '""') // Escape quotes
        ];
    });

    const csvContent = [
        headers.join(','),
        ...rows.map(row => row.map(cell => `"${cell}"`).join(','))
    ].join('\n');

    // Download CSV
    const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' });
    const link = document.createElement('a');
    const url = URL.createObjectURL(blob);

    link.setAttribute('href', url);
    link.setAttribute('download', `transactions-${period}-${Date.now()}.csv`);
    link.style.visibility = 'hidden';
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);

    showToast(`Exported ${filtered.length} transactions`, 'success', 3000);
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

        showToast(`Successfully logged ${hours} hours`, 'success', 3000);

        // Reset form
        elements.logHoursForm.reset();

        // Reload data
        await loadAllData();

    } catch (error) {
        const friendlyMessage = error.userMessage || error.message;
        showResult(elements.logResult, `Failed: ${friendlyMessage}`, false);
        showToast(`Failed to log hours: ${friendlyMessage}`, 'error');
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

// Modal event listeners
elements.showAuthHelp.addEventListener('click', showAuthHelpModal);
elements.closeAuthHelp.addEventListener('click', closeAuthHelpModal);
elements.copyCommand.addEventListener('click', copyAuthCommand);

// Close modal when clicking outside
elements.authHelpModal.addEventListener('click', (e) => {
    if (e.target === elements.authHelpModal) {
        closeAuthHelpModal();
    }
});

// Member search
elements.memberSearch.addEventListener('input', (e) => {
    filterMembers(e.target.value);
});

// History filter
elements.historyFilter.addEventListener('change', (e) => {
    filterTransactionsByDate(e.target.value);
});

// Transaction sorting
elements.transactionSort.addEventListener('change', (e) => {
    sortTransactions(e.target.value);
});

// CSV export
elements.exportCsv.addEventListener('click', exportTransactionsToCSV);

// Keyboard shortcuts (Ctrl+1-5 for tab navigation)
document.addEventListener('keydown', (e) => {
    // Only when Ctrl/Cmd is pressed with number keys
    if ((e.ctrlKey || e.metaKey) && e.key >= '1' && e.key <= '5') {
        e.preventDefault();
        const tabs = ['dashboard', 'log-hours', 'history', 'members', 'governance'];
        const tabIndex = parseInt(e.key) - 1;
        if (tabs[tabIndex]) {
            switchTab(tabs[tabIndex]);
        }
    }
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

    // Copy DID button
    if (e.target.classList.contains('btn-copy-did')) {
        const did = e.target.dataset.did;
        if (did) {
            navigator.clipboard.writeText(did).then(() => {
                showToast('DID copied to clipboard!', 'success', 2000);
            }).catch(err => {
                console.error('Failed to copy DID:', err);
                showToast('Failed to copy DID', 'error');
            });
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

// Update token expiry display every minute
setInterval(() => {
    if (state.tokenExpiry) {
        updateTokenExpiry();
    }
}, 60000);

// Load saved credentials
document.addEventListener('DOMContentLoaded', () => {
    const savedGateway = localStorage.getItem('icn-gateway');
    const savedCoop = localStorage.getItem('icn-coop');
    const savedDid = localStorage.getItem('icn-did');
    const savedToken = localStorage.getItem('icn-token');
    const savedExpiry = localStorage.getItem('icn-token-expiry');

    if (savedGateway) elements.gatewayUrl.value = savedGateway;
    if (savedCoop) elements.coopId.value = savedCoop;
    if (savedDid) elements.did.value = savedDid;
    if (savedToken) elements.token.value = savedToken;

    // Restore token expiry
    if (savedExpiry) {
        state.tokenExpiry = parseInt(savedExpiry, 10);
    }

    // Auto-login if all fields are filled and token not expired
    if (savedGateway && savedCoop && savedDid && savedToken) {
        // Check if token is expired
        if (state.tokenExpiry && state.tokenExpiry < Date.now()) {
            showToast('Your saved token has expired. Please get a new token.', 'warning', 0);
            localStorage.removeItem('icn-token');
            localStorage.removeItem('icn-token-expiry');
        } else {
            login();
        }
    }
});
