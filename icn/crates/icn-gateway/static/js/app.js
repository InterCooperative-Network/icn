// ICN Gateway Application

class ICNApp {
    constructor() {
        this.client = window.icnClient;
        this.currentPage = 'login';
        this.activityFeed = [];
        this.init();
    }

    // ===== Initialization =====

    init() {
        this.setupEventListeners();
        this.setupWebSocketHandlers();

        // Check if already authenticated
        if (this.client.isAuthenticated()) {
            this.showApp();
            this.loadDashboard();
        } else {
            this.showPage('login');
        }
    }

    setupEventListeners() {
        // Login form
        document.getElementById('login-form').addEventListener('submit', (e) => {
            e.preventDefault();
            this.handleLogin();
        });

        // Logout button
        document.getElementById('logout-btn').addEventListener('click', () => {
            this.handleLogout();
        });

        // Navigation tabs
        document.querySelectorAll('.nav-tab').forEach(tab => {
            tab.addEventListener('click', () => {
                const page = tab.dataset.page;
                this.navigateTo(page);
            });
        });

        // Cooperatives
        document.getElementById('create-coop-btn').addEventListener('click', () => {
            this.showModal('create-coop-modal');
        });

        document.getElementById('create-coop-form').addEventListener('submit', (e) => {
            e.preventDefault();
            this.handleCreateCooperative();
        });

        // Governance
        this.setupGovernanceTabs();

        // Ledger
        document.getElementById('create-payment-btn').addEventListener('click', () => {
            this.showModal('create-payment-modal');
        });

        document.getElementById('create-payment-form').addEventListener('submit', (e) => {
            e.preventDefault();
            this.handleCreatePayment();
        });

        // Modal close buttons
        document.querySelectorAll('.modal-close').forEach(btn => {
            btn.addEventListener('click', () => {
                this.hideModal(btn.closest('.modal').id);
            });
        });

        // Click outside modal to close
        document.querySelectorAll('.modal').forEach(modal => {
            modal.addEventListener('click', (e) => {
                if (e.target === modal) {
                    this.hideModal(modal.id);
                }
            });
        });
    }

    setupGovernanceTabs() {
        document.querySelectorAll('.tab-btn').forEach(btn => {
            btn.addEventListener('click', () => {
                const tab = btn.dataset.tab;
                document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
                document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
                btn.classList.add('active');
                document.getElementById(`tab-${tab}`).classList.add('active');
            });
        });
    }

    setupWebSocketHandlers() {
        this.client.on('ws:connected', () => {
            this.updateConnectionStatus(true);
        });

        this.client.on('ws:disconnected', () => {
            this.updateConnectionStatus(false);
        });

        this.client.on('ws:authenticated', () => {
            this.showToast('Connected to real-time updates', 'success');
        });

        // Handle events
        this.client.on('event:PaymentCreated', (data) => {
            this.addActivity('💰', `Payment: ${data.amount} ${data.currency}`);
            this.showToast('New payment received', 'success');
            if (this.currentPage === 'ledger') {
                this.loadLedgerPage();
            }
        });

        this.client.on('event:MemberAdded', (data) => {
            this.addActivity('👤', `New member added to ${data.coop_id}`);
            if (this.currentPage === 'cooperatives') {
                this.loadCooperativesPage();
            }
        });

        this.client.on('event:GovernanceProposalCreated', (data) => {
            this.addActivity('🗳️', `New proposal: ${data.title}`);
            this.showToast('New governance proposal', 'info');
            if (this.currentPage === 'governance') {
                this.loadGovernancePage();
            }
        });

        this.client.on('event:GovernanceVoteCast', (data) => {
            this.addActivity('✅', `Vote cast on proposal`);
            if (this.currentPage === 'governance') {
                this.loadGovernancePage();
            }
        });
    }

    // ===== Authentication =====

