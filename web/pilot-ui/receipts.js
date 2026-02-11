/**
 * ICN Pilot UI - Receipt Chain Viewer
 * 
 * Displays the economic provenance chain:
 * DecisionReceipt → AllocationReceipt → SettlementIntent → LedgerEntry
 */

(function() {
    'use strict';

    // DOM elements
    const elements = {
        decisionHashInput: null,
        queryChainBtn: null,
        copyChainBtn: null,
        chainResults: null,
        chainStatus: null,
        allocationsList: null,
        intentsList: null,
        ledgerList: null,
        errorCard: null,
        errorMessage: null,
    };

    // Last query results for copy functionality
    let lastChainData = null;
    let lastLedgerData = null;
    let lastDecisionHash = null;

    // Initialize when DOM is ready
    function init() {
        elements.decisionHashInput = document.getElementById('decision-hash-input');
        elements.queryChainBtn = document.getElementById('query-chain-btn');
        elements.copyChainBtn = document.getElementById('copy-chain-btn');
        elements.chainResults = document.getElementById('receipt-chain-results');
        elements.chainStatus = document.getElementById('chain-status');
        elements.allocationsList = document.getElementById('allocations-list');
        elements.intentsList = document.getElementById('intents-list');
        elements.ledgerList = document.getElementById('ledger-list');
        elements.errorCard = document.getElementById('receipt-error');
        elements.errorMessage = document.getElementById('receipt-error-message');

        if (elements.queryChainBtn) {
            elements.queryChainBtn.addEventListener('click', handleQueryChain);
        }

        if (elements.copyChainBtn) {
            elements.copyChainBtn.addEventListener('click', handleCopyChain);
        }

        if (elements.decisionHashInput) {
            elements.decisionHashInput.addEventListener('keypress', (e) => {
                if (e.key === 'Enter') {
                    handleQueryChain();
                }
            });
        }

        console.log('[Receipts] Module initialized');
    }

    // Get gateway URL and coop ID from main app state
    function getGatewayUrl() {
        return window.state?.gatewayUrl || localStorage.getItem('icn_gateway_url') || 'http://localhost:8080';
    }

    function getCoopId() {
        return window.state?.coopId || localStorage.getItem('icn_coop_id') || 'test-coop';
    }

    function getAuthToken() {
        return window.state?.token || localStorage.getItem('icn_token') || '';
    }

    // Validate hex hash
    function isValidHexHash(hash) {
        return /^[0-9a-fA-F]{64}$/.test(hash);
    }

    // Handle query button click
    async function handleQueryChain() {
        const decisionHash = elements.decisionHashInput.value.trim().toLowerCase();

        // Validate input
        if (!decisionHash) {
            showError('Please enter a decision hash');
            return;
        }

        if (!isValidHexHash(decisionHash)) {
            showError('Invalid hash format. Expected 64 hex characters.');
            return;
        }

        hideError();
        showLoading();

        try {
            // Query both endpoints in parallel
            const [chainData, ledgerData] = await Promise.all([
                queryReceiptChain(decisionHash),
                queryLedgerEntries(decisionHash),
            ]);

            renderResults(chainData, ledgerData, decisionHash);
        } catch (error) {
            console.error('[Receipts] Query failed:', error);
            showError(error.message || 'Failed to query receipt chain');
        }
    }

    // Query /v1/receipts/chain endpoint
    async function queryReceiptChain(decisionHash) {
        const url = `${getGatewayUrl()}/v1/receipts/chain?decision_hash=${decisionHash}`;
        const token = getAuthToken();

        const headers = { 'Accept': 'application/json' };
        if (token) {
            headers['Authorization'] = `Bearer ${token}`;
        }

        const response = await fetch(url, { headers });

        if (!response.ok) {
            if (response.status === 404) {
                return { allocations: [], intents: [] };
            }
            throw new Error(`Receipt chain query failed: ${response.status}`);
        }

        return response.json();
    }

    // Query /v1/ledger/{coop}/entries/by-decision endpoint
    async function queryLedgerEntries(decisionHash) {
        const coopId = getCoopId();
        const url = `${getGatewayUrl()}/v1/ledger/${coopId}/entries/by-decision?decision_hash=${decisionHash}`;
        const token = getAuthToken();

        const headers = { 'Accept': 'application/json' };
        if (token) {
            headers['Authorization'] = `Bearer ${token}`;
        }

        const response = await fetch(url, { headers });

        if (!response.ok) {
            if (response.status === 404) {
                return { entries: [] };
            }
            // Ledger query may fail if not authenticated - graceful degradation
            console.warn('[Receipts] Ledger query failed:', response.status);
            return { entries: [] };
        }

        return response.json();
    }

    // Render results
    function renderResults(chainData, ledgerData, decisionHash) {
        // Store for copy functionality
        lastChainData = chainData;
        lastLedgerData = ledgerData;
        lastDecisionHash = decisionHash;

        elements.chainResults.classList.remove('hidden');

        const allocations = chainData.allocations || [];
        const intents = chainData.intents || [];
        const entries = ledgerData.entries || [];

        // Update status badge
        const totalItems = allocations.length + intents.length + entries.length;
        if (totalItems > 0) {
            elements.chainStatus.textContent = `${totalItems} items found`;
            elements.chainStatus.className = 'status-badge success';
        } else {
            elements.chainStatus.textContent = 'No data found';
            elements.chainStatus.className = 'status-badge warning';
        }

        // Render allocations
        renderAllocations(allocations);

        // Render intents
        renderIntents(intents);

        // Render ledger entries
        renderLedgerEntries(entries);
    }

    // Check if array is sorted
    function isSorted(arr) {
        if (!arr || arr.length <= 1) return true;
        for (let i = 0; i < arr.length - 1; i++) {
            if (arr[i] > arr[i + 1]) return false;
        }
        return true;
    }

    // Render allocation receipts
    function renderAllocations(allocations) {
        if (!allocations.length) {
            elements.allocationsList.innerHTML = '<p class="empty-state">No allocation receipts found</p>';
            return;
        }

        const html = allocations.map(alloc => {
            const hashes = alloc.intentHashes || [];
            const sorted = isSorted(hashes);
            const sortedBadge = sorted 
                ? '<span class="sorted-badge valid">✓ canonicalized</span>'
                : '<span class="sorted-badge invalid">⚠ not sorted</span>';
            
            return `
            <div class="receipt-item allocation">
                <div class="receipt-header">
                    <span class="receipt-type">Allocation</span>
                    <span class="receipt-scope">${escapeHtml(alloc.scope || 'Unknown')}</span>
                </div>
                <div class="receipt-field">
                    <label>Canonical Hash:</label>
                    <code class="hash">${truncateHash(alloc.canonicalHash)}</code>
                </div>
                <div class="receipt-field">
                    <label>Decision Hash:</label>
                    <code class="hash">${truncateHash(alloc.decisionHash)}</code>
                </div>
                <div class="receipt-field">
                    <label>Intent Count:</label>
                    <span>${alloc.intentCount || 0}</span>
                </div>
                ${hashes.length ? `
                <div class="receipt-field">
                    <label>Intent Hashes: ${sortedBadge}</label>
                    <ul class="hash-list">
                        ${hashes.map(h => `<li><code class="hash">${truncateHash(h)}</code></li>`).join('')}
                    </ul>
                </div>
                ` : ''}
                <div class="receipt-field">
                    <label>Created:</label>
                    <span>${formatTimestamp(alloc.createdAt)}</span>
                </div>
            </div>
        `}).join('');

        elements.allocationsList.innerHTML = html;
    }

    // Render settlement intents
    function renderIntents(intents) {
        if (!intents.length) {
            elements.intentsList.innerHTML = '<p class="empty-state">No settlement intents found</p>';
            return;
        }

        const html = intents.map(intent => `
            <div class="receipt-item intent">
                <div class="receipt-header">
                    <span class="receipt-type">Intent</span>
                    <span class="receipt-asset">${intent.assetType || 'Unknown'}</span>
                </div>
                <div class="receipt-field">
                    <label>Canonical Hash:</label>
                    <code class="hash">${truncateHash(intent.canonicalHash)}</code>
                </div>
                <div class="receipt-row">
                    <div class="receipt-field">
                        <label>From:</label>
                        <span>${intent.from || '—'}</span>
                    </div>
                    <div class="receipt-field">
                        <label>To:</label>
                        <span>${intent.to || '—'}</span>
                    </div>
                </div>
                <div class="receipt-row">
                    <div class="receipt-field">
                        <label>Amount:</label>
                        <span class="amount">${intent.amount || 0} ${intent.unit || ''}</span>
                    </div>
                    <div class="receipt-field">
                        <label>Scope:</label>
                        <span>${intent.scope || 'Unknown'}</span>
                    </div>
                </div>
                ${intent.memo ? `
                <div class="receipt-field">
                    <label>Memo:</label>
                    <span class="memo">${escapeHtml(intent.memo)}</span>
                </div>
                ` : ''}
            </div>
        `).join('');

        elements.intentsList.innerHTML = html;
    }

    // Render ledger entries
    function renderLedgerEntries(entries) {
        if (!entries.length) {
            elements.ledgerList.innerHTML = '<p class="empty-state">No ledger entries found</p>';
            return;
        }

        const html = entries.map(entry => `
            <div class="receipt-item ledger">
                <div class="receipt-header">
                    <span class="receipt-type">Ledger Entry</span>
                    <span class="receipt-status">${entry.status || 'Unknown'}</span>
                </div>
                <div class="receipt-row">
                    <div class="receipt-field">
                        <label>From:</label>
                        <span>${entry.from || '—'}</span>
                    </div>
                    <div class="receipt-field">
                        <label>To:</label>
                        <span>${entry.to || '—'}</span>
                    </div>
                </div>
                <div class="receipt-row">
                    <div class="receipt-field">
                        <label>Amount:</label>
                        <span class="amount">${entry.amount || 0} ${entry.currency || ''}</span>
                    </div>
                    <div class="receipt-field">
                        <label>Type:</label>
                        <span>${entry.entryType || 'Transfer'}</span>
                    </div>
                </div>
                ${entry.decisionHash ? `
                <div class="receipt-field">
                    <label>Decision Hash:</label>
                    <code class="hash">${truncateHash(entry.decisionHash)}</code>
                </div>
                ` : ''}
                ${entry.decisionReceiptId ? `
                <div class="receipt-field">
                    <label>Decision Receipt ID:</label>
                    <span>${entry.decisionReceiptId}</span>
                </div>
                ` : ''}
                <div class="receipt-field">
                    <label>Timestamp:</label>
                    <span>${formatTimestamp(entry.timestamp)}</span>
                </div>
            </div>
        `).join('');

        elements.ledgerList.innerHTML = html;
    }

    // Handle copy chain button
    async function handleCopyChain() {
        if (!lastChainData || !lastDecisionHash) {
            showError('No chain data to copy. Query a decision hash first.');
            return;
        }

        const allocations = lastChainData.allocations || [];
        const intents = lastChainData.intents || [];
        const entries = lastLedgerData?.entries || [];

        // Build summary text
        let text = `ICN Receipt Chain Summary\n`;
        text += `========================\n\n`;
        text += `Decision Hash: ${lastDecisionHash}\n\n`;

        if (allocations.length) {
            text += `Allocation Receipts (${allocations.length}):\n`;
            for (const alloc of allocations) {
                text += `  - Canonical Hash: ${alloc.canonicalHash || '—'}\n`;
                text += `    Scope: ${alloc.scope || 'Unknown'}\n`;
                if (alloc.intentHashes?.length) {
                    text += `    Intent Hashes (${alloc.intentHashes.length}):\n`;
                    for (const h of alloc.intentHashes) {
                        text += `      • ${h}\n`;
                    }
                }
            }
            text += '\n';
        }

        if (intents.length) {
            text += `Settlement Intents (${intents.length}):\n`;
            for (const intent of intents) {
                text += `  - Canonical Hash: ${intent.canonicalHash || '—'}\n`;
                text += `    ${intent.from || '—'} → ${intent.to || '—'}: ${intent.amount || 0} ${intent.unit || ''}\n`;
            }
            text += '\n';
        }

        if (entries.length) {
            text += `Ledger Entries (${entries.length}):\n`;
            for (const entry of entries) {
                text += `  - ${entry.from || '—'} → ${entry.to || '—'}: ${entry.amount || 0} ${entry.currency || ''}\n`;
                if (entry.decisionHash) {
                    text += `    Decision Hash: ${entry.decisionHash}\n`;
                }
            }
        }

        text += `\n--- Generated ${new Date().toISOString()} ---\n`;

        try {
            await navigator.clipboard.writeText(text);
            // Visual feedback
            const btn = elements.copyChainBtn;
            const originalText = btn.textContent;
            btn.textContent = '✓ Copied';
            btn.classList.add('copied');
            setTimeout(() => {
                btn.textContent = originalText;
                btn.classList.remove('copied');
            }, 2000);
        } catch (err) {
            console.error('[Receipts] Copy failed:', err);
            showError('Failed to copy to clipboard');
        }
    }

    // Helper functions
    function truncateHash(hash) {
        if (!hash) return '—';
        if (hash.length <= 20) return hash;
        return `${hash.substring(0, 12)}...${hash.substring(hash.length - 8)}`;
    }

    function formatTimestamp(ts) {
        if (!ts) return '—';
        // Handle both Unix seconds and milliseconds
        const ms = ts > 1e12 ? ts : ts * 1000;
        return new Date(ms).toLocaleString();
    }

    function escapeHtml(str) {
        const div = document.createElement('div');
        div.textContent = str;
        return div.innerHTML;
    }

    function showLoading() {
        elements.chainResults.classList.remove('hidden');
        elements.chainStatus.textContent = 'Loading...';
        elements.chainStatus.className = 'status-badge loading';
        elements.allocationsList.innerHTML = '<p class="loading">Querying...</p>';
        elements.intentsList.innerHTML = '<p class="loading">Querying...</p>';
        elements.ledgerList.innerHTML = '<p class="loading">Querying...</p>';
    }

    function showError(message) {
        elements.errorCard.classList.remove('hidden');
        elements.errorMessage.textContent = message;
        elements.chainResults.classList.add('hidden');
    }

    function hideError() {
        elements.errorCard.classList.add('hidden');
    }

    // Initialize on DOMContentLoaded
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
