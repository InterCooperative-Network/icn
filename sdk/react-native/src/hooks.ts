/**
 * React Hooks for ICN Mobile SDK
 *
 * Easy-to-use hooks for common ICN operations.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { ICNMobileClient } from './client';
import {
  AuthState,
  WebSocketState,
  Balance,
  WsMessage,
  Cooperative,
  Member,
  Proposal,
  GovernanceDomain,
  Transaction,
} from './types';

/**
 * Hook for managing authentication state
 *
 * @example
 * ```tsx
 * function LoginScreen() {
 *   const { isAuthenticated, did, login, logout, isLoading, error } = useAuth(client);
 *
 *   if (isLoading) return <LoadingSpinner />;
 *
 *   if (isAuthenticated) {
 *     return (
 *       <View>
 *         <Text>Logged in as {did}</Text>
 *         <Button onPress={logout} title="Logout" />
 *       </View>
 *     );
 *   }
 *
 *   return (
 *     <View>
 *       <Button onPress={() => login('my-coop')} title="Login" />
 *       {error && <Text style={{color: 'red'}}>{error}</Text>}
 *     </View>
 *   );
 * }
 * ```
 */
export function useAuth(client: ICNMobileClient) {
  const [authState, setAuthState] = useState<AuthState>(client.authState);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    return client.onAuthStateChange(setAuthState);
  }, [client]);

  const login = useCallback(
    async (coopId?: string, scopes?: string[]) => {
      setIsLoading(true);
      setError(null);
      try {
        await client.login(coopId, scopes);
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsLoading(false);
      }
    },
    [client]
  );

  const logout = useCallback(async () => {
    setIsLoading(true);
    try {
      await client.logout();
    } finally {
      setIsLoading(false);
    }
  }, [client]);

  return {
    ...authState,
    isLoading,
    error,
    login,
    logout,
  };
}

/**
 * Hook for managing WebSocket connection
 *
 * @example
 * ```tsx
 * function RealtimeIndicator() {
 *   const { state, connect, disconnect } = useRealtime(client);
 *
 *   return (
 *     <View>
 *       <Text>Connection: {state}</Text>
 *       {state === 'disconnected' && (
 *         <Button onPress={connect} title="Connect" />
 *       )}
 *     </View>
 *   );
 * }
 * ```
 */
export function useRealtime(client: ICNMobileClient, autoConnect = true) {
  const [state, setState] = useState<WebSocketState>(client.connectionState);

  useEffect(() => {
    return client.onConnectionStateChange(setState);
  }, [client]);

  useEffect(() => {
    if (autoConnect && client.authState.isAuthenticated && client.authState.coopId) {
      client.connectRealtime();
    }
    return () => {
      if (autoConnect) {
        client.disconnectRealtime();
      }
    };
  }, [client, autoConnect, client.authState.isAuthenticated, client.authState.coopId]);

  const connect = useCallback(
    (coopId?: string) => {
      client.connectRealtime(coopId);
    },
    [client]
  );

  const disconnect = useCallback(() => {
    client.disconnectRealtime();
  }, [client]);

  return {
    state,
    isConnected: state === 'connected',
    connect,
    disconnect,
  };
}

/**
 * Hook for subscribing to real-time events
 *
 * @example
 * ```tsx
 * function PaymentNotifications() {
 *   useEvent(client, 'PaymentCreated', (event) => {
 *     showNotification(`Received ${event.amount} from ${event.from}`);
 *   });
 *
 *   return null;
 * }
 * ```
 */
export function useEvent(
  client: ICNMobileClient,
  eventType: string,
  handler: (message: WsMessage) => void
) {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    return client.onEvent(eventType, (message) => {
      handlerRef.current(message);
    });
  }, [client, eventType]);
}

