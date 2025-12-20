/**
 * Constitutional Governance Hooks for ICN React Native SDK
 *
 * React hooks for amendments and appeals within the ICN commons.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { ICNMobileClient } from './client';
import {
  Amendment,
  Appeal,
  CreateAmendmentRequest,
  AddChangeRequest,
  RatifyRequest,
  FileAppealRequest,
  AddEvidenceRequest,
  AddResponseRequest,
  ResolveAppealRequest,
  AmendmentStatus,
  AppealStatus,
} from './constitutional-types';

// ============================================================================
// Amendment Hooks
// ============================================================================

/**
 * Hook for viewing a single amendment
 *
 * @example
 * ```tsx
 * function AmendmentDetail({ amendmentId }: { amendmentId: string }) {
 *   const { amendment, isLoading, refresh } = useAmendment(client, amendmentId);
 *
 *   if (isLoading) return <ActivityIndicator />;
 *
 *   return (
 *     <View>
 *       <Text>{amendment?.title}</Text>
 *       <Text>Status: {amendment?.status}</Text>
 *       <Text>{amendment?.ratifications_count} ratifications</Text>
 *     </View>
 *   );
 * }
 * ```
 */
export function useAmendment(client: ICNMobileClient, amendmentId: string | null) {
  const [amendment, setAmendment] = useState<Amendment | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchAmendment = useCallback(async () => {
    if (!amendmentId) {
      setAmendment(null);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const data = await client.getAmendment(amendmentId);
      setAmendment(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, amendmentId]);

  const fetchRef = useRef(fetchAmendment);
  fetchRef.current = fetchAmendment;

  useEffect(() => {
    fetchRef.current();
  }, [client, amendmentId]);

  return {
    /** Amendment data */
    amendment,
    /** Whether data is loading */
    isLoading,
    /** Error message if any */
    error,
    /** Refresh amendment data */
    refresh: fetchAmendment,
  };
}

/**
 * Hook for listing amendments
 *
 * @example
 * ```tsx
 * function AmendmentList() {
 *   const { amendments, count, isLoading, refresh } = useAmendments(client, {
 *     status: 'voting',
 *   });
 *
 *   return (
 *     <FlatList
 *       data={amendments}
 *       refreshing={isLoading}
 *       onRefresh={refresh}
 *       renderItem={({ item }) => (
 *         <View>
 *           <Text>{item.title}</Text>
 *           <Text>{item.status}</Text>
 *         </View>
 *       )}
 *       ListHeaderComponent={<Text>{count} amendments</Text>}
 *     />
 *   );
 * }
 * ```
 */
export function useAmendments(
  client: ICNMobileClient,
  filters?: { status?: string; scope?: string; type?: string }
) {
  const [amendments, setAmendments] = useState<Amendment[]>([]);
  const [count, setCount] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchAmendments = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await client.listAmendments(filters?.status, filters?.scope, filters?.type);
      setAmendments(data.amendments);
      setCount(data.count);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, filters?.status, filters?.scope, filters?.type]);

  const fetchRef = useRef(fetchAmendments);
  fetchRef.current = fetchAmendments;

  useEffect(() => {
    fetchRef.current();
  }, [client, filters?.status, filters?.scope, filters?.type]);

  return {
    /** List of amendments */
    amendments,
    /** Total count */
    count,
    /** Whether data is loading */
    isLoading,
    /** Error message if any */
    error,
    /** Refresh amendment list */
    refresh: fetchAmendments,
  };
}

/**
 * Hook for creating and managing amendments
 *
 * @example
 * ```tsx
 * function CreateAmendment() {
 *   const {
 *     amendment,
 *     create,
 *     addChange,
 *     submit,
 *     isSubmitting,
 *   } = useAmendmentActions(client);
 *
 *   const handleCreate = async () => {
 *     const result = await create({
 *       amendment_type: 'policy',
 *       scope_type: 'jurisdiction',
 *       scope_id: 'coop:my-coop',
 *       title: 'Update Voting Rules',
 *       description: 'Proposal to update voting quorum requirements',
 *       changes: [],
 *     });
 *
 *     // Add changes to draft
 *     await addChange(result.id, {
 *       target: 'governance_rules',
 *       change_type: 'modify',
 *       description: 'Lower quorum from 50% to 40%',
 *       old_value: '50%',
 *       new_value: '40%',
 *     });
 *
 *     // Submit for review
 *     await submit(result.id);
 *   };
 *
 *   return (
 *     <Button onPress={handleCreate} disabled={isSubmitting} title="Create Amendment" />
 *   );
 * }
 * ```
 */
