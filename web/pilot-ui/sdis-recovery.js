// SDIS Recovery Flow
const API_BASE = window.location.hostname === 'localhost' 
    ? 'http://localhost:8080/api/v1'
    : '/api/v1';

let currentMethod = null;
let backupData = null;

// Initialize on page load
document.addEventListener('DOMContentLoaded', () => {
    setupEventListeners();
    setupCodeInputs();
    setupVerificationInputs();
});

function setupEventListeners() {
    // Recovery method selection
    document.querySelectorAll('.recovery-method-card').forEach(card => {
        card.addEventListener('click', () => {
            const method = card.dataset.method;
            selectRecoveryMethod(method);
        });
    });

    // Back buttons
    document.querySelectorAll('.back-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            showMethodSelection();
        });
    });

    // Recovery code form
    document.getElementById('recoveryCodeForm').addEventListener('submit', handleRecoveryCodeSubmit);

    // Anchor recovery
    document.getElementById('anchorRecoveryForm').addEventListener('submit', handleAnchorRecoverySubmit);
    document.getElementById('verifyAnchorCodeBtn').addEventListener('click', verifyAnchorCode);
    document.getElementById('resendCodeBtn').addEventListener('click', resendVerificationCode);

    // Steward recovery
    document.getElementById('stewardRecoveryForm').addEventListener('submit', handleStewardRecoverySubmit);

    // Social recovery
    document.getElementById('checkRecoveryStatusBtn').addEventListener('click', checkSocialRecoveryStatus);

    // Backup import
    document.getElementById('selectBackupFileBtn').addEventListener('click', selectBackupFile);
    document.getElementById('pasteBackupBtn').addEventListener('click', showPasteBackupModal);
    document.getElementById('processBackupBtn').addEventListener('click', processBackupJson);
    document.getElementById('importBackupBtn').addEventListener('click', importBackup);
    document.getElementById('cancelImportBtn').addEventListener('click', () => {
        document.getElementById('backupPreview').style.display = 'none';
        backupData = null;
    });

    // Success actions
    document.getElementById('downloadNewCodesBtn').addEventListener('click', downloadRecoveryCodes);
    document.getElementById('continueToIdentityBtn').addEventListener('click', () => {
        window.location.href = 'sdis-identity.html';
    });

    // Contact support
    document.getElementById('contactSupportBtn').addEventListener('click', contactSupport);

    // Modal close
    document.querySelectorAll('.close-modal').forEach(btn => {
        btn.addEventListener('click', () => {
            btn.closest('.modal').style.display = 'none';
        });
    });
}

function setupCodeInputs() {
    const inputs = document.querySelectorAll('.code-input');
    inputs.forEach((input, index) => {
        input.addEventListener('input', (e) => {
            // Auto-advance to next input
            if (e.target.value.length === e.target.maxLength && index < inputs.length - 1) {
                inputs[index + 1].focus();
            }
        });

        input.addEventListener('keydown', (e) => {
            // Backspace to previous input
            if (e.key === 'Backspace' && !e.target.value && index > 0) {
                inputs[index - 1].focus();
            }
        });
    });
}

function setupVerificationInputs() {
    const inputs = document.querySelectorAll('.verification-digit');
    inputs.forEach((input, index) => {
        input.addEventListener('input', (e) => {
            if (e.target.value.length === 1 && index < inputs.length - 1) {
                inputs[index + 1].focus();
            }
        });

        input.addEventListener('keydown', (e) => {
            if (e.key === 'Backspace' && !e.target.value && index > 0) {
                inputs[index - 1].focus();
            }
        });
    });
}

function selectRecoveryMethod(method) {
    currentMethod = method;
    
    // Hide all screens
    hideAllScreens();
    
    // Show selected method screen
    const screens = {
        'recovery-code': 'recoveryCodeEntry',
        'anchor': 'anchorRecovery',
        'steward': 'stewardRecovery',
        'social': 'socialRecovery',
        'backup': 'backupImport'
    };
    
    const screenId = screens[method];
    if (screenId) {
        document.getElementById(screenId).style.display = 'block';
        
        // Load data for social recovery
        if (method === 'social') {
            loadGuardians();
        }
    }
}

