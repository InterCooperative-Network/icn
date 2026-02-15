// ICN Node Dashboard Application

class ICNDashboard {
    constructor() {
        this.apiEndpoint = localStorage.getItem('api-endpoint') || 'http://localhost:8080';
        this.wsEndpoint = localStorage.getItem('ws-endpoint') || 'ws://localhost:8080/v1/ws';
        this.coopId = localStorage.getItem('coop-id') || 'default';
        this.refreshInterval = parseInt(localStorage.getItem('refresh-interval') || '5', 10);
        this.autoRefresh = localStorage.getItem('auto-refresh') !== 'false';

        this.ws = null;
        this.refreshTimer = null;
        this.currentView = 'overview';

        this.init();
    }

    init() {
        this.setupNavigation();
        this.loadSettings();
        this.connectWebSocket();
        this.fetchInitialData();

        if (this.autoRefresh) {
            this.startAutoRefresh();
        }
    }

    /**
     * Escape HTML special characters to prevent XSS.
     * Must be applied to ALL dynamic strings before innerHTML insertion.
     */
    escapeHtml(str) {
        if (str == null) return '';
        return String(str)
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#039;');
    }

    setupNavigation() {
        document.querySelectorAll('.nav-item').forEach(item => {
            item.addEventListener('click', (e) => {
                e.preventDefault();
                const view = e.currentTarget.dataset.view;
                this.switchView(view);
            });
        });
    }

    switchView(viewName) {
        // Update nav
        document.querySelectorAll('.nav-item').forEach(item => {
            item.classList.remove('active');
        });
        document.querySelector(`[data-view="${viewName}"]`).classList.add('active');

        // Update content
        document.querySelectorAll('.view').forEach(view => {
            view.classList.remove('active');
        });
        document.getElementById(`${viewName}-view`).classList.add('active');

        // Update title
        const titles = {
            overview: 'Overview',
            network: 'Network',
            ledger: 'Ledger',
            governance: 'Governance',
            compute: 'Compute Tasks',
            federation: 'Federation',
            metrics: 'System Metrics',
            logs: 'System Logs',
            settings: 'Settings'
        };
        document.getElementById('view-title').textContent = titles[viewName];

        this.currentView = viewName;
        this.loadViewData(viewName);
    }

    async loadViewData(viewName) {
        switch (viewName) {
            case 'overview':
                await this.loadOverview();
                break;
            case 'network':
                await this.loadPeers();
                break;
            case 'ledger':
                await this.loadLedger();
                break;
            case 'governance':
                await this.loadProposals();
                break;
            case 'compute':
                await this.loadTasks();
                break;
            case 'federation':
                await this.loadFederation();
                break;
            case 'metrics':
                await this.loadMetrics();
                break;
            case 'logs':
                await this.loadLogs();
                break;
        }
    }

    async fetchInitialData() {
        await this.getNodeInfo();
        await this.loadOverview();
    }

    async getNodeInfo() {
        try {
            // /v1/health/detailed returns node status including version and uptime
            const response = await fetch(`${this.apiEndpoint}/v1/health/detailed`);
            if (!response.ok) throw new Error('Failed to fetch node info');

            const data = await response.json();

            document.getElementById('node-did').textContent = data.node_id || data.did || 'Unknown';
            this.updateStatus('online');

            return data;
        } catch (error) {
            console.error('Failed to get node info:', error);
            this.updateStatus('offline');
            this.showToast('Failed to connect to node', 'error');
        }
    }

