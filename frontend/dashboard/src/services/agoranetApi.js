import axios from 'axios';

const API_BASE_URL = '/api/agoranet';

// Create axios instance with default config
const agoranetClient = axios.create({
  baseURL: API_BASE_URL,
  headers: {
    'Content-Type': 'application/json',
  },
});

// Add auth token interceptor
agoranetClient.interceptors.request.use(
  config => {
    const token = localStorage.getItem('auth_token');
    if (token) {
      config.headers.Authorization = `Bearer ${token}`;
    }
    return config;
  },
  error => Promise.reject(error)
);

// Thread API
export const threadApi = {
  /**
   * Get all threads with optional filtering
   * @param {Object} filters - Filter options
   * @returns {Promise<Array>} List of threads
   */
  async getThreads(filters = {}) {
    const response = await agoranetClient.get('/threads', { params: filters });
    return response.data;
  },
  
  /**
   * Get a specific thread by ID
   * @param {string} id - Thread ID
   * @returns {Promise<Object>} Thread data
   */
  async getThread(id) {
    const response = await agoranetClient.get(`/threads/${id}`);
    return response.data;
  },
  
  /**
   * Create a new thread
   * @param {Object} threadData - Thread data to create
   * @returns {Promise<Object>} Created thread
   */
  async createThread(threadData) {
    const response = await agoranetClient.post('/threads', threadData);
    return response.data;
  },
  
  /**
   * Link a proposal to a thread
   * @param {string} threadId - Thread ID
   * @param {string} proposalCid - Proposal CID
   * @returns {Promise<void>}
   */
  async linkProposal(threadId, proposalCid) {
    await agoranetClient.post(`/threads/${threadId}/link_proposal`, {
      proposal_cid: proposalCid
    });
  }
};

// Message API
export const messageApi = {
  /**
   * Get all messages for a thread
   * @param {string} threadId - Thread ID
   * @returns {Promise<Array>} List of messages
   */
  async getMessages(threadId) {
    const response = await agoranetClient.get(`/threads/${threadId}/messages`);
    return response.data;
  },
  
  /**
   * Create a new message in a thread
   * @param {string} threadId - Thread ID
   * @param {Object} messageData - Message data
   * @returns {Promise<Object>} Created message
   */
  async createMessage(threadId, messageData) {
    const response = await agoranetClient.post(`/threads/${threadId}/messages`, messageData);
    return response.data;
  }
};

export default {
  thread: threadApi,
  message: messageApi
}; 