    async handleLogin() {
        const did = document.getElementById('did-input').value.trim();
        const coopId = document.getElementById('coop-id-input').value.trim();
        const errorEl = document.getElementById('login-error');

        try {
            errorEl.style.display = 'none';
            await this.client.login(did, coopId);
            this.showApp();
            this.loadDashboard();
            this.showToast('Successfully logged in', 'success');
        } catch (error) {
            errorEl.textContent = error.message || 'Login failed';
            errorEl.style.display = 'block';
        }
    }

    handleLogout() {
        this.client.logout();
        this.showPage('login');
        this.updateConnectionStatus(false);
        document.getElementById('user-info').style.display = 'none';
        this.showToast('Logged out', 'info');
    }

    showApp() {
        this.showPage('dashboard');
        document.getElementById('user-info').style.display = 'flex';
        document.getElementById('user-did').textContent = this.client.did;
    }

    // ===== Navigation =====

    navigateTo(page) {
        this.showPage(page);

        // Update nav tabs
        document.querySelectorAll('.nav-tab').forEach(tab => {
            tab.classList.toggle('active', tab.dataset.page === page);
        });

        // Load page data
        switch (page) {
            case 'dashboard':
                this.loadDashboard();
                break;
            case 'cooperatives':
                this.loadCooperativesPage();
                break;
            case 'governance':
                this.loadGovernancePage();
                break;
            case 'ledger':
                this.loadLedgerPage();
                break;
        }
    }

    showPage(pageId) {
        this.currentPage = pageId;
        document.querySelectorAll('.page').forEach(page => {
            page.classList.toggle('active', page.id === `page-${pageId}`);
        });
    }

    // ===== Dashboard =====

    async loadDashboard() {
        try {
            // Load health status
            const health = await this.client.getHealthDetailed();
            this.renderHealthStatus(health);

            // Load stats
            await this.loadStats();
        } catch (error) {
            console.error('Failed to load dashboard:', error);
        }
    }

    async loadStats() {
        try {
            const coops = await this.client.listCooperatives();
            document.getElementById('stat-coops').textContent = coops.length;

            // Calculate total members
            let totalMembers = 0;
            for (const coop of coops) {
                const details = await this.client.getCooperative(coop.coop_id);
                totalMembers += details.members.length;
            }
            document.getElementById('stat-members').textContent = totalMembers;

            // Load proposals count
            if (this.client.coopId) {
                const domains = await this.client.listDomains(this.client.coopId);
                let proposalCount = 0;
                for (const domain of domains) {
                    const proposals = await this.client.listProposals(domain.domain_id);
                    proposalCount += proposals.filter(p => p.state === 'voting').length;
                }
                document.getElementById('stat-proposals').textContent = proposalCount;
            }

            // Load transaction count
            if (this.client.coopId) {
                const history = await this.client.getHistory(this.client.coopId, 1000);
                document.getElementById('stat-transactions').textContent = history.length;
            }
        } catch (error) {
            console.error('Failed to load stats:', error);
        }
    }

    renderHealthStatus(health) {
        const container = document.getElementById('health-status');
        const checks = health.checks || {};

        container.innerHTML = `
            <div class="health-item">
                <strong>Overall Status:</strong>
                <span class="badge badge-${health.status === 'healthy' ? 'success' : 'error'}">
                    ${health.status}
                </span>
            </div>
            <div class="health-item">
                <strong>Version:</strong> ${health.version}
            </div>
            ${Object.entries(checks).map(([key, value]) => `
                <div class="health-item">
                    <strong>${key}:</strong>
                    <span class="badge badge-${value.status === 'ok' ? 'success' : 'error'}">
                        ${value.status}
                    </span>
                    ${value.details ? `<small>${value.details}</small>` : ''}
                </div>
            `).join('')}
        `;
    }

    // ===== Cooperatives =====