    async loadOverview() {
        try {
            // Fetch stats from actual gateway endpoints
            const [ledger, proposals, wasm, federation] = await Promise.all([
                this.fetchAPI(`/v1/ledger/${encodeURIComponent(this.coopId)}/history`),
                this.fetchAPI('/v1/gov/proposals'),
                this.fetchAPI('/v1/compute/wasm'),
                this.fetchAPI('/v1/federation/coops')
            ]);

            // Update stats
            document.getElementById('peer-count').textContent = federation?.length || 0;
            document.getElementById('ledger-entries').textContent = ledger?.entries?.length || ledger?.length || 0;
            document.getElementById('active-proposals').textContent =
                proposals?.filter(p => p.status === 'active')?.length || 0;
            document.getElementById('compute-tasks').textContent = wasm?.length || 0;

            // Load recent activity
            await this.loadRecentActivity();

        } catch (error) {
            console.error('Failed to load overview:', error);
        }
    }

    async loadRecentActivity() {
        try {
            const response = await this.fetchAPI(
                `/v1/ledger/${encodeURIComponent(this.coopId)}/history?limit=10`
            );
            const activityFeed = document.getElementById('activity-feed');

            const entries = response?.entries || response;
            if (!entries || entries.length === 0) {
                activityFeed.innerHTML = '<p class="loading">No recent activity</p>';
                return;
            }

            activityFeed.innerHTML = entries.map(entry => `
                <div class="activity-item">
                    <div>
                        <strong>${this.escapeHtml(this.formatDid(entry.from_did))}</strong> &rarr;
                        <strong>${this.escapeHtml(this.formatDid(entry.to_did))}</strong>
                        <div class="activity-time">${this.escapeHtml(entry.amount)} credits</div>
                    </div>
                    <div class="activity-time">${this.escapeHtml(this.formatTime(entry.timestamp))}</div>
                </div>
            `).join('');
        } catch (error) {
            console.error('Failed to load recent activity:', error);
        }
    }

    async loadPeers() {
        try {
            // Use trust edges as a proxy for network peers
            const peers = await this.fetchAPI('/v1/trust/edges');
            const peersList = document.getElementById('peers-list');

            if (!peers || peers.length === 0) {
                peersList.innerHTML = '<p class="loading">No connected peers</p>';
                return;
            }

            peersList.innerHTML = `
                <table class="table">
                    <thead>
                        <tr>
                            <th>DID</th>
                            <th>Address</th>
                            <th>Trust Score</th>
                            <th>Connected Since</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${peers.map(peer => `
                            <tr>
                                <td><code>${this.escapeHtml(this.formatDid(peer.did || peer.target))}</code></td>
                                <td>${this.escapeHtml(peer.address || '-')}</td>
                                <td>${this.escapeHtml(peer.trust_score != null ? (peer.trust_score * 100).toFixed(1) + '%' : (peer.weight != null ? (peer.weight * 100).toFixed(1) + '%' : '-'))}</td>
                                <td>${this.escapeHtml(this.formatTime(peer.connected_at || peer.created_at))}</td>
                            </tr>
                        `).join('')}
                    </tbody>
                </table>
            `;
        } catch (error) {
            console.error('Failed to load peers:', error);
            document.getElementById('peers-list').innerHTML =
                '<p class="loading">Failed to load peers</p>';
        }
    }

    async loadLedger() {
        try {
            const response = await this.fetchAPI(
                `/v1/ledger/${encodeURIComponent(this.coopId)}/history`
            );
            const ledgerList = document.getElementById('ledger-list');

            const entries = response?.entries || response;
            if (!entries || entries.length === 0) {
                ledgerList.innerHTML = '<p class="loading">No ledger entries</p>';
                return;
            }

            ledgerList.innerHTML = `
                <table class="table">
                    <thead>
                        <tr>
                            <th>From</th>
                            <th>To</th>
                            <th>Amount</th>
                            <th>Description</th>
                            <th>Timestamp</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${entries.map(entry => `
                            <tr>
                                <td><code>${this.escapeHtml(this.formatDid(entry.from_did))}</code></td>
                                <td><code>${this.escapeHtml(this.formatDid(entry.to_did))}</code></td>
                                <td>${this.escapeHtml(entry.amount)}</td>
                                <td>${this.escapeHtml(entry.description || '-')}</td>
                                <td>${this.escapeHtml(this.formatTime(entry.timestamp))}</td>
                            </tr>
                        `).join('')}
                    </tbody>
                </table>
            `;
        } catch (error) {
            console.error('Failed to load ledger:', error);
            document.getElementById('ledger-list').innerHTML =
                '<p class="loading">Failed to load ledger entries</p>';
        }
    }

