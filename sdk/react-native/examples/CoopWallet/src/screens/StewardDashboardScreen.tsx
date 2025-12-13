/**
 * Steward Dashboard Screen
 *
 * Main dashboard for stewards to review and approve SDIS enrollments.
 */

import React, { useState } from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  FlatList,
  RefreshControl,
  ActivityIndicator,
} from 'react-native';
import { NativeStackNavigationProp } from '@react-navigation/native-stack';
import { usePendingEnrollments, useStewardStats, Enrollment } from '@icn/react-native';
import { client } from '../client';
import { RootStackParamList } from '../../App';

type Props = {
  navigation: NativeStackNavigationProp<RootStackParamList, 'StewardDashboard'>;
};

export function StewardDashboardScreen({ navigation }: Props) {
  const [activeTab, setActiveTab] = useState<'pending' | 'stats'>('pending');

  const {
    enrollments,
    pendingCount,
    isLoading: enrollmentsLoading,
    error: enrollmentsError,
    refresh: refreshEnrollments,
  } = usePendingEnrollments(client!, { autoRefresh: true, refreshInterval: 30000 });

  const {
    stats,
    isLoading: statsLoading,
    error: statsError,
    refresh: refreshStats,
  } = useStewardStats(client!);

  const handleRefresh = () => {
    refreshEnrollments();
    refreshStats();
  };

  const renderEnrollmentCard = ({ item }: { item: Enrollment }) => (
    <TouchableOpacity
      style={styles.enrollmentCard}
      onPress={() => navigation.navigate('EnrollmentDetail', { enrollmentId: item.enrollment_id })}
    >
      <View style={styles.cardHeader}>
        <Text style={styles.identityName}>{item.identity_name}</Text>
        <View style={[styles.levelBadge, getLevelStyle(item.level)]}>
          <Text style={styles.levelText}>Level {item.level}</Text>
        </View>
      </View>
      <View style={styles.cardMeta}>
        <Text style={styles.metaText}>Coop: {item.coop_id}</Text>
        <Text style={styles.metaText}>
          Expires: {new Date(item.expires_at).toLocaleDateString()}
        </Text>
      </View>
      <View style={styles.statusRow}>
        <Text style={styles.statusLabel}>Status: </Text>
        <Text style={styles.statusValue}>{formatStatus(item.status)}</Text>
      </View>
    </TouchableOpacity>
  );

  const renderStatsTab = () => (
    <View style={styles.statsContainer}>
      {statsLoading ? (
        <ActivityIndicator size="large" color="#4A90A4" />
      ) : statsError ? (
        <View style={styles.errorContainer}>
          <Text style={styles.errorText}>{statsError}</Text>
          <TouchableOpacity style={styles.retryButton} onPress={refreshStats}>
            <Text style={styles.retryText}>Retry</Text>
          </TouchableOpacity>
        </View>
      ) : stats ? (
        <>
          <View style={styles.statsGrid}>
            <View style={styles.statCard}>
              <Text style={styles.statValue}>{stats.total_vouches}</Text>
              <Text style={styles.statLabel}>Total Vouches</Text>
            </View>
            <View style={styles.statCard}>
              <Text style={styles.statValue}>{stats.monthly_vouches}</Text>
              <Text style={styles.statLabel}>This Month</Text>
            </View>
            <View style={styles.statCard}>
              <Text style={styles.statValue}>{stats.total_rejections}</Text>
              <Text style={styles.statLabel}>Rejections</Text>
            </View>
            <View style={styles.statCard}>
              <Text style={styles.statValue}>{Math.round(stats.reputation_score * 100)}%</Text>
              <Text style={styles.statLabel}>Reputation</Text>
            </View>
          </View>
          <View style={styles.responseTimeCard}>
            <Text style={styles.responseTimeLabel}>Average Response Time</Text>
            <Text style={styles.responseTimeValue}>
              {stats.avg_response_hours < 24
                ? `${Math.round(stats.avg_response_hours)} hours`
                : `${Math.round(stats.avg_response_hours / 24)} days`}
            </Text>
          </View>
          <TouchableOpacity
            style={styles.historyButton}
            onPress={() => navigation.navigate('VouchHistory')}
          >
            <Text style={styles.historyButtonText}>View Vouch History</Text>
          </TouchableOpacity>
        </>
      ) : (
        <Text style={styles.emptyText}>No statistics available</Text>
      )}
    </View>
  );

  return (
    <View style={styles.container}>
      {/* Tab Bar */}
      <View style={styles.tabBar}>
        <TouchableOpacity
          style={[styles.tab, activeTab === 'pending' && styles.activeTab]}
          onPress={() => setActiveTab('pending')}
        >
          <Text style={[styles.tabText, activeTab === 'pending' && styles.activeTabText]}>
            Pending
          </Text>
          {pendingCount > 0 && (
            <View style={styles.badge}>
              <Text style={styles.badgeText}>{pendingCount}</Text>
            </View>
          )}
        </TouchableOpacity>
        <TouchableOpacity
          style={[styles.tab, activeTab === 'stats' && styles.activeTab]}
          onPress={() => setActiveTab('stats')}
        >
          <Text style={[styles.tabText, activeTab === 'stats' && styles.activeTabText]}>
            Statistics
          </Text>
        </TouchableOpacity>
      </View>

      {/* Content */}
      {activeTab === 'pending' ? (
        <FlatList
          data={enrollments}
          renderItem={renderEnrollmentCard}
          keyExtractor={(item) => item.enrollment_id}
          refreshControl={
            <RefreshControl refreshing={enrollmentsLoading} onRefresh={handleRefresh} />
          }
          contentContainerStyle={styles.listContent}
          ListEmptyComponent={
            enrollmentsLoading ? (
              <View style={styles.loadingContainer}>
                <ActivityIndicator size="large" color="#4A90A4" />
                <Text style={styles.loadingText}>Loading enrollments...</Text>
              </View>
            ) : enrollmentsError ? (
              <View style={styles.errorContainer}>
                <Text style={styles.errorText}>{enrollmentsError}</Text>
                <TouchableOpacity style={styles.retryButton} onPress={refreshEnrollments}>
                  <Text style={styles.retryText}>Retry</Text>
                </TouchableOpacity>
              </View>
            ) : (
              <View style={styles.emptyContainer}>
                <Text style={styles.emptyIcon}>✓</Text>
                <Text style={styles.emptyTitle}>All Caught Up!</Text>
                <Text style={styles.emptyText}>No pending enrollments to review.</Text>
              </View>
            )
          }
        />
      ) : (
        renderStatsTab()
      )}
    </View>
  );
}