/**
 * Hook for fetching and caching balance
 *
 * @example
 * ```tsx
 * function BalanceDisplay() {
 *   const { balance, isLoading, error, refresh } = useBalance(client, 'my-coop', myDid);
 *
 *   if (isLoading) return <ActivityIndicator />;
 *   if (error) return <Text>Error: {error}</Text>;
 *
 *   return (
 *     <View>
 *       <Text>Balance: {balance?.balance} hours</Text>
 *       <Button onPress={refresh} title="Refresh" />
 *     </View>
 *   );
 * }
 * ```
 */
export function useBalance(client: ICNMobileClient, coopId: string, did: string) {
  const [balance, setBalance] = useState<Balance | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchBalance = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await client.getBalance(coopId, did);
      setBalance(result);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, coopId, did]);

  // Use ref to avoid dependency cycles
  const fetchRef = useRef(fetchBalance);
  fetchRef.current = fetchBalance;

  useEffect(() => {
    fetchRef.current();
  }, [client, coopId, did]);

  // Auto-refresh on payment events
  useEvent(client, 'PaymentCreated', () => {
    fetchRef.current();
  });

  return {
    balance,
    isLoading,
    error,
    refresh: fetchBalance,
  };
}

/**
 * Hook for cooperative data
 *
 * @example
 * ```tsx
 * function CoopInfo() {
 *   const { coop, members, isLoading, error } = useCoop(client, 'my-coop');
 *
 *   if (isLoading) return <ActivityIndicator />;
 *
 *   return (
 *     <View>
 *       <Text>{coop?.name}</Text>
 *       <Text>{members.length} members</Text>
 *     </View>
 *   );
 * }
 * ```
 */
export function useCoop(client: ICNMobileClient, coopId: string) {
  const [coop, setCoop] = useState<Cooperative | null>(null);
  const [members, setMembers] = useState<Member[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchCoop = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [coopData, memberData] = await Promise.all([
        client.getCoop(coopId),
        client.listMembers(coopId),
      ]);
      setCoop(coopData);
      setMembers(memberData);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, coopId]);

  // Use ref to avoid dependency cycles
  const fetchRef = useRef(fetchCoop);
  fetchRef.current = fetchCoop;

  useEffect(() => {
    fetchRef.current();
  }, [client, coopId]);

  return {
    coop,
    members,
    isLoading,
    error,
    refresh: fetchCoop,
  };
}

/**
 * Hook for governance proposals
 *
 * @example
 * ```tsx
 * function ProposalList() {
 *   const { proposals, isLoading, vote } = useProposals(client, 'coop:my-coop');
 *
 *   return (
 *     <FlatList
 *       data={proposals}
 *       renderItem={({ item }) => (
 *         <ProposalCard
 *           proposal={item}
 *           onVote={(choice) => vote(item.id, choice)}
 *         />
 *       )}
 *     />
 *   );
 * }
 * ```
 */
export function useProposals(client: ICNMobileClient, domainId?: string) {
  const [proposals, setProposals] = useState<Proposal[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchProposals = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await client.listProposals(domainId);
      setProposals(result);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, domainId]);

  // Use ref to avoid dependency cycles
  const fetchRef = useRef(fetchProposals);
  fetchRef.current = fetchProposals;

  useEffect(() => {
    fetchRef.current();
  }, [client, domainId]);

  const vote = useCallback(
    async (proposalId: string, choice: 'for' | 'against' | 'abstain') => {
      await client.vote(proposalId, { choice });
      await fetchRef.current();
    },
    [client]
  );

  // Auto-refresh on governance events
  useEvent(client, 'GovernanceProposalCreated', () => fetchRef.current());
  useEvent(client, 'GovernanceProposalClosed', () => fetchRef.current());
  useEvent(client, 'GovernanceVoteCast', () => fetchRef.current());

  return {
    proposals,
    isLoading,
    error,
    refresh: fetchProposals,
    vote,
  };
}

/**
 * Hook for governance domains
 */
export function useDomains(client: ICNMobileClient) {
  const [domains, setDomains] = useState<GovernanceDomain[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchDomains = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await client.listDomains();
      setDomains(result);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client]);

  // Use ref to avoid dependency cycles
  const fetchRef = useRef(fetchDomains);
  fetchRef.current = fetchDomains;

  useEffect(() => {
    fetchRef.current();
  }, [client]);

  return {
    domains,
    isLoading,
    error,
    refresh: fetchDomains,
  };
}