    async loadProposals() {
        try {
            const proposals = await this.fetchAPI('/v1/gov/proposals');
            const proposalsList = document.getElementById('proposals-list');

            if (!proposals || proposals.length === 0) {
                proposalsList.innerHTML = '<p class="loading">No governance proposals</p>';
                return;
            }

            proposalsList.innerHTML = proposals.map(proposal => `
                <div class="proposal-card">
                    <h4 class="proposal-title">${this.escapeHtml(proposal.title)}</h4>
                    <div class="proposal-meta">
                        Status: ${this.escapeHtml(proposal.status)} |
                        Created: ${this.escapeHtml(this.formatTime(proposal.created_at))}
                    </div>
                    <p>${this.escapeHtml(proposal.description)}</p>
                    <div class="proposal-votes">
                        <span class="vote-badge for">For: ${this.escapeHtml(proposal.votes_for || 0)}</span>
                        <span class="vote-badge against">Against: ${this.escapeHtml(proposal.votes_against || 0)}</span>
                        <span class="vote-badge abstain">Abstain: ${this.escapeHtml(proposal.votes_abstain || 0)}</span>
                    </div>
                </div>
            `).join('');
        } catch (error) {
            console.error('Failed to load proposals:', error);
            document.getElementById('proposals-list').innerHTML =
                '<p class="loading">Failed to load proposals</p>';
        }
    }

    async loadTasks() {
        try {
            // /v1/compute/wasm lists available WASM modules
            const tasks = await this.fetchAPI('/v1/compute/wasm');
            const tasksList = document.getElementById('tasks-list');

            if (!tasks || tasks.length === 0) {
                tasksList.innerHTML = '<p class="loading">No compute tasks</p>';
                return;
            }

            tasksList.innerHTML = `
                <table class="table">
                    <thead>
                        <tr>
                            <th>Hash</th>
                            <th>Name</th>
                            <th>Submitted By</th>
                            <th>Size</th>
                            <th>Created</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${tasks.map(task => `
                            <tr>
                                <td><code>${this.escapeHtml(task.hash ? task.hash.substring(0, 8) : (task.id ? task.id.substring(0, 8) : '-'))}</code></td>
                                <td>${this.escapeHtml(task.name || task.status || '-')}</td>
                                <td><code>${this.escapeHtml(this.formatDid(task.submitted_by || task.uploader))}</code></td>
                                <td>${this.escapeHtml(task.size ? Math.round(task.size / 1024) + ' KB' : '-')}</td>
                                <td>${this.escapeHtml(this.formatTime(task.created_at || task.uploaded_at))}</td>
                            </tr>
                        `).join('')}
                    </tbody>
                </table>
            `;
        } catch (error) {
            console.error('Failed to load tasks:', error);
            document.getElementById('tasks-list').innerHTML =
                '<p class="loading">Failed to load compute tasks</p>';
        }
    }

    async loadFederation() {
        try {
            const coops = await this.fetchAPI('/v1/federation/coops');
            const federationList = document.getElementById('federation-list');

            if (!coops || coops.length === 0) {
                federationList.innerHTML = '<p class="loading">No federated cooperatives</p>';
                return;
            }

            federationList.innerHTML = `
                <table class="table">
                    <thead>
                        <tr>
                            <th>Cooperative ID</th>
                            <th>Name</th>
                            <th>Gateway</th>
                            <th>Last Seen</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${coops.map(coop => `
                            <tr>
                                <td>${this.escapeHtml(coop.coop_id || coop.id)}</td>
                                <td>${this.escapeHtml(coop.name || '-')}</td>
                                <td>${this.escapeHtml((coop.gateway_endpoints && coop.gateway_endpoints[0]) || coop.gateway || '-')}</td>
                                <td>${this.escapeHtml(this.formatTime(coop.last_seen))}</td>
                            </tr>
                        `).join('')}
                    </tbody>
                </table>
            `;
        } catch (error) {
            console.error('Failed to load federation:', error);
            document.getElementById('federation-list').innerHTML =
                '<p class="loading">Failed to load federation info</p>';
        }
    }

