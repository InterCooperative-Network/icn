/**
 * Charter Hooks for ICN React Native SDK
 *
 * React hooks for commons charter management.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { ICNMobileClient } from './client';
import {
  Charter,
  CharterSummary,
  CharterSignResponse,
  CreateCharterRequest,
  OrgType,
  CharterStatus,
} from './charter-types';

/**
 * Hook for viewing a single charter
 *
 * @example
 * ```tsx
 * function CharterView({ charterId }: { charterId: string }) {
 *   const { charter, isLoading, error, refresh } = useCharter(client, charterId);
 *
 *   if (isLoading) return <ActivityIndicator />;
 *   if (error) return <Text>Error: {error}</Text>;
 *   if (!charter) return <Text>Not found</Text>;
 *
 *   return (
 *     <View>
 *       <Text>{charter.name}</Text>
 *       <Text>Status: {charter.status}</Text>
 *       <Text>{charter.founders.length} founders</Text>
 *     </View>
 *   );
 * }
 * ```
 */
export function useCharter(
  client: ICNMobileClient,
  charterId: string | null,
  options?: { byDomain?: boolean }
) {
  const [charter, setCharter] = useState<Charter | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchCharter = useCallback(async () => {
    if (!charterId) {
      setCharter(null);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const data = options?.byDomain
        ? await client.getCharterByDomain(charterId)
        : await client.getCharter(charterId);
      setCharter(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, charterId, options?.byDomain]);

  const fetchRef = useRef(fetchCharter);
  fetchRef.current = fetchCharter;

  useEffect(() => {
    fetchRef.current();
  }, [client, charterId]);

  return {
    /** Charter data */
    charter,
    /** Whether data is loading */
    isLoading,
    /** Error message if any */
    error,
    /** Refresh charter data */
    refresh: fetchCharter,
  };
}

/**
 * Hook for listing charters
 *
 * @example
 * ```tsx
 * function CharterList() {
 *   const { charters, isLoading, refresh } = useCharters(client, {
 *     orgType: 'cooperative',
 *     status: 'active',
 *   });
 *
 *   return (
 *     <FlatList
 *       data={charters}
 *       refreshing={isLoading}
 *       onRefresh={refresh}
 *       renderItem={({ item }) => (
 *         <View>
 *           <Text>{item.name}</Text>
 *           <Text>{item.founder_count} founders</Text>
 *         </View>
 *       )}
 *     />
 *   );
 * }
 * ```
 */
export function useCharters(
  client: ICNMobileClient,
  filters?: { orgType?: OrgType; status?: CharterStatus }
) {
  const [charters, setCharters] = useState<CharterSummary[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchCharters = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await client.listCharters(filters?.orgType, filters?.status);
      setCharters(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, filters?.orgType, filters?.status]);

  const fetchRef = useRef(fetchCharters);
  fetchRef.current = fetchCharters;

  useEffect(() => {
    fetchRef.current();
  }, [client, filters?.orgType, filters?.status]);

  return {
    /** List of charters */
    charters,
    /** Whether data is loading */
    isLoading,
    /** Error message if any */
    error,
    /** Refresh charter list */
    refresh: fetchCharters,
  };
}

/**
 * Hook for creating a charter
 *
 * @example
 * ```tsx
 * function CreateCharterForm() {
 *   const { createCharter, isSubmitting, error, charter } = useCreateCharter(client);
 *
 *   const handleCreate = async () => {
 *     await createCharter({
 *       domain_id: 'coop:my-new-coop',
 *       name: 'My New Cooperative',
 *       org_type: 'cooperative',
 *     });
 *   };
 *
 *   return (
 *     <View>
 *       <Button onPress={handleCreate} disabled={isSubmitting} title="Create" />
 *       {charter && <Text>Created: {charter.charter_id}</Text>}
 *     </View>
 *   );
 * }
 * ```
 */
export function useCreateCharter(client: ICNMobileClient) {
  const [charter, setCharter] = useState<Charter | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const createCharter = useCallback(
    async (request: CreateCharterRequest) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.createCharter(request);
        setCharter(result);
        return result;
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsSubmitting(false);
      }
    },
    [client]
  );

  const reset = useCallback(() => {
    setCharter(null);
    setError(null);
  }, []);

  return {
    /** Created charter (if successful) */
    charter,
    /** Create a new charter */
    createCharter,
    /** Whether submission is in progress */
    isSubmitting,
    /** Error message if any */
    error,
    /** Reset state */
    reset,
  };
}

/**
 * Hook for signing and activating charters
 *
 * @example
 * ```tsx
 * function CharterSignature({ charterId }: { charterId: string }) {
 *   const { signResult, sign, activate, isSubmitting, error } = useCharterSignature(client);
 *
 *   const handleSign = async () => {
 *     // Generate signature using wallet
 *     const signature = await wallet.sign(charterId);
 *     await sign(charterId, signature, 'founding_member');
 *   };
 *
 *   return (
 *     <View>
 *       <Button onPress={handleSign} disabled={isSubmitting} title="Sign Charter" />
 *       {signResult?.ready_for_activation && (
 *         <Button onPress={() => activate(charterId)} title="Activate" />
 *       )}
 *       {signResult && (
 *         <Text>
 *           {signResult.total_founders} founders signed
 *           {signResult.founders_needed > 0 && ` (${signResult.founders_needed} more needed)`}
 *         </Text>
 *       )}
 *     </View>
 *   );
 * }
 * ```
 */
export function useCharterSignature(client: ICNMobileClient) {
  const [signResult, setSignResult] = useState<CharterSignResponse | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  const sign = useCallback(
    async (charterId: string, signature: string, role?: string) => {
      setIsSubmitting(true);
      setError(null);
      setSuccess(false);
      try {
        const result = await client.signCharter(charterId, signature, role);
        setSignResult(result);
        setSuccess(true);
        return result;
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsSubmitting(false);
      }
    },
    [client]
  );

  const activate = useCallback(
    async (charterId: string) => {
      setIsSubmitting(true);
      setError(null);
      setSuccess(false);
      try {
        const result = await client.activateCharter(charterId);
        setSuccess(true);
        return result;
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsSubmitting(false);
      }
    },
    [client]
  );

  const reset = useCallback(() => {
    setSignResult(null);
    setError(null);
    setSuccess(false);
  }, []);

  return {
    /** Sign result (after signing) */
    signResult,
    /** Sign the charter */
    sign,
    /** Activate the charter */
    activate,
    /** Whether an operation is in progress */
    isSubmitting,
    /** Error message if any */
    error,
    /** Whether last operation was successful */
    success,
    /** Reset state */
    reset,
  };
}

/**
 * Hook for updating charter status
 *
 * @example
 * ```tsx
 * function CharterAdmin({ charterId }: { charterId: string }) {
 *   const { updateStatus, isSubmitting, error } = useCharterStatus(client);
 *
 *   return (
 *     <View>
 *       <Button
 *         onPress={() => updateStatus(charterId, 'suspended', 'Under review')}
 *         disabled={isSubmitting}
 *         title="Suspend"
 *       />
 *     </View>
 *   );
 * }
 * ```
 */
export function useCharterStatus(client: ICNMobileClient) {
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const updateStatus = useCallback(
    async (charterId: string, status: CharterStatus, reason?: string) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.updateCharterStatus(charterId, status, reason);
        return result;
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsSubmitting(false);
      }
    },
    [client]
  );

  return {
    /** Update charter status */
    updateStatus,
    /** Whether operation is in progress */
    isSubmitting,
    /** Error message if any */
    error,
  };
}
