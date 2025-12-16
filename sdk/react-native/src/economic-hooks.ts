/**
 * Economic Feature Hooks for ICN React Native SDK
 *
 * React hooks for recurring payments, escrow, and budget management.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { ICNMobileClient } from './client';

// ============================================================================
// Recurring Payments Types
// ============================================================================

/**
 * Payment frequency
 */
export type PaymentFrequency = 'daily' | 'weekly' | 'monthly' | 'yearly';

/**
 * Recurring payment status
 */
export type RecurringStatus = 'active' | 'paused' | 'cancelled' | 'completed';

/**
 * Recurring payment schedule
 */
export interface RecurringPayment {
  /** Unique ID */
  id: string;
  /** Owner/creator DID */
  owner: string;
  /** From account */
  from_account: string;
  /** To account */
  to_account: string;
  /** Amount per payment */
  amount: number;
  /** Currency */
  currency: string;
  /** Payment frequency */
  frequency: PaymentFrequency;
  /** Next execution timestamp (Unix seconds) */
  next_execution: number;
  /** Optional end date (Unix seconds) */
  end_date?: number;
  /** Current status */
  status: RecurringStatus;
  /** Total executions */
  execution_count: number;
  /** Last execution timestamp */
  last_execution?: number;
  /** Created timestamp */
  created_at: number;
  /** Updated timestamp */
  updated_at: number;
}

/**
 * Create recurring payment request
 */
export interface CreateRecurringPaymentRequest {
  /** From account DID */
  from_account: string;
  /** To account DID */
  to_account: string;
  /** Amount per payment */
  amount: number;
  /** Currency */
  currency: string;
  /** Payment frequency */
  frequency: PaymentFrequency;
  /** First execution timestamp (Unix seconds) */
  start_date: number;
  /** Optional end date (Unix seconds) */
  end_date?: number;
  /** Description */
  description?: string;
}

// ============================================================================
// Escrow Types
// ============================================================================

/**
 * Escrow status
 */
export type EscrowStatus = 'pending' | 'locked' | 'released' | 'refunded' | 'expired';

/**
 * Escrow condition type
 */
export type EscrowCondition =
  | { requires_approval: { did: string } }
  | { time_release: { timestamp: number } }
  | { proof_required: { proof_type: string } };

/**
 * Escrow record
 */
export interface Escrow {
  /** Unique ID */
  id: string;
  /** Cooperative ID (ledger namespace) */
  coop_id: string;
  /** Creator/initiator DID */
  creator: string;
  /** From account */
  from_account: string;
  /** To account (beneficiary) */
  to_account: string;
  /** Amount held */
  amount: number;
  /** Currency */
  currency: string;
  /** Current status */
  status: EscrowStatus;
  /** Release conditions */
  conditions: EscrowCondition[];
  /** Approvals received */
  approvals: string[];
  /** Expiration timestamp */
  expires_at?: number;
  /** Description/purpose */
  description: string;
  /** Created timestamp */
  created_at: number;
  /** Updated timestamp */
  updated_at: number;
}

/**
 * Create escrow request
 */
export interface CreateEscrowRequest {
  /** Cooperative ID (ledger namespace) */
  coop_id: string;
  /** From account DID */
  from_account: string;
  /** To account DID */
  to_account: string;
  /** Amount to hold */
  amount: number;
  /** Currency */
  currency: string;
  /** Release conditions */
  conditions: EscrowCondition[];
  /** Expiration timestamp (Unix seconds) */
  expires_at?: number;
  /** Description/purpose */
  description: string;
}

// ============================================================================
// Budget Types
// ============================================================================

/**
 * Budget period
 */
export type BudgetPeriod = 'daily' | 'weekly' | 'monthly' | 'yearly';

/**
 * Budget status
 */
export type BudgetStatus = 'active' | 'paused' | 'exceeded' | 'expired';

/**
 * Budget limit
 */