    async loadMetrics() {
        try {
            // Prometheus metrics are at /metrics (outside /v1 scope)
            const response = await fetch(`${this.apiEndpoint}/metrics`);
            if (!response.ok) throw new Error('Failed to fetch metrics');
            const text = await response.text();

            // Parse Prometheus text format for key metrics
            const getMetric = (name) => {
                const match = text.match(new RegExp(`^${name}\\s+(\\S+)`, 'm'));
                return match ? parseFloat(match[1]) : 0;
            };

            document.getElementById('gossip-msgs').textContent =
                getMetric('icn_gateway_http_requests_total').toFixed(0);
            document.getElementById('trust-edges').textContent =
                getMetric('icn_gateway_active_websockets') || 0;
            document.getElementById('storage-used').textContent = '-';
            document.getElementById('bandwidth').textContent = '-';
        } catch (error) {
            console.error('Failed to load metrics:', error);
        }
    }

    async loadLogs() {
        // No log streaming endpoint exists in the gateway API.
        // Logs are available via `kubectl logs` or Prometheus/Loki in production.
        const logsContainer = document.getElementById('logs-container');
        logsContainer.innerHTML = '<p class="loading">Log streaming not available via gateway API. Use kubectl logs or Loki.</p>';
    }

    async fetchAPI(path) {
        try {
            const token = localStorage.getItem('dashboard-token') || '';
            const headers = {};
            if (token) {
                headers['Authorization'] = `Bearer ${token}`;
            }
            const response = await fetch(`${this.apiEndpoint}${path}`, { headers });
            if (!response.ok) throw new Error(`HTTP ${response.status}`);
            return await response.json();
        } catch (error) {
            console.error(`API fetch failed for ${path}:`, error);
            return null;
        }
    }

    connectWebSocket() {
        try {
            this.ws = new WebSocket(this.wsEndpoint);

            this.ws.onopen = () => {
                console.log('WebSocket connected');
                this.updateStatus('online');
            };

            this.ws.onmessage = (event) => {
                const data = JSON.parse(event.data);
                this.handleWebSocketMessage(data);
            };

            this.ws.onerror = (error) => {
                console.error('WebSocket error:', error);
                this.updateStatus('offline');
            };

            this.ws.onclose = () => {
                console.log('WebSocket disconnected');
                this.updateStatus('offline');

                // Reconnect after 5 seconds
                setTimeout(() => this.connectWebSocket(), 5000);
            };
        } catch (error) {
            console.error('Failed to connect WebSocket:', error);
            this.updateStatus('offline');
        }
    }

    handleWebSocketMessage(data) {
        // Handle real-time updates
        if (data.type === 'ledger_entry') {
            this.loadRecentActivity();
            if (this.currentView === 'ledger') {
                this.loadLedger();
            }
        } else if (data.type === 'proposal_update') {
            if (this.currentView === 'governance') {
                this.loadProposals();
            }
        } else if (data.type === 'task_update') {
            if (this.currentView === 'compute') {
                this.loadTasks();
            }
        }
    }

    updateStatus(status) {
        const statusBadge = document.getElementById('node-status');
        const dot = statusBadge.querySelector('.status-dot');
        const text = statusBadge.querySelector('span:last-child');

        dot.className = `status-dot status-${status}`;

        const statusText = {
            online: 'Online',
            offline: 'Offline',
            unknown: 'Connecting...'
        };

        text.textContent = statusText[status] || 'Unknown';
    }

