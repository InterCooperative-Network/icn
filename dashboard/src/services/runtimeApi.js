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
  }
};

export default {
  proposal: proposalApi,
  dag: dagApi,
  credential: credentialApi
}; 