function showMethodSelection() {
    hideAllScreens();
    document.getElementById('recoveryMethodSelection').style.display = 'block';
    currentMethod = null;
}

function hideAllScreens() {
    document.getElementById('recoveryMethodSelection').style.display = 'none';
    document.getElementById('recoveryCodeEntry').style.display = 'none';
    document.getElementById('anchorRecovery').style.display = 'none';
    document.getElementById('stewardRecovery').style.display = 'none';
    document.getElementById('socialRecovery').style.display = 'none';
    document.getElementById('backupImport').style.display = 'none';
    document.getElementById('recoverySuccess').style.display = 'none';
}

async function handleRecoveryCodeSubmit(e) {
    e.preventDefault();
    
    const codes = Array.from(document.querySelectorAll('.code-input'))
        .map(input => input.value.toUpperCase());

    try {
        const response = await fetch(`${API_BASE}/sdis/recovery/codes`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ recovery_codes: codes })
        });

        if (!response.ok) {
            throw new Error('Invalid recovery codes');
        }

        const result = await response.json();
        showRecoverySuccess(result);
        
    } catch (error) {
        alert('Recovery failed: ' + error.message);
    }
}

async function handleAnchorRecoverySubmit(e) {
    e.preventDefault();
    
    const anchorType = document.getElementById('anchorTypeSelect').value;
    const anchorValue = document.getElementById('anchorValueInput').value;

    try {
        const response = await fetch(`${API_BASE}/sdis/recovery/anchor`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                anchor_type: anchorType,
                anchor_value: anchorValue
            })
        });

        if (!response.ok) {
            throw new Error('Anchor not found or invalid');
        }

        // Show verification code input
        document.getElementById('verificationCodeSection').style.display = 'block';
        alert('Verification code sent to your anchor');
        
    } catch (error) {
        alert('Error: ' + error.message);
    }
}

async function verifyAnchorCode() {
    const digits = Array.from(document.querySelectorAll('.verification-digit'))
        .map(input => input.value);
    
    const code = digits.join('');
    
    if (code.length !== 6) {
        alert('Please enter all 6 digits');
        return;
    }

    try {
        const response = await fetch(`${API_BASE}/sdis/recovery/anchor/verify`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ verification_code: code })
        });

        if (!response.ok) {
            throw new Error('Invalid verification code');
        }

        const result = await response.json();
        showRecoverySuccess(result);
        
    } catch (error) {
        alert('Verification failed: ' + error.message);
    }
}

function resendVerificationCode() {
    alert('Verification code resent!');
}

async function handleStewardRecoverySubmit(e) {
    e.preventDefault();
    
    const did = document.getElementById('recoveryDid').value;
    const stewardContact = document.getElementById('stewardContact').value;
    const details = document.getElementById('verificationDetails').value;

    try {
        const response = await fetch(`${API_BASE}/sdis/recovery/steward`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                did: did || null,
                steward_contact: stewardContact,
                verification_details: details
            })
        });

        if (!response.ok) {
            throw new Error('Failed to submit recovery request');
        }

        const result = await response.json();
        
        // Show success status
        document.getElementById('requestReferenceId').textContent = result.reference_id;
        document.getElementById('stewardRequestStatus').style.display = 'block';
        document.getElementById('stewardRecoveryForm').style.display = 'none';
        
    } catch (error) {
        alert('Error: ' + error.message);
    }
}

