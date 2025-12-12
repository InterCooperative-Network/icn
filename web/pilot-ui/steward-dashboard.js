// Steward Dashboard JavaScript

class StewardDashboard {
    constructor() {
        this.apiBase = this.detectApiBase();
        this.currentEnrollment = null;
        this.enrollments = [];
        this.vouchHistory = [];

        this.init();
    }

    detectApiBase() {
        // Use current origin or fallback to gateway
        if (window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1') {
            return 'http://10.8.10.40:30080';
        }
        return window.location.origin;
    }

    async init() {
        this.setupEventListeners();
        this.loadStewardInfo();
        await this.loadPendingEnrollments();
        this.loadVouchHistory();
    }

    setupEventListeners() {
        // Tab navigation
        document.querySelectorAll('.nav-btn').forEach(btn => {
            btn.addEventListener('click', () => this.switchTab(btn.dataset.tab));
        });

        // Refresh button
        document.getElementById('refreshPending').addEventListener('click', () => {
            this.loadPendingEnrollments();
        });

        // Filters
        document.getElementById('coopFilter').addEventListener('change', () => this.filterEnrollments());
        document.getElementById('levelFilter').addEventListener('change', () => this.filterEnrollments());

        // Modal close buttons
        document.getElementById('closeModal').addEventListener('click', () => this.closeModal('enrollmentModal'));
        document.getElementById('closeVouchModal').addEventListener('click', () => this.closeModal('vouchModal'));

        // Vouch buttons
        document.getElementById('vouchBtn').addEventListener('click', () => this.openVouchModal());
        document.getElementById('cancelVouch').addEventListener('click', () => this.closeModal('vouchModal'));
        document.getElementById('confirmVouch').addEventListener('click', () => this.submitVouch());

        // Reject button
        document.getElementById('rejectBtn').addEventListener('click', () => this.rejectEnrollment());

        // Vouch form validation
        document.getElementById('vouchStatement').addEventListener('input', () => this.validateVouchForm());
        document.getElementById('check1').addEventListener('change', () => this.validateVouchForm());
        document.getElementById('check2').addEventListener('change', () => this.validateVouchForm());

        // Close modals on backdrop click
        document.querySelectorAll('.modal').forEach(modal => {
            modal.addEventListener('click', (e) => {
                if (e.target === modal) {
                    this.closeModal(modal.id);
                }
            });
        });
    }

    switchTab(tabName) {
        // Update nav buttons
        document.querySelectorAll('.nav-btn').forEach(btn => {
            btn.classList.toggle('active', btn.dataset.tab === tabName);
        });

        // Update tab content
        document.querySelectorAll('.tab-content').forEach(tab => {
            tab.classList.toggle('active', tab.id === `${tabName}-tab`);
        });

        // Refresh data for the tab
        if (tabName === 'pending') {
            this.loadPendingEnrollments();
        } else if (tabName === 'history') {
            this.loadVouchHistory();
        }
    }

    loadStewardInfo() {
        // TODO: Load from auth token
        const stewardId = localStorage.getItem('stewardDid') || 'did:icn:steward-' + Math.random().toString(36).substr(2, 8);
        document.getElementById('stewardId').textContent = stewardId.substring(0, 20) + '...';
    }

    async loadPendingEnrollments() {
        const listEl = document.getElementById('enrollmentList');
        listEl.innerHTML = '<div class="loading">Loading pending enrollments...</div>';

        try {
            const response = await fetch(`${this.apiBase}/v1/sdis/pending`);
            if (!response.ok) throw new Error('Failed to load enrollments');

            const data = await response.json();
            this.enrollments = data.enrollments || [];

            document.getElementById('pendingCount').textContent = this.enrollments.length;
            this.renderEnrollments();
        } catch (error) {
            console.error('Error loading enrollments:', error);
            listEl.innerHTML = `
                <div class="empty-state">
                    <p>Error loading enrollments. Please try again.</p>
                    <button class="btn btn-secondary" onclick="dashboard.loadPendingEnrollments()">Retry</button>
                </div>
            `;
        }
    }

    filterEnrollments() {
        this.renderEnrollments();
    }

