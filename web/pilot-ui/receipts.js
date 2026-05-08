/**
 * ICN Pilot UI - Receipt Chain Viewer
 * 
 * Displays the economic provenance chain:
 * DecisionReceipt → AllocationReceipt → SettlementIntent → LedgerEntry
 */

(function() {
    'use strict';

    // HTML escape function to prevent XSS
    function escapeHtml(text) {
        if (text === null || text === undefined) return '';
        const div = document.createElement('div');
        div.textContent = String(text);
        return div.innerHTML;
    }

    // DOM elements
    const elements = {
        decisionHashInput: null,
        queryChainBtn: null,
        copyChainBtn: null,
        chainResults: null,
        chainStatus: null,
        plainLanguageSummary: null, // PR 4: plain-language framing above the chain
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
        elements.plainLanguageSummary = document.getElementById('receipt-plain-language-summary');
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
        // Ledger API returns transactions, not entries
        const transactions = ledgerData.transactions || [];

        // Update status badge
        const totalItems = allocations.length + intents.length + transactions.length;
        if (totalItems > 0) {
            elements.chainStatus.textContent = `${totalItems} items found`;
            elements.chainStatus.className = 'status-badge success';
        } else {
            elements.chainStatus.textContent = 'No data found';
            elements.chainStatus.className = 'status-badge warning';
        }

        // PR 4: render plain-language summary BEFORE the structured detail.
        // The structured chain (allocations + intents + ledger) renders below
        // unchanged inside the existing <details> wrapper; the summary leads
        // with outcome / authority / hash / timestamp in plain language so a
        // non-technical viewer can read the chain without parsing fields.
        renderPlainLanguageSummary(allocations, intents, transactions, decisionHash);

        // Render allocations
        renderAllocations(allocations);

        // Render intents
        renderIntents(intents);

        // Render ledger entries (from transactions)
        renderLedgerEntries(transactions);
    }

    // PR 4: Plain-language summary above the existing chain detail.
    // Reads only what the gateway already returned at /v1/receipts/chain
    // and /v1/ledger/{coop}/entries/by-decision; no new endpoint, no new
    // fetch. Built via DOM API (createElement / textContent) so user-
    // controlled values cannot be injected as HTML.
    function renderPlainLanguageSummary(allocations, intents, transactions, decisionHash) {
        const container = elements.plainLanguageSummary;
        if (!container) return;
        // Replace any prior contents.
        while (container.firstChild) container.removeChild(container.firstChild);

        const total = allocations.length + intents.length + transactions.length;
        if (total === 0) {
            const empty = document.createElement('p');
            empty.className = 'empty-state';
            empty.appendChild(document.createTextNode('No receipt-chain data found for decision '));
            const code = document.createElement('code');
            code.className = 'hash';
            code.textContent = truncateHash(decisionHash);
            empty.appendChild(code);
            empty.appendChild(document.createTextNode('. Either the decision hash has not produced a chain yet, or the caller does not have access.'));
            container.appendChild(empty);
            return;
        }

        // Earliest timestamp across the data we have. Allocations carry
        // createdAt; ledger transactions carry timestamp. Intents currently
        // do not surface a top-level timestamp, so we omit them from the
        // earliest-timestamp pick rather than make one up.
        const candidates = [];
        for (const a of allocations) {
            if (a.createdAt) candidates.push(a.createdAt);
        }
        for (const t of transactions) {
            if (t.timestamp) candidates.push(t.timestamp);
        }
        const earliest = candidates.length ? candidates.slice().sort()[0] : null;

        // Authority basis: the allocation scope is the closest thing the
        // chain currently surfaces to "under what governance authority did
        // this happen?". If we have allocations, use the first allocation's
        // scope; otherwise we say authority is unstated in the chain shape.
        const scope = allocations.length && allocations[0].scope
            ? allocations[0].scope
            : null;

        // Heading
        const heading = document.createElement('h4');
        heading.className = 'receipt-summary-heading';
        heading.textContent = 'Receipt chain — plain-language summary';
        container.appendChild(heading);

        // Decision hash line
        const decisionPara = document.createElement('p');
        decisionPara.className = 'receipt-summary-decision';
        decisionPara.appendChild(document.createTextNode('Decision hash: '));
        const decisionCode = document.createElement('code');
        decisionCode.className = 'hash';
        decisionCode.title = decisionHash;
        decisionCode.textContent = truncateHash(decisionHash);
        decisionPara.appendChild(decisionCode);
        container.appendChild(decisionPara);

        // Outcome intro line
        const outcomePara = document.createElement('p');
        outcomePara.className = 'receipt-summary-outcome';
        outcomePara.textContent = 'This decision produced:';
        container.appendChild(outcomePara);

        // Counts list
        const list = document.createElement('ul');
        list.className = 'receipt-summary-list';
        const parts = [];
        if (allocations.length) parts.push(`${allocations.length} allocation receipt${allocations.length === 1 ? '' : 's'}`);
        if (intents.length) parts.push(`${intents.length} settlement intent${intents.length === 1 ? '' : 's'}`);
        if (transactions.length) parts.push(`${transactions.length} ledger entr${transactions.length === 1 ? 'y' : 'ies'}`);
        if (parts.length === 0) parts.push('nothing recorded under this decision yet');
        for (const p of parts) {
            const li = document.createElement('li');
            li.textContent = p;
            list.appendChild(li);
        }
        container.appendChild(list);

        // Authority line
        const authorityPara = document.createElement('p');
        authorityPara.className = 'receipt-summary-authority';
        if (scope) {
            authorityPara.appendChild(document.createTextNode('Under governance scope '));
            const scopeCode = document.createElement('code');
            scopeCode.textContent = scope;
            authorityPara.appendChild(scopeCode);
            authorityPara.appendChild(document.createTextNode('.'));
        } else {
            authorityPara.textContent = 'Authority basis is not surfaced by the chain shape; consult the structured detail below.';
        }
        container.appendChild(authorityPara);

        // Earliest timestamp line
        const timePara = document.createElement('p');
        timePara.className = 'receipt-summary-time';
        if (earliest) {
            timePara.appendChild(document.createTextNode('Earliest timestamp in this chain: '));
            const strong = document.createElement('strong');
            strong.textContent = formatTimestamp(earliest);
            timePara.appendChild(strong);
            timePara.appendChild(document.createTextNode('.'));
        } else {
            timePara.textContent = 'No timestamps surfaced.';
        }
        container.appendChild(timePara);

        // Opaque-storage pinning explainer
        const pinningPara = document.createElement('p');
        pinningPara.className = 'receipt-summary-pinning';
        pinningPara.appendChild(document.createTextNode('Receipt bytes are stored at the kernel as '));
        const opaqueStrong = document.createElement('strong');
        opaqueStrong.textContent = 'opaque records';
        pinningPara.appendChild(opaqueStrong);
        pinningPara.appendChild(document.createTextNode(' (per '));
        const link = document.createElement('a');
        link.href = 'https://github.com/InterCooperative-Network/icn/blob/main/docs/architecture/KERNEL_APP_SEPARATION.md';
        link.target = '_blank';
        link.rel = 'noopener';
        link.textContent = 'Invariant 6';
        pinningPara.appendChild(link);
        pinningPara.appendChild(document.createTextNode('): the gateway pins each '));
        const tupleCode = document.createElement('code');
        tupleCode.textContent = '(class, record_hash)';
        pinningPara.appendChild(tupleCode);
        pinningPara.appendChild(document.createTextNode(' tuple atomically and refuses to remap a hash to a different secondary-index location, so the audit chain rendered below cannot fan out across timestamps after the fact.'));
        container.appendChild(pinningPara);

        // Steward note
        const notePara = document.createElement('p');
        notePara.className = 'receipt-summary-note';
        notePara.textContent = 'The structured detail below shows the same data field-by-field for stewards and integrators.';
        container.appendChild(notePara);
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
                    <code class="hash">${escapeHtml(truncateHash(alloc.canonicalHash))}</code>
                </div>
                <div class="receipt-field">
                    <label>Decision Hash:</label>
                    <code class="hash">${escapeHtml(truncateHash(alloc.decisionHash))}</code>
                </div>
                <div class="receipt-field">
                    <label>Intent Count:</label>
                    <span>${escapeHtml(String(alloc.intentCount || 0))}</span>
                </div>
                ${hashes.length ? `
                <div class="receipt-field">
                    <label>Intent Hashes: ${sortedBadge}</label>
                    <ul class="hash-list">
                        ${hashes.map(h => `<li><code class="hash">${escapeHtml(truncateHash(h))}</code></li>`).join('')}
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
                    <span class="receipt-asset">${escapeHtml(intent.assetType || 'Unknown')}</span>
                </div>
                <div class="receipt-field">
                    <label>Canonical Hash:</label>
                    <code class="hash">${escapeHtml(truncateHash(intent.canonicalHash))}</code>
                </div>
                <div class="receipt-row">
                    <div class="receipt-field">
                        <label>From:</label>
                        <span>${escapeHtml(intent.from || '—')}</span>
                    </div>
                    <div class="receipt-field">
                        <label>To:</label>
                        <span>${escapeHtml(intent.to || '—')}</span>
                    </div>
                </div>
                <div class="receipt-row">
                    <div class="receipt-field">
                        <label>Amount:</label>
                        <span class="amount">${escapeHtml(String(intent.amount || 0))} ${escapeHtml(intent.unit || '')}</span>
                    </div>
                    <div class="receipt-field">
                        <label>Scope:</label>
                        <span>${escapeHtml(intent.scope || 'Unknown')}</span>
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

    // Render ledger entries (from TransactionHistoryEntry format)
    function renderLedgerEntries(transactions) {
        if (!transactions.length) {
            elements.ledgerList.innerHTML = '<p class="empty-state">No ledger entries found</p>';
            return;
        }

        // TransactionHistoryEntry has: id, timestamp, author, accounts[], decision_receipt_id, decision_hash
        const html = transactions.map(tx => {
            // Summarize account deltas
            const accountSummary = (tx.accounts || []).map(acct => {
                const delta = acct.credit ? `+${acct.credit}` : (acct.debit ? `-${acct.debit}` : '0');
                return `${escapeHtml(acct.account_id)}: ${escapeHtml(delta)} ${escapeHtml(acct.currency || '')}`;
            }).join(', ') || 'No accounts';

            return `
            <div class="receipt-item ledger">
                <div class="receipt-header">
                    <span class="receipt-type">Ledger Entry</span>
                    <span class="receipt-id">${escapeHtml(truncateHash(tx.id))}</span>
                </div>
                <div class="receipt-field">
                    <label>Author:</label>
                    <span>${escapeHtml(tx.author || '—')}</span>
                </div>
                <div class="receipt-field">
                    <label>Accounts:</label>
                    <span class="accounts">${accountSummary}</span>
                </div>
                ${tx.decision_hash ? `
                <div class="receipt-field">
                    <label>Decision Hash:</label>
                    <code class="hash">${escapeHtml(truncateHash(tx.decision_hash))}</code>
                </div>
                ` : ''}
                ${tx.decision_receipt_id ? `
                <div class="receipt-field">
                    <label>Decision Receipt ID:</label>
                    <span>${escapeHtml(tx.decision_receipt_id)}</span>
                </div>
                ` : ''}
                <div class="receipt-field">
                    <label>Timestamp:</label>
                    <span>${formatTimestamp(tx.timestamp)}</span>
                </div>
            </div>
        `}).join('');

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