function loadGuardians() {
    // Mock guardians - replace with API call
    const guardians = [
        { name: 'Alice Cooper', contact: 'alice@example.com', status: 'pending' },
        { name: 'Bob Smith', contact: 'bob@example.com', status: 'approved' },
        { name: 'Carol Jones', contact: 'carol@example.com', status: 'pending' },
        { name: 'Dave Wilson', contact: 'dave@example.com', status: 'approved' },
        { name: 'Eve Brown', contact: 'eve@example.com', status: 'pending' }
    ];

    const container = document.getElementById('guardiansList');
    container.innerHTML = guardians.map(guardian => `
        <div class="guardian-item">
            <div class="guardian-info">
                <div class="guardian-name">${guardian.name}</div>
                <div class="guardian-contact">${guardian.contact}</div>
            </div>
            <span class="guardian-status ${guardian.status}">
                ${guardian.status.toUpperCase()}
            </span>
        </div>
    `).join('');

    updateRecoveryProgress(guardians);
}

function updateRecoveryProgress(guardians) {
    const approved = guardians.filter(g => g.status === 'approved').length;
    const required = 3;
    
    document.getElementById('approvalCount').textContent = approved;
    document.getElementById('requiredApprovals').textContent = required;
    
    const percentage = (approved / required) * 100;
    document.getElementById('recoveryProgressBar').style.width = `${percentage}%`;
    
    if (approved >= required) {
        setTimeout(() => {
            showRecoverySuccess({ did: 'did:icn:recovered' });
        }, 1000);
    }
}

function checkSocialRecoveryStatus() {
    loadGuardians();
}

function selectBackupFile() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.json';
    
    input.onchange = async (e) => {
        const file = e.target.files[0];
        if (!file) return;
        
        try {
            const text = await file.text();
            backupData = JSON.parse(text);
            showBackupPreview();
        } catch (error) {
            alert('Error reading backup file: ' + error.message);
        }
    };
    
    input.click();
}

function showPasteBackupModal() {
    document.getElementById('backupJsonInput').value = '';
    document.getElementById('pasteBackupModal').style.display = 'flex';
}

function processBackupJson() {
    const json = document.getElementById('backupJsonInput').value;
    
    if (!json.trim()) {
        alert('Please paste backup JSON');
        return;
    }

    try {
        backupData = JSON.parse(json);
        document.getElementById('pasteBackupModal').style.display = 'none';
        showBackupPreview();
    } catch (error) {
        alert('Invalid JSON: ' + error.message);
    }
}

function showBackupPreview() {
    if (!backupData) return;
    
    document.getElementById('backupDid').textContent = backupData.did || 'Unknown';
    document.getElementById('backupDate').textContent = backupData.exported_at ? 
        new Date(backupData.exported_at).toLocaleString() : 'Unknown';
    
    document.getElementById('backupPreview').style.display = 'block';
}

async function importBackup() {
    if (!backupData) return;
    
    const passphrase = document.getElementById('backupPassphrase').value;

    try {
        const response = await fetch(`${API_BASE}/sdis/recovery/backup`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                backup_data: backupData,
                passphrase: passphrase || null
            })
        });

        if (!response.ok) {
            throw new Error('Failed to import backup');
        }

        const result = await response.json();
        showRecoverySuccess(result);
        
    } catch (error) {
        alert('Import failed: ' + error.message);
    }
}

function showRecoverySuccess(result) {
    hideAllScreens();
    document.getElementById('recoverySuccess').style.display = 'block';
    
    // Store recovered DID
    if (result.did) {
        localStorage.setItem('userDid', result.did);
    }
}

function downloadRecoveryCodes() {
    // Mock recovery codes - replace with real ones from API
    const codes = [
        'ABC12345',
        'DEF67890',
        'GHI24680',
        'JKL13579',
        'MNO98765',
        'PQR54321'
    ];
    
    const content = `ICN Identity Recovery Codes
Generated: ${new Date().toISOString()}

Store these codes in a secure, offline location.
Each code can only be used once.

${codes.map((code, i) => `${i + 1}. ${code}`).join('\n')}

⚠️ WARNING: Anyone with these codes can recover your identity.
Keep them secure and never share them.`;
    
    const blob = new Blob([content], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `icn-recovery-codes-${Date.now()}.txt`;
    a.click();
    URL.revokeObjectURL(url);
}

function contactSupport() {
    alert('Support contact functionality coming soon!\n\nFor now, please contact your cooperative administrator directly.');
}
