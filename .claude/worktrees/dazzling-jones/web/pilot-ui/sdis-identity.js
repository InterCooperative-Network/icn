// SDIS Identity Viewer
const API_BASE = window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1'
    ? 'http://localhost:8080/api/v1'
    : `${window.location.origin}/api/v1`;

let currentIdentity = null;

// Initialize on page load
document.addEventListener('DOMContentLoaded', async () => {
    setupEventListeners();
    await loadIdentity();
});

function setupEventListeners() {
    document.getElementById('backBtn').addEventListener('click', () => {
        window.location.href = 'index.html';
    });

    document.getElementById('retryBtn').addEventListener('click', loadIdentity);
    document.getElementById('showQrBtn').addEventListener('click', showQrCode);
    document.getElementById('addAnchorBtn').addEventListener('click', showAddAnchorModal);
    document.getElementById('requestProofBtn').addEventListener('click', requestProof);
    document.getElementById('rotateKeysBtn').addEventListener('click', rotateKeys);
    
    // Recovery buttons
    document.getElementById('viewRecoveryBtn').addEventListener('click', viewRecoveryCodes);
    document.getElementById('exportIdentityBtn').addEventListener('click', exportIdentity);
    document.getElementById('revokeIdentityBtn').addEventListener('click', revokeIdentity);

    // Modal close buttons
    document.querySelectorAll('.close-modal').forEach(btn => {
        btn.addEventListener('click', () => {
            btn.closest('.modal').style.display = 'none';
        });
    });

    // Add anchor form
    document.getElementById('addAnchorForm').addEventListener('submit', handleAddAnchor);
    document.getElementById('cancelAnchorBtn').addEventListener('click', () => {
        document.getElementById('addAnchorModal').style.display = 'none';
    });

    // Copy buttons
    document.querySelectorAll('.copy-btn').forEach(btn => {
        btn.addEventListener('click', (e) => {
            const target = e.currentTarget.dataset.copy;
            copyToClipboard(target, btn);
        });
    });
}

async function loadIdentity() {
    showLoading();

    try {
        const did = localStorage.getItem('userDid');
        if (!did) {
            throw new Error('No DID found. Please enroll first.');
        }

        // Load identity info
        const response = await fetch(`${API_BASE}/sdis/identity/${did}`);
        if (!response.ok) {
            throw new Error('Failed to load identity');
        }

        currentIdentity = await response.json();
        displayIdentity();

        // Load anchors
        await loadAnchors();

        // Load credentials
        await loadCredentials();

        // Load activity
        await loadActivity();

    } catch (error) {
        showError(error.message);
    }
}

function displayIdentity() {
    // Update status badge
    const statusBadge = document.getElementById('statusBadge');
    const statusIcon = document.getElementById('statusIcon');
    const statusText = document.getElementById('statusText');
    
    statusBadge.className = 'status-badge ' + currentIdentity.status;
    statusIcon.textContent = currentIdentity.status === 'enrolled' ? '✓' : '⏳';
    statusText.textContent = currentIdentity.status.charAt(0).toUpperCase() + 
                            currentIdentity.status.slice(1);

    // Update status details
    document.getElementById('enrollmentDate').textContent = 
        new Date(currentIdentity.enrolled_at).toLocaleDateString();
    document.getElementById('stewardName').textContent = 
        currentIdentity.steward_name || 'Unknown';
    document.getElementById('trustScore').textContent = 
        (currentIdentity.trust_score || 0).toFixed(2);

    // Update DID displays
    const didValue = currentIdentity.did;
    document.getElementById('didValue').textContent = didValue;
    document.getElementById('qrDidValue').textContent = didValue;

    // Show sections
    document.getElementById('loadingState').style.display = 'none';
    document.getElementById('errorState').style.display = 'none';
    document.getElementById('identityOverview').style.display = 'block';
    document.getElementById('recoverySection').style.display = 'block';
    document.getElementById('activitySection').style.display = 'block';
}

