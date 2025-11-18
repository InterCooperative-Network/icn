// ICN Gateway API Client

class ICNClient {
    constructor() {
        this.baseURL = window.location.origin;
        this.token = localStorage.getItem('icn_token');
        this.did = localStorage.getItem('icn_did');
        this.coopId = localStorage.getItem('icn_coop_id');
        this.ws = null;
        this.wsReconnectAttempts = 0;
        this.wsMaxReconnectAttempts = 5;
        this.eventHandlers = {};
    }

    // ===== Authentication =====

    async login(did, coopId) {
        try {
            // Step 1: Get challenge
            const challengeRes = await this.request('/v1/auth/challenge', {
                method: 'POST',
                body: JSON.stringify({ did })
            });

            // In a real implementation, you would sign the challenge with your private key
            // For demo purposes, we'll simulate a signature
            const signature = this.simulateSignature(challengeRes.challenge);

            // Step 2: Verify and get token
            const verifyRes = await this.request('/v1/auth/verify', {
                method: 'POST',
                body: JSON.stringify({
                    did,
                    signature,
                    coop_id: coopId,
                    scopes: ['ledger:read', 'ledger:write', 'coop:read', 'coop:write', 'gov:read', 'gov:write']
                })
            });

            this.token = verifyRes.token;
            this.did = did;
            this.coopId = coopId;

            // Save to localStorage
            localStorage.setItem('icn_token', this.token);
            localStorage.setItem('icn_did', this.did);
            localStorage.setItem('icn_coop_id', this.coopId);

            // Connect WebSocket
            this.connectWebSocket();

            return true;
        } catch (error) {
            console.error('Login failed:', error);
            throw error;
        }
    }

    logout() {
        this.token = null;
        this.did = null;
        this.coopId = null;
        localStorage.removeItem('icn_token');
        localStorage.removeItem('icn_did');
        localStorage.removeItem('icn_coop_id');

        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
    }

    isAuthenticated() {
        return !!this.token;
    }

    // Simulate signature for demo (in production, use actual ed25519 signing)
    simulateSignature(challenge) {
        return btoa(challenge + '-simulated-signature');
    }

    // ===== WebSocket Connection =====

    connectWebSocket() {
        if (!this.token || !this.coopId) return;

        const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsURL = `${wsProtocol}//${window.location.host}/v1/ws/${this.coopId}`;

        this.ws = new WebSocket(wsURL);

        this.ws.onopen = () => {
            console.log('WebSocket connected');
            this.wsReconnectAttempts = 0;

            // Authenticate
            this.ws.send(JSON.stringify({
                type: 'Auth',
                token: this.token
            }));

            this.emit('ws:connected');
        };

        this.ws.onmessage = (event) => {
            try {
                const message = JSON.parse(event.data);
                this.handleWebSocketMessage(message);
            } catch (error) {
                console.error('Failed to parse WebSocket message:', error);
            }
        };

        this.ws.onerror = (error) => {
            console.error('WebSocket error:', error);
            this.emit('ws:error', error);
        };

        this.ws.onclose = () => {
            console.log('WebSocket disconnected');
            this.emit('ws:disconnected');
            this.attemptReconnect();
        };
    }

    attemptReconnect() {
        if (this.wsReconnectAttempts < this.wsMaxReconnectAttempts) {
            this.wsReconnectAttempts++;
            const delay = Math.min(1000 * Math.pow(2, this.wsReconnectAttempts), 30000);
            console.log(`Reconnecting WebSocket in ${delay}ms (attempt ${this.wsReconnectAttempts})`);
            setTimeout(() => this.connectWebSocket(), delay);
        }
    }

    handleWebSocketMessage(message) {
        switch (message.type) {
            case 'AuthOk':
                console.log('WebSocket authenticated');
                this.emit('ws:authenticated', message);
                break;
            case 'Event':
                // Extract event type and data
                const eventType = Object.keys(message).find(k => k !== 'type');
                if (eventType) {
                    this.emit(`event:${eventType}`, message[eventType]);
                }
                break;
            case 'Error':
                console.error('WebSocket error:', message.message);
                this.emit('ws:message-error', message.message);
                break;
            default:
                console.log('Unknown WebSocket message:', message);
        }
    }

    // ===== Event System =====

    on(event, handler) {
        if (!this.eventHandlers[event]) {
            this.eventHandlers[event] = [];
        }
        this.eventHandlers[event].push(handler);
    }

    off(event, handler) {
        if (!this.eventHandlers[event]) return;
        this.eventHandlers[event] = this.eventHandlers[event].filter(h => h !== handler);
    }