    renderEnrollments() {
        const listEl = document.getElementById('enrollmentList');
        const coopFilter = document.getElementById('coopFilter').value;
        const levelFilter = document.getElementById('levelFilter').value;

        let filtered = this.enrollments;

        if (coopFilter !== 'all') {
            filtered = filtered.filter(e => e.coop_id === coopFilter);
        }

        if (levelFilter !== 'all') {
            filtered = filtered.filter(e => e.level === parseInt(levelFilter));
        }

        if (filtered.length === 0) {
            listEl.innerHTML = `
                <div class="empty-state">
                    <p>No pending enrollments to review.</p>
                </div>
            `;
            return;
        }

        listEl.innerHTML = filtered.map(enrollment => `
            <div class="enrollment-card" onclick="dashboard.openEnrollmentDetail('${enrollment.enrollment_id}')">
                <div class="enrollment-info">
                    <div class="enrollment-name">${this.escapeHtml(enrollment.identity_name)}</div>
                    <div class="enrollment-meta">
                        <span>Coop: ${this.escapeHtml(enrollment.coop_id)}</span>
                        <span>Created: ${this.formatDate(enrollment.created_at)}</span>
                        <span>Expires: ${this.formatDate(enrollment.expires_at)}</span>
                    </div>
                </div>
                <div class="enrollment-status">
                    <span class="level-badge level-${enrollment.level}">
                        Level ${enrollment.level}
                    </span>
                </div>
            </div>
        `).join('');
    }

    async openEnrollmentDetail(enrollmentId) {
        try {
            const response = await fetch(`${this.apiBase}/v1/sdis/status/${enrollmentId}`);
            if (!response.ok) throw new Error('Failed to load enrollment details');

            this.currentEnrollment = await response.json();

            const detailsEl = document.getElementById('enrollmentDetails');
            detailsEl.innerHTML = `
                <div class="detail-item">
                    <label>Enrollment ID</label>
                    <div class="value">${this.currentEnrollment.enrollment_id}</div>
                </div>
                <div class="detail-item">
                    <label>Identity Name</label>
                    <div class="value">${this.escapeHtml(this.currentEnrollment.identity_name)}</div>
                </div>
                <div class="detail-item">
                    <label>Cooperative</label>
                    <div class="value">${this.escapeHtml(this.currentEnrollment.coop_id)}</div>
                </div>
                <div class="detail-item">
                    <label>Verification Level</label>
                    <div class="value">
                        <span class="level-badge level-${this.currentEnrollment.level}">
                            Level ${this.currentEnrollment.level}
                        </span>
                        - ${this.getLevelDescription(this.currentEnrollment.level)}
                    </div>
                </div>
                <div class="detail-item">
                    <label>Status</label>
                    <div class="value">${this.formatStatus(this.currentEnrollment.status)}</div>
                </div>
                <div class="detail-item">
                    <label>Created</label>
                    <div class="value">${this.formatDate(this.currentEnrollment.created_at)}</div>
                </div>
                <div class="detail-item">
                    <label>Expires</label>
                    <div class="value">${this.formatDate(this.currentEnrollment.expires_at)}</div>
                </div>
            `;

            // Update button states
            const vouchBtn = document.getElementById('vouchBtn');
            if (this.currentEnrollment.level < 1) {
                vouchBtn.disabled = true;
                vouchBtn.textContent = 'Awaiting Device Verification';
            } else if (this.currentEnrollment.has_steward_vouch) {
                vouchBtn.disabled = true;
                vouchBtn.textContent = 'Already Vouched';
            } else {
                vouchBtn.disabled = false;
                vouchBtn.textContent = 'Vouch for Identity';
            }

            this.openModal('enrollmentModal');
        } catch (error) {
            console.error('Error loading enrollment details:', error);
            this.showToast('Failed to load enrollment details', 'error');
        }
    }

    getLevelDescription(level) {
        switch(level) {
            case 0: return 'New enrollment, awaiting device verification';
            case 1: return 'Device verified, awaiting steward vouch';
            case 2: return 'Ready for completion';
            default: return 'Unknown';
        }
    }

    formatStatus(status) {
        const statusMap = {
            'pending_device_verification': 'Pending Device Verification',
            'pending_steward_vouch': 'Pending Steward Vouch',
            'ready_for_completion': 'Ready for Completion'
        };
        return statusMap[status] || status;
    }

    openVouchModal() {
        document.getElementById('vouchStatement').value = '';
        document.getElementById('check1').checked = false;
        document.getElementById('check2').checked = false;
        document.getElementById('confirmVouch').disabled = true;

        this.closeModal('enrollmentModal');
        this.openModal('vouchModal');
    }

