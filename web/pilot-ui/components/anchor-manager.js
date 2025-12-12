// Anchor Manager Component
// Manages SDIS recovery anchors (trusted devices/contacts)

export class AnchorManager {
  constructor(apiClient) {
    this.api = apiClient;
    this.anchors = [];
  }

  async render(container) {
    const html = `
      <div class="anchor-manager">
        <div class="anchor-header">
          <div>
            <h2>Recovery Anchors</h2>
            <p>Manage trusted devices and contacts for identity recovery</p>
          </div>
          <button class="btn-primary" id="add-anchor-btn">
            <span class="icon">➕</span> Add Anchor
          </button>
        </div>

        <div class="anchor-stats">
          <div class="stat-card">
            <div class="stat-value" id="anchor-count">0</div>
            <div class="stat-label">Total Anchors</div>
          </div>
          <div class="stat-card">
            <div class="stat-value" id="device-count">0</div>
            <div class="stat-label">Devices</div>
          </div>
          <div class="stat-card">
            <div class="stat-value" id="contact-count">0</div>
            <div class="stat-label">Contacts</div>
          </div>
        </div>

        <div class="anchor-filters">
          <button class="filter-btn active" data-filter="all">All</button>
          <button class="filter-btn" data-filter="device">Devices</button>
          <button class="filter-btn" data-filter="contact">Contacts</button>
          <button class="filter-btn" data-filter="active">Active</button>
          <button class="filter-btn" data-filter="revoked">Revoked</button>
        </div>

        <div class="anchor-list" id="anchor-list">
          <div class="loading">Loading anchors...</div>
        </div>
      </div>
    `;

    container.innerHTML = html;
    this.createModals();
    this.attachEventHandlers();
    await this.loadAnchors();
  }

  createModals() {
    const modalsHtml = `
      <div class="modal" id="add-anchor-modal" style="display: none;">
        <div class="modal-content">
          <div class="modal-header">
            <h3>Add Recovery Anchor</h3>
            <button class="close-btn" id="close-modal">&times;</button>
          </div>
          <div class="modal-body">
            <form id="add-anchor-form">
              <div class="form-group">
                <label>Anchor Type</label>
                <select id="anchor-type" required>
                  <option value="">Select type...</option>
                  <option value="device">Device</option>
                  <option value="contact">Trusted Contact</option>
                </select>
              </div>

              <div class="form-group">
                <label>Label</label>
                <input type="text" id="anchor-label" required 
                       placeholder="e.g., My Phone, Alice's Key">
              </div>

              <div class="form-group">
                <label>Public Key</label>
                <textarea id="anchor-pubkey" required rows="3"
                          placeholder="Paste the Ed25519 public key (base64 or hex)"></textarea>
              </div>

              <div class="form-group" id="contact-fields" style="display: none;">
                <label>Contact DID (optional)</label>
                <input type="text" id="contact-did" placeholder="did:icn:...">
              </div>

              <div class="form-actions">
                <button type="button" class="btn-secondary" id="cancel-add">Cancel</button>
                <button type="submit" class="btn-primary">Add Anchor</button>
              </div>
            </form>
          </div>
        </div>
      </div>

      <div class="modal" id="revoke-modal" style="display: none;">
        <div class="modal-content">
          <div class="modal-header">
            <h3>Revoke Anchor</h3>
            <button class="close-btn" id="close-revoke-modal">&times;</button>
          </div>
          <div class="modal-body">
            <p>Are you sure you want to revoke this recovery anchor?</p>
            <p class="warning-text">
              ⚠️ This will prevent this anchor from being used for identity recovery.
            </p>
            <div class="form-actions">
              <button type="button" class="btn-secondary" id="cancel-revoke">Cancel</button>
              <button type="button" class="btn-danger" id="confirm-revoke">Revoke Anchor</button>
            </div>
          </div>
        </div>
      </div>
    `;
    document.body.insertAdjacentHTML('beforeend', modalsHtml);
  }

  attachEventHandlers() {
    document.getElementById('add-anchor-btn').addEventListener('click', () => this.showAddModal());
    document.getElementById('close-modal').addEventListener('click', () => this.hideAddModal());
    document.getElementById('cancel-add').addEventListener('click', () => this.hideAddModal());
    document.getElementById('close-revoke-modal').addEventListener('click', () => this.hideRevokeModal());
    document.getElementById('cancel-revoke').addEventListener('click', () => this.hideRevokeModal());
    
    document.getElementById('anchor-type').addEventListener('change', (e) => {
      document.getElementById('contact-fields').style.display = 
        e.target.value === 'contact' ? 'block' : 'none';
    });

    document.getElementById('add-anchor-form').addEventListener('submit', async (e) => {
      e.preventDefault();
      await this.addAnchor();
    });

    document.getElementById('confirm-revoke').addEventListener('click', async () => {
      await this.revokeAnchor();
    });

    document.querySelectorAll('.filter-btn').forEach(btn => {
      btn.addEventListener('click', (e) => {
        document.querySelectorAll('.filter-btn').forEach(b => b.classList.remove('active'));
        e.target.classList.add('active');
        this.filterAnchors(e.target.dataset.filter);
      });
    });
  }

