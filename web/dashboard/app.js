// ICN Node Dashboard Application

class ICNDashboard {
    constructor() {
        this.apiEndpoint = localStorage.getItem('api-endpoint') || 'http://localhost:8000';
        this.wsEndpoint = localStorage.getItem('ws-endpoint') || 'ws://localhost:8000/ws';
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
            const response = await fetch(`${this.apiEndpoint}/v1/node/info`);
            if (!response.ok) throw new Error('Failed to fetch node info');
            
            const data = await response.json();
            
            document.getElementById('node-did').textContent = data.did || 'Unknown';
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
            // Fetch stats
            const [peers, ledger, proposals, tasks] = await Promise.all([
                this.fetchAPI('/v1/network/peers'),
                this.fetchAPI('/v1/ledger/entries'),
                this.fetchAPI('/v1/governance/proposals'),
                this.fetchAPI('/v1/compute/tasks')
            ]);
            
            // Update stats
            document.getElementById('peer-count').textContent = peers?.length || 0;
            document.getElementById('ledger-entries').textContent = ledger?.length || 0;
            document.getElementById('active-proposals').textContent = 
                proposals?.filter(p => p.status === 'active')?.length || 0;
            document.getElementById('compute-tasks').textContent = 
                tasks?.filter(t => t.status === 'running')?.length || 0;
            
            // Load recent activity
            await this.loadRecentActivity();
            
        } catch (error) {
            console.error('Failed to load overview:', error);
        }
    }

    async loadRecentActivity() {
        try {
            const response = await this.fetchAPI('/v1/ledger/entries?limit=10');
            const activityFeed = document.getElementById('activity-feed');
            
            if (!response || response.length === 0) {
                activityFeed.innerHTML = '<p class="loading">No recent activity</p>';
                return;
            }
            
            activityFeed.innerHTML = response.map(entry => `
                <div class="activity-item">
                    <div>
                        <strong>${this.formatDid(entry.from_did)}</strong> → 
                        <strong>${this.formatDid(entry.to_did)}</strong>
                        <div class="activity-time">${entry.amount} credits</div>
                    </div>
                    <div class="activity-time">${this.formatTime(entry.timestamp)}</div>
                </div>
            `).join('');
        } catch (error) {
            console.error('Failed to load recent activity:', error);
        }
    }

    async loadPeers() {
        try {
            const peers = await this.fetchAPI('/v1/network/peers');
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
                                <td><code>${this.formatDid(peer.did)}</code></td>
                                <td>${peer.address}</td>
                                <td>${(peer.trust_score * 100).toFixed(1)}%</td>
                                <td>${this.formatTime(peer.connected_at)}</td>
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
            const entries = await this.fetchAPI('/v1/ledger/entries');
            const ledgerList = document.getElementById('ledger-list');
            
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
                                <td><code>${this.formatDid(entry.from_did)}</code></td>
                                <td><code>${this.formatDid(entry.to_did)}</code></td>
                                <td>${entry.amount}</td>
                                <td>${entry.description || '-'}</td>
                                <td>${this.formatTime(entry.timestamp)}</td>
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
            const proposals = await this.fetchAPI('/v1/governance/proposals');
            const proposalsList = document.getElementById('proposals-list');
            
            if (!proposals || proposals.length === 0) {
                proposalsList.innerHTML = '<p class="loading">No governance proposals</p>';
                return;
            }
            
            proposalsList.innerHTML = proposals.map(proposal => `
                <div class="proposal-card">
                    <h4 class="proposal-title">${proposal.title}</h4>
                    <div class="proposal-meta">
                        Status: ${proposal.status} | 
                        Created: ${this.formatTime(proposal.created_at)}
                    </div>
                    <p>${proposal.description}</p>
                    <div class="proposal-votes">
                        <span class="vote-badge for">For: ${proposal.votes_for || 0}</span>
                        <span class="vote-badge against">Against: ${proposal.votes_against || 0}</span>
                        <span class="vote-badge abstain">Abstain: ${proposal.votes_abstain || 0}</span>
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
            const tasks = await this.fetchAPI('/v1/compute/tasks');
            const tasksList = document.getElementById('tasks-list');
            
            if (!tasks || tasks.length === 0) {
                tasksList.innerHTML = '<p class="loading">No compute tasks</p>';
                return;
            }
            
            tasksList.innerHTML = `
                <table class="table">
                    <thead>
                        <tr>
                            <th>Task ID</th>
                            <th>Status</th>
                            <th>Submitted By</th>
                            <th>Executor</th>
                            <th>Created</th>
                        </tr>
                    </thead>
                    <tbody>
                        ${tasks.map(task => `
                            <tr>
                                <td><code>${task.id.substring(0, 8)}</code></td>
                                <td>${task.status}</td>
                                <td><code>${this.formatDid(task.submitted_by)}</code></td>
                                <td>${task.executor ? this.formatDid(task.executor) : '-'}</td>
                                <td>${this.formatTime(task.created_at)}</td>
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
            const coops = await this.fetchAPI('/v1/federation/cooperatives');
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
                                <td>${coop.coop_id}</td>
                                <td>${coop.name}</td>
                                <td>${coop.gateway_endpoints[0] || '-'}</td>
                                <td>${this.formatTime(coop.last_seen)}</td>
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
            const metrics = await this.fetchAPI('/v1/metrics');
            
            if (!metrics) return;
            
            document.getElementById('gossip-msgs').textContent = 
                (metrics.gossip_messages_per_sec || 0).toFixed(2);
            document.getElementById('trust-edges').textContent = 
                metrics.trust_graph_edges || 0;
            document.getElementById('storage-used').textContent = 
                ((metrics.storage_bytes || 0) / 1024 / 1024).toFixed(2);
            document.getElementById('bandwidth').textContent = 
                ((metrics.network_bytes_per_sec || 0) / 1024).toFixed(2);
        } catch (error) {
            console.error('Failed to load metrics:', error);
        }
    }

    async loadLogs() {
        try {
            const logs = await this.fetchAPI('/v1/logs?limit=100');
            const logsContainer = document.getElementById('logs-container');
            
            if (!logs || logs.length === 0) {
                logsContainer.innerHTML = '<p class="loading">No logs available</p>';
                return;
            }
            
            logsContainer.innerHTML = logs.map(log => `
                <div class="log-entry ${log.level}">
                    [${this.formatTime(log.timestamp)}] ${log.level.toUpperCase()}: ${log.message}
                </div>
            `).join('');
            
            // Scroll to bottom
            logsContainer.scrollTop = logsContainer.scrollHeight;
        } catch (error) {
            console.error('Failed to load logs:', error);
            document.getElementById('logs-container').innerHTML = 
                '<p class="loading">Failed to load logs</p>';
        }
    }

    async fetchAPI(path) {
        try {
            const response = await fetch(`${this.apiEndpoint}${path}`);
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
        document.getElementById('refresh-interval').value = this.refreshInterval;
        document.getElementById('auto-refresh').checked = this.autoRefresh;
    }

    saveSettings() {
        this.apiEndpoint = document.getElementById('api-endpoint').value;
        this.wsEndpoint = document.getElementById('ws-endpoint').value;
        this.refreshInterval = parseInt(document.getElementById('refresh-interval').value, 10);
        this.autoRefresh = document.getElementById('auto-refresh').checked;
        
        localStorage.setItem('api-endpoint', this.apiEndpoint);
        localStorage.setItem('ws-endpoint', this.wsEndpoint);
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
        const date = new Date(timestamp * 1000);
        return date.toLocaleString();
    }
}

// Initialize the dashboard
const app = new ICNDashboard();