    validateVouchForm() {
        const statement = document.getElementById('vouchStatement').value.trim();
        const check1 = document.getElementById('check1').checked;
        const check2 = document.getElementById('check2').checked;

        const isValid = statement.length >= 10 && check1 && check2;
        document.getElementById('confirmVouch').disabled = !isValid;
    }

    async submitVouch() {
        if (!this.currentEnrollment) return;

        const statement = document.getElementById('vouchStatement').value.trim();
        const enrollmentId = this.currentEnrollment.enrollment_id;

        try {
            document.getElementById('confirmVouch').disabled = true;
            document.getElementById('confirmVouch').textContent = 'Submitting...';

            const response = await fetch(`${this.apiBase}/v1/sdis/vouch/${enrollmentId}`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify({
                    vouch_statement: statement,
                    steward_did: localStorage.getItem('stewardDid') || null
                })
            });

            if (!response.ok) {
                const error = await response.json();
                throw new Error(error.error || 'Failed to submit vouch');
            }

            const result = await response.json();

            this.showToast(`Successfully vouched for ${this.currentEnrollment.identity_name}`, 'success');
            this.closeModal('vouchModal');

            // Add to history
            this.addToHistory(this.currentEnrollment, statement);

            // Refresh the list
            await this.loadPendingEnrollments();

        } catch (error) {
            console.error('Error submitting vouch:', error);
            this.showToast(error.message, 'error');
        } finally {
            document.getElementById('confirmVouch').disabled = false;
            document.getElementById('confirmVouch').textContent = 'Submit Vouch';
        }
    }

    async rejectEnrollment() {
        if (!this.currentEnrollment) return;

        const reason = prompt('Please provide a reason for rejection:');
        if (!reason) return;

        // TODO: Implement rejection API
        this.showToast('Rejection functionality coming soon', 'error');
    }

    addToHistory(enrollment, statement) {
        const historyItem = {
            enrollment_id: enrollment.enrollment_id,
            identity_name: enrollment.identity_name,
            coop_id: enrollment.coop_id,
            statement: statement,
            timestamp: new Date().toISOString()
        };

        // Store in localStorage for now
        const history = JSON.parse(localStorage.getItem('vouchHistory') || '[]');
        history.unshift(historyItem);
        localStorage.setItem('vouchHistory', JSON.stringify(history.slice(0, 100))); // Keep last 100

        this.vouchHistory = history;
        this.updateStats();
    }

    loadVouchHistory() {
        this.vouchHistory = JSON.parse(localStorage.getItem('vouchHistory') || '[]');

        const listEl = document.getElementById('historyList');

        if (this.vouchHistory.length === 0) {
            listEl.innerHTML = `
                <div class="empty-state">
                    <p>No vouches recorded yet.</p>
                </div>
            `;
            return;
        }

        listEl.innerHTML = this.vouchHistory.map(item => `
            <div class="history-item">
                <div class="enrollment-info">
                    <div class="enrollment-name">${this.escapeHtml(item.identity_name)}</div>
                    <div class="enrollment-meta">
                        <span>Coop: ${this.escapeHtml(item.coop_id)}</span>
                        <span>Date: ${this.formatDate(item.timestamp)}</span>
                    </div>
                </div>
                <div class="level-badge level-2">Vouched</div>
            </div>
        `).join('');

        this.updateStats();
    }

    updateStats() {
        const history = this.vouchHistory;

        document.getElementById('totalVouches').textContent = history.length;

        // Calculate monthly vouches
        const now = new Date();
        const monthStart = new Date(now.getFullYear(), now.getMonth(), 1);
        const monthlyCount = history.filter(h => new Date(h.timestamp) >= monthStart).length;
        document.getElementById('monthlyVouches').textContent = monthlyCount;

        // Reputation is always 100% for now
        document.getElementById('reputationScore').textContent = '100%';

        // Average response time
        if (history.length > 0) {
            document.getElementById('avgResponseTime').textContent = '< 1 day';
        }
    }

    openModal(modalId) {
        document.getElementById(modalId).classList.add('active');
    }

    closeModal(modalId) {
        document.getElementById(modalId).classList.remove('active');
    }

    showToast(message, type = 'success') {
        const container = document.getElementById('toastContainer');
        const toast = document.createElement('div');
        toast.className = `toast ${type}`;
        toast.textContent = message;
        container.appendChild(toast);

        setTimeout(() => {
            toast.remove();
        }, 5000);
    }

    formatDate(dateString) {
        const date = new Date(dateString);
        return date.toLocaleString();
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
}

// Initialize dashboard
const dashboard = new StewardDashboard();
