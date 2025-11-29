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
} from 'react-native';
import { NativeStackNavigationProp } from '@react-navigation/native-stack';
import { useAuth, useBalance, useRealtime } from '@icn/react-native';
import { client } from '../client';
import { RootStackParamList } from '../../App';

type Props = {
  navigation: NativeStackNavigationProp<RootStackParamList, 'Home'>;
};

export function HomeScreen({ navigation }: Props) {
  const { did, coopId, logout } = useAuth(client);
  const { balance, isLoading, refresh } = useBalance(client, coopId || '', did || '');
  const { isConnected } = useRealtime(client);

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
        <View style={[styles.statusDot, isConnected && styles.statusConnected]} />
        <Text style={styles.statusText}>
          {isConnected ? 'Connected' : 'Offline'}
        </Text>
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

      {/* Identity Section */}
      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Your Identity</Text>
        <View style={styles.identityCard}>
          <Text style={styles.identityLabel}>DID</Text>
          <Text style={styles.identityValue}>{formatDid(did || '')}</Text>
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
    justifyContent: 'center',
    padding: 8,
    backgroundColor: '#fff',
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
  identityLabel: {
    fontSize: 12,
    color: '#666',
    marginBottom: 4,
  },
  identityValue: {
    fontSize: 14,
    color: '#333',
    fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace',
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
});

// Add Platform import
import { Platform } from 'react-native';
