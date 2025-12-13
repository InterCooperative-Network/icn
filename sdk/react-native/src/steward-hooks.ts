/**
 * SDIS Steward Hooks
 *
 * React hooks for steward dashboard operations.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { ICNMobileClient } from './client';
import {
  Enrollment,
  StewardStats,
  VouchRecord,
  PendingEnrollmentsFilter,
} from './steward-types';

/**
 * Hook for fetching pending enrollments
 *
 * @example
 * ```tsx
 * function PendingList() {
 *   const { enrollments, pendingCount, isLoading, refresh } = usePendingEnrollments(client);
 *
 *   return (
 *     <FlatList
 *       data={enrollments}
 *       refreshing={isLoading}
 *       onRefresh={refresh}
 *       renderItem={({ item }) => <EnrollmentCard enrollment={item} />}
 *     />
 *   );
 * }
 * ```
 */
export function usePendingEnrollments(
  client: ICNMobileClient,
  options?: {
    filter?: PendingEnrollmentsFilter;
    autoRefresh?: boolean;
    refreshInterval?: number;
  }
) {
  const [enrollments, setEnrollments] = useState<Enrollment[]>([]);
  const [pendingCount, setPendingCount] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchEnrollments = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await client.getPendingEnrollments(options?.filter);
      setEnrollments(data.enrollments);
      setPendingCount(data.pending_count);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, options?.filter?.coop_id, options?.filter?.level]);

  const fetchRef = useRef(fetchEnrollments);
  fetchRef.current = fetchEnrollments;

  useEffect(() => {
    fetchRef.current();

    if (options?.autoRefresh !== false) {
      const interval = setInterval(
        () => fetchRef.current(),
        options?.refreshInterval ?? 30000
      );
      return () => clearInterval(interval);
    }
  }, [options?.autoRefresh, options?.refreshInterval]);

  return {
    /** List of pending enrollments */
    enrollments,
    /** Total pending count */
    pendingCount,
    /** Whether data is loading */
    isLoading,
    /** Error message if any */
    error,
    /** Manually refresh the list */
    refresh: fetchEnrollments,
  };
}

/**
 * Hook for submitting a vouch
 *
 * @example
 * ```tsx
 * function VouchButton({ enrollmentId }: { enrollmentId: string }) {
 *   const { vouch, isSubmitting, error, success } = useVouch(client);
 *
 *   const handleVouch = async () => {
 *     await vouch(enrollmentId, 'I have verified this person in person.');
 *   };
 *
 *   return (
 *     <Button onPress={handleVouch} disabled={isSubmitting}>
 *       {isSubmitting ? 'Submitting...' : 'Vouch'}
 *     </Button>
 *   );
 * }
 * ```
 */
export function useVouch(client: ICNMobileClient) {
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  const vouch = useCallback(
    async (enrollmentId: string, statement: string) => {
      setIsSubmitting(true);
      setError(null);
      setSuccess(false);
      try {
        await client.submitVouch(enrollmentId, statement);
        setSuccess(true);
        return true;
      } catch (err) {
        setError((err as Error).message);
        return false;
      } finally {
        setIsSubmitting(false);
      }
    },
    [client]
  );

  const reset = useCallback(() => {
    setError(null);
    setSuccess(false);
  }, []);

  return {
    /** Submit a vouch */
    vouch,
    /** Whether submission is in progress */
    isSubmitting,
    /** Error message if any */
    error,
    /** Whether vouch was successful */
    success,
    /** Reset state */
    reset,
  };
}

/**
 * Hook for rejecting an enrollment
 *
 * @example
 * ```tsx
 * function RejectButton({ enrollmentId }: { enrollmentId: string }) {
 *   const { reject, isSubmitting, error } = useReject(client);
 *
 *   const handleReject = async () => {
 *     await reject(enrollmentId, 'Unable to verify identity.');
 *   };
 *
 *   return (
 *     <Button onPress={handleReject} disabled={isSubmitting} color="red">
 *       Reject
 *     </Button>
 *   );
 * }
 * ```
 */