async function loadAnchors() {
    try {
        const response = await fetch(
            `${API_BASE}/sdis/anchors/${currentIdentity.did}`
        );
        
        if (!response.ok) throw new Error('Failed to load anchors');
        
        const anchors = await response.json();
        displayAnchors(anchors);
    } catch (error) {
        console.error('Error loading anchors:', error);
    }
}

function displayAnchors(anchors) {
    const container = document.getElementById('anchorsList');
    
    if (anchors.length === 0) {
        container.innerHTML = '<p class="empty-state">No identity anchors yet</p>';
        return;
    }

    container.innerHTML = anchors.map(anchor => `
        <div class="anchor-item">
            <div class="anchor-info">
                <div class="anchor-type">${anchor.anchor_type}</div>
                <div class="anchor-value">${anchor.anchor_value}</div>
            </div>
            <div class="anchor-status ${anchor.verified ? 'verified' : 'pending'}">
                ${anchor.verified ? 'Verified' : 'Pending'}
            </div>
            <div class="anchor-actions">
                ${!anchor.verified ? `
                    <button class="icon-btn" onclick="verifyAnchor('${anchor.id}')">
                        ✓
                    </button>
                ` : ''}
                <button class="icon-btn danger" onclick="removeAnchor('${anchor.id}')">
                    ✕
                </button>
            </div>
        </div>
    `).join('');
}

async function loadCredentials() {
    try {
        const response = await fetch(
            `${API_BASE}/sdis/credentials/${currentIdentity.did}`
        );
        
        if (!response.ok) throw new Error('Failed to load credentials');
        
        const credentials = await response.json();
        displayCredentials(credentials);
    } catch (error) {
        console.error('Error loading credentials:', error);
    }
}

function displayCredentials(credentials) {
    const container = document.getElementById('credentialsList');
    
    if (credentials.length === 0) {
        container.innerHTML = '<p class="empty-state">No credentials yet</p>';
        return;
    }

    container.innerHTML = credentials.map(cred => `
        <div class="credential-item">
            <div class="credential-header">
                <div>
                    <div class="credential-type">${cred.type}</div>
                    <div class="credential-issuer">Issued by: ${cred.issuer}</div>
                </div>
                <button class="icon-btn" onclick="viewCredential('${cred.id}')">
                    👁️
                </button>
            </div>
            <ul class="credential-claims">
                ${cred.claims.map(claim => `<li>${claim}</li>`).join('')}
            </ul>
        </div>
    `).join('');
}

async function loadActivity() {
    const container = document.getElementById('activityLog');
    
    // Mock activity for now - replace with real API call
    const activities = [
        {
            icon: '✅',
            title: 'Identity Enrolled',
            description: 'Successfully enrolled with steward verification',
            time: new Date().toISOString()
        },
        {
            icon: '🔑',
            title: 'Keys Generated',
            description: 'Hybrid key pair created (Ed25519 + ML-DSA)',
            time: new Date().toISOString()
        }
    ];

    container.innerHTML = activities.map(activity => `
        <div class="activity-item">
            <div class="activity-icon">${activity.icon}</div>
            <div class="activity-content">
                <div class="activity-title">${activity.title}</div>
                <div class="activity-description">${activity.description}</div>
            </div>
            <div class="activity-time">
                ${new Date(activity.time).toLocaleString()}
            </div>
        </div>
    `).join('');
}

function showQrCode() {
    // Generate QR code using a library (qrcode.js)
    const qrContainer = document.getElementById('qrCode');
    qrContainer.innerHTML = '';
    
    // For now, show placeholder - integrate QR library later
    qrContainer.innerHTML = `
        <div style="width: 200px; height: 200px; background: #f0f0f0; 
                    display: flex; align-items: center; justify-content: center;
                    border-radius: 8px;">
            <span style="color: #999;">QR Code</span>
        </div>
    `;
    
    document.getElementById('qrModal').style.display = 'flex';
}

function showAddAnchorModal() {
    document.getElementById('addAnchorForm').reset();
    document.getElementById('addAnchorModal').style.display = 'flex';
}