export interface Budget {
  /** Unique ID */
  id: string;
  /** Owner/creator DID */
  owner: string;
  /** Account this budget applies to */
  account: string;
  /** Currency */
  currency: string;
  /** Spending limit */
  limit: number;
  /** Current spending in this period */
  spent: number;
  /** Budget period */
  period: BudgetPeriod;
  /** Period start timestamp */
  period_start: number;
  /** Period end timestamp */
  period_end: number;
  /** Current status */
  status: BudgetStatus;
  /** Notification thresholds (percentages) */
  notification_thresholds: number[];
  /** Thresholds that have been notified */
  notified_thresholds: number[];
  /** Description */
  description: string;
  /** Created timestamp */
  created_at: number;
  /** Updated timestamp */
  updated_at: number;
}

/**
 * Create budget request
 */
export interface CreateBudgetRequest {
  /** Account DID */
  account: string;
  /** Currency */
  currency: string;
  /** Spending limit */
  limit: number;
  /** Budget period */
  period: BudgetPeriod;
  /** Notification thresholds (percentages, e.g., [80, 100]) */
  notification_thresholds?: number[];
  /** Description */
  description?: string;
}

// ============================================================================
// API Functions
// ============================================================================

/**
 * Create economic API methods
 */
export function createEconomicAPI(client: ICNMobileClient) {
  const baseUrl = (client as unknown as { baseUrl: string }).baseUrl;

  const getAuthHeaders = () => {
    const token = (client as unknown as { authToken?: string }).authToken;
    if (!token) {
      throw new Error('Not authenticated');
    }
    return {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
    };
  };

  return {
    // =========================================================================
    // Recurring Payments
    // =========================================================================

    recurringPayments: {
      async list(): Promise<RecurringPayment[]> {
        const response = await fetch(`${baseUrl}/v1/recurring-payments`, {
          method: 'GET',
          headers: getAuthHeaders(),
        });
        if (!response.ok) {
          throw new Error(`Failed to list recurring payments: ${await response.text()}`);
        }
        const result = await response.json();
        return result.payments || result;
      },

      async get(id: string): Promise<RecurringPayment> {
        const response = await fetch(
          `${baseUrl}/v1/recurring-payments/${encodeURIComponent(id)}`,
          {
            method: 'GET',
            headers: getAuthHeaders(),
          }
        );
        if (!response.ok) {
          throw new Error(`Failed to get recurring payment: ${await response.text()}`);
        }
        return response.json();
      },

      async create(request: CreateRecurringPaymentRequest): Promise<RecurringPayment> {
        const response = await fetch(`${baseUrl}/v1/recurring-payments`, {
          method: 'POST',
          headers: getAuthHeaders(),
          body: JSON.stringify(request),
        });
        if (!response.ok) {
          throw new Error(`Failed to create recurring payment: ${await response.text()}`);
        }
        return response.json();
      },

      async pause(id: string): Promise<void> {
        const response = await fetch(
          `${baseUrl}/v1/recurring-payments/${encodeURIComponent(id)}/pause`,
          {
            method: 'POST',
            headers: getAuthHeaders(),
          }
        );
        if (!response.ok) {
          throw new Error(`Failed to pause recurring payment: ${await response.text()}`);
        }
      },

      async resume(id: string): Promise<void> {
        const response = await fetch(
          `${baseUrl}/v1/recurring-payments/${encodeURIComponent(id)}/resume`,
          {
            method: 'POST',
            headers: getAuthHeaders(),
          }
        );
        if (!response.ok) {
          throw new Error(`Failed to resume recurring payment: ${await response.text()}`);
        }
      },

      async cancel(id: string): Promise<void> {
        const response = await fetch(
          `${baseUrl}/v1/recurring-payments/${encodeURIComponent(id)}`,
          {
            method: 'DELETE',
            headers: getAuthHeaders(),
          }
        );
        if (!response.ok) {
          throw new Error(`Failed to cancel recurring payment: ${await response.text()}`);
        }
      },
    },

    // =========================================================================
    // Escrow
    // =========================================================================

    escrow: {
      async list(): Promise<Escrow[]> {
        const response = await fetch(`${baseUrl}/v1/escrow`, {
          method: 'GET',
          headers: getAuthHeaders(),
        });
        if (!response.ok) {
          throw new Error(`Failed to list escrows: ${await response.text()}`);
        }
        const result = await response.json();
        return result.escrows || result;
      },

      async get(id: string): Promise<Escrow> {
        const response = await fetch(`${baseUrl}/v1/escrow/${encodeURIComponent(id)}`, {
          method: 'GET',
          headers: getAuthHeaders(),
        });
        if (!response.ok) {
          throw new Error(`Failed to get escrow: ${await response.text()}`);
        }
        return response.json();
      },

      async create(request: CreateEscrowRequest): Promise<Escrow> {
        const response = await fetch(`${baseUrl}/v1/escrow`, {
          method: 'POST',
          headers: getAuthHeaders(),
          body: JSON.stringify(request),
        });
        if (!response.ok) {
          throw new Error(`Failed to create escrow: ${await response.text()}`);
        }
        return response.json();
      },

      async approve(id: string): Promise<void> {
        const response = await fetch(
          `${baseUrl}/v1/escrow/${encodeURIComponent(id)}/approve`,
          {
            method: 'POST',
            headers: getAuthHeaders(),
          }
        );
        if (!response.ok) {
          throw new Error(`Failed to approve escrow: ${await response.text()}`);
        }
      },

      async release(id: string): Promise<void> {
        const response = await fetch(
          `${baseUrl}/v1/escrow/${encodeURIComponent(id)}/release`,
          {
            method: 'POST',
            headers: getAuthHeaders(),
          }
        );
        if (!response.ok) {
          throw new Error(`Failed to release escrow: ${await response.text()}`);
        }
      },

      async refund(id: string): Promise<void> {
        const response = await fetch(
          `${baseUrl}/v1/escrow/${encodeURIComponent(id)}/refund`,
          {
            method: 'POST',
            headers: getAuthHeaders(),
          }
        );
        if (!response.ok) {
          throw new Error(`Failed to refund escrow: ${await response.text()}`);
        }
      },
    },

    // =========================================================================
    // Budgets
    // =========================================================================

    budgets: {
      async list(): Promise<Budget[]> {
        const response = await fetch(`${baseUrl}/v1/budgets`, {
          method: 'GET',
          headers: getAuthHeaders(),
        });
        if (!response.ok) {
          throw new Error(`Failed to list budgets: ${await response.text()}`);
        }
        const result = await response.json();
        return result.budgets || result;
      },

      async get(id: string): Promise<Budget> {
        const response = await fetch(`${baseUrl}/v1/budgets/${encodeURIComponent(id)}`, {
          method: 'GET',
          headers: getAuthHeaders(),
        });
        if (!response.ok) {
          throw new Error(`Failed to get budget: ${await response.text()}`);
        }
        return response.json();
      },

      async create(request: CreateBudgetRequest): Promise<Budget> {
        const response = await fetch(`${baseUrl}/v1/budgets`, {
          method: 'POST',
          headers: getAuthHeaders(),
          body: JSON.stringify(request),
        });
        if (!response.ok) {
          throw new Error(`Failed to create budget: ${await response.text()}`);
        }
        return response.json();
      },

      async update(
        id: string,
        updates: Partial<Pick<Budget, 'limit' | 'notification_thresholds' | 'description'>>
      ): Promise<Budget> {
        const response = await fetch(`${baseUrl}/v1/budgets/${encodeURIComponent(id)}`, {
          method: 'PUT',
          headers: getAuthHeaders(),
          body: JSON.stringify(updates),
        });
        if (!response.ok) {
          throw new Error(`Failed to update budget: ${await response.text()}`);
        }
        return response.json();
      },

      async delete(id: string): Promise<void> {
        const response = await fetch(`${baseUrl}/v1/budgets/${encodeURIComponent(id)}`, {
          method: 'DELETE',
          headers: getAuthHeaders(),
        });
        if (!response.ok) {
          throw new Error(`Failed to delete budget: ${await response.text()}`);
        }
      },
    },
  };
}