export function useAmendmentActions(client: ICNMobileClient) {
  const [amendment, setAmendment] = useState<Amendment | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const create = useCallback(
    async (request: CreateAmendmentRequest) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.createAmendment(request);
        setAmendment(result);
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

  const addChange = useCallback(
    async (amendmentId: string, change: AddChangeRequest) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.addAmendmentChange(amendmentId, change);
        setAmendment(result);
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

  const submit = useCallback(
    async (amendmentId: string) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.submitAmendment(amendmentId);
        setAmendment(result);
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

  const openVoting = useCallback(
    async (amendmentId: string) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.openAmendmentVoting(amendmentId);
        setAmendment(result);
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

  const ratify = useCallback(
    async (amendmentId: string, request: RatifyRequest) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.ratifyAmendment(amendmentId, request);
        setAmendment(result);
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

  const withdraw = useCallback(
    async (amendmentId: string, reason?: string) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.withdrawAmendment(amendmentId, reason);
        setAmendment(result);
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
    setAmendment(null);
    setError(null);
  }, []);

  return {
    /** Current amendment (after create/action) */
    amendment,
    /** Create a new amendment */
    create,
    /** Add a change to a draft amendment */
    addChange,
    /** Submit for review */
    submit,
    /** Open voting period */
    openVoting,
    /** Add a ratification */
    ratify,
    /** Withdraw amendment */
    withdraw,
    /** Whether an action is in progress */
    isSubmitting,
    /** Error message if any */
    error,
    /** Reset state */
    reset,
  };
}

// ============================================================================
// Appeal Hooks
// ============================================================================

/**
 * Hook for viewing a single appeal
 *
 * @example
 * ```tsx
 * function AppealDetail({ appealId }: { appealId: string }) {
 *   const { appeal, isLoading, refresh } = useAppeal(client, appealId);
 *
 *   if (isLoading) return <ActivityIndicator />;
 *
 *   return (
 *     <View>
 *       <Text>{appeal?.appeal_type}</Text>
 *       <Text>Status: {appeal?.status}</Text>
 *       <Text>{appeal?.evidence_count} pieces of evidence</Text>
 *     </View>
 *   );
 * }
 * ```
 */
export function useAppeal(client: ICNMobileClient, appealId: string | null) {
  const [appeal, setAppeal] = useState<Appeal | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchAppeal = useCallback(async () => {
    if (!appealId) {
      setAppeal(null);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const data = await client.getAppeal(appealId);
      setAppeal(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, appealId]);

  const fetchRef = useRef(fetchAppeal);
  fetchRef.current = fetchAppeal;

  useEffect(() => {
    fetchRef.current();
  }, [client, appealId]);

  return {
    /** Appeal data */
    appeal,
    /** Whether data is loading */
    isLoading,
    /** Error message if any */
    error,
    /** Refresh appeal data */
    refresh: fetchAppeal,
  };
}

/**
 * Hook for listing appeals
 *
 * @example
 * ```tsx
 * function AppealList() {
 *   const { appeals, count, isLoading, refresh } = useAppeals(client, {
 *     status: 'under_review',
 *   });
 *
 *   return (
 *     <FlatList
 *       data={appeals}
 *       refreshing={isLoading}
 *       onRefresh={refresh}
 *       renderItem={({ item }) => (
 *         <View>
 *           <Text>{item.appeal_type}</Text>
 *           <Text>{item.status}</Text>
 *         </View>
 *       )}
 *       ListHeaderComponent={<Text>{count} appeals</Text>}
 *     />
 *   );
 * }
 * ```
 */
export function useAppeals(
  client: ICNMobileClient,
  filters?: { status?: string; scope?: string; appellant?: string }
) {
  const [appeals, setAppeals] = useState<Appeal[]>([]);
  const [count, setCount] = useState(0);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchAppeals = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await client.listAppeals(filters?.status, filters?.scope, filters?.appellant);
      setAppeals(data.appeals);
      setCount(data.count);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client, filters?.status, filters?.scope, filters?.appellant]);

  const fetchRef = useRef(fetchAppeals);
  fetchRef.current = fetchAppeals;

  useEffect(() => {
    fetchRef.current();
  }, [client, filters?.status, filters?.scope, filters?.appellant]);

  return {
    /** List of appeals */
    appeals,
    /** Total count */
    count,
    /** Whether data is loading */
    isLoading,
    /** Error message if any */
    error,
    /** Refresh appeal list */
    refresh: fetchAppeals,
  };
}

/**
 * Hook for filing and managing appeals
 *
 * @example
 * ```tsx
 * function FileAppeal() {
 *   const {
 *     appeal,
 *     file,
 *     addEvidence,
 *     addResponse,
 *     withdraw,
 *     isSubmitting,
 *   } = useAppealActions(client);
 *
 *   const handleFile = async () => {
 *     const result = await file({
 *       appeal_type: { category: 'membership_denial', target_id: 'my-coop' },
 *       scope_type: 'jurisdiction',
 *       scope_id: 'coop:my-coop',
 *       grounds: [{ ground_type: 'procedural_error', description: 'Notice not provided' }],
 *       statement: 'I was denied membership without proper notice...',
 *       requested_remedy: 'reinstate',
 *     });
 *
 *     // Add evidence
 *     await addEvidence(result.id, {
 *       evidence_type: 'document',
 *       description: 'Screenshot of denial notice',
 *       uri: 'ipfs://...',
 *     });
 *   };
 *
 *   return (
 *     <Button onPress={handleFile} disabled={isSubmitting} title="File Appeal" />
 *   );
 * }
 * ```
 */
export function useAppealActions(client: ICNMobileClient) {
  const [appeal, setAppeal] = useState<Appeal | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const file = useCallback(
    async (request: FileAppealRequest) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.fileAppeal(request);
        setAppeal(result);
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

  const addEvidence = useCallback(
    async (appealId: string, evidence: AddEvidenceRequest) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.addAppealEvidence(appealId, evidence);
        setAppeal(result);
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

  const addResponse = useCallback(
    async (appealId: string, response: AddResponseRequest) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.addAppealResponse(appealId, response);
        setAppeal(result);
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

  const withdraw = useCallback(
    async (appealId: string, reason?: string) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.withdrawAppeal(appealId, reason);
        setAppeal(result);
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
    setAppeal(null);
    setError(null);
  }, []);

  return {
    /** Current appeal (after file/action) */
    appeal,
    /** File a new appeal */
    file,
    /** Add evidence to appeal */
    addEvidence,
    /** Add response to appeal */
    addResponse,
    /** Withdraw appeal */
    withdraw,
    /** Whether an action is in progress */
    isSubmitting,
    /** Error message if any */
    error,
    /** Reset state */
    reset,
  };
}

/**
 * Hook for appeal administration (admin only)
 *
 * @example
 * ```tsx
 * function AppealAdmin({ appealId }: { appealId: string }) {
 *   const { beginReview, resolve, isSubmitting, error } = useAppealAdmin(client);
 *
 *   return (
 *     <View>
 *       <Button
 *         onPress={() => beginReview(appealId)}
 *         disabled={isSubmitting}
 *         title="Begin Review"
 *       />
 *       <Button
 *         onPress={() => resolve(appealId, {
 *           outcome: 'upheld',
 *           reason: 'Procedural error confirmed',
 *           remedy: 'reinstate',
 *         })}
 *         disabled={isSubmitting}
 *         title="Uphold Appeal"
 *       />
 *     </View>
 *   );
 * }
 * ```
 */
export function useAppealAdmin(client: ICNMobileClient) {
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const beginReview = useCallback(
    async (appealId: string) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.beginAppealReview(appealId);
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

  const resolve = useCallback(
    async (appealId: string, resolution: ResolveAppealRequest) => {
      setIsSubmitting(true);
      setError(null);
      try {
        const result = await client.resolveAppeal(appealId, resolution);
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
    /** Begin review of an appeal */
    beginReview,
    /** Resolve an appeal */
    resolve,
    /** Whether an action is in progress */
    isSubmitting,
    /** Error message if any */
    error,
  };
}

// ============================================================================
// Amendment Voting Hooks (for Mobile App)
// ============================================================================

/**
 * Vote choice for amendments
 */
export type AmendmentVoteChoice = 'approve' | 'reject' | 'abstain';

/**
 * Amendment vote record
 */
export interface AmendmentVote {
  amendment_id: string;
  voter_did: string;
  vote: AmendmentVoteChoice;
  comment?: string;
  voted_at: number;
}

/**
 * Amendment voting results
 */
export interface AmendmentResults {
  amendment_id: string;
  status: AmendmentStatus;
  approve_count: number;
  reject_count: number;
  abstain_count: number;
  total_votes: number;
  eligible_voters: number;
  has_quorum: boolean;
  quorum_threshold: number;
  approval_threshold: number;
  is_approved: boolean;
}

/**
 * Hook for voting on an amendment
 *
 * @example
 * ```tsx
 * function AmendmentVoting({ amendmentId }: { amendmentId: string }) {
 *   const { vote, loading, myVote, error } = useAmendmentVoting(client, amendmentId);
 *
 *   if (myVote) {
 *     return <Text>You voted: {myVote.vote}</Text>;
 *   }
 *
 *   return (
 *     <View>
 *       <Button
 *         onPress={() => vote('approve', 'I support this')}
 *         disabled={loading}
 *         title="Approve"
 *       />
 *       <Button
 *         onPress={() => vote('reject', 'I disagree')}
 *         disabled={loading}
 *         title="Reject"
 *       />
 *     </View>
 *   );
 * }
 * ```
 */
export function useAmendmentVoting(client: ICNMobileClient, amendmentId: string) {
  const [myVote, setMyVote] = useState<AmendmentVote | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Fetch user's vote on mount
  useEffect(() => {
    if (!amendmentId || !client.authState.isAuthenticated) return;

    const fetchMyVote = async () => {
      try {
        const vote = await client.getMyAmendmentVote(amendmentId);
        setMyVote(vote);
      } catch (err) {
        // No vote cast yet is not an error
        if (!(err as Error).message.includes('not found')) {
          setError((err as Error).message);
        }
      }
    };

    fetchMyVote();
  }, [client, amendmentId, client.authState.isAuthenticated]);

  const vote = useCallback(
    async (choice: AmendmentVoteChoice, comment?: string) => {
      setLoading(true);
      setError(null);
      try {
        const result = await client.voteOnAmendment(amendmentId, choice, comment);
        setMyVote(result);
        return result;
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setLoading(false);
      }
    },
    [client, amendmentId]
  );

  return {
    /** Cast a vote */
    vote,
    /** Whether vote submission is in progress */
    loading,
    /** User's current vote (null if not voted) */
    myVote,
    /** Error message if any */
    error,
  };
}

/**
 * Hook for fetching amendment voting results
 *
 * @example
 * ```tsx
 * function AmendmentResults({ amendmentId }: { amendmentId: string }) {
 *   const { results, loading, error, refresh } = useAmendmentResults(client, amendmentId);
 *
 *   if (loading) return <ActivityIndicator />;
 *   if (error) return <Text>Error: {error}</Text>;
 *   if (!results) return null;
 *
 *   return (
 *     <View>
 *       <Text>Status: {results.status}</Text>
 *       <Text>Approve: {results.approve_count}</Text>
 *       <Text>Reject: {results.reject_count}</Text>
 *       <Text>Quorum: {results.has_quorum ? 'Met' : 'Not Met'}</Text>
 *       <ProgressBar
 *         progress={results.total_votes / results.eligible_voters}
 *       />
 *     </View>
 *   );
 * }
 * ```
 */
export function useAmendmentResults(client: ICNMobileClient, amendmentId: string) {
  const [results, setResults] = useState<AmendmentResults | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchResults = useCallback(async () => {
    if (!amendmentId) {
      setResults(null);
      setLoading(false);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const data = await client.getAmendmentResults(amendmentId);
      setResults(data);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  }, [client, amendmentId]);

  const fetchRef = useRef(fetchResults);
  fetchRef.current = fetchResults;

  useEffect(() => {
    fetchRef.current();
  }, [client, amendmentId]);

  // Auto-refresh when votes are cast
  useEffect(() => {
    if (!client.authState.isAuthenticated) return;

    // Listen for vote events
    const unsubscribe = client.onEvent?.('AmendmentVoteCast', () => {
      fetchRef.current();
    });

    return unsubscribe;
  }, [client, client.authState.isAuthenticated]);

  return {
    /** Voting results */
    results,
    /** Whether results are loading */
    loading,
    /** Error message if any */
    error,
    /** Refresh results */
    refresh: fetchResults,
  };
}
