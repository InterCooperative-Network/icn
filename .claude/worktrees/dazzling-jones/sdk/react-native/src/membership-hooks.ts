/**
 * Membership Hooks for ICN React Native SDK
 *
 * React hooks for membership management within jurisdictions.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { ICNMobileClient } from './client';
import {
  MemberResponse,
  MemberListResponse,
  MembershipActionResponse,
  CapabilityCheckResponse,
  MembershipCapability,
} from './membership-types';

/**
 * Hook for managing own membership status
 *
 * @example
 * ```tsx
 * function MembershipStatus() {
 *   const { membership, isLoading, applyForMembership } = useMembership(client, 'my-jurisdiction');
 *
 *   if (isLoading) return <ActivityIndicator />;
 *
 *   if (!membership) {
 *     return <Button onPress={() => applyForMembership(['vote'])} title="Join" />;
 *   }
 *
 *   return (
 *     <View>
 *       <Text>Status: {membership.status}</Text>
 *       <Text>Roles: {membership.roles.join(', ')}</Text>
 *     </View>
 *   );
 * }
 * ```
 */
export function useMembership(client: ICNMobileClient, jurisdictionId: string) {
  const [membership, setMembership] = useState<MemberResponse | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchMembership = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await client.getMembershipStatus(jurisdictionId);
      setMembership(data);
    } catch (err) {
      const message = (err as Error).message;
      // Not a member is not an error, just no membership
      if (message.includes('Not a member') || message.includes('not found')) {
        setMembership(null);
      } else {
        setError(message);
      }
    } finally {
      setIsLoading(false);
    }
  }, [client, jurisdictionId]);

  const fetchRef = useRef(fetchMembership);
  fetchRef.current = fetchMembership;

  useEffect(() => {
    fetchRef.current();
  }, [client, jurisdictionId]);

  const applyForMembership = useCallback(
    async (capabilitiesRequested?: MembershipCapability[]) => {
      setIsLoading(true);
      setError(null);
      try {
        const result = await client.applyForMembership(jurisdictionId, capabilitiesRequested);
        setMembership(result);
        return result;
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsLoading(false);
      }
    },
    [client, jurisdictionId]
  );

  const exitMembership = useCallback(async () => {
    if (!membership) return;
    setIsLoading(true);
    setError(null);
    try {
      await client.exitMembership(membership.holder_id, jurisdictionId);
      setMembership(null);
    } catch (err) {
      setError((err as Error).message);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, [client, jurisdictionId, membership]);

  return {
    /** Current membership status */
    membership,
    /** Whether data is loading */
    isLoading,
    /** Error message if any */
    error,
    /** Refresh membership data */
    refresh: fetchMembership,
    /** Apply for membership */
    applyForMembership,
    /** Exit membership */
    exitMembership,
  };
}

/**
 * Hook for listing members of a jurisdiction
 *
 * @example
 * ```tsx
 * function MembersList() {
 *   const { members, total, isLoading, refresh } = useJurisdictionMembers(client, 'my-jurisdiction');
 *
 *   return (
 *     <FlatList
 *       data={members}
 *       refreshing={isLoading}
 *       onRefresh={refresh}
 *       renderItem={({ item }) => (
 *         <View>
 *           <Text>{item.holder_did}</Text>
 *           <Text>{item.status}</Text>
 *         </View>
 *       )}
 *       ListHeaderComponent={<Text>{total} members</Text>}
 *     />
 *   );
 * }
 * ```
 */
export function useJurisdictionMembers(
  client: ICNMobileClient,
  jurisdictionId: string,
  statusFilter?: string
) {
  const [members, setMembers] = useState<MemberResponse[]>([]);
  const [total, setTotal] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchMembers = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await client.listJurisdictionMembers(jurisdictionId, statusFilter);
      setMembers(data.members);
      setTotal(data.total);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, jurisdictionId, statusFilter]);

  const fetchRef = useRef(fetchMembers);
  fetchRef.current = fetchMembers;

  useEffect(() => {
    fetchRef.current();
  }, [client, jurisdictionId, statusFilter]);

  return {
    /** List of members */
    members,
    /** Total member count */
    total,
    /** Whether data is loading */
    isLoading,
    /** Error message if any */
    error,
    /** Refresh member list */
    refresh: fetchMembers,
  };
}

/**
 * Hook for membership admin actions (approve, promote, suspend, etc.)
 *
 * @example
 * ```tsx
 * function MemberActions({ holderId, jurisdictionId }: Props) {
 *   const { approve, promote, suspend, reinstate, isSubmitting } = useMembershipAdmin(client);
 *
 *   return (
 *     <View>
 *       <Button
 *         onPress={() => approve(holderId, jurisdictionId)}
 *         disabled={isSubmitting}
 *         title="Approve"
 *       />
 *       <Button
 *         onPress={() => promote(holderId, jurisdictionId)}
 *         disabled={isSubmitting}
 *         title="Promote"
 *       />
 *     </View>
 *   );
 * }
 * ```
 */