async function handleAddAnchor(e) {
    e.preventDefault();
    
    const anchorType = document.getElementById('anchorType').value;
    const anchorValue = document.getElementById('anchorValue').value;
    const requireVerification = document.getElementById('requireVerification').checked;

    try {
        const response = await fetch(`${API_BASE}/sdis/anchors`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                did: currentIdentity.did,
                anchor_type: anchorType,
                anchor_value: anchorValue,
                require_verification: requireVerification
            })
        });

        if (!response.ok) throw new Error('Failed to add anchor');

        document.getElementById('addAnchorModal').style.display = 'none';
        await loadAnchors();
        showNotification('Anchor added successfully', 'success');
    } catch (error) {
        showNotification('Error: ' + error.message, 'error');
    }
}

async function verifyAnchor(anchorId) {
    if (!confirm('Verify this anchor?')) return;

    try {
        const response = await fetch(`${API_BASE}/sdis/anchors/${anchorId}/verify`, {
            method: 'POST'
        });

        if (!response.ok) throw new Error('Verification failed');

        await loadAnchors();
        showNotification('Anchor verified', 'success');
    } catch (error) {
        showNotification('Error: ' + error.message, 'error');
    }
}

async function removeAnchor(anchorId) {
    if (!confirm('Remove this anchor? This action cannot be undone.')) return;

    try {
        const response = await fetch(`${API_BASE}/sdis/anchors/${anchorId}`, {
            method: 'DELETE'
        });

        if (!response.ok) throw new Error('Failed to remove anchor');

        await loadAnchors();
        showNotification('Anchor removed', 'success');
    } catch (error) {
        showNotification('Error: ' + error.message, 'error');
    }
}

function requestProof() {
    alert('Proof request functionality coming soon!');
}

function rotateKeys() {
    if (!confirm('Rotate your cryptographic keys? This will generate new keys while maintaining your identity.')) return;
    alert('Key rotation functionality coming soon!');
}

function viewRecoveryCodes() {
    alert('Recovery codes viewer coming soon! Store these securely offline.');
}

function exportIdentity() {
    const backup = {
        did: currentIdentity.did,
        exported_at: new Date().toISOString(),
        warning: 'Keep this file secure and offline'
    };
    
    const blob = new Blob([JSON.stringify(backup, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `icn-identity-backup-${Date.now()}.json`;
    a.click();
    URL.revokeObjectURL(url);
    
    showNotification('Identity backup downloaded', 'success');
}

function revokeIdentity() {
    if (!confirm('⚠️ DANGER: Revoke your identity? This action CANNOT be undone and will permanently disable your identity.')) {
        return;
    }
    
    if (!confirm('Are you absolutely sure? Type YES to confirm (case-sensitive)') === 'YES') {
        return;
    }
    
    alert('Identity revocation functionality coming soon!');
}

function viewCredential(credId) {
    alert(`View credential ${credId} - functionality coming soon!`);
}

async function copyToClipboard(target, button) {
    const text = target === 'did' ? 
        document.getElementById('didValue').textContent :
        document.getElementById('qrDidValue').textContent;
    
    try {
        await navigator.clipboard.writeText(text);
        button.classList.add('copied');
        button.textContent = '✓';
        
        setTimeout(() => {
            button.classList.remove('copied');
            button.textContent = '📋';
        }, 2000);
        
        showNotification('Copied to clipboard', 'success');
    } catch (error) {
        showNotification('Failed to copy', 'error');
    }
}

function showLoading() {
    document.getElementById('loadingState').style.display = 'block';
    document.getElementById('errorState').style.display = 'none';
    document.getElementById('identityOverview').style.display = 'none';
    document.getElementById('recoverySection').style.display = 'none';
    document.getElementById('activitySection').style.display = 'none';
}

function showError(message) {
    document.getElementById('errorMessage').textContent = message;
    document.getElementById('loadingState').style.display = 'none';
    document.getElementById('errorState').style.display = 'block';
    document.getElementById('identityOverview').style.display = 'none';
}

function showNotification(message, type) {
    // Simple notification - enhance with toast library later
    alert(message);
}
