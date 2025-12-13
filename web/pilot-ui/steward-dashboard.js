// Steward Dashboard JavaScript

class StewardDashboard {
    constructor() {
        this.apiBase = this.detectApiBase();
        this.currentEnrollment = null;
        this.enrollments = [];
        this.vouchHistory = [];
        this.stats = null;
        this.autoRefreshInterval = null;

        this.init();
    }

    detectApiBase() {
        // Use current origin or fallback to gateway
        if (window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1') {
            return 'http://10.8.10.40:30080';
        }
        return window.location.origin;
    }

    getAuthToken() {
        return localStorage.getItem('authToken');
    }

    getHeaders() {
        const headers = {
            'Content-Type': 'application/json'
        };
        const token = this.getAuthToken();
        if (token) {
            headers['Authorization'] = `Bearer ${token}`;
        }
        return headers;
    }

    async init() {
        this.setupEventListeners();
        this.loadStewardInfo();
        await this.loadPendingEnrollments();
        await this.loadStewardStats();
        await this.loadVouchHistory();
        this.startAutoRefresh();
    }

    startAutoRefresh() {
        // Auto-refresh every 30 seconds
        this.autoRefreshInterval = setInterval(() => {
            this.loadPendingEnrollments();
            this.loadStewardStats();
        }, 30000);
    }

    stopAutoRefresh() {
        if (this.autoRefreshInterval) {
            clearInterval(this.autoRefreshInterval);
            this.autoRefreshInterval = null;
        }
    }