export function useMembershipAdmin(client: ICNMobileClient) {
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  const executeAction = useCallback(
    async (
      action: (holderId: string, jurisdictionId: string) => Promise<MembershipActionResponse>,
      holderId: string,
      jurisdictionId: string
    ) => {
      setIsSubmitting(true);
      setError(null);
      setSuccess(false);
      try {
        const result = await action(holderId, jurisdictionId);
        setSuccess(true);
        return result;
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsSubmitting(false);
      }
    },
    []
  );

  const approve = useCallback(
    (holderId: string, jurisdictionId: string) =>
      executeAction(
        (h, j) => client.approveMembership(h, j),
        holderId,
        jurisdictionId
      ),
    [client, executeAction]
  );

  const promote = useCallback(
    (holderId: string, jurisdictionId: string) =>
      executeAction(
        (h, j) => client.promoteMember(h, j),
        holderId,
        jurisdictionId
      ),
    [client, executeAction]
  );

  const suspend = useCallback(
    (holderId: string, jurisdictionId: string) =>
      executeAction(
        (h, j) => client.suspendMember(h, j),
        holderId,
        jurisdictionId
      ),
    [client, executeAction]
  );

  const reinstate = useCallback(
    (holderId: string, jurisdictionId: string) =>
      executeAction(
        (h, j) => client.reinstateMember(h, j),
        holderId,
        jurisdictionId
      ),
    [client, executeAction]
  );

  const reset = useCallback(() => {
    setError(null);
    setSuccess(false);
  }, []);

  return {
    /** Approve a membership application */
    approve,
    /** Promote to full member */
    promote,
    /** Suspend a member */
    suspend,
    /** Reinstate a suspended member */
    reinstate,
    /** Whether an action is in progress */
    isSubmitting,
    /** Error message if any */
    error,
    /** Whether last action was successful */
    success,
    /** Reset state */
    reset,
  };
}

/**
 * Hook for capability management
 *
 * @example
 * ```tsx
 * function CapabilityManager({ holderId, jurisdictionId }: Props) {
 *   const { hasCapability, grant, revoke, check, isLoading } = useCapabilities(
 *     client,
 *     holderId,
 *     jurisdictionId
 *   );
 *
 *   return (
 *     <View>
 *       <Text>Has vote: {hasCapability('vote') ? 'Yes' : 'No'}</Text>
 *       <Button onPress={() => grant('propose')} title="Grant Propose" />
 *       <Button onPress={() => check('vote')} title="Check Vote" />
 *     </View>
 *   );
 * }
 * ```
 */
export function useCapabilities(
  client: ICNMobileClient,
  holderId: string,
  jurisdictionId: string
) {
  const [capabilities, setCapabilities] = useState<Map<string, boolean>>(new Map());
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const check = useCallback(
    async (capability: string): Promise<boolean> => {
      setIsLoading(true);
      setError(null);
      try {
        const result = await client.checkCapability(holderId, jurisdictionId, capability);
        setCapabilities((prev) => new Map(prev).set(capability, result.has_capability));
        return result.has_capability;
      } catch (err) {
        setError((err as Error).message);
        return false;
      } finally {
        setIsLoading(false);
      }
    },
    [client, holderId, jurisdictionId]
  );

  const grant = useCallback(
    async (capability: string) => {
      setIsLoading(true);
      setError(null);
      try {
        await client.grantCapability(holderId, jurisdictionId, capability);
        setCapabilities((prev) => new Map(prev).set(capability, true));
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsLoading(false);
      }
    },
    [client, holderId, jurisdictionId]
  );

  const revoke = useCallback(
    async (capability: string) => {
      setIsLoading(true);
      setError(null);
      try {
        await client.revokeCapability(holderId, jurisdictionId, capability);
        setCapabilities((prev) => new Map(prev).set(capability, false));
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsLoading(false);
      }
    },
    [client, holderId, jurisdictionId]
  );

  const hasCapability = useCallback(
    (capability: string): boolean | undefined => {
      return capabilities.get(capability);
    },
    [capabilities]
  );

  return {
    /** Check if a capability is granted (undefined if not checked) */
    hasCapability,
    /** Check a capability from server */
    check,
    /** Grant a capability */
    grant,
    /** Revoke a capability */
    revoke,
    /** Whether an operation is in progress */
    isLoading,
    /** Error message if any */
    error,
  };
}

/**
 * Hook for role management
 *
 * @example
 * ```tsx
 * function RoleManager({ holderId, jurisdictionId }: Props) {
 *   const { addRole, removeRole, isSubmitting } = useRoles(client);
 *
 *   return (
 *     <View>
 *       <Button
 *         onPress={() => addRole(holderId, jurisdictionId, 'treasurer')}
 *         disabled={isSubmitting}
 *         title="Add Treasurer Role"
 *       />
 *     </View>
 *   );
 * }
 * ```
 */
export function useRoles(client: ICNMobileClient) {
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const addRole = useCallback(
    async (holderId: string, jurisdictionId: string, role: string) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.addMemberRole(holderId, jurisdictionId, role);
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

  const removeRole = useCallback(
    async (holderId: string, jurisdictionId: string, role: string) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.removeMemberRole(holderId, jurisdictionId, role);
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
    /** Add a role to a member */
    addRole,
    /** Remove a role from a member */
    removeRole,
    /** Whether an operation is in progress */
    isSubmitting,
    /** Error message if any */
    error,
  };
}
