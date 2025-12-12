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
    userRole: null,     // Track current user's role (owner, admin, member)
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
    joinScreen: document.getElementById('join-screen'),
    mainScreen: document.getElementById('main-screen'),

    // Login form
    gatewayUrl: document.getElementById('gateway-url'),
    coopId: document.getElementById('coop-id'),
    did: document.getElementById('did'),
    token: document.getElementById('token'),
    loginBtn: document.getElementById('login-btn'),
    loginError: document.getElementById('login-error'),
    logoutBtn: document.getElementById('logout-btn'),
    showJoinBtn: document.getElementById('show-join-btn'),
    backToLoginBtn: document.getElementById('back-to-login-btn'),

    // Join form
    joinGatewayUrl: document.getElementById('join-gateway-url'),
    inviteCode: document.getElementById('invite-code'),
    joinBtn: document.getElementById('join-btn'),
    joinError: document.getElementById('join-error'),
    joinSuccess: document.getElementById('join-success'),

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

// Clipboard helper with fallback for HTTP contexts
async function copyToClipboard(text) {
    // Try modern clipboard API first (works in HTTPS or localhost)
    if (navigator.clipboard && navigator.clipboard.writeText) {
        try {
            await navigator.clipboard.writeText(text);
            return true;
        } catch (err) {
            console.warn('Clipboard API failed, trying fallback:', err);
        }
    }

    // Fallback: create a temporary textarea
    try {
        const textarea = document.createElement('textarea');
        textarea.value = text;
        textarea.style.position = 'fixed';
        textarea.style.left = '-9999px';
        textarea.style.top = '-9999px';
        document.body.appendChild(textarea);
        textarea.focus();
        textarea.select();
        const success = document.execCommand('copy');
        document.body.removeChild(textarea);
        if (success) return true;
    } catch (err) {
        console.error('Fallback clipboard copy failed:', err);
    }

    return false;
}

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

async function copyAuthCommand() {
    const gateway = elements.helpGateway.textContent;
    const coop = elements.helpCoop.textContent;
    const command = `icnctl auth login --gateway ${gateway} --coop ${coop}`;

    const success = await copyToClipboard(command);
    if (success) {
        const btn = elements.copyCommand;
        const originalText = btn.textContent;
        btn.textContent = 'Copied!';
        setTimeout(() => {
            btn.textContent = originalText;
        }, 2000);
        showToast('Command copied to clipboard', 'success', 3000);
    } else {
        showToast('Failed to copy. Please select and copy manually.', 'error');
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
        const response = await apiRequest(
            'GET',
            `/ledger/${state.coopId}/balance/${encodeURIComponent(state.did)}`
        );

        // API returns { did: "...", balances: { "hours": 10, ... } }
        // Values are stored as whole hours (integers)
        const balances = response.balances || {};
        // Sum all currency balances
        const totalBalance = Object.values(balances).reduce((sum, val) => sum + (val || 0), 0);

        const value = totalBalance;
        const trend = calculateBalanceTrend();
        const trendIcon = trend > 0 ? '↑' : trend < 0 ? '↓' : '→';
        const trendClass = trend > 0 ? 'trend-up' : trend < 0 ? 'trend-down' : 'trend-stable';

        elements.myBalance.innerHTML = `${value} <span class="balance-trend ${trendClass}">${trendIcon}</span>`;
        elements.myBalance.className = `stat-value ${totalBalance >= 0 ? 'positive' : 'negative'}`;

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
        // Members are embedded in coop response, not a separate endpoint
        const coop = await apiRequest('GET', `/coops/${state.coopId}`);
        const members = coop.members || [];
        state.members = members;

        // Determine current user's role
        // API returns: Steward (owner), Facilitator (admin), Participant (member)
        const currentUser = members.find(m => m.did === state.did);
        const rawRole = currentUser?.role?.toLowerCase() || 'member';
        // Normalize role names
        if (rawRole === 'steward' || rawRole === 'owner') {
            state.userRole = 'owner';
        } else if (rawRole === 'facilitator' || rawRole === 'admin') {
            state.userRole = 'admin';
        } else {
            state.userRole = 'member';
        }

        // Update body class for admin-only elements
        document.body.classList.remove('is-owner', 'is-admin');
        if (state.userRole === 'owner') {
            document.body.classList.add('is-owner');
        } else if (state.userRole === 'admin') {
            document.body.classList.add('is-admin');
        }

        // Show/hide add member button
        const addMemberBtn = document.getElementById('add-member-btn');
        if (addMemberBtn) {
            if (state.userRole === 'owner' || state.userRole === 'admin') {
                addMemberBtn.classList.remove('hidden');
            } else {
                addMemberBtn.classList.add('hidden');
            }
        }

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
        // API returns array of entries with accounts array
        const rawEntries = Array.isArray(history) ? history : (history.transactions || []);

        // Transform API format to UI format
        // API: { id, timestamp, author, accounts: [{account_id, currency, debit, credit}] }
        // UI: { id, timestamp, from, to, amount, currency, memo }
        const transactions = rawEntries.map(entry => {
            // Find debit and credit accounts
            const debitAccount = entry.accounts?.find(a => a.debit != null);
            const creditAccount = entry.accounts?.find(a => a.credit != null);

            return {
                id: entry.id,
                timestamp: entry.timestamp / 1000, // Convert ms to seconds
                from: debitAccount?.account_id || entry.author,
                to: creditAccount?.account_id || '',
                amount: debitAccount?.debit || creditAccount?.credit || 0, // Whole hours
                currency: debitAccount?.currency || creditAccount?.currency || 'hours',
                memo: entry.memo || '',
            };
        });
        state.transactions = transactions;

        // Calculate monthly hours
        const oneMonthAgo = Date.now() / 1000 - 30 * 24 * 60 * 60;
        const monthlyTx = transactions.filter(tx => tx.timestamp > oneMonthAgo);
        const totalHours = monthlyTx.reduce((sum, tx) => sum + tx.amount, 0);
        elements.monthlyHours.textContent = totalHours;

        // Render lists
        renderRecentActivity(transactions.slice(0, 5));
        renderTransactionList(transactions);

    } catch (error) {
        console.error('Failed to load transactions:', error);
        elements.monthlyHours.textContent = '--';
    }
}

