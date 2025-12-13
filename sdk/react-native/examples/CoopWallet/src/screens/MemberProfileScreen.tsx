/**
 * Member Profile Screen
 *
 * View another member's profile, trust score, and transaction history.
 */

import React, { useState, useEffect } from 'react';
import {
  View,
  Text,
  StyleSheet,
  ScrollView,
  TouchableOpacity,
  ActivityIndicator,
  Alert,
  Clipboard,
} from 'react-native';
import { ICNMobileClient } from '@icn/react-native';
import { useNavigation } from '@react-navigation/native';
import { Identicon } from '../components/Identicon';

interface MemberProfileScreenProps {
  client: ICNMobileClient;
  coopId: string;
  memberDid: string;
  userDid: string;
}

interface MemberProfile {
  did: string;
  balance: number;
  transaction_count: number;
  role?: string;
  joined_at?: number;
}

interface TrustScore {
  did: string;
  trust_score: number;
  trust_class: 'Isolated' | 'Known' | 'Partner' | 'Federated';
}

export function MemberProfileScreen({ client, coopId, memberDid, userDid }: MemberProfileScreenProps) {
  const navigation = useNavigation();
  const [profile, setProfile] = useState<MemberProfile | null>(null);
  const [trustScore, setTrustScore] = useState<TrustScore | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const isOwnProfile = memberDid === userDid;

  useEffect(() => {
    loadProfile();
  }, [memberDid, coopId]);

  const loadProfile = async () => {
    setIsLoading(true);
    setError(null);

    try {
      const [profileData, trustData] = await Promise.all([
        client.getMemberProfile(coopId, memberDid),
        client.getTrustScore(memberDid).catch(() => null), // Trust score may fail if not authenticated
      ]);

      setProfile(profileData);
      setTrustScore(trustData);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  };

  const copyDid = () => {
    Clipboard.setString(memberDid);
    Alert.alert('Copied', 'DID copied to clipboard');
  };

  const handleSendPayment = () => {
    // Navigate to payment screen with this member pre-filled
    (navigation as any).navigate('Payment', { recipientDid: memberDid });
  };

  const handleAttest = () => {
    // Navigate to trust attestation screen
    (navigation as any).navigate('TrustAttestation', { targetDid: memberDid });
  };

  const getTrustClassColor = (trustClass: string) => {
    switch (trustClass) {
      case 'Federated':
        return '#4CAF50';
      case 'Partner':
        return '#2196F3';
      case 'Known':
        return '#FF9800';
      case 'Isolated':
        return '#9E9E9E';
      default:
        return '#9E9E9E';
    }
  };

  const formatDate = (timestamp?: number) => {
    if (!timestamp) return 'Unknown';
    return new Date(timestamp).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'long',
      day: 'numeric',
    });
  };

  const formatDid = (did: string) => {
    return `${did.slice(0, 16)}...${did.slice(-8)}`;
  };

  if (isLoading) {
    return (
      <View style={styles.loadingContainer}>
        <ActivityIndicator size="large" color="#4A90A4" />
        <Text style={styles.loadingText}>Loading profile...</Text>
      </View>
    );
  }

  if (error) {
    return (
      <View style={styles.errorContainer}>
        <Text style={styles.errorTitle}>Failed to load profile</Text>
        <Text style={styles.errorText}>{error}</Text>
        <TouchableOpacity style={styles.retryButton} onPress={loadProfile}>
          <Text style={styles.retryText}>Retry</Text>
        </TouchableOpacity>
      </View>
    );
  }

  return (
    <ScrollView style={styles.container} contentContainerStyle={styles.content}>
      {/* Header Card */}
      <View style={styles.headerCard}>
        <View style={styles.avatarContainer}>
          <Identicon did={memberDid} size={80} />
          {isOwnProfile && (
            <View style={styles.selfBadge}>
              <Text style={styles.selfBadgeText}>You</Text>
            </View>
          )}
        </View>

        <TouchableOpacity onPress={copyDid} style={styles.didContainer}>
          <Text style={styles.didText}>{formatDid(memberDid)}</Text>
          <Text style={styles.copyHint}>Tap to copy</Text>
        </TouchableOpacity>

        {profile?.role && (
          <View style={styles.roleBadge}>
            <Text style={styles.roleText}>{profile.role}</Text>
          </View>
        )}
      </View>

      {/* Trust Score Card */}
      {trustScore && (
        <View style={styles.card}>
          <Text style={styles.cardTitle}>Trust Score</Text>
          <View style={styles.trustContainer}>
            <View style={styles.trustScoreCircle}>
              <Text style={styles.trustScoreValue}>
                {(trustScore.trust_score * 100).toFixed(0)}
              </Text>
              <Text style={styles.trustScoreLabel}>%</Text>
            </View>
            <View style={styles.trustInfo}>
              <View
                style={[
                  styles.trustClassBadge,
                  { backgroundColor: getTrustClassColor(trustScore.trust_class) },
                ]}
              >
                <Text style={styles.trustClassText}>{trustScore.trust_class}</Text>
              </View>
              <Text style={styles.trustDescription}>
                {trustScore.trust_class === 'Federated' && 'Highly trusted member'}
                {trustScore.trust_class === 'Partner' && 'Trusted partner'}
                {trustScore.trust_class === 'Known' && 'Known member'}
                {trustScore.trust_class === 'Isolated' && 'New or unverified'}
              </Text>
            </View>
          </View>
        </View>
      )}

      {/* Activity Card */}
      <View style={styles.card}>
        <Text style={styles.cardTitle}>Activity</Text>
        <View style={styles.statsGrid}>
          <View style={styles.statItem}>
            <Text style={styles.statValue}>{profile?.balance ?? 0}</Text>
            <Text style={styles.statLabel}>Balance</Text>
          </View>
          <View style={styles.statDivider} />
          <View style={styles.statItem}>
            <Text style={styles.statValue}>{profile?.transaction_count ?? 0}</Text>
            <Text style={styles.statLabel}>Transactions</Text>
          </View>
        </View>
        {profile?.joined_at && (
          <View style={styles.joinedInfo}>
            <Text style={styles.joinedLabel}>Member since</Text>
            <Text style={styles.joinedDate}>{formatDate(profile.joined_at)}</Text>
          </View>
        )}
      </View>

      {/* Actions */}
      {!isOwnProfile && (
        <View style={styles.actionsContainer}>
          <TouchableOpacity style={styles.primaryButton} onPress={handleSendPayment}>
            <Text style={styles.primaryButtonText}>Send Payment</Text>
          </TouchableOpacity>

          <TouchableOpacity style={styles.secondaryButton} onPress={handleAttest}>
            <Text style={styles.secondaryButtonText}>Attest Trust</Text>
          </TouchableOpacity>
        </View>
      )}

      {/* DID Details */}
      <View style={styles.card}>
        <Text style={styles.cardTitle}>Full DID</Text>
        <TouchableOpacity onPress={copyDid}>
          <Text style={styles.fullDid}>{memberDid}</Text>
        </TouchableOpacity>
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  content: {
    padding: 16,
    paddingBottom: 32,
  },
  loadingContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  loadingText: {
    marginTop: 12,
    fontSize: 16,
    color: '#666',
  },
  errorContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    padding: 32,
  },
  errorTitle: {
    fontSize: 18,
    fontWeight: '600',
    color: '#E53935',
    marginBottom: 8,
  },
  errorText: {
    fontSize: 14,
    color: '#999',
    textAlign: 'center',
    marginBottom: 16,
  },
  retryButton: {
    paddingHorizontal: 24,
    paddingVertical: 12,
    backgroundColor: '#4A90A4',
    borderRadius: 8,
  },
  retryText: {
    color: '#fff',
    fontWeight: '600',
  },
  headerCard: {
    backgroundColor: '#4A90A4',
    borderRadius: 16,
    padding: 24,
    alignItems: 'center',
    marginBottom: 16,
  },
  avatarContainer: {
    position: 'relative',
    marginBottom: 16,
  },
  avatar: {
    width: 80,
    height: 80,
    borderRadius: 40,
    backgroundColor: 'rgba(255,255,255,0.2)',
    justifyContent: 'center',
    alignItems: 'center',
  },
  avatarText: {
    fontSize: 28,
    fontWeight: '700',
    color: '#fff',
  },
  selfBadge: {
    position: 'absolute',
    bottom: -4,
    right: -4,
    backgroundColor: '#FFD700',
    paddingHorizontal: 8,
    paddingVertical: 2,
    borderRadius: 10,
  },
  selfBadgeText: {
    fontSize: 10,
    fontWeight: '700',
    color: '#333',
  },
  didContainer: {
    alignItems: 'center',
  },
  didText: {
    fontSize: 16,
    fontWeight: '600',
    color: '#fff',
    fontFamily: 'monospace',
  },
  copyHint: {
    fontSize: 12,
    color: 'rgba(255,255,255,0.7)',
    marginTop: 4,
  },
  roleBadge: {
    marginTop: 12,
    paddingHorizontal: 12,
    paddingVertical: 4,
    backgroundColor: 'rgba(255,255,255,0.2)',
    borderRadius: 12,
  },
  roleText: {
    fontSize: 12,
    fontWeight: '600',
    color: '#fff',
    textTransform: 'uppercase',
  },
  card: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 16,
    marginBottom: 16,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.1,
    shadowRadius: 2,
    elevation: 2,
  },
  cardTitle: {
    fontSize: 12,
    fontWeight: '600',
    color: '#999',
    textTransform: 'uppercase',
    letterSpacing: 1,
    marginBottom: 12,
  },
  trustContainer: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  trustScoreCircle: {
    width: 80,
    height: 80,
    borderRadius: 40,
    backgroundColor: '#f0f0f0',
    justifyContent: 'center',
    alignItems: 'center',
    flexDirection: 'row',
  },
  trustScoreValue: {
    fontSize: 28,
    fontWeight: '700',
    color: '#333',
  },
  trustScoreLabel: {
    fontSize: 14,
    color: '#666',
    marginLeft: 2,
  },
  trustInfo: {
    flex: 1,
    marginLeft: 16,
  },
  trustClassBadge: {
    alignSelf: 'flex-start',
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 12,
    marginBottom: 4,
  },
  trustClassText: {
    fontSize: 12,
    fontWeight: '600',
    color: '#fff',
  },
  trustDescription: {
    fontSize: 14,
    color: '#666',
  },
  statsGrid: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  statItem: {
    flex: 1,
    alignItems: 'center',
  },
  statValue: {
    fontSize: 28,
    fontWeight: '700',
    color: '#333',
  },
  statLabel: {
    fontSize: 12,
    color: '#999',
    marginTop: 4,
  },
  statDivider: {
    width: 1,
    height: 40,
    backgroundColor: '#e0e0e0',
  },
  joinedInfo: {
    marginTop: 16,
    paddingTop: 16,
    borderTopWidth: 1,
    borderTopColor: '#f0f0f0',
    flexDirection: 'row',
    justifyContent: 'space-between',
  },
  joinedLabel: {
    fontSize: 14,
    color: '#999',
  },
  joinedDate: {
    fontSize: 14,
    color: '#333',
    fontWeight: '500',
  },
  actionsContainer: {
    marginBottom: 16,
    gap: 12,
  },
  primaryButton: {
    backgroundColor: '#4A90A4',
    padding: 16,
    borderRadius: 12,
    alignItems: 'center',
  },
  primaryButtonText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
  },
  secondaryButton: {
    backgroundColor: '#fff',
    padding: 16,
    borderRadius: 12,
    alignItems: 'center',
    borderWidth: 1,
    borderColor: '#4A90A4',
  },
  secondaryButtonText: {
    color: '#4A90A4',
    fontSize: 16,
    fontWeight: '600',
  },
  fullDid: {
    fontSize: 12,
    color: '#666',
    fontFamily: 'monospace',
    lineHeight: 18,
  },
});

export default MemberProfileScreen;
