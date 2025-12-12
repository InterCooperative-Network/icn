/**
 * Home Screen
 *
 * Main dashboard showing balance and quick actions.
 */

import React from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  ScrollView,
  RefreshControl,
  Platform,
} from 'react-native';
import { NativeStackNavigationProp } from '@react-navigation/native-stack';
import { useAuth, useBalance, useRealtime, useTransactions, useMemberProfile, useEvent, useNetworkState, useQueue } from '@icn/react-native';
import { client } from '../client';
import { RootStackParamList } from '../../App';

type Props = {
  navigation: NativeStackNavigationProp<RootStackParamList, 'Home'>;
};

export function HomeScreen({ navigation }: Props) {
  const { did, coopId, logout } = useAuth(client);
  const { balance, isLoading: balanceLoading, refresh: refreshBalance } = useBalance(client, coopId || '', did || '');
  const { transactions, isLoading: txLoading, refresh: refreshTx } = useTransactions(client, coopId || '', 5);
  const { isConnected } = useRealtime(client);
  const { profile, isLoading: profileLoading, refresh: refreshProfile } = useMemberProfile(client, coopId || '', did || '');
  const { networkState, isOnline, isOffline } = useNetworkState(client);
  const { pendingCount, failedCount, clearFailed } = useQueue(client);

  const isLoading = balanceLoading || txLoading;
  const refresh = () => {
    refreshBalance();
    refreshTx();
    refreshProfile();
  };

  // Auto-refresh on payment events
  useEvent(client, 'PaymentCreated', () => {
    refreshBalance();
    refreshTx();
    refreshProfile();
  });

  const formatDid = (did: string) => {
    if (did.length > 20) {
      return `${did.slice(0, 12)}...${did.slice(-6)}`;
    }
    return did;
  };

  return (
    <ScrollView
      style={styles.container}
      refreshControl={
        <RefreshControl refreshing={isLoading} onRefresh={refresh} />
      }
    >
      {/* Connection Status */}
      <View style={styles.statusBar}>
        <View style={styles.statusLeft}>
          <View style={[styles.statusDot, isConnected && styles.statusConnected]} />
          <Text style={styles.statusText}>
            {isConnected ? 'Connected' : 'Offline'}
          </Text>
        </View>
        <View style={styles.statusRight}>
          {isOffline && (
            <View style={styles.offlineBadge}>
              <Text style={styles.offlineText}>⚠️ Offline Mode</Text>
            </View>
          )}
          {pendingCount > 0 && (
            <View style={styles.queueBadge}>
              <Text style={styles.queueText}>{pendingCount} pending</Text>
            </View>
          )}
          {failedCount > 0 && (
            <TouchableOpacity style={styles.failedBadge} onPress={clearFailed}>
              <Text style={styles.failedText}>{failedCount} failed - tap to clear</Text>
            </TouchableOpacity>
          )}
        </View>
      </View>

      {/* Balance Card */}
      <View style={styles.balanceCard}>
        <Text style={styles.balanceLabel}>Available Balance</Text>
        <Text style={styles.balanceAmount}>
          {balance?.balance ?? 0}
          <Text style={styles.balanceCurrency}> hours</Text>
        </Text>
        <Text style={styles.coopName}>{coopId}</Text>
      </View>

      {/* Quick Actions */}
      <View style={styles.actions}>
        <TouchableOpacity
          style={styles.actionButton}
          onPress={() => navigation.navigate('Payment', {})}
        >
          <Text style={styles.actionIcon}>💸</Text>
          <Text style={styles.actionText}>Send</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.actionButton}
          onPress={() => navigation.navigate('Receive')}
        >
          <Text style={styles.actionIcon}>📥</Text>
          <Text style={styles.actionText}>Receive</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.actionButton}
          onPress={() => navigation.navigate('Scan')}
        >
          <Text style={styles.actionIcon}>📷</Text>
          <Text style={styles.actionText}>Scan</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.actionButton}
          onPress={() => navigation.navigate('Governance')}
        >
          <Text style={styles.actionIcon}>🗳️</Text>
          <Text style={styles.actionText}>Vote</Text>
        </TouchableOpacity>
      </View>

      {/* Recent Transactions */}
      <View style={styles.section}>
        <View style={styles.sectionHeader}>
          <Text style={styles.sectionTitle}>Recent Transactions</Text>
          <TouchableOpacity onPress={() => navigation.navigate('History')}>
            <Text style={styles.seeAllText}>See All</Text>
          </TouchableOpacity>
        </View>
        {transactions.length === 0 ? (
          <View style={styles.emptyState}>
            <Text style={styles.emptyStateText}>No transactions yet</Text>
          </View>
        ) : (
          transactions.map((tx) => {
            const isSent = tx.from === did;
            return (
              <View key={tx.id} style={styles.txRow}>
                <View style={styles.txIcon}>
                  <Text style={styles.txIconText}>{isSent ? '↑' : '↓'}</Text>
                </View>
                <View style={styles.txDetails}>
                  <Text style={styles.txTitle}>
                    {isSent ? `To: ${formatDid(tx.to)}` : `From: ${formatDid(tx.from)}`}
                  </Text>
                  {tx.memo && <Text style={styles.txMemo}>{tx.memo}</Text>}
                </View>
                <Text style={[styles.txAmount, isSent ? styles.txSent : styles.txReceived]}>
                  {isSent ? '-' : '+'}{tx.amount}
                </Text>
              </View>
            );
          })
        )}
      </View>

      {/* SDIS Actions */}
      <View style={styles.section}>
        <Text style={styles.sectionTitle}>SDIS Identity</Text>
        <View style={styles.sdisActions}>
          <TouchableOpacity
            style={styles.sdisButton}
            onPress={() => navigation.navigate('Identity')}
          >
            <Text style={styles.sdisIcon}>ID</Text>
            <View style={styles.sdisButtonContent}>
              <Text style={styles.sdisButtonTitle}>Show Proof</Text>
              <Text style={styles.sdisButtonDesc}>Generate QR code to prove your identity</Text>
            </View>
          </TouchableOpacity>

          <TouchableOpacity
            style={styles.sdisButton}
            onPress={() => navigation.navigate('Verify')}
          >
            <Text style={styles.sdisIcon}>✓</Text>
            <View style={styles.sdisButtonContent}>
              <Text style={styles.sdisButtonTitle}>Verify Others</Text>
              <Text style={styles.sdisButtonDesc}>Scan QR codes to verify identity</Text>
            </View>
          </TouchableOpacity>
        </View>
      </View>

      {/* Identity Section */}
      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Your Profile</Text>
        <View style={styles.identityCard}>
          <View style={styles.profileRow}>
            <Text style={styles.identityLabel}>DID</Text>
            <Text style={styles.identityValue}>{formatDid(did || '')}</Text>
          </View>
          {profile && (
            <>
              <View style={styles.profileRow}>
                <Text style={styles.identityLabel}>Role</Text>
                <Text style={styles.profileValue}>{profile.role}</Text>
              </View>
              <View style={styles.profileRow}>
                <Text style={styles.identityLabel}>Transactions</Text>
                <Text style={styles.profileValue}>{profile.transaction_count}</Text>
              </View>
              {profile.trust_score !== undefined && (
                <View style={styles.profileRow}>
                  <Text style={styles.identityLabel}>Trust Score</Text>
                  <Text style={styles.profileValue}>{profile.trust_score.toFixed(2)}</Text>
                </View>
              )}
            </>
          )}
        </View>
      </View>

      {/* Logout */}
      <TouchableOpacity style={styles.logoutButton} onPress={logout}>
        <Text style={styles.logoutText}>Logout</Text>
      </TouchableOpacity>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  statusBar: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: 8,
    backgroundColor: '#fff',
  },
  statusLeft: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  statusRight: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  statusDot: {
    width: 8,
    height: 8,
    borderRadius: 4,
    backgroundColor: '#ccc',
    marginRight: 6,
  },
  statusConnected: {
    backgroundColor: '#4caf50',
  },
  statusText: {
    fontSize: 12,
    color: '#666',
  },
  offlineBadge: {
    backgroundColor: '#ff9800',
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 12,
  },
  offlineText: {
    fontSize: 10,
    color: '#fff',
    fontWeight: '600',
  },
  queueBadge: {
    backgroundColor: '#2196f3',
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 12,
  },
  queueText: {
    fontSize: 10,
    color: '#fff',
    fontWeight: '600',
  },
  failedBadge: {
    backgroundColor: '#f44336',
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 12,
  },
  failedText: {
    fontSize: 10,
    color: '#fff',
    fontWeight: '600',
  },
  balanceCard: {
    backgroundColor: '#4A90A4',
    margin: 16,
    borderRadius: 16,
    padding: 24,
    alignItems: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.1,
    shadowRadius: 8,
    elevation: 4,
  },
  balanceLabel: {
    color: 'rgba(255,255,255,0.8)',
    fontSize: 14,
    marginBottom: 8,
  },
  balanceAmount: {
    color: '#fff',
    fontSize: 48,
    fontWeight: 'bold',
  },
  balanceCurrency: {
    fontSize: 24,
    fontWeight: 'normal',
  },
  coopName: {
    color: 'rgba(255,255,255,0.8)',
    fontSize: 16,
    marginTop: 8,
  },
  actions: {
    flexDirection: 'row',
    justifyContent: 'space-around',
    paddingHorizontal: 16,
    marginBottom: 24,
  },
  actionButton: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 16,
    alignItems: 'center',
    width: 80,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.05,
    shadowRadius: 4,
    elevation: 2,
  },
  actionIcon: {
    fontSize: 28,
    marginBottom: 4,
  },
  actionText: {
    fontSize: 12,
    color: '#333',
    fontWeight: '500',
  },
  section: {
    paddingHorizontal: 16,
    marginBottom: 24,
  },
  sectionTitle: {
    fontSize: 18,
    fontWeight: '600',
    color: '#333',
    marginBottom: 12,
  },
  identityCard: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 16,
  },
  profileRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 12,
  },
  identityLabel: {
    fontSize: 12,
    color: '#666',
  },
  identityValue: {
    fontSize: 14,
    color: '#333',
    fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace',
  },
  profileValue: {
    fontSize: 14,
    color: '#333',
    fontWeight: '500',
  },
  logoutButton: {
    marginHorizontal: 16,
    marginBottom: 32,
    padding: 16,
    alignItems: 'center',
  },
  logoutText: {
    color: '#e53935',
    fontSize: 16,
    fontWeight: '500',
  },
  sectionHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 12,
  },
  seeAllText: {
    color: '#4A90A4',
    fontSize: 14,
    fontWeight: '500',
  },
  emptyState: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 24,
    alignItems: 'center',
  },
  emptyStateText: {
    color: '#999',
    fontSize: 14,
  },
  txRow: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 16,
    flexDirection: 'row',
    alignItems: 'center',
    marginBottom: 8,
  },
  txIcon: {
    width: 36,
    height: 36,
    borderRadius: 18,
    backgroundColor: '#f0f0f0',
    justifyContent: 'center',
    alignItems: 'center',
    marginRight: 12,
  },
  txIconText: {
    fontSize: 18,
    fontWeight: 'bold',
  },
  txDetails: {
    flex: 1,
  },
  txTitle: {
    fontSize: 14,
    fontWeight: '500',
    color: '#333',
  },
  txMemo: {
    fontSize: 12,
    color: '#666',
    marginTop: 2,
  },
  txAmount: {
    fontSize: 16,
    fontWeight: '600',
  },
  txSent: {
    color: '#e53935',
  },
  txReceived: {
    color: '#4caf50',
  },
  sdisActions: {
    gap: 12,
  },
  sdisButton: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 16,
    flexDirection: 'row',
    alignItems: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.05,
    shadowRadius: 4,
    elevation: 2,
  },
  sdisIcon: {
    fontSize: 24,
    fontWeight: 'bold',
    color: '#4A90A4',
    width: 40,
    height: 40,
    lineHeight: 40,
    textAlign: 'center',
    backgroundColor: '#E3F2FD',
    borderRadius: 20,
    marginRight: 12,
  },
  sdisButtonContent: {
    flex: 1,
  },
  sdisButtonTitle: {
    fontSize: 16,
    fontWeight: '600',
    color: '#333',
    marginBottom: 2,
  },
  sdisButtonDesc: {
    fontSize: 12,
    color: '#666',
  },
});
