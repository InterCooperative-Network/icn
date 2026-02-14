// SDIS Proof Generator
const API_BASE = window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1'
    ? 'http://localhost:8080/api/v1'
    : `${window.location.origin}/api/v1`;

let currentProofType = null;
let generatedProof = null;

// Initialize on page load
document.addEventListener('DOMContentLoaded', () => {
    setupEventListeners();
    loadProofHistory();
});

function setupEventListeners() {
    document.getElementById('backBtn').addEventListener('click', () => {
        window.location.href = 'sdis-identity.html';
    });

    // Proof type selection
    document.querySelectorAll('.proof-type-card').forEach(card => {
        card.addEventListener('click', () => {
            const proofType = card.dataset.type;
            selectProofType(proofType);
        });
    });

    // Form handling
    document.getElementById('proofForm').addEventListener('submit', handleGenerateProof);
    document.getElementById('cancelProofBtn').addEventListener('click', resetToSelection);
    document.getElementById('newProofBtn').addEventListener('click', resetToSelection);

    // Generated proof actions
    document.getElementById('downloadProofBtn').addEventListener('click', downloadProof);
    document.getElementById('shareProofBtn').addEventListener('click', shareProof);
    document.getElementById('showProofQrBtn').addEventListener('click', showProofQr);

    // Verification actions
    document.getElementById('pasteProofBtn').addEventListener('click', showPasteModal);
    document.getElementById('scanQrBtn').addEventListener('click', scanQrCode);
    document.getElementById('uploadProofBtn').addEventListener('click', uploadProofFile);
    document.getElementById('verifyPastedBtn').addEventListener('click', verifyPastedProof);

    // Modal close buttons
    document.querySelectorAll('.close-modal').forEach(btn => {
        btn.addEventListener('click', () => {
            btn.closest('.modal').style.display = 'none';
        });
    });

    // Copy buttons
    document.querySelectorAll('.copy-btn').forEach(btn => {
        btn.addEventListener('click', (e) => {
            copySignature(btn);
        });
    });
}

function selectProofType(type) {
    currentProofType = type;
    
    document.getElementById('proofTypeSelection').style.display = 'none';
    document.getElementById('proofConfiguration').style.display = 'block';
    document.getElementById('generatedProof').style.display = 'none';
    
    document.getElementById('proofTypeDisplay').value = formatProofType(type);
    
    renderProofOptions(type);
}

function formatProofType(type) {
    const types = {
        'identity': 'Identity Proof',
        'attribute': 'Attribute Proof',
        'membership': 'Membership Proof',
        'age': 'Age Proof',
        'credential': 'Credential Proof',
        'custom': 'Custom Proof'
    };
    return types[type] || type;
}

function renderProofOptions(type) {
    const container = document.getElementById('proofOptions');
    
    const options = {
        'identity': `
            <div class="option-group">
                <label>What to prove:</label>
                <div class="checkbox-group">
                    <label><input type="checkbox" name="prove" value="did_ownership" checked disabled> DID Ownership</label>
                    <label><input type="checkbox" name="prove" value="key_control"> Key Control</label>
                    <label><input type="checkbox" name="prove" value="anchor_binding"> Anchor Binding</label>
                </div>
            </div>
        `,
        'attribute': `
            <div class="option-group">
                <label>Select attributes to prove:</label>
                <div class="checkbox-group">
                    <label><input type="checkbox" name="attribute" value="name"> Name</label>
                    <label><input type="checkbox" name="attribute" value="email"> Email</label>
                    <label><input type="checkbox" name="attribute" value="phone"> Phone</label>
                    <label><input type="checkbox" name="attribute" value="location"> Location</label>
                    <label><input type="checkbox" name="attribute" value="organization"> Organization</label>
                </div>
            </div>
        `,
        'membership': `
            <div class="option-group">
                <label>Cooperative:</label>
                <select name="coop_id" class="form-control">
                    <option value="">Select cooperative...</option>
                    <option value="test-coop">Test Cooperative</option>
                </select>
            </div>
            <div class="option-group">
                <label>Include membership details:</label>
                <div class="checkbox-group">
                    <label><input type="checkbox" name="include" value="join_date"> Join Date</label>
                    <label><input type="checkbox" name="include" value="member_number"> Member Number</label>
                    <label><input type="checkbox" name="include" value="roles"> Roles</label>
                    <label><input type="checkbox" name="include" value="standing"> Good Standing</label>
                </div>
            </div>
        `,
        'age': `
            <div class="option-group">
                <label>Age verification:</label>
                <select name="age_threshold" class="form-control">
                    <option value="18">18 years or older</option>
                    <option value="21">21 years or older</option>
                    <option value="65">65 years or older</option>
                </select>
            </div>
            <div class="option-group">
                <label>
                    <input type="checkbox" name="hide_exact_age" checked>
                    Hide exact age (only prove threshold)
                </label>
            </div>
        `,
        'credential': `
            <div class="option-group">
                <label>Select credential:</label>
                <select name="credential_id" class="form-control">
                    <option value="">Select credential...</option>
                    <option value="steward-cert">Steward Certificate</option>
                </select>
            </div>
            <div class="option-group">
                <label>What to reveal:</label>
                <div class="checkbox-group">
                    <label><input type="checkbox" name="reveal" value="issuer"> Issuer</label>
                    <label><input type="checkbox" name="reveal" value="issue_date"> Issue Date</label>
                    <label><input type="checkbox" name="reveal" value="expiry"> Expiry Date</label>
                    <label><input type="checkbox" name="reveal" value="claims"> Specific Claims</label>
                </div>
            </div>
        `,
        'custom': `
            <div class="option-group">
                <label>Custom claims (JSON):</label>
                <textarea name="custom_claims" class="form-control" rows="5" 
                          placeholder='{"claim_name": "claim_value"}'></textarea>
            </div>
        `
    };
    
    container.innerHTML = options[type] || '';
}