    startAutoRefresh() {
        this.refreshTimer = setInterval(() => {
            this.loadViewData(this.currentView);
        }, this.refreshInterval * 1000);
    }

    stopAutoRefresh() {
        if (this.refreshTimer) {
            clearInterval(this.refreshTimer);
            this.refreshTimer = null;
        }
    }

    loadSettings() {
        document.getElementById('api-endpoint').value = this.apiEndpoint;
        document.getElementById('ws-endpoint').value = this.wsEndpoint;
        const coopIdInput = document.getElementById('coop-id');
        if (coopIdInput) coopIdInput.value = this.coopId;
        document.getElementById('refresh-interval').value = this.refreshInterval;
        document.getElementById('auto-refresh').checked = this.autoRefresh;
    }

    saveSettings() {
        this.apiEndpoint = document.getElementById('api-endpoint').value;
        this.wsEndpoint = document.getElementById('ws-endpoint').value;
        const coopIdInput = document.getElementById('coop-id');
        if (coopIdInput) this.coopId = coopIdInput.value;
        this.refreshInterval = parseInt(document.getElementById('refresh-interval').value, 10);
        this.autoRefresh = document.getElementById('auto-refresh').checked;

        localStorage.setItem('api-endpoint', this.apiEndpoint);
        localStorage.setItem('ws-endpoint', this.wsEndpoint);
        localStorage.setItem('coop-id', this.coopId);
        localStorage.setItem('refresh-interval', this.refreshInterval.toString());
        localStorage.setItem('auto-refresh', this.autoRefresh.toString());

        this.showToast('Settings saved successfully', 'success');

        // Reconnect WebSocket with new endpoint
        if (this.ws) {
            this.ws.close();
        }
        this.connectWebSocket();

        // Restart auto-refresh
        this.stopAutoRefresh();
        if (this.autoRefresh) {
            this.startAutoRefresh();
        }
    }

    refreshPeers() {
        this.loadPeers();
        this.showToast('Peers refreshed', 'success');
    }

    refreshFederation() {
        this.loadFederation();
        this.showToast('Federation info refreshed', 'success');
    }

    filterLedger(period) {
        // TODO: Implement filtering logic
        this.showToast(`Filtering ledger: ${period}`, 'info');
    }

    filterTasks(status) {
        // TODO: Implement filtering logic
        this.showToast(`Filtering tasks: ${status}`, 'info');
    }

    filterLogs(level) {
        // TODO: Implement filtering logic
        this.showToast(`Filtering logs: ${level}`, 'info');
    }

    exportLedger() {
        // TODO: Implement CSV export
        this.showToast('Export functionality coming soon', 'info');
    }

    showCreateProposal() {
        // TODO: Implement proposal creation modal
        this.showToast('Proposal creation coming soon', 'info');
    }

    clearLogs() {
        document.getElementById('logs-container').innerHTML =
            '<p class="loading">Logs cleared</p>';
        this.showToast('Logs cleared', 'success');
    }

    showToast(message, type = 'info') {
        const container = document.getElementById('toast-container');
        const toast = document.createElement('div');
        toast.className = `toast ${type}`;
        toast.textContent = message;

        container.appendChild(toast);

        setTimeout(() => {
            toast.style.opacity = '0';
            setTimeout(() => toast.remove(), 300);
        }, 5000);
    }

    formatDid(did) {
        if (!did) return '-';
        if (did.length > 20) {
            return did.substring(0, 12) + '...' + did.substring(did.length - 8);
        }
        return did;
    }

    formatTime(timestamp) {
        if (!timestamp) return '-';
        const date = new Date(typeof timestamp === 'number' && timestamp < 1e12 ? timestamp * 1000 : timestamp);
        return date.toLocaleString();
    }
}

// Initialize the dashboard
const app = new ICNDashboard();