    async loadCooperativesPage() {
        const container = document.getElementById('coop-list');
        container.innerHTML = '<div class="loading">Loading cooperatives...</div>';

        try {
            const coops = await this.client.listCooperatives();

            if (coops.length === 0) {
                container.innerHTML = '<div class="empty-state">No cooperatives yet</div>';
                return;
            }

            container.innerHTML = coops.map(coop => `
                <div class="card">
                    <div class="card-body">
                        <h3>${coop.name}</h3>
                        <p class="text-muted">${coop.coop_id}</p>
                        <div style="margin-top: var(--spacing-md);">
                            <span class="badge badge-info">${coop.members.length} members</span>
                            <span class="badge badge-success">
                                ${coop.owner === this.client.did ? 'Owner' : 'Member'}
                            </span>
                        </div>
                    </div>
                </div>
            `).join('');
        } catch (error) {
            container.innerHTML = '<div class="alert alert-error">Failed to load cooperatives</div>';
            console.error('Failed to load cooperatives:', error);
        }
    }

    async handleCreateCooperative() {
        const coopId = document.getElementById('new-coop-id').value.trim();
        const name = document.getElementById('new-coop-name').value.trim();

        try {
            await this.client.createCooperative(coopId, name);
            this.hideModal('create-coop-modal');
            this.showToast('Cooperative created successfully', 'success');
            this.loadCooperativesPage();
            document.getElementById('create-coop-form').reset();
        } catch (error) {
            this.showToast(error.message || 'Failed to create cooperative', 'error');
        }
    }

    // ===== Governance =====

    async loadGovernancePage() {
        await this.loadDomains();
        await this.loadProposals();
    }

    async loadDomains() {
        const container = document.getElementById('domain-list');
        container.innerHTML = '<div class="loading">Loading domains...</div>';

        try {
            const domains = await this.client.listDomains(this.client.coopId);

            if (domains.length === 0) {
                container.innerHTML = '<div class="empty-state">No governance domains yet</div>';
                return;
            }

            container.innerHTML = domains.map(domain => `
                <div class="card">
                    <div class="card-body">
                        <h3>${domain.name}</h3>
                        <p class="text-muted">${domain.domain_id}</p>
                    </div>
                </div>
            `).join('');
        } catch (error) {
            container.innerHTML = '<div class="alert alert-error">Failed to load domains</div>';
            console.error('Failed to load domains:', error);
        }
    }

    async loadProposals() {
        const container = document.getElementById('proposal-list');
        container.innerHTML = '<div class="loading">Loading proposals...</div>';

        try {
            const domains = await this.client.listDomains(this.client.coopId);
            let allProposals = [];

            for (const domain of domains) {
                const proposals = await this.client.listProposals(domain.domain_id);
                allProposals.push(...proposals.map(p => ({ ...p, domain: domain.name })));
            }

            if (allProposals.length === 0) {
                container.innerHTML = '<div class="empty-state">No proposals yet</div>';
                return;
            }

            container.innerHTML = allProposals.map(proposal => `
                <div class="list-item">
                    <div>
                        <h4>${proposal.title}</h4>
                        <p class="text-muted">${proposal.description || ''}</p>
                        <div style="margin-top: var(--spacing-sm);">
                            <span class="badge badge-${this.getProposalStateBadge(proposal.state)}">
                                ${proposal.state}
                            </span>
                            <span class="badge badge-info">${proposal.domain}</span>
                        </div>
                    </div>
                </div>
            `).join('');
        } catch (error) {
            container.innerHTML = '<div class="alert alert-error">Failed to load proposals</div>';
            console.error('Failed to load proposals:', error);
        }
    }

    getProposalStateBadge(state) {
        const badges = {
            'draft': 'info',
            'voting': 'warning',
            'closed': 'success',
            'rejected': 'error'
        };
        return badges[state] || 'info';
    }

    // ===== Ledger =====

    async loadLedgerPage() {
        await this.loadBalance();
        await this.loadTransactionHistory();
    }

