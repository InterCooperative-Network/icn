/**
 * Governance Dashboard Hooks for ICN React Native SDK
 *
 * React hooks for governance statistics and activity.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { ICNMobileClient } from './client';

// ============================================================================
// Types
// ============================================================================

/**
 * Amendments breakdown by status
 */
export interface AmendmentsBreakdown {
  draft: number;
  submitted: number;
  voting: number;
  ratified: number;
  rejected: number;
  withdrawn: number;
}

/**
 * Appeals breakdown by status
 */
export interface AppealsBreakdown {
  filed: number;
  under_review: number;
  hearing: number;
  resolved: number;
  dismissed: number;
  withdrawn: number;
}

/**
 * Recent activity event
 */
export interface ActivityEvent {
  /** Event type */
  event_type: string;
  /** Event description */
  description: string;
  /** Timestamp (Unix seconds) */
  timestamp: number;
  /** Related entity ID */
  entity_id?: string;
  /** Actor DID */
  actor?: string;
}

/**
 * Governance dashboard data
 */
export interface GovernanceDashboard {
  /** Charter ID (hex) */
  charter_id?: string;
  /** Pending amendments count */
  pending_amendments: number;
  /** Open appeals count */
  open_appeals: number;
  /** Recent activity events */
  recent_activity: ActivityEvent[];
  /** Amendments breakdown */
  amendments_breakdown: AmendmentsBreakdown;
  /** Appeals breakdown */
  appeals_breakdown: AppealsBreakdown;
}

/**
 * Governance participation stats
 */
export interface ParticipationStats {
  /** Total eligible voters */
  total_voters: number;
  /** Voters who have participated */
  active_voters: number;
  /** Participation rate (0-100) */
  participation_rate: number;
  /** Average votes per proposal */
  avg_votes_per_proposal: number;
}

// ============================================================================
// API Functions
// ============================================================================

/**
 * Create governance dashboard API methods
 */
export function createGovernanceDashboardAPI(client: ICNMobileClient) {
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
    /**
     * Get governance dashboard for a domain
     */
    async getDashboard(domainId: string): Promise<GovernanceDashboard> {
      const response = await fetch(
        `${baseUrl}/v1/governance/${encodeURIComponent(domainId)}/dashboard`,
        {
          method: 'GET',
          headers: getAuthHeaders(),
        }
      );

      if (!response.ok) {
        throw new Error(`Failed to get governance dashboard: ${await response.text()}`);
      }

      return response.json();
    },

    /**
     * Get participation stats for a domain
     */
    async getParticipation(domainId: string): Promise<ParticipationStats> {
      const response = await fetch(
        `${baseUrl}/v1/governance/${encodeURIComponent(domainId)}/participation`,
        {
          method: 'GET',
          headers: getAuthHeaders(),
        }
      );

      if (!response.ok) {
        throw new Error(`Failed to get participation stats: ${await response.text()}`);
      }

      return response.json();
    },
  };
}

// ============================================================================
// Hooks
// ============================================================================

/**
 * Hook for governance dashboard data
 *
 * @example
 * ```tsx
 * function GovernanceDashboard({ domainId }: { domainId: string }) {
 *   const {
 *     dashboard,
 *     isLoading,
 *     error,
 *     refresh,
 *   } = useGovernanceDashboard(client, domainId);
 *
 *   if (isLoading) return <ActivityIndicator />;
 *
 *   return (
 *     <View>
 *       <Text>Pending Amendments: {dashboard?.pending_amendments}</Text>
 *       <Text>Open Appeals: {dashboard?.open_appeals}</Text>
 *
 *       <Text>Amendments by Status:</Text>
 *       <Text>  Voting: {dashboard?.amendments_breakdown.voting}</Text>
 *       <Text>  Ratified: {dashboard?.amendments_breakdown.ratified}</Text>
 *
 *       <Text>Recent Activity:</Text>
 *       {dashboard?.recent_activity.map((event, i) => (
 *         <Text key={i}>{event.description}</Text>
 *       ))}
 *     </View>
 *   );
 * }
 * ```
 */
export function useGovernanceDashboard(client: ICNMobileClient, domainId: string | null) {
  const [dashboard, setDashboard] = useState<GovernanceDashboard | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const api = createGovernanceDashboardAPI(client);

  const refresh = useCallback(async () => {
    if (!domainId || !client.authState.isAuthenticated) {
      setDashboard(null);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const data = await api.getDashboard(domainId);
      setDashboard(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [domainId, client.authState.isAuthenticated, api]);

  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;

  useEffect(() => {
    refreshRef.current();
  }, [domainId, client.authState.isAuthenticated]);

  return {
    /** Dashboard data */
    dashboard,
    /** Loading state */
    isLoading,
    /** Error message */
    error,
    /** Refresh dashboard */
    refresh,
  };
}

/**
 * Hook for governance participation stats
 *
 * @example
 * ```tsx
 * function ParticipationCard({ domainId }: { domainId: string }) {
 *   const { stats, isLoading } = useGovernanceParticipation(client, domainId);
 *
 *   return (
 *     <View>
 *       <Text>Participation Rate: {stats?.participation_rate.toFixed(1)}%</Text>
 *       <Text>Active Voters: {stats?.active_voters} / {stats?.total_voters}</Text>
 *     </View>
 *   );
 * }
 * ```
 */
export function useGovernanceParticipation(client: ICNMobileClient, domainId: string | null) {
  const [stats, setStats] = useState<ParticipationStats | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const api = createGovernanceDashboardAPI(client);

  const refresh = useCallback(async () => {
    if (!domainId || !client.authState.isAuthenticated) {
      setStats(null);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const data = await api.getParticipation(domainId);
      setStats(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [domainId, client.authState.isAuthenticated, api]);

  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;

  useEffect(() => {
    refreshRef.current();
  }, [domainId, client.authState.isAuthenticated]);

  return {
    /** Participation stats */
    stats,
    /** Loading state */
    isLoading,
    /** Error message */
    error,
    /** Refresh stats */
    refresh,
  };
}

/**
 * Combined hook for full governance overview
 *
 * @example
 * ```tsx
 * function GovernanceOverview({ domainId }: { domainId: string }) {
 *   const {
 *     dashboard,
 *     participation,
 *     isLoading,
 *     refresh,
 *   } = useGovernanceOverview(client, domainId);
 *
 *   return (
 *     <ScrollView>
 *       <ParticipationCard stats={participation} />
 *       <AmendmentsSummary breakdown={dashboard?.amendments_breakdown} />
 *       <AppealsSummary breakdown={dashboard?.appeals_breakdown} />
 *       <RecentActivity events={dashboard?.recent_activity} />
 *     </ScrollView>
 *   );
 * }
 * ```
 */
export function useGovernanceOverview(client: ICNMobileClient, domainId: string | null) {
  const {
    dashboard,
    isLoading: dashboardLoading,
    error: dashboardError,
    refresh: refreshDashboard,
  } = useGovernanceDashboard(client, domainId);

  const {
    stats: participation,
    isLoading: participationLoading,
    error: participationError,
    refresh: refreshParticipation,
  } = useGovernanceParticipation(client, domainId);

  const refresh = useCallback(async () => {
    await Promise.all([refreshDashboard(), refreshParticipation()]);
  }, [refreshDashboard, refreshParticipation]);

  return {
    /** Dashboard data */
    dashboard,
    /** Participation stats */
    participation,
    /** Loading state (either loading) */
    isLoading: dashboardLoading || participationLoading,
    /** Error message (first error) */
    error: dashboardError || participationError,
    /** Refresh all data */
    refresh,
  };
}