async function handleGenerateProof(e) {
    e.preventDefault();
    
    const formData = new FormData(e.target);
    const did = localStorage.getItem('userDid');
    
    if (!did) {
        alert('No DID found. Please enroll first.');
        return;
    }

    const proofData = {
        type: currentProofType,
        issuer_did: did,
        recipient_did: document.getElementById('recipientDid').value || null,
        expiration: document.getElementById('proofExpiration').value,
        require_challenge: document.getElementById('requireChallenge').checked,
        claims: extractClaimsFromForm(formData)
    };

    try {
        const response = await fetch(`${API_BASE}/sdis/proofs`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(proofData)
        });

        if (!response.ok) {
            throw new Error('Failed to generate proof');
        }

        generatedProof = await response.json();
        displayGeneratedProof();
        
    } catch (error) {
        alert('Error generating proof: ' + error.message);
    }
}

function extractClaimsFromForm(formData) {
    const claims = {};
    
    for (const [key, value] of formData.entries()) {
        if (value && key !== 'expiration' && key !== 'require_challenge') {
            claims[key] = value;
        }
    }
    
    // Handle checkboxes
    const checkboxes = document.querySelectorAll('#proofOptions input[type="checkbox"]:checked');
    checkboxes.forEach(cb => {
        claims[cb.value] = true;
    });
    
    return claims;
}

function displayGeneratedProof() {
    document.getElementById('proofConfiguration').style.display = 'none';
    document.getElementById('generatedProof').style.display = 'block';
    
    // Update proof display
    document.getElementById('proofBadge').textContent = formatProofType(generatedProof.type);
    document.getElementById('proofId').textContent = `ID: ${generatedProof.id}`;
    document.getElementById('proofIssuer').textContent = generatedProof.issuer_did;
    document.getElementById('proofCreated').textContent = new Date(generatedProof.created_at).toLocaleString();
    document.getElementById('proofExpires').textContent = generatedProof.expires_at ? 
        new Date(generatedProof.expires_at).toLocaleString() : 'Never';
    document.getElementById('proofRecipient').textContent = generatedProof.recipient_did || 'Public';
    
    // Display claims
    const claimsContainer = document.getElementById('proofClaims');
    claimsContainer.innerHTML = '<h3>Proven Claims</h3>';
    
    Object.entries(generatedProof.claims || {}).forEach(([key, value]) => {
        claimsContainer.innerHTML += `
            <div class="claim-item">
                <span class="claim-label">${key.replace(/_/g, ' ')}</span>
                <span class="claim-value">${value}</span>
            </div>
        `;
    });
    
    // Display signature
    document.getElementById('proofSignature').textContent = generatedProof.signature || 'N/A';
    
    // Refresh history
    loadProofHistory();
}

function resetToSelection() {
    document.getElementById('proofTypeSelection').style.display = 'block';
    document.getElementById('proofConfiguration').style.display = 'none';
    document.getElementById('generatedProof').style.display = 'none';
    currentProofType = null;
    generatedProof = null;
    document.getElementById('proofForm').reset();
}

