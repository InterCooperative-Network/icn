/**
 * Governance Screen
 *
 * List of active proposals for voting.
 */

import React from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  FlatList,
  RefreshControl,
} from 'react-native';
import { NativeStackNavigationProp } from '@react-navigation/native-stack';
import { useAuth, useProposals } from '@icn/react-native';
import { client } from '../client';
import { RootStackParamList } from '../../App';

type Props = {
  navigation: NativeStackNavigationProp<RootStackParamList, 'Governance'>;
};

export function GovernanceScreen({ navigation }: Props) {
  const { coopId } = useAuth(client!);
  const domainId = coopId ? `coop:${coopId}` : undefined;
  const { proposals, isLoading, refresh } = useProposals(client!, domainId);

  const renderProposal = ({ item }: { item: any }) => {
    const statusColors: Record<string, string> = {
      draft: '#9e9e9e',
      open: '#4caf50',
      closed: '#f44336',
    };
    const statusColor = statusColors[item.state as string] || '#666';

    return (
      <TouchableOpacity
        style={styles.proposalCard}
        onPress={() => navigation.navigate('Proposal', { proposalId: item.id })}
      >
        <View style={styles.proposalHeader}>
          <View style={[styles.statusBadge, { backgroundColor: statusColor }]}>
            <Text style={styles.statusText}>{item.state}</Text>
          </View>
          <Text style={styles.proposalType}>{item.kind}</Text>
        </View>

        <Text style={styles.proposalTitle}>{item.title}</Text>

        <View style={styles.proposalMeta}>
          <Text style={styles.metaText}>
            By: {item.author?.slice(0, 16)}...
          </Text>
          {item.tally && (
            <Text style={styles.metaText}>
              For: {item.tally.for} | Against: {item.tally.against}
            </Text>
          )}
        </View>
      </TouchableOpacity>
    );
  };

  if (proposals.length === 0 && !isLoading) {
    return (
      <View style={styles.emptyContainer}>
        <Text style={styles.emptyIcon}>🗳️</Text>
        <Text style={styles.emptyTitle}>No Active Proposals</Text>
        <Text style={styles.emptyText}>
          When your cooperative creates proposals, they'll appear here for voting.
        </Text>
        <TouchableOpacity style={styles.refreshButton} onPress={refresh}>
          <Text style={styles.refreshButtonText}>Refresh</Text>
        </TouchableOpacity>
      </View>
    );
  }

  return (
    <FlatList
      style={styles.container}
      data={proposals}
      keyExtractor={(item) => item.id}
      renderItem={renderProposal}
      refreshControl={
        <RefreshControl refreshing={isLoading} onRefresh={refresh} />
      }
      contentContainerStyle={styles.list}
    />
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  list: {
    padding: 16,
  },
  proposalCard: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 16,
    marginBottom: 12,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.05,
    shadowRadius: 4,
    elevation: 2,
  },
  proposalHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 12,
  },
  statusBadge: {
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 4,
  },
  statusText: {
    color: '#fff',
    fontSize: 12,
    fontWeight: '600',
    textTransform: 'uppercase',
  },
  proposalType: {
    fontSize: 12,
    color: '#666',
    textTransform: 'capitalize',
  },
  proposalTitle: {
    fontSize: 18,
    fontWeight: '600',
    color: '#333',
    marginBottom: 12,
  },
  proposalMeta: {
    flexDirection: 'row',
    justifyContent: 'space-between',
  },
  metaText: {
    fontSize: 12,
    color: '#666',
  },
  emptyContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    padding: 32,
  },
  emptyIcon: {
    fontSize: 64,
    marginBottom: 16,
  },
  emptyTitle: {
    fontSize: 24,
    fontWeight: '600',
    color: '#333',
    marginBottom: 8,
  },
  emptyText: {
    fontSize: 16,
    color: '#666',
    textAlign: 'center',
    marginBottom: 24,
  },
  refreshButton: {
    backgroundColor: '#4A90A4',
    paddingHorizontal: 24,
    paddingVertical: 12,
    borderRadius: 8,
  },
  refreshButtonText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
  },
});