    emit(event, data) {
        if (!this.eventHandlers[event]) return;
        this.eventHandlers[event].forEach(handler => {
            try {
                handler(data);
            } catch (error) {
                console.error(`Error in event handler for ${event}:`, error);
            }
        });
    }

    // ===== HTTP Requests =====

    async request(path, options = {}) {
        const headers = {
            'Content-Type': 'application/json',
            ...options.headers
        };

        if (this.token && options.auth !== false) {
            headers['Authorization'] = `Bearer ${this.token}`;
        }

        const response = await fetch(`${this.baseURL}${path}`, {
            ...options,
            headers
        });

        if (!response.ok) {
            const error = await response.json().catch(() => ({ error: response.statusText }));
            throw new Error(error.error || error.message || 'Request failed');
        }

        return response.json();
    }

    // ===== Cooperatives API =====

    async listCooperatives() {
        return this.request('/v1/coops');
    }

    async getCooperative(coopId) {
        return this.request(`/v1/coops/${coopId}`);
    }

    async createCooperative(coopId, name) {
        return this.request('/v1/coops', {
            method: 'POST',
            body: JSON.stringify({ coop_id: coopId, name })
        });
    }

    async updateCooperative(coopId, data) {
        return this.request(`/v1/coops/${coopId}`, {
            method: 'PUT',
            body: JSON.stringify(data)
        });
    }

    async deleteCooperative(coopId) {
        return this.request(`/v1/coops/${coopId}`, {
            method: 'DELETE'
        });
    }

    async addMember(coopId, did, role = 'member') {
        return this.request(`/v1/coops/${coopId}/members`, {
            method: 'POST',
            body: JSON.stringify({ did, role })
        });
    }

    async removeMember(coopId, did) {
        return this.request(`/v1/coops/${coopId}/members/${encodeURIComponent(did)}`, {
            method: 'DELETE'
        });
    }

    async updateMemberRole(coopId, did, role) {
        return this.request(`/v1/coops/${coopId}/members/${encodeURIComponent(did)}`, {
            method: 'PUT',
            body: JSON.stringify({ role })
        });
    }

    // ===== Governance API =====

    async listDomains(coopId) {
        return this.request(`/v1/gov/domains?coop_id=${encodeURIComponent(coopId)}`);
    }

    async getDomain(domainId) {
        return this.request(`/v1/gov/domains/${encodeURIComponent(domainId)}`);
    }

    async createDomain(domainId, name, coopId) {
        return this.request('/v1/gov/domains', {
            method: 'POST',
            body: JSON.stringify({ domain_id: domainId, name, coop_id: coopId })
        });
    }

    async listProposals(domainId) {
        return this.request(`/v1/gov/proposals?domain_id=${encodeURIComponent(domainId)}`);
    }

    async createProposal(domainId, title, description, kind = 'text') {
        return this.request('/v1/gov/proposals', {
            method: 'POST',
            body: JSON.stringify({ domain_id: domainId, title, description, kind })
        });
    }

    async openProposal(proposalId) {
        return this.request(`/v1/gov/proposals/${encodeURIComponent(proposalId)}/open`, {
            method: 'POST'
        });
    }

    async closeProposal(proposalId) {
        return this.request(`/v1/gov/proposals/${encodeURIComponent(proposalId)}/close`, {
            method: 'POST'
        });
    }

    async castVote(proposalId, choice, comment = null) {
        return this.request(`/v1/gov/proposals/${encodeURIComponent(proposalId)}/vote`, {
            method: 'POST',
            body: JSON.stringify({ choice, comment })
        });
    }

    // ===== Ledger API =====

    async getBalance(coopId, did) {
        return this.request(`/v1/ledger/${coopId}/balance/${encodeURIComponent(did)}`);
    }

    async createPayment(coopId, to, amount, currency, description) {
        return this.request(`/v1/ledger/${coopId}/payment`, {
            method: 'POST',
            body: JSON.stringify({
                from: this.did,
                to,
                amount,
                currency,
                description
            })
        });
    }

    async getHistory(coopId, limit = 50, offset = 0) {
        return this.request(`/v1/ledger/${coopId}/history?limit=${limit}&offset=${offset}`);
    }

    // ===== Health API =====

    async getHealth() {
        return this.request('/v1/health', { auth: false });
    }

    async getHealthDetailed() {
        return this.request('/v1/health/detailed');
    }
}

// Create global instance
window.icnClient = new ICNClient();