function downloadProof() {
    if (!generatedProof) return;
    
    const blob = new Blob([JSON.stringify(generatedProof, null, 2)], { 
        type: 'application/json' 
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `icn-proof-${generatedProof.id}.json`;
    a.click();
    URL.revokeObjectURL(url);
    
    alert('Proof downloaded');
}

function shareProof() {
    if (!generatedProof) return;
    
    const shareUrl = `${window.location.origin}/verify-proof.html?id=${generatedProof.id}`;
    
    navigator.clipboard.writeText(shareUrl).then(() => {
        alert('Share link copied to clipboard!');
    }).catch(() => {
        alert(`Share URL: ${shareUrl}`);
    });
}

function showProofQr() {
    const qrContainer = document.getElementById('proofQrCode');
    qrContainer.innerHTML = `
        <div style="width: 200px; height: 200px; background: #f0f0f0; 
                    display: flex; align-items: center; justify-content: center;
                    border-radius: 8px;">
            <span style="color: #999;">QR Code</span>
        </div>
    `;
    
    document.getElementById('proofQrModal').style.display = 'flex';
}

async function loadProofHistory() {
    const did = localStorage.getItem('userDid');
    if (!did) return;

    try {
        const response = await fetch(`${API_BASE}/sdis/proofs?issuer_did=${did}`);
        if (!response.ok) throw new Error('Failed to load history');
        
        const proofs = await response.json();
        displayProofHistory(proofs);
    } catch (error) {
        console.error('Error loading proof history:', error);
    }
}

function displayProofHistory(proofs) {
    const container = document.getElementById('proofHistoryList');
    
    if (proofs.length === 0) {
        container.innerHTML = '<p class="empty-state">No proofs generated yet</p>';
        return;
    }

    container.innerHTML = proofs.map(proof => {
        const isExpired = proof.expires_at && new Date(proof.expires_at) < new Date();
        const status = isExpired ? 'expired' : 'valid';
        
        return `
            <div class="history-item" onclick="viewHistoryProof('${proof.id}')">
                <div class="history-info">
                    <div class="history-type">${formatProofType(proof.type)}</div>
                    <div class="history-date">
                        ${new Date(proof.created_at).toLocaleDateString()}
                    </div>
                </div>
                <span class="history-status ${status}">
                    ${status.toUpperCase()}
                </span>
            </div>
        `;
    }).join('');
}

function viewHistoryProof(proofId) {
    alert(`View proof ${proofId} - functionality coming soon!`);
}

function showPasteModal() {
    document.getElementById('proofJsonInput').value = '';
    document.getElementById('pasteProofModal').style.display = 'flex';
}

async function verifyPastedProof() {
    const proofJson = document.getElementById('proofJsonInput').value;
    
    if (!proofJson.trim()) {
        alert('Please paste a proof JSON');
        return;
    }

    try {
        const proof = JSON.parse(proofJson);
        await verifyProof(proof);
    } catch (error) {
        alert('Invalid proof JSON: ' + error.message);
    }
}

async function verifyProof(proof) {
    try {
        const response = await fetch(`${API_BASE}/sdis/proofs/verify`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(proof)
        });

        if (!response.ok) throw new Error('Verification failed');
        
        const result = await response.json();
        displayVerificationResult(result, proof);
        
        document.getElementById('pasteProofModal').style.display = 'none';
        
    } catch (error) {
        alert('Verification error: ' + error.message);
    }
}

function displayVerificationResult(result, proof) {
    const container = document.getElementById('verificationResult');
    container.style.display = 'block';
    container.className = result.valid ? 'success' : 'failure';
    
    container.innerHTML = `
        <h3>${result.valid ? '✅ Valid Proof' : '❌ Invalid Proof'}</h3>
        <div class="verification-details">
            <p><strong>Issuer:</strong> ${proof.issuer_did}</p>
            <p><strong>Type:</strong> ${formatProofType(proof.type)}</p>
            <p><strong>Created:</strong> ${new Date(proof.created_at).toLocaleString()}</p>
            ${result.message ? `<p><strong>Message:</strong> ${result.message}</p>` : ''}
        </div>
    `;
}

function scanQrCode() {
    alert('QR code scanning requires camera access. Feature coming soon!');
}

function uploadProofFile() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.json';
    
    input.onchange = async (e) => {
        const file = e.target.files[0];
        if (!file) return;
        
        try {
            const text = await file.text();
            const proof = JSON.parse(text);
            await verifyProof(proof);
        } catch (error) {
            alert('Error reading file: ' + error.message);
        }
    };
    
    input.click();
}

async function copySignature(button) {
    const signature = document.getElementById('proofSignature').textContent;
    
    try {
        await navigator.clipboard.writeText(signature);
        button.textContent = '✓';
        setTimeout(() => {
            button.textContent = '📋';
        }, 2000);
    } catch (error) {
        alert('Failed to copy signature');
    }
}
