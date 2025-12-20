import axios, { AxiosInstance } from 'axios';

export interface Balance {
  balance: number;
  credit_limit: number;
}

export interface Transaction {
  id: string;
  from_did: string;
  to_did: string;
  amount: number;
  description: string;
  timestamp: number;
}

export interface Proposal {
  id: string;
  domain_id: string;
  title: string;
  description: string;
  proposal_type: string;
  status: string;
  votes_for: number;
  votes_against: number;
  votes_abstain: number;
  created_at: number;
  voting_ends_at: number;
}

export interface Cooperative {
  id: string;
  name: string;
  coop_type: string;
  status: string;
  member_count: number;
  created_at: number;
}

export class ICNClient {
  private api: AxiosInstance;
  private token: string;

  constructor(baseURL: string, token: string) {
    this.token = token;
    this.api = axios.create({
      baseURL,
      headers: {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/json',
      },
    });
  }

  // Authentication
  async requestChallenge(did: string): Promise<{ challenge: string; expires_at: number }> {
    const response = await this.api.post('/v1/auth/challenge', { did });
    return response.data;
  }

  async verifyChallenge(did: string, challenge: string, signature: string): Promise<{ token: string; expires_at: number }> {
    const response = await this.api.post('/v1/auth/verify', { did, challenge, signature });
    return response.data;
  }

  // Ledger
  async getBalance(): Promise<Balance> {
    const response = await this.api.get('/v1/ledger/balance');
    return response.data;
  }

  async createPayment(to_did: string, amount: number, description: string): Promise<Transaction> {
    const response = await this.api.post('/v1/ledger/payment', {
      to_did,
      amount,
      description,
    });
    return response.data;
  }

  async getHistory(limit: number = 50, offset: number = 0): Promise<{ transactions: Transaction[]; total: number }> {
    const response = await this.api.get('/v1/ledger/history', {
      params: { limit, offset },
    });
    return response.data;
  }

  // Governance
  async listProposals(domain_id: string, status?: string): Promise<Proposal[]> {
    const response = await this.api.get('/v1/governance/proposals', {
      params: { domain_id, status },
    });
    return response.data;
  }

  async getProposal(proposal_id: string): Promise<Proposal> {
    const response = await this.api.get(`/v1/governance/proposals/${proposal_id}`);
    return response.data;
  }

  async createProposal(
    domain_id: string,
    title: string,
    description: string,
    proposal_type: string,
    voting_period_days: number = 7
  ): Promise<Proposal> {
    const response = await this.api.post('/v1/governance/proposals', {
      domain_id,
      title,
      description,
      proposal_type,
      voting_period_days,
    });
    return response.data;
  }

  async vote(proposal_id: string, vote: 'for' | 'against' | 'abstain'): Promise<void> {
    await this.api.post(`/v1/governance/proposals/${proposal_id}/vote`, { vote });
  }

  // Cooperatives
  async listCooperatives(): Promise<Cooperative[]> {
    const response = await this.api.get('/v1/coops');
    return response.data;
  }

  async getCooperative(coop_id: string): Promise<Cooperative> {
    const response = await this.api.get(`/v1/coops/${coop_id}`);
    return response.data;
  }

  async createCooperative(
    name: string,
    coop_type: string,
    founding_members: string[]
  ): Promise<Cooperative> {
    const response = await this.api.post('/v1/coops', {
      name,
      coop_type,
      founding_members,
    });
    return response.data;
  }

  // Budget
  async getBudgets(): Promise<any[]> {
    const response = await this.api.get('/v1/budgets');
    return response.data;
  }

  async createBudget(name: string, total_amount: number, period: string): Promise<any> {
    const response = await this.api.post('/v1/budgets', {
      name,
      total_amount,
      period,
    });
    return response.data;
  }

  // Recurring Payments
  async getRecurringPayments(): Promise<any[]> {
    const response = await this.api.get('/v1/recurring-payments');
    return response.data;
  }

  async createRecurringPayment(
    to_did: string,
    amount: number,
    frequency: string,
    description: string
  ): Promise<any> {
    const response = await this.api.post('/v1/recurring-payments', {
      to_did,
      amount,
      frequency,
      description,
    });
    return response.data;
  }

  // Notifications
  async getNotifications(): Promise<any[]> {
    const response = await this.api.get('/v1/notifications');
    return response.data;
  }

  async markNotificationRead(notification_id: string): Promise<void> {
    await this.api.post(`/v1/notifications/${notification_id}/read`);
  }
}