    async loadBalance() {
        const container = document.getElementById('balance-display');
        container.innerHTML = '<div class="loading">Loading balance...</div>';

        try {
            const balance = await this.client.getBalance(this.client.coopId, this.client.did);

            container.innerHTML = `
                <div class="balance-card">
                    <div class="balance-amount">
                        ${balance.balance} ${balance.currency}
                    </div>
                </div>
            `;
        } catch (error) {
            container.innerHTML = '<div class="alert alert-error">Failed to load balance</div>';
            console.error('Failed to load balance:', error);
        }
    }

    async loadTransactionHistory() {
        const container = document.getElementById('transaction-list');
        container.innerHTML = '<div class="loading">Loading transactions...</div>';

        try {
            const history = await this.client.getHistory(this.client.coopId);

            if (history.length === 0) {
                container.innerHTML = '<div class="empty-state">No transactions yet</div>';
                return;
            }

            container.innerHTML = history.slice(0, 20).map(tx => `
                <div class="list-item">
                    <div>
                        <div style="display: flex; justify-content: space-between;">
                            <span>${tx.description || 'Transaction'}</span>
                            <strong>${tx.amount} ${tx.currency}</strong>
                        </div>
                        <div class="text-muted" style="font-size: 0.75rem; margin-top: 0.25rem;">
                            ${tx.from} → ${tx.to}
                        </div>
                    </div>
                </div>
            `).join('');
        } catch (error) {
            container.innerHTML = '<div class="alert alert-error">Failed to load transactions</div>';
            console.error('Failed to load transactions:', error);
        }
    }

    async handleCreatePayment() {
        const to = document.getElementById('payment-to').value.trim();
        const amount = parseInt(document.getElementById('payment-amount').value);
        const currency = document.getElementById('payment-currency').value.trim();
        const description = document.getElementById('payment-description').value.trim();

        try {
            await this.client.createPayment(this.client.coopId, to, amount, currency, description);
            this.hideModal('create-payment-modal');
            this.showToast('Payment created successfully', 'success');
            this.loadLedgerPage();
            document.getElementById('create-payment-form').reset();
        } catch (error) {
            this.showToast(error.message || 'Failed to create payment', 'error');
        }
    }

    // ===== UI Helpers =====

    showModal(modalId) {
        document.getElementById(modalId).classList.add('active');
    }

    hideModal(modalId) {
        document.getElementById(modalId).classList.remove('active');
    }

    showToast(message, type = 'info') {
        const container = document.getElementById('toast-container');
        const toast = document.createElement('div');
        toast.className = `toast ${type}`;
        toast.textContent = message;

        container.appendChild(toast);

        setTimeout(() => {
            toast.remove();
        }, 5000);
    }

    updateConnectionStatus(connected) {
        const statusEl = document.getElementById('connection-status');
        if (connected) {
            statusEl.classList.remove('offline');
            statusEl.classList.add('online');
            statusEl.querySelector('.status-text').textContent = 'Connected';
        } else {
            statusEl.classList.remove('online');
            statusEl.classList.add('offline');
            statusEl.querySelector('.status-text').textContent = 'Disconnected';
        }
    }

    addActivity(icon, text) {
        this.activityFeed.unshift({ icon, text, time: new Date() });
        this.activityFeed = this.activityFeed.slice(0, 10); // Keep last 10

        const container = document.getElementById('activity-feed');
        if (this.activityFeed.length === 0) {
            container.innerHTML = '<div class="empty-state">No recent activity</div>';
        } else {
            container.innerHTML = this.activityFeed.map(activity => `
                <div class="activity-item">
                    <div>${activity.icon} ${activity.text}</div>
                    <div class="activity-time">${this.formatTime(activity.time)}</div>
                </div>
            `).join('');
        }
    }

    formatTime(date) {
        const now = new Date();
        const diff = Math.floor((now - date) / 1000);

        if (diff < 60) return 'Just now';
        if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
        if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
        return `${Math.floor(diff / 86400)}d ago`;
    }
}

// Initialize app when DOM is ready
document.addEventListener('DOMContentLoaded', () => {
    window.app = new ICNApp();
});
