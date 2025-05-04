import axios from 'axios';

const API_BASE_URL = '/api/runtime';

// Create axios instance with default config
const runtimeClient = axios.create({
  baseURL: API_BASE_URL,
  headers: {
    'Content-Type': 'application/json',
  },
});

// Add auth token interceptor
runtimeClient.interceptors.request.use(
  config => {
    const token = localStorage.getItem('auth_token');
    if (token) {
      config.headers.Authorization = `Bearer ${token}`;
    }
    return config;
  },
  error => Promise.reject(error)
);

// Proposal API
export const proposalApi = {
  /**
   * Get all proposals with optional filtering
   * @param {Object} filters - Filter options
   * @returns {Promise<Array>} List of proposals
   */
  async getProposals(filters = {}) {
    const response = await runtimeClient.get('/proposals', { params: filters });
    return response.data;
  },
  
  /**
   * Get a specific proposal by ID
   * @param {string} id - Proposal ID
   * @returns {Promise<Object>} Proposal data
   */
  async getProposal(id) {
    const response = await runtimeClient.get(`/proposals/${id}`);
    return response.data;
  },
  
  /**
   * Submit a new proposal
   * @param {Object} proposalData - Proposal data
   * @returns {Promise<Object>} Created proposal
   */
  async submitProposal(proposalData) {
    const response = await runtimeClient.post('/proposals', proposalData);
    return response.data;
  },
  
  /**
   * Execute a proposal
   * @param {string} id - Proposal ID
   * @returns {Promise<Object>} Execution result
   */
  async executeProposal(id) {
    const response = await runtimeClient.post(`/proposals/${id}/execute`);
    return response.data;
  },

  /**
   * Get voting configuration for a proposal
   * @param {string} id - Proposal ID
   * @returns {Promise<Object>} Voting configuration including quorum, threshold, etc.
   */
  async getVotingConfig(id) {
    const response = await runtimeClient.get(`/proposals/${id}/voting-config`);
    return response.data;
  },

  /**
   * Submit a vote for a proposal
   * @param {string} id - Proposal ID
   * @param {Object} voteData - Vote data including vote choice and signature
   * @returns {Promise<Object>} Vote submission result
   */
  async submitVote(id, voteData) {
    const response = await runtimeClient.post(`/proposals/${id}/votes`, voteData);
    return response.data;
  },

  /**
   * Get current votes for a proposal
   * @param {string} id - Proposal ID
   * @returns {Promise<Object>} Vote tallies and individual votes
   */
  async getVotes(id) {
    const response = await runtimeClient.get(`/proposals/${id}/votes`);
    return response.data;
  }
};

// DAG API
export const dagApi = {
  /**
   * Get a DAG node by ID
   * @param {string} id - Node ID/CID
   * @returns {Promise<Object>} DAG node
   */
  async getNode(id) {
    const response = await runtimeClient.get(`/dag/${id}`);
    return response.data;
  },
  
  /**
   * Submit a DAG node
   * @param {Object} nodeData - Node data
   * @returns {Promise<Object>} Submission result
   */
  async submitNode(nodeData) {
    const response = await runtimeClient.post('/dag', nodeData);
    return response.data;
  },

  /**
   * Get latest DAG anchors
   * @param {string} since - Optional CID to get anchors since a specific point
   * @returns {Promise<Object>} Latest DAG anchors and updates
   */
  async getAnchors(since = null) {
    const params = since ? { since } : {};
    const response = await runtimeClient.get('/dag/anchors', { params });
    return response.data;
  },

  /**
   * Get DAG history for a specific proposal
   * @param {string} proposalId - Proposal ID
   * @returns {Promise<Array>} DAG history related to the proposal
   */
  async getProposalDagHistory(proposalId) {
    const response = await runtimeClient.get(`/dag/history/${proposalId}`);
    return response.data;
  }
};

// Credential API
export const credentialApi = {
  /**
   * Get a receipt by ID
   * @param {string} id - Receipt ID
   * @returns {Promise<Object>} Credential data
   */
  async getReceipt(id) {
    const response = await runtimeClient.get(`/receipts/${id}`);
    return response.data;
  },
  
  /**
   * Get receipts for a proposal
   * @param {string} proposalId - Proposal ID
   * @returns {Promise<Array>} List of receipts
   */
  async getReceiptsForProposal(proposalId) {
    const response = await runtimeClient.get(`/receipts`, {
      params: { proposalId }
    });
    return response.data;
  },
  
  /**
   * Verify a receipt
   * @param {string} receiptJwt - The receipt JWT
   * @returns {Promise<Object>} Verification result
   */
  async verifyReceipt(receiptJwt) {
    const response = await runtimeClient.post('/receipts/verify', {
      receipt: receiptJwt
    });
    return response.data;
  },

  /**
   * Poll for a receipt until it becomes available
   * @param {string} proposalId - Proposal ID
   * @param {number} timeout - Timeout in milliseconds
   * @param {number} interval - Polling interval in milliseconds
   * @returns {Promise<Object>} Receipt when available
   */
  async pollForReceipt(proposalId, timeout = 30000, interval = 2000) {
    const startTime = Date.now();
    
    while (Date.now() - startTime < timeout) {
      try {
        const receipts = await this.getReceiptsForProposal(proposalId);
        if (receipts && receipts.length > 0) {
          return receipts[0];
        }
      } catch (error) {
        console.error('Error polling for receipt:', error);
      }
      
      // Wait for the interval before trying again
      await new Promise(resolve => setTimeout(resolve, interval));
    }
    
    throw new Error('Receipt polling timed out');
  }
};

export default {
  proposal: proposalApi,
  dag: dagApi,
  credential: credentialApi
}; 