  async loadAnchors() {
    try {
      const response = await this.api.get('/api/v1/sdis/anchors');
      this.anchors = response.anchors || [];
      this.renderAnchors();
      this.updateStats();
    } catch (err) {
      console.error('Failed to load anchors:', err);
      document.getElementById('anchor-list').innerHTML = 
        `<div class="error-state">Failed to load anchors: ${err.message}</div>`;
    }
  }

  renderAnchors(filter = 'all') {
    const list = document.getElementById('anchor-list');
    let filtered = this.anchors;

    if (filter !== 'all') {
      if (filter === 'device' || filter === 'contact') {
        filtered = this.anchors.filter(a => a.anchor_type === filter);
      } else if (filter === 'active') {
        filtered = this.anchors.filter(a => !a.revoked_at);
      } else if (filter === 'revoked') {
        filtered = this.anchors.filter(a => a.revoked_at);
      }
    }

    if (filtered.length === 0) {
      list.innerHTML = '<div class="empty-state">No anchors found</div>';
      return;
    }

    list.innerHTML = filtered.map(a => this.renderAnchor(a)).join('');

    filtered.forEach(anchor => {
      const btn = document.getElementById(`revoke-${anchor.anchor_id}`);
      if (btn) {
        btn.addEventListener('click', () => {
          this.currentAnchor = anchor;
          this.showRevokeModal();
        });
      }
    });
  }

  renderAnchor(anchor) {
    const isRevoked = !!anchor.revoked_at;
    const icon = anchor.anchor_type === 'device' ? '📱' : '👤';
    const escapedLabel = this.escapeHtml(anchor.label);
    
    return `
      <div class="anchor-card ${isRevoked ? 'revoked' : ''}">
        <div class="anchor-icon">${icon}</div>
        <div class="anchor-info">
          <div class="anchor-name">${escapedLabel}</div>
          <div class="anchor-meta">
            <span class="anchor-type">${anchor.anchor_type}</span>
            ${anchor.contact_did ? `<span class="anchor-did">${this.truncateDid(anchor.contact_did)}</span>` : ''}
          </div>
          <div class="anchor-dates">
            <span>Added: ${this.formatDate(anchor.created_at)}</span>
            ${isRevoked ? `<span class="revoked">Revoked: ${this.formatDate(anchor.revoked_at)}</span>` : ''}
          </div>
          <div class="anchor-pubkey"><code>${this.truncateKey(anchor.pubkey)}</code></div>
        </div>
        <div class="anchor-actions">
          ${!isRevoked ? `<button class="btn-danger-small" id="revoke-${anchor.anchor_id}">Revoke</button>` : ''}
        </div>
      </div>
    `;
  }

  updateStats() {
    const active = this.anchors.filter(a => !a.revoked_at);
    const devices = active.filter(a => a.anchor_type === 'device').length;
    const contacts = active.filter(a => a.anchor_type === 'contact').length;
    
    document.getElementById('anchor-count').textContent = active.length;
    document.getElementById('device-count').textContent = devices;
    document.getElementById('contact-count').textContent = contacts;
  }

  async addAnchor() {
    const type = document.getElementById('anchor-type').value;
    const label = document.getElementById('anchor-label').value;
    const pubkey = document.getElementById('anchor-pubkey').value.trim();
    const contactDid = document.getElementById('contact-did').value.trim();

    try {
      const payload = { anchor_type: type, label, pubkey };
      if (type === 'contact' && contactDid) payload.contact_did = contactDid;

      await this.api.post('/api/v1/sdis/anchors', payload);
      this.hideAddModal();
      await this.loadAnchors();
      alert('Anchor added successfully');
    } catch (err) {
      alert('Failed to add anchor: ' + err.message);
    }
  }

  async revokeAnchor() {
    if (!this.currentAnchor) return;
    
    try {
      await this.api.post(`/api/v1/sdis/anchors/${this.currentAnchor.anchor_id}/revoke`);
      this.hideRevokeModal();
      await this.loadAnchors();
      alert('Anchor revoked successfully');
    } catch (err) {
      alert('Failed to revoke anchor: ' + err.message);
    }
  }

  showAddModal() {
    document.getElementById('add-anchor-form').reset();
    document.getElementById('contact-fields').style.display = 'none';
    document.getElementById('add-anchor-modal').style.display = 'flex';
  }

  hideAddModal() {
    document.getElementById('add-anchor-modal').style.display = 'none';
  }

  showRevokeModal() {
    document.getElementById('revoke-modal').style.display = 'flex';
  }

  hideRevokeModal() {
    document.getElementById('revoke-modal').style.display = 'none';
    this.currentAnchor = null;
  }

  filterAnchors(filter) {
    this.renderAnchors(filter);
  }

  formatDate(ts) {
    return new Date(ts).toLocaleDateString('en-US', {
      year: 'numeric', month: 'short', day: 'numeric',
      hour: '2-digit', minute: '2-digit'
    });
  }

  truncateDid(did) {
    return did.length <= 30 ? did : `${did.substring(0, 20)}...${did.slice(-8)}`;
  }

  truncateKey(key) {
    return key.length <= 40 ? key : `${key.substring(0, 20)}...${key.slice(-20)}`;
  }

  escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}