async function loadProposals() {
    try {
        // Try to load proposals from governance API
        const response = await apiRequest('GET', '/gov/proposals');
        // API returns { data: [...], pagination: {...} }
        const proposals = Array.isArray(response) ? response : (response.data || []);
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
        const response = await apiRequest('GET', '/gov/proposals');
        const proposals = Array.isArray(response) ? response : (response.data || []);
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
        // Handle both old (owner/admin) and new (Steward/Facilitator) role names
        const role = member.role?.toLowerCase() || '';
        const roleClass = (role === 'owner' || role === 'steward') ? 'owner' :
                          (role === 'admin' || role === 'facilitator') ? 'admin' : '';
        const displayRole = member.role || 'Member';

        return `
            <div class="member-item" data-did="${member.did}">
                <div class="member-info">
                    <div class="member-did" title="${member.did}">${truncateDid(member.did)}</div>
                    <button class="btn-copy-did" data-did="${member.did}" title="Copy full DID">📋</button>
                </div>
                <div class="member-role ${roleClass}">${displayRole}</div>
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
        // Convert hours to integer (API expects i64, whole hours)
        const amountInt = Math.round(hours);

        const payment = await apiRequest('POST', `/ledger/${state.coopId}/payment`, {
            from: state.did,   // You send from your account
            to: recipient,     // To the person who helped you
            amount: amountInt,
            currency: 'hours',
            memo: memo || undefined,
        });

        showResult(
            elements.logResult,
            `Sent ${hours} hours! Transaction ID: ${payment.hash?.slice(0, 8) || 'complete'}...`,
            true
        );

        showToast(`Successfully sent ${hours} hours`, 'success', 3000);

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
            copyToClipboard(did).then(success => {
                if (success) {
                    showToast('DID copied to clipboard!', 'success', 2000);
                } else {
                    showToast('Failed to copy DID', 'error');
                }
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

    // Initialize theme (dark mode)
    initializeTheme();

    // Initialize accessibility features
    initializeAccessibility();
});

// ============================================================================
// Dark Mode / Theme Toggle
// ============================================================================

function initializeTheme() {
    const themeToggle = document.getElementById('theme-toggle');
    const themeIcon = themeToggle.querySelector('.theme-icon');

    // Check for saved theme preference or default to light mode
    const savedTheme = localStorage.getItem('icn-theme') || 'light';
    const prefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
    const theme = savedTheme === 'auto' ? (prefersDark ? 'dark' : 'light') : savedTheme;

    // Apply theme
    applyTheme(theme);

    // Theme toggle click
    themeToggle.addEventListener('click', () => {
        const currentTheme = document.documentElement.getAttribute('data-theme') || 'light';
        const newTheme = currentTheme === 'light' ? 'dark' : 'light';
        applyTheme(newTheme);
        localStorage.setItem('icn-theme', newTheme);

        // Announce to screen readers
        announceToScreenReader(`Switched to ${newTheme} mode`);
    });

    // Listen for OS theme changes
    if (window.matchMedia) {
        window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
            if (localStorage.getItem('icn-theme') === 'auto') {
                applyTheme(e.matches ? 'dark' : 'light');
            }
        });
    }
}

function applyTheme(theme) {
    document.documentElement.setAttribute('data-theme', theme);
    const themeIcon = document.querySelector('.theme-icon');
    if (themeIcon) {
        themeIcon.textContent = theme === 'dark' ? '☀️' : '🌙';
    }

    // Update meta theme-color for mobile browsers
    const metaThemeColor = document.querySelector('meta[name="theme-color"]');
    if (metaThemeColor) {
        metaThemeColor.setAttribute('content', theme === 'dark' ? '#1f2937' : '#2563eb');
    }
}

// ============================================================================
// Accessibility Enhancements
// ============================================================================

function initializeAccessibility() {
    // Screen reader announcement helper
    window.announceToScreenReader = function(message, priority = 'polite') {
        const announcer = document.getElementById('sr-announcements');
        if (announcer) {
            announcer.setAttribute('aria-live', priority);
            announcer.textContent = message;

            // Clear after announcement
            setTimeout(() => {
                announcer.textContent = '';
            }, 1000);
        }
    };

    // Modal focus trap
    setupModalFocusTrap();

    // Enhanced keyboard navigation for tabs
    setupTabKeyboardNav();

    // Form submission via Enter key
    setupFormSubmission();

    // Announce page changes
    announcePageChanges();
}

// Modal Focus Trap
function setupModalFocusTrap() {
    const modal = elements.authHelpModal;
    const openBtn = elements.showAuthHelp;
    const closeBtn = elements.closeAuthHelp;

    openBtn.addEventListener('click', () => {
        modal.classList.remove('hidden');
        modal.setAttribute('aria-hidden', 'false');
        closeBtn.focus(); // Focus close button for easy escape

        // Announce modal opened
        announceToScreenReader('Authentication help dialog opened');

        // Trap focus
        trapFocus(modal);
    });

    closeBtn.addEventListener('click', () => {
        closeModal();
    });

    // Close on Escape key
    modal.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') {
            closeModal();
        }
    });

    // Close on backdrop click
    modal.addEventListener('click', (e) => {
        if (e.target === modal) {
            closeModal();
        }
    });

    function closeModal() {
        modal.classList.add('hidden');
        modal.setAttribute('aria-hidden', 'true');
        openBtn.focus(); // Return focus to trigger
        announceToScreenReader('Dialog closed');
    }
}

// Trap focus within modal
function trapFocus(element) {
    const focusableElements = element.querySelectorAll(
        'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])'
    );

    const firstFocusable = focusableElements[0];
    const lastFocusable = focusableElements[focusableElements.length - 1];

    element.addEventListener('keydown', function(e) {
        if (e.key === 'Tab') {
            if (e.shiftKey) {
                // Shift + Tab
                if (document.activeElement === firstFocusable) {
                    lastFocusable.focus();
                    e.preventDefault();
                }
            } else {
                // Tab
                if (document.activeElement === lastFocusable) {
                    firstFocusable.focus();
                    e.preventDefault();
                }
            }
        }
    });
}

// Tab Navigation with Arrow Keys
function setupTabKeyboardNav() {
    const tabButtons = Array.from(elements.navBtns);

    tabButtons.forEach((button, index) => {
        button.addEventListener('keydown', (e) => {
            let newIndex;

            switch (e.key) {
                case 'ArrowLeft':
                case 'ArrowUp':
                    e.preventDefault();
                    newIndex = index === 0 ? tabButtons.length - 1 : index - 1;
                    tabButtons[newIndex].click();
                    tabButtons[newIndex].focus();
                    break;

                case 'ArrowRight':
                case 'ArrowDown':
                    e.preventDefault();
                    newIndex = index === tabButtons.length - 1 ? 0 : index + 1;
                    tabButtons[newIndex].click();
                    tabButtons[newIndex].focus();
                    break;

                case 'Home':
                    e.preventDefault();
                    tabButtons[0].click();
                    tabButtons[0].focus();
                    break;

                case 'End':
                    e.preventDefault();
                    tabButtons[tabButtons.length - 1].click();
                    tabButtons[tabButtons.length - 1].focus();
                    break;
            }
        });
    });
}

// Form Submission via Enter
function setupFormSubmission() {
    const loginForm = document.getElementById('login-form');
    if (loginForm) {
        loginForm.addEventListener('submit', (e) => {
            e.preventDefault();
            login();
        });
    }
}

// Announce Tab Changes to Screen Readers
function announcePageChanges() {
    elements.navBtns.forEach(btn => {
        const originalClickHandler = btn.onclick;
        btn.addEventListener('click', () => {
            const tabName = btn.textContent.trim();
            announceToScreenReader(`Navigated to ${tabName} tab`);

            // Update aria-current
            elements.navBtns.forEach(b => b.removeAttribute('aria-current'));
            btn.setAttribute('aria-current', 'page');
        });
    });
}

// ============================================================================
// Progressive Web App (PWA) - Service Worker Registration
// ============================================================================

if ('serviceWorker' in navigator) {
    window.addEventListener('load', () => {
        navigator.serviceWorker.register('/sw.js')
            .then((registration) => {
                console.log('[PWA] Service Worker registered successfully:', registration.scope);

                // Check for updates periodically
                setInterval(() => {
                    registration.update();
                }, 60000); // Check every minute

                // Handle updates
                registration.addEventListener('updatefound', () => {
                    const newWorker = registration.installing;

                    newWorker.addEventListener('statechange', () => {
                        if (newWorker.state === 'installed' && navigator.serviceWorker.controller) {
                            // New service worker available, prompt user to reload
                            showUpdateNotification();
                        }
                    });
                });
            })
            .catch((error) => {
                console.error('[PWA] Service Worker registration failed:', error);
            });

        // Listen for controller change (new service worker activated)
        navigator.serviceWorker.addEventListener('controllerchange', () => {
            console.log('[PWA] New service worker activated');
        });
    });
}

// Show update notification when new version available
function showUpdateNotification() {
    const updateMessage = 'A new version of ICN Timebank is available!';

    // Use toast notification
    const toast = document.createElement('div');
    toast.className = 'toast info update-toast';
    toast.innerHTML = `
        <span class="toast-icon">🔄</span>
        <span class="toast-message">${updateMessage}</span>
        <button class="btn btn-small" onclick="window.location.reload()">Update Now</button>
    `;

    const container = document.getElementById('toast-container');
    container.appendChild(toast);

    // Don't auto-dismiss update notifications
}

// Install prompt for PWA
let deferredPrompt;

window.addEventListener('beforeinstallprompt', (e) => {
    // Prevent the mini-infobar from appearing on mobile
    e.preventDefault();

    // Stash the event so it can be triggered later
    deferredPrompt = e;

    // Show custom install button/prompt
    showInstallPromotion();
});

function showInstallPromotion() {
    // Show a custom install promotion (could be a toast or banner)
    const installMessage = 'Install ICN Timebank for quick access and offline support!';

    showToast(installMessage, 'info', 10000);

    // Note: A proper implementation would show an install button
    // that calls deferredPrompt.prompt()
}

// Track install
window.addEventListener('appinstalled', () => {
    console.log('[PWA] App installed successfully');
    deferredPrompt = null;

    showToast('ICN Timebank installed! You can now use it offline.', 'success', 5000);
});

// ============================================================================
// Phase 8: Advanced Features - Economic Health Dashboard
// ============================================================================

// Toggle advanced reports visibility
const toggleAdvancedReports = document.getElementById('toggle-advanced-reports');
const advancedReports = document.getElementById('advanced-reports');

if (toggleAdvancedReports && advancedReports) {
    toggleAdvancedReports.addEventListener('click', () => {
        const isHidden = advancedReports.classList.contains('hidden');
        advancedReports.classList.toggle('hidden');
        toggleAdvancedReports.textContent = isHidden ? 'Hide Details' : 'Show Details';

        if (isHidden) {
            // Load data when showing
            loadEconomicMetrics();
            renderVelocityChart();
            renderParticipationHeatmap();
        }
    });
}

// Calculate and display economic health metrics
function loadEconomicMetrics() {
    if (!state.transactions || state.transactions.length === 0) return;

    // Calculate transaction velocity (transactions per member per month)
    const thirtyDaysAgo = Math.floor(Date.now() / 1000) - (30 * 24 * 60 * 60);
    const recentTransactions = state.transactions.filter(tx => tx.timestamp >= thirtyDaysAgo);
    const activeMemberCount = new Set(
        recentTransactions.flatMap(tx => [tx.from, tx.to])
    ).size || 1;
    const velocity = (recentTransactions.length / activeMemberCount).toFixed(2);

    // Calculate participation rate (% of members with at least 1 transaction in 30 days)
    const totalMembers = state.members.length || 1;
    const participation = ((activeMemberCount / totalMembers) * 100).toFixed(1);

    // Calculate Gini coefficient (economic inequality)
    const balances = state.members.map(m => m.balance || 0).sort((a, b) => a - b);
    const gini = calculateGini(balances);

    // Calculate hoarding index (% with balance > 2x average)
    const avgBalance = balances.reduce((sum, b) => sum + b, 0) / balances.length;
    const hoarders = balances.filter(b => b > avgBalance * 2).length;
    const hoarding = ((hoarders / totalMembers) * 100).toFixed(1);

    // Update display
    document.getElementById('metric-velocity').textContent = velocity;
    document.getElementById('metric-participation').textContent = `${participation}%`;
    document.getElementById('metric-gini').textContent = gini.toFixed(3);
    document.getElementById('metric-hoarding').textContent = `${hoarding}%`;
}

// Calculate Gini coefficient
function calculateGini(values) {
    if (values.length === 0) return 0;

    const n = values.length;
    let numerator = 0;

    for (let i = 0; i < n; i++) {
        for (let j = 0; j < n; j++) {
            numerator += Math.abs(values[i] - values[j]);
        }
    }

    const mean = values.reduce((sum, v) => sum + v, 0) / n;
    const denominator = 2 * n * n * mean;

    return denominator === 0 ? 0 : numerator / denominator;
}

// Render velocity chart (6-month trend)
function renderVelocityChart() {
    const canvas = document.getElementById('velocity-chart');
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    const width = canvas.width;
    const height = canvas.height;

    // Clear canvas
    ctx.clearRect(0, 0, width, height);

    // Generate sample data (6 months of velocity)
    const months = ['6mo', '5mo', '4mo', '3mo', '2mo', '1mo', 'Now'];
    const velocityData = months.map((month, i) => {
        // Simulate increasing velocity trend
        const baseVelocity = 1.5 + (i * 0.3);
        return baseVelocity + (Math.random() * 0.4 - 0.2);
    });

    // Chart dimensions
    const padding = 40;
    const chartWidth = width - (padding * 2);
    const chartHeight = height - (padding * 2);
    const maxVelocity = Math.max(...velocityData) * 1.2;

    // Draw axes
    ctx.strokeStyle = '#9ca3af';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(padding, padding);
    ctx.lineTo(padding, height - padding);
    ctx.lineTo(width - padding, height - padding);
    ctx.stroke();

    // Draw velocity line
    ctx.strokeStyle = '#2563eb';
    ctx.lineWidth = 3;
    ctx.beginPath();

    velocityData.forEach((velocity, i) => {
        const x = padding + (chartWidth / (months.length - 1)) * i;
        const y = (height - padding) - ((velocity / maxVelocity) * chartHeight);

        if (i === 0) {
            ctx.moveTo(x, y);
        } else {
            ctx.lineTo(x, y);
        }

        // Draw data points
        ctx.fillStyle = '#2563eb';
        ctx.beginPath();
        ctx.arc(x, y, 5, 0, Math.PI * 2);
        ctx.fill();
    });

    ctx.stroke();

    // Draw labels
    ctx.fillStyle = '#4b5563';
    ctx.font = '12px sans-serif';
    ctx.textAlign = 'center';

    months.forEach((month, i) => {
        const x = padding + (chartWidth / (months.length - 1)) * i;
        ctx.fillText(month, x, height - padding + 20);
    });

    // Y-axis label
    ctx.textAlign = 'right';
    ctx.fillText('Velocity', padding - 10, padding - 10);

    // Title
    ctx.font = 'bold 14px sans-serif';
    ctx.textAlign = 'center';
    ctx.fillStyle = '#1f2937';
    ctx.fillText('Transaction Velocity Trend (6 months)', width / 2, 20);
}

// Render participation heatmap (last 12 weeks)
function renderParticipationHeatmap() {
    const container = document.getElementById('participation-heatmap');
    if (!container) return;

    container.innerHTML = '';

    // Generate 12 weeks of data (7 days each = 84 cells)
    for (let week = 0; week < 12; week++) {
        for (let day = 0; day < 7; day++) {
            const cell = document.createElement('div');
            cell.className = 'heatmap-cell';

            // Simulate participation level (0-4)
            const level = Math.floor(Math.random() * 5);
            cell.setAttribute('data-level', level);

            // Add tooltip
            const weekDate = new Date();
            weekDate.setDate(weekDate.getDate() - ((11 - week) * 7 + (6 - day)));
            const dateStr = weekDate.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });

            const tooltip = document.createElement('div');
            tooltip.className = 'heatmap-tooltip';
            tooltip.textContent = `${dateStr}: ${level * 2} transactions`;
            cell.appendChild(tooltip);

            container.appendChild(cell);
        }
    }
}

// ============================================================================
// Phase 8: Member Profile
// ============================================================================

// Profile data (stored in localStorage)
let profileData = {
    bio: '',
    skills: [],
    availability: {
        monday: '9am-5pm',
        tuesday: '9am-5pm',
        wednesday: '9am-5pm',
        thursday: '9am-5pm',
        friday: '9am-5pm',
        saturday: 'Not available',
        sunday: 'Not available'
    },
    contact: {
        email: '',
        phone: '',
        location: ''
    },
    serviceHistory: {
        given: [],
        received: []
    }
};

// Load profile when Profile tab is opened
document.addEventListener('click', (e) => {
    if (e.target.dataset.tab === 'member-profile') {
        loadProfile();
    }
});

// Load profile from localStorage
function loadProfile() {
    const saved = localStorage.getItem('icn-profile');
    if (saved) {
        profileData = { ...profileData, ...JSON.parse(saved) };
    }

    // Update UI
    document.getElementById('profile-name').textContent = state.did ? 'Member Profile' : 'Your Profile';
    document.getElementById('profile-did').textContent = truncateDid(state.did || 'did:icn:...');

    // Update initials
    const initials = state.did ? state.did.slice(8, 10).toUpperCase() : '?';
    document.getElementById('profile-initials').textContent = initials;

    // Load stats
    loadProfileStats();

    // Load bio
    document.getElementById('profile-bio').textContent = profileData.bio || 'No bio added yet. Click Edit Profile to add one.';

    // Load skills
    renderSkills();

    // Load availability
    renderAvailability();

    // Load contact
    renderContact();

    // Load service history
    renderServiceHistory();
}

// Load profile statistics
function loadProfileStats() {
    const myTransactions = state.transactions.filter(tx =>
        tx.from === state.did || tx.to === state.did
    );

    const hoursGiven = myTransactions
        .filter(tx => tx.from === state.did)
        .reduce((sum, tx) => sum + tx.amount, 0);

    const hoursReceived = myTransactions
        .filter(tx => tx.to === state.did)
        .reduce((sum, tx) => sum + tx.amount, 0);

    const totalHours = hoursGiven + hoursReceived;

    document.getElementById('profile-hours-given').textContent = hoursGiven.toFixed(1);
    document.getElementById('profile-hours-received').textContent = hoursReceived.toFixed(1);
    document.getElementById('profile-total-hours').textContent = totalHours.toFixed(1);
}

// Render skills list
function renderSkills() {
    const container = document.getElementById('skills-list');
    container.innerHTML = '';

    if (profileData.skills.length === 0) {
        container.innerHTML = '<p style="color: var(--text-tertiary); font-style: italic;">No skills added yet.</p>';
        return;
    }

    profileData.skills.forEach(skill => {
        const tag = document.createElement('span');
        tag.className = 'skill-tag';
        tag.textContent = skill;
        container.appendChild(tag);
    });
}

// Render availability
function renderAvailability() {
    const container = document.getElementById('availability-display');
    container.innerHTML = '';

    const days = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];
    const grid = document.createElement('div');
    grid.className = 'availability-grid';

    days.forEach(day => {
        const dayDiv = document.createElement('div');
        dayDiv.className = 'availability-day';

        const dayKey = day.toLowerCase();
        const time = profileData.availability[dayKey] || 'Not set';

        dayDiv.innerHTML = `
            <strong>${day}</strong>
            <span class="availability-time">${time}</span>
        `;

        grid.appendChild(dayDiv);
    });

    container.appendChild(grid);
}

// Render contact info
function renderContact() {
    const email = profileData.contact.email || 'Not provided';
    const phone = profileData.contact.phone || 'Not provided';
    const location = profileData.contact.location || 'Not provided';

    document.getElementById('contact-email').textContent = email;
    document.getElementById('contact-phone').textContent = phone;
    document.getElementById('contact-location').textContent = location;
}

// Render service history
function renderServiceHistory() {
    const givenContainer = document.getElementById('services-given');
    const receivedContainer = document.getElementById('services-received');

    givenContainer.innerHTML = '';
    receivedContainer.innerHTML = '';

    // Get transactions
    const given = state.transactions.filter(tx => tx.from === state.did).slice(0, 10);
    const received = state.transactions.filter(tx => tx.to === state.did).slice(0, 10);

    if (given.length === 0) {
        givenContainer.innerHTML = '<p class="empty-state">No services given yet</p>';
    } else {
        given.forEach(tx => {
            givenContainer.appendChild(createServiceHistoryItem(tx, 'given'));
        });
    }

    if (received.length === 0) {
        receivedContainer.innerHTML = '<p class="empty-state">No services received yet</p>';
    } else {
        received.forEach(tx => {
            receivedContainer.appendChild(createServiceHistoryItem(tx, 'received'));
        });
    }
}

// Create service history item
function createServiceHistoryItem(tx, type) {
    const item = document.createElement('div');
    item.className = 'service-history-item';

    const peer = type === 'given' ? tx.to : tx.from;
    const description = tx.memo || 'Service exchange';

    item.innerHTML = `
        <div class="service-history-details">
            <div class="service-history-description">${description}</div>
            <div class="service-history-date">${formatDate(tx.timestamp)} • ${truncateDid(peer)}</div>
        </div>
        <div class="service-history-hours">${tx.amount}h</div>
    `;

    return item;
}

// Service history tabs
const serviceTabBtns = document.querySelectorAll('.service-tab-btn');
const serviceTabContents = document.querySelectorAll('.service-history-content');

serviceTabBtns.forEach(btn => {
    btn.addEventListener('click', () => {
        const tabId = btn.dataset.tab;

        serviceTabBtns.forEach(b => b.classList.remove('active'));
        btn.classList.add('active');

        serviceTabContents.forEach(content => {
            content.classList.toggle('hidden', content.id !== tabId);
        });
    });
});

// Edit Profile Modal
const editProfileBtn = document.getElementById('edit-profile-btn');
const editProfileModal = document.getElementById('edit-profile-modal');
const closeEditProfile = document.getElementById('close-edit-profile');
const saveProfileBtn = document.getElementById('save-profile-btn');

if (editProfileBtn) {
    editProfileBtn.addEventListener('click', () => {
        // Populate form
        document.getElementById('edit-bio').value = profileData.bio;
        document.getElementById('edit-email').value = profileData.contact.email;
        document.getElementById('edit-phone').value = profileData.contact.phone;
        document.getElementById('edit-location').value = profileData.contact.location;

        editProfileModal.classList.remove('hidden');
    });
}

if (closeEditProfile) {
    closeEditProfile.addEventListener('click', () => {
        editProfileModal.classList.add('hidden');
    });
}

if (saveProfileBtn) {
    saveProfileBtn.addEventListener('click', () => {
        // Save profile data
        profileData.bio = document.getElementById('edit-bio').value;
        profileData.contact.email = document.getElementById('edit-email').value;
        profileData.contact.phone = document.getElementById('edit-phone').value;
        profileData.contact.location = document.getElementById('edit-location').value;

        // Save to localStorage
        localStorage.setItem('icn-profile', JSON.stringify(profileData));

        // Reload profile display
        loadProfile();

        // Close modal
        editProfileModal.classList.add('hidden');

        showToast('Profile updated successfully!', 'success');
    });
}

// ============================================================================
// Phase 8: Service Request Board
// ============================================================================

// Service listings data (stored in localStorage)
let serviceListings = [];

// Load service board when tab is opened
document.addEventListener('click', (e) => {
    if (e.target.dataset.tab === 'service-board') {
        loadServiceBoard();
    }
});

// Load service listings from localStorage
function loadServiceBoard() {
    const saved = localStorage.getItem('icn-services');
    if (saved) {
        serviceListings = JSON.parse(saved);
    } else {
        // Add sample data if empty
        serviceListings = [
            {
                id: '1',
                type: 'offer',
                category: 'Education',
                title: 'Math Tutoring',
                description: 'Offering math tutoring for high school students. Algebra, geometry, and calculus.',
                poster: state.did || 'did:icn:sample',
                date: Math.floor(Date.now() / 1000)
            },
            {
                id: '2',
                type: 'request',
                category: 'Home',
                title: 'Help Moving Furniture',
                description: 'Need help moving furniture to a new apartment this weekend.',
                poster: 'did:icn:sample2',
                date: Math.floor(Date.now() / 1000) - 86400
            }
        ];
    }

    renderServiceListings();
}

// Render service listings
function renderServiceListings(filterType = 'all', filterCategory = 'all', searchQuery = '') {
    const container = document.getElementById('service-listings');
    container.innerHTML = '';

    let filtered = serviceListings;

    // Apply filters
    if (filterType !== 'all') {
        filtered = filtered.filter(s => s.type === filterType);
    }

    if (filterCategory !== 'all') {
        filtered = filtered.filter(s => s.category === filterCategory);
    }

    if (searchQuery) {
        const query = searchQuery.toLowerCase();
        filtered = filtered.filter(s =>
            s.title.toLowerCase().includes(query) ||
            s.description.toLowerCase().includes(query)
        );
    }

    if (filtered.length === 0) {
        container.innerHTML = '<p class="empty-state">No service listings found</p>';
        return;
    }

    filtered.forEach(service => {
        container.appendChild(createServiceListing(service));
    });
}

// Create service listing card
function createServiceListing(service) {
    const card = document.createElement('div');
    card.className = 'service-listing';

    card.innerHTML = `
        <div class="service-listing-header">
            <span class="service-type-badge ${service.type}">${service.type}</span>
            <span class="service-category">${service.category}</span>
        </div>
        <h3 class="service-listing-title">${service.title}</h3>
        <p class="service-listing-description">${service.description}</p>
        <div class="service-listing-meta">
            <div class="service-listing-poster">
                Posted by <strong>${truncateDid(service.poster)}</strong>
            </div>
            <div class="service-listing-date">${formatDate(service.date)}</div>
        </div>
        <div class="service-listing-actions">
            <button class="btn-respond">Respond</button>
        </div>
    `;

    return card;
}

// Service filters
const serviceTypeFilter = document.getElementById('service-type-filter');
const serviceCategoryFilter = document.getElementById('service-category-filter');
const serviceSearch = document.getElementById('service-search');

if (serviceTypeFilter) {
    serviceTypeFilter.addEventListener('change', (e) => {
        const type = e.target.value;
        const category = serviceCategoryFilter?.value || 'all';
        const search = serviceSearch?.value || '';
        renderServiceListings(type, category, search);
    });
}

if (serviceCategoryFilter) {
    serviceCategoryFilter.addEventListener('change', (e) => {
        const type = serviceTypeFilter?.value || 'all';
        const category = e.target.value;
        const search = serviceSearch?.value || '';
        renderServiceListings(type, category, search);
    });
}

if (serviceSearch) {
    serviceSearch.addEventListener('input', (e) => {
        const type = serviceTypeFilter?.value || 'all';
        const category = serviceCategoryFilter?.value || 'all';
        const search = e.target.value;
        renderServiceListings(type, category, search);
    });
}

// Post Service Modal
const postServiceBtn = document.getElementById('post-service-btn');
const postServiceModal = document.getElementById('post-service-modal');
const closePostService = document.getElementById('close-post-service');
const submitServiceBtn = document.getElementById('submit-service-btn');

if (postServiceBtn) {
    postServiceBtn.addEventListener('click', () => {
        postServiceModal.classList.remove('hidden');
    });
}

if (closePostService) {
    closePostService.addEventListener('click', () => {
        postServiceModal.classList.add('hidden');
    });
}

if (submitServiceBtn) {
    submitServiceBtn.addEventListener('click', () => {
        const type = document.querySelector('input[name="service-type"]:checked')?.value;
        const category = document.getElementById('service-category').value;
        const title = document.getElementById('service-title').value;
        const description = document.getElementById('service-description').value;

        if (!type || !category || !title || !description) {
            showToast('Please fill in all fields', 'error');
            return;
        }

        // Create new listing
        const newListing = {
            id: Date.now().toString(),
            type,
            category,
            title,
            description,
            poster: state.did || 'did:icn:unknown',
            date: Math.floor(Date.now() / 1000)
        };

        serviceListings.unshift(newListing);

        // Save to localStorage
        localStorage.setItem('icn-services', JSON.stringify(serviceListings));

        // Reload board
        renderServiceListings();

        // Close modal and reset form
        postServiceModal.classList.add('hidden');
        document.getElementById('service-title').value = '';
        document.getElementById('service-description').value = '';

        showToast('Service posted successfully!', 'success');
    });
}

// ============================================================================
// Phase 9: Notification Center
// ============================================================================

// Notification state
let notifications = [];
let unreadCount = 0;

// Notification types with icons
const notificationIcons = {
    success: '✓',
    warning: '⚠',
    error: '✕',
    info: 'ℹ',
    payment: '💰',
    proposal: '📋',
    member: '👤',
    service: '🔧',
};

// Initialize notification center
function initializeNotificationCenter() {
    const bellBtn = document.getElementById('notification-bell');
    const panel = document.getElementById('notification-panel');
    const markAllRead = document.getElementById('mark-all-read');

    if (!bellBtn || !panel) return;

    // Load saved notifications from localStorage
    const saved = localStorage.getItem('icn-notifications');
    if (saved) {
        notifications = JSON.parse(saved);
        unreadCount = notifications.filter(n => !n.read).length;
        updateNotificationBadge();
        renderNotifications();
    }

    // Toggle notification panel
    bellBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        const isOpen = !panel.classList.contains('hidden');

        if (isOpen) {
            panel.classList.add('hidden');
            bellBtn.setAttribute('aria-expanded', 'false');
        } else {
            panel.classList.remove('hidden');
            bellBtn.setAttribute('aria-expanded', 'true');
        }
    });

    // Close panel when clicking outside
    document.addEventListener('click', (e) => {
        if (!panel.contains(e.target) && !bellBtn.contains(e.target)) {
            panel.classList.add('hidden');
            bellBtn.setAttribute('aria-expanded', 'false');
        }
    });

    // Mark all as read
    if (markAllRead) {
        markAllRead.addEventListener('click', () => {
            notifications.forEach(n => n.read = true);
            unreadCount = 0;
            updateNotificationBadge();
            renderNotifications();
            saveNotifications();
        });
    }

    // Listen to WebSocket events to create notifications
    setupNotificationListeners();
}

// Add a new notification
function addNotification(notification) {
    const newNotification = {
        id: Date.now().toString(),
        timestamp: Math.floor(Date.now() / 1000),
        read: false,
        ...notification,
    };

    notifications.unshift(newNotification);

    // Keep only last 50 notifications
    if (notifications.length > 50) {
        notifications = notifications.slice(0, 50);
    }

    unreadCount++;
    updateNotificationBadge();
    renderNotifications();
    saveNotifications();

    // Show toast for immediate feedback
    showToast(notification.message, notification.type || 'info', 3000);
}

// Update notification badge
function updateNotificationBadge() {
    const badge = document.getElementById('notification-badge');
    if (!badge) return;

    if (unreadCount > 0) {
        badge.textContent = unreadCount > 99 ? '99+' : unreadCount;
        badge.classList.remove('hidden');
        badge.setAttribute('aria-label', `${unreadCount} unread notifications`);
    } else {
        badge.classList.add('hidden');
    }
}

// Render notifications list
function renderNotifications() {
    const list = document.getElementById('notification-list');
    const empty = document.getElementById('notification-empty');

    if (!list || !empty) return;

    list.innerHTML = '';

    if (notifications.length === 0) {
        empty.style.display = 'block';
        list.style.display = 'none';
        return;
    }

    empty.style.display = 'none';
    list.style.display = 'block';

    notifications.forEach(notification => {
        const item = createNotificationItem(notification);
        list.appendChild(item);
    });
}

// Create notification item element
function createNotificationItem(notification) {
    const item = document.createElement('div');
    item.className = `notification-item ${notification.type || 'info'}`;

    if (!notification.read) {
        item.classList.add('unread');
    }

    const icon = notificationIcons[notification.type] || notificationIcons.info;
    const timeAgo = formatTimeAgo(notification.timestamp);

    item.innerHTML = `
        <div class="notification-icon">${icon}</div>
        <div class="notification-content">
            <div class="notification-title">${notification.title}</div>
            <div class="notification-message">${notification.message}</div>
            <div class="notification-time">${timeAgo}</div>
        </div>
    `;

    // Mark as read on click
    item.addEventListener('click', () => {
        if (!notification.read) {
            notification.read = true;
            unreadCount = Math.max(0, unreadCount - 1);
            updateNotificationBadge();
            item.classList.remove('unread');
            saveNotifications();
        }

        // Execute action if provided
        if (notification.action) {
            notification.action();
        }
    });

    return item;
}

// Format time ago
function formatTimeAgo(timestamp) {
    const now = Math.floor(Date.now() / 1000);
    const diff = now - timestamp;

    if (diff < 60) return 'Just now';
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    if (diff < 604800) return `${Math.floor(diff / 86400)}d ago`;

    return formatDate(timestamp);
}

// Save notifications to localStorage
function saveNotifications() {
    localStorage.setItem('icn-notifications', JSON.stringify(notifications));
}

// Setup listeners for notification triggers
function setupNotificationListeners() {
    // Listen to WebSocket events (when connected)
    if (state.ws && state.wsConnected) {
        // WebSocket message handler would trigger notifications
        // This is already set up in the WebSocket code
    }

    // Simulate some notifications for demo (remove in production)
    // addDemoNotifications();
}

// Add demo notifications (for testing)
function addDemoNotifications() {
    setTimeout(() => {
        addNotification({
            type: 'payment',
            title: 'Payment Received',
            message: 'You received 3 hours from did:icn:alice for garden work',
        });
    }, 3000);

    setTimeout(() => {
        addNotification({
            type: 'proposal',
            title: 'New Proposal',
            message: 'Vote needed: Update credit limit to -100 hours',
            action: () => {
                switchTab('governance');
                document.getElementById('notification-panel').classList.add('hidden');
            },
        });
    }, 6000);

    setTimeout(() => {
        addNotification({
            type: 'member',
            title: 'New Member',
            message: 'did:icn:charlie joined the cooperative',
        });
    }, 9000);
}

// Initialize notification center after DOM loads
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initializeNotificationCenter);
} else {
    initializeNotificationCenter();
}

// ============================================================================
// Phase 9: Pagination & Virtual Scrolling
// ============================================================================

/**
 * Generic Pagination Manager
 * Handles pagination for any list (transactions, members, etc.)
 */
class PaginationManager {
    constructor(config) {
        this.containerId = config.containerId;
        this.paginationId = config.paginationId;
        this.pageInfoId = config.pageInfoId;
        this.pageSizeId = config.pageSizeId;
        this.firstPageId = config.firstPageId;
        this.prevPageId = config.prevPageId;
        this.nextPageId = config.nextPageId;
        this.lastPageId = config.lastPageId;
        this.pageNumbersId = config.pageNumbersId;
        this.renderItem = config.renderItem;
        this.itemName = config.itemName || 'items';

        this.data = [];
        this.filteredData = [];
        this.currentPage = 1;
        this.pageSize = 25;
        this.totalPages = 1;

        this.initialize();
    }

    initialize() {
        const pageSizeSelect = document.getElementById(this.pageSizeId);
        if (pageSizeSelect) {
            pageSizeSelect.addEventListener('change', (e) => {
                this.pageSize = parseInt(e.target.value);
                this.currentPage = 1;
                this.render();
            });
        }

        // First page button
        const firstBtn = document.getElementById(this.firstPageId);
        if (firstBtn) {
            firstBtn.addEventListener('click', () => {
                this.goToPage(1);
            });
        }

        // Previous page button
        const prevBtn = document.getElementById(this.prevPageId);
        if (prevBtn) {
            prevBtn.addEventListener('click', () => {
                this.goToPage(this.currentPage - 1);
            });
        }

        // Next page button
        const nextBtn = document.getElementById(this.nextPageId);
        if (nextBtn) {
            nextBtn.addEventListener('click', () => {
                this.goToPage(this.currentPage + 1);
            });
        }

        // Last page button
        const lastBtn = document.getElementById(this.lastPageId);
        if (lastBtn) {
            lastBtn.addEventListener('click', () => {
                this.goToPage(this.totalPages);
            });
        }
    }

    setData(data, filtered = null) {
        this.data = data;
        this.filteredData = filtered || data;
        this.totalPages = Math.ceil(this.filteredData.length / this.pageSize);

        // Reset to page 1 if current page is out of bounds
        if (this.currentPage > this.totalPages) {
            this.currentPage = Math.max(1, this.totalPages);
        }

        this.render();
    }

    goToPage(pageNum) {
        if (pageNum < 1 || pageNum > this.totalPages) return;
        this.currentPage = pageNum;
        this.render();

        // Scroll to top of container
        const container = document.getElementById(this.containerId);
        if (container) {
            container.scrollIntoView({ behavior: 'smooth', block: 'start' });
        }
    }

    render() {
        this.renderItems();
        this.renderPaginationControls();
    }

    renderItems() {
        const container = document.getElementById(this.containerId);
        if (!container) return;

        // Calculate start and end indices
        const startIndex = (this.currentPage - 1) * this.pageSize;
        const endIndex = Math.min(startIndex + this.pageSize, this.filteredData.length);
        const pageData = this.filteredData.slice(startIndex, endIndex);

        // Add transition class
        container.classList.add('page-transitioning');

        // Render items
        if (pageData.length === 0) {
            container.innerHTML = `<p class="empty">No ${this.itemName} found</p>`;
        } else {
            container.innerHTML = pageData.map(item => this.renderItem(item)).join('');
        }

        // Remove transition class after animation
        setTimeout(() => {
            container.classList.remove('page-transitioning');
        }, 300);
    }

    renderPaginationControls() {
        const paginationContainer = document.getElementById(this.paginationId);
        if (!paginationContainer) return;

        // Show/hide pagination controls
        if (this.filteredData.length <= this.pageSize && this.pageSize === 25) {
            paginationContainer.classList.add('hidden');
            return;
        }

        paginationContainer.classList.remove('hidden');

        // Update page info
        const pageInfo = document.getElementById(this.pageInfoId);
        if (pageInfo) {
            const startIndex = (this.currentPage - 1) * this.pageSize + 1;
            const endIndex = Math.min(this.currentPage * this.pageSize, this.filteredData.length);
            pageInfo.textContent = `Showing ${startIndex}-${endIndex} of ${this.filteredData.length} ${this.itemName}`;
        }

        // Update button states
        const firstBtn = document.getElementById(this.firstPageId);
        const prevBtn = document.getElementById(this.prevPageId);
        const nextBtn = document.getElementById(this.nextPageId);
        const lastBtn = document.getElementById(this.lastPageId);

        if (firstBtn) firstBtn.disabled = this.currentPage === 1;
        if (prevBtn) prevBtn.disabled = this.currentPage === 1;
        if (nextBtn) nextBtn.disabled = this.currentPage === this.totalPages;
        if (lastBtn) lastBtn.disabled = this.currentPage === this.totalPages;

        // Render page numbers
        this.renderPageNumbers();
    }

    renderPageNumbers() {
        const pageNumbersContainer = document.getElementById(this.pageNumbersId);
        if (!pageNumbersContainer) return;

        const pageNumbers = this.calculatePageNumbers();
        pageNumbersContainer.innerHTML = pageNumbers.map(page => {
            if (page === '...') {
                return '<span class="page-ellipsis">...</span>';
            }

            const isActive = page === this.currentPage;
            return `
                <button
                    class="page-number ${isActive ? 'active' : ''}"
                    data-page="${page}"
                    aria-label="Page ${page}"
                    aria-current="${isActive ? 'page' : 'false'}"
                    ${isActive ? 'disabled' : ''}
                >
                    ${page}
                </button>
            `;
        }).join('');

        // Add click listeners to page numbers
        pageNumbersContainer.querySelectorAll('.page-number').forEach(btn => {
            btn.addEventListener('click', (e) => {
                const page = parseInt(e.target.dataset.page);
                this.goToPage(page);
            });
        });
    }

    calculatePageNumbers() {
        const pages = [];
        const maxVisible = 7; // Maximum number of page buttons to show

        if (this.totalPages <= maxVisible) {
            // Show all pages
            for (let i = 1; i <= this.totalPages; i++) {
                pages.push(i);
            }
        } else {
            // Always show first page
            pages.push(1);

            // Calculate range around current page
            let start = Math.max(2, this.currentPage - 1);
            let end = Math.min(this.totalPages - 1, this.currentPage + 1);

            // Adjust if at boundaries
            if (this.currentPage <= 3) {
                end = 4;
            } else if (this.currentPage >= this.totalPages - 2) {
                start = this.totalPages - 3;
            }

            // Add ellipsis before if needed
            if (start > 2) {
                pages.push('...');
            }

            // Add middle pages
            for (let i = start; i <= end; i++) {
                pages.push(i);
            }

            // Add ellipsis after if needed
            if (end < this.totalPages - 1) {
                pages.push('...');
            }

            // Always show last page
            pages.push(this.totalPages);
        }

        return pages;
    }
}

// Initialize Transaction Pagination
let transactionPagination = null;

function initializeTransactionPagination() {
    transactionPagination = new PaginationManager({
        containerId: 'transaction-list',
        paginationId: 'transaction-pagination',
        pageInfoId: 'transaction-page-info',
        pageSizeId: 'transaction-page-size',
        firstPageId: 'transaction-first-page',
        prevPageId: 'transaction-prev-page',
        nextPageId: 'transaction-next-page',
        lastPageId: 'transaction-last-page',
        pageNumbersId: 'transaction-page-numbers',
        itemName: 'transactions',
        renderItem: (tx) => {
            const isIncoming = tx.to === state.userDid;
            const amount = parseFloat(tx.amount);
            const amountStr = amount.toFixed(1);

            return `
                <div class="transaction-item ${isIncoming ? 'incoming' : 'outgoing'}">
                    <div class="transaction-icon">${isIncoming ? '↓' : '↑'}</div>
                    <div class="transaction-details">
                        <div class="transaction-party">
                            ${isIncoming ? 'From' : 'To'}:
                            <span class="did">${truncateDid(isIncoming ? tx.from : tx.to)}</span>
                        </div>
                        <div class="transaction-memo">${tx.memo || 'No description'}</div>
                        <div class="transaction-date">${formatDate(tx.timestamp)}</div>
                    </div>
                    <div class="transaction-amount ${isIncoming ? 'positive' : 'negative'}">
                        ${isIncoming ? '+' : '-'}${amountStr} hours
                    </div>
                </div>
            `;
        }
    });
}

// Initialize Member Pagination
let memberPagination = null;

function initializeMemberPagination() {
    memberPagination = new PaginationManager({
        containerId: 'member-list',
        paginationId: 'member-pagination',
        pageInfoId: 'member-page-info',
        pageSizeId: 'member-page-size',
        firstPageId: 'member-first-page',
        prevPageId: 'member-prev-page',
        nextPageId: 'member-next-page',
        lastPageId: 'member-last-page',
        pageNumbersId: 'member-page-numbers',
        itemName: 'members',
        renderItem: (member) => {
            const balance = parseFloat(member.balance || 0);
            const balanceClass = balance >= 0 ? 'positive' : 'negative';
            const isCurrentUser = member.did === state.did;
            const isOwner = member.role === 'owner';
            const canManage = state.userRole === 'owner' || state.userRole === 'admin';

            // Only show action buttons for non-owners and non-self
            const showActions = canManage && !isOwner && !isCurrentUser;

            return `
                <div class="member-item">
                    <div class="member-avatar">
                        <div class="avatar-placeholder">${member.did.slice(-6, -4).toUpperCase()}</div>
                    </div>
                    <div class="member-info">
                        <div class="member-did">${truncateDid(member.did)}</div>
                        <div class="member-role ${member.role || ''}">${member.role || 'Member'}</div>
                    </div>
                    <div class="member-balance ${balanceClass}">
                        ${balance.toFixed(1)} hours
                    </div>
                    <button
                        class="btn btn-small btn-copy-did"
                        data-did="${member.did}"
                        aria-label="Copy DID to clipboard"
                        title="Copy DID"
                    >
                        📋 Copy
                    </button>
                    ${showActions ? `
                    <div class="member-actions">
                        <button
                            class="btn btn-small btn-edit"
                            data-did="${member.did}"
                            data-role="${member.role || 'member'}"
                            title="Edit role"
                        >
                            ✏️
                        </button>
                        <button
                            class="btn btn-small btn-remove"
                            data-did="${member.did}"
                            title="Remove member"
                        >
                            🗑️
                        </button>
                    </div>
                    ` : ''}
                </div>
            `;
        }
    });
}

// Define renderTransactionHistory function with pagination support
let renderTransactionHistory = function() {
    if (!transactionPagination) {
        initializeTransactionPagination();
    }

    // Get all transactions
    const allTransactions = state.transactions || [];

    // Apply existing filters
    const filterValue = document.getElementById('history-filter')?.value || 'month';
    const sortValue = document.getElementById('transaction-sort')?.value || 'date-desc';

    // Filter transactions
    let filteredTransactions = filterTransactionsByDate(allTransactions, filterValue);

    // Sort transactions
    filteredTransactions = sortTransactions(filteredTransactions, sortValue);

    // Update pagination with filtered data
    transactionPagination.setData(allTransactions, filteredTransactions);
};

// Update the existing renderMemberList function to use pagination
const originalRenderMemberList = renderMemberList;
renderMemberList = function() {
    if (!memberPagination) {
        initializeMemberPagination();
    }

    // Get all members
    const allMembers = state.members || [];

    // Apply search filter
    const searchTerm = document.getElementById('member-search')?.value?.toLowerCase() || '';
    const filteredMembers = searchTerm
        ? allMembers.filter(m => m.did.toLowerCase().includes(searchTerm))
        : allMembers;

    // Update pagination with filtered data
    memberPagination.setData(allMembers, filteredMembers);
};

// Re-render when filters/sort/search change
document.addEventListener('DOMContentLoaded', () => {
    // Transaction filter/sort listeners
    const historyFilter = document.getElementById('history-filter');
    if (historyFilter) {
        historyFilter.addEventListener('change', renderTransactionHistory);
    }

    const transactionSort = document.getElementById('transaction-sort');
    if (transactionSort) {
        transactionSort.addEventListener('change', renderTransactionHistory);
    }

    // Member search listener
    const memberSearch = document.getElementById('member-search');
    if (memberSearch) {
        memberSearch.addEventListener('input', renderMemberList);
    }
});

// ============================================================================
// Phase 9: CSV Import for Bulk Member Operations
// ============================================================================

let csvImportState = {
    currentStep: 1,
    file: null,
    parsedData: [],
    validRows: [],
    invalidRows: [],
};

function initializeCSVImport() {
    const importBtn = document.getElementById('import-members-btn');
    const modal = document.getElementById('import-members-modal');
    const closeBtn = document.getElementById('close-import-members');
    const cancelBtn = document.getElementById('import-cancel-btn');
    const backBtn = document.getElementById('import-back-btn');
    const nextBtn = document.getElementById('import-next-btn');
    const confirmBtn = document.getElementById('import-confirm-btn');
    const closeFinalBtn = document.getElementById('import-close-btn');
    const fileInput = document.getElementById('csv-file-input');
    const uploadArea = document.getElementById('csv-upload-area');
    const downloadTemplateBtn = document.getElementById('download-template');

    if (!importBtn || !modal) return;

    // Open modal
    importBtn.addEventListener('click', () => {
        resetCSVImport();
        modal.classList.remove('hidden');
        showStep(1);
    });

    // Close modal
    const closeModal = () => {
        modal.classList.add('hidden');
        resetCSVImport();
    };

    closeBtn.addEventListener('click', closeModal);
    cancelBtn.addEventListener('click', closeModal);
    closeFinalBtn.addEventListener('click', closeModal);

    // Download template
    downloadTemplateBtn.addEventListener('click', () => {
        const template = 'did,role,balance\ndid:icn:example123,member,0.0\ndid:icn:example456,admin,10.5';
        const blob = new Blob([template], { type: 'text/csv' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = 'member-import-template.csv';
        a.click();
        URL.revokeObjectURL(url);
    });

    // File input change
    fileInput.addEventListener('change', (e) => {
        if (e.target.files.length > 0) {
            handleFile(e.target.files[0]);
        }
    });

    // Drag and drop
    uploadArea.addEventListener('dragover', (e) => {
        e.preventDefault();
        uploadArea.classList.add('dragover');
    });

    uploadArea.addEventListener('dragleave', () => {
        uploadArea.classList.remove('dragover');
    });

    uploadArea.addEventListener('drop', (e) => {
        e.preventDefault();
        uploadArea.classList.remove('dragover');
        if (e.dataTransfer.files.length > 0) {
            handleFile(e.dataTransfer.files[0]);
        }
    });

    // Step navigation
    backBtn.addEventListener('click', () => {
        showStep(csvImportState.currentStep - 1);
    });

    nextBtn.addEventListener('click', () => {
        if (csvImportState.currentStep === 1) {
            // Parse and validate CSV
            parseCSV();
        }
        showStep(csvImportState.currentStep + 1);
    });

    confirmBtn.addEventListener('click', () => {
        importMembers();
        showStep(3);
    });

    // Close modal on background click
    modal.addEventListener('click', (e) => {
        if (e.target === modal) {
            closeModal();
        }
    });
}

function resetCSVImport() {
    csvImportState = {
        currentStep: 1,
        file: null,
        parsedData: [],
        validRows: [],
        invalidRows: [],
    };

    // Reset file input
    const fileInput = document.getElementById('csv-file-input');
    if (fileInput) fileInput.value = '';

    // Hide file info
    const fileInfo = document.getElementById('file-info');
    if (fileInfo) fileInfo.classList.add('hidden');
}

function showStep(stepNum) {
    csvImportState.currentStep = stepNum;

    // Hide all steps
    for (let i = 1; i <= 3; i++) {
        const step = document.getElementById(`import-step-${i}`);
        if (step) step.classList.add('hidden');
    }

    // Show current step
    const currentStep = document.getElementById(`import-step-${stepNum}`);
    if (currentStep) currentStep.classList.remove('hidden');

    // Update buttons
    const backBtn = document.getElementById('import-back-btn');
    const cancelBtn = document.getElementById('import-cancel-btn');
    const nextBtn = document.getElementById('import-next-btn');
    const confirmBtn = document.getElementById('import-confirm-btn');
    const closeBtn = document.getElementById('import-close-btn');

    if (backBtn) backBtn.classList.toggle('hidden', stepNum === 1);
    if (cancelBtn) cancelBtn.classList.toggle('hidden', stepNum === 3);
    if (nextBtn) nextBtn.classList.toggle('hidden', stepNum !== 1);
    if (confirmBtn) confirmBtn.classList.toggle('hidden', stepNum !== 2);
    if (closeBtn) closeBtn.classList.toggle('hidden', stepNum !== 3);
}

function handleFile(file) {
    if (!file.name.endsWith('.csv')) {
        showToast('Please select a CSV file', 'error');
        return;
    }

    csvImportState.file = file;

    // Show file info
    const fileInfo = document.getElementById('file-info');
    const fileName = document.getElementById('file-name');
    const fileSize = document.getElementById('file-size');

    if (fileInfo && fileName && fileSize) {
        fileName.textContent = file.name;
        fileSize.textContent = formatFileSize(file.size);
        fileInfo.classList.remove('hidden');
    }

    // Enable next button
    const nextBtn = document.getElementById('import-next-btn');
    if (nextBtn) nextBtn.disabled = false;
}

function formatFileSize(bytes) {
    if (bytes < 1024) return bytes + ' B';
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
    return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
}

function parseCSV() {
    if (!csvImportState.file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
        const text = e.target.result;
        const lines = text.split('\n').map(line => line.trim()).filter(line => line);

        if (lines.length < 2) {
            showToast('CSV file is empty or has no data rows', 'error');
            return;
        }

        // Parse header
        const header = lines[0].split(',').map(h => h.trim().toLowerCase());
        const requiredColumns = ['did', 'role', 'balance'];

        // Validate header
        for (const col of requiredColumns) {
            if (!header.includes(col)) {
                showToast(`Missing required column: ${col}`, 'error');
                return;
            }
        }

        // Parse data rows
        const didIndex = header.indexOf('did');
        const roleIndex = header.indexOf('role');
        const balanceIndex = header.indexOf('balance');

        csvImportState.parsedData = [];
        csvImportState.validRows = [];
        csvImportState.invalidRows = [];

        const existingDIDs = new Set(state.members.map(m => m.did));
        const seenDIDs = new Set();

        for (let i = 1; i < lines.length; i++) {
            const cols = lines[i].split(',').map(c => c.trim());
            const row = {
                lineNumber: i + 1,
                did: cols[didIndex] || '',
                role: cols[roleIndex] || 'member',
                balance: cols[balanceIndex] || '0',
            };

            // Validate row
            const errors = [];

            // Validate DID
            if (!row.did) {
                errors.push('DID is required');
            } else if (!row.did.startsWith('did:icn:')) {
                errors.push('DID must start with "did:icn:"');
            } else if (row.did.length < 15) {
                errors.push('DID is too short');
            } else if (existingDIDs.has(row.did)) {
                errors.push('DID already exists');
                row.status = 'duplicate';
            } else if (seenDIDs.has(row.did)) {
                errors.push('Duplicate DID in CSV');
                row.status = 'duplicate';
            }

            seenDIDs.add(row.did);

            // Validate role
            const validRoles = ['member', 'admin', 'owner'];
            if (!validRoles.includes(row.role.toLowerCase())) {
                errors.push('Role must be one of: member, admin, owner');
            }

            // Validate balance
            const balance = parseFloat(row.balance);
            if (isNaN(balance)) {
                errors.push('Balance must be a number');
            }

            row.errors = errors;
            row.status = row.status || (errors.length === 0 ? 'valid' : 'invalid');
            row.balanceNum = balance;

            csvImportState.parsedData.push(row);

            if (row.status === 'valid') {
                csvImportState.validRows.push(row);
            } else {
                csvImportState.invalidRows.push(row);
            }
        }

        renderPreview();
    };

    reader.readAsText(csvImportState.file);
}

function renderPreview() {
    // Update stats
    document.getElementById('total-rows').textContent = csvImportState.parsedData.length;
    document.getElementById('valid-rows').textContent = csvImportState.validRows.length;
    document.getElementById('invalid-rows').textContent = csvImportState.invalidRows.length;

    // Show/hide errors section
    const errorsSection = document.getElementById('validation-errors');
    const errorList = document.getElementById('error-list');

    if (csvImportState.invalidRows.length > 0) {
        errorsSection.classList.remove('hidden');
        errorList.innerHTML = csvImportState.invalidRows
            .slice(0, 10) // Show first 10 errors
            .map(row => {
                return `<li>Line ${row.lineNumber}: ${row.errors.join(', ')}</li>`;
            })
            .join('');

        if (csvImportState.invalidRows.length > 10) {
            errorList.innerHTML += `<li>... and ${csvImportState.invalidRows.length - 10} more errors</li>`;
        }
    } else {
        errorsSection.classList.add('hidden');
    }

    // Render preview table (first 10 rows)
    const previewBody = document.getElementById('preview-body');
    previewBody.innerHTML = csvImportState.parsedData
        .slice(0, 10)
        .map(row => {
            let statusText = row.status;
            let statusClass = `status-${row.status}`;

            if (row.status === 'invalid') {
                statusText = '✗ Invalid';
            } else if (row.status === 'duplicate') {
                statusText = '⚠ Duplicate';
            } else {
                statusText = '✓ Valid';
            }

            return `
                <tr>
                    <td>${truncateDid(row.did)}</td>
                    <td>${row.role}</td>
                    <td>${parseFloat(row.balance).toFixed(1)}</td>
                    <td class="${statusClass}">${statusText}</td>
                </tr>
            `;
        })
        .join('');

    // Enable/disable import button
    const confirmBtn = document.getElementById('import-confirm-btn');
    if (confirmBtn) {
        confirmBtn.disabled = csvImportState.validRows.length === 0;
    }
}

function importMembers() {
    // Add valid rows to state
    csvImportState.validRows.forEach(row => {
        state.members.push({
            did: row.did,
            role: row.role,
            balance: row.balanceNum,
        });
    });

    // Update display
    document.getElementById('imported-count').textContent = csvImportState.validRows.length;

    // Save to localStorage
    localStorage.setItem('icn-members', JSON.stringify(state.members));

    // Re-render member list
    renderMemberList();

    // Show notification
    addNotification({
        type: 'success',
        title: 'Import Complete',
        message: `${csvImportState.validRows.length} members imported successfully`,
    });

    showToast(`Imported ${csvImportState.validRows.length} members`, 'success');
}

// Initialize CSV import after DOM loads
document.addEventListener('DOMContentLoaded', initializeCSVImport);

// ============================================================================
// Phase 9: Batch Transaction Import
// ============================================================================

let batchImportState = {
    currentStep: 1,
    file: null,
    parsedData: [],
    validRows: [],
    invalidRows: [],
};

function initializeBatchImport() {
    const importBtn = document.getElementById('import-batch-btn');
    const modal = document.getElementById('import-batch-modal');
    const closeBtn = document.getElementById('close-import-batch');
    const cancelBtn = document.getElementById('batch-cancel-btn');
    const backBtn = document.getElementById('batch-back-btn');
    const nextBtn = document.getElementById('batch-next-btn');
    const confirmBtn = document.getElementById('batch-confirm-btn');
    const closeFinalBtn = document.getElementById('batch-close-btn');
    const fileInput = document.getElementById('batch-file-input');
    const uploadArea = document.getElementById('batch-upload-area');
    const downloadTemplateBtn = document.getElementById('download-batch-template');

    if (!importBtn || !modal) return;

    // Open modal
    importBtn.addEventListener('click', () => {
        resetBatchImport();
        modal.classList.remove('hidden');
        showBatchStep(1);
    });

    // Close modal
    const closeModal = () => {
        modal.classList.add('hidden');
        resetBatchImport();
    };

    closeBtn.addEventListener('click', closeModal);
    cancelBtn.addEventListener('click', closeModal);
    closeFinalBtn.addEventListener('click', closeModal);

    // Download template
    downloadTemplateBtn.addEventListener('click', () => {
        const template = 'from,to,amount,memo\ndid:icn:alice,did:icn:bob,2.5,Garden work\ndid:icn:bob,did:icn:charlie,1.0,Tech support';
        const blob = new Blob([template], { type: 'text/csv' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = 'transaction-batch-template.csv';
        a.click();
        URL.revokeObjectURL(url);
    });

    // File input change
    fileInput.addEventListener('change', (e) => {
        if (e.target.files.length > 0) {
            handleBatchFile(e.target.files[0]);
        }
    });

    // Drag and drop
    uploadArea.addEventListener('dragover', (e) => {
        e.preventDefault();
        uploadArea.classList.add('dragover');
    });

    uploadArea.addEventListener('dragleave', () => {
        uploadArea.classList.remove('dragover');
    });

    uploadArea.addEventListener('drop', (e) => {
        e.preventDefault();
        uploadArea.classList.remove('dragover');
        if (e.dataTransfer.files.length > 0) {
            handleBatchFile(e.dataTransfer.files[0]);
        }
    });

    // Step navigation
    backBtn.addEventListener('click', () => {
        showBatchStep(batchImportState.currentStep - 1);
    });

    nextBtn.addEventListener('click', () => {
        if (batchImportState.currentStep === 1) {
            // Parse and validate CSV
            parseBatchCSV();
        }
        showBatchStep(batchImportState.currentStep + 1);
    });

    confirmBtn.addEventListener('click', () => {
        importBatchTransactions();
        showBatchStep(3);
    });

    // Close modal on background click
    modal.addEventListener('click', (e) => {
        if (e.target === modal) {
            closeModal();
        }
    });
}

function resetBatchImport() {
    batchImportState = {
        currentStep: 1,
        file: null,
        parsedData: [],
        validRows: [],
        invalidRows: [],
    };

    // Reset file input
    const fileInput = document.getElementById('batch-file-input');
    if (fileInput) fileInput.value = '';

    // Hide file info
    const fileInfo = document.getElementById('batch-file-info');
    if (fileInfo) fileInfo.classList.add('hidden');
}

function showBatchStep(stepNum) {
    batchImportState.currentStep = stepNum;

    // Hide all steps
    for (let i = 1; i <= 3; i++) {
        const step = document.getElementById(`batch-step-${i}`);
        if (step) step.classList.add('hidden');
    }

    // Show current step
    const currentStep = document.getElementById(`batch-step-${stepNum}`);
    if (currentStep) currentStep.classList.remove('hidden');

    // Update buttons
    const backBtn = document.getElementById('batch-back-btn');
    const cancelBtn = document.getElementById('batch-cancel-btn');
    const nextBtn = document.getElementById('batch-next-btn');
    const confirmBtn = document.getElementById('batch-confirm-btn');
    const closeBtn = document.getElementById('batch-close-btn');

    if (backBtn) backBtn.classList.toggle('hidden', stepNum === 1);
    if (cancelBtn) cancelBtn.classList.toggle('hidden', stepNum === 3);
    if (nextBtn) nextBtn.classList.toggle('hidden', stepNum !== 1);
    if (confirmBtn) confirmBtn.classList.toggle('hidden', stepNum !== 2);
    if (closeBtn) closeBtn.classList.toggle('hidden', stepNum !== 3);
}

function handleBatchFile(file) {
    if (!file.name.endsWith('.csv')) {
        showToast('Please select a CSV file', 'error');
        return;
    }

    batchImportState.file = file;

    // Show file info
    const fileInfo = document.getElementById('batch-file-info');
    const fileName = document.getElementById('batch-file-name');
    const fileSize = document.getElementById('batch-file-size');

    if (fileInfo && fileName && fileSize) {
        fileName.textContent = file.name;
        fileSize.textContent = formatFileSize(file.size);
        fileInfo.classList.remove('hidden');
    }

    // Enable next button
    const nextBtn = document.getElementById('batch-next-btn');
    if (nextBtn) nextBtn.disabled = false;
}

function parseBatchCSV() {
    if (!batchImportState.file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
        const text = e.target.result;
        const lines = text.split('\n').map(line => line.trim()).filter(line => line);

        if (lines.length < 2) {
            showToast('CSV file is empty or has no data rows', 'error');
            return;
        }

        // Parse header
        const header = lines[0].split(',').map(h => h.trim().toLowerCase());
        const requiredColumns = ['from', 'to', 'amount', 'memo'];

        // Validate header
        for (const col of requiredColumns) {
            if (!header.includes(col)) {
                showToast(`Missing required column: ${col}`, 'error');
                return;
            }
        }

        // Parse data rows
        const fromIndex = header.indexOf('from');
        const toIndex = header.indexOf('to');
        const amountIndex = header.indexOf('amount');
        const memoIndex = header.indexOf('memo');

        batchImportState.parsedData = [];
        batchImportState.validRows = [];
        batchImportState.invalidRows = [];

        const memberDIDs = new Set(state.members.map(m => m.did));

        for (let i = 1; i < lines.length; i++) {
            const cols = lines[i].split(',').map(c => c.trim());
            const row = {
                lineNumber: i + 1,
                from: cols[fromIndex] || '',
                to: cols[toIndex] || '',
                amount: cols[amountIndex] || '0',
                memo: cols[memoIndex] || '',
            };

            // Validate row
            const errors = [];

            // Validate from DID
            if (!row.from) {
                errors.push('From DID is required');
            } else if (!row.from.startsWith('did:icn:')) {
                errors.push('From DID must start with "did:icn:"');
            } else if (!memberDIDs.has(row.from)) {
                errors.push('From DID not found in members');
            }

            // Validate to DID
            if (!row.to) {
                errors.push('To DID is required');
            } else if (!row.to.startsWith('did:icn:')) {
                errors.push('To DID must start with "did:icn:"');
            } else if (!memberDIDs.has(row.to)) {
                errors.push('To DID not found in members');
            }

            // Check for self-transactions
            if (row.from && row.to && row.from === row.to) {
                errors.push('Cannot transfer to yourself');
            }

            // Validate amount
            const amount = parseFloat(row.amount);
            if (isNaN(amount)) {
                errors.push('Amount must be a number');
            } else if (amount <= 0) {
                errors.push('Amount must be positive');
            } else if (amount > 1000) {
                errors.push('Amount exceeds limit (max 1000)');
            }

            row.errors = errors;
            row.status = errors.length === 0 ? 'valid' : 'invalid';
            row.amountNum = amount;

            batchImportState.parsedData.push(row);

            if (row.status === 'valid') {
                batchImportState.validRows.push(row);
            } else {
                batchImportState.invalidRows.push(row);
            }
        }

        renderBatchPreview();
    };

    reader.readAsText(batchImportState.file);
}

function renderBatchPreview() {
    // Calculate total amount
    const totalAmount = batchImportState.validRows.reduce((sum, row) => sum + row.amountNum, 0);

    // Update stats
    document.getElementById('batch-total-rows').textContent = batchImportState.parsedData.length;
    document.getElementById('batch-valid-rows').textContent = batchImportState.validRows.length;
    document.getElementById('batch-invalid-rows').textContent = batchImportState.invalidRows.length;
    document.getElementById('batch-total-amount').textContent = totalAmount.toFixed(1);

    // Show/hide errors section
    const errorsSection = document.getElementById('batch-errors');
    const errorList = document.getElementById('batch-error-list');

    if (batchImportState.invalidRows.length > 0) {
        errorsSection.classList.remove('hidden');
        errorList.innerHTML = batchImportState.invalidRows
            .slice(0, 10) // Show first 10 errors
            .map(row => {
                return `<li>Line ${row.lineNumber}: ${row.errors.join(', ')}</li>`;
            })
            .join('');

        if (batchImportState.invalidRows.length > 10) {
            errorList.innerHTML += `<li>... and ${batchImportState.invalidRows.length - 10} more errors</li>`;
        }
    } else {
        errorsSection.classList.add('hidden');
    }

    // Render preview table (first 10 rows)
    const previewBody = document.getElementById('batch-preview-body');
    previewBody.innerHTML = batchImportState.parsedData
        .slice(0, 10)
        .map(row => {
            let statusText = row.status;
            let statusClass = `status-${row.status}`;

            if (row.status === 'invalid') {
                statusText = '✗ Invalid';
            } else {
                statusText = '✓ Valid';
            }

            return `
                <tr>
                    <td>${truncateDid(row.from)}</td>
                    <td>${truncateDid(row.to)}</td>
                    <td>${parseFloat(row.amount).toFixed(1)} hrs</td>
                    <td>${row.memo}</td>
                    <td class="${statusClass}">${statusText}</td>
                </tr>
            `;
        })
        .join('');

    // Enable/disable import button
    const confirmBtn = document.getElementById('batch-confirm-btn');
    if (confirmBtn) {
        confirmBtn.disabled = batchImportState.validRows.length === 0;
    }
}

function importBatchTransactions() {
    // Add valid transactions to state
    const now = Math.floor(Date.now() / 1000);
    let totalAmount = 0;

    batchImportState.validRows.forEach((row, index) => {
        state.transactions.push({
            id: `batch-${Date.now()}-${index}`,
            from: row.from,
            to: row.to,
            amount: row.amountNum,
            currency: 'hours',
            memo: row.memo,
            timestamp: now + index, // Slight offset to maintain order
        });

        totalAmount += row.amountNum;
    });

    // Update display
    document.getElementById('batch-imported-count').textContent = batchImportState.validRows.length;
    document.getElementById('batch-imported-total').textContent = totalAmount.toFixed(1);

    // Save to localStorage
    localStorage.setItem('icn-transactions', JSON.stringify(state.transactions));

    // Re-render transaction history
    renderTransactionHistory();

    // Show notification
    addNotification({
        type: 'success',
        title: 'Batch Import Complete',
        message: `${batchImportState.validRows.length} transactions imported (${totalAmount.toFixed(1)} hours)`,
    });

    showToast(`Imported ${batchImportState.validRows.length} transactions`, 'success');
}

// Initialize batch import after DOM loads
document.addEventListener('DOMContentLoaded', initializeBatchImport);

// ============================================================================
// Phase 10: Internationalization (i18n)
// ============================================================================

const translations = {
    en: {
        'logout': 'Logout',
        'notifications': 'Notifications',
        'markAllRead': 'Mark all read',
        'noNotifications': 'No notifications',
    },
    es: {
        'logout': 'Cerrar Sesión',
        'notifications': 'Notificaciones',
        'markAllRead': 'Marcar todo como leído',
        'noNotifications': 'Sin notificaciones',
    },
    fr: {
        'logout': 'Déconnexion',
        'notifications': 'Notifications',
        'markAllRead': 'Tout marquer comme lu',
        'noNotifications': 'Aucune notification',
    },
    de: {
        'logout': 'Abmelden',
        'notifications': 'Benachrichtigungen',
        'markAllRead': 'Alle als gelesen markieren',
        'noNotifications': 'Keine Benachrichtigungen',
    },
    zh: {
        'logout': '登出',
        'notifications': '通知',
        'markAllRead': '全部标记为已读',
        'noNotifications': '无通知',
    },
};

// Current language state
let currentLanguage = localStorage.getItem('icn-language') || 'en';

// Translation function
function t(key) {
    return translations[currentLanguage][key] || translations['en'][key] || key;
}

// Update all translatable elements
function updateTranslations() {
    document.body.classList.add('translating');

    // Update all elements with data-i18n attribute
    document.querySelectorAll('[data-i18n]').forEach(element => {
        const key = element.getAttribute('data-i18n');
        const translation = t(key);

        // Update text content or specific attributes
        if (element.hasAttribute('placeholder')) {
            element.placeholder = translation;
        } else if (element.hasAttribute('aria-label')) {
            element.setAttribute('aria-label', translation);
        } else if (element.hasAttribute('title')) {
            element.title = translation;
        } else {
            element.textContent = translation;
        }
    });

    // Update active language option
    document.querySelectorAll('.language-option').forEach(option => {
        const lang = option.getAttribute('data-lang');
        if (lang === currentLanguage) {
            option.classList.add('active');
        } else {
            option.classList.remove('active');
        }
    });

    // Set HTML lang attribute
    document.documentElement.setAttribute('lang', currentLanguage);

    // Set RTL direction for Arabic, Hebrew, etc.
    const rtlLanguages = ['ar', 'he', 'fa', 'ur'];
    if (rtlLanguages.includes(currentLanguage)) {
        document.documentElement.setAttribute('dir', 'rtl');
    } else {
        document.documentElement.setAttribute('dir', 'ltr');
    }

    setTimeout(() => {
        document.body.classList.remove('translating');
    }, 200);
}

// Initialize language switcher
function initializeLanguageSwitcher() {
    const languageBtn = document.getElementById('language-btn');
    const languageMenu = document.getElementById('language-menu');
    const languageOptions = document.querySelectorAll('.language-option');

    if (!languageBtn || !languageMenu) return;

    // Toggle language menu
    languageBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        const isOpen = !languageMenu.classList.contains('hidden');
        if (isOpen) {
            languageMenu.classList.add('hidden');
            languageBtn.setAttribute('aria-expanded', 'false');
        } else {
            languageMenu.classList.remove('hidden');
            languageBtn.setAttribute('aria-expanded', 'true');
        }
    });

    // Close menu when clicking outside
    document.addEventListener('click', (e) => {
        if (!languageMenu.contains(e.target) && !languageBtn.contains(e.target)) {
            languageMenu.classList.add('hidden');
            languageBtn.setAttribute('aria-expanded', 'false');
        }
    });

    // Language selection
    languageOptions.forEach(option => {
        option.addEventListener('click', () => {
            const lang = option.getAttribute('data-lang');
            if (lang !== currentLanguage) {
                currentLanguage = lang;
                localStorage.setItem('icn-language', lang);
                updateTranslations();
                showToast(`Language changed to ${option.querySelector('.language-name').textContent}`, 'success', 2000);
            }
            languageMenu.classList.add('hidden');
            languageBtn.setAttribute('aria-expanded', 'false');
        });
    });

    // Initial translation update
    updateTranslations();
}

// Initialize language switcher after DOM loads
document.addEventListener('DOMContentLoaded', initializeLanguageSwitcher);

// ============================================================================
// Phase 10: Admin Member Management
// ============================================================================

/**
 * Add a new member to the cooperative
 */
async function addMember(did, role) {
    try {
        await apiRequest('POST', `/coops/${state.coopId}/members`, { did, role });
        showToast(`Member ${truncateDid(did)} added successfully`, 'success');
        await loadMembers(); // Reload member list
    } catch (error) {
        console.error('Failed to add member:', error);
        showToast(getUserFriendlyError(error), 'error');
        throw error;
    }
}

/**
 * Remove a member from the cooperative
 */
async function removeMember(did) {
    try {
        await apiRequest('DELETE', `/coops/${state.coopId}/members/${encodeURIComponent(did)}`);
        showToast(`Member ${truncateDid(did)} removed`, 'success');
        await loadMembers(); // Reload member list
    } catch (error) {
        console.error('Failed to remove member:', error);
        showToast(getUserFriendlyError(error), 'error');
        throw error;
    }
}

/**
 * Update a member's role
 */
async function updateMemberRole(did, newRole) {
    try {
        await apiRequest('PUT', `/coops/${state.coopId}/members/${encodeURIComponent(did)}/role`, { role: newRole });
        showToast(`Updated ${truncateDid(did)} to ${newRole}`, 'success');
        await loadMembers(); // Reload member list
    } catch (error) {
        console.error('Failed to update member role:', error);
        showToast(getUserFriendlyError(error), 'error');
        throw error;
    }
}

/**
 * Initialize member management modals and event listeners
 */
function initializeMemberManagement() {
    // Add Member Modal
    const addMemberBtn = document.getElementById('add-member-btn');
    const addMemberModal = document.getElementById('add-member-modal');
    const closeAddMember = document.getElementById('close-add-member');
    const addMemberCancel = document.getElementById('add-member-cancel-btn');
    const addMemberSubmit = document.getElementById('add-member-submit-btn');
    const newMemberDid = document.getElementById('new-member-did');
    const newMemberRole = document.getElementById('new-member-role');

    if (addMemberBtn && addMemberModal) {
        addMemberBtn.addEventListener('click', () => {
            addMemberModal.classList.remove('hidden');
            newMemberDid.value = '';
            newMemberRole.value = 'member';
            newMemberDid.focus();
        });

        closeAddMember?.addEventListener('click', () => {
            addMemberModal.classList.add('hidden');
        });

        addMemberCancel?.addEventListener('click', () => {
            addMemberModal.classList.add('hidden');
        });

        addMemberSubmit?.addEventListener('click', async () => {
            const did = newMemberDid.value.trim();
            const role = newMemberRole.value;

            if (!did) {
                showToast('Please enter a DID', 'error');
                return;
            }

            if (!did.startsWith('did:icn:')) {
                showToast('DID must start with "did:icn:"', 'error');
                return;
            }

            addMemberSubmit.disabled = true;
            addMemberSubmit.textContent = 'Adding...';

            try {
                await addMember(did, role);
                addMemberModal.classList.add('hidden');
            } catch (e) {
                // Error already shown via toast
            } finally {
                addMemberSubmit.disabled = false;
                addMemberSubmit.textContent = 'Add Member';
            }
        });
    }

    // Edit Member Modal
    const editMemberModal = document.getElementById('edit-member-modal');
    const closeEditMember = document.getElementById('close-edit-member');
    const editMemberCancel = document.getElementById('edit-member-cancel-btn');
    const editMemberSubmit = document.getElementById('edit-member-submit-btn');
    const editMemberDid = document.getElementById('edit-member-did');
    const editMemberDidDisplay = document.getElementById('edit-member-did-display');
    const editMemberRole = document.getElementById('edit-member-role');

    if (editMemberModal) {
        closeEditMember?.addEventListener('click', () => {
            editMemberModal.classList.add('hidden');
        });

        editMemberCancel?.addEventListener('click', () => {
            editMemberModal.classList.add('hidden');
        });

        editMemberSubmit?.addEventListener('click', async () => {
            const did = editMemberDid.value;
            const newRole = editMemberRole.value;

            editMemberSubmit.disabled = true;
            editMemberSubmit.textContent = 'Updating...';

            try {
                await updateMemberRole(did, newRole);
                editMemberModal.classList.add('hidden');
            } catch (e) {
                // Error already shown via toast
            } finally {
                editMemberSubmit.disabled = false;
                editMemberSubmit.textContent = 'Update Role';
            }
        });
    }

    // Remove Member Modal
    const removeMemberModal = document.getElementById('remove-member-modal');
    const closeRemoveMember = document.getElementById('close-remove-member');
    const removeMemberCancel = document.getElementById('remove-member-cancel-btn');
    const removeMemberSubmit = document.getElementById('remove-member-submit-btn');
    const removeMemberDid = document.getElementById('remove-member-did');
    const removeMemberDidDisplay = document.getElementById('remove-member-did-display');

    if (removeMemberModal) {
        closeRemoveMember?.addEventListener('click', () => {
            removeMemberModal.classList.add('hidden');
        });

        removeMemberCancel?.addEventListener('click', () => {
            removeMemberModal.classList.add('hidden');
        });

        removeMemberSubmit?.addEventListener('click', async () => {
            const did = removeMemberDid.value;

            removeMemberSubmit.disabled = true;
            removeMemberSubmit.textContent = 'Removing...';

            try {
                await removeMember(did);
                removeMemberModal.classList.add('hidden');
            } catch (e) {
                // Error already shown via toast
            } finally {
                removeMemberSubmit.disabled = false;
                removeMemberSubmit.textContent = 'Remove Member';
            }
        });
    }

    // Delegate event handlers for edit/remove buttons in member list
    document.addEventListener('click', (e) => {
        // Edit button
        if (e.target.closest('.btn-edit')) {
            const btn = e.target.closest('.btn-edit');
            const did = btn.dataset.did;
            const currentRole = btn.dataset.role;

            if (editMemberModal && editMemberDid && editMemberDidDisplay && editMemberRole) {
                editMemberDid.value = did;
                editMemberDidDisplay.textContent = did;
                editMemberRole.value = currentRole || 'member';
                editMemberModal.classList.remove('hidden');
            }
        }

        // Remove button
        if (e.target.closest('.btn-remove')) {
            const btn = e.target.closest('.btn-remove');
            const did = btn.dataset.did;

            if (removeMemberModal && removeMemberDid && removeMemberDidDisplay) {
                removeMemberDid.value = did;
                removeMemberDidDisplay.textContent = did;
                removeMemberModal.classList.remove('hidden');
            }
        }
    });
}

// Initialize member management after DOM loads
document.addEventListener('DOMContentLoaded', initializeMemberManagement);

// ============================================================================
// Invite System
// ============================================================================

// Show join screen
elements.showJoinBtn?.addEventListener('click', () => {
    elements.loginScreen.classList.add('hidden');
    elements.joinScreen.classList.remove('hidden');
});

// Back to login
elements.backToLoginBtn?.addEventListener('click', () => {
    elements.joinScreen.classList.add('hidden');
    elements.loginScreen.classList.remove('hidden');
    elements.joinError.textContent = '';
    elements.joinSuccess.classList.add('hidden');
});

// Handle join via invite
document.getElementById('join-form')?.addEventListener('submit', async (e) => {
    e.preventDefault();
    
    const gatewayUrl = elements.joinGatewayUrl.value.trim();
    const inviteCode = elements.inviteCode.value.trim().toUpperCase();
    
    elements.joinError.textContent = '';
    elements.joinSuccess.classList.add('hidden');
    elements.joinBtn.disabled = true;
    elements.joinBtn.textContent = 'Joining...';
    
    try {
        // Generate a new keypair client-side
        const keypair = await generateKeypair();
        const did = `did:icn:${keypair.publicKeyBase58}`;
        
        // Join via invite
        const response = await fetch(`${gatewayUrl}/v1/invites/join`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                invite_code: inviteCode,
                did: did
            })
        });
        
        if (!response.ok) {
            const error = await response.json();
            throw new Error(error.error || 'Failed to join cooperative');
        }
        
        const data = await response.json();
        
        // Store credentials
        localStorage.setItem('icn_keypair', JSON.stringify(keypair));
        localStorage.setItem('icn_did', did);
        localStorage.setItem('icn_token', data.token);
        localStorage.setItem('icn_coop_id', data.coop_id);
        localStorage.setItem('icn_gateway_url', gatewayUrl);
        
        // Show success message with credentials
        elements.joinSuccess.innerHTML = `
            <h3>✅ Successfully Joined!</h3>
            <p><strong>Cooperative:</strong> ${data.coop_id}</p>
            <p><strong>Your DID:</strong> <code>${did}</code></p>
            <p><strong>Role:</strong> ${data.role}</p>
            <p class="help-text">Save these credentials securely. You can now sign in.</p>
            <div class="credential-box">
                <label>Private Key (keep secret!):</label>
                <textarea readonly rows="3">${keypair.privateKeyHex}</textarea>
            </div>
        `;
        elements.joinSuccess.classList.remove('hidden');
        
        // Clear form
        elements.inviteCode.value = '';
        
        // Auto-login after 3 seconds
        setTimeout(() => {
            elements.joinScreen.classList.add('hidden');
            elements.loginScreen.classList.remove('hidden');
            elements.gatewayUrl.value = gatewayUrl;
            elements.coopId.value = data.coop_id;
            elements.did.value = did;
            elements.token.value = data.token;
            // Trigger login
            document.getElementById('login-form').dispatchEvent(new Event('submit'));
        }, 3000);
        
    } catch (error) {
        console.error('Join error:', error);
        elements.joinError.textContent = error.message;
    } finally {
        elements.joinBtn.disabled = false;
        elements.joinBtn.textContent = 'Join Cooperative';
    }
});

// Generate keypair (simplified - in production use icn-identity properly)
async function generateKeypair() {
    // This is a placeholder - in a real implementation, 
    // you'd use the icn-identity library properly
    // For now, just generate random bytes and encode
    const array = new Uint8Array(32);
    crypto.getRandomValues(array);
    const hex = Array.from(array).map(b => b.toString(16).padStart(2, '0')).join('');
    const base58 = btoa(String.fromCharCode(...array))
        .replace(/\+/g, '-')
        .replace(/\//g, '_')
        .replace(/=/g, '');
    
    return {
        privateKeyHex: hex,
        publicKeyBase58: base58
    };
}

// Create invite modal
const createInviteModal = document.getElementById('create-invite-modal');
const closeCreateInvite = document.getElementById('close-create-invite');
const createInviteCancel = document.getElementById('create-invite-cancel-btn');
const createInviteSubmit = document.getElementById('create-invite-submit-btn');
const createInviteDone = document.getElementById('create-invite-done-btn');
const inviteResult = document.getElementById('invite-result');
const createInviteForm = document.getElementById('create-invite-form');

document.getElementById('create-invite-btn')?.addEventListener('click', () => {
    createInviteModal?.classList.remove('hidden');
    inviteResult?.classList.add('hidden');
    createInviteForm?.reset();
    createInviteSubmit.classList.remove('hidden');
    createInviteCancel.classList.remove('hidden');
    createInviteDone.classList.add('hidden');
});

closeCreateInvite?.addEventListener('click', () => {
    createInviteModal?.classList.add('hidden');
});

createInviteCancel?.addEventListener('click', () => {
    createInviteModal?.classList.add('hidden');
});

createInviteDone?.addEventListener('click', () => {
    createInviteModal?.classList.add('hidden');
    loadInvites(); // Refresh invite list
});

createInviteSubmit?.addEventListener('click', async () => {
    const role = document.getElementById('invite-role').value;
    const expiresIn = parseInt(document.getElementById('invite-expiration').value);
    
    createInviteSubmit.disabled = true;
    createInviteSubmit.textContent = 'Creating...';
    
    try {
        const response = await fetch(`${state.gatewayUrl}/v1/invites`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Authorization': `Bearer ${state.token}`
            },
            body: JSON.stringify({
                coop_id: state.coopId,
                role: role,
                expires_in_seconds: expiresIn
            })
        });
        
        if (!response.ok) {
            const error = await response.json();
            throw new Error(error.error || 'Failed to create invite');
        }
        
        const data = await response.json();
        
        // Show result
        document.getElementById('generated-invite-code').textContent = data.code;
        document.getElementById('generated-invite-url').value = data.invite_url;
        inviteResult.classList.remove('hidden');
        
        // Hide create button, show done button
        createInviteSubmit.classList.add('hidden');
        createInviteCancel.classList.add('hidden');
        createInviteDone.classList.remove('hidden');
        
        showToast('Invite created successfully!', 'success');
        
    } catch (error) {
        console.error('Create invite error:', error);
        showToast(error.message, 'error');
    } finally {
        createInviteSubmit.disabled = false;
        createInviteSubmit.textContent = 'Create Invite';
    }
});

// Copy invite code
document.getElementById('copy-invite-code')?.addEventListener('click', async () => {
    const code = document.getElementById('generated-invite-code').textContent;
    if (await copyToClipboard(code)) {
        showToast('Invite code copied!', 'success');
    }
});

// Copy invite URL
document.getElementById('copy-invite-url')?.addEventListener('click', async () => {
    const url = document.getElementById('generated-invite-url').value;
    if (await copyToClipboard(url)) {
        showToast('Invite link copied!', 'success');
    }
});

// Load invites
async function loadInvites() {
    const inviteList = document.getElementById('invite-list');
    if (!inviteList) return;
    
    try {
        const response = await fetch(`${state.gatewayUrl}/v1/invites?coop_id=${state.coopId}`, {
            headers: { 'Authorization': `Bearer ${state.token}` }
        });
        
        if (!response.ok) throw new Error('Failed to load invites');
        
        const data = await response.json();
        
        if (data.invites.length === 0) {
            inviteList.innerHTML = '<p class="empty-state">No invites created yet.</p>';
            return;
        }
        
        inviteList.innerHTML = data.invites.map(invite => {
            const expired = Date.now() / 1000 > invite.expires_at;
            const expiresDate = new Date(invite.expires_at * 1000).toLocaleString();
            const status = invite.used ? 'Used' : expired ? 'Expired' : 'Active';
            const statusClass = invite.used ? 'used' : expired ? 'expired' : 'active';
            
            return `
                <div class="invite-item">
                    <div class="invite-code">
                        <code>${invite.code}</code>
                        <span class="invite-status ${statusClass}">${status}</span>
                    </div>
                    <div class="invite-details">
                        <span>Role: <strong>${invite.role}</strong></span>
                        <span>Created by: ${invite.created_by}</span>
                        <span>Expires: ${expiresDate}</span>
                    </div>
                </div>
            `;
        }).join('');
        
    } catch (error) {
        console.error('Load invites error:', error);
        inviteList.innerHTML = '<p class="error">Failed to load invites</p>';
    }
}

// Load invites when members tab is shown
document.querySelector('[data-tab="members"]')?.addEventListener('click', () => {
    if (state.userRole === 'admin' || state.userRole === 'owner') {
        loadInvites();
    }
});