// ============================================================================
// Recurring Payments Hooks
// ============================================================================

/**
 * Hook for managing recurring payments
 *
 * @example
 * ```tsx
 * function RecurringPaymentsScreen() {
 *   const {
 *     payments,
 *     isLoading,
 *     create,
 *     pause,
 *     resume,
 *     cancel,
 *   } = useRecurringPayments(client);
 *
 *   return (
 *     <FlatList
 *       data={payments}
 *       renderItem={({ item }) => (
 *         <View>
 *           <Text>{item.amount} {item.currency} to {item.to_account}</Text>
 *           <Text>{item.frequency} - {item.status}</Text>
 *           {item.status === 'active' && (
 *             <Button title="Pause" onPress={() => pause(item.id)} />
 *           )}
 *         </View>
 *       )}
 *     />
 *   );
 * }
 * ```
 */
export function useRecurringPayments(client: ICNMobileClient) {
  const [payments, setPayments] = useState<RecurringPayment[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const api = createEconomicAPI(client);

  const refresh = useCallback(async () => {
    if (!client.authState.isAuthenticated) return;

    setIsLoading(true);
    setError(null);
    try {
      const data = await api.recurringPayments.list();
      setPayments(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client.authState.isAuthenticated, api]);

  useEffect(() => {
    if (client.authState.isAuthenticated) {
      refresh();
    }
  }, [client.authState.isAuthenticated, refresh]);

  const create = useCallback(
    async (request: CreateRecurringPaymentRequest) => {
      const payment = await api.recurringPayments.create(request);
      setPayments((prev) => [...prev, payment]);
      return payment;
    },
    [api]
  );

  const pause = useCallback(
    async (id: string) => {
      await api.recurringPayments.pause(id);
      setPayments((prev) =>
        prev.map((p) => (p.id === id ? { ...p, status: 'paused' as const } : p))
      );
    },
    [api]
  );

  const resume = useCallback(
    async (id: string) => {
      await api.recurringPayments.resume(id);
      setPayments((prev) =>
        prev.map((p) => (p.id === id ? { ...p, status: 'active' as const } : p))
      );
    },
    [api]
  );

  const cancel = useCallback(
    async (id: string) => {
      await api.recurringPayments.cancel(id);
      setPayments((prev) =>
        prev.map((p) => (p.id === id ? { ...p, status: 'cancelled' as const } : p))
      );
    },
    [api]
  );

  return {
    payments,
    isLoading,
    error,
    refresh,
    create,
    pause,
    resume,
    cancel,
  };
}

// ============================================================================
// Escrow Hooks
// ============================================================================

/**
 * Hook for managing escrow accounts
 *
 * @example
 * ```tsx
 * function EscrowScreen() {
 *   const {
 *     escrows,
 *     isLoading,
 *     create,
 *     approve,
 *     release,
 *     refund,
 *   } = useEscrow(client);
 *
 *   return (
 *     <FlatList
 *       data={escrows}
 *       renderItem={({ item }) => (
 *         <View>
 *           <Text>{item.amount} {item.currency}</Text>
 *           <Text>Status: {item.status}</Text>
 *           {item.status === 'locked' && (
 *             <Button title="Release" onPress={() => release(item.id)} />
 *           )}
 *         </View>
 *       )}
 *     />
 *   );
 * }
 * ```
 */
export function useEscrow(client: ICNMobileClient) {
  const [escrows, setEscrows] = useState<Escrow[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const api = createEconomicAPI(client);

  const refresh = useCallback(async () => {
    if (!client.authState.isAuthenticated) return;

    setIsLoading(true);
    setError(null);
    try {
      const data = await api.escrow.list();
      setEscrows(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client.authState.isAuthenticated, api]);

  useEffect(() => {
    if (client.authState.isAuthenticated) {
      refresh();
    }
  }, [client.authState.isAuthenticated, refresh]);

  const create = useCallback(
    async (request: CreateEscrowRequest) => {
      const escrow = await api.escrow.create(request);
      setEscrows((prev) => [...prev, escrow]);
      return escrow;
    },
    [api]
  );

  const approve = useCallback(
    async (id: string) => {
      await api.escrow.approve(id);
      await refresh(); // Refresh to get updated approvals list
    },
    [api, refresh]
  );

  const release = useCallback(
    async (id: string) => {
      await api.escrow.release(id);
      setEscrows((prev) =>
        prev.map((e) => (e.id === id ? { ...e, status: 'released' as const } : e))
      );
    },
    [api]
  );

  const refund = useCallback(
    async (id: string) => {
      await api.escrow.refund(id);
      setEscrows((prev) =>
        prev.map((e) => (e.id === id ? { ...e, status: 'refunded' as const } : e))
      );
    },
    [api]
  );

  return {
    escrows,
    isLoading,
    error,
    refresh,
    create,
    approve,
    release,
    refund,
  };
}

// ============================================================================
// Budget Hooks
// ============================================================================

/**
 * Hook for managing budgets
 *
 * @example
 * ```tsx
 * function BudgetScreen() {
 *   const {
 *     budgets,
 *     isLoading,
 *     create,
 *     update,
 *     deleteBudget,
 *   } = useBudgets(client);
 *
 *   return (
 *     <FlatList
 *       data={budgets}
 *       renderItem={({ item }) => (
 *         <View>
 *           <Text>{item.currency}: {item.spent} / {item.limit}</Text>
 *           <Text>Remaining: {item.limit - item.spent}</Text>
 *           <ProgressBar progress={item.spent / item.limit} />
 *           {item.status === 'exceeded' && (
 *             <Text style={{ color: 'red' }}>Budget exceeded!</Text>
 *           )}
 *         </View>
 *       )}
 *     />
 *   );
 * }
 * ```
 */
export function useBudgets(client: ICNMobileClient) {
  const [budgets, setBudgets] = useState<Budget[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const api = createEconomicAPI(client);

  const refresh = useCallback(async () => {
    if (!client.authState.isAuthenticated) return;

    setIsLoading(true);
    setError(null);
    try {
      const data = await api.budgets.list();
      setBudgets(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client.authState.isAuthenticated, api]);

  useEffect(() => {
    if (client.authState.isAuthenticated) {
      refresh();
    }
  }, [client.authState.isAuthenticated, refresh]);

  const create = useCallback(
    async (request: CreateBudgetRequest) => {
      const budget = await api.budgets.create(request);
      setBudgets((prev) => [...prev, budget]);
      return budget;
    },
    [api]
  );

  const update = useCallback(
    async (
      id: string,
      updates: Partial<Pick<Budget, 'limit' | 'notification_thresholds' | 'description'>>
    ) => {
      const updated = await api.budgets.update(id, updates);
      setBudgets((prev) => prev.map((b) => (b.id === id ? updated : b)));
      return updated;
    },
    [api]
  );

  const deleteBudget = useCallback(
    async (id: string) => {
      await api.budgets.delete(id);
      setBudgets((prev) => prev.filter((b) => b.id !== id));
    },
    [api]
  );

  /**
   * Get budget utilization percentage
   */
  const getUtilization = useCallback((budget: Budget): number => {
    if (budget.limit === 0) return 0;
    return Math.min(100, (budget.spent / budget.limit) * 100);
  }, []);

  /**
   * Get remaining budget amount
   */
  const getRemaining = useCallback((budget: Budget): number => {
    return Math.max(0, budget.limit - budget.spent);
  }, []);

  return {
    budgets,
    isLoading,
    error,
    refresh,
    create,
    update,
    deleteBudget,
    getUtilization,
    getRemaining,
  };
}

/**
 * Hook for a single budget (useful for budget detail screens)
 */
export function useBudget(client: ICNMobileClient, budgetId: string | null) {
  const [budget, setBudget] = useState<Budget | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const api = createEconomicAPI(client);

  const refresh = useCallback(async () => {
    if (!budgetId || !client.authState.isAuthenticated) {
      setBudget(null);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const data = await api.budgets.get(budgetId);
      setBudget(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [budgetId, client.authState.isAuthenticated, api]);

  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;

  useEffect(() => {
    refreshRef.current();
  }, [budgetId, client.authState.isAuthenticated]);

  return {
    budget,
    isLoading,
    error,
    refresh,
    utilization: budget ? Math.min(100, (budget.spent / budget.limit) * 100) : 0,
    remaining: budget ? Math.max(0, budget.limit - budget.spent) : 0,
  };
}