    setupEventListeners() {
        // Tab navigation
        document.querySelectorAll('.nav-btn').forEach(btn => {
            btn.addEventListener('click', () => this.switchTab(btn.dataset.tab));
        });

        // Refresh button
        document.getElementById('refreshPending').addEventListener('click', () => {
            this.loadPendingEnrollments();
            this.loadStewardStats();
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

        // Handle page visibility change (pause auto-refresh when hidden)
        document.addEventListener('visibilitychange', () => {
            if (document.hidden) {
                this.stopAutoRefresh();
            } else {
                this.startAutoRefresh();
                this.loadPendingEnrollments();
            }
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
        // Load from auth token if available
        const token = this.getAuthToken();
        let stewardId = 'Not authenticated';

        if (token) {
            try {
                // Decode JWT payload (base64)
                const payload = JSON.parse(atob(token.split('.')[1]));
                stewardId = payload.sub || 'Unknown';
            } catch (e) {
                console.warn('Could not decode auth token:', e);
                stewardId = localStorage.getItem('stewardDid') || 'did:icn:steward-' + Math.random().toString(36).substr(2, 8);
            }
        } else {
            stewardId = localStorage.getItem('stewardDid') || 'did:icn:steward-' + Math.random().toString(36).substr(2, 8);
        }

        document.getElementById('stewardId').textContent = stewardId.length > 20
            ? stewardId.substring(0, 20) + '...'
            : stewardId;
    }

    async loadStewardStats() {
        try {
            const response = await fetch(`${this.apiBase}/v1/sdis/steward/stats`, {
                headers: this.getHeaders()
            });

            if (!response.ok) {
                // Fall back to localStorage stats if API fails
                this.updateStatsFromLocal();
                return;
            }

            this.stats = await response.json();
            this.updateStatsDisplay();
        } catch (error) {
            console.error('Error loading steward stats:', error);
            this.updateStatsFromLocal();
        }
    }

    updateStatsDisplay() {
        if (!this.stats) return;

        document.getElementById('totalVouches').textContent = this.stats.total_vouches || 0;
        document.getElementById('monthlyVouches').textContent = this.stats.monthly_vouches || 0;
        document.getElementById('reputationScore').textContent =
            Math.round((this.stats.reputation_score || 1) * 100) + '%';

        const avgHours = this.stats.avg_response_hours || 0;
        if (avgHours < 1) {
            document.getElementById('avgResponseTime').textContent = '< 1 hour';
        } else if (avgHours < 24) {
            document.getElementById('avgResponseTime').textContent = `${Math.round(avgHours)} hours`;
        } else {
            document.getElementById('avgResponseTime').textContent = `${Math.round(avgHours / 24)} days`;
        }
    }

    updateStatsFromLocal() {
        // Fallback to localStorage-based stats
        const history = JSON.parse(localStorage.getItem('vouchHistory') || '[]');

        document.getElementById('totalVouches').textContent = history.length;

        const now = new Date();
        const monthStart = new Date(now.getFullYear(), now.getMonth(), 1);
        const monthlyCount = history.filter(h => new Date(h.timestamp || h.vouched_at) >= monthStart).length;
        document.getElementById('monthlyVouches').textContent = monthlyCount;

        document.getElementById('reputationScore').textContent = '100%';
        document.getElementById('avgResponseTime').textContent = history.length > 0 ? '< 1 day' : '-';
    }

    async loadPendingEnrollments() {
        const listEl = document.getElementById('enrollmentList');
        listEl.innerHTML = '<div class="loading">Loading pending enrollments...</div>';

        try {
            const response = await fetch(`${this.apiBase}/v1/sdis/pending`, {
                headers: this.getHeaders()
            });
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
            const response = await fetch(`${this.apiBase}/v1/sdis/status/${enrollmentId}`, {
                headers: this.getHeaders()
            });
            if (!response.ok) throw new Error('Failed to load enrollment details');

            this.currentEnrollment = await response.json();

            const detailsEl = document.getElementById('enrollmentDetails');
            let rejectionHtml = '';
            if (this.currentEnrollment.rejected) {
                rejectionHtml = `
                    <div class="detail-item rejection-info">
                        <label>Rejection Status</label>
                        <div class="value rejection-badge">REJECTED</div>
                    </div>
                    ${this.currentEnrollment.rejection_reason ? `
                    <div class="detail-item">
                        <label>Rejection Reason</label>
                        <div class="value">${this.escapeHtml(this.currentEnrollment.rejection_reason)}</div>
                    </div>` : ''}
                    ${this.currentEnrollment.rejected_at ? `
                    <div class="detail-item">
                        <label>Rejected At</label>
                        <div class="value">${this.formatDate(this.currentEnrollment.rejected_at)}</div>
                    </div>` : ''}
                `;
            }

            detailsEl.innerHTML = `
                ${rejectionHtml}
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
            const rejectBtn = document.getElementById('rejectBtn');

            if (this.currentEnrollment.rejected) {
                vouchBtn.disabled = true;
                vouchBtn.textContent = 'Enrollment Rejected';
                rejectBtn.disabled = true;
                rejectBtn.style.display = 'none';
            } else if (this.currentEnrollment.level < 1) {
                vouchBtn.disabled = true;
                vouchBtn.textContent = 'Awaiting Device Verification';
                rejectBtn.disabled = false;
                rejectBtn.style.display = '';
            } else if (this.currentEnrollment.has_steward_vouch) {
                vouchBtn.disabled = true;
                vouchBtn.textContent = 'Already Vouched';
                rejectBtn.disabled = true;
                rejectBtn.style.display = 'none';
            } else {
                vouchBtn.disabled = false;
                vouchBtn.textContent = 'Vouch for Identity';
                rejectBtn.disabled = false;
                rejectBtn.style.display = '';
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
            'ready_for_completion': 'Ready for Completion',
            'rejected': 'Rejected'
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
                headers: this.getHeaders(),
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

            // Refresh data
            await this.loadPendingEnrollments();
            await this.loadStewardStats();
            await this.loadVouchHistory();

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
        if (!reason || reason.trim().length === 0) return;

        const enrollmentId = this.currentEnrollment.enrollment_id;

        try {
            document.getElementById('rejectBtn').disabled = true;
            document.getElementById('rejectBtn').textContent = 'Rejecting...';

            const response = await fetch(`${this.apiBase}/v1/sdis/reject/${enrollmentId}`, {
                method: 'POST',
                headers: this.getHeaders(),
                body: JSON.stringify({
                    reason: reason.trim()
                })
            });

            if (!response.ok) {
                const error = await response.json();
                throw new Error(error.error || 'Failed to reject enrollment');
            }

            this.showToast(`Enrollment for ${this.currentEnrollment.identity_name} has been rejected`, 'success');
            this.closeModal('enrollmentModal');

            // Refresh data
            await this.loadPendingEnrollments();
            await this.loadStewardStats();

        } catch (error) {
            console.error('Error rejecting enrollment:', error);
            this.showToast(error.message, 'error');
        } finally {
            document.getElementById('rejectBtn').disabled = false;
            document.getElementById('rejectBtn').textContent = 'Reject';
        }
    }

    async loadVouchHistory() {
        const listEl = document.getElementById('historyList');

        try {
            const response = await fetch(`${this.apiBase}/v1/sdis/steward/history?limit=50&offset=0`, {
                headers: this.getHeaders()
            });

            if (!response.ok) {
                // Fall back to localStorage history if API fails
                this.loadVouchHistoryFromLocal();
                return;
            }

            const data = await response.json();
            this.vouchHistory = data.vouches || [];

            this.renderVouchHistory();
        } catch (error) {
            console.error('Error loading vouch history:', error);
            this.loadVouchHistoryFromLocal();
        }
    }

    loadVouchHistoryFromLocal() {
        // Fallback to localStorage-based history
        this.vouchHistory = JSON.parse(localStorage.getItem('vouchHistory') || '[]');
        this.renderVouchHistory();
    }

    renderVouchHistory() {
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
                        <span>Date: ${this.formatDate(item.vouched_at || item.timestamp)}</span>
                    </div>
                    ${item.vouch_statement ? `
                    <div class="vouch-statement">
                        "${this.escapeHtml(item.vouch_statement)}"
                    </div>` : ''}
                </div>
                <div class="level-badge level-2">Vouched</div>
            </div>
        `).join('');
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
        if (!dateString) return '-';
        const date = new Date(dateString);
        return date.toLocaleString();
    }

    escapeHtml(text) {
        if (!text) return '';
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
}

// Initialize dashboard
const dashboard = new StewardDashboard();