function formatStatus(status: string): string {
  const statusMap: Record<string, string> = {
    pending_device_verification: 'Awaiting Device',
    pending_steward_vouch: 'Awaiting Vouch',
    ready_for_completion: 'Ready',
    rejected: 'Rejected',
  };
  return statusMap[status] || status;
}

function getLevelStyle(level: number) {
  switch (level) {
    case 0:
      return { backgroundColor: '#ffcdd2' };
    case 1:
      return { backgroundColor: '#fff9c4' };
    case 2:
      return { backgroundColor: '#c8e6c9' };
    default:
      return { backgroundColor: '#e0e0e0' };
  }
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  tabBar: {
    flexDirection: 'row',
    backgroundColor: '#fff',
    borderBottomWidth: 1,
    borderBottomColor: '#e0e0e0',
  },
  tab: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 16,
    gap: 8,
  },
  activeTab: {
    borderBottomWidth: 2,
    borderBottomColor: '#4A90A4',
  },
  tabText: {
    fontSize: 16,
    color: '#666',
  },
  activeTabText: {
    color: '#4A90A4',
    fontWeight: '600',
  },
  badge: {
    backgroundColor: '#e53935',
    borderRadius: 10,
    minWidth: 20,
    height: 20,
    alignItems: 'center',
    justifyContent: 'center',
    paddingHorizontal: 6,
  },
  badgeText: {
    color: '#fff',
    fontSize: 12,
    fontWeight: 'bold',
  },
  listContent: {
    padding: 16,
  },
  enrollmentCard: {
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
  cardHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 8,
  },
  identityName: {
    fontSize: 18,
    fontWeight: '600',
    color: '#333',
    flex: 1,
  },
  levelBadge: {
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 12,
  },
  level0: {
    backgroundColor: '#ffcdd2',
  },
  level1: {
    backgroundColor: '#fff9c4',
  },
  level2: {
    backgroundColor: '#c8e6c9',
  },
  levelText: {
    fontSize: 12,
    fontWeight: '600',
    color: '#333',
  },
  cardMeta: {
    flexDirection: 'row',
    gap: 16,
    marginBottom: 8,
  },
  metaText: {
    fontSize: 13,
    color: '#666',
  },
  statusRow: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  statusLabel: {
    fontSize: 13,
    color: '#666',
  },
  statusValue: {
    fontSize: 13,
    color: '#4A90A4',
    fontWeight: '500',
  },
  loadingContainer: {
    padding: 40,
    alignItems: 'center',
  },
  loadingText: {
    marginTop: 12,
    fontSize: 14,
    color: '#666',
  },
  errorContainer: {
    padding: 40,
    alignItems: 'center',
  },
  errorText: {
    fontSize: 14,
    color: '#e53935',
    textAlign: 'center',
    marginBottom: 16,
  },
  retryButton: {
    backgroundColor: '#4A90A4',
    paddingHorizontal: 24,
    paddingVertical: 12,
    borderRadius: 8,
  },
  retryText: {
    color: '#fff',
    fontSize: 14,
    fontWeight: '600',
  },
  emptyContainer: {
    padding: 40,
    alignItems: 'center',
  },
  emptyIcon: {
    fontSize: 48,
    color: '#4caf50',
    marginBottom: 16,
  },
  emptyTitle: {
    fontSize: 20,
    fontWeight: '600',
    color: '#333',
    marginBottom: 8,
  },
  emptyText: {
    fontSize: 14,
    color: '#666',
    textAlign: 'center',
  },
  statsContainer: {
    flex: 1,
    padding: 16,
  },
  statsGrid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 12,
    marginBottom: 16,
  },
  statCard: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 20,
    flex: 1,
    minWidth: '45%',
    alignItems: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.05,
    shadowRadius: 4,
    elevation: 2,
  },
  statValue: {
    fontSize: 32,
    fontWeight: 'bold',
    color: '#4A90A4',
  },
  statLabel: {
    fontSize: 13,
    color: '#666',
    marginTop: 4,
  },
  responseTimeCard: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 20,
    alignItems: 'center',
    marginBottom: 16,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.05,
    shadowRadius: 4,
    elevation: 2,
  },
  responseTimeLabel: {
    fontSize: 13,
    color: '#666',
    marginBottom: 8,
  },
  responseTimeValue: {
    fontSize: 24,
    fontWeight: '600',
    color: '#333',
  },
  historyButton: {
    backgroundColor: '#4A90A4',
    borderRadius: 12,
    padding: 16,
    alignItems: 'center',
  },
  historyButtonText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
  },
});
