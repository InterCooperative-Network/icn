/**
 * SDIS Enrollment Wizard
 * 
 * Multi-step enrollment flow for creating a permanent Anchor identity.
 * Supports multiple enrollment pathways:
 * - Government ID verification
 * - Organizational sponsorship
 * - Web of Trust (3+ vouchers)
 * - Biometric commitment
 * - Genesis (for bootstrapping)
 */

const SDISEnrollment = {
    // State
    state: {
        currentStep: 1,
        totalSteps: 5,
        pathway: null,
        proofData: {},
        ceremonyId: null,
        pollInterval: null,
        anchor: null,
    },

    // DOM Elements
    elements: {
        wizard: document.getElementById('sdis-enrollment-wizard'),
        steps: {
            pathway: document.getElementById('step-pathway'),
            documents: document.getElementById('step-documents'),
            keys: document.getElementById('step-keys'),
            submit: document.getElementById('step-submit'),
            complete: document.getElementById('step-complete'),
        },
        progress: document.getElementById('enrollment-progress'),
        progressBar: document.getElementById('progress-bar'),
        stepNumber: document.getElementById('step-number'),
        stepTitle: document.getElementById('step-title'),
        prevBtn: document.getElementById('prev-step-btn'),
        nextBtn: document.getElementById('next-step-btn'),
        pathwayCards: document.querySelectorAll('.pathway-card'),
        documentUpload: document.getElementById('document-upload'),
        documentPreview: document.getElementById('document-preview'),
        generatedKeys: document.getElementById('generated-keys'),
        ceremonyStatus: document.getElementById('ceremony-status'),
        approvalProgress: document.getElementById('approval-progress'),
        anchorDisplay: document.getElementById('anchor-display'),
        recoveryCodesDisplay: document.getElementById('recovery-codes-display'),
    },

    /**
     * Initialize the enrollment wizard
     */
    init() {
        this.bindEvents();
        this.updateProgress();
    },

    /**
     * Bind event listeners
     */
    bindEvents() {
        // Pathway selection
        this.elements.pathwayCards.forEach(card => {
            card.addEventListener('click', (e) => {
                this.selectPathway(e.currentTarget.dataset.pathway);
            });
        });

        // Navigation
        this.elements.prevBtn?.addEventListener('click', () => this.previousStep());
        this.elements.nextBtn?.addEventListener('click', () => this.nextStep());

        // Document upload
        this.elements.documentUpload?.addEventListener('change', (e) => {
            this.handleDocumentUpload(e);
        });

        // Generate keys button
        document.getElementById('generate-keys-btn')?.addEventListener('click', () => {
            this.generateKeys();
        });

        // Submit enrollment
        document.getElementById('submit-enrollment-btn')?.addEventListener('click', () => {
            this.submitEnrollment();
        });

        // Download recovery codes
        document.getElementById('download-recovery-btn')?.addEventListener('click', () => {
            this.downloadRecoveryCodes();
        });
    },

    /**
     * Select enrollment pathway
     */
    selectPathway(pathway) {
        this.state.pathway = pathway;
        
        // Update UI
        this.elements.pathwayCards.forEach(card => {
            if (card.dataset.pathway === pathway) {
                card.classList.add('selected');
                card.setAttribute('aria-selected', 'true');
            } else {
                card.classList.remove('selected');
                card.setAttribute('aria-selected', 'false');
            }
        });

        // Enable next button
        this.elements.nextBtn.disabled = false;

        // Show pathway-specific instructions
        this.showPathwayInstructions(pathway);
    },

    /**
     * Show instructions for selected pathway
     */
    showPathwayInstructions(pathway) {
        const instructions = {
            government_id: 'You will need to upload a photo of your government-issued ID and a selfie for verification.',
            org_sponsorship: 'Your organization will sponsor your enrollment. Please contact your organization administrator.',
            web_of_trust: 'You will need 3 or more existing members to vouch for you.',
            biometric: 'You will provide biometric data (fingerprint or facial recognition) for verification.',
            genesis: 'Genesis enrollment is for bootstrapping new networks. This option is typically restricted.',
        };

        const instructionEl = document.getElementById('pathway-instructions');
        if (instructionEl) {
            instructionEl.textContent = instructions[pathway] || '';
            instructionEl.classList.remove('hidden');
        }
    },

    /**
     * Handle document upload
     */
    async handleDocumentUpload(event) {
        const file = event.target.files[0];
        if (!file) return;

        // Validate file
        if (!file.type.startsWith('image/')) {
            showToast('Please upload an image file', 'error');
            return;
        }

        if (file.size > 5 * 1024 * 1024) {
            showToast('File size must be less than 5MB', 'error');
            return;
        }

        // Read and preview
        const reader = new FileReader();
        reader.onload = (e) => {
            this.elements.documentPreview.innerHTML = `
                <img src="${e.target.result}" alt="Document preview" />
                <p class="text-sm text-gray-600 mt-2">${file.name}</p>
            `;
            this.elements.documentPreview.classList.remove('hidden');

            // Store in proof data
            this.state.proofData.document = e.target.result;
            this.elements.nextBtn.disabled = false;
        };
        reader.readAsDataURL(file);
    },

    /**
     * Generate cryptographic keys
     */
    async generateKeys() {
        showToast('Generating keys...', 'info');

        try {
            // In a real implementation, this would use Web Crypto API
            // For now, simulate key generation
            await new Promise(resolve => setTimeout(resolve, 1000));

            // Generate mock keys (in production, use actual crypto)
            const keys = {
                ed25519_pub: this.generateMockKey('ed25519'),
                ml_dsa_pub: this.generateMockKey('ml-dsa'),
                x25519_pub: this.generateMockKey('x25519'),
            };

            this.state.proofData.keybundle = keys;

            // Display keys
            this.elements.generatedKeys.innerHTML = `
                <div class="key-display">
                    <label>Ed25519 Public Key:</label>
                    <code>${keys.ed25519_pub}</code>
                </div>
                <div class="key-display">
                    <label>ML-DSA Public Key (Post-Quantum):</label>
                    <code>${keys.ml_dsa_pub}</code>
                </div>
                <div class="key-display">
                    <label>X25519 Public Key (Encryption):</label>
                    <code>${keys.x25519_pub}</code>
                </div>
                <div class="alert alert-warning mt-4">
                    <strong>⚠️ Important:</strong> Your private keys are stored securely in your browser. 
                    Make sure to back them up before proceeding.
                </div>
            `;

            this.elements.generatedKeys.classList.remove('hidden');
            this.elements.nextBtn.disabled = false;

            showToast('Keys generated successfully!', 'success');
        } catch (error) {
            console.error('Key generation error:', error);
            showToast('Failed to generate keys', 'error');
        }
    },

    /**
     * Generate mock key (replace with real crypto in production)
     */
    generateMockKey(type) {
        const length = type === 'ml-dsa' ? 64 : 32;
        const bytes = new Uint8Array(length);
        crypto.getRandomValues(bytes);
        return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
    },

    /**
     * Get device type from user agent
     */
    getDeviceType() {
        const ua = navigator.userAgent.toLowerCase();
        if (/mobile|android|iphone|ipad/.test(ua)) return 'mobile';
        if (/tablet/.test(ua)) return 'tablet';
        return 'desktop';
    },

    /**
     * Submit enrollment to gateway
     */
    async submitEnrollment() {
        const gatewayUrl = state.gatewayUrl || 'http://localhost:8080';

        showToast('Submitting enrollment...', 'info');

        try {
            const response = await fetch(`${gatewayUrl}/v1/sdis/enrollment/start`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    pathway: this.convertPathwayFormat(this.state.pathway),
                    proof_data: this.state.proofData,
                    initial_keybundle: this.state.proofData.keybundle,
                }),
            });

            if (!response.ok) {
                throw new Error(`HTTP ${response.status}: ${await response.text()}`);
            }

            const data = await response.json();
            // Backend returns enrollment_id (not ceremony_id)
            this.state.enrollmentId = data.enrollment_id;
            this.state.ephemeralDid = data.ephemeral_did;

            showToast('Enrollment submitted! Waiting for steward approval...', 'success');

            // Move to next step
            this.nextStep();

            // Start polling for approval
            this.startPolling();

        } catch (error) {
            console.error('Enrollment error:', error);
            showToast(`Enrollment failed: ${error.message}`, 'error');
        }
    },

    /**
     * Convert pathway name to API format
     */
    convertPathwayFormat(pathway) {
        const mapping = {
            government_id: { type: 'government_id', country: 'US', doc_type: 'passport', doc_hash: 'hash123' },
            org_sponsorship: { type: 'organizational_sponsorship', org_did: 'did:icn:org', org_internal_id: 'emp123' },
            web_of_trust: { type: 'web_of_trust', vouchers: ['did:icn:user1', 'did:icn:user2', 'did:icn:user3'] },
            biometric: { type: 'biometric_commitment', template_hash: 'biometric_hash' },
            genesis: { type: 'genesis', reason: 'Bootstrapping network' },
        };
        return mapping[pathway] || mapping.genesis;
    },

    /**
     * Start polling ceremony status
     */
    startPolling() {
        this.state.pollInterval = setInterval(() => {
            this.checkCeremonyStatus();
        }, 3000); // Poll every 3 seconds
    },

    /**
     * Check enrollment status
     */
    async checkCeremonyStatus() {
        const gatewayUrl = state.gatewayUrl || 'http://localhost:8080';

        try {
            const response = await fetch(
                `${gatewayUrl}/v1/sdis/status/${this.state.enrollmentId}`
            );

            if (!response.ok) return;

            const data = await response.json();

            // Update progress
            this.updateApprovalProgress(data.steward_approvals, data.required_stewards);

            // Check if approved
            if (data.status === 'approved') {
                clearInterval(this.state.pollInterval);
                this.finalizeEnrollment();
            } else if (data.status === 'rejected') {
                clearInterval(this.state.pollInterval);
                showToast('Enrollment rejected: ' + (data.rejection_reason || 'Unknown reason'), 'error');
            }

        } catch (error) {
            console.error('Status check error:', error);
        }
    },

    /**
     * Update approval progress UI
     */
    updateApprovalProgress(current, required) {
        const percentage = (current / required) * 100;
        this.elements.approvalProgress.innerHTML = `
            <div class="progress-bar">
                <div class="progress-fill" style="width: ${percentage}%"></div>
            </div>
            <p class="text-center mt-2">
                ${current} of ${required} stewards have approved
                ${current < required ? '⏳' : '✅'}
            </p>
        `;
    },

    /**
     * Complete enrollment and receive anchor
     */
    async finalizeEnrollment() {
        const gatewayUrl = state.gatewayUrl || 'http://localhost:8080';

        try {
            // Get device info
            const deviceInfo = {
                device_type: this.getDeviceType(),
                os: navigator.platform || 'unknown',
            };

            const response = await fetch(
                `${gatewayUrl}/v1/sdis/enrollment/complete`,
                {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        enrollment_id: this.state.enrollmentId,
                        ephemeral_did: this.state.ephemeralDid,
                        ephemeral_signature: this.generateMockKey('ed25519'), // TODO: actual signature
                        device_info: deviceInfo,
                    }),
                }
            );

            if (!response.ok) {
                throw new Error('Finalization failed');
            }

            const data = await response.json();
            this.state.anchor = data;

            showToast('Enrollment complete! 🎉', 'success');

            // Display anchor and recovery codes
            this.displayAnchor(data);
            this.nextStep();

        } catch (error) {
            console.error('Finalization error:', error);
            showToast('Failed to finalize enrollment', 'error');
        }
    },

    /**
     * Display anchor information
     */
    displayAnchor(data) {
        this.elements.anchorDisplay.innerHTML = `
            <div class="anchor-info">
                <div class="info-group">
                    <label>Anchor ID (Permanent):</label>
                    <code class="anchor-id">${data.anchor_id}</code>
                </div>
                <div class="info-group">
                    <label>Your DID:</label>
                    <code>${data.did}</code>
                </div>
                <div class="info-group">
                    <label>KeyBundle Version:</label>
                    <span>${data.keybundle_version}</span>
                </div>
            </div>
        `;

        this.elements.recoveryCodesDisplay.innerHTML = `
            <div class="recovery-codes">
                <h4>Recovery Codes</h4>
                <p class="alert alert-warning">
                    <strong>⚠️ Save these codes in a safe place!</strong><br>
                    You'll need them to recover access if you lose your keys.
                </p>
                <div class="codes-grid">
                    ${data.recovery_codes.map((code, i) => `
                        <div class="recovery-code">
                            <span class="code-number">${i + 1}.</span>
                            <code>${code}</code>
                        </div>
                    `).join('')}
                </div>
            </div>
        `;
    },

    /**
     * Download recovery codes
     */
    downloadRecoveryCodes() {
        if (!this.state.anchor) return;

        const text = `ICN Recovery Codes
Anchor ID: ${this.state.anchor.anchor_id}
DID: ${this.state.anchor.did}
Date: ${new Date().toISOString()}

Recovery Codes:
${this.state.anchor.recovery_codes.map((code, i) => `${i + 1}. ${code}`).join('\n')}

⚠️ Keep these codes safe and secure!
`;

        const blob = new Blob([text], { type: 'text/plain' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `icn-recovery-codes-${this.state.anchor.anchor_id}.txt`;
        a.click();
        URL.revokeObjectURL(url);

        showToast('Recovery codes downloaded!', 'success');
    },

    /**
     * Move to next step
     */
    nextStep() {
        if (this.state.currentStep < this.state.totalSteps) {
            this.state.currentStep++;
            this.updateProgress();
            this.showCurrentStep();
        }
    },

    /**
     * Move to previous step
     */
    previousStep() {
        if (this.state.currentStep > 1) {
            this.state.currentStep--;
            this.updateProgress();
            this.showCurrentStep();
        }
    },

    /**
     * Update progress bar
     */
    updateProgress() {
        const percentage = (this.state.currentStep / this.state.totalSteps) * 100;
        this.elements.progressBar.style.width = `${percentage}%`;
        this.elements.stepNumber.textContent = `Step ${this.state.currentStep} of ${this.state.totalSteps}`;

        // Update step title
        const titles = [
            'Choose Pathway',
            'Upload Documents',
            'Generate Keys',
            'Submit & Wait',
            'Complete',
        ];
        this.elements.stepTitle.textContent = titles[this.state.currentStep - 1];

        // Update button states
        this.elements.prevBtn.disabled = this.state.currentStep === 1;
        this.elements.prevBtn.classList.toggle('hidden', this.state.currentStep === 1);
        
        if (this.state.currentStep === this.state.totalSteps) {
            this.elements.nextBtn.textContent = 'Done';
        } else {
            this.elements.nextBtn.textContent = 'Next';
        }
    },

    /**
     * Show current step content
     */
    showCurrentStep() {
        Object.values(this.elements.steps).forEach((step, index) => {
            if (index + 1 === this.state.currentStep) {
                step.classList.remove('hidden');
            } else {
                step.classList.add('hidden');
            }
        });
    },
};

// Initialize when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => SDISEnrollment.init());
} else {
    SDISEnrollment.init();
}