/**
 * Hook for fetching transaction history
 *
 * @example
 * ```tsx
 * function TransactionList() {
 *   const { transactions, isLoading, error, refresh, loadMore, hasMore } = useTransactions(client, 'my-coop');
 *
 *   if (isLoading && transactions.length === 0) return <ActivityIndicator />;
 *
 *   return (
 *     <FlatList
 *       data={transactions}
 *       renderItem={({ item }) => <TransactionRow tx={item} />}
 *       onEndReached={loadMore}
 *       ListFooterComponent={hasMore ? <ActivityIndicator /> : null}
 *     />
 *   );
 * }
 * ```
 */
export function useTransactions(client: ICNMobileClient, coopId: string, limit = 20) {
  const [transactions, setTransactions] = useState<Transaction[]>([]);
  const [total, setTotal] = useState(0);
  const [offset, setOffset] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchTransactions = useCallback(async (reset = false) => {
    setIsLoading(true);
    setError(null);
    try {
      const currentOffset = reset ? 0 : offset;
      const result = await client.getHistory(coopId, { offset: currentOffset, limit });
      if (reset) {
        setTransactions(result.transactions);
        setOffset(limit);
      } else {
        setTransactions(prev => [...prev, ...result.transactions]);
        setOffset(currentOffset + limit);
      }
      setTotal(result.total);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, coopId, limit, offset]);

  // Use ref to avoid dependency cycles
  const fetchRef = useRef(fetchTransactions);
  fetchRef.current = fetchTransactions;

  useEffect(() => {
    fetchRef.current(true);
  }, [client, coopId]);

  const refresh = useCallback(() => fetchRef.current(true), []);
  const loadMore = useCallback(() => {
    if (!isLoading && transactions.length < total) {
      fetchRef.current(false);
    }
  }, [isLoading, transactions.length, total]);

  // Auto-refresh on payment events
  useEvent(client, 'PaymentCreated', () => {
    fetchRef.current(true);
  });

  return {
    transactions,
    total,
    isLoading,
    error,
    refresh,
    loadMore,
    hasMore: transactions.length < total,
  };
}

/**
 * Simple payment request (currency defaults to 'hours')
 */
export interface SimplePaymentRequest {
  /** Recipient DID */
  to: string;
  /** Amount to pay */
  amount: number;
  /** Currency (defaults to 'hours') */
  currency?: string;
  /** Optional memo */
  memo?: string;
}

/**
 * Hook for making payments
 *
 * @example
 * ```tsx
 * function PaymentForm() {
 *   const { pay, isPaying, error } = usePayment(client, 'my-coop');
 *
 *   const handlePay = async () => {
 *     await pay({ to: recipientDid, amount: 5, memo: 'Thanks!' });
 *   };
 *
 *   return (
 *     <Button
 *       onPress={handlePay}
 *       disabled={isPaying}
 *       title={isPaying ? 'Sending...' : 'Send'}
 *     />
 *   );
 * }
 * ```
 */
export function usePayment(client: ICNMobileClient, coopId: string, defaultCurrency = 'hours') {
  const [isPaying, setIsPaying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const pay = useCallback(
    async (request: SimplePaymentRequest) => {
      setIsPaying(true);
      setError(null);
      try {
        // Get sender DID from auth state
        const senderDid = client.authState.did;
        if (!senderDid) {
          throw new Error('Not authenticated - no DID available');
        }
        // Build full payment request with required fields
        const fullRequest = {
          from: senderDid,
          to: request.to,
          amount: request.amount,
          currency: request.currency || defaultCurrency,
          memo: request.memo,
        };
        const result = await client.pay(coopId, fullRequest);
        return result;
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsPaying(false);
      }
    },
    [client, coopId, defaultCurrency]
  );

  return {
    pay,
    isPaying,
    error,
  };
}
