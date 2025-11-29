/**
 * React Hooks for ICN Mobile SDK
 *
 * Easy-to-use hooks for common ICN operations.
 */

import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
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

  const fetch = useCallback(async () => {
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

  useEffect(() => {
    fetch();
  }, [fetch]);

  // Auto-refresh on payment events
  useEvent(client, 'PaymentCreated', () => {
    fetch();
  });

  return {
    balance,
    isLoading,
    error,
    refresh: fetch,
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

  const fetch = useCallback(async () => {
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

  useEffect(() => {
    fetch();
  }, [fetch]);

  return {
    coop,
    members,
    isLoading,
    error,
    refresh: fetch,
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

  const fetch = useCallback(async () => {
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

  useEffect(() => {
    fetch();
  }, [fetch]);

  const vote = useCallback(
    async (proposalId: string, choice: 'for' | 'against' | 'abstain') => {
      await client.vote(proposalId, { choice });
      await fetch();
    },
    [client, fetch]
  );

  // Auto-refresh on governance events
  useEvent(client, 'GovernanceProposalCreated', fetch);
  useEvent(client, 'GovernanceProposalClosed', fetch);
  useEvent(client, 'GovernanceVoteCast', fetch);

  return {
    proposals,
    isLoading,
    error,
    refresh: fetch,
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

  const fetch = useCallback(async () => {
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

  useEffect(() => {
    fetch();
  }, [fetch]);

  return {
    domains,
    isLoading,
    error,
    refresh: fetch,
  };
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
export function usePayment(client: ICNMobileClient, coopId: string) {
  const [isPaying, setIsPaying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const pay = useCallback(
    async (request: { to: string; amount: number; memo?: string }) => {
      setIsPaying(true);
      setError(null);
      try {
        const result = await client.pay(coopId, request);
        return result;
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsPaying(false);
      }
    },
    [client, coopId]
  );

  return {
    pay,
    isPaying,
    error,
  };
}