export function useReject(client: ICNMobileClient) {
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  const reject = useCallback(
    async (enrollmentId: string, reason: string) => {
      setIsSubmitting(true);
      setError(null);
      setSuccess(false);
      try {
        await client.rejectEnrollment(enrollmentId, reason);
        setSuccess(true);
        return true;
      } catch (err) {
        setError((err as Error).message);
        return false;
      } finally {
        setIsSubmitting(false);
      }
    },
    [client]
  );

  const reset = useCallback(() => {
    setError(null);
    setSuccess(false);
  }, []);

  return {
    /** Reject an enrollment */
    reject,
    /** Whether submission is in progress */
    isSubmitting,
    /** Error message if any */
    error,
    /** Whether rejection was successful */
    success,
    /** Reset state */
    reset,
  };
}

/**
 * Hook for fetching steward statistics
 *
 * @example
 * ```tsx
 * function StatsCard() {
 *   const { stats, isLoading } = useStewardStats(client);
 *
 *   if (isLoading || !stats) return <ActivityIndicator />;
 *
 *   return (
 *     <View>
 *       <Text>Total Vouches: {stats.total_vouches}</Text>
 *       <Text>This Month: {stats.monthly_vouches}</Text>
 *     </View>
 *   );
 * }
 * ```
 */
export function useStewardStats(client: ICNMobileClient) {
  const [stats, setStats] = useState<StewardStats | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchStats = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await client.getStewardStats();
      setStats(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client]);

  useEffect(() => {
    fetchStats();
  }, [fetchStats]);

  return {
    /** Steward statistics */
    stats,
    /** Whether stats are loading */
    isLoading,
    /** Error message if any */
    error,
    /** Manually refresh stats */
    refresh: fetchStats,
  };
}

/**
 * Hook for fetching vouch history
 *
 * @example
 * ```tsx
 * function HistoryList() {
 *   const { history, total, isLoading } = useVouchHistory(client, 50);
 *
 *   return (
 *     <FlatList
 *       data={history}
 *       ListHeaderComponent={<Text>Total: {total} vouches</Text>}
 *       renderItem={({ item }) => <VouchCard vouch={item} />}
 *     />
 *   );
 * }
 * ```
 */
export function useVouchHistory(client: ICNMobileClient, limit = 50) {
  const [history, setHistory] = useState<VouchRecord[]>([]);
  const [total, setTotal] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchHistory = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await client.getVouchHistory(limit, 0);
      setHistory(data.vouches);
      setTotal(data.total);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, limit]);

  useEffect(() => {
    fetchHistory();
  }, [fetchHistory]);

  return {
    /** Vouch history records */
    history,
    /** Total number of vouches */
    total,
    /** Whether history is loading */
    isLoading,
    /** Error message if any */
    error,
    /** Manually refresh history */
    refresh: fetchHistory,
  };
}

/**
 * Hook for fetching a single enrollment's details
 *
 * @example
 * ```tsx
 * function EnrollmentDetail({ enrollmentId }: { enrollmentId: string }) {
 *   const { enrollment, isLoading, error } = useEnrollmentDetail(client, enrollmentId);
 *
 *   if (isLoading) return <ActivityIndicator />;
 *   if (error) return <Text>Error: {error}</Text>;
 *   if (!enrollment) return <Text>Not found</Text>;
 *
 *   return (
 *     <View>
 *       <Text>{enrollment.identity_name}</Text>
 *       <Text>Level: {enrollment.level}</Text>
 *     </View>
 *   );
 * }
 * ```
 */
export function useEnrollmentDetail(
  client: ICNMobileClient,
  enrollmentId: string | null
) {
  const [enrollment, setEnrollment] = useState<Enrollment | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchEnrollment = useCallback(async () => {
    if (!enrollmentId) return;
    setIsLoading(true);
    setError(null);
    try {
      const data = await client.getEnrollmentStatus(enrollmentId);
      setEnrollment(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, enrollmentId]);

  useEffect(() => {
    fetchEnrollment();
  }, [fetchEnrollment]);

  return {
    /** Enrollment details */
    enrollment,
    /** Whether enrollment is loading */
    isLoading,
    /** Error message if any */
    error,
    /** Manually refresh enrollment */
    refresh: fetchEnrollment,
  };
}
