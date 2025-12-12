/**
 * Proposal Screen
 *
 * View proposal details and cast a vote.
 */

import React, { useState, useEffect } from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  ScrollView,
  Alert,
  ActivityIndicator,
} from 'react-native';
import { RouteProp } from '@react-navigation/native';
import { NativeStackNavigationProp } from '@react-navigation/native-stack';
import { useProposals } from '@icn/react-native';
import { client } from '../client';
import { RootStackParamList } from '../../App';

type Props = {
  navigation: NativeStackNavigationProp<RootStackParamList, 'Proposal'>;
  route: RouteProp<RootStackParamList, 'Proposal'>;
};

export function ProposalScreen({ route, navigation }: Props) {
  const { proposalId } = route.params;
  const { proposals, vote, isLoading } = useProposals(client!);
  const [voting, setVoting] = useState(false);

  const proposal = proposals.find((p) => p.id === proposalId);

  useEffect(() => {
    if (!proposal && !isLoading) {
      Alert.alert('Error', 'Proposal not found', [
        { text: 'OK', onPress: () => navigation.goBack() },
      ]);
    }
  }, [proposal, isLoading, navigation]);

  const handleVote = async (choice: 'for' | 'against' | 'abstain') => {
    setVoting(true);
    try {
      await vote(proposalId, choice);
      Alert.alert('Vote Cast', `You voted "${choice}" on this proposal.`);
    } catch (error) {
      Alert.alert('Error', (error as Error).message);
    } finally {
      setVoting(false);
    }
  };

  if (isLoading || !proposal) {
    return (
      <View style={styles.loading}>
        <ActivityIndicator size="large" color="#4A90A4" />
      </View>
    );
  }

  const isOpen = proposal.state === 'open';
  // Cast to extended type that may include tally from API
  const extendedProposal = proposal as typeof proposal & {
    tally?: { for: number; against: number; abstain: number };
    outcome?: 'accepted' | 'rejected';
  };
  const tally = extendedProposal.tally || { for: 0, against: 0, abstain: 0 };
  const totalVotes = tally.for + tally.against + tally.abstain;

  return (
    <ScrollView style={styles.container}>
      {/* Header */}
      <View style={styles.header}>
        <View
          style={[
            styles.statusBadge,
            { backgroundColor: isOpen ? '#4caf50' : '#9e9e9e' },
          ]}
        >
          <Text style={styles.statusText}>{proposal.state}</Text>
        </View>
        <Text style={styles.type}>{proposal.kind}</Text>
      </View>

      {/* Title & Description */}
      <Text style={styles.title}>{proposal.title}</Text>
      {proposal.description && (
        <Text style={styles.description}>{proposal.description}</Text>
      )}

      {/* Author */}
      <View style={styles.section}>
        <Text style={styles.sectionLabel}>Proposed by</Text>
        <Text style={styles.author}>{proposal.created_by}</Text>
      </View>

      {/* Voting Results */}
      <View style={styles.section}>
        <Text style={styles.sectionLabel}>Current Tally</Text>
        <View style={styles.tallyContainer}>
          <View style={styles.tallyItem}>
            <Text style={styles.tallyCount}>{tally.for}</Text>
            <Text style={styles.tallyLabel}>For</Text>
          </View>
          <View style={styles.tallyItem}>
            <Text style={styles.tallyCount}>{tally.against}</Text>
            <Text style={styles.tallyLabel}>Against</Text>
          </View>
          <View style={styles.tallyItem}>
            <Text style={styles.tallyCount}>{tally.abstain}</Text>
            <Text style={styles.tallyLabel}>Abstain</Text>
          </View>
        </View>
        {totalVotes > 0 && (
          <View style={styles.progressBar}>
            <View
              style={[
                styles.progressFor,
                { flex: tally.for / totalVotes || 0 },
              ]}
            />
            <View
              style={[
                styles.progressAgainst,
                { flex: tally.against / totalVotes || 0 },
              ]}
            />
            <View
              style={[
                styles.progressAbstain,
                { flex: tally.abstain / totalVotes || 0 },
              ]}
            />
          </View>
        )}
      </View>

      {/* Vote Buttons */}
      {isOpen && (
        <View style={styles.voteSection}>
          <Text style={styles.sectionLabel}>Cast Your Vote</Text>
          <View style={styles.voteButtons}>
            <TouchableOpacity
              style={[styles.voteButton, styles.voteFor]}
              onPress={() => handleVote('for')}
              disabled={voting}
            >
              <Text style={styles.voteButtonText}>For</Text>
            </TouchableOpacity>
            <TouchableOpacity
              style={[styles.voteButton, styles.voteAgainst]}
              onPress={() => handleVote('against')}
              disabled={voting}
            >
              <Text style={styles.voteButtonText}>Against</Text>
            </TouchableOpacity>
            <TouchableOpacity
              style={[styles.voteButton, styles.voteAbstain]}
              onPress={() => handleVote('abstain')}
              disabled={voting}
            >
              <Text style={styles.voteButtonText}>Abstain</Text>
            </TouchableOpacity>
          </View>
          {voting && <ActivityIndicator style={styles.votingIndicator} />}
        </View>
      )}

      {/* Outcome (for closed proposals) */}
      {proposal.state === 'closed' && extendedProposal.outcome && (
        <View style={styles.section}>
          <Text style={styles.sectionLabel}>Outcome</Text>
          <View
            style={[
              styles.outcomeBadge,
              {
                backgroundColor:
                  extendedProposal.outcome === 'accepted' ? '#4caf50' : '#f44336',
              },
            ]}
          >
            <Text style={styles.outcomeText}>{extendedProposal.outcome}</Text>
          </View>
        </View>
      )}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  loading: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  header: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: 16,
    backgroundColor: '#fff',
    borderBottomWidth: 1,
    borderBottomColor: '#e0e0e0',
  },
  statusBadge: {
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 4,
  },
  statusText: {
    color: '#fff',
    fontSize: 12,
    fontWeight: '600',
    textTransform: 'uppercase',
  },
  type: {
    fontSize: 14,
    color: '#666',
    textTransform: 'capitalize',
  },
  title: {
    fontSize: 24,
    fontWeight: 'bold',
    color: '#333',
    padding: 16,
    paddingBottom: 8,
    backgroundColor: '#fff',
  },
  description: {
    fontSize: 16,
    color: '#666',
    lineHeight: 24,
    paddingHorizontal: 16,
    paddingBottom: 16,
    backgroundColor: '#fff',
  },
  section: {
    backgroundColor: '#fff',
    padding: 16,
    marginTop: 12,
  },
  sectionLabel: {
    fontSize: 12,
    color: '#666',
    marginBottom: 8,
    textTransform: 'uppercase',
    letterSpacing: 0.5,
  },
  author: {
    fontSize: 14,
    color: '#333',
    fontFamily: 'monospace',
  },
  tallyContainer: {
    flexDirection: 'row',
    justifyContent: 'space-around',
    marginBottom: 16,
  },
  tallyItem: {
    alignItems: 'center',
  },
  tallyCount: {
    fontSize: 32,
    fontWeight: 'bold',
    color: '#333',
  },
  tallyLabel: {
    fontSize: 14,
    color: '#666',
  },
  progressBar: {
    height: 8,
    borderRadius: 4,
    backgroundColor: '#e0e0e0',
    flexDirection: 'row',
    overflow: 'hidden',
  },
  progressFor: {
    backgroundColor: '#4caf50',
  },
  progressAgainst: {
    backgroundColor: '#f44336',
  },
  progressAbstain: {
    backgroundColor: '#9e9e9e',
  },
  voteSection: {
    backgroundColor: '#fff',
    padding: 16,
    marginTop: 12,
    marginBottom: 32,
  },
  voteButtons: {
    flexDirection: 'row',
    gap: 12,
  },
  voteButton: {
    flex: 1,
    padding: 16,
    borderRadius: 8,
    alignItems: 'center',
  },
  voteFor: {
    backgroundColor: '#4caf50',
  },
  voteAgainst: {
    backgroundColor: '#f44336',
  },
  voteAbstain: {
    backgroundColor: '#9e9e9e',
  },
  voteButtonText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
  },
  votingIndicator: {
    marginTop: 16,
  },
  outcomeBadge: {
    alignSelf: 'flex-start',
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 4,
  },
  outcomeText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
    textTransform: 'uppercase',
  },
});
