/**
 * Voting Screen Component
 * Interface for voting on amendments
 */
import React, { useState } from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  ScrollView,
  Alert,
  ActivityIndicator,
} from 'react-native';
import { useAmendmentVoting, useAmendmentResults } from '@icn/react-native';

type VoteChoice = 'approve' | 'reject' | 'abstain';

interface VotingScreenProps {
  amendmentId: string;
  amendmentTitle: string;
  amendmentDescription: string;
}

export function VotingScreen({
  amendmentId,
  amendmentTitle,
  amendmentDescription,
}: VotingScreenProps) {
  const { vote, loading: voting, myVote } = useAmendmentVoting(amendmentId);
  const { results, loading: loadingResults } = useAmendmentResults(amendmentId);

  const [selectedVote, setSelectedVote] = useState<VoteChoice | null>(null);
  const [comment, setComment] = useState('');

  const handleVote = async (choice: VoteChoice) => {
    setSelectedVote(choice);
    
    Alert.alert(
      'Confirm Vote',
      `Are you sure you want to ${choice} this amendment?`,
      [
        { text: 'Cancel', style: 'cancel', onPress: () => setSelectedVote(null) },
        {
          text: 'Confirm',
          onPress: async () => {
            try {
              await vote(choice, comment);
              Alert.alert('Success', 'Your vote has been recorded');
            } catch (error) {
              Alert.alert('Error', 'Failed to submit vote');
              setSelectedVote(null);
            }
          },
        },
      ]
    );
  };

  // Calculate percentages
  const totalVotes = results
    ? results.approve_count + results.reject_count + results.abstain_count
    : 0;
  const approvePercent = totalVotes > 0
    ? Math.round((results.approve_count / totalVotes) * 100)
    : 0;
  const rejectPercent = totalVotes > 0
    ? Math.round((results.reject_count / totalVotes) * 100)
    : 0;

  return (
    <ScrollView style={styles.container}>
      {/* Amendment Details */}
      <View style={styles.section}>
        <Text style={styles.title}>{amendmentTitle}</Text>
        <Text style={styles.description}>{amendmentDescription}</Text>
      </View>

      {/* Your Vote Status */}
      {myVote && (
        <View style={[styles.section, styles.voteStatus]}>
          <Text style={styles.sectionTitle}>Your Vote</Text>
          <Text style={styles.voteStatusText}>
            You voted to <Text style={styles.voteChoice}>{myVote.vote}</Text>
          </Text>
          {myVote.comment && (
            <Text style={styles.voteComment}>"{myVote.comment}"</Text>
          )}
        </View>
      )}

      {/* Voting Buttons */}
      {!myVote && (
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Cast Your Vote</Text>
          
          <TouchableOpacity
            style={[styles.voteButton, styles.approveButton]}
            onPress={() => handleVote('approve')}
            disabled={voting}
          >
            <Text style={styles.voteButtonText}>
              {voting && selectedVote === 'approve' ? 'Submitting...' : '✓ Approve'}
            </Text>
          </TouchableOpacity>

          <TouchableOpacity
            style={[styles.voteButton, styles.rejectButton]}
            onPress={() => handleVote('reject')}
            disabled={voting}
          >
            <Text style={styles.voteButtonText}>
              {voting && selectedVote === 'reject' ? 'Submitting...' : '✗ Reject'}
            </Text>
          </TouchableOpacity>

          <TouchableOpacity
            style={[styles.voteButton, styles.abstainButton]}
            onPress={() => handleVote('abstain')}
            disabled={voting}
          >
            <Text style={styles.voteButtonText}>
              {voting && selectedVote === 'abstain' ? 'Submitting...' : '— Abstain'}
            </Text>
          </TouchableOpacity>
        </View>
      )}

      {/* Results */}
      {loadingResults ? (
        <ActivityIndicator size="large" color="#007AFF" />
      ) : results && (
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Current Results</Text>

          {/* Quorum Status */}
          <View style={[styles.quorumBanner, results.has_quorum && styles.quorumMet]}>
            <Text style={styles.quorumText}>
              {results.has_quorum ? '✓ Quorum Met' : 'Quorum Not Met'}
            </Text>
            <Text style={styles.quorumSubtext}>
              {results.total_votes} of {results.eligible_voters} votes
            </Text>
          </View>

          {/* Vote Breakdown */}
          <View style={styles.resultsContainer}>
            {/* Approve */}
            <View style={styles.resultRow}>
              <Text style={styles.resultLabel}>Approve</Text>
              <View style={styles.resultBar}>
                <View
                  style={[
                    styles.resultBarFill,
                    styles.approveBar,
                    { width: `${approvePercent}%` },
                  ]}
                />
              </View>
              <Text style={styles.resultCount}>
                {results.approve_count} ({approvePercent}%)
              </Text>
            </View>

            {/* Reject */}
            <View style={styles.resultRow}>
              <Text style={styles.resultLabel}>Reject</Text>
              <View style={styles.resultBar}>
                <View
                  style={[
                    styles.resultBarFill,
                    styles.rejectBar,
                    { width: `${rejectPercent}%` },
                  ]}
                />
              </View>
              <Text style={styles.resultCount}>
                {results.reject_count} ({rejectPercent}%)
              </Text>
            </View>

            {/* Abstain */}
            <View style={styles.resultRow}>
              <Text style={styles.resultLabel}>Abstain</Text>
              <Text style={styles.resultCount}>{results.abstain_count}</Text>
            </View>
          </View>

          {/* Status */}
          <View style={styles.statusBadge}>
            <Text style={styles.statusText}>
              Status: {results.status}
            </Text>
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
  section: {
    backgroundColor: '#fff',
    padding: 16,
    marginVertical: 8,
    marginHorizontal: 16,
    borderRadius: 8,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.1,
    shadowRadius: 2,
    elevation: 2,
  },
  title: {
    fontSize: 20,
    fontWeight: 'bold',
    marginBottom: 8,
    color: '#333',
  },
  description: {
    fontSize: 14,
    color: '#666',
    lineHeight: 20,
  },
  sectionTitle: {
    fontSize: 16,
    fontWeight: '600',
    marginBottom: 12,
    color: '#333',
  },
  voteStatus: {
    backgroundColor: '#E8F5E9',
    borderLeftWidth: 4,
    borderLeftColor: '#4CAF50',
  },
  voteStatusText: {
    fontSize: 14,
    color: '#333',
  },
  voteChoice: {
    fontWeight: 'bold',
    textTransform: 'uppercase',
  },
  voteComment: {
    fontSize: 12,
    color: '#666',
    fontStyle: 'italic',
    marginTop: 8,
  },
  voteButton: {
    padding: 16,
    borderRadius: 8,
    marginBottom: 12,
    alignItems: 'center',
  },
  approveButton: {
    backgroundColor: '#4CAF50',
  },
  rejectButton: {
    backgroundColor: '#F44336',
  },
  abstainButton: {
    backgroundColor: '#9E9E9E',
  },
  voteButtonText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
  },
  quorumBanner: {
    padding: 12,
    borderRadius: 8,
    backgroundColor: '#FFF3E0',
    marginBottom: 16,
  },
  quorumMet: {
    backgroundColor: '#E8F5E9',
  },
  quorumText: {
    fontSize: 16,
    fontWeight: '600',
    color: '#333',
    marginBottom: 4,
  },
  quorumSubtext: {
    fontSize: 12,
    color: '#666',
  },
  resultsContainer: {
    marginTop: 8,
  },
  resultRow: {
    marginBottom: 16,
  },
  resultLabel: {
    fontSize: 14,
    fontWeight: '500',
    marginBottom: 4,
    color: '#333',
  },
  resultBar: {
    height: 24,
    backgroundColor: '#E0E0E0',
    borderRadius: 4,
    marginBottom: 4,
    overflow: 'hidden',
  },
  resultBarFill: {
    height: '100%',
  },
  approveBar: {
    backgroundColor: '#4CAF50',
  },
  rejectBar: {
    backgroundColor: '#F44336',
  },
  resultCount: {
    fontSize: 12,
    color: '#666',
  },
  statusBadge: {
    marginTop: 16,
    padding: 8,
    backgroundColor: '#E3F2FD',
    borderRadius: 4,
    alignItems: 'center',
  },
  statusText: {
    fontSize: 12,
    color: '#1976D2',
    fontWeight: '500',
  },
});
